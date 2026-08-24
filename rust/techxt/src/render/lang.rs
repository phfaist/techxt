//! [`RenderLang`]: the one name for what techxt's render side needs of a language.

use alloc::string::String;

use techy::core::node::BodySlotExt;
use techy::core::NodeExtTypes;
use techy::latexlike::LatexlikeLang;

/// A latexlike language techxt can render a tree of (PLAN.md §11.1).
///
/// **Auto-implemented; never implement it by hand.** The blanket implementation below
/// covers every language that qualifies, so a language earns this by satisfying
/// [`LatexlikeLang`] and the two facts named here — nothing is registered, and nothing
/// can be opted out of.
///
/// The trait exists to give those two facts one public name. They are precisely what
/// techxt's render side is *concrete* about, and neither is negotiable:
///
/// - **origin-tagged sources spelled `Option<String>`** — the source-origin type every
///   [`Diagnostics`](techy::error::Diagnostics) and
///   [`SourceSpan`](techy::source::SourceSpan) in techxt carries, from a
///   [`Conversion`](crate::Conversion)'s diagnostics down to what a
///   [`NodeView`](super::NodeView) answers with;
/// - **body-marked slots** — techy's [`BodySlotExt`] on the language's slot ext, which
///   is what lets [`RenderCx::body`](super::RenderCx::body) fold an environment's body
///   at all.
///
/// Everything else about the language is read through payload-only accessors and the
/// latexlike role traits, so nothing here mentions techy-xp: techy's own
/// [`Latexlike`](techy::latexlike::Latexlike) and techy-xp's
/// [`LatexlikeXp`](techy_xp::lang::LatexlikeXp) both satisfy it, and a tree of either —
/// or of a latexlike language of your own — renders through the same rules.
pub trait RenderLang:
    LatexlikeLang<SourceOrigin = Option<String>, NodeExts: NodeExtTypes<SlotExt: BodySlotExt>>
{
}

impl<L> RenderLang for L where
    L: LatexlikeLang<SourceOrigin = Option<String>, NodeExts: NodeExtTypes<SlotExt: BodySlotExt>>
{
}
