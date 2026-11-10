// Fixtures for `pnpm dev` in a plain browser (no Tauri). Never imported by the production build:
// useBridge.ts only reaches for this file behind `import.meta.dev`.
import type {
  CandidateView, DecisionImportReport, Dashboard, ErrorRow, HeldWrite, Inspection, JobDetail, JobEventRow, JobItemRow, JobRow, LandauStatus,
  MatchRow, Reason, Scored, SettingsView, UndecidedRow, UpdateView, Audit,
} from '~/types/bridge';
import type { JobHandlers } from '~/composables/useBridge';

type State = 'empty' | 'running' | 'review' | 'refreshing' | 'done';
export const devState = (): State => {
  const s = new URLSearchParams(location.search).get('state');
  return s === 'empty' || s === 'running' || s === 'review' || s === 'refreshing' ? s : 'done';
};

// `?held=1`: one shelf write held for the seller, so the dashboard shows its card.
const held = typeof location !== 'undefined' && new URLSearchParams(location.search).has('held');

// `?bulk=1`: lists long enough to page through — 130 decisions, 120 undecided parts, 45 in review,
// a 412-row report, 42 jobs and 126 errors.
const bulk = typeof location !== 'undefined' && new URLSearchParams(location.search).has('bulk');

const ago = (days: number, hour = 10, minute = 20): string => {
  const d = new Date();
  d.setDate(d.getDate() - days);
  d.setHours(hour, minute, 0, 0);
  return d.toISOString();
};

const settings: SettingsView = {
  landau_url: 'https://api.landau.ir', has_landau_token: true, store: 'st_01', store_name: 'یدکی مرکزی',
  supplier_user: 'yadaki.markazi', has_supplier_password: true, default_authenticity: 'ORIGINAL', threshold: 85,
  vat_percent: 10, min_margin_percent: 15, overwrite_manual: false, fetch_since_only: true, secret_backend: 'keyring', data_dir: '/home/vahid/.local/share/ir.landau.bridge',
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
  job(41, 'REFRESH', 'DONE', 'REPORT', 1, { rows: 412, linked: 400, patched: 96, unchanged: 290, repriced: 96, skipped: 2, held: 12, failed: 0 }),
  job(40, 'IMPORT', 'FAILED', 'FETCH', 3),
  job(39, 'IMPORT', 'DONE', 'REPORT', 9, doneSummary),
  job(38, 'REFRESH', 'CANCELLED', 'WRITE', 12),
  job(37, 'IMPORT', 'DONE', 'REPORT', 20, doneSummary),
  ...(bulk ? Array.from({ length: 36 }, (_, i) => job(36 - i, i % 3 ? 'IMPORT' : 'REFRESH', i % 11 ? 'DONE' : 'FAILED', i % 11 ? 'REPORT' : 'FETCH', 21 + i, i % 11 ? doneSummary : null)) : []),
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
  error_kind: null, error_msg: null, candidates: null, status: 'DONE', invoice_ids: [5121], available: null, note: null, ...extra,
});

const reviewItems: JobItemRow[] = [
  ...(bulk
    ? Array.from({ length: 42 }, (_, i) => item(1000 + i, `BP-5${String(i).padStart(4, '0')}`, `قلم بازبینی ${i + 1}`, i + 1, { status: 'REVIEW', candidates: Array.from({ length: i % 2 ? 10 : 1 }, (_, k) => scored(`p_r${i}_${k}`, `پیشنهاد ${k + 1} برای قلم ${i + 1}`, k % 2 ? 'برند' : null, 90 - k * 4)) }))
    : []),
  item(901, 'BP-10422', 'لنت ترمز جلو پراید', 12, { status: 'REVIEW', candidates: [
    scored('p_1', 'لنت ترمز جلو پراید ۱۳۱', 'فرانتک', 91), scored('p_2', 'لنت ترمز عقب پراید', 'فرانتک', 74), scored('p_3', 'لنت ترمز جلو تیبا', 'پارس', 62),
  ] }),
  item(902, 'BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 30, { status: 'REVIEW', candidates: [
    scored('p_4', 'فیلتر روغن پژو ۲۰۶', 'سرکان', 88, 'W712/8'), scored('p_5', 'فیلتر هوا پژو ۲۰۶', 'سرکان', 57),
  ] }),
  item(903, 'BP-33005', 'شمع NGK BKR6E', 48, { status: 'REVIEW', candidates: [] }),
  // the incident (plan 130): the 69349 bearing ranked first for the 44649 name
  item(904, '33102018', 'بلبرينگ بزرگ چرخ عقب پرايد جدبد (بلبرينگ بزرگ چرخ جلو پيکان) 44649 PSN عظام (96) 3154028', 0, { status: 'REVIEW', candidates: [
    scored('p_69349', 'بلبرینگ چرخ جلو قدیم پراید', 'PSN عظام', 67, 'بلبرینگ 69349'), scored('p_44649', 'بلبرینگ بزرگ چرخ عقب پراید و چرخ جلو پیکان', 'PSN عظام', 56, 'بلبرینگ 44649 | بلبرینگ چرخ پراید عظام'),
  ] }),
];

