//! The command line: what `techxt --help` prints, and what it means (PLAN.md §13).
//!
//! Every flag here maps onto exactly one `techxt::Options` field or one builder setting,
//! and the mapping is the only thing this module does. The enumerated values are
//! restated as local `ValueEnum` types rather than derived on the library's own enums,
//! for two reasons: the library must not depend on clap, and two of the library enums
//! (`MathWrapDelims`, `FontStyle`) carry variants with payloads that no command line can
//! spell.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use techxt::convert::{
    FootnoteStyle, HeadingStyle, MathMode, MathWrapDelims, MatrixDelims, Options,
    UnknownMacroPolicy,
};

// `about` and `version` take their text from the manifest (`CARGO_PKG_DESCRIPTION`,
// `CARGO_PKG_VERSION`) so that the help banner cannot drift from what was built. clap
// derives the rest of the help text from the doc comments below, which is why
// implementation notes go in ordinary `//` comments like this one.
/// Convert LaTeX-like markup to plain (unicode) text.
///
/// Reads FILE, or standard input when no file is given, and writes the converted text to
/// standard output or to the file named by --output. Diagnostics go to standard error.
///
/// Exit codes: 0 when the document converted cleanly, 1 when it converted but something
/// in it was wrong, 2 when it could not be converted at all or a file could not be read
/// or written.
#[derive(Debug, Parser)]
#[command(name = "techxt", version, about)]
pub struct Cli {
    /// The document to convert. Reads standard input when absent.
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Write the converted text here instead of to standard output.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// How formulas are rendered.
    #[arg(long, value_enum, default_value_t = MathModeArg::Fancy, value_name = "MODE")]
    pub math_mode: MathModeArg,

    /// What a formula whose extent would be unclear is wrapped in.
    #[arg(long, value_enum, default_value_t = MathWrapArg::Parens, value_name = "DELIMS")]
    pub math_wrap: MathWrapArg,

    /// How the delimiters of a display matrix are drawn.
    #[arg(long, value_enum, default_value_t = MatrixDelimsArg::Unicode, value_name = "STYLE")]
    pub matrix_delims: MatrixDelimsArg,

    /// Wrap the text at this column. Off by default: lines are as long as their
    /// paragraphs.
    #[arg(short = 'w', long = "wrap", value_name = "COLS")]
    pub wrap: Option<usize>,

    /// Keep the document's comments, each on a line of its own.
    #[arg(long)]
    pub keep_comments: bool,

    /// How headings are rendered.
    #[arg(long, value_enum, default_value_t = HeadingStyleArg::NumberedUnderlined, value_name = "STYLE")]
    pub heading_style: HeadingStyleArg,

    /// Where a footnote's text goes.
    #[arg(long, value_enum, default_value_t = FootnoteStyleArg::Collected, value_name = "STYLE")]
    pub footnote_style: FootnoteStyleArg,

    /// What to do with a macro no rule renders.
    #[arg(long, value_enum, default_value_t = UnknownMacroArg::Skip, value_name = "POLICY")]
    pub unknown_macro: UnknownMacroArg,

    /// Resolve \input and \include against this directory, and refuse anything outside
    /// it. Without it, an \input is reported and rendered as nothing.
    #[arg(long, value_name = "DIR")]
    pub input_dir: Option<PathBuf>,

    /// Refuse a malformed document instead of converting it as best as possible.
    #[arg(long)]
    pub strict: bool,

    // -q and -v are the two directions from the default verbosity, so asking for both at
    // once is a contradiction rather than a precedence puzzle: clap rejects it.
    /// Print no diagnostics at all. The exit code still reports them.
    #[arg(short = 'q', conflicts_with = "verbose")]
    pub quiet: bool,

    /// Print notes as well as warnings and errors.
    #[arg(short = 'v')]
    pub verbose: bool,
}

impl Cli {
    /// Assemble the rendering options these arguments ask for.
    ///
    /// `today` is passed in rather than read here so that the caller owns the one impure
    /// step; [`crate::today::today`] is what it normally is.
    pub fn options(&self, today: String) -> Options {
        // Start from the library's defaults and overwrite, rather than building a struct
        // literal: `Options` is `#[non_exhaustive]`, so a literal is not available from
        // outside the library — and this way an option the library gains later keeps its
        // own default here until a flag for it is added, which is what a command line
        // with a fixed flag list should do.
        let mut options = Options::default();
        options.math_mode = self.math_mode.into();
        options.math_expression_in = self.math_wrap.into();
        options.matrix_delimiters = self.matrix_delims.into();
        options.wrap_width = self.wrap;
        options.keep_comments = self.keep_comments;
        options.heading_style = self.heading_style.into();
        options.footnote_style = self.footnote_style.into();
        options.unknown_macro = self.unknown_macro.into();
        // PLAN.md §13: the CLI is the caller that knows what day it is, so `\today`
        // renders a date here rather than the library's `<today>` placeholder.
        options.today = Some(today.into_boxed_str());
        options
    }

