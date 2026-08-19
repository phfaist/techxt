//! `--input-dir`: `\input` end to end, and the sandbox around it (PLAN.md §13, §14.3).
//!
//! The interesting cases are all about *paths*, so the documents are generated into a
//! throwaway tree rather than checked in: a `../` escape, an absolute path and a symlink
//! only mean anything relative to a real directory, and one of them (the symlink) cannot
//! be checked into git in a form that survives every platform anyway.
//!
//! The tree every test works in:
//!
//! ```text
//! <root>/dir/            <- the sandbox, what `--input-dir` names
//! <root>/dir/intro.tex
//! <root>/dir/noext.tex   <- reached as `\input{noext}`
//! <root>/dir/sub/deep.tex
//! <root>/dir/escape.tex  <- symlink to <root>/outside.tex
//! <root>/dir/a.tex       <- includes b.tex
//! <root>/dir/b.tex       <- includes a.tex, closing the cycle
//! <root>/dir-evil/secret.tex   <- the sibling whose name starts with "dir"
//! <root>/outside.tex
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What one run of the binary did.
struct Run {
    /// The exit code.
    code: i32,
    /// The converted text.
    stdout: String,
    /// The diagnostics report, or the failure message.
    stderr: String,
}

/// Run the binary with `args`, feeding `document` on standard input.
fn run(args: &[&str], document: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_techxt"))
        .args(args)
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
    Run {
        code: output
            .status
            .code()
            .expect("the child exited rather than being signalled"),
        stdout: String::from_utf8(output.stdout).expect("the converted text is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("the report is UTF-8"),
    }
}

/// Build the tree described in the module documentation and answer the sandbox root.
///
/// The tree is rebuilt from scratch on every call so that a previous run's leftovers
/// cannot make a test pass. `name` keeps concurrently running tests out of each other's
/// way — cargo runs them in threads of one process, all sharing this directory.
fn sandbox(name: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push("input-dir");
    root.push(name);
    let _ = std::fs::remove_dir_all(&root);

    let dir = root.join("dir");
    std::fs::create_dir_all(dir.join("sub")).expect("the sandbox tree is created");
    std::fs::create_dir_all(root.join("dir-evil")).expect("the sibling is created");

    write(&dir.join("intro.tex"), "INTRO");
    write(&dir.join("noext.tex"), "NOEXT");
    write(&dir.join("sub/deep.tex"), "DEEP");
    write(&dir.join("a.tex"), r"A(\input{b.tex})");
    write(&dir.join("b.tex"), r"B(\input{a.tex})");
    write(&root.join("dir-evil/secret.tex"), "SECRET");
    write(&root.join("outside.tex"), "OUTSIDE");

    // A symlink out of the sandbox: the case canonicalization exists for. Only on unix,
    // where a symlink needs no privileges to create.
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("outside.tex"), dir.join("escape.tex"))
        .expect("the escaping symlink is created");

    dir
}

/// Write a file, creating nothing — every directory it needs already exists.
fn write(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap_or_else(|error| panic!("writing {path:?}: {error}"));
}

/// Convert `document` with the sandbox rooted at `dir`.
fn with_input_dir(dir: &Path, document: &str) -> Run {
    run(
        &[
            "--input-dir",
            dir.to_str().expect("UTF-8 sandbox path"),
            "-v",
        ],
        document,
    )
}

// ---------------------------------------------------------------- what is allowed

#[test]
fn an_input_inside_the_directory_is_pulled_in() {
    // DECISIONS.md D15's end-to-end path: the resolver reaches techy's `\input` spec,
    // the spec attaches the file's content, and the `inputs` rule renders the slot.
    let dir = sandbox("allowed");
    let result = with_input_dir(&dir, r"before \input{intro.tex} after");
    assert_eq!(result.stdout, "before INTRO after\n");
    assert_eq!(result.stderr, "");
    assert_eq!(result.code, 0);
}

#[test]
fn a_reference_without_an_extension_falls_back_to_tex() {
    // `\input{chapters/intro}` is how LaTeX documents are actually written.
    let dir = sandbox("extension-fallback");
    let result = with_input_dir(&dir, r"[\input{noext}]");
    assert_eq!(result.stdout, "[NOEXT]\n");
    assert_eq!(result.code, 0);
}

#[test]
fn a_subdirectory_is_inside_the_sandbox() {
    let dir = sandbox("subdirectory");
    let result = with_input_dir(&dir, r"[\input{sub/deep.tex}]");
    assert_eq!(result.stdout, "[DEEP]\n");
    assert_eq!(result.code, 0);
}

#[test]
fn a_path_that_leaves_and_returns_is_still_inside() {
    // Canonicalization is what makes this work: the reference contains `..`, but the
    // path it names does not leave the sandbox, and the rule is about the destination.
    let dir = sandbox("round-trip");
    let result = with_input_dir(&dir, r"[\input{sub/../intro.tex}]");
    assert_eq!(result.stdout, "[INTRO]\n");
    assert_eq!(result.code, 0);
}

#[test]
fn an_include_is_resolved_like_an_input() {
    let dir = sandbox("include");
    let result = with_input_dir(&dir, r"[\include{intro.tex}]");
    assert_eq!(result.stdout, "[INTRO]\n");
    assert_eq!(result.code, 0);
}

// ---------------------------------------------------------------- what is refused