const doneItems: JobItemRow[] = [
  item(1, 'BP-10422', 'لنت ترمز جلو پراید', 12, { product_id: 'p_1', product_name: 'لنت ترمز جلو پراید ۱۳۱', action: 'ATTACH', qty_after: 12, cost: 1_430_000, price_after: 1_780_000, reprice_verdict: 'REPRICED:1780000', available: true }),
  item(2, 'BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 30, { product_id: 'p_4', product_name: 'فیلتر روغن پژو ۲۰۶', action: 'PATCH', qty_before: 8, qty_after: 38, cost: 385_000, price_before: 450_000, price_after: 480_000, reprice_verdict: 'REPRICED:480000' }),
  item(3, 'BP-33005', 'شمع NGK BKR6E', 48, { product_id: 'p_9', product_name: 'شمع NGK BKR6E', action: 'PATCH', qty_before: 20, qty_after: 68, cost: 1_210_000, price_before: 1_500_000, price_after: 1_500_000, reprice_verdict: 'SKIPPED:MANUAL', available: false, note: 'ناموجود؛ قیمت پایین‌تر نادیده گرفته شد؛ زیر حداقل سود' }),
  item(4, 'BP-44120', 'تسمه تایم ۴۰۵', 6, { status: 'FAILED', error_kind: 'NoCurrentPrice', error_msg: 'بارزپخش برای این کد قیمت روز ندارد' }),
  item(5, 'BP-51002', 'دیسک ترمز جلو سمند', 4, { product_id: 'p_12', product_name: 'دیسک ترمز جلو سمند', action: 'ATTACH', qty_after: 4, cost: 2_960_000, price_after: 3_650_000, reprice_verdict: 'REPRICED:3650000' }),
  item(6, 'BP-60233', 'روغن موتور ۴ لیتری بهران', 24, { status: 'SKIPPED', error_kind: 'NoneDecision', error_msg: 'در فهرست لاندا نیست' }),
  item(7, 'BP-71850', 'واشر سرسیلندر ۲۰۶', 10, { product_id: 'p_15', product_name: 'واشر سرسیلندر پژو ۲۰۶', action: 'ATTACH', qty_after: 10, cost: 940_000, price_after: 1_170_000, reprice_verdict: 'REPRICED:1170000' }),
  item(8, 'BP-80017', 'پمپ بنزین پراید', 5, { product_id: 'p_16', product_name: 'پمپ بنزین پراید', action: 'PATCH', qty_before: 2, qty_after: 7, cost: 1_650_000, price_before: 2_050_000, price_after: 2_050_000, reprice_verdict: 'UNCHANGED', available: true, note: 'کد بارزپخش عوض شد: BP-80016 ← BP-80017' }),
];

if (bulk) {
  for (let i = 0; i < 404; i++) {
    const cost = 200_000 + ((i * 7919) % 90) * 25_000;
    const before = Math.round(cost * 1.2 / 1000) * 1000;
    const after = i % 4 ? before : before + 30_000;
    doneItems.push(item(100 + i, `BP-8${String(i).padStart(4, '0')}`, `قطعهٔ گزارش شمارهٔ ${i + 1}`, 1 + (i % 9), i % 37 === 5
      ? { status: 'FAILED', error_kind: 'NoCurrentPrice', error_msg: 'بارزپخش برای این کد قیمت روز ندارد' }
      : { product_id: `p_g${i}`, product_name: `قطعهٔ لاندا شمارهٔ ${i + 1}`, action: 'PATCH', qty_before: i % 9, qty_after: i % 9, cost, price_before: before, price_after: after, reprice_verdict: after === before ? 'UNCHANGED' : `REPRICED:${after}`, available: i % 13 ? true : false, note: i % 13 ? null : 'ناموجود؛ قیمت پایین‌تر نادیده گرفته شد' }));
  }
}

const ev = (id: number, level: JobEventRow['level'], phase: string, message: string, minutesAgo: number): JobEventRow =>
  ({ id, at: new Date(Date.now() - minutesAgo * 60_000).toISOString(), level, phase, message, detail: null });
const doneEvents: JobEventRow[] = [
  ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 40),
  ev(2, 'INFO', 'FETCH', '۱۲۴ فاکتور خوانده شد؛ ۳ فاکتور پرداخت‌نشده رد شد', 37),
  ev(3, 'INFO', 'AGGREGATE', '۲۱۷ قلم در ۸۹ کد جمع‌بندی شد', 36),
  ev(4, 'INFO', 'MATCH', '۷۱ کد خودکار تطبیق داده شد، ۱۸ کد به تصمیم شما نیاز داشت', 35),
  ev(5, 'WARN', 'PRICE', 'برای BP-44120 قیمت روز پیدا نشد', 30),
  ev(6, 'INFO', 'WRITE', '۸۱ ردیف روی قفسه نوشته شد', 26),
  ev(7, 'INFO', 'REPRICE', 'قواعد قیمت‌گذاری لاندا روی ۴۱۲ ردیف اجرا شد؛ ۹۶ قیمت تغییر کرد', 24),
  ev(8, 'INFO', 'REPORT', JSON.stringify(doneSummary), 23),
];
const runningEvents: JobEventRow[] = doneEvents.slice(0, 3);

