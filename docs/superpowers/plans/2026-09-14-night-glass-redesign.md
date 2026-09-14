# Night-Glass Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the پل لاندا front end as a dark-only glass console with Ledger POS's structure and Landau's identity (crimson / sapphire / graphite, Mikhak + Estedad), a one-glance dashboard, three log pages (`/jobs`, `/jobs/:id`, `/errors`) and a pruned settings page — without changing anything behind the IPC boundary.

**Architecture:** Tokens live in `app/assets/css/main.css` (Tailwind 4 `@theme static` + unlayered `--ui-*` overrides on `:root`) and are mapped into `@nuxt/ui` by `app/app.config.ts`. Identity pieces (`GlowBackdrop`, `GlassCard`, `PageHeader`, `StatusPill`, `StatTile`, `JobSheet`, `EventTimeline`, `BridgeBand`) are small hand-built components; `@nuxt/ui` keeps modal, select, switch, toast, progress and form fields. Every page keeps its `<script setup>` data flow and swaps its template; the only new data composition is on the dashboard and the two new job pages, all from existing commands. A dev fixture mode plus a `?grid=1` overlay lets headless Chrome render every page for the alignment pass.

**Tech Stack:** Nuxt 4.5 (`app/` srcDir, `ssr: false`), Nuxt UI 4.11 (Tailwind 4, `colorMode: false`), Vue 3.5, `@internationalized/date` 3.12 (Jalali), Tauri 2 CLI (`tauri icon`), vitest 5, Google Chrome headless for screenshots.

**Spec:** `docs/superpowers/specs/2026-09-14-night-glass-redesign-design.md` — read it first; the plan argues from it. Section numbers below (§) refer to it.

## Global Constraints

- Nothing under `src-tauri/src`, no IPC command name/payload change, no edit to `app/types/bridge.ts`.
- Another session commits to `main` in this repo. Work on a branch created from the latest `main` (`git switch -c night-glass origin/main` or a worktree via superpowers:using-git-worktrees); rebase onto `main` before merging.
- UI is Persian only, RTL (`<html dir="rtl" lang="fa" class="dark">`). Latin data (codes, URLs, ISO dates, JSON, paths, usernames) is isolated with `dir="ltr"` or `<bdi>`. Every number the seller reads goes through `faNumber`; the kicker index is Persian («۰۱»).
- **No `letter-spacing`, no `uppercase`, no small-caps on Persian text — ever.** The kicker is `.kicker` (13 px, 500, crimson-400) and nothing else.
- Dark only. Surfaces: ink-950 `#0b0d12` page · ink-900 `#10131a` surface · ink-800 `#171b24` raised · ink-700 `#232833` accented. Text: paper `#f7f7f9` · paper-dim `#c2c3cd` · paper-mute `#9a9cab`. graphite-500 is never a text colour; danger text never sits on ink-700.
- Crimson only in its hot form (primary button gradient, active nav, kicker, links, focus ring, live dot, selected ticket, bridge cable, current pier, logo dots). Signals (ok `#8fbf8f`, warn `#d9a441`, danger `#cd7460`, info `#9ea3ff`) only as pills, dots, text, `.btn-danger`, the error banner's start bar and the job sheet's 6 % header tint. Never a filled badge.
- Fonts: Mikhak (display: `h1`–`h3`, `.display`, big numbers, wordmark) and Estedad (everything else), both self-hosted in `public/fonts/`; system monospace for codes. Vazirmatn is gone.
- Alignment (§3.4): 4 px grid, spacing steps 4/8/12/16/24/32/40; card padding 24; glass-inset 14 × 12; ledger cells 10 × 14 (head) / 12 × 14 (body); controls 36 (`sm` 30, `lg` 44); pills 22; nav items 38; equal-height cards per row; header actions at the inline-end, form submits at the inline-start.
- Motion: CSS-only reveal (`v-reveal` + `data-reveal`), page transition 280 ms, `.dot` pulse, stepper breathe, bridge draw-once; static backdrop; everything instant under `prefers-reduced-motion` and `html.fx-lite`.
- CSP in `src-tauri/tauri.conf.json` stays: no external fonts/scripts; inline SVG and `data:` images are fine.
- Commit after every task with the attribution line: `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- Screenshots go to the scratchpad directory (`$SCRATCH`, given in the session), never into the repo.
- Run `pnpm typecheck` and `pnpm test` before every commit; both must pass.

---

### Task 1: Tokens, fonts, Nuxt UI wiring, contrast test

**Files:**
- Modify: `app/assets/css/main.css` (rewrite)
- Modify: `app/app.config.ts` (rewrite)
- Modify: `nuxt.config.ts`
- Modify: `src-tauri/tauri.conf.json` (window `backgroundColor`)
- Create: `public/fonts/mikhak-var.woff2`, `public/fonts/estedad-var.woff2`; Delete: `public/fonts/Vazirmatn[wght].woff2`
- Modify: `app/utils/palette.spec.ts` (rewrite)

**Interfaces:**
- Produces: Tailwind colour utilities `*-graphite-N`, `*-crimson-N`, `*-sapphire-N`, `*-sage-N`, `*-amber-N`, `*-rust-N`, `*-ink-950|900|800|700`, `*-paper`, `*-paper-dim`, `*-paper-mute`, `*-line`, `*-line-bright`, `*-ok`, `*-warn`, `*-danger`, `*-info`; `font-display`, `font-sans`, `font-mono`; `shadow-glow-crimson`, `shadow-card`; `ease-swift`. CSS classes consumed by every later task: `.kicker`, `.display`, `.data-label`, `.tabular`, `.glass`, `.glass-inset`, `.hairline`, `.hover-glow`, `.dot`, `.btn-primary`, `.btn-ghost`, `.btn-danger`, `.btn-sm`, `.btn-lg`, `.field`, `.field-invalid`, `.code-block`, `.nav-item`, `.nav-active`, `.brand-tile`, `.segmented`, `.segmented-btn`, `.ticket`, `.ticket-check`, `.ledger`, `.ledger-roomy`, `.stepper*`, `.sim-track`, `.sim-fill`, `.level-dot`, `.check`, `.draw`, `.reveal-armed`, `.is-revealed`, `.page-enter-*`/`.page-leave-*`, `html.fx-lite` rules.
- Consumes: `contrastRatio(a, b)` from `app/utils/color.ts` (exists).

- [ ] **Step 1: Copy the fonts and drop Vazirmatn**

```bash
cp /home/vahid/Claude/examples/lnd-frontend/public/fonts/mikhak-var.woff2 public/fonts/
cp /home/vahid/Claude/examples/lnd-frontend/public/fonts/estedad-var.woff2 public/fonts/
git rm -q 'public/fonts/Vazirmatn[wght].woff2'
ls -la public/fonts
```
Expected: two `.woff2` files (≈65 KB and ≈118 KB), no Vazirmatn.

- [ ] **Step 2: Write the failing palette test**

Replace `app/utils/palette.spec.ts` with:

```ts
/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { contrastRatio } from './color';

const css = readFileSync(resolve(__dirname, '../assets/css/main.css'), 'utf8');

/** `--color-<name>-<stop>: #hex` pairs of one ramp. */
const scale = (name: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const m of css.matchAll(new RegExp(`--color-${name}-(\\d+):\\s*(#[0-9a-fA-F]{6})`, 'g'))) out[m[1]!] = m[2]!;
  return out;
};
/** Follows `var(--color-x-N)` (a ramp stop) or `var(--color-name)` (an alias) to its hex. */
const resolveRef = (raw: string): string => {
  const viaRamp = /^var\(--color-([a-z]+)-(\d+)\)$/.exec(raw);
  if (viaRamp) return scale(viaRamp[1]!)[viaRamp[2]!]!;
  const viaAlias = /^var\(--color-([a-z-]+)\)$/.exec(raw);
  return viaAlias ? token(viaAlias[1]!) : raw;
};
/** A single `--color-<name>: …` token (alias or literal), resolved to hex. */
const token = (name: string): string =>
  resolveRef(new RegExp(`--color-${name}:\\s*([^;]+);`).exec(css)?.[1]?.trim() ?? '');
/** The value a `--ui-<name>` variable resolves to on `:root`. */
const ui = (name: string): string => {
  const root = /:root\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
  return resolveRef(new RegExp(`--ui-${name}:\\s*([^;]+);`).exec(root)?.[1]?.trim() ?? '');
};

const atLeast = (fg: string, bg: string, min: number) =>
  expect(contrastRatio(fg, bg), `${fg} on ${bg}`).toBeGreaterThanOrEqual(min);

const SURFACES = () => [token('ink-950'), token('ink-900'), token('ink-800'), token('ink-700')];

