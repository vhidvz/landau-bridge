//! Write intents (plan 130 task 10, R-g, R-o): a shelf write recorded before it is sent — the row
//! as read right before it (the snapshot) and the absolute values the write sets (the target) —
//! closed only when its outcome is known. A reply closes it; a lost reply, a crash or a 5xx leaves
//! it `OPEN` for the next job's reconciliation, which reads the row and compares it with both.

use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;
use serde_json::{json, Value};

use super::{now, Db};
use crate::errors::{BridgeError, Result};

pub mod status {
    pub const OPEN: &str = "OPEN";
    /// The row holds the target: the write landed (a reply, or a lost one reconciled).
    pub const LANDED: &str = "LANDED";
    /// The write was refused, or the row still holds the snapshot: nothing landed.
    pub const NOT_LANDED: &str = "NOT_LANDED";
    /// The row holds neither: something else changed it. Held for the seller until resolved.
    pub const SUPERSEDED: &str = "SUPERSEDED";
}

/// How the seller settled a `SUPERSEDED` intent.
pub mod resolution {
    /// "It is on the shelf": the intent's invoice lines count as transferred.
    pub const ON_SHELF: &str = "ON_SHELF";
    /// "Transfer it again": the lines are released for the next import.
    pub const TRANSFER_AGAIN: &str = "TRANSFER_AGAIN";
}

/// Where a row was when it was read.
pub mod place {
    pub const LIVE: &str = "LIVE";
    /// Soft-deleted: in the seller's trash, still holding Landau's uniqueness index.
    pub const TRASH: &str = "TRASH";
    pub const ABSENT: &str = "ABSENT";
}

/// One read of a shelf row, as an intent keeps it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowState {
    /// [`place`]: `LIVE`, `TRASH` or `ABSENT`.
    pub place: String,
    pub stock_id: Option<String>,
    pub inventory: Option<i64>,
    pub cost: Option<i64>,
    pub updated_at: Option<String>,
}

impl RowState {
    pub fn absent() -> Self {
        RowState {
            place: place::ABSENT.into(),
            stock_id: None,
            inventory: None,
            cost: None,
            updated_at: None,
        }
    }

    pub fn of(row: &crate::landau::ShelfRow, place: &str) -> Self {
        RowState {
            place: place.into(),
            stock_id: Some(row.id.clone()),
            inventory: Some(row.inventory),
            cost: row.cost,
            updated_at: row.updated_at.clone(),
        }
    }
}

/// What a write is about to do, recorded before it is sent.
#[derive(Debug, Clone)]
pub struct NewIntent<'a> {
    pub job_id: i64,
    /// `IMPORT` | `REFRESH`.
    pub kind: &'a str,
    pub store: &'a str,
    pub product: &'a str,
    pub authenticity: &'a str,
    /// `ATTACH` | `PATCH` | `RESTORE`.
    pub verb: &'a str,
    pub snapshot: &'a RowState,
    /// `None`: the write leaves the row's inventory alone.
    pub target_inventory: Option<i64>,
    pub target_cost: i64,
    pub items: &'a [i64],
    /// The `(invoice, code)` lines this write transfers; marked when it lands.
    pub lines: &'a [(i64, String)],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntentRow {
    pub id: i64,
    pub job_id: i64,
    pub kind: String,
    pub store: String,
    pub product: String,
    pub authenticity: String,
    pub verb: String,
    pub snapshot: RowState,
    pub target_inventory: Option<i64>,
    pub target_cost: i64,
    pub items: Vec<i64>,
    pub lines: Vec<(i64, String)>,
    pub status: String,
    pub outcome: Option<Value>,
    pub opened_at: String,
    /// The last send whose outcome stayed unknown; reconciliation waits out the mirror's lag
    /// from here before it believes "never landed".
    pub attempted_at: Option<String>,
    pub closed_at: Option<String>,
    pub resolved_at: Option<String>,
    pub resolution: Option<String>,
}

const COLUMNS: &str =
    "id, job_id, kind, store, product, authenticity, stock_id, verb, snap_where, \
    snap_inventory, snap_cost, snap_updated_at, target_inventory, target_cost, items_json, \
    lines_json, status, outcome_json, opened_at, closed_at, resolved_at, resolution, attempted_at";

