/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { contrastRatio } from './color';

const css = readFileSync(resolve(__dirname, '../assets/css/main.css'), 'utf8');

/** `--color-<name>-<stop>: #hex` pairs of one ramp. */
const scale = (name: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const m of css.matchAll(new RegExp(`--color-${name}-(\\d+):\\s*(#[0-9a-fA-F]{6})`, 'g'))) out[m[1]!] = m[2]!;
  return out;
};
/** Follows `var(--color-x-N)` (a ramp stop) or `var(--color-name)` (an alias) to its hex. */
const resolveRef = (raw: string): string => {
  const viaRamp = /^var\(--color-([a-z]+)-(\d+)\)$/.exec(raw);
  if (viaRamp) return scale(viaRamp[1]!)[viaRamp[2]!]!;
  const viaAlias = /^var\(--color-([a-z-]+)\)$/.exec(raw);
  return viaAlias ? token(viaAlias[1]!) : raw;
};
/** A single `--color-<name>: …` token (alias or literal), resolved to hex. */
const token = (name: string): string =>
  resolveRef(new RegExp(`--color-${name}:\\s*([^;]+);`).exec(css)?.[1]?.trim() ?? '');
/** The value a `--ui-<name>` variable resolves to on `:root`. */
const ui = (name: string): string => {
  const root = /:root\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
  return resolveRef(new RegExp(`--ui-${name}:\\s*([^;]+);`).exec(root)?.[1]?.trim() ?? '');
};

const atLeast = (fg: string, bg: string, min: number) =>
  expect(contrastRatio(fg, bg), `${fg} on ${bg}`).toBeGreaterThanOrEqual(min);

const SURFACES = () => [token('ink-950'), token('ink-900'), token('ink-800'), token('ink-700')];

describe('palette', () => {
  it('defines the six ramps with 11 stops each', () => {
    for (const name of ['graphite', 'crimson', 'sapphire', 'sage', 'amber', 'rust']) {
      expect(Object.keys(scale(name)), name).toHaveLength(11);
    }
  });

  it('pins the surfaces and text roles to the spec values', () => {
    expect(SURFACES()).toEqual(['#0b0d12', '#10131a', '#171b24', '#232833']);
    expect(token('paper')).toBe('#f7f7f9');
    expect(token('paper-dim')).toBe(scale('graphite')['300']);
    expect(token('paper-mute')).toBe(scale('graphite')['400']);
    expect(ui('text-dimmed')).toBe(scale('graphite')['400']);
  });

  it('text roles clear 4.5:1 on all four surfaces', () => {
    for (const bg of SURFACES()) {
      for (const fg of [token('paper'), token('paper-dim'), token('paper-mute')]) atLeast(fg, bg, 4.5);
    }
  });

  it('crimson in its lit form is readable everywhere it is used', () => {
    const crimson = scale('crimson');
    for (const bg of SURFACES()) atLeast(crimson['400']!, bg, 4.5); // kicker, links
    expect(ui('primary')).toBe(crimson['400']);
    atLeast('#ffffff', crimson['600']!, 4.5); // primary button gradient, top
    atLeast('#ffffff', crimson['800']!, 4.5); // primary button gradient, bottom
  });

  it('signals are readable where they may appear', () => {
    for (const bg of SURFACES()) {
      atLeast(token('ok'), bg, 4.5);
      atLeast(token('warn'), bg, 4.5);
      atLeast(token('info'), bg, 4.5);
    }
    for (const bg of SURFACES().slice(0, 3)) atLeast(token('danger'), bg, 4.5); // never on ink-700
    expect(ui('success')).toBe(token('ok'));
    expect(ui('warning')).toBe(token('warn'));
    expect(ui('error')).toBe(token('danger'));
    expect(ui('info')).toBe(token('info'));
  });

  it('control outlines identify the control (WCAG 1.4.11)', () => {
    atLeast(token('line-bright'), token('ink-900'), 3);
    atLeast(token('line-bright'), token('ink-800'), 3);
    expect(ui('border-accented')).toBe(token('line-bright'));
  });
});
