//! The techy spec types that carry a techxt [`TextRule`] into the parsed tree.
//!
//! This is what PLAN.md §1's second design rule — *one definition, both sides* — comes
//! down to in code. A techxt definition is registered with techy as a spec object that
//! declares the construct's arguments (so the parser shapes the invocation correctly)
//! *and* carries the text rule (so the renderer knows what to do with it). Parser and
//! renderer read the same object; they cannot disagree about how many arguments a macro
//! takes, which is exactly the failure mode pylatexenc's two independent databases have.
//!
//! techy's [`CallableSpec`] has [`Any`](core::any::Any) as a supertrait, so the
//! renderer recovers the concrete type by downcasting the node's spec. That is the
//! sanctioned identity mechanism (techy's own `\begin` composition uses it to find an
//! environment's behaviour), and it is step 2 of techxt's dispatch chain (PLAN.md
//! §10.3).
//!
//! Environments are different, and it matters: see [`TechxtEnvironmentBehavior`].

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;

use techy::core::constructs::{ConstructParser, EnvironmentBody};
use techy::core::node::NodeRef;
use techy::core::specs::{ArgumentSpec, CallableSpec, ScopeOp, SpecsProvider};
use techy::core::ParsingStateDelta;
use techy::error::ParseError;
use techy::latexlike::{
    EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec, LatexlikeLang, Mode,
    VerbatimBehavior,
};
use techy::serialize::SerializableObject;
use techy::source::SourceResolver;
use techy_xp::lang::{LatexlikeXp, DEFAULT_GLOBAL_SCOPE_NAME, DEFAULT_LOCAL_SCOPE_NAME};
use techy_xp::presets::RefusalSpec;

use crate::convert::MacroDefinitions;
use crate::render::ListKind;

use super::TextRule;

/// A macro definition as techy sees it: its arguments, plus the techxt rule that
/// renders it.
///
/// Argument parsing is left entirely to techy's standard machinery — declaring the
/// arguments is the whole of it, and everything else keeps its default behaviour.
#[derive(Debug)]
pub struct TechxtMacroSpec {
    arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
    rule: Option<TextRule>,
}

impl TechxtMacroSpec {
    /// A macro spec with these argument specs and this text rule.
    ///
    /// The rule is optional — pass `None` for a definition that says how the macro
    /// *parses* but not how it renders. Such a macro falls through to the name fallback
    /// table and then to
    /// [`Options::unknown_macro`](crate::Options::unknown_macro), which is exactly what
    /// a parse-only entry should do.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtMacroSpec {
        TechxtMacroSpec {
            arguments,
            rule: rule.into(),
        }
    }

    /// The text rule this macro renders through, if it has one.
    pub fn rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }
}

// The `SerializableObject` supertrait of `CallableSpec` is fully defaulted; an empty
// impl opts out of serialization, which techxt does not offer in v1 (PLAN.md §2).
impl SerializableObject<LatexlikeXp> for TechxtMacroSpec {}

impl CallableSpec<LatexlikeXp> for TechxtMacroSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LatexlikeXp>>] {
        &self.arguments
    }
}

