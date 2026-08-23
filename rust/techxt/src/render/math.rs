//! The two ends of the math pipeline (PLAN.md §9.5).
//!
//! [`mathfmt`](crate::mathfmt) knows how to segment, join and lay out a formula, but it
//! knows nothing of trees, states or options. This module is the seam between it and
//! the renderer, and it has exactly two jobs.
//!
//! **Putting math into the flow.** A math handler builds an [`Atom`] and hands it to
//! [`atom`], which decides how it travels: in
//! [`Fancy`](crate::convert::MathMode::Fancy) math it rides as a
//! [`MathAtom`](FlowItem::MathAtom) so that the joiner can space it against its
//! neighbours; anywhere else — [`Plain`](crate::convert::MathMode::Plain) math, or a
//! construct such as `\frac` used in running text — it is flattened to ordinary text on
//! the spot, because no joiner will run.
//!
//! **Taking math out again.** [`finish`] is the flow boundary: a *math scope* — a
//! `$…$` group, a `\[…\]` group, an `equation` environment, an `\ensuremath` argument —
//! folds its contents, collects the atoms, joins them, and converts the result to
//! ordinary flow items. Inline math becomes text split at the joiner's own spaces, so a
//! long formula may still wrap where the joiner put a gap; display math becomes
//! preformatted lines inside a four-space indent block, where nothing may re-wrap the
//! alignment the joiner computed. **Atoms never leave a math scope**, and therefore
//! never reach the layout engine.
//!
//! **Not converting it at all.** [`source_scope`] is the other end of the same
//! boundary, for [`Source`](crate::convert::MathMode::Source) mode: the scope is not
//! entered, and the formula goes out as the LaTeX it was written as. Every scope-opening
//! construct asks it before it folds anything, which is what makes `\[…\]` and
//! `\begin{equation}` — one formula, two spellings — come out the same.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use techy::core::node::NodeRef;
use techy_xp::lang::LatexlikeXp;

use crate::convert::{MathMode, Options};
use crate::flow::{BlockKind, Flow, FlowItem};
use crate::layout::render_inline;
use crate::mathfmt::{
    join_atoms_with, segment_plain, Atom, AtomBody, AtomClass, MathBox, MathWrapDelims,
};

use super::state::RenderState;

/// The prefix a display formula's block indents by, on its first and every later line.
const DISPLAY_INDENT: &str = "    ";

/// Whether atoms are the currency here: math is being rendered *and* the fancy engine
/// is the one rendering it.
///
/// Everything that builds an atom asks this first. In `Plain` math there is no joiner
/// to hand atoms to, and outside math there is no formula at all, so both cases want
/// finished text instead.
pub(crate) fn atoms_in_use(state: &RenderState, options: &Options) -> bool {
    state.in_math() && options.math_mode == MathMode::Fancy
}

/// A math scope in [`Source`](MathMode::Source) mode: the formula as the LaTeX it was
/// written as, rather than a rendering of it (PLAN.md §9.5).
///
/// Answers `None` in the two modes that render — there is a formula to fold, and the
/// caller goes on and folds it — and `Some` in `Source`, where the subtree is re-emitted
/// from node payloads and never descended into at all. Not descending is the point: the
/// contents are being *shown*, not converted, so nothing inside them may render, report
/// or collect anything.
///
/// **Every construct that opens a math scope asks this first**, and asking at each of
/// them is exactly what keeps the spellings of one formula from disagreeing: a `$…$` or
/// `\[…\]` group, an `equation` or `align` environment, a matrix written outside a
/// formula, an argument a handler renders as math (`\ensuremath`). A scope that a
/// formula already surrounds never gets here, because that formula was itself re-emitted
/// without descending.
///
/// `display` chooses the shape, as it does in the rendering modes: display math is a
/// [`Verbatim`](FlowItem::Verbatim) block of its own, inline math an
/// [`InlineVerbatim`](FlowItem::InlineVerbatim) word in the running text. Neither is
/// given the display indent — that indent is part of rendering a formula, and this is
/// its source.
pub(crate) fn source_scope(
    node: NodeRef<'_, LatexlikeXp>,
    display: bool,
    options: &Options,
) -> Option<Flow> {
    if options.math_mode != MathMode::Source {
        return None;
    }
    let latex = super::source::latex_source(node);
    let mut flow = Flow::new();
    flow.push(if display {
        FlowItem::Verbatim(latex.into())
    } else {
        FlowItem::InlineVerbatim(latex.into())
    });
    Some(flow)
}

