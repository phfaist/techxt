//! The public conversion API (PLAN.md §11): [`Converter`], its builder, and its
//! [`Options`].
//!
//! A [`Converter`] holds a parsing language and a set of definitions, and converts as
//! many documents as you like. It is immutable, `Clone + Send + Sync`, and cheap to
//! clone (its internals are shared); everything a single conversion accumulates lives in
//! a per-run renderer, so one converter can serve many threads at once.
//!
//! ```
//! use techxt::Converter;
//!
//! let converter = Converter::standard();
//! let conversion = converter.latex_to_text("Hello  {brave}\n world.")?;
//! assert_eq!(conversion.text, "Hello brave world.\n");
//! assert!(conversion.diagnostics.is_empty());
//! # Ok::<(), techy::error::ParseError<Option<String>>>(())
//! ```
//!
//! # Three layers
//!
//! Each layer is public, and each is one step less convenient and one step more
//! controllable than the last:
//!
//! - [`Converter::latex_to_text`] — text in, text out, diagnostics on the side.
//! - [`Converter::tree_to_text`] and [`Converter::tree_to_flow`] — convert a tree you
//!   parsed (or transformed) yourself. Both are infallible.
//! - [`Converter::renderer`] plus [`layout`](crate::layout) — drive the fold yourself,
//!   which is what a consumer wrapping techxt's recomposer does.
//!
//! There is deliberately **no fourth layer converting a subtree**: techy's recomposer
//! takes a whole [`NodeTree`] and its context has no public constructor, so a
//! `node_to_text(NodeSlice)` cannot be built on the public API (PLAN.md §11.1 and §17
//! anticipate this). Convert the tree, or drive the renderer yourself as above.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use techy::latexlike::LatexlikeDriver;

use crate::def::{
    BuiltDefinitions, CallableKind, DefinitionSet, RuleTable, SpecBuildCx, TemplateError,
    TemplateScope, TextRule,
};
use crate::flow::Flow;
use crate::layout::{render, LayoutOptions};
use crate::render::{RenderConfig, RenderState, TextRenderer};

pub use crate::mathfmt::{FontStyle, FontStyleKind, MathWrapDelims, MatrixDelims};

// --------------------------------------------------------------- techy, re-exported
//
// Every techy type that appears in techxt's own public API is re-exported here, so
// that using techxt takes one dependency and not two (PLAN.md §2). They are techy's
// types, not aliases: a `SourceResolver` implemented against `techxt::convert` is the
// very trait techy's parser calls, and a `Diagnostics` returned by a conversion is the
// one techy built. Naming `techy` directly keeps working and is exactly equivalent —
// this is a convenience and a semver statement, not an abstraction layer.

/// techy's descent guard, re-exported so that embedders configuring
/// [`ConverterBuilder::descent_guard`] need not name `techy::core` themselves.
///
/// Despite the name it is pure `alloc` — "Std" is "the standard guard", not the standard
/// library — and works on a `no_std` target like everything else here.
pub use techy::core::{StdDescentGuard, StdDescentGuardInit};

/// The parse tree and its nodes: what [`Converter::tree_to_text`] converts, and what a
/// [`TextHandler`](crate::def::TextHandler) is handed.
pub use techy::core::node::{NodeRef, NodeSlice, NodeTree};

/// The techy pieces a definition of your own is built from: an argument spec for
/// [`MacroDef::arg_spec`](crate::def::MacroDef::arg_spec), a callable spec for
/// [`MacroDef::spec`](crate::def::MacroDef::spec), and the language every techxt type
/// is concrete over (PLAN.md §11.1).
pub use techy::core::specs::{ArgumentSpec, CallableSpec};
/// See [`ArgumentSpec`].
pub use techy::latexlike::{EnvironmentSpec, Latexlike};

/// The diagnostics channel: the collection a [`Conversion`] carries, one entry of it,
/// the severity to filter by, and the parse error [`Converter::latex_to_text`] can fail
/// with.
///
/// [`Recovery`] is what [`ConverterBuilder::recovery`] takes.
pub use techy::error::{Diagnostic, Diagnostics, ParseError, Recovery, Severity};

/// Source resolution for `\input` (PLAN.md §9.8): the trait
/// [`ConverterBuilder::source_resolver`] accepts, what an implementation answers with,
/// and the cycle guard a resolver should run.
pub use techy::source::{
    check_include_chain, MapResolver, ResolveError, ResolvedContent, Source, SourceResolver,
    SourceSpan,
};

/// The fold itself: the recomposer trait [`TextRenderer`]
/// implements, and the driver that runs it over a tree — what a consumer wrapping
/// techxt's recomposer names (see [`Converter::renderer`]).
pub use techy::recompose::{RecomposeError, Recomposer, TreeRecomposer};

/// The parsing language a [`Converter`] holds, as returned by [`Converter::language`],
/// and the two techy errors [`BuildError`] carries: an argument code techy did not
/// understand, and a parsing state it could not finalize.
pub use techy::core::{FinalizeError, Language};
/// See [`Language`].
pub use techy::latexlike::ArgumentCodeError;
/// The conversion [`ConverterBuilder::source_resolver`] accepts its argument through:
/// a resolver value, or one already shared in an `Arc`.
pub use techy::source::IntoSourceResolver;