/// Supplies a techy [`CallableSpec`] that techxt's own declarations cannot express
/// (DECISIONS.md D15).
///
/// [`MacroDef::arg_spec`](super::MacroDef::arg_spec) is the same escape hatch one level
/// down: it takes over one *argument* when a code cannot describe it. This one takes
/// over the whole spec, for a construct techy itself implements and techxt only wants
/// to register — the shipped case is `\input`, which techy resolves at parse time only
/// when the macro is registered with its bespoke
/// [`input_macro_spec`](techy::latexlike::input_macro_spec).
///
/// The spec is asked for once per converter, while the converter is being built, and
/// gets a [`SpecBuildCx`] describing what that converter is configured with — the only
/// moment at which both the definitions and the configuration are known. Answering
/// `None` means "nothing special after all": the macro then registers the ordinary spec
/// built from its own [`arg`](super::MacroDef::arg) declarations, which is how `\input`
/// stays inert when no source resolver was configured.
///
/// # A foreign spec carries no rule
///
/// techxt's own specs carry the construct's [`TextRule`] into the tree, and the
/// renderer recovers it by downcasting (dispatch step 2, PLAN.md §10.3). A foreign spec
/// cannot: it is not a [`TechxtMacroSpec`], so the downcast misses and the rule is found
/// at step 3 instead, in the name-keyed fallback table that
/// [`DefinitionSet`](super::DefinitionSet) builds from every entry that declares one.
/// Declaring a rule alongside the spec is therefore still required, and still works.
///
/// ```
/// use std::sync::Arc;
///
/// use techxt::def::{CallableSpecSource, SpecBuildCx};
/// use techy::core::specs::CallableSpec;
/// use techy::latexlike::MacroSpec;
/// use techy_xp::lang::LatexlikeXp;
///
/// /// Registers a macro that takes no arguments at all, whatever else it declares.
/// #[derive(Debug)]
/// struct Bare;
///
/// impl CallableSpecSource for Bare {
///     fn callable_spec(&self, _cx: &SpecBuildCx) -> Option<Arc<dyn CallableSpec<LatexlikeXp>>> {
///         Some(Arc::new(MacroSpec::new(Vec::new())))
///     }
/// }
/// ```
pub trait CallableSpecSource: Send + Sync + core::fmt::Debug {
    /// The spec to register, or `None` to register the one techxt would have built.
    fn callable_spec(&self, cx: &SpecBuildCx) -> Option<Arc<dyn CallableSpec<LatexlikeXp>>>;
}

/// What a [`CallableSpecSource`] may read about the converter being built
/// (DECISIONS.md D15).
///
/// Definitions are compiled once, when the converter is built, so this is where a spec
/// that depends on the converter's own configuration — rather than only on what the
/// definition declares — reads that configuration.
#[non_exhaustive]
#[derive(Clone)]
pub struct SpecBuildCx {
    source_resolver: Option<Arc<dyn SourceResolver<Option<String>>>>,
    macro_definitions: MacroDefinitions,
    local_scope: Box<str>,
    global_scope: Box<str>,
}

impl SpecBuildCx {
    /// A context describing a converter with nothing configured.
    pub fn new() -> SpecBuildCx {
        SpecBuildCx::default()
    }

    /// The resolver [`ConverterBuilder::source_resolver`](crate::ConverterBuilder::source_resolver)
    /// was given, if any.
    ///
    /// Its *presence* is what an inclusion spec keys on: techy runs the resolver from
    /// the parsing driver, so a spec needs the handle itself only if it resolves
    /// something of its own.
    pub fn source_resolver(&self) -> Option<&Arc<dyn SourceResolver<Option<String>>>> {
        self.source_resolver.as_ref()
    }

    /// Whether this converter honours the macro-defining commands
    /// ([`ConverterBuilder::macro_definitions`](crate::ConverterBuilder::macro_definitions)).
    ///
    /// It is what [`defs::preamble`](crate::defs::preamble)'s definer entries key on:
    /// under [`Honored`](MacroDefinitions::Honored) each hands over the techy-xp definer
    /// spec that actually makes the definition, and under
    /// [`Declared`](MacroDefinitions::Declared) each declines, so the entry registers the
    /// argument-consuming declaration it was written with instead.
    pub fn macro_definitions(&self) -> MacroDefinitions {
        self.macro_definitions
    }

    /// The name of the **local** definition scope this converter seeded — what a definer
    /// spec must be built to write into.
    ///
    /// It is the driver's own
    /// ([`XpDriver::local_scope_name`](techy_xp::lang::XpDriver::local_scope_name)),
    /// read off the very driver the converter will parse with, so a spec built from it
    /// cannot target a scope the seed does not hold. Writing into a scope the seed lacks
    /// is not an error anywhere: techy creates it *innermost*, which silently inverts
    /// techy-xp's ordering invariant.
    pub fn local_scope_name(&self) -> &str {
        &self.local_scope
    }

