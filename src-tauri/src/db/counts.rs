//! The seller's stock counts (plan 130 PI-19): the word that a shelf row's stock was counted after
//! the transfers listed with it — units quantity imports moved onto the row from items whose link
//! does not prove there. The audit leaves a counted transfer out of the row's doubt, and lists a
//! transfer made later again. A count is withdrawn, never deleted. Nothing here reaches Landau.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::jobs::shelf_writes_on;
use super::{now, Db};
use crate::errors::Result;

/// One count, as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitCountRow {
    pub id: i64,
    pub store: String,
    pub stock_id: String,
    /// The row's inventory as the audit read it when the seller said it was counted.
    pub inventory: i64,
    pub read_at: String,
    pub counted_at: String,
    /// `(job item, units)`: the transfers the count covers.
    pub items: Vec<(i64, i64)>,
}

impl Db {
    /// The seller's count of `stock_id` in `store` over exactly `items` (`(job item, units)`), with
    /// the row's `inventory` as the audit read it at `read_at`. Recorded only when each item is a
    /// quantity import's write onto that row of those units and no count still standing covers it;
    /// answers the count's id and when it was recorded, or `None` with nothing recorded.
    pub fn record_count(
        &self,
        store: &str,
        stock_id: &str,
        inventory: i64,
        read_at: &str,
        items: &[(i64, i64)],
    ) -> Result<Option<(i64, String)>> {
        if items.is_empty() {
            return Ok(None);
        }
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let transfers: HashMap<i64, i64> = shelf_writes_on(&tx)?
                .into_iter()
                .filter(|w| {
                    w.stock_id == stock_id
                        && w.kind == "IMPORT"
                        && w.mode.as_deref() == Some("QUANTITY")
                })
                .map(|w| (w.item_id, w.qty))
                .collect();
            for (item, qty) in items {
                if transfers.get(item) != Some(qty) {
                    return Ok(None);
                }
                let counted: Option<i64> = tx
                    .query_row(
                        "SELECT c.id FROM unit_count_items i JOIN unit_counts c ON c.id = i.count_id
                         WHERE i.item_id = ?1 AND c.withdrawn_at IS NULL",
                        [item],
                        |r| r.get(0),
                    )
                    .optional()?;
                if counted.is_some() {
                    return Ok(None);
                }
            }
            let at = now();
            tx.execute(
                "INSERT INTO unit_counts(store, stock_id, inventory, read_at, counted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![store, stock_id, inventory, read_at, at],
            )?;
            let id = tx.last_insert_rowid();
            for (item, qty) in items {
                tx.execute(
                    "INSERT INTO unit_count_items(count_id, item_id, qty) VALUES (?1, ?2, ?3)",
                    params![id, item, qty],
                )?;
            }
            tx.commit()?;
            Ok(Some((id, at)))
        })
    }

    /// The seller takes a count back: stamped withdrawn, kept. `false` when no count still
    /// standing has that id.
    pub fn withdraw_count(&self, id: i64) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE unit_counts SET withdrawn_at = ?2 WHERE id = ?1 AND withdrawn_at IS NULL",
                params![id, now()],
            )
            .map(|n| n == 1)
        })
    }

    /// The counts still standing in `store`, newest first, with their transfers.
    pub fn standing_counts(&self, store: &str) -> Result<Vec<UnitCountRow>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, store, stock_id, inventory, read_at, counted_at FROM unit_counts
                 WHERE store = ?1 AND withdrawn_at IS NULL ORDER BY id DESC",
            )?;
            let mut counts: Vec<UnitCountRow> = st
                .query_map([store], |r| {
                    Ok(UnitCountRow {
                        id: r.get(0)?,
                        store: r.get(1)?,
                        stock_id: r.get(2)?,
                        inventory: r.get(3)?,
                        read_at: r.get(4)?,
                        counted_at: r.get(5)?,
                        items: Vec::new(),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            let mut st = c.prepare(
                "SELECT item_id, qty FROM unit_count_items WHERE count_id = ?1 ORDER BY item_id",
            )?;
            for count in &mut counts {
                count.items = st
                    .query_map([count.id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
            }
            Ok(counts)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{i, t};

    /// A quantity import's write of `qty` units onto `stock`; its item id.
    fn transfer(db: &Db, job: i64, stock: &str, qty: i64) -> i64 {
        let id = db.add_job_item(job, "C1", "قطعه", qty).unwrap();
        db.update_job_item(
            id,
            &[
                ("stock_id", t(stock)),
                ("cost", i(1_000)),
                ("status", t("DONE")),
            ],
        )
        .unwrap();
        id
    }

    /// PI-19: a count is recorded only over writes a quantity import made onto that row, of the
    /// units they moved, none counted already; withdrawn, it is kept and stops standing, and its
    /// transfers can be counted again.
    #[test]
    fn a_count_covers_only_transfers_onto_its_row_once() {
        let db = Db::in_memory().unwrap();
        let job = db
            .create_job("IMPORT", Some("QUANTITY"), Some("1405/08/15"), "st", false)
            .unwrap();
        let a = transfer(&db, job, "row-1", 3);
        let b = transfer(&db, job, "row-1", -1);
        let other = transfer(&db, job, "row-2", 2);
        db.set_job_status(job, "DONE", "REPORT").unwrap();
        let zero = db
            .create_job("IMPORT", Some("ZERO"), None, "st", false)
            .unwrap();
        let attach = transfer(&db, zero, "row-1", 5);
        db.set_job_status(zero, "DONE", "REPORT").unwrap();
        let at = "2026-11-10T08:00:00.000Z";
        let record = |items: &[(i64, i64)]| {
            db.record_count("st", "row-1", 7, at, items)
                .unwrap()
                .map(|(id, _)| id)
        };

        assert_eq!(record(&[]), None, "nothing to count");
        assert_eq!(record(&[(other, 2)]), None, "a write onto another row");
        assert_eq!(record(&[(a, 4)]), None, "other units than it moved");
        assert_eq!(record(&[(attach, 5)]), None, "a zero import moves no units");
        assert_eq!(record(&[(999, 1)]), None, "no such item");
        assert!(db.standing_counts("st").unwrap().is_empty());

        let id = record(&[(a, 3), (b, -1)]).expect("recorded");
        assert_eq!(record(&[(a, 3)]), None, "counted already");
        let standing = db.standing_counts("st").unwrap();
        assert_eq!(standing.len(), 1);
        let c = &standing[0];
        assert_eq!(
            (c.id, c.stock_id.as_str(), c.inventory, c.read_at.as_str()),
            (id, "row-1", 7, at)
        );
        assert_eq!(c.items, [(a, 3), (b, -1)]);
        assert!(
            db.standing_counts("another").unwrap().is_empty(),
            "per store"
        );

        assert!(db.withdraw_count(id).unwrap());
        assert!(!db.withdraw_count(id).unwrap(), "withdrawn once");
        assert!(db.standing_counts("st").unwrap().is_empty());
        let kept: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM unit_counts", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(kept, 1, "a withdrawn count is kept");
        assert!(
            record(&[(a, 3)]).is_some(),
            "counted again after a withdrawal"
        );
    }
}
