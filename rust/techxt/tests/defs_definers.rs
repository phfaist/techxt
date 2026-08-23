//! What each definer *shape* does, and what expansion looks like once it meets the
//! renderer (PLAN.md §16 M9, phase 3).
//!
//! `defs_macros.rs` pins the wiring — that definitions are honoured at all, where they
//! are scoped, what is refused, what the budgets bound. This file is the other half: the
//! matrix of shapes a document actually writes. `\newcommand*`, xparse's argument codes,
//! a definition that makes another definition, a macro that expands into an environment
//! or an `\item`, a macro in math, a macro whose body is UTF-8.
//!
//! Where the answer is surprising, the test pins **what is true** and the comment says
//! why — a definition made over a name techxt itself ships, a body that sees a later
//! definition than the one in force when it was written, `\NewDocumentCommand`'s `s`
//! binding a literal `*`. A surprise pinned is a surprise that cannot regress quietly.

use techxt::convert::MacroDefinitions;
use techxt::Converter;

/// The house helper: convert with the shipped definitions and default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// Convert, and answer the identifiers of everything that was reported.
fn diagnostics(latex: &str) -> Vec<String> {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier().to_string())
        .collect()
}

// ------------------------------------------------------------------ definer shapes

#[test]
fn a_starred_definer_is_accepted_and_its_star_ignored() {
    // LaTeX's `*` says the macro is not `\long` — that its arguments may not contain a
    // paragraph break. techxt has no such distinction to make, so the star is read and
    // dropped, and the definition is the one the unstarred form would have made.
    assert_eq!(text(r"\newcommand*{\x}[1]{[#1]}\x{a}"), "[a]\n");
    assert_eq!(text(r"\renewcommand*{\emph}[1]{<#1>}\emph{a}"), "<a>\n");
    assert_eq!(text(r"\providecommand*{\y}{Y}\y"), "Y\n");
    assert!(diagnostics(r"\newcommand*{\x}[1]{[#1]}\x{a}").is_empty());
}

#[test]
fn new_document_command_reads_xparses_argument_codes() {
    // `m` and `o`, LaTeX's mandatory and optional.
    assert_eq!(
        text(r"\NewDocumentCommand\x{m o}{[#1|#2]}\x{a}[b]"),
        "[a|b]\n"
    );

    // `O{…}`: an optional argument with a default, which is what makes it different from
    // `o` — the default stands in when the brackets are not written.
    assert_eq!(
        text(r"\NewDocumentCommand\x{O{dflt} m}{[#1|#2]}\x{a}"),
        "[dflt|a]\n"
    );
    assert_eq!(
        text(r"\NewDocumentCommand\x{O{dflt} m}{[#1|#2]}\x[z]{a}"),
        "[z|a]\n"
    );
}

#[test]
fn an_xparse_star_argument_binds_the_star_itself() {
    // xparse's `s` binds `\BooleanTrue` or `\BooleanFalse`, which are conditionals —
    // exactly what techy-xp does not implement. What it does instead is bind the token
    // that was written: the star itself when there is one, nothing when there is not. A
    // body that only *tests* `#1` cannot work here, and a body that prints it prints a
    // star. Pinned because it is the difference a document would notice.
    assert_eq!(
        text(r"\NewDocumentCommand\x{s m}{[#1|#2]}\x*{a}"),
        "[*|a]\n"
    );
    assert_eq!(text(r"\NewDocumentCommand\x{s m}{[#1|#2]}\x{a}"), "[|a]\n");
}

#[test]
fn an_argument_may_be_used_twice_or_not_at_all() {
    assert_eq!(text(r"\newcommand\tw[1]{#1-#1}\tw{ha}"), "ha-ha\n");
    assert_eq!(text(r"\newcommand\drop[1]{X}\drop{ha}"), "X\n");
    assert_eq!(text(r"\newcommand\nothing{}a\nothing b"), "ab\n");
}

#[test]
fn a_definition_body_may_carry_a_comment_or_a_paragraph_break() {
    // A comment ends the line inside a body as it does anywhere else, and the line break
    // it eats goes with it.
    assert_eq!(text("\\newcommand\\c{a% note\nb}\\c"), "ab\n");
    // A blank line in a body is a paragraph break in the output, because a body is
    // *document text* served in place of the invocation.
    assert_eq!(text("\\newcommand\\p{a\n\nb}\\p"), "a\n\nb\n");
}

// --------------------------------------------- a body is text, expanded where it is used