/// The result of a conversion (PLAN.md §11.1).
#[derive(Clone, Debug)]
pub struct Conversion {
    /// The converted text. Ends with exactly one newline unless it is empty.
    pub text: String,
    /// Everything the conversion reported, parse diagnostics first.
    pub diagnostics: Diagnostics<Option<String>>,
}

/// Converts LaTeX-like markup to plain text (PLAN.md §11.1).
///
/// Build one with [`standard`](Self::standard) for the shipped definitions and default
/// options, or with [`builder`](Self::builder) to choose both. A converter is immutable
/// and shared: cloning it is an atomic increment, and converting two documents at once
/// from two threads is exactly what it is designed for.
#[derive(Clone)]
pub struct Converter {
    inner: Arc<Inner>,
}

struct Inner {
    language: Language<Latexlike>,
    config: RenderConfig,
    /// The fold side of the descent guard (DECISIONS.md D9). The parse side lives on
    /// the `Language`; both come from the same `ConverterBuilder::descent_guard` call,
    /// because a document that is too deep to parse is too deep to render as well.
    descent_guard: StdDescentGuardInit,
}

impl Converter {
    /// Start building a converter.
    pub fn builder() -> ConverterBuilder {
        ConverterBuilder::new()
    }

    /// A converter with the shipped definitions ([`defs::standard`](crate::defs::standard))
    /// and [`Options::default`].
    ///
    /// ```
    /// use techxt::Converter;
    /// let converter = Converter::standard();
    /// assert_eq!(converter.latex_to_text("one\n\n\n\ntwo")?.text, "one\n\ntwo\n");
    /// # Ok::<(), techy::error::ParseError<Option<String>>>(())
    /// ```
    pub fn standard() -> Converter {
        // The only inputs are techxt's own tables, so a failure here is a bug in this
        // crate rather than anything a caller did — there is no error for a caller to
        // handle, which is why the plan makes this constructor infallible.
        Converter::builder()
            .build()
            .expect("techxt's own definitions are well-formed")
    }

    /// Parse and convert a document.
    ///
    /// The `Err` case is a parse failure that no recovery policy survives — a document
    /// nesting past the [descent guard](ConverterBuilder::descent_guard), or a
    /// definition whose parsing hook failed. Ordinary malformedness is *not* an error:
    /// under the default
    /// tolerant recovery an unbalanced brace or an unknown macro produces a diagnostic
    /// and a best-effort tree, and the conversion continues.
    pub fn latex_to_text(&self, latex: &str) -> Result<Conversion, ParseError<Option<String>>> {
        let parsed = self.inner.language.parse(latex)?;
        let (flow, render_diagnostics) = self.tree_to_flow(&parsed.tree);
        Ok(Conversion {
            text: self.lay_out(&flow),
            // Parse diagnostics come first: they describe what the tree even is.
            diagnostics: merge(&parsed.diagnostics, &render_diagnostics),
        })
    }

    /// Convert a tree that has already been parsed — or transformed.
    ///
    /// Infallible: everything that can go wrong during rendering is a diagnostic.
    pub fn tree_to_text(&self, tree: &NodeTree<Latexlike>) -> Conversion {
        let (flow, diagnostics) = self.tree_to_flow(tree);
        Conversion {
            text: self.lay_out(&flow),
            diagnostics,
        }
    }

    /// Convert a tree to a [`Flow`], stopping short of layout.
    ///
    /// Use this to lay the result out with options of your own, to inspect the token
    /// stream, or to splice the flow into a larger one.
    pub fn tree_to_flow(&self, tree: &NodeTree<Latexlike>) -> (Flow, Diagnostics<Option<String>>) {
        let mut renderer = self.renderer();
        // So that a diagnostic raised before any node is folded still has a position.
        renderer.note_document_span(tree.root());

        let result = TreeRecomposer::new(&mut renderer)
            .with_descent_guard_init(self.inner.descent_guard)
            .recompose(tree, RenderState::initial(&self.inner.config.options));

        let mut flow = match result {
            Ok(flow) => flow,
            // The fold's only abort is the descent guard's refusal (the recomposer
            // itself cannot fail). PLAN.md §10.4: empty output plus an error diagnostic.
            Err(error) => {
                renderer.abort(alloc::format!("{error}"));
                Flow::new()
            }
        };
        let finish = renderer.finish();
        flow.extend(finish.trailing);
        (flow, finish.diagnostics)
    }

    /// The parsing language this converter uses.
    ///
    /// It is `Send + Sync` and reusable; parse with it directly when you want the tree
    /// as well as the text.
    pub fn language(&self) -> &Language<Latexlike> {
        &self.inner.language
    }

    /// The options this converter converts with.
    pub fn options(&self) -> &Options {
        &self.inner.config.options
    }

    /// A fresh renderer for one document.
    ///
    /// This is the entry point for wrapping techxt: drive it with techy's
    /// `TreeRecomposer` (or from inside a recomposer of your own), then call
    /// [`TextRenderer::finish`]. See the [`render`](crate::render) module documentation
    /// for the whole recipe.
    pub fn renderer(&self) -> TextRenderer<'_> {
        TextRenderer::new(&self.inner.config)
    }

    /// Lay a flow out with this converter's options.
    fn lay_out(&self, flow: &Flow) -> String {
        render(
            flow,
            &LayoutOptions {
                wrap_width: self.inner.config.options.wrap_width,
            },
        )
    }
}

