//! The renderer: techy node trees folded into a [`Flow`] (PLAN.md §8–§10).
//!
//! [`TextRenderer`] is techxt's implementation of techy's [`Recomposer`] trait. techy's
//! driver walks the tree in document order and asks the renderer, node by node, what to
//! do: emit a finished piece, or concatenate the children (optionally under a modified
//! state). Pieces compose bottom-up into one [`Flow`], which the
//! [layout engine](crate::layout) then renders as text.
//!
//! Two kinds of state travel with the fold, and the split matters:
//!
//! - **[`RenderState`] goes down** and is restored automatically. Whether we are in
//!   math, which font is in effect, how deeply lists are nested, whether `&` means
//!   "next cell" — all context, all scoped, none of it needing a stack to unwind.
//! - **Run state stays in the renderer** and accumulates across siblings: heading
//!   counters, collected footnotes, the document's title. The fold is in document
//!   order, so counting in `&mut self` is correct.
//!
//! # What each kind of node does (PLAN.md §9.1)
//!
//! | node | rendering |
//! |---|---|
//! | characters | split into words and inter-word spaces; a blank line becomes a paragraph break; in a verbatim context, emitted untouched |
//! | comment | nothing at all — not even its trailing newline, which is what LaTeX itself does. With [`Options::keep_comments`] it gets a line of its own |
//! | group | math groups go to the math handler, `\verb` groups emit their raw text, and every other group is transparent: `{brave}` renders as `brave`, never as `{brave}` |
//! | list | transparent |
//! | callable | the dispatch chain of PLAN.md §10.3, then the rule it found |
//!
//! Whitespace policy is fixed and has no options: a macro's trailing space is
//! invocation syntax and is never emitted, source whitespace collapses to one
//! breakable space, blank lines normalize to one, math ignores source whitespace, and
//! verbatim preserves everything.
//!
//! # Wrapping the renderer
//!
//! `TextRenderer` is public because consumers are meant to build on it: write your own
//! [`Recomposer`] that handles the nodes you care about and delegates the rest to
//! techxt's. Share its `State`, `Piece` and `Error` types, never descend explicitly
//! (return instructions and let the driver descend), and finish like this:
//!
//! ```
//! use techxt::flow::Flow;
//! use techxt::layout::{render, LayoutOptions};
//! use techxt::render::RenderState;
//! use techxt::Converter;
//! use techy::recompose::TreeRecomposer;
//!
//! let converter = Converter::standard();
//! let tree = converter.language().parse("a b").expect("parses").tree;
//!
//! let mut renderer = converter.renderer();
//! let mut flow: Flow = TreeRecomposer::new(&mut renderer)
//!     .recompose(&tree, RenderState::initial(converter.options()))
//!     .expect("no descent-limit refusal");
//!
//! // The footnote block, and anything else collected during the run, comes last.
//! let finish = renderer.finish();
//! flow.extend(finish.trailing);
//! assert_eq!(render(&flow, &LayoutOptions::default()), "a b\n");
//! assert!(finish.diagnostics.is_empty());
//! ```
//!
//! ## How far a wrapper's overrides reach
//!
//! The driver holds exactly one recomposer and re-enters it for every child it
//! descends into, so a wrapper's overrides apply at every depth the driver reaches:
//! running text, groups, lists, and anything under them.
//!
//! They do **not** reach inside a construct whose techxt rule renders it. A rule reads
//! its arguments and its body through techy's re-entrant region operations, and those
//! fold through the recomposer they are handed — which is the `TextRenderer`, the only
//! one it has a handle on. So in `\emph{x}` a wrapper sees the `\emph` node and can
//! replace it wholesale, but if it delegates, the `x` inside is rendered by techxt.
//! Overriding the *rule* rather than the node is the way to change what happens in
//! there: [`ConverterBuilder::override_macro`](crate::ConverterBuilder::override_macro)
//! and its siblings sit at the front of the dispatch chain and apply wherever the
//! construct occurs.

mod cx;
mod rules;
mod source;
mod state;

pub use cx::{RenderCx, RenderError};
pub use state::{FloatKind, ListCtx, ListKind, MathCtx, RenderState, TableCtx};

pub(crate) use cx::RendererOps;

