//! Math regions as the app receives them (web/PLAN.md §4.3, §4.8).
//!
//! The library reports *every* preformatted run of its output — a `verbatim` body, a
//! construct kept as source, a rendered display formula, a source-mode formula — and
//! only the last of those is LaTeX. These tests pin the filter, because the failure it
//! prevents is silent: a `MathRendered` region handed to MathJax is techxt's own aligned
//! Unicode being read back as source, and what comes out looks like a typesetting bug
//! rather than a binding one.
//!
//! Everything here slices the output the way the browser will slice it, in UTF-16 code
//! units, which is the other half of what the binding promises.

use techxt::convert::VerbatimProvenance;
use techxt_web::diag::{convert_native, ConversionResultDto, MathRegionDto};
use techxt_web::options::{
    self, MathModeDto, OptionsDto, UnknownEnvPolicyDto, UnknownMacroPolicyDto,
};

/// Convert `latex` in `MathMode::Source`, which is what the app's MathJax mode resolves
/// to (the library never hears the word).
fn convert_as_source(latex: &str) -> ConversionResultDto {
    let dto = OptionsDto {
        math_mode: Some(MathModeDto::Source),
        ..OptionsDto::default()
    };
    let converter = options::build(&dto).expect("the definitions build");
    convert_native(&converter, latex)
}

/// The text a region names, sliced as the DOM will slice it: in UTF-16 code units.
fn slice(text: &str, region: MathRegionDto) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16(&units[region.start as usize..region.end as usize])
        .expect("a region is whole in UTF-16")
}

/// Every region of `result`, sliced, paired with its `display` flag.
fn sliced(result: &ConversionResultDto) -> Vec<(String, bool)> {
    result
        .regions
        .iter()
        .map(|region| (slice(&result.text, *region), region.display))
        .collect()
}

/// The document these two tests share: a source-mode formula, a `\verb`, an unknown
/// macro kept as source, and a display formula. Converted twice it produces all four of
/// the library's provenances — the display formula is source under `MathMode::Source`
/// and rendered under `MathMode::Fancy` — so the filter is exercised against every tag
/// there is, on text that is otherwise identical.
const FOUR_PROVENANCES: &str = concat!(
    "A formula $\\alpha^2$ and \\verb|$not math$| and \\nope.\n",
    "\n",
    "\\begin{equation}\n",
    "  a = b\n",
    "\\end{equation}\n",
);

/// The options both passes share: unknown constructs kept as source, so that the
/// `\nope` in the document above really does become a `KeptSource` run.
fn keeping_unknowns() -> OptionsDto {
    OptionsDto {
        unknown_macro: Some(UnknownMacroPolicyDto::KeepSource),
        unknown_env: Some(UnknownEnvPolicyDto::KeepSource),
        ..OptionsDto::default()
    }
}

/// The provenances the library itself reports for `latex` under `dto`.
///
/// Asserted before the filtered list in both tests below, because a test that looked
/// only at what came *through* the filter would pass just as happily on a document that
/// never produced the other tags at all — and would then be testing nothing.
fn provenances(dto: &OptionsDto, latex: &str) -> Vec<VerbatimProvenance> {
    let converter = options::build(dto).expect("the definitions build");
    let conversion = converter.latex_to_text(latex).expect("a tolerant parse");
    conversion
        .regions
        .iter()
        .map(|region| region.kind)
        .collect()
}

/// Source mode: the two formulas are LaTeX and are reported; the `\verb` body and the
/// macro kept as source are preformatted, but neither is mathematics, so the app never
/// sees them — and neither does MathJax.
#[test]
fn only_source_mode_mathematics_is_reported() {
    let dto = OptionsDto {
        math_mode: Some(MathModeDto::Source),
        ..keeping_unknowns()
    };
    let kinds = provenances(&dto, FOUR_PROVENANCES);
    for expected in [
        VerbatimProvenance::Verbatim,
        VerbatimProvenance::KeptSource,
        VerbatimProvenance::MathSource { display: false },
        VerbatimProvenance::MathSource { display: true },
    ] {
        assert!(
            kinds.contains(&expected),
            "{expected:?} missing from {kinds:?}",
        );
    }

    let converter = options::build(&dto).expect("the definitions build");
    let result = convert_native(&converter, FOUR_PROVENANCES);
    assert!(result.ok);
    // The verbatim body is in the output, and looks exactly like a formula …
    assert!(result.text.contains("$not math$"), "{:?}", result.text);
    // … but only the two real formulas are offered as something to typeset.
    let regions = sliced(&result);
    assert_eq!(regions.len(), 2, "{regions:?}");
    assert_eq!(regions[0], (String::from("$\\alpha^2$"), false));
    assert!(regions[1].0.starts_with("\\begin{equation}"), "{regions:?}");
    assert!(regions[1].1, "the equation is display math");
}

