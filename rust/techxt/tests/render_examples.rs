//! The normative acceptance examples of PLAN.md §15 that the renderer core covers, and
//! the node-kind dispatch of PLAN.md §9.1.
//!
//! Expected strings are exact, trailing newline included: the layout engine terminates
//! non-empty output with exactly one newline, and a test that trims would stop checking
//! the very thing PLAN.md §7 guarantees.

use techxt::convert::{MathMode, StdDescentGuardInit, UnknownEnvPolicy};
use techxt::diag::{UnknownEnvironment, UnknownMacro};
use techxt::Converter;

/// Convert with the default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("a tolerant parse of well-formed input succeeds")
        .text
}

// ---------------------------------------------------------------- PLAN.md §15

#[test]
fn example_1_groups_are_transparent_and_spaces_collapse() {
    assert_eq!(text("Hello  {brave}\n world."), "Hello brave world.\n");
}

#[test]
fn example_2_blank_lines_normalize_to_one() {
    assert_eq!(text("one\n\n\n\ntwo"), "one\n\ntwo\n");
}

#[test]
fn example_9_comments_render_as_nothing() {
    // Not even the comment's trailing newline survives, so the two characters meet.
    assert_eq!(text("A% note\nB"), "AB\n");
}

#[test]
fn example_9_keep_comments_gives_the_comment_its_own_line() {
    let converter = Converter::builder()
        .keep_comments(true)
        .build()
        .expect("the minimal definitions build");
    assert_eq!(
        converter.latex_to_text("A% note\nB").expect("parses").text,
        "A\n% note\nB\n"
    );
}

#[test]
fn example_19_verb_keeps_its_characters() {
    assert_eq!(text(r"\verb|x_1|"), "x_1\n");
}

#[test]
fn example_20_href_renders_text_then_url() {
    assert_eq!(
        text(r"\href{https://ex.org/a_b}{link}"),
        "link <https://ex.org/a_b>\n"
    );
}

#[test]
fn example_24_verbatim_body_survives_byte_for_byte() {
    let converted = text("\\begin{verbatim}\n  keep   this\n\n    exactly\n\\end{verbatim}");
    assert_eq!(converted, "  keep   this\n\n    exactly\n");
}

#[test]
fn example_24_verbatim_is_blank_line_separated_and_never_wrapped() {
    let converter = Converter::builder()
        .wrap_width(Some(8))
        .build()
        .expect("the minimal definitions build");
    let converted = converter
        .latex_to_text(
            "before\n\\begin{verbatim}\n  a very long verbatim line\n\\end{verbatim}\nafter",
        )
        .expect("parses")
        .text;
    assert_eq!(
        converted,
        "before\n\n  a very long verbatim line\n\nafter\n"
    );
}

#[test]
fn example_26_unknown_environment_renders_its_body_and_warns() {
    let conversion = Converter::standard()
        .latex_to_text(r"\begin{myenv}inner\end{myenv}")
        .expect("a tolerant parse survives an unknown environment");
    assert_eq!(conversion.text, "inner\n");

    let warned: Vec<&UnknownEnvironment> = conversion
        .diagnostics
        .conditions::<UnknownEnvironment>()
        .collect();
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].name, "myenv");
}

// ------------------------------------------------------ PLAN.md §9.1 dispatch

#[test]
fn adjacent_text_is_one_unbreakable_word() {
    // PLAN.md §15 example 4's structure, without the font styling M5 adds: the two
    // pieces must not be separated by a space, and (below) must not wrap apart.
    assert_eq!(text(r"\textbf{bold}text"), "boldtext\n");
}

#[test]
fn a_macro_post_space_is_never_emitted() {
    // The space after a control word is part of the invocation, not of the text —
    // exactly as in LaTeX, where `\ldots more` sets an ellipsis flush against "more".
    // A brace group is the way to ask for the space back.
    assert_eq!(text(r"\ldots more"), "…more\n");
    assert_eq!(text(r"a\ldots b"), "a…b\n");
    assert_eq!(text(r"a\ldots{} b"), "a… b\n");
}

#[test]
fn nested_groups_are_all_transparent() {
    assert_eq!(text("a {b {c} d} e"), "a b c d e\n");
}

#[test]
fn an_empty_document_converts_to_an_empty_string() {
    assert_eq!(text(""), "");
    assert_eq!(text("   \n  "), "");
    assert_eq!(text("% only a comment\n"), "");
}

#[test]
fn a_paragraph_break_macro_separates_paragraphs() {
    assert_eq!(text(r"a\par b"), "a\n\nb\n");
}

