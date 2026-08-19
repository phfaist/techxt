//! Building a definition set (PLAN.md §10.2): shadowing order, argument codes, mode
//! visibility, and every way a build can be refused.

use std::borrow::Cow;

use techxt::convert::BuildError;
use techxt::def::{Category, DefinitionSet, EnvDef, MacroDef, SpecialsDef, TextRule};
use techxt::Converter;

/// A literal rule.
fn literal(text: &'static str) -> TextRule {
    TextRule::Literal(Cow::Borrowed(text))
}

/// A converter knowing exactly these categories, in push order.
fn build(categories: impl IntoIterator<Item = Category>) -> Result<Converter, BuildError> {
    let mut definitions = DefinitionSet::new();
    for category in categories {
        definitions.push(category);
    }
    Converter::builder().definitions(definitions).build()
}

/// Convert `latex` with a converter knowing exactly these categories.
fn text(categories: impl IntoIterator<Item = Category>, latex: &str) -> String {
    build(categories)
        .expect("the definitions build")
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// --------------------------------------------------------------- shadowing order

#[test]
fn a_later_category_shadows_an_earlier_one() {
    // The inverse of pylatexenc, where the first match wins. techy resolves innermost
    // first, and each category is pushed on top of the last.
    let first = Category::new("first")
        .with_macro(MacroDef::new("x").rule(literal("FIRST")))
        .with_env(EnvDef::new("e").rule(literal("FIRST")))
        .with_specials(SpecialsDef::new("§").rule(literal("FIRST")));
    let second = Category::new("second")
        .with_macro(MacroDef::new("x").rule(literal("SECOND")))
        .with_env(EnvDef::new("e").rule(literal("SECOND")))
        .with_specials(SpecialsDef::new("§").rule(literal("SECOND")));

    let categories = [first, second];
    assert_eq!(text(categories.clone(), r"\x"), "SECOND\n");
    assert_eq!(text(categories.clone(), r"\begin{e}\end{e}"), "SECOND\n");
    assert_eq!(text(categories, "§"), "SECOND\n");
}

#[test]
fn a_later_category_only_shadows_the_names_it_redefines() {
    let base = Category::new("base")
        .with_macro(MacroDef::symbol("a", "A"))
        .with_macro(MacroDef::symbol("b", "B"));
    let patch = Category::new("patch").with_macro(MacroDef::symbol("b", "PATCHED"));
    assert_eq!(text([base, patch], r"\a\b"), "APATCHED\n");
}

#[test]
fn within_one_category_a_later_entry_replaces_an_earlier_one() {
    let category = Category::new("c")
        .with_macro(MacroDef::symbol("x", "FIRST"))
        .with_macro(MacroDef::symbol("x", "SECOND"));
    assert_eq!(text([category], r"\x"), "SECOND\n");
}

#[test]
fn the_categories_can_be_read_back_in_push_order() {
    let mut definitions = DefinitionSet::new();
    definitions.push(Category::new("one"));
    definitions.push(Category::new("two"));
    let names: Vec<&str> = definitions.categories().map(Category::name).collect();
    assert_eq!(names, ["one", "two"]);
}

// -------------------------------------------------------------- argument codes

#[test]
fn letter_codes_and_word_codes_both_work() {
    // The word codes exist only in techy's list form — a compact argument string
    // rejects them — and `arg` declares one argument at a time, so both spellings are
    // available here. The difference is observable: `m` falls back to a single
    // expression when there is no group, `BracedOnly` does not.
    let category = Category::new("codes")
        .with_macro(MacroDef::new("loose").arg("m", "x").rule(TextRule::Content))
        .with_macro(
            MacroDef::new("strict")
                .arg("BracedOnly", "x")
                .rule(TextRule::Content),
        );
    assert_eq!(text([category.clone()], r"\loose12"), "12\n");
    // `\strict` finds no group, so its argument is absent and `12` is ordinary text.
    assert_eq!(text([category], r"\strict12"), "12\n");
}

#[test]
fn an_optional_argument_may_be_absent() {
    let category = Category::new("codes").with_macro(
        MacroDef::new("opt")
            .arg("o", "note")
            .arg("m", "text")
            .rule(TextRule::Content),
    );
    assert_eq!(text([category.clone()], r"\opt{x}"), "x\n");
    assert_eq!(text([category], r"\opt[n]{x}"), "nx\n");
}

#[test]
fn star_declares_the_marker_argument() {
    let category =
        Category::new("codes").with_macro(MacroDef::new("s").star().arg("m", "text").rule(
            TextRule::Template(techxt::def::Template::new("{?star:*|-}{text}")),
        ));
    assert_eq!(text([category.clone()], r"\s{x}"), "-x\n");
    assert_eq!(text([category], r"\s*{x}"), "*x\n");
}

#[test]
fn symbol_is_a_zero_argument_literal() {
    assert_eq!(
        text(
            [Category::new("s").with_macro(MacroDef::symbol("alpha", "α"))],
            r"\alpha"
        ),
        "α\n"
    );
}

#[test]
fn an_unknown_argument_code_is_a_build_error() {
    let error = build([Category::new("bad").with_macro(MacroDef::new("x").arg("Z", "oops"))])
        .expect_err("refused");
    match &error {
        BuildError::ArgCode(_) => {}
        other => panic!("expected an argument-code error, got {other:?}"),
    }
    assert!(format!("{error}").contains("argument code"), "{error}");
}

#[test]
fn an_unknown_argument_code_is_refused_for_environments_and_specials_too() {
    assert!(matches!(
        build([Category::new("bad").with_env(EnvDef::new("e").arg("Z", "oops"))]),
        Err(BuildError::ArgCode(_))
    ));
    assert!(matches!(
        build([Category::new("bad").with_specials(SpecialsDef::new("&").arg("Z", "oops"))]),
        Err(BuildError::ArgCode(_))
    ));
}

// ------------------------------------------------------------- category names

#[test]
fn two_categories_may_not_share_a_name() {
    let error = build([Category::new("same"), Category::new("same")]).expect_err("refused");
    match &error {
        BuildError::DuplicateCategory(name) => assert_eq!(&**name, "same"),
        other => panic!("expected a duplicate-category error, got {other:?}"),
    }
    assert!(format!("{error}").contains("same"), "{error}");
}

// ------------------------------------------------------------ mode visibility

#[test]
fn a_math_only_macro_does_not_fire_in_text() {
    // Visibility is a *parse-time* gate, so the observable difference is in the tree:
    // in math the definition is found and shapes the invocation, in text it is not and
    // the catch-all's argument-less spec answers instead.
    let category = Category::new("modes").with_macro(
        MacroDef::new("vec")
            .arg("m", "x")
            .rule(TextRule::Content)
            .math_mode_only(),
    );
    let converter = build([category]).expect("builds");
    assert_eq!(argument_count(&converter, r"$\vec{a}$", "vec"), 1);
    assert_eq!(argument_count(&converter, r"\vec{a}", "vec"), 0);
}

/// How many arguments the first callable named `name` declares, as parsed.
fn argument_count(converter: &Converter, latex: &str, name: &str) -> usize {
    let parsed = converter.language().parse(latex).expect("parses");
    let node = parsed
        .tree
        .root()
        .descendants()
        .find(|node| node.name() == Some(name))
        .expect("the callable is in the tree");
    node.arguments().map_or(0, |arguments| arguments.len())
}

#[test]
fn a_text_only_specials_does_not_fire_in_math() {
    let category = Category::new("modes")
        .with_specials(SpecialsDef::new("---").rule(literal("—")).text_mode_only());
    let converter = build([category]).expect("builds");
    assert_eq!(
        converter.latex_to_text("a---b").expect("parses").text,
        "a—b\n"
    );
    // Inside math the trigger is not scanned at all, so the characters stay characters.
    assert!(converter
        .latex_to_text("$a---b$")
        .expect("parses")
        .text
        .contains("---"));
}

#[test]
fn a_math_only_specials_does_not_fire_in_text() {
    let category = Category::new("modes").with_specials(
        SpecialsDef::new("^")
            .arg("m", "exponent")
            .rule(TextRule::Content)
            .math_mode_only(),
    );
    let converter = build([category]).expect("builds");
    let conversion = converter.latex_to_text("a^b").expect("parses");
    // Not a specials in text mode: the caret is an ordinary character.
    assert!(conversion.text.contains('^'), "{}", conversion.text);
}
