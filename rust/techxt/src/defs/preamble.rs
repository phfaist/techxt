//! Preamble declarations, and the `document` environment that ends the
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
//! # The defining commands are honoured
//!
//! `\newcommand`, `\renewcommand`, `\providecommand`, `\newenvironment`,
//! `\renewenvironment`, `\def`, `\gdef`, `\let`, `\edef`, `\xdef`,
//! `\NewDocumentCommand` and `\DeclareMathOperator` are the `DEFINERS` table, and each
//! is registered as the definer spec that actually makes the definition. A later use of
//! the defined macro is expanded by the token reader techy-xp puts under techxt's
//! parser, so
//! `\newcommand\greet[1]{Hello #1!}\greet{world}` converts to `Hello world!` — the
//! definition itself still renders as nothing, which is what a definer contributes to a
//! document's text.
//!
//! ```
//! # use techxt::Converter;
//! let converter = Converter::standard();
//! assert_eq!(
//!     converter.latex_to_text(r"\newcommand\greet[1]{Hello #1!}\greet{world}")?.text,
//!     "Hello world!\n",
//! );
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! Each definer parses its invocation the way *it* defines, not the way this module's
//! own table declares: the name form (`{\x}` or `\x`), the parameter declaration
//! (LaTeX's `[n][default]`, TeX's parameter text, xparse's argument codes) and the
//! bodies are all read by the definer. `\def\x#1,#2.{…}`, delimiters and all, therefore
//! works, and so does the unbraced `\newcommand\x{…}` that the old chars-group
//! declaration could not read.
//!
//! ## Definitions are scoped as TeX scopes them
//!
//! `\def` writes into the **local** scope, so it reverts with the enclosing group —
//! `{\def\x{a}}\x` leaves `\x` undefined again — and an environment body is such a
//! group. `\gdef` (and `\xdef`) writes into the **global** scope and escapes every group
//! it is written in, `\begin{center}\gdef\x{a}\end{center}\x` included. The two scopes
//! sit above every one of techxt's own categories, which is what lets
//! `\renewcommand{\emph}[1]{<<#1>>}` shadow the library's `\emph`.
//!
//! One limitation is techy-xp's and worth knowing: a definition written inside a
//! **parsed argument** — `\textbf{\newcommand\x{a}}` — reaches nothing, and is not
//! diagnosed.
//!
//! ## Every definer here redefines unconditionally
//!
//! LaTeX's family checks the name first: `\newcommand` refuses a name that already
//! exists, `\renewcommand` refuses one that does not, and `\providecommand` defines only
//! a new one. techy-xp implements all three, consulting the **whole scope stack** —
//! which is what LaTeX means, and which techxt cannot use, because techxt's stack
//! answers *yes* for every name there is.
//!
//! The bottom of that stack is the unknown-command catch-all: the
//! provider that gives an unclaimed `\foo` a zero-argument spec so that it reaches
//! [`Options::unknown_macro`](crate::Options::unknown_macro) rather than techy's error
//! recovery. It answers a *lookup*, so it answers an existence check too. With it
//! registered — the default under tolerant recovery — "already defined" is true of
//! `\greet`, of `\qqq`, of every name a document could invent, so `\newcommand` would
//! refuse every definition ever written and `\providecommand` would make none.
//!
//! So each definer is registered with
//! [`RedefinitionRule::Always`](techy_xp::define::RedefinitionRule::Always). The cost is
//! `techy-xp.define.definition-already-exists` and its sibling
//! `…definition-does-not-exist`, which techxt never reports; the alternative is a feature
//! that silently does nothing whenever
//! [`unknown_macro_resolution`](crate::ConverterBuilder::unknown_macro_resolution) leaves
//! the catch-all on, and that answers differently when it does not. A converter that
//! expands what the author wrote is worth more than a diagnostic about a name clash the
//! author cannot see, and making the rule unconditional keeps the two knobs independent.
//!
//! ## `\DeclareMathOperator` is techxt's own definer
//!
//! techy-xp ships no definer for amsmath's operator declaration, so techxt does.
//! `\DeclareMathOperator{\Tr}{Tr}` is `\newcommand{\Tr}{\operatorname{Tr}}` by another
//! name, and the starred form is `\operatorname*`: `MathOperatorDefiner` reads the name
//! and the body the way `\newcommand` does and builds the same token-time macro, with
//! the body wrapped in `\operatorname{…}` on the way.
//!
//! ```
//! # use techxt::Converter;
//! let converter = Converter::standard();
//! assert_eq!(
//!     converter.latex_to_text(r"\DeclareMathOperator*{\argmax}{arg\,max} $\argmax_x f(x)$")?.text,
//!     "arg maxₓ 𝑓(𝑥)\n",
//! );
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! ## What stays declared-only
//!
//! Everything that would need TeX's conditionals, category codes, registers or counters
//! — `\newtheorem`, `\setlength`, `\newcounter` and their relatives — is consumed here
//! and acts on nothing.
//!
//! ## Turning it off
//!
//! [`ConverterBuilder::macro_definitions`](crate::ConverterBuilder::macro_definitions)
//! takes [`MacroDefinitions::Declared`](crate::convert::MacroDefinitions::Declared), and
//! every entry of that table then falls back to the argument-consuming declaration
//! written beside it — the shapes below, which are what techxt 0.1.0 registered.
//!
//! # A declared definition's name and body are read as characters
//!
//! This is the fallback shape's business, and the reason it is written the way it is.
//! What a definition *defines* is not document content, and parsing it as if it were is
//! how a preamble ends up full of errors about text nobody will ever read:
//!
//! - `\renewcommand{\vec}[1]{\mathbf{#1}}` — the argument `{\vec}` would be parsed as
//!   markup, and `\vec` is a macro techxt knows *and* one that takes an argument, so
//!   the parse fails on an argument that was never meant to be there. Every physics
//!   preamble redefining `\ket`, `\bra` or `\vec` hit this.
//! - `\newenvironment{myenv}{\begin{center}}{\end{center}}` — the two halves of an
//!   environment definition are deliberately unbalanced, and each is its own argument,
//!   so parsing them as markup reports an unterminated `center` and an orphan `\end`.
//!
//! So the *command name* of `\newcommand` and its relatives, and the *body* of every
//! definition, are read with techy's chars-group parser: commands and specials off, the
//! characters staged and dropped. Nothing is lost — none of it is ever rendered — and a
//! definition can say anything it likes.
//!
//! The one cost is that such an argument must be **braced**: the chars-group parser has
//! no single-token fallback, so `\newcommand\x{…}` (a spelling LaTeX tolerates but does
//! not document) reports a missing argument where `\newcommand{\x}{…}` converts
//! silently. `\def\x{…}`, which is *only* written unbraced, therefore keeps the
//! ordinary parser for its name. Neither restriction applies once the definers are on.

