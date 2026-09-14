//! SQLite persistence (spec §8). One connection behind a mutex; every call is short.

pub mod errors;
pub mod invoices;
pub mod jobs;
pub mod matches;
pub mod prices;
pub mod settings;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::errors::{BridgeError, Result};

const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BridgeError::Db(e.to_string()))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Run `f` with the connection. Poisoned locks surface as `Db` errors rather than panics.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| BridgeError::Db(format!("lock: {e}")))?;
        f(&guard).map_err(BridgeError::from)
    }

    pub fn user_version(&self) -> Result<i64> {
        self.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
    }

    /// Each pending migration runs with its `user_version` bump in one transaction, so a crash
    /// mid-migration leaves the schema at the previous version rather than half-applied with the
    /// old version number (which would re-run the script over existing tables next launch).
    pub fn migrate(&self) -> Result<()> {
        let current = self.user_version()?;
        self.with(|c| {
            for (i, sql) in MIGRATIONS.iter().enumerate() {
                let version = i as i64 + 1;
                if version <= current {
                    continue;
                }
                let tx = c.unchecked_transaction()?;
                tx.execute_batch(sql)?;
                tx.pragma_update(None, "user_version", version)?;
                tx.commit()?;
            }
            Ok(())
        })
    }
}

/// ISO-8601 UTC timestamp used for every `*_at` column.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InvoiceHeader, InvoiceKind, InvoiceLine};

    fn header(number: &str, kind: InvoiceKind, date: &str, paid: bool) -> InvoiceHeader {
        InvoiceHeader {
            number: number.into(),
            kind,
            date_jalali: date.into(),
            date_iso: "2023-01-04".into(),
            paid,
            url: format!("/Shop/Preview/Factor/{number}/11000"),
        }
    }

    #[test]
    fn migrates_once_and_stores_settings() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.setting("threshold").unwrap(), None);
        db.set_setting("threshold", "80").unwrap();
        assert_eq!(db.setting("threshold").unwrap().as_deref(), Some("80"));
        // migrating again is a no-op
        db.migrate().unwrap();
        assert_eq!(db.user_version().unwrap(), 1);
    }

    #[test]
    fn invoices_are_unique_per_number_and_kind_and_lines_replace() {
        let db = Db::in_memory().unwrap();
        let id = db
            .upsert_invoice(
                "barezpakhsh",
                &header("1", InvoiceKind::Sale, "1401/10/05", true),
            )
            .unwrap();
        let again = db
            .upsert_invoice(
                "barezpakhsh",
                &header("1", InvoiceKind::Sale, "1401/10/05", true),
            )
            .unwrap();
        assert_eq!(id, again);
        let ret = db
            .upsert_invoice(
                "barezpakhsh",
                &header("1", InvoiceKind::Return, "1401/10/05", true),
            )
            .unwrap();
        assert_ne!(id, ret);

        assert_eq!(db.unfetched_invoice_ids("barezpakhsh").unwrap().len(), 2);
        let lines = vec![InvoiceLine {
            line_no: 1,
            code: "A".into(),
            name: "n".into(),
            qty: 2,
            unit: "عدد".into(),
            unit_price: 10,
        }];
        db.replace_lines(id, &lines).unwrap();
        db.replace_lines(id, &lines).unwrap();
        assert_eq!(db.unfetched_invoice_ids("barezpakhsh").unwrap(), vec![ret]);
        let all = db.lines_with_invoices("barezpakhsh").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].code, "A");
        assert_eq!(all[0].kind, InvoiceKind::Sale);
    }

    #[test]
    fn upsert_invoices_writes_the_batch_in_one_go_and_finds_existing_rows() {
        let db = Db::in_memory().unwrap();
        let first = db
            .upsert_invoice(
                "barezpakhsh",
                &header("1", InvoiceKind::Sale, "1401/10/05", false),
            )
            .unwrap();
        let headers = vec![
            header("1", InvoiceKind::Sale, "1401/10/05", true),
            header("2", InvoiceKind::Return, "1401/10/06", true),
            header("3", InvoiceKind::Sale, "1401/10/07", false),
        ];
        db.upsert_invoices("barezpakhsh", &headers).unwrap();
        assert_eq!(db.count_invoices("barezpakhsh").unwrap(), (3, 1));
        let ids = db.unfetched_invoice_ids("barezpakhsh").unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(
            ids[0], first,
            "the existing row was updated, not duplicated"
        );
        let lines = db.lines_with_invoices("barezpakhsh").unwrap();
        assert!(lines.is_empty(), "headers only");
        // the batch is idempotent
        db.upsert_invoices("barezpakhsh", &headers).unwrap();
        assert_eq!(db.count_invoices("barezpakhsh").unwrap(), (3, 1));
    }

    #[test]
    fn matches_persist_and_none_is_a_decision() {
        let db = Db::in_memory().unwrap();
        assert!(db.match_for("barezpakhsh", "A").unwrap().is_none());
        db.set_match(
            "barezpakhsh",
            "A",
            Some(("p1", "part one")),
            "AUTO",
            Some(91),
            "name a",
        )
        .unwrap();
        let m = db.match_for("barezpakhsh", "A").unwrap().unwrap();
        assert_eq!(m.product_id.as_deref(), Some("p1"));
        db.set_match("barezpakhsh", "B", None, "NONE", None, "name b")
            .unwrap();
        let none = db.match_for("barezpakhsh", "B").unwrap().unwrap();
        assert_eq!(none.how, "NONE");
        assert!(none.product_id.is_none());
        assert_eq!(db.list_matches("barezpakhsh").unwrap().len(), 2);
    }

    #[test]
    fn jobs_items_events_and_errors_round_trip() {
        let db = Db::in_memory().unwrap();
        let job = db
            .create_job("IMPORT", Some("ZERO"), None, "store1", false)
            .unwrap();
        let item = db.add_job_item(job, "A", "name a", 3).unwrap();
        db.update_job_item(
            item,
            &[("status", rusqlite::types::Value::Text("REVIEW".into()))],
        )
        .unwrap();
        db.add_job_event(job, "INFO", "MATCH", "matched 1", None)
            .unwrap();
        db.record_error(
            Some(job),
            "NoCurrentPrice",
            "no price",
            Some(serde_json::json!({"code": "A"})),
        )
        .unwrap();
        db.set_job_status(job, "REVIEW", "MATCH").unwrap();

        let detail = db.job_detail(job).unwrap();
        assert_eq!(detail.job.status, "REVIEW");
        assert_eq!(detail.items.len(), 1);
        assert_eq!(detail.items[0].status, "REVIEW");
        assert_eq!(detail.events.len(), 1);
        assert_eq!(db.list_errors(10).unwrap().len(), 1);
        assert_eq!(db.list_jobs().unwrap().len(), 1);
        // The job is REVIEW, not DONE/FAILED/etc, so it still counts as "the one job at a
        // time" (spec §7.3) and running_job() must still surface it, not None.
        assert_eq!(db.running_job().unwrap().map(|j| j.id), Some(job));
    }

    #[test]
    fn price_snapshots_answer_the_latest_per_code() {
        let db = Db::in_memory().unwrap();
        db.add_price_snapshot(
            "barezpakhsh",
            "A",
            100,
            110,
            Some(10.0),
            true,
            "2026-09-13T10:00:00Z",
        )
        .unwrap();
        db.add_price_snapshot(
            "barezpakhsh",
            "A",
            120,
            132,
            Some(10.0),
            true,
            "2026-09-13T11:00:00Z",
        )
        .unwrap();
        let latest = db.latest_price("barezpakhsh", "A").unwrap().unwrap();
        assert_eq!(latest.cost, 132);
    }

    #[test]
    fn invoice_imports_guard_double_transfer() {
        let db = Db::in_memory().unwrap();
        let id = db
            .upsert_invoice(
                "barezpakhsh",
                &header("9", InvoiceKind::Sale, "1401/10/05", true),
            )
            .unwrap();
        assert!(db.imported_lines().unwrap().is_empty());
        db.mark_lines_imported(&[(id, "A".into())], 1).unwrap();
        db.mark_lines_imported(&[(id, "A".into())], 2).unwrap(); // idempotent, PK is (invoice_id, code)
        assert_eq!(
            db.imported_lines().unwrap(),
            std::collections::HashSet::from([(id, "A".to_string())])
        );
        assert!(!db
            .imported_lines()
            .unwrap()
            .contains(&(id, "B".to_string())));
    }
}
