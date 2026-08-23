//! Tabular material (PLAN.md §9.6): `tabular`, `tabular*`, `tabularx` and the
//! constructs that shape their cells.
//!
//! A table handler sets [`RenderState::table`](crate::render::RenderState::table) for
//! the body fold, which is what turns the `&`, `\\` and `\hline` defined in
//! [`defs::base`](super::base) into cell separators, row separators and rules; it then
//! splits the folded flow at those markers to lay the columns out.
//!
//! # Why the markers travel through the flow
//!
//! A cell's contents are ordinary document material — macros, maths, a nested list —
//! and must be rendered by the same rules as everything else. So the table does not
//! walk the tree looking for `&` nodes; it folds its body once, exactly as any other
//! environment would, and the alignment constructs render as
//! [`CellSep`](crate::flow::FlowItem::CellSep),
//! [`RowSep`](crate::flow::FlowItem::RowSep) and
//! [`RuleMark`](crate::flow::FlowItem::RuleMark) markers that the table then splits at.
//! Those markers are the one part of the flow that never reaches the layout engine: a
//! table consumes its own.
//!
//! # What a column specification is read from
//!
//! From the specification's **source**, reassembled from node payloads, not from its
//! rendered text. `p{3cm}` renders as `p3cm`, whose `c` would silently invent a centred
//! column; the source keeps the braces, so the width group can be skipped as a unit.
//!
//! # Deliberate omissions
//!
//! Spans (`\multicolumn`, `\multirow`) are ignored — the cell's contents are rendered
//! in one column, and one `techxt.unsupported-ignored` per table says so. Cells are not
//! width-budgeted: a `p{3cm}` column is as wide as its widest line, because plain text
//! has no notion of a centimetre. Both are on PLAN.md §17's list.

use alloc::string::String;
use alloc::vec::Vec;

use techy::core::node::NodeRef;
use techy::latexlike::CallableType;
use techy_xp::lang::LatexlikeXp;

use crate::def::{Category, EnvDef, MacroDef, Template, TextHandler, TextRule};
use crate::diag::UnsupportedIgnored;
use crate::flow::{display_width, BlockKind, Flow, FlowItem};
use crate::layout::render_inline;
use crate::render::{RenderCx, RenderError, TableCtx};

use super::handler;

/// What separates two columns in the rendered table.
const COLUMN_GAP: &str = "  ";

/// The macro whose span techxt renders but does not honour.
const MULTICOLUMN: &str = "multicolumn";

/// The environments that make a table body, and therefore end the search for the table
/// a `\multicolumn` belongs to.
const TABLE_ENVIRONMENTS: [&str; 4] = ["tabular", "tabular*", "tabularx", "longtable"];

/// The tables category (PLAN.md §12.1).
///
/// The three environments differ only in the arguments that precede the column
/// specification — a target width, a placement — and none of those means anything in
/// plain text, so they are parsed and dropped and one handler serves all three.
pub fn category() -> Category {
    let mut category = Category::new("tables");
    category.add_env(
        EnvDef::new("tabular")
            .arg("m", "colspec")
            .rule(handler(Tabular)),
    );
    category.add_env(
        EnvDef::new("tabular*")
            .arg("m", "width")
            .arg("m", "colspec")
            .rule(handler(Tabular)),
    );
    // `longtable` (the package of the same name) is a `tabular` that may break across
    // pages — which plain text does not have, so what is left of it is a table. Its
    // head and foot markers end a row group that only a page break would separate.
    category.add_env(
        EnvDef::new("longtable")
            .arg("o", "position")
            .arg("m", "colspec")
            .rule(handler(Tabular)),
    );
    for name in ["endhead", "endfirsthead", "endfoot", "endlastfoot"] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }
    category.add_env(
        EnvDef::new("tabularx")
            .arg("m", "width")
            .arg("o", "position")
            .arg("m", "colspec")
            .rule(handler(Tabular)),
    );

    // The span and the cell's own alignment are dropped; the contents are not.
    category.add_macro(
        MacroDef::new(MULTICOLUMN)
            .arg("m", "columns")
            .arg("m", "colspec")
            .arg("m", "content")
            .rule(TextRule::Template(Template::new("{content}"))),
    );
    // `\multirow{2}{*}{text}` spans rows rather than columns; the same reasoning
    // applies, and the same rendering — the contents, in the one cell they start in.
    category.add_macro(
        MacroDef::new("multirow")
            .arg("m", "rows")
            .arg("m", "width")
            .arg("m", "content")
            .rule(TextRule::Template(Template::new("{content}"))),
    );
    // `\cline{2-3}` rules only some of the columns, which a full-width rule cannot
    // express; PLAN.md §9.6 renders it as `\hline` rather than dropping the separation
    // it was drawn to show.
    category.add_macro(MacroDef::new("cline").arg("m", "range").rule(handler(Rule)));
    // `booktabs`' three rules, which a great many real papers use instead of `\hline`.
    for name in ["toprule", "midrule", "bottomrule"] {
        category.add_macro(MacroDef::new(name).rule(handler(Rule)));
    }
    category
}

