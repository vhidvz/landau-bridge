import { PersianCalendar, fromDate, toCalendar, today } from '@internationalized/date';
import type { JobItemRow, JobRow } from '~/types/bridge';

const FA_DIGITS = '۰۱۲۳۴۵۶۷۸۹';
const AR_DIGITS = '٠١٢٣٤٥٦٧٨٩';
const nf = new Intl.NumberFormat('fa-IR', { maximumFractionDigits: 0 });

export const faNumber = (n: number | null | undefined): string => (n == null || Number.isNaN(n) ? '—' : nf.format(n));
export const rials = (n: number | null | undefined): string => (n == null ? '—' : `${faNumber(n)} ریال`);

export const latinDigits = (s: string): string =>
  s.replace(/[۰-۹]/g, (d) => String(FA_DIGITS.indexOf(d))).replace(/[٠-٩]/g, (d) => String(AR_DIGITS.indexOf(d)));

/** `۱۴۰۱/۱/۵` → `1401/01/05`; null when not a Jalali Y/M/D. */
export const normalizeJalali = (s: string): string | null => {
  const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})$/.exec(latinDigits(s.trim()));
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (y < 1300 || mo < 1 || mo > 12 || d < 1 || d > 31) return null;
  return `${y}/${String(mo).padStart(2, '0')}/${String(d).padStart(2, '0')}`;
};

const jalaliToday = () => toCalendar(today('Asia/Tehran'), new PersianCalendar());
export const todayJalali = (): string => {
  const t = jalaliToday();
  return `${t.year}/${String(t.month).padStart(2, '0')}/${String(t.day).padStart(2, '0')}`;
};
export const jalaliYearStart = (): string => `${jalaliToday().year}/01/01`;

export const isoToLocal = (iso: string | null | undefined): string =>
  iso ? new Intl.DateTimeFormat('fa-IR', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(iso)) : '—';

export const phaseLabel: Record<string, string> = {
  PREFLIGHT: 'بررسی اتصال‌ها',
  FETCH: 'دریافت فاکتورها',
  AGGREGATE: 'جمع‌بندی اقلام',
  MATCH: 'تطبیق با فهرست لاندا',
  REVIEW: 'در انتظار تصمیم شما',
  PRICE: 'قیمت روز بارزپخش',
  WRITE: 'نوشتن روی قفسه',
  REPRICE: 'اعمال قواعد قیمت‌گذاری لاندا',
  REPORT: 'گزارش نهایی',
};

export const statusLabel: Record<string, string> = {
  RUNNING: 'در حال اجرا',
  REVIEW: 'در انتظار تصمیم',
  DONE: 'انجام شد',
  FAILED: 'ناموفق',
  CANCELLED: 'لغو شد',
  INTERRUPTED: 'ناتمام',
  PLANNED: 'برنامه‌ریزی‌شده',
  SKIPPED: 'رد شد',
  WRITTEN: 'نوشته شد',
};

const skipLabel: Record<string, string> = {
  MANUAL: 'قیمت دستی حفظ شد',
  NO_COST: 'بدون قیمت خرید',
  NO_RULE: 'قاعده‌ای در لاندا ندارید',
  UNCHANGED: 'بدون تغییر',
};

export const verdictLabel = (v: string | null | undefined): string => {
  if (!v) return '—';
  if (v.startsWith('REPRICED:')) return `قیمت‌گذاری شد: ${rials(Number(v.slice(9)))}`;
  if (v.startsWith('SKIPPED:')) return skipLabel[v.slice(8)] ?? v;
  if (v === 'UNCHANGED') return 'بدون تغییر';
  return v;
};

export const kindLabel = (kind: string): string =>
  ({
    SupplierUnreachable: 'اتصال به بارزپخش',
    SupplierBlocked: 'مسدود شدن توسط بارزپخش',
    SupplierBadCredentials: 'نام کاربری/کلمه عبور بارزپخش',
    SupplierSessionExpired: 'انقضای نشست بارزپخش',
    SupplierParse: 'تغییر ساختار صفحه بارزپخش',
    NoCurrentPrice: 'بدون قیمت روز',
    InvoiceUnpaid: 'فاکتور پرداخت‌نشده',
    InvoiceAlreadyImported: 'فاکتور قبلاً منتقل‌شده',
    LineNonPositiveQty: 'ردیف با تعداد صفر',
    LandauUnauthorized: 'کلید API لاندا',
    LandauForbidden: 'عدم مجوز در لاندا',
    LandauUnreachable: 'اتصال به لاندا',
    LandauRejected: 'رد درخواست توسط لاندا',
    BusinessNotApproved: 'کسب‌وکار تأییدنشده',
    NoStore: 'بدون فروشگاه',
    ProductNotApproved: 'قطعه تأییدنشده',
    DuplicateRow: 'ردیف تکراری',
    MirrorTimeout: 'تأخیر قفسه لاندا',
    NoPricingRule: 'بدون قاعده قیمت‌گذاری',
    RepriceSkipped: 'قیمت‌گذاری رد شد',
    Unmatched: 'تطبیق‌نشده',
    NoneDecision: 'در فهرست لاندا نیست',
    JobCancelled: 'لغو شد',
    JobInterrupted: 'ناتمام ماند',
    VatRateMismatch: 'اختلاف نرخ ارزش افزوده',
    NegativeQuantity: 'مرجوعی بیش از خرید',
    AmbiguousSupplierLink: 'چند کد برای یک قطعه',
    NoSupplierLink: 'بدون پیوند به بارزپخش',
    SettingsIncomplete: 'تنظیمات ناقص',
    KeyringUnavailable: 'ذخیره‌ساز امن در دسترس نیست',
    Db: 'پایگاه داده برنامه',
    Other: 'خطای پیش‌بینی‌نشده',
    JobNotFound: 'عملیات پیدا نشد',
    JobBusy: 'عملیات دیگری در جریان است',
  })[kind] ?? kind;

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
