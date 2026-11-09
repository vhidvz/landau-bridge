//! Supplier links (`supplier_links`, plan 130 R-a): one per Barezpakhsh name, with the state that
//! says whether a job proved it, the identity of the product it was proven against, a version that
//! moves whenever its product or decision does, and an append-only trail in `match_events`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;
use crate::matching::normalize::name_key;

/// Link states (R-a). Only [`VALID`] and [`REBUILT`] write, and only for the job that proved them.
pub mod state {
    pub const UNVERIFIED: &str = "UNVERIFIED";
    pub const VALID: &str = "VALID";
    pub const REBUILT: &str = "REBUILT";
    pub const NEEDS_REVIEW: &str = "NEEDS_REVIEW";
    pub const STALE: &str = "STALE";
}

/// Who changed a link (R-a, plus `VALIDATE` and `WRITE` — slice 2's ruling).
pub mod actor {
    pub const SELLER: &str = "SELLER";
    pub const AUTO: &str = "AUTO";
    pub const REBUILD: &str = "REBUILD";
    pub const IMPORT: &str = "IMPORT";
    pub const VALIDATE: &str = "VALIDATE";
    pub const WRITE: &str = "WRITE";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct MatchRow {
    pub supplier: String,
    /// `name_key(name_seen)` — the link's identity (spec §14 a).
    pub name_key: String,
    /// The supplier's code when the link last saw one; an attribute, not the key.
    pub code: String,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    /// The product's brand and alias as last recorded: `None` never recorded, `Some("")` none.
    pub product_brand: Option<String>,
    pub product_alias: Option<String>,
    /// AUTO | USER | NONE
    pub how: String,
    pub similarity: Option<i64>,
    pub name_seen: String,
    pub priced_at: Option<String>,
    pub decided_at: String,
    /// See [`state`].
    pub state: String,
    /// JSON: the write table's reasons, or why the link went stale.
    pub state_reason: Option<String>,
    pub state_at: Option<String>,
    pub validated_job: Option<i64>,
    pub prev_product_id: Option<String>,
    pub link_version: i64,
    /// JSON `Reason[]`: what the seller confirmed when picking against a conflict (R-d), or
    /// `None`. A stored pick whose conflicts are all among these still writes (slice 4).
    pub confirmed_json: Option<String>,
}

impl MatchRow {
    /// Writable for `job` at `version` onto `product` (R-b, R-l): proven by that job, unchanged
    /// since, and still pointing at the product the write was planned for.
    pub fn writable_for(&self, job: i64, version: i64, product: &str) -> bool {
        matches!(self.state.as_str(), state::VALID | state::REBUILT)
            && self.validated_job == Some(job)
            && self.link_version == version
            && self.product_id.as_deref() == Some(product)
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MatchRow> {
    Ok(MatchRow {
        supplier: r.get(0)?,
        name_key: r.get(1)?,
        code: r.get(2)?,
        product_id: r.get(3)?,
        product_name: r.get(4)?,
        product_brand: r.get(5)?,
        product_alias: r.get(6)?,
        how: r.get(7)?,
        similarity: r.get(8)?,
        name_seen: r.get(9)?,
        priced_at: r.get(10)?,
        decided_at: r.get(11)?,
        state: r.get(12)?,
        state_reason: r.get(13)?,
        state_at: r.get(14)?,
        validated_job: r.get(15)?,
        prev_product_id: r.get(16)?,
        link_version: r.get(17)?,
        confirmed_json: r.get(18)?,
    })
}

const COLS: &str =
    "supplier, name_key, code, product_id, product_name, product_brand, product_alias, how, \
    similarity, name_seen, priced_at, decided_at, state, state_reason, state_at, validated_job, \
    prev_product_id, link_version, confirmed_json";

/// The product a link points at, with the definition fields that identify it (Q-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkProduct<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// `None` when not known; `Some("")` when the product has none.
    pub brand: Option<&'a str>,
    pub alias: Option<&'a str>,
}

/// What a conditional decision expects to find (slice 2's review: a seller's edit between the
/// read and the write must never be overwritten by a job's verdict).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect<'a> {
    /// No link for this name yet.
    Absent,
    /// The link still at this version, pointing at this product.
    At {
        version: i64,
        product: Option<&'a str>,
    },
}

/// One decision to store on a link.
#[derive(Debug, Clone)]
pub struct LinkPut<'a> {
    pub name_seen: &'a str,
    pub code: &'a str,
    /// `None` with `how = "NONE"`: "not in Landau".
    pub product: Option<LinkProduct<'a>>,
    pub how: &'a str,
    pub similarity: Option<i64>,
    pub state: &'a str,
    pub state_reason: Option<String>,
    /// The job that proved it, when [`state::VALID`] or [`state::REBUILT`].
    pub validated_job: Option<i64>,
    pub actor: &'a str,
    /// `shown` | `reconstructed`.
    pub evidence: Option<&'a str>,
    pub job_id: Option<i64>,
    /// Keep the decision date the caller brings (an imported file's), else now.
    pub decided_at: Option<String>,
    pub priced_at: Option<String>,
    /// JSON `Reason[]` the seller confirmed with this decision (R-d); `None` clears it.
    pub confirmed: Option<String>,
}

