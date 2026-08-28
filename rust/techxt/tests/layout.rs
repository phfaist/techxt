//! Layout behaviour that the plan states as law, exercised through the public API.
//!
//! These are the normative acceptance examples of PLAN.md §15 that the flow and layout
//! milestone can already answer, with the flows built by hand (no parser needed), plus
//! the composition scenarios that only show up once several rules meet.

use techxt::flow::{BlockKind, Flow, FlowItem, VerbatimProvenance};
use techxt::layout::{render, render_inline, render_to, LayoutOptions};

/// Build a flow from a list of items.
fn flow(items: Vec<FlowItem>) -> Flow {
    let mut f = Flow::new();
    for item in items {
        f.push(item);
    }
    f
}

fn text(s: &str) -> FlowItem {
    FlowItem::Text(s.into())
}

/// `LayoutOptions` is `#[non_exhaustive]`, so a downstream crate — which is what an
/// integration test is — has to build it exactly like this.
fn wrap_at(width: usize) -> LayoutOptions {
    let mut opts = LayoutOptions::default();
    opts.wrap_width = Some(width);
    opts
}

fn no_wrap() -> LayoutOptions {
    LayoutOptions::default()
}

// ---------------------------------------------------------------------------
// PLAN.md §15 acceptance examples
// ---------------------------------------------------------------------------

/// §15 #2: `one\n\n\n\ntwo` → `one` / blank / `two`.
///
/// The source has three blank lines' worth of newlines; the flow records one
/// paragraph break and layout renders one blank line. (The trailing newline is
/// §7's "output ends with exactly one `\n`".)
#[test]
fn example_2_blank_lines_normalize() {
    let f = Flow::from_plain_text("one\n\n\n\ntwo");
    assert_eq!(render(&f, &no_wrap()), "one\n\ntwo\n");
    assert_eq!(render(&f, &wrap_at(40)), "one\n\ntwo\n");
}

/// §15 #4: `\textbf{bold}text` is one unbreakable word.
///
/// The bold macro contributes its own `Text` item and the following characters
/// another; nothing may fall between them, at any width.
#[test]
fn example_4_adjacent_text_never_breaks() {
    let f = flow(vec![text("𝐛𝐨𝐥𝐝"), text("text")]);
    assert_eq!(render(&f, &no_wrap()), "𝐛𝐨𝐥𝐝text\n");
    assert_eq!(render(&f, &wrap_at(4)), "𝐛𝐨𝐥𝐝text\n");
    assert_eq!(render(&f, &wrap_at(1)), "𝐛𝐨𝐥𝐝text\n");
}

/// §15 #22: nested lists come out tight, with the inner list's markers indented under
/// the outer item's text.
///
/// The list environments open a block of their own around their items, which is what
/// keeps the inner list's first `\item` from auto-closing the outer list's item.
#[test]
fn example_22_nested_lists() {
    fn item(first: &str, cont: &str) -> FlowItem {
        FlowItem::BlockStart(BlockKind::Item {
            first: first.into(),
            cont: cont.into(),
        })
    }
    fn list() -> FlowItem {
        FlowItem::BlockStart(BlockKind::Indent {
            first: "".into(),
            cont: "".into(),
        })
    }

    let f = flow(vec![
        list(),
        item("  • ", "    "),
        text("one"),
        list(),
        item("1. ", "   "),
        text("x"),
        item("2. ", "   "),
        text("y"),
        FlowItem::BlockEnd,
        FlowItem::BlockEnd,
        item("  • ", "    "),
        text("two"),
        FlowItem::BlockEnd,
        FlowItem::BlockEnd,
    ]);
    assert_eq!(
        render(&f, &no_wrap()),
        "  • one\n    1. x\n    2. y\n  • two\n"
    );
}

/// §15 #24: a verbatim body survives byte for byte, is blank-line separated from its
/// surroundings, and is never wrapped.
#[test]
fn example_24_verbatim_is_preserved() {
    let body = "  keep   this\n\n    exactly\n";
    let f = flow(vec![
        text("before"),
        FlowItem::Verbatim {
            text: body.into(),
            provenance: VerbatimProvenance::Verbatim,
        },
        text("after"),
    ]);
    let expected = "before\n\n  keep   this\n\n    exactly\n\nafter\n";
    assert_eq!(render(&f, &no_wrap()), expected);
    // a wrap width narrower than the content changes nothing about the block
    assert_eq!(render(&f, &wrap_at(5)), expected);
    assert!(render(&f, &wrap_at(5)).contains(body));
}

