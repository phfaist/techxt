//! Diagnostics as the app receives them (web/PLAN.md §4.3, §4.5, §4.8).
//!
//! These run the real converter over real documents: what is under test is the shape of
//! the answer — severities, identifiers, mapped spans, the `ok: false` path — rather
//! than techxt's conversion, which `rust/` tests for itself.

use techxt_web::diag::{convert_native, ConversionResultDto, SeverityDto, SpanDto};
use techxt_web::options::{self, MacroDefinitionsDto, OptionsDto, RecoveryDto};

/// Convert `latex` with the library's defaults.
fn convert(latex: &str) -> ConversionResultDto {
    let converter = options::build(&OptionsDto::default()).expect("the definitions build");
    convert_native(&converter, latex)
}

/// Convert `latex` under `Recovery::Strict`.
fn convert_strictly(latex: &str) -> ConversionResultDto {
    let dto = OptionsDto {
        recovery: Some(RecoveryDto::Strict),
        ..OptionsDto::default()
    };
    let converter = options::build(&dto).expect("the definitions build");
    convert_native(&converter, latex)
}

/// The text a span selects, sliced the way the textarea will slice it: in UTF-16 code
/// units, exactly as `setSelectionRange` counts them.
fn selection(latex: &str, span: SpanDto) -> String {
    let units: Vec<u16> = latex.encode_utf16().collect();
    String::from_utf16(&units[span.start as usize..span.end as usize]).expect("a whole selection")
}

/// A clean document says so, and says nothing else.
#[test]
fn a_clean_document_reports_nothing() {
    let result = convert("Hello \\emph{world}.\n");
    assert!(result.ok);
    assert_eq!(result.text, "Hello 𝑤𝑜𝑟𝑙𝑑.\n");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.suppressed, 0);
    assert!(!result.truncated);
    // Off wasm there is no clock to ask; `Session::convert` fills this in.
    assert_eq!(result.ms, 0.0);
}

/// §4.8: an `\undefinedmacro` document yields one `techxt.unknown-macro` warning whose
/// span selects exactly `\undefinedmacro`.
#[test]
fn an_unknown_macro_is_one_warning_selecting_exactly_the_macro() {
    let latex = "a \\undefinedmacro.\n";
    let result = convert(latex);

    assert!(result.ok);
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.severity, SeverityDto::Warning);
    assert_eq!(diagnostic.identifier, "techxt.unknown-macro");
    assert!(
        diagnostic.message.contains("undefinedmacro"),
        "{diagnostic:?}"
    );
    // `rendered` is techy's own full report — the text `techxt-cli` prints — so a
    // screenshot of the panel is comparable to a terminal one.
    assert!(
        diagnostic.rendered.starts_with("warning:"),
        "{diagnostic:?}"
    );

    let span = diagnostic.span.expect("the span is in the document");
    assert_eq!(selection(latex, span), "\\undefinedmacro");
    assert_eq!(span.line, 1);
    assert_eq!(span.column, 3);
    // The span is the diagnostic's own, so the panel shows the position plainly.
    assert!(!diagnostic.approx);
}

/// §4.5 as amended for M9: a diagnostic raised inside an expansion points into the
/// macro's body, which the textarea cannot select — so the app gets the invocation
/// instead, marked `approx`.
///
/// This is the routine case rather than an exotic one, and it is the case with *no*
/// trace frames at all: the fallback that answers it is the source's provenance chain.
#[test]
fn a_diagnostic_from_an_expansion_points_at_the_invocation() {
    // No trailing newline: an invocation's span runs to the end of the whitespace TeX
    // skips after a control word, and `\a\n` would be a less legible assertion below.
    let latex = "\\newcommand{\\a}{x \\nope}\\a";
    let result = convert(latex);

    assert!(result.ok);
    assert_eq!(result.text, "x\n");
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.identifier, "techxt.unknown-macro");
    assert!(diagnostic.message.contains("nope"), "{diagnostic:?}");
    // The trace is empty, so this is the provenance chain's answer and not a frame's.
    assert!(diagnostic.frames.is_empty(), "{diagnostic:?}");

    let span = diagnostic.span.expect("the invocation is in the document");
    assert!(diagnostic.approx, "{diagnostic:?}");
    assert_eq!(selection(latex, span), "\\a");
    assert_eq!(span.line, 1);
    assert_eq!(span.column, 25);
}

