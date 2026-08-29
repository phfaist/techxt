//! The math definitions and the math handlers (PLAN.md §9.5, §9.7).
//!
//! PLAN.md §15's own examples live in `math_examples.rs`; what is tested here is the
//! behavior around them — the scope boundaries, the three modes' agreements and
//! disagreements, the matrix family, and the cases the port reference
//! (`/home/user/ref/notes/pylatexenc-math.md` §10) singles out as easy to get wrong.

use techxt::convert::{FontStyle, MathMode, MathWrapDelims, MatrixDelims, StdDescentGuardInit};
use techxt::Converter;

/// Convert with the shipped library and the default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// Convert with the shipped library and one option changed.
fn with(build: impl FnOnce(techxt::ConverterBuilder) -> techxt::ConverterBuilder) -> Converter {
    build(Converter::builder()).build().expect("builds")
}

// --------------------------------------------------------------- scope boundaries

#[test]
fn each_formula_is_joined_on_its_own() {
    // Two adjacent formulas are two scopes, not one: `$1$$2$` must not acquire the
    // space that the joiner would put between two digits of one formula.
    assert_eq!(text("$a$$b$"), "𝑎𝑏\n");
    assert_eq!(text("$1$$2$"), "12\n");
    // Whereas within one formula the digit rule does fire.
    assert_eq!(text("${1}2$"), "1 2\n");
}

#[test]
fn a_formula_composes_with_the_text_around_it() {
    assert_eq!(text("a $x$ b"), "a 𝑥 b\n");
    assert_eq!(
        text(r"Hello $x+y$ and \(a \le b\)."),
        "Hello 𝑥 + 𝑦 and 𝑎 ≤ 𝑏.\n"
    );
}

#[test]
fn an_empty_formula_renders_nothing() {
    assert_eq!(text("a $ $ b"), "a b\n");
    // Not even the display block: an empty formula has nothing to indent.
    assert_eq!(text(r"a \[ \] b"), "a b\n");
}

#[test]
fn no_math_atom_reaches_the_layout_engine() {
    // A `MathAtom` that escaped its scope would still render (layout flattens it) but
    // would have lost the joiner's spacing. Checking the *flow* is what pins the
    // invariant rather than the symptom.
    let converter = Converter::standard();
    let tree = converter
        .language()
        .parse(r"a $x^2 + \frac{1}{2}$ b \[ \begin{pmatrix}1&2\end{pmatrix} \]")
        .expect("parses")
        .tree;
    let (flow, _) = converter.tree_to_flow(&tree);
    assert!(
        !flow
            .items()
            .iter()
            .any(|item| matches!(item, techxt::flow::FlowItem::MathAtom(_))),
        "a math atom escaped its scope: {flow:?}"
    );
}

#[test]
fn a_deeply_braced_formula_folds_under_the_runs_descent_guard() {
    // A formula's interior is descended into by the driver like any other subtree — the
    // joiner runs afterwards, on the assembled flow (DECISIONS.md D24) — so it is
    // guarded exactly as the braces outside a formula are, by the run's own descent
    // guard and by nothing special of math's. The guard is configured explicitly
    // because the default budget is deliberately tight (DECISIONS.md D9) and would
    // refuse this depth. The depth is modest because the *parser* recurses too and is
    // the binding constraint on a test thread's smaller stack.
    let converter = Converter::builder()
        .descent_guard(StdDescentGuardInit::depth_limit(1000))
        .build()
        .expect("builds");
    let depth = 40;
    let latex = format!("${}x+y{}$", "{".repeat(depth), "}".repeat(depth));
    assert_eq!(
        converter.latex_to_text(&latex).expect("parses").text,
        "\u{1d465} + \u{1d466}\n"
    );
}

#[test]
fn ensuremath_opens_a_scope_of_its_own() {
    // `\ensuremath` renders its argument as math wherever it is written, so its atoms
    // have to be joined by the argument itself — nothing else will.
    assert_eq!(text(r"\ensuremath{a+b}"), "𝑎 + 𝑏\n");
    assert_eq!(text(r"$\ensuremath{a+b}$"), "𝑎 + 𝑏\n");
}

