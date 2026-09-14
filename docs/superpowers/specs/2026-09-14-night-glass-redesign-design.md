# Landau Bridge — night-glass redesign (Ledger POS structure, Landau identity)

Date: 2026-09-14. Status: approved in brainstorming (sections 1–3); implementation plan to follow.
Supersedes the visual system of `2026-09-14-ui-redesign-design.md` (the bridge idea, the fixture
mode and the screenshot tooling from that spec survive; its palette, light mode, sidebar and page
layouts do not).

The user likes the interface of `/home/vahid/WorkSpace/payment-pos-app` (Ledger POS): a night-blue
glass console with one warm accent, an editorial page header (kicker, display title, lede), a
narrow glass sidebar, status pills, and a lit backdrop. This spec rebuilds the پل لاندا front end on
that **structure**, with Landau's own **identity**: the crimson/sapphire/graphite tokens and the
Mikhak/Estedad typefaces of `/home/vahid/Claude/examples/lnd-frontend` (its `app/assets/css/main.css`,
`app/assets/scss/*`, `public/fonts/*`). It also restructures the dashboard into a one-glance
overview, splits «گزارش و خطاها» into three pages, and prunes settings a retailer never touches.

Nothing behind the IPC boundary changes: Rust, commands, payloads, `app/types/bridge.ts`, job logic,
error kinds and messages stay exactly as they are.

## 1. Decisions taken with the user

| Topic | Decision |
| --- | --- |
| Colour mode | **Dark only.** Light mode and the theme switch are removed; `@nuxt/ui`'s colour-mode integration is turned off and `<html class="dark">` is fixed. |
| Accent | Landau's **crimson** takes the role Ledger gives brass (primary button, active nav, kicker, focus, live dot). **Sapphire** is the cool counter-light (backdrop glow, bridge drawing, done steps, info). |
| Neutral | Landau's **graphite** ramp for surfaces and text (the four surfaces `#0b0d12 / #10131a / #171b24 / #232833`). |
| Signals | Ledger's quiet signals: sage `#8fbf8f` ok, amber `#d9a441` warn, rust `#cd7460` danger; info = sapphire-300. Never filled, never glowing — that is how crimson (hot) and error (quiet) stay apart although both are red. |
| Type | **Mikhak** for display (headings, big numbers, wordmark) and **Estedad** for everything else — the two self-hosted variable faces of the Landau front end. Vazirmatn is removed. No third (mono) family: system monospace. |
| Component layer | **Keep `@nuxt/ui` 4**, re-skinned through `--ui-*` tokens and `app.config.ts`; the identity pieces (backdrop, sidebar, page header, glass card, pill, stat tile, job sheet) are hand-built like Ledger's. |
| Logs | **Three pages**: `/jobs` (list), `/jobs/:id` (job sheet), `/errors` (errors and warnings). `/logs` is deleted. |
| Dashboard | Rebuilt as a one-glance overview: four status cards, a live/idle «عبور» card, recent activity. |
| Retailer's view | Automatic price refresh (scheduler, tray, autostart, notifications) is the **next spec**, not this one; this spec reserves its UI slots and ships the front-end-only conveniences (§11). Settings a retailer never touches move to an «پیشرفته» disclosure or to the page where the choice is made (§10). |
| Scope | Front end only: `app/`, `public/`, `nuxt.config.ts`, `app.config.ts`, `package.json` (drop nothing, add nothing but the fonts), `src-tauri/tauri.conf.json` window colour, `scripts/shots.sh`, tests under `app/utils`. |
| Working tree | Another session commits to `main` in this repo (e.g. `aed9731`, 2026-09-14 22:14). Implementation happens on a branch / worktree and rebases before merge. |

## 2. Design system

All tokens live in `app/assets/css/main.css` (Tailwind 4 `@theme static`); `app.config.ts` maps
them into `@nuxt/ui`; the `--ui-*` overrides sit on `:root` (there is no `.light`).

### 2.1 Colour

Ramps, 50–950, hex, verbatim from the Landau front end unless noted:

- **graphite** (neutral): 50 `#f7f7f9` 100 `#efeff2` 200 `#dfdfe5` 300 `#c2c3cd` 400 `#9a9cab`
  500 `#767989` 600 `#5c5f6e` 700 `#494b58` 800 `#33353f` 900 `#1b1d24` 950 `#0b0d12`.
