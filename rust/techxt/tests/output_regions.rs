//! Output regions: the side table naming the runs of a conversion that are not
//! converted text (root PLAN.md §7.1).
//!
//! What is under test is an *offset*, so nearly every assertion here slices the
//! conversion's own text with the region it reported and compares the substring. That is
//! the only check worth making: a region whose numbers are plausible but name the wrong
//! bytes is exactly the failure this table exists to prevent, and it is invisible to a
//! test that only counts regions or matches their kinds.
//!
//! Two cases carry the weight, because they are the two the offsets can be wrong in:
//! an inline region is accumulated into layout's shared word buffer and written wherever
//! wrapping decides to put the word, and a block region has layout's own continuation
//! indent inserted *inside* it. Both are checked across a spread of wrap widths and
//! inside a list.

use proptest::prelude::*;
use techxt::convert::{
    MathMode, OutputRegion, UnknownEnvPolicy, UnknownMacroPolicy, VerbatimProvenance,
};
use techxt::flow::{BlockKind, Flow, FlowItem};
use techxt::layout::{
    render, render_to, render_to_with_regions, render_with_regions, LayoutOptions,
};
use techxt::{Conversion, Converter, ConverterBuilder};

/// Two escaped dollars that convert to bare `$` characters, and one formula that does
/// not.
///
/// This sentence is the whole argument for the feature: after conversion nothing in the
/// string distinguishes the literal dollars from the formula's delimiters, so anything
/// that has to find the math must be told where it is.
const DOLLARS: &str = r"... but not these \$3 and \$4 values, only $x+y$ here.";

/// Convert with one option changed from the defaults.
fn with(build: impl FnOnce(ConverterBuilder) -> ConverterBuilder) -> Converter {
    build(Converter::builder()).build().expect("builds")
}

/// Convert in source math mode, wrapped or not — the combination an embedder typesetting
/// the formulae itself asks the library for.
fn source_mode(latex: &str, wrap: Option<usize>) -> Conversion {
    with(|b| b.math_mode(MathMode::Source).wrap_width(wrap))
        .latex_to_text(latex)
        .expect("parses")
}

/// The substrings the regions name, which is what a consumer actually consumes.
fn slices(conversion: &Conversion) -> Vec<&str> {
    conversion
        .regions
        .iter()
        .map(|region| &conversion.text[region.start..region.end])
        .collect()
}

/// The kinds, in order.
fn kinds(conversion: &Conversion) -> Vec<VerbatimProvenance> {
    conversion
        .regions
        .iter()
        .map(|region| region.kind)
        .collect()
}

/// Everything that must hold of any region table at all, checked on every conversion
/// this file makes: in bounds, non-empty, on character boundaries, in output order, and
/// non-overlapping.
///
/// Slicing a `String` panics on a bad offset, so most of this would surface as a panic
/// somewhere eventually; asserting it here says which document broke it.
fn check_invariants(conversion: &Conversion) {
    let text = &conversion.text;
    let mut previous_end = 0;
    for OutputRegion { start, end, .. } in conversion.regions.iter().copied() {
        assert!(start < end, "empty region {start}..{end} in {text:?}");
        assert!(
            end <= text.len(),
            "region {start}..{end} past the end of {text:?}"
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "region {start}..{end} splits a character of {text:?}"
        );
        assert!(
            start >= previous_end,
            "region {start}..{end} overlaps or precedes the one before it in {text:?}"
        );
        previous_end = end;
    }
}

// ------------------------------------------------------------------ the empty case

#[test]
fn ordinary_text_reports_no_regions() {
    let conversion = Converter::standard()
        .latex_to_text("Some \\textbf{ordinary} text with a $x$ formula in it.")
        .expect("parses");
    check_invariants(&conversion);
    // Nothing here is copied through: the formula is rendered, not re-emitted.
    assert_eq!(
        conversion.text,
        "Some 𝐨𝐫𝐝𝐢𝐧𝐚𝐫𝐲 text with a 𝑥 formula in it.\n"
    );
    assert!(conversion.regions.is_empty());
}

#[test]
fn escaped_dollars_alone_are_not_regions() {
    // The sentence above without its formula: two `$` in the output,
    // neither of them math, and nothing to report.
    let conversion = source_mode(r"... but not these \$3 and \$4 values.", None);
    check_invariants(&conversion);
    assert_eq!(conversion.text, "... but not these $3 and $4 values.\n");
    assert!(conversion.regions.is_empty());
}

