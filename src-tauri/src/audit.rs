//! The audit (plan 130 R-i, task 9): every row of the seller's shelf weighed against the evidence,
//! before anything is written. Read-only by construction: the links are judged by the jobs' own
//! VALIDATE ([`Prover`]) on an in-memory copy of the database, and the rows' provenance is read from
//! that same copy, so the audit can never change a seller's link; Landau and Barezpakhsh are only
//! read.
//!
//! A row is listed when the evidence does not vouch for what it holds:
//! - a link on its product does not prove ([`Doubt::UnprovenLink`] — the next job holds the row);
//! - its proven links disagree on today's cost, or one of them has no quote today
//!   ([`Doubt::CostsDisagree`], Q-3 — as PRICE holds such a row);
//! - the bridge last wrote its cost from supplier items none of which is linked to its product any
//!   more ([`Doubt::WriterUnlinked`] — cleared, re-pointed or «not in Landau» since);
//! - a quantity import moved units onto it from an item that does not prove there
//!   ([`Doubt::UnprovenUnits`]).
//!
//! Apart from the doubts, every row whose last cost stands on an automatic link is listed
//! ([`AutoWrite`], Q-6, PI-21): a link the gate passed can still be a wrong part when Landau lacks the
//! true one, and the seller is the one who can tell. A row may be in both lists.
//!
//! A row whose decisive quote could not be read is counted apart as not weighed, never as
//! consistent. "Should hold" is the proven links' cost today, when they agree, and unknown otherwise —
//! never a figure from a link that does not prove, nor the row's own cost kept by the
//! unavailable-only-raises rule. A correction is the seller's: resolving the row's links in Matches,
//! then a REFRESH, whose report shows the cost before and after.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::db::jobs::ShelfWrite;
use crate::db::matches::MatchRow;
use crate::db::Db;
use crate::errors::Result;
use crate::inspect::Read;
use crate::jobs::proof::{Judgement, Prover};
use crate::landau::{LandauClient, ShelfRow};
use crate::matching::gate::Reason;
use crate::matching::normalize::name_key;
use crate::matching::registry::Registry;
use crate::settings::SUPPLIER_ID;
use crate::supplier::Supplier;

/// The job id the audit's VALIDATE runs under, on its copy only: no real job has it.
const AUDIT_JOB: i64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Doubt {
    UnprovenLink,
    CostsDisagree,
    WriterUnlinked,
    UnprovenUnits,
}

/// Today's quote for a link, as the cost the bridge would write from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Quoted {
    pub cost: i64,
    pub available: bool,
    /// The site's code and name for the item today.
    pub code: String,
    pub name: String,
    /// Found by the code under another name than the link's: the site renamed the item.
    pub renamed: bool,
}

/// What PRICE makes of a link's quote found by its code under another name (PI-26, PI-28), as the
/// audit weighs it (PI-29). Set only on a link that proves and whose quote was renamed into a
/// conflict; a rename that agrees with the product changes nothing and sets none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RenameState {
    /// Held, and waiting for the seller's word in Matches: a job recorded this code under this name.
    Waiting,
    /// Held, and Matches asks the seller about an older name of this code (`RenamedQuote::asked`):
    /// a run that prices this code puts today's name in its place.
    AskedOlder,
    /// Held, not asked in Matches yet: a run that prices this code records it. A run prices it only
    /// once every other link on the row proves, so this may wait on the row's other doubts.
    Unasked,
    /// The seller confirmed this code under this name for the product: written, as PRICE writes it.
    Confirmed,
}

/// A link's quote found by its code under a name that contradicts the product (PI-29).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenamedQuote {
    pub state: RenameState,
    /// The site's code and name for the item today.
    pub code: String,
    pub name: String,
    /// [`RenameState::AskedOlder`]: the name Matches asks about.
    pub asked: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditLink {
    pub name: String,
    pub code: String,
    /// `AUTO` | `USER`
    pub how: String,
    /// The link's state as VALIDATE left it on the copy.
    pub state: String,
    pub proven: bool,
    /// Why it does not prove (empty when it does).
    pub reasons: Vec<Reason>,
    /// Asked only where it decides something: a listed row, or a row of several links. `None`:
    /// not asked; `Known(None)`: the site lists neither the name nor the code today.
    pub today: Option<Read<Option<Quoted>>>,
    /// Today's quote renamed into a conflict (PI-29) — also on an automatic write, whose own
    /// `today` is never asked, when its row's weighing quoted the link.
    pub rename: Option<RenamedQuote>,
    /// Units a quantity import transferred onto this row from this link.
    pub units: i64,
}

/// The write that left the row's cost, and the supplier items it came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastWrite {
    pub job_id: i64,
    pub kind: String,
    pub at: String,
    pub names: Vec<String>,
    pub cost: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditRow {
    pub stock_id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub authenticity: Option<String>,
    pub inventory: i64,
    /// The cost and price the row holds, as Landau answered at `Audit::read_at`.
    pub cost: Option<i64>,
    pub price: Option<i64>,
    pub doubts: Vec<Doubt>,
    pub links: Vec<AuditLink>,
    /// The proven links' cost today, when they agree; `None` when the evidence names no cost.
    pub should_cost: Option<i64>,
    /// The site lists that cost as unavailable: a refresh keeps the row's cost where it is higher.
    pub should_unavailable: bool,
    /// `should_cost − cost`, in rials: positive when the row holds less than it should.
    pub difference: Option<i64>,
    pub last_write: Option<LastWrite>,
    /// Units transferred onto the row from supplier items whose link does not prove here.
    pub unproven_units: i64,
}

/// A shelf row whose last cost stands on an automatic link (Q-6, PI-21): of the items that made the
/// cost, one whose link is `AUTO` now and still points at the row's product. Listed whether or not
/// the evidence doubts the row — an automatic write the gate passed is PI-5's accepted residual when
/// Landau lacks the true product, and only the seller can see that. Never quoted: it asks nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AutoWrite {
    pub stock_id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub authenticity: Option<String>,
    pub inventory: i64,
    pub cost: Option<i64>,
    pub price: Option<i64>,
    /// The automatic links the last cost came from, as the copy's VALIDATE judged them.
    pub links: Vec<AuditLink>,
    pub last_write: LastWrite,
    /// The row is listed among the doubts as well.
    pub doubted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Audit {
    pub read_at: String,
    pub registry_read_at: String,
    pub store: String,
    /// Doubtful rows: an unknown "should" first, then the largest difference.
    pub rows: Vec<AuditRow>,
    /// Every row whose last cost stands on an automatic link, newest write first.
    pub automatic: Vec<AutoWrite>,
    /// Rows with a supplier link or a bridge write, weighed.
    pub examined: usize,
    /// Of those, the rows the evidence vouches for.
    pub consistent: usize,
    /// Of those, rows whose deciding quote could not be read — neither listed nor vouched for.
    pub unweighed: usize,
    /// Rows the bridge never linked or wrote — the seller's own.
    pub unlinked: usize,
}

/// Where the audit reads from.
pub struct Sources<'a> {
    pub db: &'a Db,
    pub registry: &'a Registry,
    pub registry_read_at: &'a str,
    pub landau: &'a LandauClient,
    pub store: &'a str,
    pub supplier: std::result::Result<&'a dyn Supplier, crate::errors::BridgeError>,
    /// Held for each quote: the screens share one supplier session, one request at a time.
    pub quote_lock: Option<&'a tokio::sync::Mutex<()>>,
    pub vat_percent: f64,
    pub threshold: u8,
}

/// One link as the copy's VALIDATE judged it.
struct Judged {
    link: MatchRow,
    proven: bool,
    reasons: Vec<Reason>,
}

/// Every stored link with a product, judged by VALIDATE on `copy`, by the product it lands on. A
/// dead link nothing could rebuild stays on the dead product it names, so a row still holding that
/// product is held by it.
async fn judge_links(
    copy: &Db,
    registry: &Registry,
    threshold: u8,
) -> Result<HashMap<String, Vec<Judged>>> {
    let prover = Prover {
        db: copy,
        registry,
        supplier: SUPPLIER_ID,
        job: AUDIT_JOB,
        threshold,
    };
    let mut out: HashMap<String, Vec<Judged>> = HashMap::new();
    for link in copy.linked_matches(SUPPLIER_ID)? {
        let judged = prover.stored(&link, &link.name_seen, &link.code).await?;
        let Some(now) = copy.link_by_key(SUPPLIER_ID, &link.name_key)? else {
            continue;
        };
        let (product, proven, reasons) = match judged {
            Judgement::Proven { product, .. } => (product.id, true, Vec::new()),
            Judgement::Held {
                link_product,
                reasons,
                ..
            } => match link_product.or_else(|| now.product_id.clone()) {
                Some(p) => (p, false, reasons),
                None => continue,
            },
            Judgement::NotInLandau => continue,
        };
        out.entry(product).or_default().push(Judged {
            link: now,
            proven,
            reasons,
        });
    }
    for links in out.values_mut() {
        links.sort_by(|a, b| a.link.name_key.cmp(&b.link.name_key));
    }
    Ok(out)
}

/// The link a write came from: its recorded key, else the item's name.
fn write_key(w: &ShelfWrite) -> String {
    w.link_key.clone().unwrap_or_else(|| name_key(&w.name))
}

/// A write whose cost the unavailable-only-raises rule kept as it was: the row's earlier cost, never
/// this item's — so it vouches for nothing.
fn kept_by_hold(w: &ShelfWrite) -> bool {
    w.available == Some(false) && w.cost_before.is_none_or(|before| before == w.cost)
}

