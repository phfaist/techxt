//! Arguments a code cannot describe (`MacroDef::arg_spec`).
//!
//! Two definitions in PLAN.md §9 need more than an argument code: `\text{…}`, whose
//! argument leaves math for its own extent, and `\\`, whose optional `[len]` must not be
//! found across a space. Both are built as techy `ArgumentSpec`s and handed to the entry
//! builder directly, in declaration order with the coded arguments.

use std::sync::Arc;

use techy::core::constructs::{GroupArgumentParser, OptionalGroupArgumentParser};
use techy::core::node::{NodeRef, NodeTree, ParsedArgument};
use techy::core::specs::ArgumentSpec;
use techy::core::token::GroupRule;
use techy::core::ParsingStateDelta;
use techy::latexlike::{Event, GroupType, Latexlike, Mode};

use techxt::def::{Category, DefinitionSet, MacroDef, TextRule};
use techxt::Converter;

/// The first callable named `name` in `tree`.
fn callable<'t>(tree: &'t NodeTree<Latexlike>, name: &str) -> NodeRef<'t, Latexlike> {
    tree.root()
        .descendants()
        .find(|node| node.name() == Some(name))
        .expect("the callable is in the tree")
}

#[test]
fn an_argument_can_carry_a_parsing_state_delta() {
    // `\text{…}` leaves math for the extent of its argument. Leaving math is an *event*
    // — it restores the enclosing non-math context, token rules and all — never a mode
    // change back to text.
    let category = Category::new("fonts").with_macro(
        MacroDef::new("text")
            .arg_spec(
                ArgumentSpec::new(GroupArgumentParser::new(GroupType::Content), "text")
                    .with_state_delta(ParsingStateDelta::new().event(Event::ExitMathContext)),
            )
            .rule(TextRule::Content),
    );
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .build()
        .expect("builds");

    let tree = converter
        .language()
        .parse(r"$a\text{b}$")
        .expect("parses")
        .tree;
    let node = callable(&tree, "text");
    let inside = node
        .argument_content_nodes_named("text")
        .expect("a declared argument")
        .expect("a provided argument")
        .iter()
        .next()
        .expect("non-empty");
    assert_eq!(inside.parsing_state().mode(), Mode::Text);
    // The formula around it is still math.
    assert_eq!(node.parsing_state().mode(), Mode::Math);
}

#[test]
fn an_argument_can_refuse_a_preceding_space() {
    // `\\`'s optional `[len]` must not reach across whitespace, or `\\ [a]` would lose
    // its bracketed text. Only a hand-built parser can say so.
    let bracket = Arc::new(GroupRule::<Latexlike> {
        group_type: GroupType::Content,
        open: String::from("["),
        close: String::from("]"),
    });
    let category = Category::new("base").with_macro(
        MacroDef::new("\\")
            .star()
            .arg_spec(ArgumentSpec::new(
                OptionalGroupArgumentParser::new(bracket).require_adjacent(),
                "len",
            ))
            .rule(TextRule::Skip),
    );
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .build()
        .expect("builds");

    let provided = |latex: &str| {
        let tree = converter.language().parse(latex).expect("parses").tree;
        callable(&tree, "\\")
            .arguments()
            .and_then(|arguments| arguments.get_named("len"))
            .is_some_and(ParsedArgument::is_provided)
    };
    assert!(provided(r"a\\[2pt] b"));
    assert!(!provided(r"a\\ [2pt] b"));
}

#[test]
fn hand_built_and_coded_arguments_keep_their_declaration_order() {
    let category = Category::new("mixed").with_macro(
        MacroDef::new("mix")
            .arg("m", "first")
            .arg_spec(ArgumentSpec::new(
                GroupArgumentParser::new(GroupType::Content),
                "second",
            ))
            .arg("m", "third")
            .rule(TextRule::Template(techxt::def::Template::new(
                "{third}{second}{first}",
            ))),
    );
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"\mix{1}{2}{3}")
            .expect("parses")
            .text,
        "321\n"
    );
}
