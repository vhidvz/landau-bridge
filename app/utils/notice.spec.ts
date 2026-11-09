import { describe, expect, it } from 'vitest';
import { clampThreshold, menuCopyText, noticeLines, noticeTitle, olderCopiesText, thresholdSavedNote } from './notice';
import type { UpgradeNotice } from '~/types/bridge';

const notice = (over: Partial<UpgradeNotice>): UpgradeNotice => ({
  from_schema: 3, threshold_from: 70, threshold_to: 85, links_unverified: 869,
  appimages: [], menu_copy: null, ...over,
});

describe('upgrade notice', () => {
  it('names what an upgrade from before plan 130 changed: the threshold, the links, the audit', () => {
    const n = notice({});
    expect(noticeTitle(n)).toBe('پل لاندا به‌روز شد: هر پیوند پیش از نوشتن سنجیده می‌شود');
    const lines = noticeLines(n);
    expect(lines).toHaveLength(3);
    // the setting's own name, as Settings shows it (the slice review's L3)
    expect(lines[0]).toBe('آستانهٔ تطبیق خودکار از ۷۰ به ۸۵ رسید؛ کمتر از ۸۵ پذیرفته نمی‌شود.');
    // counted now, and true of an import as of a refresh (L2)
    expect(lines[1]).toBe('۸۶۹ پیوند هنوز با فهرست لاندا سنجیده نشده؛ هر عملیات پیوندهایی را که به کار می‌برد پیش از نوشتن می‌سنجد، و به‌روزرسانی قیمت همه را.');
    expect(lines[2]).toContain('بازبینی قفسه');
    expect(lines[2]).toContain('پیوند خودکار');
  });

  it('says only what applies: a threshold already at the floor, no links waiting', () => {
    const lines = noticeLines(notice({ threshold_from: null, links_unverified: 0 }));
    expect(lines).toHaveLength(2);
    expect(lines.some((l) => l.includes('آستانه'))).toBe(false);
    expect(lines[0]).toBe('همهٔ پیوندها با فهرست لاندا سنجیده شده‌اند؛ هر عملیات پیش از نوشتن دوباره می‌سنجد.');
  });

  it('on an install that only had its threshold raised, says that and where to look', () => {
    const n = notice({ from_schema: null, threshold_from: 60 });
    expect(noticeTitle(n)).toBe('آستانهٔ تطبیق خودکار بالا رفت');
    const lines = noticeLines(n);
    expect(lines[0]).toContain('از ۶۰ به ۸۵');
    expect(lines).toHaveLength(2);
    expect(lines[1]).toContain('بازبینی قفسه');
  });
});

describe('threshold range (R-j)', () => {
  it('keeps what is sent between 85 and 100, an empty or odd entry read as 85', () => {
    expect([70, 84, 85, 92.6, 100, 140].map(clampThreshold)).toEqual([85, 85, 85, 93, 100, 100]);
    expect(clampThreshold('')).toBe(85);
    expect(clampThreshold('abc')).toBe(85);
    expect(clampThreshold(Number.NaN)).toBe(85);
  });

  it('says so when what was saved is not what was typed', () => {
    expect(thresholdSavedNote(70, 85)).toBe('آستانهٔ تطبیق خودکار ۸۵ ذخیره شد؛ از ۸۵ تا ۱۰۰ پذیرفته می‌شود.');
    expect(thresholdSavedNote('', 85)).toBe('آستانهٔ تطبیق خودکار ۸۵ ذخیره شد؛ از ۸۵ تا ۱۰۰ پذیرفته می‌شود.');
    expect(thresholdSavedNote(90, 90)).toBeUndefined();
  });
});

describe('older copies (the slice review\'s H1)', () => {
  it('never says a newer version cannot use the data, and never asks to delete the menu copy', () => {
    const t = olderCopiesText();
    expect(t).toContain('۱٫۰ تا ۱٫۱٫۱');
    expect(t).not.toContain('هیچ‌کدام');
    expect(menuCopyText()).toContain('پاکش نکنید');
    expect(menuCopyText()).toContain('«نصب»');
    expect(menuCopyText()).not.toContain('/'); // the path goes in its own LTR run, never in the sentence
  });
});
