//! Categories, definition sets, and what building one produces (PLAN.md §10.2).
//!
//! A [`Category`] is a themed group of definitions — the accents, the list
//! environments, the maths symbols — and a [`DefinitionSet`] is an ordered stack of
//! them. Building the set is what turns declarations into a parser: one techy `Package`
//! per category, the parsing state those packages make, and the name-keyed table the
//! renderer falls back on for constructs whose spec carries no rule.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use alloc::vec::Vec;

use techy::core::specs::{CallableSpec, FallbackProvider, Package, ScopeOp, SpecsProvider};
use techy::core::{FinalizeError, ParsingState, ParsingStateDelta};
use techy::latexlike::{builtin_package, CallableType, Latexlike, MacroSpec};

use crate::convert::BuildError;

use super::entry::{EnvDef, MacroDef, SpecialsDef};
use super::spec::{
    EnvBodyKind, SpecBuildCx, TechxtEnvironmentBehavior, TechxtMacroSpec, TechxtSpecialsSpec,
};
use super::template::TemplateScope;
use super::{CallableKind, TextRule};

/// The name of the package a list environment's body delta pushes, holding `\item`.
const ITEM_PACKAGE: &str = "techxt-item";

/// The name of the provider that answers every otherwise-unresolvable command.
const UNKNOWN_PACKAGE: &str = "techxt-unknown";

/// The macro whose definition a list body brings into scope.
const ITEM_MACRO: &str = "item";

/// A themed group of definitions (PLAN.md §10.1).
///
/// Categories are how a definitions library is organized and how a user takes only part
/// of one: `techxt::defs` offers one category per theme, and a converter built from
/// three of them knows exactly those three. A category's name is how it identifies
/// itself to techy — it appears in parse diagnostics as the provider a definition came
/// from — and two categories in one set may not share it.
#[derive(Clone, Debug)]
pub struct Category {
    name: Box<str>,
    macros: Vec<MacroDef>,
    environments: Vec<EnvDef>,
    specials: Vec<SpecialsDef>,
}

impl Category {
    /// An empty category called `name`.
    pub fn new(name: impl Into<Box<str>>) -> Category {
        Category {
            name: name.into(),
            macros: Vec::new(),
            environments: Vec::new(),
            specials: Vec::new(),
        }
    }

    /// Add a macro definition.
    pub fn add_macro(&mut self, definition: MacroDef) {
        self.macros.push(definition);
    }

    /// Add an environment definition.
    pub fn add_env(&mut self, definition: EnvDef) {
        self.environments.push(definition);
    }

    /// Add a specials definition.
    pub fn add_specials(&mut self, definition: SpecialsDef) {
        self.specials.push(definition);
    }

    /// Add a macro definition, for chaining.
    pub fn with_macro(mut self, definition: MacroDef) -> Category {
        self.add_macro(definition);
        self
    }

    /// Add an environment definition, for chaining.
    pub fn with_env(mut self, definition: EnvDef) -> Category {
        self.add_env(definition);
        self
    }

    /// Add a specials definition, for chaining.
    pub fn with_specials(mut self, definition: SpecialsDef) -> Category {
        self.add_specials(definition);
        self
    }

    /// The category's name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Every definition a converter knows, in shadowing order (PLAN.md §10.2).
///
/// # Later categories shadow earlier ones
///
/// techy resolves a name through its scope stack **innermost first**, and each category
/// is pushed onto that stack in the order it was [`push`](Self::push)ed. A definition in
/// a later category therefore *wins* over one of the same name in an earlier category.
///
/// This is worth stating twice because it **inverts pylatexenc**, where the first
/// matching entry wins and a later table cannot override an earlier one. In techxt the
/// generated long tail of symbols goes in **first**, so that the curated categories
/// pushed after it can correct any of its entries; and a user's own category, pushed
/// last, overrides everything the library shipped.
///
/// ```
/// use techxt::def::{Category, DefinitionSet, MacroDef};
/// use techxt::Converter;
///
/// let mut definitions = DefinitionSet::new();
/// definitions.push(Category::new("generated").with_macro(MacroDef::symbol("ldots", "...")));
/// definitions.push(Category::new("curated").with_macro(MacroDef::symbol("ldots", "…")));
///
/// let converter = Converter::builder().definitions(definitions).build()?;
/// // The later category won.
/// assert_eq!(converter.latex_to_text(r"\ldots")?.text, "…\n");
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct DefinitionSet {
    categories: Vec<Category>,
}

impl DefinitionSet {
    /// An empty set, knowing nothing at all.
    pub fn new() -> DefinitionSet {
        DefinitionSet::default()
    }

    /// Add a category **on top of** the ones already pushed.
    ///
    /// Order is meaning: this category's definitions shadow the definitions of every
    /// category pushed before it (see the [type documentation](Self)).
    pub fn push(&mut self, category: Category) {
        self.categories.push(category);
    }

    /// Add a category, for chaining.
    pub fn with(mut self, category: Category) -> DefinitionSet {
        self.push(category);
        self
    }

    /// The categories, outermost (first pushed) first.
    pub fn categories(&self) -> impl Iterator<Item = &Category> {
        self.categories.iter()
    }

