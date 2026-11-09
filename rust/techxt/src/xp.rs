//! Everything techxt does with techy-xp: its language as a [`ParseLang`], the settings
//! that language adds, and the definers it honours.
//!
//! The rest of the crate is generic over the parse language and names techy-xp only in
//! default type parameters and in the two render-side checks that recognise its
//! refusals. What is *specific* to techy-xp's expanding reader lives here, in one place:
//! the driver with its two expansion budgets, the providers seeded around techxt's
//! categories, and the mapping from each [`Definer`] to the techy-xp spec that makes the
//! definition.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;

use techy::core::specs::CallableSpec;
use techy::error::Recovery;
use techy::latexlike::LatexlikeDriver;
use techy::source::SourceResolver;
use techy_xp::define::{
    def_definer, gdef_definer, let_definer, new_command_definer, new_document_command_definer,
    provide_command_definer, renew_command_definer, Defined, DefinerSpec, DefinitionRefused,
    DefinitionScope, NameForm, ParameterSyntax, ParsedDefinition, RedefinitionRule, StarForm,
};
use techy_xp::envs::{new_environment_definer, renew_environment_definer};
use techy_xp::expand::{Delivery, ExpandableSpec, MacroBody};
use techy_xp::lang::{LatexlikeXp, XpDriver};
use techy_xp::presets::{refusals_package, ExpandedDefinerSpec};

use crate::convert::{ConverterBuilder, MacroDefinitions};
use crate::def::{Definer, ParseLang, ScopeFrame, SpecBuildCx};

/// The parse-time settings techy-xp's [`LatexlikeXp`] adds to the common ones: whether
/// the defining commands are honoured, and the two expansion budgets.
///
/// It is the [`Settings`](ParseLang::Settings) of [`LatexlikeXp`], so a
/// [`ConverterBuilder`] for that language holds one. The builder's own
/// [`macro_definitions`](ConverterBuilder::macro_definitions),
/// [`expansion_depth_limit`](ConverterBuilder::expansion_depth_limit) and
/// [`expansion_count_limit`](ConverterBuilder::expansion_count_limit) set its fields one
/// at a time, and [`language_settings`](ConverterBuilder::language_settings) replaces it
/// whole. The defaults are techxt's, not techy-xp's: definitions honoured, 64 nested
/// expansions, 2 000 expansions per parse — see those three methods for why.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XpSettings {
    /// Whether the macro-defining commands are honoured or only declared. See
    /// [`ConverterBuilder::macro_definitions`].
    pub macro_definitions: MacroDefinitions,
    /// How deeply one expansion may nest inside another. See
    /// [`ConverterBuilder::expansion_depth_limit`].
    pub expansion_depth_limit: usize,
    /// How many expansions one parse may start. See
    /// [`ConverterBuilder::expansion_count_limit`].
    pub expansion_count_limit: usize,
}

impl Default for XpSettings {
    fn default() -> XpSettings {
        XpSettings {
            macro_definitions: MacroDefinitions::default(),
            expansion_depth_limit: ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT,
            expansion_count_limit: ConverterBuilder::DEFAULT_EXPANSION_COUNT_LIMIT,
        }
    }
}

impl XpSettings {
    /// Whether these settings honour the defining commands.
    fn honored(&self) -> bool {
        !matches!(self.macro_definitions, MacroDefinitions::Declared)
    }
}

impl ParseLang for LatexlikeXp {
    type Settings = XpSettings;

    /// techy's latexlike driver wrapped in [`XpDriver`], with techxt's two budgets.
    ///
    /// The latexlike driver carries the parse-time behavior; wrapping it in `XpDriver`
    /// is what installs the expanding token reader macro definitions are honored
    /// through, and it does so whichever way `macro_definitions` went: with no definer
    /// seeded the reader has nothing to expand and parses byte for byte as the bare
    /// `LatexlikeDriver` did — techy-xp's lockstep property. The two expansion budgets
    /// are techxt's own, lower than techy-xp's defaults for the reason
    /// [`ConverterBuilder::expansion_count_limit`] gives. The resolver is handed to the
    /// wrapper, not the inner driver: the wrapper decides where it is needed.
    fn make_driver(
        recovery: Recovery,
        source_resolver: Option<&Arc<dyn SourceResolver<Option<String>>>>,
        settings: &XpSettings,
    ) -> XpDriver<LatexlikeXp> {
        let mut driver = XpDriver::wrapping(LatexlikeDriver::new(recovery))
            .with_expansion_depth_limit(settings.expansion_depth_limit)
            .with_expansion_count_limit(settings.expansion_count_limit);
        if let Some(resolver) = source_resolver {
            driver = driver.with_source_resolver(Arc::clone(resolver));
        }
        driver
    }

