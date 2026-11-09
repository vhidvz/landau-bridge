//! What the review and Matches screens show about a pick before it is made (plan 130 R-d, R-k): the
//! evidence and the write table's verdict for a seller's pick, judged against the kept registry
//! snapshot, and the money around it — Barezpakhsh's list price today, the cost the bridge would
//! write from it, and the row's cost and price as Landau holds them. Every figure is a read: PRICE
//! quotes again at commit, so nothing shown here is ever written.

use serde::Serialize;

use crate::domain::{cost_of, Authenticity};
use crate::errors::{BridgeError, Result};
use crate::jobs::todays_quote;
use crate::landau::{shelf_term, Candidate, LandauClient, Narrow, ShelfRow};
use crate::matching::evidence::{evidence, Evidence};
use crate::matching::gate::{verdict, Judged, Origin, Reason, Verdict};
use crate::matching::normalize::search_term;
use crate::matching::registry::Registry;
use crate::matching::similarity_of;
use crate::supplier::Supplier;

/// A seller's pick of `product` for the supplier item `name`/`code`, judged as the write table
/// judges a new pick (`Origin::NewPick`): its own evidence, never a tie or another candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PickJudgement {
    /// The supplier item judged — a pick checks it is the item it writes for.
    pub name: String,
    pub code: String,
    pub product: Candidate,
    pub similarity: u8,
    pub evidence: Evidence,
    pub verdict: Verdict,
}

pub fn judge_pick(
    registry: &Registry,
    name: &str,
    code: &str,
    product_id: &str,
    threshold: u8,
) -> Result<PickJudgement> {
    let product = registry
        .get(product_id)
        .cloned()
        .ok_or_else(|| BridgeError::Other(NOT_IN_SNAPSHOT.into()))?;
    let similarity = similarity_of(&search_term(name, code), &product);
    let judged = Judged {
        similarity,
        evidence: evidence(name, code, &product, registry.vocabulary()),
        tied: false,
        another_passes: None,
    };
    Ok(PickJudgement {
        name: name.to_string(),
        code: code.to_string(),
        verdict: verdict(Origin::NewPick, &judged, threshold),
        evidence: judged.evidence,
        similarity,
        product,
    })
}

/// A product the screen offers that the kept snapshot does not hold — it was removed from Landau
/// since, or the screen is older than the snapshot.
pub const NOT_IN_SNAPSHOT: &str =
    "این قطعه در فهرست لاندایی که برنامه خوانده است نیست؛ دوباره جست‌وجو کنید.";

/// The confirmation a pick needs (R-d), enforced here rather than by the screen: `Write` needs
/// none; a conflict needs exactly its reasons confirmed — what the seller was shown. Anything else
/// is refused with the reasons to show. Returns what the pick confirms, recorded with its link.
pub fn confirmation(verdict: &Verdict, confirmed: Option<&[Reason]>) -> Result<Vec<Reason>> {
    let reasons = match verdict {
        Verdict::Write => return Ok(Vec::new()),
        Verdict::ConfirmThenWrite(r) | Verdict::Review(r) => r,
    };
    match confirmed {
        Some(c) if same_reasons(c, reasons) => Ok(reasons.clone()),
        _ => Err(BridgeError::ConfirmRequired {
            reasons: serde_json::to_string(reasons)?,
        }),
    }
}

/// The same reasons, whatever their order.
pub fn same_reasons(a: &[Reason], b: &[Reason]) -> bool {
    a.len() == b.len() && a.iter().all(|r| b.contains(r)) && b.iter().all(|r| a.contains(r))
}

// ---------------------------------------------------------------------------------------------
// Money

/// A figure from a read that may have failed: the screen shows `Unknown` as unknown, never as zero.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Read<T> {
    Known(T),
    /// The seller's sentence for what failed.
    Unknown(String),
}

impl<T> Read<T> {
    pub(crate) fn of(r: Result<T>) -> Read<T> {
        match r {
            Ok(v) => Read::Known(v),
            Err(e) => Read::Unknown(e.message_fa()),
        }
    }
}

/// Barezpakhsh's row for the item today, and the cost the bridge would write from it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteView {
    pub code: String,
    pub name: String,
    /// Whole rials before VAT, as the site lists it.
    pub list_price: i64,
    pub has_vat: bool,
    /// The site's own rate, when it states one.
    pub site_vat: Option<f64>,
    /// The seller's VAT setting — the rate the cost is computed with.
    pub vat_setting: f64,
    /// `round(list × (1 + VAT))` by the bridge (`domain::cost_of`).
    pub cost: i64,
    pub available: bool,
    /// Found by the code under another name: the site renamed the item.
    pub renamed: bool,
    pub read_at: String,
}