#[test]
fn text_returns_to_text_mode_inside_a_formula() {
    // Source whitespace counts again inside `\text{…}` (it is text), and the fragment
    // is opaque to the joiner: the `+` in it is not an operator.
    assert_eq!(text(r"$\text{a+b}$"), "a+b\n");
    assert_eq!(text(r"$a+\text{[xyz]}$"), "𝑎 + [xyz]\n");
    assert_eq!(text(r"$2\text{kg}$"), "2 kg\n");
    // And no doubled space where the fragment brought its own.
    assert_eq!(text(r"$\text{if } x > 0$"), "if 𝑥 > 0\n");
    assert_eq!(text(r"$x \text{ if } y$"), "𝑥 if 𝑦\n");
    // `\mbox` is the same macro under another name.
    assert_eq!(text(r"$a\mbox{ and }b$"), "𝑎 and 𝑏\n");
    // The *text* font stack is what applies in there, and the excursion into math
    // never touched it.
    assert_eq!(
        text(r"\textit{Italic text with math: $\mathbf{a+\text{[something]}}$}"),
        "𝐼𝑡𝑎𝑙𝑖𝑐 𝑡𝑒𝑥𝑡 𝑤𝑖𝑡ℎ 𝑚𝑎𝑡ℎ: 𝐚 + [𝑠𝑜𝑚𝑒𝑡ℎ𝑖𝑛𝑔]\n"
    );
}

// ------------------------------------------------------------- scripts (§9.5)

#[test]
fn the_script_ladder_prefers_unicode_then_retries_without_the_joiners_spaces() {
    // Step 1: unicode script characters.
    assert_eq!(text("$x^2$"), "𝑥²\n");
    assert_eq!(text("$a_i$"), "𝑎ᵢ\n");
    // Step 2: the joiner put spaces into `i = 1`, and unicode has no script space, so
    // the lookup is retried without them — which is the whole reason `∑ᵢ₌₁ⁿ` exists.
    assert_eq!(text(r"$\sum_{i=1}^n$"), "∑ᵢ₌₁ⁿ\n");
    // Step 3: LaTeX's own notation, with a retroactive space in front of the base so
    // that one can see where the base begins.
    assert_eq!(text("$x^q$"), "𝑥^𝑞\n");
    assert_eq!(text(r"$4\pi c x^q p$"), "4π𝑐 𝑥^𝑞 𝑝\n");
    // A script that *did* find unicode characters needs no such padding.
    assert_eq!(text(r"$4\pi c x^2 p$"), "4π𝑐𝑥²𝑝\n");
}

#[test]
fn a_superscript_followed_by_a_subscript_lets_the_subscript_bind_first() {
    assert_eq!(text("$x^a_b$"), "𝑥_𝑏ᵃ\n");
}

#[test]
fn a_script_wraps_only_when_a_space_would_not_be_enough() {
    assert_eq!(text("$x_{abc}$"), "𝑥_𝑎𝑏𝑐\n");
    assert_eq!(text("$x_{abc} p$"), "𝑥_𝑎𝑏𝑐 𝑝\n");
    assert_eq!(text("$x_{abc} = 1$"), "𝑥_𝑎𝑏𝑐 = 1\n");
    // A second script on the same base: a space could not tell the two apart.
    assert_eq!(text("$x_{abc}^2$"), "𝑥_(𝑎𝑏𝑐)²\n");
    // And the delimiters are the configured ones.
    let braces = with(|b| b.math_expression_in(MathWrapDelims::Braces));
    assert_eq!(
        braces.latex_to_text("$x_{abc}^2$").expect("parses").text,
        "𝑥_{𝑎𝑏𝑐}²\n"
    );
}

#[test]
fn the_script_specials_do_not_fire_outside_math() {
    assert_eq!(text("a_b and a^b"), "a_b and a^b\n");
}

#[test]
fn an_empty_script_renders_as_nothing() {
    assert_eq!(text("$x^{}$"), "𝑥\n");
}

// ------------------------------------------------- fractions, roots, operators

