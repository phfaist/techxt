//! Macro definitions honoured end to end (PLAN.md §16 M9, phases 2 and 3).
//!
//! techxt parses through techy-xp's expanding token reader and seeds techy-xp's definers
//! into [`defs::preamble`](techxt::defs::preamble), so a `\newcommand` in a document
//! defines a macro and a later use of it expands. This file pins that wiring: the scope
//! semantics that come with it, the refusal diagnostics techy-xp raises for what it
//! declines and the severity techxt reports them at, the expansion budgets that bound a
//! runaway, the shape of the provider stack the whole thing rests on, and the runtime
//! switch that turns all of it off.
//!
//! What each definer *shape* does — `\newcommand*`, xparse's argument codes, nested and
//! recursive definitions, definitions meeting math and the renderer — is the subject of
//! `defs_definers.rs`, one file over.

use techxt::convert::{MacroDefinitions, MapResolver, Severity, UnknownMacroResolution};
use techxt::{Converter, ConverterBuilder};
use techy_xp::lang::{DEFAULT_EXPANSION_COUNT_LIMIT, DEFAULT_EXPANSION_DEPTH_LIMIT};
use techy_xp::presets::{RefusedArgument, REFUSED_COMMANDS};

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

// ------------------------------------------------------------------ the definers

#[test]
fn a_newcommand_defines_a_macro_that_later_uses_expand() {
    assert_eq!(
        text(r"\newcommand{\greet}[1]{Hello #1!}\greet{world}"),
        "Hello world!\n"
    );
    // The unbraced spelling too, which the old chars-group declaration could not read:
    // the definer reads its own name form.
    assert_eq!(
        text(r"\newcommand\greet[1]{Hello #1!}\greet{world}"),
        "Hello world!\n"
    );
    // The definition itself renders as nothing, and nothing is reported: it is a known
    // construct that contributes no text.
    assert_eq!(text(r"\newcommand{\greet}[1]{Hi}"), "");
    assert!(diagnostics(r"\newcommand{\greet}[1]{Hi}").is_empty());
}

#[test]
fn an_optional_argument_takes_its_default_when_it_is_not_supplied() {
    assert_eq!(
        text(r"\newcommand{\hi}[2][there]{Hi #1 #2!}\hi{you}"),
        "Hi there you!\n"
    );
    assert_eq!(
        text(r"\newcommand{\hi}[2][there]{Hi #1 #2!}\hi[all]{you}"),
        "Hi all you!\n"
    );
}

#[test]
fn a_def_defines_and_reads_a_tex_parameter_text() {
    assert_eq!(text(r"\def\x{body}\x"), "body\n");
    // Delimited parameters, which no argument code could have declared.
    assert_eq!(text(r"\def\x#1,#2.{[#1|#2]}\x a,b."), "[a|b]\n");
}

#[test]
fn a_let_gives_a_second_name_to_an_existing_meaning() {
    // `\a` *is* `\emph`'s spec, so it reads `\emph`'s argument and renders as `\emph`
    // does — italics, in techxt's case.
    assert_eq!(text(r"\let\a\emph \a{sic}"), "𝑠𝑖𝑐\n");
    // The `=` spelling is read too.
    assert_eq!(text(r"\let\a=\emph \a{sic}"), "𝑠𝑖𝑐\n");
}

#[test]
fn an_edef_defines_and_says_that_it_approximated() {
    // techy-xp stores an `\edef` body as written rather than expanding it once at
    // definition time, which is a difference rather than a refusal: the definition *is*
    // made, and the warning says what was approximated.
    assert_eq!(text(r"\edef\x{a}\x"), "a\n");
    assert_eq!(
        diagnostics(r"\edef\x{a}\x"),
        ["techy-xp.presets.expanded-definition-approximated"]
    );
}

#[test]
fn a_newenvironment_defines_an_environment_the_document_can_use() {
    assert_eq!(
        text(r"\newenvironment{myenv}{begin }{ end}\begin{myenv}mid\end{myenv}"),
        "begin mid end\n"
    );
    // With an argument, bound in the begin code.
    assert_eq!(
        text(r"\newenvironment{e}[1]{<#1>}{</>}\begin{e}{z}mid\end{e}"),
        "<z>mid</>\n"
    );
}

