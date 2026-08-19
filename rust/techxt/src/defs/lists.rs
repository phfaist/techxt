//! List environments (PLAN.md §9.4): `itemize`, `enumerate`, `description`, `list` and
//! `trivlist`, and the `\item` that lives inside them.
//!
//! `\item` is brought into scope by each list environment's body rather than defined
//! globally, which is what [`EnvDef::list_body`](crate::def::EnvDef::list_body) is for:
//! an `\item` outside a list is a mistake worth reporting, not a construct.
//!
//! # How a list becomes text
//!
//! Two nesting depths decide what a list looks like, and they are deliberately
//! different (PLAN.md §8's [`ListCtx`]):
//!
//! - [`same_kind_depth`](ListCtx::same_kind_depth) picks the *marker*, cycling through
//!   the arrays in [`Options::list_style`](crate::Options::list_style). It counts only
//!   the enclosing lists of the **same kind**, exactly as LaTeX's own `\@itemdepth` and
//!   `\@enumdepth` do: an `enumerate` directly inside an `itemize` numbers from `1.`
//!   rather than continuing into `(a)` (PLAN.md §15 example 22), while an `itemize`
//!   inside an `enumerate` inside an `itemize` is a *second*-level itemize, because the
//!   list of another kind in between does not reset anything.
//! - [`depth`](ListCtx::depth) decides the indentation. Only the outermost list indents
//!   its contents (by two spaces); every deeper list is already offset by the hanging
//!   indent of the item it appears in, and indenting again would stack up fast.
//!
//! An item is a [hanging-indent block](crate::flow::BlockKind::Item), so the marker
//! goes on the item's first line and every further line lines up with the text after
//! it. The item's *contents* are not an argument of `\item` — they are simply the nodes
//! that follow it — so `\item` emits only the block's opening, and the layout engine
//! closes an item when the next one at the same depth begins.
//!
//! # Why a nested list opens a block of its own
//!
//! That auto-close is why a list wraps its items in a block even when it adds no
//! indentation: without one, the first `\item` of a nested list would sit at the same
//! block depth as the item it is written inside, and would close it — collapsing the
//! nesting the reader is supposed to see. The wrapping block is what keeps the inner
//! list's markers under the outer item's text.

use alloc::string::String;
use alloc::vec::Vec;

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::convert::{CounterStyle, CounterWrap, EnumFormat};
use crate::def::{Category, EnvDef, MacroDef, TextHandler};
use crate::diag::{StrayItem, UnsupportedIgnored};
use crate::flow::{display_width, BlockKind, Flow, FlowItem};
use crate::render::{ListCtx, ListKind, RenderCx, RenderError};

use super::handler;

/// The marker a stray `\item` — one outside any list — is rendered with.
///
/// Generic on purpose: there is no list to take a style from, and dropping the item
/// would lose the text that follows it.
const STRAY_MARKER: &str = "- ";

/// What separates an explicit `\item[label]` from the item's text.
///
/// Two spaces, not one: a label is a word of its own (a term being defined, most of
/// the time), and one space would read as part of the sentence after it.
const LABEL_GAP: &str = "  ";

/// The lists category (PLAN.md §12.1).
///
/// Five environments, all sharing one handler, plus the `\item` their bodies bring into
/// scope. The optional argument every one of them accepts is `enumitem`'s key-value
/// option list: parsed so that it cannot leak into the text, ignored because none of
/// what it can say (`label=…`, `itemsep=…`, `resume`) has a plain-text meaning, and
/// reported as ignored so that the reader of the diagnostics knows it was dropped.
pub fn category() -> Category {
    let mut category = Category::new("lists");
    for (name, kind) in [
        ("itemize", ListKind::Itemize),
        ("enumerate", ListKind::Enumerate),
        ("description", ListKind::Description),
        ("trivlist", ListKind::Itemize),
    ] {
        category.add_env(
            EnvDef::new(name)
                .arg("o", "options")
                .list_body(kind)
                .rule(handler(ListEnv { kind })),
        );
    }
    // `\begin{list}{label}{spacing}` is the primitive the others are built on, and its
    // two mandatory arguments are LaTeX code that must not reach the text: the label
    // template is markup for a marker techxt generates itself, and the spacing
    // declarations are lengths. They are parsed and discarded without a diagnostic —
    // they are *known* arguments, not an unsupported feature.
    category.add_env(
        EnvDef::new("list")
            .arg("o", "options")
            .arg("m", "label")
            .arg("m", "spacing")
            .list_body(ListKind::Itemize)
            .rule(handler(ListEnv {
                kind: ListKind::Itemize,
            })),
    );
    category.add_macro(MacroDef::new("item").arg("o", "label").rule(handler(Item)));
    category
}

