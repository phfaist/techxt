//! The public conversion API (PLAN.md §11), the dispatch chain (PLAN.md §10.3), the
//! rule kinds (PLAN.md §10.4) and the unknown-construct policies (PLAN.md §10.6).

use std::borrow::Cow;
use std::sync::Arc;

use techxt::convert::{
    Conversion, MathMode, UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy,
};
use techxt::def::{TextHandler, TextRule};
use techxt::diag::{HandlerFailed, UnknownEnvironment, UnknownMacro, UnknownSpecials};
use techxt::flow::Flow;
use techxt::render::{RenderCx, RenderError};
use techxt::{Converter, ConverterBuilder, Options};
use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

// ------------------------------------------------------------- PLAN.md §11.1

#[test]
fn a_converter_is_cloneable_and_shareable() {
    fn assert_send_sync<T: Send + Sync + Clone>() {}
    assert_send_sync::<Converter>();

    // Two threads, one converter: the plan's central promise about reuse.
    let converter = Converter::standard();
    let second = converter.clone();
    let handle = std::thread::spawn(move || second.latex_to_text("a b").expect("parses").text);
    assert_eq!(
        converter.latex_to_text("a b").expect("parses").text,
        "a b\n"
    );
    assert_eq!(handle.join().expect("the thread finished"), "a b\n");
}

#[test]
fn the_three_layers_agree() {
    let converter = Converter::standard();
    let tree = converter
        .language()
        .parse(r"a \emph{b} c")
        .expect("parses")
        .tree;

    let from_string = converter.latex_to_text(r"a \emph{b} c").expect("parses");
    let from_tree: Conversion = converter.tree_to_text(&tree);
    let (flow, diagnostics) = converter.tree_to_flow(&tree);

    assert_eq!(from_string.text, "a b c\n");
    assert_eq!(from_tree.text, from_string.text);
    assert!(diagnostics.is_empty());
    assert_eq!(
        techxt::layout::render(&flow, &techxt::layout::LayoutOptions::default()),
        from_tree.text
    );
}

#[test]
fn options_are_readable_from_the_converter() {
    let converter = Converter::builder()
        .wrap_width(Some(40))
        .math_mode(MathMode::Plain)
        .build()
        .expect("builds");
    assert_eq!(converter.options().wrap_width, Some(40));
    assert_eq!(converter.options().math_mode, MathMode::Plain);
    assert_eq!(converter.renderer().options().wrap_width, Some(40));
}

#[test]
fn the_builder_default_matches_the_options_default() {
    let built = ConverterBuilder::default().build().expect("builds");
    let defaults = Options::default();
    assert_eq!(built.options().wrap_width, defaults.wrap_width);
    assert_eq!(built.options().unknown_macro, defaults.unknown_macro);
    assert_eq!(built.options().keep_comments, defaults.keep_comments);
}

#[test]
fn wrapping_happens_across_macro_boundaries() {
    // PLAN.md §15 example 25's structure: the wrap decision sees "boldtext" as one
    // word because the pieces are adjacent, and breaks at the glue before it.
    let converter = Converter::builder()
        .wrap_width(Some(12))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"aaa bbb \textbf{ccc ddd} eee")
            .expect("parses")
            .text,
        "aaa bbb ccc\nddd eee\n"
    );
}

// ------------------------------------------------------------- PLAN.md §10.3

#[test]
fn the_override_map_beats_the_embedded_rule() {
    let converter = Converter::builder()
        .override_macro("emph", TextRule::Literal(Cow::Borrowed("OVERRIDDEN")))
        .build()
        .expect("builds");
    // Without the override this renders the argument's content, "x".
    assert_eq!(text(r"\emph{x}"), "x\n");
    assert_eq!(
        converter.latex_to_text(r"\emph{x}").expect("parses").text,
        "OVERRIDDEN\n"
    );
}

#[test]
fn the_override_map_beats_the_name_fallback_table() {
    // `\TeX` is registered with a plain techy spec, so its rule can only come from the
    // fallback table — and the override still wins.
    assert_eq!(text(r"\TeX"), "TeX\n");
    let converter = Converter::builder()
        .override_macro("TeX", TextRule::Literal(Cow::Borrowed("T-E-X")))
        .build()
        .expect("builds");
    assert_eq!(
        converter.latex_to_text(r"\TeX").expect("parses").text,
        "T-E-X\n"
    );
}

#[test]
fn the_name_fallback_table_beats_the_unknown_policy() {
    // With no fallback entry, `\TeX` would take the `Skip` policy and vanish.
    assert_eq!(text(r"a\TeX b"), "aTeXb\n");
    assert_eq!(
        Converter::standard()
            .latex_to_text(r"a\TeX b")
            .expect("parses")
            .diagnostics
            .conditions::<UnknownMacro>()
            .count(),
        0
    );
}

