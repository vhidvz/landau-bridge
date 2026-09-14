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
- The dev server runs on **port 3210**, not Nuxt's default 3000 (3000 is taken on this machine) — see `pnpm dev` (`nuxt dev --port 3210`) and `devUrl` in `src-tauri/tauri.conf.json`. `pnpm tauri dev` launches this automatically via `beforeDevCommand`; nothing to change by hand.
- Data: `~/.local/share/ir.landau.bridge/` (Linux) — `bridge.sqlite`, `logs/bridge.log.<date>`, `secrets.json` only when no keyring.
- If the app was run before a schema change, delete `bridge.sqlite*` in the data dir (`bridge.sqlite`, `-wal`, `-shm`) — the app recreates it on next launch. A database created after the final schema needs no such reset.

## Design review without Tauri
To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done` (`state` = `empty|running|review|refreshing|done`; fixtures in `app/dev/mockBridge.ts`; add `&grid=1` for the alignment overlay, `&fx=full` / `&fx=lite` force full or reduced effects (headless Chrome auto-detects as reduced), `&appimage=1` for the install offer); `scripts/shots.sh OUT_DIR [filter] [--grid]` screenshots every page.

## Facts worth knowing before debugging
- A fresh apt answers `401` for a few seconds until the CDC watcher lands it.
- Shelf reads lag writes by ~1 s (mirror); the jobs wait up to 15 s before repricing.
- `PATCH /career/stocks/:id` must never carry `price` — Landau stamps it MANUAL and its rules skip the row.
- Barezpakhsh's ledger has three row kinds: sales invoices (alias 2), returns (6), receipts (67). Only 2 and 6 are invoices.

## Deviations from the spec (as built)
- **Data folder is displayed, not configurable** (spec §3/§10 list it among the settings). The settings screen shows the platform data directory; moving it is not offered.
- **No "fetch everything again" control** (spec §4). Invoices are cached by number and kind and never refetched; to force a refetch, delete the invoice rows (or the whole `bridge.sqlite`) in the data folder.
- **The refresh report shows the new cost only** (spec §7.2 says old → new cost and price). `job_items.cost` holds today's cost; the old cost is not kept on the item, though the price before/after columns are.
- **`landau_validate` does not restrict to RETAIL stores** (spec §5). Every non-rejected store of the approved business is offered; the seller picks.
- **`price_of` is called once per shelf row, not once per code** (spec §7.2). Two rows linked to the same code cost two searches; the price snapshot is recorded each time.
- **Event and command names**: the job event is `job://event` (spec §3 says `job://phase`) and the detail command is `job_get` (spec §3 says `job_detail`); `job://progress` and `job://done` are as specified.
- **Return handling** (spec §13 says a net-negative item is clamped to 0 at aggregation): the net quantity stays signed across runs, the shelf is floored at zero at write time, and the `NegativeQuantity` warning names the shortfall — a return that arrives after the sale was transferred subtracts from the shelf.
- **Icons are bundled** and the app declares a CSP (`tauri.conf.json`); nothing is fetched from api.iconify.design at runtime. The CSP applies to the built app only — `pnpm tauri dev` loads the page from the Nuxt dev server, where Tauri cannot inject headers.
