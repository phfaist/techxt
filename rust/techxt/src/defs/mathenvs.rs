//! Display-math environments and matrices (PLAN.md §9.5).
//!
//! `equation`, `align`, `gather`, `multline`, `eqnarray`, `split` and `dmath` (with
//! their starred forms), and the matrix family — `matrix`, `pmatrix`, `bmatrix`,
//! `Bmatrix`, `vmatrix`, `Vmatrix`, `smallmatrix` and `array`.
//!
//! A matrix body is folded with `MathCtx::matrix` set, which is what turns the `&` and
//! `\\` defined in [`defs::base`](super::base) into cell and row separators.

use crate::def::Category;

/// The mathenvs category (PLAN.md §12.1).
///
/// **TODO(M5b): a stub — this category is empty.** It needs environment definitions
/// carrying [`math_body`](crate::def::EnvDef::math_body) and handlers that fold the
/// body with the math state set and then run the atom pipeline —
/// [`inline_matrix_atom`](crate::mathfmt::inline_matrix_atom) and
/// [`display_matrix_atom`](crate::mathfmt::display_matrix_atom) for the matrices.
pub fn category() -> Category {
    Category::new("mathenvs")
}