fn intent_from(r: &Row<'_>) -> rusqlite::Result<IntentRow> {
    let items: String = r.get(14)?;
    let lines: String = r.get(15)?;
    let outcome: Option<String> = r.get(17)?;
    Ok(IntentRow {
        id: r.get(0)?,
        job_id: r.get(1)?,
        kind: r.get(2)?,
        store: r.get(3)?,
        product: r.get(4)?,
        authenticity: r.get(5)?,
        verb: r.get(7)?,
        snapshot: RowState {
            place: r.get(8)?,
            stock_id: r.get(6)?,
            inventory: r.get(9)?,
            cost: r.get(10)?,
            updated_at: r.get(11)?,
        },
        target_inventory: r.get(12)?,
        target_cost: r.get(13)?,
        items: serde_json::from_str(&items).unwrap_or_default(),
        lines: serde_json::from_str(&lines).unwrap_or_default(),
        status: r.get(16)?,
        outcome: outcome.and_then(|s| serde_json::from_str(&s).ok()),
        opened_at: r.get(18)?,
        closed_at: r.get(19)?,
        resolved_at: r.get(20)?,
        resolution: r.get(21)?,
        attempted_at: r.get(22)?,
    })
}

impl Db {
    /// Record a write before it is sent (R-g). The intent is `OPEN` until closed.
    pub fn open_intent(&self, n: &NewIntent<'_>) -> Result<i64> {
        let items = json!(n.items).to_string();
        let lines = json!(n.lines).to_string();
        self.with(|c| {
            c.execute(
                "INSERT INTO write_intents(job_id, kind, store, product, authenticity, stock_id, verb,
                    snap_where, snap_inventory, snap_cost, snap_updated_at, target_inventory,
                    target_cost, items_json, lines_json, status, opened_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 'OPEN', ?16)",
                params![
                    n.job_id,
                    n.kind,
                    n.store,
                    n.product,
                    n.authenticity,
                    n.snapshot.stock_id,
                    n.verb,
                    n.snapshot.place,
                    n.snapshot.inventory,
                    n.snapshot.cost,
                    n.snapshot.updated_at,
                    n.target_inventory,
                    n.target_cost,
                    items,
                    lines,
                    now(),
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// An open intent re-planned before anything of it landed: a 409 showed a row the reads
    /// missed, so the write becomes a patch (or a restore) of that row from its own values.
    pub fn replan_intent(
        &self,
        id: i64,
        verb: &str,
        snapshot: &RowState,
        target_inventory: Option<i64>,
        target_cost: i64,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE write_intents SET verb = ?2, stock_id = ?3, snap_where = ?4,
                    snap_inventory = ?5, snap_cost = ?6, snap_updated_at = ?7,
                    target_inventory = ?8, target_cost = ?9
                 WHERE id = ?1 AND status = 'OPEN'",
                params![
                    id,
                    verb,
                    snapshot.stock_id,
                    snapshot.place,
                    snapshot.inventory,
                    snapshot.cost,
                    snapshot.updated_at,
                    target_inventory,
                    target_cost,
                ],
            )?;
            Ok(())
        })
    }

    /// A send whose outcome stayed unknown, stamped now.
    pub fn intent_attempted(&self, id: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE write_intents SET attempted_at = ?2 WHERE id = ?1",
                params![id, now()],
            )?;
            Ok(())
        })
    }

    /// The row id an attach's reply named, so reconciliation can find the row by id.
    pub fn set_intent_stock(&self, id: i64, stock_id: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE write_intents SET stock_id = ?2 WHERE id = ?1",
                params![id, stock_id],
            )?;
            Ok(())
        })
    }

    /// Close an open intent. A `LANDED` close marks its invoice lines transferred in the same
    /// transaction (PI-7: no crash can land the write and lose the guard). `false` when the
    /// intent was no longer open (closed by another path).
    pub fn close_intent(&self, id: i64, status: &str, outcome: &Value) -> Result<bool> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let closed = tx.execute(
                "UPDATE write_intents SET status = ?2, outcome_json = ?3, closed_at = ?4
                 WHERE id = ?1 AND status = 'OPEN'",
                params![id, status, outcome.to_string(), now()],
            )? == 1;
            if closed && status == status::LANDED {
                mark_lines(&tx, id)?;
            }
            tx.commit()?;
            Ok(closed)
        })
    }

    pub fn intent(&self, id: i64) -> Result<Option<IntentRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLUMNS} FROM write_intents WHERE id = ?1"),
                [id],
                intent_from,
            )
            .optional()
        })
    }

    /// Every intent whose outcome is still unknown, oldest first (R-o).
    pub fn open_intents(&self) -> Result<Vec<IntentRow>> {
        self.intents_where("status = 'OPEN'")
    }

    /// Every intent superseded by an external change and not yet settled by the seller: their
    /// rows and invoice lines are held by every job until then.
    pub fn unresolved_superseded(&self) -> Result<Vec<IntentRow>> {
        self.intents_where("status = 'SUPERSEDED' AND resolved_at IS NULL")
    }

    fn intents_where(&self, filter: &str) -> Result<Vec<IntentRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLUMNS} FROM write_intents WHERE {filter} ORDER BY id"
            ))?;
            let rows = st.query_map([], intent_from)?;
            rows.collect()
        })
    }

    /// The seller settles a superseded intent: `ON_SHELF` marks its lines transferred,
    /// `TRANSFER_AGAIN` releases them. `false` when it was not an unresolved superseded intent.
    /// Refused while a job runs or waits in review: that job aggregated its quantities before
    /// the settling, and marking lines under it would let it add them anyway.
    pub fn resolve_intent(&self, id: i64, how: &str) -> Result<bool> {
        if how != resolution::ON_SHELF && how != resolution::TRANSFER_AGAIN {
            return Err(BridgeError::Other(format!("unknown resolution {how}")));
        }
        let outcome = self.with(|c| {
            let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
            let running: Option<i64> = tx
                .query_row(
                    "SELECT id FROM jobs WHERE status IN ('RUNNING', 'REVIEW') ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(job) = running {
                return Ok(Err(job));
            }
            let done = tx.execute(
                "UPDATE write_intents SET resolved_at = ?2, resolution = ?3
                 WHERE id = ?1 AND status = 'SUPERSEDED' AND resolved_at IS NULL",
                params![id, now(), how],
            )? == 1;
            if done && how == resolution::ON_SHELF {
                mark_lines(&tx, id)?;
            }
            tx.commit()?;
            Ok(Ok(done))
        })?;
        outcome.map_err(|job| BridgeError::JobBusy(format!("#{job}")))
    }
}