describe('palette', () => {
  it('defines the six ramps with 11 stops each', () => {
    for (const name of ['graphite', 'crimson', 'sapphire', 'sage', 'amber', 'rust']) {
      expect(Object.keys(scale(name)), name).toHaveLength(11);
    }
  });

  it('pins the surfaces and text roles to the spec values', () => {
    expect(SURFACES()).toEqual(['#0b0d12', '#10131a', '#171b24', '#232833']);
    expect(token('paper')).toBe('#f7f7f9');
    expect(token('paper-dim')).toBe(scale('graphite')['300']);
    expect(token('paper-mute')).toBe(scale('graphite')['400']);
    expect(ui('text-dimmed')).toBe(scale('graphite')['400']);
  });

  it('text roles clear 4.5:1 on all four surfaces', () => {
    for (const bg of SURFACES()) {
      for (const fg of [token('paper'), token('paper-dim'), token('paper-mute')]) atLeast(fg, bg, 4.5);
    }
  });

  it('crimson in its lit form is readable everywhere it is used', () => {
    const crimson = scale('crimson');
    for (const bg of SURFACES()) atLeast(crimson['400']!, bg, 4.5); // kicker, links, active nav
    expect(ui('primary')).toBe(crimson['400']);
    atLeast('#ffffff', crimson['600']!, 4.5); // primary button gradient, top
    atLeast('#ffffff', crimson['800']!, 4.5); // primary button gradient, bottom
  });

  it('signals are readable where they may appear', () => {
    for (const bg of SURFACES()) {
      atLeast(token('ok'), bg, 4.5);
      atLeast(token('warn'), bg, 4.5);
      atLeast(token('info'), bg, 4.5);
    }
    for (const bg of SURFACES().slice(0, 3)) atLeast(token('danger'), bg, 4.5); // never on ink-700
    expect(ui('success')).toBe(token('ok'));
    expect(ui('warning')).toBe(token('warn'));
    expect(ui('error')).toBe(token('danger'));
    expect(ui('info')).toBe(token('info'));
  });

  it('control outlines identify the control (WCAG 1.4.11)', () => {
    atLeast(token('line-bright'), token('ink-900'), 3);
    atLeast(token('line-bright'), token('ink-800'), 3);
    expect(ui('border-accented')).toBe(token('line-bright'));
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `pnpm test -- app/utils/palette.spec.ts`
Expected: FAIL — `defines the six ramps` (no `--color-graphite-*` in the current CSS).

- [ ] **Step 4: Rewrite `app/assets/css/main.css`**

```css
@import "tailwindcss";
@import "@nuxt/ui";

/* ---- Fonts: Landau's two faces, self-hosted (OFL 1.1) ---------------------- */
@font-face { font-family: 'Mikhak'; src: url('/fonts/mikhak-var.woff2') format('woff2'); font-weight: 100 900; font-style: normal; font-display: swap; }
@font-face { font-family: 'Estedad'; src: url('/fonts/estedad-var.woff2') format('woff2'); font-weight: 100 900; font-style: normal; font-display: swap; }

/* ---- Tokens (spec §2) ------------------------------------------------------ */
@theme static {
  --font-display: 'Mikhak', 'Estedad', system-ui, sans-serif;
  --font-sans: 'Estedad', system-ui, sans-serif;
  --font-mono: ui-monospace, 'Cascadia Mono', 'Segoe UI Mono', Menlo, monospace;

  /* graphite — the blue-cast neutral (Landau) */
  --color-graphite-50: #f7f7f9;  --color-graphite-100: #efeff2; --color-graphite-200: #dfdfe5;
  --color-graphite-300: #c2c3cd; --color-graphite-400: #9a9cab; --color-graphite-500: #767989;
  --color-graphite-600: #5c5f6e; --color-graphite-700: #494b58; --color-graphite-800: #33353f;
  --color-graphite-900: #1b1d24; --color-graphite-950: #0b0d12;

  /* crimson — the blade of the ل (Landau; 400–800 are the logo's own stops) */
  --color-crimson-50: #fdf2f2;  --color-crimson-100: #fbe0e0; --color-crimson-200: #f6c4c4;
  --color-crimson-300: #ec9a9a; --color-crimson-400: #e06e6e; --color-crimson-500: #d63e3e;
  --color-crimson-600: #cc0e0e; --color-crimson-700: #b80d0d; --color-crimson-800: #a30b0b;
  --color-crimson-900: #870a0a; --color-crimson-950: #4a0505;

  /* sapphire — the bowl of the ل (Landau) */
  --color-sapphire-50: #eff0ff;  --color-sapphire-100: #e0e2ff; --color-sapphire-200: #c4c7ff;
  --color-sapphire-300: #9ea3ff; --color-sapphire-400: #757cff; --color-sapphire-500: #4f55f0;
  --color-sapphire-600: #3a3adb; --color-sapphire-700: #2e2bb8; --color-sapphire-800: #24228f;
  --color-sapphire-900: #191a72; --color-sapphire-950: #000077;

  /* signals — the 400 stop is the Ledger value; the rest exist for @nuxt/ui's ramp lookup */
  --color-sage-50: #f6faf6;  --color-sage-100: #edf5ed; --color-sage-200: #d9e9d9; --color-sage-300: #bcd9bc;
  --color-sage-400: #8fbf8f; --color-sage-500: #779f79; --color-sage-600: #5f7f62; --color-sage-700: #4a624e;
  --color-sage-800: #384a3d; --color-sage-900: #28342e; --color-sage-950: #1b2221;
  --color-amber-50: #fcf8f0;  --color-amber-100: #f9f0e1; --color-amber-200: #f2e0be; --color-amber-300: #e8c88d;
  --color-amber-400: #d9a441; --color-amber-500: #b48939; --color-amber-600: #8f6e30; --color-amber-700: #6e5529;
  --color-amber-800: #514022; --color-amber-900: #382e1c; --color-amber-950: #241f18;
  --color-rust-50: #fbf4f2;  --color-rust-100: #f7e9e6; --color-rust-200: #eed0c9; --color-rust-300: #e1aca0;
  --color-rust-400: #cd7460; --color-rust-500: #aa6152; --color-rust-600: #874f44; --color-rust-700: #683e37;
  --color-rust-800: #4d302d; --color-rust-900: #362423; --color-rust-950: #22191b;

  /* Ledger-named aliases the component CSS is written against */
  --color-ink-950: #0b0d12;
  --color-ink-900: #10131a;
  --color-ink-800: #171b24;
  --color-ink-700: #232833;
  --color-paper: #f7f7f9;
  --color-paper-dim: var(--color-graphite-300);
  --color-paper-mute: var(--color-graphite-400);
  --color-line: rgb(255 255 255 / 0.09);
  --color-line-bright: #6b6e7c;
  --color-ok: var(--color-sage-400);
  --color-warn: var(--color-amber-400);
  --color-danger: var(--color-rust-400);
  --color-info: var(--color-sapphire-300);

  --shadow-glow-crimson: 0 0 44px 0 rgb(214 62 62 / 0.16);
  --shadow-card: 0 1px 0 0 rgb(255 255 255 / 0.04) inset, 0 24px 48px -24px rgb(0 0 0 / 0.6);
  --ease-swift: cubic-bezier(0.22, 1, 0.36, 1);

  --text-xs: 0.8rem;            /* 12px */
  --text-xs--line-height: 1.5;
  --text-sm: 0.8667rem;         /* 13px */
  --text-sm--line-height: 1.6;
  --text-base: 1rem;            /* 15px */
  --text-base--line-height: 1.75;
  --text-lg: 1.1333rem;         /* 17px */
  --text-lg--line-height: 1.5;
  --text-xl: 1.3333rem;         /* 20px */
  --text-xl--line-height: 1.4;
  --text-2xl: 1.6rem;           /* 24px */
  --text-2xl--line-height: 1.3;
}

/* Nuxt UI semantic variables. Unlayered on purpose: @nuxt/ui sets its own inside @layer theme
   and an unlayered declaration wins over any layer. There is no light mode. */
:root {
  color-scheme: dark;
  --ui-radius: 0.5rem; /* rounded-md controls come out at 12 px */
  --ui-bg: var(--color-ink-900);
  --ui-bg-muted: var(--color-ink-950);
  --ui-bg-elevated: var(--color-ink-800);
  --ui-bg-accented: var(--color-ink-700);
  --ui-bg-inverted: var(--color-paper);
  --ui-border: rgb(255 255 255 / 0.09);
  --ui-border-muted: rgb(255 255 255 / 0.06);
  --ui-border-accented: var(--color-line-bright);
  --ui-text-highlighted: #ffffff;
  --ui-text: var(--color-paper);
  --ui-text-toned: var(--color-graphite-200);
  --ui-text-muted: var(--color-graphite-300);
  --ui-text-dimmed: var(--color-graphite-400);
  --ui-primary: var(--color-crimson-400);
  --ui-secondary: var(--color-sapphire-300);
  --ui-success: var(--color-sage-400);
  --ui-info: var(--color-sapphire-300);
  --ui-warning: var(--color-amber-400);
  --ui-error: var(--color-rust-400);
  --ui-neutral: var(--color-graphite-400);
}

/* ---- Base ------------------------------------------------------------------ */
@layer base {
  html { font-family: var(--font-sans); font-size: 15px; }
  body { line-height: 1.75; background: var(--color-ink-950); color: var(--color-paper); -webkit-font-smoothing: antialiased; overflow-x: hidden; }
  h1, h2, h3 { font-family: var(--font-display); line-height: 1.28; font-weight: 700; color: var(--color-paper); text-wrap: balance; }
  h2, h3 { font-weight: 600; }
  table, .tabular { font-variant-numeric: tabular-nums; }
  code, pre, kbd { font-family: var(--font-mono); font-size: 0.85em; }
  ::selection { background: rgb(214 62 62 / 0.28); color: #fff; }
  * { scrollbar-width: thin; scrollbar-color: var(--color-ink-700) transparent; }
  *::-webkit-scrollbar { width: 10px; height: 10px; }
  *::-webkit-scrollbar-thumb { background: var(--color-ink-700); border-radius: 8px; border: 3px solid transparent; background-clip: content-box; }
  *::-webkit-scrollbar-track { background: transparent; }
  :where(a, button, input, select, textarea, summary, [tabindex]):focus-visible { outline: 2px solid rgb(224 110 110 / 0.7); outline-offset: 2px; border-radius: 6px; }
}

/* ---- Shared component classes (spec §2.3, §3, §5) -------------------------- */
@layer components {
  /* editorial voices — Persian: weight and colour, never tracking or caps */
  .kicker { font-size: 0.8125rem; font-weight: 500; line-height: 1.4; color: var(--color-crimson-400); }
  .display { font-family: var(--font-display); font-weight: 700; }
  .data-label { font-size: 0.8rem; font-weight: 500; line-height: 1.4; color: var(--color-paper-mute); }

  /* glass */
  .glass {
    background: linear-gradient(180deg, rgb(255 255 255 / 0.035), rgb(255 255 255 / 0.012) 42%, rgb(255 255 255 / 0.02));
    border: 1px solid rgb(255 255 255 / 0.07);
    box-shadow: var(--shadow-card);
    backdrop-filter: blur(14px);
    border-radius: 18px;
  }
  .glass-inset { background: rgb(11 13 18 / 0.55); border: 1px solid rgb(255 255 255 / 0.05); border-radius: 12px; }
  .hairline { border-color: rgb(255 255 255 / 0.07); }
  .hover-glow { transition: box-shadow 0.4s var(--ease-swift), border-color 0.4s var(--ease-swift); }
  .hover-glow:hover { box-shadow: var(--shadow-glow-crimson); border-color: rgb(214 62 62 / 0.35); }

  /* live dot with pulse halo */
  .dot { position: relative; display: inline-block; width: 8px; height: 8px; border-radius: 9999px; flex: none; }
  .dot::after { content: ''; position: absolute; inset: -4px; border-radius: 9999px; background: currentColor; opacity: 0.18; animation: dot-pulse 2.6s ease-out infinite; }

  /* buttons: 36 / 30 / 44 px */
  .btn-primary, .btn-ghost, .btn-danger {
    display: inline-flex; align-items: center; justify-content: center; gap: 0.5rem;
    border-radius: 0.75rem; padding: 0.5rem 1rem; font-size: 0.875rem; line-height: 1.25rem; font-weight: 500;
    cursor: pointer; user-select: none; white-space: nowrap; transition: all 0.3s var(--ease-swift);
  }
  .btn-sm { padding: 0.375rem 0.75rem; font-size: 0.8125rem; line-height: 1.125rem; }
  .btn-lg { padding: 0.75rem 1.25rem; font-size: 0.9375rem; line-height: 1.25rem; }
  .btn-primary { color: #fff; background: linear-gradient(180deg, var(--color-crimson-600), var(--color-crimson-800)); box-shadow: 0 1px 0 rgb(255 255 255 / 0.25) inset, 0 8px 24px -8px rgb(214 62 62 / 0.5); }
  .btn-primary:hover { filter: brightness(1.1); }
  .btn-primary:active { transform: translateY(1px); }
  .btn-primary:disabled { opacity: 0.45; cursor: not-allowed; filter: none; }
  .btn-ghost { color: var(--color-paper-dim); border: 1px solid var(--color-line-bright); background: rgb(255 255 255 / 0.02); }
  .btn-ghost:hover { color: var(--color-paper); border-color: rgb(224 110 110 / 0.4); }
  .btn-ghost:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn-danger { color: var(--color-danger); border: 1px solid rgb(205 116 96 / 0.35); background: rgb(205 116 96 / 0.06); }
  .btn-danger:hover { background: rgb(205 116 96 / 0.14); }
  .btn-danger:disabled { opacity: 0.45; cursor: not-allowed; }

  /* plain fields (36 px) */
  .field {
    width: 100%; border-radius: 0.75rem; border: 1px solid var(--color-line-bright); background: rgb(16 19 26 / 0.8);
    padding: 0.4375rem 0.875rem; font-size: 0.875rem; line-height: 1.25rem; color: var(--color-paper);
    transition: border-color 0.3s var(--ease-swift), box-shadow 0.3s var(--ease-swift);
  }
  .field::placeholder { color: rgb(154 156 171 / 0.6); }
  .field:hover { border-color: rgb(255 255 255 / 0.28); }
  .field:focus { outline: none; border-color: rgb(224 110 110 / 0.55); box-shadow: 0 0 0 3px rgb(224 110 110 / 0.12); }
  .field:disabled { opacity: 0.5; }
  .field-invalid, .field-invalid:focus { border-color: var(--color-danger); }

  .code-block {
    background: rgb(11 13 18 / 0.55); border: 1px solid rgb(255 255 255 / 0.05); border-radius: 12px;
    font-family: var(--font-mono); font-size: 0.8rem; line-height: 1.6; color: var(--color-paper-dim);
    padding: 0.75rem 1rem; overflow-x: auto; white-space: pre; text-align: start;
  }

  /* sidebar */
  .nav-item { display: flex; align-items: center; gap: 0.6875rem; width: 100%; border-radius: 0.75rem; padding: 0.5625rem 0.8125rem; font-size: 0.85rem; line-height: 1.25rem; font-weight: 500; color: var(--color-paper-dim); transition: color 0.25s, background 0.25s; }
  .nav-item:hover { color: var(--color-paper); background: rgb(255 255 255 / 0.04); }
  .nav-active, .nav-active:hover { color: var(--color-crimson-300); background: linear-gradient(90deg, rgb(214 62 62 / 0.03), rgb(214 62 62 / 0.12)); box-shadow: inset -2px 0 0 var(--color-crimson-500); }
  .brand-tile { display: flex; width: 2.25rem; height: 2.25rem; flex: none; align-items: center; justify-content: center; border-radius: 0.75rem; color: #fff; background: linear-gradient(135deg, var(--color-sapphire-500), var(--color-sapphire-800)); box-shadow: 0 0 44px 0 rgb(79 85 240 / 0.18); }

  /* segmented control: 32 px track, 28 px buttons */
  .segmented { display: inline-flex; padding: 2px; gap: 2px; border-radius: 0.75rem; background: rgb(11 13 18 / 0.55); border: 1px solid rgb(255 255 255 / 0.05); }
  .segmented-btn { padding: 0.25rem 0.75rem; border-radius: 0.625rem; font-size: 0.8125rem; line-height: 1.25rem; font-weight: 500; color: var(--color-paper-mute); transition: background-color 150ms, color 150ms; }
  .segmented-btn:hover { color: var(--color-paper); }
  .segmented-btn[aria-checked="true"] { background: var(--color-ink-700); color: var(--color-paper); }

  /* mode tickets */
  .ticket { position: relative; display: block; cursor: pointer; padding: 1.5rem; padding-inline-end: 3.25rem; transition: border-color 150ms, box-shadow 150ms; }
  .ticket:hover { border-color: rgb(224 110 110 / 0.4); }
  .ticket[data-selected="true"] { border-color: var(--color-crimson-500); box-shadow: inset 0 0 0 1px var(--color-crimson-500), var(--shadow-card); }
  .ticket:has(input:focus-visible) { outline: 2px solid rgb(224 110 110 / 0.7); outline-offset: 2px; }
  .ticket-check { position: absolute; top: 1.5rem; inset-inline-end: 1.5rem; color: var(--color-crimson-400); }

  /* ledger tables */
  .ledger { overflow-x: auto; }
  .ledger table { width: 100%; font-size: 0.9rem; font-variant-numeric: tabular-nums; }
  .ledger thead th { background: var(--color-ink-800); color: var(--color-paper-mute); font-weight: 500; font-size: 0.8rem; text-align: start; padding: 0.625rem 0.875rem; white-space: nowrap; }
  .ledger tbody td { padding: 0.75rem 0.875rem; border-top: 1px solid rgb(255 255 255 / 0.06); vertical-align: top; text-align: start; position: relative; color: var(--color-paper-dim); }
  .ledger tbody tr:hover { background: rgb(23 27 36 / 0.55); }
  .ledger tbody tr[data-selected="true"] { background: var(--color-ink-700); }
  .ledger tbody tr[data-selected="true"] > td:first-child::before { content: ''; position: absolute; inset-block: 0; inset-inline-start: 0; width: 3px; background: var(--color-crimson-500); }
  .ledger-roomy > td { padding-block: 0.875rem; }

  /* bridge stepper */
  .stepper { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); }
  .stepper-step { position: relative; display: flex; flex-direction: column; align-items: center; gap: 0.375rem; text-align: center; padding-top: 0.75rem; }
  .stepper-step::before { content: ''; position: absolute; top: 0; inset-inline: 0; height: 3px; background: rgb(255 255 255 / 0.09); }
  .stepper-step:first-child::before { inset-inline-start: 50%; border-start-start-radius: 2px; border-end-start-radius: 2px; }
  .stepper-step:last-child::before { inset-inline-end: 50%; border-start-end-radius: 2px; border-end-end-radius: 2px; }
  .stepper-step[data-state="done"]::before { background: var(--color-sapphire-400); }
  .stepper-step[data-state="current"]::before, .stepper-step[data-state="failed"]::before { background: linear-gradient(to left, var(--color-sapphire-400) 50%, rgb(255 255 255 / 0.09) 50%); }
  .stepper-pier { width: 4px; height: 14px; border-radius: 2px; background: var(--color-line-bright); margin-top: -0.75rem; }
  .stepper-step[data-state="done"] .stepper-pier { background: var(--color-sapphire-400); }
  .stepper-step[data-state="current"] .stepper-pier { background: var(--color-crimson-400); animation: breathe 1.6s ease-in-out infinite; }
  .stepper-step[data-state="failed"] .stepper-pier { background: var(--color-danger); }
  .stepper-label { font-size: 0.8rem; line-height: 1.3; color: var(--color-paper-mute); }
  .stepper-step[data-state="current"] .stepper-label, .stepper-step[data-state="failed"] .stepper-label { color: var(--color-paper); font-weight: 600; }
  .stepper-step[data-state="done"] .stepper-label { color: var(--color-paper-dim); }

  /* similarity bar, level dots, checklist rings */
  .sim-track { display: inline-block; width: 64px; height: 6px; border-radius: 3px; background: var(--color-ink-700); overflow: hidden; vertical-align: middle; }
  .sim-fill { display: block; height: 100%; border-radius: 3px; background: var(--color-graphite-400); }
  .sim-fill[data-tone="fair"] { background: var(--color-warn); }
  .sim-fill[data-tone="good"] { background: var(--color-ok); }
  .level-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-graphite-400); flex: none; margin-top: 0.55em; }
  .level-dot[data-level="WARN"] { background: var(--color-warn); }
  .level-dot[data-level="ERROR"] { background: var(--color-danger); }
  .check { display: inline-flex; align-items: center; justify-content: center; width: 1.5rem; height: 1.5rem; border-radius: 50%; flex: none; font-size: 0.8rem; font-weight: 600; color: var(--color-paper-mute); border: 1.5px solid var(--color-line-bright); }
  .check.is-done { background: var(--color-ok); border-color: var(--color-ok); color: var(--color-ink-950); }

  /* bridge draw-in */
  .draw { stroke-dasharray: 1; stroke-dashoffset: 1; animation: draw 900ms ease-out forwards; }

  /* reveal (armed by the v-reveal directive; nothing hides without it) */
  .reveal-armed [data-reveal] { opacity: 0; transform: translateY(22px); transition: opacity 0.8s var(--ease-swift) var(--reveal-delay, 0ms), transform 0.8s var(--ease-swift) var(--reveal-delay, 0ms); }
  .reveal-armed.is-revealed [data-reveal] { opacity: 1; transform: none; }
}

/* page transition (Nuxt pageTransition) */
.page-enter-active, .page-leave-active { transition: opacity 0.28s var(--ease-swift), transform 0.28s var(--ease-swift); }
.page-enter-from { opacity: 0; transform: translateY(10px); }
.page-leave-to { opacity: 0; transform: translateY(-6px); }

@keyframes breathe { 50% { opacity: 0.55; } }
@keyframes draw { to { stroke-dashoffset: 0; } }
@keyframes dot-pulse { 0% { transform: scale(0.6); opacity: 0.3; } 70% { transform: scale(1.5); opacity: 0; } 100% { transform: scale(1.5); opacity: 0; } }

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.001ms !important; animation-iteration-count: 1 !important; transition-duration: 0.001ms !important; scroll-behavior: auto !important; }
  .draw { stroke-dashoffset: 0; }
  .reveal-armed [data-reveal] { opacity: 1; transform: none; }
}

/* reduced effects (html.fx-lite): solid panels, no blur/glow/grain/animation; spinners stay */
html.fx-lite *, html.fx-lite *::before, html.fx-lite *::after { animation-duration: 0.001ms !important; animation-iteration-count: 1 !important; transition-duration: 0.001ms !important; backdrop-filter: none !important; }
html.fx-lite .animate-spin { animation-duration: 1s !important; animation-iteration-count: infinite !important; }
html.fx-lite .glass { background: var(--color-ink-800); box-shadow: none; }
html.fx-lite .glass-inset { background: var(--color-ink-950); }
html.fx-lite .dot::after { display: none; }
html.fx-lite .hover-glow:hover { box-shadow: none; border-color: rgb(255 255 255 / 0.16); }
html.fx-lite .btn-primary, html.fx-lite .brand-tile { box-shadow: none; }
html.fx-lite .draw { stroke-dashoffset: 0; }
html.fx-lite .reveal-armed [data-reveal] { opacity: 1; transform: none; }
```

- [ ] **Step 5: Rewrite `app/app.config.ts`**

```ts
export default defineAppConfig({
  ui: {
    colors: {
      primary: 'crimson', secondary: 'sapphire', success: 'sage', info: 'sapphire',
      warning: 'amber', error: 'rust', neutral: 'graphite',
    },
    // Buttons: solid = hot crimson gradient; outline = ghost; soft error = quiet danger; ghost = text.
    button: {
      slots: { base: 'font-medium rounded-xl whitespace-nowrap transition-all duration-300 [transition-timing-function:var(--ease-swift)] active:translate-y-px' },
      variants: {
        size: {
          sm: { base: 'px-3 py-[6px] text-[0.8125rem]/[18px] gap-1.5', leadingIcon: 'size-4', trailingIcon: 'size-4' },
          md: { base: 'px-4 py-2 text-sm/5 gap-2', leadingIcon: 'size-4', trailingIcon: 'size-4' },
          lg: { base: 'px-5 py-3 text-[0.9375rem]/5 gap-2', leadingIcon: 'size-5', trailingIcon: 'size-5' },
        },
      },
      compoundVariants: [
        { color: 'primary', variant: 'solid', class: 'text-white bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] shadow-[0_1px_0_rgb(255_255_255/0.25)_inset,0_8px_24px_-8px_rgb(214_62_62/0.5)] hover:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] hover:brightness-110 active:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] disabled:opacity-45 disabled:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)]' },
        { color: 'primary', variant: 'outline', class: 'ring-0 border border-line-bright bg-white/2 text-paper-dim hover:bg-white/2 hover:text-paper hover:border-[rgb(224_110_110/0.4)]' },
        { color: 'neutral', variant: 'outline', class: 'ring-0 border border-line-bright bg-white/2 text-paper-dim hover:bg-white/2 hover:text-paper hover:border-[rgb(224_110_110/0.4)]' },
        { color: 'primary', variant: 'soft', class: 'text-crimson-300 bg-[rgb(214_62_62/0.12)] hover:bg-[rgb(214_62_62/0.18)]' },
        { color: 'error', variant: 'soft', class: 'text-danger border border-[rgb(205_116_96/0.35)] bg-[rgb(205_116_96/0.06)] hover:bg-[rgb(205_116_96/0.14)]' },
        { color: 'primary', variant: 'ghost', class: 'text-crimson-400 hover:text-crimson-300 hover:bg-white/4' },
        { color: 'neutral', variant: 'ghost', class: 'text-paper-dim hover:text-paper hover:bg-white/4' },
      ],
    },
    // Fields: the `.field` look, variant "none" so @nuxt/ui's own ring never competes.
    input: {
      slots: { base: 'rounded-xl border border-line-bright bg-ink-900/80 text-paper placeholder:text-paper-mute/60 hover:border-white/28 focus:border-[rgb(224_110_110/0.55)] focus:ring-[3px] focus:ring-[rgb(224_110_110/0.12)] focus-visible:outline-none', leadingIcon: 'text-paper-mute', trailingIcon: 'text-paper-mute' },
      variants: { size: { md: { base: 'px-3.5 py-[7px] text-sm/5' } } },
      defaultVariants: { variant: 'none' },
    },
    // If the type check rejects `variant: 'none'` for the select, use `'ghost'` and add `ring-0` to `base`.
    select: {
      slots: {
        base: 'rounded-xl border border-line-bright bg-ink-900/80 text-paper hover:border-white/28 focus:border-[rgb(224_110_110/0.55)] focus-visible:outline-none data-[state=open]:border-[rgb(224_110_110/0.55)]',
        placeholder: 'text-paper-mute/60',
        content: 'glass ring-0 shadow-none rounded-[18px] p-1',
        item: 'rounded-lg text-paper-dim data-highlighted:not-data-disabled:text-paper data-highlighted:not-data-disabled:before:bg-white/6',
        viewport: 'divide-y-0',
      },
      variants: { size: { md: { base: 'px-3.5 py-[7px] text-sm/5' } } },
      defaultVariants: { variant: 'none' },
    },
    formField: {
      slots: { label: 'text-[0.8125rem] font-medium text-paper', hint: 'text-xs text-paper-mute', help: 'text-xs text-paper-mute', description: 'text-xs text-paper-mute', error: 'text-xs text-danger' },
    },
    switch: {
      slots: {
        base: 'data-[state=unchecked]:bg-ink-800 border border-line-bright transition-colors duration-300',
        thumb: 'bg-paper-mute shadow-none data-[state=checked]:bg-crimson-300',
      },
      variants: { color: { primary: { base: 'data-[state=checked]:bg-[rgb(214_62_62/0.3)] data-[state=checked]:border-[rgb(214_62_62/0.6)]' } } },
    },
    modal: {
      slots: {
        overlay: 'bg-ink-950/70 backdrop-blur-sm',
        content: 'glass divide-y divide-white/6 ring-0 shadow-none rounded-[18px]',
        header: 'px-6 py-4', body: 'p-6', footer: 'px-6 py-4',
        title: 'font-display text-lg font-semibold text-paper', description: 'text-xs text-paper-mute',
      },
    },
    progress: { slots: { base: 'bg-ink-700', indicator: 'bg-[linear-gradient(90deg,#d63e3e,#a30b0b)]' } },
    toast: {
      slots: {
        root: "glass ring-0 shadow-none p-4 gap-2.5 items-start before:content-[''] before:mt-1.5 before:size-2 before:shrink-0 before:rounded-full",
        title: 'text-[0.8125rem] font-medium text-paper-dim', description: 'text-xs text-paper-mute',
      },
      variants: {
        color: {
          primary: { root: 'before:bg-crimson-400' }, secondary: { root: 'before:bg-info' }, success: { root: 'before:bg-ok' },
          info: { root: 'before:bg-info' }, warning: { root: 'before:bg-warn' }, error: { root: 'before:bg-danger' }, neutral: { root: 'before:bg-paper-mute' },
        },
      },
    },
  },
});
```

- [ ] **Step 6: Update `nuxt.config.ts`**

Replace the whole file:

```ts
import { version } from './package.json';

export default defineNuxtConfig({
  ssr: false,
  devtools: { enabled: false },
  modules: ['@nuxt/ui'],
  css: ['~/assets/css/main.css'],
  plugins: process.env.NODE_ENV === 'development' ? ['~/dev/devQuery.client'] : [],
  // Dark only: no colour-mode module, `class="dark"` is fixed on <html> (spec §1).
  ui: { colorMode: false },
  // A desktop app under a strict CSP: every icon the pages and Nuxt UI use is bundled at build
  // time from the local lucide collection; nothing is fetched from api.iconify.design at runtime.
  icon: {
    provider: 'none',
    clientBundle: { scan: true, sizeLimitKb: 512 },
  },
  runtimeConfig: { public: { version } },
  app: {
    pageTransition: { name: 'page', mode: 'out-in' },
    head: {
      htmlAttrs: { dir: 'rtl', lang: 'fa', class: 'dark' },
      title: 'پل لاندا',
      meta: [{ name: 'color-scheme', content: 'dark' }],
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

Then make `app/dev/devQuery.client.ts` compile without the colour-mode module (it is rewritten in Task 3; for now it must not reference `$colorMode`):

```ts
// Dev-only query switches for screenshots (rewritten in Task 3). Inert inside the Tauri shell.
export default defineNuxtPlugin(() => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
});
```

- [ ] **Step 7: Window colour**

In `src-tauri/tauri.conf.json` change `"backgroundColor": "#F2F5F7"` to `"backgroundColor": "#0b0d12"`.

- [ ] **Step 8: Run the tests and the type check**

Run: `pnpm test && pnpm typecheck`
Expected: `palette.spec.ts` PASS (6 tests); `color.spec.ts`, `fa.spec.ts`, `sequence.spec.ts`, `similarity.spec.ts`, `phases.spec.ts` still PASS. Type check may report `ThemeToggle.vue` using `useColorMode` — that component is deleted in Task 5; if the type check fails only there, delete `app/components/ThemeToggle.vue` now and remove `<ThemeToggle />` from `app/layouts/default.vue` (Task 5 rewrites the layout anyway).

- [ ] **Step 9: Smoke the dev server and the bundle**

```bash
pnpm dev & sleep 8
curl -s http://localhost:3210/ | grep -o 'class="dark"' | head -1
kill %1
pnpm generate >/dev/null && ls .output/public/fonts && ! grep -rl 'fonts.googleapis\|Vazirmatn' .output/public --include='*.css' --include='*.html' | grep .
```
Expected: `class="dark"` printed; the two woff2 files listed; the last command prints nothing (exit 0 from the negation).

- [ ] **Step 10: Commit**

```bash
git add -A app/assets/css/main.css app/app.config.ts nuxt.config.ts src-tauri/tauri.conf.json public/fonts app/utils/palette.spec.ts app/dev/devQuery.client.ts
git commit -m "Define the night-glass tokens, fonts and Nuxt UI skin

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 2: Persian helpers for jobs, durations, deltas and error families

**Files:**
- Modify: `app/utils/fa.ts`
- Modify: `app/utils/fa.spec.ts`

**Interfaces:**
- Produces (all exported from `app/utils/fa.ts`, auto-imported in components):
  - `faPercent(n: number, digits = 1): string` → «۶٫۷٪»
  - `faDuration(startIso: string, endIso?: string | null, now?: number): string`
  - `faVersion(v: string): string` → «۰.۱.۰»
  - `jobKindLabel(job: Pick<JobRow, 'kind' | 'mode'>): string`
  - `type Tone = 'ok' | 'warn' | 'danger' | 'info' | 'muted' | 'brand'`; `statusTone(status: string | null | undefined): Tone`
  - `interface SummaryTile { label: string; value: number }`; `summaryTiles(job: Pick<JobRow, 'kind' | 'summary'>): SummaryTile[]`; `summaryLabel(job: Pick<JobRow, 'kind' | 'summary'>): string`
  - `reportEventText(e: { phase: string; message: string }, kind: 'IMPORT' | 'REFRESH'): string`
  - `type KindFamily = 'supplier' | 'landau' | 'app'`; `kindFamily(kind: string): KindFamily`
  - `isoToJalali(iso: string): string` → `1405/06/23`; `nextSinceDate(lastImport: Pick<JobRow, 'kind' | 'mode' | 'status' | 'finished_at'> | null): string`
  - `interface PriceDelta { pct: number; dir: 'up' | 'down' | 'flat' }`; `priceDelta(item: Pick<JobItemRow, 'price_before' | 'price_after'>): PriceDelta | null`; `priceDeltaLabel(d: PriceDelta | null): string`

- [ ] **Step 1: Write the failing tests**

Append to `app/utils/fa.spec.ts` (extend the import line to include the new names):

```ts
import {
  faNumber, rials, latinDigits, normalizeJalali, todayJalali, jalaliYearStart, verdictLabel, kindLabel,
  faPercent, faDuration, faVersion, jobKindLabel, statusTone, summaryTiles, summaryLabel, reportEventText,
  kindFamily, isoToJalali, nextSinceDate, priceDelta, priceDeltaLabel,
} from './fa';

describe('fa — jobs and durations', () => {
  const T0 = '2026-09-14T10:20:00.000Z';

  it('formats durations in Persian', () => {
    expect(faDuration(T0, '2026-09-14T10:20:30.000Z')).toBe('کمتر از یک دقیقه');
    expect(faDuration(T0, '2026-09-14T10:41:00.000Z')).toBe('۲۱ دقیقه');
    expect(faDuration(T0, '2026-09-14T11:32:00.000Z')).toBe('۱ ساعت و ۱۲ دقیقه');
    expect(faDuration(T0, '2026-09-14T12:20:00.000Z')).toBe('۲ ساعت');
    expect(faDuration(T0, null, new Date('2026-09-14T10:25:00.000Z').getTime())).toBe('۵ دقیقه');
  });

  it('formats versions and percentages', () => {
    expect(faVersion('0.1.0')).toBe('۰.۱.۰');
    expect(faPercent(6.66)).toBe('۶٫۷٪');
    expect(faPercent(12)).toBe('۱۲٪');
  });

  it('labels job kinds and status tones', () => {
    expect(jobKindLabel({ kind: 'IMPORT', mode: 'QUANTITY' })).toBe('وارد کردن با تعداد');
    expect(jobKindLabel({ kind: 'IMPORT', mode: 'ZERO' })).toBe('وارد کردن با تعداد صفر');
    expect(jobKindLabel({ kind: 'REFRESH', mode: null })).toBe('به‌روزرسانی قیمت‌ها');
    expect(statusTone('DONE')).toBe('ok');
    expect(statusTone('FAILED')).toBe('danger');
    expect(statusTone('REVIEW')).toBe('warn');
    expect(statusTone('RUNNING')).toBe('info');
    expect(statusTone('CANCELLED')).toBe('muted');
    expect(statusTone(null)).toBe('muted');
  });

  it('turns a job summary into tiles and a one-line label', () => {
    const imp = { kind: 'IMPORT' as const, summary: { items: 89, done: 81, attached: 34, patched: 47, skipped: 0, failed: 3, invoice_ids: [1] } };
    expect(summaryTiles(imp).map((t) => t.label)).toEqual(['اقلام', 'انجام‌شده', 'افزوده', 'به‌روز', 'ردشده', 'ناموفق']);
    expect(summaryLabel(imp)).toBe('۸۹ اقلام · ۸۱ انجام‌شده · ۳۴ افزوده · ۴۷ به‌روز · ۳ ناموفق');
    const ref = { kind: 'REFRESH' as const, summary: { rows: 412, changed: 96, skipped_manual: 12 } };
    expect(summaryTiles(ref)).toEqual([{ label: 'ردیف‌ها', value: 412 }, { label: 'تغییر قیمت', value: 96 }, { label: 'قیمت دستی حفظ‌شده', value: 12 }]);
    expect(summaryTiles({ kind: 'IMPORT', summary: null })).toEqual([]);
    expect(summaryLabel({ kind: 'IMPORT', summary: null })).toBe('');
    expect(summaryTiles({ kind: 'IMPORT', summary: { items: 'x' } })).toEqual([]);
  });

  it('renders the REPORT event in Persian and leaves other events alone', () => {
    const json = JSON.stringify({ rows: 412, changed: 96, skipped_manual: 12 });
    expect(reportEventText({ phase: 'REPORT', message: json }, 'REFRESH')).toBe('گزارش نهایی: ۴۱۲ ردیف‌ها · ۹۶ تغییر قیمت · ۱۲ قیمت دستی حفظ‌شده');
    expect(reportEventText({ phase: 'REPORT', message: 'not json' }, 'REFRESH')).toBe('not json');
    expect(reportEventText({ phase: 'FETCH', message: json }, 'IMPORT')).toBe(json);
  });

  it('groups error kinds into families', () => {
    for (const k of ['SupplierUnreachable', 'SupplierParse', 'NoCurrentPrice', 'InvoiceUnpaid', 'LineNonPositiveQty', 'VatRateMismatch']) expect(kindFamily(k), k).toBe('supplier');
    for (const k of ['LandauUnauthorized', 'LandauRejected', 'BusinessNotApproved', 'NoStore', 'ProductNotApproved', 'DuplicateRow', 'MirrorTimeout', 'NoPricingRule', 'RepriceSkipped']) expect(kindFamily(k), k).toBe('landau');
    for (const k of ['JobCancelled', 'SettingsIncomplete', 'KeyringUnavailable', 'Db', 'Other', 'Unmatched', 'NoneDecision', 'Whatever']) expect(kindFamily(k), k).toBe('app');
  });

  it('suggests the next import start date from the last finished quantity import', () => {
    expect(isoToJalali('2026-09-14T06:41:00.000Z')).toBe('1405/06/23');
    const done = { kind: 'IMPORT' as const, mode: 'QUANTITY' as const, status: 'DONE', finished_at: '2026-09-14T06:41:00.000Z' };
    expect(nextSinceDate(done)).toBe('1405/06/23');
    expect(nextSinceDate({ ...done, mode: 'ZERO' })).toBe(jalaliYearStart());
    expect(nextSinceDate({ ...done, status: 'FAILED' })).toBe(jalaliYearStart());
    expect(nextSinceDate({ ...done, finished_at: null })).toBe(jalaliYearStart());
    expect(nextSinceDate(null)).toBe(jalaliYearStart());
  });

  it('computes and labels price deltas', () => {
    expect(priceDelta({ price_before: 450_000, price_after: 480_000 })).toEqual({ pct: expect.closeTo(6.667, 2), dir: 'up' });
    expect(priceDelta({ price_before: 2_000_000, price_after: 1_900_000 })).toEqual({ pct: 5, dir: 'down' });
    expect(priceDelta({ price_before: 100, price_after: 100 })).toEqual({ pct: 0, dir: 'flat' });
    expect(priceDelta({ price_before: null, price_after: 100 })).toBeNull();
    expect(priceDelta({ price_before: 0, price_after: 100 })).toBeNull();
    expect(priceDeltaLabel(priceDelta({ price_before: 450_000, price_after: 480_000 }))).toBe('▲ ۶٫۷٪');
    expect(priceDeltaLabel(priceDelta({ price_before: 2_000_000, price_after: 1_900_000 }))).toBe('▼ ۵٪');
    expect(priceDeltaLabel(priceDelta({ price_before: 100, price_after: 100 }))).toBe('—');
    expect(priceDeltaLabel(null)).toBe('—');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test -- app/utils/fa.spec.ts`
Expected: FAIL — the new names are not exported.

- [ ] **Step 3: Implement in `app/utils/fa.ts`**

Change the first import to `import { PersianCalendar, fromDate, toCalendar, today } from '@internationalized/date';`, add `import type { JobItemRow, JobRow } from '~/types/bridge';` and append:

```ts
// ---- job presentation (spec §6) -----------------------------------------------------------------

export const faPercent = (n: number, digits = 1): string =>
  `${new Intl.NumberFormat('fa-IR', { maximumFractionDigits: digits }).format(n)}٪`;

/** «۲۱ دقیقه», «۱ ساعت و ۱۲ دقیقه»; an unfinished job measures against `now`. */
export const faDuration = (startIso: string, endIso?: string | null, now: number = Date.now()): string => {
  const start = new Date(startIso).getTime();
  const end = endIso ? new Date(endIso).getTime() : now;
  const secs = Math.max(0, Math.round((end - start) / 1000));
  if (secs < 60) return 'کمتر از یک دقیقه';
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${faNumber(mins)} دقیقه`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `${faNumber(h)} ساعت` : `${faNumber(h)} ساعت و ${faNumber(m)} دقیقه`;
};

export const faVersion = (v: string): string => v.replace(/\d/g, (d) => FA_DIGITS[Number(d)]!);

export const jobKindLabel = (job: Pick<JobRow, 'kind' | 'mode'>): string =>
  job.kind === 'REFRESH' ? 'به‌روزرسانی قیمت‌ها' : job.mode === 'ZERO' ? 'وارد کردن با تعداد صفر' : 'وارد کردن با تعداد';

export type Tone = 'ok' | 'warn' | 'danger' | 'info' | 'muted' | 'brand';
export const statusTone = (status: string | null | undefined): Tone =>
  status === 'DONE' ? 'ok' : status === 'FAILED' ? 'danger' : status === 'REVIEW' ? 'warn' : status === 'RUNNING' ? 'info' : 'muted';

export interface SummaryTile { label: string; value: number }
const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const IMPORT_TILES: readonly [string, string][] = [['items', 'اقلام'], ['done', 'انجام‌شده'], ['attached', 'افزوده'], ['patched', 'به‌روز'], ['skipped', 'ردشده'], ['failed', 'ناموفق']];
const REFRESH_TILES: readonly [string, string][] = [['rows', 'ردیف‌ها'], ['changed', 'تغییر قیمت'], ['skipped_manual', 'قیمت دستی حفظ‌شده']];

/** The numeric parts of a job summary, in display order; keys that are missing or not numbers are skipped. */
export const summaryTiles = (job: Pick<JobRow, 'kind' | 'summary'>): SummaryTile[] => {
  const s = job.summary;
  if (!s) return [];
  return (job.kind === 'IMPORT' ? IMPORT_TILES : REFRESH_TILES).flatMap(([key, label]) => {
    const value = num(s[key]);
    return value == null ? [] : [{ label, value }];
  });
};

/** «۸۹ اقلام · ۳۴ افزوده · …» — zero parts are dropped except the first; '' without a summary. */
export const summaryLabel = (job: Pick<JobRow, 'kind' | 'summary'>): string =>
  summaryTiles(job).filter((t, i) => i === 0 || t.value !== 0).map((t) => `${faNumber(t.value)} ${t.label}`).join(' · ');

/** The core logs the REPORT event as raw JSON; show it as the same Persian line the tiles use. */
export const reportEventText = (e: { phase: string; message: string }, kind: 'IMPORT' | 'REFRESH'): string => {
  if (e.phase !== 'REPORT') return e.message;
  try {
    const summary = JSON.parse(e.message) as unknown;
    if (summary && typeof summary === 'object') {
      const line = summaryLabel({ kind, summary: summary as Record<string, unknown> });
      if (line) return `گزارش نهایی: ${line}`;
    }
  } catch { /* not JSON: an ordinary message */ }
  return e.message;
};

export type KindFamily = 'supplier' | 'landau' | 'app';
/** Which side an error kind belongs to — the split that tells the seller whom to call. */
export const kindFamily = (kind: string): KindFamily => {
  if (/^Supplier|^Invoice|^(NoCurrentPrice|LineNonPositiveQty|VatRateMismatch)$/.test(kind)) return 'supplier';
  if (/^Landau|^(BusinessNotApproved|NoStore|ProductNotApproved|DuplicateRow|MirrorTimeout|NoPricingRule|RepriceSkipped)$/.test(kind)) return 'landau';
  return 'app';
};

/** ISO instant → Jalali `YYYY/MM/DD` in Tehran time. */
export const isoToJalali = (iso: string): string => {
  const d = toCalendar(fromDate(new Date(iso), 'Asia/Tehran'), new PersianCalendar());
  return `${d.year}/${String(d.month).padStart(2, '0')}/${String(d.day).padStart(2, '0')}`;
};

/** The suggested «از تاریخ» for the next quantity import: the day the last one finished. */
export const nextSinceDate = (lastImport: Pick<JobRow, 'kind' | 'mode' | 'status' | 'finished_at'> | null): string =>
  lastImport && lastImport.kind === 'IMPORT' && lastImport.mode === 'QUANTITY' && lastImport.status === 'DONE' && lastImport.finished_at
    ? isoToJalali(lastImport.finished_at)
    : jalaliYearStart();

export interface PriceDelta { pct: number; dir: 'up' | 'down' | 'flat' }
export const priceDelta = (item: Pick<JobItemRow, 'price_before' | 'price_after'>): PriceDelta | null => {
  const { price_before: before, price_after: after } = item;
  if (before == null || after == null || before <= 0) return null;
  const pct = ((after - before) / before) * 100;
  return { pct: Math.abs(pct), dir: pct > 0 ? 'up' : pct < 0 ? 'down' : 'flat' };
};
export const priceDeltaLabel = (d: PriceDelta | null): string =>
  !d || d.dir === 'flat' ? '—' : `${d.dir === 'up' ? '▲' : '▼'} ${faPercent(d.pct)}`;
```

`today` stays imported because `jalaliToday` uses it. `FA_DIGITS` already exists at the top of the file.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test -- app/utils/fa.spec.ts && pnpm typecheck`
Expected: PASS (all `fa` tests), type check clean.

- [ ] **Step 5: Commit**

```bash
git add app/utils/fa.ts app/utils/fa.spec.ts
git commit -m "Add Persian helpers for job kinds, durations, summaries, deltas and error families

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 3: Reduced effects, reveal directive, dev overlay, fixtures, screenshot script

**Files:**
- Create: `app/utils/effects.ts`, `app/utils/effects.spec.ts`
- Create: `app/composables/useEffects.ts`, `app/composables/useSettingsSaved.ts`
- Create: `app/plugins/effects.client.ts`, `app/plugins/reveal.client.ts`
- Modify: `app/dev/devQuery.client.ts` (rewrite), `app/dev/mockBridge.ts`, `scripts/shots.sh`, `docs/RUNBOOK.md:31-32`

**Interfaces:**
- Produces: `isWeakRenderer(renderer: string, userAgent: string): boolean` (pure); `useReducedEffects(): Ref<boolean>`, `setReducedEffects(reduced: boolean): void`, `initReducedEffects(force?: boolean): void`, `reducedEffectsActive(): boolean`, `prefersReducedMotion(): boolean`; `useSettingsSaved(): Ref<number>`; the `v-reveal` directive; `?grid=1` and `?fx=lite` dev switches; `scripts/shots.sh OUT_DIR [filter] [--grid]`.

- [ ] **Step 1: Write the failing renderer test**

`app/utils/effects.spec.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { isWeakRenderer } from './effects';

describe('isWeakRenderer', () => {
  const desktopUa = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15';
  it('flags software rasterizers and Pi-class GPUs', () => {
    expect(isWeakRenderer('Mesa llvmpipe (LLVM 15.0.7, 256 bits)', desktopUa)).toBe(true);
    expect(isWeakRenderer('Google SwiftShader', desktopUa)).toBe(true);
    expect(isWeakRenderer('V3D 4.2', desktopUa)).toBe(true);
    expect(isWeakRenderer('Broadcom VideoCore VI', desktopUa)).toBe(true);
  });
  it('flags ARM boards by user agent', () => {
    expect(isWeakRenderer('Mesa Intel(R) UHD Graphics', 'Mozilla/5.0 (X11; Linux aarch64)')).toBe(true);
  });
  it('keeps a 4-core desktop with a real GPU on full effects', () => {
    expect(isWeakRenderer('Mesa Intel(R) UHD Graphics 620 (KBL GT2)', desktopUa)).toBe(false);
    expect(isWeakRenderer('ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Direct3D11 vs_5_0 ps_5_0)', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe(false);
    expect(isWeakRenderer('', desktopUa)).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test -- app/utils/effects.spec.ts` — Expected: FAIL (module not found).

- [ ] **Step 3: Implement the pure check, the composable and the plugins**

`app/utils/effects.ts`:

```ts
/** Is this GPU/renderer pair likely to struggle with blur and glow compositing? (spec §2.4) */
export const isWeakRenderer = (renderer: string, userAgent: string): boolean =>
  /llvmpipe|softpipe|swiftshader|software/i.test(renderer) ||
  /videocore|v3d|broadcom/i.test(renderer) ||
  /aarch64|armv7|raspberry/i.test(userAgent);
```

`app/composables/useEffects.ts`:

```ts
/**
 * Reduced-effects ("fx-lite") mode: a `fx-lite` class on <html> swaps translucent blurred panels
 * for solid ones and stops decorative animation (see main.css). Chosen per device — stored in
 * localStorage, auto-detected on first run. Ported from Ledger POS.
 */
const STORAGE_KEY = 'landau-bridge:reduced-effects';

function detectWeakGpu(): boolean {
  try {
    const canvas = document.createElement('canvas');
    const gl = (canvas.getContext('webgl') ?? canvas.getContext('experimental-webgl')) as WebGLRenderingContext | null;
    if (!gl) return true; // no GL at all: rendering already falls back to software
    const dbg = gl.getExtension('WEBGL_debug_renderer_info');
    const renderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : '';
    return isWeakRenderer(renderer, navigator.userAgent);
  } catch {
    return true;
  }
}

export const useReducedEffects = () => useState<boolean>('fx-reduced', () => false);

const applyClass = (reduced: boolean) => document.documentElement.classList.toggle('fx-lite', reduced);

/** Called once from the client plugin: `force` (dev screenshots) > stored choice > auto-detect. */
export function initReducedEffects(force?: boolean) {
  const stored = localStorage.getItem(STORAGE_KEY);
  const reduced = force ?? (stored !== null ? stored === '1' : detectWeakGpu());
  useReducedEffects().value = reduced;
  applyClass(reduced);
}

/** Explicit user choice (Settings → نمایش): apply now and remember on this device. */
export function setReducedEffects(reduced: boolean) {
  useReducedEffects().value = reduced;
  localStorage.setItem(STORAGE_KEY, reduced ? '1' : '0');
  applyClass(reduced);
}

export function reducedEffectsActive(): boolean {
  return typeof document !== 'undefined' && document.documentElement.classList.contains('fx-lite');
}

/** True when the OS asks for reduced motion or fx-lite is on. */
export function prefersReducedMotion(): boolean {
  return typeof window === 'undefined' || window.matchMedia('(prefers-reduced-motion: reduce)').matches || reducedEffectsActive();
}
```

`app/composables/useSettingsSaved.ts`:

```ts
/** Bumped by the settings page after a save so the sidebar's connection line refreshes. */
export const useSettingsSaved = () => useState<number>('settings-saved', () => 0);
```

`app/plugins/effects.client.ts`:

```ts
// Applies the reduced-effects mode before first paint. `?fx=lite` (dev, browser only) forces it
// for one page load without remembering it.
export default defineNuxtPlugin(() => {
  const forced = import.meta.dev && !('__TAURI_INTERNALS__' in window) && new URLSearchParams(location.search).get('fx') === 'lite';
  initReducedEffects(forced ? true : undefined);
});
```

`app/plugins/reveal.client.ts`:

```ts
/**
 * `v-reveal` on a page's root: children marked `data-reveal` rise into place, staggered 70 ms.
 * CSS-only (main.css `.reveal-armed`); under reduced motion or fx-lite everything shows at once.
 */
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.directive('reveal', {
    beforeMount(el: HTMLElement) {
      el.classList.add('reveal-armed');
    },
    mounted(el: HTMLElement) {
      const targets = el.querySelectorAll<HTMLElement>('[data-reveal]');
      if (prefersReducedMotion() || !targets.length) {
        el.classList.add('is-revealed');
        return;
      }
      targets.forEach((t, i) => t.style.setProperty('--reveal-delay', `${i * 70}ms`));
      // two frames: let the hidden state paint once so the transition has a start value
      requestAnimationFrame(() => requestAnimationFrame(() => el.classList.add('is-revealed')));
    },
  });
});
```

- [ ] **Step 4: Rewrite the dev query plugin**

`app/dev/devQuery.client.ts`:

```ts
// Dev-only switches for screenshots, inert inside the Tauri shell and absent from the build
// (registered from nuxt.config.ts only in development):
//   ?grid=1  — draws the 8/40 px grid and the 1060 px content column (plus its 40 px gutters)
//   ?fx=lite — handled by plugins/effects.client.ts
export default defineNuxtPlugin(() => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
  if (new URLSearchParams(location.search).get('grid') !== '1') return;
  const overlay = document.createElement('div');
  overlay.setAttribute('aria-hidden', 'true');
  overlay.style.cssText = [
    'position:fixed', 'inset:0', 'z-index:60', 'pointer-events:none',
    'background-image:linear-gradient(to right,rgb(255 255 255/.04) 1px,transparent 1px),linear-gradient(to bottom,rgb(255 255 255/.04) 1px,transparent 1px),linear-gradient(to right,rgb(255 255 255/.09) 1px,transparent 1px),linear-gradient(to bottom,rgb(255 255 255/.09) 1px,transparent 1px)',
    'background-size:8px 8px,8px 8px,40px 40px,40px 40px',
  ].join(';');
  // the main area is everything left of the 240 px sidebar (RTL); the column is centred in it
  const column = document.createElement('div');
  column.style.cssText = 'position:absolute;top:0;bottom:0;left:50%;transform:translateX(-50%);width:min(1060px,100%);border-inline:1px solid rgb(214 62 62/.45);box-sizing:border-box;';
  const gutter = document.createElement('div');
  gutter.style.cssText = 'position:absolute;top:0;bottom:0;left:40px;right:40px;border-inline:1px dashed rgb(214 62 62/.3);';
  const main = document.createElement('div');
  main.style.cssText = 'position:absolute;top:0;bottom:0;left:0;right:240px;';
  column.append(gutter);
  main.append(column);
  overlay.append(main);
  document.body.append(overlay);
});
```

- [ ] **Step 5: Extend the fixtures**

In `app/dev/mockBridge.ts`:
- `case 'jobs_list':` becomes `return state === 'empty' ? [] : [...(current(state) ? [current(state)!] : []), ...jobs];` so `/jobs?state=running` lists the live job first.
- In `detail()`, keep the existing branches; add before the final `const j = …` line: `if (id === 40) return { job: jobs[2]!, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 4400), ev(2, 'ERROR', 'FETCH', 'پاسخی از بارزپخش نیامد', 4390)] };` (job 40 is the FAILED import) — `ev` accepts `'ERROR'` already through `JobEventRow['level']`.
- Give the refresh fixture job 41 a `detail` with its own rows: in `detail()`, before the final fallback add `if (id === 41) return { job: jobs[1]!, items: doneItems.filter((i) => i.action === 'PATCH'), events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 1500), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 1490), ev(3, 'INFO', 'REPORT', JSON.stringify({ rows: 412, changed: 96, skipped_manual: 12 }), 1480)] };`
- Give error 58 a context that exercises the expander (already has `{ url }`); leave the rest.

- [ ] **Step 6: Rewrite `scripts/shots.sh`**

```sh
#!/usr/bin/env sh
# Screenshots every page (dark only) through the dev fixture mode (app/dev/mockBridge.ts).
# Run `pnpm dev` first. Usage: scripts/shots.sh OUT_DIR [page-filter] [--grid]
set -eu
out=${1:?output directory}
filter=${2:-}
grid=""
for a in "$@"; do [ "$a" = "--grid" ] && grid="&grid=1"; done
[ "$filter" = "--grid" ] && filter=""
mkdir -p "$out"
pages="/?state=done /?state=empty /?state=running /?state=review /?state=done&appimage=1 /import?state=running /import?state=review /import?state=done /refresh?state=done /refresh?state=refreshing /matches?state=done /matches?state=empty /jobs?state=done /jobs/42?state=done /jobs/40?state=done /jobs/43?state=running /errors?state=done /errors?state=done&job=42 /settings?state=done /settings?state=empty /?state=done&fx=lite"
for p in $pages; do
  case "$p" in *"$filter"*) ;; *) continue ;; esac
  name=$(printf '%s' "$p" | tr '/?=&' '____' | sed 's/^_//; s/^$/root/')
  [ -n "$grid" ] && name="$name-grid"
  google-chrome-stable --headless=new --no-proxy-server --disable-gpu --hide-scrollbars --no-sandbox \
    --window-size=1200,800 --virtual-time-budget=6000 --force-device-scale-factor=1 --force-prefers-reduced-motion \
    --screenshot="$out/$name.png" "http://localhost:3210$p$grid" >/dev/null 2>&1
  echo "$out/$name.png"
