//! Property tests for the layout invariants of PLAN.md §7 and §14.2.
//!
//! The plan states five guarantees the engine must hold for *every* flow:
//!
//! 1. no line exceeds `wrap_width`, unless it holds a single oversized word or
//!    verbatim content;
//! 2. no line carries trailing whitespace;
//! 3. two blank lines never follow one another;
//! 4. `Verbatim` payloads appear in the output byte for byte;
//! 5. the output is deterministic.
//!
//! Guarantees 1–3 are *layout* guarantees, and verbatim content is by construction
//! exempt from all three: it may be arbitrarily wide, it may end in spaces, and it may
//! contain any run of blank lines — that is what preserving it byte for byte means, and
//! guarantee 4 would contradict them otherwise. So those three are checked over flows
//! that carry no verbatim content, and guarantees 4 and 5 over flows that do.

use proptest::prelude::*;
use techxt::flow::{BlockKind, Flow, FlowItem};
use techxt::layout::{render, render_inline, render_to, LayoutOptions};

/// Characters words are built from: some ASCII, an accented letter, a wide CJK
/// character and a mathematical bold letter, so that display width and byte length
/// disagree. None of them is whitespace.
const WORD_CHARS: &[char] = &['a', 'b', 'c', 'X', '.', 'é', '漢', '𝐛'];

/// Block prefixes deliberately contain **no spaces**, so that a space in an output line
/// can only come from glue, i.e. can only mean "this line holds at least two words".
/// That is what makes the wrap-width property checkable without re-deriving the layout.
const PREFIXES: &[(&str, &str)] = &[("", ""), ("-", "|"), (">>", "||"), ("*", "")];

fn word() -> impl Strategy<Value = Box<str>> {
    prop::collection::vec(prop::sample::select(WORD_CHARS), 1..6)
        .prop_map(|chars| chars.into_iter().collect::<String>().into_boxed_str())
}

fn block_start() -> impl Strategy<Value = FlowItem> {
    (prop::sample::select(PREFIXES), any::<bool>()).prop_map(|((first, cont), is_item)| {
        let first = Box::<str>::from(first);
        let cont = Box::<str>::from(cont);
        FlowItem::BlockStart(if is_item {
            BlockKind::Item { first, cont }
        } else {
            BlockKind::Indent { first, cont }
        })
    })
}

/// Items that layout is allowed to normalize: everything except verbatim content.
fn layout_item() -> impl Strategy<Value = FlowItem> {
    prop_oneof![
        6 => word().prop_map(FlowItem::Text),
        6 => Just(FlowItem::Glue),
        2 => Just(FlowItem::HardBreak),
        2 => Just(FlowItem::ParagraphBreak),
        2 => block_start(),
        2 => Just(FlowItem::BlockEnd),
    ]
}

/// A verbatim payload: anything at all, including the trailing spaces and runs of blank
/// lines that layout must not touch.
fn verbatim_payload() -> impl Strategy<Value = Box<str>> {
    prop::collection::vec(
        prop::sample::select(&['a', 'b', ' ', '\t', '\n'][..]),
        0..12,
    )
    .prop_map(|chars| chars.into_iter().collect::<String>().into_boxed_str())
}

/// Items including verbatim content, for the preservation and determinism properties.
fn any_item() -> impl Strategy<Value = FlowItem> {
    prop_oneof![
        6 => word().prop_map(FlowItem::Text),
        6 => Just(FlowItem::Glue),
        2 => Just(FlowItem::HardBreak),
        2 => Just(FlowItem::ParagraphBreak),
        2 => word().prop_map(FlowItem::InlineVerbatim),
        3 => verbatim_payload().prop_map(FlowItem::Verbatim),
    ]
}

fn flow_of(item: impl Strategy<Value = FlowItem>) -> impl Strategy<Value = Flow> {
    prop::collection::vec(item, 0..30).prop_map(|items| {
        let mut flow = Flow::new();
        for item in items {
            flow.push(item);
        }
        flow
    })
}

fn options(wrap_width: Option<usize>) -> LayoutOptions {
    // `LayoutOptions` is `#[non_exhaustive]`, so a downstream crate builds it this way.
    let mut opts = LayoutOptions::default();
    opts.wrap_width = wrap_width;
    opts
}

/// Display width, measured the same way the engine measures it.
fn width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

proptest! {
    /// §7.1: a line may only exceed the wrap width when it holds a single word that
    /// does not fit — which, with space-free block prefixes, means a line without a
    /// space in it.
    #[test]
    fn wrap_width_is_respected(flow in flow_of(layout_item()), wrap in 1usize..24) {
        let out = render(&flow, &options(Some(wrap)));
        for line in out.lines() {
            if line.contains(' ') {
                prop_assert!(
                    width(line) <= wrap,
                    "line {line:?} of width {} exceeds wrap width {wrap}",
                    width(line)
                );
            }
        }
    }

    /// §7.2: glue never survives to the end of a line.
    #[test]
    fn no_line_ends_in_whitespace(
        flow in flow_of(layout_item()),
        wrap in prop::option::of(1usize..24),
    ) {
        let out = render(&flow, &options(wrap));
        for line in out.lines() {
            prop_assert_eq!(line.trim_end(), line, "trailing whitespace in {:?}", line);
        }
    }

    /// §7.3: separation requests merge, so at most one blank line appears anywhere, and
    /// none at either end; a nonempty output ends with exactly one newline.
    #[test]
    fn vertical_separation_is_normalized(
        flow in flow_of(layout_item()),
        wrap in prop::option::of(1usize..24),
    ) {
        let out = render(&flow, &options(wrap));
        prop_assert!(!out.contains("\n\n\n"), "two blank lines in {out:?}");
        prop_assert!(!out.starts_with('\n'), "leading blank line in {out:?}");
        if !out.is_empty() {
            prop_assert!(out.ends_with('\n'), "missing final newline in {out:?}");
            prop_assert!(!out.ends_with("\n\n"), "trailing blank line in {out:?}");
        }
    }

    /// §7.5: a `Verbatim` payload reaches the output byte for byte. Checked without
    /// blocks, so that no continuation indent is inserted between its lines; the
    /// trailing newline of a payload is its terminator, so it is the only byte that a
    /// payload may "lose".
    #[test]
    fn verbatim_payloads_are_preserved(flow in flow_of(any_item()), wrap in 1usize..12) {
        let out = render(&flow, &options(Some(wrap)));
        for item in flow.items() {
            if let FlowItem::Verbatim(payload) = item {
                let body = payload.strip_suffix('\n').unwrap_or(payload);
                if !body.is_empty() {
                    prop_assert!(
                        out.contains(body),
                        "verbatim payload {body:?} is not in {out:?}"
                    );
                }
            }
        }
    }

    /// §14.2: the same flow always renders to the same string, whichever entry point
    /// is used.
    #[test]
    fn rendering_is_deterministic(
        flow in flow_of(any_item()),
        wrap in prop::option::of(1usize..24),
    ) {
        let opts = options(wrap);
        let once = render(&flow, &opts);
        prop_assert_eq!(&once, &render(&flow, &opts));
        let mut written = String::new();
        render_to(&flow, &opts, &mut written).unwrap();
        prop_assert_eq!(&once, &written);
    }

    /// `render_inline` always produces exactly one trimmed line.
    #[test]
    fn inline_rendering_is_one_trimmed_line(flow in flow_of(any_item())) {
        let out = render_inline(&flow);
        prop_assert!(!out.contains('\n'), "newline in inline rendering {out:?}");
        prop_assert_eq!(out.trim(), &out, "untrimmed inline rendering");
    }
}
