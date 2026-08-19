//! CLI smoke tests: every flag PLAN.md §13 lists, over fixture files, asserting standard
//! output, standard error and the exit code (PLAN.md §14.5).
//!
//! These drive the real binary rather than the library, because the things worth testing
//! here are exactly the things the library does not do: argument parsing, the two streams,
//! and the three exit codes. `CARGO_BIN_EXE_techxt` is the binary cargo just built, so no
//! test dependency is needed to find it.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What one run of the binary did.
struct Run {
    /// The exit code. PLAN.md §13: 0 clean, 1 converted-with-errors, 2 no output at all.
    code: i32,
    /// Everything written to standard output — the converted text, unless `-o` sent it
    /// to a file.
    stdout: String,
    /// Everything written to standard error: the diagnostics report, or the one-line
    /// failure message that comes with exit code 2.
    stderr: String,
}

/// Run the binary with `args` and `stdin` on its standard input.
///
/// Both output streams are captured whole. A write to the child's standard input is
/// allowed to fail: `--help` and a usage error make the child exit before reading, and
/// the resulting broken pipe is not what the test is about.
fn run_with_stdin(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary cargo built for this test runs");
    {
        let mut pipe = child.stdin.take().expect("standard input was piped");
        let _ = pipe.write_all(stdin.as_bytes());
        // Dropping the pipe here is what closes it: without the end-of-file the child
        // would wait for input forever.
    }
    let output = child.wait_with_output().expect("the child was waited for");
    Run {
        code: output
            .status
            .code()
            .expect("the child exited rather than being signalled"),
        stdout: String::from_utf8(output.stdout).expect("the converted text is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("the report is UTF-8"),
    }
}

/// Run the binary with `args` and nothing on standard input.
fn run(args: &[&str]) -> Run {
    run_with_stdin(args, "")
}

/// The path of a checked-in fixture document.
fn fixture(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    path.into_os_string()
        .into_string()
        .expect("the fixture path is UTF-8")
}

/// A scratch path under cargo's own temporary directory, unique to `name`.
///
/// Cargo hands integration tests `CARGO_TARGET_TMPDIR` precisely so that they need no
/// temporary-file dependency; the directory persists between runs, so each test uses a
/// name of its own and overwrites its own file.
fn scratch(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&path).expect("the scratch directory exists");
    path.push(name);
    path
}

/// Convert a fixture with `args` and assert the whole result.
///
/// Every smoke test goes through here so that no test can quietly ignore a stream: a
/// flag that started printing a warning would fail the tests that do not expect one.
fn assert_run(args: &[&str], stdout: &str, stderr: &str, code: i32) {
    let result = run(args);
    assert_eq!(result.stdout, stdout, "standard output of {args:?}");
    assert_eq!(result.stderr, stderr, "standard error of {args:?}");
    assert_eq!(result.code, code, "exit code of {args:?}");
}

// ------------------------------------------------------------------ input and output

#[test]
fn a_file_argument_is_converted_to_standard_output() {
    // PLAN.md §15 example #1, through the command line.
    assert_run(&[&fixture("greeting.tex")], "Hello brave world.\n", "", 0);
}

#[test]
fn standard_input_is_converted_when_no_file_is_named() {
    let document = std::fs::read_to_string(fixture("greeting.tex")).expect("the fixture reads");
    let result = run_with_stdin(&[], &document);
    assert_eq!(result.stdout, "Hello brave world.\n");
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 0);
}

#[test]
fn the_output_flag_writes_a_file_and_leaves_standard_output_empty() {
    let output = scratch("output-flag.txt");
    let _ = std::fs::remove_file(&output);
    let result = run(&[
        &fixture("greeting.tex"),
        "-o",
        output.to_str().expect("UTF-8 scratch path"),
    ]);
    assert_eq!(result.stdout, "", "the text went to the file, not the pipe");
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 0);
    assert_eq!(
        std::fs::read_to_string(&output).expect("the output file was written"),
        "Hello brave world.\n"
    );
}