/// Split a plain string into atoms and put them into the flow (PLAN.md §9.5).
///
/// This is what a characters node and a rule's replacement text become inside fancy
/// math. Note what does *not* happen here: the font alphabet is not applied
/// (DECISIONS.md D6) — the caller styles document characters before calling, and rule
/// output is never styled at all.
pub(crate) fn segmented(text: &str, upright_letters_are_op: bool) -> Flow {
    let mut flow = Flow::new();
    for atom in segment_plain(text, upright_letters_are_op) {
        flow.push(FlowItem::MathAtom(atom));
    }
    flow
}

/// Put a handler-built [`Atom`] into the flow, in whichever form the mode calls for.
///
/// In fancy math the atom travels intact, so the joiner can see that `\sin` is an
/// operator and that a fraction is open-ended on the right. Otherwise it is flattened
/// immediately — including the delimiters a
/// [wrappable](AtomBody::Wrappable) body would otherwise have been given at join time,
/// since no join will happen.
pub(crate) fn atom(atom: Atom, state: &RenderState, options: &Options) -> Flow {
    if atoms_in_use(state, options) {
        let mut flow = Flow::new();
        flow.push(FlowItem::MathAtom(atom));
        return flow;
    }
    Flow::text(&flatten(&atom, &options.math_expression_in))
}

/// One atom as finished text, with no neighbours to consult.
///
/// [`Atom::flatten`] is almost this, but it shows a wrappable body's contents *without*
/// delimiters — correct for a diagnostic fallback, wrong here: with no joiner to decide,
/// `x^{ABC}` has to commit to `x^(ABC)` or the extent of the script is lost.
fn flatten(atom: &Atom, wrap: &MathWrapDelims) -> String {
    match &atom.body {
        AtomBody::Wrappable { inner, prefix } => format!("{prefix}{}", wrap.wrap(inner)),
        _ => atom.flatten(),
    }
}

/// Render a folded math fragment to one line of text: a script's argument, a matrix
/// cell, an operator's name.
///
/// The fragment is joined as *inline* math whatever encloses it, because all three
/// callers need a single line — a subscript, a table cell and an operator name have
/// nowhere to put a second one.
pub(crate) fn inline_text(flow: &Flow, state: &RenderState, options: &Options) -> String {
    if !atoms_in_use(state, options) {
        return render_inline(flow);
    }
    let joined = join_atoms_with(atoms_of(flow.items()), false, &options.math_expression_in);
    let text = joined.math_box.join_lines(" ");
    let trimmed = text.trim();
    if trimmed.len() == text.len() {
        text
    } else {
        String::from(trimmed)
    }
}

/// Close a math scope: turn its folded contents into ordinary flow (PLAN.md §9.5,
/// "Flow boundary").
///
/// `display` says which of the two shapes comes out:
///
/// - **inline** — one line, split into words and glue at the spaces the *joiner* put
///   there. Splitting at those and at nothing else is deliberate: they are the only
///   collapsible spaces in a formula, so a long formula may wrap at them, while a
///   no-break space from `~` or the em space from `\quad` stays inside a word where the
///   macro that asked for it meant it to be.
/// - **display** — the lines of the assembled box, preformatted inside a four-space
///   indent block. Preformatted because the box's columns are already laid out:
///   re-wrapping a matrix would destroy the alignment the joiner computed.
///
/// A forced line break (`\\` in an `align` body) starts a new display line; each line is
/// joined on its own, exactly as if it were a formula of its own. An empty formula
/// (`$ $`) produces nothing at all, block included.
pub(crate) fn finish(body: Flow, display: bool, options: &Options) -> Flow {
    finish_with(
        body,
        display,
        options.math_mode,
        &options.math_expression_in,
    )
}

/// [`finish`] as a function of a math scope alone, for a caller that cannot keep the
/// [`Options`] alive.
///
/// The math-group handler closes its scope from a post-processing function attached to
/// the instruction it returns ([`ConcatPieces::map`](techy::recompose::ConcatPieces::map)),
/// and that function is `'static`: it may not borrow the renderer, and so may not borrow
/// the renderer's options either. These two fields are everything the boundary consults,
/// so it carries them and not a clone of the whole option set, once per formula.
pub(crate) fn scope_closer(
    display: bool,
    options: &Options,
) -> impl FnOnce(Flow) -> Flow + 'static {
    let mode = options.math_mode;
    let wrap = options.math_expression_in.clone();
    move |body| finish_with(body, display, mode, &wrap)
}

