import { version } from './package.json';

export default defineNuxtConfig({
  ssr: false,
  devtools: { enabled: false },
  modules: ['@nuxt/ui'],
  css: ['~/assets/css/main.css'],
  plugins: process.env.NODE_ENV === 'development' ? ['~/dev/devQuery.client'] : [],
  // Dark only: no colour-mode module, `class="dark"` is fixed on <html> (spec §1).
  ui: { colorMode: false },
  // A desktop app under a strict CSP: every icon the pages and Nuxt UI use is bundled at build
  // time from the local lucide collection; nothing is fetched from api.iconify.design at runtime.
  icon: {
    provider: 'none',
    clientBundle: { scan: true, sizeLimitKb: 512 },
  },
  // The two faces are self-hosted in `public/fonts` and declared by hand in main.css; the CSP
  // forbids external fonts anyway. `local` is the only provider, so the build never reaches out
  // to Google/Bunny/Fontshare/Fontsource/Adobe for a family name it finds in the CSS.
  fonts: { provider: 'local' },
  runtimeConfig: { public: { version } },
  app: {
    pageTransition: { name: 'page', mode: 'out-in' },
    head: {
      htmlAttrs: { dir: 'rtl', lang: 'fa', class: 'dark' },
      title: 'پل لاندا',
      meta: [{ name: 'color-scheme', content: 'dark' }],
      link: [{ rel: 'icon', type: 'image/svg+xml', href: '/brand/logo.svg' }],
    },
  },
  // Tauri's dev server contract: fixed port, no screen clearing, TAURI_* env exposed.
  vite: {
    clearScreen: false,
    envPrefix: ['VITE_', 'TAURI_'],
    server: { strictPort: true },
  },
  compatibilityDate: '2026-09-13',
});
