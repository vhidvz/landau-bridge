# Landau Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri desktop app that imports a retail seller's Barezpakhsh invoices onto their Landau shelf and refreshes shelf prices from the current wholesale price, using Landau's own pricing rules and fuzzy part search.

**Architecture:** Rust core inside `src-tauri` does everything that touches the network or the disk — a `Supplier` trait with the Barezpakhsh HTTP implementation (no browser), a Landau API client driven by an `apt-…` key, matching with a calibrated threshold, two job runners (import, refresh) that persist every step to SQLite — and a Nuxt 4 + Nuxt UI page set (Persian, RTL) drives it through Tauri commands and events.

**Tech Stack:** Tauri 2.11 · Rust 2021 (cargo 1.97) · reqwest 0.12 (rustls, cookies) · scraper 0.27 · roxmltree 0.21 · rusqlite 0.40 (bundled) · keyring 3.6 · tokio 1 · Nuxt 4.5 · @nuxt/ui 4.11 · @tauri-apps/api 2.11 · pnpm 11 · vitest 5

**Spec:** `docs/superpowers/specs/2026-09-13-landau-bridge-design.md` — read it first; every task below cites the section it implements.

## Global Constraints

- The app has **no runtime dependency** on `/home/vahid/Claude`, `/home/vahid/WorkSpace/barezpakhsh-scraper` or `/home/vahid/WorkSpace/landau-parts`. Those paths appear only in the runbook and the calibration binary's *input path*, never in app code.
- Landau is reached **only through the client gateway** with `Authorization: Bearer apt-…`. Landau's own routes sit at the URL root (`/career/…`); platform collections sit behind `/-/`.
- **Never send `price` on `PATCH /career/stocks/:id`** — Landau stamps it `MANUAL` and its rules skip the row for good. `price` is sent on `POST /career/stocks` only, and only as `price = cost`.
- On attach, `condition` is always `NEW`; `authenticity` is the seller's default (settings) unless overridden per item.
- `cost = round(list_price × (1 + vat_percent / 100))`, `vat_percent` is a setting defaulting to `10`; when the site's `TaxAndTollPercent` differs from the setting, record a `VatRateMismatch` warning (never silently use either).
- Both HTTP clients are built with `.no_proxy()` — Barezpakhsh geo-blocks proxy exits and Landau is loopback/LAN in every test.
- Every error kind in spec §9 is a `BridgeError` variant or a `kinds::` constant, is logged with `tracing`, and is written to the `errors` table.
- All user-facing text is Persian. Log lines and code comments are English.
- Secrets (Landau key, supplier password) never appear in logs, events, `job_events`, `errors`, or the `settings` table when the keyring is available.
- Gates before any task is called done: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (offline), and for UI tasks `pnpm typecheck` + `pnpm generate`.
- Live tests (`cargo test --features live`) read `LND_API_URL`, `LND_APT_TOKEN`, `LND_STORE`, `BAREZ_USER`, `BAREZ_PASS` from the environment and **skip with a clear message** when any is unset; they never carry a value.
- Commit after every task with a message in the imperative mood ending with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.

## File structure (what each file owns)

```
landau-bridge/
├── package.json · pnpm-lock.yaml · nuxt.config.ts · tsconfig.json · vitest.config.ts
├── app/
│   ├── app.vue                      # <UApp> shell, RTL, global toasts
│   ├── assets/css/main.css          # Tailwind + Nuxt UI + Vazirmatn @font-face
│   ├── layouts/default.vue          # sidebar navigation (six screens)
│   ├── composables/useBridge.ts     # typed invoke()/listen() over @tauri-apps/api
│   ├── composables/useJob.ts        # subscribes to job://* events for one job id
│   ├── utils/fa.ts                  # Persian numerals, rials, Jalali helpers (+ fa.spec.ts)
│   ├── types/bridge.ts              # DTOs mirrored from src-tauri/src/commands/dto.rs
│   ├── components/JobProgress.vue · ReviewTable.vue · ReportTable.vue · ErrorBanner.vue
│   └── pages/index.vue · settings.vue · import.vue · refresh.vue · matches.vue · logs.vue
├── public/fonts/Vazirmatn[wght].woff2
├── src-tauri/
│   ├── Cargo.toml · tauri.conf.json · build.rs · capabilities/default.json · icons/
│   ├── fixtures/barezpakhsh/*.html|json   # captured 2026-09-13, anonymised (already in the repo)
│   └── src/
│       ├── main.rs · lib.rs             # entry; AppState; command registration
│       ├── errors.rs                    # BridgeError + kinds:: constants + Persian messages
│       ├── domain.rs                    # InvoiceHeader, InvoiceLine, PriceQuote, Authenticity, cost_of
│       ├── db/mod.rs                    # Db (Arc<Mutex<Connection>>), open, migrate
│       ├── db/migrations/0001_init.sql
│       ├── db/settings.rs · invoices.rs · prices.rs · matches.rs · jobs.rs · errors.rs
│       ├── secrets.rs                   # SecretStore: keyring or 0600 file
│       ├── settings.rs                  # typed Settings load/save over db::settings + secrets
│       ├── matching/normalize.rs        # Persian normalisation, search_term()
│       ├── matching/similarity.rs       # token_set_ratio()
│       ├── matching/mod.rs              # Candidate scoring, decide(), decisions over db::matches
│       ├── supplier/mod.rs              # trait Supplier, Credentials, registry()
│       ├── supplier/barezpakhsh/parse.rs    # pure parsers over fixtures
│       ├── supplier/barezpakhsh/client.rs   # reqwest session implementing Supplier
│       ├── landau/mod.rs                # LandauClient + types (Membership, ShelfRow, Candidate, RepricedRow)
│       ├── jobs/mod.rs                  # JobRunner, JobCtx, progress events, cancel flag
│       ├── jobs/planner.rs              # pure: aggregate(), plan_write()
│       ├── jobs/import.rs               # phases preflight…report (pauses at REVIEW)
│       ├── jobs/refresh.rs
│       ├── commands/mod.rs · dto.rs     # #[tauri::command] fns and their DTOs
│       └── bin/calibrate.rs             # feature `live`: writes docs/matching-threshold.md
├── tests/live/                      # (inside src-tauri/tests/) feature-gated integration tests
├── scripts/mint-apt.mjs             # dev helper: mint an apt for the fixture seller
└── docs/RUNBOOK.md · docs/matching-threshold.md
```

---
### Task 1: Scaffold — Nuxt 4 + Nuxt UI + Tauri 2, empty module tree

Implements spec §3 (layout). Produces a building, launchable empty app and the Rust module tree every later task fills in.

**Files:**
- Create: `package.json`, `nuxt.config.ts`, `tsconfig.json`, `vitest.config.ts`, `.gitignore`, `app/app.vue`, `app/assets/css/main.css`, `app/pages/index.vue`
- Create (via `tauri init`, then edit): `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `src-tauri/src/{errors,domain,secrets,settings}.rs`, `src-tauri/src/{db,matching,supplier,supplier/barezpakhsh,landau,jobs,commands}/mod.rs` as empty modules (`//! filled by Task N`)

**Interfaces:**
- Produces: the crate `landau_bridge_lib` with `pub mod errors, domain, db, secrets, settings, matching, supplier, landau, jobs, commands;` and `pub fn run()`; pnpm scripts `dev`, `generate`, `typecheck`, `test`, `tauri`.

- [ ] **Step 1: Initialise the pnpm project and install the UI stack**

```bash
cd /home/vahid/WorkSpace/landau-bridge
cat > package.json <<'EOF'
{
  "name": "landau-bridge",
  "private": true,
  "type": "module",
  "version": "0.1.0",
  "scripts": {
    "dev": "nuxt dev --port 3000",
    "generate": "nuxt generate",
    "preview": "nuxt preview",
    "typecheck": "nuxt typecheck",
    "test": "vitest run",
    "tauri": "tauri",
    "postinstall": "nuxt prepare"
  }
}
EOF
pnpm add nuxt@^4.5.2 @nuxt/ui@^4.11.1 @tauri-apps/api@^2.11.1 @tauri-apps/plugin-opener@^2.5.5 @internationalized/date@^3.12.4
pnpm add -D @tauri-apps/cli@^2.11.4 typescript@^5.9.3 vue-tsc@^3.3.11 vitest@^5.0.0
```

- [ ] **Step 2: Nuxt config, app shell, CSS**

`nuxt.config.ts`:
```ts
export default defineNuxtConfig({
  ssr: false,
  devtools: { enabled: false },
  modules: ['@nuxt/ui'],
  css: ['~/assets/css/main.css'],
  app: {
    head: {
      htmlAttrs: { dir: 'rtl', lang: 'fa' },
      title: 'پل لاندا',
    },
  },
  // Tauri's dev server contract: fixed port, no screen clearing, TAURI_* env exposed.
  vite: {
    clearScreen: false,
    envPrefix: ['VITE_', 'TAURI_'],
    server: { strictPort: true },
  },
  compatibilityDate: '2026-09-13',
});
```

`tsconfig.json`:
```json
{ "extends": "./.nuxt/tsconfig.json" }
```

`vitest.config.ts` (pure utils only; Nuxt runtime is not loaded):
```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: { include: ['app/**/*.spec.ts'], environment: 'node' },
});
```

`app/assets/css/main.css`:
```css
@import "tailwindcss";
@import "@nuxt/ui";

@font-face {
  font-family: 'Vazirmatn';
  src: url('/fonts/Vazirmatn[wght].woff2') format('woff2');
  font-weight: 100 900;
  font-display: swap;
}

:root {
  --font-sans: 'Vazirmatn', 'Tahoma', 'Segoe UI', sans-serif;
}

html { font-family: var(--font-sans); }
```

`app/app.vue`:
```vue
<template>
  <UApp :locale="fa">
    <NuxtPage />
  </UApp>
</template>

<script setup lang="ts">
import { fa } from '@nuxt/ui/locale';
</script>
```

`app/pages/index.vue` (placeholder, replaced in Task 14):
```vue
<template>
  <main class="p-6">
    <h1 class="text-xl font-bold">پل لاندا</h1>
  </main>
</template>
```

Font: download Vazirmatn (falls back to Tahoma if the download fails — the `@font-face` above is harmless without the file):
```bash
mkdir -p public/fonts && cd public/fonts && \
  curl -sSL -o vazirmatn.zip https://github.com/rastikerdar/vazirmatn/releases/download/v33.003/vazirmatn-v33.003.zip && \
  unzip -o -j vazirmatn.zip 'vazirmatn-v33.003/fonts/webfonts/Vazirmatn[wght].woff2' && rm vazirmatn.zip; cd ../..
ls public/fonts
```

- [ ] **Step 3: Initialise Tauri non-interactively**

```bash
pnpm tauri init --ci --app-name landau-bridge --window-title "پل لاندا" \
  --frontend-dist ../.output/public --dev-url http://localhost:3000 \
  --before-dev-command "pnpm dev" --before-build-command "pnpm generate"
```

Then overwrite `src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "landau-bridge",
  "version": "0.1.0",
  "identifier": "ir.landau.bridge",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:3000",
    "beforeBuildCommand": "pnpm generate",
    "frontendDist": "../.output/public"
  },
  "app": {
    "windows": [
      { "title": "پل لاندا", "width": 1200, "height": 800, "minWidth": 960, "minHeight": 640 }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```

`src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "main window",
  "windows": ["main"],
  "permissions": ["core:default", "opener:default"]
}
```

- [ ] **Step 4: Cargo manifest and the module tree**

`src-tauri/Cargo.toml`:
```toml
[package]
name = "landau-bridge"
version = "0.1.0"
description = "Barezpakhsh invoices onto a Landau shelf"
edition = "2021"
rust-version = "1.80"

[lib]
name = "landau_bridge_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[[bin]]
name = "landau-bridge"
path = "src/main.rs"

[[bin]]
name = "calibrate"
path = "src/bin/calibrate.rs"
required-features = ["live"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "cookies", "json", "gzip"] }
scraper = "0.27"
roxmltree = "0.21"
rusqlite = { version = "0.40", features = ["bundled"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"
keyring = { version = "3.6", features = ["apple-native", "windows-native", "sync-secret-service", "crypto-rust"] }
regex = "1"
strsim = "0.11"
async-trait = "0.1"
chrono = "0.4"
url = "2"

[dev-dependencies]
pretty_assertions = "1"
tempfile = "3"

[features]
default = []
live = []
```

`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    landau_bridge_lib::run()
}
```

`src-tauri/src/lib.rs` (Task 12 replaces the body of `run`):
```rust
//! Landau Bridge — Rust core. See docs/superpowers/specs/2026-09-13-landau-bridge-design.md.

pub mod commands;
pub mod db;
pub mod domain;
pub mod errors;
pub mod jobs;
pub mod landau;
pub mod matching;
pub mod secrets;
pub mod settings;
pub mod supplier;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Create each of these with a one-line doc comment naming the task that fills it:
`src/errors.rs`, `src/domain.rs`, `src/secrets.rs`, `src/settings.rs`, `src/db/mod.rs`, `src/matching/mod.rs`, `src/supplier/mod.rs`, `src/supplier/barezpakhsh/mod.rs` (declared from `supplier/mod.rs` as `pub mod barezpakhsh;`), `src/landau/mod.rs`, `src/jobs/mod.rs`, `src/commands/mod.rs`, and `src/bin/calibrate.rs` containing:
```rust
//! Threshold calibration — Task 10.
fn main() {}
```

`.gitignore`:
```
node_modules
.nuxt
.output
dist
src-tauri/target
src-tauri/gen/schemas
*.log
.DS_Store
```

- [ ] **Step 5: Verify the empty app builds**

Run: `cd src-tauri && cargo check --all-targets --features live && cargo clippy --all-targets -- -D warnings && cd ..`
Expected: finishes with no warnings (an unused-module warning means a `mod` line is missing).

Run: `pnpm generate && ls .output/public/index.html`
Expected: the file exists.

Run (smoke, optional if a display is available): `pnpm tauri dev` — a window titled «پل لاندا» opens showing the placeholder heading. Close it.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Scaffold the Tauri + Nuxt shell and the Rust module tree

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 2: Error kinds and domain types

Implements spec §9 (error kinds) and the value types of §2/§4 that every module shares.

**Files:**
- Create: `src-tauri/src/errors.rs`, `src-tauri/src/domain.rs`

**Interfaces:**
- Produces: `BridgeError` (thiserror enum, `Serialize` as `{kind, message, detail}`), `BridgeError::kind() -> &'static str`, `BridgeError::message_fa() -> String`, `kinds::*` string constants for recorded warnings, `type Result<T> = std::result::Result<T, BridgeError>`; `domain::{InvoiceKind, InvoiceHeader, InvoiceLine, PriceQuote, Authenticity, ImportMode, cost_of, normalize_jalali}`.

- [ ] **Step 1: Write the failing tests** (at the bottom of each file, `#[cfg(test)] mod tests`)

`src-tauri/src/domain.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_adds_vat_and_rounds_to_whole_rials() {
        assert_eq!(cost_of(10_731_000, 10.0), 11_804_100);
        assert_eq!(cost_of(1_647_000, 10.0), 1_811_700);
        assert_eq!(cost_of(15, 10.0), 17); // 16.5 rounds half away from zero
        assert_eq!(cost_of(0, 10.0), 0);
    }

    #[test]
    fn jalali_dates_are_zero_padded_for_lexical_comparison() {
        assert_eq!(normalize_jalali("1401/10/5"), Some("1401/10/05".to_string()));
        assert_eq!(normalize_jalali("۱۴۰۱/۱/۲۳"), Some("1401/01/23".to_string()));
        assert_eq!(normalize_jalali("garbage"), None);
        assert!(normalize_jalali("1401/10/05").unwrap() > normalize_jalali("1401/9/30").unwrap());
    }

    #[test]
    fn authenticity_round_trips_as_landau_codes() {
        assert_eq!(Authenticity::Original.as_str(), "ORIGINAL");
        assert_eq!(Authenticity::parse("fake"), Some(Authenticity::Fake));
        assert_eq!(Authenticity::parse("copy"), None);
    }
}
```

`src-tauri/src/errors.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_message_detail() {
        let e = BridgeError::NoCurrentPrice { code: "3137006".into() };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "NoCurrentPrice");
        assert!(v["message"].as_str().unwrap().contains("3137006"));
        assert_eq!(v["detail"], "no current price for 3137006");
    }

    #[test]
    fn every_variant_has_a_persian_message() {
        for e in [
            BridgeError::SupplierBlocked,
            BridgeError::SupplierBadCredentials,
            BridgeError::LandauUnauthorized,
            BridgeError::MirrorTimeout(3),
        ] {
            assert!(e.message_fa().chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)), "{}", e.kind());
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test domain errors`
Expected: compile errors — the types do not exist yet.

- [ ] **Step 3: Implement `domain.rs`**

```rust
//! Value types shared by the supplier, Landau, matching and job layers (spec §2, §4).

use serde::{Deserialize, Serialize};

/// A ledger row kind we act on. Receipts («دريافت») are not invoices and are dropped at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InvoiceKind {
    /// «فاکتور فروش» — adds quantity.
    Sale,
    /// «مرجوع از فروش» — subtracts quantity.
    Return,
}

impl InvoiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InvoiceKind::Sale => "SALE",
            InvoiceKind::Return => "RETURN",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "SALE" => Some(InvoiceKind::Sale),
            "RETURN" => Some(InvoiceKind::Return),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceHeader {
    pub number: String,
    pub kind: InvoiceKind,
    /// `YYYY/MM/DD`, zero-padded (see [`normalize_jalali`]) so string comparison orders dates.
    pub date_jalali: String,
    /// `YYYY-MM-DD` from the ledger's `VoucherDateOld`.
    pub date_iso: String,
    pub paid: bool,
    /// Site-relative, e.g. `/Shop/Preview/Factor/8818975/11000`.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub line_no: u32,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub unit: String,
    /// Whole rials, before VAT, as printed on the invoice.
    pub unit_price: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceQuote {
    pub code: String,
    pub name: String,
    /// The «فی» column (`FinalPrice`), whole rials before VAT.
    pub list_price: i64,
    /// `TaxAndTollPercent` as the site states it, e.g. `10.0`.
    pub vat_percent: Option<f64>,
    pub has_vat: bool,
    pub available: bool,
    pub unit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Authenticity {
    Original,
    Fake,
}

impl Authenticity {
    pub fn as_str(self) -> &'static str {
        match self {
            Authenticity::Original => "ORIGINAL",
            Authenticity::Fake => "FAKE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ORIGINAL" => Some(Authenticity::Original),
            "FAKE" => Some(Authenticity::Fake),
            _ => None,
        }
    }
}

/// The per-import choice of spec §7.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ImportMode {
    /// Transfer invoice quantities from invoices dated on/after `since`.
    Quantity,
    /// Every item at quantity 0 for review.
    Zero,
}

impl ImportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportMode::Quantity => "QUANTITY",
            ImportMode::Zero => "ZERO",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "QUANTITY" => Some(ImportMode::Quantity),
            "ZERO" => Some(ImportMode::Zero),
            _ => None,
        }
    }
}

/// `round(list_price × (1 + vat/100))` in whole rials — the decision of spec §1.
pub fn cost_of(list_price: i64, vat_percent: f64) -> i64 {
    ((list_price as f64) * (1.0 + vat_percent / 100.0)).round() as i64
}

/// Persian/Arabic digits → ASCII.
pub fn latin_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            '٠'..='٩' => char::from(b'0' + (c as u32 - '٠' as u32) as u8),
            other => other,
        })
        .collect()
}

/// `1401/10/5` → `1401/10/05`. `None` when the text is not a Jalali `Y/M/D` date.
pub fn normalize_jalali(s: &str) -> Option<String> {
    let s = latin_digits(s.trim());
    let mut parts = s.split('/');
    let y: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1300 {
        return None;
    }
    Some(format!("{y:04}/{m:02}/{d:02}"))
}
```

- [ ] **Step 4: Implement `errors.rs`**

```rust
//! Typed failures (spec §9). Each variant has a stable `kind` for the UI/DB and a Persian message.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, BridgeError>;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum BridgeError {
    #[error("supplier unreachable: {0}")]
    SupplierUnreachable(String),
    #[error("supplier blocked this network")]
    SupplierBlocked,
    #[error("supplier rejected the credentials")]
    SupplierBadCredentials,
    #[error("supplier session expired")]
    SupplierSessionExpired,
    #[error("supplier page {page}: missing {field}")]
    SupplierParse { page: String, field: String },
    #[error("no current price for {code}")]
    NoCurrentPrice { code: String },
    #[error("landau unauthorized")]
    LandauUnauthorized,
    #[error("landau forbidden: {0}")]
    LandauForbidden(String),
    #[error("landau unreachable: {0}")]
    LandauUnreachable(String),
    #[error("landau answered {status}: {body}")]
    LandauRejected { status: u16, body: String },
    #[error("business not approved")]
    BusinessNotApproved,
    #[error("no store")]
    NoStore,
    #[error("product not approved: {0}")]
    ProductNotApproved(String),
    #[error("duplicate shelf row")]
    DuplicateRow,
    #[error("mirror timeout: {0} row(s) not visible")]
    MirrorTimeout(usize),
    #[error("job cancelled")]
    JobCancelled,
    #[error("job not found: {0}")]
    JobNotFound(i64),
    #[error("job is busy: {0}")]
    JobBusy(String),
    #[error("settings incomplete: {0}")]
    SettingsIncomplete(String),
    #[error("database: {0}")]
    Db(String),
    #[error("keyring unavailable: {0}")]
    KeyringUnavailable(String),
    #[error("{0}")]
    Other(String),
}

/// Warning kinds recorded in `errors`/`job_events` that are not failures of a call.
pub mod kinds {
    pub const NO_PRICING_RULE: &str = "NoPricingRule";
    pub const REPRICE_SKIPPED: &str = "RepriceSkipped";
    pub const INVOICE_UNPAID: &str = "InvoiceUnpaid";
    pub const INVOICE_ALREADY_IMPORTED: &str = "InvoiceAlreadyImported";
    pub const LINE_NON_POSITIVE_QTY: &str = "LineNonPositiveQty";
    pub const UNMATCHED: &str = "Unmatched";
    pub const NONE_DECISION: &str = "NoneDecision";
    pub const JOB_INTERRUPTED: &str = "JobInterrupted";
    pub const VAT_RATE_MISMATCH: &str = "VatRateMismatch";
    pub const NEGATIVE_QUANTITY: &str = "NegativeQuantity";
    pub const AMBIGUOUS_SUPPLIER_LINK: &str = "AmbiguousSupplierLink";
    pub const NO_SUPPLIER_LINK: &str = "NoSupplierLink";
    pub const MIRROR_TIMEOUT: &str = "MirrorTimeout";
}

impl BridgeError {
    pub fn kind(&self) -> &'static str {
        use BridgeError::*;
        match self {
            SupplierUnreachable(_) => "SupplierUnreachable",
            SupplierBlocked => "SupplierBlocked",
            SupplierBadCredentials => "SupplierBadCredentials",
            SupplierSessionExpired => "SupplierSessionExpired",
            SupplierParse { .. } => "SupplierParse",
            NoCurrentPrice { .. } => "NoCurrentPrice",
            LandauUnauthorized => "LandauUnauthorized",
            LandauForbidden(_) => "LandauForbidden",
            LandauUnreachable(_) => "LandauUnreachable",
            LandauRejected { .. } => "LandauRejected",
            BusinessNotApproved => "BusinessNotApproved",
            NoStore => "NoStore",
            ProductNotApproved(_) => "ProductNotApproved",
            DuplicateRow => "DuplicateRow",
            MirrorTimeout(_) => "MirrorTimeout",
            JobCancelled => "JobCancelled",
            JobNotFound(_) => "JobNotFound",
            JobBusy(_) => "JobBusy",
            SettingsIncomplete(_) => "SettingsIncomplete",
            Db(_) => "Db",
            KeyringUnavailable(_) => "KeyringUnavailable",
            Other(_) => "Other",
        }
    }

    /// What the seller reads. Keep these sentences actionable.
    pub fn message_fa(&self) -> String {
        use BridgeError::*;
        match self {
            SupplierUnreachable(d) => format!("اتصال به سایت بارزپخش برقرار نشد ({d}). اینترنت را بررسی کنید."),
            SupplierBlocked => "سایت بارزپخش این شبکه را مسدود کرده است. پروکسی یا VPN را خاموش کنید.".into(),
            SupplierBadCredentials => "نام کاربری یا کلمه عبور بارزپخش درست نیست.".into(),
            SupplierSessionExpired => "نشست بارزپخش منقضی شد و ورود دوباره هم ناموفق بود.".into(),
            SupplierParse { page, field } => format!("ساختار صفحه «{page}» تغییر کرده است (بخش «{field}» پیدا نشد). به‌روزرسانی برنامه لازم است."),
            NoCurrentPrice { code } => format!("برای کد {code} قیمت روز در بارزپخش پیدا نشد."),
            LandauUnauthorized => "کلید API لاندا نامعتبر یا منقضی است. کلید تازه‌ای در لاندا بسازید.".into(),
            LandauForbidden(d) => format!("لاندا این درخواست را مجاز نمی‌داند ({d}). کلید باید از حساب فروشنده و از طریق درگاه لاندا باشد."),
            LandauUnreachable(d) => format!("اتصال به لاندا برقرار نشد ({d})."),
            LandauRejected { status, body } => format!("لاندا درخواست را رد کرد (کد {status}): {body}"),
            BusinessNotApproved => "کسب‌وکار شما در لاندا هنوز تأیید نشده است.".into(),
            NoStore => "هیچ فروشگاهی برای این حساب در لاندا ثبت نشده است.".into(),
            ProductNotApproved(p) => format!("قطعه {p} در فهرست لاندا تأیید نشده است."),
            DuplicateRow => "این قطعه با همین مشخصات از قبل روی قفسه است.".into(),
            MirrorTimeout(n) => format!("{n} ردیف هنوز در قفسه دیده نمی‌شود؛ قیمت‌گذاری آن‌ها در اجرای بعدی انجام می‌شود."),
            JobCancelled => "عملیات به درخواست شما متوقف شد.".into(),
            JobNotFound(id) => format!("عملیات شماره {id} پیدا نشد."),
            JobBusy(w) => format!("عملیات دیگری در حال اجراست: {w}"),
            SettingsIncomplete(w) => format!("تنظیمات ناقص است: {w}"),
            Db(d) => format!("خطای پایگاه داده: {d}"),
            KeyringUnavailable(d) => format!("ذخیره‌ساز امن سیستم در دسترس نیست ({d})؛ رمزها در پرونده برنامه نگه داشته می‌شوند."),
            Other(d) => d.clone(),
        }
    }
}

impl Serialize for BridgeError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("BridgeError", 3)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.message_fa())?;
        st.serialize_field("detail", &self.to_string())?;
        st.end()
    }
}

impl From<rusqlite::Error> for BridgeError {
    fn from(e: rusqlite::Error) -> Self {
        BridgeError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for BridgeError {
    fn from(e: serde_json::Error) -> Self {
        BridgeError::Other(format!("json: {e}"))
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test domain errors`
Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/errors.rs src-tauri/src/domain.rs
git commit -m "Add the error kinds and shared domain types

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 3: SQLite layer — schema and repositories

Implements spec §8. One `Db` handle shared by jobs and commands; every repository is a method group on it. Tests run on an in-memory database.

**Files:**
- Create: `src-tauri/src/db/mod.rs`, `src-tauri/src/db/migrations/0001_init.sql`, `src-tauri/src/db/settings.rs`, `src-tauri/src/db/invoices.rs`, `src-tauri/src/db/prices.rs`, `src-tauri/src/db/matches.rs`, `src-tauri/src/db/jobs.rs`, `src-tauri/src/db/errors.rs`

**Interfaces:**
- Consumes: `domain::*`, `errors::{BridgeError, Result}`.
- Produces: `Db::open(path) -> Result<Db>`, `Db::in_memory() -> Result<Db>`, `Db::with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T>`; the repository methods listed in each file below (exact signatures in the code).

- [ ] **Step 1: Write the schema**

`src-tauri/src/db/migrations/0001_init.sql`:
```sql
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE invoices (
  id          INTEGER PRIMARY KEY,
  supplier    TEXT NOT NULL,
  number      TEXT NOT NULL,
  kind        TEXT NOT NULL,            -- SALE | RETURN
  date_jalali TEXT NOT NULL,            -- YYYY/MM/DD zero-padded
  date_iso    TEXT NOT NULL,
  paid        INTEGER NOT NULL,
  url         TEXT NOT NULL,
  fetched_at  TEXT,                     -- NULL until the lines were fetched
  UNIQUE (supplier, number, kind)
);

CREATE TABLE invoice_lines (
  id         INTEGER PRIMARY KEY,
  invoice_id INTEGER NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
  line_no    INTEGER NOT NULL,
  code       TEXT NOT NULL,
  name       TEXT NOT NULL,
  qty        INTEGER NOT NULL,
  unit       TEXT NOT NULL,
  unit_price INTEGER NOT NULL
);
CREATE INDEX invoice_lines_code ON invoice_lines(code);

CREATE TABLE price_snapshots (
  id          INTEGER PRIMARY KEY,
  supplier    TEXT NOT NULL,
  code        TEXT NOT NULL,
  list_price  INTEGER NOT NULL,
  cost        INTEGER NOT NULL,
  vat_percent REAL,
  available   INTEGER NOT NULL,
  fetched_at  TEXT NOT NULL
);
CREATE INDEX price_snapshots_code ON price_snapshots(supplier, code, fetched_at);

CREATE TABLE matches (
  supplier     TEXT NOT NULL,
  code         TEXT NOT NULL,
  product_id   TEXT,
  product_name TEXT,
  how          TEXT NOT NULL,           -- AUTO | USER | NONE
  similarity   INTEGER,
  name_seen    TEXT NOT NULL,
  decided_at   TEXT NOT NULL,
  PRIMARY KEY (supplier, code)
);

CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,           -- IMPORT | REFRESH
  mode         TEXT,                    -- QUANTITY | ZERO (import only)
  since_date   TEXT,
  store        TEXT NOT NULL,
  overwrite    INTEGER NOT NULL DEFAULT 0,
  status       TEXT NOT NULL,           -- RUNNING | REVIEW | DONE | FAILED | CANCELLED | INTERRUPTED
  phase        TEXT NOT NULL DEFAULT 'PREFLIGHT',
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  summary_json TEXT
);

CREATE TABLE job_items (
  id              INTEGER PRIMARY KEY,
  job_id          INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  code            TEXT NOT NULL,
  name            TEXT NOT NULL,
  qty             INTEGER NOT NULL,
  product_id      TEXT,
  product_name    TEXT,
  stock_id        TEXT,
  authenticity    TEXT,
  action          TEXT,                 -- ATTACH | PATCH | SKIP
  qty_before      INTEGER,
  qty_after       INTEGER,
  cost            INTEGER,
  price_before    INTEGER,
  price_after     INTEGER,
  reprice_verdict TEXT,
  error_kind      TEXT,
  error_msg       TEXT,
  candidates_json TEXT,                 -- review candidates, best first (Scored[])
  status          TEXT NOT NULL DEFAULT 'PLANNED'  -- PLANNED | REVIEW | SKIPPED | WRITTEN | FAILED | DONE
);
CREATE INDEX job_items_job ON job_items(job_id);

CREATE TABLE job_events (
  id          INTEGER PRIMARY KEY,
  job_id      INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  at          TEXT NOT NULL,
  level       TEXT NOT NULL,            -- INFO | WARN | ERROR
  phase       TEXT NOT NULL,
  message     TEXT NOT NULL,
  detail_json TEXT
);
CREATE INDEX job_events_job ON job_events(job_id);

CREATE TABLE invoice_imports (
  invoice_id INTEGER PRIMARY KEY REFERENCES invoices(id),
  job_id     INTEGER NOT NULL
);

CREATE TABLE errors (
  id           INTEGER PRIMARY KEY,
  at           TEXT NOT NULL,
  job_id       INTEGER,
  kind         TEXT NOT NULL,
  message      TEXT NOT NULL,
  context_json TEXT
);
```

