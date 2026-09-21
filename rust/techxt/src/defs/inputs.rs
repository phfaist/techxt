//! File inclusion: `\input` and `\include`.
//!
//! Inclusion happens at **parse** time, not here. techy resolves the reference through
//! the [source resolver](crate::ConverterBuilder::source_resolver) the converter was
//! built with, parses the resolved file into the same tree, and hangs it on the
//! invocation as a slot named `attached`. The handler in this module renders that slot
//! — which is the whole of it, because the included content is already ordinary nodes
//! by the time the renderer sees them.
//!
//! The slot is deliberately *not* part of the node's children, so nothing renders it by
//! accident; a handler has to ask.
//!
//! # Two ways to register the same macro
//!
//! Resolution is techy's, and techy performs it only for a macro whose spec is an
//! [`InputMacroSpec`] — no argument declaration can ask for it. So this category
//! registers `\input` through [`MacroDef::spec`], the escape hatch for a spec techxt does
//! not model, and the spec it hands over depends on how the converter
//! was configured:
//!
//! - **with a source resolver**: techy's inclusion spec, which resolves the reference at
//!   parse time and attaches what it finds;
//! - **without one**: techxt's ordinary spec, which parses the reference and no more.
//!   Registering the inclusion spec anyway would make every `\input` in the document
//!   diagnose a missing resolver — an error, and a refusal to parse at all under
//!   [`Recovery::Strict`](techy::error::Recovery::Strict) — for a converter that simply
//!   was not asked to include anything.
//!
//! A foreign spec carries no rule, so `\input`'s rule is found by name at dispatch step 3
//! rather than by downcast at step 2; the fallback table
//! [`DefinitionSet`](crate::def::DefinitionSet) builds holds it either way.
//!
//! # The reference is a group, so a macro in it expands
//!
//! techy's [`input_macro_spec`](techy::latexlike::input_macro_spec) recipe declares the
//! reference as a *chars-group*: the braces are read raw, commands and specials off, so
//! `\input{\name}` asks the resolver for the literal `\name`. That is the right reading
//! for an embedder without macro expansion, and the wrong one here — techxt parses
//! through techy-xp, where a `\newcommand` is expanded as the tokens are read, and a file
//! name assembled from a macro is an ordinary thing to write.
//!
//! So the spec is built with [`InputMacroSpec::new`] and a reference argument declared
//! `BracedOnly`, the same word code the fallback declaration uses. Both paths therefore
//! agree on the argument's shape — a `{…}` group, contents parsed as content, and **no**
//! single-token fallback, so `\input chapter.tex` is a missing mandatory argument rather
//! than an inclusion of the file `c`.
//!
//! What the chars-group would have bought — an underscore in `\input{my_file.tex}`
//! surviving as a character — techxt already has without it: `_` outside math stages as
//! plain characters here, so the reference reads exactly as written either way.
//!
//! # An included file's definitions end with the file
//!
//! techy's inclusion spec takes a mandatory `persist_state` decision, and techxt's answer
//! is **state-transparent**: the parsing state at the `\input` governs how the included
//! content is read, and nothing the included file changes continues past it. Definitions
//! travel *in*, and not back *out*:
//!
//! | written | what happens |
//! |---|---|
//! | `\newcommand\greet[1]{Hi #1}` before the `\input`, `\greet{x}` inside the included file | the macro expands inside the file |
//! | `\newcommand\greet[1]{Hi #1}` **inside** the included file, `\greet{x}` after it | `\greet` is an unknown macro |
//! | `\gdef\x{a}` inside the included file, `\x` after it | the same: a `\gdef` escapes every *group*, and a file is not a group |
//!
//! Until techxt honoured the defining commands nothing could observe this.
//! It can now, and the answer is a real limitation: `\input{macros.tex}` in a preamble is
//! how many real documents keep their definitions. LaTeX's own `\input` opens no group
//! and so persists; techxt does not follow it here, because `persist_state: true` would
//! carry *every* state change out of the file — a mode left in math, a group left open —
//! into the rest of the document. Making it a choice is future work;
//! `tests/defs_macros.rs` pins what is true today.
//!
//! # An unresolved include is a note, not an error
//!
//! Resolution is opt-in twice over: the converter needs a resolver, and the resolver
//! has to answer. When either half is missing the invocation is staged with no attached
//! slot at all — techy reports the lookup it attempted — and this handler renders
//! nothing and raises [`InputNotResolved`], a *note*.
//! Converting without a resolver is a perfectly ordinary configuration, and the missing
//! slot is not a failure of anything: it is the absence of a feature nobody turned on.

