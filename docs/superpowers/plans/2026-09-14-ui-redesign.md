# UI/UX Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the default-looking Nuxt UI scaffold with the «پل» design — turquoise/saffron tokens, a dark sidebar, a real logo and app icon, a bridge-shaped dashboard and progress stepper — without changing any behaviour behind the IPC boundary.

**Architecture:** Tokens live in `app/assets/css/main.css` (Tailwind 4 `@theme static` + `--ui-*` overrides) and are mapped to Nuxt UI aliases in `app/app.config.ts`. Reusable pieces (`BrandLogo`, `PageHeader`, `LedgerTable`, `SegmentedControl`, `SimilarityBar`, `EmptyState`, `RunningJobCard`, `ThemeToggle`, `BridgeHero`, `SetupChecklist`) are small single-purpose components; every page keeps its `<script setup>` data flow and only swaps its template. A dev-only fixture mode lets headless Chrome render every page without Tauri so each visual task ends with screenshots.

**Tech Stack:** Nuxt 4 (`app/` srcDir, `ssr: false`), Nuxt UI 4.11 (Tailwind 4, `@nuxtjs/color-mode` bundled), Vue 3.5, Tauri 2 CLI (`tauri icon`), vitest 5, Google Chrome headless for screenshots, `rsvg-convert` for logo previews.

**Spec:** `docs/superpowers/specs/2026-09-14-ui-redesign-design.md` — read it first; the plan argues from it.

## Global Constraints

