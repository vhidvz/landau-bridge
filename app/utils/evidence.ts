// What the review and Matches screens say about a pick (plan 130 R-d, R-k): the write table's reasons
// and the evidence in plain Persian, and every money figure with its unit — rials, toman beside.
import type { BridgeError, EvidenceState, LinkState, Read, Reason, RuleView, ShelfView, Verdict } from '~/types/bridge';
import { faDigits, faNumber, faPercent, type Tone } from './fa';

/** Toman beside rials: one toman is ten rials. */
export const toman = (r: number): string => `${faNumber(Math.round(r / 10))} تومان`;
export const money = (r: number | null | undefined): string => (r == null ? '—' : `${faNumber(r)} ریال (${toman(r)})`);

const UNIT: Record<string, string> = { cm: 'سانتی‌متر', mm: 'میلی‌متر' };
/** A number as the evidence states it: «3.6 mm» → «۳٫۶ میلی‌متر»; a code («xu7») stays Latin. */
export const numLabel = (s: string): string => {
  const m = /^(\d+(?:\.\d+)?)(?: (cm|mm))?$/.exec(s.trim());
  if (!m) return s;
  const n = faDigits(m[1]!).replace('.', '٫');
  return m[2] ? `${n} ${UNIT[m[2]]}` : n;
};
const nums = (xs: string[]): string => xs.map(numLabel).join('، ');
const words = (xs: string[]): string => xs.map((w) => `«${w}»`).join('، ');

/** One reason of the write table, as the seller reads it. */
export const reasonText = (r: Reason): string => {
  switch (r.kind) {
    case 'BELOW_THRESHOLD': return `شباهت ${faNumber(r.similarity)} از حد ${faNumber(r.threshold)} کمتر است`;
    case 'TIED': return 'پیشنهاد دیگری همین شباهت را دارد';
    case 'ANOTHER_PASSES': return 'پیشنهاد دیگری هم پذیرفتنی است';
    case 'NUMBER_CONFLICT': return `عدد ناسازگار: ${nums(r.lacking)} در نام بارزپخش نیست`;
    case 'NUMBER_UNBACKED': return `عدد بی‌پشتوانه: ${nums(r.candidate)} در نام بارزپخش نیامده`;
    case 'WORDS_UNCOVERED': return `در نام بارزپخش نیست: ${words(r.words)}`;
    case 'OPPOSITE_WORDS': return `ناسازگار: «${r.supplier}» در بارزپخش، «${r.candidate}» در قطعه`;
    case 'PART_WORDS_MISSING': return `در قطعه نیست: ${words(r.words)}`;
    case 'NO_BRAND_IN_NAME': return 'برندی در نام بارزپخش پیدا نشد';
    case 'SEVERAL_BRANDS_IN_NAME': return `چند برند در نام بارزپخش: ${words(r.brands)}`;
    case 'BRAND_UNSURE': return `برند نامطمئن: «${r.supplier}» واژهٔ رایج هم هست`;
    case 'CANDIDATE_WITHOUT_BRAND': return 'قطعهٔ لاندا برند ندارد';
    case 'BRAND_MISMATCH': return `برند ناسازگار: «${r.supplier}» در بارزپخش، «${r.candidate}» در قطعه`;
    case 'DEFINITION_NOT_EXACT': return 'تعریف قطعه دقیقاً یکی نیست';
  }
};

export const reasonsOf = (v: Verdict): Reason[] => (v.verdict === 'WRITE' ? [] : v.reasons);

/** A pick's verdict in one word, for the candidate row. */
export const pickLabel = (v: Verdict): string => (v.verdict === 'WRITE' ? 'جور است' : v.verdict === 'CONFIRM_THEN_WRITE' ? 'ناسازگار' : 'نیازمند بررسی');
export const pickTone = (v: Verdict): Tone => (v.verdict === 'WRITE' ? 'ok' : v.verdict === 'CONFIRM_THEN_WRITE' ? 'danger' : 'warn');

