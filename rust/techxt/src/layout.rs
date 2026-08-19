//! The layout engine: one pass over a [`Flow`] that decides every line break, blank
//! line and indent.
//!
//! Everything about the shape of the output is decided here and nowhere else. A
//! handler that emits a display equation says "open an indented block"; it does not
//! count blank lines, it does not know the wrap width, and it cannot break a verbatim
//! block by accident. That concentration is the point: line breaking needs to see a
//! whole paragraph, and blank-line normalization needs to see both sides of a
//! boundary, so both belong to a pass that has the whole document in front of it.
//!
//! ```
//! use techxt::flow::Flow;
//! use techxt::layout::{render, LayoutOptions};
//!
//! let flow = Flow::from_plain_text("the quick brown fox jumps");
//! let mut opts = LayoutOptions::default();
//! opts.wrap_width = Some(16);
//! assert_eq!(render(&flow, &opts), "the quick brown\nfox jumps\n");
//! ```
//!
//! # The algorithm
//!
//! One pass, with an indent stack, a current column, a pending-glue flag and a
//! pending-separation level:
//!
//! 1. **Words.** Adjacent [`Text`](FlowItem::Text) items (and
//!    [`InlineVerbatim`](FlowItem::InlineVerbatim), which is a word too) accumulate
//!    into one unbreakable word. When the word is emitted and glue is pending, the
//!    engine breaks the line instead of writing the space if the word would not fit
//!    (`col + 1 + width > wrap_width`). A word that does not fit even on a fresh line
//!    overflows; it is never split.
//! 2. **Glue collapses.** Consecutive [`Glue`](FlowItem::Glue) is one potential break.
//!    Glue at the start or end of a line is dropped, so no line ever ends in
//!    whitespace.
//! 3. **Vertical separation is normalized here and only here.** Every request —
//!    [`HardBreak`](FlowItem::HardBreak), [`ParagraphBreak`](FlowItem::ParagraphBreak),
//!    [`BlockStart`](FlowItem::BlockStart), [`BlockEnd`](FlowItem::BlockEnd), a
//!    [`Verbatim`](FlowItem::Verbatim) block's two edges — raises a pending level, and
//!    consecutive requests **merge** (take the maximum) rather than sum. There is
//!    therefore at most one blank line anywhere in the output, none at the start, none
//!    at the end, and a nonempty output ends with exactly one `\n`.
//! 4. **Blocks.** A [`BlockStart`](FlowItem::BlockStart) pushes its `(first, cont)`
//!    prefixes. A line's prefix is the `cont` of every enclosing block followed by the
//!    innermost block's `first` — or its `cont` once that block has produced a line.
//!    Prefix widths count toward the wrap column. An [`Item`](BlockKind::Item) block
//!    closes a previous `Item` that is still the innermost open block.
//! 5. **Verbatim.** [`Verbatim`](FlowItem::Verbatim) is emitted line by line with the
//!    current continuation indent and nothing else: never wrapped, never trimmed,
//!    never normalized, blank-line separated from its surroundings. Its payload
//!    therefore appears in the output byte for byte.
//! 6. **No wrapping is the same pass** with the width comparison skipped, so the two
//!    modes cannot drift apart.
//!
//! # Separation levels
//!
//! Requests come at two strengths, and merging takes the stronger one:
//!
//! | request | strength |
//! |---|---|
//! | [`BlockStart`](FlowItem::BlockStart), [`BlockEnd`](FlowItem::BlockEnd), [`HardBreak`](FlowItem::HardBreak) | start a new line |
//! | [`ParagraphBreak`](FlowItem::ParagraphBreak), the edges of a [`Verbatim`](FlowItem::Verbatim) | leave a blank line |
//!
//! Blocks separating at line strength rather than blank-line strength is what makes
//! consecutive list items come out tight (PLAN.md §15 example 22), while
//! `\begin{verbatim}` still stands apart from its surroundings (example 24). A handler
//! that wants a blank line around its block — display math, a quotation — emits a
//! [`ParagraphBreak`](FlowItem::ParagraphBreak) next to the
//! [`BlockStart`](FlowItem::BlockStart); the merge rule guarantees that doing so can
//! never produce two blank lines, however the neighbouring content ends.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::flow::{display_width, BlockKind, Flow, FlowItem};

