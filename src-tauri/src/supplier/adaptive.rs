//! Adaptive parallelism for independent supplier reads: a [`Pool`] of reusable sessions whose
//! size a [`Limiter`] moves with what the connection and the site can bear.
//!
//! The limiter climbs on **measured throughput**. For every concurrency level `n` it keeps the
//! mean latency of replies that ran `n` at a time, so the throughput of that level is
//! `n / latency`. With the pool full it tries the next level up; a step up is kept only if it
//! delivered at least [`GAIN_SHARE`] of the ideal gain — `n + 1` sessions should answer
//! `(n + 1) / n` as fast as `n` — and is undone otherwise. That single test covers every reason
//! more parallelism stops paying: a thin or slow link (the transfers share the bandwidth, so each
//! one stretches as the count grows and the sum does not rise) and a site that slows under load
//! (the same, on its side). Retries — a dropped connection, a 5xx, the site's error page — are read
//! as a rate: once [`STRAIN_LIMIT`] of the last [`STRAIN_WINDOW`] replies needed one, the link is
//! unstable and the level halves at once; an isolated retry costs nothing. Levels go stale and are probed again,
//! so the limiter follows a link that recovers. It never leaves `1..=max`: a hopeless link degrades
//! to the old one-at-a-time fetching, never below it.
//!
//! An earlier shape — Vegas, latency against a slowly drifting *base* — failed its own
//! simulation: the drift absorbed the latency the extra sessions caused, and a link three requests
//! could saturate was held at the cap of eight (each request 80 s, nearing the client timeout).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;

use crate::errors::Result;

/// Replies a level needs before its mean is compared with its neighbours'. The invoice pages
/// spread widely (2026-10-07: 6 s to 63 s around a ~30 s middle), so fewer would let noise decide.
const MIN_SAMPLES: u32 = 12;
/// Replies after which an unvisited level's measurement is dropped and the level probed afresh.
const STALE_AFTER: u64 = 240;
/// Replies over which the retry rate is read.
const STRAIN_WINDOW: usize = 24;
/// Retries within [`STRAIN_WINDOW`] that mark the connection unstable — one in eight. The site's
/// own background rate (2026-10-07: 10 retries in 1 499 pages, 0.7 %) almost never reaches it, so
/// an isolated retry costs no parallelism; halving on every one held a site with room for eight
/// at 5.5 sessions and a third less throughput in the simulation below.
const STRAIN_LIMIT: usize = 3;
/// Share of the ideal throughput gain a step up must deliver to be kept. Chosen by the
/// simulations below: at 0.5 the measured site (whose knee is soft — every step to eight still
/// gains 7–15 %) settled at 4.5 sessions; at 0.3 it holds ~7.5, while a saturated link stays ~3.
const GAIN_SHARE: f64 = 0.3;

/// How one request went, as far as the limiter cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    /// Answered at the first attempt, after this long.
    Ok(Duration),
    /// Needed at least one retry, or never got an answer.
    Strained,
}

#[derive(Debug, Clone, Copy, Default)]
struct Level {
    total: f64,
    samples: u32,
    /// [`Limiter::seen`] at its latest sample.
    at: u64,
}

#[derive(Debug, Clone)]
pub struct Limiter {
    limit: usize,
    max: usize,
    /// Indexed by concurrency; `0` unused.
    levels: Vec<Level>,
    /// The last [`STRAIN_WINDOW`] replies: did each need a retry?
    recent: VecDeque<bool>,
    seen: u64,
}

impl Limiter {
    pub fn new(initial: usize, max: usize) -> Self {
        let max = max.max(1);
        Limiter {
            limit: initial.clamp(1, max),
            max,
            levels: vec![Level::default(); max + 2],
            recent: VecDeque::with_capacity(STRAIN_WINDOW + 1),
            seen: 0,
        }
    }

    /// How many requests may run at once now.
    pub fn permits(&self) -> usize {
        self.limit
    }

    /// Mean latency in seconds measured at the current level so far.
    pub fn latency(&self) -> Option<f64> {
        let level = &self.levels[self.limit];
        (level.samples > 0).then(|| level.total / level.samples as f64)
    }

