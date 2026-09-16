# Names, Unavailable Quotes and a Shelf Never at Cost — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Key the supplier link on the raw Barezpakhsh name (codes may change; Landau's registry has none), price by name with the code as fallback, resolve never-invoiced shelf rows through the landau-parts map, apply an unavailable quote only when it raises the cost, and guarantee the shelf is never on sale at cost — using Landau plan 107 (rule applied at write, targeted reprice) instead of waiting for the mirror.

**Architecture:** Pure helpers first (`matching/normalize.rs` keys, `jobs/pricing.rs` price rules), then the supplier client (name search, error-page retry), the schema (migration `0002` re-keys `matches`, adds `name_map` and two item columns), the Landau client (write replies carry price/`priced`, `reprice {stocks}`), then the two jobs, then the UI (settings map loader and margin field, report columns, matches page), then live tests and docs. Every step of both jobs keeps its shape: PRICE decides the cost per item, WRITE writes and reads the price off the reply, REPRICE is the targeted call, REPORT audits the margin.

**Tech Stack:** Rust 2021 (tokio, reqwest, rusqlite bundled, scraper/roxmltree, regex, csv), Tauri 2 (dialog plugin), Nuxt 4 / Vue 3.5 / Nuxt UI 4, vitest.

**Spec:** `docs/superpowers/specs/2026-09-13-landau-bridge-design.md` — read **§14 (rulings of 2026-09-15)** first; every task below argues from a lettered ruling there (§14 a–g). §2.1/§4/§7 describe the flows the rulings amend.

## Global Constraints

- Never edit anything under `/home/vahid/Claude` (Landau). Landau plan 107 is live on `http://localhost:6150`; the bridge adapts to it (§14 f).
- UI is Persian, RTL; Latin data (codes, ids, paths) in `<bdi dir="ltr">`; numbers through `faNumber`; no `letter-spacing`/uppercase on Persian text.
- TDD: every new function gets its failing test first (`cargo test` in `src-tauri`, `pnpm test` at the root). Before each commit: `cd src-tauri && cargo test && cargo clippy --all-targets` (zero warnings) and, when `app/` changed, `pnpm typecheck && pnpm test` at the root.
- Live checks need `export NO_PROXY='*'` and the env from `LND_ROLES_FILE=/home/vahid/Claude/.memory/.local/landau-roles.json node scripts/mint-apt.mjs` plus `BAREZ_USER`/`BAREZ_PASS` from `docs/RUNBOOK.md` §Fixtures. The fixture store is `6aa6c145f65d2a0b1a8dd070` (33 live rows, active 15 % SHELF rule `6aa72aaaaab3b347d5402a2a`, rounding 1 000). Delete every row a probe attaches.
- Work on branch `names-and-margin` off `main`; commit after every task with the trailer `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`. Never commit a token or a password.
- Supplier request pacing stays: 100 ms between search pages, `RETRY_DELAYS` = 1 s, 3 s.
- Persian copy for new labels/notes is given verbatim in the tasks; do not paraphrase.

---

### Task 0: Land the pending page-walk change on the branch

**Files:**
- Modify (already changed in the tree): `src-tauri/src/supplier/barezpakhsh/client.rs`, `src-tauri/tests/live_supplier.rs`, `docs/superpowers/specs/2026-09-13-landau-bridge-design.md`
- Add: `docs/landau-request-pricing-at-write.md`

- [x] **Step 1: Branch and confirm the tree is green**

```bash
git switch -c names-and-margin
cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)' ; cd ..
```
Expected: `90 passed`, no clippy output.

- [x] **Step 2: Commit**

```bash
git add src-tauri/src/supplier/barezpakhsh/client.rs src-tauri/tests/live_supplier.rs docs/superpowers/specs/2026-09-13-landau-bridge-design.md docs/landau-request-pricing-at-write.md
git commit -m "Walk the price search pages until the exact code appears

The site's search is a substring match that does not rank the exact code first; a first-page-only lookup answered NoCurrentPrice for listed parts (measured 2026-09-15: 34 of 396 sampled codes sat on pages 2-30). Also records the request handed to the Landau agent for pricing at write.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 1: Name key and search words (§14 a, b)

**Files:**
- Modify: `src-tauri/src/matching/normalize.rs`

**Interfaces:**
- Produces: `pub fn name_key(s: &str) -> String` (normalised, lower-cased, punctuation kept); `pub const SEARCH_WORDS: usize = 7`; `pub fn search_words(name: &str) -> String` (first 7 words of `normalize_text(name)`).

- [x] **Step 1: Write the failing tests** — append to the `#[cfg(test)] mod tests` of `normalize.rs` (create the module if the file has none):

```rust
    #[test]
    fn name_key_unifies_glyphs_digits_and_case_but_keeps_punctuation() {
        assert_eq!(name_key("لنت جلو پژو ۴۰۵ عظام* (16)"), name_key("لنت جلو پژو 405 عظام* (16)"));
        assert_eq!(name_key("سيبک فرمان ISPCO"), "سیبک فرمان ispco");
        assert_ne!(name_key("پژو 405<2> (20)"), name_key("پژو 405 2 20"), "punctuation stays significant");
        assert_eq!(name_key("  آینه\u{200c}دستی  چپ "), "آینه دستی چپ");
    }

    #[test]
    fn search_words_sends_at_most_seven_words() {
        let long = "بلبرینگ کوچک بیرونی چرخ عقب 32011 نیسان NTN Z24 ژاپن";
        assert_eq!(search_words(long), "بلبرینگ کوچک بیرونی چرخ عقب 32011 نیسان");
        assert_eq!(search_words("سپر جلو زرد کوروش يدک سمند"), "سپر جلو زرد کوروش یدک سمند");
        assert_eq!(search_words(""), "");
    }
```

- [x] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib normalize::tests 2>&1 | grep -E 'error|cannot find'`
Expected: `cannot find function name_key` / `search_words`.

- [x] **Step 3: Implement**

Add below `key_text` in `normalize.rs`:

```rust
/// The key a supplier link is stored under (spec §14 a): glyphs, digits, ZWNJ and whitespace
/// unified, Latin lower-cased, punctuation kept — two catalogue names that differ only in a
/// pack marker or a «<2>» stay two keys.
pub fn name_key(s: &str) -> String {
    normalize_text(s).to_lowercase()
}

/// How many words the site's search accepts; an eighth makes it answer its error page (§14 b).
pub const SEARCH_WORDS: usize = 7;

/// The search term for a raw name: the first [`SEARCH_WORDS`] words of the normalised name.
pub fn search_words(name: &str) -> String {
    normalize_text(name)
        .split(' ')
        .filter(|w| !w.is_empty())
        .take(SEARCH_WORDS)
        .collect::<Vec<_>>()
        .join(" ")
}
```

- [x] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test --lib normalize::tests`
Expected: all `ok`.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/matching/normalize.rs
git commit -m "Add the name key and the seven-word search term

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Supplier: error-page retry and search by name (§14 b)

**Files:**
- Modify: `src-tauri/src/supplier/barezpakhsh/parse.rs`, `src-tauri/src/supplier/barezpakhsh/client.rs`, `src-tauri/src/supplier/mod.rs`, `src-tauri/tests/live_supplier.rs`
- Create: `src-tauri/fixtures/barezpakhsh/error-page.html`

**Interfaces:**
- Consumes: `search_words` (Task 1).
- Produces: `parse::is_error_page(&str) -> bool`; trait method `async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>>` on `Supplier`; `Client::search_page` / `search_pages` (private).

- [x] **Step 1: Capture the error page fixture** (read-only; the site answers it to an eight-word term)

```bash
cd src-tauri/fixtures/barezpakhsh && export NO_PROXY='*' && python3 - <<'PY'
import os,re,json,requests
BASE='https://33314466.ir'; s=requests.Session(); s.trust_env=False
p=s.get(BASE+'/Home/Login').text; tok=re.search(r'name="__RequestVerificationToken" type="hidden" value="([^"]+)"',p).group(1)
s.post(BASE+'/Home/Login',data={'__RequestVerificationToken':tok,'Username':os.environ['BAREZ_USER'],'Password':os.environ['BAREZ_PASS']})
buy=s.get(BASE+'/Shop/Buy/Index').text
r=s.post(BASE+'/Shop/Share/ItemsSearchGradual',data=json.dumps({'selectedGroupCode':'---','selectedCarCode':'2','selectedBrandCode':'10','SpecialItemsId':'','searchTerm':'a b c d e f g h','businessId':'0','pageIndex':1,'userGroupId':'70','justAvailable':False,'pageSize':'50'}),headers={'content-type':'application/json; charset=utf-8','x-requested-with':'XMLHttpRequest'})
assert 'بروز خطا' in r.text, r.text[:200]
html=re.sub(r'آقاي [^<]+','آقاي نمونه خريدار',r.text)   # no seller name in the repo
open('error-page.html','w').write(html); print(len(html),'bytes')
PY
grep -c 'بروز خطا' error-page.html; grep -c 'نمونه خريدار' error-page.html
```
Expected: `1` and `≥ 1` (the name was scrubbed). Read the file once to confirm it holds nothing personal.

- [x] **Step 2: Write the failing parse test** — in `parse.rs` tests:

```rust
    const ERROR_PAGE: &str = include_str!("../../../fixtures/barezpakhsh/error-page.html");

    /// Measured 2026-09-15: a search term of eight or more words — and, transiently, any request —
    /// is answered with HTTP 200 and this page instead of the JSON string.
    #[test]
    fn recognises_the_sites_error_page_and_nothing_else() {
        assert!(is_error_page(ERROR_PAGE));
        assert!(!is_error_page(ITEMS));
        assert!(!is_error_page(LOGIN));
        assert!(!is_error_page(BUY_SEARCH));
    }
```

- [x] **Step 3: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib recognises_the_sites_error_page 2>&1 | grep -E 'error|cannot find'`
Expected: `cannot find function is_error_page`.

- [x] **Step 4: Implement `is_error_page`** — in `parse.rs` after `is_login_page`:

```rust
/// The site's generic error page («بروز خطا لطفا چند دقیقه دیگر دوباره سعی نمایید»): HTTP 200 with
/// HTML where a JSON string was expected. Deterministic for a search term over seven words,
/// transient (~0.3 %) for anything else — measured 2026-09-15.
pub fn is_error_page(body: &str) -> bool {
    body.trim_start().starts_with("<!DOCTYPE") && body.contains("بروز خطا")
}
```

- [x] **Step 5: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib recognises_the_sites_error_page`
Expected: `ok`.

- [x] **Step 6: Write the failing client tests** — in `client.rs` tests, below the `price_search_*` tests:

```rust
    #[tokio::test]
    async fn search_pages_collects_every_page_until_a_short_one() {
        let pages = [full_page("سیبک"), vec![quote("سیبک فرمان چپ"), quote("سیبک فرمان راست")]];
        let asked = std::sync::Mutex::new(Vec::new());
        let all = collect_pages(50, |index| {
            asked.lock().unwrap().push(index);
            let page = pages.get(index as usize - 1).cloned().unwrap_or_default();
            async move { Ok(page) }
        })
        .await
        .unwrap();
        assert_eq!(all.len(), 52);
        assert_eq!(*asked.lock().unwrap(), vec![1, 2]);
    }

    #[tokio::test]
    async fn search_pages_stops_at_the_page_cap() {
        let all = collect_pages(50, |_| async { Ok(full_page("36")) }).await.unwrap();
        assert_eq!(all.len(), 50 * MAX_SEARCH_PAGES as usize);
    }
```

