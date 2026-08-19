//! `defs::symbols_extra`: the generated long tail (PLAN.md §12.1, §12.3, §12.4).
//!
//! The table has about a thousand rows, and testing a thousand rows one at a time
//! would test the generator's output against a transcription of the generator's
//! output. So it is tested three ways instead: its *shape* is checked in the crate's
//! own unit tests (sorted, duplicate-free, the size the generator produced), a
//! representative *sample* is converted end to end here, and the two properties that a
//! mechanical port is most likely to get wrong — mode visibility and the shadowing
//! order — are checked directly.

use techxt::convert::UnknownMacroResolution;
use techxt::def::DefinitionSet;
use techxt::{defs, Converter};

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// A converter that knows the generated table and **nothing else**.
///
/// The catch-all fallback is switched off (DECISIONS.md D10), which is what makes a
/// mode restriction observable: with the catch-all registered, a command no category
/// resolves still reaches the renderer as a zero-argument callable and finds its rule
/// in the name-keyed fallback table of PLAN.md §10.3 step 3, which is mode-blind.
fn generated_only() -> Converter {
    Converter::builder()
        .definitions(DefinitionSet::new().with(defs::symbols_extra::category()))
        .unknown_macro_resolution(UnknownMacroResolution::Reject)
        .build()
        .expect("the generated category builds")
}

// ------------------------------------------------------------------- the sample

#[test]
fn text_symbols_from_the_long_tail() {
    assert_eq!(text(r"\textreferencemark"), "※\n");
    assert_eq!(text(r"\textnumero"), "№\n");
    assert_eq!(text(r"\textinterrobang"), "‽\n");
    assert_eq!(text(r"\textperthousand"), "‰\n");
    assert_eq!(text(r"\textschwa"), "ə\n");
    // A soft hyphen: invisible, and escaped in the generated table for that reason.
    assert_eq!(text(r"a\-b"), "a\u{ad}b\n");
}

#[test]
fn math_symbols_from_the_long_tail() {
    assert_eq!(text(r"$\lessapprox$"), "⪅\n");
    assert_eq!(text(r"$\varkappa$"), "ϰ\n");
    assert_eq!(text(r"$\barwedge$"), "⌅\n");
    assert_eq!(text(r"$\circledast$"), "⊛\n");
    assert_eq!(text(r"$\gtrless$"), "≷\n");
    // A relation the math engine knows the class of, so the joiner spaces it.
    assert_eq!(text(r"$a \triangleq b$"), "𝑎 ≜ 𝑏\n");
}

#[test]
fn the_cyrillic_transliteration_names() {
    assert_eq!(text(r"\CYRZH\cyrzh"), "Жж\n");
    assert_eq!(text(r"\CYRSHCH\cyrshch"), "Щщ\n");
}

/// A replacement string is never run through the font alphabets (DECISIONS.md D6).
#[test]
fn a_replacement_is_not_restyled_by_a_surrounding_font() {
    assert_eq!(text(r"$\mathbf{\hbar}$"), "ℏ\n");
    assert_eq!(text(r"$\mathbf{\circledast}$"), "⊛\n");
}

/// PLAN.md §12.3: a v3 `%`-style replacement becomes a template, not a dropped
/// argument. Each of these renders one of its arguments and discards the rest.
#[test]
fn percent_style_replacements_became_templates() {
    assert_eq!(text(r"\colorbox{blue}{visible}"), "visible\n");
    assert_eq!(text(r"\fcolorbox{blue}{white}{visible}"), "visible\n");
    assert_eq!(text(r"\textcolor{blue}{visible}"), "visible\n");
}

/// An entry with no replacement still declares its arguments, which is the whole
/// reason to carry it: an undeclared macro would spill its braces into the paragraph.
#[test]
fn a_rule_less_entry_still_swallows_its_arguments() {
    assert_eq!(text(r"\selectlanguage{french}after"), "after\n");
    assert_eq!(text(r"\definecolor{shade}{rgb}{1,0,0}after"), "after\n");
    assert_eq!(text(r"\bibitem[mark]{key}entry"), "entry\n");
}

// ------------------------------------------------------------------ mode gating

#[test]
fn a_math_only_symbol_does_not_fire_in_text() {
    let converter = generated_only();
    let converted = converter.latex_to_text(r"$\lessapprox$").expect("parses");
    assert_eq!(converted.text, "⪅\n");
    assert!(converted.diagnostics.is_empty());

    // In text the definition is not there at all: no category claims the name, so
    // techy's own unresolvable-command handling applies and, under tolerant recovery,
    // the command survives as the characters it was written with.
    let converted = converter.latex_to_text(r"\lessapprox").expect("parses");
    assert_eq!(converted.text, "\\lessapprox\n");
    assert!(!converted.diagnostics.is_empty());
}

#[test]
fn a_text_only_symbol_does_not_fire_in_math() {
    let converter = generated_only();
    let converted = converter
        .latex_to_text(r"\textreferencemark")
        .expect("parses");
    assert_eq!(converted.text, "※\n");
    assert!(converted.diagnostics.is_empty());

    let converted = converter
        .latex_to_text(r"$\textreferencemark$")
        .expect("parses");
    assert!(!converted.text.contains('※'));
    assert!(!converted.diagnostics.is_empty());
}

/// An entry that takes arguments is deliberately left visible in both modes.
///
/// A mode-invisible macro does not vanish under the default settings — the catch-all
/// gives it a *zero-argument* spec (DECISIONS.md D16), and its arguments are then read
/// as ordinary groups and rendered. `$\textcolor{red}{x}$` would print "redx".
#[test]
fn an_entry_with_arguments_is_visible_in_both_modes() {
    assert_eq!(text(r"\textcolor{red}{x}"), "x\n");
    assert_eq!(text(r"$\textcolor{red}{x}$"), "𝑥\n");
}

// --------------------------------------------------------------- shadowing order

/// PLAN.md §12.1: `symbols_extra` is pushed first, so every curated category corrects
/// it. Both halves are checked — that the generated entry exists, and that the curated
/// one is what a standard converter uses.
#[test]
fn a_curated_entry_shadows_the_generated_one() {
    // `\quad` is v3's two spaces in the generated table and one EM SPACE in
    // `defs::base` (DECISIONS.md D14).
    assert_eq!(
        generated_only()
            .latex_to_text(r"a\quad b")
            .expect("parses")
            .text,
        "a b\n",
    );
    assert_eq!(text(r"a\quad b"), "a\u{2003}b\n");

    // `\sqrt` is in the parse-side inventory with no replacement at all, so the
    // generated entry renders nothing; `defs::mathcore`'s handler draws the root sign.
    assert_eq!(
        generated_only()
            .latex_to_text(r"$\sqrt{2}$")
            .expect("parses")
            .text,
        "",
    );
    assert_eq!(text(r"$\sqrt{2}$"), "√2\n");
}

/// The order is only worth anything if the generated table really is the one being
/// shadowed: an entry both categories define must come out the curated way, and an
/// entry only the generated table defines must still come out at all.
#[test]
fn shadowing_costs_the_long_tail_nothing() {
    assert_eq!(text(r"\ldots"), "…\n");
    assert_eq!(text(r"$\hbar$"), "ℏ\n");
    assert_eq!(text(r"$\varnothing$"), "∅\n");
}
