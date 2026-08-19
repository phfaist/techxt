//! Two-dimensional math: [`MathBox`], the baseline join, and matrix rendering.
//!
//! Everything in this file that concerns *display* matrices — multi-row delimiter
//! pieces and the baseline join — is new in techxt (PLAN.md §9.5); pylatexenc v3
//! renders a matrix the same way inline and in display and has no box model at all.
//! The inline rendering, by contrast, is v3's `[ 1 2; 3 4 ]` shape with the plan's
//! corrections: per-column widths, a two-space cell separator, and the environment's
//! own delimiters.

use alloc::{boxed::Box, string::String, vec, vec::Vec};

use super::atom::{Atom, AtomClass};
use crate::flow::display_width;

/// A rectangular block of already-rendered text lines with a marked baseline.
///
/// Display-math matrices become boxes: one line per matrix row, columns padded to a
/// common width. The [`baseline`](Self::baseline) is the index of the line that sits
/// on the surrounding formula's baseline, which is what lets several boxes of
/// different heights be joined side by side — see [`baseline_join`].
#[derive(Clone, Debug)]
pub struct MathBox {
    /// The rendered lines, top to bottom.
    ///
    /// Lines are not padded to a common width: a box produced by this module carries
    /// no trailing spaces, because [`baseline_join`] re-pads every piece to its own
    /// width as it composes, and the layout engine must never be handed a line that
    /// ends in whitespace (PLAN.md §7).
    pub lines: Vec<Box<str>>,
    /// Index into [`lines`](Self::lines) of the line that carries the baseline. A
    /// one-line box has baseline `0`.
    pub baseline: usize,
}

impl MathBox {
    /// A box holding a single line, with the baseline on it.
    pub fn single_line(text: impl Into<Box<str>>) -> Self {
        MathBox {
            lines: vec![text.into()],
            baseline: 0,
        }
    }

    /// A box holding nothing at all. The joiner drops the atoms that produce one.
    pub fn empty() -> Self {
        MathBox {
            lines: Vec::new(),
            baseline: 0,
        }
    }

    /// Whether the box has no visible content: no lines, or only empty ones.
    ///
    /// The joiner drops blank pieces before computing separators, so that an atom
    /// which rendered to nothing cannot leave a stray space behind.
    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }

    /// The line the baseline sits on, or `""` for a box with no lines.
    ///
    /// This is the line that takes part in the joiner's spacing decisions: a matrix
    /// meets its neighbours on the baseline row, and the rows above and below it get
    /// blanks of the same width.
    pub fn baseline_text(&self) -> &str {
        self.lines.get(self.baseline).map(|l| &**l).unwrap_or("")
    }

    /// Render the box's lines into one string, separated by `sep`.
    ///
    /// Pass `"\n"` for the natural multi-line rendering and `" "` to squeeze the box
    /// onto a single line (what [`Atom::flatten`](super::Atom::flatten) does).
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