/// A link's trail entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchEvent {
    pub at: String,
    pub job_id: Option<i64>,
    pub actor: String,
    pub old_product_id: Option<String>,
    pub new_product_id: Option<String>,
    pub old_state: Option<String>,
    pub new_state: Option<String>,
    pub evidence: Option<String>,
    pub detail: Option<String>,
}

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

fn by_key(c: &Connection, supplier: &str, key: &str) -> rusqlite::Result<Option<MatchRow>> {
    c.query_row(
        &format!("SELECT {COLS} FROM supplier_links WHERE supplier = ?1 AND name_key = ?2"),
        params![supplier, key],
        row,
    )
    .optional()
}

#[allow(clippy::too_many_arguments)]
fn event(
    c: &Connection,
    supplier: &str,
    key: &str,
    job_id: Option<i64>,
    actor: &str,
    old: Option<&MatchRow>,
    new_product: Option<&str>,
    new_state: Option<&str>,
    evidence: Option<&str>,
    detail: Option<&str>,
) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO match_events(supplier, name_key, at, job_id, actor, old_product_id, new_product_id,
           old_state, new_state, evidence, detail_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            supplier,
            key,
            now(),
            job_id,
            actor,
            old.and_then(|o| o.product_id.clone()),
            new_product,
            old.map(|o| o.state.clone()),
            new_state,
            evidence,
            detail
        ],
    )
    .map(|_| ())
}

/// Whether the link as stored is what `expect` names.
fn as_expected(old: Option<&MatchRow>, expect: Option<Expect<'_>>) -> bool {
    match (expect, old) {
        (None, _) => true,
        (Some(Expect::Absent), old) => old.is_none(),
        (Some(Expect::At { version, product }), Some(o)) => {
            o.link_version == version && o.product_id.as_deref() == product
        }
        (Some(Expect::At { .. }), None) => false,
    }
}

