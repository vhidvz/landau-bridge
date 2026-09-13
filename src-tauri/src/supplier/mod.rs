//! Supplier layer: one trait, one implementation today (spec §3, §4). A second supplier is a
//! new module plus one arm in [`open`].

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
    /// Every sale and return header, oldest first. Receipts are not returned.
    async fn ledger(&self, progress: Progress) -> Result<Vec<InvoiceHeader>>;
    /// The lines of one invoice page (`InvoiceHeader::url`).
    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>>;
    /// Today's list price of one code, `None` when the site no longer lists it.
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>>;
}

pub fn open(id: &str, creds: Credentials) -> Result<Box<dyn Supplier>> {
    match id {
        barezpakhsh::client::ID => Ok(Box::new(barezpakhsh::client::Client::new(creds)?)),
        other => Err(BridgeError::Other(format!("unknown supplier {other}"))),
    }
}
