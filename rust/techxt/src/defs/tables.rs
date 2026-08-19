//! Tabular material (PLAN.md §9.6): `tabular`, `tabular*`, `tabularx` and the
//! constructs that shape their cells.
//!
//! A table handler sets [`RenderState::table`](crate::render::RenderState::table) for
//! the body fold, which is what turns the `&`, `\\` and `\hline` defined in
//! [`defs::base`](super::base) into cell separators, row separators and rules; it then
//! splits the folded flow at those markers to lay the columns out.

use crate::def::Category;

/// The tables category (PLAN.md §12.1).
///
/// **TODO(M6): a stub — this category is empty.** It needs the three environments, the
/// column-specification reader of PLAN.md §9.6, and `\multicolumn`, `\cline` and the
/// `booktabs` rules.
pub fn category() -> Category {
    Category::new("tables")
}