/// With the definers only *declared* the same document has no expansion in it at all,
/// so the diagnostic is about `\a` itself and its span is exact.
#[test]
fn declared_only_definitions_leave_nothing_approximate() {
    // No trailing newline: an invocation's span runs to the end of the whitespace TeX
    // skips after a control word, and `\a\n` would be a less legible assertion below.
    let latex = "\\newcommand{\\a}{x \\nope}\\a";
    let dto = OptionsDto {
        macro_definitions: Some(MacroDefinitionsDto::Declared),
        ..OptionsDto::default()
    };
    let converter = options::build(&dto).expect("the definitions build");
    let result = convert_native(&converter, latex);

    // Nothing is defined and nothing expands, so `\a` renders as the default
    // `UnknownMacroPolicy::Skip` renders it: nothing at all.
    assert_eq!(result.text, "");
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.identifier, "techxt.unknown-macro");
    assert!(diagnostic.message.contains("\\a"), "{diagnostic:?}");
    assert!(!diagnostic.approx);
    assert_eq!(
        selection(latex, diagnostic.span.expect("in the document")),
        "\\a",
    );
}

/// The offsets are UTF-16 code units, so an astral character before the macro shifts
/// them by two and not by one — or by four, which is what the byte offset would say.
#[test]
fn a_span_is_mapped_into_utf16_code_units() {
    let latex = "𝕏 \\undefinedmacro.\n";
    let result = convert(latex);
    let span = result.diagnostics[0].span.expect("in the document");

    // `𝕏` is four bytes, one character, and two UTF-16 code units.
    assert_eq!(latex.find('\\'), Some(5), "the byte offset");
    assert_eq!(span.start, 3, "the UTF-16 offset");
    assert_eq!(selection(latex, span), "\\undefinedmacro");
    // The column is counted in characters, which is deliberately not what techy's own
    // rendering counts (it reports bytes); the panel shows a person's idea of a column.
    assert_eq!(span.column, 3);
    assert_eq!(span.line, 1);
}

/// A line and column that a multi-line document can get wrong.
#[test]
fn line_and_column_follow_the_document() {
    let latex = "one\ntwo \\undefinedmacro.\n";
    let span = convert(latex).diagnostics[0].span.expect("in the document");
    assert_eq!(span.line, 2);
    assert_eq!(span.column, 5);
    assert_eq!(selection(latex, span), "\\undefinedmacro");
}

/// §4.8: under strict recovery a malformed document is `ok: false` with one error — and
/// no text, since there is none.
#[test]
fn strict_mode_answers_not_ok_with_one_error() {
    let result = convert_strictly("a { b\n");

    assert!(!result.ok);
    assert_eq!(result.text, "");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, SeverityDto::Error);
    assert_eq!(
        result.diagnostics[0].identifier,
        "core.groups.unclosed-group"
    );
    assert!(result.diagnostics[0].span.is_some());
    assert_eq!(result.suppressed, 0);
    assert!(!result.truncated);

    // The same document converts under the default tolerant recovery: `ok: false` is
    // the strict answer, not the document's.
    let tolerant = convert("a { b\n");
    assert!(tolerant.ok);
    assert_eq!(tolerant.text, "a b\n");
}

/// A parse error's trace frames reach the app with their spans mapped too.
#[test]
fn the_frames_of_a_parse_error_are_mapped() {
    let latex = "\\begin{itemize}\\item x\n";
    let result = convert_strictly(latex);

    assert!(!result.ok);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(
        diagnostic.identifier,
        "core.environments.missing-terminator"
    );
    assert!(!diagnostic.frames.is_empty(), "{diagnostic:?}");
    for frame in &diagnostic.frames {
        assert!(!frame.title.is_empty());
        let span = frame.span.expect("every frame is in this document");
        assert!(
            span.start <= span.end && (span.end as usize) <= latex.encode_utf16().count(),
            "{span:?}",
        );
    }
}

/// Diagnostics arrive in `sorted_by_position` order, which is the order the panel reads
/// in.
#[test]
fn diagnostics_come_in_document_order() {
    let latex = "\\aaa. \\bbb. \\ccc.\n";
    let result = convert(latex);
    assert_eq!(result.diagnostics.len(), 3);

    let starts: Vec<u32> = result
        .diagnostics
        .iter()
        .map(|d| d.span.expect("in the document").start)
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted, "not in document order");
    assert_eq!(
        selection(latex, result.diagnostics[0].span.expect("")),
        "\\aaa"
    );
    assert_eq!(
        selection(latex, result.diagnostics[2].span.expect("")),
        "\\ccc"
    );
}

/// Past techy's retention cap the surplus is counted rather than stored, and that is
/// what `suppressed` and `truncated` report ("and N more" in the panel).
#[test]
fn the_retention_cap_is_reported() {
    let latex = "\\aaa. ".repeat(1_200);
    let result = convert(&latex);

    assert!(result.ok);
    assert!(result.truncated);
    assert!(result.suppressed > 0, "{}", result.suppressed);
    assert_eq!(
        result.diagnostics.len() + result.suppressed as usize,
        1_200,
        "every diagnostic is either kept or counted",
    );
}
