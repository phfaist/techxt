//! Math formatting: the engine that turns rendered math fragments into text.
//!
//! Rendering a formula to unicode text is not concatenation. `x^2+y` must come out
//! as `𝑥²+𝑦` in one place and as `𝑥² + 𝑦` in another, `\frac{a+b}{c}` needs
//! parentheses around its numerator while `\frac{a}{c}` does not, and a matrix
//! occupies several lines at once. What decides all of that is not the individual
//! symbol but *how neighbours meet*: a binary operator wants space around it unless
//! it is really a unary sign, a subscript must sit flush against its base, and a
//! sub-expression whose extent would otherwise be unclear has to be bracketed.
//!
//! So the math handlers do not build strings; they build **atoms**. An [`Atom`] is a
//! piece of rendered math plus a description of what it presents to the piece on its
//! left and to the piece on its right (its [`AtomClass`] pair). A later pass — the
//! *joiner*, [`join_atoms`] — walks the sequence, lets every atom *realize* itself
//! against its actual neighbours, and inserts spacing according to a fixed set of
//! rules. Atoms therefore travel through the flow as
//! [`FlowItem::MathAtom`](crate::flow::FlowItem::MathAtom) and never reach the layout
//! engine: the math-group handler consumes them and emits ordinary text, glue and
//! verbatim lines instead (PLAN.md §9.5, "Flow boundary").
//!
//! # The pipeline
//!
//! ```text
//!   rendered fragments ──► segment_plain ──► [Atom] ──► join_atoms ──► MathBox
//!   (chars, symbols,      (splits plain      (+ atoms   (spacing,      (lines +
//!    rule output)          strings into       handlers   wrapping,      baseline)
//!                          their atoms)       built)     baselines)
//! ```
//!
//! - [`segment_plain`] splits an ordinary string — a characters node, a symbol, a
//!   rule's replacement text — into the atoms it is made of, so that the joiner sees
//!   the `+` in the middle of `x+y`.
//! - Handlers that know more build atoms directly: [`script_atom`] for `^`/`_`,
//!   [`frac_atom`], [`sqrt_atom`], [`inline_matrix_atom`] and [`display_matrix_atom`].
//! - [`join_atoms`] assembles them, and a [`MathBox`] comes out: one line for inline
//!   math, possibly several for a display matrix.
//! - [`fmt_style`] applies the unicode math alphabets — **to document characters only**
//!   (DECISIONS.md D6), never to a rule's replacement string.
//!
//! # Self-contained, and configured by parameter
//!
//! This module knows nothing of techy trees, of the definitions database or of the
//! render state, so the settings it needs arrive as parameters rather than by reading
//! [`Options`](crate::Options). The four types those settings have are declared here,
//! next to the code that acts on them, and
//! [`convert`](crate::convert) re-exports every one of them — there is one
//! [`MathWrapDelims`], one [`MatrixDelims`], one [`FontStyle`] and one
//! [`FontStyleKind`], reachable by either path:
//!
//! - [`Options::math_expression_in`](crate::Options::math_expression_in) →
//!   [`MathWrapDelims`], a parameter of [`join_atoms_with`], [`frac_atom`] and
//!   [`sqrt_atom`];
//! - [`Options::matrix_delimiters`](crate::Options::matrix_delimiters) →
//!   [`MatrixDelims`], a parameter of the display matrix builders;
//! - [`Options::text_font`](crate::Options::text_font) and
//!   [`Options::math_font`](crate::Options::math_font) → [`FontStyle`], which selects
//!   one of the alphabet tables [`fmt_style`] applies.

mod alphabets;
mod atom;
mod boxes;
mod builders;
mod join;
mod scripts;
mod segment;

pub use alphabets::{fmt_style, fmt_style_char, unstyle, unstyle_char, FontStyle, FontStyleKind};
pub use atom::{Atom, AtomBody, AtomClass, ScriptInfo, ScriptKind};
pub use boxes::{
    baseline_join, display_matrix_atom, display_matrix_box, inline_matrix_atom, MathBox,
    MatrixDelims, MatrixFence,
};
pub use builders::{frac_atom, sqrt_atom};
pub use join::{join_atoms, join_atoms_with, JoinedMath, MathWrapDelims};
pub use scripts::{fmt_script_text, script_atom};
pub use segment::{classify_plain, segment_plain};
