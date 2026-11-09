//! The one module that writes a seller's shelf (plan 130 R-l). Every path that sets a row's cost
//! or quantity — IMPORT's attach, attach → 409 → patch, 409 → restore → patch and plain patch, and
//! REFRESH's patch (fact 17) — runs here, and only from a [`ProvenWrite`]. The client's write
//! methods are visible to this module alone, so a new path cannot bypass the proof and compile.
//!
//! Right before each write the links the proof rests on are read again: one changed since it was
//! proven refuses the write (`LinkChanged`). A 404 on an attach means the product is gone, so its
//! links turn `STALE`; a 404 on a patch or a restore means the row is gone (`RowChanged`).
//!
//! Task 10 (R-g, R-o, R-p): before a write the row is read again (the fresh read) and the write is
//! planned from what it holds now, never from the job's earlier read; the write is recorded as an
//! intent — that row as the snapshot, the absolute values it sets as the target — and closed only
//! when its outcome is known. An answer after an attempt Landau may have applied unseen (no reply,
//! a 5xx, a 408) leaves the intent open, and the next job reads the row before planning anything
//! ([`crate::jobs::reconcile`]). A row whose earlier write is unsettled is not written again.

use std::time::Duration;

use serde_json::{json, Value};

use super::{AttachBody, LandauClient, Narrow, PatchBody, ShelfRow};
use crate::db::intents::{place, status, IntentRow, NewIntent, RowState};
use crate::db::matches::{actor, state};
use crate::db::Db;
use crate::domain::{Authenticity, ImportMode};
use crate::errors::{BridgeError, Result};
use crate::jobs::planner::{find_row, floored, plan_write, shortfall, WriteAction};
use crate::jobs::pricing;
use crate::jobs::proof::{ProvenWrite, Target};

/// What one import write left behind, for the item's report columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub stock_id: String,
    pub qty_before: Option<i64>,
    pub qty_after: Option<i64>,
    pub price_before: Option<i64>,
    /// The row's cost before this write (plan 130 R-i); `None` for an attach.
    pub cost_before: Option<i64>,
    /// The price Landau answered on the write (plan 107); the REPRICE phase confirms it.
    pub price_after: Option<i64>,
    pub verb: &'static str,
    /// Units of a negative transfer the shelf could not absorb (0 unless the floor bit).
    pub shortfall: i64,
    /// The cost written, after the unavailable-only-raises hold (§14 d); `held` when it bit.
    pub cost: i64,
    pub held: bool,
    /// A soft-deleted row was put back to take this write.
    pub restored: bool,
    /// The row had changed since the job read the shelf: the write was planned from the fresh
    /// read (R-p), never from the earlier figures.
    pub replanned: bool,
}

/// What a refresh write did.
#[derive(Debug, Clone, PartialEq)]
pub enum Refreshed {
    /// The row already holds this cost: nothing sent.
    Unchanged { cost: i64, held: bool },
    Patched {
        cost: i64,
        held: bool,
        /// The cost the fresh read found, which this write replaced (plan 130 R-i's report).
        before: Option<i64>,
        price_after: Option<i64>,
        /// The row had changed since the job read the shelf (R-p).
        replanned: bool,
    },
}

/// What an import write needs beyond its proof.
#[derive(Debug, Clone, Copy)]
pub struct ImportWrite<'a> {
    pub store: &'a str,
    pub mode: ImportMode,
    pub available: bool,
    pub min_margin_percent: f64,
    /// The product's registry name — the fresh read's term.
    pub term: &'a str,
    /// The invoice lines this write transfers (quantity mode), marked with its landing.
    pub lines: &'a [(i64, String)],
}

/// A lost attach reply is looked for this many times, this far apart: the shelf is read from a
/// mirror about a second behind the write.
const LOST_REPLY_READS: usize = 3;
const LOST_REPLY_GAP: Duration = Duration::from_secs(1);

pub struct ShelfWriter<'a> {
    client: &'a LandauClient,
    db: &'a Db,
    supplier: &'a str,
    lost_reply_gap: Duration,
}

/// A 404 from Landau.
fn not_found(e: &BridgeError) -> bool {
    matches!(e, BridgeError::LandauRejected { status: 404, .. })
}

/// Landau's own words when an attach names a product its registry does not hold
/// (lnd-backend `StocksService.assertApprovedProduct`: "there is no such part in Landau’s
/// registry"). Only that 404 is the product gone: the same route answers 404 "store not found",
/// and a gateway or proxy answers 404 for a route it lacks — neither says anything of the link.
pub const NO_SUCH_PART: &str = "no such part in Landau";

fn product_gone(e: &BridgeError) -> bool {
    matches!(e, BridgeError::LandauRejected { status: 404, body } if body.contains(NO_SUCH_PART))
}

/// A row-level 404 (a patch or a restore): the row is gone, whatever the link says.
fn row_gone(e: BridgeError) -> BridgeError {
    if not_found(&e) {
        BridgeError::RowChanged("Landau no longer has this row (404)".into())
    } else {
        e
    }
}

/// Does `row` hold what a write set? Its cost always; its inventory when the write set one.
pub fn holds(row: &ShelfRow, target_inventory: Option<i64>, target_cost: i64) -> bool {
    row.cost == Some(target_cost) && target_inventory.is_none_or(|t| row.inventory == t)
}

/// Has a row changed between two reads of it (R-p)?
fn differs(before: &ShelfRow, now: &ShelfRow) -> bool {
    before.inventory != now.inventory
        || before.cost != now.cost
        || before.updated_at != now.updated_at
}

/// The import row's identity: `(product, NEW, authenticity)`, the whole unique tuple.
fn is_tuple(row: &ShelfRow, product: &str, authenticity: &str) -> bool {
    row.product == product
        && row.condition.as_deref() == Some("NEW")
        && row.authenticity.as_deref() == Some(authenticity)
}

/// The row a write is about to touch, as the hold check sees it.
struct RowKey<'a> {
    store: &'a str,
    product: &'a str,
    authenticity: &'a str,
    condition: Option<&'a str>,
    stock_id: Option<&'a str>,
}

/// An unsettled intent on the row (or on these invoice lines) that a new write must not cross:
/// one superseded by an external change and not yet settled by the seller, or one still open from
/// an earlier job (reconciliation runs first, so this is the guard behind it).
fn blocks(i: &IntentRow, job: i64, key: &RowKey<'_>, lines: &[(i64, String)]) -> bool {
    let superseded = i.status == status::SUPERSEDED && i.resolved_at.is_none();
    let open_before = i.status == status::OPEN && i.job_id != job;
    if !(superseded || open_before) {
        return false;
    }
    let by_id = key.stock_id.is_some() && i.snapshot.stock_id.as_deref() == key.stock_id;
    // an import intent names its row by the whole tuple, always NEW; a refresh intent by its id
    let by_tuple = i.kind == "IMPORT"
        && key.condition == Some("NEW")
        && i.product == key.product
        && i.authenticity == key.authenticity;
    let same_row = i.store == key.store && (by_id || by_tuple);
    // An open intent's lines are held in its own store only: one left on a store the key no
    // longer reaches holds nothing here. A superseded one's lines wherever they would go, until
    // the seller settles it — they may already be on that other shelf.
    let same_lines =
        (superseded || i.store == key.store) && lines.iter().any(|l| i.lines.contains(l));
    same_row || same_lines
}

