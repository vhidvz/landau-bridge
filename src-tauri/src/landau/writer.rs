//! The one module that writes a seller's shelf (plan 130 R-l). Every path that sets a row's cost
//! or quantity — IMPORT's attach, attach → 409 → patch, 409 → restore → patch and plain patch, and
//! REFRESH's patch (fact 17) — runs here, and only from a [`ProvenWrite`]. The client's write
//! methods are visible to this module alone, so a new path cannot bypass the proof and compile.
//!
//! Right before each write the links the proof rests on are read again: one changed since it was
//! proven refuses the write (`LinkChanged`). A 404 on an attach means the product is gone, so its
//! links turn `STALE`; a 404 on a patch or a restore means the row is gone (`RowChanged`).

use serde_json::json;

use super::{AttachBody, LandauClient, PatchBody, ShelfRow};
use crate::db::matches::{actor, state};
use crate::db::Db;
use crate::domain::ImportMode;
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
}

/// What a refresh write did.
#[derive(Debug, Clone, PartialEq)]
pub enum Refreshed {
    /// The row already holds this cost: nothing sent.
    Unchanged { cost: i64, held: bool },
    Patched {
        cost: i64,
        held: bool,
        price_after: Option<i64>,
    },
}

pub struct ShelfWriter<'a> {
    client: &'a LandauClient,
    db: &'a Db,
    supplier: &'a str,
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

