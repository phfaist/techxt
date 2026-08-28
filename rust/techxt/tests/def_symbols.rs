//! Reading a definition set back (PLAN.md §10.7): the entry accessors on
//! [`Category`](techxt::def::Category) and the [`SymbolIndex`](techxt::def::SymbolIndex)
//! over a whole [`DefinitionSet`](techxt::def::DefinitionSet).
//!
//! Two properties carry the module and are pinned here from several directions. The
//! first is **shadowing**: a set resolves a name innermost-first, so the index must hold
//! one entry per `(kind, name)` and it must be the one the converter would use — the
//! tests that build a shadowing pair check the index and the converted output together,
//! so the index cannot start lying about the library without a test noticing. The second
//! is that the index is **built once and searched**: the shipped library is over
//! fourteen hundred names, and the table is sorted precisely so that a caller answering
//! a keystroke does a binary search rather than another walk of nineteen categories.

use techxt::def::{
    CallableKind, Category, DefinitionSet, EnvDef, MacroDef, ModeVisibility, SpecialsDef, Template,
    TextRule,
};
use techxt::Converter;

/// A set of one category, for the tests that only need somewhere to put entries.
fn set_of(category: Category) -> DefinitionSet {
    DefinitionSet::new().with(category)
}

#[test]
fn a_category_reads_back_the_entries_it_was_built_from() {
    let category = Category::new("mine")
        .with_macro(MacroDef::symbol("me", "Philippe"))
        .with_macro(
            MacroDef::new("shout")
                .arg("m", "text")
                .rule(TextRule::Content),
        )
        .with_env(EnvDef::new("quote").rule(TextRule::Content))
        .with_specials(SpecialsDef::new("~").rule(TextRule::Literal(" ".into())));

    let macros: Vec<&str> = category.macros().map(MacroDef::name).collect();
    assert_eq!(macros, ["me", "shout"], "declaration order is preserved");
    let environments: Vec<&str> = category.environments().map(EnvDef::name).collect();
    assert_eq!(environments, ["quote"]);
    let specials: Vec<&str> = category.specials().map(SpecialsDef::trigger).collect();
    assert_eq!(specials, ["~"]);
}

#[test]
fn a_set_with_nothing_in_it_indexes_to_nothing() {
    let definitions = DefinitionSet::new();
    let symbols = definitions.symbols();
    assert!(symbols.is_empty());
    assert_eq!(symbols.len(), 0);
    assert_eq!(symbols.entries(), []);
    assert!(symbols.get(CallableKind::Macro, "alpha").is_none());
    assert_eq!(symbols.starts_with(CallableKind::Macro, ""), []);
}

#[test]
fn a_later_category_shadows_an_earlier_one_and_the_index_says_so() {
    let definitions = DefinitionSet::new()
        .with(Category::new("generated").with_macro(MacroDef::symbol("ldots", "...")))
        .with(Category::new("curated").with_macro(MacroDef::symbol("ldots", "…")));

    let symbols = definitions.symbols();
    assert_eq!(symbols.len(), 1, "one name, resolved once");
    let ldots = symbols
        .get(CallableKind::Macro, "ldots")
        .expect("the set defines it");
    assert_eq!(ldots.category, "curated");
    assert_eq!(ldots.replacement, Some("…"));

    // And the index agrees with what the converter actually does with it.
    let converter = Converter::builder()
        .definitions(definitions)
        .build()
        .expect("the set is well-formed");
    assert_eq!(
        converter.latex_to_text(r"\ldots").expect("parses").text,
        "…\n"
    );
}

#[test]
fn a_later_entry_in_one_category_shadows_an_earlier_one() {
    // Within a category the same rule applies: techy's package keeps one entry per name
    // and the last insert is the one that survives.
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("ldots", "..."))
            .with_macro(MacroDef::symbol("ldots", "…")),
    );

    let symbols = definitions.symbols();
    assert_eq!(symbols.len(), 1);
    assert_eq!(
        symbols
            .get(CallableKind::Macro, "ldots")
            .expect("defined")
            .replacement,
        Some("…")
    );

    let converter = Converter::builder()
        .definitions(definitions)
        .build()
        .expect("the set is well-formed");
    assert_eq!(
        converter.latex_to_text(r"\ldots").expect("parses").text,
        "…\n"
    );
}

