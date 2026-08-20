//! The result DTO: diagnostics, spans, and the byte-to-UTF-16 offset mapping
//! (web/PLAN.md §4.3, §4.4, §4.5).
//!
//! [`convert_native`] is the whole of the binding's work: it converts a document and
//! shapes the answer into the `ConversionResult` of `web/src/worker/protocol.ts`. It
//! takes and returns ordinary Rust, so everything interesting here is testable without
//! wasm; `Session::convert` only times it and serializes what it returns.
//!
//! # Why the offsets are mapped here
//!
//! techy reports positions as **byte** offsets into UTF-8. `setSelectionRange` — the
//! whole point of a clickable diagnostic — wants **UTF-16 code-unit** offsets. Doing
//! that conversion in JavaScript means re-walking the document once per diagnostic and
//! getting surrogate pairs wrong at least once, so [`OffsetMap`] does it in one pass
//! over the document for every offset any diagnostic needs.
//!
//! # Why a span can be `null`
//!
//! A diagnostic's span points into a `Source`, which need not be the document the user
//! typed: a macro expansion synthesizes one, and an `\input` would resolve one. Only a
//! span into *this* document can be selected in the textarea, so the binding compares
//! the two and everything else comes back with `span: null` — message and `rendered`
//! text intact, but not clickable (§4.5).

use std::sync::Arc;

use serde::Serialize;

use techxt::convert::{Converter, Severity, Source, SourceSpan};

/// The result of one conversion, in the shape `protocol.ts` declares.
///
/// No `rename_all` here or below: every field name of `ConversionResult`, `Diagnostic`,
/// `Span` and `TraceFrame` is a single lowercase word, so Rust's own spelling is the
/// wire spelling. (`OptionsDto`, going the other way, does need one.)
#[derive(Clone, Debug, Serialize)]
pub struct ConversionResultDto {
    /// `false` only for a hard parse failure — which, with a tolerant recovery policy,
    /// means the descent guard refused (§4.6), and under `strict` means the document
    /// was malformed.
    pub ok: bool,
    /// The converted text; `""` when `!ok`.
    pub text: String,
    /// Conversion time in milliseconds. [`convert_native`] leaves this at `0.0`: it is
    /// the wasm entry point that owns the clock (`performance.now()`), because there is
    /// no `std::time::Instant` in a browser.
    pub ms: f64,
    /// In `Diagnostics::sorted_by_position()` order, so the panel reads in document
    /// order.
    pub diagnostics: Vec<DiagnosticDto>,
    /// `Diagnostics::suppressed()` — how many diagnostics were counted rather than
    /// stored once the retention cap (1000) was reached.
    pub suppressed: u32,
    /// Whether anything was suppressed, i.e. `suppressed > 0`.
    pub truncated: bool,
}

/// One diagnostic, in the shape `protocol.ts` declares.
#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticDto {
    /// How bad it is.
    pub severity: SeverityDto,
    /// techy's stable condition identifier, e.g. `techxt.unknown-macro`.
    pub identifier: String,
    /// The one-line message.
    pub message: String,
    /// `Diagnostic::render()`: the full text `techxt-cli` prints, caret line and
    /// traceback included, for the panel's per-row detail view.
    pub rendered: String,
    /// Where it is, or `null` when the span is not in the document that was converted
    /// (§4.5).
    pub span: Option<SpanDto>,
    /// The include/expansion trace behind it, outermost frame last (techy stores them
    /// innermost first, and they are passed through in that order).
    pub frames: Vec<TraceFrameDto>,
}

/// A span of the converted document, in **UTF-16 code units** (§4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpanDto {
    /// Start offset, in UTF-16 code units, ready for `setSelectionRange`.
    pub start: u32,
    /// End offset, exclusive, in UTF-16 code units.
    pub end: u32,
    /// The line the span starts on, 1-based.
    pub line: u32,
    /// The column the span starts at, 1-based and counted in characters.
    pub column: u32,
}

/// One frame of a diagnostic's trace.
#[derive(Clone, Debug, Serialize)]
pub struct TraceFrameDto {
    /// The rendered frame title, e.g. `argument #1 of ‘\frac’`.
    pub title: String,
    /// Where the parse descended into the frame, or `null` (§4.5).
    pub span: Option<SpanDto>,
}

