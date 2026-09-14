// Dev-only switches for screenshots, inert inside the Tauri shell and absent from the build
// (registered from nuxt.config.ts only in development):
//   ?grid=1  — draws the 8/40 px grid and the 1060 px content column (plus its 40 px gutters)
//   ?fx=lite — handled by plugins/effects.client.ts
export default defineNuxtPlugin(() => {
  if (!import.meta.dev || '__TAURI_INTERNALS__' in window) return;
  if (new URLSearchParams(location.search).get('grid') !== '1') return;
  const overlay = document.createElement('div');
  overlay.setAttribute('aria-hidden', 'true');
  overlay.style.cssText = [
    'position:fixed', 'inset:0', 'z-index:60', 'pointer-events:none',
    'background-image:linear-gradient(to right,rgb(255 255 255/.04) 1px,transparent 1px),linear-gradient(to bottom,rgb(255 255 255/.04) 1px,transparent 1px),linear-gradient(to right,rgb(255 255 255/.09) 1px,transparent 1px),linear-gradient(to bottom,rgb(255 255 255/.09) 1px,transparent 1px)',
    'background-size:8px 8px,8px 8px,40px 40px,40px 40px',
  ].join(';');
  // the main area is everything left of the 240 px sidebar (RTL); the column is centred in it
  const column = document.createElement('div');
  column.style.cssText = 'position:absolute;top:0;bottom:0;left:50%;transform:translateX(-50%);width:min(1060px,100%);border-inline:1px solid rgb(214 62 62/.45);box-sizing:border-box;';
  const gutter = document.createElement('div');
  gutter.style.cssText = 'position:absolute;top:0;bottom:0;left:40px;right:40px;border-inline:1px dashed rgb(214 62 62/.3);';
  const main = document.createElement('div');
  main.style.cssText = 'position:absolute;top:0;bottom:0;left:0;right:240px;';
  column.append(gutter);
  main.append(column);
  overlay.append(main);
  document.body.append(overlay);
});
