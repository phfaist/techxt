# techxt web — work in progress

The queue for the `techy-web-cooler` line of work: a soft-wrap default, an optional
MathJax math mode, a library of saved documents with import/export, and a lighter
editor with highlighting and completion.

**Read this before starting any item.** Every design decision below was taken
deliberately in discussion with the repository owner; a fresh pair of hands should
implement what is written here rather than re-litigate it. Where something is still
open it says so in as many words. Where an item contradicts `web/PLAN.md`, the plan is
edited *as part of that item* — the plan stays normative, so it is updated, never
quietly outgrown.

**Read *Instructions for implementer agents*, at the foot of this file, before you
start and again before you finish.** It has the toolchain setup and its two traps, what
is deliberately out of scope while you work, how to record a finished item here, and
what else finishing one obliges you to do.

---

# Context every item needs

## The shape of the app

`web/PLAN.md` is normative for the app; the root `PLAN.md` is normative for the
library. `web/README.md` says how to build. The bits that recur below:

- **No framework.** TypeScript, one module per region of the page. `src/main.ts` is
  wiring; `src/ui/api.ts` is the contract it programs against; nothing under `src/ui/`
  touches storage, the worker, or knows what a conversion is.
- **Conversion runs in a Web Worker** (`src/worker/`), debounced 120 ms, requests
  carry a monotonic id and a stale answer is dropped.
- **`src/state.ts` is the whole state model** — defaults, validation, `localStorage`,
  the share codec. It touches no DOM and is the part vitest actually covers.
- **Absent means the default.** Only options that differ from the app's defaults are
  stored or shared (`pruneOptions`). A read never throws: corrupt, truncated,
  wrong-version and foreign data all fall back to the default.
- **App-level options are resolved away before the worker sees them.**
  `resolveOptions()` in `state.ts` is the one place `wrap: 'fit'` becomes a column
  count and `todayMode: 'browser'` becomes a date. The binding must never receive an
  app-level value. Two new settings below follow this rule (`math: 'mathjax'`).
- **The page never scrolls.** The tool is one viewport; everything else is a
  `<dialog>` opened with `showModal()` (`src/ui/sheets.ts`), which gets the top layer,
  the backdrop, Escape, focus handling and inertness from the platform.
- **Privacy is the pitch.** Everything runs on the device; no document is ever
  uploaded; after the first load the page makes no network request. Nothing below may
  break that, which is why MathJax is bundled rather than fetched from a CDN.

## Building and checking, in this repo

```sh
cd web
npm ci                # once
npm run wasm          # wasm-pack build crate --target web --release --out-dir pkg
npm run build         # wasm + tsc --noEmit + vite build
npm test              # vitest, over the pure logic only
npm run typecheck
cd ../rust && cargo test          # the library
cd web/crate && cargo test        # the binding, natively
```

`web/crate` lives **outside** the `rust/` workspace on purpose, so `cd rust && cargo
test` cannot see it; `.github/workflows/web.yml` runs its `fmt`/`clippy`/`test` and
enforces the size budgets. `techy` and `techy-xp` are git dependencies — a first build
needs network.

All of this was verified end to end in a fresh container on 2026-08-28. **Setting the
toolchain up from nothing has two traps in it — see *Instructions for implementer
agents* at the bottom of this file before you start.**

## Verified facts these items rest on

Measured, not assumed. Re-measure rather than trust these if the pins move.

1. **`MathMode::Source` re-emits post-expansion LaTeX.** A user macro is *gone* by the
   time the source is reassembled:
   ```
   \newcommand{\ket}[1]{\lvert #1 \rangle}
   A state $\ket{\psi}$.        →   A state $\lvert \psi \rangle$.
   ```
   This is what makes item 2 viable: MathJax only ever sees primitives, never a
   document's own macros. Environments keep their own spelling (`align` stays
   `align`), so the fixed set MathJax must understand is LaTeX's, not the user's.
2. **`\$` is indistinguishable in the output.** `... but not these \$3 and \$4 values.`
   converts to `... but not these $3 and $4 values.` — which is exactly why MathJax
   cannot be pointed at the raw output blob and must be told where the math is.
3. **A source-mode formula is one contiguous run in the output.** Inline math becomes
   a `FlowItem::InlineVerbatim`, which layout never breaks; display math becomes a
   `FlowItem::Verbatim` block, emitted line by line with the current continuation
   indent (so a display formula inside an `itemize` picks up four spaces on every
   line — harmless whitespace inside a formula). Either way the region is a single
   byte range in the output, which is what makes a span table well-defined.
4. **MathJax 4.1.3 component sizes**, measured from the `mathjax` npm tarball:

   | component | raw | gzipped | runtime font fetches |
   |---|---|---|---|
   | `tex-svg.js` | 1 849 625 B | 615 224 B | ~~none~~ **wrong — see below** |
   | `tex-chtml.js` | 997 445 B | 280 899 B | some of 105 woff2 files, 1.8 MB total |
   | `tex-svg-nofont.js` | 873 900 B | 254 793 B | font package separately |

   **The "none" was wrong, and it was the reason SVG was chosen over CHTML.** MathJax 4
   splits the `mathjax-newcm` SVG font into **40 character-range modules**: the bundle
   carries the common glyphs and the rest load on demand from `loader.paths.fonts`,
   which defaults to jsdelivr. `\mathbb{R}` pulls `double-struck` and `\mathcal{H}`
   pulls `calligraphic`, both of which the app's own examples reach — so this is the
   ordinary case, not a corner. Implementing item 2 therefore meant serving all 40
   ranges from our own origin: **9 968 318 B in `dist/`**, of which the seven
   mathematical alphabets are 331 112 B and the rest are the scripts `\text{…}` needs.
   On the wire a reader fetches only the ranges their document reaches — 81 703 B for
   all six shipped examples together.

   So the standing comparison is **SVG at 616 KB gzipped of JS plus ~10 MB of
   on-demand ranges on disk**, against **CHTML at 281 KB gzipped plus 1.8 MB of woff2**.
   CHTML is now smaller on both axes, and the "one self-contained file" argument that
   decided this no longer exists. It was not revisited during item 2 because the work
   was already done and green, and because `src/mathjax.ts`'s four-function API is
   output-agnostic — switching is contained to that file and `vite.config.ts`, and
   nothing that consumes it would change. **Item 6 should weigh it with real numbers.**

   For comparison the wasm module today is 1 199 689 B raw / 421 748 B gzipped as
   built on the 2026-08-28 container, and `web/PLAN.md` §14 records 1 120 513 B /
   398 525 B from a newer stable at M9 — the toolchain moves the number by tens of
   kilobytes, which is worth remembering before reading anything into a size.
5. **`opt-level = "s"` buys 264 KB raw / 68 KB gzipped** — the table in item 6.
6. **techxt ships ~1 100 macros** (956 in the generated `symbols_extra` long tail plus
   ~150 curated), and `Category` currently exposes **no** way to read them back.

## Two changes to `rust/techxt` are in scope

Items 2 and 5 each need something the library does not offer. The bar agreed with the
owner: *a small, well-structured, targeted change that is justified for the library as
a whole* — not a patch that exists only to make the web app's case work. Both are
specified in their items (L1 under item 2, L2 under item 5) with the argument for why
the library wants them anyway. Each carries its own root-`PLAN.md` entry and its own
tests in `rust/techxt/tests/`.

---

# 1. Wrap defaults to soft-wrap

> **Done** — 2026-08-28, `4256ace`. Soft is the default; the select reads Fit the pane / Off /
> Soft (default); PLAN §5 and §6.3 updated. The hint sentence was reworded rather than
> kept verbatim: it named "both Off settings", which stopped being what the list says
> once the third entry became **Soft**. It still explains all three answers.

**The smallest item. Do it first; it is unblocked and touches nothing else.**

`wrap` already has the value `'soft'` internally — it is only the *default* and the
*label* that change.

- [x] `DEFAULT_OPTIONS.wrap` in `src/state.ts` becomes `'soft'`.
- [x] `src/ui/controls.ts` (~line 90): the wrap select reads `Fit the pane` /
      `Off` / `Soft (default)`. Today it says `Off (default)` and
      `Off, soft-wrapped`; the "(default)" marker moves and the third entry is renamed
      to just **Soft**. Keep the hint sentence, which is where the full explanation of
      the three answers lives.
- [x] `web/PLAN.md` §5 and §6.3: the option table's default column, and the prose that
      calls *Fit* "the app being helpful".
- [x] Update `test/state.test.ts` / `test/options.test.ts` wherever they assert the
      default or rely on `pruneOptions` dropping `wrap`.

**Decided: no backwards compatibility.** A share link or a stored setting that omits
`wrap` now means Soft where it used to mean Fit. No migration, no explicit `wrap:'fit'`
written into old state. **And Copy/Download now hand over unwrapped long lines, which
is intended** — that is what Soft *is*, and the reader's own text viewer can fold them.

*Done when*: a fresh profile lands on Soft, the pane folds long lines, Download
produces one line per paragraph, and the tests say so.

---

# 2. `Math: MathJax` — optional visual math

> **Done** — 2026-08-28, `d882ec4` — the app half, on top of `6400254` (L1) and
> `d237825` (the binding and the asset). `math` is an app-level `AppOptions` key
> beside `wrap`, with four values; `resolveOptions` turns `'mathjax'` into
> `mathMode: 'source'` and `mathJax(opts)` reads the display half back out.
> `src/math-regions.ts` cuts the output at the region boundaries (pure, tested in
> `test/math-regions.test.ts`), `Panes.markMath` wraps each run in a `<span>` built
> with `createElement` / `createTextNode` after `textContent` has been set, and
> `main.ts` hands those elements to `src/mathjax.ts` under a numbered-pass discipline
> of the same shape as the worker's request ids. `web/PLAN.md` §1, §2 (D10), §3, §5,
> §6.3, §6.4, §6.5, §8.3, §9.1, §13 and §16 updated; `index.html`'s install sheet says
> what an installed copy fetches.
>
> **The mode did not typeset at all when it was first driven in a browser, and neither
> failure said so.** Both were in the half that landed with the asset, both are fixed
> here, and both are now written into PLAN §9.1 because they are the kind of thing
> that comes back:
>
> 1. **`enableSpeech: false` and its four neighbours are not enough.** MathJax's
>    contextual menu applies its *own* settings to the document after the configuration
>    is read, and its defaults turn enrichment, speech and braille back on, and
>    `enableMenu: false` hides the menu without stopping that. The document then reaches its
>    `attachSpeech` render action, starts a web worker for the speech-rule engine, and
>    waits forever for an answer: `tex2svgPromise` never settles, nothing is logged, and
>    not one formula is typeset. `options.menuOptions.settings` turns the menu's answers
>    off as well. This is the only edit `src/mathjax.ts` needed.
> 2. **The service worker's MathJax route never matched.** Workbox serializes a route
>    matcher *by its source* into `sw.js`, so the `` `${BASE}mathjax/` `` in
>    `vite.config.ts` compiled to a reference to a variable the worker does not have; the
>    matcher threw on every request and the route quietly did nothing. The mode worked
>    online and failed offline — precisely what that route exists to prevent. It reads
>    `self.registration.scope` now, which is `BASE` by construction and closes over
>    nothing.
>
> **The `mathMode` key became `math`, and a read still accepts the old spelling.** All
> four answers live in one control, so the whole control had to move up to the app
> level; `sanitizeOptions` maps a stored or shared `mathMode` onto `math` on the way
> in, so links and profiles written by the previous build keep their setting. Nothing
> writes the old key. This is a smaller accommodation than item 1's "no backwards
> compatibility" for `wrap` — that decision was about a *default* changing meaning,
> and this is a key being renamed under a value the user explicitly chose.
>
> **Two decisions taken inside the *Display and behaviour* boxes.** The display
> formula's box is `display: inline-block` rather than `block`: the newlines around a
> display formula are in the text, and a block box adds a line of its own to them. And
> the *keep all fonts offline* checkbox became **Keep everything offline** rather than
> growing a second one beside it, since the app has exactly two lazily fetched assets
> and a person ticking this is answering the same question about both; the `UiState`
> key is unchanged, so a stored answer survives.
>
> **What it cost in `dist/`**: **+4 776 B** (15 960 563 → 15 965 339 B, excluding
> source maps) — the app bundle 92 595 → 96 764 B (32.62 → 34.10 kB gzipped), because
> `src/mathjax.ts` stops being dead code and `src/math-regions.ts` joins it, plus the
> stylesheet at 29 865 → 30 193 B. The precache manifest moves by the same amount
> (1614.32 → 1619.00 KiB) over an unchanged 21 entries. MathJax's own 11.8 MB was
> already in `dist/` and does not move: it is still not imported by the bundle and
> still not precached.
>
> *Observed*, in Chromium 141 against `npm run preview` on 2026-08-28: the sentence
> from fact 2 renders as one typeset formula with the two literal dollars beside it; a
> `\newcommand{\ket}` document typesets inline and display formulas with no `ket` in
> the SVG and no MathJax error node, because Source had already expanded it; **Copy
> returns the Source-mode text byte for byte**, compared against the same document
> converted with *Math: Source*; a cold reload with the network off, after the mode
> had been used once, typesets from the service worker's cache; the *Mathematics*
> example typesets all six of its formulas and pulls `double-struck` from our own
> origin, with **no request leaving the origin at any point in any run**; a wide
> display formula scrolls inside its own box while the pane does not, on a 1200 px
> window and on a 390 px one; typing through six conversions in a burst leaves every
> formula typeset and none half-rendered; and switching back to *Fancy* leaves plain
> text with no wrapper elements behind.

