//! The handler-facing context (PLAN.md §10.4) and the error a rule can fail with.

use alloc::string::String;
use alloc::vec::Vec;
use core::convert::Infallible;

use techy::core::node::{NodeRef, ParsedArguments};
use techy::error::{Diagnostic, Diagnostics};
use techy::recompose::{RecomposeContext, RecomposeError, Recomposer};
use techy::source::SourceSpan;

use crate::convert::Options;
use crate::diag::TechxtCondition;
use crate::flow::Flow;
use crate::layout::render_inline;

use super::math;
use super::state::{ListKind, RenderState};
use super::{NodeView, RenderLang};

/// Why a rule could not render a construct (PLAN.md §10.4).
///
/// Neither variant aborts the conversion: the renderer turns a `RenderError` into a
/// diagnostic, renders the construct as nothing, and carries on with the document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// A re-entrant region operation failed: the rule asked for an argument, a slot or
    /// a body that the node does not have.
    ///
    /// This means a definition and the code rendering it disagree — a techxt bug rather
    /// than a document problem — because the definition is what shaped the invocation
    /// in the first place.
    Region {
        /// What the fold reported.
        detail: String,
    },
    /// A [`TextHandler`](crate::def::TextHandler) reported that it could not render the
    /// construct.
    Handler {
        /// The construct as written (`\href`, `tabular`).
        construct: String,
        /// What went wrong.
        detail: String,
    },
}

impl RenderError {
    /// A region-operation failure, described by the fold's own error.
    pub(crate) fn region(error: RecomposeError<Infallible>) -> RenderError {
        RenderError::Region {
            detail: alloc::format!("{error}"),
        }
    }

    /// A region-operation failure described in techxt's own words (a node that is not a
    /// callable at all, say).
    pub(crate) fn region_detail(detail: impl Into<String>) -> RenderError {
        RenderError::Region {
            detail: detail.into(),
        }
    }

    /// What to say in a `techxt.handler-failed` diagnostic about this failure.
    pub(crate) fn detail(&self) -> &str {
        match self {
            RenderError::Region { detail } => detail,
            RenderError::Handler { detail, .. } => detail,
        }
    }
}

impl core::fmt::Display for RenderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RenderError::Region { detail } => write!(f, "region operation failed: {detail}"),
            RenderError::Handler { construct, detail } => {
                write!(f, "‘{construct}’ could not be rendered: {detail}")
            }
        }
    }
}

impl core::error::Error for RenderError {}

/// Everything a conversion accumulates that is not a piece of the output (PLAN.md §8).
///
/// One of these exists per conversion, inside the renderer. It is not downward state:
/// heading numbers must keep counting across siblings, footnotes are collected in
/// document order and emitted at the end, and `\title` may be set anywhere and used
/// somewhere else entirely.
// The sectioning and list counters live here rather than in the downward state because
// PLAN.md §8 puts them here, and because their *lifetime rules* — document order for the
// counters, one push per open list — are what the renderer core guarantees to the
// handlers in the definitions library that read them.
#[derive(Debug, Default)]
pub(crate) struct RunState {
    pub(crate) diagnostics: Diagnostics<Option<String>>,
    pub(crate) heading_counters: [u32; 7],
    pub(crate) chapter_seen: bool,
    pub(crate) list_counter_stack: Vec<u32>,
    /// The kinds of the open list environments, innermost last — the same stack
    /// discipline as `list_counter_stack`, and pushed and popped alongside it.
    ///
    /// `RenderState::list` names only the *innermost* list, which cannot answer
    /// PLAN.md §9.4's question "how many enclosing lists are of this kind?" once a list
    /// of another kind intervenes: an `itemize` inside an `enumerate` inside an
    /// `itemize` is a second-level itemize, and the innermost context alone cannot say
    /// so. Keeping the enclosing kinds here answers it without widening the public
    /// downward state.
    pub(crate) list_kind_stack: Vec<ListKind>,
    pub(crate) footnotes: Vec<Flow>,
    pub(crate) doc_title: Option<Flow>,
    pub(crate) doc_author: Option<Flow>,
    pub(crate) doc_date: Option<Flow>,
    /// A span covering the whole document, for diagnostics with no node to point at.
    pub(crate) document_span: Option<SourceSpan<Option<String>>>,
}

/// The renderer as the fold sees it.
///
/// The fold needs two things from the renderer at once: to be the recomposer that
/// techy's region operations fold *through* (so nested constructs are rendered by the
/// same rules), and to be the owner of the run's counters and diagnostics. Erasing it
/// behind this trait is what lets [`FoldOn`] borrow it once and keep both, without the
/// renderer's own lifetime parameter leaking any further.
pub(crate) trait RendererOps<LLL: RenderLang>:
    Recomposer<LLL, (), State = RenderState, Piece = Flow, Error = Infallible>
{
    fn run(&self) -> &RunState;
    fn run_mut(&mut self) -> &mut RunState;
    fn options(&self) -> &Options;
}

