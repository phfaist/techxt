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
//! # Status: this module is milestone M3's
//!
//! PLAN.md §5 gives `techxt::def` a much larger public face than what is here:
//! `MacroDef`, `EnvDef`, `SpecialsDef` and their builders, `Category`, `DefinitionSet`,
//! and the template parser. Those are the definitions-infrastructure milestone's
//! deliverable. What this module holds today is exactly the subset the renderer needs in
//! order to *execute* rules — the rule model itself, the handler trait, the spec types
//! that carry a rule into the tree — so that M3 fills in the authoring side without
//! reshaping the execution side.

mod rule;
mod set;
mod spec;
mod template;

pub use rule::{CallableKind, TextHandler, TextRule};
pub use spec::{EnvBodyKind, TechxtEnvironmentBehavior, TechxtMacroSpec, TechxtSpecialsSpec};
pub use template::{Template, TemplateError};

pub(crate) use set::RuleTable;
pub(crate) use spec::embedded_rule;
pub(crate) use template::{ArgRef, Seg};