    /// When the definers are honoured: techy-xp's refusals below the categories, and the
    /// driver's two definition scopes above them. When they are only declared: nothing,
    /// which is the stack techxt built before the definers were wired.
    ///
    /// Both halves come off the very [`XpDriver`] the converter will parse with, which is
    /// the point of passing a driver rather than two strings: the definer specs write
    /// into scopes **by name**, the driver's leak hook keys `\gdef`'s escape on the
    /// global one's name, and a seed that names them differently is not an error
    /// anywhere — techy creates a missing scope *innermost*, which inverts the ordering
    /// silently. Reading both off one object is what makes them agree by construction.
    fn scope_frame(driver: &XpDriver<LatexlikeXp>, settings: &XpSettings) -> ScopeFrame<Self> {
        if !settings.honored() {
            return ScopeFrame::none();
        }
        ScopeFrame {
            below: alloc::vec![refusals_package::<LatexlikeXp>()],
            above: driver.definition_scope_providers().to_vec(),
        }
    }

    /// techy-xp's definer spec when the definers are honoured, built to write into the
    /// driver's own two scopes; `None` when they are only declared, so the entry
    /// registers the argument-consuming declaration written beside it.
    fn definer_spec(
        definer: Definer,
        cx: &SpecBuildCx<'_, Self>,
    ) -> Option<Arc<dyn CallableSpec<Self>>> {
        if !cx.settings().honored() {
            return None;
        }
        Some(definer_spec(
            definer,
            cx.driver().local_scope_name(),
            cx.driver().global_scope_name(),
        ))
    }
}

/// The compatibility accessors of a context built for techy-xp's language: what a
/// [`CallableSpecSource`](crate::def::CallableSpecSource) written before the parse
/// language was a parameter reads.
impl SpecBuildCx<'_, LatexlikeXp> {
    /// Whether this converter honours the macro-defining commands
    /// ([`ConverterBuilder::macro_definitions`]).
    ///
    /// It is what [`defs::preamble`](crate::defs::preamble)'s definer entries key on:
    /// under [`Honored`](MacroDefinitions::Honored) each hands over the techy-xp definer
    /// spec that actually makes the definition, and under
    /// [`Declared`](MacroDefinitions::Declared) each declines, so the entry registers the
    /// argument-consuming declaration it was written with instead.
    pub fn macro_definitions(&self) -> MacroDefinitions {
        self.settings().macro_definitions
    }

    /// The name of the **local** definition scope this converter seeded — what a definer
    /// spec must be built to write into.
    ///
    /// It is the driver's own ([`XpDriver::local_scope_name`]), read off the very driver
    /// the converter will parse with, so a spec built from it cannot target a scope the
    /// seed does not hold. Writing into a scope the seed lacks is not an error anywhere:
    /// techy creates it *innermost*, which silently inverts techy-xp's ordering
    /// invariant.
    pub fn local_scope_name(&self) -> &str {
        self.driver().local_scope_name()
    }

    /// The name of the **global** definition scope — `\gdef`'s target. See
    /// [`local_scope_name`](Self::local_scope_name).
    pub fn global_scope_name(&self) -> &str {
        self.driver().global_scope_name()
    }
}

/// The techy-xp spec behind `definer`, built to write into `local` and `global`.
fn definer_spec(definer: Definer, local: &str, global: &str) -> Arc<dyn CallableSpec<LatexlikeXp>> {
    let scoped = |spec: DefinerSpec<LatexlikeXp>| {
        spec.with_definition_scopes(local, global)
            // Unconditionally, whatever the definer's own rule was: techxt's
            // stack cannot answer "is this name new?" — `defs::preamble`'s module
            // documentation, *Every definer here redefines unconditionally*, says why.
            .with_redefinition(RedefinitionRule::Always)
    };
    match definer {
        Definer::NewCommand => Arc::new(scoped(new_command_definer())),
        Definer::RenewCommand => Arc::new(scoped(renew_command_definer())),
        Definer::ProvideCommand => Arc::new(scoped(provide_command_definer())),
        Definer::NewEnvironment => Arc::new(scoped(new_environment_definer())),
        Definer::RenewEnvironment => Arc::new(scoped(renew_environment_definer())),
        Definer::Def => Arc::new(scoped(def_definer())),
        Definer::Gdef => Arc::new(scoped(gdef_definer())),
        Definer::Let => Arc::new(scoped(let_definer())),
        Definer::Edef => Arc::new(ExpandedDefinerSpec::new(scoped(def_definer()), "def")),
        Definer::Xdef => Arc::new(ExpandedDefinerSpec::new(scoped(gdef_definer()), "gdef")),
        Definer::NewDocumentCommand => Arc::new(scoped(new_document_command_definer())),
        Definer::DeclareMathOperator => Arc::new(scoped(declare_math_operator_definer())),
    }
}

