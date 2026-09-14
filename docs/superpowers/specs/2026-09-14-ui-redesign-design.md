# Landau Bridge — UI/UX redesign

Date: 2026-09-14. Status: approved in brainstorming; implementation plan at `docs/superpowers/plans/2026-09-14-ui-redesign.md`.

The app works but looks like a default Nuxt UI scaffold: no identity, no logo (the window icon is
Tauri's), identical grey cards, and a sidebar whose active item is unreadable in dark mode
(`text-primary-700` on `bg-primary-950`). This spec redesigns the whole front end around one idea —
**the bridge** between the Barezpakhsh bank and the Landau bank — and changes nothing behind the
IPC boundary. Rust, commands, job logic, error kinds and messages stay exactly as they are.

## 1. Decisions taken with the user

| Topic | Decision |
| --- | --- |
| Direction | «پل»: Persian-tile turquoise + saffron, dark sidebar in both themes, the bridge as the one memorable element. |
| Color mode | Light and dark, both fully designed; in-app switch (system / light / dark), default follows the OS. |
| Logo | New mark designed here; also becomes the app/window icon via `pnpm tauri icon`. |
| Typeface | Vazirmatn only (already bundled, CSP `font-src 'self'`); personality comes from weight, size and tabular figures. |
| Scope | Front end only (`app/`, `public/`, `src-tauri/icons`, `tauri.conf.json` window colour). No behaviour change. |
| Review tooling | A dev-only fixture mode so pages render without Tauri for headless screenshots. Inert in the production build. |

## 2. Design system (tokens)

All tokens live in `app/assets/css/main.css` (Tailwind 4 `@theme static`) and are wired to Nuxt UI
through `app.config.ts` (`ui.colors`) and the `--ui-*` variables Nuxt UI reads on `:root` / `.dark`.

### 2.1 Colour

Three scales, 50–950, defined once as hex (Tailwind names in parentheses):

- **firoozeh** (`primary`, also `info`): 50 `#E8F6F8`, 100 `#C9EAEF`, 200 `#98D7E0`, 300 `#5FBFCD`,
  400 `#2FA6B8`, 500 `#0F8B9D`, 600 `#0C7383`, 700 `#0A5C69`, 800 `#084750`, 900 `#06353C`, 950 `#032126`.
- **zaferan** (`secondary`, also `warning`): 50 `#FDF5E6`, 100 `#FBE8C4`, 200 `#F7D18E`, 300 `#F2BB5F`,
  400 `#EDAC46`, 500 `#E8A33D`, 600 `#C9862A`, 700 `#A26A20`, 800 `#7C5018`, 900 `#573810`, 950 `#332108`.
- **petrol** (`neutral`; a blue-tinted grey, never pure grey): 50 `#F2F5F7`, 100 `#E9EEF1`, 200 `#D5DDE2`,
  300 `#B4C2CA`, 400 `#8FA3AE`, 500 `#6B808B`, 600 `#5B6B74`, 700 `#3C4C55`, 800 `#27404D`, 900 `#152530`,
  950 `#0E1B24`.
- **sabz** (`success`): 50 `#E9F7F0`, 100 `#CDEEDD`, 200 `#9FDDBE`, 300 `#6CC79A`, 400 `#45B37F`, 500 `#2E9E6B`,
  600 `#248257`, 700 `#1C6745`, 800 `#154F35`, 900 `#0F3A27`, 950 `#082418`.
- **sorkh** (`error`): 50 `#FCEDED`, 100 `#F8D6D6`, 200 `#F1ACAC`, 300 `#E77E7E`, 400 `#DD5C5C`, 500 `#D14343`,
  600 `#B13333`, 700 `#8E2828`, 800 `#6C1F1F`, 900 `#4C1616`, 950 `#2E0D0D`.

Semantic surfaces (`--ui-*` overrides; Nuxt UI's `bg-default` is the **surface**, `bg-muted` the
**page**, `bg-elevated` the **wells**, so its components keep their own surface colour untouched):

| Token | Light | Dark |
| --- | --- | --- |
| `--ui-bg-muted` (page) | `#F2F5F7` petrol-50 | `#0E1B24` petrol-950 |
| `--ui-bg` (surface / card) | `#FFFFFF` | `#152530` petrol-900 |
| `--ui-bg-elevated` (table header, wells) | `#E9EEF1` petrol-100 | `#1C2F3B` |
| `--ui-border` (decorative) | `#D5DDE2` petrol-200 | `#27404D` petrol-800 |
| `--ui-border-accented` (control outlines) | `#6B808B` petrol-500 | `#6B808B` petrol-500 |
| `--ui-text-highlighted` (headings) | `#13232C` | `#F3F7F9` |
| `--ui-text` (body) | `#27404D` petrol-800 | `#E6EEF2` |
| `--ui-text-muted` | `#5B6B74` petrol-600 | `#8FA3AE` petrol-400 |
| `--ui-primary` (button fill, link) | firoozeh-600 | firoozeh-400 (with dark text) |
| `--ui-success` / `--ui-warning` / `--ui-error` / `--ui-neutral` (badge text) | sabz-700 / zaferan-800 / sorkh-600 / petrol-600 | sabz-400 / zaferan-300 / sorkh-300 / petrol-400 |

The 500 stops are the brand hues; the shades Nuxt UI uses for text and fills are shifted as above
so every pair clears 4.5:1 (white on firoozeh-500 alone is 4.0:1).

Sidebar (same in both modes): background gradient `#0B1820 → #10222D` (top→bottom) with a faint arch
pattern (inline SVG, 6 % white); text `#C6D4DB`; hover `rgba(255,255,255,.08)`; active item white on
firoozeh-600 `#0C7383` (contrast 6.1:1). Saffron appears only in: the logo dots, the hero arch, the
current pier of the progress stepper, and `warning` semantics.

Contrast floor: 4.5:1 for all text; 3:1 for large text and for boundaries that identify a control
(input and select outlines, the ticket radio cards, checklist rings) — `--ui-border-accented` is
petrol-500 in both modes for that reason. Decorative dividers and surface outlines (petrol-200 /
petrol-800) are exempt: the surface fill already separates them. Every page must be checked in
both modes.

### 2.2 Type

- Family: `Vazirmatn` (variable, 100–900), fallback `Tahoma`, `Segoe UI`, sans-serif.
- Base 15 px, line-height 1.7 for running text, 1.3 for headings, 1 for display numbers.
- Scale (px): 12 caption · 13 small · 15 body · 17 lead · 20 h3 · 24 h2 · 32 page title · 44 display number.
- Weights: 400 body, 500 labels and nav, 700 headings, 800 display numbers and the wordmark.
- Tabular figures (`font-feature-settings: "tnum"`) on every table, counter, price and date.
- Persian digits everywhere the current code already uses `faNumber`; Latin data (codes, URLs, ISO
  dates, JSON) stays Latin and is isolated with `dir="ltr"` / `<bdi>` as today.
- No eyebrow labels above headings, no all-caps, no « — » joins in labels; the current `A — B`
  strings on the dashboard become two lines or two cells.
- Line length ≤ 75 characters for explanatory paragraphs (`max-w-prose` ≈ 60ch for Persian).

### 2.3 Shape, depth, spacing

- Radius: 8 px controls (buttons, inputs, badges), 14 px surfaces (cards, tables, modal), full for pills.
- Shadow only on floating things (modal, select menu, toast): `0 12px 32px -12px rgb(11 24 32 / .35)`.
  Surfaces on the page use a 1 px border, no shadow.
- Spacing on a 4 px grid; page gutter 32 px, section gap 32 px, inside a surface 20–24 px.
- Grouping is by surface and whitespace. Two adjacent things get one surface with a divider only when
  the divider says something (e.g. settings sections). No grid of identical cards.

### 2.4 Motion

- One orchestrated moment: the dashboard bridge draws itself on first paint (stroke-dashoffset,
  900 ms ease-out, deck then cable then piers), once per page mount.
- Feedback: the current pier of the stepper breathes (opacity 1 → .6, 1.6 s); a job reaching `DONE`
  fills the deck and shows a check; the nav pill moves with a 150 ms transition; modals fade+scale
  120 ms.
- `@media (prefers-reduced-motion: reduce)`: all of the above become instant.
- No hover lift on cards, no per-section fade-ins.

## 3. Shell (layout)

```
┌──────────────────────────────────────────────┬────────────────┐
│ main (scrolls)                               │ aside 256px    │
│ ┌ PageHeader ──────────────────────────────┐ │ [mark] پل لاندا │
│ │ title(32/700)                  [action]  │ │                │
│ │ one sentence: what this page does        │ │ ▣ داشبورد      │
│ └──────────────────────────────────────────┘ │   وارد کردن…   │
│                                              │   به‌روزرسانی… │
│ content, max-w 1100px, gutter 32px           │   تطبیق‌ها     │
│                                              │   گزارش و خطاها│
│                                              │   تنظیمات      │
│                                              │                │
│                                              │ [RunningJobCard]│
│                                              │ ◐ theme  v0.1.0│
└──────────────────────────────────────────────┴────────────────┘
```

- `dir="rtl"` stays on `<html>`; the aside is the start side, so it renders on the right.
- Aside: fixed 256 px, full height, own scroll if needed; no collapse (window min width is 960).
- Nav items: 40 px tall, icon 20 px + label 15/500, 8 px radius pill; active = white on firoozeh-600
  with the icon in zaferan-300; hover = white 8 %; focus-visible = 2 px zaferan-400 ring.
- `RunningJobCard` (bottom of aside): shown when `jobCurrent()` returns a job; polls every 3 s as today.
  Kind + number, phase label, a thin progress line, "ادامه" link to `/import?job=N` or `/refresh`.
- `ThemeToggle`: three-way segmented control (سیستم / روشن / تیره) using Nuxt UI's `useColorMode`;
  preference persisted by `@nuxtjs/color-mode` (localStorage).
- Version from `package.json` via `import.meta.env` / Nuxt runtime config, small muted text.
- `PageHeader` props: `title`, `description?`, `#actions` slot. Every page uses it; nothing else sets an `h2`.
- `tauri.conf.json`: window `backgroundColor` set to `#F2F5F7` so there is no white flash; title unchanged.

## 4. Logo

**Mark** (`app/components/BrandLogo.vue`, inline SVG, `viewBox 0 0 64 64`, stroke-based; chosen from
four rendered variants — towers made it read as a hanging basket, so there are none):

- Deck: horizontal line y=16 from x=8 to x=56, stroke 5, round caps.
- Bowl: one cubic `M14 16C14 44 50 44 50 16` hanging from the deck (lowest point y≈37) — the bowl
  of «پ», open at the top, and the span hanging under the deck.
- Dots: three circles r=3.5 at (24,48), (32,52), (40,48) — the three dots of «پ» and the piers'
  reflection. The whole sits between y=13.5 and y=55.5, so it is optically centred in the box.

**Wordmark**: «پل لاندا» in Vazirmatn 800, 20 px, letter-spacing 0, next to the mark. Optional second
line «Landau Bridge» is *not* shown (Persian-only UI).

**App icon** (`public/brand/icon.svg`, 1024×1024): firoozeh-500→700 diagonal gradient on a
rounded square (radius 22 %), the mark in white at 62 % of the tile, dots in zaferan-400. Generated
into `src-tauri/icons/*` with `pnpm tauri icon public/brand/icon.svg`; the existing Tauri defaults
are overwritten. `public/brand/logo.svg` (mark only, currentColor) is also written for reuse.

## 5. Pages

Every page keeps its data flow, commands and error handling exactly as in the current code; only the
template and styling change. Persian copy is rewritten where it helps (plain verbs, sentence case,
one job per sentence) and every action keeps the same name from button to toast.

### 5.1 Dashboard `/`

- Header: title = store name (or «پل لاندا» when none); description «پل بین فاکتورهای بارزپخش و قفسهٔ لاندا».
- If `!settings_complete`: `SetupChecklist` replaces the hero. Three steps from `settingsGet()`:
  کلید لاندا (`has_landau_token`), فروشگاه مقصد (`store`), ورود بارزپخش (`has_supplier_password`),
  each with a check/ring and a link to `/settings`. Large mark at 96 px beside it.
- Otherwise `BridgeHero`: an SVG bridge spanning the surface. Right bank «بارزپخش»: display number
  `invoices_cached` labelled «فاکتور ذخیره‌شده», small «N پرداخت‌نشده» under it, and the supplier user
  (LTR). Left bank «لاندا»: store name and `decided_matches` «تطبیق ثبت‌شده». On the deck: the two
  actions as buttons — «وارد کردن فاکتورها» (solid) and «به‌روزرسانی قیمت‌ها» (soft) — disabled while
  a job runs; when `running` exists the deck shows the `RunningJobCard` content instead.
- Below the hero, one surface with two rows «آخرین وارد کردن» / «آخرین به‌روزرسانی»: status badge, local
  time, and a link to the job in `/logs`. Empty rows say «هنوز اجرا نشده».
- Attention rows (not alert boxes): pending review (`running.status === 'REVIEW'`) → «N قلم منتظر
  تصمیم شماست» with «ادامهٔ بازبینی»; `recent_errors > 0` → «N خطا یا هشدار در هفت روز گذشته» with
  «مشاهده». Rendered as a list with a coloured dot and the action on the end side.

### 5.2 Import `/import`

- Header: «وارد کردن فاکتورهای بارزپخش»; description one sentence; `#actions` = «توقف» while active.
- Mode choice (when no job or finished): two «tickets» side by side (radio semantics via a hidden
  input, `aria-checked`), selected one has a firoozeh border and a small mark; the Jalali date input
  lives inside the first ticket and is disabled when the second is selected. Under both, the
  shared explanation, then the «شروع وارد کردن» button. The current copy is kept but split into shorter
  sentences.
- Job surface: `JobProgress` with `kind="IMPORT"` (9 piers); the events list beneath.
- Review surface: `ReviewTable` (see §6); the header carries the count and the «ادامه» button, the
  button label changes to «ادامه با اقلام تصمیم‌گرفته‌شده» when items remain, as today.
- Report surface: `ReportTable`.

### 5.3 Refresh `/refresh`

Same structure; the only options are the overwrite switch and «شروع به‌روزرسانی». `JobProgress`
with `kind="REFRESH"` shows exactly the five phases the Rust job emits: PREFLIGHT, PRICE, WRITE,
REPRICE, REPORT (`src-tauri/src/jobs/refresh.rs`, `jobs/mod.rs`). Today all nine are shown; that is a bug.

### 5.4 Matches `/matches`

- Header + one-sentence description; the longer explanation about tickets moves to an `EmptyState`
  shown only when the filtered list is empty.
- Filter as a segmented control (`URadioGroup` styled, or `UTabs` `variant="pill"`), search input on
  the end side.
- Ledger table: name in invoice · code (LTR) · Landau part · how (badge) · similarity (`SimilarityBar`)
  · date · «تغییر». Row hover only.
- Edit modal: same actions, spacing per §2.3, candidates as a list with `SimilarityBar` + «انتخاب».

### 5.5 Logs `/logs`

- Header, `#actions` = «باز کردن پوشه لاگ» + «تازه‌سازی».
- `UTabs` with two tabs «عملیات‌ها (N)» / «خطاها و هشدارها (N)».
- Jobs tab: the jobs list on the start side and the detail panel on the end side (two columns ≥ 1024 px,
  stacked below); the selected job row is
  highlighted with firoozeh-50/900 + start border 3 px firoozeh-500. Detail: summary in a collapsed
  `<details>` («خلاصهٔ خام»), events list with a level dot (INFO petrol-400, WARN zaferan-500, ERROR
  error-500), then `ReportTable`.
- Errors tab: ledger table; `context` in a collapsed `<details>` as today.

### 5.6 Settings `/settings`

- Header «تنظیمات»; description «اتصال‌ها و قواعد وارد کردن».
- One surface with three sections separated by dividers: لاندا, بارزپخش, قواعد وارد کردن; a fourth
  quiet section «داده‌ها». Each connection section shows a status chip on its title: «متصل — <business>»
  after a successful check this session, «ذخیره‌شده» when a secret exists, «آزمایش‌نشده» otherwise.
- Field widths: URLs and secrets full width; numbers 8 rem; all LTR data fields `dir="ltr"`.
- Sticky bottom bar inside main: «ذخیره تنظیمات» (solid) on the end side.

### 5.7 Empty and error states

- `EmptyState` props: `title`, `description?`, `#action` slot; renders a 64 px line illustration
  (a small bridge with no traffic, or a shelf outline; two variants via prop `art: 'bridge' | 'shelf'`).
  Used by matches (no rows), logs (no jobs / no errors), report (no rows after filter), review (no
  candidates → inline text as today, not the component).
- `ErrorBanner` keeps its props; restyled as a surface with a 3 px start border in error-500, the
  kind as the heading (LTR when unknown, as today), message, and detail in 13 px muted LTR.

## 6. Components

New, under `app/components/`:

| Component | Props / slots | Notes |
| --- | --- | --- |
| `BrandLogo` | `size?: number` (px, default 28), `withWordmark?: boolean`, `dots?: 'accent' \| 'current'` | Inline SVG per §4. |
| `PageHeader` | `title`, `description?`; `#actions` | Title 32/700; description 15 muted; actions end-aligned. |
| `BridgeHero` | `dash: Dashboard` | SVG bridge + banks; draw-in animation once; shows `RunningJobCard` on the deck when `dash.running` is set, the two action buttons otherwise. |
| `SetupChecklist` | `settings: SettingsView` | Three steps, each links to `/settings`. |
| `RunningJobCard` | `job: JobRow`, `compact?: boolean` | Used in the aside and on the hero deck. |
| `ThemeToggle` | — | Segmented سیستم/روشن/تیره over `useColorMode()`. |
| `EmptyState` | `title`, `description?`, `art?`; `#action` | §5.7. |
| `SimilarityBar` | `value: number` (0–100) | 64 px bar + Persian percent; colour: ≥80 success, ≥60 zaferan, else petrol. Replaces the bare badge everywhere similarity is shown. |
| `LedgerTable` | `#default` (thead/tbody) | The one table wrapper: surface, muted header row, `tnum`, row divider petrol-200/800, hover petrol-50/900. Replaces the four hand-rolled `overflow-x-auto rounded-lg border …` wrappers. |
| `SegmentedControl` | `v-model` (string union), `items: { value, label }[]`, `ariaLabel?` | The one segmented filter (report filters, match filters); `role="radiogroup"`. |

Rewritten:

- `JobProgress` gains `kind: 'IMPORT' \| 'REFRESH'` and derives its phase list from it. Renders the
  bridge stepper: a deck line with one pier per phase; done piers firoozeh-500 with the deck filled
  up to them, the current pier zaferan-400 (breathing), future piers petrol-300/700; labels under
  the piers (13 px); the `UProgress` + «N از M» line under the deck when `progress.total > 0`; the
  status badge; the events list (level dot + message, phase label in muted text) capped at 16 rem.
- `ReviewTable`: one surface per item (these *are* a sequence of decisions, so a light «۱ از N» count
  in the surface header is allowed); header = name 17/700, code LTR + «تعداد N» muted; end side:
  authenticity select, «بعداً» (ghost), «هیچ‌کدام» (soft error). Body: search row, then candidates as a
  list with name/brand/alias and `SimilarityBar` + «همین است» (solid, primary). No behaviour change.
- `ReportTable`: filters as a segmented control, count, CSV button; `LedgerTable` body unchanged in
  columns; before→after cells rendered as two stacked values (muted before, strong after) instead of
  `a → b` in one LTR string.
- `ErrorBanner`: §5.7.

Layout: `app/layouts/default.vue` rewritten per §3; `app/app.vue` unchanged except `UApp` keeps the
`fa_ir` locale.

Config:

- `app.config.ts` (new): `ui.colors = { primary: 'firoozeh', secondary: 'zaferan', success: 'success',
  info: 'firoozeh', warning: 'zaferan', error: 'error', neutral: 'petrol' }`; component defaults:
  `button.slots.base` rounded 8 px and `font-medium`; `card.slots.root` 14 px radius, no shadow;
  `input`/`select` 8 px radius; `modal.slots.content` 14 px radius.
- `nuxt.config.ts`: `colorMode: { preference: 'system', fallback: 'light', classSuffix: '' }` (Nuxt UI
  already registers the module; only the preference is set). Add `<meta name="color-scheme"
  content="light dark">` and `<link rel="icon" href="/brand/logo.svg">`. Icon bundle: the lucide
  icons the new templates use are picked up by `clientBundle.scan` as today.

## 7. Dev fixture mode (review tooling)

`app/composables/useBridge.ts` keeps its API. `call()` and `onJobEvents()` delegate to
`app/dev/mockBridge.ts` when **both** `import.meta.dev` is true **and** `window.__TAURI_INTERNALS__`
is absent; otherwise the Tauri path runs unchanged. The mock returns fixed fixtures for every
command (a complete `Dashboard`, a `RUNNING` import at `MATCH`, a `RUNNING` refresh at `WRITE`,
three review items with candidates, a finished job with items and events, 12 match rows, 6 error
rows, a `SettingsView`), and `?state=` query values on the page switch scenarios the screenshots
need: `empty` (settings incomplete), `running`, `review`, `refreshing`, `done`. `?mode=light|dark`
sets the colour mode (a dev-only Nuxt plugin registered from `nuxt.config.ts` only in development).
`onJobEvents` emits one progress tick for the running fixtures and returns a no-op unsubscriber.
The fixture module is reached only through a dynamic import behind `import.meta.dev`, so the
production build does not contain it.

## 8. Verification

1. `pnpm typecheck` and `pnpm test` pass (utils and types untouched, so the existing tests still cover them).
2. Screenshots: `pnpm dev` + headless Chrome (`google-chrome-stable --headless --screenshot`), every
   page, light and dark, at 1200×800, saved under the scratchpad and shown to the user. Each screenshot
   is critiqued against §2 (contrast, no default tells, one bold element per page) before the next page.
3. Contrast: the sidebar active/inactive text, muted text on both backgrounds, and badges checked
   numerically (a small script over the token hex values) — all ≥ 4.5:1 for text.
4. `pnpm tauri icon public/brand/icon.svg` regenerates `src-tauri/icons`; `pnpm tauri dev` shows the
   new window icon and the dark-sidebar shell; the seller's flows (import in both modes, refresh,
   review, matches edit, settings save) are exercised once in the real app to confirm nothing behavioural moved.
5. `prefers-reduced-motion` checked with Chrome's emulation: no animation on the dashboard.

## 9. Out of scope

- Any change under `src-tauri/src`, to IPC command names/payloads, or to `app/types/bridge.ts`.
- New features (bulk actions, filters beyond today's, search across pages).
- A second typeface.
- Sidebar collapse / responsive layout below the 960 px window minimum.