// --------------------------------------------------------------------------- handlers

/// A `tabular`-shaped environment (PLAN.md §9.6).
#[derive(Debug)]
struct Tabular;

impl TextHandler for Tabular {
    fn render(
        &self,
        node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let alignments = parse_alignments(&colspec_source(node, cx));

        let mut state = cx.state().clone();
        state.table = Some(TableCtx);
        // A table's cells are not list items, and an `\item` inside one is as stray as
        // it would be anywhere else outside a list.
        state.list = None;
        let body = cx.body_with_state(state)?;

        // One diagnostic per table, not per span: a table of ten spanning headers has
        // one thing wrong with it, and saying so ten times would bury everything else.
        if spans_a_column(node) {
            cx.report(UnsupportedIgnored::new("\\multicolumn", "the column span"));
        }

        Ok(render_rows(&split_rows(&body), &alignments))
    }
}

/// `\cline` and the `booktabs` rules: a full-width rule inside a table, nothing outside
/// one.
#[derive(Debug)]
struct Rule;

impl TextHandler for Rule {
    fn render(
        &self,
        node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut flow = Flow::new();
        if cx.state().table.is_some() {
            flow.push(FlowItem::RuleMark);
        } else {
            let construct = alloc::format!("\\{}", node.name().unwrap_or(""));
            cx.report(UnsupportedIgnored::new(
                construct,
                "a horizontal rule outside a table",
            ));
        }
        Ok(flow)
    }
}

/// Whether `node` is the table that a `\multicolumn` inside it belongs to.
///
/// The ancestor walk is what makes the count "one per table" exact rather than
/// approximate: a `tabular` nested in a cell of another owns its own spans, and the
/// outer table must not report them a second time.
fn spans_a_column(node: NodeRef<'_, LatexlikeXp>) -> bool {
    node.descendants().any(|descendant| {
        descendant.callable_type() == Some(CallableType::Macro)
            && descendant.name() == Some(MULTICOLUMN)
            && enclosing_table(descendant).is_some_and(|table| table.id() == node.id())
    })
}

/// The innermost table environment `node` sits in, if any.
fn enclosing_table(node: NodeRef<'_, LatexlikeXp>) -> Option<NodeRef<'_, LatexlikeXp>> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.callable_type() == Some(CallableType::Environment)
            && ancestor
                .name()
                .is_some_and(|name| TABLE_ENVIRONMENTS.contains(&name))
        {
            return Some(ancestor);
        }
        current = ancestor.parent();
    }
    None
}

// ------------------------------------------------------------- column specifications

/// How one column's cells are padded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    /// `l`, and everything techxt cannot honour exactly (`p{…}`, `X`).
    Left,
    /// `c`, padded on both sides and biased left when the padding is odd.
    Center,
    /// `r`.
    Right,
}

