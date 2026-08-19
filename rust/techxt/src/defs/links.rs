//! Hyperlinks (PLAN.md §9.8): `\url` and `\href`.
//!
//! A URL is the one place in a LaTeX document where the escape character and the
//! comment character are ordinary text: `https://ex.org/a_b#c%20d` is full of
//! characters that would otherwise start a macro, a subscript or a comment. Both
//! macros therefore take the URL as a **verbatim** argument — techy reads it as raw
//! characters, with markup switched off for its extent — which is also why the URL
//! survives into the output as one unbreakable run that no line wrap can cut.
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! assert_eq!(
//!     converter.latex_to_text(r"\href{https://ex.org/a_b}{link}")?.text,
//!     "link <https://ex.org/a_b>\n",
//! );
//! # Ok::<(), techy::error::ParseError<Option<String>>>(())
//! ```

use crate::def::{Category, MacroDef, Template, TextRule};

/// The links category (PLAN.md §12.1).
pub fn category() -> Category {
    Category::new("links")
        .with_macro(
            MacroDef::new("url")
                .arg(VERBATIM_BRACES, "url")
                .rule(TextRule::Template(Template::new("<{url}>"))),
        )
        .with_macro(
            MacroDef::new("href")
                .arg(VERBATIM_BRACES, "url")
                .arg("m", "text")
                .rule(TextRule::Template(Template::new("{text} <{url}>"))),
        )
        // `\nolinkurl` prints the URL without making it a link, which in plain text is
        // the same thing.
        .with_macro(
            MacroDef::new("nolinkurl")
                .arg(VERBATIM_BRACES, "url")
                .rule(TextRule::Template(Template::new("<{url}>"))),
        )
        // `\path{…}` (the `url` package) sets a file name, not a link: it prints what
        // it was given, and the angle brackets of a URL would be an invention.
        .with_macro(
            MacroDef::new("path")
                .arg(VERBATIM_BRACES, "path")
                .rule(TextRule::Template(Template::new("{path}"))),
        )
}

/// techy's argument code for "verbatim, delimited by braces": the `v` reader with the
/// delimiter pair written out.
const VERBATIM_BRACES: &str = "v{}";