#[test]
fn a_fraction_wraps_its_operands_eagerly() {
    // DECISIONS.md D1: a true wrappable would give `(𝑎/𝑏)𝑐` here.
    assert_eq!(text(r"$\frac{a}{b}c$"), "𝑎/𝑏 𝑐\n");
    assert_eq!(text(r"$c\frac{a}{b}$"), "𝑐 𝑎/𝑏\n");
    assert_eq!(text(r"$\frac{a+b}{c}$"), "(𝑎 + 𝑏)/𝑐\n");
    assert_eq!(text(r"$\frac{a}{b+c}$"), "𝑎/(𝑏 + 𝑐)\n");
    // TeX's single-expression fallback: `\frac12` is `\frac{1}{2}`.
    assert_eq!(text(r"$\frac12$"), "1/2\n");
}

#[test]
fn a_root_closes_on_the_left_and_opens_on_the_right() {
    assert_eq!(text(r"$2\sqrt{x}$"), "2√𝑥\n");
    assert_eq!(text(r"$\sqrt{x}2$"), "√𝑥 2\n");
    // Degrees 3 and 4 are drawn into the sign; any other is written in front of it.
    assert_eq!(text(r"$\sqrt[3]{x+y}$"), "∛(𝑥 + 𝑦)\n");
    assert_eq!(text(r"$\sqrt[4]{x}$"), "∜𝑥\n");
    assert_eq!(text(r"$\sqrt[n]{x+y}$"), "𝑛√(𝑥 + 𝑦)\n");
}

#[test]
fn a_named_operator_is_upright_and_spaced_as_an_operator() {
    assert_eq!(text(r"$\sin x$"), "sin 𝑥\n");
    assert_eq!(text(r"$\sin(x)$"), "sin(𝑥)\n");
    assert_eq!(text(r"$2\sin x$"), "2 sin 𝑥\n");
    assert_eq!(text(r"$(\sin x)$"), "(sin 𝑥)\n");
    assert_eq!(text(r"$\det(A) \neq 0$"), "det(𝐴) ≠ 0\n");
    assert_eq!(text(r"$\lg n + \coth x$"), "lg 𝑛 + coth 𝑥\n");
    // DECISIONS.md D6: the name is a rule's replacement text, so no alphabet touches
    // it — that is what keeps it upright next to an italic variable.
    assert_eq!(text(r"$\operatorname{tr}(A)$"), "tr(𝐴)\n");
    assert_eq!(text(r"$2\operatorname{tr}A$"), "2 tr 𝐴\n");
    assert_eq!(text(r"$\operatorname*{argmax}_x f(x)$"), "argmaxₓ 𝑓(𝑥)\n");
}

#[test]
fn a_symbols_replacement_is_segmented_and_therefore_classified() {
    assert_eq!(text(r"$a\le b$"), "𝑎 ≤ 𝑏\n");
    assert_eq!(text(r"$a\ne b$"), "𝑎 ≠ 𝑏\n");
    assert_eq!(text(r"$x\to y$"), "𝑥 → 𝑦\n");
    assert_eq!(text(r"$\alpha=1$"), "α = 1\n");
    // An ordinary symbol joins tightly, a relation does not.
    assert_eq!(text(r"$4\pi c$"), "4π𝑐\n");
}

// --------------------------------------------------------------------- fonts

#[test]
fn upright_letters_read_as_an_operator_only_while_variables_are_styled() {
    // DECISIONS.md D4. With the default italic math font, a run of upright letters
    // stands out as a name; with no alphabet at all it does not, and `2xy` would
    // otherwise be read as a function called `xy`.
    assert_eq!(text("$2xy$"), "2𝑥𝑦\n");
    let bare = with(|b| b.math_font(FontStyle::Disabled));
    assert_eq!(bare.latex_to_text("$2 x y$").expect("parses").text, "2xy\n");
    // The declared operators still space themselves, because their class is explicit.
    assert_eq!(
        bare.latex_to_text(r"$2 \sin x$").expect("parses").text,
        "2 sin x\n"
    );
}

#[test]
fn a_font_macro_inside_math_writes_the_math_stack() {
    assert_eq!(text(r"$\mathbf{x}$"), "𝐱\n");
    assert_eq!(text(r"$\mathbb{R}$"), "ℝ\n");
    assert_eq!(text(r"$\mathbf{a+b}$"), "𝐚 + 𝐛\n");
    // DECISIONS.md D7: `Style(Upright)` is a style, and a nested one still overrides it.
    assert_eq!(text(r"$\mathrm{d}x$"), "d𝑥\n");
    assert_eq!(text(r"$\mathbf{a\mathrm{b}c}$"), "𝐚b𝐜\n");
}

