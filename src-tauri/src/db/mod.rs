//! SQLite persistence (spec §8). One connection behind a mutex; every call is short.

pub mod errors;
pub mod intents;
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
    include_str!("migrations/0004_supplier_links.sql"),
    include_str!("migrations/0005_write_intents.sql"),
    include_str!("migrations/0006_confirmed_picks.sql"),
    include_str!("migrations/0007_cost_before.sql"),
    include_str!("migrations/0008_renamed_codes.sql"),
];

/// The schema this build writes: one version per migration.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// The settings key the first migration of a launch writes: the schema the install was at before it
/// (plan 130 task 14). The lowest value is kept, and the start-up's notice consumes it.
pub const UPGRADED_FROM_SCHEMA: &str = "upgraded_from_schema";

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BridgeError::Db(e.to_string()))?;
        }
        let existed = path.is_file();
        let conn = Connection::open(path)?;
        // A second writer (another process on this file) waits for the first instead of failing:
        // the job start takes SQLite's write lock up front (R-m).
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        // Refuse a newer schema before anything (even the journal mode) is written to it.
        newer_guard(current)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        if existed && current > 0 && current < SCHEMA_VERSION {
            backup(&conn, path, current)?;
        }
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

    /// A copy of this database in memory, taken by SQLite's backup API as one consistent read —
    /// for work that must judge the seller's data exactly as a job would and may never write it
    /// (plan 130 R-i, the audit). Nothing of the copy reaches the disk; it is gone when dropped.
    pub fn scratch_copy(&self) -> Result<Db> {
        let mut copy = Connection::open_in_memory()?;
        self.with(|c| {
            let backup = rusqlite::backup::Backup::new(c, &mut copy)?;
            // every page in one step: the copy is one consistent read of the database
            backup.run_to_completion(i32::MAX, std::time::Duration::ZERO, None)
        })?;
        copy.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Db {
            conn: Arc::new(Mutex::new(copy)),
        })
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
        newer_guard(current)?;
        self.with(|c| {
            for (i, sql) in MIGRATIONS.iter().enumerate() {
                let version = i as i64 + 1;
                if version <= current {
                    continue;
                }
                let tx = c.unchecked_transaction()?;
                tx.execute_batch(sql)?;
                // An existing install's first step records where it came from, in this step's own
                // transaction, keeping the lowest value: a launch cut between steps leaves the
                // record, and the next launch (starting higher) does not overwrite it.
                if current > 0 && version == current + 1 {
                    tx.execute(
                        "INSERT INTO settings(key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value =
                           CAST(min(CAST(value AS INTEGER), CAST(excluded.value AS INTEGER)) AS TEXT)",
                        rusqlite::params![UPGRADED_FROM_SCHEMA, current.to_string()],
                    )?;
                }
                tx.pragma_update(None, "user_version", version)?;
                tx.commit()?;
            }
            Ok(())
        })?;
        self.backfill_name_keys()
    }

    /// Migration 0002 renamed the code-keyed `matches` to `matches_old`; move its rows under their
    /// name key (spec §14 a) into `supplier_links`, `UNVERIFIED` like every link migration 0004
    /// moved (plan 130). Idempotent: nothing to do once the old table is gone.
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
                        "INSERT OR REPLACE INTO supplier_links(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at, state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'UNVERIFIED')",
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

