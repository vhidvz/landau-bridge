#!/usr/bin/env node
// Build the evidence sets of Landau plan 130 task 5 (`src-tauri/fixtures/evidence/`) from three local sources:
//
//   --registry   Landau's registry as TSV (id, name, alias, brand), e.g. from the stack's Mongo:
//                docker exec <mongo> mongosh --quiet -u … -p … --authenticationDatabase … --eval '
//                  const c = db.getSiblingDB("wnx-career").products, t = (s) => (s == null ? "" : String(s)).replace(/[\t\r\n]+/g, " ");
//                  c.find({ deleted_at: { $exists: false } }, { name: 1, alias: 1, brand: 1 })
//                   .forEach((p) => print([p._id.toString(), t(p.name), t(p.alias), t(p.brand)].join("\t")))'
//   --parts-map  lnd-parts' `output/parts_map.csv` (the catalog export: evaluation labels only, never read at runtime).
//   --seller-db  a copy of a seller's `bridge.sqlite`, migrated by this build (`landau-bridge --self-check <dir>`).
//                Never the live file: open it read-only from a `.backup` copy.
//
// Writes registry.tsv.gz, seller-items.tsv, seller-links.tsv, validation.tsv and hard-cases.tsv into --out. Names and
// codes only: no price, quantity, store, invoice, account or key. Every label is an id of the registry snapshot, so a
// new snapshot means building every set again. The draws are seeded (FNV-1a over seed and code), so the same inputs
// give the same files; the labels already written into hard-cases.tsv (agent, user, note) are kept by pair id.
//
// usage: node scripts/evidence-sets.mjs --registry r.tsv --parts-map parts_map.csv --seller-db copy.sqlite \
//          --out src-tauri/fixtures/evidence [--seed 20261109] [--validation 2000] [--per-category 26]
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith('--') ? [...acc, [a.slice(2), all[i + 1]]] : acc), []),
);
for (const k of ['registry', 'parts-map', 'seller-db', 'out']) {
  if (!args[k]) throw new Error(`missing --${k}`);
}
const SEED = args.seed ?? '20261109';
const VALIDATION = Number(args.validation ?? 2000);
const PER_CATEGORY = Number(args['per-category'] ?? 26);

// --- reading -----------------------------------------------------------------------------------------------------

/** RFC 4180 CSV (quoted fields, doubled quotes, a leading BOM) → array of objects keyed by the header. */
export function parseCsv(text) {
  const rows = [];
  let row = [];
  let field = '';
  let quoted = false;
  const s = text.replace(/^﻿/, '');
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quoted) {
      if (c === '"' && s[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') {
      row.push(field);
      field = '';
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && s[i + 1] === '\n') i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else field += c;
  }
  if (field !== '' || row.length) {
    row.push(field);
    rows.push(row);
  }
  const [head, ...body] = rows.filter((r) => r.length > 1 || r[0] !== '');
  return body.map((r) => Object.fromEntries(head.map((h, i) => [h, r[i] ?? ''])));
}

const clean = (s) => (s ?? '').replace(/[\t\r\n]+/g, ' ').trim();

const registry = readFileSync(args.registry, 'utf8')
  .split('\n')
  .filter((l) => l.trim() !== '')
  .map((l) => {
    const [id, name, alias, brand] = l.split('\t');
    return { id, name: clean(name), alias: clean(alias), brand: clean(brand) };
  })
  .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
const byId = new Map(registry.map((p) => [p.id, p]));
if (byId.size !== registry.length) throw new Error('duplicate ids in the registry export');

const partsMap = parseCsv(readFileSync(args['parts-map'], 'utf8'));

const { DatabaseSync } = await import('node:sqlite');
const db = new DatabaseSync(args['seller-db'], { readOnly: true });
const version = db.prepare('PRAGMA user_version').get().user_version;
if (version < 4) throw new Error(`the seller copy is at schema ${version}; migrate it with --self-check first`);
// every code the seller's jobs saw, with the name of its latest item
const sellerItems = db
  .prepare(
    `SELECT code, name FROM job_items WHERE id IN (SELECT MAX(id) FROM job_items GROUP BY code) ORDER BY code`,
  )
  .all();
const sellerLinks = db
  .prepare(
    `SELECT name_key, name_seen, code, how, product_id, product_name, product_brand, product_alias
       FROM supplier_links ORDER BY name_key`,
  )
  .all();