/// Upserts one decision and its trail entry on an open connection; answers the link's version,
/// or `None` when `expect` no longer holds (nothing written). The version moves when the product
/// or the decision changes, never for a code or a state — and never restarts: a link created
/// again after a clear continues past every version its name ever had, so a proof taken before
/// the clear can never match it.
fn put(
    c: &Connection,
    supplier: &str,
    p: &LinkPut<'_>,
    expect: Option<Expect<'_>>,
) -> rusqlite::Result<Option<i64>> {
    let key = name_key(p.name_seen);
    let old = by_key(c, supplier, &key)?;
    if !as_expected(old.as_ref(), expect) {
        return Ok(None);
    }
    let pid = p.product.map(|x| x.id);
    let version = match &old {
        None => {
            let seen: i64 = c.query_row(
                "SELECT count(*) FROM match_events WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key],
                |r| r.get(0),
            )?;
            seen + 1
        }
        Some(o) if o.product_id.as_deref() == pid && o.how == p.how => o.link_version,
        Some(o) => o.link_version + 1,
    };
    let at = now();
    c.execute(
        "INSERT INTO supplier_links(supplier, name_key, code, product_id, product_name, product_brand,
           product_alias, how, similarity, name_seen, priced_at, decided_at, state, state_reason, state_at,
           validated_job, prev_product_id, link_version, confirmed_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL, ?17, ?18)
         ON CONFLICT(supplier, name_key) DO UPDATE SET code = excluded.code,
           product_id = excluded.product_id, product_name = excluded.product_name,
           product_brand = excluded.product_brand, product_alias = excluded.product_alias,
           how = excluded.how, similarity = excluded.similarity, name_seen = excluded.name_seen,
           priced_at = COALESCE(excluded.priced_at, supplier_links.priced_at),
           decided_at = excluded.decided_at, state = excluded.state,
           state_reason = excluded.state_reason, state_at = excluded.state_at,
           validated_job = excluded.validated_job, link_version = excluded.link_version,
           confirmed_json = excluded.confirmed_json",
        params![
            supplier,
            key,
            p.code,
            pid,
            p.product.map(|x| x.name),
            p.product.and_then(|x| x.brand),
            p.product.and_then(|x| x.alias),
            p.how,
            p.similarity,
            p.name_seen,
            p.priced_at,
            p.decided_at.clone().unwrap_or_else(|| at.clone()),
            p.state,
            p.state_reason,
            at,
            p.validated_job,
            version,
            p.confirmed
        ],
    )?;
    event(
        c,
        supplier,
        &key,
        p.job_id,
        p.actor,
        old.as_ref(),
        pid,
        Some(p.state),
        p.evidence,
        Some(&match &p.confirmed {
            Some(c) => serde_json::json!({"how": p.how, "version": version, "confirmed": serde_json::from_str::<serde_json::Value>(c).unwrap_or_default()}),
            None => serde_json::json!({"how": p.how, "version": version}),
        }
        .to_string()),
    )?;
    Ok(Some(version))
}

impl Db {
    pub fn match_for_name(&self, supplier: &str, name: &str) -> Result<Option<MatchRow>> {
        let key = name_key(name);
        self.with(|c| by_key(c, supplier, &key))
    }

    pub fn link_by_key(&self, supplier: &str, key: &str) -> Result<Option<MatchRow>> {
        self.with(|c| by_key(c, supplier, key))
    }

    /// The most recently decided link that carries `code`; never for an empty code.
    pub fn match_for_code(&self, supplier: &str, code: &str) -> Result<Option<MatchRow>> {
        if code.is_empty() {
            return Ok(None);
        }
        self.with(|c| {
            c.query_row(
                &format!(
                    "SELECT {COLS} FROM supplier_links WHERE supplier = ?1 AND code = ?2 ORDER BY decided_at DESC LIMIT 1"
                ),
                params![supplier, code],
                row,
            )
            .optional()
        })
    }

