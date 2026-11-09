//! Plan 130 task 12: the 2026-11-06 incident as a regression fixture, run end to end offline —
//! a stateful Landau ([`crate::landau::fake`]) and Barezpakhsh ([`crate::supplier::fake`]) behind
//! the real client, the jobs' own phases, the real writer.
//!
//! Two states of the seller's install, both after Landau's 2026-11-05 re-import gave every product
//! a new id:
//! - [`before_job_12`]: the links as they stood when job 12 started (the 2026-10-07 picks, now on
//!   dead ids), the store's shelf empty. Replaying job 12 on this build — the hurried picks
//!   included — must write no wrong cost.
//! - [`after_job_12`]: what job 12 left under v1.1.1 — the six rows of the plan's table on the shelf,
//!   two of them at a wrong cost, and the job's own records. Every later job must hold those two
//!   rows; the audit (task 9) must list them and nothing else.

use std::sync::Arc;

use crate::db::matches::{actor, state, LinkProduct, LinkPut};
use crate::db::Db;
use crate::domain::{Authenticity, ImportMode};
use crate::landau::fake::{FakeLandau, Row};
use crate::landau::{Candidate, LandauClient};
use crate::matching::evidence::tests::{
    bearing_5e12, bearing_6c04, cand, clamp_576d, clamp_57a5, clamp_57ac, clamp_57eb, CLAMP_11,
    CLAMP_4, MID_SHAFT, RIGHT_BEARING, WRONG_BEARING,
};
use crate::matching::registry::Registry;
use crate::settings::{Settings, SUPPLIER_ID};
use crate::supplier::fake::FakeSupplier;

use super::import::{self, ImportParams, Outcome};
use super::{JobCtx, Services};

/// «فروشگاه وکیلی», the seller's store.
pub(crate) const STORE: &str = "6aecbde9d00e9931421f9a53";

/// The four supplier items whose rows were right, as the seller's database holds them.
pub(crate) const CLAMP_5: (&str, &str) = ("AR1330-5", "بست کمري 370 7/6 تايوان آراد پارت (100)");
pub(crate) const CLAMP_8: (&str, &str) = ("AR1330-8", "بست کمري CW7.6*37 آراد پارت");
pub(crate) const CLAMP_GSR: (&str, &str) = ("8800094", "بست کمري 380*4/8 GSR مشکي");

/// The six shelf rows, by the last four hex digits the plan names them by.
pub(crate) const ROW_9D1E: &str = "6aecc0a1d00e9931421f9d1e";
pub(crate) const ROW_9CF0: &str = "6aecc0a1d00e9931421f9cf0";
pub(crate) const ROW_9CEA: &str = "6aecc0a1d00e9931421f9cea";
pub(crate) const ROW_9CED: &str = "6aecc0a1d00e9931421f9ced";
pub(crate) const ROW_9CF7: &str = "6aecc0a1d00e9931421f9cf7";
pub(crate) const ROW_9D12: &str = "6aecc0a1d00e9931421f9d12";

/// The bearing the hurried pick should have been: the catalogue's 69349 front bearing.
pub(crate) fn bearing_5e00() -> Candidate {
    cand(
        "6aec987cd00e9931421a5e00",
        "بلبرینگ چرخ جلو قدیم پراید",
        Some("بلبرینگ 69349"),
        Some("عظام"),
    )
}

/// A product as it was before the re-import: the same definition under its old id.
fn dead(c: Candidate) -> Candidate {
    Candidate {
        id: format!("6ac52427{}", &c.id[8..]),
        ..c
    }
}

/// Barezpakhsh's list prices (before VAT) — each cost in the plan is this × 1.10.
pub(crate) const PRICES: [((&str, &str), i64); 8] = [
    (RIGHT_BEARING, 5_631_000), // 6,194,100
    (WRONG_BEARING, 5_000_000), // 5,500,000
    (CLAMP_11, 83_000),         // 91,300
    (CLAMP_4, 88_000),          // 96,800
    (CLAMP_5, 150_000),         // 165,000
    (CLAMP_GSR, 19_000),        // 20,900
    (CLAMP_8, 164_000),         // 180,400
    (MID_SHAFT, 4_770_000),     // 5,247,000
];

