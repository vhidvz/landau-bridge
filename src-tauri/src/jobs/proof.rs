//! What makes a link writable (plan 130 tasks 3, 4, 7): VALIDATE against the job's registry
//! snapshot, the rebuild of a dead product's link (Q-1), the write table over every stored and new
//! link (Q-2, Q-5), and the collision check over a job's target rows (Q-3, R-e). The collision
//! check is the only constructor of [`ProvenWrite`], and the shelf writer
//! ([`crate::landau::writer`]) takes nothing else — so no path reaches a shelf without all of it.

use std::collections::{BTreeMap, HashSet};

use crate::db::matches::{actor, state, Expect, LinkProduct, LinkPut, MatchRow};
use crate::db::Db;
use crate::domain::Authenticity;
use crate::errors::{BridgeError, Result};
use crate::landau::Candidate;
use crate::matching::evidence::evidence;
use crate::matching::gate::{
    honour_confirmed, judge_link, judge_top, verdict, Judged, Origin, Reason, Verdict,
};
use crate::matching::normalize::{name_key, search_term};
use crate::matching::registry::{identity_of, Registry};
use crate::matching::{similarity_of, Scored, CANDIDATES};

/// What one supplier item's link came to in this job.
#[derive(Debug, Clone, PartialEq)]
pub enum Judgement {
    /// The link writes for this job, at this version.
    Proven {
        key: String,
        version: i64,
        product: Candidate,
    },
    /// The seller's stored «not in Landau».
    NotInLandau,
    /// The seller decides. `error` is the identity failure when a stored link was refused (none for
    /// an item that simply had no link that passed); `link_product` is the product a stored link
    /// still points at, which no other link may write to meanwhile (R-e).
    Held {
        reasons: Vec<Reason>,
        error: Option<BridgeError>,
        candidates: Vec<Scored>,
        link_key: Option<String>,
        link_product: Option<String>,
    },
}

/// The identity failure a refusal stands for: a conflict when the evidence contradicts the link,
/// otherwise not enough to write on (R-h).
pub fn held_error(reasons: &[Reason]) -> BridgeError {
    let json = serde_json::to_string(reasons).unwrap_or_default();
    let conflict = reasons.iter().any(|r| {
        matches!(
            r,
            Reason::NumberConflict { .. }
                | Reason::OppositeWords { .. }
                | Reason::BrandMismatch { .. }
        )
    });
    if conflict {
        BridgeError::EvidenceConflict { reasons: json }
    } else {
        BridgeError::NeedsReview { reasons: json }
    }
}

/// A product's definition as a link records it: absent brand and alias recorded as `""` (none).
fn recorded(c: &Candidate) -> LinkProduct<'_> {
    LinkProduct {
        id: &c.id,
        name: &c.name,
        brand: Some(c.brand.as_deref().unwrap_or("")),
        alias: Some(c.alias.as_deref().unwrap_or("")),
    }
}

/// VALIDATE and the write table for one job, over that job's registry snapshot.
pub struct Prover<'a> {
    pub db: &'a Db,
    pub registry: &'a Registry,
    pub supplier: &'a str,
    pub job: i64,
    /// The seller's setting; the gate never reads it below its floor.
    pub threshold: u8,
}

