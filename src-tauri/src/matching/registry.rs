//! Landau's registry, read whole once per job (plan 130 R-b, slice 2's ruling). The client gateway
//! offers no lookup by id — `GET /-/career/products/:id` answers `{"data":{}}` for a real id and a
//! made-up one alike (task 1, 2026-11-08) — while the seller's picker lists every approved
//! definition on an empty term. So VALIDATE reads that list to its `total` and refuses to judge on
//! anything less: existence, the rebuild key, the evidence vocabulary and every candidate set come
//! from this one snapshot.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};

use futures_util::stream::{self, StreamExt, TryStreamExt};

use super::evidence::Vocabulary;
use super::PAGE;
use crate::errors::{BridgeError, Result};
use crate::landau::{Candidate, RegistryPage};

/// Pages of the picker in flight at once while reading it whole (89 s at 8, 101 s at 4 on the
/// local stack's 52,899 definitions, 2026-11-08).
pub const PARALLEL_PAGES: usize = 8;

/// The identity of a definition (Q-1): name, brand and alias exactly as the registry stores them,
/// absent fields as `""`. Landau holds `{name, alias, brand}` unique per placement, and every
/// registry row's placement is empty (fact 12).
pub type Identity = (String, String, String);

pub fn identity_of(name: &str, brand: Option<&str>, alias: Option<&str>) -> Identity {
    (
        name.trim().to_string(),
        brand.unwrap_or("").trim().to_string(),
        alias.unwrap_or("").trim().to_string(),
    )
}

/// The picker's own reading of a term (Landau `persianTerm`): trimmed, Arabic ي and ك as Persian.
fn picker_term(term: &str) -> String {
    term.trim()
        .replace('ي', "ی")
        .replace('ك', "ک")
        .to_lowercase()
}

/// Every approved definition, as one job reads it.
pub struct Registry {
    /// In the picker's order: by name, then by id (`{name: 1, _id: 1}`).
    products: Vec<Candidate>,
    /// Lower-cased `name`, `alias` and `brand` per product, for the substring rule.
    haystacks: Vec<[String; 3]>,
    by_id: HashMap<String, usize>,
    by_identity: HashMap<Identity, Vec<usize>>,
    vocabulary: Vocabulary,
    /// A term's hits, kept for the job: the same words recur across a job's items, and
    /// [`super::candidates`] asks for each term's pages one call at a time.
    hits: Mutex<HashMap<String, Arc<Vec<usize>>>>,
}

