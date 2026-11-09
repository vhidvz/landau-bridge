import { describe, expect, it } from 'vitest';
import {
  confirmReasons, costChange, costChangeShort, costChangeText, money, numLabel, pickLabel, reasonText, ruleText, storedReasons, toman,
} from './evidence';
import type { Reason } from '~/types/bridge';

describe('evidence', () => {
  it('names every figure in rials with toman beside', () => {
    expect(toman(6_194_100)).toBe('۶۱۹٬۴۱۰ تومان');
    expect(money(5_500_000)).toBe('۵٬۵۰۰٬۰۰۰ ریال (۵۵۰٬۰۰۰ تومان)');
    expect(money(null)).toBe('—');
  });

  it('reads numbers as the seller does and leaves codes Latin', () => {
    expect(numLabel('44649')).toBe('۴۴۶۴۹');
    expect(numLabel('3.6 mm')).toBe('۳٫۶ میلی‌متر');
    expect(numLabel('37 cm')).toBe('۳۷ سانتی‌متر');
    expect(numLabel('xu7')).toBe('xu7');
  });

  it('says each reason with its values', () => {
    expect(reasonText({ kind: 'NUMBER_CONFLICT', lacking: ['69349'] })).toBe('عدد ناسازگار: ۶۹۳۴۹ در نام بارزپخش نیست');
    expect(reasonText({ kind: 'OPPOSITE_WORDS', supplier: 'عقب', candidate: 'جلو' })).toContain('«عقب» در بارزپخش');
    expect(reasonText({ kind: 'BRAND_MISMATCH', supplier: 'عظام', candidate: 'DPI' })).toContain('«DPI» در قطعه');
    const every: Reason[] = [
      { kind: 'BELOW_THRESHOLD', similarity: 70, threshold: 85 }, { kind: 'TIED' }, { kind: 'ANOTHER_PASSES', product_id: 'p' },
      { kind: 'NUMBER_UNBACKED', candidate: ['405'] }, { kind: 'WORDS_UNCOVERED', words: ['قدیم'] }, { kind: 'PART_WORDS_MISSING', words: ['بوش'] },
      { kind: 'NO_BRAND_IN_NAME' }, { kind: 'SEVERAL_BRANDS_IN_NAME', brands: ['FGP', 'پایا موتور'] }, { kind: 'BRAND_UNSURE', supplier: 'بوش' },
      { kind: 'CANDIDATE_WITHOUT_BRAND' }, { kind: 'DEFINITION_NOT_EXACT' },
    ];
    for (const r of every) expect(reasonText(r), r.kind).toMatch(/[؀-ۿ]/);
  });

  it('labels a verdict in one word', () => {
    expect(pickLabel({ verdict: 'WRITE' })).toBe('جور است');
    expect(pickLabel({ verdict: 'CONFIRM_THEN_WRITE', reasons: [{ kind: 'TIED' }] })).toBe('ناسازگار');
  });

  it('reads the reasons a ConfirmRequired refusal names, and nothing from another error', () => {
    const reasons: Reason[] = [{ kind: 'NUMBER_CONFLICT', lacking: ['69349'] }];
    expect(confirmReasons({ kind: 'ConfirmRequired', message: '…', detail: JSON.stringify(reasons) })).toEqual(reasons);
    expect(confirmReasons({ kind: 'Other', message: '…', detail: '[]' })).toBeNull();
    expect(confirmReasons({ kind: 'ConfirmRequired', message: '…', detail: 'not json' })).toBeNull();
    expect(confirmReasons(null)).toBeNull();
  });

  it('measures the cost change against the row Landau holds, unknown when either side is', () => {
    const row = { id: 's', inventory: 0, cost: 5_500_000, price: 6_325_000, price_source: 'RULE', read_at: '' };
    expect(costChange(6_194_100, row)).toEqual({ diff: 694_100, dir: 'up' });
    expect(costChangeText(costChange(6_194_100, row))).toBe('۶۹۴٬۱۰۰ ریال (۶۹٬۴۱۰ تومان) بیشتر');
    expect(costChangeText(costChange(5_000_000, row))).toBe('۵۰۰٬۰۰۰ ریال (۵۰٬۰۰۰ تومان) کمتر');
    expect(costChangeShort(costChange(5_000_000, row))).toBe('▼ ۵۰۰٬۰۰۰ ریال');
    expect(costChangeShort(costChange(5_500_000, row))).toBe('');
    expect(costChange(6_194_100, null)).toBeNull();
    expect(costChange(null, row)).toBeNull();
    expect(costChange(1, { ...row, cost: null })).toBeNull();
    expect(costChangeText(null)).toBe('—');
  });

  it('names a pricing rule instead of computing a price', () => {
    expect(ruleText({ scope: 'SHELF', basis: 'MARKUP_PERCENT', value: 15 })).toBe('سود ۱۵٪ روی قیمت خرید');
    expect(ruleText({ scope: 'SHELF', basis: 'FIXED_AMOUNT', value: 50000 })).toBe('۵۰٬۰۰۰ ریال روی قیمت خرید');
    expect(ruleText({ scope: null, basis: null, value: null })).toBe('قاعدهٔ قیمت‌گذاری لاندا');
  });

  it('reads a link\'s stored reasons in both shapes', () => {
    expect(storedReasons('[{"kind":"TIED"}]')).toEqual([{ kind: 'TIED' }]);
    expect(storedReasons('{"why":"gone","reasons":[{"kind":"DEFINITION_NOT_EXACT"}]}')).toEqual([{ kind: 'DEFINITION_NOT_EXACT' }]);
    expect(storedReasons(null)).toEqual([]);
    expect(storedReasons('oops')).toEqual([]);
  });
});
