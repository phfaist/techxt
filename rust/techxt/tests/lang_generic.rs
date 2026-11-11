//! The generic door, walked with techy's plain `Latexlike`.
//!
//! Everything here is written against the language-generic API alone —
//! [`ConverterBuilder::with_definitions`], `Converter<L>`, `DefinitionSet<L>` — and
//! instantiates it at techy's own `Latexlike`, the one parse language that needs no
//! techy-xp. Nothing names `Converter::standard()`, `Converter::builder()`,
//! `MacroDefinitions` or `techy_xp`, so the file means the same with techxt's `xp`
//! feature on or off.
//!
//! Each test pins a property a second instantiation could get wrong: that the shipped
//! definitions convert end to end, that a defining command is *declared* and never
//! honoured, that mode restrictions and the special bodies (verbatim, list, table)
//! survive the change of language, that `\input` reaches the driver's resolver, that a
//! rule embedded in a spec travels with the tree to a converter that knows nothing of
//! it, and that the converter is as shareable as the default one.

use std::borrow::Cow;

use techy::latexlike::Latexlike;

use techxt::convert::{Conversion, MapResolver, Severity};
use techxt::def::{Category, DefinitionSet, MacroDef, Template, TextRule};
use techxt::{defs, Converter, ConverterBuilder};

/// A converter for plain techy's language over the shipped definitions.
fn standard() -> Converter<Latexlike> {
    ConverterBuilder::<Latexlike>::with_definitions(defs::standard())
        .build()
        .expect("builds")
}

/// Convert `latex` with [`standard`].
fn convert(latex: &str) -> Conversion {
    standard().latex_to_text(latex).expect("parses")
}

/// The identifiers of everything a conversion reported, in order.
fn identifiers(conversion: &Conversion) -> Vec<String> {
    conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier().to_string())
        .collect()
}

// ------------------------------------------------------------------- end to end

#[test]
fn the_shipped_definitions_convert_end_to_end() {
    // One document crossing several categories: a heading, a font style, an accent, a
    // ligature, an inline formula with a script, a footnote. Every piece is a rule the
    // definitions registered *at* `Latexlike`; had any of them failed to register, the
    // construct would reach the `Skip` unknown policy and vanish from the text, and
    // `techxt.unknown-macro` would be reported.
    let conversion = convert(r"\section{Intro} \textbf{hi} caf\'e -- $x^2$\footnote{note}");
    assert_eq!(
        conversion.text,
        "1 Intro\n-------\n\n𝐡𝐢 café – 𝑥²[1]\n\n---\n[1] note\n"
    );
    assert!(
        conversion.diagnostics.is_empty(),
        "{:?}",
        identifiers(&conversion)
    );
}

// ------------------------------------------------------------ declared definers

#[test]
fn a_defining_command_is_declared_and_not_honoured() {
    // `\newcommand` is registered with its declared argument shape, so it consumes
    // `{\greet}` and `{Hi}` — the body never reaches the text — and defines nothing, so
    // the later `\greet` is an unknown command. An expanding parser would have printed
    // `Hi` and reported nothing.
    //
    // The name is braced on purpose: the declared shape reads the name as a braced
    // group, so the unbraced `\newcommand\greet{Hi}` is not consumed whole (it is
    // only an honoured definer that reads either form). `lang_parity.rs` pins that
    // the unbraced form behaves exactly as techy-xp's declared definers do.
    let conversion = convert(r"\newcommand{\greet}{Hi}\greet");
    assert_eq!(conversion.text, "");
    assert_eq!(identifiers(&conversion), ["techxt.unknown-macro"]);
    let unknown = conversion
        .diagnostics
        .iter()
        .next()
        .expect("one diagnostic");
    assert_eq!(unknown.severity(), Severity::Warning);
    assert!(unknown.message().contains("greet"), "{}", unknown.message());

    // With an argument count and a parameter: the definition consumes `[1]` and the
    // body too. The unknown `\greet` takes no argument, so `{you}` is an ordinary group
    // and renders as its content — which is what proves `\greet` was never defined:
    // a defined `\greet` would have consumed it and printed `Hi you`.
    let conversion = convert(r"\newcommand{\greet}[1]{Hi #1}\greet{you}");
    assert_eq!(conversion.text, "you\n");
    assert_eq!(identifiers(&conversion), ["techxt.unknown-macro"]);
}

// ----------------------------------------------------------- mode restrictions

#[test]
fn a_text_only_entry_is_hidden_in_math() {
    // The `--` ligature is registered for text mode only. In a paragraph it is an
    // en dash; in a formula it is not resolved, so the two hyphens stay.
    assert_eq!(convert("a--b").text, "a–b\n");
    assert_eq!(convert("$a--b$").text, "𝑎 - -𝑏\n");
}

#[test]
fn a_math_only_entry_is_hidden_in_text() {
    // `_` is a subscript in math only. In running text it is a plain character.
    assert_eq!(convert("a_b").text, "a_b\n");
    assert_eq!(convert("$a_i$").text, "𝑎ᵢ\n");
}

// ------------------------------------------------------------- special bodies

