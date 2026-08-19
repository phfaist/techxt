//! `defs::verbatim`: the `verbatim` environment family and `\verb` (PLAN.md §9.8).
//!
//! What these are about is that raw content survives *exactly*: the layout engine never
//! wraps it, never trims it, never collapses its spaces and never styles its letters.

use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

fn wrapped(width: usize, latex: &str) -> String {
    Converter::builder()
        .wrap_width(Some(width))
        .build()
        .expect("builds")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// ------------------------------------------------------------------------- `\verb`

#[test]
fn example_19_verb_keeps_its_characters() {
    assert_eq!(text(r"\verb|x_1|"), "x_1\n");
}

#[test]
fn verb_content_is_not_latex() {
    // A backslash, a comment character and a brace are all just characters in there.
    assert_eq!(text(r"\verb+\emph{a} % b+"), "\\emph{a} % b\n");
}

#[test]
fn verb_takes_the_delimiter_it_is_given() {
    assert_eq!(text(r"\verb!a b!"), "a b\n");
    assert_eq!(text(r"\verb#a#"), "a\n");
}

#[test]
fn a_starred_verb_renders_its_run_and_not_its_star() {
    assert_eq!(text(r"\verb*|x y|"), "x y\n");
}

#[test]
fn verb_is_one_unbreakable_word() {
    // Even at a width narrower than the run, it is never split — and the words around
    // it wrap around it instead.
    assert_eq!(
        wrapped(8, r"aaa \verb|bbbbbbbbbb| ccc"),
        "aaa\nbbbbbbbbbb\nccc\n"
    );
}

#[test]
fn verb_keeps_the_spaces_at_its_edges() {
    // DECISIONS.md D13: raw content is exactly the case where a line legitimately ends
    // in a space, because that space is content and not collapsible whitespace.
    assert_eq!(text(r"a\verb| x |b"), "a x b\n");
}

// ---------------------------------------------------------- verbatim environments

#[test]
fn example_24_verbatim_body_survives_byte_for_byte() {
    assert_eq!(
        text("\\begin{verbatim}\n  keep   this\n\n    exactly\n\\end{verbatim}"),
        "  keep   this\n\n    exactly\n"
    );
}

#[test]
fn example_24_verbatim_is_blank_line_separated_and_never_wrapped() {
    assert_eq!(
        wrapped(
            8,
            "before\n\\begin{verbatim}\n  a very long verbatim line\n\\end{verbatim}\nafter"
        ),
        "before\n\n  a very long verbatim line\n\nafter\n"
    );
}

#[test]
fn a_verbatim_body_is_not_latex() {
    assert_eq!(
        text("\\begin{verbatim}\n\\textbf{not bold} $x^2$ %\n\\end{verbatim}"),
        "\\textbf{not bold} $x^2$ %\n"
    );
}

#[test]
fn the_starred_and_alltt_spellings_render_the_same_way() {
    assert_eq!(text("\\begin{verbatim*}\na  b\n\\end{verbatim*}"), "a  b\n");
    assert_eq!(text("\\begin{alltt}\na  b\n\\end{alltt}"), "a  b\n");
}

#[test]
fn lstlisting_accepts_and_drops_its_options() {
    assert_eq!(
        text("\\begin{lstlisting}[language=C]\nint main() { }\n\\end{lstlisting}"),
        "int main() { }\n"
    );
}

#[test]
fn a_verbatim_block_keeps_the_indent_of_the_list_it_is_written_in() {
    // Only the continuation indent — the bullet stays available for the item's text.
    assert_eq!(
        text(
            "\\begin{itemize}\\item text\n\\begin{verbatim}\n  raw\n\\end{verbatim}\\end{itemize}"
        ),
        "  • text\n\n      raw\n"
    );
}
