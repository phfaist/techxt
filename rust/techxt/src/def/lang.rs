//! [`ParseLang`]: what techxt's definitions need of the language they parse with.
//!
//! The render side asks only for [`RenderLang`]; the parse side asks for a little more,
//! and this module names it. It also holds the three pieces a language's hooks speak in:
//! the [`ScopeFrame`] it places around techxt's categories, the [`Definer`] it may claim,
//! and the [`SpecBuildCx`] a spec source reads while a converter is built.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use techy::core::specs::{CallableSpec, SpecsProvider};
use techy::core::NodeExtTypes;
use techy::error::Recovery;
use techy::latexlike::{BodyMarker, Latexlike, LatexlikeDriver, Mode};
use techy::source::SourceResolver;

use crate::render::RenderLang;

/// A latexlike language techxt's definitions can parse with.
///
/// A converter is built for one parse language, and everything the definitions register
/// — argument specs, callable specs, packages, the parsing state — is built *at* that
/// language. This trait is what the build asks of it: the facts the shipped definitions
/// are concrete about, and three hooks through which a language decides what only it
/// can decide.
///
/// # The facts
///
/// Beyond [`RenderLang`] (a tree of the language can be rendered at all):
///
/// - **techy's own [`Mode`]** as the mode vocabulary: an entry made visible in text mode
///   only names [`Mode::Text`], and techy's mode role trait has no text-mode constructor
///   to name it through;
/// - **techy's [`BodyMarker`]** as the slot ext: `\input`'s spec marks its attached
///   content as *not* the body, and only the concrete marker can mint that value;
/// - **a defaultable argument ext**, which is what techy's argument-code factory needs
///   to turn `"m"` or `"o"` into an argument spec.
///
/// # The hooks
///
/// [`make_driver`](Self::make_driver) is the one every language answers.
/// [`scope_frame`](Self::scope_frame) and [`definer_spec`](Self::definer_spec) default
/// to *nothing*: no providers around the categories, and every defining command
/// registered with the argument shape written beside it — consumed, and dropped.
///
/// The hooks exist because generic code can behave differently for one language only
/// through a trait method. A language that honours `\newcommand` answers both;
/// one that does not leaves them alone.
pub trait ParseLang:
    RenderLang<ModeId = Mode, NodeExts: NodeExtTypes<SlotExt = BodyMarker, ArgumentExt: Default>>
{
    /// Parse-time settings this language adds to the common ones a
    /// [`ConverterBuilder`](crate::ConverterBuilder) holds.
    ///
    /// `()` for a language with none. The builder stores one value, hands it to every
    /// hook below, and offers it to a [`CallableSpecSource`](super::CallableSpecSource)
    /// through [`SpecBuildCx::settings`].
    type Settings: Clone + Default + core::fmt::Debug + Send + Sync + 'static;

    /// The parse driver of one converter.
    ///
    /// `recovery` and `source_resolver` are what the builder was configured with; the
    /// driver is built once, *before* the definitions, so that a
    /// [`scope_frame`](Self::scope_frame) or a [`definer_spec`](Self::definer_spec) can
    /// read what it needs off the very driver the converter will parse with.
    fn make_driver(
        recovery: Recovery,
        source_resolver: Option<&Arc<dyn SourceResolver<Option<String>>>>,
        settings: &Self::Settings,
    ) -> Self::Driver;

    /// The providers this language places under and over techxt's categories.
    ///
    /// The default is none, which makes the provider stack exactly techxt's own: the
    /// catch-all (when enabled), techy's builtin package, and the categories in push
    /// order. See [`ScopeFrame`] for where each half goes.
    fn scope_frame(driver: &Self::Driver, settings: &Self::Settings) -> ScopeFrame<Self> {
        let _ = (driver, settings);
        ScopeFrame::none()
    }

    /// The spec a defining command parses through, if this language honours it.
    ///
    /// The default, `None`, declines: the entry then registers the argument shape
    /// written beside it in [`defs::preamble`](crate::defs::preamble), which consumes a
    /// definition so its body cannot leak into the text and defines nothing.
    fn definer_spec(
        definer: Definer,
        cx: &SpecBuildCx<'_, Self>,
    ) -> Option<Arc<dyn CallableSpec<Self>>> {
        let _ = (definer, cx);
        None
    }
}