/// The body of [`finish`], taking the two options it consults directly rather than the
/// whole [`Options`] they belong to.
fn finish_with(body: Flow, display: bool, mode: MathMode, wrap: &MathWrapDelims) -> Flow {
    if mode != MathMode::Fancy {
        return finish_plain(body, display, wrap);
    }

    // Only a display scope has lines to break; inline math has nowhere to put a second
    // one, so a stray break there is treated as the space it stands for.
    let mut lines: Vec<Vec<FlowItem>> = vec![Vec::new()];
    for item in body.into_items() {
        match item {
            FlowItem::HardBreak | FlowItem::ParagraphBreak if display => lines.push(Vec::new()),
            other => {
                // The vector is seeded with one line and only ever grows.
                if let Some(line) = lines.last_mut() {
                    line.push(other);
                }
            }
        }
    }

    let boxes: Vec<MathBox> = lines
        .iter()
        .map(|line| join_atoms_with(atoms_of(line), display, wrap).math_box)
        .filter(|assembled| !assembled.is_blank())
        .collect();
    if boxes.is_empty() {
        return Flow::new();
    }
    if display {
        display_block(&boxes)
    } else {
        inline_flow(&boxes)
    }
}

/// [`finish`] for the modes that never built an atom.
///
/// `Plain` concatenates its pieces directly (PLAN.md §9.5), so the folded flow is
/// already the answer and only display's indent block has to be added. Stray atoms are
/// flattened rather than dropped: a handler that built one anyway must still show its
/// content.
fn finish_plain(body: Flow, display: bool, wrap: &MathWrapDelims) -> Flow {
    let mut flow = Flow::new();
    if display {
        flow.push(FlowItem::BlockStart(BlockKind::Indent {
            first: DISPLAY_INDENT.into(),
            cont: DISPLAY_INDENT.into(),
        }));
    }
    for item in body.into_items() {
        match item {
            FlowItem::MathAtom(atom) => flow.push(FlowItem::Text(flatten(&atom, wrap).into())),
            other => flow.push(other),
        }
    }
    if display {
        flow.push(FlowItem::BlockEnd);
    }
    flow
}

/// An inline formula: the joined line, split into words and glue at its spaces.
///
/// A **single** space is one the joiner put between two atoms, and becomes glue — a
/// place the line may break. A run of **two or more** is inside an atom that laid itself
/// out, an inline matrix being the only one that does, and has to survive exactly as it
/// is or the columns stop lining up; such a fragment is emitted as an
/// [`InlineVerbatim`](FlowItem::InlineVerbatim) word, which is the flow item for content
/// whose own whitespace is meant (a [`Text`](FlowItem::Text) item may hold no
/// collapsible whitespace, and glue would collapse the run to one space).
fn inline_flow(boxes: &[MathBox]) -> Flow {
    let mut flow = Flow::new();
    let mut word = String::new();
    for line in boxes.iter().flat_map(|assembled| assembled.lines.iter()) {
        let mut rest: &str = line;
        while !rest.is_empty() {
            let spaces = rest.find(|c| c != ' ').unwrap_or(rest.len());
            match spaces {
                0 => {
                    let text = rest.find(' ').unwrap_or(rest.len());
                    word.push_str(&rest[..text]);
                    rest = &rest[text..];
                }
                1 => {
                    push_word(&mut flow, &mut word);
                    // Layout drops glue at a line's start and end, so an edge one is
                    // harmless as well as unreachable (the joiner trims neither side).
                    flow.push(FlowItem::Glue);
                    rest = &rest[1..];
                }
                _ => {
                    word.push_str(&rest[..spaces]);
                    rest = &rest[spaces..];
                }
            }
        }
    }
    push_word(&mut flow, &mut word);
    flow
}

/// Emit the word being accumulated, if there is one.
fn push_word(flow: &mut Flow, word: &mut String) {
    if word.is_empty() {
        return;
    }
    if word.contains(' ') {
        flow.push(FlowItem::InlineVerbatim(word.as_str().into()));
    } else {
        flow.push(FlowItem::Text(word.as_str().into()));
    }
    word.clear();
}

