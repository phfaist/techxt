//! `defs::lists`: the five list environments, `\item`, and the markers they generate
//! (PLAN.md §9.4).

use techxt::convert::{CounterStyle, CounterWrap, EnumFormat, ListStyle};
use techxt::diag::{StrayItem, UnsupportedIgnored};
use techxt::{Conversion, Converter};

fn convert(latex: &str) -> Conversion {
    Converter::standard().latex_to_text(latex).expect("parses")
}

fn text(latex: &str) -> String {
    convert(latex).text
}

// --------------------------------------------------------- PLAN.md §15 example 22

#[test]
fn example_22_nested_lists() {
    // The inner `enumerate` is a different kind, so it starts at first-level numbering
    // (PLAN.md §15's note), and its markers sit under the outer item's text rather than
    // closing it.
    assert_eq!(
        text(
            r"\begin{itemize}\item one \begin{enumerate}\item x\item y\end{enumerate}\item two\end{itemize}"
        ),
        "  • one\n    1. x\n    2. y\n  • two\n"
    );
}

// ------------------------------------------------------------------------- markers

#[test]
fn an_itemize_bullets_its_items_and_indents_the_list() {
    assert_eq!(
        text(r"\begin{itemize}\item one\item two\end{itemize}"),
        "  • one\n  • two\n"
    );
}

#[test]
fn an_enumerate_numbers_its_items() {
    assert_eq!(
        text(r"\begin{enumerate}\item one\item two\item three\end{enumerate}"),
        "  1. one\n  2. two\n  3. three\n"
    );
}

#[test]
fn a_description_takes_its_marker_from_the_items_label() {
    assert_eq!(
        text(r"\begin{description}\item[Term] what it means\item[Other] and this\end{description}"),
        "  Term  what it means\n  Other  and this\n"
    );
}

#[test]
fn a_description_item_without_a_label_has_no_marker() {
    assert_eq!(
        text(r"\begin{description}\item nothing to define\end{description}"),
        "    nothing to define\n"
    );
}

#[test]
fn itemize_markers_cycle_through_the_configured_array() {
    // Five levels of the same kind: the four default bullets, then the first again.
    let latex = r"\begin{itemize}\item a\begin{itemize}\item b\begin{itemize}\item c
        \begin{itemize}\item d\begin{itemize}\item e
        \end{itemize}\end{itemize}\end{itemize}\end{itemize}\end{itemize}";
    assert_eq!(
        text(latex),
        "  • a\n    – b\n      * c\n        · d\n          • e\n"
    );
}

#[test]
fn enumerate_formats_cycle_through_the_configured_array() {
    let latex = r"\begin{enumerate}\item a\begin{enumerate}\item b\begin{enumerate}\item c
        \begin{enumerate}\item d\begin{enumerate}\item e
        \end{enumerate}\end{enumerate}\end{enumerate}\end{enumerate}\end{enumerate}";
    assert_eq!(
        text(latex),
        "  1. a\n     (a) b\n         i. c\n            A. d\n               1. e\n"
    );
}

#[test]
fn a_list_of_another_kind_starts_its_own_marker_sequence() {
    // The `enumerate` has no enclosing enumerate, so it numbers from `1.` rather than
    // carrying on from the bullets — PLAN.md §15 example 22's semantics.
    assert_eq!(
        text(r"\begin{itemize}\item a\begin{enumerate}\item b\end{enumerate}\end{itemize}"),
        "  • a\n    1. b\n"
    );
}

#[test]
fn a_list_of_another_kind_in_between_does_not_reset_the_marker_depth() {
    // PLAN.md §9.4 counts *all* the enclosing lists of the same kind, wherever they
    // sit, exactly as LaTeX's own itemize and enumerate depth counters do. The
    // innermost itemize here has one enclosing itemize, so it is a second-level itemize
    // and takes the second bullet: the `enumerate` in between resets nothing.
    assert_eq!(
        text(
            r"\begin{itemize}\item a\begin{enumerate}\item b\begin{itemize}\item c\end{itemize}\end{enumerate}\end{itemize}"
        ),
        "  • a\n    1. b\n       – c\n"
    );
}

#[test]
fn the_same_holds_for_an_enumerate_two_kinds_deep() {
    // A second-level enumerate, which the default formats number `(a)`.
    assert_eq!(
        text(
            r"\begin{enumerate}\item a\begin{itemize}\item b\begin{enumerate}\item c\end{enumerate}\end{itemize}\end{enumerate}"
        ),
        "  1. a\n     • b\n       (a) c\n"
    );
}

