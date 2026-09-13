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

/// Token-set Jaccard (0–100): |A ∩ B| / |A ∪ B| over the `key_text` tokens of both strings.
/// Unlike a set *ratio*, a candidate whose words are merely a subset of the term's scores below 100,
/// so a shorter variant of the right part cannot tie with it.
pub fn token_jaccard(a: &str, b: &str) -> u8 {
    let ka = key_text(a);
    let kb = key_text(b);
    let ta: BTreeSet<&str> = ka.split(' ').filter(|t| !t.is_empty()).collect();
    let tb: BTreeSet<&str> = kb.split(' ').filter(|t| !t.is_empty()).collect();
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0;
    }
    let inter = ta.intersection(&tb).count();
    ((inter as f64 / union as f64) * 100.0).round() as u8
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

    #[test]
    fn jaccard_identical_is_100() {
        assert_eq!(
            token_jaccard("خطر عقب راست پژو 206 SD", "خطر عقب راست پژو 206 SD"),
            100
        );
    }

    #[test]
    fn jaccard_a_subset_scores_below_100() {
        assert_eq!(
            token_jaccard("خطر عقب راست پژو 206 SD", "خطر عقب راست پژو 206"),
            83
        );
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        assert_eq!(token_jaccard("لنت جلو پژو 405", "شمع موتور پراید"), 0);
    }

    #[test]
    fn jaccard_is_order_insensitive() {
        assert_eq!(token_jaccard("پژو 405 لنت جلو", "لنت جلو پژو 405"), 100);
    }
}
