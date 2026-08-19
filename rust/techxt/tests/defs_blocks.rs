//! Footnotes, theorems, quotations, floats and captions (PLAN.md §9.8).

use techxt::convert::FootnoteStyle;
use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

fn with_footnotes(style: FootnoteStyle, latex: &str) -> String {
    Converter::builder()
        .footnote_style(style)
        .build()
        .expect("builds")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// ------------------------------------------------------------------- footnotes

#[test]
fn example_18_a_footnote_marks_its_place_and_collects_its_text() {
    assert_eq!(
        text(r"Fact\footnote{Proof sketch.} holds."),
        "Fact[1] holds.\n\n---\n[1] Proof sketch.\n"
    );
}

#[test]
fn footnotes_are_numbered_in_document_order() {
    assert_eq!(
        text(r"a\footnote{first} b\footnote{second} c\footnote{third}"),
        "a[1] b[2] c[3]\n\n---\n[1] first\n[2] second\n[3] third\n"
    );
}

#[test]
fn a_footnote_inside_a_heading_is_numbered_where_it_stands() {
    // The fold visits nodes in document order, arguments included.
    assert_eq!(
        text(r"a\footnote{one}\section{T\footnote{two}}b\footnote{three}"),
        "a[1]\n\n1 T[2]\n------\n\nb[3]\n\n---\n[1] one\n[2] two\n[3] three\n"
    );
}

#[test]
fn the_marker_glues_to_the_word_it_annotates() {
    // No glue before it, whatever precedes it — that is what makes it a *mark*.
    assert_eq!(
        text(r"\emph{word}\footnote{n} rest"),
        "𝑤𝑜𝑟𝑑[1] rest\n\n---\n[1] n\n"
    );
}

#[test]
fn the_inline_style_puts_the_note_where_it_was_written() {
    assert_eq!(
        with_footnotes(
            FootnoteStyle::Inline,
            r"Fact\footnote{Proof sketch.} holds."
        ),
        "Fact[Proof sketch.] holds.\n"
    );
}

#[test]
fn the_skip_style_emits_neither_mark_nor_text() {
    assert_eq!(
        with_footnotes(FootnoteStyle::Skip, r"Fact\footnote{Proof sketch.} holds."),
        "Fact holds.\n"
    );
}

#[test]
fn an_explicit_mark_is_parsed_and_the_numbering_is_still_the_documents() {
    assert_eq!(text(r"a\footnote[7]{note}"), "a[1]\n\n---\n[1] note\n");
}

#[test]
fn a_footnote_in_a_list_item_still_collects_at_the_end() {
    assert_eq!(
        text(r"\begin{itemize}\item a\footnote{n}\end{itemize}"),
        "  • a[1]\n\n---\n[1] n\n"
    );
}

// -------------------------------------------------------------------- theorems

#[test]
fn a_theorem_names_itself_and_runs_its_body_on() {
    assert_eq!(
        text(r"\begin{theorem} Every group is a set. \end{theorem}"),
        "Theorem. Every group is a set.\n"
    );
}

#[test]
fn a_note_goes_in_parentheses_before_the_full_stop() {
    assert_eq!(
        text(r"\begin{lemma}[due to Euler] It holds. \end{lemma}"),
        "Lemma (due to Euler). It holds.\n"
    );
}

#[test]
fn the_short_aliases_spell_the_word_out_in_full() {
    assert_eq!(text(r"\begin{thm} A \end{thm}"), "Theorem. A\n");
    assert_eq!(text(r"\begin{prop} A \end{prop}"), "Proposition. A\n");
    assert_eq!(text(r"\begin{defn} A \end{defn}"), "Definition. A\n");
    assert_eq!(text(r"\begin{cor} A \end{cor}"), "Corollary. A\n");
}

#[test]
fn a_proof_ends_with_an_end_of_proof_mark() {
    assert_eq!(
        text(r"\begin{proof} By induction. \end{proof}"),
        "Proof. By induction. □\n"
    );
}

#[test]
fn a_theorem_is_a_paragraph_of_its_own() {
    assert_eq!(
        text(r"Before. \begin{remark} Note. \end{remark} After."),
        "Before.\n\nRemark. Note.\n\nAfter.\n"
    );
}

// ---------------------------------------------------------------------- blocks

#[test]
fn a_quotation_is_set_in_from_the_margin() {
    assert_eq!(
        text(r"Before. \begin{quote} Quoted. \end{quote} After."),
        "Before.\n    Quoted.\nAfter.\n"
    );
}

#[test]
fn centring_is_not_simulated() {
    assert_eq!(text(r"\begin{center} Middle \end{center}"), "Middle\n");
    assert_eq!(
        text(r"\begin{flushright} Right \end{flushright}"),
        "Right\n"
    );
}

#[test]
fn an_abstract_writes_its_own_heading() {
    assert_eq!(
        text(r"\begin{abstract} We show things. \end{abstract} Body."),
        "Abstract\n\n    We show things.\n\nBody.\n"
    );
}

// --------------------------------------------------------------- floats and captions

#[test]
fn a_caption_is_named_after_the_float_it_is_in() {
    assert_eq!(
        text(r"\begin{figure}\caption{A picture.}\end{figure}"),
        "Figure: A picture.\n"
    );
    assert_eq!(
        text(r"\begin{table}[htbp]\caption{Some numbers.}\end{table}"),
        "Table: Some numbers.\n"
    );
    assert_eq!(text(r"\caption{Loose.}"), "Caption: Loose.\n");
}

#[test]
fn a_caption_is_a_paragraph_of_its_own() {
    assert_eq!(
        text(r"\begin{figure}Contents.\caption{Below it.}\end{figure}After."),
        "Contents.\n\nFigure: Below it.\n\nAfter.\n"
    );
}

#[test]
fn the_short_caption_is_dropped() {
    assert_eq!(
        text(r"\begin{figure}\caption[toc]{The real one.}\end{figure}"),
        "Figure: The real one.\n"
    );
}

#[test]
fn the_starred_float_is_the_same_float() {
    assert_eq!(
        text(r"\begin{figure*}\caption{Wide.}\end{figure*}"),
        "Figure: Wide.\n"
    );
}

#[test]
fn a_caption_inside_a_table_float_knows_it_is_a_table() {
    // The float state reaches through whatever is nested in between.
    assert_eq!(
        text(r"\begin{table}\begin{center}\caption{Counts.}\end{center}\end{table}"),
        "Table: Counts.\n"
    );
}

// ------------------------------------------------------ boxes that hold blocks

#[test]
fn a_minipage_is_a_block_of_its_own() {
    assert_eq!(
        text("before\n\n\\begin{minipage}[t]{4cm}\ninside\n\\end{minipage}\n\nafter"),
        "before\n\ninside\n\nafter\n"
    );
    // A title page is the same shape, and `spacing` and `multicols` only change how
    // their body is set.
    assert_eq!(text(r"\begin{titlepage}Title\end{titlepage}"), "Title\n");
    assert_eq!(text(r"\begin{spacing}{1.5}wide\end{spacing}"), "wide\n");
    assert_eq!(text(r"\begin{multicols}{2}columns\end{multicols}"), "columns\n");
}
