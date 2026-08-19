//! **The normative acceptance suite: all twenty-six examples of PLAN.md §15.**
//!
//! §15 says of its table: "Default options unless noted. Expected strings are exact.
//! These are behavior law; encode them as tests early." They were encoded early, and
//! spread across the suites that grew around them — the layout examples in `layout.rs`,
//! the math ones in `math_examples.rs`, the definitions ones in `defs_examples.rs`,
//! the lists and tables in their own files. This file is the single place that answers
//! "do all twenty-six pass?", numbered exactly as the plan numbers them, each converted
//! end to end through [`Converter`] — which is the level a user meets them at.
//!
//! It deliberately duplicates what the other suites assert. There each example sits
//! among the behaviour around it and is tested with the neighbours it interacts with;
//! here they sit together, in order, so that a reader can check the whole of §15 in one
//! screenful and a change to any of them cannot be mistaken for a local test edit.
//!
//! Every expected string includes the trailing newline the layout engine guarantees: a
//! nonempty output ends with exactly one `\n` (PLAN.md §7). Where §15 shows a blank
//! line it is written `\n\n`.
//!
//! Cross-references to the suites that test each example in its own context:
//!
//! | examples | also in |
//! |---|---|
//! | 1, 2, 4, 22, 24, 25, 26 | `layout.rs`, `render_examples.rs`, `render_proptest.rs` |
//! | 3, 5–9, 16, 17, 20 | `defs_examples.rs` |
//! | 10–15, 23 | `math_examples.rs`, `math_engine.rs`, `defs_math.rs` |
//! | 18, 19, 21, 22 | `defs_blocks.rs`, `defs_verbatim.rs`, `defs_tables.rs`, `defs_lists.rs` |

use techxt::convert::{MathMode, MatrixDelims};
use techxt::Converter;

/// Convert with the shipped definitions and the **default** options — what §15 means by
/// "default options unless noted".
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("a tolerant parse of well-formed input succeeds")
        .text
}

// ---------------------------------------------------------------- 1–2: text and space

/// §15 #1 — `Hello  {brave}\n world.` → `Hello brave world.`
#[test]
fn example_1_source_whitespace_collapses_and_a_group_is_transparent() {
    assert_eq!(text("Hello  {brave}\n world."), "Hello brave world.\n");
}

/// §15 #2 — `one\n\n\n\ntwo` → `one` / blank / `two`
#[test]
fn example_2_any_run_of_blank_lines_is_one_paragraph_break() {
    assert_eq!(text("one\n\n\n\ntwo"), "one\n\ntwo\n");
}

// ------------------------------------------------------- 3–9: fonts, symbols, accents

/// §15 #3 — `\emph{sic}` → `𝑠𝑖𝑐`
#[test]
fn example_3_emph_italicizes() {
    assert_eq!(text(r"\emph{sic}"), "𝑠𝑖𝑐\n");
}

/// §15 #4 — `\textbf{bold}text` → `𝐛𝐨𝐥𝐝text`, one unbreakable word
#[test]
fn example_4_a_macro_boundary_is_not_a_word_boundary() {
    assert_eq!(text(r"\textbf{bold}text"), "𝐛𝐨𝐥𝐝text\n");
    // "One unbreakable word" is the load-bearing half: a wrap width narrower than it
    // may not split it.
    let narrow = Converter::builder()
        .wrap_width(Some(4))
        .build()
        .expect("builds")
        .latex_to_text(r"\textbf{bold}text")
        .expect("parses")
        .text;
    assert_eq!(narrow, "𝐛𝐨𝐥𝐝text\n");
}

/// §15 #5 — `Sk\l odowska` → `Skłodowska`
#[test]
fn example_5_a_symbol_macro_eats_the_space_that_ends_its_name() {
    assert_eq!(text(r"Sk\l odowska"), "Skłodowska\n");
}

/// §15 #6 — `\'{e}t\'e` → `été`
#[test]
fn example_6_an_accent_takes_a_braced_or_a_bare_argument() {
    assert_eq!(text(r"\'{e}t\'e"), "été\n");
}

/// §15 #7 — `\c c` → `ç`
#[test]
fn example_7_an_accent_composes_with_the_letter_after_it() {
    assert_eq!(text(r"\c c"), "ç\n");
}

