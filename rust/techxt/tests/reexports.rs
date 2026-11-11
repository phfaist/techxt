//! techy and techy-xp as techxt re-exports them.
//!
//! An embedder is meant to need no `techy` or `techy-xp` dependency of its own: the
//! types a conversion names come from `techxt::convert`, and the rest of the two crates
//! from `techxt::techy` and `techxt::techy_xp`. This file is written the way such an
//! embedder writes. Every upstream item below is named through techxt, and none through
//! the `techy` this crate could also reach, so a path the documentation promises and the
//! crate does not export fails to compile here.
//!
//! Everything but the last test names nothing of techy-xp's, and so means the same with
//! techxt's `xp` feature on or off.

use techxt::convert::{Latexlike, Recovery};
use techxt::techy::core::specs::Package;
use techxt::techy::core::{Language, ParsingState};
use techxt::techy::latexlike::{argument_specs_named, CallableType, LatexlikeDriver, MacroSpec};
use techxt::{defs, Converter, ConverterBuilder};

/// A converter for techy's plain language over the shipped definitions.
fn standard() -> Converter<Latexlike> {
    ConverterBuilder::with_definitions(defs::standard())
        .build()
        .expect("a well-formed definition set builds")
}

#[test]
fn the_readme_feature_off_example() {
    // The README's third Rust snippet, character for character below its `use` lines.
    let converter: Converter<Latexlike> = ConverterBuilder::with_definitions(defs::standard())
        .build()
        .expect("a well-formed definition set builds");

    let conversion = converter
        .latex_to_text(r"\textbf{hi} $x^2$")
        .expect("parses");
    assert_eq!(conversion.text, "\u{1d421}\u{1d422} \u{1d465}\u{b2}\n");
}

#[test]
fn the_short_and_the_long_spelling_name_one_language() {
    // One type under two paths: a converter for one is a converter for the other.
    fn takes_the_long_spelling(_: &Converter<techxt::techy::latexlike::Latexlike>) {}

    let converter: Converter<techxt::convert::Latexlike> = standard();
    takes_the_long_spelling(&converter);
}

#[test]
fn a_tree_parsed_through_the_reexported_techy_renders() {
    // A parser of the embedder's own, built from pieces `techxt::convert` does not
    // list: a package, a macro spec, the parsing state and techy's driver.
    let mut package = Package::<Latexlike>::new("foreign");
    package.insert(
        CallableType::Macro,
        "textbf",
        MacroSpec::new(
            argument_specs_named::<Latexlike, _, _, _>([("m", "text")]).expect("argument codes"),
        ),
    );
    let foreign: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages([package]).expect("seed state"),
    );
    let tree = foreign.parse(r"a \textbf{b} c").expect("parses").tree;

    // The tree is of the techy techxt is built on, so techxt renders it.
    let conversion = standard().tree_to_text(&tree);
    assert_eq!(conversion.text, "a \u{1d41b} c\n");
    assert!(conversion.diagnostics.is_empty());
}

#[cfg(feature = "xp")]
#[test]
fn techy_xp_is_reexported_with_the_xp_feature() {
    // The default parse language, under its full upstream path.
    fn takes_the_long_spelling(_: &Converter<techxt::techy_xp::lang::LatexlikeXp>) {}

    takes_the_long_spelling(&Converter::standard());
}
