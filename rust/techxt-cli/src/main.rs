//! The `techxt` command-line converter.
//!
//! Reads a LaTeX-like document and writes it out as plain (unicode) text, using the
//! [`techxt`] library. This is the std-linking embedder of a `no_std` library, and
//! everything the library cannot do is here: file and stream I/O, the sandboxed `\input`
//! resolver ([`resolve`]), the system clock ([`today`]) — and the stack measurement that
//! lets the descent guard use the whole thread stack instead of a conservative guess
//! (see [`descent_guard`]).
//!
//! The program is one pass: read, build a converter from the arguments, convert, write
//! the text, print the diagnostics, exit with a code that says which of the three things
//! happened (PLAN.md §13).

mod cli;
mod resolve;
mod today;

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use techxt::convert::StdDescentGuardInit;
use techxt::convert::{Diagnostics, Recovery, Severity};
use techxt::Converter;

use cli::{Cli, Verbosity};

/// Exit code 0: the document converted and nothing was wrong with it.
const EXIT_CLEAN: u8 = 0;
/// Exit code 1: the document converted, but the diagnostics contain errors.
///
/// The text has still been written — a document with an error in it is a document, and
/// the caller asked for its text. The code is how a script learns not to trust it.
const EXIT_DIAGNOSED: u8 = 1;
/// Exit code 2: there is no text. A parse the recovery policy could not survive, a file
/// that could not be read, or a file that could not be written.
const EXIT_FAILED: u8 = 2;

/// Parse the command line, convert, and report.
///
/// All the work is in [`run`]; this only turns its answer into a process exit code, and
/// keeps the one `?`-free frame where a late I/O failure (a closed pipe on the final
/// flush) can still be reported.
fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(failure) => {
            // A failure message is not a diagnostic about the document — it is the
            // program saying it could not do the job, and it is the only explanation
            // exit code 2 comes with — so `-q`, which is about what the *document* got
            // wrong, does not silence it. A stderr that cannot be written to does.
            let _ = writeln!(std::io::stderr(), "techxt: {failure}");
            ExitCode::from(EXIT_FAILED)
        }
    }
}

/// Do the conversion. `Ok` carries the exit code, `Err` the message for exit code 2.
fn run(cli: &Cli) -> Result<u8, String> {
    let input = read_input(cli.file.as_deref())?;

    let converter = build_converter(cli)?;

    let conversion = match converter.latex_to_text(&input) {
        Ok(conversion) => conversion,
        // The parse aborted, so there is no text to write. Under `--strict` this is the
        // ordinary way a malformed document ends; under the default tolerant recovery
        // only the descent guard's refusal gets here, and both deserve the same full
        // report (position, traceback, include chain) rather than a bare message.
        Err(error) => return Err(error.render()),
    };

    write_output(cli.output.as_deref(), &conversion.text)?;

    // Reported after the text is written: if the output is a pipe that has gone away,
    // the caller learns about that failure rather than about the document's commas.
    report(&conversion.diagnostics, cli.verbosity())?;

    Ok(if conversion.diagnostics.has_errors() {
        EXIT_DIAGNOSED
    } else {
        EXIT_CLEAN
    })
}

/// Assemble the converter these arguments describe.
fn build_converter(cli: &Cli) -> Result<Converter, String> {
    let mut builder = Converter::builder()
        .options(cli.options(today::today()))
        .descent_guard(descent_guard())
        .macro_definitions(cli.macro_definitions())
        .expansion_depth_limit(cli.expansion_depth_limit)
        .expansion_count_limit(cli.expansion_count_limit)
        .recovery(if cli.strict {
            Recovery::Strict
        } else {
            Recovery::Tolerant
        });

    if let Some(directory) = &cli.input_dir {
        let resolver = resolve::directory_resolver(directory)
            .map_err(|error| format!("--input-dir {}: {error}", directory.display()))?;
        builder = builder.source_resolver(resolver);
    }

    // `BuildError` is a programming error in the definitions, not a user error: the
    // standard definition set is validated by the library's own tests. Reporting it as a
    // plain failure rather than panicking keeps the promise that this program never
    // panics.
    builder
        .build()
        .map_err(|error| format!("the standard definitions failed to build: {error}"))
}