// ------------------------------------------------------------------ matrices

#[test]
fn an_inline_matrix_is_one_line_with_the_environments_own_delimiters() {
    // DECISIONS.md D3, one per environment.
    assert_eq!(
        text(r"$\begin{pmatrix}1&2\\3&4\end{pmatrix}$"),
        "( 1  2; 3  4 )\n"
    );
    assert_eq!(text(r"$\begin{bmatrix}1&2\end{bmatrix}$"), "[ 1  2 ]\n");
    assert_eq!(text(r"$\begin{Bmatrix}1&2\end{Bmatrix}$"), "{ 1  2 }\n");
    assert_eq!(text(r"$\begin{vmatrix}1&2\end{vmatrix}$"), "| 1  2 |\n");
    assert_eq!(text(r"$\begin{Vmatrix}1&2\end{Vmatrix}$"), "‖ 1  2 ‖\n");
    assert_eq!(text(r"$\begin{matrix}1&2\end{matrix}$"), "1  2\n");
    assert_eq!(text(r"$\begin{smallmatrix}1&2\end{smallmatrix}$"), "1  2\n");
}

#[test]
fn a_matrix_hugs_its_neighbours_because_it_delimits_itself() {
    assert_eq!(text(r"$x\begin{pmatrix}1&2\end{pmatrix}y$"), "𝑥( 1  2 )𝑦\n");
    assert_eq!(
        text(r"$A = \begin{pmatrix}1&2\\3&4\end{pmatrix}$"),
        "𝐴 = ( 1  2; 3  4 )\n"
    );
    // No retroactive space in front of a base that is already self-delimited.
    assert_eq!(
        text(r"$2\begin{pmatrix}1&2\end{pmatrix}^{qr}$"),
        "2( 1  2 )^𝑞𝑟\n"
    );
}

#[test]
fn a_display_matrix_is_one_line_per_row() {
    // The same matrix, inline and displayed.
    assert_eq!(
        text(r"$\begin{bmatrix}1&2\\3&4\end{bmatrix}$"),
        "[ 1  2; 3  4 ]\n"
    );
    assert_eq!(
        text(r"\[\begin{bmatrix}1&2\\3&4\end{bmatrix}\]"),
        "    ⎡ 1  2 ⎤\n    ⎣ 3  4 ⎦\n"
    );
}

#[test]
fn a_tall_brace_gets_its_extension_piece() {
    // DECISIONS.md D2: U+23A8 `⎨` is the brace's *centre*, not an extension, so a
    // matrix taller than three rows needs U+23AA `⎪` for the rest.
    assert_eq!(
        text(r"\[\begin{Bmatrix}1&2\\3&4\\5&6\\7&8\end{Bmatrix}\]"),
        "    ⎧ 1  2 ⎫\n    ⎨ 3  4 ⎬\n    ⎪ 5  6 ⎪\n    ⎩ 7  8 ⎭\n"
    );
}

#[test]
fn an_empty_matrix_does_not_panic() {
    assert_eq!(text(r"$\begin{pmatrix}\end{pmatrix}$"), "( )\n");
    assert_eq!(text(r"\[\begin{bmatrix}\end{bmatrix}\]"), "    [ ]\n");
    assert_eq!(text(r"$\begin{matrix}\end{matrix}$"), "");
    // A matrix that is nothing but separators is empty too.
    assert_eq!(text(r"$\begin{pmatrix} & \\ \end{pmatrix}$"), "( )\n");
}

#[test]
fn a_display_matrix_composes_with_the_formula_around_it() {
    // The 2D baseline join: the surrounding content sits on the box's baseline row and
    // the other rows keep the column.
    assert_eq!(
        text(r"\[ A \begin{pmatrix} 1 \\ 2 \end{pmatrix} \]"),
        "    𝐴⎛ 1 ⎞\n     ⎝ 2 ⎠\n"
    );
}