// Parts ended jobs left undecided. Stateful within a page load, so deciding one in the browser
// removes it from «بدون تصمیم» as the real list does.
const undecidedPart = (code: string, name_seen: string, times: number, candidates: Scored[]): UndecidedRow =>
  ({ name_key: name_seen, name_seen, code, job_id: 41, seen_at: ago(2, 9, 30), qty: 0, times, candidates });
const undecided: UndecidedRow[] = [
  undecidedPart('BP-90011', 'کمک فنر جلو پراید', 2, [
    scored('p_u1', 'کمک فنر جلو پراید گازی', 'ایساکو', 79), scored('p_u2', 'کمک فنر جلو پراید روغنی', 'ایساکو', 79), scored('p_u3', 'کمک فنر عقب پراید', 'ایساکو', 61),
  ]),
  undecidedPart('BP-90012', 'بوش طبق پژو ۴۰۵', 1, [scored('p_u4', 'بوش طبق پژو ۴۰۵ بزرگ', 'یدک‌سازان', 72), scored('p_u5', 'بوش طبق پژو ۴۰۵ کوچک', 'یدک‌سازان', 70)]),
  undecidedPart('BP-90013', 'رادیاتور آب سمند EF7', 1, [scored('p_u6', 'رادیاتور سمند', 'رادیسا', 66)]),
  undecidedPart('BP-90014', 'پیستون ۲۰۶ تیپ ۲ استاندارد', 3, []),
];
if (bulk) {
  for (let i = 1; i <= 116; i++) {
    undecided.push(undecidedPart(`BP-7${String(i).padStart(4, '0')}`, `قطعهٔ آزمایشی شمارهٔ ${i}`, 1 + (i % 3), [
      scored(`p_b${i}a`, `پیشنهاد اول برای ${i}`, 'آزمایش', 70 + (i % 9)), scored(`p_b${i}b`, `پیشنهاد دوم برای ${i}`, null, 60),
    ]));
  }
}
const decide = (name_seen: unknown) => {
  const i = undecided.findIndex((u) => u.name_seen === name_seen);
  if (i >= 0) undecided.splice(i, 1);
};
const importReport: DecisionImportReport = {
  total: 1240, added: 1102, unchanged: 96, replaced: 21, invalid: 2,
  kept: [{ name_seen: 'تسمه تایم ۴۰۵', code: 'BP-44120', mine_how: 'USER', mine_product: 'تسمه تایم پژو ۴۰۵', file_how: 'USER', file_product: 'تسمه تایم پژو ۴۰۵ دنا' }],
  missing: [{ name_seen: 'فیلتر بنزین قدیمی', code: 'BP-12001', product_id: 'p_gone', product_name: 'فیلتر بنزین پراید (حذف‌شده)' }],
};

const match = (code: string, name_seen: string, how: MatchRow['how'], product_name: string | null, similarity: number | null, days: number, state: MatchRow['state'] = 'VALID', state_reason: string | null = null): MatchRow =>
  ({ supplier: 'barezpakhsh', name_key: name_seen.toLowerCase(), code, product_id: product_name ? `p_${code}` : null, product_name, how, similarity, name_seen, priced_at: how === 'NONE' ? null : ago(days, 15, 0), decided_at: ago(days, 14, 5),
    state, state_reason, state_at: ago(days, 15, 0), validated_job: state === 'VALID' ? 42 : null, link_version: 1, product_brand: null, product_alias: null, confirmed_json: null, renamed_json: null });