// ------------------------------------------------- the inline case, at every width

#[test]
fn a_source_formula_is_the_only_region_of_the_dollars_document() {
    let conversion = source_mode(DOLLARS, None);
    check_invariants(&conversion);
    assert_eq!(
        conversion.text,
        "... but not these $3 and $4 values, only $x+y$ here.\n"
    );
    assert_eq!(slices(&conversion), ["$x+y$"]);
    assert_eq!(
        kinds(&conversion),
        [VerbatimProvenance::MathSource { display: false }]
    );
    // The point of the exercise: the two literal dollars are outside every region, so a
    // consumer that wraps the regions leaves them alone.
    let region = conversion.regions[0];
    assert!(!conversion.text[..region.start].contains("$x"));
    assert_eq!(conversion.text[..region.start].matches('$').count(), 2);
}

#[test]
fn an_inline_region_lands_correctly_at_every_wrap_width() {
    // An inline formula accumulates into the layout engine's shared word buffer and is
    // written wherever wrapping decides to put the word — so the offset is only right if
    // the range recorded against the word was rebased onto the line it ended up on,
    // prefix included. Sweeping the widths walks the formula across the lines.
    for width in 8..=60 {
        let conversion = source_mode(DOLLARS, Some(width));
        check_invariants(&conversion);
        assert_eq!(
            slices(&conversion),
            ["$x+y$"],
            "wrong region at wrap width {width}: {:?}",
            conversion.text
        );
    }
}

#[test]
fn an_inline_region_inside_a_list_clears_the_item_prefix() {
    // The word carries a bullet or a number in front of it on the item's first line and
    // an indent on every line after, all written before the word itself: the base offset
    // has to be taken after the prefix, not before it.
    let latex = r"\begin{itemize}\item alpha beta gamma $x+y$ delta epsilon\end{itemize}";
    for width in 12..=48 {
        let conversion = source_mode(latex, Some(width));
        check_invariants(&conversion);
        assert_eq!(
            slices(&conversion),
            ["$x+y$"],
            "wrong region at wrap width {width}: {:?}",
            conversion.text
        );
    }
}

#[test]
fn two_adjacent_formulas_are_two_regions_of_one_word() {
    // `$a$$b$` is two scopes glued into a single unbreakable word, so both ranges are
    // recorded against the same word buffer and both have to be rebased.
    let conversion = source_mode("$a$$b$ done", None);
    check_invariants(&conversion);
    assert_eq!(conversion.text, "$a$$b$ done\n");
    assert_eq!(slices(&conversion), ["$a$", "$b$"]);
}

#[test]
fn an_inline_region_names_only_itself_when_text_is_glued_to_it() {
    // `a\verb|x  y|b` is one word of three items; only the middle one is preformatted.
    let conversion = source_mode(r"a\verb|x  y|b more", None);
    check_invariants(&conversion);
    assert_eq!(conversion.text, "ax  yb more\n");
    assert_eq!(slices(&conversion), ["x  y"]);
    assert_eq!(kinds(&conversion), [VerbatimProvenance::Verbatim]);
}

#[test]
fn offsets_are_bytes_and_survive_multi_byte_text_before_them() {
    // The offsets are byte indices into `text`, and everything before this formula is
    // wider than one byte per character.
    let conversion = source_mode(r"漢字 é 𝐛 $x^2$ tail", None);
    check_invariants(&conversion);
    assert_eq!(slices(&conversion), ["$x^2$"]);
    assert!(conversion.regions[0].start > "漢字 é 𝐛 ".chars().count());
}

// -------------------------------------------------- the block case, and the indent

#[test]
fn a_display_region_includes_the_continuation_indent_layout_inserted() {
    // A display formula inside a list is emitted line by line under the list's
    // continuation indent, and those spaces are *in the output*: a region naming the
    // payload's own bytes would name a string that is not there.
    let latex = r"\begin{itemize}\item text \[ E = mc^2 \] more\end{itemize}";
    let conversion = source_mode(latex, None);
    check_invariants(&conversion);
    assert_eq!(
        conversion.text,
        "  • text\n\n    \\[ E = mc^2 \\]\n\n    more\n"
    );
    assert_eq!(slices(&conversion), ["    \\[ E = mc^2 \\]"]);
    assert_eq!(
        kinds(&conversion),
        [VerbatimProvenance::MathSource { display: true }]
    );
}

