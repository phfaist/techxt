//! The descent guard the CLI configures (DECISIONS.md D9 item 3).
//!
//! techy bounds recursion with a guard so that a document nesting a thousand groups deep
//! is *refused* rather than overflowing the stack and killing the process. The guard has
//! to know how much stack it may spend, and `techxt` the library cannot find out: it is
//! `no_std`, so it ships techy's fixed default budget, which is deliberately tight — in
//! an unoptimized build it admits only on the order of ten nesting levels.
//!
//! `techxt-cli` links std, so it measures the stack instead of guessing, and that is what
//! makes it a usable converter rather than a demonstration. These tests prove the
//! difference rather than describing it: each one compares the binary against the library
//! it embeds, running in this very process, so the comparison holds in every build
//! profile without a hard-coded depth.

use std::io::Write;
use std::process::{Command, Stdio};

/// A document nesting `depth` groups around a word.
///
/// Braces are the cheapest way to nest: no definitions are involved, so what is measured
/// is the parser's own descent and nothing else.
fn nested(depth: usize) -> String {
    format!("{}deep{}", "{".repeat(depth), "}".repeat(depth))
}

/// Convert `document` through the binary; answer `(exit code, stdout, stderr)`.
fn convert(document: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary cargo built for this test runs");
    {
        let mut pipe = child.stdin.take().expect("standard input was piped");
        let _ = pipe.write_all(document.as_bytes());
    }
    let output = child.wait_with_output().expect("the child was waited for");
    (
        output
            .status
            .code()
            .expect("the child exited rather than being signalled"),
        String::from_utf8(output.stdout).expect("the converted text is UTF-8"),
        String::from_utf8(output.stderr).expect("the report is UTF-8"),
    )
}

/// The shallowest nesting depth the *library's* default guard refuses.
///
/// Searched for rather than assumed: how many levels a fixed byte budget buys depends on
/// how much stack a frame takes, which differs between debug and release builds and
/// between platforms. The search is what keeps this test honest in all of them.
fn depth_the_library_refuses() -> usize {
    let library = techxt::Converter::standard();
    (1..4000)
        .find(|depth| library.latex_to_text(&nested(*depth)).is_err())
        .expect("techy's default guard refuses some depth")
}

#[test]
fn the_cli_converts_a_document_the_library_default_refuses() {
    // The headline of D9 item 3, as a single comparison: one document, refused by the
    // library under its own default guard, converted by the binary that embeds it.
    let depth = depth_the_library_refuses();

    let refusal = techxt::Converter::standard()
        .latex_to_text(&nested(depth))
        .expect_err("the library's default guard refuses this depth");
    assert_eq!(
        refusal.identifier(),
        "core.constructs.descent-limit-exceeded"
    );

    let (code, stdout, stderr) = convert(&nested(depth));
    assert_eq!(stdout, "deep\n", "the CLI converted it");
    assert_eq!(stderr, "", "and had nothing to complain about");
    assert_eq!(code, 0);
}

#[test]
fn a_realistically_deep_document_converts() {
    // The measured budget is not a marginal improvement over the fixed one: a hundred
    // levels of nesting is more than any hand-written document reaches, and it converts
    // with room to spare in both build profiles.
    let (code, stdout, stderr) = convert(&nested(100));
    assert_eq!(stdout, "deep\n");
    assert_eq!(stderr, "");
    assert_eq!(code, 0);
    assert!(
        depth_the_library_refuses() < 100,
        "this test is only meaningful while the library's default is the tighter one"
    );
}

#[test]
fn the_guard_is_raised_not_removed() {
    // A measured budget is still a budget. Pathological input is refused — with a
    // message and exit code 2 — rather than crashing the process, which is what
    // `StdDescentGuardInit::off()` would have risked.
    let (code, stdout, stderr) = convert(&nested(50_000));
    assert_eq!(stdout, "", "a refused parse produces no text");
    assert!(
        stderr.contains("nests too deeply"),
        "the refusal says what happened: {stderr:?}"
    );
    assert_eq!(code, 2, "PLAN.md §13: a parse that aborts is exit code 2");
}

#[test]
fn the_fold_side_of_the_guard_is_raised_too() {
    // `ConverterBuilder::descent_guard` configures the parser *and* the recomposer's
    // fold, and both are on the path from standard input to standard output. A document
    // whose parse succeeds but whose fold aborts would print empty output with an error
    // diagnostic (PLAN.md §10.4) — exit code 1 with no text. Deep-but-parsable input is
    // the shape that would expose it.
    let depth = depth_the_library_refuses() * 4;
    let (code, stdout, stderr) = convert(&nested(depth));
    assert_eq!(stdout, "deep\n", "the fold ran to completion");
    assert!(
        !stderr.contains("techxt.render-aborted") && !stderr.contains("aborted"),
        "the fold did not abort: {stderr:?}"
    );
    assert_eq!(code, 0);
}
