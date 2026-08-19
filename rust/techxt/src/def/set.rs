//! The name-keyed rule tables (PLAN.md §10.3 steps 1 and 3).
//!
//! **M3 seam.** PLAN.md §10 gives this module a public face — `MacroDef`, `EnvDef`,
//! `SpecialsDef`, `Category`, `DefinitionSet` and their builders — which is the
//! definitions-infrastructure milestone's deliverable. What exists here now is the one
//! piece the renderer cannot work without: the lookup table it consults at dispatch
//! steps 1 and 3, and which the converter fills in.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use super::{CallableKind, TextRule};

/// Rules keyed by `(kind, name)`, as PLAN.md §10.3 keys both the override map and the
/// name fallback table.
///
/// Macros, environments and specials live in separate stores because their names do:
/// techy resolves `\emph` and an environment called `emph` through different tables,
/// and a specials is keyed by its trigger characters rather than by a name at all.
#[derive(Clone, Debug, Default)]
pub(crate) struct RuleTable {
    macros: BTreeMap<Box<str>, TextRule>,
    environments: BTreeMap<Box<str>, TextRule>,
    specials: BTreeMap<Box<str>, TextRule>,
}

impl RuleTable {
    /// An empty table.
    pub(crate) fn new() -> RuleTable {
        RuleTable::default()
    }

    /// Record a rule, replacing any rule already keyed the same way.
    pub(crate) fn insert(&mut self, kind: CallableKind, name: impl Into<Box<str>>, rule: TextRule) {
        self.store_mut(kind).insert(name.into(), rule);
    }

    /// The rule for this construct, if the table has one.
    pub(crate) fn get(&self, kind: CallableKind, name: &str) -> Option<&TextRule> {
        self.store(kind).get(name)
    }

    fn store(&self, kind: CallableKind) -> &BTreeMap<Box<str>, TextRule> {
        match kind {
            CallableKind::Macro => &self.macros,
            CallableKind::Environment => &self.environments,
            CallableKind::Specials => &self.specials,
        }
    }

    fn store_mut(&mut self, kind: CallableKind) -> &mut BTreeMap<Box<str>, TextRule> {
        match kind {
            CallableKind::Macro => &mut self.macros,
            CallableKind::Environment => &mut self.environments,
            CallableKind::Specials => &mut self.specials,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::borrow::Cow;

    #[test]
    fn stores_are_separate_per_kind() {
        let mut table = RuleTable::new();
        table.insert(CallableKind::Macro, "x", TextRule::Skip);
        table.insert(
            CallableKind::Environment,
            "x",
            TextRule::Literal(Cow::Borrowed("env")),
        );
        assert!(matches!(
            table.get(CallableKind::Macro, "x"),
            Some(TextRule::Skip)
        ));
        assert!(matches!(
            table.get(CallableKind::Environment, "x"),
            Some(TextRule::Literal(_))
        ));
        assert!(table.get(CallableKind::Specials, "x").is_none());
    }

    #[test]
    fn later_insert_replaces() {
        let mut table = RuleTable::new();
        table.insert(CallableKind::Macro, "x", TextRule::Skip);
        table.insert(CallableKind::Macro, "x", TextRule::Content);
        assert!(matches!(
            table.get(CallableKind::Macro, "x"),
            Some(TextRule::Content)
        ));
    }
}
