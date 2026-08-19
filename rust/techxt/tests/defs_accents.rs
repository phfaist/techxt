//! `defs::accents`: the accent macros and the first-character rule (PLAN.md §9.3).

use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

#[test]
fn every_punctuation_named_accent() {
    assert_eq!(text(r"\'e"), "é\n");
    assert_eq!(text(r"\`e"), "è\n");
    assert_eq!(text(r#"\"o"#), "ö\n");
    assert_eq!(text(r"\^a"), "â\n");
    assert_eq!(text(r"\~n"), "ñ\n");
    assert_eq!(text(r"\=o"), "ō\n");
    assert_eq!(text(r"\.z"), "ż\n");
}

#[test]
fn every_letter_named_accent() {
    assert_eq!(text(r"\c c"), "ç\n");
    assert_eq!(text(r"\H o"), "ő\n");
    assert_eq!(text(r"\k a"), "ą\n");
    assert_eq!(text(r"\b h"), "ẖ\n");
    assert_eq!(text(r"\d h"), "ḥ\n");
    assert_eq!(text(r"\r a"), "å\n");
    assert_eq!(text(r"\u g"), "ğ\n");
    assert_eq!(text(r"\v s"), "š\n");
}

#[test]
fn the_math_named_accents_are_the_same_marks() {
    assert_eq!(text(r"\acute e"), "é\n");
    assert_eq!(text(r"\grave e"), "è\n");
    assert_eq!(text(r"\hat a"), "â\n");
    assert_eq!(text(r"\check s"), "š\n");
    assert_eq!(text(r"\breve g"), "ğ\n");
    assert_eq!(text(r"\tilde n"), "ñ\n");
    assert_eq!(text(r"\ddot o"), "ö\n");
    assert_eq!(text(r"\dot z"), "ż\n");
    // No precomposed character exists for these, so the mark follows the base.
    assert_eq!(text(r"\vec v"), "v\u{20d7}\n");
    assert_eq!(text(r"\bar x"), "x\u{305}\n");
    assert_eq!(text(r"\not ="), "≠\n");
}

#[test]
fn the_mark_lands_on_the_first_character_only() {
    // The v3 defect this fixes: pylatexenc accents *every* character of the argument,
    // so `\'{abc}` comes out as `ábć`. TeX accents one character.
    assert_eq!(text(r"\'{abc}"), "ábc\n");
    assert_eq!(text(r"\c{cedilla}"), "çedilla\n");
    assert_eq!(text(r#"\"{oo}"#), "öo\n");
}

#[test]
fn a_braced_and_a_bare_argument_agree() {
    assert_eq!(text(r"\'e"), text(r"\'{e}"));
    assert_eq!(text(r"\c c"), text(r"\c{c}"));
}

#[test]
fn a_dotless_letter_gets_its_dot_back_first() {
    assert_eq!(text(r"\^\i"), "î\n");
    assert_eq!(text(r"\'{\i}"), "í\n");
    assert_eq!(text(r"\v{\j}"), "ǰ\n");
    // …and the dotless letters themselves are unchanged without an accent.
    assert_eq!(text(r"\i\j"), "ıȷ\n");
}

#[test]
fn an_empty_argument_gives_the_standalone_form() {
    assert_eq!(text(r"\'{}"), "´\n");
    assert_eq!(text(r"\`{}"), "`\n");
    assert_eq!(text(r"\^{}"), "^\n");
    assert_eq!(text(r#"\"{}"#), "¨\n");
    assert_eq!(text(r"\c{}"), "¸\n");
    assert_eq!(text(r"\v{}"), "ˇ\n");
    // A mark with no standalone form falls back to a space carrying it.
    assert_eq!(text(r"\vec{}"), " \u{20d7}\n");
}

#[test]
fn an_accent_composes_with_what_surrounds_it() {
    // One word: the accented letter is a text item adjacent to its neighbours.
    assert_eq!(text(r"caf\'e"), "café\n");
    assert_eq!(text(r#"na\"ive"#), "naïve\n");
    assert_eq!(text(r"\'Elan"), "Élan\n");
}

#[test]
fn an_accent_on_something_that_does_not_compose_keeps_both() {
    // Still correct text; simply not a single code point.
    assert_eq!(text(r"\'q"), "q\u{301}\n");
}

#[test]
fn an_accent_over_a_multi_word_argument_keeps_the_words() {
    // The argument is flattened to one line (an accent's argument is a character, not
    // a paragraph) but the words are still words.
    assert_eq!(text(r"\'{ab cd}"), "áb cd\n");
}

#[test]
fn accents_are_clean_conversions() {
    for latex in [r"\'e", r"\c c", r"\^\i", r"\'{}", r"\vec v"] {
        let conversion = Converter::standard().latex_to_text(latex).expect("parses");
        assert!(conversion.diagnostics.is_empty(), "{latex:?}");
    }
}