done
```

`docs/RUNBOOK.md` line 32 becomes:

`To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done` (`state` = `empty|running|review|refreshing|done`; fixtures in `app/dev/mockBridge.ts`; add `&grid=1` for the alignment overlay, `&fx=lite` for reduced effects, `&appimage=1` for the install offer); `scripts/shots.sh OUT_DIR [filter] [--grid]` screenshots every page.`

- [ ] **Step 7: Verify**

Run: `pnpm test && pnpm typecheck`
Expected: PASS incl. `effects.spec.ts` (3 tests).

Then: `pnpm dev & sleep 8; scripts/shots.sh "$SCRATCH/t3" settings --grid; kill %1` — open the PNG: the grid overlay and the two crimson column lines are visible over the (still old-looking) settings page.

- [ ] **Step 8: Commit**

```bash
git add app/utils/effects.ts app/utils/effects.spec.ts app/composables/useEffects.ts app/composables/useSettingsSaved.ts app/plugins app/dev scripts/shots.sh docs/RUNBOOK.md
git commit -m "Add reduced-effects mode, the reveal directive and the grid overlay for stills

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 4: Brand mark tones, SVG assets, app icon, glow backdrop

**Files:**
- Modify: `app/components/BrandLogo.vue` (rewrite)
- Modify: `public/brand/logo.svg`, `public/brand/icon.svg` (rewrite); regenerate `src-tauri/icons/*`
- Create: `app/components/GlowBackdrop.vue`, `app/components/GlassCard.vue`

**Interfaces:**
- Produces: `<BrandLogo :size tone="mono|brand" :with-wordmark :decorative />`; `<GlowBackdrop />`; `<GlassCard :pad>` (default slot; `pad` default true = `p-6`).

- [ ] **Step 1: Rewrite `app/components/BrandLogo.vue`**

```vue
<script setup lang="ts">
/**
 * The bridge mark: deck, towerless bowl, three dots. `brand` colours it after the Landau ل —
 * sapphire bowl, crimson dots — through CSS-styled gradient stops (a `stop-color` attribute
 * cannot read a CSS variable). Gradient ids are per instance so several marks can share a page.
 */
