//! Plan 130 task 5: the evidence sets of `fixtures/evidence/` measured through the job's own code.
//! It uses the shared `gather`, `judge_top`, `judge_link` and `verdict`, and `Prover` itself for the
//! seller's links. It prints the table with its sample sizes and one-sided 95 % bounds, and pins
//! every count. A change to a rule or a table entry therefore turns this test red until it is
//! measured and the table is recorded again (the plan's independence rule).
//!
//! A case the gate decides against its label must be adjudicated by reading, never trusted either
//! way. It is listed in `adjudicated.tsv`, and a hard-case pair in `hard-cases.tsv`'s `agent`
//! column. An unread case fails the test and is printed for reading.
//!
//! Slow unoptimised (the whole registry, about 3,500 names), so it is ignored by default:
//! `cargo test --release --test evidence_sets -- --ignored --nocapture`.
//! The sets are built by `scripts/evidence-sets.mjs`.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::PathBuf;

use landau_bridge_lib::db::Db;
use landau_bridge_lib::errors::BridgeError;
use landau_bridge_lib::jobs::proof::{Judgement, Prover};
use landau_bridge_lib::landau::Candidate;
use landau_bridge_lib::matching::evidence::evidence;
use landau_bridge_lib::matching::gate::{
    conflicts, judge_link, judge_top, v4_reasons, verdict, Judged, Origin, Reason, Verdict,
};
use landau_bridge_lib::matching::normalize::search_term;
use landau_bridge_lib::matching::registry::Registry;
use landau_bridge_lib::matching::{gather, similarity_of, Scored};

/// The seller's own setting; the gate never reads it below its floor of 85.
const THRESHOLD: u8 = 70;
const SUPPLIER: &str = "barezpakhsh";

type Row = HashMap<String, Option<String>>;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/evidence")
}

/// A TSV of the builder: `#` comments, a header, `\N` for a value never recorded.
fn tsv(text: &str) -> Vec<Row> {
    let mut lines = text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    let head: Vec<String> = lines
        .next()
        .expect("a header")
        .split('\t')
        .map(str::to_string)
        .collect();
    lines
        .map(|l| {
            let cells: Vec<&str> = l.split('\t').collect();
            assert_eq!(cells.len(), head.len(), "a short line: {l}");
            head.iter()
                .zip(cells)
                .map(|(h, v)| (h.clone(), (v != "\\N").then(|| v.to_string())))
                .collect()
        })
        .collect()
}

fn read(name: &str) -> Vec<Row> {
    tsv(&std::fs::read_to_string(dir().join(name)).expect(name))
}

fn get<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k)
        .unwrap_or_else(|| panic!("no column {k}"))
        .as_deref()
        .unwrap_or("")
}

fn opt(r: &Row, k: &str) -> Option<String> {
    r.get(k).cloned().flatten()
}

fn registry() -> Registry {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir().join("registry.tsv.gz")).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let products = tsv(&text)
        .iter()
        .map(|r| Candidate {
            id: get(r, "id").into(),
            name: get(r, "name").into(),
            alias: Some(get(r, "alias").to_string()).filter(|s| !s.is_empty()),
            brand: Some(get(r, "brand").to_string()).filter(|s| !s.is_empty()),
            score: 0.0,
        })
        .collect();
    Registry::from_products(products)
}

/// A fresh automatic match as a job makes one, optionally with one product taken out of the
/// candidates (the challenge: the true product missing). Returns the product it would write.
async fn fresh(reg: &Registry, name: &str, code: &str, without: Option<&str>) -> Option<Candidate> {
    let term = search_term(name, code);
    let mut scored = gather(reg, &term).await.unwrap();
    if let Some(id) = without {
        scored.retain(|s| s.candidate.id != id);
    }
    let j = judge_top(name, code, &scored, reg.vocabulary(), THRESHOLD)?;
    (verdict(Origin::Auto, &j, THRESHOLD) == Verdict::Write).then(|| scored[0].candidate.clone())
}

