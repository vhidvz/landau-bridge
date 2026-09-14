import { PersianCalendar, toCalendar, today } from '@internationalized/date';

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