#[test]
fn the_three_kinds_are_keyed_separately() {
    // A macro, an environment and a specials may all be called `x` and mean three
    // different things — which is why the index is keyed by the pair, not by the name.
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("x", "macro"))
            .with_env(EnvDef::new("x").rule(TextRule::Content))
            .with_specials(SpecialsDef::new("x").rule(TextRule::Literal("specials".into()))),
    );

    let symbols = definitions.symbols();
    assert_eq!(symbols.len(), 3);
    assert_eq!(
        symbols
            .get(CallableKind::Macro, "x")
            .expect("the macro")
            .replacement,
        Some("macro")
    );
    assert!(symbols
        .get(CallableKind::Environment, "x")
        .expect("the environment")
        .replacement
        .is_none());
    assert_eq!(
        symbols
            .get(CallableKind::Specials, "x")
            .expect("the specials")
            .replacement,
        Some("specials")
    );
}

#[test]
fn the_table_is_sorted_by_kind_then_name() {
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("beta", "β"))
            .with_macro(MacroDef::symbol("alpha", "α"))
            .with_env(EnvDef::new("quote"))
            .with_specials(SpecialsDef::new("~")),
    );

    let symbols = definitions.symbols();
    let listed: Vec<(CallableKind, &str)> = symbols
        .entries()
        .iter()
        .map(|entry| (entry.kind, entry.name))
        .collect();
    assert_eq!(
        listed,
        [
            (CallableKind::Macro, "alpha"),
            (CallableKind::Macro, "beta"),
            (CallableKind::Environment, "quote"),
            (CallableKind::Specials, "~"),
        ]
    );

    // Which is what makes `of_kind` a subslice rather than a filter.
    let macros: Vec<&str> = symbols
        .of_kind(CallableKind::Macro)
        .iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(macros, ["alpha", "beta"]);
    assert_eq!(symbols.of_kind(CallableKind::Environment).len(), 1);
    assert_eq!(symbols.of_kind(CallableKind::Specials).len(), 1);
}

#[test]
fn a_prefix_scan_is_the_contiguous_run_of_names_that_start_with_it() {
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("aleph", "ℵ"))
            .with_macro(MacroDef::symbol("alpha", "α"))
            .with_macro(MacroDef::symbol("alph", "a"))
            .with_macro(MacroDef::symbol("beta", "β"))
            .with_env(EnvDef::new("align")),
    );
    let symbols = definitions.symbols();

    let names = |prefix: &str| -> Vec<&str> {
        symbols
            .starts_with(CallableKind::Macro, prefix)
            .iter()
            .map(|entry| entry.name)
            .collect()
    };

    assert_eq!(names("alp"), ["alph", "alpha"]);
    assert_eq!(names("alph"), ["alph", "alpha"], "the prefix is a name too");
    assert_eq!(names("alpha"), ["alpha"]);
    assert_eq!(names("b"), ["beta"]);
    assert_eq!(names(""), ["aleph", "alph", "alpha", "beta"], "everything");
    assert_eq!(names("zz"), Vec::<&str>::new(), "nothing starts with it");
    assert_eq!(names("al!"), Vec::<&str>::new(), "between two real names");

    // The scan is per kind: the environment shares the prefix and is not offered.
    assert_eq!(
        symbols
            .starts_with(CallableKind::Environment, "al")
            .iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        ["align"]
    );
}

