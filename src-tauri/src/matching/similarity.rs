//! fuzzywuzzy's `token_set_ratio`, the family the previous scraper matched with (spec §6).

use std::collections::BTreeSet;

use crate::matching::normalize::key_text;

/// 0–100 similarity of two strings after [`key_text`] normalisation.
pub fn ratio(a: &str, b: &str) -> u8 {
    (strsim::normalized_levenshtein(a, b) * 100.0).round() as u8
}

/// Split both into token sets; compare the intersection against each side and the two sides
/// against each other; keep the best. Insensitive to order and to extra words on one side.
pub fn token_set_ratio(a: &str, b: &str) -> u8 {
    let ka = key_text(a);
    let kb = key_text(b);
    let ta: BTreeSet<&str> = ka.split(' ').filter(|t| !t.is_empty()).collect();
    let tb: BTreeSet<&str> = kb.split(' ').filter(|t| !t.is_empty()).collect();
    if ta.is_empty() || tb.is_empty() {
        return 0;
    }
    let inter: Vec<&str> = ta.intersection(&tb).copied().collect();
    let only_a: Vec<&str> = ta.difference(&tb).copied().collect();
    let only_b: Vec<&str> = tb.difference(&ta).copied().collect();
    let t0 = inter.join(" ");
    let t1 = format!("{t0} {}", only_a.join(" ")).trim().to_string();
    let t2 = format!("{t0} {}", only_b.join(" ")).trim().to_string();
    [ratio(&t0, &t1), ratio(&t0, &t2), ratio(&t1, &t2)]
        .into_iter()
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_100_and_disjoint_is_low() {
        assert_eq!(token_set_ratio("لنت جلو پژو 405", "لنت جلو پژو 405"), 100);
        assert!(token_set_ratio("لنت جلو پژو 405", "شمع موتور پراید") < 40);
    }

    #[test]
    fn word_order_and_extra_words_matter_little() {
        assert_eq!(token_set_ratio("پژو 405 لنت جلو", "لنت جلو پژو 405"), 100);
        assert!(token_set_ratio("لنت جلو پژو 405 عظام", "لنت جلو پژو 405") >= 85);
    }

    #[test]
    fn typos_cost_a_few_points() {
        let s = token_set_ratio("سیبک فرمان پراید امیرنیا", "سیبک فرمان پراید امیرنا");
        assert!((85..100).contains(&s), "{s}");
    }
}