impl core::fmt::Debug for Converter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Converter")
            .field("options", &self.inner.config.options)
            .finish_non_exhaustive()
    }
}

/// Concatenate two diagnostic collections without either one's retention cap silently
/// eating the other's entries — and without losing what a cap already ate.
///
/// A [`Diagnostics`] collection retains at most
/// [`DEFAULT_LIMIT`](Diagnostics::DEFAULT_LIMIT) entries and *counts* the rest, so that
/// a report can end with "… and N more" instead of pretending the surplus never
/// happened. Rebuilding a collection by pushing therefore has to carry three numbers
/// across, not one: the retained entries, [`suppressed`](Diagnostics::suppressed), and
/// [`error_count`](Diagnostics::error_count) — the last of which decides
/// [`has_errors`](Diagnostics::has_errors), and so decides the CLI's exit code.
///
/// The suppressed surplus is replayed the only way techy's public API allows: by
/// pushing into a collection that is exactly full, where a push stores nothing and
/// moves the counters alone. The pushed values are unobservable — that is the whole
/// point of pushing them past the cap — so what matters about each is its severity,
/// which is what keeps the error count exact.
fn merge(
    first: &Diagnostics<Option<String>>,
    second: &Diagnostics<Option<String>>,
) -> Diagnostics<Option<String>> {
    let retained = first.len() + second.len();
    let suppressed = first.suppressed() + second.suppressed();
    // With nothing suppressed there is nothing to replay, and the cap keeps the
    // headroom a fresh collection has. With something suppressed the merged collection
    // is already a capped one, and it has to be *exactly* full for the replay below to
    // count rather than store.
    let limit = if suppressed == 0 {
        core::cmp::max(Diagnostics::<Option<String>>::DEFAULT_LIMIT, retained)
    } else {
        retained
    };
    let mut merged = Diagnostics::with_limit(limit);
    for diagnostic in first.iter().chain(second.iter()) {
        merged.push(diagnostic.clone());
    }

    if suppressed > 0 {
        // How many of the suppressed entries were errors: the two collections counted
        // every error they were given, retained or not, so the difference is exactly
        // the errors that were dropped.
        let errors_lost =
            (first.error_count() + second.error_count()).saturating_sub(merged.error_count());
        // A span is needed to build a diagnostic at all; any of them will do, and there
        // is at least one because a collection only suppresses after it has filled up.
        // (`limit == 0` is the exception, and then `retained == 0` and there is nothing
        // to replay a severity onto either.)
        if let Some(span) = first
            .iter()
            .chain(second.iter())
            .next()
            .map(|d| d.span().clone())
        {
            for index in 0..suppressed {
                let severity = if index < errors_lost {
                    Severity::Error
                } else {
                    Severity::Warning
                };
                merged.push(Diagnostic::new(severity, SuppressedSurplus, span.clone()));
            }
        }
    }
    merged
}

/// The condition of a diagnostic pushed only to be counted (see [`merge`]).
///
/// It never reaches a report: every value of it is pushed into a collection that is
/// already full, which stores nothing and counts one. It exists because techy's
/// counters move through [`Diagnostics::push`] and nothing else.
#[derive(Clone, Copy, Debug)]
struct SuppressedSurplus;

impl core::fmt::Display for SuppressedSurplus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("a diagnostic beyond the retention cap")
    }
}

impl techy::error::DiagnosticInfo for SuppressedSurplus {
    const IDENTIFIER: &'static str = "techxt.diagnostics.suppressed-surplus";
}

/// Builds a [`Converter`] (PLAN.md §11.2).
///
/// Every [`Options`] field has a setter here, so the common case never needs to name
/// the options struct:
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::builder().keep_comments(true).build()?;
/// assert_eq!(converter.latex_to_text("A% note\nB")?.text, "A\n% note\nB\n");
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub struct ConverterBuilder {
    options: Options,
    definitions: Option<DefinitionSet>,
    overrides: Vec<(CallableKind, Box<str>, TextRule)>,
    recovery: Recovery,
    unknown_macro_resolution: UnknownMacroResolution,
    descent_guard: StdDescentGuardInit,
    source_resolver: Option<Arc<dyn techy::source::SourceResolver<Option<String>>>>,
}