use alloc::sync::Arc;

use techy::core::constructs::CharsGroupArgumentParser;
use techy::core::specs::{ArgumentSpec, CallableSpec};
use techy::latexlike::LatexlikeGroupType;

use crate::def::{
    CallableSpecSource, Category, Definer, EnvDef, MacroDef, ParseLang, SpecBuildCx, TextRule,
};

/// The preamble category.
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
pub fn category<L: ParseLang>() -> Category<L> {
    let mut category = Category::new("preamble");

    // The document itself: its body is the text, and `\begin{document}` is where the
    // declarations above stop and it starts.
    category.add_env(EnvDef::new("document").rule(TextRule::Content));

    // The definers first, so that a name appearing in both tables would be shadowed by
    // the declaration rather than the other way round. None does today, and the order
    // says which one is meant to win if one ever did.
    for (name, definer, codes) in DEFINERS {
        category.add_macro(declared(
            MacroDef::new(*name).spec(Definers(*definer)),
            codes,
        ));
    }

    for (name, codes) in DECLARATIONS {
        category.add_macro(declared(MacroDef::new(*name), codes));
    }

    category
}

/// Apply an entry's argument declaration and its `Skip` rule.
fn declared<L: ParseLang>(definition: MacroDef<L>, codes: &[(&str, &str)]) -> MacroDef<L> {
    let mut definition = definition;
    for (code, argument) in codes {
        definition = match *code {
            RAW => definition.arg_spec(raw_group(argument)),
            code => definition.arg(code, argument),
        };
    }
    definition.rule(TextRule::Skip)
}

/// The argument code of this module's own: a braced group read as **characters**.
///
/// Not one of techy's codes, because techy's chars-group parser is only reachable as a
/// spec (there is no letter for it), and not a code techxt invents in general: it is
/// spelled out here and translated in [`category`] above.
const RAW: &str = "raw";

/// A braced group whose contents are read as characters: commands and specials off,
/// nested groups still delimiting, everything staged and then dropped.
///
/// See the module documentation for why a definition's name and body are read this way
/// — and for the one thing it costs.
fn raw_group<L: ParseLang>(name: &str) -> ArgumentSpec<L> {
    ArgumentSpec::new(CharsGroupArgumentParser::new(L::GroupTypeId::content_group()), name)
}

/// Registers the definer spec the parse language hands over for one defining command,
/// or nothing when the language declines it ([`ParseLang::definer_spec`]).
///
/// Declining is what makes
/// [`ConverterBuilder::macro_definitions`](crate::ConverterBuilder::macro_definitions) a
/// runtime switch rather than two libraries: with the source silent, the entry registers
/// exactly the argument-consuming declaration written beside it in [`DEFINERS`], which is
/// what techxt 0.1.0 registered — and what every language that honours no definer
/// registers too.
#[derive(Debug)]
struct Definers(Definer);

