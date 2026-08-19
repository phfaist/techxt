//! The techy spec types techxt registers (PLAN.md §10.2), and the render-side downcast
//! that recovers a rule from a parsed node (PLAN.md §10.3 step 2).
//!
//! The environment case is the delicate one. techy's `\begin` composition reaches an
//! environment's behaviour by downcasting the spec to its own concrete
//! `EnvironmentSpec`, so a techxt environment must be registered as one; the techxt
//! payload therefore rides one downcast further in, inside the behaviour. Wrapping that
//! behaviour — which `EnvironmentSpec::with_body_delta` does — would hide it again and
//! silently cost the environment its rule, so every body kind is checked here.

use std::any::Any;

use techy::core::node::{NodeRef, NodeTree};
use techy::core::specs::CallableSpec;
use techy::latexlike::Latexlike;

use techxt::def::{
    Category, DefinitionSet, EnvBodyKind, EnvDef, MacroDef, SpecialsDef, TechxtEnvironmentBehavior,
    TechxtMacroSpec, TechxtSpecialsSpec, TextRule,
};
use techxt::render::ListKind;
use techxt::Converter;

/// The definitions every test here shares: one environment per body kind, plus the
/// `\item` a list body brings into scope.
fn definitions() -> DefinitionSet {
    DefinitionSet::new().with(
        Category::new("bodies")
            .with_macro(MacroDef::new("item").arg("o", "label").rule(TextRule::Skip))
            .with_macro(
                MacroDef::new("emph")
                    .arg("m", "text")
                    .rule(TextRule::Content),
            )
            .with_specials(SpecialsDef::new("~").rule(TextRule::Content))
            .with_env(EnvDef::new("plain").rule(TextRule::Content))
            .with_env(EnvDef::new("equation").math_body().rule(TextRule::Content))
            .with_env(
                EnvDef::new("verbatim")
                    .verbatim_body()
                    .rule(TextRule::Content),
            )
            .with_env(
                EnvDef::new("itemize")
                    .list_body(ListKind::Itemize)
                    .rule(TextRule::Content),
            ),
    )
}

fn converter() -> Converter {
    Converter::builder()
        .definitions(definitions())
        .build()
        .expect("the definitions build")
}

/// The first callable named `name` in `tree`.
fn callable<'t>(tree: &'t NodeTree<Latexlike>, name: &str) -> NodeRef<'t, Latexlike> {
    tree.root()
        .descendants()
        .find(|node| node.name() == Some(name))
        .expect("the callable is in the tree")
}

// ------------------------------------------------- the environment downcast (C7)

/// The behaviour recovered from the environment `name` in `latex`.
fn behavior_of(latex: &str, name: &str) -> (EnvBodyKind, bool) {
    let converter = converter();
    let tree = converter.language().parse(latex).expect("parses").tree;
    let node = callable(&tree, name);
    let behavior =
        TechxtEnvironmentBehavior::of(node).expect("the techxt behaviour survives registration");
    (behavior.body_kind(), behavior.rule().is_some())
}

#[test]
fn an_ordinary_environment_keeps_its_identity() {
    assert_eq!(
        behavior_of(r"\begin{plain}x\end{plain}", "plain"),
        (EnvBodyKind::Normal, true)
    );
}

#[test]
fn a_math_body_environment_keeps_its_identity() {
    assert_eq!(
        behavior_of(r"\begin{equation}x\end{equation}", "equation"),
        (EnvBodyKind::Math, true)
    );
}

#[test]
fn a_verbatim_body_environment_keeps_its_identity() {
    assert_eq!(
        behavior_of("\\begin{verbatim}\nx\n\\end{verbatim}", "verbatim"),
        (EnvBodyKind::Verbatim, true)
    );
}

#[test]
fn a_list_body_environment_keeps_its_identity() {
    assert_eq!(
        behavior_of(r"\begin{itemize}\item x\end{itemize}", "itemize"),
        (EnvBodyKind::List(ListKind::Itemize), true)
    );
}

// ------------------------------------------------------ what each body kind does