/// Assert that `reference` is refused, and that the refusal says so out loud.
///
/// A refusal is not a silent nothing: the resolver's message goes to standard error and
/// the exit code is 1 (the document converted, one construct in it did not). What must
/// never appear is the content of the file the reference was reaching for.
fn assert_refused(dir: &Path, reference: &str, forbidden_content: &str) {
    let result = with_input_dir(dir, &format!(r"[\input{{{reference}}}]"));
    assert_eq!(
        result.stdout, "[]\n",
        "nothing was included for ‘{reference}’"
    );
    assert!(
        !result.stdout.contains(forbidden_content),
        "‘{reference}’ leaked {forbidden_content:?}: {:?}",
        result.stdout
    );
    assert!(
        result.stderr.contains("cannot resolve source reference"),
        "‘{reference}’ was refused out loud: {:?}",
        result.stderr
    );
    assert_eq!(result.code, 1, "the refusal is an error diagnostic");
}

#[test]
fn a_parent_directory_escape_is_refused() {
    let dir = sandbox("escape-parent");
    assert_refused(&dir, "../outside.tex", "OUTSIDE");
}

#[test]
fn a_sibling_sharing_the_directorys_name_prefix_is_refused() {
    // PLAN.md §14.3's `dir-evil` case, and the reason the containment check compares
    // against the directory *plus a separator*: `<root>/dir-evil/secret.tex` does start
    // with the string `<root>/dir`, and a bare prefix test would let it through.
    let dir = sandbox("escape-sibling");
    assert_refused(&dir, "../dir-evil/secret.tex", "SECRET");
}

#[test]
fn an_absolute_path_is_refused() {
    // `Path::join` with an absolute path throws the sandbox root away entirely, which is
    // exactly why containment is checked on the joined result rather than assumed.
    let dir = sandbox("escape-absolute");
    let outside = dir
        .parent()
        .expect("the sandbox has a parent")
        .join("outside.tex");
    assert_refused(&dir, outside.to_str().expect("UTF-8 path"), "OUTSIDE");
}

#[cfg(unix)]
#[test]
fn a_symlink_pointing_out_of_the_directory_is_refused() {
    // The file is *inside* the sandbox by every check that does not follow links. This
    // is the case `canonicalize` is here for.
    let dir = sandbox("escape-symlink");
    assert_refused(&dir, "escape.tex", "OUTSIDE");
}

#[test]
fn a_reference_to_nothing_is_refused_without_naming_a_path_that_exists() {
    let dir = sandbox("missing");
    let result = with_input_dir(&dir, r"[\input{nowhere.tex}]");
    assert_eq!(result.stdout, "[]\n");
    assert!(
        result.stderr.contains("no such file"),
        "{:?}",
        result.stderr
    );
    assert_eq!(result.code, 1);
}

#[test]
fn a_directory_is_not_a_document() {
    // `\input{sub}` names something that exists and is inside the sandbox, but is not a
    // file. Reading it would fail later and more confusingly.
    let dir = sandbox("directory-reference");
    let result = with_input_dir(&dir, r"[\input{sub}]");
    assert_eq!(result.stdout, "[]\n");
    assert!(
        result.stderr.contains("no such file"),
        "{:?}",
        result.stderr
    );
    assert_eq!(result.code, 1);
}

// ------------------------------------------------------------------ the cycle guard

#[test]
fn an_include_cycle_is_broken_rather_than_followed() {
    // techy's `check_include_chain` walks the *origin* chain of the including source, so
    // it only works because the resolver mints canonical paths as origins. Without it,
    // `a.tex` including `b.tex` including `a.tex` would recurse until the parser's
    // descent guard stopped it — much later, and with a far less useful message.
    let dir = sandbox("cycle");
    let result = with_input_dir(&dir, r"\input{a.tex}");
    // `a.tex` pulls in `b.tex`, whose own `\input{a.tex}` is already on its include
    // chain and is refused there: the text is finite.
    assert_eq!(result.stdout, "A(B())\n");
    assert!(
        result.stderr.contains("include cycle detected"),
        "{:?}",
        result.stderr
    );
    assert_eq!(result.code, 1);
}

// ------------------------------------------------------------- without the flag

#[test]
fn without_the_flag_an_input_is_a_note_and_renders_nothing() {
    // PLAN.md §9.8: with no resolver configured, techxt does not register techy's
    // inclusion spec at all, so `\input` parses as an ordinary macro whose `attached`
    // slot is missing — a note, never an error.
    let result = run(&["-v"], r"[\input{intro.tex}]");
    assert_eq!(result.stdout, "[]\n");
    assert!(result.stderr.starts_with("note: "), "{:?}", result.stderr);
    assert_eq!(result.code, 0);
}

#[test]
fn a_directory_that_does_not_exist_is_a_failure_before_anything_is_read() {
    // A mistyped `--input-dir` is a mistake in the command, so it fails as a command
    // does: exit code 2, one message, no output.
    let mut missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    missing.push("input-dir/no-such-sandbox");
    let _ = std::fs::remove_dir_all(&missing);
    let result = run(
        &["--input-dir", missing.to_str().expect("UTF-8 path")],
        "text",
    );
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.starts_with("techxt: --input-dir "),
        "{:?}",
        result.stderr
    );
    assert_eq!(result.code, 2);
}

#[test]
fn a_file_is_not_a_directory() {
    let dir = sandbox("file-as-directory");
    let file = dir.join("intro.tex");
    let result = run(&["--input-dir", file.to_str().expect("UTF-8 path")], "text");
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.starts_with("techxt: --input-dir "),
        "{:?}",
        result.stderr
    );
    assert_eq!(result.code, 2);
}
