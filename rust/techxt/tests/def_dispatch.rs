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
use std::convert::Infallible;
use std::sync::Arc;

use techy::core::node::{NodeRef, NodeTree};
use techy::core::specs::{ArgumentSpec, Package};
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs_named, CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
    VerbatimBehavior,
};
use techy::recompose::{Recompose, RecomposeContext, Recomposer, TreeRecomposer};
use techy_xp::lang::{LatexlikeXp, XpDriver};

use techxt::convert::{
    MathMode, UnknownEnvPolicy, UnknownMacroPolicy, UnknownMacroResolution, UnknownSpecialsPolicy,
};
use techxt::def::{
    Category, DefinitionSet, EnvDef, MacroDef, SpecialsDef, Template, TextHandler, TextRule,
};
use techxt::diag::UnknownMacro;
use techxt::flow::Flow;
use techxt::layout::{render, LayoutOptions};
use techxt::render::{NodeView, RenderCx, RenderError, RenderState, TextRenderer};
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
    let mut package = Package::<LatexlikeXp>::new("foreign");
    package.insert(CallableType::Macro, "mark", MacroSpec::new(Vec::new()));
    let foreign = Language::new(
        XpDriver::new(Recovery::Tolerant),
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
    let mut package = Package::<LatexlikeXp>::new("foreign");
    package.insert(CallableType::Macro, "elsewhere", MacroSpec::new(Vec::new()));
    let foreign = Language::new(
        XpDriver::new(Recovery::Tolerant),
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

// ------------------------------------------------- unknown-macro resolution (D10)

/// Convert `\nowhere` — a command no category defines — under one point of the
/// {recovery} x {resolution} matrix.
fn under_resolution(
    recovery: Recovery,
    resolution: UnknownMacroResolution,
) -> Result<techxt::Conversion, techy::error::ParseError<Option<String>>> {
    Converter::builder()
        .recovery(recovery)
        .unknown_macro_resolution(resolution)
        .build()
        .expect("builds")
        .latex_to_text(r"a \nowhere b")
}

/// The conversion of an accepted unknown command: it renders as nothing (the default
/// `Skip` policy) and is reported as a techxt warning.
fn assert_accepted(recovery: Recovery, resolution: UnknownMacroResolution) {
    let conversion = under_resolution(recovery, resolution)
        .unwrap_or_else(|error| panic!("{recovery:?}/{resolution:?} should parse: {error}"));
    assert_eq!(conversion.text, "a b\n", "{recovery:?}/{resolution:?}");
    assert!(
        !conversion.diagnostics.has_errors(),
        "{recovery:?}/{resolution:?}: {:?}",
        conversion.diagnostics
    );
    assert_eq!(
        conversion.diagnostics.conditions::<UnknownMacro>().count(),
        1,
        "{recovery:?}/{resolution:?}"
    );
}

#[test]
fn following_recovery_ties_an_unknown_macro_to_the_recovery_mode() {
    // DECISIONS.md D10: the default pairs the two. Tolerant accepts the command as an
    // argument-less callable and warns; strict refuses to parse it at all.
    assert_accepted(Recovery::Tolerant, UnknownMacroResolution::FollowRecovery);

    let error = under_resolution(Recovery::Strict, UnknownMacroResolution::FollowRecovery)
        .expect_err("strict refuses a command it cannot resolve");
    assert_eq!(error.identifier(), "core.specs.unresolvable-command");
}

#[test]
fn accept_keeps_unknown_macros_convertible_under_strict_recovery() {
    assert_accepted(Recovery::Strict, UnknownMacroResolution::Accept);
    assert_accepted(Recovery::Tolerant, UnknownMacroResolution::Accept);
}

#[test]
fn reject_refuses_unknown_macros_under_tolerant_recovery_too() {
    // Without the catch-all it is techy's own handling that applies: under tolerant
    // recovery an error diagnostic plus the command recovered as literal characters,
    // and no techxt warning at all — the macro never reached the renderer.
    let conversion = under_resolution(Recovery::Tolerant, UnknownMacroResolution::Reject)
        .expect("tolerant recovery keeps going");
    assert!(conversion.text.contains("nowhere"), "{:?}", conversion.text);
    assert!(conversion.diagnostics.has_errors());
    assert_eq!(
        conversion
            .diagnostics
            .with_identifier("core.specs.unresolvable-command")
            .count(),
        1
    );
    assert_eq!(
        conversion.diagnostics.conditions::<UnknownMacro>().count(),
        0
    );

    let error = under_resolution(Recovery::Strict, UnknownMacroResolution::Reject)
        .expect_err("strict refuses it as well");
    assert_eq!(error.identifier(), "core.specs.unresolvable-command");
}

#[test]
fn a_declared_macro_is_unaffected_by_the_resolution_setting() {
    // The setting is about commands *no definition claims*. A declared but rule-less
    // macro still parses and still reaches the unknown-macro policy, whatever it says.
    for resolution in [
        UnknownMacroResolution::FollowRecovery,
        UnknownMacroResolution::Accept,
        UnknownMacroResolution::Reject,
    ] {
        let conversion = Converter::builder()
            .recovery(Recovery::Strict)
            .unknown_macro_resolution(resolution)
            .definitions(with_ruleless())
            .unknown_macro(UnknownMacroPolicy::RenderArgs)
            .build()
            .expect("builds")
            .latex_to_text(r"a\ruleless{x}b")
            .unwrap_or_else(|error| panic!("{resolution:?} should parse: {error}"));
        assert_eq!(conversion.text, "axb\n", "{resolution:?}");
    }
}

// ------------------------------------------- over plain techy (`Latexlike`)
//
// techxt parses with techy-xp's `LatexlikeXp`, but it *renders* any latexlike language
// (PLAN.md §11.1): the fold reads node payloads, and no payload is techy-xp's. These
// tests hand it trees of plain techy's own `Latexlike`, parsed by `LatexlikeDriver`
// with no techy-xp anywhere — the case techxt 0.1.0 served only by accident, because
// everyone happened to name the same concrete type.

/// A plain-techy language holding `packages`, driven by techy's own latexlike driver.
fn plain_techy(packages: impl IntoIterator<Item = Package<Latexlike>>) -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages(packages).expect("seed state"),
    )
}