- [x] **Step 7: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib search_pages_ 2>&1 | grep -E 'cannot find'`
Expected: `cannot find function collect_pages`.

- [x] **Step 8: Implement** — in `client.rs`:

(a) Next to `find_across_pages`:

```rust
/// Every row of a search, page after page, until a page comes back short of `page_size` or
/// [`MAX_SEARCH_PAGES`] is reached (spec §14 b).
pub(crate) async fn collect_pages<F, Fut>(page_size: usize, fetch: F) -> Result<Vec<PriceQuote>>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<PriceQuote>>>,
{
    let mut all = Vec::new();
    for index in 1..=MAX_SEARCH_PAGES {
        let page = fetch(index).await?;
        let rows = page.len();
        all.extend(page);
        if rows < page_size {
            return Ok(all);
        }
    }
    tracing::warn!(pages = MAX_SEARCH_PAGES, "barezpakhsh search stopped at the page cap");
    Ok(all)
}
```

(b) Replace `post_json` so the error page is retried and then reported per item, never as an outage:

```rust
    /// POST JSON. The site answers its «خطا» page (HTTP 200, HTML) transiently to any request and
    /// always to an over-long search term; it is retried with [`RETRY_DELAYS`] and then reported as
    /// a parse failure of `page` — one item, never the job (spec §14 b).
    async fn post_json(&self, path: &str, page: &str, body: &serde_json::Value) -> Result<String> {
        let req_url = self.url(path)?;
        let payload = body.to_string();
        let mut attempt: usize = 0;
        loop {
            attempt += 1;
            let res = self
                .send_with_retry(path, || {
                    self.http
                        .post(req_url.clone())
                        .header("content-type", "application/json; charset=utf-8")
                        .header("x-requested-with", "XMLHttpRequest")
                        .body(payload.clone())
                })
                .await?;
            let status = res.status();
            tracing::debug!(method = "POST", path, %status, "barezpakhsh json");
            let text = res.text().await.map_err(Self::unreachable)?;
            if status.as_u16() == 403 && parse::is_blocked_page(&text) {
                return Err(BridgeError::SupplierBlocked);
            }
            Self::check_status(path, status)?;
            if !parse::is_error_page(&text) {
                return Ok(text);
            }
            if attempt <= RETRY_DELAYS.len() {
                tracing::warn!(path, attempt, "barezpakhsh answered its error page; retrying");
                tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
                continue;
            }
            return Err(BridgeError::SupplierParse {
                page: page.into(),
                field: "error page".into(),
            });
        }
    }
```
Update `post_json_authed` to take and pass the same `page: &str`, and the ledger call to pass `"moein2-page"`.

(c) Replace `price_of` and add the shared page fetch plus the new trait method:

```rust
    /// One page of `/Shop/Share/ItemsSearchGradual` for `term`, 100 ms after the previous one.
    async fn search_page(&self, s: &BuySession, term: &str, index: u32) -> Result<Vec<PriceQuote>> {
        if index > 1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let body = self
            .post_json_authed(
                "/Shop/Share/ItemsSearchGradual",
                "items-search",
                &json!({
                    "selectedGroupCode": s.group,
                    "selectedCarCode": s.car,
                    "selectedBrandCode": s.brand,
                    "SpecialItemsId": "",
                    "searchTerm": term,
                    "businessId": s.ctx.business_id,
                    "pageIndex": index,
                    "userGroupId": s.ctx.user_group_id,
                    "justAvailable": false,
                    "pageSize": SEARCH_PAGE_SIZE.to_string()
                }),
            )
            .await?;
        parse::price_quotes(&body)
    }
```
and in `impl Supplier for Client`:

```rust
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        let s = self.buy_session().await?;
        find_across_pages(code, SEARCH_PAGE_SIZE, |index| self.search_page(&s, code, index)).await
    }

    async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>> {
        let term = search_words(name);
        if term.is_empty() {
            return Ok(Vec::new());
        }
        let s = self.buy_session().await?;
        collect_pages(SEARCH_PAGE_SIZE, |index| self.search_page(&s, &term, index)).await
    }
```
(import `crate::matching::normalize::search_words`). In `supplier/mod.rs` add to the trait, after `price_of`:

```rust
    /// Every catalogue row the site's search answers for the first seven words of `name`
    /// (spec §14 b), all pages; empty when nothing matches. The caller picks the equal name.
    async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>>;
```

- [x] **Step 9: Run the whole offline suite and clippy**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: all passed, nothing from clippy.

- [x] **Step 10: Extend the live test** — in `tests/live_supplier.rs`, after the `deep` assertion:

```rust
    // §14 b: the site's search is a per-word AND over at most seven words; the equal-name row is
    // what the bridge prices by. Measured 2026-09-15 on 150 raw names from the landau-parts map.
    let by_name = supplier
        .quotes_by_name(&lines[0].name)
        .await
        .expect("name search");
    assert!(
        by_name.iter().any(|q| q.code == lines[0].code),
        "the invoice line's own row is among {} rows for «{}»",
        by_name.len(),
        lines[0].name
    );
    assert!(supplier
        .quotes_by_name("بلبرینگ کوچک بیرونی چرخ عقب 32011 نیسان NTN Z24 ژاپن")
        .await
        .expect("an over-long name is cut to seven words, never the error page")
        .len()
        >= 1);
```

- [x] **Step 11: Run it live**

Run: `cd src-tauri && export NO_PROXY='*' && cargo test --features live --test live_supplier -- --nocapture 2>&1 | grep -E 'test result|panicked'`
Expected: `2 passed`.

- [x] **Step 12: Commit**

```bash
git add src-tauri/src/supplier src-tauri/tests/live_supplier.rs src-tauri/fixtures/barezpakhsh/error-page.html
git commit -m "Search the price list by name and retry the site's error page

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Schema 0002 — links keyed by name, item columns, name map table (§14 a, c, g)

**Files:**
- Create: `src-tauri/src/db/migrations/0002_names.sql`
- Modify: `src-tauri/src/db/mod.rs`, `src-tauri/src/db/matches.rs`, `src-tauri/src/db/jobs.rs`

**Interfaces:**
- Consumes: `name_key` (Task 1).
- Produces: `MatchRow { supplier, name_key, code, product_id, product_name, how, similarity, name_seen, priced_at: Option<String>, decided_at }`; `Db::match_for_name(supplier, name)`, `Db::match_for_code(supplier, code)`, `Db::set_match(supplier, name_seen, code, product: Option<(&str, &str)>, how, similarity)`, `Db::touch_priced(supplier, name_seen)`, `Db::clear_match(supplier, name_seen)`, `Db::list_matches`, `Db::linked_matches`; `JobItemRow { …, available: Option<bool>, note: Option<String> }`; `Db::add_item_note(item_id, note)`; `Db::backfill_name_keys()` (called by `migrate`).

- [x] **Step 1: Write the migration**

`src-tauri/src/db/migrations/0002_names.sql`:

```sql
-- Supplier links keyed on the raw Barezpakhsh name (spec §14 a). The app moves the old rows
-- back under their name key on the next launch (Db::backfill_name_keys) and drops matches_old.
ALTER TABLE matches RENAME TO matches_old;
CREATE TABLE matches (
  supplier     TEXT NOT NULL,
  name_key     TEXT NOT NULL,           -- name_key(name_seen)
  code         TEXT NOT NULL DEFAULT '',
  product_id   TEXT,
  product_name TEXT,
  how          TEXT NOT NULL,           -- AUTO | USER | NONE | MAP
  similarity   INTEGER,
  name_seen    TEXT NOT NULL,
  priced_at    TEXT,                    -- when this link last answered a price
  decided_at   TEXT NOT NULL,
  PRIMARY KEY (supplier, name_key)
);
CREATE INDEX matches_code ON matches(supplier, code);

-- Report columns (spec §14 d, g).
ALTER TABLE job_items ADD COLUMN available INTEGER;   -- 1/0 per the supplier's quote, NULL unknown
ALTER TABLE job_items ADD COLUMN note TEXT;           -- «؛»-joined remarks for the report

-- The landau-parts map: which raw Barezpakhsh names were imported under which Landau product
-- (spec §14 c). Replaced whole on every load.
CREATE TABLE name_map (
  product_key TEXT NOT NULL,            -- name_key(product name)
  brand_key   TEXT NOT NULL,            -- name_key(brand), '' when none
  raw_name    TEXT NOT NULL,
  code        TEXT NOT NULL,
  available   INTEGER NOT NULL,
  seq         INTEGER NOT NULL,         -- file order
  PRIMARY KEY (product_key, brand_key, raw_name)
);
```
Register it: in `db/mod.rs` change `MIGRATIONS` to `&[include_str!("migrations/0001_init.sql"), include_str!("migrations/0002_names.sql")]`.

- [x] **Step 2: Write the failing tests** — in `db/mod.rs` tests:

```rust
    #[test]
    fn migrates_to_version_2_and_moves_old_links_under_their_name_key() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.user_version().unwrap(), 2);
        // simulate a row left behind by a v1 database
        db.with(|c| {
            c.execute_batch(
                "CREATE TABLE matches_old (supplier TEXT, code TEXT, product_id TEXT, product_name TEXT, how TEXT, similarity INTEGER, name_seen TEXT, decided_at TEXT);
                 INSERT INTO matches_old VALUES ('barezpakhsh', '3137006', 'p1', 'لنت جلو', 'AUTO', 91, 'لنت جلو پژو ۴۰۵ عظام* (16)', '2026-09-13T00:00:00Z');",
            )
        })
        .unwrap();
        db.backfill_name_keys().unwrap();
        let row = db
            .match_for_name("barezpakhsh", "لنت جلو پژو 405 عظام* (16)")
            .unwrap()
            .expect("found by the normalised name");
        assert_eq!(row.code, "3137006");
        assert_eq!(row.product_id.as_deref(), Some("p1"));
        assert_eq!(row.how, "AUTO");
        let gone: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'matches_old'", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(gone, 0);
    }
```
(If `open_in_memory` does not exist, use the temp-file constructor the existing tests use — read the top of the tests module and copy it.)

