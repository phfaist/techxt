//! The handler-facing context (PLAN.md §10.4) and the error a rule can fail with.

use alloc::string::String;
use alloc::vec::Vec;
use core::convert::Infallible;

use techy::core::node::{NodeRef, ParsedArgument};
use techy::error::{Diagnostic, Diagnostics};
use techy::latexlike::Latexlike;
use techy::recompose::{RecomposeContext, RecomposeError, Recomposer};
use techy::source::SourceSpan;

use crate::convert::Options;
use crate::diag::TechxtCondition;
use crate::flow::Flow;
use crate::layout::render_inline;

use super::state::RenderState;

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
    pub(crate) footnotes: Vec<Flow>,
    pub(crate) doc_title: Option<Flow>,
    pub(crate) doc_author: Option<Flow>,
    pub(crate) doc_date: Option<Flow>,
    /// A span covering the whole document, for diagnostics with no node to point at.
    pub(crate) document_span: Option<SourceSpan<Option<String>>>,
}

/// The renderer as the context sees it.
///
/// The context needs two things from the renderer at once: to be the recomposer that
/// techy's region operations fold *through* (so nested constructs are rendered by the
/// same rules), and to be the owner of the run's counters and diagnostics. Erasing it
/// behind this trait is what lets [`RenderCx`] borrow it once and keep both, without
/// the renderer's own lifetime parameter leaking into every handler signature.
pub(crate) trait RendererOps:
    Recomposer<Latexlike, (), State = RenderState, Piece = Flow, Error = Infallible>
{
    fn run(&self) -> &RunState;
    fn run_mut(&mut self) -> &mut RunState;
    fn options(&self) -> &Options;
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
/// ```
/// use techxt::def::{TextHandler, TextRule};
/// use techxt::flow::Flow;
/// use techxt::render::{RenderCx, RenderError};
/// use techy::core::node::NodeRef;
/// use techy::latexlike::Latexlike;
///
/// /// Renders `\emph{…}` as `*…*`.
/// #[derive(Debug)]
/// struct Stars;
///
/// impl TextHandler for Stars {
///     fn render(
///         &self,
///         _node: NodeRef<'_, Latexlike>,
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
    renderer: &'a mut (dyn RendererOps + 'a),
    cx: &'a mut RecomposeContext<'t, Latexlike, ()>,
    node: NodeRef<'a, Latexlike, ()>,
    state: &'a RenderState,
}

