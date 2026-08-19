//! List environments (PLAN.md §9.4): `itemize`, `enumerate`, `description`, `list` and
//! `trivlist`, and the `\item` that lives inside them.
//!
//! `\item` is brought into scope by each list environment's body rather than defined
//! globally, which is what [`EnvDef::list_body`](crate::def::EnvDef::list_body) is for:
//! an `\item` outside a list is a mistake worth reporting, not a construct.

use crate::def::Category;

/// The lists category (PLAN.md §12.1).
///
/// **TODO(M6): a stub — this category is empty.** It needs the five environments
/// declared with `list_body`, an `\item` handler emitting a
/// [`BlockKind::Item`](crate::flow::BlockKind::Item) whose marker comes from
/// [`Options::list_style`](crate::Options::list_style) and the nesting depths in
/// [`ListCtx`](crate::render::ListCtx), and the run's list counter stack (already
/// declared in the renderer's run state).
pub fn category() -> Category {
    Category::new("lists")
}
