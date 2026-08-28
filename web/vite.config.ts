import { createReadStream } from 'node:fs';
import { cp, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, relative, resolve, sep } from 'node:path';

import type { Plugin } from 'vite';
import { defineConfig } from 'vitest/config';
import { VitePWA } from 'vite-plugin-pwa';

// The project Pages path. Everything the app references is relative to it, which is
// also what `start_url` and `scope` in the manifest have to say (web/PLAN.md §9).
const BASE = '/techxt/';

/* ------------------------------------------------------------------- MathJax */

const require = createRequire(import.meta.url);

/**
 * MathJax's version, read from the package rather than written down twice, and the
 * directory it is served from — `mathjax/4.1.3/…`, version and all (web/PLAN.md §9.1).
 *
 * The version *is* the cache key. These files are copied verbatim rather than passed
 * through Rollup, so there is no content hash to bust the year-long `CacheFirst` route
 * with; an upgrade changes the directory instead, which is the same discipline the
 * hashed font faces get and the reason a stale range file cannot outlive the engine
 * that understands it.
 */
const MATHJAX_VERSION = (require('mathjax/package.json') as { version: string }).version;
const MATHJAX_DIR = `mathjax/${MATHJAX_VERSION}`;

/**
 * The TeX extensions that must be served from our own origin, by their file name under
 * `mathjax/input/tex/extensions/`.
 *
 * `tex-chtml.js` carries `ams`, `newcommand`, `configmacros`, `noundefined`,
 * `textmacros`, `require` and `autoload`; every other package is a file of its own that
 * MathJax's loader fetches from `loader.paths.mathjax`, which is us (§9.1). So a package
 * named in `src/mathjax.ts`'s `TEX_INPUT.packages` and *not* copied here would be a 404
 * at startup on a page whose whole promise is that it asks nobody for anything.
 *
 * Two of these three are the packages the coverage measurement chose (§9.1);
 * `boldsymbol` is here because
 * `mathtools` depends on it, which is the kind of thing only the loader knows —
 * `tools/mathjax_coverage.mjs` runs MathJax under the app's own package list, asks the
 * loader what it loaded, and fails if the answer is not covered by this list.
 */
export const MATHJAX_TEX_EXTENSIONS: readonly string[] = [
  'mathtools',
  'upgreek',
  'boldsymbol',
];

/**
 * The trees and files copied under it: the combined TeX→CHTML bundle, the TeX extensions
 * above, the font's dynamically loaded character ranges, and the woff2 faces those
 * ranges are metrics for.
 *
 * The ranges are the surprise in MathJax 4 (see §9.1): the
 * bundle carries the common characters, and a formula reaching outside them —
 * `\mathbb{R}`, `\mathcal{H}` — asks for one more file, which by default comes from
 * jsdelivr. Serving the whole set ourselves is what keeps "no CDN, ever" true. Everything
 * here is lazily fetched and never precached, so the weight is in `dist/` and almost none
 * of it on the wire: a reader who opens every shipped example fetches eleven of these
 * files and 414 KB (§14), plus the three extensions — 27 006 B on disk, 9 658 B gzipped —
 * which are fetched at startup rather than on demand, because a package the configuration
 * names has to be there before the first formula is.
 *
 * Under the SVG output this was two trees and 11.8 MB, because an SVG range carries
 * glyph outlines where a CHTML range carries metrics and lets a woff2 face do the
 * drawing. §9.1 has the comparison that moved it.
 *
 * `to` is a URL path — always `/`, never the platform separator — because it is matched
 * against a request before it is turned into a filename.
 */
const MATHJAX_ASSETS: readonly { readonly from: string; readonly to: string }[] = [
  { from: require.resolve('mathjax/tex-chtml.js'), to: 'tex-chtml.js' },
  ...MATHJAX_TEX_EXTENSIONS.map((name) => ({
    from: require.resolve(`mathjax/input/tex/extensions/${name}.js`),
    to: `input/tex/extensions/${name}.js`,
  })),
  {
    from: join(
      dirname(require.resolve('@mathjax/mathjax-newcm-font/package.json')),
      'chtml',
      'dynamic',
    ),
    to: 'mathjax-newcm-font/chtml/dynamic',
  },
  {
    from: join(
      dirname(require.resolve('@mathjax/mathjax-newcm-font/package.json')),
      'chtml',
      'woff2',
    ),
    to: 'mathjax-newcm-font/chtml/woff2',
  },
];

/** Where `url` lands under {@link MATHJAX_ASSETS}, or `null` if it is not one of ours. */
function mathjaxAsset(url: string): string | null {
  const prefix = `${BASE}${MATHJAX_DIR}/`;
  if (!url.startsWith(prefix)) return null;
  const wanted = url.slice(prefix.length);
  for (const { from, to } of MATHJAX_ASSETS) {
    if (wanted === to) return from;
    if (!wanted.startsWith(`${to}/`)) continue;
    // Resolve, then check containment: a `..` in the request must not escape the tree.
    const file = resolve(from, wanted.slice(to.length + 1));
    const inside = relative(from, file);
    if (inside && !inside.startsWith(`..${sep}`) && inside !== '..') return file;
  }
  return null;
}

