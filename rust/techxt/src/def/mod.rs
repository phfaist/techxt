//! Definitions: what techxt knows about a construct (PLAN.md §10).
//!
//! A techxt definition says two things at once about one construct: **how it parses**
//! (how many arguments it takes and of what shape) and **how it renders** (its
//! [`TextRule`]). Both halves travel together into the parsed tree, inside the spec
//! object techy stamps on every callable node, so the parser and the renderer read the
//! same declaration and cannot disagree about it. That is techxt's answer to
//! pylatexenc's two independent databases, whose divergence is a documented source of
//! dropped arguments and index errors.
//!
//! At render time a construct's rule is found through the dispatch chain of PLAN.md
//! §10.3, in this order:
//!
//! 1. the converter's **override map** — `ConverterBuilder::override_macro` and friends,
//!    keyed by [`CallableKind`] and name;
//! 2. the **rule embedded in the node's spec** — recovered by downcasting, and the
//!    reason a definition can never drift from what was parsed;
//! 3. the **name fallback table**, which is what makes techxt work on a tree parsed by
//!    someone else's definitions;
//! 4. the **unknown-construct policy** from [`Options`](crate::Options), which also
//!    raises a diagnostic.
//!
//! # Writing definitions
//!
//! A definition is authored as one entry — [`MacroDef`], [`EnvDef`] or [`SpecialsDef`] —
//! grouped into a [`Category`], and categories are stacked into a [`DefinitionSet`]
//! that a converter is built from. **Later categories shadow earlier ones**; see
//! [`DefinitionSet`] for why that matters.
//!
//! ```
//! use techxt::def::{Category, DefinitionSet, MacroDef, TextRule};
//! use techxt::Converter;
//!
//! let mut definitions = DefinitionSet::new();
//! definitions.push(
//!     Category::new("mine")
//!         .with_macro(MacroDef::symbol("me", "Philippe"))
//!         .with_macro(MacroDef::new("shout").arg("m", "text").rule(TextRule::Content)),
//! );
//!
//! let converter = Converter::builder().definitions(definitions).build()?;
//! assert_eq!(converter.latex_to_text(r"\shout{hi} \me")?.text, "hi Philippe\n");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

mod entry;
mod rule;
mod set;
mod spec;
mod template;

pub use entry::{EnvDef, MacroDef, SpecialsDef};
pub use rule::{CallableKind, TextHandler, TextRule};
pub use set::{Category, DefinitionSet};
pub use spec::{EnvBodyKind, TechxtEnvironmentBehavior, TechxtMacroSpec, TechxtSpecialsSpec};
pub use template::{Template, TemplateError};

pub(crate) use set::{BuiltDefinitions, RuleTable};
pub(crate) use spec::embedded_rule;
pub(crate) use template::{ArgRef, Seg, TemplateScope};
