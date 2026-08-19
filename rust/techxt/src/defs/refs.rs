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
//!
//! Resolving a reference to the thing it names would take a second pass — collect every
//! `\label` and the number of the construct it sits in, then convert again with that
//! table in hand — and a citation would need the bibliography besides. Both are
//! deliberate omissions of this version (PLAN.md §17), not oversights: an embedder that
//! has the numbers can supply them today by overriding these rules
//! ([`ConverterBuilder::override_macro`](crate::ConverterBuilder::override_macro)).
//!
//! # The bibliography
//!
//! `thebibliography` renders its entries, one paragraph each. Two things it does in
//! LaTeX are deliberately not done here: it prints no heading — which of "References"
//! and "Bibliography" LaTeX prints depends on the document class, and techxt discards
//! the class along with the rest of the preamble — and it numbers no entry, for the
//! same reason `\cite` resolves to `<cit.>` rather than to `[7]`.

use alloc::borrow::Cow;

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::def::{Category, EnvDef, MacroDef, TextHandler, TextRule};
use crate::flow::{Flow, FlowItem};
use crate::render::{RenderCx, RenderError};

use super::handler;

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

    // The bibliography. Its argument is the widest label, which sizes an indent that
    // plain text does not have; the entries are what matters, and `\bibitem` is what
    // keeps them apart.
    category.add_env(
        EnvDef::new("thebibliography")
            .arg("m", "widest")
            .rule(TextRule::Content),
    );
    category.add_macro(
        MacroDef::new("bibitem")
            .arg("o", "label")
            .arg("m", "key")
            .rule(handler(EntryBreak)),
    );
    // biblatex prints the bibliography from a database techxt cannot read.
    category.add_macro(
        MacroDef::new("printbibliography")
            .arg("o", "options")
            .rule(TextRule::Skip),
    );

    category
}

/// `\bibitem`: the start of one bibliography entry.
///
/// Neither the key nor the optional label is rendered — a key is not text, and a label
/// techxt cannot resolve a `\cite` to is not worth printing next to one it cannot
/// either. What the entry needs is to begin on its own, which is what a paragraph
/// break gives it.
#[derive(Debug)]
struct EntryBreak;

impl TextHandler for EntryBreak {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        _cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut flow = Flow::new();
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}

/// A reference macro: one label argument, one fixed marker.
fn reference(name: &str, marker: &'static str) -> MacroDef {
    MacroDef::new(name)
        .arg("m", "label")
        .rule(TextRule::Literal(Cow::Borrowed(marker)))
}