#[test]
fn a_renewcommand_shadows_a_macro_techxt_ships() {
    // This is what the scope-order invariant buys: the local definition scope sits above
    // every one of techxt's category packages, so a definition made by the document wins
    // over the library's own `\emph`.
    assert_eq!(text(r"\renewcommand{\emph}[1]{<<#1>>}\emph{x}"), "<<x>>\n");
    assert!(diagnostics(r"\renewcommand{\emph}[1]{<<#1>>}\emph{x}").is_empty());
}

// -------------------------------------------------------------------- scoping

#[test]
fn a_def_reverts_with_its_group_and_a_gdef_escapes_it() {
    // Local: `\x` is undefined again outside the group, so it is an unknown macro and
    // renders as nothing.
    assert_eq!(text(r"{\def\x{a}}\x"), "");
    assert_eq!(text(r"\def\x{a}{\def\x{b}\x}\x"), "ba\n");
    // Global: the definition escapes the group it was written in.
    assert_eq!(text(r"{\gdef\x{a}}\x"), "a\n");
}

#[test]
fn a_gdef_escapes_an_environment_body_too() {
    // An environment body is a group like any other, and its interior's after-effects
    // used to die with the body (techy-xp §13.1). This is that fix, seen through the
    // whole converter.
    assert_eq!(text(r"\begin{center}\gdef\x{a}\end{center}\x"), "a\n");
    assert!(diagnostics(r"\begin{center}\gdef\x{a}\end{center}\x").is_empty());
}

#[test]
fn a_later_def_shadows_an_earlier_gdef_as_it_does_in_tex() {
    // The local scope sits *above* the global one, which is what makes this read `b`.
    // A stack with the two the other way round reads `a`, silently.
    assert_eq!(text(r"\def\y{1}\gdef\x{a}\def\x{b}\x"), "b\n");
}

// ------------------------------------------------------------------- refusals

#[test]
fn a_refused_command_is_diagnosed_by_name_rather_than_left_unknown() {
    // techy-xp's refusals package resolves `\expandafter` with its true (empty)
    // signature and says which feature is missing. The node then reaches the renderer
    // carrying a *foreign* spec, so techxt's dispatch misses at step 2 (downcast) and at
    // step 3 (the name table has no entry for a name techxt never declared), and the
    // unknown-construct policy of PLAN.md §10.6 decides what it looks like: nothing.
    //
    // It does *not* also report `techxt.unknown-macro`. Step 4 recognizes the refusal
    // spec (`def::is_refusal`) and leaves the reporting to the parse, which named the
    // missing feature — one occurrence, one diagnostic.
    assert_eq!(text(r"a\expandafter b"), "ab\n");
    assert_eq!(
        diagnostics(r"a\expandafter b"),
        ["techy-xp.presets.expandafter-unsupported"]
    );
}

#[test]
fn a_refusal_is_reported_as_a_warning_and_does_not_fail_the_conversion() {
    // PLAN.md §10.6 as amended for M9 phase 3: techxt's severity convention calls a
    // construct it has never heard of a warning, so a construct it refuses *by name*
    // cannot rank worse. techy-xp raises these at error severity — it must, since a
    // strict parse has to abort on one — and techxt restamps them on the way out.
    for latex in [r"a\expandafter b", r"\ifx x\fi", r"\noexpand x"] {
        let conversion = Converter::standard().latex_to_text(latex).expect("parses");
        assert!(!conversion.diagnostics.has_errors(), "{latex:?}");
        for diagnostic in conversion.diagnostics.iter() {
            assert_eq!(diagnostic.severity(), Severity::Warning, "{latex:?}");
            // The identifier and the message are techy-xp's, untouched: only the
            // severity is techxt's.
            assert!(
                diagnostic.identifier().starts_with("techy-xp.presets."),
                "{latex:?}: {}",
                diagnostic.identifier()
            );
            assert!(!diagnostic.message().is_empty());
        }
    }
}

