//! The same normalisation the registry was built with (spec §6), so a search term and a
//! candidate name are compared on equal footing.

use regex::Regex;
use std::sync::LazyLock;

use crate::domain::latin_digits;

static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static PUNCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[()\[\]{}\-_.,،;:؛"'«»/\\*+|!?؟#@&%^~`<>]+"#).unwrap());
/// «(10)», «( 10 )», «(10 عددی)», «(10عددي)», «(10 عدد)», «(10 تایی)», «(2 pcs)»
static PACK_PAREN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(\s*(\d{1,4})\s*(?:عددی|عدد|تایی|تا|pcs?|PCS?)?\s*\)").unwrap());
/// bare «10 عددی» / «10عددی» — the character before the digits is kept (`$1`)
static PACK_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^\d])\d{1,4}\s*عددی").unwrap());

/// Unify Arabic/Persian glyphs, digits, ZWNJ and whitespace. Keeps punctuation.
pub fn normalize_text(s: &str) -> String {
    let mapped: String = s
        .chars()
        .filter_map(|c| match c {
            'ي' => Some('ی'),
            'ك' => Some('ک'),
            'ة' | 'ۀ' => Some('ه'),
            'أ' | 'إ' => Some('ا'),
            'ؤ' => Some('و'),
            '\u{0654}' | '\u{FEFF}' | '\u{200E}' | '\u{200F}' => None, // hamza above, BOM, LRM, RLM
            '\u{200C}' | '\t' => Some(' '),                            // ZWNJ, tab
            '٫' => Some('.'),
            '٬' => Some(','),
            '×' => Some('*'),
            other => Some(other),
        })
        .collect();
    WS.replace_all(latin_digits(&mapped).trim(), " ")
        .to_string()
}

/// Lower-cased, punctuation-free, whitespace-collapsed — the key a similarity is measured on.
pub fn key_text(s: &str) -> String {
    let lowered = normalize_text(s).to_lowercase();
    WS.replace_all(PUNCT.replace_all(&lowered, " ").trim(), " ")
        .to_string()
}

/// The invoice name as a search term: normalised, pack markers and a trailing SKU removed.
pub fn search_term(name: &str, code: &str) -> String {
    let mut s = normalize_text(name);
    let code = normalize_text(code);
    if code.len() >= 4 {
        if let Some(stripped) = s.strip_suffix(code.as_str()) {
            s = stripped.trim_end_matches([' ', '-', '_', '/']).to_string();
        }
    }
    let s = PACK_PAREN.replace_all(&s, " ");
    let s = PACK_BARE.replace_all(&s, "$1 ");
    WS.replace_all(s.trim(), " ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unifies_arabic_letters_digits_and_zwnj() {
        assert_eq!(
            normalize_text("لنت‌ترمز  جلو پرايد ۴"),
            "لنت ترمز جلو پراید 4"
        );
        assert_eq!(normalize_text("كاسه\tنمد"), "کاسه نمد");
    }

    #[test]
    fn key_text_drops_punctuation_and_case() {
        assert_eq!(
            key_text("سیبک فرمان (0631)(48) ABS"),
            "سیبک فرمان 0631 48 abs"
        );
    }

    #[test]
    fn search_term_strips_pack_and_box_markers_and_a_trailing_code() {
        // «(0631)», «(48)», «(16)» are box counts / catalogue numbers the registry names never carry
        assert_eq!(
            search_term("سيبک فرمان پرايد اميرنيا (0631)(48) (2عددي)", "1032001010"),
            "سیبک فرمان پراید امیرنیا"
        );
        assert_eq!(
            search_term("لنت جلو پژو 405 عظام* (16)", "3137006"),
            "لنت جلو پژو 405 عظام*"
        );
        assert_eq!(
            search_term("بلبرینگ چرخ عقب پراید 3154028", "3154028"),
            "بلبرینگ چرخ عقب پراید"
        );
        assert_eq!(
            search_term("قرقري فرمان پژو 405 اميرنيا <2>(0421)(40)", "1023002010"),
            "قرقری فرمان پژو 405 امیرنیا <2>"
        );
        assert_eq!(search_term("واشر 10 عددی سرسیلندر", "1"), "واشر سرسیلندر");
        assert_eq!(
            search_term("لنت (چپ) پراید", "2"),
            "لنت (چپ) پراید",
            "a non-numeric group stays"
        );
    }
}