fn newer_guard(current: i64) -> Result<()> {
    if current > SCHEMA_VERSION {
        return Err(BridgeError::DbNewer {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

/// What the seller is told when the database cannot be opened at launch — after an unattended
/// update this is the first thing they see, so it says where to look, never just "failed".
pub fn open_failure_fa(e: &BridgeError) -> String {
    match e {
        BridgeError::DbNewer { .. } => e.message_fa(),
        other => format!(
            "پایگاه دادهٔ برنامه باز نشد؛ فضای خالی دیسک و دسترسی به پوشهٔ داده را بررسی کنید و برنامه را دوباره باز کنید. دادهٔ شما تغییری نکرده است.\n({other})"
        ),
    }
}

/// Backups kept beside the database, newest schema first.
const KEEP_BACKUPS: usize = 2;

/// Copy the database as it is to `<db>.v<schema>.bak` before migrating it, so data a newer build
/// rewrote can be put back under an older one. Written under a `.part` name and renamed, so a crash
/// never leaves a half backup that looks whole; then all but the newest [`KEEP_BACKUPS`] go.
fn backup(conn: &Connection, path: &Path, schema: i64) -> Result<()> {
    let io = |e: std::io::Error| BridgeError::Db(format!("backup: {e}"));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = path.with_file_name(format!("{name}.v{schema}.bak"));
    let part = path.with_file_name(format!("{name}.v{schema}.bak.part"));
    match std::fs::remove_file(&part) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(e)),
        _ => {}
    }
    conn.execute("VACUUM INTO ?1", [part.to_string_lossy()])?;
    std::fs::rename(&part, &target).map_err(io)?;

    let dir = path.parent().unwrap_or(Path::new("."));
    let prefix = format!("{name}.v");
    let mut found: Vec<(i64, std::path::PathBuf)> = std::fs::read_dir(dir)
        .map_err(io)?
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let v = file
                .strip_prefix(&prefix)?
                .strip_suffix(".bak")?
                .parse()
                .ok()?;
            Some((v, e.path()))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, old) in found.into_iter().skip(KEEP_BACKUPS) {
        // An old backup that will not go (held open, permissions) costs disk, never the launch.
        if let Err(e) = std::fs::remove_file(&old) {
            tracing::warn!("old backup {} not removed: {e}", old.display());
        }
    }
    Ok(())
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
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn migrates_to_version_2_and_moves_old_links_under_their_name_key() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
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
        // the state's own stamp is when each copy wrote it; both arrive UNVERIFIED (plan 130)
        for m in a.iter_mut().chain(b.iter_mut()) {
            m.state_at = None;
        }
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
                    "CREATE TRIGGER no_blank BEFORE INSERT ON supplier_links WHEN NEW.name_seen = ''
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

    fn backups(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".bak"))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_newer_database_is_refused_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        drop(Db::open(&path).unwrap());
        {
            let c = Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let err = Db::open(&path).err().expect("refused");
        assert_eq!(
            err,
            BridgeError::DbNewer {
                found: SCHEMA_VERSION + 1,
                supported: SCHEMA_VERSION
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "file untouched");
        assert!(backups(dir.path()).is_empty());
    }

    #[test]
    fn a_pending_migration_is_backed_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_2_file(&path);
        Db::open(&path).unwrap();
        assert_eq!(backups(dir.path()), vec!["bridge.sqlite.v2.bak"]);
        let bak = Connection::open(dir.path().join("bridge.sqlite.v2.bak")).unwrap();
        let v: i64 = bak
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);
        let maps: i64 = bak
            .query_row("SELECT count(*) FROM name_map", [], |r| r.get(0))
            .unwrap();
        assert_eq!(maps, 1, "the backup holds the data as it was");
    }

    #[test]
    fn an_up_to_date_database_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        drop(Db::open(&path).unwrap());
        Db::open(&path).unwrap();
        assert!(backups(dir.path()).is_empty());
    }

    #[test]
    fn a_new_database_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        Db::open(&dir.path().join("bridge.sqlite")).unwrap();
        assert!(backups(dir.path()).is_empty());
    }

    /// Plan 130 R-i: the audit's copy is the whole database as one read, and whatever is done to
    /// it never reaches the seller's file — not its rows, not its bytes.
    #[test]
    fn a_scratch_copy_is_whole_and_writes_nothing_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        let db = Db::open(&path).unwrap();
        db.set_match("s", "قطعه", "C1", Some(("p1", "x")), "USER", None)
            .unwrap();
        db.with(|c| c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)"))
            .unwrap();
        let bytes = || std::fs::read(&path).unwrap();
        let before = bytes();

        let copy = db.scratch_copy().unwrap();
        assert_eq!(copy.user_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(
            copy.list_matches("s").unwrap(),
            db.list_matches("s").unwrap()
        );
        copy.clear_match("s", "قطعه").unwrap();
        copy.create_job("REFRESH", None, None, "store", false)
            .unwrap();

        assert_eq!(
            db.list_matches("s").unwrap().len(),
            1,
            "the seller's link stays"
        );
        assert!(db.list_jobs().unwrap().is_empty());
        drop(copy);
        assert_eq!(bytes(), before, "the file is byte for byte as it was");
    }

    #[test]
    fn only_the_two_newest_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_2_file(&path);
        std::fs::write(dir.path().join("bridge.sqlite.v0.bak"), b"old").unwrap();
        std::fs::write(dir.path().join("bridge.sqlite.v1.bak"), b"older").unwrap();
        Db::open(&path).unwrap();
        assert_eq!(
            backups(dir.path()),
            vec!["bridge.sqlite.v1.bak", "bridge.sqlite.v2.bak"]
        );
    }

    #[test]
    fn a_backup_that_cannot_be_pruned_does_not_stop_the_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_2_file(&path);
        // something the prune cannot delete: a directory under a backup's name
        std::fs::create_dir_all(dir.path().join("bridge.sqlite.v0.bak/held")).unwrap();
        std::fs::write(dir.path().join("bridge.sqlite.v1.bak"), b"older").unwrap();
        let db = Db::open(&path).expect("opened despite the stuck backup");
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn every_open_failure_has_a_persian_reason() {
        let newer = BridgeError::DbNewer {
            found: 9,
            supported: 3,
        };
        assert_eq!(open_failure_fa(&newer), newer.message_fa());
        let full = open_failure_fa(&BridgeError::Db("database or disk is full".into()));
        assert!(full.contains("فضای خالی دیسک"), "{full}");
        assert!(full.contains("database or disk is full"), "{full}");
    }

    #[test]
    fn self_check_creates_and_migrates_a_fresh_database() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(crate::self_check(dir.path()), Ok(SCHEMA_VERSION));
        assert!(dir.path().join("bridge.sqlite").is_file());
    }

    #[test]
    fn migration_3_drops_the_map_and_turns_map_links_into_auto() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_2_file(&path);
        let before = Db::open(&path).unwrap();
        assert_eq!(before.user_version().unwrap(), SCHEMA_VERSION);

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

    /// A database at version 3, as v1.1.1 leaves it: a seller's pick, an automatic link, a «none»,
    /// and a job whose review saved the pick's product among its candidates (brand and alias).
    fn version_3_file(path: &Path) {
        let c = Connection::open(path).unwrap();
        for sql in &MIGRATIONS[..3] {
            c.execute_batch(sql).unwrap();
        }
        c.pragma_update(None, "user_version", 3).unwrap();
        c.execute_batch(
            r#"DROP TABLE IF EXISTS matches_old;
             INSERT INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, priced_at, decided_at) VALUES
               ('barezpakhsh', 'u', '33102018', 'p_user', 'بلبرینگ بزرگ چرخ عقب پراید', 'USER', 56, 'u', NULL, '2026-10-07T16:30:00Z'),
               ('barezpakhsh', 'a', 'A1', 'p_auto', 'auto part', 'AUTO', 91, 'a', '2026-10-08T00:00:00Z', '2026-10-07T16:30:00Z'),
               ('barezpakhsh', 'n', 'N1', NULL, NULL, 'NONE', NULL, 'n', NULL, '2026-10-07T16:30:00Z');
             INSERT INTO jobs(id, kind, mode, store, status, phase, started_at) VALUES (2, 'IMPORT', 'ZERO', 's', 'DONE', 'REPORT', '2026-10-07T03:38:00Z');
             INSERT INTO job_items(job_id, code, name, qty, status, candidates_json) VALUES
               (2, '33102018', 'u', 0, 'DONE', '[{"candidate":{"id":"p_user","name":"بلبرینگ بزرگ چرخ عقب پراید","alias":"بلبرینگ 44649","brand":"PSN عظام","score":0.0},"similarity":56},{"candidate":{"id":"p_x","name":"x","alias":null,"brand":null,"score":0.0},"similarity":40}]'),
               (2, 'A9', 'other', 0, 'DONE', 'not json');"#,
        )
        .unwrap();
    }

    fn table_exists(db: &Db, name: &str) -> bool {
        db.with(|c| {
            c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
        })
        .unwrap()
            > 0
    }

    /// Plan 130 task 2 (R-a, R-n): every decision survives the move, every link is `UNVERIFIED`,
    /// identity comes from what review recorded, and `matches` is gone — so a pre-guard build
    /// finds no link to write from.
    #[test]
    fn migration_4_keeps_every_decision_unverified_and_backfills_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_3_file(&path);
        let db = Db::open(&path).unwrap();
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
        assert!(!table_exists(&db, "matches"), "old binaries find no links");
        assert!(!table_exists(&db, "link_identity_backfill"));
        let all = db.list_matches("barezpakhsh").unwrap();
        assert_eq!(all.len(), 3, "every decision kept");
        assert!(all.iter().all(|m| m.state == "UNVERIFIED"));
        assert!(all.iter().all(|m| m.validated_job.is_none()));
        let u = db.match_for_name("barezpakhsh", "u").unwrap().unwrap();
        assert_eq!(u.how, "USER");
        assert_eq!(u.similarity, Some(56));
        assert_eq!(u.product_brand.as_deref(), Some("PSN عظام"));
        assert_eq!(u.product_alias.as_deref(), Some("بلبرینگ 44649"));
        let a = db.match_for_name("barezpakhsh", "a").unwrap().unwrap();
        assert_eq!(a.product_brand, None, "never recorded stays NULL");
        assert_eq!(a.priced_at.as_deref(), Some("2026-10-08T00:00:00Z"));
        let n = db.match_for_name("barezpakhsh", "n").unwrap().unwrap();
        assert_eq!((n.how.as_str(), n.product_id), ("NONE", None));
        assert!(all.iter().all(|m| !m.writable_for(
            2,
            m.link_version,
            m.product_id.as_deref().unwrap_or("")
        )));
    }

    /// Plan 130 task 2: a migration cut halfway leaves version 3 whole; the next launch finishes it.
    #[test]
    fn an_interrupted_migration_4_leaves_version_3_whole_and_completes_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_3_file(&path);
        // the migration's last table already exists: 0004 fails at its end, after the copy and drop
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE match_events (x INTEGER);")
            .unwrap();
        assert!(Db::open(&path).is_err());
        let c = Connection::open(&path).unwrap();
        let v: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 3, "rolled back whole");
        let links: i64 = c
            .query_row("SELECT count(*) FROM matches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 3, "matches untouched");
        c.execute_batch("DROP TABLE match_events;").unwrap();
        drop(c);
        let db = Db::open(&path).unwrap();
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(db.list_matches("barezpakhsh").unwrap().len(), 3);
    }

    /// Plan 130 task 2 (R-n): a pre-upgrade backup put back re-migrates, every link `UNVERIFIED`
    /// again — whatever the upgraded copy had proven since.
    #[test]
    fn a_restored_pre_upgrade_backup_re_migrates_with_every_link_unverified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_3_file(&path);
        let db = Db::open(&path).unwrap();
        db.validate_link(
            "barezpakhsh",
            "a",
            1,
            matches::LinkProduct {
                id: "p_auto",
                name: "auto part",
                brand: Some(""),
                alias: Some(""),
            },
            matches::state::VALID,
            None,
            9,
        )
        .unwrap()
        .expect("still the migrated link");
        drop(db);
        let backup = dir.path().join("bridge.sqlite.v3.bak");
        assert!(backup.is_file());
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.path().join(format!("bridge.sqlite{suffix}")));
        }
        std::fs::copy(&backup, &path).unwrap();
        let db = Db::open(&path).unwrap();
        let all = db.list_matches("barezpakhsh").unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|m| m.state == "UNVERIFIED"));
    }

    /// Plan 130 task 14: the first migration a launch applies records the schema the install came
    /// from, in that migration's own transaction and keeping the lowest value — so a launch cut
    /// halfway through an upgrade still leaves the record, and the next launch does not overwrite it.
    #[test]
    fn an_upgrade_records_the_schema_it_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sqlite");
        version_3_file(&path);
        let db = Db::open(&path).unwrap();
        assert_eq!(
            db.setting(UPGRADED_FROM_SCHEMA).unwrap().as_deref(),
            Some("3")
        );

        // cut after 0004: version 4 on disk, the record already there; the rest lands next launch
        let path2 = dir.path().join("cut.sqlite");
        version_3_file(&path2);
        Connection::open(&path2)
            .unwrap()
            .execute_batch("CREATE TABLE write_intents (x INTEGER);")
            .unwrap();
        assert!(Db::open(&path2).is_err(), "0005 fails on the stray table");
        let c = Connection::open(&path2).unwrap();
        let (v, rec): (i64, String) = (
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
            c.query_row(
                "SELECT value FROM settings WHERE key = 'upgraded_from_schema'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        );
        assert_eq!((v, rec.as_str()), (4, "3"), "0004 landed with its record");
        c.execute_batch("DROP TABLE write_intents;").unwrap();
        drop(c);
        let db = Db::open(&path2).unwrap();
        assert_eq!(db.user_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(
            db.setting(UPGRADED_FROM_SCHEMA).unwrap().as_deref(),
            Some("3"),
            "the lowest kept, never 4"
        );

        // a new install and an up-to-date one record nothing
        assert_eq!(
            Db::in_memory()
                .unwrap()
                .setting(UPGRADED_FROM_SCHEMA)
                .unwrap(),
            None
        );
        let fresh = dir.path().join("fresh.sqlite");
        drop(Db::open(&fresh).unwrap());
        assert_eq!(
            Db::open(&fresh)
                .unwrap()
                .setting(UPGRADED_FROM_SCHEMA)
                .unwrap(),
            None
        );
    }
}
