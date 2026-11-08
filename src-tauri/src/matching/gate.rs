//! Who may write without being asked — plan 130's write table (Q-2, Q-5, R-d, R-j) over the
//! evidence of [`super::evidence`]. Pure: the caller brings the evidence, the similarity and the
//! other candidates; nothing here reads or writes anything.

use serde::Serialize;

use super::evidence::{evidence, Evidence, State, SupplierBrand, Vocabulary};
use super::Scored;

/// The lowest similarity an automatic link may write at, whatever the setting says (Q-2, R-j).
pub const SIMILARITY_FLOOR: u8 = 85;

/// The similarity an automatic link needs: the seller's setting, never below the floor.
pub fn effective_threshold(setting: u8) -> u8 {
    setting.clamp(SIMILARITY_FLOOR, 100)
}

/// Why a link may not write on its own. Typed, so the review screen and the audit can say it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reason {
    BelowThreshold {
        similarity: u8,
        threshold: u8,
    },
    /// Another candidate scores the same.
    Tied,
    /// Another candidate passes the gate too.
    AnotherPasses {
        product_id: String,
    },
    NumberConflict {
        lacking: Vec<String>,
    },
    NumberUnbacked {
        candidate: Vec<String>,
    },
    WordsUncovered {
        words: Vec<String>,
    },
    OppositeWords {
        supplier: String,
        candidate: String,
    },
    PartWordsMissing {
        words: Vec<String>,
    },
    NoBrandInName,
    SeveralBrandsInName {
        brands: Vec<String>,
    },
    /// The only brand in the name is also a common word, and it is not the candidate's.
    BrandUnsure {
        supplier: String,
    },
    CandidateWithoutBrand,
    BrandMismatch {
        supplier: String,
        candidate: String,
    },
    /// A rebuilt link whose recorded name, brand and alias do not equal exactly one product.
    DefinitionNotExact,
}

/// Where a link to be written came from — the rows of the write table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A fresh automatic match, or a stored automatic link re-judged — never grandfathered.
    Auto,
    /// A stored automatic link whose dead product was rebuilt (Q-1).
    RebuiltAuto { identity_exact: bool },
    /// A stored seller pick whose product still exists.
    Pick,
    /// A stored seller pick whose dead product was rebuilt (Q-1, Q-5).
    RebuiltPick { identity_exact: bool },
    /// The seller picking now, in review (R-d).
    NewPick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "verdict",
    content = "reasons",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum Verdict {
    Write,
    Review(Vec<Reason>),
    /// A seller's pick against a conflict: written after a second confirm showing these.
    ConfirmThenWrite(Vec<Reason>),
}

/// One link judged: its evidence, its similarity, and what the other candidates say.
#[derive(Debug, Clone)]
pub struct Judged {
    pub similarity: u8,
    pub evidence: Evidence,
    pub tied: bool,
    /// The first other candidate that passes the gate too, if any.
    pub another_passes: Option<String>,
}

/// The V4 gate on one pair (Q-2), uniqueness aside: empty when it passes. A similarity of 100
/// passes nothing on its own — every dimension must agree.
pub fn v4_reasons(e: &Evidence, similarity: u8, threshold: u8) -> Vec<Reason> {
    let mut out = Vec::new();
    let threshold = effective_threshold(threshold);
    if similarity < threshold {
        out.push(Reason::BelowThreshold {
            similarity,
            threshold,
        });
    }
    // Q-2: no candidate number the supplier lacks — name or alias, whatever its kind.
    if e.numbers.state == State::Conflict {
        out.push(Reason::NumberConflict {
            lacking: e.numbers.lacking.clone(),
        });
    } else if !e.numbers.lacking.is_empty() {
        out.push(Reason::NumberUnbacked {
            candidate: e.numbers.lacking.clone(),
        });
    }
    for (supplier, candidate) in &e.words.opposites {
        out.push(Reason::OppositeWords {
            supplier: supplier.clone(),
            candidate: candidate.clone(),
        });
    }
    if !e.words.uncovered.is_empty() {
        out.push(Reason::WordsUncovered {
            words: e.words.uncovered.clone(),
        });
    }
    if !e.words.missing.is_empty() {
        out.push(Reason::PartWordsMissing {
            words: e.words.missing.clone(),
        });
    }
    if e.brand.state != State::Consistent {
        out.push(brand_reason(e));
    }
    out
}