impl core::fmt::Debug for ConverterBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConverterBuilder")
            .field("options", &self.options)
            .field("recovery", &self.recovery)
            .field("unknown_macro_resolution", &self.unknown_macro_resolution)
            .field("descent_guard", &self.descent_guard)
            .field("has_source_resolver", &self.source_resolver.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for ConverterBuilder {
    fn default() -> ConverterBuilder {
        ConverterBuilder::new()
    }
}

impl ConverterBuilder {
    /// A builder with the shipped definitions, [`Options::default`], and tolerant
    /// recovery.
    pub fn new() -> ConverterBuilder {
        ConverterBuilder {
            options: Options::default(),
            definitions: None,
            overrides: Vec::new(),
            recovery: Recovery::Tolerant,
            unknown_macro_resolution: UnknownMacroResolution::default(),
            descent_guard: StdDescentGuardInit::default(),
            source_resolver: None,
        }
    }

    /// Use these definitions instead of the shipped ones (PLAN.md §11.2).
    ///
    /// Everything the converter knows about macros, environments and specials comes
    /// from here — both how they parse and how they render. Categories shadow in push
    /// order; see [`DefinitionSet`]. Left unset, the converter uses
    /// [`defs::standard`](crate::defs::standard); assemble a smaller set from the
    /// modules of [`techxt::defs`](crate::defs), or push a category of your own on top
    /// of `defs::standard()` to override part of it.
    pub fn definitions(mut self, definitions: DefinitionSet) -> ConverterBuilder {
        self.definitions = Some(definitions);
        self
    }

    /// Configure techy's descent guard, which is what keeps a pathologically nested
    /// document from costing the process its stack (DECISIONS.md D9).
    ///
    /// The choice is relayed to **both** sides: the parser, which spends stack per
    /// syntactic nesting level, and the renderer's fold, which spends it per tree
    /// nesting level. Reaching the limit is the one thing that abandons a conversion; a
    /// parse refusal comes back as `Err` from [`Converter::latex_to_text`], and a fold
    /// refusal as a `techxt.render-aborted` diagnostic with empty output.
    ///
    /// The default is techy's own — a fixed 250 KiB stack budget, which warns once at
    /// half the budget and names this method in its refusal. techxt deliberately does
    /// not raise it: a bigger fixed budget is unsafe on a small-stack thread, and a
    /// library cannot measure the real stack. A caller that can measure it should say
    /// so; a caller that wants a document's fate not to depend on the build profile
    /// should ask for a depth limit instead:
    ///
    /// ```
    /// use techxt::convert::StdDescentGuardInit;
    /// use techxt::Converter;
    ///
    /// let converter = Converter::builder()
    ///     .descent_guard(StdDescentGuardInit::depth_limit(24))
    ///     .build()?;
    /// assert!(converter.latex_to_text(&"{".repeat(400)).is_err());
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    ///
    /// Note that the default budget is *tight* in an unoptimized build — on the order
    /// of ten parse nesting levels — so a test that feeds deeply nested input should
    /// configure the guard rather than rely on it.
    pub fn descent_guard(mut self, descent_guard: StdDescentGuardInit) -> ConverterBuilder {
        self.descent_guard = descent_guard;
        self
    }

    /// Replace the whole options struct.
    pub fn options(mut self, options: Options) -> ConverterBuilder {
        self.options = options;
        self
    }

    /// Render a macro with `rule` instead of whatever its definition says (PLAN.md
    /// §10.3 step 1). The name carries no escape character.
    pub fn override_macro(mut self, name: impl Into<Box<str>>, rule: TextRule) -> ConverterBuilder {
        self.overrides
            .push((CallableKind::Macro, name.into(), rule));
        self
    }

    /// Render an environment with `rule` instead of whatever its definition says.
    pub fn override_environment(
        mut self,
        name: impl Into<Box<str>>,
        rule: TextRule,
    ) -> ConverterBuilder {
        self.overrides
            .push((CallableKind::Environment, name.into(), rule));
        self
    }

    /// Render a specials construct with `rule` instead of whatever its definition says.
    /// The name is the trigger characters.
    pub fn override_specials(
        mut self,
        name: impl Into<Box<str>>,
        rule: TextRule,
    ) -> ConverterBuilder {
        self.overrides
            .push((CallableKind::Specials, name.into(), rule));
        self
    }

    /// Resolve `\input`-like references at parse time.
    ///
    /// Source resolution is opt-in twice: the parser needs a resolver, and the resolver
    /// has to answer. techy interprets the reference string not at all — path
    /// resolution, sandboxing and cycle policy are the resolver's to define.
    pub fn source_resolver<M>(
        mut self,
        resolver: impl IntoSourceResolver<Option<String>, M>,
    ) -> ConverterBuilder {
        self.source_resolver = Some(resolver.into_source_resolver());
        self
    }

    /// Choose the parser's recovery policy. The default is [`Recovery::Tolerant`], which
    /// converts a malformed document as best it can and reports what was wrong.
    ///
    /// # It decides the fate of an unknown command too
    ///
    /// A command no definition claims cannot be shaped into an invocation by the parser,
    /// so what happens to it is a *parsing* question (DECISIONS.md D10). Under the
    /// default [`Tolerant`](Recovery::Tolerant) techxt registers a catch-all provider
    /// that resolves every command as an argument-less callable, and `\foo` converts to
    /// whatever [`Options::unknown_macro`] says, with a `techxt.unknown-macro` warning.
    /// Under [`Strict`](Recovery::Strict) the catch-all is *not* registered, so an
    /// unknown command is a hard parse error and
    /// [`latex_to_text`](Converter::latex_to_text) answers `Err` — strict means strict.
    ///
    /// Decouple the two with [`unknown_macro_resolution`](Self::unknown_macro_resolution),
    /// which overrides that pairing in either direction.
    pub fn recovery(mut self, recovery: Recovery) -> ConverterBuilder {
        self.recovery = recovery;
        self
    }

    /// Decide the fate of a command no definition claims, independently of
    /// [`recovery`](Self::recovery) (DECISIONS.md D10).
    ///
    /// The default, [`UnknownMacroResolution::FollowRecovery`], pairs the two: tolerant
    /// parses accept unknown commands and strict ones refuse them.
    /// [`Accept`](UnknownMacroResolution::Accept) keeps unknown commands convertible
    /// under strict recovery — everything *else* strict still applies — and
    /// [`Reject`](UnknownMacroResolution::Reject) refuses them under tolerant recovery,
    /// where techy's own recovery then turns `\foo` into literal characters and reports
    /// `core.specs.unresolvable-command`.
    ///
    /// ```
    /// use techxt::convert::UnknownMacroResolution;
    /// use techxt::Converter;
    /// use techy::error::Recovery;
    ///
    /// let strict = Converter::builder().recovery(Recovery::Strict).build()?;
    /// assert!(strict.latex_to_text(r"\nosuchmacro").is_err());
    ///
    /// let lenient = Converter::builder()
    ///     .recovery(Recovery::Strict)
    ///     .unknown_macro_resolution(UnknownMacroResolution::Accept)
    ///     .build()?;
    /// assert_eq!(lenient.latex_to_text(r"a \nosuchmacro b")?.text, "a b\n");
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    pub fn unknown_macro_resolution(
        mut self,
        resolution: UnknownMacroResolution,
    ) -> ConverterBuilder {
        self.unknown_macro_resolution = resolution;
        self
    }

    /// Build the converter, validating the definitions (PLAN.md §10.2).
    pub fn build(self) -> Result<Converter, BuildError> {
        let ConverterBuilder {
            options,
            definitions,
            overrides: override_list,
            recovery,
            unknown_macro_resolution,
            descent_guard,
            source_resolver,
        } = self;

        let definitions = definitions.unwrap_or_else(crate::defs::standard);
        // The definitions never see the recovery mode: what they need is the answer,
        // and resolving `FollowRecovery` is the builder's job (DECISIONS.md D10).
        let unknown_macro_fallback = match unknown_macro_resolution {
            UnknownMacroResolution::FollowRecovery => recovery == Recovery::Tolerant,
            UnknownMacroResolution::Accept => true,
            UnknownMacroResolution::Reject => false,
        };
        let spec_cx = SpecBuildCx::with_source_resolver(source_resolver.clone());
        let BuiltDefinitions { state, fallback } =
            definitions.build(&spec_cx, unknown_macro_fallback)?;

        let mut overrides = RuleTable::new();
        for (kind, name, rule) in override_list {
            // An override is written against whichever definition it lands on, so its
            // template cannot be checked against one entry's argument names; what *can*
            // be checked is its shape, and that `{body}` is only asked of an
            // environment.
            if let TextRule::Template(template) = &rule {
                let scope = TemplateScope::unchecked(kind == CallableKind::Environment);
                template
                    .validate(scope)
                    .map_err(|error| BuildError::Template {
                        definition: kind.write_construct(&name).into(),
                        template: template.source().into(),
                        error,
                    })?;
            }
            overrides.insert(kind, name, rule);
        }

        let mut driver = LatexlikeDriver::new(recovery);
        if let Some(resolver) = source_resolver {
            driver = driver.with_source_resolver(resolver);
        }
        let language = Language::new(driver, state).with_descent_guard_init(descent_guard);

        Ok(Converter {
            inner: Arc::new(Inner {
                language,
                config: RenderConfig {
                    options,
                    overrides,
                    fallback,
                },
                descent_guard,
            }),
        })
    }
}

/// Per-option setters, one per [`Options`] field (PLAN.md §11.2).
impl ConverterBuilder {
    /// See [`Options::math_mode`].
    pub fn math_mode(mut self, math_mode: MathMode) -> ConverterBuilder {
        self.options.math_mode = math_mode;
        self
    }

    /// See [`Options::math_expression_in`].
    pub fn math_expression_in(mut self, delimiters: MathWrapDelims) -> ConverterBuilder {
        self.options.math_expression_in = delimiters;
        self
    }

    /// See [`Options::matrix_delimiters`].
    pub fn matrix_delimiters(mut self, delimiters: MatrixDelims) -> ConverterBuilder {
        self.options.matrix_delimiters = delimiters;
        self
    }

    /// See [`Options::wrap_width`].
    pub fn wrap_width(mut self, wrap_width: Option<usize>) -> ConverterBuilder {
        self.options.wrap_width = wrap_width;
        self
    }

    /// See [`Options::keep_comments`].
    pub fn keep_comments(mut self, keep_comments: bool) -> ConverterBuilder {
        self.options.keep_comments = keep_comments;
        self
    }

    /// See [`Options::heading_style`].
    pub fn heading_style(mut self, heading_style: HeadingStyle) -> ConverterBuilder {
        self.options.heading_style = heading_style;
        self
    }

    /// See [`Options::footnote_style`].
    pub fn footnote_style(mut self, footnote_style: FootnoteStyle) -> ConverterBuilder {
        self.options.footnote_style = footnote_style;
        self
    }

    /// See [`Options::list_style`].
    pub fn list_style(mut self, list_style: ListStyle) -> ConverterBuilder {
        self.options.list_style = list_style;
        self
    }

    /// See [`Options::text_font`].
    pub fn text_font(mut self, text_font: FontStyle) -> ConverterBuilder {
        self.options.text_font = text_font;
        self
    }

    /// See [`Options::math_font`].
    pub fn math_font(mut self, math_font: FontStyle) -> ConverterBuilder {
        self.options.math_font = math_font;
        self
    }

    /// See [`Options::unknown_macro`].
    pub fn unknown_macro(mut self, policy: UnknownMacroPolicy) -> ConverterBuilder {
        self.options.unknown_macro = policy;
        self
    }

    /// See [`Options::unknown_env`].
    pub fn unknown_env(mut self, policy: UnknownEnvPolicy) -> ConverterBuilder {
        self.options.unknown_env = policy;
        self
    }

    /// See [`Options::unknown_specials`].
    pub fn unknown_specials(mut self, policy: UnknownSpecialsPolicy) -> ConverterBuilder {
        self.options.unknown_specials = policy;
        self
    }

    /// See [`Options::today`].
    pub fn today(mut self, today: Option<Box<str>>) -> ConverterBuilder {
        self.options.today = today;
        self
    }
}

/// Why a converter could not be built (PLAN.md §10.2).
///
/// Every definition is validated once, here, rather than failing halfway through a
/// document: a template that names an argument the definition does not declare is a
/// build error, not a hole in someone's output.
#[non_exhaustive]
#[derive(Debug)]
pub enum BuildError {
    /// A template did not compile against its definition's arguments.
    Template {
        /// The definition whose template it is.
        definition: Box<str>,
        /// The template, as written.
        template: Box<str>,
        /// What is wrong with it.
        error: TemplateError,
    },
    /// An argument code was not understood.
    ArgCode(ArgumentCodeError),
    /// Two categories in the definition set share a name.
    DuplicateCategory(Box<str>),
    /// techy could not finalize the parsing state the definitions build.
    State(FinalizeError),
}

impl core::fmt::Display for BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BuildError::Template {
                definition,
                template,
                error,
            } => write!(f, "‘{definition}’: template ‘{template}’: {error}"),
            BuildError::ArgCode(error) => write!(f, "invalid argument code: {error}"),
            BuildError::DuplicateCategory(name) => {
                write!(f, "two definition categories are named ‘{name}’")
            }
            BuildError::State(error) => {
                write!(
                    f,
                    "the definitions do not form a valid parsing state: {error}"
                )
            }
        }
    }
}

