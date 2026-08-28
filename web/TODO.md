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
   | `tex-svg.js` | 1 849 625 B | 615 224 B | none |
   | `tex-chtml.js` | 997 445 B | 280 899 B | some of 105 woff2 files, 1.8 MB total |
   | `tex-svg-nofont.js` | 873 900 B | 254 793 B | font package separately |

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

**The smallest item. Do it first; it is unblocked and touches nothing else.**

`wrap` already has the value `'soft'` internally — it is only the *default* and the
*label* that change.

- [ ] `DEFAULT_OPTIONS.wrap` in `src/state.ts` becomes `'soft'`.
- [ ] `src/ui/controls.ts` (~line 90): the wrap select reads `Fit the pane` /
      `Off` / `Soft (default)`. Today it says `Off (default)` and
      `Off, soft-wrapped`; the "(default)" marker moves and the third entry is renamed
      to just **Soft**. Keep the hint sentence, which is where the full explanation of
      the three answers lives.
- [ ] `web/PLAN.md` §5 and §6.3: the option table's default column, and the prose that
      calls *Fit* "the app being helpful".
- [ ] Update `test/state.test.ts` / `test/options.test.ts` wherever they assert the
      default or rely on `pruneOptions` dropping `wrap`.

**Decided: no backwards compatibility.** A share link or a stored setting that omits
`wrap` now means Soft where it used to mean Fit. No migration, no explicit `wrap:'fit'`
written into old state. **And Copy/Download now hand over unwrapped long lines, which
is intended** — that is what Soft *is*, and the reader's own text viewer can fold them.

*Done when*: a fresh profile lands on Soft, the pane folds long lines, Download
produces one line per paragraph, and the tests say so.

---

# 2. `Math: MathJax` — optional visual math

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

- [ ] `web/crate/src/diag.rs`: map `regions` into the result DTO, converting byte
      offsets to **UTF-16 code units** — the same mapping §4.4 already does for
      diagnostic spans; reuse that machinery rather than writing a second one.
- [ ] `src/worker/protocol.ts`: `ConversionResult` gains
      `regions: MathRegion[]` (`{ start, end, display }`).
- [ ] The binding reports regions unconditionally; the app ignores them unless it is
      in MathJax mode. There is no option to turn them on.

## Shipping MathJax

- [ ] **Bundle it. No CDN, ever** — it would break both the offline promise and the
      privacy claim in About.
- [ ] **Take the SVG output, `tex-svg`** (615 KB gzipped, one file, zero runtime font
      fetches) rather than CHTML (281 KB gzipped but 105 woff2 files, 1.8 MB, that an
      offline-first app would have to precache anyway). One asset is the whole offline
      story. Revisit with a custom `@mathjax/src` build — we know exactly which TeX
      extensions are needed and need neither MathML input nor the a11y tree — if the
      number has to come down.
- [ ] **Lazy on the web, complete when installed.** Fetch the bundle on first
      selection of the MathJax mode, held by a `CacheFirst` runtime route beside the
      existing `techxt-fonts` one. An installed PWA should not have to think about it:
      fetch it once on first run in the background and keep it. Consider extending the
      existing *keep all fonts offline* checkbox into one "keep everything offline"
      setting rather than adding a second one.
- [ ] Configure the TeX input with the package set the primitives need — `base`,
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

- [ ] Inline math: MathJax handles line-breaking within an inline formula; let it.
- [ ] Display math: give each formula its own horizontally scrolling box, so a wide
      formula scrolls by itself instead of forcing the whole pane sideways. This
      matters most under Soft wrap, which is now the default.
- [ ] Fit-to-pane column measurement is meaningless across a typeset formula. Under
      MathJax mode this is a known and accepted imprecision; do not try to correct it.
- [ ] `mathExpressionIn`, `matrixDelimiters` and `mathFont` do nothing in this mode
      (they are rendering options that Source mode bypasses). Disable them in the
      *More options* → *Math* fieldset while MathJax is selected, with a one-line
      explanation, rather than leaving inert controls.
- [ ] Copy and Download hand over the source-mode text, `$…$` included. Say so in the
      control's hint: this is the one mode where what you see and what you copy differ.
- [ ] Typesetting is async and can be slow on a large document. Do not block the pane:
      set the text first (it is readable immediately), then typeset, and drop a
      typeset pass whose conversion has already been superseded.

*Done when*: the sentence from fact 2 renders with exactly one formula and two literal
dollar signs; a document using `\newcommand` in math typesets without MathJax knowing
the macro; Copy still returns the library's text byte for byte; and a cold reload with
the network off, after MathJax has been used once, still typesets.

---

# 3. The library — an automatic log of what you converted

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

- [ ] **IndexedDB**, one database, one object store keyed by `id`, with indices on
      `updatedAt` and `starred`. `localStorage` keeps the session state as it does
      today; the two are separate and neither can exhaust the other.
- [ ] Call `navigator.storage.persist()` the first time an entry is written, so the
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