#[test]
fn every_refusal_condition_is_demoted() {
    // The demotion in `convert::at_techxt_severity` lists the refusal conditions one by
    // one, and a refusal techy-xp adds later would silently keep error severity. techy-xp
    // ships its refusal table as data, so the list can be checked against it: every
    // command in `REFUSED_COMMANDS`, invoked, must report at warning severity.
    let converter = Converter::standard();
    for refused in REFUSED_COMMANDS {
        // Written the way the refusal declares itself, so that what is measured is the
        // refusal and not a malformed invocation: a command-name argument gets a command
        // name, a mandatory one a group, and an optional one is left out.
        let mut latex = format!("A\\{}", refused.name);
        for argument in refused.arguments {
            match argument {
                RefusedArgument::CommandName => latex.push_str("\\foo"),
                RefusedArgument::Optional => {}
                _ => latex.push_str("{alpha}"),
            }
        }
        latex.push_str(" Z");
        let conversion = converter.latex_to_text(&latex).expect("parses");
        let reported: Vec<&str> = conversion
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.identifier())
            .collect();
        // Some refused names are claimed by techxt itself and never reach the refusal at
        // all (`\setcounter`, `\makeatletter`, …) — those report nothing, which is
        // `a_name_techxt_declares_itself_wins_over_the_refusal_of_the_same_name`'s
        // subject. What must never happen, for any of them, is an error.
        assert!(
            !conversion.diagnostics.has_errors(),
            "\\{} reported {reported:?} as an error",
            refused.name
        );
        for diagnostic in conversion.diagnostics.iter() {
            assert_eq!(
                diagnostic.severity(),
                Severity::Warning,
                "\\{}: {}",
                refused.name,
                diagnostic.identifier()
            );
        }
    }
}

#[test]
fn a_malformed_refusal_invocation_is_still_an_error() {
    // The demotion is about the *refusal* conditions, and nothing else techy-xp raises.
    // A register allocator reads a command name — `\newcount\foo` — so `\newcount{alpha}`
    // has no name to read, and techy-xp reports a malformed invocation. That is not a
    // feature it declined to implement, it is a document it could not read, and it stays
    // an error.
    let conversion = Converter::standard()
        .latex_to_text(r"\newcount{alpha}")
        .expect("parses");
    assert!(conversion.diagnostics.has_errors());
    let reported: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert_eq!(reported[0], "techy-xp.constructs.expected-command-name");
    // Written as LaTeX writes it, the same command is a warning like any other refusal.
    let conversion = Converter::standard()
        .latex_to_text(r"\newcount\foo")
        .expect("parses");
    assert!(!conversion.diagnostics.has_errors());
    assert_eq!(
        diagnostics(r"\newcount\foo"),
        ["techy-xp.presets.register-allocation-unsupported"]
    );
}

#[test]
fn a_refusal_reached_through_an_expansion_draws_no_second_parse_error() {
    // techy-xp on its own can follow a token-time refusal with techy's
    // `core.specs.unresolvable-command`. Inside techxt's stack it does not, and the
    // reason is techxt's own outermost catch-all provider: every name resolves to
    // *something*, so nothing is ever unresolvable. Measured, not assumed — all three
    // routes to a refusal report exactly one techy-xp condition and nothing else.
    for latex in [
        r"a\expandafter b",
        r"\def\x{\expandafter}a\x b",
        r"\newcommand\x{\ifx}a\x b",
    ] {
        let reported = Converter::standard()
            .latex_to_text(latex)
            .expect("parses")
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.identifier().to_string())
            .collect::<Vec<_>>();
        assert!(
            !reported
                .iter()
                .any(|id| id == "core.specs.unresolvable-command"),
            "{latex:?} reported {reported:?}"
        );
        assert_eq!(reported.len(), 1, "{latex:?} reported {reported:?}");
    }
}

#[test]
fn a_name_techxt_declares_itself_wins_over_the_refusal_of_the_same_name() {
    // The refusals package sits *below* every techxt category, mirroring where techy-xp's
    // own seed puts its shipped packages. `\setcounter` is declared in `defs::preamble`
    // and dropped silently, which is techxt's answer for it, and the refusal never fires.
    assert_eq!(text(r"\setcounter{page}{1}text"), "text\n");
    assert!(diagnostics(r"\setcounter{page}{1}text").is_empty());
    assert_eq!(text(r"\newcounter{thm}[section]text"), "text\n");
    assert!(diagnostics(r"\newcounter{thm}[section]text").is_empty());
    // `defs::base` claims the counter-printing family for the same reason.
    assert_eq!(text(r"\stepcounter{eq}\value{page}text"), "text\n");
    assert!(diagnostics(r"\stepcounter{eq}\value{page}text").is_empty());
    assert_eq!(text(r"\makeatletter a\makeatother"), "a\n");
    assert!(diagnostics(r"\makeatletter a\makeatother").is_empty());
}

