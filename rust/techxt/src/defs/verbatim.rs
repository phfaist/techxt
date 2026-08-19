//! Verbatim text (PLAN.md §9.8): the `verbatim` environment family and `\verb`.
//!
//! Verbatim content reaches the flow as [`Verbatim`](crate::flow::FlowItem::Verbatim)
//! and [`InlineVerbatim`](crate::flow::FlowItem::InlineVerbatim) items, which the
//! layout engine emits byte for byte: never wrapped, never trimmed, never styled. The
//! renderer already recognizes both shapes; only the definitions are missing.

use crate::def::Category;

/// The verbatim category (PLAN.md §12.1).
///
/// **TODO(M6): a stub — this category is empty.** It needs `verbatim`, `verbatim*`,
/// `lstlisting` (with its optional options argument accepted and ignored) and `alltt`
/// declared with [`verbatim_body`](crate::def::EnvDef::verbatim_body), plus `\verb`
/// with a `v` argument.
pub fn category() -> Category {
    Category::new("verbatim")
}
