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
use techy::core::specs::{ArgumentSpec, CallableSpec};
use techy::core::ParsingStateDelta;
use techy::error::ParseError;
use techy::latexlike::{
    EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec, Latexlike, Mode, VerbatimBehavior,
};
use techy::serialize::SerializableObject;

use super::TextRule;

/// A macro definition as techy sees it: its arguments, plus the techxt rule that
/// renders it.
///
/// Argument parsing is left entirely to techy's standard machinery — declaring the
/// arguments is the whole of it, and everything else keeps its default behaviour.
#[derive(Debug)]
pub struct TechxtMacroSpec {
    arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
    rule: TextRule,
}

impl TechxtMacroSpec {
    /// A macro spec with these argument specs and this text rule.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<Latexlike>>>, rule: TextRule) -> TechxtMacroSpec {
        TechxtMacroSpec { arguments, rule }
    }

    /// The text rule this macro renders through.
    pub fn rule(&self) -> &TextRule {
        &self.rule
    }
}

// The `SerializableObject` supertrait of `CallableSpec` is fully defaulted; an empty
// impl opts out of serialization, which techxt does not offer in v1 (PLAN.md §2).
impl SerializableObject<Latexlike> for TechxtMacroSpec {}

impl CallableSpec<Latexlike> for TechxtMacroSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
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
    arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
    rule: TextRule,
}

impl TechxtSpecialsSpec {
    /// A specials spec with these argument specs and this text rule.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<Latexlike>>>, rule: TextRule) -> TechxtSpecialsSpec {
        TechxtSpecialsSpec { arguments, rule }
    }

    /// The text rule this specials renders through.
    pub fn rule(&self) -> &TextRule {
        &self.rule
    }
}

impl SerializableObject<Latexlike> for TechxtSpecialsSpec {}

impl CallableSpec<Latexlike> for TechxtSpecialsSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }
}

/// What an environment's body is, syntactically.
///
/// Recording this in the definition is what keeps the renderer from having to guess:
/// PLAN.md §9.1's rule "characters in a verbatim body are emitted verbatim" is answered
/// by asking the environment, not by inspecting the text.
///
/// **M3/M6 seam:** list environments gain a `List(ListKind)` variant, whose body delta
/// pushes the package defining `\item` (PLAN.md §9.4). The enum is `#[non_exhaustive]`
/// so that is an additive change.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvBodyKind {
    /// An ordinary body, parsed in the enclosing mode.
    Normal,
    /// A body parsed in math mode (`equation`, `align`).
    Math,
    /// A raw body: one characters node, no markup (`verbatim`).
    Verbatim,
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
    arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
    body: EnvBodyKind,
    body_behavior: BodyBehavior,
    rule: TextRule,
}

/// Which body parser an environment uses.
///
/// [`EnvironmentBehavior::make_body_parser`]'s default builds techy's standard body
/// parser, and that default is only reachable *through the trait* — hence a unit
/// implementor to call it on, rather than a `None` branch that has nothing to call.
#[derive(Debug)]
enum BodyBehavior {
    Standard(StandardBody),
    Verbatim(VerbatimBehavior<Latexlike>),
}

/// A behaviour that overrides nothing, kept solely to reach the default body parser.
#[derive(Debug)]
struct StandardBody;

impl EnvironmentBehavior<Latexlike> for StandardBody {}

impl TechxtEnvironmentBehavior {
    /// An environment behaviour with these argument specs, body kind and text rule.
    pub fn new(
        arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
        body: EnvBodyKind,
        rule: TextRule,
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
            rule,
        }
    }

    /// The text rule this environment renders through.
    pub fn rule(&self) -> &TextRule {
        &self.rule
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
    pub fn into_spec(self) -> EnvironmentSpec<Latexlike> {
        EnvironmentSpec::from_behavior(Arc::new(self))
    }

    /// Recover this behaviour from a parsed environment node's spec, if it has one.
    ///
    /// This is step 2 of the dispatch chain for environments: `spec` → the concrete
    /// [`EnvironmentSpec`] → its behaviour → this type.
    pub fn of(node: NodeRef<'_, Latexlike>) -> Option<&TechxtEnvironmentBehavior> {
        let spec = node.spec()?;
        let environment = (&**spec as &dyn Any).downcast_ref::<EnvironmentSpec<Latexlike>>()?;
        (environment.behavior() as &dyn Any).downcast_ref::<TechxtEnvironmentBehavior>()
    }
}

impl EnvironmentBehavior<Latexlike> for TechxtEnvironmentBehavior {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }

    fn body_state_delta(
        &self,
        _invocation: EnvironmentInvocation<'_, Latexlike>,
    ) -> Result<Option<ParsingStateDelta<Latexlike>>, ParseError<Option<String>>> {
        match self.body {
            // Entering math is a mode change; *leaving* it is an event, never
            // `.mode(Mode::Text)` — see `techxt::render`'s notes on `\text{…}`.
            EnvBodyKind::Math => Ok(Some(ParsingStateDelta::new().mode(Mode::Math))),
            EnvBodyKind::Normal | EnvBodyKind::Verbatim => Ok(None),
        }
    }

    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p, Latexlike>,
    ) -> Box<dyn ConstructParser<Latexlike, Output = EnvironmentBody<Latexlike>> + 'p> {
        match &self.body_behavior {
            BodyBehavior::Standard(standard) => standard.make_body_parser(invocation),
            BodyBehavior::Verbatim(verbatim) => verbatim.make_body_parser(invocation),
        }
    }
}

/// The techxt rule embedded in a callable node's spec, if it has one (PLAN.md §10.3
/// step 2).
pub(crate) fn embedded_rule(node: NodeRef<'_, Latexlike>) -> Option<&TextRule> {
    let spec = node.spec()?;
    let object = &**spec as &dyn Any;
    if let Some(macro_spec) = object.downcast_ref::<TechxtMacroSpec>() {
        return Some(&macro_spec.rule);
    }
    if let Some(specials_spec) = object.downcast_ref::<TechxtSpecialsSpec>() {
        return Some(&specials_spec.rule);
    }
    TechxtEnvironmentBehavior::of(node).map(TechxtEnvironmentBehavior::rule)
}
