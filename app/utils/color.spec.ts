import { describe, expect, it } from 'vitest';
import { contrastRatio, hexToRgb, relativeLuminance } from './color';

describe('color', () => {
  it('parses 6-digit hex', () => {
    expect(hexToRgb('#0F8B9D')).toEqual([15, 139, 157]);
    expect(hexToRgb('ffffff')).toEqual([255, 255, 255]);
  });

  it('computes WCAG relative luminance', () => {
    expect(relativeLuminance([255, 255, 255])).toBeCloseTo(1, 5);
    expect(relativeLuminance([0, 0, 0])).toBe(0);
  });

  it('computes contrast ratio, order-independent', () => {
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 2);
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 2);
    expect(contrastRatio('#777777', '#FFFFFF')).toBeCloseTo(4.48, 1);
  });
});
