//! The public conversion API (PLAN.md §11), the dispatch chain (PLAN.md §10.3), the
//! rule kinds (PLAN.md §10.4) and the unknown-construct policies (PLAN.md §10.6).

use std::borrow::Cow;
use std::sync::Arc;

use techxt::convert::{
    Conversion, MathMode, UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy,
};
use techxt::def::{Category, DefinitionSet, MacroDef, SpecialsDef, TextHandler, TextRule};
use techxt::diag::{HandlerFailed, UnknownEnvironment, UnknownMacro, UnknownSpecials};
use techxt::flow::Flow;
use techxt::render::{RenderCx, RenderError};
use techxt::{Converter, ConverterBuilder, Options};
use techy::core::node::NodeRef;
use techy_xp::lang::LatexlikeXp;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// The shipped library plus two constructs that exist only to be *rule-less*.
///
/// PLAN.md §10.6's policies act on a construct that parses — its arguments are
/// declared — but carries no text rule anywhere. Every entry `techxt::defs` ships has a
/// rule, as it should, so the dispatch chain's last step needs a vehicle of its own.
fn with_ruleless() -> DefinitionSet {
    techxt::defs::standard().with(
        Category::new("test-ruleless")
            .with_macro(MacroDef::new("ruleless").arg("m", "text"))
            .with_specials(SpecialsDef::new("@@")),
    )
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

    // `\emph` italicizes, which is what the shipped `defs::fontstyles` does with it.
    assert_eq!(from_string.text, "a \u{1d44f} c\n");
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
    // PLAN.md §15 example 25, exactly: the wrap decision sees the styled letters as
    // ordinary words and breaks at the glue inside the macro's argument.
    let converter = Converter::builder()
        .wrap_width(Some(12))
        .build()
        .expect("builds");
    assert_eq!(
        converter
            .latex_to_text(r"aaa bbb \textbf{ccc ddd} eee")
            .expect("parses")
            .text,
        "aaa bbb \u{1d41c}\u{1d41c}\u{1d41c}\n\u{1d41d}\u{1d41d}\u{1d41d} eee\n"
    );
}

// ------------------------------------------------------------- PLAN.md §10.3

#[test]
fn the_override_map_beats_the_embedded_rule() {
    let converter = Converter::builder()
        .override_macro("emph", TextRule::Literal(Cow::Borrowed("OVERRIDDEN")))
        .build()
        .expect("builds");
    // Without the override this renders the argument, emphasized.
    assert_eq!(text(r"\emph{x}"), "\u{1d465}\n");
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
    // `\ruleless` parses (it has an argument spec) but has no rule anywhere.
    let converter = Converter::builder()
        .definitions(with_ruleless())
        .build()
        .expect("builds");
    let conversion = converter.latex_to_text(r"a\ruleless{x}b").expect("parses");
    assert_eq!(conversion.text, "ab\n");
    let unknown: Vec<&UnknownMacro> = conversion.diagnostics.conditions().collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].name, "ruleless");
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
        "\u{1d465}\n"
    );
}

// ------------------------------------------------------------- PLAN.md §10.4

#[test]
fn rule_kind_literal() {
    assert_eq!(text(r"\ldots"), "…\n");
}

#[test]
fn rule_kind_template() {
    // `\href` is a template with two named references and literal text between them,
    // `\url` one reference inside literal text, and `\texorpdfstring` a template that
    // deliberately drops an argument — which is why it is a template and not `Content`.
    assert_eq!(text(r"\href{u}{t}"), "t <u>\n");
    assert_eq!(text(r"\url{u}"), "<u>\n");
    assert_eq!(text(r"\texorpdfstring{tex}{pdf}"), "tex\n");
}

#[test]
fn rule_kind_skip_prunes_the_subtree() {
    // Not merely "renders as nothing": the argument is never folded at all.
    assert_eq!(text(r"a\label{\emph{deep}}b"), "ab\n");
}

