//! The rule dispatch chain (PLAN.md §10.3) and the unknown-construct policies
//! (PLAN.md §10.6).
//!
//! Dispatch consults four sources in order, and each test here pins one link of the
//! chain by making the sources disagree:
//!
//! 1. the converter's override map,
//! 2. the rule embedded in the node's own spec,
//! 3. the converter's name fallback table,
//! 4. the unknown-construct policy.

use std::borrow::Cow;

use techy::core::specs::Package;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{CallableType, Latexlike, LatexlikeDriver, MacroSpec};

use techxt::convert::{UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy};
use techxt::def::{Category, DefinitionSet, EnvDef, MacroDef, SpecialsDef, TextRule};
use techxt::Converter;

/// A literal rule.
fn literal(text: &'static str) -> TextRule {
    TextRule::Literal(Cow::Borrowed(text))
}

/// A converter whose only macro is `\mark`, rendering as `text`.
fn marking(text: &'static str) -> Converter {
    Converter::builder()
        .definitions(
            DefinitionSet::new()
                .with(Category::new("marks").with_macro(MacroDef::new("mark").rule(literal(text)))),
        )
        .build()
        .expect("builds")
}

// ------------------------------------------------------- 1. the override map

#[test]
fn an_override_beats_the_rule_in_the_spec() {
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(
            Category::new("marks").with_macro(MacroDef::new("mark").rule(literal("FROM-THE-SPEC"))),
        ))
        .override_macro("mark", literal("FROM-THE-OVERRIDE"))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\mark").expect("parses").text,
        "FROM-THE-OVERRIDE\n"
    );
}

#[test]
fn overrides_are_keyed_by_kind_as_well_as_name() {
    // A macro called `x` and an environment called `x` are different constructs, and
    // techy keeps them in different stores; an override for one must not reach the
    // other.
    let converter = Converter::builder()
        .definitions(
            DefinitionSet::new().with(
                Category::new("both")
                    .with_macro(MacroDef::new("x").rule(literal("macro")))
                    .with_env(EnvDef::new("x").rule(literal("environment"))),
            ),
        )
        .override_macro("x", literal("OVERRIDDEN"))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\x").expect("parses").text,
        "OVERRIDDEN\n"
    );
    assert_eq!(
        converter
            .latex_to_text(r"\begin{x}\end{x}")
            .expect("parses")
            .text,
        "environment\n"
    );
}

// ------------------------------------------------- 2. the rule in the own spec

#[test]
fn the_rule_in_the_spec_beats_the_name_fallback_table() {
    // The tree is parsed by one converter's definitions and rendered by another's, so
    // the rule that travelled *inside* the tree can be told apart from the rule the
    // rendering converter has under that name. The one in the tree wins: that is what
    // makes a definition and what it parsed impossible to disagree about.
    let parser = marking("FROM-THE-SPEC");
    let renderer = marking("FROM-THE-TABLE");
    let tree = parser.language().parse(r"\mark").expect("parses").tree;
    assert_eq!(renderer.tree_to_text(&tree).text, "FROM-THE-SPEC\n");
}

// -------------------------------------------- 3. the name fallback table

#[test]
fn a_foreign_tree_is_rendered_through_the_name_fallback_table() {
    // A tree parsed with somebody else's definitions carries plain techy specs, which
    // hold no techxt rule at all. The name table is what makes techxt useful on it.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(CallableType::Macro, "mark", MacroSpec::new(Vec::new()));
    let foreign = Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages([package]).expect("seed state"),
    );
    let tree = foreign.parse(r"a\mark b").expect("parses").tree;

    assert_eq!(
        marking("FROM-THE-TABLE").tree_to_text(&tree).text,
        "aFROM-THE-TABLEb\n"
    );
}

