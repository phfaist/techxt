//! The document's title block (PLAN.md §9.8).
//!
//! `\title`, `\author` and `\date` do not print anything where they are written; they
//! record a value that `\maketitle` prints later, usually from a different part of the
//! document. techxt keeps them in the *run* state for exactly that reason — downward
//! render state is restored on the way back up, and these have to survive to a sibling.
//!
//! The argument is rendered where it is *declared*, not where it is used, so a title
//! written in the preamble is converted under the state it was written in.
//!
//! `\maketitle` lays the three out as pylatexenc does — the title, then the author and
//! the date indented under it, then a rule of `=` as wide as the widest of the three:
//!
//! ```text
//! On the Electrodynamics of Moving Bodies
//!     A. Einstein
//!     June 30, 1905
//! ===========================================
//! ```
//!
//! A missing title or author is reported in the output itself (`<no title>`,
//! `<no author>`) rather than silently skipped: a title block with a hole in it is a
//! document problem the reader should see. A missing date falls back to `\today`, which
//! in a `no_std` library with no clock is [`Options::today`](crate::Options::today) if
//! the caller set one and `<today>` otherwise.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::def::{Category, MacroDef, TextHandler, TextRule};
use crate::flow::{display_width, Flow, FlowItem};
use crate::layout::render_inline;
use crate::render::{NodeView, RenderCx, RenderError};

use super::handler;

/// The indent under the title line, on the author and date lines.
const INDENT: &str = "    ";

/// The titling category (PLAN.md §12.1).
///
/// The three field macros are declared with a leading optional argument, which is the
/// "short" form the AMS classes and beamer accept; the field itself is the mandatory
/// one.
pub fn category() -> Category {
    Category::new("titling")
        .with_macro(field("title", Field::Title))
        .with_macro(field("author", Field::Author))
        .with_macro(field("date", Field::Date))
        .with_macro(MacroDef::new("maketitle").rule(handler(MakeTitle)))
        .with_macro(MacroDef::new("today").rule(handler(Today)))
        // `\and` separates two authors of the same paper. LaTeX sets them side by side
        // in columns; a title block in plain text is one line per field, so the
        // separator is the word the macro is named after — an English word, like the
        // "Abstract" and "Theorem" the other categories generate, and localizing those
        // is on PLAN.md §17's list. Written with its spaces, so that it separates two
        // names whether or not the source put spaces around it: a literal's text goes
        // through the same word-and-glue splitting as document text, and the layout
        // engine collapses the glue against whatever is next to it.
        .with_macro(MacroDef::new("and").rule(TextRule::Literal(Cow::Borrowed(" and "))))
        // Not a title field, but the same shape and the same fate: it prints nothing.
        .with_macro(
            MacroDef::new("thanks")
                .arg("m", "text")
                .rule(TextRule::Skip),
        )
}

/// One of the three fields `\maketitle` prints.
#[derive(Clone, Copy, Debug)]
enum Field {
    /// `\title{…}`.
    Title,
    /// `\author{…}`.
    Author,
    /// `\date{…}`.
    Date,
}

/// A field macro: `[short form]{value}`, rendering nothing and recording the value.
fn field(name: &str, which: Field) -> MacroDef {
    MacroDef::new(name)
        .arg("o", "short")
        .arg("m", "value")
        .rule(handler(SetField(which)))
}

/// Record one field of the title block.
#[derive(Debug)]
struct SetField(Field);

impl TextHandler for SetField {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let value = cx.arg("value")?.unwrap_or_default();
        match self.0 {
            Field::Title => cx.set_doc_title(value),
            Field::Author => cx.set_doc_author(value),
            Field::Date => cx.set_doc_date(value),
        }
        // The declaration itself prints nothing where it stands.
        Ok(Flow::new())
    }
}

/// `\today` (PLAN.md §9.8).
///
/// techxt is `no_std` and has no clock; a caller who wants a date knows which one, and
/// says so through [`Options::today`](crate::Options::today).
#[derive(Debug)]
struct Today;

impl TextHandler for Today {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        Ok(Flow::from_plain_text(&today(cx)))
    }
}

/// What `\today` resolves to.
fn today(cx: &RenderCx<'_, '_>) -> String {
    match &cx.options().today {
        Some(date) => String::from(&**date),
        None => String::from("<today>"),
    }
}

/// `\maketitle` (PLAN.md §9.8).
#[derive(Debug)]
struct MakeTitle;

impl TextHandler for MakeTitle {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        // Each field on one line: a title block is a block of lines, and a paragraph
        // break inside a recorded title would otherwise break the layout apart.
        let title = cx.doc_title().map_or_else(
            || String::from("<no title>"),
            |flow: &Flow| render_inline(flow),
        );
        let author = cx.doc_author().map_or_else(
            || String::from("<no author>"),
            |flow: &Flow| render_inline(flow),
        );
        let date = match cx.doc_date() {
            Some(flow) => render_inline(flow),
            None => today(cx),
        };

        let lines = [
            title,
            alloc::format!("{INDENT}{author}"),
            alloc::format!("{INDENT}{date}"),
        ];
        let width = lines
            .iter()
            .map(|line| display_width(line))
            .max()
            .unwrap_or(0);
        let rule: String = core::iter::repeat_n('=', width).collect();

        let mut flow = Flow::new();
        flow.push(FlowItem::ParagraphBreak);
        let mut items: Vec<String> = lines.into_iter().collect();
        items.push(rule);
        for (index, line) in items.into_iter().enumerate() {
            if index > 0 {
                flow.push(FlowItem::HardBreak);
            }
            // One item per line: the rule's width was measured from these exact
            // strings, and wrapping one of them would make the block ragged.
            flow.push(FlowItem::Text(line.into_boxed_str()));
        }
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}