/// §15 #8 — ``` ``Hi,'' -- ok ``` → `“Hi,” – ok`
#[test]
fn example_8_input_ligatures() {
    assert_eq!(text("``Hi,'' -- ok"), "“Hi,” – ok\n");
}

/// §15 #9 — `A% note\nB` → `AB`; with `keep_comments`: `A` / `% note` / `B`
#[test]
fn example_9_a_comment_and_its_newline_vanish_unless_kept() {
    assert_eq!(text("A% note\nB"), "AB\n");

    let keeping = Converter::builder()
        .keep_comments(true)
        .build()
        .expect("builds")
        .latex_to_text("A% note\nB")
        .expect("parses")
        .text;
    assert_eq!(keeping, "A\n% note\nB\n");
}

// -------------------------------------------------------------------- 10–15, 23: math

/// §15 #10 — `$x^2 + y_i$` → `𝑥² + 𝑦ᵢ`
#[test]
fn example_10_scripts_become_unicode_script_characters() {
    assert_eq!(text("$x^2 + y_i$"), "𝑥² + 𝑦ᵢ\n");
}

/// §15 #11 — `$\frac{4\pi c}{2}\sin(x+y)$` → `(4π𝑐)/2 sin(𝑥 + 𝑦)`
#[test]
fn example_11_a_fraction_wraps_its_operands_eagerly() {
    assert_eq!(text(r"$\frac{4\pi c}{2}\sin(x+y)$"), "(4π𝑐)/2 sin(𝑥 + 𝑦)\n");
}

/// §15 #12 — `$\sum_{i=1}^n x_i$` → `∑ᵢ₌₁ⁿ 𝑥ᵢ`
#[test]
fn example_12_the_script_retry_without_the_joiners_spaces() {
    assert_eq!(text(r"$\sum_{i=1}^n x_i$"), "∑ᵢ₌₁ⁿ 𝑥ᵢ\n");
}

/// §15 #13 — `$a + b$` with `math_mode = Plain` → `𝑎+𝑏` (fonts apply, no joiner spacing)
#[test]
fn example_13_plain_math_keeps_the_fonts_and_drops_the_spacing() {
    let plain = Converter::builder()
        .math_mode(MathMode::Plain)
        .build()
        .expect("builds")
        .latex_to_text("$a + b$")
        .expect("parses")
        .text;
    assert_eq!(plain, "𝑎+𝑏\n");
}

/// §15 #14 — `$a + b$` with `math_mode = Source` → `$a + b$`
#[test]
fn example_14_source_math_re_emits_the_formula() {
    let source = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("builds")
        .latex_to_text("$a + b$")
        .expect("parses")
        .text;
    assert_eq!(source, "$a + b$\n");
}

/// §15 #15 — `\[ E = mc^2 \]` → `    𝐸 = 𝑚𝑐²`, a four-space display block
#[test]
fn example_15_display_math_is_an_indented_block() {
    assert_eq!(text(r"\[ E = mc^2 \]"), "    𝐸 = 𝑚𝑐²\n");
}

/// §15 #23 — a display `pmatrix`, in both delimiter styles
#[test]
fn example_23_a_display_matrix_draws_its_delimiters_to_height() {
    let matrix = r"\[\begin{pmatrix} 1 & 2 \\ 30 & 4 \end{pmatrix}\]";
    assert_eq!(text(matrix), "    ⎛  1  2 ⎞\n    ⎝ 30  4 ⎠\n");

    let ascii = Converter::builder()
        .matrix_delimiters(MatrixDelims::Ascii)
        .build()
        .expect("builds")
        .latex_to_text(matrix)
        .expect("parses")
        .text;
    assert_eq!(ascii, "    (  1  2 )\n    ( 30  4 )\n");
}

// ------------------------------------------------------- 16–18: headings and footnotes

/// §15 #16 — `\section{Intro}\nText.` → `1 Intro` / rule / blank / `Text.`
#[test]
fn example_16_a_numbered_section_over_its_rule() {
    assert_eq!(
        text("\\section{Intro}\nText."),
        "1 Intro\n-------\n\nText.\n"
    );
}

/// §15 #17 — `\subsection*{Notes}` → `Notes` / `~~~~~`, unnumbered and uncounted
#[test]
fn example_17_a_starred_heading_is_neither_numbered_nor_counted() {
    assert_eq!(text(r"\subsection*{Notes}"), "Notes\n~~~~~\n");
    // "not counted": the next unstarred subsection is still the first one.
    assert_eq!(
        text("\\section{S}\\subsection*{Notes}\\subsection{First}"),
        "1 S\n---\n\nNotes\n~~~~~\n\n1.1 First\n~~~~~~~~~\n"
    );
}