use alloc::string::String;
use core::convert::Infallible;

use techy::core::node::{NodeKind, NodeRef};
use techy::core::DescentWarning;
use techy::error::Diagnostics;
use techy::latexlike::{GroupType, Latexlike, MathGroupForm};
use techy::recompose::{ConcatPieces, Recompose, RecomposeContext, Recomposer};

use crate::convert::{FootnoteStyle, MathMode, Options};
use crate::def::{embedded_rule, CallableKind, RuleTable};
use crate::diag::{RenderAborted, TechxtCondition};
use crate::flow::{display_width, BlockKind, Flow, FlowItem};

use cx::RunState;

/// What a renderer needs that does not change from document to document.
///
/// Held by the [`Converter`](crate::Converter) behind an `Arc` and borrowed by every
/// renderer it makes, which is what lets one converter render many documents at once
/// while staying immutable.
#[derive(Clone, Debug)]
pub(crate) struct RenderConfig {
    pub(crate) options: Options,
    /// Dispatch step 1: rules the builder was told to use instead of the definitions'.
    pub(crate) overrides: RuleTable,
    /// Dispatch step 3: rules keyed by name, for constructs whose spec carries none.
    pub(crate) fallback: RuleTable,
}

/// What a finished run leaves behind (PLAN.md §11.1).
#[derive(Debug)]
pub struct RenderFinish {
    /// Content that belongs after the document body: the collected footnote block.
    ///
    /// Append it to the folded flow before laying the flow out.
    pub trailing: Flow,
    /// Everything the run reported.
    pub diagnostics: Diagnostics<Option<String>>,
}

/// techxt's [`Recomposer`]: folds a techy node tree into a [`Flow`] (PLAN.md §8).
///
/// One renderer converts one document — it carries that document's counters,
/// footnotes and diagnostics — and is made by
/// [`Converter::renderer`](crate::Converter::renderer). See the [module
/// documentation](self) for what it does with each kind of node, and for how to wrap it
/// in a recomposer of your own.
#[derive(Debug)]
pub struct TextRenderer<'a> {
    config: &'a RenderConfig,
    run: RunState,
}