withDefaults(defineProps<{ size?: number; tone?: 'mono' | 'brand'; withWordmark?: boolean; decorative?: boolean }>(), {
  size: 28, tone: 'brand', withWordmark: false, decorative: false,
});
const gradId = `bowl-${useId()}`;
</script>

<template>
  <span class="inline-flex items-center gap-3">
    <svg
      :width="size" :height="size" viewBox="0 0 64 64" fill="none" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"
      class="brand-mark" :class="`brand-mark--${tone}`"
      :role="withWordmark || decorative ? undefined : 'img'"
      :aria-label="withWordmark || decorative ? undefined : 'پل لاندا'"
      :aria-hidden="withWordmark || decorative ? 'true' : undefined"
    >
      <defs v-if="tone === 'brand'">
        <linearGradient :id="gradId" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" class="brand-mark__bowl-lit" />
          <stop offset="1" class="brand-mark__bowl-deep" />
        </linearGradient>
      </defs>
      <path d="M8 16H56" class="brand-mark__deck" />
      <path d="M14 16C14 44 50 44 50 16" class="brand-mark__bowl" :stroke="tone === 'brand' ? `url(#${gradId})` : undefined" />
      <g class="brand-mark__dots" stroke="none">
        <circle cx="24" cy="48" r="3.5" />
        <circle cx="32" cy="52" r="3.5" />
        <circle cx="40" cy="48" r="3.5" />
      </g>
    </svg>
    <span v-if="withWordmark" class="display text-xl leading-none text-paper">پل لاندا</span>
  </span>
</template>

<style scoped>
.brand-mark { display: block; flex: none; }
.brand-mark--mono .brand-mark__deck, .brand-mark--mono .brand-mark__bowl { stroke: currentColor; }
.brand-mark--mono .brand-mark__dots { fill: currentColor; }
.brand-mark--brand .brand-mark__deck { stroke: var(--color-paper-dim); }
.brand-mark--brand .brand-mark__dots { fill: var(--color-crimson-400); }
.brand-mark__bowl-lit { stop-color: var(--color-sapphire-400); }
.brand-mark__bowl-deep { stop-color: var(--color-sapphire-700); }
</style>
```

- [ ] **Step 2: Static SVGs (literal colours — no CSS variables in files served as images)**

`public/brand/logo.svg` (favicon / window icon in the head):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="none" stroke-width="5" stroke-linecap="round" stroke-linejoin="round">
  <defs><linearGradient id="bowl" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#757cff"/><stop offset="1" stop-color="#2e2bb8"/></linearGradient></defs>
  <path d="M8 16H56" stroke="#c2c3cd"/>
  <path d="M14 16C14 44 50 44 50 16" stroke="url(#bowl)"/>
  <g fill="#e06e6e" stroke="none"><circle cx="24" cy="48" r="3.5"/><circle cx="32" cy="52" r="3.5"/><circle cx="40" cy="48" r="3.5"/></g>
</svg>
```

`public/brand/icon.svg` (source for `tauri icon`):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="ground" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#171b24"/><stop offset="1" stop-color="#0b0d12"/></linearGradient>
    <linearGradient id="bowl" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#757cff"/><stop offset="1" stop-color="#2e2bb8"/></linearGradient>
  </defs>
  <rect width="1024" height="1024" rx="225" fill="url(#ground)"/>
  <rect x="1" y="1" width="1022" height="1022" rx="224" fill="none" stroke="#ffffff" stroke-opacity="0.08" stroke-width="2"/>
  <g transform="translate(194 170) scale(9.94)" fill="none" stroke-width="5" stroke-linecap="round" stroke-linejoin="round">
    <path d="M8 16H56" stroke="#dfdfe5"/>
    <path d="M14 16C14 44 50 44 50 16" stroke="url(#bowl)"/>
    <g fill="#e06e6e" stroke="none"><circle cx="24" cy="48" r="3.5"/><circle cx="32" cy="52" r="3.5"/><circle cx="40" cy="48" r="3.5"/></g>
  </g>
</svg>
```

Run: `pnpm tauri icon public/brand/icon.svg` — Expected: `src-tauri/icons/*` regenerated (git shows them modified). Preview: `rsvg-convert -w 128 public/brand/icon.svg -o "$SCRATCH/icon.png"` and look at it.

- [ ] **Step 3: `app/components/GlowBackdrop.vue`**

```vue
<script setup lang="ts">
/** The lit night backdrop (spec §3.1). Static; the decorative layers are skipped under fx-lite. */
const reduced = useReducedEffects();
const noise =
  'data:image/svg+xml,' +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="160" height="160"><filter id="n"><feTurbulence type="fractalNoise" baseFrequency="0.82" numOctaves="3" stitchTiles="stitch"/></filter><rect width="100%" height="100%" filter="url(#n)"/></svg>`,
  );
</script>

<template>
  <div aria-hidden="true" class="pointer-events-none fixed inset-0 -z-10 overflow-hidden">
    <div class="absolute inset-0 bg-ink-950" />
    <template v-if="!reduced">
      <!-- crimson key light, outside the inline-start (top-right) corner -->
      <div class="absolute h-[70vh] w-[70vw] rounded-full opacity-[0.13]" style="top: -30%; inset-inline-start: -12%; background: radial-gradient(closest-side, #d63e3e, transparent 70%)" />
      <!-- sapphire counter-light, outside the inline-end (bottom-left) corner -->
      <div class="absolute h-[80vh] w-[70vw] rounded-full opacity-[0.10]" style="bottom: -35%; inset-inline-end: -18%; background: radial-gradient(closest-side, #3a3adb, transparent 70%)" />
      <!-- blueprint grid, fading down -->
      <div
        class="absolute inset-0"
        style="
          background-image:
            linear-gradient(to right, rgb(255 255 255 / 0.04) 1px, transparent 1px),
            linear-gradient(to bottom, rgb(255 255 255 / 0.04) 1px, transparent 1px),
            linear-gradient(to right, rgb(255 255 255 / 0.025) 1px, transparent 1px),
            linear-gradient(to bottom, rgb(255 255 255 / 0.025) 1px, transparent 1px);
          background-size: 88px 88px, 88px 88px, 22px 22px, 22px 22px;
          mask-image: radial-gradient(ellipse 90% 70% at 50% 0%, black 30%, transparent 75%);
        "
      />
      <!-- film grain (plain opacity, no blend mode: cheaper on a CPU renderer) -->
      <div class="absolute inset-0 opacity-[0.05]" :style="{ backgroundImage: `url(&quot;${noise}&quot;)` }" />
      <!-- vignette -->
      <div class="absolute inset-0" style="background: radial-gradient(ellipse 120% 90% at 50% 40%, transparent 55%, rgb(4 5 9 / 0.55) 100%)" />
    </template>
  </div>
</template>
```

- [ ] **Step 4: `app/components/GlassCard.vue`**

```vue
<script setup lang="ts">
withDefaults(defineProps<{ pad?: boolean }>(), { pad: true });
</script>

<template>
  <section class="glass" :class="pad && 'p-6'">
    <slot />
  </section>
</template>
```

- [ ] **Step 5: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. Temporarily open `http://localhost:3210/?state=done` with `pnpm dev`: the page background must show the two glows (the sidebar still old — fine).

```bash
git add app/components/BrandLogo.vue app/components/GlowBackdrop.vue app/components/GlassCard.vue public/brand src-tauri/icons
git commit -m "Recolour the bridge mark after the Landau ل and add the glass backdrop

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 5: Shell — sidebar, page header, pills, tiles, running-job card, toasts

**Files:**
- Modify: `app/layouts/default.vue` (rewrite), `app/app.vue`, `app/components/PageHeader.vue` (rewrite), `app/components/RunningJobCard.vue` (rewrite)
- Create: `app/components/StatusPill.vue`, `app/components/StatTile.vue`
- Delete: `app/components/ThemeToggle.vue`

**Interfaces:**
- Produces: `<PageHeader kicker title :lede>` with slot `actions`; `<StatusPill :tone="Tone" :label>`; `<StatTile :label :value :hint :tone>`; `<RunningJobCard :job :compact>`; the sidebar reads `useSettingsSaved()`.
- Consumes: `faVersion`, `jobKindLabel`, `phaseLabel`, `statusLabel`, `faNumber` (Task 2); `.nav-item`, `.nav-active`, `.brand-tile`, `.glass-inset`, `.dot` (Task 1); `GlowBackdrop`, `BrandLogo` (Task 4).

- [ ] **Step 1: `app/components/StatusPill.vue`**

```vue
<script setup lang="ts">
import type { Tone } from '~/utils/fa';

const props = defineProps<{ tone: Tone; label: string }>();
const cls = computed(() => ({
  ok: 'text-ok border-ok/30 bg-ok/6',
  warn: 'text-warn border-warn/30 bg-warn/6',
  danger: 'text-danger border-danger/30 bg-danger/6',
  info: 'text-info border-info/30 bg-info/6',
  muted: 'text-paper-mute border-line-bright bg-white/2',
  brand: 'text-crimson-300 border-crimson-500/35 bg-crimson-500/8',
})[props.tone]);
</script>

<template>
  <span class="inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-[4px] text-[0.6875rem] leading-[12px] font-medium" :class="cls">
    <span class="size-1.5 rounded-full bg-current" aria-hidden="true" />
    {{ label }}
  </span>
</template>
```

- [ ] **Step 2: `app/components/StatTile.vue`**

```vue
<script setup lang="ts">
const props = defineProps<{ label: string; value: number | string; hint?: string; tone?: 'ok' | 'warn' | 'danger' | 'info' }>();
const text = computed(() => (typeof props.value === 'number' ? faNumber(props.value) : props.value));
const toneCls = computed(() => (props.tone ? { ok: 'text-ok', warn: 'text-warn', danger: 'text-danger', info: 'text-info' }[props.tone] : 'text-paper'));
</script>

