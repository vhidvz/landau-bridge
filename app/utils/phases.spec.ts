/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { IMPORT_PHASES, REFRESH_PHASES, phasesOf } from './phases';

const rust = (file: string) => readFileSync(resolve(__dirname, '../../src-tauri/src/jobs', file), 'utf8');
const emitted = (src: string) => new Set([...src.matchAll(/ctx\.phase\("([A-Z]+)"\)/g)].map((m) => m[1]!));

describe('phasesOf', () => {
  it('lists the refresh phases the Rust job emits, in order', () => {
    const fromRust = new Set([...emitted(rust('mod.rs')), ...emitted(rust('refresh.rs'))]);
    expect(new Set(REFRESH_PHASES)).toEqual(fromRust);
    expect(REFRESH_PHASES).toEqual(['PREFLIGHT', 'PRICE', 'WRITE', 'REPRICE', 'REPORT']);
  });

  it('lists the import phases the Rust job emits plus REVIEW, in order', () => {
    const fromRust = new Set([...emitted(rust('mod.rs')), ...emitted(rust('import.rs')), 'REVIEW']);
    expect(new Set(IMPORT_PHASES)).toEqual(fromRust);
    expect(IMPORT_PHASES).toEqual(['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT']);
  });

  it('picks by kind', () => {
    expect(phasesOf('IMPORT')).toBe(IMPORT_PHASES);
    expect(phasesOf('REFRESH')).toBe(REFRESH_PHASES);
  });
});
