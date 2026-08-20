//! The wasm binding behind the techxt web app (web/PLAN.md §4).
//!
//! Three exports and no more: a [`Session`], its [`convert`](Session::convert) method,
//! and [`techxt_version`]. Everything else this crate contains is in service of the
//! two data shapes that cross the boundary — [`options::OptionsDto`] coming in and
//! [`diag::ConversionResultDto`] going out — both of which are declared normatively in
//! `web/src/worker/protocol.ts`.
//!
//! # It is app-private
//!
//! This crate is shaped for one user interface and is free to change without a release.
//! It is deliberately *not* the published `js/` package the root PLAN §17 anticipates;
//! if that ever exists, `web/` depends on it and this folder is deleted, which is one
//! of the reasons the binding surface is kept this small.
//!
//! # Where the work is
//!
//! Almost none of it is here. [`options::build`] maps the DTO onto a
//! [`ConverterBuilder`](techxt::convert::ConverterBuilder), [`diag::convert_native`]
//! converts a document and shapes the answer, and both are ordinary Rust that
//! `cargo test` exercises natively. This module adds the three things that genuinely
//! need a browser: the `JsValue` boundary, the panic hook, and the clock.

pub mod diag;
pub mod options;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use techxt::Converter;

use crate::options::OptionsDto;

/// A converter, kept alive across conversions (web/PLAN.md §4.1).
///
/// The app creates one at load and converts with it on every keystroke. It holds the
/// [`Converter`] together with the options it was built from, and rebuilds only when
/// those change: a rebuild costs 1.2–1.9 ms natively (PLAN §14), so this is a small
/// optimisation rather than a necessary one — but it makes typing under fixed options
/// cost exactly one `latex_to_text` call.
///
/// The options are kept in full rather than hashed. They are fourteen small fields, the
/// comparison is over as soon as one differs, and an exact fingerprint cannot collide —
/// which a hash could, and the symptom would be output that silently ignores an option
/// the user just changed.
#[wasm_bindgen]
pub struct Session {
    /// The converter and the options it was built from, or `None` until the first
    /// conversion. Building is deferred so that the constructor cannot fail: a
    /// `BuildError` is a bug in techxt's own definitions, and the constructor has
    /// nowhere to report one.
    built: Option<(OptionsDto, Converter)>,
}

#[wasm_bindgen]
impl Session {
    /// Create a session. Installs the panic hook, and builds nothing yet.
    ///
    /// `console_error_panic_hook` is installed in release builds too: a panic leaves
    /// the wasm instance unusable and the worker respawns it (PLAN §6.2), and a panic
    /// report from a real user's document is worth the ~10 KB it costs.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session {
        console_error_panic_hook::set_once();
        Session { built: None }
    }

    /// Convert `latex` under `options` (a plain JavaScript object), and answer a
    /// `ConversionResult`.
    ///
    /// It does **not** throw for a document-level failure: a strict-mode parse error,
    /// or the descent guard refusing a pathologically nested document, comes back as a
    /// result with `ok: false` and exactly one error diagnostic. The `Err` case is
    /// reserved for the two things that are the *caller's* mistake or the binding's:
    /// an options object this build does not understand — an unknown field, or an enum
    /// string that is not one of the values `protocol.ts` lists — and a failure to
    /// serialize the answer.
    ///
    /// `null` and `undefined` are accepted for `options` and mean "the library's
    /// defaults", which is the same thing an empty object means.
    pub fn convert(&mut self, latex: &str, options: JsValue) -> Result<JsValue, JsValue> {
        let dto: OptionsDto = if options.is_undefined() || options.is_null() {
            OptionsDto::default()
        } else {
            serde_wasm_bindgen::from_value(options).map_err(|error| {
                JsError::new(&format!(
                    "techxt: this options object is not one this build understands: {error}"
                ))
            })?
        };

        let rebuild = match &self.built {
            Some((built_from, _)) => *built_from != dto,
            None => true,
        };
        if rebuild {
            let converter = options::build(&dto).map_err(|error| JsError::new(&error))?;
            self.built = Some((dto, converter));
        }
        let (_, converter) = self
            .built
            .as_ref()
            .expect("the converter was just built or already there");

        let started = now_ms();
        let mut result = diag::convert_native(converter, latex);
        result.ms = now_ms() - started;

        // `serialize_missing_as_null` so that a diagnostic outside the buffer arrives as
        // `span: null` — what `protocol.ts` declares — rather than as serde-wasm-bindgen's
        // default `undefined`, which would survive `JSON.stringify` differently and read
        // as a missing key rather than an absent span.
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
        result.serialize(&serializer).map_err(|error| {
            JsError::new(&format!("techxt: could not serialize the result: {error}")).into()
        })
    }
}

impl Default for Session {
    fn default() -> Session {
        Session::new()
    }
}

/// The version of the embedded techxt, for the About section and for bug reports.
///
/// It is the `rust/` workspace version, read out of `rust/Cargo.toml` at build time by
/// `build.rs` — the same number the crate and the CLI carry.
#[wasm_bindgen]
pub fn techxt_version() -> String {
    env!("TECHXT_VERSION").to_string()
}

// `performance.now()`, declared by hand.
//
// The obvious `js_sys::Date::now()` would mean a dependency, and the binding's
// dependency list is a design property rather than an implementation detail (PLAN
// Appendix D). `wasm-bindgen` can declare the import on its own, and `performance` is a
// global in a worker as it is in a window.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

/// The clock, in milliseconds, for [`ConversionResultDto::ms`](diag::ConversionResultDto::ms).
///
/// `std::time::Instant` panics on `wasm32-unknown-unknown` — there is no clock below
/// the host — so the browser's own is used instead. Off wasm there is no host to ask
/// and no `ms` field anyone reads, so `cargo test` gets a constant and a conversion
/// that reports zero milliseconds.
fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}