#[test]
fn a_body_sees_the_definitions_in_force_where_it_is_used() {
    // techy-xp stores a body as *text* and expands it at the point of use, so a macro
    // named in a body is resolved then — not when the definition was written. `\a` reads
    // the `\b` of the moment, which is TeX's own behaviour for `\def`.
    assert_eq!(text(r"\def\a{\b}\def\b{B}\a"), "B\n");
    assert_eq!(text(r"\def\b{B}\def\a{\b}\def\b{C}\a"), "C\n");

    // `\edef` is where TeX differs — it expands its body once, at definition time, and
    // would have frozen `B`. techy-xp approximates it as `\def` and says so, which is the
    // one place this rule is a departure rather than a match.
    assert_eq!(text(r"\edef\a{\b}\def\b{B}\a"), "B\n");
    assert_eq!(
        diagnostics(r"\edef\a{\b}\def\b{B}\a"),
        ["techy-xp.presets.expanded-definition-approximated"]
    );
}

#[test]
fn a_macro_used_before_its_definition_is_an_unknown_macro() {
    // Order matters, and it is the reading order: the first `\x` is read before the
    // definition exists, so it is a command no definition claims — nothing rendered, one
    // warning — and the second is the defined one.
    assert_eq!(text(r"\x\def\x{a}\x"), "a\n");
    assert_eq!(diagnostics(r"\x\def\x{a}\x"), ["techxt.unknown-macro"]);
}

// ------------------------------------------------------- nested and recursive shapes

#[test]
fn a_definition_may_define_another() {
    // The body is text, so a body that *is* a definition makes one when it is used —
    // `\outer` is what runs `\newcommand\inner`.
    assert_eq!(
        text(r"\newcommand\outer{\newcommand\inner{IN}}\outer\inner"),
        "IN\n"
    );
    assert_eq!(text(r"\def\outer{\def\inner{IN}}\outer\inner"), "IN\n");

    // The outer definition's own argument reaches the inner body, and `##1` is how the
    // inner one keeps a parameter of its own — TeX's doubling rule, read by techy-xp's
    // parameter-text reader.
    assert_eq!(
        text(r"\def\outer#1{\def\inner{[#1]}}\outer{z}\inner"),
        "[z]\n"
    );
    assert_eq!(
        text(r"\def\outer#1{\def\inner##1{[#1|##1]}}\outer{z}\inner{y}"),
        "[z|y]\n"
    );
}

#[test]
fn a_recursion_that_terminates_terminates() {
    // The recursion is through the *argument*: each call's argument is one call less, and
    // the innermost is not a call at all, so the expansion ends without a budget or a
    // conditional being involved.
    assert_eq!(text(r"\def\d#1{(#1)}\d{\d{\d{x}}}"), "(((x)))\n");
    assert_eq!(
        text(r"\def\walk#1{#1}\def\stop{.}\walk{\walk{\walk{\stop}}}"),
        ".\n"
    );
    // A macro naming itself in its own argument is the same shape seen from the other
    // side: `\tw{\tw{a}}` doubles a doubling.
    assert_eq!(text(r"\newcommand\tw[1]{#1#1}\tw{\tw{a}}"), "aaaa\n");
    assert!(diagnostics(r"\newcommand\tw[1]{#1#1}\tw{\tw{a}}").is_empty());
}

#[test]
fn a_def_reads_a_delimited_parameter_text_more_than_once() {
    // TeX's delimiters, which no argument code could declare — and a definition is reused
    // as often as it is invoked.
    assert_eq!(
        text(r"\def\pair#1,#2.{[#1|#2]}\pair a,b. \pair x,y."),
        "[a|b] [x|y]\n"
    );
}

// -------------------------------------------------------------------- environments

#[test]
fn a_newenvironment_takes_arguments_and_an_optional_default() {
    // Both halves are stored, the arguments bind in the begin code, and the body between
    // them renders as document text.
    assert_eq!(
        text(r"\newenvironment{e}[2]{<#1|#2>}{</>}\begin{e}{a}{b}mid\end{e}"),
        "<a|b>mid</>\n"
    );
    // `[2][D]` is LaTeX's *two arguments, the first optional with default `D`*.
    assert_eq!(
        text(r"\newenvironment{e}[2][D]{<#1|#2>}{</>}\begin{e}{b}mid\end{e}"),
        "<D|b>mid</>\n"
    );
    assert_eq!(
        text(r"\newenvironment{e}[2][D]{<#1|#2>}{</>}\begin{e}[z]{b}mid\end{e}"),
        "<z|b>mid</>\n"
    );
}

#[test]
fn a_renewenvironment_replaces_a_definition_the_document_made_or_the_library_ships() {
    assert_eq!(
        text(r"\newenvironment{e}{A}{B}\renewenvironment{e}{C}{D}\begin{e}m\end{e}"),
        "CmD\n"
    );
    // And over one of techxt's own, for the same reason `\renewcommand\emph` works: the
    // definition scopes sit above every category.
    assert_eq!(
        text(r"\renewenvironment{center}{<}{>}\begin{center}m\end{center}"),
        "<m>\n"
    );
}

