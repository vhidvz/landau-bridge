//! Supplier layer: one trait, one implementation today (spec §3, §4). A second supplier is a
//! new module plus one arm in [`open`].

pub mod adaptive;
pub mod barezpakhsh;

use std::sync::Arc;

use crate::domain::{InvoiceHeader, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// `(done, total)` callback for long reads.
pub type Progress = Arc<dyn Fn(u32, u32) + Send + Sync>;

#[async_trait::async_trait]
pub trait Supplier: Send + Sync {
    fn id(&self) -> &'static str;
    /// Establish a session. Idempotent.
    async fn login(&self) -> Result<()>;
    /// Every sale and return header dated on/after `from` (Jalali `YYYY/MM/DD`; all of them when
    /// `None`), oldest first. Receipts are not returned.
    async fn ledger(&self, from: Option<&str>, progress: Progress) -> Result<Vec<InvoiceHeader>>;
    /// The lines of one invoice page (`InvoiceHeader::url`). Safe to call concurrently.
    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>>;
    /// How many [`Supplier::invoice_lines`] calls are worth keeping in flight at once — an
    /// upper bound; the supplier may serve fewer at a time as its connection allows.
    fn parallelism(&self) -> usize {
        1
    }
    /// Today's list price of one code, `None` when the site no longer lists it.
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>>;
    /// Every catalogue row the site's search answers for the first seven words of `name`
    /// (spec §14 b), all pages; empty when nothing matches. The caller picks the equal name.
    async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>>;
}

pub fn open(id: &str, creds: Credentials) -> Result<Box<dyn Supplier>> {
    open_with(id, creds, false)
}

/// [`open`] for a check the seller is waiting on: a short retry budget, so a dead connection
/// answers in seconds instead of after a job's patient minutes.
pub fn open_for_check(id: &str, creds: Credentials) -> Result<Box<dyn Supplier>> {
    open_with(id, creds, true)
}

fn open_with(id: &str, creds: Credentials, quick: bool) -> Result<Box<dyn Supplier>> {
    match id {
        barezpakhsh::client::ID => {
            let client = barezpakhsh::client::Client::new(creds)?;
            Ok(Box::new(if quick { client.impatient() } else { client }))
        }
        other => Err(BridgeError::Other(format!("unknown supplier {other}"))),
    }
}