const matches: MatchRow[] = [
  ...(bulk ? Array.from({ length: 121 }, (_, i) => match(`BP-6${String(i).padStart(4, '0')}`, `تطبیق آزمایشی ${i + 1}`, 'AUTO', `قطعهٔ لاندا ${i + 1}`, 85, 2)) : []),
  match('BP-10422', 'لنت ترمز جلو پراید', 'USER', 'لنت ترمز جلو پراید ۱۳۱', 91, 0),
  match('BP-20871', 'فیلتر روغن ۲۰۶ تیپ ۵', 'AUTO', 'فیلتر روغن پژو ۲۰۶', 88, 0),
  match('BP-33005', 'شمع NGK BKR6E', 'AUTO', 'شمع NGK BKR6E', 97, 1),
  match('BP-44120', 'تسمه تایم ۴۰۵', 'AUTO', 'تسمه تایم پژو ۴۰۵', 84, 1, 'NEEDS_REVIEW', JSON.stringify([{ kind: 'BELOW_THRESHOLD', similarity: 84, threshold: 85 }, { kind: 'NO_BRAND_IN_NAME' }])),
  match('BP-51002', 'دیسک ترمز جلو سمند', 'AUTO', 'دیسک ترمز جلو سمند', 95, 3),
  match('BP-60233', 'روغن موتور ۴ لیتری بهران', 'NONE', null, null, 3),
  match('BP-70110', 'سیبک فرمان چپ پژو 405 امیرنیا', 'AUTO', 'سیبک فرمان چپ پژو ۴۰۵', null, 0),
  match('BP-71850', 'واشر سرسیلندر ۲۰۶', 'USER', 'واشر سرسیلندر پژو ۲۰۶', 78, 9),
  match('BP-80017', 'پمپ بنزین پراید', 'AUTO', 'پمپ بنزین پراید', 100, 9),
  match('BP-90311', 'کمک فنر عقب تیبا', 'AUTO', 'کمک فنر عقب تیبا', 93, 12),
  match('BP-90420', 'بلبرینگ چرخ جلو ۲۰۶', 'USER', 'بلبرینگ چرخ جلو پژو ۲۰۶', 71, 12, 'STALE', JSON.stringify({ why: '2 products carry its name, brand and alias', reasons: [{ kind: 'DEFINITION_NOT_EXACT' }] })),
  match('BP-91002', 'رادیاتور آب دنا', 'NONE', null, null, 20),
  match('BP-91777', 'سنسور اکسیژن سمند EF7', 'AUTO', 'سنسور اکسیژن EF7', 86, 20),
];
// PI-28: the site now lists BP-70110 under the right-hand part, which conflicts with the left one
const RENAME_REASONS: Reason[] = [{ kind: 'OPPOSITE_WORDS', supplier: 'راست', candidate: 'چپ' }];
const renamedLink = matches.find((m) => m.code === 'BP-70110')!;
renamedLink.renamed_json = JSON.stringify({
  product_id: renamedLink.product_id, code: 'BP-70110', name: 'سیبک فرمان راست پژو 405 امیرنیا', reasons: RENAME_REASONS, seen_at: ago(0, 9, 0), confirmed_at: null,
});
const confirmRename = (confirmed: unknown) => {
  if (JSON.stringify(confirmed ?? null) !== JSON.stringify(RENAME_REASONS)) {
    throw { kind: 'ConfirmRequired', message: 'نام بارزپخش با این قطعه جور نیست؛ ناسازگاری‌ها و تغییر قیمت خرید را ببینید و دوباره تأیید کنید.', detail: JSON.stringify(RENAME_REASONS) };
  }
  renamedLink.renamed_json = JSON.stringify({ ...JSON.parse(renamedLink.renamed_json!), confirmed_at: new Date().toISOString() });
  return true;
};

const err = (id: number, days: number, job_id: number | null, kind: string, message: string, context: unknown = null): ErrorRow =>
  ({ id, at: ago(days, 9, 12), job_id, kind, message, context });
const errors: ErrorRow[] = [
  err(61, 0, 42, 'NoCurrentPrice', 'بارزپخش برای BP-44120 قیمت روز ندارد', { code: 'BP-44120' }),
  err(60, 0, 42, 'NoneDecision', 'BP-60233 در فهرست لاندا نیست؛ برای افزودن آن تیکت ثبت کنید'),
  err(59, 1, 41, 'RepriceSkipped', '۱۲ ردیف با قیمت دستی دست نخورد', { skipped: 'MANUAL', rows: 12 }),
  err(58, 3, 40, 'SupplierUnreachable', 'پاسخی از بارزپخش نیامد (timeout after 30s)', { url: 'https://barezpakhsh.ir/invoices' }),
  err(57, 5, null, 'LandauUnauthorized', 'کلید API لاندا پذیرفته نشد'),
  err(56, 12, 38, 'JobCancelled', 'به درخواست شما متوقف شد'),
  ...(bulk ? Array.from({ length: 120 }, (_, i) => err(55 - i, 13 + Math.floor(i / 4), 36 - Math.floor(i / 4), i % 2 ? 'NoCurrentPrice' : 'SupplierUnreachable', i % 2 ? `بارزپخش برای BP-8${String(i).padStart(4, '0')} قیمت روز ندارد` : 'پاسخی از بارزپخش نیامد (timeout after 30s)', i % 2 ? { code: `BP-8${String(i).padStart(4, '0')}` } : null)) : []),
];

const dashboard = (state: State): Dashboard => {
  const running = current(state);
  return {
    settings_complete: state !== 'empty', store_name: state === 'empty' ? null : 'یدکی مرکزی',
    supplier_user: state === 'empty' ? null : 'yadaki.markazi', invoices_cached: state === 'empty' ? 0 : 124,
    invoices_unpaid: state === 'empty' ? 0 : 3, last_import: state === 'empty' ? null : jobs[0]!,
    last_refresh: state === 'empty' ? null : jobs[1]!, running, pending_review: state === 'review' ? reviewItems.length : 0,
    recent_errors: state === 'empty' ? 0 : 4, decided_matches: state === 'empty' ? 0 : matches.length,
    links_to_review: state === 'empty' ? 0 : matches.filter((m) => m.state === 'NEEDS_REVIEW' || m.state === 'STALE').length,
    held_writes: held ? heldWrites.length : 0,
    appimage_offer: new URLSearchParams(location.search).has('appimage'),
    upgrade_notice: upgraded ? upgradeNotice : null,
  };
};

