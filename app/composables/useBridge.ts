import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  BridgeError, Dashboard, ErrorRow, JobDetail, JobDone, JobEvent, JobItemRow, JobRow, LandauStatus, MatchRow, Progress, ReviewDecision, Scored, SettingsPatch, SettingsView, UndecidedRow, DecisionImportReport,
} from '~/types/bridge';

export const isBridgeError = (e: unknown): e is BridgeError =>
  typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;

// In `pnpm dev` opened in a plain browser there is no Tauri runtime; the fixtures in app/dev
// stand in so the pages can be looked at. The branch is dead code in the production build.
const useMock = import.meta.dev && typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in window);

/** One place every command goes through; a Rust `BridgeError` arrives as a typed rejection. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (useMock) return (await import('~/dev/mockBridge')).mockCall<T>(cmd, args);
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isBridgeError(e)) throw e;
    throw { kind: 'Other', message: String(e), detail: String(e) } satisfies BridgeError;
  }
}

export const bridge = {
  settingsGet: () => call<SettingsView>('settings_get'),
  settingsSave: (patch: SettingsPatch) => call<SettingsView>('settings_save', { patch }),
  landauValidate: () => call<LandauStatus>('landau_validate'),
  supplierTestLogin: () => call<void>('supplier_test_login'),
  dashboardGet: () => call<Dashboard>('dashboard_get'),
  importStart: (mode: 'QUANTITY' | 'ZERO', since_date?: string, fetch_limit?: number) =>
    call<number>('import_start', { mode, since_date, fetch_limit }),
  importReview: (job_id: number) => call<JobItemRow[]>('import_review', { job_id }),
  reviewDecide: (decision: ReviewDecision) => call<void>('review_decide', { decision }),
  reviewSearch: (term: string) => call<Scored[]>('review_search', { term }),
  importContinue: (job_id: number) => call<void>('import_continue', { job_id }),
  refreshStart: () => call<number>('refresh_start'),
  jobCancel: (job_id: number) => call<boolean>('job_cancel', { job_id }),
  jobCurrent: () => call<JobRow | null>('job_current'),
  jobsList: () => call<JobRow[]>('jobs_list'),
  jobGet: (job_id: number) => call<JobDetail>('job_get', { job_id }),
  matchesList: () => call<MatchRow[]>('matches_list'),
  matchSet: (name_seen: string, code: string, product_id: string, product_name: string, similarity?: number) =>
    call<void>('match_set', { name_seen, code, product_id, product_name, similarity }),
  matchesUndecided: () => call<UndecidedRow[]>('matches_undecided'),
  decisionsExport: (path: string) => call<number>('decisions_export', { path }),
  decisionsImport: (path: string, prefer_file: boolean) => call<DecisionImportReport>('decisions_import', { path, prefer_file }),
  matchNone: (name_seen: string, code: string) => call<void>('match_none', { name_seen, code }),
  matchClear: (name_seen: string) => call<void>('match_clear', { name_seen }),
  errorsList: (limit = 200) => call<ErrorRow[]>('errors_list', { limit }),
  openLogFolder: () => call<void>('open_log_folder'),
  appimageInstall: () => call<string>('appimage_install'),
  appimageDismiss: () => call<void>('appimage_dismiss'),
};

export interface JobHandlers {
  progress?: (p: Progress) => void;
  event?: (e: JobEvent) => void;
  done?: (d: JobDone) => void;
}

/** Subscribe to the three job events; returns one function that unsubscribes all. */
export async function onJobEvents(h: JobHandlers): Promise<UnlistenFn> {
  if (useMock) return (await import('~/dev/mockBridge')).mockEvents(h);
  const offs: UnlistenFn[] = [];
  if (h.progress) offs.push(await listen<Progress>('job://progress', (e) => h.progress!(e.payload)));
  if (h.event) offs.push(await listen<JobEvent>('job://event', (e) => h.event!(e.payload)));
  if (h.done) offs.push(await listen<JobDone>('job://done', (e) => h.done!(e.payload)));
  return () => offs.forEach((off) => off());
}