/// One pair judged three ways: the V4 gate uniqueness aside, the gate with the candidates a job
/// would gather, and as a seller's pick (Q-5: reopened by a conflict or an unbacked number).
struct Pair {
    passes_alone: bool,
    passes: bool,
    /// A number, an opposite word or a brand contradicts the pair.
    conflict: bool,
    /// The candidate's own name states a number of a kind the supplier states none of.
    unbacked: bool,
    pick_reopened: bool,
    /// What a seller's pick of it would be reopened for, as the screens name it.
    against: String,
}

async fn pair(reg: &Registry, name: &str, code: &str, c: &Candidate, scored: &[Scored]) -> Pair {
    let term = search_term(name, code);
    let sim = similarity_of(&term, c);
    let e = evidence(name, code, c, reg.vocabulary());
    let j = judge_link(name, code, c, sim, scored, reg.vocabulary(), THRESHOLD);
    let pick = Judged {
        similarity: sim,
        evidence: e.clone(),
        tied: false,
        another_passes: None,
    };
    let against = conflicts(&e);
    let unbacked = |r: &Reason| matches!(r, Reason::NumberUnbacked { .. });
    Pair {
        passes_alone: v4_reasons(&e, sim, THRESHOLD).is_empty(),
        passes: verdict(Origin::Auto, &j, THRESHOLD) == Verdict::Write,
        conflict: against.iter().any(|r| !unbacked(r)),
        unbacked: against.iter().any(unbacked),
        pick_reopened: verdict(Origin::Pick, &pick, THRESHOLD) != Verdict::Write,
        against: serde_json::to_string(&against).unwrap(),
    }
}

fn add(m: &mut BTreeMap<String, usize>, k: impl Into<String>, n: usize) {
    *m.entry(k.into()).or_default() += n;
}

/// Counts a reading under `<prefix>.right`, `.wrong`, `.duplicate` or `.unsure`; an unread case
/// is listed by [`Readings::of`] and fails the test.
fn tally(m: &mut BTreeMap<String, usize>, prefix: &str, seen: Option<Seen>) {
    let Some(seen) = seen else { return };
    let k = match seen {
        Seen::Right => "right",
        Seen::Wrong => "wrong",
        Seen::Duplicate => "duplicate",
        Seen::Unsure => "unsure",
    };
    add(m, format!("{prefix}.{k}"), 1);
}

/// One-sided 95 % Clopper–Pearson upper bound on a rate seen `k` times in `n`.
fn upper95(k: usize, n: usize) -> f64 {
    if n == 0 || k >= n {
        return 1.0;
    }
    let cdf = |p: f64| {
        let mut log_c = 0.0;
        let mut s = 0.0;
        for i in 0..=k {
            if i > 0 {
                log_c += ((n - i + 1) as f64).ln() - (i as f64).ln();
            }
            s += (log_c + i as f64 * p.ln() + (n - i) as f64 * (1.0 - p).ln()).exp();
        }
        s
    };
    let (mut lo, mut hi) = (k as f64 / n as f64, 1.0);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if cdf(mid) > 0.05 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

fn pct(k: usize, n: usize) -> String {
    format!(
        "{k}/{n} ({:.1} %, ≤ {:.2} %)",
        100.0 * k as f64 / n.max(1) as f64,
        100.0 * upper95(k, n)
    )
}

/// A case read by a person: is the candidate the part the supplier names?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Right,
    Wrong,
    /// A registry duplicate of the right part (PI-13): not a part error.
    Duplicate,
    Unsure,
}

fn reading(s: &str) -> Option<Seen> {
    match s {
        "SAME" | "RIGHT" => Some(Seen::Right),
        "DIFFERENT" | "WRONG" => Some(Seen::Wrong),
        "DUPLICATE" => Some(Seen::Duplicate),
        "UNSURE" => Some(Seen::Unsure),
        _ => None,
    }
}

struct Readings {
    by: HashMap<(String, String, String), Seen>,
    unread: Vec<String>,
}

