//! Evidence about one supplier item and one Landau product — plan 130's model as pure functions.
//!
//! Similarity says how alike two strings are; evidence says something about *this* pair. Three
//! dimensions — numbers, words, brand — each answer consistent, conflict or insufficient, and the
//! write table ([`super::gate`]) decides who may write on which answers. Nothing here touches the
//! network, the database or a clock: the registry's vocabulary comes in as a value.
//!
//! The rulings behind the details (plan 130, *Execution slice 1*): plain numbers and measures
//! (a decimal or a centimetre/millimetre value) are two kinds, and only a kind both sides state can
//! conflict; digits glued to letters («XU7», «6PK1575», «L90») are a code, compared only with a code
//! of the same letters; `x/y` of one or two digits each is a decimal; brand spellings compare without
//! spaces, and one brand inside another is related, not equal; a registry brand that is also a common
//! name word («بوش», the bushing) is ambiguous; unit words and dimension labels need no covering. The
//! tables are data files under `data/evidence/`, each line reviewed.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::normalize::{normalize_text, without_own_code, PACK_BARE, PACK_PAREN};
use crate::landau::Candidate;

/// A word or brand used in at least this many registry names is vocabulary, not noise.
pub const NAME_WORD_MIN: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    Consistent,
    Conflict,
    Insufficient,
}

// ---------------------------------------------------------------------------------------------
// Tables

const IMPLIED: &str = include_str!("../../data/evidence/implied.tsv");
const OPPOSITES: &str = include_str!("../../data/evidence/opposites.tsv");
const BRAND_EQUIVALENCE: &str = include_str!("../../data/evidence/brand-equivalence.tsv");
const STOP_WORDS: &str = include_str!("../../data/evidence/stop-words.tsv");

struct Tables {
    /// `(trigger, implied word)`: the trigger in a name puts the word in it too.
    implied: Vec<(String, String)>,
    opposites: Vec<(String, String)>,
    /// Every listed spelling, spaces removed → its group's first spelling, as tokens.
    brand_class: HashMap<String, Vec<String>>,
    /// Every listed spelling, as written in the file (for the vocabulary's findable brands).
    brand_spellings: Vec<String>,
    stop: HashSet<String>,
}

/// Non-comment lines of a table, split on tabs, each cell trimmed.
fn rows(text: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            l.split('\t')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect()
        })
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let pair = |r: Vec<&str>| match r.as_slice() {
        [a, b, ..] => Some((key(a), key(b))),
        _ => None,
    };
    let mut brand_class = HashMap::new();
    let mut brand_spellings = Vec::new();
    for group in rows(BRAND_EQUIVALENCE) {
        let first = coarse(group[0]);
        for spelling in group {
            brand_class.insert(coarse(spelling).concat(), first.clone());
            brand_spellings.push(spelling.to_string());
        }
    }
    Tables {
        implied: rows(IMPLIED).filter_map(pair).collect(),
        opposites: rows(OPPOSITES).filter_map(pair).collect(),
        brand_class,
        brand_spellings,
        stop: rows(STOP_WORDS).map(|r| key(r[0])).collect(),
    }
});

/// A brand as compared: the tokens of its equivalence group's first spelling, else its own.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BrandClass(Vec<String>);

impl BrandClass {
    fn of(tokens: &[String]) -> BrandClass {
        match TABLES.brand_class.get(&tokens.concat()) {
            Some(first) => BrandClass(first.clone()),
            None => BrandClass(tokens.to_vec()),
        }
    }

    fn sorted(&self) -> Vec<&str> {
        let mut t: Vec<&str> = self.0.iter().map(String::as_str).collect();
        t.sort_unstable();
        t
    }

    /// One brand however the registry spaced or ordered it («نیکو پخش» and «نیکوپخش»,
    /// «تکنیک مدرن ماهان» and «ماهان (تکنیک مدرن)»).
    fn same(&self, other: &BrandClass) -> bool {
        self.0.concat() == other.0.concat() || self.sorted() == other.sorted()
    }

    /// One brand's words inside the other's («کروز» in «کروز پلاس», «عظام» in «PSN عظام»): a product
    /// line or a maker's longer name — not proof of the same brand, and not proof of another. Whole
    /// words only: «NT» is not inside «NTN».
    fn related(&self, other: &BrandClass) -> bool {
        let subset = |x: &BrandClass, y: &BrandClass| x.0.iter().all(|t| y.0.contains(t));
        !self.same(other) && (subset(self, other) || subset(other, self))
    }
}

// ---------------------------------------------------------------------------------------------
// Tokens

/// Kashida and the Arabic vowel marks — they change no word.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{0640}' | '\u{064B}'..='\u{065F}' | '\u{0670}')
}

