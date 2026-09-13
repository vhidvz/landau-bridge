use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchRow {
    pub supplier: String,
    pub code: String,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    /// AUTO | USER | NONE
    pub how: String,
    pub similarity: Option<i64>,
    pub name_seen: String,
    pub decided_at: String,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MatchRow> {
    Ok(MatchRow {
        supplier: r.get(0)?,
        code: r.get(1)?,
        product_id: r.get(2)?,
        product_name: r.get(3)?,
        how: r.get(4)?,
        similarity: r.get(5)?,
        name_seen: r.get(6)?,
        decided_at: r.get(7)?,
    })
}

const COLS: &str =
    "supplier, code, product_id, product_name, how, similarity, name_seen, decided_at";

impl Db {
    pub fn match_for(&self, supplier: &str, code: &str) -> Result<Option<MatchRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND code = ?2"),
                params![supplier, code],
                row,
            )
            .optional()
        })
    }

    /// `product` = `Some((id, name))` for a match, `None` with `how = "NONE"` for "not in Landau".
    pub fn set_match(
        &self,
        supplier: &str,
        code: &str,
        product: Option<(&str, &str)>,
        how: &str,
        similarity: Option<i64>,
        name_seen: &str,
    ) -> Result<()> {
        let (pid, pname) = match product {
            Some((id, name)) => (Some(id), Some(name)),
            None => (None, None),
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO matches(supplier, code, product_id, product_name, how, similarity, name_seen, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(supplier, code) DO UPDATE SET product_id = excluded.product_id, product_name = excluded.product_name,
                   how = excluded.how, similarity = excluded.similarity, name_seen = excluded.name_seen, decided_at = excluded.decided_at",
                params![supplier, code, pid, pname, how, similarity, name_seen, now()],
            )
            .map(|_| ())
        })
    }

    pub fn clear_match(&self, supplier: &str, code: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM matches WHERE supplier = ?1 AND code = ?2",
                params![supplier, code],
            )
            .map(|_| ())
        })
    }

    pub fn list_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLS} FROM matches WHERE supplier = ?1 ORDER BY decided_at DESC"
            ))?;
            let rows = st.query_map(params![supplier], row)?;
            rows.collect()
        })
    }

    /// Every (code → product_id) link with a product, for the refresh job.
    pub fn linked_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLS} FROM matches WHERE supplier = ?1 AND product_id IS NOT NULL"
            ))?;
            let rows = st.query_map(params![supplier], row)?;
            rows.collect()
        })
    }
}
