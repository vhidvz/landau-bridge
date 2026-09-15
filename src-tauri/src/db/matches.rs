use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;
use crate::matching::normalize::name_key;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchRow {
    pub supplier: String,
    /// `name_key(name_seen)` — the link's identity (spec §14 a).
    pub name_key: String,
    /// The supplier's code when the link last saw one; an attribute, not the key.
    pub code: String,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    /// AUTO | USER | NONE | MAP
    pub how: String,
    pub similarity: Option<i64>,
    pub name_seen: String,
    pub priced_at: Option<String>,
    pub decided_at: String,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MatchRow> {
    Ok(MatchRow {
        supplier: r.get(0)?,
        name_key: r.get(1)?,
        code: r.get(2)?,
        product_id: r.get(3)?,
        product_name: r.get(4)?,
        how: r.get(5)?,
        similarity: r.get(6)?,
        name_seen: r.get(7)?,
        priced_at: r.get(8)?,
        decided_at: r.get(9)?,
    })
}

const COLS: &str = "supplier, name_key, code, product_id, product_name, how, similarity, name_seen, priced_at, decided_at";

impl Db {
    pub fn match_for_name(&self, supplier: &str, name: &str) -> Result<Option<MatchRow>> {
        let key = name_key(name);
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND name_key = ?2"),
                params![supplier, key],
                row,
            )
            .optional()
        })
    }

    /// The most recently decided link that carries `code`; never for an empty code.
    pub fn match_for_code(&self, supplier: &str, code: &str) -> Result<Option<MatchRow>> {
        if code.is_empty() {
            return Ok(None);
        }
        self.with(|c| {
            c.query_row(
                &format!(
                    "SELECT {COLS} FROM matches WHERE supplier = ?1 AND code = ?2 ORDER BY decided_at DESC LIMIT 1"
                ),
                params![supplier, code],
                row,
            )
            .optional()
        })
    }

    /// `product` = `Some((id, name))` for a match, `None` with `how = "NONE"` for "not in Landau".
    /// Keyed on `name_key(name_seen)`; a second decision for the same name replaces the first.
    pub fn set_match(
        &self,
        supplier: &str,
        name_seen: &str,
        code: &str,
        product: Option<(&str, &str)>,
        how: &str,
        similarity: Option<i64>,
    ) -> Result<()> {
        let (pid, pname) = match product {
            Some((id, name)) => (Some(id), Some(name)),
            None => (None, None),
        };
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "INSERT INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(supplier, name_key) DO UPDATE SET code = excluded.code, product_id = excluded.product_id,
                   product_name = excluded.product_name, how = excluded.how, similarity = excluded.similarity,
                   name_seen = excluded.name_seen, decided_at = excluded.decided_at",
                params![supplier, key, code, pid, pname, how, similarity, name_seen, now()],
            )
            .map(|_| ())
        })
    }

    /// The link for `name_seen` answered a price just now (spec §14 c: refresh prefers it).
    pub fn touch_priced(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "UPDATE matches SET priced_at = ?3 WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key, now()],
            )
            .map(|_| ())
        })
    }

    pub fn clear_match(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "DELETE FROM matches WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key],
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

    /// Every (name → product_id) link with a product, for the refresh job.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::in_memory().unwrap()
    }

    #[test]
    fn links_are_found_by_name_then_by_code_and_carry_the_code_as_an_attribute() {
        let db = db();
        db.set_match(
            "s",
            "لنت جلو پژو ۴۰۵ عظام* (16)",
            "3137006",
            Some(("p1", "لنت جلو")),
            "AUTO",
            Some(91),
        )
        .unwrap();
        let by_name = db
            .match_for_name("s", "لنت جلو پژو 405 عظام* (16)")
            .unwrap()
            .unwrap();
        assert_eq!(by_name.name_key, "لنت جلو پژو 405 عظام* (16)");
        assert_eq!(by_name.code, "3137006");
        assert!(by_name.priced_at.is_none());
        let by_code = db.match_for_code("s", "3137006").unwrap().unwrap();
        assert_eq!(by_code.product_id.as_deref(), Some("p1"));
        assert!(
            db.match_for_code("s", "").unwrap().is_none(),
            "an empty code never matches"
        );
        assert!(db.match_for_name("s", "چیز دیگر").unwrap().is_none());
    }

    #[test]
    fn a_second_decision_for_the_same_name_replaces_the_first() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("p1", "الف")), "AUTO", Some(80))
            .unwrap();
        db.set_match("s", "قطعه", "2", None, "NONE", None).unwrap();
        let row = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(
            (row.how.as_str(), row.code.as_str(), row.product_id),
            ("NONE", "2", None)
        );
        assert_eq!(db.list_matches("s").unwrap().len(), 1);
        assert!(db.linked_matches("s").unwrap().is_empty());
    }

    #[test]
    fn touch_priced_stamps_the_link_and_clear_removes_it() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("p1", "الف")), "USER", None)
            .unwrap();
        db.touch_priced("s", "قطعه").unwrap();
        assert!(db
            .match_for_name("s", "قطعه")
            .unwrap()
            .unwrap()
            .priced_at
            .is_some());
        db.clear_match("s", "قطعه").unwrap();
        assert!(db.match_for_name("s", "قطعه").unwrap().is_none());
    }
}