- [ ] Cap a single entry's `source` at the same 512 KB `MAX_STORED_DOC` uses, and tell
      the user an entry was too large to log — never log it truncated, and never let
      one huge paste be the reason something else gets dropped.
- [ ] **Private browsing.** IndexedDB may be absent, or present and ephemeral. Do not
      change the app's behaviour: offer the library as usual, and if it is easy to
      detect (`navigator.storage.estimate()`, a failed `persist()`), show a small
      ⚠️ note in the library header saying this browsing session will probably not keep
      these and pointing at Export. If IndexedDB throws outright, degrade the way
      `browserStorage()` already does for `localStorage`: an inert, honest "not
      available here" state, never a broken button.
- [ ] **A local file for more space** is Chromium-only (`showSaveFilePicker` and a
      persisted handle) and unavailable on iOS, so it is *not* the answer for the base
      feature. Export (item 4) is. Revisit a File System Access backend as a
      desktop-only convenience once the rest works, if the quota warning turns out to
      fire in practice.

## The pane

- [ ] **A `<dialog>` sheet**, like About and Install, using `src/ui/sheets.ts`. A
      scrolling list of entries belongs inside a dialog in an app whose page never
      scrolls (§6.8, D8), and the sheet machinery already gives Escape, the backdrop,
      focus handling and inertness for free.
- [ ] **Open it from the header**, beside About and Install, where the app's other
      sheets live. Also offer it from the primary options row next to *More options*,
      since that is where it was originally asked for — one action, two doors, and no
      third row on a phone.
- [ ] **Desktop**: list on the left, selected entry on the right — title, date,
      options summary, the preview, and the actions. **Phone**: list, tapping an entry
      pushes to its detail with a back control. Same data, one column.
- [ ] Per entry: open, star/unstar, rename, delete, and copy/download its source.
- [ ] Filters: **all / starred**, and a text search over title and source. Sort by most
      recently updated.
- [ ] **Delete** removes one entry, with an Undo in the toast (the app's existing
      single-level-undo idiom).
- [ ] **Clear library** with a real confirmation — a typed confirmation or a two-step
      dialog naming the count, not a bare "are you sure". Make it hard to lose data
      by accident. Starred entries are counted separately in the confirmation.
- [ ] **Discoverability.** The library only helps if people know it is there:
      - The first time an entry is auto-logged, a one-time toast: *"Saved to your
        library"* with an **Open library** action.
      - A subtle pulse on the library button for the first ~3 sessions, driven by a
        counter in `localStorage`, then never again.
- [ ] **About** gains a sentence: the library is stored on this device only and is
      never uploaded, alongside the existing privacy line.

*Done when*: converting a document and reloading finds it in the library; starring
survives a prune; deleting one entry is undoable; the pane is usable one-handed on a
390 px screen; and a browser with IndexedDB blocked still shows a working app.

---

# 4. Library import and export

- [ ] **Export**: the whole library as one JSON file, downloaded through the same
      `Blob` path the output's Download button uses. Named
      `techxt-library-YYYY-MM-DD.json`.
- [ ] **Format**, versioned, and boring on purpose:
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
- [ ] **Include the preview** so an imported library is legible before anything is
      re-converted. This is the strongest argument for keeping the preview genuinely
      small — a few lines, not the whole rendering.
- [ ] **Import offers explicit options** in a dialog, so nothing about the result is a
      surprise:
      - **Add to my library** (the default) — everything already there is kept; an
        id collision gets a fresh id rather than overwriting.
      - **Skip items I already have** (a checkbox on the above) — matched by a hash of
        `source` + `options`, not by id.
      - **Replace my library** — behind its own confirmation naming what will be lost,
        including the starred count.
- [ ] **Existing entries are never removed unless the user explicitly chose Replace on
      that particular import.** No heuristic, no "clean up duplicates", no exception.
- [ ] Report the outcome: *"12 added, 3 skipped, 0 replaced."*
- [ ] **Treat an import as hostile input**, with the discipline `decodeShare()` already
      uses: every field through a validator, unknown fields dropped, unknown option
      values dropped (`sanitizeOptions` is already exactly this function), size caps,
      and a read that never throws. A refusal names what was wrong with the file.
- [ ] Unit-test the codec the way the share codec is tested: round-trip, truncation, a
      foreign file, a future `v`, an item with a bad option value.

*Done when*: a library exported from one profile imports into another with previews
intact, and every import path has been shown not to remove an existing entry.

---

# 5. A lighter editor: highlighting and completion

