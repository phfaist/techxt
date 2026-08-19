//! natbib's citation commands (PLAN.md §9.8, §12.1).
//!
//! natbib replaces LaTeX's one `\cite` with a family of about twenty, distinguished by
//! how the reference is *typeset*: `\citet{x}` writes "Author (2019)", `\citep{x}`
//! writes "(Author, 2019)", `\citeauthor{x}` writes just the name, `\citeyear{x}` just
//! the year, and each has a starred form (spell out every author), a capitalized form
//! (start of a sentence) and up to two optional notes (`\citep[see][p. 5]{x}` →
//! "(see Author, 2019, p. 5)").
//!
//! **All of it needs the bibliography.** Author, year and the ordering of the reference
//! list come out of a BibTeX run that a plain-text conversion never performs, so every
//! one of these renders as `<cit.>` — the same honest marker
//! [`defs::refs`](super::refs) uses, saying that a citation stood here (PLAN.md §9.8).
//!
//! This category is pushed **last** (PLAN.md §12.1) so that its fuller shapes shadow
//! the plain `\cite` of [`defs::refs`](super::refs). That is not cosmetic: `refs`
//! declares `\cite` as `[m]`, and against `\citep[see][p. 5]{x}` such a declaration
//! reads `[see]` as the note and then spills `[p. 5]` into the reader's sentence.
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! assert_eq!(
//!     converter.latex_to_text(r"As shown in \citet*[chap.~2]{knuth1984}.")?.text,
//!     "As shown in <cit.>.\n",
//! );
//! # Ok::<(), techy::error::ParseError<Option<String>>>(())
//! ```

use alloc::borrow::Cow;

use crate::def::{Category, MacroDef, TextRule};

/// What every citation command renders as (PLAN.md §9.8).
const MARKER: &str = "<cit.>";

/// techy's code for a mandatory group with **no** single-expression fallback.
///
/// A citation key list is not LaTeX code, and `\cite key` is not a citation with the
/// key `k` followed by the letters `ey` — it is a mistake. `m` would silently take the
/// first token; `BracedOnly` declines, which is the answer that leaves a diagnostic
/// behind instead of a mangled sentence.
const KEY_LIST: &str = "BracedOnly";

/// The natbib category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("natbib");

    // The full shape: `\citet*[pre][post]{keys}`. The capitalized forms are natbib's
    // sentence-initial variants ("Author (2019)" → "Author (2019)" with the "van" in
    // "van Dijk" capitalized), which plain text cannot show either.
    for name in [
        "cite",
        "citet",
        "citep",
        "citealt",
        "citealp",
        "citeauthor",
        "citefullauthor",
        "Citet",
        "Citep",
        "Citealt",
        "Citealp",
        "Citeauthor",
        "Citefullauthor",
    ] {
        category.add_macro(citation(MacroDef::new(name).star()));
    }

    // No starred form: there is no "all authors" to spell out in a year, and an alias
    // is whatever `\defcitealias` said it was.
    for name in ["citeyear", "citeyearpar", "citetalias", "citepalias"] {
        category.add_macro(citation(MacroDef::new(name)));
    }

    // `\citenum` prints the bare number of the entry in the reference list, with no
    // brackets and no notes.
    category.add_macro(
        MacroDef::new("citenum")
            .arg(KEY_LIST, "keys")
            .rule(marker()),
    );

    // The exception in the family: `\citetext`'s argument is the *text* to place inside
    // the citation delimiters ("\citep[see][]{x}" written the long way round), not a
    // list of keys. It is the author's own prose, and the reason this category exists
    // is to stop such prose being thrown away, so it is rendered.
    category.add_macro(
        MacroDef::new("citetext")
            .arg("m", "text")
            .rule(TextRule::Content),
    );

    // A declaration, like `\label`: it names a citation alias for `\citetalias` to use
    // later, and prints nothing where it stands.
    category.add_macro(
        MacroDef::new("defcitealias")
            .arg(KEY_LIST, "keys")
            .arg("m", "alias")
            .rule(TextRule::Skip),
    );

    // `\shortcites{keys}` asks natbib to abbreviate one entry's author list. It is a
    // setting, and prints nothing.
    category.add_macro(
        MacroDef::new("shortcites")
            .arg(KEY_LIST, "keys")
            .rule(TextRule::Skip),
    );

    category
}

/// Finish a citation command: two optional notes, the key list, and the marker.
///
/// The notes are *declared and discarded*. Declaring them is what keeps
/// `\citep[see][p. 5]{x}` from leaving `[p. 5]` behind in the paragraph — the whole
/// point of this category — and discarding them keeps the output honest: with no
/// bibliography there is no "(see Author, 2019, p. 5)" for a note to be part of, and a
/// bare "see" or "p. 5" next to `<cit.>` reads as a sentence fragment.
fn citation(definition: MacroDef) -> MacroDef {
    definition
        .arg("o", "pre")
        .arg("o", "post")
        .arg(KEY_LIST, "keys")
        .rule(marker())
}

/// The rule every citation command shares.
fn marker() -> TextRule {
    TextRule::Literal(Cow::Borrowed(MARKER))
}