impl<'a> ShelfWriter<'a> {
    pub fn new(client: &'a LandauClient, db: &'a Db, supplier: &'a str) -> Self {
        ShelfWriter {
            client,
            db,
            supplier,
        }
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

    /// IMPORT: the proven cost (and in quantity mode the summed quantity) onto the row for
    /// `(product, authenticity)` in `store`, attaching it when the shelf lacks one. `shelf` is the
    /// job's local mirror and is kept current with what was written.
    pub async fn import(
        &self,
        w: &ProvenWrite,
        store: &str,
        mode: ImportMode,
        available: bool,
        min_margin_percent: f64,
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
        self.still_proven(w)?;
        let existing = find_row(shelf, product, authenticity.as_str()).cloned();
        // §14 d: an unavailable quote never lowers a row's cost.
        let (cost, held) =
            pricing::effective_cost(w.cost(), available, existing.as_ref().and_then(|r| r.cost));
        let mut restored = false;
        let written = match plan_write(existing.as_ref(), *qty, mode) {
            WriteAction::Attach { inventory } => {
                let body = AttachBody {
                    product: product.clone(),
                    store: store.to_string(),
                    inventory,
                    cost,
                    price: pricing::floor_price(cost, min_margin_percent),
                    authenticity: *authenticity,
                };
                match self.client.attach(&body).await {
                    Ok(row) => {
                        shelf.push(row.clone());
                        Written {
                            stock_id: row.id,
                            qty_before: None,
                            qty_after: Some(inventory),
                            price_before: None,
                            price_after: row.price,
                            verb: "ATTACH",
                            shortfall: shortfall(0, *qty),
                            cost,
                            held,
                            restored,
                        }
                    }
                    Err(BridgeError::DuplicateRow) => {
                        // The mirror lagged behind a row that exists: re-read the live shelf and
                        // patch that row instead. Only the row found is merged into the local
                        // shelf — replacing the whole mirror would drop the figures this run has
                        // already written to sibling rows the lagging read does not show yet.
                        let fresh = self.client.shelf(store).await?;
                        match find_row(&fresh, product, authenticity.as_str()).cloned() {
                            Some(row) => {
                                merge_row(shelf, row.clone());
                                let inv = (mode == ImportMode::Quantity)
                                    .then_some(floored(row.inventory, *qty));
                                let reply = self
                                    .client
                                    .patch(
                                        &row.id,
                                        &PatchBody {
                                            inventory: inv,
                                            cost: Some(cost),
                                        },
                                    )
                                    .await
                                    .map_err(row_gone)?;
                                Written {
                                    stock_id: row.id.clone(),
                                    qty_before: Some(row.inventory),
                                    qty_after: inv.or(Some(row.inventory)),
                                    price_before: row.price,
                                    price_after: reply.price,
                                    verb: "PATCH",
                                    shortfall: shortfall(row.inventory, *qty),
                                    cost,
                                    held,
                                    restored,
                                }
                            }
                            // Not live either: a seller who removed this exact part earlier and is
                            // now importing it again. Landau's uniqueness index on (product, store,
                            // …) still holds the soft-deleted row, so a fresh attach 409s forever —
                            // the row must be restored before it can be patched (spec §2.1
                            // amendment, discovered live). A restored row is a fresh attach:
                            // whatever it held when it was removed is gone from the seller's point
                            // of view, so its inventory is set to this run's quantity (zero in
                            // zero mode), not added to.
                            None => {
                                let trash = self.client.shelf_deleted(store).await?;
                                let Some(removed) =
                                    find_row(&trash, product, authenticity.as_str()).cloned()
                                else {
                                    return Err(BridgeError::DuplicateRow);
                                };
                                self.client.restore(&removed.id).await.map_err(row_gone)?;
                                let inv = if mode == ImportMode::Quantity {
                                    floored(0, *qty)
                                } else {
                                    0
                                };
                                let reply = self
                                    .client
                                    .patch(
                                        &removed.id,
                                        &PatchBody {
                                            inventory: Some(inv),
                                            cost: Some(cost),
                                        },
                                    )
                                    .await
                                    .map_err(row_gone)?;
                                restored = true;
                                let mut back = removed.clone();
                                back.inventory = inv;
                                back.cost = Some(cost);
                                merge_row(shelf, back);
                                Written {
                                    stock_id: removed.id.clone(),
                                    qty_before: Some(removed.inventory),
                                    qty_after: Some(inv),
                                    price_before: removed.price,
                                    price_after: reply.price,
                                    verb: "PATCH",
                                    shortfall: shortfall(0, *qty),
                                    cost,
                                    held,
                                    restored,
                                }
                            }
                        }
                    }
                    Err(e) => return Err(self.stale_links(w, product, e)),
                }
            }
            WriteAction::Patch {
                stock_id,
                inventory,
                qty_before,
            } => {
                let reply = self
                    .client
                    .patch(
                        &stock_id,
                        &PatchBody {
                            inventory,
                            cost: Some(cost),
                        },
                    )
                    .await
                    .map_err(row_gone)?;
                Written {
                    stock_id: stock_id.clone(),
                    qty_before: Some(qty_before),
                    qty_after: inventory.or(Some(qty_before)),
                    price_before: existing.as_ref().and_then(|r| r.price),
                    price_after: reply.price,
                    verb: "PATCH",
                    shortfall: shortfall(qty_before, *qty),
                    cost,
                    held,
                    restored,
                }
            }
        };
        // keep the local mirror current with what was written
        if let Some(pos) = shelf.iter().position(|r| r.id == written.stock_id) {
            if let Some(q) = written.qty_after {
                shelf[pos].inventory = q;
            }
            shelf[pos].cost = Some(cost);
        }
        Ok(written)
    }

    /// REFRESH: the proven cost onto the existing row, unless the row already holds it.
    pub async fn refresh(
        &self,
        w: &ProvenWrite,
        available: bool,
        current_cost: Option<i64>,
    ) -> Result<Refreshed> {
        let Target::Stock { stock_id, .. } = w.target() else {
            return Err(BridgeError::Other(
                "an import write is not a refresh".into(),
            ));
        };
        let (cost, held) = pricing::effective_cost(w.cost(), available, current_cost);
        if current_cost == Some(cost) {
            return Ok(Refreshed::Unchanged { cost, held });
        }
        self.still_proven(w)?;
        let reply = self
            .client
            .patch(
                stock_id,
                &PatchBody {
                    inventory: None,
                    cost: Some(cost),
                },
            )
            .await
            .map_err(row_gone)?;
        Ok(Refreshed::Patched {
            cost,
            held,
            price_after: reply.price,
        })
    }
}

/// Put a freshly read row into the local shelf mirror: replace the entry with the same id, or
/// add it. Never replaces the whole mirror (see the 409 branch of [`ShelfWriter::import`]).
pub fn merge_row(shelf: &mut Vec<ShelfRow>, row: ShelfRow) {
    match shelf.iter().position(|r| r.id == row.id) {
        Some(pos) => shelf[pos] = row,
        None => shelf.push(row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::matches::LinkProduct;
    use crate::domain::Authenticity;
    use crate::jobs::proof::{plan_import, prove_refresh, ImportCandidate, Planned};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    const SUP: &str = "barezpakhsh";

    type Seen = Arc<Mutex<Vec<String>>>;

    /// A Landau that answers `answer(method, path)`; records every request as "METHOD path".
    fn stub(answer: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (String, Seen) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let (method, path) = (
                    parts.next().unwrap_or("").to_string(),
                    parts.next().unwrap_or("").to_string(),
                );
                let mut length = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                log.lock().unwrap().push(format!("{method} {path}"));
                let (status, reply) = answer(&method, &path);
                let mut out = stream;
                let _ = write!(
                    out,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        (url, seen)
    }

    fn row_json(id: &str, product: &str, inventory: i64) -> String {
        format!(
            r#"{{"id":"{id}","product":"{product}","inventory":{inventory},"price":1000,"cost":900,"condition":"NEW","authenticity":"ORIGINAL"}}"#
        )
    }

    /// A job with one link proven for it, and the write the collision check makes from it.
    fn proven(db: &Db, product: &str) -> (i64, ProvenWrite) {
        let job = db
            .create_job("IMPORT", Some("ZERO"), None, "store", false)
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
                qty: 0,
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

    /// R-h: Landau's 404 to an attach is the product gone — the link turns STALE, by WRITE.
    #[tokio::test]
    async fn a_404_at_attach_turns_the_link_stale() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "gone");
        // the literal Landau answers (lnd-backend `stocks.service.ts`, `assertApprovedProduct`)
        let (url, seen) = stub(|_, _| {
            (
                404,
                r#"{"message":"there is no such part in Landau’s registry","statusCode":404}"#
                    .into(),
            )
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        let e = writer
            .import(&w, "store", ImportMode::Zero, true, 15.0, &mut Vec::new())
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
        assert_eq!(seen.lock().unwrap().as_slice(), ["POST /career/stocks"]);
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
            let (url, _) = stub(move |_, _| (404, reply.clone()));
            let client = LandauClient::new(&url, "apt-test").unwrap();
            let writer = ShelfWriter::new(&client, &db, SUP);
            let e = writer
                .import(&w, "store", ImportMode::Zero, true, 15.0, &mut Vec::new())
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
        let (url, _) = stub(|_, _| (404, r#"{"message":"stock not found"}"#.into()));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        let mut shelf =
            vec![
                ShelfRow::from_item(&serde_json::from_str(&row_json("s1", "p1", 3)).unwrap())
                    .unwrap(),
            ];
        let e = writer
            .import(&w, "store", ImportMode::Zero, true, 15.0, &mut shelf)
            .await
            .unwrap_err();
        assert!(matches!(e, BridgeError::RowChanged(_)), "{e:?}");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        assert!(link.writable_for(job, link.link_version, "p1"));
    }

    /// R-l, task 8: a link edited after the proof refuses the write before anything is sent.
    #[tokio::test]
    async fn a_link_changed_since_its_proof_is_not_written() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        db.set_match(SUP, "قطعه", "C1", Some(("p2", "y")), "USER", None)
            .unwrap();
        let (url, seen) = stub(|_, _| (200, row_json("s1", "p1", 0)));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        let e = writer
            .import(&w, "store", ImportMode::Zero, true, 15.0, &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(e, BridgeError::LinkChanged);
        assert!(seen.lock().unwrap().is_empty(), "nothing reached Landau");
    }

    /// The preserved 409 path: a row the mirror did not show yet is re-read and patched.
    #[tokio::test]
    async fn a_409_on_attach_patches_the_live_row() {
        let db = Db::in_memory().unwrap();
        let (_, w) = proven(&db, "p1");
        let (url, seen) = stub(|method, path| match (method, path) {
            ("POST", _) => (409, r#"{"message":"exists"}"#.into()),
            ("GET", p) if p.starts_with("/career/stocks/shelf") => (
                200,
                format!(r#"{{"items":[{}],"total":1}}"#, row_json("s9", "p1", 4)),
            ),
            ("PATCH", _) => (200, row_json("s9", "p1", 4)),
            _ => (500, "{}".into()),
        });
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        let mut shelf = Vec::new();
        let done = writer
            .import(&w, "store", ImportMode::Zero, true, 15.0, &mut shelf)
            .await
            .unwrap();
        assert_eq!((done.stock_id.as_str(), done.verb), ("s9", "PATCH"));
        assert_eq!(shelf.len(), 1);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], "POST /career/stocks");
        assert!(seen[1].starts_with("GET /career/stocks/shelf"));
        assert_eq!(seen[2], "PATCH /career/stocks/s9");
    }

    /// A transient 5xx stays a transfer failure: retried, then reported, the link untouched.
    #[tokio::test]
    async fn a_transient_5xx_stays_a_transfer_error() {
        let db = Db::in_memory().unwrap();
        let (job, w) = proven(&db, "p1");
        let (url, seen) = stub(|_, _| (503, r#"{"message":"busy"}"#.into()));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        let e = writer
            .import(&w, "store", ImportMode::Zero, true, 15.0, &mut Vec::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BridgeError::LandauRejected { status: 503, .. }),
            "{e:?}"
        );
        assert!(!e.is_identity());
        assert_eq!(seen.lock().unwrap().len(), 3, "three attempts");
        let link = db.match_for_name(SUP, "قطعه").unwrap().unwrap();
        assert!(link.writable_for(job, link.link_version, "p1"));
    }

    /// REFRESH: an unchanged cost sends nothing; a changed one is patched; a 404 is RowChanged.
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
        let (url, seen) = stub(|_, _| (200, row_json("s1", "p1", 3)));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        assert_eq!(
            writer.refresh(&w, true, Some(5_000)).await.unwrap(),
            Refreshed::Unchanged {
                cost: 5_000,
                held: false
            }
        );
        assert!(seen.lock().unwrap().is_empty());
        assert!(matches!(
            writer.refresh(&w, true, Some(4_000)).await.unwrap(),
            Refreshed::Patched { cost: 5_000, .. }
        ));
        // unavailable never lowers (§14 d)
        assert_eq!(
            writer.refresh(&w, false, Some(6_000)).await.unwrap(),
            Refreshed::Unchanged {
                cost: 6_000,
                held: true
            }
        );
        let (url, _) = stub(|_, _| (404, "{}".into()));
        let client = LandauClient::new(&url, "apt-test").unwrap();
        let writer = ShelfWriter::new(&client, &db, SUP);
        assert!(matches!(
            writer.refresh(&w, true, Some(4_000)).await,
            Err(BridgeError::RowChanged(_))
        ));
    }
}
