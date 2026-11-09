import { describe, expect, it } from 'vitest';
import { auditSummary, costBefore, differenceText, doubtText, linkToday } from './audit';
import type { AuditLink } from '~/types/bridge';

const link = (over: Partial<AuditLink>): AuditLink => ({
  name: 'n', code: 'c', how: 'USER', state: 'VALID', proven: true, reasons: [], today: null, units: 0, ...over,
});

describe('audit', () => {
  it('says why a row is listed in the seller\'s words', () => {
    expect(doubtText('UNPROVEN_LINK')).toBe('پیوندی روی این ردیف اثبات نمی‌شود');
    expect(doubtText('COSTS_DISAGREE')).toContain('هم‌نظر نیستند');
    expect(doubtText('WRITER_UNLINKED')).toContain('دیگر به این قطعه وصل نیست');
    expect(doubtText('UNPROVEN_UNITS')).toContain('موجودی');
  });

  it('names the gap between the row and the evidence, in rials with toman beside', () => {
    // the incident's bearing row: 5,500,000 held, 6,194,100 by the evidence
    expect(differenceText({ difference: 694_100, should_cost: 6_194_100 })).toBe('۶۹۴٬۱۰۰ ریال (۶۹٬۴۱۰ تومان) کمتر از شواهد');
    expect(differenceText({ difference: -15_000, should_cost: 1 })).toBe('۱۵٬۰۰۰ ریال (۱٬۵۰۰ تومان) بیشتر از شواهد');
    expect(differenceText({ difference: 0, should_cost: 1 })).toBe('برابر شواهد');
    expect(differenceText({ difference: null, should_cost: null })).toBe('شواهد بهایی نمی‌گویند');
    expect(differenceText({ difference: null, should_cost: 5 })).toBe('روی ردیف بهایی نیست');
  });

  it('shows a link\'s cost today, or why there is none — never a zero', () => {
    expect(linkToday(link({ today: { status: 'KNOWN', value: { cost: 6_194_100, available: true } } })))
      .toBe('امروز ۶٬۱۹۴٬۱۰۰ ریال (۶۱۹٬۴۱۰ تومان)');
    expect(linkToday(link({ today: { status: 'KNOWN', value: { cost: 900, available: false } } }))).toContain('ناموجود');
    expect(linkToday(link({ today: { status: 'KNOWN', value: null } }))).toBe('امروز در بارزپخش نیست');
    expect(linkToday(link({ today: { status: 'UNKNOWN', value: 'وصل نشد' } }))).toBe('بهای امروز نامعلوم: وصل نشد');
    expect(linkToday(link({ today: null }))).toBeNull();
  });

  it('shows the cost a job found on a row only when it changed', () => {
    expect(costBefore({ cost_before: 5_500_000, cost: 6_194_100 })).toBe('پیش از این ۵٬۵۰۰٬۰۰۰');
    expect(costBefore({ cost_before: 165_000, cost: 165_000 })).toBeNull();
    expect(costBefore({ cost_before: null, cost: 165_000 })).toBeNull();
    expect(costBefore({ cost: 165_000 })).toBeNull();
  });

  it('sums an audit up in one line', () => {
    expect(auditSummary({ rows: [{}, {}] as never, examined: 6, consistent: 4, unweighed: 0, unlinked: 1 }))
      .toBe('۶ ردیف سنجیده شد: ۴ ردیف هم‌خوان با شواهد، ۲ ردیف نیازمند بررسی؛ ۱ ردیف بی‌پیوند');
    expect(auditSummary({ rows: [], examined: 3, consistent: 3, unweighed: 0, unlinked: 0 }))
      .toBe('۳ ردیف سنجیده شد: همه هم‌خوان با شواهد');
    // a row whose quote could not be read is never counted as consistent
    expect(auditSummary({ rows: [], examined: 3, consistent: 2, unweighed: 1, unlinked: 0 }))
      .toBe('۳ ردیف سنجیده شد: ۲ ردیف هم‌خوان با شواهد، ۱ ردیف سنجیده‌نشده (قیمت روز خوانده نشد)');
  });
});
