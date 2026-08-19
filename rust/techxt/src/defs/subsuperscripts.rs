//! Sub- and superscripts (PLAN.md §9.5): `^` and `_`.
//!
//! Both are specials taking one expression argument, registered **math mode only**, so
//! that `a_b` in running text stays the characters it was written as.

use crate::def::Category;

/// The subsuperscripts category (PLAN.md §12.1).
///
/// **TODO(M5b): a stub — this category is empty.** It needs the two specials plus the
/// rendering ladder of PLAN.md §9.5 — unicode script characters, then a retry with the
/// joiner's spaces stripped, then the `^(…)` fallback — built on
/// [`script_atom`](crate::mathfmt::script_atom).
pub fn category() -> Category {
    Category::new("subsuperscripts")
}