// `?upgraded=1`: an install upgraded from v1.1.1 (plan 130 task 14), so the dashboard shows its notice.
let upgraded = typeof location !== 'undefined' && new URLSearchParams(location.search).has('upgraded');
const upgradeNotice = {
  from_schema: 3, threshold_from: 70, threshold_to: 85, links_unverified: 869,
  appimages: ['/home/seller/Downloads/landau-bridge_0.1.0_amd64.AppImage', '/home/seller/دانلودها/landau-bridge_1.1.1_amd64.AppImage'],
  menu_copy: '/home/seller/.local/bin/landau-bridge.AppImage',
};

// ---- plan 130: what a pick would be, and the second confirm against a conflict
const CONFLICTS: Record<string, Reason[]> = {
  p_69349: [{ kind: 'NUMBER_CONFLICT', lacking: ['69349'] }],
  p_2: [{ kind: 'OPPOSITE_WORDS', supplier: 'جلو', candidate: 'عقب' }],
  p_5: [{ kind: 'NUMBER_UNBACKED', candidate: ['206'] }],
};
const everyCandidate = (): Scored[] => [...reviewItems.flatMap((i) => i.candidates ?? []), ...undecided.flatMap((u) => u.candidates)];
const SHELF_COST: Record<string, number> = { p_69349: 5_500_000, p_1: 1_380_000, p_4: 372_000 };
const inspection = (name: string, code: string, ids: string[]): Inspection => {
  const candidates: CandidateView[] = ids.flatMap((id) => {
    const s = everyCandidate().find((c) => c.candidate.id === id);
    if (!s) return [];
    const r = CONFLICTS[id];
    const cost = SHELF_COST[id];
    const list = code === '33102018' ? 5_631_000 : code === 'BP-33005' ? null : 1_300_000;
    return [{
      name, code, product: s.candidate, similarity: s.similarity,
      next_cost: list == null || id === 'p_3' ? null : Math.round(list * 1.1), cost_kept: false,
      evidence: {
        numbers: { state: r?.[0]?.kind === 'NUMBER_CONFLICT' ? 'CONFLICT' : 'INSUFFICIENT', unbacked: r?.[0]?.kind === 'NUMBER_UNBACKED', supplier: id.startsWith('p_6') || id.startsWith('p_4') ? ['44649'] : [], candidate: id === 'p_69349' ? ['69349'] : id === 'p_44649' ? ['44649'] : [], shared: id === 'p_44649' ? ['44649'] : [], lacking: id === 'p_69349' ? ['69349'] : [] },
        words: { state: r?.[0]?.kind === 'OPPOSITE_WORDS' ? 'CONFLICT' : 'CONSISTENT', uncovered: id === 'p_69349' ? ['قدیم'] : [], opposites: r?.[0]?.kind === 'OPPOSITE_WORDS' ? [['جلو', 'عقب']] : [], missing: [] },
        brand: { state: s.candidate.brand ? 'CONSISTENT' : 'INSUFFICIENT', supplier: s.candidate.brand ? { kind: 'ONE', brand: s.candidate.brand, ambiguous: false } : { kind: 'NONE' }, candidate: s.candidate.brand },
      },
      verdict: r ? { verdict: 'CONFIRM_THEN_WRITE', reasons: r } : { verdict: 'WRITE' },
      shelf: id === 'p_3' ? { status: 'UNKNOWN', value: 'اتصال به لاندا برقرار نشد. نشانی درگاه و اینترنت را بررسی کنید.' }
        : { status: 'KNOWN', value: cost == null ? null : { id: `s_${id}`, inventory: 0, cost, price: Math.round(cost * 1.15 / 1000) * 1000, price_source: 'RULE', read_at: ago(0, 7, 40) } },
    }];
  });
  const list = code === '33102018' ? 5_631_000 : code === 'BP-33005' ? null : 1_300_000;
  return {
    registry_read_at: ago(0, 7, 31),
    quote: list == null ? { status: 'UNKNOWN', value: 'اتصال به سایت بارزپخش برقرار نشد.' }
      : { status: 'KNOWN', value: { code, name, list_price: list, has_vat: true, site_vat: 10, vat_setting: 10, cost: Math.round(list * 1.1), available: code !== 'BP-20871', renamed: false, read_at: ago(0, 7, 40) } },
    rules: { status: 'KNOWN', value: [{ scope: 'SHELF', basis: 'MARKUP_PERCENT', value: 15 }] },
    candidates, missing: ids.filter((id) => !candidates.some((c) => c.product.id === id)),
  };
};
const confirmFirst = (product_id: unknown, confirmed: unknown) => {
  const r = CONFLICTS[String(product_id)];
  if (r && JSON.stringify(confirmed ?? null) !== JSON.stringify(r)) {
    throw { kind: 'ConfirmRequired', message: 'نام بارزپخش با این قطعه جور نیست؛ ناسازگاری‌ها و تغییر قیمت خرید را ببینید و دوباره تأیید کنید.', detail: JSON.stringify(r) };
  }
};
const heldWrites: HeldWrite[] = [{
  id: 7, job_id: 42, kind: 'IMPORT', store: 'st_01', product: 'p_1', authenticity: 'ORIGINAL', verb: 'PATCH',
  snapshot: { place: 'LIVE', stock_id: 's_1', inventory: 3, cost: 1_380_000, updated_at: ago(0, 9, 0) },
  target_inventory: 4, target_cost: 1_430_000, items: [1], status: 'SUPERSEDED',
  outcome: { row: { place: 'LIVE', stock_id: 's_1', inventory: 2, cost: 1_380_000, updated_at: ago(0, 9, 30) } },
  opened_at: ago(0, 9, 1), closed_at: ago(0, 10, 2), product_name: 'لنت ترمز جلو پراید ۱۳۱', supplier_names: ['لنت ترمز جلو پراید'],
}];

