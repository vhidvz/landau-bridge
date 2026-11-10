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

    /// A second code of the GSR clamp: the same part as `CLAMP_GSR`, its numbers the same.
    const GSR_TWIN: (&str, &str) = ("8800095", "بست کمري 4/8*380 GSR مشکي");

    /// A ZERO import whose invoice carries only `GSR_TWIN` (list 21,000 → cost 23,100), while the
    /// install also links `other` to the same clamp and the site prices it at `other_price`. The
    /// clamp's row is on the shelf at `CLAMP_GSR`'s 20,900.
    async fn one_code_of_two(
        other: (&str, &str),
        other_how: &str,
        other_price: i64,
    ) -> (FakeLandau, JobCtx) {
        one_code_of_two_named(other, other_how, other_price, other.1, false).await
    }

    /// The seller's word, through the core, that the site's `site_name` for `item`'s code is
    /// `product`: first refused, then sent again with exactly the reasons the refusal named.
    fn confirm_as_shown(db: &Db, item: (&str, &str), product: &str, site_name: &str) {
        let registry = Registry::from_products(registry());
        let version = db
            .match_for_name(SUPPLIER_ID, item.1)
            .unwrap()
            .unwrap()
            .link_version;
        let confirm = |c: Option<&[crate::matching::gate::Reason]>| {
            import::confirm_rename(
                db, &registry, item.1, version, product, item.0, site_name, c,
            )
        };
        let shown = match confirm(None) {
            Err(BridgeError::ConfirmRequired { reasons }) => {
                serde_json::from_str::<Vec<crate::matching::gate::Reason>>(&reasons).unwrap()
            }
            other => panic!("{other:?}"),
        };
        confirm(Some(&shown)).unwrap();
    }

    /// [`one_code_of_two`], the site listing the other code under `site_name` — a rename the
    /// seller confirmed beforehand when `confirmed`.
    async fn one_code_of_two_named(
        other: (&str, &str),
        other_how: &str,
        other_price: i64,
        site_name: &str,
        confirmed: bool,
    ) -> (FakeLandau, JobCtx) {
        let db = Db::in_memory().unwrap();
        link(&db, other, other_how, &clamp_57eb());
        link(&db, GSR_TWIN, "USER", &clamp_57eb());
        if confirmed {
            confirm_as_shown(&db, other, &clamp_57eb().id, site_name);
        }
        let landau = FakeLandau::start(
            STORE,
            registry(),
            vec![row(ROW_9CED, &clamp_57eb(), 20_900)],
        );
        let supplier = FakeSupplier::new(&[(GSR_TWIN.0, GSR_TWIN.1, 1, 21_000)]).price_only(
            other.0,
            site_name,
            other_price,
        );
        let svc = Services {
            landau: LandauClient::new(&landau.url, "apt-test").unwrap(),
            supplier: Box::new(supplier),
            settings: settings(&landau.url),
            snapshot: Default::default(),
        };
        let ctx = ctx(&db, "IMPORT");
        assert_eq!(
            import::start(&ctx, &svc, &zero()).await.unwrap(),
            Outcome::Done
        );
        ctx.finish(&Ok(()), "REPORT");
        (landau, ctx)
    }

    /// PI-24 (DoD 24's review, finding 3): a QUANTITY run holds the twin's line (another code
    /// prices its clamp differently) and ends `DONE`, its line unmarked (R-f). The seller resolves
    /// it — the site now prices both alike — and runs again with «از تاریخ» after the invoice, as
    /// the import page defaults it to the last run's finish. The held unit still reaches the shelf.
    #[tokio::test]
    async fn a_held_line_is_transferred_when_from_date_moves_past_it() {
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        link(&db, GSR_TWIN, "USER", &clamp_57eb());
        let landau = FakeLandau::start(
            STORE,
            registry(),
            vec![row(ROW_9CED, &clamp_57eb(), 20_900)],
        );
        let svc = |other_price: i64| Services {
            landau: LandauClient::new(&landau.url, "apt-test").unwrap(),
            supplier: Box::new(
                FakeSupplier::new(&[(GSR_TWIN.0, GSR_TWIN.1, 1, 21_000)]).price_only(
                    CLAMP_GSR.0,
                    CLAMP_GSR.1,
                    other_price,
                ),
            ),
            settings: settings(&landau.url),
            snapshot: Default::default(),
        };
        let quantity = |since: &str| ImportParams {
            mode: ImportMode::Quantity,
            since: Some(since.into()),
            fetch_since_only: false,
            fetch_limit: None,
        };
        let run = |since: &'static str, other_price: i64| {
            let db = db.clone();
            let svc = svc(other_price);
            async move {
                let job = db
                    .create_job("IMPORT", Some("QUANTITY"), Some(since), STORE, false)
                    .unwrap();
                let ctx = JobCtx {
                    db: db.clone(),
                    app: None,
                    job_id: job,
                    cancel: Default::default(),
                };
                let outcome = import::start(&ctx, &svc, &quantity(since)).await;
                ctx.finish(
                    &outcome.as_ref().map(|_| ()).map_err(Clone::clone),
                    "REPORT",
                );
                assert_eq!(outcome.unwrap(), Outcome::Done);
                ctx
            }
        };

        // the invoice is dated 1405/08/15; the first run holds the twin against the other code
        let first = run("1405/08/01", 19_000).await;
        assert_eq!(
            item(&first, GSR_TWIN.0).error_kind.as_deref(),
            Some("Conflict")
        );
        assert!(landau.writes().is_empty());
        assert!(
            db.imported_lines().unwrap().is_empty(),
            "R-f: the line unmarked"
        );

        // resolved; the next run starts after the invoice's date
        let second = run("1405/09/01", 21_000).await;
        let twin = item(&second, GSR_TWIN.0);
        assert_eq!((twin.qty, twin.status.as_str()), (1, "DONE"), "{twin:?}");
        let r = landau.row(ROW_9CED).unwrap();
        assert_eq!((r.inventory, r.cost), (1, Some(23_100)));
        assert_eq!(db.imported_lines().unwrap().len(), 1);
    }

    /// PI-26 (DoD 24's review, finding 5): the site no longer lists `CLAMP_GSR` under its invoice
    /// name; its code finds an item now named `site_name`, at 21,000 (23,100 with VAT). An import
    /// and a refresh each write that cost only when the new name does not contradict the clamp.
    async fn renamed_at_the_site(site_name: &str) -> (FakeLandau, JobCtx, JobCtx) {
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        let landau = FakeLandau::start(
            STORE,
            registry(),
            vec![row(ROW_9CED, &clamp_57eb(), 20_900)],
        );
        let (import, refresh) = import_and_refresh(&db, &landau, site_name).await;
        (landau, import, refresh)
    }

    /// One ZERO import, then one refresh, over `db` and `landau`, the site listing `CLAMP_GSR`'s
    /// code under `site_name` at 21,000.
    async fn import_and_refresh(db: &Db, landau: &FakeLandau, site_name: &str) -> (JobCtx, JobCtx) {
        let svc = Services {
            landau: LandauClient::new(&landau.url, "apt-test").unwrap(),
            supplier: Box::new(
                FakeSupplier::new(&[(CLAMP_GSR.0, CLAMP_GSR.1, 1, 21_000)])
                    .renamed(CLAMP_GSR.0, site_name),
            ),
            settings: settings(&landau.url),
            snapshot: Default::default(),
        };
        let import = ctx(db, "IMPORT");
        assert_eq!(
            import::start(&import, &svc, &zero()).await.unwrap(),
            Outcome::Done
        );
        import.finish(&Ok(()), "REPORT");
        let refresh = ctx(db, "REFRESH");
        super::super::refresh::run(&refresh, &svc).await.unwrap();
        refresh.finish(&Ok(()), "REPORT");
        (import, refresh)
    }

    /// PI-28: the hold PI-26 puts on a renamed code is recorded on its link for the seller, and
    /// the seller's word that the new name is the clamp — confirmed through the core, which
    /// refuses it until exactly the conflicts shown come back — lets the cost through in an
    /// import and a refresh, under that name alone. A further rename asks again.
    #[tokio::test]
    async fn a_rename_the_seller_confirmed_writes_and_only_under_that_name() {
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        let landau = FakeLandau::start(
            STORE,
            registry(),
            vec![row(ROW_9CED, &clamp_57eb(), 20_900)],
        );
        let clamp = clamp_57eb().id;
        let site = "بست کمري 300*3/6 GSR سفيد";
        let link_now = || {
            db.match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
                .unwrap()
                .unwrap()
        };

        let (import, _) = import_and_refresh(&db, &landau, site).await;
        assert!(landau.writes().is_empty(), "{:?}", landau.writes());
        let seen = link_now()
            .renamed()
            .expect("the hold is recorded on the link");
        assert_eq!(
            (
                seen.product_id.as_str(),
                seen.code.as_str(),
                seen.name.as_str()
            ),
            (clamp.as_str(), CLAMP_GSR.0, site)
        );
        assert!(seen.confirmed_at.is_none() && !seen.reasons.is_empty());
        assert_eq!(db.count_links_needing_review(SUPPLIER_ID).unwrap(), 1);
        assert!(
            item(&import, CLAMP_GSR.0)
                .note
                .unwrap_or_default()
                .contains("تطبیق‌ها"),
            "the report says where to confirm"
        );

        // the core refuses an unconfirmed or partial word, then takes exactly the reasons shown
        let registry = Registry::from_products(registry());
        let l = link_now();
        let confirm = |confirmed: Option<&[crate::matching::gate::Reason]>| {
            import::confirm_rename(
                &db,
                &registry,
                CLAMP_GSR.1,
                l.link_version,
                &clamp,
                CLAMP_GSR.0,
                site,
                confirmed,
            )
        };
        let shown = match confirm(None) {
            Err(BridgeError::ConfirmRequired { reasons }) => {
                serde_json::from_str::<Vec<crate::matching::gate::Reason>>(&reasons).unwrap()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(shown, seen.reasons);
        assert!(matches!(
            confirm(Some(&[])),
            Err(BridgeError::ConfirmRequired { .. })
        ));
        confirm(Some(&shown)).unwrap();
        assert_eq!(db.count_links_needing_review(SUPPLIER_ID).unwrap(), 0);

        let (import, refresh) = import_and_refresh(&db, &landau, site).await;
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(23_100));
        let written = item(&import, CLAMP_GSR.0);
        assert_eq!(written.status, "DONE", "{written:?}");
        assert!(written
            .note
            .unwrap_or_default()
            .contains("فروشنده تأیید کرده"));
        assert!(
            ctx_items(&refresh).iter().all(|i| i.error_kind.is_none()),
            "{:?}",
            ctx_items(&refresh)
        );

        // the site renames the code again, the same conflict under another name: held in both,
        // asked again — a confirmation counts for the name it was given for
        let writes = landau.writes().len();
        let again = "بست کمري 300*3/6 GSR سفيد براق";
        let (import, refresh) = import_and_refresh(&db, &landau, again).await;
        assert_eq!(landau.writes().len(), writes, "{:?}", landau.writes());
        assert_eq!(
            item(&import, CLAMP_GSR.0).error_kind.as_deref(),
            Some("EvidenceConflict")
        );
        assert_eq!(
            ctx_items(&refresh)[0].error_kind.as_deref(),
            Some("EvidenceConflict")
        );
        let pending = link_now().renamed().unwrap();
        assert_eq!((pending.name.as_str(), pending.confirmed_at), (again, None));

        // the site lists the invoice's name again: the waiting rename is forgotten
        let (_, _) = import_and_refresh(&db, &landau, CLAMP_GSR.1).await;
        assert_eq!(link_now().renamed(), None);
        assert_eq!(db.count_links_needing_review(SUPPLIER_ID).unwrap(), 0);
    }

    #[tokio::test]
    async fn a_code_given_to_another_part_never_writes_its_cost() {
        // 30 cm × 3.6 white where the clamp is 38 cm × 4.8 black: the numbers conflict
        let (landau, import, refresh) = renamed_at_the_site("بست کمري 300*3/6 GSR سفيد").await;
        assert!(landau.writes().is_empty(), "{:?}", landau.writes());
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(20_900));
        assert_eq!(
            item(&import, CLAMP_GSR.0).error_kind.as_deref(),
            Some("EvidenceConflict")
        );
        let held = ctx_items(&refresh);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].error_kind.as_deref(), Some("EvidenceConflict"));

        // the same part under a corrected spelling: written as before (the rename noted)
        let (landau, import, _) = renamed_at_the_site("بست کمری 380*4/8 GSR مشکی").await;
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(23_100));
        assert_eq!(item(&import, CLAMP_GSR.0).status, "DONE");
    }

    /// PI-28: the core takes a seller's word on a rename only for the link as the screen read it,
    /// a product the kept snapshot holds, and exactly the conflicts it judges itself; a name that
    /// agrees needs no word and forgets a record still waiting.
    #[test]
    fn a_rename_is_confirmed_only_as_the_core_judges_it() {
        use crate::matching::gate::Reason;
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        let registry = Registry::from_products(registry());
        let clamp = clamp_57eb().id;
        let site = "بست کمري 300*3/6 GSR سفيد";
        let now = || {
            db.match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
                .unwrap()
                .unwrap()
        };
        let v = now().link_version;
        let confirm = |version: i64, product: &str, name: &str, c: Option<&[Reason]>| {
            import::confirm_rename(
                &db,
                &registry,
                CLAMP_GSR.1,
                version,
                product,
                CLAMP_GSR.0,
                name,
                c,
            )
        };
        let refused =
            |r: crate::errors::Result<bool>| matches!(r, Err(BridgeError::ConfirmRequired { .. }));
        let other = |r: crate::errors::Result<bool>, text: &str| matches!(r, Err(BridgeError::Other(m)) if m == text);
        let shown = match confirm(v, &clamp, site, None) {
            Err(BridgeError::ConfirmRequired { reasons }) => {
                serde_json::from_str::<Vec<Reason>>(&reasons).unwrap()
            }
            r => panic!("{r:?}"),
        };
        assert!(
            shown
                .iter()
                .any(|r| matches!(r, Reason::NumberConflict { .. })),
            "{shown:?}"
        );

        // not what was shown: another reason, one more, or none
        assert!(refused(confirm(
            v,
            &clamp,
            site,
            Some(&[Reason::NoBrandInName])
        )));
        let more = [shown.clone(), vec![Reason::NoBrandInName]].concat();
        assert!(refused(confirm(v, &clamp, site, Some(&more))));
        assert!(refused(confirm(v, &clamp, site, Some(&[]))));
        // the link as the screen read it, onto the product it points at
        assert!(other(
            confirm(v + 1, &clamp, site, Some(&shown)),
            import::LINK_MOVED
        ));
        assert!(other(
            confirm(v, &clamp_57a5().id, site, Some(&shown)),
            import::LINK_MOVED
        ));
        assert_eq!(now().renamed(), None, "nothing stored on a refusal");
        // a product the snapshot lacks, and a link with no product
        let gone = dead(clamp_57a5());
        link(&db, CLAMP_5, "USER", &gone);
        let v5 = db
            .match_for_name(SUPPLIER_ID, CLAMP_5.1)
            .unwrap()
            .unwrap()
            .link_version;
        let r = import::confirm_rename(
            &db,
            &registry,
            CLAMP_5.1,
            v5,
            &gone.id,
            CLAMP_5.0,
            site,
            Some(&shown),
        );
        assert!(other(r, crate::inspect::NOT_IN_SNAPSHOT));
        db.set_match(SUPPLIER_ID, CLAMP_8.1, CLAMP_8.0, None, "NONE", None)
            .unwrap();
        let v8 = db
            .match_for_name(SUPPLIER_ID, CLAMP_8.1)
            .unwrap()
            .unwrap()
            .link_version;
        let r = import::confirm_rename(
            &db,
            &registry,
            CLAMP_8.1,
            v8,
            &clamp,
            CLAMP_8.0,
            site,
            Some(&shown),
        );
        assert!(other(r, import::LINK_MOVED));

        // a waiting record binds the word to its own code and name: another is refused
        assert!(db
            .note_rename(
                SUPPLIER_ID,
                &now().name_key,
                v,
                &clamp,
                None,
                CLAMP_GSR.0,
                site,
                &shown
            )
            .unwrap());
        let agrees = "بست کمری 380*4/8 GSR مشکی";
        assert!(other(confirm(v, &clamp, agrees, None), import::LINK_MOVED));
        // a recorded name that agrees with the product now (the registry changed since): no word
        // is needed, nothing is confirmed, and the waiting record is forgotten
        assert!(db.withdraw_rename(SUPPLIER_ID, &now().name_key, v).unwrap());
        assert!(db
            .note_rename(
                SUPPLIER_ID,
                &now().name_key,
                v,
                &clamp,
                None,
                CLAMP_GSR.0,
                agrees,
                &shown
            )
            .unwrap());
        assert!(!confirm(v, &clamp, agrees, None).unwrap());
        assert_eq!(now().renamed(), None);

        // exactly what was shown, in any order: stored
        let reversed: Vec<Reason> = shown.iter().rev().cloned().collect();
        confirm(v, &clamp, site, Some(&reversed)).unwrap();
        let stored = now().renamed().unwrap();
        assert!(stored.confirmed_at.is_some());
        assert!(stored.covers(&clamp, CLAMP_GSR.0, site, &shown));
    }

    /// Slice 9's review, findings 1, 4 and 7: a rename judged from the job's read never undoes a
    /// confirmation the seller made since; a waiting record is forgotten only by a quote of its own
    /// code; a product the registry no longer holds is held without asking the seller anything.
    #[test]
    fn a_rename_is_judged_from_the_job_read_without_undoing_the_seller() {
        use super::super::{judge_rename, Costed, Rename};
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        let registry = Registry::from_products(registry());
        let clamp = clamp_57eb().id;
        let site = "بست کمري 300*3/6 GSR سفيد";
        let quote = |code: &str, name: &str, renamed: bool| Costed {
            cost: 23_100,
            available: true,
            code: code.into(),
            name: name.into(),
            renamed,
        };
        let read = || {
            db.match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
                .unwrap()
                .unwrap()
        };
        let held = |r: Rename| matches!(r, Rename::Holds { .. });
        let asks = |r: &Rename| {
            matches!(
                r,
                Rename::Holds {
                    asks_seller: true,
                    ..
                }
            )
        };

        let first = judge_rename(
            &db,
            &registry,
            Some(&read()),
            &clamp,
            &quote(CLAMP_GSR.0, site, true),
        )
        .unwrap();
        assert!(asks(&first), "{first:?}");
        let stale = read();
        confirm_as_shown(&db, CLAMP_GSR, &clamp, site);
        // a refresh reaching the row on its read from before the confirmation: held this run,
        // the seller not asked again, the confirmation kept
        let r = judge_rename(
            &db,
            &registry,
            Some(&stale),
            &clamp,
            &quote(CLAMP_GSR.0, site, true),
        )
        .unwrap();
        assert!(held(r.clone()) && !asks(&r), "{r:?}");
        assert!(read().renamed().unwrap().confirmed_at.is_some());

        // a waiting record is forgotten only by a quote of its own code
        assert!(db
            .withdraw_rename(SUPPLIER_ID, &read().name_key, read().link_version)
            .unwrap());
        judge_rename(
            &db,
            &registry,
            Some(&read()),
            &clamp,
            &quote(CLAMP_GSR.0, site, true),
        )
        .unwrap();
        assert!(read().renamed().is_some_and(|r| r.confirmed_at.is_none()));
        let other_code = judge_rename(
            &db,
            &registry,
            Some(&read()),
            &clamp,
            &quote("8800999", CLAMP_GSR.1, false),
        )
        .unwrap();
        assert_eq!(other_code, Rename::Writes);
        assert!(
            read().renamed().is_some(),
            "another code's quote says nothing of this one"
        );
        judge_rename(
            &db,
            &registry,
            Some(&read()),
            &clamp,
            &quote(CLAMP_GSR.0, CLAMP_GSR.1, false),
        )
        .unwrap();
        assert_eq!(
            read().renamed(),
            None,
            "the site lists the code under the invoice's name again"
        );

        // a product the registry no longer holds: held, and nothing waits for the seller
        let gone = dead(clamp_57a5());
        link(&db, CLAMP_5, "USER", &gone);
        let l5 = || db.match_for_name(SUPPLIER_ID, CLAMP_5.1).unwrap().unwrap();
        let r = judge_rename(
            &db,
            &registry,
            Some(&l5()),
            &gone.id,
            &quote(CLAMP_5.0, site, true),
        )
        .unwrap();
        assert!(held(r.clone()) && !asks(&r), "{r:?}");
        assert_eq!(l5().renamed(), None);
    }

    /// Slice 9's review, finding 3: PRICE judges a rename on the link the item was planned on —
    /// none when the seller re-pointed it since, so nothing is noted on their new decision.
    #[test]
    fn an_item_is_judged_on_the_link_it_was_planned_on() {
        let db = Db::in_memory().unwrap();
        link(&db, CLAMP_GSR, "USER", &clamp_57eb());
        let l = db
            .match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
            .unwrap()
            .unwrap();
        let ctx = ctx(&db, "IMPORT");
        let id = db
            .add_job_item(ctx.job_id, CLAMP_GSR.0, CLAMP_GSR.1, 0)
            .unwrap();
        let plan = |version: i64, product: &str| {
            db.update_job_item(
                id,
                &[
                    ("link_key", crate::jobs::t(&l.name_key)),
                    ("link_version", crate::jobs::i(version)),
                    ("product_id", crate::jobs::t(product)),
                ],
            )
            .unwrap();
            db.job_item(id).unwrap().unwrap()
        };
        let clamp = clamp_57eb().id;
        assert!(import::item_link(&ctx, &plan(l.link_version, &clamp))
            .unwrap()
            .is_some());
        assert!(import::item_link(&ctx, &plan(l.link_version - 1, &clamp))
            .unwrap()
            .is_none());
        assert!(
            import::item_link(&ctx, &plan(l.link_version, &clamp_57a5().id))
                .unwrap()
                .is_none()
        );
    }

    fn ctx_items(ctx: &JobCtx) -> Vec<JobItemRow> {
        ctx.db.job_items(ctx.job_id).unwrap()
    }

    /// PI-22 (DoD 24's review, finding 1): Q-3 holds a row that two codes with different costs
    /// feed, and a job whose invoices carry only one of them must hold it too — as REFRESH does —
    /// or the row's cost follows whichever code was invoiced last. Equal costs write; another link
    /// on the product that does not prove holds the row whatever the costs.
    #[tokio::test]
    async fn a_code_the_job_lacks_still_holds_its_row() {
        // the other code proves and its cost differs: held, the shelf untouched
        let (landau, ctx) = one_code_of_two(CLAMP_GSR, "USER", 19_000).await;
        assert!(landau.writes().is_empty(), "{:?}", landau.writes());
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(20_900));
        let twin = item(&ctx, GSR_TWIN.0);
        assert_eq!(
            (twin.status.as_str(), twin.error_kind.as_deref()),
            ("SKIPPED", Some("Conflict")),
            "held, not failed"
        );
        let other = ctx
            .db
            .match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
            .unwrap()
            .unwrap();
        assert_eq!(
            (other.state.as_str(), other.validated_job),
            (state::VALID, Some(ctx.job_id)),
            "the other code was judged by this job"
        );

        // the other code agrees: one write at the shared cost, both links proven for it
        let (landau, ctx) = one_code_of_two(CLAMP_GSR, "USER", 21_000).await;
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(23_100));
        assert_eq!(item(&ctx, GSR_TWIN.0).status, "DONE");

        // another link on the product that does not prove: held, even at an equal cost
        let (landau, ctx) = one_code_of_two(CLAMP_11, "AUTO", 21_000).await;
        assert!(landau.writes().is_empty(), "{:?}", landau.writes());
        assert_eq!(
            item(&ctx, GSR_TWIN.0).error_kind.as_deref(),
            Some("Conflict")
        );

        // PI-26 on the other link: its code now names another part, at an equal cost — held
        let renamed = "بست کمري 300*3/6 GSR سفيد";
        let (landau, ctx) = one_code_of_two_named(CLAMP_GSR, "USER", 21_000, renamed, false).await;
        assert!(landau.writes().is_empty(), "{:?}", landau.writes());
        assert_eq!(
            item(&ctx, GSR_TWIN.0).error_kind.as_deref(),
            Some("Conflict")
        );
        assert!(
            ctx.db
                .match_for_name(SUPPLIER_ID, CLAMP_GSR.1)
                .unwrap()
                .unwrap()
                .renamed()
                .is_some_and(|r| r.confirmed_at.is_none()),
            "the rename waits for the seller on the other link"
        );

        // PI-28: the seller confirmed that rename — the other link counts at its equal cost
        let (landau, ctx) = one_code_of_two_named(CLAMP_GSR, "USER", 21_000, renamed, true).await;
        assert_eq!(landau.row(ROW_9CED).unwrap().cost, Some(23_100));
        assert_eq!(item(&ctx, GSR_TWIN.0).status, "DONE");
    }
}