/// Compose boxes side by side, aligning their baselines (PLAN.md §9.5).
///
/// `seps[i]` is placed between piece `i` and piece `i + 1`; a missing entry counts as
/// no separator. The separator is written out on the baseline row only, and the other
/// rows receive spaces of the same display width, so that everything stays in column.
/// Every piece is first padded to its own width and to the common height — blank lines
/// above and below as its baseline requires — and the pieces are then concatenated row
/// by row. The result's baseline is the common one.
///
/// Trailing spaces are trimmed from the finished lines: they carry no information (no
/// content follows them) and the layout engine's invariant is that no line it emits
/// ends in whitespace.
///
/// ```
/// use techxt::mathfmt::{baseline_join, MathBox};
///
/// let matrix = MathBox { lines: vec!["( 1 2 )".into(), "( 3 4 )".into()], baseline: 0 };
/// let letter = MathBox::single_line("𝑣");
/// let joined = baseline_join(&[matrix, letter], &[" "]);
/// assert_eq!(joined.lines[0].as_ref(), "( 1 2 ) 𝑣");
/// assert_eq!(joined.lines[1].as_ref(), "( 3 4 )");
/// ```
pub fn baseline_join(pieces: &[MathBox], seps: &[&str]) -> MathBox {
    if pieces.is_empty() {
        return MathBox::empty();
    }

    // A box with no lines behaves as a box with one empty line, and a baseline that
    // points past the end is clamped: neither can happen for boxes this module builds,
    // but a caller-built box must not be able to panic here.
    let shape: Vec<(&[Box<str>], usize)> = pieces
        .iter()
        .map(|p| {
            let baseline = p.baseline.min(p.lines.len().saturating_sub(1));
            (&p.lines[..], baseline)
        })
        .collect();

    let above = shape.iter().map(|(_, b)| *b).max().unwrap_or(0);
    let below = shape
        .iter()
        .map(|(lines, b)| lines.len().saturating_sub(1).saturating_sub(*b))
        .max()
        .unwrap_or(0);
    let height = above + below + 1;

    let widths: Vec<usize> = pieces.iter().map(|p| p.width()).collect();

    let mut out_lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::new();
        for (i, (lines, baseline)) in shape.iter().enumerate() {
            if i > 0 {
                let sep = seps.get(i - 1).copied().unwrap_or("");
                if row == above {
                    line.push_str(sep);
                } else {
                    pad(&mut line, display_width(sep));
                }
            }
            // Where this piece's own first line falls in the composed box.
            let top = above - baseline;
            let text = if row >= top {
                lines.get(row - top).map(|l| &**l).unwrap_or("")
            } else {
                ""
            };
            line.push_str(text);
            pad(&mut line, widths[i].saturating_sub(display_width(text)));
        }
        while line.ends_with(' ') {
            line.pop();
        }
        out_lines.push(line.into_boxed_str());
    }

    MathBox {
        lines: out_lines,
        baseline: above,
    }
}

/// Append `n` spaces.
fn pad(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

/// The delimiter pair that surrounds a matrix, named after the environment it comes
/// from (DECISIONS.md D3).
///
/// LaTeX spells the delimiters in the environment's name — `pmatrix` is parenthesized,
/// `bmatrix` bracketed — and techxt reproduces the environment's own pair rather than
/// v3's universal `[ ]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixFence {
    /// No delimiters at all: `matrix` and `smallmatrix`.
    None,
    /// `(` … `)`: `pmatrix`.
    Parens,
    /// `[` … `]`: `bmatrix` and `array`.
    Brackets,
    /// `{` … `}`: `Bmatrix`.
    Braces,
    /// `|` … `|`: `vmatrix` (a determinant).
    Vert,
    /// `‖` … `‖`: `Vmatrix` (a norm).
    DoubleVert,
}

impl MatrixFence {
    /// The delimiter pair of a matrix environment, or `None` if the name is not one
    /// of the matrix environments techxt knows (DECISIONS.md D3).
    ///
    /// ```
    /// use techxt::mathfmt::MatrixFence;
    ///
    /// assert_eq!(MatrixFence::for_environment("pmatrix"), Some(MatrixFence::Parens));
    /// assert_eq!(MatrixFence::for_environment("matrix"), Some(MatrixFence::None));
    /// assert_eq!(MatrixFence::for_environment("tabular"), None);
    /// ```
    pub fn for_environment(name: &str) -> Option<MatrixFence> {
        Some(match name {
            "matrix" | "smallmatrix" => MatrixFence::None,
            "pmatrix" | "psmallmatrix" => MatrixFence::Parens,
            "bmatrix" | "bsmallmatrix" | "array" => MatrixFence::Brackets,
            // `cases` draws only a left brace in LaTeX; there is no half-delimited box
            // here, and a pair of braces around a column of alternatives is what the
            // construct means. `dcases` is the same environment in display size.
            "Bmatrix" | "cases" | "dcases" => MatrixFence::Braces,
            "vmatrix" => MatrixFence::Vert,
            "Vmatrix" => MatrixFence::DoubleVert,
            _ => return None,
        })
    }

    /// The plain single-character delimiter pair, as used inline, on a one-row matrix,
    /// and by the [`Ascii`](MatrixDelims::Ascii) style.
    pub fn plain_pair(self) -> Option<(char, char)> {
        Some(match self {
            MatrixFence::None => return None,
            MatrixFence::Parens => ('(', ')'),
            MatrixFence::Brackets => ('[', ']'),
            MatrixFence::Braces => ('{', '}'),
            MatrixFence::Vert => ('|', '|'),
            MatrixFence::DoubleVert => ('\u{2016}', '\u{2016}'),
        })
    }