#[test]
fn a_refusal_changes_what_is_reported_and_not_what_is_rendered() {
    // Registering the refusals is the one part of this phase that a document defining no
    // macro can notice, and this is the whole of what it notices: the *text* is what it
    // always was, and the diagnostic is a techy-xp condition naming the missing feature
    // where it used to be a `techxt.unknown-macro` saying only that techxt had no rule.
    // Both are warnings, so neither costs a caller reading
    // `Conversion::diagnostics.has_errors()` — the CLI's exit code among them — anything.
    let declared = Converter::builder()
        .macro_definitions(MacroDefinitions::Declared)
        .build()
        .expect("builds");
    // `\newcount` is deliberately not in this list: it reads a command *name*, so
    // `\newcount{alpha}` is malformed rather than merely refused — see
    // `a_malformed_refusal_invocation_is_still_an_error`.
    for name in ["expandafter", "ifx", "fi", "catcode", "count", "noexpand"] {
        // Bare, and with two groups after it, so that a refusal declaring the wrong
        // arity would show up as a text difference.
        for latex in [
            format!("A\\{name}{{alpha}}{{beta}} Z"),
            format!("A\\{name} Z"),
        ] {
            let honored = Converter::standard().latex_to_text(&latex).expect("parses");
            let plain = declared.latex_to_text(&latex).expect("parses");
            assert_eq!(honored.text, plain.text, "{latex:?} rendered differently");
            assert!(!honored.diagnostics.has_errors(), "{latex:?}");
            assert!(!plain.diagnostics.has_errors(), "{latex:?}");
            // One report either way — the honoured stack names the feature, the declared
            // one says the macro is unknown.
            let reported: Vec<&str> = honored
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.identifier())
                .collect();
            assert_eq!(reported.len(), 1, "{latex:?} reported {reported:?}");
            assert!(reported[0].starts_with("techy-xp.presets."), "{latex:?}");
            assert_eq!(plain.diagnostics.len(), 1, "{latex:?}");
        }
    }
}

// --------------------------------------------------------------- expansion budgets

#[test]
fn a_runaway_definition_stops_at_the_count_budget() {
    // techy-xp does no cycle detection: `\def\x{\x}\x` expands for ever, and the count
    // budget is what ends it. The budget here is deliberately tiny so that the test costs
    // milliseconds — the cost of reaching one grows with the *square* of the budget, which
    // is why techxt's default is 2 000 and not techy-xp's 100 000
    // (`ConverterBuilder::expansion_count_limit` carries the measurement).
    let converter = Converter::builder()
        .expansion_count_limit(100)
        .build()
        .expect("builds");
    let conversion = converter.latex_to_text(r"\def\x{\x}\x").expect("parses");
    let reported: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert!(
        reported.contains(&"techy-xp.expand.expansion-count-budget-exceeded"),
        "{reported:?}"
    );
    // A budget is not a refusal: the document really was cut off, so it stays an error
    // and the CLI exits non-zero on it.
    assert!(conversion.diagnostics.has_errors());
}

#[test]
fn a_nesting_runaway_stops_at_the_depth_budget() {
    // The other shape of runaway — each step nests one frame deeper instead of looping
    // flat — and the other budget that catches it.
    //
    // What makes it nest is the `x` and the `y`: a frame with nothing left to serve is
    // popped *before* its successor is pushed, so the obvious `\def\a{\b}\def\b{\a}\a`
    // does not stack frames at all and is caught by the count budget instead. Measured,
    // not assumed — with the trailing characters the depth budget is what fires.
    let converter = Converter::builder()
        .expansion_depth_limit(8)
        .build()
        .expect("builds");
    let conversion = converter
        .latex_to_text(r"\def\a{\b x}\def\b{\a y}\a")
        .expect("parses");
    let reported: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert!(
        reported.contains(&"techy-xp.expand.expansion-depth-budget-exceeded"),
        "{reported:?}"
    );
}

