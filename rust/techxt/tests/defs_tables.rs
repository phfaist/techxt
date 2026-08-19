//! `defs::tables`: `tabular` and its relatives, column alignment, rules and spans
//! (PLAN.md §9.6).

use techxt::diag::UnsupportedIgnored;
use techxt::{Conversion, Converter};

fn convert(latex: &str) -> Conversion {
    Converter::standard().latex_to_text(latex).expect("parses")
}

fn text(latex: &str) -> String {
    convert(latex).text
}

// --------------------------------------------------------- PLAN.md §15 example 21

#[test]
fn example_21_columns_are_padded_to_their_widest_cell() {
    assert_eq!(
        text(r"\begin{tabular}{lr} a & 10 \\ bb & 3 \end{tabular}"),
        "a   10\nbb   3\n"
    );
}

// ----------------------------------------------------------------------- alignment

#[test]
fn each_alignment_letter_pads_its_column_its_own_way() {
    // Column widths are 5 (`left`, `wwwww`), 6 (`centre`, `wwwww`) and 5 (`right`,
    // `wwwww`); the short row shows where each alignment puts its padding.
    let table = r"\begin{tabular}{lcr} left & centre & right \\ x & y & z \end{tabular}";
    assert_eq!(text(table), "left  centre  right\nx       y         z\n");
}

#[test]
fn centring_is_left_biased_when_the_padding_is_odd() {
    // Width 4, content 1: one space before, two after.
    assert_eq!(
        text(r"\begin{tabular}{cl} x & a \\ wxyz & b \end{tabular}"),
        " x    a\nwxyz  b\n"
    );
}

#[test]
fn rules_and_inserted_material_take_no_column() {
    assert_eq!(
        text(r"\begin{tabular}{|l|@{--}r|} a & 1 \\ bb & 22 \end{tabular}"),
        "a    1\nbb  22\n"
    );
}

#[test]
fn a_paragraph_column_reads_as_left_aligned_and_its_width_is_not_a_column() {
    // `p{3cm}` would render as the text `p3cm`, whose `c` must not invent a column.
    assert_eq!(
        text(r"\begin{tabular}{p{3cm}r} a & 1 \\ bb & 22 \end{tabular}"),
        "a    1\nbb  22\n"
    );
}

#[test]
fn a_column_the_specification_does_not_describe_is_left_aligned() {
    assert_eq!(
        text(r"\begin{tabular}{r} 1 & aa \\ 22 & b \end{tabular}"),
        " 1  aa\n22  b\n"
    );
}

#[test]
fn an_empty_cell_still_holds_its_column_open() {
    assert_eq!(
        text(r"\begin{tabular}{ll} a & \\ & b \end{tabular}"),
        "a\n   b\n"
    );
}

// --------------------------------------------------------------------------- rules

#[test]
fn hline_draws_a_rule_across_the_whole_table() {
    assert_eq!(
        text(r"\begin{tabular}{lr} \hline a & 10 \\ \hline bb & 3 \\ \hline \end{tabular}"),
        "------\na   10\n------\nbb   3\n------\n"
    );
}

#[test]
fn cline_and_the_booktabs_rules_are_drawn_like_an_hline() {
    assert_eq!(
        text(r"\begin{tabular}{lr} \toprule a & 10 \\ \cline{1-2} bb & 3 \\ \bottomrule \end{tabular}"),
        "------\na   10\n------\nbb   3\n------\n"
    );
}

#[test]
fn a_rule_outside_a_table_is_reported_and_dropped() {
    let converted = convert(r"a \midrule b");
    assert_eq!(converted.text, "a b\n");
    let ignored: Vec<&UnsupportedIgnored> = converted.diagnostics.conditions().collect();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].construct, "\\midrule");
}

// ---------------------------------------------------------------------- the family

#[test]
fn the_width_arguments_of_the_other_environments_are_dropped() {
    assert_eq!(
        text(r"\begin{tabular*}{5cm}{lr} a & 10 \\ bb & 3 \end{tabular*}"),
        "a   10\nbb   3\n"
    );
    assert_eq!(
        text(r"\begin{tabularx}{\textwidth}[t]{lr} a & 10 \\ bb & 3 \end{tabularx}"),
        "a   10\nbb   3\n"
    );
}

// ---------------------------------------------------------------------- multicolumn

#[test]
fn a_multicolumn_renders_its_contents_and_reports_the_span_once() {
    let converted = convert(
        r"\begin{tabular}{ll} \multicolumn{2}{c}{Title} & x \\ \multicolumn{2}{c}{More} & y \end{tabular}",
    );
    assert_eq!(converted.text, "Title  x\nMore   y\n");
    // One diagnostic for the table, not one per span.
    let ignored: Vec<&UnsupportedIgnored> = converted.diagnostics.conditions().collect();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].construct, "\\multicolumn");
}

#[test]
fn a_table_without_spans_reports_nothing() {
    let converted = convert(r"\begin{tabular}{ll} a & b \end{tabular}");
    assert_eq!(
        converted
            .diagnostics
            .conditions::<UnsupportedIgnored>()
            .count(),
        0
    );
}

// -------------------------------------------------------------------- composition

#[test]
fn a_cell_is_flattened_to_one_line() {
    // Whatever a cell contains, it occupies one line of one column.
    assert_eq!(
        text("\\begin{tabular}{ll} a\n\nstill a & b \\\\ c & d \\end{tabular}"),
        "a still a  b\nc          d\n"
    );
}

#[test]
fn a_table_starts_on_a_line_of_its_own() {
    assert_eq!(
        text(r"before \begin{tabular}{l} a \end{tabular} after"),
        "before\na\nafter\n"
    );
}

#[test]
fn an_empty_table_renders_as_nothing() {
    // Nothing at all, not even a block boundary: the words on either side meet exactly
    // as they would across an empty group.
    assert_eq!(text(r"a\begin{tabular}{l}\end{tabular}b"), "ab\n");
}

#[test]
fn a_trailing_row_separator_does_not_add_an_empty_row() {
    assert_eq!(
        text(r"\begin{tabular}{l} a \\ b \\ \end{tabular}"),
        "a\nb\n"
    );
}