A fourth value of the *Math* control beside Fancy (the default), Plain and Source. The
formula is emitted as source and MathJax typesets it in the output pane, so a
document's structure and its light font styling can be previewed without paying for
text-mode math that reads badly.

## The architecture, decided

**MathJax is app-level; the library never hears the word.** `math: 'mathjax'` is an
app-level setting exactly like `wrap: 'fit'`: `resolveOptions()` turns it into
`mathMode: 'source'` plus "ask the binding for the math regions", and the worker's
`OptionsPayload` never carries it. It lives in the same *Math* control as the other
three so the user answers one question once.

**Math regions are reported as a side table, not as markers in the text.** The
converted text stays exactly what it is; the conversion result grows a list of
`{start, end, display}` regions naming the byte ranges that are math. Nothing the user
copies, downloads or saves is ever polluted, and there is no marker character to
choose, strip, or collide with a pasted document.

**The output pane keeps `textContent`.** Set the text as it does today, then walk the
region table and wrap each range in an element, then hand those elements to MathJax.
No `innerHTML` anywhere: every node is built with `createElement`/`createTextNode`, so
`web/PLAN.md` §6.3's rule survives intact and only gains a sentence saying that a
region may be wrapped in an element after the text is set.

`Panes.getOutput()` keeps returning the string that was set, so Copy, Download and the
library keep handing over the library's own text.

## L1 — the library change: output regions

> **Done** — 2026-08-28, `6400254`. `FlowItem::Verbatim`/`InlineVerbatim` carry a
> `VerbatimProvenance`; `layout::render_with_regions`/`render_to_with_regions` answer
> with `Vec<OutputRegion>` beside the text (`render`/`render_to` delegate, so it is one
> pass); `Conversion.regions`, with both types re-exported from `convert`. Fact 3 above
> is confirmed as written. Root `PLAN.md` gains §7.1; tests in
> `rust/techxt/tests/output_regions.rs`. Nothing reaches `dist/`: no dependency, a
> counter increment per write, and a `Vec` that never allocates on a document with no
> verbatim content.
>
> **The tag splits in two, and the binding must respect it.** Not `Math { display }` but
> `MathSource { display }` *and* `MathRendered { display }`, beside `KeptSource` and
> `Verbatim`. Only `MathSource` is LaTeX; `MathRendered` is techxt's own aligned output
> (a Fancy display formula's lines, an inline matrix's padded columns), preformatted
> because its columns are fragile. So the binding checkbox below must map only
> `MathSource` into `regions`, or MathJax gets handed rendered Unicode math. Two more
> decisions the sketch left open: a block's range **excludes** the newline terminating
> its last line (it separates the block from what follows), and an item that renders to
> nothing reports nothing.
>
> **Three surprises.** (1) `MathMode::Plain` reports no *math* regions at all — it
> flattens formulas to text, and its display block is an indented text block rather than
> a `Verbatim` — so "all three math modes" has one mode whose math answer is empty. (A
> `\verb` inside a Plain-mode formula still reports, as it should.) (2) An
> `InlineVerbatim` payload can contain a newline — a `KeepSource` macro keeps its
> post-newline — so a region can span a line break inside what layout treats as one
> word. The app wraps regions in elements, so it will meet this. (3) Rendered *inline*
> math contributes regions only where a fragment carries its own spacing, never one over
> a whole formula — an asymmetry with display math that is inherent, not an oversight.
>
> Two files outside L1's stated scope needed mechanical fixes for the breaking enum
> change: `rust/techxt/tests/layout.rs` and `layout_proptest.rs`, five construction
> sites. `web/PLAN.md` is untouched — L1 is a library change and §6.3's sentence about
> wrapping a region in an element belongs to item 2's app half.

**What.** Some runs of techxt's output are not converted text at all: they are source
copied through — math in `Source` mode, an unknown construct under a `KeepSource`
policy, a `verbatim` body. The renderer knows which; the information is thrown away at
the flow/layout boundary. Report it instead, as a side table beside the text, exactly
as diagnostics already are.

**Why the library wants it anyway.** Any embedder rendering into something richer than
a terminal needs it: a GUI styling verbatim blocks in monospace, an HTML backend, a
`techxt-cli --json`. It costs nothing when unused, and it is the one fact about the
output that cannot be recovered afterwards — as the `\$` case above proves.

**Where the code goes.**

- `techxt::flow`: the two verbatim items gain a provenance tag. They are already *the*
  atomic items — layout copies them byte for byte and never re-wraps them — so the tag
  rides the thing that is already a well-defined region. Something like
  `Verbatim { text, provenance }` / `InlineVerbatim { text, provenance }` with
  `#[non_exhaustive] enum VerbatimProvenance { Math { display: bool }, KeptSource,
  Verbatim, … }`. This is a breaking change to a public enum; the crate is alpha and
  says so, and the construction sites are few (`render/math.rs`, `render/mod.rs`,
  `render/rules.rs`, `defs/verbatim.rs`).
- `techxt::layout`: wrap the `&mut dyn Write` sink in a byte counter — all writes
  already go through about eight `self.out.write_str(…)` sites — and record a region's
  output range as it is emitted. Two cases, and the second is the fiddly one:
  - `Verbatim` is written line by line at a known point; record before the first line
    and after the last, so the recorded range **includes** any continuation indent
    layout inserted inside the block. That is correct: those bytes are in the output.
  - `InlineVerbatim` is accumulated into `Engine::word` alongside adjacent `Text`
    items and flushed later at a position wrapping decides. Record the range *within
    the word* as it accumulates, then translate to output offsets in `emit_word` once
    the word's base offset is known.
- `techxt::convert::Conversion` gains `regions: Vec<OutputRegion>` —
  `{ start: usize, end: usize, kind }`, byte offsets into `text`, in output order.
- Tests in `rust/techxt/tests/`: the offsets name the right substrings under every
  wrap width, inside a list (the indent case), for the `\$` document of fact 2, and
  across all three math modes.
- A root `PLAN.md` entry.

**Do not** implement this as a marker string, an option that changes the text, or a
second conversion.

## The binding and the protocol

> **Done** — 2026-08-28, `d237825`. `ConversionResultDto.regions` is a
> `Vec<MathRegionDto>`, filtered to `MathSource` in `diag::math_regions` and mapped to
> UTF-16 through the *same* `OffsetMap` a diagnostic's span goes through — a second
> instance of it, built over the output rather than the input, which is where these
> offsets index. `protocol.ts` gains `MathRegion` and `ConversionResult.regions`.
> `web/PLAN.md` §4.3 and §4.8 updated. Tests in `web/crate/tests/regions.rs`.
>
> **The filter is tested against the library, not only against itself.** One document
> converted twice produces all four provenances — source-mode inline and display, a
> `\verb`, an unknown macro kept as source, and the *same* display formula rendered
> under Fancy — and `regions.rs` asserts what `techxt` itself reported before asserting
> what came through the filter. A test that looked only at the filtered list would pass
> just as happily on a document that never produced the other three tags.
>
> **Two boxes below are not ticked, deliberately.** The DOM-wrapping one is the app
> half's: the three properties are now written down in `protocol.ts`, in `diag.rs` and
> in PLAN §4.3, and the newline-exclusion one is pinned by a test, but the code that
> meets them does not exist yet. The CI budget box under *Shipping MathJax* is
> `.github/workflows/web.yml`, which this change did not touch.
>
> **`protocol.ts` also grew item 5's completion types** — `Completion`, the `complete`
> request and the `completions` answer — written here ahead of the handler that answers
> them, because this file is the one place the Rust side and the app agree on a message
> and both ends have to be written against the same shape. `ToWorker` is now a union,
> which is why `convert.worker.ts` narrows before dispatching; it ignores a `complete`
> message rather than answering one, since `Session::complete` does not exist yet.

**L1 landed, and it changed one thing here — read its `> **Done**` note above before
starting.** `Conversion.regions` is a `Vec<OutputRegion>` whose `kind` is a
`VerbatimProvenance` with **four** variants, not the single `Math { display }` this
section was written against:

| variant | what the bytes are | goes to MathJax? |
|---|---|---|
| `MathSource { display }` | the formula's own LaTeX, post-expansion | **yes — only this one** |
| `MathRendered { display }` | techxt's *rendered* aligned math, already Unicode text | no |
| `KeptSource` | a construct's source under a `KeepSource` policy | no |
| `Verbatim` | a `verbatim` body, `\verb` | no |

- [x] `web/crate/src/diag.rs`: map regions into the result DTO, **filtering to
      `MathSource`** — handing MathJax a `MathRendered` region feeds it techxt's own
      converted Unicode and it will produce nonsense. Convert byte offsets to
      **UTF-16 code units** with the machinery §4.4 already uses for diagnostic spans;
      do not write a second one.
- [x] `src/worker/protocol.ts`: `ConversionResult` gains
      `regions: MathRegion[]` (`{ start, end, display }`) — already flat and already
      filtered, so the app never meets a provenance it has to reason about.
- [x] The binding reports regions unconditionally; the app ignores them unless it is
      in MathJax mode. There is no option to turn them on.
- [x] Three properties L1 measured that the DOM-wrapping code will meet:
      a block region's range **excludes** the newline that ends its last line; a
      construct that renders to nothing reports nothing; and an inline region can
      contain a newline (a `KeepSource` macro keeps its post-newline), so a region is
      not guaranteed to sit within one line of the output.
- [x] `MathMode::Plain` reports no math regions at all — it flattens formulas into
      ordinary text. Nothing to do about it, but do not treat an empty region list as
      a bug when testing across modes.

## Shipping MathJax

> **Done** — 2026-08-28, `d237825`, except the two boxes named below. MathJax 4.1.3's
> `tex-svg.js` is copied into `dist/mathjax/<version>/` by a `techxt:mathjax` plugin in
> `vite.config.ts` (which also serves it from `node_modules` in `vite dev`, so the mode
> is usable without a build); `src/mathjax.ts` injects it with a `<script>` tag on first
> use and exposes four functions — `loadMathJax`, `mathJaxLoaded`, `typeset`,
> `resetMathJax` — so that the app half never names a MathJax option. A `techxt-mathjax`
> `CacheFirst` route sits beside `techxt-fonts`, and `globIgnores` keeps `mathjax/**` out
> of the precache. `web/PLAN.md` §9 gains **§9.1**, which is where the whole asset story
> now lives.
>
> **Verified fact 4 is wrong in its last column, and it matters.** `tex-svg.js` does
> *not* have zero runtime font fetches. MathJax 4's `mathjax-newcm` SVG font is split
> into **40 character-range modules**; the bundle carries the common glyphs and the rest
> load on demand from `loader.paths.fonts`, which defaults to
> `https://cdn.jsdelivr.net/npm/@mathjax`. This is not a corner case: `\mathbb{R}` pulls
> `double-struck` and `\mathcal{H}` pulls `calligraphic`, and the app's own *Mathematics*
> and *Macros of your own* examples reach both. Left alone, selecting MathJax mode on the
> shipped examples would have made two third-party requests. So the whole range set is
> served from our own origin too — `@mathjax/mathjax-newcm-font` is now a direct
> dependency, pinned to the same version — and `loader.paths` is redirected at load. The
> *second* CDN call MathJax 4 makes was found the same way: the speech-rule engine
> fetches its locale tables from jsdelivr, so `enableSpeech`, `enableBraille`,
> `enableEnrichment`, `enableExplorer` and `enableMenu` are all off. Correcting this here
> rather than up in the facts list, so the diff stays in one place; fold the real column
> in when this file is folded into the plan.
>
> **What it costs in `dist/`**, for item 6: **+11 817 943 B**, from 4 108 399 B to
> 15 926 619 B excluding source maps. That is 1 849 625 B of `tex-svg.js` (616 713 B
> gzipped, matching the measurement above) and 9 968 318 B of font ranges. Almost none of
> it is on the wire — the ranges are fetched one at a time and only when a formula needs
> one; all six shipped examples together pull 81 703 B of them. The app's own bundle is
> unchanged at 92 595 B and the precache manifest is unchanged at 21 entries / 1 581 KiB,
> which is the point of not bundling it. **Item 6 should weigh trimming the range set
> before trimming the bundle**, and the argument against was the reason all 40 shipped:
> the seven that are recognisably *mathematical* alphabets — `double-struck`,
> `calligraphic`, `script`, `fraktur`, `variants`, `marrows`, `mshapes` — come to 331 112
> B, and the remaining 9.6 MB is Cyrillic, Greek in text variants, Hebrew, Arabic,
> Devanagari, Cherokee, braille, phonetics and the extended Latin, sans and mono
> alphabets. Those are exactly what a `\text{…}` in a multilingual document needs, and
> the *Unicode passthrough* example is precisely about multilingual documents. Curating
> the set by guesswork buys a few megabytes of `dist/` and pays for it with a wrong glyph
> in somebody's formula, which is not a trade to make without measuring first. A range
> that is missing does not break the page: MathJax logs a warning and falls back.
>
> **Two boxes stay unticked.** *Lazy on the web, complete when installed*: the lazy half
> is done and the route is in place, but "on first selection of the MathJax mode" and
> "fetch it once on first run in the background" are app-side wiring, and so is the
> question of folding *keep all fonts offline* into one *keep everything offline*
> setting. `loadMathJax()` is idempotent and safe to call speculatively, which is the
> hook that half needs. *A new size budget line in the workflow* is
> `.github/workflows/web.yml`, which this change did not touch.