- [ ] **Step 2: Write the failing tests** (in `src-tauri/src/db/mod.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InvoiceHeader, InvoiceKind, InvoiceLine};

    fn header(number: &str, kind: InvoiceKind, date: &str, paid: bool) -> InvoiceHeader {
        InvoiceHeader {
            number: number.into(),
            kind,
            date_jalali: date.into(),
            date_iso: "2023-01-04".into(),
            paid,
            url: format!("/Shop/Preview/Factor/{number}/11000"),
        }
    }

    #[test]
    fn migrates_once_and_stores_settings() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.setting("threshold").unwrap(), None);
        db.set_setting("threshold", "80").unwrap();
        assert_eq!(db.setting("threshold").unwrap().as_deref(), Some("80"));
        // migrating again is a no-op
        db.migrate().unwrap();
        assert_eq!(db.user_version().unwrap(), 1);
    }

    #[test]
    fn invoices_are_unique_per_number_and_kind_and_lines_replace() {
        let db = Db::in_memory().unwrap();
        let id = db.upsert_invoice("barezpakhsh", &header("1", InvoiceKind::Sale, "1401/10/05", true)).unwrap();
        let again = db.upsert_invoice("barezpakhsh", &header("1", InvoiceKind::Sale, "1401/10/05", true)).unwrap();
        assert_eq!(id, again);
        let ret = db.upsert_invoice("barezpakhsh", &header("1", InvoiceKind::Return, "1401/10/05", true)).unwrap();
        assert_ne!(id, ret);

        assert_eq!(db.unfetched_invoice_ids("barezpakhsh").unwrap().len(), 2);
        let lines = vec![InvoiceLine { line_no: 1, code: "A".into(), name: "n".into(), qty: 2, unit: "عدد".into(), unit_price: 10 }];
        db.replace_lines(id, &lines).unwrap();
        db.replace_lines(id, &lines).unwrap();
        assert_eq!(db.unfetched_invoice_ids("barezpakhsh").unwrap(), vec![ret]);
        let all = db.lines_with_invoices("barezpakhsh").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].code, "A");
        assert_eq!(all[0].kind, InvoiceKind::Sale);
    }

    #[test]
    fn matches_persist_and_none_is_a_decision() {
        let db = Db::in_memory().unwrap();
        assert!(db.match_for("barezpakhsh", "A").unwrap().is_none());
        db.set_match("barezpakhsh", "A", Some(("p1", "part one")), "AUTO", Some(91), "name a").unwrap();
        let m = db.match_for("barezpakhsh", "A").unwrap().unwrap();
        assert_eq!(m.product_id.as_deref(), Some("p1"));
        db.set_match("barezpakhsh", "B", None, "NONE", None, "name b").unwrap();
        let none = db.match_for("barezpakhsh", "B").unwrap().unwrap();
        assert_eq!(none.how, "NONE");
        assert!(none.product_id.is_none());
        assert_eq!(db.list_matches("barezpakhsh").unwrap().len(), 2);
    }

    #[test]
    fn jobs_items_events_and_errors_round_trip() {
        let db = Db::in_memory().unwrap();
        let job = db.create_job("IMPORT", Some("ZERO"), None, "store1", false).unwrap();
        let item = db.add_job_item(job, "A", "name a", 3).unwrap();
        db.update_job_item(item, &[("status", rusqlite::types::Value::Text("REVIEW".into()))]).unwrap();
        db.add_job_event(job, "INFO", "MATCH", "matched 1", None).unwrap();
        db.record_error(Some(job), "NoCurrentPrice", "no price", Some(serde_json::json!({"code": "A"}))).unwrap();
        db.set_job_status(job, "REVIEW", "MATCH").unwrap();

        let detail = db.job_detail(job).unwrap();
        assert_eq!(detail.job.status, "REVIEW");
        assert_eq!(detail.items.len(), 1);
        assert_eq!(detail.items[0].status, "REVIEW");
        assert_eq!(detail.events.len(), 1);
        assert_eq!(db.list_errors(10).unwrap().len(), 1);
        assert_eq!(db.list_jobs().unwrap().len(), 1);
        assert_eq!(db.running_job().unwrap(), None);
    }

    #[test]
    fn price_snapshots_answer_the_latest_per_code() {
        let db = Db::in_memory().unwrap();
        db.add_price_snapshot("barezpakhsh", "A", 100, 110, Some(10.0), true, "2026-09-13T10:00:00Z").unwrap();
        db.add_price_snapshot("barezpakhsh", "A", 120, 132, Some(10.0), true, "2026-09-13T11:00:00Z").unwrap();
        let latest = db.latest_price("barezpakhsh", "A").unwrap().unwrap();
        assert_eq!(latest.cost, 132);
    }

    #[test]
    fn invoice_imports_guard_double_transfer() {
        let db = Db::in_memory().unwrap();
        let id = db.upsert_invoice("barezpakhsh", &header("9", InvoiceKind::Sale, "1401/10/05", true)).unwrap();
        assert!(db.imported_invoice_ids().unwrap().is_empty());
        db.mark_invoices_imported(&[id], 1).unwrap();
        db.mark_invoices_imported(&[id], 2).unwrap(); // idempotent
        assert_eq!(db.imported_invoice_ids().unwrap(), std::collections::HashSet::from([id]));
    }
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cd src-tauri && cargo test db::`
Expected: compile errors.

- [ ] **Step 4: Implement `db/mod.rs`**

```rust
//! SQLite persistence (spec §8). One connection behind a mutex; every call is short.

pub mod errors;
pub mod invoices;
pub mod jobs;
pub mod matches;
pub mod prices;
pub mod settings;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::errors::{BridgeError, Result};

const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BridgeError::Db(e.to_string()))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let db = Db { conn: Arc::new(Mutex::new(conn)) };
        db.migrate()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Db { conn: Arc::new(Mutex::new(conn)) };
        db.migrate()?;
        Ok(db)
    }

    /// Run `f` with the connection. Poisoned locks surface as `Db` errors rather than panics.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let guard = self.conn.lock().map_err(|e| BridgeError::Db(format!("lock: {e}")))?;
        f(&guard).map_err(BridgeError::from)
    }

    pub fn user_version(&self) -> Result<i64> {
        self.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
    }

    pub fn migrate(&self) -> Result<()> {
        let current = self.user_version()?;
        self.with(|c| {
            for (i, sql) in MIGRATIONS.iter().enumerate() {
                let version = i as i64 + 1;
                if version <= current {
                    continue;
                }
                c.execute_batch(sql)?;
                c.pragma_update(None, "user_version", version)?;
            }
            Ok(())
        })
    }
}

/// ISO-8601 UTC timestamp used for every `*_at` column.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
```

- [ ] **Step 5: Implement the repositories**

`src-tauri/src/db/settings.rs`:
```rust
use rusqlite::{params, OptionalExtension};

use super::Db;
use crate::errors::Result;

impl Db {
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
        })
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM settings WHERE key = ?1", params![key]).map(|_| ()))
    }
}
```

`src-tauri/src/db/invoices.rs`:
```rust
use rusqlite::params;

use super::{now, Db};
use crate::domain::{InvoiceHeader, InvoiceKind, InvoiceLine};
use crate::errors::Result;

/// One invoice line joined with the header facts the planner needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineWithInvoice {
    pub invoice_id: i64,
    pub number: String,
    pub kind: InvoiceKind,
    pub paid: bool,
    pub date_jalali: String,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub unit: String,
    pub unit_price: i64,
}

impl Db {
    /// Insert or find the header. Returns the row id.
    pub fn upsert_invoice(&self, supplier: &str, h: &InvoiceHeader) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO invoices(supplier, number, kind, date_jalali, date_iso, paid, url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(supplier, number, kind) DO UPDATE SET paid = excluded.paid, date_jalali = excluded.date_jalali",
                params![supplier, h.number, h.kind.as_str(), h.date_jalali, h.date_iso, h.paid as i64, h.url],
            )?;
            c.query_row(
                "SELECT id FROM invoices WHERE supplier = ?1 AND number = ?2 AND kind = ?3",
                params![supplier, h.number, h.kind.as_str()],
                |r| r.get(0),
            )
        })
    }

    pub fn invoice_url(&self, invoice_id: i64) -> Result<String> {
        self.with(|c| c.query_row("SELECT url FROM invoices WHERE id = ?1", params![invoice_id], |r| r.get(0)))
    }

    /// Headers whose lines were never fetched.
    pub fn unfetched_invoice_ids(&self, supplier: &str) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id FROM invoices WHERE supplier = ?1 AND fetched_at IS NULL ORDER BY id")?;
            let rows = st.query_map(params![supplier], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Replace the invoice's lines and stamp `fetched_at`. One transaction.
    pub fn replace_lines(&self, invoice_id: i64, lines: &[InvoiceLine]) -> Result<()> {
        self.with(|c| {
            c.execute_batch("BEGIN")?;
            let result = (|| {
                c.execute("DELETE FROM invoice_lines WHERE invoice_id = ?1", params![invoice_id])?;
                let mut st = c.prepare(
                    "INSERT INTO invoice_lines(invoice_id, line_no, code, name, qty, unit, unit_price) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for l in lines {
                    st.execute(params![invoice_id, l.line_no, l.code, l.name, l.qty, l.unit, l.unit_price])?;
                }
                c.execute("UPDATE invoices SET fetched_at = ?2 WHERE id = ?1", params![invoice_id, now()])?;
                Ok(())
            })();
            match result {
                Ok(()) => c.execute_batch("COMMIT"),
                Err(e) => {
                    let _ = c.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }

    /// Every fetched line of the supplier, oldest invoice first.
    pub fn lines_with_invoices(&self, supplier: &str) -> Result<Vec<LineWithInvoice>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT i.id, i.number, i.kind, i.paid, i.date_jalali, l.code, l.name, l.qty, l.unit, l.unit_price
                 FROM invoice_lines l JOIN invoices i ON i.id = l.invoice_id
                 WHERE i.supplier = ?1 ORDER BY i.date_jalali, i.id, l.line_no",
            )?;
            let rows = st.query_map(params![supplier], |r| {
                let kind: String = r.get(2)?;
                Ok(LineWithInvoice {
                    invoice_id: r.get(0)?,
                    number: r.get(1)?,
                    kind: InvoiceKind::parse(&kind).unwrap_or(InvoiceKind::Sale),
                    paid: r.get::<_, i64>(3)? != 0,
                    date_jalali: r.get(4)?,
                    code: r.get(5)?,
                    name: r.get(6)?,
                    qty: r.get(7)?,
                    unit: r.get(8)?,
                    unit_price: r.get(9)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn count_invoices(&self, supplier: &str) -> Result<(i64, i64)> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(paid = 0), 0) FROM invoices WHERE supplier = ?1",
                params![supplier],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
    }

    pub fn imported_invoice_ids(&self) -> Result<std::collections::HashSet<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT invoice_id FROM invoice_imports")?;
            let rows = st.query_map([], |r| r.get::<_, i64>(0))?;
            rows.collect()
        })
    }

    pub fn mark_invoices_imported(&self, invoice_ids: &[i64], job_id: i64) -> Result<()> {
        self.with(|c| {
            let mut st = c.prepare("INSERT OR IGNORE INTO invoice_imports(invoice_id, job_id) VALUES (?1, ?2)")?;
            for id in invoice_ids {
                st.execute(params![id, job_id])?;
            }
            Ok(())
        })
    }
}
```

`src-tauri/src/db/prices.rs`:
```rust
use rusqlite::{params, OptionalExtension};

use super::Db;
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct PriceRow {
    pub list_price: i64,
    pub cost: i64,
    pub vat_percent: Option<f64>,
    pub available: bool,
    pub fetched_at: String,
}

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub fn add_price_snapshot(
        &self,
        supplier: &str,
        code: &str,
        list_price: i64,
        cost: i64,
        vat_percent: Option<f64>,
        available: bool,
        fetched_at: &str,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO price_snapshots(supplier, code, list_price, cost, vat_percent, available, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![supplier, code, list_price, cost, vat_percent, available as i64, fetched_at],
            )
            .map(|_| ())
        })
    }

    pub fn latest_price(&self, supplier: &str, code: &str) -> Result<Option<PriceRow>> {
        self.with(|c| {
            c.query_row(
                "SELECT list_price, cost, vat_percent, available, fetched_at FROM price_snapshots
                 WHERE supplier = ?1 AND code = ?2 ORDER BY fetched_at DESC, id DESC LIMIT 1",
                params![supplier, code],
                |r| {
                    Ok(PriceRow {
                        list_price: r.get(0)?,
                        cost: r.get(1)?,
                        vat_percent: r.get(2)?,
                        available: r.get::<_, i64>(3)? != 0,
                        fetched_at: r.get(4)?,
                    })
                },
            )
            .optional()
        })
    }
}
```

`src-tauri/src/db/matches.rs`:
```rust
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchRow {
    pub supplier: String,
    pub code: String,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    /// AUTO | USER | NONE
    pub how: String,
    pub similarity: Option<i64>,
    pub name_seen: String,
    pub decided_at: String,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MatchRow> {
    Ok(MatchRow {
        supplier: r.get(0)?,
        code: r.get(1)?,
        product_id: r.get(2)?,
        product_name: r.get(3)?,
        how: r.get(4)?,
        similarity: r.get(5)?,
        name_seen: r.get(6)?,
        decided_at: r.get(7)?,
    })
}

const COLS: &str = "supplier, code, product_id, product_name, how, similarity, name_seen, decided_at";

impl Db {
    pub fn match_for(&self, supplier: &str, code: &str) -> Result<Option<MatchRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND code = ?2"),
                params![supplier, code],
                row,
            )
            .optional()
        })
    }

    /// `product` = `Some((id, name))` for a match, `None` with `how = "NONE"` for "not in Landau".
    pub fn set_match(
        &self,
        supplier: &str,
        code: &str,
        product: Option<(&str, &str)>,
        how: &str,
        similarity: Option<i64>,
        name_seen: &str,
    ) -> Result<()> {
        let (pid, pname) = match product {
            Some((id, name)) => (Some(id), Some(name)),
            None => (None, None),
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO matches(supplier, code, product_id, product_name, how, similarity, name_seen, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(supplier, code) DO UPDATE SET product_id = excluded.product_id, product_name = excluded.product_name,
                   how = excluded.how, similarity = excluded.similarity, name_seen = excluded.name_seen, decided_at = excluded.decided_at",
                params![supplier, code, pid, pname, how, similarity, name_seen, now()],
            )
            .map(|_| ())
        })
    }

    pub fn clear_match(&self, supplier: &str, code: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM matches WHERE supplier = ?1 AND code = ?2", params![supplier, code]).map(|_| ()))
    }

    pub fn list_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {COLS} FROM matches WHERE supplier = ?1 ORDER BY decided_at DESC"))?;
            let rows = st.query_map(params![supplier], row)?;
            rows.collect()
        })
    }

    /// Every (code → product_id) link with a product, for the refresh job.
    pub fn linked_matches(&self, supplier: &str) -> Result<Vec<MatchRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {COLS} FROM matches WHERE supplier = ?1 AND product_id IS NOT NULL"))?;
            let rows = st.query_map(params![supplier], row)?;
            rows.collect()
        })
    }
}
```

`src-tauri/src/db/jobs.rs`:
```rust
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub mode: Option<String>,
    pub since_date: Option<String>,
    pub store: String,
    pub overwrite: bool,
    pub status: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub summary: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobItemRow {
    pub id: i64,
    pub job_id: i64,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    pub stock_id: Option<String>,
    pub authenticity: Option<String>,
    pub action: Option<String>,
    pub qty_before: Option<i64>,
    pub qty_after: Option<i64>,
    pub cost: Option<i64>,
    pub price_before: Option<i64>,
    pub price_after: Option<i64>,
    pub reprice_verdict: Option<String>,
    pub error_kind: Option<String>,
    pub error_msg: Option<String>,
    pub candidates: Option<serde_json::Value>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobEventRow {
    pub id: i64,
    pub at: String,
    pub level: String,
    pub phase: String,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobDetail {
    pub job: JobRow,
    pub items: Vec<JobItemRow>,
    pub events: Vec<JobEventRow>,
}

/// A column/value pair for [`Db::update_job_item`]. Allowed columns are listed in `ITEM_COLUMNS`.
pub type ItemPatch<'a> = (&'a str, rusqlite::types::Value);

const ITEM_COLUMNS: &[&str] = &[
    "product_id", "product_name", "stock_id", "authenticity", "action", "qty_before", "qty_after", "cost",
    "price_before", "price_after", "reprice_verdict", "error_kind", "error_msg", "candidates_json", "status", "qty", "name",
];

const JOB_COLS: &str = "id, kind, mode, since_date, store, overwrite, status, phase, started_at, finished_at, summary_json";

fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    let summary: Option<String> = r.get(10)?;
    Ok(JobRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        mode: r.get(2)?,
        since_date: r.get(3)?,
        store: r.get(4)?,
        overwrite: r.get::<_, i64>(5)? != 0,
        status: r.get(6)?,
        phase: r.get(7)?,
        started_at: r.get(8)?,
        finished_at: r.get(9)?,
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

const ITEM_COLS: &str = "id, job_id, code, name, qty, product_id, product_name, stock_id, authenticity, action, qty_before, qty_after, cost, price_before, price_after, reprice_verdict, error_kind, error_msg, candidates_json, status";

fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobItemRow> {
    Ok(JobItemRow {
        id: r.get(0)?,
        job_id: r.get(1)?,
        code: r.get(2)?,
        name: r.get(3)?,
        qty: r.get(4)?,
        product_id: r.get(5)?,
        product_name: r.get(6)?,
        stock_id: r.get(7)?,
        authenticity: r.get(8)?,
        action: r.get(9)?,
        qty_before: r.get(10)?,
        qty_after: r.get(11)?,
        cost: r.get(12)?,
        price_before: r.get(13)?,
        price_after: r.get(14)?,
        reprice_verdict: r.get(15)?,
        error_kind: r.get(16)?,
        error_msg: r.get(17)?,
        candidates: r.get::<_, Option<String>>(18)?.and_then(|c| serde_json::from_str(&c).ok()),
        status: r.get(19)?,
    })
}

impl Db {
    pub fn create_job(&self, kind: &str, mode: Option<&str>, since_date: Option<&str>, store: &str, overwrite: bool) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO jobs(kind, mode, since_date, store, overwrite, status, phase, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'RUNNING', 'PREFLIGHT', ?6)",
                params![kind, mode, since_date, store, overwrite as i64, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn set_job_status(&self, job_id: i64, status: &str, phase: &str) -> Result<()> {
        let finished = matches!(status, "DONE" | "FAILED" | "CANCELLED" | "INTERRUPTED").then(now);
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET status = ?2, phase = ?3, finished_at = COALESCE(?4, finished_at) WHERE id = ?1",
                params![job_id, status, phase, finished],
            )
            .map(|_| ())
        })
    }

    pub fn set_job_phase(&self, job_id: i64, phase: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE jobs SET phase = ?2 WHERE id = ?1", params![job_id, phase]).map(|_| ()))
    }

    pub fn set_job_summary(&self, job_id: i64, summary: &serde_json::Value) -> Result<()> {
        self.with(|c| c.execute("UPDATE jobs SET summary_json = ?2 WHERE id = ?1", params![job_id, summary.to_string()]).map(|_| ()))
    }

    pub fn job(&self, job_id: i64) -> Result<Option<JobRow>> {
        self.with(|c| c.query_row(&format!("SELECT {JOB_COLS} FROM jobs WHERE id = ?1"), params![job_id], job_row).optional())
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {JOB_COLS} FROM jobs ORDER BY id DESC LIMIT 200"))?;
            let rows = st.query_map([], job_row)?;
            rows.collect()
        })
    }

    /// The one job that is RUNNING or paused in REVIEW, if any.
    pub fn running_job(&self) -> Result<Option<JobRow>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {JOB_COLS} FROM jobs WHERE status IN ('RUNNING', 'REVIEW') ORDER BY id DESC LIMIT 1"),
                [],
                job_row,
            )
            .optional()
        })
    }

    /// On launch: a job left RUNNING by a closed app is INTERRUPTED (spec §7.3).
    pub fn interrupt_stale_jobs(&self) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id FROM jobs WHERE status = 'RUNNING'")?;
            let ids: Vec<i64> = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for id in &ids {
                c.execute("UPDATE jobs SET status = 'INTERRUPTED', finished_at = ?2 WHERE id = ?1", params![id, now()])?;
            }
            Ok(ids)
        })
    }

    pub fn add_job_item(&self, job_id: i64, code: &str, name: &str, qty: i64) -> Result<i64> {
        self.with(|c| {
            c.execute("INSERT INTO job_items(job_id, code, name, qty) VALUES (?1, ?2, ?3, ?4)", params![job_id, code, name, qty])?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn update_job_item(&self, item_id: i64, patch: &[ItemPatch<'_>]) -> Result<()> {
        if patch.is_empty() {
            return Ok(());
        }
        for (col, _) in patch {
            if !ITEM_COLUMNS.contains(col) {
                return Err(crate::errors::BridgeError::Db(format!("job_items has no column {col}")));
            }
        }
        let sets: Vec<String> = patch.iter().enumerate().map(|(i, (col, _))| format!("{col} = ?{}", i + 2)).collect();
        let sql = format!("UPDATE job_items SET {} WHERE id = ?1", sets.join(", "));
        self.with(|c| {
            let mut values: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Integer(item_id)];
            values.extend(patch.iter().map(|(_, v)| v.clone()));
            c.execute(&sql, rusqlite::params_from_iter(values)).map(|_| ())
        })
    }

    pub fn job_items(&self, job_id: i64) -> Result<Vec<JobItemRow>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {ITEM_COLS} FROM job_items WHERE job_id = ?1 ORDER BY id"))?;
            let rows = st.query_map(params![job_id], item_row)?;
            rows.collect()
        })
    }

    pub fn job_item(&self, item_id: i64) -> Result<Option<JobItemRow>> {
        self.with(|c| c.query_row(&format!("SELECT {ITEM_COLS} FROM job_items WHERE id = ?1"), params![item_id], item_row).optional())
    }

    pub fn add_job_event(&self, job_id: i64, level: &str, phase: &str, message: &str, detail: Option<serde_json::Value>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO job_events(job_id, at, level, phase, message, detail_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![job_id, now(), level, phase, message, detail.map(|d| d.to_string())],
            )
            .map(|_| ())
        })
    }

    pub fn job_detail(&self, job_id: i64) -> Result<JobDetail> {
        let job = self.job(job_id)?.ok_or(crate::errors::BridgeError::JobNotFound(job_id))?;
        let items = self.job_items(job_id)?;
        let events = self.with(|c| {
            let mut st = c.prepare("SELECT id, at, level, phase, message, detail_json FROM job_events WHERE job_id = ?1 ORDER BY id")?;
            let rows = st.query_map(params![job_id], |r| {
                let detail: Option<String> = r.get(5)?;
                Ok(JobEventRow {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    level: r.get(2)?,
                    phase: r.get(3)?,
                    message: r.get(4)?,
                    detail: detail.and_then(|d| serde_json::from_str(&d).ok()),
                })
            })?;
            rows.collect()
        })?;
        Ok(JobDetail { job, items, events })
    }
}
```

`src-tauri/src/db/errors.rs`:
```rust
use rusqlite::params;
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorRow {
    pub id: i64,
    pub at: String,
    pub job_id: Option<i64>,
    pub kind: String,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

impl Db {
    pub fn record_error(&self, job_id: Option<i64>, kind: &str, message: &str, context: Option<serde_json::Value>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO errors(at, job_id, kind, message, context_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![now(), job_id, kind, message, context.map(|v| v.to_string())],
            )
            .map(|_| ())
        })
    }

    pub fn list_errors(&self, limit: i64) -> Result<Vec<ErrorRow>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, at, job_id, kind, message, context_json FROM errors ORDER BY id DESC LIMIT ?1")?;
            let rows = st.query_map(params![limit], |r| {
                let ctx: Option<String> = r.get(5)?;
                Ok(ErrorRow {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    job_id: r.get(2)?,
                    kind: r.get(3)?,
                    message: r.get(4)?,
                    context: ctx.and_then(|c| serde_json::from_str(&c).ok()),
                })
            })?;
            rows.collect()
        })
    }

    pub fn count_unresolved_errors(&self) -> Result<i64> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM errors WHERE at > datetime('now', '-7 days')", [], |r| r.get(0)))
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cd src-tauri && cargo test db::`
Expected: 6 tests pass. If `pragma_update` is unavailable in the rusqlite version resolved, replace it with `c.execute_batch(&format!("PRAGMA user_version = {version}"))`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db
git commit -m "Add the SQLite schema and repositories

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 4: Secrets store and typed settings

Implements spec §3 (config) and the `KeyringUnavailable` notice.

**Files:**
- Create: `src-tauri/src/secrets.rs`, `src-tauri/src/settings.rs`

**Interfaces:**
- Consumes: `db::Db` (`setting`, `set_setting`, `delete_setting`).
- Produces: `secrets::{SecretStore, SecretKey}` with `SecretStore::open(data_dir) -> SecretStore`, `get(&self, key) -> Result<Option<String>>`, `set(&self, key, value) -> Result<()>`, `delete(&self, key) -> Result<()>`, `backend(&self) -> &'static str` (`"keyring"` | `"file"`); `settings::{Settings, SettingsPatch}` with `Settings::load(&Db, &SecretStore) -> Result<Settings>`, `Settings::save(&Db, &SecretStore, &SettingsPatch) -> Result<()>`, `Settings::require_landau(&self) -> Result<(&str, &str, &str)>` (url, token, store), `Settings::require_supplier(&self) -> Result<supplier::Credentials>`.

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/settings.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::secrets::SecretStore;

    fn stores() -> (Db, SecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Db::in_memory().unwrap(), SecretStore::file_only(dir.path()), dir)
    }

    #[test]
    fn defaults_are_the_spec_values() {
        let (db, sec, _d) = stores();
        let s = Settings::load(&db, &sec).unwrap();
        assert_eq!(s.threshold, DEFAULT_THRESHOLD);
        assert_eq!(s.vat_percent, 10.0);
        assert_eq!(s.default_authenticity, Authenticity::Original);
        assert!(!s.overwrite_manual);
        assert!(s.landau_token.is_none());
        assert!(s.require_landau().is_err());
    }

    #[test]
    fn save_keeps_secrets_out_of_the_settings_table_and_empty_means_keep() {
        let (db, sec, _d) = stores();
        Settings::save(&db, &sec, &SettingsPatch {
            landau_url: Some("http://localhost:6150".into()),
            landau_token: Some("apt-abc".into()),
            store: Some("s1".into()),
            supplier_user: Some("0913".into()),
            supplier_password: Some("pw".into()),
            threshold: Some(77),
            ..Default::default()
        }).unwrap();
        assert_eq!(db.setting("landau_token").unwrap(), None);
        let s = Settings::load(&db, &sec).unwrap();
        assert_eq!(s.landau_token.as_deref(), Some("apt-abc"));
        assert_eq!(s.threshold, 77);
        let (url, tok, store) = s.require_landau().unwrap();
        assert_eq!((url, tok, store), ("http://localhost:6150", "apt-abc", "s1"));

        Settings::save(&db, &sec, &SettingsPatch { landau_token: Some(String::new()), ..Default::default() }).unwrap();
        assert_eq!(Settings::load(&db, &sec).unwrap().landau_token.as_deref(), Some("apt-abc"));
        let creds = Settings::load(&db, &sec).unwrap().require_supplier().unwrap();
        assert_eq!(creds.username, "0913");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test settings::`
Expected: compile errors.

- [ ] **Step 3: Implement `secrets.rs`**

```rust
//! Where the Landau key and the supplier password live. Keyring when the OS offers one, else a
//! 0600 JSON file next to the database — and the settings screen says which (spec §3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::errors::{BridgeError, Result};

const SERVICE: &str = "ir.landau.bridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    LandauToken,
    SupplierPassword,
}

impl SecretKey {
    fn name(self) -> &'static str {
        match self {
            SecretKey::LandauToken => "landau_token",
            SecretKey::SupplierPassword => "supplier_password",
        }
    }
}

#[derive(Clone)]
pub struct SecretStore {
    keyring: bool,
    file: PathBuf,
}

impl SecretStore {
    /// Probe the keyring once; fall back to the file store when it is unusable.
    pub fn open(data_dir: &Path) -> SecretStore {
        let file = data_dir.join("secrets.json");
        let keyring = match keyring::Entry::new(SERVICE, "probe") {
            Ok(entry) => entry.set_password("probe").and_then(|_| entry.delete_credential()).is_ok(),
            Err(_) => false,
        };
        if !keyring {
            tracing::warn!("keyring unavailable; secrets stored in {}", file.display());
        }
        SecretStore { keyring, file }
    }

    /// For tests and for machines with no secret service.
    pub fn file_only(data_dir: &Path) -> SecretStore {
        SecretStore { keyring: false, file: data_dir.join("secrets.json") }
    }

    pub fn backend(&self) -> &'static str {
        if self.keyring { "keyring" } else { "file" }
    }

    pub fn get(&self, key: SecretKey) -> Result<Option<String>> {
        if self.keyring {
            return match keyring::Entry::new(SERVICE, key.name()).and_then(|e| e.get_password()) {
                Ok(v) => Ok(Some(v)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(BridgeError::KeyringUnavailable(e.to_string())),
            };
        }
        Ok(self.read_file()?.get(key.name()).cloned())
    }

    pub fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        if self.keyring {
            return keyring::Entry::new(SERVICE, key.name())
                .and_then(|e| e.set_password(value))
                .map_err(|e| BridgeError::KeyringUnavailable(e.to_string()));
        }
        let mut map = self.read_file()?;
        map.insert(key.name().to_string(), value.to_string());
        self.write_file(&map)
    }

    pub fn delete(&self, key: SecretKey) -> Result<()> {
        if self.keyring {
            return match keyring::Entry::new(SERVICE, key.name()).and_then(|e| e.delete_credential()) {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(BridgeError::KeyringUnavailable(e.to_string())),
            };
        }
        let mut map = self.read_file()?;
        map.remove(key.name());
        self.write_file(&map)
    }

    fn read_file(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.file) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| BridgeError::Other(format!("secrets.json: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(BridgeError::Other(format!("secrets.json: {e}"))),
        }
    }

    fn write_file(&self, map: &BTreeMap<String, String>) -> Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BridgeError::Other(e.to_string()))?;
        }
        let text = serde_json::to_string_pretty(map)?;
        std::fs::write(&self.file, text).map_err(|e| BridgeError::Other(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.file, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Implement `settings.rs`**

```rust
//! Typed view over the `settings` table plus the secret store (spec §3, §10 screen 1).

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::domain::Authenticity;
use crate::errors::{BridgeError, Result};
use crate::secrets::{SecretKey, SecretStore};
use crate::supplier::Credentials;

/// Replaced by the calibrated value in Task 10 (docs/matching-threshold.md).
pub const DEFAULT_THRESHOLD: u8 = 80;
pub const DEFAULT_VAT_PERCENT: f64 = 10.0;
pub const SUPPLIER_ID: &str = "barezpakhsh";

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub landau_url: String,
    #[serde(skip_serializing)]
    pub landau_token: Option<String>,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    #[serde(skip_serializing)]
    pub supplier_password: Option<String>,
    pub default_authenticity: Authenticity,
    pub threshold: u8,
    pub vat_percent: f64,
    pub overwrite_manual: bool,
}

/// Fields a save may carry. `None` = untouched; an empty string on a secret = untouched too.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub landau_url: Option<String>,
    pub landau_token: Option<String>,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub supplier_password: Option<String>,
    pub default_authenticity: Option<Authenticity>,
    pub threshold: Option<u8>,
    pub vat_percent: Option<f64>,
    pub overwrite_manual: Option<bool>,
}

impl Settings {
    pub fn load(db: &Db, secrets: &SecretStore) -> Result<Settings> {
        let get = |k: &str| db.setting(k);
        Ok(Settings {
            landau_url: get("landau_url")?.unwrap_or_else(|| "http://localhost:6150".into()),
            landau_token: secrets.get(SecretKey::LandauToken)?,
            store: get("store")?,
            store_name: get("store_name")?,
            supplier_user: get("supplier_user")?,
            supplier_password: secrets.get(SecretKey::SupplierPassword)?,
            default_authenticity: get("default_authenticity")?
                .and_then(|v| Authenticity::parse(&v))
                .unwrap_or(Authenticity::Original),
            threshold: get("threshold")?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_THRESHOLD),
            vat_percent: get("vat_percent")?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_VAT_PERCENT),
            overwrite_manual: get("overwrite_manual")?.map(|v| v == "1").unwrap_or(false),
        })
    }

    pub fn save(db: &Db, secrets: &SecretStore, p: &SettingsPatch) -> Result<()> {
        if let Some(v) = &p.landau_url {
            db.set_setting("landau_url", v.trim().trim_end_matches('/'))?;
        }
        if let Some(v) = &p.landau_token {
            if !v.trim().is_empty() {
                secrets.set(SecretKey::LandauToken, v.trim())?;
            }
        }
        if let Some(v) = &p.store {
            db.set_setting("store", v)?;
        }
        if let Some(v) = &p.store_name {
            db.set_setting("store_name", v)?;
        }
        if let Some(v) = &p.supplier_user {
            db.set_setting("supplier_user", v.trim())?;
        }
        if let Some(v) = &p.supplier_password {
            if !v.is_empty() {
                secrets.set(SecretKey::SupplierPassword, v)?;
            }
        }
        if let Some(v) = p.default_authenticity {
            db.set_setting("default_authenticity", v.as_str())?;
        }
        if let Some(v) = p.threshold {
            db.set_setting("threshold", &v.min(100).to_string())?;
        }
        if let Some(v) = p.vat_percent {
            db.set_setting("vat_percent", &v.to_string())?;
        }
        if let Some(v) = p.overwrite_manual {
            db.set_setting("overwrite_manual", if v { "1" } else { "0" })?;
        }
        Ok(())
    }

    /// `(url, token, store)` or `SettingsIncomplete` naming the missing piece.
    pub fn require_landau(&self) -> Result<(&str, &str, &str)> {
        let token = self.landau_token.as_deref().filter(|t| !t.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("کلید API لاندا".into()))?;
        let store = self.store.as_deref().filter(|s| !s.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("فروشگاه مقصد در لاندا".into()))?;
        Ok((self.landau_url.as_str(), token, store))
    }

    pub fn require_supplier(&self) -> Result<Credentials> {
        let username = self.supplier_user.clone().filter(|u| !u.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("نام کاربری بارزپخش".into()))?;
        let password = self.supplier_password.clone().filter(|p| !p.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("کلمه عبور بارزپخش".into()))?;
        Ok(Credentials { username, password })
    }
}
```

`supplier::Credentials` does not exist yet — add the minimum to `src-tauri/src/supplier/mod.rs` now (Task 7 extends this file):
```rust
//! Supplier layer: one trait, one implementation today (spec §3, §4).

pub mod barezpakhsh;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
```

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test settings::`
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/secrets.rs src-tauri/src/settings.rs src-tauri/src/supplier/mod.rs
git commit -m "Add the secret store and typed settings

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Persian normalisation and similarity

Implements spec §6 (search term, similarity function). Pure functions with no I/O.

**Files:**
- Create: `src-tauri/src/matching/normalize.rs`, `src-tauri/src/matching/similarity.rs`
- Modify: `src-tauri/src/matching/mod.rs` (declare the two modules; Task 9 adds the rest)

**Interfaces:**
- Produces: `normalize::normalize_text(&str) -> String`, `normalize::key_text(&str) -> String`, `normalize::search_term(name: &str, code: &str) -> String`; `similarity::token_set_ratio(&str, &str) -> u8`.

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/matching/normalize.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unifies_arabic_letters_digits_and_zwnj() {
        assert_eq!(normalize_text("لنت‌ترمز  جلو پرايد ۴"), "لنت ترمز جلو پراید 4");
        assert_eq!(normalize_text("كاسه\tنمد"), "کاسه نمد");
    }

    #[test]
    fn key_text_drops_punctuation_and_case() {
        assert_eq!(key_text("سیبک فرمان (0631)(48) ABS"), "سیبک فرمان 0631 48 abs");
    }

    #[test]
    fn search_term_strips_pack_and_box_markers_and_a_trailing_code() {
        // «(0631)», «(48)», «(16)» are box counts / catalogue numbers the registry names never carry
        assert_eq!(search_term("سيبک فرمان پرايد اميرنيا (0631)(48) (2عددي)", "1032001010"), "سیبک فرمان پراید امیرنیا");
        assert_eq!(search_term("لنت جلو پژو 405 عظام* (16)", "3137006"), "لنت جلو پژو 405 عظام*");
        assert_eq!(search_term("بلبرینگ چرخ عقب پراید 3154028", "3154028"), "بلبرینگ چرخ عقب پراید");
        assert_eq!(search_term("قرقري فرمان پژو 405 اميرنيا <2>(0421)(40)", "1023002010"), "قرقری فرمان پژو 405 امیرنیا <2>");
        assert_eq!(search_term("واشر 10 عددی سرسیلندر", "1"), "واشر سرسیلندر");
        assert_eq!(search_term("لنت (چپ) پراید", "2"), "لنت (چپ) پراید", "a non-numeric group stays");
    }
}
```

`src-tauri/src/matching/similarity.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_100_and_disjoint_is_low() {
        assert_eq!(token_set_ratio("لنت جلو پژو 405", "لنت جلو پژو 405"), 100);
        assert!(token_set_ratio("لنت جلو پژو 405", "شمع موتور پراید") < 40);
    }

    #[test]
    fn word_order_and_extra_words_matter_little() {
        assert_eq!(token_set_ratio("پژو 405 لنت جلو", "لنت جلو پژو 405"), 100);
        assert!(token_set_ratio("لنت جلو پژو 405 عظام", "لنت جلو پژو 405") >= 85);
    }

    #[test]
    fn typos_cost_a_few_points() {
        let s = token_set_ratio("سیبک فرمان پراید امیرنیا", "سیبک فرمان پرايد اميرنيا");
        assert!(s >= 85 && s < 100, "{s}");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test matching::`
Expected: compile errors.

- [ ] **Step 3: Implement `normalize.rs`**

```rust
//! The same normalisation the registry was built with (spec §6), so a search term and a
//! candidate name are compared on equal footing.

use regex::Regex;
use std::sync::LazyLock;

use crate::domain::latin_digits;

static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[()\[\]{}\-_.,،;:؛"'«»/\\*+|!?؟#@&%^~`<>]+"#).unwrap());
/// «(10)», «( 10 )», «(10 عددی)», «(10عددي)», «(10 عدد)», «(10 تایی)», «(2 pcs)»
static PACK_PAREN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(\s*(\d{1,4})\s*(?:عددی|عدد|تایی|تا|pcs?|PCS?)?\s*\)").unwrap());
/// bare «10 عددی» / «10عددی» — the character before the digits is kept (`$1`)
static PACK_BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^\d])\d{1,4}\s*عددی").unwrap());

