//! Part matching (spec §6 as amended by §13): candidates gathered from the seller's registry
//! picker (a substring search), scored by token-set similarity against the normalised invoice
//! name, auto-accepted at the threshold, every decision persisted.

pub mod evidence;
pub mod gate;
pub mod normalize;
pub mod registry;
pub mod similarity;

use std::collections::HashSet;
use std::future::Future;

use serde::Serialize;

use crate::db::Db;
use crate::errors::Result;
use crate::landau::{Candidate, RegistryPage};
use normalize::{key_text, search_term};
use similarity::token_jaccard;

/// Candidates shown on the review screen.
pub const CANDIDATES: usize = 10;
/// The picker's page size (its hard maximum).
pub const PAGE: usize = 100;
/// A single token matching more rows than this is too generic to page through («پراید» ≈ 5 300).
pub const BROAD_TOKEN: usize = 300;
/// Stop gathering once this many distinct definitions are in hand.
pub const MAX_CANDIDATES: usize = 500;
/// Pages read per query at most (300 rows).
const PAGES_PER_QUERY: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub candidate: Candidate,
    /// 0–100 (spec §6), the figure the threshold is compared with.
    pub similarity: u8,
}

/// Best token-set Jaccard of the term against the name, the name with its brand, and each alias
/// (with and without the brand) — the brand often appears in the invoice name and not in the
/// registry name.
pub fn similarity_of(term: &str, c: &Candidate) -> u8 {
    let brand = c.brand.as_deref().filter(|b| !b.is_empty());
    let mut best = token_jaccard(term, &c.name);
    if let Some(brand) = brand {
        best = best.max(token_jaccard(term, &format!("{} {brand}", c.name)));
    }
    if let Some(alias) = &c.alias {
        for a in alias.split('|').map(str::trim).filter(|a| !a.is_empty()) {
            best = best.max(token_jaccard(term, a));
            if let Some(brand) = brand {
                best = best.max(token_jaccard(term, &format!("{a} {brand}")));
            }
        }
    }
    best
}

pub fn score(term: &str, candidates: &[Candidate]) -> Vec<Scored> {
    let mut scored: Vec<Scored> = candidates
        .iter()
        .map(|c| Scored {
            candidate: c.clone(),
            similarity: similarity_of(term, c),
        })
        .collect();
    scored.sort_by(|a, b| {
        b.similarity
            .cmp(&a.similarity)
            .then(b.candidate.score.total_cmp(&a.candidate.score))
    });
    scored
}