#[test]
fn the_marker_style_is_configurable() {
    // `ListStyle` is `#[non_exhaustive]`, so a downstream crate — which an integration
    // test is — assigns onto the default rather than writing a struct literal.
    let mut style = ListStyle::default();
    style.itemize_markers = alloc_vec(["-"]);
    style.enumerate_formats = vec![EnumFormat {
        style: CounterStyle::UpperRoman,
        wrap: CounterWrap::Parens,
    }];
    let converter = Converter::builder()
        .list_style(style)
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(
                r"\begin{itemize}\item a\end{itemize}\begin{enumerate}\item b\end{enumerate}"
            )
            .expect("parses")
            .text,
        "  - a\n  (I) b\n"
    );
}

fn alloc_vec<const N: usize>(values: [&str; N]) -> Vec<Box<str>> {
    values.into_iter().map(Box::from).collect()
}

// -------------------------------------------------------------- explicit labels

#[test]
fn an_explicit_label_does_not_advance_the_counter() {
    // As in LaTeX: the labelled item is not numbered, and the item after it takes the
    // number the labelled one would have had.
    assert_eq!(
        text(r"\begin{enumerate}\item a\item[note] b\item c\end{enumerate}"),
        "  1. a\n  note  b\n  2. c\n"
    );
}

#[test]
fn an_explicit_label_replaces_the_bullet_of_an_itemize_too() {
    assert_eq!(
        text(r"\begin{itemize}\item[!] urgent\item ordinary\end{itemize}"),
        "  !  urgent\n  • ordinary\n"
    );
}

#[test]
fn an_empty_label_is_no_label_at_all() {
    assert_eq!(
        text(r"\begin{enumerate}\item[] a\item b\end{enumerate}"),
        "  1. a\n  2. b\n"
    );
}

// ----------------------------------------------------------------- wrapping

#[test]
fn an_items_continuation_lines_up_with_its_text() {
    let converter = Converter::builder()
        .wrap_width(Some(20))
        .build()
        .expect("builds");
    let converted = converter
        .latex_to_text(r"\begin{itemize}\item one two three four five six\end{itemize}")
        .expect("parses")
        .text;
    // The hanging indent is the marker's width, so the second line starts under "one".
    assert_eq!(converted, "  • one two three\n    four five six\n");
}

// ------------------------------------------------------------------ diagnostics

#[test]
fn a_stray_item_is_reported_and_still_rendered() {
    let converted = convert(r"\item loose");
    assert_eq!(converted.text, "- loose\n");
    assert_eq!(converted.diagnostics.conditions::<StrayItem>().count(), 1);
}

#[test]
fn an_item_inside_a_list_is_not_stray() {
    let converted = convert(r"\begin{itemize}\item fine\end{itemize}");
    assert_eq!(converted.diagnostics.conditions::<StrayItem>().count(), 0);
}

#[test]
fn the_enumitem_option_list_is_accepted_and_reported_as_ignored() {
    let converted = convert(r"\begin{enumerate}[label=\alph*)]\item a\end{enumerate}");
    // The options do not leak into the text …
    assert_eq!(converted.text, "  1. a\n");
    // … and the reader of the diagnostics is told they were dropped.
    let ignored: Vec<&UnsupportedIgnored> = converted.diagnostics.conditions().collect();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].construct, "enumerate");
}

#[test]
fn a_list_without_options_reports_nothing() {
    let converted = convert(r"\begin{itemize}\item a\end{itemize}");
    assert_eq!(
        converted
            .diagnostics
            .conditions::<UnsupportedIgnored>()
            .count(),
        0
    );
}

// ------------------------------------------------------------- the other spellings

#[test]
fn trivlist_and_list_behave_like_an_itemize() {
    assert_eq!(text(r"\begin{trivlist}\item a\end{trivlist}"), "  • a\n");
    // `list`'s two mandatory arguments are consumed, not rendered.
    assert_eq!(
        text(r"\begin{list}{$\bullet$}{\setlength{\itemsep}{0pt}}\item a\end{list}"),
        "  • a\n"
    );
}

// ------------------------------------------------------------------ composition

#[test]
fn a_list_stands_apart_from_the_text_around_it() {
    assert_eq!(
        text("before\n\n\\begin{itemize}\\item a\\end{itemize}\n\nafter"),
        "before\n\n  • a\n\nafter\n"
    );
}

#[test]
fn text_after_a_list_is_no_longer_indented() {
    // The list closes every block it opened, including the last item's.
    assert_eq!(
        text(r"\begin{itemize}\item a\end{itemize}after"),
        "  • a\nafter\n"
    );
}

#[test]
fn an_empty_list_renders_as_nothing() {
    assert_eq!(text(r"a\begin{itemize}\end{itemize}b"), "a\nb\n");
}

#[test]
fn a_nested_list_does_not_disturb_the_counter_of_the_one_it_is_in() {
    // One counter per open list: the inner `enumerate` counts in its own, and the outer
    // one carries on where it left off.
    assert_eq!(
        text(
            r"\begin{enumerate}\item a\begin{enumerate}\item x\item y\end{enumerate}\item b\end{enumerate}"
        ),
        "  1. a\n     (a) x\n     (b) y\n  2. b\n"
    );
}
