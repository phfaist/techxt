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

**All of the above was verified end to end in a fresh container on 2026-08-28**:
`npm ci`, `npm run build` (wasm included), `npm test` (85 passing), `npm run
typecheck`, `cd rust && cargo test` (all green) and `cd web/crate && cargo test`.

Two container gotchas worth knowing before you lose an hour to them:

- **`wasm-opt` cannot download itself.** `wasm-pack` fetches binaryen 117 from GitHub
  releases with its own HTTP client, which does not use the sandbox's proxy, and the
  build dies at the last step with `failed to download from …binaryen…`. `curl` gets
  the same URL fine, so seed wasm-pack's cache by hand and it never asks again:
  ```sh
  curl -sSL -o binaryen.tar.gz \
    https://github.com/WebAssembly/binaryen/releases/download/version_117/binaryen-version_117-x86_64-linux.tar.gz
  mkdir -p ~/.cache/.wasm-pack/wasm-opt-1ceaaea8b7b5f7e0
  tar xzf binaryen.tar.gz -C ~/.cache/.wasm-pack/wasm-opt-1ceaaea8b7b5f7e0 --strip-components=1
  ```
  The directory name matches the `.wasm-opt-*.lock` file wasm-pack leaves in
  `~/.cache/.wasm-pack/`; check it if this stops working. Do **not** "fix" this by
  setting `wasm-opt = false` in the committed `Cargo.toml`.
- **A container's rustc is not CI's.** This container is on 1.94.1 and produces a
  1 199 689 B module — 49 689 B *over* `WASM_MAX_BYTES`. CI, on a newer stable, is
  green on `main`. So a local size overrun is not by itself a broken build; check the
  workflow's own runs before believing a budget has been tripped.

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

   For comparison the wasm module today is 1 120 513 B raw / ~400 000 B gzipped.
5. **`opt-level = "s"` buys 264 KB raw / 68 KB gzipped** — the measurement in item 2.
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

## Paying for the growth: `opt-level = "s"`

The app roughly doubles. `web/PLAN.md` §4.7 has named `opt-level = "s"` as the first
response since W1, and §14 has wanted the browser-side speed comparison for as long.

**The size half is already measured**, on this container's toolchain, both ends built
the same way (`opt-level` in `[profile.release]` moved together with `wasm-opt`'s
`-O3`/`-Os`):

| build | raw | gzipped |
|---|---|---|
| `opt-level = 3`, `wasm-opt -O3` (today) | 1 199 689 B | 421 748 B |
| `opt-level = "s"`, `wasm-opt -Os` | 935 590 B | 353 885 B |

264 KB off the raw figure (22 %) and 68 KB off the gzip one (16 %) — enough to absorb
MathJax's arrival with room left over, and it brings raw back under budget on this
toolchain too. The gzip number matches §4.7's earlier 344 828 B closely enough to
trust. **The speed half is the part still missing, and it is the whole reason the last
budget raise was a deferral.**

- [ ] Flip `[profile.release] opt-level` to `"s"` in `web/crate/Cargo.toml`, and
      `wasm-opt`'s `-O3` to `-Os` in the same file's
      `[package.metadata.wasm-pack.profile.release]`.
- [ ] Measure conversion time **in a real browser**, before and after, on the §14
      documents. This is the measurement §14's profile table has wanted since W1 and
      the one the ceiling raise was explicitly deferred against. If `"s"` costs real
      typing latency, say so and take a different trade — do not flip it silently.
- [ ] Record both in §14, and lower `WASM_MAX_GZIP_BYTES` to fit the new figure
      rather than leaving the raised ceiling in place. Note in §4.7 that the deferred
      trade was taken and why.

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
- [ ] **Retention.** An automatic log grows without bound, so it needs a rule, and the
      rule must be visible in the UI rather than a surprise:
      - **Starred entries are never pruned. Ever, by anything, automatically.**
      - Unstarred entries: keep the most recent 200, and drop unstarred entries older
        than 90 days. Both numbers belong in one exported constant with a comment.
      - Pruning happens on a schedule the user can see coming — show the count in the
        library header ("142 entries · 8 starred").
- [ ] **Quota.** Use `navigator.storage.estimate()` to show usage, and warn at 80 % of
      quota with a message that names Export as the remedy. Never silently drop an
      entry to make room. If a write genuinely fails, say so loudly, in a toast with an
      **Export library** action — losing the user's data quietly is the one outcome
      this feature must not have.
- [ ] Cap a single entry's `source` at the same 512 KB `MAX_STORED_DOC` uses, and say
      so when an entry is too large to log rather than logging it truncated.
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

## Completion

**Where the suggestions come from.**

- [ ] **techxt's own declared symbols**, through the wasm module (below).
- [ ] **The user's own definitions, the cheap way**: scan the input for
      `\newcommand`, `\renewcommand`, `\providecommand`, `\def`, `\DeclareMathOperator`
      and `\newenvironment` and take the names. ~20 lines, no library change, and it
      gets nearly all of the value. The exact route — having the binding call
      `language.parse()` itself and read techy's final parsing state — is explicitly
      **not** being taken: `Conversion` exposes only `text` and `diagnostics`, and the
      work is out of proportion to the difference. Mark these suggestions as coming
      from the document so they are distinguishable from the shipped ones.

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
- [ ] Export `Session.complete(prefix, limit)` returning a small array of
      `{ name, kind, replacement, arity }` — a binary search plus a prefix scan, so the
      answer is microseconds and the payload is a handful of entries.
- [ ] `src/worker/protocol.ts` grows `{ type: 'complete', id, prefix, limit }` and
      `{ type: 'completions', id, items }`, with the same monotonic-id discipline
      conversions use: a stale answer is dropped.
- [ ] **The one risk is head-of-line blocking** behind a slow conversion in the same
      worker. Conversions are debounced 120 ms and take 2–20 ms on ordinary documents,
      so there is normally a gap. Measure it on a 200 KB document. If it is laggy, add
      a small JS-side prefix→results cache before reaching for a second worker; a
      second wasm instance is a megabyte of memory for a nicety.
- [ ] Merge the document's own definitions into the answer on the JS side.

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

# Order of work

1. **Item 1** — independent, small, unblocks nothing but costs nothing.
2. **L1 + item 2** — the library change lands first with its own tests, then the
   binding, then the app. `opt-level = "s"` and its measurement belong to this item.
3. **Items 3 and 4 together** — they are one feature; the export format is easier to
   get right while the entry model is still being written.
4. **L2 + item 5** — the largest, and the one most likely to want its scope trimmed
   after the survey.

Every item edits `web/PLAN.md`; L1 and L2 also edit the root `PLAN.md`. vitest covers
pure logic only, so the library store, the import/export codec, the region→element
mapping and the completion matcher should all be written as pure functions that a test
can reach without a DOM.
