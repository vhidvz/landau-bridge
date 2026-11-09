//! A Barezpakhsh for the offline end-to-end tests (plan 130 task 12): one paid invoice holding the
//! given lines, and a price list answered by name (the equal-name row of a search) and by code.

use std::sync::Mutex;

use crate::domain::{InvoiceHeader, InvoiceKind, InvoiceLine, PriceQuote};
use crate::errors::Result;
use crate::supplier::{Progress, Supplier};

pub struct FakeSupplier {
    lines: Vec<InvoiceLine>,
    /// `(code, name, list price before VAT)`.
    prices: Vec<(String, String, i64)>,
    /// Every price read, `name:…` or `code:…`, in order.
    pub asked: Mutex<Vec<String>>,
    /// Codes the site lists as unavailable today.
    unavailable: Vec<String>,
}

impl FakeSupplier {
    /// `items`: `(code, name, qty, list price)` — one invoice line each, priced as listed today.
    pub fn new(items: &[(&str, &str, i64, i64)]) -> FakeSupplier {
        FakeSupplier {
            lines: items
                .iter()
                .enumerate()
                .map(|(n, (code, name, qty, price))| InvoiceLine {
                    line_no: n as u32 + 1,
                    code: code.to_string(),
                    name: name.to_string(),
                    qty: *qty,
                    unit: "عدد".into(),
                    unit_price: *price,
                })
                .collect(),
            prices: items
                .iter()
                .map(|(code, name, _, price)| (code.to_string(), name.to_string(), *price))
                .collect(),
            asked: Mutex::default(),
            unavailable: Vec::new(),
        }
    }

    /// The site lists `code` as unavailable today.
    pub fn unavailable(mut self, code: &str) -> FakeSupplier {
        self.unavailable.push(code.to_string());
        self
    }

    fn quote(&self, (code, name, price): &(String, String, i64)) -> PriceQuote {
        PriceQuote {
            code: code.clone(),
            name: name.clone(),
            list_price: *price,
            vat_percent: Some(10.0),
            has_vat: true,
            available: !self.unavailable.contains(code),
            unit: "عدد".into(),
        }
    }
}

#[async_trait::async_trait]
impl Supplier for FakeSupplier {
    fn id(&self) -> &'static str {
        "barezpakhsh"
    }
    async fn login(&self) -> Result<()> {
        Ok(())
    }
    async fn ledger(&self, _from: Option<&str>, _progress: Progress) -> Result<Vec<InvoiceHeader>> {
        Ok(vec![InvoiceHeader {
            number: "9001".into(),
            kind: InvoiceKind::Sale,
            date_jalali: "1405/08/15".into(),
            date_iso: "2026-11-06".into(),
            paid: true,
            url: "/Shop/Preview/Factor/9001/1".into(),
        }])
    }
    async fn invoice_lines(&self, _url: &str) -> Result<Vec<InvoiceLine>> {
        Ok(self.lines.clone())
    }
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        self.asked.lock().unwrap().push(format!("code:{code}"));
        Ok(self
            .prices
            .iter()
            .find(|p| p.0 == code)
            .map(|p| self.quote(p)))
    }
    async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>> {
        self.asked.lock().unwrap().push(format!("name:{name}"));
        Ok(self
            .prices
            .iter()
            .filter(|p| p.1 == name)
            .map(|p| self.quote(p))
            .collect())
    }
}