/// The descent guard this program parses under (DECISIONS.md D9 item 3).
///
/// techy's guard bounds how deeply the parser and the fold may recurse, so that a
/// document nesting a thousand groups deep is refused instead of overflowing the stack.
/// It offers a fixed byte budget, a measured one, a depth count, and off; `techxt` the
/// library ships the fixed default, because a `no_std` library has no way to ask the
/// operating system how much stack is actually left, and a bigger guess would be unsafe
/// on a small-stack thread. In an unoptimized build that default admits only on the
/// order of ten nesting levels — fine as a safety net, useless as a converter.
///
/// This program links std, so it can do the honest thing: measure. `stacker` reports the
/// stack still available on the parsing thread, the guard subtracts its own headroom and
/// spends the rest, and an ordinary document — a paper nesting environments, groups and
/// braces a few dozen deep — converts. A probe answer of `None` (a platform `stacker`
/// cannot measure) falls back to techy's fixed default, so the guard is never absent.
fn descent_guard() -> StdDescentGuardInit {
    // `remaining_stack` is already the `fn() -> Option<usize>` the constructor asks for,
    // so it is passed as itself rather than wrapped in the closure techy's example
    // writes — same function, one indirection less, and clippy's `redundant_closure`
    // insists.
    StdDescentGuardInit::computed_stack_budget(stacker::remaining_stack)
}

/// Read the document: the named file, or standard input when there is none.
///
/// The input must be UTF-8 — LaTeX-like markup that is not is a file this program has
/// nothing to say about — and a decoding failure is reported as the I/O failure it
/// effectively is.
fn read_input(file: Option<&Path>) -> Result<String, String> {
    match file {
        Some(path) => {
            std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
        }
        None => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| format!("<stdin>: {error}"))?;
            Ok(input)
        }
    }
}

/// Write the converted text to the named file, or to standard output.
///
/// Standard output is flushed explicitly: a `println!` that fails at flush time on
/// process exit would otherwise report success for text nobody received.
fn write_output(file: Option<&Path>, text: &str) -> Result<(), String> {
    match file {
        Some(path) => {
            std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
        }
        None => {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            stdout
                .write_all(text.as_bytes())
                .and_then(|()| stdout.flush())
                .map_err(|error| format!("<stdout>: {error}"))
        }
    }
}

/// Print the diagnostics this verbosity asks for, to standard error.
///
/// The severity threshold is the whole of the `-q` / default / `-v` behaviour: nothing,
/// warnings and errors, or everything. Filtering happens by rebuilding a collection and
/// handing it to techy's [`Diagnostics::render_all`], which shares one line index across
/// the whole report — rendering each diagnostic separately would re-scan the document
/// once per diagnostic.
fn report(diagnostics: &Diagnostics, verbosity: Verbosity) -> Result<(), String> {
    let threshold = match verbosity {
        Verbosity::Quiet => return Ok(()),
        Verbosity::Default => Severity::Warning,
        Verbosity::Verbose => Severity::Note,
    };

    // The retention cap is set from what is already there, so filtering can never be the
    // thing that drops a diagnostic.
    let mut shown = Diagnostics::with_limit(diagnostics.len().max(1));
    for diagnostic in diagnostics.iter() {
        if diagnostic.severity() >= threshold {
            shown.push(diagnostic.clone());
        }
    }

    let mut report = String::new();
    if !shown.is_empty() {
        report.push_str(&shown.render_all());
    }
    // A collection that hit *its* own retention cap during the conversion has already
    // forgotten the rest; say so, since the report above is then not the whole story.
    // The count survives the library's merge of its parse-side and render-side
    // collections, so what this prints is the real surplus.
    if diagnostics.suppressed() > 0 {
        if !report.is_empty() {
            report.push('\n');
        }
        report.push_str(&format!(
            "… and {} further diagnostics were not recorded (retention limit reached)",
            diagnostics.suppressed(),
        ));
    }
    if report.is_empty() {
        return Ok(());
    }

    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    writeln!(stderr, "{report}").map_err(|error| format!("<stderr>: {error}"))
}
