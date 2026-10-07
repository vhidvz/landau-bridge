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

/// A part an import job left without a decision, as the «بدون تصمیم» list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UndecidedRow {
    pub name_key: String,
    pub name_seen: String,
    pub code: String,
    /// The latest job that left it undecided, when that job started, and its quantity there.
    pub job_id: i64,
    pub seen_at: String,
    pub qty: i64,
    /// How many jobs have left it undecided.
    pub times: i64,
    /// The candidates review showed in that job, best first (`Scored[]`); `[]` when Landau's
    /// picker had none.
    pub candidates: serde_json::Value,
}

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

    /// Writes an import's decisions in one transaction — all of them or, on any error, none —
    /// each keyed on `name_key(name_seen)` as [`Self::set_match`] keys it, keeping the date it
    /// was taken when the file carries one.
    pub fn apply_decisions(
        &self,
        supplier: &str,
        decisions: &[crate::decisions::Decision],
    ) -> Result<()> {
        self.with(|c| {
            c.execute_batch("BEGIN")?;
            let result = (|| {
                let mut st = c.prepare(
                    "INSERT INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at, priced_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(supplier, name_key) DO UPDATE SET code = excluded.code, product_id = excluded.product_id,
                       product_name = excluded.product_name, how = excluded.how, similarity = excluded.similarity,
                       name_seen = excluded.name_seen, decided_at = excluded.decided_at, priced_at = excluded.priced_at",
                )?;
                for d in decisions {
                    st.execute(params![
                        supplier,
                        name_key(&d.name_seen),
                        d.code,
                        d.product_id,
                        d.product_name,
                        d.how,
                        d.similarity,
                        d.name_seen,
                        d.decided_at.clone().unwrap_or_else(now),
                        d.priced_at,
                    ])?;
                }
                Ok(())
            })();
            match result {
                Ok(()) => c.execute_batch("COMMIT"),
                Err(e) => {
                    let _ = c.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
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

    /// Every part an ended import job left undecided — deferred in review and skipped at
    /// continue (`SKIPPED`/`Unmatched`), or still `REVIEW` in a job that was cancelled or
    /// interrupted — for which no decision exists now. "Exists" is the matcher's own lookup, by
    /// name and then by code ([`crate::matching::Matcher::stored`]), read-only here, so this list
    /// and the next import agree. Jobs are never deleted, so the list outlives the job that made
    /// it; a decision taken anywhere removes the part, a cleared one brings it back. One row per
    /// name, carrying its latest job's candidates; a job still paused in review keeps its own.
    pub fn undecided_matches(&self, supplier: &str) -> Result<Vec<UndecidedRow>> {
        let items: Vec<(i64, String, String, i64, Option<String>, String)> = self.with(|c| {
            let mut st = c.prepare(
                "SELECT ji.job_id, ji.code, ji.name, ji.qty, ji.candidates_json, j.started_at
                 FROM job_items ji JOIN jobs j ON j.id = ji.job_id
                 WHERE j.kind = 'IMPORT'
                   AND j.status IN ('DONE', 'FAILED', 'CANCELLED', 'INTERRUPTED')
                   AND ((ji.status = 'SKIPPED' AND ji.error_kind = 'Unmatched') OR ji.status = 'REVIEW')
                 ORDER BY ji.job_id DESC, ji.id",
            )?;
            let rows = st.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?;
            rows.collect()
        })?;
        let mut out: Vec<UndecidedRow> = Vec::new();
        // name key → (its row in `out`, the last job counted for it); rows arrive job by job.
        let mut at: std::collections::HashMap<String, (usize, i64)> =
            std::collections::HashMap::new();
        for (job_id, code, name, qty, candidates, started_at) in items {
            let key = name_key(&name);
            if let Some((i, last)) = at.get_mut(&key) {
                if *last != job_id {
                    *last = job_id;
                    out[*i].times += 1;
                }
                continue;
            }
            if self.match_for_name(supplier, &name)?.is_some()
                || self.match_for_code(supplier, &code)?.is_some()
            {
                continue;
            }
            let candidates = candidates
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_else(|| serde_json::json!([]));
            at.insert(key.clone(), (out.len(), job_id));
            out.push(UndecidedRow {
                name_key: key,
                name_seen: name,
                code,
                job_id,
                seen_at: started_at,
                qty,
                times: 1,
                candidates,
            });
        }
        Ok(out)
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