<template>
  <div class="glass-inset flex min-w-0 flex-col gap-1 px-4 py-3.5">
    <span class="data-label truncate">{{ label }}</span>
    <span class="display tabular text-2xl leading-none" :class="toneCls">{{ text }}</span>
    <span v-if="hint" class="truncate text-xs text-paper-mute">{{ hint }}</span>
  </div>
</template>
```

- [ ] **Step 3: `app/components/PageHeader.vue`**

```vue
<script setup lang="ts">
defineProps<{ kicker: string; title: string; lede?: string }>();
</script>

<template>
  <header class="mb-10">
    <p class="kicker mb-3" data-reveal>{{ kicker }}</p>
    <h1 class="display text-[2.4rem] leading-[1.2] text-paper" data-reveal>{{ title }}</h1>
    <div class="mt-4 flex items-end justify-between gap-6">
      <p v-if="lede" class="max-w-[58ch] text-[0.95rem] leading-relaxed text-paper-dim" data-reveal>{{ lede }}</p>
      <div v-if="$slots.actions" class="ms-auto flex shrink-0 items-center gap-2" data-reveal><slot name="actions" /></div>
    </div>
  </header>
</template>
```

- [ ] **Step 4: `app/components/RunningJobCard.vue`**

```vue
<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const props = defineProps<{ job: JobRow; compact?: boolean }>();
const kindText = computed(() => jobKindLabel(props.job));
const stateText = computed(() => (props.job.status === 'REVIEW' ? statusLabel.REVIEW : (phaseLabel[props.job.phase] ?? props.job.phase)));
const to = computed(() => (props.job.kind === 'IMPORT' ? `/import?job=${props.job.id}` : '/refresh'));
</script>

<template>
  <NuxtLink :to="to" class="glass-inset hover-glow flex items-center gap-2.5 px-3.5 py-3">
    <span class="dot bg-crimson-400 text-crimson-400" aria-hidden="true" />
    <span class="min-w-0 flex-1 leading-tight">
      <span class="block truncate font-medium text-paper-dim" :class="compact ? 'text-xs' : 'text-sm'">{{ kindText }} <span class="tabular">{{ faNumber(job.id) }}</span></span>
      <span class="block truncate text-paper-mute" :class="compact ? 'text-[0.6875rem]' : 'text-xs'">{{ stateText }}</span>
    </span>
    <UIcon name="i-lucide-arrow-left" class="size-3.5 shrink-0 text-paper-mute" />
  </NuxtLink>
</template>
```

- [ ] **Step 5: `app/layouts/default.vue`**

```vue
<script setup lang="ts">
import type { JobRow, SettingsView } from '~/types/bridge';

