# Landau Bridge — design

Date: 2026-09-13. Status: approved in brainstorming (sections 1–4), awaiting the implementation plan.

Landau Bridge is a desktop app (Tauri 2 shell, Nuxt 3 UI, Rust core) for a retail seller on the
Landau platform. It does two things:

1. **Import** the seller's Barezpakhsh invoices onto their Landau shelf.
2. **Refresh** shelf prices from the current Barezpakhsh wholesale price, on request.

Both use Landau's own pricing rules, Landau's own fuzzy part search, and persist every decision
and every error. The seller must be able to run it with no agent present: everything is explicit
in the UI and the code. The app has **no runtime dependency** on the Landau super-repo, the prior
scraper, or the parts importer; those were read once to learn the facts recorded here.

## 1. Decisions taken with the user

| Topic | Decision |
| --- | --- |
| Cost basis | Barezpakhsh list price × 1.10 (VAT). `cost = round(list_price * 1.10)` in whole rials. |
| Unpaid invoices («پرداخت نشده») | Skipped, listed in the report. |
| UI language | Persian only, RTL. |
| Architecture | Rust core (scraping, Landau client, matching, jobs, SQLite); Nuxt is a thin view. |
| Row variant on attach | `condition = NEW` always; `authenticity` = a default chosen once in settings (`ORIGINAL` or `FAKE`), editable per item in the review table. |
| No pricing rule in Landau | Mirror Landau's own delivery: price = cost, and a visible warning. |
| Manual prices in Landau | Respected by default (Landau's `MANUAL` stamp); an "overwrite manual prices" switch maps to Landau's `overwrite`. |
| Quantity unit | Invoice quantity is transferred as-is, in the invoice's unit. No pack multiplication (the registry does not know pack sizes). |
| Item with no current Barezpakhsh price | Not written; reported as `NoCurrentPrice`, retried on the next run. |

## 2. Facts learned from the references (the app's contract with the outside)

### 2.1 Landau (client gateway, e.g. `http://localhost:6150`)

- Auth: `Authorization: Bearer apt-…`. Keys are minted in the Landau app under Account → API keys,
  are `strict` (client gateway only), expire, can be revoked. A key created seconds ago answers
  `401` until activated; the first real call is the validity check — nothing probes `auth/apts`.
- Landau's own routes sit at the URL root; platform collections are behind `/-/`.
- `GET /career/staff/memberships` → `ActingMembership[]`: `{business, name, principal, audience,
  approved, stores: [{id, name, type, state, status}], scoped}`. The seller's outlets come from here.
- `GET /career/stocks/shelf?limit=100&skip=N` → `{items, total, limit, skip}`; `limit > 100` is a
  `400`; `?pagination[limit]` is silently ignored. Rows publish `condition`/`authenticity` (unpacked
  from the stored `name`), `cost`, `price`, `inventory`, `product`, `store`. Reads are ~1 s behind
  writes (CDC mirror).
- `POST /career/stocks` body `{product, store, type: 'NORMAL', inventory, price?, cost?, condition,
  authenticity}`; `condition`/`authenticity` are **required** on create. The reply is the row,
  **flat** (not `{data}`). On create, `price` is not stamped as manual; `cost` is folded into
  `props.pricing` with `cost_source: 'SELLER'`.
- `PATCH /career/stocks/:id` body `{inventory?, cost?, condition?, authenticity?}`. Sending `price`
  on a PATCH stamps the row `MANUAL` and Landau's rules skip it thereafter — **the bridge never
  sends `price` on a PATCH**. Omitting `cost` leaves the stored cost alone.
- `409` on create = the `(product, store, condition|authenticity)` tuple already exists.
- Unique tuple is `(name, product, store, …)` where `name` is the composed variant; one store may hold
  a `NEW|ORIGINAL` and a `NEW|FAKE` row of one part.
- `GET /career/pricing-rules` → the seller's rules (`props.active`, scope `SHELF|PRODUCT|BAND`, basis
  `MARKUP_PERCENT|FIXED_AMOUNT`, `value`, `rounding`, `discount`, `floor_at_cost`).
- `POST /career/pricing-rules/reprice` body `{store?, skip?, overwrite?, commit?}` → `{rows, total,
  changed, applied, limit: 100, skip}`; each row `{stock, product, price, cost, next?, rule?,
  floored?, skipped?: 'MANUAL'|'NO_COST'|'NO_RULE'|'UNCHANGED'}`. `commit` defaults to `false`
  (preview). It pages the **mirror**, so rows written a moment ago must be visible there first.