/// The argument specs for these `(code, name)` pairs, in plain techy's language.
fn foreign_args(codes: &[(&str, &str)]) -> Vec<Arc<ArgumentSpec<Latexlike>>> {
    argument_specs_named::<Latexlike, _, _, _>(codes.iter().copied()).expect("argument codes")
}

#[test]
fn a_plain_techy_tree_is_rendered_through_the_name_fallback_table() {
    // The same test as `a_foreign_tree_is_rendered_through_the_name_fallback_table`
    // above, one language further away: not merely somebody else's *definitions* but
    // somebody else's `Lang`. Nothing in the tree is a techxt spec, so the rule is
    // found at step 3, by name.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(CallableType::Macro, "mark", MacroSpec::new(Vec::new()));
    let tree = plain_techy([package])
        .parse(r"a\mark b")
        .expect("parses")
        .tree;

    assert_eq!(
        marking("FROM-THE-TABLE").tree_to_text(&tree).text,
        "aFROM-THE-TABLEb\n"
    );
}

#[test]
fn a_plain_techy_tree_with_no_matching_name_falls_through_to_the_policy() {
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(CallableType::Macro, "elsewhere", MacroSpec::new(Vec::new()));
    let tree = plain_techy([package])
        .parse(r"a\elsewhere b")
        .expect("parses")
        .tree;

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

/// `\shout{hi}` → `*hi*`: a handler of a consumer's own, on a foreign tree.
#[derive(Debug)]
struct Shout;

impl TextHandler for Shout {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let mut flow = Flow::text("*");
        flow.extend(cx.arg("text")?.unwrap_or_default());
        flow.extend(Flow::text("*"));
        Ok(flow)
    }
}

/// A plain-techy language with everything the four-construct test needs: a macro taking
/// one argument, and an environment with an ordinary body. Groups and math need no
/// definitions at all — they are the language's own syntax.
fn foreign_constructs() -> Language<Latexlike> {
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Macro,
        "shout",
        MacroSpec::new(foreign_args(&[("m", "text")])),
    );
    package.insert(
        CallableType::Environment,
        "quoteish",
        EnvironmentSpec::new(Vec::new()),
    );
    plain_techy([package])
}

