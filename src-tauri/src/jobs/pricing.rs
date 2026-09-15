//! Pure pricing rules (spec §14 b, d, e): which search row is "the" quote, when an unavailable
//! quote is applied, the provisional attach price and the margin audit.

use crate::domain::PriceQuote;
use crate::matching::normalize::name_key;

/// Landau's rounding step and ours.
pub const ROUNDING: i64 = 1000;

/// The row whose normalised name equals `name`'s; several → available first, then the highest
/// list price (the reference scraper keeps the maximum price per name).
pub fn pick_by_name(name: &str, quotes: Vec<PriceQuote>) -> Option<PriceQuote> {
    let key = name_key(name);
    let mut same: Vec<PriceQuote> = quotes
        .into_iter()
        .filter(|q| name_key(&q.name) == key)
        .collect();
    same.sort_by(|a, b| {
        b.available
            .cmp(&a.available)
            .then(b.list_price.cmp(&a.list_price))
    });
    same.into_iter().next()
}

/// `(cost to write, held)`: an unavailable quote that would lower a row's current cost is held
/// back — it is stale and will be relisted higher (§14 d).
pub fn effective_cost(quote_cost: i64, available: bool, current: Option<i64>) -> (i64, bool) {
    match current {
        Some(c) if !available && quote_cost < c => (c, true),
        _ => (quote_cost, false),
    }
}

/// The provisional price a new row is created with: cost plus the minimum margin, rounded up to
/// [`ROUNDING`]. Landau's rule overrides it on the write (§14 e, f).
pub fn floor_price(cost: i64, margin_percent: f64) -> i64 {
    let raw = cost as f64 * (1.0 + margin_percent / 100.0);
    (raw / ROUNDING as f64).ceil() as i64 * ROUNDING
}

/// Whether a final price leaves less than the minimum margin, allowing half a rounding step for
/// Landau rounding to nearest rather than up.
pub fn below_min_margin(price: i64, cost: i64, margin_percent: f64) -> bool {
    (price as f64) + (ROUNDING as f64) / 2.0 < cost as f64 * (1.0 + margin_percent / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(name: &str, code: &str, price: i64, available: bool) -> PriceQuote {
        PriceQuote {
            code: code.into(),
            name: name.into(),
            list_price: price,
            vat_percent: Some(10.0),
            has_vat: true,
            available,
            unit: "عدد".into(),
        }
    }

    #[test]
    fn picks_the_equal_name_available_first_then_the_highest_price() {
        let quotes = vec![
            q("لنت جلو پژو 405 عظام* (16)", "A", 900, false),
            q("لنت جلو پژو 405 عظام* (16) طرح جدید", "B", 999, true),
            q("لنت جلو پژو ۴۰۵ عظام* (16)", "C", 800, true),
            q("لنت جلو پژو 405 عظام* (16)", "D", 850, true),
        ];
        assert_eq!(
            pick_by_name("لنت جلو پژو 405 عظام* (16)", quotes.clone())
                .unwrap()
                .code,
            "D"
        );
        assert!(pick_by_name("چیز دیگر", quotes).is_none());
    }

    #[test]
    fn an_unavailable_quote_only_raises_the_cost() {
        assert_eq!(effective_cost(900, false, Some(1000)), (1000, true));
        assert_eq!(effective_cost(1100, false, Some(1000)), (1100, false));
        assert_eq!(effective_cost(900, true, Some(1000)), (900, false));
        assert_eq!(effective_cost(900, false, None), (900, false));
    }

    #[test]
    fn the_provisional_price_is_the_cost_plus_the_margin_rounded_up_to_a_thousand() {
        assert_eq!(floor_price(1_000_000, 15.0), 1_150_000);
        assert_eq!(floor_price(5_370_200, 15.0), 6_176_000);
        assert_eq!(floor_price(10_000, 15.0), 12_000);
        assert_eq!(floor_price(0, 15.0), 0);
    }

    #[test]
    fn below_min_margin_tolerates_half_a_rounding_step() {
        assert!(!below_min_margin(1_150_000, 1_000_000, 15.0));
        assert!(below_min_margin(1_149_000, 1_000_000, 15.0));
        assert!(below_min_margin(1_000_000, 1_000_000, 15.0));
        assert!(
            !below_min_margin(11_000, 10_000, 15.0),
            "a 500-rial shortfall is rounding, not a thin margin"
        );
        assert!(
            !below_min_margin(1_150_000, 1_000_300, 15.0),
            "Landau rounds 1 150 345 to nearest, not up"
        );
        assert!(
            below_min_margin(3_669_000, 3_191_100, 15.0),
            "765 rials under the margin is more than a rounding step down"
        );
    }
}