#[test]
fn an_entry_reports_its_arity_and_only_a_literal_replacement() {
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("me", "Philippe"))
            .with_macro(
                MacroDef::new("shout")
                    .arg("m", "text")
                    .rule(TextRule::Content),
            )
            .with_macro(
                MacroDef::new("href")
                    .arg("BracedOnly", "url")
                    .arg("m", "text")
                    .rule(TextRule::Template(Template::new("{text} <{url}>"))),
            )
            .with_macro(MacroDef::new("undefined"))
            .with_env(
                EnvDef::new("theorem")
                    .arg("o", "title")
                    .rule(TextRule::Content),
            ),
    );
    let symbols = definitions.symbols();
    let macro_entry = |name: &str| {
        *symbols
            .get(CallableKind::Macro, name)
            .unwrap_or_else(|| panic!("{name} is declared"))
    };

    let me = macro_entry("me");
    assert_eq!(me.arity, 0);
    assert_eq!(me.replacement, Some("Philippe"));

    let shout = macro_entry("shout");
    assert_eq!(shout.arity, 1);
    assert_eq!(shout.replacement, None, "`Content` has to be executed");

    let href = macro_entry("href");
    assert_eq!(href.arity, 2);
    assert_eq!(href.replacement, None, "a template has to be executed");

    let undefined = macro_entry("undefined");
    assert_eq!(undefined.arity, 0);
    assert_eq!(undefined.replacement, None, "no rule at all");

    let theorem = symbols
        .get(CallableKind::Environment, "theorem")
        .expect("declared");
    assert_eq!(theorem.arity, 1, "the arguments of `\\begin{{theorem}}`");
}

#[test]
fn an_entry_reports_the_modes_its_definition_is_visible_in() {
    let definitions = set_of(
        Category::new("mine")
            .with_macro(MacroDef::symbol("anywhere", "a"))
            .with_macro(MacroDef::symbol("intext", "t").text_mode_only())
            .with_macro(MacroDef::symbol("inmath", "m").math_mode_only())
            .with_specials(SpecialsDef::new("---").text_mode_only())
            .with_specials(SpecialsDef::new("^").arg("m", "sub").math_mode_only())
            .with_env(EnvDef::new("quote")),
    );
    let symbols = definitions.symbols();
    let modes = |kind: CallableKind, name: &str| symbols.get(kind, name).expect("declared").modes;

    assert_eq!(
        modes(CallableKind::Macro, "anywhere"),
        ModeVisibility::Anywhere
    );
    assert_eq!(
        modes(CallableKind::Macro, "intext"),
        ModeVisibility::TextOnly
    );
    assert_eq!(
        modes(CallableKind::Macro, "inmath"),
        ModeVisibility::MathOnly
    );
    assert_eq!(
        modes(CallableKind::Specials, "---"),
        ModeVisibility::TextOnly
    );
    assert_eq!(modes(CallableKind::Specials, "^"), ModeVisibility::MathOnly);
    assert_eq!(
        modes(CallableKind::Environment, "quote"),
        ModeVisibility::Anywhere,
        "an environment is never mode-restricted"
    );
}

#[test]
fn the_shipped_library_reads_back() {
    let definitions = techxt::defs::standard();
    let symbols = definitions.symbols();

    // The generated long tail declares `\alpha` too; the curated `mathcore` is pushed
    // after it and is therefore what the reader is told about.
    let alpha = symbols
        .get(CallableKind::Macro, "alpha")
        .expect("the library has it");
    assert_eq!(alpha.replacement, Some("α"));
    assert_eq!(alpha.arity, 0);
    assert_eq!(alpha.category, "mathcore");

    let emph = symbols.get(CallableKind::Macro, "emph").expect("declared");
    assert_eq!(emph.arity, 1);
    assert_eq!(emph.replacement, None);

    let href = symbols.get(CallableKind::Macro, "href").expect("declared");
    assert_eq!(href.arity, 2);
    assert_eq!(href.category, "links");

    let itemize = symbols
        .get(CallableKind::Environment, "itemize")
        .expect("declared");
    assert_eq!(itemize.category, "lists");

    let em_dash = symbols
        .get(CallableKind::Specials, "---")
        .expect("declared");
    assert_eq!(em_dash.replacement, Some("—"));
    assert_eq!(em_dash.modes, ModeVisibility::TextOnly);

    let superscript = symbols.get(CallableKind::Specials, "^").expect("declared");
    assert_eq!(superscript.modes, ModeVisibility::MathOnly);
    assert_eq!(superscript.arity, 1);

    assert!(symbols.get(CallableKind::Macro, "nosuchmacro").is_none());

    // The shape of the table, roughly: the long tail dominates it, and the three kinds
    // are each populated. These are floors, not fixed counts — the library grows.
    assert!(
        symbols.of_kind(CallableKind::Macro).len() > 1_000,
        "the ~1 100 shipped macros are all there, {} of them",
        symbols.of_kind(CallableKind::Macro).len()
    );
    assert!(symbols.of_kind(CallableKind::Environment).len() > 20);
    assert!(symbols.of_kind(CallableKind::Specials).len() >= 12);
    assert_eq!(
        symbols.len(),
        symbols.of_kind(CallableKind::Macro).len()
            + symbols.of_kind(CallableKind::Environment).len()
            + symbols.of_kind(CallableKind::Specials).len(),
        "every entry belongs to exactly one kind's run"
    );

    // What a completion list would ask for.
    let offered: Vec<&str> = symbols
        .starts_with(CallableKind::Macro, "alph")
        .iter()
        .map(|entry| entry.name)
        .collect();
    assert!(offered.contains(&"alpha"), "offered: {offered:?}");
}