/**
 * Put MathJax in `dist/` at a stable, version-stamped path, and serve it in `vite dev`
 * from `node_modules` so the mode is usable without a build.
 *
 * Rollup is deliberately not involved. The bundle is a script that configures itself
 * from `window.MathJax` and installs globals; `src/mathjax.ts` injects it with a
 * `<script>` tag when the user first asks for MathJax, which is what keeps a megabyte
 * out of the app's own bundle and off every other visitor's first paint.
 */
function mathjax(): Plugin {
  return {
    name: 'techxt:mathjax',
    // Before Vite's own middlewares, so the SPA fallback does not answer with the app.
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const file = mathjaxAsset((req.url ?? '').split('?')[0] ?? '');
        if (!file) return next();
        res.setHeader(
          'content-type',
          file.endsWith('.woff2') ? 'font/woff2' : 'text/javascript; charset=utf-8',
        );
        createReadStream(file)
          .on('error', next)
          .pipe(res);
      });
    },
    async writeBundle(options) {
      const root = join(options.dir ?? 'dist', ...MATHJAX_DIR.split('/'));
      for (const { from, to } of MATHJAX_ASSETS) {
        const target = join(root, ...to.split('/'));
        await mkdir(dirname(target), { recursive: true });
        await cp(from, target, { recursive: true });
      }
    },
  };
}

export default defineConfig({
  base: BASE,
  // `src/mathjax.ts` builds every MathJax URL from this, so the version-stamped
  // directory is written down once, here, and read from the package.
  define: {
    __MATHJAX_DIR__: JSON.stringify(MATHJAX_DIR),
  },
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
    mathjax(),
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
        // MathJax is `.js` and would otherwise be swept into the precache by the
        // pattern above — a megabyte of typesetter plus half a megabyte of metric
        // ranges, on the install path of every visitor, most of whom never turn the mode
        // on. It is a runtime asset instead, held by the `techxt-mathjax` route below
        // (§9.1). The `woff2` faces beside them are not matched by the pattern at all,
        // but they are under `mathjax/` and so covered here too, which is what should
        // happen: they belong to the same asset and the same route.
        globIgnores: ['**/node_modules/**/*', 'mathjax/**'],
        // The wasm module is ~950 KiB today (it was ~890 KB before M9 linked techy-xp in
        // and 1.18 MiB before the size pass took `opt-level = "s"`). Set the cap
        // explicitly so future growth fails the build loudly instead of silently
        // dropping the engine from the precache.
        maximumFileSizeToCacheInBytes: 2 * 1024 * 1024,
        cleanupOutdatedCaches: true,
        navigateFallback: `${BASE}index.html`,
        runtimeCaching: [
          {
            // The display face in use. Same-origin, like everything else here, and
            // matched on the `fonts/` directory `assetFileNames` above puts the display
            // faces in rather than on the `.woff2` extension: since the typesetter
            // became CHTML its own faces are woff2 too, and an eight-entry cache shared
            // with a hundred-odd MathJax faces would evict the display face on sight.
            // The two are different assets with different lifetimes; they get different
            // routes. Same serialization rule as the MathJax matcher below — nothing in
            // this module is in scope where this runs.
            urlPattern: ({ url }: { url: URL }) =>
              url.origin === self.location.origin &&
              url.pathname.endsWith('.woff2') &&
              url.pathname.includes('/fonts/'),
            handler: 'CacheFirst',
            options: {
              cacheName: 'techxt-fonts',
              expiration: { maxEntries: 8, maxAgeSeconds: 60 * 60 * 24 * 365 },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
          {
            // The typesetter, the metric ranges and the woff2 faces it asks for, on
            // first use of the MathJax math mode (§9.1). Same shape as the font route
            // above, and for the same reason: once it has been fetched the mode works
            // offline. 160 entries is the bundle plus all 40 ranges plus all 105 faces
            // with room to spare — a cap that could evict would produce exactly the
            // offline hole this route exists to close, and the whole set is only about
            // 3.2 MB. The version-stamped path is what makes a year-long cache safe.
            //
            // **This function is serialized into `sw.js` by its source**, so nothing
            // in this module is in scope where it runs: a `${BASE}` here compiled to a
            // reference to a variable the service worker does not have, the matcher
            // threw on every request, and the route silently never matched — which is
            // to say the mode worked online and not offline, in exactly the way this
            // route exists to prevent. The base is read from the worker's own
            // registration scope instead, which is `BASE` by construction and needs
            // nothing from here.
            urlPattern: ({ url }: { url: URL }) =>
              url.href.startsWith(
                `${(self as unknown as { registration: { scope: string } }).registration.scope}mathjax/`,
              ),
            handler: 'CacheFirst',
            options: {
              cacheName: 'techxt-mathjax',
              expiration: { maxEntries: 160, maxAgeSeconds: 60 * 60 * 24 * 365 },
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