    /// Which diagnostics reach standard error.
    pub fn verbosity(&self) -> Verbosity {
        match (self.quiet, self.verbose) {
            (true, _) => Verbosity::Quiet,
            (_, true) => Verbosity::Verbose,
            _ => Verbosity::Default,
        }
    }
}

/// Which diagnostics are printed (PLAN.md §13's `-q` / default / `-v`).
///
/// This is about *reporting* only. Whatever is printed, every diagnostic still counts
/// towards the exit code: `-q` silences the report, not the failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verbosity {
    /// `-q`: nothing at all.
    Quiet,
    /// The default: warnings and errors.
    Default,
    /// `-v`: notes too.
    Verbose,
}

/// `--math-mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MathModeArg {
    /// Unicode symbols, scripts and alphabets, spaced like a typesetter would.
    Fancy,
    /// The same symbols, concatenated without the typesetter's spacing.
    Plain,
    /// The formula's LaTeX source, left as it was written.
    Source,
}

impl From<MathModeArg> for MathMode {
    fn from(arg: MathModeArg) -> MathMode {
        match arg {
            MathModeArg::Fancy => MathMode::Fancy,
            MathModeArg::Plain => MathMode::Plain,
            MathModeArg::Source => MathMode::Source,
        }
    }
}

/// `--math-wrap`.
///
/// `MathWrapDelims::Custom` is deliberately unreachable from the command line: a pair of
/// arbitrary strings needs a syntax to separate them, and a knob nobody asked for is not
/// worth one. Embedders have the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MathWrapArg {
    /// `(a+b)/2`.
    Parens,
    /// `{a+b}/2`.
    Braces,
    /// `a+b/2` — nothing at all, even where the extent is then unclear.
    None,
}

impl From<MathWrapArg> for MathWrapDelims {
    fn from(arg: MathWrapArg) -> MathWrapDelims {
        match arg {
            MathWrapArg::Parens => MathWrapDelims::Parens,
            MathWrapArg::Braces => MathWrapDelims::Braces,
            MathWrapArg::None => MathWrapDelims::None,
        }
    }
}

/// `--matrix-delims`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MatrixDelimsArg {
    /// Multi-line box-drawing brackets, assembled from the unicode extension pieces.
    Unicode,
    /// One-character ASCII brackets on every row.
    Ascii,
}

impl From<MatrixDelimsArg> for MatrixDelims {
    fn from(arg: MatrixDelimsArg) -> MatrixDelims {
        match arg {
            MatrixDelimsArg::Unicode => MatrixDelims::Unicode,
            MatrixDelimsArg::Ascii => MatrixDelims::Ascii,
        }
    }
}

/// `--heading-style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HeadingStyleArg {
    /// A section number, the title, and a rule of underline characters.
    NumberedUnderlined,
    /// The title and its underline, without a number.
    Underlined,
    /// The number and the title on one line, with no underline.
    Prefix,
    /// The title alone.
    Plain,
}

impl From<HeadingStyleArg> for HeadingStyle {
    fn from(arg: HeadingStyleArg) -> HeadingStyle {
        match arg {
            HeadingStyleArg::NumberedUnderlined => HeadingStyle::NumberedUnderlined,
            HeadingStyleArg::Underlined => HeadingStyle::Underlined,
            HeadingStyleArg::Prefix => HeadingStyle::Prefix,
            HeadingStyleArg::Plain => HeadingStyle::Plain,
        }
    }
}

/// `--footnote-style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FootnoteStyleArg {
    /// A marker in the text, and the notes gathered into a block at the end.
    Collected,
    /// The note's text where the footnote was.
    Inline,
    /// Neither marker nor text.
    Skip,
}

impl From<FootnoteStyleArg> for FootnoteStyle {
    fn from(arg: FootnoteStyleArg) -> FootnoteStyle {
        match arg {
            FootnoteStyleArg::Collected => FootnoteStyle::Collected,
            FootnoteStyleArg::Inline => FootnoteStyle::Inline,
            FootnoteStyleArg::Skip => FootnoteStyle::Skip,
        }
    }
}

/// `--unknown-macro`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum UnknownMacroArg {
    /// Render nothing.
    Skip,
    /// Render the macro's arguments as if the macro were transparent.
    RenderArgs,
    /// Emit the macro's LaTeX source.
    KeepSource,
    /// Emit `<name>`.
    Placeholder,
}