impl core::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            BuildError::Template { error, .. } => Some(error),
            BuildError::ArgCode(error) => Some(error),
            BuildError::DuplicateCategory(_) => None,
            BuildError::State(_) => None,
        }
    }
}

impl From<ArgumentCodeError> for BuildError {
    fn from(error: ArgumentCodeError) -> BuildError {
        BuildError::ArgCode(error)
    }
}

impl From<FinalizeError> for BuildError {
    fn from(error: FinalizeError) -> BuildError {
        BuildError::State(error)
    }
}

/// Everything a conversion can be told to do differently (PLAN.md §11.3).
///
/// Deliberately *not* here: whitespace behaviour. LaTeX's whitespace semantics are
/// exact, techxt implements them, and a knob would only ever make the output less
/// correct.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Options {
    /// How formulas are rendered. Default [`MathMode::Fancy`].
    pub math_mode: MathMode,
    /// What a sub-expression is wrapped in when its extent would otherwise be unclear.
    /// Default [`MathWrapDelims::Parens`].
    pub math_expression_in: MathWrapDelims,
    /// How the delimiters of a display matrix are drawn. Default
    /// [`MatrixDelims::Unicode`].
    pub matrix_delimiters: MatrixDelims,
    /// The column to wrap lines at, or `None` (the default) for no wrapping at all.
    pub wrap_width: Option<usize>,
    /// Whether comments survive into the output, each on a line of its own. Default
    /// `false` — a comment is a note to the source's reader, not to the text's.
    pub keep_comments: bool,
    /// How headings are rendered. Default [`HeadingStyle::NumberedUnderlined`].
    pub heading_style: HeadingStyle,
    /// Where footnote text goes. Default [`FootnoteStyle::Collected`].
    pub footnote_style: FootnoteStyle,
    /// The markers lists number and bullet themselves with.
    pub list_style: ListStyle,
    /// The font alphabet text starts in. Default [`FontStyle::Default`] (upright).
    pub text_font: FontStyle,
    /// The font alphabet math starts in. Default `FontStyle::Style(FontStyleKind::Italic)`,
    /// which is what makes variables look like variables.
    pub math_font: FontStyle,
    /// What to do with a macro no rule renders. Default [`UnknownMacroPolicy::Skip`].
    ///
    /// # What it can and cannot see
    ///
    /// A command *no definition claims at all* reaches this policy only through the
    /// catch-all provider, and that provider gives it a **zero-argument** spec — it has
    /// no way to know what arguments the absent definition would have declared. So on a
    /// genuinely absent name there is nothing to render and nothing to re-emit:
    /// [`RenderArgs`](UnknownMacroPolicy::RenderArgs) is indistinguishable from
    /// [`Skip`](UnknownMacroPolicy::Skip), and
    /// [`KeepSource`](UnknownMacroPolicy::KeepSource) recovers `\foo`, never the whole
    /// invocation `\foo{x}` — the `{x}` is an ordinary group that follows it and
    /// survives whatever this policy says. The four policies differ only on a macro that
    /// is *declared but rule-less*, which is the case that has arguments to act on.
    ///
    /// Whether the catch-all is registered at all is
    /// [`ConverterBuilder::unknown_macro_resolution`]'s decision (and, by default, the
    /// recovery mode's). Without it an unclaimed command never becomes a construct, and
    /// this option is inert for it — it still applies to the declared-but-rule-less
    /// macros, which parse either way.
    pub unknown_macro: UnknownMacroPolicy,
    /// What to do with an environment no rule renders. Default
    /// [`UnknownEnvPolicy::RenderBody`].
    pub unknown_env: UnknownEnvPolicy,
    /// What to do with a specials construct no rule renders. Default
    /// [`UnknownSpecialsPolicy::EmitChars`].
    pub unknown_specials: UnknownSpecialsPolicy,
    /// What `\today` renders as. `None` (the default) renders `<today>`: a no_std
    /// library has no clock, and a caller who wants a date knows which one.
    pub today: Option<Box<str>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            math_mode: MathMode::Fancy,
            math_expression_in: MathWrapDelims::Parens,
            matrix_delimiters: MatrixDelims::Unicode,
            wrap_width: None,
            keep_comments: false,
            heading_style: HeadingStyle::NumberedUnderlined,
            footnote_style: FootnoteStyle::Collected,
            list_style: ListStyle::default(),
            text_font: FontStyle::Default,
            math_font: FontStyle::Style(FontStyleKind::Italic),
            unknown_macro: UnknownMacroPolicy::Skip,
            unknown_env: UnknownEnvPolicy::RenderBody,
            unknown_specials: UnknownSpecialsPolicy::EmitChars,
            today: None,
        }
    }
}

