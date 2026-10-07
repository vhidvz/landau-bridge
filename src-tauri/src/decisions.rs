//! Portable match decisions (LND-Q07). Every stored decision — automatic, the seller's own, from
//! the name map, «not in Landau» — goes into a versioned JSON file, and a file from another
//! installation merges into this one without undoing the seller's work here:
//!
//! - a name with no decision here takes the file's;
//! - the same decision on both sides changes nothing;
//! - a different one goes to whichever is more deliberate — the seller's pick or «not in Landau»
//!   over the name map over an automatic match — and on a tie this installation's stays, listed as
//!   a conflict, unless the seller asked for the file's to win;
//! - a decision whose product this Landau no longer lists is not applied, so the part comes back
//!   to review rather than being written against a product that is gone;
//! - a malformed entry is counted and skipped; a file for another supplier or format is refused.
//!
//! Undecided parts are not in the file: they are not decisions, and another installation meets
//! them again on its first import, with candidates from its own catalogue.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::db::matches::MatchRow;
use crate::errors::{BridgeError, Result};
use crate::landau::RegistryPage;
use crate::matching::normalize::name_key;

pub const FORMAT: &str = "landau-bridge-decisions";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionFile {
    pub format: String,
    pub version: u32,
    pub supplier: String,
    pub exported_at: String,
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub name_seen: String,
    #[serde(default)]
    pub code: String,
    /// AUTO | USER | MAP | NONE
    pub how: String,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub similarity: Option<i64>,
    #[serde(default)]
    pub decided_at: Option<String>,
    /// When the link last answered a price; refresh prefers the freshest of several links to one
    /// product (`jobs/refresh.rs`), so it travels with the decision.
    #[serde(default)]
    pub priced_at: Option<String>,
}

/// A decision both sides hold differently, where this installation's was kept.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conflict {
    pub name_seen: String,
    pub code: String,
    pub mine_how: String,
    pub mine_product: Option<String>,
    pub file_how: String,
    pub file_product: Option<String>,
}

/// A decision for a product this Landau's registry does not list (any more).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Missing {
    pub name_seen: String,
    pub code: String,
    pub product_id: String,
    pub product_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportReport {
    pub total: usize,
    pub added: usize,
    pub unchanged: usize,
    pub replaced: usize,
    pub kept: Vec<Conflict>,
    pub missing: Vec<Missing>,
    pub invalid: usize,
}

/// What an import would write, before the catalogue check.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// `(decision, replaces one here)`.
    pub writes: Vec<(Decision, bool)>,
    pub report: ImportReport,
}

impl From<&MatchRow> for Decision {
    fn from(m: &MatchRow) -> Self {
        Decision {
            name_seen: m.name_seen.clone(),
            code: m.code.clone(),
            how: m.how.clone(),
            product_id: m.product_id.clone(),
            product_name: m.product_name.clone(),
            similarity: m.similarity,
            decided_at: Some(m.decided_at.clone()),
            priced_at: m.priced_at.clone(),
        }
    }
}

/// Registry pages read per product at most when checking it is still listed (300 rows).
const CHECK_PAGES: usize = 3;
const CHECK_PAGE_SIZE: usize = 100;

pub fn export(supplier: &str, rows: &[MatchRow], exported_at: &str) -> DecisionFile {
    DecisionFile {
        format: FORMAT.into(),
        version: VERSION,
        supplier: supplier.into(),
        exported_at: exported_at.into(),
        decisions: rows.iter().map(Decision::from).collect(),
    }
}

/// Reads a file and refuses one of another format, version or supplier.
pub fn parse(text: &str, supplier: &str) -> Result<DecisionFile> {
    let not_ours = || BridgeError::Other("این فایل، فایل تصمیم‌های پل لاندا نیست.".into());
    let file: DecisionFile = serde_json::from_str(text).map_err(|_| not_ours())?;
    if file.format != FORMAT {
        return Err(not_ours());
    }
    if file.version > VERSION {
        return Err(BridgeError::Other(
            "این فایل را نسخهٔ جدیدتری از برنامه ساخته است؛ اول برنامه را به‌روز کنید.".into(),
        ));
    }
    if file.version != VERSION {
        return Err(not_ours());
    }
    if file.supplier != supplier {
        return Err(BridgeError::Other(format!(
            "این فایل تصمیم‌های تأمین‌کنندهٔ دیگری است ({}).",
            file.supplier
        )));
    }
    Ok(file)
}

