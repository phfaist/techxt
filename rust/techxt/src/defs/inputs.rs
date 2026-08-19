//! File inclusion (PLAN.md §9.8): `\input` and `\include`.
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
//! # An unresolved include is a note, not an error
//!
//! Resolution is opt-in twice over: the converter needs a resolver, and the resolver
//! has to answer. When either half is missing the invocation is staged with no attached
//! slot at all — techy reports the lookup it attempted — and this handler renders
//! nothing and raises [`InputNotResolved`], a *note*.
//! Converting without a resolver is a perfectly ordinary configuration, and the missing
//! slot is not a failure of anything: it is the absence of a feature nobody turned on
//! (DECISIONS.md C4).

use alloc::string::String;

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::def::{Category, MacroDef, TextHandler};
use crate::diag::InputNotResolved;
use crate::flow::Flow;
use crate::render::{RenderCx, RenderError};

use super::handler;

/// What the inclusion macros call their argument.
const FILENAME: &str = "filename";

/// The inputs category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("inputs");
    for name in ["input", "include"] {
        category.add_macro(
            MacroDef::new(name)
                // `BracedOnly`, not `m`: a file name is machine text, and TeX's
                // single-expression fallback would let `\input chapter.tex` swallow
                // one character and leave the rest in the document.
                .arg("BracedOnly", FILENAME)
                .rule(handler(Include)),
        );
    }
    category
}

/// Render the content the parser attached, or report that there was none.
#[derive(Debug)]
struct Include;

impl TextHandler for Include {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
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
/// techxt's own definition names the argument `filename`; a tree parsed with techy's
/// preset `\input` spec names it `reference`. Falling back to the first argument covers
/// both without this handler having to guess at names.
fn target(cx: &mut RenderCx<'_, '_>) -> String {
    if let Ok(Some(text)) = cx.arg_text(FILENAME) {
        return text;
    }
    match cx.arg_at(0) {
        Ok(Some(flow)) => crate::layout::render_inline(&flow),
        _ => String::new(),
    }
}
