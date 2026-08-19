//! natbib's citation commands (PLAN.md §12.1).
//!
//! Pushed **last** of all the categories, so that its fuller citation shapes shadow the
//! plain `\cite` family of [`defs::refs`](super::refs).

use crate::def::Category;

/// The natbib category (PLAN.md §12.1).
///
/// **TODO(M8): a stub — this category is empty.** It needs `\citet`, `\citep`,
/// `\citealt`, `\citealp`, `\citeauthor`, `\citeyear` and their starred and capitalized
/// forms, each with natbib's two optional arguments.
pub fn category() -> Category {
    Category::new("natbib")
}