/// The intent's invoice lines into the transfer guard, under the job that made the write.
fn mark_lines(c: &rusqlite::Connection, id: i64) -> rusqlite::Result<()> {
    let (job, lines): (i64, String) = c.query_row(
        "SELECT job_id, lines_json FROM write_intents WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let lines: Vec<(i64, String)> = serde_json::from_str(&lines).unwrap_or_default();
    let mut st = c.prepare(
        "INSERT OR IGNORE INTO invoice_imports(invoice_id, code, job_id) VALUES (?1, ?2, ?3)",
    )?;
    for (invoice, code) in &lines {
        st.execute(params![invoice, code, job])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InvoiceHeader, InvoiceKind};

    fn invoice(db: &Db, number: &str) -> i64 {
        db.upsert_invoice(
            "barezpakhsh",
            &InvoiceHeader {
                number: number.into(),
                kind: InvoiceKind::Sale,
                date_jalali: "1404/08/10".into(),
                date_iso: "2025-11-01".into(),
                paid: true,
                url: format!("/f/{number}"),
            },
        )
        .unwrap()
    }

    fn open(db: &Db, lines: &[(i64, String)]) -> i64 {
        let snap = RowState {
            place: place::LIVE.into(),
            stock_id: Some("s1".into()),
            inventory: Some(3),
            cost: Some(900),
            updated_at: Some("2026-11-09T08:00:00.000Z".into()),
        };
        db.open_intent(&NewIntent {
            job_id: 7,
            kind: "IMPORT",
            store: "store",
            product: "p1",
            authenticity: "ORIGINAL",
            verb: "PATCH",
            snapshot: &snap,
            target_inventory: Some(5),
            target_cost: 1_000,
            items: &[11, 12],
            lines,
        })
        .unwrap()
    }

    #[test]
    fn an_intent_round_trips_and_lists_open_until_closed() {
        let db = Db::in_memory().unwrap();
        let id = open(&db, &[]);
        let got = db.intent(id).unwrap().unwrap();
        assert_eq!(got.status, status::OPEN);
        assert_eq!(got.snapshot.inventory, Some(3));
        assert_eq!(got.snapshot.stock_id.as_deref(), Some("s1"));
        assert_eq!((got.target_inventory, got.target_cost), (Some(5), 1_000));
        assert_eq!(got.items, vec![11, 12]);
        assert_eq!(db.open_intents().unwrap().len(), 1);
        assert!(db
            .close_intent(id, status::NOT_LANDED, &json!({"why": "refused"}))
            .unwrap());
        assert!(db.open_intents().unwrap().is_empty());
        // a second close finds nothing open
        assert!(!db.close_intent(id, status::LANDED, &json!({})).unwrap());
        assert_eq!(db.intent(id).unwrap().unwrap().status, status::NOT_LANDED);
    }

    /// PI-7: a landing and its transfer guard are one transaction.
    #[test]
    fn a_landed_close_marks_its_lines_and_no_other_close_does() {
        let db = Db::in_memory().unwrap();
        let (a, b) = (invoice(&db, "1001"), invoice(&db, "1002"));
        let landed = open(&db, &[(a, "C1".into())]);
        let refused = open(&db, &[(b, "C1".into())]);
        db.close_intent(landed, status::LANDED, &json!({})).unwrap();
        db.close_intent(refused, status::NOT_LANDED, &json!({}))
            .unwrap();
        let marked = db.imported_lines().unwrap();
        assert!(marked.contains(&(a, "C1".into())));
        assert!(!marked.contains(&(b, "C1".into())));
    }

    #[test]
    fn a_superseded_intent_is_held_until_the_seller_resolves_it() {
        let db = Db::in_memory().unwrap();
        let (a, b) = (invoice(&db, "2001"), invoice(&db, "2002"));
        let shelf = open(&db, &[(a, "C1".into())]);
        let again = open(&db, &[(b, "C1".into())]);
        for id in [shelf, again] {
            db.close_intent(id, status::SUPERSEDED, &json!({})).unwrap();
        }
        assert!(
            db.imported_lines().unwrap().is_empty(),
            "nothing marked yet"
        );
        assert_eq!(db.unresolved_superseded().unwrap().len(), 2);
        assert!(db.resolve_intent(shelf, resolution::ON_SHELF).unwrap());
        assert!(db
            .resolve_intent(again, resolution::TRANSFER_AGAIN)
            .unwrap());
        assert!(
            !db.resolve_intent(shelf, resolution::TRANSFER_AGAIN)
                .unwrap(),
            "resolved once"
        );
        let marked = db.imported_lines().unwrap();
        assert!(marked.contains(&(a, "C1".into())));
        assert!(!marked.contains(&(b, "C1".into())), "released, not marked");
        assert!(db.unresolved_superseded().unwrap().is_empty());
        assert!(db.resolve_intent(shelf, "WHATEVER").is_err());
    }

    #[test]
    fn nothing_is_resolved_while_a_job_runs() {
        let db = Db::in_memory().unwrap();
        let a = invoice(&db, "3001");
        let id = open(&db, &[(a, "C1".into())]);
        db.close_intent(id, status::SUPERSEDED, &json!({})).unwrap();
        let job = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        assert_eq!(
            db.resolve_intent(id, resolution::ON_SHELF),
            Err(BridgeError::JobBusy(format!("#{job}")))
        );
        assert!(db.imported_lines().unwrap().is_empty());
        db.set_job_status(job, "DONE", "REPORT").unwrap();
        assert!(db.resolve_intent(id, resolution::ON_SHELF).unwrap());
    }

    #[test]
    fn only_a_superseded_intent_can_be_resolved() {
        let db = Db::in_memory().unwrap();
        let id = open(&db, &[]);
        assert!(!db.resolve_intent(id, resolution::ON_SHELF).unwrap());
        db.close_intent(id, status::LANDED, &json!({})).unwrap();
        assert!(!db.resolve_intent(id, resolution::ON_SHELF).unwrap());
    }

    #[test]
    fn a_replan_rewrites_an_open_intent_only() {
        let db = Db::in_memory().unwrap();
        let id = open(&db, &[]);
        let trashed = RowState {
            place: place::TRASH.into(),
            stock_id: Some("s9".into()),
            inventory: Some(2),
            cost: Some(700),
            updated_at: None,
        };
        db.replan_intent(id, "RESTORE", &trashed, Some(4), 1_100)
            .unwrap();
        let got = db.intent(id).unwrap().unwrap();
        assert_eq!(got.verb, "RESTORE");
        assert_eq!(got.snapshot, trashed);
        assert_eq!((got.target_inventory, got.target_cost), (Some(4), 1_100));
        db.close_intent(id, status::LANDED, &json!({})).unwrap();
        db.replan_intent(id, "PATCH", &RowState::absent(), None, 1)
            .unwrap();
        assert_eq!(db.intent(id).unwrap().unwrap().verb, "RESTORE");
    }
}
