//! The other test that needs a browser: `Session::complete` across the `JsValue`
//! boundary (web/PLAN.md §4.9).
//!
//! What only wasm can show is the wire spelling — that `from_document` arrives as
//! `fromDocument`, that a kind is the lowercase string `protocol.ts` declares, that a
//! missing replacement is `null` and not an absent key, and that `arity` is a number
//! rather than the `BigInt` a 64-bit integer would have become. The ranking and the
//! document scan are ordinary Rust and are tested natively in `completion.rs`.
//!
//! Deliberately not run in CI, like `wasm.rs` — it needs a headless browser — but it is
//! compiled by `cargo build --target wasm32-unknown-unknown --tests`, so it cannot rot
//! unnoticed. Run it with `wasm-pack test --headless --firefox crate` from `web/`.
#![cfg(target_arch = "wasm32")]

use serde::Deserialize;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use techxt_web::Session;

wasm_bindgen_test_configure!(run_in_browser);

/// One suggestion, as the app receives it — declared here independently of the binding's
/// own DTO so that this test reads the wire rather than the code that wrote it.
///
/// `deny_unknown_fields` is the point of the exercise: a field this crate grows without
/// `protocol.ts` growing it too fails here.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceivedCompletion {
    name: String,
    kind: String,
    replacement: Option<String>,
    arity: u32,
    #[serde(rename = "fromDocument")]
    from_document: bool,
}

#[wasm_bindgen_test]
fn complete_round_trips_through_jsvalue() {
    let mut session = Session::new();

    let value = session
        .complete("", "alpha", 4)
        .expect("a completion request does not throw");
    let items: Vec<ReceivedCompletion> =
        serde_wasm_bindgen::from_value(value).expect("the array is the shape protocol.ts says");

    let alpha = items
        .iter()
        .find(|item| item.name == "alpha")
        .expect("the library ships it");
    assert_eq!(alpha.kind, "macro");
    assert_eq!(alpha.replacement.as_deref(), Some("α"));
    assert_eq!(alpha.arity, 0);
    assert!(!alpha.from_document);
}

#[wasm_bindgen_test]
fn a_documents_own_macro_arrives_flagged_and_first() {
    let mut session = Session::new();

    let value = session
        .complete(r"\newcommand{\ket}[1]{\lvert #1 \rangle}", "ke", 4)
        .expect("no throw");
    let items: Vec<ReceivedCompletion> = serde_wasm_bindgen::from_value(value).expect("shape");

    assert_eq!(items[0].name, "ket");
    assert!(items[0].from_document);
    assert_eq!(items[0].arity, 1);
    // A definition the scan recognized but did not evaluate: `null`, not an absent key,
    // which is what `replacement: string | null` promises a reader of the row.
    assert_eq!(items[0].replacement, None);

    // The second call reuses the table the first one built.
    let again = session.complete("", "beta", 4).expect("no throw");
    let again: Vec<ReceivedCompletion> = serde_wasm_bindgen::from_value(again).expect("shape");
    assert!(again.iter().any(|item| item.name == "beta"));
}

#[wasm_bindgen_test]
fn an_empty_answer_is_an_empty_array() {
    let mut session = Session::new();

    let value = session.complete("", "qqzzxx", 8).expect("no throw");
    let items: Vec<ReceivedCompletion> =
        serde_wasm_bindgen::from_value(value).expect("an empty array is still an array");
    assert!(items.is_empty());

    let capped = session.complete("", "a", 0).expect("no throw");
    let capped: Vec<ReceivedCompletion> = serde_wasm_bindgen::from_value(capped).expect("shape");
    assert!(capped.is_empty());
}