- [x] **Bundle it. No CDN, ever** — it would break both the offline promise and the
      privacy claim in About.
- [x] **Take the SVG output, `tex-svg`** (615 KB gzipped, one file, zero runtime font
      fetches) rather than CHTML (281 KB gzipped but 105 woff2 files, 1.8 MB, that an
      offline-first app would have to precache anyway). One asset is the whole offline
      story. Revisit with a custom `@mathjax/src` build — we know exactly which TeX
      extensions are needed and need neither MathML input nor the a11y tree — if the
      number has to come down.
- [x] **Lazy on the web, complete when installed.** Fetch the bundle on first
      selection of the MathJax mode, held by a `CacheFirst` runtime route beside the
      existing `techxt-fonts` one. An installed PWA should not have to think about it:
      fetch it once on first run in the background and keep it. Consider extending the
      existing *keep all fonts offline* checkbox into one "keep everything offline"
      setting rather than adding a second one.
- [x] Configure the TeX input with the package set the primitives need — `base`,
      `ams`, and whatever the shipped examples exercise — and **turn off MathJax's own
      `$…$` scanning**: it is handed one element per region and typesets exactly that.
- [ ] A new size budget line in `.github/workflows/web.yml` beside `WASM_MAX_BYTES`,
      with the same "these two values are the only authoritative copy" discipline.

## Size: not this item's problem

Bundling MathJax roughly doubles the app, and `web/PLAN.md` §4.7 has named
`opt-level = "s"` as the first response since W1. **Do not act on that here.** Size
budgets, optimisation flags and the browser-side speed measurement are one pass at the
end of the whole plan — item 6 — because tuning them item by item means tuning them
several times against a target that keeps moving. Ignore `WASM_MAX_BYTES` and
`WASM_MAX_GZIP_BYTES` while implementing; a red size step is expected and is item 6's
to clear.

What this item *does* owe item 6: pick the MathJax build deliberately (SVG, per above),
keep it lazily fetched rather than in the main bundle, and write down what it actually
costs in `dist/` so item 6 has a number to work with.

## Display and behaviour

- [x] Inline math: MathJax handles line-breaking within an inline formula; let it.
- [x] Display math: give each formula its own horizontally scrolling box, so a wide
      formula scrolls by itself instead of forcing the whole pane sideways. This
      matters most under Soft wrap, which is now the default.
- [x] Fit-to-pane column measurement is meaningless across a typeset formula. Under
      MathJax mode this is a known and accepted imprecision; do not try to correct it.
- [x] `mathExpressionIn`, `matrixDelimiters` and `mathFont` do nothing in this mode
      (they are rendering options that Source mode bypasses). Disable them in the
      *More options* → *Math* fieldset while MathJax is selected, with a one-line
      explanation, rather than leaving inert controls.
- [x] Copy and Download hand over the source-mode text, `$…$` included. Say so in the
      control's hint: this is the one mode where what you see and what you copy differ.
- [x] Typesetting is async and can be slow on a large document. Do not block the pane:
      set the text first (it is readable immediately), then typeset, and drop a
      typeset pass whose conversion has already been superseded.

*Done when*: the sentence from fact 2 renders with exactly one formula and two literal
dollar signs; a document using `\newcommand` in math typesets without MathJax knowing
the macro; Copy still returns the library's text byte for byte; and a cold reload with
the network off, after MathJax has been used once, still typesets.

---

# 3. The library — an automatic log of what you converted

> **Done** — 2026-08-28, `29a65dd`, with item 4. `src/library.ts` (the entry
> model, the session and the retention policy), `src/library-store.ts` (IndexedDB and
> the quota facts), `src/ui/library-pane.ts` (the sheet and its dialogs);
> `web/PLAN.md` §6.10 and §6.11 are new, and §1, §2 (D8, D9), §3, §6.1, §6.4, §6.8,
> §13 and §16 were edited to match. 92 vitest cases were added over the pure halves.
>
> **Four things diverged from what is written below, and the text above them now says
> so:**
>
> 1. **A reload had to be added to the list of things that do *not* start a new
>    entry.** It was not on either list here, and the first browser run logged a
>    second copy of the document on every reload — which is exactly the pile of copies
>    this item exists to avoid. The id of the current entry is now kept in
>    `localStorage` under `techxt.library.current.v1` and adopted on load when the
>    document came from storage.
> 2. **The `starred` index is a derived `star` of 0/1.** IndexedDB cannot key on a
>    boolean — a record whose indexed value is one is simply left out of the index —
>    so `library-store.ts` writes the flag twice and strips the derived half on the
>    way out. Nothing above that file ever sees it, and an export never carries it.
> 3. **Toasts had to learn to follow a modal.** Delete's Undo lives in a toast and
>    Delete happens inside the sheet; `showModal()` makes everything outside the
>    dialog inert, so the Undo was drawn under the sheet and would not answer a click.
>    The top layer is no escape — a popover over a modal is painted above it and is
>    still inert, which was measured — so `ui/toast.ts` now moves the toast mount into
>    the open dialog and brings it home when the dialog closes.
> 4. **`downloadName`'s regex became `src/title.ts`**, shared with the entry title
>    rather than copied. A derived title also follows the document until the user
>    renames the entry and then stops; `titleIsAutomatic()` tells the two apart by
>    asking whether the stored title is still the one the stored source would produce,
>    which costs no extra field and nothing in the export format.
>
> One thing was left for later, deliberately: the pane loads every entry in full to
> render the list, because the text search reads the source. That is fine for the
> libraries a person accumulates and would not be for tens of thousands of entries —
> see the new checkbox at the foot of this item.

**The design changed in discussion: saving is automatic.** The library is a historical
log of the documents the user has run through the app, like a browser's history. The
button beside Copy and Download is not *Save* but **⭐ Save** meaning *star this* —
starring marks an entry worth keeping, and the library can filter to starred entries.
This is both more useful (nothing is ever lost because someone forgot to press a
button) and simpler to explain.

## The entry model, decided

- An entry holds: `id`, `createdAt`, `updatedAt`, `title`, `source`, `options` (the
  full `AppOptions` in force, pruned as everywhere else), `starred`, and a small
  `preview` of the rendered output.
- **One "current entry" per editing session.** It is created on the first conversion
  of a non-empty document and *updated in place* (debounced, ~2 s, and on `pagehide`)
  as the user keeps typing, so the log does not grow per keystroke. Changing only the
  options updates the current entry too, rather than making a new one.
- **A new entry begins** when the document is replaced wholesale — Load ▾ example, the
  `.tex` file handler, a share link, opening an item from the library, an import — or
  after a long idle gap (30 minutes is a reasonable first value). These are all points
  the app already knows about.
- **⭐ Save stars the current entry**, creating one first if there is not one yet, so
  pressing it always produces something visible. Un-starring is the same button.
- **Title**: reuse `downloadName(state.doc)`'s logic — the first `\title` or
  `\section` — falling back to the first non-empty line, then to the date. The user can
  rename an entry from the library. Do not prompt on save; the app's idiom is silent
  plus a toast (that is what Load ▾ does), and a prompt on an *automatic* save would be
  absurd.
- **Preview**: store a small one — the first ~400 characters or six lines of rendered
  output, whichever is shorter. Enough for a legible card, cheap to store, cheap to
  export, and honestly stale-able. The full rendering is always regenerable from
  `source` + `options`.
- **Opening an entry** restores its document *and* its options. That is not
  destructive: the settings being replaced belong to the current entry, which is
  itself in the log and one click away. Offer the usual single-level undo in the toast
  anyway.

## Storage

- [x] **IndexedDB**, one database, one object store keyed by `id`, with indices on
      `updatedAt` and `starred`. `localStorage` keeps the session state as it does
      today; the two are separate and neither can exhaust the other.
- [x] Call `navigator.storage.persist()` the first time an entry is written, so the
      browser stops treating the library as evictable. An installed PWA usually gets
      this for free; asking costs nothing.

### Retention: the app never quietly drops the user's data

**This is the rule the rest of the storage design serves, and it is not negotiable.**

- **Nothing is ever deleted for tidiness.** Not because an entry is old, not because
  it is "expired", not because there are a lot of them. An entry that bothers nobody
  stays. There is no scheduled prune, no age cutoff, no silent cap — the user deletes
  what they want gone, and *only* the user.
- **Starred entries are never removed by any automatic mechanism, under any
  circumstance.** Full stop.
- **If space genuinely runs out**, and only then, the app may propose removing the
  oldest unstarred entries — as a *proposal*, never as an action:
  1. Say plainly what is happening: storage for this site is full, the library cannot
     grow, here is how much it is using.
  2. Offer **Export library** first and prominently, so nothing has to be lost at all.
  3. Only after the user has had that chance, and has explicitly agreed in that same
     dialog, remove the entries they agreed to — showing exactly which ones, with a
     count and the date range.
  4. If the user declines, stop logging new entries and say so in the status line.
     A library that has stopped growing is a nuisance; a library that ate the user's
     work is a betrayal. Take the nuisance.
- **Warn early enough that this dialog is rare.** `navigator.storage.estimate()` at,
  say, 80 % of quota earns one unobtrusive note in the library header naming Export
  as the remedy — not a modal, not repeated every session.
- **A failed write is loud.** If IndexedDB refuses a write, say so in a toast with an
  **Export library** action rather than dropping the entry and moving on.
- Show the user where they stand without being asked: a count and the storage figure
  in the library header ("142 entries · 8 starred · 3.1 MB").

### The rest of the storage story

- [x] Cap a single entry's `source` at the same 512 KB `MAX_STORED_DOC` uses, and tell
      the user an entry was too large to log — never log it truncated, and never let
      one huge paste be the reason something else gets dropped.
- [x] **Private browsing.** IndexedDB may be absent, or present and ephemeral. Do not
      change the app's behaviour: offer the library as usual, and if it is easy to
      detect (`navigator.storage.estimate()`, a failed `persist()`), show a small
      ⚠️ note in the library header saying this browsing session will probably not keep
      these and pointing at Export. If IndexedDB throws outright, degrade the way
      `browserStorage()` already does for `localStorage`: an inert, honest "not
      available here" state, never a broken button.
- [x] **A local file for more space** is Chromium-only (`showSaveFilePicker` and a
      persisted handle) and unavailable on iOS, so it is *not* the answer for the base
      feature. Export (item 4) is. Revisit a File System Access backend as a
      desktop-only convenience once the rest works, if the quota warning turns out to
      fire in practice.

## The pane

- [x] **A `<dialog>` sheet**, like About and Install, using `src/ui/sheets.ts`. A
      scrolling list of entries belongs inside a dialog in an app whose page never
      scrolls (§6.8, D8), and the sheet machinery already gives Escape, the backdrop,
      focus handling and inertness for free.
- [x] **Open it from the header**, beside About and Install, where the app's other
      sheets live. Also offer it from the primary options row next to *More options*,
      since that is where it was originally asked for — one action, two doors, and no
      third row on a phone.
- [x] **Desktop**: list on the left, selected entry on the right — title, date,
      options summary, the preview, and the actions. **Phone**: list, tapping an entry
      pushes to its detail with a back control. Same data, one column.
