//! The normative acceptance examples of PLAN.md §15 that the definitions library
//! covers, end to end through `Converter::standard()`.
//!
//! Expected strings are exact, trailing newline included. These are behaviour law: an
//! example that changes is a design change, not a test that needs updating.

use techxt::Converter;

/// Convert with the shipped definitions and the default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("a tolerant parse of well-formed input succeeds")
        .text
}

#[test]
fn example_3_emph_italicizes() {
    assert_eq!(text(r"\emph{sic}"), "𝑠𝑖𝑐\n");
}

#[test]
fn example_4_textbf_is_bold_and_joins_what_follows() {
    // One unbreakable word: the bold letters and the plain ones are adjacent text
    // items, so no line break may fall between them.
    assert_eq!(text(r"\textbf{bold}text"), "𝐛𝐨𝐥𝐝text\n");
}

#[test]
fn example_5_a_text_symbol_and_its_post_space() {
    assert_eq!(text(r"Sk\l odowska"), "Skłodowska\n");
}

#[test]
fn example_6_accents_braced_and_bare() {
    assert_eq!(text(r"\'{e}t\'e"), "été\n");
}

#[test]
fn example_7_an_accent_takes_a_bare_argument() {
    assert_eq!(text(r"\c c"), "ç\n");
}

#[test]
fn example_8_ligatures() {
    assert_eq!(text("``Hi,'' -- ok"), "“Hi,” – ok\n");
}

#[test]
fn example_9_comments_render_as_nothing() {
    assert_eq!(text("A% note\nB"), "AB\n");
}

#[test]
fn example_16_a_numbered_section_over_its_rule() {
    assert_eq!(
        text("\\section{Intro}\nText."),
        "1 Intro\n-------\n\nText.\n"
    );
}

#[test]
fn example_17_a_starred_subsection_is_neither_numbered_nor_counted() {
    assert_eq!(text(r"\subsection*{Notes}"), "Notes\n~~~~~\n");
    // "Not counted": the starred one left the subsection counter alone, so the next
    // subsection is the first. (The leading `0` is the section counter, which no
    // `\section` has raised — exactly what LaTeX prints in the same situation.)
    assert_eq!(
        text(r"\subsection*{Notes}\subsection{Real}"),
        "Notes\n~~~~~\n\n0.1 Real\n~~~~~~~~\n"
    );
}

#[test]
fn example_20_href_renders_text_then_url() {
    assert_eq!(
        text(r"\href{https://ex.org/a_b}{link}"),
        "link <https://ex.org/a_b>\n"
    );
}

#[test]
fn the_examples_are_clean_conversions() {
    // None of the above should report anything: they are all constructs techxt knows.
    for latex in [
        r"\emph{sic}",
        r"\textbf{bold}text",
        r"Sk\l odowska",
        r"\'{e}t\'e",
        r"\c c",
        "``Hi,'' -- ok",
        "A% note\nB",
        "\\section{Intro}\nText.",
        r"\subsection*{Notes}",
        r"\href{https://ex.org/a_b}{link}",
    ] {
        let conversion = Converter::standard().latex_to_text(latex).expect("parses");
        assert!(
            conversion.diagnostics.is_empty(),
            "{latex:?} reported {:?}",
            conversion
                .diagnostics
                .iter()
                .map(|d| d.identifier())
                .collect::<Vec<_>>()
        );
    }
}