/// Everything [`RenderCx`] does to the fold, with the tree's language erased.
///
/// This is the seam that keeps [`TextHandler`](crate::def::TextHandler) non-generic
/// (PLAN.md §11.1): the renderer, the recompose context and the node all name the
/// [`RenderLang`](super::RenderLang) `LLL`, and all three are erased together behind one
/// trait object, of which only the tree's lifetime `'t` survives. The one implementor is
/// [`FoldOn`].
pub(crate) trait Fold<'t> {
    /// The node being rendered.
    fn node(&self) -> NodeView<'t>;

    /// Fold the content of the argument called `name`, under `state`.
    fn arg_named(&mut self, name: &str, state: &RenderState) -> Result<Flow, RenderError>;

    /// Fold the content of the argument at `index` in declaration order, under `state`.
    fn arg_at(&mut self, index: usize, state: &RenderState) -> Result<Flow, RenderError>;

    /// Fold the node's environment body, under `state`.
    fn body(&mut self, state: &RenderState) -> Result<Flow, RenderError>;

    /// Fold the content of the slot called `name`, or answer `None` when the node has
    /// no such slot.
    fn slot_named(&mut self, name: &str, state: &RenderState) -> Result<Option<Flow>, RenderError>;

    /// Whether the argument called `name` was written; `Err` when none is declared.
    fn argument_provided(&self, name: &str) -> Result<bool, RenderError>;

    /// Whether the argument at `index` was written; `Err` when the index has none.
    fn argument_provided_at(&self, index: usize) -> Result<bool, RenderError>;

    /// How many arguments the node's definition declares.
    fn argument_count(&self) -> usize;

    /// The run's accumulated state.
    fn run(&self) -> &RunState;

    /// The run's accumulated state, to add to.
    fn run_mut(&mut self) -> &mut RunState;

    /// The conversion's options.
    fn options(&self) -> &Options;

    /// [`argument_provided`](Self::argument_provided), reading an undeclared argument as
    /// absent rather than as a failure.
    fn arg_provided(&self, name: &str) -> bool {
        self.argument_provided(name).unwrap_or(false)
    }

    /// [`argument_provided_at`](Self::argument_provided_at), reading an index the
    /// definition does not have as absent.
    fn arg_provided_at(&self, index: usize) -> bool {
        self.argument_provided_at(index).unwrap_or(false)
    }
}

/// The fold, positioned on one node of one tree (PLAN.md §10.4).
///
/// Note the three lifetimes and why they differ: techy hands
/// [`recompose_node`](Recomposer::recompose_node) the node and the context with
/// *unrelated* lifetimes, so `'t` is the tree's — the one a [`NodeView`] keeps and a
/// handler may hold on to — while the context's own `'c` is visible only here, folded
/// away into the borrow `'a` as soon as this is erased behind [`Fold`].
pub(crate) struct FoldOn<'a, 'c, 't, LLL: RenderLang> {
    renderer: &'a mut dyn RendererOps<LLL>,
    cx: &'a mut RecomposeContext<'c, LLL, ()>,
    node: NodeRef<'t, LLL, ()>,
}

impl<'a, 'c, 't, LLL: RenderLang> FoldOn<'a, 'c, 't, LLL> {
    /// The fold as it stands at `node`.
    pub(crate) fn new(
        renderer: &'a mut dyn RendererOps<LLL>,
        cx: &'a mut RecomposeContext<'c, LLL, ()>,
        node: NodeRef<'t, LLL, ()>,
    ) -> FoldOn<'a, 'c, 't, LLL> {
        FoldOn { renderer, cx, node }
    }

    /// The node's parsed arguments, or a region error naming what went wrong.
    fn arguments(&self) -> Result<&'t ParsedArguments<LLL>, RenderError> {
        self.node.arguments().ok_or_else(|| {
            RenderError::region_detail("asked for an argument of a node that is not a callable")
        })
    }
}