/// Unify Arabic/Persian glyphs, digits, ZWNJ and whitespace. Keeps punctuation.
pub fn normalize_text(s: &str) -> String {
    let mapped: String = s
        .chars()
        .filter_map(|c| match c {
            'ي' => Some('ی'),
            'ك' => Some('ک'),
            'ة' | 'ۀ' => Some('ه'),
            'أ' | 'إ' => Some('ا'),
            'ؤ' => Some('و'),
            '\u{0654}' | '\u{FEFF}' | '\u{200E}' | '\u{200F}' => None, // hamza above, BOM, LRM, RLM
            '\u{200C}' | '\t' => Some(' '),                             // ZWNJ, tab
            '٫' => Some('.'),
            '٬' => Some(','),
            '×' => Some('*'),
            other => Some(other),
        })
        .collect();
    WS.replace_all(latin_digits(&mapped).trim(), " ").to_string()
}

/// Lower-cased, punctuation-free, whitespace-collapsed — the key a similarity is measured on.
pub fn key_text(s: &str) -> String {
    let lowered = normalize_text(s).to_lowercase();
    WS.replace_all(PUNCT.replace_all(&lowered, " ").trim(), " ").to_string()
}

/// The invoice name as a search term: normalised, pack markers and a trailing SKU removed.
pub fn search_term(name: &str, code: &str) -> String {
    let mut s = normalize_text(name);
    let code = normalize_text(code);
    if code.len() >= 4 {
        if let Some(stripped) = s.strip_suffix(code.as_str()) {
            s = stripped.trim_end_matches([' ', '-', '_', '/']).to_string();
        }
    }
    let s = PACK_PAREN.replace_all(&s, " ");
    let s = PACK_BARE.replace_all(&s, "$1 ");
    WS.replace_all(s.trim(), " ").to_string()
}
```

- [ ] **Step 4: Implement `similarity.rs`**

```rust
//! fuzzywuzzy's `token_set_ratio`, the family the previous scraper matched with (spec §6).

use std::collections::BTreeSet;

use crate::matching::normalize::key_text;

/// 0–100 similarity of two strings after [`key_text`] normalisation.
pub fn ratio(a: &str, b: &str) -> u8 {
    (strsim::normalized_levenshtein(a, b) * 100.0).round() as u8
}

/// Split both into token sets; compare the intersection against each side and the two sides
/// against each other; keep the best. Insensitive to order and to extra words on one side.
pub fn token_set_ratio(a: &str, b: &str) -> u8 {
    let ka = key_text(a);
    let kb = key_text(b);
    let ta: BTreeSet<&str> = ka.split(' ').filter(|t| !t.is_empty()).collect();
    let tb: BTreeSet<&str> = kb.split(' ').filter(|t| !t.is_empty()).collect();
    if ta.is_empty() || tb.is_empty() {
        return 0;
    }
    let inter: Vec<&str> = ta.intersection(&tb).copied().collect();
    let only_a: Vec<&str> = ta.difference(&tb).copied().collect();
    let only_b: Vec<&str> = tb.difference(&ta).copied().collect();
    let t0 = inter.join(" ");
    let t1 = format!("{t0} {}", only_a.join(" ")).trim().to_string();
    let t2 = format!("{t0} {}", only_b.join(" ")).trim().to_string();
    [ratio(&t0, &t1), ratio(&t0, &t2), ratio(&t1, &t2)].into_iter().max().unwrap_or(0)
}
```

`src-tauri/src/matching/mod.rs`:
```rust
//! Part matching (spec §6). Task 9 adds scoring and decisions.

pub mod normalize;
pub mod similarity;
```

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test matching::`
Expected: 7 tests pass. If `typos_cost_a_few_points` fails because Arabic ye/kaf are unified by `normalize_text` (making the strings identical, score 100), change that test's second string to `"سیبک فرمان پراید امیرنا"` (a real one-letter typo) and keep the bounds.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/matching
git commit -m "Add Persian normalisation and token-set similarity

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 6: Barezpakhsh parsers (pure, fixture-tested)

Implements spec §2.2 and §4 parsing. Every function takes a string and returns typed data or `SupplierParse{page, field}`. The fixtures in `src-tauri/fixtures/barezpakhsh/` were captured from the live site on 2026-09-13 and anonymised (business id `11000`, code `97100000`).

**Files:**
- Create: `src-tauri/src/supplier/barezpakhsh/parse.rs`, `src-tauri/src/supplier/barezpakhsh/mod.rs` (`pub mod parse; pub mod client;` — `client` arrives in Task 7; declare it then)

**Interfaces:**
- Consumes: `domain::*`, `errors::*`.
- Produces (all `pub` in `parse`): `is_blocked_page(&str) -> bool`, `is_login_page(&str) -> bool`, `antiforgery_token(html, page) -> Result<String>`, `form_fields(html, action, page) -> Result<Vec<(String, String)>>`, `LedgerContext{total, page_size, year_id, business_code}` + `ledger_context(html) -> Result<LedgerContext>`, `LedgerRow{…}` + `ledger_rows(body) -> Result<Vec<LedgerRow>>`, `header_of(&LedgerRow) -> Option<InvoiceHeader>`, `invoice_lines(html) -> Result<Vec<InvoiceLine>>`, `BuyContext{business_id, user_group_id}` + `buy_context(html) -> Result<BuyContext>`, `price_quotes(body) -> Result<Vec<PriceQuote>>`, `digits(&str) -> Option<i64>`.