/// A diagnostic severity, lowercase on the wire as `protocol.ts` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SeverityDto {
    /// Informational.
    Note,
    /// Something suspicious that did not prevent the conversion.
    Warning,
    /// A genuine error. The text is still there unless `ok` is `false`.
    Error,
}

impl From<Severity> for SeverityDto {
    fn from(severity: Severity) -> SeverityDto {
        match severity {
            Severity::Note => SeverityDto::Note,
            Severity::Warning => SeverityDto::Warning,
            Severity::Error => SeverityDto::Error,
        }
    }
}

/// Convert `latex` and shape the answer for the app.
///
/// This never fails: a parse the recovery policy could not survive comes back as
/// `ok: false` with exactly one error diagnostic, which is the whole of §4.1's promise
/// that `Session::convert` does not throw for a document-level failure.
///
/// [`ms`](ConversionResultDto::ms) is left at `0.0` for the caller to fill in.
pub fn convert_native(converter: &Converter, latex: &str) -> ConversionResultDto {
    let mut ours = OurSource::new(latex);
    let mut wanted = Vec::new();

    match converter.latex_to_text(latex) {
        Ok(conversion) => {
            let sorted = conversion.diagnostics.sorted_by_position();
            let raw: Vec<RawDiagnostic> = sorted
                .into_iter()
                .map(|diagnostic| {
                    RawDiagnostic::collect(
                        diagnostic.severity().into(),
                        diagnostic.identifier(),
                        diagnostic.message(),
                        diagnostic.render(),
                        diagnostic.span(),
                        diagnostic.frames().iter().map(|f| (f.title(), f.span())),
                        &mut ours,
                        &mut wanted,
                    )
                })
                .collect();

            let map = OffsetMap::build(latex, wanted);
            let suppressed = conversion.diagnostics.suppressed();
            ConversionResultDto {
                ok: true,
                text: conversion.text,
                ms: 0.0,
                diagnostics: raw.into_iter().map(|raw| raw.resolve(&map)).collect(),
                suppressed: as_u32(suppressed),
                truncated: suppressed > 0,
            }
        }
        Err(error) => {
            // No text, and one diagnostic: the parse error itself, rendered exactly as
            // the CLI would render it.
            let raw = RawDiagnostic::collect(
                SeverityDto::Error,
                error.identifier(),
                error.message(),
                error.render(),
                error.span(),
                error.frames().iter().map(|f| (f.title(), f.span())),
                &mut ours,
                &mut wanted,
            );
            let map = OffsetMap::build(latex, wanted);
            ConversionResultDto {
                ok: false,
                text: String::new(),
                ms: 0.0,
                diagnostics: vec![raw.resolve(&map)],
                suppressed: 0,
                truncated: false,
            }
        }
    }
}

/// A diagnostic whose spans are still byte offsets, waiting for the one pass over the
/// document that turns them into [`SpanDto`]s.
///
/// The two-phase shape is what makes §4.4's single pass possible: phase one decides
/// which spans are in the document at all and collects their byte offsets, phase two
/// maps them all at once.
struct RawDiagnostic {
    severity: SeverityDto,
    identifier: String,
    message: String,
    rendered: String,
    span: Option<(usize, usize)>,
    frames: Vec<(String, Option<(usize, usize)>)>,
}

impl RawDiagnostic {
    /// Phase one: copy the strings out, and record the byte offsets to be mapped.
    ///
    /// The frames arrive as an iterator of `(title, span)` rather than as techy's
    /// `TraceFrame` because a `TraceFrame` cannot be named through `techxt::convert` —
    /// and does not need to be, since a `Diagnostic` and a `ParseError` offer the same
    /// two accessors.
    #[allow(clippy::too_many_arguments)]
    fn collect<'a>(
        severity: SeverityDto,
        identifier: &str,
        message: String,
        rendered: String,
        span: &SourceSpan,
        frames: impl Iterator<Item = (&'a str, &'a SourceSpan)>,
        ours: &mut OurSource<'_>,
        wanted: &mut Vec<usize>,
    ) -> RawDiagnostic {
        RawDiagnostic {
            severity,
            identifier: identifier.to_string(),
            message,
            rendered,
            span: byte_span(span, ours, wanted),
            frames: frames
                .map(|(title, span)| (title.to_string(), byte_span(span, ours, wanted)))
                .collect(),
        }
    }

    /// Phase two: replace every byte range with the mapped [`SpanDto`].
    fn resolve(self, map: &OffsetMap) -> DiagnosticDto {
        DiagnosticDto {
            severity: self.severity,
            identifier: self.identifier,
            message: self.message,
            rendered: self.rendered,
            span: self.span.map(|range| map.span(range)),
            frames: self
                .frames
                .into_iter()
                .map(|(title, range)| TraceFrameDto {
                    title,
                    span: range.map(|range| map.span(range)),
                })
                .collect(),
        }
    }
}