impl Readings {
    fn load() -> Readings {
        let by = read("adjudicated.tsv")
            .iter()
            .map(|r| {
                let key = (
                    get(r, "set").into(),
                    get(r, "code").into(),
                    get(r, "candidate_id").into(),
                );
                let v = reading(get(r, "verdict"))
                    .unwrap_or_else(|| panic!("an unknown verdict in adjudicated.tsv: {r:?}"));
                (key, v)
            })
            .collect();
        Readings {
            by,
            unread: Vec::new(),
        }
    }

    /// The reading of one case, or a line asking for it.
    fn of(
        &mut self,
        set: &str,
        code: &str,
        name: &str,
        c: &Candidate,
        other: &str,
    ) -> Option<Seen> {
        let key = (set.to_string(), code.to_string(), c.id.clone());
        let found = self.by.get(&key).copied();
        if found.is_none() {
            self.unread.push(format!(
                "{set}\t{code}\t{}\t?\t{name} ⟶ {} | {} | {}{other}",
                c.id,
                c.name,
                c.brand.as_deref().unwrap_or(""),
                c.alias.as_deref().unwrap_or("")
            ));
        }
        found
    }
}

#[tokio::test]
#[ignore = "slow unoptimised: cargo test --release --test evidence_sets -- --ignored --nocapture"]
async fn the_evidence_sets_reproduce_the_recorded_table() {
    let reg = registry();
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    let mut readings = Readings::load();
    m.insert("registry".into(), reg.len());

    // --- the seller's codes: calibration, the challenge, and task 15 ------------------------------
    let items = read("seller-items.tsv");
    let links = read("seller-links.tsv");
    let db = Db::in_memory().unwrap();
    for l in &links {
        db.with(|c| {
            // no code has two links, so `match_for_code`'s order by decision time never decides
            c.execute(
                "INSERT INTO supplier_links (supplier, name_key, code, product_id, product_name,
                   product_brand, product_alias, how, name_seen, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '2026-01-01T00:00:00Z')",
                rusqlite::params![
                    SUPPLIER,
                    get(l, "name_key"),
                    get(l, "code"),
                    opt(l, "product_id").filter(|s| !s.is_empty()),
                    opt(l, "product_name"),
                    opt(l, "product_brand"),
                    opt(l, "product_alias"),
                    get(l, "how"),
                    get(l, "name_seen"),
                ],
            )
        })
        .unwrap();
    }
    let prover = Prover {
        db: &db,
        registry: &reg,
        supplier: SUPPLIER,
        job: 1,
        threshold: THRESHOLD,
    };
    m.insert("seller.items".into(), items.len());
    for it in &items {
        let (code, name) = (get(it, "code"), get(it, "name"));
        let before = prover.stored_link(name, code).unwrap();
        let how = before.as_ref().map(|l| l.how.clone()).unwrap_or_default();
        let judged = prover.item(name, code).await.unwrap();
        let after = prover.stored_link(name, code).unwrap();
        let outcome = match &judged {
            Judgement::Proven { .. } => "write",
            Judgement::NotInLandau => "none",
            Judgement::Held {
                error: Some(BridgeError::StaleLink { .. }),
                ..
            } => "stale",
            Judgement::Held {
                error: Some(BridgeError::LinkChanged),
                ..
            } => panic!("{code}: a link changed under a harness nothing else writes"),
            Judgement::Held { .. } => "review",
        };
        let origin = match how.as_str() {
            "USER" => "user",
            "AUTO" => "auto",
            "" => "fresh",
            "NONE" => "none",
            other => panic!("{code}: an unknown link origin {other}"),
        };
        add(&mut m, format!("seller.{origin}.{outcome}"), 1);
        if after.as_ref().is_some_and(|l| l.state == "REBUILT") {
            add(&mut m, "seller.rebuilt", 1);
        }

        // The challenge: these codes as new codes whose product is missing (Q-6's case). A stored
        // link that lost its product would be rebuilt or go stale instead, never matched afresh.
        let linked = before
            .as_ref()
            .filter(|l| l.how != "NONE" && l.product_brand.is_some() && l.product_alias.is_some());
        let resolved = after
            .as_ref()
            .and_then(|l| l.product_id.as_deref())
            .and_then(|id| reg.get(id));
        if let (Some(_), Some(product)) = (linked, resolved) {
            add(&mut m, "seller.removed.n", 1);
            if let Some(alt) = fresh(&reg, name, code, Some(&product.id)).await {
                add(&mut m, "seller.removed.write", 1);
                let note = format!(
                    "\t(linked: {} | {})",
                    product.name,
                    product.brand.as_deref().unwrap_or("")
                );
                let seen = readings.of("seller-removed", code, name, &alt, &note);
                tally(&mut m, "seller.removed.read", seen);
            }
        }

        // task 15: the export as a veto on the automatic writes this build makes
        let status = get(it, "export_status");
        add(&mut m, format!("seller.export.{status}"), 1);
        let export = (status == "resolved").then(|| get(it, "export_id"));
        if let (true, Judgement::Proven { product, .. }) =
            (origin == "auto" || origin == "fresh", &judged)
        {
            add(&mut m, "seller.auto_writes", 1);
            match export {
                // Q-6's exposure on this seller: a code the export gives no one product for, written
                None => {
                    add(&mut m, format!("seller.auto_writes.export_{status}"), 1);
                    let seen = readings.of("seller-export-absent", code, name, product, "");
                    tally(&mut m, "seller.auto_writes.unlabelled.read", seen);
                }
                Some(e) if e == product.id => add(&mut m, "seller.auto_writes.export_agrees", 1),
                Some(e) => {
                    add(&mut m, "seller.auto_writes.export_disagrees", 1);
                    let other = reg
                        .get(e)
                        .map(|p| {
                            format!(
                                "\t(export: {} | {})",
                                p.name,
                                p.brand.as_deref().unwrap_or("")
                            )
                        })
                        .unwrap_or_default();
                    let seen = readings.of("seller-export", code, name, product, &other);
                    tally(&mut m, "seller.auto_writes.disagreeing.read", seen);
                }
            }
        }
    }

    // --- validation: independent codes, labelled by the export ------------------------------------
    let validation = read("validation.tsv");
    m.insert("validation.n".into(), validation.len());
    for v in &validation {
        let (code, name, label_id) = (get(v, "code"), get(v, "name"), get(v, "label_id"));
        let label = reg.get(label_id).expect("a label in the snapshot").clone();
        let label_note = format!(
            "\t(label: {} | {})",
            label.name,
            label.brand.as_deref().unwrap_or("")
        );
        if let Some(w) = fresh(&reg, name, code, None).await {
            add(&mut m, "validation.write", 1);
            if w.id == label.id {
                add(&mut m, "validation.write.agree", 1);
            } else {
                add(&mut m, "validation.write.disagree", 1);
                let seen = readings.of("validation-write", code, name, &w, &label_note);
                tally(&mut m, "validation.write.read", seen);
            }
        }
        let p = pair(&reg, name, code, &label, &[]).await;
        if p.passes_alone {
            add(&mut m, "validation.label.passes_alone", 1);
        }
        if p.conflict {
            add(&mut m, "validation.label.conflict", 1);
            let why = format!("\t{}", p.against);
            let seen = readings.of("validation-conflict", code, name, &label, &why);
            tally(&mut m, "validation.label.read", seen);
        }
        if p.unbacked {
            add(&mut m, "validation.label.unbacked", 1);
        }
        if let Some(alt) = fresh(&reg, name, code, Some(label_id)).await {
            add(&mut m, "validation.removed.write", 1);
            let seen = readings.of("validation-removed", code, name, &alt, &label_note);
            tally(&mut m, "validation.removed.read", seen);
        }
    }

    // --- the hard cases: the user's reading where given, else the agent's ----------------------------
    let hard = read("hard-cases.tsv");
    m.insert("hard.pairs".into(), hard.len());
    let mut by_code: HashMap<&str, Vec<&Row>> = HashMap::new();
    for h in &hard {
        by_code.entry(get(h, "code")).or_default().push(h);
    }
    m.insert("hard.codes".into(), by_code.len());
    let label_of = |h: &Row, column: &str| {
        let v = get(h, column);
        (!v.is_empty()).then(|| {
            reading(v)
                .unwrap_or_else(|| panic!("{}: an unknown {column} label {v:?}", get(h, "id")))
        })
    };
    let mut agreement = (0usize, 0usize);
    let mut gathered: HashMap<String, Vec<Scored>> = HashMap::new();
    for h in &hard {
        let (code, name, category) = (get(h, "code"), get(h, "name"), get(h, "category"));
        let c = reg
            .get(get(h, "candidate_id"))
            .expect("a hard case in the snapshot")
            .clone();
        let scored = match gathered.get(code) {
            Some(s) => s.clone(),
            None => {
                let s = gather(&reg, &search_term(name, code)).await.unwrap();
                gathered.insert(code.to_string(), s.clone());
                s
            }
        };
        let p = pair(&reg, name, code, &c, &scored).await;
        let (agent, user) = (label_of(h, "agent"), label_of(h, "user"));
        if let (Some(a), Some(u)) = (agent, user) {
            agreement.1 += 1;
            if a == u {
                agreement.0 += 1;
            }
        }
        let Some(seen) = user.or(agent) else {
            readings.unread.push(format!(
                "hard\t{}\t{category}\t{name} ⟶ {} | {} | {}",
                get(h, "id"),
                c.name,
                c.brand.as_deref().unwrap_or(""),
                c.alias.as_deref().unwrap_or("")
            ));
            continue;
        };
        let label = match seen {
            Seen::Right => "same",
            Seen::Wrong => "different",
            Seen::Duplicate => "duplicate",
            Seen::Unsure => "unsure",
        };
        add(&mut m, format!("hard.{label}"), 1);
        add(&mut m, format!("hard.{category}.{label}"), 1);
        match seen {
            Seen::Right => {
                if p.conflict {
                    add(&mut m, "hard.same.conflict", 1);
                    add(&mut m, format!("hard.{category}.same.conflict"), 1);
                    println!(
                        "RIGHT PAIR IN CONFLICT: {} {name} ⟶ {} {}",
                        get(h, "id"),
                        c.name,
                        p.against
                    );
                }
                if p.unbacked {
                    add(&mut m, "hard.same.unbacked", 1);
                }
                if p.pick_reopened {
                    add(&mut m, "hard.same.pick_reopened", 1);
                }
                if p.passes_alone {
                    add(&mut m, "hard.same.passes_alone", 1);
                }
            }
            Seen::Wrong => {
                if p.passes_alone {
                    add(&mut m, "hard.different.passes_alone", 1);
                    add(&mut m, format!("hard.{category}.different.passes_alone"), 1);
                    println!(
                        "WRONG PART PASSES (uniqueness aside): {} {name} ⟶ {}",
                        get(h, "id"),
                        c.name
                    );
                }
                if p.passes {
                    add(&mut m, "hard.different.passes", 1);
                }
                if !p.pick_reopened {
                    add(&mut m, "hard.different.pick_kept", 1);
                }
            }
            Seen::Duplicate | Seen::Unsure => {}
        }
    }
    // What a job would write for each hard code: the listed pair's reading when it is one of them,
    // else a reading of its own, so a third product written wrongly cannot go unseen.
    let mut codes: Vec<&&str> = by_code.keys().collect();
    codes.sort();
    for code in codes {
        let pairs = &by_code[*code];
        let name = get(pairs[0], "name");
        let Some(w) = fresh(&reg, name, code, None).await else {
            continue;
        };
        add(&mut m, "hard.write", 1);
        let seen = match pairs.iter().find(|h| get(h, "candidate_id") == w.id) {
            Some(h) => label_of(h, "user").or(label_of(h, "agent")),
            None => readings.of("hard-write", code, name, &w, ""),
        };
        tally(&mut m, "hard.write.read", seen);
    }

    // --- the table ---------------------------------------------------------------------------------
    let n = |k: &str| m.get(k).copied().unwrap_or(0);
    let right_pairs =
        n("validation.n") - n("validation.label.read.wrong") - n("validation.label.read.unsure");
    let false_conflicts = n("validation.label.read.right") + n("validation.label.read.duplicate");
    let seller_wrong = n("seller.auto_writes.unlabelled.read.wrong")
        + n("seller.auto_writes.disagreeing.read.wrong");
    println!(
        "\nEvidence sets, registry of {} products, threshold floor 85",
        n("registry")
    );
    println!(
        "seller ({} codes): AUTO {} write · {} review · {} stale; USER {} write · {} review; unlinked {} write · {} review; {} rebuilt",
        n("seller.items"),
        n("seller.auto.write"),
        n("seller.auto.review"),
        n("seller.auto.stale"),
        n("seller.user.write"),
        n("seller.user.review"),
        n("seller.fresh.write"),
        n("seller.fresh.review"),
        n("seller.rebuilt"),
    );
    println!(
        "  wrong automatic writes on the seller's codes (read): {}",
        pct(seller_wrong, n("seller.auto_writes"))
    );
    println!(
        "  the {} linked codes as new codes whose product is missing (Q-6): written {}, read wrong {}",
        n("seller.removed.n"),
        pct(n("seller.removed.write"), n("seller.removed.n")),
        n("seller.removed.read.wrong"),
    );
    println!(
        "validation ({}): automatic {} · agree {} · disagree {} (read wrong: {})",
        n("validation.n"),
        n("validation.write"),
        n("validation.write.agree"),
        n("validation.write.disagree"),
        pct(n("validation.write.read.wrong"), n("validation.write")),
    );
    println!(
        "  the label's pair: conflict {} (false: {} of the right pairs; label-level {}; unsure {}), unbacked {}, passing uniqueness aside {}",
        n("validation.label.conflict"),
        pct(false_conflicts, right_pairs),
        n("validation.label.read.wrong"),
        n("validation.label.read.unsure"),
        n("validation.label.unbacked"),
        n("validation.label.passes_alone"),
    );
    println!(
        "  true product removed: written {}, read wrong {} ({}) · duplicate {} · right {}",
        n("validation.removed.write"),
        n("validation.removed.read.wrong"),
        pct(n("validation.removed.read.wrong"), n("validation.n")),
        n("validation.removed.read.duplicate"),
        n("validation.removed.read.right"),
    );
    println!(
        "hard cases ({} pairs, {} codes: {} same · {} different · {} duplicate · {} unsure): wrong passing {} uniqueness aside · {} with the candidates; right in conflict {}; right reopened as a pick {}; a job writes {} codes, read wrong {}",
        n("hard.pairs"),
        n("hard.codes"),
        n("hard.same"),
        n("hard.different"),
        n("hard.duplicate"),
        n("hard.unsure"),
        pct(n("hard.different.passes_alone"), n("hard.different")),
        n("hard.different.passes"),
        pct(n("hard.same.conflict"), n("hard.same")),
        n("hard.same.pick_reopened"),
        n("hard.write"),
        n("hard.write.read.wrong"),
    );
    let mut categories: Vec<&str> = hard.iter().map(|h| get(h, "category")).collect();
    categories.sort();
    categories.dedup();
    for c in categories {
        println!(
            "  {c}: {} same ({} in conflict) · {} different ({} passing) · {} unsure",
            n(&format!("hard.{c}.same")),
            n(&format!("hard.{c}.same.conflict")),
            n(&format!("hard.{c}.different")),
            n(&format!("hard.{c}.different.passes_alone")),
            n(&format!("hard.{c}.unsure")),
        );
    }
    println!(
        "  second reviewer: {}/{} pairs agree",
        agreement.0, agreement.1
    );
    println!(
        "task 15 (the export as a veto): export absent {} · ambiguous {} · unresolved {}; {} automatic writes: {} agree · {} disagree (V4 read wrong {}) · {} without an export label (read wrong {})",
        n("seller.export.absent"),
        n("seller.export.ambiguous"),
        n("seller.export.unresolved"),
        n("seller.auto_writes"),
        n("seller.auto_writes.export_agrees"),
        n("seller.auto_writes.export_disagrees"),
        n("seller.auto_writes.disagreeing.read.wrong"),
        n("seller.auto_writes") - n("seller.auto_writes.export_agrees") - n("seller.auto_writes.export_disagrees"),
        n("seller.auto_writes.unlabelled.read.wrong"),
    );

    if !readings.unread.is_empty() {
        println!("\nUNREAD ({}):", readings.unread.len());
        for u in &readings.unread {
            println!("{u}");
        }
        panic!("{} cases are not adjudicated yet", readings.unread.len());
    }

    // Task 5's acceptance, asserted on its own so re-recording the table cannot hide it: a worse
    // result goes back to the user.
    let accept = "task 5's acceptance fails: return the result to the user";
    assert_eq!(n("validation.write.read.wrong"), 0, "{accept}");
    assert_eq!(n("hard.different.passes_alone"), 0, "{accept}");
    assert_eq!(n("hard.different.passes"), 0, "{accept}");
    assert_eq!(n("hard.write.read.wrong"), 0, "{accept}");
    assert!(false_conflicts * 20 <= right_pairs, "{accept}");
    assert!(n("hard.same.conflict") * 20 <= n("hard.same"), "{accept}");

    let measured: Vec<(&str, usize)> = m.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    println!("\nmeasured: {measured:?}");
    let recorded: Vec<(&str, usize)> = RECORDED.to_vec();
    pretty_assertions::assert_eq!(
        recorded,
        measured,
        "record the table again only after measuring it"
    );
}

