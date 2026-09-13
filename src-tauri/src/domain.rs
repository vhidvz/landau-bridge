//! Value types shared by the supplier, Landau, matching and job layers (spec §2, §4).

use serde::{Deserialize, Serialize};

/// A ledger row kind we act on. Receipts («دريافت») are not invoices and are dropped at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InvoiceKind {
    /// «فاکتور فروش» — adds quantity.
    Sale,
    /// «مرجوع از فروش» — subtracts quantity.
    Return,
}

impl InvoiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InvoiceKind::Sale => "SALE",
            InvoiceKind::Return => "RETURN",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "SALE" => Some(InvoiceKind::Sale),
            "RETURN" => Some(InvoiceKind::Return),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceHeader {
    pub number: String,
    pub kind: InvoiceKind,
    /// `YYYY/MM/DD`, zero-padded (see [`normalize_jalali`]) so string comparison orders dates.
    pub date_jalali: String,
    /// `YYYY-MM-DD` from the ledger's `VoucherDateOld`.
    pub date_iso: String,
    pub paid: bool,
    /// Site-relative, e.g. `/Shop/Preview/Factor/8818975/11000`.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub line_no: u32,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub unit: String,
    /// Whole rials, before VAT, as printed on the invoice.
    pub unit_price: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceQuote {
    pub code: String,
    pub name: String,
    /// The «فی» column (`FinalPrice`), whole rials before VAT.
    pub list_price: i64,
    /// `TaxAndTollPercent` as the site states it, e.g. `10.0`.
    pub vat_percent: Option<f64>,
    pub has_vat: bool,
    pub available: bool,
    pub unit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Authenticity {
    Original,
    Fake,
}

impl Authenticity {
    pub fn as_str(self) -> &'static str {
        match self {
            Authenticity::Original => "ORIGINAL",
            Authenticity::Fake => "FAKE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ORIGINAL" => Some(Authenticity::Original),
            "FAKE" => Some(Authenticity::Fake),
            _ => None,
        }
    }
}

/// The per-import choice of spec §7.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ImportMode {
    /// Transfer invoice quantities from invoices dated on/after `since`.
    Quantity,
    /// Every item at quantity 0 for review.
    Zero,
}

impl ImportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportMode::Quantity => "QUANTITY",
            ImportMode::Zero => "ZERO",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "QUANTITY" => Some(ImportMode::Quantity),
            "ZERO" => Some(ImportMode::Zero),
            _ => None,
        }
    }
}

/// `round(list_price × (1 + vat/100))` in whole rials — the decision of spec §1 — computed exactly:
/// the rate is taken to two decimals (basis points) and the division rounds half up, so no
/// floating-point representation can move a `.5` boundary.
pub fn cost_of(list_price: i64, vat_percent: f64) -> i64 {
    let rate_bp = (vat_percent * 100.0).round() as i64; // 10.0 % → 1000 bp
    let scaled = list_price as i128 * (10_000 + rate_bp as i128);
    ((scaled + 5_000) / 10_000) as i64
}

/// Persian/Arabic digits → ASCII.
pub fn latin_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            '٠'..='٩' => char::from(b'0' + (c as u32 - '٠' as u32) as u8),
            other => other,
        })
        .collect()
}

/// `1401/10/5` → `1401/10/05`. `None` when the text is not a Jalali `Y/M/D` date.
pub fn normalize_jalali(s: &str) -> Option<String> {
    let s = latin_digits(s.trim());
    let mut parts = s.split('/');
    let y: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1300 {
        return None;
    }
    Some(format!("{y:04}/{m:02}/{d:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_adds_vat_and_rounds_to_whole_rials() {
        assert_eq!(cost_of(10_731_000, 10.0), 11_804_100);
        assert_eq!(cost_of(1_647_000, 10.0), 1_811_700);
        assert_eq!(cost_of(15, 10.0), 17); // 16.5 rounds half away from zero
        assert_eq!(cost_of(0, 10.0), 0);
        assert_eq!(cost_of(846_259_450, 13.0), 956_273_179); // f64 would answer …178
        assert_eq!(cost_of(1_000, 0.0), 1_000);
        assert_eq!(cost_of(999, 9.0), 1_089); // 1088.91 → 1089
        assert_eq!(cost_of(5, 10.0), 6); // 5.5 rounds half up
    }

    #[test]
    fn jalali_dates_are_zero_padded_for_lexical_comparison() {
        assert_eq!(
            normalize_jalali("1401/10/5"),
            Some("1401/10/05".to_string())
        );
        assert_eq!(
            normalize_jalali("۱۴۰۱/۱/۲۳"),
            Some("1401/01/23".to_string())
        );
        assert_eq!(normalize_jalali("garbage"), None);
        assert!(normalize_jalali("1401/10/05").unwrap() > normalize_jalali("1401/9/30").unwrap());
    }

    #[test]
    fn authenticity_round_trips_as_landau_codes() {
        assert_eq!(Authenticity::Original.as_str(), "ORIGINAL");
        assert_eq!(Authenticity::parse("fake"), Some(Authenticity::Fake));
        assert_eq!(Authenticity::parse("copy"), None);
    }
}
