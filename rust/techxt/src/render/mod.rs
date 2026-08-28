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
//! | group | a math group is a whole formula, folded in math mode and joined (§9.5); a `\verb` group emits its raw text; every other group is transparent: `{brave}` renders as `brave`, never as `{brave}` |
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
//! ## One renderer, every language
//!
//! A renderer folds a tree of **any** [`RenderLang`] (PLAN.md §11.1) — it implements
//! [`Recomposer<LLL, ()>`](Recomposer) for every one of them at once — so
//! [`Converter::renderer`](crate::Converter::renderer) takes no language parameter and
//! what it hands back is specialized to nothing. Which language a run is over is settled
//! by the tree handed to `recompose`, and a wrapper of your own names its own language
//! in its `impl Recomposer<…>` exactly as it always did.
//!
//! One call leaves rustc without an answer, and it is the only one: a [`Recomposer`]
//! method invoked on the renderer *directly*, when nothing in the arguments names the
//! language. [`observe_descent_warning`](Recomposer::observe_descent_warning) is that
//! case — its one argument is a [`DescentWarning`], which says nothing about the tree —
//! so it is called as
//! `Recomposer::<LatexlikeXp, ()>::observe_descent_warning(&mut renderer, warning)`,
//! naming whichever language the run is over. Everything reached *through* a fold, and
//! every inherent method ([`options`](TextRenderer::options),
//! [`finish`](TextRenderer::finish)), is unambiguous as it stands.
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
//!
//! **A formula is not such a construct.** A math group has to see its whole contents
//! before any of them can become text — that is what the joiner is for (PLAN.md §9.5) —
//! but it gets them without folding them itself: it answers with an ordinary
//! instruction to descend and attaches the joining to it as post-processing
//! ([`ConcatPieces::map`]). The driver still does the descending, so a wrapper's
//! overrides reach the `x` inside `$x + y$` exactly as they reach the `x` in `{x}`.
//!
//! Inside a formula the currency changes, and an override there has to know it: the
//! pieces are [`MathAtom`](crate::flow::FlowItem::MathAtom)s, and a run of anything
//! else — [`Text`](crate::flow::FlowItem::Text), glue, a verbatim fragment — is read by
//! the joiner as *one* opaque text atom, spaced against its neighbours but never
//! respaced inside. An override that wants the joiner's spacing emits atoms of its own
//! ([`mathfmt`](crate::mathfmt) builds them); one that does not, and simply replaces
//! `x` with `X`, gets `X` where `x` would have been.
//!
//! The guarantee that goes with the atom stands either way: **an atom never leaves the
//! formula it belongs to.** The post-processing function converts the whole scope
//! before its piece reaches the formula's parent, so no atom reaches the layout engine,
//! and none survives into the piece a run finally yields.

mod cx;
mod lang;
pub(crate) mod math;
mod rules;
mod source;
mod state;
mod view;

pub use cx::{RenderCx, RenderError};
pub use lang::RenderLang;
pub use state::{FloatKind, ListCtx, ListKind, MathCtx, RenderState, TableCtx};
pub use view::NodeView;

pub(crate) use cx::{FoldOn, RendererOps};

use alloc::string::String;
use core::any::Any;
use core::convert::Infallible;

use techy::core::constructs::DescentLimitApproaching;
use techy::core::node::{NodeKind, NodeRef};
use techy::core::DescentWarning;
use techy::error::{Diagnostic, Diagnostics, Severity};
use techy::latexlike::{EnvironmentSpec, LatexlikeGroupType, MathGroupForm, VerbatimBehavior};
use techy::recompose::{ConcatPieces, Recompose, RecomposeContext, Recomposer};

