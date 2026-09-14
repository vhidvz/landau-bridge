/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { contrastRatio } from './color';

const css = readFileSync(resolve(__dirname, '../assets/css/main.css'), 'utf8');
const scale = (name: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const m of css.matchAll(new RegExp(`--color-${name}-(\\d+):\\s*(#[0-9a-fA-F]{6})`, 'g'))) out[m[1]!] = m[2]!;
  return out;
};
const firoozeh = scale('firoozeh');
const zaferan = scale('zaferan');
const petrol = scale('petrol');
const sabz = scale('sabz');
const sorkh = scale('sorkh');

const WHITE = '#FFFFFF';
const SIDEBAR_TOP = '#0B1820';
const SIDEBAR_BOTTOM = '#10222D';
const SIDEBAR_TEXT = '#C6D4DB';
const LIGHT = { page: petrol['50']!, surface: WHITE, well: petrol['100']!, text: petrol['800']!, heading: '#13232C', muted: petrol['600']! };
const DARK = { page: petrol['950']!, surface: petrol['900']!, well: '#1C2F3B', text: '#E6EEF2', heading: '#F3F7F9', muted: petrol['400']! };

const atLeast = (fg: string, bg: string, min: number) =>
  expect(contrastRatio(fg, bg), `${fg} on ${bg}`).toBeGreaterThanOrEqual(min);

describe('palette', () => {
  it('defines all five scales with 11 stops', () => {
    for (const s of [firoozeh, zaferan, petrol, sabz, sorkh]) expect(Object.keys(s)).toHaveLength(11);
  });

  it('sidebar text is readable in every state', () => {
    atLeast(WHITE, firoozeh['600']!, 4.5); // active item
    atLeast(SIDEBAR_TEXT, SIDEBAR_TOP, 4.5);
    atLeast(SIDEBAR_TEXT, SIDEBAR_BOTTOM, 4.5);
    atLeast(petrol['400']!, SIDEBAR_BOTTOM, 4.5); // version / muted line
    atLeast(zaferan['300']!, firoozeh['600']!, 3); // active icon (large, non-text)
  });

  it('light surfaces carry readable text', () => {
    for (const bg of [LIGHT.page, LIGHT.surface, LIGHT.well]) {
      atLeast(LIGHT.text, bg, 4.5);
      atLeast(LIGHT.heading, bg, 4.5);
      atLeast(LIGHT.muted, bg, 4.5);
    }
  });

  it('dark surfaces carry readable text', () => {
    for (const bg of [DARK.page, DARK.surface, DARK.well]) {
      atLeast(DARK.text, bg, 4.5);
      atLeast(DARK.heading, bg, 4.5);
      atLeast(DARK.muted, bg, 4.5);
    }
  });

  it('semantic colours used as text or button fills pass', () => {
    // solid primary button: white on firoozeh-600 (light), petrol-900 on firoozeh-400 (dark)
    atLeast(WHITE, firoozeh['600']!, 4.5);
    atLeast(petrol['900']!, firoozeh['400']!, 4.5);
    // soft badges: the --ui-* text shade on the 50 stop (light) and on the dark surface (dark)
    atLeast(firoozeh['600']!, firoozeh['50']!, 4.5);
    atLeast(zaferan['800']!, zaferan['50']!, 4.5);
    atLeast(sabz['700']!, sabz['50']!, 4.5);
    atLeast(sorkh['600']!, sorkh['50']!, 4.5);
    atLeast(petrol['600']!, petrol['100']!, 4.5);
    atLeast(firoozeh['400']!, DARK.surface, 4.5);
    atLeast(zaferan['300']!, DARK.surface, 4.5);
    atLeast(sabz['400']!, DARK.surface, 4.5);
    atLeast(sorkh['300']!, DARK.surface, 4.5);
    // control outlines (--ui-border-accented) must identify the control: WCAG 1.4.11
    atLeast(petrol['500']!, LIGHT.surface, 3);
    atLeast(petrol['500']!, DARK.surface, 3);
    // decorative dividers only need to be visible; the surface fill does the separating
    atLeast(petrol['200']!, LIGHT.page, 1.2);
    atLeast(petrol['800']!, DARK.page, 1.2);
  });
});