const detail = (id: number, state: State): JobDetail => {
  if (id === 43 && state === 'running') return { job: runningJob, items: [], events: runningEvents };
  if (id === 43 && state === 'review') return { job: reviewJob, items: reviewItems, events: doneEvents.slice(0, 4) };
  if (id === 44 && state === 'refreshing') return { job: refreshJob, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 6), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 3)] };
  if (id === 40) return { job: jobs[2]!, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 4400), ev(2, 'ERROR', 'FETCH', 'پاسخی از بارزپخش نیامد', 4390)] };
  if (id === 41) return { job: jobs[1]!, items: doneItems.filter((i) => i.action === 'PATCH'), events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 1500), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 1490), ev(3, 'INFO', 'REPORT', JSON.stringify(jobs[1]!.summary), 1480)] };
  const j = jobs.find((x) => x.id === id) ?? jobs[0]!;
  return { job: j, items: j.status === 'DONE' ? doneItems : [], events: j.status === 'DONE' ? doneEvents : doneEvents.slice(0, 2) };
};

/** `?update=ready|failed|downloading|deb` shows the app-update card and band in that state. */
const updateView = (): UpdateView => {
  const u = new URLSearchParams(location.search).get('update');
  const base = { current: '1.0.0', kind: u === 'deb' ? 'deb' : 'appimage', auto_update: true, last_check: ago(0, 9, 5) } as const;
  if (u === 'ready' || u === 'deb') return { ...base, state: { state: 'Ready', version: '1.0.1' } };
  if (u === 'downloading') return { ...base, state: { state: 'Downloading', version: '1.0.1', percent: 42 } };
  if (u === 'failed') return { ...base, state: { state: 'Failed', kind: 'Network', message: 'اتصال به GitHub برقرار نشد؛ بعداً دوباره بررسی می‌شود.', at: ago(0, 9, 5) } };
  return { ...base, state: { state: 'Idle' } };
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// The incident's two damaged rows as the audit lists them (plan 130 task 9).
const audit: Audit = {
  read_at: '2026-11-09T09:30:00Z', registry_read_at: '2026-11-09T09:28:00Z', store: 'store1', examined: 6, consistent: 4, unweighed: 0, unlinked: 12,
  rows: [
    {
      stock_id: '6aecc0a1d00e9931421f9cf0', product_id: '6aec9869d00e9931421a576d', product_name: 'بست کمری نایلونی 30 سانتی عرض 3.6 میلی متر CW',
      brand: 'آراد پارت', authenticity: 'ORIGINAL', inventory: 0, cost: 96_800, price: 111_000, doubts: ['UNPROVEN_LINK'], should_cost: null, should_unavailable: false, difference: null,
      links: [
        { name: 'بست کمري CW3.6*370 آراد پارت (100)', code: 'AR1330-11', how: 'AUTO', state: 'NEEDS_REVIEW', proven: false, reasons: [{ kind: 'NUMBER_CONFLICT', lacking: ['30 cm'] }], today: { status: 'KNOWN', value: { cost: 91_300, available: true, code: 'AR1330-11', name: 'بست کمري CW3.6*370 آراد پارت (100)', renamed: false } }, rename: null, units: 0 },
        { name: 'بست کمري CW4.8*370 تايوان آراد پارت (100)', code: 'AR1330-4', how: 'AUTO', state: 'NEEDS_REVIEW', proven: false, reasons: [{ kind: 'NUMBER_CONFLICT', lacking: ['30 cm', '3.6 mm'] }], today: { status: 'UNKNOWN', value: 'بارزپخش پاسخ نداد' }, rename: null, units: 0 },
      ],
      last_write: { job_id: 12, kind: 'IMPORT', at: '2026-11-06T06:08:00Z', names: ['بست کمري CW4.8*370 تايوان آراد پارت (100)'], cost: 96_800 }, unproven_units: 0,
      transfers: [], counted_units: 0,
    },
    {
      stock_id: '6aecc0a1d00e9931421f9d1e', product_id: '6aec987cd00e9931421a5e12', product_name: 'بلبرینگ بزرگ چرخ عقب پراید و چرخ جلو پیکان',
      brand: 'PSN عظام', authenticity: 'ORIGINAL', inventory: 0, cost: 5_500_000, price: 6_325_000, doubts: ['UNPROVEN_LINK', 'UNPROVEN_UNITS'], should_cost: 6_194_100, should_unavailable: false, difference: 694_100,
      links: [
        { name: 'بلبرينگ بزرگ چرخ عقب پرايد جدبد (بلبرينگ بزرگ چرخ جلو پيکان) 44649 PSN عظام (96) 3154028', code: '33102018', how: 'USER', state: 'REBUILT', proven: true, reasons: [], today: { status: 'KNOWN', value: { cost: 6_194_100, available: true, code: '33102018', name: 'بلبرينگ بزرگ چرخ عقب پرايد جدبد (بلبرينگ بزرگ چرخ جلو پيکان) 44649 PSN عظام (96) 3154028', renamed: false } }, rename: null, units: 0 },
        { name: 'بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027', code: '33102017', how: 'USER', state: 'NEEDS_REVIEW', proven: false, reasons: [{ kind: 'NUMBER_CONFLICT', lacking: ['44649'] }], today: { status: 'KNOWN', value: { cost: 5_500_000, available: true, code: '33102017', name: 'بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027', renamed: false } }, rename: null, units: 3 },
      ],
      last_write: { job_id: 12, kind: 'IMPORT', at: '2026-11-06T06:08:00Z', names: ['بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027'], cost: 5_500_000 }, unproven_units: 3,
      // PI-19: the 3 units a quantity import moved from the hurried pick, for the seller to count
      transfers: [{ item_id: 1207, job_id: 14, at: '2026-11-07T09:12:00Z', name: 'بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027', code: '33102017', qty: 3 }],
      counted_units: 0,
    },
  ],
  // PI-21: every row an automatic link last wrote — the clamp row (a doubt too) and a consistent one.
  automatic: [
    {
      stock_id: '6aecc0a1d00e9931421f9cf0', product_id: '6aec9869d00e9931421a576d', product_name: 'بست کمری نایلونی 30 سانتی عرض 3.6 میلی متر CW',
      brand: 'آراد پارت', authenticity: 'ORIGINAL', inventory: 0, cost: 96_800, price: 111_000, doubted: true,
      links: [{ name: 'بست کمري CW4.8*370 تايوان آراد پارت (100)', code: 'AR1330-4', how: 'AUTO', state: 'NEEDS_REVIEW', proven: false, reasons: [{ kind: 'NUMBER_CONFLICT', lacking: ['30 cm', '3.6 mm'] }], today: null, rename: null, units: 0 }],
      last_write: { job_id: 12, kind: 'IMPORT', at: '2026-11-06T06:08:00Z', names: ['بست کمري CW4.8*370 تايوان آراد پارت (100)'], cost: 96_800 },
    },
    ...Array.from({ length: bulk ? 41 : 1 }, (_, i) => ({
      stock_id: `6aecc0a1d00e9931421f${(0x9d12 + i).toString(16)}`, product_id: `6aec9869d00e9931421a${(0x6c04 + i).toString(16)}`,
      product_name: i ? `طبق پراید بوش فلزی ${i}` : 'بلبرینگ میانی گاردان پژو 405 6006', brand: i ? 'سایپا یدک' : 'الما۷',
      authenticity: 'ORIGINAL' as const, inventory: i % 4, cost: i ? 410_000 + i * 1_000 : 5_247_000, price: null, doubted: false,
      links: [{ name: i ? `بوش طبق پرايد فلزي سايپا يدک ${i}` : 'بلبرينگ ميل گاردان 405 (6006) ELMA7', code: i ? `5903${10 + i}` : '115001165', how: 'AUTO' as const, state: 'VALID' as const, proven: true, reasons: [], today: null, rename: null, units: 0 }],
      last_write: { job_id: 12, kind: 'IMPORT' as const, at: '2026-11-06T06:08:00Z', names: [i ? `بوش طبق پرايد فلزي سايپا يدک ${i}` : 'بلبرينگ ميل گاردان 405 (6006) ELMA7'], cost: i ? 410_000 + i * 1_000 : 5_247_000 },
    })),
  ],
  // PI-19: a count the seller made on another row earlier
  counts: [{
    id: 1, stock_id: '6aecc0a1d00e9931421f9ced', product_name: 'بست کمری نایلونی 38 سانتی عرض 4.8 میلی متر مشکی', inventory: 6,
    read_at: '2026-11-08T10:00:00Z', counted_at: '2026-11-08T10:04:00Z', units: 4,
    transfers: [{ item_id: 1102, job_id: 13, at: '2026-11-07T08:00:00Z', name: 'بست کمري CW4.8*370 تايوان آراد پارت (100)', code: 'AR1330-4', qty: 4 }],
  }],
  stale: false,
};

/** PI-19 in the mock: a count takes the row's units out as the core does; a withdrawal turns the audit stale. */
const countUnits = (stockId: string): Audit => {
  const i = audit.rows.findIndex((r) => r.stock_id === stockId);
  const row = audit.rows[i];
  if (!row) throw { kind: 'Other', message: 'بازبینی پس از باز شدن صفحه عوض شد؛ دوباره بازبینی کنید.' };
  audit.counts.unshift({ id: audit.counts.length + 2, stock_id: row.stock_id, product_name: row.product_name, inventory: row.inventory,
    read_at: audit.read_at, counted_at: new Date().toISOString(), units: row.unproven_units, transfers: row.transfers });
  row.counted_units += row.unproven_units; row.unproven_units = 0; row.transfers = [];
  row.doubts = row.doubts.filter((d) => d !== 'UNPROVEN_UNITS');
  if (!row.doubts.length) { audit.rows.splice(i, 1); audit.consistent += 1; }
  return audit;
};

/** A withdrawal lists a listed row's units again; on a row the audit vouched for it turns the audit stale. */
const withdrawCount = (id: number): Audit => {
  const c = audit.counts.find((x) => x.id === id);
  if (!c) throw { kind: 'Other', message: 'بازبینی پس از باز شدن صفحه عوض شد؛ دوباره بازبینی کنید.' };
  audit.counts = audit.counts.filter((x) => x.id !== id);
  const row = audit.rows.find((r) => r.stock_id === c.stock_id);
  if (!row) { audit.stale = true; return audit; }
  row.counted_units -= c.units; row.unproven_units += c.units; row.transfers = [...row.transfers, ...c.transfers];
  if (!row.doubts.includes('UNPROVEN_UNITS')) row.doubts = [...row.doubts, 'UNPROVEN_UNITS'];
  return audit;
};

export async function mockCall<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const state = devState();
  await wait(120);
  const out: unknown = (() => {
    switch (cmd) {
      case 'settings_get': return state === 'empty' ? settingsEmpty : settings;
      case 'settings_save': return { ...(state === 'empty' ? settingsEmpty : settings), ...(args.patch as object) };
      case 'update_status': case 'update_check': return updateView();
      case 'update_install': return undefined;
      case 'landau_validate': return landau;
      case 'supplier_test_login': return undefined;
      case 'dashboard_get': return dashboard(state);
      case 'import_start': return 43;
      case 'refresh_start': return 44;
      case 'import_review': return reviewItems;
      case 'match_set': confirmFirst(args.product_id, args.confirmed); decide(args.name_seen); return undefined;
      case 'match_none': decide(args.name_seen); return undefined;
      case 'link_rename_confirm': return confirmRename(args.confirmed);
      case 'link_rename_withdraw': renamedLink.renamed_json = null; return undefined;
      case 'review_decide': {
        const d = args.decision as { product_id?: string; confirmed?: Reason[] };
        if (d.product_id) confirmFirst(d.product_id, d.confirmed);
        return undefined;
      }
      case 'review_inspect': return inspection(String(args.name), String(args.code), args.product_ids as string[]);
      case 'intents_held': return held ? heldWrites : [];
      case 'intent_resolve': heldWrites.splice(0, heldWrites.length); return true;
      case 'import_continue': case 'match_clear': case 'open_log_folder': case 'appimage_dismiss': return undefined;
      case 'upgrade_notice_dismiss': upgraded = false; return undefined;
      case 'matches_undecided': return state === 'empty' ? [] : undecided;
      case 'decisions_export': return matches.length;
    case 'audit_run': return audit;
    case 'audit_export': return audit.rows.length + audit.automatic.length;
      case 'audit_units_count': return countUnits(String(args.stock_id));
      case 'audit_units_withdraw': return withdrawCount(Number(args.count));
      case 'decisions_import': return importReport;
      case 'appimage_install': return '/home/vahid/.local/bin/landau-bridge.AppImage';
      case 'review_search': return [scored('p_s1', `${args.term}`, 'جست‌وجو', 66)];
      case 'job_cancel': return true;
      case 'job_current': return current(state);
      case 'jobs_list': return state === 'empty' ? [] : [...(current(state) ? [current(state)!] : []), ...jobs];
      case 'job_get': return detail(Number(args.job_id), state);
      case 'matches_list': return state === 'empty' ? [] : matches;
      case 'errors_list': return state === 'empty' ? [] : errors;
      default: throw { kind: 'Other', message: `mock: unknown command ${cmd}`, detail: cmd };
    }
  })();
  return out as T;
}

