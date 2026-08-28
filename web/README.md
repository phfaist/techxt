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
  index.html      the shell: header, the mount points, and the sheets' prose
  src/            the app (TypeScript, no framework)
  crate/          the wasm binding — a standalone cargo package, not a rust/ member
  fonts/          six unsubsetted woff2 faces and their licences, committed
  public/         icons and the social card, generated and committed
  tools/          dev-only checks: font packaging, glyph coverage, icons (Python)
                  and MathJax's coverage of techxt's symbols (Node)
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
node tools/mathjax_coverage.mjs         # what MathJax does not know that techxt does
node tools/mathjax_coverage.mjs --check # the same, as the CI gate
```

`mathjax_coverage.mjs` is the odd one out: it produces nothing to commit, and it runs
in CI on every push rather than when a font changes. It asks `cargo run --example
symbol_index` for every name the library defines, typesets each one under the TeX
configuration `src/mathjax.ts` exports, and fails when a construct techxt calls
mathematics is one MathJax cannot read — which, since `noundefined` makes an unknown
construct *render* rather than fail, nothing else would notice. See
[`PLAN.md`](PLAN.md) §9.1.

`fetch_fonts.py` **never subsets**. A subset is a bet on which codepoints will
appear, and this app cannot make that bet: the document is whatever the user pastes,
and the converter copies its text through — see [`PLAN.md`](PLAN.md) §8.4. Each face
ships whole, behind a CSS fallback chain, and is fetched only when it is selected.

## Deployment

`.github/workflows/web.yml` builds on every push touching `web/**` or
`rust/techxt/**`, enforces the wasm, MathJax and font size budgets and the
glyph-coverage and MathJax-coverage gates, and deploys `web/dist` to GitHub Pages on a
push to `main`.

> **One manual step, outside the repository.** Settings → Pages → Source →
> **GitHub Actions**. Until that is set, `deploy-pages` succeeds and publishes
> nothing.

## Release checklist

[`PLAN.md`](PLAN.md) §13. Several of these were driven headlessly in Chromium at W7
and are noted below with what was measured; the ones that need a real device, or a
browser engine that is not Blink, still have to be done by hand. The numbers are §13's
own; its items 8 and 9 — the library, and *Math: MathJax* — have no row here yet.

| # | Check | Status |
|---|---|---|
| 1 | Desktop: type, wrap, switch fonts, incoming share link | **Chromium: passes.** A link is 183 characters for a document plus two changed options and reproduces the session in a fresh profile. **Firefox and Safari still to do by hand.** |
| 2 | iOS Safari: install to the Home Screen, launch offline, keyboard up, copy works | **By hand — needs a device.** |
| 3 | Android Chrome: install, offline, share link from the share sheet | **By hand — needs a device.** The `share_target` itself is verified: a `?text=` visit wins over the stored document and converts on arrival. |
| 4 | DevTools offline reload after a cold cache | **Passes.** Service worker installs, and a reload with the network off serves the app and converts. |
| 5 | A 200 KB document stays responsive | **Passes, with a caveat now measured.** 157 ms in the worker, 338 ms wall including the debounce; the main thread answers in 2–20 ms throughout and a keystroke round-trips in ~100 ms. That last figure is the pane and not the conversion — 83 ms of a keystroke in a document that size goes on rebuilding the mirror behind the textarea, which highlighting adds 0.6 ms to and did not cause ([`PLAN.md`](PLAN.md) §6.12). |
| 6 | A pathological document is a diagnostic, not a dead tab | **Passes.** Every nesting shape refuses at 100–150 levels; a document 20 000 levels deep still returns a diagnostic and the same session converts afterwards. Since M9 a *macro-expanding* pathological document is covered too, and refuses sooner (65 levels of a one-construct body, 50 of a two-construct one), measured natively — see [`PLAN.md`](PLAN.md) §4.6 for both calibrations and for why the plan's byte budget had to become a depth limit. |
| 7 | Markup mixed with CJK, Hebrew and emoji: no tofu in any of the six fonts | **Passes** in all six. |
| 10 | The editor: highlighting and the completion row | **Chromium: passes**, at 1280×820 and at 390 px with touch emulation. `\alp` offers `\alpha  α` then `\alph`; Tab cycles and Shift-Tab steps back, both ends returning what was typed; Enter still inserts a newline while the row is showing; Tab with no row moves focus to the pane divider; a `\newcommand` above is offered and marked as the document's; `\begin{ali` offers five environments; a chip applies by tap, keeping the keyboard up; Ctrl+Z undoes a completion; a composition puts the plain text back and takes the colours up again afterwards. **A real IME, and a real phone keyboard, still to do by hand.** |
| 11 | Lighthouse: PWA installable, performance ≥ 95, accessibility 100 | **Accessibility: axe-core reports zero violations** at 1280×800 and 390×844. **Lighthouse itself still to run by hand.** |

To re-drive the automated ones you need a browser and Playwright, which the repository
deliberately does not depend on — install them outside it, build with `npm run build`,
serve with `npm run preview`, and drive `http://localhost:4173/techxt/`.
