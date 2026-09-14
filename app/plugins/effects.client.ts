// Applies the reduced-effects mode before first paint. `?fx=lite` (dev, browser only) forces it
// for one page load without remembering it.
export default defineNuxtPlugin(() => {
  const forced = import.meta.dev && !('__TAURI_INTERNALS__' in window) && new URLSearchParams(location.search).get('fx') === 'lite';
  initReducedEffects(forced ? true : undefined);
});