/// Landau's registry after the re-import: the incident's products, their siblings, and enough
/// names to give the evidence its vocabulary.
pub(crate) fn registry() -> Vec<Candidate> {
    let mut products = vec![
        bearing_5e12(),
        bearing_5e00(),
        clamp_576d(),
        clamp_57a5(),
        clamp_57ac(),
        clamp_57eb(),
        bearing_6c04(),
        cand("dpi", "لنت ترمز جلو پراید", None, Some("DPI")),
        cand("azam-pad", "لنت ترمز جلو پراید", None, Some("عظام")),
        cand("ns", "شمع موتور", None, Some("بوش")),
    ];
    for (n, w) in [
        "بوش طبق پژو 405",
        "بست تایوان",
        "پیچ تایوان",
        "مهره تایوان",
        "بلبرینگ چرخ عقب پراید",
        "بلبرینگ چرخ جلو پیکان",
        "پلوس پژو 405",
        "بلبرینگ وسط پلوس پراید",
        "بست کمری نایلونی سفید",
        "پمپ بنزین پراید",
    ]
    .iter()
    .enumerate()
    {
        products.push(cand(&format!("f{n}"), w, None, None));
    }
    products
}

pub(crate) fn supplier() -> FakeSupplier {
    let items: Vec<(&str, &str, i64, i64)> = PRICES
        .iter()
        .map(|((code, name), price)| (*code, *name, 1, *price))
        .collect();
    FakeSupplier::new(&items)
}

/// The install's settings as the seller has them: threshold 70 (the gate reads it as 85).
pub(crate) fn settings(url: &str) -> Settings {
    Settings {
        landau_url: url.into(),
        landau_token: Some("apt-test".into()),
        store: Some(STORE.into()),
        store_name: Some("فروشگاه وکیلی".into()),
        supplier_user: Some("u".into()),
        supplier_password: Some("p".into()),
        default_authenticity: Authenticity::Original,
        threshold: 70,
        vat_percent: 10.0,
        min_margin_percent: 0.0,
        overwrite_manual: false,
        fetch_since_only: true,
        auto_update: true,
    }
}

pub(crate) fn services(landau: &FakeLandau) -> Services {
    Services {
        landau: LandauClient::new(&landau.url, "apt-test").unwrap(),
        supplier: Box::new(supplier()),
        settings: settings(&landau.url),
        snapshot: Default::default(),
    }
}

/// A link as v1.1.1 stored it, migrated by 0004: `UNVERIFIED`, with the definition recorded.
pub(crate) fn link(db: &Db, item: (&str, &str), how: &str, product: &Candidate) {
    db.put_link(
        SUPPLIER_ID,
        &LinkPut {
            name_seen: item.1,
            code: item.0,
            product: Some(LinkProduct {
                id: &product.id,
                name: &product.name,
                brand: Some(product.brand.as_deref().unwrap_or("")),
                alias: Some(product.alias.as_deref().unwrap_or("")),
            }),
            how,
            similarity: Some(if how == "AUTO" { 72 } else { 56 }),
            state: state::UNVERIFIED,
            state_reason: None,
            validated_job: None,
            actor: actor::IMPORT,
            evidence: None,
            job_id: None,
            decided_at: Some("2026-10-07T16:30:00Z".into()),
            priced_at: None,
            confirmed: None,
        },
    )
    .unwrap();
}