#[test]
fn a_tie_is_a_space_no_line_may_break_in() {
    let converter = Converter::builder()
        .wrap_width(Some(6))
        .build()
        .expect("the minimal definitions build");
    // "Fig.\u{a0}1" is one word of width 6, so it cannot be split even though the
    // line has no room for it and the following word.
    assert_eq!(
        converter
            .latex_to_text(r"Fig.~1 next")
            .expect("parses")
            .text,
        "Fig.\u{a0}1\nnext\n"
    );
}

#[test]
fn math_is_a_documented_placeholder_until_the_math_milestone() {
    // TODO(M5b): PLAN.md §15 examples 10 and 15 replace these expectations with
    // `𝑥²` and `    𝐸 = 𝑚𝑐²`. What is settled already is the *structure*: inline math
    // is transparent, and display math is a four-space indented block.
    assert_eq!(text("$x^2$"), "x^2\n");
    assert_eq!(text(r"\[ E = mc^2 \]"), "    E = mc^2\n");
}

#[test]
fn example_14_source_math_mode_re_emits_the_latex() {
    let converter = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("the minimal definitions build");
    assert_eq!(
        converter.latex_to_text("$a + b$").expect("parses").text,
        "$a + b$\n"
    );
    // Display math becomes a preformatted block of its own.
    assert_eq!(
        converter
            .latex_to_text(r"x \[ a + b \] y")
            .expect("parses")
            .text,
        "x\n\n\\[ a + b \\]\n\ny\n"
    );
}

#[test]
fn source_math_is_reassembled_from_payloads_not_from_the_source_buffer() {
    let converter = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("the minimal definitions build");
    // A macro invocation inside the formula has to be re-spelled from its recorded
    // invocation syntax — escape character, name, post-space — and its argument groups
    // from their recorded delimiters.
    assert_eq!(
        converter
            .latex_to_text(r"$\textbf{a} + \ldots b$")
            .expect("parses")
            .text,
        "$\\textbf{a} + \\ldots b$\n"
    );
}

// ------------------------------------------------------------- robustness

#[test]
fn an_unknown_macro_is_a_warning_and_the_policy_decides() {
    // techy cannot shape an invocation it has no definition for, so techxt registers a
    // catch-all provider that answers any unclaimed command with a generic,
    // argument-less spec. `\foo` therefore parses as a macro, techxt's unknown-macro
    // policy applies to it, and the report is a warning rather than a parse error.
    //
    // The catch-all takes no arguments, which is the deliberate consequence: `{x}` is
    // an ordinary group after the macro, so its content survives under the default
    // `Skip` policy while `\foo` itself renders as nothing.
    let conversion = Converter::standard()
        .latex_to_text(r"\foo{x} bar")
        .expect("tolerant recovery keeps going");
    assert_eq!(conversion.text, "x bar\n");
    assert_eq!(
        conversion.diagnostics.conditions::<UnknownMacro>().count(),
        1
    );
    assert!(!conversion.diagnostics.has_errors());
}

#[test]
fn deeply_nested_input_is_refused_rather_than_overflowing_the_stack() {
    // Recursive-descent parsing spends stack per nesting level; the descent guard is
    // what keeps a pathological document from aborting the process. The limit is
    // configured explicitly here (DECISIONS.md D9) because the library default is a
    // *stack budget*, whose reach depends on the build profile; a depth limit is the
    // deterministic mode and the one techy's documentation recommends for tests.
    let converter = Converter::builder()
        .descent_guard(StdDescentGuardInit::depth_limit(24))
        .build()
        .expect("the placeholder definitions build");
    let pathological = "{".repeat(2000);
    let error = converter
        .latex_to_text(&pathological)
        .expect_err("the descent guard refuses");
    assert_eq!(error.identifier(), "core.constructs.descent-limit-exceeded");

    // ... and a document just inside the limit still converts.
    assert_eq!(
        converter
            .latex_to_text(&alloc_nested("deep", 8))
            .expect("inside the limit")
            .text,
        "deep\n"
    );
}

/// `depth` nested groups around `text`.
fn alloc_nested(text: &str, depth: usize) -> String {
    format!("{}{text}{}", "{".repeat(depth), "}".repeat(depth))
}

#[test]
fn an_unterminated_environment_still_converts() {
    let conversion = Converter::standard()
        .latex_to_text(r"\begin{center}text")
        .expect("tolerant recovery keeps going");
    assert_eq!(conversion.text, "text\n");
}

#[test]
fn an_unknown_environment_can_be_skipped_entirely() {
    let converter = Converter::builder()
        .unknown_env(UnknownEnvPolicy::Skip)
        .build()
        .expect("the minimal definitions build");
    assert_eq!(
        converter
            .latex_to_text(r"a\begin{myenv}inner\end{myenv}b")
            .expect("parses")
            .text,
        "ab\n"
    );
}