impl Prover<'_> {
    /// The candidates the picker would gather for `term`, from the snapshot, best first.
    async fn gather(&self, term: &str) -> Result<Vec<Scored>> {
        crate::matching::gather(self.registry, term).await
    }

    /// The stored link for this name, else for this code (spec §14 a). A name hit seen under a new
    /// code takes the code as an attribute; a code hit under another spelling is judged as it is.
    pub fn stored_link(&self, name: &str, code: &str) -> Result<Option<MatchRow>> {
        if let Some(row) = self.db.match_for_name(self.supplier, name)? {
            if !code.is_empty() && row.code != code {
                self.db.set_link_code(self.supplier, &row.name_key, code)?;
            }
            return Ok(Some(row));
        }
        self.db.match_for_code(self.supplier, code)
    }

    /// One supplier item of an import: its stored link judged, or a fresh automatic match.
    pub async fn item(&self, name: &str, code: &str) -> Result<Judgement> {
        match self.stored_link(name, code)? {
            Some(link) => self.stored(&link, name, code).await,
            None => self.fresh(name, code, None).await,
        }
    }

    /// The judged evidence of one link to `c`: a seller's pick by its conflicts alone (Q-5), an
    /// automatic one through the whole gate, its tie and other passing candidates included (Q-2).
    async fn judge(
        &self,
        c: &Candidate,
        user: bool,
        name: &str,
        code: &str,
    ) -> Result<(Judged, Option<Vec<Scored>>)> {
        let term = search_term(name, code);
        let similarity = similarity_of(&term, c);
        if user {
            // a pick is judged by its own evidence; the candidates are gathered only for review
            let judged = Judged {
                similarity,
                evidence: evidence(name, code, c, self.registry.vocabulary()),
                tied: false,
                another_passes: None,
            };
            return Ok((judged, None));
        }
        let scored = self.gather(&term).await?;
        let judged = judge_link(
            name,
            code,
            c,
            similarity,
            &scored,
            self.registry.vocabulary(),
            self.threshold,
        );
        Ok((judged, Some(scored)))
    }

    /// The review list for a link to `c`: the candidates already gathered, else gathered now.
    async fn review_list(
        &self,
        scored: Option<Vec<Scored>>,
        c: &Candidate,
        similarity: u8,
        name: &str,
        code: &str,
    ) -> Result<Vec<Scored>> {
        let scored = match scored {
            Some(s) => s,
            None => self.gather(&search_term(name, code)).await?,
        };
        Ok(first(scored, c, similarity))
    }

    /// A stored link (R-b): its product looked up in the snapshot; a dead one rebuilt only on an
    /// exact name + brand + alias (Q-1); then the write table for its origin.
    pub async fn stored(&self, link: &MatchRow, name: &str, code: &str) -> Result<Judgement> {
        let Some(pid) = link.product_id.as_deref() else {
            return Ok(Judgement::NotInLandau);
        };
        let key = link.name_key.clone();
        let user = link.how == "USER";
        if let Some(c) = self.registry.get(pid) {
            let c = c.clone();
            let (judged, scored) = self.judge(&c, user, name, code).await?;
            let origin = if user { Origin::Pick } else { Origin::Auto };
            return match judged_for(link, name, origin, &judged, self.threshold) {
                Verdict::Write => {
                    let Some(version) = self.db.validate_link(
                        self.supplier,
                        &key,
                        link.link_version,
                        recorded(&c),
                        state::VALID,
                        None,
                        self.job,
                    )?
                    else {
                        return Ok(changed(link));
                    };
                    Ok(Judgement::Proven {
                        key,
                        version,
                        product: c,
                    })
                }
                Verdict::Review(r) | Verdict::ConfirmThenWrite(r) => {
                    let saved = self.db.validate_link(
                        self.supplier,
                        &key,
                        link.link_version,
                        recorded(&c),
                        state::NEEDS_REVIEW,
                        Some(serde_json::to_string(&r)?),
                        self.job,
                    )?;
                    if saved.is_none() {
                        return Ok(changed(link));
                    }
                    Ok(Judgement::Held {
                        error: Some(held_error(&r)),
                        candidates: self
                            .review_list(scored, &c, judged.similarity, name, code)
                            .await?,
                        reasons: r,
                        link_key: Some(key),
                        link_product: Some(c.id),
                    })
                }
            };
        }

        // The product is gone (a catalogue re-import gives every product a new id).
        let identity = match (&link.product_name, &link.product_brand, &link.product_alias) {
            (Some(n), Some(b), Some(a)) => Some(identity_of(n, Some(b), Some(a))),
            _ => None,
        };
        let Some(identity) = identity else {
            if !user {
                // An automatic link holds no human judgment: matched afresh under the gate (Q-1).
                return self.fresh(name, code, Some(link)).await;
            }
            return self
                .stale(
                    link,
                    name,
                    code,
                    "a seller's pick with no recorded brand and alias",
                )
                .await;
        };
        let hits = self.registry.with_identity(&identity);
        let [c] = hits.as_slice() else {
            return self
                .stale(
                    link,
                    name,
                    code,
                    &format!("{} products carry its name, brand and alias", hits.len()),
                )
                .await;
        };
        let c = (*c).clone();
        let (judged, scored) = self.judge(&c, user, name, code).await?;
        let origin = if user {
            Origin::RebuiltPick {
                identity_exact: true,
            }
        } else {
            Origin::RebuiltAuto {
                identity_exact: true,
            }
        };
        match judged_for(link, name, origin, &judged, self.threshold) {
            Verdict::Write => {
                let Some(version) = self.db.rebuild_link(
                    self.supplier,
                    &key,
                    link.link_version,
                    pid,
                    recorded(&c),
                    state::REBUILT,
                    None,
                    self.job,
                )?
                else {
                    return Ok(changed(link));
                };
                Ok(Judgement::Proven {
                    key,
                    version,
                    product: c,
                })
            }
            Verdict::Review(r) | Verdict::ConfirmThenWrite(r) => {
                let saved = self.db.rebuild_link(
                    self.supplier,
                    &key,
                    link.link_version,
                    pid,
                    recorded(&c),
                    state::NEEDS_REVIEW,
                    Some(serde_json::to_string(&r)?),
                    self.job,
                )?;
                if saved.is_none() {
                    return Ok(changed(link));
                }
                Ok(Judgement::Held {
                    error: Some(held_error(&r)),
                    candidates: self
                        .review_list(scored, &c, judged.similarity, name, code)
                        .await?,
                    reasons: r,
                    link_key: Some(key),
                    link_product: Some(c.id),
                })
            }
        }
    }

    /// A dead link nothing could rebuild: `STALE`, and the seller picks again.
    async fn stale(&self, link: &MatchRow, name: &str, code: &str, why: &str) -> Result<Judgement> {
        let reasons = vec![Reason::DefinitionNotExact];
        let marked = self.db.set_link_state_if(
            self.supplier,
            &link.name_key,
            link.link_version,
            link.product_id.as_deref(),
            state::STALE,
            Some(serde_json::json!({"why": why, "reasons": reasons}).to_string()),
            Some(self.job),
            actor::VALIDATE,
        )?;
        if !marked {
            return Ok(changed(link));
        }
        let scored = self.gather(&search_term(name, code)).await?;
        Ok(Judgement::Held {
            reasons,
            error: Some(BridgeError::StaleLink {
                product: link.product_id.clone().unwrap_or_default(),
            }),
            candidates: scored.into_iter().take(CANDIDATES).collect(),
            link_key: Some(link.name_key.clone()),
            link_product: None,
        })
    }

    /// A fresh automatic match: the top candidate writes only through the V4 gate (Q-2). With
    /// `replacing`, it takes the place of that dead automatic link.
    async fn fresh(
        &self,
        name: &str,
        code: &str,
        replacing: Option<&MatchRow>,
    ) -> Result<Judgement> {
        let term = search_term(name, code);
        let scored = self.gather(&term).await?;
        let judged = judge_top(
            name,
            code,
            &scored,
            self.registry.vocabulary(),
            self.threshold,
        );
        let outcome = judged.map(|j| (verdict(Origin::Auto, &j, self.threshold), j));
        if let Some((Verdict::Write, j)) = &outcome {
            let top = scored[0].candidate.clone();
            let name_seen = replacing.map(|l| l.name_seen.as_str()).unwrap_or(name);
            // never over a decision the seller made since the read: the dead link as it was read,
            // or no link at all for a name nobody decided
            let expect = match replacing {
                Some(old) => Expect::At {
                    version: old.link_version,
                    product: old.product_id.as_deref(),
                },
                None => Expect::Absent,
            };
            let Some(version) = self.db.put_link_if(
                self.supplier,
                &LinkPut {
                    name_seen,
                    code,
                    product: Some(recorded(&top)),
                    how: "AUTO",
                    similarity: Some(j.similarity as i64),
                    state: state::VALID,
                    state_reason: None,
                    validated_job: Some(self.job),
                    actor: actor::AUTO,
                    evidence: None,
                    job_id: Some(self.job),
                    decided_at: None,
                    priced_at: None,
                    confirmed: None,
                },
                expect,
            )?
            else {
                return Ok(Judgement::Held {
                    reasons: Vec::new(),
                    error: Some(BridgeError::LinkChanged),
                    candidates: Vec::new(),
                    link_key: Some(name_key(name_seen)),
                    link_product: replacing.and_then(|l| l.product_id.clone()),
                });
            };
            return Ok(Judgement::Proven {
                key: name_key(name_seen),
                version,
                product: top,
            });
        }
        let reasons = match outcome {
            Some((Verdict::Review(r) | Verdict::ConfirmThenWrite(r), _)) => r,
            _ => Vec::new(),
        };
        let error = match replacing {
            Some(old) => {
                let marked = self.db.set_link_state_if(
                    self.supplier,
                    &old.name_key,
                    old.link_version,
                    old.product_id.as_deref(),
                    state::STALE,
                    Some(serde_json::json!({"why": "its product is gone and no fresh match passed", "reasons": reasons}).to_string()),
                    Some(self.job),
                    actor::VALIDATE,
                )?;
                if !marked {
                    return Ok(changed(old));
                }
                Some(BridgeError::StaleLink {
                    product: old.product_id.clone().unwrap_or_default(),
                })
            }
            None => None,
        };
        Ok(Judgement::Held {
            reasons,
            error,
            candidates: scored.into_iter().take(CANDIDATES).collect(),
            link_key: replacing.map(|l| l.name_key.clone()),
            link_product: None,
        })
    }
}