#[test]
fn a_display_region_is_stable_across_wrap_widths_and_nesting_depths() {
    // Wrapping never touches a preformatted block, but it does move everything around
    // it, and a deeper list widens the indent the block picks up.
    let flat = r"\begin{itemize}\item text \[ E = mc^2 \] more\end{itemize}";
    let nested = r"\begin{itemize}\item\begin{enumerate}\item t \[ E = mc^2 \] u\end{enumerate}\end{itemize}";
    for width in [None, Some(8), Some(12), Some(20), Some(40), Some(100)] {
        let conversion = source_mode(flat, width);
        check_invariants(&conversion);
        assert_eq!(
            slices(&conversion),
            ["    \\[ E = mc^2 \\]"],
            "at {width:?}"
        );

        let conversion = source_mode(nested, width);
        check_invariants(&conversion);
        assert_eq!(
            slices(&conversion),
            ["       \\[ E = mc^2 \\]"],
            "at {width:?}"
        );
    }
}

#[test]
fn a_verbatim_block_covers_its_own_blank_lines_and_every_indent() {
    let latex =
        "\\begin{itemize}\n\\item\n\\begin{verbatim}\na  b\n\n  c\n\\end{verbatim}\n\\end{itemize}";
    let conversion = source_mode(latex, None);
    check_invariants(&conversion);
    assert_eq!(conversion.text, "    a  b\n\n      c\n");
    // Every line's indent, and the blank line between them, are part of the block.
    assert_eq!(slices(&conversion), ["    a  b\n\n      c"]);
    assert_eq!(kinds(&conversion), [VerbatimProvenance::Verbatim]);
}

#[test]
fn a_blocks_region_stops_before_the_newline_that_terminates_it() {
    // The last line's newline separates the block from what follows rather than
    // belonging to it, so a consumer wrapping the range does not swallow the break.
    let conversion = source_mode(
        "before\n\\begin{verbatim}\nraw\n\\end{verbatim}\nafter",
        None,
    );
    check_invariants(&conversion);
    assert_eq!(conversion.text, "before\n\nraw\n\nafter\n");
    assert_eq!(slices(&conversion), ["raw"]);
}

// ------------------------------------------------------------------ the provenances

#[test]
fn the_three_math_modes_report_three_different_things() {
    let latex = r"A $x+y$ and \[ E=mc^2 \]";

    // Source: the formulas are LaTeX, copied through, and both are regions.
    let conversion = with(|b| b.math_mode(MathMode::Source))
        .latex_to_text(latex)
        .expect("parses");
    check_invariants(&conversion);
    assert_eq!(slices(&conversion), ["$x+y$", "\\[ E=mc^2 \\]"]);
    assert_eq!(
        kinds(&conversion),
        [
            VerbatimProvenance::MathSource { display: false },
            VerbatimProvenance::MathSource { display: true },
        ]
    );

    // Fancy: the display formula is techxt's own aligned output, which is preformatted
    // for a different reason and says so. The inline formula is ordinary words and glue
    // that wrapping may split, so there is nothing to name.
    let conversion = Converter::standard().latex_to_text(latex).expect("parses");
    check_invariants(&conversion);
    assert_eq!(slices(&conversion), ["    𝐸 = 𝑚𝑐²"]);
    assert_eq!(
        kinds(&conversion),
        [VerbatimProvenance::MathRendered { display: true }]
    );

    // Plain: neither formula is preformatted at all.
    let conversion = with(|b| b.math_mode(MathMode::Plain))
        .latex_to_text(latex)
        .expect("parses");
    check_invariants(&conversion);
    assert!(conversion.regions.is_empty(), "{:?}", conversion.regions);
}

#[test]
fn an_inline_matrix_is_reported_where_its_columns_are() {
    // Rendered inline math contributes a region only where a fragment carries spacing of
    // its own — an inline matrix's padded columns — because that is the only part layout
    // is forbidden to touch. There is no such thing as a region over a whole rendered
    // inline formula.
    let conversion = Converter::standard()
        .latex_to_text(r"inline $\begin{pmatrix} a & b \\ c & d\end{pmatrix}$ done")
        .expect("parses");
    check_invariants(&conversion);
    assert!(!conversion.regions.is_empty());
    for kind in kinds(&conversion) {
        assert_eq!(kind, VerbatimProvenance::MathRendered { display: false });
    }
    for slice in slices(&conversion) {
        assert!(
            slice.contains("  "),
            "{slice:?} carries no spacing of its own"
        );
    }
}

