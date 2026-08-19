//! The document-furniture categories: `refs`, `links`, `graphics`, `titling`,
//! `preamble` and `inputs` (PLAN.md §9.8), plus the stubs of PLAN.md §12.1.

use techxt::def::DefinitionSet;
use techxt::diag::InputNotResolved;
use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// ---------------------------------------------------------------------- refs

#[test]
fn references_render_as_markers() {
    assert_eq!(text(r"see \ref{sec:one}"), "see <ref>\n");
    assert_eq!(
        text(r"\autoref{x} \cref{x} \vref{x} \pageref{x}"),
        "<ref> <ref> <ref> <ref>\n"
    );
    assert_eq!(text(r"\Cref{x} says"), "<Ref> says\n");
    assert_eq!(text(r"in \eqref{eq:one}"), "in (<ref>)\n");
}

#[test]
fn citations_render_as_markers() {
    assert_eq!(text(r"\cite{a}"), "<cit.>\n");
    assert_eq!(text(r"\cite[p.~3]{a}"), "<cit.>\n");
    assert_eq!(text(r"\citet{a} and \citep{b}"), "<cit.> and <cit.>\n");
}

#[test]
fn a_label_renders_as_nothing_and_never_leaks_its_key() {
    assert_eq!(text(r"a\label{sec:one}b"), "ab\n");
    assert_eq!(text(r"a\nocite{key}b"), "ab\n");
}

// --------------------------------------------------------------------- links

#[test]
fn a_url_argument_is_verbatim() {
    // The characters that would otherwise start a macro, a subscript or a comment all
    // survive.
    assert_eq!(
        text(r"\url{https://ex.org/a_b#c%20d}"),
        "<https://ex.org/a_b#c%20d>\n"
    );
    assert_eq!(text(r"\nolinkurl{a_b}"), "<a_b>\n");
}

#[test]
fn a_link_keeps_its_url_whole_under_a_narrow_wrap() {
    let converter = Converter::builder()
        .wrap_width(Some(10))
        .build()
        .expect("builds");
    let converted = converter
        .latex_to_text(r"see \href{https://ex.org/a_b}{link}")
        .expect("parses")
        .text;
    // The URL is one unbreakable run, so it overflows rather than splitting.
    assert!(converted.contains("<https://ex.org/a_b>"), "{converted:?}");
}

// ------------------------------------------------------------------ graphics

#[test]
fn a_graphic_is_an_indented_placeholder_block() {
    assert_eq!(
        text(r"before \includegraphics[width=2cm]{fig.png} after"),
        "before\n\n    < g r a p h i c s >\n\nafter\n"
    );
    // The optional argument is optional.
    assert_eq!(
        text(r"\includegraphics{fig.png}"),
        "    < g r a p h i c s >\n"
    );
}

// ------------------------------------------------------------------- titling

#[test]
fn a_title_block_is_laid_out_under_a_rule_as_wide_as_the_widest_line() {
    assert_eq!(
        text(r"\title{On Motion}\author{A. Einstein}\date{1905}\maketitle"),
        "On Motion\n    A. Einstein\n    1905\n===============\n"
    );
}

#[test]
fn the_fields_may_be_declared_anywhere_and_print_nothing_where_they_are() {
    assert_eq!(text(r"\title{T}"), "");
    // Recorded in the run state, used by a sibling later on.
    assert_eq!(
        text(r"\title{T}Body.\author{A}\date{D}\maketitle"),
        "Body.\n\nT\n    A\n    D\n=====\n"
    );
}

#[test]
fn missing_fields_say_so_in_the_output() {
    assert_eq!(
        text(r"\maketitle"),
        "<no title>\n    <no author>\n    <today>\n===============\n"
    );
    assert_eq!(
        text(r"\title{T}\maketitle"),
        "T\n    <no author>\n    <today>\n===============\n"
    );
}

