//! Display-math environments, the aligned environments and matrices.
//!
//! `equation`, `gather`, `multline` and `dmath` (with their starred forms),
//! `displaymath` and `subequations`; `align`, `alignat`, `flalign`, `eqnarray`,
//! `aligned`, `alignedat` and `split`; the matrix family — `matrix`, `pmatrix`,
//! `bmatrix`, `Bmatrix`, `vmatrix`, `Vmatrix`, `smallmatrix` — the `cases` family and
//! `array`.
//!
//! # An environment can be a math scope, or contribute to one
//!
//! `\begin{equation}` outside a formula *opens* display math: it renders its body in
//! math mode, joins the atoms, and emits the finished lines. The same environment
//! inside `\[…\]`, and every `split` or matrix inside an `equation`, does the opposite
//! — it hands its atoms to the formula that already surrounds it, so that the joiner
//! sees the whole expression at once and can space `𝐴 = ⎛…⎞` correctly. Which of the
//! two it is depends on nothing but whether math was already being rendered, so no
//! environment has to know where it was written.
//!
//! Opening a scope carries the third obligation as well: in
//! [`MathMode::Source`](crate::convert::MathMode::Source) an environment that opens a
//! formula re-emits it as LaTeX and never enters its body, which is what makes
//! `\begin{equation}` and the `\[…\]` it is a synonym for answer that mode alike.
//!
//! # An `align` is a matrix without delimiters
//!
//! Every environment that lays its body out in columns goes through one handler,
//! `Block`: `&` splits cells and `\\` splits rows, each cell is joined as a formula of
//! its own, and the rows are laid out as the environment's
//! [`Columns`] say. A matrix right-justifies every column and
//! `cases` left-justifies; an `align` pushes what stands left of each `&` up against it
//! and starts what stands right of it there, so that the relations of consecutive rows
//! fall into a column as they do on the page:
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! assert_eq!(
//!     converter.latex_to_text(r"\begin{align} a &= b + c \\ xyz &= d \end{align}")?.text,
//!     "      𝑎 = 𝑏 + 𝑐\n    𝑥𝑦𝑧 = 𝑑\n"
//! );
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! `aligned` and `split` are the same block handed to the formula around them, exactly
//! as a matrix inside a formula is.
//!
//! # `&` and `\\`
//!
//! The `&` and `\\` that [`defs::base`](super::base) defines read
//! [`RenderState`](crate::render::RenderState) to decide what they are, and these
//! environments are what sets it. In a block body
//! [`MathCtx::matrix`](crate::render::MathCtx::matrix) is set, so they are the cell and
//! row separators the handler splits at. In any other formula `\\` is a new display
//! line and `&` is nothing: there are no columns to align to.

use crate::def::{Category, EnvDef, TextHandler, TextRule};
use crate::flow::Flow;
use crate::mathfmt::{display_block_atom, inline_block_atom, Columns, MatrixFence};
use crate::render::{math, MathCtx, NodeView, RenderCx, RenderError};

use super::handler;
use super::tables::parse_alignments;

/// The mathenvs category.
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::standard();
/// assert_eq!(
///     converter.latex_to_text(r"\begin{equation} a^2 + b^2 = c^2 \end{equation}")?.text,
///     "    𝑎² + 𝑏² = 𝑐²\n"
/// );
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub fn category() -> Category {
    let mut category = Category::new("mathenvs");
    display_environments(&mut category);
    aligned_environments(&mut category);
    matrices(&mut category);
    category
}

// ------------------------------------------------------- display environments

/// The environments whose body is one displayed formula.
///
/// All of them render alike: plain text has no equation numbers to place and no page
/// width to break a `multline` across, so what is left of each is "a formula,
/// displayed" — and `\\` inside one starts the next line of it.
fn display_environments(category: &mut Category) {
    for name in [
        "equation",
        "equation*",
        "gather",
        "gather*",
        "multline",
        "multline*",
        "dmath",
        "dmath*",
        // LaTeX's own displayed-formula environment, which `\[…\]` is short for.
        "displaymath",
        // The inner form: a `gathered` is written *inside* another formula, and
        // renders by handing its atoms to it — which is what every environment here
        // does when math already surrounds it.
        "gathered",
    ] {
        category.add_env(
            EnvDef::new(name)
                .math_body()
                .rule(handler(Formula { display: true })),
        );
    }
    // `\begin{math}` is `$…$`: a formula in running text, never displayed, wherever it
    // is written.
    category.add_env(
        EnvDef::new("math")
            .math_body()
            .rule(handler(Formula { display: false })),
    );
    // `subequations` only renumbers what is inside it, which plain text does not show;
    // its body is ordinary document content holding formulas of its own.
    category.add_env(EnvDef::new("subequations").rule(TextRule::Content));
}

// ------------------------------------------------------- aligned environments

/// The environments whose body is rows of cells aligned at `&`, drawn without
/// delimiters.
fn aligned_environments(category: &mut Category) {
    for name in [
        "align",
        "align*",
        "flalign",
        "flalign*",
        "eqnarray",
        "eqnarray*",
        // The inner forms, written inside another formula and handed to it.
        "aligned",
        "split",
    ] {
        category.add_env(EnvDef::new(name).math_body().rule(handler(Block)));
    }
    // `alignat` and its relatives take the number of column pairs, which the rows
    // already show; it is parsed so that it cannot leak, and dropped.
    for name in ["alignat", "alignat*", "alignedat"] {
        category.add_env(
            EnvDef::new(name)
                .arg("m", "columns")
                .math_body()
                .rule(handler(Block)),
        );
    }
}

// ------------------------------------------------------------------ matrices

/// The matrix family, plus `array`.
fn matrices(category: &mut Category) {
    for name in [
        "matrix",
        "pmatrix",
        "bmatrix",
        "Bmatrix",
        "vmatrix",
        "Vmatrix",
        "smallmatrix",
        "psmallmatrix",
        "bsmallmatrix",
        // `cases` is a matrix in every way that plain text can see: rows of
        // alternatives, a condition in the second column, and a brace around the lot.
        // mathtools' seven relatives are that same matrix spelled differently: `d…`
        // sets the cells in display size, `r…` moves the brace to the other side, and
        // the starred forms set the second column in text mode. The first two are
        // invisible here — plain text has one size, and `MatrixFence` explains why the
        // brace is a pair either way.
        //
        // The third is *not* invisible, and is a known gap: a `cases*` writing its
        // condition as bare words rather than `\text{…}` gets them math-italic, since
        // a cell here is math wherever it sits. Registering the name is still the
        // better answer — the alternative is the unknown-environment path, which loses
        // the rows and the brace as well and reports two diagnostics for a document
        // that is not wrong.
        "cases",
        "cases*",
        "dcases",
        "dcases*",
        "rcases",
        "rcases*",
        "drcases",
        "drcases*",
    ] {
        category.add_env(EnvDef::new(name).math_body().rule(handler(Block)));
    }
    // `array`'s column specification says how each column is aligned and where its
    // rules go; the alignment is honoured, the rules are not drawn.
    category.add_env(
        EnvDef::new("array")
            .arg("o", "position")
            .arg("m", "colspec")
            .math_body()
            .rule(handler(Block)),
    );
}

// ------------------------------------------------------------------ handlers

/// One formula environment: `equation`, `gather`, `math`, … .
#[derive(Debug)]
struct Formula {
    /// Whether this environment *displays* its formula when nothing surrounds it.
    ///
    /// True for all of them but `math`, which is `$…$` written out and therefore
    /// inline wherever it stands.
    display: bool,
}

impl TextHandler for Formula {
    fn render(&self, node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let enclosing = cx.state().math;
        // A math environment *is* display math — unless it is written
        // inside an inline formula, where there is no second line to display it on.
        let display = self.display && enclosing.is_none_or(|math| math.display);

        // A formula this environment opens is a formula, however it is spelled: in
        // `Source` mode it re-emits as LaTeX exactly as `\[…\]` does, and its body is
        // never entered. One a formula already surrounds is that formula's business —
        // and in `Source` mode there is no such case to answer, since the formula
        // outside was not descended into either.
        if enclosing.is_none() {
            if let Some(flow) = math::source_scope(node, display, cx.options()) {
                return Ok(flow);
            }
        }

        let mut inner = cx.state().clone();
        inner.math = Some(MathCtx {
            display,
            matrix: false,
        });
        let body = cx.body_with_state(inner)?;

        Ok(match enclosing {
            // Nothing surrounds this formula, so it closes its own scope.
            None => math::finish(body, display, cx.options()),
            // A formula surrounds it and will join everything at once.
            Some(_) => body,
        })
    }
}

/// One block laid out in rows and columns: a matrix, a `cases`, an `align`.
///
/// Inline, a block is a single line — `[ 1  2; 30  4 ]`, `𝑎 = 𝑏; 𝑐 = 𝑑`; displayed, it
/// is one line per row, between delimiters drawn as tall as it is where it has any.
/// Either way it is **one atom**, opaque to the joiner and self-delimiting on both
/// edges, so `$x\begin{pmatrix}1&2\end{pmatrix}y$` hugs the way a bracketed expression
/// does.
///
/// The environment's *name* says which delimiters it draws and how its columns are
/// laid out — that is how LaTeX spells them — so one handler serves them all and asks
/// [`MatrixFence::for_environment`] and [`Columns::for_environment`]. `array` says so
/// in an argument instead, and its column specification is read the way a `tabular`'s
/// is.
#[derive(Debug)]
struct Block;

impl TextHandler for Block {
    fn render(&self, node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let name = node.name().unwrap_or("");
        // A name neither table knows can only be an environment registered elsewhere
        // against this handler; brackets are v3's universal pair and the safe reading
        // of "some kind of matrix", and an empty column specification lays out as a
        // matrix does.
        let fence = MatrixFence::for_environment(name).unwrap_or(MatrixFence::Brackets);
        let columns = Columns::for_environment(name).unwrap_or_else(|| {
            Columns::Spec(parse_alignments(
                &node.argument_source("colspec").unwrap_or_default(),
            ))
        });

        let enclosing = cx.state().math;
        let display = enclosing.is_none_or(|math| math.display);

        // Written outside a formula, a block opens one — so `Source` mode re-emits it
        // rather than drawing it, on the same terms as every other math scope.
        if enclosing.is_none() {
            if let Some(flow) = math::source_scope(node, display, cx.options()) {
                return Ok(flow);
            }
        }

        let mut inner = cx.state().clone();
        inner.math = Some(MathCtx {
            display,
            matrix: true,
        });

        // A multi-row box needs a joiner to compose it with what surrounds it, so it is
        // only ever built where one runs. Without one a matrix takes its single-line
        // form, which is also what v3 renders in every non-fancy mode — but an `align`
        // on a single line would lose its rows, so there it stays the formula it was:
        // `\\` a new display line, `&` nothing.
        let fancy = math::atoms_in_use(&inner, cx.options());
        if !fancy && matches!(columns, Columns::Aligned | Columns::Eqnarray) {
            return Formula { display: true }.render(node, cx);
        }

        let body = cx.body_with_state(inner.clone())?;
        let rows = math::matrix_rows(body, &inner, cx.options());
        let atom = if display && fancy {
            display_block_atom(&rows, fence, &columns, cx.options().matrix_delimiters)
        } else {
            inline_block_atom(&rows, fence, &columns)
        };
        let flow = math::atom(atom, &inner, cx.options());

        Ok(match enclosing {
            None => math::finish(flow, display, cx.options()),
            Some(_) => flow,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Converter;

    fn text(latex: &str) -> alloc::string::String {
        Converter::standard()
            .latex_to_text(latex)
            .expect("parses")
            .text
    }

    #[test]
    fn an_align_body_lines_its_relations_up() {
        assert_eq!(
            text(r"\begin{align} a &= 1 \\ xyz &= 2 \end{align}"),
            "      𝑎 = 1\n    𝑥𝑦𝑧 = 2\n"
        );
    }

    #[test]
    fn a_matrix_inside_a_formula_is_one_opaque_atom() {
        assert_eq!(text(r"$x\begin{pmatrix}1&2\end{pmatrix}y$"), "𝑥( 1  2 )𝑦\n");
        assert_eq!(
            text(r"$\sin\begin{pmatrix}1&2\end{pmatrix}$"),
            "sin( 1  2 )\n"
        );
    }

    #[test]
    fn an_empty_matrix_renders_its_delimiters_and_does_not_panic() {
        assert_eq!(text(r"$\begin{pmatrix}\end{pmatrix}$"), "( )\n");
        // `matrix` has no delimiters at all, so an empty one renders nothing.
        assert_eq!(text(r"$\begin{matrix}\end{matrix}$"), "");
        assert_eq!(text(r"\begin{bmatrix}\end{bmatrix}"), "    [ ]\n");
    }

    #[test]
    fn subequations_renders_the_formulas_inside_it() {
        assert_eq!(
            text(r"\begin{subequations}\begin{equation}a=1\end{equation}\end{subequations}"),
            "    𝑎 = 1\n"
        );
    }
}
