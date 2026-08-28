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
itself never scrolls: the tool is exactly one viewport, and About, Install and the
library are dialogs (§6.8, §6.10).

Goals, in priority order:

1. Paste LaTeX, read text, copy it out. Fast, on a phone, with the keyboard up.
2. Expose the conversion options that change the answer, without burying the three
   that change it most (wrapping, math rendering, display font).
3. Show techxt's diagnostics as the structured, positioned things they are — this is
   the feature that distinguishes it from a macro stripper, and a demo that hides it
   undersells the library.
4. Work offline, installable, no server, no telemetry, no document ever leaving the
   device. A converter people paste unpublished papers into must be able to say this
   plainly — which is also why the library of §6.10 is on the device, is never
   uploaded, and is never trimmed behind the user's back.
5. Be a credible landing page: what techxt is, how to get the crate and the CLI.

Non-goals: rendering the *document* visually — this is a converter to text, not a
previewer, and the output pane shows what techxt produced; a file tree or multi-file
projects; `\input` resolution (there is no filesystem — the diagnostic explains it);
editing features beyond a textarea (no syntax highlighting, no CodeMirror); accounts;
server-side anything.

**The formulas are the one exception, and an opt-in one.** *Math: MathJax* (§5)
typesets the mathematics in the output pane while everything around it stays the
converted text it always was. It earns the exception because text-mode mathematics is
the one part of a conversion that reads badly however well it is done, so being able to
see a formula while judging the *structure* around it is what the option is for; and it
costs the rest of the page nothing, because it is a fourth answer to a question the bar
already asks and 1.8 MB that only the readers who choose it ever fetch (§9.1).

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
| D8 | App fills the viewport; header is one line; About/Install/Library are modal sheets and the page never scrolls | Mobile layout has to work with the on-screen keyboard up (§6.6); a toast raised from inside a sheet has to move into it, since a modal makes everything else inert (§6.10) |
| D9 | Every converted document is logged automatically to an IndexedDB library, and nothing in it is ever removed without the user saying so | A second store to keep honest: a quota story that proposes rather than prunes, and an export format that is the answer to a full disk (§6.10, §6.11) |
| D10 | *MathJax* is a fourth value of the *Math* control, resolved to `math_mode: Source` before the binding sees it, and the formulas are typeset in the output pane | The library never hears the word, so the app has to be told where the formulas are (`regions`, §4.3), wrap each one in an element after the text is set (§6.3), and carry 11.8 MB in `dist/` that is fetched only by the readers who ask for it (§9.1) |

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
    title.ts              what a document calls itself: file names and entry titles
    library.ts            the library's entry model, session and retention policy
    library-store.ts      the IndexedDB backend, and the quota facts around it
    library-io.ts         the export format, and what an import is allowed to do
    mathjax.ts            MathJax in four functions: load, loaded, typeset, reset
    math-regions.ts       cutting the output into the runs the pane wraps in elements
    worker/
      convert.worker.ts   loads wasm, answers convert requests
      protocol.ts         message types shared by both sides
    ui/
      api.ts              what main.ts may assume about the five modules below
      panes.ts            input/output panes, resize, autofit measurement
      controls.ts         primary bar + "More options" disclosure
      diagnostics.ts      the diagnostics panel and jump-to-source
      library-pane.ts     the library sheet, and its import/confirm dialogs
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
types the state codec and the UI both need — including the three settings of §5 that
are the *app* answering a question the library leaves open. `src/ui/api.ts` holds the
interfaces the five UI modules satisfy and `main.ts` programs against, so neither side
can drift without `tsc` saying so.

The three `library*.ts` files are split along the same line: `library.ts` and
`library-io.ts` are pure and know nothing about a browser — the backend and the clock
arrive as parameters — so the retention policy and the import rules are reachable from
vitest, while `library-store.ts` is the only file in the app that opens a database.
`math-regions.ts` and `mathjax.ts` are the same split again: the arithmetic of cutting
the output at the region boundaries is pure and tested, and the file that fetches a
typesetter and hands it elements has nothing in it worth a unit test.

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

Four exports, no more:

```rust
#[wasm_bindgen]
pub struct Session { /* cached Converter + the options it was built from,
                       plus the completion table once it is asked for */ }

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session;

    /// Convert `latex` under `options` (a plain JS object). Returns a
    /// `ConversionResult` object; never throws for a document-level failure —
    /// a strict-mode parse error comes back as a result with `ok: false` and one
    /// diagnostic.
    pub fn convert(&mut self, latex: &str, options: JsValue) -> Result<JsValue, JsValue>;

    /// The completions for `prefix`, merged from techxt's own table and from what
    /// `latex` defines for itself, ranked, and capped at `limit`. Returns an array
    /// of `Completion` objects; see §4.9.
    pub fn complete(&mut self, latex: &str, prefix: &str, limit: usize)
        -> Result<JsValue, JsValue>;
}

/// Version string of the embedded techxt, for the About section and bug reports.
#[wasm_bindgen]
pub fn techxt_version() -> String;
```

`Session` holds one `Converter` plus a hash of the options it was built from, and
rebuilds only when they change. Measured cost of a rebuild is ~1.2–1.9 ms natively
(§14), so this is a small optimisation, not a necessary one — but it makes typing
under fixed options cost exactly one `latex_to_text` call.

**`complete` was added with the lighter editor** and is the reason this section says
four and not three. It is a second question asked of the same instance rather than a
second module: the answer is a handful of entries, the work behind it is a binary
search, and putting it here is what lets the app keep one worker and one wasm heap.

### 4.2 Options in

Options arrive as a plain JS object and are deserialized with `serde` +
`serde-wasm-bindgen` into an `OptionsDto` whose every field is `#[serde(default)]`
and whose enums are lowercase-kebab strings (`"numbered-underlined"`). The DTO is
then mapped onto `ConverterBuilder` in one `fn build(dto: &OptionsDto) ->
Result<Converter, String>`.

`serde` here is a *binding* dependency; `techxt` itself keeps its exactly-two
runtime dependencies (root PLAN §2), which the `rust/` CI continues to enforce.

Two properties the mapping must have, both unit-testable natively (no wasm needed):

- **Total.** *(Amended for M9: the scope is the builder, not only `Options`.)* Every
  field of `techxt::convert::Options` **and every parse-time setter of
  `ConverterBuilder`** is either mapped or listed in a `// not exposed:` comment with
  the reason. The original wording named `Options` alone, which was already inexact —
  `recovery` is a builder setter and not a field — and M9 made it wrong: it added
  three setters (`macro_definitions`, `expansion_depth_limit`, `expansion_count_limit`)
  that exist nowhere in `Options`, and a totality rule that never looked at the builder
  would have let all three through without a reviewer noticing.
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
  regions: MathRegion[];    // where the formulas are — see below
}

interface MathRegion {
  start: number;            // UTF-16 code units into `text`, not into the input
  end: number;
  display: boolean;         // \[…\], equation, align — rather than $…$
}