    /// The delimiter characters for row `row` of a `rows`-row box in the
    /// [`Unicode`](MatrixDelims::Unicode) style.
    ///
    /// Parentheses and brackets come as {top, extension, bottom} triples that stack
    /// into a delimiter of any height. Braces need a fourth piece: U+23A8 `⎨` is the
    /// *centre* of the brace, not an extension, so it goes on the middle row alone and
    /// U+23AA `⎪` fills the rest (DECISIONS.md D2 — PLAN.md §9.5 lists only three
    /// pieces, which would leave a four-row brace with no extension). The two vertical
    /// bars are single characters that simply repeat.
    ///
    /// The middle row is the box's baseline row, `(rows - 1) / 2`, so the brace's
    /// centre and the formula's baseline agree.
    fn unicode_pieces(self, row: usize, rows: usize) -> Option<(char, char)> {
        let last = rows.saturating_sub(1);
        let middle = last / 2;
        Some(match self {
            MatrixFence::None => return None,
            MatrixFence::Parens => {
                if row == 0 {
                    ('\u{239B}', '\u{239E}')
                } else if row == last {
                    ('\u{239D}', '\u{23A0}')
                } else {
                    ('\u{239C}', '\u{239F}')
                }
            }
            MatrixFence::Brackets => {
                if row == 0 {
                    ('\u{23A1}', '\u{23A4}')
                } else if row == last {
                    ('\u{23A3}', '\u{23A6}')
                } else {
                    ('\u{23A2}', '\u{23A5}')
                }
            }
            MatrixFence::Braces => {
                if row == 0 {
                    ('\u{23A7}', '\u{23AB}')
                } else if row == last {
                    ('\u{23A9}', '\u{23AD}')
                } else if row == middle {
                    ('\u{23A8}', '\u{23AC}')
                } else {
                    ('\u{23AA}', '\u{23AA}')
                }
            }
            MatrixFence::Vert => ('\u{2502}', '\u{2502}'),
            MatrixFence::DoubleVert => ('\u{2016}', '\u{2016}'),
        })
    }
}

/// How the delimiters of a *display* matrix are drawn
/// (`Options::matrix_delimiters`, PLAN.md §9.5).
///
/// **Seam for M5b:** `techxt::convert` does not exist yet, so this enum lives here and
/// the matrix builders take it as a parameter. When `Options` lands it should re-export
/// this type rather than declare a second one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatrixDelims {
    /// Stack the unicode multi-row delimiter pieces (`⎛⎜⎝`, `⎡⎢⎣`, `⎧⎪⎨⎩`, …), which
    /// draw a delimiter as tall as the matrix. A one-row matrix uses the plain
    /// characters. This is the default.
    #[default]
    Unicode,
    /// Repeat the plain delimiter character on every row, for terminals and fonts that
    /// do not have the box-drawing pieces.
    Ascii,
}

/// Lay the cells out into one text line per row: every cell right-justified to its
/// column's display width, cells separated by two spaces.
///
/// Right-justification is what makes a column of numbers line up on its units digit,
/// which is the only alignment plain text can offer without knowing the column type.
/// Widths are per column (PLAN.md §9.5), not v3's single global width.
fn layout_rows<R, S>(rows: &[R]) -> Vec<String>
where
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    let mut widths: Vec<usize> = Vec::new();
    for row in rows {
        for (col, cell) in row.as_ref().iter().enumerate() {
            let w = display_width(cell.as_ref());
            if col < widths.len() {
                widths[col] = widths[col].max(w);
            } else {
                widths.push(w);
            }
        }
    }

    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (col, cell) in row.as_ref().iter().enumerate() {
                if col > 0 {
                    line.push_str("  ");
                }
                let cell = cell.as_ref();
                pad(&mut line, widths[col].saturating_sub(display_width(cell)));
                line.push_str(cell);
            }
            line
        })
        .collect()
}