/// Coarse tokens: glyphs and digits unified, lower-cased, split on whitespace and punctuation —
/// except a `.` or `/` between two digits, a decimal point here («7/6» is 7.6 in these names).
/// Digits glued to Persian letters are split off («206تیپ», «75عظام»: a missing space); glued to
/// Latin letters they stay one token, a code («XU7», «6PK1575»).
pub fn coarse(s: &str) -> Vec<String> {
    let text = normalize_text(s).to_lowercase();
    // «ى» and «ھ» are folded here, not in `normalize_text`: that one keys the stored links.
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !is_mark(*c))
        .map(|c| match c {
            'ى' => 'ی',
            'ھ' => 'ه',
            other => other,
        })
        .collect();
    let persian = |c: char| c.is_alphabetic() && !c.is_ascii();
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let decimal = (c == '.' || c == '/')
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        if c.is_alphanumeric() || decimal {
            let seam = cur.chars().last().is_some_and(|p| {
                (persian(p) && c.is_ascii_digit()) || (p.is_ascii_digit() && persian(c))
            });
            if seam {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The comparison key of a word or phrase: its coarse tokens, space-joined.
pub fn key(s: &str) -> String {
    coarse(s).join(" ")
}

/// A non-negative decimal, normalised so equal values compare equal (`7.60` = `7.6`, `0631` = `631`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Decimal {
    mantissa: u128,
    scale: u32,
}

impl Decimal {
    /// Digits with at most one `.` between them; `None` past 30 digits.
    fn parse(lit: &str) -> Option<Decimal> {
        let (int, frac) = lit.split_once('.').unwrap_or((lit, ""));
        if frac.contains('.') || int.len() + frac.len() > 30 || int.len() + frac.len() == 0 {
            return None;
        }
        let mantissa = format!("{int}{frac}").parse::<u128>().ok()?;
        Some(Decimal::normal(mantissa, frac.len() as u32))
    }

    fn normal(mut mantissa: u128, mut scale: u32) -> Decimal {
        while scale > 0 && mantissa.is_multiple_of(10) {
            mantissa /= 10;
            scale -= 1;
        }
        Decimal { mantissa, scale }
    }

    fn times10(self) -> Decimal {
        if self.scale > 0 {
            Decimal::normal(self.mantissa, self.scale - 1)
        } else {
            Decimal {
                mantissa: self.mantissa * 10,
                scale: 0,
            }
        }
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let digits = self.mantissa.to_string();
        let scale = self.scale as usize;
        if scale == 0 {
            return f.write_str(&digits);
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (int, frac) = padded.split_at(padded.len() - scale);
        write!(f, "{int}.{frac}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Unit {
    None,
    Cm,
    Mm,
}

/// What a number says: a plain integer (a part number, a model, a count) or a measure (a decimal,
/// or a value with a length unit). A conflict needs both sides to state the same kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Plain,
    Measure,
}

/// A number stated in a name, with the length unit written after it, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Num {
    pub value: Decimal,
    pub unit: Unit,
    /// Written with a decimal point («3.00» is a measure though its value is whole).
    pub decimal: bool,
}

impl Num {
    pub fn kind(&self) -> Kind {
        if self.unit != Unit::None || self.decimal {
            Kind::Measure
        } else {
            Kind::Plain
        }
    }

    /// Two statements of one value: equal in one unit, a unitless number equal to the value in any
    /// unit or to ten times a centimetre value («370» ≡ «37 سانتی»), centimetres ×10 = millimetres.
    pub fn same(&self, other: &Num) -> bool {
        same_value(self.value, self.unit, other.value, other.unit)
    }

    fn label(&self) -> String {
        match self.unit {
            Unit::None => self.value.to_string(),
            Unit::Cm => format!("{} cm", self.value),
            Unit::Mm => format!("{} mm", self.value),
        }
    }
}

fn same_value(a: Decimal, ua: Unit, b: Decimal, ub: Unit) -> bool {
    use Unit::*;
    match (ua, ub) {
        (x, y) if x == y => a == b,
        (None, Cm) => a == b || a == b.times10(),
        (Cm, None) => same_value(b, ub, a, ua),
        (None, Mm) | (Mm, None) => a == b,
        (Cm, Mm) => a.times10() == b,
        (Mm, Cm) => same_value(b, ub, a, ua),
        _ => false,
    }
}

/// Digits glued to Latin letters in one token — an engine, model or catalogue code («XU7»,
/// «6PK1575», «L90»). Compared whole, and only with a code of the same shape: «6PK1220» against
/// «6PK1575» conflicts, «XU7» against a name that names no `xu#` code says nothing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Code {
    pub text: String,
    family: String,
    digits: String,
}

/// Unit words, as coarse-token sequences, longest first so «میلی متر» wins over «میلی».
const CM: &[&[&str]] = &[
    &["سانتی", "متر"],
    &["سانتیمتر"],
    &["سانتی"],
    &["سانت"],
    &["cm"],
];
/// «میل» alone is not here: after a number it is as often the rod of «میل لنگ» as a millimetre.
const MM: &[&[&str]] = &[&["میلی", "متر"], &["میلیمتر"], &["میلی"], &["mm"]];

enum Piece {
    Word(String),
    Num(Num),
    /// A code, with the numbers its digits read as (they back the other side's numbers only).
    Code(Code, Vec<Num>),
}

/// The numbers a run of digits states: one, a decimal, or — with several separators, or `x/y`
/// longer than two digits a side («206/207») — each part on its own.
fn literal(text: &str) -> Vec<Num> {
    let num = |value, decimal| Num {
        value,
        unit: Unit::None,
        decimal,
    };
    let ints = |t: &str| -> Vec<Num> {
        t.split(['.', '/'])
            .filter_map(Decimal::parse)
            .map(|d| num(d, false))
            .collect()
    };
    let decimal = |t: &str| -> Vec<Num> {
        Decimal::parse(t)
            .map(|d| vec![num(d, true)])
            .unwrap_or_default()
    };
    match text.matches(['.', '/']).count() {
        0 => ints(text),
        1 if text.contains('.') => decimal(text),
        1 => {
            // One reading only: «4/8» is 4.8, never also 8.4.
            let (a, b) = text.split_once('/').unwrap_or((text, ""));
            if a.len() <= 2 && b.len() <= 2 {
                decimal(&format!("{a}.{b}"))
            } else {
                ints(text)
            }
        }
        _ => ints(text),
    }
}

/// Split coarse tokens into words, numbers and codes («cw3.6» → the word `cw` and the code).
fn pieces(tokens: &[String]) -> Vec<Piece> {
    let mut out = Vec::new();
    for t in tokens {
        let mut runs: Vec<(bool, String)> = Vec::new();
        for c in t.chars() {
            let digit = c.is_ascii_digit() || c == '.' || c == '/';
            match runs.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => runs.push((digit, c.to_string())),
            }
        }
        let has_digits = runs.iter().any(|r| r.0);
        let has_letters = runs.iter().any(|r| !r.0);
        let unit_suffix = runs.len() == 2
            && runs[0].0
            && [CM, MM].iter().any(|table| {
                table
                    .iter()
                    .any(|seq| seq.len() == 1 && seq[0] == runs[1].1)
            });
        if !has_letters {
            out.extend(literal(t).into_iter().map(Piece::Num));
        } else if !has_digits {
            out.push(Piece::Word(t.clone()));
        } else if unit_suffix {
            // «380mm», «37cm»: a number and its unit written together.
            out.extend(literal(&runs[0].1).into_iter().map(Piece::Num));
            out.push(Piece::Word(runs[1].1.clone()));
        } else {
            // The code's shape: its letters with each digit run as `#` («6pk1575» → `#pk#`).
            let family: String = runs
                .iter()
                .map(|r| if r.0 { "#" } else { r.1.as_str() })
                .collect();
            let digit_runs: Vec<&str> = runs.iter().filter(|r| r.0).map(|r| r.1.as_str()).collect();
            let nums: Vec<Num> = digit_runs.iter().flat_map(|d| literal(d)).collect();
            out.extend(
                runs.iter()
                    .filter(|r| !r.0)
                    .map(|r| Piece::Word(r.1.clone())),
            );
            out.push(Piece::Code(
                Code {
                    text: t.clone(),
                    family,
                    digits: digit_runs.join("|"),
                },
                nums,
            ));
        }
    }
    out
}

/// What a text states: its numbers (with units), its codes, the numbers its codes' digits read as,
/// and its words (unit words consumed, stop words dropped) — order kept, duplicates removed.
#[derive(Debug, Default, Clone)]
struct Reading {
    nums: Vec<Num>,
    codes: Vec<Code>,
    code_nums: Vec<Num>,
    words: Vec<String>,
}

fn push_new<T: PartialEq>(v: &mut Vec<T>, x: T) {
    if !v.contains(&x) {
        v.push(x);
    }
}

fn read(tokens: &[String]) -> Reading {
    let ps = pieces(tokens);
    let mut r = Reading::default();
    let word_at = |j: usize| match ps.get(j) {
        Some(Piece::Word(w)) => Some(w.as_str()),
        _ => None,
    };
    let unit_at = |j: usize, table: &[&[&str]]| {
        table
            .iter()
            .find(|seq| {
                seq.iter()
                    .enumerate()
                    .all(|(k, w)| word_at(j + k) == Some(w))
            })
            .map(|seq| seq.len())
    };
    let mut i = 0;
    while i < ps.len() {
        match &ps[i] {
            Piece::Num(n) => {
                let (unit, used) = if let Some(k) = unit_at(i + 1, CM) {
                    (Unit::Cm, k)
                } else if let Some(k) = unit_at(i + 1, MM) {
                    (Unit::Mm, k)
                } else {
                    (Unit::None, 0)
                };
                push_new(&mut r.nums, Num { unit, ..*n });
                i += 1 + used;
            }
            Piece::Code(code, nums) => {
                push_new(&mut r.codes, code.clone());
                for n in nums {
                    push_new(&mut r.code_nums, *n);
                }
                i += 1;
            }
            Piece::Word(w) => {
                if !TABLES.stop.contains(w) {
                    push_new(&mut r.words, w.clone());
                }
                i += 1;
            }
        }
    }
    r
}