pub async fn quote(
    supplier: &dyn Supplier,
    vat_setting: f64,
    name: &str,
    code: &str,
) -> Result<Option<QuoteView>> {
    let read_at = crate::db::now();
    let Some((q, renamed)) = todays_quote(supplier, name, code).await? else {
        return Ok(None);
    };
    let applied = if q.has_vat { vat_setting } else { 0.0 };
    Ok(Some(QuoteView {
        cost: cost_of(q.list_price, applied),
        code: q.code,
        name: q.name,
        list_price: q.list_price,
        has_vat: q.has_vat,
        site_vat: q.vat_percent,
        vat_setting,
        available: q.available,
        renamed,
        read_at,
    }))
}

/// The shelf row a pick would write: `(product, NEW, authenticity)` in the seller's store, as
/// Landau holds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShelfView {
    pub id: String,
    pub inventory: i64,
    pub cost: Option<i64>,
    pub price: Option<i64>,
    /// `MANUAL` | `RULE` | absent.
    pub price_source: Option<String>,
    pub read_at: String,
}

pub async fn shelf_row(
    landau: &LandauClient,
    store: &str,
    product: &Candidate,
    authenticity: Authenticity,
) -> Result<Option<ShelfView>> {
    let read_at = crate::db::now();
    let auth = authenticity.as_str();
    let is_tuple = |r: &ShelfRow| {
        r.product == product.id
            && r.condition.as_deref() == Some("NEW")
            && r.authenticity.as_deref() == Some(auth)
    };
    let row = landau
        .shelf_find(
            store,
            &Narrow {
                term: &shelf_term(&product.name),
                condition: Some("NEW"),
                authenticity: Some(auth),
            },
            &is_tuple,
        )
        .await?;
    Ok(row.map(|r| ShelfView {
        id: r.id,
        inventory: r.inventory,
        cost: r.cost,
        price: r.price,
        price_source: r.price_source,
        read_at,
    }))
}

/// One active pricing rule as Landau states it — the screens name it instead of computing a price.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleView {
    pub scope: Option<String>,
    pub basis: Option<String>,
    pub value: Option<f64>,
}

pub async fn active_rules(landau: &LandauClient) -> Result<Vec<RuleView>> {
    Ok(landau
        .pricing_rules()
        .await?
        .into_iter()
        .filter(|r| r.active)
        .map(|r| RuleView {
            scope: r.scope,
            basis: r.basis,
            value: r.value,
        })
        .collect())
}

/// One candidate as the screen shows it: the pick's judgement, its row, and the cost PRICE would
/// write on that row today.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateView {
    #[serde(flatten)]
    pub pick: PickJudgement,
    pub shelf: Read<Option<ShelfView>>,
    /// `pricing::effective_cost` of today's quote against the row: `None` while either is unknown.
    pub next_cost: Option<i64>,
    /// An unavailable quote below the row's cost: the row keeps its cost (§14 d).
    pub cost_kept: bool,
}