    /// Compile the set into what a converter needs (PLAN.md §10.2).
    ///
    /// Everything is validated here — argument codes, templates against the argument
    /// names of the entry they belong to, category names — so that a bad definition
    /// costs a `build()` call rather than showing up as a hole in somebody's converted
    /// document.
    ///
    /// Two things the *converter* decides reach the definitions here rather than being
    /// read back out of them: `cx` is what an entry's own
    /// [`CallableSpecSource`](super::CallableSpecSource) may consult (DECISIONS.md D15),
    /// and `unknown_macro_fallback` says whether the catch-all provider is registered.
    /// The set deliberately does not look at the recovery mode itself — the builder has
    /// already resolved that question (DECISIONS.md D10).
    pub(crate) fn build(
        &self,
        cx: &SpecBuildCx,
        unknown_macro_fallback: bool,
    ) -> Result<BuiltDefinitions, BuildError> {
        self.check_category_names()?;

        // The `\item` package is built first because every list environment's body
        // delta pushes it, and the behaviours that carry those deltas are built below.
        let item = self.item_provider(cx)?;

        let mut fallback = RuleTable::new();
        let mut providers: Vec<Arc<dyn SpecsProvider<Latexlike>>> =
            Vec::with_capacity(self.categories.len() + 2);

        // Outermost: the catch-all that gives an unresolvable command a spec, so that
        // it reaches the renderer as a callable node and techxt's unknown-macro policy
        // can apply to it (PLAN.md §10.6). Being outermost, it is shadowed by every
        // real definition — suppression is a property of the search order, not a
        // special case. Whether it is registered at all is the converter's call
        // (DECISIONS.md D10): without it an unclaimed command is techy's own
        // unresolvable-command error.
        if unknown_macro_fallback {
            providers.push(Arc::new(unknown_command_provider()));
        }
        // Then techy's own `\begin`/`\end`. The whole-stack replacement discards the
        // seed's copy of them, and they have to sit *above* the catch-all: below it,
        // the catch-all would answer for `\begin` and no environment would parse.
        providers.push(builtin_package());

        for category in &self.categories {
            providers.push(Arc::new(self.build_category(
                category,
                item.as_ref(),
                &mut fallback,
                cx,
            )?));
        }

        let delta = ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(providers));
        let state = ParsingState::<Latexlike>::lang_initial()?
            .derived(&delta)
            // A scope-op failure here is a techxt bug, not a caller's mistake — the op
            // is a whole-stack replacement, which targets no name and cannot miss — but
            // it still has to be reported rather than swallowed.
            .map_err(|error| {
                let message = alloc::format!("{error}");
                BuildError::State(
                    error
                        .finalize_error
                        .unwrap_or_else(|| FinalizeError::new(message)),
                )
            })?;

        Ok(BuiltDefinitions { state, fallback })
    }

    /// Refuse two categories with the same name: they would be indistinguishable in
    /// parse diagnostics, and one would silently shadow the other.
    fn check_category_names(&self) -> Result<(), BuildError> {
        let mut seen = BTreeSet::new();
        for category in &self.categories {
            if !seen.insert(category.name.clone()) {
                return Err(BuildError::DuplicateCategory(category.name.clone()));
            }
        }
        Ok(())
    }

    /// The package a list environment's body delta pushes, holding the set's own
    /// definition of `\item` (PLAN.md §9.4).
    ///
    /// The definition is the innermost one the set declares, exactly as name resolution
    /// would find it, so customizing `\item` customizes it inside every list too.
    /// `None` when the set defines no `\item`: a list body then simply has no `\item` of
    /// its own, and a top-level definition (or the unknown-macro policy) applies.
    fn item_provider(
        &self,
        cx: &SpecBuildCx,
    ) -> Result<Option<Arc<dyn SpecsProvider<Latexlike>>>, BuildError> {
        let Some(definition) = self.categories.iter().rev().find_map(|category| {
            category
                .macros
                .iter()
                .rev()
                .find(|def| def.name() == ITEM_MACRO)
        }) else {
            return Ok(None);
        };
        let mut package = Package::<Latexlike>::new(ITEM_PACKAGE);
        package.insert_in_modes(
            CallableType::Macro,
            ITEM_MACRO,
            macro_spec(definition, cx)?,
            definition.modes(),
        );
        Ok(Some(Arc::new(package)))
    }