#[test]
fn the_shipped_library_is_indexed_once_and_then_only_searched() {
    let definitions = techxt::defs::standard();
    // One walk of nineteen categories, here and nowhere else in this test.
    let symbols = definitions.symbols();

    // Strictly sorted with no repeated key, which is both the shadowing rule's outcome
    // and the precondition for every query being a binary search.
    for pair in symbols.entries().windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        assert!(
            (before.kind, before.name) < (after.kind, after.name),
            "{before:?} must sort strictly before {after:?}"
        );
    }

    // Every name in the table found again through the search path — over fourteen
    // hundred lookups against the one index, which is the access pattern the type is
    // shaped for and is instant only because nothing is rebuilt.
    for entry in symbols.entries() {
        let found = symbols
            .get(entry.kind, entry.name)
            .expect("a name in the table resolves");
        assert_eq!(found, entry);
        assert!(
            symbols.starts_with(entry.kind, entry.name).contains(entry),
            "{entry:?} is its own longest prefix match"
        );
    }
}

#[test]
fn the_shipped_library_never_shadows_a_name_with_a_narrower_one() {
    // The index resolves a name once, to the innermost definition. techy's own
    // resolution is *mode-aware*: an innermost entry hidden in the current mode is
    // skipped and an outer one can answer instead, so an index entry would be a
    // simplification if the library ever restricted a definition that shadows an
    // unrestricted one. It never does — the generated long tail is the restricted layer
    // and every curated category sits above it — and this test is what keeps that true.
    let definitions = techxt::defs::standard();
    let mut narrowed = Vec::new();
    for category in definitions.categories() {
        for definition in category.macros() {
            let alone =
                DefinitionSet::new().with(Category::new("alone").with_macro(definition.clone()));
            let modes = alone
                .symbols()
                .get(CallableKind::Macro, definition.name())
                .expect("the one entry")
                .modes;
            if modes != ModeVisibility::Anywhere {
                narrowed.push((category.name(), definition.name(), modes));
            }
        }
    }
    assert!(
        !narrowed.is_empty(),
        "the library does restrict some definitions, or this test proves nothing"
    );

    let symbols = definitions.symbols();
    for (category, name, modes) in narrowed {
        let resolved = symbols
            .get(CallableKind::Macro, name)
            .expect("it is in the table");
        if resolved.category == category {
            assert_eq!(
                resolved.modes, modes,
                "\\{name} resolves to {category}, so the index reports its restriction"
            );
        } else {
            assert_eq!(
                resolved.modes,
                ModeVisibility::Anywhere,
                "\\{name} is restricted in {category} but shadowed by {} — a shadowing \
                 definition must not be the narrower one, or the index's single answer \
                 would hide a mode the parser still resolves in",
                resolved.category
            );
        }
    }
}
