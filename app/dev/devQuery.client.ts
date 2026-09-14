// Dev-only query switches for screenshots (rewritten in Task 3). Inert inside the Tauri shell.
export default defineNuxtPlugin(() => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
});
