//! # techxt
//!
//! Convert LaTeX-like markup to plain (unicode) text.
//!
//! `techxt` reads a document written in a LaTeX-like language, parses it with the
//! [`techy`] parser, and renders it as human-readable plain text: `\emph{hi}` becomes
//! `hi`, `\"o` becomes `ö`, `\frac{1}{2}` becomes `(1/2)`, `itemize` becomes a bulleted
//! list, and a `tabular` becomes an aligned text table. The output is meant to be read
//! by a person or fed to tools that want text rather than markup — search indexes,
//! terminals, plain-text mail, screen readers.
//!
//! It is a from-scratch redesign of the capabilities of
//! [`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc); pylatexenc is an
//! idea source, not a compatibility target, and techxt deliberately deviates wherever
//! pylatexenc has quirks or structural weaknesses.
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
//! ## Crate dependencies and panic policy
//!
//! **no_std + alloc:** the crate depends on `core` and `alloc` only, and builds for
//! std-less targets (CI proves this on `thumbv7em-none-eabihf`). As in [`techy`], shared
//! objects use `Arc`, so the target must support atomics. All I/O — reading files for
//! `\input`, writing the result — belongs to the embedder; the `techxt` command-line
//! program in the `techxt-cli` crate is one such embedder.
//!
//! **Minimal dependencies:** the runtime dependencies are exactly [`techy`] (the parser)
//! and [`unicode-width`](https://docs.rs/unicode-width) (display width of the rendered
//! text, for column accounting in the layout engine). There are no cargo features: the
//! definition library is plain Rust modules that a user references explicitly, so a
//! build that never mentions a module drops it through dead-code elimination.
//!
//! **No-panic policy:** this library never panics on document input, no matter how
//! malformed. A caller contract violation (for instance an out-of-range index passed to
//! a public accessor) may panic, following the usual Rust conventions.
//!
//! ## Quick start
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! let conversion = converter.latex_to_text("Hello  {brave}\n world.")?;
//! assert_eq!(conversion.text, "Hello brave world.\n");
//! # Ok::<(), techy::error::ParseError<Option<String>>>(())
//! ```
//!
//! One [`Converter`] converts any number of documents, from any number of threads.
//! Build a configured one with [`Converter::builder`], and read
//! [`Conversion::diagnostics`] for everything the conversion could not do faithfully.
//!
//! ## Modules
//!
//! - [`convert`] — the public conversion API: [`Converter`], [`ConverterBuilder`],
//!   [`Options`], [`Conversion`].
//! - [`render`] — the renderer that folds a [`techy`] node tree into a flow, its
//!   downward state, and the context a rule's handler works through.
//! - [`def`] — what techxt knows about a construct: its text rule, and the spec types
//!   that carry that rule into the parsed tree.
//! - [`flow`] — the typed token sequence handlers produce: words, breakable spaces,
//!   vertical separation requests, blocks, verbatim runs.
//! - [`layout`] — the single pass that turns a flow into text, deciding every line
//!   break, blank line and indent.
//! - [`mathfmt`] — the atom model the math engine composes formulas from.
//! - [`diag`] — the conditions a conversion reports.
//!
//! ## Status
//!
//! Early development, built up milestone by milestone. The conversion pipeline, the
//! layout engine and the public API are in place; what is not yet is the *content*:
//! the definitions library (so the set of macros and environments techxt knows is a
//! deliberately small placeholder) and the math engine (so formulas render as their
//! characters rather than as unicode math, except in
//! [`MathMode::Source`](crate::convert::MathMode::Source), which is complete).

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod convert;
pub mod def;
pub mod diag;
pub mod flow;
pub mod layout;
pub mod mathfmt;
pub mod render;

pub use convert::{Conversion, Converter, ConverterBuilder, Options};