impl<'a> ShelfWriter<'a> {
    pub fn new(client: &'a LandauClient, db: &'a Db, supplier: &'a str) -> Self {
        ShelfWriter {
            client,
            db,
            supplier,
            lost_reply_gap: LOST_REPLY_GAP,
        }
    }

    /// Shorter waits for a lost reply (the tests' stub has no mirror lag).
    #[cfg(test)]
    pub fn with_lost_reply_gap(mut self, gap: Duration) -> Self {
        self.lost_reply_gap = gap;
        self
    }

    /// Every link the write rests on is still the one this job proved, at its version (R-b, R-l).
    fn still_proven(&self, w: &ProvenWrite) -> Result<()> {
        for (key, version) in w.links() {
            let ok = self
                .db
                .link_by_key(self.supplier, key)?
                .is_some_and(|l| l.writable_for(w.job(), *version, w.product()));
            if !ok {
                return Err(BridgeError::LinkChanged);
            }
        }
        Ok(())
    }

    /// Refuse a write onto a row, or of invoice lines, whose earlier write is not settled (R-o).
    fn unsettled(&self, job: i64, key: &RowKey<'_>, lines: &[(i64, String)]) -> Result<()> {
        let mut pending = self.db.open_intents()?;
        pending.extend(self.db.unresolved_superseded()?);
        match pending.iter().find(|i| blocks(i, job, key, lines)) {
            Some(i) if i.status == status::OPEN => Err(BridgeError::RowChanged(format!(
                "write #{} of job #{} on this row has an unknown outcome — the next job reads \
                 the row and settles it before anything more is written here",
                i.id, i.job_id
            ))),
            Some(i) => Err(BridgeError::RowChanged(format!(
                "write #{} of job #{} on this row was overtaken by a change outside the bridge — \
                 the seller settles it before anything more is written here",
                i.id, i.job_id
            ))),
            None => Ok(()),
        }
    }

    /// An attach's 404 naming the product gone from Landau's registry: the links turn `STALE`
    /// (R-h). Any other failure stays a transfer failure.
    fn stale_links(&self, w: &ProvenWrite, product: &str, e: BridgeError) -> BridgeError {
        if !product_gone(&e) {
            return e;
        }
        for (key, _) in w.links() {
            if let Err(err) = self.db.set_link_state(
                self.supplier,
                key,
                state::STALE,
                Some(
                    json!({"why": "Landau answered 404 to the attach", "product": product})
                        .to_string(),
                ),
                Some(w.job()),
                actor::WRITE,
            ) {
                tracing::warn!(key, error = %err, "link not marked stale");
            }
        }
        BridgeError::StaleLink {
            product: product.to_string(),
        }
    }

    /// The write landed: its intent closes, and its invoice lines are marked in the same
    /// transaction. A failure here fails the item — never reported written with its lines
    /// unmarked — and leaves the intent open, which the next job settles.
    fn landed(&self, intent: i64, how: Value) -> Result<()> {
        self.db.close_intent(intent, status::LANDED, &how)?;
        Ok(())
    }

    /// The outcome is unknown: the intent stays open, stamped with when the write was last tried,
    /// so reconciliation waits out the mirror's lag before it believes "never landed".
    fn unknown(&self, intent: i64, e: &BridgeError) {
        tracing::warn!(intent, error = %e, "write outcome unknown; reconciled next job");
        if let Err(err) = self.db.intent_attempted(intent) {
            tracing::warn!(intent, error = %err, "write attempt not stamped");
        }
    }

    /// The write failed. Refused outright, nothing landed: the intent closes `NOT_LANDED`. After
    /// an attempt Landau may have applied unseen, the outcome is unknown: the intent stays open.
    fn unlanded(&self, intent: i64, uncertain: bool, e: &BridgeError) {
        if uncertain {
            self.unknown(intent, e);
            return;
        }
        let outcome = json!({"refused": e.kind(), "detail": e.to_string()});
        if let Err(err) = self.db.close_intent(intent, status::NOT_LANDED, &outcome) {
            tracing::warn!(intent, error = %err, "refused write left open");
        }
    }

    /// One patch of an intent's absolute target onto `stock_id`.
    async fn send_patch(
        &self,
        intent: i64,
        stock_id: &str,
        inventory: Option<i64>,
        cost: i64,
    ) -> Result<ShelfRow> {
        let sent = self
            .client
            .patch(
                stock_id,
                &PatchBody {
                    inventory,
                    cost: Some(cost),
                },
            )
            .await;
        match sent.result {
            Ok(reply) => {
                self.landed(intent, json!({"by": "reply", "stock_id": stock_id}))?;
                Ok(reply)
            }
            Err(e) => {
                self.unlanded(intent, sent.uncertain, &e);
                Err(row_gone(e))
            }
        }
    }