/// One list environment, identified by the kind of marker it gives its items.
#[derive(Debug)]
struct ListEnv {
    /// What its items are marked with.
    kind: ListKind,
}

impl TextHandler for ListEnv {
    fn render(
        &self,
        node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        if cx.arg_provided("options") {
            let name = String::from(node.name().unwrap_or("list"));
            cx.report(UnsupportedIgnored::new(name, "the list option list"));
        }

        let context = self.context(cx);
        let mut state = cx.state().clone();
        state.list = Some(context);

        // One counter and one kind per open list, pushed for the body's extent, popped
        // whatever the body did. `enumerate` counts in the counter; the other kinds do
        // not, but they still push so that the stack's top is always the innermost
        // list's counter — and so that the kinds line up with it.
        cx.list_counter_stack_mut().push(0);
        cx.list_kind_stack_mut().push(self.kind);
        let body = cx.body_with_state(state);
        cx.list_kind_stack_mut().pop();
        cx.list_counter_stack_mut().pop();
        let body = body?;

        let mut flow = Flow::new();
        flow.push(FlowItem::BlockStart(if context.depth == 1 {
            BlockKind::Indent {
                first: "  ".into(),
                cont: "  ".into(),
            }
        } else {
            // A nested list adds no indentation of its own — see the module
            // documentation for why it still opens a block.
            BlockKind::Indent {
                first: "".into(),
                cont: "".into(),
            }
        }));
        // The last item of the body is still open: an item block is closed by the next
        // item or by the end of the list, and this is the end of the list.
        let open = open_blocks(&body);
        flow.extend(body);
        for _ in 0..open {
            flow.push(FlowItem::BlockEnd);
        }
        flow.push(FlowItem::BlockEnd);
        Ok(flow)
    }
}

impl ListEnv {
    /// The context this list's body is rendered in (PLAN.md §9.4).
    ///
    /// The total depth comes from the enclosing list's own context, and the marker
    /// depth from the run's stack of open list kinds: it is `1 +` the number of
    /// enclosing lists **of this kind**, counted wherever they are, so an `itemize`
    /// written inside an `enumerate` inside an `itemize` is a second-level itemize and
    /// takes the second bullet. The innermost
    /// [`ListCtx`](crate::render::ListCtx) alone cannot answer that — it knows only the
    /// list immediately around this one — which is why the kinds are kept in run state.
    fn context(&self, cx: &RenderCx<'_, '_>) -> ListCtx {
        ListCtx {
            kind: self.kind,
            same_kind_depth: 1 + cx.enclosing_lists_of_kind(self.kind),
            depth: cx.state().list.map_or(0, |outer| outer.depth) + 1,
        }
    }
}

/// How many blocks a flow leaves open, counting an item block that a later item closed
/// only once.
///
/// This mirrors the layout engine's own frame rule ([`crate::layout`] step 4): a
/// [`BlockKind::Item`] auto-closes a preceding item that is still the innermost open
/// block, so a body of three `\item`s leaves *one* block open, not three. A list has to
/// know the count exactly — it emits that many closings before its own — because
/// closing one too many would end a block belonging to whatever the list is written
/// inside.
fn open_blocks(flow: &Flow) -> usize {
    // `true` for an item block, which is the only kind that auto-closes a sibling.
    let mut open: Vec<bool> = Vec::new();
    for item in flow.items() {
        match item {
            FlowItem::BlockStart(BlockKind::Item { .. }) => {
                if open.last() == Some(&true) {
                    open.pop();
                }
                open.push(true);
            }
            FlowItem::BlockStart(_) => open.push(false),
            FlowItem::BlockEnd => {
                open.pop();
            }
            _ => {}
        }
    }
    open.len()
}