In `db/matches.rs` add a tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn links_are_found_by_name_then_by_code_and_carry_the_code_as_an_attribute() {
        let db = db();
        db.set_match("s", "لنت جلو پژو ۴۰۵ عظام* (16)", "3137006", Some(("p1", "لنت جلو")), "AUTO", Some(91))
            .unwrap();
        let by_name = db.match_for_name("s", "لنت جلو پژو 405 عظام* (16)").unwrap().unwrap();
        assert_eq!(by_name.name_key, "لنت جلو پژو 405 عظام* (16)");
        assert_eq!(by_name.code, "3137006");
        assert!(by_name.priced_at.is_none());
        let by_code = db.match_for_code("s", "3137006").unwrap().unwrap();
        assert_eq!(by_code.product_id.as_deref(), Some("p1"));
        assert!(db.match_for_code("s", "").unwrap().is_none(), "an empty code never matches");
        assert!(db.match_for_name("s", "چیز دیگر").unwrap().is_none());
    }

    #[test]
    fn a_second_decision_for_the_same_name_replaces_the_first() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("p1", "الف")), "AUTO", Some(80)).unwrap();
        db.set_match("s", "قطعه", "2", None, "NONE", None).unwrap();
        let row = db.match_for_name("s", "قطعه").unwrap().unwrap();
        assert_eq!((row.how.as_str(), row.code.as_str(), row.product_id), ("NONE", "2", None));
        assert_eq!(db.list_matches("s").unwrap().len(), 1);
        assert!(db.linked_matches("s").unwrap().is_empty());
    }

    #[test]
    fn touch_priced_stamps_the_link_and_clear_removes_it() {
        let db = db();
        db.set_match("s", "قطعه", "1", Some(("p1", "الف")), "USER", None).unwrap();
        db.touch_priced("s", "قطعه").unwrap();
        assert!(db.match_for_name("s", "قطعه").unwrap().unwrap().priced_at.is_some());
        db.clear_match("s", "قطعه").unwrap();
        assert!(db.match_for_name("s", "قطعه").unwrap().is_none());
    }
}
```

In `db/jobs.rs` tests (find the existing tests module):

```rust
    #[test]
    fn item_notes_are_appended_and_availability_is_read_back() {
        let db = Db::open_in_memory().unwrap();
        let job = db.create_job("REFRESH", None, None, "store", false).unwrap();
        let id = db.add_job_item(job, "1", "قطعه", 0).unwrap();
        db.add_item_note(id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد").unwrap();
        db.add_item_note(id, "زیر حداقل سود").unwrap();
        db.update_job_item(id, &[("available", crate::jobs::i(0))]).unwrap();
        let row = db.job_item(id).unwrap().unwrap();
        assert_eq!(row.note.as_deref(), Some("ناموجود؛ قیمت پایین‌تر نادیده گرفته شد؛ زیر حداقل سود"));
        assert_eq!(row.available, Some(false));
    }
```
(Use the job-creating helper the file's tests already use if its signature differs from `create_job(kind, mode, since, store, overwrite)`.)

- [x] **Step 3: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib db:: 2>&1 | grep -E 'error\[|cannot find|no method'`
Expected: unresolved `backfill_name_keys`, `match_for_name`, `match_for_code`, `touch_priced`, `add_item_note`, and `set_match` arity errors.

- [x] **Step 4: Implement**

`db/mod.rs` — after the migration loop in `migrate`, call the backfill; add the method:

```rust
    pub fn migrate(&self) -> Result<()> {
        // … existing loop unchanged …
        self.backfill_name_keys()
    }

    /// Migration 0002 renamed the code-keyed `matches` to `matches_old`; move its rows back under
    /// their name key (spec §14 a). Idempotent: nothing to do once the old table is gone.
    pub fn backfill_name_keys(&self) -> Result<()> {
        use crate::matching::normalize::name_key;
        self.with(|c| {
            let old: i64 = c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'matches_old'",
                [],
                |r| r.get(0),
            )?;
            if old == 0 {
                return Ok(());
            }
            let tx = c.unchecked_transaction()?;
            {
                let mut st = tx.prepare(
                    "SELECT supplier, code, product_id, product_name, how, similarity, name_seen, decided_at
                     FROM matches_old ORDER BY decided_at",
                )?;
                type Old = (String, String, Option<String>, Option<String>, String, Option<i64>, String, String);
                let rows: Vec<Old> = st
                    .query_map([], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                for (supplier, code, pid, pname, how, sim, name_seen, decided_at) in rows {
                    tx.execute(
                        "INSERT OR REPLACE INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        rusqlite::params![supplier, name_key(&name_seen), code, pid, pname, how, sim, name_seen, decided_at],
                    )?;
                }
            }
            tx.execute_batch("DROP TABLE matches_old")?;
            tx.commit()?;
            Ok(())
        })
    }
```

`db/matches.rs` — replace the struct, `COLS`, `row` and the methods:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchRow {
    pub supplier: String,
    /// `name_key(name_seen)` — the link's identity (spec §14 a).
    pub name_key: String,
    /// The supplier's code when the link last saw one; an attribute, not the key.
    pub code: String,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    /// AUTO | USER | NONE | MAP
    pub how: String,
    pub similarity: Option<i64>,
    pub name_seen: String,
    pub priced_at: Option<String>,
    pub decided_at: String,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MatchRow> {
    Ok(MatchRow {
        supplier: r.get(0)?,
        name_key: r.get(1)?,
        code: r.get(2)?,
        product_id: r.get(3)?,
        product_name: r.get(4)?,
        how: r.get(5)?,
        similarity: r.get(6)?,
        name_seen: r.get(7)?,
        priced_at: r.get(8)?,
        decided_at: r.get(9)?,
    })
}

const COLS: &str = "supplier, name_key, code, product_id, product_name, how, similarity, name_seen, priced_at, decided_at";

impl Db {
    pub fn match_for_name(&self, supplier: &str, name: &str) -> Result<Option<MatchRow>> {
        let key = name_key(name);
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND name_key = ?2"),
                params![supplier, key],
                row,
            )
            .optional()
        })
    }

    /// The most recently decided link that carries `code`; never for an empty code.
    pub fn match_for_code(&self, supplier: &str, code: &str) -> Result<Option<MatchRow>> {
        if code.is_empty() {
            return Ok(None);
        }
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND code = ?2 ORDER BY decided_at DESC LIMIT 1"),
                params![supplier, code],
                row,
            )
            .optional()
        })
    }

    /// `product` = `Some((id, name))` for a match, `None` with `how = "NONE"` for "not in Landau".
    /// Keyed on `name_key(name_seen)`; a second decision for the same name replaces the first.
    pub fn set_match(
        &self,
        supplier: &str,
        name_seen: &str,
        code: &str,
        product: Option<(&str, &str)>,
        how: &str,
        similarity: Option<i64>,
    ) -> Result<()> {
        let (pid, pname) = match product {
            Some((id, name)) => (Some(id), Some(name)),
            None => (None, None),
        };
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "INSERT INTO matches(supplier, name_key, code, product_id, product_name, how, similarity, name_seen, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(supplier, name_key) DO UPDATE SET code = excluded.code, product_id = excluded.product_id,
                   product_name = excluded.product_name, how = excluded.how, similarity = excluded.similarity,
                   name_seen = excluded.name_seen, decided_at = excluded.decided_at",
                params![supplier, key, code, pid, pname, how, similarity, name_seen, now()],
            )
            .map(|_| ())
        })
    }

    /// The link for `name_seen` answered a price just now (spec §14 c: refresh prefers it).
    pub fn touch_priced(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "UPDATE matches SET priced_at = ?3 WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key, now()],
            )
            .map(|_| ())
        })
    }

    pub fn clear_match(&self, supplier: &str, name_seen: &str) -> Result<()> {
        let key = name_key(name_seen);
        self.with(|c| {
            c.execute(
                "DELETE FROM matches WHERE supplier = ?1 AND name_key = ?2",
                params![supplier, key],
            )
            .map(|_| ())
        })
    }
    // list_matches and linked_matches: unchanged bodies (they select COLS).
}
```
(add `use crate::matching::normalize::name_key;` at the top.)

`db/jobs.rs` — add `pub available: Option<bool>, pub note: Option<String>` to `JobItemRow` (after `status` is fine; keep field order consistent with the row reader), append `, available, note` to `ITEM_COLS`, read them in the row mapper (`available: r.get::<_, Option<i64>>(21)?.map(|v| v != 0)`, `note: r.get(22)?` — count the columns), and add:

```rust
    /// Append a remark to the item's `note`, «؛»-separated (spec §14 g).
    pub fn add_item_note(&self, item_id: i64, note: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE job_items SET note = CASE WHEN note IS NULL OR note = '' THEN ?2 ELSE note || '؛ ' || ?2 END WHERE id = ?1",
                params![item_id, note],
            )
            .map(|_| ())
        })
    }
```

- [x] **Step 5: Fix every caller of the old `set_match`/`match_for`/`clear_match` signatures so the crate compiles** — `matching/mod.rs` (`stored`, `decide`, `record_user_pick`, `record_none`), `commands/mod.rs` (`match_set`, `match_none`, `match_clear`, `dashboard_get` if it counts matches), `jobs/refresh.rs` (`links`). Make the *minimal* edit here (pass the name as the key and the code as the attribute); Tasks 5 and 10 rewrite these properly.

- [x] **Step 6: Run to verify the tests pass**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED|panicked' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: all passed, clippy silent.

- [x] **Step 7: Commit**

```bash
git add src-tauri/src/db src-tauri/src/matching src-tauri/src/commands src-tauri/src/jobs
git commit -m "Key supplier links on the raw name; add item notes and the name-map table

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: The landau-parts name map — loader, command, settings section (§14 c)

**Files:**
- Create: `src-tauri/src/db/name_map.rs`
- Modify: `src-tauri/Cargo.toml` (make `csv` a normal dependency), `src-tauri/src/db/mod.rs` (`pub mod name_map;`), `src-tauri/src/commands/dto.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (register the command), `src-tauri/capabilities/default.json`, `app/types/bridge.ts`, `app/composables/useBridge.ts`, `app/pages/settings.vue`, `app/dev/mockBridge.ts`

**Interfaces:**
- Consumes: `name_key` (Task 1), table `name_map` (Task 3).
- Produces: `pub struct NameMapInfo { rows: i64, products: i64, loaded_at: Option<String>, file: Option<String> }`; `pub struct MapEntry { raw_name: String, code: String, available: bool }`; `pub fn parse_parts_map<R: std::io::Read>(r: R) -> Result<Vec<MapRow>>` with `pub struct MapRow { product_name, brand, raw_name, code, available }`; `Db::replace_name_map(&[MapRow], file: &str) -> Result<NameMapInfo>`; `Db::name_map_info()`; `Db::raw_names_for(product_name, brand) -> Result<Vec<MapEntry>>`; command `name_map_load(path) -> NameMapInfo`; `SettingsView.name_map: NameMapInfo`.

- [x] **Step 1: Dependency and capability**

In `src-tauri/Cargo.toml` change `csv = { version = "1", optional = true }` to `csv = "1"` and `live = ["dep:csv"]` to `live = []`. In `capabilities/default.json` add `"dialog:allow-open"` after `"dialog:allow-save"`.

- [x] **Step 2: Write the failing tests** — `src-tauri/src/db/name_map.rs` (new file, tests at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}code,raw_name,stock_status,price,state,attempts,last_error,product_id,product_name,brand,pack_qty,alias_of,duplicate_of_code\n\
CR39150201,ECU ای سی یو E5 EOBD موتور ECMT EC5 DAE AT6 پژو 207 کروز,موجود,118373640,done,0,,80,ECU موتور پژو 207 اتوماتیک AT6,کروز,1,,\n\
FIT0002601,آرمیچر استارت پراید AZE1540 کروز,ناموجود,1,done,0,,90,آرمیچر استارت پراید,کروز,1,,\n\
4100118,آرمیچر استارت پراید ایسکرا (کروز) 16360422,موجود,2,done,0,,90,آرمیچر استارت پراید,کروز,1,,\n\
9999,بدون محصول,موجود,3,pending,0,,,,,,,\n";

    #[test]
    fn parses_only_rows_that_name_a_product() {
        let rows = parse_parts_map(SAMPLE.as_bytes()).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].product_name, "ECU موتور پژو 207 اتوماتیک AT6");
        assert_eq!(rows[0].brand, "کروز");
        assert!(rows[0].available);
        assert!(!rows[1].available);
    }

    #[test]
    fn a_loaded_map_answers_raw_names_available_first_in_file_order() {
        let db = Db::open_in_memory().unwrap();
        let info = db.replace_name_map(&parse_parts_map(SAMPLE.as_bytes()).unwrap(), "/x/parts_map.csv").unwrap();
        assert_eq!((info.rows, info.products), (3, 2));
        assert_eq!(info.file.as_deref(), Some("/x/parts_map.csv"));
        assert!(info.loaded_at.is_some());
        let names = db.raw_names_for("آرميچر استارت پرايد", "كروز").unwrap();
        assert_eq!(
            names.iter().map(|e| e.raw_name.as_str()).collect::<Vec<_>>(),
            ["آرمیچر استارت پراید ایسکرا (کروز) 16360422", "آرمیچر استارت پراید AZE1540 کروز"]
        );
        assert_eq!(names[0].code, "4100118");
        assert!(db.raw_names_for("چیزی", "").unwrap().is_empty());
        // a second load replaces the first
        let info = db.replace_name_map(&[], "/y.csv").unwrap();
        assert_eq!(info.rows, 0);
        assert_eq!(db.name_map_info().unwrap().rows, 0);
    }
}
```