#[test]
fn an_array_ignores_its_column_specification_and_squares_its_rows() {
    assert_eq!(
        text(r"\[\begin{array}{lll}1 & 2 & abcdef\\ 3 & 4\end{array}\]"),
        "    ⎡ 1  2  𝑎𝑏𝑐𝑑𝑒𝑓 ⎤\n    ⎣ 3  4         ⎦\n"
    );
}

#[test]
fn plain_mode_renders_a_matrix_on_one_line() {
    // There is no joiner to compose a multi-row box with, so `Plain` uses the inline
    // shape everywhere — which is also what v3 does in every non-fancy mode.
    let plain = with(|b| b.math_mode(MathMode::Plain));
    assert_eq!(
        plain
            .latex_to_text(r"\[\begin{pmatrix}1&2\\3&4\end{pmatrix}\]")
            .expect("parses")
            .text,
        "    ( 1  2; 3  4 )\n"
    );
    // And the delimiter style is a display-only setting, so it changes nothing here.
    let ascii = with(|b| {
        b.math_mode(MathMode::Plain)
            .matrix_delimiters(MatrixDelims::Ascii)
    });
    assert_eq!(
        ascii
            .latex_to_text(r"\[\begin{pmatrix}1&2\\3&4\end{pmatrix}\]")
            .expect("parses")
            .text,
        "    ( 1  2; 3  4 )\n"
    );
}

// -------------------------------------------- math environments (§9.5, §9.7)

#[test]
fn a_math_environment_is_display_math() {
    assert_eq!(
        text(r"\begin{equation} a^2 + b^2 = c^2 \end{equation}"),
        "    𝑎² + 𝑏² = 𝑐²\n"
    );
    for name in ["equation*", "gather", "multline", "eqnarray", "dmath"] {
        assert_eq!(
            text(&format!(r"\begin{{{name}}} a = 1 \end{{{name}}}")),
            "    𝑎 = 1\n",
            "{name}"
        );
    }
}

#[test]
fn a_math_environment_re_emits_its_source_in_source_mode() {
    // A math environment opens a formula (PLAN.md §9.5), so `Source` mode shows it
    // rather than rendering it — the same answer `\[…\]` gives. Answering at the group
    // alone is what once made one equation read two ways depending on its spelling.
    let source = with(|b| b.math_mode(MathMode::Source));
    let round_trips = |latex: &str| {
        assert_eq!(
            source.latex_to_text(latex).expect("parses").text,
            format!("{latex}\n"),
            "{latex}"
        );
    };

    round_trips(r"\[ \sum_{k=1}^\infty a_k \]");
    round_trips(r"\begin{equation} \sum_{k=1}^\infty a_k \end{equation}");
    for name in [
        "equation*",
        "align",
        "gather",
        "multline",
        "eqnarray",
        "dmath",
    ] {
        round_trips(&format!(
            r"\begin{{{name}}} a &= 1 \\ b &= 2 \end{{{name}}}"
        ));
    }
    // An environment argument is part of the invocation and comes back with it.
    round_trips(r"\begin{alignat}{2} a &= 1 \end{alignat}");
    round_trips(r"\begin{array}{cc} 1 & 2 \end{array}");
    // As do a matrix's delimiters, which are its name rather than its body.
    round_trips(r"\begin{pmatrix} 1 & 2 \\ 30 & 4 \end{pmatrix}");
}

#[test]
fn source_mode_separates_an_expanded_macro_from_the_text_after_it() {
    // A macro at the end of a `\newcommand` body swallowed no post-space — nothing
    // followed it in the definition — so re-emitting the expansion next to the text
    // after the invocation once ran the two together: `\ranglex`, a name MathJax has
    // never heard of, in place of the `\rangle` and the `x` the tree actually holds.
    let source = with(|b| b.math_mode(MathMode::Source));
    let text = |latex: &str| source.latex_to_text(latex).expect("parses").text;

    assert_eq!(
        text(r"\newcommand\ketx[1]{\lvert{#1}\rangle}$\ketx\phi x$"),
        "$\\lvert{\\phi}\\rangle x$\n"
    );
    // The separator goes in only where the name would otherwise grow: an escape
    // character, a digit and a brace all end a name on their own.
    assert_eq!(
        text(r"\newcommand\aa{\alpha}$\aa\beta$ and $\aa 12$ and $\aa{}b$"),
        "$\\alpha\\beta$ and $\\alpha12$ and $\\alpha{}b$\n"
    );
}