/// techy's plain latexlike language: techxt's definitions, parsed without expansion.
///
/// This is the language for a converter that needs no techy-xp at all. It parses with
/// techy's own [`LatexlikeDriver`] and nothing around it, so what a converter built
/// for it does is fully described by the definitions it was given:
///
/// - **no settings** of its own ([`Settings`](ParseLang::Settings) is `()`);
/// - **no frame**: the provider stack is the catch-all (when enabled), techy's builtin
///   package, and the categories in push order — nothing below or above them;
/// - **no definers**: every defining command is *declared* — `\newcommand{\x}{…}`
///   consumes its arguments, so the definition's body never leaks into the text, and
///   defines nothing, so a later `\x` is an unknown command.
///
/// The output is the one techy-xp's [`LatexlikeXp`](crate::convert::LatexlikeXp) gives
/// with its definitions only declared
/// ([`MacroDefinitions::Declared`](crate::convert::MacroDefinitions::Declared)): the same
/// text, and the same diagnostics up to their source spans.
///
/// Build a converter for it through the generic door,
/// [`ConverterBuilder::with_definitions`](crate::ConverterBuilder::with_definitions).
impl ParseLang for Latexlike {
    type Settings = ();

    /// techy's latexlike driver under `recovery`, resolving `\input`-like references
    /// through `source_resolver` when one was given.
    fn make_driver(
        recovery: Recovery,
        source_resolver: Option<&Arc<dyn SourceResolver<Option<String>>>>,
        _settings: &(),
    ) -> LatexlikeDriver {
        let driver = LatexlikeDriver::new(recovery);
        match source_resolver {
            Some(resolver) => driver.with_source_resolver(Arc::clone(resolver)),
            None => driver,
        }
    }
}

/// The providers a [`ParseLang`] places around techxt's categories.
///
/// A converter's provider stack is, bottom to top: the unknown-command catch-all (when
/// enabled), techy's builtin package, [`below`](Self::below), one package per category
/// in push order, and [`above`](Self::above). Name resolution is innermost first, so
/// `below` is shadowed by every category and `above` shadows all of them.
///
/// Both halves are read off the converter's own driver
/// ([`ParseLang::scope_frame`]), which is what lets a language whose definitions write
/// into scopes *by name* seed exactly the scopes its driver and its definers agree on.
pub struct ScopeFrame<L: ParseLang> {
    /// Pushed below every category, so a name a category declares wins over it.
    pub below: Vec<Arc<dyn SpecsProvider<L>>>,
    /// Pushed above every category, in order, so the last one is innermost.
    pub above: Vec<Arc<dyn SpecsProvider<L>>>,
}

impl<L: ParseLang> ScopeFrame<L> {
    /// The empty frame: nothing below the categories, nothing above them.
    pub fn none() -> ScopeFrame<L> {
        ScopeFrame {
            below: Vec::new(),
            above: Vec::new(),
        }
    }
}

// Hand-written rather than derived: a derive would demand `L: Clone` / `L: Debug` / `L:
// Default` of a language that is only a marker type.
impl<L: ParseLang> Clone for ScopeFrame<L> {
    fn clone(&self) -> ScopeFrame<L> {
        ScopeFrame {
            below: self.below.clone(),
            above: self.above.clone(),
        }
    }
}

impl<L: ParseLang> Default for ScopeFrame<L> {
    fn default() -> ScopeFrame<L> {
        ScopeFrame::none()
    }
}

impl<L: ParseLang> core::fmt::Debug for ScopeFrame<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ScopeFrame")
            .field("below", &self.below)
            .field("above", &self.above)
            .finish()
    }
}

