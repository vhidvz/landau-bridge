import { describe, expect, it } from 'vitest';
import {
  faNumber, rials, latinDigits, normalizeJalali, todayJalali, jalaliYearStart, verdictLabel, kindLabel, availabilityLabel,
  faPercent, faDuration, faVersion, jobKindLabel, statusTone, summaryTiles, summaryLabel, reportEventText,
  kindFamily, isoToJalali, nextSinceDate, priceDelta, priceDeltaLabel,
} from './fa';

describe('fa', () => {
  it('formats numbers with Persian digits and grouping', () => {
    expect(faNumber(1234567)).toBe('۱٬۲۳۴٬۵۶۷');
    expect(faNumber(null)).toBe('—');
    expect(rials(173000)).toBe('۱۷۳٬۰۰۰ ریال');
  });

  it('normalises Jalali dates', () => {
    expect(latinDigits('۱۴۰۱/۱/۵')).toBe('1401/1/5');
    expect(normalizeJalali('۱۴۰۱/۱/۵')).toBe('1401/01/05');
    expect(normalizeJalali('1401-01-05')).toBeNull();
    expect(normalizeJalali('1401/13/01')).toBeNull();
  });

  it('knows today and the year start in Jalali', () => {
    expect(todayJalali()).toMatch(/^1[34]\d\d\/\d\d\/\d\d$/);
    expect(jalaliYearStart()).toMatch(/^1[34]\d\d\/01\/01$/);
  });

  it('labels verdicts and kinds in Persian', () => {
    expect(verdictLabel('REPRICED:173000')).toContain('۱۷۳٬۰۰۰');
    expect(verdictLabel('SKIPPED:MANUAL')).toContain('دستی');
    expect(verdictLabel(null)).toBe('—');
    expect(verdictLabel('MISSING')).toContain('قفسه');
    expect(availabilityLabel(true)).toBe('موجود');
    expect(availabilityLabel(false)).toBe('ناموجود');
    expect(availabilityLabel(null)).toBe('');
    expect(kindLabel('BelowMinMargin')).toContain('سود');
    expect(kindLabel('NoCurrentPrice')).toContain('قیمت');
    expect(kindLabel('JobBusy')).toContain('عملیات');
    expect(kindLabel('Whatever')).toBe('Whatever');
  });
});

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