/// The seller's install when job 12 started: the 2026-10-07 picks, each product since given a new
/// id by the re-import; the wrong bearing and both clamps not yet linked; the store's shelf empty.
pub(crate) fn before_job_12(db: &Db) -> FakeLandau {
    link(db, RIGHT_BEARING, "USER", &dead(bearing_5e12()));
    link(db, CLAMP_5, "USER", &dead(clamp_57a5()));
    link(db, CLAMP_8, "USER", &dead(clamp_57ac()));
    link(db, CLAMP_GSR, "USER", &dead(clamp_57eb()));
    link(db, MID_SHAFT, "USER", &dead(bearing_6c04()));
    FakeLandau::start(STORE, registry(), Vec::new())
}

fn row(id: &str, product: &Candidate, cost: i64) -> Row {
    Row {
        id: id.into(),
        product: product.id.clone(),
        inventory: 0,
        cost: Some(cost),
        price: Some(crate::landau::fake::rule_price(cost)),
        authenticity: "ORIGINAL".into(),
        deleted: false,
        updated: 1,
    }
}

/// What job 12 left under v1.1.1 (the plan's *The six rows of sup01*), on this build's schema: the
/// six rows on the shelf — `…9d1e` at the hurried pick's 5,500,000, `…9cf0` at the second clamp's
/// 96,800 over the first one's 91,300 — the links as the job left them, and the job's own items,
/// which record who wrote each row.
pub(crate) fn after_job_12(db: &Db) -> FakeLandau {
    link(db, RIGHT_BEARING, "USER", &dead(bearing_5e12()));
    link(db, WRONG_BEARING, "USER", &bearing_5e12());
    link(db, CLAMP_11, "AUTO", &clamp_576d());
    link(db, CLAMP_4, "AUTO", &clamp_576d());
    link(db, CLAMP_5, "USER", &clamp_57a5());
    link(db, CLAMP_8, "USER", &clamp_57ac());
    link(db, CLAMP_GSR, "USER", &clamp_57eb());
    link(db, MID_SHAFT, "USER", &bearing_6c04());

    let job = db
        .create_job("IMPORT", Some("ZERO"), None, STORE, false)
        .unwrap();
    let wrote = |item: (&str, &str), product: &Candidate, stock: &str, cost: i64, verb: &str| {
        let id = db.add_job_item(job, item.0, item.1, 0).unwrap();
        db.update_job_item(
            id,
            &[
                ("product_id", super::t(&product.id)),
                ("product_name", super::t(&product.name)),
                ("stock_id", super::t(stock)),
                ("cost", super::i(cost)),
                ("action", super::t(verb)),
                ("status", super::t("DONE")),
            ],
        )
        .unwrap();
    };
    let failed = db
        .add_job_item(job, RIGHT_BEARING.0, RIGHT_BEARING.1, 0)
        .unwrap();
    db.update_job_item(
        failed,
        &[
            ("status", super::t("FAILED")),
            ("error_kind", super::t("LandauRejected")),
        ],
    )
    .unwrap();
    wrote(
        WRONG_BEARING,
        &bearing_5e12(),
        ROW_9D1E,
        5_500_000,
        "ATTACH",
    );
    wrote(CLAMP_11, &clamp_576d(), ROW_9CF0, 91_300, "ATTACH");
    wrote(CLAMP_4, &clamp_576d(), ROW_9CF0, 96_800, "PATCH");
    wrote(CLAMP_5, &clamp_57a5(), ROW_9CEA, 165_000, "ATTACH");
    wrote(CLAMP_GSR, &clamp_57eb(), ROW_9CED, 20_900, "ATTACH");
    wrote(CLAMP_8, &clamp_57ac(), ROW_9CF7, 180_400, "ATTACH");
    wrote(MID_SHAFT, &bearing_6c04(), ROW_9D12, 5_247_000, "ATTACH");
    db.set_job_status(job, "DONE", "REPORT").unwrap();

    FakeLandau::start(
        STORE,
        registry(),
        vec![
            row(ROW_9D1E, &bearing_5e12(), 5_500_000),
            row(ROW_9CF0, &clamp_576d(), 96_800),
            row(ROW_9CEA, &clamp_57a5(), 165_000),
            row(ROW_9CED, &clamp_57eb(), 20_900),
            row(ROW_9CF7, &clamp_57ac(), 180_400),
            row(ROW_9D12, &bearing_6c04(), 5_247_000),
        ],
    )
}