/// The words a set of tokens implies through the table («405» ⇒ «پژو»).
fn implied(r: &Reading) -> HashSet<String> {
    let present: HashSet<String> = r
        .words
        .iter()
        .cloned()
        .chain(r.nums.iter().map(|n| n.value.to_string()))
        .collect();
    TABLES
        .implied
        .iter()
        .filter(|(trigger, _)| present.contains(trigger))
        .map(|(_, word)| word.clone())
        .collect()
}

/// `tokens` with every contiguous occurrence of `phrase` removed.
fn without(tokens: &[String], phrase: &[String]) -> Vec<String> {
    if phrase.is_empty() {
        return tokens.to_vec();
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i..].starts_with(phrase) {
            i += phrase.len();
        } else {
            out.push(tokens[i].clone());
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Vocabulary

#[derive(Debug, Clone)]
struct BrandSpelling {
    class: BrandClass,
    shown: String,
    ambiguous: bool,
}

/// What the evidence needs from the whole registry: every brand (with the equivalence table's
/// spellings), which brands are also common name words, and the words of its names.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    brands: HashMap<Vec<String>, BrandSpelling>,
    longest_brand: usize,
    part_words: HashSet<String>,
}

impl Vocabulary {
    /// Built from `(name, brand)` of every registry definition.
    pub fn build<'a, I>(products: I) -> Vocabulary
    where
        I: IntoIterator<Item = (&'a str, Option<&'a str>)>,
    {
        let products: Vec<(Vec<String>, Option<&str>)> = products
            .into_iter()
            .map(|(name, brand)| (coarse(name), brand.filter(|b| !b.trim().is_empty())))
            .collect();

        let mut spellings: HashMap<Vec<String>, String> = HashMap::new();
        let table = TABLES.brand_spellings.iter().map(String::as_str);
        for b in products.iter().filter_map(|(_, b)| *b).chain(table) {
            let toks = coarse(b);
            if !toks.is_empty() {
                spellings
                    .entry(toks)
                    .or_insert_with(|| b.trim().to_string());
            }
        }
        let longest = spellings.keys().map(Vec::len).max().unwrap_or(0);

        // Names a brand phrase or a word appears in, each name counted once.
        let mut brand_names: HashMap<&[String], usize> = HashMap::new();
        let mut word_names: HashMap<String, usize> = HashMap::new();
        for (toks, brand) in &products {
            // A brand named inside its own products' names is still just a brand; only other
            // products using it as a word make it ambiguous.
            let own = brand.map(|b| BrandClass::of(&coarse(b)));
            let mut seen: HashSet<&[String]> = HashSet::new();
            for n in 1..=longest.min(toks.len()) {
                for w in toks.windows(n) {
                    if let Some((k, _)) = spellings.get_key_value(w) {
                        if !own.as_ref().is_some_and(|o| o.same(&BrandClass::of(k))) {
                            seen.insert(k.as_slice());
                        }
                    }
                }
            }
            for k in seen {
                *brand_names.entry(k).or_default() += 1;
            }
            let words: BTreeSet<String> = pieces(toks)
                .into_iter()
                .filter_map(|p| match p {
                    Piece::Word(w) => Some(w),
                    Piece::Num(_) | Piece::Code(..) => None,
                })
                .collect();
            for w in words {
                *word_names.entry(w).or_default() += 1;
            }
        }

        let brands: HashMap<Vec<String>, BrandSpelling> = spellings
            .iter()
            .map(|(toks, shown)| {
                let ambiguous =
                    brand_names.get(toks.as_slice()).copied().unwrap_or(0) >= NAME_WORD_MIN;
                let spelling = BrandSpelling {
                    class: BrandClass::of(toks),
                    shown: shown.clone(),
                    ambiguous,
                };
                (toks.clone(), spelling)
            })
            .collect();
        // A word that is a brand and nothing else is not a part word; «بوش» (also the bushing) is.
        let plain_brand_words: HashSet<&str> = brands
            .iter()
            .filter(|(toks, s)| toks.len() == 1 && !s.ambiguous)
            .map(|(toks, _)| toks[0].as_str())
            .collect();
        let part_words = word_names
            .into_iter()
            .filter(|(w, n)| *n >= NAME_WORD_MIN && !plain_brand_words.contains(w.as_str()))
            .map(|(w, _)| w)
            .collect();
        Vocabulary {
            brands,
            longest_brand: longest,
            part_words,
        }
    }

    pub fn is_part_word(&self, w: &str) -> bool {
        self.part_words.contains(w) && !TABLES.stop.contains(w)
    }

    /// Every brand phrase in `tokens`, longest first and never overlapping («PSN عظام» over «عظام»).
    fn brands_in(&self, tokens: &[String]) -> Vec<FoundBrand> {
        let mut all: Vec<FoundBrand> = Vec::new();
        for n in 1..=self.longest_brand.min(tokens.len()) {
            for (start, w) in tokens.windows(n).enumerate() {
                if let Some(s) = self.brands.get(w) {
                    all.push(FoundBrand {
                        start,
                        len: n,
                        class: s.class.clone(),
                        shown: s.shown.clone(),
                        ambiguous: s.ambiguous,
                    });
                }
            }
        }
        let chars = |f: &FoundBrand| -> usize {
            tokens[f.start..f.start + f.len]
                .iter()
                .map(|t| t.chars().count())
                .sum()
        };
        all.sort_by(|a, b| {
            b.len
                .cmp(&a.len)
                .then(chars(b).cmp(&chars(a)))
                .then(a.start.cmp(&b.start))
        });
        let mut taken: Vec<FoundBrand> = Vec::new();
        for f in all {
            let overlaps = taken
                .iter()
                .any(|t| f.start < t.start + t.len && t.start < f.start + f.len);
            if !overlaps {
                taken.push(f);
            }
        }
        taken.sort_by_key(|f| f.start);
        taken
    }
}

#[derive(Debug, Clone)]
struct FoundBrand {
    start: usize,
    len: usize,
    class: BrandClass,
    shown: String,
    ambiguous: bool,
}

// ---------------------------------------------------------------------------------------------
// Evidence

/// What the supplier name says its brand is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierBrand {
    /// One brand. `ambiguous`: the brand is also a common name word, so the name may not mean it.
    One {
        brand: String,
        ambiguous: bool,
    },
    /// Two or more brands — the name does not say which.
    Several {
        brands: Vec<String>,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Numbers {
    pub state: State,
    /// The candidate's name states a number of a kind the supplier name states none of.
    pub unbacked: bool,
    /// Numbers and codes, as each side states them.
    pub supplier: Vec<String>,
    pub candidate: Vec<String>,
    pub shared: Vec<String>,
    /// Candidate numbers and codes the supplier name does not state.
    pub lacking: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Words {
    pub state: State,
    /// Words of the candidate's name the supplier name does not have, even implied.
    pub uncovered: Vec<String>,
    /// `(supplier word, candidate word)` pairs of opposites.
    pub opposites: Vec<(String, String)>,
    /// Part words of the supplier name the candidate does not have.
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brand {
    pub state: State,
    pub supplier: SupplierBrand,
    pub candidate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub numbers: Numbers,
    pub words: Words,
    pub brand: Brand,
}

impl Evidence {
    pub fn any_conflict(&self) -> bool {
        [self.numbers.state, self.words.state, self.brand.state].contains(&State::Conflict)
    }
}

/// The evidence a supplier item (`name`, `code`) and a registry candidate give about each other.
pub fn evidence(name: &str, code: &str, candidate: &Candidate, vocab: &Vocabulary) -> Evidence {
    static ANGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<\s*\d{1,3}\s*>").unwrap());

    // Supplier: «<N>» and a trailing copy of its own code are not part of the part. A bracketed or
    // «N عددی» number may be a carton count: it backs a candidate's number, and states its kind only
    // when it has three digits or more («(855)» a belt, «(6006)» a bearing; «(96)» a carton).
    let base = without_own_code(name, code);
    let mut pack_nums: Vec<Num> = Vec::new();
    let mut pack_stating: Vec<Num> = Vec::new();
    for cap in PACK_PAREN.captures_iter(&base) {
        let nums = literal(&cap[1]);
        if cap[1].len() >= 3 {
            pack_stating.extend(nums.iter().copied());
        }
        pack_nums.extend(nums);
    }
    for cap in PACK_BARE.captures_iter(&base) {
        pack_nums.extend(literal(&cap[2]));
    }
    let base = PACK_PAREN.replace_all(&base, " ");
    let base = PACK_BARE.replace_all(&base, "$1 ");
    let s_tokens = coarse(&ANGLE.replace_all(&base, " "));

    // Candidate: its own brand (any spelling of it) is not a word its name must have covered.
    let c_brand = candidate
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let c_class = c_brand.map(|b| BrandClass::of(&coarse(b)));
    let strip_own = |tokens: Vec<String>| -> Vec<String> {
        let Some(class) = &c_class else {
            return tokens;
        };
        let own: Vec<Vec<String>> = vocab
            .brands
            .iter()
            .filter(|(_, s)| s.class.same(class))
            .map(|(t, _)| t.clone())
            .chain(c_brand.map(coarse))
            .collect();
        own.iter().fold(tokens, |acc, phrase| without(&acc, phrase))
    };
    let mut c_name = read(&strip_own(coarse(&candidate.name)));
    // A code in the candidate's own name carries its part number in its digits («6205ZZ»): three
    // digits or more are a plain number of the name too. Short runs («XU7», «L90») stay inside
    // the code, and so does every alias code, which names other models.
    let part_numbers: Vec<Num> = c_name
        .code_nums
        .iter()
        .filter(|n| !n.decimal && n.value.scale == 0 && n.value.mantissa >= 100)
        .copied()
        .collect();
    for n in part_numbers {
        push_new(&mut c_name.nums, n);
    }
    let mut c_all = c_name.clone();
    for alias in candidate.alias.as_deref().unwrap_or("").split('|') {
        let a = read(&strip_own(coarse(alias)));
        for n in a.nums {
            push_new(&mut c_all.nums, n);
        }
        for c in a.codes {
            push_new(&mut c_all.codes, c);
        }
        for w in a.words {
            push_new(&mut c_all.words, w);
        }
    }

    // Brand.
    let found = vocab.brands_in(&s_tokens);
    let classes = |ambiguous: bool| -> Vec<&FoundBrand> {
        let mut distinct: Vec<&FoundBrand> = Vec::new();
        for f in found.iter().filter(|f| f.ambiguous == ambiguous) {
            if !distinct.iter().any(|d| d.class.same(&f.class)) {
                distinct.push(f);
            }
        }
        distinct
    };
    let (plain, unsure) = (classes(false), classes(true));
    let chosen: Option<&FoundBrand> = match (plain.len(), unsure.len()) {
        (1, _) => Some(plain[0]),
        (0, 1) => Some(unsure[0]),
        _ => None,
    };
    let supplier_brand = match chosen {
        Some(f) => SupplierBrand::One {
            brand: f.shown.clone(),
            ambiguous: f.ambiguous,
        },
        None if plain.len() + unsure.len() == 0 => SupplierBrand::None,
        None => SupplierBrand::Several {
            brands: if plain.len() >= 2 { &plain } else { &unsure }
                .iter()
                .map(|f| f.shown.clone())
                .collect(),
        },
    };
    // An ambiguous brand proves nothing either way: «بوش استارت» is a starter bushing, not a
    // Bosch starter. It stays one of the name's words, so the bushing is still missed if absent.
    let brand_state = match (chosen, &c_class) {
        (Some(f), Some(c)) if !f.ambiguous && f.class.same(c) => State::Consistent,
        (Some(f), Some(c)) if !f.ambiguous && !f.class.related(c) => State::Conflict,
        _ => State::Insufficient,
    };
    let s_word_tokens: Vec<String> = match chosen {
        Some(f) if !f.ambiguous => {
            let mut toks = Vec::with_capacity(s_tokens.len());
            for (i, t) in s_tokens.iter().enumerate() {
                let in_brand = found
                    .iter()
                    .any(|g| g.class.same(&f.class) && i >= g.start && i < g.start + g.len);
                if !in_brand {
                    toks.push(t.clone());
                }
            }
            toks
        }
        _ => s_tokens.clone(),
    };
    let s = read(&s_word_tokens);

    // Numbers. The candidate's are its name's and its aliases' together; the supplier's codes back
    // a number with their digits («CW7.6» backs «7.6 میلی متر») and state a measure, never a plain
    // number — a belt code's digits are not the car it fits.
    let supplier_nums: Vec<Num> = s
        .nums
        .iter()
        .chain(&s.code_nums)
        .chain(&pack_nums)
        .copied()
        .collect();
    let backed = |c: &Num| supplier_nums.iter().any(|x| x.same(c));
    let states = |k: Kind| {
        s.nums.iter().chain(&pack_stating).any(|x| x.kind() == k)
            || (k == Kind::Measure && s.code_nums.iter().any(|x| x.kind() == k))
    };
    let mut shared: Vec<String> = c_all
        .nums
        .iter()
        .filter(|c| backed(c))
        .map(Num::label)
        .collect();
    let lacking_nums: Vec<&Num> = c_all.nums.iter().filter(|c| !backed(c)).collect();
    let mut conflict = lacking_nums.iter().any(|c| states(c.kind()));
    let mut lacking: Vec<String> = lacking_nums.iter().map(|c| c.label()).collect();
    for code in &c_all.codes {
        let family: Vec<&Code> = s.codes.iter().filter(|x| x.family == code.family).collect();
        if family.iter().any(|x| x.digits == code.digits) {
            shared.push(code.text.clone());
        } else if !family.is_empty() {
            lacking.push(code.text.clone());
            conflict = true;
        }
    }
    // Unbacked: the candidate's own name states a number of a kind the supplier name states none
    // of. An alias's numbers never do this alone — aliases list other models and other codes.
    let unbacked = !conflict && c_name.nums.iter().any(|c| !backed(c));
    let numbers_state = if conflict {
        State::Conflict
    } else if !shared.is_empty() && lacking.is_empty() {
        State::Consistent
    } else {
        State::Insufficient
    };
    let numbers = Numbers {
        state: numbers_state,
        unbacked,
        supplier: s
            .nums
            .iter()
            .map(Num::label)
            .chain(s.codes.iter().map(|c| c.text.clone()))
            .chain(pack_nums.iter().map(|n| format!("({})", n.label())))
            .collect(),
        candidate: c_all
            .nums
            .iter()
            .map(Num::label)
            .chain(c_all.codes.iter().map(|c| c.text.clone()))
            .collect(),
        shared,
        lacking,
    };

    // Words.
    let s_words: HashSet<&String> = s.words.iter().collect();
    let s_implied = implied(&s);
    let uncovered: Vec<String> = c_name
        .words
        .iter()
        .filter(|w| !s_words.contains(w) && !s_implied.contains(*w))
        .cloned()
        .collect();
    let c_name_words: HashSet<&String> = c_name.words.iter().collect();
    let mut opposites = Vec::new();
    for (a, b) in &TABLES.opposites {
        for (x, y) in [(a, b), (b, a)] {
            if s_words.contains(x)
                && !s_words.contains(y)
                && c_name_words.contains(y)
                && !c_name_words.contains(x)
            {
                opposites.push((x.clone(), y.clone()));
            }
        }
    }
    let c_all_words: HashSet<&String> = c_all.words.iter().collect();
    let c_implied = implied(&c_all);
    let missing: Vec<String> = s
        .words
        .iter()
        .filter(|w| vocab.is_part_word(w))
        .filter(|w| !c_all_words.contains(w) && !c_implied.contains(*w))
        .cloned()
        .collect();
    let words_state = if !opposites.is_empty() {
        State::Conflict
    } else if uncovered.is_empty() && missing.is_empty() {
        State::Consistent
    } else {
        State::Insufficient
    };

    Evidence {
        numbers,
        words: Words {
            state: words_state,
            uncovered,
            opposites,
            missing,
        },
        brand: Brand {
            state: brand_state,
            supplier: supplier_brand,
            candidate: c_brand.map(str::to_string),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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

    /// The six products the incident touched, as Landau's registry held them on 2026-11-08
    /// (`wnx-career.products`, read-only), by the last four hex digits the plan names them by.
    pub(crate) fn bearing_5e12() -> Candidate {
        cand(
            "6aec987cd00e9931421a5e12",
            "بلبرینگ بزرگ چرخ عقب پراید و چرخ جلو پیکان",
            Some("بلبرینگ 44649 | بلبرینگ چرخ پراید عظام"),
            Some("PSN عظام"),
        )
    }
    pub(crate) fn clamp_576d() -> Candidate {
        cand(
            "6aec9869d00e9931421a576d",
            "بست کمری نایلونی 30 سانتی عرض 3.6 میلی متر CW",
            Some("بست نایلونی | زیپ تای | cable tie | بست کابل"),
            Some("آراد پارت"),
        )
    }
    pub(crate) fn clamp_57a5() -> Candidate {
        cand(
            "6aec9869d00e9931421a57a5",
            "بست کمری نایلونی 37 سانتی عرض 7.6 میلی متر",
            Some("بست نایلونی | زیپ تای | cable tie | بست کابل"),
            Some("آراد پارت"),
        )
    }
    pub(crate) fn clamp_57ac() -> Candidate {
        cand(
            "6aec986ad00e9931421a57ac",
            "بست کمری نایلونی 37 سانتی عرض 7.6 میلی متر CW",
            Some("بست نایلونی | زیپ تای | cable tie | بست کابل"),
            Some("آراد پارت"),
        )
    }
    pub(crate) fn clamp_57eb() -> Candidate {
        cand(
            "6aec986ad00e9931421a57eb",
            "بست کمری نایلونی مشکی 38 سانتی عرض 4.8 میلی متر",
            Some("بست نایلونی | زیپ تای | cable tie | بست کابل"),
            Some("GSR"),
        )
    }
    pub(crate) fn bearing_6c04() -> Candidate {
        cand(
            "6aec98a4d00e9931421a6c04",
            "بلبرینگ وسط پلوس پژو 405 (6006)",
            Some("بلبرینگ وسط پلوس 405 | بلبرینگ 6006 | mid shaft bearing 405"),
            Some("الما۷"),
        )
    }

    /// The supplier items of the incident, as the seller's bridge database holds them
    /// (`invoice_lines`, read from a `.backup` copy on 2026-11-08): `(code, name)`.
    pub(crate) const WRONG_BEARING: (&str, &str) = (
        "33102017",
        "بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027",
    );
    pub(crate) const RIGHT_BEARING: (&str, &str) = (
        "33102018",
        "بلبرينگ بزرگ چرخ عقب پرايد جدبد (بلبرينگ بزرگ چرخ جلو پيکان) 44649 PSN عظام (96) 3154028",
    );
    pub(crate) const CLAMP_11: (&str, &str) = ("AR1330-11", "بست کمري CW3.6*370 آراد پارت (100)");
    pub(crate) const CLAMP_4: (&str, &str) =
        ("AR1330-4", "بست کمري CW4.8*370 تايوان آراد پارت (100)");
    const CLAMP_5: (&str, &str) = ("AR1330-5", "بست کمري 370 7/6 تايوان آراد پارت (100)");
    const CLAMP_8: (&str, &str) = ("AR1330-8", "بست کمري CW7.6*37 آراد پارت");
    const CLAMP_GSR: (&str, &str) = ("8800094", "بست کمري 380*4/8 GSR مشکي");
    pub(crate) const MID_SHAFT: (&str, &str) =
        ("115001165", "بلبرينگ وسط 6006 پلوس پژو ELMA7-405 (10)");

    /// A small registry: the six products above, their siblings, and enough names to make the
    /// common words vocabulary («بوش» as both a brand and the bushing; «تایوان» as a word).
    pub(crate) fn vocab() -> Vocabulary {
        let mut products: Vec<(String, Option<&str>)> = vec![
            (bearing_5e12().name, Some("PSN عظام")),
            (clamp_576d().name, Some("آراد پارت")),
            (clamp_57a5().name, Some("آراد پارت")),
            (clamp_57ac().name, Some("آراد پارت")),
            (clamp_57eb().name, Some("GSR")),
            (bearing_6c04().name, Some("الما۷")),
            ("بلبرینگ چرخ جلو قدیم پراید".into(), Some("عظام")),
            ("لنت ترمز جلو پراید".into(), Some("DPI")),
            ("لنت ترمز جلو پراید".into(), Some("عظام")),
            ("وایر شمع پراید".into(), Some("FGP")),
            ("وایر شمع پراید".into(), Some("پایا موتور")),
            ("شمع موتور".into(), Some("بوش")),
            ("آینه بغل چپ پراید".into(), Some("کروز")),
            ("آینه بغل راست پراید".into(), Some("کروز")),
        ];
        for w in [
            "بوش طبق پژو 405",
            "بوش طبق پراید",
            "بوش جناغی پراید",
            "بست تایوان",
            "پیچ تایوان",
            "مهره تایوان",
            "طبق پژو 405 کامل",
            "طبق پراید چپ",
            "طبق پراید راست",
            "بلبرینگ چرخ عقب پراید",
            "بلبرینگ چرخ جلو پیکان",
            "پلوس پژو 405",
            "بلبرینگ وسط پلوس پراید",
            "بست کمری نایلونی سفید",
            "شمع موتور سمند",
            "وایر شمع سمند",
            "آینه بغل پراید",
            "آینه بغل سمند",
            "پمپ بنزین پراید",
            "پمپ بنزین پژو 405",
            "پمپ بنزین سمند",
        ] {
            products.push((w.into(), None));
        }
        Vocabulary::build(products.iter().map(|(n, b)| (n.as_str(), *b)))
    }

    fn ev(item: (&str, &str), c: &Candidate) -> Evidence {
        evidence(item.1, item.0, c, &vocab())
    }

    #[test]
    fn coarse_keeps_decimals_between_digits_and_splits_the_rest() {
        assert_eq!(
            coarse("بست کمري CW3.6*370"),
            ["بست", "کمری", "cw3.6", "370"]
        );
        assert_eq!(coarse("380*4/8 GSR"), ["380", "4/8", "gsr"]);
        assert_eq!(coarse("ELMA7-405 (10)"), ["elma7", "405", "10"]);
        assert_eq!(coarse("پژو ۴۰۵. «عظام»"), ["پژو", "405", "عظام"]);
        assert_eq!(coarse("بـــست"), ["بست"], "kashida");
    }

    #[test]
    fn decimals_normalise_and_scale() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        assert_eq!(literal("7/6")[0].value, d("7.6"));
        assert_eq!(d("7.60"), d("7.6"));
        assert_eq!(d("0631"), d("631"));
        assert_eq!(d("37").times10(), d("370"));
        assert_eq!(d("3.6").times10(), d("36"));
        assert_eq!(d("0.25").to_string(), "0.25");
        assert_eq!(d("7.6").to_string(), "7.6");
        assert!(Decimal::parse("1".repeat(31).as_str()).is_none());
    }

    #[test]
    fn units_follow_numbers_and_millimetres_equal_ten_centimetres() {
        let r = read(&coarse("بست 37 سانتی عرض 7.6 میلی متر 380mm 12"));
        let labels: Vec<String> = r.nums.iter().map(Num::label).collect();
        assert_eq!(labels, ["37 cm", "7.6 mm", "380 mm", "12"]);
        assert_eq!(r.words, ["بست"], "unit words consumed, «عرض» a stop word");
        let n = |v: &str, unit| Num {
            value: Decimal::parse(v).unwrap(),
            unit,
            decimal: v.contains('.'),
        };
        assert!(n("370", Unit::None).same(&n("37", Unit::Cm)));
        assert!(n("37", Unit::None).same(&n("37", Unit::Cm)));
        assert!(n("370", Unit::Mm).same(&n("37", Unit::Cm)));
        assert!(!n("37", Unit::Mm).same(&n("37", Unit::Cm)));
        assert!(!n("300", Unit::None).same(&n("37", Unit::Cm)));
        assert!(n("4.8", Unit::None).same(&n("4.8", Unit::Mm)));
    }

    #[test]
    fn the_wrong_bearing_conflicts_on_its_part_number() {
        let e = ev(WRONG_BEARING, &bearing_5e12());
        assert_eq!(e.numbers.state, State::Conflict, "{e:?}");
        assert_eq!(e.numbers.lacking, ["44649"]);
        assert_eq!(
            e.numbers.supplier,
            ["69349", "3154027", "(54)"],
            "the trailing reference kept, the carton count only backs"
        );
        assert_eq!(
            e.brand.state,
            State::Consistent,
            "PSN عظام, the longest brand"
        );
        assert_eq!(
            e.brand.supplier,
            SupplierBrand::One {
                brand: "PSN عظام".into(),
                ambiguous: false
            }
        );
        assert_eq!(e.words.state, State::Insufficient);
        assert_eq!(e.words.uncovered, ["بزرگ", "عقب", "پیکان"]);
        assert!(e.any_conflict());
    }

    #[test]
    fn the_right_bearing_is_consistent_in_every_dimension() {
        let e = ev(RIGHT_BEARING, &bearing_5e12());
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
        assert_eq!(e.numbers.shared, ["44649"]);
        assert_eq!(e.words.state, State::Consistent, "{e:?}");
        assert_eq!(e.brand.state, State::Consistent);
        assert!(!e.any_conflict());
    }

    #[test]
    fn the_collided_clamp_row_conflicts_with_both_supplier_items() {
        for item in [CLAMP_11, CLAMP_4] {
            let e = ev(item, &clamp_576d());
            assert_eq!(e.numbers.state, State::Conflict, "{item:?}: {e:?}");
            assert!(e.numbers.lacking.contains(&"30 cm".to_string()), "{e:?}");
        }
    }

    #[test]
    fn the_four_consistent_rows_show_no_conflict() {
        for (item, c) in [
            (CLAMP_5, clamp_57a5()),
            (CLAMP_8, clamp_57ac()),
            (CLAMP_GSR, clamp_57eb()),
            (MID_SHAFT, bearing_6c04()),
        ] {
            let e = ev(item, &c);
            assert_eq!(e.numbers.state, State::Consistent, "{item:?}: {e:?}");
            assert!(!e.any_conflict(), "{item:?}: {e:?}");
            assert_eq!(e.brand.state, State::Consistent, "{item:?}: {e:?}");
        }
    }

    #[test]
    fn a_latin_brand_spelling_matches_its_listed_equivalent_only() {
        let e = ev(MID_SHAFT, &bearing_6c04());
        assert_eq!(
            e.brand.supplier,
            SupplierBrand::One {
                brand: "ELMA7".into(),
                ambiguous: false
            }
        );
        assert_eq!(e.words.state, State::Consistent, "{e:?}");
        let other = cand("x", "بلبرینگ وسط پلوس پژو 405 (6006)", None, Some("عظام"));
        assert_eq!(ev(MID_SHAFT, &other).brand.state, State::Conflict);
    }

    #[test]
    fn a_pick_of_another_brand_conflicts() {
        // The DPI case of the 33 historical picks: the supplier name says عظام.
        let item = ("31370061", "لنت جلو پرايد عظام (20)");
        let dpi = cand("d", "لنت ترمز جلو پراید", None, Some("DPI"));
        let e = ev(item, &dpi);
        assert_eq!(e.brand.state, State::Conflict, "{e:?}");
        assert_eq!(
            e.words.state,
            State::Consistent,
            "«لنت» implies «ترمز»: {e:?}"
        );
    }

    #[test]
    fn two_brands_in_one_name_say_nothing_about_the_brand() {
        let item = ("2001", "وایر شمع ساژم پراید FGP (پایا موتور)");
        let e = ev(item, &cand("f", "وایر شمع پراید", None, Some("FGP")));
        assert_eq!(e.brand.state, State::Insufficient);
        assert!(matches!(e.brand.supplier, SupplierBrand::Several { .. }));
    }

    #[test]
    fn a_brand_that_is_also_a_part_word_yields_to_a_plain_brand() {
        // «بوش» is a registry brand and the bushing word of three names.
        let item = ("3001", "بوش طبق پژو 405 عظام");
        let bushing = cand("b", "بوش طبق پژو 405", None, Some("عظام"));
        let e = ev(item, &bushing);
        assert_eq!(e.brand.state, State::Consistent, "{e:?}");
        assert_eq!(e.words.state, State::Consistent, "{e:?}");
        // The arm itself, without the bushing word, misses a part word.
        let arm = cand("a", "طبق پژو 405 کامل", None, Some("عظام"));
        let e = ev(("3002", "بوش طبق پژو 405 عظام"), &arm);
        assert!(e.words.missing.contains(&"بوش".to_string()), "{e:?}");
        assert_ne!(e.words.state, State::Consistent);
        // Alone, the ambiguous brand is read as the brand, and a different one is not a conflict.
        let e = ev(
            ("3003", "بوش طبق پراید"),
            &cand("p", "بوش طبق پراید", None, Some("عظام")),
        );
        assert_eq!(e.brand.state, State::Insufficient, "{e:?}");
    }

    #[test]
    fn opposite_words_on_the_two_sides_conflict() {
        let item = ("4001", "آینه بغل چپ پراید کروز");
        let right = cand("r", "آینه بغل راست پراید", None, Some("کروز"));
        let e = ev(item, &right);
        assert_eq!(e.words.state, State::Conflict, "{e:?}");
        assert_eq!(e.words.opposites, [("چپ".to_string(), "راست".to_string())]);
        let left = cand("l", "آینه بغل چپ پراید", None, Some("کروز"));
        assert_eq!(ev(item, &left).words.state, State::Consistent);
    }

    #[test]
    fn a_candidate_number_the_supplier_name_lacks_is_unbacked() {
        let c = cand("c", "پمپ بنزین 405", None, Some("عظام"));
        let e = ev(("5001", "پمپ بنزین پژو عظام"), &c);
        assert!(e.numbers.unbacked, "{e:?}");
        assert_eq!(e.numbers.state, State::Insufficient);
        let none = cand("n", "پمپ بنزین پراید", None, Some("عظام"));
        let e = ev(("5002", "پمپ بنزین پراید عظام"), &none);
        assert!(!e.numbers.unbacked);
        assert_eq!(
            e.numbers.state,
            State::Insufficient,
            "no number either side"
        );
    }

    #[test]
    fn pack_markers_and_the_items_own_code_are_not_numbers_of_the_part() {
        let item = ("3137006", "لنت جلو پژو 405 عظام* <2> (16) 3137006");
        let c = cand("c", "لنت ترمز جلو پژو 405", None, Some("عظام"));
        let e = ev(item, &c);
        assert_eq!(e.numbers.supplier, ["405", "(16)"], "{e:?}");
        assert_eq!(e.numbers.state, State::Consistent);
        assert_eq!(e.words.state, State::Consistent, "{e:?}");
    }

    #[test]
    fn a_slash_is_a_decimal_only_between_short_runs() {
        let labels = |t: &str| -> Vec<String> { literal(t).iter().map(Num::label).collect() };
        assert_eq!(labels("4/8"), ["4.8"]);
        assert_eq!(labels("2/05"), ["2.05"]);
        assert_eq!(labels("206/207"), ["206", "207"], "two models, not 206.207");
        assert_eq!(labels("1402/05/10"), ["1402", "5", "10"]);
        // One reading only, so 4.8 and 8.4 stay two widths.
        let n = |t: &str| literal(t)[0];
        assert!(!n("4/8").same(&n("8.4")));
        assert!(n("4/8").same(&n("4.8")));
    }

    #[test]
    fn codes_compare_only_with_codes_of_the_same_letters() {
        let v = vocab();
        // An alias naming the engine («XU7») says nothing against a name that names no `xu` code.
        let seal = cand(
            "s",
            "کاسه نمد عقب میلنگ پژو 405",
            Some("سیل عقب میلنگ XU7"),
            Some("GSR"),
        );
        let e = evidence("کاسه نمد عقب میلنگ پژو 405 GSR", "13800058", &seal, &v);
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
        // The same belt family with other digits is a different belt.
        let belt = cand(
            "b",
            "تسمه دینام پژو 206",
            Some("تسمه 6PK1220"),
            Some("دایکو"),
        );
        let e = evidence("تسمه دینام 6PK1575 پژو 206 دایکو", "420097", &belt, &v);
        assert_eq!(e.numbers.state, State::Conflict, "{e:?}");
        assert_eq!(e.numbers.lacking, ["6pk1220"]);
        let same = cand(
            "b",
            "تسمه دینام پژو 206",
            Some("تسمه 6PK1575"),
            Some("DAYCO"),
        );
        let e = evidence("تسمه دینام 6PK1575 پژو 206 دایکو", "420097", &same, &v);
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
        assert_eq!(e.brand.state, State::Consistent, "DAYCO ≡ دایکو: {e:?}");
    }

    #[test]
    fn a_number_of_a_kind_the_supplier_does_not_state_is_unbacked_not_a_conflict() {
        let v = vocab();
        // A shim sized 2.05 for the 405: the supplier names the size, not the car.
        let shim = cand(
            "s",
            "شیم تنظیم سوپاپ 2/05 پژو 405",
            Some("شیم XU7 405"),
            Some("آراد پارت"),
        );
        let e = evidence("شیم سوپاپ سایز 2.05 آراد پارت", "JHN100559", &shim, &v);
        assert_eq!(e.numbers.state, State::Insufficient, "{e:?}");
        assert!(e.numbers.unbacked);
        // An alias's model number alone never makes a pair unbacked.
        let disc = cand(
            "d",
            "دیسک ترمز چرخ جلو تیبا",
            Some("دیسک ترمز تیبا 2"),
            Some("عظام"),
        );
        let e = evidence("دیسک ترمز چرخ جلو تیبا عظام (4)", "55003L", &disc, &v);
        assert!(!e.numbers.unbacked, "{e:?}");
        assert_eq!(e.numbers.state, State::Insufficient);
    }

    #[test]
    fn brand_spellings_compare_without_spaces_and_a_brand_inside_another_is_related() {
        let v = Vocabulary::build([
            ("میل موجگیر مگان", Some("نیکو پخش")),
            ("گلگیر مگان", Some("نیکوپخش")),
            ("فلاپ سپر پارس", Some("کروز")),
            ("فلاپ سپر سمند", Some("کروز پلاس")),
        ]);
        let c = cand("m", "میل موجگیر بلند مگان", None, Some("نیکو پخش"));
        let e = evidence("میل موجگیر بلند مگان نیکوپخش", "42006116", &c, &v);
        assert_eq!(e.brand.state, State::Consistent, "{e:?}");
        let c = cand("f", "فلاپ سپر پارس", None, Some("کروز"));
        let e = evidence("فلاپ سپر پارس کروز پلاس", "CR37137801", &c, &v);
        assert_eq!(
            e.brand.state,
            State::Insufficient,
            "a product line, not proof: {e:?}"
        );
        assert!(!e.any_conflict());
    }

    #[test]
    fn digits_glued_to_persian_letters_are_a_missing_space_not_a_code() {
        assert_eq!(coarse("206تیپ 5 75عظام"), ["206", "تیپ", "5", "75", "عظام"]);
        assert_eq!(
            coarse("XU7 6PK1575"),
            ["xu7", "6pk1575"],
            "Latin codes stay whole"
        );
        let v = vocab();
        let c = cand(
            "n",
            "سوزن انژکتور پژو 206 تیپ 5",
            Some("انژکتور 206 تیپ5"),
            Some("عظام"),
        );
        let e = evidence("سوزن انژکتور 206تیپ 5 عظام", "0130104003", &c, &v);
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
    }

    #[test]
    fn a_code_only_meets_a_code_of_its_own_shape() {
        let v = vocab();
        // «6206C3» (`#c#`) and «C3» (`c#`) are different shapes: no conflict between them.
        let c = cand("b", "بلبرینگ 6206 C3 گیربکس نیسان", None, Some("NACHI"));
        let e = evidence("بلبرینگ (6206C3) گیربکس نیسان NACHI", "10044S", &c, &v);
        assert_ne!(e.numbers.state, State::Conflict, "{e:?}");
    }

    #[test]
    fn a_number_written_with_a_point_is_a_measure_whatever_its_value() {
        let v = vocab();
        // «3.00» states a size, so the 405 it does not state is unbacked, not a conflict.
        let c = cand("s", "شیم تنظیم سوپاپ 3.00 پژو 405", None, Some("آراد پارت"));
        let e = evidence("شیم تنظیم سوپاپ 3.00 آراد پارت", "87010370", &c, &v);
        assert_ne!(e.numbers.state, State::Conflict, "{e:?}");
        assert!(e.numbers.unbacked);
    }

    #[test]
    fn a_bracketed_supplier_number_backs_but_never_conflicts() {
        let v = vocab();
        let belt = cand("b", "تسمه دینام پژو 405 855", None, Some("دایکو"));
        let e = evidence("تسمه دینام(855) پژو 405 دایکو", "70PCF166506052", &belt, &v);
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
        // A carton count states nothing: the candidate's 44649 is unbacked, not in conflict.
        let e = evidence(
            "بلبرينگ بزرگ چرخ عقب پراید PSN عظام (96)",
            "1",
            &bearing_5e12(),
            &v,
        );
        assert_eq!(e.numbers.state, State::Insufficient, "{e:?}");
    }

    #[test]
    fn a_brand_in_another_word_order_is_the_same_brand() {
        let v = Vocabulary::build([
            ("کاسه نمد چرخ جلو پراید", Some("ماهان (تکنیک مدرن)")),
            ("فندک کامل پژو 206", Some("تکنیک مدرن ماهان")),
        ]);
        let c = cand(
            "k",
            "کاسه نمد چرخ جلو پراید",
            None,
            Some("ماهان (تکنیک مدرن)"),
        );
        let e = evidence(
            "کاسه نمد چرخ جلو پراید تکنیک مدرن ماهان",
            "13800004",
            &c,
            &v,
        );
        assert_eq!(e.brand.state, State::Consistent, "{e:?}");
    }

    #[test]
    fn an_ambiguous_brand_proves_nothing_and_stays_a_word() {
        // «بوش استارت» is the starter's bushing; a Bosch starter is another part.
        let starter = cand("s", "استارت پژو 405", None, Some("بوش"));
        let e = ev(("6001", "بوش استارت پژو 405"), &starter);
        assert_eq!(e.brand.state, State::Insufficient, "{e:?}");
        assert!(e.words.missing.contains(&"بوش".to_string()), "{e:?}");
    }

    #[test]
    fn a_brand_inside_its_own_products_names_is_not_ambiguous() {
        let v = Vocabulary::build([
            ("لنت ترمز جلو پراید عظام", Some("عظام")),
            ("لنت ترمز عقب پراید عظام", Some("عظام")),
            ("لنت ترمز جلو پژو 405 عظام", Some("عظام")),
            ("لنت ترمز جلو پراید", Some("DPI")),
        ]);
        let dpi = cand("d", "لنت ترمز جلو پراید", None, Some("DPI"));
        let e = evidence("لنت جلو پرايد عظام (20)", "31370061", &dpi, &v);
        assert_eq!(e.brand.state, State::Conflict, "{e:?}");
    }

    #[test]
    fn a_short_brand_is_not_inside_a_longer_word() {
        let v = Vocabulary::build([("بلبرینگ چرخ", Some("NTN")), ("بلبرینگ چرخ", Some("NT"))]);
        let c = cand("n", "بلبرینگ چرخ", None, Some("NT"));
        assert_eq!(
            evidence("بلبرینگ چرخ NTN", "1", &c, &v).brand.state,
            State::Conflict
        );
    }

    #[test]
    fn door_is_a_word_that_must_be_covered() {
        // «در» is "door" as well as "in": the windscreen is not the front door's glass.
        let v = Vocabulary::build([
            ("شیشه در جلو پراید", Some("عظام")),
            ("شیشه در عقب پراید", Some("عظام")),
            ("دستگیره در جلو پراید", Some("عظام")),
            ("شیشه جلو پراید", Some("عظام")),
        ]);
        let screen = cand("w", "شیشه جلو پراید", None, Some("عظام"));
        let e = evidence("شیشه در جلو پراید عظام", "7001", &screen, &v);
        assert_eq!(e.words.missing, ["در"], "{e:?}");
    }

    #[test]
    fn a_bracketed_part_number_states_its_number() {
        let v = vocab();
        let c = cand(
            "b",
            "بلبرینگ وسط پلوس پژو",
            Some("بلبرینگ 6005"),
            Some("ELMA7"),
        );
        let e = evidence("بلبرينگ وسط پلوس پژو (6006) ELMA7", "8001", &c, &v);
        assert_eq!(e.numbers.state, State::Conflict, "{e:?}");
    }

    #[test]
    fn a_part_number_glued_to_letters_in_the_candidates_name_still_counts() {
        let v = vocab();
        let c = cand("b", "بلبرینگ چرخ جلو پراید 6205ZZ", None, Some("عظام"));
        let e = evidence("بلبرينگ چرخ جلو پرايد 6206 ZZ عظام", "9001", &c, &v);
        assert_eq!(e.numbers.state, State::Conflict, "{e:?}");
        let e = evidence("بلبرينگ چرخ جلو پرايد 6205ZZ عظام", "9002", &c, &v);
        assert_eq!(e.numbers.state, State::Consistent, "{e:?}");
    }

    #[test]
    fn arabic_alef_maksura_reads_as_ye() {
        assert_eq!(coarse("قدىم"), coarse("قدیم"));
    }

    #[test]
    fn a_rod_after_a_number_is_not_a_millimetre() {
        let r = read(&coarse("پژو 206 میل لنگ"));
        assert_eq!(r.nums.iter().map(Num::label).collect::<Vec<_>>(), ["206"]);
        assert!(r.words.contains(&"میل".to_string()));
    }

    #[test]
    fn the_tables_load_and_hold_only_reviewed_lines() {
        let lines = |t: &'static str| rows(t).count();
        assert_eq!(
            TABLES.implied.len(),
            lines(IMPLIED),
            "a line without two cells"
        );
        assert_eq!(
            TABLES.opposites.len(),
            lines(OPPOSITES),
            "a line without two cells"
        );
        assert_eq!(TABLES.opposites.len(), 6);
        assert!(TABLES.implied.contains(&("405".into(), "پژو".into())));
        let class = |b: &str| BrandClass::of(&coarse(b)).0.concat();
        assert_eq!(class("ELMA7"), "الما7");
        assert_eq!(class("DAYCO"), "دایکو");
        assert_eq!(class("نیکو پخش"), "نیکوپخش");
        assert_eq!(class("عظام"), "عظام");
        assert!(TABLES.stop.contains("و"));
    }
}
