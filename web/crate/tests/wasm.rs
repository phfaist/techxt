//! The one test that needs a browser: `Session::convert` across the `JsValue` boundary
//! (web/PLAN.md §4.8).
//!
//! Everything else about the binding is ordinary Rust and is tested natively; what only
//! wasm can show is that a plain JavaScript options object deserializes into an
//! `OptionsDto`, and that the answer serializes into an object with the field names and
//! value spellings `web/src/worker/protocol.ts` declares. Deliberately not run in CI —
//! it needs a headless browser for one assertion — but it is compiled by
//! `cargo build --target wasm32-unknown-unknown --tests`, so it cannot rot unnoticed.
//!
//! Run it with `wasm-pack test --headless --firefox crate` from `web/`.
#![cfg(target_arch = "wasm32")]

use serde::{Deserialize, Serialize};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use techxt_web::{techxt_version, Session};

wasm_bindgen_test_configure!(run_in_browser);

/// What the app sends: a partial object, in the wire's camelCase.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SentOptions {
    wrap_width: Option<usize>,
    math_mode: &'static str,
}

/// What the app receives, declared here independently of the binding's own DTO so that
/// this test reads the wire rather than the code that wrote it. serde ignores the
/// fields it is not told about.
#[derive(Deserialize)]
struct ReceivedResult {
    ok: bool,
    text: String,
    ms: f64,
    diagnostics: Vec<ReceivedDiagnostic>,
    suppressed: u32,
    truncated: bool,
}

/// One diagnostic, as it arrives.
#[derive(Deserialize)]
struct ReceivedDiagnostic {
    severity: String,
    identifier: String,
    message: String,
    rendered: String,
    span: Option<ReceivedSpan>,
    frames: Vec<serde::de::IgnoredAny>,
}

/// A span, as it arrives.
#[derive(Deserialize)]
struct ReceivedSpan {
    start: u32,
    end: u32,
    line: u32,
    column: u32,
}

#[wasm_bindgen_test]
fn convert_round_trips_through_jsvalue() {
    let mut session = Session::new();

    let options = serde_wasm_bindgen::to_value(&SentOptions {
        wrap_width: Some(20),
        math_mode: "plain",
    })
    .expect("the options serialize");

    let value = session
        .convert("𝕏 \\undefinedmacro.", options)
        .expect("a well-formed options object does not throw");
    let result: ReceivedResult =
        serde_wasm_bindgen::from_value(value).expect("the result is the shape protocol.ts says");

    assert!(result.ok);
    assert!(result.text.contains('𝕏'));
    assert!(result.ms >= 0.0, "performance.now() answered");
    assert_eq!(result.suppressed, 0);
    assert!(!result.truncated);

    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.severity, "warning");
    assert_eq!(diagnostic.identifier, "techxt.unknown-macro");
    assert!(!diagnostic.message.is_empty());
    assert!(!diagnostic.rendered.is_empty());
    assert!(diagnostic.frames.is_empty());
    let span = diagnostic.span.as_ref().expect("span: not null");
    // UTF-16 code units: `𝕏` counts two, not one and not four (§4.4).
    assert_eq!(span.start, 3);
    assert_eq!(span.end, 18);
    assert_eq!(span.line, 1);
    assert_eq!(span.column, 3);

    // The second call reuses the cached converter, since the options are the same.
    let again = session
        .convert(
            "plain",
            serde_wasm_bindgen::to_value(&SentOptions {
                wrap_width: Some(20),
                math_mode: "plain",
            })
            .expect("the options serialize"),
        )
        .expect("no throw");
    let again: ReceivedResult = serde_wasm_bindgen::from_value(again).expect("shape");
    assert_eq!(again.text, "plain\n");

    // `null` means "the library's defaults", the same as an empty object.
    let defaulted = session
        .convert("a", wasm_bindgen::JsValue::NULL)
        .expect("null is accepted");
    let defaulted: ReceivedResult = serde_wasm_bindgen::from_value(defaulted).expect("shape");
    assert_eq!(defaulted.text, "a\n");
}

#[wasm_bindgen_test]
fn a_malformed_options_object_throws_and_a_malformed_document_does_not() {
    let mut session = Session::new();

    // An enum spelling this build does not know is the caller's mistake: `Err`.
    let bad = serde_wasm_bindgen::to_value(&SentOptions {
        wrap_width: None,
        math_mode: "shiny",
    })
    .expect("it serializes, it just is not a math mode");
    assert!(session.convert("a", bad).is_err());

    // A document the recovery policy cannot survive is not: `ok: false`, one error.
    #[derive(Serialize)]
    struct Strict {
        recovery: &'static str,
    }
    let strict = serde_wasm_bindgen::to_value(&Strict { recovery: "strict" }).expect("serializes");
    let value = session
        .convert("a { b", strict)
        .expect("a document-level failure never throws");
    let result: ReceivedResult = serde_wasm_bindgen::from_value(value).expect("shape");
    assert!(!result.ok);
    assert_eq!(result.text, "");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, "error");
}

#[wasm_bindgen_test]
fn the_version_is_the_embedded_one() {
    let version = techxt_version();
    assert!(!version.is_empty());
    assert!(version.contains('.'), "{version}");
}