pub(crate) fn ctx(db: &Db, kind: &str) -> JobCtx {
    let mode = (kind == "IMPORT").then_some("ZERO");
    let job_id = db.create_job(kind, mode, None, STORE, false).unwrap();
    JobCtx {
        db: db.clone(),
        app: None,
        job_id,
        cancel: Default::default(),
    }
}

pub(crate) fn zero() -> ImportParams {
    ImportParams {
        mode: ImportMode::Zero,
        since: None,
        fetch_since_only: false,
        fetch_limit: None,
    }
}

/// The job's kept registry snapshot — what the review screen judges a pick against.
pub(crate) fn kept(svc: &Services) -> Arc<Registry> {
    svc.snapshot
        .latest()
        .expect("the job kept its read")
        .registry
}

/// The seller's pick in review of `product` for `item`, judged by the core as `review_decide` does;
/// with `hurried`, the seller confirms whatever the second confirm shows.
pub(crate) fn pick(
    ctx: &JobCtx,
    svc: &Services,
    item: (&str, &str),
    product: &Candidate,
    hurried: bool,
) -> crate::errors::Result<()> {
    let row = ctx
        .db
        .job_items(ctx.job_id)?
        .into_iter()
        .find(|i| i.code == item.0)
        .expect("the item is in the job");
    let judged = crate::inspect::judge_pick(
        &kept(svc),
        &row.name,
        &row.code,
        &product.id,
        svc.settings.threshold,
    )?;
    let shown = match &judged.verdict {
        crate::matching::gate::Verdict::Write => None,
        crate::matching::gate::Verdict::ConfirmThenWrite(r)
        | crate::matching::gate::Verdict::Review(r) => Some(r.clone()),
    };
    let confirmed = if hurried { shown } else { None };
    import::pick(&ctx.db, &row, &judged, confirmed.as_deref(), None)
}