impl<'t, LLL: RenderLang> Fold<'t> for FoldOn<'_, '_, 't, LLL> {
    fn node(&self) -> NodeView<'t> {
        NodeView::of(self.node)
    }

    fn arg_named(&mut self, name: &str, state: &RenderState) -> Result<Flow, RenderError> {
        self.cx
            .recompose_argument_content_named(self.node, name, state, &mut *self.renderer)
            .map_err(RenderError::region)
    }

    fn arg_at(&mut self, index: usize, state: &RenderState) -> Result<Flow, RenderError> {
        self.cx
            .recompose_argument_content(self.node, index, state, &mut *self.renderer)
            .map_err(RenderError::region)
    }

    fn body(&mut self, state: &RenderState) -> Result<Flow, RenderError> {
        self.cx
            .recompose_body(self.node, state, &mut *self.renderer)
            .map_err(RenderError::region)
    }

    fn slot_named(&mut self, name: &str, state: &RenderState) -> Result<Option<Flow>, RenderError> {
        match self
            .cx
            .recompose_slot_content_named(self.node, name, state, &mut *self.renderer)
        {
            Ok(flow) => Ok(Some(flow)),
            Err(RecomposeError::UnknownSlotName { .. }) => Ok(None),
            Err(error) => Err(RenderError::region(error)),
        }
    }

    fn argument_provided(&self, name: &str) -> Result<bool, RenderError> {
        let arguments = self.arguments()?;
        let argument = arguments.get_named(name).ok_or_else(|| {
            RenderError::region_detail(alloc::format!("no argument named ‘{name}’ is declared"))
        })?;
        Ok(argument.is_provided())
    }

    fn argument_provided_at(&self, index: usize) -> Result<bool, RenderError> {
        let arguments = self.arguments()?;
        let argument = arguments.get(index).ok_or_else(|| {
            RenderError::region_detail(alloc::format!(
                "argument index {index} is out of range ({} declared)",
                arguments.len()
            ))
        })?;
        Ok(argument.is_provided())
    }

    fn argument_count(&self) -> usize {
        self.node.arguments().map_or(0, ParsedArguments::len)
    }

    fn run(&self) -> &RunState {
        self.renderer.run()
    }

    fn run_mut(&mut self) -> &mut RunState {
        self.renderer.run_mut()
    }

    fn options(&self) -> &Options {
        self.renderer.options()
    }
}

/// A rule's view of the conversion in progress (PLAN.md §10.4).
///
/// This is the whole interface between a [`TextHandler`](crate::def::TextHandler) and
/// the fold. A handler never walks the tree itself: it asks the context for an
/// argument, a body, or an attached slot, and the context folds that region *through
/// the renderer*, so whatever is nested inside gets the same treatment it would get
/// anywhere else. That is also why a handler must ask for its arguments in declaration
/// order — the fold's side effects (footnote numbers, heading counters) happen in the
/// order the regions are folded.
///
/// The tree's language is not a parameter here: it was erased when the context was
/// built, which is what lets one handler render trees of every
/// [`RenderLang`](super::RenderLang) (PLAN.md §11.1). `'t` is the tree's own lifetime —
/// the one a [`NodeView`] read out of the context keeps.
///
/// ```
/// use techxt::def::{TextHandler, TextRule};
/// use techxt::flow::Flow;
/// use techxt::render::{NodeView, RenderCx, RenderError};
///
/// /// Renders `\emph{…}` as `*…*`.
/// #[derive(Debug)]
/// struct Stars;
///
/// impl TextHandler for Stars {
///     fn render(
///         &self,
///         _node: NodeView<'_>,
///         cx: &mut RenderCx<'_, '_>,
///     ) -> Result<Flow, RenderError> {
///         let mut flow = Flow::text("*");
///         flow.extend(cx.arg("text")?.unwrap_or_default());
///         flow.extend(Flow::text("*"));
///         Ok(flow)
///     }
/// }
/// ```
pub struct RenderCx<'a, 't> {
    fold: &'a mut (dyn Fold<'t> + 'a),
    state: &'a RenderState,
}

impl<'a, 't> RenderCx<'a, 't> {
    /// Build a context around the fold as it stands at the node being rendered.
    pub(crate) fn new(
        fold: &'a mut (dyn Fold<'t> + 'a),
        state: &'a RenderState,
    ) -> RenderCx<'a, 't> {
        RenderCx { fold, state }
    }