/// A display formula: the assembled lines, preformatted inside the indent block.
fn display_block(boxes: &[MathBox]) -> Flow {
    let mut text = String::new();
    for line in boxes.iter().flat_map(|assembled| assembled.lines.iter()) {
        if !text.is_empty() {
            text.push('\n');
        }
        // A box pads its shorter rows to the common width, which would otherwise leave
        // real trailing spaces in preformatted output.
        text.push_str(line.trim_end());
    }

    let mut flow = Flow::new();
    flow.push(FlowItem::BlockStart(BlockKind::Indent {
        first: DISPLAY_INDENT.into(),
        cont: DISPLAY_INDENT.into(),
    }));
    flow.push(FlowItem::Verbatim(text.into()));
    flow.push(FlowItem::BlockEnd);
    flow
}

/// The atoms of a folded math fragment, in order.
///
/// Everything that is not already an atom is *ordinary text that reached a formula* —
/// what `\text{…}` renders to, what `~` and `\quad` emit — and a run of it becomes one
/// [`Text`](AtomClass::Text) atom. That is what makes `\text{…}` opaque to the joiner:
/// `$\text{a+b}$` reads `a+b`, with no spacing around a `+` that is not an operator,
/// while `$a+\text{[xyz]}$` still gets the space its `+` asks for.
fn atoms_of(items: &[FlowItem]) -> Vec<Atom> {
    let mut atoms = Vec::new();
    let mut run = String::new();
    for item in items {
        match item {
            FlowItem::MathAtom(built) => {
                flush_text(&mut run, &mut atoms);
                atoms.push(built.clone());
            }
            FlowItem::Text(text) | FlowItem::InlineVerbatim(text) => run.push_str(text),
            // A formula has no lines and no paragraphs; whatever asked for one meant a
            // gap, and a gap is what the joiner already understands.
            FlowItem::Glue | FlowItem::HardBreak | FlowItem::ParagraphBreak => run.push(' '),
            FlowItem::Verbatim(text) => {
                run.extend(text.chars().map(|c| if c == '\n' { ' ' } else { c }));
            }
            // Block structure inside a formula (a stray `\begin{quote}`, say) has no
            // meaning the joiner could use, and its contents are in the flow either way.
            FlowItem::BlockStart(_) | FlowItem::BlockEnd => {}
            // Consumed by the enclosing matrix handler before this is ever called; one
            // that reaches here belongs to no matrix and separates nothing.
            FlowItem::CellSep | FlowItem::RowSep | FlowItem::RuleMark => {}
        }
    }
    flush_text(&mut run, &mut atoms);
    atoms
}

/// Close off a run of ordinary text as one [`Text`](AtomClass::Text) atom.
fn flush_text(run: &mut String, atoms: &mut Vec<Atom>) {
    if run.is_empty() {
        return;
    }
    atoms.push(Atom::from_text(&**run, AtomClass::both(AtomClass::Text)));
    run.clear();
}

