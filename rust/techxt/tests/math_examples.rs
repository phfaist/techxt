//! PLAN.md §15's normative math examples, end to end through [`Converter`].
//!
//! These are behavior law: every assertion here is an exact string from the plan's
//! acceptance table, converted with the shipped definitions and (except where a test
//! says otherwise) the default options.

use techxt::convert::{MathMode, MatrixDelims};
use techxt::Converter;

/// Convert with the shipped library and the default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

#[test]
fn example_10_scripts() {
    assert_eq!(text("$x^2 + y_i$"), "𝑥² + 𝑦ᵢ\n");
}

#[test]
fn example_11_a_fraction_beside_a_named_operator() {
    // The numerator needs parentheses and the denominator does not; the fraction is
    // open-ended on the right, so `sin` is set apart from it.
    assert_eq!(text(r"$\frac{4\pi c}{2}\sin(x+y)$"), "(4π𝑐)/2 sin(𝑥 + 𝑦)\n");
}

#[test]
fn example_12_a_sum_with_both_limits() {
    assert_eq!(text(r"$\sum_{i=1}^n x_i$"), "∑ᵢ₌₁ⁿ 𝑥ᵢ\n");
}

#[test]
fn example_13_plain_mode_keeps_the_fonts_and_drops_the_spacing() {
    let converter = Converter::builder()
        .math_mode(MathMode::Plain)
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text("$a + b$").expect("parses").text,
        "𝑎+𝑏\n"
    );
}

#[test]
fn example_14_source_mode_re_emits_the_latex() {
    let converter = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text("$a + b$").expect("parses").text,
        "$a + b$\n"
    );
}

#[test]
fn example_15_display_math_is_a_four_space_block() {
    assert_eq!(text(r"\[ E = mc^2 \]"), "    𝐸 = 𝑚𝑐²\n");
}

#[test]
fn example_23_a_display_matrix_in_both_delimiter_styles() {
    let latex = r"\[\begin{pmatrix} 1 & 2 \\ 30 & 4 \end{pmatrix}\]";
    // Unicode (the default): delimiter pieces stacked to the matrix's height.
    assert_eq!(text(latex), "    ⎛  1  2 ⎞\n    ⎝ 30  4 ⎠\n");

    let ascii = Converter::builder()
        .matrix_delimiters(MatrixDelims::Ascii)
        .build()
        .expect("builds");
    assert_eq!(
        ascii.latex_to_text(latex).expect("parses").text,
        "    (  1  2 )\n    ( 30  4 )\n"
    );
}

#[test]
fn example_23_holds_for_a_bare_matrix_environment_too() {
    // A math environment *is* display math (PLAN.md §9.5), so the matrix renders the
    // same whether or not a display group was written around it.
    assert_eq!(
        text(r"\begin{pmatrix} 1 & 2 \\ 30 & 4 \end{pmatrix}"),
        "    ⎛  1  2 ⎞\n    ⎝ 30  4 ⎠\n"
    );
}

// ------------------------------------------------- the three modes, one sample

/// The sample all three modes are compared on: a fraction, a named operator, a script
/// and source whitespace, so that every part of the pipeline shows in the answer.
const SAMPLE: &str = r"$\frac{a+b}{2} \sin x^2$";

#[test]
fn fancy_mode_is_the_default_and_spaces_the_formula() {
    assert_eq!(text(SAMPLE), "(𝑎 + 𝑏)/2 sin 𝑥²\n");
    assert_eq!(Converter::standard().options().math_mode, MathMode::Fancy);
}

#[test]
fn plain_mode_converts_the_same_and_concatenates() {
    // Same symbols, same scripts, same alphabets, same fraction delimiters — the
    // difference is only that no joiner ran, so nothing is spaced.
    let converter = Converter::builder()
        .math_mode(MathMode::Plain)
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(SAMPLE).expect("parses").text,
        "(𝑎+𝑏)/2sin𝑥²\n"
    );
}

#[test]
fn source_mode_hands_back_the_latex() {
    let converter = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(SAMPLE).expect("parses").text,
        "$\\frac{a+b}{2} \\sin x^2$\n"
    );
}