/// The column specification as it was written, reassembled from node payloads.
///
/// Answers an empty specification for a table with no `colspec` argument at all — a
/// tree parsed against a different definition set — which reads as "every column is
/// left-aligned", the same default an over-short specification gets.
fn colspec_source(node: NodeRef<'_, LatexlikeXp>, cx: &RenderCx<'_, '_>) -> String {
    let Ok(Some(nodes)) = node.argument_content_nodes_named("colspec") else {
        return String::new();
    };
    let mut source = String::new();
    for child in nodes.iter() {
        if let Ok(text) = cx.source_of(child) {
            source.push_str(&text);
        }
    }
    source
}

/// Read the alignment letters of a column specification (PLAN.md §9.6).
///
/// Everything that does not describe a column of text is skipped: rules (`|`), inserted
/// material (`@{…}`, `!{…}`) and cell decorations (`>{…}`, `<{…}`) take no column of
/// their own, and the paragraph columns (`p`, `m`, `b`) and `tabularx`'s `X` take one
/// but have a width techxt cannot honour, so they read as left-aligned.
fn parse_alignments(spec: &str) -> Vec<Align> {
    let mut alignments = Vec::new();
    let mut chars = spec.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            'l' => alignments.push(Align::Left),
            'c' => alignments.push(Align::Center),
            'r' => alignments.push(Align::Right),
            'X' => alignments.push(Align::Left),
            'p' | 'm' | 'b' => {
                alignments.push(Align::Left);
                skip_group(&mut chars);
            }
            '@' | '!' | '>' | '<' => skip_group(&mut chars),
            // A group that no letter claimed — the repetition count of `*{3}{c}`, say.
            // Skipping it as a unit is what keeps its contents from reading as columns.
            '{' => skip_balanced(&mut chars),
            _ => {}
        }
    }
    alignments
}

/// Skip the `{…}` that follows a column letter, if one does.
fn skip_group(chars: &mut core::iter::Peekable<core::str::Chars<'_>>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    if chars.peek() == Some(&'{') {
        chars.next();
        skip_balanced(chars);
    }
}

/// Consume through the `}` matching an opening brace that has already been consumed.
fn skip_balanced(chars: &mut core::iter::Peekable<core::str::Chars<'_>>) {
    let mut depth = 1usize;
    for character in chars {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

// -------------------------------------------------------------------- the table body

/// One line of a table: a row of rendered cells, or a rule.
#[derive(Debug)]
enum Row {
    /// The cells of one row, already flattened to single lines.
    Cells(Vec<String>),
    /// A horizontal rule (`\hline`, `\cline`, a `booktabs` rule).
    Rule,
}

/// Split a folded table body at its cell, row and rule markers.
///
/// Each cell is flattened to one line as it is closed: a cell is a *cell*, and a
/// paragraph break or a nested block inside one has nowhere to go in a column of a
/// text table.
fn split_rows(body: &Flow) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cell = Flow::new();

    for item in body.items() {
        match item {
            FlowItem::CellSep => cells.push(render_inline(&core::mem::take(&mut cell))),
            FlowItem::RowSep => {
                cells.push(render_inline(&core::mem::take(&mut cell)));
                flush_row(&mut rows, &mut cells);
            }
            FlowItem::RuleMark => {
                cells.push(render_inline(&core::mem::take(&mut cell)));
                flush_row(&mut rows, &mut cells);
                rows.push(Row::Rule);
            }
            other => cell.push(other.clone()),
        }
    }
    cells.push(render_inline(&cell));
    flush_row(&mut rows, &mut cells);
    rows
}

/// Close the row being accumulated, unless there is nothing in it.
///
/// A row of one empty cell is what a `\\` at the end of the last row leaves behind, and
/// what stands between an `\hline` and the row before it; neither is a row the reader
/// should see.
fn flush_row(rows: &mut Vec<Row>, cells: &mut Vec<String>) {
    let row = core::mem::take(cells);
    if row.len() <= 1 && row.iter().all(String::is_empty) {
        return;
    }
    rows.push(Row::Cells(row));
}

/// Lay the rows out: columns padded to their widest cell, rules across the whole table.
fn render_rows(rows: &[Row], alignments: &[Align]) -> Flow {
    let columns = rows
        .iter()
        .filter_map(|row| match row {
            Row::Cells(cells) => Some(cells.len()),
            Row::Rule => None,
        })
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return Flow::new();
    }

    let mut widths = alloc::vec![0usize; columns];
    for row in rows {
        if let Row::Cells(cells) = row {
            for (index, cell) in cells.iter().enumerate() {
                widths[index] = widths[index].max(display_width(cell));
            }
        }
    }
    let total: usize =
        widths.iter().sum::<usize>() + display_width(COLUMN_GAP) * widths.len().saturating_sub(1);

    let mut flow = Flow::new();
    // The table is a block of its own so that it starts on a line of its own, whatever
    // it was written next to; it adds no indentation, because a table's own columns are
    // the alignment the reader is meant to see.
    flow.push(FlowItem::BlockStart(BlockKind::Indent {
        first: "".into(),
        cont: "".into(),
    }));
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            flow.push(FlowItem::HardBreak);
        }
        let line = match row {
            Row::Rule => "-".repeat(total),
            Row::Cells(cells) => render_row(cells, &widths, alignments),
        };
        // One text item: the padding *is* the content here, and a line break inside a
        // row would put a column somewhere it does not belong. Trailing padding is
        // trimmed, because no line of output ends in whitespace.
        flow.push(FlowItem::Text(line.trim_end().into()));
    }
    flow.push(FlowItem::BlockEnd);
    flow
}