export const stateText: Record<EvidenceState, string> = { CONSISTENT: 'سازگار', CONFLICT: 'ناسازگار', INSUFFICIENT: 'ناکافی' };
export const stateTone: Record<EvidenceState, Tone> = { CONSISTENT: 'ok', CONFLICT: 'danger', INSUFFICIENT: 'muted' };

/** The reasons a `ConfirmRequired` refusal names — the second confirm shows exactly these. */
export const confirmReasons = (e: unknown): Reason[] | null => {
  const err = e as Partial<BridgeError> | null;
  if (!err || err.kind !== 'ConfirmRequired' || typeof err.detail !== 'string') return null;
  try {
    const r = JSON.parse(err.detail) as unknown;
    return Array.isArray(r) ? (r as Reason[]) : null;
  } catch { return null; }
};

export interface CostChange { diff: number; dir: 'up' | 'down' | 'flat' }
/** The new cost against the row's cost as Landau holds it; null when either is unknown. */
export const costChange = (next: number | null | undefined, row: ShelfView | null | undefined): CostChange | null => {
  if (next == null || row?.cost == null) return null;
  const diff = next - row.cost;
  return { diff, dir: diff > 0 ? 'up' : diff < 0 ? 'down' : 'flat' };
};
/** In words, never a sign: a leading «+» or «−» lands on the wrong side of a number in RTL. */
export const costChangeText = (c: CostChange | null): string =>
  !c ? '—' : c.dir === 'flat' ? 'بدون تغییر' : `${money(Math.abs(c.diff))} ${c.dir === 'up' ? 'بیشتر' : 'کمتر'}`;
/** The compact form for a candidate row; the full figures are in the confirm and the money line. */
export const costChangeShort = (c: CostChange | null): string =>
  !c || c.dir === 'flat' ? '' : `${c.dir === 'up' ? '▲' : '▼'} ${faNumber(Math.abs(c.diff))} ریال`;

/** An active pricing rule as the seller set it in Landau — named, never computed (fact 19). */
export const ruleText = (r: RuleView): string => {
  if (r.basis === 'MARKUP_PERCENT' && r.value != null) return `سود ${faPercent(r.value, 2)} روی قیمت خرید`;
  if (r.basis === 'FIXED_AMOUNT' && r.value != null) return `${faNumber(r.value)} ریال روی قیمت خرید`;
  return 'قاعدهٔ قیمت‌گذاری لاندا';
};

export const known = <T>(r: Read<T> | null | undefined): T | undefined => (r?.status === 'KNOWN' ? r.value : undefined);
export const unknownText = <T>(r: Read<T> | null | undefined): string | null => (r?.status === 'UNKNOWN' ? r.value : null);

export const linkStateLabel: Record<LinkState, string> = {
  UNVERIFIED: 'بررسی‌نشده', VALID: 'معتبر', REBUILT: 'بازسازی‌شده', NEEDS_REVIEW: 'نیازمند بررسی', STALE: 'قطعه حذف شده',
};
export const linkStateTone: Record<LinkState, Tone> = { UNVERIFIED: 'muted', VALID: 'ok', REBUILT: 'ok', NEEDS_REVIEW: 'warn', STALE: 'danger' };
export const needsReview = (s: LinkState): boolean => s === 'NEEDS_REVIEW' || s === 'STALE';

/** A link's stored reasons (`state_reason`): the write table's `Reason[]`, or `{why, reasons}`. */
export const storedReasons = (stateReason: string | null): Reason[] => {
  if (!stateReason) return [];
  try {
    const v = JSON.parse(stateReason) as unknown;
    if (Array.isArray(v)) return v as Reason[];
    const r = (v as { reasons?: unknown }).reasons;
    return Array.isArray(r) ? (r as Reason[]) : [];
  } catch { return []; }
};
