//! The text flow: the typed token sequence that handlers produce and the
//! [layout engine](crate::layout) renders.
//!
//! techxt never lets a handler write layout. A handler that wanted a blank line before
//! a display equation would have to know whether the preceding handler already emitted
//! one, whether it is inside a list, and how wide the remaining line is — which is
//! exactly how pylatexenc ends up with accidental double blank lines and with wrapping
//! that cuts verbatim blocks apart. So handlers emit *intent* instead: a word, a place
//! where a line may break, a request for vertical separation, an indented block, a run
//! of text that must survive untouched. That is a [`Flow`], and a single pass over it
//! ([`crate::layout`]) decides every line break, blank line and indent from complete
//! information.
//!
//! # The pieces
//!
//! - [`FlowItem::Text`] is a run of text carrying no *collapsible* whitespace.
//!   **Adjacent `Text` items are one unbreakable word**: `\textbf{bold}text`
//!   contributes `Text("𝐛𝐨𝐥𝐝")` then `Text("text")`, and no line break may fall
//!   between them.
//! - [`FlowItem::Glue`] is one collapsible inter-word space, and the *only* place a
//!   line may break.
//! - [`FlowItem::HardBreak`] and [`FlowItem::ParagraphBreak`] request vertical
//!   separation; so do [`FlowItem::BlockStart`] and [`FlowItem::BlockEnd`]. Layout
//!   merges consecutive requests instead of summing them.
//! - [`FlowItem::Verbatim`] and [`FlowItem::InlineVerbatim`] are content that layout
//!   must not touch.
//!
//! # Composition
//!
//! `Flow` is techy's piece monoid for the recomposition fold: it implements
//! [`ComposePiece`], where the empty piece is the empty flow and appending is
//! concatenation. It is a newtype over `Vec<FlowItem>`, so appending a child's
//! contribution is an amortized O(1)-per-item `Vec::append` rather than the quadratic
//! string copying a `String` piece would do.
//!
//! ```
//! use techxt::flow::Flow;
//! use techxt::layout::{render, LayoutOptions};
//!
//! let mut flow = Flow::from_plain_text("the quick");
//! flow.extend(Flow::glue());
//! flow.extend(Flow::text("brown"));       // one word, no splitting
//! assert_eq!(render(&flow, &LayoutOptions::default()), "the quick brown\n");
//! ```

use alloc::{boxed::Box, vec::Vec};

use techy::recompose::ComposePiece;

use crate::mathfmt::Atom;

/// A sequence of [`FlowItem`]s: what a handler produces and what
/// [`layout`](crate::layout) renders.
///
/// See the [module documentation](self) for what the items mean and why the flow
/// exists at all. Build one with [`Flow::text`], [`Flow::from_plain_text`],
/// [`Flow::glue`] or [`Flow::push`], and concatenate with [`Flow::extend`].
#[derive(Clone, Debug, Default)]
pub struct Flow(Vec<FlowItem>);

impl Flow {
    /// An empty flow.
    ///
    /// ```
    /// use techxt::flow::Flow;
    /// assert!(Flow::new().is_empty());
    /// ```
    pub const fn new() -> Flow {
        Flow(Vec::new())
    }

    /// Append one item.
    pub fn push(&mut self, item: FlowItem) {
        self.0.push(item);
    }

    /// Append every item of `other`, leaving it empty.
    ///
    /// This is the flow's monoid append (see [`ComposePiece`]); it moves `other`'s
    /// buffer contents rather than cloning them.
    ///
    /// ```
    /// use techxt::flow::Flow;
    /// let mut flow = Flow::text("a");
    /// flow.extend(Flow::glue());
    /// flow.extend(Flow::text("b"));
    /// assert_eq!(flow.items().len(), 3);
    /// ```
    pub fn extend(&mut self, mut other: Flow) {
        self.0.append(&mut other.0);
    }

    /// A flow holding exactly one [`FlowItem::Text`] with these contents.
    ///
    /// The text is **not** split: whatever is passed becomes a single item, and
    /// therefore a single unbreakable word. Use it for content that is already known
    /// to be one word — a rendered symbol, a substituted macro replacement, a URL. For
    /// document text that still contains spaces and newlines, use
    /// [`from_plain_text`](Self::from_plain_text) instead.
    ///
    /// ```
    /// use techxt::flow::Flow;
    /// use techxt::layout::{render, LayoutOptions};
    ///
    /// // one item, so no break may fall inside it even though it contains a space
    /// assert_eq!(Flow::text("a b").items().len(), 1);
    /// assert_eq!(render(&Flow::text("→"), &LayoutOptions::default()), "→\n");
    /// ```
    pub fn text(text: &str) -> Flow {
        Flow(alloc::vec![FlowItem::Text(text.into())])
    }