use crate::convert::{FootnoteStyle, Options};
use crate::def::{embedded_rule, is_refusal, CallableKind, RuleTable};
use crate::diag::{RenderAborted, TechxtCondition};
use crate::flow::{display_width, BlockKind, Flow, FlowItem, VerbatimProvenance};

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
    pub(crate) fn note_document_span<LLL: RenderLang>(&mut self, node: NodeRef<'_, LLL>) {
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
    fn chars<LLL: RenderLang>(&self, node: NodeRef<'_, LLL>, state: &RenderState) -> Flow {
        // `chars()` reads the node's own payload, resolved against the node's own
        // source: the one text-reading operation that is safe on any tree (PLAN.md
        // §1.6). `span_content()` would be wrong on a transformed tree, silently.
        let text = node.chars().unwrap_or("");

        if in_verbatim_group(node) {
            let mut flow = Flow::new();
            flow.push(FlowItem::InlineVerbatim {
                text: text.into(),
                provenance: VerbatimProvenance::Verbatim,
            });
            return flow;
        }
        if in_verbatim_body(node) {
            let mut flow = Flow::new();
            flow.push(FlowItem::Verbatim {
                text: text.into(),
                provenance: VerbatimProvenance::Verbatim,
            });
            return flow;
        }

        if state.in_math() {
            // A formula's own spacing is computed, so the source's is noise: `$4 \pi c$`
            // and `$4\pi c$` are the same formula and must render alike (PLAN.md §9.1).
            let stripped: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if stripped.is_empty() {
                return Flow::new();
            }
            // These *are* document characters, so the math alphabet applies here — and
            // only here (DECISIONS.md D6).
            let styled = self.styled(&stripped, state);
            if math::atoms_in_use(state, &self.config.options) {
                // DECISIONS.md D4: a run of upright latin letters may be read as a
                // function name exactly when variables are being styled, so that the
                // name stands out from them.
                return math::segmented(&styled, state.math_font.is_style());
            }
            return Flow::text(&styled);
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
    fn comment<LLL: RenderLang>(&self, node: NodeRef<'_, LLL>) -> Flow {
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
    fn group<LLL: RenderLang>(
        &mut self,
        node: NodeRef<'_, LLL>,
        state: &RenderState,
    ) -> Recompose<Flow, RenderState> {
        // Asked through the language's own group-class roles rather than by matching a
        // concrete enum, so that every latexlike language answers (PLAN.md §11.1).
        if let Some(group_type) = node.group_type() {
            if let Some(form) = group_type.math_form() {
                return self.math_group(node, state, form);
            }
            if group_type == LLL::GroupTypeId::verbatim_group() {
                // One raw characters child, by construction. `Emit` prunes it, so it is
                // read here rather than folded.
                let raw = node.child(0).and_then(|child| child.chars()).unwrap_or("");
                let mut flow = Flow::new();
                flow.push(FlowItem::InlineVerbatim {
                    text: raw.into(),
                    provenance: VerbatimProvenance::Verbatim,
                });
                return Recompose::Emit(flow);
            }
        }
        // Ordinary grouping is transparent: `{brave}` renders as `brave`. The braces
        // were syntax, not content.
        Recompose::Concat(ConcatPieces::children())
    }

    /// A math group — `$…$`, `\(…\)`, `\[…\]`, `$$…$$` (PLAN.md §9.5).
    ///
    /// A math group is a **math scope**: its contents are folded with
    /// [`RenderState::math`] set, and the [`MathAtom`](FlowItem::MathAtom)s they produce
    /// go to the joiner, which decides the spacing. What comes back out is ordinary
    /// text, glue and preformatted lines — see [`math::finish`] — so an atom never
    /// escapes the formula it belongs to.
    ///
    /// The joiner has to see the whole formula at once, but that is *not* a reason to
    /// fold the children here. Doing so would take the driver out of the loop, and with
    /// it any recomposer wrapping this one — the wrapping contract of PLAN.md §3 would
    /// stop at the `$`. So the answer is an ordinary instruction to descend, with the
    /// scope's closing attached to it as post-processing
    /// ([`ConcatPieces::map`]): the driver folds the children through the run's
    /// outermost recomposer, and only then is the assembled flow handed to
    /// [`math::scope_closer`]'s function, which joins it and converts it at the flow
    /// boundary.
    ///
    /// A formula's interior is therefore descended into by the driver like any other
    /// subtree, which also puts it under the run's descent guard — one policy for the
    /// whole document rather than an exception for math.
    ///
    /// In [`Source`](crate::convert::MathMode::Source) mode there is nothing to fold:
    /// the formula is re-emitted as the LaTeX it was written as, from node payloads
    /// (PLAN.md §1.6). That is [`math::source_scope`]'s job rather than this method's,
    /// because a math group is only one of the ways a formula is opened — a math
    /// environment is another — and all of them have to answer alike.
    fn math_group<LLL: RenderLang>(
        &mut self,
        node: NodeRef<'_, LLL>,
        state: &RenderState,
        form: MathGroupForm,
    ) -> Recompose<Flow, RenderState> {
        let display = form == MathGroupForm::Display;

        if let Some(flow) = math::source_scope(NodeView::of(node), display, &self.config.options) {
            return Recompose::Emit(flow);
        }

        let mut inner = state.clone();
        inner.math = Some(MathCtx {
            display,
            matrix: false,
        });
        Recompose::Concat(
            ConcatPieces::children()
                .with_state(inner)
                .map(math::scope_closer(display, &self.config.options)),
        )
    }

    /// A callable: find its rule (PLAN.md §10.3), then run it (PLAN.md §10.4).
    fn callable<LLL: RenderLang>(
        &mut self,
        node: NodeRef<'_, LLL>,
        state: &RenderState,
        cx: &mut RecomposeContext<'_, LLL, ()>,
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

        // Step 4 (PLAN.md §10.6): no rule, so the unknown-construct policy decides — and
        // says so, unless the parse has already named this construct better than
        // `techxt.unknown-macro` could. A techy-xp refusal is exactly that case.
        let diagnose = !is_refusal(node);
        let mut fold = FoldOn::new(self, cx, node);
        let mut render_cx = RenderCx::new(&mut fold, state);
        let flow = match rule {
            Some(rule) => rules::execute(rule, &mut render_cx),
            None => rules::unknown(kind, diagnose, &mut render_cx),
        };
        Recompose::Emit(flow)
    }
}

impl<LLL: RenderLang> RendererOps<LLL> for TextRenderer<'_> {
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

impl<LLL: RenderLang> Recomposer<LLL, ()> for TextRenderer<'_> {
    type State = RenderState;
    type Piece = Flow;
    /// The fold itself never fails: a rule that cannot render its construct becomes a
    /// diagnostic and an empty piece, so that one broken construct costs the reader
    /// that construct and nothing else. The single abort — techy's descent guard — is
    /// reported by the driver, not by the recomposer.
    type Error = Infallible;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, LLL, ()>,
        state: &RenderState,
        cx: &mut RecomposeContext<'_, LLL, ()>,
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

    /// Report that the run is approaching techy's descent limit (DECISIONS.md C3, D12).
    ///
    /// This is the guard's *early notice*, raised while the fold is going perfectly
    /// well — under the default budget a moderately deep document reaches it and then
    /// finishes normally. So it is a warning, and it is techy's own
    /// `core.constructs.descent-limit-approaching`: the exact condition the parse side
    /// of the same guard records, so that a fold-side near-miss reads identically to a
    /// parse-side one. Reporting it as [`RenderAborted`] instead would claim an error
    /// on a conversion that produced complete, correct text — and a caller who treats
    /// `has_errors()` as the exit code would fail the run for it.
    fn observe_descent_warning(&mut self, warning: DescentWarning) {
        if let Some(span) = self.run.document_span.clone() {
            self.run.diagnostics.push(Diagnostic::new(
                Severity::Warning,
                DescentLimitApproaching::new(warning.detail),
                span,
            ));
        }
    }
}

/// Whether these characters are the raw contents of a `\verb`-style group.
///
/// The structural test is the reliable one: techy parses `\verb|x_1|` into a group of
/// class [`GroupType::Verbatim`] holding one characters node.
fn in_verbatim_group<LLL: RenderLang>(node: NodeRef<'_, LLL>) -> bool {
    node.parent()
        .and_then(|parent| parent.group_type())
        .is_some_and(|group_type| group_type == LLL::GroupTypeId::verbatim_group())
}

/// Whether these characters are the raw body of a verbatim environment.
///
/// Asked of the environment's definition rather than guessed from the text — see
/// [`is_verbatim_environment`], which is where the two definitions that can say so are
/// consulted.
fn in_verbatim_body<LLL: RenderLang>(node: NodeRef<'_, LLL>) -> bool {
    let Some(body_list) = node.parent() else {
        return false;
    };
    let Some(environment) = body_list.parent() else {
        return false;
    };
    if !is_verbatim_environment(environment) {
        return false;
    }
    // Only the characters the body parser *designated* are raw: the newline techy
    // gobbles after `\begin{verbatim}` is a sibling of the body content, and an
    // argument of the same environment is not body at all.
    environment
        .body()
        .is_some_and(|body| body.iter().any(|content| content.id() == node.id()))
}

/// Whether this environment node's definition says its body is raw.
///
/// Two definitions can say so, and the order they are asked in is the point.
///
/// **techxt's own comes first.** A techxt [`EnvDef`](crate::def::EnvDef) records
/// [`EnvBodyKind::Verbatim`](crate::def::EnvBodyKind), and when the tree is techxt's
/// that record is authoritative: it is the very declaration the body was parsed from,
/// so nothing else can be more accurate about it.
///
/// **techy's own is asked second**, and only when the first downcast missed — which is
/// what a foreign tree looks like (PLAN.md §10.3 step 3). A foreign environment carries
/// no techxt payload at all, and among everything it *can* carry, techy's
/// [`VerbatimBehavior`] is the one thing that says *raw body*: it is the behaviour techy
/// itself registers a `verbatim` environment with, and the parse it produced is exactly
/// the parse techxt's own verbatim body kind produces. Consulting it is what makes a
/// foreign `verbatim` render byte for byte as a native one does, instead of having its
/// body reflowed into running words.
fn is_verbatim_environment<LLL: RenderLang>(environment: NodeRef<'_, LLL>) -> bool {
    if let Some(behavior) = crate::def::TechxtEnvironmentBehavior::of(environment) {
        return behavior.body_kind() == crate::def::EnvBodyKind::Verbatim;
    }
    environment.spec().is_some_and(|spec| {
        (&**spec as &dyn Any)
            .downcast_ref::<EnvironmentSpec<LLL>>()
            .is_some_and(|spec| (spec.behavior() as &dyn Any).is::<VerbatimBehavior<LLL>>())
    })
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
    use techy_xp::lang::LatexlikeXp;

    use super::*;
    use crate::convert::Converter;
    use crate::def::{TextHandler, TextRule};
    use crate::layout::{render, LayoutOptions};

    fn parse(converter: &Converter, latex: &str) -> NodeTree<LatexlikeXp> {
        converter.language().parse(latex).expect("parses").tree
    }

    /// A handler that renders its argument under a state it derives itself.
    #[derive(Debug)]
    struct EnterFigure;

    impl TextHandler for EnterFigure {
        fn render(
            &self,
            _node: NodeView<'_>,
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
            _node: NodeView<'_>,
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

    impl Recomposer<LatexlikeXp, ()> for ShoutingWrapper<'_> {
        type State = RenderState;
        type Piece = Flow;
        type Error = Infallible;

        fn recompose_node(
            &mut self,
            node: NodeRef<'_, LatexlikeXp, ()>,
            state: &RenderState,
            cx: &mut RecomposeContext<'_, LatexlikeXp, ()>,
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
    fn a_wrappers_override_is_consulted_inside_a_formula() {
        // The whole point of closing a math scope with `ConcatPieces::map` instead of
        // folding the children here (DECISIONS.md D24): the driver does the descending,
        // so the wrapper's override applies inside `$…$` and inside `\[…\]` just as it
        // does in running text — braces in the way included.
        let converter = Converter::standard();
        let tree = parse(&converter, r"a ${x} + y$ b \[z\]");

        let mut wrapper = ShoutingWrapper {
            inner: converter.renderer(),
        };
        let flow = TreeRecomposer::new(&mut wrapper)
            .recompose(&tree, RenderState::initial(converter.options()))
            .expect("no refusal");

        // Uppercased, so the characters went through the wrapper; joined and spaced,
        // so the formula still closed its own scope around them.
        assert_eq!(
            render(&flow, &LayoutOptions::default()),
            "A X + Y B\n\n    Z\n"
        );
        // And the guarantee that survives the change: an atom never leaves the formula
        // it belongs to, so no wrapper — this one or any other — is ever handed one.
        assert!(!flow
            .items()
            .iter()
            .any(|item| matches!(item, FlowItem::MathAtom(_))));
    }

    /// A consumer's recomposer that renders one macro as a math atom of its own.
    struct StarWrapper<'a> {
        inner: TextRenderer<'a>,
    }

    impl Recomposer<LatexlikeXp, ()> for StarWrapper<'_> {
        type State = RenderState;
        type Piece = Flow;
        type Error = Infallible;

        fn recompose_node(
            &mut self,
            node: NodeRef<'_, LatexlikeXp, ()>,
            state: &RenderState,
            cx: &mut RecomposeContext<'_, LatexlikeXp, ()>,
        ) -> Result<Recompose<Flow, RenderState>, Infallible> {
            if node.macro_name() == Some("star") {
                let mut flow = Flow::new();
                flow.push(FlowItem::MathAtom(crate::mathfmt::Atom::from_text(
                    "★",
                    crate::mathfmt::AtomClass::both(crate::mathfmt::AtomClass::Bin),
                )));
                return Ok(Recompose::Emit(flow));
            }
            self.inner.recompose_node(node, state, cx)
        }
    }

    #[test]
    fn a_wrappers_override_inside_a_formula_takes_part_in_the_joining() {
        // An override inside a formula is not merely *reached*: what it emits is the
        // scope's own currency, so an atom it builds is spaced by the joiner like any
        // other. Here a binary operator gets the spaces a binary operator gets.
        let converter = Converter::standard();
        let tree = parse(&converter, r"$a \star b$");

        let mut wrapper = StarWrapper {
            inner: converter.renderer(),
        };
        let flow = TreeRecomposer::new(&mut wrapper)
            .recompose(&tree, RenderState::initial(converter.options()))
            .expect("no refusal");
        assert_eq!(
            render(&flow, &LayoutOptions::default()),
            "\u{1d44e} ★ \u{1d44f}\n"
        );
    }

    #[test]
    fn a_wrappers_override_does_not_reach_a_math_environments_body() {
        // The counterpart, and *not* a math-specific rule: a math environment is
        // rendered by a techxt rule, and a rule folds its body through the renderer it
        // holds — the inner one. See the module documentation on the reach of a
        // wrapper. `\[…\]` above is a math *group*, which no rule renders.
        let converter = Converter::standard();
        let tree = parse(&converter, r"\begin{equation}z\end{equation}");

        let mut wrapper = ShoutingWrapper {
            inner: converter.renderer(),
        };
        let flow = TreeRecomposer::new(&mut wrapper)
            .recompose(&tree, RenderState::initial(converter.options()))
            .expect("no refusal");
        assert_eq!(render(&flow, &LayoutOptions::default()), "    \u{1d467}\n");
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
    fn a_descent_warning_is_a_warning_and_not_an_abort() {
        // DECISIONS.md C3 and D12: the warning arrives while there is still output to
        // save, so it must not claim the run was abandoned. It carries techy's own
        // approaching-limit condition, at warning severity, exactly as the parse side
        // of the same guard records it.
        let converter = Converter::standard();
        let tree = parse(&converter, "a");
        let mut renderer = converter.renderer();
        renderer.note_document_span(tree.root());
        // The renderer recomposes the tree of *any* `RenderLang`, so a `Recomposer`
        // method called on it directly — rather than through a fold, where the tree
        // pins the language — has to say which one. See "One renderer, every language"
        // in the module documentation: this is the only call that needs it.
        Recomposer::<LatexlikeXp, ()>::observe_descent_warning(
            &mut renderer,
            DescentWarning {
                detail: String::from("half the budget is gone"),
            },
        );
        let finish = renderer.finish();
        assert_eq!(
            finish
                .diagnostics
                .with_identifier("core.constructs.descent-limit-approaching")
                .count(),
            1
        );
        assert!(finish
            .diagnostics
            .with_identifier("techxt.render-aborted")
            .next()
            .is_none());
        // The whole point: a conversion that completed reports no error.
        assert!(!finish.diagnostics.has_errors());
        let approaching: Vec<&DescentLimitApproaching> = finish.diagnostics.conditions().collect();
        assert_eq!(approaching.len(), 1);
        assert_eq!(approaching[0].detail, "half the budget is gone");
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
