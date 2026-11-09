//! The techy spec types that carry a techxt [`TextRule`] into the parsed tree.
//!
//! This is what techxt's second design rule — *one definition, both sides* — comes
//! down to in code. A techxt definition is registered with techy as a spec object that
//! declares the construct's arguments (so the parser shapes the invocation correctly)
//! *and* carries the text rule (so the renderer knows what to do with it). Parser and
//! renderer read the same object; they cannot disagree about how many arguments a macro
//! takes, which is exactly the failure mode pylatexenc's two independent databases have.
//!
//! techy's [`CallableSpec`] has [`Any`](core::any::Any) as a supertrait, so the
//! renderer recovers the concrete type by downcasting the node's spec. That is the
//! sanctioned identity mechanism (techy's own `\begin` composition uses it to find an
//! environment's behaviour), and it is step 2 of techxt's dispatch chain.
//!
//! Environments are different, and it matters: see [`TechxtEnvironmentBehavior`].

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;

use techy::core::constructs::{ConstructParser, EnvironmentBody};
use techy::core::node::NodeRef;
use techy::core::specs::{ArgumentSpec, CallableSpec, ScopeOp, SpecsProvider};
use techy::core::ParsingStateDelta;
use techy::error::ParseError;
use techy::latexlike::{
    EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec, LatexlikeLang, LatexlikeMode,
    VerbatimBehavior,
};
use techy::serialize::SerializableObject;

use crate::render::ListKind;

use super::lang::{ParseLang, SpecBuildCx};
use super::TextRule;

/// A macro definition as techy sees it: its arguments, plus the techxt rule that
/// renders it.
///
/// Argument parsing is left entirely to techy's standard machinery — declaring the
/// arguments is the whole of it, and everything else keeps its default behaviour.
///
/// Generic over the language it is registered at, and bounded by [`LatexlikeLang`]
/// alone: the renderer names it at the language of whatever tree it folds.
pub struct TechxtMacroSpec<
    #[cfg(feature = "xp")] L = techy_xp::lang::LatexlikeXp,
    #[cfg(not(feature = "xp"))] L,
> where
    L: LatexlikeLang,
{
    arguments: Vec<Arc<ArgumentSpec<L>>>,
    rule: Option<TextRule>,
}

impl<L: LatexlikeLang> TechxtMacroSpec<L> {
    /// A macro spec with these argument specs and this text rule.
    ///
    /// The rule is optional — pass `None` for a definition that says how the macro
    /// *parses* but not how it renders. Such a macro falls through to the name fallback
    /// table and then to
    /// [`Options::unknown_macro`](crate::Options::unknown_macro), which is exactly what
    /// a parse-only entry should do.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<L>>>,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtMacroSpec<L> {
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

// Hand-written rather than derived: a derive would demand `L: Debug` of a language
// that is only a marker type, and `CallableSpec<L>` needs `Debug` for every `L`.
impl<L: LatexlikeLang> core::fmt::Debug for TechxtMacroSpec<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TechxtMacroSpec")
            .field("arguments", &self.arguments)
            .field("rule", &self.rule)
            .finish()
    }
}

// The `SerializableObject` supertrait of `CallableSpec` is fully defaulted; an empty
// impl opts out of serialization, which techxt does not offer in v1.
impl<L: LatexlikeLang> SerializableObject<L> for TechxtMacroSpec<L> {}

impl<L: LatexlikeLang> CallableSpec<L> for TechxtMacroSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

/// Supplies a techy [`CallableSpec`] that techxt's own declarations cannot express.
///
/// [`MacroDef::arg_spec`](super::MacroDef::arg_spec) is the same escape hatch one level
/// down: it takes over one *argument* when a code cannot describe it. This one takes
/// over the whole spec, for a construct techy itself implements and techxt only wants
/// to register — the shipped case is `\input`, which techy resolves at parse time only
/// when the macro is registered with its bespoke
/// [`InputMacroSpec`](techy::latexlike::InputMacroSpec).
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
/// renderer recovers it by downcasting (dispatch step 2). A foreign spec
/// cannot: it is not a [`TechxtMacroSpec`], so the downcast misses and the rule is found
/// at step 3 instead, in the name-keyed fallback table that
/// [`DefinitionSet`](super::DefinitionSet) builds from every entry that declares one.
/// Declaring a rule alongside the spec is therefore still required, and still works.
///
/// A spec from another *language*'s definitions behaves identically, and by the same
/// mechanism: the renderer folds the tree of any [`RenderLang`](crate::render::RenderLang)
/// and downcasts at that tree's own language, so a techxt spec is found whichever
/// [`ParseLang`] it was registered at, and a spec that is not techxt's simply misses.
/// Missing is the designed outcome, not a degradation: step 3 is what such a tree is
/// rendered by.
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
/// impl CallableSpecSource<LatexlikeXp> for Bare {
///     fn callable_spec(
///         &self,
///         _cx: &SpecBuildCx<'_, LatexlikeXp>,
///     ) -> Option<Arc<dyn CallableSpec<LatexlikeXp>>> {
///         Some(Arc::new(MacroSpec::new(Vec::new())))
///     }
/// }
/// ```
pub trait CallableSpecSource<
    #[cfg(feature = "xp")] L = techy_xp::lang::LatexlikeXp,
    #[cfg(not(feature = "xp"))] L,