/// `\item` (PLAN.md §9.4).
///
/// It renders as the opening of a hanging-indent block and nothing else: what follows
/// it in the document *is* the item's text, and the block ends when the next item
/// begins.
#[derive(Debug)]
struct Item;

impl TextHandler for Item {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let context = cx.state().list;
        if context.is_none() {
            cx.report(StrayItem);
        }
        // An explicit label is used verbatim and does **not** advance the counter, as
        // in LaTeX: `\item[Note]` in an `enumerate` leaves the next item numbered as if
        // this one had not been there. An empty label (`\item[]`) is no label at all.
        let label = cx
            .arg_text("label")?
            .filter(|label| !label.is_empty())
            .map(|label| label + LABEL_GAP);

        let first = match (label, context) {
            (Some(label), _) => label,
            (None, Some(context)) => automatic_marker(context, cx),
            (None, None) => String::from(STRAY_MARKER),
        };
        let cont: String = core::iter::repeat_n(' ', display_width(&first)).collect();

        let mut flow = Flow::new();
        flow.push(FlowItem::BlockStart(BlockKind::Item {
            first: first.into_boxed_str(),
            cont: cont.into_boxed_str(),
        }));
        Ok(flow)
    }
}

/// The marker an item takes from its list, when it was not given one.
fn automatic_marker(context: ListCtx, cx: &mut RenderCx<'_, '_>) -> String {
    match context.kind {
        ListKind::Itemize => {
            let markers = &cx.options().list_style.itemize_markers;
            let marker = cycle(markers, context.same_kind_depth).map_or("\u{2022}", |m| &**m);
            alloc::format!("{marker} ")
        }
        ListKind::Enumerate => {
            // Counted before the format is read, so that the mutable borrow of the run
            // state ends before the options are borrowed.
            let counter = next_counter(cx);
            let formats = &cx.options().list_style.enumerate_formats;
            let format = cycle(formats, context.same_kind_depth)
                .copied()
                .unwrap_or(EnumFormat {
                    style: CounterStyle::Arabic,
                    wrap: CounterWrap::Dot,
                });
            alloc::format!("{} ", format_counter(counter, format))
        }
        // A description item's marker *is* its label, so an item without one has no
        // marker at all — only the indentation that lines it up with its siblings.
        ListKind::Description => String::from(LABEL_GAP),
    }
}

/// Advance the innermost list's counter and answer its new value.
///
/// A tree parsed elsewhere can present an `\item` whose enclosing list techxt never
/// entered — a hand-built node, or a wrapping recomposer that rendered the environment
/// itself — so an empty stack answers 1 rather than failing.
fn next_counter(cx: &mut RenderCx<'_, '_>) -> u32 {
    match cx.list_counter_stack_mut().last_mut() {
        Some(counter) => {
            *counter = counter.saturating_add(1);
            *counter
        }
        None => 1,
    }
}

/// The entry of `values` that a list nested `depth` deep (counting from 1) uses.
///
/// The arrays cycle, so no nesting depth is ever without a marker. An array a caller
/// emptied has nothing to answer with, and the caller substitutes a default rather than
/// dividing by zero.
fn cycle<T>(values: &[T], depth: usize) -> Option<&T> {
    if values.is_empty() {
        return None;
    }
    values.get(depth.saturating_sub(1) % values.len())
}

/// Format an `enumerate` counter: `1.`, `(a)`, `iii.`, `B.`.
fn format_counter(counter: u32, format: EnumFormat) -> String {
    let body = match format.style {
        CounterStyle::Arabic => alloc::format!("{counter}"),
        CounterStyle::LowerAlpha => alphabetic(counter),
        CounterStyle::UpperAlpha => alphabetic(counter).to_uppercase(),
        CounterStyle::LowerRoman => roman(counter),
        CounterStyle::UpperRoman => roman(counter).to_uppercase(),
    };
    match format.wrap {
        CounterWrap::Dot => body + ".",
        CounterWrap::Parens => alloc::format!("({body})"),
    }
}