pub(crate) async fn finish_import(ctx: &JobCtx, svc: &Services) {
    let outcome = import::commit(ctx, svc).await;
    ctx.finish(&outcome, "REPORT");
    outcome.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::jobs::JobItemRow;
    use crate::errors::BridgeError;

    fn item(ctx: &JobCtx, code: &str) -> JobItemRow {
        ctx.db
            .job_items(ctx.job_id)
            .unwrap()
            .into_iter()
            .find(|i| i.code == code)
            .unwrap()
    }

    /// Every write the fake received names one of these products, with the cost the plan says is
    /// right for it — the regression bar of task 12: no wrong cost.
    fn writes_only(landau: &FakeLandau, expected: &[(&Candidate, i64)]) {
        for w in landau.writes() {
            let product = w.json()["product"]
                .as_str()
                .map(str::to_string)
                .or_else(|| {
                    let id = w.path.rsplit('/').next().unwrap_or("");
                    landau.row(id).map(|r| r.product)
                });
            let product = product.unwrap_or_default();
            let cost = w.json()["cost"].as_i64();
            let ok = expected
                .iter()
                .any(|(c, right)| c.id == product && cost.is_none_or(|v| v == *right));
            assert!(
                ok,
                "a write the incident must never make: {} {}",
                w.line(),
                w.body
            );
        }
    }

    /// Task 12, first half: job 12 replayed on this build. VALIDATE rebuilds the right bearing's
    /// 2026-10-07 pick onto `…5e12`; the hurried seller picks `…5e12` for the 69349 bearing and
    /// `…576d` for both clamps, and confirms every conflict the second confirm shows. Neither the
    /// bearing row nor the clamp row is written: each holds two links with two costs. The four
    /// consistent parts are written at their own cost.
    #[tokio::test]
    async fn job_12_replayed_writes_no_wrong_cost_however_hurried_the_picks() {
        let db = Db::in_memory().unwrap();
        let landau = before_job_12(&db);
        let svc = services(&landau);
        let ctx = ctx(&db, "IMPORT");
        let outcome = import::start(&ctx, &svc, &zero()).await.unwrap();
        assert_eq!(
            outcome,
            Outcome::Review(3),
            "the 69349 bearing and both clamps"
        );

        let right = db
            .match_for_name(SUPPLIER_ID, RIGHT_BEARING.1)
            .unwrap()
            .unwrap();
        assert_eq!(
            (right.state.as_str(), right.product_id.as_deref()),
            (state::REBUILT, Some(bearing_5e12().id.as_str())),
            "rebuilt onto …5e12"
        );
        assert_eq!(right.prev_product_id, Some(dead(bearing_5e12()).id));
        let last = db
            .match_events(SUPPLIER_ID, &right.name_key)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(last.evidence.as_deref(), Some("reconstructed"));

        // the hurried pick: refused without the second confirm, written with it
        let e = pick(&ctx, &svc, WRONG_BEARING, &bearing_5e12(), false).unwrap_err();
        assert!(matches!(e, BridgeError::ConfirmRequired { .. }), "{e:?}");
        pick(&ctx, &svc, WRONG_BEARING, &bearing_5e12(), true).unwrap();
        for clamp in [CLAMP_11, CLAMP_4] {
            let e = pick(&ctx, &svc, clamp, &clamp_576d(), false).unwrap_err();
            assert!(matches!(e, BridgeError::ConfirmRequired { .. }), "{e:?}");
            pick(&ctx, &svc, clamp, &clamp_576d(), true).unwrap();
        }
        finish_import(&ctx, &svc).await;

        writes_only(
            &landau,
            &[
                (&clamp_57a5(), 165_000),
                (&clamp_57eb(), 20_900),
                (&clamp_57ac(), 180_400),
                (&bearing_6c04(), 5_247_000),
            ],
        );
        assert_eq!(
            landau.writes().len(),
            4,
            "the four consistent rows, once each"
        );
        assert!(
            landau.row_of(&bearing_5e12().id).is_none(),
            "no bearing row"
        );
        assert!(landau.row_of(&clamp_576d().id).is_none(), "no clamp row");
        for code in [RIGHT_BEARING.0, WRONG_BEARING.0, CLAMP_11.0, CLAMP_4.0] {
            let i = item(&ctx, code);
            assert_eq!(
                (i.status.as_str(), i.error_kind.as_deref()),
                ("SKIPPED", Some("Conflict")),
                "{code} held, not failed"
            );
        }
        for (part, cost) in [
            (clamp_57a5(), 165_000),
            (clamp_57eb(), 20_900),
            (clamp_57ac(), 180_400),
            (bearing_6c04(), 5_247_000),
        ] {
            assert_eq!(
                landau.row_of(&part.id).unwrap().cost,
                Some(cost),
                "{}",
                part.name
            );
        }
        let job = db.job(ctx.job_id).unwrap().unwrap();
        assert_eq!(job.status, "DONE");
        let summary = job.summary.unwrap();
        assert_eq!(
            (summary["held"].as_i64(), summary["attached"].as_i64()),
            (Some(4), Some(4))
        );
    }

    /// Task 12, second half: held there until resolved. The seller re-points the 69349 bearing at
    /// its own product in Matches (consistent: no confirm); the next import writes the right cost
    /// on the big bearing's row and the 69349 bearing on its own, while the clamps — still two
    /// links with two costs on one product — keep their row held.
    #[tokio::test]
    async fn the_bearing_row_is_held_until_the_seller_resolves_it() {
        let db = Db::in_memory().unwrap();
        let landau = before_job_12(&db);
        let svc = services(&landau);
        let ctx1 = ctx(&db, "IMPORT");
        import::start(&ctx1, &svc, &zero()).await.unwrap();
        pick(&ctx1, &svc, WRONG_BEARING, &bearing_5e12(), true).unwrap();
        for clamp in [CLAMP_11, CLAMP_4] {
            pick(&ctx1, &svc, clamp, &clamp_576d(), true).unwrap();
        }
        finish_import(&ctx1, &svc).await;

        // Matches: the seller resolves the bearing
        let judged = crate::inspect::judge_pick(
            &kept(&svc),
            WRONG_BEARING.1,
            WRONG_BEARING.0,
            &bearing_5e00().id,
            70,
        )
        .unwrap();
        import::matches_pick(&db, WRONG_BEARING.1, WRONG_BEARING.0, &judged, None).unwrap();

        landau.forget_requests();
        let ctx2 = ctx(&db, "IMPORT");
        assert_eq!(
            import::start(&ctx2, &svc, &zero()).await.unwrap(),
            Outcome::Done,
            "nothing left to ask: the clamps' confirmed picks stand"
        );
        assert_eq!(
            landau.row_of(&bearing_5e12().id).unwrap().cost,
            Some(6_194_100),
            "the big bearing's row at its own cost"
        );
        assert_eq!(
            landau.row_of(&bearing_5e00().id).unwrap().cost,
            Some(5_500_000),
            "the 69349 bearing on its own row"
        );
        assert!(
            landau.row_of(&clamp_576d().id).is_none(),
            "the clamps still held"
        );
        for code in [CLAMP_11.0, CLAMP_4.0] {
            assert_eq!(item(&ctx2, code).error_kind.as_deref(), Some("Conflict"));
        }
    }

    /// The damage v1.1.1 left on the shelf is never confirmed by a later job (fact 6): neither an
    /// import nor a refresh writes `…9d1e` or `…9cf0`. The bearing row's links are the right
    /// bearing's — rebuilt and proven — and the hurried pick, which conflicts; the clamp row's are
    /// two automatic links that both conflict. The four consistent rows are written as before.
    #[tokio::test]
    async fn the_damaged_rows_are_held_by_every_later_job() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let svc = services(&landau);

        let refresh = ctx(&db, "REFRESH");
        let outcome = super::super::refresh::run(&refresh, &svc).await;
        refresh.finish(&outcome, "REPORT");
        outcome.unwrap();
        let untouched = |id: &str| {
            assert!(
                !landau.writes().iter().any(|w| w.path.contains(id)),
                "{id} written"
            )
        };
        untouched(ROW_9D1E);
        untouched(ROW_9CF0);
        let held: Vec<String> = db
            .job_items(refresh.job_id)
            .unwrap()
            .into_iter()
            .filter(|i| i.status == "SKIPPED")
            .filter(|i| {
                i.error_kind
                    .as_deref()
                    .is_some_and(|k| crate::errors::IDENTITY_KINDS.contains(&k))
            })
            .filter_map(|i| i.stock_id)
            .collect();
        assert_eq!(held.len(), 2, "{held:?}");
        assert!(held.contains(&ROW_9D1E.to_string()) && held.contains(&ROW_9CF0.to_string()));

        landau.forget_requests();
        let import = ctx(&db, "IMPORT");
        assert_eq!(
            import::start(&import, &svc, &zero()).await.unwrap(),
            Outcome::Review(3)
        );
        finish_import(&import, &svc).await;
        untouched(ROW_9D1E);
        untouched(ROW_9CF0);
        writes_only(
            &landau,
            &[
                (&clamp_57a5(), 165_000),
                (&clamp_57eb(), 20_900),
                (&clamp_57ac(), 180_400),
                (&bearing_6c04(), 5_247_000),
            ],
        );
        assert_eq!(landau.row(ROW_9D1E).unwrap().cost, Some(5_500_000));
        assert_eq!(landau.row(ROW_9CF0).unwrap().cost, Some(96_800));
        assert_eq!(
            item(&import, RIGHT_BEARING.0).error_kind.as_deref(),
            Some("Conflict"),
            "the right bearing held against the hurried pick's link"
        );
    }
}