/// A defining command techxt declares, which a [`ParseLang`] may honour
/// ([`ParseLang::definer_spec`]).
///
/// One variant per definer rather than one per command, because `\edef` and `\xdef` are
/// `\def` and `\gdef` under a warning rather than definers of their own. The commands
/// themselves, and the argument shape each is declared with when no language claims it,
/// are [`defs::preamble`](crate::defs::preamble)'s.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Definer {
    /// `\newcommand{\x}[n][d]{body}`.
    NewCommand,
    /// `\renewcommand{\x}[n][d]{body}`.
    RenewCommand,
    /// `\providecommand{\x}[n][d]{body}`.
    ProvideCommand,
    /// `\newenvironment{x}[n][d]{begin}{end}`.
    NewEnvironment,
    /// `\renewenvironment{x}[n][d]{begin}{end}`.
    RenewEnvironment,
    /// `\def\x#1{body}` — TeX's, into the local scope.
    Def,
    /// `\gdef\x#1{body}` — the same, into the global scope, so it escapes its group.
    Gdef,
    /// `\let\a=\b` — copy `\b`'s meaning to `\a`.
    Let,
    /// `\edef` — `\def`, plus an approximation warning.
    Edef,
    /// `\xdef` — `\gdef`, plus the same warning.
    Xdef,
    /// `\NewDocumentCommand\x{m o}{body}` — xparse's, with `m`, `o`, `O{…}` and `s`.
    NewDocumentCommand,
    /// `\DeclareMathOperator{\x}{name}` — amsmath's, which is
    /// `\newcommand{\x}{\operatorname{name}}`.
    DeclareMathOperator,
}

/// What a [`CallableSpecSource`](super::CallableSpecSource) may read about the converter
/// being built.
///
/// Definitions are compiled once, when the converter is built, so this is where a spec
/// that depends on the converter's own configuration — rather than only on what the
/// definition declares — reads that configuration: whether a source resolver was given,
/// the driver the converter will parse with, and the language's own
/// [`Settings`](ParseLang::Settings).
///
/// It borrows all three, so it lives exactly as long as the build that made it.
#[non_exhaustive]
pub struct SpecBuildCx<'a, L: ParseLang = techy_xp::lang::LatexlikeXp> {
    source_resolver: Option<&'a Arc<dyn SourceResolver<Option<String>>>>,
    driver: &'a L::Driver,
    settings: &'a L::Settings,
}

impl<'a, L: ParseLang> SpecBuildCx<'a, L> {
    /// A context for a converter parsing with `driver` under `settings`, with no source
    /// resolver.
    pub fn new(driver: &'a L::Driver, settings: &'a L::Settings) -> SpecBuildCx<'a, L> {
        SpecBuildCx {
            source_resolver: None,
            driver,
            settings,
        }
    }

    /// The same context, with `resolver` as the converter's source resolver.
    pub(crate) fn with_source_resolver(
        mut self,
        resolver: Option<&'a Arc<dyn SourceResolver<Option<String>>>>,
    ) -> SpecBuildCx<'a, L> {
        self.source_resolver = resolver;
        self
    }

    /// The resolver [`ConverterBuilder::source_resolver`](crate::ConverterBuilder::source_resolver)
    /// was given, if any.
    ///
    /// Its *presence* is what an inclusion spec keys on: techy runs the resolver from
    /// the parsing driver, so a spec needs the handle itself only if it resolves
    /// something of its own.
    pub fn source_resolver(&self) -> Option<&'a Arc<dyn SourceResolver<Option<String>>>> {
        self.source_resolver
    }

    /// The driver the converter will parse with.
    ///
    /// It is built before the definitions precisely so that a spec can read what it
    /// must agree with off the very object that will run the parse.
    pub fn driver(&self) -> &'a L::Driver {
        self.driver
    }

    /// The language's own settings, as the builder was configured with them
    /// ([`ConverterBuilder::language_settings`](crate::ConverterBuilder::language_settings)).
    pub fn settings(&self) -> &'a L::Settings {
        self.settings
    }
}

// Hand-written: a derive would demand `L: Clone` of a marker type. Every field is a
// shared borrow, so a copy is the whole of it.
impl<L: ParseLang> Clone for SpecBuildCx<'_, L> {
    fn clone(&self) -> Self {
        SpecBuildCx {
            source_resolver: self.source_resolver,
            driver: self.driver,
            settings: self.settings,
        }
    }
}

// `SourceResolver` is not `Debug` (it is `Send + Sync + Any`), so the handle is
// reported by presence, exactly as `ConverterBuilder` reports it; the driver is not
// required to be `Debug` either.
impl<L: ParseLang> core::fmt::Debug for SpecBuildCx<'_, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SpecBuildCx")
            .field("has_source_resolver", &self.source_resolver.is_some())
            .field("settings", self.settings)
            .finish_non_exhaustive()
    }
}