#[test]
fn a_source_formula_is_reported_after_expansion() {
    // A document's own macro is gone by the time the source is reassembled (§9.5 leaves
    // expansion to techy-xp, at token-reading time), so what the region names is
    // primitives — which is what makes the
    // reported range meaningful to anything that only understands LaTeX's own commands.
    let conversion = source_mode(
        "\\newcommand{\\ket}[1]{\\lvert #1 \\rangle}\nA state $\\ket{\\psi}$.",
        None,
    );
    check_invariants(&conversion);
    assert_eq!(slices(&conversion), [r"$\lvert \psi \rangle$"]);
    assert!(!conversion.text.contains(r"\ket"));
}

#[test]
fn kept_source_is_reported_as_kept_source() {
    let converter = with(|b| {
        b.unknown_macro(UnknownMacroPolicy::KeepSource)
            .unknown_env(UnknownEnvPolicy::KeepSource)
    });

    let conversion = converter
        .latex_to_text(r"text \nosuchmacro end")
        .expect("parses");
    check_invariants(&conversion);
    assert_eq!(conversion.text, "text \\nosuchmacro end\n");
    // The kept source is the macro's whole span, its post-space included, and that
    // space reaches the output inside the region rather than as glue beside it. A
    // region names what was written, not what one might have guessed was written.
    assert_eq!(slices(&conversion), ["\\nosuchmacro "]);
    assert_eq!(kinds(&conversion), [VerbatimProvenance::KeptSource]);

    let conversion = converter
        .latex_to_text("a\n\\begin{nosuchenv}\nbody\n\\end{nosuchenv}\nb")
        .expect("parses");
    check_invariants(&conversion);
    assert_eq!(
        slices(&conversion),
        ["\\begin{nosuchenv}\nbody\n\\end{nosuchenv}"]
    );
    assert_eq!(kinds(&conversion), [VerbatimProvenance::KeptSource]);
}

#[test]
fn a_document_of_several_kinds_reports_them_all_in_output_order() {
    let latex = concat!(
        r"Start $x$ and \verb|raw| then \nosuchmacro{}.",
        "\n\n",
        "\\begin{verbatim}\nblock\n\\end{verbatim}\n\n",
        r"End \[ y \] here.",
    );
    let conversion = with(|b| {
        b.math_mode(MathMode::Source)
            .unknown_macro(UnknownMacroPolicy::KeepSource)
    })
    .latex_to_text(latex)
    .expect("parses");
    check_invariants(&conversion);
    assert_eq!(
        kinds(&conversion),
        [
            VerbatimProvenance::MathSource { display: false },
            VerbatimProvenance::Verbatim,
            VerbatimProvenance::KeptSource,
            VerbatimProvenance::Verbatim,
            VerbatimProvenance::MathSource { display: true },
        ]
    );
    assert_eq!(
        slices(&conversion),
        ["$x$", "raw", r"\nosuchmacro", "block", r"\[ y \]"]
    );
}

// ------------------------------------------------------------- the layout entry points

#[test]
fn the_region_pass_writes_exactly_what_the_plain_pass_writes() {
    // The regions come out of the same single pass, so asking for them may not change a
    // byte of the text — the whole design rests on the output being untouched.
    let mut flow = Flow::from_plain_text("alpha beta gamma");
    flow.push(FlowItem::Verbatim {
        text: "  raw  line\n".into(),
        provenance: VerbatimProvenance::Verbatim,
    });
    flow.extend(Flow::from_plain_text(" delta"));
    flow.push(FlowItem::InlineVerbatim {
        text: "$3".into(),
        provenance: VerbatimProvenance::MathSource { display: false },
    });

    for width in [None, Some(6), Some(11), Some(30)] {
        let mut opts = LayoutOptions::default();
        opts.wrap_width = width;
        let (text, regions) = render_with_regions(&flow, &opts);
        assert_eq!(text, render(&flow, &opts), "at {width:?}");

        let mut streamed = String::new();
        let streamed_regions =
            render_to_with_regions(&flow, &opts, &mut streamed).expect("a String cannot fail");
        assert_eq!(streamed, text);
        assert_eq!(streamed_regions, regions);

        let mut plain = String::new();
        render_to(&flow, &opts, &mut plain).expect("a String cannot fail");
        assert_eq!(plain, text);

        assert_eq!(
            regions
                .iter()
                .map(|region| &text[region.start..region.end])
                .collect::<Vec<_>>(),
            ["  raw  line", "$3"],
            "at {width:?}"
        );
    }
}