>: Send + Sync + core::fmt::Debug
where
    L: ParseLang,
{
    /// The spec to register, or `None` to register the one techxt would have built.
    fn callable_spec(&self, cx: &SpecBuildCx<'_, L>) -> Option<Arc<dyn CallableSpec<L>>>;
}

/// A specials definition as techy sees it: its arguments, plus the techxt rule that
/// renders it.
///
/// Specials are keyed by the characters that trigger them rather than by a name, but
/// they are otherwise ordinary callables and take arguments the same way (`^` and `_`
/// each take one expression argument).
///
/// Bounded by [`LatexlikeLang`] alone, as [`TechxtMacroSpec`] is and for the same reason.
pub struct TechxtSpecialsSpec<
    #[cfg(feature = "xp")] L = techy_xp::lang::LatexlikeXp,
    #[cfg(not(feature = "xp"))] L,
> where
    L: LatexlikeLang,
{
    arguments: Vec<Arc<ArgumentSpec<L>>>,
    rule: Option<TextRule>,
}

impl<L: LatexlikeLang> TechxtSpecialsSpec<L> {
    /// A specials spec with these argument specs and this text rule.
    ///
    /// As for [`TechxtMacroSpec::new`], the rule is optional.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<L>>>,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtSpecialsSpec<L> {
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

impl<L: LatexlikeLang> core::fmt::Debug for TechxtSpecialsSpec<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TechxtSpecialsSpec")
            .field("arguments", &self.arguments)
            .field("rule", &self.rule)
            .finish()
    }
}

impl<L: LatexlikeLang> SerializableObject<L> for TechxtSpecialsSpec<L> {}

impl<L: LatexlikeLang> CallableSpec<L> for TechxtSpecialsSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

/// What an environment's body is, syntactically.
///
/// Recording this in the definition is what keeps the renderer from having to guess:
/// the rule "characters in a verbatim body are emitted verbatim" is answered by asking
/// the environment, not by inspecting the text.
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
    /// mode, but with the provider defining `\item` pushed for its extent. Attach that
    /// provider with
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
///
/// Bounded by [`LatexlikeLang`] alone, as [`TechxtMacroSpec`] is and for the same reason.
pub struct TechxtEnvironmentBehavior<
    #[cfg(feature = "xp")] L = techy_xp::lang::LatexlikeXp,
    #[cfg(not(feature = "xp"))] L,
> where
    L: LatexlikeLang,
{
    arguments: Vec<Arc<ArgumentSpec<L>>>,
    body: EnvBodyKind,
    body_behavior: BodyBehavior<L>,
    body_provider: Option<Arc<dyn SpecsProvider<L>>>,
    rule: Option<TextRule>,
}

impl<L: LatexlikeLang> core::fmt::Debug for TechxtEnvironmentBehavior<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TechxtEnvironmentBehavior")
            .field("arguments", &self.arguments)
            .field("body", &self.body)
            .field("body_behavior", &self.body_behavior)
            .field("body_provider", &self.body_provider)
            .field("rule", &self.rule)
            .finish()
    }
}

/// Which body parser an environment uses.
///
/// [`EnvironmentBehavior::make_body_parser`]'s default builds techy's standard body
/// parser, and that default is only reachable *through the trait* — hence a unit
/// implementor to call it on, rather than a `None` branch that has nothing to call.
enum BodyBehavior<L: LatexlikeLang> {
    Standard(StandardBody),
    Verbatim(VerbatimBehavior<L>),
}

impl<L: LatexlikeLang> core::fmt::Debug for BodyBehavior<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BodyBehavior::Standard(standard) => f.debug_tuple("Standard").field(standard).finish(),
            BodyBehavior::Verbatim(verbatim) => f.debug_tuple("Verbatim").field(verbatim).finish(),
        }
    }
}

/// A behaviour that overrides nothing, kept solely to reach the default body parser.
#[derive(Debug)]
struct StandardBody;

impl<L: LatexlikeLang> EnvironmentBehavior<L> for StandardBody {}

