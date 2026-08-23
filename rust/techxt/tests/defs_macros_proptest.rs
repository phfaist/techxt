//! Property tests for definer syntax and expansion (PLAN.md §14.2, §16 M9 phase 3).
//!
//! The generators here write `\def`, `\newcommand`, `\gdef`, `\let` and the environment
//! definers at random, with random bodies, uses and groups around them — including the
//! shapes that do not terminate on their own. The properties are the ones a converter
//! pointed at a corpus has to keep whatever it is fed: it **never panics**, it **always
//! finishes**, and its output still obeys the layout contract.
//!
//! Every converter built here carries deliberately tiny expansion budgets. That is not a
//! way of avoiding the runaway cases — the generator writes `\def\x{\x}\x` and worse on
//! purpose — but of reaching them cheaply: the cost of exhausting the count budget grows
//! with the square of the budget (see `ConverterBuilder::expansion_count_limit`), so a
//! budget of 64 makes a thousand generated documents a matter of milliseconds each.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use proptest::prelude::*;

use techxt::convert::MacroDefinitions;
use techxt::Converter;

/// The budgets every converter in this file uses: enough expansions for the generated
/// documents to actually expand, few enough that a runaway is caught at once.
const DEPTH_BUDGET: usize = 8;
/// See [`DEPTH_BUDGET`].
const COUNT_BUDGET: usize = 64;

/// A converter with the tiny budgets and the given definer setting.
///
/// Built once and shared: a converter is immutable and `Sync`, and validating the whole
/// shipped definition set per generated case would cost more than the conversions do.
fn converter(macro_definitions: MacroDefinitions) -> &'static Converter {
    static HONORED: LazyLock<Converter> = LazyLock::new(|| build(MacroDefinitions::Honored));
    static DECLARED: LazyLock<Converter> = LazyLock::new(|| build(MacroDefinitions::Declared));
    match macro_definitions {
        MacroDefinitions::Declared => &DECLARED,
        _ => &HONORED,
    }
}

/// See [`converter`].
fn build(macro_definitions: MacroDefinitions) -> Converter {
    Converter::builder()
        .macro_definitions(macro_definitions)
        .expansion_depth_limit(DEPTH_BUDGET)
        .expansion_count_limit(COUNT_BUDGET)
        .build()
        .expect("the standard definitions build")
}

/// Definitions, uses, and the grouping that scopes them — including three shapes that
/// expand for ever and are stopped only by a budget.
fn definitions() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        // Definers, in every spelling the table registers.
        Just(r"\def\x{a}".to_string()),
        Just(r"\def\x#1{[#1]}".to_string()),
        Just(r"\def\x#1,#2.{[#1|#2]}".to_string()),
        Just(r"\gdef\x{g}".to_string()),
        Just(r"\edef\x{e}".to_string()),
        Just(r"\let\x\emph".to_string()),
        Just(r"\let\x=\y".to_string()),
        Just(r"\newcommand\y[1]{<#1>}".to_string()),
        Just(r"\newcommand*{\y}[2][d]{<#1#2>}".to_string()),
        Just(r"\renewcommand\emph[1]{#1}".to_string()),
        Just(r"\providecommand\y{p}".to_string()),
        Just(r"\NewDocumentCommand\z{s O{q} m}{(#1#2#3)}".to_string()),
        Just(r"\newenvironment{e}[1]{<#1}{>}".to_string()),
        Just(r"\renewenvironment{e}{<}{>}".to_string()),
        // Definitions that do not terminate: a flat loop, a nesting one, and a doubling
        // one. Each is bounded by a budget and by nothing else.
        Just(r"\def\x{\x}".to_string()),
        Just(r"\def\x{\x x}".to_string()),
        Just(r"\def\x{\x\x}".to_string()),
        // Uses.
        Just(r"\x".to_string()),
        Just(r"\x{u}".to_string()),
        Just(r"\x a,b.".to_string()),
        Just(r"\y{u}".to_string()),
        Just(r"\z*{u}".to_string()),
        Just(r"\begin{e}".to_string()),
        Just(r"\end{e}".to_string()),
        Just(r"\emph{u}".to_string()),
        // Grouping, which is what scopes a definition, and ordinary content.
        Just("{".to_string()),
        Just("}".to_string()),
        Just(r"\begin{center}".to_string()),
        Just(r"\end{center}".to_string()),
        Just("$".to_string()),
        Just("word ".to_string()),
        Just("\n\n".to_string()),
    ];
    proptest::collection::vec(atom, 0..16).prop_map(|parts| parts.concat())
}