    /// IMPORT: the proven cost (and in quantity mode the summed quantity) onto the row for
    /// `(product, authenticity)` in the store, attaching it when the shelf lacks one. `shelf` is
    /// the job's local mirror and is kept current with what was read and written.
    pub async fn import(
        &self,
        w: &ProvenWrite,
        req: &ImportWrite<'_>,
        shelf: &mut Vec<ShelfRow>,
    ) -> Result<Written> {
        let Target::Row {
            product,
            authenticity,
            qty,
        } = w.target()
        else {
            return Err(BridgeError::Other(
                "a refresh write is not an import".into(),
            ));
        };
        let auth = authenticity.as_str();
        let lines: &[(i64, String)] = if req.mode == ImportMode::Quantity {
            req.lines
        } else {
            &[]
        };
        self.still_proven(w)?;
        let planned = find_row(shelf, product, auth).cloned();
        self.unsettled(
            w.job(),
            &RowKey {
                store: req.store,
                product,
                authenticity: auth,
                condition: Some("NEW"),
                stock_id: planned.as_ref().map(|r| r.id.as_str()),
            },
            lines,
        )?;
        // R-p: the row as it is now, not as the job read it at the start of WRITE
        let fresh = self
            .client
            .shelf_find(
                req.store,
                &Narrow {
                    term: req.term,
                    condition: Some("NEW"),
                    authenticity: Some(auth),
                },
                &|r| is_tuple(r, product, auth),
            )
            .await?;
        let (existing, replanned) = match (&planned, fresh) {
            (Some(p), None) => {
                return Err(BridgeError::RowChanged(format!(
                    "row {} left the shelf during the run",
                    p.id
                )))
            }
            (Some(p), Some(f)) if f.id != p.id => {
                return Err(BridgeError::RowChanged(format!(
                    "row {} was replaced by {} during the run",
                    p.id, f.id
                )))
            }
            (Some(p), Some(f)) => {
                let changed = differs(p, &f);
                merge_row(shelf, f.clone());
                (Some(f), changed)
            }
            (None, Some(f)) => {
                merge_row(shelf, f.clone());
                (Some(f), true)
            }
            (None, None) => (None, false),
        };
        // §14 d: an unavailable quote never lowers a row's cost.
        let (cost, held) = pricing::effective_cost(
            w.cost(),
            req.available,
            existing.as_ref().and_then(|r| r.cost),
        );
        let mut written = match plan_write(existing.as_ref(), *qty, req.mode) {
            WriteAction::Attach { inventory } => {
                let snapshot = RowState::absent();
                let intent = self.db.open_intent(&NewIntent {
                    job_id: w.job(),
                    kind: "IMPORT",
                    store: req.store,
                    product,
                    authenticity: auth,
                    verb: "ATTACH",
                    snapshot: &snapshot,
                    target_inventory: Some(inventory),
                    target_cost: cost,
                    items: w.items(),
                    lines,
                })?;
                self.attach(w, req, intent, *authenticity, inventory, cost, held, shelf)
                    .await?
            }
            WriteAction::Patch {
                stock_id,
                inventory,
                qty_before,
            } => {
                let row = existing.as_ref().ok_or_else(|| {
                    BridgeError::Other("a patch is planned only onto a row".into())
                })?;
                let snapshot = RowState::of(row, place::LIVE);
                let intent = self.db.open_intent(&NewIntent {
                    job_id: w.job(),
                    kind: "IMPORT",
                    store: req.store,
                    product,
                    authenticity: auth,
                    verb: "PATCH",
                    snapshot: &snapshot,
                    target_inventory: inventory,
                    target_cost: cost,
                    items: w.items(),
                    lines,
                })?;
                let reply = self.send_patch(intent, &stock_id, inventory, cost).await?;
                Written {
                    stock_id: stock_id.clone(),
                    qty_before: Some(qty_before),
                    qty_after: inventory.or(Some(qty_before)),
                    price_before: row.price,
                    cost_before: row.cost,
                    price_after: reply.price,
                    verb: "PATCH",
                    shortfall: shortfall(qty_before, *qty),
                    cost,
                    held,
                    restored: false,
                    replanned: false,
                }
            }
        };
        written.replanned |= replanned;
        // keep the local mirror current with what was written
        if let Some(pos) = shelf.iter().position(|r| r.id == written.stock_id) {
            if let Some(q) = written.qty_after {
                shelf[pos].inventory = q;
            }
            shelf[pos].cost = Some(written.cost);
        }
        Ok(written)
    }

    /// The attach of an intent, and the two ways a 409 answers it.
    #[allow(clippy::too_many_arguments)]
    async fn attach(
        &self,
        w: &ProvenWrite,
        req: &ImportWrite<'_>,
        intent: i64,
        authenticity: Authenticity,
        inventory: i64,
        cost: i64,
        held: bool,
        shelf: &mut Vec<ShelfRow>,
    ) -> Result<Written> {
        let Target::Row { product, qty, .. } = w.target() else {
            return Err(BridgeError::Other(
                "a refresh write is not an import".into(),
            ));
        };
        let body = AttachBody {
            product: product.clone(),
            store: req.store.to_string(),
            inventory,
            cost,
            price: pricing::floor_price(cost, req.min_margin_percent),
            authenticity,
        };
        let sent = self.client.attach(&body).await;
        let attached = |row: &ShelfRow| Written {
            stock_id: row.id.clone(),
            qty_before: None,
            qty_after: Some(inventory),
            price_before: None,
            cost_before: None,
            price_after: row.price,
            verb: "ATTACH",
            shortfall: shortfall(0, *qty),
            cost,
            held,
            restored: false,
            replanned: false,
        };
        match sent.result {
            Ok(row) => {
                if let Err(e) = self.db.set_intent_stock(intent, &row.id) {
                    tracing::warn!(intent, error = %e, "attach's row id not recorded");
                }
                self.landed(intent, json!({"by": "reply", "stock_id": row.id}))?;
                shelf.push(row.clone());
                Ok(attached(&row))
            }
            // PI-8: an earlier attempt may have created the row and lost its reply. The row at
            // the target is this attach; nothing more is sent, so nothing is added twice.
            Err(BridgeError::DuplicateRow) if sent.uncertain => {
                for n in 0..LOST_REPLY_READS {
                    if n > 0 {
                        tokio::time::sleep(self.lost_reply_gap).await;
                    }
                    let live = self.client.shelf(req.store).await?;
                    let Some(row) = find_row(&live, product, authenticity.as_str()).cloned() else {
                        continue;
                    };
                    if holds(&row, Some(inventory), cost) {
                        if let Err(e) = self.db.set_intent_stock(intent, &row.id) {
                            tracing::warn!(intent, error = %e, "attach's row id not recorded");
                        }
                        self.landed(
                            intent,
                            json!({"by": "lost reply: the row holds the target", "stock_id": row.id}),
                        )?;
                        merge_row(shelf, row.clone());
                        return Ok(attached(&row));
                    }
                    let now = RowState::of(&row, place::LIVE);
                    // the row's id first, so the hold this leaves names the row and not only its
                    // tuple
                    self.db.set_intent_stock(intent, &row.id)?;
                    self.db.close_intent(
                        intent,
                        status::SUPERSEDED,
                        &json!({"why": "after a lost attach reply the row holds other values", "row": now}),
                    )?;
                    merge_row(shelf, row.clone());
                    return Err(BridgeError::RowChanged(format!(
                        "after a lost reply row {} holds inventory {} and cost {:?}, not the {} \
                         and {} this attach set",
                        row.id, row.inventory, row.cost, inventory, cost
                    )));
                }
                // A tuple in the trash is what answered 409: Landau's index still holds a removed
                // row, so this attach cannot have landed. The restore path takes it from there.
                let trash = self.client.shelf_deleted(req.store).await?;
                if find_row(&trash, product, authenticity.as_str()).is_some() {
                    return self.existing_row(w, req, intent, authenticity, shelf).await;
                }
                let e = BridgeError::WriteUnconfirmed(
                    "Landau answered 409 after a lost attach reply and the row is not on the \
                     shelf yet"
                        .into(),
                );
                self.unknown(intent, &e);
                Err(e)
            }
            // The first attempt answered 409: the row existed before this write (the reads
            // missed it). Planned again from that row — a patch, or a restore out of the trash.
            Err(BridgeError::DuplicateRow) => {
                self.existing_row(w, req, intent, authenticity, shelf).await
            }
            Err(e) => {
                self.unlanded(intent, sent.uncertain, &e);
                Err(self.stale_links(w, product, e))
            }
        }
    }

