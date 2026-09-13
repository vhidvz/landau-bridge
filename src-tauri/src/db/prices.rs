use rusqlite::{params, OptionalExtension};

use super::Db;
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct PriceRow {
    pub list_price: i64,
    pub cost: i64,
    pub vat_percent: Option<f64>,
    pub available: bool,
    pub fetched_at: String,
}

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub fn add_price_snapshot(
        &self,
        supplier: &str,
        code: &str,
        list_price: i64,
        cost: i64,
        vat_percent: Option<f64>,
        available: bool,
        fetched_at: &str,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO price_snapshots(supplier, code, list_price, cost, vat_percent, available, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![supplier, code, list_price, cost, vat_percent, available as i64, fetched_at],
            )
            .map(|_| ())
        })
    }

    pub fn latest_price(&self, supplier: &str, code: &str) -> Result<Option<PriceRow>> {
        self.with(|c| {
            c.query_row(
                "SELECT list_price, cost, vat_percent, available, fetched_at FROM price_snapshots
                 WHERE supplier = ?1 AND code = ?2 ORDER BY fetched_at DESC, id DESC LIMIT 1",
                params![supplier, code],
                |r| {
                    Ok(PriceRow {
                        list_price: r.get(0)?,
                        cost: r.get(1)?,
                        vat_percent: r.get(2)?,
                        available: r.get::<_, i64>(3)? != 0,
                        fetched_at: r.get(4)?,
                    })
                },
            )
            .optional()
        })
    }
}
