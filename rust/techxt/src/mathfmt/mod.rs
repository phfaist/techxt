//! Math formatting: the atom model the math engine composes formulas from.
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
//! *joiner* — walks the sequence, lets every atom *realize* itself against its actual
//! neighbours, and inserts spacing according to a fixed set of rules. Atoms therefore
//! travel through the flow as [`FlowItem::MathAtom`](crate::flow::FlowItem::MathAtom)
//! and never reach the layout engine: the math-group handler consumes them and emits
//! ordinary text, glue and verbatim lines instead (PLAN.md §9.5, "Flow boundary").
//!
//! # Status: type skeleton
//!
//! This module currently defines only the *data model*. The engine that produces and
//! consumes atoms — segmentation of plain strings into atoms, the joiner, the script
//! ladder, the font-alphabet tables — lands with the math milestone (PLAN.md §16 M5)
//! and adds `segment_plain` and `join_atoms` here. The types below are already what
//! [`crate::flow`] and [`crate::layout`] depend on, and [`Atom::flatten`] is what the
//! layout engine falls back to should an atom ever escape a math subtree.

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::flow::display_width;

/// One piece of rendered math, together with how it wants to meet its neighbours.
///
/// An atom is deliberately not a plain string: its final text can depend on what
/// surrounds it. A [wrappable](AtomBody::Wrappable) body only learns whether it needs
/// delimiters once its right neighbour is known, and a [block](AtomBody::Block) body
/// renders differently inline and in display math. The joiner is what turns a sequence
/// of atoms into final text.
#[derive(Clone, Debug)]
pub struct Atom {
    /// The rendered contents, in whichever of the three shapes fits this atom.
    pub body: AtomBody,
    /// What this atom presents to its left neighbour and to its right neighbour, in
    /// that order. The two are frequently different: `\frac{a}{b}` is ordinary on the
    /// left but [open-ended](AtomClass::OpenEnd) on the right, because nothing marks
    /// where the denominator stops.
    pub cls: (AtomClass, AtomClass),
    /// `Some` when this atom is a sub- or superscript that binds to the atom on its
    /// left; `None` for everything else. See [`ScriptInfo`].
    pub script: Option<ScriptInfo>,
}

impl Atom {
    /// Render this atom to plain text on a single line, ignoring its neighbours.
    ///
    /// This is the "no joiner available" rendering: a [wrappable](AtomBody::Wrappable)
    /// body shows its contents without delimiters (outside a join there is no right
    /// neighbour that could make the extent ambiguous), and a
    /// [block](AtomBody::Block) body has its rows joined with single spaces so that
    /// the result stays one line. The [layout engine](crate::layout) uses it as the
    /// fallback for a stray [`FlowItem::MathAtom`](crate::flow::FlowItem::MathAtom),
    /// and [`render_inline`](crate::layout::render_inline) uses it for atoms in a
    /// table cell.
    ///
    /// ```
    /// use techxt::mathfmt::{Atom, AtomBody, AtomClass};
    ///
    /// let atom = Atom {
    ///     body: AtomBody::Wrappable { inner: "a/b".into(), prefix: "".into() },
    ///     cls: (AtomClass::Ord, AtomClass::OpenEnd),
    ///     script: None,
    /// };
    /// assert_eq!(atom.flatten(), "a/b");
    /// ```
    pub fn flatten(&self) -> String {
        match &self.body {
            AtomBody::Str(text) => String::from(&**text),
            AtomBody::Wrappable { inner, prefix } => {
                let mut out = String::with_capacity(prefix.len() + inner.len());
                out.push_str(prefix);
                out.push_str(inner);
                out
            }
            AtomBody::Block(math_box) => math_box.join_lines(" "),
        }
    }
}

/// The three shapes an [`Atom`]'s contents can take.
#[derive(Clone, Debug)]
pub enum AtomBody {
    /// Finished text: a symbol, a variable, an operator name, an already-rendered
    /// sub-expression. What you see is what gets emitted.
    Str(Box<str>),
    /// Contents that decide at join time whether they need delimiters around them.
    ///
    /// A fraction, a script rendered in the `^(...)` fallback notation, or any other
    /// sub-expression whose extent is not self-evident. The joiner wraps `inner` in
    /// the configured math-expression delimiters when — and only when — a separating
    /// space would still leave the extent ambiguous: another script attaches to the
    /// same base, the contents contain a space, or the contents are themselves
    /// [open-ended](AtomClass::OpenEnd).
    Wrappable {
        /// The contents that may or may not end up enclosed in delimiters.
        inner: Box<str>,
        /// Text that always precedes the contents and always stays *outside* the
        /// delimiters, such as the `^` or `_` of a script in fallback notation.
        prefix: Box<str>,
    },
    /// A rectangular object that occupies several lines at once: a matrix or an array.
    ///
    /// The joiner treats a block as opaque and as self-delimiting on both edges — the
    /// `+` in the middle of a matrix cell has nothing to say about how the matrix as a
    /// whole meets its neighbours; its delimiters do.
    Block(MathBox),
}

