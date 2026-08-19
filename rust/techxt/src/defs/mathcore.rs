//! The core of mathematical notation (PLAN.md §9.5): Greek letters, operators and
//! relations, `\frac`, `\sqrt`, brackets, arrows, dots and delimiters.
//!
//! Everything here builds [`mathfmt`](crate::mathfmt) atoms rather than plain text,
//! because how a formula reads depends on how its neighbours meet: `\frac{a}{b}c` must
//! come out as `𝑎/𝑏 𝑐` and not as `(𝑎/𝑏)𝑐`. The atom model and the joiner already
//! exist; what is missing is the table of definitions that feeds them.

use crate::def::Category;

/// The mathcore category (PLAN.md §12.1).
///
/// **TODO(M5b): a stub — this category is empty.** It needs the symbol tables (Greek,
/// operators, relations, arrows, delimiters) as `Literal` rules segmented through
/// [`segment_plain`](crate::mathfmt::segment_plain), plus handlers for `\frac`, `\sqrt`
/// and the delimiter macros built on [`frac_atom`](crate::mathfmt::frac_atom) and
/// [`sqrt_atom`](crate::mathfmt::sqrt_atom), and the operator-name macros (`\sin`,
/// `\lim`, …) that [`defs::fontstyles`](super::fontstyles)'s `\operatorname` is the
/// generic form of.
pub fn category() -> Category {
    Category::new("mathcore")
}