impl<'a> TextRenderer<'a> {
    /// A renderer for one document, reading its rules and options from `config`.
    pub(crate) fn new(config: &'a RenderConfig) -> TextRenderer<'a> {
        TextRenderer {
            config,
            run: RunState::default(),
        }
    }

    /// The options this renderer converts with.
    pub fn options(&self) -> &Options {
        &self.config.options
    }

    /// Finish the run: hand back what belongs after the document, and everything the
    /// run reported (PLAN.md §11.1).
    pub fn finish(self) -> RenderFinish {
        let trailing = footnote_block(&self.run.footnotes, &self.config.options);
        RenderFinish {
            trailing,
            diagnostics: self.run.diagnostics,
        }
    }

    /// Note a document-wide span, so that a diagnostic with no node to point at still
    /// has a position. Called by the converter before the fold, and by the fold itself
    /// on the first node it sees.
    pub(crate) fn note_document_span(&mut self, node: NodeRef<'_, Latexlike>) {
        if self.run.document_span.is_none() {
            self.run.document_span = Some(node.span().clone());
        }
    }

    /// Report that the fold was abandoned, and hand back the empty output that goes
    /// with it (PLAN.md §10.4).
    pub(crate) fn abort(&mut self, detail: impl Into<String>) {
        if let Some(span) = self.run.document_span.clone() {
            self.run
                .diagnostics
                .push(RenderAborted::new(detail).diagnose(span));
        }
    }

    // ------------------------------------------------------------------ per kind

    /// Characters (PLAN.md §9.1).
    fn chars(&self, node: NodeRef<'_, Latexlike>, state: &RenderState) -> Flow {
        // `chars()` reads the node's own payload, resolved against the node's own
        // source: the one text-reading operation that is safe on any tree (PLAN.md
        // §1.6). `span_content()` would be wrong on a transformed tree, silently.
        let text = node.chars().unwrap_or("");

        if in_verbatim_group(node) {
            let mut flow = Flow::new();
            flow.push(FlowItem::InlineVerbatim(text.into()));
            return flow;
        }
        if in_verbatim_body(node) {
            let mut flow = Flow::new();
            flow.push(FlowItem::Verbatim(text.into()));
            return flow;
        }

        if state.in_math() {
            // TODO(M5b): PLAN.md §9.1 — math ignores source whitespace, and in Fancy
            // mode the text is segmented into `mathfmt` atoms so the joiner can space
            // it. Until the math engine lands, math subtrees are rendered in text mode
            // and this branch is only reachable through a wrapping recomposer that sets
            // `RenderState::math` itself; stripping the whitespace is the part of the
            // rule that is already settled.
            let stripped: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if stripped.is_empty() {
                return Flow::new();
            }
            return Flow::text(&self.styled(&stripped, state));
        }

        // Document text: words, inter-word spaces, and paragraph breaks for the
        // whitespace-only nodes techy makes out of blank lines.
        let styled = self.styled(text, state);
        Flow::from_plain_text(&styled)
    }

    /// Apply the font alphabet in effect, if any.
    ///
    /// Only characters that came from the document are styled (DECISIONS.md D6); text a
    /// rule produced never passes through here.
    fn styled(&self, text: &str, state: &RenderState) -> String {
        let (style, fallback) = if state.in_math() {
            (state.math_font, self.config.options.math_font)
        } else {
            (state.text_font, self.config.options.text_font)
        };
        match style.resolve(fallback) {
            Some(kind) => crate::mathfmt::fmt_style(text, kind),
            None => String::from(text),
        }
    }

    /// A comment (PLAN.md §9.1).
    ///
    /// By default a comment renders as *nothing at all* — not even the newline that
    /// ends it, which is exactly what LaTeX does and why `A% note` followed by `B` is
    /// one word `AB`.
    fn comment(&self, node: NodeRef<'_, Latexlike>) -> Flow {
        if !self.config.options.keep_comments {
            return Flow::new();
        }
        let Some(comment) = node.comment() else {
            return Flow::new();
        };
        // Payload fields, resolved against the node's own source (PLAN.md §1.6).
        let source = node.span().source();
        let mut text = String::from(comment.start.resolve(source));
        text.push_str(comment.content.resolve(source));
        // The comment keeps its own line, and no output line ever ends in whitespace.
        let text = text.trim_end();

        let mut flow = Flow::new();
        flow.push(FlowItem::HardBreak);
        flow.push(FlowItem::Text(text.into()));
        flow.push(FlowItem::HardBreak);
        flow
    }

    /// A group (PLAN.md §9.1).
    fn group(
        &self,
        node: NodeRef<'_, Latexlike>,
        state: &RenderState,
    ) -> Recompose<Flow, RenderState> {
        match node.group_type() {
            Some(GroupType::Math(form)) => self.math_group(node, state, form),
            Some(GroupType::Verbatim) => {
                // One raw characters child, by construction. `Emit` prunes it, so it is
                // read here rather than folded.
                let raw = node.child(0).and_then(|child| child.chars()).unwrap_or("");
                let mut flow = Flow::new();
                flow.push(FlowItem::InlineVerbatim(raw.into()));
                Recompose::Emit(flow)
            }
            // Ordinary grouping is transparent: `{brave}` renders as `brave`. The
            // braces were syntax, not content.
            _ => Recompose::Concat(ConcatPieces::children()),
        }
    }

    /// A math group (PLAN.md §9.5).
    ///
    /// **M5b seam.** `Source` mode is complete — it re-emits the subtree's LaTeX from
    /// node payloads — but `Fancy` and `Plain` are placeholders: they render the
    /// formula's content *in text mode*, so `$x^2$` comes out as its characters rather
    /// than as `𝑥²`. The math milestone replaces the marked branch with the atom
    /// pipeline of PLAN.md §9.5: fold the body with `RenderState::math` set, collect
    /// the [`MathAtom`](crate::flow::FlowItem::MathAtom)s, run `mathfmt::join_atoms`,
    /// and convert the resulting box to text (inline) or verbatim lines (display).
    /// The block structure around display math is already what §9.5 prescribes.
    fn math_group(
        &self,
        node: NodeRef<'_, Latexlike>,
        _state: &RenderState,
        form: MathGroupForm,
    ) -> Recompose<Flow, RenderState> {
        let display = form == MathGroupForm::Display;

        if self.config.options.math_mode == MathMode::Source {
            let latex = source::latex_source(node);
            let mut flow = Flow::new();
            flow.push(if display {
                FlowItem::Verbatim(latex.into())
            } else {
                FlowItem::InlineVerbatim(latex.into())
            });
            return Recompose::Emit(flow);
        }

        // TODO(M5b): fold in math mode and run the atom pipeline instead of this.
        if display {
            let mut open = Flow::new();
            open.push(FlowItem::BlockStart(BlockKind::Indent {
                first: "    ".into(),
                cont: "    ".into(),
            }));
            let mut close = Flow::new();
            close.push(FlowItem::BlockEnd);
            Recompose::Concat(ConcatPieces::children().wrap(open, close))
        } else {
            Recompose::Concat(ConcatPieces::children())
        }
    }

    /// A callable: find its rule (PLAN.md §10.3), then run it (PLAN.md §10.4).
    fn callable(
        &mut self,
        node: NodeRef<'_, Latexlike>,
        state: &RenderState,
        cx: &mut RecomposeContext<'_, Latexlike, ()>,
    ) -> Recompose<Flow, RenderState> {
        // Read the configuration *out* of `self` first: it is a shared reference with
        // the converter's lifetime, so the rule it yields outlives the mutable borrow
        // the context below needs.
        let config: &RenderConfig = self.config;
        let kind = CallableKind::of(node);
        let name = node.name().unwrap_or("");

        let rule = kind
            .and_then(|kind| config.overrides.get(kind, name))
            .or_else(|| embedded_rule(node))
            .or_else(|| kind.and_then(|kind| config.fallback.get(kind, name)));

        let mut render_cx = RenderCx::new(self, cx, node, state);
        let flow = match rule {
            Some(rule) => rules::execute(rule, &mut render_cx),
            None => rules::unknown(kind, &mut render_cx),
        };
        Recompose::Emit(flow)
    }
}

impl RendererOps for TextRenderer<'_> {
    fn run(&self) -> &RunState {
        &self.run
    }