- [ ] **Step 1: Write the failing tests** (bottom of `parse.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const LOGIN: &str = include_str!("../../../fixtures/barezpakhsh/login.html");
    const LOGIN_FAILED: &str = include_str!("../../../fixtures/barezpakhsh/login-failed.html");
    const BLOCKED: &str = include_str!("../../../fixtures/barezpakhsh/blocked-403.html");
    const BILL: &str = include_str!("../../../fixtures/barezpakhsh/bill.html");
    const MOEIN: &str = include_str!("../../../fixtures/barezpakhsh/bill-moein2.html");
    const MOEIN_PAGE2: &str = include_str!("../../../fixtures/barezpakhsh/moein2-page2.json");
    const FACTOR: &str = include_str!("../../../fixtures/barezpakhsh/factor-8818975.html");
    const RETURN: &str = include_str!("../../../fixtures/barezpakhsh/return-9066582.html");
    const BUY_SEARCH: &str = include_str!("../../../fixtures/barezpakhsh/buy-search-3137006.html");
    const ITEMS: &str = include_str!("../../../fixtures/barezpakhsh/items-3137006.json");

    #[test]
    fn recognises_login_and_blocked_pages() {
        assert!(is_login_page(LOGIN));
        assert!(is_login_page(LOGIN_FAILED));
        assert!(!is_login_page(BILL));
        assert!(is_blocked_page(BLOCKED));
        assert!(!is_blocked_page(LOGIN));
        assert_eq!(antiforgery_token(LOGIN, "login").unwrap(), "TOKEN-FIXTURE");
        assert!(antiforgery_token(BLOCKED, "login").is_err());
    }

    #[test]
    fn reads_the_statement_form_without_buttons() {
        let fields = form_fields(BILL, "/Shop/Report/Bill", "bill").unwrap();
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("BusinessCode"), Some("97100000"));
        assert_eq!(get("BusinessId"), Some("11000"));
        assert_eq!(get("FromDate"), Some("1405/4/24"));
        assert_eq!(get("SelectedFiscalYear.fiscalYearID"), Some("0"));
        assert_eq!(get("command"), None);
        assert_eq!(get("__RequestVerificationToken"), Some("TOKEN-FIXTURE"));
    }

    #[test]
    fn reads_the_ledger_paging_context() {
        let ctx = ledger_context(MOEIN).unwrap();
        assert_eq!(ctx, LedgerContext { total: 3107, page_size: 15, year_id: 0, business_code: "97100000".into() });
        assert_eq!(ctx.page_count(), 208);
    }

    #[test]
    fn parses_ledger_rows_and_keeps_only_invoices_and_returns() {
        let rows = ledger_rows(MOEIN_PAGE2).unwrap();
        assert_eq!(rows.len(), 15);
        let first = &rows[0];
        assert_eq!(first.trans_number, "226456");
        assert_eq!(first.doc_alias_id, "2");
        let h = header_of(first).unwrap();
        assert_eq!(h, InvoiceHeader {
            number: "226456".into(),
            kind: InvoiceKind::Sale,
            date_jalali: "1401/10/14".into(),
            date_iso: "2023-01-04".into(),
            paid: true,
            url: "/Shop/Preview/Factor/8818975/11000".into(),
        });
        let headers: Vec<_> = rows.iter().filter_map(header_of).collect();
        assert_eq!(headers.len(), 10, "five receipts (alias 67) are dropped");
        let receipt = rows.iter().find(|r| r.doc_alias_id == "67").unwrap();
        assert!(header_of(receipt).is_none());
        let ret = LedgerRow { doc_alias_id: "6".into(), status: "پرداخت شده".into(), ..first.clone() };
        assert_eq!(header_of(&ret).unwrap().kind, InvoiceKind::Return);
        let unpaid = LedgerRow { status: "پرداخت نشده".into(), ..first.clone() };
        assert!(!header_of(&unpaid).unwrap().paid);
    }

    #[test]
    fn parses_sale_invoice_lines() {
        let lines = invoice_lines(FACTOR).unwrap();
        assert_eq!(lines.len(), 36);
        assert_eq!(lines[0], InvoiceLine {
            line_no: 1,
            code: "3137006".into(),
            name: "لنت جلو پژو 405 عظام* (16)".into(),
            qty: 4,
            unit: "دست".into(),
            unit_price: 1_647_000,
        });
        assert_eq!(lines[35].code, "3167247");
        assert_eq!(lines.iter().map(|l| l.qty).sum::<i64>(), 135, "matches the invoice's «جمع» row");
    }

    #[test]
    fn parses_return_lines_with_the_same_reader() {
        let lines = invoice_lines(RETURN).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].code, "1023002010");
        assert_eq!(lines[0].qty, 8);
        assert_eq!(lines[0].unit_price, 852_000);
        assert_eq!(lines[1].line_no, 2);
    }

    #[test]
    fn reads_the_price_search_context_and_quotes() {
        assert_eq!(buy_context(BUY_SEARCH).unwrap(), BuyContext { business_id: "11000".into(), user_group_id: "70".into() });
        let quotes = price_quotes(ITEMS).unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0], PriceQuote {
            code: "3137006".into(),
            name: "لنت جلو پژو 405 عظام* (16)".into(),
            list_price: 10_731_000,
            vat_percent: Some(10.0),
            has_vat: true,
            available: true,
            unit: "دست".into(),
        });
    }

    #[test]
    fn digits_reads_persian_and_grouped_numbers() {
        assert_eq!(digits("1,647,000"), Some(1_647_000));
        assert_eq!(digits("۴"), Some(4));
        assert_eq!(digits(" 7,429,440 "), Some(7_429_440));
        assert_eq!(digits("&nbsp;"), None);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test barezpakhsh::parse`
Expected: compile errors.

- [ ] **Step 3: Implement `parse.rs`**

```rust
//! Pure parsers over Barezpakhsh pages (spec §2.2, §4). No network, no state.

use std::sync::LazyLock;

use regex::Regex;
use roxmltree::Document;
use scraper::{ElementRef, Html, Selector};

use crate::domain::{latin_digits, normalize_jalali, InvoiceHeader, InvoiceKind, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn missing(page: &str, field: &str) -> BridgeError {
    BridgeError::SupplierParse { page: page.into(), field: field.into() }
}

static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"name="__RequestVerificationToken" type="hidden" value="([^"]+)""#).unwrap());

fn js_var(html: &str, name: &str) -> Option<String> {
    Regex::new(&format!(r"var {name} = '([^']*)'")).ok()?.captures(html).map(|c| c[1].to_string())
}

/// Visible text of an element, whitespace collapsed.
pub fn text_of(el: ElementRef<'_>) -> String {
    let joined: String = el.text().collect::<Vec<_>>().join(" ");
    WS.replace_all(joined.replace('\u{a0}', " ").trim(), " ").to_string()
}

/// `"1,647,000"` / `"۴"` → number; `None` when there is no digit at all.
pub fn digits(s: &str) -> Option<i64> {
    let d: String = latin_digits(s).chars().filter(|c| c.is_ascii_digit()).collect();
    d.parse().ok()
}

pub fn is_blocked_page(html: &str) -> bool {
    html.contains("error-section__status") || html.contains("Access Denied")
}

pub fn is_login_page(html: &str) -> bool {
    html.contains(r#"action="/Home/Login""#) && html.contains(r#"name="Username""#)
}

pub fn antiforgery_token(html: &str, page: &str) -> Result<String> {
    TOKEN.captures(html).map(|c| c[1].to_string()).ok_or_else(|| missing(page, "__RequestVerificationToken"))
}

/// Every `input[name]` (except buttons and unchecked checkboxes) and `select[name]` of the form
/// with `action`, in document order — what a browser would submit before the seller touches it.
pub fn form_fields(html: &str, action: &str, page: &str) -> Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let form_sel = Selector::parse(&format!(r#"form[action="{action}"]"#)).map_err(|_| missing(page, "form"))?;
    let form = doc.select(&form_sel).next().ok_or_else(|| missing(page, "form"))?;
    let mut out = Vec::new();
    for el in form.select(&sel("input[name], select[name]")) {
        let v = el.value();
        let name = v.attr("name").unwrap_or_default().to_string();
        match v.name() {
            "input" => {
                let ty = v.attr("type").unwrap_or("text").to_ascii_lowercase();
                if matches!(ty.as_str(), "submit" | "button" | "image" | "reset") {
                    continue;
                }
                if ty == "checkbox" && v.attr("checked").is_none() {
                    continue;
                }
                out.push((name, v.attr("value").unwrap_or_default().to_string()));
            }
            "select" => {
                let selected = el.select(&sel("option[selected]")).next().or_else(|| el.select(&sel("option")).next());
                let value = selected.map(|o| o.value().attr("value").map(str::to_string).unwrap_or_else(|| text_of(o)));
                out.push((name, value.unwrap_or_default()));
            }
            _ => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- ledger («معین فاکتور»)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerContext {
    pub total: u32,
    pub page_size: u32,
    pub year_id: i64,
    pub business_code: String,
}

impl LedgerContext {
    pub fn page_count(&self) -> u32 {
        self.total.div_ceil(self.page_size.max(1))
    }
}

/// The paging constants the page's own script uses for `/Shop/Share/GradualMoein2Load`.
pub fn ledger_context(html: &str) -> Result<LedgerContext> {
    let page = "moein2";
    let total = js_var(html, "tmp").and_then(|v| v.parse().ok()).ok_or_else(|| missing(page, "tmp"))?;
    let page_size = js_var(html, "tmpPageSize").and_then(|v| v.parse().ok()).ok_or_else(|| missing(page, "tmpPageSize"))?;
    let year_id = js_var(html, "tmpYearId").and_then(|v| v.parse().ok()).ok_or_else(|| missing(page, "tmpYearId"))?;
    let business_code = js_var(html, "tmpBusCode").ok_or_else(|| missing(page, "tmpBusCode"))?;
    Ok(LedgerContext { total, page_size, year_id, business_code })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LedgerRow {
    pub doc_alias_id: String,
    pub doc_alias: String,
    pub trans_number: String,
    pub voucher_date: String,
    pub voucher_date_old: String,
    pub transaction_id: String,
    pub business_id: String,
    pub status: String,
}

/// The ajax endpoints answer a JSON *string* whose content is XML.
fn unwrap_json_xml(body: &str, page: &str) -> Result<String> {
    serde_json::from_str::<String>(body).map_err(|_| missing(page, "json string body"))
}

fn child_text(node: roxmltree::Node<'_, '_>, tag: &str) -> String {
    node.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text()).unwrap_or("").trim().to_string()
}

pub fn ledger_rows(body: &str) -> Result<Vec<LedgerRow>> {
    let page = "moein2-page";
    let xml = unwrap_json_xml(body, page)?;
    let doc = Document::parse(&xml).map_err(|_| missing(page, "xml"))?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("result"))
        .map(|n| LedgerRow {
            doc_alias_id: child_text(n, "DocAliasID"),
            doc_alias: child_text(n, "DocAliasDescription"),
            trans_number: child_text(n, "TransNumber"),
            voucher_date: child_text(n, "VoucherDate"),
            voucher_date_old: child_text(n, "VoucherDateOld"),
            transaction_id: child_text(n, "TransactionID"),
            business_id: child_text(n, "BusinessID"),
            status: child_text(n, "status"),
        })
        .collect())
}

/// `DocAliasID` 2 = «فاکتور فروش», 6 = «مرجوع از فروش»; 67 («دريافت», a payment) and anything else
/// is not an invoice. Payment state comes from `status`.
pub fn header_of(row: &LedgerRow) -> Option<InvoiceHeader> {
    let kind = match row.doc_alias_id.as_str() {
        "2" => InvoiceKind::Sale,
        "6" => InvoiceKind::Return,
        _ => return None,
    };
    Some(InvoiceHeader {
        number: row.trans_number.clone(),
        kind,
        date_jalali: normalize_jalali(&row.voucher_date).unwrap_or_else(|| row.voucher_date.clone()),
        date_iso: row.voucher_date_old.chars().take(10).collect(),
        paid: row.status.trim() == "پرداخت شده",
        url: format!("/Shop/Preview/Factor/{}/{}", row.transaction_id, row.business_id),
    })
}

// ---------------------------------------------------------------- invoice pages

/// Lines of a sale or a return page. Both print the same six leading cells once the mobile-only
/// `td.visible-xs` is dropped: row no, code, name, quantity, unit, unit price.
pub fn invoice_lines(html: &str) -> Result<Vec<InvoiceLine>> {
    let page = "factor";
    let doc = Html::parse_document(html);
    let table = doc
        .select(&sel("table"))
        .find(|t| text_of(*t).contains("کد کالا"))
        .ok_or_else(|| missing(page, "lines table"))?;
    let mut lines = Vec::new();
    for tr in table.select(&sel("tr")) {
        let cells: Vec<String> = tr
            .select(&sel("td"))
            .filter(|td| !td.value().attr("class").unwrap_or("").contains("visible-xs"))
            .map(text_of)
            .collect();
        if cells.len() < 6 {
            continue;
        }
        let Some(line_no) = digits(&cells[0]).filter(|_| latin_digits(&cells[0]).chars().all(|c| c.is_ascii_digit())) else {
            continue; // the «جمع» row and any header row
        };
        let qty = digits(&cells[3]).ok_or_else(|| missing(page, "quantity"))?;
        let unit_price = digits(&cells[5]).ok_or_else(|| missing(page, "unit price"))?;
        lines.push(InvoiceLine {
            line_no: line_no as u32,
            code: latin_digits(&cells[1]),
            name: cells[2].clone(),
            qty,
            unit: cells[4].clone(),
            unit_price,
        });
    }
    if lines.is_empty() {
        return Err(missing(page, "lines"));
    }
    Ok(lines)
}

// ---------------------------------------------------------------- price list («ثبت فاکتور»)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyContext {
    pub business_id: String,
    pub user_group_id: String,
}

/// From a `POST /Shop/Buy/Index` reply (the ids only appear after a search).
pub fn buy_context(html: &str) -> Result<BuyContext> {
    let page = "buy";
    Ok(BuyContext {
        business_id: js_var(html, "tmpBusId").ok_or_else(|| missing(page, "tmpBusId"))?,
        user_group_id: js_var(html, "UserGroupId").ok_or_else(|| missing(page, "UserGroupId"))?,
    })
}

pub fn price_quotes(body: &str) -> Result<Vec<PriceQuote>> {
    let page = "items-search";
    let xml = unwrap_json_xml(body, page)?;
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }
    let doc = Document::parse(&xml).map_err(|_| missing(page, "xml"))?;
    doc.descendants()
        .filter(|n| n.has_tag_name("result"))
        .map(|n| {
            let list_price = child_text(n, "FinalPrice").parse::<f64>().map_err(|_| missing(page, "FinalPrice"))?.round() as i64;
            let qty: f64 = child_text(n, "QtyAvailable").parse().unwrap_or(0.0);
            Ok(PriceQuote {
                code: latin_digits(&child_text(n, "ItemCode")),
                name: child_text(n, "ItemName"),
                list_price,
                vat_percent: child_text(n, "TaxAndTollPercent").parse().ok(),
                has_vat: child_text(n, "HasTaxAndToll") == "true",
                available: qty > 0.0,
                unit: child_text(n, "UnitName"),
            })
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test barezpakhsh::parse`
Expected: 8 tests pass. `Option::unwrap_or_default` on `Option<&str>` needs Rust ≥ 1.83 — the toolchain here is 1.97; if a lower toolchain complains, write `.unwrap_or("")`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/supplier src-tauri/fixtures
git commit -m "Add the Barezpakhsh page parsers with captured fixtures

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 7: Barezpakhsh HTTP client behind the `Supplier` trait

Implements spec §3 (supplier layer) and §4 (login, ledger, invoice, price). Plain HTTP with a cookie jar, proxy bypassed, one automatic re-login. Verified by a live test.

**Files:**
- Modify: `src-tauri/src/supplier/mod.rs` (trait, `Progress`, `open()`)
- Create: `src-tauri/src/supplier/barezpakhsh/client.rs`, `src-tauri/tests/live_supplier.rs`
- Modify: `src-tauri/src/supplier/barezpakhsh/mod.rs` → `pub mod client; pub mod parse;`

**Interfaces:**
- Consumes: `parse::*`, `domain::*`, `errors::*`, `supplier::Credentials`.
- Produces: `supplier::{Supplier, Progress, open}`; `barezpakhsh::client::{Client, BASE_URL}`.

```rust
pub type Progress = std::sync::Arc<dyn Fn(u32, u32) + Send + Sync>;   // (done, total)

#[async_trait::async_trait]
pub trait Supplier: Send + Sync {
    fn id(&self) -> &'static str;
    async fn login(&self) -> Result<()>;
    async fn ledger(&self, progress: Progress) -> Result<Vec<InvoiceHeader>>;
    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>>;
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>>;
}
pub fn open(id: &str, creds: Credentials) -> Result<Box<dyn Supplier>>;
```

- [ ] **Step 1: Write the live test** (`src-tauri/tests/live_supplier.rs`)

```rust
//! Runs only with `--features live` and BAREZ_USER / BAREZ_PASS set. Never carries a value.
#![cfg(feature = "live")]

use std::sync::Arc;

use landau_bridge_lib::domain::InvoiceKind;
use landau_bridge_lib::supplier::{open, Credentials};

fn creds() -> Option<Credentials> {
    let username = std::env::var("BAREZ_USER").ok()?;
    let password = std::env::var("BAREZ_PASS").ok()?;
    Some(Credentials { username, password })
}

#[tokio::test]
async fn logs_in_reads_the_ledger_an_invoice_and_a_price() {
    let Some(creds) = creds() else {
        eprintln!("skipped: BAREZ_USER/BAREZ_PASS not set");
        return;
    };
    let supplier = open("barezpakhsh", creds).unwrap();
    supplier.login().await.expect("login");

    let headers = supplier.ledger(Arc::new(|done, total| eprintln!("ledger {done}/{total}"))).await.expect("ledger");
    assert!(headers.len() > 100, "the fixture account has thousands of ledger rows");
    assert!(headers.iter().any(|h| h.kind == InvoiceKind::Sale && h.paid));
    let sale = headers.iter().find(|h| h.kind == InvoiceKind::Sale && h.paid).unwrap();
    assert!(sale.date_jalali.len() == 10, "{}", sale.date_jalali);

    let lines = supplier.invoice_lines(&sale.url).await.expect("lines");
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| !l.code.is_empty() && l.qty > 0));

    let quote = supplier.price_of(&lines[0].code).await.expect("price");
    let quote = quote.expect("a purchased part is still in the price list");
    assert_eq!(quote.code, lines[0].code);
    assert!(quote.list_price > 0);
    assert_eq!(quote.vat_percent, Some(10.0));

    assert!(supplier.price_of("00000000-does-not-exist").await.unwrap().is_none());
}

#[tokio::test]
async fn wrong_password_is_bad_credentials() {
    let Some(creds) = creds() else {
        eprintln!("skipped: BAREZ_USER/BAREZ_PASS not set");
        return;
    };
    let supplier = open("barezpakhsh", Credentials { password: "definitely-wrong".into(), ..creds }).unwrap();
    let err = supplier.login().await.unwrap_err();
    assert_eq!(err.kind(), "SupplierBadCredentials");
}
```

- [ ] **Step 2: Run to verify it fails to compile**

Run: `cd src-tauri && cargo test --features live --test live_supplier`
Expected: compile error — `open`/`Supplier` missing.

- [ ] **Step 3: Extend `supplier/mod.rs`**

```rust
//! Supplier layer: one trait, one implementation today (spec §3, §4). A second supplier is a
//! new module plus one arm in [`open`].

pub mod barezpakhsh;

use std::sync::Arc;

use crate::domain::{InvoiceHeader, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// `(done, total)` callback for long reads.
pub type Progress = Arc<dyn Fn(u32, u32) + Send + Sync>;

#[async_trait::async_trait]
pub trait Supplier: Send + Sync {
    fn id(&self) -> &'static str;
    /// Establish a session. Idempotent.
    async fn login(&self) -> Result<()>;
    /// Every sale and return header, oldest first. Receipts are not returned.
    async fn ledger(&self, progress: Progress) -> Result<Vec<InvoiceHeader>>;
    /// The lines of one invoice page (`InvoiceHeader::url`).
    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>>;
    /// Today's list price of one code, `None` when the site no longer lists it.
    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>>;
}

pub fn open(id: &str, creds: Credentials) -> Result<Box<dyn Supplier>> {
    match id {
        barezpakhsh::client::ID => Ok(Box::new(barezpakhsh::client::Client::new(creds)?)),
        other => Err(BridgeError::Other(format!("unknown supplier {other}"))),
    }
}
```

- [ ] **Step 4: Implement `barezpakhsh/client.rs`**

```rust
//! The Barezpakhsh session (spec §4): anti-forgery login, ledger paging, invoice pages, price
//! search. Every request logs method, path and status; never the credentials.

use std::time::Duration;

use reqwest::{Client as Http, Url};
use serde_json::json;
use tokio::sync::Mutex;

use super::parse::{self, BuyContext};
use crate::domain::{InvoiceHeader, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};
use crate::supplier::{Credentials, Progress, Supplier};

pub const ID: &str = "barezpakhsh";
pub const BASE_URL: &str = "https://33314466.ir";
/// Earliest date the statement form is asked for — "every invoice to date".
const LEDGER_FROM: &str = "1390/01/01";
const USER_AGENT: &str = "LandauBridge/0.1 (+https://landau.ir)";

/// Defaults the price search needs, learned once per session from the «ثبت فاکتور» page.
#[derive(Debug, Clone)]
struct BuySession {
    ctx: BuyContext,
    car: String,
    brand: String,
    group: String,
}

pub struct Client {
    http: Http,
    base: Url,
    creds: Credentials,
    logged_in: Mutex<bool>,
    buy: Mutex<Option<BuySession>>,
}

impl Client {
    pub fn new(creds: Credentials) -> Result<Self> {
        Self::with_base(creds, BASE_URL)
    }

    pub fn with_base(creds: Credentials, base: &str) -> Result<Self> {
        let http = Http::builder()
            .cookie_store(true)
            .no_proxy()
            .gzip(true)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| BridgeError::SupplierUnreachable(e.to_string()))?;
        let base = Url::parse(base).map_err(|e| BridgeError::Other(format!("base url: {e}")))?;
        Ok(Client { http, base, creds, logged_in: Mutex::new(false), buy: Mutex::new(None) })
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base.join(path).map_err(|e| BridgeError::Other(format!("url {path}: {e}")))
    }

    fn unreachable(e: reqwest::Error) -> BridgeError {
        BridgeError::SupplierUnreachable(e.without_url().to_string())
    }

    async fn get(&self, path: &str) -> Result<(Url, String)> {
        let res = self.http.get(self.url(path)?).send().await.map_err(Self::unreachable)?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "GET", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Ok((url, body))
    }

    async fn post_form(&self, path: &str, fields: &[(String, String)]) -> Result<(Url, String)> {
        let res = self.http.post(self.url(path)?).form(fields).send().await.map_err(Self::unreachable)?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "POST", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Ok((url, body))
    }

    async fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        let res = self
            .http
            .post(self.url(path)?)
            .header("content-type", "application/json; charset=utf-8")
            .header("x-requested-with", "XMLHttpRequest")
            .body(body.to_string())
            .send()
            .await
            .map_err(Self::unreachable)?;
        let status = res.status();
        tracing::debug!(method = "POST", path, %status, "barezpakhsh json");
        let text = res.text().await.map_err(Self::unreachable)?;
        if !status.is_success() {
            return Err(BridgeError::SupplierParse { page: path.into(), field: format!("status {status}") });
        }
        Ok(text)
    }

    async fn do_login(&self) -> Result<()> {
        let (_, page) = self.get("/Home/Login").await?;
        let token = parse::antiforgery_token(&page, "login")?;
        let fields = [
            ("__RequestVerificationToken".to_string(), token),
            ("Username".to_string(), self.creds.username.clone()),
            ("Password".to_string(), self.creds.password.clone()),
        ];
        let (url, body) = self.post_form("/Home/Login", &fields).await?;
        if url.path().starts_with("/Shop") {
            tracing::info!("barezpakhsh login ok");
            *self.logged_in.lock().await = true;
            return Ok(());
        }
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierBadCredentials);
        }
        Err(BridgeError::SupplierParse { page: "login".into(), field: "outcome".into() })
    }

    async fn ensure_session(&self) -> Result<()> {
        if *self.logged_in.lock().await {
            return Ok(());
        }
        self.do_login().await
    }

    /// GET an authenticated page; on the login page, log in once more and retry.
    async fn get_authed(&self, path: &str) -> Result<String> {
        self.ensure_session().await?;
        let (_, body) = self.get(path).await?;
        if !parse::is_login_page(&body) {
            return Ok(body);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *self.logged_in.lock().await = false;
        self.do_login().await.map_err(|_| BridgeError::SupplierSessionExpired)?;
        let (_, body) = self.get(path).await?;
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(body)
    }

    /// Replace (or add) a field value.
    fn set_field(fields: &mut Vec<(String, String)>, name: &str, value: &str) {
        fields.retain(|(n, _)| n != name);
        fields.push((name.to_string(), value.to_string()));
    }

    async fn buy_session(&self) -> Result<BuySession> {
        if let Some(s) = self.buy.lock().await.clone() {
            return Ok(s);
        }
        let page = self.get_authed("/Shop/Buy/Index").await?;
        let mut fields = parse::form_fields(&page, "/Shop/Buy/Index", "buy")?;
        let pick = |fields: &[(String, String)], k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
        let car = pick(&fields, "SelectedCarCode");
        let brand = pick(&fields, "SelectedBrandCode");
        let group = pick(&fields, "SelectedGroupCode");
        Self::set_field(&mut fields, "SearchTerm", "-");
        Self::set_field(&mut fields, "JustAvailable", "false");
        Self::set_field(&mut fields, "SelectedPageSize", "50");
        Self::set_field(&mut fields, "command", "searchItem");
        let (_, html) = self.post_form("/Shop/Buy/Index", &fields).await?;
        let ctx = parse::buy_context(&html)?;
        let session = BuySession { ctx, car, brand, group };
        *self.buy.lock().await = Some(session.clone());
        Ok(session)
    }
}

#[async_trait::async_trait]
impl Supplier for Client {
    fn id(&self) -> &'static str {
        ID
    }

    async fn login(&self) -> Result<()> {
        *self.logged_in.lock().await = false;
        self.do_login().await
    }

    async fn ledger(&self, progress: Progress) -> Result<Vec<InvoiceHeader>> {
        let page = self.get_authed("/Shop/Report/Bill").await?;
        let mut fields = parse::form_fields(&page, "/Shop/Report/Bill", "bill")?;
        Self::set_field(&mut fields, "FromDate", LEDGER_FROM);
        Self::set_field(&mut fields, "ToDate", "");
        Self::set_field(&mut fields, "command", "moein2");
        let (_, html) = self.post_form("/Shop/Report/Bill", &fields).await?;
        let ctx = parse::ledger_context(&html)?;
        let pages = ctx.page_count();
        let mut headers = Vec::new();
        for index in 1..=pages {
            let body = self
                .post_json(
                    "/Shop/Share/GradualMoein2Load",
                    &json!({
                        "pageIndex": index,
                        "yearId": ctx.year_id,
                        "businessCode": ctx.business_code,
                        "fromDate": LEDGER_FROM,
                        "toDate": ""
                    }),
                )
                .await?;
            headers.extend(parse::ledger_rows(&body)?.iter().filter_map(parse::header_of));
            progress(index, pages);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        headers.sort_by(|a, b| a.date_jalali.cmp(&b.date_jalali).then(a.number.cmp(&b.number)));
        headers.dedup_by(|a, b| a.number == b.number && a.kind == b.kind);
        Ok(headers)
    }

    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>> {
        let html = self.get_authed(url).await?;
        parse::invoice_lines(&html)
    }

    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        let s = self.buy_session().await?;
        let body = self
            .post_json(
                "/Shop/Share/ItemsSearchGradual",
                &json!({
                    "selectedGroupCode": s.group,
                    "selectedCarCode": s.car,
                    "selectedBrandCode": s.brand,
                    "SpecialItemsId": "",
                    "searchTerm": code,
                    "businessId": s.ctx.business_id,
                    "pageIndex": 1,
                    "userGroupId": s.ctx.user_group_id,
                    "justAvailable": false,
                    "pageSize": "50"
                }),
            )
            .await?;
        let quotes = parse::price_quotes(&body)?;
        Ok(quotes.into_iter().find(|q| q.code == code))
    }
}
```

- [ ] **Step 5: Run offline gates, then the live test**

Run: `cd src-tauri && cargo clippy --all-targets --features live -- -D warnings && cargo test`
Expected: green (the live test file compiles but is skipped without the feature).

Run (credentials exported from the prior scraper's code, never typed into a file in this repo):
```bash
export BAREZ_USER=$(grep -o '"USER": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4)
export BAREZ_PASS=$(grep -o '"PASS": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4)
cd src-tauri && cargo test --features live --test live_supplier -- --nocapture
```
Expected: both tests pass; the ledger progress prints up to ~208 pages (≈1–2 min).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/supplier src-tauri/tests/live_supplier.rs
git commit -m "Add the Barezpakhsh HTTP session behind the Supplier trait

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 8: Landau client

Implements spec §2.1 and §5. One struct, one method per route, envelope-tolerant JSON reading (`body.data ?? body`), typed refusals.

**Files:**
- Create: `src-tauri/src/landau/mod.rs`, `src-tauri/tests/live_landau.rs`

**Interfaces:**
- Consumes: `errors::*`, `domain::Authenticity`.
- Produces: `landau::{LandauClient, Membership, Store, ShelfRow, Candidate, PricingRule, RepricedRow, AttachBody, PatchBody}` with
  `LandauClient::new(base_url, token) -> Result<LandauClient>`, `memberships()`, `shelf(store)`, `registry_search(term, limit, skip) -> Result<RegistryPage>` (the seller's picker — a substring search over the whole registry; **replaces the ES `search()` of the original spec per §13**), `attach(&AttachBody) -> Result<ShelfRow>`, `patch(id, &PatchBody)`, `delete(id)`, `pricing_rules()`, `reprice(store, overwrite) -> Result<Vec<RepricedRow>>`, `await_visible(store, ids, timeout) -> Result<Vec<String>>` (the ids still missing).

- [ ] **Step 1: Write the unit tests for the pure helpers and the live test**

Unit tests (bottom of `landau/mod.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peels_the_data_envelope_only_when_present() {
        let enveloped = serde_json::json!({"data": {"id": "1"}});
        let flat = serde_json::json!({"id": "2"});
        assert_eq!(peel(&enveloped)["id"], "1");
        assert_eq!(peel(&flat)["id"], "2");
    }

    #[test]
    fn shelf_rows_read_price_source_and_variant() {
        let item = serde_json::json!({
            "stock": {"id": "s1", "product": "p1", "store": "st", "inventory": 3, "price": 173000.0, "cost": 150000,
                      "condition": "NEW", "authenticity": "ORIGINAL", "priced": {"source": "RULE", "rule": "r1"}},
            "product": {"id": "p1", "name": "لنت جلو"}
        });
        let row = ShelfRow::from_item(&item).unwrap();
        assert_eq!(row.id, "s1");
        assert_eq!(row.price, Some(173000));
        assert_eq!(row.cost, Some(150000));
        assert_eq!(row.price_source.as_deref(), Some("RULE"));
        assert_eq!(row.product_name.as_deref(), Some("لنت جلو"));
        assert_eq!(row.authenticity.as_deref(), Some("ORIGINAL"));
    }

    #[test]
    fn attach_body_is_what_landau_expects() {
        let body = AttachBody { product: "p".into(), store: "s".into(), inventory: 2, cost: 1100, authenticity: Authenticity::Fake };
        let v = serde_json::to_value(body.to_json()).unwrap();
        assert_eq!(v["type"], "NORMAL");
        assert_eq!(v["condition"], "NEW");
        assert_eq!(v["authenticity"], "FAKE");
        assert_eq!(v["price"], 1100);
        assert_eq!(v["cost"], 1100);
        assert_eq!(v["inventory"], 2);
    }

    #[test]
    fn registry_page_reads_the_picker_shape() {
        let v = serde_json::json!({"items": [{"product": {"id": "p1", "name": "سیبک فرمان پراید", "alias": "سیبک | جعبه فرمان", "brand": "امیرنیا"}, "held": []}], "total": 14, "limit": 100, "skip": 0});
        let page = peel(&v);
        let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
        assert_eq!(total, 14);
        let p = page["items"][0].get("product").unwrap();
        assert_eq!(s(p, "id").as_deref(), Some("p1"));
        assert_eq!(s(p, "brand").as_deref(), Some("امیرنیا"));
    }

    #[test]
    fn patch_body_never_carries_price() {
        let v = PatchBody { inventory: Some(5), cost: Some(1200) }.to_json();
        assert!(v.get("price").is_none());
        assert_eq!(v["inventory"], 5);
        let v = PatchBody { inventory: None, cost: Some(1200) }.to_json();
        assert!(v.get("inventory").is_none());
    }
}
```

`src-tauri/tests/live_landau.rs`:
```rust
//! Runs only with `--features live` and LND_API_URL / LND_APT_TOKEN / LND_STORE set.
#![cfg(feature = "live")]

use std::time::Duration;

use landau_bridge_lib::domain::Authenticity;
use landau_bridge_lib::landau::{AttachBody, LandauClient, PatchBody};

fn client() -> Option<(LandauClient, String)> {
    let url = std::env::var("LND_API_URL").ok()?;
    let token = std::env::var("LND_APT_TOKEN").ok()?;
    let store = std::env::var("LND_STORE").ok()?;
    Some((LandauClient::new(&url, &token).unwrap(), store))
}

#[tokio::test]
async fn memberships_shelf_search_attach_patch_reprice_delete() {
    let Some((landau, store)) = client() else {
        eprintln!("skipped: LND_API_URL/LND_APT_TOKEN/LND_STORE not set");
        return;
    };
    let memberships = landau.memberships().await.expect("memberships");
    let mine = memberships.iter().find(|m| m.stores.iter().any(|s| s.id == store)).expect("the store belongs to the key's business");
    assert!(mine.approved);

    let before = landau.shelf(&store).await.expect("shelf");

    let page = landau.registry_search("سیبک فرمان پراید", 100, 0).await.expect("registry search");
    assert!(page.total >= 1 && !page.items.is_empty(), "the registry must be seeded (see docs/RUNBOOK.md)");
    assert!(page.items.iter().any(|c| c.name.contains("سیبک")));
    let broad = landau.registry_search("پراید", 1, 0).await.expect("registry search");
    assert!(broad.total > page.total, "a single generic token matches far more rows than a phrase");
    // pick a product this store does not already hold as NEW|FAKE
    let held: std::collections::HashSet<_> = before
        .iter()
        .filter(|r| r.authenticity.as_deref() == Some("FAKE"))
        .map(|r| r.product.clone())
        .collect();
    let product = page.items.iter().find(|h| !held.contains(&h.id)).expect("an unstocked hit");

    let row = landau
        .attach(&AttachBody { product: product.id.clone(), store: store.clone(), inventory: 1, cost: 150_000, authenticity: Authenticity::Fake })
        .await
        .expect("attach");
    assert_eq!(row.product, product.id);

    let missing = landau.await_visible(&store, &[row.id.clone()], Duration::from_secs(15)).await.expect("await");
    assert!(missing.is_empty(), "row not visible on the mirror within 15 s");

    let visible = landau.shelf(&store).await.unwrap().into_iter().find(|r| r.id == row.id).unwrap();
    assert_eq!(visible.cost, Some(150_000));
    assert_eq!(visible.price, Some(150_000), "price = cost before any rule runs");
    assert_ne!(visible.price_source.as_deref(), Some("MANUAL"));

    landau.patch(&row.id, &PatchBody { inventory: Some(4), cost: Some(160_000) }).await.expect("patch");
    let _rules = landau.pricing_rules().await.expect("rules");
    let repriced = landau.reprice(&store, false).await.expect("reprice");
    assert!(repriced.iter().any(|r| r.stock == row.id), "reprice pages the whole store");

    landau.delete(&row.id).await.expect("delete");
}

#[tokio::test]
async fn a_bad_key_is_unauthorized() {
    let Some((_, _)) = client() else {
        eprintln!("skipped");
        return;
    };
    let url = std::env::var("LND_API_URL").unwrap();
    let bad = LandauClient::new(&url, "apt-not-a-real-key").unwrap();
    assert_eq!(bad.memberships().await.unwrap_err().kind(), "LandauUnauthorized");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test landau::`
Expected: compile errors.

- [ ] **Step 3: Implement `landau/mod.rs`**

```rust
//! Landau client gateway calls (spec §2.1, §5). Every reply is read through [`peel`] so a
//! `{data: …}` envelope and a flat body are the same thing to the rest of the app.

use std::time::Duration;

use reqwest::{Client as Http, Method, StatusCode, Url};
use serde::Serialize;
use serde_json::{json, Value};

use crate::domain::Authenticity;
use crate::errors::{BridgeError, Result};

const SHELF_PAGE: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Store {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Membership {
    pub business: String,
    pub name: Option<String>,
    pub principal: String,
    pub approved: bool,
    pub stores: Vec<Store>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShelfRow {
    pub id: String,
    pub product: String,
    pub product_name: Option<String>,
    pub store: Option<String>,
    pub inventory: i64,
    pub price: Option<i64>,
    pub cost: Option<i64>,
    pub condition: Option<String>,
    pub authenticity: Option<String>,
    /// `MANUAL` | `RULE` | absent — from `priced.source`.
    pub price_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub brand: Option<String>,
    /// Kept for a search that ranks; the registry picker ranks nothing, so it is `0.0` there.
    pub score: f64,
}

/// One page of the seller's picker (`GET /career/stocks/registry`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegistryPage {
    pub total: usize,
    pub items: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PricingRule {
    pub id: String,
    pub active: bool,
    pub scope: Option<String>,
    pub basis: Option<String>,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepricedRow {
    pub stock: String,
    pub product: String,
    pub price: Option<i64>,
    pub next: Option<i64>,
    pub cost: Option<i64>,
    pub rule: Option<String>,
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachBody {
    pub product: String,
    pub store: String,
    pub inventory: i64,
    pub cost: i64,
    pub authenticity: Authenticity,
}

impl AttachBody {
    /// `price = cost` is what Landau's own delivery writes before rules run (spec §2.1).
    pub fn to_json(&self) -> Value {
        json!({
            "product": self.product,
            "store": self.store,
            "type": "NORMAL",
            "inventory": self.inventory,
            "price": self.cost,
            "cost": self.cost,
            "condition": "NEW",
            "authenticity": self.authenticity.as_str(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PatchBody {
    pub inventory: Option<i64>,
    pub cost: Option<i64>,
}

impl PatchBody {
    /// Never `price` — a PATCHed price is stamped MANUAL and Landau's rules skip the row for good.
    pub fn to_json(&self) -> Value {
        let mut v = serde_json::Map::new();
        if let Some(i) = self.inventory {
            v.insert("inventory".into(), json!(i));
        }
        if let Some(c) = self.cost {
            v.insert("cost".into(), json!(c));
        }
        Value::Object(v)
    }
}

pub fn peel(v: &Value) -> &Value {
    match v.get("data") {
        Some(inner) if inner.is_object() || inner.is_array() => inner,
        _ => v,
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}

fn n(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_f64()).map(|f| f.round() as i64)
}

impl ShelfRow {
    pub fn from_item(item: &Value) -> Option<ShelfRow> {
        let stock = item.get("stock").unwrap_or(item);
        Some(ShelfRow {
            id: s(stock, "id")?,
            product: s(stock, "product")?,
            product_name: item.get("product").and_then(|p| s(p, "name")),
            store: s(stock, "store"),
            inventory: n(stock, "inventory").unwrap_or(0),
            price: n(stock, "price"),
            cost: n(stock, "cost"),
            condition: s(stock, "condition"),
            authenticity: s(stock, "authenticity"),
            price_source: stock.get("priced").and_then(|p| s(p, "source")),
        })
    }
}

pub struct LandauClient {
    http: Http,
    base: Url,
    token: String,
}

impl LandauClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let http = Http::builder()
            .no_proxy()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| BridgeError::LandauUnreachable(e.to_string()))?;
        let base = Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
            .map_err(|e| BridgeError::SettingsIncomplete(format!("نشانی لاندا نامعتبر است: {e}")))?;
        Ok(LandauClient { http, base, token: token.to_string() })
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base.join(path.trim_start_matches('/')).map_err(|e| BridgeError::Other(format!("url {path}: {e}")))
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = self.url(path)?;
        self.call_url(method, url, body).await
    }

    async fn call_url(&self, method: Method, url: Url, body: Option<&Value>) -> Result<Value> {
        let path = url.path().to_string();
        let mut req = self.http.request(method.clone(), url).bearer_auth(&self.token);
        if let Some(b) = body {
            req = req.json(b);
        }
        let res = req.send().await.map_err(|e| BridgeError::LandauUnreachable(e.without_url().to_string()))?;
        let status = res.status();
        let text = res.text().await.map_err(|e| BridgeError::LandauUnreachable(e.to_string()))?;
        tracing::debug!(%method, path, %status, "landau");
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.chars().take(300).collect()));
        if status.is_success() {
            return Ok(value);
        }
        let message = value.get("message").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| value.to_string());
        Err(match status {
            StatusCode::UNAUTHORIZED => BridgeError::LandauUnauthorized,
            StatusCode::FORBIDDEN => BridgeError::LandauForbidden(message),
            StatusCode::CONFLICT => BridgeError::DuplicateRow,
            _ => BridgeError::LandauRejected { status: status.as_u16(), body: message.chars().take(300).collect() },
        })
    }

    /// `GET /career/staff/memberships` — what the key's owner acts for.
    pub async fn memberships(&self) -> Result<Vec<Membership>> {
        let v = self.call(Method::GET, "/career/staff/memberships", None).await?;
        let items = peel(&v).get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|m| {
                Some(Membership {
                    business: s(m, "business")?,
                    name: s(m, "name"),
                    principal: s(m, "principal").unwrap_or_default(),
                    approved: m.get("approved").and_then(|a| a.as_bool()).unwrap_or(false),
                    stores: m
                        .get("stores")
                        .and_then(|st| st.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|st| {
                                    Some(Store { id: s(st, "id")?, name: s(st, "name"), kind: s(st, "type"), state: s(st, "state"), status: s(st, "status") })
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect())
    }

    /// `GET /career/stocks/shelf?store=&limit=100&skip=` paged to `total`.
    pub async fn shelf(&self, store: &str) -> Result<Vec<ShelfRow>> {
        let mut rows = Vec::new();
        let mut skip = 0usize;
        loop {
            let path = format!("/career/stocks/shelf?store={store}&limit={SHELF_PAGE}&skip={skip}");
            let v = self.call(Method::GET, &path, None).await?;
            let page = peel(&v);
            let items = page.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
            let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
            rows.extend(items.iter().filter_map(ShelfRow::from_item));
            skip += SHELF_PAGE;
            if items.is_empty() || skip >= total {
                break;
            }
        }
        Ok(rows)
    }

    /// The seller's picker (spec §13): a case-insensitive substring match over `name`/`alias`/
    /// `brand` of every APPROVED definition, alphabetical, `limit` ≤ 100, with `total`.
    pub async fn registry_search(&self, term: &str, limit: usize, skip: usize) -> Result<RegistryPage> {
        let mut url = self.url("career/stocks/registry")?;
        url.query_pairs_mut()
            .append_pair("term", term)
            .append_pair("limit", &limit.clamp(1, 100).to_string())
            .append_pair("skip", &skip.to_string());
        let v = self.call_url(Method::GET, url, None).await?;
        let page = peel(&v);
        let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
        let items = page.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
        Ok(RegistryPage {
            total,
            items: items
                .iter()
                .filter_map(|it| {
                    let p = it.get("product").unwrap_or(it);
                    Some(Candidate { id: s(p, "id")?, name: s(p, "name")?, alias: s(p, "alias"), brand: s(p, "brand"), score: 0.0 })
                })
                .collect(),
        })
    }

    /// `POST /career/stocks` — reply is the row, flat or enveloped.
    pub async fn attach(&self, body: &AttachBody) -> Result<ShelfRow> {
        let v = self.call(Method::POST, "/career/stocks", Some(&body.to_json())).await?;
        let row = peel(&v);
        ShelfRow::from_item(row).ok_or_else(|| BridgeError::LandauRejected { status: 201, body: "reply without id".into() })
    }

    pub async fn patch(&self, id: &str, body: &PatchBody) -> Result<()> {
        self.call(Method::PATCH, &format!("/career/stocks/{id}"), Some(&body.to_json())).await.map(|_| ())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/career/stocks/{id}"), None).await.map(|_| ())
    }

    pub async fn pricing_rules(&self) -> Result<Vec<PricingRule>> {
        let v = self.call(Method::GET, "/career/pricing-rules", None).await?;
        let items = peel(&v).get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|r| {
                let props = r.get("props").unwrap_or(r);
                Some(PricingRule {
                    id: s(r, "id")?,
                    active: props.get("active").or_else(|| r.get("active")).and_then(|a| a.as_bool()).unwrap_or(false),
                    scope: s(props, "scope").or_else(|| s(r, "scope")),
                    basis: s(props, "basis").or_else(|| s(r, "basis")),
                    value: props.get("value").or_else(|| r.get("value")).and_then(|x| x.as_f64()),
                })
            })
            .collect())
    }

    /// `POST /career/pricing-rules/reprice {store, commit: true, overwrite, skip}` paged to total.
    pub async fn reprice(&self, store: &str, overwrite: bool) -> Result<Vec<RepricedRow>> {
        let mut rows = Vec::new();
        let mut skip = 0u64;
        loop {
            let body = json!({"store": store, "commit": true, "overwrite": overwrite, "skip": skip});
            let v = self.call(Method::POST, "/career/pricing-rules/reprice", Some(&body)).await?;
            let page = peel(&v);
            let items = page.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_default();
            let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            let limit = page.get("limit").and_then(|t| t.as_u64()).unwrap_or(100).max(1);
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
            skip += limit;
            if items.is_empty() || skip >= total {
                break;
            }
        }
        Ok(rows)
    }

    /// Poll the shelf until every id is visible. Returns the ids still missing at `timeout`.
    pub async fn await_visible(&self, store: &str, ids: &[String], timeout: Duration) -> Result<Vec<String>> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut missing: Vec<String> = ids.to_vec();
        loop {
            let seen: std::collections::HashSet<String> = self.shelf(store).await?.into_iter().map(|r| r.id).collect();
            missing.retain(|id| !seen.contains(id));
            if missing.is_empty() || tokio::time::Instant::now() >= deadline {
                return Ok(missing);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
```

- [ ] **Step 4: Run the unit tests and clippy**

Run: `cd src-tauri && cargo test landau:: && cargo clippy --all-targets --features live -- -D warnings`
Expected: 4 tests pass, clippy clean.

- [ ] **Step 5: Run the live test** (needs the staged fixtures and a minted key — see Task 19's runbook; do it now if the stack is up)

```bash
export LND_API_URL=http://localhost:6150 LND_APT_TOKEN=apt-… LND_STORE=…   # from scripts/mint-apt.mjs (Task 19) or the Landau app's Account → API keys
export NO_PROXY='*'
cd src-tauri && cargo test --features live --test live_landau -- --nocapture
```
Expected: both tests pass. If `search` answers no hits, the registry has not been imported after the last database reset — the runbook's *Registry* step covers it. If `shelf?store=` is ignored by the gateway (rows from other stores appear), filter client-side by `row.store == store` inside `shelf()` and keep going.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/landau src-tauri/tests/live_landau.rs
git commit -m "Add the Landau gateway client

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 9: Matching — scoring and persisted decisions

Implements spec §6 (candidates, threshold, decisions). `decide` takes the search as a closure so it is unit-tested without Landau.

**Files:**
- Modify: `src-tauri/src/matching/mod.rs`

**Interfaces:**
- Consumes: `landau::Candidate`, `db::Db` (`match_for`, `set_match`), `normalize::search_term`, `similarity::token_set_ratio`.
- Produces: `matching::{Scored, Decision, Matcher, queries, candidates, RegistrySearch}`:
  `score(term, &[Candidate]) -> Vec<Scored>`; `queries(term) -> Vec<String>` (whole term, adjacent bigrams, single tokens ≥ 2 chars, deduped, most specific first); `candidates(search, term) -> Result<Vec<Candidate>>` where `search: Fn(String, usize, usize) -> Fut<Output = Result<RegistryPage>>` is the picker `(query, limit, skip)`; `Matcher::new(db: Db, supplier: &str, threshold: u8)`; `Matcher::decide(&self, search, code, name) -> Result<Decision>` (same closure shape); `Matcher::record_user_pick(code, name_seen, product_id, product_name, similarity)`; `Matcher::record_none(code, name_seen)`; `Matcher::clear(code)`. Spec §13 (option A): the registry picker is a substring search, so gathering candidates is the bridge's job and the fuzziness lives in `score`.

- [ ] **Step 1: Write the failing tests** (bottom of `matching/mod.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::landau::{Candidate, RegistryPage};
    use std::collections::HashMap;

    fn cand(id: &str, name: &str, alias: Option<&str>, score: f64) -> Candidate {
        Candidate { id: id.into(), name: name.into(), alias: alias.map(str::to_string), brand: None, score }
    }

    /// A picker stub: query → (total, rows). Unknown queries answer nothing.
    fn picker(table: HashMap<&'static str, (usize, Vec<Candidate>)>) -> impl Fn(String, usize, usize) -> std::future::Ready<Result<RegistryPage>> {
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
            cand("b", "لنت ترمز جلو پراید", Some("لنت جلو پراید | لنت پراید"), 8.0),
            cand("c", "چیز دیگر", Some("لنت جلو پراید ABS"), 7.0),
        ];
        let scored = score("لنت جلو پراید", &hits);
        assert_eq!(scored[0].candidate.id, "b");
        assert_eq!(scored[0].similarity, 100);
        assert!(scored[1].similarity >= 80);
        assert_eq!(scored[2].candidate.id, "a");
    }

    #[test]
    fn queries_go_from_the_whole_term_to_bigrams_to_tokens() {
        assert_eq!(
            queries("سیبک فرمان پراید امیرنیا"),
            vec!["سیبک فرمان پراید امیرنیا", "سیبک فرمان", "فرمان پراید", "پراید امیرنیا", "سیبک", "فرمان", "پراید", "امیرنیا"]
        );
        assert_eq!(queries("لنت"), vec!["لنت"]);
        assert_eq!(queries("  a  لنت "), vec!["a لنت", "لنت"], "one-character tokens are dropped");
    }

    #[tokio::test]
    async fn candidates_union_specific_queries_and_skip_broad_tokens() {
        let table = HashMap::from([
            ("سیبک فرمان پراید", (2, vec![cand("p1", "سیبک فرمان پراید", None, 0.0), cand("p2", "سیبک فرمان پراید امیرنیا", None, 0.0)])),
            ("سیبک فرمان", (3, vec![cand("p1", "سیبک فرمان پراید", None, 0.0), cand("p3", "سیبک فرمان پژو 405", None, 0.0), cand("p2", "سیبک فرمان پراید امیرنیا", None, 0.0)])),
            ("فرمان پراید", (1, vec![cand("p1", "سیبک فرمان پراید", None, 0.0)])),
            ("سیبک", (40, vec![cand("p9", "سیبک بغل شاسی", None, 0.0)])),
            ("پراید", (5297, vec![cand("p8", "آبگیر پراید", None, 0.0)])), // broader than BROAD_TOKEN → skipped
        ]);
        let found = candidates(&picker(table), "سیبک فرمان پراید").await.unwrap();
        let ids: Vec<&str> = found.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["p1", "p2", "p3", "p9"], "unioned in query order, deduped, broad token skipped");
    }

    #[tokio::test]
    async fn candidates_fall_back_to_the_least_broad_token_when_nothing_else_matches() {
        let table = HashMap::from([
            ("پراید", (5297, vec![cand("p8", "آبگیر پراید", None, 0.0)])),
            ("405", (5317, vec![cand("p7", "آبگیر 405", None, 0.0)])),
        ]);
        let found = candidates(&picker(table), "پراید 405").await.unwrap();
        assert_eq!(found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["p8"], "the less broad token's first page");
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
            let items = if q == "لنت جلو پراید" { vec![cand("p1", "لنت جلو پراید", None, 0.0), cand("p2", "شمع", None, 0.0)] } else { Vec::new() };
            let total = items.len();
            std::future::ready(Ok(RegistryPage { total, items }))
        };
        let d = m.decide(&search, "111", "لنت جلو پرايد (4 عددي)").await.unwrap();
        assert!(matches!(d, Decision::Matched { ref product_id, similarity: Some(100), ref how, .. } if product_id == "p1" && how == "AUTO"));
        let first = calls.load(std::sync::atomic::Ordering::SeqCst);
        assert!(first >= 1);
        // second call answers from the table, no search
        let d2 = m.decide(&search, "111", "anything").await.unwrap();
        assert!(matches!(d2, Decision::Matched { .. }));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), first);
    }

    #[tokio::test]
    async fn below_threshold_goes_to_review_and_none_sticks() {
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db.clone(), "barezpakhsh", 95);
        let search = |_: String, _: usize, _: usize| std::future::ready(Ok(RegistryPage { total: 1, items: vec![cand("p1", "لنت جلو پژو 405", None, 0.0)] }));
        let d = m.decide(&search, "222", "لنت جلو پراید").await.unwrap();
        let Decision::Review(scored) = d else { panic!("expected review") };
        assert_eq!(scored[0].candidate.id, "p1");
        assert!(scored[0].similarity < 95);

        m.record_none("222", "لنت جلو پراید").unwrap();
        assert!(matches!(m.decide(&search, "222", "x").await.unwrap(), Decision::None));

        m.record_user_pick("222", "لنت جلو پراید", "p9", "picked", Some(70)).unwrap();
        assert!(matches!(m.decide(&search, "222", "x").await.unwrap(), Decision::Matched { ref how, .. } if how == "USER"));

        m.clear("222").unwrap();
        assert!(matches!(m.decide(&search, "222", "لنت جلو پراید").await.unwrap(), Decision::Review(_)));
    }

    #[tokio::test]
    async fn no_hits_is_a_review_with_no_candidates() {
        let db = Db::in_memory().unwrap();
        let m = Matcher::new(db, "barezpakhsh", 80);
        let search = |_: String, _: usize, _: usize| std::future::ready(Ok(RegistryPage { total: 0, items: Vec::new() }));
        assert!(matches!(m.decide(&search, "3", "چیزی").await.unwrap(), Decision::Review(ref v) if v.is_empty()));
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test matching::tests`
Expected: compile errors.

- [ ] **Step 3: Implement**

Replace `src-tauri/src/matching/mod.rs` with:
```rust
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
use similarity::token_set_ratio;

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
    Matched { product_id: String, product_name: String, similarity: Option<i64>, how: String },
    /// A stored «هیچ‌کدام»: the part is not in Landau; a support ticket is the way forward.
    None,
    /// Needs the seller. Candidates best first; may be empty.
    Review(Vec<Scored>),
}

/// Best of the candidate's name and each `" | "`-separated alias.
pub fn similarity_of(term: &str, c: &Candidate) -> u8 {
    let mut best = token_set_ratio(term, &c.name);
    if let Some(alias) = &c.alias {
        for a in alias.split('|').map(str::trim).filter(|a| !a.is_empty()) {
            best = best.max(token_set_ratio(term, a));
        }
    }
    best
}

pub fn score(term: &str, candidates: &[Candidate]) -> Vec<Scored> {
    let mut scored: Vec<Scored> = candidates.iter().map(|c| Scored { candidate: c.clone(), similarity: similarity_of(term, c) }).collect();
    scored.sort_by(|a, b| b.similarity.cmp(&a.similarity).then(b.candidate.score.total_cmp(&a.candidate.score)));
    scored
}

/// The picker queries for a term, most specific first: the whole term, each adjacent bigram, then
/// each single token of at least two characters. Deduped, order kept.
pub fn queries(term: &str) -> Vec<String> {
    let cleaned = key_text(term);
    let tokens: Vec<&str> = cleaned.split(' ').filter(|t| t.chars().count() >= 2).collect();
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
    for t in &tokens {
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
            if fallback.as_ref().map(|(t, _)| first.total < *t).unwrap_or(true) {
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
        Matcher { db, supplier: supplier.into(), threshold }
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
        if let Some(top) = scored.first() {
            if top.similarity >= self.threshold {
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
        Ok(Decision::Review(scored.into_iter().take(CANDIDATES).collect()))
    }

    pub fn record_user_pick(&self, code: &str, name_seen: &str, product_id: &str, product_name: &str, similarity: Option<i64>) -> Result<()> {
        self.db.set_match(&self.supplier, code, Some((product_id, product_name)), "USER", similarity, name_seen)
    }

    pub fn record_none(&self, code: &str, name_seen: &str) -> Result<()> {
        self.db.set_match(&self.supplier, code, None, "NONE", None, name_seen)
    }

    pub fn clear(&self, code: &str) -> Result<()> {
        self.db.clear_match(&self.supplier, code)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test matching::`
Expected: all matching tests pass (6 from Task 5 + 7 here).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/matching/mod.rs
git commit -m "Add candidate scoring and persisted match decisions

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 10: Threshold calibration binary and its record

Implements spec §6 (calibration) — "choose it by estimating and testing against real data, and record the threshold and the reasoning". The binary uses the app's own `search_term`, `score` and `LandauClient`, so the measured threshold is the app's.

**Files:**
- Create: `src-tauri/src/bin/calibrate.rs` (replace the stub), `docs/matching-threshold.md` (generated, then committed)
- Modify: `src-tauri/Cargo.toml` (add `csv = "1"` under `[dependencies]`), `src-tauri/src/settings.rs` (`DEFAULT_THRESHOLD` ← the chosen value)

**Interfaces:**
- Consumes: `matching::{score, normalize::{search_term, key_text}}`, `landau::LandauClient`.
- Produces: `docs/matching-threshold.md`; the `DEFAULT_THRESHOLD` constant.

- [ ] **Step 1: Write the calibration binary**

```rust
//! Measures auto-accept precision/coverage per threshold against the SKU → registry-name map the
//! registry was imported from, and writes docs/matching-threshold.md. Feature `live`.
//!
//! Env: LND_API_URL, LND_APT_TOKEN (required); PARTS_MAP (default: the landau-parts export);
//! SAMPLE (default 2000); SEED (default 42); OUT (default docs/matching-threshold.md).

use std::collections::BTreeMap;
use std::io::Write;

use landau_bridge_lib::landau::LandauClient;
use landau_bridge_lib::matching::normalize::{key_text, search_term};
use landau_bridge_lib::matching::{candidates, score};

#[derive(Debug)]
struct Trial {
    code: String,
    raw_name: String,
    expected: String,
    top_name: Option<String>,
    similarity: u8,
    correct: bool,
}

/// Deterministic sample without an RNG crate: a linear congruential walk over the row index.
fn sample_indices(len: usize, n: usize, seed: u64) -> Vec<usize> {
    let mut out = Vec::with_capacity(n.min(len));
    let mut x = seed;
    let mut seen = std::collections::HashSet::new();
    while out.len() < n.min(len) {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let i = (x >> 33) as usize % len;
        if seen.insert(i) {
            out.push(i);
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LND_API_URL")?;
    let token = std::env::var("LND_APT_TOKEN")?;
    let map = std::env::var("PARTS_MAP").unwrap_or_else(|_| "/home/vahid/WorkSpace/landau-parts/output/parts_map.csv".into());
    let sample: usize = std::env::var("SAMPLE").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let seed: u64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(42);
    let out = std::env::var("OUT").unwrap_or_else(|_| "../docs/matching-threshold.md".into());

    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_path(&map)?;
    let headers = reader.headers()?.clone();
    let col = |name: &str| headers.iter().position(|h| h.trim_start_matches('\u{feff}') == name).unwrap_or_else(|| panic!("column {name}"));
    let (c_code, c_raw, c_state, c_product) = (col("code"), col("raw_name"), col("state"), col("product_name"));
    let rows: Vec<(String, String, String)> = reader
        .records()
        .filter_map(|r| r.ok())
        .filter(|r| r.get(c_state) == Some("done") && !r.get(c_product).unwrap_or("").is_empty())
        .map(|r| (r[c_code].to_string(), r[c_raw].to_string(), r[c_product].to_string()))
        .collect();
    eprintln!("{} usable rows in {map}", rows.len());

    let landau = LandauClient::new(&url, &token)?;
    let mut trials = Vec::new();
    let mut empty_streak = 0;
    for (i, idx) in sample_indices(rows.len(), sample, seed).into_iter().enumerate() {
        let (code, raw, expected) = &rows[idx];
        let term = search_term(raw, code);
        let search = |q: String, limit: usize, skip: usize| async move { landau.registry_search(&q, limit, skip).await };
        let hits = candidates(&search, &term).await?;
        empty_streak = if hits.is_empty() { empty_streak + 1 } else { 0 };
        if i < 20 && empty_streak == 20 {
            panic!("the first 20 searches answered nothing — is the registry imported? (docs/RUNBOOK.md → Registry)");
        }
        let scored = score(&term, &hits);
        let top = scored.first();
        let top_name = top.map(|t| t.candidate.name.clone());
        let correct = top_name.as_deref().map(|n| key_text(n) == key_text(expected)).unwrap_or(false);
        trials.push(Trial { code: code.clone(), raw_name: raw.clone(), expected: expected.clone(), top_name, similarity: top.map(|t| t.similarity).unwrap_or(0), correct });
        if (i + 1) % 100 == 0 {
            eprintln!("{}/{sample}", i + 1);
        }
    }

    // precision/coverage per threshold
    let mut table: BTreeMap<u8, (usize, usize)> = BTreeMap::new(); // t -> (accepted, correct)
    for t in (50..=100).step_by(5) {
        let accepted: Vec<&Trial> = trials.iter().filter(|x| x.similarity >= t).collect();
        let correct = accepted.iter().filter(|x| x.correct).count();
        table.insert(t, (accepted.len(), correct));
    }
    let chosen = table
        .iter()
        .filter(|(_, (a, c))| *a > 0 && (*c as f64) / (*a as f64) >= 0.98)
        .map(|(t, _)| *t)
        .min()
        .unwrap_or(100);
    let top1_correct = trials.iter().filter(|x| x.correct).count();

    let mut f = std::fs::File::create(&out)?;
    writeln!(f, "# Matching threshold — calibration record\n")?;
    writeln!(f, "Generated by `cargo run --bin calibrate --features live` on {} against `{map}`.", chrono::Utc::now().format("%Y-%m-%d"))?;
    writeln!(f, "Sample: {} SKUs (seed {seed}) of {} usable rows; candidates = the seller's registry picker (`GET /career/stocks/registry?term=`, substring over name/alias/brand) queried with the whole normalised name, its bigrams and its tokens (single tokens matching > 300 rows skipped); similarity = token-set ratio of the normalised invoice name against each candidate's name and aliases.\n", trials.len(), rows.len())?;
    writeln!(f, "Top-1 hit names the expected registry part for **{top1_correct} / {}** samples regardless of threshold.\n", trials.len())?;
    writeln!(f, "| threshold | auto-accepted | correct | precision | coverage |")?;
    writeln!(f, "| --- | --- | --- | --- | --- |")?;
    for (t, (a, c)) in &table {
        let p = if *a == 0 { 0.0 } else { *c as f64 / *a as f64 * 100.0 };
        writeln!(f, "| {t} | {a} | {c} | {p:.1}% | {:.1}% |", *a as f64 / trials.len() as f64 * 100.0)?;
    }
    writeln!(f, "\n## Decision\n")?;
    writeln!(f, "**Threshold = {chosen}** — the lowest threshold whose auto-accept precision is ≥ 98 % (a wrong auto-match puts a seller's stock under the wrong part with no one looking; a missed match only costs one click in review). Set as `DEFAULT_THRESHOLD` in `src-tauri/src/settings.rs`; adjustable per seller in تنظیمات.\n")?;
    writeln!(f, "## Misses at the chosen threshold (first 30)\n")?;
    writeln!(f, "| code | invoice name | expected | top hit | similarity |")?;
    writeln!(f, "| --- | --- | --- | --- | --- |")?;
    for x in trials.iter().filter(|x| x.similarity >= chosen && !x.correct).take(30) {
        writeln!(f, "| {} | {} | {} | {} | {} |", x.code, x.raw_name, x.expected, x.top_name.clone().unwrap_or_default(), x.similarity)?;
    }
    println!("threshold={chosen} written to {out}");
    Ok(())
}
```

- [ ] **Step 2: Build it**

Run: `cd src-tauri && cargo build --bin calibrate --features live`
Expected: builds. (`csv` and `chrono` are dependencies; `chrono` already is.)

- [ ] **Step 3: Run it against the live registry**

Precondition: the Landau registry is imported (runbook → *Registry*) and `LND_API_URL`/`LND_APT_TOKEN` are exported (Task 8 step 5).

Run: `cd src-tauri && NO_PROXY='*' cargo run --bin calibrate --features live`
Expected: progress lines, then `threshold=NN written to ../docs/matching-threshold.md`. Takes a few minutes for 2 000 searches.

- [ ] **Step 4: Adopt the value**

Edit `src-tauri/src/settings.rs`: `pub const DEFAULT_THRESHOLD: u8 = NN;` (the printed value) and update its doc comment to `/// Calibrated 2026-09-13 — see docs/matching-threshold.md.` Read the misses table in the generated file; if a systematic miss family is visible (e.g. brand words dominating), note it under a `## Observations` heading in the same file by hand — the reasoning must be recorded, not just the number.

Run: `cargo test` — green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/calibrate.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/settings.rs docs/matching-threshold.md
git commit -m "Calibrate the matching threshold against the registry import map

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 11: Job engine — planner, import job, refresh job

Implements spec §7 (both jobs and the invariants). The planner is pure and unit-tested; the runners are exercised live in Task 19.

**Files:**
- Create: `src-tauri/src/jobs/mod.rs`, `src-tauri/src/jobs/planner.rs`, `src-tauri/src/jobs/import.rs`, `src-tauri/src/jobs/refresh.rs`

**Interfaces:**
- Consumes: `db::*`, `landau::*`, `supplier::*`, `matching::*`, `settings::Settings`, `domain::*`.
- Produces:
  - `planner::{AggregatedItem, Aggregation, aggregate(lines, mode, since, already) -> Aggregation, WriteAction, plan_write(existing, qty, mode) -> WriteAction, find_row(shelf, product_id, authenticity) -> Option<&ShelfRow>}`
  - `jobs::{JobCtx, JobRegistry, Services, Progress, emit_event}`; `JobRegistry::begin(job_id) -> Result<Arc<AtomicBool>>`, `finish(job_id)`, `cancel(job_id) -> bool`, `current() -> Option<i64>`
  - `import::{ImportParams, Outcome, start(&JobCtx, &Services, &ImportParams) -> Result<Outcome>, commit(&JobCtx, &Services) -> Result<()>}`
  - `refresh::run(&JobCtx, &Services) -> Result<()>`

- [ ] **Step 1: Write the planner tests** (bottom of `planner.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn line(invoice_id: i64, kind: InvoiceKind, paid: bool, date: &str, code: &str, name: &str, qty: i64) -> LineWithInvoice {
        LineWithInvoice {
            invoice_id,
            number: invoice_id.to_string(),
            kind,
            paid,
            date_jalali: date.into(),
            code: code.into(),
            name: name.into(),
            qty,
            unit: "عدد".into(),
            unit_price: 1000,
        }
    }

    #[test]
    fn quantity_mode_sums_paid_sales_minus_returns_since_the_date() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1401/10/05", "A", "old name", 5),
            line(2, InvoiceKind::Sale, true, "1403/01/10", "A", "new name", 3),
            line(3, InvoiceKind::Return, true, "1403/02/01", "A", "new name", 1),
            line(4, InvoiceKind::Sale, false, "1403/03/01", "A", "new name", 9),
            line(5, InvoiceKind::Sale, true, "1403/03/02", "B", "b", 0),
            line(6, InvoiceKind::Sale, true, "1403/03/03", "C", "c", 2),
        ];
        let already = HashSet::from([6]);
        let agg = aggregate(&lines, ImportMode::Quantity, Some("1402/01/01"), &already);
        let a = agg.items.iter().find(|i| i.code == "A").unwrap();
        assert_eq!(a.qty, 2, "3 bought minus 1 returned; the 1401 invoice is before the date");
        assert_eq!(a.name, "new name", "the most recent name wins");
        assert_eq!(a.invoice_ids, vec![2, 3]);
        assert!(agg.items.iter().all(|i| i.code != "C"), "invoice 6 was transferred before");
        assert_eq!(agg.skipped_unpaid, vec![4]);
        assert_eq!(agg.skipped_imported, vec![6]);
        assert_eq!(agg.non_positive_lines, 1);
        assert!(agg.items.iter().all(|i| i.code != "B"), "a zero-quantity line contributes nothing");
    }

    #[test]
    fn zero_mode_takes_every_paid_invoice_at_zero() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1401/10/05", "A", "a", 5),
            line(2, InvoiceKind::Sale, false, "1403/01/10", "B", "b", 3),
        ];
        let agg = aggregate(&lines, ImportMode::Zero, Some("1402/01/01"), &HashSet::from([1]));
        assert_eq!(agg.items.len(), 1);
        assert_eq!(agg.items[0].code, "A");
        assert_eq!(agg.items[0].qty, 0);
        assert!(agg.skipped_imported.is_empty(), "zero mode ignores the transfer guard");
    }

    #[test]
    fn net_negative_becomes_zero_and_is_flagged() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1403/01/01", "A", "a", 1),
            line(2, InvoiceKind::Return, true, "1403/01/02", "A", "a", 3),
        ];
        let agg = aggregate(&lines, ImportMode::Quantity, None, &HashSet::new());
        assert_eq!(agg.items[0].qty, 0);
        assert!(agg.items[0].negative);
    }

    fn row(id: &str, product: &str, auth: Option<&str>, inventory: i64) -> ShelfRow {
        ShelfRow {
            id: id.into(),
            product: product.into(),
            product_name: None,
            store: None,
            inventory,
            price: Some(1000),
            cost: None,
            condition: Some("NEW".into()),
            authenticity: auth.map(str::to_string),
            price_source: None,
        }
    }

    #[test]
    fn find_row_matches_the_whole_variant_tuple() {
        let shelf = vec![row("s1", "p1", Some("ORIGINAL"), 2), row("s2", "p1", Some("FAKE"), 0), row("s3", "p2", None, 1)];
        assert_eq!(find_row(&shelf, "p1", "FAKE").map(|r| r.id.as_str()), Some("s2"));
        assert_eq!(find_row(&shelf, "p1", "ORIGINAL").map(|r| r.id.as_str()), Some("s1"));
        assert!(find_row(&shelf, "p2", "ORIGINAL").is_none(), "a legacy row without a variant is not the same tuple");
    }

    #[test]
    fn plan_write_adds_quantities_and_never_touches_inventory_in_zero_mode() {
        let existing = row("s1", "p1", Some("ORIGINAL"), 2);
        assert_eq!(plan_write(Some(&existing), 3, ImportMode::Quantity), WriteAction::Patch { stock_id: "s1".into(), inventory: Some(5), qty_before: 2 });
        assert_eq!(plan_write(Some(&existing), 0, ImportMode::Zero), WriteAction::Patch { stock_id: "s1".into(), inventory: None, qty_before: 2 });
        assert_eq!(plan_write(None, 3, ImportMode::Quantity), WriteAction::Attach { inventory: 3 });
        assert_eq!(plan_write(None, 0, ImportMode::Zero), WriteAction::Attach { inventory: 0 });
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test jobs::planner`
Expected: compile errors.

- [ ] **Step 3: Implement `planner.rs`**

```rust
//! The pure half of the jobs (spec §7.1 Aggregate/Write, §7.3): what to add, what to write.

use std::collections::{BTreeMap, HashSet};

use crate::db::invoices::LineWithInvoice;
use crate::domain::{ImportMode, InvoiceKind};
use crate::landau::ShelfRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedItem {
    pub code: String,
    /// The most recent invoice's spelling — the search term is built from it.
    pub name: String,
    pub qty: i64,
    pub invoice_ids: Vec<i64>,
    /// Returns exceeded purchases; `qty` was clamped to 0.
    pub negative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aggregation {
    pub items: Vec<AggregatedItem>,
    pub skipped_unpaid: Vec<i64>,
    pub skipped_imported: Vec<i64>,
    pub non_positive_lines: usize,
}

/// Quantity mode: paid invoices dated on/after `since` and not transferred before, sales minus
/// returns. Zero mode: every paid invoice, quantity 0. Lines are expected oldest first.
pub fn aggregate(lines: &[LineWithInvoice], mode: ImportMode, since: Option<&str>, already: &HashSet<i64>) -> Aggregation {
    let mut agg = Aggregation::default();
    let mut items: BTreeMap<String, AggregatedItem> = BTreeMap::new();
    let mut unpaid: HashSet<i64> = HashSet::new();
    let mut imported: HashSet<i64> = HashSet::new();

    for l in lines {
        if !l.paid {
            unpaid.insert(l.invoice_id);
            continue;
        }
        if mode == ImportMode::Quantity {
            if let Some(since) = since {
                if l.date_jalali.as_str() < since {
                    continue;
                }
            }
            if already.contains(&l.invoice_id) {
                imported.insert(l.invoice_id);
                continue;
            }
        }
        if l.qty <= 0 {
            agg.non_positive_lines += 1;
            continue;
        }
        let signed = if l.kind == InvoiceKind::Return { -l.qty } else { l.qty };
        let item = items.entry(l.code.clone()).or_insert_with(|| AggregatedItem {
            code: l.code.clone(),
            name: l.name.clone(),
            qty: 0,
            invoice_ids: Vec::new(),
            negative: false,
        });
        item.qty += signed;
        item.name = l.name.clone();
        if !item.invoice_ids.contains(&l.invoice_id) {
            item.invoice_ids.push(l.invoice_id);
        }
    }

    agg.items = items
        .into_values()
        .map(|mut item| {
            if mode == ImportMode::Zero {
                item.qty = 0;
            } else if item.qty < 0 {
                item.negative = true;
                item.qty = 0;
            }
            item
        })
        .collect();
    agg.items.sort_by(|a, b| a.name.cmp(&b.name));
    agg.skipped_unpaid = unpaid.into_iter().collect();
    agg.skipped_unpaid.sort_unstable();
    agg.skipped_imported = imported.into_iter().collect();
    agg.skipped_imported.sort_unstable();
    agg
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteAction {
    Attach { inventory: i64 },
    Patch { stock_id: String, inventory: Option<i64>, qty_before: i64 },
}

/// The row for `(product, NEW, authenticity)` — the whole unique tuple, never a subset (spec §2.1).
pub fn find_row<'a>(shelf: &'a [ShelfRow], product_id: &str, authenticity: &str) -> Option<&'a ShelfRow> {
    shelf.iter().find(|r| {
        r.product == product_id && r.condition.as_deref() == Some("NEW") && r.authenticity.as_deref() == Some(authenticity)
    })
}

pub fn plan_write(existing: Option<&ShelfRow>, qty: i64, mode: ImportMode) -> WriteAction {
    match existing {
        Some(row) => WriteAction::Patch {
            stock_id: row.id.clone(),
            inventory: (mode == ImportMode::Quantity).then_some(row.inventory + qty),
            qty_before: row.inventory,
        },
        None => WriteAction::Attach { inventory: if mode == ImportMode::Quantity { qty } else { 0 } },
    }
}
```

- [ ] **Step 4: Implement `jobs/mod.rs`**

```rust
//! Job runners (spec §7). One job at a time; every step persisted; progress as Tauri events.

pub mod import;
pub mod planner;
pub mod refresh;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::db::jobs::ItemPatch;
use crate::db::Db;
use crate::domain::cost_of;
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{LandauClient, RepricedRow};
use crate::settings::{Settings, SUPPLIER_ID};
use crate::supplier::Supplier;

pub const EVENT_PROGRESS: &str = "job://progress";
pub const EVENT_EVENT: &str = "job://event";
pub const EVENT_DONE: &str = "job://done";
pub const MIRROR_WAIT: Duration = Duration::from_secs(15);

/// Short constructors for [`ItemPatch`] values.
pub fn t(s: &str) -> rusqlite::types::Value {
    rusqlite::types::Value::Text(s.to_string())
}
pub fn i(n: i64) -> rusqlite::types::Value {
    rusqlite::types::Value::Integer(n)
}
pub fn opt_i(n: Option<i64>) -> rusqlite::types::Value {
    n.map(rusqlite::types::Value::Integer).unwrap_or(rusqlite::types::Value::Null)
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub job_id: i64,
    pub phase: String,
    pub done: u32,
    pub total: u32,
    pub message: String,
}

/// Everything a job talks to. Built per job by the command layer.
pub struct Services {
    pub landau: LandauClient,
    pub supplier: Box<dyn Supplier>,
    pub settings: Settings,
}

pub fn emit_event<T: Serialize + Clone>(app: &Option<AppHandle>, name: &str, payload: T) {
    if let Some(app) = app {
        if let Err(e) = app.emit(name, payload) {
            tracing::warn!("emit {name}: {e}");
        }
    }
}

pub struct JobCtx {
    pub db: Db,
    pub app: Option<AppHandle>,
    pub job_id: i64,
    pub cancel: Arc<AtomicBool>,
}

impl JobCtx {
    pub fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(BridgeError::JobCancelled)
        } else {
            Ok(())
        }
    }

    pub fn phase(&self, phase: &str) -> Result<()> {
        self.db.set_job_phase(self.job_id, phase)?;
        self.progress(phase, 0, 0, "");
        Ok(())
    }

    pub fn progress(&self, phase: &str, done: u32, total: u32, message: &str) {
        emit_event(&self.app, EVENT_PROGRESS, Progress { job_id: self.job_id, phase: phase.into(), done, total, message: message.into() });
    }

    pub fn info(&self, phase: &str, message: &str, detail: Option<Value>) {
        tracing::info!(job = self.job_id, phase, "{message}");
        let _ = self.db.add_job_event(self.job_id, "INFO", phase, message, detail.clone());
        emit_event(&self.app, EVENT_EVENT, json!({"job_id": self.job_id, "level": "INFO", "phase": phase, "message": message, "detail": detail}));
    }

    /// A recorded warning: `errors` row + job event + UI event (spec §9).
    pub fn warn(&self, phase: &str, kind: &str, message: &str, detail: Option<Value>) {
        tracing::warn!(job = self.job_id, phase, kind, "{message}");
        let _ = self.db.record_error(Some(self.job_id), kind, message, detail.clone());
        let _ = self.db.add_job_event(self.job_id, "WARN", phase, message, detail.clone());
        emit_event(&self.app, EVENT_EVENT, json!({"job_id": self.job_id, "level": "WARN", "kind": kind, "phase": phase, "message": message, "detail": detail}));
    }

    pub fn error(&self, phase: &str, err: &BridgeError, detail: Option<Value>) {
        tracing::error!(job = self.job_id, phase, kind = err.kind(), "{err}");
        let _ = self.db.record_error(Some(self.job_id), err.kind(), &err.message_fa(), detail.clone());
        let _ = self.db.add_job_event(self.job_id, "ERROR", phase, &err.message_fa(), detail.clone());
        emit_event(&self.app, EVENT_EVENT, json!({"job_id": self.job_id, "level": "ERROR", "kind": err.kind(), "phase": phase, "message": err.message_fa(), "detail": detail}));
    }

    pub fn item(&self, item_id: i64, patch: &[ItemPatch<'_>]) -> Result<()> {
        self.db.update_job_item(item_id, patch)
    }

    /// Terminal bookkeeping shared by both jobs.
    pub fn finish(&self, outcome: &Result<()>, phase: &str) {
        let status = match outcome {
            Ok(()) => "DONE",
            Err(BridgeError::JobCancelled) => "CANCELLED",
            Err(e) => {
                self.error(phase, e, None);
                "FAILED"
            }
        };
        let _ = self.db.set_job_status(self.job_id, status, phase);
        emit_event(&self.app, EVENT_DONE, json!({"job_id": self.job_id, "status": status}));
    }
}

/// Which job is running, and its cancel flag. One at a time (spec §7.3).
#[derive(Default)]
pub struct JobRegistry {
    current: Mutex<Option<(i64, Arc<AtomicBool>)>>,
}

impl JobRegistry {
    pub fn begin(&self, job_id: i64) -> Result<Arc<AtomicBool>> {
        let mut cur = self.current.lock().map_err(|e| BridgeError::Other(e.to_string()))?;
        if let Some((running, _)) = cur.as_ref() {
            return Err(BridgeError::JobBusy(format!("#{running}")));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *cur = Some((job_id, flag.clone()));
        Ok(flag)
    }

    pub fn finish(&self, job_id: i64) {
        if let Ok(mut cur) = self.current.lock() {
            if matches!(cur.as_ref(), Some((id, _)) if *id == job_id) {
                *cur = None;
            }
        }
    }

    pub fn cancel(&self, job_id: i64) -> bool {
        match self.current.lock() {
            Ok(cur) => match cur.as_ref() {
                Some((id, flag)) if *id == job_id => {
                    flag.store(true, Ordering::SeqCst);
                    true
                }
                _ => false,
            },
            Err(_) => false,
        }
    }

    pub fn current(&self) -> Option<i64> {
        self.current.lock().ok().and_then(|c| c.as_ref().map(|(id, _)| *id))
    }
}

// ------------------------------------------------------------ shared phases

/// Preflight of both jobs: the key acts for the store's approved business; the supplier logs in;
/// zero active rules is a warning (spec §7.1).
pub async fn preflight(ctx: &JobCtx, svc: &Services, store: &str) -> Result<()> {
    ctx.phase("PREFLIGHT")?;
    let memberships = svc.landau.memberships().await?;
    let owner = memberships.iter().find(|m| m.stores.iter().any(|s| s.id == store)).ok_or(BridgeError::NoStore)?;
    if !owner.approved {
        return Err(BridgeError::BusinessNotApproved);
    }
    svc.supplier.login().await?;
    let rules = svc.landau.pricing_rules().await?;
    let active = rules.iter().filter(|r| r.active).count();
    if active == 0 {
        ctx.warn("PREFLIGHT", kinds::NO_PRICING_RULE, "در لاندا هیچ قاعده قیمت‌گذاری فعالی ندارید؛ قیمت قفسه برابر قیمت خرید خواهد بود.", None);
    }
    ctx.info("PREFLIGHT", &format!("business {} · {active} active pricing rule(s)", owner.business), None);
    Ok(())
}

/// Today's cost of one code; records the snapshot and a `VatRateMismatch` when the site's rate
/// differs from the setting (the setting wins — spec Global Constraints).
pub async fn cost_for(ctx: &JobCtx, svc: &Services, phase: &str, code: &str) -> Result<Option<i64>> {
    let Some(q) = svc.supplier.price_of(code).await? else {
        return Ok(None);
    };
    let setting = svc.settings.vat_percent;
    let applied = if q.has_vat { setting } else { 0.0 };
    if q.has_vat {
        if let Some(site) = q.vat_percent {
            if (site - setting).abs() > 0.01 {
                ctx.warn(phase, kinds::VAT_RATE_MISMATCH, &format!("نرخ ارزش افزوده سایت برای {code} {site}٪ است ولی تنظیمات {setting}٪ می‌گوید؛ تنظیمات اعمال شد."), Some(json!({"code": code, "site": site, "setting": setting})));
            }
        }
    }
    let cost = cost_of(q.list_price, applied);
    ctx.db.add_price_snapshot(SUPPLIER_ID, code, q.list_price, cost, q.vat_percent, q.available, &crate::db::now())?;
    Ok(Some(cost))
}

/// Wait for the mirror, run Landau's rules over the store, and stamp each item's verdict.
/// `items` = (item_id, stock_id).
pub async fn reprice_items(ctx: &JobCtx, svc: &Services, store: &str, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPRICE")?;
    let ids: Vec<String> = items.iter().map(|(_, s)| s.clone()).collect();
    if !ids.is_empty() {
        let missing = svc.landau.await_visible(store, &ids, MIRROR_WAIT).await?;
        if !missing.is_empty() {
            ctx.warn("REPRICE", kinds::MIRROR_TIMEOUT, &BridgeError::MirrorTimeout(missing.len()).message_fa(), Some(json!({"missing": missing})));
        }
    }
    let rows: Vec<RepricedRow> = svc.landau.reprice(store, svc.settings.overwrite_manual).await?;
    ctx.info("REPRICE", &format!("reprice answered {} row(s)", rows.len()), None);
    for (item_id, stock_id) in items {
        ctx.check_cancel()?;
        let verdict = rows.iter().find(|r| &r.stock == stock_id).map(|r| match (&r.next, &r.skipped) {
            (Some(next), _) => format!("REPRICED:{next}"),
            (None, Some(why)) => format!("SKIPPED:{why}"),
            (None, None) => "UNCHANGED".to_string(),
        });
        if let Some(v) = &verdict {
            if v.starts_with("SKIPPED:") && !v.ends_with("UNCHANGED") {
                ctx.warn("REPRICE", kinds::REPRICE_SKIPPED, &format!("ردیف {stock_id}: {v}"), Some(json!({"stock": stock_id})));
            }
        }
        ctx.item(*item_id, &[("reprice_verdict", verdict.as_deref().map(t).unwrap_or(rusqlite::types::Value::Null))])?;
    }
    Ok(())
}

/// Read the shelf once more and stamp final prices; mark items DONE.
pub async fn readback(ctx: &JobCtx, svc: &Services, store: &str, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPORT")?;
    let shelf = svc.landau.shelf(store).await?;
    for (item_id, stock_id) in items {
        let price = shelf.iter().find(|r| &r.id == stock_id).and_then(|r| r.price);
        ctx.item(*item_id, &[("price_after", opt_i(price)), ("status", t("DONE"))])?;
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `jobs/import.rs`**

```rust
//! Feature 1 (spec §7.1): preflight → fetch → aggregate → match → [REVIEW] → price → write →
//! reprice → report. `start` runs up to the review pause; `commit` runs the rest.

use std::collections::HashSet;

use serde_json::json;

use super::planner::{aggregate, find_row, plan_write, WriteAction};
use super::{cost_for, i, opt_i, preflight, readback, reprice_items, t, JobCtx, Services};
use crate::domain::{Authenticity, ImportMode};
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{AttachBody, PatchBody};
use crate::matching::{Decision, Matcher};
use crate::settings::SUPPLIER_ID;
use crate::supplier::Progress;

#[derive(Debug, Clone)]
pub struct ImportParams {
    pub mode: ImportMode,
    pub since: Option<String>,
    /// At most this many not-yet-fetched invoices this run (`None` = all). Invoices are cached, so
    /// a capped run simply continues next time; the live e2e test uses it to stay short.
    pub fetch_limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Paused: this many items need the seller.
    Review(usize),
    Done,
}

fn fatal(e: &BridgeError) -> bool {
    matches!(
        e,
        BridgeError::SupplierUnreachable(_) | BridgeError::SupplierBlocked | BridgeError::SupplierBadCredentials
            | BridgeError::SupplierSessionExpired | BridgeError::LandauUnauthorized | BridgeError::LandauUnreachable(_)
            | BridgeError::JobCancelled
    )
}

pub async fn start(ctx: &JobCtx, svc: &Services, params: &ImportParams) -> Result<Outcome> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;
    fetch(ctx, svc, params.fetch_limit).await?;
    aggregate_phase(ctx, params)?;
    let pending = match_phase(ctx, svc).await?;
    if pending > 0 {
        ctx.db.set_job_status(ctx.job_id, "REVIEW", "REVIEW")?;
        ctx.info("REVIEW", &format!("{pending} item(s) await the seller"), None);
        super::emit_event(&ctx.app, super::EVENT_DONE, json!({"job_id": ctx.job_id, "status": "REVIEW"}));
        return Ok(Outcome::Review(pending));
    }
    commit(ctx, svc).await?;
    Ok(Outcome::Done)
}

/// Ledger headers → unseen invoices' lines. Per-invoice parse failures are recorded and skipped;
/// session/transport failures stop the job.
async fn fetch(ctx: &JobCtx, svc: &Services, limit: Option<usize>) -> Result<()> {
    ctx.phase("FETCH")?;
    let progress_ctx = (ctx.app.clone(), ctx.job_id);
    let progress: Progress = std::sync::Arc::new(move |done, total| {
        super::emit_event(&progress_ctx.0, super::EVENT_PROGRESS, super::Progress { job_id: progress_ctx.1, phase: "FETCH".into(), done, total, message: "فهرست فاکتورها".into() });
    });
    let headers = svc.supplier.ledger(progress).await?;
    ctx.check_cancel()?;
    let mut unpaid = 0;
    for h in &headers {
        ctx.db.upsert_invoice(SUPPLIER_ID, h)?;
        if !h.paid {
            unpaid += 1;
        }
    }
    ctx.info("FETCH", &format!("{} ledger row(s), {unpaid} unpaid", headers.len()), None);
    let mut todo = ctx.db.unfetched_invoice_ids(SUPPLIER_ID)?;
    if let Some(limit) = limit {
        if todo.len() > limit {
            ctx.info("FETCH", &format!("fetch capped at {limit} of {} pending invoice(s)", todo.len()), None);
            todo.truncate(limit);
        }
    }
    let total = todo.len() as u32;
    for (n, invoice_id) in todo.into_iter().enumerate() {
        ctx.check_cancel()?;
        let url = ctx.db.invoice_url(invoice_id)?;
        match svc.supplier.invoice_lines(&url).await {
            Ok(lines) => ctx.db.replace_lines(invoice_id, &lines)?,
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => ctx.warn("FETCH", e.kind(), &format!("فاکتور {url}: {}", e.message_fa()), Some(json!({"invoice_id": invoice_id}))),
        }
        ctx.progress("FETCH", n as u32 + 1, total, "جزئیات فاکتورها");
    }
    Ok(())
}

fn aggregate_phase(ctx: &JobCtx, params: &ImportParams) -> Result<()> {
    ctx.phase("AGGREGATE")?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let already = ctx.db.imported_invoice_ids()?;
    let agg = aggregate(&lines, params.mode, params.since.as_deref(), &already);
    if !agg.skipped_unpaid.is_empty() {
        ctx.warn("AGGREGATE", kinds::INVOICE_UNPAID, &format!("{} فاکتور پرداخت‌نشده نادیده گرفته شد.", agg.skipped_unpaid.len()), Some(json!({"invoice_ids": agg.skipped_unpaid})));
    }
    if !agg.skipped_imported.is_empty() {
        ctx.warn("AGGREGATE", kinds::INVOICE_ALREADY_IMPORTED, &format!("{} فاکتور قبلاً منتقل شده بود و دوباره شمرده نشد.", agg.skipped_imported.len()), Some(json!({"invoice_ids": agg.skipped_imported})));
    }
    if agg.non_positive_lines > 0 {
        ctx.warn("AGGREGATE", kinds::LINE_NON_POSITIVE_QTY, &format!("{} ردیف با تعداد صفر یا منفی نادیده گرفته شد.", agg.non_positive_lines), None);
    }
    let mut invoice_ids: HashSet<i64> = HashSet::new();
    for item in &agg.items {
        let id = ctx.db.add_job_item(ctx.job_id, &item.code, &item.name, item.qty)?;
        if item.negative {
            ctx.warn("AGGREGATE", kinds::NEGATIVE_QUANTITY, &format!("«{}»: مرجوعی‌ها بیشتر از خرید بود؛ تعداد صفر منظور شد.", item.name), Some(json!({"item_id": id})));
        }
        invoice_ids.extend(item.invoice_ids.iter().copied());
    }
    let mut ids: Vec<i64> = invoice_ids.into_iter().collect();
    ids.sort_unstable();
    ctx.db.set_job_summary(ctx.job_id, &json!({"invoice_ids": ids, "items": agg.items.len()}))?;
    ctx.info("AGGREGATE", &format!("{} distinct part(s) from {} invoice(s)", agg.items.len(), ids.len()), None);
    Ok(())
}

/// Returns how many items need review.
async fn match_phase(ctx: &JobCtx, svc: &Services) -> Result<usize> {
    ctx.phase("MATCH")?;
    let matcher = Matcher::new(ctx.db.clone(), SUPPLIER_ID, svc.settings.threshold);
    let landau = &svc.landau;
    let search = |q: String, limit: usize, skip: usize| async move { landau.registry_search(&q, limit, skip).await };
    let items = ctx.db.job_items(ctx.job_id)?;
    let total = items.len() as u32;
    let mut pending = 0usize;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match matcher.decide(&search, &item.code, &item.name).await? {
            Decision::Matched { product_id, product_name, .. } => {
                ctx.item(item.id, &[("product_id", t(&product_id)), ("product_name", t(&product_name)), ("status", t("PLANNED"))])?;
            }
            Decision::None => {
                ctx.item(item.id, &[("status", t("SKIPPED")), ("error_kind", t(kinds::NONE_DECISION)), ("error_msg", t("این قطعه در لاندا نیست (تصمیم قبلی)."))])?;
            }
            Decision::Review(scored) => {
                pending += 1;
                ctx.item(item.id, &[("status", t("REVIEW")), ("candidates_json", t(&serde_json::to_string(&scored)?))])?;
            }
        }
        ctx.progress("MATCH", n as u32 + 1, total, "تطبیق با فهرست لاندا");
    }
    if pending > 0 {
        ctx.warn("MATCH", kinds::UNMATCHED, &format!("{pending} قطعه نیاز به تأیید شما دارد."), None);
    }
    Ok(pending)
}

/// Price → write → reprice → report over every item that is PLANNED with a product.
pub async fn commit(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    let job = ctx.db.job(ctx.job_id)?.ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let mode = job.mode.as_deref().and_then(ImportMode::parse).unwrap_or(ImportMode::Zero);
    ctx.db.set_job_status(ctx.job_id, "RUNNING", "PRICE")?;

    // deferred review items are left out of this run
    for item in ctx.db.job_items(ctx.job_id)?.iter().filter(|i| i.status == "REVIEW") {
        ctx.item(item.id, &[("status", t("SKIPPED")), ("error_kind", t(kinds::UNMATCHED)), ("error_msg", t("بدون تصمیم؛ در اجرای بعدی دوباره پرسیده می‌شود."))])?;
    }

    ctx.phase("PRICE")?;
    let items: Vec<_> = ctx.db.job_items(ctx.job_id)?.into_iter().filter(|i| i.status == "PLANNED" && i.product_id.is_some()).collect();
    let total = items.len() as u32;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match cost_for(ctx, svc, "PRICE", &item.code).await {
            Ok(Some(cost)) => ctx.item(item.id, &[("cost", i(cost))])?,
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice { code: item.code.clone() };
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(item.id, &[("status", t("SKIPPED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(item.id, &[("status", t("FAILED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }

    ctx.phase("WRITE")?;
    let mut shelf = svc.landau.shelf(&store).await?;
    let items: Vec<_> = ctx.db.job_items(ctx.job_id)?.into_iter().filter(|i| i.status == "PLANNED" && i.cost.is_some()).collect();
    let total = items.len() as u32;
    let mut written: Vec<(i64, String)> = Vec::new();
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        let product = item.product_id.clone().unwrap_or_default();
        let cost = item.cost.unwrap_or(0);
        let authenticity = item.authenticity.as_deref().and_then(Authenticity::parse).unwrap_or(svc.settings.default_authenticity);
        let existing = find_row(&shelf, &product, authenticity.as_str());
        let action = plan_write(existing, item.qty, mode);
        let result: Result<(String, Option<i64>, Option<i64>, Option<i64>, &str)> = match &action {
            WriteAction::Attach { inventory } => {
                match svc.landau.attach(&AttachBody { product: product.clone(), store: store.clone(), inventory: *inventory, cost, authenticity }).await {
                    Ok(row) => {
                        shelf.push(row.clone());
                        Ok((row.id, None, Some(*inventory), None, "ATTACH"))
                    }
                    Err(BridgeError::DuplicateRow) => {
                        // the mirror lagged behind a row that exists: re-read and patch instead
                        shelf = svc.landau.shelf(&store).await?;
                        match find_row(&shelf, &product, authenticity.as_str()) {
                            Some(row) => {
                                let inv = (mode == ImportMode::Quantity).then_some(row.inventory + item.qty);
                                svc.landau.patch(&row.id, &PatchBody { inventory: inv, cost: Some(cost) }).await.map(|_| (row.id.clone(), Some(row.inventory), inv.or(Some(row.inventory)), row.price, "PATCH"))
                            }
                            None => Err(BridgeError::DuplicateRow),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            WriteAction::Patch { stock_id, inventory, qty_before } => {
                let price_before = existing.and_then(|r| r.price);
                svc.landau.patch(stock_id, &PatchBody { inventory: *inventory, cost: Some(cost) }).await.map(|_| (stock_id.clone(), Some(*qty_before), inventory.or(Some(*qty_before)), price_before, "PATCH"))
            }
        };
        match result {
            Ok((stock_id, before, after, price_before, verb)) => {
                ctx.item(item.id, &[("stock_id", t(&stock_id)), ("action", t(verb)), ("qty_before", opt_i(before)), ("qty_after", opt_i(after)), ("price_before", opt_i(price_before)), ("authenticity", t(authenticity.as_str())), ("status", t("WRITTEN"))])?;
                written.push((item.id, stock_id));
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("WRITE", &e, Some(json!({"item_id": item.id, "product": product})));
                ctx.item(item.id, &[("status", t("FAILED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
            }
        }
        ctx.progress("WRITE", n as u32 + 1, total, "نوشتن روی قفسه لاندا");
    }

    reprice_items(ctx, svc, &store, &written).await?;
    readback(ctx, svc, &store, &written).await?;

    if mode == ImportMode::Quantity {
        let ids: Vec<i64> = job.summary.as_ref().and_then(|s| s.get("invoice_ids")).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
        ctx.db.mark_invoices_imported(&ids, ctx.job_id)?;
    }
    let all = ctx.db.job_items(ctx.job_id)?;
    let count = |status: &str| all.iter().filter(|i| i.status == status).count();
    let summary = json!({
        "items": all.len(),
        "done": count("DONE"),
        "attached": all.iter().filter(|i| i.action.as_deref() == Some("ATTACH")).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        "skipped": count("SKIPPED"),
        "failed": count("FAILED"),
        "invoice_ids": job.summary.as_ref().and_then(|s| s.get("invoice_ids")).cloned().unwrap_or(json!([])),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}
```

- [ ] **Step 6: Implement `jobs/refresh.rs`**

```rust
//! Feature 2 (spec §7.2): every shelf row with a supplier link gets today's cost, then Landau's
//! rules run over the store.

use std::collections::HashMap;

use serde_json::json;

use super::{cost_for, i, opt_i, preflight, readback, reprice_items, t, JobCtx, Services};
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::PatchBody;
use crate::settings::SUPPLIER_ID;

/// product_id → the code to price it by. Several codes for one product: the most recently
/// invoiced one, and a warning.
fn links(ctx: &JobCtx) -> Result<HashMap<String, String>> {
    let rows = ctx.db.linked_matches(SUPPLIER_ID)?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let last_seen: HashMap<&str, &str> = lines.iter().map(|l| (l.code.as_str(), l.date_jalali.as_str())).collect(); // oldest first → last write wins
    let mut by_product: HashMap<String, Vec<String>> = HashMap::new();
    for r in &rows {
        if let Some(p) = &r.product_id {
            by_product.entry(p.clone()).or_default().push(r.code.clone());
        }
    }
    let mut out = HashMap::new();
    for (product, mut codes) in by_product {
        if codes.len() > 1 {
            codes.sort_by(|a, b| last_seen.get(b.as_str()).cmp(&last_seen.get(a.as_str())));
            ctx.warn("PRICE", kinds::AMBIGUOUS_SUPPLIER_LINK, &format!("قطعه {product} به {} کد بارزپخش وصل است؛ {} (آخرین خرید) ملاک شد.", codes.len(), codes[0]), Some(json!({"product": product, "codes": codes})));
        }
        out.insert(product, codes[0].clone());
    }
    Ok(out)
}

pub async fn run(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;

    ctx.phase("PRICE")?;
    let shelf = svc.landau.shelf(&store).await?;
    let links = links(ctx)?;
    let total = shelf.len() as u32;
    let mut to_write: Vec<(i64, String, i64)> = Vec::new(); // item, stock, cost
    for (n, row) in shelf.iter().enumerate() {
        ctx.check_cancel()?;
        let name = row.product_name.clone().unwrap_or_else(|| row.product.clone());
        let Some(code) = links.get(&row.product) else {
            let id = ctx.db.add_job_item(ctx.job_id, "", &name, row.inventory)?;
            ctx.item(id, &[("stock_id", t(&row.id)), ("product_id", t(&row.product)), ("price_before", opt_i(row.price)), ("action", t("SKIP")), ("status", t("SKIPPED")), ("error_kind", t(kinds::NO_SUPPLIER_LINK)), ("error_msg", t("این ردیف به هیچ کد بارزپخش وصل نیست."))])?;
            continue;
        };
        let id = ctx.db.add_job_item(ctx.job_id, code, &name, row.inventory)?;
        ctx.item(id, &[("stock_id", t(&row.id)), ("product_id", t(&row.product)), ("price_before", opt_i(row.price)), ("qty_before", i(row.inventory))])?;
        match cost_for(ctx, svc, "PRICE", code).await {
            Ok(Some(cost)) => {
                ctx.item(id, &[("cost", i(cost))])?;
                if row.cost == Some(cost) {
                    ctx.item(id, &[("action", t("SKIP")), ("reprice_verdict", t("COST_UNCHANGED")), ("status", t("WRITTEN"))])?;
                    to_write.push((id, row.id.clone(), cost)); // still repriced: a rule may have changed
                } else {
                    to_write.push((id, row.id.clone(), cost));
                }
            }
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice { code: code.clone() };
                ctx.error("PRICE", &e, Some(json!({"item_id": id})));
                ctx.item(id, &[("status", t("SKIPPED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
            }
            Err(e) if matches!(e, BridgeError::SupplierUnreachable(_) | BridgeError::SupplierBlocked | BridgeError::SupplierSessionExpired | BridgeError::JobCancelled) => return Err(e),
            Err(e) => {
                ctx.error("PRICE", &e, Some(json!({"item_id": id})));
                ctx.item(id, &[("status", t("FAILED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }

    ctx.phase("WRITE")?;
    let mut written: Vec<(i64, String)> = Vec::new();
    let total = to_write.len() as u32;
    for (n, (item_id, stock_id, cost)) in to_write.iter().enumerate() {
        ctx.check_cancel()?;
        let unchanged = ctx.db.job_item(*item_id)?.map(|i| i.reprice_verdict.as_deref() == Some("COST_UNCHANGED")).unwrap_or(false);
        if !unchanged {
            match svc.landau.patch(stock_id, &PatchBody { inventory: None, cost: Some(*cost) }).await {
                Ok(()) => ctx.item(*item_id, &[("action", t("PATCH")), ("status", t("WRITTEN"))])?,
                Err(e @ (BridgeError::LandauUnauthorized | BridgeError::LandauUnreachable(_))) => return Err(e),
                Err(e) => {
                    ctx.error("WRITE", &e, Some(json!({"item_id": item_id})));
                    ctx.item(*item_id, &[("status", t("FAILED")), ("error_kind", t(e.kind())), ("error_msg", t(&e.message_fa()))])?;
                    continue;
                }
            }
        }
        written.push((*item_id, stock_id.clone()));
        ctx.progress("WRITE", n as u32 + 1, total, "به‌روزرسانی قیمت خرید روی قفسه");
    }

    reprice_items(ctx, svc, &store, &written).await?;
    readback(ctx, svc, &store, &written).await?;

    let all = ctx.db.job_items(ctx.job_id)?;
    let summary = json!({
        "rows": all.len(),
        "linked": all.iter().filter(|i| !i.code.is_empty()).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        "repriced": all.iter().filter(|i| i.reprice_verdict.as_deref().map(|v| v.starts_with("REPRICED:")).unwrap_or(false)).count(),
        "skipped": all.iter().filter(|i| i.status == "SKIPPED").count(),
        "failed": all.iter().filter(|i| i.status == "FAILED").count(),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}
```

- [ ] **Step 7: Run the planner tests and the gates**

Run: `cd src-tauri && cargo test jobs:: && cargo clippy --all-targets --features live -- -D warnings`
Expected: 5 planner tests pass; clippy clean. Common fixes: `Emitter` must be in scope for `app.emit`; the `search` closure in `match_phase` must be `Fn` — it is, because it only captures `&LandauClient`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/jobs
git commit -m "Add the import and refresh job runners

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 12: Tauri commands, app state, logging

Implements spec §3 (runtime shape, commands/events) and §9 (file log). Commands are thin: validate → call → DTO. Every command uses `rename_all = "snake_case"` so the UI passes snake_case argument names.

**Files:**
- Create: `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/dto.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `AppState`; commands `settings_get`, `settings_save`, `landau_validate`, `supplier_test_login`, `dashboard_get`, `import_start`, `import_review`, `review_decide`, `review_search`, `import_continue`, `refresh_start`, `job_cancel`, `job_current`, `jobs_list`, `job_get`, `matches_list`, `match_set`, `match_none`, `match_clear`, `errors_list`, `open_log_folder`; events `job://progress`, `job://event`, `job://done`.

- [ ] **Step 1: DTOs** — `src-tauri/src/commands/dto.rs`

```rust
//! Wire shapes between the Rust core and the Nuxt pages. Mirrored in app/types/bridge.ts.

use serde::{Deserialize, Serialize};

use crate::db::jobs::JobRow;
use crate::domain::Authenticity;
use crate::landau::Store;

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub landau_url: String,
    pub has_landau_token: bool,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub has_supplier_password: bool,
    pub default_authenticity: Authenticity,
    pub threshold: u8,
    pub vat_percent: f64,
    pub overwrite_manual: bool,
    pub secret_backend: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandauStatus {
    pub business: String,
    pub business_name: Option<String>,
    pub approved: bool,
    pub stores: Vec<Store>,
    pub active_rules: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub settings_complete: bool,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub invoices_cached: i64,
    pub invoices_unpaid: i64,
    pub last_import: Option<JobRow>,
    pub last_refresh: Option<JobRow>,
    pub running: Option<JobRow>,
    pub pending_review: i64,
    pub recent_errors: i64,
    pub decided_matches: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewDecision {
    pub item_id: i64,
    /// Pick this registry part…
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    pub similarity: Option<i64>,
    /// …or «هیچ‌کدام» (open a Landau ticket)…
    #[serde(default)]
    pub none: bool,
    /// …or leave it for next time.
    #[serde(default)]
    pub later: bool,
    pub authenticity: Option<Authenticity>,
}
```

- [ ] **Step 2: Commands** — `src-tauri/src/commands/mod.rs`

```rust
//! Tauri commands (spec §3). Thin by design: no business logic lives here.

pub mod dto;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::errors::ErrorRow;
use crate::db::jobs::{JobDetail, JobItemRow, JobRow};
use crate::db::matches::MatchRow;
use crate::db::Db;
use crate::domain::ImportMode;
use crate::errors::{kinds, BridgeError, Result};
use crate::jobs::{self, JobCtx, JobRegistry, Services};
use crate::landau::LandauClient;
use crate::matching::{candidates, score, Matcher, Scored};
use crate::secrets::SecretStore;
use crate::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use crate::supplier;
use dto::*;

pub struct AppState {
    pub db: Db,
    pub secrets: SecretStore,
    pub jobs: Arc<JobRegistry>,
    pub data_dir: PathBuf,
    pub _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

fn settings(state: &AppState) -> Result<Settings> {
    Settings::load(&state.db, &state.secrets)
}

fn landau(s: &Settings) -> Result<LandauClient> {
    let token = s.landau_token.as_deref().filter(|t| !t.is_empty()).ok_or_else(|| BridgeError::SettingsIncomplete("کلید API لاندا".into()))?;
    LandauClient::new(&s.landau_url, token)
}

fn services(state: &AppState) -> Result<Services> {
    let settings = settings(state)?;
    let landau = landau(&settings)?;
    let supplier = supplier::open(SUPPLIER_ID, settings.require_supplier()?)?;
    Ok(Services { landau, supplier, settings })
}

// ------------------------------------------------------------ settings

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_get(state: State<'_, AppState>) -> Result<SettingsView> {
    let s = settings(&state)?;
    Ok(SettingsView {
        landau_url: s.landau_url,
        has_landau_token: s.landau_token.as_deref().map(|t| !t.is_empty()).unwrap_or(false),
        store: s.store,
        store_name: s.store_name,
        supplier_user: s.supplier_user,
        has_supplier_password: s.supplier_password.as_deref().map(|p| !p.is_empty()).unwrap_or(false),
        default_authenticity: s.default_authenticity,
        threshold: s.threshold,
        vat_percent: s.vat_percent,
        overwrite_manual: s.overwrite_manual,
        secret_backend: state.secrets.backend().into(),
        data_dir: state.data_dir.display().to_string(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_save(state: State<'_, AppState>, patch: SettingsPatch) -> Result<SettingsView> {
    Settings::save(&state.db, &state.secrets, &patch)?;
    settings_get(state).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn landau_validate(state: State<'_, AppState>) -> Result<LandauStatus> {
    let s = settings(&state)?;
    let client = landau(&s)?;
    let memberships = client.memberships().await?;
    let mine = memberships.iter().find(|m| m.approved).or_else(|| memberships.first()).ok_or(BridgeError::NoStore)?;
    if !mine.approved {
        return Err(BridgeError::BusinessNotApproved);
    }
    let stores: Vec<_> = mine.stores.iter().filter(|st| st.state.as_deref() != Some("REJECTED")).cloned().collect();
    if stores.is_empty() {
        return Err(BridgeError::NoStore);
    }
    let active_rules = client.pricing_rules().await.map(|r| r.iter().filter(|x| x.active).count()).unwrap_or(0);
    Ok(LandauStatus { business: mine.business.clone(), business_name: mine.name.clone(), approved: mine.approved, stores, active_rules })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn supplier_test_login(state: State<'_, AppState>) -> Result<()> {
    let s = settings(&state)?;
    let sup = supplier::open(SUPPLIER_ID, s.require_supplier()?)?;
    sup.login().await
}

// ------------------------------------------------------------ dashboard

#[tauri::command(rename_all = "snake_case")]
pub async fn dashboard_get(state: State<'_, AppState>) -> Result<Dashboard> {
    let s = settings(&state)?;
    let jobs = state.db.list_jobs()?;
    let (invoices_cached, invoices_unpaid) = state.db.count_invoices(SUPPLIER_ID)?;
    let running = state.db.running_job()?;
    let pending_review = match &running {
        Some(j) if j.status == "REVIEW" => state.db.job_items(j.id)?.iter().filter(|i| i.status == "REVIEW").count() as i64,
        _ => 0,
    };
    Ok(Dashboard {
        settings_complete: s.require_landau().is_ok() && s.require_supplier().is_ok(),
        store_name: s.store_name,
        supplier_user: s.supplier_user,
        invoices_cached,
        invoices_unpaid,
        last_import: jobs.iter().find(|j| j.kind == "IMPORT").cloned(),
        last_refresh: jobs.iter().find(|j| j.kind == "REFRESH").cloned(),
        running,
        pending_review,
        recent_errors: state.db.count_unresolved_errors()?,
        decided_matches: state.db.list_matches(SUPPLIER_ID)?.len() as i64,
    })
}

// ------------------------------------------------------------ jobs

fn spawn<F, Fut>(app: AppHandle, state: &AppState, job_id: i64, run: F) -> Result<()>
where
    F: FnOnce(JobCtx, Services) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let cancel = state.jobs.begin(job_id)?;
    let ctx = JobCtx { db: state.db.clone(), app: Some(app), job_id, cancel };
    let registry = state.jobs.clone();
    let svc = match services(state) {
        Ok(s) => s,
        Err(e) => {
            ctx.finish(&Err(e.clone()), "PREFLIGHT");
            registry.finish(job_id);
            return Err(e);
        }
    };
    tauri::async_runtime::spawn(async move {
        let outcome = run(ctx, svc).await;
        registry.finish(job_id);
        outcome
    });
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_start(app: AppHandle, state: State<'_, AppState>, mode: String, since_date: Option<String>, fetch_limit: Option<usize>) -> Result<i64> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    let mode = ImportMode::parse(&mode).ok_or_else(|| BridgeError::Other(format!("unknown mode {mode}")))?;
    let since = since_date.as_deref().and_then(crate::domain::normalize_jalali);
    if mode == ImportMode::Quantity && since.is_none() {
        return Err(BridgeError::SettingsIncomplete("تاریخ شروع فاکتورها (مثلاً ۱۴۰۳/۰۱/۰۱)".into()));
    }
    let job_id = state.db.create_job("IMPORT", Some(mode.as_str()), since.as_deref(), store, s.overwrite_manual)?;
    let params = jobs::import::ImportParams { mode, since, fetch_limit };
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let phase = ctx.db.job(ctx.job_id).ok().flatten().map(|j| j.phase).unwrap_or_default();
        match jobs::import::start(&ctx, &svc, &params).await {
            Ok(jobs::import::Outcome::Review(_)) => Ok(()), // paused; status already REVIEW
            Ok(jobs::import::Outcome::Done) => {
                ctx.finish(&Ok(()), "REPORT");
                Ok(())
            }
            Err(e) => {
                let phase = ctx.db.job(ctx.job_id).ok().flatten().map(|j| j.phase).unwrap_or(phase);
                ctx.finish(&Err(e.clone()), &phase);
                Err(e)
            }
        }
    })?;
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_review(state: State<'_, AppState>, job_id: i64) -> Result<Vec<JobItemRow>> {
    Ok(state.db.job_items(job_id)?.into_iter().filter(|i| i.status == "REVIEW").collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn review_decide(state: State<'_, AppState>, decision: ReviewDecision) -> Result<()> {
    let item = state.db.job_item(decision.item_id)?.ok_or(BridgeError::JobNotFound(decision.item_id))?;
    let s = settings(&state)?;
    let matcher = Matcher::new(state.db.clone(), SUPPLIER_ID, s.threshold);
    if decision.later {
        return Ok(()); // stays REVIEW; commit() skips it and the next run asks again
    }
    if decision.none {
        matcher.record_none(&item.code, &item.name)?;
        state.db.record_error(Some(item.job_id), kinds::NONE_DECISION, &format!("«{}» در فهرست لاندا نیست؛ برای افزودن آن یک تیکت برای پشتیبانی لاندا ثبت کنید.", item.name), Some(serde_json::json!({"code": item.code})))?;
        return state.db.update_job_item(item.id, &[("status", jobs::t("SKIPPED")), ("error_kind", jobs::t(kinds::NONE_DECISION)), ("error_msg", jobs::t("این قطعه در لاندا نیست."))]);
    }
    let product_id = decision.product_id.ok_or_else(|| BridgeError::Other("no product picked".into()))?;
    let product_name = decision.product_name.unwrap_or_default();
    matcher.record_user_pick(&item.code, &item.name, &product_id, &product_name, decision.similarity)?;
    let mut patch = vec![("product_id", jobs::t(&product_id)), ("product_name", jobs::t(&product_name)), ("status", jobs::t("PLANNED"))];
    if let Some(a) = decision.authenticity {
        patch.push(("authenticity", jobs::t(a.as_str())));
    }
    state.db.update_job_item(item.id, &patch)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn review_search(state: State<'_, AppState>, term: String) -> Result<Vec<Scored>> {
    let s = settings(&state)?;
    let client = landau(&s)?;
    let search = |q: String, limit: usize, skip: usize| async move { client.registry_search(&q, limit, skip).await };
    let hits = candidates(&search, &term).await?;
    Ok(score(&term, &hits).into_iter().take(crate::matching::CANDIDATES).collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_continue(app: AppHandle, state: State<'_, AppState>, job_id: i64) -> Result<()> {
    let job = state.db.job(job_id)?.ok_or(BridgeError::JobNotFound(job_id))?;
    if job.status != "REVIEW" {
        return Err(BridgeError::JobBusy(format!("#{job_id} is {}", job.status)));
    }
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let outcome = jobs::import::commit(&ctx, &svc).await;
        let phase = ctx.db.job(ctx.job_id).ok().flatten().map(|j| j.phase).unwrap_or_default();
        ctx.finish(&outcome, &phase);
        outcome
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn refresh_start(app: AppHandle, state: State<'_, AppState>) -> Result<i64> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    s.require_supplier()?;
    let job_id = state.db.create_job("REFRESH", None, None, store, s.overwrite_manual)?;
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let outcome = jobs::refresh::run(&ctx, &svc).await;
        let phase = ctx.db.job(ctx.job_id).ok().flatten().map(|j| j.phase).unwrap_or_default();
        ctx.finish(&outcome, &phase);
        outcome
    })?;
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_cancel(state: State<'_, AppState>, job_id: i64) -> Result<bool> {
    Ok(state.jobs.cancel(job_id))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_current(state: State<'_, AppState>) -> Result<Option<JobRow>> {
    state.db.running_job()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn jobs_list(state: State<'_, AppState>) -> Result<Vec<JobRow>> {
    state.db.list_jobs()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_get(state: State<'_, AppState>, job_id: i64) -> Result<JobDetail> {
    state.db.job_detail(job_id)
}

// ------------------------------------------------------------ matches & errors

#[tauri::command(rename_all = "snake_case")]
pub async fn matches_list(state: State<'_, AppState>) -> Result<Vec<MatchRow>> {
    state.db.list_matches(SUPPLIER_ID)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_set(state: State<'_, AppState>, code: String, product_id: String, product_name: String, name_seen: String) -> Result<()> {
    state.db.set_match(SUPPLIER_ID, &code, Some((&product_id, &product_name)), "USER", None, &name_seen)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_none(state: State<'_, AppState>, code: String, name_seen: String) -> Result<()> {
    state.db.set_match(SUPPLIER_ID, &code, None, "NONE", None, &name_seen)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_clear(state: State<'_, AppState>, code: String) -> Result<()> {
    state.db.clear_match(SUPPLIER_ID, &code)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn errors_list(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<ErrorRow>> {
    state.db.list_errors(limit.unwrap_or(200))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_log_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let dir = state.data_dir.join("logs");
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(|e| BridgeError::Other(e.to_string()))
}
```

- [ ] **Step 3: App wiring** — replace `run()` in `src-tauri/src/lib.rs`

```rust
use std::sync::Arc;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(data_dir.join("logs")).expect("create data dir");

            let file = tracing_appender::rolling::daily(data_dir.join("logs"), "bridge.log");
            let (writer, guard) = tracing_appender::non_blocking(file);
            let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,landau_bridge_lib=debug".into());
            tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).init();
            tracing::info!("landau-bridge starting; data dir {}", data_dir.display());

            let db = db::Db::open(&data_dir.join("bridge.sqlite")).expect("open database");
            for id in db.interrupt_stale_jobs().expect("stale jobs") {
                let _ = db.record_error(Some(id), errors::kinds::JOB_INTERRUPTED, "برنامه در میانه این عملیات بسته شده بود؛ می‌توانید دوباره اجرا کنید.", None);
            }
            let secrets = secrets::SecretStore::open(&data_dir);
            app.manage(AppState { db, secrets, jobs: Arc::new(jobs::JobRegistry::default()), data_dir, _log_guard: guard });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings_get,
            commands::settings_save,
            commands::landau_validate,
            commands::supplier_test_login,
            commands::dashboard_get,
            commands::import_start,
            commands::import_review,
            commands::review_decide,
            commands::review_search,
            commands::import_continue,
            commands::refresh_start,
            commands::job_cancel,
            commands::job_current,
            commands::jobs_list,
            commands::job_get,
            commands::matches_list,
            commands::match_set,
            commands::match_none,
            commands::match_clear,
            commands::errors_list,
            commands::open_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Gates**

Run: `cd src-tauri && cargo clippy --all-targets --features live -- -D warnings && cargo test`
Expected: clean and green. Typical fixes: `Scored` must derive `Serialize` (it does); `SettingsPatch` must derive `Deserialize` (it does); `Manager` in scope for `app.path()`/`app.manage()`.

Run: `pnpm tauri dev` and, in the window's devtools console, `await window.__TAURI__.core.invoke('settings_get')` — with `"app": {"withGlobalTauri": true}` temporarily added to `tauri.conf.json` if `__TAURI__` is undefined. Expected: the settings object with `secret_backend` and `data_dir`. Remove the temporary flag afterwards.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands src-tauri/src/lib.rs
git commit -m "Wire the Tauri commands, app state and file logging

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 13: UI foundation — types, bridge composable, Persian utils, layout

Implements spec §10 (shell). Everything a page needs to talk to the core and to render Persian.

**Files:**
- Create: `app/types/bridge.ts`, `app/composables/useBridge.ts`, `app/composables/useJob.ts`, `app/utils/fa.ts`, `app/utils/fa.spec.ts`, `app/layouts/default.vue`, `app/components/ErrorBanner.vue`, `app/components/JobProgress.vue`
- Modify: `app/app.vue`

**Interfaces:**
- Produces: `bridge.*` typed wrappers over every command of Task 12; `onJobEvents({progress, event, done})`; `useJob(jobId)`; `fa.ts` helpers `faNumber`, `rials`, `latinDigits`, `normalizeJalali`, `todayJalali`, `jalaliYearStart`, `isoToLocal`, `phaseLabel`, `statusLabel`, `verdictLabel`, `kindLabel`.

- [ ] **Step 1: Write the failing utils test** — `app/utils/fa.spec.ts`

```ts
import { describe, expect, it } from 'vitest';
import { faNumber, rials, latinDigits, normalizeJalali, todayJalali, jalaliYearStart, verdictLabel, kindLabel } from './fa';

describe('fa', () => {
  it('formats numbers with Persian digits and grouping', () => {
    expect(faNumber(1234567)).toBe('۱٬۲۳۴٬۵۶۷');
    expect(faNumber(null)).toBe('—');
    expect(rials(173000)).toBe('۱۷۳٬۰۰۰ ریال');
  });

  it('normalises Jalali dates', () => {
    expect(latinDigits('۱۴۰۱/۱/۵')).toBe('1401/1/5');
    expect(normalizeJalali('۱۴۰۱/۱/۵')).toBe('1401/01/05');
    expect(normalizeJalali('1401-01-05')).toBeNull();
    expect(normalizeJalali('1401/13/01')).toBeNull();
  });

  it('knows today and the year start in Jalali', () => {
    expect(todayJalali()).toMatch(/^1[34]\d\d\/\d\d\/\d\d$/);
    expect(jalaliYearStart()).toMatch(/^1[34]\d\d\/01\/01$/);
  });

  it('labels verdicts and kinds in Persian', () => {
    expect(verdictLabel('REPRICED:173000')).toContain('۱۷۳٬۰۰۰');
    expect(verdictLabel('SKIPPED:MANUAL')).toContain('دستی');
    expect(verdictLabel(null)).toBe('—');
    expect(kindLabel('NoCurrentPrice')).toContain('قیمت');
    expect(kindLabel('Whatever')).toBe('Whatever');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test`
Expected: FAIL — `./fa` not found.

- [ ] **Step 3: Implement `app/utils/fa.ts`**

```ts
import { PersianCalendar, toCalendar, today } from '@internationalized/date';

const FA_DIGITS = '۰۱۲۳۴۵۶۷۸۹';
const AR_DIGITS = '٠١٢٣٤٥٦٧٨٩';
const nf = new Intl.NumberFormat('fa-IR', { maximumFractionDigits: 0 });

export const faNumber = (n: number | null | undefined): string => (n == null || Number.isNaN(n) ? '—' : nf.format(n));
export const rials = (n: number | null | undefined): string => (n == null ? '—' : `${faNumber(n)} ریال`);

export const latinDigits = (s: string): string =>
  s.replace(/[۰-۹]/g, (d) => String(FA_DIGITS.indexOf(d))).replace(/[٠-٩]/g, (d) => String(AR_DIGITS.indexOf(d)));

/** `۱۴۰۱/۱/۵` → `1401/01/05`; null when not a Jalali Y/M/D. */
export const normalizeJalali = (s: string): string | null => {
  const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})$/.exec(latinDigits(s.trim()));
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (y < 1300 || mo < 1 || mo > 12 || d < 1 || d > 31) return null;
  return `${y}/${String(mo).padStart(2, '0')}/${String(d).padStart(2, '0')}`;
};

const jalaliToday = () => toCalendar(today('Asia/Tehran'), new PersianCalendar());
export const todayJalali = (): string => {
  const t = jalaliToday();
  return `${t.year}/${String(t.month).padStart(2, '0')}/${String(t.day).padStart(2, '0')}`;
};
export const jalaliYearStart = (): string => `${jalaliToday().year}/01/01`;

export const isoToLocal = (iso: string | null | undefined): string =>
  iso ? new Intl.DateTimeFormat('fa-IR', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(iso)) : '—';

export const phaseLabel: Record<string, string> = {
  PREFLIGHT: 'بررسی اتصال‌ها',
  FETCH: 'دریافت فاکتورها',
  AGGREGATE: 'جمع‌بندی اقلام',
  MATCH: 'تطبیق با فهرست لاندا',
  REVIEW: 'در انتظار تصمیم شما',
  PRICE: 'قیمت روز بارزپخش',
  WRITE: 'نوشتن روی قفسه',
  REPRICE: 'اعمال قواعد قیمت‌گذاری لاندا',
  REPORT: 'گزارش نهایی',
};

export const statusLabel: Record<string, string> = {
  RUNNING: 'در حال اجرا',
  REVIEW: 'در انتظار تصمیم',
  DONE: 'انجام شد',
  FAILED: 'ناموفق',
  CANCELLED: 'لغو شد',
  INTERRUPTED: 'ناتمام',
  PLANNED: 'برنامه‌ریزی‌شده',
  SKIPPED: 'رد شد',
  WRITTEN: 'نوشته شد',
};

const skipLabel: Record<string, string> = {
  MANUAL: 'قیمت دستی حفظ شد',
  NO_COST: 'بدون قیمت خرید',
  NO_RULE: 'قاعده‌ای در لاندا ندارید',
  UNCHANGED: 'بدون تغییر',
};

export const verdictLabel = (v: string | null | undefined): string => {
  if (!v) return '—';
  if (v.startsWith('REPRICED:')) return `قیمت‌گذاری شد: ${rials(Number(v.slice(9)))}`;
  if (v.startsWith('SKIPPED:')) return skipLabel[v.slice(8)] ?? v;
  if (v === 'COST_UNCHANGED') return 'قیمت خرید تغییری نکرده';
  if (v === 'UNCHANGED') return 'بدون تغییر';
  return v;
};

export const kindLabel = (kind: string): string =>
  ({
    SupplierUnreachable: 'اتصال به بارزپخش',
    SupplierBlocked: 'مسدود شدن توسط بارزپخش',
    SupplierBadCredentials: 'نام کاربری/کلمه عبور بارزپخش',
    SupplierSessionExpired: 'انقضای نشست بارزپخش',
    SupplierParse: 'تغییر ساختار صفحه بارزپخش',
    NoCurrentPrice: 'بدون قیمت روز',
    InvoiceUnpaid: 'فاکتور پرداخت‌نشده',
    InvoiceAlreadyImported: 'فاکتور قبلاً منتقل‌شده',
    LineNonPositiveQty: 'ردیف با تعداد صفر',
    LandauUnauthorized: 'کلید API لاندا',
    LandauForbidden: 'عدم مجوز در لاندا',
    LandauUnreachable: 'اتصال به لاندا',
    LandauRejected: 'رد درخواست توسط لاندا',
    BusinessNotApproved: 'کسب‌وکار تأییدنشده',
    NoStore: 'بدون فروشگاه',
    ProductNotApproved: 'قطعه تأییدنشده',
    DuplicateRow: 'ردیف تکراری',
    MirrorTimeout: 'تأخیر قفسه لاندا',
    NoPricingRule: 'بدون قاعده قیمت‌گذاری',
    RepriceSkipped: 'قیمت‌گذاری رد شد',
    Unmatched: 'تطبیق‌نشده',
    NoneDecision: 'در فهرست لاندا نیست',
    JobCancelled: 'لغو شد',
    JobInterrupted: 'ناتمام ماند',
    VatRateMismatch: 'اختلاف نرخ ارزش افزوده',
    NegativeQuantity: 'مرجوعی بیش از خرید',
    AmbiguousSupplierLink: 'چند کد برای یک قطعه',
    NoSupplierLink: 'بدون پیوند به بارزپخش',
    SettingsIncomplete: 'تنظیمات ناقص',
    KeyringUnavailable: 'ذخیره‌ساز امن در دسترس نیست',
  })[kind] ?? kind;
```

- [ ] **Step 4: Run the test**

Run: `pnpm test`
Expected: 4 tests pass. If `faNumber` yields `۱,۲۳۴,۵۶۷` (ASCII comma) on this Node's ICU, change the expected strings to what `Intl` produces here and note it in the test — the UI accepts either.

- [ ] **Step 5: Types and the bridge composable**

`app/types/bridge.ts`:
```ts
export type Authenticity = 'ORIGINAL' | 'FAKE';
export type ImportMode = 'QUANTITY' | 'ZERO';
export type Phase = 'PREFLIGHT' | 'FETCH' | 'AGGREGATE' | 'MATCH' | 'REVIEW' | 'PRICE' | 'WRITE' | 'REPRICE' | 'REPORT';

export interface BridgeError { kind: string; message: string; detail: string }

export interface SettingsView {
  landau_url: string; has_landau_token: boolean; store: string | null; store_name: string | null;
  supplier_user: string | null; has_supplier_password: boolean; default_authenticity: Authenticity;
  threshold: number; vat_percent: number; overwrite_manual: boolean; secret_backend: 'keyring' | 'file'; data_dir: string;
}
export interface SettingsPatch {
  landau_url?: string; landau_token?: string; store?: string; store_name?: string; supplier_user?: string;
  supplier_password?: string; default_authenticity?: Authenticity; threshold?: number; vat_percent?: number; overwrite_manual?: boolean;
}
export interface Store { id: string; name: string | null; kind: string | null; state: string | null; status: string | null }
export interface LandauStatus { business: string; business_name: string | null; approved: boolean; stores: Store[]; active_rules: number }

export interface JobRow {
  id: number; kind: 'IMPORT' | 'REFRESH'; mode: ImportMode | null; since_date: string | null; store: string;
  overwrite: boolean; status: string; phase: Phase; started_at: string; finished_at: string | null; summary: Record<string, unknown> | null;
}
export interface Candidate { id: string; name: string; alias: string | null; brand: string | null; score: number }
export interface Scored { candidate: Candidate; similarity: number }
export interface JobItemRow {
  id: number; job_id: number; code: string; name: string; qty: number; product_id: string | null; product_name: string | null;
  stock_id: string | null; authenticity: Authenticity | null; action: 'ATTACH' | 'PATCH' | 'SKIP' | null;
  qty_before: number | null; qty_after: number | null; cost: number | null; price_before: number | null; price_after: number | null;
  reprice_verdict: string | null; error_kind: string | null; error_msg: string | null; candidates: Scored[] | null; status: string;
}
export interface JobEventRow { id: number; at: string; level: 'INFO' | 'WARN' | 'ERROR'; phase: string; message: string; detail: unknown }
export interface JobDetail { job: JobRow; items: JobItemRow[]; events: JobEventRow[] }
export interface Dashboard {
  settings_complete: boolean; store_name: string | null; supplier_user: string | null; invoices_cached: number; invoices_unpaid: number;
  last_import: JobRow | null; last_refresh: JobRow | null; running: JobRow | null; pending_review: number; recent_errors: number; decided_matches: number;
}
export interface MatchRow {
  supplier: string; code: string; product_id: string | null; product_name: string | null; how: 'AUTO' | 'USER' | 'NONE';
  similarity: number | null; name_seen: string; decided_at: string;
}
export interface ErrorRow { id: number; at: string; job_id: number | null; kind: string; message: string; context: unknown }
export interface ReviewDecision {
  item_id: number; product_id?: string; product_name?: string; similarity?: number; none?: boolean; later?: boolean; authenticity?: Authenticity;
}
export interface Progress { job_id: number; phase: Phase; done: number; total: number; message: string }
export interface JobEvent { job_id: number; level: 'INFO' | 'WARN' | 'ERROR'; kind?: string; phase: string; message: string; detail?: unknown }
export interface JobDone { job_id: number; status: string }
```

`app/composables/useBridge.ts`:
```ts
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  BridgeError, Dashboard, ErrorRow, JobDetail, JobDone, JobEvent, JobItemRow, JobRow, LandauStatus, MatchRow,
  Progress, ReviewDecision, Scored, SettingsPatch, SettingsView,
} from '~/types/bridge';

export const isBridgeError = (e: unknown): e is BridgeError =>
  typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;

/** One place every command goes through; a Rust `BridgeError` arrives as a typed rejection. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isBridgeError(e)) throw e;
    throw { kind: 'Other', message: String(e), detail: String(e) } satisfies BridgeError;
  }
}

export const bridge = {
  settingsGet: () => call<SettingsView>('settings_get'),
  settingsSave: (patch: SettingsPatch) => call<SettingsView>('settings_save', { patch }),
  landauValidate: () => call<LandauStatus>('landau_validate'),
  supplierTestLogin: () => call<void>('supplier_test_login'),
  dashboardGet: () => call<Dashboard>('dashboard_get'),
  importStart: (mode: 'QUANTITY' | 'ZERO', since_date?: string) => call<number>('import_start', { mode, since_date }),
  importReview: (job_id: number) => call<JobItemRow[]>('import_review', { job_id }),
  reviewDecide: (decision: ReviewDecision) => call<void>('review_decide', { decision }),
  reviewSearch: (term: string) => call<Scored[]>('review_search', { term }),
  importContinue: (job_id: number) => call<void>('import_continue', { job_id }),
  refreshStart: () => call<number>('refresh_start'),
  jobCancel: (job_id: number) => call<boolean>('job_cancel', { job_id }),
  jobCurrent: () => call<JobRow | null>('job_current'),
  jobsList: () => call<JobRow[]>('jobs_list'),
  jobGet: (job_id: number) => call<JobDetail>('job_get', { job_id }),
  matchesList: () => call<MatchRow[]>('matches_list'),
  matchSet: (code: string, product_id: string, product_name: string, name_seen: string) =>
    call<void>('match_set', { code, product_id, product_name, name_seen }),
  matchNone: (code: string, name_seen: string) => call<void>('match_none', { code, name_seen }),
  matchClear: (code: string) => call<void>('match_clear', { code }),
  errorsList: (limit = 200) => call<ErrorRow[]>('errors_list', { limit }),
  openLogFolder: () => call<void>('open_log_folder'),
};

export interface JobHandlers {
  progress?: (p: Progress) => void;
  event?: (e: JobEvent) => void;
  done?: (d: JobDone) => void;
}

/** Subscribe to the three job events; returns one function that unsubscribes all. */
export async function onJobEvents(h: JobHandlers): Promise<UnlistenFn> {
  const offs: UnlistenFn[] = [];
  if (h.progress) offs.push(await listen<Progress>('job://progress', (e) => h.progress!(e.payload)));
  if (h.event) offs.push(await listen<JobEvent>('job://event', (e) => h.event!(e.payload)));
  if (h.done) offs.push(await listen<JobDone>('job://done', (e) => h.done!(e.payload)));
  return () => offs.forEach((off) => off());
}
```

`app/composables/useJob.ts` — live state for one job, re-read from the DB on open:
```ts
import type { JobDetail, JobEvent, Phase, Progress } from '~/types/bridge';

export function useJob() {
  const jobId = ref<number | null>(null);
  const detail = ref<JobDetail | null>(null);
  const phase = ref<Phase | null>(null);
  const progress = ref<Progress | null>(null);
  const events = ref<JobEvent[]>([]);
  const status = ref<string | null>(null);
  let off: (() => void) | null = null;

  const reload = async () => {
    if (jobId.value == null) return;
    detail.value = await bridge.jobGet(jobId.value);
    status.value = detail.value.job.status;
    phase.value = detail.value.job.phase;
  };

  const follow = async (id: number) => {
    jobId.value = id;
    events.value = [];
    progress.value = null;
    await reload();
    off?.();
    off = await onJobEvents({
      progress: (p) => { if (p.job_id === id) { progress.value = p; phase.value = p.phase; } },
      event: (e) => { if (e.job_id === id) events.value.unshift(e); },
      done: async (d) => { if (d.job_id === id) { status.value = d.status; await reload(); } },
    });
  };

  onBeforeUnmount(() => off?.());
  return { jobId, detail, phase, progress, events, status, follow, reload };
}
```

- [ ] **Step 6: Layout, banner, progress components, app shell**

`app/app.vue`:
```vue
<script setup lang="ts">
import { fa } from '@nuxt/ui/locale';
</script>

<template>
  <UApp :locale="fa">
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </UApp>
</template>
```
(If `@nuxt/ui/locale` has no `fa` export in the installed version, drop the `:locale` prop — the page is RTL through `htmlAttrs` regardless.)

`app/layouts/default.vue`:
```vue
<script setup lang="ts">
const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/logs', label: 'گزارش و خطاها', icon: 'i-lucide-scroll-text' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const running = ref<{ id: number; kind: string; status: string } | null>(null);
const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
</script>

<template>
  <div class="flex min-h-screen bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
    <aside class="w-64 shrink-0 border-e border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-900">
      <h1 class="mb-6 text-lg font-bold">پل لاندا</h1>
      <nav class="space-y-1">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="flex items-center gap-2 rounded-lg px-3 py-2 hover:bg-neutral-100 dark:hover:bg-neutral-800" active-class="bg-primary-50 text-primary-700 dark:bg-primary-950">
          <UIcon :name="l.icon" class="size-5" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>
      <div v-if="running" class="mt-6 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-100">
        عملیات {{ running.kind === 'IMPORT' ? 'وارد کردن' : 'به‌روزرسانی' }} شماره {{ faNumber(running.id) }} —
        {{ statusLabel[running.status] ?? running.status }}
      </div>
    </aside>
    <main class="flex-1 overflow-auto p-6">
      <slot />
    </main>
  </div>
</template>
```

`app/components/ErrorBanner.vue`:
```vue
<script setup lang="ts">
import type { BridgeError } from '~/types/bridge';
defineProps<{ error: BridgeError | null }>();
</script>

<template>
  <UAlert v-if="error" color="error" variant="soft" icon="i-lucide-alert-triangle" :title="kindLabel(error.kind)" :description="error.message" class="mb-4" />
</template>
```

`app/components/JobProgress.vue`:
```vue
<script setup lang="ts">
import type { JobEvent, Phase, Progress } from '~/types/bridge';
const props = defineProps<{ phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null }>();
const phases: Phase[] = ['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];
const index = computed(() => (props.phase ? phases.indexOf(props.phase) : -1));
const percent = computed(() => (props.progress && props.progress.total > 0 ? Math.round((props.progress.done / props.progress.total) * 100) : null));
</script>

<template>
  <div class="space-y-3">
    <ol class="flex flex-wrap gap-2 text-sm">
      <li v-for="(p, i) in phases" :key="p" class="rounded-full px-3 py-1" :class="i < index ? 'bg-emerald-100 text-emerald-900' : i === index ? 'bg-primary-100 text-primary-900 font-semibold' : 'bg-neutral-100 text-neutral-500'">
        {{ phaseLabel[p] }}
      </li>
    </ol>
    <div v-if="progress && progress.total > 0">
      <UProgress :model-value="percent ?? 0" />
      <p class="mt-1 text-sm text-neutral-600">{{ progress.message }} — {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>
    <p v-if="status" class="text-sm">وضعیت: <UBadge :color="status === 'DONE' ? 'success' : status === 'FAILED' ? 'error' : 'neutral'">{{ statusLabel[status] ?? status }}</UBadge></p>
    <ul class="max-h-64 space-y-1 overflow-auto rounded-lg border border-neutral-200 p-2 text-sm dark:border-neutral-800">
      <li v-for="(e, i) in events" :key="i" :class="e.level === 'ERROR' ? 'text-red-700' : e.level === 'WARN' ? 'text-amber-700' : 'text-neutral-700'">
        <span class="font-mono text-xs">[{{ phaseLabel[e.phase] ?? e.phase }}]</span> {{ e.message }}
      </li>
    </ul>
  </div>
</template>
```

Nuxt auto-imports `bridge`, `onJobEvents`, `useJob` (composables) and every export of `app/utils/fa.ts` (utils) — no explicit imports needed in components.

- [ ] **Step 7: Gates**

Run: `pnpm test && pnpm typecheck && pnpm generate`
Expected: all green. If `nuxt typecheck` complains that `bridge` is not auto-imported inside `useJob.ts`, add `import { bridge, onJobEvents } from './useBridge';` at its top.

- [ ] **Step 8: Commit**

```bash
git add app nuxt.config.ts
git commit -m "Add the UI foundation: bridge composable, Persian utils, layout

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 14: Settings and dashboard screens

Implements spec §10 screens 1 and 2.

**Files:**
- Create: `app/pages/settings.vue`
- Modify: `app/pages/index.vue` (replace the placeholder)

- [ ] **Step 1: Settings page** — `app/pages/settings.vue`

```vue
<script setup lang="ts">
import type { BridgeError, LandauStatus, SettingsView } from '~/types/bridge';

const toast = useToast();
const view = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const landau = ref<LandauStatus | null>(null);
const busy = ref<'landau' | 'supplier' | 'save' | null>(null);

const form = reactive({
  landau_url: '', landau_token: '', store: '', supplier_user: '', supplier_password: '',
  default_authenticity: 'ORIGINAL' as 'ORIGINAL' | 'FAKE', threshold: 80, vat_percent: 10, overwrite_manual: false,
});

const load = async () => {
  view.value = await bridge.settingsGet();
  Object.assign(form, {
    landau_url: view.value.landau_url, store: view.value.store ?? '', supplier_user: view.value.supplier_user ?? '',
    default_authenticity: view.value.default_authenticity, threshold: view.value.threshold, vat_percent: view.value.vat_percent,
    overwrite_manual: view.value.overwrite_manual, landau_token: '', supplier_password: '',
  });
};
onMounted(load);

const save = async () => {
  error.value = null; busy.value = 'save';
  try {
    const storeName = landau.value?.stores.find((s) => s.id === form.store)?.name ?? view.value?.store_name ?? undefined;
    view.value = await bridge.settingsSave({ ...form, store_name: storeName ?? undefined });
    form.landau_token = ''; form.supplier_password = '';
    toast.add({ title: 'تنظیمات ذخیره شد', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const validateLandau = async () => {
  error.value = null; busy.value = 'landau';
  try {
    // save url+key first so the check uses what is typed
    await bridge.settingsSave({ landau_url: form.landau_url, landau_token: form.landau_token || undefined });
    form.landau_token = '';
    landau.value = await bridge.landauValidate();
    if (!form.store && landau.value.stores.length === 1) form.store = landau.value.stores[0]!.id;
    toast.add({ title: `اتصال به لاندا برقرار است — ${landau.value.business_name ?? landau.value.business}`, color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const testSupplier = async () => {
  error.value = null; busy.value = 'supplier';
  try {
    await bridge.settingsSave({ supplier_user: form.supplier_user, supplier_password: form.supplier_password || undefined });
    form.supplier_password = '';
    await bridge.supplierTestLogin();
    toast.add({ title: 'ورود به بارزپخش موفق بود', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const storeItems = computed(() => (landau.value?.stores ?? []).map((s) => ({ label: s.name ?? s.id, value: s.id })));
const authItems = [{ label: 'اصلی (ORIGINAL)', value: 'ORIGINAL' }, { label: 'غیراصلی / کپی (FAKE)', value: 'FAKE' }];
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-6">
    <h2 class="text-2xl font-bold">تنظیمات</h2>
    <ErrorBanner :error="error" />

    <UCard>
      <template #header><h3 class="font-semibold">لاندا</h3></template>
      <div class="space-y-4">
        <UFormField label="نشانی درگاه لاندا" hint="مثال: https://api.landau.ir">
          <UInput v-model="form.landau_url" dir="ltr" class="w-full" />
        </UFormField>
        <UFormField label="کلید API لاندا" :hint="view?.has_landau_token ? 'کلیدی ذخیره شده است؛ برای تغییر، کلید تازه را وارد کنید' : 'در لاندا: حساب کاربری ← کلیدهای API'">
          <UInput v-model="form.landau_token" type="password" dir="ltr" placeholder="apt-…" class="w-full" />
        </UFormField>
        <UButton :loading="busy === 'landau'" icon="i-lucide-plug-zap" @click="validateLandau">بررسی اتصال و دریافت فروشگاه‌ها</UButton>
        <UAlert v-if="landau" color="info" variant="soft" :title="`کسب‌وکار: ${landau.business_name ?? landau.business}`"
          :description="landau.active_rules === 0 ? 'هیچ قاعده قیمت‌گذاری فعالی ندارید؛ تا زمانی که در لاندا قاعده‌ای بسازید، قیمت قفسه برابر قیمت خرید خواهد بود.' : `${faNumber(landau.active_rules)} قاعده قیمت‌گذاری فعال`" />
        <UFormField label="فروشگاه مقصد" hint="ردیف‌ها روی قفسه این فروشگاه نوشته می‌شوند">
          <USelect v-model="form.store" :items="storeItems" :disabled="!storeItems.length" placeholder="ابتدا اتصال را بررسی کنید" class="w-full" />
          <p v-if="!storeItems.length && view?.store_name" class="mt-1 text-sm text-neutral-500">فروشگاه ذخیره‌شده: {{ view.store_name }}</p>
        </UFormField>
      </div>
    </UCard>

    <UCard>
      <template #header><h3 class="font-semibold">بارزپخش</h3></template>
      <div class="space-y-4">
        <UFormField label="نام کاربری"><UInput v-model="form.supplier_user" dir="ltr" class="w-full" /></UFormField>
        <UFormField label="کلمه عبور" :hint="view?.has_supplier_password ? 'ذخیره شده؛ برای تغییر، مقدار تازه را وارد کنید' : ''">
          <UInput v-model="form.supplier_password" type="password" dir="ltr" class="w-full" />
        </UFormField>
        <UButton :loading="busy === 'supplier'" icon="i-lucide-log-in" @click="testSupplier">آزمایش ورود</UButton>
      </div>
    </UCard>

    <UCard>
      <template #header><h3 class="font-semibold">قواعد وارد کردن</h3></template>
      <div class="space-y-4">
        <UFormField label="اصالت پیش‌فرض ردیف‌های تازه" hint="وضعیت همیشه «نو» ثبت می‌شود؛ اصالت را می‌توانید برای هر قلم در مرحله بازبینی تغییر دهید">
          <USelect v-model="form.default_authenticity" :items="authItems" class="w-full" />
        </UFormField>
        <UFormField label="آستانه تطبیق خودکار (۰ تا ۱۰۰)" hint="مقدار پیش‌فرض با آزمایش روی داده واقعی انتخاب شده است (docs/matching-threshold.md)؛ بالاتر = محتاط‌تر">
          <UInput v-model.number="form.threshold" type="number" min="0" max="100" dir="ltr" class="w-32" />
        </UFormField>
        <UFormField label="نرخ ارزش افزوده (٪)" hint="قیمت خرید = قیمت فهرست بارزپخش × (۱ + نرخ)">
          <UInput v-model.number="form.vat_percent" type="number" min="0" max="100" step="0.5" dir="ltr" class="w-32" />
        </UFormField>
        <UFormField label="قیمت‌های دستی لاندا را هم بازنویسی کن" hint="لاندا قیمتی را که خودتان تایپ کرده باشید نگه می‌دارد؛ این گزینه همان «بازنویسی» لانداست">
          <USwitch v-model="form.overwrite_manual" />
        </UFormField>
      </div>
    </UCard>

    <UCard>
      <template #header><h3 class="font-semibold">داده‌ها</h3></template>
      <p class="text-sm">پوشه داده: <code dir="ltr">{{ view?.data_dir }}</code></p>
      <p class="mt-2 text-sm" :class="view?.secret_backend === 'file' ? 'text-amber-700' : 'text-neutral-600'">
        {{ view?.secret_backend === 'keyring' ? 'رمزها در ذخیره‌ساز امن سیستم‌عامل نگه داشته می‌شوند.' : 'ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده‌ای با دسترسی محدود در پوشه داده نگه داشته می‌شوند.' }}
      </p>
    </UCard>

    <div class="flex justify-end">
      <UButton size="lg" :loading="busy === 'save'" icon="i-lucide-save" @click="save">ذخیره تنظیمات</UButton>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Dashboard** — `app/pages/index.vue`

```vue
<script setup lang="ts">
import type { BridgeError, Dashboard } from '~/types/bridge';

const dash = ref<Dashboard | null>(null);
const error = ref<BridgeError | null>(null);
const load = async () => { try { dash.value = await bridge.dashboardGet(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">داشبورد</h2>
    <ErrorBanner :error="error" />
    <UAlert v-if="dash && !dash.settings_complete" color="warning" variant="soft" title="تنظیمات ناقص است" description="کلید لاندا، فروشگاه مقصد و ورود بارزپخش را در صفحه تنظیمات کامل کنید.">
      <template #actions><UButton to="/settings" size="sm">تنظیمات</UButton></template>
    </UAlert>

    <div v-if="dash" class="grid gap-4 md:grid-cols-3">
      <UCard><p class="text-sm text-neutral-500">فروشگاه لاندا</p><p class="text-lg font-semibold">{{ dash.store_name ?? '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">حساب بارزپخش</p><p class="text-lg font-semibold" dir="ltr">{{ dash.supplier_user ?? '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">فاکتورهای ذخیره‌شده</p><p class="text-lg font-semibold">{{ faNumber(dash.invoices_cached) }} <span class="text-sm text-neutral-500">({{ faNumber(dash.invoices_unpaid) }} پرداخت‌نشده)</span></p></UCard>
      <UCard><p class="text-sm text-neutral-500">آخرین وارد کردن</p><p class="font-semibold">{{ dash.last_import ? `${isoToLocal(dash.last_import.started_at)} — ${statusLabel[dash.last_import.status] ?? dash.last_import.status}` : '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">آخرین به‌روزرسانی قیمت</p><p class="font-semibold">{{ dash.last_refresh ? `${isoToLocal(dash.last_refresh.started_at)} — ${statusLabel[dash.last_refresh.status] ?? dash.last_refresh.status}` : '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">تطبیق‌های ثبت‌شده</p><p class="text-lg font-semibold">{{ faNumber(dash.decided_matches) }}</p></UCard>
    </div>

    <UAlert v-if="dash?.running?.status === 'REVIEW'" color="info" variant="soft" :title="`${faNumber(dash.pending_review)} قلم منتظر تصمیم شماست`">
      <template #actions><UButton :to="`/import?job=${dash.running.id}`" size="sm">ادامه بازبینی</UButton></template>
    </UAlert>
    <UAlert v-if="dash && dash.recent_errors > 0" color="warning" variant="soft" :title="`${faNumber(dash.recent_errors)} خطا یا هشدار در هفت روز گذشته`">
      <template #actions><UButton to="/logs" size="sm" variant="soft">مشاهده</UButton></template>
    </UAlert>

    <div class="flex flex-wrap gap-4">
      <UButton size="xl" icon="i-lucide-file-input" to="/import" :disabled="!dash?.settings_complete || !!dash?.running">وارد کردن فاکتورها</UButton>
      <UButton size="xl" icon="i-lucide-refresh-cw" variant="soft" to="/refresh" :disabled="!dash?.settings_complete || !!dash?.running">به‌روزرسانی قیمت‌ها</UButton>
    </div>
  </div>
</template>
```

- [ ] **Step 3: Gates and a manual pass**

Run: `pnpm typecheck && pnpm generate`
Expected: green.

Run: `pnpm tauri dev` → تنظیمات: paste the Landau URL and a minted key, «بررسی اتصال» lists the fixture store; pick it; enter the Barezpakhsh user/password, «آزمایش ورود» succeeds; «ذخیره». Back on the داشبورد the warning is gone and both action buttons are enabled.

- [ ] **Step 4: Commit**

```bash
git add app/pages/settings.vue app/pages/index.vue
git commit -m "Add the settings and dashboard screens

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 15: Import screen — mode, progress, review, report

Implements spec §10 screen 3 and the REVIEW pause of §7.1.

**Files:**
- Create: `app/pages/import.vue`, `app/components/ReviewTable.vue`, `app/components/ReportTable.vue`

- [ ] **Step 1: ReviewTable** — `app/components/ReviewTable.vue`

```vue
<script setup lang="ts">
import type { Authenticity, BridgeError, JobItemRow, ReviewDecision, Scored } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; defaultAuthenticity: Authenticity }>();
const emit = defineEmits<{ decided: [item_id: number] }>();

const error = ref<BridgeError | null>(null);
const searching = ref<number | null>(null);
const extra = reactive<Record<number, Scored[]>>({});
const terms = reactive<Record<number, string>>({});
const auth = reactive<Record<number, Authenticity>>({});

const candidatesOf = (item: JobItemRow): Scored[] => extra[item.id] ?? item.candidates ?? [];

const decide = async (item: JobItemRow, d: Omit<ReviewDecision, 'item_id'>) => {
  error.value = null;
  try {
    await bridge.reviewDecide({ item_id: item.id, authenticity: auth[item.id] ?? props.defaultAuthenticity, ...d });
    emit('decided', item.id);
  } catch (e) { error.value = e as BridgeError; }
};

const research = async (item: JobItemRow) => {
  const term = (terms[item.id] ?? '').trim();
  if (!term) return;
  searching.value = item.id;
  try { extra[item.id] = await bridge.reviewSearch(term); } catch (e) { error.value = e as BridgeError; } finally { searching.value = null; }
};
</script>

<template>
  <div class="space-y-4">
    <ErrorBanner :error="error" />
    <p class="text-sm text-neutral-600">برای هر قلم، قطعه درست را از فهرست لاندا انتخاب کنید. اگر هیچ‌کدام درست نیست، «هیچ‌کدام» را بزنید — این قلم دیگر پرسیده نمی‌شود و باید برای افزودن آن یک تیکت برای پشتیبانی لاندا ثبت کنید. «بعداً» آن را برای اجرای بعدی نگه می‌دارد.</p>
    <UCard v-for="item in items" :key="item.id">
      <template #header>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <p class="font-semibold">{{ item.name }}</p>
            <p class="text-xs text-neutral-500" dir="ltr">{{ item.code }} · تعداد {{ faNumber(item.qty) }}</p>
          </div>
          <div class="flex items-center gap-2">
            <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
            <UButton size="sm" color="neutral" variant="soft" @click="decide(item, { later: true })">بعداً</UButton>
            <UButton size="sm" color="error" variant="soft" @click="decide(item, { none: true })">هیچ‌کدام</UButton>
          </div>
        </div>
      </template>
      <div class="space-y-2">
        <div class="flex gap-2">
          <UInput v-model="terms[item.id]" placeholder="جست‌وجوی دوباره در لاندا با عبارت دیگر" class="flex-1" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" variant="soft" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-neutral-500">لاندا پیشنهادی نداشت. عبارت دیگری را امتحان کنید یا «هیچ‌کدام» را بزنید.</p>
        <ul class="divide-y divide-neutral-200 dark:divide-neutral-800">
          <li v-for="c in candidatesOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2">
            <div>
              <p>{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-neutral-500">— {{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-neutral-500">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex items-center gap-2">
              <UBadge :color="c.similarity >= 80 ? 'success' : c.similarity >= 60 ? 'warning' : 'neutral'" variant="soft">{{ faNumber(c.similarity) }}٪</UBadge>
              <UButton size="sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">همین است</UButton>
            </div>
          </li>
        </ul>
      </div>
    </UCard>
  </div>
</template>
```

- [ ] **Step 2: ReportTable** — `app/components/ReportTable.vue`

```vue
<script setup lang="ts">
import type { JobItemRow } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }>();
const filter = ref<'ALL' | 'DONE' | 'SKIPPED' | 'FAILED'>('ALL');
const rows = computed(() => props.items.filter((i) => filter.value === 'ALL' || i.status === filter.value));
const actionLabel: Record<string, string> = { ATTACH: 'افزوده شد', PATCH: 'به‌روزرسانی شد', SKIP: 'بدون تغییر' };

const csv = () => {
  const head = ['code', 'name', 'action', 'qty_before', 'qty_after', 'cost', 'price_before', 'price_after', 'verdict', 'status', 'error'];
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const lines = [head.join(','), ...props.items.map((i) => [i.code, i.name, i.action, i.qty_before, i.qty_after, i.cost, i.price_before, i.price_after, i.reprice_verdict, i.status, i.error_msg].map(esc).join(','))];
  const blob = new Blob(['﻿' + lines.join('\n')], { type: 'text/csv;charset=utf-8' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = `landau-bridge-${props.kind.toLowerCase()}.csv`;
  a.click();
  URL.revokeObjectURL(a.href);
};
</script>

<template>
  <div class="space-y-3">
    <div class="flex flex-wrap items-center gap-2">
      <UButton v-for="f in (['ALL', 'DONE', 'SKIPPED', 'FAILED'] as const)" :key="f" size="sm" :variant="filter === f ? 'solid' : 'soft'" @click="filter = f">
        {{ { ALL: 'همه', DONE: 'انجام‌شده', SKIPPED: 'ردشده', FAILED: 'ناموفق' }[f] }}
      </UButton>
      <span class="ms-auto text-sm text-neutral-500">{{ faNumber(rows.length) }} ردیف</span>
      <UButton size="sm" variant="soft" icon="i-lucide-download" @click="csv">خروجی CSV</UButton>
    </div>
    <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900">
          <tr class="text-start">
            <th class="p-2 text-start">قطعه</th>
            <th class="p-2">اقدام</th>
            <th class="p-2">تعداد</th>
            <th class="p-2">قیمت خرید</th>
            <th class="p-2">قیمت قفسه</th>
            <th class="p-2">نتیجه قیمت‌گذاری</th>
            <th class="p-2">وضعیت</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="i in rows" :key="i.id" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2"><p>{{ i.product_name ?? i.name }}</p><p class="text-xs text-neutral-500">{{ i.name }} <span dir="ltr">{{ i.code }}</span></p></td>
            <td class="p-2 text-center">{{ i.action ? actionLabel[i.action] : '—' }}</td>
            <td class="p-2 text-center" dir="ltr">{{ i.qty_before != null ? `${faNumber(i.qty_before)} → ` : '' }}{{ faNumber(i.qty_after ?? i.qty) }}</td>
            <td class="p-2 text-center">{{ rials(i.cost) }}</td>
            <td class="p-2 text-center" dir="ltr">{{ i.price_before != null ? `${faNumber(i.price_before)} → ` : '' }}{{ faNumber(i.price_after) }}</td>
            <td class="p-2 text-center">{{ verdictLabel(i.reprice_verdict) }}</td>
            <td class="p-2 text-center">
              <UBadge :color="i.status === 'DONE' ? 'success' : i.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[i.status] ?? i.status }}</UBadge>
              <p v-if="i.error_msg" class="mt-1 text-xs text-red-700">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
```

- [ ] **Step 3: Import page** — `app/pages/import.vue`

```vue
<script setup lang="ts">
import type { BridgeError, JobItemRow, SettingsView } from '~/types/bridge';

const route = useRoute();
const toast = useToast();
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const mode = ref<'QUANTITY' | 'ZERO'>('QUANTITY');
const since = ref(jalaliYearStart());
const starting = ref(false);
const review = ref<JobItemRow[]>([]);
const job = useJob();

const sinceOk = computed(() => normalizeJalali(since.value) !== null);

const loadReview = async () => {
  if (job.jobId.value == null) return;
  review.value = await bridge.importReview(job.jobId.value);
};

onMounted(async () => {
  settings.value = await bridge.settingsGet();
  const fromQuery = Number(route.query.job);
  const current = Number.isFinite(fromQuery) && fromQuery > 0 ? { id: fromQuery } : await bridge.jobCurrent();
  if (current) {
    await job.follow(current.id);
    if (job.status.value === 'REVIEW') await loadReview();
  }
});

watch(job.status, async (s) => { if (s === 'REVIEW') await loadReview(); });

const start = async () => {
  error.value = null; starting.value = true;
  try {
    const id = await bridge.importStart(mode.value, mode.value === 'QUANTITY' ? normalizeJalali(since.value)! : undefined);
    await job.follow(id);
  } catch (e) { error.value = e as BridgeError; } finally { starting.value = false; }
};

const decided = async (item_id: number) => {
  review.value = review.value.filter((i) => i.id !== item_id);
};

const cont = async () => {
  if (job.jobId.value == null) return;
  error.value = null;
  try {
    await bridge.importContinue(job.jobId.value);
    review.value = [];
    await job.reload();
  } catch (e) { error.value = e as BridgeError; }
};

const cancel = async () => { if (job.jobId.value != null) { await bridge.jobCancel(job.jobId.value); toast.add({ title: 'درخواست توقف ثبت شد', color: 'neutral' }); } };

const active = computed(() => job.status.value === 'RUNNING');
const finished = computed(() => job.status.value && ['DONE', 'FAILED', 'CANCELLED', 'INTERRUPTED'].includes(job.status.value));
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">وارد کردن فاکتورهای بارزپخش</h2>
    <ErrorBanner :error="error" />

    <UCard v-if="!job.jobId.value || finished">
      <template #header><h3 class="font-semibold">انتخاب روش</h3></template>
      <div class="space-y-4">
        <label class="flex cursor-pointer items-start gap-3 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800" :class="mode === 'QUANTITY' && 'ring-2 ring-primary-500'">
          <input v-model="mode" type="radio" value="QUANTITY" class="mt-1" />
          <div>
            <p class="font-semibold">با تعداد، از تاریخ</p>
            <p class="text-sm text-neutral-600">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد (منهای مرجوعی‌ها) به قفسه اضافه می‌شود؛ اگر قطعه از قبل روی قفسه باشد، به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</p>
            <UInput v-model="since" placeholder="۱۴۰۳/۰۱/۰۱" dir="ltr" class="mt-2 w-40" :color="sinceOk ? 'primary' : 'error'" />
          </div>
        </label>
        <label class="flex cursor-pointer items-start gap-3 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800" :class="mode === 'ZERO' && 'ring-2 ring-primary-500'">
          <input v-model="mode" type="radio" value="ZERO" class="mt-1" />
          <div>
            <p class="font-semibold">با تعداد صفر، برای بازبینی</p>
            <p class="text-sm text-neutral-600">همه اقلام همه فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</p>
          </div>
        </label>
        <p class="text-sm text-neutral-600">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لاندا از روی قیمت روز بارزپخش (با ارزش افزوده) محاسبه می‌شود، نه قیمت روی فاکتور.</p>
        <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع</UButton>
      </div>
    </UCard>

    <UCard v-if="job.jobId.value">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold">عملیات شماره {{ faNumber(job.jobId.value) }}</h3>
          <UButton v-if="active" size="sm" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
        </div>
      </template>
      <JobProgress :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
    </UCard>

    <UCard v-if="job.status.value === 'REVIEW'">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold">بازبینی اقلام تطبیق‌نشده ({{ faNumber(review.length) }})</h3>
          <UButton icon="i-lucide-arrow-left" @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
        </div>
      </template>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </UCard>

    <UCard v-if="finished && job.detail.value">
      <template #header><h3 class="font-semibold">گزارش</h3></template>
      <ReportTable :items="job.detail.value.items" kind="IMPORT" />
    </UCard>
  </div>
</template>
```

- [ ] **Step 4: Gates and a manual pass**

Run: `pnpm typecheck && pnpm generate`
Expected: green.

Run: `pnpm tauri dev` → وارد کردن → «با تعداد صفر» → شروع. Watch the phases advance; when it pauses at بازبینی, decide a few items (one «همین است», one «هیچ‌کدام», one «بعداً»), press ادامه, and read the report. Check in Landau's app that the shelf rows appeared with `cost` and a rule-derived price.

- [ ] **Step 5: Commit**

```bash
git add app/pages/import.vue app/components/ReviewTable.vue app/components/ReportTable.vue
git commit -m "Add the import screen with review and report

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 16: Price refresh screen

Implements spec §10 screen 4 and §7.2's options.

**Files:**
- Create: `app/pages/refresh.vue`

- [ ] **Step 1: Page**

```vue
<script setup lang="ts">
import type { BridgeError, SettingsView } from '~/types/bridge';

const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const starting = ref(false);
const overwrite = ref(false);
const job = useJob();

onMounted(async () => {
  settings.value = await bridge.settingsGet();
  overwrite.value = settings.value.overwrite_manual;
  const current = await bridge.jobCurrent();
  if (current?.kind === 'REFRESH') await job.follow(current.id);
});

const start = async () => {
  error.value = null; starting.value = true;
  try {
    if (settings.value && overwrite.value !== settings.value.overwrite_manual) {
      settings.value = await bridge.settingsSave({ overwrite_manual: overwrite.value });
    }
    const id = await bridge.refreshStart();
    await job.follow(id);
  } catch (e) { error.value = e as BridgeError; } finally { starting.value = false; }
};

const cancel = async () => { if (job.jobId.value != null) await bridge.jobCancel(job.jobId.value); };
const active = computed(() => job.status.value === 'RUNNING');
const finished = computed(() => job.status.value && ['DONE', 'FAILED', 'CANCELLED', 'INTERRUPTED'].includes(job.status.value));
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">به‌روزرسانی قیمت‌های قفسه</h2>
    <ErrorBanner :error="error" />

    <UCard v-if="!job.jobId.value || finished">
      <div class="space-y-4">
        <p class="text-sm text-neutral-600">برای هر ردیف قفسه که به یک کد بارزپخش وصل است، قیمت روز بارزپخش (با ارزش افزوده) به عنوان قیمت خرید ثبت می‌شود و سپس قواعد قیمت‌گذاری لاندا روی کل فروشگاه اجرا می‌شود. ردیف‌هایی که به بارزپخش وصل نیستند در گزارش فهرست می‌شوند و دست نمی‌خورند.</p>
        <UFormField label="قیمت‌های دستی لاندا را هم بازنویسی کن" hint="لاندا قیمتی را که خودتان در پنل تایپ کرده باشید حفظ می‌کند مگر این گزینه روشن باشد">
          <USwitch v-model="overwrite" />
        </UFormField>
        <UButton size="lg" icon="i-lucide-refresh-cw" :loading="starting" @click="start">شروع به‌روزرسانی</UButton>
      </div>
    </UCard>

    <UCard v-if="job.jobId.value">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold">عملیات شماره {{ faNumber(job.jobId.value) }}</h3>
          <UButton v-if="active" size="sm" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
        </div>
      </template>
      <JobProgress :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
    </UCard>

    <UCard v-if="finished && job.detail.value">
      <template #header><h3 class="font-semibold">گزارش</h3></template>
      <ReportTable :items="job.detail.value.items" kind="REFRESH" />
    </UCard>
  </div>
</template>
```

- [ ] **Step 2: Gates and a manual pass**

Run: `pnpm typecheck && pnpm generate` → green. `pnpm tauri dev` → به‌روزرسانی → شروع; after the import of Task 15 the linked rows show old → new cost and the Landau verdict.

- [ ] **Step 3: Commit**

```bash
git add app/pages/refresh.vue
git commit -m "Add the price refresh screen

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 17: Matches screen

Implements spec §10 screen 5 and §6's "never asked again, but the seller can change it".

**Files:**
- Create: `app/pages/matches.vue`

- [ ] **Step 1: Page**

```vue
<script setup lang="ts">
import type { BridgeError, MatchRow, Scored } from '~/types/bridge';

const rows = ref<MatchRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'AUTO' | 'USER' | 'NONE'>('ALL');
const query = ref('');
const editing = ref<MatchRow | null>(null);
const term = ref('');
const candidates = ref<Scored[]>([]);
const searching = ref(false);

const load = async () => { try { rows.value = await bridge.matchesList(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);

const shown = computed(() => rows.value.filter((r) => (filter.value === 'ALL' || r.how === filter.value) && (!query.value || r.name_seen.includes(query.value) || r.code.includes(query.value) || (r.product_name ?? '').includes(query.value))));
const howLabel: Record<string, string> = { AUTO: 'خودکار', USER: 'انتخاب شما', NONE: 'در لاندا نیست' };

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const pick = async (c: Scored) => {
  if (!editing.value) return;
  await bridge.matchSet(editing.value.code, c.candidate.id, c.candidate.name, editing.value.name_seen);
  editing.value = null; await load();
};
const setNone = async () => { if (editing.value) { await bridge.matchNone(editing.value.code, editing.value.name_seen); editing.value = null; await load(); } };
const clear = async () => { if (editing.value) { await bridge.matchClear(editing.value.code); editing.value = null; await load(); } };
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">تطبیق‌های ثبت‌شده</h2>
    <ErrorBanner :error="error" />
    <p class="text-sm text-neutral-600">هر کد بارزپخش یک بار به یک قطعه لاندا وصل می‌شود و دیگر پرسیده نمی‌شود. برای اقلامی که «در لاندا نیست» علامت خورده‌اند، یک تیکت برای پشتیبانی لاندا ثبت کنید تا قطعه اضافه شود؛ سپس اینجا تصمیم را تغییر دهید.</p>

    <div class="flex flex-wrap items-center gap-2">
      <UButton v-for="f in (['ALL', 'AUTO', 'USER', 'NONE'] as const)" :key="f" size="sm" :variant="filter === f ? 'solid' : 'soft'" @click="filter = f">
        {{ f === 'ALL' ? 'همه' : howLabel[f] }}
      </UButton>
      <UInput v-model="query" placeholder="جست‌وجو در نام یا کد" class="ms-auto w-64" />
    </div>

    <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2 text-start">نام در فاکتور</th><th class="p-2">کد</th><th class="p-2 text-start">قطعه لاندا</th><th class="p-2">نحوه</th><th class="p-2">شباهت</th><th class="p-2">تاریخ</th><th class="p-2"></th></tr></thead>
        <tbody>
          <tr v-for="r in shown" :key="r.code" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2">{{ r.name_seen }}</td>
            <td class="p-2 text-center" dir="ltr">{{ r.code }}</td>
            <td class="p-2">{{ r.product_name ?? '—' }}</td>
            <td class="p-2 text-center"><UBadge :color="r.how === 'NONE' ? 'error' : r.how === 'USER' ? 'info' : 'success'" variant="soft">{{ howLabel[r.how] }}</UBadge></td>
            <td class="p-2 text-center">{{ r.similarity != null ? `${faNumber(r.similarity)}٪` : '—' }}</td>
            <td class="p-2 text-center">{{ isoToLocal(r.decided_at) }}</td>
            <td class="p-2 text-center"><UButton size="xs" variant="soft" @click="openEdit(r)">تغییر</UButton></td>
          </tr>
        </tbody>
      </table>
    </div>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-3">
          <p class="text-sm">{{ editing.name_seen }} <span dir="ltr" class="text-neutral-500">{{ editing.code }}</span></p>
          <div class="flex gap-2">
            <UInput v-model="term" class="flex-1" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لاندا</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-neutral-200 overflow-auto dark:divide-neutral-800">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between py-2">
              <span>{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-neutral-500">— {{ c.candidate.brand }}</span></span>
              <span class="flex items-center gap-2"><UBadge variant="soft">{{ faNumber(c.similarity) }}٪</UBadge><UButton size="xs" @click="pick(c)">انتخاب</UButton></span>
            </li>
          </ul>
          <div class="flex justify-between">
            <UButton color="error" variant="soft" @click="setNone">در لاندا نیست (تیکت)</UButton>
            <UButton color="neutral" variant="soft" @click="clear">حذف تصمیم (دوباره پرسیده شود)</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
```

- [ ] **Step 2: Gates** — `pnpm typecheck && pnpm generate` green; in `pnpm tauri dev` change one AUTO decision to a different part and confirm the row updates.

- [ ] **Step 3: Commit**

```bash
git add app/pages/matches.vue
git commit -m "Add the matches screen

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 18: Logs and errors screen

Implements spec §10 screen 6 and §9 (every error visible, log folder reachable).

**Files:**
- Create: `app/pages/logs.vue`

- [ ] **Step 1: Page**

```vue
<script setup lang="ts">
import type { BridgeError, ErrorRow, JobDetail, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const errors = ref<ErrorRow[]>([]);
const detail = ref<JobDetail | null>(null);
const error = ref<BridgeError | null>(null);
const tab = ref<'jobs' | 'errors'>('jobs');

const load = async () => {
  try { [jobs.value, errors.value] = await Promise.all([bridge.jobsList(), bridge.errorsList(300)]); } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);
const open = async (j: JobRow) => { detail.value = await bridge.jobGet(j.id); };
const kindLabelOf = (j: JobRow) => (j.kind === 'IMPORT' ? `وارد کردن (${j.mode === 'ZERO' ? 'صفر' : `از ${j.since_date}`})` : 'به‌روزرسانی قیمت');
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <h2 class="text-2xl font-bold">گزارش و خطاها</h2>
      <UButton variant="soft" icon="i-lucide-folder-open" @click="bridge.openLogFolder()">باز کردن پوشه لاگ</UButton>
    </div>
    <ErrorBanner :error="error" />
    <div class="flex gap-2">
      <UButton size="sm" :variant="tab === 'jobs' ? 'solid' : 'soft'" @click="tab = 'jobs'">عملیات‌ها ({{ faNumber(jobs.length) }})</UButton>
      <UButton size="sm" :variant="tab === 'errors' ? 'solid' : 'soft'" @click="tab = 'errors'">خطاها و هشدارها ({{ faNumber(errors.length) }})</UButton>
      <UButton size="sm" variant="ghost" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
    </div>

    <div v-if="tab === 'jobs'" class="grid gap-6 lg:grid-cols-2">
      <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
        <table class="w-full text-sm">
          <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2">#</th><th class="p-2 text-start">نوع</th><th class="p-2">شروع</th><th class="p-2">وضعیت</th><th class="p-2">مرحله</th></tr></thead>
          <tbody>
            <tr v-for="j in jobs" :key="j.id" class="cursor-pointer border-t border-neutral-200 hover:bg-neutral-50 dark:border-neutral-800 dark:hover:bg-neutral-900" :class="detail?.job.id === j.id && 'bg-primary-50 dark:bg-primary-950'" @click="open(j)">
              <td class="p-2 text-center">{{ faNumber(j.id) }}</td>
              <td class="p-2">{{ kindLabelOf(j) }}</td>
              <td class="p-2 text-center">{{ isoToLocal(j.started_at) }}</td>
              <td class="p-2 text-center"><UBadge :color="j.status === 'DONE' ? 'success' : j.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[j.status] ?? j.status }}</UBadge></td>
              <td class="p-2 text-center">{{ phaseLabel[j.phase] ?? j.phase }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <UCard v-if="detail">
        <template #header><h3 class="font-semibold">عملیات {{ faNumber(detail.job.id) }} — رویدادها</h3></template>
        <pre v-if="detail.job.summary" dir="ltr" class="mb-3 overflow-auto rounded bg-neutral-100 p-2 text-xs dark:bg-neutral-900">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
        <ul class="max-h-96 space-y-1 overflow-auto text-sm">
          <li v-for="e in detail.events" :key="e.id" :class="e.level === 'ERROR' ? 'text-red-700' : e.level === 'WARN' ? 'text-amber-700' : ''">
            <span class="font-mono text-xs text-neutral-500">{{ isoToLocal(e.at) }}</span> [{{ phaseLabel[e.phase] ?? e.phase }}] {{ e.message }}
          </li>
        </ul>
        <div class="mt-4"><ReportTable :items="detail.items" :kind="detail.job.kind" /></div>
      </UCard>
    </div>

    <div v-else class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2">زمان</th><th class="p-2">عملیات</th><th class="p-2 text-start">نوع</th><th class="p-2 text-start">پیام</th></tr></thead>
        <tbody>
          <tr v-for="e in errors" :key="e.id" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2 text-center whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td class="p-2 text-center">{{ e.job_id != null ? faNumber(e.job_id) : '—' }}</td>
            <td class="p-2"><UBadge variant="soft" color="neutral">{{ kindLabel(e.kind) }}</UBadge></td>
            <td class="p-2">{{ e.message }}<details v-if="e.context" class="mt-1"><summary class="cursor-pointer text-xs text-neutral-500">جزئیات</summary><pre dir="ltr" class="text-xs">{{ JSON.stringify(e.context, null, 1) }}</pre></details></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Gates** — `pnpm typecheck && pnpm generate` green; in `pnpm tauri dev` the earlier jobs and their warnings (NoPricingRule, InvoiceUnpaid, …) are listed and «باز کردن پوشه لاگ» opens the folder holding `bridge.log.<date>`.

- [ ] **Step 3: Commit**

```bash
git add app/pages/logs.vue
git commit -m "Add the logs and errors screen

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---
### Task 19: Runbook, key-minting helper, live end-to-end test, README, release build

Implements spec §11 (live tests, runbook) and closes the loop: everything verified against the running Landau and the real Barezpakhsh, then a bundle is built.

**Files:**
- Create: `scripts/mint-apt.mjs`, `docs/RUNBOOK.md`, `README.md`, `src-tauri/tests/live_e2e.rs`

- [ ] **Step 1: Key-minting helper** — `scripts/mint-apt.mjs` (dev only; reads credentials from a path given in env, prints exports)

```js
#!/usr/bin/env node
// Mint an apt-… key for a Landau fixture account and print the env exports the live tests need.
// Env: LND_API_URL (default http://localhost:6150), LND_ROLES_FILE (JSON array of {role,user,pass}),
//      LND_ROLE (default retail-supplier). Nothing here is ever written to disk.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const API = process.env.LND_API_URL ?? 'http://localhost:6150';
const role = process.env.LND_ROLE ?? 'retail-supplier';
const roles = JSON.parse(readFileSync(process.env.LND_ROLES_FILE, 'utf8'));
const account = roles.find((r) => r.role === role);
if (!account) throw new Error(`no ${role} in ${process.env.LND_ROLES_FILE}`);

async function captcha() {
  const { algorithm, challenge, salt, signature, maxnumber = 1_000_000 } = await (await fetch(`${API}/challenge`)).json();
  for (let n = 0; n <= maxnumber; n++) {
    if (createHash('sha256').update(salt + n).digest('hex') === challenge) {
      return Buffer.from(JSON.stringify({ algorithm, challenge, number: n, salt, signature })).toString('base64');
    }
  }
  throw new Error('captcha unsolved');
}

const login = await fetch(`${API}/-/auth/token`, {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ grant_type: 'password', response_type: 'token', username: account.user, password: account.pass, domain: 'landau.ir', captcha: await captcha() }),
});
const { access_token } = await login.json();
if (!access_token) throw new Error(`login failed (${login.status})`);

const minted = await fetch(`${API}/-/auth/apts?zone=own`, {
  method: 'POST',
  headers: { 'content-type': 'application/json', authorization: `Bearer ${access_token}` },
  body: JSON.stringify({ name: `landau-bridge ${new Date().toISOString().slice(0, 10)}`, expires_at: Date.now() + 90 * 86_400_000 }),
});
const body = await minted.json();
const token = body?.data?.token ?? body?.token;
if (!token) throw new Error(`mint failed (${minted.status}): ${JSON.stringify(body).slice(0, 200)}`);

// a fresh key answers 401 until the CDC watcher lands it; wait for it, then read the stores
let stores = [];
for (let i = 0; i < 30; i++) {
  const res = await fetch(`${API}/career/staff/memberships`, { headers: { authorization: `Bearer ${token}` } });
  if (res.status === 200) {
    const items = (await res.json()).items ?? [];
    stores = items.flatMap((m) => m.stores ?? []);
    break;
  }
  await new Promise((r) => setTimeout(r, 1000));
}
console.log(`export LND_API_URL=${API}`);
console.log(`export LND_APT_TOKEN=${token}`);
if (stores[0]) console.log(`export LND_STORE=${stores[0].id}   # ${stores.map((s) => `${s.id}=${s.name ?? ''}`).join(', ')}`);
```

Run: `NO_PROXY='*' LND_ROLES_FILE=/home/vahid/Claude/.memory/.local/landau-roles.json node scripts/mint-apt.mjs`
Expected: three `export` lines. Paste them into the shell (`eval "$(…)"`).

- [ ] **Step 2: Runbook** — `docs/RUNBOOK.md`

```markdown
# Runbook — developing and live-testing Landau Bridge

## Prerequisites (this machine)
- cargo ≥ 1.80, node 22, pnpm 11, webkit2gtk-4.1 + libsoup3 dev packages (Linux), dbus (keyring).
- The Landau stack running: `docker ps` shows `lnd-gateway` on :6150. To (re)start it: `/home/vahid/Claude/.scripts/running.sh` from that repo (see its CLAUDE.md).
- The dev machine's HTTP proxy must be bypassed for both Barezpakhsh (geo-blocked exit) and the loopback gateway: `export NO_PROXY='*'` in the shell that runs tests. The app itself never uses a proxy.

## Fixtures
1. **Landau accounts** — after any database reset the deck accounts are gone; rebuild them (idempotent):
   `cd /home/vahid/Claude && set -a && . .memory/.local/landau-journeys.env && set +a && node .scripts/e2e/journeys.mjs landau stage`
   The retail-supplier fixture is `deck-retail-supplier-…@lnd.ir` with one RETAIL store.
2. **Registry** — the bridge matches against Landau's `career/products`. If `POST /-/career/products/search` answers no hits, re-import the parts (curator key needed):
   `cd /home/vahid/WorkSpace/landau-parts && ./run_import_parts_landau.sh preflight && ./run_import_parts_landau.sh run --workers 4`
3. **API key** — `LND_ROLES_FILE=/home/vahid/Claude/.memory/.local/landau-roles.json node scripts/mint-apt.mjs` → `eval` its output.
4. **Pricing rule** (optional, to see rule-derived prices rather than price = cost):
   `curl -s -X POST "$LND_API_URL/career/pricing-rules" -H "authorization: Bearer $LND_APT_TOKEN" -H 'content-type: application/json' -d '{"scope":"SHELF","basis":"MARKUP_PERCENT","value":15,"rounding":1000,"floor_at_cost":true,"active":true}'`
5. **Barezpakhsh credentials** — the seller's; for tests, the ones in the prior scraper's code:
   `export BAREZ_USER=$(grep -o '"USER": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4); export BAREZ_PASS=$(grep -o '"PASS": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4)`

## Tests
- Offline: `cd src-tauri && cargo test` · `pnpm test`
- Live: `cd src-tauri && cargo test --features live -- --nocapture` (skips with a message when env is missing)
- Calibration: `cd src-tauri && cargo run --bin calibrate --features live` → `docs/matching-threshold.md`

## Running the app
- Dev: `pnpm tauri dev` · Release bundle: `pnpm tauri build` → `src-tauri/target/release/bundle/`
- Data: `~/.local/share/ir.landau.bridge/` (Linux) — `bridge.sqlite`, `logs/bridge.log.<date>`, `secrets.json` only when no keyring.

## Facts worth knowing before debugging
- A fresh apt answers `401` for a few seconds until the CDC watcher lands it.
- Shelf reads lag writes by ~1 s (mirror); the jobs wait up to 15 s before repricing.
- `PATCH /career/stocks/:id` must never carry `price` — Landau stamps it MANUAL and its rules skip the row.
- Barezpakhsh's ledger has three row kinds: sales invoices (alias 2), returns (6), receipts (67). Only 2 and 6 are invoices.
```

- [ ] **Step 3: Live end-to-end test** — `src-tauri/tests/live_e2e.rs`

```rust
//! Whole-flow test against the real Barezpakhsh and the running Landau (feature `live`).
//! Env: LND_API_URL, LND_APT_TOKEN, LND_STORE, BAREZ_USER, BAREZ_PASS.
#![cfg(feature = "live")]

use landau_bridge_lib::db::Db;
use landau_bridge_lib::domain::ImportMode;
use landau_bridge_lib::jobs::import::{self, ImportParams, Outcome};
use landau_bridge_lib::jobs::{refresh, JobCtx, Services};
use landau_bridge_lib::landau::LandauClient;
use landau_bridge_lib::matching::Matcher;
use landau_bridge_lib::secrets::SecretStore;
use landau_bridge_lib::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use landau_bridge_lib::supplier;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[tokio::test]
async fn zero_import_then_refresh_round_trip() {
    let (Some(url), Some(token), Some(store), Some(user), Some(pass)) =
        (env("LND_API_URL"), env("LND_APT_TOKEN"), env("LND_STORE"), env("BAREZ_USER"), env("BAREZ_PASS"))
    else {
        eprintln!("skipped: live env not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("bridge.sqlite")).unwrap();
    let secrets = SecretStore::file_only(dir.path());
    Settings::save(&db, &secrets, &SettingsPatch {
        landau_url: Some(url.clone()), landau_token: Some(token.clone()), store: Some(store.clone()),
        supplier_user: Some(user.clone()), supplier_password: Some(pass.clone()), ..Default::default()
    }).unwrap();
    let settings = Settings::load(&db, &secrets).unwrap();
    let services = || Services {
        landau: LandauClient::new(&url, &token).unwrap(),
        supplier: supplier::open(SUPPLIER_ID, settings.require_supplier().unwrap()).unwrap(),
        settings: settings.clone(),
    };

    // ---- import, zero mode, three invoices
    let job_id = db.create_job("IMPORT", Some("ZERO"), None, &store, false).unwrap();
    let ctx = JobCtx { db: db.clone(), app: None, job_id, cancel: Default::default() };
    let svc = services();
    let outcome = import::start(&ctx, &svc, &ImportParams { mode: ImportMode::Zero, since: None, fetch_limit: Some(3) }).await.expect("import start");

    if let Outcome::Review(n) = outcome {
        eprintln!("{n} item(s) in review; accepting the top candidate of the first, deferring the rest");
        let matcher = Matcher::new(db.clone(), SUPPLIER_ID, settings.threshold);
        let mut first = true;
        for item in db.job_items(job_id).unwrap().into_iter().filter(|i| i.status == "REVIEW") {
            let top = item.candidates.as_ref().and_then(|c| c.as_array()).and_then(|a| a.first()).cloned();
            match (first, top) {
                (true, Some(top)) => {
                    let id = top["candidate"]["id"].as_str().unwrap().to_string();
                    let name = top["candidate"]["name"].as_str().unwrap().to_string();
                    matcher.record_user_pick(&item.code, &item.name, &id, &name, top["similarity"].as_i64()).unwrap();
                    db.update_job_item(item.id, &[("product_id", landau_bridge_lib::jobs::t(&id)), ("product_name", landau_bridge_lib::jobs::t(&name)), ("status", landau_bridge_lib::jobs::t("PLANNED"))]).unwrap();
                    first = false;
                }
                _ => {} // stays REVIEW → skipped by commit
            }
        }
        import::commit(&ctx, &svc).await.expect("import commit");
    }
    let detail = db.job_detail(job_id).unwrap();
    let done: Vec<_> = detail.items.iter().filter(|i| i.status == "DONE").collect();
    assert!(!done.is_empty(), "at least one row reached the shelf: {:?}", detail.items.iter().map(|i| (&i.status, &i.error_kind)).collect::<Vec<_>>());
    assert!(done.iter().all(|i| i.stock_id.is_some() && i.cost.is_some()));
    assert!(done.iter().all(|i| i.qty_after == Some(0) || i.action.as_deref() == Some("PATCH")), "zero mode");
    assert!(detail.events.iter().any(|e| e.phase == "REPRICE"));

    // ---- refresh
    let job_id = db.create_job("REFRESH", None, None, &store, false).unwrap();
    let ctx = JobCtx { db: db.clone(), app: None, job_id, cancel: Default::default() };
    let svc = services();
    refresh::run(&ctx, &svc).await.expect("refresh");
    let detail = db.job_detail(job_id).unwrap();
    assert!(detail.items.iter().any(|i| !i.code.is_empty() && i.status == "DONE"), "linked rows were refreshed");

    // ---- cleanup: soft-delete rows this test attached
    let landau = LandauClient::new(&url, &token).unwrap();
    for item in db.job_items(job_id - 1).unwrap().iter().filter(|i| i.action.as_deref() == Some("ATTACH")) {
        if let Some(id) = &item.stock_id {
            let _ = landau.delete(id).await;
        }
    }
}
```

Run: `cd src-tauri && NO_PROXY='*' cargo test --features live -- --nocapture --test-threads=1`
Expected: `live_supplier`, `live_landau` and `live_e2e` pass. The e2e run fetches the whole ledger once (~2 min) plus three invoices. Note the reprice verdicts in the output: with the runbook's 15 % rule, a `cost` of `1 811 700` prices to `2 083 000` (×1.15 = 2 083 455 → rounded to the 1 000 step); without a rule the verdict is `SKIPPED:NO_RULE` and price = cost.

- [ ] **Step 4: README** — `README.md`

```markdown
# پل لاندا (Landau Bridge)

برنامه‌ای برای فروشنده خرده‌فروش لاندا که دو کار می‌کند:

1. **وارد کردن فاکتورهای بارزپخش روی قفسه لاندا** — همه فاکتورهای پرداخت‌شده تا امروز خوانده می‌شود، هر قلم با جست‌وجوی فازی لاندا به قطعه درست وصل می‌شود (اقلام مبهم را خودتان تأیید می‌کنید و تصمیم شما ذخیره می‌ماند)، قیمت خرید = قیمت روز بارزپخش با ارزش افزوده، و قیمت قفسه با **قواعد قیمت‌گذاری خودتان در لاندا** محاسبه می‌شود. دو حالت: «با تعداد، از تاریخ» یا «با تعداد صفر برای بازبینی».
2. **به‌روزرسانی قیمت‌ها** — هر وقت بخواهید، قیمت خرید ردیف‌های قفسه از قیمت روز بارزپخش تازه می‌شود و قواعد لاندا دوباره اعمال می‌شود.

## راه‌اندازی
1. در لاندا: حساب کاربری ← کلیدهای API ← یک کلید بسازید (فقط یک بار نمایش داده می‌شود).
2. برنامه را باز کنید ← **تنظیمات**: نشانی درگاه لاندا و کلید را وارد کنید، «بررسی اتصال» را بزنید، فروشگاه مقصد را انتخاب کنید؛ نام کاربری و کلمه عبور بارزپخش را وارد و «آزمایش ورود» کنید؛ ذخیره.
3. اگر در لاندا قاعده قیمت‌گذاری ندارید، ابتدا یکی بسازید — وگرنه قیمت قفسه برابر قیمت خرید خواهد بود (برنامه هشدار می‌دهد).

## نکته‌ها
- قیمت‌هایی که خودتان در لاندا دستی تایپ کرده باشید حفظ می‌شود، مگر گزینه «بازنویسی قیمت‌های دستی» را روشن کنید.
- هر فاکتور فقط یک بار به موجودی اضافه می‌شود؛ اجرای دوباره امن است.
- قطعه‌ای که در فهرست لاندا نیست: «هیچ‌کدام» را بزنید و برای افزودن آن تیکتی برای پشتیبانی لاندا ثبت کنید.
- همه خطاها و هشدارها در صفحه «گزارش و خطاها» و در پوشه لاگ ثبت می‌شوند.

---

## For developers
Tauri 2 + Nuxt 4 + Rust core. Design: `docs/superpowers/specs/2026-09-13-landau-bridge-design.md`. Plan: `docs/superpowers/plans/2026-09-13-landau-bridge.md`. Live testing and fixtures: `docs/RUNBOOK.md`. Matching threshold and why: `docs/matching-threshold.md`.

```sh
pnpm install
cd src-tauri && cargo test && cd ..          # offline
pnpm test && pnpm typecheck                   # UI utils, types
pnpm tauri dev                                # run
pnpm tauri build                              # bundle → src-tauri/target/release/bundle/
```
```

- [ ] **Step 5: Full gates and the release bundle**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets --features live -- -D warnings && cargo test && cd ..
pnpm test && pnpm typecheck && pnpm generate
pnpm tauri build
ls src-tauri/target/release/bundle/*/
```
Expected: every gate green; the bundle directory holds a `.deb`/`.AppImage` (Linux). Launch the AppImage once: the window opens on the dashboard.

- [ ] **Step 6: Commit**

```bash
git add scripts/mint-apt.mjs docs/RUNBOOK.md README.md src-tauri/tests/live_e2e.rs
git commit -m "Add the runbook, key helper, live end-to-end test and README

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

## Self-review against the spec

| Spec | Task |
| --- | --- |
| §1 decisions (cost ×1.10 as a VAT setting, unpaid skipped, Persian RTL, NEW + default authenticity, no rule → price = cost, manual prices respected, qty as-is, no price → skipped) | 2 (`cost_of`), 11 (`aggregate`, `cost_for`, `plan_write`, `AttachBody`), 13–15 (UI copy) |
| §2.1 Landau routes and traps | 8 (`LandauClient`), Global Constraints (never `price` on PATCH) |
| §2.2 Barezpakhsh mechanics | 6 (parsers over fixtures), 7 (session) |
| §3 architecture, config, secrets, proxy bypass | 1, 4, 7/8 (`no_proxy`), 12 |
| §4 supplier module | 6, 7 |
| §5 Landau client incl. `await_visible`, reprice paging | 8 |
| §6 matching, calibration, decisions, ticket text | 5, 9, 10, 15, 17 |
| §7.1 import phases, review pause, 409 → patch, `invoice_imports` | 11 (`import.rs`), 12 (`import_start`/`import_continue`), 15 |
| §7.2 refresh, ambiguous links, unlinked rows | 11 (`refresh.rs`), 16 |
| §7.3 invariants (one job, cancel, INTERRUPTED on launch) | 11 (`JobRegistry`), 12 (`interrupt_stale_jobs`) |
| §8 schema | 3 |
| §9 error kinds, file log, errors table | 2, 11 (`JobCtx::warn/error`), 12 (tracing), 18 |
| §10 six screens | 13–18 |
| §11 tests, live tests, calibration, gates | every task's steps; 7, 8, 10, 19 |

Amendments discovered while pinning the site (both recorded in the plan and to be reflected in the spec by the executor of Task 19 as a short "Amendments" section): return invoices («مرجوع از فروش», DocAliasID 6) subtract quantities; the price search exposes the VAT rate per item, so the ×1.10 decision became a `vat_percent` setting with a `VatRateMismatch` warning when the site disagrees.
