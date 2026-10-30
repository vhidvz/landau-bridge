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

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_names.sql"),
    include_str!("migrations/0003_drop_name_map.sql"),
];

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
        })?;
        self.backfill_name_keys()
    }

    /// Migration 0002 renamed the code-keyed `matches` to `matches_old`; move its rows back under
    /// their name key (spec §14 a). Idempotent: nothing to do once the old table is gone.
    pub fn backfill_name_keys(&self) -> Result<()> {
        use crate::matching::normalize::name_key;
        self.with(|c| {
            let old: i64 = c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'matches_old'",
                [],
                |r| r.get(0),
            )?;
            if old == 0 {
                return Ok(());
            }
            let tx = c.unchecked_transaction()?;
            {
                let mut st = tx.prepare(
                    "SELECT supplier, code, product_id, product_name, how, similarity, name_seen, decided_at
                     FROM matches_old ORDER BY decided_at",
                )?;
                type Old = (
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    String,
                    Option<i64>,
                    String,
                    String,
                );
                let rows: Vec<Old> = st
                    .query_map([], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                for (supplier, code, pid, pname, how, sim, name_seen, decided_at) in rows {
                    tx.execute(
                        "INSERT OR REPLACE INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        rusqlite::params![
                            supplier,
                            name_key(&name_seen),
                            code,
                            pid,
                            pname,
                            how,
                            sim,
                            name_seen,
                            decided_at
                        ],
                    )?;
                }
            }
            tx.execute_batch("DROP TABLE matches_old")?;
            tx.commit()?;
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
        assert_eq!(db.user_version().unwrap(), 3);
    }

    #[test]
    fn migrates_to_version_2_and_moves_old_links_under_their_name_key() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.user_version().unwrap(), 3);
        // simulate a row left behind by a v1 database
        db.with(|c| {
            c.execute_batch(
                "CREATE TABLE matches_old (supplier TEXT, code TEXT, product_id TEXT, product_name TEXT, how TEXT, similarity INTEGER, name_seen TEXT, decided_at TEXT);
                 INSERT INTO matches_old VALUES ('barezpakhsh', '3137006', 'p1', 'لنت جلو', 'AUTO', 91, 'لنت جلو پژو ۴۰۵ عظام* (16)', '2026-09-13T00:00:00Z');",
            )
        })
        .unwrap();
        db.backfill_name_keys().unwrap();
        let row = db
            .match_for_name("barezpakhsh", "لنت جلو پژو 405 عظام* (16)")
            .unwrap()
            .expect("found by the normalised name");
        assert_eq!(row.code, "3137006");
        assert_eq!(row.product_id.as_deref(), Some("p1"));
        assert_eq!(row.how, "AUTO");
        let gone: i64 = db
            .with(|c| {
                c.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name = 'matches_old'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(gone, 0);
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

        assert_eq!(
            db.unfetched_invoice_ids("barezpakhsh", None).unwrap().len(),
            2
        );
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
        assert_eq!(
            db.unfetched_invoice_ids("barezpakhsh", None).unwrap(),
            vec![ret]
        );
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
        let ids = db.unfetched_invoice_ids("barezpakhsh", None).unwrap();
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
        assert!(db
            .match_for_name("barezpakhsh", "name a")
            .unwrap()
            .is_none());
        db.set_match(
            "barezpakhsh",
            "name a",
            "A",
            Some(("p1", "part one")),
            "AUTO",
            Some(91),
        )
        .unwrap();
        let m = db.match_for_name("barezpakhsh", "name a").unwrap().unwrap();
        assert_eq!(m.product_id.as_deref(), Some("p1"));
        assert_eq!(m.code, "A");
        db.set_match("barezpakhsh", "name b", "B", None, "NONE", None)
            .unwrap();
        let none = db.match_for_name("barezpakhsh", "name b").unwrap().unwrap();
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
    fn unfetched_invoice_ids_can_be_bounded_to_a_start_date() {
        let db = Db::in_memory().unwrap();
        let headers = vec![
            header("1", InvoiceKind::Sale, "1404/12/29", true),
            header("2", InvoiceKind::Sale, "1405/01/01", true),
            header("3", InvoiceKind::Return, "1405/01/02", true),
        ];
        db.upsert_invoices("barezpakhsh", &headers).unwrap();
        let all = db.unfetched_invoice_ids("barezpakhsh", None).unwrap();
        assert_eq!(all.len(), 3);
        let bounded = db
            .unfetched_invoice_ids("barezpakhsh", Some("1405/01/01"))
            .unwrap();
        assert_eq!(
            bounded,
            all[1..].to_vec(),
            "dated on/after the bound, in id order"
        );
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

    // ------------------------------------------------------------ undecided matches (LND-Q07)

    fn text(v: &str) -> rusqlite::types::Value {
        rusqlite::types::Value::Text(v.into())
    }

    /// `(code, name, item status, error kind, candidates JSON)`.
    type ItemSpec<'a> = (&'a str, &'a str, &'a str, Option<&'a str>, Option<&'a str>);

    /// One import job ending in `status`, with `items`. Returns the job id.
    fn job_with(db: &Db, status: &str, items: &[ItemSpec<'_>]) -> i64 {
        let job = db
            .create_job("IMPORT", Some("ZERO"), None, "store1", false)
            .unwrap();
        for (code, name, item_status, kind, candidates) in items {
            let id = db.add_job_item(job, code, name, 2).unwrap();
            let mut patch = vec![("status", text(item_status))];
            if let Some(kind) = kind {
                patch.push(("error_kind", text(kind)));
            }
            if let Some(c) = candidates {
                patch.push(("candidates_json", text(c)));
            }
            db.update_job_item(id, &patch).unwrap();
        }
        db.set_job_status(job, status, "REPORT").unwrap();
        job
    }

    const CANDS_OLD: &str = r#"[{"candidate":{"id":"p_old","name":"old","alias":null,"brand":null,"score":0.0},"similarity":70}]"#;
    const CANDS_NEW: &str = r#"[{"candidate":{"id":"p1","name":"لنت جلو","alias":null,"brand":"x","score":0.0},"similarity":81},{"candidate":{"id":"p2","name":"لنت عقب","alias":null,"brand":null,"score":0.0},"similarity":64}]"#;

    /// A part a job left undecided stays listed — with the candidates review showed, from its
    /// latest job — until a decision exists for it by name or by code, the matcher's own lookup.
    #[test]
    fn undecided_parts_outlive_their_job_until_decided() {
        let db = Db::in_memory().unwrap();
        let sup = "barezpakhsh";
        // Two codes under one name in one job count as one job.
        job_with(
            &db,
            "DONE",
            &[
                (
                    "A1",
                    "لنت جلو پراید",
                    "SKIPPED",
                    Some("Unmatched"),
                    Some(CANDS_OLD),
                ),
                (
                    "A1-OLD",
                    "لنت جلو پراید",
                    "SKIPPED",
                    Some("Unmatched"),
                    Some(CANDS_OLD),
                ),
            ],
        );
        let latest = job_with(
            &db,
            "DONE",
            &[
                (
                    "A1",
                    "لنت جلو پراید",
                    "SKIPPED",
                    Some("Unmatched"),
                    Some(CANDS_NEW),
                ),
                ("B2", "فیلتر روغن", "SKIPPED", Some("Unmatched"), None),
                ("C3", "شمع موتور", "SKIPPED", Some("NoneDecision"), None),
                ("D4", "تسمه تایم", "DONE", None, None),
            ],
        );
        job_with(
            &db,
            "CANCELLED",
            &[("E5", "واشر سرسیلندر", "REVIEW", None, Some(CANDS_NEW))],
        );
        job_with(
            &db,
            "REVIEW",
            &[("F6", "دیسک کلاچ", "REVIEW", None, Some(CANDS_NEW))],
        );

        let rows = db.undecided_matches(sup).unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name_seen.as_str()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.contains(&"لنت جلو پراید"));
        assert!(
            names.contains(&"فیلتر روغن"),
            "no candidates is still undecided"
        );
        assert!(
            names.contains(&"واشر سرسیلندر"),
            "a cancelled review is not a decision"
        );
        assert!(
            !names.contains(&"دیسک کلاچ"),
            "a job still paused in review shows its own"
        );
        assert!(
            !names.contains(&"شمع موتور"),
            "«not in Landau» is a decision"
        );

        let brake = rows.iter().find(|r| r.code == "A1").unwrap();
        assert_eq!(brake.times, 2, "left undecided by two jobs");
        assert_eq!(brake.job_id, latest);
        assert_eq!(
            brake.candidates[0]["candidate"]["id"], "p1",
            "the latest job's list"
        );
        assert_eq!(brake.candidates[1]["similarity"], 64);
        let filter = rows.iter().find(|r| r.code == "B2").unwrap();
        assert_eq!(filter.candidates, serde_json::json!([]));

        // Decided by name — and, under another spelling, by code — it leaves the list.
        db.set_match(
            sup,
            "لنت جلو پراید",
            "A1",
            Some(("p1", "لنت جلو")),
            "USER",
            Some(81),
        )
        .unwrap();
        db.set_match(sup, "فیلتر روغن موتور", "B2", None, "NONE", None)
            .unwrap();
        let left: Vec<String> = db
            .undecided_matches(sup)
            .unwrap()
            .into_iter()
            .map(|r| r.name_seen)
            .collect();
        assert_eq!(left, vec!["واشر سرسیلندر".to_string()]);

        // A cleared decision puts it back.
        db.clear_match(sup, "لنت جلو پراید").unwrap();
        assert_eq!(db.undecided_matches(sup).unwrap().len(), 2);
    }

    /// Decisions exported from one installation land in another as they were — product, how,
    /// similarity and the date they were taken — and a written import is one transaction.
    #[test]
    fn exported_decisions_apply_to_another_installation_as_they_were() {
        let sup = "barezpakhsh";
        let here = Db::in_memory().unwrap();
        here.set_match(
            sup,
            "لنت جلو پراید",
            "A1",
            Some(("p1", "لنت جلو")),
            "USER",
            Some(81),
        )
        .unwrap();
        here.set_match(sup, "شمع موتور", "C3", None, "NONE", None)
            .unwrap();
        here.set_match(
            sup,
            "فیلتر روغن",
            "B2",
            Some(("p2", "فیلتر")),
            "AUTO",
            Some(92),
        )
        .unwrap();
        here.touch_priced(sup, "فیلتر روغن").unwrap(); // carried too
        let file = crate::decisions::export(sup, &here.list_matches(sup).unwrap(), "now");

        let there = Db::in_memory().unwrap();
        there.apply_decisions(sup, &file.decisions).unwrap();
        let mut a = here.list_matches(sup).unwrap();
        let mut b = there.list_matches(sup).unwrap();
        a.sort_by(|x, y| x.name_key.cmp(&y.name_key));
        b.sort_by(|x, y| x.name_key.cmp(&y.name_key));
        assert_eq!(a, b);

        // A batch with one row the table refuses writes nothing at all.
        let mut bad = file.decisions.clone();
        bad[0].name_seen = "یک قطعهٔ تازه".into();
        bad.push(crate::decisions::Decision {
            how: "USER".into(),
            ..bad[1].clone()
        });
        bad.last_mut().unwrap().name_seen = String::new();
        let fresh = Db::in_memory().unwrap();
        fresh
            .with(|c| {
                c.execute_batch(
                    "CREATE TRIGGER no_blank BEFORE INSERT ON matches WHEN NEW.name_seen = ''
                 BEGIN SELECT RAISE(ABORT, 'blank'); END;",
                )
            })
            .unwrap();
        assert!(fresh.apply_decisions(sup, &bad).is_err());
        assert!(
            fresh.list_matches(sup).unwrap().is_empty(),
            "rolled back whole"
        );
    }

    /// A database at version 2, as an installed 0.1.0 leaves it: the landau-parts map loaded
    /// and one link of every kind, `MAP` among them.
    fn version_2_file(path: &Path) {
        let c = Connection::open(path).unwrap();
        for sql in &MIGRATIONS[..2] {
            c.execute_batch(sql).unwrap();
        }
        c.pragma_update(None, "user_version", 2).unwrap();
        c.execute_batch(
            "DROP TABLE matches_old;
             INSERT INTO name_map VALUES ('p', 'b', 'raw', 'C1', 1, 0);
             INSERT INTO settings(key, value) VALUES ('name_map_file', '/x/parts_map.csv'),
               ('name_map_loaded_at', '2026-09-16T00:00:00Z'), ('threshold', '80');
             INSERT INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, priced_at, decided_at) VALUES
               ('barezpakhsh', 'm', 'M1', 'pm', 'map part', 'MAP', NULL, 'm', '2026-09-20T00:00:00Z', '2026-09-16T00:00:00Z'),
               ('barezpakhsh', 'u', 'U1', 'pu', 'user part', 'USER', NULL, 'u', NULL, '2026-09-16T00:00:00Z'),
               ('barezpakhsh', 'a', 'A1', 'pa', 'auto part', 'AUTO', 91, 'a', NULL, '2026-09-16T00:00:00Z'),
               ('barezpakhsh', 'n', 'N1', NULL, NULL, 'NONE', NULL, 'n', NULL, '2026-09-16T00:00:00Z');",
        )
        .unwrap();
    }

    #[test]
    fn migration_3_drops_the_map_and_turns_map_links_into_auto() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_2_file(&path);
        let before = Db::open(&path).unwrap();
        assert_eq!(before.user_version().unwrap(), 3);

        let tables: i64 = before
            .with(|c| {
                c.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name = 'name_map'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(tables, 0, "the map table is gone");
        assert_eq!(before.setting("name_map_file").unwrap(), None);
        assert_eq!(before.setting("name_map_loaded_at").unwrap(), None);
        assert_eq!(before.setting("threshold").unwrap().as_deref(), Some("80"));

        let m = before.match_for_name("barezpakhsh", "m").unwrap().unwrap();
        assert_eq!(m.how, "AUTO");
        assert_eq!(m.code, "M1");
        assert_eq!(m.product_id.as_deref(), Some("pm"));
        assert_eq!(m.priced_at.as_deref(), Some("2026-09-20T00:00:00Z"));
        let how = |n: &str| {
            before
                .match_for_name("barezpakhsh", n)
                .unwrap()
                .unwrap()
                .how
        };
        assert_eq!(how("u"), "USER");
        assert_eq!(how("a"), "AUTO");
        assert_eq!(how("n"), "NONE");
    }
}
