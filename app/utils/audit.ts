// What the audit page says (plan 130 R-i): why a shelf row is listed, how far its cost is from what
// the evidence names, each link's cost today — every figure in rials with toman beside, and an unknown
// figure said as unknown, never as zero.
import type { Audit, AuditLink, AuditRow, AutoWrite, Doubt, JobItemRow, RenamedQuote } from '~/types/bridge';
import { faNumber } from './fa';
import { money, toman } from './evidence';

const DOUBT: Record<Doubt, string> = {
  UNPROVEN_LINK: 'پیوندی روی این ردیف اثبات نمی‌شود',
  COSTS_DISAGREE: 'پیوندهای اثبات‌شدهٔ این ردیف بر سر بهای خرید هم‌نظر نیستند',
  WRITER_UNLINKED: 'آخرین بها را کالایی نوشته که دیگر به این قطعه وصل نیست',
  UNPROVEN_UNITS: 'بخشی از موجودی از کالایی منتقل شده که پیوندش اینجا اثبات نمی‌شود',
};
export const doubtText = (d: Doubt): string => DOUBT[d];

/** `should − holds`: positive when the row holds less than the evidence names. */
export const differenceText = (row: Pick<AuditRow, 'difference' | 'should_cost'>): string => {
  const d = row.difference;
  if (d == null) return row.should_cost == null ? 'شواهد بهایی نمی‌گویند' : 'روی ردیف بهایی نیست';
  if (d === 0) return 'برابر شواهد';
  const abs = Math.abs(d);
  return `${faNumber(abs)} ریال (${toman(abs)}) ${d > 0 ? 'کمتر از' : 'بیشتر از'} شواهد`;
};

export const linkToday = (l: AuditLink): string | null => {
  const t = l.today;
  if (!t) return null;
  if (t.status === 'UNKNOWN') return `بهای امروز نامعلوم: ${t.value}`;
  if (!t.value) return 'امروز در بارزپخش نیست';
  return `امروز ${money(t.value.cost)}${t.value.available ? '' : ' — ناموجود؛ بها فقط بالا می‌رود'}`;
};

// a run prices a code only once every other link on its row proves (`refresh::price_row`)
const HELD = 'با قطعهٔ لاندا جور نیست و بهایش حساب نشد';
const AFTER = 'اگر پیوند دیگری روی این ردیف اثبات نشود، پس از رفع آن';
const renameThen = (r: RenamedQuote): string => {
  switch (r.state) {
    case 'WAITING': return `${HELD}؛ در «تطبیق‌ها» منتظر تأیید شماست`;
    case 'ASKED_OLDER': return `${HELD}؛ «تطبیق‌ها» هنوز نام پیشین «${r.asked ?? ''}» را می‌پرسد و کاری که بهای این کد را بخواند نام امروز را جای آن می‌گذارد، ${AFTER}`;
    case 'UNASKED': return `${HELD}؛ هنوز در «تطبیق‌ها» پرسیده نشده و کاری که بهای این کد را بخواند آن را به «تطبیق‌ها» می‌فرستد، ${AFTER}`;
    case 'CONFIRMED': return 'این نام تازه را خودتان تأیید کرده‌اید';
  }
};

/**
 * PI-29: a code the site sells under another name today, and what PRICE makes of it — the CSV's
 * sentences (`rename_fa`). null when today's quote was not renamed.
 */
export const renameText = (l: AuditLink): string | null => {
  if (l.rename) return `نام امروز این کد در بارزپخش: «${l.rename.name}»؛ ${renameThen(l.rename)}`;
  const t = l.today;
  if (t?.status === 'KNOWN' && t.value?.renamed) return `نام امروز این کد در بارزپخش: «${t.value.name}»`;
  return null;
};

/** Beside a link's units: one held only for its renamed quote still proves them (`UNITS_UNDER_OWN_NAME`). */
export const linkUnits = (l: AuditLink): string | null => {
  if (!l.units) return null;
  const units = `${faNumber(l.units)} واحد از این کالا به این ردیف منتقل شده`;
  return l.rename && l.rename.state !== 'CONFIRMED' ? `${units}، با نامی که در فاکتور آمده و اثبات می‌شود` : units;
};

/** The cost a job found on the row, shown only when the job changed it. */
export const costBefore = (i: Pick<JobItemRow, 'cost' | 'cost_before'>): string | null =>
  i.cost_before != null && i.cost != null && i.cost_before !== i.cost ? `پیش از این ${faNumber(i.cost_before)}` : null;

export const auditSummary = (
  a: Pick<Audit, 'rows' | 'examined' | 'consistent' | 'unweighed' | 'unlinked'> & { automatic?: readonly unknown[] },
): string => {
  const head = `${faNumber(a.examined)} ردیف سنجیده شد: `;
  const parts = a.rows.length || a.unweighed
    ? [`${faNumber(a.consistent)} ردیف هم‌خوان با شواهد`, a.rows.length ? `${faNumber(a.rows.length)} ردیف نیازمند بررسی` : '',
      a.unweighed ? `${faNumber(a.unweighed)} ردیف سنجیده‌نشده (قیمت روز خوانده نشد)` : ''].filter(Boolean).join('، ')
    : 'همه هم‌خوان با شواهد';
  // the automatic writes are named here too, so «all consistent» never stands alone above that list
  const auto = a.automatic?.length ? `؛ آخرین بهای ${faNumber(a.automatic.length)} ردیف را پیوند خودکار نوشته` : '';
  return head + parts + (a.unlinked ? `؛ ${faNumber(a.unlinked)} ردیف بی‌پیوند` : '') + auto;
};

/** The cost an automatic link wrote, shown only when the row holds another now (edited since). */
export const writtenCost = (r: Pick<AutoWrite, 'cost' | 'last_write'>): string | null =>
  r.cost === r.last_write.cost ? null : `نوشته‌شده ${money(r.last_write.cost)}؛ بعد تغییر کرده`;
