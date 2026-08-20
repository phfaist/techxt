# techxt in the browser

A static, installable single-page app that converts LaTeX-like markup to plain text
using a wasm build of `techxt`, and doubles as the project's home page at
<https://phfaist.github.io/techxt/>.

Everything runs on the device: no server, no upload, no telemetry, and — after the
first load — no network. [`PLAN.md`](PLAN.md) in this folder is the normative design
for the app and is where a question about *why* something is the way it is gets
answered; the root [`PLAN.md`](../PLAN.md) is normative for the library.

## Layout

```
web/
  index.html      the shell: header, four mount points, the below-the-fold prose
  src/            the app (TypeScript, no framework)
  crate/          the wasm binding — a standalone cargo package, not a rust/ member
  fonts/          five unsubsetted woff2 faces and their licences, committed
  public/         icons and the social card, generated and committed
  tools/          dev-only Python: font packaging, glyph coverage, icons
  test/           vitest over the pure logic
```

## Prerequisites

- **Rust** (stable) with the `wasm32-unknown-unknown` target and
  [`wasm-pack`](https://rustwasm.github.io/wasm-pack/):
  ```sh
  rustup target add wasm32-unknown-unknown
  cargo install wasm-pack --locked
  ```
- **Node 22** and npm.
- Python 3 with `fonttools` and `brotli` — **only** to re-run the scripts in
  `tools/`. Their outputs are committed, so an ordinary build needs neither.

**The `rust/` MSRV does not apply here.** `rust/`'s 1.86 floor exists for library
consumers; `web/crate/` is a leaf artifact built by CI on stable and may use whatever
`wasm-bindgen` requires. This is a deliberate exception, not an oversight
([`PLAN.md`](PLAN.md) §3).

## Develop

```sh
cd web
npm install
npm run dev          # builds the wasm module, then starts Vite
```

| script | does |
|---|---|
| `npm run wasm` | `wasm-pack build crate --target web --release --out-dir pkg` |
| `npm run dev` | `npm run wasm` then `vite` |
| `npm run build` | `npm run wasm`, `tsc --noEmit`, `vite build` |
| `npm run preview` | serve `dist/` as it will be deployed |
| `npm test` | `vitest run` |
| `npm run typecheck` | `tsc --noEmit` |
| `npm run fonts` | re-obtain `fonts/` (dev only — see below) |

A change under `rust/techxt` needs `npm run wasm` re-run before Vite sees it. To
iterate on both sides at once:

```sh
cargo watch -w ../rust/techxt -w crate/src -s 'npm run wasm'
```

`crate/pkg/` and `crate/target/` are generated and git-ignored; `crate/Cargo.lock` is
committed, so a deploy resolves the same `techy` revision the plan was written
against.

### If `wasm-opt` fails

`wasm-pack` ships binaryen 117, which predates the bulk-memory operations current
rustc emits by default. The `[package.metadata.wasm-pack.profile.release]` block in
`crate/Cargo.toml` passes `--enable-bulk-memory --enable-nontrapping-float-to-int
--enable-sign-ext` for exactly that reason; if you are tempted to delete those flags,
[`PLAN.md`](PLAN.md) §4.7 and Appendix B explain what breaks. `wasm-opt = false` is
the fallback and costs little.

## The dev-only scripts

Their outputs are committed. Run them when a font is added or updated, or when the
icon changes — not as part of a build.

```sh
pip install fonttools brotli
python3 tools/fetch_fonts.py            # obtain and re-package fonts/
python3 tools/fetch_fonts.py --check    # verify the committed files against SOURCES.md
python3 tools/coverage_check.py --check # the CI glyph-coverage gate
python3 tools/make_icons.py             # icon.svg → public/icons/ and public/og.png
```

`fetch_fonts.py` **never subsets**. A subset is a bet on which codepoints will
appear, and this app cannot make that bet: the document is whatever the user pastes,
and the converter copies its text through — see [`PLAN.md`](PLAN.md) §8.4. Each face
ships whole, behind a CSS fallback chain, and is fetched only when it is selected.

## Deployment

`.github/workflows/web.yml` builds on every push touching `web/**` or
`rust/techxt/**`, enforces the wasm and font size budgets and the glyph-coverage
gate, and deploys `web/dist` to GitHub Pages on a push to `main`.

> **One manual step, outside the repository.** Settings → Pages → Source →
> **GitHub Actions**. Until that is set, `deploy-pages` succeeds and publishes
> nothing.

## Release checklist

Run these by hand before calling a release done ([`PLAN.md`](PLAN.md) §13):

1. Desktop Chrome, Firefox and Safari: type, wrap, switch fonts, share-link round trip.
2. iOS Safari: install to the Home Screen, launch offline, keyboard up, copy works.
3. Android Chrome: install, offline, share link from the share sheet.
4. DevTools offline reload after a cold cache.
5. A 200 KB document: typing stays responsive, the status line shows the time.
6. A pathological document (deeply nested braces, `\frac` 200 deep): a diagnostic,
   not a dead tab — the descent-guard calibration of [`PLAN.md`](PLAN.md) §4.6.
7. A document mixing `\emph{…}`, CJK, Hebrew and emoji: no tofu in any of the six
   display-font settings.
8. Lighthouse: PWA installable, performance ≥ 95, accessibility 100.