/// §15 #25: `aaa bbb \textbf{ccc ddd} eee` at `wrap_width = 12` wraps *inside* the
/// macro, because glue decides breaks and item boundaries do not.
#[test]
fn example_25_wraps_across_a_macro_boundary() {
    let f = flow(vec![
        text("aaa"),
        FlowItem::Glue,
        text("bbb"),
        FlowItem::Glue,
        // \textbf{ccc ddd} — its own two words, contributed by a different handler
        text("ccc"),
        FlowItem::Glue,
        text("ddd"),
        FlowItem::Glue,
        text("eee"),
    ]);
    assert_eq!(render(&f, &wrap_at(12)), "aaa bbb ccc\nddd eee\n");
    // the same flow, same pass, no width check
    assert_eq!(render(&f, &no_wrap()), "aaa bbb ccc ddd eee\n");
}

// ---------------------------------------------------------------------------
// composition scenarios
// ---------------------------------------------------------------------------

/// A display-math-shaped block: indented, blank-line separated because the handler
/// asked for it, and never wrapped away from its indent.
#[test]
fn indented_block_between_paragraphs() {
    let mut f = Flow::from_plain_text("Consider the identity");
    f.push(FlowItem::ParagraphBreak);
    f.push(FlowItem::BlockStart(BlockKind::Indent {
        first: "    ".into(),
        cont: "    ".into(),
    }));
    f.extend(Flow::text("𝐸"));
    f.extend(Flow::glue());
    f.extend(Flow::text("="));
    f.extend(Flow::glue());
    f.extend(Flow::text("𝑚𝑐²"));
    f.push(FlowItem::BlockEnd);
    f.push(FlowItem::ParagraphBreak);
    f.extend(Flow::from_plain_text("which is well known."));

    assert_eq!(
        render(&f, &no_wrap()),
        "Consider the identity\n\n    𝐸 = 𝑚𝑐²\n\nwhich is well known.\n"
    );
    // narrow enough that the indent itself forces the equation onto two lines
    assert_eq!(
        render(&f, &wrap_at(10)),
        "Consider\nthe\nidentity\n\n    𝐸 =\n    𝑚𝑐²\n\nwhich is\nwell\nknown.\n"
    );
}

/// Whatever the flow, the output never carries trailing whitespace and never runs to
/// two blank lines.
#[test]
fn no_trailing_whitespace_and_at_most_one_blank_line() {
    let f = flow(vec![
        FlowItem::ParagraphBreak,
        FlowItem::Glue,
        text("a"),
        FlowItem::Glue,
        FlowItem::ParagraphBreak,
        FlowItem::BlockStart(BlockKind::Indent {
            first: "> ".into(),
            cont: "> ".into(),
        }),
        FlowItem::BlockEnd,
        FlowItem::ParagraphBreak,
        FlowItem::Glue,
        text("b"),
        FlowItem::Glue,
        FlowItem::HardBreak,
        FlowItem::ParagraphBreak,
    ]);
    let out = render(&f, &wrap_at(3));
    assert_eq!(out, "a\n\nb\n");
    for line in out.lines() {
        assert_eq!(line.trim_end(), line);
    }
    assert!(!out.contains("\n\n\n"));
}

/// `render_to` writes what `render` returns, and `render_inline` squeezes the same flow
/// onto one line.
#[test]
fn the_three_entry_points_agree() {
    let f = flow(vec![
        text("a"),
        FlowItem::Glue,
        text("b"),
        FlowItem::ParagraphBreak,
        FlowItem::BlockStart(BlockKind::Indent {
            first: "  ".into(),
            cont: "  ".into(),
        }),
        text("c"),
        FlowItem::BlockEnd,
    ]);
    let opts = wrap_at(3);
    let mut out = String::new();
    render_to(&f, &opts, &mut out).expect("writing into a String cannot fail");
    assert_eq!(out, render(&f, &opts));
    assert_eq!(out, "a b\n\n  c\n");
    assert_eq!(render_inline(&f), "a b c");
}

/// An empty flow renders to nothing at all — not to a lone newline.
#[test]
fn empty_input_renders_empty() {
    assert_eq!(render(&Flow::new(), &no_wrap()), "");
    assert_eq!(render(&Flow::from_plain_text("   \n\n  "), &no_wrap()), "");
    assert_eq!(render_inline(&Flow::from_plain_text("   ")), "");
}