    /// One category's techy package, recording its rules in the fallback table as it
    /// goes.
    fn build_category(
        &self,
        category: &Category,
        item: Option<&Arc<dyn SpecsProvider<Latexlike>>>,
        fallback: &mut RuleTable,
        cx: &SpecBuildCx,
    ) -> Result<Package<Latexlike>, BuildError> {
        let mut package = Package::<Latexlike>::new(category.name.clone());

        for definition in &category.macros {
            let spec = macro_spec(definition, cx)?;
            if let Some(rule) = definition.text_rule() {
                fallback.insert(CallableKind::Macro, definition.name(), rule.clone());
            }
            package.insert_in_modes(
                CallableType::Macro,
                definition.name(),
                spec,
                definition.modes(),
            );
        }

        for definition in &category.environments {
            let arguments = definition.arg_specs()?;
            let rule = validated_rule(
                definition.name(),
                definition.text_rule(),
                &definition.arg_names(),
                true,
            )?;
            let mut behavior =
                TechxtEnvironmentBehavior::new(arguments, definition.body_kind(), rule.clone());
            if let (EnvBodyKind::List(_), Some(item)) = (definition.body_kind(), item) {
                behavior = behavior.with_body_provider(Arc::clone(item));
            }
            if let Some(rule) = rule {
                fallback.insert(CallableKind::Environment, definition.name(), rule);
            }
            package.insert(
                CallableType::Environment,
                definition.name(),
                behavior.into_spec(),
            );
        }

        for definition in &category.specials {
            let arguments = definition.arg_specs()?;
            let rule = validated_rule(
                definition.trigger(),
                definition.text_rule(),
                &definition.arg_names(),
                false,
            )?;
            if let Some(rule) = rule.clone() {
                fallback.insert(CallableKind::Specials, definition.trigger(), rule);
            }
            package.insert_specials_in_modes(
                CallableType::Specials,
                definition.trigger(),
                TechxtSpecialsSpec::new(arguments, rule),
                definition.modes(),
            );
        }

        Ok(package)
    }
}

/// The techy spec one macro definition registers.
///
/// Normally that is techxt's own [`TechxtMacroSpec`], carrying the rule into the tree.
/// An entry with a [`spec`](MacroDef::spec) source may hand back a techy spec of its own
/// instead (DECISIONS.md D15) — its rule then travels no further than the name fallback
/// table, which is built from the same entry either way. The declarations are validated
/// in both cases: a template that names an argument the entry does not declare is a
/// build error whoever ends up parsing the invocation.
fn macro_spec(
    definition: &MacroDef,
    cx: &SpecBuildCx,
) -> Result<Arc<dyn CallableSpec<Latexlike>>, BuildError> {
    let arguments = definition.arg_specs()?;
    let rule = validated_rule(
        definition.name(),
        definition.text_rule(),
        &definition.arg_names(),
        false,
    )?;
    if let Some(source) = definition.spec_source() {
        if let Some(spec) = source.callable_spec(cx) {
            return Ok(spec);
        }
    }
    Ok(Arc::new(TechxtMacroSpec::new(arguments, rule)))
}

/// Check a rule's template against the entry it belongs to, and hand the rule back.
fn validated_rule(
    definition: &str,
    rule: Option<&TextRule>,
    names: &[Box<str>],
    has_body: bool,
) -> Result<Option<TextRule>, BuildError> {
    if let Some(TextRule::Template(template)) = rule {
        template
            .validate(TemplateScope::declared(names, has_body))
            .map_err(|error| BuildError::Template {
                definition: definition.into(),
                template: template.source().into(),
                error,
            })?;
    }
    Ok(rule.cloned())
}

/// The provider that answers every command no definition claims (PLAN.md §10.6).
///
/// Without it an unresolvable `\foo` never reaches the renderer as a construct at all:
/// under tolerant recovery techy cannot shape an invocation it has no definition for, so
/// it recovers `\foo` as literal characters *and* reports an error, and techxt's
/// unknown-macro policy — which is about a macro that parses but has no text rule —
/// would have nothing to act on. Answering with a generic, argument-less spec makes the
/// invocation parse, so the policy decides what the reader sees and the diagnostic is a
/// `techxt.unknown-macro` warning rather than a parse error.
///
/// The consequence is deliberate and documented on
/// [`Options::unknown_macro`](crate::Options::unknown_macro): the generic spec takes no
/// arguments, so `\foo{x}` is an unknown macro followed by an ordinary group, and `x`
/// survives under every policy.
fn unknown_command_provider() -> FallbackProvider<Latexlike> {
    let mut provider = FallbackProvider::<Latexlike>::new(UNKNOWN_PACKAGE);
    // One shared singleton for every unknown name: specs are de-keyed, so an unknown
    // macro costs no allocation of its own.
    let spec: Arc<dyn CallableSpec<Latexlike>> = Arc::new(MacroSpec::new(Vec::new()));
    provider.set(CallableType::Macro, spec);
    provider
}

/// What building a [`DefinitionSet`] produces (PLAN.md §10.2).
pub(crate) struct BuiltDefinitions {
    /// The parsing state the packages make, ready for `Language::new`.
    pub(crate) state: ParsingState<Latexlike>,
    /// Dispatch step 3: every rule, keyed by kind and name.
    pub(crate) fallback: RuleTable,
}

/// Rules keyed by `(kind, name)`, as PLAN.md §10.3 keys both the override map and the
/// name fallback table.
///
/// Macros, environments and specials live in separate stores because their names do:
/// techy resolves `\emph` and an environment called `emph` through different tables,
/// and a specials is keyed by its trigger characters rather than by a name at all.
///
/// The table is *mode-blind*, unlike techy's own resolution: a definition restricted to
/// math mode still contributes its rule here, because the table's job is to render a
/// tree somebody else parsed, whose nodes carry no techxt spec to ask.
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