impl<'a, 't> RenderCx<'a, 't> {
    /// Build a context around the node being rendered.
    pub(crate) fn new(
        renderer: &'a mut (dyn RendererOps + 'a),
        cx: &'a mut RecomposeContext<'t, Latexlike, ()>,
        node: NodeRef<'a, Latexlike, ()>,
        state: &'a RenderState,
    ) -> RenderCx<'a, 't> {
        RenderCx {
            renderer,
            cx,
            node,
            state,
        }
    }

    /// The node being rendered.
    pub(crate) fn node(&self) -> NodeRef<'a, Latexlike, ()> {
        self.node
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
    pub fn arg_with_state(
        &mut self,
        name: &str,
        state: RenderState,
    ) -> Result<Option<Flow>, RenderError> {
        if !self.argument(name)?.is_provided() {
            return Ok(None);
        }
        let flow = self
            .cx
            .recompose_argument_content_named(self.node, name, &state, self.renderer)
            .map_err(RenderError::region)?;
        Ok(Some(flow))
    }

    /// Render the content of the argument at `index` in declaration order, counting
    /// from zero.
    ///
    /// Answers `Ok(None)` for a declared-but-absent argument, `Err` for an index the
    /// definition does not have.
    pub fn arg_at(&mut self, index: usize) -> Result<Option<Flow>, RenderError> {
        if !self.argument_at(index)?.is_provided() {
            return Ok(None);
        }
        let state = self.state.clone();
        let flow = self
            .cx
            .recompose_argument_content(self.node, index, &state, self.renderer)
            .map_err(RenderError::region)?;
        Ok(Some(flow))
    }

    /// Whether the argument called `name` was actually written.
    ///
    /// This is also the star test: a `*` argument's providedness *is* whether the star
    /// is there. An unknown name answers `false` rather than failing, so a rule can ask
    /// about an argument it is not sure the definition has.
    pub fn arg_provided(&self, name: &str) -> bool {
        self.node
            .arguments()
            .and_then(|arguments| arguments.get_named(name))
            .is_some_and(ParsedArgument::is_provided)
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
        self.cx
            .recompose_body(self.node, &state, self.renderer)
            .map_err(RenderError::region)
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
        match self
            .cx
            .recompose_slot_content_named(self.node, "attached", &state, self.renderer)
        {
            Ok(flow) => Ok(Some(flow)),
            Err(RecomposeError::UnknownSlotName { .. }) => Ok(None),
            Err(error) => Err(RenderError::region(error)),
        }
    }

    // -------------------------------------------------------------- environment

    /// The downward state this construct is being rendered in.
    pub fn state(&self) -> &RenderState {
        self.state
    }

    /// The conversion's options.
    pub fn options(&self) -> &Options {
        self.renderer.options()
    }

    /// The LaTeX source `node` was parsed from.
    ///
    /// Reassembled from node payloads, never read out of the source buffer, so it works
    /// on transformed trees too (PLAN.md §1.6). This is what the `KeepSource` policies
    /// and math's `Source` mode emit.
    pub fn source_of(&self, node: NodeRef<'_, Latexlike>) -> Result<String, RenderError> {
        Ok(super::source::latex_source(node))
    }

    // -------------------------------------------------------------- diagnostics

    /// Report a diagnostic.
    ///
    /// Build it through [`TechxtCondition::diagnose`](crate::diag::TechxtCondition) so
    /// that the condition keeps the severity PLAN.md §10.6 assigns it.
    pub fn diag(&mut self, diagnostic: Diagnostic<Option<String>>) {
        self.renderer.run_mut().diagnostics.push(diagnostic);
    }

    /// Report a condition, positioned at the node being rendered.
    pub(crate) fn report(&mut self, condition: impl TechxtCondition) {
        let span = self.node.span().clone();
        self.diag(condition.diagnose(span));
    }

    // ---------------------------------------------------------- document metadata

    /// Record the document's title (`\title{…}`).
    pub fn set_doc_title(&mut self, title: Flow) {
        self.renderer.run_mut().doc_title = Some(title);
    }

    /// Record the document's author (`\author{…}`).
    pub fn set_doc_author(&mut self, author: Flow) {
        self.renderer.run_mut().doc_author = Some(author);
    }

    /// Record the document's date (`\date{…}`).
    pub fn set_doc_date(&mut self, date: Flow) {
        self.renderer.run_mut().doc_date = Some(date);
    }

    /// The document's title, if one has been recorded so far.
    pub fn doc_title(&self) -> Option<&Flow> {
        self.renderer.run().doc_title.as_ref()
    }

    /// The document's author, if one has been recorded so far.
    pub fn doc_author(&self) -> Option<&Flow> {
        self.renderer.run().doc_author.as_ref()
    }

    /// The document's date, if one has been recorded so far.
    pub fn doc_date(&self) -> Option<&Flow> {
        self.renderer.run().doc_date.as_ref()
    }

    /// Register a footnote's text and get its number, counting from 1.
    ///
    /// Where the text ends up is the [footnote
    /// style](crate::convert::FootnoteStyle)'s business; the number is the same either
    /// way, and it is assigned in document order because the fold visits nodes in
    /// document order.
    pub fn push_footnote(&mut self, text: Flow) -> usize {
        let footnotes = &mut self.renderer.run_mut().footnotes;
        footnotes.push(text);
        footnotes.len()
    }

    // ---------------------------------------------------- crate-internal accessors

    /// The section-level counters, indexed by heading level.
    pub(crate) fn heading_counters_mut(&mut self) -> &mut [u32; 7] {
        &mut self.renderer.run_mut().heading_counters
    }

    /// Whether a `\chapter` has been seen, which changes what `\section` numbers mean.
    pub(crate) fn chapter_seen_mut(&mut self) -> &mut bool {
        &mut self.renderer.run_mut().chapter_seen
    }

    /// The enumerate counters, one per open list environment.
    ///
    /// A list environment pushes one for the extent of its body and pops it afterwards,
    /// so the top of the stack is always the innermost list's counter and `\item` can
    /// count without knowing how deeply it is nested.
    pub(crate) fn list_counter_stack_mut(&mut self) -> &mut Vec<u32> {
        &mut self.renderer.run_mut().list_counter_stack
    }

    /// The argument called `name`, or a region error naming what went wrong.
    fn argument(&self, name: &str) -> Result<&'a ParsedArgument<Latexlike>, RenderError> {
        let arguments = self.node.arguments().ok_or_else(|| {
            RenderError::region_detail("asked for an argument of a node that is not a callable")
        })?;
        arguments.get_named(name).ok_or_else(|| {
            RenderError::region_detail(alloc::format!("no argument named ‘{name}’ is declared"))
        })
    }

    /// The argument at `index`, or a region error naming what went wrong.
    fn argument_at(&self, index: usize) -> Result<&'a ParsedArgument<Latexlike>, RenderError> {
        let arguments = self.node.arguments().ok_or_else(|| {
            RenderError::region_detail("asked for an argument of a node that is not a callable")
        })?;
        arguments.get(index).ok_or_else(|| {
            RenderError::region_detail(alloc::format!(
                "argument index {index} is out of range ({} declared)",
                arguments.len()
            ))
        })
    }
}

impl core::fmt::Debug for RenderCx<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RenderCx")
            .field("node", &self.node.id())
            .field("state", self.state)
            .finish_non_exhaustive()
    }
}