#[test]
fn a_file_that_cannot_be_read_is_a_failure_not_a_panic() {
    let missing = scratch("no-such-document.tex");
    let _ = std::fs::remove_file(&missing);
    let result = run(&[missing.to_str().expect("UTF-8 scratch path")]);
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.starts_with("techxt: "),
        "the failure names the program: {:?}",
        result.stderr
    );
    assert!(
        result.stderr.contains("no-such-document.tex"),
        "the failure names the file: {:?}",
        result.stderr
    );
    assert_eq!(result.code, 2, "PLAN.md §13: an I/O failure is exit code 2");
}

#[test]
fn an_output_file_that_cannot_be_written_is_a_failure_not_a_panic() {
    let unwritable = scratch("no-such-directory/out.txt");
    let result = run(&[
        &fixture("greeting.tex"),
        "-o",
        unwritable.to_str().expect("UTF-8 scratch path"),
    ]);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.starts_with("techxt: "), "{:?}", result.stderr);
    assert_eq!(result.code, 2);
}

// ------------------------------------------------------------------------- the flags

#[test]
fn the_wrap_flag_wraps_across_a_macro_boundary() {
    // PLAN.md §15 example #25, which is this milestone's named acceptance example: the
    // bold word is three characters of styled text and wraps like any other word, even
    // though `\textbf{ccc ddd}` is one macro invocation.
    assert_run(
        &[&fixture("example25.tex"), "--wrap", "12"],
        "aaa bbb 𝐜𝐜𝐜\n𝐝𝐝𝐝 eee\n",
        "",
        0,
    );
    // The short form is the same flag.
    assert_run(
        &[&fixture("example25.tex"), "-w", "12"],
        "aaa bbb 𝐜𝐜𝐜\n𝐝𝐝𝐝 eee\n",
        "",
        0,
    );
    // And without it, the paragraph is one line however long it is.
    assert_run(&[&fixture("example25.tex")], "aaa bbb 𝐜𝐜𝐜 𝐝𝐝𝐝 eee\n", "", 0);
}

#[test]
fn the_math_mode_flag_selects_all_three_modes() {
    let file = fixture("math.tex");
    // PLAN.md §15 examples #10 and #13–#14, on one document.
    assert_run(&[&file], "𝑎 + 𝑏 and 𝑥² + 𝑦ᵢ\n", "", 0);
    assert_run(
        &[&file, "--math-mode", "fancy"],
        "𝑎 + 𝑏 and 𝑥² + 𝑦ᵢ\n",
        "",
        0,
    );
    assert_run(&[&file, "--math-mode", "plain"], "𝑎+𝑏 and 𝑥²+𝑦ᵢ\n", "", 0);
    assert_run(
        &[&file, "--math-mode", "source"],
        "$a + b$ and $x^2 + y_i$\n",
        "",
        0,
    );
}

#[test]
fn the_math_wrap_flag_selects_the_delimiters() {
    let file = fixture("mathwrap.tex");
    assert_run(&[&file], "The ratio (𝑎 + 𝑏)/2 matters.\n", "", 0);
    assert_run(
        &[&file, "--math-wrap", "braces"],
        "The ratio {𝑎 + 𝑏}/2 matters.\n",
        "",
        0,
    );
    // `none` is the option to take responsibility yourself: the numerator's extent is
    // then genuinely ambiguous, which is the point of the default.
    assert_run(
        &[&file, "--math-wrap", "none"],
        "The ratio 𝑎 + 𝑏/2 matters.\n",
        "",
        0,
    );
}

#[test]
fn the_matrix_delims_flag_selects_the_bracket_style() {
    // PLAN.md §15 example #23, both halves.
    let file = fixture("matrix.tex");
    assert_run(&[&file], "    ⎛  1  2 ⎞\n    ⎝ 30  4 ⎠\n", "", 0);
    assert_run(
        &[&file, "--matrix-delims", "ascii"],
        "    (  1  2 )\n    ( 30  4 )\n",
        "",
        0,
    );
}

