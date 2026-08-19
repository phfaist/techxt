//! Property tests for the renderer core (PLAN.md §14.2).
//!
//! The one that matters most: **tolerant parsing and conversion must never panic**, for
//! any input at all. techxt is meant to be pointed at whatever a corpus contains —
//! truncated files, binary noise, LaTeX nobody would write on purpose — and answer with
//! text and diagnostics rather than with a crash. The generators below therefore favour
//! the characters that mean something to the parser, so that random strings actually
//! reach the interesting code paths instead of being one long run of letters.

use proptest::prelude::*;

use techxt::convert::{MathMode, UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy};
use techxt::Converter;

/// Characters the parser cares about, so that generated input is structurally lively.
const INTERESTING: &str = r"[\\{}\[\]$&%~^_ \n\|a-c1]{0,120}";

/// A document made of the constructs the minimal definition set knows about.
fn constructs() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just(r"\emph{".to_string()),
        Just(r"\textbf{".to_string()),
        Just(r"\href{u}{".to_string()),
        Just(r"\begin{center}".to_string()),
        Just(r"\end{center}".to_string()),
        Just(r"\begin{verbatim}".to_string()),
        Just(r"\end{verbatim}".to_string()),
        Just(r"\begin{myenv}".to_string()),
        Just(r"\verb|x|".to_string()),
        Just(r"\phantom{".to_string()),
        Just(r"\ldots".to_string()),
        Just(r"\par".to_string()),
        Just("$".to_string()),
        Just(r"\[".to_string()),
        Just(r"\]".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("~".to_string()),
        Just("&".to_string()),
        Just("% c\n".to_string()),
        Just("\n\n".to_string()),
        Just("word ".to_string()),
    ];
    proptest::collection::vec(atom, 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    /// Arbitrary text never panics and never aborts the process.
    #[test]
    fn arbitrary_strings_never_panic(input in INTERESTING) {
        let converter = Converter::standard();
        // A parse error is a legitimate answer (the descent guard refuses input that
        // nests past the limit); a panic is not.
        if let Ok(conversion) = converter.latex_to_text(&input) {
            // Whatever comes out, the layout engine's contract holds.
            prop_assert!(conversion.text.is_empty() || conversion.text.ends_with('\n'));
            prop_assert!(!conversion.text.contains("\n\n\n"));
            for line in conversion.text.lines() {
                // No line ends in *collapsible* whitespace. A no-break space is
                // content — it is what `~` is for — so it is allowed to sit there.
                let trimmed = line.trim_end_matches([' ', '\t']);
                prop_assert_eq!(line, trimmed, "trailing whitespace in {:?}", line);
            }
        }
    }

    /// Any string at all, including binary-ish noise.
    #[test]
    fn truly_arbitrary_strings_never_panic(input in ".*") {
        let _ = Converter::standard().latex_to_text(&input);
    }

    /// Structurally lively documents never panic, under every policy combination that
    /// changes what unknown constructs do.
    #[test]
    fn every_policy_survives_arbitrary_constructs(
        input in constructs(),
        macro_policy in prop_oneof![
            Just(UnknownMacroPolicy::Skip),
            Just(UnknownMacroPolicy::RenderArgs),
            Just(UnknownMacroPolicy::KeepSource),
            Just(UnknownMacroPolicy::Placeholder),
        ],
        env_policy in prop_oneof![
            Just(UnknownEnvPolicy::RenderBody),
            Just(UnknownEnvPolicy::Skip),
            Just(UnknownEnvPolicy::KeepSource),
        ],
        specials_policy in prop_oneof![
            Just(UnknownSpecialsPolicy::EmitChars),
            Just(UnknownSpecialsPolicy::Skip),
        ],
        math_mode in prop_oneof![
            Just(MathMode::Fancy),
            Just(MathMode::Plain),
            Just(MathMode::Source),
        ],
        wrap_width in prop_oneof![Just(None), Just(Some(1usize)), Just(Some(20))],
    ) {
        let converter = Converter::builder()
            .unknown_macro(macro_policy)
            .unknown_env(env_policy)
            .unknown_specials(specials_policy)
            .math_mode(math_mode)
            .wrap_width(wrap_width)
            .build()
            .expect("the minimal definitions build");
        let _ = converter.latex_to_text(&input);
    }

    /// Converting the same document twice gives the same answer, and going through the
    /// tree layer gives the same answer as going through the string layer.
    #[test]
    fn conversion_is_deterministic_and_layer_independent(input in constructs()) {
        let converter = Converter::standard();
        if let Ok(first) = converter.latex_to_text(&input) {
            let second = converter.latex_to_text(&input).expect("parses again");
            prop_assert_eq!(&first.text, &second.text);
            prop_assert_eq!(first.diagnostics.len(), second.diagnostics.len());

            let tree = converter.language().parse(input.as_str()).expect("parses");
            let from_tree = converter.tree_to_text(&tree.tree);
            prop_assert_eq!(&first.text, &from_tree.text);
        }
    }

    /// Comments never contribute anything by default, so removing them cannot change
    /// the output — the LaTeX-correct behaviour PLAN.md §9.1 prescribes.
    #[test]
    fn a_trailing_comment_changes_nothing(input in "[a-c ]{0,40}") {
        let converter = Converter::standard();
        let plain = converter.latex_to_text(&input).expect("parses").text;
        let commented = converter
            .latex_to_text(&format!("{input}% a comment\n"))
            .expect("parses")
            .text;
        prop_assert_eq!(plain, commented);
    }
}