#[test]
fn a_math_body_is_parsed_in_math_mode() {
    let converter = converter();
    let tree = converter
        .language()
        .parse(r"a\begin{equation}x\end{equation}")
        .expect("parses")
        .tree;
    let body = callable(&tree, "equation").body().expect("has a body");
    let inside = body.iter().next().expect("a non-empty body");
    assert_eq!(inside.parsing_state().mode(), techy::latexlike::Mode::Math);
    // ... and the text outside it is not.
    assert_eq!(
        tree.root()
            .child(0)
            .expect("a first child")
            .parsing_state()
            .mode(),
        techy::latexlike::Mode::Text
    );
}

#[test]
fn a_verbatim_body_is_raw_characters() {
    let converter = converter();
    let conversion = converter
        .latex_to_text("\\begin{verbatim}\n  a_1 \\emph{x} %\n\\end{verbatim}")
        .expect("parses");
    // Markup inside a verbatim body is not markup: nothing was interpreted, and the
    // characters reach the output untouched.
    assert!(
        conversion.text.contains("a_1 \\emph{x} %"),
        "{:?}",
        conversion.text
    );
}

#[test]
fn a_list_body_brings_item_into_scope() {
    // The body delta pushes the package holding the set's own `\item`; scope reversion
    // is structural, so the push is visible exactly on the body's nodes.
    let converter = converter();
    let tree = converter
        .language()
        .parse(r"a\begin{itemize}\item x\end{itemize}")
        .expect("parses")
        .tree;

    let body = callable(&tree, "itemize").body().expect("has a body");
    let inside = body.iter().next().expect("a non-empty body");
    let providers: Vec<&str> = inside.parsing_state().scopes().provider_names().collect();
    assert!(providers.contains(&"techxt-item"), "{providers:?}");

    // Outside the environment it is not in scope.
    let providers: Vec<&str> = tree
        .root()
        .child(0)
        .expect("a first child")
        .parsing_state()
        .scopes()
        .provider_names()
        .collect();
    assert!(!providers.contains(&"techxt-item"), "{providers:?}");
}

#[test]
fn an_ordinary_body_gets_no_delta_at_all() {
    let converter = converter();
    let tree = converter
        .language()
        .parse(r"\begin{plain}x\end{plain}")
        .expect("parses")
        .tree;
    let body = callable(&tree, "plain").body().expect("has a body");
    let inside = body.iter().next().expect("a non-empty body");
    assert_eq!(inside.parsing_state().mode(), techy::latexlike::Mode::Text);
    let providers: Vec<&str> = inside.parsing_state().scopes().provider_names().collect();
    assert!(!providers.contains(&"techxt-item"), "{providers:?}");
}

// ------------------------------------------------- the macro and specials specs

#[test]
fn a_macro_node_carries_its_techxt_spec() {
    let converter = converter();
    let tree = converter
        .language()
        .parse(r"\emph{x}")
        .expect("parses")
        .tree;
    let spec = callable(&tree, "emph").spec().expect("a resolved spec");
    let techxt_spec = (&**spec as &dyn Any)
        .downcast_ref::<TechxtMacroSpec>()
        .expect("the techxt spec survives registration");
    assert!(matches!(techxt_spec.rule(), Some(TextRule::Content)));
    assert_eq!(techxt_spec.arguments().len(), 1);
}

#[test]
fn a_specials_node_carries_its_techxt_spec() {
    let converter = converter();
    let tree = converter.language().parse("a~b").expect("parses").tree;
    let spec = callable(&tree, "~").spec().expect("a resolved spec");
    let techxt_spec = (&**spec as &dyn Any)
        .downcast_ref::<TechxtSpecialsSpec>()
        .expect("the techxt spec survives registration");
    assert!(matches!(techxt_spec.rule(), Some(TextRule::Content)));
}

#[test]
fn a_rule_less_entry_carries_a_spec_with_no_rule() {
    let converter = Converter::builder()
        .definitions(
            DefinitionSet::new()
                .with(Category::new("bare").with_macro(MacroDef::new("bare").arg("m", "x"))),
        )
        .build()
        .expect("builds");
    let tree = converter
        .language()
        .parse(r"\bare{x}")
        .expect("parses")
        .tree;
    let spec = callable(&tree, "bare").spec().expect("a resolved spec");
    let techxt_spec = (&**spec as &dyn Any)
        .downcast_ref::<TechxtMacroSpec>()
        .expect("the techxt spec survives registration");
    // The parse shape is techxt's; the rendering is the unknown-macro policy's.
    assert_eq!(techxt_spec.arguments().len(), 1);
    assert!(techxt_spec.rule().is_none());
}
