# Landau Bridge (پل لاندا)

A desktop app for a Landau retail seller. It does two things:

1. **Imports Barezpakhsh invoices onto your Landau shelf.** Every paid invoice up to today is read; each line is linked to a part in Landau's catalogue only on evidence — its part numbers and sizes, its words, its brand — and every link is checked against Landau's whole catalogue at the start of each run before anything is written from it. A line the evidence cannot settle is shown to you for a decision, with the evidence and the cost it would write, and your decision is remembered — by the part's Barezpakhsh *name*, since their codes change. A pick that contradicts the supplier's name (another part number, the other side, another brand) asks you to confirm it a second time, showing what conflicts and how the cost changes. Cost = Barezpakhsh's current price plus VAT, looked up by that name; the shelf price is set by **your own pricing rules in Landau** on the write itself, so a row is never on sale at cost. Two modes: *with quantities, from a date* or *with zero quantities, for review*.
2. **Refreshes prices.** Whenever you like, the cost of every shelf row is refreshed from Barezpakhsh's current price and Landau's rule prices it again. A price Barezpakhsh lists as «ناموجود» (out of stock) is applied only when it *raises* a cost — it is stale and will be relisted higher. A shelf row the app has no link for (a part never bought through it) is listed in the report as unlinked; it gets its link the first time that part arrives on an imported invoice.

The interface is in Persian (right-to-left, Persian numerals, Jalali dates).

## Contents