#[test]
fn rule_kind_content() {
    // Small capitals and the over/under decorations have no plain-text rendering, so
    // each renders exactly its argument's content.
    assert_eq!(text(r"\textsc{x}"), "x\n");
    assert_eq!(text(r"\overline{a b}"), "a b\n");
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
            _node: NodeRef<'_, LatexlikeXp>,
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
            _node: NodeRef<'_, LatexlikeXp>,
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
            _node: NodeRef<'_, LatexlikeXp>,
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
            _node: NodeRef<'_, LatexlikeXp>,
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
        (UnknownMacroPolicy::KeepSource, "a\\ruleless{x}b\n"),
        (UnknownMacroPolicy::Placeholder, "a<ruleless>b\n"),
    ];
    for (policy, expected) in cases {
        let converter = Converter::builder()
            .definitions(with_ruleless())
            .unknown_macro(policy)
            .build()
            .expect("builds");
        let conversion = converter.latex_to_text(r"a\ruleless{x}b").expect("parses");
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
        (UnknownSpecialsPolicy::EmitChars, "a @@ b\n"),
        (UnknownSpecialsPolicy::Skip, "a b\n"),
    ];
    for (policy, expected) in cases {
        let converter = Converter::builder()
            .definitions(with_ruleless())
            .unknown_specials(policy)
            .build()
            .expect("builds");
        let conversion = converter.latex_to_text("a @@ b").expect("parses");
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
        .definitions(with_ruleless())
        .unknown_macro(UnknownMacroPolicy::KeepSource)
        .wrap_width(Some(8))
        .build()
        .expect("builds");
    // The re-emitted source is one unbreakable run, so it overflows rather than
    // splitting in the middle of a macro name.
    assert_eq!(
        converter
            .latex_to_text(r"aa \ruleless{bbbbbbbbbb} cc")
            .expect("parses")
            .text,
        "aa\n\\ruleless{bbbbbbbbbb}\ncc\n"
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

#[test]
fn the_surplus_past_the_retention_cap_is_counted_not_forgotten() {
    // techy retains a thousand diagnostics and *counts* the rest, so that a report can
    // end with "… and N more" rather than pretend the surplus never happened. The
    // merge of the parse-side and render-side collections has to carry that count
    // across, along with the error count that decides `has_errors`.
    let document: String = (0..1500)
        .map(|index| format!("\\nosuchmacro{index} "))
        .collect();
    let conversion = Converter::standard()
        .latex_to_text(&document)
        .expect("tolerant recovery keeps going");

    assert_eq!(
        conversion.diagnostics.len(),
        1000,
        "the retention cap holds"
    );
    assert_eq!(
        conversion.diagnostics.suppressed(),
        500,
        "the surplus was counted"
    );
    // Every one of them is an unknown-macro warning, so nothing is an error — and the
    // count is of *all* of them, retained and suppressed alike.
    assert!(!conversion.diagnostics.has_errors());
    assert_eq!(conversion.diagnostics.error_count(), 0);
    assert!(!conversion.diagnostics.is_empty());
}

#[test]
fn a_suppressed_error_still_counts_as_an_error() {
    // The counters must survive the merge even when what they count is gone: a
    // conversion whose error is pushed past the retention cap must still answer
    // `has_errors`, which is what the CLI turns into exit code 1.
    struct AlwaysFails;

    impl core::fmt::Debug for AlwaysFails {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("AlwaysFails")
        }
    }

    impl TextHandler for AlwaysFails {
        fn render(
            &self,
            _node: NodeRef<'_, LatexlikeXp>,
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

    // A thousand and five warnings fill the render-side collection and start its
    // suppression count; the error comes after them, and is suppressed too.
    let mut document: String = (0..1005)
        .map(|index| format!("\\nosuchmacro{index} "))
        .collect();
    document.push_str(r"\emph{x}");
    let conversion = converter
        .latex_to_text(&document)
        .expect("tolerant recovery keeps going");

    assert_eq!(conversion.diagnostics.len(), 1000);
    assert_eq!(conversion.diagnostics.suppressed(), 6);
    assert!(
        conversion.diagnostics.has_errors(),
        "the error was suppressed out of existence"
    );
    assert_eq!(conversion.diagnostics.error_count(), 1);
    // …and it is nowhere in the retained entries, which is the point.
    assert_eq!(
        conversion.diagnostics.conditions::<HandlerFailed>().count(),
        0
    );
}