    /// A 409 at the first attempt: the row exists. The live shelf is read again and the write
    /// re-planned from that row; a row only in the trash is restored first (spec §2.1 amendment:
    /// Landau's uniqueness index still holds a soft-deleted row, so an attach 409s for ever).
    async fn existing_row(
        &self,
        w: &ProvenWrite,
        req: &ImportWrite<'_>,
        intent: i64,
        authenticity: Authenticity,
        shelf: &mut Vec<ShelfRow>,
    ) -> Result<Written> {
        let Target::Row { product, qty, .. } = w.target() else {
            return Err(BridgeError::Other(
                "a refresh write is not an import".into(),
            ));
        };
        let auth = authenticity.as_str();
        // Nothing of this attach landed (the 409 came first, or from a row in the trash): any
        // failure before the next write closes the intent `NOT_LANDED`, so no later job reads
        // the existing row as a change made outside the bridge.
        let refused = |e: BridgeError| {
            self.unlanded(intent, false, &e);
            e
        };
        let fresh = self.client.shelf(req.store).await.map_err(refused)?;
        if let Some(row) = find_row(&fresh, product, auth).cloned() {
            // Only the row found is merged into the local shelf — replacing the whole mirror
            // would drop the figures this run has already written to sibling rows the lagging
            // read does not show yet.
            merge_row(shelf, row.clone());
            let (cost, held) = pricing::effective_cost(w.cost(), req.available, row.cost);
            let inv = (req.mode == ImportMode::Quantity).then_some(floored(row.inventory, *qty));
            self.db
                .replan_intent(intent, "PATCH", &RowState::of(&row, place::LIVE), inv, cost)
                .map_err(refused)?;
            let reply = self.send_patch(intent, &row.id, inv, cost).await?;
            return Ok(Written {
                stock_id: row.id.clone(),
                qty_before: Some(row.inventory),
                qty_after: inv.or(Some(row.inventory)),
                price_before: row.price,
                cost_before: row.cost,
                price_after: reply.price,
                verb: "PATCH",
                shortfall: shortfall(row.inventory, *qty),
                cost,
                held,
                restored: false,
                replanned: true,
            });
        }
        // Not live either: a seller who removed this exact part earlier and is now importing it
        // again. A restored row is a fresh attach: whatever it held when it was removed is gone
        // from the seller's point of view, so its inventory is set to this run's quantity (zero
        // in zero mode), not added to.
        let trash = self
            .client
            .shelf_deleted(req.store)
            .await
            .map_err(refused)?;
        let Some(removed) = find_row(&trash, product, auth).cloned() else {
            self.unlanded(intent, false, &BridgeError::DuplicateRow);
            return Err(BridgeError::DuplicateRow);
        };
        let (cost, held) = pricing::effective_cost(w.cost(), req.available, None);
        let inv = if req.mode == ImportMode::Quantity {
            floored(0, *qty)
        } else {
            0
        };
        self.db
            .replan_intent(
                intent,
                "RESTORE",
                &RowState::of(&removed, place::TRASH),
                Some(inv),
                cost,
            )
            .map_err(refused)?;
        let sent = self.client.restore(&removed.id).await;
        if let Err(e) = sent.result {
            self.unlanded(intent, sent.uncertain, &e);
            return Err(row_gone(e));
        }
        // The row is back on the shelf: from here a failed patch leaves the intent open, since
        // the row now holds neither the snapshot (it was in the trash) nor the target.
        let sent = self
            .client
            .patch(
                &removed.id,
                &PatchBody {
                    inventory: Some(inv),
                    cost: Some(cost),
                },
            )
            .await;
        let reply = match sent.result {
            Ok(reply) => reply,
            Err(e) => {
                self.unknown(intent, &e);
                return Err(row_gone(e));
            }
        };
        self.landed(
            intent,
            json!({"by": "reply", "stock_id": removed.id, "restored": true}),
        )?;
        let mut back = removed.clone();
        back.inventory = inv;
        back.cost = Some(cost);
        merge_row(shelf, back);
        Ok(Written {
            stock_id: removed.id.clone(),
            qty_before: Some(removed.inventory),
            qty_after: Some(inv),
            price_before: removed.price,
            cost_before: removed.cost,
            price_after: reply.price,
            verb: "PATCH",
            shortfall: shortfall(0, *qty),
            cost,
            held,
            restored: true,
            replanned: false,
        })
    }

    /// REFRESH: the proven cost onto the existing row, unless the row already holds it. `row` is
    /// the job's read of it; it is read again right before the write (R-p).
    pub async fn refresh(
        &self,
        w: &ProvenWrite,
        store: &str,
        available: bool,
        row: &ShelfRow,
    ) -> Result<Refreshed> {
        let Target::Stock { stock_id, product } = w.target() else {
            return Err(BridgeError::Other(
                "an import write is not a refresh".into(),
            ));
        };
        let (cost, held) = pricing::effective_cost(w.cost(), available, row.cost);
        if row.cost == Some(cost) {
            return Ok(Refreshed::Unchanged { cost, held });
        }
        self.still_proven(w)?;
        let auth = row.authenticity.as_deref().unwrap_or("");
        self.unsettled(
            w.job(),
            &RowKey {
                store,
                product,
                authenticity: auth,
                condition: row.condition.as_deref(),
                stock_id: Some(stock_id),
            },
            &[],
        )?;
        let term = row.product_name.as_deref().unwrap_or("");
        let Some(fresh) = self
            .client
            .shelf_find(
                store,
                &Narrow {
                    term,
                    condition: row.condition.as_deref(),
                    authenticity: row.authenticity.as_deref(),
                },
                &|r| r.id == *stock_id,
            )
            .await?
        else {
            return Err(BridgeError::RowChanged(format!(
                "row {stock_id} left the shelf during the run"
            )));
        };
        let replanned = differs(row, &fresh);
        let (cost, held) = pricing::effective_cost(w.cost(), available, fresh.cost);
        if fresh.cost == Some(cost) {
            return Ok(Refreshed::Unchanged { cost, held });
        }
        let snapshot = RowState::of(&fresh, place::LIVE);
        let intent = self.db.open_intent(&NewIntent {
            job_id: w.job(),
            kind: "REFRESH",
            store,
            product,
            authenticity: auth,
            verb: "PATCH",
            snapshot: &snapshot,
            target_inventory: None,
            target_cost: cost,
            items: w.items(),
            lines: &[],
        })?;
        let reply = self.send_patch(intent, stock_id, None, cost).await?;
        Ok(Refreshed::Patched {
            cost,
            held,
            before: fresh.cost,
            price_after: reply.price,
            replanned,
        })
    }
}