/// The byte range of `span` if it is in the converted document, recording its two
/// offsets in `wanted`; `None` otherwise (§4.5).
fn byte_span(
    span: &SourceSpan,
    ours: &mut OurSource<'_>,
    wanted: &mut Vec<usize>,
) -> Option<(usize, usize)> {
    if !ours.accepts(span) {
        return None;
    }
    wanted.push(span.start());
    wanted.push(span.end());
    Some((span.start(), span.end()))
}

/// Decides whether a span points into the document this call converted (§4.5).
///
/// `Language::parse` mints a fresh `Source` per call, so there is no `Arc` the binding
/// could have kept to compare identities against: the test is `Source::content()`
/// against the string that was passed in, exactly as Appendix A suggests. The `Arc`
/// address of the source that answered is then remembered, so the common case — every
/// diagnostic in the document, one after another — costs one pointer comparison each
/// and the string comparison happens once.
struct OurSource<'a> {
    latex: &'a str,
    /// The address of the `Source` already recognized as the converted document.
    recognized: Option<*const Source>,
}

impl<'a> OurSource<'a> {
    /// A matcher for the document `latex`.
    fn new(latex: &'a str) -> OurSource<'a> {
        OurSource {
            latex,
            recognized: None,
        }
    }

    /// Whether `span`'s source is the converted document.
    fn accepts(&mut self, span: &SourceSpan) -> bool {
        let source = span.source();
        let address = Arc::as_ptr(source);
        if self.recognized == Some(address) {
            return true;
        }
        // `str` equality compares lengths first, so a synthesized source — a macro
        // expansion, typically a few characters — is rejected without a byte compare.
        if source.content() == self.latex {
            self.recognized = Some(address);
            return true;
        }
        false
    }
}

/// Where a byte offset lands, in the terms JavaScript needs (§4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The offset in UTF-16 code units — what `setSelectionRange` counts in.
    pub utf16: usize,
    /// The line, 1-based. Only `\n` ends a line, so a `\r\n` counts once.
    pub line: usize,
    /// The column, 1-based and counted in **characters** (not code units, and not
    /// display columns).
    pub column: usize,
}

impl Position {
    /// The start of any document: offset zero, line one, column one.
    pub const START: Position = Position {
        utf16: 0,
        line: 1,
        column: 1,
    };
}

/// Byte offsets into a document, mapped to [`Position`]s in one pass (§4.4).
///
/// Build it with every offset any diagnostic needs — [`build`](Self::build) sorts and
/// deduplicates them — and then ask for them back in any order.
///
/// # What it does with an awkward offset
///
/// An offset past the end of the document maps to the end of the document, and an
/// offset that is not on a `char` boundary maps to the next boundary at or after it.
/// Neither can arise from a span techy built over this document — `SourceSpan::new`
/// asserts both — but a mapping that panics would take the wasm instance with it, and
/// the app would lose the user's text over an arithmetic disagreement.
pub struct OffsetMap {
    /// The offsets asked for, sorted and deduplicated.
    offsets: Vec<usize>,
    /// Their positions, in the same order.
    positions: Vec<Position>,
}

impl OffsetMap {
    /// Map every offset in `wanted` against `latex`, in a single pass over the
    /// document.
    pub fn build(latex: &str, wanted: impl IntoIterator<Item = usize>) -> OffsetMap {
        let mut offsets: Vec<usize> = wanted.into_iter().collect();
        offsets.sort_unstable();
        offsets.dedup();

        let mut positions = Vec::with_capacity(offsets.len());
        let mut at = Position::START;
        let mut next = 0;

        for (byte, ch) in latex.char_indices() {
            // `<=` rather than `==`: an offset inside a multi-byte character was never
            // passed, so it clamps forward to this boundary rather than panicking.
            while next < offsets.len() && offsets[next] <= byte {
                positions.push(at);
                next += 1;
            }
            if next == offsets.len() {
                break;
            }
            at.utf16 += ch.len_utf16();
            // A `\r` is an ordinary character: only the `\n` of a `\r\n` ends the line,
            // so a Windows document does not come out with twice the lines it has.
            if ch == '\n' {
                at.line += 1;
                at.column = 1;
            } else {
                at.column += 1;
            }
        }
        // Whatever is left is at or past the end of the document, and `at` is the end.
        while positions.len() < offsets.len() {
            positions.push(at);
        }

        OffsetMap { offsets, positions }
    }