#[test]
fn source_mode_shows_a_formula_once_and_does_not_enter_it() {
    // An environment inside a formula is part of that formula's source, and the
    // formula is re-emitted whole: the inner one is not shown a second time, and
    // nothing inside either of them renders.
    let source = with(|b| b.math_mode(MathMode::Source));
    assert_eq!(
        source
            .latex_to_text(r"\[\begin{split} a &= b \end{split}\]")
            .expect("parses")
            .text,
        "\\[\\begin{split} a &= b \\end{split}\\]\n"
    );
    // `subequations` is not a formula — it is document content holding formulas — so
    // it drops away and each equation inside it answers for itself.
    assert_eq!(
        source
            .latex_to_text(
                r"\begin{subequations}\begin{equation}a=1\end{equation}\begin{equation}b=2\end{equation}\end{subequations}"
            )
            .expect("parses")
            .text,
        "\\begin{equation}a=1\\end{equation}\n\n\\begin{equation}b=2\\end{equation}\n"
    );
}

#[test]
fn source_mode_keeps_an_inline_formula_in_its_running_text() {
    // `\begin{math}` and `\ensuremath` are `$…$` in other words: they open an inline
    // scope, so their source stays in the line instead of taking a block of its own.
    let source = with(|b| b.math_mode(MathMode::Source));
    let text = |latex: &str| source.latex_to_text(latex).expect("parses").text;

    assert_eq!(text("a $x^2$ b"), "a $x^2$ b\n");
    assert_eq!(
        text(r"a \begin{math}x^2\end{math} b"),
        "a \\begin{math}x^2\\end{math} b\n"
    );
    assert_eq!(text(r"a \ensuremath{x^2} b"), "a \\ensuremath{x^2} b\n");
    // A display environment does stand apart, exactly as `\[…\]` does.
    assert_eq!(
        text(r"a \begin{equation}x^2\end{equation} b"),
        "a\n\n\\begin{equation}x^2\\end{equation}\n\nb\n"
    );
}

#[test]
fn inside_a_math_environment_a_break_is_a_line_and_an_ampersand_is_nothing() {
    // PLAN.md §9.7: the joiner already spaces the relation the `&` was aligning.
    assert_eq!(
        text(r"\begin{align} a &= 1 \\ b &= 2 \end{align}"),
        "    𝑎 = 1\n    𝑏 = 2\n"
    );
    assert_eq!(
        text(r"\begin{align*} x &\le y \\ y &\le z \end{align*}"),
        "    𝑥 ≤ 𝑦\n    𝑦 ≤ 𝑧\n"
    );
}

#[test]
fn split_contributes_to_the_formula_that_encloses_it() {
    assert_eq!(
        text(r"\begin{equation}\begin{split} a &= b \\ &= c \end{split}\end{equation}"),
        "    𝑎 = 𝑏\n    = 𝑐\n"
    );
}

#[test]
fn subequations_renders_the_equations_inside_it() {
    assert_eq!(
        text(
            r"\begin{subequations}\begin{equation}a=1\end{equation}\begin{equation}b=2\end{equation}\end{subequations}"
        ),
        "    𝑎 = 1\n\n    𝑏 = 2\n"
    );
}

#[test]
fn a_display_formula_stands_apart_from_the_text_around_it() {
    assert_eq!(
        text(r"Before \[ x = 1 \] after."),
        "Before\n\n    𝑥 = 1\n\nafter.\n"
    );
}

#[test]
fn display_math_is_never_re_wrapped() {
    // The joiner has already laid the line out; a wrap width must not touch it.
    let narrow = with(|b| b.wrap_width(Some(12)));
    assert_eq!(
        narrow
            .latex_to_text(r"\[ a + b + c + d + e + f \]")
            .expect("parses")
            .text,
        "    𝑎 + 𝑏 + 𝑐 + 𝑑 + 𝑒 + 𝑓\n"
    );
    // Inline math may wrap, at the joiner's own spaces and nowhere else.
    assert_eq!(
        narrow
            .latex_to_text(r"one $a + b + c$ two")
            .expect("parses")
            .text,
        "one 𝑎 + 𝑏 +\n𝑐 two\n"
    );
}

