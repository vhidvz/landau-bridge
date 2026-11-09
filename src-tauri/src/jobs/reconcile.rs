//! Reconciliation of open write intents (plan 130 R-o), at the start of every job — after
//! PREFLIGHT, before anything is fetched, validated or planned. Each intent whose outcome is
//! unknown (a lost reply, a crash, a 5xx) is settled by reading its row and comparing it with the
//! intent: the target means the write landed (its invoice lines are marked now, so the next
//! aggregation cannot add them again); the snapshot means it never landed (the job plans the row
//! from scratch — never a replay of the old target); anything else is an external change, which
//! is held for the seller with both values and never overwritten.
//!
//! The rows are read from the whole shelf, live and trash, never by a product name: a product
//! renamed in between would read as "never landed" and its quantity would be added twice.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use serde_json::json;

use super::JobCtx;
use crate::db::intents::{place, status, IntentRow, RowState};
use crate::errors::{BridgeError, Result};
use crate::jobs::planner::find_row;
use crate::landau::{LandauClient, ShelfRow};

/// What the row says of an intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Landed,
    NotLanded,
    Superseded,
}

/// The intent's row in a store's two reads: by the id it knows, else by its tuple.
pub fn locate(intent: &IntentRow, live: &[ShelfRow], trash: &[ShelfRow]) -> RowState {
    let pick = |rows: &[ShelfRow]| -> Option<ShelfRow> {
        match intent.snapshot.stock_id.as_deref() {
            Some(id) => rows.iter().find(|r| r.id == id).cloned(),
            None => find_row(rows, &intent.product, &intent.authenticity).cloned(),
        }
    };
    if let Some(row) = pick(live) {
        RowState::of(&row, place::LIVE)
    } else if let Some(row) = pick(trash) {
        RowState::of(&row, place::TRASH)
    } else {
        RowState::absent()
    }
}

/// The fields the write sets — the cost, and the inventory when it set one — compared; never
/// the price (Landau's rule reprices every write). The target is judged without `updated_at`,
/// which every write moves. "Never landed" for a write that sets inventory also needs the row's
/// `updated_at` unmoved: a landed `3 → 4` followed by one sale reads `3` again, and only the
/// stamp tells it from a write that never arrived — judged "never landed", the next job would add
/// the unit twice (slice 3's review). A cost-only write carries no such arithmetic.
pub fn judge(intent: &IntentRow, now: &RowState) -> Verdict {
    let sets_inventory = intent.target_inventory.is_some();
    let same = |a: &RowState, cost: Option<i64>, inventory: Option<i64>| {
        a.cost == cost && (!sets_inventory || a.inventory == inventory)
    };
    if now.place == place::LIVE && same(now, Some(intent.target_cost), intent.target_inventory) {
        return Verdict::Landed;
    }
    let snap = &intent.snapshot;
    let untouched = !sets_inventory || now.updated_at == snap.updated_at;
    let unchanged = now.place == snap.place
        && (now.place == place::ABSENT || (same(now, snap.cost, snap.inventory) && untouched));
    if unchanged {
        Verdict::NotLanded
    } else {
        Verdict::Superseded
    }
}

/// How long after a write's last attempt its row may still read stale: the shelf is read from a
/// mirror about a second behind, sometimes more.
pub const SETTLE: Duration = Duration::from_secs(10);

