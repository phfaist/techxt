# techxt — implementation plan

`techxt` converts LaTeX-like markup to plain (unicode) text. It is built on the
[`techy`](https://github.com/phfaist/techy) parser (Rust) and is a from-scratch
redesign of the *capabilities* of
[`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc) (v3 beta) — pylatexenc
is an idea source and a porting reference, **not** a compatibility target. Where
pylatexenc has quirks, defects, or structural weaknesses, techxt deliberately deviates
and improves.

This document is self-contained: an implementing agent needs no other context beyond
read access to the `techy` and `pylatexenc` repositories for looking up specific code
being referenced. All design decisions below were made interactively with the project
owner and are settled; do not re-litigate them. Where this plan says "(settled)",
implement as stated. Where it says "(implementer's choice)", use judgement.

Reference checkouts used while writing this plan:
- techy @ `https://github.com/phfaist/techy` (main; workspace version 0.1.0)
- pylatexenc @ `https://github.com/phfaist/pylatexenc` (main; version 3.0beta2)

Line numbers cited below are from those checkouts and may drift; prefer symbol names.

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
   produce structured diagnostics with source positions. Policies are configurable;
   silence is not the default.
4. **LaTeX-correct whitespace semantics, no compatibility knobs.** (settled — see §8)
5. **Reusable, immutable converter.** All per-document state lives in a per-run
   structure. The converter is `Send + Sync` and converts many documents concurrently.
6. **Payload-only reading.** techxt must work on trees that have been transformed
   (`techy::transform`) and no longer map 1:1 to source bytes. Handlers read node
   *payloads* only. Resolving a node's own `TextContent::Spanned` against the node's
   own source is permitted (treat it as optimized payload data). Never use
   `NodeRef::span_content()` / `NodeSlice::source_text()` or any inter-node span
   arithmetic to obtain content. To re-emit a subtree as LaTeX source (math verbatim
   mode, "keep source" policies), use techy's payload-based
   `techy::latexlike::SourceRecomposer` — never raw spans. Node *spans* may be used
   for diagnostic positions only.
7. **techy-grade engineering from day one**: `missing_docs = deny`, denied broken doc
   links, clippy clean, rustfmt, no_std proof in CI, MSRV pinned. No panics on
   document input (contract violations only).

---

## 2. Repository and workspace layout (settled)

```
techxt/                        # repo root (language-neutral)
  README.md                    # short: what techxt is, sibling-folder layout
  PLAN.md                      # this file
  rust/                        # Cargo workspace root
    Cargo.toml                 # [workspace] resolver = "2", members = ["techxt", "techxt-cli"]
    techxt/                    # the library crate: no_std + alloc
      Cargo.toml
      src/...
    techxt-cli/                # the binary crate (std); installed command is `techxt`
      Cargo.toml               # [[bin]] name = "techxt"
      src/main.rs
  tools/                       # dev-only scripts (symbol-table generation, §11.4)
  # later, sibling root folders that do not talk to rust/'s build system:
  # python/  (maturin extension)   js/  (wasm/Node)
```

- Workspace config mirrors techy's: `resolver = "2"`, shared `[workspace.package]`
  (version `0.1.0`, edition `2021`, `rust-version = "1.86"`, license `MIT`,
  author Philippe Faist, repository URL), `[workspace.lints.rust] missing_docs = "deny"`,
  `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"`,
  `private_intra_doc_links = "warn"`, `bare_urls = "warn"`; release profile
  `lto = true`, `codegen-units = 1`. Edition/MSRV may be bumped (e.g. edition 2024)
  if it brings significant code-clarity benefit; matching techy is the default.
- `techxt` lib: `#![cfg_attr(not(test), no_std)]` + `extern crate alloc`.
  Dependencies: `techy` (git dependency pinned to a specific rev of
  `https://github.com/phfaist/techy`, switched to a crates.io version once techy is
  published) and `unicode-width` (settled). Nothing else in the runtime tree.
- `techxt-cli`: depends on `techxt`, `clap` (derive feature) (settled), and std.
- **No cargo features** (settled). The definitions library is organized as Rust
  modules the user references explicitly; unreferenced modules are removed by
  dead-code elimination. Do not add a `serde` feature or others in v1.

---

## 3. What techy provides (API contract techxt builds on)

techxt uses the `techy::latexlike` preset. Key facts (verify against the techy docs;
narrative guides live in techy's `docs/` folder — `ai-guide.md`, `ai-guide-trees.md`,
`ai-guide-definitions.md`, `ai-guide-pylatexenc.md` are the fastest reads):

**Parsing.**
```rust
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),      // techxt default: Tolerant
    ParsingState::lang_initial_with_packages(packages)?,
);
let result = language.parse(input)?;               // ParseResult { tree, diagnostics, .. }
```
`Language` is `Send + Sync`, built once, parses many documents. Tolerant parses return
a whole-input tree plus diagnostics (`result.diagnostics.has_errors()`); an
unresolvable `\foo` becomes a literal `Chars` node `"\foo "` plus an error diagnostic.
`LatexlikeDriver::with_source_resolver(...)` enables `\input` resolution at *parse*
time (resolved content appears as an `Attached` slot on the `\input` node);
`techy::source::SourceResolver` is the trait, `check_include_chain` guards cycles.
Keep the driver's default `ParagraphBreakStyle::Chars` (paragraph breaks arrive as
whitespace-only `Chars` nodes containing a blank line).

**Node model** (`techy::core::node`). Five kinds:
`Chars { content }`, `Group(GroupData { group_type, open, close })`,
`Callable(CallableData { callable_type, name, spec, arguments, slots, invocation_syntax })`,
`Comment(CommentData { start, content, post_space })`, `List`.
There is no Math/Macro/Environment/Verbatim node kind: math is
`GroupType::Math(MathGroupForm::{Inline,Display})`; macros/environments/specials are
`Callable` distinguished by `callable_type`; `\verb` is `GroupType::Verbatim` with one
raw `Chars` child; a `verbatim` environment is an environment whose body list holds
one raw `Chars` node. Latexlike sugar on `NodeRef`: `is_math_group()`, `math_form()`,
`macro_name()`, `environment_name()`, `specials_name()`, `post_space()`.
Arguments are accessed semantically: `argument_content_nodes(i)`,
`argument_content_nodes_named(name)` (absent optional → `Ok(None)`; wrong name →
`Err`), providedness via `ParsedArgument::is_provided()`. Environment bodies via
`body()` / slots via `slot_content_nodes_named("attached")` etc. Every node records
its parsing state: `node.parsing_state().mode()` is `techy::latexlike::Mode::{Text,Math}`.

**Recompose** (`techy::recompose`) — techxt's engine (settled):
```rust
pub trait Recomposer<L: Lang, A> {
    type State;                 // downward-threaded context
    type Piece: ComposePiece;   // techxt: the Flow type (§6)
    type Error;
    fn recompose_node(&mut self, node: NodeRef<'_, L, A>, state: &Self::State,
                      cx: &mut RecomposeContext<'_, L, A>)
        -> Result<Recompose<Self::Piece, Self::State>, Self::Error>;
}
```
Instructions: `Recompose::Emit(piece)` or `Recompose::Concat(ConcatPieces::children()
.wrap(head, tail).join(sep).with_state(derived).include_attached().include_hidden())`.
Re-entrant region ops on `RecomposeContext` (always pass `self` back):
`recompose_argument_content_named(node, name, state, self)`,
`recompose_argument_content(node, i, ...)`, `recompose_body(node, ...)`,
`recompose_slot_content_named(node, "attached", ...)`. Absent argument → empty piece.
Driver: `TreeRecomposer::new(&mut recomposer).recompose(&tree, initial_state)`.
Fold order is document order (enter order; eager region ops preserve this) — sectioning
counters and footnote collection in `&mut self` rely on this.
**Wrapping contract**: consumers extend techxt by wrapping its recomposer with their
own `Recomposer` that overrides some nodes and delegates the rest — a wrap-intended
recomposer returns instructions and never descends explicitly. techxt's recomposer
must be usable as such an inner recomposer (public, documented).
**Role rule**: `Concat` skips `Attached`/`Hidden` slots by default — the `\input`
handler must explicitly render the attached slot.

**Definitions** (`techy::core::specs`, `techy::latexlike`). techy ships **no**
standard LaTeX definitions (only `\begin`/`\end` in `builtin_package()`); techxt owns
the whole definitions database. Building blocks:
`Package::new(name)` / `Package::insert(callable_type, name, spec)` /
`insert_specials(...)` / mode-restricted variants; pylatexenc-style argspec codes via
`techy::latexlike::argument_specs_from_str::<Latexlike>("*[{")` and named arguments
via `argument_specs_named([("s","star"),("o","toctitle"),("m","title")])` (codes:
`m/{`, `o/[`, `s/*`, `t<c>`, `r<c1><c2>`, `d<c1><c2>`, `v`, `e{...}`, plus word codes
`AnyDelimited`, `BracedOnly`). Spec types: `MacroSpec::new(args)`,
`EnvironmentSpec::new(args)` (+ `.with_body_delta(...)` — how `equation` enters
`Mode::Math` and how list bodies inject an `\item` package), `SpecialsSpec`,
`VerbatimBehavior` for verbatim-bodied environments,
`ArgumentSpec::with_state_delta(...)` (how `\text{...}` leaves math). `CallableSpec`
has an `Any` supertrait → **downcastable**: `(&**node.spec().unwrap() as &dyn Any)
.downcast_ref::<TechxtMacroSpec>()` is the sanctioned identity mechanism techxt's
dispatch uses (§9). Register names **without** the escape char (`"emph"`, not `"\\emph"`).

**Diagnostics** (`techy::error`). techxt defines its own condition types deriving
`techy::error::DiagnosticInfo` (re-exported derive) and returns
`techy::error::Diagnostics<Option<String>>` from conversions. Use `Diagnostic::warning
(condition, span)` with the node's `SourceSpan` (spans are fine for *positions*).

**Extract** (`techy::extract`): `content_as_chars(nodes)` for plain-text arguments
(`\label{...}`, URLs); errors on callables. Group-protected splitting helpers exist
but techxt's cell/item splitting works at the flow level instead (§10.4, §10.6).

---

## 4. What to take from pylatexenc v3 (and what to fix)

Porting references in the pylatexenc checkout (`pylatexenc/latex2text/__init__.py`
≈3700 lines, `pylatexenc/latex2text/_defaultspecs.py` ≈2000 lines):

**Adopt (redesigned into techxt's architecture):**
- The five math modes with `fancy` as default, and the whole fancy math engine:
  atom classes, plain-string segmentation, join rules, unary-minus reclassification,
  script handling and sub/superscript unicode tables, wrappable pieces,
  `math_expression_in` delimiters, `\sqrt` → `√`/`∛`/`∜`. (§10.5)
- Unicode font alphabets (`fmt_math_text_style`, offsets + exception tables) with
  separate text/math font-style state. (§10.5)
- List rendering: per-depth markers `• – * ·` and `1. (a) i. A.`, *same-kind* depth
  counting (an `itemize` inside an `enumerate` is a first-level itemize), explicit
  `\item[label]` does not advance the counter, hanging indents. (§10.4)
- The accent tables (`unicode_accents_list`) and combining-char approach — but apply
  the combining char to the **first base character only**, not every character
  (fixes v3's `\hat` stamping the hat onto whole rendered arguments).
- The symbol tables (Greek, operators, relations, arrows, the large auto-generated
  set) — via a generation script, §11.4.
- `\href`/`\url` with verbatim-parsed URL arguments (v3's parse-side fix).
- The *inventory* of its test suite (`test/test_2_latex2text.py`) as a coverage
  checklist — with techxt's own expected outputs.

**Fix / deliberately deviate (all settled):**
- One unified definitions database instead of two (§9).
- Named argument access everywhere; no positional `%(2)s` crashes.
- Diagnostics for unknown constructs instead of silence (§12).
- Whole-paragraph layout instead of per-chars-node `textwrap`; verbatim content is
  never wrapped, styled, or whitespace-normalized (§7).
- A real paragraph/block model with one normalization policy instead of accidental
  `'\n%s\n'` spacing (§7).
- Verbatim renders its content (v3 renders `\verb` as nothing and mangles
  `{verbatim}`).
- Immutable, reusable converter; per-run state (v3 mutates the converter).
- `\input` is parse-time (techy resolver) with cycle guard; no unguarded recursion.
- Display width via `unicode-width`, not `len()`.
- Sectioning: numbered + underlined by default, no uppercasing (§10.3).
- Footnotes collected with `[n]` markers by default (§10.7).
- Basic aligned `tabular` tables (§10.6).
- `\today` is an option supplied by the embedder (no_std has no clock); CLI passes the
  current date; default when unset: placeholder `<today>` (v3 freezes the date at
  Python-import time).
- Dropped entirely: `strict_latex_spaces` (all four knobs and presets),
  `keep_braced_groups`, pylatexenc-v1 compat shims, the personal categories
  (`latex-ethuebung`, `nonstandard-qit`), `%`-style replacement strings, callable
  introspection by parameter name.

---

## 5. Architecture overview

```
                       (techxt-owned definitions: parse specs + text rules)
                                        │
 input &str ──► techy Language::parse ──► NodeTree + parse Diagnostics
                                        │
        NodeTree (possibly user-transformed) ──► TextRenderer (impl techy Recomposer)
                                        │            │ downward RenderState
                                        │            │ &mut RunState (counters, footnotes, diags)
                                        ▼
                                   Flow (typed token sequence)
                                        │
                                   layout engine (wrap, indent, blocks, normalize)
                                        ▼
                              String  +  Diagnostics
```

All three layers are public, documented API (settled): the convenience layer
(string → string), the tree layer (convert an existing `NodeTree`/`NodeSlice`), and
the flow/layout layer (flow tokens, layout engine, the `TextRenderer` recomposer).
Evolve public enums/structs with `#[non_exhaustive]`.

Proposed module map for `rust/techxt/src/` (implementer may refine; keep one
canonical public path per item, techy-style):

```
lib.rs          — crate docs, re-exports of the main entry points
convert.rs      — Converter, ConverterBuilder, Options, Conversion  (§13)
flow/           — Flow, FlowItem, BlockKind, math-atom items        (§6)
layout/         — LayoutOptions, layout engine                       (§7)
render/         — TextRenderer (Recomposer impl), RenderState, RunState, RenderCx (§8, §10)
def/            — Definition model: DefinitionSet, Category, MacroDef/EnvDef/SpecialsDef,
                  TechxtMacroSpec/TechxtEnvironmentSpec/TechxtSpecialsSpec,
                  TextRule, Template (+ parser/validator), TextHandler   (§9)
mathfmt/        — the fancy math engine: atom classes, segmentation, joiner,
                  sub/superscript + font-alphabet tables              (§10.5)
defs/           — the default definitions library, one module per category (§11)
diag.rs         — techxt DiagnosticInfo condition types               (§12)
```

---

## 6. The flow model (`techxt::flow`)

The `Recomposer::Piece` type. Requirements (settled): simple; minimal overhead when
no wrapping is requested; expressive enough for wrapping, indented blocks, verbatim,
tables, and math atoms.

```rust
/// The piece monoid. Newtype over Vec so ComposePiece::append is an O(amortized) extend.
pub struct Flow(Vec<FlowItem>);
impl techy::recompose::ComposePiece for Flow { /* empty(); append = extend */ }

#[non_exhaustive]
pub enum FlowItem {
    /// A run of non-whitespace text. ADJACENT Text items are glued (no break between
    /// them) — `\textbf{bold}text` must never wrap between "bold" and "text".
    Text(Box<str>),
    /// One collapsible inter-word space; the only place wrapping may break.
    Glue,
    /// Forced line break (e.g. `\\`, table row end after layout).
    HardBreak,
    /// Paragraph separator. Layout normalizes any run of these (and block
    /// boundaries) to at most one blank line.
    ParagraphBreak,
    /// Preformatted block: emitted line-by-line with the current continuation
    /// indent, NEVER wrapped, styled, or whitespace-normalized.
    Verbatim(Box<str>),
    /// Inline preformatted fragment (`\verb`): unbreakable, un-normalized.
    InlineVerbatim(Box<str>),
    /// Open a block context. Blocks imply paragraph-level separation from
    /// surrounding content (layout inserts/normalizes the blank lines).
    BlockStart(BlockKind),
    BlockEnd,
    /// Math atom — exists ONLY transiently inside math subtrees; the math-group
    /// handler resolves atoms via the joiner (§10.5) into plain items above.
    /// Layout never sees this variant (debug_assert in layout).
    MathAtom(mathfmt::Atom),
}

#[non_exhaustive]
pub enum BlockKind {
    /// Indented block with a hanging indent: first-line prefix + continuation prefix.
    /// Used for list items ("  • " / "    "), display math ("    "/"    "),
    /// footnote entries, quote-like environments.
    Indent { first: Box<str>, cont: Box<str> },
    /// A list-item block. A new Item at the same nesting level implicitly closes
    /// the previous one (auto-close in layout), so `\item` handlers need no lookahead.
    Item { first: Box<str>, cont: Box<str> },
    /// Table cell/row separators, consumed by the table handler before layout (§10.6).
    CellSep,
    RowSep,
}
```

Construction helpers: `Flow::text(&str)`, `Flow::word(&str)` (text + implies nothing),
`Flow::glue()`, plus a `flow!`-style builder if useful. A helper
`flow::from_plain_text(&str)` converts an arbitrary string (e.g. a `Literal` rule's
replacement, template literal segments) into Text/Glue/ParagraphBreak items by
splitting on whitespace (a blank line → ParagraphBreak). Chars nodes go through the
same helper.

Width measurement: a single internal function `display_width(&str) -> usize` using
`unicode_width::UnicodeWidthStr` — the only place width is computed (used by layout,
tables, matrices, heading underlines).

---

## 7. The layout engine (`techxt::layout`)

One pass over a `Flow`, producing `String` (or writing into `core::fmt::Write`).

```rust
#[non_exhaustive]
pub struct LayoutOptions {
    /// None (default) = no wrapping: glue renders as a single space.
    pub wrap_width: Option<usize>,
}
pub fn render(flow: &Flow, opts: &LayoutOptions) -> String;
pub fn render_to(flow: &Flow, opts: &LayoutOptions, out: &mut dyn core::fmt::Write) -> fmt::Result;
```

State: indent stack (per open block: first/cont prefixes), current column
(display width of current line), pending glue flag, pending vertical-space request.
Rules (all settled):

1. **Words**: adjacent `Text` items concatenate into one unbreakable word. A word is
   emitted whole; if `wrap_width` is set and `current_col + 1 + width(word)` exceeds
   it, break at the pending glue first (emit newline + continuation indent). A word
   wider than the remaining width on an empty line overflows (never split words).
2. **Glue** collapses: any number of consecutive Glue items = one potential break
   point / one space. Glue at line start/end is dropped (no trailing spaces, ever).
3. **Vertical spacing is normalized in one place**: `ParagraphBreak`, `BlockStart`,
   `BlockEnd` all *request* separation; consecutive requests merge (max, not sum).
   Result: at most one blank line between any two content lines; no leading/trailing
   blank lines in the output; output ends with exactly one `\n` if nonempty.
4. **Blocks**: `BlockStart(Indent|Item)` pushes prefixes; first content line of the
   block gets `first`, subsequent lines get `cont` (both count toward the column for
   wrapping). `Item` auto-closes a previous open `Item` at the same depth.
5. **Verbatim**: emit each line raw, prefixed by the current continuation indent
   only; never wrapped or trimmed; surrounding separation as a block.
   `InlineVerbatim` behaves as an unbreakable word whose interior is emitted raw.
6. **No-wrap fast path**: with `wrap_width = None` the same single pass runs without
   the width checks — glue → one space. Keep the code shared; do not fork the engine.

proptest invariants (§15): no line exceeds `wrap_width` unless it contains a single
oversized word or verbatim content; no trailing whitespace on any line; never two
consecutive blank lines; verbatim payload substrings appear byte-identical in output;
layout is deterministic.

---

## 8. Render state

Two channels (matching techy's state discipline):

**Downward state** (`Recomposer::State`, cloned per derived scope, auto-restored):
```rust
#[non_exhaustive]
pub struct RenderState {
    pub math: Option<MathCtx>,          // None = text mode; Some { form: Inline|Display }
    pub text_font: FontStyle,           // None-equivalent = upright; see §10.5
    pub math_font: FontStyle,           // default Italic
    pub in_table: bool,                 // & and \\ emit CellSep/RowSep when set
    pub list: Option<ListCtx>,          // marker kind + same-kind depth for the innermost list
}
```
Derived with `ConcatPieces::with_state(...)` (containers) or passed to re-entrant
region ops (handlers). Math mode may *also* be read from `node.parsing_state().mode()`
(techy tracks it), but the downward state is authoritative for rendering because
options (`math_mode`) affect it.

**Per-run state** (fields of the `TextRenderer` instance, which is constructed fresh
per conversion; the public `Converter` stays immutable):
```rust
struct RunState {
    diagnostics: techy::error::Diagnostics<Option<String>>,
    heading_counters: [u32; 7],         // part..subparagraph
    list_counter_stack: Vec<u32>,       // pushed/popped by list-env handlers around recompose_body
    footnotes: Vec<Flow>,               // collected notes; markers are 1-based
    doc_title: Option<Flow>, doc_author: Option<Flow>, doc_date: Option<Flow>,
}
```
Fold order is document order, so counters in `&mut self` are correct. List counters
live here (not in downward state) because increments must survive across siblings;
the env handler brackets `cx.recompose_body(...)` with push/pop.

---

## 9. The definitions model (`techxt::def`)

### 9.1 Definition entries

One entry carries name, parsing arguments, parse-side state deltas, and the text rule:

```rust
pub struct MacroDef { /* name, args: Vec<ArgSpec-ish>, rule: TextRule, modes, after_effect… */ }
pub struct EnvDef   { /* name, args, body_delta (math/verbatim/item-package), rule */ }
pub struct SpecialsDef { /* trigger chars, args, rule, modes */ }
```
Builder-style constructors, e.g.:
```rust
MacroDef::new("frac")
    .args([("m", "num"), ("m", "den")])          // pylatexenc-style codes + REQUIRED names
    .rule(TextRule::template("{num}/{den}"))      // parsed & validated here
MacroDef::symbol("alpha", "α")                    // shorthand: zero args + Literal
EnvDef::new("equation").math_body().rule(TextRule::handler(EquationHandler))
```
Every argument in techxt's own database gets a **name** (settled: named access
everywhere). Registration converts each entry into a techxt spec object:

```rust
pub struct TechxtMacroSpec       { args: Vec<Arc<ArgumentSpec<Latexlike>>>, rule: TextRule, .. }
pub struct TechxtEnvironmentSpec { .. }   // wraps EnvironmentSpec behavior incl. VerbatimBehavior
pub struct TechxtSpecialsSpec    { .. }
```
each implementing `techy CallableSpec<Latexlike>` (delegate parsing to the standard
argument machinery; `Any` supertrait makes the embedded rule reachable by downcast).
These types are public (all-layers-public decision) but most users never touch them.

### 9.2 Categories and sets

```rust
pub struct Category { name: &'static str, macros: Vec<MacroDef>, envs: …, specials: … }
pub struct DefinitionSet { categories: Vec<Category> }   // ordered; later = higher priority? NO:
```
Priority model: a `DefinitionSet` builds **one techy `Package` per category**, pushed
onto the scope stack in order — techy's innermost-first resolution means categories
added *later* shadow earlier ones (document this clearly; it inverts pylatexenc's
first-category-wins). `DefinitionSet::to_parsing_state()` produces the
`ParsingState` for `Language::new`. Users extend by `set.push(category)` or by
registering into their own category.

### 9.3 Text-rule dispatch at render time (settled — hybrid)

For each `Callable` node, in order:
1. **Override map**: `Converter`-level `HashMap<(CallableKind, Box<str>), TextRule>`
   set via `ConverterBuilder::override_macro("section", rule)` etc. — cheap per-name
   render overrides without touching parsing and without writing a wrapping recomposer.
2. **Embedded rule**: downcast `node.spec()` to a techxt spec type; use its rule.
3. **Name fallback table**: `(kind, name) → TextRule`, auto-populated from the same
   `DefinitionSet` at converter build time — covers trees parsed with plain-techy
   (foreign) specs.
4. **Unknown policy** (§12).

### 9.4 `TextRule`

```rust
#[non_exhaustive]
pub enum TextRule {
    Literal(Cow<'static, str>),   // run through flow::from_plain_text (and math segmentation in math)
    Template(Template),           // compiled, validated (§9.5)
    Skip,                         // emit nothing (the explicit pylatexenc discard=True)
    Content,                      // macros: render provided arguments' content in order;
                                  // environments: render body; specials: emit trigger chars
    Handler(Arc<dyn TextHandler>),
}
pub trait TextHandler: Send + Sync + core::fmt::Debug {
    fn render(&self, node: NodeRef<'_, Latexlike>, cx: &mut RenderCx<'_, '_>)
        -> Result<Flow, RenderError>;
}
```
`RenderCx` (borrowing the `TextRenderer` + techy `RecomposeContext`) offers:
`arg(name) -> Result<Option<Flow>, …>` (render argument content by name),
`arg_provided(name) -> bool`, `arg_plain_text(name)` (via `extract::content_as_chars`
for URLs/labels), `body() -> Flow`, `attached() -> Option<Flow>`, `state() ->
&RenderState`, `with_state(new, f)` for derived-state sub-renders, `options()`,
`run_mut()` (footnotes/counters/title), `warn(condition, span)`,
`source_of(node) -> String` (payload-based, via techy's `SourceRecomposer`), and
flow construction helpers. Rules producing plain strings in math mode are segmented
into atoms by the math engine (§10.5), exactly as v3 segments `simplify_repl` output.

### 9.5 Template mini-language (settled, incl. one conditional form)

Compact strings parsed **at registration** into typed segments; every reference
validated against the entry's argument names (unknown name, out-of-range index,
`{body}` on a macro, nested conditional → `Err` at registration, never at render).

Syntax:
- `{name}` — converted content of the named argument (absent optional → empty).
- `{1}` — 1-based index (allowed, discouraged; techxt's own DB uses names only).
- `{body}` — environment body (environments only).
- `{?name:then|else}` — IfPresent conditional: if argument `name` was provided,
  render the `then` segment sequence, else `else`. `|else` may be omitted. Branches
  may contain `{...}` references but not nested conditionals. Literal `|` inside a
  branch is not supported (use a Handler).
- `{{` and `}}` — literal braces.

Typed form:
```rust
pub struct Template(Vec<Seg>);
enum Seg { Str(Box<str>), Arg(ArgRef), Body,
           IfPresent { arg: ArgRef, then: Vec<Seg>, els: Vec<Seg> } }
```

---

## 10. Conversion semantics

### 10.1 Node-kind dispatch (the `TextRenderer::recompose_node` skeleton)

- `Chars`: whitespace-only run containing a blank line → `ParagraphBreak`; otherwise
  `flow::from_plain_text` (words + glue). In math mode: strip all whitespace and
  segment into math atoms (§10.5). Apply the active font style (§10.5) to letter
  mapping at this leaf level. In a verbatim parsing context (raw `Chars` under a
  verbatim group/env — detectable via the parent group type / env spec), emit
  `Verbatim`/`InlineVerbatim` instead, untouched.
- `Comment`: default `Skip` (contributes nothing, including its trailing newline —
  LaTeX-correct). With `Options::keep_comments`, emit the comment text as its own
  hard line (`% …` + HardBreak).
- `Group`: math group → math handling (§10.5). Verbatim group (`\verb`) →
  `InlineVerbatim` of its raw child. Other groups → transparent
  `Concat(children)` (no braces in output; `keep_braced_groups` is deliberately gone).
- `List` → `Concat(children)`.
- `Callable` → rule dispatch (§9.3), executing the rule (§9.4).

Whitespace/paragraph policy is **fixed** (settled, no knobs): macro post-space is
invocation syntax and never emitted; source whitespace runs collapse to glue;
paragraph breaks normalize to one blank line; math ignores source whitespace;
verbatim preserves everything.

### 10.2 Escapes, spacing macros, ligatures, accents (in `defs::base`, `defs::accents`)

Port the v3 inventories: `\{ \} \$ \& \# \_ \% \~`, `\,`/`\;`/`\:`/`\ ` → space,
`\!` → nothing, `\quad`/`\qquad`, `\\` → `HardBreak` (parse spec `*[` with
no-pre-space optional, as in pylatexenc), ligature specials (`` ` `` `''` `--` `---`
`!`` ` `?`` ` → unicode), `~` → no-break: render as `Text("\u{00A0}")` or as glue that
never breaks — use NBSP text (simplest; unbreakable by construction). Accent macros:
combining char appended to the **first base char** of the rendered argument, dotless
ı/ȷ mapped to i/j, then NFC-normalize *that pair only* via a small built-in
composition table (no unicode-normalization dependency; a table of the ~200
accent+base→composed pairs occurring in the accents set is generated with the symbols
script, §11.4).

### 10.3 Sectioning (`defs::sectioning`) — settled defaults

Parse spec `s o m` named `("star","toctitle","title")` for all seven levels. Handler:
increment heading counters (unless starred), reset deeper counters, render as a block:
number (`2.3`) + title line, underlined (`=` part/chapter, `-` section, `~`
subsection) with underline length = display width of the heading line; deeper levels
(subsubsection and below) render as an indented plain heading line. No uppercasing.
`HeadingStyle` option: `NumberedUnderlined` (default) | `Underlined` | `Prefix`
(§/§.§ style) | `Plain`.

### 10.4 Lists (`defs::lists`) — settled design

- `itemize`/`enumerate`/`description`/`list`/`trivlist` (+ `enumitem` arg accepted
  and ignored in v1). Env handler: derive downward `ListCtx { kind, same_kind_depth }`,
  push `0` onto `run.list_counter_stack`, `cx.recompose_body(...)`, pop, wrap the
  result between `BlockStart(Indent…)`/`BlockEnd` (outermost list only gets the
  2-space indent, per v3).
- `\item` (defined inside list bodies via the env's body delta, techy-style; plus a
  global fallback def): handler reads `ListCtx` + counter stack; emits
  `BlockStart(Item { first: marker_prefix, cont: aligned_spaces })`. Markers by
  same-kind depth: itemize `• – * ·` (cycle), enumerate `1.` `(a)` `i.` `A.`
  (arabic/alph/roman/Alph formatters), description: the rendered `\item[label]`
  followed by two spaces. Explicit `[label]` replaces the marker and does **not**
  advance the counter. Item blocks auto-close at the next `Item`/`BlockEnd` (§6).
  A stray `\item` outside any list: warn + render as a dashed item.
- `ListStyle` option to customize marker sets (implementer: simple struct of arrays).

### 10.5 Math (`techxt::mathfmt` + `defs::math*`) — settled: full fancy engine in v1

`MathMode` option: `Fancy` (default) | `Text` | `WithDelimiters` | `Verbatim` |
`Remove`. Display detection: `math_form() == Display` or math environment. Display
output is an indented block (4 spaces) in all convert-modes; `Verbatim` re-emits the
subtree as LaTeX via `SourceRecomposer` (payload-based) — inline stays inline, display
becomes a verbatim block; `Remove` emits nothing.

Fancy engine — port from v3 (`pylatexenc/latex2text/__init__.py`, lines ≈1067–1954):
- `Atom { text: Box<str>, cls: (AtomClass, AtomClass), flags }` with
  `AtomClass = Ord|Op|Bin|Rel|Open|Close|Punct|OpenEnd|Text|Script|Block` (v3's set).
- Plain-string segmentation `_segment_plain_str`: ≥2 upright latin letters → one `Op`
  atom (only when letters are upright, i.e. font mapping left them ASCII); digit runs
  with decimal point → one `Ord`; explicit character tables for bin/rel/open/close/
  punct/op chars (port v3's tables verbatim; `|` and `/` deliberately unclassified).
- The joiner `_join_math_pieces`: realize against neighbors; drop empty pieces;
  unary-minus reclassification; script class propagation; the seven spacing rules of
  `_math_needs_space` (space around bin/rel and openend, around op/text except at
  delimiters, after punct, between adjacent digits); no double spaces; scripts attach
  with no space, retroactive space before the base of a `^`-notation fallback script;
  superscript-then-subscript pairs swapped so the subscript binds first.
- Wrappable atoms (v3 `_MathWrappablePiece`): carry unwrapped/wrapped renderings and
  decide at join time (`needs_wrapping`: another script on the same base, contains a
  space, contents open-ended). `math_expression_in` option: `Parens` (default, per
  v3) | `Braces` | custom `(open, close)` | `None`.
- Sub/superscripts: `^`/`_` as specials taking one expression argument **in math mode
  only** (text mode: literal chars — parse-side handled by mode-restricted
  registration, techy `insert_specials_in_modes`). Renderer: try unicode script
  chars (port `_fmt_superscript_chars`/`_fmt_subscript_chars` tables + the
  math-italic→ASCII normalization inverse table), retry with joiner-inserted spaces
  stripped, else fall back to `^(…)` wrappable. 
- Font alphabets: port `_fmt_math_style_offsets` + `_fmt_math_style_exceptions`
  (13 styles + reserved-codepoint exceptions). `FontStyle` in both text and math
  variants with v3's three-valued semantics (style / upright-default / disabled).
  Applied at chars leaves. `\emph` toggles; `\text{}`/`\mbox{}` switch mode only.
- `\frac` → wrappable `{num}/{den}` handler; `\sqrt` handler (`√`, `∛`, `∜`, symbolic
  degree prefixed); `\operatorname`, operator-name macros as `Op` literals.
- Math environments (`equation(*)`, `align(*)`, `gather(*)`, `multline(*)`,
  `eqnarray(*)`, `split`, `subequations`, `dmath(*)`): math body via techy body delta;
  render via the math pipeline as display blocks; `&` in math context → alignment
  glue (two spaces), `\\` → HardBreak within the display block.
- Matrices (`matrix/pmatrix/bmatrix/vmatrix/Vmatrix/smallmatrix` + `array`): block
  atoms — split body flow at CellSep/RowSep (the `&`/`\\` rules emit these when
  `in_table`-analog math-matrix context is set), render cells, measure with
  `display_width`, right-justify, join `' '` / `'; '`, wrap in the env's delimiters
  (`( )`, `[ ]`, …). Empty matrix → `[ ]` (no panic — v3 crashes on `max()` of empty).

### 10.6 Tables (`defs::tables`) — settled: basic aligned tables in v1

`tabular`/`tabular*`/`tabularx` (extra width args parsed and ignored): env handler
sets `in_table` downward state; `&` → `BlockKind::CellSep`, `\\` → `RowSep`, `\hline`
→ a marker the handler turns into a dashed rule; handler folds body, splits flow at
separators into rows×cells, lays each cell out (single-line, no wrap), pads to column
widths (`display_width`), aligns per column-spec letters `l`/`c`/`r` (parse the
colspec argument's plain text; ignore `|`, `@{…}`, `!{…}`, `p{…}`→`l`), joins with
two spaces, emits rows as HardBreak-separated lines inside a block. `\multicolumn`:
render content into its cell, ignore the span (accepted, warned once). Out of scope
(document as such): row/column spans, width-budgeted cell wrapping, booktabs styling.

### 10.7 Footnotes, refs, links, graphics, misc (settled defaults)

- `\footnote` (arg names `("o","mark")`, `("m","note")`): default `FootnoteStyle::
  Collected` — emit `[n]` marker (n = `run.footnotes.len()+1`), push rendered note;
  after the root fold, if footnotes were collected, append `--- ` rule block + one
  `Item`-style block per `[n] note`. Also `Inline` and `Skip` variants.
- `\ref/\autoref/\cref` → `<ref>`, `\Cref` → `<Ref>`, `\eqref` → `(<ref>)`,
  `\cite/\citet/\citep` + natbib set → `<cit.>`; `\label` → `Skip`. (Label/ref
  resolution is future work, §17.)
- `\url` (verbatim arg named `url`) → `<url>`; `\href{url}{text}` → `text <url>`.
- `\includegraphics` → placeholder block `< g r a p h i c s >` (keep v3's spaced-out
  letters rationale: avoids polluting search indexes with the word "graphics").
- `\title/\author/\date` store rendered flow in run state; `\maketitle` renders
  title/author/date lines + `=` rule sized by display width; missing pieces render
  placeholders (`<no title>` …); `\today` → `Options::today` string or `<today>`.
- `\input`/`\include` (arg `("m","filename")` with chars-name parsing): render the
  `attached` slot via `cx.recompose_slot_content_named("attached", …)`; if the slot
  is absent (no resolver configured), emit nothing + info diagnostic.
- `verbatim`/`verbatim*`/`lstlisting` environments: `VerbatimBehavior` parse-side;
  render body raw as `Verbatim` block (lstlisting options arg parsed & ignored).
  `\verb` → `InlineVerbatim`.
- `center`/`flushleft`/`flushright`/`quote`/`quotation` → simple indent blocks
  (centering not simulated in v1); `figure`/`table`(+`*`) → render body;
  `\caption` → render its content on its own line prefixed `Figure: `/`Table: `?
  — v1: render content as own paragraph (implementer's choice on prefix); `abstract`
  → indented block; theorem envs (`theorem`,`lemma`,… + short aliases, optional
  title arg) → block starting `Theorem (title). ` style.
- `\texorpdfstring` → second argument; `\phantom`/`\hspace` → nothing; `\vspace` →
  ParagraphBreak; `\mbox`/`\text`/`\textrm…` per font-style rules.

---

## 11. The default definitions library (`techxt::defs`)

### 11.1 Organization (settled: Rust modules, no cargo features)

One module per category; each exposes `pub fn category() -> Category`. A convenience
`defs::standard() -> DefinitionSet` assembles the standard set (everything below
except `natbib` extras? — no: include everything listed; users wanting less assemble
manually). Dead-code elimination trims whatever a user never references; the
`standard()` function is the only item referencing all categories.

Module list (target contents; counts are pylatexenc-order-of-magnitude):
`base` (escapes, spacing, ligatures, quotes/dashes, `\\`, misc text macros),
`accents`, `fontstyles` (`\textbf`… `\mathbb`… `\emph`, `\operatorname`),
`sectioning`, `lists`, `paragraphs` (`\par` → ParagraphBreak — v3 forgot it),
`mathcore` (Greek, operators, common symbols, `\frac`, `\sqrt`, brakets, arrows,
dots, delimiters), `mathenvs` (equation family + matrices), `subsuperscripts`,
`verbatim`, `tables`, `theorems`, `refs`, `links`, `graphics`, `titling`
(`\title`/`\maketitle`/`\today`), `preamble` (`\documentclass`, `\usepackage`,
`\newcommand` & friends — parse specs so arguments are consumed, rule `Skip`),
`inputs` (`\input`/`\include`), `natbib`, `symbols_extra` (the ~1000-entry
auto-generated long tail).

### 11.2 Parse-side inventory

Port the union of pylatexenc v3's `latexwalker/_defaultspecs.py` (≈200 macros,
43 environments, 8 specials + `\n\n`+`^`+`_`) — with named arguments added to every
entry techxt renders, and including entries v3's l2t forgot (so nothing renders as
"unknown" out of the box for standard LaTeX).

### 11.3 Render-side inventory

Port v3's `latex2text/_defaultspecs.py` categories minus the dropped ones
(`latex-ethuebung`, `nonstandard-qit`), restructured per §11.1, with the fixes from
§4. Every v3 `simplify_repl` string maps to a `Literal` or named-`Template`; v3
callables map to handlers listed in §10.

### 11.4 Symbol-table generation (`tools/gen_symbols.py`)

A checked-in Python script (dev-only, not part of the build) reads pylatexenc's
tables (`latex2text/_defaultspecs.py` symbol lists, the Greek/accent builders, the
sub/superscript and font-alphabet tables) from a pylatexenc checkout and emits
checked-in Rust source (`defs/symbols_extra.rs` static tables, `mathfmt` tables,
`accents` composition pairs). Deduplicate (v3 has ~295 shadowed duplicates; last
wins), sort, and emit `static SYMBOLS: &[(&str, &str)]`. The generated files carry a
`// GENERATED by tools/gen_symbols.py — do not edit` header and are committed.

---

## 12. Unknown constructs and diagnostics (settled)

`techxt::diag` defines conditions with `techy::error::DiagnosticInfo` derive:
`UnknownMacro { name }`, `UnknownEnvironment { name }`, `UnknownSpecials { chars }`,
`HandlerFailed { name, detail }`, `UnsupportedFeatureIgnored { what }` (multicolumn,
enumitem options…), `InputNotResolved { name }`. Severity: warning (conversion
continues). Positions: the node's `SourceSpan`.

Policies (each an `Options` field):
```rust
pub enum UnknownMacroPolicy    { Skip /*default*/, RenderArgs, KeepSource, Placeholder }
pub enum UnknownEnvPolicy      { RenderBody /*default*/, Skip, KeepSource }
pub enum UnknownSpecialsPolicy { EmitChars /*default*/, Skip }
```
Every unknown hit emits its diagnostic regardless of policy. `KeepSource` uses the
payload-based `SourceRecomposer`. Parse-level diagnostics (from techy, when techxt
drives parsing) are merged into the returned `Diagnostics` ahead of render ones.

---

## 13. Public API (`techxt::convert`)

```rust
pub struct Converter { /* Arc<Language<Latexlike>>, rule tables, Options */ }  // Send + Sync
pub struct ConverterBuilder { … }

impl Converter {
    pub fn builder() -> ConverterBuilder;
    pub fn standard() -> Converter;                       // defs::standard() + default Options
    pub fn latex_to_text(&self, latex: &str) -> Result<Conversion, techy ParseError>;
    pub fn tree_to_text(&self, tree: &NodeTree<Latexlike>) -> Conversion;
    pub fn nodes_to_text(&self, nodes: NodeSlice<'_, Latexlike>) -> Conversion;
    pub fn tree_to_flow(&self, tree: &NodeTree<Latexlike>) -> (Flow, Diagnostics<…>);
    pub fn language(&self) -> &Language<Latexlike>;        // parse separately / reuse
}
pub struct Conversion { pub text: String, pub diagnostics: Diagnostics<Option<String>> }
```
`ConverterBuilder`: `definitions(DefinitionSet)`, `options(Options)` (or per-field
setters), `override_macro/environment/specials(name, TextRule)`,
`source_resolver(...)` (forwards to the driver), `recovery(Recovery)` (default
Tolerant; Strict makes `latex_to_text` return `Err` on first parse error).

```rust
#[non_exhaustive]
pub struct Options {
    pub math_mode: MathMode,                  // Fancy
    pub math_expression_in: MathWrapDelims,   // Parens
    pub wrap_width: Option<usize>,            // None
    pub keep_comments: bool,                  // false
    pub heading_style: HeadingStyle,          // NumberedUnderlined
    pub footnote_style: FootnoteStyle,        // Collected
    pub list_style: ListStyle,                // v3 markers
    pub text_font: FontStyle, pub math_font: FontStyle,   // Upright-default / Italic
    pub unknown_macro: UnknownMacroPolicy, pub unknown_env: UnknownEnvPolicy,
    pub unknown_specials: UnknownSpecialsPolicy,
    pub today: Option<Box<str>>,              // None → "<today>"
}
```
Internally, each conversion constructs a fresh `TextRenderer` (borrowing the
converter's tables) and runs `TreeRecomposer`. The `TextRenderer` itself is public
(flow layer) so consumers can wrap it per techy's wrapping contract.

---

## 14. CLI (`techxt-cli`, command `techxt`) — clap derive (settled)

```
techxt [OPTIONS] [FILE]        # FILE or stdin → stdout (or --output FILE)
  -o, --output <FILE>
      --math-mode <fancy|text|with-delimiters|verbatim|remove>
      --math-wrap <parens|braces|none>
  -w, --wrap <COLS>
      --keep-comments
      --heading-style <numbered-underlined|underlined|prefix|plain>
      --footnote-style <collected|inline|skip>
      --unknown-macro <skip|render-args|keep-source|placeholder>
      --input-dir <DIR>        # enables \input via a sandboxed fs resolver:
                               # realpath containment (dir + separator prefix),
                               # .tex/.latex extension fallback, include-cycle guard
      --strict                 # Recovery::Strict (default tolerant)
  -q / -v                      # diagnostics: -q none; default warnings+; -v notes too
```
Diagnostics print to stderr via techy's `render_all()` (line/col formatting included).
Exit code: 0 clean, 1 diagnostics-with-errors (tolerant), 2 hard parse/IO error.
Sets `Options::today` from the system clock (`%B %-d, %Y`-style English formatting is
fine, hand-rolled — no chrono dependency; implementer's choice on exact formatting).

---

## 15. Testing (settled strategy)

1. **Inline expected-string tests** (`assert_eq!(convert(input), expected)`) as the
   main body — unit tests per module + integration tests per feature area under
   `rust/techxt/tests/`.
2. **proptest** (dev-dependency, as in techy): layout invariants (§7), math joiner
   stability (joining is invariant under re-chunking of adjacent plain text; no
   double spaces; empty pieces drop), template parser round-trips, no-panic fuzzing
   of `Converter::standard().latex_to_text` over arbitrary strings (tolerant mode
   must never panic).
3. **Coverage checklist ported from pylatexenc** — recreate the *cases* of
   `pylatexenc/test/test_2_latex2text.py` (accents incl. `\"{o}`/`{\"o}`/`\L`
   combinations, `$a$$b$`, all math modes on the same sample, nested lists, spacing
   around bare macros, sub/superscripts incl. `∑ᵢ₌₁ⁿ` and fallbacks, matrices incl.
   the empty matrix, maketitle, `\input` sandboxing incl. the `dir-evil` sibling
   case, verbatim edge cases) with techxt's own expected outputs.
4. **Doctests** on all public API examples (enforced by `missing_docs` culture).
5. CLI smoke tests (run the binary on fixture files; assert stdout/stderr/exit code).

---

## 16. Milestones (implement in order; each ends green under the full CI)

- **M0 — scaffolding.** Repo layout §2, workspace, lint config, CI (GitHub Actions):
  `cargo fmt --check`, `clippy -D warnings` (all targets), `cargo test` (workspace),
  `cargo doc` with denied warnings, MSRV job (1.86), **no_std proof job**: build the
  lib crate for a std-less target (e.g. `--target thumbv7em-none-eabihf`; if techy's
  `Arc` needs atomics, this target has them). Empty lib compiles no_std; CLI prints
  version.
- **M1 — flow + layout.** §6, §7 complete with proptest invariants. No techy
  dependency needed by the tests (construct flows by hand).
- **M2 — renderer core.** `TextRenderer` over techy trees: chars/comment/group/list,
  paragraph breaks, downward state plumbing, run state, diagnostics channel, unknown
  policies, `Conversion` plumbing, tree/flow/convenience entry points with a
  hand-built 5-entry definition set. End-to-end: plain paragraphs, groups, comments.
- **M3 — definitions infrastructure.** §9 complete: def builders, techxt spec types,
  template parser/validator (with full error cases tested), dispatch chain,
  `DefinitionSet` → packages + parsing state + fallback table, override map.
- **M4 — base library.** `defs::{base, accents, fontstyles, sectioning, paragraphs,
  refs, links, graphics, titling, preamble, inputs}` + generation script §11.4 for
  accents/symbols data. Headings with counters/underlines; `\href`; maketitle.
- **M5 — math engine.** §10.5 complete (atoms, segmentation, joiner, scripts,
  wrappables, font alphabets, `\frac`/`\sqrt`, math envs, matrices, all five modes).
  This is the largest milestone; port systematically from v3 with tests at each step.
- **M6 — blocks.** Lists §10.4, verbatim §10.7, tables §10.6, footnotes, theorems,
  quote/center blocks, `\input` rendering incl. resolver-configured integration test.
- **M7 — CLI.** §14 + smoke tests + `defs::standard()` audit (run the CLI over a
  realistic sample paper; eyeball + freeze as integration expectations).
- **M8 — polish & release prep.** `symbols_extra` + `natbib` long tail, crate-level
  narrative docs (techy-style guide: quick start, extending with custom definitions,
  writing a handler, wrapping the recomposer, layout invariants), README(s),
  CHANGELOG, version 0.1.0. Do not publish to crates.io until techy is published
  (git dependency blocks publishing) — leave a note in the README.

## 17. Deliberate omissions / future work (documented, not implemented in v1)

Label/`\ref` resolution (two-pass); `\newcommand` expansion; multicolumn/multirow
spans and width-budgeted table cells; centering simulation; SourceLike whitespace
mode; pylatexenc-compat output mode; localization of generated words ("Theorem",
"Hint"); streaming (incremental) layout; serde support; Python (`python/`, maturin)
and JS/wasm (`js/`) sibling bindings — the module-based defs organization (§11.1)
was chosen with wasm dead-code elimination in mind.