/// How deliberate a decision is: the seller's own (a pick, or «not in Landau») over the name map
/// over an automatic match.
fn rank(how: &str) -> u8 {
    match how {
        "USER" | "NONE" => 2,
        "MAP" => 1,
        _ => 0,
    }
}

fn valid(d: &Decision) -> bool {
    let has_product = d
        .product_id
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());
    !d.name_seen.trim().is_empty()
        && match d.how.as_str() {
            "NONE" => d.product_id.is_none(),
            "USER" | "MAP" | "AUTO" => has_product,
            _ => false,
        }
}

/// Classifies every decision of `file` against this installation's (`local`, by name).
pub fn plan(
    file: &DecisionFile,
    local: impl Fn(&str) -> Result<Option<MatchRow>>,
    prefer_file: bool,
) -> Result<Plan> {
    let mut report = ImportReport {
        total: file.decisions.len(),
        ..Default::default()
    };
    let mut writes = Vec::new();
    let mut seen = HashSet::new();
    for d in &file.decisions {
        if !valid(d) || !seen.insert(name_key(&d.name_seen)) {
            report.invalid += 1;
            continue;
        }
        let Some(mine) = local(&d.name_seen)? else {
            report.added += 1;
            writes.push((d.clone(), false));
            continue;
        };
        if mine.product_id == d.product_id {
            report.unchanged += 1;
            continue;
        }
        let (file_rank, mine_rank) = (rank(&d.how), rank(&mine.how));
        if file_rank > mine_rank || (file_rank == mine_rank && prefer_file) {
            report.replaced += 1;
            writes.push((d.clone(), true));
        } else {
            report.kept.push(Conflict {
                name_seen: d.name_seen.clone(),
                code: d.code.clone(),
                mine_how: mine.how,
                mine_product: mine.product_name,
                file_how: d.how.clone(),
                file_product: d.product_name.clone(),
            });
        }
    }
    Ok(Plan { writes, report })
}

/// The products among `writes` this Landau's registry does not list.
pub async fn missing_products<F, Fut>(
    writes: &[(Decision, bool)],
    search: F,
    parallel: usize,
) -> Result<HashSet<String>>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<RegistryPage>>,
{
    use futures_util::stream::{self, StreamExt, TryStreamExt};
    let mut products: HashMap<String, String> = HashMap::new();
    for (d, _) in writes {
        if let Some(id) = &d.product_id {
            products
                .entry(id.clone())
                .or_insert_with(|| d.product_name.clone().unwrap_or_default());
        }
    }
    let search = &search;
    let listed: Vec<(String, bool)> = stream::iter(products)
        .map(|(id, name)| async move {
            if name.trim().is_empty() {
                return Ok((id, false)); // nothing to look it up by: not applied, never guessed
            }
            for page in 0..CHECK_PAGES {
                let hits = match search(name.clone(), page * CHECK_PAGE_SIZE).await {
                    Ok(hits) => hits,
                    // A Landau out of reach (or a key it refuses) stops the import whole.
                    Err(e) if crate::jobs::fatal(&e) => return Err(e),
                    // One query it rejects only leaves that product unconfirmed.
                    Err(_) => return Ok((id, false)),
                };
                if hits.items.iter().any(|c| c.id == id) {
                    return Ok((id, true));
                }
                if hits.items.len() < CHECK_PAGE_SIZE {
                    break;
                }
            }
            Ok((id, false))
        })
        .buffer_unordered(parallel.max(1))
        .try_collect()
        .await?;
    Ok(listed
        .into_iter()
        .filter(|(_, listed)| !listed)
        .map(|(id, _)| id)
        .collect())
}