/// Spreadsheet-style letters: 1 → `a`, 26 → `z`, 27 → `aa`.
fn alphabetic(counter: u32) -> String {
    let mut value = counter;
    let mut letters = Vec::new();
    while value > 0 {
        let index = (value - 1) % 26;
        // `b'a' + index` is at most `b'z'`, so the cast is exact.
        letters.push((b'a' + index as u8) as char);
        value = (value - 1) / 26;
    }
    letters.reverse();
    letters.into_iter().collect()
}

/// Roman numerals, lowercase: 1 → `i`, 4 → `iv`, 1990 → `mcmxc`.
///
/// A counter of zero has no numeral at all; it answers the empty string rather than
/// looping.
fn roman(counter: u32) -> String {
    const NUMERALS: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut value = counter;
    let mut out = String::new();
    for (amount, numeral) in NUMERALS {
        while value >= amount {
            out.push_str(numeral);
            value -= amount;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabetic_counters_run_past_the_alphabet() {
        assert_eq!(alphabetic(1), "a");
        assert_eq!(alphabetic(26), "z");
        assert_eq!(alphabetic(27), "aa");
        assert_eq!(alphabetic(52), "az");
        assert_eq!(alphabetic(53), "ba");
        assert_eq!(alphabetic(0), "");
    }

    #[test]
    fn roman_counters_are_subtractive() {
        assert_eq!(roman(1), "i");
        assert_eq!(roman(4), "iv");
        assert_eq!(roman(9), "ix");
        assert_eq!(roman(14), "xiv");
        assert_eq!(roman(1990), "mcmxc");
        assert_eq!(roman(0), "");
    }

    #[test]
    fn markers_cycle_and_an_empty_array_has_none() {
        let markers = ["a", "b", "c"];
        assert_eq!(cycle(&markers, 1), Some(&"a"));
        assert_eq!(cycle(&markers, 3), Some(&"c"));
        assert_eq!(cycle(&markers, 4), Some(&"a"));
        assert_eq!(cycle(&markers, 7), Some(&"a"));
        // A depth of zero cannot occur, but it must not panic either.
        assert_eq!(cycle(&markers, 0), Some(&"a"));
        assert_eq!(cycle::<&str>(&[], 1), None);
    }

    #[test]
    fn counter_formats_read_as_the_plan_writes_them() {
        let dot = |style| EnumFormat {
            style,
            wrap: CounterWrap::Dot,
        };
        assert_eq!(format_counter(1, dot(CounterStyle::Arabic)), "1.");
        assert_eq!(format_counter(3, dot(CounterStyle::LowerRoman)), "iii.");
        assert_eq!(format_counter(2, dot(CounterStyle::UpperAlpha)), "B.");
        assert_eq!(format_counter(4, dot(CounterStyle::UpperRoman)), "IV.");
        assert_eq!(
            format_counter(
                2,
                EnumFormat {
                    style: CounterStyle::LowerAlpha,
                    wrap: CounterWrap::Parens
                }
            ),
            "(b)"
        );
    }

    #[test]
    fn open_blocks_counts_an_item_run_once() {
        let item = || {
            FlowItem::BlockStart(BlockKind::Item {
                first: "• ".into(),
                cont: "  ".into(),
            })
        };
        let indent = || {
            FlowItem::BlockStart(BlockKind::Indent {
                first: "".into(),
                cont: "".into(),
            })
        };

        let mut flow = Flow::new();
        assert_eq!(open_blocks(&flow), 0);

        flow.push(item());
        flow.push(item());
        flow.push(item());
        assert_eq!(open_blocks(&flow), 1);

        // A nested list: its own block, its items, and both of its closings.
        flow.push(indent());
        flow.push(item());
        flow.push(item());
        flow.push(FlowItem::BlockEnd);
        flow.push(FlowItem::BlockEnd);
        assert_eq!(open_blocks(&flow), 1);

        flow.push(FlowItem::BlockEnd);
        assert_eq!(open_blocks(&flow), 0);
        // Never negative, whatever an unbalanced handler emitted.
        flow.push(FlowItem::BlockEnd);
        assert_eq!(open_blocks(&flow), 0);
    }
}