impl Registry {
    pub fn from_products(mut products: Vec<Candidate>) -> Registry {
        products.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        products.dedup_by(|a, b| a.id == b.id);
        let lower = |s: Option<&str>| s.unwrap_or("").to_lowercase();
        let haystacks = products
            .iter()
            .map(|p| {
                [
                    lower(Some(&p.name)),
                    lower(p.alias.as_deref()),
                    lower(p.brand.as_deref()),
                ]
            })
            .collect();
        let mut by_id = HashMap::new();
        let mut by_identity: HashMap<Identity, Vec<usize>> = HashMap::new();
        for (i, p) in products.iter().enumerate() {
            by_id.insert(p.id.clone(), i);
            by_identity
                .entry(identity_of(&p.name, p.brand.as_deref(), p.alias.as_deref()))
                .or_default()
                .push(i);
        }
        let vocabulary = Vocabulary::build(
            products
                .iter()
                .map(|p| (p.name.as_str(), p.brand.as_deref())),
        );
        Registry {
            products,
            haystacks,
            by_id,
            by_identity,
            vocabulary,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.products.len()
    }

    pub fn is_empty(&self) -> bool {
        self.products.is_empty()
    }

    pub fn vocabulary(&self) -> &Vocabulary {
        &self.vocabulary
    }

    /// The definition with this id, when it is still approved.
    pub fn get(&self, id: &str) -> Option<&Candidate> {
        self.by_id.get(id).map(|&i| &self.products[i])
    }

    /// The current definitions whose name, brand and alias equal these exactly (Q-1).
    pub fn with_identity(&self, identity: &Identity) -> Vec<&Candidate> {
        self.by_identity
            .get(identity)
            .map(|ix| ix.iter().map(|&i| &self.products[i]).collect())
            .unwrap_or_default()
    }

    /// The picker's answer, computed here: a case-insensitive literal substring over name, alias
    /// and brand, in the picker's order, paged — so [`super::candidates`] gathers from the snapshot
    /// exactly as it gathers from Landau.
    pub fn search(&self, term: &str, limit: usize, skip: usize) -> RegistryPage {
        let needle = picker_term(term);
        let cached = self.hits.lock().ok().and_then(|m| m.get(&needle).cloned());
        let hits = cached.unwrap_or_else(|| {
            let found: Arc<Vec<usize>> = Arc::new(
                (0..self.products.len())
                    .filter(|&i| {
                        needle.is_empty() || self.haystacks[i].iter().any(|h| h.contains(&needle))
                    })
                    .collect(),
            );
            if let Ok(mut m) = self.hits.lock() {
                m.insert(needle.clone(), found.clone());
            }
            found
        });
        RegistryPage {
            total: hits.len(),
            items: hits
                .iter()
                .skip(skip)
                .take(limit.clamp(1, PAGE))
                .map(|&i| self.products[i].clone())
                .collect(),
        }
    }
}

/// Reads the whole registry through `page(skip)` (the picker on an empty term, [`PAGE`] rows a
/// page), [`PARALLEL_PAGES`] at a time. Complete or an error: every page must report the first
/// page's `total`, and exactly that many distinct definitions must arrive. A read that falls
/// short is tried once more from the start (a definition approved or removed mid-read shifts the
/// pages), then refused as [`BridgeError::RegistryIncomplete`].
pub async fn read_whole<F, Fut>(page: &F) -> Result<Registry>
where
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<RegistryPage>>,
{
    let mut last = (0, 0);
    for _ in 0..2 {
        match read_once(page).await? {
            // an empty registry is a degraded answer, never a catalogue with nothing in it
            Ok(products) if products.is_empty() => last = (0, 0),
            Ok(products) => return Ok(Registry::from_products(products)),
            Err(short) => {
                tracing::warn!(read = short.0, total = short.1, "registry read incomplete");
                last = short;
            }
        }
    }
    Err(BridgeError::RegistryIncomplete {
        read: last.0,
        total: last.1,
    })
}

/// One pass: the products, or `(distinct read, total)` when the pass fell short.
async fn read_once<F, Fut>(page: &F) -> Result<std::result::Result<Vec<Candidate>, (usize, usize)>>
where
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<RegistryPage>>,
{
    let first = page(0).await?;
    let total = first.total;
    let skips: Vec<usize> = (1..total.div_ceil(PAGE)).map(|n| n * PAGE).collect();
    let rest: Vec<RegistryPage> = stream::iter(skips)
        .map(page)
        .buffer_unordered(PARALLEL_PAGES)
        .try_collect()
        .await?;
    let consistent = rest.iter().all(|p| p.total == total);
    let mut seen: HashSet<String> = HashSet::new();
    let mut products = Vec::with_capacity(total);
    for p in std::iter::once(first).chain(rest) {
        for c in p.items {
            if seen.insert(c.id.clone()) {
                products.push(c);
            }
        }
    }
    if consistent && products.len() == total {
        Ok(Ok(products))
    } else {
        Ok(Err((products.len(), total)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) fn cand(
        id: &str,
        name: &str,
        alias: Option<&str>,
        brand: Option<&str>,
    ) -> Candidate {
        Candidate {
            id: id.into(),
            name: name.into(),
            alias: alias.map(str::to_string),
            brand: brand.map(str::to_string),
            score: 0.0,
        }
    }

    fn sample() -> Vec<Candidate> {
        vec![
            cand("b2", "لنت جلو پراید", None, Some("عظام")),
            cand("a1", "Filter", Some("oil FILTER | x"), Some("ELMA7")),
            cand(
                "c3",
                "بلبرینگ چرخ عقب",
                Some("بلبرینگ 44649"),
                Some("PSN عظام"),
            ),
            cand("c0", "بلبرینگ چرخ عقب", Some("بلبرینگ 44649"), Some("DPI")),
        ]
    }

    #[test]
    fn the_local_picker_reads_a_term_as_landau_does() {
        let r = Registry::from_products(sample());
        // case-insensitive over name, alias and brand
        assert_eq!(r.search("filter", 10, 0).total, 1);
        assert_eq!(r.search("elma7", 10, 0).total, 1);
        // Arabic ي/ك typed into the term read as the Persian the registry stores
        assert_eq!(r.search("  لنت جلو پرايد ", 10, 0).total, 1);
        // a literal substring, never a word rule: «عظام» is inside «PSN عظام»
        let azam = r.search("عظام", 10, 0);
        assert_eq!(azam.total, 2);
        // alphabetical, the id breaking the tie
        let all = r.search("", 10, 0);
        let ids: Vec<&str> = all.items.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["a1", "c0", "c3", "b2"]);
        // paged
        let page = r.search("", 2, 2);
        assert_eq!(
            (page.total, page.items.len(), page.items[0].id.as_str()),
            (4, 2, "c3")
        );
    }

    #[test]
    fn a_definition_is_found_by_id_and_by_its_exact_identity() {
        let r = Registry::from_products(sample());
        assert_eq!(r.get("c3").unwrap().brand.as_deref(), Some("PSN عظام"));
        assert!(r.get("dead").is_none());
        let exact = identity_of("بلبرینگ چرخ عقب", Some("PSN عظام"), Some("بلبرینگ 44649"));
        assert_eq!(r.with_identity(&exact).len(), 1);
        let other_brand = identity_of("بلبرینگ چرخ عقب", Some("عظام"), Some("بلبرینگ 44649"));
        assert!(
            r.with_identity(&other_brand).is_empty(),
            "never a fuzzy brand"
        );
        let no_alias = identity_of("بلبرینگ چرخ عقب", Some("PSN عظام"), None);
        assert!(
            r.with_identity(&no_alias).is_empty(),
            "the alias is part of the key"
        );
    }

    /// A picker of `n` definitions, `PAGE` a page; `drop` loses one row on the page at that skip.
    fn picker(
        n: usize,
        drop: Option<usize>,
        calls: &AtomicUsize,
    ) -> impl Fn(usize) -> std::future::Ready<Result<RegistryPage>> + '_ {
        move |skip| {
            calls.fetch_add(1, Ordering::SeqCst);
            let mut items: Vec<Candidate> = (skip..(skip + PAGE).min(n))
                .map(|i| {
                    cand(
                        &format!("p{i:05}"),
                        &format!("قطعه {i:05}"),
                        None,
                        Some("عظام"),
                    )
                })
                .collect();
            if drop == Some(skip) {
                items.pop();
            }
            std::future::ready(Ok(RegistryPage { total: n, items }))
        }
    }

    #[tokio::test]
    async fn the_registry_is_read_whole_or_refused() {
        let calls = AtomicUsize::new(0);
        let whole = read_whole(&picker(250, None, &calls)).await.unwrap();
        assert_eq!(whole.len(), 250);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "pages 0, 100, 200");
        assert!(whole.get("p00249").is_some());

        let calls = AtomicUsize::new(0);
        let short = read_whole(&picker(250, Some(100), &calls)).await;
        assert_eq!(
            short.err(),
            Some(BridgeError::RegistryIncomplete {
                read: 249,
                total: 250
            })
        );
        assert_eq!(calls.load(Ordering::SeqCst), 6, "tried twice, then refused");
    }

    #[tokio::test]
    async fn an_empty_registry_is_refused() {
        let calls = AtomicUsize::new(0);
        assert_eq!(
            read_whole(&picker(0, None, &calls)).await.err(),
            Some(BridgeError::RegistryIncomplete { read: 0, total: 0 })
        );
    }

    #[tokio::test]
    async fn a_failed_page_stops_the_read() {
        let failing = |skip: usize| {
            std::future::ready(if skip == 100 {
                Err(BridgeError::LandauUnreachable("down".into()))
            } else {
                Ok(RegistryPage {
                    total: 300,
                    items: vec![],
                })
            })
        };
        assert!(matches!(
            read_whole(&failing).await,
            Err(BridgeError::LandauUnreachable(_))
        ));
    }
}