#[test]
fn today_comes_from_the_options_or_says_it_does_not_know() {
    assert_eq!(text(r"\today"), "<today>\n");
    let converter = Converter::builder()
        .today(Some("1 April 1905".into()))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\today").expect("parses").text,
        "1 April 1905\n"
    );
    // …and `\maketitle` falls back to the same resolution for a missing date.
    assert_eq!(
        converter
            .latex_to_text(r"\title{T}\author{A}\maketitle")
            .expect("parses")
            .text,
        "T\n    A\n    1 April 1905\n================\n"
    );
}

#[test]
fn the_title_block_measures_display_columns() {
    // The rule is as wide as the widest line, counted in columns rather than bytes.
    assert_eq!(
        text(r"\title{漢字}\author{A}\date{D}\maketitle"),
        "漢字\n    A\n    D\n=====\n"
    );
}

// ------------------------------------------------------------------ preamble

#[test]
fn preamble_declarations_consume_their_arguments_and_say_nothing() {
    for latex in [
        r"\documentclass[12pt,a4paper]{article}",
        r"\usepackage[utf8]{inputenc}",
        r"\RequirePackage{amsmath}",
        r"\newcommand*{\foo}[2][d]{body}",
        r"\renewcommand{\foo}{body}",
        r"\providecommand{\foo}{body}",
        r"\def\foo{body}",
        r"\newenvironment{env}[1][d]{begin}{end}",
        r"\renewenvironment{env}{begin}{end}",
        r"\newtheorem{thm}[counter]{Theorem}[section]",
        r"\setlength{\parindent}{0pt}",
        r"\addtolength{\parskip}{1ex}",
        r"\setcounter{page}{1}",
        r"\addtocounter{page}{1}",
        r"\pagestyle{empty}",
        r"\thispagestyle{plain}",
        r"\hypersetup{colorlinks=true}",
        r"\graphicspath{{figures/}}",
        r"\bibliographystyle{plain}",
        r"\bibliography{refs}",
    ] {
        let conversion = Converter::standard().latex_to_text(latex).expect("parses");
        assert_eq!(conversion.text, "", "{latex:?} left text behind");
        // Known constructs: rendering them as nothing is the complete answer, so
        // nothing is reported.
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

#[test]
fn a_preamble_declaration_does_not_swallow_what_follows_it() {
    assert_eq!(
        text("\\documentclass{article}\n\\usepackage{amsmath}\n\nBody text."),
        "Body text.\n"
    );
}

// -------------------------------------------------------------------- inputs

#[test]
fn an_unresolved_include_is_a_note_and_not_an_error() {
    // DECISIONS.md C4: no attached slot means "not resolved", which is an ordinary
    // configuration and not a failure of anything.
    let conversion = Converter::standard()
        .latex_to_text(r"a\input{chapters/intro.tex}b")
        .expect("parses");
    assert_eq!(conversion.text, "ab\n");
    assert!(!conversion.diagnostics.has_errors());

    let notes: Vec<&InputNotResolved> = conversion.diagnostics.conditions().collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].target, "chapters/intro.tex");

    let included = Converter::standard()
        .latex_to_text(r"\include{part2}")
        .expect("parses");
    assert_eq!(
        included
            .diagnostics
            .conditions::<InputNotResolved>()
            .count(),
        1
    );
}

// --------------------------------------------------------------------- stubs

#[test]
fn every_stub_category_is_present_and_empty() {
    // PLAN.md §12.1 fixes the category list; the ones later milestones fill are still
    // part of `standard()` so that the order cannot drift while they are written.
    let empty = |category: techxt::def::Category| {
        let converter = Converter::builder()
            .definitions(DefinitionSet::new().with(category))
            .build()
            .expect("an empty category builds");
        // Nothing is defined, so every command is unknown and renders as nothing.
        assert_eq!(
            converter
                .latex_to_text(r"a \anything b")
                .expect("parses")
                .text,
            "a b\n"
        );
    };
    empty(techxt::defs::mathcore::category());
    empty(techxt::defs::mathenvs::category());
    empty(techxt::defs::subsuperscripts::category());
    empty(techxt::defs::lists::category());
    empty(techxt::defs::verbatim::category());
    empty(techxt::defs::tables::category());
    empty(techxt::defs::theorems::category());
    empty(techxt::defs::symbols_extra::category());
    empty(techxt::defs::natbib::category());
}