/// Per shelf row: the write that left its cost (the latest one, with the items written together
/// with it — a collision group writes one cost from several), the keys of the links it came from,
/// and the units each link transferred onto the row.
type Provenance = HashMap<String, (LastWrite, Vec<String>, HashMap<String, i64>)>;

fn provenance(writes: Vec<ShelfWrite>) -> Provenance {
    let mut by_row: HashMap<String, Vec<ShelfWrite>> = HashMap::new();
    for w in writes {
        by_row.entry(w.stock_id.clone()).or_default().push(w);
    }
    let mut out = Provenance::new();
    for (stock, writes) in by_row {
        let mut units: HashMap<String, i64> = HashMap::new();
        for w in writes
            .iter()
            .filter(|w| w.kind == "IMPORT" && w.mode.as_deref() == Some("QUANTITY"))
        {
            *units.entry(write_key(w)).or_default() += w.qty;
        }
        let Some(latest) = writes
            .iter()
            .filter(|w| !kept_by_hold(w))
            .max_by_key(|w| w.item_id)
        else {
            continue;
        };
        let group: Vec<&ShelfWrite> = writes
            .iter()
            .filter(|w| w.job_id == latest.job_id && w.cost == latest.cost && !kept_by_hold(w))
            .collect();
        let mut names: Vec<String> = group.iter().map(|w| w.name.clone()).collect();
        names.dedup();
        let keys = group.iter().map(|w| write_key(w)).collect();
        let last = LastWrite {
            job_id: latest.job_id,
            kind: latest.kind.clone(),
            at: latest.at.clone(),
            names,
            cost: latest.cost,
        };
        out.insert(stock, (last, keys, units));
    }
    out
}

/// Per shelf row: the latest write of any kind, with its keys — what the automatic list reads beside
/// the last cost's own items (PI-21). Unlike [`provenance`] it keeps a write the unavailable-only-raises
/// hold may have kept: v1.1.1 recorded no cost before, so its writes at an unavailable price read as
/// kept, yet an automatic link still wrote on that row, and Q-6 wants every automatic write named.
fn latest_writes(writes: &[ShelfWrite]) -> HashMap<String, (LastWrite, Vec<String>)> {
    let mut by_row: HashMap<&str, Vec<&ShelfWrite>> = HashMap::new();
    for w in writes {
        by_row.entry(w.stock_id.as_str()).or_default().push(w);
    }
    let mut out = HashMap::new();
    for (stock, writes) in by_row {
        let Some(latest) = writes.iter().max_by_key(|w| w.item_id) else {
            continue;
        };
        let group: Vec<&&ShelfWrite> = writes
            .iter()
            .filter(|w| w.job_id == latest.job_id && w.cost == latest.cost)
            .collect();
        let mut names: Vec<String> = group.iter().map(|w| w.name.clone()).collect();
        names.dedup();
        let last = LastWrite {
            job_id: latest.job_id,
            kind: latest.kind.clone(),
            at: latest.at.clone(),
            names,
            cost: latest.cost,
        };
        out.insert(
            stock.to_string(),
            (last, group.iter().map(|w| write_key(w)).collect()),
        );
    }
    out
}

async fn quote(sources: &Sources<'_>, name: &str, code: &str) -> Read<Option<Quoted>> {
    let _one = match sources.quote_lock {
        Some(lock) => Some(lock.lock().await),
        None => None,
    };
    match &sources.supplier {
        Ok(s) => Read::of(
            crate::inspect::quote(*s, sources.vat_percent, name, code)
                .await
                .map(|q| {
                    q.map(|q| Quoted {
                        cost: q.cost,
                        available: q.available,
                        code: q.code,
                        name: q.name,
                        renamed: q.renamed,
                    })
                }),
        ),
        Err(e) => Read::Unknown(e.message_fa()),
    }
}

/// What one row came to.
enum Weighed {
    Listed(Box<AuditRow>),
    Consistent,
    Unweighed,
}

/// Weigh every row of the store's shelf. Reads the shelf, judges every link on a copy of the
/// database and quotes where a quote decides something; writes nothing anywhere.
pub async fn audit(sources: &Sources<'_>) -> Result<Audit> {
    let read_at = crate::db::now();
    let shelf = sources.landau.shelf(sources.store).await?;
    let copy = sources.db.scratch_copy()?;
    let judged = judge_links(&copy, sources.registry, sources.threshold).await?;
    let writes = copy.shelf_writes()?;
    let mut latest = latest_writes(&writes);
    let mut provenance = provenance(writes);
    let mut audit = Audit {
        read_at,
        registry_read_at: sources.registry_read_at.to_string(),
        store: sources.store.to_string(),
        rows: Vec::new(),
        automatic: Vec::new(),
        examined: 0,
        consistent: 0,
        unweighed: 0,
        unlinked: 0,
    };
    for row in &shelf {
        let links: &[Judged] = judged.get(&row.product).map(Vec::as_slice).unwrap_or(&[]);
        let written = provenance.remove(&row.id);
        let latest = latest.remove(&row.id);
        if links.is_empty() && written.is_none() {
            audit.unlinked += 1;
            continue;
        }
        audit.examined += 1;
        // The last cost's own items and the latest write's, whichever the hold may have kept: an
        // automatic link among either wrote on this row (PI-21).
        let mut writer_keys: Vec<String> = Vec::new();
        if let Some((_, k, _)) = &written {
            writer_keys.extend(k.iter().cloned());
        }
        if let Some((_, k)) = &latest {
            writer_keys.extend(k.iter().cloned());
        }
        let units = written
            .as_ref()
            .map(|(_, _, u)| u.clone())
            .unwrap_or_default();
        let last = latest
            .as_ref()
            .map(|(l, _)| l.clone())
            .or_else(|| written.as_ref().map(|(l, _, _)| l.clone()));
        let (weighed, verdicts) = weigh(sources, &copy, row, links, written).await?;
        // weighed first: an automatic write shows its link as the row's quote left it (PI-29)
        let automatic = {
            let by = automatic_writers(&copy, row, links, &writer_keys, &units, &verdicts)?;
            match last {
                Some(last) if !by.is_empty() => Some((last, by)),
                _ => None,
            }
        };
        let doubted = match weighed {
            Weighed::Listed(r) => {
                audit.rows.push(*r);
                true
            }
            Weighed::Consistent => {
                audit.consistent += 1;
                false
            }
            Weighed::Unweighed => {
                audit.unweighed += 1;
                false
            }
        };
        if let Some((last_write, links)) = automatic {
            audit.automatic.push(AutoWrite {
                stock_id: row.id.clone(),
                product_id: row.product.clone(),
                product_name: row.product_name.clone(),
                brand: row.brand.clone(),
                authenticity: row.authenticity.clone(),
                inventory: row.inventory,
                cost: row.cost,
                price: row.price,
                links,
                last_write,
                doubted,
            });
        }
    }
    sort_automatic(&mut audit.automatic);
    audit.rows.sort_by(|a, b| {
        a.should_cost
            .is_some()
            .cmp(&b.should_cost.is_some())
            .then(b.difference.map(i64::abs).cmp(&a.difference.map(i64::abs)))
            .then(a.stock_id.cmp(&b.stock_id))
    });
    Ok(audit)
}

/// The automatic links a row's last cost stands on: of the links the items that made it came from,
/// those `AUTO` now and still on the row's product, on the copy after its VALIDATE (PI-21). A link the
/// seller has since picked makes the cost the seller's; one moved since is [`Doubt::WriterUnlinked`].
/// No job item records its link's origin, so "automatic at the moment of the write" cannot be read.
/// A link whose quote the row's weighing judged (`verdicts`, by name key) reads as that verdict.
fn automatic_writers(
    copy: &Db,
    row: &ShelfRow,
    links: &[Judged],
    writer_keys: &[String],
    units: &HashMap<String, i64>,
    verdicts: &Verdicts,
) -> Result<Vec<AuditLink>> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for key in writer_keys {
        if !seen.insert(key.as_str()) {
            continue;
        }
        let Some(link) = copy.link_by_key(SUPPLIER_ID, key)? else {
            continue;
        };
        if link.how != "AUTO" || link.product_id.as_deref() != Some(row.product.as_str()) {
            continue;
        }
        let judged = links.iter().find(|j| j.link.name_key == link.name_key);
        let (proven, reasons, rename) = match verdicts.get(&link.name_key) {
            Some(v) => v.clone(),
            None => (
                judged.is_some_and(|j| j.proven),
                judged.map(|j| j.reasons.clone()).unwrap_or_default(),
                None,
            ),
        };
        out.push(AuditLink {
            proven,
            reasons,
            today: None,
            rename,
            units: units.get(&link.name_key).copied().unwrap_or(0),
            name: link.name_seen,
            code: link.code,
            how: link.how,
            state: link.state,
        });
    }
    Ok(out)
}

/// What a row's weighing made of each link it quoted, by name key: `(proven, reasons, rename)`.
type Verdicts = HashMap<String, (bool, Vec<Reason>, Option<RenamedQuote>)>;

/// A rename recorded on `link` for `product`, waiting or confirmed (PI-28): its quote decides
/// whether PRICE writes the row, so it is asked even on a row of one link (PI-29).
fn rename_on_record(link: &MatchRow, product: &str) -> bool {
    link.renamed().is_some_and(|r| r.product_id == product)
}