    /// A flow holding exactly one [`FlowItem::Glue`].
    pub fn glue() -> Flow {
        Flow(alloc::vec![FlowItem::Glue])
    }

    /// Split plain document text into words, glue and paragraph breaks.
    ///
    /// This is LaTeX's whitespace rule, and the only place techxt applies it: runs of
    /// non-whitespace become [`Text`](FlowItem::Text) items, and each run of
    /// whitespace becomes exactly one [`Glue`](FlowItem::Glue) — or one
    /// [`ParagraphBreak`](FlowItem::ParagraphBreak) if the run contains **two or more
    /// newlines**, which is what a blank line in the source means. Whitespace is
    /// [`char::is_whitespace`], i.e. the Unicode `White_Space` property, so a
    /// no-break space in the source is a break opportunity like any other; content
    /// that must not break (what `~` produces) is emitted as a
    /// [`Text`](FlowItem::Text) item and never passes through here. Only `'\n'`
    /// counts as a newline, so a `\r\n` pair counts as one.
    ///
    /// Leading and trailing whitespace produces leading and trailing glue, which is
    /// deliberate: the flow of a fragment must still compose correctly with what comes
    /// before and after it. Layout drops glue at the start and end of a line, so it
    /// never reaches the output.
    ///
    /// ```
    /// use techxt::flow::Flow;
    /// use techxt::layout::{render, LayoutOptions};
    ///
    /// let flow = Flow::from_plain_text("one\n\n\n\ntwo");
    /// // four newlines are still one paragraph break, and one blank line
    /// assert_eq!(render(&flow, &LayoutOptions::default()), "one\n\ntwo\n");
    ///
    /// let flow = Flow::from_plain_text("Hello  world");
    /// assert_eq!(render(&flow, &LayoutOptions::default()), "Hello world\n");
    /// ```
    pub fn from_plain_text(text: &str) -> Flow {
        let mut flow = Flow::new();
        let mut rest = text;
        while !rest.is_empty() {
            // `rest` starts either with whitespace or with a word; handle whichever it
            // is and loop. Both branches consume at least one byte, so this terminates.
            let space_len = rest
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(rest.len());
            if space_len > 0 {
                let spaces = &rest[..space_len];
                // A blank line in the source is two newlines; anything more is still
                // one paragraph break (layout would merge them anyway, but the flow
                // should already say what the source meant).
                let newlines = spaces.bytes().filter(|&b| b == b'\n').count();
                flow.push(if newlines >= 2 {
                    FlowItem::ParagraphBreak
                } else {
                    FlowItem::Glue
                });
                rest = &rest[space_len..];
                continue;
            }
            let word_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
            flow.push(FlowItem::Text(rest[..word_len].into()));
            rest = &rest[word_len..];
        }
        flow
    }

    /// The items, in order.
    pub fn items(&self) -> &[FlowItem] {
        &self.0
    }

    /// Consume the flow and return its items.
    pub fn into_items(self) -> Vec<FlowItem> {
        self.0
    }

    /// Whether the flow holds no items at all.
    ///
    /// Note that this asks about *items*, not about output: a flow of one empty
    /// [`Text`](FlowItem::Text) is not empty here, even though it renders to nothing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The flow is techy's piece monoid: the empty flow and concatenation.
impl ComposePiece for Flow {
    fn empty() -> Flow {
        Flow::new()
    }