    /// The name of the **global** definition scope — `\gdef`'s target. See
    /// [`local_scope_name`](Self::local_scope_name).
    pub fn global_scope_name(&self) -> &str {
        &self.global_scope
    }

    /// The context for a converter built this way.
    pub(crate) fn for_converter(
        source_resolver: Option<Arc<dyn SourceResolver<Option<String>>>>,
        macro_definitions: MacroDefinitions,
        local_scope: &str,
        global_scope: &str,
    ) -> SpecBuildCx {
        SpecBuildCx {
            source_resolver,
            macro_definitions,
            local_scope: local_scope.into(),
            global_scope: global_scope.into(),
        }
    }
}

/// The two scope names default to techy-xp's own, which is what an
/// [`XpDriver`](techy_xp::lang::XpDriver) nobody reconfigured answers — so a context
/// built by hand agrees with the shipped converter.
impl Default for SpecBuildCx {
    fn default() -> SpecBuildCx {
        SpecBuildCx {
            source_resolver: None,
            macro_definitions: MacroDefinitions::default(),
            local_scope: DEFAULT_LOCAL_SCOPE_NAME.into(),
            global_scope: DEFAULT_GLOBAL_SCOPE_NAME.into(),
        }
    }
}

// `SourceResolver` is not `Debug` (it is `Send + Sync + Any`), so the handle is
// reported by presence, exactly as `ConverterBuilder` reports it.
impl core::fmt::Debug for SpecBuildCx {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SpecBuildCx")
            .field("has_source_resolver", &self.source_resolver.is_some())
            .field("macro_definitions", &self.macro_definitions)
            .field("local_scope", &self.local_scope)
            .field("global_scope", &self.global_scope)
            .finish_non_exhaustive()
    }
}

/// A specials definition as techy sees it: its arguments, plus the techxt rule that
/// renders it.
///
/// Specials are keyed by the characters that trigger them rather than by a name, but
/// they are otherwise ordinary callables and take arguments the same way (`^` and `_`
/// each take one expression argument).
#[derive(Debug)]
pub struct TechxtSpecialsSpec {
    arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
    rule: Option<TextRule>,
}

impl TechxtSpecialsSpec {
    /// A specials spec with these argument specs and this text rule.
    ///
    /// As for [`TechxtMacroSpec::new`], the rule is optional.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtSpecialsSpec {
        TechxtSpecialsSpec {
            arguments,
            rule: rule.into(),
        }
    }

    /// The text rule this specials renders through, if it has one.
    pub fn rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }
}

impl SerializableObject<LatexlikeXp> for TechxtSpecialsSpec {}

impl CallableSpec<LatexlikeXp> for TechxtSpecialsSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LatexlikeXp>>] {
        &self.arguments
    }
}

/// What an environment's body is, syntactically.
///
/// Recording this in the definition is what keeps the renderer from having to guess:
/// PLAN.md §9.1's rule "characters in a verbatim body are emitted verbatim" is answered
/// by asking the environment, not by inspecting the text.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvBodyKind {
    /// An ordinary body, parsed in the enclosing mode.
    Normal,
    /// A body parsed in math mode (`equation`, `align`).
    Math,
    /// A raw body: one characters node, no markup (`verbatim`).
    Verbatim,
    /// A list body (`itemize`, `enumerate`, `description`): parsed in the enclosing
    /// mode, but with the provider defining `\item` pushed for its extent (PLAN.md
    /// §9.4). Attach that provider with
    /// [`with_body_provider`](TechxtEnvironmentBehavior::with_body_provider).
    List(ListKind),
}