/// The write table's verdict on a stored link for the supplier item `name`, a seller's pick
/// honouring what the seller confirmed with it (R-d, slice 4) — only for the name it was confirmed
/// for. A link reached by its code under another name (the site gave the code to another item)
/// is judged without it: a reason equal in kind and value can come from a different item.
fn judged_for(
    link: &MatchRow,
    name: &str,
    origin: Origin,
    judged: &Judged,
    threshold: u8,
) -> Verdict {
    let v = verdict(origin, judged, threshold);
    if link.how != "USER" || name_key(name) != link.name_key {
        return v;
    }
    let confirmed: Vec<Reason> = link
        .confirmed_json
        .as_deref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default();
    honour_confirmed(v, &confirmed)
}

/// A link the seller changed between this job's read and its verdict: nothing is stored, and the
/// item waits — its old product held against every other link on it (R-e).
fn changed(link: &MatchRow) -> Judgement {
    Judgement::Held {
        reasons: Vec::new(),
        error: Some(BridgeError::LinkChanged),
        candidates: Vec::new(),
        link_key: Some(link.name_key.clone()),
        link_product: link.product_id.clone(),
    }
}

/// The review list with the link's own product first, so the seller confirms it in one click.
fn first(scored: Vec<Scored>, c: &Candidate, similarity: u8) -> Vec<Scored> {
    let mut out = vec![Scored {
        candidate: c.clone(),
        similarity,
    }];
    out.extend(scored.into_iter().filter(|s| s.candidate.id != c.id));
    out.truncate(CANDIDATES);
    out
}

// ------------------------------------------------------------ collisions and the proven write

/// Constructible only here: a [`ProvenWrite`] cannot be made anywhere else.
#[derive(Debug, Clone, PartialEq)]
struct Seal;

/// The row a proven write lands on.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// IMPORT: the row for `(product, authenticity)` in the job's store, found or attached.
    Row {
        product: String,
        authenticity: Authenticity,
        qty: i64,
    },
    /// REFRESH: an existing row's cost.
    Stock { stock_id: String, product: String },
}

/// A cost (and, on import, a quantity) whose every link this job proved and no other link on the
/// same row contradicts (R-l). Only [`plan_import`] and [`prove_refresh`] make one.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvenWrite {
    job: i64,
    items: Vec<i64>,
    target: Target,
    cost: i64,
    links: Vec<(String, i64)>,
    _seal: Seal,
}

impl ProvenWrite {
    pub fn job(&self) -> i64 {
        self.job
    }
    /// Every job item this write stands for, the one carrying the report first.
    pub fn items(&self) -> &[i64] {
        &self.items
    }
    pub fn lead(&self) -> i64 {
        self.items[0]
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    /// The supplier's cost, before the unavailable-only-raises hold, which needs the row.
    pub fn cost(&self) -> i64 {
        self.cost
    }
    /// `(link key, version)` each link was proven at; the writer re-reads them before writing.
    pub fn links(&self) -> &[(String, i64)] {
        &self.links
    }
    /// The product every link must still point at.
    pub fn product(&self) -> &str {
        match &self.target {
            Target::Row { product, .. } | Target::Stock { product, .. } => product,
        }
    }
}

/// One import item ready to write: proven at MATCH (or by the seller's pick), priced.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidate {
    pub item_id: i64,
    pub product: String,
    pub authenticity: Authenticity,
    pub cost: i64,
    pub qty: i64,
    pub link_key: String,
    pub link_version: i64,
}

/// Another stored link on a product the job writes, carried by none of the job's items (PI-22):
/// weighed as REFRESH weighs a row's links. `cost` is today's, `None` when the site lists none or
/// the link was not proven (then its version is the one read, and it holds the row).
#[derive(Debug, Clone, PartialEq)]
pub struct OtherLink {
    pub key: String,
    pub version: i64,
    pub cost: Option<i64>,
}

/// A write, or the items held and why.
#[derive(Debug, Clone, PartialEq)]
pub enum Planned {
    Write(ProvenWrite),
    Held { items: Vec<i64>, error: BridgeError },
}

/// Is the link still what this job proved, onto this product?
fn still_proven(
    db: &Db,
    supplier: &str,
    job: i64,
    key: &str,
    version: i64,
    product: &str,
) -> Result<bool> {
    Ok(db
        .link_by_key(supplier, key)?
        .is_some_and(|l| l.writable_for(job, version, product)))
}

