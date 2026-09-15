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
  vat_percent: 10, overwrite_manual: false, fetch_since_only: true, secret_backend: 'keyring', data_dir: '/home/vahid/.local/share/ir.landau.bridge',
  name_map: { rows: 29472, products: 28643, loaded_at: ago(1, 12, 0), file: '/home/vahid/WorkSpace/landau-parts/output/parts_map.csv' },
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
  item(6, 'BP-60233', 'روغن موتور ۴ لیتری بهران', 24, { status: 'SKIPPED', error_kind: 'NoneDecision', error_msg: 'در فهرست لاندا نیست' }),
  item(7, 'BP-71850', 'واشر سرسیلندر ۲۰۶', 10, { product_id: 'p_15', product_name: 'واشر سرسیلندر پژو ۲۰۶', action: 'ATTACH', qty_after: 10, cost: 940_000, price_after: 1_170_000, reprice_verdict: 'REPRICED:1170000' }),
  item(8, 'BP-80017', 'پمپ بنزین پراید', 5, { product_id: 'p_16', product_name: 'پمپ بنزین پراید', action: 'PATCH', qty_before: 2, qty_after: 7, cost: 1_650_000, price_before: 2_050_000, price_after: 2_050_000, reprice_verdict: 'UNCHANGED' }),
];

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
  err(60, 0, 42, 'NoneDecision', 'BP-60233 در فهرست لاندا نیست؛ برای افزودن آن تیکت ثبت کنید'),
  err(59, 1, 41, 'RepriceSkipped', '۱۲ ردیف با قیمت دستی دست نخورد', { skipped: 'MANUAL', rows: 12 }),
  err(58, 3, 40, 'SupplierUnreachable', 'پاسخی از بارزپخش نیامد (timeout after 30s)', { url: 'https://barezpakhsh.ir/invoices' }),
  err(57, 5, null, 'LandauUnauthorized', 'کلید API لاندا پذیرفته نشد'),
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
    appimage_offer: new URLSearchParams(location.search).has('appimage'),
  };
};

const detail = (id: number, state: State): JobDetail => {
  if (id === 43 && state === 'running') return { job: runningJob, items: [], events: runningEvents };
  if (id === 43 && state === 'review') return { job: reviewJob, items: reviewItems, events: doneEvents.slice(0, 4) };
  if (id === 44 && state === 'refreshing') return { job: refreshJob, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 6), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 3)] };
  if (id === 40) return { job: jobs[2]!, items: [], events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 4400), ev(2, 'ERROR', 'FETCH', 'پاسخی از بارزپخش نیامد', 4390)] };
  if (id === 41) return { job: jobs[1]!, items: doneItems.filter((i) => i.action === 'PATCH'), events: [ev(1, 'INFO', 'PREFLIGHT', 'اتصال به لاندا و بارزپخش برقرار است', 1500), ev(2, 'INFO', 'PRICE', 'قیمت روز ۴۱۲ کد از بارزپخش خوانده شد', 1490), ev(3, 'INFO', 'REPORT', JSON.stringify({ rows: 412, changed: 96, skipped_manual: 12 }), 1480)] };
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
      case 'name_map_load': return settings.name_map;
      case 'landau_validate': return landau;
      case 'supplier_test_login': return undefined;
      case 'dashboard_get': return dashboard(state);
      case 'import_start': return 43;
      case 'refresh_start': return 44;
      case 'import_review': return reviewItems;
      case 'review_decide': case 'import_continue': case 'match_set': case 'match_none': case 'match_clear': case 'open_log_folder': case 'appimage_dismiss': return undefined;
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
