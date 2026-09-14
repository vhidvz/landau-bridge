// Dev-only: `?mode=dark|light|system` picks the colour mode for screenshots. Registered from
// nuxt.config.ts only when NODE_ENV is development, and inert inside the Tauri shell.
export default defineNuxtPlugin((nuxtApp) => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
  const mode = new URLSearchParams(location.search).get('mode');
  if (mode === 'dark' || mode === 'light' || mode === 'system') {
    (nuxtApp.$colorMode as { preference: string }).preference = mode;
  }
});
