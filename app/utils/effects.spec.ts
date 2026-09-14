import { describe, expect, it } from 'vitest';
import { isWeakRenderer } from './effects';

describe('isWeakRenderer', () => {
  const desktopUa = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15';
  it('flags software rasterizers and Pi-class GPUs', () => {
    expect(isWeakRenderer('Mesa llvmpipe (LLVM 15.0.7, 256 bits)', desktopUa)).toBe(true);
    expect(isWeakRenderer('Google SwiftShader', desktopUa)).toBe(true);
    expect(isWeakRenderer('V3D 4.2', desktopUa)).toBe(true);
    expect(isWeakRenderer('Broadcom VideoCore VI', desktopUa)).toBe(true);
  });
  it('flags ARM boards by user agent', () => {
    expect(isWeakRenderer('Mesa Intel(R) UHD Graphics', 'Mozilla/5.0 (X11; Linux aarch64)')).toBe(true);
  });
  it('keeps a 4-core desktop with a real GPU on full effects', () => {
    expect(isWeakRenderer('Mesa Intel(R) UHD Graphics 620 (KBL GT2)', desktopUa)).toBe(false);
    expect(isWeakRenderer('ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Direct3D11 vs_5_0 ps_5_0)', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe(false);
    expect(isWeakRenderer('', desktopUa)).toBe(false);
  });
});