**This reverses `web/PLAN.md` §1 and §16**, which currently name syntax highlighting
and a code editor component as non-goals ("a textarea is honest and fast, and
CodeMirror would outweigh the engine"). The reversal is deliberate and must be written
into those sections — *and the reasons they gave stay true and become the constraints*:
whatever ships must not outweigh the engine, and must not make typing slower.

## Approach, decided

- [ ] **Survey CodeMirror 6 and friends first**, then **hand-roll**, which is the
      expected outcome. Record the survey's conclusion in `PLAN.md` §16's replacement
      so the decision is not re-taken from scratch later.
- [ ] **Keep the `<textarea>`.** Highlighting is an overlay: a `<pre>` mirror behind a
      transparent-text textarea. `src/ui/panes.ts` **already maintains a hidden mirror
      element** for positioning diagnostic gutter markers, so half the machinery and
      all of the metric-agreement discipline is there to build on.
- [ ] **`contenteditable` is out.** It breaks `setSelectionRange`, which
      `Panes.selectSpan` — the diagnostics' jump-to-source — depends on.
- [ ] Watch the known overlay failure modes, all of which get worse on a phone with the
      keyboard up (a stated §6.6 priority): IME composition, scroll synchronisation,
      exact font-metric agreement between mirror and textarea, and mobile autocorrect.
      If the overlay cannot be made to behave on a touch device, ship highlighting on
      pointer devices only rather than shipping something that fights the keyboard.

## Highlighting

- [ ] Minimal and structural: commands, math delimiters and their contents, comments,
      braces, and environment `\begin`/`\end` pairs. Not a LaTeX grammar — a lexer.
- [ ] It must survive a 200 KB document without making a keystroke feel slow. Highlight
      the visible region plus a margin, not the whole buffer, if measurement demands.
- [ ] It shares the pane with the existing diagnostic underline and gutter markers;
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

**Where the suggestions come from.**

- [ ] **techxt's own declared symbols**, through the wasm module (below).
- [ ] **The user's own definitions, the cheap way — and in Rust, not JS.** Scan the
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

- [ ] **The index lives in wasm and stays there.** ~1 100 macros with their
      replacements is a table the JS side has no reason to hold a second copy of.
      `Session` builds a sorted `SymbolIndex` lazily on the first completion request
      and keeps it.
- [ ] Export `Session.complete(latex, prefix, limit)` returning a small array of
      `{ name, kind, replacement, arity, fromDocument }` — a binary search plus a
      prefix scan over the index, plus the definer scan over `latex`, merged and
      ranked. Microseconds either way, and the payload is a handful of entries.
      The document is passed in rather than remembered so the call stays stateless;
      if the scan ever shows up in a profile, cache it against the text's length and
      hash inside `Session` and leave the signature alone.
- [ ] `src/worker/protocol.ts` grows
      `{ type: 'complete', id, text, prefix, limit }` and
      `{ type: 'completions', id, items }`, with the same monotonic-id discipline
      conversions use: a stale answer is dropped.
- [ ] **The one risk is head-of-line blocking** behind a slow conversion in the same
      worker. Conversions are debounced 120 ms and take 2–20 ms on ordinary documents,
      so there is normally a gap. Measure it on a 200 KB document. If it is laggy, add
      a small JS-side prefix→results cache before reaching for a second worker; a
      second wasm instance is a megabyte of memory for a nicety.
- [ ] **The JS side does no matching, no merging and no ranking.** It sends a prefix
      and renders what comes back. Every rule about what is offered and in what order
      lives in one place, in Rust, next to the table it is drawn from.

**The completion UI, decided.**

- [ ] **A row of chips below the input**, not a popup. It works identically on desktop
      and on a phone, it never covers what you are typing, and it degrades to nothing
      when there is nothing to suggest.
- [ ] Trigger only after `\` plus at least one letter. Never on `\` alone, never in the
      middle of a word.
- [ ] **Tab accepts the first chip**, which is visually highlighted to show that it is
      the one Tab takes. Click or tap accepts any chip.
- [ ] **Enter and space are never intercepted** — the user's newlines are their own.
      This is the whole point of Tab-only acceptance.
- [ ] Escape dismisses the row until the next `\`.
- [ ] A tiny persistent hint on the row: **"Tab to accept"**.
- [ ] Show the replacement beside the name where there is one (`\alpha  α`), which is
      what makes the list worth reading.
- [ ] Cap the row at a handful of entries; a scrolling chip row is a popup with extra
      steps.

*Done when*: typing `\alp` offers `\alpha  α`, Tab completes it, Enter still inserts a
newline while the row is showing, a `\newcommand` written earlier in the document is
offered, and the row is usable by thumb on a 390 px screen.

---

# 6. The size and optimisation pass — *last*, after everything else lands

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

- [ ] Re-measure both builds, since by then the module carries L1 and L2.
- [ ] Measure conversion time **in a real browser**, before and after, on the §14
      documents. This is the measurement §14's profile table has wanted since W1. If
      `"s"` costs real typing latency, say so and take a different trade — do not flip
      it silently because the number looked good.
- [ ] Decide and apply: `opt-level` and the `wasm-opt` flag together.
- [ ] Set `WASM_MAX_BYTES` and `WASM_MAX_GZIP_BYTES` to the new reality, keeping the
      existing discipline that those two values in `.github/workflows/web.yml` are the
      *only* authoritative copy of the ceiling and the plan restates neither.
- [ ] Add the MathJax bundle's own budget line beside them, under the same rule.
- [ ] Record the measurements in §14 and note in §4.7 that the deferred trade was
      taken, and why.

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
5. **Item 6 last**, once nothing else is going to move the number.

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