db.close();

// --- shared helpers ----------------------------------------------------------------------------------------------

/** FNV-1a, 64-bit: the seeded order of a draw. */
function fnv(s) {
  let h = 0xcbf29ce484222325n;
  for (const b of Buffer.from(s, 'utf8')) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return h;
}
const seeded = (rows, salt) =>
  rows
    .map((r) => [fnv(`${SEED}:${salt}:${r.code}`), r])
    .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    .map(([, r]) => r);

// A label resolves by the export's product name and brand, exact after trimming, as slice 1 resolved it. The registry
// spells three of the export's Latin brands in Persian; measured 2026-11-09, they are every label that did not
// resolve (5,427 of 54,515 codes).
const REGISTRY_SPELLING = { AMATA: 'آماتا', ELMA7: 'الما۷', DOOSTAN: 'دوستان' };
const byNameBrand = new Map();
for (const p of registry) {
  const k = `${p.name}\u0001${p.brand}`;
  byNameBrand.set(k, [...(byNameBrand.get(k) ?? []), p.id]);
}
function resolve(row) {
  if (!row) return { status: 'absent', id: '' };
  const brand = row.brand.trim();
  const ids = byNameBrand.get(`${row.product_name.trim()}\u0001${REGISTRY_SPELLING[brand] ?? brand}`) ?? [];
  if (ids.length === 1) return { status: 'resolved', id: ids[0] };
  return { status: ids.length ? 'ambiguous' : 'unresolved', id: '' };
}
const mapByCode = new Map(partsMap.map((r) => [r.code, r]));

// Tokens for choosing hard cases only (the gate's own reading is the Rust code's, never this one).
const DIGITS = { '۰': 0, '۱': 1, '۲': 2, '۳': 3, '۴': 4, '۵': 5, '۶': 6, '۷': 7, '۸': 8, '۹': 9 };
function norm(s) {
  return s
    .replace(/ي/g, 'ی')
    .replace(/ك/g, 'ک')
    .replace(/[ةۀ]/g, 'ه')
    .replace(/[أإ]/g, 'ا')
    .replace(/[۰-۹]/g, (d) => String(DIGITS[d]))
    .replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660))
    .replace(/‌/g, ' ')
    .toLowerCase();
}
const tokens = (s) =>
  norm(s)
    .split(/[^\p{L}\p{N}.]+/u)
    .map((t) => t.replace(/^\.+|\.+$/g, ''))
    .filter(Boolean);
/** The supplier name without carton counts «(N)», «N عددی» and a trailing copy of its own code. */
function supplierTokens(name, code) {
  const t = tokens(norm(name).replace(/\(\s*\d+\s*(عددی)?\s*\)/g, ' ').replace(/\d+\s*عددی/g, ' '));
  if (t.length && t[t.length - 1] === norm(code)) t.pop();
  return t;
}
const hasDigit = (ts) => ts.some((t) => /\d/.test(t));
const jaccard = (a, b) => {
  const A = new Set(a);
  const B = new Set(b);
  let inter = 0;
  for (const x of A) if (B.has(x)) inter++;
  return inter / (A.size + B.size - inter || 1);
};
const sameSet = (a, b) => {
  const A = new Set(a);
  const B = new Set(b);
  return A.size === B.size && [...A].every((x) => B.has(x));
};
const isLatin = (s) => /[a-z]/i.test(s) && !/[؀-ۿ]/.test(s);

const products = registry.map((p) => ({ ...p, toks: tokens(p.name) }));
const productById = new Map(products.map((p) => [p.id, p]));
const brandOf = new Map();
for (const p of products) {
  if (!p.brand) continue;
  brandOf.set(p.brand, [...(brandOf.get(p.brand) ?? []), p]);
}
const productsByName = new Map();
for (const p of products) productsByName.set(p.name, [...(productsByName.get(p.name) ?? []), p]);
// A brand that is also a word in ≥ 3 names of other brands' products («بوش», the bushing) never counts as a brand
// here: the two-brand category wants two makers, not a maker and a part word.
const wordIn = new Map();
for (const p of products) for (const t of new Set(p.toks)) wordIn.set(t, [...(wordIn.get(t) ?? []), p.brand]);
const brands = [...brandOf.keys()]
  .map((b) => ({ b, toks: tokens(b) }))
  .filter(({ b, toks }) => toks.length && !(toks.length === 1 && (wordIn.get(toks[0]) ?? []).filter((x) => x !== b).length >= 3));