/// How formulas are rendered (PLAN.md §9.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MathMode {
    /// Unicode symbols, unicode scripts, font alphabets — *and* the joiner, which
    /// spaces a formula the way a typesetter would: `𝑎 + 𝑏`, `𝑥² + 𝑦ᵢ`. The default.
    #[default]
    Fancy,
    /// The same symbol conversion without the joiner: pieces concatenate directly, so
    /// `$a + b$` comes out as `𝑎+𝑏`. Source whitespace is still ignored.
    Plain,
    /// Leave the formula as LaTeX, re-emitted from the parsed tree, however the formula
    /// was spelled: `$…$`, `\[…\]`, an `equation` or `align` environment, a matrix
    /// written outside a formula, an `\ensuremath`. The contents are shown rather than
    /// converted, so nothing inside them is rendered.
    Source,
}

/// How headings are rendered (PLAN.md §9.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeadingStyle {
    /// A section number, then the title, then a rule of underline characters. The
    /// default.
    #[default]
    NumberedUnderlined,
    /// The title and its underline, without a number.
    Underlined,
    /// The number and the title on one line, with no underline.
    Prefix,
    /// The title alone.
    Plain,
}

/// Where a footnote's text goes (PLAN.md §9.8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FootnoteStyle {
    /// A marker in the text, and the notes gathered into a block at the end. The
    /// default.
    #[default]
    Collected,
    /// The note's text in the running text, where the footnote was.
    Inline,
    /// Neither marker nor text.
    Skip,
}