#[test]
fn the_default_budgets_are_techxts_own_and_the_knobs_reach_techy_xps() {
    // The numbers are stated in three places — here, on the two constants, and in the
    // CLI's `--help` — so a test says they agree. techxt's are lower on purpose: it
    // converts documents it did not write.
    assert_eq!(ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT, 64);
    assert_eq!(ConverterBuilder::DEFAULT_EXPANSION_COUNT_LIMIT, 2_000);
    const {
        assert!(ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT < DEFAULT_EXPANSION_DEPTH_LIMIT);
        assert!(ConverterBuilder::DEFAULT_EXPANSION_COUNT_LIMIT < DEFAULT_EXPANSION_COUNT_LIMIT);
    }

    // And the knobs reach upstream's own defaults for a caller who vouches for the input.
    let upstream = Converter::builder()
        .expansion_depth_limit(DEFAULT_EXPANSION_DEPTH_LIMIT)
        .expansion_count_limit(DEFAULT_EXPANSION_COUNT_LIMIT)
        .build()
        .expect("builds");
    assert_eq!(
        upstream
            .latex_to_text(r"\newcommand\greet[1]{Hello #1!}\greet{world}")
            .expect("parses")
            .text,
        "Hello world!\n"
    );
}

#[test]
fn an_ordinary_document_is_nowhere_near_the_budget() {
    // One expansion per invocation: the budget is a runaway threshold, not a per-macro
    // one. A hundred uses of two macros is a hundred and ninety-nine expansions below the
    // default and reports nothing.
    let mut latex = String::from(r"\newcommand\kw[1]{\emph{#1}}\newcommand\ket[1]{|#1>}");
    for _ in 0..100 {
        latex.push_str(r"A \kw{term} and \ket{\psi}. ");
    }
    let conversion = Converter::standard().latex_to_text(&latex).expect("parses");
    assert!(conversion.diagnostics.is_empty());
    assert!(conversion.text.contains("𝑡𝑒𝑟𝑚"));
}

// -------------------------------------------------------------- the provider stack

#[test]
fn the_provider_stack_is_ordered_as_techy_xp_requires() {
    // The invariant this pins is *silent* when it breaks: techy creates a missing
    // definition scope innermost, so a seed that omits or reorders the two parses without
    // complaint and answers `\def\y{1}\gdef\x{a}\def\x{b}\x` wrongly. Outermost first:
    let converter = Converter::standard();
    let providers: Vec<&str> = converter
        .language()
        .initial_state()
        .scopes()
        .providers()
        .iter()
        .map(|provider| provider.name())
        .collect();

    // The catch-all is outermost, so every real definition shadows it; techy's own
    // `\begin`/`\end` sit above it, or no environment would parse; techy-xp's refusals
    // below every category, as techy-xp's own seed puts its shipped packages.
    assert_eq!(
        &providers[..3],
        ["techxt-unknown", "_builtin", "techy-xp.refusals"]
    );
    // The categories, in `defs::standard`'s push order.
    assert_eq!(providers[3], "symbols_extra");
    assert_eq!(providers[providers.len() - 3], "natbib");
    // Innermost, after every package: global then local — local above global.
    assert_eq!(
        &providers[providers.len() - 2..],
        ["techy-xp:global", "techy-xp:local"]
    );

    // Under strict recovery the catch-all is not registered, and everything else keeps
    // its place.
    let strict = Converter::builder()
        .unknown_macro_resolution(UnknownMacroResolution::Reject)
        .build()
        .expect("builds");
    let providers: Vec<&str> = strict
        .language()
        .initial_state()
        .scopes()
        .providers()
        .iter()
        .map(|provider| provider.name())
        .collect();
    assert_eq!(&providers[..2], ["_builtin", "techy-xp.refusals"]);
    assert_eq!(
        &providers[providers.len() - 2..],
        ["techy-xp:global", "techy-xp:local"]
    );
}

// ------------------------------------------------------------------ the off switch