    fn append(&mut self, other: Flow) {
        self.extend(other);
    }
}

/// One token of a [`Flow`].
///
/// The variants carry intent, not formatting: nothing here says how many spaces or
/// newlines to emit, because that is the [layout engine](crate::layout)'s decision.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum FlowItem {
    /// Text carrying no *collapsible* whitespace.
    ///
    /// **Adjacent `Text` items are glued**: no break may occur between them, and the
    /// wrap decision measures them as one word. That is what keeps
    /// `\textbf{bold}text` from wrapping between `bold` and `text`, without any
    /// handler having to know that it is adjacent to anything.
    ///
    /// A space that a handler *means* — `~` as `\u{00A0}`, `\quad` as `\u{2003}` — is
    /// content and belongs here, even though Unicode calls those characters
    /// whitespace: layout neither collapses nor trims a `Text` item, and breaking at
    /// one would defeat the macro that asked for it. What must never appear here is
    /// whitespace the layout engine would otherwise have collapsed — an ASCII space,
    /// tab or newline. Those are [`Glue`](FlowItem::Glue), and putting one in a `Text`
    /// leaks it into the output as a trailing space at a line end.
    Text(Box<str>),
    /// One collapsible inter-word space; the only place wrapping may break.
    ///
    /// Consecutive glue collapses to one, and glue at the start or end of a line is
    /// dropped, so no output line ends in *collapsible* whitespace. Whitespace that is
    /// content — the spaces inside a [`Verbatim`](FlowItem::Verbatim) or
    /// [`InlineVerbatim`](FlowItem::InlineVerbatim) payload, a no-break space emitted as
    /// [`Text`](FlowItem::Text) — is not glue and is never dropped, so `\verb| x |` at
    /// the end of a line does leave a space there.
    Glue,
    /// Forced line break (`\\`, `\newline`): the next content starts on a new line,
    /// without a blank line in between.
    HardBreak,
    /// Paragraph separator: the next content starts after a blank line.
    ///
    /// Layout normalizes runs of these — and the separation that blocks request — to
    /// at most one blank line.
    ParagraphBreak,
    /// Preformatted block (`verbatim`, `lstlisting`, source-mode display math):
    /// emitted line by line with the current continuation indent, and **never**
    /// wrapped, styled or whitespace-normalized. Blank-line separated from its
    /// surroundings.
    Verbatim(Box<str>),
    /// Inline preformatted fragment (`\verb`): an unbreakable word emitted exactly as
    /// given.
    InlineVerbatim(Box<str>),
    /// Open a block context, which is separated from its surroundings and gives its
    /// lines a prefix (see [`BlockKind`]).
    BlockStart(BlockKind),
    /// Close the innermost open block.
    BlockEnd,
    /// Column separator inside a table or matrix body (`&`).
    ///
    /// Table markers are consumed by the enclosing table or matrix handler *before*
    /// layout, so layout never sees them in correct operation. Should one escape, the
    /// layout engine renders it as [`Glue`](Self::Glue) in release builds and trips a
    /// `debug_assert!` in debug builds.
    CellSep,
    /// Row separator inside a table or matrix body (`\\`).
    ///
    /// Consumed before layout like [`CellSep`](Self::CellSep); layout's release
    /// fallback is [`HardBreak`](Self::HardBreak).
    RowSep,
    /// Horizontal rule marker inside a table body (`\hline`, `\toprule`).
    ///
    /// Consumed before layout like [`CellSep`](Self::CellSep); layout's release
    /// fallback is [`HardBreak`](Self::HardBreak).
    RuleMark,
    /// A math atom, which exists only transiently inside a math subtree.
    ///
    /// The math-group handler collects the atoms of its body, joins them, and emits
    /// ordinary text, glue and verbatim lines, so atoms do not reach layout in correct
    /// operation. Layout's fallback is to emit the atom's
    /// [flattened](Atom::flatten) text as a word.
    MathAtom(Atom),
}

/// What kind of block a [`FlowItem::BlockStart`] opens.
///
/// Both kinds are hanging-indent blocks with two prefixes: `first` goes on the block's
/// first line and `cont` on every line after it, including lines produced by wrapping.
/// A nested block's prefixes are appended to the enclosing blocks' `cont` prefixes, and
/// the total prefix width counts toward the wrap column.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum BlockKind {
    /// A plain hanging-indent block: display math, quotations, footnote entries,
    /// placeholder blocks.
    Indent {
        /// Prefix for the block's first line.
        first: Box<str>,
        /// Prefix for every subsequent line of the block.
        cont: Box<str>,
    },
    /// A list-item block, whose `first` prefix is the item marker (`  • `, `1. `).
    ///
    /// Opening an `Item` while another `Item` is the innermost open block closes that
    /// one first. That is what lets `\item` emit a `BlockStart` and nothing else: it
    /// never has to look ahead to find where its own item ends. A list *environment*
    /// consequently has to open a block of its own around its items, so that the first
    /// item of a nested list does not close the enclosing list's item.
    Item {
        /// The item marker, used as the prefix of the item's first line.
        first: Box<str>,
        /// Prefix for every subsequent line of the item.
        cont: Box<str>,
    },
}