#[test]
fn the_keep_comments_flag_keeps_them() {
    // PLAN.md §15 example #9, both halves.
    let file = fixture("comments.tex");
    assert_run(&[&file], "AB\n", "", 0);
    assert_run(&[&file, "--keep-comments"], "A\n% note\nB\n", "", 0);
}

#[test]
fn the_heading_style_flag_selects_all_four_styles() {
    // PLAN.md §9.2's table, one row per style.
    let file = fixture("headings.tex");
    assert_run(
        &[&file],
        "1 Intro\n-------\n\nText.\n\n1.1 Detail\n~~~~~~~~~~\n\nMore.\n",
        "",
        0,
    );
    assert_run(
        &[&file, "--heading-style", "underlined"],
        "Intro\n-----\n\nText.\n\nDetail\n~~~~~~\n\nMore.\n",
        "",
        0,
    );
    assert_run(
        &[&file, "--heading-style", "prefix"],
        "§ Intro\n\nText.\n\n§.§ Detail\n\nMore.\n",
        "",
        0,
    );
    assert_run(
        &[&file, "--heading-style", "plain"],
        "Intro\n\nText.\n\nDetail\n\nMore.\n",
        "",
        0,
    );
}

#[test]
fn the_footnote_style_flag_selects_all_three_styles() {
    // PLAN.md §15 example #18 plus the two other styles.
    let file = fixture("footnotes.tex");
    assert_run(
        &[&file],
        "Fact[1] holds.\n\n---\n[1] Proof sketch.\n",
        "",
        0,
    );
    assert_run(
        &[&file, "--footnote-style", "inline"],
        "Fact[Proof sketch.] holds.\n",
        "",
        0,
    );
    assert_run(&[&file, "--footnote-style", "skip"], "Fact holds.\n", "", 0);
}

#[test]
fn the_unknown_macro_flag_selects_the_policy() {
    // A macro no definition claims resolves to a zero-argument callable (DECISIONS.md
    // D16), so what is observable here is what the *name* renders as. `render-args` is
    // deliberately indistinguishable from `skip` on such a macro — there are no
    // arguments to render — and the pair is asserted rather than glossed over.
    let file = fixture("unknown.tex");
    let warning = "warning: no text rule for the macro ‘\\nosuchmacro’\n  at: @ (line 1, col 3)\n";
    assert_run(&[&file], "a b\n", warning, 0);
    assert_run(&[&file, "--unknown-macro", "skip"], "a b\n", warning, 0);
    assert_run(
        &[&file, "--unknown-macro", "render-args"],
        "a b\n",
        warning,
        0,
    );
    assert_run(
        &[&file, "--unknown-macro", "placeholder"],
        // The space after a macro name is part of the name's terminator, so the
        // placeholder abuts the following word exactly as `\nosuchmacro b` would.
        "a <nosuchmacro>b\n",
        warning,
        0,
    );
    assert_run(
        &[&file, "--unknown-macro", "keep-source"],
        "a \\nosuchmacro b\n",
        warning,
        0,
    );
}

#[test]
fn the_today_option_is_set_from_the_clock() {
    // The date itself is untestable from outside, so what is asserted is its shape: the
    // library's `<today>` placeholder is gone and an English date has taken its place.
    let result = run(&[&fixture("today.tex")]);
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 0);
    let text = result.stdout;
    assert!(
        text.starts_with("Written on ") && text.ends_with(".\n"),
        "{text:?}"
    );
    assert!(
        !text.contains("<today>"),
        "the placeholder survived: {text:?}"
    );
    let date = text
        .trim_start_matches("Written on ")
        .trim_end_matches(".\n");
    let (month, rest) = date.split_once(' ').expect("a month name: {date:?}");
    assert!(month.chars().all(|c| c.is_ascii_alphabetic()), "{date:?}");
    let (day, year) = rest.split_once(", ").expect("a day and a year");
    assert!(matches!(day.parse::<u32>(), Ok(1..=31)), "{date:?}");
    assert!(matches!(year.parse::<i32>(), Ok(2020..=2200)), "{date:?}");
}

