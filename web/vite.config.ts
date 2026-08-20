import { defineConfig } from 'vitest/config';
import { VitePWA } from 'vite-plugin-pwa';

// The project Pages path. Everything the app references is relative to it, which is
// also what `start_url` and `scope` in the manifest have to say (web/PLAN.md §9).
const BASE = '/techxt/';

export default defineConfig({
  base: BASE,
  build: {
    // The wasm glue wasm-pack emits for `--target web` is modern; so is every browser
    // that can run a module worker.
    target: 'es2022',
    sourcemap: true,
    rollupOptions: {
      output: {
        // Keep the woff2 faces under a `fonts/` path of their own, so the service
        // worker's runtime route and anyone reading a network log can tell an
        // unsubsetted display face from an app asset (§8.3, §9). The hash stays:
        // the CacheFirst route holds a font for a year and a new file must not be
        // shadowed by the old one.
        assetFileNames: (info) =>
          info.names?.some((n) => n.endsWith('.woff2'))
            ? 'fonts/[name]-[hash][extname]'
            : 'assets/[name]-[hash][extname]',
      },
    },
  },
  // `--target web` glue loads the .wasm through `import.meta.url`, which Vite hashes
  // and rewrites — no wasm plugin needed, in the worker as well as the main thread.
  worker: {
    format: 'es',
  },
  plugins: [
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['icons/apple-touch-icon.png', 'icon.svg', 'og.png'],
      manifest: {
        name: 'techxt — LaTeX to text',
        short_name: 'techxt',
        description:
          'Convert LaTeX-like markup to readable plain text, entirely in your browser.',
        start_url: BASE,
        scope: BASE,
        display: 'standalone',
        orientation: 'any',
        background_color: '#f0f2ee',
        theme_color: '#f0f2ee',
        categories: ['productivity', 'utilities'],
        icons: [
          { src: 'icons/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: 'icons/icon-512.png', sizes: '512x512', type: 'image/png' },
          {
            src: 'icons/icon-maskable-512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'maskable',
          },
        ],
        // W8, both additive: Android's share sheet can send selected LaTeX straight
        // into the app, and an installed copy can offer to open a .tex. A GET target
        // rather than POST, so no service-worker request handler is involved and the
        // app just reads `?text=` on load (§9, §6.4). Neither field is in
        // vite-plugin-pwa's manifest type yet, hence the cast; both are ignored by a
        // browser that does not implement them.
        ...({
          share_target: {
            action: BASE,
            method: 'GET',
            params: { text: 'text', title: 'title', url: 'url' },
          },
          file_handlers: [
            {
              action: BASE,
              accept: { 'text/x-tex': ['.tex', '.latex'], 'application/x-tex': ['.tex'] },
            },
          ],
        } as Record<string, unknown>),
      },
      workbox: {
        // The app only: shell, styles, glue and the engine. Deliberately no *display*
        // font — an unsubsetted face is several hundred KB and the page fetches the
        // one it needs anyway (§9, §8.3). The interface face is the exception: it is
        // part of the shell, applied to every screen, so an installed copy should not
        // have to draw its own chrome in a fallback the first time it opens with the
        // network off (§8.7).
        globPatterns: [
          '**/*.{html,js,css,wasm,png,svg,webmanifest}',
          'fonts/Commissioner-Variable-*.woff2',
        ],
        // The wasm module is ~890 KB today. Set the cap explicitly so future growth
        // fails the build loudly instead of silently dropping the engine from the
        // precache.
        maximumFileSizeToCacheInBytes: 2 * 1024 * 1024,
        cleanupOutdatedCaches: true,
        navigateFallback: `${BASE}index.html`,
        runtimeCaching: [
          {
            // The one runtime request the page ever makes, and it is same-origin.
            urlPattern: ({ url }: { url: URL }) =>
              url.origin === self.location.origin && url.pathname.endsWith('.woff2'),
            handler: 'CacheFirst',
            options: {
              cacheName: 'techxt-fonts',
              expiration: { maxEntries: 8, maxAgeSeconds: 60 * 60 * 24 * 365 },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
        ],
      },
      devOptions: {
        // A service worker in `vite dev` mostly gets in the way of iterating.
        enabled: false,
      },
    }),
  ],
  test: {
    environment: 'node',
    include: ['test/**/*.test.ts'],
  },
});