/// Fancy mode, over the same document: the display formula is now techxt's own aligned
/// Unicode, reported as `MathRendered` because its columns are as fragile as a verbatim
/// body's. It is *output*, not source, so nothing at all survives the filter — handing
/// those bytes to a typesetter would ask it to read techxt's answer back as a question.
#[test]
fn rendered_mathematics_is_not_offered_to_a_typesetter() {
    let dto = keeping_unknowns();
    let kinds = provenances(&dto, FOUR_PROVENANCES);
    assert!(
        kinds.contains(&VerbatimProvenance::MathRendered { display: true }),
        "the display formula is rendered, and reported as such: {kinds:?}",
    );

    let converter = options::build(&dto).expect("the definitions build");
    let result = convert_native(&converter, FOUR_PROVENANCES);
    assert!(result.ok);
    // The formula was rendered, so techxt's own Unicode is in the text …
    assert!(result.text.contains('𝑎'), "{:?}", result.text);
    // … and none of it, nor the verbatim body beside it, is offered to MathJax.
    assert!(result.regions.is_empty(), "{:?}", sliced(&result));
}

/// The `\$` document of the TODO's verified fact 2, which is the whole argument for a
/// side table: the output has three dollar signs and only one pair of them opens a
/// formula, and nothing in the string says which.
#[test]
fn an_escaped_dollar_is_not_a_formula() {
    let result = convert_as_source("but not these \\$3 and $x$ values");
    assert!(result.ok);
    assert_eq!(result.text, "but not these $3 and $x$ values\n");
    assert_eq!(sliced(&result), vec![(String::from("$x$"), false)]);
}

/// Inline and display are told apart, and a display block's range stops before the
/// newline that ends its last line — the newline separates the block from what follows
/// and is not part of the formula.
#[test]
fn display_and_inline_are_distinguished() {
    let latex = concat!(
        "Inline $a+b$ first.\n",
        "\\[\n",
        "  c = d\n",
        "\\]\n",
        "Then text.\n",
    );
    let result = convert_as_source(latex);
    assert!(result.ok);

    let regions = sliced(&result);
    assert_eq!(regions.len(), 2, "{regions:?}");
    assert_eq!(regions[0], (String::from("$a+b$"), false));
    assert!(regions[1].1, "the second is display math");
    let block = &regions[1].0;
    assert!(block.starts_with("\\["), "{block:?}");
    assert!(block.ends_with("\\]"), "{block:?}");
    assert!(
        !block.ends_with('\n'),
        "the terminating newline is excluded"
    );
}

/// `MathMode::Plain` flattens a formula into ordinary text, so there is no math region
/// to report at all. An empty list is the right answer here rather than a bug.
#[test]
fn plain_mode_reports_no_mathematics() {
    let dto = OptionsDto {
        math_mode: Some(MathModeDto::Plain),
        ..OptionsDto::default()
    };
    let converter = options::build(&dto).expect("the definitions build");
    let result = convert_native(&converter, "An inline $a+b$ formula.\n");
    assert!(result.ok);
    assert!(result.regions.is_empty(), "{:?}", sliced(&result));
}

/// The UTF-16 conversion, actually exercised: emoji and other astral characters ahead of
/// the formula mean the byte offset the library reports and the offset the DOM counts in
/// differ, and by more than one.
///
/// The assertion is deliberately double — the region slices to the formula in UTF-16,
/// *and* slicing at the same numbers as bytes would not — because a mapper that silently
/// passed byte offsets through would satisfy the first half on an ASCII document.
#[test]
fn offsets_are_utf16_code_units_not_bytes() {
    let latex = "🎉 漢字 𝕏 then $x^2$ ends.\n";
    let result = convert_as_source(latex);
    assert!(result.ok);
    assert_eq!(sliced(&result), vec![(String::from("$x^2$"), false)]);

    let region = result.regions[0];
    let start = region.start as usize;
    assert_eq!(
        start,
        result
            .text
            .split("$x^2$")
            .next()
            .expect("a prefix")
            .encode_utf16()
            .count(),
        "the start is the prefix's length as JavaScript counts it",
    );
    // Each of 🎉 and 𝕏 costs one more byte than it costs UTF-16 units twice over, and
    // 漢 and 字 one more each: the two numbers genuinely disagree here.
    let bytes = result
        .text
        .find("$x^2$")
        .expect("the formula is in the output");
    assert!(
        bytes > start,
        "byte offset {bytes} vs UTF-16 offset {start}"
    );
}

/// Several formulas in one document come back in output order, and each names its own
/// text — the property the app relies on when it walks the table and the text between
/// the regions in one pass.
#[test]
fn regions_arrive_in_output_order() {
    let latex = "One $a$, two $b$, three $c$.\n";
    let result = convert_as_source(latex);
    assert_eq!(
        sliced(&result),
        vec![
            (String::from("$a$"), false),
            (String::from("$b$"), false),
            (String::from("$c$"), false),
        ],
    );
    let starts: Vec<u32> = result.regions.iter().map(|region| region.start).collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted);
}

/// A document with no preformatted content at all reports an empty table, and a failed
/// parse reports one too — there is no text for a region to point into.
#[test]
fn a_document_without_mathematics_reports_nothing() {
    let result = convert_as_source("Just words.\n");
    assert!(result.ok);
    assert!(result.regions.is_empty());
}
