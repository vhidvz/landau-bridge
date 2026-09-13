import { describe, expect, it } from 'vitest';
import { faNumber, rials, latinDigits, normalizeJalali, todayJalali, jalaliYearStart, verdictLabel, kindLabel } from './fa';

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
    expect(kindLabel('NoCurrentPrice')).toContain('قیمت');
    expect(kindLabel('Whatever')).toBe('Whatever');
  });
});