/// §15 #18 — `Fact\footnote{Proof sketch.} holds.` → the marker, then the collected note
#[test]
fn example_18_footnotes_are_collected_after_the_document() {
    assert_eq!(
        text(r"Fact\footnote{Proof sketch.} holds."),
        "Fact[1] holds.\n\n---\n[1] Proof sketch.\n"
    );
}

// ---------------------------------------------------- 19–22, 24: blocks and raw content

/// §15 #19 — `\verb|x_1|` → `x_1`
#[test]
fn example_19_verb_is_raw_characters() {
    assert_eq!(text(r"\verb|x_1|"), "x_1\n");
}

/// §15 #20 — `\href{https://ex.org/a_b}{link}` → `link <https://ex.org/a_b>`
#[test]
fn example_20_href_renders_its_text_then_its_url() {
    assert_eq!(
        text(r"\href{https://ex.org/a_b}{link}"),
        "link <https://ex.org/a_b>\n"
    );
}

/// §15 #21 — a `tabular`, its columns padded to their widest cell
#[test]
fn example_21_a_table_is_aligned_in_columns() {
    assert_eq!(
        text(r"\begin{tabular}{lr} a & 10 \\ bb & 3 \end{tabular}"),
        "a   10\nbb   3\n"
    );
}

/// §15 #22 — nested lists, the inner one indented under the outer item's text
#[test]
fn example_22_nested_lists() {
    assert_eq!(
        text(
            r"\begin{itemize}\item one \begin{enumerate}\item x\item y\end{enumerate}\item two\end{itemize}"
        ),
        "  • one\n    1. x\n    2. y\n  • two\n"
    );
}

/// §15 #24 — a `verbatim` body byte for byte, blank-line separated, never wrapped
#[test]
fn example_24_a_verbatim_body_survives_exactly() {
    let document =
        "before\n\\begin{verbatim}\n  keep   this\n\n    exactly\n\\end{verbatim}\nafter";
    let expected = "before\n\n  keep   this\n\n    exactly\n\nafter\n";
    assert_eq!(text(document), expected);

    // "never wrapped": the same document at a width narrower than the content is
    // byte-identical inside the block.
    let narrow = Converter::builder()
        .wrap_width(Some(5))
        .build()
        .expect("builds")
        .latex_to_text(document)
        .expect("parses")
        .text;
    assert!(
        narrow.contains("  keep   this\n\n    exactly\n"),
        "the verbatim body was reflowed: {narrow:?}"
    );
}

// ------------------------------------------------------- 25–26: wrapping and the unknown

/// §15 #25 — `aaa bbb \textbf{ccc ddd} eee` at `wrap_width = 12` wraps across the macro
#[test]
fn example_25_wrapping_crosses_a_macro_boundary() {
    let wrapped = Converter::builder()
        .wrap_width(Some(12))
        .build()
        .expect("builds")
        .latex_to_text(r"aaa bbb \textbf{ccc ddd} eee")
        .expect("parses")
        .text;
    assert_eq!(wrapped, "aaa bbb 𝐜𝐜𝐜\n𝐝𝐝𝐝 eee\n");
}

/// §15 #26 — an unknown environment renders its body and warns
#[test]
fn example_26_an_unknown_environment_renders_its_body_with_a_warning() {
    let conversion = Converter::standard()
        .latex_to_text(r"\begin{myenv}inner\end{myenv}")
        .expect("tolerant recovery keeps going");
    assert_eq!(conversion.text, "inner\n");

    let warnings: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert!(
        warnings.contains(&"techxt.unknown-environment"),
        "{warnings:?}"
    );

    // What §15 asks for is the body and that warning, and both are here. The
    // conversion also carries techy's own `latexlike.environments.unknown-environment`
    // at *error* severity: no definition claimed the name, so the parser records a
    // resolution failure and recovers, which is what tolerant recovery is. §15 says
    // nothing about it, and it is the reason a document using an environment techxt
    // does not know exits 1 from the CLI while converting perfectly well — hence
    // PLAN.md §12.2's rule that nothing *standard* may reach this path.
    assert!(warnings.contains(&"latexlike.environments.unknown-environment"));
}