- Landau's arithmetic (server-side, never re-implemented here): margin → round to `rounding` step →
  time-boxed discount → round → floor at cost; precedence `PRODUCT > BAND > SHELF`, narrower band,
  newer rule, lower id. No matching rule → the row keeps its price; Landau's own wholesale delivery
  sets `price = cost` before rules run, and the bridge does the same on create.
- `POST /-/career/products/search?zone=client` body = Elasticsearch request:
  `{from, size, track_scores: true, query: {bool: {must: [{multi_match: {query, fields:
  ['name^3','alias^2','brand','categories'], fuzziness: 'AUTO', operator: 'OR'}}], filter:
  [{term: {state: 'APPROVED'}}]}}}` → `{total, hits: [{id, source, score}]}`. This is exactly the
  query Landau's own technician search sends. `_persian_` stopwords (`طبق`, `بالا`, `عقب`) are
  dropped by the analyzer; a zero-hit answer is not proof the part is absent.
- Registry rows (`career/products`) are definitions Landau authors: `name`, `alias` (`" | "`-joined
  nicknames), `brand`, `categories` (ASCII slugs), `features[]`. The seller cannot create them; a
  missing part needs a Landau support ticket.

### 2.2 Barezpakhsh (`https://33314466.ir`)

- ASP.NET MVC, server-rendered, jQuery unobtrusive ajax. No captcha on login (probed 2026-09-13).
- The site geo-blocks the developer machine's proxy exit with a static `403 Access Denied` page; the
  client must **bypass any HTTP proxy** for this host.
- Login: `GET /Home/Login` → hidden `__RequestVerificationToken`; `POST /Home/Login` form fields
  `__RequestVerificationToken, Username, Password` → `302 Location: /Shop/Home` on success.
- Invoices: `GET /Shop/Report/Bill` → form `{BusinessId, BusinessCode, FromDate (Jalali
  `YYYY/M/D`), ToDate, …}` posted to `/Shop/Report/Bill`; the «معین فاکتور» row's «جزییات...» opens
  the invoice list; list rows with class `success` link to invoice detail pages (the list loads more
  rows on scroll — the exact request is pinned by the first implementation task). Invoice page: date
  in the first table's header (`\d{4}/\d{1,2}/\d{1,2}`), payment state cell («پرداخت نشده» =
  unpaid), line columns 2–6 = code, name, quantity, unit, unit price; the last row is a total.
- Price list: `GET /Shop/Buy/Index` → form `{BusinessCode, SearchTerm, JustAvailable, SelectedCarCode,
  SelectedBrandCode, SelectedGroupCode, SelectedPageSize}`; table columns 4–8 = status
  (`موجود`/`ناموجود`), code, name, list price, VAT flag (`دارد`). The bridge searches by **code**
  with `JustAvailable = false`.
- Credentials for live tests are the ones in the prior scraper's code, supplied via env.

### 2.3 Matching data

`landau-parts/output/parts_map.csv` (54 515 rows: `code, raw_name, …, product_name, brand,
pack_qty`) records which registry name each Barezpakhsh SKU was imported under. It is the
calibration set for the threshold and nothing else — the app never reads it.

## 3. Architecture

```
landau-bridge/
├── package.json, nuxt.config.ts        # Nuxt 3, ssr:false, static output → Tauri
├── app/                                # Persian RTL UI: pages, components, composables
├── src-tauri/
│   ├── Cargo.toml, tauri.conf.json
│   ├── fixtures/barezpakhsh/*.html     # saved pages the parsers are tested against
│   └── src/
│       ├── lib.rs                      # Tauri setup, command registry, app state (db + config)
│       ├── commands/                   # thin: validate args → call domain → return DTOs
│       ├── supplier/                   # trait Supplier; barezpakhsh/{client,forms,parse}
│       ├── landau/                     # apt-token client
│       ├── matching/                   # normalize, query, similarity, threshold, decisions
│       ├── jobs/                       # import + refresh runners: steps, progress, cancel
│       ├── db/                         # rusqlite schema + repositories (embedded migrations)
│       ├── errors.rs                   # typed error kinds (§7)
│       └── log.rs                      # tracing → rolling file + job_events/errors tables
├── src-tauri/src/bin/calibrate.rs      # threshold calibration (feature `live`)
├── tests/live/                         # Rust integration tests behind `--features live`
└── docs/                               # this spec, matching-threshold.md, RUNBOOK.md
```