#[test]
fn a_foreign_tree_with_no_matching_name_falls_through_to_the_policy() {
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(CallableType::Macro, "elsewhere", MacroSpec::new(Vec::new()));
    let foreign = Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages([package]).expect("seed state"),
    );
    let tree = foreign.parse(r"a\elsewhere b").expect("parses").tree;

    let conversion = marking("unused").tree_to_text(&tree);
    assert_eq!(conversion.text, "ab\n");
    assert_eq!(
        conversion
            .diagnostics
            .with_identifier("techxt.unknown-macro")
            .count(),
        1
    );
}

// ------------------------------------------------------ 4. the unknown policies

/// Convert `latex` under an unknown-macro policy, with the placeholder definitions.
fn under_macro_policy(policy: UnknownMacroPolicy, latex: &str) -> String {
    Converter::builder()
        .unknown_macro(policy)
        .build()
        .expect("builds")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// The shipped library plus a macro and a specials that are declared but rule-less.
///
/// Every entry `techxt::defs` ships carries a rule, as it should; the last link of the
/// dispatch chain therefore needs constructs made for the purpose. Declaring the
/// arguments is the point: it is the only case in which `RenderArgs` has anything to
/// render and `KeepSource` has a full invocation to re-emit.
fn with_ruleless() -> DefinitionSet {
    techxt::defs::standard().with(
        Category::new("test-ruleless")
            .with_macro(MacroDef::new("ruleless").arg("m", "text"))
            .with_specials(SpecialsDef::new("@@")),
    )
}

/// [`under_macro_policy`] with the rule-less vehicles in scope.
fn under_macro_policy_ruleless(policy: UnknownMacroPolicy, latex: &str) -> String {
    Converter::builder()
        .definitions(with_ruleless())
        .unknown_macro(policy)
        .build()
        .expect("builds")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

#[test]
fn every_unknown_macro_policy_acts_on_a_genuinely_unregistered_macro() {
    // `\foo` is defined nowhere. techxt's catch-all provider still gives it a spec, so
    // it parses as a macro and the policy — rather than techy's error recovery —
    // decides what the reader sees. The catch-all takes no arguments, so `{x}` after it
    // is an ordinary group whose content survives whatever the policy says.
    assert_eq!(
        under_macro_policy(UnknownMacroPolicy::Skip, r"\foo{x} bar"),
        "x bar\n"
    );
    assert_eq!(
        under_macro_policy(UnknownMacroPolicy::RenderArgs, r"\foo{x} bar"),
        "x bar\n"
    );
    assert_eq!(
        under_macro_policy(UnknownMacroPolicy::KeepSource, r"\foo{x} bar"),
        "\\foox bar\n"
    );
    assert_eq!(
        under_macro_policy(UnknownMacroPolicy::Placeholder, r"\foo{x} bar"),
        "<foo>x bar\n"
    );
}

#[test]
fn every_unknown_macro_policy_acts_on_a_macro_that_parses_but_has_no_rule() {
    // `\ruleless` is declared with one argument and no rule, which is where the
    // policies differ: only here can `RenderArgs` have arguments to render.
    assert_eq!(
        under_macro_policy_ruleless(UnknownMacroPolicy::Skip, r"a\ruleless{x}b"),
        "ab\n"
    );
    assert_eq!(
        under_macro_policy_ruleless(UnknownMacroPolicy::RenderArgs, r"a\ruleless{x}b"),
        "axb\n"
    );
    assert_eq!(
        under_macro_policy_ruleless(UnknownMacroPolicy::KeepSource, r"a\ruleless{x}b"),
        "a\\ruleless{x}b\n"
    );
    assert_eq!(
        under_macro_policy_ruleless(UnknownMacroPolicy::Placeholder, r"a\ruleless{x}b"),
        "a<ruleless>b\n"
    );
}

#[test]
fn an_unknown_macro_is_reported_whatever_the_policy_says() {
    for policy in [
        UnknownMacroPolicy::Skip,
        UnknownMacroPolicy::RenderArgs,
        UnknownMacroPolicy::KeepSource,
        UnknownMacroPolicy::Placeholder,
    ] {
        let conversion = Converter::builder()
            .unknown_macro(policy)
            .build()
            .expect("builds")
            .latex_to_text(r"\foo")
            .expect("parses");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-macro")
                .count(),
            1,
            "{policy:?}"
        );
        // A warning, not an error: the document converted, something in it did not.
        assert!(!conversion.diagnostics.has_errors(), "{policy:?}");
    }
}

#[test]
fn the_unknown_environment_policies_each_act() {
    for (policy, expected) in [
        (UnknownEnvPolicy::RenderBody, "a inner b\n"),
        (UnknownEnvPolicy::Skip, "a b\n"),
        (
            UnknownEnvPolicy::KeepSource,
            "a\n\n\\begin{myenv}inner\\end{myenv}\n\nb\n",
        ),
    ] {
        let conversion = Converter::builder()
            .unknown_env(policy)
            .build()
            .expect("builds")
            .latex_to_text(r"a \begin{myenv}inner\end{myenv} b")
            .expect("parses");
        assert_eq!(conversion.text, expected, "{policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-environment")
                .count(),
            1,
            "{policy:?}"
        );
    }
}