// ------------------------------------------------------------------------- scoping

#[test]
fn a_definition_made_in_a_group_dies_with_it_however_deep_the_group() {
    assert_eq!(text(r"{\def\x{a}}\x"), "");
    assert_eq!(diagnostics(r"{\def\x{a}}\x"), ["techxt.unknown-macro"]);
    // Each group shadows the one outside it, and each one's definition comes back when
    // the group closes.
    assert_eq!(text(r"\def\x{a}{\def\x{b}{\def\x{c}\x}\x}\x"), "cba\n");
    // An environment body is a group like any other.
    assert_eq!(text(r"\begin{center}\def\x{a}\x\end{center}"), "a\n");
    assert_eq!(text(r"\begin{center}\def\x{a}\end{center}\x"), "");
}

#[test]
fn a_gdef_escapes_every_group_it_is_written_in() {
    assert_eq!(text(r"{{{\gdef\x{a}}}}\x"), "a\n");
    assert!(diagnostics(r"{{{\gdef\x{a}}}}\x").is_empty());
}

// ------------------------------------------------- the redefinition rule, as it stands

#[test]
fn every_definer_defines_unconditionally_whatever_latexs_rule_would_be() {
    // `defs::preamble`'s *Every definer here redefines unconditionally*: techxt registers
    // each definer with `RedefinitionRule::Always`, because the existence check LaTeX's
    // rules need consults the whole scope stack — and the bottom of techxt's stack is the
    // unknown-command catch-all, which answers *yes* for every name there is.
    //
    // So all three of LaTeX's rules collapse into one, and this is what a document sees:

    // `\newcommand` over a name techxt itself ships — LaTeX would refuse this.
    assert_eq!(text(r"\newcommand\emph[1]{<#1>}\emph{a}"), "<a>\n");
    // `\renewcommand` of a name nothing defined — LaTeX would refuse this too.
    assert_eq!(text(r"\renewcommand\zzz{Z}\zzz"), "Z\n");
    // And `\providecommand`, whose whole point is to define *only* what is not defined,
    // defines regardless: over one of techxt's own macros, and over its own earlier self.
    assert_eq!(text(r"\providecommand{\emph}[1]{<#1>}\emph{a}"), "<a>\n");
    assert_eq!(
        text(r"\providecommand{\zzz}{A}\providecommand{\zzz}{B}\zzz"),
        "B\n"
    );
    // None of it is diagnosed: techy-xp's `definition-already-exists` and its sibling can
    // never fire on techxt's stack (PLAN.md §17, amended for M9).
    assert!(diagnostics(r"\providecommand{\emph}[1]{<#1>}\emph{a}").is_empty());
}

#[test]
fn a_definition_may_shadow_a_refusal() {
    // techy-xp's own rule, reached through techxt: `\def` does not resolve the name it is
    // defining, so a document may define `\ifx` and the refusal is shadowed for the rest
    // of the group. The refusal is not reported, because it never fires.
    assert_eq!(text(r"\def\ifx{IFX}\ifx"), "IFX\n");
    assert!(diagnostics(r"\def\ifx{IFX}\ifx").is_empty());
}

// ---------------------------------------------------------------------------- `\let`

#[test]
fn a_let_takes_a_snapshot_rather_than_a_reference() {
    // `\let\a\emph` copies the *meaning* `\emph` has right now. Redefining `\emph`
    // afterwards leaves `\a` as it was — which is exactly how TeX's `\let` differs from a
    // body that names `\emph` (that one would follow the redefinition; see
    // `a_body_sees_the_definitions_in_force_where_it_is_used`).
    assert_eq!(
        text(r"\let\a\emph \renewcommand\emph[1]{<#1>}\a{x} \emph{x}"),
        "𝑥 <x>\n"
    );
    // The same with a definition the document made itself.
    assert_eq!(text(r"\def\b{B}\let\a\b \def\b{C}\a\b"), "BC\n");
}

// ----------------------------------------------------------------------------- math

#[test]
fn a_macro_expands_inside_a_formula() {
    assert_eq!(text(r"\newcommand\aa{\alpha}$\aa + 1$"), "α + 1\n");
    assert_eq!(text(r"\def\aa{\alpha}\[ \aa^2 \]"), "    α²\n");
    // The expansion is text, so what it expands *to* is what decides the atom class: an
    // operator name is spaced as an operator, upright, next to an italic variable.
    assert_eq!(text(r"\def\op{\sin}$\op x$"), "sin 𝑥\n");
}

#[test]
fn a_macro_whose_body_is_a_formula_opens_the_formula() {
    // The `$…$` is in the body, so it is the expansion that starts math mode — the
    // renderer sees an ordinary math group.
    assert_eq!(text(r"\newcommand\m{$x^2$}text \m end"), "text 𝑥²end\n");
}