#[test]
fn the_unknown_policy_is_the_last_resort() {
    // `\phantom` parses (it has an argument spec) but has no rule anywhere.
    let conversion = Converter::standard()
        .latex_to_text(r"a\phantom{x}b")
        .expect("parses");
    assert_eq!(conversion.text, "ab\n");
    let unknown: Vec<&UnknownMacro> = conversion.diagnostics.conditions().collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].name, "phantom");
}

#[test]
fn overrides_are_keyed_by_kind_as_well_as_name() {
    // An environment override must not affect a macro of the same name, and vice
    // versa: techy keeps the two in separate stores, and so does techxt.
    let converter = Converter::builder()
        .override_environment("center", TextRule::Literal(Cow::Borrowed("ENV")))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"\begin{center}body\end{center}")
            .expect("parses")
            .text,
        "ENV\n"
    );
    // `\emph` is untouched by an environment override.
    assert_eq!(
        converter.latex_to_text(r"\emph{x}").expect("parses").text,
        "x\n"
    );
}

// ------------------------------------------------------------- PLAN.md §10.4

#[test]
fn rule_kind_literal() {
    assert_eq!(text(r"\ldots"), "…\n");
}

#[test]
fn rule_kind_template() {
    // `\textbf` is a named reference, `\textit` a positional one, `\href` a template
    // with literal text in it, and `center` a `{body}` template.
    assert_eq!(text(r"\textbf{b}"), "b\n");
    assert_eq!(text(r"\textit{i}"), "i\n");
    assert_eq!(text(r"\href{u}{t}"), "t <u>\n");
    assert_eq!(text(r"\begin{center}c\end{center}"), "c\n");
}

#[test]
fn rule_kind_skip_prunes_the_subtree() {
    // Not merely "renders as nothing": the argument is never folded at all.
    assert_eq!(text(r"a\label{\emph{deep}}b"), "ab\n");
}

#[test]
fn rule_kind_content() {
    assert_eq!(text(r"\emph{x}"), "x\n");
    assert_eq!(text(r"\verb+raw  text+"), "raw  text\n");
    assert_eq!(text(r"\begin{verbatim}body\end{verbatim}"), "body\n");
}

#[test]
fn rule_kind_handler() {
    assert_eq!(text(r"a\par b"), "a\n\nb\n");
    assert_eq!(text(r"a~b"), "a\u{a0}b\n");
}

#[test]
fn a_failing_handler_costs_its_construct_and_nothing_else() {
    #[derive(Debug)]
    struct AlwaysFails;

    impl TextHandler for AlwaysFails {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            _cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            Err(RenderError::Handler {
                construct: "\\emph".into(),
                detail: "deliberate".into(),
            })
        }
    }

    let converter = Converter::builder()
        .override_macro("emph", TextRule::Handler(Arc::new(AlwaysFails)))
        .build()
        .expect("builds");
    let conversion = converter
        .latex_to_text(r"before \emph{x} after")
        .expect("parses");
    assert_eq!(conversion.text, "before after\n");

    let failures: Vec<&HandlerFailed> = conversion.diagnostics.conditions().collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].construct, "\\emph");
    assert_eq!(failures[0].detail, "deliberate");
    assert!(conversion.diagnostics.has_errors());
}

#[test]
fn a_handler_reads_its_arguments_through_the_context() {
    #[derive(Debug)]
    struct Stars;

    impl TextHandler for Stars {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            assert!(cx.arg_provided("text"));
            assert!(!cx.arg_provided("no-such-argument"));
            // `arg_text` is the flattened form of the same argument.
            let flat = cx.arg_text("text")?.expect("the argument is provided");
            assert!(!flat.is_empty());
            assert!(!flat.contains('\n'));
            let mut flow = Flow::text("*");
            flow.extend(cx.arg("text")?.unwrap_or_default());
            flow.extend(Flow::text("*"));
            Ok(flow)
        }
    }

    let converter = Converter::builder()
        .override_macro("emph", TextRule::Handler(Arc::new(Stars)))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"\emph{inner \emph{text}}")
            .expect("parses")
            .text,
        // The nested `\emph` is rendered by the same rule, which is what folding
        // through the renderer buys.
        "*inner *text**\n"
    );
}

#[test]
fn a_handler_can_register_footnotes_and_document_metadata() {
    #[derive(Debug)]
    struct Note;

    impl TextHandler for Note {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            let body = cx.arg("text")?.unwrap_or_default();
            let number = cx.push_footnote(body);
            Ok(Flow::text(&format!("[{number}]")))
        }
    }

    #[derive(Debug)]
    struct Title;

    impl TextHandler for Title {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            let title = cx.arg("text")?.unwrap_or_default();
            assert!(cx.doc_title().is_none());
            cx.set_doc_title(title);
            assert!(cx.doc_title().is_some());
            Ok(Flow::new())
        }
    }

    let converter = Converter::builder()
        .override_macro("emph", TextRule::Handler(Arc::new(Note)))
        .override_macro("textbf", TextRule::Handler(Arc::new(Title)))
        .build()
        .expect("builds");
    // PLAN.md §15 example 18's shape: markers in the text, notes gathered at the end.
    assert_eq!(
        converter
            .latex_to_text(r"\textbf{T}Fact\emph{Proof sketch.} holds.")
            .expect("parses")
            .text,
        "Fact[1] holds.\n\n---\n[1] Proof sketch.\n"
    );
}