/// The same, minus every definer: documents that define nothing.
fn no_definitions() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just(r"\emph{u}".to_string()),
        Just(r"\textbf{u}".to_string()),
        Just(r"\begin{center}".to_string()),
        Just(r"\end{center}".to_string()),
        Just(r"\begin{itemize}\item one\end{itemize}".to_string()),
        Just(r"\qqq".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("$x^2$".to_string()),
        Just("~".to_string()),
        Just("word ".to_string()),
        Just("\n\n".to_string()),
    ];
    proptest::collection::vec(atom, 0..16).prop_map(|parts| parts.concat())
}

proptest! {
    /// Definer syntax, however it is assembled, converts without panicking — and
    /// finishes. The clock is a runaway detector rather than a benchmark: a document
    /// this size cannot legitimately take a second, and the budgets are what guarantee
    /// it.
    #[test]
    fn generated_definitions_never_panic_and_always_finish(input in definitions()) {
        let started = Instant::now();
        let converted = converter(MacroDefinitions::Honored).latex_to_text(&input);
        prop_assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?} took {:?}",
            input,
            started.elapsed(),
        );
        // A parse error is a legitimate answer (the descent guard refuses input that
        // nests past the limit); a panic or a hang is not.
        if let Ok(conversion) = converted {
            prop_assert!(conversion.text.is_empty() || conversion.text.ends_with('\n'));
            prop_assert!(!conversion.text.contains("\n\n\n"));
            for line in conversion.text.lines() {
                let trimmed = line.trim_end_matches([' ', '\t']);
                prop_assert_eq!(line, trimmed, "trailing whitespace in {:?}", line);
            }
        }
    }

    /// The off switch survives the same documents — the definers are then argument
    /// consumers, and consuming a body that was never meant to be read must not panic
    /// either.
    #[test]
    fn generated_definitions_never_panic_with_the_definers_off(input in definitions()) {
        let _ = converter(MacroDefinitions::Declared).latex_to_text(&input);
    }

    /// Expansion is deterministic: the same document converts to the same text twice,
    /// budgets and all.
    #[test]
    fn expansion_is_deterministic(input in definitions()) {
        let converter = converter(MacroDefinitions::Honored);
        if let Ok(first) = converter.latex_to_text(&input) {
            let second = converter.latex_to_text(&input).expect("parses again");
            prop_assert_eq!(first.text, second.text);
        }
    }

    /// techy-xp's lockstep property, as a property test: a document that defines nothing
    /// converts to exactly what it converted to before any of this existed — which is
    /// what the off switch reproduces.
    #[test]
    fn a_document_that_defines_nothing_converts_the_same_either_way(input in no_definitions()) {
        let honored = converter(MacroDefinitions::Honored).latex_to_text(&input);
        let declared = converter(MacroDefinitions::Declared).latex_to_text(&input);
        match (honored, declared) {
            (Ok(honored), Ok(declared)) => prop_assert_eq!(honored.text, declared.text),
            (Err(_), Err(_)) => {}
            (honored, declared) => prop_assert!(
                false,
                "one side refused {:?}: {:?} / {:?}",
                input,
                honored.map(|c| c.text),
                declared.map(|c| c.text),
            ),
        }
    }
}