    fn run_mut(&mut self) -> &mut RunState {
        &mut self.run
    }

    fn options(&self) -> &Options {
        &self.config.options
    }
}

impl Recomposer<Latexlike, ()> for TextRenderer<'_> {
    type State = RenderState;
    type Piece = Flow;
    /// The fold itself never fails: a rule that cannot render its construct becomes a
    /// diagnostic and an empty piece, so that one broken construct costs the reader
    /// that construct and nothing else. The single abort — techy's descent guard — is
    /// reported by the driver, not by the recomposer.
    type Error = Infallible;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, Latexlike, ()>,
        state: &RenderState,
        cx: &mut RecomposeContext<'_, Latexlike, ()>,
    ) -> Result<Recompose<Flow, RenderState>, Infallible> {
        self.note_document_span(node);
        Ok(match node.kind() {
            NodeKind::Chars { .. } => Recompose::Emit(self.chars(node, state)),
            NodeKind::Comment(_) => Recompose::Emit(self.comment(node)),
            NodeKind::Group(_) => self.group(node, state),
            NodeKind::List => Recompose::Concat(ConcatPieces::children()),
            NodeKind::Callable(_) => self.callable(node, state, cx),
        })
    }

    /// Report that the run is approaching techy's descent limit (DECISIONS.md C3).
    ///
    /// The warning arrives while there is still output to save, and it names the same
    /// condition the abort would: a document deep enough to warn is a document about to
    /// lose its rendering entirely.
    fn observe_descent_warning(&mut self, warning: DescentWarning) {
        self.abort(warning.detail);
    }
}

/// Whether these characters are the raw contents of a `\verb`-style group.
///
/// The structural test is the reliable one: techy parses `\verb|x_1|` into a group of
/// class [`GroupType::Verbatim`] holding one characters node.
fn in_verbatim_group(node: NodeRef<'_, Latexlike>) -> bool {
    node.parent()
        .and_then(|parent| parent.group_type())
        .is_some_and(|group_type| group_type == GroupType::Verbatim)
}