fn number_reasons(e: &Evidence) -> Vec<Reason> {
    let mut out = Vec::new();
    if e.numbers.state == State::Conflict {
        out.push(Reason::NumberConflict {
            lacking: e.numbers.lacking.clone(),
        });
    }
    if e.numbers.unbacked {
        out.push(Reason::NumberUnbacked {
            candidate: e.numbers.lacking.clone(),
        });
    }
    out
}

/// Why the brand is not consistent — the most specific reason first.
fn brand_reason(e: &Evidence) -> Reason {
    match (&e.brand.supplier, &e.brand.candidate) {
        (SupplierBrand::None, _) => Reason::NoBrandInName,
        (SupplierBrand::Several { brands }, _) => Reason::SeveralBrandsInName {
            brands: brands.clone(),
        },
        (SupplierBrand::One { .. }, None) => Reason::CandidateWithoutBrand,
        (SupplierBrand::One { brand, ambiguous }, Some(candidate)) => {
            if *ambiguous {
                Reason::BrandUnsure {
                    supplier: brand.clone(),
                }
            } else {
                Reason::BrandMismatch {
                    supplier: brand.clone(),
                    candidate: candidate.clone(),
                }
            }
        }
    }
}

/// What reopens a seller's pick (Q-5): any dimension in conflict, or a number the supplier lacks.
/// What the seller merely did not see covered (a word, a brand the name does not state) does not.
pub fn conflicts(e: &Evidence) -> Vec<Reason> {
    let mut out = number_reasons(e);
    for (supplier, candidate) in &e.words.opposites {
        out.push(Reason::OppositeWords {
            supplier: supplier.clone(),
            candidate: candidate.clone(),
        });
    }
    if e.brand.state == State::Conflict {
        out.push(brand_reason(e));
    }
    out
}

/// The write table: whether this link may put a cost or a quantity on a shelf row now.
pub fn verdict(origin: Origin, j: &Judged, threshold: u8) -> Verdict {
    let review = |r: Vec<Reason>| {
        if r.is_empty() {
            Verdict::Write
        } else {
            Verdict::Review(r)
        }
    };
    let automatic = || {
        let mut r = v4_reasons(&j.evidence, j.similarity, threshold);
        if j.tied {
            r.push(Reason::Tied);
        }
        if let Some(other) = &j.another_passes {
            r.push(Reason::AnotherPasses {
                product_id: other.clone(),
            });
        }
        r
    };
    match origin {
        Origin::RebuiltAuto {
            identity_exact: false,
        }
        | Origin::RebuiltPick {
            identity_exact: false,
        } => Verdict::Review(vec![Reason::DefinitionNotExact]),
        Origin::Auto | Origin::RebuiltAuto { .. } => review(automatic()),
        Origin::Pick | Origin::RebuiltPick { .. } => review(conflicts(&j.evidence)),
        Origin::NewPick => {
            let r = conflicts(&j.evidence);
            if r.is_empty() {
                Verdict::Write
            } else {
                Verdict::ConfirmThenWrite(r)
            }
        }
    }
}

