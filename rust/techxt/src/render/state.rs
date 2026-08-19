//! The downward render state (PLAN.md §8).

use crate::convert::Options;
use crate::mathfmt::FontStyle;

/// Everything a node needs to know about where it sits in the document (PLAN.md §8).
///
/// This is the recomposition fold's *downward* state: a handler receives the state its
/// parent derived, and passes a modified copy to whatever it renders below it. techy
/// restores it automatically on the way back up — there is no stack to unwind and no
/// way to forget to pop, which is what makes nesting (`\textbf` inside `\emph` inside a
/// list item inside a table cell) correct by construction.
///
/// Two things are deliberately *not* here. Anything that must survive across siblings —
/// heading counters, collected footnotes, the document title — is per-run state on the
/// renderer, not downward state. And math mode is entered by the math handlers, never
/// inferred from the parser's own mode: whether `$…$` renders as unicode math, as plain
/// concatenation or as LaTeX source is an [option](crate::Options), while the parsing
/// state's mode is what made `^` parse as a script in the first place.
///
/// # Deriving a state
///
/// [`Clone`] it and assign what differs, which keeps the fields a handler does not care
/// about intact. The struct is `#[non_exhaustive]` so that it can grow without breaking
/// anyone, which means it cannot be written as a struct literal (functional update
/// syntax included) from outside techxt — clone and assign instead:
///
/// ```
/// use techxt::render::{FloatKind, RenderState};
/// use techxt::Options;
///
/// let outer = RenderState::initial(&Options::default());
/// let mut inner = outer.clone();
/// inner.float = Some(FloatKind::Figure);
///
/// assert!(outer.float.is_none());
/// assert!(inner.list.is_none());
/// ```
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RenderState {
    /// The math context, or `None` in text mode.
    pub math: Option<MathCtx>,
    /// The font style in effect for text-mode characters.
    pub text_font: FontStyle,
    /// The font style in effect for math-mode characters.
    pub math_font: FontStyle,
    /// The table context: while it is `Some`, `&` separates cells and `\\` separates
    /// rows instead of meaning what they mean in running text.
    pub table: Option<TableCtx>,
    /// The innermost enclosing list, if any.
    pub list: Option<ListCtx>,
    /// The innermost enclosing float, which is what tells `\caption` whether to write
    /// "Figure" or "Table".
    pub float: Option<FloatKind>,
}

impl RenderState {
    /// The state a document starts in: outside everything, with the fonts the options
    /// ask for.
    pub fn initial(options: &Options) -> RenderState {
        RenderState {
            math: None,
            text_font: options.text_font,
            math_font: options.math_font,
            table: None,
            list: None,
            float: None,
        }
    }

    /// Whether math is being rendered.
    pub fn in_math(&self) -> bool {
        self.math.is_some()
    }
}

impl Default for RenderState {
    /// The initial state for [`Options::default`] — see [`initial`](Self::initial).
    fn default() -> RenderState {
        RenderState::initial(&Options::default())
    }
}

/// Where in a formula a node sits (PLAN.md §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathCtx {
    /// Whether this is display math (`\[…\]`, an `equation` body) rather than inline
    /// math. Display math is laid out as an indented block of its own.
    pub display: bool,
    /// Whether this is a matrix body, where `&` separates cells and `\\` separates
    /// rows.
    pub matrix: bool,
}

/// The innermost enclosing list (PLAN.md §8, §9.4).
///
/// Both depths are needed and they differ: the *marker* cycles with
/// [`same_kind_depth`](Self::same_kind_depth), so an `enumerate` nested directly in an
/// `itemize` numbers from `1.` again rather than continuing into `(a)`, while the
/// indentation and the block structure follow [`depth`](Self::depth).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListCtx {
    /// What kind of list it is.
    pub kind: ListKind,
    /// How deeply lists *of this kind* are nested here, counting from 1.
    pub same_kind_depth: usize,
    /// How deeply lists of any kind are nested here, counting from 1.
    pub depth: usize,
}

/// The three kinds of list environment (PLAN.md §9.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    /// A bulleted list (`itemize`): markers cycle through a fixed set of bullets.
    Itemize,
    /// A numbered list (`enumerate`): markers count, in a format that depends on the
    /// nesting depth.
    Enumerate,
    /// A definition list (`description`): the marker is the item's own label.
    Description,
}

/// Which kind of float encloses a node, for `\caption` (PLAN.md §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatKind {
    /// A `figure` environment.
    Figure,
    /// A `table` environment.
    Table,
}

/// Marker for "inside a table body" (PLAN.md §8).
///
/// It carries nothing yet: what the alignment and row separators must *do* is decided
/// by the enclosing table handler, which reads the cell and row markers back out of the
/// flow. The type exists so that the decision "`&` is a cell separator here" is part of
/// the state rather than something a handler has to infer, and so that column
/// information can be added later without changing [`RenderState`]'s shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableCtx;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mathfmt::FontStyleKind;

    #[test]
    fn initial_state_is_outside_everything() {
        let state = RenderState::initial(&Options::default());
        assert!(state.math.is_none());
        assert!(state.table.is_none());
        assert!(state.list.is_none());
        assert!(state.float.is_none());
        assert!(!state.in_math());
    }

    #[test]
    fn initial_fonts_come_from_the_options() {
        let state = RenderState::initial(&Options::default());
        assert_eq!(state.text_font, FontStyle::Default);
        assert_eq!(state.math_font, FontStyle::Style(FontStyleKind::Italic));

        let options = Options {
            text_font: FontStyle::Disabled,
            math_font: FontStyle::Disabled,
            ..Options::default()
        };
        let state = RenderState::initial(&options);
        assert_eq!(state.text_font, FontStyle::Disabled);
        assert_eq!(state.math_font, FontStyle::Disabled);
    }

    #[test]
    fn deriving_keeps_the_other_fields() {
        let outer = RenderState {
            list: Some(ListCtx {
                kind: ListKind::Itemize,
                same_kind_depth: 1,
                depth: 1,
            }),
            ..RenderState::default()
        };
        let inner = RenderState {
            math: Some(MathCtx {
                display: true,
                matrix: false,
            }),
            ..outer.clone()
        };
        assert_eq!(inner.list, outer.list);
        assert!(inner.in_math());
        assert!(!outer.in_math());
    }
}