interface Diagnostic {
  severity: 'error' | 'warning' | 'note';
  identifier: string;       // e.g. "techxt.unknown-macro"
  message: string;
  rendered: string;         // Diagnostic::render() — the CLI's full text, for details
  span: null | {            // null when nothing it came from is in the input (§4.5)
    start: number;          // UTF-16 code-unit offset — see §4.4
    end: number;
    line: number;           // 1-based
    column: number;         // 1-based, in characters
  };
  approx: boolean;          // added at M9 — see §4.5
  frames: { title: string; span: Span | null }[];   // the include/expansion trace
}
```

**Amended for M9: `approx`.** `false` means `span` is the diagnostic's own position;
`true` means it is the nearest enclosing macro invocation in the typed document,
substituted because the diagnostic's own position is inside an expansion (§4.5). It is
`false` whenever `span` is `null` — there is then nothing for it to qualify — so a panel
that ignores the field renders exactly what it rendered before, only pointed at the
macro call.

Diagnostics are emitted in `Diagnostics::sorted_by_position()` order so the panel
matches reading order. `Diagnostics::DEFAULT_LIMIT` is 1000; beyond that techy counts
rather than stores, which is what `suppressed`/`truncated` report.

**`regions`: where the formulas are.** A conversion's text is text, and there is no way
to read it back and find the mathematics: a document writing `\$5` produces a `$` that
is indistinguishable from the `$` of a formula. So `Conversion.regions` reports the runs
of the output that are preformatted rather than converted, and the binding turns that
into this flat list — the offsets mapped from bytes to UTF-16 code units by the same
single pass §4.4 describes, over the *output* this time rather than over the input.

Two things about it are deliberate. It is **filtered**: techxt tags four kinds of
preformatted run, and only `MathSource` — a formula re-emitted as its own post-expansion
LaTeX — is something a typesetter can be handed. `MathRendered` is techxt's own aligned
Unicode, kept preformatted because its columns are fragile; feeding it back to a TeX
engine would ask the engine to read techxt's answer as a question. `KeptSource` and
`Verbatim` are not mathematics. The app therefore never meets a provenance. And it is
**unconditional**: there is no option that turns it on, it costs an empty `Vec` on a
document without formulas, and a caller with no use for it ignores the field.

Three properties the layout engine guarantees and the code wrapping these ranges in
elements will meet: a display formula's range **excludes** the newline that ends its
last line; a construct that renders to nothing reports nothing; and a range may contain
a line break, so it is not guaranteed to sit within one line of the output. `mathMode:
'plain'` reports no regions at all — it flattens formulas into ordinary text — which is
an empty list rather than a failure.

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

**Rewritten for M9.** The rule below is the one this section always had, and the reason
for it has not moved; what changed is how often it fires and what the binding does after
it does.

A diagnostic's span points into a `Source`, which need not be the document the user
typed — a synthesized source, or (in principle) an `\input`ed one. The binding compares
the span's source against the one it created for this call, by content (Appendix A), and
a span that is not in the buffer is not one the textarea can select.

- **Minted sources are now routine, not hypothetical.** The parse reads through
  techy-xp, so a `\newcommand` in the document defines a macro and a use of it is
  *expanded*: the body becomes a synthesized source, and every diagnostic raised while
  reading it points into that body. `\newcommand{\a}{x \nope}\a` reports its
  `techxt.unknown-macro` at an offset in `"x \nope"` — a document a person might
  plausibly type, producing what used to be an unreachable case. Before M9 the binding
  exposed no `source_resolver` and techxt's own definitions synthesized nothing, so
  every span a browser conversion produced was in the buffer and the `null` branch was
  dead code.
- **So the binding substitutes the invocation** rather than dropping the position. For a
  span it cannot accept it looks for the nearest enclosing position that *is* in the
  typed document, and reports it with `approx: true` (§4.3) so the panel can say the
  position is the macro call and not the message's own place. Two chains are searched,
  in this order:
  1. the diagnostic's **trace frames**, innermost first — a frame is a construct the
     parse descended into, and the innermost one that is in the buffer is the most
     precise answer available;
  2. the source's **provenance chain** — every synthesized source records the span that
     triggered it, which for an expansion is the invocation being expanded, and each hop
     lands in an older source, so the walk reaches the primary source in a few steps.

  > **The second step is the one that earns its keep, and measurement is why it is
  > here.** The M9 design for this section named the frames alone, and stopped there.
  > Measured against the real converter,
  > the routine expansion diagnostic has *no frames at all*: `techxt.unknown-macro`
  > raised inside a macro body arrives with an empty trace, and so do
  > `techy-xp.expand.*-budget-exceeded` and `core.groups.unclosed-group` from inside a
  > body. Frames answer only the minority of shapes that were mid-descent when the
  > diagnostic was raised (`\newcommand{\a}{$x^}\a` is one). With frames alone the
  > commonest case would still have rendered as an inert row, which is the thing this
  > work exists to fix. The provenance chain is reachable through `techxt::convert`
  > without naming `techy` — `span.source().provenance_chain()` — so it costs the
  > binding no new dependency and `rust/` no change.
- **`span: null` is the residue.** It survives for a span with no accepted frame and no
  triggering location — nothing this app can produce today, since every synthesized
  source records where it came from and the chain ends at the primary source, but the
  panel keeps its inert row and now says *why* in terms of expansion rather than of
  "outside the document" (§7). Those diagnostics still show their message and `rendered`
  text.

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
- **Amended for M9: expansion multiplies descents per typed level, and the guard is
  still the one that counts.** A macro whose body nests constructs contributes those
  constructs' descents at every level the document nests the macro, so a
  macro-expanding document reaches 300 descents at a *shallower* typed depth than the
  raw construct would — which is the safe direction: the guard counts descents, so the
  margin under the engine's stack limit measured above is unchanged, and the only thing
  that moves is how deep a document may be written before it is refused. Measured
  natively through this binding's own converter (`depth_limit(300)`, the library's
  expansion budgets), searching for the shallowest depth that refuses:

  | document, nested *n* deep | refuses at | condition |
  |---|---|---|
  | `\textbf{…}` | 100 | `core.constructs.descent-limit-exceeded` |
  | `{…}` | 150 | `core.constructs.descent-limit-exceeded` |
  | `\w{…}`, `\w` defined as `\textbf{#1}` | 65 | `techy-xp.expand.expansion-depth-budget-exceeded` |
  | `\w{…}`, `\w` defined as `\textbf{\textbf{#1}}` | 50 | `core.constructs.descent-limit-exceeded` |
  | `\m0 → \m1 → … → x`, a chain of *n* definitions | no refusal at 500 | — |

  The first two rows reproduce the browser figures of the table above exactly, which is
  the depth limit being engine-independent as claimed. The third shows techy-xp's own
  64-deep expansion budget refusing first when each level is one live expansion; the
  fourth shows the descent guard refusing first when the body nests two constructs
  (six descents per level instead of three). The last is the shape that does *not* nest
  — each frame is popped before the next is pushed — and is the count budget's business,
  not this one. Every refusal is a diagnostic and the parse continues, which is checklist
  item 6 of §13 with a macro in it.
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
(§14) held in reserve — if the module grows past the gzip ceiling, or a phone profile
shows the download hurting more than the speed helps, drop to `"s"` first (`"z"` costs
the most speed for the last few tens of kilobytes). The CI size budget of §11 is what
makes that growth visible.

The ceiling itself is **not written down here**: `WASM_MAX_GZIP_BYTES` (and
`WASM_MAX_BYTES`) in `.github/workflows/web.yml` are its only authoritative copy, with
the reasoning for their current values in the comment above them. This section owns the
*policy* — grow past the ceiling and the first response is `"s"` — not the number.

**Reached once, and deferred (2026-08-28).** M10 took the module past the gzip ceiling
and CI fired. The owner's call was to raise that ceiling for now rather than drop to
`"s"`; see §14 for the measurement. The paragraph above is not withdrawn — `"s"` is
still the first response, still worth 56 KB gzipped, and still waiting on the
browser-side speed measurement that would let the trade be made on evidence. What moved
is where the tripwire sits, not the plan.

### 4.8 Tests

Native `cargo test` in `web/crate` (the mapping and offset code is ordinary Rust):

- Options DTO: empty object → `Options::default()`; every enum string round-trips;
  an unknown enum string is a clean `Err`, not a panic.
- Offsets: the property test of §4.4.
- Diagnostics: an `\undefinedmacro` document yields one `techxt.unknown-macro`
  warning whose span selects exactly `\undefinedmacro` in the input.
- Strict mode: a malformed document yields `ok: false` with one error diagnostic.

**Added for M9**, all native too:

- Options DTO: `macroDefinitions` deserializes both spellings and reaches the builder,
  observed through what it does (`declared` leaves `\greet` an unknown macro); an absent
  field still expands, which is the library's own default and not one re-typed here; and
  neither expansion budget is a field the app can send.
- Diagnostics, the §4.5 substitution: `\newcommand{\a}{x \nope}\a` — whose diagnostic
  has an empty trace — comes back with the span of the `\a`, `approx: true`; the same
  document under `macroDefinitions: 'declared'` comes back exact, with `approx: false`;
  an accepted frame is preferred to the provenance chain when there is one; and a span
  with neither stays `null` with `approx: false`.
- The `OurSource` unit tests keep testing the comparison itself, but their premise is
  now the opposite of what it was: the reject path is what an ordinary document takes.

**Added with the math regions** (`crate/tests/regions.rs`): one document converted twice
produces all four provenances — source-mode inline and display, a `\verb`, an unknown
macro kept as source under one pass, and the same display formula *rendered* under the
other — and the filter keeps exactly the source-mode formulas both times. The `\$`
document is there too, since it is the whole argument for reporting regions at all. Every
assertion slices the output in UTF-16 code units, the way the DOM will, and one document
puts an emoji and an astral-plane character before the formula so that the byte offset
and the UTF-16 offset genuinely disagree.

Plus one `wasm-bindgen-test` smoke test that `Session::convert` round-trips through
`JsValue` under `wasm-pack test --headless --firefox`, run locally rather than in CI
unless it proves cheap.

### 4.9 Completion

The editor's chip row asks one question — *what could this be?* — and the binding
answers it whole:

```ts
interface Completion {
  name: string;                                  // without the escape character
  kind: 'macro' | 'environment' | 'specials';
  replacement: string | null;                    // the literal it renders as, if any
  arity: number;
  fromDocument: boolean;                         // scanned out of the user's document
}
```

**The app does no matching, no merging and no ranking.** It sends a prefix and renders
what comes back, in the order it comes back in. That is the section's whole design
decision, and it is worth stating as a rule rather than as an implementation note:
there are two sources of suggestions, and reconciling them in TypeScript would mean a
second matcher to keep in step with the first, a second copy of a fourteen-hundred-entry
table in the bundle, and two places to change when the order is wrong. `complete.rs`
holds all of it, next to the table it is drawn from.

**The two sources.** The first is techxt's own declared symbols, read through
`DefinitionSet::symbols()` (root PLAN §10.7) — 1 406 names, with the literal each one
renders to where it has one, which is what makes a row worth reading rather than a list
of words. The second is what the document defines for itself: `\newcommand`,
`\renewcommand`, `\providecommand`, `\def`, `\DeclareMathOperator` and
`\newenvironment`, with their starred forms and both the braced and the bare spelling of
the name. Those are flagged `fromDocument` and rank first, because a name the author
wrote is the one they meant — and, since a `\renewcommand` in the document is also the
definition that will actually fire, the document's entry *replaces* the library's rather
than sitting above a duplicate of it.

**The order, in full**, which is to say the answer to "what does the first Tab take, and
what does the Tab after it take?": an exact match on what has been typed, before
anything else; then what the document defines; then the curated names, in the curated
order; then macros before environments before specials; then the shortest name — the one
closest to what has been typed — and then alphabetically, so that the same prefix always
gives the same answer.

**The curated list, and why the ranking needed one.** The last two rules measure the
*name*, because a table of definitions knows nothing about the person typing it. By them
alone `\alp` offers `\alph` — LaTeX's alphabetic counter format — ahead of `\alpha`,
which is defensible and wrong. Nothing available here measures which of the two is
*wanted* more often, so the answer is written down rather than derived: a hundred macros
in `complete.rs`, the Greek alphabet, the mathematics one writes with it, and the
everyday text and structure macros, in an explicit order that is never re-sorted. Two
properties make it safe to hand-write. It is a **ranking overlay and never a source of
entries** — every suggestion still comes out of the table or the document, so a curated
name techxt does not define does not appear at all, which is a silent failure and is
therefore pinned by a test that resolves every name on the list against the shipped
definitions. And **the exact-match rule sits above it**, which is what keeps the list
from moving the bug instead of fixing it: `\alp` now offers `\alpha` first, and `\alph`
typed in full still offers `\alph`, so the shorter name stays reachable.

**The list is short on purpose, and it lives in the binding.** A list long enough to
cover the table is the shortest-first rule again with extra steps, and every name on one
is a ranking someone has to justify. It is in `web/crate/src/complete.rs` rather than in
`rust/techxt` because *what people type most* is a fact about a completion UI and not
about LaTeX: it would change with the audience, nothing in the library could test it, and
a converter converts no better for knowing that `\alpha` is popular. What the library
owed this feature it has already given, in `DefinitionSet::symbols()`.

**What writing the list found out.** `\begin` and `\end` — the two macros a LaTeX
document has most of, and the obvious head of any such list — are not on it, because
techxt does not define them: they are structure the parser handles itself rather than
entries in a `DefinitionSet`, so no ranking can offer them and a completion for `\begi`
is empty. `equation` and `align`, checked for the same reason, *are* defined — as
environments, and they are not curated either, because the row fires on an escape
character and an environment name is not typed after one. Both findings are pinned by
tests, so the day `\begin` becomes completable somebody is told rather than left to
notice.

**The table is built once and kept.** `Session` builds it lazily on the first completion
request, so a session whose user never types a `\` never pays for it, and then holds it:
the alternative is resolving fourteen hundred definitions to answer three letters. It is
held as an owned copy of the resolved entries rather than as a `SymbolIndex`, which
borrows the `DefinitionSet` it was read from and so cannot be stored beside it without a
self-referencing struct. The copy is a few tens of kilobytes and keeps the property the
index exists for: sorted by kind and then name, so a prefix query is two binary searches
and a subslice.

**The document is passed in, not remembered.** `complete(latex, prefix, limit)` is
stateless with respect to the text, so the session can never answer from a stale copy of
something the user has since edited. The scan is linear and the search is a binary one;
if the scan ever shows up in a profile, the place to cache it is inside `Session`,
against the text's length and hash, without moving it out of this signature.

**Measured**, on the release module built by `npm run wasm`, driven from Node rather
than a browser (2026-08-28 container): the first call costs **7.4 ms**, which is the
table being built and is why it is built lazily; after that a call is **0.03 ms** on an
empty or an ordinary 2 KB document and **1.1 ms** on a 197 KB one, where the linear scan
is the whole of the difference. So the cost is the document's length and not the table's
size, and even the large case is well inside a keystroke. Placing a candidate in the
curated list is a walk over a hundred short names, which is cheap once and not cheap
inside a comparator, so each candidate's sort key is computed once and carried beside it
rather than recomputed for every comparison the entry takes part in.

**What the scan cannot see, and why that is the right line.** It is a scan and not a
parse: `Conversion` exposes the converted text and its diagnostics and no parsing state,
and running the parser a second time to read one out is far out of proportion to the
difference it makes to a chip row. So the scan reads what a definer *looks* like.
Comments are filtered, because `%` is one unambiguous character and the scan is walking
escape sequences anyway — `\%` is a control symbol and does not start one. A `verbatim`
body is **not** filtered: recognizing one would mean tracking `\begin`/`\end` pairs,
`\verb` with its arbitrary delimiter and every listing package a document might use,
which is the parse just declined. The cost is one chip offering a name that will not
fire, and the tests pin both halves so that neither reads as a bug.

**Not on the wire: the mode restriction.** `SymbolEntry` says whether a definition is
text-only or math-only, and it is dropped rather than passed on, because the app has no
idea which mode the cursor is in — deciding that is a parse. Offering `\alpha` in a
paragraph is the honest failure here; hiding it would be the dishonest one.

Tests, native in `web/crate/tests/completion.rs`: a prefix matching shipped symbols comes
back with their replacements; a document defining `\ket` has it first, flagged, with the
arity it declared, and exactly once; every definer is recognized in every spelling; a
later definition replaces an earlier one; a commented-out definer is not offered and an
escaped percent does not swallow the line; a definer in a `verbatim` body is offered, on
purpose; the limit is a cap; an empty prefix and a prefix matching nothing both behave;
and each ranking rule is asserted on its own. The curated list gets four of its own:
every name on it resolves against the shipped definitions as a macro, no name is listed
twice, the order a chip row shows is the list's own order and not a re-sort of it — the
six Greek variants, `\varepsilon` first, are the case where the two orders are exact
opposites — and everything past the curated names is still shortest-first. Plus the pair
that record the reversal: `\alp` leads with `\alpha`, and `\alph` leads with `\alph`. Plus `wasm_completion.rs`, the browser-only
half, which is about the wire spelling alone — `fromDocument` in camelCase, a kind as a
lowercase string, `replacement: null` rather than an absent key, and an `arity` that is a
number and not the `BigInt` a 64-bit integer would have become.

## 5. The option model

**Primary bar** (always visible, one row on desktop, wrapping to two on a phone):

| Control | Maps to | Default |
|---|---|---|
| Wrap: Fit / Off / Soft / 40 / 60 / 72 / 80 / custom | `wrap_width(Option<usize>)` | **Soft** (see below) |
| Math: Fancy / Plain / Source / MathJax | `math_mode` | Fancy |
| Display font: JuliaMono / Fira Math / Latin Modern / STIX Two / Libertinus / System | CSS only | JuliaMono |

*Soft* is the app's default, and it is the library's own answer shown kindly: it sends
the library exactly what *Off* sends (nothing: `wrap_width` stays `None`) and differs
only in the CSS the output pane is shown in, which folds the long lines to the
container's width (§6.3). Because the fold is display only, Copy and Download hand over
the same text *Off* would, byte for byte — so the app's default no longer rewrites the
text the user came for. That is why it, rather than *Fit*, is where a fresh profile
starts: the app's opinion is confined to the pane, and the bytes that leave are the
library's.

*Fit* is an app-level value, not a library one: it measures the output pane and sends
the resulting column count (§6.5), which is a real change to the output — hard line
breaks at a width nobody asked for by name. It is the right answer for someone
producing text to paste into a fixed-width medium, and it stays one keystroke away, but
it is a decision the user should make rather than inherit.

The three answers are values of `wrap` rather than a toggle beside it so that it stays
one question with one answer: *where do the line breaks come from?* — the pane, the
library, a column count — with the two library-default answers next to each other in
the list.

***Math: MathJax* is the same shape of app-level value.** The control asks one
question — how should a formula be shown? — and *typeset it* is one of the answers to
it, so it is a fourth value rather than a checkbox beside the other three. The whole
control is therefore app-level: `AppOptions` carries `math`, not `mathMode`, and
`resolveOptions` translates — `'mathjax'` becomes `mathMode: 'source'` and the other
three pass through under the library's own name. The binding cannot be handed the word
`mathjax`, which is exactly what makes it safe to put an app-level answer in the same
list as three library ones. What Source re-emits is the formula's own post-expansion
LaTeX (so MathJax only ever meets primitives, never a document's macros), and the app
typesets each run the binding pointed at (§4.3, §6.3, §9.1).

Three consequences are worth stating where the option is described. **Copy and Download
hand over the source**, `$…$` and all: this is the one setting where what is on the
screen is not what leaves the app, and the control's hint says so. **The three rendering
options of the *Math* fieldset stop meaning anything** — `math_expression_in`,
`matrix_delimiters` and `math_font` are the renderer's, and Source bypasses the
renderer — so they are disabled while MathJax is selected, with one line saying why,
rather than left as controls that do nothing. And **fit-to-pane measurement becomes
approximate**: a typeset formula is not a number of columns wide, and *Fit* is still
sent the column count the pane measured for text. That is a known and accepted
imprecision rather than something to correct; *Fit* wraps the text around the formulas,
which is what it can honestly do.

`math` is app-level but is **not** in `DEFAULT_OPTIONS`, unlike `wrap` and `todayMode`:
three of its four answers are the library's, and the one it starts on — *Fancy* — is
the library's own default, so absent means for it what absent means everywhere else.
The one accommodation for the move is on the way in: `sanitizeOptions` reads a
`mathMode` in stored or shared data as `math`, because links and settings written
before the mode existed carry the old spelling of the same choice. Nothing writes it.

**No backwards compatibility was kept for the change of default.** A share link or a
stored setting that omits `wrap` now means *Soft* where it used to mean *Fit*, and no
migration writes an explicit `wrap: 'fit'` into older state. This is deliberate: absent
means "the app's default" everywhere else in this file (§6.4), and freezing one key's
old meaning for the benefit of links already in the wild would cost that rule more than
it is worth. The visible consequence is that Copy and Download over an old link now
hand over unwrapped long lines — which is what *Soft* is, and what the reader's own
text viewer is equipped to fold.

**"More options"** — a `<details>` disclosure, three fieldsets:

*Layout*: heading style (`heading_style`, 4 values) · footnote style
(`footnote_style`, 3) · keep comments (`keep_comments`) · **text char styles**
(`text_font`: on / off — off means `\textbf` stops producing 𝐛𝐨𝐥𝐝) · `\today`
(browser date / `<today>` / custom → `today(Option<Box<str>>)`) · keep everything
offline (§8.3, §9.1, app-level).

*Math*: expression delimiters (`math_expression_in`: parens / braces / none) · matrix
delimiters (`matrix_delimiters`: unicode / ascii) · **math char styles** (`math_font`:
italic (default) / upright / off, and the other Unicode alphabets for anyone who wants
their variables in 𝔣𝔯𝔞𝔨𝔱𝔲𝔯). All three are disabled, with a line of explanation, while
*Math: MathJax* is selected (above).

The two `*_font` options are Unicode *character* styles — which alphabet a letter is
mapped into — and have nothing to do with the display font of the primary bar, which
is CSS and changes nothing about the text. The labels keep them apart: **display
font** versus **text/math char styles**.

*Parsing*: **macro definitions** (`macro_definitions`, 2 values — added at M9) ·
unknown macros (`unknown_macro`, 4 values) · unknown environments (`unknown_env`, 3) ·
unknown specials (`unknown_specials`, 2) · strict (`recovery(Recovery::Strict)`).

**Macro definitions** is labelled in the user's words rather than the library's:
*Expanded where they are used* (`MacroDefinitions::Honored`, the default) and *Read and
dropped* (`Declared`). `Honored`/`Declared` is precise and means nothing to a reader,
while what the two settings *do* — a `\newcommand` written in the document either takes
effect or does not — is one line of copy. It sits in *Parsing* because that is where it
happens: the definers are read by the token reader under the parser, not applied by a
rendering rule (Appendix A). Example 3 of §6.7 is the fastest way to see the difference.

**Not exposed**, each with a comment in `options.rs` saying so: `list_style` (two
arrays of strings — a UI in itself, and rarely the thing someone came to change),
`unknown_macro_resolution` (subtle interaction with `recovery`; the "strict" checkbox
covers the observable case), `descent_guard` (a safety limit, not a preference),
`source_resolver` (no filesystem in a browser), `override_*` and custom definitions
(an extension API, not an option), and — **added at M9** — `expansion_depth_limit` and
`expansion_count_limit`. The two budgets are `descent_guard`'s case exactly: a safety
limit rather than a preference, and a page that could raise one would be a page a
one-line document can hang. They differ from `descent_guard` in one way that saves this
file some work: the library's defaults (64, 2 000) are *already* the conservative
choice — techxt lowers techy-xp's own count budget fiftyfold precisely because it
converts documents it did not write, which is this page's situation — so unlike the
descent guard there is nothing for the binding to set, and it sets nothing.

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
│ Wrap [Soft ▾] Math [Fancy ▾] Font [JuliaMono ▾] Library ▸More│  primary bar (sticky)
├───────────────────────────┬──────────────────────────────┤
│ LaTeX             [Load ▾]│ Text  [☆][Copy] [Download]   │  pane headers
│                           │                              │
│ <textarea>                │ <pre>                        │
│                           │                              │
├───────────────────────────┴──────────────────────────────┤
│ ▸ 3 warnings · 128 ms · 1 240 chars                      │  status + diagnostics
└──────────────────────────────────────────────────────────┘
      (About, Install and the library are sheets over this, not a page under it)
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

The one exception is *Wrap: Soft* (§5) — which, being the default, is the state the
pane is usually in: it adds `white-space: pre-wrap` plus
`overflow-wrap: break-word` (the second so a line with no space in it — a long URL, a
`\verb` run — is folded too rather than left to overflow). This is not the browser
second-guessing the library; it is the pane admitting it is narrower than a page, and
folding rather than hiding what does not fit. Nothing else changes: `Panes.setSoftWrap`
only toggles a class, the `<pre>`'s `textContent` still holds the library's long lines,
and Copy and Download read `Panes.getOutput()` rather than anything the DOM has
folded.

**A math region may be wrapped in an element *after* the text has been set**, which is
the one thing besides text that is ever in the pane, and the `textContent`-only rule
above survives it intact. Under *Math: MathJax* (§5) the pane sets the string exactly as
it always does, then walks the region table the binding reported beside it (§4.3) and
wraps each range in a `<span>` — every node built with `createElement` and
`createTextNode` around a slice of the string that is already there, no markup parsed,
no `innerHTML` anywhere. `Panes.markMath` hands those elements back and `main.ts` gives
them to `src/mathjax.ts`, which replaces each one's contents with an SVG; until it does,
each span still reads as its own LaTeX, so the pane is never blank and never blocked.
The cutting itself is `src/math-regions.ts`, a pure function whose one invariant is that
the runs concatenate back to the text they came from — which is why `Panes.getOutput()`
still returns the string that was set, and Copy, Download and the library still hand
over the library's own bytes.

Typesetting is asynchronous and can be slow, so it carries the same discipline the
worker's requests do (§6.2): each pass is numbered, passes are chained rather than
raced — MathJax holds one document's worth of state — and a pass whose conversion has
been superseded before its turn comes is dropped rather than rendered into elements the
pane has already replaced.

Copy uses `navigator.clipboard.writeText` with a `<textarea>`+`execCommand` fallback
for older iOS; Download builds a `Blob` and a temporary object URL, named after the
first `\title`/`\section` if one exists, else `converted.txt`.

### 6.4 State, persistence and sharing

One versioned object:

```ts
interface AppState { v: 1; doc: string; opts: AppOptions; ui: UiState }

// AppOptions is Partial<Options> plus the three app-level settings of §5, which are
// not library options and must not be sent to the binding as if they were:
//   wrap: 'fit' | 'off' | 'soft' | number  →  wrapWidth, once the pane has been
//       measured. 'soft' resolves exactly as 'off' does; its display half is read
//       back out by `softWraps(opts)` and applied to the pane, not to the payload.
//   math: 'fancy' | 'plain' | 'source' | 'mathjax'  →  mathMode. 'mathjax' resolves
//       exactly as 'source' does; its display half is read back out by
//       `mathJax(opts)` and typeset in the pane (§6.3).
//   todayMode: 'browser' | 'library' | 'custom' (+ todayCustom)  →  today
// `resolveOptions(opts, columns)` in state.ts is the single place that translation
// happens, so the worker never sees an app-level value.
```

- **localStorage**, debounced 500 ms, three keys (`techxt.doc.v1`, `techxt.opts.v1`,
  `techxt.ui.v1`); the document is capped at 512 KB with the excess simply not stored
  (and a note in the status line), so a huge paste cannot break the quota and lose the
  settings too. Two further keys belong to the library and carry no document:
  `techxt.library.hints.v1` (whether it has been introduced yet) and
  `techxt.library.current.v1` (which entry this session is writing into, so a reload
  continues it — §6.10). The library itself is in IndexedDB, so it can never cost the
  user their settings.
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

Under *Math: MathJax* the measurement is knowingly approximate: a typeset formula is
not a whole number of the gauge's characters wide, so a line the library wrapped to fit
may render narrower or wider than the pane once its formula is an SVG. *Fit* is still
sent the column count measured for text, which is what it can honestly measure, and the
error is left uncorrected rather than guessed at (§5).

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

`src/examples.ts` holds **six** short documents (five until M9), inlined as string
constants so they cost no fetch and work offline. They are listed here in the order the
Load menu shows them, and each is at most ~15 lines — a demo, not a corpus:

1. **A paper fragment** (the default): `\section`, `\emph`, an accent, an inline
   formula, a `\footnote`, a `\cite`. Shows the headline behaviours in one screen.
2. **Mathematics**: sums with limits, a fraction, a square root, Greek, a `matrix`,
   a display equation — the case for `math_mode` and for the display fonts.
3. **Macros of your own** *(added at M9)*: a small preamble — `\newcommand{\ket}`,
   `\braket`, a `\newenvironment` — used in the paragraph below it, so M9's headline
   behaviour is visible in one screen. Its aside names the *Macro definitions* control
   of §5, which makes the example its own demonstration of what that setting does. It
   sits third rather than last because a behaviour this central should not be at the
   bottom of a menu.
4. **Lists and tables**: nested `itemize`/`enumerate` and a `tabular` that aligns.
5. **Accents and symbols**: `\"o`, `\'e`, `\c{c}`, `\ss`, dashes, quotes,
   `\alpha…\omega`, arrows — the long tail, and a font stress test.
6. **Unicode passthrough**: a paragraph mixing LaTeX markup with CJK, Hebrew and an
   emoji — the case the fallback chains of §8.2 exist for, and the one a reviewer
   should look at before believing them.

The invariant is unchanged and was re-verified for all six at M9 — every one converts
with no diagnostics at all, the new one included, under a parse that now expands macros.

A **Load ▾** menu in the input pane header offers them; choosing one replaces the
document (with a single-level undo via the toast, since it discards work).

### 6.8 The sheets

The page does not scroll. The tool is one viewport tall and everything else is a
`<dialog>` opened with `showModal()` over it — the top layer, the backdrop, Escape,
the focus ring and the inertness of the tool behind are the platform's, and none of
them is reimplemented (`src/ui/sheets.ts`). A sheet is a card on the desktop and the
whole screen on a phone. The header's **About** and **Install** are buttons, not
anchors: the fragment belongs to the share codec (§6.4), and a nav link that
overwrites it would cost a reader their document on reload.

There are three: **About** and **Install**, whose bodies are the prose in
`index.html`, and the **library** (§6.10), whose body is built in TypeScript because
it is a list of the user's own documents rather than anything that could be written
ahead of time.

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

### 6.10 The library

Every document that goes through the app is kept, on the device, in a library the user
can search, star, rename, delete, export and re-import. **Saving is automatic**: this
is a log of what was converted, the way a browser keeps history, not a folder the user
files things into. The button beside Copy and Download is therefore not *Save* but
**⭐ Save**, meaning *star this* — starring marks an entry worth keeping and the pane
can filter to starred entries. Nothing is lost because somebody forgot to press a
button, and there is one less concept to explain.

**An entry** holds `id`, `createdAt`, `updatedAt`, `title`, `source`, `options` (the
full `AppOptions` in force, pruned as everywhere else), `starred`, and a small
`preview` of the rendered output — the first six lines or 400 characters, whichever is
shorter. The preview exists so a card is legible and an export is worth reading; the
real rendering is always regenerable from `source` and `options`, so it is allowed to
be stale and is never what the user acts on.

**One current entry per editing session**, created on the first conversion of a
non-empty document and updated in place from then on, debounced 2 s and on `pagehide`.
Changing only the options updates it too. A new entry begins where the app already
knows the user has moved on: Load ▾, the `.tex` file handler, opening an entry from the
library, an import that replaced everything — and after a 30-minute idle gap. A
*reload* is none of those: the id of the current entry is kept in `localStorage`
(`techxt.library.current.v1`) and adopted on load when the document came from storage,
so coming back to a tab continues the entry instead of logging a second copy of it.

**The title** is the document's own: the first `\title` or `\section`, else the first
non-empty line, else the date (`src/title.ts`, shared with Download's file name). It
follows the document until the user renames the entry, and then stops — `library.ts`
tells the two apart by asking whether the stored title is still the one the stored
source would have produced, which costs no extra field. There is no prompt on save: the
save is automatic, and asking someone to name something they did not ask to save would
be absurd.

**Storage is IndexedDB** (`src/library-store.ts`): one database, one object store keyed
by `id`, with indices on `updatedAt` and on a derived `star` of 0/1 — IndexedDB cannot
key on a boolean, so the flag is stored twice and the derived half never leaves that
file. `localStorage` keeps the session state as it does today; the two are separate
stores and neither can exhaust the other. `navigator.storage.persist()` is asked once,
before the first write, so the browser stops treating the library as evictable.

A browser that will not give us a database — a locked-down profile, some private
windows — gets the same treatment `browserStorage()` gives a refused `localStorage`: an
honest inert pane that says so, the ⭐ button hidden rather than dead, and everything
else working exactly as usual.

#### Retention: the app never quietly drops the user's data

This is the rule the rest of the storage design serves, and it is not negotiable.

- **Nothing is ever deleted for tidiness.** Not because an entry is old, not because it
  is "expired", not because there are a lot of them. There is no scheduled prune, no
  age cutoff and no cap on the number of entries anywhere in the app. The user deletes
  what they want gone, and only the user.
- **Starred entries are never removed by any automatic mechanism, under any
  circumstance.** `prunableEntries()` is the only function that ever proposes a
  removal and it cannot see a starred entry; a library in which everything is starred
  proposes nothing at all.
- **If storage genuinely runs out** — a write actually failed, not a number that looked
  close — the app *proposes*, once per session: it says plainly that storage for this
  site is full, offers **Export library** first and prominently, and only then offers
  to remove the oldest unstarred entries, naming how many, which dates they span, and
  how many entries (and starred ones) would remain. Nothing is removed without an
  explicit answer to that dialog.
- **A refusal is a complete answer.** If the user declines, the app stops logging new
  documents for the rest of the session and says so in the status line; everything
  already in the library is untouched. A library that has stopped growing is a
  nuisance; a library that ate the user's work is a betrayal. Take the nuisance.
- **Warn early enough that the dialog is rare**: past 80 % of
  `navigator.storage.estimate()`, one unobtrusive line in the library header naming
  Export as the remedy. Not a modal, not every session.
- **A failed write is loud**: a toast with an **Export library** action, never a
  dropped entry and a shrug.
- **Say where the user stands without being asked**: the header carries
  `142 entries · 8 starred · 3.1 MB`, and a session whose data the browser will
  probably not keep says so with a ⚠️ line pointing at Export.

A single entry's `source` is capped at the same 512 KB as `MAX_STORED_DOC`. Over it the
document is not logged at all — never logged truncated — and the status line says so;
one huge paste must not be the reason something else is lost.

#### The pane

A `<dialog>` sheet like About and Install (§6.8), because a scrolling list of entries
belongs inside a dialog in an app whose page never scrolls, and because the sheet
machinery already gives Escape, the backdrop, focus handling and inertness for free.
It opens from the header, beside About and Install, and from the primary options row
next to *More options* — one action, two doors, and no third row on a phone.

Desktop shows the list on the left and the selected entry on the right; below 860 px it
is one column and a tap pushes to the entry's detail with a back control, which is a
`data-view` attribute and a media query rather than a second rendering path. Per entry:
open, star, rename, delete, copy and download its source. Filters are all/starred plus
a text search over title and source, sorted by most recently updated. Delete offers an
Undo in the toast; **Clear library** demands a typed confirmation and names the count
and the starred count in it.

Opening an entry restores its document *and* its options. That is not destructive — the
settings being replaced belong to the current entry, which is itself in the log and one
click away — and it still offers the usual single-level undo in the toast.

Every node in the pane is built with `createElement` and `textContent`: entry titles,
previews and imported text are all somebody's own text, and §6.3's rule about
`innerHTML` is not relaxed for a card.

**Toasts follow the modal.** A `showModal()` dialog makes the rest of the document
inert, so a toast raised from inside the library sheet would be drawn under it and its
Undo would not answer a click — and the top layer is no escape, since a popover shown
over a modal is painted above it and is still inert. `ui/toast.ts` therefore moves the
toast mount into the open dialog for as long as one is open and moves it home when the
dialog closes. It is `position: fixed`, so it lands in the same place either way.

**Discoverability**, since the library only helps if people know it is there: the first
time an entry is logged, one toast — *"Saved to your library"* with an **Open library**
action — shown once, ever; and the library button in the primary bar tints itself for
the first three sessions, driven by a counter in `localStorage`
(`techxt.library.hints.v1`), stopping for good the first time the pane is opened. About
gains a sentence saying the library is stored on this device only, is never uploaded,
and is never trimmed to save space.

### 6.11 Library export and import

**Export** writes the whole library as one JSON file through the same `Blob` path the
output's Download button uses, named `techxt-library-YYYY-MM-DD.json`. The format is
versioned and boring on purpose, and carries each entry's preview so an imported
library is legible before anything has been re-converted:

```json
{
  "format": "techxt.library",
  "v": 1,
  "exportedAt": "2026-08-28T12:00:00.000Z",
  "app": "techxt-web",
  "techxt": "0.1.0",
  "items": [ { "id": …, "createdAt": …, "updatedAt": …, "title": …,
               "source": …, "options": { … }, "starred": …, "preview": … } ]
}
```

Timestamps are ISO 8601 strings rather than epoch numbers: the file is meant to be
readable by a person who opens it in an editor a year from now.

**Import asks first**, in a dialog, so nothing about the result is a surprise:

- **Add to my library** (the default) — everything already there is kept; an incoming
  id that collides with one of the user's gets a fresh id rather than overwriting an
  entry. Importing a library into itself therefore produces a second copy, which is
  what "add" means.
- **Skip items I already have** (a checkbox on the above) — matched on a hash of
  `source` + `options` and confirmed by comparison, never on the id, because two
  libraries grown from one export share ids by accident and two copies of a document
  generally do not. It skips the *incoming* entry; it never touches the one already
  there.
- **Replace my library** — behind its own typed confirmation, which names how many
  entries will be removed and how many of those are starred, and offers Export first.

**An import never removes an existing entry unless the user chose Replace on that
particular import.** No heuristic, no "clean up duplicates", no exception. This is a
property of `planImport()` in `src/library-io.ts` — outside `mode: 'replace'` its
`remove` list is empty by construction — and it has its own tests saying so.

The outcome is reported plainly: *"12 added, 3 skipped, 0 replaced."*

**A file is hostile input**, and is read with the discipline `decodeShare()` already
uses: every field through a validator, unknown fields dropped, unknown option values
dropped (`sanitizeOptions` is exactly that function), an oversize `source` skipped
rather than truncated, a size cap ahead of `JSON.parse`, and a read that never throws.
A refusal names what was wrong with the file — a foreign file, a truncated one and one
from a future format version send a person to three different places, and a single
"could not read that" would send them nowhere.

## 7. The diagnostics panel

Collapsed by default, summarised in the status bar as
`▸ 2 errors · 3 warnings · 128 ms`, with severity-coloured counts (and a neutral
"clean" state when there are none — the absence of diagnostics is information too).

Expanded, each row is: severity chip · `identifier` in monospace · message ·
`line:column`. Clicking a row focuses the textarea and
`setSelectionRange(start, end)`s the span (§4.4), scrolling it into view. An expander
per row reveals techy's own `rendered` form, which carries the caret line and the trace
frames — the same text `techxt-cli` prints, which makes a screenshot from the web app
directly comparable to a terminal report.

**Amended for M9**, two rows that used to be one:

- A diagnostic with `approx: true` (§4.5) is clickable, gutter-painted and treated like
  any other — it *has* a span. Only its position column says which span: `12:5 (via
  macro)`, and its expanded detail opens with one line saying that the position is where
  the macro is used while the report below it is positioned inside the expansion. The
  alternative — showing an expansion's position as if it were the document's — is the
  one thing §4.5 exists to prevent.
- A row whose `span` is `null` is still not clickable and still says why, but the
  sentence is about expansion rather than about the document: *"this arose inside an
  expansion with no call in your document to point at"*. After M9 that is the residual
  case rather than the whole of the story.

One more thing that had to change with them: a row is keyed by its **position in the
list**, not by `identifier|start|message`. Two rows can now be identical field for field
— the same unknown macro raised twice inside the same expansion carries the same
substituted span — and a content key would collapse them, so one expander would open
both details and a gutter click would land on whichever was rendered last. `reveal`
finds its row by object identity instead, which is what the gutter marker was painted
from.

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
moment it is chosen. **Keep everything offline** in More options fetches all five
deliberately, for someone about to board a plane. If a face was never fetched and the
network is gone, the swap simply does not happen and the chain of §8.2 renders —
nothing to handle.

The checkbox is *everything* rather than *all fonts* because the app has exactly two
kinds of asset it fetches after the first load, and someone ticking this is answering
the same question about both: the faces here and the MathJax bundle of §9.1, which the
same tick asks for. One setting rather than two, which is what the box asks for — a
second checkbox for the second lazy asset is how a preferences screen starts. The
stored key stays `keepFontsOffline`, so a profile written before the label changed
keeps its answer.

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
  cache on first paint and works offline from then on (§8.3). The wasm module (~1.07 MiB
  since M9; ~890 KB when this was written) is under Workbox's default 2 MiB per-file
  precache cap; the cap is set explicitly anyway, so future growth fails the build
  loudly instead of silently skipping the engine.
- **Offline**: the app — shell, engine, worker — is precached, so a cold offline start
  works. The runtime requests the page makes are same-origin and few: the display font
  in use (§8.3), and, if the user asks for MathJax, the typesetter (§9.1). Nothing
  third-party is contacted at any point: no CDN, no analytics, no error reporting, and
  no document leaves the device. This is stated in About and is worth keeping true.
- **Updates**: `autoUpdate` plus a toast ("A new version is ready — Reload"). The
  document is already in localStorage, so a reload never loses work.
- **Stretch (W8), both shipped**: a GET `share_target` (`?text=`) so Android's share
  sheet can send selected LaTeX straight into the app, and `file_handlers` for
  `.tex`/`.latex` where supported. Both are additive — a browser that implements
  neither ignores both manifest fields, and the code paths are only reached when the
  browser calls them. A GET target rather than POST, so no service-worker request
  handler is involved and the app simply reads `?text=` on load.

### 9.1 MathJax is an asset, not part of the bundle

The *Math: MathJax* mode typesets the formulas in the output pane. The typesetter is
MathJax 4's combined TeX→SVG build, and everything about how it reaches the browser is
decided by two facts: it is very large, and it must never be fetched from somebody
else's server.

**Bundled, never a CDN.** A `<script src="https://cdn.jsdelivr.net/…">` would break both
the offline story above and the privacy claim in About, and it would do so silently. So
the `techxt:mathjax` plugin in `vite.config.ts` copies MathJax into `dist/` and
`src/mathjax.ts` points every MathJax path at our own origin. This takes more than
copying one file, because **MathJax 4 fetches from a CDN in two places if left alone**:
`loader.paths.fonts` defaults to jsdelivr, and the speech-rule engine pulls its locale
tables from there the first time it is asked to describe a formula. Both are shut off —
the paths redirected, the speech, braille, enrichment, explorer and menu layers disabled
— in the configuration `src/mathjax.ts` installs before the script runs.

**SVG output, and the font ranges nobody expects.** SVG rather than CHTML: CHTML is
smaller as a bundle but pulls 105 woff2 faces, 1.8 MB, that an offline-first app would
have to precache. SVG carries its glyphs as path data. It is *not*, however, the single
self-contained file it looks like: the `mathjax-newcm` SVG font is split into 40
character-range modules, of which the bundle carries the common ones and the rest are
loaded on demand. A formula reaching outside them asks for one more file — `\mathbb{R}`
wants `double-struck`, `\mathcal{H}` wants `calligraphic` — and the app's own examples
reach both. So the whole range set is served from our origin as well. It is 9.6 MB in
`dist/` and almost none of it on the wire: the shipped examples between them pull 82 KB
of it, and a document that stays inside the bundled ranges pulls none.

**Lazily fetched, then held.** `src/mathjax.ts` injects the script the first time
`loadMathJax()` is called, which is the first time the user selects the mode; the other
visitors — the great majority — never pay for it. The service worker holds the bundle
and every range it asks for in a `CacheFirst` route, `techxt-mathjax`, exactly like the
one that holds the display faces: once fetched, the mode works with the network off and
after a reload. Nothing MathJax is *pre*cached — `globIgnores` keeps `mathjax/**` out of
the precache manifest — because putting 1.8 MB on the install path of every visitor to
serve the few who want it is the trade the runtime route exists to avoid.

**Lazy on the web, complete when installed.** `main.ts` asks for the bundle the moment
the mode is selected rather than when the first formula arrives — a click, a share link,
a library entry and a reload into the mode all go through the same idempotent call — so
the fetch overlaps the conversion it belongs to. An **installed** copy asks for it once
on every run, in the background and off the idle callback, whether or not anyone selects
the mode: an app that was installed to work offline should not discover on a train that
its typesetter is a download away, and after the first run the call is a cache read. The
test is `display-mode: standalone`; on the web the same speculation would be 1.8 MB
spent on visitors who never turn the mode on, which is the whole point of the route
above. **Keep everything offline** (§8.3) asks for it too, deliberately, for the person
who is about to lose the network on purpose.

**Two things in the configuration are load-bearing and were both found in a browser
rather than on paper**, because neither fails loudly:

- The five `enableSpeech`/`enableBraille`/`enableEnrichment`/`enableExplorer`/
  `enableMenu` options are **not enough on their own**. MathJax's contextual menu
  applies its *own* settings to the document after the configuration is read, and its
  defaults turn enrichment, speech and braille straight back on; `enableMenu: false`
  hides the menu without stopping that. The document then reaches the `attachSpeech`
  render action, starts a web worker for the speech-rule engine, and waits forever for
  an answer — `tex2svgPromise` never settles, no error is raised, and not one formula is
  typeset. `options.menuOptions.settings` turns the menu's own answers off as well,
  which is what actually keeps the speech engine out of the picture.
- The service worker's route matcher **cannot close over anything in
  `vite.config.ts`**: workbox serializes the function by its source into `sw.js`. A
  matcher written as `` url.pathname.startsWith(`${BASE}mathjax/`) `` compiled to a
  reference to a variable the worker does not have, threw on every request, and the
  route silently never matched — the mode worked online and failed offline, which is
  precisely what it exists to prevent. It reads the base from the worker's own
  `registration.scope` instead, which is `BASE` by construction and needs nothing from
  the config module.

**The version is the cache key.** These files are copied verbatim rather than passed
through Rollup, so they carry no content hash. They are served from
`mathjax/<version>/…` instead — an upgrade changes the directory, so a year-long cache
entry can never outlive the engine that understands it. The version is read from the
installed package, in one place, in `vite.config.ts`, and reaches `src/mathjax.ts`
through a `define`.

**What it costs.** `dist/` grows from 4.1 MB to 15.9 MB (excluding source maps):
1 849 625 B for `tex-svg.js` (616 713 B gzipped) and 9 968 318 B for the 40 font ranges.
The app's own bundle does not move — MathJax is not imported by it — and the precache
manifest does not move either. Item 6 of the TODO owns what to do about the number.

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
   set roughly 20 % over the measured `opt-level = 3` figures — so a dependency mistake
   shows up as a failed build rather than a slow page, and so the §4.7 decision to
   trade size for speed gets revisited deliberately. The budget's actual values live in
   `.github/workflows/web.yml` (`WASM_MAX_BYTES`, `WASM_MAX_GZIP_BYTES`) and nowhere
   else; §14 records what each measured build came in at.
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
codec, options diffing, fit-to-pane arithmetic, the region → run cutting behind the
MathJax mode, worker-protocol sequencing with a mocked worker, the document-title rules,
and the library's model, retention policy and import/export codec against an in-memory
backend; `tsc --noEmit`; the glyph coverage check.

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
8. The library (§6.10): convert, reload, and find one entry rather than two; star it
   and delete another, with the Undo in the toast reachable from inside the sheet;
   export, then import the file back under each of the three answers; open an entry
   and see its options come back with it; the pane one-handed on a 390 px screen; and
   a profile with IndexedDB blocked, where the app must be whole and the pane honest.
9. *Math: MathJax* (§5, §9.1): the formulas typeset while the rest of the pane stays
   readable; Copy still hands over the source, `$…$` and all; a wide display formula
   scrolls in its own box rather than dragging the pane; and — the one that has failed
   before — a reload with the network off, after the mode has been used once, still
   typesets, with no request leaving the origin at any point.
10. Lighthouse: PWA installable, performance ≥ 95, accessibility 100.

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

### Measured at M9 — the binding with techy-xp linked in

The parse now goes through techy-xp's `LatexlikeXp` (root PLAN M9), and the expansion
machinery is in the module whether a document defines a macro or not:

| Quantity | Value |
|---|---|
| wasm as shipped, before M9 (W7 figure above) | 939 287 B raw · 333 KB gzip |
| wasm as shipped, at M9 (`opt-level = 3` + LTO + `wasm-opt -O3`) | **1 120 513 B raw · 398 525 B gzip** |
| CI budget *as it stood at M9* (§11) | 1 150 000 B raw · 400 000 B gzip |
| headroom | 29 487 B raw (2.6 %) · **1 475 B gzip (0.4 %)** |
| the same build at `opt-level = "s"` (`wasm-opt -O3` unchanged) | 895 214 B raw · 343 435 B gzip |

**The gzip budget is now a tripwire rather than a ceiling.** 1 475 bytes is one
`String::from` away from red, and the number that matters is the one §4.7 names as the
decision point: "if the module grows past the gzip ceiling … drop to `"s"` first".
This build is 1 475 B short of it. The budgets are deliberately left where they are — raising
one is how a size budget stops meaning anything — and the last row is here so the trade
can be made on evidence: `opt-level = "s"` buys 55 KB gzipped back, at a speed cost this
file has still never measured — the paragraph below has said so since W1, and it is the
measurement to take before the trade, not after. It is a decision for the owner
(Appendix D), not something to slip into a commit that was only meant to make CI green.

*(Superseded — read the present tense above as of M9. The tripwire fired at M10 and the
ceiling was raised on 2026-08-28; see the rev-bump subsection below.)*

The three build profiles differ by 106 KB gzipped between the fastest and the
smallest. `opt-level = 3` takes the speed; the other two rows are here so the trade
can be reversed on evidence rather than re-measured from scratch (§4.7). The speed
side of that table is still unmeasured — W7 fills it in from the browser.

### Measured at the techy `736c97c` / techy-xp `58c8aef` rev bump — the tripwire fires

The upstream parse-entry rework (`parse_source` becoming `parse_setup(...).parse()`,
plus a defaulted `ParseDriver::make_root_parser`) touches nothing this app calls, and it
is not what moved the needle:

| Quantity | Value |
|---|---|
| wasm at M9 (table above) | 1 120 513 B raw · 398 525 B gzip |
| wasm on `main` at 94c0366, **before** this bump (M10 in, never re-measured) | 1 130 119 B raw · **401 841 B gzip** |
| wasm at techy `736c97c` + techy-xp `58c8aef` (`opt-level = 3` + LTO + `wasm-opt -O3`) | **1 130 392 B raw · 400 993 B gzip** |
| the same build at `opt-level = "s"` (`wasm-opt -O3` unchanged) | 898 175 B raw · 344 828 B gzip |

Measured against the gzip ceiling **as it stood that day**, the middle row was 993 B
over and the row above it 1 841 B over. (The ceilings themselves are not restated here
— `.github/workflows/web.yml` holds the only copy, per §4.7.)

The bump itself is size-neutral, marginally favourable: −848 B gzipped, +273 B raw.
**The gzip budget was already breached on `main` before it.** M10 (the render side made
generic over `LatexlikeLang`, root PLAN §16) spent the 1 475 B of headroom M9 left and
was never re-measured, so the size step failed on `main` for reasons that had nothing to
do with techy's revision.

So §4.7's decision point was reached exactly as the M9 note above predicted, and the
answer it names is unchanged: `opt-level = "s"` buys 56.2 KB gzipped back — at a speed
cost this file has still never measured.

**The owner's decision (2026-08-28): raise the gzip ceiling for now, and leave
`opt-level` alone.** The measurement is why it is only *for now*: 993 B over is not
evidence that the module is too big, it is evidence that the tripwire sat where the plan
wanted it and has now fired once. The M9 note's warning — "raising one is how a size
budget stops meaning anything" — is the cost being accepted here, knowingly and once.
The answer to a *second* firing is `"s"`, and the input that trade still lacks is the
browser-side speed comparison the profile table has wanted since W1.

The **raw** ceiling was left untouched, and the same build sits 19 608 B (1.7 %) under
it — proportionally tighter than the gzip line now is. Raw is the likelier of the two to
fire next.

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
CodeMirror would outweigh the engine); rendering the document visually for comparison —
*Math: MathJax* typesets the formulas of the answer, in the answer's own pane, and there
is no second pane showing what LaTeX would have produced (§1, §5);
multi-file/`\input`; a definitions playground (the extension API is a crate-docs
subject, not a UI); server-side conversion for very large documents; i18n of the UI;
any analytics.

The library of §6.10 is a log on the device, and stays one: no accounts, no sync
between devices, and no server to sync with. **Export is the answer** to "I want this
somewhere else", which is why it is also the answer offered first when the disk fills
up. A File System Access backend (`showSaveFilePicker` and a persisted handle) would
buy more room, but only on Chromium and never on iOS, so it is not the base feature;
it is worth revisiting as a desktop convenience if the quota warning turns out to fire
in practice.

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
| `macro_definitions` *(M9)* | `MacroDefinitions::{Honored, Declared}`, `#[non_exhaustive]` | `Honored` |
| `expansion_depth_limit` *(M9)* | `usize` | `ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT` = 64 |
| `expansion_count_limit` *(M9)* | `usize` | `ConverterBuilder::DEFAULT_EXPANSION_COUNT_LIMIT` = 2 000 |

Of the three M9 setters the app exposes **`macro_definitions` only** (§5): the two
budgets are safety limits whose library defaults are already the conservative ones, and
`options.rs` carries the `// not exposed:` comment that says so. `MacroDefinitions` is
`#[non_exhaustive]`, so the DTO keeps its own closed copy of the two variants rather
than mirroring the enum.

New diagnostic identifier families a user can now see, all of them ordinary rows in the
panel: `techy-xp.expand.*` (the two budgets, errors), `techy-xp.define.*` and
`techy-xp.constructs.*` (a definition techy-xp will not accept), and
`techy-xp.presets.*-unsupported` (`\expandafter` and the TeX conditionals, which reach
the caller demoted to warnings).

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
Source::content() -> &str                                  // content-compare to find "our" source

ParseError::{identifier, message, span, frames, render}    // same shape, for the Err case
```

**Amended for M9: content identity is no longer the only mechanism.** It is still how
the binding decides whether a span is in the buffer, but a span that fails the test is
no longer simply dropped (§4.5) — the binding then asks the diagnostic's frames and the
source's provenance where the expansion was *invoked*:

```rust
Source::provenance_chain() -> impl Iterator<Item = &SourceProvenance>
    // this source's provenance, then each triggering source's, ending at Primary
SourceProvenance::triggered_at() -> Option<&SourceSpan>
    // where a Synthesized (macro expansion) or Resolved (\input) source came from
Source::including_sources() -> impl Iterator<Item = &Source>   // the sibling iterator
```

`SourceProvenance` is techy's, and `techxt::convert` does **not** re-export it — but
none of the three lines above needs to name it, since a method call on a value does not
require the type in scope. So the binding uses them while still depending on `techxt`
alone, and `rust/` needs no change to make the panel able to point at a macro call.

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

- **`techy` is a git dependency** pinned to rev `736c97c`, so a cold build needs
  network. `web/crate/Cargo.lock` is committed to pin it for deploys.
- **`techy-xp` joins it at M9**, pinned the same way and to a revision that itself pins
  the *same* techy revision — cargo cannot unify two revs of one git dependency, so the
  two pins move together. It reaches `web/crate` through `techxt`, which is why
  `.github/workflows/web.yml` now watches `rust/Cargo.toml` and `rust/Cargo.lock` as
  well as `rust/techxt/**`: the revision can move without a byte of the crate's own
  sources changing. **`web/crate/Cargo.lock` must be regenerated whenever it does**, and
  again — with a diff to review rather than a rubber stamp — when the workspace flips
  the temporary `techy-xp = { path = … }` entry to its git form: a lock recorded against
  a path dependency pins nothing for a deploy.
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
