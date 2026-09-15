# Task 14 Report: Estedad in WebKitGTK — missing dots and text/icon alignment

**Status:** DONE_WITH_CONCERNS (the fix is the third option in the controller's order — Estedad
cannot ship at all; see *Concerns*.)

`$SCRATCH` = `/tmp/claude-1000/-home-vahid-WorkSpace-landau-bridge/65d8d792-5c28-4dc7-8db0-5c6de787aa9d/scratchpad`

---

## 1. Reproduction

Built `scripts/wk-shot.py` first, so every claim below is a picture I can retake.

```sh
GDK_BACKEND=x11 DISPLAY=:0 scripts/wk-shot.py URL OUT.png [W H] [--settle MS] [--nowait] [--motion] [--visible]
```

It loads the URL in a `WebKit2.WebView` (webkit2gtk-4.1 — the engine Tauri uses), waits, and writes
a PNG via `get_snapshot` + cairo `write_to_png`. Two things it has to do that a naive version does
not, both found the hard way:

- **The window is mapped off-screen (-4000,-4000), not a `Gtk.OffscreenWindow`.** The page-enter
  reveal is driven by `requestAnimationFrame` (`app/plugins/reveal.client.ts`), which never fires in
  an unmapped window — `<main>` snapshotted completely blank.
- **GTK animations off + wait for a quiet DOM.** WebKit reports `gtk-enable-animations: false` as
  `prefers-reduced-motion: reduce`, which is what `scripts/shots.sh` asks Chrome for with
  `--force-prefers-reduced-motion`; it then polls `document.body.innerHTML.length` every 200 ms until
  three readings agree. Before that, the same page gave 19044 / 27578 / 19044 inked pixels on three
  consecutive runs — half-populated stills that varied run to run. After: 48630 / 48630 / 48630, and
  matching Chrome's 48949.

Before-stills (Estedad, both engines): `$SCRATCH/t14/before/` — `settings_state_done__{chrome,webkit}.png`,
`_state_done__*`, `jobs_state_done__*`, `import_state_review__*`.

The WebKit before-still of `/settings?state=done` shows it at a glance: «داشبورد» → «داسبورد»,
«تنظیمات» → «سطیماب», «تطبیق‌ها» → «بطبیق‌ها», «هشدارها» → «هسدارها».

**The pattern the transliterations hide:** dots *below* the letter survive, dots *above* are gone.
ب (one below) and پ (three below) keep theirs; ت, ش, ن, ز, خ, ظ all lose theirs. That is the whole
bug in one sentence, and it is what pointed at the cause.

---

## 2. Font-file findings — Estedad vs Mikhak

Tools: `fontTools` 4.65.0 + `brotli` + `uharfbuzz` in `$SCRATCH/t14/venv`.

### Provenance — the shipped file is already the latest upstream

| | |
| --- | --- |
| `public/fonts/estedad-var.woff2` | `name[5] = "Version 8.5"`, 2276 glyphs, `wght` 100–900 |
| Upstream `Estedad-v8.5/Estedad[wght].woff2` | md5 `440f276eca2e0e47e848f9e8a27a3e94` |
| Shipped file | md5 `440f276eca2e0e47e848f9e8a27a3e94` — **byte-identical** |

8.5 is the current release (`api.github.com/repos/aminabedi68/Estedad/releases/latest`, fetched with
`curl`; the network was reachable). So the shipped file is neither a damaged subset nor an old build.

### The root cause in the file: Estedad has no precomposed dotted letters

`cmap` maps U+0628 «ب» to the glyph `uni0628`, and that glyph is **empty** — `numberOfContours = 0`,
not a composite. Same for `uni062A`, `uni0634`, every dotted letter. The shapes come from GSUB, and
the `init`/`medi`/`fina`/`isol` features do it with a **Multiple Substitution** that splits each
letter into a dotless skeleton plus a free-standing dot mark:

```
lookup15 (init):  uni0628 -> ['uni066E.init', 'dotbelowar']            ب
lookup15 (init):  uni0634 -> ['uni0633.init', 'threedotsupabovear']    ش
lookup11 (fina):  uni062A -> ['uni066E.fina', 'twodotshorizontalabovear']  ت
lookup11 (fina):  uni0646 -> ['uni06BA.fina', 'dotabovear']            ن
```

GPOS `mark` (lookups 4–38, mark-to-base) then attaches the mark. The anchors are sane — e.g. for
`uni066E.init` the above-anchor is (123, 408) and `dotabovear`'s mark anchor is (0, −107), putting
the dot 515 units up.

The precomposed positional glyphs **exist as names but are empty placeholders**:

```
uni0628.init  contours=0  bbox=(0,0,0,0)  adv=600
uni0628.isol  contours=0  bbox=(0,0,0,0)  adv=600
uni0634.init  contours=0  bbox=(0,0,0,0)  adv=600
uni066E.init  contours=1  bbox=(-14,0,228,371)  adv=305   <- the dotless skeleton is the real one
```

So there is **no way to rebuild Estedad to avoid the decomposition** — the whole-letter outlines do
not exist in the file. That closes the "re-export it" door.

Mikhak is built the opposite way — whole glyphs with the dots in them:

```
Mikhak  U+0628 uni0628: yMin=-454 yMax=646   (real outline)
Mikhak  U+0634 uni0634: yMin=-490 yMax=1190  (real outline)
```

That is exactly why Mikhak never showed the bug.

### Vertical metrics

| | upm | hhea asc/desc/gap | OS/2 typo | OS/2 win | USE_TYPO_METRICS |
| --- | --- | --- | --- | --- | --- |
| Estedad 8.5 | 1000 | 1025 / −500 / 0 | 1025 / −500 / 0 | 1025 / 500 | yes |
| Mikhak 3.4 | 2048 | 2200 / −1200 / 0 | 2200 / −1200 / 0 | 2553 / 1444 | yes |

Estedad's `usWinAscent` equals its typo ascender exactly (no headroom) and `head.yMax` is 1001 —
under it, so nothing is clipped by the metrics. The metrics were **not** the cause of the missing
dots; they were the cause of the alignment half of the bug (§5).

---

## 3. Hypotheses tested

Every row is a snapshot I took, not a deduction.

| # | Hypothesis | Test | Result |
| --- | --- | --- | --- |
| H3 | The shipped woff2 is a damaged subset or an old build | md5 vs upstream 8.5 | **Rejected** — byte-identical to the current release |
| H1 | Composite glyphs with point-matching in a VF; FreeType drops the dot components | Listed every point-matched component: all 452 are Latin accented glyphs (`uni1E28`, `dcaron`, …). The Arabic dots are not components at all — they are separate glyphs joined by GPOS | **Rejected** |
| H4 | `gvar` deltas at the requested weight move the dots off-glyph | Rendered Estedad **static instances** (upstream `Statics/webfonts/Estedad-{Regular,Medium,SemiBold,Bold}.woff2`, no `gvar`, no `fvar`) at 400/500/600/700 | **Rejected** — statics lose the dots identically (`$SCRATCH/t14/work/diag_webkit.png` rows 5–9) |
| — | The woff2 container / `font-variation-settings` | Rendered the upstream **TTF** of both the VF and the static | **Rejected** — both fail the same way (diag rows 4 and 9) |
| H2 | The dots are GPOS marks and `mark` is not applied on WebKit's shaping path | `font-feature-settings: 'mark' 0` in WebKit | **Refined** — with `mark` **off** the dots *appear* (unattached, near the baseline); with `mark` **on** they vanish. So the glyphs are in the buffer and it is the `mark` positioning that loses them |

Then I isolated which layer is at fault:

