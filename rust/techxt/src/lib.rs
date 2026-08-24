//! # techxt
//!
//! Convert LaTeX-like markup to plain (unicode) text.
//!
//! `techxt` reads a document written in a LaTeX-like language, parses it with the
//! [`techy`] parser, and renders it as human-readable plain text: `\emph{hi}` becomes
//! `ℎ𝑖`, `\"o` becomes `ö`, `\section{Intro}` becomes a numbered heading over its
//! underline, an `itemize` becomes a bulleted list, a `tabular` becomes an aligned
//! text table, and `$\sum_{i=1}^n x_i$` becomes `∑ᵢ₌₁ⁿ 𝑥ᵢ`. The output is meant to be
//! read by a person or fed to tools that want text rather than markup — search
//! indexes, terminals, plain-text mail, screen readers.
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! assert_eq!(converter.latex_to_text(r"$\sum_{i=1}^n x_i$")?.text, "∑ᵢ₌₁ⁿ 𝑥ᵢ\n");
//! assert_eq!(converter.latex_to_text(r"$\frac{a+b}{2}$")?.text, "(𝑎 + 𝑏)/2\n");
//! # Ok::<(), techxt::convert::ParseError<Option<String>>>(())
//! ```
//!
//! It is a from-scratch redesign of the capabilities of
//! [`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc); pylatexenc is an
//! idea source, not a compatibility target, and techxt deliberately deviates wherever
//! pylatexenc has quirks or structural weaknesses.
//!
//! ## Quick start
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! let conversion = converter.latex_to_text("Hello  {brave}\n world.")?;
//! assert_eq!(conversion.text, "Hello brave world.\n");
//! assert!(conversion.diagnostics.is_empty());
//! # Ok::<(), techxt::convert::ParseError<Option<String>>>(())
//! ```
//!
//! One [`Converter`] converts any number of documents, from any number of threads.
//! Build a configured one with [`Converter::builder`] — every [`Options`] field has a
//! setter on it — and read [`Conversion::diagnostics`] for everything the conversion
//! could not do faithfully:
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::builder()
//!     .wrap_width(Some(24))
//!     .keep_comments(true)
//!     .build()?;
//! let conversion = converter.latex_to_text("aaa bbb \\textbf{ccc ddd} eee % here\nfff")?;
//! assert_eq!(conversion.text, "aaa bbb 𝐜𝐜𝐜 𝐝𝐝𝐝 eee\n% here\nfff\n");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! ## Design in one page
//!
//! **Layout is a first-class concern, separate from conversion.** Handlers never
//! concatenate raw newlines to fake vertical spacing. Instead they emit a typed *flow*
//! of tokens (words, breakable spaces, block boundaries, verbatim runs), and a single
//! layout engine turns that flow into a string. Line wrapping, blank-line counts
//! between blocks, list indentation and verbatim protection are therefore decided in
//! exactly one place, from complete information, rather than per handler:
//!
//! ```text
//!  input &str ──► techy Language::parse ──► NodeTree + parse diagnostics
//!                                               │
//!         NodeTree (possibly user-transformed) ──► TextRenderer
//!                                               ▼
//!                                          Flow (typed tokens)
//!                                               │
//!                                          layout engine
//!                                               ▼
//!                                     String + diagnostics
//! ```
//!
//! **One definition, both sides.** A single definition entry carries both the parsing
//! argument structure and the text rule that renders it, so the parser and the renderer
//! can never disagree about what a macro's arguments are.
//!
//! **Nothing silent.** Unknown constructs, skipped content and handler problems produce
//! structured diagnostics carrying source positions, alongside the converted text.
//!
//! **Reusable, immutable converter.** A converter is `Clone + Send + Sync` and holds no
//! per-document state; all per-run state lives in a separate structure, so one converter
//! can convert many documents concurrently.
//!
//! All three layers are public API: the convenience layer (string to string), the tree
//! layer (convert a [`techy`] node tree you already have, including one you transformed
//! yourself), and the flow/layout layer (build flow tokens directly and run the layout
//! engine on them).
//!
//! ## What the layout engine guarantees
//!
//! These hold for every conversion, whatever the definitions did — they are properties
//! of [`layout::render`], the one pass that turns a [`Flow`](flow::Flow) into text
//! (PLAN.md §7):
//!
//! - **At most one blank line anywhere**, never at the start, never at the end, and a
//!   nonempty output always ends with exactly one `\n`. Vertical separation is
//!   *requested* by handlers and *merged* here — consecutive requests take the
//!   strongest rather than adding up — so no combination of neighbouring constructs can
//!   produce a gap of two blank lines or lose the separation between two blocks.
//! - **No line ends in collapsible whitespace.** Inter-word space is glue, and glue at
//!   the end of a line is dropped rather than written. Raw content is exempt by
//!   construction, and only raw content: `\verb| x |` keeps its space because a
//!   verbatim payload is emitted byte for byte.
//! - **Verbatim survives byte for byte.** A verbatim block is never wrapped, trimmed,
//!   re-indented or styled, and it is always separated from its surroundings by a blank
//!   line.
//! - **Wrapping never breaks a word, and a macro boundary is not a word boundary.**
//!   `\textbf{ccc ddd}` is two words, wrappable between them; `\textbf{bold}text` is
//!   one word and stays whole. With [`Options::wrap_width`] set, no line exceeds it
//!   unless a single unbreakable word does.
//! - **Indentation is composed, not concatenated.** Block prefixes nest, count toward
//!   the wrap column, and a list item's marker is the first line's prefix — so a
//!   wrapped item's continuation lines align under its text.
//!
//! ## Extending: definitions of your own
//!
//! A [`DefinitionSet`](def::DefinitionSet) is a stack of [`Category`](def::Category)
//! values, and **later categories shadow earlier ones**. So adding to (or correcting)
//! the shipped library is a matter of pushing one more category after
//! [`defs::standard`]:
//!
//! ```
//! use techxt::def::{Category, MacroDef, Template, TextRule};
//! use techxt::{defs, Converter};
//!
//! let mine = Category::new("mine")
//!     // One entry, both sides: `{word}` is how it parses *and* what the rule names.
//!     .with_macro(
//!         MacroDef::new("keyword")
//!             .arg("m", "word")
//!             .rule(TextRule::Template(Template::new("‹{word}›"))),
//!     )
//!     .with_macro(MacroDef::symbol("ohm", "Ω"))
//!     // An environment renders its body with `TextRule::Content`.
//!     .with_env(techxt::def::EnvDef::new("aside").rule(TextRule::Content));
//!
//! let converter = Converter::builder()
//!     .definitions(defs::standard().with(mine))
//!     .build()?;
//! assert_eq!(
//!     converter.latex_to_text(r"a \keyword{macro}, 5\,\ohm, \begin{aside}note\end{aside}")?.text,
//!     "a ‹macro›, 5 Ω, note\n",
//! );
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! Assemble a set from single modules ([`defs::base`], [`defs::accents`], …) when the
//! shipped library is more than the document needs; a build that never mentions a
//! module drops it. To change one construct without touching a category, use
//! [`ConverterBuilder::override_macro`] and its siblings, which sit at the front of the
//! dispatch chain.
//!
//! ## Extending: writing a handler
//!
//! [`TextRule::Handler`](def::TextRule::Handler) runs code. A handler is handed a
//! [`NodeView`](render::NodeView) of the node and a [`RenderCx`](render::RenderCx),
//! which is its whole interface to the fold:
//! it reads arguments *through* the context (so nested constructs keep working), reads
//! the downward [`RenderState`](render::RenderState) and the [`Options`], raises
//! diagnostics, and registers footnotes and document metadata. It must be `Send + Sync`
//! and hold no per-document state — the converter is shared across threads — and it
//! returns a [`Flow`](flow::Flow), never a string with newlines in it.
//!
//! ```
//! use std::sync::Arc;
//!
//! use techxt::def::{Category, MacroDef, TextHandler, TextRule};
//! use techxt::flow::{Flow, FlowItem};
//! use techxt::render::{NodeView, RenderCx, RenderError};
//! use techxt::{defs, Converter};
//!
//! /// `\twice{ho}` → `ho ho`.
//! #[derive(Debug)]
//! struct Twice;
//!
//! impl TextHandler for Twice {
//!     fn render(
//!         &self,
//!         _node: NodeView<'_>,
//!         cx: &mut RenderCx<'_, '_>,
//!     ) -> Result<Flow, RenderError> {
//!         // Rendering the argument through the context is what makes `\twice{\emph{x}}`
//!         // work: it folds with the renderer, under the state in force here.
//!         let once = cx.arg("text")?.unwrap_or_default();
//!         let mut flow = once.clone();
//!         // A *breakable* space, not the character: where the line may break is the
//!         // layout engine's decision, and this is how a handler asks for one.
//!         flow.push(FlowItem::Glue);
//!         flow.extend(once);
//!         Ok(flow)
//!     }
//! }
//!
//! let mine = Category::new("mine").with_macro(
//!     MacroDef::new("twice")
//!         .arg("m", "text")
//!         .rule(TextRule::Handler(Arc::new(Twice))),
//! );
//! let converter = Converter::builder()
//!     .definitions(defs::standard().with(mine))
//!     .build()?;
//! assert_eq!(converter.latex_to_text(r"\twice{\emph{ho}}")?.text, "ℎ𝑜 ℎ𝑜\n");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! ## Extending: wrapping the recomposer
//!
//! [`TextRenderer`](render::TextRenderer) is techxt's implementation of techy's
//! `Recomposer` trait, and it is public because consumers are meant to build on it:
//! write a recomposer that handles the nodes you care about and delegates the rest to
//! techxt's. Drive it yourself, and finish by appending what the run collected:
//!
//! ```
//! use techxt::convert::TreeRecomposer;
//! use techxt::layout::{render, LayoutOptions};
//! use techxt::render::RenderState;
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! let tree = converter.language().parse(r"\section{Intro}").expect("parses").tree;
//!
//! let mut renderer = converter.renderer();
//! let mut flow = TreeRecomposer::new(&mut renderer)
//!     .recompose(&tree, RenderState::initial(converter.options()))
//!     .expect("no descent-limit refusal");
//! // The footnote block, and anything else collected during the run, comes last.
//! flow.extend(renderer.finish().trailing);
//!
//! assert_eq!(render(&flow, &LayoutOptions::default()), "1 Intro\n-------\n");
//! ```
//!
//! [`render`] documents how far a wrapper's overrides reach — and the one place
//! they do not, which is inside a construct techxt's own rule renders.
//!
//! ## Crate dependencies and panic policy
//!
//! **no_std + alloc:** the crate depends on `core` and `alloc` only, and builds for
//! std-less targets (CI proves this on `thumbv7em-none-eabihf`). As in [`techy`], shared
//! objects use `Arc`, so the target must support atomics. All I/O — reading files for
//! `\input`, writing the result — belongs to the embedder; the `techxt` command-line
//! program in the `techxt-cli` crate is one such embedder.
//!
//! **Minimal dependencies:** the runtime dependencies are exactly [`techy`] (the parser),
//! [`techy_xp`] (techy's latexlike preset carrying macro expansion, PLAN.md §16 M9) and
//! [`unicode-width`](https://docs.rs/unicode-width) (display width of the rendered
//! text, for column accounting in the layout engine). techy-xp adds no crate to the tree
//! that techy had not already put there. There are no cargo features: the
//! definition library is plain Rust modules that a user references explicitly, so a
//! build that never mentions a module drops it through dead-code elimination.
//!
//! **One dependency for an embedder:** every upstream type that appears in techxt's
//! public API — [`Diagnostics`](convert::Diagnostics), [`Severity`](convert::Severity),
//! [`Recovery`](convert::Recovery), [`SourceResolver`](convert::SourceResolver),
//! [`NodeRef`](convert::NodeRef), [`TreeRecomposer`](convert::TreeRecomposer), techy-xp's
//! [`LatexlikeXp`](convert::LatexlikeXp) and the rest — is re-exported from [`convert`].
//! They are the upstream crates' own types, not wrappers, so naming `techy` or
//! `techy_xp` directly stays exactly equivalent.
//!
//! **No-panic policy:** this library never panics on document input, no matter how
//! malformed. A caller contract violation (for instance an out-of-range index passed to
//! a public accessor) may panic, following the usual Rust conventions.
//!
//! ## Modules
//!
//! - [`convert`] — the public conversion API: [`Converter`], [`ConverterBuilder`],
//!   [`Options`], [`Conversion`], and the techy types they name.
//! - [`render`] — the renderer that folds a [`techy`] node tree into a flow, its
//!   downward state, and the context a rule's handler works through.
//! - [`def`] — what techxt knows about a construct: the entry builders a definitions
//!   library is written with, the template mini-language, the rule model, and the spec
//!   types that carry a rule into the parsed tree.
//! - [`defs`] — the definitions library itself: one module per category, and
//!   [`defs::standard`] stacking all of them.
//! - [`flow`] — the typed token sequence handlers produce: words, breakable spaces,
//!   vertical separation requests, blocks, verbatim runs.
//! - [`layout`] — the single pass that turns a flow into text, deciding every line
//!   break, blank line and indent.
//! - [`mathfmt`] — the atom model the math engine composes formulas from.
//! - [`diag`] — the conditions a conversion reports.
//!
//! ## Status
//!
//! Version 0.1.0, feature-complete against its plan and green under the full gate set
//! (rustfmt, clippy, an MSRV build, a `no_std` build, and the test suite). Everything
//! PLAN.md specifies is implemented: the layout engine and the flow model; the
//! renderer, its state and the diagnostics channel; the definitions model with its
//! template mini-language; and the whole definitions library — escapes and spacing,
//! accents, font styles, sectioning, lists, verbatim, tables, theorems and the other
//! block environments, footnotes, cross-references, citations (including natbib),
//! links, graphics, titling, the preamble and the `document` environment, `\input` with
//! a caller-supplied resolver, and the generated ~1000-entry symbol table.
//!
//! **Macro definitions are honoured** (PLAN.md §16 M9): `\newcommand`, `\renewcommand`,
//! `\providecommand`, `\newenvironment`, `\renewenvironment`, `\def`, `\gdef`, `\let`,
//! `\edef`, `\xdef` and `\NewDocumentCommand` define, and a later use of the defined
//! macro expands, through [`techy_xp`]'s token reader. Definitions are scoped as TeX
//! scopes them, and
//! [`ConverterBuilder::macro_definitions`](convert::ConverterBuilder::macro_definitions)
//! turns the whole of it off. See [`defs::preamble`].
//!
//! Expansion is bounded rather than cycle-detected, by two budgets a caller may raise:
//! [`expansion_depth_limit`](convert::ConverterBuilder::expansion_depth_limit) and
//! [`expansion_count_limit`](convert::ConverterBuilder::expansion_count_limit). Both
//! default lower than techy-xp's own, because a converter pointed at documents it did
//! not write cannot afford what a runaway definition costs at the upstream allowance.
//!
//! Mathematics is complete too, in all three
//! [modes](convert::MathMode): the atom model with TeX's spacing classes, sub- and
//! superscripts through unicode's script characters, `\frac` and `\sqrt`, the display
//! environments, and matrices — inline, and as multi-line boxes with delimiters drawn
//! to height.
//!
//! **Not published to crates.io**, and not publishable while [`techy`] and [`techy_xp`]
//! are git dependencies: a release on crates.io cannot carry one. Publication waits for
//! their own releases.
//!
//! ## Deliberate omissions
//!
//! Documented rather than half-implemented (PLAN.md §17). Each is noted again where a
//! reader meets it:
//!
//! - **Label and reference resolution.** `\ref`, `\eqref` and `\cite` render markers
//!   (`<ref>`, `<cit.>`) rather than numbers: resolving them needs a second pass over
//!   the document, and citations need the bibliography besides. See [`defs::refs`].
//! - **TeX's conditionals, category codes, registers and counters.** `\ifx`, `\catcode`,
//!   `\count` and `\setcounter` are read and reported rather than acted on — techy-xp
//!   declines them by design, and a document reaching one is told which feature is
//!   missing rather than left to guess. `\DeclareMathOperator` is likewise declared and
//!   dropped: the operator it names is not defined. See [`defs::preamble`].
//! - **Numbering beyond headings.** Theorems, figures, tables and equations are not
//!   numbered, and counters (`\arabic{page}`) render as nothing; heading numbers are
//!   the one exception. See [`defs::theorems`].
//! - **Table spans and widths.** `\multicolumn` and `\multirow` render their contents
//!   in one cell, and a `p{3cm}` column is as wide as its widest line. See
//!   [`defs::tables`].
//! - **Centring.** `center` is an ordinary block: padding lines with spaces would
//!   misalign at any other window width and would put trailing whitespace on every
//!   line. See [`defs::theorems`].
//! - **Localization.** The words techxt generates — "Theorem", "Abstract", "Figure:",
//!   the "and" between two authors — are English. A definition of your own overrides
//!   any of them.
//! - **Serde.** No serialization of options, flows or diagnostics, and no cargo
//!   feature for one.
//! - **Subtree conversion.** There is no entry point that converts a
//!   [`NodeSlice`](convert::NodeSlice): techy's recomposer takes a whole tree, and its
//!   context has no public constructor. Convert the tree, or drive the renderer
//!   yourself as shown above.
//! - **Generalization over the language.** Every type here is concrete over techy-xp's
//!   `LatexlikeXp` — techy's latexlike preset plus the expanding reader the definers of
//!   [`defs::preamble`] write into — and over the `()` tree annotation.
//! - **Python and JavaScript bindings.** Planned as sibling folders (`python/`, `js/`)
//!   of the repository root; the module-based organization of [`defs`] was chosen with
//!   wasm dead-code elimination in mind.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod convert;
pub mod def;
pub mod defs;
pub mod diag;
pub mod flow;
pub mod layout;
pub mod mathfmt;
pub mod render;

pub use convert::{Conversion, Converter, ConverterBuilder, Options};
