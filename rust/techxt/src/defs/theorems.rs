//! Theorem-like environments, and the other blocks that shape a body (PLAN.md §9.8).
//!
//! `theorem`, `proposition`, `lemma`, `corollary`, `definition`, `conjecture`,
//! `remark`, `example` and `proof`, with their short aliases, plus the block
//! environments they sit among: `center`, `flushleft`, `flushright`, `quote`,
//! `quotation`, `verse`, `abstract`, `figure`, `table` and `\caption`.

use crate::def::Category;

/// The theorems category (PLAN.md §12.1).
///
/// **TODO(M6): a stub — this category is empty.** It needs the environments with their
/// `o`-shaped note argument, the fixed alias table that spells a label out in full, and
/// the [`FloatKind`](crate::render::FloatKind) state that tells `\caption` whether to
/// write `Figure` or `Table`.
pub fn category() -> Category {
    Category::new("theorems")
}