- **HarfBuzz is correct.** `uharfbuzz` (HB 14.4.0, the system version) shapes «نزشتخ بپ» and gives the
  above-dots a positive `y_offset` of +366…+529 and the below-dots −121…−124. Nothing is dropped,
  nothing is mispositioned. Identical for the VF and the static.
- **FreeType + Cairo are correct.** Rendered the same font and string through **Pango**
  (`$SCRATCH/t14/work/pango.png`) — system HarfBuzz, system FreeType, Cairo, no WebKit. **Every dot
  renders.**
- **WebKit is where it breaks**, and only for these marks. A mark-class probe
  (`$SCRATCH/t14/work/marks_webkit.png`) shows WebKit renders, in Estedad:
  - Arabic dots **above** → **missing**
  - Arabic dots **below** → fine
  - harakat above (fatha/damma/sukun) → **fine**
  - harakat below (kasra) → fine
  - Latin combining above (a+U+0301, e+U+0308) → **fine**
  - Latin combining below (a+U+0323) → fine

  In the very same cluster — «نَ» = dotless-noon + `dotabovear` + `uni064E` — WebKit draws the fatha
  and drops the letter's own dot. Disabling `mkmk`, `rlig`, `ccmp`, `kern`, `locl` and the `ss01`/
  `ss02` stylistic sets changes nothing; only `mark` does.

### Root cause

**Estedad draws no dotted Arabic letter as a single glyph.** Every one is a GSUB multiple
substitution into a dotless skeleton plus a separate dot-mark glyph, re-joined by GPOS mark-to-base.
**WebKitGTK 2.52 drops the marks that attach above the letter** (it keeps the ones below, and keeps
harakat and Latin combining marks above), so every Persian letter dotted above loses its dots. The
font is not malformed — HarfBuzz 14.4.0 shapes it correctly and Pango renders it correctly through
the same FreeType — so this is a defect in WebKit's own text path that I cannot patch. Because
Estedad's precomposed positional glyphs are empty placeholders, **no Estedad build can avoid it.**

---

## 4. The fix, and the options rejected

The controller's order was: (1) latest upstream VF, (2) Estedad statics, (3) Mikhak, (4) Vazirmatn —
take the first that satisfies R1 and R2.

| Option | R1 (dots) | Evidence |
| --- | --- | --- |
| **1. Latest upstream Estedad VF (8.5)** | **FAIL** | Byte-identical to what we shipped; `diag_webkit.png` rows 1–3 (400/500/700) and row 10 (`font-variation-settings`) all dotless |
| **2. Estedad static instances, one face per weight** | **FAIL** | `diag_webkit.png` rows 5–8 — Regular/Medium/SemiBold/Bold all dotless; row 9 the static TTF likewise |
| **3. Mikhak for text as well** | **PASS** | `cand_webkit.png` rows 2–5 — 400/500/600/700 all keep every dot |
| 4. Vazirmatn | (pass, not needed) | `cand_webkit.png` rows 6–9 — also fine; recovered from git history (`bc0165c^`), precomposed glyphs |

**Chosen: Mikhak for text as well** — the first option that satisfies R1 and R2.

```css
/* app/assets/css/main.css */
@font-face { font-family: 'Mikhak'; src: url('/fonts/mikhak-var.woff2') format('woff2'); … }
--font-display: 'Mikhak', system-ui, sans-serif;
--font-sans:    'Mikhak', system-ui, sans-serif;
```

`public/fonts/estedad-var.woff2` is deleted rather than kept as a fallback: it covers all of Persian,
so a fallback would silently reintroduce dotless rendering for any character Mikhak lacks — exactly
the failure that cost a whole pass to notice.