/// The markers lists number and bullet themselves with (PLAN.md §9.4).
///
/// Both arrays *cycle*: a list nested deeper than the array is long starts over at the
/// beginning, so no nesting depth is ever without a marker.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListStyle {
    /// Bullets for `itemize`, by nesting depth among `itemize` lists.
    pub itemize_markers: Vec<Box<str>>,
    /// Number formats for `enumerate`, by nesting depth among `enumerate` lists.
    pub enumerate_formats: Vec<EnumFormat>,
}

impl Default for ListStyle {
    fn default() -> ListStyle {
        ListStyle {
            itemize_markers: alloc::vec!["•".into(), "–".into(), "*".into(), "·".into()],
            enumerate_formats: alloc::vec![
                EnumFormat {
                    style: CounterStyle::Arabic,
                    wrap: CounterWrap::Dot
                },
                EnumFormat {
                    style: CounterStyle::LowerAlpha,
                    wrap: CounterWrap::Parens
                },
                EnumFormat {
                    style: CounterStyle::LowerRoman,
                    wrap: CounterWrap::Dot
                },
                EnumFormat {
                    style: CounterStyle::UpperAlpha,
                    wrap: CounterWrap::Dot
                },
            ],
        }
    }
}

/// How one level of an `enumerate` numbers its items: `1.`, `(a)`, `i.`, `A.`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumFormat {
    /// What the counter counts in.
    pub style: CounterStyle,
    /// What punctuates it.
    pub wrap: CounterWrap,
}

/// What an `enumerate` counter counts in (PLAN.md §9.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CounterStyle {
    /// `1`, `2`, `3`.
    Arabic,
    /// `a`, `b`, `c`.
    LowerAlpha,
    /// `A`, `B`, `C`.
    UpperAlpha,
    /// `i`, `ii`, `iii`.
    LowerRoman,
    /// `I`, `II`, `III`.
    UpperRoman,
}

/// What punctuates an `enumerate` counter (PLAN.md §9.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CounterWrap {
    /// `1.`
    Dot,
    /// `(1)`
    Parens,
}