#[test]
fn every_construct_of_a_plain_techy_tree_renders() {
    // Groups, math, an environment body and a macro invocation — the four shapes a
    // latexlike tree is made of — each folded by the renderer over a language it was
    // not compiled against.
    let converter = Converter::builder()
        .definitions(
            DefinitionSet::new().with(
                Category::new("foreign")
                    .with_macro(
                        MacroDef::new("shout")
                            .arg("m", "text")
                            .rule(TextRule::Handler(Arc::new(Shout))),
                    )
                    .with_env(EnvDef::new("quoteish").rule(TextRule::Content)),
            ),
        )
        .build()
        .expect("builds");
    let tree = foreign_constructs()
        .parse(r"{grouped} $a b$ \begin{quoteish}inside\end{quoteish} \shout{hi}")
        .expect("parses")
        .tree;

    // The group is transparent, the formula is joined and styled by the math engine,
    // the environment renders its body, and the handler runs.
    assert_eq!(
        converter.tree_to_text(&tree).text,
        "grouped \u{1d44e}\u{1d44f} inside *hi*\n"
    );
}

#[test]
fn a_handler_of_ones_own_fires_on_a_plain_techy_tree() {
    // The same handler, reached through the *override* map rather than the fallback
    // table: dispatch step 1 is language-blind too.
    let converter = Converter::builder()
        .override_macro("shout", TextRule::Handler(Arc::new(Shout)))
        .build()
        .expect("builds");
    let tree = foreign_constructs()
        .parse(r"\shout{hi}")
        .expect("parses")
        .tree;

    assert_eq!(converter.tree_to_text(&tree).text, "*hi*\n");
}

#[test]
fn a_shipped_handler_renders_a_plain_techy_list() {
    // `itemize` and `\item` are rendered by `defs::lists`'s handlers, found by name
    // because the foreign specs carry no rule. The markers, the numbering and the
    // hanging indent are the shipped ones, on a tree techxt never parsed.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Environment,
        "itemize",
        EnvironmentSpec::new(Vec::new()),
    );
    package.insert(
        CallableType::Macro,
        "item",
        MacroSpec::new(foreign_args(&[("o", "label")])),
    );
    let latex = r"\begin{itemize}\item one \item two\end{itemize}";
    let tree = plain_techy([package]).parse(latex).expect("parses").tree;

    let converter = Converter::standard();
    let conversion = converter.tree_to_text(&tree);
    assert_eq!(conversion.text, "  \u{2022} one\n  \u{2022} two\n");
    assert!(
        !conversion.diagnostics.has_errors(),
        "{:?}",
        conversion.diagnostics
    );
    // And byte for byte what the same document renders as when techxt parsed it
    // itself: the language the tree came from changes nothing about the rendering.
    assert_eq!(
        conversion.text,
        converter.latex_to_text(latex).expect("parses").text
    );
}

#[test]
fn a_shipped_handler_renders_text_inside_a_plain_techy_formula() {
    // `\text{…}` is `defs::base`'s `ModeShift` handler: it leaves math for its
    // argument, so the words inside come out unstyled while the variable around them
    // does not.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Macro,
        "text",
        MacroSpec::new(foreign_args(&[("m", "text")])),
    );
    let tree = plain_techy([package])
        .parse(r"$x \text{if} y$")
        .expect("parses")
        .tree;

    assert_eq!(
        Converter::standard().tree_to_text(&tree).text,
        "\u{1d465} if \u{1d466}\n"
    );
}

// ---------------------------------- the unknown-construct policies, over plain techy

/// A plain-techy language with the two vehicles the unknown-construct policies act on:
/// a macro that declares one argument and an environment with an ordinary body. Neither
/// name is defined anywhere in `techxt::defs`, so both reach step 4.
fn foreign_unknowns() -> Language<Latexlike> {
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Macro,
        "ruleless",
        MacroSpec::new(foreign_args(&[("m", "text")])),
    );
    package.insert(
        CallableType::Environment,
        "myenv",
        EnvironmentSpec::new(Vec::new()),
    );
    plain_techy([package])
}

#[test]
fn every_unknown_macro_policy_acts_on_a_plain_techy_macro() {
    // Step 4 over a foreign language. `KeepSource` is the one that could have gone
    // wrong: it re-emits the invocation through `Latexlike`'s own `InvocationSyntaxData`
    // — a different type from the one `LatexlikeXp` carries, under a span regime that
    // tiles where techy-xp's does not — and PLAN.md §1.6's payload-only rule is what
    // makes the two answer alike.
    let latex = r"a\ruleless{x}b";
    let tree = foreign_unknowns().parse(latex).expect("parses").tree;

    for (policy, expected) in [
        (UnknownMacroPolicy::Skip, "ab\n"),
        (UnknownMacroPolicy::RenderArgs, "axb\n"),
        (UnknownMacroPolicy::KeepSource, "a\\ruleless{x}b\n"),
        (UnknownMacroPolicy::Placeholder, "a<ruleless>b\n"),
    ] {
        let converter = Converter::builder()
            .definitions(with_ruleless())
            .unknown_macro(policy)
            .build()
            .expect("builds");
        let conversion = converter.tree_to_text(&tree);
        assert_eq!(conversion.text, expected, "{policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-macro")
                .count(),
            1,
            "{policy:?}"
        );
        // And byte for byte what techxt's own parse of the same source renders as.
        assert_eq!(
            conversion.text,
            converter.latex_to_text(latex).expect("parses").text,
            "{policy:?}"
        );
    }
}