/// Render a matrix on one line, in the `[ 1  2; 30  4 ]` form, for inline math.
///
/// The rows are separated by `"; "` and the whole is set off from its delimiters by
/// one space on each side. An empty matrix — valid LaTeX — renders as just the
/// delimiter pair, `[ ]`, and a matrix with no delimiters
/// ([`MatrixFence::None`](MatrixFence::None)) renders as its rows alone.
fn inline_matrix_text<R, S>(rows: &[R], fence: MatrixFence) -> String
where
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    let contents = layout_rows(rows).join("; ");
    match fence.plain_pair() {
        None => contents,
        Some((open, close)) => {
            let mut out = String::new();
            out.push(open);
            out.push(' ');
            if !contents.is_empty() {
                out.push_str(&contents);
                out.push(' ');
            }
            out.push(close);
            out
        }
    }
}

/// Build the inline rendering of a matrix as a single-line [`Atom`] (PLAN.md §9.5).
///
/// `rows` is the matrix contents, one inner slice per row, each cell already rendered
/// to text. The atom's class pair is [`Block`](AtomClass::Block) on both edges: a
/// matrix delimits itself, so it hugs whatever it meets — `sin[ 1 2 ]` and `2[ 1 2 ]`
/// stay tight — while remaining recognizable as a block.
///
/// ```
/// use techxt::mathfmt::{inline_matrix_atom, MatrixFence};
///
/// let m = inline_matrix_atom(&[["1", "2"], ["30", "4"]], MatrixFence::Parens);
/// assert_eq!(m.flatten(), "(  1  2; 30  4 )");
/// ```
pub fn inline_matrix_atom<R, S>(rows: &[R], fence: MatrixFence) -> Atom
where
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    Atom::from_text(
        inline_matrix_text(rows, fence),
        AtomClass::both(AtomClass::Block),
    )
}

/// Build the display rendering of a matrix: one line per row, in a [`MathBox`].
///
/// The box's baseline is `(rows - 1) / 2`, so a surrounding formula meets the matrix
/// at its middle row. Delimiters are drawn per `style`; see [`MatrixDelims`]. An empty
/// matrix renders as the one-line `[ ]` and never panics.
///
/// ```
/// use techxt::mathfmt::{display_matrix_box, MatrixDelims, MatrixFence};
///
/// let b = display_matrix_box(&[["1", "2"], ["30", "4"]], MatrixFence::Parens,
///                            MatrixDelims::Unicode);
/// assert_eq!(b.lines[0].as_ref(), "⎛  1  2 ⎞");
/// assert_eq!(b.lines[1].as_ref(), "⎝ 30  4 ⎠");
/// assert_eq!(b.baseline, 0);
/// ```
pub fn display_matrix_box<R, S>(rows: &[R], fence: MatrixFence, style: MatrixDelims) -> MathBox
where
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    let body = layout_rows(rows);
    if body.is_empty() {
        // No rows at all: there is nothing to stack, so fall back on the inline shape,
        // which is `[ ]` for a fenced matrix and empty otherwise.
        let text = inline_matrix_text(rows, fence);
        return if text.is_empty() {
            MathBox::empty()
        } else {
            MathBox::single_line(text)
        };
    }

    let n = body.len();
    let baseline = (n - 1) / 2;
    let lines = body
        .into_iter()
        .enumerate()
        .map(|(row, text)| {
            let pieces = match style {
                MatrixDelims::Ascii => fence.plain_pair(),
                MatrixDelims::Unicode if n == 1 => fence.plain_pair(),
                MatrixDelims::Unicode => fence.unicode_pieces(row, n),
            };
            match pieces {
                None => text.into_boxed_str(),
                Some((open, close)) => {
                    let mut line = String::new();
                    line.push(open);
                    line.push(' ');
                    line.push_str(&text);
                    line.push(' ');
                    line.push(close);
                    line.into_boxed_str()
                }
            }
        })
        .collect();

    MathBox { lines, baseline }
}

