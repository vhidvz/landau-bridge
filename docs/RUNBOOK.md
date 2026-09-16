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
4. **Pricing rule** (required since spec §14 e — a job without an active rule fails preflight; the fixture store already carries one):
   `curl -s -X POST "$LND_API_URL/career/pricing-rules" -H "authorization: Bearer $LND_APT_TOKEN" -H 'content-type: application/json' -d '{"scope":"SHELF","basis":"MARKUP_PERCENT","value":15,"rounding":1000,"floor_at_cost":true,"active":true}'`
5. **Barezpakhsh credentials** — the seller's; for tests, the ones in the prior scraper's code:
   `export BAREZ_USER=$(grep -o '"USER": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4); export BAREZ_PASS=$(grep -o '"PASS": "[^"]*"' /home/vahid/WorkSpace/barezpakhsh-scraper/stock.py | cut -d'"' -f4)`
6. **Name map** (spec §14 c) — in the app: Settings → «بارگذاری فایل نگاشت» → `/home/vahid/WorkSpace/landau-parts/output/parts_map.csv`. The refresh job costs unlinked shelf rows through it; without it they are reported «بدون پیوند».

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
To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done` (`state` = `empty|running|review|refreshing|done`; fixtures in `app/dev/mockBridge.ts`; add `&grid=1` for the alignment overlay, `&fx=full` / `&fx=lite` force full or reduced effects (headless Chrome auto-detects as reduced), `&appimage=1` for the install offer); `scripts/shots.sh OUT_DIR [filter] [--grid]` screenshots every page at 1200 × 800 (`SHOT_HEIGHT=1700 scripts/shots.sh …` for a taller viewport — `<main>` scrolls internally, so the default height captures only the top of a long page).

Those shots come from Chrome, which is **not** the engine the app ships on. Anything to do with text — Persian shaping, where a line sits in its box — can differ in WebKitGTK and be perfectly fine in Chrome (that is how Estedad shipped for a whole pass rendering Persian without its dots). To capture the same page in the engine Tauri actually uses:

```sh
GDK_BACKEND=x11 DISPLAY=:0 scripts/wk-shot.py URL OUT.png [WIDTH HEIGHT] [--settle MS]
GDK_BACKEND=x11 DISPLAY=:0 scripts/wk-shot.py \
  "http://localhost:3210/settings?state=done&fx=full" /tmp/settings-webkit.png 1200 800
```

It needs a display — Xwayland is fine; the window is mapped far off-screen, so nothing appears on a workspace. `webkit2gtk-4.1` you already have (the build needs it), but its **Python bindings are an extra this script alone wants** — they are not in the build prerequisites, and nothing else in the repo uses them. Install them only if you want WebKit stills:

| | |
| --- | --- |
| Arch / Manjaro | `sudo pacman -S --needed python-gobject` |
| Debian / Ubuntu | `sudo apt install python3-gi gir1.2-webkit2-4.1` |
| Fedora | `sudo dnf install python3-gobject webkit2gtk4.1` |

Check it with `python3 -c "import gi; gi.require_version('WebKit2','4.1')"`. By default it turns GTK animations off (WebKit reports that as `prefers-reduced-motion`, which is what `shots.sh` asks Chrome for) and waits for the DOM to stop changing before it shoots; `--nowait` and `--motion` turn those off. Put a font or alignment change in front of it before believing a Chrome still.

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
- **One name search per shelf row, not one per code** (spec §7.2, §14 b). Two rows linked to the same name cost two searches; the price snapshot is recorded each time.
- **Spec §14 supersedes §1/§7.2 on pricing**: no active rule fails preflight (`NoPricingRule`); `reprice` is Landau's targeted `{stocks}` form (plan 107, chunks of 50); the mirror is never awaited — `price_after` comes from the write reply and the reprice verdict.
- **Schema**: a `bridge.sqlite` from before 2026-09-16 (user_version 1) is migrated in place on launch; `matches` is re-keyed by name and old decisions are backfilled.
- **Event and command names**: the job event is `job://event` (spec §3 says `job://phase`) and the detail command is `job_get` (spec §3 says `job_detail`); `job://progress` and `job://done` are as specified.
- **Return handling** (spec §13 says a net-negative item is clamped to 0 at aggregation): the net quantity stays signed across runs, the shelf is floored at zero at write time, and the `NegativeQuantity` warning names the shortfall — a return that arrives after the sale was transferred subtracts from the shelf.
- **Icons are bundled** and the app declares a CSP (`tauri.conf.json`); nothing is fetched from api.iconify.design at runtime. The CSP applies to the built app only — `pnpm tauri dev` loads the page from the Nuxt dev server, where Tauri cannot inject headers.