// ------------------------------------------------- expansion meeting the renderer

#[test]
fn a_macro_may_expand_into_any_construct_the_renderer_knows() {
    // Into another macro's invocation.
    assert_eq!(text(r"\newcommand\hi[1]{\emph{#1}}\hi{there}"), "𝑡ℎ𝑒𝑟𝑒\n");
    // Into an environment — `\begin` and `\end` both come out of one expansion.
    assert_eq!(
        text(r"\newcommand\c[1]{\begin{center}#1\end{center}}\c{mid}"),
        "mid\n"
    );
    // Into a whole list.
    assert_eq!(
        text(r"\newcommand\l{\begin{itemize}\item one\item two\end{itemize}}\l"),
        "  • one\n  • two\n"
    );
    // Into an `\item` *inside* someone else's list, which is the case that needs the
    // expansion to happen at token-reading time: the list's handler sees ordinary items.
    assert_eq!(
        text(r"\newcommand\it[1]{\item #1}\begin{itemize}\it{one}\it{two}\end{itemize}"),
        "  • one\n  • two\n"
    );
    // Into a sectioning command, counters and underline included.
    assert_eq!(
        text(r"\newcommand\s[1]{\section{#1}}\s{Title}text"),
        "1 Title\n-------\n\ntext\n"
    );
    // Into a footnote, which is a run-level side effect and still lands in the collected
    // block at the end.
    assert_eq!(
        text(r"\newcommand\fn[1]{\footnote{#1}}text\fn{note}"),
        "text[1]\n\n---\n[1] note\n"
    );
}

#[test]
fn a_macro_expands_inside_an_argument_that_is_rendered() {
    // `\t` is expanded where it stands, so the heading's title is the expansion. The
    // missing space is TeX's own rule and not this phase's doing: the space after a
    // command name ends the name and is consumed.
    assert_eq!(
        text(r"\def\t{Deep}\section{\t Title}text"),
        "1 DeepTitle\n-----------\n\ntext\n"
    );
}

#[test]
fn a_verbatim_body_is_not_expanded() {
    // Verbatim text is read by a parser that recognizes no commands, so `\x` inside it is
    // the two characters it looks like — which is what verbatim means.
    assert_eq!(
        text(r"\def\x{EXPANDED}\begin{verbatim}\x\end{verbatim}"),
        "\\x\n"
    );
}

// ----------------------------------------------------------------------------- UTF-8

#[test]
fn utf8_survives_a_definition_body_an_argument_and_a_delimiter() {
    assert_eq!(text(r"\newcommand\u{café — naïve}\u"), "café — naïve\n");
    assert_eq!(text(r"\newcommand\u[1]{«#1»}\u{émoji 🌍}"), "«émoji 🌍»\n");
    // A non-ASCII delimiter in a TeX parameter text, and a non-ASCII command *name*.
    assert_eq!(text(r"\def\p#1—#2.{[#1|#2]}\p a—b."), "[a|b]\n");
    assert_eq!(text(r"\def\é{a}\é"), "a\n");
    // And a body that expands to an accent construct rather than to a composed character.
    assert_eq!(text(r"\def\n{\'e}\n"), "é\n");
}

// -------------------------------------------------------------------- lockstep again

#[test]
fn a_document_that_defines_nothing_converts_as_it_always_did() {
    // techy-xp's lockstep property, restated at this level: honouring the definers may
    // change what a document *that defines something* converts to, and must change
    // nothing else. The whole rest of the suite is the real proof — every expectation in
    // it was written before the definers were seeded — and this is the property itself,
    // asserted against the off switch on documents that exercise the parser's corners.
    let declared = Converter::builder()
        .macro_definitions(MacroDefinitions::Declared)
        .build()
        .expect("builds");
    for latex in [
        r"Hello  {brave}\n world.",
        r"\section{Title}\emph{a} and $x^2 + \alpha$",
        r"\begin{itemize}\item one\item two\end{itemize}",
        r"\begin{tabular}{ll}a & b\\c & d\end{tabular}",
        r"A~B---C \textbf{d} \'e \#1 100\%",
        r"\begin{verbatim}\def\x{a}\end{verbatim}",
        r"\[ \frac{1}{2} \sqrt{x} \]",
        r"unknown \qqq{arg} macro",
    ] {
        let honored = Converter::standard().latex_to_text(latex).expect("parses");
        let plain = declared.latex_to_text(latex).expect("parses");
        assert_eq!(honored.text, plain.text, "{latex:?}");
        let reported: Vec<&str> = honored
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.identifier())
            .collect();
        let plain_reported: Vec<&str> = plain
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.identifier())
            .collect();
        assert_eq!(reported, plain_reported, "{latex:?}");
    }
}
