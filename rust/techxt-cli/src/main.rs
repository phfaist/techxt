//! The `techxt` command-line converter.
//!
//! Reads a LaTeX-like document and writes it out as plain (unicode) text, using the
//! [`techxt`] library. This is the std-linking embedder of the no_std library: file and
//! stream I/O, the sandboxed `\input` resolver and the system clock all live here.
//!
//! The full option set (output file, math mode, wrapping, heading and footnote styles,
//! unknown-macro policy, ...) arrives with a later milestone; today the program only
//! reports its version.

use clap::Parser;

/// Convert LaTeX-like markup to plain (unicode) text.
///
/// `version` makes clap serve `--version` from the crate version, so the number can
/// only come from `Cargo.toml` and cannot drift.
#[derive(Debug, Parser)]
#[command(name = "techxt", version, about)]
struct Cli {}

/// Parse the command line and run the conversion.
///
/// Argument parsing already handles `--help` and `--version` (clap exits on both), so
/// what is left for the still-unimplemented converter is to say which version would
/// have done the work.
fn main() {
    let Cli {} = Cli::parse();
    println!("techxt {}", env!("CARGO_PKG_VERSION"));
}