/// When a write was last tried: its unknown-outcome stamp, else its opening.
fn last_try(intent: &IntentRow) -> Option<DateTime<Utc>> {
    let at = intent.attempted_at.as_deref().unwrap_or(&intent.opened_at);
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Settle every open intent before the job plans anything. A store whose shelf cannot be read
/// stops the job — except another store the key no longer reaches, whose intents stay open and
/// hold nothing here.
pub async fn reconcile(ctx: &JobCtx, landau: &LandauClient, job_store: &str) -> Result<()> {
    reconcile_with(ctx, landau, job_store, SETTLE).await
}

async fn read_store(landau: &LandauClient, store: &str) -> Result<(Vec<ShelfRow>, Vec<ShelfRow>)> {
    let live = landau.shelf(store).await?;
    let trash = landau.shelf_deleted(store).await?;
    Ok((live, trash))
}

/// [`reconcile`] with the lag bound given. "Never landed" is believed only on two whole-shelf
/// reads that agree, the second at least `settle` after the write's last attempt: a row the
/// mirror has not caught up with, or one a paged read skipped because it moved while the pages
/// were read (no shelf order is stable under writes), reads exactly like a write that never
/// arrived — and its quantity would be added twice (slice 3's review).
pub async fn reconcile_with(
    ctx: &JobCtx,
    landau: &LandauClient,
    job_store: &str,
    settle_after: Duration,
) -> Result<()> {
    let open = ctx.db.open_intents()?;
    if open.is_empty() {
        return Ok(());
    }
    ctx.progress("PREFLIGHT", 0, open.len() as u32, "بررسی نوشتن‌های نیمه‌تمام");
    let mut by_store: BTreeMap<&str, Vec<&IntentRow>> = BTreeMap::new();
    for i in &open {
        by_store.entry(i.store.as_str()).or_default().push(i);
    }
    let mut done = 0u32;
    for (store, intents) in by_store {
        let (live, trash) = match read_store(landau, store).await {
            Ok(rows) => rows,
            Err(e) if store != job_store && out_of_reach(&e) => {
                ctx.warn(
                    "PREFLIGHT",
                    e.kind(),
                    &format!(
                        "{} نوشتن نیمه‌تمام روی فروشگاه دیگری است که این کلید به آن دسترسی ندارد؛ تا دسترسی برنگردد باز می‌مانند.",
                        intents.len()
                    ),
                    Some(json!({"store": store, "intents": intents.iter().map(|i| i.id).collect::<Vec<_>>()})),
                );
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut judged: Vec<(&IntentRow, RowState, Verdict)> = intents
            .iter()
            .map(|i| {
                let now = locate(i, &live, &trash);
                let verdict = judge(i, &now);
                (*i, now, verdict)
            })
            .collect();
        let doubtful: Vec<&IntentRow> = judged
            .iter()
            .filter(|(_, _, v)| *v == Verdict::NotLanded)
            .map(|(i, _, _)| *i)
            .collect();
        if !doubtful.is_empty() {
            if let Some(latest) = doubtful.iter().filter_map(|i| last_try(i)).max() {
                let until = latest + chrono::Duration::from_std(settle_after).unwrap_or_default();
                if let Ok(wait) = (until - Utc::now()).to_std() {
                    tokio::time::sleep(wait).await;
                }
            }
            let (live, trash) = read_store(landau, store).await?;
            for entry in judged
                .iter_mut()
                .filter(|(_, _, v)| *v == Verdict::NotLanded)
            {
                let now = locate(entry.0, &live, &trash);
                entry.2 = judge(entry.0, &now);
                entry.1 = now;
            }
        }
        for (intent, now, verdict) in judged {
            settle(ctx, intent, &now, verdict)?;
            done += 1;
            ctx.progress(
                "PREFLIGHT",
                done,
                open.len() as u32,
                "بررسی نوشتن‌های نیمه‌تمام",
            );
        }
    }
    Ok(())
}

fn out_of_reach(e: &BridgeError) -> bool {
    matches!(
        e,
        BridgeError::LandauForbidden(_) | BridgeError::LandauRejected { status: 404, .. }
    )
}

fn settle(ctx: &JobCtx, intent: &IntentRow, now: &RowState, verdict: Verdict) -> Result<()> {
    let detail = json!({
        "intent": intent.id,
        "job": intent.job_id,
        "items": intent.items,
        "row": now,
        "snapshot": intent.snapshot,
        "target": {"inventory": intent.target_inventory, "cost": intent.target_cost},
        "reconciled_by": ctx.job_id,
        // PI-23: "never landed" rests on reads a lagging mirror can still fool; the next write onto
        // this row or these lines reads the row again before adding anything
        // ([`crate::landau::writer`]). Only a write that transfers invoice lines adds units twice
        // if it landed late; a cost-only or zero write sets absolute values.
        "recheck": verdict == Verdict::NotLanded
            && intent.target_inventory.is_some()
            && !intent.lines.is_empty(),
    });
    let (to, note) = match verdict {
        Verdict::Landed => (
            status::LANDED,
            format!(
                "نوشتنِ عملیات #{} روی قفسه نشسته بود (پاسخش نرسیده بود)؛ در عملیات #{} تأیید شد.",
                intent.job_id, ctx.job_id
            ),
        ),
        Verdict::NotLanded => (
            status::NOT_LANDED,
            format!(
                "نوشتنِ عملیات #{} به قفسه نرسیده بود؛ این ردیف در عملیات تازه از نو سنجیده می‌شود.",
                intent.job_id
            ),
        ),
        Verdict::Superseded => (
            status::SUPERSEDED,
            format!(
                "ردیف قفسه پس از نوشتنِ نیمه‌تمام عملیات #{} بیرون از پل تغییر کرده است؛ تا شما تصمیم نگیرید چیزی روی آن نوشته نمی‌شود.",
                intent.job_id
            ),
        ),
    };
    if !ctx.db.close_intent(intent.id, to, &detail)? {
        return Ok(());
    }
    for item in &intent.items {
        if let Err(e) = ctx.db.add_item_note(*item, &note) {
            tracing::warn!(item, error = %e, "reconciliation note not added");
        }
    }
    match verdict {
        Verdict::Superseded => ctx.warn("PREFLIGHT", "RowChanged", &note, Some(detail)),
        _ => ctx.info("PREFLIGHT", &note, Some(detail)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::intents::NewIntent;
    use crate::db::Db;

    fn row(id: &str, inventory: i64, cost: i64) -> ShelfRow {
        ShelfRow {
            id: id.into(),
            product: "p1".into(),
            product_name: Some("قطعه".into()),
            brand: None,
            store: Some("store".into()),
            inventory,
            price: Some(cost + 100),
            cost: Some(cost),
            condition: Some("NEW".into()),
            authenticity: Some("ORIGINAL".into()),
            price_source: None,
            updated_at: Some("2026-11-09T09:00:00.000Z".into()),
        }
    }

    fn intent(db: &Db, verb: &str, snap: RowState, inv: Option<i64>, cost: i64) -> IntentRow {
        let id = db
            .open_intent(&NewIntent {
                job_id: 3,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb,
                snapshot: &snap,
                target_inventory: inv,
                target_cost: cost,
                items: &[1],
                lines: &[],
            })
            .unwrap();
        db.intent(id).unwrap().unwrap()
    }

    /// Crash after the patch, before the mark: the row at the target landed.
    #[test]
    fn a_row_at_the_target_landed() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            Some(5),
            1_000,
        );
        let now = locate(&i, &[row("s1", 5, 1_000)], &[]);
        assert_eq!(judge(&i, &now), Verdict::Landed);
    }

    #[test]
    fn a_row_at_the_snapshot_never_landed() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            Some(5),
            1_000,
        );
        let now = locate(&i, &[row("s1", 3, 900)], &[]);
        assert_eq!(judge(&i, &now), Verdict::NotLanded);
    }

    /// A POS sale (or a hand edit) after the snapshot: neither — held, never overwritten.
    #[test]
    fn a_row_at_neither_was_changed_outside() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            Some(5),
            1_000,
        );
        for other in [row("s1", 4, 1_000), row("s1", 2, 900), row("s1", 5, 950)] {
            let now = locate(&i, &[other], &[]);
            assert_eq!(judge(&i, &now), Verdict::Superseded);
        }
        let gone = locate(&i, &[], &[]);
        assert_eq!(gone.place, place::ABSENT);
        assert_eq!(judge(&i, &gone), Verdict::Superseded);
    }

    /// An attach whose reply was lost has no row id: the tuple finds its row.
    #[test]
    fn an_attach_is_found_by_its_tuple() {
        let db = Db::in_memory().unwrap();
        let i = intent(&db, "ATTACH", RowState::absent(), Some(4), 1_000);
        assert_eq!(
            judge(&i, &locate(&i, &[row("new", 4, 1_000)], &[])),
            Verdict::Landed
        );
        assert_eq!(judge(&i, &locate(&i, &[], &[])), Verdict::NotLanded);
        assert_eq!(
            judge(&i, &locate(&i, &[row("new", 7, 1_000)], &[])),
            Verdict::Superseded
        );
    }

    /// A cost-only write ignores inventory: a sale meanwhile does not hide the landing.
    #[test]
    fn a_cost_only_write_compares_the_cost_alone() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            None,
            1_000,
        );
        assert_eq!(
            judge(&i, &locate(&i, &[row("s1", 1, 1_000)], &[])),
            Verdict::Landed
        );
        assert_eq!(
            judge(&i, &locate(&i, &[row("s1", 1, 900)], &[])),
            Verdict::NotLanded
        );
    }

    // ------------------------------------------------------------ against a Landau (task 10)

    use crate::db::intents::resolution;
    use crate::domain::{InvoiceHeader, InvoiceKind};
    use crate::jobs::planner::aggregate;
    use crate::landau::stub::{self, page, row_json, Reply};
    use std::collections::HashSet;

    fn ctx(db: Db) -> JobCtx {
        let job_id = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        JobCtx {
            db,
            app: None,
            job_id,
            cancel: Default::default(),
        }
    }

    fn invoice(db: &Db, number: &str) -> i64 {
        db.upsert_invoice(
            "barezpakhsh",
            &InvoiceHeader {
                number: number.into(),
                kind: InvoiceKind::Sale,
                date_jalali: "1404/08/18".into(),
                date_iso: "2025-11-09".into(),
                paid: true,
                url: format!("/f/{number}"),
            },
        )
        .unwrap()
    }

    /// An open quantity patch of job `job`: the row was 3 at 900, the write sets 5 at 1 000 and
    /// transfers line `(invoice, C1)`.
    fn open_patch(db: &Db, job: i64, store: &str, invoice: i64, item: i64) -> i64 {
        let snap = RowState::of(&row("s1", 3, 900), place::LIVE);
        db.open_intent(&NewIntent {
            job_id: job,
            kind: "IMPORT",
            store,
            product: "p1",
            authenticity: "ORIGINAL",
            verb: "PATCH",
            snapshot: &snap,
            target_inventory: Some(5),
            target_cost: 1_000,
            items: &[item],
            lines: &[(invoice, "C1".into())],
        })
        .unwrap()
    }

    fn line(invoice: i64) -> crate::db::invoices::LineWithInvoice {
        crate::db::invoices::LineWithInvoice {
            invoice_id: invoice,
            number: "n".into(),
            kind: InvoiceKind::Sale,
            paid: true,
            date_jalali: "1404/08/18".into(),
            code: "C1".into(),
            name: "قطعه".into(),
            qty: 2,
            unit: "عدد".into(),
            unit_price: 1,
        }
    }

    /// PI-7: the process died after the patch landed and before its lines were marked. The next
    /// job reads the row at the target, marks the lines, and the aggregation adds nothing again.
    #[tokio::test]
    async fn a_crash_after_the_write_marks_its_lines_before_anything_is_planned() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7001");
        let old = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        let item = db.add_job_item(old, "C1", "قطعه", 2).unwrap();
        db.set_job_status(old, "INTERRUPTED", "WRITE").unwrap();
        let id = open_patch(&db, old, "store", inv, item);
        let ctx = ctx(db);
        let (url, seen) = stub::serve(|r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 5, 1_000)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(ctx.db.intent(id).unwrap().unwrap().status, status::LANDED);
        let already = ctx.db.imported_lines().unwrap();
        assert!(already.contains(&(inv, "C1".into())));
        let agg = aggregate(
            &[line(inv)],
            crate::domain::ImportMode::Quantity,
            None,
            &already,
        );
        assert!(
            agg.items.is_empty(),
            "the landed quantity is not aggregated again"
        );
        assert!(stub::writes(&seen).is_empty(), "reconciliation only reads");
        let note = ctx.db.job_items(old).unwrap()[0]
            .note
            .clone()
            .unwrap_or_default();
        assert!(note.contains(&format!("#{}", ctx.job_id)), "{note}");
    }

    /// A write that never landed is closed and planned again from scratch — its old target is
    /// never sent, and its lines stay to be transferred.
    #[tokio::test]
    async fn a_write_that_never_landed_is_replanned_not_replayed() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7002");
        let id = open_patch(&db, 1, "store", inv, 1);
        let ctx = ctx(db);
        let (url, seen) = stub::serve(|r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        assert!(stub::writes(&seen).is_empty(), "never replayed");
        let already = ctx.db.imported_lines().unwrap();
        let agg = aggregate(
            &[line(inv)],
            crate::domain::ImportMode::Quantity,
            None,
            &already,
        );
        assert_eq!(
            agg.items.len(),
            1,
            "the lines are transferred by the new plan"
        );
        assert!(ctx.db.open_intents().unwrap().is_empty());
        assert!(
            ctx.db.unresolved_superseded().unwrap().is_empty(),
            "nothing held"
        );
    }

    /// The job's own proven write of `qty` onto `p1` at 1 000, from link «قطعه» (code C1).
    fn proven_write(db: &Db, job: i64, qty: i64) -> crate::jobs::proof::ProvenWrite {
        proven_write_on(db, job, ("قطعه", "C1"), "p1", qty)
    }

    /// [`proven_write`] from link `name` (code) onto `product`.
    fn proven_write_on(
        db: &Db,
        job: i64,
        (name, code): (&str, &str),
        product: &str,
        qty: i64,
    ) -> crate::jobs::proof::ProvenWrite {
        use crate::db::matches::{state, LinkProduct};
        use crate::jobs::proof::{plan_import, ImportCandidate, Planned};
        db.set_match(
            "barezpakhsh",
            name,
            code,
            Some((product, "x")),
            "USER",
            None,
        )
        .unwrap();
        let p = LinkProduct {
            id: product,
            name: "x",
            brand: Some(""),
            alias: Some(""),
        };
        let key = crate::matching::normalize::name_key(name);
        let read = db.link_by_key("barezpakhsh", &key).unwrap().unwrap();
        let v = db
            .validate_link(
                "barezpakhsh",
                &key,
                read.link_version,
                p,
                state::VALID,
                None,
                job,
            )
            .unwrap()
            .unwrap();
        let plan = plan_import(
            db,
            "barezpakhsh",
            job,
            &[ImportCandidate {
                item_id: 1,
                product: product.into(),
                authenticity: crate::domain::Authenticity::Original,
                cost: 1_000,
                qty,
                link_key: key,
                link_version: v,
            }],
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        match plan.into_iter().next().unwrap() {
            Planned::Write(w) => w,
            p => panic!("{p:?}"),
        }
    }

    /// PI-23's run: job 1's quantity patch (3 → 5 at 1 000, line C1) lost its reply. This job's
    /// reconciliation reads the whole shelf twice and both reads still show the snapshot (the
    /// mirror lags past the settle), so the intent closes `NOT_LANDED` and line C1 is released;
    /// the job aggregates it again (2 units) and writes. The writer's fresh read — a term read,
    /// later than both — answers `fresh`.
    async fn written_after_a_late_mirror(
        fresh: String,
    ) -> (
        Result<crate::landau::writer::Written>,
        stub::Seen,
        JobCtx,
        i64,
        i64,
    ) {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7010");
        let id = open_patch(&db, 1, "store", inv, 1);
        let ctx = ctx(db);
        let (url, seen) = stub::serve(move |r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else if r.method == "PATCH" {
                Reply::Status(200, row_json("s1", "p1", 5, 1_000))
            } else if r.path.contains("term=") {
                Reply::Status(200, page(std::slice::from_ref(&fresh)))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED,
            "both lagging reads showed the snapshot"
        );
        let w = proven_write(&ctx.db, ctx.job_id, 2);
        let lines = vec![(inv, "C1".to_string())];
        let req = crate::landau::writer::ImportWrite {
            store: "store",
            mode: crate::domain::ImportMode::Quantity,
            available: true,
            min_margin_percent: 15.0,
            term: "قطعه",
            lines: &lines,
        };
        let mut shelf = vec![row("s1", 3, 900)];
        let out = crate::landau::writer::ShelfWriter::new(&client, &ctx.db, "barezpakhsh")
            .import(&w, &req, &mut shelf)
            .await;
        (out, seen, ctx, id, inv)
    }

    /// PI-23 (DoD 24's review, finding 2): by WRITE the mirror shows job 1's patch landed (5, its
    /// stamp moved). The job's write of the same line is held — never 5 + 2 — and job 1's write is
    /// settled `LANDED` after all, its line marked, so no later job adds it either.
    #[tokio::test]
    async fn a_landing_the_mirror_showed_late_is_never_added_twice() {
        let landed = row_json("s1", "p1", 5, 1_000).replace("09:00:00", "09:00:12");
        let (out, seen, ctx, id, inv) = written_after_a_late_mirror(landed).await;
        assert!(matches!(out, Err(BridgeError::RowChanged(_))), "{out:?}");
        assert!(
            stub::writes(&seen).is_empty(),
            "nothing sent: {:?}",
            stub::lines(&seen)
        );
        assert_eq!(ctx.db.intent(id).unwrap().unwrap().status, status::LANDED);
        assert!(ctx
            .db
            .imported_lines()
            .unwrap()
            .contains(&(inv, "C1".into())));
    }

    /// The fresh read still shows the snapshot: job 1's write truly never landed, and this job's
    /// write goes on (3 + 2). A row that moved some other way (a sale: 2, its stamp moved) is held
    /// for the seller, the earlier write superseded, its line unmarked.
    #[tokio::test]
    async fn a_never_landed_write_is_rechecked_once_by_the_writer() {
        let (out, seen, ctx, id, _) =
            written_after_a_late_mirror(row_json("s1", "p1", 3, 900)).await;
        let done = out.unwrap();
        assert_eq!(done.qty_after, Some(5));
        assert_eq!(stub::writes(&seen).pop().unwrap().json()["inventory"], 5);
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        assert!(ctx.db.rechecks().unwrap().is_empty(), "checked once");

        let sold = row_json("s1", "p1", 2, 900).replace("09:00:00", "09:03:00");
        let (out, seen, ctx, id, _) = written_after_a_late_mirror(sold).await;
        assert!(matches!(out, Err(BridgeError::RowChanged(_))), "{out:?}");
        assert!(stub::writes(&seen).is_empty());
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::SUPERSEDED
        );
        assert!(ctx.db.imported_lines().unwrap().is_empty());
        assert!(ctx
            .db
            .unresolved_superseded()
            .unwrap()
            .iter()
            .any(|i| i.id == id));
    }

    /// PI-23's review, finding 1: job 1's write carried codes C1 and C2 onto `p1`. Its reply was
    /// lost and the mirror lagged past the settle, so this job's reconciliation released both lines;
    /// C2 has since been linked to `p2`. The job's first write (C1, onto `p1`) finds job 1's write
    /// landed after all and marks both lines. Its second write (C2, onto `p2`) must not send C2's
    /// units again: they are on `p1`.
    #[tokio::test]
    async fn a_line_marked_mid_job_is_never_sent_again() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7012");
        let snap = RowState::of(&row("s1", 3, 900), place::LIVE);
        let old = db
            .open_intent(&NewIntent {
                job_id: 1,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "PATCH",
                snapshot: &snap,
                target_inventory: Some(5),
                target_cost: 1_000,
                items: &[1, 2],
                lines: &[(inv, "C1".into()), (inv, "C2".into())],
            })
            .unwrap();
        let ctx = ctx(db);
        let landed = row_json("s1", "p1", 5, 1_000).replace("09:00:00", "09:00:12");
        let (url, seen) = stub::serve(move |r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else if r.method != "GET" {
                Reply::Status(201, row_json("s2", "p2", 1, 1_000))
            } else if r.path.contains("term=") {
                Reply::Status(200, page(std::slice::from_ref(&landed)))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        let writer = crate::landau::writer::ShelfWriter::new(&client, &ctx.db, "barezpakhsh");
        let req = |lines| crate::landau::writer::ImportWrite {
            store: "store",
            mode: crate::domain::ImportMode::Quantity,
            available: true,
            min_margin_percent: 15.0,
            term: "قطعه",
            lines,
        };
        let first = proven_write_on(&ctx.db, ctx.job_id, ("قطعه", "C1"), "p1", 1);
        let c1 = vec![(inv, "C1".to_string())];
        let out = writer
            .import(&first, &req(&c1), &mut vec![row("s1", 3, 900)])
            .await;
        assert!(matches!(out, Err(BridgeError::RowChanged(_))), "{out:?}");
        assert_eq!(ctx.db.intent(old).unwrap().unwrap().status, status::LANDED);

        let second = proven_write_on(&ctx.db, ctx.job_id, ("قطعه دوم", "C2"), "p2", 1);
        let c2 = vec![(inv, "C2".to_string())];
        let out = writer.import(&second, &req(&c2), &mut Vec::new()).await;
        assert!(matches!(out, Err(BridgeError::RowChanged(_))), "{out:?}");
        assert!(
            stub::writes(&seen).is_empty(),
            "nothing sent: {:?}",
            stub::lines(&seen)
        );
    }

    /// PI-23's review, finding 3: only a write that transfers invoice lines is read again — a ZERO
    /// attach (inventory 0, no lines) adds no units if it landed late. And a refresh onto a row
    /// whose quantity write was read as never landed settles it from its own fresh read: at the
    /// snapshot the flag clears once the refresh lands; at the target the earlier write is
    /// settled `LANDED` (its line marked) and the refresh, which adds nothing, still writes. Left
    /// flagged, the refresh's own move of the row would later read as a change from outside.
    #[tokio::test]
    async fn only_unit_writes_are_reread_and_a_refresh_settles_them() {
        // a ZERO attach is not flagged
        let db = Db::in_memory().unwrap();
        let attach = db
            .open_intent(&NewIntent {
                job_id: 1,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "ATTACH",
                snapshot: &RowState::absent(),
                target_inventory: Some(0),
                target_cost: 1_000,
                items: &[1],
                lines: &[],
            })
            .unwrap();
        let ctx0 = ctx(db);
        let (url, _) = stub::serve(|_| Reply::Status(200, page(&[])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx0, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx0.db.intent(attach).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        assert!(ctx0.db.rechecks().unwrap().is_empty(), "not flagged");

        // a refresh over a flagged quantity write, its fresh read at `fresh`
        let refresh = |fresh: String| async move {
            let db = Db::in_memory().unwrap();
            let inv = invoice(&db, "7013");
            let id = open_patch(&db, 1, "store", inv, 1);
            let ctx = ctx(db);
            let (url, seen) = stub::serve(move |r| {
                if r.path.contains("deleted=true") {
                    Reply::Status(200, page(&[]))
                } else if r.method == "PATCH" {
                    Reply::Status(200, row_json("s1", "p1", 3, 1_200))
                } else if r.path.contains("term=") {
                    Reply::Status(200, page(std::slice::from_ref(&fresh)))
                } else {
                    Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
                }
            });
            let client = LandauClient::new(&url, "apt-test").unwrap();
            reconcile_with(&ctx, &client, "store", Duration::ZERO)
                .await
                .unwrap();
            let link = {
                proven_write(&ctx.db, ctx.job_id, 0);
                ctx.db
                    .match_for_name("barezpakhsh", "قطعه")
                    .unwrap()
                    .unwrap()
            };
            let crate::jobs::proof::Planned::Write(w) = crate::jobs::proof::prove_refresh(
                &ctx.db,
                "barezpakhsh",
                ctx.job_id,
                9,
                "s1",
                "p1",
                &[(link, Some(1_200))],
            )
            .unwrap() else {
                panic!("a refresh write")
            };
            let out = crate::landau::writer::ShelfWriter::new(&client, &ctx.db, "barezpakhsh")
                .refresh(&w, "store", true, &row("s1", 3, 900))
                .await;
            (out, seen, ctx, id, inv)
        };
        let (out, seen, ctx, id, _) = refresh(row_json("s1", "p1", 3, 900)).await;
        assert!(out.is_ok(), "{out:?}");
        assert_eq!(stub::writes(&seen).len(), 1);
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        assert!(
            ctx.db.rechecks().unwrap().is_empty(),
            "settled by the refresh"
        );

        let landed = row_json("s1", "p1", 5, 1_000).replace("09:00:00", "09:00:12");
        let (out, seen, ctx, id, inv) = refresh(landed).await;
        assert!(out.is_ok(), "the refresh adds no units: {out:?}");
        assert_eq!(stub::writes(&seen).len(), 1);
        assert_eq!(ctx.db.intent(id).unwrap().unwrap().status, status::LANDED);
        assert!(ctx
            .db
            .imported_lines()
            .unwrap()
            .contains(&(inv, "C1".into())));
    }

    /// PI-23's review, finding 4: a never-landed restore (its row was in the trash) is met by a
    /// write whose fresh read — live rows only — finds no row. That says nothing of the trash:
    /// the intent waits for the restore path's own check, never superseded on the absence.
    #[tokio::test]
    async fn a_never_landed_restore_is_not_superseded_by_a_live_read() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7014");
        let mut trashed = RowState::of(&row("s1", 3, 900), place::TRASH);
        trashed.place = place::TRASH.into();
        let id = db
            .open_intent(&NewIntent {
                job_id: 1,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "RESTORE",
                snapshot: &trashed,
                target_inventory: Some(2),
                target_cost: 1_000,
                items: &[1],
                lines: &[(inv, "C1".into())],
            })
            .unwrap();
        let ctx = ctx(db);
        let trash_row = row_json("s1", "p1", 3, 900);
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "GET" if r.path.contains("deleted=true") => {
                Reply::Status(200, page(std::slice::from_ref(&trash_row)))
            }
            "GET" => Reply::Status(200, page(&[])),
            "POST" if r.path.contains("restore") => {
                Reply::Status(200, row_json("s1", "p1", 3, 900))
            }
            "POST" => Reply::Status(409, "{}".into()),
            _ => Reply::Status(200, row_json("s1", "p1", 2, 1_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        let w = proven_write(&ctx.db, ctx.job_id, 2);
        let lines = vec![(inv, "C1".to_string())];
        let req = crate::landau::writer::ImportWrite {
            store: "store",
            mode: crate::domain::ImportMode::Quantity,
            available: true,
            min_margin_percent: 15.0,
            term: "قطعه",
            lines: &lines,
        };
        let out = crate::landau::writer::ShelfWriter::new(&client, &ctx.db, "barezpakhsh")
            .import(&w, &req, &mut Vec::new())
            .await;
        assert!(out.is_ok(), "{out:?} {:?}", stub::lines(&seen));
        assert_eq!(
            ctx.db.intent(id).unwrap().unwrap().status,
            status::NOT_LANDED,
            "still never landed, the trash row unchanged"
        );
        assert!(ctx.db.rechecks().unwrap().is_empty());
    }

    /// PI-23 through an attach: job 1's attach (2 at 1 000, line C1) lost its reply, and the mirror
    /// still lacks the row at reconciliation and at this job's fresh read, so this job attaches
    /// again. Landau answers 409 — the row exists — and the re-read now finds it at job 1's
    /// target: that attach landed after all. Held, nothing patched on top, job 1's line marked.
    #[tokio::test]
    async fn an_attach_answered_409_after_a_never_landed_attach_is_held() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7011");
        let snap = RowState::absent();
        let old = db
            .open_intent(&NewIntent {
                job_id: 1,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "ATTACH",
                snapshot: &snap,
                target_inventory: Some(2),
                target_cost: 1_000,
                items: &[1],
                lines: &[(inv, "C1".into())],
            })
            .unwrap();
        let ctx = ctx(db);
        let live_reads = Arc::new(AtomicUsize::new(0));
        let n = live_reads.clone();
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "POST" => Reply::Status(409, r#"{"message":"exists"}"#.into()),
            "GET" if r.path.contains("deleted=true") || r.path.contains("term=") => {
                Reply::Status(200, page(&[]))
            }
            // reconciliation's two reads lag; the read after the 409 does not
            "GET" if n.fetch_add(1, Ordering::SeqCst) < 2 => Reply::Status(200, page(&[])),
            "GET" => Reply::Status(200, page(&[row_json("s9", "p1", 2, 1_000)])),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(old).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        let w = proven_write(&ctx.db, ctx.job_id, 2);
        let lines = vec![(inv, "C1".to_string())];
        let req = crate::landau::writer::ImportWrite {
            store: "store",
            mode: crate::domain::ImportMode::Quantity,
            available: true,
            min_margin_percent: 15.0,
            term: "قطعه",
            lines: &lines,
        };
        let out = crate::landau::writer::ShelfWriter::new(&client, &ctx.db, "barezpakhsh")
            .import(&w, &req, &mut Vec::new())
            .await;
        assert!(matches!(out, Err(BridgeError::RowChanged(_))), "{out:?}");
        assert!(
            stub::writes(&seen).iter().all(|r| r.method == "POST"),
            "no patch on top: {:?}",
            stub::lines(&seen)
        );
        assert_eq!(ctx.db.intent(old).unwrap().unwrap().status, status::LANDED);
        assert!(ctx
            .db
            .imported_lines()
            .unwrap()
            .contains(&(inv, "C1".into())));
        let mine = ctx.db.intent(old + 1).unwrap().unwrap();
        assert_eq!(
            mine.status,
            status::NOT_LANDED,
            "this attach landed nothing"
        );
    }

    /// An external change before resume: neither the snapshot nor the target. Held for the
    /// seller with both values — the row is not overwritten, the lines not marked — until settled.
    #[tokio::test]
    async fn an_external_change_before_resume_is_held_never_overwritten() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7003");
        let id = open_patch(&db, 1, "store", inv, 1);
        let ctx = ctx(db);
        let (url, seen) = stub::serve(|r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else {
                // a POS sale took one: 2 where 3 (snapshot) or 5 (target) was expected
                Reply::Status(200, page(&[row_json("s1", "p1", 2, 900)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        let got = ctx.db.intent(id).unwrap().unwrap();
        assert_eq!(got.status, status::SUPERSEDED);
        let outcome = got.outcome.unwrap();
        assert_eq!(outcome["row"]["inventory"], 2);
        assert_eq!(outcome["snapshot"]["inventory"], 3);
        assert_eq!(outcome["target"]["inventory"], 5);
        assert!(stub::writes(&seen).is_empty());
        assert!(ctx.db.imported_lines().unwrap().is_empty());
        let held: HashSet<i64> = ctx
            .db
            .unresolved_superseded()
            .unwrap()
            .iter()
            .map(|i| i.id)
            .collect();
        assert!(held.contains(&id));
        ctx.db.set_job_status(ctx.job_id, "DONE", "REPORT").unwrap();
        assert!(ctx.db.resolve_intent(id, resolution::ON_SHELF).unwrap());
        assert!(ctx
            .db
            .imported_lines()
            .unwrap()
            .contains(&(inv, "C1".into())));
    }

    /// Intents of two older jobs are both settled by the newer one, oldest first, before it
    /// plans anything — one landed, one not.
    #[tokio::test]
    async fn every_older_jobs_intent_is_settled_first() {
        let db = Db::in_memory().unwrap();
        let (a, b) = (invoice(&db, "7004"), invoice(&db, "7005"));
        let first = open_patch(&db, 1, "store", a, 1);
        let snap = RowState::absent();
        let second = db
            .open_intent(&NewIntent {
                job_id: 2,
                kind: "IMPORT",
                store: "store",
                product: "p2",
                authenticity: "ORIGINAL",
                verb: "ATTACH",
                snapshot: &snap,
                target_inventory: Some(1),
                target_cost: 2_000,
                items: &[2],
                lines: &[(b, "C2".into())],
            })
            .unwrap();
        let ctx = ctx(db);
        let (url, _) = stub::serve(|r| {
            if r.path.contains("deleted=true") {
                Reply::Status(200, page(&[]))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 5, 1_000)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(first).unwrap().unwrap().status,
            status::LANDED
        );
        assert_eq!(
            ctx.db.intent(second).unwrap().unwrap().status,
            status::NOT_LANDED
        );
        assert!(ctx.db.open_intents().unwrap().is_empty());
        let already = ctx.db.imported_lines().unwrap();
        assert!(already.contains(&(a, "C1".into())));
        assert!(!already.contains(&(b, "C2".into())));
    }

    /// Slice 3's review: the mirror had not caught up with a write whose reply was lost. The first
    /// read shows the snapshot; the confirming read, past the lag bound, shows the target — landed,
    /// its lines marked, nothing added again.
    #[tokio::test]
    async fn never_landed_is_believed_only_on_a_second_read_past_the_lag() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7007");
        let id = open_patch(&db, 1, "store", inv, 1);
        let ctx = ctx(db);
        let reads = Arc::new(AtomicUsize::new(0));
        let n = reads.clone();
        let (url, _) = stub::serve(move |r| {
            if r.path.contains("deleted=true") {
                return Reply::Status(200, page(&[]));
            }
            if n.fetch_add(1, Ordering::SeqCst) == 0 {
                Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
            } else {
                Reply::Status(200, page(&[row_json("s1", "p1", 5, 1_000)]))
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let started = std::time::Instant::now();
        reconcile_with(&ctx, &client, "store", Duration::from_millis(300))
            .await
            .unwrap();
        assert!(
            started.elapsed() >= Duration::from_millis(250),
            "waited out the lag bound"
        );
        assert_eq!(ctx.db.intent(id).unwrap().unwrap().status, status::LANDED);
        assert!(ctx
            .db
            .imported_lines()
            .unwrap()
            .contains(&(inv, "C1".into())));
        assert_eq!(reads.load(Ordering::SeqCst), 2, "two live reads");
    }

    /// A landed or superseded verdict needs no confirming read.
    #[tokio::test]
    async fn a_landed_verdict_is_not_read_twice() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7008");
        open_patch(&db, 1, "store", inv, 1);
        let ctx = ctx(db);
        let reads = Arc::new(AtomicUsize::new(0));
        let n = reads.clone();
        let (url, _) = stub::serve(move |r| {
            if !r.path.contains("deleted=true") {
                n.fetch_add(1, Ordering::SeqCst);
            }
            Reply::Status(200, page(&[row_json("s1", "p1", 5, 1_000)]))
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    /// The job's own store unreadable stops the job before anything is planned; another store
    /// the key no longer reaches leaves its intents open and holds nothing here.
    #[tokio::test]
    async fn an_unreadable_shelf_stops_the_job_unless_it_is_another_store() {
        let db = Db::in_memory().unwrap();
        let inv = invoice(&db, "7006");
        let elsewhere = open_patch(&db, 1, "old-store", inv, 1);
        let ctx = ctx(db);
        let (url, _) =
            stub::serve(|_| Reply::Status(403, r#"{"message":"Forbidden resource"}"#.into()));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(
            ctx.db.intent(elsewhere).unwrap().unwrap().status,
            status::OPEN
        );
        let here = open_patch(&ctx.db, 1, "store", inv, 1);
        let e = reconcile_with(&ctx, &client, "store", Duration::ZERO)
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::LandauForbidden(_)), "{e:?}");
        assert_eq!(ctx.db.intent(here).unwrap().unwrap().status, status::OPEN);
    }

    fn stamped(id: &str, inventory: i64, cost: i64, at: &str) -> ShelfRow {
        ShelfRow {
            updated_at: Some(at.into()),
            ..row(id, inventory, cost)
        }
    }

    /// Slice 3's review: a landed `3 → 4` at an unchanged cost, then one sale, reads `3` again.
    /// Only the moved stamp tells it from a write that never arrived — held, never added twice.
    #[test]
    fn a_sale_after_a_landed_write_is_not_read_as_never_landed() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            Some(4),
            900,
        );
        let sold = stamped("s1", 3, 900, "2026-11-09T09:05:00.000Z");
        assert_eq!(judge(&i, &locate(&i, &[sold], &[])), Verdict::Superseded);
        assert_eq!(
            judge(&i, &locate(&i, &[row("s1", 3, 900)], &[])),
            Verdict::NotLanded,
            "an untouched row is still never landed"
        );
        assert_eq!(
            judge(
                &i,
                &locate(
                    &i,
                    &[stamped("s1", 4, 900, "2026-11-09T09:01:00.000Z")],
                    &[]
                )
            ),
            Verdict::Landed
        );
    }

    /// A cost-only write carries no quantity arithmetic: a sale's stamp does not hold it.
    #[test]
    fn a_cost_only_write_ignores_the_stamp() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "PATCH",
            RowState::of(&row("s1", 3, 900), place::LIVE),
            None,
            1_000,
        );
        let sold = stamped("s1", 2, 900, "2026-11-09T09:05:00.000Z");
        assert_eq!(judge(&i, &locate(&i, &[sold], &[])), Verdict::NotLanded);
    }

    /// A restore that landed but whose patch did not: the row is live with its old values —
    /// neither the trashed snapshot nor the target, so the seller decides.
    #[test]
    fn a_restore_without_its_patch_is_held() {
        let db = Db::in_memory().unwrap();
        let i = intent(
            &db,
            "RESTORE",
            RowState::of(&row("s1", 2, 700), place::TRASH),
            Some(4),
            1_000,
        );
        assert_eq!(
            judge(&i, &locate(&i, &[], &[row("s1", 2, 700)])),
            Verdict::NotLanded
        );
        assert_eq!(
            judge(&i, &locate(&i, &[row("s1", 2, 700)], &[])),
            Verdict::Superseded
        );
        assert_eq!(
            judge(&i, &locate(&i, &[row("s1", 4, 1_000)], &[])),
            Verdict::Landed
        );
    }
}
