# techxt web — implementation plan

The normative design for `web/`: a static, installable single-page app that converts
LaTeX-like markup to plain text in the browser, using a wasm build of `techxt`, and
that doubles as the project's home page at `https://phfaist.github.io/techxt/`.

Root [`PLAN.md`](../PLAN.md) is normative for the library; this file is normative for
the app and defers to it on everything about conversion behaviour.

## 1. Goals and non-goals

**The page is a tool, not a brochure.** A visitor lands on an input box with a
document already in it and an output pane beside it; conversion happens as they type.
The project framing — name, one sentence, GitHub link — is a header strip, and the
prose lives in sheets over the tool where it cannot cost it a screenful. The page
itself never scrolls: the tool is exactly one viewport, and About and Install are
dialogs (§6.8).

Goals, in priority order:

1. Paste LaTeX, read text, copy it out. Fast, on a phone, with the keyboard up.
2. Expose the conversion options that change the answer, without burying the three
   that change it most (wrapping, math rendering, display font).
3. Show techxt's diagnostics as the structured, positioned things they are — this is
   the feature that distinguishes it from a macro stripper, and a demo that hides it
   undersells the library.
4. Work offline, installable, no server, no telemetry, no document ever leaving the
   device. A converter people paste unpublished papers into must be able to say this
   plainly.
5. Be a credible landing page: what techxt is, how to get the crate and the CLI.

Non-goals: rendering LaTeX visually; a file tree or multi-file projects; `\input`
resolution (there is no filesystem — the diagnostic explains it); editing features
beyond a textarea (no syntax highlighting, no CodeMirror); accounts; server-side
anything.

## 2. Decisions taken

| # | Decision | Consequence to manage |
|---|---|---|
| D1 | The wasm binding crate lives at `web/crate/`, so the whole app is one deletable folder | It is outside the `rust/` workspace, so `rust/`'s CI gates do not see it — §11 adds `fmt`/`clippy`/`test` for it to the web workflow |
| D2 | Vite + TypeScript + `vite-plugin-pwa` | `node_modules` and a lockfile enter the repo; the JS toolchain needs periodic bumping. Buys hashed assets, a real dev server, a generated service worker, and types from wasm-pack's `.d.ts` |
| D3 | Conversion runs in a Web Worker, debounced-live | A message protocol and a respawn path (§6.2); in exchange the UI cannot be frozen by a pathological document |
| D4 | Five self-hosted **unsubsetted** woff2 faces, lazily fetched on selection, each backed by a fallback chain; monospace default | Several hundred KB per face, so lazy loading and the runtime cache carry real weight (§8.3); in exchange, arbitrary input renders and there is no CDN |
| D5 | Three primary controls, the rest behind "More options" | Two-tier options model (§5) |
| D6 | `localStorage` for session state, URL fragment for sharing | A versioned state codec (§6.4) |
| D7 | Diagnostics in a collapsible panel, click to select the span in the input | The binding must return UTF-16 offsets, not byte offsets (§4.4) |
| D8 | App fills the viewport; header is one line; About/Install are modal sheets and the page never scrolls | Mobile layout has to work with the on-screen keyboard up (§6.6) |

## 3. Folder layout

```
web/
  PLAN.md                 this file
  README.md               how to develop, build and deploy
  index.html              the shell (Vite entry)
  package.json            scripts and dependencies
  package-lock.json       committed
  tsconfig.json
  vite.config.ts
  src/
    main.ts               bootstrap and wiring
    state.ts              option model, defaults, localStorage, URL codec
    convert-client.ts     worker lifecycle, debounce, request sequencing
    types.ts              app-level types shared by the codec and the UI
    worker/
      convert.worker.ts   loads wasm, answers convert requests
      protocol.ts         message types shared by both sides
    ui/
      api.ts              what main.ts may assume about the four modules below
      panes.ts            input/output panes, resize, autofit measurement
      controls.ts         primary bar + "More options" disclosure
      diagnostics.ts      the diagnostics panel and jump-to-source
      toast.ts            copy confirmation, update-available notice
    fonts.ts              font registry (family, metrics class, warnings)
    examples.ts           the sample documents, inlined
    about.ts              below-the-fold content, or plain markup in index.html
    styles.css            tokens, layout, light/dark
  test/                   vitest over the pure logic (§13)
  public/
    icons/                PWA icons (generated, committed)
    og.png                social preview (generated, committed)
  fonts/
    *.woff2                five display faces (§8.1) and the interface face (§8.7)
    licences/              each upstream OFL / GUST licence, verbatim
  tools/
    fetch_fonts.py        obtains and re-packages web/fonts/ (no subsetting)
    coverage_check.py     reports each face's coverage of techxt's repertoire
    make_icons.py         icon/og generation from icon.svg
  crate/                  the wasm binding — a standalone cargo package
    Cargo.toml
    Cargo.lock            committed, for reproducible deploys
    .cargo/config.toml    wasm stack size
    src/lib.rs            the wasm-bindgen surface
    src/options.rs        the options DTO and its mapping to techxt
    src/diag.rs           the diagnostic DTO, offsets, line/column
    pkg/                  wasm-pack output — generated, gitignored
```

Two files are contracts rather than implementation, and exist because the app is
written by more than one pair of hands at a time. `src/types.ts` holds the app-level
types the state codec and the UI both need — including the two settings of §5 that
are the *app* being helpful rather than the library offering a choice. `src/ui/api.ts`
holds the interfaces the four UI modules satisfy and `main.ts` programs against, so
neither side can drift without `tsc` saying so.

`web/crate/` is deliberately *not* a member of the `rust/` workspace and is not
reachable from it: the repository root has no `Cargo.toml`, so a package under `web/`
is standalone and cannot perturb `cd rust && cargo test`. It depends on `techxt` by
relative path (`../../rust/techxt`) and resolves `techy` itself, into its own
committed `Cargo.lock`.

**MSRV does not apply here.** `rust/`'s 1.86 floor exists for library consumers;
`web/crate/` is a leaf artifact built by CI on stable and may use whatever
`wasm-bindgen` requires. This must be stated in `web/README.md` so the exception does
not read as an oversight.

**Relation to the planned `js/` package.** Root PLAN §17 anticipates a published
`js/` wasm/Node binding. `web/crate/` is *not* that: it is app-private, shaped for
this UI, and free to change without a release. If `js/` ever exists, `web/` switches
to depending on it and `web/crate/` is deleted — one of the reasons for keeping the
binding surface small.

## 4. The wasm binding (`web/crate/`)

### 4.1 Surface

Three exports, no more:

```rust
#[wasm_bindgen]
pub struct Session { /* cached Converter + the options it was built from */ }

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session;

    /// Convert `latex` under `options` (a plain JS object). Returns a
    /// `ConversionResult` object; never throws for a document-level failure —
    /// a strict-mode parse error comes back as a result with `ok: false` and one
    /// diagnostic.
    pub fn convert(&mut self, latex: &str, options: JsValue) -> Result<JsValue, JsValue>;
}

/// Version string of the embedded techxt, for the About section and bug reports.
#[wasm_bindgen]
pub fn techxt_version() -> String;
```

`Session` holds one `Converter` plus a hash of the options it was built from, and
rebuilds only when they change. Measured cost of a rebuild is ~1.2–1.9 ms natively
(§14), so this is a small optimisation, not a necessary one — but it makes typing
under fixed options cost exactly one `latex_to_text` call.

### 4.2 Options in

Options arrive as a plain JS object and are deserialized with `serde` +
`serde-wasm-bindgen` into an `OptionsDto` whose every field is `#[serde(default)]`
and whose enums are lowercase-kebab strings (`"numbered-underlined"`). The DTO is
then mapped onto `ConverterBuilder` in one `fn build(dto: &OptionsDto) ->
Result<Converter, String>`.

`serde` here is a *binding* dependency; `techxt` itself keeps its exactly-two
runtime dependencies (root PLAN §2), which the `rust/` CI continues to enforce.