- [x] **Step 3: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib name_map 2>&1 | grep -E 'error|cannot find'`
Expected: compile errors for the missing items.

- [x] **Step 4: Implement** — top of `src-tauri/src/db/name_map.rs`:

```rust
//! The landau-parts map (spec §14 c): every raw Barezpakhsh name that `landau-parts` imported
//! under a Landau product name + brand. Loaded from `output/parts_map.csv` by the seller.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{now, Db};
use crate::errors::{BridgeError, Result};
use crate::matching::normalize::name_key;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NameMapInfo {
    pub rows: i64,
    pub products: i64,
    pub loaded_at: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    pub product_name: String,
    pub brand: String,
    pub raw_name: String,
    pub code: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapEntry {
    pub raw_name: String,
    pub code: String,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
struct Csv {
    code: String,
    raw_name: String,
    stock_status: String,
    product_name: String,
    brand: String,
}

/// Rows of `parts_map.csv` that name a product (state `done`/`alias`); a BOM is tolerated.
pub fn parse_parts_map<R: std::io::Read>(r: R) -> Result<Vec<MapRow>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(r);
    let mut out = Vec::new();
    for rec in reader.deserialize::<Csv>() {
        let rec = rec.map_err(|e| BridgeError::Other(format!("parts_map.csv: {e}")))?;
        if rec.product_name.trim().is_empty() || rec.raw_name.trim().is_empty() {
            continue;
        }
        out.push(MapRow {
            product_name: rec.product_name.trim().to_string(),
            brand: rec.brand.trim().to_string(),
            raw_name: rec.raw_name.trim().to_string(),
            code: rec.code.trim().to_string(),
            available: rec.stock_status.trim() == "موجود",
        });
    }
    Ok(out)
}

impl Db {
    /// Replace the whole map and record where and when it came from.
    pub fn replace_name_map(&self, rows: &[MapRow], file: &str) -> Result<NameMapInfo> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute_batch("DELETE FROM name_map")?;
            for (seq, r) in rows.iter().enumerate() {
                tx.execute(
                    "INSERT OR REPLACE INTO name_map(product_key, brand_key, raw_name, code, available, seq)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![name_key(&r.product_name), name_key(&r.brand), r.raw_name, r.code, r.available as i64, seq as i64],
                )?;
            }
            tx.execute("INSERT OR REPLACE INTO settings(key, value) VALUES ('name_map_file', ?1)", params![file])?;
            tx.execute("INSERT OR REPLACE INTO settings(key, value) VALUES ('name_map_loaded_at', ?1)", params![now()])?;
            tx.commit()?;
            Ok(())
        })?;
        self.name_map_info()
    }

    pub fn name_map_info(&self) -> Result<NameMapInfo> {
        self.with(|c| {
            let rows: i64 = c.query_row("SELECT count(*) FROM name_map", [], |r| r.get(0))?;
            let products: i64 = c.query_row(
                "SELECT count(*) FROM (SELECT DISTINCT product_key, brand_key FROM name_map)",
                [],
                |r| r.get(0),
            )?;
            let get = |k: &str| -> rusqlite::Result<Option<String>> {
                c.query_row("SELECT value FROM settings WHERE key = ?1", params![k], |r| r.get(0))
                    .optional()
            };
            Ok(NameMapInfo {
                rows,
                products,
                loaded_at: get("name_map_loaded_at")?,
                file: get("name_map_file")?,
            })
        })
    }

    /// The raw names imported under this product name + brand, available first, then file order.
    pub fn raw_names_for(&self, product_name: &str, brand: &str) -> Result<Vec<MapEntry>> {
        let (pk, bk) = (name_key(product_name), name_key(brand));
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT raw_name, code, available FROM name_map WHERE product_key = ?1 AND brand_key = ?2
                 ORDER BY available DESC, seq ASC",
            )?;
            let rows = st.query_map(params![pk, bk], |r| {
                Ok(MapEntry {
                    raw_name: r.get(0)?,
                    code: r.get(1)?,
                    available: r.get::<_, i64>(2)? != 0,
                })
            })?;
            rows.collect()
        })
    }
}
```
(`use rusqlite::OptionalExtension;` for `.optional()`.) Add `pub mod name_map;` to `db/mod.rs`.

- [x] **Step 5: Run to verify they pass**

Run: `cd src-tauri && cargo test --lib name_map`
Expected: `2 passed`.

- [x] **Step 6: Command and settings view**

`commands/dto.rs`: add `pub name_map: NameMapInfo` to `SettingsView` (import `crate::db::name_map::NameMapInfo`). `commands/mod.rs`: fill it in `settings_get`/`settings_save` wherever `SettingsView` is built (`name_map: state.db.name_map_info()?`), and add:

```rust
/// Load `landau-parts/output/parts_map.csv` (spec §14 c). The file is read here, never by the
/// webview, so no fs scope is needed for it.
#[tauri::command(rename_all = "snake_case")]
pub async fn name_map_load(state: State<'_, AppState>, path: String) -> Result<NameMapInfo> {
    let file = std::fs::File::open(&path)
        .map_err(|e| BridgeError::Other(format!("خواندن فایل نگاشت ممکن نشد: {e}")))?;
    let rows = crate::db::name_map::parse_parts_map(std::io::BufReader::new(file))?;
    if rows.is_empty() {
        return Err(BridgeError::Other(
            "این فایل هیچ ردیفی با نام محصول ندارد؛ parts_map.csv خروجی landau-parts را انتخاب کنید.".into(),
        ));
    }
    state.db.replace_name_map(&rows, &path)
}
```
Register `name_map_load` in the `generate_handler![…]` list in `lib.rs`.

- [x] **Step 7: Frontend**

`app/types/bridge.ts`: `export interface NameMapInfo { rows: number; products: number; loaded_at: string | null; file: string | null }` and `name_map: NameMapInfo;` on `SettingsView`. `app/composables/useBridge.ts`: `nameMapLoad: (path: string) => invoke<NameMapInfo>('name_map_load', { path })` following the file's pattern. `app/dev/mockBridge.ts`: add `name_map: { rows: 29472, products: 28643, loaded_at: '2026-09-15T12:00:00Z', file: '/home/seller/parts_map.csv' }` to the mock settings and a `nameMapLoad` that returns it.

`app/pages/settings.vue`: import `open` from `@tauri-apps/plugin-dialog`; add after the «فقط فاکتورهای از تاریخ شروع» field, inside the same section:

```vue
          <UFormField label="نگاشت نام‌های بارزپخش" hint="فایل parts_map.csv خروجی landau-parts؛ ردیف‌های قفسه‌ای که هیچ فاکتوری ندارند از این نگاشت به نام بارزپخش وصل می‌شوند">
            <div class="flex flex-wrap items-center gap-3">
              <UButton size="sm" color="neutral" variant="outline" icon="i-lucide-file-up" :loading="busy === 'map'" @click="loadMap">بارگذاری فایل نگاشت</UButton>
              <span v-if="view?.name_map.rows" class="text-sm text-paper-mute">{{ faNumber(view.name_map.rows) }} نام برای {{ faNumber(view.name_map.products) }} محصول · {{ faDateTime(view.name_map.loaded_at) }}</span>
              <span v-else class="text-sm text-paper-mute">هنوز بارگذاری نشده</span>
            </div>
          </UFormField>