**Coverage checked against the real source.** Scanned every character in `app/**` + `README.md`
(172 distinct) against Mikhak's cmap. Mikhak covers every Persian letter, Persian and ASCII digits,
the full Latin alphabet and all punctuation used. Five characters are missing: `←` (11×, visible UI —
«بارزپخش ← لاندا», «رفتن به تنظیمات ←»), `→`, `≥`, and `▲`/`▼` (which Estedad lacked too and already
fell back). **Vazirmatn lacks `←` and `→` as well**, so this is not a reason to prefer it. They now
come from `system-ui` and render correctly (visible in the after-stills).

No `ascent-override` / `descent-override` / `line-gap-override` was added — see §5.

---

## 5. Alignment (R2) — measurements

Measured from pixels (ink centre of the text vs ink centre of the icon), not from layout rects: both
engines agree on the line box; the difference is where the glyphs sit inside it.

### The real app, `/settings?state=done`, save button «ذخیرهٔ تنظیمات»

| | Chrome | WebKitGTK |
| --- | --- | --- |
| **before** (Estedad) | −1.5 px | **−4.0 px** |
| **after** (Mikhak) | −0.5 px | **−1.5 px** |

(Negative = text above the icon centre. The before/WebKit figure *understates* the error: the missing
above-dots shrink the measured ink box downward. The controlled mock below removes that confound.)

Sidebar nav items, after: Chrome and WebKit produce **identical** numbers (0.0 / −1.0 / −1.0 / −0.5 /
−0.5 px down the list). The button pill also lands at exactly y 132–167 in **both** engines after the
change, where before the two engines disagreed.

### Controlled mock, every control type (`$SCRATCH/t14/work/r2_{chrome,webkit}.png`)

The app's own `.btn-primary` / `.btn-sm` / `.btn-lg` / `.nav-item` / StatusPill / `.field` rules,
text-vs-icon ink centre:

| control | height | Estedad Chrome | Estedad **WebKit** | Mikhak Chrome | Mikhak **WebKit** |
| --- | --- | --- | --- | --- | --- |
| `.btn-primary` | 36 px | +0.5 | **−5.5** | −1.0 | **−1.0** |
| `.btn-sm` | 30 px | −1.5 | **−4.0** | −1.0 | **−1.5** |
| `.btn-lg` | 44 px | −1.5 | **−6.0** | −1.5 | **−1.5** |
| `.nav-item` | 38 px | — | — | −0.5 | **−0.5** |
| StatusPill | 22 px | — | — | +0.5 | **+0.5** |
| `.field` | 36 px | — | — | +0.0 | **+0.5** |

An earlier isolated mock (`cand_webkit.png` / `cand_chrome.png`) gives the cleanest single number:
**Estedad WebKit −6.0 px vs Chrome +0.0 px; Mikhak −1.0 px in both; Vazirmatn 0.0 px in both.**

**Control heights do not move.** Read back from the DOM in the mock, identical for both faces:
`.btn-primary` 36.0, `.btn-sm` 30.0, `.btn-lg` 44.0, `.nav-item` 38.0, pill 22.0, `.field` 36.0.

### Why no metric override

With `line-height` fixed (as it is on every control), the baseline's offset from the line-box centre
is `(ascent − descent) / 2`, so `ascent-override`/`descent-override` is the only lever and it moves
**both** engines equally. After the switch the two engines agree to within 0.5 px on every control,
and the residual −1.0…−1.5 px on buttons is Mikhak's own optical centre, present identically in
Chrome — the same value the spec's approved Chrome stills already carry for the headings Mikhak
always set. Correcting it would overshoot Chrome into positive territory and shift every line box in
the app, including h1–h3. The defect R2 names — a 5–6 px WebKit-only error — is gone; what is left is
not an engine discrepancy, so I left the metrics alone.

---

## 6. Verification

```
pnpm typecheck   -> exit 0
pnpm test        -> exit 0    Test Files 7 passed (7),  Tests 32 passed (32)
pnpm generate    -> exit 0
  .output/public/fonts/    -> mikhak-var.woff2 (66668 bytes)  — only that file
  .output/public/_fonts/   -> does not exist
  grep -rl estedad .output -> no matches
```