    /// Where `byte` lands.
    ///
    /// An offset that was not collected answers with the nearest collected one at or
    /// before it, or [`Position::START`] — a fallback that cannot be reached through
    /// [`convert_native`], which collects every offset it later asks for.
    pub fn position(&self, byte: usize) -> Position {
        match self.offsets.binary_search(&byte) {
            Ok(index) => self.positions[index],
            Err(index) => index
                .checked_sub(1)
                .map_or(Position::START, |index| self.positions[index]),
        }
    }

    /// A byte range as the app's [`SpanDto`]: UTF-16 offsets, plus the line and column
    /// of its start.
    fn span(&self, (start, end): (usize, usize)) -> SpanDto {
        let from = self.position(start);
        let to = self.position(end);
        SpanDto {
            start: as_u32(from.utf16),
            end: as_u32(to.utf16),
            line: as_u32(from.line),
            column: as_u32(from.column),
        }
    }
}

/// A count as a JavaScript-friendly `u32`, saturating rather than wrapping.
///
/// `u32` and not `usize`: `serde-wasm-bindgen` turns a 64-bit integer into a `BigInt`,
/// which is not what `interface Span { start: number }` says. A document long enough to
/// saturate this is four gigabytes of LaTeX in a textarea.
fn as_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.5's reject path, which no document can reach from outside: the binding
    /// exposes no `source_resolver` and techxt's own definitions synthesize no source,
    /// so every span a browser conversion produces is in the buffer. The comparison
    /// still has to be right the day one is not — a panel that scrolls the textarea to
    /// an offset in some other string is worse than one that says "not in this
    /// document".
    #[test]
    fn a_span_in_another_source_is_not_ours() {
        let latex = "the document";
        let ours: Arc<Source> = Arc::new(Source::new(latex));
        let elsewhere: Arc<Source> = Arc::new(Source::new("an expansion"));

        let mut matcher = OurSource::new(latex);
        assert!(matcher.accepts(&SourceSpan::new(&ours, 0..3)));
        assert!(!matcher.accepts(&SourceSpan::new(&elsewhere, 0..2)));
        // And the answer does not depend on which was asked about first.
        let mut matcher = OurSource::new(latex);
        assert!(!matcher.accepts(&SourceSpan::new(&elsewhere, 0..2)));
        assert!(matcher.accepts(&SourceSpan::new(&ours, 0..3)));
    }

    /// The test is content, not `Arc` identity: `Language::parse` mints a fresh source
    /// per call, so there is no handle to compare against and a second source holding
    /// the same document is the same document as far as the textarea is concerned.
    #[test]
    fn a_second_source_with_the_same_content_is_ours() {
        let latex = "the document";
        let copy: Arc<Source> = Arc::new(Source::new(String::from(latex)));
        assert!(OurSource::new(latex).accepts(&SourceSpan::new(&copy, 4..12)));
    }

    /// A span that is ours contributes its two offsets to the single pass; one that is
    /// not contributes nothing and comes back `None`, which serializes as `span: null`.
    #[test]
    fn only_our_spans_are_collected() {
        let latex = "the document";
        let ours: Arc<Source> = Arc::new(Source::new(latex));
        let elsewhere: Arc<Source> = Arc::new(Source::new("an expansion"));
        let mut matcher = OurSource::new(latex);
        let mut wanted = Vec::new();

        assert_eq!(
            byte_span(&SourceSpan::new(&ours, 4..12), &mut matcher, &mut wanted),
            Some((4, 12)),
        );
        assert_eq!(
            byte_span(
                &SourceSpan::new(&elsewhere, 0..2),
                &mut matcher,
                &mut wanted
            ),
            None,
        );
        assert_eq!(wanted, vec![4, 12]);
    }
}