impl<L: LatexlikeLang> TechxtEnvironmentBehavior<L> {
    /// An environment behaviour with these argument specs, body kind and text rule.
    ///
    /// As for [`TechxtMacroSpec::new`], the rule is optional: an environment may
    /// declare only how it parses.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<L>>>,
        body: EnvBodyKind,
        rule: impl Into<Option<TextRule>>,
    ) -> TechxtEnvironmentBehavior<L> {
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
    /// This is how a list environment brings `\item` into scope: one shared package,
    /// pushed by every list environment's body delta, so that `\item` is defined exactly
    /// where it means something. Scope reversion is structural — nothing has to pop it
    /// when the body ends.
    pub fn with_body_provider(
        mut self,
        provider: Arc<dyn SpecsProvider<L>>,
    ) -> TechxtEnvironmentBehavior<L> {
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
    pub fn into_spec(self) -> EnvironmentSpec<L> {
        EnvironmentSpec::from_behavior(Arc::new(self))
    }

    /// Recover this behaviour from a parsed environment node's spec, if it has one.
    ///
    /// This is step 2 of the dispatch chain for environments: `spec` → the concrete
    /// [`EnvironmentSpec`] → its behaviour → this type.
    ///
    /// The downcast is at the **tree's** language, so a behaviour registered by any
    /// techxt converter is found again whatever language that converter parsed with;
    /// on a node whose spec is not techxt's — a foreign definition, a plain-techy
    /// default — it simply misses, and the environment's rule is found one step later,
    /// in the name fallback table. The bound is [`LatexlikeLang`] rather than
    /// [`RenderLang`](crate::render::RenderLang): asking a node for its spec needs
    /// nothing of the tree's slots or of how its sources are tagged.
    pub fn of(node: NodeRef<'_, L>) -> Option<&TechxtEnvironmentBehavior<L>> {
        let spec = node.spec()?;
        let environment = (&**spec as &dyn Any).downcast_ref::<EnvironmentSpec<L>>()?;
        (environment.behavior() as &dyn Any).downcast_ref::<TechxtEnvironmentBehavior<L>>()
    }
}

impl<L: LatexlikeLang> EnvironmentBehavior<L> for TechxtEnvironmentBehavior<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }

    fn body_state_delta(
        &self,
        _invocation: EnvironmentInvocation<'_, L>,
    ) -> Result<Option<ParsingStateDelta<L>>, ParseError<L::SourceOrigin>> {
        let mut delta = ParsingStateDelta::new();
        let mut anything = false;
        if self.body == EnvBodyKind::Math {
            // Entering math is a mode change; *leaving* it is an event, never
            // `.mode(Mode::Text)` — see `techxt::render`'s notes on `\text{…}`.
            delta = delta.mode(L::ModeId::math_mode());
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
        invocation: EnvironmentInvocation<'p, L>,
    ) -> Box<dyn ConstructParser<L, Output = EnvironmentBody<L>> + 'p> {
        match &self.body_behavior {
            BodyBehavior::Standard(standard) => {
                EnvironmentBehavior::<L>::make_body_parser(standard, invocation)
            }
            BodyBehavior::Verbatim(verbatim) => verbatim.make_body_parser(invocation),
        }
    }
}

/// Whether this node is one of techy-xp's **refusals** — `\expandafter`, `\ifx`,
/// `\catcode`, `\setcounter` and the rest of `techy-xp.refusals`.
///
/// The same downcast as [`embedded_rule`] above, asking the opposite question: not
/// *"does this spec carry a techxt rule?"* but *"is this spec techy-xp's answer for a
/// feature it declined?"*. A refusal reaches the renderer as a foreign spec with no rule,
/// so without this it would land in the unknown-construct policy and be reported a second
/// time, under a name that says less than the first report did — see
/// [`rules::unknown`](crate::render). What it *renders* as is unchanged: the policy still
/// decides that.
///
/// The downcast is at the tree's own language, as every render-side downcast is.
#[cfg(feature = "xp")]
pub(crate) fn is_refusal<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> bool {
    node.spec()
        .is_some_and(|spec| (&**spec as &dyn Any).is::<techy_xp::presets::RefusalSpec<LLL>>())
}

/// Never: without the `xp` feature techy-xp's refusals do not exist, so no node is one
/// (see the `xp` version of this function).
#[cfg(not(feature = "xp"))]
pub(crate) fn is_refusal<LLL: LatexlikeLang>(_node: NodeRef<'_, LLL>) -> bool {
    false
}

/// The techxt rule embedded in a callable node's spec, if it has one (dispatch step 2).
///
/// Every downcast is at the **tree's** language `LLL`: a techxt spec registered by a
/// converter built for that language is found, whichever converter now renders it.
pub(crate) fn embedded_rule<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> Option<&TextRule> {
    let spec = node.spec()?;
    let object = &**spec as &dyn Any;
    if let Some(macro_spec) = object.downcast_ref::<TechxtMacroSpec<LLL>>() {
        return macro_spec.rule.as_ref();
    }
    if let Some(specials_spec) = object.downcast_ref::<TechxtSpecialsSpec<LLL>>() {
        return specials_spec.rule.as_ref();
    }
    TechxtEnvironmentBehavior::of(node).and_then(TechxtEnvironmentBehavior::rule)
}