#[test]
fn the_off_switch_reproduces_the_declared_and_dropped_behaviour() {
    let declared = Converter::builder()
        .macro_definitions(MacroDefinitions::Declared)
        .build()
        .expect("builds");

    // techxt 0.1.0's answer, byte for byte: the definition is consumed and dropped, and
    // a later use of `\greet` is an unknown macro that takes no arguments — so its
    // `{world}` is an ordinary group and survives.
    let latex = r"\newcommand{\greet}[1]{Hello #1!}\greet{world}";
    assert_eq!(
        declared.latex_to_text(latex).expect("parses").text,
        "world\n"
    );
    assert_eq!(text(latex), "Hello world!\n");

    // And the body of a definition still never reaches the text.
    let latex = r"\newcommand{\ket}[1]{\lvert #1 \rangle}between\def\x{body}after";
    assert_eq!(
        declared.latex_to_text(latex).expect("parses").text,
        "betweenafter\n"
    );
}

#[test]
fn the_off_switch_seeds_neither_the_scopes_nor_the_refusals() {
    let declared = Converter::builder()
        .macro_definitions(MacroDefinitions::Declared)
        .build()
        .expect("builds");
    let providers: Vec<&str> = declared
        .language()
        .initial_state()
        .scopes()
        .providers()
        .iter()
        .map(|provider| provider.name())
        .collect();
    assert_eq!(&providers[..2], ["techxt-unknown", "_builtin"]);
    assert!(
        !providers.iter().any(|name| name.starts_with("techy-xp")),
        "{providers:?}"
    );

    // So a refused command is an unknown one again, with techxt's warning and nothing
    // from techy-xp.
    let conversion = declared.latex_to_text(r"a\expandafter b").expect("parses");
    assert_eq!(conversion.text, "ab\n");
    let reported: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert_eq!(reported, ["techxt.unknown-macro"]);
}

// --------------------------------------------------------------- `\input` and state

#[test]
fn definitions_travel_into_an_included_file_and_not_back_out() {
    // techxt registers techy's inclusion spec as **state-transparent**
    // (`persist_state: false`, `defs::inputs`), which nothing could observe before the
    // definers were seeded. This is what it means now, measured in both directions.
    let mut resolver = MapResolver::new();
    resolver.insert("defs.tex", r"\newcommand\greet[1]{Hello #1!}\gdef\qq{QQ}");
    resolver.insert("use.tex", r"\greet{inner}");
    let converter = Converter::builder()
        .source_resolver(resolver)
        .build()
        .expect("builds");

    // In: the state at the `\input` governs how the file is read, so a macro defined
    // before it expands inside it.
    assert_eq!(
        converter
            .latex_to_text(r"\newcommand\greet[1]{Hello #1!}\input{use.tex}")
            .expect("parses")
            .text,
        "Hello inner!\n"
    );

    // Not out: neither the `\newcommand` nor the `\gdef` survives the file. `\greet` is
    // an unknown macro that takes no arguments, so its `{world}` is an ordinary group
    // and survives; `\qq` renders as nothing.
    let conversion = converter
        .latex_to_text(r"\input{defs.tex}\greet{world}\qq")
        .expect("parses");
    assert_eq!(conversion.text, "world\n");
    let reported: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    assert_eq!(reported, ["techxt.unknown-macro", "techxt.unknown-macro"]);
}

// ---------------------------------------------------------------- what stays out

#[test]
fn declare_math_operator_is_still_read_and_dropped() {
    // techy-xp ships no definer for it, deliberately, so `\tr` is not defined and the
    // declaration itself is consumed and renders as nothing.
    assert_eq!(text(r"\DeclareMathOperator{\tr}{tr}text"), "text\n");
    assert!(diagnostics(r"\DeclareMathOperator{\tr}{tr}text").is_empty());
}

#[test]
fn a_definition_written_in_a_parsed_argument_reaches_nothing() {
    // techy-xp §3.3: the definition is made inside a parsed argument's own state, which
    // is discarded — and nothing is diagnosed. Pinned so that a future fix is noticed.
    assert_eq!(text(r"\textbf{\def\x{a}}\x"), "");
    assert_eq!(
        diagnostics(r"\textbf{\def\x{a}}\x"),
        ["techxt.unknown-macro"]
    );
}