- [Install a release](#install-a-release)
- [First-time setup](#first-time-setup)
- [Good to know](#good-to-know)
- [Shelf audit](#shelf-audit)
- [Updates](#updates)
- [Build from source](#build-from-source)
- [For developers](#for-developers)

## Install a release

Download the file for your system from the latest release on [GitHub Releases](https://github.com/vhidvz/landau-bridge/releases/latest) (or [build it yourself](#build-from-source)); `SHA256SUMS` there lists each package's checksum (`sha256sum -c SHA256SUMS --ignore-missing`). The version number in the file names is the app's version; `1.0.0` is used in the examples below. From then on the app [updates itself](#updates).

### Debian / Ubuntu / Linux Mint

```sh
sudo apt install ./landau-bridge_1.0.0_amd64.deb
```

`apt` pulls in the two runtime libraries the app needs (`libwebkit2gtk-4.1-0`, `libgtk-3-0`). The app appears in the applications menu as **پل لاندا**. To remove it: `sudo apt remove landau-bridge`.

### Fedora / RHEL / openSUSE (RPM)

```sh
sudo dnf install ./landau-bridge-1.0.0-1.x86_64.rpm      # Fedora, RHEL
sudo zypper install ./landau-bridge-1.0.0-1.x86_64.rpm   # openSUSE
```

To remove it: `sudo dnf remove landau-bridge`.

### Other Linux (Arch, Manjaro, NixOS, …) — AppImage

The AppImage is a single file that runs on any 64-bit Linux desktop; nothing is installed system-wide.

```sh
chmod +x landau-bridge_1.0.0_amd64.AppImage
./landau-bridge_1.0.0_amd64.AppImage
```

**Add it to the applications menu.** On the first run from an AppImage the dashboard offers to install the app for you. It detects on its own that it is running from an AppImage on Linux — the offer never appears on Windows or on a `.deb`/`.rpm` install, which already register themselves. Click **نصب** (install) and the app:

- copies the AppImage to `~/.local/bin/landau-bridge.AppImage`,
- copies its icons to `~/.local/share/icons/hicolor/`,
- writes a menu entry to `~/.local/share/applications/ir.landau.bridge.desktop`.

Nothing outside your home folder is touched, and your data and settings stay where they are. From then on start the app from the menu; the downloaded file can be deleted. Click **نه، دیگر نپرس** (no, don't ask again) to keep the AppImage portable — the offer will not come back. When you later run a newer AppImage, the offer appears again and installing replaces the old copy.

To undo the install, delete the three items above.

**If the AppImage does not start** with a message about FUSE, either install FUSE 2 (`sudo pacman -S fuse2` on Arch/Manjaro, `sudo apt install libfuse2` on Ubuntu, `sudo dnf install fuse-libs` on Fedora) or run it without mounting:

```sh
./landau-bridge_1.0.0_amd64.AppImage --appimage-extract-and-run
```

### Windows 10 / 11

Run `landau-bridge_1.0.0_x64-setup.exe`, a per-user setup wizard (no administrator rights needed).

The app uses Microsoft Edge WebView2, which Windows 10 (1803+) and Windows 11 already ship; if it is missing, the installer downloads it. Uninstall from *Settings → Apps* as usual.

## First-time setup

1. In Landau: *Account → API keys → create a key*. The key is shown only once — copy it.
2. Open the app → **تنظیمات** (Settings): paste the key, click **بررسی اتصال** (check connection), pick the destination store (the gateway address already has a default and sits under **تنظیمات پیشرفته**, advanced); enter your Barezpakhsh username and password and click **آزمایش ورود** (test login); save.
3. In Landau, create at least one active pricing rule. **The app refuses to run without one** — otherwise rows would sit on the shelf at cost. Settings also carries **حداقل سود خرده‌فروش** (minimum retail margin, default 15 %): new rows are created with at least that margin until Landau's rule prices them, and the report flags any row whose final price ends up below it (a manual price, or a thin rule).

## Good to know

- Prices you typed by hand in Landau are kept, unless you switch on **قیمت‌های دستی لاندا را هم بازنویسی کن** (overwrite Landau's manual prices) on the price-refresh page.
- Each invoice is added to stock only once; running an import again is safe.
- A **با تعداد، از تاریخ** (quantities since a date) import asks Barezpakhsh only for invoices from that date on. Switch off **فقط فاکتورهای از این تاریخ از بارزپخش خوانده شود** (read only invoices from this date) — next to the date, on the import page — to list and cache the whole statement instead; slower, but only the first time, since every invoice is downloaded once.
- A part that is not in Landau's catalogue: choose **هیچ‌کدام** (none) and open a ticket with Landau support to have it added.
- The report shows, per row, whether Barezpakhsh had the part in stock and any remark: a held-back «ناموجود» price, a changed code or name, a final price below the minimum margin.
- Upgrading migrates the database in place and keeps your match decisions. From 1.1.1 or earlier, every link starts unverified and the next run checks each one against Landau's catalogue before writing from it; the automatic-match threshold (Settings → advanced) is raised to at least 85; and the dashboard says so once, naming any other copy of the app it finds in your home folder, Downloads, Desktop, `~/Applications`, `~/AppImages`, `~/.local/bin` or `~/bin`. A copy older than the one you run cannot use the upgraded data — 1.0 to 1.1.1 refuse it, earlier builds find no links in it — so delete the old ones; the copy in the applications menu is the exception, since **نصب** replaces it.
- **Run one install per store.** Each install remembers which invoices it has already added to stock; two installs on one store would each add them.
- A run holds a shelf row rather than guess: two Barezpakhsh items with different costs on one Landau part, a link the evidence does not prove, or a row someone changed outside the app after a write whose answer never arrived. A held row keeps its cost and stock; the run's report says why, and **پیوندهای نیازمند بررسی** (links to review) and **نوشتن‌های نگه‌داشته** (held writes) on the dashboard take you to the decision.
- Every error and warning is listed on the **خطاها و هشدارها** (errors and warnings) page and written to the log folder; **عملیات‌ها** (operations) keeps every run with its report and events.
- **Where your data lives.** The database, logs and settings are in one folder, shown on the Settings page:
  - Linux: `~/.local/share/ir.landau.bridge/`
  - Windows: `%APPDATA%\ir.landau.bridge\`

  Back up that folder to keep your match decisions and import history. Deleting it resets the app.
- **Where your passwords live.** The Landau key and the Barezpakhsh password go into the operating system's keyring when there is one (GNOME Keyring or KDE Wallet on Linux, Credential Manager on Windows). Without a keyring they are stored in `secrets.json` inside the data folder, readable only by your user. The Settings page tells you which of the two is in use.
- The app talks directly to Landau's gateway and to Barezpakhsh's site at `33314466.ir` (`src-tauri/src/supplier/barezpakhsh/client.rs`); it never uses a proxy.

## Shelf audit

**بازبینی قفسه** (shelf audit) weighs every row of your store's shelf against the evidence and writes nothing — not to Landau, not to the app's own data. It reads Landau's whole catalogue and your shelf, and Barezpakhsh's price today only where a price decides something, so it takes a few minutes. It lists:

- **Rows to check** — a link on the row the evidence does not prove; proven links that disagree on the cost; a last cost written by an item no longer linked to the row; or stock moved onto the row from an item that does not prove there. Each shows the cost the row holds, the cost the proven links give today, the difference in rials and toman, and why.
- **Automatic writes** — every row a link the app chose itself last wrote on, newest first, whether or not it is doubted, each link shown as proven or not today. Even a proven one can be a similar part when the right part is missing from Landau's catalogue (see below), so compare the Barezpakhsh name with the Landau part: you are the one who can tell. A row you find right leaves the list once you pick that same part yourself on **تطبیق‌ها**.

**ذخیرهٔ CSV** saves both lists in one file (UTF-8, opens in a spreadsheet), the last column saying which list a row is in. To correct a row, fix its links on **تطبیق‌ها** (Matches) and run a price refresh: the refresh's report shows each row's cost before and after.

### What the app cannot prevent

These are measured or known, and named rather than hidden:

- **A part Landau's catalogue lacks.** When Barezpakhsh sells a part that is not in Landau's catalogue, a similar part can pass every check: 3 of 2,000 codes in the measurement, and 1 of 557 automatic writes on a real install (a control-arm bushing written as the arm). The audit's automatic list is where you find one.
- **A sale at the till during a write.** Landau overwrites a row's stock with the number the app sends. The app reads the row again just before each write and plans from what it holds, which leaves about a second in which a sale could be overwritten. This stays until Landau accepts stock receipts it applies once.
- **Two installs on one store.** Each install keeps its own record of the invoices it has added; the rule above, one install per store, is the only guard.
- **A piece price on a pack product.** Barezpakhsh prices a part per the unit it invoices; a Landau product sold per pack, set or pair gets that price as it is. No conversion is made, and such rows are not held yet.

## Updates

From 1.0.0 the app keeps itself up to date from [GitHub Releases](https://github.com/vhidvz/landau-bridge/releases). A minute after it starts, and every six hours while it runs, it asks whether a newer version is out; if so it downloads it in the background and checks its signature against the key built into the app — a file that does not match is never installed. Settings → **نسخهٔ برنامه** (app version) shows where it stands and has the switch **بررسی و دریافت خودکار نسخهٔ جدید** (check and download automatically); with it off, the **بررسی نسخهٔ جدید** button still checks on demand.

- **When it goes in.** An AppImage or a Windows install takes the new version when you close the app; **نصب و راه‌اندازی دوباره** (install and restart), on Settings or the dashboard, does it right away. A `.deb`/`.rpm` install asks for the administrator password, so it only installs from that button. Nothing is installed while an import or a price refresh is running or waiting in review.
- **Your data stays.** The database, decisions, logs and passwords live in the data folder, outside the program, and an update never touches them. When a new version changes the database, it first copies it beside the original as `bridge.sqlite.v<N>.bak` (the two newest are kept).
- **No internet, no harm.** If GitHub cannot be reached the app works as before and Settings says when it last tried; it tries again later. A download cut off half-way starts again on the next try.
- An installation older than 1.0.0 has no updater: install 1.0.0 once by hand, as under [Install a release](#install-a-release).

### Rolling back

Updates only ever move forward, so going back is done by hand:

1. In the app, **switch off** Settings → **بررسی و دریافت خودکار نسخهٔ جدید** — otherwise the newer version is fetched again and goes back in the next time the app closes. Then close the app.
2. Download the older version's file from [GitHub Releases](https://github.com/vhidvz/landau-bridge/releases) and install it over the newer one:
   - AppImage: replace `~/.local/bin/landau-bridge.AppImage` (or wherever you keep it) with the older file;
   - Debian/Ubuntu: `sudo apt install --allow-downgrades ./landau-bridge_<version>_amd64.deb`;
   - Fedora/RHEL: `sudo dnf downgrade ./landau-bridge-<version>-1.x86_64.rpm`; openSUSE: `sudo zypper install --oldpackage ./…rpm`;
   - Windows: run the older `…_x64-setup.exe`.
3. If the newer version had changed the database, the older one says so and will not open it: with the app closed, replace `bridge.sqlite` in the data folder with the matching `bridge.sqlite.v<N>.bak` and delete `bridge.sqlite-wal` and `bridge.sqlite-shm` — the decisions and history since that update are lost with it. The backup still has automatic updates on, so switch them off again (step 1) before you next close the app.

## Build from source

Each operating system builds its own installers — there is no cross-compiling. Build the Windows installers on Windows and the Linux packages on Linux. A `.deb`/`.rpm` built on a newer distribution may refuse to install on an older one (it links against the builder's system libraries), so build on the oldest distribution you want to support, or hand out the AppImage, which carries its libraries with it.

### Prerequisites — everything

| Tool | Version | How |
| --- | --- | --- |
| Rust (stable) | ≥ 1.90 | [rustup.rs](https://rustup.rs) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` on Linux, `rustup-init.exe` on Windows |
| Node.js | ≥ 22.19 (22 LTS) or ≥ 24.11 | [nodejs.org](https://nodejs.org) or your package manager |
| pnpm | ≥ 10 | `corepack enable pnpm` (comes with Node) or `npm install -g pnpm` |
| git | any | your package manager |

The first build downloads a few hundred megabytes of Rust crates and npm packages and takes several minutes; later builds are incremental.

### Prerequisites — Debian / Ubuntu

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Ubuntu 22.04 and Debian 12 or newer are needed for `libwebkit2gtk-4.1-dev`.

### Prerequisites — Fedora

```sh
sudo dnf group install "c-development"
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libxdo-devel libappindicator-gtk3-devel librsvg2-devel
```

### Prerequisites — Arch / Manjaro

```sh
sudo pacman -Syu
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool webp-pixbuf-loader
```

Bundling the AppImage downloads `linuxdeploy` and `appimagetool` on first use, which is why `curl`/`wget` and `file` are in every list. Running AppImages needs FUSE 2 (`fuse2`, `libfuse2` or `fuse-libs`, see above).

On Arch/Manjaro (and any distribution with a recent glibc) the AppImage step can fail with `failed to run linuxdeploy` after a wall of `unknown type [0x13] section '.relr.dyn'` messages: the `strip` inside `linuxdeploy` is too old for today's libraries. Skip its stripping — the `.deb` and `.rpm` are unaffected — by prefixing the build with `NO_STRIP=true` (Linux shells only; it is not needed on Windows):

```sh
NO_STRIP=true pnpm tauri build --config src-tauri/tauri.unsigned.conf.json
```

If it still fails, now with `Failed to run plugin: gtk` right after `realpath: /usr/lib/gdk-pixbuf-2.0/2.10.0: No such file or directory`, the `gdk-pixbuf2` package no longer ships that folder (it loads images through `glycin` since 2.44) while its `pkg-config` file still names it, and `linuxdeploy`'s GTK plugin copies it unconditionally. `webp-pixbuf-loader` in the list above puts a real loader there, and pacman's hook writes its `loaders.cache`.

### Prerequisites — Windows

1. **Microsoft C++ Build Tools.** Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and tick **Desktop development with C++** (this brings the MSVC compiler and the Windows SDK). Visual Studio Community with the same workload works too.
2. **WebView2.** Already on Windows 10 (1803+) and Windows 11. Otherwise install the [Evergreen Bootstrapper](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
3. **Rust.** Run `rustup-init.exe` from [rustup.rs](https://rustup.rs); accept the default `x86_64-pc-windows-msvc` toolchain.
4. **Node.js 22 LTS and pnpm**, as in the table above. Use *PowerShell* or the *Developer Command Prompt for VS*, and open a new terminal after installing so `PATH` is refreshed.

### Build

```sh
git clone <this repository> landau-bridge
cd landau-bridge
pnpm install          # also runs `nuxt prepare`
pnpm tauri build --config src-tauri/tauri.unsigned.conf.json   # UI, Rust core in release mode, bundles
                                     # (Linux: NO_STRIP=true pnpm tauri build …, see above)
node scripts/finalize-appimage.mjs   # Linux: drop the bundled libwayland from the AppImage
```

`tauri.unsigned.conf.json` turns off the update signatures, which need the release key only CI holds; a build without it stops with an error about the missing private key. `finalize-appimage.mjs` removes the `libwayland` libraries `linuxdeploy` copies in and packs the AppImage again: a bundled old copy stops a current Mesa from loading its graphics driver and the window stays blank on Arch, Manjaro and Fedora. `node scripts/check-appimage.mjs` confirms the result.

Bundles land in `src-tauri/target/release/bundle/`:

| OS | Files |
| --- | --- |
| Linux | `deb/landau-bridge_1.0.0_amd64.deb`, `rpm/landau-bridge-1.0.0-1.x86_64.rpm`, `appimage/landau-bridge_1.0.0_amd64.AppImage` |
| Windows | `nsis\landau-bridge_1.0.0_x64-setup.exe` |

The UI ships one self-hosted variable font from the Landau front end, **Mikhak** (headings and text), under the SIL Open Font License 1.1 (`public/fonts/`). Estedad, the front end's text face, is not shipped: it builds every dotted Arabic letter out of a dotless skeleton plus a separate dot mark, and WebKitGTK — the engine Tauri uses on Linux — drops those marks, so Persian text loses its dots in the desktop window.

The bare executable is `src-tauri/target/release/landau-bridge` (`.exe` on Windows) — it runs without installing, but only on the machine it was built on or one with the same libraries. To build only some bundles: `pnpm tauri build --bundles appimage` (or `deb`, `rpm`, `nsis`).

To install what you just built, follow [Install a release](#install-a-release).

### Run in development

```sh
pnpm tauri dev                                # hot-reloading app with the Rust core
cd src-tauri && cargo test && cd ..           # Rust unit tests, offline
pnpm test && pnpm typecheck                   # UI utils, TypeScript
```

## For developers

### Releasing

A release is a version bump merged into `main`; GitHub Actions does the rest.

1. On a branch: `node scripts/release.mjs 1.2.0` — sets the version in `src-tauri/Cargo.toml` (the one source), `package.json` and `Cargo.lock`, and turns the CHANGELOG's `[Unreleased]` section into `[1.2.0]` dated in Tehran. A version with a suffix (`1.2.0-rc.1`) is published as a prerelease, which installed apps never pick up.
2. Merge it into `main`. `.github/workflows/release.yml` builds the AppImage, `.deb` and `.rpm` on Ubuntu 22.04 and the setup `.exe` on Windows, signs them for the updater, writes `latest.json` and `SHA256SUMS`, and publishes `v1.2.0` — as a draft first, made public only once all ten files are there. A pull request into `main` runs the same build without publishing anything.
3. If the run fails half-way (a runner hiccup, an expired secret), fix the cause and re-run it from *Actions → release → Run workflow* on `main`; a draft it left behind is replaced, a published release is never touched.

The signing key lives in the repository's Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and in the maintainer's own keeping; its public half is in `src-tauri/tauri.conf.json`. **Losing the key means installed copies can never update again** — they would need a manual reinstall of a build signed with a new one. To try a prerelease on one machine without offering it to anyone, run the app with `LANDAU_BRIDGE_UPDATE_ENDPOINT=https://github.com/vhidvz/landau-bridge/releases/download/v1.2.0-rc.1/latest.json`.

Tauri 2 + Nuxt 4 + Rust core.

To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done` (`state` = `empty|running|review|refreshing|done`; add `&grid=1` for the alignment overlay, `&fx=full` / `&fx=lite` for full or reduced effects, `&appimage=1` for the install offer; fixtures in `app/dev/mockBridge.ts`); `scripts/shots.sh OUT_DIR [filter] [--grid]` screenshots every page.

The AppImage install offer lives in `src-tauri/src/appimage.rs` (detection through the runtime's `APPIMAGE`/`APPDIR` variables, the `~/.local` layout, the copy and the `.desktop` entry — all unit-tested against temporary directories) and is wired through `dashboard_get`, `appimage_install` and `appimage_dismiss` in `src-tauri/src/commands/mod.rs`.