use alloc::string::String;
use alloc::sync::Arc;

use techy::core::specs::CallableSpec;
use techy::latexlike::{argument_specs_named, BodyMarker, InputMacroSpec};
use techy_xp::lang::LatexlikeXp;

use crate::def::{CallableSpecSource, Category, MacroDef, SpecBuildCx, TextHandler};
use crate::diag::InputNotResolved;
use crate::flow::Flow;
use crate::render::{NodeView, RenderCx, RenderError};

use super::handler;

/// What the inclusion macros call their argument.
const FILENAME: &str = "filename";

/// The inputs category.
pub fn category() -> Category {
    let mut category = Category::new("inputs");
    for name in ["input", "include"] {
        category.add_macro(
            MacroDef::new(name)
                // `BracedOnly`, not `m`: a file name is machine text, and TeX's
                // single-expression fallback would let `\input chapter.tex` swallow
                // one character and leave the rest in the document.
                .arg("BracedOnly", FILENAME)
                .spec(Resolving)
                .rule(handler(Include)),
        );
    }
    category
}

/// Register techy's inclusion spec, but only for a converter that can resolve.
#[derive(Debug)]
struct Resolving;

impl CallableSpecSource for Resolving {
    fn callable_spec(&self, cx: &SpecBuildCx) -> Option<Arc<dyn CallableSpec<LatexlikeXp>>> {
        cx.source_resolver()?;
        // `InputMacroSpec::new`, not the `input_macro_spec` recipe: the recipe declares
        // the reference as a *chars-group*, which reads the braces raw and so never lets
        // techy-xp expand what is inside them — `\input{\name}` would ask the resolver
        // for the literal `\name`. `BracedOnly` is the same code the fallback
        // declaration above uses, so both paths now say the same thing about the
        // argument: a `{…}` group, no single-token fallback, contents parsed as content.
        // The name is techy's to choose — the reference is read from the argument called
        // `REFERENCE_ARGUMENT_NAME` — where the fallback spec is free to call it
        // `filename`.
        let arguments = argument_specs_named::<LatexlikeXp, _, _, _>([(
            "BracedOnly",
            InputMacroSpec::<LatexlikeXp>::REFERENCE_ARGUMENT_NAME,
        )])
        .expect("`BracedOnly` is one of techy's word codes");
        Some(Arc::new(
            InputMacroSpec::new(
                arguments,
                // State-transparent (`persist_state: false`): every state change an
                // included file makes ends with the file. Now that the defining commands
                // are honoured that is **observable**, and the module documentation's
                // *An included file's definitions end with the file* says exactly what it
                // costs.
                false,
                // Not the node's body: the attached content is retrieved by slot name,
                // and marking it as a body would make every generic body walk descend
                // into a whole included file.
                BodyMarker::not_body(),
            )
            .expect("the one declared argument carries the reference name"),
        ))
    }
}

/// Render the content the parser attached, or report that there was none.
#[derive(Debug)]
struct Include;

impl TextHandler for Include {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        if let Some(content) = cx.attached()? {
            return Ok(content);
        }
        let target = target(cx);
        cx.report(InputNotResolved::new(target));
        Ok(Flow::new())
    }
}

/// The reference as the document wrote it, for the diagnostic.
///
/// techxt's own definition names the argument `filename`; techy's inclusion spec — the
/// one this category registers whenever a resolver is configured — names it `reference`.
/// Falling back to the first argument covers both without this handler having to guess
/// at names.
fn target(cx: &mut RenderCx<'_, '_>) -> String {
    if let Ok(Some(text)) = cx.arg_text(FILENAME) {
        return text;
    }
    match cx.arg_at(0) {
        Ok(Some(flow)) => crate::layout::render_inline(&flow),
        _ => String::new(),
    }
}