/// The job's writes over its whole set of items, before anything is written (R-e). Per target row
/// `(product, authenticity)`: a link changed since it was proven holds its item (`LinkChanged`); a
/// product another link of this job still waits on, or several links with different costs, hold
/// the row (Q-3); equal costs are one write carrying the summed quantity, led by the lowest item.
/// `others` are the product's stored links no item of this job carries (PI-22): each must be proven
/// by this job and priced at the same cost, or the row is held, so a row's cost never follows
/// whichever of its codes was invoiced last. The answer is the same in whatever order the items come.
pub fn plan_import(
    db: &Db,
    supplier: &str,
    job: i64,
    items: &[ImportCandidate],
    unresolved: &HashSet<String>,
    others: &BTreeMap<String, Vec<OtherLink>>,
) -> Result<Vec<Planned>> {
    let mut out = Vec::new();
    let mut groups: BTreeMap<(String, &'static str), Vec<&ImportCandidate>> = BTreeMap::new();
    let mut sorted: Vec<&ImportCandidate> = items.iter().collect();
    sorted.sort_by_key(|c| c.item_id);
    // an item that drops out here still points at its product: no other link writes there in its
    // place (slice 2's review — the row would otherwise take the other cost alone)
    let mut unresolved = unresolved.clone();
    for c in sorted {
        if !still_proven(db, supplier, job, &c.link_key, c.link_version, &c.product)? {
            out.push(Planned::Held {
                items: vec![c.item_id],
                error: BridgeError::LinkChanged,
            });
            unresolved.insert(c.product.clone());
            continue;
        }
        groups
            .entry((c.product.clone(), c.authenticity.as_str()))
            .or_default()
            .push(c);
    }
    for ((product, _), group) in groups {
        let ids: Vec<i64> = group.iter().map(|c| c.item_id).collect();
        let extra = others.get(&product).map(Vec::as_slice).unwrap_or(&[]);
        let mut costs: HashSet<Option<i64>> = group.iter().map(|c| Some(c.cost)).collect();
        costs.extend(extra.iter().map(|o| o.cost));
        let links: HashSet<&str> = group.iter().map(|c| c.link_key.as_str()).collect();
        let mut extra_unproven = false;
        for o in extra {
            extra_unproven |= !still_proven(db, supplier, job, &o.key, o.version, &product)?;
        }
        if unresolved.contains(&product) {
            out.push(Planned::Held {
                items: ids,
                error: BridgeError::LinkConflict {
                    links: links.len() + extra.len() + 1,
                },
            });
        } else if extra_unproven || costs.len() > 1 {
            out.push(Planned::Held {
                items: ids,
                error: BridgeError::LinkConflict {
                    links: links.len() + extra.len(),
                },
            });
        } else {
            let mut proven: Vec<(String, i64)> = group
                .iter()
                .map(|c| (c.link_key.clone(), c.link_version))
                .chain(extra.iter().map(|o| (o.key.clone(), o.version)))
                .collect();
            proven.sort();
            proven.dedup();
            out.push(Planned::Write(ProvenWrite {
                job,
                items: ids,
                target: Target::Row {
                    product,
                    authenticity: group[0].authenticity,
                    qty: group.iter().map(|c| c.qty).sum(),
                },
                cost: group[0].cost,
                links: proven,
                _seal: Seal,
            }));
        }
    }
    out.sort_by_key(|p| match p {
        Planned::Write(w) => w.items[0],
        Planned::Held { items, .. } => items[0],
    });
    Ok(out)
}

/// A refresh row's cost from every link on its product (R-e): each must be proven by this job and
/// priced, and all must agree; otherwise the row is held.
pub fn prove_refresh(
    db: &Db,
    supplier: &str,
    job: i64,
    item_id: i64,
    stock_id: &str,
    product: &str,
    links: &[(MatchRow, Option<i64>)],
) -> Result<Planned> {
    let held = |error| {
        Ok(Planned::Held {
            items: vec![item_id],
            error,
        })
    };
    if links.is_empty() {
        return held(BridgeError::NeedsReview {
            reasons: "[]".into(),
        });
    }
    for (link, _) in links {
        if !still_proven(
            db,
            supplier,
            job,
            &link.name_key,
            link.link_version,
            product,
        )? {
            return held(BridgeError::LinkChanged);
        }
    }
    let costs: HashSet<Option<i64>> = links.iter().map(|(_, c)| *c).collect();
    let cost = match costs.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(cost)] => *cost,
        _ => return held(BridgeError::LinkConflict { links: links.len() }),
    };
    Ok(Planned::Write(ProvenWrite {
        job,
        items: vec![item_id],
        target: Target::Stock {
            stock_id: stock_id.into(),
            product: product.into(),
        },
        cost,
        links: links
            .iter()
            .map(|(l, _)| (l.name_key.clone(), l.link_version))
            .collect(),
        _seal: Seal,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::evidence::tests::{
        bearing_5e12, bearing_6c04, cand, clamp_576d, clamp_57a5, clamp_57ac, clamp_57eb, CLAMP_11,
        CLAMP_4, MID_SHAFT, RIGHT_BEARING, WRONG_BEARING,
    };

    const SUP: &str = "barezpakhsh";

    /// The 69349 bearing the hurried pick should have been, as the catalogue lists it.
    fn bearing_69349() -> Candidate {
        cand(
            "6aec987cd00e9931421a5e00",
            "بلبرینگ چرخ جلو قدیم پراید",
            Some("بلبرینگ 69349"),
            Some("عظام"),
        )
    }

    fn dpi_pad() -> Candidate {
        cand("dpi", "لنت ترمز جلو پراید", None, Some("DPI"))
    }

    /// Landau's registry after the 2026-11-05 re-import: the incident's products under their new
    /// ids, their siblings, and enough names to give the evidence its vocabulary.
    fn registry() -> Registry {
        let mut products = vec![
            bearing_5e12(),
            bearing_69349(),
            clamp_576d(),
            clamp_57a5(),
            clamp_57ac(),
            clamp_57eb(),
            bearing_6c04(),
            dpi_pad(),
            cand("azam-pad", "لنت ترمز جلو پراید", None, Some("عظام")),
            cand("ns", "شمع موتور", None, Some("بوش")),
        ];
        for (n, w) in [
            "بوش طبق پژو 405",
            "بست تایوان",
            "پیچ تایوان",
            "مهره تایوان",
            "بلبرینگ چرخ عقب پراید",
            "بلبرینگ چرخ جلو پیکان",
            "پلوس پژو 405",
            "بلبرینگ وسط پلوس پراید",
            "بست کمری نایلونی سفید",
            "پمپ بنزین پراید",
        ]
        .iter()
        .enumerate()
        {
            products.push(cand(&format!("f{n}"), w, None, None));
        }
        Registry::from_products(products)
    }

    fn db_with_job() -> (Db, i64) {
        let db = Db::in_memory().unwrap();
        let job = db
            .create_job("IMPORT", Some("ZERO"), None, "store", false)
            .unwrap();
        (db, job)
    }

    /// A stored link as v1.1.1 left it, migrated: `UNVERIFIED`, with its identity when recorded.
    fn stored(db: &Db, item: (&str, &str), how: &str, c: &Candidate, recorded: bool) {
        db.put_link(
            SUP,
            &LinkPut {
                name_seen: item.1,
                code: item.0,
                product: Some(LinkProduct {
                    id: &c.id,
                    name: &c.name,
                    brand: recorded.then(|| c.brand.as_deref().unwrap_or("")),
                    alias: recorded.then(|| c.alias.as_deref().unwrap_or("")),
                }),
                how,
                similarity: Some(56),
                state: state::UNVERIFIED,
                state_reason: None,
                validated_job: None,
                actor: actor::IMPORT,
                evidence: None,
                job_id: None,
                decided_at: None,
                priced_at: None,
                confirmed: None,
            },
        )
        .unwrap();
    }

    /// The same definition under the id it had before the re-import.
    fn dead(c: Candidate) -> Candidate {
        Candidate {
            id: format!("dead-{}", c.id),
            ..c
        }
    }

    fn prover<'a>(db: &'a Db, registry: &'a Registry, job: i64) -> Prover<'a> {
        Prover {
            db,
            registry,
            supplier: SUP,
            job,
            threshold: 70, // this install's setting; the gate reads it as 85
        }
    }

    fn reasons(j: &Judgement) -> Vec<Reason> {
        match j {
            Judgement::Held { reasons, .. } => reasons.clone(),
            other => panic!("expected held, got {other:?}"),
        }
    }

    /// Task 3: every id changed. A pick whose definition is still listed exactly once is rebuilt
    /// onto it — the old id kept, the trail saying reconstructed — and writes for this job only.
    #[tokio::test]
    async fn every_id_changed_and_an_exact_definition_rebuilds_the_pick() {
        let (db, job) = db_with_job();
        let r = registry();
        stored(&db, RIGHT_BEARING, "USER", &dead(bearing_5e12()), true);
        let j = prover(&db, &r, job)
            .item(RIGHT_BEARING.1, RIGHT_BEARING.0)
            .await
            .unwrap();
        let Judgement::Proven {
            key,
            version,
            product,
        } = j
        else {
            panic!("expected proven, got {j:?}");
        };
        assert_eq!(product.id, bearing_5e12().id);
        let link = db.link_by_key(SUP, &key).unwrap().unwrap();
        assert_eq!(link.state, state::REBUILT);
        assert_eq!(link.prev_product_id, Some(dead(bearing_5e12()).id));
        assert!(link.writable_for(job, version, &bearing_5e12().id));
        assert!(!link.writable_for(job + 1, version, &bearing_5e12().id));
        let last = db.match_events(SUP, &key).unwrap().pop().unwrap();
        assert_eq!(
            (last.actor.as_str(), last.evidence.as_deref()),
            (actor::REBUILD, Some("reconstructed"))
        );
    }

    /// Task 4: no hit, two hits, an alias changed, no recorded identity — each a stale pick for
    /// the seller, never a guess; nothing writes.
    #[tokio::test]
    async fn a_pick_is_rebuilt_only_on_exactly_one_identical_definition() {
        let r = registry();
        let cases: Vec<(&str, Candidate, bool)> = vec![
            (
                "no hit",
                Candidate {
                    name: "بلبرینگ ناموجود".into(),
                    ..dead(bearing_5e12())
                },
                true,
            ),
            (
                "alias changed",
                Candidate {
                    alias: Some("بلبرینگ 44649".into()),
                    ..dead(bearing_5e12())
                },
                true,
            ),
            ("brand unknown", dead(bearing_5e12()), false),
            (
                "two hits",
                Candidate {
                    brand: None,
                    alias: None,
                    ..dead(cand("x", "بست تایوان", None, None))
                },
                true,
            ),
        ];
        let twin = Registry::from_products(
            [
                registry_products(),
                vec![cand("f-twin", "بست تایوان", None, None)],
            ]
            .concat(),
        );
        for (case, c, recorded) in cases {
            let (db, job) = db_with_job();
            stored(&db, RIGHT_BEARING, "USER", &c, recorded);
            let reg = if case == "two hits" { &twin } else { &r };
            let j = prover(&db, reg, job)
                .item(RIGHT_BEARING.1, RIGHT_BEARING.0)
                .await
                .unwrap();
            let Judgement::Held {
                error,
                link_product,
                ..
            } = &j
            else {
                panic!("{case}: expected held, got {j:?}");
            };
            assert!(
                matches!(error, Some(BridgeError::StaleLink { .. })),
                "{case}: {error:?}"
            );
            assert_eq!(link_product, &None, "{case}");
            let link = db.match_for_name(SUP, RIGHT_BEARING.1).unwrap().unwrap();
            assert_eq!(link.state, state::STALE, "{case}");
            assert_eq!(
                link.product_id,
                Some(c.id.clone()),
                "{case}: the dead id is kept"
            );
        }
    }

    fn registry_products() -> Vec<Candidate> {
        let r = registry();
        r.search("", 100, 0).items
    }

    /// Q-1: an automatic link to a dead product with no recorded brand holds no human judgment —
    /// it is matched afresh under the gate, which here writes the one product that passes.
    #[tokio::test]
    async fn a_brandless_dead_automatic_link_is_matched_afresh_under_the_gate() {
        let (db, job) = db_with_job();
        let r = registry();
        stored(&db, MID_SHAFT, "AUTO", &dead(bearing_6c04()), false);
        let j = prover(&db, &r, job)
            .item(MID_SHAFT.1, MID_SHAFT.0)
            .await
            .unwrap();
        let Judgement::Proven { product, .. } = j else {
            panic!("expected proven, got {j:?}");
        };
        assert_eq!(product.id, bearing_6c04().id);
        let link = db.match_for_name(SUP, MID_SHAFT.1).unwrap().unwrap();
        assert_eq!(
            (link.how.as_str(), link.state.as_str()),
            ("AUTO", state::VALID)
        );
        assert_eq!(link.product_brand.as_deref(), Some("الما۷"), "recorded now");
    }

    /// Q-5: an old pick writes unless a dimension conflicts — the hurried 69349 pick onto the
    /// 44649 bearing and the DPI pad picked for an عظام name both go back to the seller.
    #[tokio::test]
    async fn an_old_pick_with_a_conflict_goes_to_review() {
        let r = registry();
        let (db, job) = db_with_job();
        stored(&db, WRONG_BEARING, "USER", &bearing_5e12(), true);
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert!(reasons(&j)
            .iter()
            .any(|r| matches!(r, Reason::NumberConflict { .. })));
        let Judgement::Held {
            error,
            link_product,
            candidates,
            ..
        } = &j
        else {
            unreachable!()
        };
        assert!(matches!(error, Some(BridgeError::EvidenceConflict { .. })));
        assert_eq!(link_product.as_deref(), Some(bearing_5e12().id.as_str()));
        assert_eq!(
            candidates[0].candidate.id,
            bearing_5e12().id,
            "its own product first"
        );
        assert_eq!(
            db.match_for_name(SUP, WRONG_BEARING.1)
                .unwrap()
                .unwrap()
                .state,
            state::NEEDS_REVIEW
        );

        let (db, job) = db_with_job();
        let item = ("31370061", "لنت جلو پرايد عظام (20)");
        stored(&db, item, "USER", &dpi_pad(), true);
        let j = prover(&db, &r, job).item(item.1, item.0).await.unwrap();
        assert!(
            reasons(&j)
                .iter()
                .any(|r| matches!(r, Reason::BrandMismatch { .. })),
            "{j:?}"
        );

        // and the consistent pick of the same bearing writes, as it is
        let (db, job) = db_with_job();
        stored(&db, RIGHT_BEARING, "USER", &bearing_5e12(), true);
        let j = prover(&db, &r, job)
            .item(RIGHT_BEARING.1, RIGHT_BEARING.0)
            .await
            .unwrap();
        assert!(matches!(j, Judgement::Proven { .. }), "{j:?}");
    }

    /// R-d, slice 4: a pick the seller confirmed against its conflicts writes in a later job while
    /// they are still all its conflicts — the confirmation counts for what the seller was shown.
    /// A conflict the seller never confirmed reopens it.
    #[tokio::test]
    async fn a_confirmed_pick_writes_until_a_conflict_it_never_confirmed() {
        let r = registry();
        let shown = crate::matching::gate::conflicts(&evidence(
            WRONG_BEARING.1,
            WRONG_BEARING.0,
            &bearing_5e12(),
            r.vocabulary(),
        ));
        assert!(!shown.is_empty(), "the hurried pick conflicts");
        let confirmed = |db: &Db, reasons: &[Reason]| {
            stored(db, WRONG_BEARING, "USER", &bearing_5e12(), true);
            db.with(|c| {
                c.execute(
                    "UPDATE supplier_links SET confirmed_json = ?1",
                    [serde_json::to_string(reasons).unwrap()],
                )
            })
            .unwrap();
        };

        let (db, job) = db_with_job();
        confirmed(&db, &shown);
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert!(matches!(j, Judgement::Proven { .. }), "{j:?}");

        // confirmed against another value than the one the name now conflicts on
        let (db, job) = db_with_job();
        confirmed(
            &db,
            &[Reason::NumberConflict {
                lacking: vec!["99999".into()],
            }],
        );
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert_eq!(reasons(&j), shown, "reopened on what was never confirmed");

        // slice 4's review: the site gives the confirmed item's code to another item; reached by
        // its code, the link is judged without the confirmation the other name earned
        let (db, job) = db_with_job();
        confirmed(&db, &shown);
        // another bearing, one the name says is not the 44649 either: its conflicts with the
        // product are exactly the ones the seller confirmed for the 69349 name
        let renamed = "بلبرينگ چرخ جلو پرايد قديم 30205 PSN عظام (54) 3154027";
        assert_eq!(
            crate::matching::gate::conflicts(&evidence(
                renamed,
                WRONG_BEARING.0,
                &bearing_5e12(),
                r.vocabulary()
            )),
            shown,
            "the case needs a reason equal in kind and value"
        );
        let j = prover(&db, &r, job)
            .item(renamed, WRONG_BEARING.0)
            .await
            .unwrap();
        assert!(matches!(j, Judgement::Held { .. }), "{j:?}");

        // an automatic link holds no human judgment: even a column naming every reason the gate
        // gives it decides nothing (only a pick ever writes one)
        let (db, job) = db_with_job();
        stored(&db, WRONG_BEARING, "AUTO", &bearing_5e12(), true);
        let gate = reasons(
            &prover(&db, &r, job)
                .item(WRONG_BEARING.1, WRONG_BEARING.0)
                .await
                .unwrap(),
        );
        let (db, job) = db_with_job();
        stored(&db, WRONG_BEARING, "AUTO", &bearing_5e12(), true);
        db.with(|c| {
            c.execute(
                "UPDATE supplier_links SET confirmed_json = ?1",
                [serde_json::to_string(&gate).unwrap()],
            )
        })
        .unwrap();
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert_eq!(reasons(&j), gate, "still held on the gate's own reasons");
    }

    /// Q-5: a pick whose product claims a number the supplier's name lacks is reopened.
    #[tokio::test]
    async fn an_old_pick_with_an_unbacked_number_goes_to_review() {
        let (db, job) = db_with_job();
        let r = registry();
        let item = ("9001", "بلبرينگ وسط پلوس پژو ELMA7");
        stored(&db, item, "USER", &bearing_6c04(), true);
        let j = prover(&db, &r, job).item(item.1, item.0).await.unwrap();
        assert!(
            reasons(&j)
                .iter()
                .any(|r| matches!(r, Reason::NumberUnbacked { .. })),
            "{j:?}"
        );
    }

    /// Q-2: an existing automatic link is re-judged through the whole gate, never grandfathered.
    #[tokio::test]
    async fn a_stored_automatic_link_is_re_judged_by_the_gate() {
        let r = registry();
        let (db, job) = db_with_job();
        stored(&db, MID_SHAFT, "AUTO", &bearing_6c04(), true);
        let j = prover(&db, &r, job)
            .item(MID_SHAFT.1, MID_SHAFT.0)
            .await
            .unwrap();
        assert!(matches!(j, Judgement::Proven { .. }), "{j:?}");

        // the same automatic link onto the wrong bearing: held, however it was stored
        let (db, job) = db_with_job();
        stored(&db, WRONG_BEARING, "AUTO", &bearing_5e12(), true);
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert!(matches!(j, Judgement::Held { .. }), "{j:?}");
    }

    /// The matcher's preserved behaviour: a stored «none» sticks; a name hit under a new code
    /// takes the code without moving the version; no candidate at all is a review with none.
    #[tokio::test]
    async fn a_stored_decision_is_found_by_name_then_by_code() {
        let (db, job) = db_with_job();
        let r = registry();
        db.set_match(SUP, "قطعه‌ای نیست", "N1", None, "NONE", None)
            .unwrap();
        let p = prover(&db, &r, job);
        assert_eq!(
            p.item("قطعه‌ای نیست", "N1").await.unwrap(),
            Judgement::NotInLandau
        );
        // the code finds it under another spelling
        assert_eq!(
            p.item("نام دیگر", "N1").await.unwrap(),
            Judgement::NotInLandau
        );
        let before = db.match_for_name(SUP, "قطعه‌ای نیست").unwrap().unwrap();
        p.item("قطعه‌ای نیست", "N2").await.unwrap();
        let after = db.match_for_name(SUP, "قطعه‌ای نیست").unwrap().unwrap();
        assert_eq!(
            (after.code.as_str(), after.link_version),
            ("N2", before.link_version)
        );
        let j = p.item("zzzz qqqq", "").await.unwrap();
        assert_eq!(
            j,
            Judgement::Held {
                reasons: vec![],
                error: None,
                candidates: vec![],
                link_key: None,
                link_product: None
            }
        );
    }

    /// A fresh match writes only through the V4 gate, and is then stored as a proven automatic
    /// link; one that fails goes to review with its typed reasons and no link stored.
    #[tokio::test]
    async fn a_fresh_match_writes_only_through_the_gate() {
        let r = registry();
        let (db, job) = db_with_job();
        let j = prover(&db, &r, job)
            .item(MID_SHAFT.1, MID_SHAFT.0)
            .await
            .unwrap();
        let Judgement::Proven { key, version, .. } = j else {
            panic!("{j:?}");
        };
        assert!(db.link_by_key(SUP, &key).unwrap().unwrap().writable_for(
            job,
            version,
            &bearing_6c04().id
        ));

        let (db, job) = db_with_job();
        let j = prover(&db, &r, job)
            .item(WRONG_BEARING.1, WRONG_BEARING.0)
            .await
            .unwrap();
        assert!(!reasons(&j).is_empty());
        assert!(db.match_for_name(SUP, WRONG_BEARING.1).unwrap().is_none());
    }

    /// Task 12's links: after the re-import the right bearing's pick is rebuilt onto `…5e12`,
    /// while the hurried pick of the wrong bearing points there too and conflicts — so the right
    /// one is held against it until the seller resolves it; both clamps conflict with the row.
    #[tokio::test]
    async fn the_incident_writes_no_wrong_cost() {
        let (db, job) = db_with_job();
        let r = registry();
        stored(&db, RIGHT_BEARING, "USER", &dead(bearing_5e12()), true);
        stored(&db, WRONG_BEARING, "USER", &bearing_5e12(), true);
        stored(&db, CLAMP_11, "USER", &clamp_576d(), true);
        stored(&db, CLAMP_4, "USER", &clamp_576d(), true);
        let p = prover(&db, &r, job);
        let right = p.item(RIGHT_BEARING.1, RIGHT_BEARING.0).await.unwrap();
        let wrong = p.item(WRONG_BEARING.1, WRONG_BEARING.0).await.unwrap();
        let c11 = p.item(CLAMP_11.1, CLAMP_11.0).await.unwrap();
        let c4 = p.item(CLAMP_4.1, CLAMP_4.0).await.unwrap();
        let Judgement::Proven { key, version, .. } = right else {
            panic!("{right:?}");
        };
        for held in [&wrong, &c11, &c4] {
            assert!(matches!(held, Judgement::Held { .. }), "{held:?}");
        }
        let Judgement::Held { link_product, .. } = &wrong else {
            unreachable!()
        };
        let unresolved: HashSet<String> = link_product.iter().cloned().collect();
        let plan = plan_import(
            &db,
            SUP,
            job,
            &[ImportCandidate {
                item_id: 1,
                product: bearing_5e12().id,
                authenticity: Authenticity::Original,
                cost: 6_194_100,
                qty: 0,
                link_key: key,
                link_version: version,
            }],
            &unresolved,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            matches!(
                &plan[..],
                [Planned::Held {
                    error: BridgeError::LinkConflict { .. },
                    ..
                }]
            ),
            "{plan:?}"
        );
    }

    fn proven_link(db: &Db, job: i64, name: &str, product: &str) -> (String, i64) {
        db.set_match(SUP, name, "", Some((product, "x")), "USER", None)
            .unwrap();
        let key = name_key(name);
        let p = LinkProduct {
            id: product,
            name: "x",
            brand: Some(""),
            alias: Some(""),
        };
        let version = db.link_by_key(SUP, &key).unwrap().unwrap().link_version;
        let v = db
            .validate_link(SUP, &key, version, p, state::VALID, None, job)
            .unwrap()
            .unwrap();
        (key, v)
    }

    fn candidate(
        id: i64,
        product: &str,
        cost: i64,
        qty: i64,
        link: &(String, i64),
    ) -> ImportCandidate {
        ImportCandidate {
            item_id: id,
            product: product.into(),
            authenticity: Authenticity::Original,
            cost,
            qty,
            link_key: link.0.clone(),
            link_version: link.1,
        }
    }

    /// Q-3, R-e: two codes on one row with two costs hold it; equal costs write once with the
    /// quantities summed; and the plan is the same in every order the items come.
    #[test]
    fn collisions_are_decided_over_the_whole_job_in_any_order() {
        let (db, job) = db_with_job();
        let a = proven_link(&db, job, "کد الف", "row1");
        let b = proven_link(&db, job, "کد ب", "row1");
        let c = proven_link(&db, job, "کد ج", "row2");
        let d = proven_link(&db, job, "کد د", "row2");
        let e = proven_link(&db, job, "کد ه", "row3");
        let items = vec![
            candidate(5, "row1", 91_300, 1, &a),
            candidate(2, "row1", 96_800, 1, &b),
            candidate(9, "row2", 50_000, 2, &c),
            candidate(3, "row2", 50_000, 4, &d),
            candidate(7, "row3", 10_000, 1, &e),
        ];
        let expected =
            plan_import(&db, SUP, job, &items, &HashSet::new(), &BTreeMap::new()).unwrap();
        assert_eq!(expected.len(), 3);
        assert!(
            matches!(&expected[0], Planned::Held { items, error: BridgeError::LinkConflict { links: 2 } } if items == &vec![2, 5])
        );
        let Planned::Write(w) = &expected[1] else {
            panic!()
        };
        assert_eq!(w.items(), &[3, 9]);
        assert_eq!(w.lead(), 3);
        assert!(matches!(w.target(), Target::Row { qty: 6, .. }));
        assert_eq!(w.cost(), 50_000);
        // every order of the five gives the same plan
        let mut order: Vec<usize> = (0..items.len()).collect();
        let mut seen = 0;
        permute(&mut order, 0, &mut |o| {
            let shuffled: Vec<ImportCandidate> = o.iter().map(|&i| items[i].clone()).collect();
            assert_eq!(
                plan_import(&db, SUP, job, &shuffled, &HashSet::new(), &BTreeMap::new()).unwrap(),
                expected
            );
            seen += 1;
        });
        assert_eq!(seen, 120);
    }

    fn permute(v: &mut Vec<usize>, k: usize, f: &mut dyn FnMut(&[usize])) {
        if k == v.len() {
            f(v);
            return;
        }
        for i in k..v.len() {
            v.swap(k, i);
            permute(v, k + 1, f);
            v.swap(k, i);
        }
    }

    /// R-b, task 8: a link changed after this job proved it holds its item; one never validated
    /// by this job (a migrated link, `UNVERIFIED`) never writes.
    #[test]
    fn no_link_writes_unless_this_job_proved_it_at_its_version() {
        let (db, job) = db_with_job();
        let a = proven_link(&db, job, "کد الف", "row1");
        db.set_match(SUP, "کد الف", "", Some(("row9", "y")), "USER", None)
            .unwrap();
        db.set_match(SUP, "کد ب", "", Some(("row2", "y")), "USER", None)
            .unwrap();
        let migrated = (name_key("کد ب"), 1);
        let plan = plan_import(
            &db,
            SUP,
            job,
            &[
                candidate(1, "row1", 10, 1, &a),
                candidate(2, "row2", 10, 1, &migrated),
            ],
            &HashSet::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(plan.iter().all(|p| matches!(
            p,
            Planned::Held {
                error: BridgeError::LinkChanged,
                ..
            }
        )));
    }

    /// R-e in REFRESH: every link on the row proven and agreeing writes; different costs, or a
    /// link with no price beside one with a price, hold the row.
    #[test]
    fn a_refresh_row_writes_only_when_every_link_agrees() {
        let (db, job) = db_with_job();
        proven_link(&db, job, "کد الف", "p");
        proven_link(&db, job, "کد ب", "p");
        let links: Vec<MatchRow> = ["کد الف", "کد ب"]
            .iter()
            .map(|n| db.match_for_name(SUP, n).unwrap().unwrap())
            .collect();
        let with = |a: Option<i64>, b: Option<i64>| {
            prove_refresh(
                &db,
                SUP,
                job,
                1,
                "s1",
                "p",
                &[(links[0].clone(), a), (links[1].clone(), b)],
            )
            .unwrap()
        };
        assert!(
            matches!(with(Some(5), Some(5)), Planned::Write(w) if w.cost() == 5 && w.links().len() == 2)
        );
        assert!(matches!(
            with(Some(5), Some(6)),
            Planned::Held {
                error: BridgeError::LinkConflict { links: 2 },
                ..
            }
        ));
        assert!(matches!(
            with(Some(5), None),
            Planned::Held {
                error: BridgeError::LinkConflict { .. },
                ..
            }
        ));
        // a link edited since: held whatever the costs
        db.set_match(SUP, "کد ب", "", Some(("q", "y")), "USER", None)
            .unwrap();
        assert!(matches!(
            with(Some(5), Some(5)),
            Planned::Held {
                error: BridgeError::LinkChanged,
                ..
            }
        ));
    }

    /// PI-22: a product's other stored links, carried by no item of the job, join the collision
    /// check — agreeing, they join the write (and the writer re-checks them); a different cost, no
    /// price, or a link this job did not prove holds the row.
    #[test]
    fn a_products_other_links_join_the_collision_check() {
        let (db, job) = db_with_job();
        let a = proven_link(&db, job, "CW3.6", "row1");
        let b = proven_link(&db, job, "CW4.8", "row1");
        let items = [candidate(1, "row1", 96_800, 1, &b)];
        let plan = |cost: Option<i64>, version: i64| {
            let others = BTreeMap::from([(
                "row1".to_string(),
                vec![OtherLink {
                    key: a.0.clone(),
                    version,
                    cost,
                }],
            )]);
            plan_import(&db, SUP, job, &items, &HashSet::new(), &others).unwrap()
        };
        let held = |p: Vec<Planned>| matches!(&p[..], [Planned::Held { items, error: BridgeError::LinkConflict { links: 2 } }] if items == &vec![1]);
        assert!(held(plan(Some(91_300), a.1)), "two costs");
        assert!(held(plan(None, a.1)), "the other link unpriced");
        assert!(
            held(plan(Some(96_800), a.1 + 1)),
            "not proven at that version"
        );
        let [Planned::Write(w)] = &plan(Some(96_800), a.1)[..] else {
            panic!("agreeing links write");
        };
        assert_eq!(w.cost(), 96_800);
        assert!(w.links().contains(&a) && w.links().contains(&b));
        // never validated by this job: held
        db.set_match(SUP, "CW3.6", "", Some(("row1", "x")), "USER", None)
            .unwrap();
        let fresh = db.link_by_key(SUP, &a.0).unwrap().unwrap().link_version;
        assert!(held(plan(Some(96_800), fresh)));
    }

    /// Slice 2's review: an item set aside because its link changed still holds its product —
    /// the sibling on the same row never writes its own cost alone in its place.
    #[test]
    fn an_item_set_aside_still_holds_its_row() {
        let (db, job) = db_with_job();
        let a = proven_link(&db, job, "CW3.6", "row1");
        let b = proven_link(&db, job, "CW4.8", "row1");
        // the seller confirms the first on the Matches screen mid-job: its version moves
        db.set_match(SUP, "CW3.6", "", Some(("row1", "x")), "AUTO", None)
            .unwrap();
        db.set_match(SUP, "CW3.6", "", Some(("row1", "x")), "USER", None)
            .unwrap();
        let plan = plan_import(
            &db,
            SUP,
            job,
            &[
                candidate(1, "row1", 91_300, 1, &a),
                candidate(2, "row1", 96_800, 1, &b),
            ],
            &HashSet::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(plan.len(), 2, "{plan:?}");
        assert!(
            plan.iter().all(|p| matches!(p, Planned::Held { .. })),
            "{plan:?}"
        );
    }
}