    /// One finished request. `in_flight` is the level it ran at — the pool full at that count when
    /// it started and when it ended, the limit unchanged in between — or `None` otherwise: such a
    /// reply measures a mix of levels and is not used for latency (a retry still counts).
    pub fn record(&mut self, sample: Sample, in_flight: Option<usize>) {
        self.seen += 1;
        self.recent.push_back(sample == Sample::Strained);
        if self.recent.len() > STRAIN_WINDOW {
            self.recent.pop_front();
        }
        let rtt = match sample {
            Sample::Strained => {
                if self.recent.iter().filter(|&&strained| strained).count() >= STRAIN_LIMIT {
                    // One burst halves once: the window starts over at the new level.
                    self.recent.clear();
                    self.change((self.limit / 2).max(1));
                }
                return;
            }
            Sample::Ok(rtt) => rtt.as_secs_f64(),
        };
        let n = self.limit;
        if in_flight != Some(n) {
            return;
        }
        let level = &mut self.levels[n];
        level.total += rtt;
        level.samples += 1;
        level.at = self.seen;
        let Some(here) = self.throughput(n) else {
            return;
        };
        if let Some(below) = n.checked_sub(1).and_then(|m| self.throughput(m)) {
            if here < below * (1.0 + gain_needed(n - 1)) {
                self.change(n - 1);
                return;
            }
        }
        if n < self.max {
            match self.throughput(n + 1) {
                None => self.change(n + 1),
                Some(above) if above >= here * (1.0 + gain_needed(n)) => self.change(n + 1),
                Some(_) => {}
            }
        }
    }

    /// Replies per second at level `n`, while its measurement is complete and fresh.
    fn throughput(&self, n: usize) -> Option<f64> {
        let level = self.levels.get(n).filter(|_| n > 0)?;
        (level.samples >= MIN_SAMPLES && self.seen - level.at <= STALE_AFTER)
            .then(|| n as f64 * level.samples as f64 / level.total)
    }

    fn change(&mut self, to: usize) {
        if to == self.limit {
            return;
        }
        self.levels[to] = Level::default();
        self.limit = to;
    }
}

/// The gain a step from `n` to `n + 1` must show: [`GAIN_SHARE`] of the ideal `1 / n`.
fn gain_needed(n: usize) -> f64 {
    GAIN_SHARE / n as f64
}

struct State<T> {
    limiter: Limiter,
    idle: Vec<T>,
    busy: usize,
    /// Bumped whenever the limit moves, so a lease can tell whether it ran at one level.
    epoch: u64,
}

/// Up to [`Limiter::permits`] leases at once; a released item is kept for the next lease.
pub struct Pool<T> {
    label: &'static str,
    state: Mutex<State<T>>,
    freed: Notify,
}

impl<T> Pool<T> {
    pub fn new(label: &'static str, limiter: Limiter) -> Self {
        Pool {
            label,
            state: Mutex::new(State {
                limiter,
                idle: Vec::new(),
                busy: 0,
                epoch: 0,
            }),
            freed: Notify::new(),
        }
    }

    pub fn permits(&self) -> usize {
        self.state.lock().unwrap().limiter.permits()
    }

    /// Waits for a permit, then hands out an idle item or a new one from `open`.
    pub async fn lease<F>(&self, open: F) -> Result<Lease<'_, T>>
    where
        F: Fn() -> Result<T>,
    {
        loop {
            // Registered before the check, so a release between the check and the wait is not lost.
            let freed = self.freed.notified();
            let granted = {
                let mut st = self.state.lock().unwrap();
                if st.busy < st.limiter.permits() {
                    st.busy += 1;
                    Some((st.idle.pop(), st.busy, st.epoch))
                } else {
                    None
                }
            };
            match granted {
                Some((Some(item), busy, epoch)) => return Ok(self.leased(item, busy, epoch)),
                Some((None, busy, epoch)) => match open() {
                    Ok(item) => return Ok(self.leased(item, busy, epoch)),
                    Err(e) => {
                        self.release(None);
                        return Err(e);
                    }
                },
                None => freed.await,
            }
        }
    }

    fn leased(&self, item: T, busy: usize, epoch: u64) -> Lease<'_, T> {
        Lease {
            pool: self,
            item: Some(item),
            busy,
            epoch,
        }
    }

    fn release(&self, item: Option<T>) {
        {
            let mut st = self.state.lock().unwrap();
            st.busy -= 1;
            st.idle.extend(item);
        }
        self.freed.notify_waiters();
    }

    #[cfg(test)]
    pub fn seed(&self, item: T) {
        self.state.lock().unwrap().idle.push(item);
    }
}

/// One item out of a [`Pool`]. Give it back with [`Lease::finish`]; a lease dropped without it
/// (its request abandoned mid-flight) frees the permit but discards the item, which may still be
/// busy on the far side.
pub struct Lease<'a, T> {
    pool: &'a Pool<T>,
    item: Option<T>,
    /// The pool's `busy` and `epoch` right after this lease was granted.
    busy: usize,
    epoch: u64,
}