/// The table as recorded in Landau plan 130, execution slice 6 (registry of 2026-11-09).
const RECORDED: &[(&str, usize)] = &[
    ("hard.brand-script.different", 26),
    ("hard.brand-script.same", 26),
    ("hard.brand-spelling.different", 24),
    ("hard.brand-spelling.same", 26),
    ("hard.brand-spelling.same.conflict", 1),
    ("hard.brand-spelling.unsure", 2),
    ("hard.codes", 182),
    ("hard.different", 162),
    ("hard.different.pick_kept", 50),
    ("hard.dimensions.different", 26),
    ("hard.dimensions.same", 25),
    ("hard.dimensions.unsure", 1),
    ("hard.numberless.different", 26),
    ("hard.numberless.same", 26),
    ("hard.pairs", 364),
    ("hard.same", 179),
    ("hard.same-brand.different", 26),
    ("hard.same-brand.same", 25),
    ("hard.same-brand.unsure", 1),
    ("hard.same.conflict", 6),
    ("hard.same.passes_alone", 70),
    ("hard.same.pick_reopened", 10),
    ("hard.same.unbacked", 4),
    ("hard.sides.different", 26),
    ("hard.sides.same", 26),
    ("hard.two-brands.different", 8),
    ("hard.two-brands.same", 25),
    ("hard.two-brands.same.conflict", 5),
    ("hard.two-brands.unsure", 19),
    ("hard.unsure", 23),
    ("hard.write", 66),
    ("hard.write.read.right", 66),
    ("registry", 52899),
    ("seller.auto.review", 87),
    ("seller.auto.stale", 185),
    ("seller.auto.write", 557),
    ("seller.auto_writes", 557),
    ("seller.auto_writes.export_absent", 1),
    ("seller.auto_writes.export_agrees", 556),
    ("seller.auto_writes.unlabelled.read.wrong", 1),
    ("seller.export.absent", 23),
    ("seller.export.resolved", 1128),
    ("seller.fresh.review", 281),
    ("seller.items", 1151),
    ("seller.none.none", 1),
    ("seller.rebuilt", 264),
    ("seller.removed.n", 362),
    ("seller.user.review", 7),
    ("seller.user.write", 33),
    ("validation.label.conflict", 34),
    ("validation.label.passes_alone", 888),
    ("validation.label.read.right", 29),
    ("validation.label.read.unsure", 2),
    ("validation.label.read.wrong", 3),
    ("validation.label.unbacked", 8),
    ("validation.n", 2000),
    ("validation.removed.read.duplicate", 1),
    ("validation.removed.read.wrong", 3),
    ("validation.removed.write", 4),
    ("validation.write", 854),
    ("validation.write.agree", 854),
];