#[test]
fn an_empty_verbatim_item_contributes_no_region() {
    // An empty verbatim is a no-op in layout — not even a word boundary — so there is
    // nothing to point at either.
    let mut flow = Flow::text("a");
    flow.push(FlowItem::Verbatim {
        text: "".into(),
        provenance: VerbatimProvenance::Verbatim,
    });
    flow.push(FlowItem::InlineVerbatim {
        text: "".into(),
        provenance: VerbatimProvenance::Verbatim,
    });
    flow.extend(Flow::text("b"));
    let (text, regions) = render_with_regions(&flow, &LayoutOptions::default());
    assert_eq!(text, "ab\n");
    assert!(regions.is_empty());
}

#[test]
fn an_inline_region_keeps_a_newline_its_payload_carries() {
    // An inline verbatim goes out byte for byte, newline included — which is how a
    // construct kept as source ends up with a line break inside a "word". The region
    // names the bytes that were written, so it holds the newline too rather than
    // stopping at the end of the line.
    let mut flow = Flow::text("a");
    flow.push(FlowItem::InlineVerbatim {
        text: "raw\n".into(),
        provenance: VerbatimProvenance::KeptSource,
    });
    flow.extend(Flow::text("b"));
    let (text, regions) = render_with_regions(&flow, &LayoutOptions::default());
    assert_eq!(text, "araw\nb\n");
    assert_eq!(&text[regions[0].start..regions[0].end], "raw\n");
}

#[test]
fn streamed_offsets_count_what_this_call_wrote() {
    // `render_to_with_regions` reports offsets from zero, so they index the sink only if
    // the sink started empty. Saying so here keeps the doc comment honest.
    let mut flow = Flow::text("x");
    flow.push(FlowItem::InlineVerbatim {
        text: "$1$".into(),
        provenance: VerbatimProvenance::MathSource { display: false },
    });

    let mut sink = String::from("PREFIX");
    let regions =
        render_to_with_regions(&flow, &LayoutOptions::default(), &mut sink).expect("cannot fail");
    assert_eq!(sink, "PREFIXx$1$\n");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].start, 1);
    assert_eq!(
        &sink["PREFIX".len() + regions[0].start.."PREFIX".len() + regions[0].end],
        "$1$"
    );
}

// ------------------------------------------------------------------------ properties

/// Characters a generated payload is built from: enough whitespace to exercise the
/// blank lines and trailing spaces layout must not touch, and no character that could be
/// mistaken for a block prefix.
const PAYLOAD_CHARS: &[char] = &['a', 'b', ' ', '\t', '\n'];

/// A payload with at least one character that is not a newline.
///
/// A payload of newlines alone renders to blank lines and therefore to nothing a region
/// could point at, which would make the "one region per item" property below say
/// something more complicated than it is worth. What it excludes is one degenerate
/// shape, not a class of behaviour: `an_empty_verbatim_item_contributes_no_region`
/// covers the empty end of it directly.
fn payload() -> impl Strategy<Value = Box<str>> {
    prop::collection::vec(prop::sample::select(PAYLOAD_CHARS), 1..12)
        .prop_map(|chars| chars.into_iter().collect::<String>().into_boxed_str())
        .prop_filter("all newlines", |text| text.contains(|c| c != '\n'))
}

/// A word with no whitespace in it, for the ordinary text between the verbatim items.
fn word() -> impl Strategy<Value = Box<str>> {
    prop::collection::vec(prop::sample::select(&['a', 'é', '漢', '𝐛'][..]), 1..5)
        .prop_map(|chars| chars.into_iter().collect::<String>().into_boxed_str())
}

