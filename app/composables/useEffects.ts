/**
 * Reduced-effects ("fx-lite") mode: a `fx-lite` class on <html> swaps translucent blurred panels
 * for solid ones and stops decorative animation (see main.css). Chosen per device — stored in
 * localStorage, auto-detected on first run. Ported from Ledger POS.
 */
const STORAGE_KEY = 'landau-bridge:reduced-effects';

function detectWeakGpu(): boolean {
  try {
    const canvas = document.createElement('canvas');
    const gl = (canvas.getContext('webgl') ?? canvas.getContext('experimental-webgl')) as WebGLRenderingContext | null;
    if (!gl) return true; // no GL at all: rendering already falls back to software
    const dbg = gl.getExtension('WEBGL_debug_renderer_info');
    const renderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : '';
    return isWeakRenderer(renderer, navigator.userAgent);
  } catch {
    return true;
  }
}

export const useReducedEffects = () => useState<boolean>('fx-reduced', () => false);

const applyClass = (reduced: boolean) => document.documentElement.classList.toggle('fx-lite', reduced);

/** Called once from the client plugin: `force` (dev screenshots) > stored choice > auto-detect. */
export function initReducedEffects(force?: boolean) {
  const stored = localStorage.getItem(STORAGE_KEY);
  const reduced = force ?? (stored !== null ? stored === '1' : detectWeakGpu());
  useReducedEffects().value = reduced;
  applyClass(reduced);
}

/** Explicit user choice (Settings → نمایش): apply now and remember on this device. */
export function setReducedEffects(reduced: boolean) {
  useReducedEffects().value = reduced;
  localStorage.setItem(STORAGE_KEY, reduced ? '1' : '0');
  applyClass(reduced);
}

export function reducedEffectsActive(): boolean {
  return typeof document !== 'undefined' && document.documentElement.classList.contains('fx-lite');
}

/** True when the OS asks for reduced motion or fx-lite is on. */
export function prefersReducedMotion(): boolean {
  return typeof window === 'undefined' || window.matchMedia('(prefers-reduced-motion: reduce)').matches || reducedEffectsActive();
}
