# techxt — implementation plan

`techxt` converts LaTeX-like markup to plain (unicode) text. It is built on the
[`techy`](https://github.com/phfaist/techy) parser (Rust) and is a from-scratch
redesign of the *capabilities* of
[`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc) (v3 beta) — pylatexenc
is an idea source and a porting reference, **not** a compatibility target. Where
pylatexenc has quirks, defects, or structural weaknesses, techxt deliberately deviates
and improves.

**This document is normative.** All design decisions were settled interactively with
the project owner; implement as specified and do not re-litigate. Only *mechanical*
details are left to the implementer: private helper structure, exact lifetime
parameters, file-internal organization, incidental naming of non-public items, and
faithful porting of data tables. Public type/module names, enum variants, defaults,
semantics, and rendering behaviors given below are binding (fields/enums marked
`#[non_exhaustive]` may gain variants later, but v1 ships exactly what is listed).
An implementing agent needs no context beyond this file plus read access to the
`techy` and `pylatexenc` repositories for looking up referenced code:

- techy @ `https://github.com/phfaist/techy` (main; workspace version 0.1.0)
- pylatexenc @ `https://github.com/phfaist/pylatexenc` (main; version 3.0beta2)

Line numbers cited below may drift; prefer symbol names.

---

## 1. Philosophy

1. **Correct text layout is a first-class concern.** pylatexenc's worst defects are
   layout defects (per-chunk wrapping that breaks at macro boundaries and destroys
   verbatim content; accidental blank-line counts around blocks). techxt separates
   *content conversion* from *layout*: handlers produce a typed "text flow", and a
   single layout engine renders it. No handler ever concatenates raw newlines to fake
   vertical spacing.
2. **One definition, both sides.** pylatexenc keeps two independent databases (parse
   argspecs vs. text rules) that diverge silently (dropped arguments, `IndexError`
   crashes). In techxt, one definition entry carries both the parsing argument
   structure and the text rule; parse and render cannot disagree.
3. **Nothing silent.** Unknown constructs, skipped content, and handler problems
   produce structured diagnostics with source positions.
4. **LaTeX-correct whitespace semantics, no compatibility knobs.**
5. **Reusable, immutable converter.** All per-document state lives in a per-run
   structure. The converter is `Clone + Send + Sync` and converts many documents
   concurrently.
6. **Payload-only reading.** techxt must work on trees that have been transformed
   (`techy::transform`) and no longer map 1:1 to source bytes. Handlers read node
   *payloads* only. Resolving a node's own `TextContent::Spanned` against the node's
   own source is permitted (treat it as optimized payload data). Never use
   `NodeRef::span_content()` / `NodeSlice::source_text()` or any inter-node span
   arithmetic to obtain content. To re-emit a subtree as LaTeX source (math Source
   mode, keep-source policies), use techy's payload-based
   `techy::latexlike::SourceRecomposer`. Node *spans* may be used for diagnostic
   positions only.
7. **techy-grade engineering from day one**: `missing_docs = deny`, denied broken doc
   links, clippy clean, rustfmt, no_std proof in CI, MSRV pinned. No panics on
   document input (contract violations only).

---

## 2. Repository and workspace layout

```
techxt/                        # repo root (language-neutral)
  README.md                    # short: what techxt is, sibling-folder layout
  PLAN.md                      # this file
  rust/                        # Cargo workspace root
    Cargo.toml                 # [workspace] resolver = "2", members = ["techxt", "techxt-cli"]
    techxt/                    # library crate: no_std + alloc
    techxt-cli/                # binary crate (std); [[bin]] name = "techxt"
  web/                         # browser app: wasm binding + static SPA (web/PLAN.md)
    crate/                     # the wasm-bindgen binding — standalone, not a member
  tools/                       # dev-only scripts (symbol-table generation, §12.4)
  # later, sibling root folders that do not talk to rust/'s build system:
  # python/  (maturin extension)   js/  (wasm/Node)
```

- Workspace config mirrors techy's: `resolver = "2"`, shared `[workspace.package]`
  (version `0.1.0`, edition `2021`, `rust-version = "1.86"`, license `MIT`, author
  Philippe Faist, repository URL), `[workspace.lints.rust] missing_docs = "deny"`,
  `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"`,
  `private_intra_doc_links = "warn"`, `bare_urls = "warn"`; release profile
  `lto = true`, `codegen-units = 1`. Edition/MSRV may be bumped (e.g. edition 2024)
  only if it brings significant code-clarity benefit; matching techy is the default.
- `techxt` lib: `#![cfg_attr(not(test), no_std)]` + `extern crate alloc`.
  Runtime dependencies: exactly `techy` (git dependency pinned to a specific rev of
  `https://github.com/phfaist/techy`; switch to a crates.io version once techy
  publishes) and `unicode-width`. Dev-dependency: `proptest`.
  - **Amended for M9:** the runtime set is exactly `techy`, `techy-xp` and
    `unicode-width`. `techy-xp` supplies `LatexlikeXp` and `XpDriver`, the language
    the converter parses through (§16 M9); it is a git dependency pinned like techy,
    and pinned to a rev that itself pins the *same* techy rev — cargo cannot unify
    two revs of one git dependency, so the two pins move in lockstep. Its own
    dependencies are techy and `hashbrown`, techy's map choice and already in the
    tree through techy, so the crate graph gains no node. Still no cargo features,
    and `techxt-cli` still names neither upstream crate.
- `techxt-cli`: depends on `techxt`, `clap` (derive), std.
- **No cargo features.** The definitions library is organized as Rust modules the
  user references explicitly; unreferenced modules are removed by dead-code
  elimination. No `serde` feature in v1.
- `web/` is a sibling root of the same kind as the planned `python/` and `js/`: it
  has its own build system and its own CI workflow, and the repository root has no
  `Cargo.toml`, so the package under `web/crate/` is standalone and cannot perturb
  `cd rust && cargo test`. Its normative design is [`web/PLAN.md`](web/PLAN.md).

---

## 3. What techy provides (API contract techxt builds on)

techxt uses the `techy::latexlike` preset, **concretely**: all techxt types are
non-generic over the language (`Latexlike`) and over the tree annotation (`A = ()`).
(Settled API decision — see §11.1. Generalization over `LatexlikeLang` is future
work.) techy facts to rely on (narrative guides: techy `docs/ai-guide*.md`):

**Parsing.**
```rust
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),      // techxt default: Tolerant
    ParsingState::lang_initial_with_packages(packages)?,
);
let result = language.parse(input)?;               // ParseResult { tree, diagnostics, .. }
```
`Language` is `Send + Sync`, built once, parses many documents. Tolerant parses return
a whole-input tree plus diagnostics; an unresolvable `\foo` becomes a literal `Chars`
node `"\foo "` plus an error diagnostic. `LatexlikeDriver::with_source_resolver(...)`
enables `\input` at *parse* time (resolved content appears as an `Attached` slot on
the `\input` node); `techy::source::SourceResolver` is the trait,
`check_include_chain` guards cycles. Keep the driver's default
`ParagraphBreakStyle::Chars` (paragraph breaks arrive as whitespace-only `Chars`
nodes containing a blank line).

**Node model** (`techy::core::node`). Five kinds:
`Chars { content }`, `Group(GroupData { group_type, open, close })`,
`Callable(CallableData { callable_type, name, spec, arguments, slots,
invocation_syntax })`, `Comment(CommentData)`, `List`.
No Math/Macro/Environment/Verbatim node kinds: math is
`GroupType::Math(MathGroupForm::{Inline,Display})`; macros/environments/specials are
`Callable` distinguished by `callable_type`; `\verb` is `GroupType::Verbatim` with one
raw `Chars` child; a `verbatim` environment is an environment whose body list holds
one raw `Chars` node. Latexlike sugar on `NodeRef`: `is_math_group()`, `math_form()`,
`macro_name()`, `environment_name()`, `specials_name()`, `post_space()`.
Arguments: `argument_content_nodes_named(name)` (absent optional → `Ok(None)`; wrong
name → `Err`), providedness via `ParsedArgument::is_provided()` (a `*` star argument's
providedness *is* the star test). Environment body via `body()`; `\input` content via
`slot_content_nodes_named("attached")`. Every node records its parsing state:
`node.parsing_state().mode()` is `techy::latexlike::Mode::{Text,Math}`.

**Recompose** (`techy::recompose`) — techxt's engine:
```rust
pub trait Recomposer<L: Lang, A> {
    type State;                 // downward-threaded context
    type Piece: ComposePiece;   // techxt: Flow (§6)
    type Error;
    fn recompose_node(&mut self, node: NodeRef<'_, L, A>, state: &Self::State,
                      cx: &mut RecomposeContext<'_, L, A>)
        -> Result<Recompose<Self::Piece, Self::State>, Self::Error>;
}
```
Instructions: `Recompose::Emit(piece)` or `Recompose::Concat(ConcatPieces::children()
.wrap(head, tail).join(sep).with_state(derived).include_attached())`.
Re-entrant region ops on `RecomposeContext` (always pass `self` back):
`recompose_argument_content_named`, `recompose_argument_content`, `recompose_body`,
`recompose_slot_content_named`. Absent argument → empty piece. Driver:
`TreeRecomposer::new(&mut r).recompose(&tree, initial_state)`. Fold order is document
order (enter order; eager region ops preserve it) — counters and footnote collection
in `&mut self` rely on this.
**Wrapping contract**: consumers extend techxt by wrapping its recomposer with their
own `Recomposer` that overrides some nodes and delegates the rest; techxt's
`TextRenderer` must be usable as such an inner recomposer (public, documented).
**Role rule**: `Concat` skips `Attached`/`Hidden` slots by default — the `\input`
handler renders the attached slot explicitly.

**Definitions** (`techy::core::specs`, `techy::latexlike`). techy ships **no**
standard LaTeX definitions (only `\begin`/`\end`); techxt owns the whole database.
Building blocks: `Package::new/insert/insert_specials` (+ `insert_in_modes` /
`insert_specials_in_modes` for mode-restricted entries like `^`/`_` and ligatures);
argspec codes via `techy::latexlike::argument_specs_from_str` /
`argument_specs_named` (codes `m/{`, `o/[`, `s/*`, `t<c>`, `r<c1><c2>`, `d<c1><c2>`,
`v`, `e{...}`, word codes `AnyDelimited`, `BracedOnly`); spec types
`MacroSpec::new(args)`, `EnvironmentSpec::new(args)` + `.with_body_delta(...)` (how
`equation` enters `Mode::Math`, how list bodies inject an `\item` package),
`SpecialsSpec`, `VerbatimBehavior` (verbatim-bodied environments),
`ArgumentSpec::with_state_delta` (how `\text{...}` leaves math). `CallableSpec` has an
`Any` supertrait → downcastable; that is the sanctioned identity mechanism techxt's
dispatch uses (§10.3). Register names **without** the escape char (`"emph"`).

**Diagnostics** (`techy::error`). techxt defines its own condition types with the
re-exported `DiagnosticInfo` derive and returns
`techy::error::Diagnostics<Option<String>>`. Use the node's `SourceSpan` for
positions.

---

## 4. What to take from pylatexenc v3 (and what to fix)

Porting references: `pylatexenc/latex2text/__init__.py` (spec classes, math engine
lines ≈1067–1954, list machinery ≈416–682), `pylatexenc/latex2text/_defaultspecs.py`,
`pylatexenc/latexwalker/_defaultspecs.py`, `pylatexenc/test/test_2_latex2text.py`.

**Adopt (redesigned into techxt's architecture):**
- The fancy math engine as the default mode: atom classes, plain-string segmentation,
  join rules, unary-minus reclassification, script handling, sub/superscript unicode
  tables, wrappable pieces, `math_expression_in` delimiters, `\sqrt` → `√`/`∛`/`∜`.
  techxt's mode *set* is redesigned: exactly `Fancy | Plain | Source` (§9.5).
- Unicode font alphabets with **separate math and text style stacks** (§9.5).
- List rendering: per-depth markers, same-kind depth counting, `\item[label]` doesn't
  advance the counter, hanging indents (§9.4).
- Accent tables (`unicode_accents_list`) — but apply the combining char to the
  **first base character only** (fixes v3 stamping accents onto whole arguments).
- The symbol tables via a generation script (§12.4).
- `\href`/`\url` with verbatim-parsed URL arguments.
- The *inventory* of `test_2_latex2text.py` as a coverage checklist with techxt's own
  expected outputs.

**Fix / deliberately deviate (all settled):** one unified definitions database; named
argument access everywhere; diagnostics for unknown constructs; whole-paragraph
layout (verbatim never wrapped/styled/normalized); a real block model with one
normalization policy; verbatim renders its content; immutable reusable converter;
parse-time `\input` with cycle guard; display width via `unicode-width`; numbered +
underlined sectioning without uppercasing; collected footnotes; basic aligned
tables; `\today` supplied by the embedder. **Dropped entirely:**
`strict_latex_spaces` (all knobs/presets), `keep_braced_groups`, v1/v2 compat shims,
`latex-ethuebung` + `nonstandard-qit` categories, `%`-style replacement strings,
callable introspection by parameter name, v3's `text`/`with-delimiters`/`remove`
math modes.

---

## 5. Architecture and module map (normative)

```
 input &str ──► techy Language::parse ──► NodeTree + parse Diagnostics
                                              │
        NodeTree (possibly user-transformed) ──► TextRenderer (impl techy Recomposer)
                                              │     downward RenderState
                                              │     &mut per-run state (counters, footnotes, diags)
                                              ▼
                                         Flow (typed token sequence)
                                              │
                                         layout engine
                                              ▼
                                    String + Diagnostics
```

All three layers are public, documented API: convenience (string → string), tree
(convert an existing `NodeTree`), and flow/layout (flow tokens, layout engine,
`TextRenderer`). Public enums/structs that may grow are `#[non_exhaustive]`.

Module map for `rust/techxt/src/` (public paths are binding; file layout inside a
module is mechanical):

```
lib.rs         — crate docs; re-export Converter, ConverterBuilder, Options, Conversion
convert.rs     — techxt::convert: Converter, ConverterBuilder, BuildError, Options + option enums, Conversion
flow.rs        — techxt::flow: Flow, FlowItem, BlockKind
layout.rs      — techxt::layout: LayoutOptions, render, render_to, render_inline
render/        — techxt::render: TextRenderer, RenderState (+ MathCtx, ListCtx, FloatKind),
                 RenderCx, RenderError, RenderFinish
def/           — techxt::def: MacroDef, EnvDef, SpecialsDef, Category, DefinitionSet,
                 CallableKind, TextRule, Template, TemplateError, TextHandler,
                 TechxtMacroSpec, TechxtEnvironmentSpec, TechxtSpecialsSpec
mathfmt/       — techxt::mathfmt: Atom, AtomClass, MathBox, join_atoms, segment_plain,
                 script/alphabet tables
defs/          — techxt::defs: one module per category (§12.1) + defs::standard()
diag.rs        — techxt::diag: condition types (§10.6)
```

---

## 6. The flow model (`techxt::flow`)

```rust
/// The recomposer's piece monoid. Newtype over Vec: append = extend (amortized O(1) per item).
#[derive(Clone, Debug, Default)]
pub struct Flow(Vec<FlowItem>);

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum FlowItem {
    /// Non-whitespace text. ADJACENT Text items are glued: no break may occur
    /// between them (`\textbf{bold}text` never wraps between "bold" and "text").
    Text(Box<str>),
    /// One collapsible inter-word space; the only place wrapping may break.
    Glue,
    /// Forced line break.
    HardBreak,
    /// Paragraph separator. Layout normalizes runs of these (and block boundaries)
    /// to at most one blank line.
    ParagraphBreak,
    /// Preformatted block: emitted line-by-line with the current continuation
    /// indent; NEVER wrapped, styled, or whitespace-normalized.
    Verbatim(Box<str>),
    /// Inline preformatted fragment (`\verb`): an unbreakable word, raw contents.
    InlineVerbatim(Box<str>),
    /// Open a block context; implies paragraph-level separation from surroundings.
    BlockStart(BlockKind),
    BlockEnd,
    /// Table/matrix markers, consumed by the enclosing table/matrix handler BEFORE
    /// layout. Layout must not see them in correct operation; release-mode fallback
    /// if it does: CellSep → Glue, RowSep → HardBreak, RuleMark → HardBreak
    /// (plus debug_assert!(false) in debug builds).
    CellSep,
    RowSep,
    RuleMark,
    /// Math atom — exists only transiently inside math subtrees; resolved by the
    /// math-group handler (§9.5). Layout fallback: emit the atom's flattened text.
    MathAtom(crate::mathfmt::Atom),
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum BlockKind {
    /// Hanging-indent block: first-line prefix + continuation prefix. Used for
    /// display math, quotes, footnote entries, placeholder blocks.
    Indent { first: Box<str>, cont: Box<str> },
    /// A list-item block. A new Item at the same nesting depth implicitly closes
    /// the previous one (auto-close in layout), so `\item` needs no lookahead.
    Item { first: Box<str>, cont: Box<str> },
}
```

API (complete): `Flow::new()`, `push(FlowItem)`, `extend(Flow)`,
`text(&str) -> Flow` (one `Text` item, no splitting),
`from_plain_text(&str) -> Flow` (split on whitespace: words → `Text`, whitespace runs
→ `Glue`, whitespace runs containing ≥ 2 newlines → `ParagraphBreak`),
`glue() -> Flow`, `items(&self) -> &[FlowItem]`, `into_items(self) -> Vec<FlowItem>`,
`is_empty()`. Implement `techy ComposePiece` for `Flow`. No construction macro.

Display width: one internal function `display_width(&str) -> usize` via
`unicode_width::UnicodeWidthStr` — the only place width is computed (layout, tables,
matrices, underlines, maketitle rule).

---

## 7. The layout engine (`techxt::layout`)

```rust
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    /// None (default) = no wrapping: glue renders as a single space.
    pub wrap_width: Option<usize>,
}
pub fn render(flow: &Flow, opts: &LayoutOptions) -> String;
pub fn render_to(flow: &Flow, opts: &LayoutOptions, out: &mut dyn core::fmt::Write)
    -> core::fmt::Result;
/// Single-line rendering for table/matrix cells, heading width measurement,
/// maketitle lines: Glue/HardBreak/ParagraphBreak → one space, block prefixes
/// ignored, verbatim contents inserted raw with internal newlines → space,
/// result trimmed.
pub fn render_inline(flow: &Flow) -> String;
```

Algorithm (single pass; state: indent stack of `(first, cont)` prefixes, current
column, pending-glue flag, pending-vertical-separation flag):

1. **Words**: adjacent `Text` items concatenate into one unbreakable word. Emitting a
   word: if pending glue and `wrap_width` is set and
   `col + 1 + display_width(word) > wrap_width`, emit newline + continuation indent
   instead of the space; otherwise emit the space (if pending glue) then the word.
   A word wider than the available width on a fresh line overflows (never split).
2. **Glue** collapses: consecutive `Glue` = one potential break/space. Glue at line
   start/end is dropped — no trailing spaces, ever.
3. **Vertical separation** is normalized here and only here: `ParagraphBreak`,
   `BlockStart`, `BlockEnd` all *request* separation; consecutive requests merge
   (never sum). Result: at most one blank line between content lines; no
   leading/trailing blank lines; output ends with exactly one `\n` if nonempty.
4. **Blocks**: `BlockStart` pushes `(first, cont)`; the first content line of the
   block is prefixed with all enclosing `cont`s + this block's `first`; subsequent
   lines use `cont`. Prefix widths count toward the wrap column. `Item` auto-closes
   a previous open `Item` at the same depth.
5. **Verbatim**: each line emitted raw, prefixed by the current continuation indent
   only; never wrapped or trimmed; block-level separation before and after.
   `InlineVerbatim` behaves as an unbreakable word emitted raw.
6. **No-wrap path**: same single pass with the width checks skipped (glue → one
   space). Shared code; do not fork the engine.

proptest invariants (§14): no line exceeds `wrap_width` unless it contains one
oversized word or verbatim; no trailing whitespace on any line; never two consecutive
blank lines; `Verbatim` payloads appear byte-identical; deterministic output.

---

## 8. Render state

**Downward state** (`Recomposer::State`, `Clone`, derived via
`ConcatPieces::with_state` or region-op parameters, auto-restored):

```rust
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RenderState {
    pub math: Option<MathCtx>,          // None = text mode
    pub text_font: FontStyle,           // default FontStyle::Default (upright)
    pub math_font: FontStyle,           // default FontStyle::Style(Italic)
    pub table: Option<TableCtx>,        // & → CellSep, \\ → RowSep when Some
    pub list: Option<ListCtx>,          // innermost list: kind + same-kind depth + total depth
    pub float: Option<FloatKind>,       // Figure | Table — for \caption prefix
}
#[derive(Clone, Debug)]
pub struct MathCtx { pub display: bool, pub matrix: bool }
#[derive(Clone, Debug)]
pub struct ListCtx { pub kind: ListKind, pub same_kind_depth: usize, pub depth: usize }
#[derive(Clone, Copy, Debug)] pub enum ListKind { Itemize, Enumerate, Description }
#[derive(Clone, Copy, Debug)] pub enum FloatKind { Figure, Table }
#[derive(Clone, Copy, Debug)] pub struct TableCtx;   // marker; fields may come later
```

Initial state: all `None`, fonts as defaulted from `Options`. Math mode is entered by
the math-group / math-environment handlers (`math: Some(MathCtx { .. })`), never
inferred from `node.parsing_state()` (options control rendering); parsing state is
still what makes `^`/`_` parse only in math.

**Per-run state** (fields of `TextRenderer`; one `TextRenderer` per conversion — the
public `Converter` stays immutable):
diagnostics collector, `heading_counters: [u32; 7]`, `chapter_seen: bool`,
`list_counter_stack: Vec<u32>` (pushed/popped by list-env handlers around
`recompose_body`), `footnotes: Vec<Flow>`, `doc_title/author/date: Option<Flow>`.
Fold order is document order, so `&mut self` counters are correct.

---

## 9. Conversion semantics (normative behavior)

### 9.1 Node-kind dispatch (`TextRenderer::recompose_node`)

- `Chars`:
  - In a verbatim context (parent group has `GroupType::Verbatim`, or the enclosing
    environment's techxt spec is flagged verbatim-body): `InlineVerbatim` (from
    `\verb`) or `Verbatim` (environment body), contents untouched.
  - In math (per `RenderState::math`, modes Fancy/Plain): strip all whitespace; in
    Fancy, segment into `MathAtom`s (§9.5); in Plain, emit as `Text` (after font
    mapping), no atoms.
  - Otherwise: `flow::from_plain_text`, with the active text font applied to letters
    (§9.5) before emission. A whitespace-only run containing a blank line →
    `ParagraphBreak` (this is how techy's paragraph-break Chars nodes render).
- `Comment`: nothing at all (not even its trailing newline — LaTeX-correct). With
  `Options::keep_comments = true`: block-separated own line `%<comment text>`.
- `Group`: math group → §9.5. Verbatim group → `InlineVerbatim` of its raw child.
  Any other group → transparent `Concat(children)` (no braces in output).
- `List` → `Concat(children)`.
- `Callable` → rule dispatch (§10.3) then rule execution (§10.4).

Fixed whitespace policy (no options): macro post-space is invocation syntax, never
emitted; source whitespace collapses to glue; paragraph breaks normalize to one blank
line; math ignores source whitespace; verbatim preserves everything.

### 9.2 Sectioning (defs::sectioning)

All seven commands parse as `s o m` named `("star","toctitle","title")`; `toctitle`
is accepted and ignored. Levels: 0 part, 1 chapter, 2 section, 3 subsection,
4 subsubsection, 5 paragraph, 6 subparagraph.

Numbering (when the style says numbered): incrementing level L sets
`counters[L] += 1` and zeroes all deeper counters. `part` has its own counter
rendered as uppercase Roman and resets nothing. The dotted number for levels 1–4
joins `counters[first..=L]` with `.`, where `first = 1` if any `\chapter` has
occurred in this run, else `2`. Starred forms neither increment nor display a number.
Levels 5–6 are never numbered.

Rendering by `Options::heading_style` (default `NumberedUnderlined`); no uppercasing
anywhere:

| level | NumberedUnderlined | Underlined | Prefix | Plain |
|---|---|---|---|---|
| part | `Part II: Title` underlined `=` | `Title` underlined `=` | `Part: Title` | `Title` |
| chapter | `3 Title` underlined `=` | `Title` underlined `=` | `Chapter: Title` | `Title` |
| section | `3.1 Title` underlined `-` | underlined `-` | `§ Title` | `Title` |
| subsection | `3.1.2 Title` underlined `~` | underlined `~` | `§.§ Title` | `Title` |
| subsubsection | `3.1.2.1 Title` plain line | plain line | `§.§.§ Title` | `Title` |
| paragraph / subparagraph | unnumbered plain line | plain line | plain line | `Title` |

Underline = the underline char repeated to `display_width` of the heading line
(measure via `layout::render_inline`). Headings are block-separated (blank line
before and after, normalized by layout).

### 9.3 Escapes, spacing, ligatures, accents (defs::base, defs::accents)

`\{ \} \$ \& \# \_ \%` → literal char; `\~{}`-family via accents. `\,` `\;` `\:`
`\ ` (control space) and escaped newline → one glue; `\!` → nothing; `\quad` →
`Text("  ")`, `\qquad` → `Text("    ")`. `~` → `Text("\u{00A0}")` (unbreakable by
construction). Ligature specials (text mode only): `` ` `` `'` → ‘ ’, ``` `` ``` `''`
→ “ ”, `--` → –, `---` → —, `` !` `` → ¡, `` ?` `` → ¿. `\\`: see §9.7.
`\par` → `ParagraphBreak`. Accents: combining char appended to the **first base
character** of the rendered argument (rest unchanged); dotless ı/ȷ → i/j first;
compose the (base, combining) pair to its NFC form via a generated static table
(§12.4); empty argument → the standalone spacing form if the table has one, else the
combining char after a space.

### 9.4 Lists (defs::lists)

Environments `itemize`, `enumerate`, `description`, `list`, `trivlist` (+ accept and
ignore the `enumitem` optional argument, with an `unsupported-ignored` diagnostic if
provided). Env handler: derive `ListCtx { kind, same_kind_depth = 1 + count of
enclosing lists of the same kind, depth = 1 + enclosing depth }`; push `0` onto the
run counter stack; `cx.recompose_body(...)`; pop; if `depth == 1`, wrap the result in
`BlockStart(Indent { first: "  ", cont: "  " })`/`BlockEnd`; else emit as-is.

`\item` (parse spec `o` named `label`; defined via the list environments' body delta
package, plus a top-level fallback definition): handler reads `ListCtx` + counter
stack and emits `BlockStart(Item { first, cont })` — auto-closed by the next
item/blockend. Marker selection by `same_kind_depth` d (1-based, cycling through the
configured arrays):
- itemize (default `["•", "–", "*", "·"]`): `first = marker + " "`.
- enumerate (default formats `1.` `(a)` `i.` `A.` — arabic/dot, lower-alpha/parens,
  lower-roman/dot, upper-alpha/dot): increment top-of-stack counter, format,
  `first = formatted + " "`.
- description: `first = render_inline(label) + "  "` (label absent → `"  "`).
- Explicit `\item[label]` in any list kind uses the label verbatim and does **not**
  advance the counter.
`cont` = spaces of `display_width(first)`. A stray `\item` outside any list: warning
diagnostic `techxt.stray-item` + rendered as `first = "- "`.

```rust
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ListStyle {
    pub itemize_markers: Vec<Box<str>>,          // default as above; cycles
    pub enumerate_formats: Vec<EnumFormat>,      // default as above; cycles
}
#[derive(Clone, Copy, Debug)]
pub struct EnumFormat { pub style: CounterStyle, pub wrap: CounterWrap }
#[derive(Clone, Copy, Debug)]
pub enum CounterStyle { Arabic, LowerAlpha, UpperAlpha, LowerRoman, UpperRoman }
#[derive(Clone, Copy, Debug)]
pub enum CounterWrap { Dot, Parens }             // "1." vs "(a)"
```

### 9.5 Math (`techxt::mathfmt` + defs::mathcore/mathenvs/subsuperscripts)

`MathMode` (exactly three): **`Fancy`** (default), **`Plain`** (same database-driven
conversion — unicode symbols, scripts with unicode-or-fallback incl.
`math_expression_in` wrapping, font alphabets — source whitespace ignored, but no
atom joiner: outputs concatenate directly), **`Source`** (re-emit the math subtree as
LaTeX via `SourceRecomposer`: inline → `InlineVerbatim`, display → `Verbatim` block).
Display = `math_form() == Display` or a math environment; display output in
Fancy/Plain is wrapped in `BlockStart(Indent { first: "    ", cont: "    " })`.

**Fonts (full model, separate stacks):** `text_font` and `math_font` in
`RenderState`, derived independently by style macros; nesting composes and restores
automatically. Three-valued:
```rust
#[derive(Clone, Copy, Debug)]
pub enum FontStyle { Disabled, Default, Style(FontStyleKind) }
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum FontStyleKind { Bold, Italic, BoldItalic, Script, BoldScript, Fraktur,
    BoldFraktur, DoubleStruck, SansSerif, SansSerifBold, SansSerifItalic,
    SansSerifBoldItalic, Monospace }
```
`Disabled` disables alphabet mapping and stays disabled through style macros.
`Default` = upright in text, and in math means "use `Options::math_font`". Mapping
applies to ASCII letters only, at chars leaves, via the offsets + exception tables
ported from v3 (`_fmt_math_style_offsets`, `_fmt_math_style_exceptions`). `\emph`
toggles italic relative to the enclosing text style; `\text{}`/`\mbox{}` switch mode
only; math font macros in text mode set the text style.

**Fancy engine** (port from v3, `latex2text/__init__.py` ≈1067–1954):
```rust
#[derive(Clone, Debug)]
pub struct Atom { pub body: AtomBody, pub cls: (AtomClass, AtomClass),
                  pub script: Option<ScriptInfo> }
#[derive(Clone, Debug)]
pub enum AtomBody { Str(Box<str>),
    Wrappable { inner: Box<str>, prefix: Box<str> },   // decides wrapping at join time
    Block(MathBox) }                                   // matrices
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomClass { Ord, Op, Bin, Rel, Open, Close, Punct, OpenEnd, Text, Script, Block }
#[derive(Clone, Debug)]
pub struct MathBox { pub lines: Vec<Box<str>>, pub baseline: usize }
pub fn segment_plain(s: &str, upright_letters_are_op: bool) -> Vec<Atom>;
pub fn join_atoms(atoms: Vec<Atom>, display: bool) -> MathBox;
```
Port faithfully: segmentation (`_segment_plain_str` — ≥2 upright latin letters → one
`Op`; digit runs with one decimal point → one `Ord`; the explicit bin/rel/open/close/
punct/op char tables; everything else per-char `Ord`; `|` and `/` unclassified);
the joiner (`_join_math_pieces` — realize against neighbors, drop empties,
unary-minus reclassification, script class propagation, the seven spacing rules of
`_math_needs_space`, no doubled spaces, scripts attach spaceless with retroactive
space before a `^`-notation fallback's base, superscript-then-subscript swapped so
the subscript binds first); wrappables (`needs_wrapping`: second script on the same
base, contains a space, or open-ended contents; wrapped in `Options::
math_expression_in` delimiters — `Parens` default, `Braces`, `Custom(open, close)`,
`None`). Rule outputs (Literal/Template strings) produced inside math are run through
`segment_plain`, exactly as v3 segments `simplify_repl` output.

Sub/superscripts: `^`/`_` registered as specials with one expression argument, math
mode only (text mode: literal chars — mode-restricted registration). Renderer ladder:
(1) unicode script chars (port `_fmt_superscript_chars`/`_fmt_subscript_chars` and
the math-alphabet inverse normalization); (2) retry with joiner-inserted spaces
stripped; (3) fallback `^(…)`/`_(…)` wrappable with `script_latex_notation` behavior.
`\frac` → wrappable `num/den`; `\sqrt` (arg `o` named `index`, `m` named `radicand`)
→ `√`, index `3` → `∛`, `4` → `∜`, other index rendered before the radical;
`\operatorname` + operator-name macros → `Op` literals.

**Math environments** (`equation(*)`, `align(*)`, `gather(*)`, `multline(*)`,
`eqnarray(*)`, `split`, `dmath(*)`; `subequations` renders its body): math body via
techy body delta; rendered as display math. Inside them `\\` → `HardBreak` (new
display line), `&` → nothing (the joiner spaces relations).

**Matrices** (`matrix/pmatrix/bmatrix/Bmatrix/vmatrix/Vmatrix/smallmatrix` +
`array` with its colspec argument ignored): body folded with `MathCtx.matrix = true`
(there `&` → `CellSep`, `\\` → `RowSep`); split at separators into rows × cells; each
cell joined via the math pipeline and rendered single-line; columns right-justified
to per-column width (`display_width`); cells joined by two spaces.
- **Inline math** → single-line `Str` atom: `[ <row>; <row> ]` with the env's plain
  delimiter pair (`matrix` has none: `<row>; <row>`), atom class `(Open→…→Close)` /
  `Block` semantics as in v3.
- **Display math** → `Block(MathBox)`: one line per row; delimiters per
  `Options::matrix_delimiters` — **`Unicode` (default)**: multi-row pieces
  (parens `⎛⎜⎝`/`⎞⎟⎠`, brackets `⎡⎢⎣`/`⎤⎥⎦`, braces `⎧⎨⎩`/`⎫⎬⎭`, vert `│`,
  double-vert `‖`; one-row matrices use the plain chars); **`Ascii`**: the plain
  delimiter char repeated on every row. `MathBox.baseline = (rows - 1) / 2`.
- Multi-row boxes compose with surrounding display content by a **2D baseline
  join**: pad every piece to the common height, align baselines, concatenate
  row-wise; joiner spacing applies on the baseline row, other rows get spaces of the
  same width. Only matrices produce multi-row boxes in v1.
- Empty matrix → `[ ]` (no panic).

**Flow boundary:** the math-group / math-env handler collects the folded atoms, runs
`join_atoms`, and converts the result: inline single-line → `Text`/`Glue` split at
the joiner's spaces (long formulas may wrap there); display → the box's lines emitted
as `Verbatim` lines inside the display indent block (wrap never touches them).
`MathAtom` items never escape to layout.

### 9.6 Tables (defs::tables)

`tabular`/`tabular*`/`tabularx` (leading width args parsed and ignored; colspec arg
`m` named `colspec`): handler sets `table: Some(TableCtx)` for the body fold; splits
the resulting flow at `CellSep`/`RowSep`/`RuleMark` into rows/cells/rules; renders
cells via `layout::render_inline`; alignment letters parsed from the colspec's plain
text — `l`/`c`/`r` honored (`c`: centered by padding, left-biased on odd), `p{…}`/
`X` → `l`, `|`/`@{…}`/`!{…}`/`>{…}`/`<{…}` ignored; missing/extra letters default
`l`. Columns padded to per-column width, joined by two spaces; `\hline`/`RuleMark`
→ one line of `-` repeated to the full table width; rows are `HardBreak`-separated
lines inside a block. `\multicolumn{n}{a}{content}`: render `content` in its cell,
ignore the span, one `unsupported-ignored` diagnostic per table. `\cline{range}`:
treated as `\hline`.

### 9.7 Context-dependent specials (`&`, `\\`, `\hline`)

| context | `&` | `\\` |
|---|---|---|
| table body (`table.is_some()`) | `CellSep` | `RowSep` |
| math, `matrix == true` | `CellSep` | `RowSep` |
| math, non-matrix (align etc.) | nothing | `HardBreak` (display) / `Glue` (inline) |
| text, no table | `Glue` + warning `techxt.misplaced-alignment` | `HardBreak` |

`\\` parse spec: star (`s` named `star`) plus, **iff** techy's optional-group parser
supports refusing pre-space (check `techy::core::constructs::
OptionalGroupArgumentParser` for such a knob), an optional `[len]` argument parsed
with that knob and ignored; if techy has no such knob, `\\` takes the star only
(documented limitation — this mirrors why v3 needed `optional_arg_no_space`).
`\hline`: `RuleMark` in table context; elsewhere skip + `unsupported-ignored`.

### 9.8 Everything else (defaults; all in the defs library)

- **Footnotes** (`FootnoteStyle`, default `Collected`): `\footnote` (args `o` named
  `mark`, `m` named `note`) emits `Text("[n]")` directly adjacent to preceding
  content (no glue before), n = 1-based collection index; note flow is collected.
  After the root fold, if any notes: `ParagraphBreak`, a line `---`, then each note
  as `Item { first: "[n] ", cont: spaces }`. `Inline` variant: `[` note `]` at the
  call site. `Skip`: nothing.
- **Refs/citations**: `\ref`/`\autoref`/`\cref`/`\vref`/`\pageref` → `<ref>`;
  `\Cref` → `<Ref>`; `\eqref` → `(<ref>)`; `\cite`/`\citet`/`\citep` + natbib
  variants → `<cit.>`; `\label` → nothing (arg parsed, discarded).
- **Links**: `\url` (verbatim arg `url`) → `<url>`; `\href{url}{text}` (url arg
  verbatim-parsed) → `text <url>`.
- **Graphics**: `\includegraphics` (`o`,`m`) → placeholder block
  `< g r a p h i c s >` as `Indent { first: "    ", cont: "    " }` (spaced-out
  letters, per v3's search-index rationale).
- **Titling**: `\title`/`\author`/`\date` store rendered flow in run state (arg
  content rendered at definition site); `\maketitle` emits, block-separated:
  title line, `    ` + author line, `    ` + date line, then `=` repeated to the max
  `display_width` of the three lines. Missing title/author → `<no title>` /
  `<no author>`; missing date → `\today` resolution. `\today` → `Options::today`
  string if set, else `<today>`.
- **`\input`/`\include`** (`m` named `filename`, chars-parsed): render the
  `attached` slot; slot absent → nothing + `techxt.input-not-resolved` (note
  severity).
- **Verbatim** (`verbatim(*)`, `lstlisting` with its `o` options arg ignored,
  `alltt`): `VerbatimBehavior` parse-side; body → `Verbatim` block. `\verb` →
  `InlineVerbatim`.
- **Blocks**: `center`/`flushleft`/`flushright` → body as its own block, no indent
  (no centering simulation); `quote`/`quotation`/`verse` →
  `Indent { first: "    ", cont: "    " }`; `abstract` → line `Abstract` (plain,
  block-separated) then body in a 4-space indent block; `figure(*)`/`table(*)` →
  body rendered, `float` state set to `Figure`/`Table`; `\caption` → own paragraph
  `Figure: <content>` / `Table: <content>` / `Caption: <content>` (by `float`
  state); theorem environments (`theorem`, `proposition`, `lemma`, `corollary`,
  `definition`, `conjecture`, `remark`, `example`, `proof` and short aliases `thm`,
  `prop`, `lem`, `cor`, `defn`, `rem`; all with `o` named `note`) → own paragraph
  starting `Theorem. ` or `Theorem (note). ` (capitalized full word from a fixed
  alias table; `proof` ends with an appended ` □`), body inline after the label.
- **Misc**: `\texorpdfstring{tex}{pdf}` → first argument; `\phantom`/`\hspace` →
  nothing; `\vspace` → `ParagraphBreak`; `\mbox`/`\text` → argument in text mode;
  `\ensuremath` → argument in math mode; over/under decorations (`\overline`,
  `\underline`, `\widehat`, arrow/brace/bracket variants — the v3 list) → argument
  unchanged; `\smallskip`/`\medskip`/`\bigskip` → `ParagraphBreak`;
  `\noindent`/`\centering`/`\raggedright`/`\raggedleft` → nothing.
- **Preamble** (parse + discard so arguments never leak): `\documentclass`,
  `\usepackage`, `\newcommand`/`\renewcommand`/`\providecommand`/`\def`-lite
  (`\newcommand` parse spec `s m o o m`), `\newenvironment`(+`s`),
  `\(re)newtheorem`, `\setlength`, `\addtolength`, `\pagestyle`, `\thispagestyle`,
  `\hypersetup`, `\graphicspath`, `\bibliographystyle`, `\bibliography` →
  arguments consumed, rule `Skip` (no diagnostic — they are *known*).

  **Amended for M9 (phase 2):** the *defining* commands of that list are no longer
  parse-and-discard. `\newcommand`, `\renewcommand`, `\providecommand`,
  `\newenvironment`, `\renewenvironment` and `\def` — plus `\gdef`, `\let`, `\edef`,
  `\xdef` and `\NewDocumentCommand`, which this list never had — are registered as
  techy-xp's definer specs through the `CallableSpecSource` seam, so each parses its
  invocation the way *it* defines and installs a macro the reader expands. The rule
  stays `Skip` (a definition contributes no text) and no diagnostic is raised, so the
  §9.8 promise above is unchanged; the shapes listed here are what
  `ConverterBuilder::macro_definitions(MacroDefinitions::Declared)` falls back to.
  Every definer is registered with `RedefinitionRule::Always`: techxt's
  unknown-command catch-all (§10.6) resolves every name, so "is this name new?" cannot
  be answered on this stack. `\DeclareMathOperator` stays a declaration — techy-xp
  ships no definer for it. techy-xp's `refusals_package` is seeded below every
  category, so `\expandafter` and the conditionals are diagnosed by name while
  `\setcounter` and the other names this list declares keep techxt's silent answer.

---

## 10. Definitions model (`techxt::def`)

### 10.1 Entries and builders

```rust
pub struct MacroDef { .. }     // + EnvDef, SpecialsDef
impl MacroDef {
    pub fn new(name: impl Into<Box<str>>) -> Self;
    pub fn symbol(name: impl Into<Box<str>>, replacement: impl Into<Box<str>>) -> Self; // 0 args + Literal
    pub fn arg(self, code: &str, name: &str) -> Self;   // pylatexenc code + REQUIRED name
    pub fn star(self) -> Self;                          // = .arg("s", "star")
    pub fn rule(self, rule: TextRule) -> Self;
    pub fn text_mode_only(self) -> Self / pub fn math_mode_only(self) -> Self;
}
impl EnvDef {
    pub fn new(name) / arg(code, name) / rule(rule);
    pub fn math_body(self) -> Self;                     // body delta → Mode::Math
    pub fn verbatim_body(self) -> Self;                 // VerbatimBehavior
    pub fn list_body(self, kind: ListKind) -> Self;     // body delta injects the \item package
}
impl SpecialsDef { pub fn new(chars) / arg(code, name) / rule(rule) / text_mode_only / math_mode_only; }
```
Every argument in techxt's own database is named. `Category::new(name)` +
`add_macro/add_env/add_specials` (and `with_*` chaining variants);
`DefinitionSet::new()` + `push(Category)`.

### 10.2 Building

`DefinitionSet` builds (at `ConverterBuilder::build()` time): one techy `Package` per
category pushed in list order — techy resolution is innermost-first, so **later
categories shadow earlier ones** (document prominently; this inverts pylatexenc);
the parsing state (`ParsingState::lang_initial_with_packages`); and the name-keyed
fallback rule table. All validation happens here and surfaces as `BuildError`
(`#[non_exhaustive]`): `Template(TemplateError)` (with definition name + offending
template), `ArgCode(...)` (techy `ArgumentCodeError` passthrough),
`DuplicateCategory(name)`, `State(...)` (techy `FinalizeError`). Spec objects:

```rust
pub struct TechxtMacroSpec { /* args, rule: TextRule, .. */ }   // + Environment, Specials
```
implementing techy `CallableSpec<Latexlike>` (delegate argument parsing to techy's
standard machinery; the `Any` supertrait exposes the embedded rule to dispatch).
`TechxtEnvironmentSpec` records body kind (normal / math / verbatim / list(kind)) so
render-side detection (§9.1, §9.4) never guesses.

### 10.3 Rule dispatch (per callable node, in order)

1. **Override map** — `ConverterBuilder::override_macro/environment/specials(name,
   TextRule)`, keyed `(CallableKind, name)` where
   `pub enum CallableKind { Macro, Environment, Specials }`.
2. **Embedded rule** — downcast `node.spec()` to a techxt spec type.
3. **Name fallback table** — from the `DefinitionSet` (covers foreign/plain-techy
   trees).
4. **Unknown policy** (§10.5).

### 10.4 `TextRule` and execution

```rust
#[non_exhaustive]
pub enum TextRule {
    Literal(Cow<'static, str>),
    Template(Template),
    Skip,
    Content,
    Handler(Arc<dyn TextHandler>),
}
pub trait TextHandler: Send + Sync + core::fmt::Debug {
    fn render(&self, node: NodeRef<'_, Latexlike>, cx: &mut RenderCx<'_, '_>)
        -> Result<Flow, RenderError>;
}
```
Execution semantics: `Literal` → `flow::from_plain_text` (segmented into math atoms
when in Fancy math). `Template` → substitute (below), same text treatment.
`Skip` → empty. `Content` → macros: provided arguments' content rendered in
declaration order, concatenated with nothing inserted; environments: the body;
specials: the trigger characters as text. `Handler` → call; on `Err`, emit
`techxt.handler-failed` (error severity) and render nothing (conversion continues).

`RenderCx` public methods (complete):
`arg(name) -> Result<Option<Flow>, RenderError>`, `arg_at(index) -> ...`,
`arg_provided(name) -> bool`, `arg_text(name) -> Result<Option<String>, RenderError>`
(= `render_inline` of the arg's flow), `arg_with_state(name, RenderState)`,
`body()`, `body_with_state(RenderState)`, `attached() -> Result<Option<Flow>, _>`,
`state() -> &RenderState`, `options() -> &Options`,
`source_of(node) -> Result<String, RenderError>` (via `SourceRecomposer`),
`diag(Diagnostic<Option<String>>)`, `set_doc_title/author/date(Flow)`,
`doc_title/author/date() -> Option<&Flow>`, `push_footnote(Flow) -> usize`.
Crate-internal handlers may use additional `pub(crate)` accessors (heading counters,
list counter stack) — mechanical.

`RenderError` (`#[non_exhaustive]`): `Region { detail }` (wraps techy
`RecomposeError` variants from region ops), `Handler { construct, detail }`.
`TextRenderer`'s `Recomposer::Error = core::convert::Infallible`: all rule/handler
errors are converted to diagnostics inside `recompose_node` (render nothing,
continue). The only fold abort is techy's descent limit; conversions map it to a
`techxt.render-aborted` error diagnostic with empty text output.

### 10.5 Template mini-language

Parsed and validated at build time against the entry's argument names. Grammar:

```
template   := ( literal | escape | ref | cond )*
escape     := "{{" | "}}"                      # literal brace
ref        := "{" name "}" | "{" integer "}"   # named arg | 1-based index | "{body}" (envs only)
cond       := "{?" name ":" branch ( "|" branch )? "}"
branch     := ( literal | escape | ref )*      # no nested conditionals, no "|" literal
```
Validation errors (`TemplateError`, `#[non_exhaustive]`): unknown argument name,
index 0 or out of range, `{body}` on a macro/specials, nested conditional,
unterminated construct. Absent optional argument renders as empty (use `{?}` to
branch). Typed form: `Template(Vec<Seg>)`,
`enum Seg { Str, Arg(ArgRef), Body, IfPresent { arg: ArgRef, then: Vec<Seg>, els: Vec<Seg> } }`,
`enum ArgRef { Name(Box<str>), Index(usize) }`.

### 10.6 Unknown constructs & diagnostics

Policies in `Options` (defaults first):
```rust
pub enum UnknownMacroPolicy    { Skip, RenderArgs, KeepSource, Placeholder } // Placeholder → "<name>"
pub enum UnknownEnvPolicy      { RenderBody, Skip, KeepSource }
pub enum UnknownSpecialsPolicy { EmitChars, Skip }
```
Every unknown hit emits its diagnostic regardless of policy; `KeepSource` uses
`SourceRecomposer` (inline → `InlineVerbatim`). Condition types in `techxt::diag`
(identifier / fields / severity):

| identifier | fields | severity |
|---|---|---|
| `techxt.unknown-macro` | name | warning |
| `techxt.unknown-environment` | name | warning |
| `techxt.unknown-specials` | chars | warning |
| `techxt.handler-failed` | construct, detail | error |
| `techxt.unsupported-ignored` | construct, what | warning |
| `techxt.misplaced-alignment` | — | warning |
| `techxt.stray-item` | — | warning |
| `techxt.input-not-resolved` | target | note |
| `techxt.render-aborted` | detail | error |

Parse-level techy diagnostics (when techxt drives parsing) are merged ahead of render
diagnostics in the returned collection.

**Amended for M9 (phase 3):** techxt reports techy-xp's **refusals** as *warnings*.
The eight `techy-xp.presets.*-unsupported` conditions (`\expandafter`, the
conditionals, category codes, registers, allocators, counter commands, `\csname`,
`\noexpand`) are raised upstream at error severity — they must be, since a strict
parse has to abort on one — and `convert::at_techxt_severity` restamps them as the
merge above rebuilds the collection, keeping identifier, message and payload and
losing only the one traceback frame techy has no public way to carry over. The reason
is this table's own philosophy: an unknown construct is a warning here, so a construct
techxt refuses *by name* cannot rank worse than one it has never heard of, and a
document whose text is entirely correct must not sit behind a non-zero exit code. The
budgets (`techy-xp.expand.*-budget-exceeded`) stay errors: a budget is not a missing
feature, it is a document that was cut off. Past the retention cap a *suppressed*
refusal cannot be reclassified — its identity is gone — and is still counted as the
error it arrived as.

The same phase removes the double report: a refusal reaching the renderer with no rule
is rendered by the unknown-construct policy above but **not** diagnosed a second time
as `techxt.unknown-macro` (dispatch step 4 recognizes techy-xp's `RefusalSpec`). One
occurrence, one diagnostic — the one that names the missing feature.

---

## 11. Public API (`techxt::convert`)

### 11.1 Converter

```rust
#[derive(Clone)]                        // internals Arc-shared; Send + Sync
pub struct Converter { .. }
impl Converter {
    pub fn builder() -> ConverterBuilder;
    pub fn standard() -> Converter;     // defs::standard() + Options::default()
    pub fn latex_to_text(&self, latex: &str)
        -> Result<Conversion, techy ParseError<Option<String>>>;   // Err only under Strict
    pub fn tree_to_text(&self, tree: &NodeTree<Latexlike>) -> Conversion;   // infallible
    pub fn tree_to_flow(&self, tree: &NodeTree<Latexlike>)
        -> (Flow, Diagnostics<Option<String>>);
    pub fn language(&self) -> &Language<Latexlike>;
    pub fn options(&self) -> &Options;
    pub fn renderer(&self) -> TextRenderer<'_>;   // for wrapping consumers (§3)
}
pub struct Conversion { pub text: String, pub diagnostics: Diagnostics<Option<String>> }
```
Settled API constraints: annotation fixed to `A = ()` — callers with annotated trees
use techy's cheap zero-copy `tree.annotate(|_| ())`; language fixed to `Latexlike`
(**amended for M9:** to techy-xp's `LatexlikeXp`, techy's latexlike preset carrying
the expanding token reader — read `Latexlike` as `LatexlikeXp` throughout the block
above, and see §16 M9); **no `NodeSlice` entry point in v1** (techy's driver folds
whole trees; if a subtree entry exists in techy at implementation time it may be
added as `node_to_text(NodeRef)`, otherwise omit). `tree_to_text`/`tree_to_flow` are
infallible (§10.4). For wrapping consumers: drive `TreeRecomposer` over
`Converter::renderer()` yourself, then call
`TextRenderer::finish(self) -> RenderFinish { pub trailing: Flow, pub diagnostics:
Diagnostics<Option<String>> }` and append `trailing` (the footnote block) before
layout.

### 11.2 Builder

`ConverterBuilder`: `definitions(DefinitionSet)` (default `defs::standard()`),
`options(Options)` plus per-field setters mirroring every `Options` field,
`override_macro/environment/specials(name, TextRule)`,
`source_resolver(impl techy IntoSourceResolver)`, `recovery(Recovery)` (default
`Tolerant`), `build() -> Result<Converter, BuildError>`.

**Amended for M9 (phase 3):** plus `macro_definitions(MacroDefinitions)` (default
`Honored`) and the two expansion budgets — `expansion_depth_limit(usize)` and
`expansion_count_limit(usize)`, defaulting to the associated constants
`ConverterBuilder::DEFAULT_EXPANSION_{DEPTH,COUNT}_LIMIT` (64 and 2 000), which are
techxt's own rather than techy-xp's for the reason §13's amendment states.

### 11.3 Options (complete, with defaults)

```rust
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Options {
    pub math_mode: MathMode,                  // Fancy       {Fancy, Plain, Source}
    pub math_expression_in: MathWrapDelims,   // Parens      {Parens, Braces, Custom(Box<str>, Box<str>), None}
    pub matrix_delimiters: MatrixDelims,      // Unicode     {Unicode, Ascii}
    pub wrap_width: Option<usize>,            // None
    pub keep_comments: bool,                  // false
    pub heading_style: HeadingStyle,          // NumberedUnderlined {.., Underlined, Prefix, Plain}
    pub footnote_style: FootnoteStyle,        // Collected   {Collected, Inline, Skip}
    pub list_style: ListStyle,                // §9.4 defaults
    pub text_font: FontStyle,                 // Default
    pub math_font: FontStyle,                 // Style(Italic)
    pub unknown_macro: UnknownMacroPolicy,    // Skip
    pub unknown_env: UnknownEnvPolicy,        // RenderBody
    pub unknown_specials: UnknownSpecialsPolicy, // EmitChars
    pub today: Option<Box<str>>,              // None → "<today>"
}
impl Default for Options { .. }
```

---

## 12. The default definitions library (`techxt::defs`)

### 12.1 Modules (one `pub fn category() -> Category` each)

`base` (escapes, spacing, ligature specials, `~`, `\\`, `\par`, misc §9.8 text
macros), `accents`, `fontstyles` (`\textbf`… `\mathbb`… `\emph`, `\operatorname`),
`sectioning`, `lists`, `mathcore` (Greek, operators, symbols, `\frac`, `\sqrt`,
brakets, arrows, dots, delimiters), `mathenvs`, `subsuperscripts`, `verbatim`,
`tables`, `theorems`, `refs`, `links`, `graphics`, `titling`, `preamble`, `inputs`,
`natbib`, `symbols_extra` (the ~1000-entry auto-generated long tail).

`defs::standard() -> DefinitionSet` pushes **all** of them, in this exact order
(later shadows earlier): `symbols_extra`, `base`, `accents`, `fontstyles`,
`mathcore`, `mathenvs`, `subsuperscripts`, `sectioning`, `lists`, `verbatim`,
`tables`, `theorems`, `refs`, `links`, `graphics`, `titling`, `preamble`, `inputs`,
`natbib`. Curated entries thus shadow generated ones. Users wanting less assemble
their own `DefinitionSet` from the modules; dead-code elimination trims the rest.

### 12.2 Parse-side inventory

The union of pylatexenc v3's `latexwalker/_defaultspecs.py` (≈200 macros, 43
environments, specials) with named arguments added everywhere, plus every entry §9
requires. Nothing standard should hit the unknown policy out of the box.

### 12.3 Render-side inventory

Everything in §9 plus the mechanical port of v3's `latex2text/_defaultspecs.py`
symbol/replacement entries (minus dropped categories). Every v3 `simplify_repl`
string becomes a `Literal` or named `Template`; every v3 callable maps to a §9
handler.

### 12.4 Symbol-table generation (`tools/gen_symbols.py`)

A checked-in Python script (dev-only) reads pylatexenc's tables (symbol lists, Greek
and accent builders, sub/superscript tables, font-alphabet offset/exception tables,
and the accent-composition pairs needed by §9.3) from a pylatexenc checkout and
emits checked-in Rust statics (`defs/symbols_extra.rs`, `mathfmt` tables,
`defs/accents` data). Deduplicate (last wins), sort by name, header comment
`// GENERATED by tools/gen_symbols.py — do not edit`.

---

## 13. CLI (`techxt-cli`, command `techxt`, clap derive)

```
techxt [OPTIONS] [FILE]        # FILE or stdin → stdout (or -o FILE)
  -o, --output <FILE>
      --math-mode <fancy|plain|source>          # default fancy
      --math-wrap <parens|braces|none>          # default parens
      --matrix-delims <unicode|ascii>           # default unicode
  -w, --wrap <COLS>                             # default off
      --keep-comments
      --heading-style <numbered-underlined|underlined|prefix|plain>
      --footnote-style <collected|inline|skip>
      --unknown-macro <skip|render-args|keep-source|placeholder>
      --input-dir <DIR>       # sandboxed fs resolver: realpath containment
                              # (dir + separator prefix), .tex/.latex fallback,
                              # include-cycle guard via techy check_include_chain
      --strict                # Recovery::Strict
  -q / -v                     # -q: no diagnostics; default: warning+; -v: notes too
```
Diagnostics → stderr via techy `Diagnostics::render_all()`. Exit codes: 0 clean,
1 conversion completed but diagnostics contain errors, 2 hard parse (strict) or I/O
error. `Options::today` set from the system clock formatted as English
`"August 19, 2026"` (hand-rolled month-name table; no chrono).

**Amended for M9 (phase 3):** three flags for the macro definitions (§16 M9), each
mapping to the `ConverterBuilder` setting of the same name rather than to an `Options`
field:
```
      --no-macro-definitions        # MacroDefinitions::Declared: read the definers,
                                    # honour none of them (techxt 0.1.0's behaviour)
      --expansion-depth-limit <N>   # default 64      (techy-xp's own default: 256)
      --expansion-count-limit <N>   # default 2000    (techy-xp's own default: 100000)
```
The two budget defaults are the library's, and lower than techy-xp's on purpose:
techxt converts untrusted input, and at the upstream allowance `\def\x{\x}\x` runs for
minutes while `\def\x{\x\x}\x` overflows the stack outright.
`ConverterBuilder::expansion_count_limit` carries the measurement; the flags reach the
upstream numbers for a caller who vouches for the input.

---

## 14. Testing

1. **Inline expected-string tests** as the main body (unit + integration under
   `rust/techxt/tests/`), starting from the normative examples in §15.
2. **proptest**: layout invariants (§7), joiner stability (invariant under
   re-chunking of adjacent plain text, no doubled spaces, empties drop), template
   parser round-trips, and no-panic fuzzing of
   `Converter::standard().latex_to_text` over arbitrary strings (tolerant mode must
   never panic).
3. **Coverage checklist from pylatexenc** `test/test_2_latex2text.py` — its *cases*
   (accents incl. `\"{o}`/`{\"o}`/`\L` combinations, `$a$$b$`, math modes on one
   sample, nested lists, spacing around bare macros, sub/superscripts incl. the
   `∑ᵢ₌₁ⁿ` retry, matrices incl. empty, maketitle, `\input` sandboxing incl. the
   `dir-evil` sibling case, verbatim edge cases) with techxt's own expected outputs.
4. **Doctests** on all public API examples.
5. CLI smoke tests (fixture files; assert stdout/stderr/exit code).

---

## 15. Normative acceptance examples

Default options unless noted. Expected strings are exact (trailing `\n` shown as ⏎
only where load-bearing). These are behavior law; encode them as tests early.

| # | input | output |
|---|---|---|
| 1 | `Hello  {brave}\n world.` | `Hello brave world.` |
| 2 | `one\n\n\n\ntwo` | `one\n\ntwo` |
| 3 | `\emph{sic}` | `𝑠𝑖𝑐` |
| 4 | `\textbf{bold}text` | `𝐛𝐨𝐥𝐝text` (one unbreakable word) |
| 5 | `Sk\l odowska` | `Skłodowska` |
| 6 | `\'{e}t\'e` | `été` |
| 7 | `\c c` | `ç` |
| 8 | `` ``Hi,'' -- ok`` | `“Hi,” – ok` |
| 9 | `A% note\nB` | `AB` ; with `keep_comments`: `A` ⏎ `% note` ⏎ `B` |
| 10 | `$x^2 + y_i$` | `𝑥² + 𝑦ᵢ` |
| 11 | `$\frac{4\pi c}{2}\sin(x+y)$` | `(4π𝑐)/2 sin(𝑥 + 𝑦)` |
| 12 | `$\sum_{i=1}^n x_i$` | `∑ᵢ₌₁ⁿ 𝑥ᵢ` |
| 13 | `$a + b$` with `math_mode=Plain` | `𝑎+𝑏` (fonts still apply; no joiner spacing) |
| 14 | `$a + b$` with `math_mode=Source` | `$a + b$` |
| 15 | `\[ E = mc^2 \]` | `    𝐸 = 𝑚𝑐²` (4-space display block) |
| 16 | `\section{Intro}\nText.` | `1 Intro` ⏎ `-------` ⏎ blank ⏎ `Text.` |
| 17 | `\subsection*{Notes}` | `Notes` ⏎ `~~~~~` (no number, not counted) |
| 18 | `Fact\footnote{Proof sketch.} holds.` | `Fact[1] holds.` ⏎ blank ⏎ `---` ⏎ `[1] Proof sketch.` |
| 19 | `\verb|x_1|` | `x_1` |
| 20 | `\href{https://ex.org/a_b}{link}` | `link <https://ex.org/a_b>` |
| 21 | `\begin{tabular}{lr} a & 10 \\ bb & 3 \end{tabular}` | `a   10` ⏎ `bb   3` |
| 22 | `\begin{itemize}\item one \begin{enumerate}\item x\item y\end{enumerate}\item two\end{itemize}` | `  • one` ⏎ `    1. x` ⏎ `    2. y` ⏎ `  • two` |
| 23 | display `\begin{pmatrix} 1 & 2 \\ 30 & 4 \end{pmatrix}` | `    ⎛  1  2 ⎞` ⏎ `    ⎝ 30  4 ⎠` ; with `matrix_delimiters=Ascii`: `    (  1  2 )` ⏎ `    ( 30  4 )` |
| 24 | `\begin{verbatim}\n  keep   this\n\n    exactly\n\end{verbatim}` | body byte-identical, blank-line separated from surroundings, never wrapped |
| 25 | `aaa bbb \textbf{ccc ddd} eee` with `wrap_width=12` | `aaa bbb 𝐜𝐜𝐜` ⏎ `𝐝𝐝𝐝 eee` (wraps across the macro boundary) |
| 26 | `\begin{myenv}inner\end{myenv}` (unknown env, tolerant defaults) | `inner` + warning `techxt.unknown-environment` |

Example 22's markers assume `same_kind_depth = 1` for the inner enumerate (different
kind ⇒ first-level numbering) — that is the intended semantics.

---

## 16. Milestones (in order; each ends green under full CI)

- **M0 — scaffolding.** §2 layout, workspace, lints, CI (GitHub Actions):
  `cargo fmt --check`; `clippy -D warnings` (all targets); `cargo test` (workspace);
  `cargo doc` with denied warnings; MSRV job (1.86); **no_std proof job** building
  the lib for a std-less atomics-capable target (e.g. `thumbv7em-none-eabihf`).
  Empty lib compiles no_std; CLI prints version.
- **M1 — flow + layout.** §6–§7 complete, incl. `render_inline`, with the proptest
  invariants. Tests construct flows by hand (no techy needed).
- **M2 — renderer core.** `TextRenderer` over techy trees: §9.1 dispatch,
  paragraph breaks, state plumbing (§8), diagnostics channel, unknown policies,
  `Conversion` plumbing, all §11 entry points with a hand-built 5-entry definition
  set. Examples 1–2, 26 pass.
- **M3 — definitions infrastructure.** §10 complete: builders, techxt spec types,
  template parser/validator with all error cases tested, dispatch chain, set
  building + `BuildError`, override map.
- **M4 — base library.** `defs::{base, accents, fontstyles, sectioning, refs,
  links, graphics, titling, preamble, inputs}` + generation script (§12.4) for
  accents/alphabet data. Examples 3–9, 16–17, 20 pass.
- **M5 — math engine.** §9.5 complete: atoms, segmentation, joiner, scripts,
  wrappables, dual-stack fonts, `\frac`/`\sqrt`, math envs, matrices incl. display
  multi-line + 2D baseline join + both delimiter styles, all three modes.
  Examples 10–15, 23 pass.
- **M6 — blocks.** Lists (§9.4), verbatim, tables (§9.6), footnotes, theorems,
  quote/center/abstract/captions, `\input` end-to-end with a resolver. Examples
  18–19, 21–22, 24 pass.
- **M7 — CLI.** §13 + smoke tests; run the CLI over a realistic sample paper and
  freeze the result as an integration expectation. Example 25 passes (wrap flag).
- **M8 — polish & release prep.** `symbols_extra` + `natbib` long tail, crate-level
  narrative docs (quick start; extending with custom definitions; writing a
  handler; wrapping the recomposer; layout guarantees), READMEs, CHANGELOG, version
  0.1.0. Do not publish to crates.io while the techy git dependency remains (note
  in README).
- **M9 — macro expansion (techy-xp).** *Amendment to the list above.* Honour
  `\newcommand`, `\renewcommand`, `\def`, `\let` and the environment definers at
  token-reading time by parsing through techy-xp's `LatexlikeXp`/`XpDriver` rather
  than techy's bare `Latexlike`/`LatexlikeDriver`, retiring the §17 omission. Four
  phases: the language switch alone, seeding no definer, so techy-xp's lockstep
  property makes the output byte-identical and the existing suite is the proof; then
  the definer wiring — definition scopes, expansion budgets, and refusal diagnostics
  for what techy-xp declines (conditionals, category codes, registers, counters);
  then tests and the CLI surface for the new diagnostics; then the web app and the
  documentation. Every phase ends green under full CI, not just the milestone.

## 17. Deliberate omissions / future work (documented, not implemented)

Label/`\ref` resolution (two-pass); `\newcommand` expansion; multicolumn/multirow
spans and width-budgeted table cells; centering simulation; source-mirroring
whitespace mode; pylatexenc-compat mode; theorem/figure numbering; localization of
generated words ("Theorem", "Abstract"); streaming incremental layout; serde;
`NodeSlice`/subtree conversion entry (pending techy support); generalization over
`LatexlikeLang` and tree annotations; Python (`python/`, maturin) and JS/wasm
(`js/`) sibling bindings — the module-based defs organization was chosen with wasm
dead-code elimination in mind.

**Amended for M9:** `\newcommand` expansion is no longer deferred — §16 M9 delivers
it through techy-xp, as of phase 2. The entry stays in the list above as the record of
what the first shape omitted. What techy-xp itself declines stays omitted: TeX's
conditionals, category codes, registers and counters are reported, not acted on.

Three things phase 2 leaves for later, each documented where a reader meets it:

- ~~**The expansion budgets are not configurable.**~~ **Done in phase 3:**
  `ConverterBuilder::expansion_depth_limit` / `expansion_count_limit`, defaulting to
  64 and 2 000 rather than techy-xp's 256 and 100 000 for exactly the two hazards this
  bullet named — the flat loop that ran for minutes, and the doubling body whose
  structure overflowed the stack as it was dropped. Both are measured on the
  `expansion_count_limit` doc comment. What is left is upstream's: the quadratic cost
  of reaching the count budget, and a drop that recurses with the size of what was
  expanded.
- **`\input` is state-transparent**, so a definition made inside an included file does
  not survive it (`defs::inputs`). LaTeX's `\input` persists; making it a choice needs
  the rest of techy's `persist_state` consequences thought through.
- **`RedefinitionRule::Always` costs two diagnostics.**
  `techy-xp.define.definition-already-exists` and `…definition-does-not-exist` can
  never be raised on techxt's stack. Recovering them needs an upstream way to keep a
  fallback provider out of a definer's existence query.

The wasm binding under [`web/crate/`](web/crate) is **not** the planned `js/`
package: it is app-private, shaped for one UI, and free to change without a release
(see [`web/PLAN.md`](web/PLAN.md) §3). It is, however, the working proof that the
public API compiles and performs acceptably on `wasm32-unknown-unknown`, so a future
`js/` starts from a known-good shape rather than from scratch — and when it exists,
`web/` depends on it and `web/crate/` is deleted.