```
and in the script:

```ts
const loadMap = async () => {
  error.value = null;
  try {
    const path = await open({ multiple: false, directory: false, filters: [{ name: 'CSV', extensions: ['csv'] }] });
    if (!path) return;
    busy.value = 'map';
    const info = await bridge.nameMapLoad(typeof path === 'string' ? path : path.path);
    if (view.value) view.value = { ...view.value, name_map: info };
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};
```
(`busy` already exists as a string union in this page; extend its type with `'map'`. `faDateTime` exists in `app/utils/fa.ts` — if its name differs, use the page's existing date formatter.)

- [x] **Step 8: Typecheck, tests, clippy**

Run: `pnpm typecheck && pnpm test 2>&1 | tail -3 && cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: clean.

- [x] **Step 9: Commit**

```bash
git add -A src-tauri app
git commit -m "Load the landau-parts name map from Settings

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Matcher and match commands by name (§14 a)

**Files:**
- Modify: `src-tauri/src/matching/mod.rs`, `src-tauri/src/commands/mod.rs`, `app/types/bridge.ts`, `app/composables/useBridge.ts`, `app/pages/matches.vue`, `app/utils/fa.ts`, `app/dev/mockBridge.ts`

**Interfaces:**
- Consumes: `Db::match_for_name/match_for_code/set_match` (Task 3).
- Produces: `Matcher::stored(name, code) -> Result<Option<MatchRow>>`, `Matcher::record_user_pick(name_seen, code, product_id, product_name, similarity)`, `Matcher::record_none(name_seen, code)`; commands `match_set(name_seen, code, product_id, product_name)`, `match_none(name_seen, code)`, `match_clear(name_seen)`.

- [x] **Step 1: Write the failing test** — in `matching/mod.rs` tests (the module has tests with a fake `search` closure; reuse its helper that builds a `RegistryPage`):

```rust
    #[tokio::test]
    async fn a_stored_decision_is_found_by_name_then_by_code_and_re_keyed_under_the_new_name() {
        let db = Db::open_in_memory().unwrap();
        let m = Matcher::new(db.clone(), "s", 80);
        db.set_match("s", "لنت جلو پژو 405 عظام* (16)", "3137006", Some(("p1", "لنت جلو پژو 405")), "USER", None).unwrap();
        let never = |_q: String, _l: usize, _s: usize| async { panic!("no search for a stored decision") };
        // same name, new code → found by name; the code is refreshed
        let d = m.decide(&never, "NEW-1", "لنت جلو پژو ۴۰۵ عظام* (16)").await.unwrap();
        assert!(matches!(d, Decision::Matched { ref product_id, .. } if product_id == "p1"));
        assert_eq!(db.match_for_name("s", "لنت جلو پژو 405 عظام* (16)").unwrap().unwrap().code, "NEW-1");
        // same code, new name → found by code; a link is stored under the new name too
        let d = m.decide(&never, "NEW-1", "لنت جلو 405 عظام (16 عددی)").await.unwrap();
        assert!(matches!(d, Decision::Matched { ref product_id, .. } if product_id == "p1"));
        assert_eq!(db.match_for_name("s", "لنت جلو 405 عظام (16 عددی)").unwrap().unwrap().how, "USER");
    }
```

- [x] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib a_stored_decision_is_found_by_name 2>&1 | grep -E 'error|panicked|FAILED'`
Expected: fails (compile error on `decide`'s lookup or a panic from the search closure).

- [x] **Step 3: Implement** — in `matching/mod.rs`:

```rust
    /// The stored decision for this name, else for this code (the site renamed the SKU); a hit
    /// under the other key is re-stored under the current name with the current code (§14 a).
    pub fn stored(&self, name: &str, code: &str) -> Result<Option<MatchRow>> {
        if let Some(row) = self.db.match_for_name(&self.supplier, name)? {
            if !code.is_empty() && row.code != code {
                let product = row.product_id.as_deref().zip(row.product_name.as_deref());
                self.db.set_match(&self.supplier, name, code, product, &row.how, row.similarity)?;
            }
            return Ok(Some(row));
        }
        if let Some(row) = self.db.match_for_code(&self.supplier, code)? {
            let product = row.product_id.as_deref().zip(row.product_name.as_deref());
            self.db.set_match(&self.supplier, name, code, product, &row.how, row.similarity)?;
            return Ok(Some(row));
        }
        Ok(None)
    }
```
In `decide`, replace `self.stored(code)?` with `self.stored(name, code)?`; the AUTO `set_match` call becomes `self.db.set_match(&self.supplier, name, code, Some((&top.candidate.id, &top.candidate.name)), "AUTO", Some(top.similarity as i64))?`. `record_user_pick(&self, name_seen, code, product_id, product_name, similarity)` and `record_none(&self, name_seen, code)` call `set_match` the same way with `"USER"` / `"NONE"`.

`commands/mod.rs`: `review_decide` passes `(&item.name, &item.code, …)`; `match_set(state, name_seen: String, code: String, product_id: String, product_name: String)` → `set_match(SUPPLIER_ID, &name_seen, &code, Some((&product_id, &product_name)), "USER", None)`; `match_none(state, name_seen, code)`; `match_clear(state, name_seen)`.

`tests/live_e2e.rs` `accept_top_candidates`: `record_user_pick(&item.name, &item.code, &id, &name, …)`.

- [x] **Step 4: Frontend** — `app/types/bridge.ts` `MatchRow`: add `name_key: string; priced_at: string | null;` and `how: 'AUTO' | 'USER' | 'NONE' | 'MAP'`. `useBridge.ts`: `matchSet(name_seen, code, product_id, product_name)`, `matchNone(name_seen, code)`, `matchClear(name_seen)`. `matches.vue`: `:key="r.name_key"`, calls pass `editing.value.name_seen` (+ `editing.value.code`), the `how` filter list gains `{ value: 'MAP', label: 'از نگاشت' }`. `fa.ts`: add the `MAP: 'از نگاشت'` label wherever `AUTO`/`USER`/`NONE` are labelled. `mockBridge.ts`: add `name_key`/`priced_at` to mock match rows.

- [x] **Step 5: Verify**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'; cd .. && pnpm typecheck && pnpm test 2>&1 | tail -3`
Expected: clean.

- [x] **Step 6: Commit**

```bash
git add -A src-tauri app
git commit -m "Look decisions up by name, then by code

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Pricing rules, the margin setting and the hard preflight (§14 b, d, e)

**Files:**
- Create: `src-tauri/src/jobs/pricing.rs`
- Modify: `src-tauri/src/jobs/mod.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/errors.rs`, `src-tauri/src/commands/dto.rs`, `src-tauri/src/commands/mod.rs`, `app/types/bridge.ts`, `app/pages/settings.vue`, `app/utils/fa.ts`, `app/dev/mockBridge.ts`

**Interfaces:**
- Consumes: `quotes_by_name`/`price_of` (Task 2), `name_key` (Task 1), `Db::touch_priced` (Task 3).
- Produces: in `jobs/pricing.rs`: `pick_by_name(name, Vec<PriceQuote>) -> Option<PriceQuote>`, `effective_cost(quote_cost, available, current: Option<i64>) -> (i64, bool)`, `floor_price(cost, margin_percent) -> i64`, `below_min_margin(price, cost, margin_percent) -> bool`, `pub const ROUNDING: i64 = 1000`; in `jobs/mod.rs`: `pub struct Costed { cost, available, code, name, renamed: bool }`, `pub async fn quote_for(ctx, svc, name, code, vat) -> Result<Option<Costed>>` (replaces `cost_for`); `Settings.min_margin_percent: f64` (+ patch/view/UI); `BridgeError::NoPricingRule` returned by `preflight`; `kinds::BELOW_MIN_MARGIN = "BelowMinMargin"`.

- [x] **Step 1: Write the failing tests** — `src-tauri/src/jobs/pricing.rs` (new file; tests at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn q(name: &str, code: &str, price: i64, available: bool) -> PriceQuote {
        PriceQuote { code: code.into(), name: name.into(), list_price: price, vat_percent: Some(10.0), has_vat: true, available, unit: "عدد".into() }
    }

    #[test]
    fn picks_the_equal_name_available_first_then_the_highest_price() {
        let quotes = vec![
            q("لنت جلو پژو 405 عظام* (16)", "A", 900, false),
            q("لنت جلو پژو 405 عظام* (16) طرح جدید", "B", 999, true),
            q("لنت جلو پژو ۴۰۵ عظام* (16)", "C", 800, true),
            q("لنت جلو پژو 405 عظام* (16)", "D", 850, true),
        ];
        assert_eq!(pick_by_name("لنت جلو پژو 405 عظام* (16)", quotes.clone()).unwrap().code, "D");
        assert!(pick_by_name("چیز دیگر", quotes).is_none());
    }

    #[test]
    fn an_unavailable_quote_only_raises_the_cost() {
        assert_eq!(effective_cost(900, false, Some(1000)), (1000, true));
        assert_eq!(effective_cost(1100, false, Some(1000)), (1100, false));
        assert_eq!(effective_cost(900, true, Some(1000)), (900, false));
        assert_eq!(effective_cost(900, false, None), (900, false));
    }

    #[test]
    fn the_provisional_price_is_the_cost_plus_the_margin_rounded_up_to_a_thousand() {
        assert_eq!(floor_price(1_000_000, 15.0), 1_150_000);
        assert_eq!(floor_price(5_370_200, 15.0), 6_176_000);
        assert_eq!(floor_price(10_000, 15.0), 12_000);
        assert_eq!(floor_price(0, 15.0), 0);
    }

    #[test]
    fn below_min_margin_tolerates_half_a_rounding_step() {
        assert!(!below_min_margin(1_150_000, 1_000_000, 15.0));
        assert!(below_min_margin(1_149_000, 1_000_000, 15.0));
        assert!(below_min_margin(1_000_000, 1_000_000, 15.0));
        assert!(!below_min_margin(11_000, 10_000, 15.0), "a 500-rial shortfall is rounding, not a thin margin");
        assert!(!below_min_margin(3_669_000, 3_191_100, 15.0), "Landau rounding to nearest, not up");
    }
}
```

- [x] **Step 2: Run to verify they fail**

Add `pub mod pricing;` to `jobs/mod.rs`. Run: `cd src-tauri && cargo test --lib pricing::tests 2>&1 | grep -E 'cannot find|error'`
Expected: unresolved functions.

- [x] **Step 3: Implement `pricing.rs`**

```rust
//! Pure pricing rules (spec §14 b, d, e): which search row is "the" quote, when an unavailable
//! quote is applied, the provisional attach price and the margin audit.

use crate::domain::PriceQuote;
use crate::matching::normalize::name_key;

/// Landau's rounding step and ours.
pub const ROUNDING: i64 = 1000;

/// The row whose normalised name equals `name`'s; several → available first, then the highest
/// list price (the reference scraper keeps the maximum price per name).
pub fn pick_by_name(name: &str, quotes: Vec<PriceQuote>) -> Option<PriceQuote> {
    let key = name_key(name);
    let mut same: Vec<PriceQuote> = quotes
        .into_iter()
        .filter(|q| name_key(&q.name) == key)
        .collect();
    same.sort_by(|a, b| {
        b.available
            .cmp(&a.available)
            .then(b.list_price.cmp(&a.list_price))
    });
    same.into_iter().next()
}

/// `(cost to write, held)`: an unavailable quote that would lower a row's current cost is held
/// back — it is stale and will be relisted higher (§14 d).
pub fn effective_cost(quote_cost: i64, available: bool, current: Option<i64>) -> (i64, bool) {
    match current {
        Some(c) if !available && quote_cost < c => (c, true),
        _ => (quote_cost, false),
    }
}

/// The provisional price a new row is created with: cost plus the minimum margin, rounded up to
/// [`ROUNDING`]. Landau's rule overrides it on the write (§14 e, f).
pub fn floor_price(cost: i64, margin_percent: f64) -> i64 {
    let raw = cost as f64 * (1.0 + margin_percent / 100.0);
    (raw / ROUNDING as f64).ceil() as i64 * ROUNDING
}

/// Whether a final price leaves less than the minimum margin, allowing half a rounding step for
/// Landau rounding to nearest rather than up.
pub fn below_min_margin(price: i64, cost: i64, margin_percent: f64) -> bool {
    (price as f64) + (ROUNDING as f64) / 2.0 < cost as f64 * (1.0 + margin_percent / 100.0)
}
```

- [x] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test --lib pricing::tests`
Expected: `4 passed`.

- [x] **Step 5: `quote_for`, the setting, the error** — in `jobs/mod.rs` replace `cost_for` with:

```rust
/// One priced supplier row for an item (spec §14 b): by name first, then by code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Costed {
    /// Whole rials, VAT applied per the setting (before the §14 d hold, which needs the row).
    pub cost: i64,
    pub available: bool,
    /// The row's code and name as the site lists them today.
    pub code: String,
    pub name: String,
    /// Found by code under a different name — the site renamed the SKU.
    pub renamed: bool,
}

/// Today's quote for `name`/`code`: the equal-name row of a name search, else the exact code
/// (§14 b). Records the snapshot, notes a VAT-rate difference in `vat`, and stamps the link as
/// priced. `None` when the site lists neither.
pub async fn quote_for(
    ctx: &JobCtx,
    svc: &Services,
    name: &str,
    code: &str,
    vat: &mut VatMismatches,
) -> Result<Option<Costed>> {
    let by_name = pricing::pick_by_name(name, svc.supplier.quotes_by_name(name).await?);
    let (q, renamed) = match by_name {
        Some(q) => (q, false),
        None => match svc.supplier.price_of(code).await? {
            Some(q) if !code.is_empty() => (q, true),
            _ => return Ok(None),
        },
    };
    let setting = svc.settings.vat_percent;
    let applied = if q.has_vat { setting } else { 0.0 };
    if q.has_vat {
        if let Some(site) = q.vat_percent {
            if (site - setting).abs() > 0.01 {
                tracing::warn!(code = %q.code, site, setting, "vat rate differs from the setting");
                vat.codes.push((q.code.clone(), site));
            }
        }
    }
    let cost = cost_of(q.list_price, applied);
    ctx.db.add_price_snapshot(SUPPLIER_ID, &q.code, q.list_price, cost, q.vat_percent, q.available, &crate::db::now())?;
    ctx.db.touch_priced(SUPPLIER_ID, name)?;
    Ok(Some(Costed { cost, available: q.available, code: q.code, name: q.name, renamed }))
}
```
(`price_of("")` is never sent: guard with `if code.is_empty() { return Ok(None) }` before the code branch.)

`settings.rs`: `pub min_margin_percent: f64` on `Settings` (`DEFAULT_MIN_MARGIN_PERCENT: f64 = 15.0`, key `min_margin_percent`, same load/save pattern as `vat_percent`, clamp to `0.0..=1000.0`), `pub min_margin_percent: Option<f64>` on `SettingsPatch`; extend the settings test that asserts defaults with `assert_eq!(s.min_margin_percent, 15.0)`.

`errors.rs`: add `#[error("no active pricing rule")] NoPricingRule,` to `BridgeError`; `kind()` → `"NoPricingRule"`; `message_fa()` → `"در لاندا هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید. اول یک قاعده بسازید تا قفسه بدون سود نماند."`; add `pub const BELOW_MIN_MARGIN: &str = "BelowMinMargin";` to `kinds`.

`jobs/mod.rs` `preflight`: replace the `if active == 0 { ctx.warn(…) }` with `if active == 0 { return Err(BridgeError::NoPricingRule); }` and update its doc comment (§14 e). Delete `kinds::NO_PRICING_RULE` only if nothing else uses it (grep).

`commands/dto.rs` + `commands/mod.rs`: `min_margin_percent` on `SettingsView`, filled from settings. `app/types/bridge.ts`: `min_margin_percent: number` on `SettingsView` and `SettingsPatch`. `settings.vue`: `min_margin_percent: 15` in `form`, copied on load, and a field right after «نرخ ارزش افزوده»:

```vue
            <UFormField label="حداقل سود خرده‌فروش (٪)" hint="ردیف تازه با این سود روی قفسه می‌آید تا قاعدهٔ لاندا قیمت نهایی را بگذارد؛ ردیفی که زیر این سود بماند در گزارش هشدار می‌گیرد">
              <UInput v-model.number="form.min_margin_percent" type="number" min="0" max="1000" step="0.5" dir="ltr" class="w-32" />
            </UFormField>
```
`fa.ts`: kind labels `BelowMinMargin: 'زیر حداقل سود'` and (if the kind map lacks it) `NoPricingRule: 'بدون قاعدهٔ قیمت‌گذاری'`. `mockBridge.ts`: `min_margin_percent: 15`.

- [x] **Step 6: Make the crate compile** — every `cost_for(ctx, svc, code, &mut vat)` caller (import PRICE loop, refresh PRICE loop) becomes `quote_for(ctx, svc, &item.name, &item.code, &mut vat)` mapping `Ok(Some(c))` to the old `cost` handling (`c.cost`) for now; Tasks 9–10 finish the wiring.

- [x] **Step 7: Verify**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'; cd .. && pnpm typecheck && pnpm test 2>&1 | tail -3`
Expected: clean.

- [x] **Step 8: Commit**

```bash
git add -A src-tauri app
git commit -m "Price by name, hold stale unavailable quotes, require a pricing rule

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Landau client for plan 107 (§14 f)

**Files:**
- Modify: `src-tauri/src/landau/mod.rs`

**Interfaces:**
- Produces: `ShelfRow.brand: Option<String>`; `AttachBody.price: i64` (sent as `price`); `LandauClient::patch(id, body) -> Result<ShelfRow>` (the reply row); `LandauClient::reprice_stocks(ids: &[String], overwrite: bool) -> Result<Vec<RepricedRow>>` (chunks of `REPRICE_STOCKS_MAX = 50`); `await_visible` and `MIRROR_WAIT` removed.

- [x] **Step 1: Write the failing tests** — in the `landau/mod.rs` tests module:

```rust
    #[test]
    fn a_write_reply_carries_the_rule_price_and_provenance() {
        // the PATCH reply measured live on 2026-09-15 (plan 107)
        let reply: Value = serde_json::from_str(r#"{"id":"6aa9","product":"6aa1","store":"6aa6","inventory":0,"price":2300000,"cost":2000000,
            "priced":{"cost":2000000,"cost_source":"SELLER","cost_at":"2026-09-15T18:01:25.469Z","source":"RULE","rule":"6aa7","at":"2026-09-15T18:01:25.478Z"},
            "condition":"NEW","authenticity":"ORIGINAL","type":"NORMAL"}"#).unwrap();
        let row = ShelfRow::from_item(&reply).unwrap();
        assert_eq!((row.price, row.cost), (Some(2_300_000), Some(2_000_000)));
        assert_eq!(row.price_source.as_deref(), Some("RULE"));
        assert!(row.brand.is_none(), "a raw row has no product object");
        let shelf_item: Value = serde_json::from_str(r#"{"stock":{"id":"s","product":"p","price":1,"cost":1},"product":{"id":"p","name":"سیبک","brand":"امیرنیا"}}"#).unwrap();
        let row = ShelfRow::from_item(&shelf_item).unwrap();
        assert_eq!(row.brand.as_deref(), Some("امیرنیا"));
        assert_eq!(row.product_name.as_deref(), Some("سیبک"));
    }

    #[test]
    fn attach_sends_the_provisional_price_and_reprice_bodies_are_chunked_by_fifty() {
        let body = AttachBody { product: "p".into(), store: "s".into(), inventory: 3, cost: 1_000_000, price: 1_150_000, authenticity: Authenticity::Original };
        assert_eq!(body.to_json()["price"], 1_150_000);
        assert_eq!(body.to_json()["cost"], 1_000_000);
        let ids: Vec<String> = (0..120).map(|i| format!("id{i}")).collect();
        let chunks = reprice_chunks(&ids);
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [50, 50, 20]);
    }
```

- [x] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib landau::tests 2>&1 | grep -E 'error|no field|cannot find'`
Expected: missing `price`, `brand`, `reprice_chunks`.

- [x] **Step 3: Implement**

```rust
pub const REPRICE_STOCKS_MAX: usize = 50;

/// `reprice {stocks}` takes at most fifty ids (plan 107).
pub fn reprice_chunks(ids: &[String]) -> Vec<Vec<String>> {
    ids.chunks(REPRICE_STOCKS_MAX).map(|c| c.to_vec()).collect()
}
```
`ShelfRow`: add `pub brand: Option<String>` (from `item.get("product").and_then(|p| s(p, "brand"))` in `from_item`; the test constructors in `jobs/import.rs` and `jobs/planner.rs` gain `brand: None`). `AttachBody`: add `pub price: i64`; `to_json` sends `"price": self.price` (update its doc: the provisional price, spec §14 e). `patch`:

```rust
    /// `PATCH /career/stocks/:id` — the reply is the row, priced by Landau's rule when the body
    /// carried `cost` (plan 107); never send `price` here (it would stamp the row MANUAL).
    pub async fn patch(&self, id: &str, body: &PatchBody) -> Result<ShelfRow> {
        let v = self
            .call(Method::PATCH, &format!("/career/stocks/{id}"), Some(&body.to_json()))
            .await?;
        ShelfRow::from_item(peel(&v)).ok_or_else(|| BridgeError::LandauRejected {
            status: 200,
            body: "reply without id".into(),
        })
    }

    /// `POST /career/pricing-rules/reprice {stocks, commit: true, overwrite}` in chunks of fifty,
    /// priced from the authoritative store — no mirror wait (spec §14 f). Every existing id comes
    /// back; a deleted one is absent.
    pub async fn reprice_stocks(&self, ids: &[String], overwrite: bool) -> Result<Vec<RepricedRow>> {
        let mut rows = Vec::new();
        for chunk in reprice_chunks(ids) {
            let body = json!({"stocks": chunk, "commit": true, "overwrite": overwrite});
            let v = self
                .call(Method::POST, "/career/pricing-rules/reprice", Some(&body))
                .await?;
            let page = peel(&v);
            let items = page.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_default();
            rows.extend(items.iter().filter_map(|r| {
                Some(RepricedRow {
                    stock: s(r, "stock")?,
                    product: s(r, "product").unwrap_or_default(),
                    price: n(r, "price"),
                    next: n(r, "next"),
                    cost: n(r, "cost"),
                    rule: s(r, "rule"),
                    skipped: s(r, "skipped"),
                })
            }));
        }
        Ok(rows)
    }
```
Delete `await_visible` and, in `jobs/mod.rs`, `MIRROR_WAIT`. Keep the store-wide `reprice` only if something still calls it after Task 8; otherwise delete it too.

- [x] **Step 4: Verify** (`patch`'s callers in `jobs/import.rs`/`refresh.rs` now get a `ShelfRow`; adjust `.map(|_| …)` to `.map(|_row| …)` for now — Tasks 9–10 use the row)

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: clean.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "Read the price off Landau's write replies and reprice by stock ids

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Targeted reprice and the margin audit (§14 e, f)

**Files:**
- Modify: `src-tauri/src/jobs/mod.rs`

**Interfaces:**
- Consumes: `reprice_stocks` (Task 7), `below_min_margin` (Task 6), `Db::add_item_note` (Task 3).
- Produces: `pub fn verdict_of(row: &RepricedRow) -> (String, Option<i64>)`; `pub async fn reprice_items(ctx, svc, items: &[(i64, String)]) -> Result<()>` (no `store` argument); `pub fn report(ctx, svc, items: &[(i64, String)]) -> Result<()>` (replaces `readback`; synchronous, no network).

- [x] **Step 1: Write the failing tests** — in `jobs/mod.rs` tests:

```rust
    fn repriced(stock: &str, price: Option<i64>, next: Option<i64>, skipped: Option<&str>) -> RepricedRow {
        RepricedRow { stock: stock.into(), product: "p".into(), price, next, cost: Some(1), rule: None, skipped: skipped.map(str::to_string) }
    }

    #[test]
    fn a_reprice_row_yields_a_verdict_and_the_final_price() {
        assert_eq!(verdict_of(&repriced("a", Some(100), Some(115), None)), ("REPRICED:115".into(), Some(115)));
        assert_eq!(verdict_of(&repriced("a", Some(100), None, Some("MANUAL"))), ("SKIPPED:MANUAL".into(), Some(100)));
        assert_eq!(verdict_of(&repriced("a", Some(100), None, Some("UNCHANGED"))), ("UNCHANGED".into(), Some(100)));
        assert_eq!(verdict_of(&repriced("a", Some(100), None, None)), ("UNCHANGED".into(), Some(100)));
    }

    #[test]
    fn the_report_marks_items_done_and_flags_a_thin_margin() {
        let db = Db::open_in_memory().unwrap();
        let ctx = JobCtx { db: db.clone(), app: None, job_id: db.create_job("REFRESH", None, None, "store", false).unwrap(), cancel: Arc::new(AtomicBool::new(false)) };
        let svc = test_services(15.0);
        let fine = db.add_job_item(ctx.job_id, "1", "خوب", 0).unwrap();
        db.update_job_item(fine, &[("cost", i(1_000_000)), ("price_after", i(1_150_000)), ("status", t("WRITTEN"))]).unwrap();
        let thin = db.add_job_item(ctx.job_id, "2", "کم‌سود", 0).unwrap();
        db.update_job_item(thin, &[("cost", i(1_000_000)), ("price_after", i(1_050_000)), ("status", t("WRITTEN"))]).unwrap();
        report(&ctx, &svc, &[(fine, "sa".into()), (thin, "sb".into())]).unwrap();
        let rows = db.job_items(ctx.job_id).unwrap();
        assert!(rows.iter().all(|r| r.status == "DONE"));
        assert_eq!(rows[0].note, None);
        assert_eq!(rows[1].note.as_deref(), Some("زیر حداقل سود"));
        let events = db.job_events(ctx.job_id).unwrap();
        assert!(events.iter().any(|e| e.level == "WARN" && e.message.contains("زیر حداقل سود")));
    }
```
`test_services(margin)` builds `Services { landau: LandauClient::new("http://127.0.0.1:1", "apt-test").unwrap(), supplier: Box::new(NoSupplier), settings: Settings { min_margin_percent: margin, ..test_settings() } }` where `NoSupplier` is a test-only `Supplier` impl whose every method returns `Err(BridgeError::Other("offline".into()))` (define it in the tests module) and `test_settings()` builds a `Settings` with the defaults (copy the field list from `Settings::load`). If a job-events reader is named differently from `job_events`, use that name.

- [x] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib jobs::tests 2>&1 | grep -E 'cannot find|error'`
Expected: `verdict_of`, `report` missing.

- [x] **Step 3: Implement** — replace `reprice_items` and `readback` in `jobs/mod.rs`:

```rust
/// `(reprice_verdict, final price)` of one row Landau answered (spec §14 f).
pub fn verdict_of(row: &RepricedRow) -> (String, Option<i64>) {
    let verdict = match (&row.next, row.skipped.as_deref()) {
        (Some(next), _) => format!("REPRICED:{next}"),
        (None, Some("UNCHANGED")) | (None, None) => "UNCHANGED".to_string(),
        (None, Some(why)) => format!("SKIPPED:{why}"),
    };
    (verdict, row.next.or(row.price))
}

/// Price the written rows by Landau's rules from the authoritative store (plan 107), stamp each
/// item's verdict and final price. `items` = (item_id, stock_id). A row Landau no longer has is
/// `MISSING`, warned once.
pub async fn reprice_items(ctx: &JobCtx, svc: &Services, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPRICE")?;
    if items.is_empty() {
        return Ok(());
    }
    let job = ctx.db.job(ctx.job_id)?.ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let ids: Vec<String> = items.iter().map(|(_, s)| s.clone()).collect();
    let rows = svc.landau.reprice_stocks(&ids, job.overwrite).await?;
    ctx.info("REPRICE", &format!("reprice answered {} of {} row(s)", rows.len(), ids.len()), None);
    let mut skipped: HashMap<String, Vec<String>> = HashMap::new();
    let mut missing: Vec<String> = Vec::new();
    for (item_id, stock_id) in items {
        ctx.check_cancel()?;
        let Some(row) = rows.iter().find(|r| &r.stock == stock_id) else {
            missing.push(stock_id.clone());
            ctx.item(*item_id, &[("reprice_verdict", t("MISSING"))])?;
            continue;
        };
        if let Some(why) = row.skipped.as_deref().filter(|w| *w != "UNCHANGED") {
            skipped.entry(why.to_string()).or_default().push(stock_id.clone());
        }
        let (verdict, price) = verdict_of(row);
        ctx.item(*item_id, &[("reprice_verdict", t(&verdict)), ("price_after", opt_i(price))])?;
    }
    if !skipped.is_empty() {
        let count: usize = skipped.values().map(Vec::len).sum();
        ctx.warn("REPRICE", kinds::REPRICE_SKIPPED, &format!("{count} ردیف در قیمت‌گذاری رد شد."), Some(json!(skipped)));
    }
    if !missing.is_empty() {
        ctx.warn("REPRICE", kinds::REPRICE_SKIPPED, &format!("{} ردیف دیگر روی قفسه نیست.", missing.len()), Some(json!({"missing": missing})));
    }
    Ok(())
}

/// Mark the written items DONE and audit the margin (spec §14 e): a final price under the
/// minimum is noted on the item and counted in one warning.
pub fn report(ctx: &JobCtx, svc: &Services, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPORT")?;
    let margin = svc.settings.min_margin_percent;
    let mut thin: Vec<Value> = Vec::new();
    for (item_id, stock_id) in items {
        let Some(row) = ctx.db.job_item(*item_id)? else { continue };
        if let (Some(cost), Some(price)) = (row.cost, row.price_after) {
            if pricing::below_min_margin(price, cost, margin) {
                ctx.db.add_item_note(*item_id, "زیر حداقل سود")?;
                thin.push(json!({"item_id": item_id, "stock_id": stock_id, "cost": cost, "price": price}));
            }
        }
        ctx.item(*item_id, &[("status", t("DONE"))])?;
    }
    if !thin.is_empty() {
        ctx.warn(
            "REPORT",
            kinds::BELOW_MIN_MARGIN,
            &format!("{} ردیف زیر حداقل سود ({margin}٪) قیمت خورده است؛ قیمت دستی یا قاعدهٔ کم‌سود را بررسی کنید.", thin.len()),
            Some(json!(thin)),
        );
    }
    Ok(())
}
```
Update the two jobs' calls to `reprice_items(ctx, svc, &written)` / `report(ctx, svc, &written)`; remove the now-unused `Duration` import if any.

- [x] **Step 4: Verify**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: clean.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "Reprice the written rows by id and audit the margin in the report

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Import job wiring (§14 b, d, e, f)

**Files:**
- Modify: `src-tauri/src/jobs/import.rs`

**Interfaces:**
- Consumes: `quote_for`/`Costed`, `pricing::{effective_cost, floor_price}`, `AttachBody.price`, `patch -> ShelfRow`, `Db::add_item_note`.

- [x] **Step 1: PRICE phase** — in `commit`, the match arm becomes:

```rust
        match quote_for(ctx, svc, &item.name, &item.code, &mut vat).await {
            Ok(Some(c)) => {
                let patch = [("cost", i(c.cost)), ("available", i(c.available as i64))];
                if c.code != item.code && !item.code.is_empty() {
                    ctx.db.add_item_note(item.id, &format!("کد بارزپخش عوض شد: {} ← {}", item.code, c.code))?;
                }
                if c.renamed {
                    ctx.db.add_item_note(item.id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?;
                }
                if !c.available {
                    ctx.db.add_item_note(item.id, "ناموجود در بارزپخش")?;
                }
                ctx.item(item.id, &patch)?
            }
            Ok(None) => { /* unchanged: NoCurrentPrice → SKIPPED */ }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => { /* unchanged: FAILED */ }
        }
```
(`NoCurrentPrice { code }` keeps the code; when the code is empty use the name: `code: if item.code.is_empty() { item.name.clone() } else { item.code.clone() }`.)

- [x] **Step 2: WRITE phase** — right after `let existing = find_row(&shelf, &product, authenticity.as_str());` compute the cost to write:

```rust
        let quoted = item.cost.unwrap_or(0);
        let (cost, held) = pricing::effective_cost(quoted, item.available.unwrap_or(true), existing.and_then(|r| r.cost));
        if held {
            ctx.db.add_item_note(item.id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?;
            ctx.item(item.id, &[("cost", i(cost))])?;
        }
```
and delete the earlier `let cost = item.cost.unwrap_or(0);`. `AttachBody` gains `price: pricing::floor_price(cost, svc.settings.min_margin_percent)`. Every `Written` gains `price_after: Option<i64>` taken from the reply row: `attach` → `row.price`; the three `patch(...)` calls → `.map(|row| Written { …, price_after: row.price, … })`. In the success arm add `("price_after", opt_i(price_after))` to the `ctx.item(...)` patch. (In the restore branch, `existing` is `None`, so the quote is applied as-is — the restored row is a fresh attach, §13 b.)

- [x] **Step 3: Update the test constructors** (`row(...)` in this file's tests gains `brand: None`) and run

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: clean.

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/jobs/import.rs
git commit -m "Import: price by name, hold stale unavailable quotes, attach with a margin

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Refresh job wiring — links by name, map resolution (§14 a–f)

**Files:**
- Modify: `src-tauri/src/jobs/refresh.rs`

**Interfaces:**
- Consumes: `Db::linked_matches`, `Db::raw_names_for`, `Db::set_match`, `quote_for`, `pricing::effective_cost`, `patch -> ShelfRow`, `reprice_items`, `report`.
- Produces: `pub fn choose_link(rows: Vec<MatchRow>, last_invoiced: &HashMap<String, String>) -> HashMap<String, (MatchRow, usize)>` (product_id → the chosen link and how many links the product had).

- [x] **Step 1: Write the failing test** — in `refresh.rs` tests (create the module if absent):

```rust
    fn link(name: &str, code: &str, product: &str, priced_at: Option<&str>) -> MatchRow {
        MatchRow { supplier: "s".into(), name_key: name.into(), code: code.into(), product_id: Some(product.into()), product_name: Some("p".into()),
            how: "AUTO".into(), similarity: None, name_seen: name.into(), priced_at: priced_at.map(str::to_string), decided_at: "2026-09-01T00:00:00Z".into() }
    }

    #[test]
    fn the_most_recently_priced_link_wins_then_the_most_recently_invoiced() {
        let rows = vec![
            link("قدیمی", "1", "p1", Some("2026-09-01T00:00:00Z")),
            link("تازه", "2", "p1", Some("2026-09-15T00:00:00Z")),
            link("الف", "3", "p2", None),
            link("ب", "4", "p2", None),
        ];
        let mut invoiced = HashMap::new();
        invoiced.insert("3".to_string(), "1404/01/01".to_string());
        invoiced.insert("4".to_string(), "1405/01/01".to_string());
        let chosen = choose_link(rows, &invoiced);
        assert_eq!((chosen["p1"].0.name_seen.as_str(), chosen["p1"].1), ("تازه", 2));
        assert_eq!((chosen["p2"].0.name_seen.as_str(), chosen["p2"].1), ("ب", 2));
    }
```

- [x] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib refresh::tests 2>&1 | grep -E 'cannot find|error'`
Expected: `choose_link` missing.

- [x] **Step 3: Implement** — replace `links` with:

```rust
/// product_id → the link to price it by, and how many links the product had. Several links for
/// one product: the most recently priced wins, then the most recently invoiced code (§14 c).
pub fn choose_link(rows: Vec<MatchRow>, last_invoiced: &HashMap<String, String>) -> HashMap<String, (MatchRow, usize)> {
    let mut by_product: HashMap<String, Vec<MatchRow>> = HashMap::new();
    for r in rows {
        if let Some(p) = r.product_id.clone() {
            by_product.entry(p).or_default().push(r);
        }
    }
    by_product
        .into_iter()
        .map(|(product, mut links)| {
            let n = links.len();
            links.sort_by(|a, b| {
                b.priced_at
                    .cmp(&a.priced_at)
                    .then_with(|| last_invoiced.get(&b.code).cmp(&last_invoiced.get(&a.code)))
            });
            (product, (links.swap_remove(0), n))
        })
        .collect()
}

fn links(ctx: &JobCtx) -> Result<HashMap<String, (MatchRow, usize)>> {
    let rows = ctx.db.linked_matches(SUPPLIER_ID)?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let last_invoiced: HashMap<String, String> = lines
        .iter()
        .map(|l| (l.code.clone(), l.date_jalali.clone()))
        .collect(); // oldest first → last write wins
    Ok(choose_link(rows, &last_invoiced))
}
```
In `run`, the PRICE loop per shelf row becomes:

```rust
        let name = row.product_name.clone().unwrap_or_else(|| row.product.clone());
        // 1. a stored link; 2. the landau-parts map (§14 c); 3. nothing → «بدون پیوند»
        let (link_name, link_code, quoted, from_map) = match links.get(&row.product) {
            Some((link, n)) => {
                if *n > 1 {
                    ctx.warn("PRICE", kinds::AMBIGUOUS_SUPPLIER_LINK,
                        &format!("قطعه {name} به {n} نام بارزپخش وصل است؛ «{}» (آخرین قیمت) ملاک شد.", link.name_seen),
                        Some(json!({"product": row.product, "chosen": link.name_seen})));
                }
                let q = match quote_for(ctx, svc, &link.name_seen, &link.code, &mut vat).await {
                    Ok(q) => q,
                    Err(e) if fatal(&e) => return Err(e),
                    Err(e) => { /* item FAILED as before, then `continue` */ }
                };
                (link.name_seen.clone(), link.code.clone(), q, false)
            }
            None => {
                let entries = ctx.db.raw_names_for(&name, row.brand.as_deref().unwrap_or(""))?;
                let mut found = None;
                for e in &entries {
                    match quote_for(ctx, svc, &e.raw_name, &e.code, &mut vat).await {
                        Ok(Some(q)) => { found = Some((e.raw_name.clone(), e.code.clone(), q)); break; }
                        Ok(None) => continue,
                        Err(e) if fatal(&e) => return Err(e),
                        Err(_) => continue,
                    }
                }
                match found {
                    Some((raw, code, q)) => {
                        ctx.db.set_match(SUPPLIER_ID, &raw, &code, Some((&row.product, &name)), "MAP", None)?;
                        (raw, code, Some(q), true)
                    }
                    None if entries.is_empty() => { /* SKIPPED, NO_SUPPLIER_LINK, message «این ردیف به هیچ نام بارزپخش وصل نیست و در نگاشت نام‌ها هم نبود.» — as the existing arm, then continue */ }
                    None => { /* SKIPPED, NoCurrentPrice { code: name } — none of the map's names priced; then continue */ }
                }
            }
        };
        let id = ctx.db.add_job_item(ctx.job_id, &link_code, &link_name, row.inventory)?;
        ctx.item(id, &[("stock_id", t(&row.id)), ("product_id", t(&row.product)), ("product_name", t(&name)), ("price_before", opt_i(row.price)), ("qty_before", i(row.inventory))])?;
        if from_map { ctx.db.add_item_note(id, "از نگاشت نام‌ها وصل شد")?; }
        match quoted {
            Some(c) => {
                let (cost, held) = pricing::effective_cost(c.cost, c.available, row.cost);
                if held { ctx.db.add_item_note(id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?; }
                else if !c.available { ctx.db.add_item_note(id, "ناموجود در بارزپخش")?; }
                if c.renamed { ctx.db.add_item_note(id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?; }
                if c.code != link_code && !link_code.is_empty() { ctx.db.add_item_note(id, &format!("کد بارزپخش عوض شد: {link_code} ← {}", c.code))?; }
                ctx.item(id, &[("cost", i(cost)), ("available", i(c.available as i64))])?;
                to_write.push((id, row.id.clone(), cost, row.cost == Some(cost), row.price));
            }
            None => { /* NoCurrentPrice → SKIPPED as before */ }
        }
```
(Write the two commented arms out in full, copying the existing SKIPPED/FAILED patches; the `NoCurrentPrice` code field carries the name when the code is empty.) `to_write` becomes `Vec<(i64, String, i64, bool, Option<i64>)>` (…, unchanged, current price). WRITE: an unchanged row stamps `("price_after", opt_i(*current_price))` with `SKIP`; a patched row uses the reply: `Ok(reply) => ctx.item(*item_id, &[("action", t("PATCH")), ("price_after", opt_i(reply.price)), ("status", t("WRITTEN"))])?`. Then `reprice_items(ctx, svc, &written).await?; report(ctx, svc, &written)?;`. The summary's `"linked"` count stays `!i.code.is_empty() || !i.name.is_empty()` → use `i.status != "SKIPPED" || i.error_kind.as_deref() != Some(kinds::NO_SUPPLIER_LINK)`; add `"from_map": all.iter().filter(|i| i.note.as_deref().map(|n| n.contains("از نگاشت")).unwrap_or(false)).count()`.

- [x] **Step 4: Verify**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'`
Expected: clean.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/jobs/refresh.rs
git commit -m "Refresh: choose the link by name, resolve unlinked rows through the name map

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Report UI — availability and notes (§14 g)

**Files:**
- Modify: `app/types/bridge.ts`, `app/components/ReportTable.vue`, `app/utils/fa.ts`, `app/dev/mockBridge.ts`, `app/utils/fa.spec.ts` (if a label test table exists)

- [x] **Step 1: Types and labels** — `JobItemRow` gains `available: boolean | null; note: string | null;`. `fa.ts`: `verdictLabel` handles `MISSING` → `'دیگر روی قفسه نیست'`; add `availabilityLabel = (a: boolean | null) => a == null ? '' : a ? 'موجود' : 'ناموجود'` beside the other labels. Mock items: `available: true, note: null` (one with `available: false, note: 'ناموجود؛ قیمت پایین‌تر نادیده گرفته شد'`).

- [x] **Step 2: Report table** — in the first cell, after the name line:

```vue
            <p v-if="i.available != null || i.note" class="mt-0.5 flex flex-wrap items-center gap-2 text-xs">
              <StatusPill v-if="i.available != null" :tone="i.available ? 'ok' : 'warn'" :label="availabilityLabel(i.available)" />
              <span v-if="i.note" class="text-paper-mute">{{ i.note }}</span>
            </p>
```
CSV export: add `'موجودی بارزپخش'` and `'یادداشت'` heads and `availabilityLabel(i.available), i.note` values; the search box also matches `i.note`.

- [x] **Step 3: Verify**

Run: `pnpm typecheck && pnpm test 2>&1 | tail -3`
Expected: clean. If the dev fixture mode exists (`?fixture=1` per the redesign plan), open `/jobs/<id>` in it and check the pill and note render on the right line.

- [x] **Step 4: Commit**

```bash
git add app
git commit -m "Show availability and notes in the report

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Live end-to-end, docs, runbook

**Files:**
- Modify: `src-tauri/tests/live_e2e.rs`, `README.md`, `docs/RUNBOOK.md`

- [x] **Step 1: Adapt the live e2e** — read `live_e2e.rs` end to end first. Then: the `Settings` literal gains `min_margin_percent: 15.0`; any `await_visible`/`MirrorTimeout` expectation goes; after the zero import assert for every DONE item with an `ATTACH` action that `price_after == Some(floor_price(cost, 15.0))` (the fixture rule is 15 %, rounding 1 000, so Landau's price equals the provisional one) and that the job has **no** `BelowMinMargin` warning; after the refresh assert every DONE item has `reprice_verdict` in `{"UNCHANGED", "REPRICED:…", "SKIPPED:MANUAL"}` and `price_after.is_some()`. Keep the idempotency test's assertions; its `accept_top_candidates` already uses the new `record_user_pick` signature (Task 5).

- [x] **Step 2: Run everything live**

```bash
export NO_PROXY='*'; eval "$(LND_ROLES_FILE=/home/vahid/Claude/.memory/.local/landau-roles.json node scripts/mint-apt.mjs)"
export BAREZ_USER=$(grep -o '"USER": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4); export BAREZ_PASS=$(grep -o '"PASS": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4)
cd src-tauri && sleep 5 && cargo test --features live -- --nocapture 2>&1 | grep -E 'test result|panicked|FAILED|running'
```
Expected: every live test file passes. If a test attaches rows it does not clean up, delete them the way `live_e2e.rs` already does.

- [x] **Step 3: Docs** — `README.md` (English, for installers/builders): in the feature list say prices are looked up by the Barezpakhsh name (codes may change), that a «ناموجود» price only ever raises a cost, that Settings needs an active Landau pricing rule and carries a minimum-margin figure, and add the «بارگذاری نگاشت نام‌ها» step with the path `landau-parts/output/parts_map.csv`. `docs/RUNBOOK.md`: in *Fixtures* add `6. **Name map** — Settings → «بارگذاری فایل نگاشت» → /home/vahid/WorkSpace/landau-parts/output/parts_map.csv`; in *Deviations from the spec* delete the «`price_of` is called once per shelf row» line (now one name search per row, still per row) and add: `- **Spec §14 supersedes §1/§7.2 on pricing**: no active rule fails preflight; `reprice` is the targeted `{stocks}` form (plan 107); the mirror is never awaited.` Also mention the schema change: a `bridge.sqlite` from before 2026-09-15 is migrated in place (links keep working).

- [x] **Step 4: Final check and commit**

Run: `cd src-tauri && cargo test 2>&1 | grep -E 'test result|FAILED' && cargo clippy --all-targets 2>&1 | grep -E '^(warning|error)'; cd .. && pnpm typecheck && pnpm test 2>&1 | tail -3 && git status --short`
Expected: clean, only the intended files modified.

```bash
git add -A
git commit -m "Live e2e for pricing at write; document names, margins and the name map

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

- [x] **Step 5: Tell the Landau agent** — append to `/home/vahid/conversation.txt` under `### [landau-bridge] <date>` that the bridge side landed on branch `names-and-margin`, what it keys on (`priced.source`, `reprice {stocks}`), and any Landau behaviour observed that differs from the completion message (none expected).