#[test]
fn every_unknown_environment_policy_acts_on_a_plain_techy_environment() {
    // `KeepSource` here goes through `Latexlike`'s environment syntax — `write_begin`
    // and `write_end`, which reassemble `\begin{myenv}` and `\end{myenv}` from what the
    // node recorded rather than from the source buffer.
    let latex = r"a \begin{myenv}inner\end{myenv} b";
    let tree = foreign_unknowns().parse(latex).expect("parses").tree;

    for (policy, expected) in [
        (UnknownEnvPolicy::RenderBody, "a inner b\n"),
        (UnknownEnvPolicy::Skip, "a b\n"),
        (
            UnknownEnvPolicy::KeepSource,
            "a\n\n\\begin{myenv}inner\\end{myenv}\n\nb\n",
        ),
    ] {
        let converter = Converter::builder()
            .unknown_env(policy)
            .build()
            .expect("builds");
        let conversion = converter.tree_to_text(&tree);
        assert_eq!(conversion.text, expected, "{policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-environment")
                .count(),
            1,
            "{policy:?}"
        );
        assert_eq!(
            conversion.text,
            converter.latex_to_text(latex).expect("parses").text,
            "{policy:?}"
        );
    }
}

// ------------------------------------- data rules over a plain-techy two-argument macro

/// A plain-techy language whose `\deco` takes an optional and a mandatory argument,
/// named exactly as the techxt definition names them — which is what lets a template
/// refer to them and `Content` find which of them were written.
fn foreign_deco() -> Language<Latexlike> {
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Macro,
        "deco",
        MacroSpec::new(foreign_args(&[("o", "opt"), ("m", "main")])),
    );
    plain_techy([package])
}

/// A converter whose `\deco` declares the same two arguments and renders through `rule`.
///
/// The declaration is what the template is validated against when the converter is
/// built; on a foreign tree the rule itself is found by name, at dispatch step 3.
fn decorating(rule: TextRule) -> Converter {
    Converter::builder()
        .definitions(
            DefinitionSet::new().with(
                Category::new("deco").with_macro(
                    MacroDef::new("deco")
                        .arg("o", "opt")
                        .arg("m", "main")
                        .rule(rule),
                ),
            ),
        )
        .build()
        .expect("builds")
}

#[test]
fn a_template_rule_reads_a_plain_techy_macros_arguments() {
    // Every way a template can reach an argument, at once: by name, by 1-based index,
    // and by asking whether an optional one was written. All three go through the
    // language-erased `argument_provided_at` / `argument_count` path.
    let converter = decorating(TextRule::Template(Template::new(
        "{main}/{2}/{?opt:<{opt}>|-}",
    )));
    let language = foreign_deco();

    let written = language.parse(r"\deco[o]{m}").expect("parses").tree;
    assert_eq!(converter.tree_to_text(&written).text, "m/m/<o>\n");

    let omitted = language.parse(r"\deco{m}").expect("parses").tree;
    assert_eq!(converter.tree_to_text(&omitted).text, "m/m/-\n");

    // The same answers techxt's own parse gives, so the template is reading the
    // invocation and not the language.
    assert_eq!(
        converter.tree_to_text(&written).text,
        converter
            .latex_to_text(r"\deco[o]{m}")
            .expect("parses")
            .text
    );
    assert_eq!(
        converter.tree_to_text(&omitted).text,
        converter.latex_to_text(r"\deco{m}").expect("parses").text
    );
}

#[test]
fn a_content_rule_concatenates_a_plain_techy_macros_provided_arguments() {
    // `Content` is every *provided* argument in declaration order, so the optional one
    // contributes exactly when it was written.
    let converter = decorating(TextRule::Content);
    let language = foreign_deco();

    let written = language.parse(r"\deco[o]{m}").expect("parses").tree;
    assert_eq!(converter.tree_to_text(&written).text, "om\n");

    let omitted = language.parse(r"\deco{m}").expect("parses").tree;
    assert_eq!(converter.tree_to_text(&omitted).text, "m\n");

    assert_eq!(
        converter.tree_to_text(&written).text,
        converter
            .latex_to_text(r"\deco[o]{m}")
            .expect("parses")
            .text
    );
}