    /// The node being rendered.
    pub(crate) fn node(&self) -> NodeView<'t> {
        self.fold.node()
    }

    // ---------------------------------------------------------------- arguments

    /// Render the content of the argument called `name`, under the current state.
    ///
    /// Answers `Ok(None)` when the argument is *declared but absent* — an optional
    /// argument that was not written — and `Err` only when the definition declares no
    /// argument by that name at all, which is a bug in techxt's own tables rather than
    /// anything the document did.
    ///
    /// Only the argument's *content* is rendered: not the braces around it, not the
    /// whitespace before it.
    pub fn arg(&mut self, name: &str) -> Result<Option<Flow>, RenderError> {
        let state = self.state.clone();
        self.arg_with_state(name, state)
    }

    /// Like [`arg`](Self::arg), but rendering the argument under a state of the
    /// handler's choosing — how `\text{…}` leaves math, or how a list environment
    /// tells its body how deeply it is nested.
    ///
    /// # An argument that enters math is a whole formula
    ///
    /// When `state` enters math and the construct itself is not in math — `\ensuremath`
    /// is the shipped case — the argument is a complete **math scope**, so its atoms are
    /// joined and converted to text before the flow is handed back (PLAN.md §9.5). A
    /// handler therefore never has to know that the math pipeline exists in order to
    /// render something as a formula, and a
    /// [`MathAtom`](crate::flow::FlowItem::MathAtom) cannot leak out of one. Rendering an
    /// argument in the math state the construct is *already* in — what `\frac` and
    /// `\sqrt` do with their operands — leaves the atoms alone, which is what lets those
    /// constructs read their operands' classes.
    pub fn arg_with_state(
        &mut self,
        name: &str,
        state: RenderState,
    ) -> Result<Option<Flow>, RenderError> {
        if !self.fold.argument_provided(name)? {
            return Ok(None);
        }
        let flow = self.fold.arg_named(name, &state)?;
        Ok(Some(self.close_math_scope(flow, &state)))
    }

    /// Close a math scope that a derived state opened, if it opened one.
    fn close_math_scope(&self, flow: Flow, derived: &RenderState) -> Flow {
        match (self.state.math, derived.math) {
            (None, Some(entered)) => math::finish(flow, entered.display, self.options()),
            _ => flow,
        }
    }

    /// Render the content of the argument at `index` in declaration order, counting
    /// from zero.
    ///
    /// Answers `Ok(None)` for a declared-but-absent argument, `Err` for an index the
    /// definition does not have.
    pub fn arg_at(&mut self, index: usize) -> Result<Option<Flow>, RenderError> {
        if !self.fold.argument_provided_at(index)? {
            return Ok(None);
        }
        let state = self.state.clone();
        Ok(Some(self.fold.arg_at(index, &state)?))
    }

    /// Whether the argument called `name` was actually written.
    ///
    /// This is also the star test: a `*` argument's providedness *is* whether the star
    /// is there. An unknown name answers `false` rather than failing, so a rule can ask
    /// about an argument it is not sure the definition has.
    pub fn arg_provided(&self, name: &str) -> bool {
        self.fold.arg_provided(name)
    }

    /// The argument called `name`, rendered and flattened to a single line.
    ///
    /// This is [`arg`](Self::arg) followed by
    /// [`render_inline`](crate::layout::render_inline) — what a rule wants when it
    /// needs the argument as a *string* rather than as flowing text: a URL, a table
    /// column specification, a list item's label.
    pub fn arg_text(&mut self, name: &str) -> Result<Option<String>, RenderError> {
        Ok(self.arg(name)?.as_ref().map(render_inline))
    }

    // --------------------------------------------------------------------- body

    /// Render the environment body of the node, under the current state.
    pub fn body(&mut self) -> Result<Flow, RenderError> {
        let state = self.state.clone();
        self.body_with_state(state)
    }

    /// Render the environment body of the node under a state of the handler's choosing.
    pub fn body_with_state(&mut self, state: RenderState) -> Result<Flow, RenderError> {
        self.fold.body(&state)
    }

    /// Render the content attached to the node by source resolution, if any.
    ///
    /// This is `\input`'s resolved file, which the parser attaches as a slot that the
    /// ordinary child fold deliberately skips — so the handler that wants it has to ask
    /// for it, and nothing else can pull it in by accident.
    ///
    /// Answers `Ok(None)` when nothing was attached, which is what "not resolved" looks
    /// like: with no resolver configured, or with one that failed, the invocation is
    /// staged with no attached slot at all. A handler that gets `None` should raise
    /// [`InputNotResolved`](crate::diag::InputNotResolved), which is the one thing this
    /// method cannot do for it — only the handler knows what was being included.
    pub fn attached(&mut self) -> Result<Option<Flow>, RenderError> {
        let state = self.state.clone();
        self.fold.slot_named("attached", &state)
    }

    // -------------------------------------------------------------- environment

    /// The downward state this construct is being rendered in.
    pub fn state(&self) -> &RenderState {
        self.state
    }

    /// The conversion's options.
    pub fn options(&self) -> &Options {
        self.fold.options()
    }

    /// The LaTeX source `node` was parsed from.
    ///
    /// Reassembled from node payloads, never read out of the source buffer, so it works
    /// on transformed trees too (PLAN.md §1.6). This is what the `KeepSource` policies
    /// and math's `Source` mode emit.
    pub fn source_of(&self, node: NodeView<'_>) -> Result<String, RenderError> {
        Ok(node.source())
    }

    // -------------------------------------------------------------- diagnostics

    /// Report a diagnostic.
    ///
    /// Build it through [`TechxtCondition::diagnose`](crate::diag::TechxtCondition) so
    /// that the condition keeps the severity PLAN.md §10.6 assigns it.
    pub fn diag(&mut self, diagnostic: Diagnostic<Option<String>>) {
        self.fold.run_mut().diagnostics.push(diagnostic);
    }

    /// Report a condition, positioned at the node being rendered.
    pub(crate) fn report(&mut self, condition: impl TechxtCondition) {
        let span = self.node().span().clone();
        self.diag(condition.diagnose(span));
    }

    // ---------------------------------------------------------- document metadata

    /// Record the document's title (`\title{…}`).
    pub fn set_doc_title(&mut self, title: Flow) {
        self.fold.run_mut().doc_title = Some(title);
    }

    /// Record the document's author (`\author{…}`).
    pub fn set_doc_author(&mut self, author: Flow) {
        self.fold.run_mut().doc_author = Some(author);
    }

    /// Record the document's date (`\date{…}`).
    pub fn set_doc_date(&mut self, date: Flow) {
        self.fold.run_mut().doc_date = Some(date);
    }

    /// The document's title, if one has been recorded so far.
    pub fn doc_title(&self) -> Option<&Flow> {
        self.fold.run().doc_title.as_ref()
    }

    /// The document's author, if one has been recorded so far.
    pub fn doc_author(&self) -> Option<&Flow> {
        self.fold.run().doc_author.as_ref()
    }

    /// The document's date, if one has been recorded so far.
    pub fn doc_date(&self) -> Option<&Flow> {
        self.fold.run().doc_date.as_ref()
    }

    /// Register a footnote's text and get its number, counting from 1.
    ///
    /// Where the text ends up is the [footnote
    /// style](crate::convert::FootnoteStyle)'s business; the number is the same either
    /// way, and it is assigned in document order because the fold visits nodes in
    /// document order.
    pub fn push_footnote(&mut self, text: Flow) -> usize {
        let footnotes = &mut self.fold.run_mut().footnotes;
        footnotes.push(text);
        footnotes.len()
    }

    // ---------------------------------------------------- crate-internal accessors

    /// Whether the argument at `index` was written, reading an index the definition does
    /// not have as absent.
    pub(crate) fn arg_provided_at(&self, index: usize) -> bool {
        self.fold.arg_provided_at(index)
    }

    /// How many arguments the node's definition declares.
    pub(crate) fn argument_count(&self) -> usize {
        self.fold.argument_count()
    }

    /// The section-level counters, indexed by heading level.
    pub(crate) fn heading_counters_mut(&mut self) -> &mut [u32; 7] {
        &mut self.fold.run_mut().heading_counters
    }

    /// Whether a `\chapter` has been seen, which changes what `\section` numbers mean.
    pub(crate) fn chapter_seen_mut(&mut self) -> &mut bool {
        &mut self.fold.run_mut().chapter_seen
    }

    /// The enumerate counters, one per open list environment.
    ///
    /// A list environment pushes one for the extent of its body and pops it afterwards,
    /// so the top of the stack is always the innermost list's counter and `\item` can
    /// count without knowing how deeply it is nested.
    pub(crate) fn list_counter_stack_mut(&mut self) -> &mut Vec<u32> {
        &mut self.fold.run_mut().list_counter_stack
    }

    /// The kinds of the open list environments, innermost last.
    ///
    /// Pushed and popped by the list environment handlers exactly as the counter stack
    /// is, and read to derive
    /// [`ListCtx::same_kind_depth`](crate::render::ListCtx::same_kind_depth), which
    /// counts *all* the enclosing lists of a kind and not only an unbroken run of them.
    pub(crate) fn list_kind_stack_mut(&mut self) -> &mut Vec<ListKind> {
        &mut self.fold.run_mut().list_kind_stack
    }

    /// How many of the open list environments are of this kind.
    pub(crate) fn enclosing_lists_of_kind(&self, kind: ListKind) -> usize {
        self.fold
            .run()
            .list_kind_stack
            .iter()
            .filter(|open| **open == kind)
            .count()
    }
}

impl core::fmt::Debug for RenderCx<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RenderCx")
            .field("node", &self.node().id())
            .field("state", self.state)
            .finish_non_exhaustive()
    }
}