/// Put a freshly read row into the local shelf mirror: replace the entry with the same id, or
/// add it. Never replaces the whole mirror (see [`ShelfWriter::import`]'s 409 handling).
pub fn merge_row(shelf: &mut Vec<ShelfRow>, row: ShelfRow) {
    match shelf.iter().position(|r| r.id == row.id) {
        Some(pos) => shelf[pos] = row,
        None => shelf.push(row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::intents::{resolution, IntentRow};
    use crate::db::matches::LinkProduct;
    use crate::domain::{InvoiceHeader, InvoiceKind};
    use crate::jobs::proof::{plan_import, prove_refresh, ImportCandidate, Planned};
    use crate::landau::stub::{self, page, row_json, Reply, Req};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const SUP: &str = "barezpakhsh";

    fn shelf_row(id: &str, product: &str, inventory: i64, cost: i64) -> ShelfRow {
        ShelfRow::from_item(&serde_json::from_str(&row_json(id, product, inventory, cost)).unwrap())
            .unwrap()
    }

    /// A job with one link proven for it, and the write the collision check makes from it.
    fn proven_qty(db: &Db, product: &str, qty: i64) -> (i64, ProvenWrite) {
        let job = db
            .create_job("IMPORT", Some("QUANTITY"), None, "store", false)
            .unwrap();
        db.set_match(SUP, "قطعه", "C1", Some((product, "x")), "USER", None)
            .unwrap();
        let p = LinkProduct {
            id: product,
            name: "x",
            brand: Some(""),
            alias: Some(""),
        };
        let v = db
            .validate_link(SUP, "قطعه", 1, p, state::VALID, None, job)
            .unwrap()
            .unwrap();
        let plan = plan_import(
            db,
            SUP,
            job,
            &[ImportCandidate {
                item_id: 1,
                product: product.into(),
                authenticity: Authenticity::Original,
                cost: 5_000,
                qty,
                link_key: "قطعه".into(),
                link_version: v,
            }],
            &Default::default(),
        )
        .unwrap();
        let Planned::Write(w) = plan.into_iter().next().unwrap() else {
            panic!("expected a write");
        };
        (job, w)
    }

    fn proven(db: &Db, product: &str) -> (i64, ProvenWrite) {
        proven_qty(db, product, 0)
    }

    fn zero<'a>() -> ImportWrite<'a> {
        ImportWrite {
            store: "store",
            mode: ImportMode::Zero,
            available: true,
            min_margin_percent: 15.0,
            term: "x",
            lines: &[],
        }
    }

    fn quantity<'a>(lines: &'a [(i64, String)]) -> ImportWrite<'a> {
        ImportWrite {
            mode: ImportMode::Quantity,
            lines,
            ..zero()
        }
    }

    fn invoice(db: &Db, number: &str) -> i64 {
        db.upsert_invoice(
            SUP,
            &InvoiceHeader {
                number: number.into(),
                kind: InvoiceKind::Sale,
                date_jalali: "1404/08/18".into(),
                date_iso: "2025-11-09".into(),
                paid: true,
                url: format!("/f/{number}"),
            },
        )
        .unwrap()
    }

    fn only_intent(db: &Db) -> IntentRow {
        let id: i64 = db
            .with(|c| c.query_row("SELECT max(id) FROM write_intents", [], |r| r.get(0)))
            .unwrap();
        db.intent(id).unwrap().unwrap()
    }

    fn empty_shelf(r: &Req) -> bool {
        r.method == "GET" && r.path.starts_with("/career/stocks/shelf")
    }

    fn writer<'a>(client: &'a LandauClient, db: &'a Db) -> ShelfWriter<'a> {
        ShelfWriter::new(client, db, SUP).with_lost_reply_gap(Duration::from_millis(10))
    }

    /// R-h: Landau's 404 to an attach is the product gone — the link turns STALE, by WRITE.
    #[tokio::test]
    async fn a_404_at_attach_turns_the_link_stale() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "gone");
        // the literal Landau answers (lnd-backend `stocks.service.ts`, `assertApprovedProduct`)
        let (url, seen) = stub::serve(|r| {
            if empty_shelf(r) {
                return Reply::Status(200, page(&[]));
            }
            Reply::Status(
                404,
                r#"{"message":"there is no such part in Landau’s registry","statusCode":404}"#
                    .into(),
            )
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(
            e,
            BridgeError::StaleLink {
                product: "gone".into()
            }
        );
        assert!(e.is_identity());
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        assert_eq!(link.state, state::STALE);
        assert_eq!(
            db.match_events(SUP, "قطعه").unwrap().last().unwrap().actor,
            actor::WRITE
        );
        let lines = stub::lines(&seen);
        assert!(
            lines[0].starts_with("GET /career/stocks/shelf"),
            "the fresh read first"
        );
        assert_eq!(lines[1], "POST /career/stocks");
        assert_eq!(
            only_intent(&db).status,
            status::NOT_LANDED,
            "refused: closed"
        );
    }

    /// Slice 2's review: a 404 that does not name the product — "store not found", a route the
    /// gateway lacks — is a transfer failure; the link stays proven.
    #[tokio::test]
    async fn a_404_that_is_not_the_product_leaves_the_link_alone() {
        for body in [
            r#"{"message":"store not found","statusCode":404}"#,
            r#"{"message":"Cannot POST /career/stocks","error":"Not Found","statusCode":404}"#,
        ] {
            let db = Db::in_memory().unwrap();
            let (job, w) = proven(&db, "p1");
            let reply = body.to_string();
            let (url, _) = stub::serve(move |r| {
                if empty_shelf(r) {
                    Reply::Status(200, page(&[]))
                } else {
                    Reply::Status(404, reply.clone())
                }
            });
            let client = LandauClient::new(&url, "apt-test").unwrap();
            let e = writer(&client, &db)
                .import(&w, &zero(), &mut Vec::new())
                .await
                .unwrap_err();
            assert!(
                matches!(e, BridgeError::LandauRejected { status: 404, .. }),
                "{body}: {e:?}"
            );
            assert!(!e.is_identity());
            let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
            assert!(link.writable_for(job, link.link_version, "p1"), "{body}");
        }
    }

    /// R-h: a 404 to a patch is the row gone; the link is left alone.
    #[tokio::test]
    async fn a_404_at_patch_is_a_changed_row_and_leaves_the_link() {
        let db = Db::in_memory().unwrap();
        let (job, w) = proven(&db, "p1");
        let (url, _) = stub::serve(|r| {
            if empty_shelf(r) {
                Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)]))
            } else {
                Reply::Status(404, r#"{"message":"stock not found"}"#.into())
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = vec![shelf_row("s1", "p1", 3, 900)];
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut shelf)
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        assert!(link.writable_for(job, link.link_version, "p1"));
        assert_eq!(only_intent(&db).status, status::NOT_LANDED);
    }

    /// R-l, task 8: a link edited after the proof refuses the write before anything is sent.
    #[tokio::test]
    async fn a_link_changed_since_its_proof_is_not_written() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        db.set_match(SUP, "قطعه", "C1", Some(("p2", "y")), "USER", None)
            .unwrap();
        let (url, seen) = stub::serve(|_| Reply::Status(200, row_json("s1", "p1", 0, 900)));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(e, BridgeError::LinkChanged);
        assert!(seen.lock().unwrap().is_empty(), "nothing reached Landau");
    }

    /// The preserved 409 path: a row the reads did not show is re-read and patched from its own
    /// values — and the intent is re-planned onto it.
    #[tokio::test]
    async fn a_409_on_attach_patches_the_live_row() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let (url, seen) = stub::serve(|r| match r.method.as_str() {
            "POST" => Reply::Status(409, r#"{"message":"exists"}"#.into()),
            "GET" if r.path.contains("term=") => Reply::Status(200, page(&[])),
            "GET" => Reply::Status(200, page(&[row_json("s9", "p1", 4, 900)])),
            "PATCH" => Reply::Status(200, row_json("s9", "p1", 4, 5_000)),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = Vec::new();
        let done = writer(&client, &db)
            .import(&w, &zero(), &mut shelf)
            .await
            .unwrap();
        assert_eq!((done.stock_id.as_str(), done.verb), ("s9", "PATCH"));
        assert!(done.replanned);
        assert_eq!(shelf.len(), 1);
        let lines = stub::lines(&seen);
        assert!(lines[0].contains("term="), "{lines:?}");
        assert_eq!(lines[1], "POST /career/stocks");
        assert!(lines[2].starts_with("GET /career/stocks/shelf") && !lines[2].contains("term="));
        assert_eq!(lines[3], "PATCH /career/stocks/s9");
        let intent = only_intent(&db);
        assert_eq!(
            (intent.verb.as_str(), intent.status.as_str()),
            ("PATCH", status::LANDED)
        );
        assert_eq!(intent.snapshot.stock_id.as_deref(), Some("s9"));
    }

    /// A transient 5xx stays a transfer failure: retried, then reported, the link untouched — and
    /// the outcome unknown, so the intent stays open for the next job.
    #[tokio::test]
    async fn a_transient_5xx_stays_a_transfer_error_and_its_intent_open() {
        let db = Db::in_memory().unwrap();
        let (job, w) = proven(&db, "p1");
        let (url, seen) = stub::serve(|r| {
            if empty_shelf(r) {
                Reply::Status(200, page(&[]))
            } else {
                Reply::Status(503, r#"{"message":"busy"}"#.into())
            }
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::LandauRejected { status: 503, .. }),
            "{e:?}"
        );
        assert!(!e.is_identity());
        assert_eq!(stub::writes(&seen).len(), 3, "three attempts");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        assert!(link.writable_for(job, link.link_version, "p1"));
        assert_eq!(only_intent(&db).status, status::OPEN);
    }

    /// PI-8: the attach landed, its reply was lost, the retry answered 409. The row at the target
    /// is this attach: nothing is patched on top, the lines are marked once.
    #[tokio::test]
    async fn a_lost_attach_reply_never_doubles_the_quantity() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 3);
        let inv = invoice(&db, "9001");
        let lines = vec![(inv, "C1".to_string())];
        let posts = Arc::new(AtomicUsize::new(0));
        let n = posts.clone();
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "GET" if r.path.contains("term=") => Reply::Status(200, page(&[])),
            "GET" => Reply::Status(200, page(&[row_json("new", "p1", 3, 5_000)])),
            "POST" if n.fetch_add(1, Ordering::SeqCst) == 0 => Reply::Drop,
            "POST" => Reply::Status(409, r#"{"message":"exists"}"#.into()),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = Vec::new();
        let done = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut shelf)
            .await
            .unwrap();
        assert_eq!((done.verb, done.stock_id.as_str()), ("ATTACH", "new"));
        assert_eq!(done.qty_after, Some(3));
        assert!(
            stub::writes(&seen).iter().all(|r| r.method == "POST"),
            "no patch on top of the landed attach: {:?}",
            stub::lines(&seen)
        );
        let intent = only_intent(&db);
        assert_eq!(intent.status, status::LANDED);
        assert_eq!(intent.snapshot.stock_id.as_deref(), Some("new"));
        assert!(db.imported_lines().unwrap().contains(&(inv, "C1".into())));
    }

    /// The same lost reply, but the row holds other values: not ours to judge — held, the
    /// intent superseded, nothing written, the lines unmarked.
    #[tokio::test]
    async fn a_lost_attach_reply_onto_other_values_is_held() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 3);
        let inv = invoice(&db, "9002");
        let lines = vec![(inv, "C1".to_string())];
        let posts = Arc::new(AtomicUsize::new(0));
        let n = posts.clone();
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "GET" if r.path.contains("term=") => Reply::Status(200, page(&[])),
            "GET" => Reply::Status(200, page(&[row_json("new", "p1", 7, 5_000)])),
            "POST" if n.fetch_add(1, Ordering::SeqCst) == 0 => Reply::Drop,
            "POST" => Reply::Status(409, "{}".into()),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        assert!(stub::writes(&seen).iter().all(|r| r.method == "POST"));
        assert_eq!(only_intent(&db).status, status::SUPERSEDED);
        assert!(db.imported_lines().unwrap().is_empty());
    }

    /// The lost reply's row is not visible yet (the mirror lags): the intent stays open for the
    /// next job; the item fails as a transfer error, nothing is retried on a guess.
    #[tokio::test]
    async fn a_lost_attach_reply_not_yet_visible_stays_open() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 3);
        let posts = Arc::new(AtomicUsize::new(0));
        let n = posts.clone();
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[])),
            "POST" if n.fetch_add(1, Ordering::SeqCst) == 0 => Reply::Drop,
            "POST" => Reply::Status(409, "{}".into()),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &quantity(&[]), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::WriteUnconfirmed(_)), "{e:?}");
        assert!(!e.is_identity());
        assert!(
            !crate::jobs::fatal(&e),
            "the job goes on with its other rows"
        );
        let reads = stub::lines(&seen);
        let live_reads = reads
            .iter()
            .filter(|l| l.starts_with("GET") && !l.contains("term=") && !l.contains("deleted=true"))
            .count();
        assert_eq!(live_reads, LOST_REPLY_READS);
        assert!(
            reads.iter().any(|l| l.contains("deleted=true")),
            "the trash is read before the outcome is called unknown"
        );
        assert_eq!(only_intent(&db).status, status::OPEN);
    }

    /// R-p: a POS sale between the job's read and the write. The write is planned from the fresh
    /// row — 3 + 2, never the 5 + 2 the earlier read would have set.
    #[tokio::test]
    async fn a_row_changed_between_planning_and_writing_is_replanned() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 2);
        let (url, seen) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)])),
            "PATCH" => Reply::Status(200, row_json("s1", "p1", 5, 5_000)),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = vec![shelf_row("s1", "p1", 5, 900)];
        let done = writer(&client, &db)
            .import(&w, &quantity(&[]), &mut shelf)
            .await
            .unwrap();
        assert!(done.replanned);
        assert_eq!((done.qty_before, done.qty_after), (Some(3), Some(5)));
        let patch = stub::writes(&seen).pop().unwrap();
        assert_eq!(patch.json()["inventory"], 5, "{}", patch.body);
        let intent = only_intent(&db);
        assert_eq!(
            intent.snapshot.inventory,
            Some(3),
            "the snapshot is the fresh row"
        );
        assert_eq!(intent.target_inventory, Some(5));
        assert_eq!(shelf[0].inventory, 5);
    }

    /// R-p: the seller removed the row during the run. Restoring it would overwrite that: held.
    #[tokio::test]
    async fn a_row_removed_during_the_run_is_held_and_not_restored() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 2);
        let (url, seen) = stub::serve(|_| Reply::Status(200, page(&[])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = vec![shelf_row("s1", "p1", 5, 900)];
        let e = writer(&client, &db)
            .import(&w, &quantity(&[]), &mut shelf)
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        assert!(stub::writes(&seen).is_empty());
        let count: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM write_intents", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(count, 0, "nothing was about to be written");
    }

    /// R-o: a row whose earlier write was superseded by an external change is not written —
    /// not even read — until the seller settles it; then it is.
    #[tokio::test]
    async fn a_superseded_row_waits_for_the_seller() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let snapshot = RowState::absent();
        let old = db
            .open_intent(&NewIntent {
                job_id: 99,
                kind: "IMPORT",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "ATTACH",
                snapshot: &snapshot,
                target_inventory: Some(1),
                target_cost: 4_000,
                items: &[5],
                lines: &[],
            })
            .unwrap();
        db.close_intent(old, status::SUPERSEDED, &json!({}))
            .unwrap();
        let (url, seen) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[])),
            _ => Reply::Status(201, row_json("s1", "p1", 0, 5_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::RowChanged(ref m) if m.contains(&format!("#{old}"))),
            "{e:?}"
        );
        assert!(seen.lock().unwrap().is_empty());
        db.set_job_status(w.job(), "DONE", "REPORT").unwrap();
        assert!(db.resolve_intent(old, resolution::TRANSFER_AGAIN).unwrap());
        let done = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap();
        assert_eq!(done.verb, "ATTACH");
    }

    /// The same hold reaches an item through its invoice lines, whatever row it now maps to:
    /// those lines may already be on the shelf the superseded write targeted.
    #[tokio::test]
    async fn superseded_invoice_lines_are_held_on_any_row() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 1);
        let inv = invoice(&db, "9100");
        let lines = vec![(inv, "C1".to_string())];
        let snapshot = RowState::absent();
        let old = db
            .open_intent(&NewIntent {
                job_id: 98,
                kind: "IMPORT",
                store: "store",
                product: "another",
                authenticity: "ORIGINAL",
                verb: "ATTACH",
                snapshot: &snapshot,
                target_inventory: Some(1),
                target_cost: 4_000,
                items: &[5],
                lines: &lines,
            })
            .unwrap();
        db.close_intent(old, status::SUPERSEDED, &json!({}))
            .unwrap();
        let (url, seen) = stub::serve(|_| Reply::Status(200, page(&[])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        assert!(seen.lock().unwrap().is_empty());
    }

    /// An intent an earlier job left open holds its row too: the guard behind reconciliation.
    #[tokio::test]
    async fn an_open_intent_of_an_earlier_job_holds_its_row() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let snapshot = RowState::absent();
        db.open_intent(&NewIntent {
            job_id: w.job() - 1,
            kind: "IMPORT",
            store: "store",
            product: "p1",
            authenticity: "ORIGINAL",
            verb: "ATTACH",
            snapshot: &snapshot,
            target_inventory: Some(0),
            target_cost: 4_000,
            items: &[5],
            lines: &[],
        })
        .unwrap();
        let (url, seen) = stub::serve(|_| Reply::Status(200, page(&[])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        assert!(seen.lock().unwrap().is_empty());
    }

    /// REFRESH: an unchanged cost sends nothing; a changed one is read fresh and patched; a
    /// 404 is RowChanged; a row gone from the fresh read is held.
    #[tokio::test]
    async fn a_refresh_write_patches_only_a_changed_cost() {
        let db = Db::in_memory().unwrap();
        let (job, _) = proven(&db, "p1");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        let Planned::Write(w) =
            prove_refresh(&db, SUP, job, 1, "s1", "p1", &[(link, Some(5_000))]).unwrap()
        else {
            panic!("expected a write");
        };
        let (url, seen) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[row_json("s1", "p1", 3, 4_000)])),
            _ => Reply::Status(200, row_json("s1", "p1", 3, 5_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = writer(&client, &db);
        assert_eq!(
            writer
                .refresh(&w, "store", true, &shelf_row("s1", "p1", 3, 5_000))
                .await
                .unwrap(),
            Refreshed::Unchanged {
                cost: 5_000,
                held: false
            }
        );
        assert!(seen.lock().unwrap().is_empty());
        assert!(matches!(
            writer
                .refresh(&w, "store", true, &shelf_row("s1", "p1", 3, 4_000))
                .await
                .unwrap(),
            Refreshed::Patched {
                cost: 5_000,
                before: Some(4_000),
                replanned: false,
                ..
            }
        ));
        let intent = only_intent(&db);
        assert_eq!(
            (intent.kind.as_str(), intent.status.as_str()),
            ("REFRESH", status::LANDED)
        );
        assert_eq!(intent.target_inventory, None, "a cost-only write");
        // unavailable never lowers (§14 d)
        assert_eq!(
            writer
                .refresh(&w, "store", false, &shelf_row("s1", "p1", 3, 6_000))
                .await
                .unwrap(),
            Refreshed::Unchanged {
                cost: 6_000,
                held: true
            }
        );
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[row_json("s1", "p1", 3, 4_000)])),
            _ => Reply::Status(404, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        assert!(matches!(
            writer
                .refresh(&w, "store", true, &shelf_row("s1", "p1", 3, 4_000))
                .await,
            Err(BridgeError::RowChanged(_))
        ));
        let (url, _) = stub::serve(|_| Reply::Status(200, page(&[])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        assert!(matches!(
            writer
                .refresh(&w, "store", true, &shelf_row("s1", "p1", 3, 4_000))
                .await,
            Err(BridgeError::RowChanged(_))
        ));
    }

    /// R-p in REFRESH: the seller set the cost by hand meanwhile, to what the supplier says
    /// now — nothing is sent.
    #[tokio::test]
    async fn a_refresh_whose_row_already_moved_to_the_cost_sends_nothing() {
        let db = Db::in_memory().unwrap();
        let (job, _) = proven(&db, "p1");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        let Planned::Write(w) =
            prove_refresh(&db, SUP, job, 1, "s1", "p1", &[(link, Some(5_000))]).unwrap()
        else {
            panic!("expected a write");
        };
        let (url, seen) =
            stub::serve(|_| Reply::Status(200, page(&[row_json("s1", "p1", 3, 5_000)])));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let out = writer(&client, &db)
            .refresh(&w, "store", true, &shelf_row("s1", "p1", 3, 4_000))
            .await
            .unwrap();
        assert_eq!(
            out,
            Refreshed::Unchanged {
                cost: 5_000,
                held: false
            }
        );
        assert!(stub::writes(&seen).is_empty());
    }

    /// Slice 3's review (#4): the tuple is in the trash, the attach's first attempt answered 503
    /// unapplied, its retry 409 from the trashed row's index. Nothing of the attach can have
    /// landed: the restore path takes it, the intent lands with its lines — no false hold.
    #[tokio::test]
    async fn a_lost_reply_whose_409_came_from_the_trash_restores_the_row() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 2);
        let inv = invoice(&db, "9200");
        let lines = vec![(inv, "C1".to_string())];
        let posts = Arc::new(AtomicUsize::new(0));
        let n = posts.clone();
        let (url, seen) = stub::serve(move |r| match r.method.as_str() {
            "GET" if r.path.contains("deleted=true") => {
                Reply::Status(200, page(&[row_json("s7", "p1", 9, 700)]))
            }
            "GET" => Reply::Status(200, page(&[])),
            "POST" if n.fetch_add(1, Ordering::SeqCst) == 0 => Reply::Status(503, "{}".into()),
            "POST" => Reply::Status(409, "{}".into()),
            "PUT" => Reply::Status(200, row_json("s7", "p1", 9, 700)),
            "PATCH" => Reply::Status(200, row_json("s7", "p1", 2, 5_000)),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let done = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut Vec::new())
            .await
            .unwrap();
        assert!(done.restored);
        assert_eq!((done.stock_id.as_str(), done.qty_after), ("s7", Some(2)));
        let patch = stub::writes(&seen).pop().unwrap();
        assert_eq!(
            patch.json()["inventory"],
            2,
            "a restored row takes this run's quantity"
        );
        let intent = only_intent(&db);
        assert_eq!(
            (intent.verb.as_str(), intent.status.as_str()),
            ("RESTORE", status::LANDED)
        );
        assert_eq!(intent.snapshot.place, place::TRASH);
        assert!(db.imported_lines().unwrap().contains(&(inv, "C1".into())));
    }

    /// #8: a restore whose patch then fails leaves the intent open, stamped with the attempt,
    /// its lines unmarked — the row is neither the trashed snapshot nor the target.
    #[tokio::test]
    async fn a_restore_whose_patch_fails_stays_open() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 2);
        let inv = invoice(&db, "9201");
        let lines = vec![(inv, "C1".to_string())];
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" if r.path.contains("deleted=true") => {
                Reply::Status(200, page(&[row_json("s7", "p1", 9, 700)]))
            }
            "GET" => Reply::Status(200, page(&[])),
            "POST" => Reply::Status(409, "{}".into()),
            "PUT" => Reply::Status(200, row_json("s7", "p1", 9, 700)),
            _ => Reply::Status(400, r#"{"message":"inventory must be a number"}"#.into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::LandauRejected { status: 400, .. }),
            "{e:?}"
        );
        let intent = only_intent(&db);
        assert_eq!(intent.status, status::OPEN);
        assert!(intent.attempted_at.is_some());
        assert!(db.imported_lines().unwrap().is_empty());
    }

    /// #8: an ordinary quantity patch lands through the writer and marks its lines with it.
    #[tokio::test]
    async fn a_landed_quantity_patch_marks_its_lines() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 2);
        let inv = invoice(&db, "9202");
        let lines = vec![(inv, "C1".to_string())];
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[row_json("s1", "p1", 3, 900)])),
            _ => Reply::Status(200, row_json("s1", "p1", 5, 5_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let mut shelf = vec![shelf_row("s1", "p1", 3, 900)];
        writer(&client, &db)
            .import(&w, &quantity(&lines), &mut shelf)
            .await
            .unwrap();
        assert_eq!(only_intent(&db).status, status::LANDED);
        assert!(db.imported_lines().unwrap().contains(&(inv, "C1".into())));
    }

    /// #5: after a first-attempt 409 nothing of the attach landed, so a failed re-read closes the
    /// intent — no later job reads the existing row as a change made outside the bridge.
    #[tokio::test]
    async fn a_failed_reread_after_a_409_closes_the_intent() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" if r.path.contains("term=") => Reply::Status(200, page(&[])),
            "GET" => Reply::Status(400, r#"{"message":"bad query"}"#.into()),
            "POST" => Reply::Status(409, "{}".into()),
            _ => Reply::Status(500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let e = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::LandauRejected { status: 400, .. }),
            "{e:?}"
        );
        assert_eq!(only_intent(&db).status, status::NOT_LANDED);
    }

    fn intent_on(db: &Db, n: &NewIntent<'_>, close: Option<&str>) -> i64 {
        let id = db.open_intent(n).unwrap();
        if let Some(to) = close {
            db.close_intent(id, to, &json!({})).unwrap();
        }
        id
    }

    /// #3: an open intent left on another store holds nothing here, its lines included; a
    /// superseded one's lines are held wherever they would go.
    #[tokio::test]
    async fn an_open_intent_elsewhere_holds_nothing_here() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven_qty(&db, "p1", 1);
        let inv = invoice(&db, "9300");
        let lines = vec![(inv, "C1".to_string())];
        let absent = RowState::absent();
        let elsewhere = NewIntent {
            job_id: w.job() - 1,
            kind: "IMPORT",
            store: "old-store",
            product: "p1",
            authenticity: "ORIGINAL",
            verb: "ATTACH",
            snapshot: &absent,
            target_inventory: Some(1),
            target_cost: 4_000,
            items: &[5],
            lines: &lines,
        };
        intent_on(&db, &elsewhere, None);
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[])),
            _ => Reply::Status(201, row_json("s1", "p1", 1, 5_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let done = writer(&client, &db)
            .import(&w, &quantity(&lines), &mut Vec::new())
            .await
            .unwrap();
        assert_eq!(done.verb, "ATTACH");
        // the same lines under a superseded intent of that store are held here
        db.set_job_status(w.job(), "DONE", "REPORT").unwrap();
        let (_, w2) = proven_qty(&db, "p1", 1);
        let inv2 = invoice(&db, "9301");
        let lines2 = vec![(inv2, "C1".to_string())];
        intent_on(
            &db,
            &NewIntent {
                lines: &lines2,
                ..elsewhere
            },
            Some(status::SUPERSEDED),
        );
        let e = writer(&client, &db)
            .import(&w2, &quantity(&lines2), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::RowChanged(ref m) if m.contains("overtaken")),
            "{e:?}"
        );
    }

    /// #7: a refresh intent names its row by id: one superseded on the product's USED row does
    /// not hold the NEW row an import writes.
    #[tokio::test]
    async fn a_refresh_intent_holds_its_own_row_only() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let used = RowState {
            place: place::LIVE.into(),
            stock_id: Some("s-used".into()),
            inventory: Some(1),
            cost: Some(800),
            updated_at: None,
        };
        intent_on(
            &db,
            &NewIntent {
                job_id: 50,
                kind: "REFRESH",
                store: "store",
                product: "p1",
                authenticity: "ORIGINAL",
                verb: "PATCH",
                snapshot: &used,
                target_inventory: None,
                target_cost: 900,
                items: &[5],
                lines: &[],
            },
            Some(status::SUPERSEDED),
        );
        let (url, _) = stub::serve(|r| match r.method.as_str() {
            "GET" => Reply::Status(200, page(&[])),
            _ => Reply::Status(201, row_json("s-new", "p1", 0, 5_000)),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let done = writer(&client, &db)
            .import(&w, &zero(), &mut Vec::new())
            .await
            .unwrap();
        assert_eq!(done.stock_id, "s-new");
    }

    #[test]
    fn a_row_holds_a_target_by_cost_and_any_inventory_it_set() {
        let row = shelf_row("s1", "p1", 3, 900);
        assert!(holds(&row, Some(3), 900));
        assert!(holds(&row, None, 900));
        assert!(!holds(&row, Some(4), 900));
        assert!(!holds(&row, None, 901));
    }
}
