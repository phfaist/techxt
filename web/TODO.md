# techxt web — work in progress

The queue for the `techy-web-cooler` line of work. `PLAN.md` in this folder stays
normative for the app: anything below that contradicts it is a plan change to be
written there as part of the work, not a quiet exception to it.

Nothing here is started. Items are in the order they were asked for, which is roughly
increasing order of cost.

---

## 1. Wrap defaults to soft-wrap

- [ ] `DEFAULT_OPTIONS.wrap` becomes `'soft'` instead of `'fit'` (`src/state.ts`).
- [ ] Rename the control's entry: *Off, soft-wrapped* → **Soft** (`src/ui/controls.ts`,
      and the option model table in `PLAN.md` §5).
- [ ] Follow the default through everything that reads it: `pruneOptions` (absent
      means the default, so an existing user who is on *Fit* today has nothing
      stored), `resolveOptions`, `softWraps`, the share codec, and their tests in
      `test/state.test.ts` / `test/options.test.ts`.

## 2. Optional MathJax rendering — `Math: MathJax`

A fourth value of the *Math* control, beside Fancy (default), Plain and Source. The
formula is emitted **as source** and MathJax typesets it in the output pane, so a
document's structure and its light font styling can be previewed without paying for
text-mode math that reads badly.

- [ ] Ship MathJax locally — no CDN. The app's promise is that it makes no network
      request after it loads and works offline; a third-party script would break both.
      Lazy-load it only when the mode is selected, and keep it out of the precache.
- [ ] Delimit exactly what MathJax may typeset. Target:
      `Support mathjax math like this $a+b-c$ but not these \$3 and \$4 values.`
      must produce **one** formula, not a run from the first `$` to the third.
- [ ] Preferred route — markers from the converter: `render/math.rs::source_scope`
      already emits the formula's source as a single `Verbatim` / `InlineVerbatim`
      item, so wrapping it in a marker pair is a small change there, behind a new
      option, plumbed through `web/crate/src/options.rs` and
      `src/worker/protocol.ts`. Take this only if it stays a fistful of lines and
      the library tests it properly.
- [ ] Fallback route, explicitly accepted: emit plain source and let MathJax parse the
      whole output blob, accepting the occasional misparse around a literal `$`.
- [ ] Whichever route: Copy and Download keep handing over the library's own text.
      Markers are display-only, exactly as `wrap: 'soft'` is.

## 3. A library of saved documents

- [ ] A **Save to library** button beside Copy and Download in the output pane header.
- [ ] An item remembers: the source, the options in force when it was saved, a
      timestamp, and a title. Whether it also stores the rendered text is open — it is
      regenerable, but it makes a preview cheap and keeps a saved item readable when
      the converter's output later changes.
- [ ] Persist in IndexedDB (localStorage already holds the session and has a quota to
      protect).
- [ ] Open the library from somewhere meaningful — next to *More options*.
- [ ] The pane itself: a visual list of everything saved, source on the left and
      preview on the right, or whatever reads better. User-friendliness and being
      obvious at a glance are the point.
- [ ] Delete an individual item from within the pane.

## 4. Library import / export

- [ ] **Export**: the whole library as one JSON file.
- [ ] **Import**: read such a file back. Decide and state what happens to items that
      are already there — merge, replace, or skip.
- [ ] The format is versioned, and a corrupt or foreign file is refused rather than
      half-imported.

## 5. A better editor

- [ ] Survey what exists (CodeMirror 6 and friends) before writing anything; take a
      hand-rolled lightweight solution if it keeps the app leaner, which is the
      expectation.
- [ ] Minimal syntax highlighting of the input: commands, math, comments, braces.
- [ ] Autocompletion that stays out of the way — never steals a keystroke, never
      commits on its own.
- [ ] Suggestions come from techxt's own declared symbols. `Category` currently has
      no accessor over its `macros` / `environments` / `specials`, so exposing the
      name list needs a small addition to `rust/techxt` plus an export from
      `web/crate`.
- [ ] Also suggest the user's own definitions, if the final parsing state can be
      reached without much work; scanning the input for `\newcommand` and friends is
      the cheap approximation if it cannot.
- [ ] `PLAN.md` §1 and §16 currently name syntax highlighting and a code editor
      component as non-goals. This item changes that decision, so it changes those
      sections too.
