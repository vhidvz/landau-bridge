# Landau Bridge (پل لاندا)

A desktop app for a Landau retail seller. It does two things:

1. **Imports Barezpakhsh invoices onto your Landau shelf.** Every paid invoice up to today is read; each line is matched to the right part with Landau's fuzzy search (ambiguous lines are shown to you for a decision, and your decision is remembered — by the part's Barezpakhsh *name*, since their codes change). Cost = Barezpakhsh's current price plus VAT, looked up by that name; the shelf price is set by **your own pricing rules in Landau** on the write itself, so a row is never on sale at cost. Two modes: *with quantities, from a date* or *with zero quantities, for review*.
2. **Refreshes prices.** Whenever you like, the cost of every shelf row is refreshed from Barezpakhsh's current price and Landau's rule prices it again. A price Barezpakhsh lists as «ناموجود» (out of stock) is applied only when it *raises* a cost — it is stale and will be relisted higher. Rows you never bought through the app are linked through the name map (below).

The interface is in Persian (right-to-left, Persian numerals, Jalali dates).

## Contents

- [Install a release](#install-a-release)
- [First-time setup](#first-time-setup)
- [Good to know](#good-to-know)
- [Build from source](#build-from-source)
- [For developers](#for-developers)

## Install a release

Get the package for your system from whoever built it (or [build it yourself](#build-from-source)). The version number in the file names follows `src-tauri/tauri.conf.json`; `0.1.0` is used in the examples below.

### Debian / Ubuntu / Linux Mint

```sh
sudo apt install ./landau-bridge_0.1.0_amd64.deb
```

`apt` pulls in the two runtime libraries the app needs (`libwebkit2gtk-4.1-0`, `libgtk-3-0`). The app appears in the applications menu as **پل لاندا**. To remove it: `sudo apt remove landau-bridge`.

### Fedora / RHEL / openSUSE (RPM)

```sh
sudo dnf install ./landau-bridge-0.1.0-1.x86_64.rpm      # Fedora, RHEL
sudo zypper install ./landau-bridge-0.1.0-1.x86_64.rpm   # openSUSE
```

To remove it: `sudo dnf remove landau-bridge`.

### Other Linux (Arch, Manjaro, NixOS, …) — AppImage

The AppImage is a single file that runs on any 64-bit Linux desktop; nothing is installed system-wide.

```sh
chmod +x landau-bridge_0.1.0_amd64.AppImage
./landau-bridge_0.1.0_amd64.AppImage
```

**Add it to the applications menu.** On the first run from an AppImage the dashboard offers to install the app for you. It detects on its own that it is running from an AppImage on Linux — the offer never appears on Windows or on a `.deb`/`.rpm` install, which already register themselves. Click **نصب** (install) and the app:

- copies the AppImage to `~/.local/bin/landau-bridge.AppImage`,
- copies its icons to `~/.local/share/icons/hicolor/`,
- writes a menu entry to `~/.local/share/applications/ir.landau.bridge.desktop`.

Nothing outside your home folder is touched, and your data and settings stay where they are. From then on start the app from the menu; the downloaded file can be deleted. Click **نه، دیگر نپرس** (no, don't ask again) to keep the AppImage portable — the offer will not come back. When you later run a newer AppImage, the offer appears again and installing replaces the old copy.

To undo the install, delete the three items above.

**If the AppImage does not start** with a message about FUSE, either install FUSE 2 (`sudo pacman -S fuse2` on Arch/Manjaro, `sudo apt install libfuse2` on Ubuntu, `sudo dnf install fuse-libs` on Fedora) or run it without mounting:

```sh
./landau-bridge_0.1.0_amd64.AppImage --appimage-extract-and-run
```

### Windows 10 / 11

Run either installer:

- `landau-bridge_0.1.0_x64-setup.exe` — per-user setup wizard (no administrator rights needed), or
- `landau-bridge_0.1.0_x64_en-US.msi` — MSI package, for a system-wide install.

The app uses Microsoft Edge WebView2, which Windows 10 (1803+) and Windows 11 already ship; if it is missing, the installer downloads it. Uninstall from *Settings → Apps* as usual.

## First-time setup

1. In Landau: *Account → API keys → create a key*. The key is shown only once — copy it.
2. Open the app → **تنظیمات** (Settings): paste the key, click **بررسی اتصال** (check connection), pick the destination store (the gateway address already has a default and sits under **تنظیمات پیشرفته**, advanced); enter your Barezpakhsh username and password and click **آزمایش ورود** (test login); save.
3. In Landau, create at least one active pricing rule. **The app refuses to run without one** — otherwise rows would sit on the shelf at cost. Settings also carries **حداقل سود خرده‌فروش** (minimum retail margin, default 15 %): new rows are created with at least that margin until Landau's rule prices them, and the report flags any row whose final price ends up below it (a manual price, or a thin rule).
4. *Optional.* Load the **name map**: Settings → **بارگذاری فایل نگاشت** → pick `output/parts_map.csv` from the `landau-parts` project (the data-cleaning step that built Landau's parts registry from Barezpakhsh). It tells the app which Barezpakhsh names each Landau product was imported under, so a price refresh can also cost shelf rows that never appeared on one of your invoices. Without it the app still runs: an import is unaffected, and a price refresh costs every row it has a link for and skips the rest, listing each skipped row in the report as «این ردیف به هیچ نام بارزپخش وصل نیست» — you can link those by hand on the **تطبیق‌ها** (matches) page instead. Load it again after every new `landau-parts` export.

## Good to know

- Prices you typed by hand in Landau are kept, unless you switch on **قیمت‌های دستی لاندا را هم بازنویسی کن** (overwrite Landau's manual prices) on the price-refresh page.
- Each invoice is added to stock only once; running an import again is safe.
- A **با تعداد، از تاریخ** (quantities since a date) import asks Barezpakhsh only for invoices from that date on. Switch off **فقط فاکتورهای از این تاریخ از بارزپخش خوانده شود** (read only invoices from this date) — next to the date, on the import page — to list and cache the whole statement instead; slower, but only the first time, since every invoice is downloaded once.
- A part that is not in Landau's catalogue: choose **هیچ‌کدام** (none) and open a ticket with Landau support to have it added.
- The report shows, per row, whether Barezpakhsh had the part in stock and any remark: a held-back «ناموجود» price, a changed code or name, a link made from the name map, a final price below the minimum margin.
- Upgrading from a build older than 2026-09-16 migrates the database in place; your match decisions are kept.
- Every error and warning is listed on the **خطاها و هشدارها** (errors and warnings) page and written to the log folder; **عملیات‌ها** (operations) keeps every run with its report and events.
- **Where your data lives.** The database, logs and settings are in one folder, shown on the Settings page:
  - Linux: `~/.local/share/ir.landau.bridge/`
  - Windows: `%APPDATA%\ir.landau.bridge\`

  Back up that folder to keep your match decisions and import history. Deleting it resets the app.
- **Where your passwords live.** The Landau key and the Barezpakhsh password go into the operating system's keyring when there is one (GNOME Keyring or KDE Wallet on Linux, Credential Manager on Windows). Without a keyring they are stored in `secrets.json` inside the data folder, readable only by your user. The Settings page tells you which of the two is in use.
- The app talks directly to Landau's gateway and to barezpakhsh.ir; it never uses a proxy.

## Build from source

Each operating system builds its own installers — there is no cross-compiling. Build the Windows installers on Windows and the Linux packages on Linux. A `.deb`/`.rpm` built on a newer distribution may refuse to install on an older one (it links against the builder's system libraries), so build on the oldest distribution you want to support, or hand out the AppImage, which carries its libraries with it.

### Prerequisites — everything

| Tool | Version | How |
| --- | --- | --- |
| Rust (stable) | ≥ 1.80 | [rustup.rs](https://rustup.rs) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` on Linux, `rustup-init.exe` on Windows |
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
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

Bundling the AppImage downloads `linuxdeploy` and `appimagetool` on first use, which is why `curl`/`wget` and `file` are in every list. Running AppImages needs FUSE 2 (`fuse2`, `libfuse2` or `fuse-libs`, see above).

On Arch/Manjaro (and any distribution with a recent glibc) the AppImage step can fail with `failed to run linuxdeploy` after a wall of `unknown type [0x13] section '.relr.dyn'` messages: the `strip` inside `linuxdeploy` is too old for today's libraries. Skip its stripping — the `.deb` and `.rpm` are unaffected:

```sh
NO_STRIP=true pnpm tauri build
```

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
pnpm tauri build      # generates the UI, compiles the Rust core in release mode, bundles
```

Bundles land in `src-tauri/target/release/bundle/`:

| OS | Files |
| --- | --- |
| Linux | `deb/landau-bridge_0.1.0_amd64.deb`, `rpm/landau-bridge-0.1.0-1.x86_64.rpm`, `appimage/landau-bridge_0.1.0_amd64.AppImage` |
| Windows | `nsis\landau-bridge_0.1.0_x64-setup.exe`, `msi\landau-bridge_0.1.0_x64_en-US.msi` |

The UI ships one self-hosted variable font from the Landau front end, **Mikhak** (headings and text), under the SIL Open Font License 1.1 (`public/fonts/`). Estedad, the front end's text face, is not shipped: it builds every dotted Arabic letter out of a dotless skeleton plus a separate dot mark, and WebKitGTK — the engine Tauri uses on Linux — drops those marks, so Persian text loses its dots in the desktop window.

The bare executable is `src-tauri/target/release/landau-bridge` (`.exe` on Windows) — it runs without installing, but only on the machine it was built on or one with the same libraries. To build only some bundles: `pnpm tauri build --bundles appimage` (or `deb`, `rpm`, `nsis`, `msi`).

To install what you just built, follow [Install a release](#install-a-release).

### Run in development

```sh
pnpm tauri dev                                # hot-reloading app with the Rust core
cd src-tauri && cargo test && cd ..           # Rust unit tests, offline
pnpm test && pnpm typecheck                   # UI utils, TypeScript
```

## For developers

Tauri 2 + Nuxt 4 + Rust core. Design: `docs/superpowers/specs/2026-09-13-landau-bridge-design.md`. Plan: `docs/superpowers/plans/2026-09-13-landau-bridge.md`. Live testing and fixtures: `docs/RUNBOOK.md`. Matching threshold and why: `docs/matching-threshold.md`.

Visual design: `docs/superpowers/specs/2026-09-14-night-glass-redesign-design.md`, plan `docs/superpowers/plans/2026-09-14-night-glass-redesign.md`. To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done` (`state` = `empty|running|review|refreshing|done`; add `&grid=1` for the alignment overlay, `&fx=full` / `&fx=lite` for full or reduced effects, `&appimage=1` for the install offer; fixtures in `app/dev/mockBridge.ts`); `scripts/shots.sh OUT_DIR [filter] [--grid]` screenshots every page — see `docs/RUNBOOK.md`.

The AppImage install offer lives in `src-tauri/src/appimage.rs` (detection through the runtime's `APPIMAGE`/`APPDIR` variables, the `~/.local` layout, the copy and the `.desktop` entry — all unit-tested against temporary directories) and is wired through `dashboard_get`, `appimage_install` and `appimage_dismiss` in `src-tauri/src/commands/mod.rs`.