/// Split a folded matrix body into rows of rendered cells (PLAN.md §9.5).
///
/// The body was folded with [`MathCtx::matrix`](super::MathCtx::matrix) set, so `&` and
/// `\\` arrived as [`CellSep`](FlowItem::CellSep) and [`RowSep`](FlowItem::RowSep);
/// splitting at them is all that is left. Every cell is then joined as a formula of its
/// own, which is why the `+` in one cell never spaces itself against the next.
///
/// A cell left empty by a trailing `&`, and a row left empty by a trailing `\\`, are
/// dropped — they are punctuation, not content — so `\begin{pmatrix}\end{pmatrix}`
/// yields no rows at all rather than one empty one.
pub(crate) fn matrix_rows(body: Flow, state: &RenderState, options: &Options) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = Flow::new();
    let close_cell = |cell: &mut Flow, row: &mut Vec<String>| {
        row.push(inline_text(&core::mem::take(cell), state, options));
    };

    for item in body.into_items() {
        match item {
            FlowItem::CellSep => close_cell(&mut cell, &mut row),
            FlowItem::RowSep => {
                close_cell(&mut cell, &mut row);
                rows.push(core::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    close_cell(&mut cell, &mut row);
    rows.push(row);

    for row in &mut rows {
        while row.last().is_some_and(|cell| cell.is_empty()) {
            row.pop();
        }
    }
    rows.retain(|row| !row.is_empty());

    // A row the author left short is filled out with empty cells, so that every line of
    // the matrix comes out the same width and its delimiters stand in a straight
    // column. Without this a display matrix whose last row has fewer cells draws a
    // closing delimiter part-way along the line.
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(columns, String::new());
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{render, LayoutOptions};
    use crate::mathfmt::MathWrapDelims;
    use crate::render::MathCtx;

    fn math_state(display: bool) -> RenderState {
        RenderState {
            math: Some(MathCtx {
                display,
                matrix: false,
            }),
            ..RenderState::default()
        }
    }

    fn body(text: &str) -> Flow {
        segmented(text, true)
    }

    #[test]
    fn an_inline_scope_splits_at_the_joiners_spaces() {
        let flow = finish(body("a+b"), false, &Options::default());
        // "a + b" is three words and two glues, so a narrow line may break there.
        assert_eq!(flow.items().len(), 5);
        assert_eq!(render(&flow, &LayoutOptions::default()), "a + b\n");
    }

    #[test]
    fn a_no_break_space_stays_inside_its_word() {
        let mut inner = Flow::new();
        inner.push(FlowItem::Text("a\u{a0}b".into()));
        let flow = finish(inner, false, &Options::default());
        assert_eq!(flow.items().len(), 1);
    }

    #[test]
    fn a_display_scope_is_a_preformatted_indent_block() {
        let flow = finish(body("a+b"), true, &Options::default());
        assert!(matches!(
            flow.items().first(),
            Some(FlowItem::BlockStart(_))
        ));
        assert_eq!(render(&flow, &LayoutOptions::default()), "    a + b\n");
    }

    #[test]
    fn a_forced_break_starts_a_new_display_line() {
        let mut inner = body("a=1");
        inner.push(FlowItem::HardBreak);
        inner.extend(body("b=2"));
        assert_eq!(
            render(
                &finish(inner, true, &Options::default()),
                &LayoutOptions::default()
            ),
            "    a = 1\n    b = 2\n"
        );
        // Inline, the same break is only the gap it stands for.
        let mut inner = body("a=1");
        inner.push(FlowItem::HardBreak);
        inner.extend(body("b=2"));
        assert_eq!(
            render(
                &finish(inner, false, &Options::default()),
                &LayoutOptions::default()
            ),
            "a = 1 b = 2\n"
        );
    }

    #[test]
    fn an_empty_formula_produces_nothing_at_all() {
        assert!(finish(Flow::new(), false, &Options::default()).is_empty());
        assert!(finish(Flow::new(), true, &Options::default()).is_empty());
    }

    #[test]
    fn text_that_reached_a_formula_becomes_one_opaque_atom() {
        let mut inner = Flow::new();
        inner.push(FlowItem::Text("a+b".into()));
        assert_eq!(
            render(
                &finish(inner, false, &Options::default()),
                &LayoutOptions::default()
            ),
            "a+b\n"
        );
    }

    #[test]
    fn a_wrappable_atom_commits_to_its_delimiters_outside_fancy_math() {
        let atom = Atom {
            body: AtomBody::Wrappable {
                inner: "abc".into(),
                prefix: "^".into(),
            },
            cls: AtomClass::both(AtomClass::Script),
            script: None,
        };
        assert_eq!(flatten(&atom, &MathWrapDelims::Braces), "^{abc}");
        // In fancy math the atom travels intact instead, for the joiner to decide.
        let flow = super::atom(atom, &math_state(false), &Options::default());
        assert!(matches!(flow.items().first(), Some(FlowItem::MathAtom(_))));
    }

    #[test]
    fn matrix_rows_split_at_the_separators_and_drop_trailing_punctuation() {
        let mut inner = body("1");
        inner.push(FlowItem::CellSep);
        inner.extend(body("2"));
        inner.push(FlowItem::RowSep);
        inner.extend(body("30"));
        inner.push(FlowItem::CellSep);
        inner.extend(body("4"));
        inner.push(FlowItem::RowSep);
        let rows = matrix_rows(inner, &math_state(true), &Options::default());
        assert_eq!(rows, [["1", "2"], ["30", "4"]]);

        assert!(matrix_rows(Flow::new(), &math_state(true), &Options::default()).is_empty());
    }
}
