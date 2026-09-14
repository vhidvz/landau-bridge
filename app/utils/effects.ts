/** Is this GPU/renderer pair likely to struggle with blur and glow compositing? (spec §2.4) */
export const isWeakRenderer = (renderer: string, userAgent: string): boolean =>
  /llvmpipe|softpipe|swiftshader|software/i.test(renderer) ||
  /videocore|v3d|broadcom/i.test(renderer) ||
  /aarch64|armv7|raspberry/i.test(userAgent);