impl From<UnknownMacroArg> for UnknownMacroPolicy {
    fn from(arg: UnknownMacroArg) -> UnknownMacroPolicy {
        match arg {
            UnknownMacroArg::Skip => UnknownMacroPolicy::Skip,
            UnknownMacroArg::RenderArgs => UnknownMacroPolicy::RenderArgs,
            UnknownMacroArg::KeepSource => UnknownMacroPolicy::KeepSource,
            UnknownMacroArg::Placeholder => UnknownMacroPolicy::Placeholder,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_definition_is_well_formed() {
        // clap's own consistency check (duplicate names, impossible defaults, ...). It
        // panics on a mistake, which is why it belongs in a test rather than in `main`.
        Cli::command().debug_assert();
    }

    #[test]
    fn the_defaults_are_the_librarys_defaults() {
        // PLAN.md §13 states the default of every flag, and every one of them is the
        // library's own default. Stating them twice (here and in the library) is only
        // safe if a test says so.
        let defaults = Options::default();
        let cli = Cli::parse_from(["techxt"]).options("May 1, 2001".into());
        assert_eq!(cli.math_mode, defaults.math_mode);
        assert_eq!(cli.math_expression_in, defaults.math_expression_in);
        assert_eq!(cli.matrix_delimiters, defaults.matrix_delimiters);
        assert_eq!(cli.wrap_width, defaults.wrap_width);
        assert_eq!(cli.keep_comments, defaults.keep_comments);
        assert_eq!(cli.heading_style, defaults.heading_style);
        assert_eq!(cli.footnote_style, defaults.footnote_style);
        assert_eq!(cli.unknown_macro, defaults.unknown_macro);
        // The one deliberate difference: the library has no clock, the CLI does.
        assert_eq!(defaults.today, None);
        assert!(cli.today.is_some());
    }

    #[test]
    fn every_flag_reaches_its_option() {
        let cli = Cli::parse_from([
            "techxt",
            "--math-mode",
            "plain",
            "--math-wrap",
            "braces",
            "--matrix-delims",
            "ascii",
            "--wrap",
            "40",
            "--keep-comments",
            "--heading-style",
            "prefix",
            "--footnote-style",
            "inline",
            "--unknown-macro",
            "keep-source",
        ]);
        let options = cli.options("May 1, 2001".into());
        assert_eq!(options.math_mode, MathMode::Plain);
        assert_eq!(options.math_expression_in, MathWrapDelims::Braces);
        assert_eq!(options.matrix_delimiters, MatrixDelims::Ascii);
        assert_eq!(options.wrap_width, Some(40));
        assert!(options.keep_comments);
        assert_eq!(options.heading_style, HeadingStyle::Prefix);
        assert_eq!(options.footnote_style, FootnoteStyle::Inline);
        assert_eq!(options.unknown_macro, UnknownMacroPolicy::KeepSource);
        assert_eq!(options.today.as_deref(), Some("May 1, 2001"));
    }

    #[test]
    fn verbosity_follows_the_two_flags() {
        assert_eq!(Cli::parse_from(["techxt"]).verbosity(), Verbosity::Default);
        assert_eq!(
            Cli::parse_from(["techxt", "-q"]).verbosity(),
            Verbosity::Quiet
        );
        assert_eq!(
            Cli::parse_from(["techxt", "-v"]).verbosity(),
            Verbosity::Verbose
        );
        // Asking for both is a usage error, not a silent precedence rule.
        assert!(Cli::try_parse_from(["techxt", "-q", "-v"]).is_err());
    }

    #[test]
    fn the_enumerated_values_are_spelled_as_the_plan_writes_them() {
        // PLAN.md §13 fixes the value spellings; clap's kebab-casing produces them, but
        // a rename of a variant would silently change the command line without this.
        let parse = |flag: &str, value: &str| {
            Cli::try_parse_from(["techxt", flag, value])
                .map(|_| ())
                .map_err(|error| error.to_string())
        };
        for value in ["fancy", "plain", "source"] {
            assert!(parse("--math-mode", value).is_ok(), "{value}");
        }
        for value in ["parens", "braces", "none"] {
            assert!(parse("--math-wrap", value).is_ok(), "{value}");
        }
        for value in ["unicode", "ascii"] {
            assert!(parse("--matrix-delims", value).is_ok(), "{value}");
        }
        for value in ["numbered-underlined", "underlined", "prefix", "plain"] {
            assert!(parse("--heading-style", value).is_ok(), "{value}");
        }
        for value in ["collected", "inline", "skip"] {
            assert!(parse("--footnote-style", value).is_ok(), "{value}");
        }
        for value in ["skip", "render-args", "keep-source", "placeholder"] {
            assert!(parse("--unknown-macro", value).is_ok(), "{value}");
        }
        assert!(parse("--math-mode", "shiny").is_err());
    }
}
