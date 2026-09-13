//! Part matching (spec §6 as amended by §13): candidates gathered from the seller's registry
//! picker (a substring search), scored by token-set similarity against the normalised invoice
//! name, auto-accepted at the threshold, every decision persisted.

pub mod normalize;
pub mod similarity;

use std::collections::HashSet;
use std::future::Future;

use serde::Serialize;

use crate::db::matches::MatchRow;
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

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Matched {
        product_id: String,
        product_name: String,
        similarity: Option<i64>,
        how: String,
    },
    /// A stored «هیچ‌کدام»: the part is not in Landau; a support ticket is the way forward.
    None,
    /// Needs the seller. Candidates best first; may be empty.
    Review(Vec<Scored>),
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

pub struct Matcher {
    db: Db,
    supplier: String,
    threshold: u8,
}

impl Matcher {
    pub fn new(db: Db, supplier: &str, threshold: u8) -> Matcher {
        Matcher {
            db,
            supplier: supplier.into(),
            threshold,
        }
    }

    pub fn stored(&self, code: &str) -> Result<Option<MatchRow>> {
        self.db.match_for(&self.supplier, code)
    }

    /// The search term for a name — exposed so the review screen's re-search uses the same rule.
    pub fn term(name: &str, code: &str) -> String {
        search_term(name, code)
    }

    pub async fn decide<F, Fut>(&self, search: &F, code: &str, name: &str) -> Result<Decision>
    where
        F: Fn(String, usize, usize) -> Fut,
        Fut: Future<Output = Result<RegistryPage>>,
    {
        if let Some(row) = self.stored(code)? {
            return Ok(match row.product_id {
                Some(product_id) => Decision::Matched {
                    product_id,
                    product_name: row.product_name.unwrap_or_default(),
                    similarity: row.similarity,
                    how: row.how,
                },
                None => Decision::None,
            });
        }
        let term = search_term(name, code);
        let hits = candidates(search, &term).await?;
        let scored = score(&term, &hits);
        let tied = scored.len() >= 2
            && scored[1].similarity == scored[0].similarity
            && scored[1].candidate.id != scored[0].candidate.id;
        if let Some(top) = scored.first() {
            if top.similarity >= self.threshold && !tied {
                self.db.set_match(
                    &self.supplier,
                    code,
                    Some((&top.candidate.id, &top.candidate.name)),
                    "AUTO",
                    Some(top.similarity as i64),
                    name,
                )?;
                return Ok(Decision::Matched {
                    product_id: top.candidate.id.clone(),
                    product_name: top.candidate.name.clone(),
                    similarity: Some(top.similarity as i64),
                    how: "AUTO".into(),
                });
            }
        }
        Ok(Decision::Review(
            scored.into_iter().take(CANDIDATES).collect(),
        ))
    }

    pub fn record_user_pick(
        &self,
        code: &str,
        name_seen: &str,
        product_id: &str,
        product_name: &str,
        similarity: Option<i64>,
    ) -> Result<()> {
        self.db.set_match(
            &self.supplier,
            code,
            Some((product_id, product_name)),
            "USER",
            similarity,
            name_seen,
        )
    }

    pub fn record_none(&self, code: &str, name_seen: &str) -> Result<()> {
        self.db
            .set_match(&self.supplier, code, None, "NONE", None, name_seen)
    }

    pub fn clear(&self, code: &str) -> Result<()> {
        self.db.clear_match(&self.supplier, code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
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

    #[tokio::test]
    async fn auto_accepts_at_threshold_and_persists_the_decision() {
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db.clone(), "barezpakhsh", 90);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = calls.clone();
        let search = move |q: String, _limit: usize, _skip: usize| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let items = if q == "لنت جلو پراید" {
                vec![
                    cand("p1", "لنت جلو پراید", None, 0.0),
                    cand("p2", "شمع", None, 0.0),
                ]
            } else {
                Vec::new()
            };
            let total = items.len();
            std::future::ready(Ok(RegistryPage { total, items }))
        };
        let d = m
            .decide(&search, "111", "لنت جلو پرايد (4 عددي)")
            .await
            .unwrap();
        assert!(
            matches!(d, Decision::Matched { ref product_id, similarity: Some(100), ref how, .. } if product_id == "p1" && how == "AUTO")
        );
        let first = calls.load(std::sync::atomic::Ordering::SeqCst);
        assert!(first >= 1);
        // second call answers from the table, no search
        let d2 = m.decide(&search, "111", "anything").await.unwrap();
        assert!(matches!(d2, Decision::Matched { .. }));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), first);
    }

    #[tokio::test]
    async fn a_tie_between_two_candidates_goes_to_review() {
        // Same name under two brands; the invoice names no brand, so nothing breaks the tie.
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db, "barezpakhsh", 90);
        let search = |q: String, _: usize, _: usize| {
            let items = if q == "لنت جلو پراید" {
                vec![
                    Candidate {
                        id: "p1".into(),
                        name: "لنت جلو پراید".into(),
                        alias: None,
                        brand: Some("برند الف".into()),
                        score: 0.0,
                    },
                    Candidate {
                        id: "p2".into(),
                        name: "لنت جلو پراید".into(),
                        alias: None,
                        brand: Some("برند ب".into()),
                        score: 0.0,
                    },
                ]
            } else {
                Vec::new()
            };
            let total = items.len();
            std::future::ready(Ok(RegistryPage { total, items }))
        };
        let d = m.decide(&search, "444", "لنت جلو پراید").await.unwrap();
        let Decision::Review(scored) = d else {
            panic!("expected review, both candidates tie at 100")
        };
        assert_eq!(scored[0].similarity, 100);
        assert_eq!(scored[1].similarity, 100);
    }

    #[tokio::test]
    async fn below_threshold_goes_to_review_and_none_sticks() {
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db.clone(), "barezpakhsh", 95);
        let search = |_: String, _: usize, _: usize| {
            std::future::ready(Ok(RegistryPage {
                total: 1,
                items: vec![cand("p1", "لنت جلو پژو 405", None, 0.0)],
            }))
        };
        let d = m.decide(&search, "222", "لنت جلو پراید").await.unwrap();
        let Decision::Review(scored) = d else {
            panic!("expected review")
        };
        assert_eq!(scored[0].candidate.id, "p1");
        assert!(scored[0].similarity < 95);

        m.record_none("222", "لنت جلو پراید").unwrap();
        assert!(matches!(
            m.decide(&search, "222", "x").await.unwrap(),
            Decision::None
        ));

        m.record_user_pick("222", "لنت جلو پراید", "p9", "picked", Some(70))
            .unwrap();
        assert!(
            matches!(m.decide(&search, "222", "x").await.unwrap(), Decision::Matched { ref how, .. } if how == "USER")
        );

        m.clear("222").unwrap();
        assert!(matches!(
            m.decide(&search, "222", "لنت جلو پراید").await.unwrap(),
            Decision::Review(_)
        ));
    }

    #[tokio::test]
    async fn no_hits_is_a_review_with_no_candidates() {
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db, "barezpakhsh", 80);
        let search = |_: String, _: usize, _: usize| {
            std::future::ready(Ok(RegistryPage {
                total: 0,
                items: Vec::new(),
            }))
        };
        assert!(
            matches!(m.decide(&search, "3", "چیزی").await.unwrap(), Decision::Review(ref v) if v.is_empty())
        );
    }
}
