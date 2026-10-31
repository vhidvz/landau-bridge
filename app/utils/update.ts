import type { InstallKind, UpdateState, UpdateView } from '~/types/bridge';
import { faNumber, faVersion, type Tone } from './fa';

/** The Settings card's status line for the app's own version. */
export const updateLabel = (s: UpdateState, kind: InstallKind): { text: string; tone: Tone } => {
  switch (s.state) {
    case 'Idle': return { text: 'آخرین نسخه را دارید', tone: 'ok' };
    case 'Checking': return { text: 'در حال بررسی نسخهٔ جدید…', tone: 'info' };
    case 'Available': return { text: `نسخهٔ ${faVersion(s.version)} پیدا شد`, tone: 'info' };
    case 'Downloading': return { text: `در حال دریافت نسخهٔ ${faVersion(s.version)} (${faNumber(s.percent)}٪)`, tone: 'info' };
    case 'Ready':
      // deb and rpm ask for an administrator's password, so they never install on their own;
      // an unknown install (a development build) installs only from the button
      if (kind === 'unknown') return { text: `نسخهٔ ${faVersion(s.version)} آماده نصب است.`, tone: 'ok' };
      return kind === 'deb' || kind === 'rpm'
        ? { text: `نسخهٔ ${faVersion(s.version)} آماده است؛ نصب آن رمز مدیر سیستم را می‌خواهد.`, tone: 'warn' }
        : { text: `نسخهٔ ${faVersion(s.version)} آماده است؛ با بستن برنامه نصب می‌شود.`, tone: 'ok' };
    case 'Installing': return { text: `در حال نصب نسخهٔ ${faVersion(s.version)}…`, tone: 'info' };
    case 'Failed': return { text: s.message, tone: s.kind === 'Signature' ? 'danger' : 'warn' };
  }
};

export const canInstallNow = (v: UpdateView): boolean => v.state.state === 'Ready';
