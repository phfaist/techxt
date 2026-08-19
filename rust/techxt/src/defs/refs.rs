//! Cross-references and citations (PLAN.md §9.8).
//!
//! A reference's *text* is the number LaTeX would have resolved it to, and nothing in a
//! plain-text conversion knows that number: it comes from the `.aux` file of a run that
//! never happened. So techxt writes a marker — `<ref>`, `<Ref>`, `<cit.>` — which says
//! honestly that a reference stood here, keeps the sentence readable, and is easy to
//! find in the output.
//!
//! `\label` renders as nothing at all, but its argument is still *declared*, which is
//! the point: a label key is not text, and a definition that did not declare the
//! argument would leave `{eq:main}` in the reader's paragraph.

use alloc::borrow::Cow;

use crate::def::{Category, MacroDef, TextRule};

/// The refs category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("refs");

    // The reference family. `\vref` and `\autoref` add words of their own in LaTeX
    // ("section 3 on page 5"); with no resolved number there is nothing to add them to.
    for name in ["ref", "autoref", "cref", "vref", "pageref"] {
        category.add_macro(reference(name, "<ref>"));
    }
    // `\Cref` is the capitalized form, for the start of a sentence.
    category.add_macro(reference("Cref", "<Ref>"));
    // An equation reference carries its own parentheses.
    category.add_macro(reference("eqref", "(<ref>)"));

    // Citations. natbib's fuller shapes are defined in `defs::natbib`, which is pushed
    // after this category and therefore shadows these.
    for name in ["cite", "citet", "citep"] {
        category.add_macro(
            MacroDef::new(name)
                .arg("o", "note")
                .arg("m", "keys")
                .rule(TextRule::Literal(Cow::Borrowed("<cit.>"))),
        );
    }

    // Declared, parsed, and rendered as nothing.
    for name in ["label", "nocite"] {
        category.add_macro(MacroDef::new(name).arg("m", "key").rule(TextRule::Skip));
    }

    category
}

/// A reference macro: one label argument, one fixed marker.
fn reference(name: &str, marker: &'static str) -> MacroDef {
    MacroDef::new(name)
        .arg("m", "label")
        .rule(TextRule::Literal(Cow::Borrowed(marker)))
}