const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/jobs', label: 'عملیات‌ها', icon: 'i-lucide-history' },
  { to: '/errors', label: 'خطاها و هشدارها', icon: 'i-lucide-triangle-alert' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const route = useRoute();
const isActive = (to: string) => (to === '/' ? route.path === '/' : route.path.startsWith(to));

const running = ref<JobRow | null>(null);
const settings = ref<SettingsView | null>(null);
const savedTick = useSettingsSaved();
const complete = computed(() =>
  !!settings.value && settings.value.has_landau_token && !!settings.value.store && !!settings.value.supplier_user && settings.value.has_supplier_password);

const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
const loadSettings = async () => { settings.value = await bridge.settingsGet().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); loadSettings(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
watch(savedTick, loadSettings);

const version = faVersion(useRuntimeConfig().public.version);
</script>

<template>
  <div class="flex h-screen overflow-hidden">
    <GlowBackdrop />

    <aside class="flex w-[240px] shrink-0 flex-col border-e hairline bg-ink-900/40 backdrop-blur-xl" aria-label="بخش‌های برنامه">
      <NuxtLink to="/" class="flex items-center gap-3 px-5 pt-6 pb-8">
        <span class="brand-tile"><BrandLogo :size="20" tone="mono" decorative /></span>
        <span class="leading-tight">
          <span class="display block text-[1.05rem] text-paper">پل لاندا</span>
          <span class="block text-[0.6875rem] text-paper-mute">بارزپخش ← لاندا</span>
        </span>
      </NuxtLink>

      <nav class="flex-1 space-y-1 px-3" aria-label="صفحه‌ها">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="nav-item" :class="isActive(l.to) && 'nav-active'">
          <UIcon :name="l.icon" class="size-[17px] shrink-0" aria-hidden="true" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>

      <div class="mt-auto flex flex-col gap-3 px-5 pb-5">
        <RunningJobCard v-if="running" :job="running" compact />
        <div v-else class="glass-inset flex items-center gap-2.5 px-3.5 py-3">
          <span class="dot" :class="complete ? 'bg-ok text-ok' : 'bg-warn text-warn'" aria-hidden="true" />
          <div class="min-w-0 leading-tight">
            <div class="text-xs font-medium text-paper-dim">{{ complete ? 'هر دو کرانه وصل است' : 'تنظیمات ناقص' }}</div>
            <div class="truncate text-[0.6875rem] text-paper-mute">
              <template v-if="complete">{{ settings?.store_name ?? settings?.store }}</template>
              <NuxtLink v-else to="/settings" class="hover:text-crimson-300">رفتن به تنظیمات ←</NuxtLink>
            </div>
          </div>
        </div>
        <div class="px-1 text-[0.6875rem] text-paper-mute">نسخهٔ {{ version }}</div>
      </div>
    </aside>

    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-[1060px] px-10 pt-10 pb-20">
        <slot />
      </div>
    </main>
  </div>
</template>
```

- [ ] **Step 6: `app/app.vue` and the theme toggle**

```vue
<script setup lang="ts">
import { fa_ir } from '@nuxt/ui/locale';
</script>

<template>
  <UApp :locale="fa_ir" :toaster="{ position: 'bottom-left', duration: 4200 }">
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </UApp>
</template>
```

`git rm app/components/ThemeToggle.vue` (if not already removed in Task 1).

- [ ] **Step 7: Verify**

Run: `pnpm typecheck && pnpm test` — PASS. `pnpm dev` + `scripts/shots.sh "$SCRATCH/t5" "?state=done"` and `… "settings"`: the sidebar is glass with the sapphire tile, seven items, the active item carries the crimson bar on its right edge, the foot shows the sage dot line and «نسخهٔ ۰.۱.۰». Also `"?state=running"`: the running card with the pulsing dot. With `--grid`: the sidebar edge at 240 px and the column lines land on the 40 px grid.

- [ ] **Step 8: Commit**

```bash
git add -A app/layouts/default.vue app/app.vue app/components/PageHeader.vue app/components/RunningJobCard.vue app/components/StatusPill.vue app/components/StatTile.vue app/components/ThemeToggle.vue
git commit -m "Rebuild the shell: glass sidebar, editorial page header, pills and tiles

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 6: Shared pieces — empty/error states, stepper, timeline, bridge band

**Files:**
- Modify: `app/components/EmptyState.vue`, `app/components/ErrorBanner.vue`, `app/components/JobProgress.vue`, `app/components/SetupChecklist.vue`
- Create: `app/components/EventTimeline.vue`, `app/components/BridgeBand.vue`
- Delete: `app/components/BridgeHero.vue`
- Unchanged (their CSS was restyled in Task 1): `LedgerTable.vue`, `SegmentedControl.vue`, `SimilarityBar.vue`

**Interfaces:**
- Produces: `<EventTimeline :events="(JobEvent|JobEventRow)[]" :kind :limit>`; `<BridgeBand />`; `JobProgress` keeps its props (`kind, phase, progress, events, status`) and now renders a `StatusPill` and an `EventTimeline`.
- Consumes: `reportEventText`, `statusTone`, `isoToLocal`, `phaseLabel`, `statusLabel` (Task 2); `StatusPill` (Task 5).

- [ ] **Step 1: `app/components/EventTimeline.vue`**

```vue
<script setup lang="ts">
import type { JobEvent, JobEventRow } from '~/types/bridge';

type AnyEvent = JobEvent | JobEventRow;
const props = defineProps<{ events: AnyEvent[]; kind: 'IMPORT' | 'REFRESH'; limit?: number }>();
const shown = computed(() => (props.limit ? props.events.slice(0, props.limit) : props.events));
// live `JobEvent`s carry no timestamp; stored rows do
const at = (e: AnyEvent): string | null => ('at' in e && e.at ? isoToLocal(e.at) : null);
const keyOf = (e: AnyEvent, i: number) => ('id' in e ? e.id : i);
</script>

<template>
  <ul class="divide-y divide-white/5">
    <li v-for="(e, i) in shown" :key="keyOf(e, i)" class="flex items-start gap-3 px-6 py-3">
      <span class="level-dot" :data-level="e.level" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <p class="tabular text-xs text-paper-mute">
          <template v-if="at(e)">{{ at(e) }} · </template>
          <template v-if="phaseLabel[e.phase]">{{ phaseLabel[e.phase] }}</template>
          <bdi v-else dir="ltr">{{ e.phase }}</bdi>
        </p>
        <p class="text-sm break-words" :class="e.level === 'ERROR' ? 'text-danger' : e.level === 'WARN' ? 'text-warn' : 'text-paper-dim'">{{ reportEventText(e, kind) }}</p>
        <details v-if="e.detail != null" class="mt-1.5">
          <summary class="cursor-pointer text-xs text-paper-mute hover:text-paper-dim">جزئیات فنی</summary>
          <pre class="code-block mt-2" dir="ltr">{{ JSON.stringify(e.detail, null, 1) }}</pre>
        </details>
      </div>
    </li>
  </ul>
</template>
```

- [ ] **Step 2: `app/components/BridgeBand.vue`**

```vue
<template>
  <div class="relative h-[72px] w-full overflow-hidden border-b hairline" aria-hidden="true">
    <svg class="absolute inset-0 h-full w-full" viewBox="0 0 1000 72" preserveAspectRatio="none" fill="none" stroke-linecap="round" stroke-linejoin="round">
      <path d="M0 58H1000" stroke="var(--color-sapphire-500)" stroke-width="3" vector-effect="non-scaling-stroke" pathLength="1" class="draw" />
      <path d="M220 58V20M780 58V20" stroke="var(--color-sapphire-500)" stroke-width="3" vector-effect="non-scaling-stroke" pathLength="1" class="draw [animation-delay:150ms]" />
      <path d="M0 48Q110 22 220 20Q500 66 780 20Q890 22 1000 48" stroke="var(--color-crimson-400)" stroke-width="2.5" vector-effect="non-scaling-stroke" pathLength="1" class="draw [animation-delay:300ms]" />
      <path d="M290 33V58M360 42V58M430 48V58M500 50V58M570 48V58M640 42V58M710 33V58" stroke="var(--color-sapphire-300)" stroke-width="1.5" vector-effect="non-scaling-stroke" pathLength="1" class="draw [animation-delay:600ms]" />
    </svg>
  </div>
</template>
```

- [ ] **Step 3: `app/components/JobProgress.vue`** — keep the `<script setup>` exactly as it is (phases, index, `pierState`, `percent`); drop `statusColor`; replace the template:

```vue
<template>
  <div class="space-y-5">
    <ol class="stepper" :style="{ '--n': phases.length }" aria-label="مراحل عملیات">
      <li v-for="(p, i) in phases" :key="p" class="stepper-step" :data-state="pierState(i)" :aria-current="i === index && !finished && !halted ? 'step' : undefined">
        <span class="stepper-pier" aria-hidden="true" />
        <span class="stepper-label">{{ phaseLabel[p] }}</span>
      </li>
    </ol>

    <div v-if="progress && progress.total > 0" class="space-y-1.5">
      <UProgress :model-value="percent ?? 0" size="sm" />
      <p class="tabular text-sm text-paper-mute">{{ progress.message }} · {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>

    <div v-if="status" class="flex items-center gap-2 text-sm">
      <span class="text-paper-mute">وضعیت</span>
      <StatusPill :tone="statusTone(status)" :label="statusLabel[status] ?? status" />
    </div>

    <div v-if="events.length" class="glass-inset max-h-64 overflow-auto">
      <EventTimeline :events="events" :kind="kind" />
    </div>
  </div>
</template>
```

- [ ] **Step 4: `app/components/EmptyState.vue`** — same props; recolour the art and text:

```vue
<script setup lang="ts">
withDefaults(defineProps<{ title: string; description?: string; art?: 'bridge' | 'shelf' }>(), { art: 'bridge' });
</script>

<template>
  <div class="flex flex-col items-center gap-3 px-6 py-12 text-center">
    <svg v-if="art === 'bridge'" width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-graphite-600" aria-hidden="true">
      <path d="M4 22H92" /><path d="M20 22v10M76 22v10" /><path d="M20 32Q48 58 76 32" class="text-sapphire-500" />
    </svg>
    <svg v-else width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-graphite-600" aria-hidden="true">
      <path d="M8 12H88M8 28H88M8 44H88" /><path d="M8 6v40M88 6v40" /><rect x="18" y="16" width="14" height="12" rx="1" class="text-sapphire-500" />
    </svg>
    <p class="font-semibold text-paper">{{ title }}</p>
    <p v-if="description" class="max-w-[58ch] text-sm text-paper-mute">{{ description }}</p>
    <div v-if="$slots.action" class="mt-1"><slot name="action" /></div>
  </div>
</template>
```

- [ ] **Step 5: `app/components/ErrorBanner.vue`** — keep the script; template:

```vue
<template>
  <div v-if="error" role="alert" class="glass mb-6 px-4 py-3" style="border-inline-start: 3px solid var(--color-danger)">
    <p class="font-semibold text-danger"><span :dir="isUnknownKind ? 'ltr' : undefined">{{ title }}</span></p>
    <p class="mt-0.5 text-sm text-paper-dim">{{ error.message }}</p>
    <template v-if="error.detail && error.detail !== error.message">
      <p class="data-label mt-2">جزئیات فنی</p>
      <p dir="ltr" class="font-mono text-xs break-all text-paper-mute">{{ error.detail }}</p>
    </template>
  </div>
</template>
```

- [ ] **Step 6: `app/components/SetupChecklist.vue`** — keep the script; template:

```vue
<template>
  <GlassCard class="flex flex-wrap items-center gap-8 !p-8">
    <BrandLogo :size="96" tone="brand" class="shrink-0" />
    <div class="min-w-0 flex-1">
      <h2 class="display text-2xl text-paper">سه قدم تا اولین عبور</h2>
      <p class="mt-1 max-w-[58ch] text-paper-dim">پل وقتی باز می‌شود که هر دو کرانه وصل باشند.</p>
      <ol class="mt-5 space-y-3">
        <li v-for="(s, i) in steps" :key="s.label" class="flex items-start gap-3">
          <span class="check mt-0.5" :class="s.done && 'is-done'">
            <UIcon v-if="s.done" name="i-lucide-check" class="size-3.5" />
            <template v-else>{{ faNumber(i + 1) }}</template>
          </span>
          <span class="min-w-0">
            <span class="block font-medium text-paper">{{ s.label }}</span>
            <span class="block text-sm text-paper-mute">{{ s.hint }}</span>
          </span>
        </li>
      </ol>
      <NuxtLink to="/settings" class="btn-primary mt-6"><UIcon name="i-lucide-settings" class="size-4" />رفتن به تنظیمات</NuxtLink>
    </div>
  </GlassCard>
</template>
```

- [ ] **Step 7: `app/components/SimilarityBar.vue`** — one class: `text-xs text-muted` → `text-xs text-paper-mute` (the template is otherwise unchanged).

- [ ] **Step 8: Delete `BridgeHero.vue`, verify, commit**

`git rm app/components/BridgeHero.vue` — `pages/index.vue` still references it until Task 9; to keep the type check green now, replace `<BridgeHero v-else :dash="dash" />` in `app/pages/index.vue` with `<GlassCard v-else><p class="text-paper-dim">داشبورد در حال بازسازی است.</p></GlassCard>` (Task 9 rewrites the file).

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t6" "import?state=running"`: piers sapphire/crimson, events in the glass inset, the status pill.

```bash
git add -A app/components app/pages/index.vue
git commit -m "Restyle the shared pieces: timeline, bridge band, stepper, empty and error states

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 7: Jobs list and the job sheet

**Files:**
- Create: `app/pages/jobs/index.vue`, `app/pages/jobs/[id].vue`, `app/components/JobSheet.vue`

**Interfaces:**
- Produces: routes `/jobs` and `/jobs/:id`; `<JobSheet :job="JobRow" :store-name :live="{ phase, progress, events, status } | null">`.
- Consumes: `bridge.jobsList()`, `bridge.jobGet(id)`, `bridge.settingsGet()`, `bridge.openLogFolder()`, `useJob()`; `jobKindLabel`, `statusTone`, `summaryTiles`, `summaryLabel`, `faDuration`, `isoToLocal`, `phaseLabel`, `statusLabel`, `faNumber` (Task 2); `GlassCard`, `StatusPill`, `StatTile`, `PageHeader`, `EventTimeline`, `JobProgress`, `LedgerTable`, `SegmentedControl`, `EmptyState`, `ErrorBanner`, `ReportTable` (restyled in Task 10; its current version renders fine meanwhile).

- [ ] **Step 1: `app/components/JobSheet.vue`**

```vue
<script setup lang="ts">
import type { JobEvent, JobRow, Phase, Progress } from '~/types/bridge';

const props = defineProps<{
  job: JobRow;
  storeName?: string | null;
  live?: { phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null } | null;
}>();

const status = computed(() => props.live?.status ?? props.job.status);
const phase = computed(() => props.live?.phase ?? props.job.phase);
const tone = computed(() => statusTone(status.value));
const active = computed(() => status.value === 'RUNNING' || status.value === 'REVIEW');
const strip = computed(() => ({ ok: 'bg-ok/6', danger: 'bg-danger/6', warn: 'bg-warn/6', info: 'bg-info/6', muted: 'bg-white/2', brand: 'bg-crimson-500/6' })[tone.value]);
const circle = computed(() => ({ ok: 'bg-ok/15 text-ok', danger: 'bg-danger/15 text-danger', warn: 'bg-warn/15 text-warn', info: 'bg-info/15 text-info', muted: 'bg-white/8 text-paper-mute', brand: 'bg-crimson-500/15 text-crimson-300' })[tone.value]);
const icon = computed(() => ({ ok: 'i-lucide-check', danger: 'i-lucide-x', warn: 'i-lucide-hourglass', info: 'i-lucide-loader', muted: 'i-lucide-minus', brand: 'i-lucide-check' })[tone.value]);

const facts = computed(() => {
  const j = props.job;
  const out: { label: string; value: string; ltr?: boolean }[] = [];
  if (j.kind === 'IMPORT') out.push({ label: 'حالت', value: j.mode === 'ZERO' ? 'با تعداد صفر، برای بازبینی' : 'با تعداد، از تاریخ' });
  if (j.since_date) out.push({ label: 'از تاریخ', value: j.since_date, ltr: true });
  out.push({ label: 'فروشگاه', value: props.storeName ?? j.store });
  if (j.kind === 'REFRESH') out.push({ label: 'بازنویسی قیمت دستی', value: j.overwrite ? 'بله' : 'خیر' });
  const invoices = j.summary?.invoice_ids;
  if (Array.isArray(invoices)) out.push({ label: 'فاکتورها', value: faNumber(invoices.length) });
  out.push({ label: 'شروع', value: isoToLocal(j.started_at) });
  if (j.finished_at) out.push({ label: 'پایان', value: isoToLocal(j.finished_at) });
  return out;
});
const tiles = computed(() => summaryTiles(props.job));
</script>

<template>
  <GlassCard :pad="false" class="overflow-hidden">
    <div class="flex items-center justify-between gap-4 border-b hairline px-6 py-4" :class="strip">
      <div class="flex items-center gap-3">
        <span class="flex size-8 items-center justify-center rounded-full" :class="circle" aria-hidden="true">
          <UIcon :name="icon" class="size-4" :class="tone === 'info' && 'animate-spin'" />
        </span>
        <div class="leading-tight">
          <div class="text-sm font-medium text-paper">{{ statusLabel[status] ?? status }}</div>
          <div class="text-xs text-paper-mute">مرحله: {{ phaseLabel[phase] ?? phase }}</div>
        </div>
      </div>
      <div class="text-end text-xs text-paper-mute">
        <div>عملیات <span class="tabular">{{ faNumber(job.id) }}</span></div>
        <div class="tabular">{{ faDuration(job.started_at, job.finished_at) }}</div>
      </div>
    </div>
    <div class="space-y-5 px-6 py-5">
      <dl class="grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-3">
        <div v-for="f in facts" :key="f.label">
          <dt class="data-label mb-0.5">{{ f.label }}</dt>
          <dd class="tabular text-[0.8125rem] text-paper-dim"><bdi v-if="f.ltr" dir="ltr">{{ f.value }}</bdi><template v-else>{{ f.value }}</template></dd>
        </div>
      </dl>
      <JobProgress v-if="active && live" :kind="job.kind" :phase="live.phase" :progress="live.progress" :events="live.events" :status="live.status" />
      <div v-else-if="tiles.length" class="grid grid-cols-3 gap-2 md:grid-cols-6">
        <StatTile v-for="t in tiles" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
      </div>
    </div>
  </GlassCard>
</template>
```

- [ ] **Step 2: `app/pages/jobs/index.vue`**

```vue
<script setup lang="ts">
import type { BridgeError, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'IMPORT' | 'REFRESH'>('ALL');
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'IMPORT', label: 'وارد کردن' }, { value: 'REFRESH', label: 'به‌روزرسانی' }] as const;
const shown = computed(() => jobs.value.filter((j) => filter.value === 'ALL' || j.kind === filter.value));
const load = async () => { try { jobs.value = await bridge.jobsList(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
const open = (j: JobRow) => navigateTo(`/jobs/${j.id}`);
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="عملیات‌ها · ۰۵" title="هر عبور، ثبت‌شده." lede="هر وارد کردن و به‌روزرسانی، با نتیجه و رویدادهایش." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع عملیات" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} عملیات</span>
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead>
        <tr><th>#</th><th>نوع</th><th>شروع</th><th>مدت</th><th>وضعیت</th><th>مرحله</th><th>نتیجه</th><th></th></tr>
      </thead>
      <tbody>
        <tr v-for="j in shown" :key="j.id" class="cursor-pointer" tabindex="0" @click="open(j)" @keydown.enter="open(j)">
          <td class="tabular">{{ faNumber(j.id) }}</td>
          <td>{{ jobKindLabel(j) }}<bdi v-if="j.since_date" dir="ltr" class="block text-xs text-paper-mute">{{ j.since_date }}</bdi></td>
          <td class="whitespace-nowrap">{{ isoToLocal(j.started_at) }}</td>
          <td class="whitespace-nowrap">{{ faDuration(j.started_at, j.finished_at) }}</td>
          <td><StatusPill :tone="statusTone(j.status)" :label="statusLabel[j.status] ?? j.status" /></td>
          <td class="whitespace-nowrap">{{ phaseLabel[j.phase] ?? j.phase }}</td>
          <td class="max-w-[22rem] truncate text-xs">{{ summaryLabel(j) || '—' }}</td>
          <td class="text-end"><UIcon name="i-lucide-chevron-left" class="size-4 text-paper-mute" /></td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState title="هنوز عملیاتی اجرا نشده" description="اولین وارد کردن یا به‌روزرسانی اینجا ثبت می‌شود." />
    </GlassCard>
  </div>
</template>
```

`LedgerTable.vue` stays `<div class="ledger"><table><slot /></table></div>`; the page passes `class="glass"` so the container is a glass card (the `.ledger` rule only handles overflow now).

- [ ] **Step 3: `app/pages/jobs/[id].vue`**

```vue
<script setup lang="ts">
import type { BridgeError, JobDetail, SettingsView } from '~/types/bridge';

const route = useRoute();
const id = computed(() => Number(route.params.id));
const detail = ref<JobDetail | null>(null);
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const job = useJob();

const load = async () => {
  error.value = null;
  try {
    detail.value = await bridge.jobGet(id.value);
    if (detail.value.job.status === 'RUNNING' || detail.value.job.status === 'REVIEW') await job.follow(id.value);
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(async () => {
  settings.value = await bridge.settingsGet().catch(() => null);
  await load();
});
watch(id, load);
// once the followed job finishes, useJob reloads its detail — mirror it here
watch(job.detail, (d) => { if (d) detail.value = d; });

const active = computed(() => job.status.value === 'RUNNING' || job.status.value === 'REVIEW');
const live = computed(() => (active.value ? { phase: job.phase.value, progress: job.progress.value, events: job.events.value, status: job.status.value } : null));
const storeName = computed(() => (settings.value && detail.value && settings.value.store === detail.value.job.store ? settings.value.store_name : null));
const lede = computed(() => {
  const j = detail.value?.job;
  if (!j) return '';
  return j.finished_at
    ? `شروع ${isoToLocal(j.started_at)} · پایان ${isoToLocal(j.finished_at)} · ${faDuration(j.started_at, j.finished_at)}`
    : `در حال اجرا از ${isoToLocal(j.started_at)}`;
});
const openFolder = async () => { error.value = null; try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; } };
</script>

<template>
  <div v-reveal>
    <PageHeader :kicker="`عملیات · ${faNumber(id)}`" :title="detail ? jobKindLabel(detail.job) : 'عملیات'" :lede="lede">
      <template #actions>
        <UButton to="/jobs" color="neutral" variant="outline" icon="i-lucide-arrow-right">همهٔ عملیات‌ها</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div v-if="detail" class="space-y-6">
      <JobSheet :job="detail.job" :store-name="storeName" :live="live" data-reveal />

      <GlassCard :pad="false" data-reveal>
        <div class="flex items-center justify-between px-6 pt-5 pb-3">
          <h2 class="display text-xl text-paper">رویدادها</h2>
          <span class="tabular text-xs text-paper-mute">{{ faNumber(detail.events.length) }} رویداد</span>
        </div>
        <div v-if="detail.events.length" class="max-h-96 overflow-auto"><EventTimeline :events="detail.events" :kind="detail.job.kind" /></div>
        <p v-else class="px-6 pb-6 text-sm text-paper-mute">رویدادی ثبت نشده.</p>
      </GlassCard>

      <section data-reveal>
        <h2 class="display mb-3 text-xl text-paper">ردیف‌ها</h2>
        <ReportTable :items="detail.items" :kind="detail.job.kind" />
      </section>

      <details v-if="detail.job.summary" class="text-sm" data-reveal>
        <summary class="cursor-pointer text-xs text-paper-mute hover:text-paper-dim">خلاصهٔ خام</summary>
        <pre class="code-block mt-2" dir="ltr">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
      </details>
    </div>
  </div>
</template>
```

- [ ] **Step 4: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t7" jobs`: `/jobs` lists six rows with pills and summaries; `/jobs/42` shows the sage header strip, seven facts, six tiles, the timeline whose last line reads «گزارش نهایی: ۸۹ اقلام · …», the rows table; `/jobs/40` the rust strip with the failed FETCH event; `/jobs/43?state=running` the live stepper inside the sheet.

```bash
git add app/pages/jobs app/components/JobSheet.vue
git commit -m "Add the jobs list and the job sheet pages

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 8: Errors page; retire the logs page

**Files:**
- Create: `app/pages/errors.vue`
- Delete: `app/pages/logs.vue`

**Interfaces:**
- Produces: route `/errors` (optional `?job=ID`).
- Consumes: `bridge.errorsList(300)`, `bridge.openLogFolder()`; `kindFamily`, `kindLabel`, `isoToLocal`, `faNumber`, `KindFamily` (Task 2); `StatusPill`, `LedgerTable`, `SegmentedControl`, `EmptyState`, `GlassCard`, `PageHeader`, `ErrorBanner`; `.field`, `.ledger-roomy`, `.code-block`.

- [ ] **Step 1: `app/pages/errors.vue`**

```vue
<script setup lang="ts">
import type { BridgeError, ErrorRow } from '~/types/bridge';
import type { KindFamily } from '~/utils/fa';

const route = useRoute();
const router = useRouter();
const rows = ref<ErrorRow[]>([]);
const error = ref<BridgeError | null>(null);
const family = ref<'ALL' | KindFamily>('ALL');
const families = [{ value: 'ALL', label: 'همه' }, { value: 'supplier', label: 'بارزپخش' }, { value: 'landau', label: 'لاندا' }, { value: 'app', label: 'برنامه' }] as const;
const query = ref('');
const opened = ref<number | null>(null);

const jobFilter = computed(() => { const n = Number(route.query.job); return Number.isFinite(n) && n > 0 ? n : null; });
const shown = computed(() => rows.value.filter((r) =>
  (family.value === 'ALL' || kindFamily(r.kind) === family.value)
  && (jobFilter.value == null || r.job_id === jobFilter.value)
  && (!query.value || r.message.includes(query.value) || kindLabel(r.kind).includes(query.value))));

const load = async () => { try { rows.value = await bridge.errorsList(300); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
const clearJob = () => router.replace({ query: {} });
const openFolder = async () => { error.value = null; try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; } };
const toggle = (e: ErrorRow) => { if (e.context != null) opened.value = opened.value === e.id ? null : e.id; };
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="خطاها و هشدارها · ۰۶" title="هر لغزش، با جزئیات." lede="هر خطا یا هشداری که در اجرا ثبت شده، با نوع، عملیات و جزئیات فنی.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="family" :items="families" aria-label="خانوادهٔ خطا" />
      <button
        v-if="jobFilter != null" type="button"
        class="inline-flex items-center gap-1.5 rounded-full border border-crimson-500/35 bg-crimson-500/8 px-2.5 py-[4px] text-[0.6875rem] leading-[12px] font-medium text-crimson-300"
        @click="clearJob"
      >
        فقط عملیات {{ faNumber(jobFilter) }}
        <UIcon name="i-lucide-x" class="size-3" aria-hidden="true" /><span class="sr-only">حذف فیلتر</span>
      </button>
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} مورد</span>
      <input v-model="query" class="field ms-auto w-72" placeholder="جست‌وجو در پیام یا نوع" aria-label="جست‌وجو" />
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead><tr><th>زمان</th><th>عملیات</th><th>نوع</th><th>پیام</th><th></th></tr></thead>
      <tbody>
        <template v-for="e in shown" :key="e.id">
          <tr class="ledger-roomy" :class="e.context != null && 'cursor-pointer'" @click="toggle(e)">
            <td class="whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td>
              <NuxtLink v-if="e.job_id != null" :to="`/jobs/${e.job_id}`" @click.stop><StatusPill tone="muted" :label="faNumber(e.job_id)" /></NuxtLink>
              <span v-else class="text-paper-mute">—</span>
            </td>
            <td class="whitespace-nowrap"><StatusPill tone="muted" :label="kindLabel(e.kind)" /></td>
            <td class="break-words">{{ e.message }}</td>
            <td class="text-end"><UIcon v-if="e.context != null" name="i-lucide-chevron-down" class="size-4 text-paper-mute transition-transform" :class="opened === e.id && 'rotate-180'" /></td>
          </tr>
          <tr v-if="opened === e.id">
            <td colspan="5" class="!border-t-0 !pt-0">
              <div class="glass-inset px-4 py-3">
                <p class="data-label mb-2">جزئیات فنی</p>
                <pre class="code-block" dir="ltr">{{ JSON.stringify(e.context, null, 1) }}</pre>
              </div>
            </td>
          </tr>
        </template>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState
        :title="rows.length ? 'موردی با این فیلتر نیست' : 'خطا یا هشداری ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'هر مشکلی در اجرا، با نوع و جزئیاتش، اینجا فهرست می‌شود.'"
      />
    </GlassCard>
  </div>
</template>
```

- [ ] **Step 2: Retire the logs page**

`git rm app/pages/logs.vue`. Then `grep -rn "'/logs\|\`/logs\|/logs?" app/` — the only hits are in `app/pages/index.vue` (rewritten in Task 9); change them now so nothing links to a dead route: `to: '/logs'` → `to: '/errors'` and `` `/logs?job=${r.job.id}` `` → `` `/jobs/${r.job.id}` ``.

- [ ] **Step 3: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t8" errors`: six roomy rows, muted pills, the search field at the inline-end; with `&job=42` two rows and the crimson chip.

```bash
git add -A app/pages/errors.vue app/pages/logs.vue app/pages/index.vue
git commit -m "Add the errors page and retire the logs page

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 9: Dashboard — status band, «عبور» card, recent activity

**Files:**
- Modify: `app/pages/index.vue` (rewrite)

**Interfaces:**
- Consumes: `bridge.dashboardGet()`, `bridge.jobsList()`, `bridge.jobCancel()`, `bridge.appimageInstall()`, `bridge.appimageDismiss()`, `useJob()`; `summaryTiles`, `summaryLabel`, `jobKindLabel`, `statusTone`, `faDuration`, `isoToLocal`, `faNumber`, `statusLabel` (Task 2); `GlassCard`, `StatusPill`, `StatTile`, `BridgeBand`, `JobProgress`, `EventTimeline`, `SetupChecklist`, `PageHeader`, `ErrorBanner`.

- [ ] **Step 1: Rewrite `app/pages/index.vue`**

```vue
<script setup lang="ts">
import type { BridgeError, Dashboard, JobRow, SettingsView } from '~/types/bridge';

const toast = useToast();
const dash = ref<Dashboard | null>(null);
const settings = ref<SettingsView | null>(null);
const recent = ref<JobRow[]>([]);
const error = ref<BridgeError | null>(null);
const job = useJob();

const load = async () => {
  try {
    const d = await bridge.dashboardGet();
    settings.value = d.settings_complete ? null : await bridge.settingsGet();
    recent.value = d.settings_complete ? (await bridge.jobsList()).slice(0, 6) : [];
    dash.value = d;
    if (d.running && job.jobId.value !== d.running.id) await job.follow(d.running.id);
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);
// the followed job finished: refresh the overview so «آخرین …» and the list catch up
watch(job.status, (s) => { if (s && !['RUNNING', 'REVIEW'].includes(s)) load(); });

const live = computed(() => (dash.value?.running ? { phase: job.phase.value, progress: job.progress.value, events: job.events.value, status: job.status.value } : null));
const runningTo = computed(() => (dash.value?.running ? (dash.value.running.kind === 'IMPORT' ? `/import?job=${dash.value.running.id}` : '/refresh') : '/'));
const inReview = computed(() => dash.value?.running?.status === 'REVIEW');
const supplierReady = computed(() => !!dash.value?.supplier_user);
const lede = computed(() => dash.value?.store_name
  ? `فاکتورهای بارزپخش خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قواعد خودتان روی قفسهٔ «${dash.value.store_name}» می‌نشیند.`
  : 'فاکتورهای بارزپخش خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قواعد خودتان روی قفسهٔ لاندا می‌نشیند.');

const lastRuns = computed(() => [
  { key: 'import', label: 'آخرین وارد کردن', job: dash.value?.last_import ?? null, to: '/import', action: 'وارد کردن فاکتورها' },
  { key: 'refresh', label: 'آخرین به‌روزرسانی قیمت', job: dash.value?.last_refresh ?? null, to: '/refresh', action: 'به‌روزرسانی قیمت‌ها' },
]);
const dotOf = (j: JobRow) => ({ ok: 'bg-ok', danger: 'bg-danger', warn: 'bg-warn', info: 'bg-warn', muted: 'bg-graphite-500', brand: 'bg-crimson-400' })[statusTone(j.status)];

const cancel = async () => {
  if (!dash.value?.running) return;
  error.value = null;
  try {
    await bridge.jobCancel(dash.value.running.id);
    toast.add({ title: 'درخواست توقف ثبت شد', color: 'neutral' });
  } catch (e) { error.value = e as BridgeError; }
};

// Linux AppImage only: offer to put the file in the applications menu (README, "Other Linux").
const installing = ref(false);
const installAppImage = async () => {
  error.value = null; installing.value = true;
  try {
    const path = await bridge.appimageInstall();
    toast.add({ title: 'در فهرست برنامه‌ها نصب شد', description: path, color: 'success' });
    await load();
  } catch (e) { error.value = e as BridgeError; } finally { installing.value = false; }
};
const dismissAppImage = async () => {
  try { await bridge.appimageDismiss(); await load(); } catch (e) { error.value = e as BridgeError; }
};
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="داشبورد · ۰۱" title="دو کرانه، یک قفسه." :lede="lede">
      <template v-if="dash?.settings_complete" #actions>
        <NuxtLink v-if="dash.running" :to="runningTo" class="btn-ghost">رفتن به عملیات جاری</NuxtLink>
        <template v-else>
          <NuxtLink to="/refresh" class="btn-ghost"><UIcon name="i-lucide-refresh-cw" class="size-4" />به‌روزرسانی قیمت‌ها</NuxtLink>
          <NuxtLink to="/import" class="btn-primary"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink>
        </template>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <template v-if="dash">
      <div v-if="dash.appimage_offer" class="glass-inset mb-6 flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4" data-reveal>
        <UIcon name="i-lucide-package-plus" class="size-6 shrink-0 text-crimson-400" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <p class="font-medium text-paper">برنامه از فایل AppImage اجرا شده است</p>
          <p class="text-sm text-paper-mute">می‌خواهید در فهرست برنامه‌ها نصب شود؟ نسخه‌ای از فایل در پوشهٔ خانگی شما (<code dir="ltr">~/.local/bin</code>) نگه داشته می‌شود و میان‌بر آن به منوی برنامه‌ها اضافه می‌شود.</p>
        </div>
        <div class="flex gap-2">
          <UButton size="sm" icon="i-lucide-download" :loading="installing" @click="installAppImage">نصب</UButton>
          <UButton size="sm" variant="outline" color="neutral" :disabled="installing" @click="dismissAppImage">نه، دیگر نپرس</UButton>
        </div>
      </div>

      <SetupChecklist v-if="!dash.settings_complete && settings" :settings="settings" data-reveal />

      <template v-else>
        <!-- status band -->
        <div class="mb-6 grid gap-4 md:grid-cols-4">
          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex items-center justify-between"><span class="data-label">بارزپخش</span><StatusPill :tone="supplierReady ? 'ok' : 'warn'" :label="supplierReady ? 'آماده' : 'تنظیم‌نشده'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.invoices_cached) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">فاکتور ذخیره‌شده</div>
            <div class="mt-auto pt-3 text-xs" :class="dash.invoices_unpaid > 0 ? 'text-warn' : 'text-paper-mute'">{{ dash.invoices_unpaid > 0 ? `${faNumber(dash.invoices_unpaid)} پرداخت‌نشده` : 'همه پرداخت‌شده' }}</div>
            <div v-if="dash.supplier_user" class="truncate font-mono text-[0.6875rem] text-paper-mute" dir="ltr">{{ dash.supplier_user }}</div>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex items-center justify-between"><span class="data-label">لاندا</span><StatusPill :tone="dash.store_name ? 'ok' : 'warn'" :label="dash.store_name ? 'آماده' : 'تنظیم‌نشده'" /></div>
            <div class="display truncate text-[1.75rem] leading-none text-paper">{{ dash.store_name ?? '—' }}</div>
            <div class="mt-1.5 text-xs text-paper-mute tabular">{{ faNumber(dash.decided_matches) }} تطبیق ثبت‌شده</div>
            <NuxtLink to="/matches" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">تطبیق‌ها ←</NuxtLink>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex items-center justify-between"><span class="data-label">در انتظار شما</span><StatusPill :tone="dash.pending_review > 0 ? 'warn' : 'ok'" :label="dash.pending_review > 0 ? 'نیاز به تصمیم' : 'چیزی نیست'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.pending_review) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">قلم منتظر تصمیم</div>
            <NuxtLink v-if="inReview" :to="runningTo" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">ادامهٔ بازبینی ←</NuxtLink>
            <div v-else class="mt-auto pt-3 text-xs text-paper-mute">تصمیم‌ها در مرحلهٔ بازبینی پرسیده می‌شود</div>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex items-center justify-between"><span class="data-label">سلامت</span><StatusPill :tone="dash.recent_errors > 0 ? 'danger' : 'ok'" :label="dash.recent_errors > 0 ? 'بررسی کنید' : 'پاک'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.recent_errors) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">خطا و هشدار در هفت روز</div>
            <NuxtLink to="/errors" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">خطاها ←</NuxtLink>
          </GlassCard>
        </div>

        <!-- the crossing -->
        <GlassCard :pad="false" class="mb-6 overflow-hidden" data-reveal>
          <BridgeBand />
          <div v-if="dash.running" class="space-y-5 px-6 py-5">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p class="data-label">{{ inReview ? 'در انتظار تصمیم شما' : 'در حال اجرا' }}</p>
                <h2 class="display text-xl text-paper">{{ jobKindLabel(dash.running) }} <span class="tabular">{{ faNumber(dash.running.id) }}</span></h2>
              </div>
              <div class="flex gap-2">
                <NuxtLink :to="runningTo" class="btn-primary btn-sm">{{ inReview ? 'ادامهٔ بازبینی' : 'مشاهده' }}</NuxtLink>
                <button type="button" class="btn-danger btn-sm" @click="cancel">توقف</button>
              </div>
            </div>
            <JobProgress v-if="live" :kind="dash.running.kind" :phase="live.phase" :progress="live.progress" :events="live.events.slice(0, 3)" :status="live.status" />
          </div>
          <div v-else class="grid gap-6 px-6 py-5 md:grid-cols-2">
            <div v-for="r in lastRuns" :key="r.key" class="flex min-w-0 flex-col">
              <p class="data-label mb-2">{{ r.label }}</p>
              <template v-if="r.job">
                <div class="mb-3 flex flex-wrap items-center gap-2 text-xs text-paper-mute">
                  <StatusPill :tone="statusTone(r.job.status)" :label="statusLabel[r.job.status] ?? r.job.status" />
                  <span class="tabular">{{ isoToLocal(r.job.started_at) }}</span>
                  <span class="tabular">· {{ faDuration(r.job.started_at, r.job.finished_at) }}</span>
                </div>
                <div v-if="summaryTiles(r.job).length" class="grid grid-cols-3 gap-2">
                  <StatTile v-for="t in summaryTiles(r.job)" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
                </div>
                <NuxtLink :to="`/jobs/${r.job.id}`" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
              </template>
              <template v-else>
                <p class="mb-3 text-sm text-paper-mute">هنوز اجرا نشده</p>
                <NuxtLink :to="r.to" class="btn-ghost btn-sm mt-auto self-start">{{ r.action }}</NuxtLink>
              </template>
            </div>
          </div>
        </GlassCard>

        <!-- recent activity -->
        <GlassCard :pad="false" data-reveal>
          <div class="flex items-center justify-between px-6 pt-5 pb-3">
            <h2 class="display text-xl text-paper">فعالیت اخیر</h2>
            <span class="text-xs text-paper-mute">شش عملیات آخر</span>
          </div>
          <p v-if="!recent.length" class="px-6 pt-2 pb-8 text-sm text-paper-mute">هنوز عملیاتی اجرا نشده.</p>
          <ul v-else class="divide-y divide-white/5">
            <li v-for="j in recent" :key="j.id">
              <NuxtLink :to="`/jobs/${j.id}`" class="flex items-center gap-4 px-6 py-3 hover:bg-white/3">
                <span class="size-2 shrink-0 rounded-full" :class="dotOf(j)" aria-hidden="true" />
                <div class="min-w-0 flex-1">
                  <div class="text-[0.8125rem] text-paper">{{ jobKindLabel(j) }} <bdi v-if="j.since_date" dir="ltr" class="text-xs text-paper-mute">{{ j.since_date }}</bdi></div>
                  <div class="truncate text-xs text-paper-mute">{{ summaryLabel(j) || (statusLabel[j.status] ?? j.status) }}</div>
                </div>
                <div class="shrink-0 text-end text-xs text-paper-mute tabular">
                  <div>{{ isoToLocal(j.started_at) }}</div>
                  <div>{{ faDuration(j.started_at, j.finished_at) }}</div>
                </div>
                <UIcon name="i-lucide-chevron-left" class="size-4 shrink-0 text-paper-mute" aria-hidden="true" />
              </NuxtLink>
            </li>
          </ul>
          <div class="border-t hairline px-6 py-3"><NuxtLink to="/jobs" class="text-xs text-crimson-400 hover:text-crimson-300">همهٔ عملیات‌ها ←</NuxtLink></div>
        </GlassCard>
      </template>
    </template>
  </div>
</template>
```

- [ ] **Step 2: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t9" "/?state"`: `done` — four equal cards on one baseline, the band card with two columns of tiles, six activity rows; `running` — the live panel with stepper and «توقف»; `review` — «در انتظار تصمیم شما» and the amber «نیاز به تصمیم» pill; `empty` — only the checklist; `appimage=1` — the offer inset. With `--grid`: card edges on the 8 px grid, the column gutters at 40 px.

```bash
git add app/pages/index.vue
git commit -m "Rebuild the dashboard as a one-glance overview

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 10: Import and refresh pages, review cards, report table

**Files:**
- Modify: `app/pages/import.vue`, `app/pages/refresh.vue`, `app/components/ReviewTable.vue`, `app/components/ReportTable.vue`

**Interfaces:**
- Consumes: `nextSinceDate`, `priceDelta`, `priceDeltaLabel`, `summaryTiles`, `faNumber`, `rials`, `verdictLabel`, `statusLabel`, `kindLabel`, `statusTone` (Task 2); `bridge.dashboardGet()` (for the smart date and the first-run path); `GlassCard`, `StatusPill`, `StatTile`, `PageHeader`, `ErrorBanner`, `JobProgress`, `SegmentedControl`, `LedgerTable`, `SimilarityBar`, `EmptyState`; `.ticket`, `.field`, `.btn-*`.
- `ReportTable` keeps `props: { items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }`; `ReviewTable` keeps `props: { items; defaultAuthenticity }` and `emit('decided', item_id)`.

- [ ] **Step 1: `app/components/ReportTable.vue`** — keep the script's CSV logic; add the delta column, the search field and the «تغییر قیمت» filter:

Script changes:

```ts
const filter = ref<'ALL' | 'DONE' | 'CHANGED' | 'SKIPPED' | 'FAILED'>('ALL');
const query = ref('');
const rows = computed(() => props.items.filter((i) =>
  (filter.value === 'ALL' || (filter.value === 'CHANGED' ? priceDelta(i)?.dir === 'up' || priceDelta(i)?.dir === 'down' : i.status === filter.value))
  && (!query.value || i.name.includes(query.value) || i.code.includes(query.value) || (i.product_name ?? '').includes(query.value))));
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'DONE', label: 'انجام‌شده' }, { value: 'CHANGED', label: 'تغییر قیمت' }, { value: 'SKIPPED', label: 'ردشده' }, { value: 'FAILED', label: 'ناموفق' }] as const;
```

In `csv()` add a header «تغییر قیمت» after «قیمت قفسه بعد» and the value `priceDeltaLabel(priceDelta(i))` in the same position.

Template:

```vue
<template>
  <div class="space-y-3">
    <div class="flex flex-wrap items-center gap-3">
      <SegmentedControl v-model="filter" :items="filters" aria-label="فیلتر ردیف‌ها" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(rows.length) }} ردیف</span>
      <input v-model="query" class="field ms-auto w-64" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در ردیف‌ها" />
      <UButton v-if="items.length" size="sm" color="neutral" variant="outline" icon="i-lucide-download" :loading="saving" @click="csv">خروجی CSV</UButton>
    </div>
    <ErrorBanner :error="error" />
    <LedgerTable v-if="rows.length" class="glass">
      <thead>
        <tr><th>قطعه</th><th>اقدام</th><th>تعداد</th><th>قیمت خرید</th><th>قیمت قفسه</th><th>تغییر قیمت</th><th>نتیجهٔ قیمت‌گذاری</th><th>وضعیت</th></tr>
      </thead>
      <tbody>
        <tr v-for="i in rows" :key="i.id">
          <td>
            <p class="font-medium text-paper">{{ i.product_name ?? i.name }}</p>
            <p class="text-xs text-paper-mute">{{ i.name }} <bdi dir="ltr" class="whitespace-nowrap">{{ i.code }}</bdi></p>
          </td>
          <td>{{ i.action ? actionLabel[i.action] : '—' }}</td>
          <td class="whitespace-nowrap">
            <span v-if="i.qty_before != null" class="block text-xs text-paper-mute">{{ faNumber(i.qty_before) }}</span>
            <span class="font-medium text-paper">{{ faNumber(i.qty_after ?? i.qty) }}</span>
          </td>
          <td class="whitespace-nowrap">{{ rials(i.cost) }}</td>
          <td class="whitespace-nowrap">
            <span v-if="i.price_before != null" class="block text-xs text-paper-mute">{{ faNumber(i.price_before) }}</span>
            <span class="font-medium text-paper">{{ faNumber(i.price_after) }}</span>
          </td>
          <td class="whitespace-nowrap" :class="priceDelta(i)?.dir === 'up' ? 'text-warn' : priceDelta(i)?.dir === 'down' ? 'text-info' : 'text-paper-mute'">{{ priceDeltaLabel(priceDelta(i)) }}</td>
          <td class="whitespace-nowrap">{{ verdictLabel(i.reprice_verdict) }}</td>
          <td>
            <StatusPill :tone="statusTone(i.status)" :label="statusLabel[i.status] ?? i.status" />
            <p v-if="i.error_msg" class="mt-1 text-xs text-danger">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
          </td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else-if="items.length" :pad="false"><EmptyState art="shelf" title="ردیفی با این فیلتر نیست" description="فیلتر یا عبارت جست‌وجو را عوض کنید." /></GlassCard>
    <GlassCard v-else :pad="false"><EmptyState art="shelf" title="این عملیات ردیفی ندارد" /></GlassCard>
  </div>
