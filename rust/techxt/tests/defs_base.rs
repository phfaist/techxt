//! `defs::base`: escapes, spacing, ligatures, breaks and the miscellaneous text macros
//! (PLAN.md §9.3, §9.7, §9.8).

use techxt::def::{Category, DefinitionSet, MacroDef, TextRule};
use techxt::diag::{MisplacedAlignment, UnsupportedIgnored};
use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

fn conversion(latex: &str) -> techxt::Conversion {
    Converter::standard().latex_to_text(latex).expect("parses")
}

// ------------------------------------------------------------------- escapes

#[test]
fn the_seven_reserved_characters_escape_to_themselves() {
    assert_eq!(text(r"\{ \} \$ \& \# \_ \%"), "{ } $ & # _ %\n");
    // `\%` in particular must not start a comment.
    assert_eq!(text(r"50\% off"), "50% off\n");
}

// ------------------------------------------------------------------- spacing

#[test]
fn thin_spaces_are_one_collapsible_space_each() {
    assert_eq!(text(r"a\,b"), "a b\n");
    assert_eq!(text(r"a\;b"), "a b\n");
    assert_eq!(text(r"a\:b"), "a b\n");
    // The control space, and an escaped newline.
    assert_eq!(text("a\\ b"), "a b\n");
    assert_eq!(text("a\\\nb"), "a b\n");
    // Adjacent spacing macros still collapse to one space, because glue collapses.
    assert_eq!(text(r"a\,\;\:b"), "a b\n");
}

#[test]
fn a_negative_thin_space_is_nothing_at_all() {
    assert_eq!(text(r"a\!b"), "ab\n");
}

#[test]
fn quads_are_em_spaces_not_ascii_runs() {
    // DECISIONS.md D14: a text item holds no whitespace, and a quad *is* one em.
    assert_eq!(text(r"a\quad b"), "a\u{2003}b\n");
    assert_eq!(text(r"a\qquad b"), "a\u{2003}\u{2003}b\n");
    // Unbreakable, so a narrow wrap width cannot split the gap.
    let converter = Converter::builder()
        .wrap_width(Some(4))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"a\quad b").expect("parses").text,
        "a\u{2003}b\n"
    );
}

#[test]
fn a_quad_at_the_end_of_a_line_leaves_no_ascii_trailing_whitespace() {
    // The reason D14 chose U+2003 over a run of ASCII spaces: a text item is content,
    // never trimmed, so an ASCII-space item here would break the layout engine's own
    // invariant that no line ends in collapsible whitespace.
    for latex in [r"word\quad", r"word\qquad", r"word\quad\par next"] {
        let converted = text(latex);
        for line in converted.lines() {
            assert!(
                !line.ends_with(' ') && !line.ends_with('\t'),
                "{latex:?} produced a line ending in ASCII whitespace: {line:?}"
            );
        }
    }
}

#[test]
fn a_tie_is_a_space_no_line_break_may_fall_in() {
    assert_eq!(text(r"Fig.~1"), "Fig.\u{a0}1\n");
    let converter = Converter::builder()
        .wrap_width(Some(6))
        .build()
        .expect("builds");
    // "Fig. 1" is one word of width 6, so it survives whole on its own line.
    assert_eq!(
        converter
            .latex_to_text(r"xxxxxx Fig.~1")
            .expect("parses")
            .text,
        "xxxxxx\nFig.\u{a0}1\n"
    );
}

// -------------------------------------------------------------------- breaks

#[test]
fn paragraph_and_line_breaks() {
    assert_eq!(text(r"a\par b"), "a\n\nb\n");
    assert_eq!(text(r"a\newline b"), "a\nb\n");
    assert_eq!(text(r"a\\b"), "a\nb\n");
    // Repeated requests merge rather than accumulate.
    assert_eq!(text("a\\par\n\n\\par b"), "a\n\nb\n");
}

#[test]
fn a_line_break_takes_its_optional_length_and_ignores_it() {
    // DECISIONS.md D8: the length is parsed, so it never reaches the reader.
    assert_eq!(text(r"a\\[2ex]b"), "a\nb\n");
    assert_eq!(text(r"a\\*[2ex]b"), "a\nb\n");
}

#[test]
fn a_line_breaks_optional_argument_must_be_adjacent() {
    // The whole point of the adjacency requirement: a bracketed expression that merely
    // *follows* `\\` is content, not a length.
    assert_eq!(text(r"A=0 \\ [C,D]=0"), "A=0\n[C,D]=0\n");
    // A comment between the two is not adjacency either.
    assert_eq!(text("A \\\\% here\n[x]"), "A\n[x]\n");
}