#[test]
fn a_verbatim_body_is_copied_through() {
    // Parsed as verbatim, the body is never tokenized: `\textbf` and `$` stay as
    // written. Had the body been parsed, the bold and the formula would be converted.
    let conversion = convert("\\begin{verbatim}\n\\textbf{x} $y$\n\\end{verbatim}");
    assert_eq!(conversion.text, "\\textbf{x} $y$\n");
    assert_eq!(conversion.regions.len(), 1);
}

#[test]
fn a_list_is_itemized() {
    let conversion = convert(
        r"\begin{enumerate}\item one \item two\end{enumerate}\begin{itemize}\item a\end{itemize}",
    );
    assert_eq!(conversion.text, "  1. one\n  2. two\n  • a\n");
    assert!(conversion.diagnostics.is_empty());
}

#[test]
fn a_table_is_laid_out_in_columns() {
    let conversion = convert(r"\begin{tabular}{ll} a & bb \\ ccc & d \end{tabular}");
    assert_eq!(conversion.text, "a    bb\nccc  d\n");
    assert!(conversion.diagnostics.is_empty());
}

// ---------------------------------------------------------------------- `\input`

#[test]
fn input_is_resolved_through_the_source_resolver() {
    let mut resolver = MapResolver::new();
    resolver.insert("part.tex", r"\textbf{inside}");
    let converter: Converter<Latexlike> = ConverterBuilder::with_definitions(defs::standard())
        .source_resolver(resolver)
        .build()
        .expect("builds");

    // The included source is parsed into the same tree and converted with it: the
    // bold can only have come from the resolver's content.
    let conversion = converter
        .latex_to_text(r"before \input{part.tex} after")
        .expect("parses");
    assert_eq!(conversion.text, "before 𝐢𝐧𝐬𝐢𝐝𝐞 after\n");
    assert!(conversion.diagnostics.is_empty());

    // Without a resolver the same document converts, but the content is missing and
    // the missing resolver is reported.
    let conversion = convert(r"before \input{part.tex} after");
    assert_eq!(conversion.text, "before after\n");
    assert_eq!(identifiers(&conversion), ["techxt.input-not-resolved"]);
}

// ------------------------------------------------ dispatch step 2, across converters

/// A converter for plain techy's language whose definitions are exactly `category`.
fn only(category: Category<Latexlike>) -> Converter<Latexlike> {
    let definitions: DefinitionSet<Latexlike> = DefinitionSet::new().with(category);
    ConverterBuilder::with_definitions(definitions)
        .build()
        .expect("builds")
}

#[test]
fn a_rule_embedded_in_the_tree_is_found_by_another_converter() {
    // Parse with a converter that defines `\mark{text}` through a template no other
    // definition writes ...
    let parsing = only(
        Category::new("marks").with_macro(
            MacroDef::new("mark")
                .arg("m", "text")
                .rule(TextRule::Template(Template::new("<<{text}>>"))),
        ),
    );
    let tree = parsing
        .language()
        .parse(r"a\mark{x}b")
        .expect("parses")
        .tree;
    assert_eq!(parsing.tree_to_text(&tree).text, "a<<x>>b\n");

    // ... and render the tree with a converter that has no definitions at all. Its
    // override map and its name table are empty, and its unknown policy is the default
    // `Skip`, which would print `ab`: `<<x>>` can only have come from the spec the
    // parsing converter embedded in the tree (dispatch step 2).
    let definitions: DefinitionSet<Latexlike> = DefinitionSet::new();
    let empty: Converter<Latexlike> = ConverterBuilder::with_definitions(definitions)
        .build()
        .expect("builds");
    let conversion = empty.tree_to_text(&tree);
    assert_eq!(conversion.text, "a<<x>>b\n");
    assert!(conversion.diagnostics.is_empty());

    // Step 2 also beats the name table (step 3): a converter whose own `\mark` renders
    // differently still renders the tree's rule.
    let other = only(
        Category::new("other-marks").with_macro(
            MacroDef::new("mark")
                .arg("m", "text")
                .rule(TextRule::Literal(Cow::Borrowed("FROM-THE-TABLE"))),
        ),
    );
    assert_eq!(other.tree_to_text(&tree).text, "a<<x>>b\n");
    // The same converter parsing for itself uses its own rule, so the difference above
    // is the tree's doing.
    assert_eq!(
        other.latex_to_text(r"a\mark{x}b").expect("parses").text,
        "aFROM-THE-TABLEb\n"
    );
}

// ------------------------------------------------------------ Clone, Send, Sync

#[test]
fn a_converter_for_latexlike_is_clone_send_and_sync() {
    fn assert_clone_send_sync<T: Clone + Send + Sync + 'static>() {}
    assert_clone_send_sync::<Converter<Latexlike>>();

    // And it behaves as shared: a clone converts on another thread, the same way.
    let converter = standard();
    let clone = converter.clone();
    let latex = r"\emph{shared}";
    let from_thread = std::thread::spawn(move || clone.latex_to_text(latex).expect("parses"))
        .join()
        .expect("thread completes");
    assert_eq!(from_thread.text, "𝑠ℎ𝑎𝑟𝑒𝑑\n");
    assert_eq!(
        from_thread.text,
        converter.latex_to_text(latex).expect("parses").text
    );
}