/// How to lay a [`Flow`] out.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    /// Wrap lines to this display width, or `None` (the default) not to wrap at all.
    ///
    /// The width is measured in terminal columns and *includes* block prefixes, so an
    /// indented list item has that much less room for its text. A word that does not
    /// fit on a fresh line is emitted anyway, overflowing the width: techxt never
    /// splits a word, because in converted text a long "word" is typically a URL, a
    /// file path or an identifier that must survive intact.
    pub wrap_width: Option<usize>,
}

/// Render a flow to a [`String`].
///
/// ```
/// use techxt::flow::{Flow, FlowItem};
/// use techxt::layout::{render, LayoutOptions};
///
/// let mut flow = Flow::text("one");
/// flow.push(FlowItem::ParagraphBreak);
/// flow.extend(Flow::text("two"));
/// assert_eq!(render(&flow, &LayoutOptions::default()), "one\n\ntwo\n");
/// ```
pub fn render(flow: &Flow, opts: &LayoutOptions) -> String {
    let mut out = String::new();
    // `String`'s `fmt::Write` impl is infallible, so this cannot fail and there is
    // nothing to report; ignoring the result keeps the panic-free promise.
    let _ = render_to(flow, opts, &mut out);
    out
}

/// Render a flow into any [`core::fmt::Write`] sink, forwarding its errors.
///
/// Use this to lay a document out straight into a writer instead of building the whole
/// string first.
///
/// ```
/// use techxt::flow::Flow;
/// use techxt::layout::{render_to, LayoutOptions};
///
/// let mut out = String::new();
/// render_to(&Flow::from_plain_text("a b"), &LayoutOptions::default(), &mut out).unwrap();
/// assert_eq!(out, "a b\n");
/// ```
pub fn render_to(flow: &Flow, opts: &LayoutOptions, out: &mut dyn Write) -> core::fmt::Result {
    let mut engine = Engine {
        out,
        wrap_width: opts.wrap_width,
        frames: Vec::new(),
        word: String::new(),
        col: 0,
        line_open: false,
        wrote_any: false,
        pending_glue: false,
        pending_sep: Sep::None,
    };
    engine.run(flow)
}

/// Render a flow onto a single line, for places that must not contain one.
///
/// Table and matrix cells, the width measurement behind a heading underline, and the
/// lines of `\maketitle` all need a flow as one line of text: [`Glue`](FlowItem::Glue),
/// [`HardBreak`](FlowItem::HardBreak), [`ParagraphBreak`](FlowItem::ParagraphBreak) and
/// block boundaries all become a single space, block prefixes are ignored, verbatim
/// contents go in raw with their newlines turned into spaces, and the result is
/// trimmed.
///
/// ```
/// use techxt::flow::{Flow, FlowItem};
/// use techxt::layout::render_inline;
///
/// let mut flow = Flow::from_plain_text(" a b ");
/// flow.push(FlowItem::ParagraphBreak);
/// flow.extend(Flow::text("c"));
/// assert_eq!(render_inline(&flow), "a b c");
/// ```
pub fn render_inline(flow: &Flow) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for item in flow.items() {
        match item {
            FlowItem::Text(text) | FlowItem::InlineVerbatim(text) => {
                push_inline_word(&mut out, &mut pending_space, text);
            }
            FlowItem::MathAtom(atom) => {
                push_inline_word(&mut out, &mut pending_space, &atom.flatten());
            }
            FlowItem::Glue
            | FlowItem::HardBreak
            | FlowItem::ParagraphBreak
            | FlowItem::BlockStart(_)
            | FlowItem::BlockEnd => pending_space = true,
            FlowItem::Verbatim(payload) => {
                if !payload.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    // raw, except that the newlines cannot survive on one line
                    out.extend(payload.chars().map(|c| if c == '\n' { ' ' } else { c }));
                    pending_space = true;
                }
            }
            FlowItem::CellSep | FlowItem::RowSep | FlowItem::RuleMark => {
                stray_table_marker("a table marker");
                pending_space = true;
            }
        }
    }
    let trimmed = out.trim();
    if trimmed.len() == out.len() {
        out
    } else {
        String::from(trimmed)
    }
}

/// Append one word to an inline rendering, honouring the pending separator.
fn push_inline_word(out: &mut String, pending_space: &mut bool, word: &str) {
    if word.is_empty() {
        return;
    }
    if *pending_space && !out.is_empty() {
        out.push(' ');
    }
    *pending_space = false;
    out.push_str(word);
}