</template>
```

`statusTone` maps item statuses too: DONE ok, FAILED danger, everything else (SKIPPED, PLANNED, WRITTEN) muted — that is the intended quiet look.

- [ ] **Step 2: `app/components/ReviewTable.vue`** — keep the script; template:

```vue
<template>
  <div class="space-y-4">
    <ErrorBanner :error="error" />
    <p class="max-w-[58ch] text-sm text-paper-mute">برای هر قلم، قطعهٔ درست را از فهرست لاندا انتخاب کنید. «هیچ‌کدام» یعنی این قطعه در لاندا نیست: دیگر پرسیده نمی‌شود و باید برای افزودنش تیکتی برای پشتیبانی لاندا ثبت کنید. «بعداً» آن را برای اجرای بعدی نگه می‌دارد.</p>
    <GlassCard v-for="(item, n) in items" :key="item.id" :pad="false">
      <div class="flex flex-wrap items-center justify-between gap-3 border-b hairline px-6 py-4">
        <div class="min-w-0">
          <h3 class="display text-[1.0625rem] text-paper">{{ item.name }}</h3>
          <p class="tabular text-xs text-paper-mute"><bdi dir="ltr" class="whitespace-nowrap">{{ item.code }}</bdi> · تعداد {{ faNumber(item.qty) }} · {{ faNumber(n + 1) }} از {{ faNumber(items.length) }}</p>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
          <button type="button" class="btn-ghost btn-sm" @click="decide(item, { later: true })">بعداً</button>
          <button type="button" class="btn-danger btn-sm" @click="decide(item, { none: true })">هیچ‌کدام</button>
        </div>
      </div>
      <div class="space-y-3 px-6 py-5">
        <div class="flex gap-2">
          <input v-model="terms[item.id]" class="field flex-1" placeholder="جست‌وجوی دوباره در لاندا با عبارت دیگر" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" color="neutral" variant="outline" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-paper-mute">لاندا پیشنهادی نداشت. عبارت دیگری را امتحان کنید یا «هیچ‌کدام» را بزنید.</p>
        <ul v-else class="divide-y divide-white/5">
          <li v-for="c in candidatesOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
            <div class="min-w-0">
              <p class="truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-paper-mute" dir="auto">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              <SimilarityBar :value="c.similarity" />
              <button type="button" class="btn-primary btn-sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">همین است</button>
            </div>
          </li>
        </ul>
      </div>
    </GlassCard>
  </div>
</template>
```

- [ ] **Step 3: `app/pages/import.vue`** — script: add the dashboard load for the smart date and the first-run path; template rewritten.

Script additions/changes (the rest of the script is unchanged):

```ts
import type { BridgeError, Dashboard, JobItemRow, SettingsView } from '~/types/bridge';
const dash = ref<Dashboard | null>(null);
const firstRun = computed(() => !!dash.value && dash.value.decided_matches === 0 && dash.value.last_import === null);
```

In `onMounted`, right after `settings.value = await bridge.settingsGet();` add:

```ts
    dash.value = await bridge.dashboardGet();
    since.value = nextSinceDate(dash.value.last_import);
```

Template:

```vue
<template>
  <div v-reveal>
    <PageHeader kicker="وارد کردن فاکتورها · ۰۲" title="فاکتورها، روی قفسه." lede="فاکتورهای پرداخت‌شده خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قیمت روز روی قفسه می‌نشیند.">
      <template v-if="active" #actions>
        <button type="button" class="btn-danger" @click="cancel"><UIcon name="i-lucide-square" class="size-4" />توقف</button>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="space-y-4" data-reveal>
      <div v-if="firstRun" class="glass-inset flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
        <div class="min-w-0 flex-1">
          <p class="data-label mb-1">مسیر پیشنهادی برای بار اول</p>
          <p class="text-sm text-paper-dim">۱. همهٔ فاکتورهای قدیمی را با تعداد صفر بیاورید و تطبیق‌ها را یک بار تصمیم بگیرید.</p>
          <p class="text-sm text-paper-dim">۲. از فردا، «با تعداد، از تاریخ» را اجرا کنید؛ تاریخ شروع خودش پیشنهاد می‌شود.</p>
        </div>
        <button type="button" class="btn-ghost btn-sm" @click="mode = 'ZERO'">انتخاب حالت بازبینی</button>
      </div>

      <div class="grid gap-4 md:grid-cols-2">
        <label class="glass ticket" :data-selected="mode === 'QUANTITY'">
          <input v-model="mode" type="radio" name="mode" value="QUANTITY" class="sr-only" />
          <UIcon v-if="mode === 'QUANTITY'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="display block text-lg text-paper">با تعداد، از تاریخ</span>
          <span class="mt-1 block text-sm text-paper-mute">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد، منهای مرجوعی‌ها، به قفسه اضافه می‌شود. اگر قطعه از قبل روی قفسه باشد به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</span>
          <span class="mt-4 block text-sm font-medium text-paper">از تاریخ</span>
          <input v-model="since" class="field mt-1 w-40" :class="!sinceOk && 'field-invalid'" aria-label="از تاریخ" placeholder="۱۴۰۵/۰۱/۰۱" dir="ltr" :disabled="mode !== 'QUANTITY'" />
          <span class="mt-3 flex items-center justify-between gap-4">
            <span class="text-sm text-paper">فقط فاکتورهای از این تاریخ از بارزپخش خوانده شود<span class="block text-xs text-paper-mute">خاموش: کل صورت‌حساب از ۱۳۹۰ خوانده و ذخیره می‌شود؛ کندتر، ولی فقط بار اول.</span></span>
            <USwitch v-model="sinceOnly" aria-label="فقط فاکتورهای از این تاریخ خوانده شود" :disabled="mode !== 'QUANTITY'" />
          </span>
        </label>
        <label class="glass ticket" :data-selected="mode === 'ZERO'">
          <input v-model="mode" type="radio" name="mode" value="ZERO" class="sr-only" />
          <UIcon v-if="mode === 'ZERO'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="display block text-lg text-paper">با تعداد صفر، برای بازبینی</span>
          <span class="mt-1 block text-sm text-paper-mute">همهٔ اقلام همهٔ فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</span>
        </label>
      </div>
      <p class="max-w-[58ch] text-sm text-paper-mute">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لاندا از روی قیمت روز بارزپخش با ارزش افزوده محاسبه می‌شود، نه قیمت روی فاکتور.</p>
      <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع وارد کردن</UButton>
    </section>

    <GlassCard v-if="job.jobId.value" :pad="false" class="mt-6" data-reveal>
      <div class="flex items-center justify-between border-b hairline px-6 py-4">
        <h2 class="display text-[1.0625rem] text-paper">عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
        <StatusPill v-if="job.status.value" :tone="statusTone(job.status.value)" :label="statusLabel[job.status.value] ?? job.status.value" />
      </div>
      <div class="px-6 py-5">
        <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="null" />
      </div>
    </GlassCard>

    <section v-if="job.status.value === 'REVIEW'" class="mt-6">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <h2 class="display text-xl text-paper">بازبینی اقلام تطبیق‌نشده <span class="tabular text-paper-mute">({{ faNumber(review.length) }})</span></h2>
        <UButton icon="i-lucide-arrow-left" trailing @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
      </div>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </section>

    <section v-if="finished && job.detail.value" class="mt-6 space-y-3">
      <div v-if="summaryTiles(job.detail.value.job).length" class="grid grid-cols-3 gap-2 md:grid-cols-6">
        <StatTile v-for="t in summaryTiles(job.detail.value.job)" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
      </div>
      <div class="flex items-center justify-between">
        <h2 class="display text-xl text-paper">گزارش</h2>
        <NuxtLink :to="`/jobs/${job.jobId.value}`" class="text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
      </div>
      <ReportTable :items="job.detail.value.items" kind="IMPORT" />
    </section>
  </div>