#[test]
fn skips_are_paragraph_breaks_and_layout_switches_are_nothing() {
    assert_eq!(text(r"a\smallskip b"), "a\n\nb\n");
    assert_eq!(text(r"a\medskip b"), "a\n\nb\n");
    assert_eq!(text(r"a\bigskip b"), "a\n\nb\n");
    assert_eq!(text(r"a\vspace{2cm}b"), "a\n\nb\n");
    assert_eq!(text(r"a\hspace{2cm}b"), "ab\n");
    assert_eq!(text(r"\noindent\centering\raggedright\raggedleft x"), "x\n");
}

// ----------------------------------------------------------------- ligatures

#[test]
fn every_ligature() {
    assert_eq!(text("`a'"), "‘a’\n");
    assert_eq!(text("``a''"), "“a”\n");
    assert_eq!(text("a--b"), "a–b\n");
    assert_eq!(text("a---b"), "a—b\n");
    assert_eq!(text("!`a?`"), "¡a¿\n");
    // The longest trigger wins, without any ordering rule in the definitions.
    assert_eq!(text("---"), "—\n");
    assert_eq!(text("''"), "”\n");
}

// -------------------------------------------------------- context-dependent

#[test]
fn an_alignment_character_outside_a_table_is_a_reported_space() {
    let conversion = conversion("a & b");
    assert_eq!(conversion.text, "a b\n");
    let misplaced: Vec<&MisplacedAlignment> = conversion.diagnostics.conditions().collect();
    assert_eq!(misplaced.len(), 1);
}

#[test]
fn a_rule_outside_a_table_is_dropped_and_reported() {
    // The macro's post-space is invocation syntax, so `a` and `b` meet.
    let conversion = conversion(r"a\hline b");
    assert_eq!(conversion.text, "ab\n");
    let ignored: Vec<&UnsupportedIgnored> = conversion.diagnostics.conditions().collect();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].construct, "\\hline");
}

// ---------------------------------------------------------------------- misc

#[test]
fn texorpdfstring_keeps_the_tex_form() {
    // The TeX form is rendered as TeX: the formula in it really is a formula.
    assert_eq!(text(r"\texorpdfstring{$x$}{x}"), "\u{1d465}\n");
    assert_eq!(text(r"\texorpdfstring{a}{b}"), "a\n");
}

#[test]
fn phantoms_render_nothing_and_do_not_leak_their_argument() {
    assert_eq!(text(r"a\phantom{leak}b"), "ab\n");
    assert_eq!(text(r"a\hphantom{leak}b"), "ab\n");
    assert_eq!(text(r"a\vphantom{leak}b"), "ab\n");
}

#[test]
fn decorations_render_their_argument_unchanged() {
    assert_eq!(text(r"\overline{abc}"), "abc\n");
    assert_eq!(text(r"\underbrace{x + y}"), "x + y\n");
    assert_eq!(text(r"\widehat{n}"), "n\n");
}

#[test]
fn text_and_mbox_change_the_mode_and_not_the_font() {
    assert_eq!(text(r"\text{a b}"), "a b\n");
    assert_eq!(text(r"\mbox{a b}"), "a b\n");
    // The enclosing text style survives the excursion: `\textbf` set it, `\text` did
    // not disturb it.
    assert_eq!(text(r"\textbf{a\text{b}}"), "𝐚𝐛\n");
}

#[test]
fn ensuremath_renders_its_argument_as_math() {
    // Source whitespace is ignored in math, and the math font applies.
    assert_eq!(text(r"\ensuremath{a b}"), "𝑎𝑏\n");
}

// -------------------------------------------------------------- text symbols

#[test]
fn the_curated_text_symbols() {
    assert_eq!(text(r"\l\L\o\O\ss\aa\AA\ae\AE\oe\OE"), "łŁøØßåÅæÆœŒ\n");
    assert_eq!(text(r"\i\j"), "ıȷ\n");
    assert_eq!(text(r"\S\P\dag\ddag\copyright\pounds"), "§¶†‡©£\n");
    assert_eq!(text(r"\ldots\dots"), "……\n");
    assert_eq!(
        text(r"\textbackslash\textasciitilde\textasciicircum\textbar"),
        "\\~^|\n"
    );
    assert_eq!(text(r"\TeX\ and \LaTeX"), "TeX and LaTeX\n");
}

// ------------------------------------------------------------ category alone

#[test]
fn the_category_stands_on_its_own() {
    // Every module is usable without the rest of the library (PLAN.md §12).
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(techxt::defs::base::category()))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"a---b \& c").expect("parses").text,
        "a—b & c\n"
    );
}

#[test]
fn a_later_category_shadows_base() {
    // The shadowing rule of PLAN.md §10.2, exercised against the shipped library.
    let definitions = techxt::defs::standard().with(
        Category::new("mine")
            .with_macro(MacroDef::new("ldots").rule(TextRule::Literal("...".into()))),
    );
    let converter = Converter::builder()
        .definitions(definitions)
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\ldots").expect("parses").text,
        "...\n"
    );
}