/// One row of cells, padded to the column widths and joined.
fn render_row(cells: &[String], widths: &[usize], alignments: &[Align]) -> String {
    let mut line = String::new();
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            line.push_str(COLUMN_GAP);
        }
        let cell = cells.get(index).map_or("", String::as_str);
        // A column the specification did not describe — or described with something
        // techxt cannot honour — is left-aligned.
        let align = alignments.get(index).copied().unwrap_or(Align::Left);
        pad_into(&mut line, cell, *width, align);
    }
    line
}

/// Append `cell`, padded to `width` according to `align`.
fn pad_into(line: &mut String, cell: &str, width: usize, align: Align) {
    let padding = width.saturating_sub(display_width(cell));
    let (before, after) = match align {
        Align::Left => (0, padding),
        Align::Right => (padding, 0),
        // Left-biased on an odd count: the extra space goes on the right, so a column
        // of centred cells still starts where the eye expects it to.
        Align::Center => (padding / 2, padding - padding / 2),
    };
    line.extend(core::iter::repeat_n(' ', before));
    line.push_str(cell);
    line.extend(core::iter::repeat_n(' ', after));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_letters_are_read_and_the_rest_is_skipped() {
        assert_eq!(
            parse_alignments("lcr"),
            [Align::Left, Align::Center, Align::Right]
        );
        assert_eq!(
            parse_alignments("|l||c|r|"),
            [Align::Left, Align::Center, Align::Right]
        );
        // A paragraph column takes a column but not its width, and its `{3cm}` must not
        // read as a centred column of its own.
        assert_eq!(parse_alignments("p{3cm}r"), [Align::Left, Align::Right]);
        assert_eq!(parse_alignments("Xl"), [Align::Left, Align::Left]);
        // Inserted material and cell decorations take no column at all.
        assert_eq!(
            parse_alignments("@{}l>{\\bfseries}c<{}"),
            [Align::Left, Align::Center]
        );
        assert_eq!(parse_alignments("!{\\vrule}r"), [Align::Right]);
        // Nested braces inside a skipped group.
        assert_eq!(
            parse_alignments(">{\\raggedright{a}}lr"),
            [Align::Left, Align::Right]
        );
        assert_eq!(parse_alignments(""), []);
    }

    #[test]
    fn padding_is_left_biased_when_centring() {
        let mut line = String::new();
        pad_into(&mut line, "x", 4, Align::Center);
        assert_eq!(line, " x  ");

        let mut line = String::new();
        pad_into(&mut line, "x", 3, Align::Right);
        assert_eq!(line, "  x");

        let mut line = String::new();
        pad_into(&mut line, "xyz", 2, Align::Left);
        assert_eq!(line, "xyz");
    }
}