/** Emits one progress tick for the running fixtures so the stepper shows its bar, plus a few
 *  event payloads so the stepper's event list is visible in stills. */
export function mockEvents(h: JobHandlers): () => void {
  const state = devState();
  if (state === 'running') {
    setTimeout(() => h.progress?.({ job_id: 43, phase: 'MATCH', done: 37, total: 89, message: 'تطبیق با فهرست لاندا' }), 200);
    setTimeout(() => h.event?.({ job_id: 43, level: 'INFO', phase: 'PREFLIGHT', message: 'اتصال به لاندا و بارزپخش برقرار است' }), 250);
    setTimeout(() => h.event?.({ job_id: 43, level: 'INFO', phase: 'FETCH', message: '۱۲۴ فاکتور خوانده شد؛ ۳ فاکتور پرداخت‌نشده رد شد' }), 300);
    setTimeout(() => h.event?.({ job_id: 43, level: 'WARN', phase: 'MATCH', message: 'برای BP-44120 چند نامزد نزدیک هم پیدا شد' }), 350);
  } else if (state === 'refreshing') {
    setTimeout(() => h.progress?.({ job_id: 44, phase: 'WRITE', done: 210, total: 412, message: 'نوشتن قیمت خرید روی قفسه' }), 200);
    setTimeout(() => h.event?.({ job_id: 44, level: 'INFO', phase: 'PRICE', message: 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد' }), 250);
  }
  return () => {};
}