/// Report a table marker that reached layout, which means the enclosing table or
/// matrix handler failed to consume it (PLAN.md §6: they never reach layout in correct
/// operation).
///
/// The plan prescribes exactly this: a `debug_assert!(false)`, so that the handler bug
/// surfaces loudly while it is being written, plus a defined fallback in release builds
/// so that a shipped conversion degrades to slightly wrong spacing instead of losing
/// content or panicking on a document.
//
// The `#[allow]` is deliberate and cannot be avoided by rewriting: the assertion's
// condition *is* the constant `false`, that being the whole point. Clippy's suggested
// replacement, `unreachable!()`, would be wrong — this branch is reachable, and its
// release behaviour is specified.
#[allow(
    clippy::assertions_on_constants,
    reason = "PLAN.md §6 prescribes debug_assert!(false) with a defined release fallback"
)]
#[inline]
fn stray_table_marker(marker: &str) {
    debug_assert!(
        false,
        "{marker} reached the layout engine: the enclosing table or matrix handler \
         must consume CellSep/RowSep/RuleMark before layout"
    );
}

/// How strongly something wants to be separated from what precedes it.
///
/// Ordered, because merging requests is `max`: two paragraph breaks are one blank
/// line, and a block boundary next to a paragraph break is one blank line too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sep {
    /// Stay on the current line.
    None,
    /// Continue on the next line.
    Line,
    /// Continue after one blank line.
    Blank,
}

/// One open block on the indent stack.
struct Frame {
    /// Prefix for the block's first line.
    first: Box<str>,
    /// Prefix for every line after that.
    cont: Box<str>,
    /// Whether this is an [`BlockKind::Item`], which auto-closes a sibling item.
    is_item: bool,
    /// Whether a line has already been opened while this block was on the stack, i.e.
    /// whether `first` has been spent.
    first_used: bool,
}

/// The single-pass renderer. See the [module documentation](self) for the algorithm.
struct Engine<'a> {
    out: &'a mut dyn Write,
    wrap_width: Option<usize>,
    frames: Vec<Frame>,
    /// The word being accumulated from adjacent `Text`/`InlineVerbatim` items.
    word: String,
    /// Display width already occupied on the current line, prefix included.
    col: usize,
    /// Whether the current line has been started (its prefix written).
    line_open: bool,
    /// Whether anything at all has been written. Separation requests before the first
    /// content are dropped, which is what keeps leading blank lines out.
    wrote_any: bool,
    pending_glue: bool,
    pending_sep: Sep,
}