// -------------------------------------------- wrapping the renderer over a foreign tree

/// A consumer's recomposer over a *plain techy* tree: it uppercases characters nodes and
/// delegates every other node to techxt's renderer — PLAN.md §3's wrapping contract,
/// written against a language techxt does not parse with.
struct ShoutingWrapper<'a> {
    inner: TextRenderer<'a>,
}

impl Recomposer<Latexlike, ()> for ShoutingWrapper<'_> {
    type State = RenderState;
    type Piece = Flow;
    type Error = Infallible;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, Latexlike, ()>,
        state: &RenderState,
        cx: &mut RecomposeContext<'_, Latexlike, ()>,
    ) -> Result<Recompose<Flow, RenderState>, Infallible> {
        if let Some(text) = node.chars() {
            return Ok(Recompose::Emit(Flow::from_plain_text(&text.to_uppercase())));
        }
        self.inner.recompose_node(node, state, cx)
    }
}

#[test]
fn a_consumers_recomposer_can_wrap_the_renderer_over_a_plain_techy_tree() {
    // The blanket `Recomposer<LLL, ()>` impl is what makes this compile at all: the
    // renderer this wrapper holds was never specialized to a language, and the tree
    // handed to `recompose` is what settles which one this run is over.
    let converter = Converter::builder()
        .override_macro("shout", TextRule::Handler(Arc::new(Shout)))
        .build()
        .expect("builds");
    let tree: NodeTree<Latexlike> = foreign_constructs()
        .parse(r"a \shout{hi} \begin{quoteish}c\end{quoteish}")
        .expect("parses")
        .tree;

    let mut wrapper = ShoutingWrapper {
        inner: converter.renderer(),
    };
    let flow = TreeRecomposer::new(&mut wrapper)
        .recompose(&tree, RenderState::initial(converter.options()))
        .expect("no refusal");
    let finish = wrapper.inner.finish();

    // The override reaches every node the *driver* descends into, and stops at the edge
    // of a construct a techxt rule renders: `hi` is folded by the inner renderer through
    // the handler, and so is the environment body.
    assert_eq!(render(&flow, &LayoutOptions::default()), "A *hi* c\n");
    // And the inner renderer still reported what it saw: `quoteish` is defined nowhere.
    assert_eq!(
        finish
            .diagnostics
            .with_identifier("techxt.unknown-environment")
            .count(),
        1
    );
}

// ------------------------------------------------------- a foreign verbatim environment

#[test]
fn a_plain_techy_verbatim_environment_keeps_its_body_raw() {
    // The one thing a foreign tree can carry that says *raw body* is techy's own
    // `VerbatimBehavior`. techxt's own environment definition is not in this tree —
    // `verbatim`'s rule is found by name, at step 3 — so without consulting techy's
    // behaviour the body would come back reflowed into running words.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Environment,
        "verbatim",
        EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::<Latexlike>::new(Vec::new()))),
    );
    let latex = "\\begin{verbatim}  a   b\n   c\\end{verbatim}";
    let tree = plain_techy([package]).parse(latex).expect("parses").tree;

    let converter = Converter::standard();
    let conversion = converter.tree_to_text(&tree);
    assert_eq!(conversion.text, "  a   b\n   c\n");
    // Byte for byte what techxt's own parse of the same source renders as.
    assert_eq!(
        conversion.text,
        converter.latex_to_text(latex).expect("parses").text
    );
}

// --------------------------------------------- math `Source` mode over a foreign tree

#[test]
fn math_source_mode_re_emits_a_plain_techy_formula() {
    // `Source` mode never enters the formula: it re-emits it from the node payloads it
    // finds, which on a plain-techy tree are `Latexlike`'s own. The interior spacing
    // survives, which is the whole point of the mode.
    let converter = Converter::builder()
        .math_mode(MathMode::Source)
        .build()
        .expect("builds");
    let latex = "x $a  b$ y";
    let tree = plain_techy([]).parse(latex).expect("parses").tree;

    let conversion = converter.tree_to_text(&tree);
    assert_eq!(conversion.text, "x $a  b$ y\n");
    assert_eq!(
        conversion.text,
        converter.latex_to_text(latex).expect("parses").text
    );
}