Stills, 1200 × 800, `&fx=full`, both engines, four pages:

| | before (Estedad) | after (Mikhak) |
| --- | --- | --- |
| `/settings?state=done` | `$SCRATCH/t14/before/settings_state_done__{chrome,webkit}.png` | `$SCRATCH/t14/after/settings_state_done__{chrome,webkit}.png` |
| `/?state=done` | `$SCRATCH/t14/before/_state_done__{chrome,webkit}.png` | `$SCRATCH/t14/after/_state_done__{chrome,webkit}.png` |
| `/jobs?state=done` | `$SCRATCH/t14/before/jobs_state_done__{chrome,webkit}.png` | `$SCRATCH/t14/after/jobs_state_done__{chrome,webkit}.png` |
| `/import?state=review` | `$SCRATCH/t14/before/import_state_review__{chrome,webkit}.png` | `$SCRATCH/t14/after/import_state_review__{chrome,webkit}.png` |

Both sets were captured with the **same** tool and settings (the before set was retaken with the
Estedad setup temporarily restored) so they are directly comparable.

Side-by-side sidebar crop, WebKit before vs after: `$SCRATCH/t14/work/evidence_nav.png` — every
above-dot restored (داسبورد→داشبورد, بطبیق‌ها→تطبیق‌ها, عملیاب‌ها→عملیات‌ها, سطیماب→تنظیمات).

Diagnostics kept: `diag_webkit.png` (11 font variants), `cand_{webkit,chrome}.png` (Mikhak/Vazirmatn
at four weights), `marks_webkit.png` (mark-class probe), `big_webkit.png` (90 px), `pango.png`
(non-WebKit control), `r2_{chrome,webkit}.png` (control types).

---

## 7. Files changed and commits

| Commit | Subject |
| --- | --- |
| `0ed0bd0` | Add a WebKitGTK snapshot script for design review |
| `cf64fdf` | Set the UI in Mikhak so WebKitGTK keeps the dots |
| `d0a43a2` | Point design review at WebKitGTK, not only Chrome |

- `scripts/wk-shot.py` — **new**
- `app/assets/css/main.css` — Estedad `@font-face` removed, `--font-display` / `--font-sans` → Mikhak,
  comment recording why (only the `@font-face` rules and the `--font-*` tokens were touched)
- `public/fonts/estedad-var.woff2` — **deleted**
- `README.md` — fonts note under «### Build»
- `docs/RUNBOOK.md` — «Design review without Tauri» gains the WebKit capture

Nothing under `src-tauri/src`, no IPC change, no component padding touched.

---

## 8. Self-review

- **R1** met — every Persian string keeps its dots in WebKitGTK at 400/500/600/700 (`cand_webkit.png`
  rows 2–5) and in Chrome; confirmed on all four app pages in both engines.
- **R2** met — WebKit's 5–6 px text/icon error is gone; the engines now agree to within 0.5 px on
  buttons (36/30/44), nav items, pills and fields. Control heights unchanged.
- **R3** met — one self-hosted OFL 1.1 woff2 under `public/fonts/`, nothing external.
- **R4** — Chrome stills keep the same sizes, weights and layout; only the typeface changes.
- **R5** met — typecheck, 32 tests, generate all green; `.output/public/fonts/` holds only
  `mikhak-var.woff2`, no `_fonts/`.
- **R6** met — before/after in both engines for all four pages, captured identically.
- Discipline: only the permitted files changed.

**Caught and fixed while working:** the font deletion first landed in the wk-shot commit; I
soft-reset and recommitted so each commit is coherent.

**Not done:** no `pnpm tauri dev` smoke run. The four pages were captured in webkit2gtk-4.1 itself,
which is the same engine and the same font stack the Tauri window uses, so it adds no new signal
about fonts — but the real window (title bar, CSP, packaged assets) was not exercised in this task.