/// The tags, so that a generated flow exercises more than one of them.
fn provenance() -> impl Strategy<Value = VerbatimProvenance> {
    prop_oneof![
        Just(VerbatimProvenance::Verbatim),
        Just(VerbatimProvenance::KeptSource),
        Just(VerbatimProvenance::MathSource { display: false }),
        Just(VerbatimProvenance::MathSource { display: true }),
    ]
}

fn item(with_blocks: bool) -> impl Strategy<Value = FlowItem> {
    let structural = if with_blocks { 2 } else { 0 };
    prop_oneof![
        6 => word().prop_map(FlowItem::Text),
        6 => Just(FlowItem::Glue),
        2 => Just(FlowItem::HardBreak),
        2 => Just(FlowItem::ParagraphBreak),
        3 => (payload(), provenance())
            .prop_map(|(text, provenance)| FlowItem::InlineVerbatim { text, provenance }),
        3 => (payload(), provenance())
            .prop_map(|(text, provenance)| FlowItem::Verbatim { text, provenance }),
        structural => word().prop_map(|first| FlowItem::BlockStart(BlockKind::Item {
            first,
            cont: "..".into(),
        })),
        structural => Just(FlowItem::BlockEnd),
    ]
}

fn flow_of(with_blocks: bool) -> impl Strategy<Value = Flow> {
    prop::collection::vec(item(with_blocks), 0..24).prop_map(|items| {
        let mut flow = Flow::new();
        for item in items {
            flow.push(item);
        }
        flow
    })
}

/// The verbatim items of a flow, in order, as `(text, provenance, is_block)`.
fn verbatim_items(flow: &Flow) -> Vec<(&str, VerbatimProvenance, bool)> {
    flow.items()
        .iter()
        .filter_map(|item| match item {
            FlowItem::InlineVerbatim { text, provenance } => Some((&**text, *provenance, false)),
            FlowItem::Verbatim { text, provenance } => Some((&**text, *provenance, true)),
            _ => None,
        })
        .collect()
}

fn layout(width: Option<usize>) -> LayoutOptions {
    let mut opts = LayoutOptions::default();
    opts.wrap_width = width;
    opts
}

proptest! {
    /// Whatever the flow and whatever the width: one region per verbatim item, in output
    /// order, carrying that item's tag, and every inline region naming exactly its own
    /// payload.
    ///
    /// The inline half is the one the wrapping decision can break, because the word it
    /// belongs to is written at a position nothing knows until it is written; a block's
    /// range additionally picks up the indent, so it is checked separately below.
    #[test]
    fn every_verbatim_item_yields_one_correctly_placed_region(
        flow in flow_of(true),
        wrap in prop::option::of(1usize..14),
    ) {
        let (text, regions) = render_with_regions(&flow, &layout(wrap));
        let items = verbatim_items(&flow);
        prop_assert_eq!(regions.len(), items.len(), "in {:?}", text);

        let mut previous_end = 0;
        for (region, (payload, provenance, is_block)) in regions.iter().zip(items) {
            prop_assert_eq!(region.kind, provenance);
            prop_assert!(region.start < region.end);
            prop_assert!(region.end <= text.len());
            prop_assert!(text.is_char_boundary(region.start) && text.is_char_boundary(region.end));
            prop_assert!(region.start >= previous_end);
            previous_end = region.end;
            if !is_block {
                prop_assert_eq!(&text[region.start..region.end], payload);
            }
        }
    }

    /// Outside a block there is no continuation indent to insert, so a block region
    /// names its payload exactly — minus the trailing newline that terminates the last
    /// line rather than belonging to it.
    #[test]
    fn a_block_region_names_its_payload_when_no_indent_is_inserted(
        flow in flow_of(false),
        wrap in prop::option::of(1usize..14),
    ) {
        let (text, regions) = render_with_regions(&flow, &layout(wrap));
        for (region, (payload, _, is_block)) in regions.iter().zip(verbatim_items(&flow)) {
            let expected = if is_block {
                payload.strip_suffix('\n').unwrap_or(payload)
            } else {
                payload
            };
            prop_assert_eq!(&text[region.start..region.end], expected);
        }
    }

    /// Asking for the regions never changes the text.
    #[test]
    fn the_text_is_the_same_with_and_without_regions(
        flow in flow_of(true),
        wrap in prop::option::of(1usize..14),
    ) {
        let opts = layout(wrap);
        prop_assert_eq!(render_with_regions(&flow, &opts).0, render(&flow, &opts));
    }
}
