//! The CLI over a realistic paper, with its output frozen (PLAN.md §16, M7).
//!
//! `tests/fixtures/paper.tex` is a small but complete article: `\documentclass` and
//! preamble, `\maketitle`, an abstract, numbered sections and a subsection, inline and
//! display mathematics, an itemized list, a theorem and a proof, a `tabular` inside a
//! `table` float with a caption, a `figure` with `\includegraphics`, two footnotes,
//! citations and cross-references. `tests/fixtures/paper.txt` is what techxt makes of it.
//!
//! The point of freezing it is coverage no unit test buys: every one of those constructs
//! meets every other one in a single document, so a change in spacing, in block
//! separation or in the order the footnote block is appended shows up here even when each
//! construct's own test still passes. When the expectation legitimately changes,
//! regenerate it with
//!
//! ```text
//! cargo run -p techxt-cli -- --wrap 78 \
//!     techxt-cli/tests/fixtures/paper.tex > techxt-cli/tests/fixtures/paper.txt
//! ```
//!
//! and read the diff before committing it.

use std::path::PathBuf;
use std::process::Command;

/// The path of a checked-in fixture.
fn fixture(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    path
}

#[test]
fn the_sample_paper_converts_to_its_frozen_expectation() {
    let output = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .arg("--wrap")
        .arg("78")
        .arg(fixture("paper.tex"))
        .output()
        .expect("the binary cargo built for this test runs");

    let text = String::from_utf8(output.stdout).expect("the converted text is UTF-8");
    let expected =
        std::fs::read_to_string(fixture("paper.txt")).expect("the frozen expectation reads");
    assert_eq!(text, expected, "the paper's conversion changed");

    // Spot checks that say *what* the frozen text is supposed to contain, so that a
    // wholesale regeneration of the expectation cannot quietly drop a whole construct.
    for expected_fragment in [
        "On the Slow Convergence of Impatient Methods", // \maketitle
        "March 3, 2025",                                // \date, not \today
        "A.\u{a0}Researcher and B.\u{a0}Coauthor",      // two authors, separated by \and
        "Abstract",                                     // the abstract environment
        "1 Introduction",                               // a numbered section
        "2.1 The impatient stopping rule",              // a numbered subsection
        "𝑥ₙ₊₁ = 𝑇(𝑥ₙ)",                                 // inline math with a subscript
        "  • 0 < 𝐿 < 1",                                // an itemize marker
        "Theorem.",                                     // a theorem environment
        "□",                                            // the proof's end mark
        "Contraction 𝐿  Patient  Impatient",            // the tabular's header row
        "Table: Iterations to reach ε = 10⁻⁶.",         // a float caption
        "< g r a p h i c s >",                          // \includegraphics
        "Figure: Distance to the fixed point",          // the figure's caption
        "<cit.>",                                       // \cite
        "Theorem\u{a0}<ref> predicts",                  // \ref, after a `~` tie
        "(<ref>)",                                      // \eqref
        "[1] It is elementary",                         // the collected footnote block
    ] {
        assert!(
            text.contains(expected_fragment),
            "the frozen paper lost {expected_fragment:?}"
        );
    }

    // No line exceeds the requested width. The wrap flag is doing its job across a
    // document of mixed content, not only on the one paragraph example #25 tests.
    for line in text.lines() {
        assert!(
            line.chars().count() <= 78,
            "a line escaped the wrap width: {line:?}"
        );
    }
}

#[test]
fn the_sample_paper_converts_without_a_single_diagnostic() {
    // Frozen deliberately, and worth stating plainly: a complete, ordinary LaTeX file
    // converts **silently** and exits 0. Everything this paper uses — `document`
    // included, and `\and` between its two authors — is in `techxt::defs`, so nothing
    // reaches the unknown policy (PLAN.md §12.2) and nothing reaches techy's own
    // unknown-environment error. This is the property that makes the exit code worth
    // reading: a non-zero one means the document really did have something wrong with
    // it, not that techxt has a gap.
    let output = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .arg("--wrap")
        .arg("78")
        .arg("-v") // notes as well as warnings: everything techxt has to say.
        .arg(fixture("paper.tex"))
        .output()
        .expect("the binary cargo built for this test runs");

    let report = String::from_utf8(output.stderr).expect("the report is UTF-8");
    assert_eq!(report, "", "a clean paper reported something");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn quiet_converts_the_same_paper_without_a_word() {
    // The same run under `-q`: identical text, nothing on standard error, and the same
    // exit code — the report is silenced, the outcome is not.
    let output = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .arg("--wrap")
        .arg("78")
        .arg("-q")
        .arg(fixture("paper.tex"))
        .output()
        .expect("the binary cargo built for this test runs");

    let expected =
        std::fs::read_to_string(fixture("paper.txt")).expect("the frozen expectation reads");
    assert_eq!(
        String::from_utf8(output.stdout).expect("the converted text is UTF-8"),
        expected
    );
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}
