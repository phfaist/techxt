//! The template mini-language end to end (PLAN.md §10.5): what a template renders to,
//! and what a malformed one costs.
//!
//! The parser's own unit tests live next to it; these exercise it through the public
//! surface — a definition carrying a template, built into a converter and run over a
//! document.

use techxt::convert::BuildError;
use techxt::def::{
    Category, DefinitionSet, EnvDef, MacroDef, SpecialsDef, Template, TemplateError, TextRule,
};
use techxt::Converter;

/// A template rule written in the source syntax.
fn template(source: &str) -> TextRule {
    TextRule::Template(Template::new(source))
}

/// A converter knowing exactly `category`.
fn converter(category: Category) -> Result<Converter, BuildError> {
    Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .build()
}

/// Convert `latex` with a converter knowing exactly `category`.
fn text(category: Category, latex: &str) -> String {
    converter(category)
        .expect("the definitions build")
        .latex_to_text(latex)
        .expect("a tolerant parse of well-formed input succeeds")
        .text
}

// ------------------------------------------------------------------- rendering

#[test]
fn a_template_substitutes_named_arguments() {
    let category = Category::new("t").with_macro(
        MacroDef::new("href")
            .arg("BracedOnly", "url")
            .arg("m", "text")
            .rule(template("{text} <{url}>")),
    );
    assert_eq!(
        text(category, r"\href{https://ex.org/a_b}{link}"),
        "link <https://ex.org/a_b>\n"
    );
}

#[test]
fn a_template_substitutes_by_one_based_index() {
    let category = Category::new("t").with_macro(
        MacroDef::new("swap")
            .arg("m", "first")
            .arg("m", "second")
            .rule(template("{2}{1}")),
    );
    assert_eq!(text(category, r"\swap{a}{b}"), "ba\n");
}

#[test]
fn a_template_may_drop_an_argument() {
    // What `TextRule::Content` cannot do: render one argument and discard the other.
    let category = Category::new("t").with_macro(
        MacroDef::new("textcolor")
            .arg("m", "color")
            .arg("m", "text")
            .rule(template("{text}")),
    );
    assert_eq!(text(category, r"\textcolor{red}{hello}"), "hello\n");
}

#[test]
fn doubled_braces_are_literal_braces() {
    let category =
        Category::new("t").with_macro(MacroDef::new("set").arg("m", "x").rule(template("{{{x}}}")));
    assert_eq!(text(category, r"\set{a}"), "{a}\n");
}

#[test]
fn an_absent_optional_argument_renders_as_nothing() {
    let category = Category::new("t").with_macro(
        MacroDef::new("cite")
            .arg("o", "note")
            .arg("m", "key")
            .rule(template("[{key}{note}]")),
    );
    assert_eq!(text(category.clone(), r"\cite{Ein05}"), "[Ein05]\n");
    assert_eq!(text(category, r"\cite[p. 3]{Ein05}"), "[Ein05p. 3]\n");
}

#[test]
fn a_conditional_branches_on_whether_an_argument_was_written() {
    let category = Category::new("t").with_macro(
        MacroDef::new("cite")
            .arg("o", "note")
            .arg("m", "key")
            .rule(template("[{key}{?note:, {note}}]")),
    );
    assert_eq!(text(category.clone(), r"\cite{Ein05}"), "[Ein05]\n");
    assert_eq!(text(category, r"\cite[p. 3]{Ein05}"), "[Ein05, p. 3]\n");
}

#[test]
fn a_conditional_may_have_an_else_branch() {
    let category = Category::new("t").with_macro(
        MacroDef::new("sect")
            .star()
            .arg("m", "title")
            .rule(template("{?star:unnumbered|numbered}: {title}")),
    );
    assert_eq!(text(category.clone(), r"\sect{A}"), "numbered: A\n");
    assert_eq!(text(category, r"\sect*{A}"), "unnumbered: A\n");
}

