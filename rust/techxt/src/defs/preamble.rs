//! Preamble declarations (PLAN.md §9.8), and the `document` environment that ends the
//! preamble: parsed so that they cannot leak, and silent.
//!
//! `\documentclass`, `\usepackage`, `\newcommand` and their relatives contribute
//! nothing to a document's text. What matters is that they are **declared**: a macro
//! techxt has never heard of takes no arguments (see
//! [`Options::unknown_macro`](crate::Options::unknown_macro)), so an undeclared
//! `\setlength{\parindent}{0pt}` would put `0pt` in the reader's first paragraph. Every
//! entry here exists to stop that happening.
//!
//! And none of them raises a diagnostic. `techxt.unknown-macro` means "techxt does not
//! know this construct"; these are known, and rendering them as nothing is the correct
//! and complete answer.
//!
//! # `document`
//!
//! `\begin{document}` … `\end{document}` is the one construct here that renders
//! something: its body, which is the document. It belongs in this category because it
//! is the other half of the preamble — everything before it is a declaration, and
//! everything inside it is the text. Without the entry techy has no definition to
//! resolve `\begin{document}` against, and reports an **error**, which would make
//! every complete LaTeX file a failed conversion however clean it is.
//!
//! # `\newcommand` is declared, not honoured
//!
//! techxt does not expand user macros: the body of a `\newcommand` is consumed and
//! dropped, and a later use of the defined macro is an unknown macro like any other.
//! Expansion would need a whole TeX mouth, and the plan defers it (PLAN.md §17).
//! `\def` is accepted in its simplest shape only — a name followed by a body — because
//! its parameter text (`\def\x#1#2{…}`) has no fixed argument structure to declare.

use crate::def::{Category, EnvDef, MacroDef, TextRule};

/// The preamble category (PLAN.md §12.1).
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::standard();
/// let converted = converter.latex_to_text(
///     "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n",
/// )?;
/// assert_eq!(converted.text, "Hello.\n");
/// assert!(!converted.diagnostics.has_errors());
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub fn category() -> Category {
    let mut category = Category::new("preamble");

    // The document itself: its body is the text, and `\begin{document}` is where the
    // declarations above stop and it starts.
    category.add_env(EnvDef::new("document").rule(TextRule::Content));

    for (name, codes) in DECLARATIONS {
        let mut definition = MacroDef::new(*name);
        for (code, argument) in *codes {
            definition = definition.arg(code, argument);
        }
        category.add_macro(definition.rule(TextRule::Skip));
    }

    category
}

/// Every preamble declaration, with its argument shape.
///
/// Written out rather than derived, because the shapes genuinely differ and getting one
/// wrong is exactly the leak this category exists to prevent.
type Declaration = (&'static str, &'static [(&'static str, &'static str)]);

/// The declarations of PLAN.md §9.8, in the order the plan lists them.
static DECLARATIONS: &[Declaration] = &[
    ("documentclass", &[("o", "options"), ("m", "class")]),
    ("usepackage", &[("o", "options"), ("m", "packages")]),
    ("RequirePackage", &[("o", "options"), ("m", "packages")]),
    // `s m o o m`: `\newcommand*{\cmd}[3][default]{body}`.
    (
        "newcommand",
        &[
            ("s", "star"),
            ("m", "command"),
            ("o", "count"),
            ("o", "default"),
            ("m", "body"),
        ],
    ),
    (
        "renewcommand",
        &[
            ("s", "star"),
            ("m", "command"),
            ("o", "count"),
            ("o", "default"),
            ("m", "body"),
        ],
    ),
    (
        "providecommand",
        &[
            ("s", "star"),
            ("m", "command"),
            ("o", "count"),
            ("o", "default"),
            ("m", "body"),
        ],
    ),
    // `\def` in its simplest shape only — see the module documentation.
    ("def", &[("m", "command"), ("m", "body")]),
    (
        "newenvironment",
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "count"),
            ("o", "default"),
            ("m", "begin"),
            ("m", "end"),
        ],
    ),
    (
        "renewenvironment",
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "count"),
            ("o", "default"),
            ("m", "begin"),
            ("m", "end"),
        ],
    ),
    // `\newtheorem{env}[shares counter with]{Title}[reset by]`.
    (
        "newtheorem",
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "counter"),
            ("m", "title"),
            ("o", "within"),
        ],
    ),
    (
        "renewtheorem",
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "counter"),
            ("m", "title"),
            ("o", "within"),
        ],
    ),
    ("setlength", &[("m", "length"), ("m", "value")]),
    ("addtolength", &[("m", "length"), ("m", "value")]),
    ("setcounter", &[("m", "counter"), ("m", "value")]),
    ("addtocounter", &[("m", "counter"), ("m", "value")]),
    ("pagestyle", &[("m", "style")]),
    ("thispagestyle", &[("m", "style")]),
    ("hypersetup", &[("m", "settings")]),
    ("graphicspath", &[("m", "paths")]),
    ("bibliographystyle", &[("m", "style")]),
    ("bibliography", &[("m", "files")]),
    // Lengths and counters are declared here and set in the body; `\setlength` above
    // is the one that is written in both places.
    ("newlength", &[("m", "length")]),
    ("settowidth", &[("m", "length"), ("m", "text")]),
    ("settoheight", &[("m", "length"), ("m", "text")]),
    ("settodepth", &[("m", "length"), ("m", "text")]),
    ("newcounter", &[("m", "counter"), ("o", "within")]),
    ("usecounter", &[("m", "counter")]),
    ("pagenumbering", &[("m", "style")]),
    // amsmath and amsthm declarations: an operator name, a counter's parent, the shape
    // the next `\newtheorem` takes.
    (
        "DeclareMathOperator",
        &[("s", "star"), ("m", "command"), ("m", "name")],
    ),
    ("numberwithin", &[("m", "counter"), ("m", "within")]),
    ("theoremstyle", &[("m", "style")]),
    ("allowdisplaybreaks", &[("o", "strength")]),
    // `\includeonly{a,b}` selects which `\include`d files are read; techxt reads what
    // its resolver hands it (see [`defs::inputs`](super::inputs)).
    ("includeonly", &[("m", "files")]),
];