## 9. Concerns

1. **The spec's type pairing is gone.** §2 pairs Mikhak (display) with Estedad (text); the UI is now
   one face at two weights. That is the controller's option 3 and it is what rendering correctness
   costs here, but it is a real change to the design and the user should see it before it is final.
   If a distinct text face is wanted, **Vazirmatn** is the ready alternative — it also renders
   correctly in WebKit (measured, `cand_webkit.png` rows 6–9), centres even better (0.0 px), has a
   wider cmap (811 vs 596), and was the project's own font before Task 1. It is one `@font-face` and
   one token away.
2. **`←` and `→` now come from `system-ui`.** Eleven visible occurrences. They render fine in both
   engines, but they are not Mikhak's shapes. Vazirmatn would not fix this — only Estedad had them.
3. **Mikhak's Latin is distinctive** (single-storey `a`, rounded forms) and the UI shows some Latin:
   `st_01`, `yedaki.markazi`, `BP-10422`. Legible, and visible in the after-stills, but worth the
   user's eye.
4. **The WebKit bug is upstream and unreported.** I could not narrow it past "WebKitGTK 2.52 drops
   GPOS-attached above-marks that came from a one-to-many GSUB substitution, while HarfBuzz and
   Pango handle the same font correctly". Any future Persian face must be checked with
   `scripts/wk-shot.py` before it ships — a Chrome still will not catch this.
5. **`/import?state=review` hydrates late in WebKit.** Unrelated to fonts (it reproduces with
   Estedad), and `wk-shot.py` now waits it out, but a real Tauri window may show the same brief
   flash of an unpopulated review list.

---

# Fix report — review findings (both Important, both valid)

## Finding 1 — `scripts/wk-shot.py`: the documented `--settle MS` form was silently dropped

**Confirmed.** The hand-rolled parser split `argv` into "starts with `--`" and "everything else",
so the value after a space-separated `--settle` went into the *positional* list. `flags.index("--settle")`
was therefore always the last element, `i + 1 < len(flags)` always false, and the value was discarded.
Worse, with `WIDTH`/`HEIGHT` omitted the stray number was consumed as `width` — a 500 px viewport
instead of the documented 1440. Only `--settle=500` worked, and the docstring documented the broken form.

This affected my own capture runs: `shots.sh` used `--settle 1500`, which never took effect (the
delay stayed at 1200 ms). It did not invalidate any still — the quiet-DOM wait is what actually makes
the captures deterministic, and `WIDTH HEIGHT` were always passed explicitly (`1200 800`), so no
still was taken at the wrong size. Re-confirmed by the geometry line now printed on every run.

**Fix:** replaced the parsing with `argparse` (`parse_args()`), which accepts both `--settle 500` and
`--settle=500`, type-checks the value, and gives the script a real `--help`. The script now also
prints its resolved configuration, so a still can be traced back to the invocation that made it.

### Covering checks (the script is outside vitest, so these are its test)

```
$ GDK_BACKEND=x11 DISPLAY=:0 python3 scripts/wk-shot.py "file://$T/page.html" $T/a.png --settle 500
wk-shot: 1440x900, settle 500ms, full-document, reduced-motion, quiet-DOM wait
wk-shot: wrote .../a.png (1440x900)
exit=0

$ GDK_BACKEND=x11 DISPLAY=:0 python3 scripts/wk-shot.py "file://$T/page.html" $T/b.png --settle=500
wk-shot: 1440x900, settle 500ms, full-document, reduced-motion, quiet-DOM wait
wk-shot: wrote .../b.png (1440x900)
exit=0

$ GDK_BACKEND=x11 DISPLAY=:0 python3 scripts/wk-shot.py "file://$T/page.html" $T/c.png 1200 800 --settle 500
wk-shot: 1200x800, settle 500ms, full-document, reduced-motion, quiet-DOM wait
wk-shot: wrote .../c.png (1200x800)
exit=0
```

