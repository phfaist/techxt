//! The template mini-language (PLAN.md §10.5) — **the parser is milestone M3**.
//!
//! A template is the data form of "this construct renders as that text, with these
//! arguments dropped into it": `\href{url}{text}` is `"{text} <{url}>"`. Templates are
//! parsed and validated once, when the converter is built, against the argument names
//! the definition declares, so a template that names an argument that does not exist is
//! a build error rather than a silent hole in the output.
//!
//! # What exists today
//!
//! The renderer executes templates already — [`Template`]'s segment model and its
//! evaluation are complete. What M3 adds is the *parser* that turns the source syntax
//!
//! ```text
//! template   := ( literal | escape | ref | cond )*
//! escape     := "{{" | "}}"
//! ref        := "{" name "}" | "{" integer "}"
//! cond       := "{?" name ":" branch ( "|" branch )? "}"
//! ```
//!
//! into that model, together with its validation against the entry's argument names,
//! plus the `{?name:then|else}` conditional — the one piece of the model that is not
//! here yet, because nothing can build one until the parser exists. Until then a
//! [`Template`] can only be built inside techxt (there is no public constructor), which
//! is why [`TemplateError`] has no producer yet.

use alloc::boxed::Box;
use alloc::vec::Vec;

/// Replacement text with holes for a construct's arguments (PLAN.md §10.5).
///
/// A template is the data form of "this construct renders as that text, with these
/// arguments dropped into it": `\href{url}{text}` is `"{text} <{url}>"`. Templates are
/// compiled and validated once, when the converter is built, against the argument names
/// the definition declares, so a template naming an argument that does not exist is a
/// [build error](crate::convert::BuildError::Template) rather than a silent hole in
/// somebody's output.
///
/// **M3 seam:** the renderer executes templates already — the segment model and its
/// evaluation are complete. What the definitions milestone adds is the *parser* for the
/// source syntax, its validation, and the `{?name:then|else}` conditional. Until then
/// the type has no public constructor, which is also why [`TemplateError`] has no
/// producer yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub(crate) segments: Vec<Seg>,
}

impl Template {
    /// A template made of ready-made segments.
    ///
    /// Crate-internal until M3 ships the parser: the source syntax, not a segment
    /// vector, is the intended way to write one.
    pub(crate) fn from_segments(segments: Vec<Seg>) -> Template {
        Template { segments }
    }

    /// The segments, in order.
    pub(crate) fn segments(&self) -> &[Seg] {
        &self.segments
    }

    /// The template `"{name}"`: one named argument's content and nothing else.
    ///
    /// A stand-in for M3's parser, so that the renderer's template path can be built
    /// and tested before the source syntax exists.
    pub(crate) fn named_argument(name: &str) -> Template {
        Template::from_segments(alloc::vec![Seg::Arg(ArgRef::Name(name.into()))])
    }

    /// The template `"{index}"`: one positional argument's content, 1-based.
    pub(crate) fn positional_argument(index: usize) -> Template {
        Template::from_segments(alloc::vec![Seg::Arg(ArgRef::Index(index))])
    }

    /// The template `"{body}"`: an environment's body and nothing else.
    pub(crate) fn body() -> Template {
        Template::from_segments(alloc::vec![Seg::Body])
    }

    /// The template `"{text} <{url}>"`, which is how `\href` reads as plain text.
    pub(crate) fn link() -> Template {
        Template::from_segments(alloc::vec![
            Seg::Arg(ArgRef::Name("text".into())),
            Seg::Str(" <".into()),
            Seg::Arg(ArgRef::Name("url".into())),
            Seg::Str(">".into()),
        ])
    }
}

/// One piece of a [`Template`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Seg {
    /// Literal text.
    Str(Box<str>),
    /// The content of one argument, empty when the argument is absent.
    Arg(ArgRef),
    /// The environment's body (rejected at build time for macros and specials).
    Body,
}

/// How a [`Seg`] names an argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArgRef {
    /// By the name the definition gave it.
    Name(Box<str>),
    /// By 1-based position in the declaration order.
    Index(usize),
}

/// Why a template could not be compiled (PLAN.md §10.5).
///
/// Templates are validated when the converter is built, so these surface as
/// [`BuildError::Template`](crate::convert::BuildError::Template) and never at
/// conversion time.
///
/// **M3 seam:** the template parser that produces these lands with the definitions
/// milestone; the variants are the ones PLAN.md §10.5 enumerates.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// `{foo}`, where the definition declares no argument named `foo`.
    UnknownArgumentName {
        /// The name the template used.
        name: Box<str>,
    },
    /// `{0}`, or `{3}` on a two-argument definition. Indices are 1-based.
    ArgumentIndexOutOfRange {
        /// The index the template used.
        index: usize,
        /// How many arguments the definition declares.
        count: usize,
    },
    /// `{body}` in a macro's or a specials' template: only environments have a body.
    BodyOutsideEnvironment,
    /// A conditional inside a conditional's branch, which the grammar does not allow.
    NestedConditional,
    /// A `{…}` reference or a `{?…}` conditional that is never closed.
    UnterminatedConstruct,
}

impl core::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TemplateError::UnknownArgumentName { name } => {
                write!(f, "no argument named ‘{name}’")
            }
            TemplateError::ArgumentIndexOutOfRange { index, count } => write!(
                f,
                "argument index {index} is out of range (1..={count}; indices are 1-based)"
            ),
            TemplateError::BodyOutsideEnvironment => {
                write!(
                    f,
                    "‘{{body}}’ is only available in an environment's template"
                )
            }
            TemplateError::NestedConditional => {
                write!(f, "a conditional may not contain another conditional")
            }
            TemplateError::UnterminatedConstruct => {
                write!(f, "unterminated ‘{{…}}’ construct")
            }
        }
    }
}

impl core::error::Error for TemplateError {}