- Nothing under `src-tauri/src`, no IPC command name/payload change, no edit to `app/types/bridge.ts`.
- UI is Persian only, RTL (`<html dir="rtl" lang="fa">` stays). Latin data (codes, URLs, ISO dates, JSON) is isolated with `dir="ltr"` or `<bdi>`.
- Typeface: Vazirmatn only (`public/fonts/Vazirmatn[wght].woff2`, has `tnum`). Base 15 px, body line-height 1.7.
- Scales (hex, 50…950) exactly as in spec §2.1; Tailwind names: `firoozeh`, `zaferan`, `petrol`, `sabz` (the spec's success scale), `sorkh` (the spec's error scale).
- Nuxt UI surface mapping used throughout: `--ui-bg` = **surface** (white / petrol-900), `--ui-bg-muted` = **page** (`#F2F5F7` / `#0E1B24`), `--ui-bg-elevated` = **wells / table headers** (`#E9EEF1` / `#1C2F3B`). Utilities: `bg-default` surface, `bg-muted` page, `bg-elevated` well.
- Contrast ≥ 4.5:1 for text, ≥ 3:1 for large text and borders, in both modes (Task 1's test enforces the listed pairs).
- Saffron only for: logo dots, hero cable, the current stepper pier, `warning`/`secondary` semantics.
- Motion: dashboard draw-in once; stepper current pier breathes; everything instant under `prefers-reduced-motion: reduce`. No hover lift, no per-section fade.
- No eyebrow labels, no all-caps, no `A — B` joins in labels, no grid of identical cards.
- Copy: plain verbs, sentence case, one job per sentence; an action keeps its name from button to toast.
- CSP in `src-tauri/tauri.conf.json` stays: no external fonts/scripts; inline SVG and `data:` images are fine.
- Commit after every task with the attribution line: `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Screenshots go to the scratchpad directory (`$SCRATCH`, given in the session), never into the repo.

---

### Task 1: Design tokens, Nuxt UI wiring, contrast test

**Files:**
- Modify: `app/assets/css/main.css`
- Create: `app/app.config.ts`
- Modify: `nuxt.config.ts`
- Modify: `src-tauri/tauri.conf.json`
- Create: `app/utils/color.ts`, `app/utils/color.spec.ts`, `app/utils/palette.spec.ts`

**Interfaces:**
- Produces: CSS classes `.surface`, `.surface-head`, `.surface-body`, `.sidebar`, `.nav-item`, `.nav-item-active`, `.nav-icon`, `.theme-toggle`, `.theme-toggle-btn`, `.running-card`, `.running-dot`, `.segmented`, `.segmented-btn`, `.ticket`, `.ticket-check`, `.ledger`, `.stepper*`, `.sim-track`, `.sim-fill`, `.level-dot`, `.check`, `.draw` (all defined here so later tasks only consume them); Tailwind colour utilities `*-firoozeh-N`, `*-zaferan-N`, `*-petrol-N`, `*-sabz-N`, `*-sorkh-N`; `rounded-surface`, `rounded-control`, `shadow-float`; `contrastRatio(a: string, b: string): number` in `app/utils/color.ts`; `useRuntimeConfig().public.version: string`.

- [ ] **Step 1: Write the failing colour-util test**

`app/utils/color.spec.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { contrastRatio, hexToRgb, relativeLuminance } from './color';

describe('color', () => {
  it('parses 6-digit hex', () => {
    expect(hexToRgb('#0F8B9D')).toEqual([15, 139, 157]);
    expect(hexToRgb('ffffff')).toEqual([255, 255, 255]);
  });

  it('computes WCAG relative luminance', () => {
    expect(relativeLuminance([255, 255, 255])).toBeCloseTo(1, 5);
    expect(relativeLuminance([0, 0, 0])).toBe(0);
  });

  it('computes contrast ratio, order-independent', () => {
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 2);
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 2);
    expect(contrastRatio('#777777', '#FFFFFF')).toBeCloseTo(4.48, 1);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run app/utils/color.spec.ts`
Expected: FAIL — `Failed to resolve import "./color"`.

- [ ] **Step 3: Implement `app/utils/color.ts`**

```ts
export type Rgb = [number, number, number];

export const hexToRgb = (hex: string): Rgb => {
  const h = hex.replace('#', '');
  if (!/^[0-9a-fA-F]{6}$/.test(h)) throw new Error(`not a 6-digit hex colour: ${hex}`);
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
};

const channel = (c: number): number => {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};

/** WCAG 2.x relative luminance, 0 (black) … 1 (white). */
export const relativeLuminance = ([r, g, b]: Rgb): number =>
  0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);

/** WCAG contrast ratio between two hex colours, 1 … 21. */
export const contrastRatio = (a: string, b: string): number => {
  const la = relativeLuminance(hexToRgb(a));
  const lb = relativeLuminance(hexToRgb(b));
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
};
```

- [ ] **Step 4: Run the colour test**

Run: `pnpm vitest run app/utils/color.spec.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing palette test**

`app/utils/palette.spec.ts` reads the real stylesheet so the numbers in the spec are enforced, not copied:

```ts
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { contrastRatio } from './color';

const css = readFileSync(resolve(__dirname, '../assets/css/main.css'), 'utf8');
const scale = (name: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const m of css.matchAll(new RegExp(`--color-${name}-(\\d+):\\s*(#[0-9a-fA-F]{6})`, 'g'))) out[m[1]!] = m[2]!;
  return out;
};
const firoozeh = scale('firoozeh');
const zaferan = scale('zaferan');
const petrol = scale('petrol');
const sabz = scale('sabz');
const sorkh = scale('sorkh');

const WHITE = '#FFFFFF';
const SIDEBAR_TOP = '#0B1820';
const SIDEBAR_BOTTOM = '#10222D';
const SIDEBAR_TEXT = '#C6D4DB';
const LIGHT = { page: petrol['50']!, surface: WHITE, well: petrol['100']!, text: petrol['800']!, heading: '#13232C', muted: petrol['600']! };
const DARK = { page: petrol['950']!, surface: petrol['900']!, well: '#1C2F3B', text: '#E6EEF2', heading: '#F3F7F9', muted: petrol['400']! };

const atLeast = (fg: string, bg: string, min: number) =>
  expect(contrastRatio(fg, bg), `${fg} on ${bg}`).toBeGreaterThanOrEqual(min);

describe('palette', () => {
  it('defines all five scales with 11 stops', () => {
    for (const s of [firoozeh, zaferan, petrol, sabz, sorkh]) expect(Object.keys(s)).toHaveLength(11);
  });

  it('sidebar text is readable in every state', () => {
    atLeast(WHITE, firoozeh['600']!, 4.5); // active item
    atLeast(SIDEBAR_TEXT, SIDEBAR_TOP, 4.5);
    atLeast(SIDEBAR_TEXT, SIDEBAR_BOTTOM, 4.5);
    atLeast(petrol['400']!, SIDEBAR_BOTTOM, 4.5); // version / muted line
    atLeast(zaferan['300']!, firoozeh['600']!, 3); // active icon (large, non-text)
  });

  it('light surfaces carry readable text', () => {
    for (const bg of [LIGHT.page, LIGHT.surface, LIGHT.well]) {
      atLeast(LIGHT.text, bg, 4.5);
      atLeast(LIGHT.heading, bg, 4.5);
      atLeast(LIGHT.muted, bg, 4.5);
    }
  });

  it('dark surfaces carry readable text', () => {
    for (const bg of [DARK.page, DARK.surface, DARK.well]) {
      atLeast(DARK.text, bg, 4.5);
      atLeast(DARK.heading, bg, 4.5);
      atLeast(DARK.muted, bg, 4.5);
    }
  });

  it('semantic colours used as text or button fills pass', () => {
    // solid primary button: white on firoozeh-600 (light), petrol-900 on firoozeh-400 (dark)
    atLeast(WHITE, firoozeh['600']!, 4.5);
    atLeast(petrol['900']!, firoozeh['400']!, 4.5);
    // soft badges: the --ui-* text shade on the 50 stop (light) and on the dark surface (dark)
    atLeast(firoozeh['600']!, firoozeh['50']!, 4.5);
    atLeast(zaferan['800']!, zaferan['50']!, 4.5);
    atLeast(sabz['700']!, sabz['50']!, 4.5);
    atLeast(sorkh['600']!, sorkh['50']!, 4.5);
    atLeast(petrol['600']!, petrol['100']!, 4.5);
    atLeast(firoozeh['400']!, DARK.surface, 4.5);
    atLeast(zaferan['300']!, DARK.surface, 4.5);
    atLeast(sabz['400']!, DARK.surface, 4.5);
    atLeast(sorkh['400']!, DARK.surface, 4.5);
    // borders against both pages
    atLeast(petrol['200']!, LIGHT.page, 1.2);
    atLeast(petrol['800']!, DARK.page, 1.2);
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `pnpm vitest run app/utils/palette.spec.ts`
Expected: FAIL — scales have 0 stops (main.css has no `--color-firoozeh-*` yet).

- [ ] **Step 7: Replace `app/assets/css/main.css`**

```css
@import "tailwindcss";
@import "@nuxt/ui";

@font-face {
  font-family: 'Vazirmatn';
  src: url('/fonts/Vazirmatn[wght].woff2') format('woff2');
  font-weight: 100 900;
  font-display: swap;
}

/* ---- Tokens (spec §2) ---------------------------------------------------- */
@theme static {
  --font-sans: 'Vazirmatn', 'Tahoma', 'Segoe UI', sans-serif;

  --color-firoozeh-50: #E8F6F8;
  --color-firoozeh-100: #C9EAEF;
  --color-firoozeh-200: #98D7E0;
  --color-firoozeh-300: #5FBFCD;
  --color-firoozeh-400: #2FA6B8;
  --color-firoozeh-500: #0F8B9D;
  --color-firoozeh-600: #0C7383;
  --color-firoozeh-700: #0A5C69;
  --color-firoozeh-800: #084750;
  --color-firoozeh-900: #06353C;
  --color-firoozeh-950: #032126;

  --color-zaferan-50: #FDF5E6;
  --color-zaferan-100: #FBE8C4;
  --color-zaferan-200: #F7D18E;
  --color-zaferan-300: #F2BB5F;
  --color-zaferan-400: #EDAC46;
  --color-zaferan-500: #E8A33D;
  --color-zaferan-600: #C9862A;
  --color-zaferan-700: #A26A20;
  --color-zaferan-800: #7C5018;
  --color-zaferan-900: #573810;
  --color-zaferan-950: #332108;

  --color-petrol-50: #F2F5F7;
  --color-petrol-100: #E9EEF1;
  --color-petrol-200: #D5DDE2;
  --color-petrol-300: #B4C2CA;
  --color-petrol-400: #8FA3AE;
  --color-petrol-500: #6B808B;
  --color-petrol-600: #5B6B74;
  --color-petrol-700: #3C4C55;
  --color-petrol-800: #27404D;
  --color-petrol-900: #152530;
  --color-petrol-950: #0E1B24;

  --color-sabz-50: #E9F7F0;
  --color-sabz-100: #CDEEDD;
  --color-sabz-200: #9FDDBE;
  --color-sabz-300: #6CC79A;
  --color-sabz-400: #45B37F;
  --color-sabz-500: #2E9E6B;
  --color-sabz-600: #248257;
  --color-sabz-700: #1C6745;
  --color-sabz-800: #154F35;
  --color-sabz-900: #0F3A27;
  --color-sabz-950: #082418;

  --color-sorkh-50: #FCEDED;
  --color-sorkh-100: #F8D6D6;
  --color-sorkh-200: #F1ACAC;
  --color-sorkh-300: #E77E7E;
  --color-sorkh-400: #DD5C5C;
  --color-sorkh-500: #D14343;
  --color-sorkh-600: #B13333;
  --color-sorkh-700: #8E2828;
  --color-sorkh-800: #6C1F1F;
  --color-sorkh-900: #4C1616;
  --color-sorkh-950: #2E0D0D;

  --radius-control: 0.5rem;
  --radius-surface: 0.875rem;
  --shadow-float: 0 12px 32px -12px rgb(11 24 32 / 0.35);
}

/* Nuxt UI semantic variables. `--ui-radius` drives rounded-md (controls) at 1.5×. */
:root {
  --ui-radius: 0.34rem;
  --ui-bg: #FFFFFF;
  --ui-bg-muted: var(--color-petrol-50);
  --ui-bg-elevated: var(--color-petrol-100);
  --ui-bg-accented: var(--color-petrol-200);
  --ui-bg-inverted: var(--color-petrol-900);
  --ui-border: var(--color-petrol-200);
  --ui-border-muted: var(--color-petrol-100);
  --ui-border-accented: var(--color-petrol-300);
  --ui-text-highlighted: #13232C;
  --ui-text: var(--color-petrol-800);
  --ui-text-toned: var(--color-petrol-700);
  --ui-text-muted: var(--color-petrol-600);
  --ui-text-dimmed: var(--color-petrol-500);
  --ui-primary: var(--ui-color-primary-600);
  --ui-secondary: var(--ui-color-secondary-800);
  --ui-success: var(--ui-color-success-700);
  --ui-info: var(--ui-color-info-600);
  --ui-warning: var(--ui-color-warning-800);
  --ui-error: var(--ui-color-error-600);
  --ui-neutral: var(--ui-color-neutral-600);
  --sidebar-top: #0B1820;
  --sidebar-bottom: #10222D;
  --sidebar-text: #C6D4DB;
  --sidebar-muted: var(--color-petrol-400);
}
.dark {
  --ui-bg: var(--color-petrol-900);
  --ui-bg-muted: var(--color-petrol-950);
  --ui-bg-elevated: #1C2F3B;
  --ui-bg-accented: var(--color-petrol-800);
  --ui-bg-inverted: #F3F7F9;
  --ui-border: var(--color-petrol-800);
  --ui-border-muted: #1F3440;
  --ui-border-accented: var(--color-petrol-700);
  --ui-text-highlighted: #F3F7F9;
  --ui-text: #E6EEF2;
  --ui-text-toned: #C6D4DB;
  --ui-text-muted: var(--color-petrol-400);
  --ui-text-dimmed: var(--color-petrol-500);
  --ui-primary: var(--ui-color-primary-400);
  --ui-secondary: var(--ui-color-secondary-300);
  --ui-success: var(--ui-color-success-400);
  --ui-info: var(--ui-color-info-400);
  --ui-warning: var(--ui-color-warning-300);
  --ui-error: var(--ui-color-error-400);
  --ui-neutral: var(--ui-color-neutral-400);
}

/* ---- Base ---------------------------------------------------------------- */
html { font-family: var(--font-sans); font-size: 15px; }
body { line-height: 1.7; }
h1, h2, h3 { line-height: 1.3; font-weight: 700; color: var(--ui-text-highlighted); }
table, .tabular { font-variant-numeric: tabular-nums; }
code, pre, kbd { font-family: ui-monospace, 'Cascadia Mono', 'Segoe UI Mono', Menlo, monospace; font-size: 0.85em; }
:where(button, a, input, select, textarea):focus-visible { outline: 2px solid var(--color-zaferan-400); outline-offset: 2px; }

/* ---- Shared component classes (consumed by later tasks) ------------------ */
@layer components {
  .surface { background: var(--ui-bg); border: 1px solid var(--ui-border); border-radius: var(--radius-surface); }
  .surface-head { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.875rem 1.25rem; border-bottom: 1px solid var(--ui-border); }
  .surface-head h2, .surface-head h3 { font-size: 1.0625rem; margin: 0; }
  .surface-body { padding: 1.25rem; }

  .sidebar { position: relative; background: linear-gradient(180deg, var(--sidebar-top), var(--sidebar-bottom)); color: var(--sidebar-text); isolation: isolate; }
  .sidebar::before {
    content: ''; position: absolute; inset: 0; z-index: -1; opacity: 0.06; pointer-events: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='96' height='48' viewBox='0 0 96 48' fill='none' stroke='white' stroke-width='1.5'%3E%3Cpath d='M0 40Q24 8 48 40T96 40'/%3E%3C/svg%3E");
    background-size: 96px 48px;
  }
  .nav-item { display: flex; align-items: center; gap: 0.75rem; height: 2.5rem; padding-inline: 0.75rem; border-radius: var(--radius-control); font-weight: 500; color: var(--sidebar-text); transition: background-color 150ms, color 150ms; }
  .nav-item:hover { background: rgb(255 255 255 / 0.08); color: #fff; }
  .nav-item-active, .nav-item-active:hover { background: var(--color-firoozeh-600); color: #fff; }
  .nav-item-active .nav-icon { color: var(--color-zaferan-300); }

  .theme-toggle { display: inline-flex; padding: 2px; gap: 2px; border-radius: 999px; background: rgb(255 255 255 / 0.08); }
  .theme-toggle-btn { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 24px; border-radius: 999px; color: var(--sidebar-muted); transition: background-color 150ms, color 150ms; }
  .theme-toggle-btn:hover { color: #fff; }
  .theme-toggle-btn.is-on { background: rgb(255 255 255 / 0.16); color: #fff; }

  .running-card { display: flex; align-items: center; gap: 0.625rem; padding: 0.75rem; border-radius: var(--radius-control); background: rgb(255 255 255 / 0.08); color: #fff; font-size: 0.875rem; transition: background-color 150ms; }
  .running-card:hover { background: rgb(255 255 255 / 0.14); }
  .running-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--color-zaferan-400); flex: none; animation: breathe 1.6s ease-in-out infinite; }

  .segmented { display: inline-flex; padding: 2px; gap: 2px; border-radius: var(--radius-control); background: var(--ui-bg-elevated); }
  .segmented-btn { padding: 0.25rem 0.75rem; border-radius: calc(var(--radius-control) - 2px); font-size: 0.875rem; font-weight: 500; color: var(--ui-text-muted); transition: background-color 150ms, color 150ms; }
  .segmented-btn:hover { color: var(--ui-text-highlighted); }
  .segmented-btn[aria-checked="true"] { background: var(--ui-bg); color: var(--ui-text-highlighted); box-shadow: 0 1px 2px rgb(11 24 32 / 0.12); }

  .ticket { position: relative; display: block; cursor: pointer; padding: 1.25rem; padding-inline-end: 3rem; border-radius: var(--radius-surface); background: var(--ui-bg); border: 1px solid var(--ui-border); transition: border-color 150ms, box-shadow 150ms; }
  .ticket:hover { border-color: var(--ui-border-accented); }
  .ticket[data-selected="true"] { border-color: var(--color-firoozeh-500); box-shadow: inset 0 0 0 1px var(--color-firoozeh-500); }
  .ticket:has(input:focus-visible) { outline: 2px solid var(--color-zaferan-400); outline-offset: 2px; }
  .ticket-check { position: absolute; top: 1.25rem; inset-inline-end: 1.25rem; color: var(--color-firoozeh-500); }

  .ledger { background: var(--ui-bg); border: 1px solid var(--ui-border); border-radius: var(--radius-surface); overflow-x: auto; }
  .ledger table { width: 100%; font-size: 0.9375rem; font-variant-numeric: tabular-nums; }
  .ledger thead th { background: var(--ui-bg-elevated); color: var(--ui-text-muted); font-weight: 500; font-size: 0.8125rem; text-align: start; padding: 0.625rem 0.875rem; white-space: nowrap; }
  .ledger tbody td { padding: 0.625rem 0.875rem; border-top: 1px solid var(--ui-border); vertical-align: top; text-align: start; }
  .ledger tbody tr:hover { background: color-mix(in oklab, var(--ui-bg-elevated) 55%, transparent); }
  .ledger tbody tr[aria-selected="true"] { background: var(--color-firoozeh-50); box-shadow: inset 3px 0 0 var(--color-firoozeh-500); }
  .dark .ledger tbody tr[aria-selected="true"] { background: var(--color-firoozeh-950); }

  .stepper { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); }
  .stepper-step { position: relative; display: flex; flex-direction: column; align-items: center; gap: 0.375rem; text-align: center; padding-top: 0.75rem; }
  .stepper-step::before { content: ''; position: absolute; top: 0; inset-inline: 0; height: 3px; background: var(--ui-border); }
  .stepper-step:first-child::before { inset-inline-start: 50%; border-start-start-radius: 2px; border-end-start-radius: 2px; }
  .stepper-step:last-child::before { inset-inline-end: 50%; border-start-end-radius: 2px; border-end-end-radius: 2px; }
  .stepper-step[data-state="done"]::before { background: var(--color-firoozeh-500); }
  .stepper-step[data-state="current"]::before { background: linear-gradient(to left, var(--color-firoozeh-500) 50%, var(--ui-border) 50%); }
  .stepper-pier { width: 4px; height: 14px; border-radius: 2px; background: var(--ui-border-accented); margin-top: -0.75rem; }
  .stepper-step[data-state="done"] .stepper-pier { background: var(--color-firoozeh-500); }
  .stepper-step[data-state="current"] .stepper-pier { background: var(--color-zaferan-400); animation: breathe 1.6s ease-in-out infinite; }
  .stepper-label { font-size: 0.8125rem; line-height: 1.3; color: var(--ui-text-muted); }
  .stepper-step[data-state="current"] .stepper-label { color: var(--ui-text-highlighted); font-weight: 600; }
  .stepper-step[data-state="done"] .stepper-label { color: var(--ui-text); }

  .sim-track { display: inline-block; width: 64px; height: 6px; border-radius: 3px; background: var(--ui-bg-accented); overflow: hidden; vertical-align: middle; }
  .sim-fill { display: block; height: 100%; border-radius: 3px; background: var(--color-petrol-400); }
  .sim-fill[data-tone="fair"] { background: var(--color-zaferan-500); }
  .sim-fill[data-tone="good"] { background: var(--color-sabz-500); }

  .level-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-petrol-400); flex: none; margin-top: 0.55em; }
  .level-dot[data-level="WARN"] { background: var(--color-zaferan-500); }
  .level-dot[data-level="ERROR"] { background: var(--color-sorkh-500); }
  .level-dot[data-level="PRIMARY"] { background: var(--color-firoozeh-500); }

  .check { display: inline-flex; align-items: center; justify-content: center; width: 1.5rem; height: 1.5rem; border-radius: 50%; flex: none; font-size: 0.8125rem; font-weight: 600; color: var(--ui-text-muted); border: 1.5px solid var(--ui-border-accented); }
  .check.is-done { background: var(--color-sabz-500); border-color: var(--color-sabz-500); color: #fff; }

  .draw { stroke-dasharray: 1; stroke-dashoffset: 1; animation: draw 900ms ease-out forwards; }
}

@keyframes breathe { 50% { opacity: 0.55; } }
@keyframes draw { to { stroke-dashoffset: 0; } }

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.001ms !important; animation-iteration-count: 1 !important; transition-duration: 0.001ms !important; }
  .draw { stroke-dashoffset: 0; }
}
```

- [ ] **Step 8: Create `app/app.config.ts`**

```ts
export default defineAppConfig({
  ui: {
    colors: {
      primary: 'firoozeh',
      secondary: 'zaferan',
      success: 'sabz',
      info: 'firoozeh',
      warning: 'zaferan',
      error: 'sorkh',
      neutral: 'petrol',
    },
    button: { slots: { base: 'font-medium' } },
    // Arbitrary values, not the named tokens: tailwind-merge only replaces the theme's
    // `rounded-lg` / `shadow-lg` when it recognises the incoming class as the same group.
    modal: { slots: { content: 'rounded-[0.875rem] shadow-[0_12px_32px_-12px_rgb(11_24_32/0.35)]' } },
    select: { slots: { content: 'shadow-[0_12px_32px_-12px_rgb(11_24_32/0.35)]' } },
  },
});
```

- [ ] **Step 9: Update `nuxt.config.ts`**

```ts
import { version } from './package.json';

export default defineNuxtConfig({
  ssr: false,
  devtools: { enabled: false },
  modules: ['@nuxt/ui'],
  css: ['~/assets/css/main.css'],
  // A desktop app under a strict CSP: every icon the pages and Nuxt UI use is bundled at build
  // time from the local lucide collection; nothing is fetched from api.iconify.design at runtime.
  icon: {
    provider: 'none',
    clientBundle: { scan: true, sizeLimitKb: 512 },
  },
  colorMode: { preference: 'system', fallback: 'light' },
  runtimeConfig: { public: { version } },
  app: {
    head: {
      htmlAttrs: { dir: 'rtl', lang: 'fa' },
      title: 'پل لندو',
      meta: [{ name: 'color-scheme', content: 'light dark' }],
      link: [{ rel: 'icon', type: 'image/svg+xml', href: '/brand/logo.svg' }],
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

(`/brand/logo.svg` is written in Task 3; a missing favicon is harmless until then.)

- [ ] **Step 10: Set the Tauri window background**

In `src-tauri/tauri.conf.json`, change the window entry to:

```json
{ "title": "پل لندو", "width": 1200, "height": 800, "minWidth": 960, "minHeight": 640, "backgroundColor": "#F2F5F7" }
```

- [ ] **Step 11: Run tests and typecheck**

Run: `pnpm test && pnpm typecheck`
Expected: all vitest files pass (fa, sequence, color, palette); typecheck clean. If a palette pair fails, adjust the **shade used** (e.g. move a `--ui-*` to the next darker stop) rather than the hex values in the spec, and note it in the commit message.

- [ ] **Step 12: Commit**

```bash
git add app/assets/css/main.css app/app.config.ts nuxt.config.ts src-tauri/tauri.conf.json app/utils/color.ts app/utils/color.spec.ts app/utils/palette.spec.ts
git commit -m "Define the پل design tokens and wire them into Nuxt UI

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Dev fixture mode and the screenshot script

**Files:**
- Create: `app/dev/mockBridge.ts`, `app/dev/devQuery.client.ts`
- Modify: `app/composables/useBridge.ts`
- Modify: `nuxt.config.ts` (register the dev plugin)
- Create: `scripts/shots.sh`

**Interfaces:**
- Consumes: `bridge`/`onJobEvents` API in `app/composables/useBridge.ts` (unchanged signatures).
- Produces: `mockCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T>`, `mockEvents(h: JobHandlers): () => void`; query params `?state=empty|running|review|refreshing|done` and `?mode=light|dark` honoured only in `pnpm dev` without Tauri; `scripts/shots.sh <outdir> [filter]` writing `<mode>-<page>.png` files.

- [ ] **Step 1: Write `app/dev/mockBridge.ts`**

```ts
// Fixtures for `pnpm dev` in a plain browser (no Tauri). Never imported by the production build:
// useBridge.ts only reaches for this file behind `import.meta.dev`.
import type {
  Dashboard, ErrorRow, JobDetail, JobEventRow, JobItemRow, JobRow, LandauStatus, MatchRow, Scored, SettingsView,
} from '~/types/bridge';
import type { JobHandlers } from '~/composables/useBridge';

type State = 'empty' | 'running' | 'review' | 'refreshing' | 'done';
export const devState = (): State => {
  const s = new URLSearchParams(location.search).get('state');
  return s === 'empty' || s === 'running' || s === 'review' || s === 'refreshing' ? s : 'done';
};

const ago = (days: number, hour = 10, minute = 20): string => {
  const d = new Date();
  d.setDate(d.getDate() - days);
  d.setHours(hour, minute, 0, 0);
  return d.toISOString();
};

const settings: SettingsView = {
  landau_url: 'https://api.landau.ir', has_landau_token: true, store: 'st_01', store_name: 'یدکی مرکزی',
  supplier_user: 'yadaki.markazi', has_supplier_password: true, default_authenticity: 'ORIGINAL', threshold: 80,
  vat_percent: 10, overwrite_manual: false, secret_backend: 'keyring', data_dir: '/home/vahid/.local/share/ir.landau.bridge',
};
const settingsEmpty: SettingsView = {
  ...settings, has_landau_token: false, store: null, store_name: null, supplier_user: null, has_supplier_password: false,
};

const landau: LandauStatus = {
  business: 'biz_77', business_name: 'یدکی مرکزی', approved: true,
  stores: [{ id: 'st_01', name: 'شعبه امیرکبیر', kind: 'RETAIL', state: 'ACTIVE', status: 'OPEN' }], active_rules: 2,
};

const job = (id: number, kind: JobRow['kind'], status: string, phase: JobRow['phase'], days: number, summary: JobRow['summary'] = null): JobRow => ({
  id, kind, mode: kind === 'IMPORT' ? 'QUANTITY' : null, since_date: kind === 'IMPORT' ? '1405/01/01' : null,
  store: 'st_01', overwrite: false, status, phase, started_at: ago(days),
  finished_at: status === 'RUNNING' || status === 'REVIEW' ? null : ago(days, 10, 41), summary,
});

const doneSummary = { items: 89, done: 81, attached: 34, patched: 47, skipped: 5, failed: 3, invoice_ids: [5121, 5140, 5177] };
const jobs: JobRow[] = [
  job(42, 'IMPORT', 'DONE', 'REPORT', 0, doneSummary),
  job(41, 'REFRESH', 'DONE', 'REPORT', 1, { rows: 412, changed: 96, skipped_manual: 12 }),
  job(40, 'IMPORT', 'FAILED', 'FETCH', 3),
  job(39, 'IMPORT', 'DONE', 'REPORT', 9, doneSummary),
  job(38, 'REFRESH', 'CANCELLED', 'WRITE', 12),
  job(37, 'IMPORT', 'DONE', 'REPORT', 20, doneSummary),
];
const runningJob = job(43, 'IMPORT', 'RUNNING', 'MATCH', 0);
const reviewJob = job(43, 'IMPORT', 'REVIEW', 'REVIEW', 0);
const refreshJob = job(44, 'REFRESH', 'RUNNING', 'WRITE', 0);
const current = (state: State): JobRow | null =>
  state === 'running' ? runningJob : state === 'review' ? reviewJob : state === 'refreshing' ? refreshJob : null;

const scored = (id: string, name: string, brand: string | null, similarity: number, alias: string | null = null): Scored =>
  ({ candidate: { id, name, alias, brand, score: similarity }, similarity });

const item = (id: number, code: string, name: string, qty: number, extra: Partial<JobItemRow>): JobItemRow => ({
  id, job_id: 42, code, name, qty, product_id: null, product_name: null, stock_id: null, authenticity: 'ORIGINAL', action: null,
  qty_before: null, qty_after: null, cost: null, price_before: null, price_after: null, reprice_verdict: null,
  error_kind: null, error_msg: null, candidates: null, status: 'DONE', invoice_ids: [5121], ...extra,
});

const reviewItems: JobItemRow[] = [
  item(901, 'BP-10422', 'لنت ترمز جلو پراید', 12, { status: 'REVIEW', candidates: [
    scored('p_1', 'لنت ترمز جلو پراید ۱۳۱', 'فرانتک', 91), scored('p_2', 'لنت ترمز عقب پراید', 'فرانتک', 74), scored('p_3', 'لنت ترمز جلو تیبا', 'پارس', 62),
  ] }),
  item(902, 'BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 30, { status: 'REVIEW', candidates: [
    scored('p_4', 'فیلتر روغن پژو ۲۰۶', 'سرکان', 88, 'W712/8'), scored('p_5', 'فیلتر هوا پژو ۲۰۶', 'سرکان', 57),
  ] }),
  item(903, 'BP-33005', 'شمع NGK BKR6E', 48, { status: 'REVIEW', candidates: [] }),
];

const doneItems: JobItemRow[] = [
  item(1, 'BP-10422', 'لنت ترمز جلو پراید', 12, { product_id: 'p_1', product_name: 'لنت ترمز جلو پراید ۱۳۱', action: 'ATTACH', qty_after: 12, cost: 1_430_000, price_after: 1_780_000, reprice_verdict: 'REPRICED:1780000' }),
  item(2, 'BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 30, { product_id: 'p_4', product_name: 'فیلتر روغن پژو ۲۰۶', action: 'PATCH', qty_before: 8, qty_after: 38, cost: 385_000, price_before: 450_000, price_after: 480_000, reprice_verdict: 'REPRICED:480000' }),
  item(3, 'BP-33005', 'شمع NGK BKR6E', 48, { product_id: 'p_9', product_name: 'شمع NGK BKR6E', action: 'PATCH', qty_before: 20, qty_after: 68, cost: 1_210_000, price_before: 1_500_000, price_after: 1_500_000, reprice_verdict: 'SKIPPED:MANUAL' }),
  item(4, 'BP-44120', 'تسمه تایم ۴۰۵', 6, { status: 'FAILED', error_kind: 'NoCurrentPrice', error_msg: 'بارزپخش برای این کد قیمت روز ندارد' }),
  item(5, 'BP-51002', 'دیسک ترمز جلو سمند', 4, { product_id: 'p_12', product_name: 'دیسک ترمز جلو سمند', action: 'ATTACH', qty_after: 4, cost: 2_960_000, price_after: 3_650_000, reprice_verdict: 'REPRICED:3650000' }),
  item(6, 'BP-60233', 'روغن موتور ۴ لیتری بهران', 24, { status: 'SKIPPED', error_kind: 'NoneDecision', error_msg: 'در فهرست لندو نیست' }),
  item(7, 'BP-71850', 'واشر سرسیلندر ۲۰۶', 10, { product_id: 'p_15', product_name: 'واشر سرسیلندر پژو ۲۰۶', action: 'ATTACH', qty_after: 10, cost: 940_000, price_after: 1_170_000, reprice_verdict: 'REPRICED:1170000' }),
  item(8, 'BP-80017', 'پمپ بنزین پراید', 5, { product_id: 'p_16', product_name: 'پمپ بنزین پراید', action: 'PATCH', qty_before: 2, qty_after: 7, cost: 1_650_000, price_before: 2_050_000, price_after: 2_050_000, reprice_verdict: 'UNCHANGED' }),
];

const ev = (id: number, level: JobEventRow['level'], phase: string, message: string, minutesAgo: number): JobEventRow =>
  ({ id, at: new Date(Date.now() - minutesAgo * 60_000).toISOString(), level, phase, message, detail: null });
const doneEvents: JobEventRow[] = [
  ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لندو و بارزپخش برقرار است', 40),
  ev(2, 'INFO', 'FETCH', '۱۲۴ فاکتور خوانده شد؛ ۳ فاکتور پرداخت‌نشده رد شد', 37),
  ev(3, 'INFO', 'AGGREGATE', '۲۱۷ قلم در ۸۹ کد جمع‌بندی شد', 36),
  ev(4, 'INFO', 'MATCH', '۷۱ کد خودکار تطبیق داده شد، ۱۸ کد به تصمیم شما نیاز داشت', 35),
  ev(5, 'WARN', 'PRICE', 'برای BP-44120 قیمت روز پیدا نشد', 30),
  ev(6, 'INFO', 'WRITE', '۸۱ ردیف روی قفسه نوشته شد', 26),
  ev(7, 'INFO', 'REPRICE', 'قواعد قیمت‌گذاری لندو روی ۴۱۲ ردیف اجرا شد؛ ۹۶ قیمت تغییر کرد', 24),
  ev(8, 'INFO', 'REPORT', JSON.stringify(doneSummary), 23),
];
const runningEvents: JobEventRow[] = doneEvents.slice(0, 3);

const match = (code: string, name_seen: string, how: MatchRow['how'], product_name: string | null, similarity: number | null, days: number): MatchRow =>
  ({ supplier: 'barezpakhsh', code, product_id: product_name ? `p_${code}` : null, product_name, how, similarity, name_seen, decided_at: ago(days, 14, 5) });
const matches: MatchRow[] = [
  match('BP-10422', 'لنت ترمز جلو پراید', 'USER', 'لنت ترمز جلو پراید ۱۳۱', 91, 0),
  match('BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 'AUTO', 'فیلتر روغن پژو ۲۰۶', 88, 0),
  match('BP-33005', 'شمع NGK BKR6E', 'AUTO', 'شمع NGK BKR6E', 97, 1),
  match('BP-44120', 'تسمه تایم ۴۰۵', 'AUTO', 'تسمه تایم پژو ۴۰۵', 84, 1),
  match('BP-51002', 'دیسک ترمز جلو سمند', 'AUTO', 'دیسک ترمز جلو سمند', 95, 3),
  match('BP-60233', 'روغن موتور ۴ لیتری بهران', 'NONE', null, null, 3),
  match('BP-71850', 'واشر سرسیلندر ۲۰۶', 'USER', 'واشر سرسیلندر پژو ۲۰۶', 78, 9),
  match('BP-80017', 'پمپ بنزین پراید', 'AUTO', 'پمپ بنزین پراید', 100, 9),
  match('BP-90311', 'کمک فنر عقب تیبا', 'AUTO', 'کمک فنر عقب تیبا', 93, 12),
  match('BP-90420', 'بلبرینگ چرخ جلو ۲۰۶', 'USER', 'بلبرینگ چرخ جلو پژو ۲۰۶', 71, 12),
  match('BP-91002', 'رادیاتور آب دنا', 'NONE', null, null, 20),
  match('BP-91777', 'سنسور اکسیژن سمند EF7', 'AUTO', 'سنسور اکسیژن EF7', 86, 20),
];

const err = (id: number, days: number, job_id: number | null, kind: string, message: string, context: unknown = null): ErrorRow =>
  ({ id, at: ago(days, 9, 12), job_id, kind, message, context });
const errors: ErrorRow[] = [
  err(61, 0, 42, 'NoCurrentPrice', 'بارزپخش برای BP-44120 قیمت روز ندارد', { code: 'BP-44120' }),
  err(60, 0, 42, 'NoneDecision', 'BP-60233 در فهرست لندو نیست؛ برای افزودن آن تیکت ثبت کنید'),
  err(59, 1, 41, 'RepriceSkipped', '۱۲ ردیف با قیمت دستی دست نخورد', { skipped: 'MANUAL', rows: 12 }),
  err(58, 3, 40, 'SupplierUnreachable', 'پاسخی از بارزپخش نیامد (timeout after 30s)', { url: 'https://barezpakhsh.ir/invoices' }),
  err(57, 5, null, 'LandauUnauthorized', 'کلید API لندو پذیرفته نشد'),
  err(56, 12, 38, 'JobCancelled', 'به درخواست شما متوقف شد'),
];

const dashboard = (state: State): Dashboard => {
  const running = current(state);
  return {
    settings_complete: state !== 'empty', store_name: state === 'empty' ? null : 'یدکی مرکزی',
    supplier_user: state === 'empty' ? null : 'yadaki.markazi', invoices_cached: state === 'empty' ? 0 : 124,
    invoices_unpaid: state === 'empty' ? 0 : 3, last_import: state === 'empty' ? null : jobs[0]!,
    last_refresh: state === 'empty' ? null : jobs[1]!, running, pending_review: state === 'review' ? reviewItems.length : 0,
    recent_errors: state === 'empty' ? 0 : 4, decided_matches: state === 'empty' ? 0 : matches.length,
  };
};

const detail = (id: number, state: State): JobDetail => {
  if (id === 43 && state === 'running') return { job: runningJob, items: [], events: runningEvents };
  if (id === 43 && state === 'review') return { job: reviewJob, items: reviewItems, events: doneEvents.slice(0, 4) };
  if (id === 44 && state === 'refreshing') return { job: refreshJob, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لندو و بارزپخش برقرار است', 6), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 3)] };
  const j = jobs.find((x) => x.id === id) ?? jobs[0]!;
  return { job: j, items: j.status === 'DONE' ? doneItems : [], events: j.status === 'DONE' ? doneEvents : doneEvents.slice(0, 2) };
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function mockCall<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const state = devState();
  await wait(120);
  const out: unknown = (() => {
    switch (cmd) {
      case 'settings_get': return state === 'empty' ? settingsEmpty : settings;
      case 'settings_save': return { ...(state === 'empty' ? settingsEmpty : settings), ...(args.patch as object) };
      case 'landau_validate': return landau;
      case 'supplier_test_login': return undefined;
      case 'dashboard_get': return dashboard(state);
      case 'import_start': return 43;
      case 'refresh_start': return 44;
      case 'import_review': return reviewItems;
      case 'review_decide': case 'import_continue': case 'match_set': case 'match_none': case 'match_clear': case 'open_log_folder': return undefined;
      case 'review_search': return [scored('p_s1', `${args.term}`, 'جست‌وجو', 66)];
      case 'job_cancel': return true;
      case 'job_current': return current(state);
      case 'jobs_list': return state === 'empty' ? [] : jobs;
      case 'job_get': return detail(Number(args.job_id), state);
      case 'matches_list': return state === 'empty' ? [] : matches;
      case 'errors_list': return state === 'empty' ? [] : errors;
      default: throw { kind: 'Other', message: `mock: unknown command ${cmd}`, detail: cmd };
    }
  })();
  return out as T;
}

/** Emits one progress tick for the running fixtures so the stepper shows its bar; nothing else. */
export function mockEvents(h: JobHandlers): () => void {
  const state = devState();
  if (state === 'running') {
    setTimeout(() => h.progress?.({ job_id: 43, phase: 'MATCH', done: 37, total: 89, message: 'تطبیق با فهرست لندو' }), 200);
  } else if (state === 'refreshing') {
    setTimeout(() => h.progress?.({ job_id: 44, phase: 'WRITE', done: 210, total: 412, message: 'نوشتن قیمت خرید روی قفسه' }), 200);
  }
  return () => {};
}
```

- [ ] **Step 2: Write `app/dev/devQuery.client.ts`**

```ts
// Dev-only: `?mode=dark|light|system` picks the colour mode for screenshots. Registered from
// nuxt.config.ts only when NODE_ENV is development, and inert inside the Tauri shell.
export default defineNuxtPlugin((nuxtApp) => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
  const mode = new URLSearchParams(location.search).get('mode');
  if (mode === 'dark' || mode === 'light' || mode === 'system') {
    (nuxtApp.$colorMode as { preference: string }).preference = mode;
  }
});
```

- [ ] **Step 3: Route `useBridge.ts` through the mock when there is no Tauri runtime in dev**

Replace the `call` function and `onJobEvents` in `app/composables/useBridge.ts`:

```ts
// In `pnpm dev` opened in a plain browser there is no Tauri runtime; the fixtures in app/dev
// stand in so the pages can be looked at. The branch is dead code in the production build.
const useMock = import.meta.dev && typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in window);

/** One place every command goes through; a Rust `BridgeError` arrives as a typed rejection. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (useMock) return (await import('~/dev/mockBridge')).mockCall<T>(cmd, args);
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isBridgeError(e)) throw e;
    throw { kind: 'Other', message: String(e), detail: String(e) } satisfies BridgeError;
  }
}
```

and

```ts
/** Subscribe to the three job events; returns one function that unsubscribes all. */
export async function onJobEvents(h: JobHandlers): Promise<UnlistenFn> {
  if (useMock) return (await import('~/dev/mockBridge')).mockEvents(h);
  const offs: UnlistenFn[] = [];
  if (h.progress) offs.push(await listen<Progress>('job://progress', (e) => h.progress!(e.payload)));
  if (h.event) offs.push(await listen<JobEvent>('job://event', (e) => h.event!(e.payload)));
  if (h.done) offs.push(await listen<JobDone>('job://done', (e) => h.done!(e.payload)));
  return () => offs.forEach((off) => off());
}
```

- [ ] **Step 4: Register the dev plugin in `nuxt.config.ts`**

Add to the config object (next to `css`):

```ts
  plugins: process.env.NODE_ENV === 'development' ? ['~/dev/devQuery.client'] : [],
```

- [ ] **Step 5: Write `scripts/shots.sh`**

```sh
#!/usr/bin/env sh
# Screenshots every page in both colour modes through the dev fixture mode (app/dev/mockBridge.ts).
# Run `pnpm dev` first. Usage: scripts/shots.sh OUT_DIR [page-filter]
set -eu
out=${1:?output directory}
filter=${2:-}
mkdir -p "$out"
pages="/?state=done /?state=empty /import?state=running /import?state=review /import?state=done /refresh?state=done /refresh?state=refreshing /matches?state=done /matches?state=empty /logs?state=done /logs?state=done&job=42 /settings?state=done"
for mode in light dark; do
  for p in $pages; do
    case "$p" in *"$filter"*) ;; *) continue ;; esac
    name=$(printf '%s' "$p" | tr '/?=&' '____' | sed 's/^_//; s/^$/root/')
    google-chrome-stable --headless=new --no-proxy-server --disable-gpu --hide-scrollbars --no-sandbox \
      --window-size=1200,800 --virtual-time-budget=6000 --force-device-scale-factor=1 \
      --screenshot="$out/$mode-$name.png" "http://localhost:3210$p&mode=$mode" >/dev/null 2>&1
    echo "$out/$mode-$name.png"
  done
done
```

Then `chmod +x scripts/shots.sh`.

- [ ] **Step 6: Start the dev server in the background and shoot the current (old) pages**

Run: `pnpm dev` (background, keep it running for the rest of the plan; restart it only if it dies).
Wait until `curl -s -o /dev/null -w '%{http_code}' http://localhost:3210/` prints `200`.
Run: `scripts/shots.sh "$SCRATCH/shots-00-baseline"`
Expected: 24 PNGs. Read `light-_state_done.png` and `dark-import_state_review.png`: the pages must show fixture data (store «یدکی مرکزی», three review items) — proving the mock is wired — rendered with the Task 1 tokens but still the old layout.

- [ ] **Step 7: Typecheck and test**

Run: `pnpm typecheck && pnpm test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add app/dev/mockBridge.ts app/dev/devQuery.client.ts app/composables/useBridge.ts nuxt.config.ts scripts/shots.sh
git commit -m "Add a dev-only fixture mode and a screenshot script for design review

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Brand — logo component, SVG assets, app icon

**Files:**
- Create: `app/components/BrandLogo.vue`, `public/brand/logo.svg`, `public/brand/icon.svg`
- Regenerate: `src-tauri/icons/*` (via `pnpm tauri icon`)

**Interfaces:**
- Produces: `<BrandLogo :size="28" :with-wordmark="false" dots="accent" />` — props `size?: number` (px, default 28), `withWordmark?: boolean`, `dots?: 'accent' | 'current'`; stroke uses `currentColor`.

- [ ] **Step 1: Write the mark once as `public/brand/logo.svg`**

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="none" stroke="currentColor" stroke-width="5" stroke-linecap="round" stroke-linejoin="round">
  <path d="M8 14H56"/>
  <path d="M14 14v8M50 14v8"/>
  <path d="M14 22Q32 54 50 22"/>
  <g fill="#EDAC46" stroke="none">
    <circle cx="24" cy="48" r="3.5"/>
    <circle cx="32" cy="52" r="3.5"/>
    <circle cx="40" cy="48" r="3.5"/>
  </g>
</svg>
```

- [ ] **Step 2: Preview it at three sizes and look**

Run:
```sh
for s in 32 96 512; do rsvg-convert -w $s -h $s --stylesheet <(echo 'svg{color:#0F8B9D}') public/brand/logo.svg -o "$SCRATCH/logo-$s.png"; done
```
Read `$SCRATCH/logo-512.png` and `$SCRATCH/logo-32.png`. Check: the bowl reads as a sagging cable, the top line as a deck, the three dots sit clearly under the bowl and are visible at 32 px. If the bowl is too shallow/deep, move the control point (`Q32 54`) by ±6 and re-render; if the dots merge at 32 px, raise `r` to 4 and spread x by 2.

- [ ] **Step 3: Write the app icon `public/brand/icon.svg`**

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0F8B9D"/>
      <stop offset="1" stop-color="#0A5C69"/>
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" rx="225" fill="url(#g)"/>
  <g transform="translate(194 194) scale(9.94)" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round" stroke-linejoin="round">
    <path d="M8 14H56"/>
    <path d="M14 14v8M50 14v8"/>
    <path d="M14 22Q32 54 50 22"/>
    <g fill="#EDAC46" stroke="none">
      <circle cx="24" cy="48" r="3.5"/>
      <circle cx="32" cy="52" r="3.5"/>
      <circle cx="40" cy="48" r="3.5"/>
    </g>
  </g>
</svg>
```

Run: `rsvg-convert -w 256 -h 256 public/brand/icon.svg -o "$SCRATCH/icon-256.png"` and Read it. The mark must be centred with even margins; if it sits low (the mark's optical centre is above its box centre), change the translate y from 194 to 170.

- [ ] **Step 4: Generate the Tauri icon set**

Run: `pnpm tauri icon public/brand/icon.svg`
Expected: files under `src-tauri/icons/` rewritten (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, `icon.png`, the `Square*Logo.png` set, `StoreLogo.png`). Read `src-tauri/icons/128x128.png` to confirm it is the new mark. If `tauri icon` refuses SVG input, run `rsvg-convert -w 1024 -h 1024 public/brand/icon.svg -o "$SCRATCH/icon-1024.png"` and pass that PNG instead.

- [ ] **Step 5: Write `app/components/BrandLogo.vue`**

```vue
<script setup lang="ts">
withDefaults(defineProps<{ size?: number; withWordmark?: boolean; dots?: 'accent' | 'current' }>(), {
  size: 28, withWordmark: false, dots: 'accent',
});
</script>

<template>
  <span class="inline-flex items-center gap-3">
    <svg :width="size" :height="size" viewBox="0 0 64 64" fill="none" stroke="currentColor" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" role="img" aria-label="پل لندو">
      <path d="M8 14H56" />
      <path d="M14 14v8M50 14v8" />
      <path d="M14 22Q32 54 50 22" />
      <g :fill="dots === 'accent' ? 'var(--color-zaferan-400)' : 'currentColor'" stroke="none">
        <circle cx="24" cy="48" r="3.5" />
        <circle cx="32" cy="52" r="3.5" />
        <circle cx="40" cy="48" r="3.5" />
      </g>
    </svg>
    <span v-if="withWordmark" class="text-xl font-extrabold leading-none tracking-tight">پل لندو</span>
  </span>
</template>
```

- [ ] **Step 6: Typecheck, commit**

Run: `pnpm typecheck`
Expected: clean.

```bash
git add app/components/BrandLogo.vue public/brand src-tauri/icons
git commit -m "Draw the پل لندو mark and generate the app icon from it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Shell — sidebar, page header, theme toggle, running-job card

**Files:**
- Create: `app/components/PageHeader.vue`, `app/components/ThemeToggle.vue`, `app/components/RunningJobCard.vue`
- Modify: `app/layouts/default.vue`

**Interfaces:**
- Consumes: `.sidebar`, `.nav-item*`, `.theme-toggle*`, `.running-*` classes (Task 1); `BrandLogo` (Task 3); `bridge.jobCurrent()`, `faNumber`, `phaseLabel`, `statusLabel` (existing).
- Produces: `<PageHeader title description><template #actions>…</template></PageHeader>`; `<RunningJobCard :job compact />`; `<ThemeToggle />`.

- [ ] **Step 1: Write `app/components/PageHeader.vue`**

```vue
<script setup lang="ts">
defineProps<{ title: string; description?: string }>();
</script>

<template>
  <header class="mb-8 flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
    <div class="min-w-0">
      <h1 class="text-[2rem] leading-tight">{{ title }}</h1>
      <p v-if="description" class="mt-1 max-w-prose text-muted">{{ description }}</p>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2"><slot name="actions" /></div>
  </header>
</template>
```

- [ ] **Step 2: Write `app/components/ThemeToggle.vue`**

```vue
<script setup lang="ts">
const colorMode = useColorMode();
const modes = [
  { value: 'system', icon: 'i-lucide-monitor', label: 'مطابق سیستم' },
  { value: 'light', icon: 'i-lucide-sun', label: 'روشن' },
  { value: 'dark', icon: 'i-lucide-moon', label: 'تیره' },
] as const;
</script>

<template>
  <div class="theme-toggle" role="radiogroup" aria-label="حالت رنگی">
    <button
      v-for="m in modes" :key="m.value" type="button" role="radio" :aria-checked="colorMode.preference === m.value"
      :title="m.label" class="theme-toggle-btn" :class="colorMode.preference === m.value && 'is-on'"
      @click="colorMode.preference = m.value"
    >
      <UIcon :name="m.icon" class="size-4" />
      <span class="sr-only">{{ m.label }}</span>
    </button>
  </div>
</template>
```

- [ ] **Step 3: Write `app/components/RunningJobCard.vue`**

```vue
<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const props = defineProps<{ job: JobRow; compact?: boolean }>();
const kindText = computed(() => (props.job.kind === 'IMPORT' ? 'وارد کردن فاکتورها' : 'به‌روزرسانی قیمت‌ها'));
const stateText = computed(() =>
  props.job.status === 'REVIEW' ? statusLabel.REVIEW : (phaseLabel[props.job.phase] ?? props.job.phase));
const to = computed(() => (props.job.kind === 'IMPORT' ? `/import?job=${props.job.id}` : '/refresh'));
</script>

<template>
  <NuxtLink :to="to" class="running-card" :class="!compact && 'text-base'">
    <span class="running-dot" aria-hidden="true" />
    <span class="min-w-0 flex-1">
      <span class="block truncate font-medium">
        {{ kindText }} <span class="tabular">{{ faNumber(job.id) }}</span>
      </span>
      <span class="block truncate opacity-80" :class="compact ? 'text-xs' : 'text-sm'">{{ stateText }}</span>
    </span>
    <UIcon name="i-lucide-arrow-left" class="size-4 shrink-0 opacity-70" />
  </NuxtLink>
</template>
```

- [ ] **Step 4: Rewrite `app/layouts/default.vue`**

```vue
<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/logs', label: 'گزارش و خطاها', icon: 'i-lucide-scroll-text' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const running = ref<JobRow | null>(null);
const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
const version = useRuntimeConfig().public.version;
</script>

<template>
  <div class="flex h-screen bg-muted text-default">
    <aside class="sidebar flex w-64 shrink-0 flex-col overflow-y-auto">
      <NuxtLink to="/" class="flex items-center px-5 pb-4 pt-6 text-white">
        <BrandLogo :size="32" with-wordmark />
      </NuxtLink>
      <nav class="mt-2 flex flex-col gap-1 px-3" aria-label="بخش‌های برنامه">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="nav-item" active-class="nav-item-active">
          <UIcon :name="l.icon" class="nav-icon size-5 shrink-0" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>
      <div class="mt-auto flex flex-col gap-3 p-3">
        <RunningJobCard v-if="running" :job="running" compact />
        <div class="flex items-center justify-between px-2 pb-1">
          <ThemeToggle />
          <span class="tabular text-xs" :style="{ color: 'var(--sidebar-muted)' }" dir="ltr">v{{ version }}</span>
        </div>
      </div>
    </aside>
    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-[1100px] px-8 py-8">
        <slot />
      </div>
    </main>
  </div>
</template>
```

- [ ] **Step 5: Screenshot and critique the shell**

Run: `scripts/shots.sh "$SCRATCH/shots-04-shell" "state=done"` and Read `light-_state_done.png` and `dark-_state_done.png`.
Check against the spec: sidebar dark in both modes, the mark and wordmark white, the active item white-on-turquoise with a saffron icon, the arch pattern barely visible, the version bottom-left, the page background `#F2F5F7`/`#0E1B24`. Also Read `light-import_state_running.png` to see the running-job card in the sidebar. Fix anything off (spacing, clipped wordmark, wrong active state) and re-shoot.

- [ ] **Step 6: Typecheck, commit**

Run: `pnpm typecheck && pnpm test`

```bash
git add app/components/PageHeader.vue app/components/ThemeToggle.vue app/components/RunningJobCard.vue app/layouts/default.vue
git commit -m "Rebuild the shell: dark sidebar, logo, theme switch, running-job card

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Shared pieces — LedgerTable, SegmentedControl, SimilarityBar, EmptyState, ErrorBanner

**Files:**
- Create: `app/utils/similarity.ts`, `app/utils/similarity.spec.ts`
- Create: `app/components/LedgerTable.vue`, `app/components/SegmentedControl.vue`, `app/components/SimilarityBar.vue`, `app/components/EmptyState.vue`
- Modify: `app/components/ErrorBanner.vue`

**Interfaces:**
- Produces: `similarityTone(v: number): 'good' | 'fair' | 'weak'`; `<LedgerTable>` (default slot holds `<thead>`/`<tbody>`); `<SegmentedControl v-model :items="[{ value, label }]" />`; `<SimilarityBar :value />`; `<EmptyState title description art><template #action>…</template></EmptyState>` with `art?: 'bridge' | 'shelf'`.

- [ ] **Step 1: Write the failing tone test**

`app/utils/similarity.spec.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { similarityTone } from './similarity';

describe('similarityTone', () => {
  it('maps the spec thresholds', () => {
    expect(similarityTone(100)).toBe('good');
    expect(similarityTone(80)).toBe('good');
    expect(similarityTone(79)).toBe('fair');
    expect(similarityTone(60)).toBe('fair');
    expect(similarityTone(59)).toBe('weak');
    expect(similarityTone(0)).toBe('weak');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run app/utils/similarity.spec.ts` — Expected: FAIL, module not found.

- [ ] **Step 3: Implement `app/utils/similarity.ts`**

```ts
export type SimilarityTone = 'good' | 'fair' | 'weak';

/** The colour band of a similarity percentage; the same cut-offs the review badges used. */
export const similarityTone = (value: number): SimilarityTone => (value >= 80 ? 'good' : value >= 60 ? 'fair' : 'weak');
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm vitest run app/utils/similarity.spec.ts` — Expected: PASS.

- [ ] **Step 5: Write the four components**

`app/components/LedgerTable.vue`:

```vue
<template>
  <div class="ledger">
    <table>
      <slot />
    </table>
  </div>
</template>
```

`app/components/SegmentedControl.vue`:

```vue
<script setup lang="ts" generic="T extends string">
defineProps<{ items: readonly { value: T; label: string }[]; ariaLabel?: string }>();
const model = defineModel<T>({ required: true });
</script>

<template>
  <div class="segmented" role="radiogroup" :aria-label="ariaLabel">
    <button
      v-for="i in items" :key="i.value" type="button" role="radio" :aria-checked="i.value === model"
      class="segmented-btn" @click="model = i.value"
    >
      {{ i.label }}
    </button>
  </div>
</template>
```

`app/components/SimilarityBar.vue`:

```vue
<script setup lang="ts">
const props = defineProps<{ value: number }>();
const clamped = computed(() => Math.max(0, Math.min(100, props.value)));
</script>

<template>
  <span class="tabular inline-flex items-center gap-2" :title="`شباهت ${faNumber(clamped)} درصد`">
    <span class="sim-track" aria-hidden="true">
      <span class="sim-fill" :data-tone="similarityTone(clamped)" :style="{ width: `${clamped}%` }" />
    </span>
    <span class="text-xs text-muted">{{ faNumber(clamped) }}٪</span>
  </span>
</template>
```

`app/components/EmptyState.vue`:

```vue
<script setup lang="ts">
withDefaults(defineProps<{ title: string; description?: string; art?: 'bridge' | 'shelf' }>(), { art: 'bridge' });
</script>

<template>
  <div class="flex flex-col items-center gap-3 px-6 py-12 text-center">
    <svg v-if="art === 'bridge'" width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-petrol-300 dark:text-petrol-700" aria-hidden="true">
      <path d="M4 22H92" />
      <path d="M20 22v10M76 22v10" />
      <path d="M20 32Q48 58 76 32" />
      <path d="M6 42q6-3 12 0t12 0 12 0 12 0 12 0 12 0 12 0" class="text-firoozeh-300 dark:text-firoozeh-800" />
    </svg>
    <svg v-else width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-petrol-300 dark:text-petrol-700" aria-hidden="true">
      <path d="M8 12H88M8 28H88M8 44H88" />
      <path d="M8 6v40M88 6v40" />
      <rect x="18" y="16" width="14" height="12" rx="1" class="text-firoozeh-300 dark:text-firoozeh-800" />
    </svg>
    <p class="font-semibold text-highlighted">{{ title }}</p>
    <p v-if="description" class="max-w-prose text-sm text-muted">{{ description }}</p>
    <div v-if="$slots.action" class="mt-1"><slot name="action" /></div>
  </div>
</template>
```

- [ ] **Step 6: Restyle `app/components/ErrorBanner.vue`**

Keep the script block as is; replace the template:

```vue
<template>
  <div v-if="error" role="alert" class="surface mb-6 border-s-[3px] border-s-sorkh-500 px-4 py-3">
    <p class="font-semibold text-error">
      <span :dir="isUnknownKind ? 'ltr' : undefined">{{ title }}</span>
    </p>
    <p class="mt-0.5 text-sm">{{ error.message }}</p>
    <p v-if="error.detail && error.detail !== error.message" dir="ltr" class="mt-1 text-xs text-muted">{{ error.detail }}</p>
  </div>
</template>
```

- [ ] **Step 7: Typecheck, test, commit**

Run: `pnpm typecheck && pnpm test` — Expected: clean; the palette, colour, similarity, fa, sequence suites pass.

```bash
git add app/utils/similarity.ts app/utils/similarity.spec.ts app/components/LedgerTable.vue app/components/SegmentedControl.vue app/components/SimilarityBar.vue app/components/EmptyState.vue app/components/ErrorBanner.vue
git commit -m "Add the shared ledger, segmented control, similarity bar and empty state

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Phase list per job kind and the bridge stepper

**Files:**
- Create: `app/utils/phases.ts`, `app/utils/phases.spec.ts`
- Modify: `app/components/JobProgress.vue`
- Modify: `app/pages/import.vue`, `app/pages/refresh.vue` (pass `kind` — template-only, one attribute each; the full page rewrites come in Task 8)

**Interfaces:**
- Produces: `phasesOf(kind: 'IMPORT' | 'REFRESH'): Phase[]`; `<JobProgress kind phase progress events status />` (new required prop `kind`).

- [ ] **Step 1: Write the failing phases test (it reads the Rust source so the lists cannot drift)**

`app/utils/phases.spec.ts`:

```ts
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { IMPORT_PHASES, REFRESH_PHASES, phasesOf } from './phases';

const rust = (file: string) => readFileSync(resolve(__dirname, '../../src-tauri/src/jobs', file), 'utf8');
const emitted = (src: string) => new Set([...src.matchAll(/ctx\.phase\("([A-Z]+)"\)/g)].map((m) => m[1]!));

describe('phasesOf', () => {
  it('lists the refresh phases the Rust job emits, in order', () => {
    const fromRust = new Set([...emitted(rust('mod.rs')), ...emitted(rust('refresh.rs'))]);
    expect(new Set(REFRESH_PHASES)).toEqual(fromRust);
    expect(REFRESH_PHASES).toEqual(['PREFLIGHT', 'PRICE', 'WRITE', 'REPRICE', 'REPORT']);
  });

  it('lists the import phases the Rust job emits plus REVIEW, in order', () => {
    const fromRust = new Set([...emitted(rust('mod.rs')), ...emitted(rust('import.rs')), 'REVIEW']);
    expect(new Set(IMPORT_PHASES)).toEqual(fromRust);
    expect(IMPORT_PHASES).toEqual(['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT']);
  });

  it('picks by kind', () => {
    expect(phasesOf('IMPORT')).toBe(IMPORT_PHASES);
    expect(phasesOf('REFRESH')).toBe(REFRESH_PHASES);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run app/utils/phases.spec.ts` — Expected: FAIL, module not found.

- [ ] **Step 3: Implement `app/utils/phases.ts`**

```ts
import type { Phase } from '~/types/bridge';

/** The phases each job kind walks through, in order; REVIEW only happens when an import has undecided items. */
export const IMPORT_PHASES: readonly Phase[] = ['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];
export const REFRESH_PHASES: readonly Phase[] = ['PREFLIGHT', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];

export const phasesOf = (kind: 'IMPORT' | 'REFRESH'): readonly Phase[] => (kind === 'IMPORT' ? IMPORT_PHASES : REFRESH_PHASES);
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm vitest run app/utils/phases.spec.ts` — Expected: PASS (3 tests).

- [ ] **Step 5: Rewrite `app/components/JobProgress.vue`**

```vue
<script setup lang="ts">
import type { JobEvent, Phase, Progress } from '~/types/bridge';

const props = defineProps<{
  kind: 'IMPORT' | 'REFRESH'; phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null;
}>();
const phases = computed(() => phasesOf(props.kind));
const index = computed(() => (props.phase ? phases.value.indexOf(props.phase) : -1));
const finished = computed(() => props.status === 'DONE');
const percent = computed(() =>
  props.progress && props.progress.total > 0 ? Math.round((props.progress.done / props.progress.total) * 100) : null);
const pierState = (i: number) => (finished.value || i < index.value ? 'done' : i === index.value ? 'current' : 'todo');
const statusColor = computed(() =>
  props.status === 'DONE' ? 'success' : props.status === 'FAILED' ? 'error' : props.status === 'REVIEW' ? 'warning' : 'neutral');
</script>

<template>
  <div class="space-y-5">
    <ol class="stepper" :style="{ '--n': phases.length }" aria-label="مراحل عملیات">
      <li v-for="(p, i) in phases" :key="p" class="stepper-step" :data-state="pierState(i)" :aria-current="i === index && !finished ? 'step' : undefined">
        <span class="stepper-pier" aria-hidden="true" />
        <span class="stepper-label">{{ phaseLabel[p] }}</span>
      </li>
    </ol>

    <div v-if="progress && progress.total > 0" class="space-y-1">
      <UProgress :model-value="percent ?? 0" size="sm" />
      <p class="tabular text-sm text-muted">{{ progress.message }} · {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>

    <div v-if="status" class="flex items-center gap-2 text-sm">
      <span class="text-muted">وضعیت</span>
      <UBadge :color="statusColor" variant="soft">{{ statusLabel[status] ?? status }}</UBadge>
    </div>

    <ul v-if="events.length" class="max-h-64 space-y-1.5 overflow-auto rounded-control bg-elevated/60 p-3 text-sm">
      <li v-for="(e, i) in events" :key="i" class="flex items-start gap-2">
        <span class="level-dot" :data-level="e.level" aria-hidden="true" />
        <span class="min-w-0">
          <span class="text-xs text-muted">
            <template v-if="phaseLabel[e.phase]">{{ phaseLabel[e.phase] }}</template>
            <bdi v-else dir="ltr">{{ e.phase }}</bdi>
          </span>
          <span class="block" :class="e.level === 'ERROR' ? 'text-error' : e.level === 'WARN' ? 'text-warning' : ''">{{ e.message }}</span>
        </span>
      </li>
    </ul>
  </div>
</template>
```

(The `·` above separates a message from a count inside one sentence-like line, which is what the spec allows; it is not a label join.)

- [ ] **Step 6: Pass `kind` from the two pages**

In `app/pages/import.vue`, change the `<JobProgress …>` line to:

```vue
      <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
```

In `app/pages/refresh.vue`:

```vue
      <JobProgress kind="REFRESH" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
```

- [ ] **Step 7: Screenshot the stepper**

Run: `scripts/shots.sh "$SCRATCH/shots-06-stepper" "import?state=running"` and Read both PNGs. Check: nine piers under one deck line, the first three turquoise with the deck filled up to them, «تطبیق با فهرست لندو» saffron and bold, the progress bar «۳۷ از ۸۹», three events with grey dots. Then `scripts/shots.sh "$SCRATCH/shots-06-stepper" "refreshing"` and Read `light-refresh_state_refreshing.png`: exactly five piers (بررسی اتصال‌ها، قیمت روز بارزپخش، نوشتن روی قفسه، اعمال قواعد قیمت‌گذاری لندو، گزارش نهایی), the third one saffron, «۲۱۰ از ۴۱۲».

- [ ] **Step 8: Typecheck, test, commit**

Run: `pnpm typecheck && pnpm test`

```bash
git add app/utils/phases.ts app/utils/phases.spec.ts app/components/JobProgress.vue app/pages/import.vue app/pages/refresh.vue
git commit -m "Draw job progress as bridge piers and show only the phases each kind emits

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Dashboard — setup checklist, bridge hero, attention rows

**Files:**
- Create: `app/components/SetupChecklist.vue`, `app/components/BridgeHero.vue`
- Modify: `app/pages/index.vue`
- Modify: `app/pages/logs.vue` (accept `?job=N` and open that job's detail — 4 lines; the full restyle is Task 10)

**Interfaces:**
- Consumes: `PageHeader`, `BrandLogo`, `RunningJobCard`, `.surface`, `.check`, `.draw`; `bridge.dashboardGet()`, `bridge.settingsGet()`.
- Produces: `<SetupChecklist :settings />`, `<BridgeHero :dash />`; `/logs?job=N` opens job N.

- [ ] **Step 1: Write `app/components/SetupChecklist.vue`**

```vue
<script setup lang="ts">
import type { SettingsView } from '~/types/bridge';

const props = defineProps<{ settings: SettingsView }>();
const steps = computed(() => [
  { label: 'کلید API لندو', done: props.settings.has_landau_token, hint: 'در لندو: حساب کاربری ← کلیدهای API؛ کلید فقط یک بار نمایش داده می‌شود' },
  { label: 'فروشگاه مقصد', done: !!props.settings.store, hint: 'بعد از «بررسی اتصال»، فروشگاهی را که ردیف‌ها روی قفسه‌اش نوشته می‌شود انتخاب کنید' },
  { label: 'ورود بارزپخش', done: !!props.settings.supplier_user && props.settings.has_supplier_password, hint: 'نام کاربری و کلمه عبور بارزپخش، سپس «آزمایش ورود»' },
]);
</script>

<template>
  <section class="surface flex flex-wrap items-center gap-8 p-8">
    <BrandLogo :size="96" class="shrink-0 text-primary" />
    <div class="min-w-0 flex-1">
      <h2 class="text-2xl">سه قدم تا اولین عبور</h2>
      <p class="mt-1 max-w-prose text-muted">پل وقتی باز می‌شود که هر دو کرانه وصل باشند.</p>
      <ol class="mt-5 space-y-3">
        <li v-for="(s, i) in steps" :key="s.label" class="flex items-start gap-3">
          <span class="check mt-0.5" :class="s.done && 'is-done'">
            <UIcon v-if="s.done" name="i-lucide-check" class="size-3.5" />
            <template v-else>{{ faNumber(i + 1) }}</template>
          </span>
          <span class="min-w-0">
            <span class="block font-medium text-highlighted">{{ s.label }}</span>
            <span class="block text-sm text-muted">{{ s.hint }}</span>
          </span>
        </li>
      </ol>
      <UButton to="/settings" class="mt-6" icon="i-lucide-settings">رفتن به تنظیمات</UButton>
    </div>
  </section>
</template>
```

- [ ] **Step 2: Write `app/components/BridgeHero.vue`**

```vue
<script setup lang="ts">
import type { Dashboard } from '~/types/bridge';

const props = defineProps<{ dash: Dashboard }>();
const busy = computed(() => !!props.dash.running);
</script>

<template>
  <section class="surface relative overflow-hidden">
    <svg class="pointer-events-none absolute inset-x-0 top-0 h-full w-full" viewBox="0 0 1000 300" preserveAspectRatio="none" fill="none" aria-hidden="true">
      <!-- water -->
      <path d="M0 262q40-10 80 0t80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0" stroke="var(--color-firoozeh-200)" stroke-width="2" class="dark:[stroke:var(--color-firoozeh-900)]" />
      <path d="M0 280q40-8 80 0t80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0" stroke="var(--color-firoozeh-100)" stroke-width="2" class="dark:[stroke:var(--color-firoozeh-950)]" />
    </svg>
    <div class="relative grid gap-6 p-8 md:grid-cols-[1fr_1.4fr_1fr]">
      <!-- right bank (start side): Barezpakhsh -->
      <div class="min-w-0">
        <p class="text-sm font-medium text-muted">بارزپخش</p>
        <p class="tabular mt-1 text-[2.75rem] font-extrabold leading-none text-highlighted">{{ faNumber(dash.invoices_cached) }}</p>
        <p class="mt-1 text-sm">فاکتور ذخیره‌شده</p>
        <p v-if="dash.invoices_unpaid > 0" class="tabular text-sm text-muted">{{ faNumber(dash.invoices_unpaid) }} پرداخت‌نشده</p>
        <p v-if="dash.supplier_user" class="mt-3 truncate text-xs text-muted" dir="ltr">{{ dash.supplier_user }}</p>
      </div>

      <!-- the bridge -->
      <div class="flex min-w-0 flex-col items-center justify-end gap-4">
        <svg class="w-full max-w-[420px]" viewBox="0 0 420 120" fill="none" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M10 78H410" stroke="var(--color-firoozeh-600)" stroke-width="4" pathLength="1" class="draw motion-safe:[animation-delay:0ms]" />
          <path d="M90 78V26M330 78V26" stroke="var(--color-firoozeh-600)" stroke-width="4" pathLength="1" class="draw motion-safe:[animation-delay:150ms]" />
          <path d="M10 70Q50 30 90 26Q210 90 330 26Q370 30 410 70" stroke="var(--color-zaferan-400)" stroke-width="3" pathLength="1" class="draw motion-safe:[animation-delay:300ms]" />
          <path d="M120 44V78M150 55V78M180 63V78M210 66V78M240 63V78M270 55V78M300 44V78" stroke="var(--color-firoozeh-300)" stroke-width="2" pathLength="1" class="draw motion-safe:[animation-delay:600ms]" />
        </svg>
        <RunningJobCard v-if="dash.running" :job="dash.running" class="w-full !bg-firoozeh-600 !text-white hover:!bg-firoozeh-700" />
        <div v-else class="flex flex-wrap justify-center gap-3">
          <UButton size="lg" icon="i-lucide-file-input" to="/import" :disabled="busy">وارد کردن فاکتورها</UButton>
          <UButton size="lg" icon="i-lucide-refresh-cw" variant="soft" to="/refresh" :disabled="busy">به‌روزرسانی قیمت‌ها</UButton>
        </div>
      </div>

      <!-- left bank (end side): Landau -->
      <div class="min-w-0 md:text-left">
        <p class="text-sm font-medium text-muted">لندو</p>
        <p class="mt-1 truncate text-2xl font-bold leading-tight text-highlighted">{{ dash.store_name ?? '—' }}</p>
        <p class="tabular mt-3 text-[2rem] font-extrabold leading-none text-highlighted">{{ faNumber(dash.decided_matches) }}</p>
        <p class="mt-1 text-sm">تطبیق ثبت‌شده</p>
      </div>
    </div>
  </section>
</template>
```

- [ ] **Step 3: Rewrite `app/pages/index.vue`**

```vue
<script setup lang="ts">
import type { BridgeError, Dashboard, JobRow, SettingsView } from '~/types/bridge';

const dash = ref<Dashboard | null>(null);
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const load = async () => {
  try {
    dash.value = await bridge.dashboardGet();
    settings.value = dash.value.settings_complete ? null : await bridge.settingsGet();
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

const attention = computed(() => {
  const rows: { key: string; tone: 'info' | 'warn'; text: string; to: string; action: string }[] = [];
  const d = dash.value;
  if (!d) return rows;
  if (d.running?.status === 'REVIEW') rows.push({ key: 'review', tone: 'info', text: `${faNumber(d.pending_review)} قلم منتظر تصمیم شماست`, to: `/import?job=${d.running.id}`, action: 'ادامهٔ بازبینی' });
  if (d.recent_errors > 0) rows.push({ key: 'errors', tone: 'warn', text: `${faNumber(d.recent_errors)} خطا یا هشدار در هفت روز گذشته`, to: '/logs', action: 'مشاهده' });
  return rows;
});
const lastRuns = computed(() => [
  { label: 'آخرین وارد کردن', job: dash.value?.last_import ?? null },
  { label: 'آخرین به‌روزرسانی قیمت', job: dash.value?.last_refresh ?? null },
]);
const statusColor = (j: JobRow) => (j.status === 'DONE' ? 'success' : j.status === 'FAILED' ? 'error' : 'neutral');
</script>

<template>
  <div>
    <PageHeader :title="dash?.store_name ?? 'پل لندو'" description="پل بین فاکتورهای بارزپخش و قفسهٔ لندو" />
    <ErrorBanner :error="error" />

    <template v-if="dash">
      <SetupChecklist v-if="!dash.settings_complete && settings" :settings="settings" />
      <BridgeHero v-else :dash="dash" />

      <ul v-if="attention.length" class="surface mt-6 divide-y divide-default">
        <li v-for="a in attention" :key="a.key" class="flex items-center gap-3 px-5 py-3">
          <span class="level-dot !mt-0" :data-level="a.tone === 'warn' ? 'WARN' : 'PRIMARY'" aria-hidden="true" />
          <span class="tabular flex-1">{{ a.text }}</span>
          <UButton :to="a.to" size="sm" variant="soft">{{ a.action }}</UButton>
        </li>
      </ul>

      <section class="surface mt-6 divide-y divide-default">
        <div v-for="r in lastRuns" :key="r.label" class="flex flex-wrap items-center gap-x-4 gap-y-1 px-5 py-3">
          <span class="w-56 shrink-0 text-muted">{{ r.label }}</span>
          <template v-if="r.job">
            <UBadge :color="statusColor(r.job)" variant="soft">{{ statusLabel[r.job.status] ?? r.job.status }}</UBadge>
            <span class="tabular text-sm">{{ isoToLocal(r.job.started_at) }}</span>
            <UButton :to="`/logs?job=${r.job.id}`" size="xs" variant="ghost" class="ms-auto">جزئیات</UButton>
          </template>
          <span v-else class="text-sm text-muted">هنوز اجرا نشده</span>
        </div>
      </section>
    </template>
  </div>
</template>
```

- [ ] **Step 4: Let `/logs?job=N` open that job**

In `app/pages/logs.vue` `<script setup>`, add `const route = useRoute();` after the refs and change the `onMounted(load);` line to:

```ts
onMounted(async () => {
  await load();
  const wanted = Number(route.query.job);
  const j = jobs.value.find((x) => x.id === wanted);
  if (j) await open(j);
});
```

(`open` is declared with `const` below `onMounted` in the current file — move the `onMounted(...)` call to after the `open` declaration so it is defined when called.)

- [ ] **Step 5: Screenshot and critique**

Run: `scripts/shots.sh "$SCRATCH/shots-07-dash" "/?state"` and Read all four PNGs (done/empty × light/dark).
Check: the hero reads as one bridge between two banks, the big number on the right bank and the store name on the left, the two actions sitting under the deck, water lines faint; attention rows and last-run rows quiet beneath; the empty state shows the checklist with the large mark. Confirm nothing uses saffron other than the cable. Fix and re-shoot until it matches.

- [ ] **Step 6: Typecheck, test, commit**

Run: `pnpm typecheck && pnpm test`

```bash
git add app/components/SetupChecklist.vue app/components/BridgeHero.vue app/pages/index.vue app/pages/logs.vue
git commit -m "Turn the dashboard into the bridge between the two banks

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Import and refresh pages, ReviewTable, ReportTable

**Files:**
- Modify: `app/pages/import.vue`, `app/pages/refresh.vue`, `app/components/ReviewTable.vue`, `app/components/ReportTable.vue`

**Interfaces:**
- Consumes: `PageHeader`, `JobProgress` (`kind`), `LedgerTable`, `SegmentedControl`, `SimilarityBar`, `EmptyState`, `.ticket*`, `.surface*`.
- Props of `ReviewTable`/`ReportTable` unchanged.

- [ ] **Step 1: Rewrite the template of `app/pages/import.vue`** (script block unchanged)

```vue
<template>
  <div>
    <PageHeader title="وارد کردن فاکتورهای بارزپخش" description="فاکتورهای پرداخت‌شده خوانده می‌شود، هر قلم به قطعهٔ لندو وصل می‌شود و با قیمت روز روی قفسه می‌نشیند.">
      <template #actions>
        <UButton v-if="active" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="space-y-4">
      <div class="grid gap-4 md:grid-cols-2">
        <label class="ticket" :data-selected="mode === 'QUANTITY'">
          <input v-model="mode" type="radio" name="mode" value="QUANTITY" class="sr-only" />
          <UIcon v-if="mode === 'QUANTITY'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="block text-lg font-semibold text-highlighted">با تعداد، از تاریخ</span>
          <span class="mt-1 block text-sm text-muted">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد، منهای مرجوعی‌ها، به قفسه اضافه می‌شود. اگر قطعه از قبل روی قفسه باشد به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</span>
          <UFormField label="از تاریخ" class="mt-4">
            <UInput v-model="since" placeholder="۱۴۰۵/۰۱/۰۱" dir="ltr" class="w-40" :color="sinceOk ? 'primary' : 'error'" :disabled="mode !== 'QUANTITY'" />
          </UFormField>
        </label>
        <label class="ticket" :data-selected="mode === 'ZERO'">
          <input v-model="mode" type="radio" name="mode" value="ZERO" class="sr-only" />
          <UIcon v-if="mode === 'ZERO'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="block text-lg font-semibold text-highlighted">با تعداد صفر، برای بازبینی</span>
          <span class="mt-1 block text-sm text-muted">همهٔ اقلام همهٔ فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</span>
        </label>
      </div>
      <p class="max-w-prose text-sm text-muted">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لندو از روی قیمت روز بارزپخش با ارزش افزوده محاسبه می‌شود، نه قیمت روی فاکتور.</p>
      <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع وارد کردن</UButton>
    </section>

    <section v-if="job.jobId.value" class="surface mt-6">
      <div class="surface-head">
        <h2>عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
      </div>
      <div class="surface-body">
        <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
      </div>
    </section>

    <section v-if="job.status.value === 'REVIEW'" class="mt-6">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <h2 class="text-xl">بازبینی اقلام تطبیق‌نشده <span class="tabular text-muted">({{ faNumber(review.length) }})</span></h2>
        <UButton icon="i-lucide-arrow-left" trailing @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
      </div>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </section>

    <section v-if="finished && job.detail.value" class="surface mt-6">
      <div class="surface-head"><h2>گزارش</h2></div>
      <div class="surface-body">
        <ReportTable :items="job.detail.value.items" kind="IMPORT" />
      </div>
    </section>
  </div>
</template>
```

- [ ] **Step 2: Rewrite the template of `app/pages/refresh.vue`** (script block unchanged)

```vue
<template>
  <div>
    <PageHeader title="به‌روزرسانی قیمت‌های قفسه" description="قیمت خرید هر ردیف از قیمت روز بارزپخش تازه می‌شود و قواعد قیمت‌گذاری لندو دوباره اجرا می‌شود.">
      <template #actions>
        <UButton v-if="active" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="surface">
      <div class="surface-body space-y-5">
        <p class="max-w-prose text-sm text-muted">برای هر ردیف قفسه که به یک کد بارزپخش وصل است، قیمت روز بارزپخش با ارزش افزوده به عنوان قیمت خرید ثبت می‌شود و سپس قواعد قیمت‌گذاری لندو روی کل فروشگاه اجرا می‌شود. ردیف‌هایی که به بارزپخش وصل نیستند در گزارش فهرست می‌شوند و دست نمی‌خورند.</p>
        <UFormField label="قیمت‌های دستی لندو را هم بازنویسی کن" hint="لندو قیمتی را که خودتان در پنل تایپ کرده باشید حفظ می‌کند مگر این گزینه روشن باشد">
          <USwitch v-model="overwrite" />
        </UFormField>
        <UButton size="lg" icon="i-lucide-refresh-cw" :loading="starting" @click="start">شروع به‌روزرسانی</UButton>
      </div>
    </section>

    <section v-if="job.jobId.value" class="surface mt-6">
      <div class="surface-head">
        <h2>عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
      </div>
      <div class="surface-body">
        <JobProgress kind="REFRESH" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
      </div>
    </section>

    <section v-if="finished && job.detail.value" class="surface mt-6">
      <div class="surface-head"><h2>گزارش</h2></div>
      <div class="surface-body">
        <ReportTable :items="job.detail.value.items" kind="REFRESH" />
      </div>
    </section>
  </div>
</template>
```

- [ ] **Step 3: Rewrite the template of `app/components/ReviewTable.vue`** (script block unchanged)

```vue
<template>
  <div class="space-y-4">
    <ErrorBanner :error="error" />
    <p class="max-w-prose text-sm text-muted">برای هر قلم، قطعهٔ درست را از فهرست لندو انتخاب کنید. «هیچ‌کدام» یعنی این قطعه در لندو نیست: دیگر پرسیده نمی‌شود و باید برای افزودنش تیکتی برای پشتیبانی لندو ثبت کنید. «بعداً» آن را برای اجرای بعدی نگه می‌دارد.</p>
    <section v-for="(item, n) in items" :key="item.id" class="surface">
      <div class="surface-head flex-wrap">
        <div class="min-w-0">
          <h3 class="text-[1.0625rem]">{{ item.name }}</h3>
          <p class="tabular text-xs text-muted"><span dir="ltr">{{ item.code }}</span> · تعداد {{ faNumber(item.qty) }} · {{ faNumber(n + 1) }} از {{ faNumber(items.length) }}</p>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
          <UButton size="sm" color="neutral" variant="ghost" @click="decide(item, { later: true })">بعداً</UButton>
          <UButton size="sm" color="error" variant="soft" @click="decide(item, { none: true })">هیچ‌کدام</UButton>
        </div>
      </div>
      <div class="surface-body space-y-3">
        <div class="flex gap-2">
          <UInput v-model="terms[item.id]" placeholder="جست‌وجوی دوباره در لندو با عبارت دیگر" class="flex-1" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" variant="soft" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-muted">لندو پیشنهادی نداشت. عبارت دیگری را امتحان کنید یا «هیچ‌کدام» را بزنید.</p>
        <ul v-else class="divide-y divide-default">
          <li v-for="c in candidatesOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
            <div class="min-w-0">
              <p class="truncate">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-muted">{{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-muted" dir="auto">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              <SimilarityBar :value="c.similarity" />
              <UButton size="sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">همین است</UButton>
            </div>
          </li>
        </ul>
      </div>
    </section>
  </div>
</template>
```

- [ ] **Step 4: Rewrite the template of `app/components/ReportTable.vue`** (script block unchanged, but add one line after `const rows = …`:)

```ts
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'DONE', label: 'انجام‌شده' }, { value: 'SKIPPED', label: 'ردشده' }, { value: 'FAILED', label: 'ناموفق' }] as const;
```

Template:

```vue
<template>
  <div class="space-y-3">
    <div class="flex flex-wrap items-center gap-3">
      <SegmentedControl v-model="filter" :items="filters" aria-label="فیلتر ردیف‌ها" />
      <span class="tabular ms-auto text-sm text-muted">{{ faNumber(rows.length) }} ردیف</span>
      <UButton size="sm" variant="soft" icon="i-lucide-download" :loading="saving" @click="csv">خروجی CSV</UButton>
    </div>
    <ErrorBanner :error="error" />
    <LedgerTable v-if="rows.length">
      <thead>
        <tr>
          <th>قطعه</th>
          <th>اقدام</th>
          <th>تعداد</th>
          <th>قیمت خرید</th>
          <th>قیمت قفسه</th>
          <th>نتیجهٔ قیمت‌گذاری</th>
          <th>وضعیت</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="i in rows" :key="i.id">
          <td>
            <p class="font-medium text-highlighted">{{ i.product_name ?? i.name }}</p>
            <p class="text-xs text-muted">{{ i.name }} <span dir="ltr">{{ i.code }}</span></p>
          </td>
          <td>{{ i.action ? actionLabel[i.action] : '—' }}</td>
          <td>
            <span v-if="i.qty_before != null" class="block text-xs text-muted">{{ faNumber(i.qty_before) }}</span>
            <span class="font-medium">{{ faNumber(i.qty_after ?? i.qty) }}</span>
          </td>
          <td>{{ rials(i.cost) }}</td>
          <td>
            <span v-if="i.price_before != null" class="block text-xs text-muted">{{ faNumber(i.price_before) }}</span>
            <span class="font-medium">{{ faNumber(i.price_after) }}</span>
          </td>
          <td>{{ verdictLabel(i.reprice_verdict) }}</td>
          <td>
            <UBadge :color="i.status === 'DONE' ? 'success' : i.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[i.status] ?? i.status }}</UBadge>
            <p v-if="i.error_msg" class="mt-1 text-xs text-error">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
          </td>
        </tr>
      </tbody>
    </LedgerTable>
    <EmptyState v-else art="shelf" title="ردیفی با این فیلتر نیست" description="فیلتر دیگری را انتخاب کنید." />
  </div>
</template>
```

(`SegmentedControl` is generic over the item values; `filter` is typed `'ALL' | 'DONE' | 'SKIPPED' | 'FAILED'` and `filters` is `as const`, so `v-model` type-checks. If `vue-tsc` complains about `readonly` items, change the prop in `SegmentedControl` to `items: readonly { readonly value: T; readonly label: string }[]`.)

- [ ] **Step 5: Screenshot and critique**

Run: `scripts/shots.sh "$SCRATCH/shots-08-import" "import"` then `scripts/shots.sh "$SCRATCH/shots-08-import" "refresh"`. Read: `light-import_state_done.png` (tickets + report), `dark-import_state_review.png` (three review items, bars, «همین است»), `light-import_state_running.png`, `light-refresh_state_done.png`.
Check: selected ticket has the turquoise border and check mark; the date field is disabled when the second ticket is selected (toggle mentally — verified in the real app in Task 12); report before/after stacked; similarity bars green/saffron/grey per value; nothing overflows at 1200 px.

- [ ] **Step 6: Typecheck, test, commit**

Run: `pnpm typecheck && pnpm test`

```bash
git add app/pages/import.vue app/pages/refresh.vue app/components/ReviewTable.vue app/components/ReportTable.vue
git commit -m "Restyle the import and refresh flows: tickets, review blocks, ledger report

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Matches page

**Files:**
- Modify: `app/pages/matches.vue`

- [ ] **Step 1: Add the filter items to the script** (after `const howLabel = …`):

```ts
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'AUTO', label: 'خودکار' }, { value: 'USER', label: 'انتخاب شما' }, { value: 'NONE', label: 'در لندو نیست' }] as const;
```

- [ ] **Step 2: Rewrite the template**

```vue
<template>
  <div>
    <PageHeader title="تطبیق‌های ثبت‌شده" description="هر کد بارزپخش یک بار به یک قطعهٔ لندو وصل می‌شود و دیگر پرسیده نمی‌شود." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3">
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <UInput v-model="query" icon="i-lucide-search" placeholder="جست‌وجو در نام یا کد" class="ms-auto w-72" />
    </div>

    <LedgerTable v-if="shown.length">
      <thead>
        <tr>
          <th>نام در فاکتور</th>
          <th>کد</th>
          <th>قطعهٔ لندو</th>
          <th>نحوه</th>
          <th>شباهت</th>
          <th>تاریخ</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in shown" :key="r.code">
          <td class="font-medium text-highlighted">{{ r.name_seen }}</td>
          <td><span dir="ltr">{{ r.code }}</span></td>
          <td>{{ r.product_name ?? '—' }}</td>
          <td><UBadge :color="r.how === 'NONE' ? 'error' : r.how === 'USER' ? 'info' : 'success'" variant="soft">{{ howLabel[r.how] }}</UBadge></td>
          <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-muted">—</span></td>
          <td class="whitespace-nowrap">{{ isoToLocal(r.decided_at) }}</td>
          <td class="text-end"><UButton size="xs" variant="soft" @click="openEdit(r)">تغییر</UButton></td>
        </tr>
      </tbody>
    </LedgerTable>
    <div v-else class="surface">
      <EmptyState
        :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها، تطبیق‌های خودکار و تصمیم‌های شما اینجا جمع می‌شود. برای اقلامی که «در لندو نیست» علامت خورده‌اند، تیکتی برای پشتیبانی لندو ثبت کنید و بعد تصمیم را همین‌جا تغییر دهید.'"
      >
        <template v-if="!rows.length" #action><UButton to="/import" variant="soft" icon="i-lucide-file-input">وارد کردن فاکتورها</UButton></template>
      </EmptyState>
    </div>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-4">
          <p><span class="font-medium text-highlighted">{{ editing.name_seen }}</span> <span dir="ltr" class="text-sm text-muted">{{ editing.code }}</span></p>
          <div class="flex gap-2">
            <UInput v-model="term" class="flex-1" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لندو</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-default overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
              <span class="min-w-0 truncate">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-muted">{{ c.candidate.brand }}</span></span>
              <span class="flex shrink-0 items-center gap-3"><SimilarityBar :value="c.similarity" /><UButton size="xs" @click="pick(c)">انتخاب</UButton></span>
            </li>
          </ul>
          <div class="flex flex-wrap justify-between gap-2 border-t border-default pt-4">
            <UButton color="error" variant="soft" @click="setNone">در لندو نیست</UButton>
            <UButton color="neutral" variant="ghost" @click="clear">حذف تصمیم، دوباره پرسیده شود</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
```

- [ ] **Step 3: Screenshot and critique**

Run: `scripts/shots.sh "$SCRATCH/shots-09-matches" "matches"`. Read `light-matches_state_done.png`, `dark-matches_state_done.png`, `light-matches_state_empty.png`. Check: the ledger's muted header row, bars in three tones, the `NONE` rows with «—», the empty state with the bridge line art and the action.

- [ ] **Step 4: Typecheck, test, commit**

```bash
pnpm typecheck && pnpm test
git add app/pages/matches.vue
git commit -m "Restyle the matches ledger and its edit dialog

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Logs page

**Files:**
- Modify: `app/pages/logs.vue`

- [ ] **Step 1: Add tab items to the script**

Change `const tab = ref<'jobs' | 'errors'>('jobs');` to `const tab = ref<string>('jobs');` (`UTabs` writes back `string | number`), then add after it:

```ts
const tabs = computed(() => [
  { value: 'jobs', label: `عملیات‌ها (${faNumber(jobs.value.length)})` },
  { value: 'errors', label: `خطاها و هشدارها (${faNumber(errors.value.length)})` },
]);
const jobStatusColor = (s: string) => (s === 'DONE' ? 'success' : s === 'FAILED' ? 'error' : 'neutral');
```

- [ ] **Step 2: Rewrite the template**

```vue
<template>
  <div>
    <PageHeader title="گزارش و خطاها" description="هر عملیات با رویدادها و ردیف‌هایش، و هر خطا یا هشداری که ثبت شده.">
      <template #actions>
        <UButton variant="ghost" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
        <UButton variant="soft" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <UTabs v-model="tab" :items="tabs" :content="false" variant="pill" class="mb-4 w-fit" />

    <div v-if="tab === 'jobs'" class="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)]">
      <LedgerTable v-if="jobs.length">
        <thead>
          <tr><th>#</th><th>نوع</th><th>شروع</th><th>وضعیت</th><th>مرحله</th></tr>
        </thead>
        <tbody>
          <tr v-for="j in jobs" :key="j.id" class="cursor-pointer" :aria-selected="detail?.job.id === j.id" @click="open(j)">
            <td>{{ faNumber(j.id) }}</td>
            <td>
              {{ kindLabelOf(j) }}
              <bdi v-if="j.kind === 'IMPORT' && j.mode === 'QUANTITY' && j.since_date" dir="ltr" class="block text-xs text-muted">{{ j.since_date }}</bdi>
            </td>
            <td class="whitespace-nowrap">{{ isoToLocal(j.started_at) }}</td>
            <td><UBadge :color="jobStatusColor(j.status)" variant="soft">{{ statusLabel[j.status] ?? j.status }}</UBadge></td>
            <td>{{ phaseLabel[j.phase] ?? j.phase }}</td>
          </tr>
        </tbody>
      </LedgerTable>
      <div v-else class="surface"><EmptyState title="هنوز عملیاتی اجرا نشده" description="اولین وارد کردن یا به‌روزرسانی اینجا ثبت می‌شود." /></div>

      <section v-if="detail" class="surface">
        <div class="surface-head">
          <h2>عملیات <span class="tabular">{{ faNumber(detail.job.id) }}</span></h2>
          <UBadge :color="jobStatusColor(detail.job.status)" variant="soft">{{ statusLabel[detail.job.status] ?? detail.job.status }}</UBadge>
        </div>
        <div class="surface-body space-y-4">
          <ul class="max-h-80 space-y-1.5 overflow-auto text-sm">
            <li v-for="e in detail.events" :key="e.id" class="flex items-start gap-2">
              <span class="level-dot" :data-level="e.level" aria-hidden="true" />
              <span class="min-w-0">
                <span class="tabular text-xs text-muted">{{ isoToLocal(e.at) }} · {{ phaseLabel[e.phase] ?? e.phase }}</span>
                <span class="block break-words" :class="e.level === 'ERROR' ? 'text-error' : e.level === 'WARN' ? 'text-warning' : ''">{{ e.message }}</span>
              </span>
            </li>
          </ul>
          <details v-if="detail.job.summary" class="text-sm">
            <summary class="cursor-pointer text-muted">خلاصهٔ خام</summary>
            <pre dir="ltr" class="mt-2 overflow-auto rounded-control bg-elevated p-3 text-xs">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
          </details>
          <ReportTable :items="detail.items" :kind="detail.job.kind" />
        </div>
      </section>
      <div v-else-if="jobs.length" class="surface"><EmptyState title="یک عملیات را انتخاب کنید" description="رویدادها و ردیف‌های آن اینجا نشان داده می‌شود." /></div>
    </div>

    <template v-else>
      <LedgerTable v-if="errors.length">
        <thead>
          <tr><th>زمان</th><th>عملیات</th><th>نوع</th><th>پیام</th></tr>
        </thead>
        <tbody>
          <tr v-for="e in errors" :key="e.id">
            <td class="whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td>{{ e.job_id != null ? faNumber(e.job_id) : '—' }}</td>
            <td><UBadge variant="soft" color="neutral">{{ kindLabel(e.kind) }}</UBadge></td>
            <td>
              {{ e.message }}
              <details v-if="e.context" class="mt-1">
                <summary class="cursor-pointer text-xs text-muted">جزئیات</summary>
                <pre dir="ltr" class="mt-1 overflow-auto rounded-control bg-elevated p-2 text-xs">{{ JSON.stringify(e.context, null, 1) }}</pre>
              </details>
            </td>
          </tr>
        </tbody>
      </LedgerTable>
      <div v-else class="surface"><EmptyState title="خطا یا هشداری ثبت نشده" description="هر مشکلی در اجرا، با نوع و جزئیاتش، اینجا فهرست می‌شود." /></div>
    </template>
  </div>
</template>
```

(`UTabs` `v-model` holds the item's `value`; with `:content="false"` the page renders its own panels, as before.)

- [ ] **Step 3: Screenshot and critique**

Run: `scripts/shots.sh "$SCRATCH/shots-10-logs" "logs"`. Read the four PNGs (`logs_state_done` with no job open, `logs_state_done_job_42` with job 42 open, both modes). Check: two-column layout, the selected row's start-side turquoise bar, the events list with level dots, the report beneath, the empty «یک عملیات را انتخاب کنید» panel when nothing is open.

- [ ] **Step 4: Typecheck, test, commit**

```bash
pnpm typecheck && pnpm test
git add app/pages/logs.vue
git commit -m "Restyle the logs page: tabs, ledger, side panel

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Settings page

**Files:**
- Modify: `app/pages/settings.vue`

- [ ] **Step 1: Add the status chips to the script** (after `const busy = …` and before `const form = …`):

```ts
const supplierOk = ref(false);
const landauChip = computed(() =>
  landau.value ? { label: `متصل — ${landau.value.business_name ?? landau.value.business}`, color: 'success' as const }
    : view.value?.has_landau_token ? { label: 'کلید ذخیره‌شده', color: 'neutral' as const }
      : { label: 'آزمایش‌نشده', color: 'warning' as const });
const supplierChip = computed(() =>
  supplierOk.value ? { label: 'ورود موفق', color: 'success' as const }
    : view.value?.has_supplier_password ? { label: 'رمز ذخیره‌شده', color: 'neutral' as const }
      : { label: 'آزمایش‌نشده', color: 'warning' as const });
```

and in `testSupplier`, after `await bridge.supplierTestLogin();` add `supplierOk.value = true;`.

(The chip label uses « — » inside a status *value*, joining a state and a name the seller reads as one phrase; it is not a label pattern.)

- [ ] **Step 2: Rewrite the template**

```vue
<template>
  <div class="mx-auto max-w-3xl">
    <PageHeader title="تنظیمات" description="اتصال‌ها و قواعد وارد کردن." />
    <ErrorBanner :error="error" />

    <div class="surface divide-y divide-default">
      <section class="p-6">
        <div class="mb-4 flex items-center justify-between gap-3">
          <h2>لندو</h2>
          <UBadge :color="landauChip.color" variant="soft">{{ landauChip.label }}</UBadge>
        </div>
        <div class="space-y-4">
          <UFormField label="نشانی درگاه لندو" hint="مثال: https://api.landau.ir">
            <UInput v-model="form.landau_url" dir="ltr" class="w-full" />
          </UFormField>
          <UFormField label="کلید API لندو" :hint="view?.has_landau_token ? 'کلیدی ذخیره شده است؛ برای تغییر، کلید تازه را وارد کنید' : 'در لندو: حساب کاربری ← کلیدهای API'">
            <UInput v-model="form.landau_token" type="password" dir="ltr" placeholder="apt-…" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'landau'" variant="soft" icon="i-lucide-plug-zap" @click="validateLandau">بررسی اتصال و دریافت فروشگاه‌ها</UButton>
          <p v-if="landau && landau.active_rules === 0" class="text-sm text-warning">هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید؛ تا زمانی که در لندو قاعده‌ای بسازید، قیمت قفسه برابر قیمت خرید خواهد بود.</p>
          <p v-else-if="landau" class="tabular text-sm text-muted">{{ faNumber(landau.active_rules) }} قاعدهٔ قیمت‌گذاری فعال</p>
          <UFormField label="فروشگاه مقصد" hint="ردیف‌ها روی قفسهٔ این فروشگاه نوشته می‌شوند">
            <USelect v-model="form.store" :items="storeItems" :disabled="!storeItems.length" placeholder="ابتدا اتصال را بررسی کنید" class="w-full" />
            <p v-if="!storeItems.length && view?.store_name" class="mt-1 text-sm text-muted">فروشگاه ذخیره‌شده: {{ view.store_name }}</p>
          </UFormField>
        </div>
      </section>

      <section class="p-6">
        <div class="mb-4 flex items-center justify-between gap-3">
          <h2>بارزپخش</h2>
          <UBadge :color="supplierChip.color" variant="soft">{{ supplierChip.label }}</UBadge>
        </div>
        <div class="space-y-4">
          <UFormField label="نام کاربری"><UInput v-model="form.supplier_user" dir="ltr" class="w-full" /></UFormField>
          <UFormField label="کلمه عبور" :hint="view?.has_supplier_password ? 'ذخیره شده؛ برای تغییر، مقدار تازه را وارد کنید' : ''">
            <UInput v-model="form.supplier_password" type="password" dir="ltr" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'supplier'" variant="soft" icon="i-lucide-log-in" @click="testSupplier">آزمایش ورود</UButton>
        </div>
      </section>

      <section class="p-6">
        <h2 class="mb-4">قواعد وارد کردن</h2>
        <div class="space-y-4">
          <UFormField label="اصالت پیش‌فرض ردیف‌های تازه" hint="وضعیت همیشه «نو» ثبت می‌شود؛ اصالت را می‌توانید برای هر قلم در مرحلهٔ بازبینی تغییر دهید">
            <USelect v-model="form.default_authenticity" :items="authItems" class="w-full" />
          </UFormField>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="آستانهٔ تطبیق خودکار" hint="از ۰ تا ۱۰۰؛ بالاتر یعنی محتاط‌تر. پیش‌فرض با آزمایش روی دادهٔ واقعی انتخاب شده">
              <UInput v-model.number="form.threshold" type="number" min="0" max="100" dir="ltr" class="w-32" />
            </UFormField>
            <UFormField label="نرخ ارزش افزوده (٪)" hint="قیمت خرید = قیمت فهرست بارزپخش × (۱ + نرخ)">
              <UInput v-model.number="form.vat_percent" type="number" min="0" max="100" step="0.5" dir="ltr" class="w-32" />
            </UFormField>
          </div>
          <UFormField label="قیمت‌های دستی لندو را هم بازنویسی کن" hint="لندو قیمتی را که خودتان تایپ کرده باشید نگه می‌دارد؛ این گزینه همان «بازنویسی» لندوست">
            <USwitch v-model="form.overwrite_manual" />
          </UFormField>
        </div>
      </section>

      <section class="p-6 text-sm">
        <h2 class="mb-3">داده‌ها</h2>
        <p>پوشهٔ داده: <code dir="ltr">{{ view?.data_dir }}</code></p>
        <p class="mt-2" :class="view?.secret_backend === 'file' ? 'text-warning' : 'text-muted'">
          {{ view?.secret_backend === 'keyring' ? 'رمزها در ذخیره‌ساز امن سیستم‌عامل نگه داشته می‌شوند.' : 'ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده‌ای با دسترسی محدود در پوشهٔ داده نگه داشته می‌شوند.' }}
        </p>
      </section>
    </div>

    <div class="sticky bottom-0 -mx-8 mt-6 flex justify-end border-t border-default bg-muted/85 px-8 py-3 backdrop-blur">
      <UButton size="lg" :loading="busy === 'save'" icon="i-lucide-save" @click="save">ذخیره تنظیمات</UButton>
    </div>
  </div>
</template>
```

- [ ] **Step 3: Screenshot and critique**

Run: `scripts/shots.sh "$SCRATCH/shots-11-settings" "settings"`. Read both. Check: one surface, dividers between the four sections, chips on the section titles, the sticky save bar at the bottom edge, LTR fields aligned left inside RTL labels.

- [ ] **Step 4: Typecheck, test, commit**

```bash
pnpm typecheck && pnpm test
git add app/pages/settings.vue
git commit -m "Restyle settings: sectioned surface, connection chips, sticky save bar

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Final verification — full screenshot pass, reduced motion, real app, docs

**Files:**
- Modify: `README.md` (developer section), `docs/RUNBOOK.md` (one paragraph on the fixture mode)

- [ ] **Step 1: Full screenshot pass**

Run: `scripts/shots.sh "$SCRATCH/shots-12-final"` and Read all 24 PNGs, one by one. For each, tick the spec checklist: contrast, no saffron outside its four uses, no grey-card grid, tabular numbers aligned in tables, no clipped RTL text, dark mode surfaces distinct from the page. Fix anything found in the page it belongs to; re-shoot that page; commit fixes as `Polish <page> after the screenshot review`.

- [ ] **Step 2: Reduced-motion check**

Run:
```sh
google-chrome-stable --headless=new --no-proxy-server --disable-gpu --hide-scrollbars --no-sandbox --window-size=1200,800 \
  --force-prefers-reduced-motion --virtual-time-budget=100 --screenshot="$SCRATCH/reduced.png" "http://localhost:3210/?state=done&mode=light"
```
Read `reduced.png`: the bridge must be fully drawn at 100 ms of virtual time (no partial strokes). If Chrome ignores the flag, temporarily set `@media (prefers-reduced-motion: reduce)` to `all` in `main.css`, re-shoot, and revert.

- [ ] **Step 3: Grep for leftovers**

Run: `grep -rn "neutral-\(50\|100\|200\|500\|600\|700\|800\|900\|950\)\|text-red-\|text-amber-\|bg-amber-\|emerald-" app/ --include=*.vue`
Expected: no matches (all old Tailwind palette classes are gone). Fix any remaining ones with the token utilities.

- [ ] **Step 4: Build the production bundle and confirm the mock is absent**

Run: `pnpm generate && grep -rl "mock: unknown command" .output/public/_nuxt || echo "mock not in bundle"`
Expected: `mock not in bundle`.

- [ ] **Step 5: Run the real app once**

Run: `pnpm tauri dev` (background). With the fixtures from `docs/RUNBOOK.md` and memory (`landau-fixtures-for-bridge-tests`), walk: dashboard → settings (check chips, save) → import (ZERO mode, follow progress, review one item, finish) → matches (change one, revert it) → refresh → logs (open the job). Confirm the window icon is the new mark and the window title stays «پل لندو». Take one screenshot of the running app with `import -window root "$SCRATCH/real-app.png"` (X11) and Read it. Leave the fixture store as found (see the memory note).

- [ ] **Step 6: Docs**

In `README.md` under **For developers**, add after the Design line:

```
Visual design: `docs/superpowers/specs/2026-09-14-ui-redesign-design.md`. To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done&mode=dark` (`state` = `empty|running|review|done`; fixtures in `app/dev/mockBridge.ts`); `scripts/shots.sh OUT_DIR` screenshots every page in both modes.
```

In `docs/RUNBOOK.md`, add a short section «Design review without Tauri» with the same two sentences.

- [ ] **Step 7: Final checks and commit**

Run: `pnpm typecheck && pnpm test`

```bash
git add README.md docs/RUNBOOK.md
git commit -m "Document the fixture mode and the screenshot script

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

Then show the user the final screenshot set (paths under `$SCRATCH/shots-12-final`) and the real-app screenshot.
