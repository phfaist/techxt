//! `defs::fontstyles`: the font macros, the two stacks, and `\emph`'s toggle
//! (PLAN.md §9.5, DECISIONS.md D6 and D7).

use techxt::mathfmt::FontStyle;
use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// -------------------------------------------------------------- the alphabets

#[test]
fn the_text_mode_family() {
    assert_eq!(text(r"\textbf{ab}"), "𝐚𝐛\n");
    assert_eq!(text(r"\textit{ab}"), "𝑎𝑏\n");
    assert_eq!(text(r"\textsl{ab}"), "𝑎𝑏\n");
    assert_eq!(text(r"\textsf{ab}"), "𝖺𝖻\n");
    assert_eq!(text(r"\texttt{ab}"), "𝚊𝚋\n");
    // Upright is a style that maps letters to themselves.
    assert_eq!(text(r"\textrm{ab}"), "ab\n");
    assert_eq!(text(r"\textup{ab}"), "ab\n");
    assert_eq!(text(r"\textmd{ab}"), "ab\n");
    assert_eq!(text(r"\textnormal{ab}"), "ab\n");
    // Small capitals have no alphabet, and techxt never changes case.
    assert_eq!(text(r"\textsc{ab}"), "ab\n");
}

#[test]
fn the_math_mode_family_used_in_text_styles_the_text() {
    // A math font macro outside a formula is an error in LaTeX; techxt honours it
    // against the stack that governs the text it was written in.
    assert_eq!(text(r"\mathbf{ab}"), "𝐚𝐛\n");
    assert_eq!(text(r"\mathbb{AB}"), "𝔸𝔹\n");
    assert_eq!(text(r"\mathcal{AB}"), "𝒜ℬ\n");
    assert_eq!(text(r"\mathscr{AB}"), "𝒜ℬ\n");
    assert_eq!(text(r"\mathfrak{AB}"), "𝔄𝔅\n");
    assert_eq!(text(r"\mathtt{ab}"), "𝚊𝚋\n");
    assert_eq!(text(r"\mathsf{ab}"), "𝖺𝖻\n");
    assert_eq!(text(r"\mathit{ab}"), "𝑎𝑏\n");
    assert_eq!(text(r"\mathrm{ab}"), "ab\n");
}

#[test]
fn only_ascii_letters_are_mapped() {
    assert_eq!(text(r"\textbf{a1é!}"), "𝐚1é!\n");
}

// -------------------------------------------------------------------- nesting

#[test]
fn a_nested_style_overrides_an_upright_one() {
    // DECISIONS.md D7: `Upright` is a style, not the absence of one, so a nested
    // `\mathbf` still applies.
    assert_eq!(text(r"\mathrm{a\mathbf{b}}"), "a𝐛\n");
    assert_eq!(text(r"\textrm{a\textbf{b}}"), "a𝐛\n");
}

#[test]
fn a_style_is_restored_when_its_argument_ends() {
    assert_eq!(text(r"a\textbf{b}c"), "a𝐛c\n");
    assert_eq!(text(r"\textbf{a\textit{b}c}"), "𝐚𝑏𝐜\n");
}

#[test]
fn styling_applies_to_document_characters_only() {
    // DECISIONS.md D6: a rule's replacement text is not document text. `\ldots` stays
    // an ellipsis and `\TeX` stays upright even inside a bold argument.
    assert_eq!(text(r"\textbf{a\ldots b}"), "𝐚…𝐛\n");
    assert_eq!(text(r"\textbf{\TeX}"), "TeX\n");
}

// -------------------------------------------------------------------- \emph

#[test]
fn emph_toggles_rather_than_setting() {
    assert_eq!(text(r"\emph{ab}"), "𝑎𝑏\n");
    // Inside italics it goes back upright, which is what `\emph` is for.
    assert_eq!(text(r"\textit{a\emph{b}c}"), "𝑎b𝑐\n");
    // And it composes with a weight or a family that is already in force.
    assert_eq!(text(r"\textbf{\emph{ab}}"), "𝒂𝒃\n");
    assert_eq!(text(r"\textsf{\emph{ab}}"), "𝘢𝘣\n");
    // Twice is back where it started.
    assert_eq!(text(r"\emph{\emph{ab}}"), "ab\n");
}

// ------------------------------------------------------------ \operatorname

#[test]
fn operatorname_typesets_its_argument_upright() {
    assert_eq!(text(r"\operatorname{tr}"), "tr\n");
    assert_eq!(text(r"\operatorname*{tr}"), "tr\n");
    // Upright wins over the italics around it, and a nested style still overrides.
    assert_eq!(text(r"\textit{a\operatorname{tr}b}"), "𝑎tr𝑏\n");
}

// -------------------------------------------------------------- the off switch

#[test]
fn disabling_the_alphabet_disables_it_for_every_macro() {
    // `FontStyle::Disabled` is the option-level off switch and is deliberately sticky:
    // no macro may turn mapping back on.
    let converter = Converter::builder()
        .text_font(FontStyle::Disabled)
        .build()
        .expect("builds");
    for latex in [r"\textbf{ab}", r"\emph{ab}", r"\mathbb{AB}"] {
        let converted = converter.latex_to_text(latex).expect("parses").text;
        assert!(converted.is_ascii(), "{latex:?} produced {converted:?}");
    }
}

// ------------------------------------------------------------- declarations

#[test]
fn font_and_size_declarations_are_known_and_change_nothing() {
    // A declaration changes the font for the rest of its group, and techxt's font
    // state travels down into arguments rather than sideways to siblings — so the
    // styling is lost and every character is kept. What matters here is that it is
    // *known*: no diagnostic, and no leaked command name.
    let converted = Converter::standard()
        .latex_to_text(r"{\bfseries loud} and {\Large big} and {\itshape slanted}")
        .expect("parses");
    assert_eq!(converted.text, "loud and big and slanted\n");
    assert!(
        converted.diagnostics.is_empty(),
        "{:?}",
        converted.diagnostics
    );
    // The argument forms are what carries a font into the output.
    assert_eq!(text(r"\textbf{loud}"), "𝐥𝐨𝐮𝐝\n");
}
