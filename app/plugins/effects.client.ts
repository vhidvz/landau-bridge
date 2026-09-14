// Applies the reduced-effects mode before first paint. `?fx=lite` / `?fx=full` (dev, browser only)
// force reduced or full effects for one page load without remembering it.
export default defineNuxtPlugin(() => {
  const devOverride = import.meta.dev && !('__TAURI_INTERNALS__' in window);
  const fx = devOverride ? new URLSearchParams(location.search).get('fx') : null;
  initReducedEffects(fx === 'lite' ? true : fx === 'full' ? false : undefined);
});
