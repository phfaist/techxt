//! Included graphics (PLAN.md §9.8).
//!
//! An image has no plain-text rendering, and the file name is not one: it is a path
//! into a directory the reader does not have. techxt writes a placeholder block instead
//! — `< g r a p h i c s >`, indented on a line of its own.
//!
//! The spaced-out letters are pylatexenc's idea and a good one: converted text is often
//! fed to a search index, and `graphics` written solid would be indexed as a word of
//! the document. Spaced out, it is unmistakably a marker.

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::def::{Category, MacroDef, TextHandler};
use crate::flow::{BlockKind, Flow, FlowItem};
use crate::render::{RenderCx, RenderError};

use super::handler;

/// The indent a placeholder block sits in.
const INDENT: &str = "    ";

/// The graphics category (PLAN.md §12.1).
pub fn category() -> Category {
    Category::new("graphics").with_macro(
        MacroDef::new("includegraphics")
            .arg("o", "options")
            .arg("m", "file")
            .rule(handler(Placeholder("graphics"))),
    )
}

/// A block that says something was here that plain text cannot show.
#[derive(Debug)]
struct Placeholder(&'static str);

impl TextHandler for Placeholder {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        _cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut text = alloc::string::String::from("<");
        for character in self.0.chars() {
            text.push(' ');
            text.push(character);
        }
        text.push_str(" >");

        let mut flow = Flow::new();
        // A block on its own, set off by blank lines: it interrupts the text, and
        // pretending otherwise would read as if the placeholder were a word in the
        // sentence.
        flow.push(FlowItem::ParagraphBreak);
        flow.push(FlowItem::BlockStart(BlockKind::Indent {
            first: INDENT.into(),
            cont: INDENT.into(),
        }));
        // One item, so that the spaces inside the marker are never touched.
        flow.push(FlowItem::Text(text.into_boxed_str()));
        flow.push(FlowItem::BlockEnd);
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}
