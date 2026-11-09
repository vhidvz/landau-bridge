// The one-time upgrade notice (plan 130 task 14): what an upgrade from before plan 130 changed, in
// the seller's words, shown on the dashboard until dismissed. Each point appears only when it
// applies: the threshold only when the floor raised it, the waiting links only after a migration.
import type { UpgradeNotice } from '~/types/bridge';
import { faNumber } from './fa';

export const noticeTitle = (n: UpgradeNotice): string =>
  n.from_schema != null ? 'پل لاندا به‌روز شد: هر پیوند پیش از نوشتن سنجیده می‌شود' : 'آستانهٔ تطبیق خودکار بالا رفت';

export const noticeLines = (n: UpgradeNotice): string[] => {
  const out: string[] = [];
  // the setting's own name, as Settings shows it
  if (n.threshold_from != null) {
    out.push(`آستانهٔ تطبیق خودکار از ${faNumber(n.threshold_from)} به ${faNumber(n.threshold_to)} رسید؛ کمتر از ${faNumber(n.threshold_to)} پذیرفته نمی‌شود.`);
  }
  // counted now: an import judges the links it uses, a price refresh all of them
  if (n.from_schema != null) {
    out.push(n.links_unverified > 0
      ? `${faNumber(n.links_unverified)} پیوند هنوز با فهرست لاندا سنجیده نشده؛ هر عملیات پیوندهایی را که به کار می‌برد پیش از نوشتن می‌سنجد، و به‌روزرسانی قیمت همه را.`
      : 'همهٔ پیوندها با فهرست لاندا سنجیده شده‌اند؛ هر عملیات پیش از نوشتن دوباره می‌سنجد.');
  }
  out.push('صفحهٔ «بازبینی قفسه» ردیف‌هایی را که بهایشان با شواهد نمی‌خواند و هر ردیفی را که پیوند خودکار نوشته فهرست می‌کند، و چیزی نمی‌نویسد.');
  return out;
};

/** Above the list of other copies: true of any copy older than this one, never of the running build. */
export const olderCopiesText = (): string =>
  'نسخه‌های دیگری از پل لاندا روی این رایانه هست. نسخه‌ای قدیمی‌تر از این با دادهٔ به‌روزشده کار نمی‌کند — نسخه‌های ۱٫۰ تا ۱٫۱٫۱ آن را باز نمی‌کنند و نسخه‌های پیش از ۱٫۰ پیوندی در آن نمی‌یابند. اگر قدیمی‌اند، پاکشان کنید تا اشتباهی باز نشوند:';

/** The applications menu's copy, older than the running one: replaced by «نصب», never deleted. The
 *  path is shown on its own, in a left-to-right run, never inside the Persian sentence. */
export const menuCopyText = (): string =>
  'نسخهٔ منوی برنامه‌ها قدیمی‌تر از این نسخه است. پاکش نکنید: «نصب» همین نسخه را جای آن می‌گذارد.';

/** The automatic-match threshold's range (R-j): 85–100. The core clamps as well; this keeps an empty
 *  or out-of-range entry from being refused as a malformed number. */
export const clampThreshold = (v: unknown): number => {
  const n = Math.round(Number(v));
  return Number.isFinite(n) && v !== '' ? Math.min(100, Math.max(85, n)) : 85;
};

/** After a save: the value kept, when it is not the one typed — never a field still showing 70. */
export const thresholdSavedNote = (typed: unknown, saved: number): string | undefined =>
  Number(typed) === saved && typed !== '' ? undefined : `آستانهٔ تطبیق خودکار ${faNumber(saved)} ذخیره شد؛ از ۸۵ تا ۱۰۰ پذیرفته می‌شود.`;
