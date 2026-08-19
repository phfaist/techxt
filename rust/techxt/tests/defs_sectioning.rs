//! `defs::sectioning`: the seven heading commands, the counter rules, and the four
//! heading styles (PLAN.md §9.2).

use techxt::convert::HeadingStyle;
use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

fn styled(style: HeadingStyle, latex: &str) -> String {
    Converter::builder()
        .heading_style(style)
        .build()
        .expect("builds")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// ------------------------------------------------------------------ numbering

#[test]
fn incrementing_a_level_zeroes_every_deeper_counter() {
    // The second section restarts its subsections rather than continuing them.
    assert_eq!(
        text(r"\section{A}\subsection{A1}\subsection{A2}\section{B}\subsection{B1}"),
        "1 A\n---\n\n1.1 A1\n~~~~~~\n\n1.2 A2\n~~~~~~\n\n2 B\n---\n\n2.1 B1\n~~~~~~\n"
    );
}

#[test]
fn a_subsubsection_carries_the_whole_dotted_number_and_no_rule() {
    assert_eq!(
        text(r"\section{A}\subsection{B}\subsubsection{C}"),
        "1 A\n---\n\n1.1 B\n~~~~~\n\n1.1.1 C\n"
    );
}

#[test]
fn without_a_chapter_the_dotted_number_is_rooted_at_the_section() {
    assert_eq!(text(r"\section{S}"), "1 S\n---\n");
    assert_eq!(
        text(r"\section{S}\subsection{U}"),
        "1 S\n---\n\n1.1 U\n~~~~~\n"
    );
}

#[test]
fn a_chapter_roots_the_dotted_number_one_level_higher() {
    assert_eq!(
        text(r"\chapter{C}\section{S}\subsection{U}"),
        "1 C\n===\n\n1.1 S\n-----\n\n1.1.1 U\n~~~~~~~\n"
    );
}

#[test]
fn a_chapter_appearing_later_changes_what_a_following_section_number_means() {
    // Before the chapter the run is article-shaped and `\section` is `1`; after it the
    // run is book-shaped and the same command is `1.1`. This is the `first` rule of
    // PLAN.md §9.2, and it is a property of the run, not of a document class.
    let converted = text(r"\section{Early}\chapter{Ch}\section{Late}");
    assert!(converted.contains("1 Early"), "{converted:?}");
    assert!(converted.contains("1.1 Late"), "{converted:?}");
}

#[test]
fn a_starred_heading_neither_increments_nor_displays() {
    // The starred section between the two numbered ones did not consume a number.
    assert_eq!(
        text(r"\section{A}\section*{Skipped}\section{B}"),
        "1 A\n---\n\nSkipped\n-------\n\n2 B\n---\n"
    );
}

#[test]
fn a_starred_heading_does_not_zero_deeper_counters_either() {
    // Not incrementing means not renumbering: the subsection after the starred section
    // continues where it was.
    assert_eq!(
        text(r"\section{A}\subsection{One}\section*{Aside}\subsection{Two}"),
        "1 A\n---\n\n1.1 One\n~~~~~~~\n\nAside\n-----\n\n1.2 Two\n~~~~~~~\n"
    );
}

#[test]
fn a_part_counts_in_roman_and_resets_nothing() {
    assert_eq!(
        text(r"\section{A}\part{One}\section{B}"),
        "1 A\n---\n\nPart I: One\n===========\n\n2 B\n---\n"
    );
    assert_eq!(
        text(r"\part{A}\part{B}"),
        "Part I: A\n=========\n\nPart II: B\n==========\n"
    );
}

#[test]
fn the_two_deepest_levels_are_never_numbered() {
    assert_eq!(
        text(r"\section{A}\paragraph{P}\subparagraph{Q}"),
        "1 A\n---\n\nP\n\nQ\n"
    );
}

#[test]
fn the_toc_title_is_accepted_and_ignored() {
    assert_eq!(
        text(r"\section[Short]{The Long Title}"),
        "1 The Long Title\n----------------\n"
    );
}

// --------------------------------------------------------------------- styles

#[test]
fn the_underlined_style_drops_the_number_and_keeps_the_rule() {
    assert_eq!(
        styled(HeadingStyle::Underlined, r"\section{A}\subsection{B}"),
        "A\n-\n\nB\n~\n"
    );
    assert_eq!(styled(HeadingStyle::Underlined, r"\chapter{C}"), "C\n=\n");
}

#[test]
fn the_prefix_style_writes_a_label_and_no_rule() {
    assert_eq!(styled(HeadingStyle::Prefix, r"\part{A}"), "Part: A\n");
    assert_eq!(styled(HeadingStyle::Prefix, r"\chapter{A}"), "Chapter: A\n");
    assert_eq!(styled(HeadingStyle::Prefix, r"\section{A}"), "§ A\n");
    assert_eq!(styled(HeadingStyle::Prefix, r"\subsection{A}"), "§.§ A\n");
    assert_eq!(
        styled(HeadingStyle::Prefix, r"\subsubsection{A}"),
        "§.§.§ A\n"
    );
    assert_eq!(styled(HeadingStyle::Prefix, r"\paragraph{A}"), "A\n");
}

#[test]
fn the_plain_style_is_the_title_and_nothing_else() {
    assert_eq!(
        styled(HeadingStyle::Plain, r"\section{A}\subsection{B}"),
        "A\n\nB\n"
    );
}

// ---------------------------------------------------------------- the rule

#[test]
fn the_rule_is_as_wide_as_the_heading_in_display_columns() {
    // Not bytes and not characters: an accented letter is one column even though it is
    // two bytes, and a wide character is two columns.
    assert_eq!(text(r"\section*{été}"), "été\n---\n");
    assert_eq!(text(r"\section*{漢字}"), "漢字\n----\n");
    // A styled heading measures the styled letters, which are one column each.
    assert_eq!(text(r"\section*{\textbf{ab}}"), "𝐚𝐛\n--\n");
}

#[test]
fn a_heading_is_separated_from_its_surroundings_by_one_blank_line() {
    assert_eq!(
        text("Before.\n\n\\section{S}\n\nAfter."),
        "Before.\n\n1 S\n---\n\nAfter.\n"
    );
}

#[test]
fn a_multi_word_heading_stays_on_one_line_under_a_narrow_wrap() {
    // The rule was measured from the heading line, so the heading must not wrap away
    // from it.
    let converter = Converter::builder()
        .wrap_width(Some(6))
        .build()
        .expect("builds");
    let converted = converter
        .latex_to_text(r"\section*{a b c}")
        .expect("parses")
        .text;
    assert_eq!(converted, "a b c\n-----\n");
}

#[test]
fn nothing_is_ever_uppercased() {
    assert_eq!(
        text(r"\section*{lower case title}"),
        "lower case title\n----------------\n"
    );
}