/// Build the display rendering of a matrix as a [`Block`](super::AtomBody::Block) atom.
///
/// Same rendering as [`display_matrix_box`], wrapped in the atom the joiner expects,
/// with the class pair [`Block`](AtomClass::Block) on both edges.
pub fn display_matrix_atom<R, S>(rows: &[R], fence: MatrixFence, style: MatrixDelims) -> Atom
where
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    Atom {
        body: super::AtomBody::Block(display_matrix_box(rows, fence, style)),
        cls: AtomClass::both(AtomClass::Block),
        script: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of a matrix, as the builders take them.
    const M: [[&str; 2]; 2] = [["1", "2"], ["30", "4"]];

    #[test]
    fn box_width_is_the_widest_line() {
        let b = MathBox {
            lines: vec!["ab".into(), "abcd".into()],
            baseline: 1,
        };
        assert_eq!(b.width(), 4);
        let empty = MathBox::empty();
        assert_eq!(empty.width(), 0);
        assert_eq!(empty.join_lines("\n"), "");
        assert!(empty.is_blank());
        assert_eq!(empty.baseline_text(), "");
    }

    #[test]
    fn baseline_join_pads_shorter_pieces() {
        let tall = MathBox {
            lines: vec!["⎡ 1 ⎤".into(), "⎣ 2 ⎦".into()],
            baseline: 0,
        };
        let joined = baseline_join(&[MathBox::single_line("𝐴"), tall], &[" "]);
        assert_eq!(joined.lines[0].as_ref(), "𝐴 ⎡ 1 ⎤");
        // the second row keeps the column: one space for `𝐴`, one for the separator
        assert_eq!(joined.lines[1].as_ref(), "  ⎣ 2 ⎦");
        assert_eq!(joined.baseline, 0);
    }

    #[test]
    fn baseline_join_aligns_baselines_not_tops() {
        let left = MathBox {
            lines: vec!["a".into(), "b".into(), "c".into()],
            baseline: 2,
        };
        let right = MathBox {
            lines: vec!["x".into(), "y".into()],
            baseline: 0,
        };
        let joined = baseline_join(&[left, right], &[""]);
        assert_eq!(joined.lines.len(), 4);
        assert_eq!(joined.lines[0].as_ref(), "a");
        assert_eq!(joined.lines[1].as_ref(), "b");
        assert_eq!(joined.lines[2].as_ref(), "cx");
        assert_eq!(joined.lines[3].as_ref(), " y");
        assert_eq!(joined.baseline, 2);
    }

    #[test]
    fn baseline_join_tolerates_degenerate_boxes() {
        let bogus = MathBox {
            lines: vec![],
            baseline: 7,
        };
        let joined = baseline_join(&[bogus, MathBox::single_line("x")], &[]);
        assert_eq!(joined.lines, vec!["x".into()]);
        assert_eq!(baseline_join(&[], &[]).lines.len(), 0);
    }

    #[test]
    fn inline_matrix_uses_per_column_widths_and_two_spaces() {
        assert_eq!(
            inline_matrix_atom(&M, MatrixFence::Brackets).flatten(),
            "[  1  2; 30  4 ]"
        );
        assert_eq!(
            inline_matrix_atom(&M, MatrixFence::None).flatten(),
            " 1  2; 30  4"
        );
    }

    #[test]
    fn inline_empty_matrix_is_a_bare_delimiter_pair() {
        let empty: [[&str; 0]; 0] = [];
        assert_eq!(
            inline_matrix_atom(&empty, MatrixFence::Brackets).flatten(),
            "[ ]"
        );
        assert_eq!(inline_matrix_atom(&empty, MatrixFence::None).flatten(), "");
    }

    #[test]
    fn display_matrix_ascii_style_repeats_the_plain_char() {
        let b = display_matrix_box(&M, MatrixFence::Parens, MatrixDelims::Ascii);
        assert_eq!(b.lines[0].as_ref(), "(  1  2 )");
        assert_eq!(b.lines[1].as_ref(), "( 30  4 )");
    }

    #[test]
    fn display_one_row_matrix_uses_plain_delimiters() {
        let b = display_matrix_box(&[["1", "2"]], MatrixFence::Brackets, MatrixDelims::Unicode);
        assert_eq!(b.lines, vec!["[ 1  2 ]".into()]);
        assert_eq!(b.baseline, 0);
    }

    #[test]
    fn display_braces_get_the_four_piece_assembly() {
        let rows = [["1"], ["2"], ["3"], ["4"]];
        let b = display_matrix_box(&rows, MatrixFence::Braces, MatrixDelims::Unicode);
        assert_eq!(b.lines[0].as_ref(), "\u{23A7} 1 \u{23AB}");
        assert_eq!(b.lines[1].as_ref(), "\u{23A8} 2 \u{23AC}");
        assert_eq!(b.lines[2].as_ref(), "\u{23AA} 3 \u{23AA}");
        assert_eq!(b.lines[3].as_ref(), "\u{23A9} 4 \u{23AD}");
        assert_eq!(b.baseline, 1);
    }

    #[test]
    fn display_brackets_and_bars_in_the_unicode_style() {
        let rows = [["1"], ["2"], ["3"]];
        let b = display_matrix_box(&rows, MatrixFence::Brackets, MatrixDelims::Unicode);
        assert_eq!(b.lines[0].as_ref(), "\u{23A1} 1 \u{23A4}");
        assert_eq!(b.lines[1].as_ref(), "\u{23A2} 2 \u{23A5}");
        assert_eq!(b.lines[2].as_ref(), "\u{23A3} 3 \u{23A6}");

        let b = display_matrix_box(&rows, MatrixFence::Parens, MatrixDelims::Unicode);
        assert_eq!(b.lines[0].as_ref(), "\u{239B} 1 \u{239E}");
        assert_eq!(b.lines[1].as_ref(), "\u{239C} 2 \u{239F}");
        assert_eq!(b.lines[2].as_ref(), "\u{239D} 3 \u{23A0}");

        // the two bars are single characters that simply repeat
        let b = display_matrix_box(&rows, MatrixFence::Vert, MatrixDelims::Unicode);
        assert_eq!(
            b.lines,
            vec![
                "\u{2502} 1 \u{2502}".into(),
                "\u{2502} 2 \u{2502}".into(),
                "\u{2502} 3 \u{2502}".into()
            ]
        );
        let b = display_matrix_box(&rows, MatrixFence::DoubleVert, MatrixDelims::Unicode);
        assert_eq!(
            b.lines,
            vec![
                "\u{2016} 1 \u{2016}".into(),
                "\u{2016} 2 \u{2016}".into(),
                "\u{2016} 3 \u{2016}".into()
            ]
        );
    }

    #[test]
    fn the_ascii_style_never_uses_a_box_drawing_piece() {
        let rows = [["1"], ["2"], ["3"]];
        for fence in [
            MatrixFence::Parens,
            MatrixFence::Brackets,
            MatrixFence::Braces,
            MatrixFence::Vert,
            MatrixFence::DoubleVert,
        ] {
            let b = display_matrix_box(&rows, fence, MatrixDelims::Ascii);
            let (open, close) = fence.plain_pair().expect("every fence here has a pair");
            for line in &b.lines {
                assert!(line.starts_with(open), "{line:?}");
                assert!(line.ends_with(close), "{line:?}");
            }
        }
    }

    #[test]
    fn display_empty_matrix_does_not_panic() {
        let empty: [[&str; 0]; 0] = [];
        let b = display_matrix_box(&empty, MatrixFence::Brackets, MatrixDelims::Unicode);
        assert_eq!(b.lines, vec!["[ ]".into()]);
        let b = display_matrix_box(&empty, MatrixFence::None, MatrixDelims::Unicode);
        assert!(b.is_blank());
    }

    #[test]
    fn environment_names_map_to_their_own_delimiters() {
        for (env, fence) in [
            ("matrix", MatrixFence::None),
            ("smallmatrix", MatrixFence::None),
            ("pmatrix", MatrixFence::Parens),
            ("bmatrix", MatrixFence::Brackets),
            ("array", MatrixFence::Brackets),
            ("Bmatrix", MatrixFence::Braces),
            ("vmatrix", MatrixFence::Vert),
            ("Vmatrix", MatrixFence::DoubleVert),
        ] {
            assert_eq!(MatrixFence::for_environment(env), Some(fence), "{env}");
        }
        assert_eq!(MatrixFence::for_environment("equation"), None);
    }

    #[test]
    fn ragged_rows_do_not_panic() {
        let rows: [&[&str]; 2] = [&["1", "2", "3"], &["4"]];
        let atom = inline_matrix_atom(&rows, MatrixFence::Brackets);
        assert_eq!(atom.flatten(), "[ 1  2  3; 4 ]");
    }
}