// ------------------------------------------------------------- PLAN.md §10.6

#[test]
fn every_unknown_macro_policy() {
    let cases = [
        (UnknownMacroPolicy::Skip, "ab\n"),
        (UnknownMacroPolicy::RenderArgs, "axb\n"),
        (UnknownMacroPolicy::KeepSource, "a\\phantom{x}b\n"),
        (UnknownMacroPolicy::Placeholder, "a<phantom>b\n"),
    ];
    for (policy, expected) in cases {
        let converter = Converter::builder()
            .unknown_macro(policy)
            .build()
            .expect("builds");
        let conversion = converter.latex_to_text(r"a\phantom{x}b").expect("parses");
        assert_eq!(conversion.text, expected, "policy {policy:?}");
        // The diagnostic is raised whatever the policy says.
        assert_eq!(
            conversion.diagnostics.conditions::<UnknownMacro>().count(),
            1,
            "policy {policy:?}"
        );
    }
}

#[test]
fn every_unknown_environment_policy() {
    let cases = [
        (UnknownEnvPolicy::RenderBody, "ainb\n"),
        (UnknownEnvPolicy::Skip, "ab\n"),
        (
            UnknownEnvPolicy::KeepSource,
            "a\n\n\\begin{unknownenv}in\\end{unknownenv}\n\nb\n",
        ),
    ];
    for (policy, expected) in cases {
        let converter = Converter::builder()
            .unknown_env(policy)
            .build()
            .expect("builds");
        let conversion = converter
            .latex_to_text(r"a\begin{unknownenv}in\end{unknownenv}b")
            .expect("parses");
        assert_eq!(conversion.text, expected, "policy {policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .conditions::<UnknownEnvironment>()
                .count(),
            1,
            "policy {policy:?}"
        );
    }
}

#[test]
fn every_unknown_specials_policy() {
    let cases = [
        (UnknownSpecialsPolicy::EmitChars, "a & b\n"),
        (UnknownSpecialsPolicy::Skip, "a b\n"),
    ];
    for (policy, expected) in cases {
        let converter = Converter::builder()
            .unknown_specials(policy)
            .build()
            .expect("builds");
        let conversion = converter.latex_to_text("a & b").expect("parses");
        assert_eq!(conversion.text, expected, "policy {policy:?}");
        assert_eq!(
            conversion
                .diagnostics
                .conditions::<UnknownSpecials>()
                .count(),
            1,
            "policy {policy:?}"
        );
    }
}

#[test]
fn keep_source_is_protected_from_wrapping() {
    let converter = Converter::builder()
        .unknown_macro(UnknownMacroPolicy::KeepSource)
        .wrap_width(Some(8))
        .build()
        .expect("builds");
    // The re-emitted source is one unbreakable run, so it overflows rather than
    // splitting in the middle of a macro name.
    assert_eq!(
        converter
            .latex_to_text(r"aa \phantom{bbbbbbbbbb} cc")
            .expect("parses")
            .text,
        "aa\n\\phantom{bbbbbbbbbb}\ncc\n"
    );
}

#[test]
fn parse_diagnostics_come_before_render_diagnostics() {
    // `\foo` is a parse-level failure; the unknown environment is a render-level one.
    let conversion = Converter::standard()
        .latex_to_text(r"\foo \begin{myenv}x\end{myenv}")
        .expect("tolerant recovery keeps going");
    let identifiers: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.identifier())
        .collect();
    let parse_position = identifiers
        .iter()
        .position(|id| id.starts_with("core.") || id.starts_with("latexlike."))
        .expect("a parse diagnostic");
    let render_position = identifiers
        .iter()
        .position(|id| id.starts_with("techxt."))
        .expect("a render diagnostic");
    assert!(parse_position < render_position, "{identifiers:?}");
}

#[test]
fn diagnostics_carry_a_usable_position() {
    let conversion = Converter::standard()
        .latex_to_text("padding \\begin{myenv}x\\end{myenv}")
        .expect("parses");
    let diagnostic = conversion
        .diagnostics
        .with_identifier("techxt.unknown-environment")
        .next()
        .expect("the warning");
    assert_eq!(diagnostic.span().start(), "padding ".len());
    assert!(diagnostic.message().contains("myenv"));
    assert!(!diagnostic.render().is_empty());
}