/// What an [`Atom`] presents to a neighbour, which is what decides the spacing
/// between them.
///
/// These are TeX's atom classes plus three of techxt's own ([`OpenEnd`](Self::OpenEnd),
/// [`Text`](Self::Text), [`Block`](Self::Block)). An atom carries a *pair* of them,
/// one per edge, because the two edges can differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomClass {
    /// Ordinary: variables, digits, ordinary symbols.
    Ord,
    /// A named function or a large operator: `sin`, `∑`.
    Op,
    /// A binary operator: `+`, `×`. Reclassified as unary — and then bound tightly to
    /// what follows — when its left neighbour shows that nothing can precede it.
    Bin,
    /// A relation: `=`, `∈`, `→`.
    Rel,
    /// An opening delimiter.
    Open,
    /// A closing delimiter.
    Close,
    /// A punctuation mark: `,` or `;`.
    Punct,
    /// A rendering with no visible extent on that side, such as `x^q`, `a/b` or `√x`:
    /// whatever comes next would look as though it were part of it. This is what makes
    /// the joiner add a space, or a wrappable body reach for its delimiters.
    OpenEnd,
    /// A fragment of ordinary text (`\text{...}`), which the joiner treats as opaque.
    Text,
    /// A superscript or subscript, which binds to the atom on its left.
    Script,
    /// A large object such as a matrix, which brings its own delimiters: an opening
    /// delimiter on the left edge and a closing one on the right.
    Block,
}

/// Marks an [`Atom`] as a sub- or superscript and records how it had to be rendered.
///
/// The joiner needs two facts about a script that the rendered text alone does not
/// carry. [`kind`](Self::kind) lets it swap a superscript that is directly followed by
/// a subscript, so that the subscript binds to the base first (`x^2_i` reads as
/// `𝑥ᵢ²`). [`latex_notation`](Self::latex_notation) says that the script had to fall
/// back on LaTeX's own `^`/`_` spelling instead of unicode script characters, in which
/// case the joiner also puts a space *in front of the base* so that one can see where
/// the base begins.
///
/// The script's rendered text lives in the atom's [`body`](Atom::body) like any other;
/// this struct is only the metadata that goes with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptInfo {
    /// Whether this atom is a superscript or a subscript.
    pub kind: ScriptKind,
    /// Whether the script is spelled in LaTeX's `^`/`_` notation because no unicode
    /// script characters were available for its contents.
    pub latex_notation: bool,
}

/// Which of the two scripts an [`Atom`] is (see [`ScriptInfo`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptKind {
    /// A superscript, `x^2`.
    Superscript,
    /// A subscript, `x_i`.
    Subscript,
}

/// A rectangular block of already-rendered text lines with a marked baseline.
///
/// Display-math matrices become boxes: one line per matrix row, columns padded to a
/// common width. The [`baseline`](Self::baseline) is the index of the line that sits
/// on the surrounding formula's baseline, which is what lets several boxes of
/// different heights be joined side by side (pad to a common height, align baselines,
/// concatenate row-wise).
#[derive(Clone, Debug)]
pub struct MathBox {
    /// The rendered lines, top to bottom. All of them are meant to be displayed at the
    /// same width; the producer pads them.
    pub lines: Vec<Box<str>>,
    /// Index into [`lines`](Self::lines) of the line that carries the baseline. A
    /// one-line box has baseline `0`.
    pub baseline: usize,
}

impl MathBox {
    /// Render the box's lines into one string, separated by `sep`.
    ///
    /// Pass `"\n"` for the natural multi-line rendering and `" "` to squeeze the box
    /// onto a single line (what [`Atom::flatten`] does).
    ///
    /// ```
    /// use techxt::mathfmt::MathBox;
    ///
    /// let b = MathBox { lines: vec!["1 2".into(), "3 4".into()], baseline: 0 };
    /// assert_eq!(b.join_lines("\n"), "1 2\n3 4");
    /// ```
    pub fn join_lines(&self, sep: &str) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push_str(sep);
            }
            out.push_str(line);
        }
        out
    }

    /// The display width of the widest line, i.e. the width the box occupies.
    ///
    /// Uses the crate's single width function, so east-asian wide characters and
    /// combining marks are accounted for exactly as the layout engine accounts for
    /// them.
    pub fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| display_width(l))
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn flatten_str_body() {
        let atom = Atom {
            body: AtomBody::Str("𝑥²".into()),
            cls: (AtomClass::Ord, AtomClass::Ord),
            script: None,
        };
        assert_eq!(atom.flatten(), "𝑥²");
    }

    #[test]
    fn flatten_wrappable_keeps_prefix_and_drops_delimiters() {
        let atom = Atom {
            body: AtomBody::Wrappable {
                inner: "i+1".into(),
                prefix: "^".into(),
            },
            cls: (AtomClass::Script, AtomClass::OpenEnd),
            script: Some(ScriptInfo {
                kind: ScriptKind::Superscript,
                latex_notation: true,
            }),
        };
        assert_eq!(atom.flatten(), "^i+1");
    }

    #[test]
    fn flatten_block_body_stays_on_one_line() {
        let atom = Atom {
            body: AtomBody::Block(MathBox {
                lines: vec!["( 1 2 )".into(), "( 3 4 )".into()],
                baseline: 0,
            }),
            cls: (AtomClass::Block, AtomClass::Block),
            script: None,
        };
        assert_eq!(atom.flatten(), "( 1 2 ) ( 3 4 )");
    }

    #[test]
    fn box_width_is_the_widest_line() {
        let b = MathBox {
            lines: vec!["ab".into(), "abcd".into()],
            baseline: 1,
        };
        assert_eq!(b.width(), 4);
        let empty = MathBox {
            lines: vec![],
            baseline: 0,
        };
        assert_eq!(empty.width(), 0);
        assert_eq!(empty.join_lines("\n"), "");
    }
}