// ------------------------------------------------------ diagnostics and exit codes

#[test]
fn a_clean_document_says_nothing_and_exits_zero() {
    assert_run(&[&fixture("greeting.tex")], "Hello brave world.\n", "", 0);
}

#[test]
fn warnings_alone_exit_zero() {
    // The subtlety worth pinning: the exit code asks whether the diagnostics contain
    // *errors*, not whether they contain anything. A document that only earns warnings —
    // an unknown macro here, but equally techy's `descent-limit-approaching` when a run
    // comes close to the guard's limit without crossing it — converted fine, and a
    // script that treats it as a failure would be wrong.
    let result = run(&[&fixture("unknown.tex")]);
    assert_eq!(result.stdout, "a b\n");
    assert!(
        result.stderr.starts_with("warning: "),
        "{:?}",
        result.stderr
    );
    assert!(!result.stderr.contains("error:"), "{:?}", result.stderr);
    assert_eq!(result.code, 0);
}

#[test]
fn notes_alone_exit_zero_and_are_printed_only_under_v() {
    // An unresolved `\input` is a note (PLAN.md §10.6), which is below the default
    // reporting threshold: silent by default, visible under `-v`, and never a failure.
    let document = "before \\input{nowhere.tex} after";
    let quiet_by_default = run_with_stdin(&[], document);
    assert_eq!(quiet_by_default.stdout, "before after\n");
    assert_eq!(quiet_by_default.stderr, "");
    assert_eq!(quiet_by_default.code, 0);

    let verbose = run_with_stdin(&["-v"], document);
    assert_eq!(verbose.stdout, "before after\n");
    assert!(verbose.stderr.starts_with("note: "), "{:?}", verbose.stderr);
    assert_eq!(verbose.code, 0);
}

#[test]
fn an_error_diagnostic_exits_one_with_the_text_still_written() {
    // PLAN.md §13's exit code 1: the document converted — the text is there, and the
    // stray brace is in it, because tolerant recovery keeps what it cannot parse — but
    // something in it was wrong.
    let result = run(&[&fixture("stray-brace.tex")]);
    assert_eq!(result.stdout, "a } b\n");
    assert!(result.stderr.starts_with("error: "), "{:?}", result.stderr);
    assert_eq!(result.code, 1);
}

#[test]
fn strict_refuses_the_document_outright_and_exits_two() {
    // Under `--strict` the same document has no output at all: the parse aborts, so
    // there is nothing to lay out. That is the difference between exit 1 and exit 2.
    let result = run(&[&fixture("stray-brace.tex"), "--strict"]);
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.starts_with("techxt: error: "),
        "{:?}",
        result.stderr
    );
    assert!(
        result.stderr.contains("no group is open"),
        "the report says what went wrong: {:?}",
        result.stderr
    );
    assert_eq!(result.code, 2);
}

#[test]
fn strict_refuses_an_unknown_macro() {
    // DECISIONS.md D10: under strict recovery techxt registers no catch-all, so a
    // command no definition claims cannot be shaped into an invocation at all.
    let result = run(&[&fixture("unknown.tex"), "--strict"]);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("nosuchmacro"), "{:?}", result.stderr);
    assert_eq!(result.code, 2);
}

#[test]
fn quiet_prints_nothing_but_still_reports_the_exit_code() {
    // `-q` is about the report, not about the outcome: the same run keeps its exit code.
    let result = run(&[&fixture("stray-brace.tex"), "-q"]);
    assert_eq!(result.stdout, "a } b\n");
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 1);

    let warned = run(&[&fixture("unknown.tex"), "-q"]);
    assert_eq!(warned.stdout, "a b\n");
    assert_eq!(warned.stderr, "");
    assert_eq!(warned.code, 0);
}