/// An environment definition as techy sees it: its arguments, what its body is, and the
/// techxt rule that renders it.
///
/// Register it with [`into_spec`](Self::into_spec).
///
/// # Why an environment's payload lives in a *behaviour*
///
/// An environment must be registered as a concrete [`EnvironmentSpec`] or techy's
/// `\begin` composition cannot find its body behaviour at all: the composition
/// downcasts the spec to `EnvironmentSpec` and asks it for the behaviour. A fresh
/// [`CallableSpec`] type — the shape [`TechxtMacroSpec`] takes — would silently lose the
/// body's state delta (no math mode for `equation`) and its verbatim body. So the techxt
/// payload for an environment rides one downcast further in, in the behaviour, and
/// [`EnvironmentSpec::with_body_delta`] must never be used on it: that wrapper hides the
/// custom behaviour from the render-side downcast, silently costing the environment its
/// rule. The body's state delta is implemented in
/// [`body_state_delta`](EnvironmentBehavior::body_state_delta) here instead.
#[derive(Debug)]
pub struct TechxtEnvironmentBehavior {
    arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
    body: EnvBodyKind,
    body_behavior: BodyBehavior,
    body_provider: Option<Arc<dyn SpecsProvider<LatexlikeXp>>>,
    rule: Option<TextRule>,
}

/// Which body parser an environment uses.
///
/// [`EnvironmentBehavior::make_body_parser`]'s default builds techy's standard body
/// parser, and that default is only reachable *through the trait* — hence a unit
/// implementor to call it on, rather than a `None` branch that has nothing to call.
#[derive(Debug)]
enum BodyBehavior {
    Standard(StandardBody),
    Verbatim(VerbatimBehavior<LatexlikeXp>),
}

/// A behaviour that overrides nothing, kept solely to reach the default body parser.
#[derive(Debug)]
struct StandardBody;

impl EnvironmentBehavior<LatexlikeXp> for StandardBody {}

impl TechxtEnvironmentBehavior {
    /// An environment behaviour with these argument specs, body kind and text rule.
    ///
    /// As for [`TechxtMacroSpec::new`], the rule is optional: an environment may
    /// declare only how it parses.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<LatexlikeXp>>>,
        body: EnvBodyKind,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtEnvironmentBehavior {
        let body_behavior = match body {
            // The verbatim body parser needs the argument specs too: it is the
            // behaviour's own `arguments()` that the composition parses, and the
            // verbatim behaviour answers with the ones it was built with.
            EnvBodyKind::Verbatim => {
                BodyBehavior::Verbatim(VerbatimBehavior::new(arguments.clone()))
            }
            _ => BodyBehavior::Standard(StandardBody),
        };
        TechxtEnvironmentBehavior {
            arguments,
            body,
            body_behavior,
            body_provider: None,
            rule: rule.into(),
        }
    }

    /// Push `provider` onto the scope stack for the extent of this environment's body.
    ///
    /// This is how a list environment brings `\item` into scope (PLAN.md §9.4): one
    /// shared package, pushed by every list environment's body delta, so that `\item`
    /// is defined exactly where it means something. Scope reversion is structural —
    /// nothing has to pop it when the body ends.
    pub fn with_body_provider(
        mut self,
        provider: Arc<dyn SpecsProvider<LatexlikeXp>>,
    ) -> TechxtEnvironmentBehavior {
        self.body_provider = Some(provider);
        self
    }