/// The picker queries for a term, most specific first: the whole term, each adjacent bigram, then
/// each single token of at least two characters. Deduped, order kept.
pub fn queries(term: &str) -> Vec<String> {
    let cleaned = key_text(term);
    let tokens: Vec<&str> = cleaned.split(' ').filter(|t| !t.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut push = |q: String| {
        if !q.is_empty() && !out.contains(&q) {
            out.push(q);
        }
    };
    push(tokens.join(" "));
    for pair in tokens.windows(2) {
        push(pair.join(" "));
    }
    for t in tokens.iter().filter(|t| t.chars().count() >= 2) {
        push((*t).to_string());
    }
    out
}

/// Gather registry candidates for a term through the picker (`search(query, limit, skip)`):
/// every query in [`queries`] order, single tokens broader than [`BROAD_TOKEN`] skipped, up to
/// [`PAGES_PER_QUERY`] pages each, unioned by id up to [`MAX_CANDIDATES`]. When nothing at all
/// matched, the least broad skipped token's first page is the fallback.
pub async fn candidates<F, Fut>(search: &F, term: &str) -> Result<Vec<Candidate>>
where
    F: Fn(String, usize, usize) -> Fut,
    Fut: Future<Output = Result<RegistryPage>>,
{
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Candidate> = Vec::new();
    let mut fallback: Option<(usize, String)> = None;
    let mut extend = |out: &mut Vec<Candidate>, items: Vec<Candidate>| {
        for c in items {
            if seen.insert(c.id.clone()) {
                out.push(c);
            }
        }
    };
    for q in queries(term) {
        let single = !q.contains(' ');
        let first = search(q.clone(), PAGE, 0).await?;
        if first.total == 0 {
            continue;
        }
        if single && first.total > BROAD_TOKEN {
            if fallback
                .as_ref()
                .map(|(t, _)| first.total < *t)
                .unwrap_or(true)
            {
                fallback = Some((first.total, q.clone()));
            }
            continue;
        }
        let total = first.total;
        extend(&mut out, first.items);
        let mut skip = PAGE;
        while skip < total && skip < PAGES_PER_QUERY * PAGE && out.len() < MAX_CANDIDATES {
            let more = search(q.clone(), PAGE, skip).await?;
            extend(&mut out, more.items);
            skip += PAGE;
        }
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    if out.is_empty() {
        if let Some((_, q)) = fallback {
            let page = search(q, PAGE, 0).await?;
            extend(&mut out, page.items);
        }
    }
    Ok(out)
}

/// The candidates a job gathers for `term` from its registry snapshot, scored best first: the one
/// gathering VALIDATE and the evidence measurement share (plan 130 slice 6), so the measured gate is
/// the job's gate.
pub async fn gather(registry: &registry::Registry, term: &str) -> Result<Vec<Scored>> {
    let search = |q: String, limit: usize, skip: usize| {
        std::future::ready(Ok(registry.search(&q, limit, skip)))
    };
    let hits = candidates(&search, term).await?;
    Ok(score(term, &hits))
}

/// The seller's link decisions outside a job's proof — «not in Landau» and a cleared link — and the
/// search term rule the review screen shares with the automatic pass. Judging a link is
/// [`crate::jobs::proof::Prover`]'s.
pub struct Matcher {
    db: Db,
    supplier: String,
}

impl Matcher {
    pub fn new(db: Db, supplier: &str) -> Matcher {
        Matcher {
            db,
            supplier: supplier.into(),
        }
    }

    /// The search term for a name — exposed so the review screen's re-search uses the same rule.
    pub fn term(name: &str, code: &str) -> String {
        search_term(name, code)
    }

    pub fn record_none(&self, name_seen: &str, code: &str) -> Result<()> {
        self.db
            .set_match(&self.supplier, name_seen, code, None, "NONE", None)
    }

    pub fn clear(&self, name_seen: &str) -> Result<()> {
        self.db.clear_match(&self.supplier, name_seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landau::{Candidate, RegistryPage};
    use std::collections::HashMap;

    fn cand(id: &str, name: &str, alias: Option<&str>, score: f64) -> Candidate {
        Candidate {
            id: id.into(),
            name: name.into(),
            alias: alias.map(str::to_string),
            brand: None,
            score,
        }
    }

    /// A picker stub: query → (total, rows). Unknown queries answer nothing.
    fn picker(
        table: HashMap<&'static str, (usize, Vec<Candidate>)>,
    ) -> impl Fn(String, usize, usize) -> std::future::Ready<Result<RegistryPage>> {
        move |q, limit, skip| {
            let (total, rows) = table.get(q.as_str()).cloned().unwrap_or((0, Vec::new()));
            let items = rows.into_iter().skip(skip).take(limit).collect();
            std::future::ready(Ok(RegistryPage { total, items }))
        }
    }

    #[test]
    fn score_uses_the_best_of_name_and_aliases_and_sorts_desc() {
        let hits = vec![
            cand("a", "شمع موتور", None, 9.0),
            cand(
                "b",
                "لنت ترمز جلو پراید",
                Some("لنت جلو پراید | لنت پراید"),
                8.0,
            ),
            cand("c", "چیز دیگر", Some("لنت جلو پراید ABS"), 7.0),
        ];
        let scored = score("لنت جلو پراید", &hits);
        assert_eq!(scored[0].candidate.id, "b");
        assert_eq!(scored[0].similarity, 100);
        // "c"'s alias «لنت جلو پراید ABS» shares 3 of the union's 4 tokens with the term (Jaccard).
        assert_eq!(scored[1].similarity, 75);
        assert_eq!(scored[2].candidate.id, "a");
    }

    #[test]
    fn brand_in_the_invoice_name_is_matched_through_name_plus_brand() {
        let term = "لنت جلو پراید برند الف";
        // p1's name+brand is exactly the term → 100.
        let p1 = Candidate {
            id: "p1".into(),
            name: "لنت جلو پراید".into(),
            alias: None,
            brand: Some("برند الف".into()),
            score: 0.0,
        };
        // p2's name alone shares 3 of the term's 5 tokens (3/5 = 60), but its name+brand shares
        // "برند" too (the generic word) while swapping «الف» for «ب» — 4 of 6 union tokens (4/6 ≈
        // 66.7, rounds to 67) — the higher of the two facets, so that is `similarity_of`'s answer.
        let p2 = Candidate {
            id: "p2".into(),
            name: "لنت جلو پراید".into(),
            alias: None,
            brand: Some("برند ب".into()),
            score: 0.0,
        };
        assert_eq!(similarity_of(term, &p1), 100);
        assert_eq!(similarity_of(term, &p2), 67);
        assert_eq!(score(term, &[p2.clone(), p1.clone()])[0].candidate.id, "p1");

        // p3's name is unrelated; only its alias, folded with the matching brand, hits the term.
        let p3 = Candidate {
            id: "p3".into(),
            name: "قطعه نامرتبط".into(),
            alias: Some("لنت جلو پراید".into()),
            brand: Some("برند الف".into()),
            score: 0.0,
        };
        assert_eq!(similarity_of(term, &p3), 100);
    }

    #[test]
    fn queries_go_from_the_whole_term_to_bigrams_to_tokens() {
        assert_eq!(
            queries("سیبک فرمان پراید امیرنیا"),
            vec![
                "سیبک فرمان پراید امیرنیا",
                "سیبک فرمان",
                "فرمان پراید",
                "پراید امیرنیا",
                "سیبک",
                "فرمان",
                "پراید",
                "امیرنیا"
            ]
        );
        assert_eq!(queries("لنت"), vec!["لنت"]);
        assert_eq!(
            queries("  a  لنت "),
            vec!["a لنت", "لنت"],
            "one-character tokens are dropped"
        );
    }

    #[tokio::test]
    async fn candidates_union_specific_queries_and_skip_broad_tokens() {
        let table = HashMap::from([
            (
                "سیبک فرمان پراید",
                (
                    2,
                    vec![
                        cand("p1", "سیبک فرمان پراید", None, 0.0),
                        cand("p2", "سیبک فرمان پراید امیرنیا", None, 0.0),
                    ],
                ),
            ),
            (
                "سیبک فرمان",
                (
                    3,
                    vec![
                        cand("p1", "سیبک فرمان پراید", None, 0.0),
                        cand("p3", "سیبک فرمان پژو 405", None, 0.0),
                        cand("p2", "سیبک فرمان پراید امیرنیا", None, 0.0),
                    ],
                ),
            ),
            (
                "فرمان پراید",
                (1, vec![cand("p1", "سیبک فرمان پراید", None, 0.0)]),
            ),
            ("سیبک", (40, vec![cand("p9", "سیبک بغل شاسی", None, 0.0)])),
            ("پراید", (5297, vec![cand("p8", "آبگیر پراید", None, 0.0)])), // broader than BROAD_TOKEN → skipped
        ]);
        let found = candidates(&picker(table), "سیبک فرمان پراید")
            .await
            .unwrap();
        let ids: Vec<&str> = found.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["p1", "p2", "p3", "p9"],
            "unioned in query order, deduped, broad token skipped"
        );
    }

    #[tokio::test]
    async fn candidates_fall_back_to_the_least_broad_token_when_nothing_else_matches() {
        let table = HashMap::from([
            ("پراید", (5297, vec![cand("p8", "آبگیر پراید", None, 0.0)])),
            ("405", (5317, vec![cand("p7", "آبگیر 405", None, 0.0)])),
        ]);
        let found = candidates(&picker(table), "پراید 405").await.unwrap();
        assert_eq!(
            found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["p8"],
            "the less broad token's first page"
        );
        let none = candidates(&picker(HashMap::new()), "چیزی").await.unwrap();
        assert!(none.is_empty());
    }
}
