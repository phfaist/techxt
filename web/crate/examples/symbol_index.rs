//! Print every name the shipped definitions define, as JSON on stdout.
//!
//! `web/tools/mathjax_coverage.mjs` is the consumer: it needs the whole symbol table to
//! ask MathJax about each name in turn, and this is the cheapest honest way to get a
//! Rust data structure into a Node script. Run it with
//!
//! ```sh
//! cd web/crate && cargo run --quiet --example symbol_index
//! ```
//!
//! It reads `techxt::defs::standard()` through `DefinitionSet::symbols()` (root PLAN.md
//! §10.7) — the same table the completion chip row is drawn from (web/PLAN.md §4.9) — so
//! what the checker measures is exactly what the app offers a user, resolved the way the
//! converter itself resolves it.
//!
//! An example rather than a binary: nothing ships it, `cargo run --example` is the whole
//! interface, and `web/crate` is a `cdylib` whose only product is the wasm module.
//!
//! The JSON is written by hand rather than through serde. This crate does not depend on
//! `serde_json`, the shape is six fields of a flat array, and a dependency added to a
//! module that is size-budgeted (web/PLAN.md §4.7) to serve a development tool would be
//! the wrong trade even though an example never reaches `dist/`.

use techxt::def::{CallableKind, ModeVisibility};

fn main() {
    let definitions = techxt::defs::standard();
    let symbols = definitions.symbols();

    let mut out = String::from("[\n");
    for (index, entry) in symbols.entries().iter().enumerate() {
        if index > 0 {
            out.push_str(",\n");
        }
        out.push_str("  {\"name\":");
        quote(&mut out, entry.name);
        out.push_str(",\"kind\":\"");
        out.push_str(match entry.kind {
            CallableKind::Macro => "macro",
            CallableKind::Environment => "environment",
            CallableKind::Specials => "specials",
        });
        out.push_str("\",\"category\":");
        quote(&mut out, entry.category);
        out.push_str(",\"replacement\":");
        match entry.replacement {
            Some(replacement) => quote(&mut out, replacement),
            None => out.push_str("null"),
        }
        out.push_str(",\"arity\":");
        out.push_str(&entry.arity.to_string());
        out.push_str(",\"modes\":\"");
        out.push_str(match entry.modes {
            ModeVisibility::Anywhere => "anywhere",
            ModeVisibility::TextOnly => "text",
            ModeVisibility::MathOnly => "math",
        });
        out.push_str("\"}");
    }
    out.push_str("\n]\n");
    print!("{out}");
}

/// Append `text` as a JSON string literal, escapes and all.
///
/// Names and replacements are arbitrary text — a specials is spelled `"` or `\\`, a
/// replacement can be any character techxt renders — so this escapes what JSON requires
/// and nothing else: the two structural characters, the control range, and the five
/// short forms a reader recognises.
fn quote(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control < ' ' => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            ordinary => out.push(ordinary),
        }
    }
    out.push('"');
}