// ------------------------------------------ binomials, stacking, and the inner envs

#[test]
fn a_binomial_coefficient_names_itself() {
    assert_eq!(text(r"$\binom{n}{k}$"), "(𝑛 choose 𝑘)\n");
    assert_eq!(text(r"$\dbinom{n+1}{k}$"), "(𝑛 + 1 choose 𝑘)\n");
}

#[test]
fn stacking_macros_render_the_annotation_as_a_script() {
    // The one way plain text can put something above a baseline.
    assert_eq!(text(r"$a \stackrel{\mathrm{def}}{=} b$"), "𝑎 =ᵈᵉᶠ 𝑏\n");
    assert_eq!(text(r"$\overset{a}{b}$"), "𝑏ᵃ\n");
    assert_eq!(text(r"$\underset{i}{\max}$"), "maxᵢ\n");
    // An annotation unicode cannot set as a script falls back to LaTeX's own notation
    // rather than being dropped.
    assert_eq!(text(r"$\overset{Q}{x}$"), "𝑥^𝑄\n");
}

#[test]
fn the_inner_math_environments_hand_their_atoms_to_the_formula_around_them() {
    assert_eq!(
        text(r"\begin{equation}\begin{aligned} a &= b \\ c &= d \end{aligned}\end{equation}"),
        "    𝑎 = 𝑏\n    𝑐 = 𝑑\n"
    );
    assert_eq!(
        text(r"\begin{displaymath} x = y \end{displaymath}"),
        "    𝑥 = 𝑦\n"
    );
    // `\begin{math}` is `$…$` written out: inline wherever it stands.
    assert_eq!(
        text(r"in \begin{math}a+b\end{math} here"),
        "in 𝑎 + 𝑏 here\n"
    );
}

#[test]
fn cases_is_a_matrix_with_braces() {
    assert_eq!(
        text(r"\begin{equation}\begin{cases} 1 & x > 0 \\ 0 & x \le 0 \end{cases}\end{equation}"),
        "    ⎧ 1  𝑥 > 0 ⎫\n    ⎩ 0  𝑥 ≤ 0 ⎭\n"
    );
    assert_eq!(text(r"$\begin{cases} a \\ b \end{cases}$"), "{ 𝑎; 𝑏 }\n");
}

/// mathtools' seven spellings of `cases` render as the one environment they are: the
/// display size `d…` asks for is not a thing plain text has, and `rcases`' right-hand
/// brace is a pair here for the reason `cases`' left-hand one is.
#[test]
fn the_rest_of_the_cases_family_renders_as_cases_does() {
    let body = |name: &str| {
        text(&format!(
            r"\[\begin{{{name}}} 1 & x > 0 \\ 0 & x \le 0 \end{{{name}}}\]"
        ))
    };
    let expected = "    ⎧ 1  𝑥 > 0 ⎫\n    ⎩ 0  𝑥 ≤ 0 ⎭\n";
    for name in [
        "cases", "cases*", "dcases", "dcases*", "rcases", "rcases*", "drcases", "drcases*",
    ] {
        assert_eq!(body(name), expected, "{name}");
    }
}

/// And none of them reaches the unknown-environment policy on the way there.
#[test]
fn the_cases_family_converts_without_a_diagnostic() {
    for name in [
        "cases", "cases*", "dcases", "dcases*", "rcases", "rcases*", "drcases", "drcases*",
    ] {
        let converted = Converter::standard()
            .latex_to_text(&format!(r"\[\begin{{{name}}} a \\ b \end{{{name}}}\]"))
            .expect("parses");
        assert!(
            converted.diagnostics.is_empty(),
            "{name}: {:?}",
            converted.diagnostics
        );
    }
}

#[test]
fn math_style_declarations_are_known_and_render_as_nothing() {
    let converted = Converter::standard()
        .latex_to_text(r"$\displaystyle \sum\limits_{i} x_i \nonumber$")
        .expect("parses");
    assert_eq!(converted.text, "∑ᵢ 𝑥ᵢ\n");
    assert!(
        converted.diagnostics.is_empty(),
        "{:?}",
        converted.diagnostics
    );
}