#[test]
fn the_unknown_specials_policies_each_act() {
    // `@@` is declared with no rule, so it is a specials the policy has to decide
    // about — unlike the `&` the shipped library defines.
    for (policy, expected) in [
        (UnknownSpecialsPolicy::EmitChars, "a@@b\n"),
        (UnknownSpecialsPolicy::Skip, "ab\n"),
    ] {
        let conversion = Converter::builder()
            .definitions(with_ruleless())
            .unknown_specials(policy)
            .build()
            .expect("builds")
            .latex_to_text("a@@b")
            .expect("parses");
        assert_eq!(conversion.text, expected, "{policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-specials")
                .count(),
            1,
            "{policy:?}"
        );
    }
}

#[test]
fn a_converter_that_knows_nothing_still_converts() {
    // The extreme case of the catch-all: an empty definition set. Every command is
    // unknown, every one of them is reported, and the text between them survives.
    let converter = Converter::builder()
        .definitions(DefinitionSet::new())
        .build()
        .expect("an empty set builds");
    let conversion = converter
        .latex_to_text(r"one \two three \four")
        .expect("parses");
    assert_eq!(conversion.text, "one three\n");
    assert_eq!(
        conversion
            .diagnostics
            .with_identifier("techxt.unknown-macro")
            .count(),
        2
    );
    assert!(!conversion.diagnostics.has_errors());
}

// ------------------------------------------------------------ rule kinds

#[test]
fn every_rule_kind_renders() {
    let category = Category::new("kinds")
        .with_macro(MacroDef::new("lit").rule(literal("LIT")))
        .with_macro(MacroDef::new("skip").arg("m", "text").rule(TextRule::Skip))
        .with_macro(
            MacroDef::new("content")
                .arg("m", "a")
                .arg("m", "b")
                .rule(TextRule::Content),
        )
        .with_env(EnvDef::new("keep").rule(TextRule::Content))
        .with_specials(SpecialsDef::new("--").rule(literal("–")));
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .build()
        .expect("builds");
    let text = |latex: &str| converter.latex_to_text(latex).expect("parses").text;

    assert_eq!(text(r"\lit"), "LIT\n");
    // `Skip` prunes: the argument is not even visited.
    assert_eq!(text(r"a\skip{gone}b"), "ab\n");
    // `Content` on a macro is every provided argument, in declaration order.
    assert_eq!(text(r"\content{1}{2}"), "12\n");
    assert_eq!(text(r"\begin{keep}body\end{keep}"), "body\n");
    assert_eq!(text("a--b"), "a–b\n");
}

#[test]
fn a_specials_content_rule_emits_its_trigger() {
    let converter =
        Converter::builder()
            .definitions(DefinitionSet::new().with(
                Category::new("s").with_specials(SpecialsDef::new("~").rule(TextRule::Content)),
            ))
            .build()
            .expect("builds");
    assert_eq!(
        converter.latex_to_text("a~b").expect("parses").text,
        "a~b\n"
    );
}