impl Engine<'_> {
    fn run(&mut self, flow: &Flow) -> core::fmt::Result {
        for item in flow.items() {
            self.item(item)?;
        }
        self.finish()
    }

    fn item(&mut self, item: &FlowItem) -> core::fmt::Result {
        match item {
            // A word is built up from adjacent items so that the wrap decision sees
            // its true width: `\textbf{bold}text` must be measured as "boldtext".
            FlowItem::Text(text) | FlowItem::InlineVerbatim(text) => {
                self.word.push_str(text);
                Ok(())
            }
            FlowItem::MathAtom(atom) => {
                // Should never happen: the math-group handler resolves its atoms.
                // Falling back to the flattened text loses the joiner's spacing but
                // keeps the content, which beats dropping the formula.
                let text = atom.flatten();
                self.word.push_str(&text);
                Ok(())
            }
            FlowItem::Glue => {
                self.flush_word()?;
                self.pending_glue = true;
                Ok(())
            }
            FlowItem::HardBreak => {
                self.flush_word()?;
                self.request_sep(Sep::Line);
                Ok(())
            }
            FlowItem::ParagraphBreak => {
                self.flush_word()?;
                self.request_sep(Sep::Blank);
                Ok(())
            }
            FlowItem::BlockStart(kind) => {
                self.flush_word()?;
                let (first, cont, is_item) = match kind {
                    BlockKind::Indent { first, cont } => (first, cont, false),
                    BlockKind::Item { first, cont } => (first, cont, true),
                };
                // A new item at the same depth closes the previous one, so `\item`
                // never has to look ahead to find where its own item ends.
                if is_item && self.frames.last().is_some_and(|f| f.is_item) {
                    self.frames.pop();
                }
                self.request_sep(Sep::Line);
                self.frames.push(Frame {
                    first: first.clone(),
                    cont: cont.clone(),
                    is_item,
                    first_used: false,
                });
                Ok(())
            }
            FlowItem::BlockEnd => {
                self.flush_word()?;
                // An unmatched BlockEnd means a handler is unbalanced; dropping it is
                // the harmless reading, and layout must not panic on any flow.
                self.frames.pop();
                self.request_sep(Sep::Line);
                Ok(())
            }
            FlowItem::Verbatim(payload) => {
                if payload.is_empty() {
                    // Nothing to emit, and nothing to separate from: an empty
                    // verbatim block must not even interrupt the word around it.
                    return Ok(());
                }
                self.flush_word()?;
                self.verbatim(payload)
            }
            FlowItem::CellSep => {
                stray_table_marker("CellSep");
                self.flush_word()?;
                self.pending_glue = true;
                Ok(())
            }
            FlowItem::RowSep => {
                stray_table_marker("RowSep");
                self.flush_word()?;
                self.request_sep(Sep::Line);
                Ok(())
            }
            FlowItem::RuleMark => {
                stray_table_marker("RuleMark");
                self.flush_word()?;
                self.request_sep(Sep::Line);
                Ok(())
            }
        }
    }

    /// Emit the accumulated word, if any.
    fn flush_word(&mut self) -> core::fmt::Result {
        if self.word.is_empty() {
            return Ok(());
        }
        // Take the buffer out so that `emit_word` can borrow `self` mutably, then put
        // it back to keep its capacity for the next word.
        let word = core::mem::take(&mut self.word);
        let result = self.emit_word(&word);
        self.word = word;
        self.word.clear();
        result
    }

    /// Write one unbreakable word, breaking the line first if it does not fit.
    fn emit_word(&mut self, word: &str) -> core::fmt::Result {
        if word.is_empty() {
            return Ok(());
        }
        self.flush_sep()?;
        let width = display_width(word);
        if !self.line_open {
            // Start of the document, or right after a break: glue never survives to
            // the start of a line.
            self.open_line()?;
            self.pending_glue = false;
        } else if self.pending_glue {
            self.pending_glue = false;
            // The `+ 1` is the space the glue would become; wrapping happens exactly
            // when writing it plus the word would overrun.
            let wrap = matches!(self.wrap_width, Some(w) if self.col + 1 + width > w);
            if wrap {
                self.out.write_str("\n")?;
                self.line_open = false;
                self.col = 0;
                self.open_line()?;
            } else {
                self.out.write_str(" ")?;
                self.col += 1;
            }
        }
        self.out.write_str(word)?;
        self.col += width;
        self.wrote_any = true;
        Ok(())
    }

    /// Emit a verbatim block: raw lines under the continuation indent, blank-line
    /// separated from its surroundings.
    fn verbatim(&mut self, payload: &str) -> core::fmt::Result {
        debug_assert!(!payload.is_empty());
        self.request_sep(Sep::Blank);
        self.flush_sep()?;
        // A trailing newline terminates the last line rather than starting an empty
        // one; every other newline separates two lines of the payload.
        let body = payload.strip_suffix('\n').unwrap_or(payload);
        for line in body.split('\n') {
            if !line.is_empty() {
                // Only the continuation indent: a block's `first` prefix (a list
                // bullet, say) belongs on a line of text, not in front of preformatted
                // content, and it stays available for the block's next real line.
                let Engine { out, frames, .. } = &mut *self;
                for frame in frames.iter() {
                    out.write_str(&frame.cont)?;
                }
                self.out.write_str(line)?;
            }
            self.out.write_str("\n")?;
        }
        self.line_open = false;
        self.col = 0;
        self.wrote_any = true;
        self.request_sep(Sep::Blank);
        Ok(())
    }

    /// Record a request for vertical separation, merging it with any pending one.
    fn request_sep(&mut self, sep: Sep) {
        // Glue never survives across a break: no line ends in whitespace.
        self.pending_glue = false;
        if !self.wrote_any {
            // Nothing to separate from yet, so no leading blank lines.
            return;
        }
        if sep > self.pending_sep {
            self.pending_sep = sep;
        }
    }

    /// Act on the pending separation request, if any, leaving the cursor at the start
    /// of a fresh line.
    fn flush_sep(&mut self) -> core::fmt::Result {
        if self.pending_sep == Sep::None {
            return Ok(());
        }
        if self.line_open {
            self.out.write_str("\n")?;
            self.line_open = false;
            self.col = 0;
        }
        if self.pending_sep == Sep::Blank {
            self.out.write_str("\n")?;
        }
        self.pending_sep = Sep::None;
        self.pending_glue = false;
        Ok(())
    }

    /// Start a line by writing its prefix, and account for the prefix's width.
    fn open_line(&mut self) -> core::fmt::Result {
        debug_assert!(!self.line_open);
        {
            let Engine {
                out, frames, col, ..
            } = &mut *self;
            let innermost = frames.len().saturating_sub(1);
            let mut width = 0;
            for (i, frame) in frames.iter().enumerate() {
                let prefix: &str = if i == innermost && !frame.first_used {
                    &frame.first
                } else {
                    &frame.cont
                };
                out.write_str(prefix)?;
                width += display_width(prefix);
            }
            *col = width;
            // Every open block has now produced a line, so each has spent its `first`.
            for frame in frames.iter_mut() {
                frame.first_used = true;
            }
        }
        self.line_open = true;
        Ok(())
    }

    /// Terminate the output: one trailing newline if anything was written, and no
    /// trailing blank line (a pending separation request simply expires).
    fn finish(&mut self) -> core::fmt::Result {
        self.flush_word()?;
        if self.line_open {
            self.out.write_str("\n")?;
            self.line_open = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowItem::*;
    use crate::mathfmt::{Atom, AtomBody, AtomClass};
    use alloc::vec;

    fn flow(items: Vec<FlowItem>) -> Flow {
        let mut f = Flow::new();
        for item in items {
            f.push(item);
        }
        f
    }

    fn plain(items: Vec<FlowItem>) -> String {
        render(&flow(items), &LayoutOptions::default())
    }

    fn wrapped(width: usize, items: Vec<FlowItem>) -> String {
        render(
            &flow(items),
            &LayoutOptions {
                wrap_width: Some(width),
            },
        )
    }

    fn t(s: &str) -> FlowItem {
        Text(s.into())
    }

    fn indent(first: &str, cont: &str) -> FlowItem {
        BlockStart(BlockKind::Indent {
            first: first.into(),
            cont: cont.into(),
        })
    }

    fn item(first: &str, cont: &str) -> FlowItem {
        BlockStart(BlockKind::Item {
            first: first.into(),
            cont: cont.into(),
        })
    }

    // ----- words and glue -------------------------------------------------

    #[test]
    fn empty_flow_renders_nothing() {
        assert_eq!(plain(vec![]), "");
        assert_eq!(wrapped(10, vec![]), "");
        // an empty word is not content either
        assert_eq!(plain(vec![t("")]), "");
        assert_eq!(plain(vec![Glue, t(""), Glue]), "");
    }

    #[test]
    fn words_are_separated_by_one_space() {
        assert_eq!(plain(vec![t("a"), Glue, t("b")]), "a b\n");
    }

    #[test]
    fn glue_collapses() {
        assert_eq!(plain(vec![t("a"), Glue, Glue, Glue, t("b")]), "a b\n");
    }

    #[test]
    fn glue_at_the_edges_is_dropped() {
        assert_eq!(plain(vec![Glue, t("a"), Glue]), "a\n");
        assert_eq!(
            plain(vec![Glue, Glue, t("a"), HardBreak, Glue, t("b"), Glue]),
            "a\nb\n"
        );
    }

    #[test]
    fn adjacent_text_items_are_one_word() {
        // PLAN.md §15 #4: `\textbf{bold}text` is one unbreakable word.
        assert_eq!(plain(vec![t("bold"), t("text")]), "boldtext\n");
        // and it is measured as one word when deciding to wrap
        assert_eq!(
            wrapped(10, vec![t("aa"), Glue, t("bold"), t("text")]),
            "aa\nboldtext\n"
        );
        // (were they measured separately, "aa bold" would have fit)
    }

    #[test]
    fn inline_verbatim_is_an_unbreakable_word() {
        assert_eq!(plain(vec![t("a"), InlineVerbatim("x_1".into())]), "ax_1\n");
        assert_eq!(
            wrapped(6, vec![t("aa"), Glue, InlineVerbatim("b  c".into())]),
            "aa\nb  c\n"
        );
    }

    #[test]
    fn math_atom_falls_back_to_flattened_text() {
        let atom = Atom {
            body: AtomBody::Str("𝑥²".into()),
            cls: (AtomClass::Ord, AtomClass::Ord),
            script: None,
        };
        assert_eq!(plain(vec![t("a"), Glue, MathAtom(atom)]), "a 𝑥²\n");
    }

    // ----- wrapping -------------------------------------------------------

    #[test]
    fn wrapping_breaks_at_glue() {
        assert_eq!(
            wrapped(
                11,
                vec![t("aaa"), Glue, t("bbb"), Glue, t("ccc"), Glue, t("ddd")]
            ),
            "aaa bbb ccc\nddd\n"
        );
    }

    #[test]
    fn wrapping_crosses_item_boundaries() {
        // PLAN.md §15 #25: `aaa bbb \textbf{ccc ddd} eee` at wrap_width 12 wraps
        // *inside* the macro, because glue decides breaks, not item boundaries.
        assert_eq!(
            wrapped(
                12,
                vec![
                    t("aaa"),
                    Glue,
                    t("bbb"),
                    Glue,
                    t("ccc"),
                    Glue,
                    t("ddd"),
                    Glue,
                    t("eee")
                ]
            ),
            "aaa bbb ccc\nddd eee\n"
        );
    }

    #[test]
    fn no_wrap_is_the_same_pass_without_the_width_check() {
        let items = vec![
            t("aaa"),
            Glue,
            t("bbb"),
            Glue,
            t("ccc"),
            Glue,
            t("ddd"),
            Glue,
            t("eee"),
        ];
        assert_eq!(plain(items), "aaa bbb ccc ddd eee\n");
    }

    #[test]
    fn an_oversized_word_overflows_rather_than_splitting() {
        assert_eq!(wrapped(3, vec![t("aaaaa"), Glue, t("bb")]), "aaaaa\nbb\n");
        assert_eq!(
            wrapped(5, vec![t("a"), Glue, t("https://example.org/very/long")]),
            "a\nhttps://example.org/very/long\n"
        );
    }

    #[test]
    fn wrapping_uses_display_width_not_byte_length() {
        // four bold letters are four columns wide but sixteen bytes long
        assert_eq!(wrapped(9, vec![t("aaaa"), Glue, t("𝐛𝐨𝐥𝐝")]), "aaaa 𝐛𝐨𝐥𝐝\n");
        assert_eq!(wrapped(8, vec![t("aaaa"), Glue, t("𝐛𝐨𝐥𝐝")]), "aaaa\n𝐛𝐨𝐥𝐝\n");
    }

    #[test]
    fn a_word_that_exactly_fits_stays_on_the_line() {
        assert_eq!(wrapped(7, vec![t("aaa"), Glue, t("bbb")]), "aaa bbb\n");
        assert_eq!(wrapped(6, vec![t("aaa"), Glue, t("bbb")]), "aaa\nbbb\n");
    }

    // ----- vertical separation --------------------------------------------

    #[test]
    fn hard_break_starts_a_new_line() {
        assert_eq!(plain(vec![t("a"), HardBreak, t("b")]), "a\nb\n");
    }

    #[test]
    fn paragraph_break_leaves_one_blank_line() {
        assert_eq!(plain(vec![t("a"), ParagraphBreak, t("b")]), "a\n\nb\n");
    }

    #[test]
    fn separation_requests_merge_and_never_sum() {
        assert_eq!(
            plain(vec![
                t("a"),
                ParagraphBreak,
                ParagraphBreak,
                ParagraphBreak,
                t("b")
            ]),
            "a\n\nb\n"
        );
        assert_eq!(
            plain(vec![t("a"), HardBreak, ParagraphBreak, HardBreak, t("b")]),
            "a\n\nb\n"
        );
        assert_eq!(
            plain(vec![t("a"), ParagraphBreak, Glue, ParagraphBreak, t("b")]),
            "a\n\nb\n"
        );
    }

    #[test]
    fn example_2_four_newlines_are_one_blank_line() {
        // PLAN.md §15 #2
        let f = Flow::from_plain_text("one\n\n\n\ntwo");
        assert_eq!(render(&f, &LayoutOptions::default()), "one\n\ntwo\n");
    }

    #[test]
    fn no_leading_or_trailing_blank_lines() {
        assert_eq!(
            plain(vec![
                ParagraphBreak,
                HardBreak,
                t("a"),
                ParagraphBreak,
                HardBreak
            ]),
            "a\n"
        );
        assert_eq!(plain(vec![ParagraphBreak, ParagraphBreak]), "");
    }

    #[test]
    fn output_ends_with_exactly_one_newline() {
        assert_eq!(plain(vec![t("a")]), "a\n");
        assert_eq!(plain(vec![t("a"), HardBreak]), "a\n");
        assert_eq!(plain(vec![Verbatim("v\n".into())]), "v\n");
    }

    // ----- blocks ---------------------------------------------------------

    #[test]
    fn a_block_prefixes_its_first_and_later_lines() {
        assert_eq!(
            plain(vec![
                indent("> ", "| "),
                t("a"),
                HardBreak,
                t("b"),
                BlockEnd,
                t("c")
            ]),
            "> a\n| b\nc\n"
        );
    }

    #[test]
    fn block_prefix_width_counts_toward_the_wrap_column() {
        assert_eq!(
            wrapped(
                8,
                vec![indent("* ", "  "), t("aaa"), Glue, t("bbb"), Glue, t("ccc")]
            ),
            "* aaa\n  bbb\n  ccc\n"
        );
        // without the prefix the same words would have fitted two to a line
        assert_eq!(
            wrapped(8, vec![t("aaa"), Glue, t("bbb"), Glue, t("ccc")]),
            "aaa bbb\nccc\n"
        );
    }

    #[test]
    fn nested_blocks_stack_their_prefixes() {
        assert_eq!(
            plain(vec![
                indent("", "  "),
                t("a"),
                indent("> ", "| "),
                t("b"),
                HardBreak,
                t("c"),
                BlockEnd,
                t("d"),
                BlockEnd,
            ]),
            "a\n  > b\n  | c\n  d\n"
        );
    }

    #[test]
    fn a_block_boundary_separates_but_does_not_blank() {
        assert_eq!(
            plain(vec![t("a"), indent("", ""), t("b"), BlockEnd, t("c")]),
            "a\nb\nc\n"
        );
        // a handler that wants a blank line asks for one, and merging keeps it to one
        assert_eq!(
            plain(vec![
                t("a"),
                ParagraphBreak,
                indent("", ""),
                t("b"),
                BlockEnd,
                t("c")
            ]),
            "a\n\nb\nc\n"
        );
    }

    #[test]
    fn empty_blocks_produce_nothing() {
        assert_eq!(
            plain(vec![t("a"), indent("> ", "| "), BlockEnd, t("b")]),
            "a\nb\n"
        );
        assert_eq!(plain(vec![indent("> ", "| "), BlockEnd]), "");
    }

    #[test]
    fn an_unmatched_block_end_is_ignored() {
        assert_eq!(
            plain(vec![BlockEnd, t("a"), BlockEnd, BlockEnd, t("b")]),
            "a\nb\n"
        );
    }

    #[test]
    fn an_item_auto_closes_the_previous_item() {
        assert_eq!(
            plain(vec![
                item("1. ", "   "),
                t("x"),
                item("2. ", "   "),
                t("y"),
                BlockEnd
            ]),
            "1. x\n2. y\n"
        );
    }

    #[test]
    fn example_22_nested_lists() {
        // PLAN.md §15 #22, with the list environments opening a block of their own so
        // that the inner list's first item does not close the outer list's item.
        assert_eq!(
            plain(vec![
                indent("", ""), // itemize
                item("  • ", "    "),
                t("one"),
                indent("", ""), // enumerate, inside the outer item
                item("1. ", "   "),
                t("x"),
                item("2. ", "   "),
                t("y"),
                BlockEnd, // closes item "2."
                BlockEnd, // closes enumerate
                item("  • ", "    "),
                t("two"),
                BlockEnd, // closes item "two"
                BlockEnd, // closes itemize
            ]),
            "  • one\n    1. x\n    2. y\n  • two\n"
        );
    }

    #[test]
    fn a_block_first_prefix_is_used_once() {
        assert_eq!(
            plain(vec![
                item("- ", "  "),
                t("a"),
                Glue,
                t("b"),
                HardBreak,
                t("c"),
                BlockEnd
            ]),
            "- a b\n  c\n"
        );
    }

    // ----- verbatim -------------------------------------------------------

    #[test]
    fn verbatim_is_preserved_byte_for_byte() {
        // PLAN.md §15 #24: body byte-identical, blank-line separated, never wrapped.
        let body = "  keep   this\n\n    exactly\n";
        assert_eq!(
            wrapped(6, vec![t("a"), Verbatim(body.into()), t("b")]),
            "a\n\n  keep   this\n\n    exactly\n\nb\n"
        );
    }

    #[test]
    fn verbatim_is_never_wrapped() {
        let body = "a very long preformatted line that must not be broken\n";
        let out = wrapped(10, vec![Verbatim(body.into())]);
        assert!(out.contains("a very long preformatted line that must not be broken"));
    }

    #[test]
    fn verbatim_lines_get_the_continuation_indent() {
        assert_eq!(
            plain(vec![
                indent("", "  "),
                t("a"),
                Verbatim("x\n\ny\n".into()),
                t("b"),
                BlockEnd
            ]),
            "a\n\n  x\n\n  y\n\n  b\n"
        );
    }

    #[test]
    fn empty_verbatim_lines_carry_no_indent() {
        let out = plain(vec![
            indent("", "    "),
            Verbatim("x\n\ny".into()),
            BlockEnd,
        ]);
        assert_eq!(out, "    x\n\n    y\n");
        for line in out.lines() {
            assert_eq!(line.trim_end(), line, "trailing whitespace in {line:?}");
        }
    }

    #[test]
    fn an_empty_verbatim_is_a_no_op() {
        // not even a word boundary: "a" and "b" stay one unbreakable word
        assert_eq!(plain(vec![t("a"), Verbatim("".into()), t("b")]), "ab\n");
        assert_eq!(
            plain(vec![t("a"), Glue, Verbatim("".into()), t("b")]),
            "a b\n"
        );
    }

    #[test]
    fn verbatim_at_the_document_edges_needs_no_blank_lines() {
        assert_eq!(plain(vec![Verbatim("x\n".into())]), "x\n");
        assert_eq!(plain(vec![Verbatim("x".into())]), "x\n");
        assert_eq!(
            plain(vec![Verbatim("x".into()), Verbatim("y".into())]),
            "x\n\ny\n"
        );
    }

    // ----- render_inline --------------------------------------------------

    #[test]
    fn render_inline_flattens_everything_to_one_line() {
        assert_eq!(render_inline(&flow(vec![])), "");
        assert_eq!(
            render_inline(&flow(vec![Glue, t("a"), Glue, Glue, t("b"), Glue])),
            "a b"
        );
        assert_eq!(
            render_inline(&flow(vec![
                t("a"),
                HardBreak,
                t("b"),
                ParagraphBreak,
                t("c")
            ])),
            "a b c"
        );
        assert_eq!(render_inline(&flow(vec![t("bold"), t("text")])), "boldtext");
    }

    #[test]
    fn render_inline_ignores_block_prefixes() {
        assert_eq!(
            render_inline(&flow(vec![
                t("a"),
                indent("> ", "| "),
                t("b"),
                BlockEnd,
                t("c")
            ])),
            "a b c"
        );
    }

    #[test]
    fn render_inline_squeezes_verbatim_onto_the_line() {
        assert_eq!(
            render_inline(&flow(vec![t("a"), Verbatim("x  y\nz\n".into()), t("b")])),
            "a x  y z  b"
        );
        assert_eq!(
            render_inline(&flow(vec![InlineVerbatim(" x ".into())])),
            "x"
        );
    }

    #[test]
    fn render_inline_flattens_math_atoms() {
        let atom = Atom {
            body: AtomBody::Wrappable {
                inner: "a/b".into(),
                prefix: "".into(),
            },
            cls: (AtomClass::Ord, AtomClass::OpenEnd),
            script: None,
        };
        assert_eq!(
            render_inline(&flow(vec![t("x"), Glue, MathAtom(atom)])),
            "x a/b"
        );
    }

    // ----- writer plumbing ------------------------------------------------

    #[test]
    fn render_to_matches_render() {
        let f = flow(vec![t("a"), ParagraphBreak, t("b")]);
        let opts = LayoutOptions::default();
        let mut out = String::new();
        render_to(&f, &opts, &mut out).expect("String's fmt::Write cannot fail");
        assert_eq!(out, render(&f, &opts));
    }

    #[test]
    fn render_to_propagates_writer_errors() {
        struct Failing;
        impl Write for Failing {
            fn write_str(&mut self, _s: &str) -> core::fmt::Result {
                Err(core::fmt::Error)
            }
        }
        let f = flow(vec![t("a")]);
        assert!(render_to(&f, &LayoutOptions::default(), &mut Failing).is_err());
    }

    /// The `debug_assert!` fires in debug builds, so the documented release-mode
    /// fallback can only be observed with `cargo test --release`.
    #[cfg(not(debug_assertions))]
    #[test]
    fn stray_table_markers_fall_back_in_release() {
        assert_eq!(plain(vec![t("a"), CellSep, t("b")]), "a b\n");
        assert_eq!(plain(vec![t("a"), RowSep, t("b")]), "a\nb\n");
        assert_eq!(plain(vec![t("a"), RuleMark, t("b")]), "a\nb\n");
        assert_eq!(render_inline(&flow(vec![t("a"), CellSep, t("b")])), "a b");
    }
}