/// The display width of a string, in terminal columns.
///
/// **This is the only place in techxt where a width is computed** (PLAN.md §6). Line
/// wrapping, table column alignment, matrix padding, heading underlines and the
/// `\maketitle` rule all call it, so that they agree exactly on what a wide CJK
/// character, a combining accent or a zero-width joiner costs. Anything that reaches
/// for `str::len` or `chars().count()` instead is a bug.
pub(crate) fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, string::String, vec};

    /// Compact rendering of a flow's shape, for expected-value tests.
    fn shape(flow: &Flow) -> String {
        let mut out = String::new();
        for item in flow.items() {
            if !out.is_empty() {
                out.push(' ');
            }
            match item {
                FlowItem::Text(t) => out.push_str(&format!("T({t})")),
                FlowItem::Glue => out.push('G'),
                FlowItem::ParagraphBreak => out.push('P'),
                other => out.push_str(&format!("{other:?}")),
            }
        }
        out
    }

    #[test]
    fn new_and_default_are_empty() {
        assert!(Flow::new().is_empty());
        assert!(Flow::default().is_empty());
        assert!(Flow::new().items().is_empty());
    }

    #[test]
    fn text_does_not_split() {
        assert_eq!(shape(&Flow::text("a b\nc")), "T(a b\nc)");
        assert_eq!(Flow::text("").items().len(), 1);
    }

    #[test]
    fn glue_is_one_item() {
        assert_eq!(shape(&Flow::glue()), "G");
    }

    #[test]
    fn from_plain_text_splits_words_and_spaces() {
        assert_eq!(shape(&Flow::from_plain_text("")), "");
        assert_eq!(shape(&Flow::from_plain_text("one")), "T(one)");
        assert_eq!(shape(&Flow::from_plain_text("one two")), "T(one) G T(two)");
        assert_eq!(
            shape(&Flow::from_plain_text("one  \t two")),
            "T(one) G T(two)"
        );
        assert_eq!(shape(&Flow::from_plain_text(" a ")), "G T(a) G");
        assert_eq!(shape(&Flow::from_plain_text("  ")), "G");
    }

    #[test]
    fn from_plain_text_recognizes_blank_lines() {
        // one newline is still just a space
        assert_eq!(shape(&Flow::from_plain_text("a\nb")), "T(a) G T(b)");
        // two or more make a paragraph break, however many
        assert_eq!(shape(&Flow::from_plain_text("a\n\nb")), "T(a) P T(b)");
        assert_eq!(shape(&Flow::from_plain_text("a\n\n\n\nb")), "T(a) P T(b)");
        assert_eq!(shape(&Flow::from_plain_text("a \n \n b")), "T(a) P T(b)");
        // \r\n counts its single \n
        assert_eq!(shape(&Flow::from_plain_text("a\r\nb")), "T(a) G T(b)");
        assert_eq!(shape(&Flow::from_plain_text("a\r\n\r\nb")), "T(a) P T(b)");
    }

    #[test]
    fn from_plain_text_uses_unicode_whitespace() {
        // `char::is_whitespace` is the Unicode White_Space property, so a
        // non-breaking space is a break opportunity here too.
        assert_eq!(shape(&Flow::from_plain_text("é\u{a0}x")), "T(é) G T(x)");
        // but a zero-width joiner is not whitespace and stays inside the word
        assert_eq!(shape(&Flow::from_plain_text("a\u{200d}b")), "T(a\u{200d}b)");
    }

    #[test]
    fn extend_concatenates_and_empties_the_argument() {
        let mut a = Flow::text("x");
        let b = Flow::from_plain_text("y z");
        a.extend(b);
        assert_eq!(shape(&a), "T(x) T(y) G T(z)");
    }

    #[test]
    fn into_items_round_trips() {
        let flow = Flow::from_plain_text("a b");
        let items = flow.into_items();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn compose_piece_is_empty_and_append() {
        let mut piece = <Flow as ComposePiece>::empty();
        assert!(piece.is_empty());
        piece.append(Flow::text("a"));
        piece.append(Flow::glue());
        assert_eq!(shape(&piece), "T(a) G");
    }

    #[test]
    fn push_accepts_every_kind_of_item() {
        let mut flow = Flow::new();
        flow.push(FlowItem::HardBreak);
        flow.push(FlowItem::Verbatim("v".into()));
        flow.push(FlowItem::InlineVerbatim("i".into()));
        flow.push(FlowItem::BlockStart(BlockKind::Indent {
            first: "> ".into(),
            cont: "| ".into(),
        }));
        flow.push(FlowItem::BlockEnd);
        assert_eq!(flow.items().len(), 5);
    }

    #[test]
    fn display_width_counts_columns_not_bytes() {
        assert_eq!(display_width("abc"), 3);
        // a wide character occupies two columns
        assert_eq!(display_width("漢字"), 4);
        // a combining mark occupies none
        assert_eq!(display_width("e\u{301}"), 1);
        // mathematical bold letters are one column each, unlike their byte length
        assert_eq!(display_width("𝐛𝐨𝐥𝐝"), 4);
    }

    #[test]
    fn flow_items_are_debuggable() {
        let items = vec![FlowItem::Glue, FlowItem::CellSep, FlowItem::RowSep];
        assert!(format!("{items:?}").contains("CellSep"));
    }
}
