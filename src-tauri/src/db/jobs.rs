use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;

use super::{now, Db};
use crate::errors::{BridgeError, Result};

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
    /// The row's cost before this job wrote it (plan 130 R-i); `None` for a new row.
    pub cost_before: Option<i64>,
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
    /// The link this item is planned from and the version it was proven at (plan 130 R-b).
    pub link_key: Option<String>,
    pub link_version: Option<i64>,
    /// The product a stored link under review still points at (R-e).
    pub link_product: Option<String>,
    /// Why the write table held the link, typed for the screens (`Reason[]`, R-k).
    pub reasons: Option<serde_json::Value>,
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
    "cost_before",
    "price_after",
    "reprice_verdict",
    "error_kind",
    "error_msg",
    "candidates_json",
    "invoice_ids_json",
    "status",
    "qty",
    "name",
    "link_key",
    "link_version",
    "link_product",
    "reasons_json",
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

const ITEM_COLS: &str = "id, job_id, code, name, qty, product_id, product_name, stock_id, authenticity, action, qty_before, qty_after, cost, price_before, price_after, reprice_verdict, error_kind, error_msg, candidates_json, invoice_ids_json, status, available, note, link_key, link_version, link_product, reasons_json, cost_before";

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
        cost_before: r.get(27)?,
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
        link_key: r.get(23)?,
        link_version: r.get(24)?,
        link_product: r.get(25)?,
        reasons: r
            .get::<_, Option<String>>(26)?
            .and_then(|s| serde_json::from_str(&s).ok()),
    })
}

/// A cost a job left on a shelf row — written, or confirmed unchanged by a refresh — with the
/// supplier item it came from: the row's provenance, which the audit reads (plan 130 R-i). A write
/// whose reply was lost and that a later job settled as landed counts too, from its intent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShelfWrite {
    pub item_id: i64,
    pub job_id: i64,
    /// `IMPORT` | `REFRESH`
    pub kind: String,
    /// `ZERO` | `QUANTITY` for an import.
    pub mode: Option<String>,
    /// When the job ended (or started, if it never did).
    pub at: String,
    pub code: String,
    pub name: String,
    /// The link the item was written from (since plan 130 slice 2); `None` on older items.
    pub link_key: Option<String>,
    pub stock_id: String,
    pub cost: i64,
    /// The row's cost before the write, when the job recorded it.
    pub cost_before: Option<i64>,
    /// The supplier's availability when the item was priced.
    pub available: Option<bool>,
    /// The item's quantity; transferred onto the row only by a `QUANTITY` import.
    pub qty: i64,
}

