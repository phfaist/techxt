//! What a definition says a construct renders to.

use alloc::borrow::Cow;
use alloc::sync::Arc;

use techy::core::node::NodeRef;
use techy::latexlike::CallableType;
use techy_xp::lang::LatexlikeXp;

use crate::flow::Flow;
use crate::render::{RenderCx, RenderError};

use super::Template;

/// Which of the three kinds of callable a definition or a rule is about.
///
/// techy keys macros and environments by name and specials by their trigger
/// characters, in three separate stores; a name therefore means nothing on its own, and
/// every techxt lookup — the override map, the name fallback table — is keyed by the
/// pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    /// `\emph`, `\textbf`: a macro, named without its escape character.
    Macro,
    /// `\begin{itemize}…\end{itemize}`: an environment, named as in `\begin{…}`.
    Environment,
    /// `~`, `&`, `---`: a construct triggered by the characters themselves.
    Specials,
}

impl CallableKind {
    /// The techxt kind matching one of techy's callable types.
    ///
    /// Answers `None` for a callable type techxt does not know — techy's
    /// [`CallableType`] is `#[non_exhaustive]`, so a future language could grow a
    /// fourth kind, and a tree carrying one is a tree techxt has no rules for.
    pub fn from_callable_type(callable_type: CallableType) -> Option<CallableKind> {
        match callable_type {
            CallableType::Macro => Some(CallableKind::Macro),
            CallableType::Environment => Some(CallableKind::Environment),
            CallableType::Specials => Some(CallableKind::Specials),
            _ => None,
        }
    }

    /// The techxt kind of the callable `node` is, or `None` when it is not a callable.
    pub fn of(node: NodeRef<'_, LatexlikeXp>) -> Option<CallableKind> {
        node.callable_type()
            .and_then(CallableKind::from_callable_type)
    }

    /// How a construct of this kind is written, for diagnostic messages: `\name` for a
    /// macro, the bare name for an environment or specials.
    pub(crate) fn write_construct(self, name: &str) -> alloc::string::String {
        match self {
            CallableKind::Macro => alloc::format!("\\{name}"),
            CallableKind::Environment | CallableKind::Specials => alloc::string::String::from(name),
        }
    }
}

/// What one construct renders to (PLAN.md §10.4).
///
/// Four of the five variants are data, so that the overwhelming majority of a
/// definitions database is a table rather than code: `\alpha` is a
/// [`Literal`](Self::Literal), `\href{url}{text}` is a [`Template`](Self::Template),
/// `\label` is a [`Skip`](Self::Skip), `\emph` is [`Content`](Self::Content). Only what
/// genuinely needs to compute — a heading that has to number itself, a matrix that has
/// to measure its columns — reaches for a [`Handler`](Self::Handler).
///
/// # Execution
///
/// - [`Literal`](Self::Literal) — the text, split into words and spaces exactly as
///   document text is.
/// - [`Template`](Self::Template) — the template with its argument references
///   substituted, then treated as literal text.
/// - [`Skip`](Self::Skip) — nothing at all. Note that this *prunes*: nothing inside the
///   construct is visited, so a `\footnote` hidden in a skipped macro's argument is
///   never collected.
/// - [`Content`](Self::Content) — for a macro, the content of every *provided*
///   argument, in declaration order, with nothing inserted between them; for an
///   environment, its body; for specials, the trigger characters as text.
/// - [`Handler`](Self::Handler) — the handler decides. If it returns an error, the
///   construct renders as nothing, a `techxt.handler-failed` diagnostic is raised, and
///   the conversion continues.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum TextRule {
    /// Fixed replacement text: `\alpha` → `α`, `\LaTeX` → `LaTeX`.
    Literal(Cow<'static, str>),
    /// Replacement text built from the construct's arguments (PLAN.md §10.5).
    Template(Template),
    /// Render nothing, and do not descend into the construct.
    Skip,
    /// Render the construct's own content: arguments, body, or trigger characters.
    Content,
    /// Render by running code.
    Handler(Arc<dyn TextHandler>),
}

/// Rendering code for one construct (PLAN.md §10.4).
///
/// A handler is called with the node it is rendering and a [`RenderCx`], which is its
/// whole interface to the fold: it reads arguments through the context (which folds
/// them with the renderer, so nested constructs keep working), reads the downward
/// [`RenderState`](crate::render::RenderState) and the [`Options`](crate::Options),
/// raises diagnostics, and registers footnotes and document metadata.
///
/// Handlers are shared across threads and across documents — the converter holding them
/// is `Clone + Send + Sync` and immutable — so a handler must be `Send + Sync` and must
/// hold no per-document state. Everything per-run belongs to the renderer, reachable
/// through the context.
///
/// Returning `Err` is for a genuine failure to render (a malformed argument the rule
/// cannot make sense of), not for "nothing to emit": it costs the construct its output
/// and raises an error diagnostic. Return an empty [`Flow`] instead when the correct
/// rendering is nothing.
pub trait TextHandler: Send + Sync + core::fmt::Debug {
    /// Render `node`, reading everything else through `cx`.
    fn render(
        &self,
        node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError>;
}