- One process. The webview calls Tauri commands (`settings_*`, `landau_validate`,
  `supplier_test_login`, `import_start`, `import_review_decide`, `import_continue`, `refresh_start`,
  `job_cancel`, `jobs_list`, `job_detail`, `matches_*`, `errors_list`) and listens to events
  (`job://progress`, `job://phase`, `job://done`). Jobs run on a Tokio task owned by the Rust
  side; state is written to SQLite at every step, and the UI re-reads it on open.
- `trait Supplier { fn id(); async fn login(); async fn invoices(); async fn price_of(code) }` with
  one implementation, `barezpakhsh`, registered by id. Every supplier-scoped row carries
  `supplier_id`; a second supplier is a new module plus one registry line.
- Networking: `reqwest` (cookie jar, `no_proxy` for the supplier host, retries with backoff, every
  request's method/URL/status logged). Landau calls go only through the client gateway.
- Config: gateway URL, API key, target store, supplier username/password, default authenticity,
  threshold, overwrite-manual switch, data folder. Secrets in the OS keyring (`keyring` crate);
  if no keyring service exists they are stored in the SQLite file and the settings screen says so.

## 4. Supplier module (Barezpakhsh)

- `login()`: token → POST → expect `302 /Shop/Home`. Static 403 block page → `SupplierBlocked`;
  200 with a validation message → `SupplierBadCredentials`; transport → `SupplierUnreachable`.
  A session that later answers the login page again is re-established once (`SupplierSessionExpired`
  logged), then the step fails.
- `invoices()`: `FromDate = 1390/01/01`, `ToDate = today` (Jalali). Returns invoice headers
  `{number, date_jalali, date_iso, paid, url}` and lines `{code, name, qty, unit, unit_price}`.
  Invoices are cached by number and never refetched unless the seller presses "fetch everything
  again". Lines with non-positive quantity are kept in the cache but excluded from aggregation.
- `price_of(code)`: search by code, `JustAvailable = false`; the row whose code equals the query →
  `{list_price, available}`; `cost = round(list_price × 1.10)`. Cached in `price_snapshots`; a job
  asks the site once per distinct code. No row → `NoCurrentPrice`.
- Text normalisation on read: Arabic ye/kaf → Persian, Persian/Arabic digits → Latin, ZWNJ → space,
  whitespace collapsed, tabs/commas neutralised.
- Every parser is a pure function over an HTML string, tested against fixtures in
  `src-tauri/fixtures/barezpakhsh/`. A missing element fails as `SupplierParse{page, field}`.

## 5. Landau client

- `validate()`: `GET /career/staff/memberships` → the approved membership(s) and their `RETAIL`
  stores. `401` → `LandauUnauthorized`; `403` → `LandauForbidden`; no approved membership →
  `BusinessNotApproved`; no store → `NoStore`.
- `shelf(store)`: page `GET /career/stocks/shelf` to `total`; index by `(product, condition,
  authenticity)`.
- `search(term)`: the query in §2.1, `size: 10`.
- `attach(row)` / `patch(id, {inventory?, cost?})` as in §2.1; `409` on attach → re-read the shelf
  and patch instead (`DuplicateRow` logged).
- `rules()`: `GET /career/pricing-rules`, counts active rules for the preflight warning.
- `reprice(store, overwrite)`: `commit: true`, paged by `skip` until `skip + limit ≥ total`.
- `await_visible(ids)`: poll the shelf until every id is present, 1 s apart, ≤ 15 s →
  `MirrorTimeout` (the job still reprices; rows not yet visible are reported as "not repriced").

## 6. Matching

- Search term = invoice name normalised as in §4, with pack markers («(N عددی)», «N عددی», «(N)»)
  and a trailing copy of the SKU removed.
- Candidates = Landau's hits. **Similarity** (0–100) = token-set ratio between the normalised term
  and each candidate's normalised `name` and each `alias`, best of them. One pure function,
  `matching::similarity(term, candidate) -> u8`, so calibration can replace it.
- **Threshold** (settings, 0–100): the top candidate is auto-accepted when its similarity ≥
  threshold; otherwise the item goes to review with up to 10 candidates and their similarities.
- **Calibration** (`cargo run --bin calibrate --features live`): sample 2 000 SKUs from the
  calibration set (§2.3), run the real search, and for thresholds 50…100 report auto-accept
  precision (top candidate's name equals the mapped product name after normalisation) and coverage.
  Choose the highest threshold with precision ≥ 98 %; write the table, the sample size, the date,
  and the reasoning to `docs/matching-threshold.md`, and set that value as the default in code.
- Decisions persist in `matches(supplier, code, product_id, product_name, how: AUTO|USER|NONE,
  similarity, name_seen, decided_at)`. An item with a decision is never searched again. `NONE`
  shows: «این قطعه در فهرست لندو نیست — برای افزودن آن یک تیکت برای پشتیبانی لندو ثبت کنید» and
  the item stays excluded until the seller changes the decision on the matches screen.

## 7. Jobs

### 7.1 Import

| Phase | What happens | Persisted |
| --- | --- | --- |
| Preflight | validate Landau key + store, supplier login, count active rules (0 → warning) | `jobs.status = RUNNING`, events |
| Fetch | unseen invoice numbers → fetch → parse | `invoices`, `invoice_lines` |
| Aggregate | mode **quantities since ‹date›**: qty per SKU = Σ positive lines over paid invoices in range not in `invoice_imports`; mode **zero**: every SKU over all paid invoices, qty 0 | `job_items` (planned) |
| Match | stored decisions → auto ≥ threshold → the rest to review; job pauses (`status = REVIEW`) | `matches`, `job_items` |
| Review (user) | pick / «هیچ‌کدام» / later; free-text re-search; per-item authenticity override | `matches` |
| Price | `price_of` per matched SKU | `price_snapshots` |
| Write | existing row → `PATCH {inventory: existing + qty, cost}` (zero mode: `{cost}`); missing → `POST {inventory: qty|0, price: cost, cost, condition: NEW, authenticity}` | `job_items.action/stock_id/qty_before/qty_after` |
| Reprice | `await_visible` → `reprice(store, overwrite)` paged; verdict per row | `job_items.reprice_verdict` |
| Report | read the shelf → final `price` per item; quantity mode → `invoice_imports` rows | `jobs.status = DONE`, `summary_json` |

### 7.2 Refresh

Preflight → rows of the store with a supplier link (`matches.product_id` → code; several codes for
one product → the most recently invoiced one, choice logged) → `price_of` each → `PATCH {cost}`
where the cost changed → `await_visible` → `reprice(store, overwrite)` → read back → report
old → new cost and price, skipped reasons, and rows with no supplier link.

### 7.3 Invariants

- Writes are derived from the live shelf on every run; only quantity addition is additive, and it
  is guarded by `invoice_imports` (one transfer per invoice, ever).
- A job interrupted by closing the app is marked `INTERRUPTED` at its last persisted step on the
  next launch; re-running it is safe.
- One job at a time; `job_cancel` stops before the next item and marks `CANCELLED`.

## 8. Persistence (SQLite, `rusqlite`, migrations embedded)

```
settings(key PK, value)
invoices(id PK, supplier, number, date_jalali, date_iso, paid, url, fetched_at, UNIQUE(supplier, number))
invoice_lines(id PK, invoice_id FK, line_no, code, name, qty, unit, unit_price)
supplier_items(supplier, code, last_name, first_seen, last_seen, PK(supplier, code))
price_snapshots(id PK, supplier, code, list_price, cost, available, fetched_at)
matches(supplier, code, product_id, product_name, how, similarity, name_seen, decided_at, PK(supplier, code))
jobs(id PK, kind, mode, since_date, store, overwrite, status, started_at, finished_at, summary_json)
job_items(id PK, job_id FK, code, name, qty, product_id, stock_id, authenticity, action, qty_before, qty_after, cost, price_before, price_after, reprice_verdict, error_kind, error_msg)
job_events(id PK, job_id FK, at, level, phase, message, detail_json)
invoice_imports(invoice_id FK, job_id FK, PK(invoice_id))
errors(id PK, at, job_id, kind, message, context_json)
```

## 9. Errors and logging

Typed kinds, each with a Persian message and a log line; every occurrence is a row in `errors`
(with context JSON) and, inside a job, a `job_events` row:

`SupplierUnreachable`, `SupplierBlocked`, `SupplierBadCredentials`, `SupplierSessionExpired`,
`SupplierParse{page, field}`, `NoCurrentPrice`, `InvoiceUnpaid`, `InvoiceAlreadyImported`,
`LineNonPositiveQty`, `LandauUnauthorized`, `LandauForbidden`, `LandauUnreachable`,
`BusinessNotApproved`, `NoStore`, `ProductNotApproved`, `DuplicateRow`, `MirrorTimeout`,
`NoPricingRule` (warning), `RepriceSkipped{reason}`, `Unmatched`, `NoneDecision`, `JobCancelled`,
`JobInterrupted`, `KeyringUnavailable` (warning), `SettingsIncomplete`.

Logging: `tracing` to a daily-rolling file in the data folder (request method/URL/status, never
secrets or tokens) and to the tables above. "Open log folder" on the errors screen.

## 10. UI (Nuxt 3 + Nuxt UI, `dir="rtl"`, Vazirmatn bundled, Persian numerals, Jalali dates)

1. **تنظیمات** — gateway URL + API key → validate → business + stores → pick store; supplier
   username/password → test login; default authenticity; threshold (calibrated default + one-line
   reason); overwrite-manual switch; data folder; keyring notice when applicable.
2. **داشبورد** — Landau ✓ (store name), supplier ✓, last import, last refresh, items awaiting
   review, unresolved errors; buttons «وارد کردن فاکتورها» and «به‌روزرسانی قیمت‌ها».
3. **وارد کردن فاکتورها** — mode + Jalali date → progress by phase → review table → continue →
   report (filters, CSV export).
4. **به‌روزرسانی قیمت‌ها** — options → progress → report.
5. **تطبیق‌ها** — decisions list, filters, change a decision, ticket instruction for «هیچ‌کدام».
6. **گزارش و خطاها** — jobs → detail (events + items); errors; open log folder.

## 11. Testing

- `cargo test`: parsers on fixtures; normalisation; similarity; cost rounding; aggregation; the
  write-planner (pure: shelf + items + mode → actions); repositories on in-memory SQLite.
- `cargo test --features live` (env: `LND_API_URL`, `LND_APT_TOKEN`, `BAREZ_USER`, `BAREZ_PASS`):
  supplier login → invoices → price lookup; Landau validate → shelf → search → attach → patch →
  reprice → read-back on the retail-supplier fixture store; a zero-mode import over a bounded
  invoice subset, then a refresh; test rows soft-deleted afterwards. `docs/RUNBOOK.md` explains
  staging Landau's fixtures (`stage` journey in the Landau workspace) and exporting the supplier
  credentials; a missing fixture fails loudly.
- Calibration binary (§6) writes `docs/matching-threshold.md`.
- UI: vitest on pure TS utils; manual smoke via `pnpm tauri dev`.
- Gates before "done": `cargo clippy -- -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm build`,
  `pnpm tauri build`.

## 12. Out of scope

Other suppliers (structure only), pack-size conversion, editing Landau pricing rules from the
bridge, creating registry definitions, a non-Persian UI, automated UI end-to-end tests.

## 13. Amendments (2026-09-13, while pinning the site for the plan)

- **Return invoices subtract.** Barezpakhsh's ledger carries sales invoices («فاکتور فروش», `DocAliasID` 2), returns («مرجوع از فروش», 6) and receipts («دريافت», 67). Sales add quantity, returns subtract it, receipts are not invoices. A net-negative item is clamped to 0 and reported (`NegativeQuantity`).
- **VAT is a setting, checked against the site.** The price search exposes each item's VAT rate (`TaxAndTollPercent`, `10.00` today) and whether VAT applies (`HasTaxAndToll`). The ×1.10 decision (§1) is kept as `vat_percent = 10` in settings; when the site's rate differs, a `VatRateMismatch` warning is recorded and the setting is applied. An item without VAT is costed at its list price.
- **Ledger and price list are JSON-wrapped XML endpoints.** `POST /Shop/Share/GradualMoein2Load` (15 rows per page, page count from the statement page's script) and `POST /Shop/Share/ItemsSearchGradual` (search by code, needs `businessId` and `userGroupId` read from one price-form reply). Both replace the "scroll" mechanics guessed in §2.2.
- **Fetch cap.** The import job accepts an optional `fetch_limit` (invoices fetched per run; the rest continue next run because invoices are cached). The UI does not expose it; the live end-to-end test uses it.