    /// Stores one decision with its trail entry; answers the link's version. The seller's latest
    /// decision wins, whatever the link held.
    pub fn put_link(&self, supplier: &str, p: &LinkPut<'_>) -> Result<i64> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let v = put(&tx, supplier, p, None)?;
            tx.commit()?;
            Ok(v.unwrap_or_default())
        })
    }

    /// A job's decision, stored only if the link is still what the job read (`None` otherwise).
    pub fn put_link_if(
        &self,
        supplier: &str,
        p: &LinkPut<'_>,
        expect: Expect<'_>,
    ) -> Result<Option<i64>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let v = put(&tx, supplier, p, Some(expect))?;
            tx.commit()?;
            Ok(v)
        })
    }

    /// The seller's decision from the Matches screen (or a test): `product` = `Some((id, name))`
    /// for a match, `None` with `how = "NONE"`. Not proven by any job, so `UNVERIFIED`.
    pub fn set_match(
        &self,
        supplier: &str,
        name_seen: &str,
        code: &str,
        product: Option<(&str, &str)>,
        how: &str,
        similarity: Option<i64>,
    ) -> Result<()> {
        self.put_link(
            supplier,
            &LinkPut {
                name_seen,
                code,
                product: product.map(|(id, name)| LinkProduct {
                    id,
                    name,
                    brand: None,
                    alias: None,
                }),
                how,
                similarity,
                state: state::UNVERIFIED,
                state_reason: None,
                validated_job: None,
                actor: if how == "AUTO" {
                    actor::AUTO
                } else {
                    actor::SELLER
                },
                evidence: (how == "USER").then_some("shown"),
                job_id: None,
                decided_at: None,
                priced_at: None,
                confirmed: None,
            },
        )
        .map(|_| ())
    }

    /// A name hit seen under a new code: the code is an attribute, so the version stays.
    pub fn set_link_code(&self, supplier: &str, key: &str, code: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE supplier_links SET code = ?3 WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key, code],
            )
            .map(|_| ())
        })
    }

    /// VALIDATE's verdict on a link whose product exists (R-b): its state for `job`, and the
    /// product's definition as the registry lists it now. The version stays: the product did not
    /// change. Stored only if the link is still at `version` on `product` — a seller's edit since
    /// the read wins — and answers the version the verdict holds for, or `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_link(
        &self,
        supplier: &str,
        key: &str,
        version: i64,
        product: LinkProduct<'_>,
        new_state: &str,
        reason: Option<String>,
        job: i64,
    ) -> Result<Option<i64>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let old = by_key(&tx, supplier, key)?;
            let expect = Expect::At {
                version,
                product: Some(product.id),
            };
            let Some(old) = old.filter(|o| as_expected(Some(o), Some(expect))) else {
                return Ok(None);
            };
            tx.execute(
                "UPDATE supplier_links SET product_name = ?3, product_brand = ?4, product_alias = ?5,
                   state = ?6, state_reason = ?7, state_at = ?8, validated_job = ?9
                 WHERE supplier = ?1 AND name_key = ?2",
                params![
                    supplier,
                    key,
                    product.name,
                    product.brand,
                    product.alias,
                    new_state,
                    reason,
                    now(),
                    job
                ],
            )?;
            if old.state != new_state {
                event(
                    &tx,
                    supplier,
                    key,
                    Some(job),
                    actor::VALIDATE,
                    Some(&old),
                    old.product_id.as_deref(),
                    Some(new_state),
                    Some("reconstructed"),
                    reason.as_deref(),
                )?;
            }
            tx.commit()?;
            Ok(Some(old.link_version))
        })
    }

    /// A dead product's link moved to the one current product its recorded name, brand and alias
    /// equal (Q-1): the old id is kept, the version moves. Only if the link is still at `version`
    /// on `dead` — never over a seller's edit since the read. Answers the new version, or `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_link(
        &self,
        supplier: &str,
        key: &str,
        version: i64,
        dead: &str,
        product: LinkProduct<'_>,
        new_state: &str,
        reason: Option<String>,
        job: i64,
    ) -> Result<Option<i64>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let expect = Expect::At {
                version,
                product: Some(dead),
            };
            let Some(old) = by_key(&tx, supplier, key)?.filter(|o| as_expected(Some(o), Some(expect)))
            else {
                return Ok(None);
            };
            let next = old.link_version + 1;
            tx.execute(
                "UPDATE supplier_links SET prev_product_id = product_id, product_id = ?3, product_name = ?4,
                   product_brand = ?5, product_alias = ?6, state = ?7, state_reason = ?8, state_at = ?9,
                   validated_job = ?10, link_version = ?11
                 WHERE supplier = ?1 AND name_key = ?2",
                params![
                    supplier,
                    key,
                    product.id,
                    product.name,
                    product.brand,
                    product.alias,
                    new_state,
                    reason,
                    now(),
                    job,
                    next
                ],
            )?;
            event(
                &tx,
                supplier,
                key,
                Some(job),
                actor::REBUILD,
                Some(&old),
                Some(product.id),
                Some(new_state),
                Some("reconstructed"),
                reason.as_deref(),
            )?;
            tx.commit()?;
            Ok(Some(next))
        })
    }

    /// A state change that leaves the product alone — a link gone stale, or one sent to review.
    /// Unconditional: the writer's 404 feedback is about the product it just wrote to.
    pub fn set_link_state(
        &self,
        supplier: &str,
        key: &str,
        new_state: &str,
        reason: Option<String>,
        job: Option<i64>,
        who: &str,
    ) -> Result<()> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            if let Some(old) = by_key(&tx, supplier, key)? {
                tx.execute(
                    "UPDATE supplier_links SET state = ?3, state_reason = ?4, state_at = ?5,
                       validated_job = NULL WHERE supplier = ?1 AND name_key = ?2",
                    params![supplier, key, new_state, reason, now()],
                )?;
                if old.state != new_state {
                    event(
                        &tx,
                        supplier,
                        key,
                        job,
                        who,
                        Some(&old),
                        old.product_id.as_deref(),
                        Some(new_state),
                        None,
                        reason.as_deref(),
                    )?;
                }
            }
            tx.commit()
        })
    }

    /// [`Self::set_link_state`] only if the link is still at `version` on `product`: VALIDATE's
    /// refusal never lands on a decision the seller made since the job read the link.
    #[allow(clippy::too_many_arguments)]
    pub fn set_link_state_if(
        &self,
        supplier: &str,
        key: &str,
        version: i64,
        product: Option<&str>,
        new_state: &str,
        reason: Option<String>,
        job: Option<i64>,
        who: &str,
    ) -> Result<bool> {
        let current = self.link_by_key(supplier, key)?;
        let expect = Expect::At { version, product };
        if !as_expected(current.as_ref(), Some(expect)) {
            return Ok(false);
        }
        self.set_link_state(supplier, key, new_state, reason, job, who)?;
        Ok(true)
    }

    /// Writes an import's decisions in one transaction — all of them or, on any error, none —
    /// each keyed on `name_key(name_seen)` as [`Self::set_match`] keys it, keeping the date it
    /// was taken when the file carries one. Every one arrives `UNVERIFIED`: no job proved it here.
    pub fn apply_decisions(
        &self,
        supplier: &str,
        decisions: &[crate::decisions::Decision],
    ) -> Result<()> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            for d in decisions {
                let product = d.product_id.as_deref().map(|id| LinkProduct {
                    id,
                    name: d.product_name.as_deref().unwrap_or_default(),
                    brand: None,
                    alias: None,
                });
                put(
                    &tx,
                    supplier,
                    &LinkPut {
                        name_seen: &d.name_seen,
                        code: &d.code,
                        product,
                        how: &d.how,
                        similarity: d.similarity,
                        state: state::UNVERIFIED,
                        state_reason: None,
                        validated_job: None,
                        actor: actor::IMPORT,
                        evidence: None,
                        job_id: None,
                        decided_at: d.decided_at.clone(),
                        priced_at: d.priced_at.clone(),
                        confirmed: None,
                    },
                    None,
                )?;
            }
            tx.commit()
        })
    }

    /// The link for `name_seen` answered a price just now (spec §14 c).
    pub fn touch_priced(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "UPDATE supplier_links SET priced_at = ?3 WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key, now()],
            )
            .map(|_| ())
        })
    }

    pub fn clear_match(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            if let Some(old) = by_key(&tx, supplier, &key)? {
                tx.execute(
                    "DELETE FROM supplier_links WHERE supplier = ?1 AND name_key = ?2",
                    params![supplier, key],
                )?;
                event(
                    &tx,
                    supplier,
                    &key,
                    None,
                    actor::SELLER,
                    Some(&old),
                    None,
                    None,
                    Some("shown"),
                    None,
                )?;
            }
            tx.commit()
        })
    }

    /// Links a job held for the seller — `NEEDS_REVIEW` or `STALE` — for the dashboard's tile.
    pub fn count_links_needing_review(&self, supplier: &str) -> Result<i64> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM supplier_links WHERE supplier = ?1 AND state IN (?2, ?3)",
                params![supplier, state::NEEDS_REVIEW, state::STALE],
                |r| r.get(0),
            )
        })
    }

    pub fn list_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLS} FROM supplier_links WHERE supplier = ?1 ORDER BY decided_at DESC"
            ))?;
            let rows = st.query_map(params![supplier], row)?;
            rows.collect()
        })
    }

    /// A link's trail, oldest first.
    pub fn match_events(&self, supplier: &str, key: &str) -> Result<Vec<MatchEvent>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT at, job_id, actor, old_product_id, new_product_id, old_state, new_state, evidence, detail_json
                 FROM match_events WHERE supplier = ?1 AND name_key = ?2 ORDER BY id",
            )?;
            let rows = st.query_map(params![supplier, key], |r| {
                Ok(MatchEvent {
                    at: r.get(0)?,
                    job_id: r.get(1)?,
                    actor: r.get(2)?,
                    old_product_id: r.get(3)?,
                    new_product_id: r.get(4)?,
                    old_state: r.get(5)?,
                    new_state: r.get(6)?,
                    evidence: r.get(7)?,
                    detail: r.get(8)?,
                })
            })?;
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

    /// Every link pointing at `product`, whatever its state, in key order.
    pub fn links_on_product(&self, supplier: &str, product: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLS} FROM supplier_links WHERE supplier = ?1 AND product_id = ?2 \
                 ORDER BY name_key"
            ))?;
            let rows = st.query_map(params![supplier, product], row)?;
            rows.collect()
        })
    }

    /// Every link with a product, whatever its state.
    pub fn linked_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {COLS} FROM supplier_links WHERE supplier = ?1 AND product_id IS NOT NULL"
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
        let trail = db.match_events("s", "قطعه").unwrap();
        assert_eq!(trail.last().unwrap().actor, actor::SELLER);
        assert_eq!(trail.last().unwrap().new_product_id, None);
    }

    fn product<'a>(id: &'a str) -> LinkProduct<'a> {
        LinkProduct {
            id,
            name: "بلبرینگ",
            brand: Some("PSN عظام"),
            alias: Some(""),
        }
    }

    /// R-b, R-l: only a link a job proved, still at the version it was proven at, writes — and
    /// only for that job. A seller's edit from the Matches screen moves the version.
    #[test]
    fn a_link_writes_only_for_the_job_that_proved_it_at_that_version() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("p1", "الف")), "USER", None)
            .unwrap();
        let fresh = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(fresh.state, state::UNVERIFIED);
        assert!(
            !fresh.writable_for(7, fresh.link_version, "p1"),
            "never before VALIDATE"
        );

        let v = db
            .validate_link("s", "قطعه", 1, product("p1"), state::VALID, None, 7)
            .unwrap()
            .unwrap();
        let proven = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert!(proven.writable_for(7, v, "p1"));
        assert!(
            !proven.writable_for(8, v, "p1"),
            "another job validates for itself"
        );
        assert_eq!(proven.product_brand.as_deref(), Some("PSN عظام"));

        db.set_match("s", "قطعه", "1", Some(("p2", "ب")), "USER", None)
            .unwrap();
        let edited = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(edited.link_version, v + 1);
        assert!(
            !edited.writable_for(7, v, "p1"),
            "a link changed mid-job does not write"
        );

        // a new code is an attribute: the version stays
        db.set_link_code("s", "قطعه", "9").unwrap();
        assert_eq!(
            db.match_for_name("s", "قطعه")
                .unwrap()
                .unwrap()
                .link_version,
            v + 1
        );
    }

    #[test]
    fn a_rebuild_keeps_the_old_id_moves_the_version_and_marks_the_trail_reconstructed() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("dead", "الف")), "USER", None)
            .unwrap();
        let v = db
            .rebuild_link(
                "s",
                "قطعه",
                1,
                "dead",
                product("new"),
                state::REBUILT,
                None,
                3,
            )
            .unwrap()
            .unwrap();
        let row = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(row.product_id.as_deref(), Some("new"));
        assert_eq!(row.prev_product_id.as_deref(), Some("dead"));
        assert_eq!(row.link_version, v);
        assert!(row.writable_for(3, v, "new"));
        let trail = db.match_events("s", "قطعه").unwrap();
        let last = trail.last().unwrap();
        assert_eq!(last.actor, actor::REBUILD);
        assert_eq!(last.evidence.as_deref(), Some("reconstructed"));
        assert_eq!(last.old_product_id.as_deref(), Some("dead"));

        db.set_link_state("s", "قطعه", state::STALE, None, Some(3), actor::WRITE)
            .unwrap();
        let stale = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert!(!stale.writable_for(3, v, "new"));
        assert_eq!(
            db.match_events("s", "قطعه").unwrap().last().unwrap().actor,
            actor::WRITE
        );
    }

    fn pick<'a>(name: &'a str, product: LinkProduct<'a>, job: i64) -> LinkPut<'a> {
        LinkPut {
            name_seen: name,
            code: "",
            product: Some(product),
            how: "USER",
            similarity: None,
            state: state::VALID,
            state_reason: None,
            validated_job: Some(job),
            actor: actor::SELLER,
            evidence: Some("shown"),
            job_id: Some(job),
            decided_at: None,
            priced_at: None,
            confirmed: None,
        }
    }

    /// Slice 2's review: the version alone refuses a proof — a review pick in the same job keeps
    /// the link `VALID` for that job, and only its new version tells the old proof apart.
    #[test]
    fn the_version_alone_refuses_an_older_proof() {
        let db = db();
        let v1 = db.put_link("s", &pick("قطعه", product("p1"), 7)).unwrap();
        let v2 = db.put_link("s", &pick("قطعه", product("p2"), 7)).unwrap();
        let now = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(
            (now.state.as_str(), now.validated_job),
            (state::VALID, Some(7))
        );
        assert!(now.writable_for(7, v2, "p2"));
        assert!(
            !now.writable_for(7, v1, "p2"),
            "the old version is refused on its own"
        );
        assert!(!now.writable_for(7, v2, "p1"), "and so is another product");
    }

    /// Slice 2's review: a link cleared and created again never restarts its version, so a proof
    /// taken before the clear cannot match the new link.
    #[test]
    fn a_link_created_again_after_a_clear_continues_its_version() {
        let db = db();
        let v1 = db.put_link("s", &pick("قطعه", product("p1"), 7)).unwrap();
        db.clear_match("s", "قطعه").unwrap();
        let v2 = db.put_link("s", &pick("قطعه", product("q"), 7)).unwrap();
        assert!(v2 > v1, "{v1} then {v2}");
        let now = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert!(!now.writable_for(7, v1, "p1"));
    }

    /// Slice 2's review: VALIDATE's verdict and a rebuild land only on the link as the job read
    /// it; a seller's edit in between wins, and nothing of the job's is stamped on it.
    #[test]
    fn a_verdict_never_lands_on_a_link_the_seller_changed_since_the_read() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("dead", "الف")), "USER", None)
            .unwrap();
        let read = db.match_for_name("s", "قطعه").unwrap().unwrap();
        db.set_match("s", "قطعه", "1", Some(("q", "ب")), "USER", None)
            .unwrap();
        assert_eq!(
            db.rebuild_link(
                "s",
                "قطعه",
                read.link_version,
                "dead",
                product("new"),
                state::REBUILT,
                None,
                3
            )
            .unwrap(),
            None
        );
        assert_eq!(
            db.validate_link(
                "s",
                "قطعه",
                read.link_version,
                product("q"),
                state::VALID,
                None,
                3
            )
            .unwrap(),
            None
        );
        assert!(!db
            .set_link_state_if(
                "s",
                "قطعه",
                read.link_version,
                Some("dead"),
                state::STALE,
                None,
                Some(3),
                actor::VALIDATE
            )
            .unwrap());
        let now = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!(
            (
                now.product_id.as_deref(),
                now.state.as_str(),
                now.product_brand
            ),
            (Some("q"), state::UNVERIFIED, None)
        );
        // a fresh match never takes a name the seller decided meanwhile
        assert_eq!(
            db.put_link_if("s", &pick("قطعه", product("auto"), 3), Expect::Absent)
                .unwrap(),
            None
        );
    }
}