function brandsIn(ts) {
  const found = brands.filter(({ toks }) => ts.some((_, i) => toks.every((t, j) => ts[i + j] === t)));
  // a brand inside a longer one found too («عظام» in «PSN عظام») is the same maker
  return found.filter(({ toks }) => !found.some((o) => o.toks.length > toks.length && toks.every((t) => o.toks.includes(t))));
}

const OPPOSITE = { چپ: 'راست', راست: 'چپ', جلو: 'عقب', عقب: 'جلو', بالا: 'پایین', پایین: 'بالا', داخلی: 'بیرونی', بیرونی: 'داخلی' };

// --- the sets ----------------------------------------------------------------------------------------------------

const sellerCodes = new Set(sellerItems.map((r) => r.code));
const items = sellerItems.map((r) => {
  const { status, id } = resolve(mapByCode.get(r.code));
  return { code: r.code, name: clean(r.name), export_id: id, export_status: status };
});

const validation = [];
for (const row of seeded(partsMap.filter((r) => !sellerCodes.has(r.code)), 'validation')) {
  if (validation.length >= VALIDATION) break;
  const { status, id } = resolve(row);
  if (status === 'resolved') validation.push({ code: row.code, name: clean(row.raw_name), label_id: id });
}
const taken = new Set([...sellerCodes, ...validation.map((v) => v.code)]);

/** The best same-brand alternative to `label` for supplier tokens `st`, by token overlap, at least `floor`. */
function bestSibling(st, label, pool, floor) {
  let best = null;
  let score = floor;
  for (const p of pool) {
    if (p.id === label.id) continue;
    const j = jaccard(st, p.toks);
    if (j > score || (j === score && best && p.id < best.id)) {
      best = p;
      score = j;
    }
  }
  return best;
}

const CATEGORIES = {
  'two-brands': (st, label) => {
    const found = brandsIn(st);
    if (found.length < 2) return null;
    const other = found.find(({ b }) => b !== label.brand);
    if (!other) return null;
    const twin = (productsByName.get(label.name) ?? []).find((p) => p.brand === other.b);
    return twin ?? bestSibling(st, label, brandOf.get(other.b) ?? [], 0.5);
  },
  'brand-script': (st, label) => {
    if (!label.brand) return null;
    const twin = (productsByName.get(label.name) ?? []).find(
      (p) => p.id !== label.id && p.brand && isLatin(p.brand) !== isLatin(label.brand),
    );
    return twin ?? null;
  },
  sides: (st, label) => {
    const side = st.find((t) => OPPOSITE[t] && label.toks.includes(t));
    if (!side) return null;
    const want = label.toks.map((t) => (t === side ? OPPOSITE[side] : t));
    return (brandOf.get(label.brand) ?? []).find((p) => p.id !== label.id && sameSet(p.toks, want)) ?? null;
  },
  dimensions: (st, label) => {
    if (!hasDigit(st)) return null;
    const words = (ts) => ts.filter((t) => !/\d/.test(t));
    const nums = (ts) => ts.filter((t) => /\d/.test(t));
    const pool = (brandOf.get(label.brand) ?? []).filter(
      (p) => p.id !== label.id && sameSet(words(p.toks), words(label.toks)) && !sameSet(nums(p.toks), nums(label.toks)),
    );
    return bestSibling(st, label, pool, 0);
  },
  numberless: (st, label) => (hasDigit(st) ? null : bestSibling(st, label, brandOf.get(label.brand) ?? [], 0.4)),
  'same-brand': (st, label) => bestSibling(st, label, brandOf.get(label.brand) ?? [], 0.4),
  // Checked last, so adding it moved no earlier pair: the supplier writes the label's maker in the other
  // script («ELMA7» for «الما۷», «HAMED» for «حامد»), the pair an equivalence table must recognise; its
  // alternative is the same name under another maker.
  'brand-spelling': (st, label) => {
    if (!label.brand || isLatin(label.brand)) return null;
    const own = tokens(label.brand);
    if (own.every((t) => st.includes(t))) return null;
    const named = new Set([...label.toks, ...tokens(label.alias)]);
    if (!st.some((t) => /^[a-z][a-z0-9]{2,}$/.test(t) && /[a-z]{3}/.test(t) && !named.has(t))) return null;
    return (productsByName.get(label.name) ?? []).find((p) => p.id !== label.id && p.brand && p.brand !== label.brand) ?? null;
  },
};