- **crimson** (primary; 400–800 are the logo's own gradient stops): 50 `#fdf2f2` 100 `#fbe0e0`
  200 `#f6c4c4` 300 `#ec9a9a` 400 `#e06e6e` 500 `#d63e3e` 600 `#cc0e0e` 700 `#b80d0d` 800 `#a30b0b`
  900 `#870a0a` 950 `#4a0505`.
- **sapphire** (secondary/info): 50 `#eff0ff` 100 `#e0e2ff` 200 `#c4c7ff` 300 `#9ea3ff` 400 `#757cff`
  500 `#4f55f0` 600 `#3a3adb` 700 `#2e2bb8` 800 `#24228f` 900 `#191a72` 950 `#000077`.
- **sage** (success), **amber** (warning), **rust** (error): the 400 stop is the Ledger signal value;
  the other stops are sRGB mixes toward white (50–300) and toward `#0b0d12` (500–950). Only the
  400 stop is ever used directly; the ramps exist because `@nuxt/ui` needs a full scale per name.
  - sage: 50 `#f6faf6` 100 `#edf5ed` 200 `#d9e9d9` 300 `#bcd9bc` 400 `#8fbf8f` 500 `#779f79` 600 `#5f7f62` 700 `#4a624e` 800 `#384a3d` 900 `#28342e` 950 `#1b2221`
  - amber: 50 `#fcf8f0` 100 `#f9f0e1` 200 `#f2e0be` 300 `#e8c88d` 400 `#d9a441` 500 `#b48939` 600 `#8f6e30` 700 `#6e5529` 800 `#514022` 900 `#382e1c` 950 `#241f18`
  - rust: 50 `#fbf4f2` 100 `#f7e9e6` 200 `#eed0c9` 300 `#e1aca0` 400 `#cd7460` 500 `#aa6152` 600 `#874f44` 700 `#683e37` 800 `#4d302d` 900 `#362423` 950 `#22191b`

Ledger-named aliases (the component CSS ports from Ledger and keeps its vocabulary):

| Token | Value | Role |
| --- | --- | --- |
| `--color-ink-950` | `#0b0d12` | page (window background too) |
| `--color-ink-900` | `#10131a` | surface: card fill base, inputs |
| `--color-ink-800` | `#171b24` | raised: wells, table header, glass-inset |
| `--color-ink-700` | `#232833` | accented: selected rows, sim-track, scrollbar thumb |
| `--color-paper` | `#f7f7f9` | primary text |
| `--color-paper-dim` | `#c2c3cd` (graphite-300) | secondary text, lede, table body |
| `--color-paper-mute` | `#9a9cab` (graphite-400) | labels, hints, timestamps |
| `--color-line` | `rgb(255 255 255 / .09)` | hairlines: dividers, card borders |
| `--color-line-bright` | `#6b6e7c` (opaque) | control outlines: fields, selects, unselected tickets, segmented track |
| `--color-ok` / `--color-warn` / `--color-danger` / `--color-info` | `#8fbf8f` / `#d9a441` / `#cd7460` / `#9ea3ff` | signals |
| `--shadow-glow-crimson` | `0 0 44px 0 rgb(214 62 62 / .16)` | hover glow, brand tile |
| `--shadow-card` | `0 1px 0 0 rgb(255 255 255 / .04) inset, 0 24px 48px -24px rgb(0 0 0 / .6)` | glass |
| `--ease-swift` | `cubic-bezier(0.22, 1, 0.36, 1)` | every transition |

`@nuxt/ui` semantic variables on `:root`: `--ui-bg #10131a`, `--ui-bg-muted #0b0d12`,
`--ui-bg-elevated #171b24`, `--ui-bg-accented #232833`, `--ui-bg-inverted #f7f7f9`;
`--ui-border rgb(255 255 255/.09)`, `--ui-border-muted .06`, `--ui-border-accented #6b6e7c`;
`--ui-text-highlighted #fff`, `--ui-text #f7f7f9`, `--ui-text-toned graphite-200`, `--ui-text-muted
graphite-300`, `--ui-text-dimmed graphite-400`; `--ui-primary crimson-400`, `--ui-secondary
sapphire-300`, `--ui-success sage-400`, `--ui-warning amber-400`, `--ui-error rust-400`, `--ui-info
sapphire-300`, `--ui-neutral graphite-400`; `--ui-radius 0.5rem` (so `@nuxt/ui`'s `rounded-md` controls come out at 12 px). `app.config.ts` `ui.colors`:
primary `crimson`, secondary `sapphire`, success `sage`, warning `amber`, error `rust`, info
`sapphire`, neutral `graphite`. `color-scheme: dark` on `:root`.

**Where each colour may appear**

- Crimson, hot form only: primary button (gradient `crimson-600 → crimson-800`, white text, inset
  highlight `rgb(255 255 255 / .25)`, glow `0 8px 24px -8px rgb(214 62 62 / .5)`); active nav item
  (crimson-300 text, fill `linear-gradient(90deg, rgb(214 62 62 / .12), rgb(214 62 62 / .03))` in
  reading direction, 2 px crimson-500 bar on the inline-start edge); kicker text (crimson-400);
  links (crimson-400, hover crimson-300); focus ring `2px solid rgb(224 110 110 / .7)`; the live
  dot; the ticket's selected border; the bridge cable; the current stepper pier; the logo dots.
- Crimson never appears as a filled badge, a filled danger button or a row background.
- Sapphire: backdrop counter-glow, bridge deck and piers, done stepper piers, «خودکار» match pill,
  price-decrease delta, info pill, empty-state accent line.
- Signals (sage / amber / rust) only as: pill (text = colour, border = colour at 30 %, fill = colour
  at 6 %), dot, plain text, `.btn-danger` (rust text, border 35 %, fill 6 %, hover 14 %), the 3 px
  start-border of the error banner, the status-tinted header strip of the job sheet (colour at 6 %).
- The logo's steel underlay is graphite-400 at 32 % (Landau's dark value) if the Landau mark is ever
  drawn; the bridge mark (§3.2) does not use it.

**Contrast contract** (rewrite `app/utils/palette.spec.ts` to assert exactly this from the CSS):

| Pair | Floor | Measured |
| --- | --- | --- |
| paper, paper-dim, paper-mute on ink-950/900/800/700 | 4.5 | ≥ 5.42 (paper-mute on ink-700) |
| crimson-400 on ink-950/900/800/700 | 4.5 | 6.13 / 5.86 / 5.44 / 4.66 |
| white on crimson-600 and crimson-800 (button gradient ends) | 4.5 | 5.77 / 8.05 |
| ok, warn, info on the four surfaces | 4.5 | ≥ 6.42 |
| danger on ink-950/900/800 | 4.5 | 5.80 / 5.54 / 5.14 |
| line-bright `#6b6e7c` on ink-900 and ink-800 (control outlines) | 3.0 | 3.64 / 3.36 |

Rule that the test cannot see: danger text never sits on ink-700 (4.40); ink-700 carries only
muted text (table header) and no text at all elsewhere. graphite-500 is never a text colour.

### 2.2 Type

- `--font-display: "Mikhak", "Estedad", system-ui, sans-serif` — `h1`–`h3`, `.display`, big
  numbers (`StatTile`, stat cards), the wordmark.
- `--font-sans: "Estedad", system-ui, sans-serif` — body, controls, tables, kicker, labels.
- `--font-mono: ui-monospace, "Cascadia Mono", "Segoe UI Mono", Menlo, monospace` — codes,
  paths, URLs, JSON, the username. No self-hosted mono (Landau's reasoning: a mono is for
  transcribability, not a voice).
- Fonts: copy `mikhak-var.woff2` (65 KB) and `estedad-var.woff2` (118 KB) from
  `/home/vahid/Claude/examples/lnd-frontend/public/fonts/` to `public/fonts/`; delete
  `Vazirmatn[wght].woff2`. `@font-face` for each: `font-weight: 100 900`, `format('woff2')`,
  `font-display: swap`. Both are OFL 1.1 (Amin Abedi); note the licence in the README's build section.
  CSP `font-src 'self'` already allows them.
- Sizes (base 15 px): 12 caption · 13 small/kicker · 15 body · 17 lead · 20 h2 · 24 h3-display ·
  38 page title (`2.4rem`, Mikhak 700, line-height 1.2) · 28 stat number (`1.75rem`, Mikhak 700,
  line-height 1) · 44 dashboard big number (`2.75rem`). Body line-height 1.75 (Persian sets looser),
  headings 1.28, numbers 1.
- Weights: 400 body, 500 labels/nav/kicker, 600 h2/h3, 700 h1 and numbers.
- **Persian rules** (from Landau's `_typography.scss`): no `letter-spacing` on Persian text ever
  (joined letters break), no uppercase, no small-caps. Ledger's tracked-uppercase kicker becomes
  `.kicker { font-size: .8125rem; font-weight: 500; color: crimson-400 }`. Ledger's `.data-label`
  becomes 12 px, 500, paper-mute, no tracking.
- Tabular figures on every table, counter, price, date and duration. Persian digits for every
  number the seller reads (`faNumber`); Latin only for codes, URLs, ISO strings, JSON, inside
  `dir="ltr"` / `<bdi>` isolates as today. The kicker index uses Persian digits («۰۱»).
- Explanatory paragraphs ≤ 58 ch (`max-w-[58ch]`).

### 2.3 Shape and depth (Ledger's values)

- `.glass`: `border-radius: 18px`, `border: 1px solid rgb(255 255 255 / .07)`, fill
  `linear-gradient(180deg, rgb(255 255 255 / .035), rgb(255 255 255 / .012) 42%, rgb(255 255 255 / .02))`,
  `box-shadow: var(--shadow-card)`, `backdrop-filter: blur(14px)`. Default padding 24 px.
- `.glass-inset`: `border-radius: 12px`, fill `rgb(11 13 18 / .55)`, border `rgb(255 255 255 / .05)`.
- Controls (buttons, fields, segmented, tickets' inner ring): 12 px radius; pills: full; ledger
  table container: glass; modal and select menu: glass (`app.config.ts` slots), 18 px.
- `.hover-glow` on interactive cards: border → `rgb(214 62 62 / .35)`, shadow → glow, 0.4 s swift.
- Scrollbars: thin, thumb ink-700 with 3 px transparent inset, transparent track (Ledger's rules).
- Selection: `rgb(214 62 62 / .28)` on `#fff`.

### 2.4 Motion and reduced effects

- **Reveal** (CSS only, no GSAP): a `v-reveal` directive (`app/plugins/reveal.client.ts`) on a
  page's root element. In its `beforeMount` hook it adds `reveal-armed` to the root; in `mounted`
  it queries `[data-reveal]`, sets `--reveal-delay: <index × 70ms>` on each and, on the next
  animation frame, adds `is-revealed` to the root. CSS: `.reveal-armed [data-reveal] { opacity: 0;
  transform: translateY(22px); transition: opacity .8s var(--ease-swift) var(--reveal-delay),
  transform .8s var(--ease-swift) var(--reveal-delay) } .reveal-armed.is-revealed [data-reveal] {
  opacity: 1; transform: none }`. Without the directive nothing is hidden. Under reduced motion or
  fx-lite the directive adds both classes at once (no transition).
- Page transition: `page-enter/leave` fade + 10 px / −6 px slide, 0.28 s swift (Nuxt
  `pageTransition`).
- Live dot (`.dot`): pulse halo 2.6 s, as Ledger. Current stepper pier breathes 1.6 s.
- The bridge drawing (§4.1) draws itself once per mount (stroke-dashoffset, 900 ms, deck → piers →
  cable → hangers), kept from the previous spec.
- The backdrop is **static** (Landau's CPU-renderer policy: nothing full-viewport animates).
- **Reduced effects (fx-lite)** — port of Ledger's `composables/effects.ts` + `plugins/effects.client.ts`
  as `app/composables/useEffects.ts` + `app/plugins/effects.client.ts`: class `fx-lite` on `<html>`
  disables blur, glows, grain, hover lift, reveal and pulse (spinners keep spinning); `.glass`
  becomes solid ink-800, `.glass-inset` solid ink-950. Chosen per device: `localStorage`
  `landau-bridge:reduced-effects`; auto-detected on first run — **software rasterizer (llvmpipe /
  swiftshader / softpipe / "software"), Pi-class GPU (videocore / v3d / broadcom) or ARM user
  agent → lite; otherwise full.** (Ledger's "≤ 4 cores" rule is dropped: a 4-core desktop composites
  blur fine.) Toggle in Settings → نمایش. `prefers-reduced-motion: reduce` kills all animation and
  transition durations globally, as both apps do.

## 3. Shell

### 3.1 Window and backdrop

- `tauri.conf.json` window `backgroundColor` → `#0b0d12`.
- `GlowBackdrop.vue` (fixed, `-z-10`, `pointer-events: none`), layers in order: base wash
  ink-950; crimson key glow — radial `closest-side, #d63e3e → transparent 70%`, 70 vw × 70 vh,
  centred 12 % outside the **inline-start** top corner (top-right in RTL), opacity .13; sapphire
  counter-glow — radial `#3a3adb`, 70 vw × 80 vh, 18 % outside the inline-end bottom corner,
  opacity .10; blueprint grid — 88 px major over 22 px minor, `rgb(255 255 255 / .04)`, masked
  `radial-gradient(ellipse 90% 70% at 50% 0%, black 30%, transparent 75%)`; film grain — inline SVG
  `feTurbulence` 0.82/3 octaves, opacity .05, **no** `mix-blend-mode`; vignette —
  `radial-gradient(ellipse 120% 90% at 50% 40%, transparent 55%, rgb(4 5 9 / .55))`. All but the
  wash are skipped under fx-lite.

### 3.2 Sidebar (`layouts/default.vue`)

- `aside`, 240 px, on the inline-start (right), full height, `bg-ink-900/40 backdrop-blur-xl`,
  1 px `line` border on its inline-end edge. Padding: brand 20 px inline / 24 px top; nav 12 px.
- **Brand block**: a 36 px tile (12 px radius, `linear-gradient(135deg, sapphire-500, sapphire-800)`,
  glow shadow `0 0 44px rgb(79 85 240 / .18)`) holding the bridge mark at 20 px in white; to its
  side the wordmark «پل لاندا» in Mikhak 1.05 rem 700 and, beneath, «بارزپخش ← لاندا» 11 px
  paper-mute. `BrandLogo.vue` keeps its geometry (deck + towerless bowl + three dots) and gains a
  `tone` prop: `mono` (all `currentColor`, used in the tile) and `brand` (bowl stroke
  `linear-gradient(sapphire-400 → sapphire-700)` through an SVG `<linearGradient>` whose id comes
  from `useId()` so several marks on one page keep their own fill — gradient stops styled via CSS
  classes, not `stop-color` attributes, as Landau's `Mark.vue` does —, deck paper-dim, dots
  crimson-400; used in the setup checklist and empty states at 96 px). The `public/brand/*.svg` and `src-tauri/icons`
  are regenerated from the `brand` tone on the ink-950 ground (`pnpm tauri icon`).
- **Nav**: 7 `NuxtLink`s, `.nav-item` (flex, gap 11 px, 12 px radius, padding 9 × 13 px, 13.6 px
  Estedad 500, paper-dim; hover paper on `rgb(255 255 255 / .04)`; active as §2.1). Lucide icons
  17 px via `UIcon` (bundled): `layout-dashboard`, `file-input`, `refresh-cw`, `link`, `history`,
  `triangle-alert`, `settings`. Labels: داشبورد · وارد کردن فاکتورها · به‌روزرسانی قیمت‌ها · تطبیق‌ها ·
  عملیات‌ها · خطاها و هشدارها · تنظیمات. `isActive`: exact for `/`, prefix otherwise (`/jobs/42`
  lights «عملیات‌ها»).
- **Foot** (padding 20 px): when a job is running or in review, `RunningJobCard` (glass-inset, 14 ×
  12 px: crimson `.dot`, «وارد کردن فاکتورها ۴۳» 12 px paper-dim 500, phase/«در انتظار تصمیم» 11 px
  paper-mute, chevron; links to `/import?job=ID` or `/refresh`). Otherwise a status line in the same
  glass-inset: sage dot + «هر دو کرانه وصل است» + the store name, 11 px paper-mute, or amber dot +
  «تنظیمات ناقص» + «رفتن به تنظیمات ←». Below it, «نسخهٔ ۰.۱.۰» (Persian digits, `faVersion`) in
  11 px paper-mute. The layout polls `jobCurrent()` every 3 s as today and `settingsGet()` once
  (and again after a `settings-saved` event emitted by the settings page through a tiny
  `useState` flag) for the status line.
- `ThemeToggle.vue` is deleted.

### 3.3 Content column and page header

- `main`: `min-w-0 flex-1 overflow-y-auto`; inner `mx-auto max-w-[1060px] px-10 pt-10 pb-20`.
- `PageHeader.vue` props `kicker`, `title`, `lede?`, slot `actions`: `<header class="mb-10">`,
  kicker `.kicker mb-3`, `h1.display text-[2.4rem] leading-[1.2] text-paper`, then a row `mt-4 flex
  items-end justify-between gap-6` with the lede (`max-w-[58ch] text-[0.95rem] leading-relaxed
  text-paper-dim`) and the actions (`shrink-0 flex items-center gap-2`). Every page uses it; every
  page's first content block starts 40 px below the header and blocks stack with 24 px gaps.
- Toasts: `<UApp :locale="fa_ir" :toaster="{ position: 'bottom-left', duration: 4200 }">`; toast
  slots restyled to glass with a leading dot coloured by `color` (§7).

### 3.4 Alignment rules (one set, every page)

- 4 px grid; spacing steps 4 / 8 / 12 / 16 / 24 / 32 / 40; nothing in between.
- Card padding 24; glass-inset padding 14 × 12; ledger head cells 10 × 14, body cells 12 × 14;
  `StatTile` 16 × 14.
- Heights: controls 36 (`sm` 30, `lg` 44); pills 22; segmented 32 (buttons 28); nav items 38.
- Cards in one grid row are equal height (`grid` + `flex flex-col` with the footer `mt-auto`).
- Header actions sit at the **inline-end** (left); form submits sit at the **inline-start** (right)
  under their form, as Ledger's `FnForm` does; a card's own action sits at its inline-end footer.
- Table rows: pills and numbers `align-top`; numeric columns `whitespace-nowrap`; the last column
  of a clickable row is a 16 px chevron, `text-end`.
- Big numbers share a baseline across a card row (same font size and line-height per row).
- Verified with the `?grid=1` overlay (§8) on every screenshot: column edges, card edges and
  baselines must fall on the grid.

## 4. Pages

Kickers («name · index»), titles (Mikhak) and ledes are fixed copy:

| Route | Kicker | Title | Lede |
| --- | --- | --- | --- |
| `/` | داشبورد · ۰۱ | دو کرانه، یک قفسه. | فاکتورهای بارزپخش خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قواعد خودتان روی قفسهٔ «{store}» می‌نشیند. (no store: «…روی قفسهٔ لاندا می‌نشیند.») |
| `/import` | وارد کردن فاکتورها · ۰۲ | فاکتورها، روی قفسه. | فاکتورهای پرداخت‌شده خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قیمت روز روی قفسه می‌نشیند. |
| `/refresh` | به‌روزرسانی قیمت‌ها · ۰۳ | قیمت‌ها، به روز. | قیمت خرید هر ردیف از قیمت روز بارزپخش تازه می‌شود و قواعد قیمت‌گذاری لاندا دوباره اجرا می‌شود. |
| `/matches` | تطبیق‌ها · ۰۴ | هر کد، یک قطعه. | هر کد بارزپخش یک بار به یک قطعهٔ لاندا وصل می‌شود و دیگر پرسیده نمی‌شود. |
| `/jobs` | عملیات‌ها · ۰۵ | هر عبور، ثبت‌شده. | هر وارد کردن و به‌روزرسانی، با نتیجه و رویدادهایش. |
| `/jobs/:id` | عملیات · {id} | kind label (§6 `jobKindLabel`) | «شروع {date} · پایان {date} · {duration}» or «در حال اجرا از {date}» |
| `/errors` | خطاها و هشدارها · ۰۶ | هر لغزش، با جزئیات. | هر خطا یا هشداری که در اجرا ثبت شده، با نوع، عملیات و جزئیات فنی. |
| `/settings` | تنظیمات · ۰۷ | دو کرانه را وصل کنید. | کلید لاندا و ورود بارزپخش، و چند قاعدهٔ وارد کردن. هر چیز دیگری پیش‌فرض خوبی دارد. |

### 4.1 Dashboard `/`

Data: `dashboardGet()`, `jobsList()` (first 6), and — while `running` is set — `useJob().follow(running.id)`;
on `done` the page reloads the dashboard and the list.

Header actions: `btn-primary` «وارد کردن فاکتورها» → `/import`, `btn-ghost` «به‌روزرسانی قیمت‌ها»
→ `/refresh`. While a job runs: one `btn-ghost` «رفتن به عملیات جاری» → its page.

1. **Status band** — `grid gap-4 md:grid-cols-4`, four `GlassCard hover-glow` (each: a top row with
   `data-label` and a `StatusPill`; a display value; a 12 px sub-line; an optional 12 px link at the
   bottom, `mt-auto`):
   - **بارزپخش**: pill sage «آماده» when `supplier_user` and the password are set, else amber
     «تنظیم‌نشده». Value: `invoices_cached` (Mikhak 1.75 rem) + «فاکتور ذخیره‌شده». Sub-line:
     «{n} پرداخت‌نشده» in amber when `invoices_unpaid > 0`, else «همه پرداخت‌شده» paper-mute; then
     the username, mono 11 px, `dir="ltr"`.
   - **لاندا**: pill sage «آماده» / amber «تنظیم‌نشده». Value: `store_name` on the same 1.75 rem display line as the
     numbers (truncated), so the four values share one baseline. Sub-line: «{decided_matches} تطبیق ثبت‌شده». Link «تطبیق‌ها ←» → `/matches`.
   - **در انتظار شما**: value `pending_review` + «قلم منتظر تصمیم». Pill amber «نیاز به تصمیم» when
     > 0, else sage «چیزی نیست». Link «ادامهٔ بازبینی ←» → `/import?job={running.id}` when
     `running.status === 'REVIEW'`.
   - **سلامت**: value `recent_errors` + «خطا و هشدار در هفت روز». Pill sage «پاک» when 0, else
     rust «بررسی کنید». Link «خطاها ←» → `/errors`.
2. **«عبور» card** — one `GlassCard :pad="false"`. Top: `BridgeBand` (the bridge SVG from the
   previous `BridgeHero`, deck/piers sapphire-500, hangers sapphire-300, cable crimson-400, 72 px
   tall, full width, drawn once). Body (`px-6 py-5`):
   - **running or review**: `data-label` «در حال اجرا» (or «در انتظار تصمیم شما»), h2 Mikhak
     «{kind} {id}», `JobProgress` (piers, progress bar + message + «{done} از {total}», status pill),
     the last three events (`EventTimeline :limit="3"`), and actions: `btn-primary` «مشاهده» → its
     page, `btn-danger` «توقف» (`jobCancel`). In review: «ادامهٔ بازبینی» instead of «مشاهده».
   - **idle**: `grid gap-6 md:grid-cols-2`, each column a mini job sheet: `data-label` («آخرین وارد
     کردن» / «آخرین به‌روزرسانی قیمت»), a row with `StatusPill` and `isoToLocal(started_at)` and
     `faDuration`, then `StatTile`s in a `grid grid-cols-3 gap-2` from `summaryTiles(job)` (§6), and
     a footer link «برگهٔ عملیات ←» → `/jobs/{id}`. Never run: «هنوز اجرا نشده» paper-mute and a
     `btn-ghost` to the page.
3. **فعالیت اخیر** — `GlassCard :pad="false"`; head row `px-6 pt-5 pb-3` with h2 «فعالیت اخیر» and
   «شش عملیات آخر» 12 px paper-mute; `ul divide-y divide-white/5`, rows `px-6 py-3 flex gap-4`:
   status dot (sage DONE / rust FAILED / amber REVIEW-RUNNING / graphite else), `jobKindLabel` +
   mode and «از {since}» `<bdi dir="ltr">` on a second 12 px line, `isoToLocal(started_at)`,
   `faDuration`, `summaryLabel(job)` 12 px paper-mute (truncated), chevron; row → `/jobs/{id}`.
   Footer `px-6 py-3` link «همهٔ عملیات‌ها ←». Empty: «هنوز عملیاتی اجرا نشده.»
4. **Setup incomplete** (`!settings_complete`): header + `SetupChecklist` only (glass, 96 px
   `BrandLogo tone="brand"`, h2 «سه قدم تا اولین عبور», the three steps with `.check` rings —
   done = sage fill —, `btn-primary` «رفتن به تنظیمات»). None of 1–3 renders.
5. **AppImage offer**: a `glass-inset` banner between header and band, copy and buttons as today
   (`btn-primary` sm «نصب», `btn-ghost` sm «نه، دیگر نپرس»).
6. Reserved for the next spec: a third line under the idle columns («به‌روزرسانی خودکار: …») and a
   fifth nav-less status in the sidebar foot. Not rendered now.

### 4.2 Import `/import`

- Start state (`!jobId || finished`): `grid gap-4 md:grid-cols-2` of two **tickets** (`label.ticket`:
  `.glass` + `cursor-pointer`; selected: border `crimson-500`, `box-shadow: inset 0 0 0 1px
  crimson-500, var(--shadow-card)`; check icon crimson-400 at the inline-end top). Ticket 1 «با تعداد،
  از تاریخ»: description, «از تاریخ» field (`.field w-40 dir=ltr`, invalid → rust border), the
  «فقط فاکتورهای از این تاریخ خوانده شود» switch row (label + hint | `USwitch`) — this switch is the
  only place `fetch_since_only` is edited; it is persisted with `settingsSave` when the job starts,
  as `refresh.vue` does with `overwrite`. Ticket 2 «با تعداد صفر، برای بازبینی». Under the grid: the
  pricing note (58 ch) and `btn-primary lg` «شروع وارد کردن» at the inline-start.
- **Smart default date**: `since` initialises to `nextSinceDate(dashboard.last_import)` (§6) instead
  of the Jalali year start.
- **First-run path** (`decided_matches === 0 && last_import === null`): a `glass-inset` block above
  the tickets, `data-label` «مسیر پیشنهادی برای بار اول» and two numbered lines: «۱. همهٔ
  فاکتورهای قدیمی را با تعداد صفر بیاورید و تطبیق‌ها را یک بار تصمیم بگیرید.» «۲. از فردا، «با
  تعداد، از تاریخ» را اجرا کنید؛ تاریخ شروع خودش پیشنهاد می‌شود.» with a `btn-ghost sm` «انتخاب
  حالت بازبینی» that selects ZERO.
- Running: `GlassCard` «عملیات {id}» (head row with h2 and `StatusPill`) → `JobProgress`. Header
  action `btn-danger` «توقف».
- Review: h2 «بازبینی اقلام تطبیق‌نشده ({n})» + `btn-primary` «ادامه…» at the inline-end of that
  row; `ReviewTable` items as `GlassCard`s (head: name + code/qty/«{i} از {n}»; actions: authenticity
  select sm, `btn-ghost sm` «بعداً», `btn-danger sm` «هیچ‌کدام»; body: search row `.field` +
  `btn-ghost` «جست‌وجو»; candidates `divide-y` rows with `SimilarityBar` and `btn-primary sm`
  «همین است»).
- Finished: `StatTile` row from `summaryTiles`, h2 «گزارش», `ReportTable`, and a link «برگهٔ
  عملیات ←» → `/jobs/{id}`.

### 4.3 Refresh `/refresh`

One `GlassCard`: the explanation (58 ch), a settings-style row «قیمت‌های دستی لاندا را هم بازنویسی
کن» (label + hint | `USwitch`; the only place `overwrite_manual` is edited; persisted on start as
today), `btn-primary lg` «شروع به‌روزرسانی» at the inline-start. Running / finished as §4.2 with
`kind="REFRESH"`; finished shows `summaryTiles` (ردیف‌ها / تغییر قیمت / قیمت دستی حفظ‌شده).

### 4.4 Matches `/matches`

Toolbar `mb-4 flex items-center gap-3`: `SegmentedControl` (همه / خودکار / انتخاب شما / در لاندا نیست)
at the inline-start, «{n} تطبیق» 12 px paper-mute, `.field w-72` search with a lucide `search` icon
at the inline-end. `LedgerTable` (glass): نام در فاکتور · کد (`<bdi dir=ltr>`) · قطعهٔ لاندا · نحوه
(`StatusPill`: info «خودکار», ok «انتخاب شما», danger «در لاندا نیست») · شباهت (`SimilarityBar`) ·
تاریخ · `btn-ghost sm` «تغییر». Empty states as today in a `GlassCard`. The edit `UModal` is glass
(§7): name + code, search row, candidates list (max-h 18 rem), footer `btn-danger` «در لاندا نیست»
and `btn-ghost` «حذف تصمیم، دوباره پرسیده شود».

### 4.5 Jobs `/jobs`

Data `jobsList()`. Toolbar: `SegmentedControl` (همه / وارد کردن / به‌روزرسانی) and «{n} عملیات».
`LedgerTable`, rows clickable (`cursor-pointer`, hover ink-800/55, chevron column) → `/jobs/{id}`:
`#` (Persian digits) · نوع (`jobKindLabel`; second 12 px line «از {since}» `<bdi dir=ltr>` for
QUANTITY imports) · شروع (`isoToLocal`) · مدت (`faDuration`) · وضعیت (`StatusPill` by
`statusTone`) · مرحله (`phaseLabel`) · نتیجه (`summaryLabel`, 12 px paper-dim, truncated) · chevron.
Empty: `EmptyState` «هنوز عملیاتی اجرا نشده». No header actions. Files: `app/pages/jobs/index.vue`
and `app/pages/jobs/[id].vue`.

### 4.6 Job sheet `/jobs/[id].vue`

Data `jobGet(id)`; if the job is RUNNING/REVIEW, `useJob().follow(id)` keeps it live. Header actions:
`btn-ghost` «همهٔ عملیات‌ها» → `/jobs`, `btn-ghost` «باز کردن پوشهٔ لاگ».

1. **`JobSheet`** (`GlassCard :pad="false" overflow-hidden`):
   - Header strip `px-6 py-4 border-b line`, tinted `bg-{tone}/6`: a 32 px circle (`bg-{tone}/15
     text-{tone}`) with check / cross / clock / dash icon; «{statusLabel}» 14 px 500 and
     «مرحله: {phaseLabel}» 12 px paper-mute; at the inline-end: «عملیات {id}» and
     `faDuration` tabular; the id line is 12 px paper-mute.
   - Body `space-y-5 px-6 py-5`: a `dl grid grid-cols-2 sm:grid-cols-3 gap-x-8 gap-y-3` of facts
     (`data-label` + value 13 px paper-dim): حالت, از تاریخ (`<bdi dir=ltr>`), فروشگاه (name from
     settings if the id matches, else the id), بازنویسی قیمت دستی (بله/خیر), فاکتورها (count of
     `summary.invoice_ids`), شروع, پایان. Then `StatTile`s in `grid grid-cols-3 md:grid-cols-6 gap-2`
     from `summaryTiles`. Running/review: `JobProgress` replaces the tiles.
2. **رویدادها** — `GlassCard :pad="false"`, head row h2 + «{n} رویداد»; `EventTimeline` (`max-h-96
   overflow-auto`): rows `px-6 py-3 flex gap-3`: level dot (INFO graphite-400 / WARN amber / ERROR
   rust), 12 px paper-mute «{isoToLocal(at)} · {phaseLabel}», message 14 px (WARN amber, ERROR rust);
   the REPORT event whose message parses as JSON is rendered through `summaryLabel`; `detail`, when
   present, under a `<details>` «جزئیات فنی» as a `.code-block` `dir="ltr"`.
3. **ردیف‌ها** — h2 + `ReportTable` (§5) full width. Empty rows: «این عملیات ردیفی ندارد».
4. `<details>` «خلاصهٔ خام» → `.code-block` JSON, 12 px.

Unknown id → `ErrorBanner` from the IPC error and the «همهٔ عملیات‌ها» action.

### 4.7 Errors `/errors`

Data `errorsList(300)`. Header actions: `btn-ghost` «تازه‌سازی», `btn-ghost` «باز کردن پوشهٔ لاگ».
Toolbar: `SegmentedControl` (همه / بارزپخش / لاندا / برنامه via `kindFamily`), `.field w-72` search
over message and kind label, and when `?job=` is present a chip «فقط عملیات {id}» with an ✕ that
clears the query. `LedgerTable` with roomier cells (`py-3.5`): زمان · عملیات (`StatusPill muted`
«{id}» linking to `/jobs/{id}`, or «—») · نوع (`StatusPill muted` `kindLabel`) · پیام (wide,
`break-words`) · a 16 px chevron that toggles the row's detail. Expanded: a second `tr` with one
full-width cell holding a `glass-inset` «جزئیات فنی» `.code-block` `dir="ltr"` of `context`; rows
without context have no chevron. Empty: `EmptyState` «خطا یا هشداری ثبت نشده».

### 4.8 Settings `/settings`

Header action: `btn-primary` «ذخیرهٔ تنظیمات» (Ledger's placement; the sticky bottom bar goes).
`max-w-3xl` column of `GlassCard`s, 24 px apart, each with h2 (Mikhak 20 px) and, where relevant, a
`StatusPill` at the head's inline-end:

1. **لاندا** — pill: ok «متصل به {business}» / muted «کلید ذخیره‌شده» / warn «آزمایش‌نشده». Fields
   (`UFormField` + `UInput` restyled to `.field`): «کلید API لاندا» (password, `dir=ltr`,
   hint as today); `btn-ghost` «بررسی اتصال و دریافت فروشگاه‌ها»; the pricing-rules note (amber when
   0 rules); «فروشگاه مقصد» select.
2. **بارزپخش** — pill ok «ورود موفق» / muted «رمز ذخیره‌شده» / warn «آزمایش‌نشده»; «نام کاربری»,
   «کلمه عبور», `btn-ghost` «آزمایش ورود».
3. **قواعد وارد کردن** — «اصالت پیش‌فرض ردیف‌های تازه» select with items «اصلی» / «غیراصلی (کپی)»
   (no Latin codes); «نرخ ارزش افزوده (٪)» number `w-32`. (`overwrite_manual` and
   `fetch_since_only` are **not** here — §10.)
4. **نمایش** — one switch row «جلوه‌های بصری کمتر» + hint «پنل‌های مات به جای شیشه، بدون هاله و
   انیمیشن؛ برای رایانه‌های ضعیف. خودکار تشخیص داده می‌شود و روی همین دستگاه ذخیره می‌ماند.» bound
   to `useReducedEffects`.
5. **پیشرفته** — a collapsed `<details>` inside the card (summary «تنظیمات پیشرفته» + chevron):
   «نشانی درگاه لاندا» (`dir=ltr`, hint «پیش‌فرض: https://api.landau.ir») and «آستانهٔ تطبیق خودکار»
   (0–100, hint as today). Opens automatically when the saved URL is not the default.
6. **داده‌ها** — `GlassCard :pad="false" px-6 py-4`: `data-label` «پوشهٔ داده» + path in mono 12 px
   paper-dim `dir=ltr` (click copies, toast «کپی شد»); second line the secret-backend note (amber
   when `file`).

Save behaviour unchanged (`settingsSave` with the whole form; token/password cleared after save;
the «بررسی اتصال» / «آزمایش ورود» flows unchanged). Saving sets the shared `settings-saved` flag so
the sidebar status refreshes.

### 4.9 Empty and error states

- `EmptyState` (`art="bridge" | "shelf"`): line art stroke graphite-600 with one sapphire-500 line,
  title 15 px 600 paper, description 13 px paper-mute, optional action. Always inside a `GlassCard`.
- `ErrorBanner`: `.glass` with `border-inline-start: 3px solid rust-400`, `px-4 py-3`, title rust
  600, message 13 px paper-dim, and `detail` (when different) under `data-label` «جزئیات فنی» in
  mono 12 px paper-mute `dir="ltr"`.

## 5. Components (`app/components`)

| Component | Props / notes |
| --- | --- |
| `GlowBackdrop` | none; reads `useReducedEffects` |
| `GlassCard` | `pad?: boolean = true`; `<section class="glass">` |
| `PageHeader` | `kicker, title, lede?`; slot `actions` |
| `StatusPill` | `tone: 'ok'│'warn'│'danger'│'info'│'muted'│'brand'`, `label`; 22 px, dot + 11 px text |
| `StatTile` | `label, value: number │ string, hint?, tone?`; numbers go through `faNumber`; `glass-inset` 16 × 14, `data-label`, Mikhak 1.5 rem tabular |
| `BridgeBand` | none; the bridge SVG, draws on mount |
| `JobSheet` | `job: JobRow, items, events, live?: { phase, progress, events, status }`; §4.6 |
| `EventTimeline` | `events: (JobEvent │ JobEventRow)[], limit?`; live `JobEvent`s have no `at`, so their meta line shows the phase only |
| `JobProgress` | as today; pier colours: done sapphire-400, current crimson-400 breathing, failed rust-400, todo line-bright; bar `UProgress` restyled to a crimson gradient |
| `RunningJobCard` | `job, compact?`; glass-inset |
| `SetupChecklist` | `settings`; glass |
| `LedgerTable` | slot; glass container, head ink-800, hairline rows, hover ink-800/55, `[data-selected]` row: ink-700 + 3 px crimson-500 start bar |
| `ReportTable` | `items, kind`; filters همه / انجام‌شده / تغییر قیمت / ردشده / ناموفق; search `.field` over name/code; new column «تغییر قیمت»: `priceDelta(i)` → «▲ ۶٫۷٪» amber / «▼ ۳٫۱٪» sapphire-300 / «—»; CSV gains a «تغییر قیمت» column with the signed percent |
| `ReviewTable` | as today, glass cards |
| `SegmentedControl` | as today; `glass-inset` track, active button ink-700 + paper, 12 px radius inner 10 |
| `SimilarityBar` | track ink-700; fill graphite-400 / amber-400 / sage-400 by `similarityTone` |
| `EmptyState`, `ErrorBanner`, `BrandLogo` | §4.9, §3.2 |

Removed: `ThemeToggle`, `BridgeHero` (its SVG moves into `BridgeBand`, its numbers into the status
band), `pages/logs.vue`.

Buttons: `.btn-primary`, `.btn-ghost`, `.btn-danger` utility classes (Ledger's rules, crimson/rust
values) for plain `<button>`/`NuxtLink`; `UButton` is kept where a `loading` state or `to` prop is
convenient and is themed to the same three looks (§7). Sizes: default 36 px, `sm` 30, `lg` 44.

## 6. Utilities (`app/utils/fa.ts`, tested in `fa.spec.ts`)

- `faDuration(startIso, endIso?: string | null, now = Date.now()) → string`: `< 60 s` → «کمتر از یک
  دقیقه»; `< 60 min` → «{m} دقیقه»; else «{h} ساعت و {m} دقیقه» (drop «و {m} دقیقه» when m = 0).
  Persian digits.
- `faVersion('0.1.0') → '۰.۱.۰'` (Persian digits, plain periods).
- `jobKindLabel(job) → string`: IMPORT+QUANTITY «وارد کردن با تعداد», IMPORT+ZERO «وارد کردن با
  تعداد صفر», REFRESH «به‌روزرسانی قیمت‌ها».
- `statusTone(status) → StatusPill tone`: DONE ok, FAILED danger, REVIEW warn, RUNNING info,
  CANCELLED/INTERRUPTED muted, else muted.
- `summaryTiles(job) → { label, value }[]`: IMPORT from `summary.items/attached/patched/skipped/failed`
  → اقلام / افزوده / به‌روز / ردشده / ناموفق (+ انجام‌شده when `done` present); REFRESH from
  `rows/changed/skipped_manual` → ردیف‌ها / تغییر قیمت / قیمت دستی حفظ‌شده. Missing keys → tile omitted;
  `summary` null → `[]`.
- `summaryLabel(job | { kind, summary }) → string`: the tiles joined with « · », zero-valued parts
  omitted except the first; `''` when no summary.
- `reportEventText(e) → string`: for `phase === 'REPORT'` whose message is JSON, `summaryLabel`
  of it prefixed «گزارش نهایی: »; otherwise the message unchanged.
- `kindFamily(kind) → 'supplier' │ 'landau' │ 'app'`: `Supplier*`, `NoCurrentPrice`, `Invoice*`,
  `LineNonPositiveQty`, `VatRateMismatch` → supplier; `Landau*`, `BusinessNotApproved`, `NoStore`,
  `ProductNotApproved`, `DuplicateRow`, `MirrorTimeout`, `NoPricingRule`, `RepriceSkipped` → landau;
  everything else → app.
- `nextSinceDate(lastImport: JobRow | null, today = todayJalali()) → string`: the Jalali date of
  `lastImport.finished_at` (via `@internationalized/date`, Asia/Tehran) when the last import is a
  finished QUANTITY import, else `jalaliYearStart()`. (Invoices paid on the last run's day and
  imported already are skipped by the core anyway — «هر فاکتور فقط یک بار منتقل می‌شود».)
- `priceDelta(item) → { pct: number, dir: 'up' │ 'down' │ 'flat' } | null`: from
  `price_before`/`price_after`; null when either is null; `faPercent` with one decimal.

## 7. `@nuxt/ui` theming (`app.config.ts`)

`ui.colors` as §2.1. Slots (arbitrary values so `tailwind-merge` replaces the defaults):

- `button`: `base: 'font-medium rounded-xl transition-all duration-300 [transition-timing-function:var(--ease-swift)] active:translate-y-px'`;
  `variants.variant.solid` (primary) → the crimson gradient, white text, inset highlight, glow;
  `outline` → `.btn-ghost` look; `soft` with `color: error` → `.btn-danger` look; `ghost` → text
  paper-dim, hover paper. `defaultVariants.size: 'md'`.
- `input`, `select`, `textarea`: `base: 'rounded-xl bg-ink-900/80 border border-line-bright text-paper placeholder:text-paper-mute/60 focus:border-[rgb(224_110_110/.55)] focus:ring-[3px] focus:ring-[rgb(224_110_110/.12)]'`;
  select menu `content: 'glass'`.
- `formField`: label 13 px paper, hint/help 12 px paper-mute.
- `switch`: track ink-800 / line-bright, checked `bg-[rgb(214_62_62/.3)] border-[rgb(214_62_62/.6)]`, thumb paper-mute → crimson-300.
- `modal`: `overlay: 'bg-ink-950/70 backdrop-blur-sm'`, `content: 'glass rounded-[18px]'`, header
  h2 Mikhak.
- `progress`: indicator `bg-[linear-gradient(90deg,#d63e3e,#a30b0b)]`, track ink-700.
- `badge`: unused after this spec (every badge is a `StatusPill`).
- `toast`: `root: 'glass'`, icon replaced by a dot in `color`, title 13 px paper-dim.
- `nuxt.config.ts`: `ui: { colorMode: false }`; `app.head.htmlAttrs: { dir: 'rtl', lang: 'fa', class: 'dark' }`;
  `meta color-scheme: dark`; `pageTransition: { name: 'page', mode: 'out-in' }`; the `colorMode`
  block and `~/dev/devQuery.client` mode switching are removed (the plugin keeps `?grid=`, §8).

## 8. Dev tooling

- `app/dev/mockBridge.ts`: fixtures cover `jobs_list` (6 jobs incl. a REFRESH with summary), `job_get`
  for 42 (done), 40 (failed), 43 (running/review), 44 (refreshing), `errors_list` with contexts,
  and `?state=` as today; `?appimage` keeps showing the offer.
- `app/dev/devQuery.client.ts`: dev-only, inert in Tauri; `?grid=1` mounts a fixed overlay
  (`pointer-events: none`, z 60): 8 px grid lines `rgb(255 255 255 / .04)`, 40 px lines `.08`, and
  the 1060 px column edges (accounting for the 240 px sidebar) in `rgb(214 62 62 / .35)`; `?fx=lite`
  forces reduced effects for that page load without persisting the choice.
- `scripts/shots.sh OUT_DIR [filter] [--grid]`: dark only, pages `/?state=done /?state=empty
  /?state=running /?state=review /import?state=running /import?state=review /import?state=done
  /refresh?state=done /refresh?state=refreshing /matches?state=done /matches?state=empty
  /jobs?state=done /jobs/42?state=done /jobs/43?state=running /errors?state=done
  /errors?state=done&job=42 /settings?state=done`, 1200 × 800, `--force-prefers-reduced-motion`;
  `--grid` appends `&grid=1`. Chrome flags as today (`--no-proxy-server`).
- `docs/RUNBOOK.md` «Design review without Tauri» updated (no `mode=`, new pages, `grid=`).

## 9. Persian audit

Scanned: every template and script string under `app/`, `src-tauri/src/errors.rs` (`message_fa`),
`src-tauri/src/jobs/*` (events carry Persian messages; errors go through `message_fa`). Findings,
all fixed by this spec: the REPORT event shows raw JSON (§6 `reportEventText`); the authenticity
select labels carry Latin codes (§4.8); the sidebar version is «v0.1.0» (`faVersion`); no Persian
duration formatter existed (`faDuration`). Kept deliberately: `detail`/`context` (HTTP bodies,
paths) as LTR mono under the Persian label «جزئیات فنی»; the username and data path as LTR mono;
«API», «CSV», «AppImage» as proper nouns inside Persian sentences. Every new string in this spec is
Persian; `fa_ir` covers `@nuxt/ui`'s own strings.

## 10. The retailer's view — what moves, what goes

| Setting / feature | Decision | Why |
| --- | --- | --- |
| `landau_url` | → پیشرفته (collapsed) | A seller never changes the gateway address. |
| `threshold` | → پیشرفته | Calibrated on real data (`docs/matching-threshold.md`); a knob for support, not for the seller. |
| `overwrite_manual` | Removed from Settings; lives on `/refresh` only | It is a per-run decision; two places drifted. The saved value stays the page's default. |
| `fetch_since_only` | Removed from Settings; lives on the import ticket only | Same: decided where it applies. |
| `vat_percent`, `default_authenticity` | Stay visible | Business facts the seller must find when the law or the supplier changes. |
| `data_dir`, secret backend | Stay, as the quiet last card | Support information. |
| «باز کردن پوشهٔ لاگ» | Only on `/errors` and `/jobs/:id` | Where a support call starts. |
| Raw summaries / technical details | Kept, collapsed | Support, never in the way. |
| Sticky save bar | Replaced by the header button | Ledger's pattern; one save control. |

## 11. Follow-up spec (not in scope here): automatic price refresh

The seller's real need is the latest market price without doing anything. That requires the Rust
side: a scheduler (daily at a chosen time or every N hours; run at launch; skip when a job is
running or settings are incomplete), close-to-tray and start-at-login (Ledger has both), and
desktop notifications when a run finishes, fails, or waits for decisions. This spec reserves the
UI slots (§4.1 item 6; a settings card «به‌روزرسانی خودکار» between «بارزپخش» and «قواعد وارد
کردن») and adds nothing that would need reflowing later.

## 12. Verification

1. `pnpm typecheck`, `pnpm test` (rewritten `palette.spec.ts`, extended `fa.spec.ts`).
2. `pnpm dev` + `scripts/shots.sh` for every page/state, once plain and once with `--grid`; each
   still is critiqued against §2 and §3.4 (contrast, alignment, one hot element per page, no
   Latin leak) before the next page is touched; the set is shown to the user.
3. fx-lite: `?fx=lite` stills of dashboard and job sheet; reduced motion via Chrome's flag.
4. `pnpm tauri icon` regenerated icons; `pnpm tauri dev`: window colour, sidebar, every route; the
   seller's flows exercised once (import in both modes incl. the switch, review, refresh, match edit,
   settings save incl. advanced, jobs list → sheet → errors chip, AppImage banner on Linux).
5. Bundle check: `pnpm generate` output contains no `Vazirmatn`, no `mockBridge`, no Google fonts.

## 13. Out of scope

- Any change under `src-tauri/src`, to IPC command names/payloads, or to `app/types/bridge.ts`.
- Light mode; responsive layout below the 960 px window minimum; sidebar collapse.
- Automatic refresh, tray, autostart, notifications (§11).
- A shelf browser, unpaid-invoice list, cost-before column (each needs a new command).
