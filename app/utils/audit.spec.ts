import { describe, expect, it } from 'vitest';
import { auditSummary, costBefore, countEffect, countSummary, countedText, differenceText, doubtText, linkToday, linkUnits, renameText, transferText, writtenCost } from './audit';
import type { AuditLink, Quoted, RenameState, UnitCount, UnitTransfer } from '~/types/bridge';

const link = (over: Partial<AuditLink>): AuditLink => ({
  name: 'n', code: 'c', how: 'USER', state: 'VALID', proven: true, reasons: [], today: null, rename: null, units: 0, ...over,
});
const quoted = (over: Partial<Quoted> = {}): Quoted => ({ cost: 6_194_100, available: true, code: 'c', name: 'n', renamed: false, ...over });

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
    expect(linkToday(link({ today: { status: 'KNOWN', value: quoted() } })))
      .toBe('امروز ۶٬۱۹۴٬۱۰۰ ریال (۶۱۹٬۴۱۰ تومان)');
    expect(linkToday(link({ today: { status: 'KNOWN', value: quoted({ cost: 900, available: false }) } }))).toContain('ناموجود');
    expect(linkToday(link({ today: { status: 'KNOWN', value: null } }))).toBe('امروز در بارزپخش نیست');
    expect(linkToday(link({ today: { status: 'UNKNOWN', value: 'وصل نشد' } }))).toBe('بهای امروز نامعلوم: وصل نشد');
    expect(linkToday(link({ today: null }))).toBeNull();
  });

  // PI-29: the same sentences as the CSV's (`rename_fa` in src-tauri/src/audit.rs)
  it('names a code the site renamed, and what that does to the row', () => {
    const site = 'بست کمري 300*3/6 GSR سفيد';
    const held = (state: RenameState, asked: string | null = null) => link({ proven: state === 'CONFIRMED', rename: { state, code: 'c', name: site, asked } });
    expect(renameText(held('WAITING'))).toBe(`نام امروز این کد در بارزپخش: «${site}»؛ با قطعهٔ لاندا جور نیست و بهایش حساب نشد؛ در «تطبیق‌ها» منتظر تأیید شماست`);
    // never a promise the next run keeps: a run prices the code only once the row's other links prove
    expect(renameText(held('UNASKED'))).toBe(`نام امروز این کد در بارزپخش: «${site}»؛ با قطعهٔ لاندا جور نیست و بهایش حساب نشد؛ هنوز در «تطبیق‌ها» پرسیده نشده و کاری که بهای این کد را بخواند آن را به «تطبیق‌ها» می‌فرستد، اگر پیوند دیگری روی این ردیف اثبات نشود، پس از رفع آن`);
    // Matches asks about the name a job recorded before: said, with that name
    expect(renameText(held('ASKED_OLDER', 'نام پیشین'))).toBe(`نام امروز این کد در بارزپخش: «${site}»؛ با قطعهٔ لاندا جور نیست و بهایش حساب نشد؛ «تطبیق‌ها» هنوز نام پیشین «نام پیشین» را می‌پرسد و کاری که بهای این کد را بخواند نام امروز را جای آن می‌گذارد، اگر پیوند دیگری روی این ردیف اثبات نشود، پس از رفع آن`);
    expect(renameText(held('CONFIRMED'))).toBe(`نام امروز این کد در بارزپخش: «${site}»؛ این نام تازه را خودتان تأیید کرده‌اید`);
    // an automatic write's link is never quoted, yet the rename its row met is named
    expect(renameText(link({ today: null, rename: { state: 'WAITING', code: 'c', name: site, asked: null } }))).toContain(site);
    // a rename that agrees with the part changes nothing, and is only named
    expect(renameText(link({ today: { status: 'KNOWN', value: quoted({ name: site, renamed: true }) } }))).toBe(`نام امروز این کد در بارزپخش: «${site}»`);
    expect(renameText(link({ today: { status: 'KNOWN', value: quoted() } }))).toBeNull();
    expect(renameText(link({ today: null }))).toBeNull();
  });

  // the slice review's finding 3: a link held only for its rename still proves its units
  it('says why units under a renamed code are not counted as unproven', () => {
    const rename = { state: 'UNASKED' as const, code: 'c', name: 'n2', asked: null };
    expect(linkUnits(link({ units: 0 }))).toBeNull();
    expect(linkUnits(link({ units: 4 }))).toBe('۴ واحد از این کالا به این ردیف منتقل شده');
    expect(linkUnits(link({ units: 4, proven: false, rename }))).toBe('۴ واحد از این کالا به این ردیف منتقل شده، با نامی که در فاکتور آمده و اثبات می‌شود');
    expect(linkUnits(link({ units: 4, rename: { ...rename, state: 'CONFIRMED' } }))).toBe('۴ واحد از این کالا به این ردیف منتقل شده');
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

  it('names the automatic writes in the summary, so «all consistent» never stands alone above them', () => {
    expect(auditSummary({ rows: [], automatic: [{}, {}] as never, examined: 3, consistent: 3, unweighed: 0, unlinked: 0 }))
      .toBe('۳ ردیف سنجیده شد: همه هم‌خوان با شواهد؛ آخرین بهای ۲ ردیف را پیوند خودکار نوشته');
  });

  it('shows the cost an automatic link wrote only when the row holds another now', () => {
    expect(writtenCost({ cost: 410_000, last_write: { cost: 410_000 } as never })).toBeNull();
    expect(writtenCost({ cost: 450_000, last_write: { cost: 410_000 } as never })).toBe('نوشته‌شده ۴۱۰٬۰۰۰ ریال (۴۱٬۰۰۰ تومان)؛ بعد تغییر کرده');
  });
});

// PI-19: the seller's word that a row's stock was counted after the units listed on it
describe('stock counts', () => {
  const transfer = (over: Partial<UnitTransfer> = {}): UnitTransfer => ({
    item_id: 7, job_id: 14, at: '2026-11-10T08:00:00Z', name: 'بست کمري CW4.8*370 تايوان آراد پارت (100)', code: 'AR1330-4', qty: 4, ...over,
  });

  it('says what a count leaves out, in the CSV\'s words', () => {
    // `counted_fa` in the core, with the page's digits
    expect(countedText(4)).toBe('موجودی پس از انتقال ۴ واحد از کالاهای اثبات‌نشده را خودتان شمرده‌اید');
  });

  it('names each transfer with its units and its job', () => {
    expect(transferText(transfer())).toBe('۴ واحد در عملیات #۱۴');
    expect(transferText(transfer({ qty: -1 }))).toBe('۱ واحد برگشتی در عملیات #۱۴');
  });

  it('says before the click what a count does and does not do', () => {
    const effect = countEffect(4);
    expect(effect).toContain('۴ واحد');
    expect(effect).toContain('چیزی در لاندا نمی‌نویسد');
    expect(effect).toContain('دوباره');
    expect(effect).toContain('پس بگیرید');
    // a count of returns says so, never a minus sign (slice 11's review)
    expect(countEffect(-3)).toContain('این ۳ واحد برگشتی');
    expect(countedText(-3)).toBe('موجودی پس از انتقال ۳ واحد برگشتی از کالاهای اثبات‌نشده را خودتان شمرده‌اید');
  });

  it('sums up a standing count with the inventory read when it was made', () => {
    const c: UnitCount = { id: 1, stock_id: 's', product_name: null, inventory: 5, read_at: 't', counted_at: 't', units: 4, transfers: [transfer()] };
    expect(countSummary(c)).toBe('۴ واحد · موجودی هنگام شمارش ۵');
  });
});