Two properties the mapping must have, both unit-testable natively (no wasm needed):

- **Total.** Every field of `techxt::convert::Options` is either mapped or listed in
  a `// not exposed:` comment with the reason. A new library option should make this
  file's reviewer notice.
- **Defaults are the library's.** An absent field means "whatever `Options::default()`
  says", never a value re-typed here. The UI sends only what the user changed.

### 4.3 Result out

```ts
interface ConversionResult {
  ok: boolean;              // false only for a hard parse failure (strict mode)
  text: string;             // '' when !ok
  ms: number;               // conversion time, for the status line
  diagnostics: Diagnostic[];
  suppressed: number;       // Diagnostics::suppressed() — "and N more"
  truncated: boolean;       // suppressed > 0
}

interface Diagnostic {
  severity: 'error' | 'warning' | 'note';
  identifier: string;       // e.g. "techxt.unknown-macro"
  message: string;
  rendered: string;         // Diagnostic::render() — the CLI's full text, for details
  span: null | {            // null when the span is not in the current input (§4.5)
    start: number;          // UTF-16 code-unit offset — see §4.4
    end: number;
    line: number;           // 1-based
    column: number;         // 1-based, in characters
  };
  frames: { title: string; span: Span | null }[];   // the include/expansion trace
}
```

Diagnostics are emitted in `Diagnostics::sorted_by_position()` order so the panel
matches reading order. `Diagnostics::DEFAULT_LIMIT` is 1000; beyond that techy counts
rather than stores, which is what `suppressed`/`truncated` report.

### 4.4 Byte offsets are not JS offsets

techy spans are **byte** offsets into UTF-8. `textarea.setSelectionRange` wants
**UTF-16 code-unit** offsets. Converting in JS means re-walking the string per
diagnostic and getting surrogate pairs wrong at least once; the binding does it in
Rust instead, in a single pass:

1. Collect every byte offset needed (each diagnostic's `start`/`end`, plus frames).
2. Sort them.
3. Walk `latex.char_indices()` once, accumulating `ch.len_utf16()` and a line/column
   counter, emitting the mapped values as each collected offset is passed.

So JS receives offsets it can use directly, and line/column for display. Property
test: for random strings containing astral characters (`𝕏`, emoji) and CRLF, the
mapped offset of every char boundary equals the offset JS computes for the same
prefix.

### 4.5 Spans that are not in the buffer

A diagnostic's span points into a `Source`, which need not be the document the user
typed — a synthesized source, or (in principle) an `\input`ed one. The binding
compares the span's source against the one it created for this call and emits
`span: null` otherwise, so the panel never scrolls the textarea to a meaningless
offset. Those diagnostics still show their message and `rendered` text.

### 4.6 Recursion, stack and panics

- `web/crate/.cargo/config.toml` raises the wasm stack from its 1 MiB default:
  `rustflags = ["-C", "link-arg=-zstack-size=8388608"]` (8 MiB).
- ~~**Use a byte budget, not a depth limit.**~~ **Revised at W7 by measurement — use a
  depth limit.** The original reasoning was that techy's `StdDescentGuard` estimates
  stack use by *address distance*, with no operating-system call anywhere, so what
  `techxt-cli` has and wasm lacks is only the *probe* behind `computed_stack_budget`
  and a fixed byte budget should work here exactly as it does natively. That is wrong,
  and the reason is worth keeping: **address distance measures the shadow stack in
  linear memory, and the shadow stack is not the one that runs out.** Only
  address-taken locals live there; an ordinary Rust local becomes a wasm local, held in
  the engine's own call stack, which linear memory cannot see and `-zstack-size` cannot
  size. Measured in Chromium, every deeply-nested shape died as a `RangeError: Maximum
  call stack size exceeded` — an engine limit of roughly 1 MB a module cannot raise —
  with the 6 MiB byte budget untouched and the guard silent. The budget was not too
  generous; it was watching the wrong stack.
- **So: `StdDescentGuardInit::depth_limit(300)`**, calibrated below. A depth limit is
  also engine-independent in a way a byte budget is not, which matters because Safari's
  and Firefox's stack limits are neither Chromium's nor probeable from here. The 8 MiB
  shadow stack stays — it costs nothing and address-taken data below the parse still
  lives there.
- **Leaving the default would be wrong.** `StdDescentGuardInit::default()` is
  `fixed_stack_budget(250 KiB)` *marked unconfigured*: deliberately tight — on the
  order of ten nesting levels in a debug build — and it emits a self-describing
  refusal aimed at an embedder who has not chosen. We are that embedder.
- **Calibration (W7, Chromium 140, binary-searched, one fresh module instance per
  probe — a trap poisons the whole instance, not just the `Session`).** Input nesting
  depth at which an *unguarded* conversion killed the instance, and at which
  `depth_limit(300)` refuses instead:

  | shape | engine dies at | refuses at | descents/level | margin |
  |---|---|---|---|---|
  | `\sqrt{…}` | 577 | 100 | 3 | 5.8× |
  | `\textbf{…}`, `x^{…}` | 585 | 100 | 3 | 5.9× |
  | `\frac{…}{2}` | 593 | 100 | 3 | 5.9× |
  | `\begin{itemize}\item …` | 670 | 100 | 3 | 6.7× |
  | `{…}`, `${…}$` | 993 | 149–150 | 2 | 6.6× |

  A document **20 000 levels** deep still comes back as
  `core.constructs.descent-limit-exceeded` rather than a trap, and the same `Session`
  converts an ordinary document correctly afterwards — which is checklist item 6 of
  §13. If the margin ever looks thin, lower the limit: the stack cannot be raised at
  all here, so the margin is the whole of the safety.
- A panic — or a genuine overflow — leaves the wasm instance unusable. The worker
  catches it, posts `{type: 'fatal'}`, and the client discards and respawns the worker
  (§6.2). `console_error_panic_hook` is installed in all builds: a panic report from a
  real user's document is worth its ~10 KB.

### 4.7 Build profile

```toml
[profile.release]
opt-level = 3; lto = true; codegen-units = 1; panic = "abort"; strip = true

[package.metadata.wasm-pack.profile.release]
wasm-opt = ["-O3", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext"]
```

The `--enable-*` flags are **required**, not cosmetic: wasm-pack ships binaryen 117,
which predates the bulk-memory operations current rustc emits by default, and without
them `wasm-opt` fails validation and the build breaks (verified locally). If a future
wasm-pack ships a newer binaryen the flags become harmless.

`opt-level = 3` is the choice: conversion speed is what a person feels while typing,
and 296 KB gzipped is not a heavy page. `"s"` and `"z"` are measured alternatives
(§14) held in reserve — if the module grows past roughly 400 KB gzipped, or a phone
profile shows the download hurting more than the speed helps, drop to `"s"` first
(`"z"` costs the most speed for the last few tens of kilobytes). The CI size budget
of §11 is what makes that growth visible.

### 4.8 Tests

Native `cargo test` in `web/crate` (the mapping and offset code is ordinary Rust):

- Options DTO: empty object → `Options::default()`; every enum string round-trips;
  an unknown enum string is a clean `Err`, not a panic.
- Offsets: the property test of §4.4.
- Diagnostics: an `\undefinedmacro` document yields one `techxt.unknown-macro`
  warning whose span selects exactly `\undefinedmacro` in the input.
- Strict mode: a malformed document yields `ok: false` with one error diagnostic.

Plus one `wasm-bindgen-test` smoke test that `Session::convert` round-trips through
`JsValue` under `wasm-pack test --headless --firefox`, run locally rather than in CI
unless it proves cheap.

## 5. The option model

**Primary bar** (always visible, one row on desktop, wrapping to two on a phone):

| Control | Maps to | Default |
|---|---|---|
| Wrap: Fit / Off / 40 / 60 / 72 / 80 / custom | `wrap_width(Option<usize>)` | **Fit** (see below) |
| Math: Fancy / Plain / Source | `math_mode` | Fancy |
| Display font: JuliaMono / Fira Math / Latin Modern / STIX Two / Libertinus / System | CSS only | JuliaMono |

*Fit* is an app-level value, not a library one: it measures the output pane and sends
the resulting column count (§6.5). The library default is `None` (no wrapping), which
is right for a CLI writing to a file and wrong for a pane on a phone, where unwrapped
output means horizontal scrolling. The distinction is visible in the UI: *Off* is the
library default, *Fit* is the app being helpful.

**"More options"** — a `<details>` disclosure, three fieldsets:

*Layout*: heading style (`heading_style`, 4 values) · footnote style
(`footnote_style`, 3) · keep comments (`keep_comments`) · **text char styles**
(`text_font`: on / off — off means `\textbf` stops producing 𝐛𝐨𝐥𝐝) · `\today`
(browser date / `<today>` / custom → `today(Option<Box<str>>)`) · keep all fonts
offline (§8.3, app-level).

*Math*: expression delimiters (`math_expression_in`: parens / braces / none) · matrix
delimiters (`matrix_delimiters`: unicode / ascii) · **math char styles** (`math_font`:
italic (default) / upright / off, and the other Unicode alphabets for anyone who wants
their variables in 𝔣𝔯𝔞𝔨𝔱𝔲𝔯).

The two `*_font` options are Unicode *character* styles — which alphabet a letter is
mapped into — and have nothing to do with the display font of the primary bar, which
is CSS and changes nothing about the text. The labels keep them apart: **display
font** versus **text/math char styles**.

*Parsing*: unknown macros (`unknown_macro`, 4 values) · unknown environments
(`unknown_env`, 3) · unknown specials (`unknown_specials`, 2) · strict
(`recovery(Recovery::Strict)`).

**Not exposed**, each with a comment in `options.rs` saying so: `list_style` (two
arrays of strings — a UI in itself, and rarely the thing someone came to change),
`unknown_macro_resolution` (subtle interaction with `recovery`; the "strict" checkbox
covers the observable case), `descent_guard` (a safety limit, not a preference),
`source_resolver` (no filesystem in a browser), `override_*` and custom definitions
(an extension API, not an option).

`\today` deserves its one line of code: the browser *has* a clock, so the app can
send a real date where the no_std library must render `<today>`. Format matches
`techxt-cli`'s (`"August 20, 2026"`, month names spelled out) so the app and the CLI
agree; the app uses the *local* date, since a person looking at their own screen has
no use for the CLI's UTC caution.

## 6. The app

### 6.1 Shape

```
┌──────────────────────────────────────────────────────────┐
│ techxt   LaTeX-like markup → plain text        [GitHub]  │  header, one line
├──────────────────────────────────────────────────────────┤
│ Wrap [Fit ▾] Math [Fancy ▾] Display font [JuliaMono ▾] ▸ More│  primary bar (sticky)
├───────────────────────────┬──────────────────────────────┤
│ LaTeX             [Load ▾]│ Text        [Copy] [Download]│  pane headers
│                           │                              │
│ <textarea>                │ <pre>                        │
│                           │                              │
├───────────────────────────┴──────────────────────────────┤
│ ▸ 3 warnings · 128 ms · 1 240 chars                      │  status + diagnostics
└──────────────────────────────────────────────────────────┘
            (About and Install are sheets over this, not a page under it)
```

Panes are a CSS grid, `1fr 1fr` on desktop with a draggable divider (the ratio is
persisted); stacked at `max-width: 860px`. The app region is `height: 100dvh` minus
header, so `dvh` handles the mobile URL bar.

### 6.2 Worker protocol (`src/worker/protocol.ts`)

```ts
type ToWorker   = { type: 'convert'; id: number; text: string; options: OptionsPayload };
type FromWorker = { type: 'ready'; version: string }
                | { type: 'result'; id: number; result: ConversionResult }
                | { type: 'fatal';  message: string };
```

Client rules:

- One worker, created eagerly at load (`new Worker(new URL('./worker/convert.worker.ts',
  import.meta.url), { type: 'module' })`; `worker.format = 'es'` in `vite.config.ts`).
- Requests carry a monotonic `id`; a result whose `id` is not the latest is dropped.
  There is no cancellation in wasm, so a superseded conversion runs to completion and
  its answer is discarded — acceptable at the measured speeds.
- Debounce 120 ms after the last keystroke; convert immediately on an option change,
  on paste, and on Ctrl/Cmd+Enter.
- A conversion still running after 300 ms shows a subtle "converting…" state; after
  1500 ms a **Cancel** button appears, which terminates the worker, respawns it, and
  restores the last good output.
- `fatal` ⇒ same respawn path plus a toast inviting a bug report with the input link
  (§6.4) — a panic is a techxt bug and the link is the reproduction.

Requests send full text (`postMessage` copies the string; at document sizes that
matter this is microseconds compared to conversion).

### 6.3 Rendering the output

`<pre>` with `white-space: pre`, `overflow: auto`, `tab-size: 8`. Never
`white-space: pre-wrap`: the library decided the line breaks, and a second, invisible
wrapping by the browser would misrepresent the output — with *Wrap: Off* the correct
behaviour is a horizontal scrollbar. `textContent` assignment only (no `innerHTML`).

Copy uses `navigator.clipboard.writeText` with a `<textarea>`+`execCommand` fallback
for older iOS; Download builds a `Blob` and a temporary object URL, named after the
first `\title`/`\section` if one exists, else `converted.txt`.

### 6.4 State, persistence and sharing

One versioned object:

```ts
interface AppState { v: 1; doc: string; opts: AppOptions; ui: UiState }

// AppOptions is Partial<Options> plus the two app-level settings of §5, which are not
// library options and must not be sent to the binding as if they were:
//   wrap: 'fit' | 'off' | number        →  wrapWidth, once the pane has been measured
//   todayMode: 'browser' | 'library' | 'custom' (+ todayCustom)  →  today
// `resolveOptions(opts, columns)` in state.ts is the single place that translation
// happens, so the worker never sees an app-level value.
```

- **localStorage**, debounced 500 ms, three keys (`techxt.doc.v1`, `techxt.opts.v1`,
  `techxt.ui.v1`); the document is capped at 512 KB with the excess simply not stored
  (and a note in the status line), so a huge paste cannot break the quota and lose the
  settings too.
- **Share link**: `#d=` + base64url(`deflate-raw`(JSON of `{v, doc, opts}`)) via
  `CompressionStream`, with an uncompressed base64url fallback where it is missing.
  Read on load, and written only into a crash report (§6.2), which is the one place
  the app needs a reproduction it can hand to someone — never on every keystroke, so
  the URL and the history stay stable. Over ~8000 characters the settings-only
  encoding is used instead, since browsers and chat clients start mangling longer
  URLs. There is no "copy link" control: a link that carries a document silently is
  a thing to be asked for, not a button to press by accident.
- On load: fragment (if present) wins over localStorage, which wins over defaults.
  The fragment is left in place so a reload reproduces it.
- Only options that differ from `Options::default()` are serialized, so a link stays
  short and a future change of a library default is picked up rather than frozen.

Unit-tested (vitest): round-trip of every field, tolerance of a truncated or corrupt
fragment (fall back to defaults, never throw), and version-mismatch handling.

### 6.5 Fit-to-pane wrapping

*Fit* asks the library to wrap at a column count, so the app turns a pane width in
pixels into columns. Measure a representative sample — the 62 alphanumerics and a
space, rendered in a hidden span with the output's computed style — and divide by its
length for a mean advance; columns = `max(20, floor(paneContentWidth * 0.98 /
advance))`. For a monospace face the mean advance *is* the advance and the fit is
exact; for a proportional one it is a good estimate, and the 2 % margin plus the
pane's horizontal scroll absorb the occasional long line.

Cached per (display font, size) pair. Re-measured on a debounced (100 ms)
`ResizeObserver`, on font or size change, and on orientation change; a conversion is
re-issued only when the column count actually changes.

### 6.6 Mobile

- `<textarea autocapitalize="off" autocorrect="off" autocomplete="off"
  spellcheck="false" inputmode="text">` — an editor that capitalises `\alpha` is
  worse than useless.
- Stacked panes, each `min-height: 38dvh`; a **⇅ Focus** button maximises one pane
  and remembers which.
- `visualViewport` `resize`/`scroll` listener keeps the primary bar and the copy
  button reachable with the keyboard up.
- Touch targets ≥ 44 px; the divider is desktop-only.

### 6.7 Examples and the first visit

An empty output pane is a bad first impression and a bad demo, so a first visit (no
fragment, no stored document) loads example 1 with the library defaults, converted
before the user touches anything.

`src/examples.ts` holds five short documents, inlined as string constants so they
cost no fetch and work offline. Each is at most ~15 lines — a demo, not a corpus:

1. **A paper fragment** (the default): `\section`, `\emph`, an accent, an inline
   formula, a `\footnote`, a `\cite`. Shows the headline behaviours in one screen.
2. **Mathematics**: sums with limits, a fraction, a square root, Greek, a `matrix`,
   a display equation — the case for `math_mode` and for the display fonts.
3. **Lists and tables**: nested `itemize`/`enumerate` and a `tabular` that aligns.
4. **Accents and symbols**: `\"o`, `\'e`, `\c{c}`, `\ss`, dashes, quotes,
   `\alpha…\omega`, arrows — the long tail, and a font stress test.
5. **Unicode passthrough**: a paragraph mixing LaTeX markup with CJK, Hebrew and an
   emoji — the case the fallback chains of §8.2 exist for, and the one a reviewer
   should look at before believing them.

A **Load ▾** menu in the input pane header offers them; choosing one replaces the
document (with a single-level undo via the toast, since it discards work).

### 6.8 The two sheets

The page does not scroll. The tool is one viewport tall and everything else is a
`<dialog>` opened with `showModal()` over it — the top layer, the backdrop, Escape,
the focus ring and the inertness of the tool behind are the platform's, and none of
them is reimplemented (`src/ui/sheets.ts`). A sheet is a card on the desktop and the
whole screen on a phone. The header's **About** and **Install** are buttons, not
anchors: the fragment belongs to the share codec (§6.4), and a nav link that
overwrites it would cost a reader their document on reload.

**About**, short and written once: what techxt is (two sentences, adapted from the
repository README); the crate and CLI snippets from the README, verbatim so they
cannot drift into being wrong; a link to the repository, to `CHANGELOG.md` and to the
design notes; the privacy line ("everything runs in your browser — no document is
ever uploaded, and the page makes no network requests after it loads"); font credits
with their licences (§8.1, §8.7); and the embedded techxt version from
`techxt_version()`, which is what makes a bug report actionable.

**Install** explains how to install the app on the device that is reading it. The
`beforeinstallprompt` event is captured at module scope — Chrome fires it once, early,
and an event nobody listened for is gone — and offers a real install button where the
browser has one to give. Everything else is prose per platform: the steps are chosen
by user-agent sniffing, which is the right tool exactly once, since "tap Share, then
Add to Home Screen" is true of Safari on iOS and of nothing else and no feature test
will tell you where a menu item is. The other platforms stay one disclosure away, so
a wrong guess costs a click rather than the answer, and an already-installed copy is
told so instead (`display-mode: standalone`).

### 6.9 Accessibility

Sheets are real `<dialog>`s, so the tool behind one is inert and Escape closes it
without a keydown handler of ours. Labelled controls (`<label for>`, no
placeholder-as-label); the diagnostics summary
is `aria-live="polite"` and announces counts, not every keystroke; visible focus
rings; light/dark via `prefers-color-scheme` with CSS custom properties and AA
contrast in both; `prefers-reduced-motion` respected by every transition and the one animation.
Keyboard: Ctrl/Cmd+Enter converts, Ctrl/Cmd+Shift+C copies output, Escape closes the
options disclosure.

## 7. The diagnostics panel

Collapsed by default, summarised in the status bar as
`▸ 2 errors · 3 warnings · 128 ms`, with severity-coloured counts (and a neutral
"clean" state when there are none — the absence of diagnostics is information too).

Expanded, each row is: severity chip · `identifier` in monospace · message ·
`line:column`. Clicking a row focuses the textarea and
`setSelectionRange(start, end)`s the span (§4.4), scrolling it into view; rows whose
`span` is `null` are not clickable and say why. An expander per row reveals techy's
own `rendered` form, which carries the caret line and the trace frames — the same
text `techxt-cli` prints, which makes a screenshot from the web app directly
comparable to a terminal report.

Filter chips (errors / warnings / notes) with counts, defaulting to showing
everything. When `truncated`, a final row reads "and N more (retention limit)".

## 8. Display fonts

### 8.1 The registry (`src/fonts.ts`)

Five self-hosted faces plus the system stack, each shipped **whole** — see §8.4 for
why nothing is subsetted. The selection criterion is coverage of what techxt itself
emits: the Mathematical Alphanumeric Symbols, the sub/superscripts, the stacked
delimiter pieces.

> **Measured at W5, and it does not hold for all five.** Of the 1 349 distinct
> codepoints techxt's own repertoire emits, Fira Math covers 834 — it has bold and
> double-struck but **no script and no fraktur alphabet, and no sub/superscript
> digits** (140 missing in the Mathematical Alphanumeric block alone). Latin Modern
> Math covers 965, but 175 of its 384 gaps are Cyrillic, which is passthrough text
> rather than something techxt emits. STIX Two Math covers 1 241 and Libertinus Math
> 1 245.
>
> Nothing here renders as a box — the chains of §8.2 fall back per glyph — so the cost
> is a *mixed* face in `\mathfrak{A}` rather than tofu, which §8.2 argues is the right
> outcome. But the sentence that used to stand here ("a face that cannot render those
> is not offered, which is why the selector needs no warnings next to any entry") was
> written before the measurement and Fira Math contradicts it. **This is a decision
> for the owner, not for the implementation** (Appendix D: dropping a face needs the
> same conversation as adding one). The three options are: keep it and accept the
> fallback, keep it with a note in the selector, or drop it. Until that is settled it
> stays, because it is the only humanist sans in the list and the fallback is honest.

| id | face | character | licence |
|---|---|---|---|
| `julia` | JuliaMono Regular | monospace; the default | OFL 1.1 |
| `firamath` | Fira Math Regular | humanist sans | OFL 1.1 |
| `lmmath` | Latin Modern Math | Computer Modern — the LaTeX look | GUST Font Licence |
| `stix` | STIX Two Math | Times-like journal serif | OFL 1.1 |
| `libertinus` | Libertinus Math | modern serif | OFL 1.1 |
| `system` | the platform stack | zero download | — |

Latin Modern Math, STIX Two Math and Libertinus Math are OpenType *math* fonts, which
suits this exactly: their Latin, Greek, Mathematical Alphanumeric Symbols and tall
delimiter pieces all live in the ordinary `cmap`, which is all a CSS `font-family`
consults. Their MATH tables are dead weight for a CSS `font-family`, but they are a
small part of the file and nothing is stripped (§8.4).

JuliaMono is the default because a monospace grid is what the library's own column
arithmetic (`unicode-width`) is computed against, so tables, matrices and heading
rules line up exactly as the layout engine intended. The others are offered on equal
footing — a serif face reads better for prose-heavy output, and that is a real
preference, not a compromise.

One weight each: techxt expresses boldness with Unicode alphabets (𝐛𝐨𝐥𝐝), never with
font weight, so a bold face would never be used. A size control (12–20 px) sits beside
the selector, and the chosen face applies to the output pane alone — the source pane
is code, and stays in the platform monospace at a size of its own.

### 8.2 Every face is a chain, not a font

No font covers Unicode, and techxt copies input text straight through: paste a
Japanese abstract, a Hebrew quotation or an emoji and it is in the output. So a
display font is never declared alone — each entry in the registry is a CSS chain
ending in the platform's own coverage:

```css
font-family: "JuliaMono", ui-monospace, SFMono-Regular, Menlo, Consolas,
             "Noto Sans", "Noto Sans CJK JP", "Apple Color Emoji",
             "Segoe UI Emoji", sans-serif;
```

Browsers fall back **per glyph**, not per element, so the chosen face renders
everything it has and the system supplies the rest. A mixed-script document therefore
renders as a mix of faces, which is exactly right: legible beats uniform, and the
alternative is a box. The serif entries end in a serif system stack so the fallback
does not clash.

One consequence worth knowing rather than fixing: the layout engine measures columns
with `unicode-width`, which counts East Asian wide characters as two columns. Whether
a table containing them *looks* aligned then depends on the fallback font honouring
that same convention — most CJK-capable system fonts do.

### 8.3 Lazy loading

Each face is declared with `@font-face` and `font-display: swap`, and a declared face
that nothing applies is never fetched — the browser's own laziness is the entire
mechanism, so there is no loader to write. Selecting a face applies it;
`document.fonts.load('16px "…"')` gives the promise behind a brief "loading font…"
state in the pane header.

`src: local("Latin Modern Math"), url("/fonts/…woff2")` — `local()` first, so the
TeX users who already have these installed download nothing at all.

Offline follows from §9: no face is precached, and every face is runtime-cached on
first use — so the default lands in the cache on first paint, and any other face the
moment it is chosen. **Keep all fonts offline** in More options fetches all five
deliberately, for someone about to board a plane. If a face was never fetched and the
network is gone, the swap simply does not happen and the chain of §8.2 renders —
nothing to handle.

Unsubsetted faces are large — expect roughly 250–700 KB of woff2 each (measured and
recorded in §14 at W5). Only the *selected* face is ever fetched, so first load costs
one of them; the rest arrive if and when someone goes looking. This is what turns
lazy loading from a nicety into the thing that makes five whole fonts affordable.

### 8.4 Packaging (`tools/fetch_fonts.py`)

**Nothing is subsetted.** A subset is a bet on which codepoints will appear, and this
app cannot make that bet: the document is whatever the user pastes, and the converter
copies its text through. A block list chosen from techxt's own repertoire would render
techxt's own symbols beautifully and turn a Chinese author's name into boxes — the one
failure mode the font work exists to prevent. The chain of §8.2 handles what the face
genuinely lacks; the face itself ships whole.

So the script only *obtains* fonts, and never edits glyphs:

1. Where upstream publishes woff2 (JuliaMono does), ship that file byte for byte.
2. Otherwise convert the upstream otf with `fontTools.ttLib.woff2` — a container
   change, no glyph, `cmap` or metric touched — and append " Web" to name IDs 1/4/6/16,
   the conservative reading of the OFL's Reserved Font Name clause and of the GUST
   licence's LPPL ancestry for a re-packaged file.
3. Record each source URL, version and SHA-256 in `web/fonts/SOURCES.md`, and copy the
   upstream licence verbatim into `web/fonts/licences/`.

Outputs are **committed**, so an ordinary build needs neither Python nor fonttools;
the script is a dev aid, like `tools/gen_symbols.py`, run when a font is added or
updated. Licences are credited in About.

### 8.5 Coverage report (`tools/coverage_check.py`)

With whole fonts and a fallback chain, a gap is a cosmetic mix of faces rather than a
box — so this stops being a safety net and becomes the evidence behind §8.1's
selection criterion, and the one hard gate on the default face:

1. Generate an "everything" document — every macro name in
   `rust/techxt/src/defs/symbols_extra_data.rs` and `accents_data.rs`, every font-style
   macro (`\mathbb`, `\mathfrak`, `\textsf`, …) applied to `Aa Zz 0-9`, a matrix, a
   table, a nested list.
2. Convert it with the built CLI (`cargo run -q --bin techxt`).
3. Collect the codepoints of the output and report, per face, which of them the
   `cmap` lacks. **Fails** if the default face (JuliaMono) is missing any beyond a
   named baseline — the out-of-the-box rendering of techxt's own output is not allowed
   to regress.

   > The baseline exists because measurement found the default face already has two
   > gaps: **U+301A/U+301B**, LEFT/RIGHT WHITE SQUARE BRACKET, which
   > `\openbracketleft`/`\openbracketright` map to. JuliaMono has U+27E6/U+27E7, the
   > mathematical white square brackets, and not the CJK-punctuation ones. A gate that
   > is red on the day it is written teaches people to ignore it, so the two are a
   > justified `KNOWN_DEFAULT_GAPS` constant in the script and anything beyond them
   > fails; a gap that later closes is reported so the constant can be trimmed.
   >
   > **Worth raising as a library question** (Appendix D — not something the app may
   > fix by reaching into the crate): whether `\openbracketleft` should map to U+27E6
   > rather than U+301A. pylatexenc's choice is the CJK codepoint; U+27E6 is what the
   > construct means and what math fonts carry.
   **Warns**, with the list written to the job summary, for the other four: a handful
   of gaps filled by the fallback chain is a documented fact about a face, not a bug,
   and only a face with a large or ugly gap comes off the list.

Runs in CI (`--check`); cost is fonttools plus one cargo run.

### 8.6 Icons

`tools/make_icons.py` renders `icon.svg` to 192/512/maskable-512 PNGs and a 180 px
apple-touch-icon, plus `og.png` (1200×630) for social previews. Outputs committed;
the script is a dev aid. The mark: `∑` converting to `S`-shaped text, or simply
`𝕥` — decided when it is drawn, not in this plan.

### 8.7 The interface face

The five faces above are *display* faces: the user picks one and it renders the
converted text. The app's own chrome — labels, prose, diagnostics — is set in one
more, **Commissioner** (Kostas Bartsokas, OFL), and it is deliberately not part of
the registry in `src/fonts.ts`: it is never offered, never persisted, never named in
a share link, and `coverage_check.py` skips it, because what it does with a fraktur
alphabet is nobody's business.

It is shipped like the others — whole, pinned, hashed, licence copied, in
`fetch_fonts.py` and `SOURCES.md` — with three differences that follow from its being
chrome rather than content:

- **One file, every weight.** The variable cut (`wght` 100–900, 170 KB of woff2)
  costs less than the three static weights the interface uses would, and comes with
  Commissioner's `FLAR` and `VOLM` axes. `styles.css` sets a little of both — a face
  chosen rather than defaulted to, and not enough to notice at 13 px.
- **No `local()` arm.** §8.3's first arm finds an installed original; an installed
  Commissioner would be a static instance, and the rule promises a range.
- **It is precached.** §8.3's laziness is for faces nobody may pick. This one is on
  every screen, so it belongs to the shell: an installed copy should not have to draw
  its own chrome in a fallback the first time it opens with the network off.

## 9. PWA

- **Manifest** (via `vite-plugin-pwa`): `name: "techxt — LaTeX to text"`,
  `short_name: "techxt"`, `start_url: "/techxt/"`, `scope: "/techxt/"`,
  `display: "standalone"`, theme/background from the CSS tokens (both schemes),
  the icon set of §8.6.
- **Service worker**: Workbox `generateSW`, `registerType: 'autoUpdate'`. The
  precache is the app only — `**/*.{html,js,css,wasm,png,svg}` plus the interface
  face (§8.7) — and **no display font**: unsubsetted faces are several hundred KB
  each, and precaching even one would put it on the install path twice (the page
  fetches it anyway). Fonts are served by a `CacheFirst` runtime route on
  `/fonts/*.woff2` (max 8 entries, one-year expiry), so the face in use lands in the
  cache on first paint and works offline from then on (§8.3). The wasm module (~890 KB) is under Workbox's default 2 MiB per-file
  precache cap; the cap is set explicitly anyway, so future growth fails the build
  loudly instead of silently skipping the engine.
- **Offline**: the app — shell, engine, worker — is precached, so a cold offline start
  works. The one runtime request the page ever makes is same-origin, for the display
  font in use (§8.3), and it happens once. Nothing third-party is contacted at any
  point: no CDN, no analytics, no error reporting, and no document leaves the device.
  This is stated in About and is worth keeping true.
- **Updates**: `autoUpdate` plus a toast ("A new version is ready — Reload"). The
  document is already in localStorage, so a reload never loses work.
- **Stretch (W8), both shipped**: a GET `share_target` (`?text=`) so Android's share
  sheet can send selected LaTeX straight into the app, and `file_handlers` for
  `.tex`/`.latex` where supported. Both are additive — a browser that implements
  neither ignores both manifest fields, and the code paths are only reached when the
  browser calls them. A GET target rather than POST, so no service-worker request
  handler is involved and the app simply reads `?text=` on load.

## 10. Build and tooling

`web/package.json` scripts:

| script | does |
|---|---|
| `wasm` | `wasm-pack build crate --target web --release --out-dir pkg` |
| `dev` | `npm run wasm && vite` |
| `build` | `npm run wasm && tsc --noEmit && vite build` |
| `preview` | `vite preview` |
| `test` | `vitest run` |
| `typecheck` | `tsc --noEmit` |
| `fonts` | `python3 tools/fetch_fonts.py` (dev only) |

`vite.config.ts`: `base: '/techxt/'` (project Pages path), `build.target: 'es2022'`,
`worker.format: 'es'`, `VitePWA({...})`. `--target web` (not `bundler`) is chosen so
the generated glue is self-contained, initialises with an explicit `init()`, and works
identically inside a module worker without extra Vite wasm plugins; Vite still hashes
and precaches the `.wasm` because the glue references it through `import.meta.url`.

Rust changes need `npm run wasm` re-run; `web/README.md` documents `cargo watch -w
../rust/techxt -w src -s 'npm run wasm'` for anyone iterating on both sides.

## 11. CI and deployment

A new `.github/workflows/web.yml` — `ci.yml` stays untouched and `rust/`-scoped.

**Job `web`** (on `push`, `pull_request`, `workflow_dispatch`; path filter
`web/**`, `rust/techxt/**`, the workflow itself):

1. rustup stable + `rustup target add wasm32-unknown-unknown`; cache `~/.cargo` and
   `web/crate/target`.
2. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` —
   inside `web/crate`, restoring the gates the crate loses by living outside `rust/`
   (D1). The `[lints]` block copies `rust/`'s policy verbatim.
3. `wasm-pack build …` (§10), then assert the emitted `.wasm` is under a size budget
   — 1.1 MB raw / 350 KB gzipped, roughly 20 % over today's `opt-level = 3` figures —
   so a dependency mistake shows up as a failed build rather than a slow page, and so
   the §4.7 decision to trade size for speed gets revisited deliberately.
4. Node 22, `npm ci`, `npm run typecheck`, `npm test`, `npm run build`.
5. `pip install fonttools brotli && python3 web/tools/coverage_check.py --check` —
   fails on a default-face gap, warns (into the job summary) on the others (§8.5).
6. Assert the font budget: no single `web/fonts/*.woff2` over 900 KB, no more than
   3.5 MB total. Whole fonts are the point, but a 4 MB face added by accident should
   still be a conversation.
7. `actions/upload-pages-artifact` with `web/dist`.

**Job `deploy`** (needs `web`; only `push` to `main`): `permissions: pages: write,
id-token: write`, `environment: github-pages`, `actions/deploy-pages`. A separate
`concurrency: pages` group so a rapid second push queues rather than races.

First-time setup outside the repo: Settings → Pages → Source: GitHub Actions. Worth
noting in `web/README.md`, since the first deploy silently does nothing otherwise.

## 12. Repository integration

- `README.md`: add `web/` to the repository-layout block, and a line near the top
  linking the live app ("Try it in the browser: …") — for most visitors that link is
  the fastest possible explanation of what techxt does.
- Root `PLAN.md` §2: add `web/` to the layout sketch; §17 gains a sentence tying the
  future `js/` package to `web/crate/` (§3 of this file).
- `.gitignore`: `web/node_modules/`, `web/dist/`, `web/crate/target/`,
  `web/crate/pkg/`, `web/.vite/`.
- `CHANGELOG.md`: an "Unreleased" entry for the web app; the library version is
  untouched by any of this.
- `LICENSE`: unchanged (MIT covers the app); the two OFL font licences ship beside
  the fonts they cover and are listed in About.

## 13. Testing and acceptance

**Automated**: `cargo test` in `web/crate` (§4.8); vitest over the pure logic — state
codec, options diffing, fit-to-pane arithmetic, worker-protocol sequencing with a
mocked worker; `tsc --noEmit`; the glyph coverage check.

**Manual checklist**, run before each release and recorded in `web/README.md`:

1. Desktop Chrome/Firefox/Safari: type, wrap, switch fonts, share link round-trip.
2. iOS Safari: install to Home Screen, launch offline, keyboard up, copy works.
3. Android Chrome: install, offline, share link from the sheet.
4. DevTools offline reload after a cold cache.
5. A 200 KB document: typing stays responsive (worker), status shows the time.
6. A pathological document (deeply nested braces, `\frac` 200 deep): a diagnostic,
   not a dead tab — the descent-guard calibration of §4.6.
7. A document mixing `\emph{…}`, CJK, Hebrew and emoji: no tofu in any of the six
   display-font settings (the fallback chains of §8.2).
8. Lighthouse: PWA installable, performance ≥ 95, accessibility 100.

## 14. Measured baselines

Measured on this machine (Apple silicon, rustc 1.97) while writing this plan; they are
the numbers the budgets above are set from, and W7 re-measures them in the browser.

| Quantity | Value |
|---|---|
| wasm, **`opt-level = 3`** + LTO + `wasm-opt -O3` (the choice) | 890 KB raw · **296 KB gzip** · 221 KB brotli |
| wasm, `opt-level = "s"` + `wasm-opt -Os` | 568 KB raw · 220 KB gzip · 176 KB brotli |
| wasm, `opt-level = "z"` + `wasm-opt -Oz` | 470 KB raw · 190 KB gzip · 157 KB brotli |
| `Converter::standard()` | 4.2 ms first · 1.9 ms warm |
| `builder().wrap_width(72).build()` | 1.1 ms |
| convert 402 B | 1.4 ms |
| convert 8 KB | 2.7 ms |
| convert 80 KB | 17 ms |

Native timings; wasm is typically 1.5–3× slower and a mid-range phone slower again, so
budget ~50–150 ms for an 80 KB document on a phone — comfortably inside the debounce,
and the reason a Worker is enough and cancellation is not needed.

### Measured at W7 — the finished binding, in a browser

The probe of the table above was a scratch crate with `fn convert(&str, Option<usize>)
-> String`. The shipped binding carries the serde derives and techy's
`Diagnostic::render()`, which the panel's `rendered` field needs, so it is larger:

| Quantity | Value |
|---|---|
| wasm, as shipped (`opt-level = 3` + LTO + `wasm-opt -O3`) | **939 287 B raw · 333 KB gzip** |

Conversion in Chromium 140 on this machine, median of seven warm runs against one
`Session` (a repeated `\section`/`\emph`/`$…$`/`\footnote`/`itemize` unit of 225 B):

| Document | Value |
|---|---|
| 225 B | 4.6 ms |
| 4.5 KB | 7.0 ms |
| 45 KB | 28.8 ms |
| 45 KB, `wrap_width(72)` | 19.2 ms |
| first `Session` + first `convert` | 4.1 ms |
| 200 KB, through the whole app (§13 item 5) | 157 ms convert · 338 ms wall including the debounce |

Roughly 1.7× the native figures for the same shape of work — the low end of the
1.5–3× the table above predicted, and comfortably inside the 120 ms debounce. Wrapping
is *faster* than not wrapping at this size, which is the layout engine doing less work
per line rather than more. Nothing here argues for revisiting `opt-level`.

While that 200 KB document converts, the main thread answers in 2–20 ms and a keystroke
round-trips in about 100 ms — which is the Worker of D3 doing its job, and the reason
§6.2's Cancel button is a safety net rather than a routine control.

Fonts as committed (§8.3 estimated "roughly 250–700 KB of woff2 each"; the default
face is half again the top of that range, which is what moved the §11 per-file budget
to 1.15 MB — subsetting it is the one thing §8.4 rules out):

| face | bytes |
|---|---|
| `JuliaMono-Regular.woff2` | 1 042 116 |
| `STIXTwoMath-Regular.woff2` | 552 084 |
| `LatinModernMath-Regular.woff2` | 391 580 |
| `LibertinusMath-Regular.woff2` | 380 036 |
| `FiraMath-Regular.woff2` | 98 964 |
| **total** | **2 464 780** |

Glyph coverage of the 1 349 distinct codepoints techxt's own repertoire emits (§8.5):
JuliaMono 1 347, Libertinus Math 1 245, STIX Two Math 1 241, Latin Modern Math 965,
Fira Math 834. See §8.1 for what the two gaps in the default face are and §8.5 for what
the others mean.

The three build profiles differ by 106 KB gzipped between the fastest and the
smallest. `opt-level = 3` takes the speed; the other two rows are here so the trade
can be reversed on evidence rather than re-measured from scratch (§4.7). The speed
side of that table is still unmeasured — W7 fills it in from the browser.

## 15. Milestones

Each is a working, deployable state.

- **W0 — skeleton, end to end.** `web/` scaffolded, `crate/` converting a hardcoded
  string through the worker, Vite build, Pages deploy live.
  *Done when*: the public URL shows real converted text produced by wasm.
- **W1 — binding complete.** Options DTO, diagnostics DTO, UTF-16 offsets, stack and
  descent-guard settings, native tests, size budget in CI.
  *Done when*: `cargo test` in `web/crate` covers §4.8 and CI enforces the budget.
- **W2 — the tool.** Two panes, debounced live conversion, copy, download, status
  line, keyboard shortcuts, light/dark.
  *Done when*: the manual checklist items 1 and 5 pass.
- **W3 — options and state.** Primary bar, "More options", localStorage, share link.
  *Done when*: every option in §5 changes the output, and a link reproduces a session
  in a fresh browser profile.
- **W4 — diagnostics.** Panel, filters, jump-to-source, `rendered` detail, truncation.
  *Done when*: clicking a warning selects exactly the offending macro.
- **W5 — display fonts.** `fetch_fonts.py`, five whole faces plus the system stack,
  the fallback chains, lazy loading with the runtime cache route, coverage report in
  CI, fit-to-pane; font sizes recorded in §14.
  *Done when*: the default face passes the coverage gate, switching a face fetches
  exactly one file, a face chosen once still renders offline, and a document mixing
  LaTeX with CJK and emoji renders with no boxes in any face.
- **W6 — PWA and page.** Manifest, icons, service worker, offline, About/examples/
  install snippets, README/PLAN/CHANGELOG updates.
  *Done when*: manual checklist 2–4 and 7 pass.
- **W7 — calibration and polish.** Descent-guard calibration (§4.6), in-browser
  timings for the §14 table (and a re-look at `opt-level` only if the module has
  grown), a11y pass, mobile pass, empty/huge/pathological input states.
  *Done when*: checklist item 6 passes and the browser timings are recorded in §14.
- **W8 — stretch.** Share target, `.tex` file handler, a pane-divider drag, an
  "explain this diagnostic" link into the crate docs.
  *Done*: the pane-divider drag (with a keyboard-operable `role="separator"`); a GET
  `share_target`, so Android's share sheet sends selected LaTeX straight in as
  `?text=` — ahead of anything stored, since it is an explicit act by the person
  sharing — verified end to end; and a `file_handlers` entry for `.tex`/`.latex`
  consumed through `launchQueue`, which replaces the document with the same
  single-level undo the Load menu offers.
  *Not done*: the "explain this diagnostic" link. techxt is not published, so there is
  no rendered crate documentation to link to and no per-identifier anchor to link at —
  the link would be a 404 dressed as help. It becomes worth doing when the crate has
  published docs, and the identifier is already on screen in monospace so a search
  finds what there is.

## 16. Deliberate omissions

Syntax highlighting or a code editor component (a textarea is honest and fast, and
CodeMirror would outweigh the engine); rendering LaTeX visually for comparison;
multi-file/`\input`; a definitions playground (the extension API is a crate-docs
subject, not a UI); server-side conversion for very large documents; i18n of the UI;
any analytics.

---

# Appendices

These exist so this file can be executed without the conversation that produced it.
Everything below was read out of the tree or measured on 2026-08-20; anything marked
*verify* is a fact about the outside world that may have moved.

## Appendix A — the techxt API the binding uses

Every name here is re-exported from **`techxt::convert`**, so `web/crate` depends on
`techxt` and never on `techy` directly — the same one-dependency property
`techxt-cli` demonstrates. Read `rust/techxt/src/convert.rs` for the doc comments;
this is the index, not a substitute.

**Entry points**

```rust
Converter::standard() -> Converter                       // shipped defs, default options
Converter::builder()  -> ConverterBuilder
ConverterBuilder::build(self) -> Result<Converter, BuildError>
Converter::latex_to_text(&self, latex: &str)
    -> Result<Conversion, ParseError<Option<String>>>    // Err only on a fatal/strict parse failure
Converter::options(&self) -> &Options
pub struct Conversion { pub text: String, pub diagnostics: Diagnostics<Option<String>> }
```

`Converter` is `Send + Sync + Clone` (clone is an `Arc` bump). `Conversion::text` ends
with exactly one newline unless it is empty.

**Builder setters** — one per `Options` field, plus three parse-time ones:

| setter | argument | default |
|---|---|---|
| `math_mode` | `MathMode::{Fancy, Plain, Source}` | `Fancy` |
| `math_expression_in` | `MathWrapDelims::{Parens, Braces, Custom(Box<str>, Box<str>), None}` | `Parens` |
| `matrix_delimiters` | `MatrixDelims::{Unicode, Ascii}` | `Unicode` |
| `wrap_width` | `Option<usize>` | `None` |
| `keep_comments` | `bool` | `false` |
| `heading_style` | `HeadingStyle::{NumberedUnderlined, Underlined, Prefix, Plain}` | `NumberedUnderlined` |
| `footnote_style` | `FootnoteStyle::{Collected, Inline, Skip}` | `Collected` |
| `list_style` | `ListStyle { itemize_markers, enumerate_formats }` | bullets `• – * ·`, `1.` `(a)` `i.` `A.` |
| `text_font` | `FontStyle::{Disabled, Default, Style(FontStyleKind)}` | `Default` |
| `math_font` | `FontStyle` | `Style(FontStyleKind::Italic)` |
| `unknown_macro` | `UnknownMacroPolicy::{Skip, RenderArgs, KeepSource, Placeholder}` | `Skip` |
| `unknown_env` | `UnknownEnvPolicy::{RenderBody, Skip, KeepSource}` | `RenderBody` |
| `unknown_specials` | `UnknownSpecialsPolicy::{EmitChars, Skip}` | `EmitChars` |
| `today` | `Option<Box<str>>` | `None` → renders `<today>` |
| `recovery` | `Recovery::{Tolerant, Strict}` | `Tolerant` |
| `unknown_macro_resolution` | `UnknownMacroResolution::{FollowRecovery, Accept, Reject}` | `FollowRecovery` |
| `descent_guard` | `StdDescentGuardInit` | `fixed_stack_budget(250 KiB)`, unconfigured |

`FontStyleKind`: `Bold, Italic, BoldItalic, Script, BoldScript, Fraktur, DoubleStruck,
BoldFraktur, SansSerif, SansSerifBold, SansSerifItalic, SansSerifBoldItalic, Monospace,
Upright` (fourteen).

`StdDescentGuardInit`: `default()`, `fixed_stack_budget(bytes)`,
`computed_stack_budget(fn() -> Option<usize>)`, `depth_limit(levels)`, `off()`.
Constants: `StdDescentGuard::DEFAULT_STACK_BUDGET = 250 * 1024`,
`StdDescentGuard::HEADROOM = 64 * 1024`. See §4.6 for what to use here and why.

**Diagnostics**

```rust
Diagnostics::{iter, as_slice, len, is_empty, suppressed, has_errors, error_count}
Diagnostics::sorted_by_position() -> Vec<&Diagnostic<O>>   // the order the panel wants
Diagnostics::<O>::DEFAULT_LIMIT = 1000                     // beyond it, counted not stored

Diagnostic::severity()   -> Severity                       // Note < Warning < Error (Ord)
Diagnostic::identifier() -> &str                           // "techxt.unknown-macro"
Diagnostic::message()    -> String
Diagnostic::span()       -> &SourceSpan<O>
Diagnostic::frames()     -> &[TraceFrame<O>]               // .title(), .span()
Diagnostic::render()     -> String                         // the CLI's full rendering

SourceSpan::{start, end, range, len, content, source}      // start/end are UTF-8 BYTE offsets
Source::content() -> &str                                  // identity-compare to find "our" source

ParseError::{identifier, message, span, frames, render}    // same shape, for the Err case
```

**Reference implementation.** `rust/techxt-cli/src/main.rs` (~200 lines) is a complete
embedder: builder wiring, diagnostic filtering by severity, `render_all`, exit codes.
`rust/techxt-cli/src/cli.rs` maps CLI flag strings onto these enums and is the closest
existing analogue of the options DTO of §4.2. `rust/techxt-cli/src/today.rs` is the
date format the app must match (§5).

## Appendix B — environment and verified recipes

Confirmed present on the development machine on 2026-08-20: rustc/cargo **1.97.0**,
`wasm32-unknown-unknown` target installed, `wasm-pack`, `node`, `npm`, `python3`,
`pyftsubset`/fonttools. `wasm-bindgen-cli` and a standalone `wasm-opt` are **not**
installed — wasm-pack supplies both.

- **`techy` is a git dependency** pinned to rev `aa71c83`, so a cold build needs
  network. `web/crate/Cargo.lock` is committed to pin it for deploys.
- **`wasm-opt` fails out of the box.** wasm-pack's bundled binaryen is version 117,
  which rejects the bulk-memory operations rustc 1.97 emits:
  `[wasm-validator error] Bulk memory operations require bulk memory`. The fix is the
  metadata block of §4.7 (`--enable-bulk-memory --enable-nontrapping-float-to-int
  --enable-sign-ext`), verified to build. `wasm-opt = false` is the fallback; it costs
  little (§14).
- **MSRV does not apply to `web/crate`** (§3). `rust/`'s 1.86 floor is for library
  consumers; the binding builds on stable.
- **Measurements in §14** came from a scratch crate with a `path` dependency on
  `rust/techxt` — a `#[wasm_bindgen] fn convert(&str, Option<usize>) -> String` for the
  sizes, and a native `--release` binary timing `Converter::standard()`,
  `builder().wrap_width(72).build()` and `latex_to_text` over a document repeated 1/20/200
  times. Reproduce the same way; the numbers are a baseline to beat, not a contract.
- **GitHub Pages needs a one-time manual step**: Settings → Pages → Source → *GitHub
  Actions*. Until it is set, `deploy-pages` succeeds and publishes nothing.

## Appendix C — font sources (*verify*)

Release asset paths move between versions; the script records the exact URL, version
and SHA-256 it used in `web/fonts/SOURCES.md`, and copies each licence verbatim into
`web/fonts/licences/`.

| face | upstream | file to take |
|---|---|---|
| JuliaMono | `github.com/cormullion/juliamono` | the release's `webfonts/JuliaMono-Regular.woff2` — ship as-is |
| Fira Math | `github.com/firamath/firamath` | `FiraMath-Regular.otf` → convert |
| Latin Modern Math | CTAN package `lm-math` (GUST) — *but see below* | `latinmodern-math.otf` → convert |
| STIX Two Math | `github.com/stipub/stixfonts` | `STIXTwoMath-Regular.otf` → convert |
| Libertinus | `github.com/alerque/libertinus` | `LibertinusMath-Regular.otf` → convert |

"Convert" means container-only otf→woff2 per §8.4, never subsetting. Confirm each
face's licence file *in the release actually downloaded* rather than assuming the
table above; OFL versus GUST decides whether the name-ID suffix of §8.4 is the OFL's
Reserved Font Name clause or the LPPL's rename requirement.

**What the script actually fetched at W5** (`web/fonts/SOURCES.md` is the record, with
every URL, version and SHA-256): JuliaMono 0.63.2 from its release's
`JuliaMono-webfonts.tar.gz`, shipped byte for byte; Fira Math 0.3.4 (0.4 is beta-only
and publishes no `.otf`); STIX Two Math **2.13, not 2.14** — 2.14 is tagged "INTERIM
(build process conversion)" and ships no built fonts; Libertinus 7.051.

**Latin Modern Math did not come from CTAN.** CTAN and gust.org.pl are unreachable
from the network this was built on, so the file was taken from Debian/Ubuntu's
`fonts-lmodern` package (`archive.ubuntu.com/…/lmodern/fonts-lmodern_2.005-2_all.deb`,
member `usr/share/texmf/fonts/opentype/public/lm-math/latinmodern-math.otf`), which is
GUST's 1.959 file redistributed unmodified, together with the GUST licence from the
same package. `SOURCES.md` says so plainly rather than claiming CTAN. On a machine that
can reach CTAN, prefer it and update the recorded hashes.

**One deliberate deviation from §8.4's wording.** Name IDs 1/4/16 get `" Web"` as
specified; **ID 6, the PostScript name, gets `"Web"` without the space**, because the
OpenType specification excludes the space character from that field.

## Appendix D — executing this plan

**Read first**, in this order: this file end to end; the repository `README.md`;
the root `PLAN.md` §2 (layout) and §9 (what each rendering option *means*);
`rust/techxt/src/convert.rs`; `rust/techxt-cli/src/main.rs` and `cli.rs`.

**Boundaries.**

- Do not change anything under `rust/` except the documentation edits §12 lists
  (README layout block and live link, root `PLAN.md` §2 and §17, `CHANGELOG.md`). If the
  app wants an option or an API the library does not have, that is a library decision
  — raise it, do not reach into the crate to make the app easier.
- `web/crate` depends on `techxt`, `wasm-bindgen`, `serde`, `serde-wasm-bindgen` and
  `console_error_panic_hook`, and nothing else without asking.
- Runtime npm dependencies beyond Vite, `vite-plugin-pwa` and vitest need asking too:
  the app's weight is a design property, not an implementation detail.
- Ship no display font that is not in §8.1, and add no option that is not in §5,
  without the same conversation.

**Working order.** Milestones W0–W8 (§15) are sequential and each ends in a commit
with green CI. W0 in particular must be *deployed and reachable* before W1 starts: a
Pages misconfiguration discovered at W6 is expensive, and W0 is the cheapest possible
probe of the whole pipeline.

**This file is the record.** Fill in §14 as measurements are taken (font sizes at W5;
browser timings and the descent-guard calibration at W7). If a decision in §2 is
revisited, change the row and note why — a plan that silently diverges from the code
is worse than no plan.

**Done, overall**, when: the app is live at `https://phfaist.github.io/techxt/`;
`README.md` links it; every item of the §13 manual checklist passes; CI builds,
tests, size-budgets and deploys on a push to `main`; and a person on a phone with no
network can open it from their home screen, paste a formula, and read the answer.