/// A proven link's quote through PRICE's rename rule (`jobs::rename_rule`), read-only: a code the
/// site renamed into a conflict nobody confirmed for that name proves no cost (PI-29). `None` when
/// the quote changes nothing.
fn judge_quote(
    registry: &Registry,
    link: &MatchRow,
    product: &str,
    q: &Quoted,
) -> Option<(bool, Vec<Reason>, RenamedQuote)> {
    use crate::jobs::{rename_rule, Costed, RenameRule};
    let costed = Costed {
        cost: q.cost,
        available: q.available,
        code: q.code.clone(),
        name: q.name.clone(),
        renamed: q.renamed,
    };
    let renamed = |state| RenamedQuote {
        state,
        code: q.code.clone(),
        name: q.name.clone(),
        asked: None,
    };
    match rename_rule(registry, Some(link), product, &costed) {
        RenameRule::Writes => None,
        RenameRule::Confirmed => Some((true, Vec::new(), renamed(RenameState::Confirmed))),
        RenameRule::Holds(reasons) => {
            // Matches asks about any unconfirmed record on the link's product (its `waitingRename`):
            // this code under this name once a job recorded it (PI-28), else an older name
            let waiting = link
                .renamed()
                .filter(|r| r.confirmed_at.is_none() && r.product_id == product);
            let held = match waiting {
                Some(r) if r.code == q.code && name_key(&r.name) == name_key(&q.name) => {
                    renamed(RenameState::Waiting)
                }
                Some(r) => RenamedQuote {
                    asked: Some(r.name),
                    ..renamed(RenameState::AskedOlder)
                },
                None => renamed(RenameState::Unasked),
            };
            Some((false, reasons, held))
        }
    }
}

/// One row, from its links and the write that left its cost; and what it made of each link's
/// quote.
async fn weigh(
    sources: &Sources<'_>,
    copy: &Db,
    row: &ShelfRow,
    links: &[Judged],
    written: Option<(LastWrite, Vec<String>, HashMap<String, i64>)>,
) -> Result<(Weighed, Verdicts)> {
    let (last, writer_keys, units) = match written {
        Some((last, keys, units)) => (Some(last), keys, units),
        None => (None, Vec::new(), HashMap::new()),
    };
    let mut doubts = Vec::new();
    if links.iter().any(|l| !l.proven) {
        doubts.push(Doubt::UnprovenLink);
    }
    if last.is_some() {
        let mut vouched = false;
        for key in &writer_keys {
            vouched |= copy
                .link_by_key(SUPPLIER_ID, key)?
                .is_some_and(|l| l.product_id.as_deref() == Some(row.product.as_str()));
        }
        if !vouched {
            doubts.push(Doubt::WriterUnlinked);
        }
    }
    // units from links that do not prove here: one held on this row, or one no longer on it at all
    let proven_keys: BTreeSet<&str> = links
        .iter()
        .filter(|l| l.proven)
        .map(|l| l.link.name_key.as_str())
        .collect();
    let unproven_units: i64 = units
        .iter()
        .filter(|(k, _)| !proven_keys.contains(k.as_str()))
        .map(|(_, u)| u)
        .sum();
    if unproven_units != 0 {
        doubts.push(Doubt::UnprovenUnits);
    }
    let listed = !doubts.is_empty();
    let mut verdicts = Verdicts::new();
    let decisive = listed
        || links.len() >= 2
        || links
            .iter()
            .any(|l| rename_on_record(&l.link, &row.product));
    if !decisive {
        return Ok((Weighed::Consistent, verdicts));
    }
    let mut out = Vec::with_capacity(links.len());
    for l in links {
        let today = if l.proven || listed {
            Some(quote(sources, &l.link.name_seen, &l.link.code).await)
        } else {
            None
        };
        // PRICE holds a quote found by code under a name that contradicts the product (PI-26), so
        // its cost proves nothing here either (PI-29); units stay judged on the link, whose own name
        // the invoices carried
        let (proven, reasons, rename) = match &today {
            Some(Read::Known(Some(q))) if l.proven => {
                match judge_quote(sources.registry, &l.link, &row.product, q) {
                    Some((proven, reasons, renamed)) => {
                        verdicts.insert(
                            l.link.name_key.clone(),
                            (proven, reasons.clone(), Some(renamed.clone())),
                        );
                        (proven, reasons, Some(renamed))
                    }
                    None => (true, Vec::new(), None),
                }
            }
            _ => (l.proven, l.reasons.clone(), None),
        };
        out.push(AuditLink {
            name: l.link.name_seen.clone(),
            code: l.link.code.clone(),
            how: l.link.how.clone(),
            state: l.link.state.clone(),
            proven,
            reasons,
            today,
            rename,
            units: units.get(&l.link.name_key).copied().unwrap_or(0),
        });
    }
    if out.iter().any(|l| !l.proven) && !doubts.contains(&Doubt::UnprovenLink) {
        doubts.insert(0, Doubt::UnprovenLink);
    }
    // the proven links' quotes, as PRICE weighs them: one cost between them, a link the site no
    // longer lists holding the row like a second cost, and a quote that could not be read leaving
    // the row not weighed
    let proven: Vec<&AuditLink> = out.iter().filter(|l| l.proven).collect();
    let mut costs: BTreeSet<i64> = BTreeSet::new();
    let (mut missing, mut unread) = (false, false);
    for l in &proven {
        match &l.today {
            Some(Read::Known(Some(q))) => {
                costs.insert(q.cost);
            }
            Some(Read::Known(None)) => missing = true,
            _ => unread = true,
        }
    }
    if costs.len() > 1 || (missing && proven.len() > 1) {
        doubts.push(Doubt::CostsDisagree);
    }
    if doubts.is_empty() {
        let weighed = if unread {
            Weighed::Unweighed
        } else {
            Weighed::Consistent
        };
        return Ok((weighed, verdicts));
    }
    let should_cost = match (unread || missing, costs.len()) {
        (false, 1) => costs.iter().next().copied(),
        _ => None,
    };
    let should_unavailable = should_cost.is_some()
        && proven
            .iter()
            .any(|l| matches!(&l.today, Some(Read::Known(Some(q))) if !q.available));
    let listed = AuditRow {
        stock_id: row.id.clone(),
        product_id: row.product.clone(),
        product_name: row.product_name.clone(),
        brand: row.brand.clone(),
        authenticity: row.authenticity.clone(),
        inventory: row.inventory,
        cost: row.cost,
        price: row.price,
        difference: should_cost.zip(row.cost).map(|(s, c)| s - c),
        should_cost,
        should_unavailable,
        doubts,
        links: out,
        last_write: last,
        unproven_units,
    };
    Ok((Weighed::Listed(Box::new(listed)), verdicts))
}

// ---------------------------------------------------------------------------------------------
// CSV

/// A reason in the seller's words — the same sentences the review screen shows
/// (`app/utils/evidence.ts`).
pub fn reason_fa(r: &Reason) -> String {
    let list = |v: &[String]| v.join("، ");
    match r {
        Reason::BelowThreshold {
            similarity,
            threshold,
        } => format!("شباهت {similarity} از حد {threshold} کمتر است"),
        Reason::Tied => "پیشنهاد دیگری همین شباهت را دارد".into(),
        Reason::AnotherPasses { .. } => "پیشنهاد دیگری هم پذیرفتنی است".into(),
        Reason::NumberConflict { lacking } => {
            format!("عدد ناسازگار: {} در نام بارزپخش نیست", list(lacking))
        }
        Reason::NumberUnbacked { candidate } => {
            format!("عدد بی‌پشتوانه: {} در نام بارزپخش نیامده", list(candidate))
        }
        Reason::WordsUncovered { words } => format!("در نام بارزپخش نیست: {}", list(words)),
        Reason::OppositeWords {
            supplier,
            candidate,
        } => format!("ناسازگار: «{supplier}» در بارزپخش، «{candidate}» در قطعه"),
        Reason::PartWordsMissing { words } => format!("در قطعه نیست: {}", list(words)),
        Reason::NoBrandInName => "برندی در نام بارزپخش پیدا نشد".into(),
        Reason::SeveralBrandsInName { brands } => {
            format!("چند برند در نام بارزپخش: {}", list(brands))
        }
        Reason::BrandUnsure { supplier } => {
            format!("برند نامطمئن: «{supplier}» واژهٔ رایج هم هست")
        }
        Reason::CandidateWithoutBrand => "قطعهٔ لاندا برند ندارد".into(),
        Reason::BrandMismatch {
            supplier,
            candidate,
        } => format!("برند ناسازگار: «{supplier}» در بارزپخش، «{candidate}» در قطعه"),
        Reason::DefinitionNotExact => "تعریف قطعه دقیقاً یکی نیست".into(),
    }
}

pub fn doubt_fa(d: Doubt) -> &'static str {
    match d {
        Doubt::UnprovenLink => "پیوندی روی این ردیف اثبات نمی‌شود",
        Doubt::CostsDisagree => "پیوندهای اثبات‌شدهٔ این ردیف بر سر بهای خرید هم‌نظر نیستند",
        Doubt::WriterUnlinked => "آخرین بها را کالایی نوشته که دیگر به این قطعه وصل نیست",
        Doubt::UnprovenUnits => "بخشی از موجودی از کالایی منتقل شده که پیوندش اینجا اثبات نمی‌شود",
    }
}

const UNKNOWN: &str = "نامعلوم";

fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt(n: Option<i64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_else(|| UNKNOWN.into())
}