/// Drops the writes whose product is missing, recording them in the report.
pub fn without_missing(plan: Plan, missing: &HashSet<String>) -> Plan {
    let Plan { writes, mut report } = plan;
    let mut kept = Vec::with_capacity(writes.len());
    for (d, replaces) in writes {
        match d.product_id.as_ref().filter(|id| missing.contains(*id)) {
            Some(id) => {
                if replaces {
                    report.replaced -= 1;
                } else {
                    report.added -= 1;
                }
                report.missing.push(Missing {
                    name_seen: d.name_seen.clone(),
                    code: d.code.clone(),
                    product_id: id.clone(),
                    product_name: d.product_name.clone(),
                });
            }
            None => kept.push((d, replaces)),
        }
    }
    Plan {
        writes: kept,
        report,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landau::Candidate;

    const SUP: &str = "barezpakhsh";

    fn row(name: &str, code: &str, how: &str, product: Option<&str>) -> MatchRow {
        MatchRow {
            supplier: SUP.into(),
            name_key: name_key(name),
            code: code.into(),
            product_id: product.map(str::to_string),
            product_name: product.map(|p| format!("name of {p}")),
            how: how.into(),
            similarity: product.map(|_| 80),
            name_seen: name.into(),
            priced_at: product.map(|_| "2026-10-02T00:00:00Z".to_string()),
            decided_at: "2026-10-01T00:00:00Z".into(),
        }
    }

    fn file(decisions: Vec<Decision>) -> DecisionFile {
        DecisionFile {
            format: FORMAT.into(),
            version: VERSION,
            supplier: SUP.into(),
            exported_at: "2026-10-07T00:00:00Z".into(),
            decisions,
        }
    }

    fn local_of(rows: Vec<MatchRow>) -> impl Fn(&str) -> Result<Option<MatchRow>> {
        let by_key: HashMap<String, MatchRow> =
            rows.into_iter().map(|r| (r.name_key.clone(), r)).collect();
        move |name| Ok(by_key.get(&name_key(name)).cloned())
    }

    #[test]
    fn a_decision_file_survives_the_round_trip() {
        let rows = vec![
            row("لنت جلو پراید", "A1", "USER", Some("p1")),
            row("شمع موتور", "C3", "NONE", None),
            row("فیلتر روغن", "B2", "AUTO", Some("p2")),
        ];
        let text =
            serde_json::to_string_pretty(&export(SUP, &rows, "2026-10-07T00:00:00Z")).unwrap();
        let back = parse(&text, SUP).unwrap();
        assert_eq!(back.decisions.len(), 3);
        assert_eq!(back.decisions[0], Decision::from(&rows[0]));
        assert_eq!(back.decisions[1].product_id, None);
        assert_eq!(back.decisions[1].how, "NONE");
    }

    #[test]
    fn a_file_of_another_kind_is_refused() {
        let ok = serde_json::to_string(&file(vec![])).unwrap();
        assert!(parse(&ok, SUP).is_ok());
        assert!(parse(&ok, "another-supplier").is_err(), "another supplier");
        assert!(
            parse(&ok.replace(FORMAT, "something-else"), SUP).is_err(),
            "another format"
        );
        assert!(
            parse(&ok.replace("\"version\":1", "\"version\":2"), SUP).is_err(),
            "a newer version"
        );
        assert!(parse("not json", SUP).is_err());
        assert!(
            parse("name,product\nx,y", SUP).is_err(),
            "a CSV is not a decision file"
        );
    }

    fn d(name: &str, code: &str, how: &str, product: Option<&str>) -> Decision {
        Decision::from(&row(name, code, how, product))
    }

    #[test]
    fn the_merge_keeps_deliberate_decisions_and_reports_true_conflicts() {
        let here = vec![
            row("same", "S", "USER", Some("p1")),
            row("auto here", "AH", "AUTO", Some("p-auto")),
            row("mine manual", "MM", "USER", Some("p-mine")),
            row("tie", "T", "USER", Some("p-mine")),
            row("none here", "NH", "NONE", None),
        ];
        let incoming = file(vec![
            d("new part", "N", "USER", Some("p-new")),
            d("same", "S", "USER", Some("p1")),
            d("auto here", "AH", "USER", Some("p-user")), // manual beats automatic
            d("mine manual", "MM", "AUTO", Some("p-auto")), // automatic loses to manual
            d("tie", "T", "USER", Some("p-file")),        // equal rank: a conflict
            d("none here", "NH", "MAP", Some("p-map")),   // «not in Landau» outranks the map
            Decision {
                how: "BOGUS".into(),
                ..d("bad", "X", "USER", Some("p"))
            },
            Decision {
                product_id: None,
                ..d("no product", "Y", "USER", Some("p"))
            },
            d("new part", "N", "USER", Some("p-dup")), // a second entry for one name
        ]);
        let plan = plan(&incoming, local_of(here.clone()), false).unwrap();
        let r = &plan.report;
        assert_eq!(r.total, 9);
        assert_eq!((r.added, r.unchanged, r.replaced, r.invalid), (1, 1, 1, 3));
        assert_eq!(r.kept.len(), 3, "{:?}", r.kept);
        let tie = r.kept.iter().find(|c| c.name_seen == "tie").unwrap();
        assert_eq!(tie.mine_product.as_deref(), Some("name of p-mine"));
        assert_eq!(tie.file_product.as_deref(), Some("name of p-file"));
        let written: Vec<(&str, bool)> = plan
            .writes
            .iter()
            .map(|(w, replaces)| (w.name_seen.as_str(), *replaces))
            .collect();
        assert_eq!(written, vec![("new part", false), ("auto here", true)]);

        // Asked to prefer the file, only the equal-rank conflict changes side.
        let plan = super::plan(&incoming, local_of(here), true).unwrap();
        assert_eq!(plan.report.replaced, 2);
        assert_eq!(
            plan.report.kept.len(),
            2,
            "manual over automatic still holds"
        );
    }

    fn page(ids: &[&str]) -> RegistryPage {
        RegistryPage {
            total: ids.len(),
            items: ids
                .iter()
                .map(|id| Candidate {
                    id: id.to_string(),
                    name: String::new(),
                    alias: None,
                    brand: None,
                    score: 0.0,
                })
                .collect(),
        }
    }

    /// The registry is searched by the product's name; a product counts as present only when its
    /// own id comes back, on any of the first pages.
    #[tokio::test]
    async fn products_the_catalogue_no_longer_lists_are_found_and_not_applied() {
        let incoming = file(vec![
            d("kept part", "K", "USER", Some("p-here")),
            d("deep part", "D", "USER", Some("p-deep")),
            d("gone part", "G", "USER", Some("p-gone")),
            d("not in landau", "Z", "NONE", None),
        ]);
        let plan = plan(&incoming, local_of(vec![]), false).unwrap();
        let asked = std::sync::Mutex::new(Vec::new());
        let missing = missing_products(
            &plan.writes,
            |term, skip| {
                asked.lock().unwrap().push((term.clone(), skip));
                let ids: Vec<&str> = match (term.as_str(), skip) {
                    ("name of p-here", 0) => vec!["other", "p-here"],
                    ("name of p-deep", 0) => vec!["x"; 100],
                    ("name of p-deep", 100) => vec!["p-deep"],
                    _ => vec!["unrelated"],
                };
                let p = page(&ids);
                async move { Ok(p) }
            },
            4,
        )
        .await
        .unwrap();
        assert_eq!(missing, HashSet::from(["p-gone".to_string()]));
        assert!(
            !asked.lock().unwrap().iter().any(|(t, _)| t.is_empty()),
            "«not in Landau» needs no lookup"
        );

        let plan = without_missing(plan, &missing);
        assert_eq!(
            plan.report.added, 3,
            "two products found, and the «not in Landau»"
        );
        assert_eq!(plan.report.missing.len(), 1);
        assert_eq!(plan.report.missing[0].product_id, "p-gone");
        assert!(plan.writes.iter().all(|(w, _)| w.name_seen != "gone part"));
    }

    /// A Landau out of reach stops the import before anything is written.
    #[tokio::test]
    async fn an_unreachable_landau_stops_the_check() {
        let incoming = file(vec![d("part", "P", "USER", Some("p1"))]);
        let plan = plan(&incoming, local_of(vec![]), false).unwrap();
        let err = missing_products(
            &plan.writes,
            |_, _| async { Err(BridgeError::LandauUnreachable("connection refused".into())) },
            4,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, BridgeError::LandauUnreachable(_)));
    }
}