/// Whether these characters are the raw body of a verbatim environment.
///
/// Asked of the environment's definition rather than guessed from the text: a techxt
/// environment records what its body is (`EnvBodyKind::Verbatim`), which is the whole
/// point of one definition serving both parsing and rendering.
fn in_verbatim_body(node: NodeRef<'_, Latexlike>) -> bool {
    let Some(body_list) = node.parent() else {
        return false;
    };
    let Some(environment) = body_list.parent() else {
        return false;
    };
    let Some(behavior) = crate::def::TechxtEnvironmentBehavior::of(environment) else {
        return false;
    };
    if behavior.body_kind() != crate::def::EnvBodyKind::Verbatim {
        return false;
    }
    // Only the characters the body parser *designated* are raw: the newline techy
    // gobbles after `\begin{verbatim}` is a sibling of the body content, and an
    // argument of the same environment is not body at all.
    environment
        .body()
        .is_some_and(|body| body.iter().any(|content| content.id() == node.id()))
}

/// The block of collected footnotes that follows the document (PLAN.md §9.8, §11.1).
///
/// Numbered in document order — the fold visits nodes in document order — and separated
/// from the body by a rule, so that a reader can tell the notes from the text.
fn footnote_block(footnotes: &[Flow], options: &Options) -> Flow {
    if footnotes.is_empty() || options.footnote_style != FootnoteStyle::Collected {
        return Flow::new();
    }
    let mut flow = Flow::new();
    flow.push(FlowItem::ParagraphBreak);
    flow.push(FlowItem::Text("---".into()));
    for (index, note) in footnotes.iter().enumerate() {
        let first = alloc::format!("[{}] ", index + 1);
        let cont: String = core::iter::repeat_n(' ', display_width(&first)).collect();
        flow.push(FlowItem::BlockStart(BlockKind::Item {
            first: first.into_boxed_str(),
            cont: cont.into_boxed_str(),
        }));
        flow.extend(note.clone());
        flow.push(FlowItem::BlockEnd);
    }
    flow
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};
    use alloc::sync::Arc;

    use techy::core::node::NodeTree;
    use techy::core::StdDescentGuardInit;
    use techy::recompose::{RecomposeError, TreeRecomposer};

    use super::*;
    use crate::convert::Converter;
    use crate::def::{TextHandler, TextRule};
    use crate::layout::{render, LayoutOptions};

    fn parse(converter: &Converter, latex: &str) -> NodeTree<Latexlike> {
        converter.language().parse(latex).expect("parses").tree
    }

    /// A handler that renders its argument under a state it derives itself.
    #[derive(Debug)]
    struct EnterFigure;

    impl TextHandler for EnterFigure {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            let derived = RenderState {
                float: Some(FloatKind::Figure),
                ..cx.state().clone()
            };
            Ok(cx.arg_with_state("text", derived)?.unwrap_or_default())
        }
    }

    /// A handler that reports what state it was called in.
    #[derive(Debug)]
    struct ReportFloat;

    impl TextHandler for ReportFloat {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            Ok(Flow::text(match cx.state().float {
                Some(FloatKind::Figure) => "[figure]",
                Some(FloatKind::Table) => "[table]",
                None => "[none]",
            }))
        }
    }

    #[test]
    fn derived_state_reaches_the_argument_and_is_restored_afterwards() {
        let converter = Converter::builder()
            .override_macro("emph", TextRule::Handler(Arc::new(EnterFigure)))
            .override_macro("textbf", TextRule::Handler(Arc::new(ReportFloat)))
            .build()
            .expect("builds");
        // Inside the derived region the state is visible; outside it — and after the
        // region closes — techy has restored the parent's state with no unwinding of
        // techxt's own.
        assert_eq!(
            converter
                .latex_to_text(r"\textbf{}\emph{\textbf{}}\textbf{}")
                .expect("parses")
                .text,
            "[none][figure][none]\n"
        );
    }

    #[test]
    fn the_initial_state_comes_from_the_options() {
        let converter = Converter::builder()
            .override_macro("textbf", TextRule::Handler(Arc::new(ReportFloat)))
            .build()
            .expect("builds");
        let state = RenderState::initial(converter.options());
        assert!(state.float.is_none());
        assert!(!state.in_math());
        assert_eq!(
            converter.latex_to_text(r"\textbf{}").expect("parses").text,
            "[none]\n"
        );
    }

    /// A consumer's recomposer that overrides one node kind and delegates the rest —
    /// the wrapping contract of PLAN.md §3.
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
    fn the_renderer_can_be_wrapped_by_a_consumers_recomposer() {
        let converter = Converter::standard();
        let tree = parse(&converter, r"a \emph{b} \begin{myenv}c\end{myenv}");

        let mut wrapper = ShoutingWrapper {
            inner: converter.renderer(),
        };
        let flow = TreeRecomposer::new(&mut wrapper)
            .recompose(&tree, RenderState::initial(converter.options()))
            .expect("no refusal");
        let finish = wrapper.inner.finish();

        // The override applies to every node the *driver* descends into. It does not
        // reach the characters inside `\emph{b}` or inside the environment body,
        // because techxt's own rules fold those regions through the renderer they hold
        // — which is the inner one. See the module documentation's note on the reach of
        // a wrapper.
        // The `b` is italic because the shipped `\emph` italicizes it (defs::fontstyles);
        // what this test is about is that the wrapper's uppercasing did *not* reach it.
        assert_eq!(render(&flow, &LayoutOptions::default()), "A \u{1d44f} c\n");
        // And the inner renderer still reported what it saw.
        assert_eq!(
            finish
                .diagnostics
                .with_identifier("techxt.unknown-environment")
                .count(),
            1
        );
    }

    #[test]
    fn the_descent_guard_is_the_one_abort_and_it_is_reported() {
        let converter = Converter::standard();
        let tree = parse(&converter, "a {b {c}} d");

        let mut renderer = converter.renderer();
        let refused = TreeRecomposer::new(&mut renderer)
            .with_descent_guard_init(StdDescentGuardInit::depth_limit(1))
            .recompose(&tree, RenderState::initial(converter.options()));
        let error = refused.expect_err("one level of descent is not enough");
        assert!(matches!(error, RecomposeError::DescentLimitExceeded { .. }));

        // Which is what `Converter::tree_to_flow` turns into a diagnostic.
        renderer.abort(error.to_string());
        let finish = renderer.finish();
        let aborted: Vec<&crate::diag::RenderAborted> = finish.diagnostics.conditions().collect();
        assert_eq!(aborted.len(), 1);
        assert!(finish.diagnostics.has_errors());
    }

    #[test]
    fn a_descent_warning_is_reported_rather_than_passing_silently() {
        // DECISIONS.md C3: the warning arrives while there is still output to save.
        let converter = Converter::standard();
        let tree = parse(&converter, "a");
        let mut renderer = converter.renderer();
        renderer.note_document_span(tree.root());
        renderer.observe_descent_warning(DescentWarning {
            detail: String::from("half the budget is gone"),
        });
        let finish = renderer.finish();
        assert_eq!(
            finish
                .diagnostics
                .with_identifier("techxt.render-aborted")
                .count(),
            1
        );
    }

    #[test]
    fn footnotes_are_numbered_in_document_order_and_gathered_at_the_end() {
        let converter = Converter::standard();
        let mut renderer = converter.renderer();
        // Registered the way a handler registers them, through the run state.
        renderer.run.footnotes.push(Flow::from_plain_text("first"));
        renderer.run.footnotes.push(Flow::from_plain_text("second"));

        let finish = renderer.finish();
        assert_eq!(
            render(&finish.trailing, &LayoutOptions::default()),
            "---\n[1] first\n[2] second\n"
        );
    }

    #[test]
    fn a_footnote_style_that_does_not_collect_leaves_no_trailing_block() {
        let converter = Converter::builder()
            .footnote_style(crate::convert::FootnoteStyle::Skip)
            .build()
            .expect("builds");
        let mut renderer = converter.renderer();
        renderer
            .run
            .footnotes
            .push(Flow::from_plain_text("dropped"));
        assert!(renderer.finish().trailing.is_empty());
    }

    #[test]
    fn there_is_no_trailing_block_without_footnotes() {
        let converter = Converter::standard();
        assert!(converter.renderer().finish().trailing.is_empty());
    }
}
