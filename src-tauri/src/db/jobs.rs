use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub mode: Option<String>,
    pub since_date: Option<String>,
    pub store: String,
    pub overwrite: bool,
    pub status: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub summary: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobItemRow {
    pub id: i64,
    pub job_id: i64,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    pub stock_id: Option<String>,
    pub authenticity: Option<String>,
    pub action: Option<String>,
    pub qty_before: Option<i64>,
    pub qty_after: Option<i64>,
    pub cost: Option<i64>,
    pub price_before: Option<i64>,
    pub price_after: Option<i64>,
    pub reprice_verdict: Option<String>,
    pub error_kind: Option<String>,
    pub error_msg: Option<String>,
    pub candidates: Option<serde_json::Value>,
    /// Invoice ids this item's quantity was aggregated from (empty for a refresh-job row).
    pub invoice_ids: Vec<i64>,
    pub status: String,
    /// The supplier's availability at pricing time (spec §14 d); `None` before the PRICE phase.
    pub available: Option<bool>,
    /// «؛»-joined remarks for the report (spec §14 g).
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobEventRow {
    pub id: i64,
    pub at: String,
    pub level: String,
    pub phase: String,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobDetail {
    pub job: JobRow,
    pub items: Vec<JobItemRow>,
    pub events: Vec<JobEventRow>,
}

/// A column/value pair for [`Db::update_job_item`]. Allowed columns are listed in `ITEM_COLUMNS`.
pub type ItemPatch<'a> = (&'a str, rusqlite::types::Value);

const ITEM_COLUMNS: &[&str] = &[
    "available",
    "note",
    "product_id",
    "product_name",
    "stock_id",
    "authenticity",
    "action",
    "qty_before",
    "qty_after",
    "cost",
    "price_before",
    "price_after",
    "reprice_verdict",
    "error_kind",
    "error_msg",
    "candidates_json",
    "invoice_ids_json",
    "status",
    "qty",
    "name",
];

const JOB_COLS: &str = "id, kind, mode, since_date, store, overwrite, status, phase, started_at, finished_at, summary_json";

fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    let summary: Option<String> = r.get(10)?;
    Ok(JobRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        mode: r.get(2)?,
        since_date: r.get(3)?,
        store: r.get(4)?,
        overwrite: r.get::<_, i64>(5)? != 0,
        status: r.get(6)?,
        phase: r.get(7)?,
        started_at: r.get(8)?,
        finished_at: r.get(9)?,
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

const ITEM_COLS: &str = "id, job_id, code, name, qty, product_id, product_name, stock_id, authenticity, action, qty_before, qty_after, cost, price_before, price_after, reprice_verdict, error_kind, error_msg, candidates_json, invoice_ids_json, status, available, note";

fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobItemRow> {
    Ok(JobItemRow {
        id: r.get(0)?,
        job_id: r.get(1)?,
        code: r.get(2)?,
        name: r.get(3)?,
        qty: r.get(4)?,
        product_id: r.get(5)?,
        product_name: r.get(6)?,
        stock_id: r.get(7)?,
        authenticity: r.get(8)?,
        action: r.get(9)?,
        qty_before: r.get(10)?,
        qty_after: r.get(11)?,
        cost: r.get(12)?,
        price_before: r.get(13)?,
        price_after: r.get(14)?,
        reprice_verdict: r.get(15)?,
        error_kind: r.get(16)?,
        error_msg: r.get(17)?,
        candidates: r
            .get::<_, Option<String>>(18)?
            .and_then(|c| serde_json::from_str(&c).ok()),
        invoice_ids: r
            .get::<_, Option<String>>(19)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        status: r.get(20)?,
        available: r.get::<_, Option<i64>>(21)?.map(|v| v != 0),
        note: r.get(22)?,
    })
}

impl Db {
    pub fn create_job(
        &self,
        kind: &str,
        mode: Option<&str>,
        since_date: Option<&str>,
        store: &str,
        overwrite: bool,
    ) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO jobs(kind, mode, since_date, store, overwrite, status, phase, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'RUNNING', 'PREFLIGHT', ?6)",
                params![kind, mode, since_date, store, overwrite as i64, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn set_job_status(&self, job_id: i64, status: &str, phase: &str) -> Result<()> {
        let finished = matches!(status, "DONE" | "FAILED" | "CANCELLED" | "INTERRUPTED").then(now);
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET status = ?2, phase = ?3, finished_at = COALESCE(?4, finished_at) WHERE id = ?1",
                params![job_id, status, phase, finished],
            )
            .map(|_| ())
        })
    }

    pub fn set_job_phase(&self, job_id: i64, phase: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET phase = ?2 WHERE id = ?1",
                params![job_id, phase],
            )
            .map(|_| ())
        })
    }

    pub fn set_job_summary(&self, job_id: i64, summary: &serde_json::Value) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET summary_json = ?2 WHERE id = ?1",
                params![job_id, summary.to_string()],
            )
            .map(|_| ())
        })
    }

    pub fn job(&self, job_id: i64) -> Result<Option<JobRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {JOB_COLS} FROM jobs WHERE id = ?1"),
                params![job_id],
                job_row,
            )
            .optional()
        })
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {JOB_COLS} FROM jobs ORDER BY id DESC LIMIT 200"
            ))?;
            let rows = st.query_map([], job_row)?;
            rows.collect()
        })
    }

    /// The one job that is RUNNING or paused in REVIEW, if any.
    pub fn running_job(&self) -> Result<Option<JobRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {JOB_COLS} FROM jobs WHERE status IN ('RUNNING', 'REVIEW') ORDER BY id DESC LIMIT 1"),
                [],
                job_row,
            )
            .optional()
        })
    }

    /// On launch: a job left RUNNING by a closed app is INTERRUPTED (spec §7.3).
    pub fn interrupt_stale_jobs(&self) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id FROM jobs WHERE status = 'RUNNING'")?;
            let ids: Vec<i64> = st
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for id in &ids {
                c.execute(
                    "UPDATE jobs SET status = 'INTERRUPTED', finished_at = ?2 WHERE id = ?1",
                    params![id, now()],
                )?;
            }
            Ok(ids)
        })
    }

    pub fn add_job_item(&self, job_id: i64, code: &str, name: &str, qty: i64) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO job_items(job_id, code, name, qty) VALUES (?1, ?2, ?3, ?4)",
                params![job_id, code, name, qty],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn update_job_item(&self, item_id: i64, patch: &[ItemPatch<'_>]) -> Result<()> {
        if patch.is_empty() {
            return Ok(());
        }
        for (col, _) in patch {
            if !ITEM_COLUMNS.contains(col) {
                return Err(crate::errors::BridgeError::Db(format!(
                    "job_items has no column {col}"
                )));
            }
        }
        let sets: Vec<String> = patch
            .iter()
            .enumerate()
            .map(|(i, (col, _))| format!("{col} = ?{}", i + 2))
            .collect();
        let sql = format!("UPDATE job_items SET {} WHERE id = ?1", sets.join(", "));
        self.with(|c| {
            let mut values: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Integer(item_id)];
            values.extend(patch.iter().map(|(_, v)| v.clone()));
            c.execute(&sql, rusqlite::params_from_iter(values))
                .map(|_| ())
        })
    }

    /// Append a remark to the item's `note`, «؛»-separated (spec §14 g).
    pub fn add_item_note(&self, item_id: i64, note: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE job_items SET note = CASE WHEN note IS NULL OR note = '' THEN ?2 ELSE note || '؛ ' || ?2 END WHERE id = ?1",
                params![item_id, note],
            )
            .map(|_| ())
        })
    }

    pub fn job_items(&self, job_id: i64) -> Result<Vec<JobItemRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {ITEM_COLS} FROM job_items WHERE job_id = ?1 ORDER BY id"
            ))?;
            let rows = st.query_map(params![job_id], item_row)?;
            rows.collect()
        })
    }

    pub fn job_item(&self, item_id: i64) -> Result<Option<JobItemRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {ITEM_COLS} FROM job_items WHERE id = ?1"),
                params![item_id],
                item_row,
            )
            .optional()
        })
    }

    pub fn add_job_event(
        &self,
        job_id: i64,
        level: &str,
        phase: &str,
        message: &str,
        detail: Option<serde_json::Value>,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO job_events(job_id, at, level, phase, message, detail_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![job_id, now(), level, phase, message, detail.map(|d| d.to_string())],
            )
            .map(|_| ())
        })
    }

    pub fn job_detail(&self, job_id: i64) -> Result<JobDetail> {
        let job = self
            .job(job_id)?
            .ok_or(crate::errors::BridgeError::JobNotFound(job_id))?;
        let items = self.job_items(job_id)?;
        let events = self.with(|c| {
            let mut st = c.prepare("SELECT id, at, level, phase, message, detail_json FROM job_events WHERE job_id = ?1 ORDER BY id")?;
            let rows = st.query_map(params![job_id], |r| {
                let detail: Option<String> = r.get(5)?;
                Ok(JobEventRow {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    level: r.get(2)?,
                    phase: r.get(3)?,
                    message: r.get(4)?,
                    detail: detail.and_then(|d| serde_json::from_str(&d).ok()),
                })
            })?;
            rows.collect()
        })?;
        Ok(JobDetail { job, items, events })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_notes_are_appended_and_availability_is_read_back() {
        let db = Db::in_memory().unwrap();
        let job = db
            .create_job("REFRESH", None, None, "store", false)
            .unwrap();
        let id = db.add_job_item(job, "1", "قطعه", 0).unwrap();
        db.add_item_note(id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")
            .unwrap();
        db.add_item_note(id, "زیر حداقل سود").unwrap();
        db.update_job_item(id, &[("available", crate::jobs::i(0))])
            .unwrap();
        let row = db.job_item(id).unwrap().unwrap();
        assert_eq!(
            row.note.as_deref(),
            Some("ناموجود؛ قیمت پایین‌تر نادیده گرفته شد؛ زیر حداقل سود")
        );
        assert_eq!(row.available, Some(false));
    }
}