impl<L: ParseLang> CallableSpecSource<L> for Definers {
    fn callable_spec(&self, cx: &SpecBuildCx<'_, L>) -> Option<Arc<dyn CallableSpec<L>>> {
        L::definer_spec(self.0, cx)
    }
}

/// One defining command: its name, the definer it hands over, and the shape it declares
/// when the definers are **off**.
///
/// The declared shape is never what parses an honoured definition — the definer spec
/// reads its own name form, parameter declaration and bodies, which is why
/// `\newcommand\x{…}` (unbraced) works now and did not before. It is what parses the
/// invocation under
/// [`MacroDefinitions::Declared`](crate::convert::MacroDefinitions::Declared), and it is
/// still what names the arguments for a template or a handler.
type Definition = (
    &'static str,
    Definer,
    &'static [(&'static str, &'static str)],
);

/// The defining commands, with the shape each falls back to when they are only declared.
///
/// `\newcommand`, `\renewcommand`, `\providecommand`, the two environment definers, `\def`
/// and `\DeclareMathOperator` are the ones techxt always declared, at their original
/// shapes; the others are names techxt never declared at all before the definers
/// arrived, and their fallback shapes are this table's own. `\let` declares
/// **nothing**: `\let\a=\b` and
/// `\let\a\b` are both written, no argument code reads the optional `=`, and a shape that
/// read one argument would leave the other half in the document's text.
static DEFINERS: &[Definition] = &[
    // `s m o o m`: `\newcommand*{\cmd}[3][default]{body}`.
    (
        "newcommand",
        Definer::NewCommand,
        &[
            ("s", "star"),
            (RAW, "command"),
            ("o", "count"),
            ("o", "default"),
            (RAW, "body"),
        ],
    ),
    (
        "renewcommand",
        Definer::RenewCommand,
        &[
            ("s", "star"),
            (RAW, "command"),
            ("o", "count"),
            ("o", "default"),
            (RAW, "body"),
        ],
    ),
    (
        "providecommand",
        Definer::ProvideCommand,
        &[
            ("s", "star"),
            (RAW, "command"),
            ("o", "count"),
            ("o", "default"),
            (RAW, "body"),
        ],
    ),
    (
        "newenvironment",
        Definer::NewEnvironment,
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "count"),
            ("o", "default"),
            (RAW, "begin"),
            (RAW, "end"),
        ],
    ),
    (
        "renewenvironment",
        Definer::RenewEnvironment,
        &[
            ("s", "star"),
            ("m", "name"),
            ("o", "count"),
            ("o", "default"),
            (RAW, "begin"),
            (RAW, "end"),
        ],
    ),
    // `\def` in its simplest shape only — see the module documentation. Its name is
    // written unbraced, always, so it keeps the ordinary parser; its body does not.
    ("def", Definer::Def, &[("m", "command"), (RAW, "body")]),
    ("gdef", Definer::Gdef, &[("m", "command"), (RAW, "body")]),
    ("edef", Definer::Edef, &[("m", "command"), (RAW, "body")]),
    ("xdef", Definer::Xdef, &[("m", "command"), (RAW, "body")]),
    ("let", Definer::Let, &[]),
    (
        "NewDocumentCommand",
        Definer::NewDocumentCommand,
        &[("m", "command"), (RAW, "codes"), (RAW, "body")],
    ),
    // `s m m`: `\DeclareMathOperator*{\cmd}{name}`, amsmath's.
    (
        "DeclareMathOperator",
        Definer::DeclareMathOperator,
        &[("s", "star"), (RAW, "command"), ("m", "name")],
    ),
];

/// Every preamble declaration, with its argument shape.
///
/// Written out rather than derived, because the shapes genuinely differ and getting one
/// wrong is exactly the leak this category exists to prevent.
type Declaration = (&'static str, &'static [(&'static str, &'static str)]);

/// The preamble declarations techxt knows.
static DECLARATIONS: &[Declaration] = &[
    ("documentclass", &[("o", "options"), ("m", "class")]),
    ("usepackage", &[("o", "options"), ("m", "packages")]),
    ("RequirePackage", &[("o", "options"), ("m", "packages")]),
    // The defining commands used to be here; they are in [`DEFINERS`] now.
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
    // amsmath and amsthm declarations: a counter's parent, the shape the next
    // `\newtheorem` takes. (`\DeclareMathOperator` is in [`DEFINERS`].)
    ("numberwithin", &[("m", "counter"), ("m", "within")]),
    ("theoremstyle", &[("m", "style")]),
    ("allowdisplaybreaks", &[("o", "strength")]),
    // `\includeonly{a,b}` selects which `\include`d files are read; techxt reads what
    // its resolver hands it (see [`defs::inputs`](super::inputs)).
    ("includeonly", &[("m", "files")]),
];