impl<T> Lease<'_, T> {
    pub fn get(&self) -> &T {
        self.item
            .as_ref()
            .expect("a lease holds its item until finished")
    }

    /// Returns the item for reuse and tells the limiter how its request went (`None`: nothing
    /// worth learning, e.g. a refusal that says nothing about the connection).
    pub fn finish(mut self, sample: Option<Sample>) {
        let item = self.item.take();
        if let Some(sample) = sample {
            let mut st = self.pool.state.lock().unwrap();
            let before = st.limiter.permits();
            let steady = (st.epoch == self.epoch && st.busy == self.busy).then_some(self.busy);
            st.limiter.record(sample, steady);
            let after = st.limiter.permits();
            if before != after {
                st.epoch += 1;
                tracing::info!(
                    pool = self.pool.label,
                    from = before,
                    to = after,
                    strained = sample == Sample::Strained,
                    "parallel sessions"
                );
            }
        }
        self.pool.release(item);
    }
}

impl<T> Drop for Lease<'_, T> {
    fn drop(&mut self) {
        if self.item.take().is_some() {
            self.pool.release(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    const SEC: fn(f64) -> Duration = Duration::from_secs_f64;

    /// Deterministic xorshift — the simulations must replay identically.
    struct Rng(u64);
    impl Rng {
        fn unit(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    struct Run {
        /// Mean permits over the second half, once the start-up ramp is behind.
        mean_permits: f64,
        max_permits: usize,
        /// Replies per second over the second half — the steady state a long fetch lives in.
        throughput: f64,
    }

    /// One simulated request, ordered by when it ends. Times in virtual milliseconds.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Flight {
        ends: u64,
        took: u64,
        strained: bool,
        /// How many ran at once, and the limit's epoch, when it started.
        level: usize,
        epoch: u64,
    }

    /// Event-driven run of `replies` requests through a limiter, in virtual time. A request
    /// started beside `n − 1` others takes `latency(n)` × uniform noise in [0.5, 1.5) — the
    /// spread of the 2026-10-07 invoice pages — and needs a retry with probability `strain(n)`.
    fn simulate(
        replies: usize,
        latency: impl Fn(usize) -> f64,
        strain: impl Fn(usize) -> f64,
    ) -> Run {
        let mut limiter = Limiter::new(2, 8);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut running: BinaryHeap<Reverse<Flight>> = BinaryHeap::new();
        let mut epoch = 0u64;
        let (mut now, mut done) = (0u64, 0usize);
        let (mut permit_sum, mut permit_n, mut max_permits) = (0.0, 0usize, 0usize);
        let mut half_time = 0u64;
        loop {
            while running.len() < limiter.permits() {
                let n = running.len() + 1;
                let took = (latency(n) * (0.5 + rng.unit()) * 1000.0) as u64;
                let strained = rng.unit() < strain(n);
                running.push(Reverse(Flight {
                    ends: now + took,
                    took,
                    strained,
                    level: n,
                    epoch,
                }));
            }
            let in_flight = running.len();
            let Reverse(flight) = running.pop().unwrap();
            let steady =
                (flight.epoch == epoch && in_flight == flight.level).then_some(flight.level);
            now = flight.ends;
            done += 1;
            let sample = if flight.strained {
                Sample::Strained
            } else {
                Sample::Ok(Duration::from_millis(flight.took))
            };
            let before = limiter.permits();
            limiter.record(sample, steady);
            if limiter.permits() != before {
                epoch += 1;
            }
            max_permits = max_permits.max(limiter.permits());
            if done == replies / 2 {
                half_time = now;
            }
            if done > replies / 2 {
                permit_sum += limiter.permits() as f64;
                permit_n += 1;
            }
            if done == replies {
                break;
            }
        }
        eprintln!(
            "sim: mean {:.2} max {} throughput {:.4}",
            permit_sum / permit_n as f64,
            max_permits,
            (replies - replies / 2) as f64 / ((now - half_time) as f64 / 1000.0)
        );
        Run {
            mean_permits: permit_sum / permit_n as f64,
            max_permits,
            throughput: (replies - replies / 2) as f64 / ((now - half_time) as f64 / 1000.0),
        }
    }

    #[test]
    fn a_site_with_room_to_spare_is_given_every_session_allowed() {
        let run = simulate(600, |_| 30.0, |_| 0.0);
        assert!(run.mean_permits > 7.5, "{}", run.mean_permits);
        assert!(run.throughput > 0.9 * 8.0 / 30.0, "{}", run.throughput);
    }

    /// The live measurement of 2026-10-07: ~30 s up to four sessions, ~41 s at eight — slower
    /// per page, yet still more pages per minute at every step.
    #[test]
    fn the_measured_site_gets_close_to_the_best_fixed_setting() {
        let site = |n: usize| 30.0 + 2.75 * n.saturating_sub(4) as f64;
        let run = simulate(1200, site, |_| 0.0);
        assert!(run.mean_permits > 6.5, "{}", run.mean_permits);
        // Within 10 % of the best fixed setting's throughput (eight sessions: 8 / 41 s).
        assert!(run.throughput > 0.9 * 8.0 / 41.0, "{}", run.throughput);
    }

    /// A thin link: three transfers fill it, and every one beyond stretches them all — more
    /// sessions only lengthen each request (toward the client's 90 s timeout) for no gain.
    #[test]
    fn a_link_that_three_requests_saturate_is_not_flooded() {
        let link = |n: usize| 30.0 * (n as f64 / 3.0).max(1.0);
        let run = simulate(1200, link, |_| 0.0);
        assert!(
            (2.0..=5.5).contains(&run.mean_permits),
            "{}",
            run.mean_permits
        );
        assert!(run.max_permits < 8, "{}", run.max_permits);
        assert!(run.throughput > 0.85 * 3.0 / 30.0, "{}", run.throughput);
    }

    /// An unstable connection whose drops grow with the load: retries pull the count down.
    #[test]
    fn retries_that_grow_with_parallelism_hold_it_down() {
        let calm = simulate(1200, |_| 30.0, |_| 0.0);
        let shaky = simulate(1200, |_| 30.0, |n| 0.03 * n as f64);
        assert!(shaky.mean_permits < 4.0, "{}", shaky.mean_permits);
        assert!(shaky.mean_permits < calm.mean_permits / 2.0);
    }

    #[test]
    fn the_limit_stays_within_one_and_the_cap() {
        let mut l = Limiter::new(2, 8);
        for _ in 0..50 {
            l.record(Sample::Strained, Some(1));
        }
        assert_eq!(l.permits(), 1);
        for _ in 0..500 {
            l.record(Sample::Ok(SEC(30.0)), Some(l.permits()));
        }
        assert_eq!(l.permits(), 8);
    }

    #[test]
    fn a_half_empty_pool_does_not_grow() {
        let mut l = Limiter::new(2, 8);
        for _ in 0..100 {
            l.record(Sample::Ok(SEC(30.0)), Some(1));
        }
        assert_eq!(l.permits(), 2);
    }

    #[tokio::test]
    async fn a_pool_never_hands_out_more_than_its_permits_and_reuses_items() {
        let pool: Pool<u32> = Pool::new("test", Limiter::new(2, 8));
        let opened = std::sync::atomic::AtomicU32::new(0);
        let open = || Ok(opened.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        let a = pool.lease(open).await.unwrap();
        let b = pool.lease(open).await.unwrap();
        let third = tokio::time::timeout(Duration::from_millis(50), pool.lease(open)).await;
        assert!(third.is_err(), "a third lease waits while two are out");
        let (a_item, b_item) = (*a.get(), *b.get());
        a.finish(None);
        let c = pool.lease(open).await.unwrap();
        assert_eq!(*c.get(), a_item, "the released item is reused");
        drop(b); // abandoned: its permit is freed, the item is not reused
        let d = pool.lease(open).await.unwrap();
        assert_ne!(*d.get(), b_item);
        assert_eq!(opened.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_waiting_lease_wakes_when_one_is_released() {
        let pool = std::sync::Arc::new(Pool::<u32>::new("test", Limiter::new(1, 8)));
        let first = pool.lease(|| Ok(1)).await.unwrap();
        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move { *pool.lease(|| Ok(2)).await.unwrap().get() })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        first.finish(Some(Sample::Ok(SEC(1.0))));
        let got = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("woken")
            .unwrap();
        assert_eq!(got, 1);
    }
    /// The site's own background of retries (2026-10-07: 0.7 % of pages), which more sessions do
    /// not cause, must cost no parallelism — halving on each one cost a third of the throughput.
    #[test]
    fn isolated_retries_at_the_sites_own_rate_cost_nothing() {
        let site = |n: usize| 30.0 + 2.75 * n.saturating_sub(4) as f64;
        let calm = simulate(3000, site, |_| 0.0);
        let background = simulate(3000, site, |_| 0.0067);
        assert!(background.mean_permits > 7.5, "{}", background.mean_permits);
        assert!(background.throughput > 0.95 * calm.throughput);
    }
}