Both forms parse to **500**, and with `WIDTH`/`HEIGHT` omitted the viewport is the documented
**1440x900**, not 500-wide. Explicit geometry still wins.

Failure paths:

```
$ ... wk-shot.py "file://$T/does-not-exist.html" $T/d.png --settle 300
wk-shot: load failed for file://.../does-not-exist.html: Error opening file ...: No such file or directory
exit=1

$ ... wk-shot.py "http://localhost:59999/nope" $T/e.png --settle 300
wk-shot: load failed for http://localhost:59999/nope: Could not connect to localhost: Connection refused
exit=1

$ python3 scripts/wk-shot.py                                    # missing required args
wk-shot.py: error: the following arguments are required: url, OUT.png        exit=2

$ python3 scripts/wk-shot.py URL OUT.png --settle abc            # non-numeric
wk-shot.py: error: argument --settle: invalid int value: 'abc'               exit=2
  (no PNG written)

$ python3 scripts/wk-shot.py --help                                          exit=0
```

End-to-end regression — the tool still captures a real page:

```
$ ... wk-shot.py "file://$SCRATCH/t14/work/r2.html" regress.png 700 900 --settle 800
wk-shot: 700x900, settle 800ms, full-document, reduced-motion, quiet-DOM wait
wk-shot: wrote .../regress.png (700x900)          size (700, 900) inked 71738
```

Test page: `$SCRATCH/t14/argtest/page.html`; outputs in `$SCRATCH/t14/argtest/`.

## Finding 2 — `docs/RUNBOOK.md`: false claim that PyGObject is already a build dependency

**Confirmed.** The line said the script "needs `python-gobject` and `webkit2gtk-4.1` (both already
required to build the app)". Only the second is true: `webkit2gtk-4.1` / `libwebkit2gtk-4.1-dev`
appears in the README's three prerequisite tables and the runbook's own, but PyGObject appears in
none of them, and nothing else in the repo imports `gi`. It is an extra this script introduced, so
someone following the runbook on a clean machine would have hit an `ImportError` with nothing to go on.

**Fix:** the sentence now says plainly that the Python bindings are an extra this script alone wants
and are not in the build prerequisites, with the package named per distro and a one-line check:

| | |
| --- | --- |
| Arch / Manjaro | `sudo pacman -S --needed python-gobject` |
| Debian / Ubuntu | `sudo apt install python3-gi gir1.2-webkit2-4.1` |
| Fedora | `sudo dnf install python3-gobject webkit2gtk4.1` |

`python3 -c "import gi; gi.require_version('WebKit2','4.1')"` as the check. Package names verified on
this machine: `/usr/lib/python3.14/site-packages/gi/__init__.py` is owned by `python-gobject 3.56.3-1`
and `/usr/lib/girepository-1.0/WebKit2-4.1.typelib` by `webkit2gtk-4.1 2.52.6-1`.

**The build prerequisite tables were not touched** — confirmed by grepping the diff for any
Rust/Node/pnpm/`libwebkit2gtk-4.1-dev`/`pacman -Syu`/`dnf group` line: none.

## Verification for this pass

```
pnpm typecheck  -> exit 0
pnpm test       -> exit 0    Test Files 7 passed (7),  Tests 32 passed (32)
```

(`pnpm generate` was not re-run: this pass changed only `scripts/wk-shot.py` and `docs/RUNBOOK.md`,
neither of which the build consumes. The generate result from the main pass stands — `.output/public/fonts/`
holds only `mikhak-var.woff2`, no `_fonts/`.)

## Commits

| Commit | Subject |
| --- | --- |
| `12e085c` | Parse wk-shot's arguments with argparse |
| `dfcaf26` | Say plainly that wk-shot needs PyGObject installed |

No font, CSS or app file changed in this pass; the font fix from the main pass is untouched.