/// The cost PRICE would write from `quote` on `row` — the unavailable-only-raises rule included.
pub fn next_cost(
    quote: &Read<Option<QuoteView>>,
    row: &Read<Option<ShelfView>>,
) -> (Option<i64>, bool) {
    let (Read::Known(Some(q)), Read::Known(row)) = (quote, row) else {
        return (None, false);
    };
    let current = row.as_ref().and_then(|r| r.cost);
    let (cost, kept) = crate::jobs::pricing::effective_cost(q.cost, q.available, current);
    (Some(cost), kept)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inspection {
    /// When the snapshot every judgement here stands on was read.
    pub registry_read_at: String,
    /// `Known(None)`: the site lists neither the name nor the code today.
    pub quote: Read<Option<QuoteView>>,
    pub rules: Read<Vec<RuleView>>,
    pub candidates: Vec<CandidateView>,
    /// Offered ids the snapshot does not hold.
    pub missing: Vec<String>,
}

/// Everything one supplier item's screen shows for `product_ids` (its visible candidates).
#[allow(clippy::too_many_arguments)]
pub async fn inspect(
    registry: &Registry,
    registry_read_at: &str,
    supplier: Result<&dyn Supplier>,
    landau: &LandauClient,
    store: &str,
    vat_setting: f64,
    threshold: u8,
    item: (&str, &str),
    product_ids: &[String],
    authenticity: Authenticity,
) -> Inspection {
    let (name, code) = item;
    let mut candidates = Vec::new();
    let mut missing = Vec::new();
    for id in product_ids {
        match judge_pick(registry, name, code, id, threshold) {
            Ok(pick) => candidates.push(pick),
            Err(_) => missing.push(id.clone()),
        }
    }
    let quote = match supplier {
        Ok(s) => Read::of(quote(s, vat_setting, name, code).await),
        Err(e) => Read::Unknown(e.message_fa()),
    };
    let rules = Read::of(active_rules(landau).await);
    let mut views = Vec::with_capacity(candidates.len());
    for pick in candidates {
        let shelf = Read::of(shelf_row(landau, store, &pick.product, authenticity).await);
        let (next_cost, cost_kept) = next_cost(&quote, &shelf);
        views.push(CandidateView {
            pick,
            shelf,
            next_cost,
            cost_kept,
        });
    }
    Inspection {
        registry_read_at: registry_read_at.to_string(),
        quote,
        rules,
        candidates: views,
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::evidence::tests::{bearing_5e12, cand, RIGHT_BEARING, WRONG_BEARING};

    fn registry() -> Registry {
        let wrong = cand(
            "6aec987c0000000000000017",
            "بلبرینگ چرخ جلو قدیم پراید",
            Some("بلبرینگ 69349"),
            Some("PSN عظام"),
        );
        Registry::from_products(vec![bearing_5e12(), wrong])
    }

    #[test]
    fn a_pick_whose_numbers_conflict_needs_a_confirm() {
        let reg = registry();
        let (code, name) = RIGHT_BEARING;
        let wrong = judge_pick(&reg, name, code, "6aec987c0000000000000017", 85).unwrap();
        let Verdict::ConfirmThenWrite(r) = &wrong.verdict else {
            panic!("expected a confirm, got {:?}", wrong.verdict)
        };
        assert!(
            r.iter().any(|x| matches!(x, Reason::NumberConflict { .. })),
            "{r:?}"
        );
        let right = judge_pick(&reg, name, code, &bearing_5e12().id, 85).unwrap();
        assert_eq!(right.verdict, Verdict::Write);
    }

    #[test]
    fn a_product_the_snapshot_lacks_is_refused() {
        let (code, name) = WRONG_BEARING;
        let e = judge_pick(&registry(), name, code, "000000000000000000000000", 85).unwrap_err();
        assert_eq!(e, BridgeError::Other(NOT_IN_SNAPSHOT.into()));
    }

    /// Slice 4's review: the cost a pick would bring is PRICE's — an unavailable quote below the
    /// row's cost leaves the cost as it is, and an unknown side leaves it unknown.
    #[test]
    fn the_next_cost_is_what_price_would_write() {
        let q = |cost: i64, available: bool| {
            Read::Known(Some(QuoteView {
                code: "c".into(),
                name: "n".into(),
                list_price: cost,
                has_vat: false,
                site_vat: None,
                vat_setting: 10.0,
                cost,
                available,
                renamed: false,
                read_at: String::new(),
            }))
        };
        let row = |cost: i64| {
            Read::Known(Some(ShelfView {
                id: "s".into(),
                inventory: 0,
                cost: Some(cost),
                price: None,
                price_source: None,
                read_at: String::new(),
            }))
        };
        assert_eq!(
            next_cost(&q(900_000, false), &row(1_000_000)),
            (Some(1_000_000), true)
        );
        assert_eq!(
            next_cost(&q(900_000, true), &row(1_000_000)),
            (Some(900_000), false)
        );
        assert_eq!(
            next_cost(&q(1_100_000, false), &row(1_000_000)),
            (Some(1_100_000), false)
        );
        assert_eq!(
            next_cost(&q(900_000, false), &Read::Known(None)),
            (Some(900_000), false)
        );
        assert_eq!(
            next_cost(&q(900_000, true), &Read::Unknown("x".into())),
            (None, false)
        );
        assert_eq!(next_cost(&Read::Known(None), &row(1)), (None, false));
        assert_eq!(
            next_cost(&Read::Unknown("x".into()), &row(1)),
            (None, false)
        );
    }

    #[test]
    fn a_consistent_pick_needs_no_confirmation() {
        assert_eq!(confirmation(&Verdict::Write, None).unwrap(), Vec::new());
        assert_eq!(
            confirmation(&Verdict::Write, Some(&[Reason::Tied])).unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn a_conflict_is_refused_until_exactly_its_reasons_are_confirmed() {
        let a = Reason::NumberConflict {
            lacking: vec!["69349".into()],
        };
        let b = Reason::OppositeWords {
            supplier: "عقب".into(),
            candidate: "جلو".into(),
        };
        let v = Verdict::ConfirmThenWrite(vec![a.clone(), b.clone()]);
        let shown = serde_json::to_string(&[a.clone(), b.clone()]).unwrap();
        let refused = |c: Option<&[Reason]>| matches!(confirmation(&v, c), Err(BridgeError::ConfirmRequired { reasons }) if reasons == shown);
        assert!(refused(None), "no confirmation");
        assert!(refused(Some(std::slice::from_ref(&a))), "half of it");
        assert!(
            refused(Some(&[a.clone(), b.clone(), Reason::Tied])),
            "more than it"
        );
        let other = Reason::NumberConflict {
            lacking: vec!["44649".into()],
        };
        assert!(refused(Some(&[other, b.clone()])), "another value");
        assert_eq!(
            confirmation(&v, Some(&[b.clone(), a.clone()])).unwrap(),
            vec![a, b],
            "the same reasons in another order"
        );
    }
}