</template>
```

(The job card passes `:status="null"` to `JobProgress` because the pill already sits in its header.)

- [ ] **Step 4: `app/pages/refresh.vue`** — script unchanged; template:

```vue
<template>
  <div v-reveal>
    <PageHeader kicker="به‌روزرسانی قیمت‌ها · ۰۳" title="قیمت‌ها، به روز." lede="قیمت خرید هر ردیف از قیمت روز بارزپخش تازه می‌شود و قواعد قیمت‌گذاری لاندا دوباره اجرا می‌شود.">
      <template v-if="active" #actions>
        <button type="button" class="btn-danger" @click="cancel"><UIcon name="i-lucide-square" class="size-4" />توقف</button>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <GlassCard v-if="!job.jobId.value || finished" data-reveal>
      <div class="space-y-5">
        <p class="max-w-[58ch] text-sm text-paper-dim">برای هر ردیف قفسه که به یک کد بارزپخش وصل است، قیمت روز بارزپخش با ارزش افزوده به عنوان قیمت خرید ثبت می‌شود و سپس قواعد قیمت‌گذاری لاندا روی کل فروشگاه اجرا می‌شود. ردیف‌هایی که به بارزپخش وصل نیستند در گزارش فهرست می‌شوند و دست نمی‌خورند.</p>
        <div class="flex items-center justify-between gap-6 border-t hairline pt-5">
          <div>
            <div class="text-sm text-paper">قیمت‌های دستی لاندا را هم بازنویسی کن</div>
            <p class="text-xs text-paper-mute">لاندا قیمتی را که خودتان در پنل تایپ کرده باشید حفظ می‌کند مگر این گزینه روشن باشد.</p>
          </div>
          <USwitch v-model="overwrite" aria-label="بازنویسی قیمت‌های دستی" />
        </div>
        <UButton size="lg" icon="i-lucide-refresh-cw" :loading="starting" @click="start">شروع به‌روزرسانی</UButton>
      </div>
    </GlassCard>

    <GlassCard v-if="job.jobId.value" :pad="false" class="mt-6" data-reveal>
      <div class="flex items-center justify-between border-b hairline px-6 py-4">
        <h2 class="display text-[1.0625rem] text-paper">عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
        <StatusPill v-if="job.status.value" :tone="statusTone(job.status.value)" :label="statusLabel[job.status.value] ?? job.status.value" />
      </div>
      <div class="px-6 py-5">
        <JobProgress kind="REFRESH" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="null" />
      </div>
    </GlassCard>

    <section v-if="finished && job.detail.value" class="mt-6 space-y-3">
      <div v-if="summaryTiles(job.detail.value.job).length" class="grid grid-cols-3 gap-2">
        <StatTile v-for="t in summaryTiles(job.detail.value.job)" :key="t.label" :label="t.label" :value="t.value" />
      </div>
      <div class="flex items-center justify-between">
        <h2 class="display text-xl text-paper">گزارش</h2>
        <NuxtLink :to="`/jobs/${job.jobId.value}`" class="text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
      </div>
      <ReportTable :items="job.detail.value.items" kind="REFRESH" />
    </section>
  </div>
</template>
```

- [ ] **Step 5: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t10" import` and `… refresh`: the two glass tickets with the crimson selected ring; `review` — glass cards with the quiet «هیچ‌کدام» and the hot «همین است»; `done` — six tiles above the report with the «تغییر قیمت» column (▲ amber, ▼ sapphire) and the search field. In the running state the «توقف» button sits at the header's inline-end.

```bash
git add app/pages/import.vue app/pages/refresh.vue app/components/ReviewTable.vue app/components/ReportTable.vue
git commit -m "Restyle the import and refresh flows with tickets, review cards and price deltas

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 11: Matches page

**Files:**
- Modify: `app/pages/matches.vue` (template only)

- [ ] **Step 1: Replace the template** (script unchanged):

```vue
<template>
  <div v-reveal>
    <PageHeader kicker="تطبیق‌ها · ۰۴" title="هر کد، یک قطعه." lede="هر کد بارزپخش یک بار به یک قطعهٔ لاندا وصل می‌شود و دیگر پرسیده نمی‌شود." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} تطبیق</span>
      <input v-model="query" class="field ms-auto w-72" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در تطبیق‌ها" />
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead>
        <tr><th>نام در فاکتور</th><th>کد</th><th>قطعهٔ لاندا</th><th>نحوه</th><th>شباهت</th><th>تاریخ</th><th></th></tr>
      </thead>
      <tbody>
        <tr v-for="r in shown" :key="r.code">
          <td class="font-medium text-paper">{{ r.name_seen }}</td>
          <td><bdi dir="ltr" class="whitespace-nowrap">{{ r.code }}</bdi></td>
          <td>{{ r.product_name ?? '—' }}</td>
          <td class="whitespace-nowrap"><StatusPill :tone="r.how === 'NONE' ? 'danger' : r.how === 'USER' ? 'ok' : 'info'" :label="howLabel[r.how] ?? r.how" /></td>
          <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-paper-mute">—</span></td>
          <td class="whitespace-nowrap">{{ isoToLocal(r.decided_at) }}</td>
          <td class="text-end"><button type="button" class="btn-ghost btn-sm" @click="openEdit(r)">تغییر</button></td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState
        :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها، تطبیق‌های خودکار و تصمیم‌های شما اینجا جمع می‌شود. برای اقلامی که «در لاندا نیست» علامت خورده‌اند، تیکتی برای پشتیبانی لاندا ثبت کنید و بعد تصمیم را همین‌جا تغییر دهید.'"
      >
        <template v-if="!rows.length" #action><NuxtLink to="/import" class="btn-ghost"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink></template>
      </EmptyState>
    </GlassCard>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-4">
          <p><span class="font-medium text-paper">{{ editing.name_seen }}</span> <bdi dir="ltr" class="whitespace-nowrap text-sm text-paper-mute">{{ editing.code }}</bdi></p>
          <div class="flex gap-2">
            <input v-model="term" class="field flex-1" aria-label="عبارت جست‌وجو" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لاندا</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-white/5 overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
              <span class="min-w-0 truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></span>
              <span class="flex shrink-0 items-center gap-3"><SimilarityBar :value="c.similarity" /><button type="button" class="btn-primary btn-sm" @click="pick(c)">انتخاب</button></span>
            </li>
          </ul>
          <div class="flex flex-wrap justify-between gap-2 border-t hairline pt-4">
            <button type="button" class="btn-danger" @click="setNone">در لاندا نیست</button>
            <button type="button" class="btn-ghost" @click="clear">حذف تصمیم، دوباره پرسیده شود</button>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
```

- [ ] **Step 2: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t11" matches`: the glass ledger with three pill tones, the search at the inline-end. Open the page in the browser and click «تغییر»: the modal is glass with the dark overlay.

```bash
git add app/pages/matches.vue
git commit -m "Restyle the matches page

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 12: Settings — six glass cards, header save, advanced disclosure

**Files:**
- Modify: `app/pages/settings.vue` (rewrite)

**Interfaces:**
- Consumes: `useSettingsSaved()` (Task 3), `useReducedEffects`/`setReducedEffects` (Task 3), `copyText` behaviour via `navigator.clipboard`; `GlassCard`, `StatusPill`, `PageHeader`, `ErrorBanner`.

- [ ] **Step 1: Rewrite `app/pages/settings.vue`**

```vue
<script setup lang="ts">
import type { BridgeError, LandauStatus, SettingsView } from '~/types/bridge';

const DEFAULT_URL = 'https://api.landau.ir';
const toast = useToast();
const view = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const landau = ref<LandauStatus | null>(null);
const busy = ref<'landau' | 'supplier' | 'save' | null>(null);
const savedTick = useSettingsSaved();
const advancedOpen = ref(false);

const supplierOk = ref(false);
const landauPill = computed(() =>
  landau.value ? { label: `متصل به ${landau.value.business_name ?? landau.value.business}`, tone: 'ok' as const }
    : view.value?.has_landau_token ? { label: 'کلید ذخیره‌شده', tone: 'muted' as const }
      : { label: 'آزمایش‌نشده', tone: 'warn' as const });
const supplierPill = computed(() =>
  supplierOk.value ? { label: 'ورود موفق', tone: 'ok' as const }
    : view.value?.has_supplier_password ? { label: 'رمز ذخیره‌شده', tone: 'muted' as const }
      : { label: 'آزمایش‌نشده', tone: 'warn' as const });

// overwrite_manual and fetch_since_only are edited where they apply (refresh / import pages), not here.
const form = reactive({
  landau_url: '', landau_token: '', store: '', supplier_user: '', supplier_password: '',
  default_authenticity: 'ORIGINAL' as 'ORIGINAL' | 'FAKE', threshold: 80, vat_percent: 10,
});

const load = async () => {
  try {
    view.value = await bridge.settingsGet();
    Object.assign(form, {
      landau_url: view.value.landau_url, store: view.value.store ?? '', supplier_user: view.value.supplier_user ?? '',
      default_authenticity: view.value.default_authenticity, threshold: view.value.threshold, vat_percent: view.value.vat_percent,
      landau_token: '', supplier_password: '',
    });
    advancedOpen.value = view.value.landau_url !== DEFAULT_URL;
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

const save = async () => {
  error.value = null; busy.value = 'save';
  try {
    const storeName = landau.value?.stores.find((s) => s.id === form.store)?.name ?? view.value?.store_name ?? undefined;
    view.value = await bridge.settingsSave({
      landau_url: form.landau_url, landau_token: form.landau_token || undefined, store: form.store || undefined, store_name: storeName ?? undefined,
      supplier_user: form.supplier_user, supplier_password: form.supplier_password || undefined,
      default_authenticity: form.default_authenticity, threshold: form.threshold, vat_percent: form.vat_percent,
    });
    form.landau_token = ''; form.supplier_password = '';
    savedTick.value++;
    toast.add({ title: 'تنظیمات ذخیره شد', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const validateLandau = async () => {
  error.value = null; busy.value = 'landau';
  try {
    view.value = await bridge.settingsSave({ landau_url: form.landau_url, landau_token: form.landau_token || undefined });
    form.landau_token = '';
    landau.value = await bridge.landauValidate();
    if (!form.store && landau.value.stores.length === 1) form.store = landau.value.stores[0]!.id;
    toast.add({ title: `اتصال به لاندا برقرار است: ${landau.value.business_name ?? landau.value.business}`, color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const testSupplier = async () => {
  error.value = null; busy.value = 'supplier';
  try {
    view.value = await bridge.settingsSave({ supplier_user: form.supplier_user, supplier_password: form.supplier_password || undefined });
    form.supplier_password = '';
    await bridge.supplierTestLogin();
    supplierOk.value = true;
    toast.add({ title: 'ورود به بارزپخش موفق بود', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const reducedFx = computed({ get: () => useReducedEffects().value, set: (v: boolean) => setReducedEffects(v) });
const copyPath = async () => {
  if (!view.value) return;
  try { await navigator.clipboard.writeText(view.value.data_dir); toast.add({ title: 'کپی شد', color: 'success' }); }
  catch { toast.add({ title: 'دسترسی به کلیپ‌بورد ممکن نبود', color: 'error' }); }
};

const storeItems = computed(() => (landau.value?.stores ?? []).map((s) => ({ label: s.name ?? s.id, value: s.id })));
const authItems = [{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }];
</script>

<template>
  <div v-reveal class="mx-auto max-w-3xl">
    <PageHeader kicker="تنظیمات · ۰۷" title="دو کرانه را وصل کنید." lede="کلید لاندا و ورود بارزپخش، و چند قاعدهٔ وارد کردن. هر چیز دیگری پیش‌فرض خوبی دارد.">
      <template #actions>
        <UButton :loading="busy === 'save'" icon="i-lucide-save" @click="save">ذخیرهٔ تنظیمات</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="space-y-6">
      <GlassCard data-reveal>
        <div class="mb-5 flex items-center justify-between gap-3"><h2 class="display text-xl text-paper">لاندا</h2><StatusPill :tone="landauPill.tone" :label="landauPill.label" /></div>
        <div class="space-y-4">
          <UFormField label="کلید API لاندا" :hint="view?.has_landau_token ? 'کلیدی ذخیره شده است؛ برای تغییر، کلید تازه را وارد کنید' : 'در لاندا: حساب کاربری ← کلیدهای API'">
            <UInput v-model="form.landau_token" type="password" dir="ltr" placeholder="apt-…" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'landau'" color="neutral" variant="outline" icon="i-lucide-plug-zap" @click="validateLandau">بررسی اتصال و دریافت فروشگاه‌ها</UButton>
          <p v-if="landau && landau.active_rules === 0" class="text-sm text-warn">هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید؛ تا زمانی که در لاندا قاعده‌ای بسازید، قیمت قفسه برابر قیمت خرید خواهد بود.</p>
          <p v-else-if="landau" class="tabular text-sm text-paper-mute">{{ faNumber(landau.active_rules) }} قاعدهٔ قیمت‌گذاری فعال</p>
          <UFormField label="فروشگاه مقصد" hint="ردیف‌ها روی قفسهٔ این فروشگاه نوشته می‌شوند">
            <USelect v-model="form.store" :items="storeItems" :disabled="!storeItems.length" placeholder="ابتدا اتصال را بررسی کنید" class="w-full" />
            <p v-if="!storeItems.length && view?.store_name" class="mt-1 text-sm text-paper-mute">فروشگاه ذخیره‌شده: {{ view.store_name }}</p>
          </UFormField>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <div class="mb-5 flex items-center justify-between gap-3"><h2 class="display text-xl text-paper">بارزپخش</h2><StatusPill :tone="supplierPill.tone" :label="supplierPill.label" /></div>
        <div class="space-y-4">
          <UFormField label="نام کاربری"><UInput v-model="form.supplier_user" dir="ltr" class="w-full" /></UFormField>
          <UFormField label="کلمه عبور" :hint="view?.has_supplier_password ? 'ذخیره شده؛ برای تغییر، مقدار تازه را وارد کنید' : ''">
            <UInput v-model="form.supplier_password" type="password" dir="ltr" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'supplier'" color="neutral" variant="outline" icon="i-lucide-log-in" @click="testSupplier">آزمایش ورود</UButton>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <h2 class="display mb-5 text-xl text-paper">قواعد وارد کردن</h2>
        <div class="space-y-4">
          <UFormField label="اصالت پیش‌فرض ردیف‌های تازه" hint="وضعیت همیشه «نو» ثبت می‌شود؛ اصالت را می‌توانید برای هر قلم در مرحلهٔ بازبینی تغییر دهید">
            <USelect v-model="form.default_authenticity" :items="authItems" class="w-full" />
          </UFormField>
          <UFormField label="نرخ ارزش افزوده (٪)" hint="قیمت خرید = قیمت فهرست بارزپخش × (۱ + نرخ)">
            <UInput v-model.number="form.vat_percent" type="number" min="0" max="100" step="0.5" dir="ltr" class="w-32" />
          </UFormField>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <h2 class="display mb-5 text-xl text-paper">نمایش</h2>
        <div class="flex items-center justify-between gap-6">
          <div>
            <div class="text-sm text-paper">جلوه‌های بصری کمتر</div>
            <p class="text-xs text-paper-mute">پنل‌های مات به جای شیشه، بدون هاله و انیمیشن؛ برای رایانه‌های ضعیف. خودکار تشخیص داده می‌شود و روی همین دستگاه ذخیره می‌ماند.</p>
          </div>
          <USwitch v-model="reducedFx" aria-label="جلوه‌های بصری کمتر" />
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <details :open="advancedOpen" @toggle="advancedOpen = ($event.target as HTMLDetailsElement).open">
          <summary class="flex cursor-pointer list-none items-center justify-between gap-3">
            <h2 class="display text-xl text-paper">تنظیمات پیشرفته</h2>
            <UIcon name="i-lucide-chevron-down" class="size-4 text-paper-mute transition-transform" :class="advancedOpen && 'rotate-180'" aria-hidden="true" />
          </summary>
          <div class="mt-5 space-y-4">
            <UFormField label="نشانی درگاه لاندا" :hint="`پیش‌فرض: ${DEFAULT_URL}`">
              <UInput v-model="form.landau_url" dir="ltr" class="w-full" />
            </UFormField>
            <UFormField label="آستانهٔ تطبیق خودکار" hint="از ۰ تا ۱۰۰؛ بالاتر یعنی محتاط‌تر. پیش‌فرض با آزمایش روی دادهٔ واقعی انتخاب شده">
              <UInput v-model.number="form.threshold" type="number" min="0" max="100" dir="ltr" class="w-32" />
            </UFormField>
          </div>
        </details>
      </GlassCard>

      <GlassCard :pad="false" class="px-6 py-4" data-reveal>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="data-label mb-1">پوشهٔ داده</div>
            <button type="button" class="max-w-full cursor-pointer truncate font-mono text-xs text-paper-dim transition-colors hover:text-crimson-300" dir="ltr" title="برای کپی کلیک کنید" @click="copyPath">{{ view?.data_dir ?? '—' }}</button>
          </div>
        </div>
        <p class="mt-3 text-xs" :class="view?.secret_backend === 'file' ? 'text-warn' : 'text-paper-mute'">
          {{ view?.secret_backend === 'keyring' ? 'رمزها در ذخیره‌ساز امن سیستم‌عامل نگه داشته می‌شوند.' : 'ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده‌ای با دسترسی محدود در پوشهٔ داده نگه داشته می‌شوند.' }}
        </p>
      </GlassCard>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Verify and commit**

Run: `pnpm typecheck && pnpm test` — PASS. `scripts/shots.sh "$SCRATCH/t12" settings`: six glass cards, pills in the card heads, the save button in the header at the inline-end, the advanced card collapsed (fixture URL is the default). Check in the browser that toggling «جلوه‌های بصری کمتر» turns the panels solid immediately and survives a reload.

```bash
git add app/pages/settings.vue
git commit -m "Prune and restyle settings: six glass cards, header save, advanced disclosure

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 13: Final verification — full screenshot pass, alignment, fx-lite, real app, docs

**Files:**
- Modify: `README.md` (fonts note under «### Build», developer pointers), `docs/RUNBOOK.md` (already updated in Task 3; re-check)

- [ ] **Step 1: Dead references**

```bash
grep -rn "ThemeToggle\|BridgeHero\|useColorMode\|\$colorMode\|Vazirmatn\|/logs\|text-highlighted\|text-muted\b\|bg-elevated\|border-default\|surface-head\|surface-body\|UBadge\|UTabs" app/ nuxt.config.ts app.config.ts 2>/dev/null
```
Expected: no hits (Nuxt UI's `text-muted`/`bg-elevated` utilities are replaced by the paper/ink classes everywhere; `UBadge`/`UTabs` are no longer used). Fix any hit.

- [ ] **Step 2: Full still pass**

```bash
pnpm dev & sleep 8
scripts/shots.sh "$SCRATCH/final" && scripts/shots.sh "$SCRATCH/final" "" --grid
kill %1
```
Open every PNG and check against the spec: one hot element per page; pills quiet; the four dashboard values on one baseline; card edges and the column gutters on the grid; no Latin outside `dir="ltr"` islands; `?state=done&fx=lite` shows solid panels and no glows. Fix what fails and re-shoot that page.

- [ ] **Step 3: Contrast and bundle**

`pnpm test` (palette + fa + effects), then `pnpm generate` and:

```bash
! grep -rl 'Vazirmatn\|mockBridge\|fonts.googleapis' .output/public | grep .
ls .output/public/fonts
```
Expected: nothing listed by grep; the two woff2 files present.

- [ ] **Step 4: The real app**

`pnpm tauri dev`: window colour is ink before the page paints; new icon in the title bar / taskbar; sidebar and every route; the seller's flows once: import in both modes (the since-only switch persists), review one item, refresh, change a match, save settings (advanced open/close, the sidebar status line updates), jobs list → sheet → «خطاها» chip on `/errors?job=…`, reduced-effects toggle. Note anything behavioural that moved — nothing should.

- [ ] **Step 5: Docs**

In `README.md` under «### Build» add after the bundle table:

`The UI ships two self-hosted variable fonts from the Landau front end, **Mikhak** (headings) and **Estedad** (text), both under the SIL Open Font License 1.1 (`public/fonts/`).`

In «## For developers» add the two documents: `Redesign: docs/superpowers/specs/2026-09-14-night-glass-redesign-design.md, plan docs/superpowers/plans/2026-09-14-night-glass-redesign.md.`

Confirm `docs/RUNBOOK.md` «Design review without Tauri» matches Task 3 Step 6.

- [ ] **Step 6: Commit and hand over**

```bash
git add README.md docs/RUNBOOK.md
git commit -m "Document the fonts and the redesign; final verification pass

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
git log --oneline main..HEAD
```

Then rebase onto `main` (`git fetch && git rebase origin/main`, resolve conflicts in favour of keeping both the redesign and any concurrent behavioural change), re-run `pnpm typecheck && pnpm test`, and report the branch with the final stills.