#[test]
fn quiet_does_not_silence_the_reason_there_is_no_output() {
    // Exit code 2 with an empty standard error would be a program that failed without
    // saying why. `-q` suppresses the *document's* diagnostics; it does not suppress the
    // one line explaining why nothing was produced.
    let result = run(&[&fixture("stray-brace.tex"), "--strict", "-q"]);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.starts_with("techxt: "), "{:?}", result.stderr);
    assert_eq!(result.code, 2);
}

#[test]
fn verbose_adds_notes_to_the_warnings_and_errors() {
    // One document with all three severities, so the three thresholds are visibly
    // different rather than three separate documents that happen to agree.
    let document = "a } b \\nosuchmacro c \\input{nowhere.tex} d";

    let quiet = run_with_stdin(&["-q"], document);
    assert_eq!(quiet.stderr, "");

    let default = run_with_stdin(&[], document);
    assert!(default.stderr.contains("error: "), "{:?}", default.stderr);
    assert!(default.stderr.contains("warning: "), "{:?}", default.stderr);
    assert!(
        !default.stderr.contains("note: "),
        "the default threshold is warnings and above: {:?}",
        default.stderr
    );

    let verbose = run_with_stdin(&["-v"], document);
    assert!(verbose.stderr.contains("error: "), "{:?}", verbose.stderr);
    assert!(verbose.stderr.contains("warning: "), "{:?}", verbose.stderr);
    assert!(verbose.stderr.contains("note: "), "{:?}", verbose.stderr);

    // Every one of them converted the same document to the same text.
    assert_eq!(quiet.stdout, default.stdout);
    assert_eq!(default.stdout, verbose.stdout);
    assert_eq!(quiet.code, 1);
    assert_eq!(default.code, 1);
    assert_eq!(verbose.code, 1);
}

// ------------------------------------------------------------------- the command itself

#[test]
fn help_and_version_are_answered_without_reading_anything() {
    let help = run(&["--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("--math-mode"), "{}", help.stdout);
    assert!(help.stdout.contains("--input-dir"), "{}", help.stdout);
    assert!(help.stderr.is_empty());

    let version = run(&["--version"]);
    assert_eq!(version.code, 0);
    assert!(version.stdout.starts_with("techxt "), "{}", version.stdout);
}

#[test]
fn an_unknown_flag_value_is_a_usage_error() {
    let result = run(&["--math-mode", "shiny"]);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("shiny"), "{:?}", result.stderr);
    assert_eq!(result.code, 2);
}

#[test]
fn quiet_and_verbose_together_are_a_usage_error() {
    let result = run(&["-q", "-v"]);
    assert_eq!(result.code, 2);
    assert!(result.stdout.is_empty());
}

#[test]
fn a_document_full_of_diagnostics_is_reported_within_the_retention_cap() {
    // techy caps a diagnostics collection at a thousand retained items. What matters
    // here is that the cap is a *reporting* limit and nothing else: the text is still
    // complete, the exit code still reflects the severities, nothing panics on the way,
    // and the surplus is *counted* rather than forgotten — the report ends by saying how
    // many diagnostics it is not showing.
    let document = (0..1500)
        .map(|index| format!("\\nosuchmacro{index} w{index} "))
        .collect::<String>();
    let result = run_with_stdin(&[], &document);
    assert_eq!(
        result.stdout.matches('w').count(),
        1500,
        "every word survived"
    );
    assert_eq!(result.stderr.matches("warning: ").count(), 1000);
    assert!(
        result
            .stderr
            .contains("… and 500 further diagnostics were not recorded"),
        "the surplus went unreported: {}",
        result.stderr
    );
    assert_eq!(result.code, 0, "a thousand warnings are still not an error");
}

#[test]
fn an_empty_document_converts_to_nothing() {
    // The degenerate input, which must be neither a panic nor a diagnostic.
    let result = run_with_stdin(&[], "");
    assert_eq!(result.stdout, "");
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 0);
}