impl Db {
    /// The `(invoice, code)` lines QUANTITY imports before `job` aggregated and did not write (PI-24),
    /// split by how their run ended. `finished`: a run that ended `DONE` held them — skipped for an
    /// identity reason, a missing price or a deferred decision, or failed in transfer — and R-f says
    /// a held line is transferred once resolved. `unfinished`: a run cancelled, failed or interrupted
    /// left them. A line the seller decided is not in Landau (`NoneDecision`) is in neither: it was
    /// decided, not held, and carried it would arrive whole the day the part is linked. Marked lines
    /// are the caller's to leave out.
    pub fn aggregated_lines(&self, job: i64) -> Result<crate::jobs::planner::Carried> {
        let rows: Vec<(String, String, String)> = self.with(|c| {
            let mut st = c.prepare(
                "SELECT j.status, i.code, i.invoice_ids_json FROM job_items i
                 JOIN jobs j ON j.id = i.job_id
                 WHERE j.kind = 'IMPORT' AND j.mode = 'QUANTITY' AND j.id < ?1
                   AND i.invoice_ids_json IS NOT NULL
                   AND i.status NOT IN ('WRITTEN', 'DONE')
                   AND COALESCE(i.error_kind, '') <> 'NoneDecision'
                   AND (j.status <> 'DONE' OR i.status IN ('SKIPPED', 'FAILED'))",
            )?;
            let rows = st.query_map([job], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect()
        })?;
        let mut out = crate::jobs::planner::Carried::default();
        for (status, code, invoices) in rows {
            let invoices: Vec<i64> = serde_json::from_str(&invoices).unwrap_or_default();
            let set = if status == "DONE" {
                &mut out.finished
            } else {
                &mut out.unfinished
            };
            set.extend(invoices.into_iter().map(|inv| (inv, code.clone())));
        }
        Ok(out)
    }

    /// Every cost a job left on a shelf row, oldest first (see [`ShelfWrite`]): the items stamped
    /// written, then the items of landed intents that were not (a lost reply settled by a later job).
    pub fn shelf_writes(&self) -> Result<Vec<ShelfWrite>> {
        self.with(shelf_writes_on)
    }

    pub fn create_job(
        &self,
        kind: &str,
        mode: Option<&str>,
        since_date: Option<&str>,
        store: &str,
        overwrite: bool,
    ) -> Result<i64> {
        // One job at a time, across processes too (plan 130 R-m): SQLite's write lock is taken
        // before the check, so two starts on one database cannot both see "nothing running".
        let started = self.with(|c| {
            let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
            let running: Option<i64> = tx
                .query_row(
                    "SELECT id FROM jobs WHERE status IN ('RUNNING', 'REVIEW') ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = running {
                return Ok(Err(id)); // the transaction rolls back on drop
            }
            tx.execute(
                "INSERT INTO jobs(kind, mode, since_date, store, overwrite, status, phase, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'RUNNING', 'PREFLIGHT', ?6)",
                params![kind, mode, since_date, store, overwrite as i64, now()],
            )?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(Ok(id))
        })?;
        started.map_err(|id| BridgeError::JobBusy(format!("#{id}")))
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

/// [`Db::shelf_writes`] on an open connection, so a caller can read them inside its transaction.
pub(crate) fn shelf_writes_on(c: &rusqlite::Connection) -> rusqlite::Result<Vec<ShelfWrite>> {
    const COLS: &str = "ji.id, ji.job_id, j.kind, j.mode, COALESCE(j.finished_at, j.started_at),
                        ji.code, ji.name, ji.link_key, ji.cost_before, ji.available, ji.qty";
    let read = |r: &rusqlite::Row<'_>, stock: String, cost: i64| -> rusqlite::Result<ShelfWrite> {
        Ok(ShelfWrite {
            item_id: r.get(0)?,
            job_id: r.get(1)?,
            kind: r.get(2)?,
            mode: r.get(3)?,
            at: r.get(4)?,
            code: r.get(5)?,
            name: r.get(6)?,
            link_key: r.get(7)?,
            cost_before: r.get(8)?,
            available: r.get::<_, Option<i64>>(9)?.map(|a| a != 0),
            qty: r.get(10)?,
            stock_id: stock,
            cost,
        })
    };
    let mut out: Vec<ShelfWrite> = Vec::new();
    let mut st = c.prepare(&format!(
        "SELECT {COLS}, ji.stock_id, ji.cost
             FROM job_items ji JOIN jobs j ON j.id = ji.job_id
             WHERE ji.stock_id IS NOT NULL AND ji.cost IS NOT NULL
               AND ji.status IN ('WRITTEN', 'DONE')"
    ))?;
    for w in st.query_map([], |r| {
        let (stock, cost) = (r.get(11)?, r.get(12)?);
        read(r, stock, cost)
    })? {
        out.push(w?);
    }
    let mut st = c.prepare(&format!(
        "SELECT {COLS}, wi.stock_id, wi.target_cost
             FROM write_intents wi, json_each(wi.items_json) item
             JOIN job_items ji ON ji.id = item.value
             JOIN jobs j ON j.id = ji.job_id
             WHERE wi.status = 'LANDED' AND wi.stock_id IS NOT NULL
               AND NOT (ji.status IN ('WRITTEN', 'DONE') AND ji.stock_id IS NOT NULL
                        AND ji.cost IS NOT NULL)"
    ))?;
    for w in st.query_map([], |r| {
        let (stock, cost) = (r.get(11)?, r.get(12)?);
        read(r, stock, cost)
    })? {
        out.push(w?);
    }
    out.sort_by_key(|w| w.item_id);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PI-24, its review: only a line an earlier QUANTITY run *held* is carried past «از تاریخ» —
    /// never one the seller decided is not in Landau (it would come back, every unit at once, the
    /// day the part is linked), nor a written one.
    #[test]
    fn only_held_lines_of_earlier_runs_are_carried() {
        let db = Db::in_memory().unwrap();
        let item = |job: i64, code: &str, invoice: i64, status: &str, kind: Option<&str>| {
            let id = db.add_job_item(job, code, code, 1).unwrap();
            let mut patch = vec![
                ("invoice_ids_json", crate::jobs::t(&format!("[{invoice}]"))),
                ("status", crate::jobs::t(status)),
            ];
            if let Some(k) = kind {
                patch.push(("error_kind", crate::jobs::t(k)));
            }
            db.update_job_item(id, &patch).unwrap();
        };
        let done = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        item(done, "N", 1, "SKIPPED", Some("NoneDecision"));
        item(done, "H", 2, "SKIPPED", Some("Conflict"));
        item(done, "F", 3, "FAILED", Some("LandauUnreachable"));
        item(done, "U", 4, "SKIPPED", Some("Unmatched"));
        item(done, "W", 5, "DONE", None);
        db.set_job_status(done, "DONE", "REPORT").unwrap();
        let cut = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        item(cut, "R", 6, "REVIEW", None);
        item(cut, "M", 7, "SKIPPED", Some("NoneDecision"));
        db.set_job_status(cut, "CANCELLED", "REVIEW").unwrap();
        let now = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        let carried = db.aggregated_lines(now).unwrap();
        let mut finished: Vec<_> = carried.finished.into_iter().collect();
        finished.sort();
        assert_eq!(
            finished,
            vec![(2, "H".to_string()), (3, "F".into()), (4, "U".into())]
        );
        assert_eq!(
            carried.unfinished.into_iter().collect::<Vec<_>>(),
            vec![(6, "R".to_string())]
        );
    }

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

    /// Plan 130 R-m: two processes on one database cannot both start a job — each start takes
    /// the write lock before it looks for a running one.
    #[test]
    fn two_processes_cannot_both_start_a_job() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        drop(Db::open(&path).unwrap());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
        let starts: Vec<_> = (0..6)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let db = Db::open(&path).unwrap(); // its own connection, as a second app has
                    barrier.wait();
                    db.create_job("IMPORT", Some("ZERO"), None, "store", false)
                })
            })
            .collect();
        let outcomes: Vec<_> = starts.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(
            outcomes.iter().filter(|o| o.is_ok()).count(),
            1,
            "{outcomes:?}"
        );
        assert!(outcomes
            .iter()
            .filter_map(|o| o.as_ref().err())
            .all(|e| matches!(e, BridgeError::JobBusy(_))));
        let db = Db::open(&path).unwrap();
        assert_eq!(db.list_jobs().unwrap().len(), 1);
        // a job paused in review still holds the slot; a finished one frees it
        let id = db.running_job().unwrap().unwrap().id;
        db.set_job_status(id, "REVIEW", "REVIEW").unwrap();
        assert!(db
            .create_job("REFRESH", None, None, "store", false)
            .is_err());
        db.set_job_status(id, "DONE", "REPORT").unwrap();
        assert!(db.create_job("REFRESH", None, None, "store", false).is_ok());
    }
}