#[test]
fn a_conditional_may_test_an_argument_by_index() {
    let category = Category::new("t").with_macro(
        MacroDef::new("maybe")
            .arg("o", "opt")
            .rule(template("{?1:yes|no}")),
    );
    assert_eq!(text(category.clone(), r"\maybe"), "no\n");
    assert_eq!(text(category, r"\maybe[x]"), "yes\n");
}

#[test]
fn an_environment_template_can_use_the_body() {
    let category = Category::new("t").with_env(EnvDef::new("quote").rule(template("“{body}”")));
    assert_eq!(text(category, r"\begin{quote}hi\end{quote}"), "“hi”\n");
}

#[test]
fn a_specials_template_sees_its_arguments() {
    let category = Category::new("t").with_specials(
        SpecialsDef::new("^")
            .arg("m", "exponent")
            .rule(template("^({exponent})")),
    );
    assert_eq!(text(category, "a^2"), "a^(2)\n");
}

#[test]
fn a_templates_replacement_text_is_treated_as_document_text() {
    // Words and inter-word spaces, so the layout engine can wrap inside it.
    let category =
        Category::new("t").with_macro(MacroDef::new("x").rule(template("one two three four")));
    let converter = Converter::builder()
        .definitions(DefinitionSet::new().with(category))
        .wrap_width(Some(9))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\x").expect("parses").text,
        "one two\nthree\nfour\n"
    );
}

// ----------------------------------------------------------------- build errors

/// The build error of a one-macro category whose template is `source`.
fn macro_error(source: &str, definition: MacroDef) -> BuildError {
    converter(Category::new("t").with_macro(definition.rule(template(source))))
        .expect_err("the template is refused")
}

/// The template error inside a `BuildError::Template`, with its context checked.
fn template_error(error: &BuildError, definition: &str, source: &str) -> TemplateError {
    match error {
        BuildError::Template {
            definition: name,
            template,
            error,
        } => {
            assert_eq!(&**name, definition);
            assert_eq!(&**template, source);
            error.clone()
        }
        other => panic!("expected a template error, got {other:?}"),
    }
}

#[test]
fn an_unknown_argument_name_is_a_build_error() {
    let error = macro_error("{nope}", MacroDef::new("m").arg("m", "text"));
    assert_eq!(
        template_error(&error, "m", "{nope}"),
        TemplateError::UnknownArgumentName {
            name: "nope".into()
        }
    );
    // The message names both the definition and the template.
    let message = alloc_display(&error);
    assert!(message.contains('m'), "{message}");
    assert!(message.contains("{nope}"), "{message}");
}

#[test]
fn argument_index_zero_is_a_build_error() {
    let error = macro_error("{0}", MacroDef::new("m").arg("m", "text"));
    assert_eq!(
        template_error(&error, "m", "{0}"),
        TemplateError::ArgumentIndexOutOfRange { index: 0, count: 1 }
    );
}

#[test]
fn an_index_past_the_last_argument_is_a_build_error() {
    let error = macro_error("{3}", MacroDef::new("m").arg("m", "a").arg("m", "b"));
    assert_eq!(
        template_error(&error, "m", "{3}"),
        TemplateError::ArgumentIndexOutOfRange { index: 3, count: 2 }
    );
}

#[test]
fn body_in_a_macro_template_is_a_build_error() {
    let error = macro_error("{body}", MacroDef::new("m"));
    assert_eq!(
        template_error(&error, "m", "{body}"),
        TemplateError::BodyOutsideEnvironment
    );
}

#[test]
fn body_in_a_specials_template_is_a_build_error() {
    let error =
        converter(Category::new("t").with_specials(SpecialsDef::new("&").rule(template("{body}"))))
            .expect_err("refused");
    assert_eq!(
        template_error(&error, "&", "{body}"),
        TemplateError::BodyOutsideEnvironment
    );
}