- [x] Per entry: open, star/unstar, rename, delete, and copy/download its source.
- [x] Filters: **all / starred**, and a text search over title and source. Sort by most
      recently updated.
- [x] **Delete** removes one entry, with an Undo in the toast (the app's existing
      single-level-undo idiom).
- [x] **Clear library** with a real confirmation — a typed confirmation or a two-step
      dialog naming the count, not a bare "are you sure". Make it hard to lose data
      by accident. Starred entries are counted separately in the confirmation.
- [x] **Discoverability.** The library only helps if people know it is there:
      - The first time an entry is auto-logged, a one-time toast: *"Saved to your
        library"* with an **Open library** action.
      - A subtle pulse on the library button for the first ~3 sessions, driven by a
        counter in `localStorage`, then never again.
- [x] **About** gains a sentence: the library is stored on this device only and is
      never uploaded, alongside the existing privacy line.

- [ ] **If a library ever gets slow to open**, stop loading every entry in full to
      render the list: list from the `updatedAt` index without `source`, and fetch the
      source when an entry is selected or when a search actually needs it. Deliberately
      not done — the text search reads the source, so this is a real trade rather than
      a free win, and nothing a person plausibly accumulates is slow today. Left here
      as a box rather than in someone's head, per the rules at the foot of this file.

*Done when*: converting a document and reloading finds it in the library; starring
survives a prune; deleting one entry is undoable; the pane is usable one-handed on a
390 px screen; and a browser with IndexedDB blocked still shows a working app.

*Observed*, in Chromium on 2026-08-28: converting and reloading finds **one** entry,
not two; a full disk (every write failing with `QuotaExceededError`) proposes the 3
oldest unstarred of 4, offers Export first, and leaves the starred one alone whichever
way the dialog is answered; declining stops the log and says so in the status line;
Delete's Undo works from inside the sheet; the pane pushes to a detail at 390 px with
44 px targets and no sideways scroll; and a profile with `indexedDB` throwing on
access converts normally with the ⭐ button hidden and an honest pane.

---

# 4. Library import and export

> **Done** — 2026-08-28, `29a65dd`, with item 3. `src/library-io.ts` is the whole
> codec: the format, `decodeLibrary()` and `planImport()`. The rule that an import
> never removes an existing entry unless the user chose Replace is a property of
> `planImport` — outside `mode: 'replace'` its `remove` list is empty by construction
> — and `test/library-io.test.ts` holds it to that from five directions, including the
> cases where every incoming id collides and where the file is the library itself.
> Timestamps are written as ISO 8601 strings rather than epoch numbers, so the file is
> readable by a person who opens it in an editor; the reader accepts both.

- [x] **Export**: the whole library as one JSON file, downloaded through the same
      `Blob` path the output's Download button uses. Named
      `techxt-library-YYYY-MM-DD.json`.
- [x] **Format**, versioned, and boring on purpose:
      ```json
      {
        "format": "techxt.library",
        "v": 1,
        "exportedAt": "2026-08-28T12:00:00Z",
        "app": "techxt-web",
        "techxt": "0.1.0",
        "items": [ { "id": …, "createdAt": …, "updatedAt": …, "title": …,
                     "source": …, "options": { … }, "starred": …, "preview": … } ]
      }
      ```
- [x] **Include the preview** so an imported library is legible before anything is
      re-converted. This is the strongest argument for keeping the preview genuinely
      small — a few lines, not the whole rendering.
- [x] **Import offers explicit options** in a dialog, so nothing about the result is a
      surprise:
      - **Add to my library** (the default) — everything already there is kept; an
        id collision gets a fresh id rather than overwriting.
      - **Skip items I already have** (a checkbox on the above) — matched by a hash of
        `source` + `options`, not by id.
      - **Replace my library** — behind its own confirmation naming what will be lost,
        including the starred count.
- [x] **Existing entries are never removed unless the user explicitly chose Replace on
      that particular import.** No heuristic, no "clean up duplicates", no exception.
- [x] Report the outcome: *"12 added, 3 skipped, 0 replaced."*
- [x] **Treat an import as hostile input**, with the discipline `decodeShare()` already
      uses: every field through a validator, unknown fields dropped, unknown option
      values dropped (`sanitizeOptions` is already exactly this function), size caps,
      and a read that never throws. A refusal names what was wrong with the file.
- [x] Unit-test the codec the way the share codec is tested: round-trip, truncation, a
      foreign file, a future `v`, an item with a bad option value.

*Done when*: a library exported from one profile imports into another with previews
intact, and every import path has been shown not to remove an existing entry.

*Observed*, in Chromium on 2026-08-28: an export of a starred entry re-imported under
Add landed a second copy with its preview and star intact and the original untouched;
the same file under Add + *skip what I have* landed nothing and removed nothing;
Replace named "2 entries, 2 of them starred", demanded the word typed, and only then
removed them; and a foreign JSON file was refused with *"That file is not a techxt
library export."* while the library stayed exactly as it was.

---

# 5. A lighter editor: highlighting and completion