/// The definer behind `\DeclareMathOperator`: a token-time macro whose body is the
/// operator's name wrapped in `\operatorname{…}`.
///
/// techy-xp ships no definer for amsmath's declaration (its own docs list it among the
/// deferred ones), and there is nothing to defer: `\DeclareMathOperator{\Tr}{Tr}` is
/// `\newcommand{\Tr}{\operatorname{Tr}}`, and the starred form is `\operatorname*`.
/// So this is techy-xp's macro definer with one step in front — the body's text is
/// prefixed with `\operatorname`, spelled with the escape character the author used,
/// and braced. The macro is otherwise what `\newcommand` would have made: the same
/// token rules (those in force where the declaration was written), the same definition
/// site, no parameters. `\operatorname` is techxt's own
/// ([`defs::mathcore`](crate::defs::mathcore)), so wherever the operator is used its
/// name is set upright and classed as an operator, and a document's later
/// `\renewcommand{\Tr}{…}` shadows it like any other macro.
#[derive(Debug)]
struct MathOperatorDefiner;

/// `MathOperatorDefiner`'s wire identity, namespaced the way every techy identifier is.
const MATH_OPERATOR_DEFINER_IDENTIFIER: &str = "techxt.define.math-operator";

impl techy_xp::define::Definer<LatexlikeXp> for MathOperatorDefiner {
    fn identifier(&self) -> &str {
        MATH_OPERATOR_DEFINER_IDENTIFIER
    }

    fn define(
        &self,
        mut parsed: ParsedDefinition<LatexlikeXp>,
    ) -> Result<Defined<LatexlikeXp>, DefinitionRefused> {
        // Empty by construction — the spec declares no parameter syntax — and handed
        // over rather than invented, so that the spec stays the one source of truth.
        let shapes = parsed.take_shapes();
        let mut bodies = parsed.take_bodies();
        if bodies.is_empty() {
            return Err(DefinitionRefused::new(
                parsed.name(),
                "this definer read no operator name, and the operator is its name",
            ));
        }
        let body = bodies.remove(0);
        let star = if parsed.starred() { "*" } else { "" };
        let text = format!(
            "{escape}operatorname{star}{{{name}}}",
            escape = parsed.escape_char(),
            name = body.text(),
        );
        let macro_body = MacroBody::new(
            text,
            Arc::clone(body.definition_state()),
            body.parameters(),
        );
        let spec = ExpandableSpec::new(macro_body, shapes, Delivery::TokenTime)
            .map_err(|error| DefinitionRefused::new(parsed.name(), error.to_string()))?
            .with_definition_site(parsed.site().clone());
        Ok(Defined::one(
            parsed.scope(),
            parsed.callable_type(),
            parsed.name(),
            Arc::new(spec),
        ))
    }
}

/// `\DeclareMathOperator{\x}{name}` and `\DeclareMathOperator*{\x}{name}` — amsmath's.
///
/// The `\newcommand` shape without the parameter declaration: a `{\cs}`-or-`\cs` name
/// and one body. The star is read the way the `\newcommand` family reads it
/// ([`StarForm::AcceptedAndIgnored`] is techy-xp's name for *read and recorded*), and
/// this definer does not ignore it: it chooses `\operatorname*`.
fn declare_math_operator_definer() -> DefinerSpec<LatexlikeXp> {
    DefinerSpec::new(Arc::new(MathOperatorDefiner))
        .with_name_form(NameForm::BracedOrBare)
        .with_parameters(ParameterSyntax::None)
        .with_bodies(1)
        .with_redefinition(RedefinitionRule::Always)
        .with_scope(DefinitionScope::Local)
        .with_star(StarForm::AcceptedAndIgnored)
}