#[test]
fn a_nested_conditional_is_a_build_error() {
    let error = macro_error(
        "{?a:{?b:x}}",
        MacroDef::new("m").arg("o", "a").arg("o", "b"),
    );
    assert_eq!(
        template_error(&error, "m", "{?a:{?b:x}}"),
        TemplateError::NestedConditional
    );
}

#[test]
fn an_unterminated_construct_is_a_build_error() {
    let error = macro_error("{text", MacroDef::new("m").arg("m", "text"));
    assert_eq!(
        template_error(&error, "m", "{text"),
        TemplateError::UnterminatedConstruct
    );
    let error = macro_error("{?text:x", MacroDef::new("m").arg("m", "text"));
    assert_eq!(
        template_error(&error, "m", "{?text:x"),
        TemplateError::UnterminatedConstruct
    );
}

#[test]
fn an_environment_template_is_checked_against_its_own_arguments() {
    let error = converter(
        Category::new("t").with_env(
            EnvDef::new("thm")
                .arg("o", "title")
                .rule(template("{body}{name}")),
        ),
    )
    .expect_err("refused");
    assert_eq!(
        template_error(&error, "thm", "{body}{name}"),
        TemplateError::UnknownArgumentName {
            name: "name".into()
        }
    );
}

// --------------------------------------------------------------- override rules

#[test]
fn an_override_template_is_checked_for_shape_but_not_for_names() {
    // An override is written against whichever definition it lands on, so its names
    // cannot be checked against one entry — but its shape can.
    let converter = Converter::builder()
        .override_macro("emph", template("<{text}>"))
        .build()
        .expect("an override naming an argument builds");
    assert_eq!(
        converter.latex_to_text(r"\emph{hi}").expect("parses").text,
        "<hi>\n"
    );

    let error = Converter::builder()
        .override_macro("emph", template("{?a:{?b:x}}"))
        .build()
        .expect_err("a malformed override is refused");
    assert_eq!(
        template_error(&error, r"\emph", "{?a:{?b:x}}"),
        TemplateError::NestedConditional
    );
}

#[test]
fn body_is_refused_in_a_macro_override_and_allowed_in_an_environment_one() {
    let error = Converter::builder()
        .override_macro("emph", template("{body}"))
        .build()
        .expect_err("refused");
    assert_eq!(
        template_error(&error, r"\emph", "{body}"),
        TemplateError::BodyOutsideEnvironment
    );

    let converter = Converter::builder()
        .override_environment("center", template("[{body}]"))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"\begin{center}x\end{center}")
            .expect("parses")
            .text,
        "[x]\n"
    );
}

#[test]
fn an_override_naming_an_argument_that_is_not_there_is_a_render_diagnostic() {
    // The check that could not happen at build time happens here, and costs the
    // construct its output rather than the document its conversion.
    let converter = Converter::builder()
        .override_macro("ldots", template("{nope}"))
        .build()
        .expect("builds");
    let conversion = converter.latex_to_text(r"a\ldots b").expect("parses");
    // The macro renders as nothing (and the space after a control word belongs to the
    // invocation, not to the text), so only the surrounding characters survive.
    assert_eq!(conversion.text, "ab\n");
    assert_eq!(
        conversion
            .diagnostics
            .with_identifier("techxt.handler-failed")
            .count(),
        1
    );
}

// -------------------------------------------------------------------- the type

#[test]
fn a_template_keeps_its_source_and_reports_its_syntax_error() {
    let template = Template::new("{a} {b}");
    assert_eq!(template.source(), "{a} {b}");
    assert!(template.error().is_none());

    let broken = Template::new("{a");
    assert_eq!(broken.source(), "{a");
    assert_eq!(broken.error(), Some(&TemplateError::UnterminatedConstruct));
}

/// `Display` of a build error, as a caller would print it.
fn alloc_display(error: &BuildError) -> String {
    format!("{error}")
}