/// A fresh automatic match among scored candidates (best first): the top one, judged with the
/// tie and every other candidate that would pass as well. `None` when there is no candidate.
pub fn judge_top(
    name: &str,
    code: &str,
    scored: &[Scored],
    vocab: &Vocabulary,
    threshold: u8,
) -> Option<Judged> {
    let top = scored.first()?;
    let tied = scored
        .get(1)
        .is_some_and(|s| s.similarity == top.similarity && s.candidate.id != top.candidate.id);
    let floor = effective_threshold(threshold);
    let another_passes = scored[1..]
        .iter()
        .filter(|s| s.similarity >= floor && s.candidate.id != top.candidate.id)
        .find(|s| {
            v4_reasons(
                &evidence(name, code, &s.candidate, vocab),
                s.similarity,
                threshold,
            )
            .is_empty()
        })
        .map(|s| s.candidate.id.clone());
    Some(Judged {
        similarity: top.similarity,
        evidence: evidence(name, code, &top.candidate, vocab),
        tied,
        another_passes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::evidence::tests::{
        bearing_5e12, bearing_6c04, cand, vocab, MID_SHAFT, RIGHT_BEARING, WRONG_BEARING,
    };
    use crate::matching::{score, similarity_of};

    fn judged(item: (&str, &str), c: &crate::landau::Candidate, similarity: u8) -> Judged {
        Judged {
            similarity,
            evidence: evidence(item.1, item.0, c, &vocab()),
            tied: false,
            another_passes: None,
        }
    }

    fn reasons(v: &Verdict) -> Vec<Reason> {
        match v {
            Verdict::Write => Vec::new(),
            Verdict::Review(r) | Verdict::ConfirmThenWrite(r) => r.clone(),
        }
    }

    #[test]
    fn the_threshold_setting_cannot_go_below_85() {
        assert_eq!(effective_threshold(70), 85);
        assert_eq!(effective_threshold(85), 85);
        assert_eq!(effective_threshold(93), 93);
        assert_eq!(effective_threshold(120), 100);
    }

    #[test]
    fn an_automatic_link_writes_at_85_and_not_at_84() {
        let threshold = effective_threshold(70);
        let at = |s| {
            verdict(
                Origin::Auto,
                &judged(MID_SHAFT, &bearing_6c04(), s),
                threshold,
            )
        };
        assert_eq!(at(85), Verdict::Write);
        assert_eq!(
            at(84),
            Verdict::Review(vec![Reason::BelowThreshold {
                similarity: 84,
                threshold: 85
            }])
        );
    }

    #[test]
    fn a_tie_or_a_second_passing_candidate_holds_the_automatic_link() {
        let mut j = judged(MID_SHAFT, &bearing_6c04(), 90);
        j.tied = true;
        assert_eq!(
            verdict(Origin::Auto, &j, 85),
            Verdict::Review(vec![Reason::Tied])
        );
        j.tied = false;
        j.another_passes = Some("p2".into());
        assert_eq!(
            verdict(Origin::Auto, &j, 85),
            Verdict::Review(vec![Reason::AnotherPasses {
                product_id: "p2".into()
            }])
        );
    }

    #[test]
    fn judge_top_finds_ties_and_other_passing_candidates() {
        let v = vocab();
        let item = MID_SHAFT;
        let term = crate::matching::normalize::search_term(item.1, item.0);
        // A registry duplicate: same name, alias and brand — the same score.
        let twin = cand(
            "twin",
            &bearing_6c04().name,
            bearing_6c04().alias.as_deref(),
            Some("الما۷"),
        );
        let scored = score(&term, &[bearing_6c04(), twin]);
        let j = judge_top(item.1, item.0, &scored, &v, 85).expect("a top candidate");
        assert!(j.tied, "{j:?}");
        assert_eq!(
            verdict(Origin::Auto, &j, 85),
            Verdict::Review(vec![
                Reason::Tied,
                Reason::AnotherPasses {
                    product_id: "twin".into()
                }
            ])
        );
        // The same part under the equivalent Latin spelling scores higher and passes as well,
        // so the top one does not write either (PI-13's two-script brands).
        let latin = cand(
            "latin",
            &bearing_6c04().name,
            bearing_6c04().alias.as_deref(),
            Some("ELMA7"),
        );
        let scored = score(&term, &[bearing_6c04(), latin]);
        let j = judge_top(item.1, item.0, &scored, &v, 85).unwrap();
        assert_eq!(j.similarity, 100);
        assert_eq!(
            j.another_passes.as_deref(),
            Some("6aec98a4d00e9931421a6c04")
        );
        assert!(matches!(verdict(Origin::Auto, &j, 85), Verdict::Review(_)));

        let alone = score(&term, &[bearing_6c04()]);
        let j = judge_top(item.1, item.0, &alone, &v, 85).unwrap();
        assert!(!j.tied && j.another_passes.is_none());
        assert_eq!(j.similarity, similarity_of(&term, &bearing_6c04()));

        assert!(judge_top(item.1, item.0, &[], &v, 85).is_none());
    }

    #[test]
    fn no_brand_or_two_brands_in_the_name_hold_the_automatic_link() {
        let c = cand("c", "وایر شمع پراید", None, Some("FGP"));
        let none = judged(("2000", "وایر شمع پراید"), &c, 100);
        assert_eq!(
            reasons(&verdict(Origin::Auto, &none, 85)),
            vec![Reason::NoBrandInName]
        );
        let two = judged(("2001", "وایر شمع ساژم پراید FGP (پایا موتور)"), &c, 90);
        assert!(reasons(&verdict(Origin::Auto, &two, 85))
            .iter()
            .any(|r| matches!(r, Reason::SeveralBrandsInName { .. })));
        let brandless = cand("b", "وایر شمع پراید", None, None);
        let j = judged(("2002", "وایر شمع پراید FGP"), &brandless, 90);
        assert!(reasons(&verdict(Origin::Auto, &j, 85)).contains(&Reason::CandidateWithoutBrand));
    }

    #[test]
    fn an_unbacked_number_an_opposite_word_or_a_missing_part_word_hold_it() {
        let pump = cand("c", "پمپ بنزین 405", None, Some("عظام"));
        let j = judged(("5001", "پمپ بنزین پژو عظام"), &pump, 100);
        assert_eq!(
            reasons(&verdict(Origin::Auto, &j, 85)),
            vec![Reason::NumberUnbacked {
                candidate: vec!["405".into()]
            }]
        );

        let right = cand("r", "آینه بغل راست پراید", None, Some("کروز"));
        let j = judged(("4001", "آینه بغل چپ پراید کروز"), &right, 90);
        let r = reasons(&verdict(Origin::Auto, &j, 85));
        assert!(
            r.contains(&Reason::OppositeWords {
                supplier: "چپ".into(),
                candidate: "راست".into()
            }),
            "{r:?}"
        );

        let arm = cand("a", "طبق پژو 405 کامل", None, Some("عظام"));
        let j = judged(("3002", "بوش طبق پژو 405 کامل عظام"), &arm, 90);
        assert_eq!(
            reasons(&verdict(Origin::Auto, &j, 85)),
            vec![Reason::PartWordsMissing {
                words: vec!["بوش".into()]
            }]
        );
    }

    #[test]
    fn a_bushing_never_writes_as_a_bosch_part() {
        let starter = cand("s", "استارت پژو 405", None, Some("بوش"));
        let j = judged(("6001", "بوش استارت پژو 405"), &starter, 100);
        let r = reasons(&verdict(Origin::Auto, &j, 85));
        assert!(
            r.contains(&Reason::BrandUnsure {
                supplier: "بوش".into()
            }),
            "{r:?}"
        );
        assert!(
            r.iter()
                .any(|x| matches!(x, Reason::PartWordsMissing { .. })),
            "{r:?}"
        );
    }

    #[test]
    fn an_alias_number_the_supplier_lacks_holds_an_automatic_link() {
        let c = cand(
            "b",
            "بلبرینگ وسط پلوس پژو",
            Some("بلبرینگ 6005"),
            Some("ELMA7"),
        );
        let j = judged(("8002", "بلبرينگ وسط پلوس پژو ELMA7"), &c, 100);
        assert_eq!(
            reasons(&verdict(Origin::Auto, &j, 85)),
            vec![Reason::NumberUnbacked {
                candidate: vec!["6005".into()]
            }]
        );
        // A seller saw both names; an alias's number alone does not reopen the pick.
        assert_eq!(verdict(Origin::Pick, &j, 85), Verdict::Write);
    }

    #[test]
    fn a_conflict_holds_a_link_even_at_100() {
        let j = judged(WRONG_BEARING, &bearing_5e12(), 100);
        let r = reasons(&verdict(Origin::Auto, &j, 85));
        assert!(
            r.contains(&Reason::NumberConflict {
                lacking: vec!["44649".into()]
            }),
            "{r:?}"
        );
    }

    #[test]
    fn an_old_seller_pick_writes_unless_a_dimension_conflicts() {
        // The incident's two picks: the right item writes, the wrong one goes back to review.
        let right = judged(RIGHT_BEARING, &bearing_5e12(), 69);
        assert_eq!(verdict(Origin::Pick, &right, 85), Verdict::Write);
        let wrong = judged(WRONG_BEARING, &bearing_5e12(), 56);
        assert_eq!(
            verdict(Origin::Pick, &wrong, 85),
            Verdict::Review(vec![Reason::NumberConflict {
                lacking: vec!["44649".into()]
            }])
        );
        // Words merely uncovered are not a conflict: the seller saw the names.
        let words_only = cand("w", "بلبرینگ چرخ جلو پراید نو", None, Some("PSN عظام"));
        let j = judged(
            ("33102099", "بلبرينگ چرخ جلو پرايد PSN عظام"),
            &words_only,
            60,
        );
        assert_eq!(verdict(Origin::Pick, &j, 85), Verdict::Write);
        // The DPI case: the name says عظام.
        let dpi = cand("d", "لنت ترمز جلو پراید", None, Some("DPI"));
        let j = judged(("31370061", "لنت جلو پرايد عظام (20)"), &dpi, 75);
        assert_eq!(
            verdict(
                Origin::RebuiltPick {
                    identity_exact: true
                },
                &j,
                85
            ),
            Verdict::Review(vec![Reason::BrandMismatch {
                supplier: "عظام".into(),
                candidate: "DPI".into()
            }])
        );
    }

    #[test]
    fn a_rebuilt_link_needs_an_exact_definition() {
        let j = judged(RIGHT_BEARING, &bearing_5e12(), 69);
        assert_eq!(
            verdict(
                Origin::RebuiltPick {
                    identity_exact: true
                },
                &j,
                85
            ),
            Verdict::Write
        );
        assert_eq!(
            verdict(
                Origin::RebuiltPick {
                    identity_exact: false
                },
                &j,
                85
            ),
            Verdict::Review(vec![Reason::DefinitionNotExact])
        );
        let auto = judged(MID_SHAFT, &bearing_6c04(), 90);
        assert_eq!(
            verdict(
                Origin::RebuiltAuto {
                    identity_exact: true
                },
                &auto,
                85
            ),
            Verdict::Write
        );
        assert_eq!(
            verdict(
                Origin::RebuiltAuto {
                    identity_exact: false
                },
                &auto,
                85
            ),
            Verdict::Review(vec![Reason::DefinitionNotExact])
        );
        let low = judged(MID_SHAFT, &bearing_6c04(), 80);
        assert!(matches!(
            verdict(
                Origin::RebuiltAuto {
                    identity_exact: true
                },
                &low,
                85
            ),
            Verdict::Review(_)
        ));
    }

    #[test]
    fn a_new_pick_against_a_conflict_needs_a_second_confirm() {
        let wrong = judged(WRONG_BEARING, &bearing_5e12(), 56);
        assert_eq!(
            verdict(Origin::NewPick, &wrong, 85),
            Verdict::ConfirmThenWrite(vec![Reason::NumberConflict {
                lacking: vec!["44649".into()]
            }])
        );
        let right = judged(RIGHT_BEARING, &bearing_5e12(), 30);
        assert_eq!(verdict(Origin::NewPick, &right, 85), Verdict::Write);
    }

    #[test]
    fn reasons_serialise_for_the_screens() {
        let v = Verdict::Review(vec![Reason::BrandMismatch {
            supplier: "عظام".into(),
            candidate: "DPI".into(),
        }]);
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({"verdict": "REVIEW", "reasons": [
                {"kind": "BRAND_MISMATCH", "supplier": "عظام", "candidate": "DPI"}
            ]})
        );
        let _ = State::Consistent;
        let _ = SupplierBrand::None;
    }
}