    /// The text rule this environment renders through, if it has one.
    pub fn rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }

    /// What this environment's body is, syntactically.
    pub fn body_kind(&self) -> EnvBodyKind {
        self.body
    }

    /// Wrap this behaviour in the [`EnvironmentSpec`] that techy registers.
    ///
    /// Do **not** call [`EnvironmentSpec::with_body_delta`] on the result: it wraps the
    /// behaviour in a private override type, and the render-side downcast that finds
    /// this object again then fails, silently costing the environment its rule. The
    /// body's state delta belongs in [`body_state_delta`](EnvironmentBehavior::body_state_delta)
    /// here instead.
    pub fn into_spec(self) -> EnvironmentSpec<LatexlikeXp> {
        EnvironmentSpec::from_behavior(Arc::new(self))
    }

    /// Recover this behaviour from a parsed environment node's spec, if it has one.
    ///
    /// This is step 2 of the dispatch chain for environments: `spec` → the concrete
    /// [`EnvironmentSpec`] → its behaviour → this type.
    ///
    /// Generic over the tree's language but downcasting to techxt's own concrete spec
    /// type, which is the whole design: on a foreign tree — a plain-techy
    /// [`Latexlike`](techy::latexlike::Latexlike) parse, say — the downcast simply
    /// misses, and the environment's rule is found one step later, in the name fallback
    /// table (PLAN.md §10.3 step 3).
    pub fn of<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> Option<&TechxtEnvironmentBehavior> {
        let spec = node.spec()?;
        let environment = (&**spec as &dyn Any).downcast_ref::<EnvironmentSpec<LatexlikeXp>>()?;
        (environment.behavior() as &dyn Any).downcast_ref::<TechxtEnvironmentBehavior>()
    }
}

impl EnvironmentBehavior<LatexlikeXp> for TechxtEnvironmentBehavior {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LatexlikeXp>>] {
        &self.arguments
    }

    fn body_state_delta(
        &self,
        _invocation: EnvironmentInvocation<'_, LatexlikeXp>,
    ) -> Result<Option<ParsingStateDelta<LatexlikeXp>>, ParseError<Option<String>>> {
        let mut delta = ParsingStateDelta::new();
        let mut anything = false;
        if self.body == EnvBodyKind::Math {
            // Entering math is a mode change; *leaving* it is an event, never
            // `.mode(Mode::Text)` — see `techxt::render`'s notes on `\text{…}`.
            delta = delta.mode(Mode::Math);
            anything = true;
        }
        if let Some(provider) = &self.body_provider {
            delta = delta.scope_op(ScopeOp::Push(Arc::clone(provider)));
            anything = true;
        }
        Ok(anything.then_some(delta))
    }

    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p, LatexlikeXp>,
    ) -> Box<dyn ConstructParser<LatexlikeXp, Output = EnvironmentBody<LatexlikeXp>> + 'p> {
        match &self.body_behavior {
            BodyBehavior::Standard(standard) => standard.make_body_parser(invocation),
            BodyBehavior::Verbatim(verbatim) => verbatim.make_body_parser(invocation),
        }
    }
}

/// Whether this node is one of techy-xp's **refusals** — `\expandafter`, `\ifx`,
/// `\catcode`, `\setcounter` and the rest of `techy-xp.refusals` (PLAN.md §16 M9).
///
/// The same downcast as [`embedded_rule`] above, asking the opposite question: not
/// *"does this spec carry a techxt rule?"* but *"is this spec techy-xp's answer for a
/// feature it declined?"*. A refusal reaches the renderer as a foreign spec with no rule,
/// so without this it would land in the unknown-construct policy and be reported a second
/// time, under a name that says less than the first report did — see
/// [`rules::unknown`](crate::render). What it *renders* as is unchanged: the policy still
/// decides that.
pub(crate) fn is_refusal<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> bool {
    node.spec()
        .is_some_and(|spec| (&**spec as &dyn Any).is::<RefusalSpec<LatexlikeXp>>())
}

/// The techxt rule embedded in a callable node's spec, if it has one (PLAN.md §10.3
/// step 2).
pub(crate) fn embedded_rule<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> Option<&TextRule> {
    let spec = node.spec()?;
    let object = &**spec as &dyn Any;
    if let Some(macro_spec) = object.downcast_ref::<TechxtMacroSpec>() {
        return macro_spec.rule.as_ref();
    }
    if let Some(specials_spec) = object.downcast_ref::<TechxtSpecialsSpec>() {
        return specials_spec.rule.as_ref();
    }
    TechxtEnvironmentBehavior::of(node).and_then(TechxtEnvironmentBehavior::rule)
}