/// How the definitions decide the fate of a command no category defines
/// (DECISIONS.md D10).
///
/// This is a *parsing* setting, chosen with
/// [`ConverterBuilder::unknown_macro_resolution`] alongside
/// [`recovery`](ConverterBuilder::recovery), because techy cannot shape an invocation it
/// has no definition for: whether an unclaimed `\foo` becomes a callable node at all is
/// settled before rendering, and only then does [`Options::unknown_macro`] — a rendering
/// setting — get to say what it looks like.
///
/// The two interact in exactly one place: with no catch-all registered, an unknown
/// command never reaches the renderer as a construct, so [`Options::unknown_macro`] is
/// inert for it (it still applies to a macro that *is* declared but has no rule).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnknownMacroResolution {
    /// Default. Follow the recovery mode: [`Recovery::Tolerant`] registers the catch-all,
    /// so an unknown command parses as a no-argument callable and reaches
    /// [`Options::unknown_macro`]; [`Recovery::Strict`] registers nothing, so an unknown
    /// command is a hard parse error.
    #[default]
    FollowRecovery,
    /// Always register the catch-all, even under [`Recovery::Strict`]: an unknown command
    /// is a no-argument callable and [`Options::unknown_macro`] decides what it renders
    /// as.
    Accept,
    /// Never register the catch-all, even under [`Recovery::Tolerant`]: techy's own
    /// unresolvable-command handling applies — an error diagnostic, and under tolerant
    /// recovery the command survives as literal characters.
    Reject,
}

/// What to do with a macro no rule renders (PLAN.md §10.6).
///
/// Whichever it is, a `techxt.unknown-macro` diagnostic is raised — a warning, not an
/// error: the document converted, one construct in it did not.
///
/// # A macro no definition claims at all
///
/// techy cannot shape an invocation it has no definition for, so techxt registers a
/// catch-all provider underneath every definition it has: an unclaimed `\foo` resolves
/// to a generic spec that takes **no arguments** and carries no rule, which is what
/// brings it here rather than leaving it to techy's error recovery.
///
/// The consequence is deliberate. Since the generic spec takes no arguments, `\foo{x}`
/// is an unknown macro followed by an ordinary group: whatever this policy decides about
/// `\foo`, the `x` inside the braces is text and survives. [`RenderArgs`](Self::RenderArgs)
/// therefore differs from [`Skip`](Self::Skip) only for a macro that *is* defined —
/// declared with arguments, but with no rule to render them — and
/// [`KeepSource`](Self::KeepSource) can only re-emit `\foo`, never the invocation
/// `\foo{x}` it was written as.
///
/// Whether an unclaimed command reaches this policy at all is settled at parse time by
/// [`ConverterBuilder::unknown_macro_resolution`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnknownMacroPolicy {
    /// Render nothing at all. The default: an unknown macro is usually formatting, and
    /// formatting is what plain text drops anyway.
    #[default]
    Skip,
    /// Render the macro's arguments, in order, as if the macro were transparent.
    RenderArgs,
    /// Emit the macro's LaTeX source, protected from wrapping.
    KeepSource,
    /// Emit `<name>`, so the reader can see that something was there.
    Placeholder,
}

/// What to do with an environment no rule renders (PLAN.md §10.6).
///
/// Whichever it is, a `techxt.unknown-environment` diagnostic is raised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnknownEnvPolicy {
    /// Render the body and drop the wrapper. The default: an unknown wrapper rarely
    /// means the text inside it is unwanted.
    #[default]
    RenderBody,
    /// Render nothing at all, body included.
    Skip,
    /// Emit the environment's LaTeX source as a preformatted block.
    KeepSource,
}

/// What to do with a specials construct no rule renders (PLAN.md §10.6).
///
/// Whichever it is, a `techxt.unknown-specials` diagnostic is raised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnknownSpecialsPolicy {
    /// Emit the trigger characters themselves. The default: they were, after all, in
    /// the document.
    #[default]
    EmitChars,
    /// Render nothing at all.
    Skip,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn converter_is_shareable() {
        assert_send_sync::<Converter>();
        assert_send_sync::<Options>();
    }

    /// `BuildError::State` is a defensive path: at this techy revision nothing a
    /// caller can write makes the parsing state refuse to finalize (the seed never
    /// fails, and the whole-stack replacement techxt applies targets no name and so
    /// cannot miss). It is still a variant callers may match on, so its reporting is
    /// pinned here.
    #[test]
    fn a_state_error_reports_what_techy_said() {
        let error = BuildError::State(FinalizeError::new("no"));
        assert!(alloc::format!("{error}").contains("no"));
        assert!(core::error::Error::source(&error).is_none());
    }

    #[test]
    fn default_options_match_the_plan() {
        let options = Options::default();
        assert_eq!(options.math_mode, MathMode::Fancy);
        assert_eq!(options.math_expression_in, MathWrapDelims::Parens);
        assert_eq!(options.matrix_delimiters, MatrixDelims::Unicode);
        assert_eq!(options.wrap_width, None);
        assert!(!options.keep_comments);
        assert_eq!(options.heading_style, HeadingStyle::NumberedUnderlined);
        assert_eq!(options.footnote_style, FootnoteStyle::Collected);
        assert_eq!(options.text_font, FontStyle::Default);
        assert_eq!(options.math_font, FontStyle::Style(FontStyleKind::Italic));
        assert_eq!(options.unknown_macro, UnknownMacroPolicy::Skip);
        assert_eq!(options.unknown_env, UnknownEnvPolicy::RenderBody);
        assert_eq!(options.unknown_specials, UnknownSpecialsPolicy::EmitChars);
        assert_eq!(options.today, None);

        let list = ListStyle::default();
        assert_eq!(list.itemize_markers.len(), 4);
        assert_eq!(&*list.itemize_markers[0], "•");
        assert_eq!(list.enumerate_formats.len(), 4);
        assert_eq!(list.enumerate_formats[0].style, CounterStyle::Arabic);
        assert_eq!(list.enumerate_formats[1].wrap, CounterWrap::Parens);
    }
}