const kept = new Map();
const hardPath = join(args.out, 'hard-cases.tsv');
if (existsSync(hardPath)) {
  for (const r of readTsv(readFileSync(hardPath, 'utf8'))) kept.set(r.id, r);
}
const counts = Object.fromEntries(Object.keys(CATEGORIES).map((c) => [c, 0]));
const hard = [];
for (const row of seeded(partsMap.filter((r) => !taken.has(r.code)), 'hard')) {
  if (Object.values(counts).every((n) => n >= PER_CATEGORY)) break;
  const { status, id } = resolve(row);
  if (status !== 'resolved') continue;
  const label = productById.get(id);
  const st = supplierTokens(row.raw_name, row.code);
  for (const [category, pick] of Object.entries(CATEGORIES)) {
    if (counts[category] >= PER_CATEGORY) continue;
    const alt = pick(st, label);
    if (!alt) continue;
    counts[category]++;
    for (const [role, c] of [['label', label], ['alternative', alt]]) {
      const pid = `${row.code}>${c.id}`;
      const old = kept.get(pid) ?? {};
      hard.push({
        id: pid,
        category,
        role,
        code: row.code,
        name: clean(row.raw_name),
        candidate_id: c.id,
        candidate: c.name,
        brand: c.brand,
        alias: c.alias,
        agent: old.agent ?? '',
        user: old.user ?? '',
        note: old.note ?? '',
      });
    }
    break;
  }
}

// --- writing -----------------------------------------------------------------------------------------------------

function readTsv(text) {
  const [head, ...rows] = text.split('\n').filter((l) => l !== '' && !l.startsWith('#'));
  const h = head.split('\t');
  return rows.map((l) => Object.fromEntries(l.split('\t').map((v, i) => [h[i], v])));
}
const NULL = '\\N';
function tsv(header, rows, comment) {
  const cell = (v) => (v === null || v === undefined ? NULL : clean(String(v)));
  return [...comment.map((c) => `# ${c}`), header.join('\t'), ...rows.map((r) => header.map((h) => cell(r[h])).join('\t'))].join('\n') + '\n';
}

writeFileSync(
  join(args.out, 'registry.tsv.gz'),
  gzipSync(tsv(['id', 'name', 'alias', 'brand'], registry, [`Landau's registry, ${registry.length} products`]), { level: 9 }),
);
writeFileSync(
  join(args.out, 'seller-items.tsv'),
  tsv(['code', 'name', 'export_id', 'export_status'], items, [
    `Every code a seller's jobs saw (${items.length}), with the name of its latest item.`,
    "export_*: the code's product in lnd-parts' map, resolved by name and brand to one registry id (task 15).",
  ]),
);
writeFileSync(
  join(args.out, 'seller-links.tsv'),
  tsv(['name_key', 'name_seen', 'code', 'how', 'product_id', 'product_name', 'product_brand', 'product_alias'], sellerLinks, [
    `The seller's links as migration 0004 leaves them (${sellerLinks.length}); \\N = never recorded, empty = recorded as none.`,
  ]),
);
writeFileSync(
  join(args.out, 'validation.tsv'),
  tsv(['code', 'name', 'label_id'], validation, [
    `${validation.length} export codes outside the seller's, seed ${SEED}, each label resolving to one registry id.`,
  ]),
);
writeFileSync(
  hardPath,
  tsv(['id', 'category', 'role', 'code', 'name', 'candidate_id', 'candidate', 'brand', 'alias', 'agent', 'user', 'note'], hard, [
    `Hard cases, seed ${SEED}, ${PER_CATEGORY} codes per category: the export's label and a hard alternative each.`,
    'agent / user: SAME (the candidate is the part the supplier names, maker included), DIFFERENT, or UNSURE.',
  ]),
);
console.log(
  JSON.stringify({
    registry: registry.length,
    seller: { items: items.length, links: sellerLinks.length, export: tally(items.map((i) => i.export_status)) },
    validation: validation.length,
    hard: { pairs: hard.length, per: counts, labelled: hard.filter((h) => h.agent).length },
  }),
);
function tally(xs) {
  return xs.reduce((m, x) => ({ ...m, [x]: (m[x] ?? 0) + 1 }), {});
}
