import type { Phase } from '~/types/bridge';

/** The phases each job kind walks through, in order; REVIEW only happens when an import has undecided items. */
export const IMPORT_PHASES: readonly Phase[] = ['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];
export const REFRESH_PHASES: readonly Phase[] = ['PREFLIGHT', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];

export const phasesOf = (kind: 'IMPORT' | 'REFRESH'): readonly Phase[] => (kind === 'IMPORT' ? IMPORT_PHASES : REFRESH_PHASES);