> **Done** — 2026-08-28, `1dc8e60`. The app half: the worker answers `complete`, the
> source pane is highlighted by a lexer painting the mirror it already had, and a row of
> chips under the input completes a name on Tab. `src/highlight.ts` and
> `src/completion.ts` are the pure halves (44 vitest cases between them), `ui/panes.ts`
> the elements and the keyboard, `convert-client.ts` a `complete()` on a counter of its
> own. `web/PLAN.md` §1 and §16 are rewritten as this item requires, §6.12 and §6.13 are
> new and normative, and §4.9, §6.2, §6.9, §13 and §14 are amended. **`dist/` cost:
> +9 105 B of `index.js` and +2 317 B of CSS raw, +3 257 B and +476 B gzipped — no
> dependency, and nothing at all to the wasm module.**
>
> **Observed in Chromium** (headless, `npm run preview`, Playwright installed outside the
> repo), at 1280×820 and at 390 px with touch emulation — the *Done when* line and then
> some: `\alp` offers `\alpha  α` then `\alph`; Tab applies the first and the next Tab
> the second; Shift-Tab steps back and, from the first candidate, restores `\alp`; seven
> Tabs in a row walk `\alp → \alpha → \alph → \alp → …`, so both ends of the ring come
> back to the user's own text; Enter inserts a newline with the row showing; a
> `\newcommand{\ketstate}` above is offered, flagged and tinted as the document's own;
> `\beg` offers `\begin` and `\begin{ali` offers five environments; the row is one line
> of 37 px chips at 390 px and a chip applies by tap without closing the keyboard; Tab
> with no row moves focus to the pane divider. Screenshots in light and dark, and the
> mirror checked character-for-character against the textarea's value.
>
> **Five things reality settled.**
>
> 1. **The window had to be small, and the measurement is what says so.** A mirror
>    rebuild is 4.5 ms for the text plus **5.3 µs per span**, and dense LaTeX carries
>    ~120 spans per KB — so spanning a whole 20 KB document costs **+17 ms on every
>    keystroke**, which a typist feels. Documents up to 6 000 characters are highlighted
>    whole and larger ones only within the screenful in view plus 3 000 characters each
>    side; that brings 20 KB to +4.0 ms and 200 KB to +0.6 ms. The window is *estimated*
>    from the scroll offset rather than measured (measuring means a forced layout inside
>    the keystroke), and the estimate was checked against the truth — a `Range` binary
>    search over the mirror — on a deliberately uneven 200 KB document: wrong by at most
>    **1 033 characters**, which is where the margin comes from.
> 2. **Head-of-line blocking is real, and only on a large document.** Keystroke to chips
>    on screen: **5.1 ms** on a 2 KB document inside a typing burst and 5.7 ms just behind
>    a conversion; **47.8 ms** and **249.2 ms** on a 200 KB one. The debounce is what
>    saves the ordinary case — a burst of typing leaves the worker idle. The cache this
>    item held in reserve is in, and **scoped to one name**: it is emptied the moment the
>    caret moves to a different `\`, which makes it impossible for it to answer from a
>    table that predates a definition the document has since gained, and it makes a
>    backspace free. What it cannot do is make a *new* prefix faster — a letter nobody has
>    typed yet is a question nobody has asked yet — so it is a mitigation and the 249 ms
>    stands as the number a second wasm instance would have to be worth. It is not.
> 3. **The environment trigger needs a filter, because `complete()` takes no kind.** It
>    ranks macros above environments, so an answer capped at five would rarely reach one.
>    The app asks for 250 (a cap, not a count) and renders the `kind: 'environment'`
>    entries in the order they came back. That is a filter and never a re-sort, and it is
>    named as an exception in PLAN §4.9 and §6.13 rather than left to be discovered. If
>    the binding ever grows a kind argument, that filter is the code to delete. **Only
>    `\begin{` triggers it**, as this item specifies; `\end{` was left alone rather than
>    quietly added.
> 4. **Inserting a completion had to go through `execCommand('insertText')`.**
>    `setRangeText` and an assignment to `value` both drop the browser's undo stack, so
>    the first Tab would have silently cost the user every keystroke they had typed
>    before it — observed, before the fix, as a Ctrl+Z that did nothing at all. The
>    deprecated call is the only one that edits a textarea and leaves Ctrl+Z working;
>    `setRangeText` is the fallback where it is refused, and Shift-Tab is then the only
>    undo there is.
> 5. **Highlighting ships on touch as well.** The escape hatch this item allowed was not
>    needed: the mirror has been carrying the diagnostic underline on phones since W4, so
>    only the transparent glyphs, the translucent selection and the IME fallback were new,
>    and all three behave at 390 px under Chromium's touch emulation. A real device and a
>    real IME are still unverified — one flag in `ui/panes.ts` turns the colours off
>    together with the lexing if a device disagrees.
>
> **What surprised, and it is not this item's doing:** a keystroke in a 200 KB document
> already cost **83 ms** before any of this, on the mirror the diagnostics have been
> painting since W4 — `setRangeText`, a forced layout to read `scrollTop`, and the whole
> text rebuilt into the mirror every time. Highlighting adds 0.6 ms to that. Nobody had
> measured it because the conversion, the thing everyone expected to be slow, has a worker
> and a debounce in front of it while the keystroke does not. It is now **item 7** below.
>
> **Regression, and the fix — 2026-08-28.** Reported from a real paper: the highlighting
> came away from the code wherever a line wrapped, and the pane could not be edited. Both
> were one geometry mistake with two faces. The two layers share a *border* box
> (`position: absolute; inset: 0`) and wrap at their *content* box, and the mirror carried
> `scrollbar-width: none` while the textarea showed a real scrollbar — so on any document
> tall enough to scroll, the textarea wrapped **15 px narrower** than the mirror. Measured
> in Chromium on a 27 KB LaTeX paper of the reported shape: content widths 650 px against
> 665 px, wrapped heights 11 514 px against 11 254 px. That produced the drift *and* the
> "cannot edit": clicking a glyph put the caret **333 to 688 characters** away from it,
> growing with depth, so what you typed appeared somewhere else entirely; and the mirror,
> being shorter, clamped 260 px before the textarea's own scroll bottom, so the end of the
> document could not be brought on screen at all. No exception was thrown, no keystroke
> was swallowed, the console stayed clean and `is-composing` never latched — every one of
> those was reproduced and ruled out before the geometry was believed. The fix is
> `scrollbar-gutter: stable` on the rule *both* layers read, plus a mirror that keeps a
> real scrollbar painted in nothing rather than removing one, since removing it takes the
> reserved gutter with it. Afterwards: widths and heights equal, click-to-caret exact,
> a mirror-against-textarea pixel diff down from 5–8.6 % of the pane to 0.24–0.55 %
> (residual sub-pixel antialiasing at span boundaries, no glyph displaced).
>
> A second, older bug came out of checking that the gutter markers still landed on their
> spans: they did not, and had not since W4. The throwaway mirror `ui/panes.ts` measures
> caret offsets in took its width from the resolved `width`, which under this app's
> `box-sizing: border-box` is the *border* box and knows nothing about a scrollbar — so it
> was 43.8 px too wide, and markers sat **107 to 687 px** above their own underline while
> the diagnostics' jump-to-source scrolled a span 944 px outside a 615 px pane. It now
> takes `clientWidth` minus the padding. Markers are within 0.8–8.0 px of a 21.7 px row
> (the residue is sub-pixel line-box rounding between a `<textarea>` and a `<div>`), and
> the jump lands its span 293 px into the pane. `web/test/editor-mirror.test.ts` guards
> the stylesheet's shape — the pixels need a browser, and PLAN §6.12 now states the
> invariant they are held against.

**This reverses `web/PLAN.md` §1 and §16**, which currently name syntax highlighting
and a code editor component as non-goals ("a textarea is honest and fast, and
CodeMirror would outweigh the engine"). The reversal is deliberate and must be written
into those sections — *and the reasons they gave stay true and become the constraints*:
whatever ships must not outweigh the engine, and must not make typing slower.

## Approach, decided

- [x] **Survey CodeMirror 6 and friends first**, then **hand-roll**, which is the
      expected outcome. Record the survey's conclusion in `PLAN.md` §16's replacement
      so the decision is not re-taken from scratch later.
- [x] **Keep the `<textarea>`.** Highlighting is an overlay: a `<pre>` mirror behind a
      transparent-text textarea. `src/ui/panes.ts` **already maintains a hidden mirror
      element** for positioning diagnostic gutter markers, so half the machinery and
      all of the metric-agreement discipline is there to build on.
- [x] **`contenteditable` is out.** It breaks `setSelectionRange`, which
      `Panes.selectSpan` — the diagnostics' jump-to-source — depends on.
- [x] Watch the known overlay failure modes, all of which get worse on a phone with the
      keyboard up (a stated §6.6 priority): IME composition, scroll synchronisation,
      exact font-metric agreement between mirror and textarea, and mobile autocorrect.
      If the overlay cannot be made to behave on a touch device, ship highlighting on
      pointer devices only rather than shipping something that fights the keyboard.

## Highlighting

- [x] Minimal and structural: commands, math delimiters and their contents, comments,
      braces, and environment `\begin`/`\end` pairs. Not a LaTeX grammar — a lexer.
- [x] It must survive a 200 KB document without making a keystroke feel slow. Highlight
      the visible region plus a margin, not the whole buffer, if measurement demands.
- [x] It shares the pane with the existing diagnostic underline and gutter markers;
      they must not fight over the same visual channel.

**Considered and declined: highlighting from the parser.** The binding could expose
techy's own token spans and get highlighting that is exactly as accurate as the parse,
riding along on the conversion response for free. It is declined because of *when* the
answer arrives: conversions are debounced 120 ms and the round trip is through the
worker, so the colours would trail the cursor by a visible fraction of a second on
every keystroke. A dumb synchronous lexer on the main thread repaints with the
character. Should the overlay later want something only a parse knows — which
`\begin` an `\end` closes, say — enriching it from the conversion result is additive
and the door stays open.

## Completion

> **The binding half is done** — 2026-08-28, `406d717`. `Session::complete(latex,
> prefix, limit)` answers with the `Completion` array `protocol.ts` declares, merged
> from both sources and ranked in Rust. `web/crate/src/complete.rs` is the whole of it,
> with `tests/completion.rs` natively and `tests/wasm_completion.rs` for the wire
> spelling; `web/PLAN.md` §4.1 grows a fourth export and a new §4.9 records the design.
> It cost **33 933 B raw / 15 627 B gzipped** in the module, measured on this container
> against the same build without it — mostly L2's machinery, which until something
> called `symbols()` was dropped by the linker. What is left of this item is the app:
> the protocol messages, the chip row, and the highlighting above.
>
> **"Microseconds either way" is right for a document and wrong for the first call.**
> Driving the release module from Node: the first `complete` costs 7.4 ms, because that
> is where the table is built, and every call after it costs 0.03 ms on an ordinary
> document and 1.1 ms on a 197 KB one, the linear scan being the whole of the
> difference. Nothing here needs the cache the bullet below holds in reserve; what the
> figures do say is that the head-of-line-blocking measurement still to be done should
> budget for a first request that is slower than the rest.
>
> **Five things reality had to settle**, all of them also written into §4.9:
>
> 1. **The index could not be kept as a `SymbolIndex`.** It borrows the `DefinitionSet`
>    it was read from, so a `Session` holding both is a self-referencing struct, and
>    there is no honest way to write one. The binding keeps an owned copy of the 1 406
>    resolved entries instead — a few tens of kilobytes, built once, lazily, and still
>    sorted by `(kind, name)`, so a prefix query is two binary searches and a subslice
>    exactly as the bullet below intends. Only the type changed.
> 2. **A definition the document makes *replaces* the shipped one of the same name**
>    rather than ranking above it. It is the definition that will actually fire, and two
>    chips both reading `\ket` would have been a worse answer than one. `\ket` is not a
>    hypothetical: techxt ships one, so the TODO's own example is a shadowing case.
> 3. **Comments are filtered out of the scan; a `verbatim` body is not.** `%` is one
>    unambiguous character and the scan already walks escape sequences, so a
>    commented-out `\newcommand` costs nothing to ignore — and `\%` is a control symbol,
>    so a macro body that prints a percent sign is still found. A `verbatim` body would
>    mean tracking `\begin`/`\end`, `\verb` with its arbitrary delimiter and every
>    listing package there is, which is the parse this item declined; a definer inside
>    one is offered, and there is a test that says so on purpose.
> 4. **`\alp` offers `\alph` before `\alpha`.** The *Done when* line below is worth
>    reading with this in mind. `\alph` is a real macro and a shorter completion of the
>    same prefix, so the shortest-first rule puts it first and Tab takes it; `\alpha` is
>    the second chip. Ranking `\alpha` first would mean nobody can ever complete
>    `\alph`, and nothing available here measures which of the two is wanted more often.
>    The line is left standing rather than quietly rewritten, because the example is a
>    good one and what it runs into is worth knowing.
> 5. **What the scan does not claim to know.** A document's own definition has
>    `replacement: null` — the scan recognizes a definition, it does not evaluate one —
>    and the definer list is exactly the six named below, so `\renewenvironment` and
>    `\let` are not scanned. `SymbolEntry`'s mode restriction is dropped rather than put
>    on the wire, because the app cannot know which mode the cursor is in without a
>    parse.

> **The ranking is curated now, and fact 4 above is no longer what the code does** —
> 2026-08-28, `93780b8`. `\alp` offers `\alpha` first. The reasoning in fact 4 is left standing
> because it is still true, and because it is the reason this fix has the shape it has:
> nothing available here measures which of `\alph` and `\alpha` is wanted more often, so
> the answer is not measured but *written down*. `web/crate/src/complete.rs` grows a
> hand-written list of **99 macros** — the Greek alphabet with its capitals and variants,
> about forty mathematical symbols and constructs, twenty everyday text and structure
> macros — and ranks them, in the list's own order, above everything else that shares
> their prefix.
>
> **The objection fact 4 raised is answered by the rule above the list, not by
> argument.** An exact match now outranks everything, so `\alph` typed in full still
> offers `\alph` first and stays completable; without that rule the curated list would
> have moved the bug rather than fixed it. The two cases are pinned by
> `a_curated_name_outranks_a_shorter_neighbour` and
> `a_prefix_typed_in_full_still_leads_even_against_a_curated_name`.
>
> **The list is a ranking overlay and never a source of entries.** Every suggestion still
> comes out of the `SymbolIndex` or the document scan, so a curated name techxt does not
> define is offered by nobody and noticed by nobody — a silent failure, and therefore the
> one thing a hand-written list must be tested for:
> `every_curated_name_is_a_macro_the_library_defines` resolves all 99 against the shipped
> definitions, as macros, so a typo in the list is a red test.
>
> **`\begin` and `\end` could not be curated, because techxt does not define them.** They
> are the head any list of everyday macros would start with, and they are structure the
> parser handles itself rather than entries in a `DefinitionSet`: `\begi` completes to
> nothing at all, and no ranking can change that. Checked for the same reason,
> `equation` and `align` *are* defined — as **environments**, not macros — and they are
> not on the list either, because the row fires on an escape character and an environment
> name is not typed after one. Both findings are pinned by
> `begin_and_end_are_not_names_the_library_defines`, which goes red the day either
> becomes completable and tells whoever is holding it to put them at the head of the
> list.
>
> **Where the list lives is the decision behind it.** "What people type most" is a fact
> about a completion UI and not about LaTeX — it changes with the audience, nothing in
> the library could test it, and it comes nowhere near the bar this file sets for a
> change to `rust/techxt` (L2 cleared that bar because a data structure you can only
> write to is half a data structure; `\alpha` being popular is not that). So the list
> sits in the binding, next to the ranking it feeds.
>
> **What it cost the module**, for item 6's ledger — release build, this container, the
> same toolchain and flags at both ends: **+4 290 B raw / +1 628 B gzipped**
> (1 236 973 → 1 241 263 raw, 438 459 → 440 087 gzipped). Measured a third way, against
> the same code with the list emptied to `[]`, the hundred names themselves are **1 298 B
> raw / 545 B gzipped** and the remaining ~3 KB is the ranking machinery — the extra sort
> key and the walk that places a name in the list. Nothing here is a dependency and
> nothing reaches `dist/` beyond the module itself.

**Where the suggestions come from.**

- [x] **techxt's own declared symbols**, through the wasm module (below).
- [x] **The user's own definitions, the cheap way — and in Rust, not JS.** Scan the
      document for `\newcommand`, `\renewcommand`, `\providecommand`, `\def`,
      `\DeclareMathOperator` and `\newenvironment` and take the names. A linear scan,
      ~30 lines, no library change. Doing it **inside the binding** rather than in the
      app is the simplification: `complete()` then returns one already-merged, already-
      ranked list and the JS side has no second source to reconcile, no second matcher
      to keep in step with the first, and nothing to test twice. Mark these entries
      with a flag (`fromDocument: true`) so the chips can show where they came from.
      The exact route — having the binding call `language.parse()` itself and read
      techy's final parsing state — is explicitly **not** being taken: `Conversion`
      exposes only `text` and `diagnostics`, and the work is out of proportion to the
      difference.

**L2 — the library change: reading a `DefinitionSet` back.**

> **Done** — 2026-08-28, `324b6fd`. `Category::macros/environments/specials` and
> `DefinitionSet::symbols() -> SymbolIndex<'_>`, with `SymbolEntry` as sketched below.
> Three things the sketch left open: `modes` is a `ModeVisibility { Anywhere, TextOnly,
> MathOnly }`, the index carries the borrow it obviously must (`SymbolIndex<'a>`), and it
> has the query API a completion list needs — `entries`, `len`, `is_empty`, `of_kind`,
> `get(kind, name)`, `starts_with(kind, prefix)`. The table is sorted by `(kind, name)`,
> so entries of one kind are contiguous, `of_kind` and `starts_with` are subslices found
> by binary search rather than filters, and a caller answering a keystroke rebuilds
> nothing — `defs::standard()` resolves to **1 406** names (1 311 macros, 83
> environments, 12 specials). Root `PLAN.md` gains §10.7; tests in
> `rust/techxt/tests/def_symbols.rs`. Nothing here touches `web/`, and nothing reaches
> `dist/`: the module is reachable only through `symbols()`, so a binding that never
> calls it drops the whole thing.
>
> **Verified fact 6 is now stale, in both halves.** Its second half — that `Category`
> exposes no way to read the macros back — is what this change removes. Its first half
> undercounts: the shipped library declares **1 757** macros (991 in `symbols_extra`, 766
> across the curated categories), resolving after shadowing to **1 311** distinct names,
> not ~1 100. Corrected here rather than up there, so that this diff stays in one place;
> fold the real numbers in when this file is folded into the plans.
>
> **What the sketch could not have known.** "Later categories win, so the index resolves
> each name once" is true of the *set*, but techy's resolution is also **mode-aware**: an
> entry hidden in the current mode is skipped and an outer definition of the same name
> answers instead. So a name whose innermost definition is mode-restricted over an
> unrestricted one really does resolve to two different definitions in the two modes, and
> one index entry cannot say so. The situation is common in shape — **223** shipped macro
> names are declared in two categories with *different* mode restrictions (`\Delta` is
> math-only in the generated `symbols_extra`, unrestricted in `mathcore`) — and harmless
> in direction, because the restricted layer is always the outermost one. That was
> measured rather than assumed, and it is now pinned by
> `the_shipped_library_never_shadows_a_name_with_a_narrower_one`, which fails if a future
> category ever shadows a name with a narrower one. If that day comes the index needs a
> second answer for such a name, not a vaguer one: whoever offers `\Delta` in a chip row
> is entitled to know it will fire.

*What.* `Category` can be built but not read: it has `add_macro`/`with_macro` and
friends, and no accessor at all. Add the reading half, then a shadowing-aware index
over a whole set.

*Why the library wants it anyway.* A data structure you can only write to is half a
data structure. "What does this converter know?" is a question any embedder building a
user interface has to be able to ask, `techxt-cli --list-symbols` is an obvious small
feature that needs exactly this, and the crate documentation's extension examples read
better when a set can be inspected.

*Shape.*

```rust
// techxt::def
impl Category {
    pub fn macros(&self) -> impl Iterator<Item = &MacroDef>;
    pub fn environments(&self) -> impl Iterator<Item = &EnvDef>;
    pub fn specials(&self) -> impl Iterator<Item = &SpecialsDef>;
}

/// Every name a definition set defines, later categories shadowing earlier ones.
pub struct SymbolIndex { … }

pub struct SymbolEntry<'a> {
    pub name: &'a str,
    pub kind: CallableKind,          // already exists: Macro | Environment | Specials
    pub category: &'a str,
    /// What it renders as, when the rule is a plain literal (`\alpha` → `α`).
    pub replacement: Option<&'a str>,
    pub arity: usize,
    pub modes: …,                    // text-only / math-only / both
}

impl DefinitionSet { pub fn symbols(&self) -> SymbolIndex; }
```

The shadowing rule is the set's own and already documented: later categories win, so
the index resolves each name once. `MacroDef` already has `name()`; `EnvDef` has
`name()`; `SpecialsDef` has `trigger()`; the rest is reading fields that exist.
Root `PLAN.md` entry and tests as usual.

**The app's architecture for completion.**

- [x] **The index lives in wasm and stays there.** ~1 100 macros with their
      replacements is a table the JS side has no reason to hold a second copy of.
      `Session` builds a sorted `SymbolIndex` lazily on the first completion request
      and keeps it.
- [x] Export `Session.complete(latex, prefix, limit)` returning a small array of
      `{ name, kind, replacement, arity, fromDocument }` — a binary search plus a
      prefix scan over the index, plus the definer scan over `latex`, merged and
      ranked. Microseconds either way, and the payload is a handful of entries.
      The document is passed in rather than remembered so the call stays stateless;
      if the scan ever shows up in a profile, cache it against the text's length and
      hash inside `Session` and leave the signature alone.
- [x] `src/worker/protocol.ts` grows
      `{ type: 'complete', id, text, prefix, limit }` and
      `{ type: 'completions', id, items }`, with the same monotonic-id discipline
      conversions use: a stale answer is dropped.
- [x] **The one risk is head-of-line blocking** behind a slow conversion in the same
      worker. Conversions are debounced 120 ms and take 2–20 ms on ordinary documents,
      so there is normally a gap. Measure it on a 200 KB document. If it is laggy, add
      a small JS-side prefix→results cache before reaching for a second worker; a
      second wasm instance is a megabyte of memory for a nicety.
- [x] **`\begin` and `\end` need app-side help, and so do environment names.**
      Building the curated list turned up that techxt defines neither `\begin` nor
      `\end` — they are structure the parser handles itself, not entries in a
      `DefinitionSet` — so `\begi` completes to nothing at all, which is arguably the
      most useful completion in LaTeX missing. Two parts, both app-side:
      offer `\begin` and `\end` as literals the app knows about rather than names it
      looked up; and once the user is inside `\begin{`, complete **environment
      names** — `SymbolIndex` carries them, `complete()` already returns
      `kind: 'environment'`, and nothing but the trigger is missing. That trigger is
      the reason this was not in the original design: the chip row fires on `\` plus
      a letter, and this one fires on `\begin{` plus a letter. Treat it as a second
      trigger feeding the same row, not as a second mechanism.
- [x] **The JS side does no matching, no merging and no ranking.** It sends a prefix
      and renders what comes back. Every rule about what is offered and in what order
      lives in one place, in Rust, next to the table it is drawn from.

**The completion UI, decided.**

- [x] **A row of chips below the input**, not a popup. It works identically on desktop
      and on a phone, it never covers what you are typing, and it degrades to nothing
      when there is nothing to suggest.
- [x] Trigger only after `\` plus at least one letter. Never on `\` alone, never in the
      middle of a word.
- [x] **Tab applies the first candidate; the Tab after it applies the second.** Tab is a
      cycle, not an accept: each press replaces the text the previous press inserted with
      the next candidate, walking the row from left to right. **Shift-Tab steps back**
      through it. Tapping or clicking a chip applies that chip directly, and ends the
      cycle there.
- [x] **The candidate list freezes when the cycle starts**, keyed to the prefix the user
      typed rather than to what is in the buffer now. This is not an optimisation, it is
      what makes the cycle exist: re-filtering on the newly inserted text would collapse
      the list to the one entry just inserted, and the second Tab would have nowhere to
      go.
- [x] **Both ends of the cycle come back to the user's own text.** Shift-Tab from the
      first candidate restores exactly what was typed — that is the undo half, and it is
      how a person who Tabbed by accident gets their `\alp` back — and Tab past the last
      candidate wraps to it as well. Whichever direction you keep pressing in, your own
      text comes round again.
- [x] **The chip row highlights the candidate currently applied**, moving the highlight
      as the cycle moves, so that pressing Tab three times is a visible act rather than a
      guess. With the user's own text restored, nothing is highlighted.
- [x] **The cycle ends** on any other keystroke, on a cursor move, on Escape and on blur.
      Once it has ended the inserted text is just text: the next Tab is the next Tab, not
      the fourth press of the old cycle.
- [x] **Tab is intercepted only while the row is showing**, and the row only appears
      after `\` plus at least one letter — so for all the rest of the time Tab moves
      focus out of the textarea exactly as it always did. State it as the obligation it
      is: a keyboard-only user who could not leave the editor would be trapped in it, and
      that is an accessibility failure and not a rough edge.
- [x] **Enter and space are never intercepted** — the user's newlines are their own.
      This is the whole point of hanging completion off Tab.
- [x] Escape dismisses the row until the next `\`. Mid-cycle it also ends the cycle,
      leaving the last applied candidate in place — Escape is "stop bothering me", not
      "undo"; the undo is Shift-Tab back to the start.
- [x] A tiny persistent hint on the row: **"Tab to cycle"** — it accepts nothing, it
      moves through them.
- [x] Show the replacement beside the name where there is one (`\alpha  α`), which is
      what makes the list worth reading.
- [x] Cap the row at a handful of entries; a scrolling chip row is a popup with extra
      steps. The cycle is over the row as shown, so the cap is also the length of the
      cycle — one more reason it stays small.
- [x] **The order the cycle walks, decided and implemented in the binding.** The app
      renders the array it is given, left to right, and Tab walks it in that order.
      In full: **an exact match on what was typed**, first of all and whatever its kind;
      then **what the document defines** (`fromDocument: true`), which also *replaces* a
      shipped entry of the same name rather than doubling it; then the **curated names,
      in the curated list's own order**, which is deliberate and is never re-sorted; then
      everything else by the old rule — macros before environments before specials, then
      the shortest name, then alphabetically. The curated list is 99 macros in
      `web/crate/src/complete.rs`, and it belongs there rather than in `rust/techxt`
      because "what people type most" is a fact about a completion UI and not about
      LaTeX; the note at the top of this section has the rest of the reasoning, what
      writing the list found out about `\begin`, and what it cost.

*Done when*: typing `\alp` offers `\alpha  α`, Tab completes it, Enter still inserts a
newline while the row is showing, a `\newcommand` written earlier in the document is
offered, and the row is usable by thumb on a 390 px screen.

---

# 6. The size and optimisation pass — *last*, after everything else lands

> **Done** — 2026-08-28, `a4ce824`. `opt-level = "s"` + `wasm-opt -Os` taken, MathJax
> switched from the SVG output to CHTML, and four budgets set from the measurements.
> PLAN §4.7, §9.1, §11 and §14 updated; §14 gains two new subsections, one per decision.
>
> **Both trades came out the same way, and neither on the number that was being argued
> about.**
>
> 1. **`opt-level = "s"`.** 1 241 264 B raw / 440 084 B gzipped → **971 601 / 370 323**,
>    so 269 663 B and 69 761 B back. The speed half — the thing this item existed for —
>    is that `"s"` costs about a fifth of the conversion *CPU* above 45 KB (27.4 → 34.0 ms
>    at 45 KB, 114.5 → 138.2 ms at 200 KB) and nothing under it (6.4 → 6.6 ms at 4.5 KB).
>    That reads like exactly the cost §4.7 was afraid of, until you ask where a person
>    stands: conversion is behind a Worker and a 120 ms debounce, so measured through the
>    whole app, keystroke to repaint, the same builds are 139.6 → 139.9 ms at 4.5 KB and
>    371.6 → 399.1 ms at 200 KB. **The premise "conversion speed is what a person feels
>    while typing" is false as stated** — what a person feels is the debounce — and it had
>    been steering this decision since W1. Meanwhile `"s"` makes the module *start* a
>    fifth faster (instantiation 17.5 → 14.5 ms, first convert 32.3 → 26.1 ms), because
>    there is a quarter less code to compile, and that is paid by every visitor rather
>    than by the rare 200 KB document.
> 2. **CHTML.** On the wire, for a reader who turns the mode on and opens all six shipped
>    examples: **414 244 B against SVG's 647 876 B**, and 3 171 162 B in `dist/` against
>    11 817 943 B. Verified fact 4 was right that CHTML wins and understated by how much
>    — it counted CHTML's woff2 but not its `chtml/dynamic` metric ranges, which are
>    550 677 B where the SVG equivalents are 9 968 318 B. That difference *is* the story:
>    an SVG range carries glyph outlines, a CHTML range carries metrics and lets a woff2
>    face draw. The switch is 3 lines in `src/mathjax.ts` and one array in
>    `vite.config.ts`, exactly as the item predicted.
>
> **Budgets.** `WASM_MAX_BYTES` 1 150 000 → **1 100 000**, `WASM_MAX_GZIP_BYTES`
> 450 000 → **415 000** — *lower* than the ceilings they replace, even though the module
> has gained L1, L2 and the completion surface since they were set, which is what taking
> the trade instead of raising the wire a third time buys. New beside them:
> `MATHJAX_MAX_BYTES` (**3 600 000**, the whole `dist/mathjax/` tree) and
> `MATHJAX_MAX_GZIP_BYTES` (**320 000**, `tex-chtml.js` gzipped). All four are the only
> authoritative copy; the plan restates none of them. Headroom is ~13 % on each, sized
> from two things rather than a round percentage: the toolchain spread (the same commit
> measured 1 199 689 B on a container's rustc and 1 120 513 B on a newer stable — 6.6 %,
> larger than most features) plus about one more feature the size of the completion
> surface. The size step is green: 128 399 B and 44 677 B of room on the module,
> 428 838 B and 39 101 B on MathJax.
>
> **One thing had to change that the item did not foresee, and it would have been a
> silent bug.** The service worker's display-font route matched *any* same-origin
> `.woff2`, which was unambiguous while the only woff2 in `dist/` were the five display
> faces — and stopped being so the moment the typesetter brought 105 of its own. Workbox
> takes the first matching route, so MathJax's faces would have landed in `techxt-fonts`
> and its `maxEntries: 8`, evicting the face the page is drawing in. It matches `/fonts/`
> now, and the MathJax route's cap went 48 → 160 to cover bundle + 40 ranges + 105 faces.
> This is a third entry in the family §9.1 already documents: nothing about it fails
> loudly. PLAN §9.1 has it written down.
>
> **What surprised me.** (1) `wasm-opt`'s own flag is nearly irrelevant here — `"s"` with
> `-O3` measured 978 045 B / 370 522 B and the same speed as `"s"` with `-Os`, so rustc's
> `opt-level` is the whole of both effects and "move them together" costs nothing to
> honour. (2) The measurement that had been deferred three times took about twenty
> minutes once someone opened a browser, which is worth remembering the next time
> something is waiting on evidence. (3) The `\$` document, the macro document, Copy, the
> wide-formula box at 1200 px and 390 px, and the offline reload all behave under CHTML
> exactly as item 2 recorded them under SVG — the four-function API really was
> output-agnostic.
>
> *Observed*, in Chromium 141 against `npm run preview`, with a request log: the
> *Mathematics* example typesets all six formulas with no error node and `\mathbb{R}`
> drawn from our own origin; verified fact 2's sentence typesets one formula and leaves
> both literal dollars; a `\newcommand{\ket}` document typesets inline and display with
> no `ket` in the output; Copy returns the Source-mode text byte for byte against the same
> document under *Math: Source*; switching back to *Fancy* leaves no wrapper elements; a
> wide display formula scrolls inside its own box at 1200 px and at 390 px while the page
> does not; a reload with the network off still typesets from the service worker's cache;
> and **not one request left the origin in any run**.

Everything about module size, optimisation flags and the CI budgets is deferred to
here, on purpose. Earlier items add weight (MathJax above all) and none of them should
be spending effort on a ceiling that the next item is going to move anyway.

**The size half is already measured**, on a 2026-08-28 container toolchain, both ends
built the same way (`opt-level` in `[profile.release]` moved together with `wasm-opt`'s
`-O3`/`-Os` in `[package.metadata.wasm-pack.profile.release]`):

| build | raw | gzipped |
|---|---|---|
| `opt-level = 3`, `wasm-opt -O3` (today) | 1 199 689 B | 421 748 B |
| `opt-level = "s"`, `wasm-opt -Os` | 935 590 B | 353 885 B |

264 KB off raw (22 %) and 68 KB off gzip (16 %) — enough to absorb MathJax's arrival
with room left over. The gzip figure matches §4.7's earlier 344 828 B closely enough to
trust. **The speed half is what is still missing, and it is the whole reason the last
budget raise was recorded as a deferral rather than a decision.**

- [x] **Re-weigh SVG against CHTML for MathJax**, on the corrected numbers in
      verified fact 4 above. The choice was made on a premise that turned out to be
      false (that SVG fetches no fonts at runtime), and on the true numbers CHTML is
      smaller both in JS and on disk. `src/mathjax.ts`'s API is output-agnostic, so a
      switch touches that file and `vite.config.ts` and nothing else. Measure what a
      reader actually fetches under each, not just what sits in `dist/`, and decide.
- [x] Re-measure both builds, since by then the module carries L1 and L2.
- [x] Measure conversion time **in a real browser**, before and after, on the §14
      documents. This is the measurement §14's profile table has wanted since W1. If
      `"s"` costs real typing latency, say so and take a different trade — do not flip
      it silently because the number looked good.
- [x] Decide and apply: `opt-level` and the `wasm-opt` flag together.
- [x] Set `WASM_MAX_BYTES` and `WASM_MAX_GZIP_BYTES` to the new reality, keeping the
      existing discipline that those two values in `.github/workflows/web.yml` are the
      *only* authoritative copy of the ceiling and the plan restates neither.
- [x] Add the MathJax bundle's own budget line beside them, under the same rule.
- [x] Record the measurements in §14 and note in §4.7 that the deferred trade was
      taken, and why.

---

# 7. The input pane's own keystroke cost, on a very large document

*Found while measuring item 5, and not caused by it.* A keystroke in a 200 KB document
costs about **83 ms** on the main thread before a single character is coloured, measured
in Chromium: `setRangeText` on a textarea that size, a forced layout to read `scrollTop`
so the mirror can be scrolled with it, and the mirror rebuilt from the whole text on
every keystroke. Highlighting adds 0.6 ms to that, because it is windowed (§6.12); the
83 ms is the machinery the diagnostic underline has used since W4.

It went unnoticed because the expensive-looking half of the app is behind a worker and a
120 ms debounce, and this half is not behind anything. The release checklist's item 5 has
in fact recorded "a keystroke round-trips in ~100 ms" since W7 without anyone asking what
the 100 ms was.

- [ ] **Rebuild the mirror incrementally, not wholly.** It has to *hold* every character
      — the alignment with the textarea above it is the whole mechanism — but an edit
      that changes one line need not replace every node. The chunk list is already
      computed from a window; keeping the nodes outside the edit and replacing only the
      run that changed is the obvious shape.
- [ ] **Stop forcing a layout inside the keystroke.** `backdrop.scrollTop =
      input.scrollTop` reads a value the edit just invalidated. Deferring the scroll sync
      to a `requestAnimationFrame` would let the browser lay out once instead of twice —
      check that the mirror does not visibly lag the textarea by a frame when it does.
- [ ] Re-measure with the A/B harness item 5 used (a 5 KB, a 20 KB and a 200 KB document,
      median of fifteen keystrokes, with the forced layout timed separately), and record
      the result in `web/PLAN.md` §6.12 and §14 beside the numbers it replaces.

*Done when*: a keystroke in a 200 KB document costs a fraction of what it costs today,
and the release checklist's item 5 can say so with a number.

---

# 8. The library, after using it: sealing an entry, and reading one

> **Done** — 2026-08-28, `077f772`. Sealing is one primitive in `src/library.ts`
> (`seal`, and `noteEdit` for the per-event rule and the lazy unseal); `src/main.ts`
> puts the three verbs over it, `src/ui/panes.ts` grows **New** beside `Load ▾`, a
> plain **Save** and an icon-only **★** in the output header, and the entry chip that
> names what is being written to; `src/ui/library-pane.ts` reads an entry, source and
> rendered preview each in its own scrolling region. `web/PLAN.md` §6.10 and §13 say
> so. 24 vitest cases were added over the fork rule, the sealing state machine and the
> merge-back, and one was rewritten where ★ changed meaning — 270 in all.
>
> **Six things this item did not say, and now does:**
>
> 1. **The output header's order is the table's**, not the prose's: Copy, Download,
>    **Save**, **★**. The paragraph above says "nothing that already shipped moves",
>    and Save did move — one place right, from before Copy to after Download — because
>    the table is the more specific instruction and ★ has to sit next to the button it
>    is the sealed-and-starred version of.
> 2. **The 30 % rule has an absolute floor**, `FORK_MIN_REMOVED = 24` characters.
>    Without it a two-character buffer forks on a backspace, which is a new entry per
>    keystroke and a toast with it. Any real document loses hundreds of characters to a
>    select-all, so the floor never decides a case the ratio was right about.
> 3. **A seal has to survive a reload**, which this item does not mention and which
>    would otherwise have re-broken item 3's "one entry per reload, not two": a sealed
>    session writes no id to `localStorage`, so on load a document that is in the log
>    *verbatim* is adopted **sealed** rather than logged again. Storing a seal flag
>    would have meant editing `src/state.ts`, which this item did not own.
> 4. **The seal ends at the input event, not at the conversion.** Both the fork and the
>    unseal are facts about an *edit*, and a document that fails to convert is being
>    replaced just as surely as one that does — so `noteEdit(before, after)` is called
>    from the pane's `onInput`, and `record` keeps only the guard that a conversion of
>    the *same* text writes nothing at all.
> 5. **Save dims itself once the entry is sealed**, rather than answering a second
>    click with nothing visible. It is the sealed state's second home on screen, beside
>    the chip's ✓.
> 6. **The chip costs the source pane's header its slack**, so `.pane-tools` there is
>    pinned against shrinking and the chip truncates instead — otherwise the buttons
>    wrapped onto a second row on a phone. The output header now carries five controls
>    and fits one row at 390 px; at 320 px it wraps, which is the safety valve it always
>    had.

Feedback from the owner running the branch. Two problems, both about the library
being *silent*: the current entry is overwritten without the user seeing it happen,
and an entry cannot be read once it is saved.

## The problem

Saving is automatic and one "current entry" absorbs edits as you type. Item 3 lists
what starts a new entry — a Load, a file open, a share link, an import, a reload, a
long idle gap — and a select-all-and-paste is none of them. It looks like typing. So
pasting a fresh document over an old one overwrites the old entry's source and the
old document is gone.

## The decision: the user seals, the heuristic only catches what they missed

**Three verbs over one primitive.** *Sealing* an entry means it stops absorbing edits;
the next edit starts a new one. The buttons differ only in what happens after:

| button | where | does |
|---|---|---|
| **New** | input pane header, beside `Load ▾` | seal the current entry, clear the input |
| **Save** | output pane header, after Copy/Download | seal it, keep it on screen |
| **★** | output pane header, icon-only toggle | seal it *and* star it |

**Placement is by moment of use, not by what the button acts on.** You reach for New
when you are about to type something new and your attention is on the input; you reach
for Save or ★ when you are happy with a *result* and your attention is on the output.
That beats grouping them by the fact that all three act on the document. It also means
nothing that already shipped moves — Save stays where item 3 put it, and only New is
added.

★ is an icon-only toggle rather than a fourth labelled button: that header is four
controls plus the ⇅ Focus button on a phone, and it already hides labels below 620 px.
Star on an already-sealed entry just toggles the flag — no second seal.

## Five things to get right

- [x] **Save must not create a phantom empty entry.** Seal the current entry, then
      create the next one **lazily, on the first edit** — never eagerly. Otherwise
      pressing Save and walking away leaves an empty entry in the log.
- [x] **The per-event fork rule, as the safety net underneath.** While a draft is
      unsealed, **a single input event that removes more than ~30 % of the document
      starts a new entry.** Measure the change *in that one event*, never cumulatively
      against the stored source: ordinary typing changes one character and can never
      trip it; appending or pasting at the end removes nothing and can never trip it;
      select-all-and-paste and select-all-and-delete trip it every time. A cumulative
      rule drifts — a long session that rewrites a section at a time crosses any
      threshold while genuinely being one document.
      **Bias toward forking.** A wrong fork costs one extra entry in a log that is
      filterable and only ever pruned deliberately; a wrong non-fork loses work. That
      asymmetry is the whole argument for a low threshold and against cleverness.
- [x] **Make the current entry visible.** The real complaint is silence, and no
      heuristic fixes that. Show which entry is being written to, and say so when one
      is sealed or forked — a toast on an automatic fork, with an undo that merges the
      new draft back into the previous entry. Then a wrong guess costs a click instead
      of the user's work, and the behaviour is legible whether or not it guessed right.
- [x] **New needs an Undo**, in the toast, restoring the document and unsealing. The
      app's existing single-level-undo idiom — Load ▾ already does exactly this.
- [x] **"Save" is now slightly a lie**, since everything is already saved
      continuously and the button really means *stop changing this one*. Carry the
      truth in the tooltip: "Keep this version — further edits start a new entry."

## Reading an entry

- [x] **The detail pane shows the entry's full source, in its own scrolling region.**
      Nothing new to store: an entry already keeps the whole `source`. The small
      stored `preview` is the *rendered* output and stays what it is — a card
      preview, not the document.
- [x] Keep the source and the rendered preview both reachable in the detail view, per
      item 3's split; on a phone the list → detail shape already there still applies.

## Left open — answered

> **Answered and implemented** — 2026-08-28, `548b7e0`. The owner's answer removes the
> question rather than picking a side of it: *everything in the library is sealed except
> the very last one* — meaning the entry the session is writing into. Seal-ness is
> therefore **derived**, and the `sealed` field the argument-against was about — in the
> entry model, in §6.11's export format, in every import that would have to sanitise it —
> never has to exist. `adoptionOnOpen` in `src/library.ts` is the whole of it, `openEntry`
> in `src/main.ts` is its only caller, and PLAN §6.10 carries the rule.
>
> **Three consequences, written down because none of them is obvious from the rule.**
> (1) After Save, ★ or New *nothing at all* is unsealed until the next edit — the next
> entry is created lazily, so the invariant is "at most one unsealed entry, and if there
> is one it is the most recent", not "the last one is always unsealed". (2) The rule is
> about identity, not list position: a filtered or searched list can perfectly well show
> a sealed entry at the top, so the chip stays the thing that says which entry is live.
> (3) Editing an entry you opened never updates it in place any more — it forks a copy —
> which is the asymmetry every other seal is already decided on: a wrong fork costs an
> entry in a filterable log that is only ever pruned deliberately, and a wrong non-fork
> costs work. Unlike the automatic fork, this one offers no Undo toast, because nothing
> was lost to undo: the kept version is untouched and the edits are in the new entry.
>
> **What the browser showed that the code did not.** The forked entry takes its title
> from the same `\section` as the entry it came from, so the chip reads ● *Alpha* where
> it read ✓ *Alpha* a moment earlier and the list holds two rows called Alpha. That is
> honest — they are two versions of one document, which is what the log is for — but the
> glyph is the only thing that distinguishes them, and it is worth knowing before someone
> reports it as a bug.

- [x] **Opening an entry comes back sealed unless it is the one being written into.**
      Sealing is a fact about the editing session and is not stored on the entry, so a
      version the user kept a week ago started absorbing edits again the moment it was
      opened — item 3's rule for Open, unchanged. It may well be the wrong one now that
      Save exists: the argument for making it come back sealed is that "keep this
      version" ought to outlive the session, and the argument against is a `sealed` field
      in the entry, in the export format, and in every import that has to sanitise it.
      Left as a question rather than a silent decision — and answered by the owner, in
      the note above, in the one way that pays neither price.

*Observed* for the sealing rule, in Chromium against `npm run preview` on 2026-08-28:
typing one document, pressing **New**, typing a second, then opening the first from the
pane brings it back as ✓ *Alpha* — sealed — and the next keystroke leaves the kept
version byte for byte as it was and starts a third entry holding the edit; opening the
*live* draft instead leaves it ● and typing into it adds no entry at all, which is the
one case the rule must not fork.

*Observed*, in Chromium on 2026-08-28: typing a document and then pasting a different
one over it with everything selected leaves **both** in the library, with a toast that
names the one that was kept and an Undo that folds the draft back into it; Save seals,
says so, and adds nothing to the log however long the app is then left alone, while the
next keystroke starts a new entry; ★ seals and stars, and a second ★ only removes the
star; New clears the input with an Undo that brings back the document *and* the entry
it was being written to; the chip reads ● *title* while writing, ✓ *title* once sealed,
and opens that entry in the library when clicked; a reload of a kept document finds one
entry rather than two and comes back sealed; the detail scrolls the whole source in its
own region at 390 px with the actions still at the top; and a profile with `indexedDB`
throwing converts normally with Save, ★ and the chip hidden and the pane honest.

---

# 9. What MathJax does not know that techxt does

Owner feedback from running the branch: several constructs render as *undefined* in
MathJax mode — `\ket`, `\bra`, `\ketbra`, `\norm`, `\abs`, `\coloneqq`,
`psmallmatrix`, `bsmallmatrix`.

## What was measured, 2026-08-28

The eight split into two different bugs, and only one of them is about MathJax:

| construct | techxt | where MathJax has it |
|---|---|---|
| `\ket` `\bra` `\braket` `\ketbra` | **defines** | `braket` (`\ketbra` may need `physics`) |
| `psmallmatrix` `bsmallmatrix` `smallmatrix` | **defines** | `mathtools` |
| `\norm` `\abs` `\coloneqq` | **does not define** — `warning: no text rule for the macro` | `physics` / `mathtools` |

`Source` mode re-emits every formula verbatim whether techxt understands it or not,
which is why the last three reach MathJax at all and why the gap is only visible
there. In `Fancy` mode they warn and render their argument bare.

So `\norm`, `\abs` and `\coloneqq` are a **library** question — common enough that
techxt arguably should define them — and not this item's. Raise it separately against
`rust/techxt`; do not fix it by teaching MathJax alone, which would leave Fancy mode
still wrong and the two modes disagreeing.

## Do not patch eight names

The reported eight are a sample, not the set. techxt ships ~1 100 macros and the
current MathJax package list is `base, ams, newcommand, configmacros, noundefined`;
nobody has ever compared the two. Patch the list and the next document finds the next
gap.

- [ ] **Measure the whole gap.** L2 gave the binding a `SymbolIndex`, so the set of
      names techxt defines is now enumerable. Walk it, typeset each construct in
      MathJax under Node, and produce the definitive list of what MathJax does not
      know. This is the same idea as `tools/coverage_check.py`, which already gates
      *glyph* coverage in CI — that script is the precedent for both the shape and
      the reporting.
- [ ] **Then choose packages against the measurement**, not against the report. Two
      cautions:
      - **`physics` is the risky one.** The LaTeX package aggressively redefines
        unrelated things (`\div`, `\dd` and friends) and MathJax's port carries that,
        so loading it globally changes documents that never mention `\ket`. Prefer
        `braket` + `mathtools`, and check what is actually lost.
      - **`configmacros` is already loaded**, so any remaining techxt-known macro can
        be supplied as a definition in the MathJax config — precise, no collateral,
        and it keeps the package list short. Prefer this over a heavyweight package
        pulled in for two names.
- [ ] **Gate it.** Once the gap is closed, a check that fails when a techxt-known
      construct is unknown to MathJax stops it silently reopening. Follow
      `coverage_check.py`'s policy: a hard gate on the core, a warning into the job
      summary for the long tail, since some of ~1 100 names will never matter.
- [ ] Whatever the outcome, `noundefined` stays: a construct nobody anticipated should
      render as a marker, not kill the formula.

**Depends on item 6**, which owns `web/src/mathjax.ts` and may replace its output
renderer entirely (the SVG→CHTML re-weigh). Do not start until that has landed —
the package list is a small part of a file item 6 may rewrite.

---

# Order of work

1. **Item 1** — independent, small, unblocks nothing but costs nothing.
2. **L1 + item 2** — the library change lands first with its own tests, then the
   binding, then the app. Nothing about size or optimisation flags belongs here; that
   is item 6.
3. **Items 3 and 4 together** — they are one feature; the export format is easier to
   get right while the entry model is still being written.
4. **L2 + item 5** — the largest, and the one most likely to want its scope trimmed
   after the survey.
5. **Items 7 and 8** — both found by using the app rather than by planning it. Item 8
   is owner feedback from running the branch and outranks item 6; item 7 is a
   latency bug that predates all of this work and is independent of everything.
6. ~~**Item 6 last**~~ — done. What remains after it is item 7 (a latency bug older
   than all of this) and item 9 (which had to wait for item 6, since item 6 replaced
   the file it edits).

Item 7 was added while item 5 was being measured and is independent of all of them; it
touches the pane and not the module, so it does not have to wait for item 6.


Every item edits `web/PLAN.md`; L1 and L2 also edit the root `PLAN.md`. vitest covers
pure logic only, so the library store, the import/export codec, the region→element
mapping and the completion matcher should all be written as pure functions that a test
can reach without a DOM.

---

# Instructions for implementer agents

You are picking up one item from this file, probably with no memory of the discussion
that produced it. This section is the practical half: how to get a working toolchain,
what to run, and what *not* to spend effort on.

## Setting up the build from nothing

A fresh container normally has Node 22 and a Rust toolchain already. Check first, then
add what is missing:

```sh
node --version && npm --version && cargo --version   # expect node 22, cargo 1.9x
rustup target add wasm32-unknown-unknown             # needed, usually not preinstalled
command -v wasm-pack || cargo install wasm-pack --locked   # ~2 min to build
```

Then, **before the first `npm run wasm`**, work around the one thing that reliably
breaks (below). After that:

```sh
cd web
npm ci                # ~10 s
npm run build         # wasm-pack, tsc --noEmit, vite build
npm test              # vitest — 85 tests at the time of writing
```

### Trap 1: `wasm-opt` cannot download itself

`wasm-pack` fetches binaryen 117 from GitHub releases using its own HTTP client, which
does not go through the sandbox's proxy. The build gets all the way to the last step
and dies with:

```
Error: failed to download from https://github.com/WebAssembly/binaryen/releases/download/version_117/binaryen-version_117-x86_64-linux.tar.gz
To disable `wasm-opt`, add `wasm-opt = false` to your package metadata in your `Cargo.toml`.
```

`curl` fetches that exact URL fine, so seed wasm-pack's cache by hand and it never
asks again:

```sh
curl -sSL -o /tmp/binaryen.tar.gz \
  https://github.com/WebAssembly/binaryen/releases/download/version_117/binaryen-version_117-x86_64-linux.tar.gz
mkdir -p ~/.cache/.wasm-pack/wasm-opt-1ceaaea8b7b5f7e0
tar xzf /tmp/binaryen.tar.gz -C ~/.cache/.wasm-pack/wasm-opt-1ceaaea8b7b5f7e0 --strip-components=1
~/.cache/.wasm-pack/wasm-opt-1ceaaea8b7b5f7e0/bin/wasm-opt --version   # → version 117
```

The directory name has to match the `.wasm-opt-*.lock` file wasm-pack leaves in
`~/.cache/.wasm-pack/` — list that directory if the hash above stops working, and if
the binaryen version in the error message ever changes, change it in the URL too.

**Do not "fix" this by setting `wasm-opt = false` in `web/crate/Cargo.toml`.** That
file's flags exist for a documented reason (`web/PLAN.md` §4.7 and Appendix B) and the
disabled build is not the one that ships.

### Trap 2: a container's rustc is not CI's

The container measured above ran rustc 1.94.1 and produced a **1 199 689 B** module —
about 50 KB over `WASM_MAX_BYTES` — while CI, on a newer stable, was green on `main`
at the same commit. A local size overrun therefore proves nothing on its own. Check
the workflow's own runs before concluding a budget has been tripped, and see the next
heading before concluding you should do anything about it.

## Ignore the wasm size budgets while implementing

**`WASM_MAX_BYTES`, `WASM_MAX_GZIP_BYTES`, `opt-level` and the `wasm-opt` flags are
out of scope for items 1–5.** A red size step during this work is expected and is not
yours to fix. Do not raise a ceiling, do not flip an optimisation flag, do not trim a
feature to fit a number.

Sizing, optimisation and the browser-side speed measurement are **item 6**, one pass
after the whole plan has landed, precisely so they are done once against a settled
target instead of five times against a moving one.

What you *should* do is leave item 6 something to work with: note in your commit
message what your change cost in `dist/` if it is material (a new dependency, a
bundled library), and prefer a lazily fetched asset to one in the main bundle where
the choice exists.

## Working rules

- **`web/PLAN.md` is normative and stays that way.** Every item here changes it —
  items 2 and 5 reverse stated non-goals outright. Update the plan *in the same commit*
  as the code, in the plan's own voice: what was decided and why, not a changelog.
  L1 and L2 also update the root `PLAN.md`. A plan that silently disagrees with the
  code is worse than no plan.
- **Keep the invariants in *Context every item needs* above.** In particular: app-level
  options never reach the binding, a read of stored or shared data never throws, the
  output pane never gets `innerHTML`, and nothing the user copies or downloads is ever
  something the app added for display.
- **Tests.** vitest runs in `node` with no DOM, so anything on the app side worth
  testing has to be a pure function: the library store's retention and quota policy,
  the import/export codec, the region → element mapping. Write them that way from the
  start rather than extracting them afterwards. Rust changes get tests in
  `rust/techxt/tests/` (L1, L2) and `web/crate/tests/` (the binding — which is where
  the completion matcher's tests belong, since the matching happens there).
- **CI runs exactly the checks listed under *When your item is done* below**, and
  `missing_docs` is denied in `web/crate` too — so a doc comment on a new public item
  is not optional there.
- **The repository's prose has a voice** — the plan and the commit messages explain
  *why*, in full sentences, and assume a reader who was not in the room. Match it.

## Keeping this file up to date

This file is a design record, not a scratchpad. It is the only place several of these
decisions are written down, so it has to stay true as the work lands.

**As you go**, tick the boxes: `- [ ]` becomes `- [x]`. Nothing else.

**When an item is finished**, add a status line directly under its heading and leave
the rest of the item exactly where it is:

```markdown
# 1. Wrap defaults to soft-wrap

> **Done** — 2026-09-02, `abc1234`. Soft is the default; the select reads
> Fit / Off / Soft; PLAN §5 and §6.3 updated.
```

**Do not delete a finished item, and do not strike it through.** The value of this file
is the *why*, which outlives the work — the next person to touch soft-wrap needs to
know it was deliberate that old links changed meaning. A struck-through wall of text is
also unreadable, and these items are long. A `> **Done**` line at the top says
everything a reader needs to skip it.

**Do not renumber or reorder the items.** Commit messages, the *Order of work* list and
these instructions all refer to "item 3", "L1", "item 6". A stable number is worth more
than a tidy sequence.

Three more rules:

- **If reality diverged from the plan, fix the text — do not leave the correction in a
  commit message.** Say what actually happened and why, in the item, in the same commit
  as the code. The commit message explains the change; this file explains the design.
- **New work you discover belongs here**, as a new checkbox in whichever item owns it,
  or as a new item at the end if it owns nothing. A follow-up that exists only in
  someone's head is a follow-up that does not exist.
- **When every item is done, this file's job is over.** Fold whatever is still true and
  still normative into `web/PLAN.md` (and the root `PLAN.md` for L1 and L2), then
  delete `TODO.md` in that same commit. The plan is where design lives once it has
  shipped; keeping a completed TODO alongside it just gives a future reader two
  documents to reconcile.

## When your item is done

Before you call it finished, in roughly this order:

1. **Run the full check set.** `npm run typecheck`, `npm test`, `npm run build`, and
   for a Rust change `cd rust && cargo test` plus, in `web/crate`, `cargo fmt --all
   --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`. Do not push
   on a red anything except the wasm size step, which is item 6's.
2. **Check the item's own *Done when* line.** Each item ends with one. It names the
   observable behaviour, not the diff — actually observe it, in a browser, rather than
   reasoning that it must hold.
3. **Update `web/PLAN.md`** in the same commit — and the root `PLAN.md` for L1 and L2.
   Not a changelog entry: the plan is written as present-tense design, so change the
   design it describes.
4. **`web/README.md`**, if and only if the developer-facing story changed: a new npm
   script, a new dependency, a new step in a build, a new dev-only tool.
5. **Tick the boxes and add the `> **Done**` line**, per the section above.
6. **If you touched `src/examples.ts`**, re-check the invariant `web/PLAN.md` §6.7
   states: every shipped example converts with **no diagnostics at all**. It has been
   true since W2 and is worth keeping.
7. **Note what you cost, for item 6.** If your change adds material weight to `dist/` —
   a bundled library, a new dependency — put the figure in the commit message. Item 6
   should not have to bisect for it.
8. **Commit and push to the working branch.** One commit per item where the item is
   coherent; a Rust change plus its app-side consumer may reasonably be two (the
   library change standing on its own, with its own tests, is a feature of the plan
   rather than an accident).
9. **Say what surprised you.** If something here was wrong, if a decision looks worse
   from the inside than it did from the outside, or if you found a cheaper way — write
   it down, in the item, and say so plainly when you report. The next agent reads this
   file, not your transcript.
