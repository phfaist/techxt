//! The default definitions library: what techxt knows about LaTeX out of the box
//! (PLAN.md §12).
//!
//! Everything here is *data* in the sense that matters: one [`Category`](crate::def::Category) per theme,
//! each built from the entry builders of [`techxt::def`](crate::def), each usable on
//! its own. [`standard`] stacks all of them into the [`DefinitionSet`] that
//! [`Converter::standard`](crate::Converter::standard) uses.
//!
//! # Taking only part of it
//!
//! A converter that should know about accents and headings and nothing else is
//! assembled from the modules directly:
//!
//! ```
//! use techxt::def::DefinitionSet;
//! use techxt::{defs, Converter};
//!
//! let mut definitions = DefinitionSet::new();
//! definitions.push(defs::base::category());
//! definitions.push(defs::accents::category());
//! definitions.push(defs::sectioning::category());
//!
//! let converter = Converter::builder().definitions(definitions).build()?;
//! assert_eq!(converter.latex_to_text(r"\'e")?.text, "é\n");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! There are no cargo features to pick between: a build that never names a module
//! drops it through dead-code elimination.
//!
//! # How much LaTeX this covers
//!
//! Everything standard: the LaTeX2e kernel's own commands and environments, the
//! `amsmath` and `amssymb` families, and the packages a paper reaches for without
//! thinking — `graphicx`, `hyperref`, `natbib`, `booktabs`, `multirow`, `longtable`,
//! `verbatim`. That is what PLAN.md §12.2 asks for: nothing standard should reach the
//! unknown policy out of the box, so an ordinary document converts without a single
//! diagnostic. Where a construct has no plain-text meaning at all — a font size, a
//! page break, a counter's value — it is still *declared*, which is what keeps its
//! arguments out of the reader's paragraph, and it renders as nothing without
//! complaining.
//!
//! Three things a document may contain are knowingly left out, and each reports
//! itself when met:
//!
//! - **TeX's conditionals** (`\if…\else\fi`), and with them category codes, registers
//!   and counters. Macro *definition* is no longer on this list — [`preamble`] seeds
//!   techy-xp's definers, so `\newcommand` and `\def` are honoured (PLAN.md §16 M9) —
//!   but a conditional is outside techy-xp's scope by design, and reaching one raises
//!   `techy-xp.presets.conditional-unsupported` and renders as nothing.
//! - **`tabbing`**, whose `\=` and `\>` collide with the accent macros of
//!   [`accents`] — it would have to be a mode of its own, and it is rare in documents
//!   written this century.
//! - **Package-specific environments** beyond the list above (`IEEEeqnarray`,
//!   `algorithmic`, …). Adding one is a `Category` of your own, pushed after
//!   [`standard`]; see the crate documentation's "definitions of your own".
//!
//! # Order is meaning
//!
//! [`DefinitionSet`] resolves **later categories first** (see its documentation), so
//! the order [`standard`] pushes in is part of the specification: the generated
//! long tail of symbols goes in first so that every curated category can correct it,
//! and a category of your own pushed after [`standard`]'s output overrides anything
//! the library shipped.

use crate::def::DefinitionSet;

pub mod accents;
mod accents_data;
pub mod base;
pub mod fontstyles;
pub mod graphics;
pub mod inputs;
pub mod links;
pub mod mathcore;
pub mod mathenvs;
pub mod natbib;
pub mod preamble;
pub mod refs;
pub mod sectioning;
pub mod subsuperscripts;
pub mod symbols_extra;
pub mod titling;

pub mod lists;
pub mod tables;
pub mod theorems;
pub mod verbatim;

/// Every category the library ships, stacked in PLAN.md §12.1's order.
///
/// This is what [`Converter::standard`](crate::Converter::standard) builds from. The
/// order is normative and **later categories shadow earlier ones**: `symbols_extra`
/// (the generated long tail) goes in first precisely so that every curated category
/// after it can correct an entry, and `natbib` goes in last so that its citation
/// commands win over the plain ones in [`refs`].
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::builder().definitions(techxt::defs::standard()).build()?;
/// assert_eq!(converter.latex_to_text(r"\textbf{hi}")?.text, "𝐡𝐢\n");
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub fn standard() -> DefinitionSet {
    DefinitionSet::new()
        .with(symbols_extra::category())
        .with(base::category())
        .with(accents::category())
        .with(fontstyles::category())
        .with(mathcore::category())
        .with(mathenvs::category())
        .with(subsuperscripts::category())
        .with(sectioning::category())
        .with(lists::category())
        .with(verbatim::category())
        .with(tables::category())
        .with(theorems::category())
        .with(refs::category())
        .with(links::category())
        .with(graphics::category())
        .with(titling::category())
        .with(preamble::category())
        .with(inputs::category())
        .with(natbib::category())
}

/// Wrap a handler in the rule that runs it.
///
/// Every category in this module reaches for it, and spelling out
/// `TextRule::Handler(Arc::new(…))` at each of several dozen call sites hides what the
/// definition table says.
fn handler(handler: impl crate::def::TextHandler + 'static) -> crate::def::TextRule {
    crate::def::TextRule::Handler(alloc::sync::Arc::new(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Converter;
    use alloc::collections::BTreeSet;
    use alloc::string::String;
    use alloc::vec::Vec;

    #[test]
    fn the_standard_set_builds() {
        let converter = Converter::builder()
            .definitions(standard())
            .build()
            .expect("the shipped definitions are well-formed");
        assert_eq!(converter.latex_to_text("hi").expect("parses").text, "hi\n");
    }

    #[test]
    fn the_categories_are_pushed_in_the_plans_order() {
        let definitions = standard();
        let names: Vec<&str> = definitions.categories().map(|c| c.name()).collect();
        assert_eq!(
            names,
            [
                "symbols_extra",
                "base",
                "accents",
                "fontstyles",
                "mathcore",
                "mathenvs",
                "subsuperscripts",
                "sectioning",
                "lists",
                "verbatim",
                "tables",
                "theorems",
                "refs",
                "links",
                "graphics",
                "titling",
                "preamble",
                "inputs",
                "natbib",
            ]
        );
    }

    #[test]
    fn category_names_are_unique() {
        let names: Vec<String> = standard()
            .categories()
            .map(|c| String::from(c.name()))
            .collect();
        let unique: BTreeSet<&String> = names.iter().collect();
        assert_eq!(names.len(), unique.len());
    }

    #[test]
    fn converter_standard_uses_the_shipped_library() {
        // Not the placeholder set any more: a heading numbers itself.
        let converted = Converter::standard()
            .latex_to_text(r"\section{Intro}")
            .expect("parses");
        assert_eq!(converted.text, "1 Intro\n-------\n");
    }
}