/// A text cell a spreadsheet must never read as a formula: a catalogue name may begin with one.
fn text(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

/// Rials as toman: whole when it divides, else to one decimal.
fn toman(rials: i64) -> String {
    if rials % 10 == 0 {
        (rials / 10).to_string()
    } else {
        format!("{:.1}", rials as f64 / 10.0)
    }
}

/// What today's quote says of a code the site renamed, in the seller's words — the same sentences
/// the audit page shows (`app/utils/audit.ts`). `None` when the quote was not renamed.
pub fn rename_fa(l: &AuditLink) -> Option<String> {
    // a run prices a code only once every other link on its row proves (`refresh::price_row`)
    const HELD: &str = "با قطعهٔ لاندا جور نیست و بهایش حساب نشد";
    const AFTER: &str = "اگر پیوند دیگری روی این ردیف اثبات نشود، پس از رفع آن";
    if let Some(r) = &l.rename {
        let then = match r.state {
            RenameState::Waiting => format!("{HELD}؛ در «تطبیق‌ها» منتظر تأیید شماست"),
            RenameState::AskedOlder => format!(
                "{HELD}؛ «تطبیق‌ها» هنوز نام پیشین «{}» را می‌پرسد و کاری که بهای این کد را بخواند نام امروز را جای آن می‌گذارد، {AFTER}",
                r.asked.as_deref().unwrap_or_default()
            ),
            RenameState::Unasked => format!(
                "{HELD}؛ هنوز در «تطبیق‌ها» پرسیده نشده و کاری که بهای این کد را بخواند آن را به «تطبیق‌ها» می‌فرستد، {AFTER}"
            ),
            RenameState::Confirmed => "این نام تازه را خودتان تأیید کرده‌اید".into(),
        };
        return Some(format!("نام امروز این کد در بارزپخش: «{}»؛ {then}", r.name));
    }
    match &l.today {
        Some(Read::Known(Some(q))) if q.renamed => {
            Some(format!("نام امروز این کد در بارزپخش: «{}»", q.name))
        }
        _ => None,
    }
}

/// A link VALIDATE proves that reads «اثبات‌نشده» only because its quote was renamed into a
/// conflict (PI-29): its units still count as proven.
fn held_for_rename(l: &AuditLink) -> bool {
    l.rename
        .as_ref()
        .is_some_and(|r| r.state != RenameState::Confirmed)
}

fn link_fa(l: &AuditLink) -> String {
    let verdict = if l.proven {
        "اثبات‌شده".to_string()
    } else {
        let why: Vec<String> = l.reasons.iter().map(reason_fa).collect();
        format!("اثبات‌نشده: {}", why.join("؛ "))
    };
    let today = match &l.today {
        None => String::new(),
        Some(Read::Known(Some(q))) => format!(
            " — بهای خرید امروز {} ریال{}",
            q.cost,
            if q.available { "" } else { " (ناموجود)" }
        ),
        Some(Read::Known(None)) => " — امروز در بارزپخش نیست".into(),
        Some(Read::Unknown(why)) => format!(" — بهای امروز {UNKNOWN}: {why}"),
    };
    let units = match (l.units, held_for_rename(l)) {
        (0, _) => String::new(),
        (n, true) => format!(" — {n} واحد منتقل‌شده، {UNITS_UNDER_OWN_NAME}"),
        (n, false) => format!(" — {n} واحد منتقل‌شده"),
    };
    let rename = rename_fa(l).map(|r| format!(" — {r}")).unwrap_or_default();
    format!(
        "«{}» ({}) — {verdict}{today}{rename}{units}",
        l.name, l.code
    )
}

/// Appended to a doubt row's reason in the CSV when the row is an automatic write too: a row in
/// both lists says so on both sides.
pub const ALSO_AUTOMATIC: &str = "نوشتهٔ خودکار هم هست";

/// Beside the units of a link held only for its renamed quote (PI-29): they came under the invoice's
/// own name, which proves, so the row does not count them as unproven.
pub const UNITS_UNDER_OWN_NAME: &str = "با نامی که در فاکتور آمده و اثبات می‌شود";

/// Newest write first (a code added after the catalog import is the residual's case), the row id
/// breaking a tie.
fn sort_automatic(v: &mut [AutoWrite]) {
    v.sort_by(|a, b| {
        b.last_write
            .job_id
            .cmp(&a.last_write.job_id)
            .then(a.stock_id.cmp(&b.stock_id))
    });
}

/// What the CSV's last column says a row is: a doubt, or an automatic write (PI-21).
pub const KIND_DOUBT: &str = "تردید";
pub const KIND_AUTOMATIC: &str = "نوشتهٔ خودکار";

/// Why an automatic write is listed, in the seller's words.
pub fn automatic_fa(doubted: bool) -> String {
    let mut why = "آخرین بها را پیوندی خودکار نوشته است؛ درستی قطعه را خودتان ببینید".to_string();
    if doubted {
        why.push_str("؛ در فهرست تردیدها هم آمده");
    }
    why
}

fn last_write_fa(w: &LastWrite) -> String {
    format!(
        "کار #{} ({}) · {} · {} ریال · {}",
        w.job_id,
        w.kind,
        w.at,
        w.cost,
        w.names
            .iter()
            .map(|n| format!("«{n}»"))
            .collect::<Vec<_>>()
            .join(" + ")
    )
}

/// The audit as a CSV file (R-i): UTF-8 with a BOM so a spreadsheet reads the Persian, one line per
/// listed row, rials and toman side by side, Latin digits in the number columns, RFC 4180 quoting.
/// The doubts come first, then every automatic write (PI-21); the last column names which. Nothing
/// was asked for an automatic write, so its evidence cells are empty — never «نامعلوم».
pub fn csv(a: &Audit) -> String {
    let header = [
        "شناسه ردیف",
        "قطعه",
        "برند",
        "اصالت",
        "موجودی",
        "بهای خرید روی قفسه (ریال)",
        "بهای خرید بر پایهٔ شواهد (ریال)",
        "اختلاف (ریال)",
        "اختلاف (تومان)",
        "علت",
        "پیوندها",
        "آخرین نوشتن",
        "واحدهای منتقل‌شده از پیوندهای اثبات‌نشده",
        "فروشگاه",
        "خوانده‌شده در",
        "نوع ردیف",
    ];
    let mut out = String::from("\u{feff}");
    let mut line = |cells: [String; 16]| {
        let line: Vec<String> = cells.iter().map(|c| field(c)).collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    };
    line(header.map(String::from));
    let automatic: BTreeSet<&str> = a.automatic.iter().map(|r| r.stock_id.as_str()).collect();
    for r in &a.rows {
        let mut why: Vec<&str> = r.doubts.iter().map(|d| doubt_fa(*d)).collect();
        if automatic.contains(r.stock_id.as_str()) {
            why.push(ALSO_AUTOMATIC);
        }
        let links: Vec<String> = r.links.iter().map(link_fa).collect();
        let last = r.last_write.as_ref().map(last_write_fa).unwrap_or_default();
        let mut should = opt(r.should_cost);
        if r.should_unavailable {
            should.push_str(" (ناموجود)");
        }
        line([
            r.stock_id.clone(),
            text(r.product_name.as_deref().unwrap_or(&r.product_id)),
            text(r.brand.as_deref().unwrap_or_default()),
            r.authenticity.clone().unwrap_or_default(),
            r.inventory.to_string(),
            r.cost.map(|c| c.to_string()).unwrap_or_default(),
            should,
            opt(r.difference),
            r.difference.map(toman).unwrap_or_else(|| UNKNOWN.into()),
            why.join("؛ "),
            text(&links.join(" | ")),
            text(&last),
            r.unproven_units.to_string(),
            a.store.clone(),
            a.read_at.clone(),
            KIND_DOUBT.into(),
        ]);
    }
    for r in &a.automatic {
        let links: Vec<String> = r.links.iter().map(link_fa).collect();
        line([
            r.stock_id.clone(),
            text(r.product_name.as_deref().unwrap_or(&r.product_id)),
            text(r.brand.as_deref().unwrap_or_default()),
            r.authenticity.clone().unwrap_or_default(),
            r.inventory.to_string(),
            r.cost.map(|c| c.to_string()).unwrap_or_default(),
            String::new(),
            String::new(),
            String::new(),
            automatic_fa(r.doubted),
            text(&links.join(" | ")),
            text(&last_write_fa(&r.last_write)),
            String::new(),
            a.store.clone(),
            a.read_at.clone(),
            KIND_AUTOMATIC.into(),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::incident::{
        self, after_job_12, ROW_9CEA, ROW_9CED, ROW_9CF0, ROW_9D12, ROW_9D1E,
    };
    use crate::landau::fake::FakeLandau;
    use crate::matching::evidence::tests::{
        bearing_6c04, clamp_57a5, CLAMP_4, MID_SHAFT, RIGHT_BEARING, WRONG_BEARING,
    };
    use crate::supplier::fake::FakeSupplier;

    async fn run(db: &Db, landau: &FakeLandau, supplier: &FakeSupplier) -> Audit {
        let registry = Registry::from_products(incident::registry());
        let client = LandauClient::new(&landau.url, "apt-test").unwrap();
        audit(&Sources {
            db,
            registry: &registry,
            registry_read_at: "2026-11-09T09:00:00Z",
            landau: &client,
            store: incident::STORE,
            supplier: Ok(supplier as &dyn Supplier),
            quote_lock: None,
            vat_percent: 10.0,
            threshold: 70,
        })
        .await
        .unwrap()
    }

    /// `(name key, state, product, version, rename record)` of every link, the trail's length, the
    /// jobs' count.
    type Fingerprint = (
        Vec<(String, String, Option<String>, i64, Option<String>)>,
        i64,
        i64,
    );

    /// What the audit may never change: the seller's links and their trail, the jobs.
    fn fingerprint(db: &Db) -> Fingerprint {
        let links = db
            .list_matches(SUPPLIER_ID)
            .unwrap()
            .into_iter()
            .map(|l| {
                (
                    l.name_key,
                    l.state,
                    l.product_id,
                    l.link_version,
                    l.renamed_json,
                )
            })
            .collect();
        let count = |sql: &str| db.with(|c| c.query_row(sql, [], |r| r.get(0))).unwrap();
        (
            links,
            count("SELECT count(*) FROM match_events"),
            count("SELECT count(*) FROM jobs"),
        )
    }

    /// Task 9: on the shelf job 12 left, the audit lists the bearing row (a link that does not
    /// prove; the right bearing's proven quote says 6,194,100, 694,100 rials more than it holds)
    /// and the clamp row (both links conflict; no cost the evidence can name) — and not the four
    /// consistent rows. It writes nothing: not the seller's links, not Landau.
    #[tokio::test]
    async fn the_audit_lists_the_two_damaged_rows_and_writes_nothing() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let before = fingerprint(&db);
        let a = run(&db, &landau, &incident::supplier()).await;

        let ids: Vec<&str> = a.rows.iter().map(|r| r.stock_id.as_str()).collect();
        assert_eq!(ids, [ROW_9CF0, ROW_9D1E], "an unknown cost first");
        assert_eq!((a.examined, a.consistent, a.unlinked), (6, 4, 0));

        let bearing = &a.rows[1];
        assert_eq!(bearing.doubts, [Doubt::UnprovenLink]);
        assert_eq!(
            (bearing.cost, bearing.should_cost, bearing.difference),
            (Some(5_500_000), Some(6_194_100), Some(694_100))
        );
        let by = |name: &str| bearing.links.iter().find(|l| l.name == name).unwrap();
        assert!(by(RIGHT_BEARING.1).proven);
        let wrong = by(WRONG_BEARING.1);
        assert!(!wrong.proven);
        assert!(
            wrong
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::NumberConflict { .. })),
            "{:?}",
            wrong.reasons
        );
        assert_eq!(
            wrong.today,
            Some(Read::Known(Some(Quoted {
                cost: 5_500_000,
                available: true,
                code: WRONG_BEARING.0.into(),
                name: WRONG_BEARING.1.into(),
                renamed: false,
            }))),
            "every link of a listed row is quoted, so the seller sees each cost"
        );
        assert_eq!(
            bearing.last_write.as_ref().unwrap().names,
            [WRONG_BEARING.1.to_string()]
        );

        let clamp = &a.rows[0];
        assert_eq!(clamp.doubts, [Doubt::UnprovenLink]);
        assert_eq!((clamp.should_cost, clamp.difference), (None, None));
        assert_eq!(clamp.links.len(), 2);
        assert!(clamp.links.iter().all(|l| !l.proven));

        assert_eq!(
            fingerprint(&db),
            before,
            "the seller's database is unchanged"
        );
        assert!(
            crate::landau::stub::writes(&landau.seen).is_empty(),
            "nothing but reads sent to Landau, reprice included"
        );
    }

    /// The seller clears the hurried pick: the bearing row's remaining link proves, but its cost
    /// was written by an item no longer linked to it — still listed, until a refresh rewrites it.
    #[tokio::test]
    async fn a_cost_written_by_an_item_no_longer_linked_is_listed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        db.clear_match(SUPPLIER_ID, WRONG_BEARING.1).unwrap();
        let a = run(&db, &landau, &incident::supplier()).await;
        let bearing = a.rows.iter().find(|r| r.stock_id == ROW_9D1E).unwrap();
        assert_eq!(bearing.doubts, [Doubt::WriterUnlinked]);
        assert_eq!(bearing.should_cost, Some(6_194_100));
        assert_eq!(bearing.links.len(), 1);
    }

    /// Two proven links on one row that price it differently today (Q-3): listed, no cost named.
    /// And units a quantity import moved onto a row from an item that does not prove are counted.
    #[tokio::test]
    async fn disagreeing_costs_and_unproven_units_are_listed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let other = ("AR1330-9", "بست کمري 370 7/6 تايوان آراد پارت");
        incident::link(&db, other, "USER", &clamp_57a5());
        let mut items: Vec<(&str, &str, i64, i64)> = incident::PRICES
            .iter()
            .map(|((c, n), p)| (*c, *n, 1, *p))
            .collect();
        items.push((other.0, other.1, 1, 160_000));
        let supplier = FakeSupplier::new(&items);

        let job = db
            .create_job(
                "IMPORT",
                Some("QUANTITY"),
                Some("1405/08/15"),
                incident::STORE,
                false,
            )
            .unwrap();
        let id = db
            .add_job_item(job, WRONG_BEARING.0, WRONG_BEARING.1, 3)
            .unwrap();
        db.update_job_item(
            id,
            &[
                ("stock_id", crate::jobs::t(ROW_9D1E)),
                ("cost", crate::jobs::i(5_500_000)),
                ("status", crate::jobs::t("DONE")),
            ],
        )
        .unwrap();
        db.set_job_status(job, "DONE", "REPORT").unwrap();

        let a = run(&db, &landau, &supplier).await;
        let clamp = a.rows.iter().find(|r| r.stock_id == ROW_9CEA).unwrap();
        assert_eq!(clamp.doubts, [Doubt::CostsDisagree]);
        assert_eq!(clamp.should_cost, None);
        let bearing = a.rows.iter().find(|r| r.stock_id == ROW_9D1E).unwrap();
        assert_eq!(bearing.unproven_units, 3);
        assert_eq!(a.consistent, 3);
    }

    /// R-i's correction is the seller's: the audit lists the bearing row; the seller re-points the
    /// hurried pick at its own product in Matches; the next REFRESH writes the row's proven cost,
    /// its report naming the cost before and after; and the audit no longer lists the row.
    #[tokio::test]
    async fn a_listed_row_is_corrected_by_the_refresh_after_the_seller_resolves_it() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let a = run(&db, &landau, &incident::supplier()).await;
        assert!(a.rows.iter().any(|r| r.stock_id == ROW_9D1E));
        assert!(landau.writes().is_empty());

        let registry = Registry::from_products(incident::registry());
        let judged = crate::inspect::judge_pick(
            &registry,
            WRONG_BEARING.1,
            WRONG_BEARING.0,
            &incident::bearing_5e00().id,
            70,
        )
        .unwrap();
        crate::jobs::import::matches_pick(&db, WRONG_BEARING.1, WRONG_BEARING.0, &judged, None)
            .unwrap();

        let svc = incident::services(&landau);
        let ctx = incident::ctx(&db, "REFRESH");
        let outcome = crate::jobs::refresh::run(&ctx, &svc).await;
        ctx.finish(&outcome, "REPORT");
        outcome.unwrap();
        assert_eq!(landau.row(ROW_9D1E).unwrap().cost, Some(6_194_100));
        let item = db
            .job_items(ctx.job_id)
            .unwrap()
            .into_iter()
            .find(|i| i.stock_id.as_deref() == Some(ROW_9D1E))
            .unwrap();
        assert_eq!(
            (item.cost_before, item.cost, item.action.as_deref()),
            (Some(5_500_000), Some(6_194_100), Some("PATCH")),
            "the report shows the cost before and after"
        );

        let a = run(&db, &landau, &incident::supplier()).await;
        let ids: Vec<&str> = a.rows.iter().map(|r| r.stock_id.as_str()).collect();
        assert_eq!(ids, [ROW_9CF0], "the clamp row waits on the seller still");
    }

    fn item_on(db: &Db, job: i64, item: (&str, &str), stock: &str, patch: &[(&str, i64)]) -> i64 {
        let id = db.add_job_item(job, item.0, item.1, 0).unwrap();
        let mut fields = vec![
            ("stock_id", crate::jobs::t(stock)),
            ("status", crate::jobs::t("DONE")),
        ];
        fields.extend(patch.iter().map(|(k, v)| (*k, crate::jobs::i(*v))));
        db.update_job_item(id, &fields).unwrap();
        id
    }

    /// Review finding 1: a refresh whose cost the unavailable-only-raises rule kept vouches for
    /// nothing, and "should" is the proven quote as the site lists it — never the row's own cost.
    #[tokio::test]
    async fn a_cost_the_hold_kept_vouches_for_nothing() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        db.clear_match(SUPPLIER_ID, WRONG_BEARING.1).unwrap();
        // a later refresh, the right bearing unavailable and cheaper: the row's cost kept
        let job = db
            .create_job("REFRESH", None, None, incident::STORE, false)
            .unwrap();
        item_on(
            &db,
            job,
            RIGHT_BEARING,
            ROW_9D1E,
            &[
                ("cost", 5_500_000),
                ("cost_before", 5_500_000),
                ("available", 0),
            ],
        );
        db.set_job_status(job, "DONE", "REPORT").unwrap();
        let mut items: Vec<(&str, &str, i64, i64)> = incident::PRICES
            .iter()
            .map(|((c, n), p)| {
                (
                    *c,
                    *n,
                    1,
                    if *c == RIGHT_BEARING.0 { 4_000_000 } else { *p },
                )
            })
            .collect();
        items.truncate(8);
        let supplier = FakeSupplier::new(&items).unavailable(RIGHT_BEARING.0);
        let a = run(&db, &landau, &supplier).await;
        let bearing = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9D1E)
            .expect("still listed");
        assert_eq!(bearing.doubts, [Doubt::WriterUnlinked]);
        assert_eq!(
            (
                bearing.should_cost,
                bearing.should_unavailable,
                bearing.difference
            ),
            (Some(4_400_000), true, Some(-1_100_000))
        );
        assert_eq!(
            bearing.last_write.as_ref().unwrap().names,
            [WRONG_BEARING.1.to_string()]
        );
    }

    /// Review finding 2: a write whose reply was lost, settled as landed by a later job, is still
    /// the row's provenance — though its item never got a row id.
    #[tokio::test]
    async fn a_write_settled_after_a_lost_reply_counts() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        db.clear_match(SUPPLIER_ID, WRONG_BEARING.1).unwrap();
        db.with(|c| {
            c.execute_batch(&format!(
                "UPDATE job_items SET status = 'FAILED', stock_id = NULL WHERE code = '{}';
                 INSERT INTO write_intents(job_id, kind, store, product, authenticity, stock_id, verb,
                   snap_where, target_cost, items_json, status, opened_at, closed_at)
                 SELECT job_id, 'IMPORT', '{}', '{}', 'ORIGINAL', '{}', 'ATTACH', 'ABSENT', 5500000,
                   json_array(id), 'LANDED', '2026-11-06T06:00:00Z', '2026-11-06T06:10:00Z'
                 FROM job_items WHERE code = '{}';",
                WRONG_BEARING.0,
                incident::STORE,
                crate::matching::evidence::tests::bearing_5e12().id,
                ROW_9D1E,
                WRONG_BEARING.0
            ))
        })
        .unwrap();
        let a = run(&db, &landau, &incident::supplier()).await;
        let bearing = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9D1E)
            .expect("listed");
        assert_eq!(bearing.doubts, [Doubt::WriterUnlinked]);
    }

    /// Review finding 3: the last write is the items that made it — one job writing a row twice
    /// (v1.1.1's clamps) names the second item only; a collision group names every item of it.
    #[test]
    fn the_last_write_names_only_the_items_that_made_it() {
        let w = |item_id: i64, name: &str, cost: i64| ShelfWrite {
            item_id,
            job_id: 12,
            kind: "IMPORT".into(),
            mode: Some("ZERO".into()),
            at: "t".into(),
            code: String::new(),
            name: name.into(),
            link_key: None,
            stock_id: "row".into(),
            cost,
            cost_before: None,
            available: Some(true),
            qty: 0,
        };
        let p = provenance(vec![w(1, "CW3.6", 91_300), w(2, "CW4.8", 96_800)]);
        assert_eq!(p["row"].0.names, ["CW4.8"]);
        let p = provenance(vec![w(1, "a", 50_000), w(2, "b", 50_000)]);
        assert_eq!(p["row"].0.names, ["a", "b"]);
        // the link the item was written from wins over its name
        let mut keyed = w(3, "renamed", 70_000);
        keyed.link_key = Some("the-link".into());
        assert_eq!(provenance(vec![keyed])["row"].1, ["the-link"]);
    }

    /// Review finding 4: a proven link the site no longer lists holds a row of two links, as PRICE
    /// does; and a quote that cannot be read leaves such a row not weighed — never consistent.
    #[tokio::test]
    async fn a_missing_quote_disagrees_and_an_unread_one_is_not_weighed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let other = ("AR1330-9", "بست کمري 370 7/6 تايوان آراد پارت");
        incident::link(&db, other, "USER", &clamp_57a5());
        // the site does not list `other` today
        let a = run(&db, &landau, &incident::supplier()).await;
        let clamp = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9CEA)
            .expect("listed");
        assert_eq!(clamp.doubts, [Doubt::CostsDisagree]);
        assert_eq!(clamp.should_cost, None);

        let registry = Registry::from_products(incident::registry());
        let client = LandauClient::new(&landau.url, "apt-test").unwrap();
        let a = audit(&Sources {
            db: &db,
            registry: &registry,
            registry_read_at: "t",
            landau: &client,
            store: incident::STORE,
            supplier: Err(crate::errors::BridgeError::SupplierUnreachable(
                "down".into(),
            )),
            quote_lock: None,
            vat_percent: 10.0,
            threshold: 70,
        })
        .await
        .unwrap();
        assert_eq!(
            (a.consistent, a.unweighed),
            (3, 1),
            "the two-link row is not weighed"
        );
        assert!(!a.rows.iter().any(|r| r.stock_id == ROW_9CEA));
    }

    /// Review finding 5: a link nothing could rebuild still holds the row on the dead product it names.
    #[tokio::test]
    async fn a_stale_link_holds_the_row_on_its_dead_product() {
        let db = Db::in_memory().unwrap();
        let gone = crate::matching::evidence::tests::cand(
            "dead-0001",
            "قطعه‌ای که دیگر نیست",
            None,
            Some("عظام"),
        );
        let item = ("X1", "قطعه‌ای که دیگر نیست عظام");
        incident::link(&db, item, "USER", &gone);
        let landau = FakeLandau::start(
            incident::STORE,
            incident::registry(),
            vec![crate::landau::fake::Row {
                id: "row-dead".into(),
                product: gone.id.clone(),
                inventory: 0,
                cost: Some(1_000),
                price: Some(2_000),
                authenticity: "ORIGINAL".into(),
                deleted: false,
                updated: 1,
            }],
        );
        let a = run(&db, &landau, &incident::supplier()).await;
        assert_eq!(a.rows.len(), 1, "{:?}", a.rows);
        assert_eq!(a.rows[0].doubts, [Doubt::UnprovenLink]);
        assert!(!a.rows[0].links[0].proven);
    }

    /// Review finding 6: units a wrong item moved onto a row keep it listed after its cost is right.
    #[tokio::test]
    async fn units_from_an_unproven_item_keep_the_row_listed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let job = db
            .create_job(
                "IMPORT",
                Some("QUANTITY"),
                Some("1405/08/15"),
                incident::STORE,
                false,
            )
            .unwrap();
        let id = item_on(&db, job, WRONG_BEARING, ROW_9D1E, &[("cost", 5_500_000)]);
        db.with(|c| c.execute("UPDATE job_items SET qty = 3 WHERE id = ?1", [id]))
            .unwrap();
        db.set_job_status(job, "DONE", "REPORT").unwrap();
        let judged = crate::inspect::judge_pick(
            &Registry::from_products(incident::registry()),
            WRONG_BEARING.1,
            WRONG_BEARING.0,
            &incident::bearing_5e00().id,
            70,
        )
        .unwrap();
        crate::jobs::import::matches_pick(&db, WRONG_BEARING.1, WRONG_BEARING.0, &judged, None)
            .unwrap();
        let svc = incident::services(&landau);
        let ctx = incident::ctx(&db, "REFRESH");
        let outcome = crate::jobs::refresh::run(&ctx, &svc).await;
        ctx.finish(&outcome, "REPORT");
        outcome.unwrap();
        assert_eq!(landau.row(ROW_9D1E).unwrap().cost, Some(6_194_100));

        let a = run(&db, &landau, &incident::supplier()).await;
        let bearing = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9D1E)
            .expect("still listed");
        assert_eq!(bearing.doubts, [Doubt::UnprovenUnits]);
        assert_eq!((bearing.unproven_units, bearing.difference), (3, Some(0)));
    }

    /// PI-21 (Q-6): every row whose last cost stands on an automatic link is listed apart from the
    /// doubts — a consistent one too, since a link the gate passed can still be a wrong part when
    /// Landau lacks the true one. Here the mid-shaft bearing was written from an automatic link that
    /// proves (consistent, not a doubt), and the clamp row's last cost came from `AR1330-4`'s
    /// automatic link (a doubt as well). The four rows written from the seller's picks are not.
    #[tokio::test]
    async fn every_automatic_write_is_listed_apart_from_the_doubts() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, MID_SHAFT, "AUTO", &bearing_6c04());
        let a = run(&db, &landau, &incident::supplier()).await;

        let doubts: Vec<&str> = a.rows.iter().map(|r| r.stock_id.as_str()).collect();
        assert_eq!(doubts, [ROW_9CF0, ROW_9D1E], "the doubts are unchanged");
        assert_eq!(a.consistent, 4, "the mid-shaft row is consistent");

        let auto: Vec<(&str, bool)> = a
            .automatic
            .iter()
            .map(|r| (r.stock_id.as_str(), r.doubted))
            .collect();
        assert_eq!(auto, [(ROW_9CF0, true), (ROW_9D12, false)]);

        let shaft = &a.automatic[1];
        assert_eq!(shaft.cost, Some(5_247_000));
        assert_eq!(shaft.last_write.names, [MID_SHAFT.1.to_string()]);
        assert_eq!(shaft.links.len(), 1);
        let l = &shaft.links[0];
        assert_eq!(
            (l.name.as_str(), l.how.as_str(), l.proven),
            (MID_SHAFT.1, "AUTO", true)
        );
        assert_eq!(l.today, None, "an automatic write is never quoted");

        let clamp = &a.automatic[0];
        let names: Vec<&str> = clamp.links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, [CLAMP_4.1], "only the link its last cost came from");
        assert!(!clamp.links[0].proven);
    }

    /// Found by the slice-7 drive on the seller's data: v1.1.1 wrote at an unavailable price and
    /// recorded no cost before, so slice 5's provenance reads the write as one the hold may have
    /// kept and drops it. The automatic list still names it: an automatic link wrote on that row.
    #[tokio::test]
    async fn an_automatic_write_at_an_unavailable_price_is_still_listed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, MID_SHAFT, "AUTO", &bearing_6c04());
        db.with(|c| {
            c.execute(
                "UPDATE job_items SET available = 0, cost_before = NULL WHERE code = ?1",
                [MID_SHAFT.0],
            )
        })
        .unwrap();
        let a = run(&db, &landau, &incident::supplier()).await;
        let shaft = a
            .automatic
            .iter()
            .find(|r| r.stock_id == ROW_9D12)
            .unwrap_or_else(|| panic!("{:?}", a.automatic));
        assert_eq!(shaft.links.len(), 1);
        assert_eq!(shaft.links[0].name, MID_SHAFT.1);
        assert_eq!(shaft.last_write.names, [MID_SHAFT.1.to_string()]);
    }

    /// A seller's pick of the same product after an automatic write makes the cost the seller's:
    /// the row leaves the automatic list. A link moved to another product is a doubt instead.
    #[tokio::test]
    async fn a_later_pick_or_move_takes_a_row_off_the_automatic_list() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, MID_SHAFT, "AUTO", &bearing_6c04());
        incident::link(&db, MID_SHAFT, "USER", &bearing_6c04());
        let a = run(&db, &landau, &incident::supplier()).await;
        assert!(
            !a.automatic.iter().any(|r| r.stock_id == ROW_9D12),
            "{:?}",
            a.automatic
        );

        db.clear_match(SUPPLIER_ID, CLAMP_4.1).unwrap();
        let a = run(&db, &landau, &incident::supplier()).await;
        assert!(a.automatic.is_empty(), "{:?}", a.automatic);
        let clamp = a.rows.iter().find(|r| r.stock_id == ROW_9CF0).unwrap();
        assert!(clamp.doubts.contains(&Doubt::WriterUnlinked));
    }

    /// An automatic link a later match moved to another product, still `AUTO`, no longer stands
    /// behind the row it wrote: the row is a `WRITER_UNLINKED` doubt, never an automatic write of
    /// a link that now names another part.
    #[tokio::test]
    async fn an_automatic_link_moved_to_another_product_is_not_the_rows_automatic_write() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, CLAMP_4, "AUTO", &bearing_6c04());
        let a = run(&db, &landau, &incident::supplier()).await;
        assert!(
            !a.automatic.iter().any(|r| r.stock_id == ROW_9CF0),
            "{:?}",
            a.automatic
        );
        let clamp = a.rows.iter().find(|r| r.stock_id == ROW_9CF0).unwrap();
        assert!(
            clamp.doubts.contains(&Doubt::WriterUnlinked),
            "{:?}",
            clamp.doubts
        );
    }

    /// The CSV carries the automatic writes below the doubts, a sixteenth column naming each row's
    /// kind; nothing was asked for an automatic row, so its evidence cells are empty, never unknown.
    #[tokio::test]
    async fn the_csv_lists_automatic_writes_after_the_doubts() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, MID_SHAFT, "AUTO", &bearing_6c04());
        let a = run(&db, &landau, &incident::supplier()).await;
        let text = csv(&a);
        let lines: Vec<&str> = text.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 5, "a header, two doubts, two automatic writes");
        assert!(lines[0].ends_with(",نوع ردیف"), "{}", lines[0]);
        assert!(lines[1].ends_with(",تردید") && lines[2].ends_with(",تردید"));
        let shaft = lines[4];
        assert!(shaft.starts_with(ROW_9D12), "{shaft}");
        assert!(shaft.ends_with(",نوشتهٔ خودکار"), "{shaft}");
        assert!(shaft.contains(",5247000,,,,"), "no evidence asked: {shaft}");
        assert!(!shaft.contains(UNKNOWN), "{shaft}");
        assert!(
            lines[3].contains("در فهرست تردیدها هم آمده"),
            "{}",
            lines[3]
        );
        // and the doubt row says it too (the slice review's L6): both sides are marked
        assert!(lines[1].contains(ALSO_AUTOMATIC), "{}", lines[1]);
        assert!(!lines[2].contains(ALSO_AUTOMATIC), "{}", lines[2]);
    }

    /// Newest write first across jobs, the row id breaking a tie (the slice review's L8).
    #[test]
    fn automatic_writes_are_listed_newest_first() {
        let w = |stock: &str, job: i64| AutoWrite {
            stock_id: stock.into(),
            product_id: "p".into(),
            product_name: None,
            brand: None,
            authenticity: None,
            inventory: 0,
            cost: None,
            price: None,
            links: Vec::new(),
            last_write: LastWrite {
                job_id: job,
                kind: "IMPORT".into(),
                at: String::new(),
                names: Vec::new(),
                cost: 1,
            },
            doubted: false,
        };
        let mut v = vec![w("b", 12), w("c", 14), w("a", 12), w("d", 13)];
        sort_automatic(&mut v);
        let order: Vec<&str> = v.iter().map(|r| r.stock_id.as_str()).collect();
        assert_eq!(order, ["c", "d", "a", "b"]);
    }

    #[tokio::test]
    async fn the_csv_names_each_listed_row_in_rials_and_toman() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let a = run(&db, &landau, &incident::supplier()).await;
        let text = csv(&a);
        assert!(text.starts_with('\u{feff}'), "a BOM for spreadsheets");
        let lines: Vec<&str> = text.trim_end().split("\r\n").collect();
        assert_eq!(
            lines.len(),
            4,
            "a header, the two rows, and the clamp row's automatic write"
        );
        assert!(lines[0].contains("اختلاف (تومان)"));
        assert!(lines[1].starts_with(ROW_9CF0) && lines[1].contains(UNKNOWN));
        let bearing = lines[2];
        assert!(bearing.starts_with(ROW_9D1E));
        for figure in [",5500000,", ",6194100,", ",694100,", ",69410,"] {
            assert!(bearing.contains(figure), "{figure} in {bearing}");
        }
        assert!(bearing.contains("عدد ناسازگار"), "{bearing}");
    }

    /// The incident's prices, each `(item, list price)` in `extra` added or replacing its own.
    fn priced_with(extra: &[((&str, &str), i64)]) -> FakeSupplier {
        let mut items: Vec<(&str, &str, i64, i64)> = incident::PRICES
            .iter()
            .filter(|(item, _)| !extra.iter().any(|(e, _)| e.0 == item.0))
            .map(|((c, n), p)| (*c, *n, 1, *p))
            .collect();
        items.extend(extra.iter().map(|((c, n), p)| (*c, *n, 1, *p)));
        FakeSupplier::new(&items)
    }

    /// A REFRESH over the shelf with today's prices from `supplier`; its job id.
    async fn refresh_with(db: &Db, landau: &FakeLandau, supplier: FakeSupplier) -> i64 {
        let svc = crate::jobs::Services {
            supplier: Box::new(supplier),
            ..incident::services(landau)
        };
        let ctx = incident::ctx(db, "REFRESH");
        let outcome = crate::jobs::refresh::run(&ctx, &svc).await;
        ctx.finish(&outcome, "REPORT");
        outcome.unwrap();
        ctx.job_id
    }

    /// The seller's word on the rename recorded for `item`, as Matches sends it: the core's
    /// reasons, shown and confirmed.
    fn confirm_rename(db: &Db, item: (&str, &str), product: &str, site: &str) {
        let registry = Registry::from_products(incident::registry());
        let version = db
            .match_for_name(SUPPLIER_ID, item.1)
            .unwrap()
            .unwrap()
            .link_version;
        let confirm = |c: Option<&[Reason]>| {
            crate::jobs::import::confirm_rename(
                db, &registry, item.1, version, product, item.0, site, c,
            )
        };
        let shown = match confirm(None) {
            Err(crate::errors::BridgeError::ConfirmRequired { reasons }) => {
                serde_json::from_str::<Vec<Reason>>(&reasons).unwrap()
            }
            other => panic!("{other:?}"),
        };
        assert!(confirm(Some(&shown)).unwrap());
    }

    fn link_named<'a>(row: &'a AuditRow, name: &str) -> &'a AuditLink {
        row.links
            .iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", row.links))
    }

    /// PI-29: a code the site renamed into a conflict proves no cost in the audit, as every job
    /// holds it (PI-26). Two links pricing a row alike, one of them renamed — the clamp row's own
    /// code and a second code the site now sells as a 30 cm × 3.6 clamp — are not consistent: the
    /// row is listed for the renamed link, named so, and its cost never counts. The audit records
    /// nothing; a REFRESH on the same prices holds that row; and once a job has recorded the rename
    /// the audit says it waits for the seller in Matches.
    #[tokio::test]
    async fn a_code_renamed_into_a_conflict_proves_no_cost() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let other = ("AR1330-9", "بست کمري 370 7/6 تايوان آراد پارت");
        incident::link(&db, other, "USER", &clamp_57a5());
        let site = "بست کمري 300*3/6 تايوان آراد پارت";
        // the renamed item lists at the clamp's own price: 165,000 either way
        let prices = || priced_with(&[(other, 150_000)]).renamed(other.0, site);
        let before = fingerprint(&db);

        let a = run(&db, &landau, &prices()).await;
        let clamp = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9CEA)
            .expect("an equal renamed cost is no agreement");
        assert_eq!(clamp.doubts, [Doubt::UnprovenLink]);
        assert_eq!(
            (clamp.should_cost, clamp.difference),
            (Some(165_000), Some(0)),
            "«should» stands on the clamp's own code alone"
        );
        let renamed = link_named(clamp, other.1);
        assert!(!renamed.proven);
        assert!(
            renamed
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::NumberConflict { .. })),
            "{:?}",
            renamed.reasons
        );
        assert_eq!(
            renamed.rename.as_ref().map(|r| r.state),
            Some(RenameState::Unasked)
        );
        assert_eq!(
            renamed.today,
            Some(Read::Known(Some(Quoted {
                cost: 165_000,
                available: true,
                code: other.0.into(),
                name: site.into(),
                renamed: true,
            })))
        );
        let own = link_named(clamp, incident::CLAMP_5.1);
        assert!(own.proven && own.rename.is_none(), "{own:?}");
        assert_eq!(fingerprint(&db), before, "the audit records no rename");

        // a job on the same prices holds that row: the audit concludes as PRICE does
        let job = refresh_with(&db, &landau, prices()).await;
        let held = db
            .job_items(job)
            .unwrap()
            .into_iter()
            .find(|i| i.stock_id.as_deref() == Some(ROW_9CEA))
            .unwrap();
        assert_eq!(held.error_kind.as_deref(), Some("EvidenceConflict"));

        let recorded = fingerprint(&db);
        let a = run(&db, &landau, &prices()).await;
        assert_eq!(
            fingerprint(&db),
            recorded,
            "read-only with a record waiting"
        );
        let clamp = a.rows.iter().find(|r| r.stock_id == ROW_9CEA).unwrap();
        assert_eq!(
            link_named(clamp, other.1).rename.as_ref().map(|r| r.state),
            Some(RenameState::Waiting),
            "the job recorded it: Matches asks"
        );
        let text = csv(&a);
        let line = text.lines().find(|l| l.starts_with(ROW_9CEA)).unwrap();
        assert!(line.contains(site), "{line}");
        assert!(line.contains("منتظر تأیید شماست"), "{line}");

        // the site renames it again: Matches still asks about the name the job recorded (as its
        // `waitingRename` shows any unconfirmed record on the product), and the audit says so
        let again = "بست کمري 300*3/6 تايوان آراد پارت براق";
        let a = run(
            &db,
            &landau,
            &priced_with(&[(other, 150_000)]).renamed(other.0, again),
        )
        .await;
        assert_eq!(fingerprint(&db), recorded);
        let clamp = a.rows.iter().find(|r| r.stock_id == ROW_9CEA).unwrap();
        let l = link_named(clamp, other.1);
        let r = l.rename.as_ref().unwrap();
        assert_eq!(
            (r.state, r.name.as_str(), r.asked.as_deref()),
            (RenameState::AskedOlder, again, Some(site))
        );
        let text = csv(&a);
        let line = text.lines().find(|l| l.starts_with(ROW_9CEA)).unwrap();
        assert!(line.contains(&format!("نام پیشین «{site}»")), "{line}");
    }

    /// PI-29 with the seller's word: a row of one link, its code renamed into a conflict. Before
    /// any job meets the rename the audit does not look for it (the named residual: no quote is
    /// asked); once a job has recorded it, the link is quoted and holds the row; the seller's
    /// confirmation lets it prove, as PRICE writes it; a further rename asks again.
    #[tokio::test]
    async fn a_rename_on_record_is_weighed_and_honoured_only_as_confirmed() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let gsr = incident::CLAMP_GSR;
        let clamp = crate::matching::evidence::tests::clamp_57eb().id;
        let site = "بست کمري 300*3/6 GSR سفيد";
        let prices = |name: &str| priced_with(&[]).renamed(gsr.0, name);
        let asked = |s: &FakeSupplier| s.asked.lock().unwrap().clone();

        let s = prices(site);
        let a = run(&db, &landau, &s).await;
        assert!(!a.rows.iter().any(|r| r.stock_id == ROW_9CED));
        assert!(
            !asked(&s)
                .iter()
                .any(|q| q.contains(gsr.0) || q.contains(gsr.1)),
            "a row of one link with no rename on record is not quoted: {:?}",
            asked(&s)
        );

        refresh_with(&db, &landau, prices(site)).await;
        let recorded = fingerprint(&db);
        let a = run(&db, &landau, &prices(site)).await;
        assert_eq!(
            fingerprint(&db),
            recorded,
            "read-only with a record waiting"
        );
        let row = a
            .rows
            .iter()
            .find(|r| r.stock_id == ROW_9CED)
            .expect("a rename on record makes the quote decisive");
        assert_eq!(row.doubts, [Doubt::UnprovenLink]);
        assert_eq!((row.should_cost, row.difference), (None, None));
        let l = link_named(row, gsr.1);
        assert_eq!(
            (l.proven, l.rename.as_ref().map(|r| r.state)),
            (false, Some(RenameState::Waiting))
        );

        // the site lists the invoice's own name again: the quote writes, so the row is consistent;
        // a job would forget the waiting record, the audit leaves it where it is
        let a = run(&db, &landau, &priced_with(&[])).await;
        assert!(!a.rows.iter().any(|r| r.stock_id == ROW_9CED));
        assert_eq!(fingerprint(&db), recorded, "the record still waits");

        confirm_rename(&db, gsr, &clamp, site);
        let confirmed = fingerprint(&db);
        let s = prices(site);
        let a = run(&db, &landau, &s).await;
        assert!(
            !a.rows.iter().any(|r| r.stock_id == ROW_9CED),
            "confirmed for this name: proven, as PRICE writes it"
        );
        assert!(asked(&s).iter().any(|q| q == &format!("code:{}", gsr.0)));

        let again = "بست کمري 300*3/6 GSR سفيد براق";
        let a = run(&db, &landau, &prices(again)).await;
        assert_eq!(fingerprint(&db), confirmed);
        let row = a.rows.iter().find(|r| r.stock_id == ROW_9CED).unwrap();
        let l = link_named(row, gsr.1);
        assert_eq!(
            (l.proven, l.rename.as_ref().map(|r| r.state)),
            (false, Some(RenameState::Unasked)),
            "a confirmation counts for the name it was given for"
        );
    }

    /// PI-29, the slice review's finding 3: units a quantity import moved under a code since renamed
    /// stay judged on the link (the invoices named the part itself), and the link says so where it
    /// reads «اثبات‌نشده», so its units never look like the row's unproven ones.
    #[tokio::test]
    async fn units_under_a_renamed_code_stay_proven_and_say_why() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        let gsr = incident::CLAMP_GSR;
        let job = db
            .create_job(
                "IMPORT",
                Some("QUANTITY"),
                Some("1405/08/15"),
                incident::STORE,
                false,
            )
            .unwrap();
        let id = item_on(&db, job, gsr, ROW_9CED, &[("cost", 20_900)]);
        db.with(|c| c.execute("UPDATE job_items SET qty = 4 WHERE id = ?1", [id]))
            .unwrap();
        db.set_job_status(job, "DONE", "REPORT").unwrap();
        let site = "بست کمري 300*3/6 GSR سفيد";
        let prices = || priced_with(&[]).renamed(gsr.0, site);
        refresh_with(&db, &landau, prices()).await;

        let a = run(&db, &landau, &prices()).await;
        let row = a.rows.iter().find(|r| r.stock_id == ROW_9CED).unwrap();
        assert_eq!(row.doubts, [Doubt::UnprovenLink]);
        assert_eq!(row.unproven_units, 0);
        let l = link_named(row, gsr.1);
        assert_eq!((l.proven, l.units), (false, 4));
        let text = csv(&a);
        let line = text.lines().find(|l| l.starts_with(ROW_9CED)).unwrap();
        assert!(line.contains(UNITS_UNDER_OWN_NAME), "{line}");
    }

    /// PI-29: a row listed for another doubt never reads a «should» from a renamed quote no job
    /// would write. The seller cleared the hurried pick, so the bearing row is listed for its
    /// writer; the right bearing's code is now sold as the 69349 bearing at 5,500,000.
    #[tokio::test]
    async fn a_renamed_quote_never_names_should() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        db.clear_match(SUPPLIER_ID, WRONG_BEARING.1).unwrap();
        let site = "بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54)";
        let supplier = priced_with(&[(RIGHT_BEARING, 5_000_000)]).renamed(RIGHT_BEARING.0, site);
        let a = run(&db, &landau, &supplier).await;
        let bearing = a.rows.iter().find(|r| r.stock_id == ROW_9D1E).unwrap();
        assert_eq!(bearing.doubts, [Doubt::UnprovenLink, Doubt::WriterUnlinked]);
        assert_eq!((bearing.should_cost, bearing.difference), (None, None));
        assert!(!link_named(bearing, RIGHT_BEARING.1).proven);
    }

    /// PI-29: a row in both lists shows its automatic link with the verdict its quote got among
    /// the doubts; the automatic list itself still asks nothing.
    #[tokio::test]
    async fn an_automatic_write_shows_the_rename_its_row_was_weighed_with() {
        let db = Db::in_memory().unwrap();
        let landau = after_job_12(&db);
        incident::link(&db, MID_SHAFT, "AUTO", &bearing_6c04());
        let site = "بلبرينگ وسط 6005 پلوس پژو ELMA7-405 (10)";
        let prices = || priced_with(&[]).renamed(MID_SHAFT.0, site);
        refresh_with(&db, &landau, prices()).await;
        let a = run(&db, &landau, &prices()).await;
        assert!(
            a.rows.iter().any(|r| r.stock_id == ROW_9D12),
            "{:?}",
            a.rows
        );
        let shaft = a.automatic.iter().find(|r| r.stock_id == ROW_9D12).unwrap();
        assert!(shaft.doubted);
        let l = &shaft.links[0];
        assert_eq!(
            (l.proven, l.rename.as_ref().map(|r| r.state)),
            (false, Some(RenameState::Waiting))
        );
        assert_eq!(
            l.rename.as_ref().unwrap().name,
            site,
            "named, though not quoted here"
        );
        assert!(!l.reasons.is_empty());
        assert_eq!(l.today, None, "an automatic write is never quoted");
    }

    #[test]
    fn a_field_with_a_comma_quote_or_line_break_is_quoted() {
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("a \"b\""), "\"a \"\"b\"\"\"");
        assert_eq!(field("a\nb"), "\"a\nb\"");
        assert_eq!(field("ساده"), "ساده");
        assert_eq!(toman(694_100), "69410");
        assert_eq!(text("=HYPERLINK(1)"), "'=HYPERLINK(1)", "never a formula");
        assert_eq!(text("بست -10"), "بست -10");
        assert_eq!(toman(-15), "-1.5");
    }
}
