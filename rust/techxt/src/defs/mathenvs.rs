//! Display-math environments and matrices (PLAN.md §9.5).
//!
//! `equation`, `align`, `gather`, `multline`, `eqnarray`, `split` and `dmath` (with
//! their starred forms), `subequations`, and the matrix family — `matrix`, `pmatrix`,
//! `bmatrix`, `Bmatrix`, `vmatrix`, `Vmatrix`, `smallmatrix`, the `cases` family and
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
//! # Two separators, three meanings
//!
//! The `&` and `\\` that [`defs::base`](super::base) defines read
//! [`RenderState`](crate::render::RenderState) to decide what they are (PLAN.md §9.7),
//! and these environments are what sets it. In a matrix body
//! [`MathCtx::matrix`](crate::render::MathCtx::matrix) is set, so they become cell and
//! row separators that the matrix handler splits at. In an `align` body it is not, so
//! `\\` is a new display line and `&` is *nothing at all*: the joiner already spaces the
//! relation the `&` was aligning, and a rendering that is not in columns has nothing to
//! align it to.

use crate::def::{Category, EnvDef, TextHandler, TextRule};
use crate::flow::Flow;
use crate::mathfmt::{display_matrix_atom, inline_matrix_atom, MatrixFence};
use crate::render::{math, MathCtx, NodeView, RenderCx, RenderError};

use super::handler;

/// The mathenvs category (PLAN.md §12.1).
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
    matrices(&mut category);
    category
}

// ------------------------------------------------------- display environments

/// The environments whose body is one displayed formula.
///
/// All of them render alike: plain text has no equation numbers to place, no columns to
/// align on and no page width to break a `multline` across, so what is left of each is
/// "a formula, displayed" — and `\\` inside one starts the next line of it.
fn display_environments(category: &mut Category) {
    for name in [
        "equation",
        "equation*",
        "align",
        "align*",
        "gather",
        "gather*",
        "multline",
        "multline*",
        "eqnarray",
        "eqnarray*",
        "split",
        "dmath",
        "dmath*",
        // LaTeX's own displayed-formula environment, which `\[…\]` is short for.
        "displaymath",
        "flalign",
        "flalign*",
        // The inner forms: an `aligned` or a `gathered` is written *inside* another
        // formula, and renders by handing its atoms to it — which is what every
        // environment here does when math already surrounds it.
        "aligned",
        "gathered",
    ] {
        category.add_env(
            EnvDef::new(name)
                .math_body()
                .rule(handler(Formula { display: true })),
        );
    }
    // `alignat` and its relatives take the number of column pairs, which says nothing
    // about the text; it is parsed so that it cannot leak, and dropped.
    for name in ["alignat", "alignat*", "alignedat"] {
        category.add_env(
            EnvDef::new(name)
                .arg("m", "columns")
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

// ------------------------------------------------------------------ matrices

/// The matrix family, plus `array` (PLAN.md §9.5, DECISIONS.md D3).
///
/// The environment's *name* is what says which delimiters it draws — that is how LaTeX
/// spells them — so one handler serves them all and asks
/// [`MatrixFence::for_environment`] which pair to use.
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
        category.add_env(EnvDef::new(name).math_body().rule(handler(Matrix)));
    }
    // `array`'s column specification says how each column is aligned and where its
    // rules go. Plain text right-justifies every column (a column of numbers lining up
    // on its units digit is the one alignment that always helps), so the argument is
    // parsed — which is what keeps it out of the output — and then ignored.
    category.add_env(
        EnvDef::new("array")
            .arg("o", "position")
            .arg("m", "colspec")
            .math_body()
            .rule(handler(Matrix)),
    );
}

// ------------------------------------------------------------------ handlers

/// One formula environment: `equation`, `align`, `gather`, `math`, … .
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
        // A math environment *is* display math (PLAN.md §9.5) — unless it is written
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

/// One matrix (PLAN.md §9.5).
///
/// Inline, a matrix is a single-line `[ 1  2; 30  4 ]`; displayed, it is one line per
/// row between delimiters drawn as tall as it is. Either way it is **one atom**, opaque
/// to the joiner and self-delimiting on both edges, so `$x\begin{pmatrix}1&2\end{pmatrix}y$`
/// hugs the way a bracketed expression does.
#[derive(Debug)]
struct Matrix;

impl TextHandler for Matrix {
    fn render(&self, node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let name = node.name().unwrap_or("");
        // A name the fence table does not know can only be an environment registered
        // elsewhere against this handler; brackets are v3's universal pair and the
        // safe reading of "some kind of matrix".
        let fence = MatrixFence::for_environment(name).unwrap_or(MatrixFence::Brackets);

        let enclosing = cx.state().math;
        let display = enclosing.is_none_or(|math| math.display);

        // Written outside a formula, a matrix opens one — so `Source` mode re-emits it
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
        let body = cx.body_with_state(inner.clone())?;
        let rows = math::matrix_rows(body, &inner, cx.options());

        // A multi-row box needs a joiner to compose it with what surrounds it, so it is
        // only ever built where one runs; `Plain` gets the single-line form, which is
        // also what v3 renders in every non-fancy mode.
        let atom = if display && math::atoms_in_use(&inner, cx.options()) {
            display_matrix_atom(&rows, fence, cx.options().matrix_delimiters)
        } else {
            inline_matrix_atom(&rows, fence)
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
    fn an_alignment_marker_is_dropped_and_a_break_starts_a_line() {
        assert_eq!(
            text(r"\begin{align} a &= 1 \\ b &= 2 \end{align}"),
            "    𝑎 = 1\n    𝑏 = 2\n"
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
