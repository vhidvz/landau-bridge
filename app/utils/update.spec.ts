import { describe, expect, it } from 'vitest';
import { canInstallNow, updateLabel } from './update';
import type { UpdateView } from '~/types/bridge';

const view = (state: UpdateView['state'], kind: UpdateView['kind'] = 'appimage'): UpdateView =>
  ({ current: '1.0.0', state, kind, auto_update: true, last_check: null });

describe('update', () => {
  it('names each state in Persian, versions in Persian digits', () => {
    expect(updateLabel({ state: 'Idle' }, 'appimage')).toEqual({ text: 'آخرین نسخه را دارید', tone: 'ok' });
    expect(updateLabel({ state: 'Checking' }, 'appimage').text).toBe('در حال بررسی نسخهٔ جدید…');
    expect(updateLabel({ state: 'Downloading', version: '1.0.1', percent: 42 }, 'appimage').text)
      .toBe('در حال دریافت نسخهٔ ۱.۰.۱ (۴۲٪)');
    expect(updateLabel({ state: 'Installing', version: '1.0.1' }, 'nsis').text).toBe('در حال نصب نسخهٔ ۱.۰.۱…');
  });

  it('says how a ready version goes in, by install kind', () => {
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'appimage'))
      .toEqual({ text: 'نسخهٔ ۱.۰.۱ آماده است؛ با بستن برنامه نصب می‌شود.', tone: 'ok' });
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'nsis').text).toBe('نسخهٔ ۱.۰.۱ آماده است؛ با بستن برنامه نصب می‌شود.');
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'deb'))
      .toEqual({ text: 'نسخهٔ ۱.۰.۱ آماده است؛ نصب آن رمز مدیر سیستم را می‌خواهد.', tone: 'warn' });
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'rpm').tone).toBe('warn');
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'unknown').text).toBe('نسخهٔ ۱.۰.۱ آماده نصب است.');
  });

  it('with automatic updates off, a ready version waits for the button', () => {
    expect(updateLabel({ state: 'Ready', version: '1.0.1' }, 'appimage', false))
      .toEqual({ text: 'نسخهٔ ۱.۰.۱ آماده است؛ با دکمهٔ «نصب و راه‌اندازی دوباره» نصب می‌شود.', tone: 'ok' });
  });

  it('shows a failure as the core worded it, as a warning', () => {
    const failed = { state: 'Failed', kind: 'Network', message: 'اتصال به GitHub برقرار نشد؛ بعداً دوباره بررسی می‌شود.', at: '2026-10-31T10:00:00Z' } as const;
    expect(updateLabel(failed, 'appimage')).toEqual({ text: failed.message, tone: 'warn' });
    expect(updateLabel({ ...failed, kind: 'Signature', message: 'امضا' }, 'appimage').tone).toBe('danger');
  });

  it('offers the install button only for a ready version', () => {
    expect(canInstallNow(view({ state: 'Ready', version: '1.0.1' }))).toBe(true);
    expect(canInstallNow(view({ state: 'Downloading', version: '1.0.1', percent: 3 }))).toBe(false);
    expect(canInstallNow(view({ state: 'Idle' }))).toBe(false);
  });
});
