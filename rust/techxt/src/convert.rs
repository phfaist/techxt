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

use techy::error::{DiagnosticData, DiagnosticInfo};
use techy::latexlike::LatexlikeDriver;
use techy_xp::lang::XpDriver;
use techy_xp::presets::{
    CategoryCodeChangeUnsupported, ConditionalUnsupported, CounterCommandUnsupported,
    CsNameUnsupported, ExpandAfterUnsupported, NoExpandUnsupported, RegisterAllocationUnsupported,
    RegisterUnsupported,
};

use crate::def::{
    BuiltDefinitions, CallableKind, DefinitionSet, RuleTable, ScopeFrame, SpecBuildCx,
    TemplateError, TemplateScope, TextRule,
};
use crate::flow::Flow;
use crate::layout::{render_with_regions, LayoutOptions};
use crate::render::{RenderConfig, RenderState, TextRenderer};

pub use crate::mathfmt::{FontStyle, FontStyleKind, MathWrapDelims, MatrixDelims};

// --------------------------------------------------- techy and techy-xp, re-exported
//
// Every techy (and techy-xp) type that appears in techxt's own public API is
// re-exported here, so that using techxt takes one dependency and not three (PLAN.md
// §2). They are the upstream types, not aliases: a `SourceResolver` implemented
// against `techxt::convert` is the very trait techy's parser calls, and a
// `Diagnostics` returned by a conversion is the one techy built. Naming `techy` or
// `techy_xp` directly keeps working and is exactly equivalent — this is a convenience
// and a semver statement, not an abstraction layer.

/// techy's descent guard, re-exported so that embedders configuring
/// [`ConverterBuilder::descent_guard`] need not name `techy::core` themselves.
///
/// Despite the name it is pure `alloc` — "Std" is "the standard guard", not the standard
/// library — and works on a `no_std` target like everything else here.
pub use techy::core::{StdDescentGuard, StdDescentGuardInit};

/// The parse tree and its nodes: what [`Converter::tree_to_text`] converts. What a
/// [`TextHandler`](crate::def::TextHandler) is handed is techxt's own language-erased
/// [`NodeView`](crate::render::NodeView) instead — and [`NodeId`] is what that view's
/// [`id`](crate::render::NodeView::id) answers with, the identity a tree walk that must
/// not count a node twice keys on.
pub use techy::core::node::{NodeId, NodeRef, NodeSlice, NodeTree};

/// The bound [`Converter::tree_to_text`] takes its tree under (PLAN.md §11.1): any
/// latexlike language whose sources are origin-tagged and whose body slots are marked,
/// not only the [`LatexlikeXp`] this converter parses with.
///
/// It is auto-implemented — see [`RenderLang`] for what those two facts are, and why
/// they are the only ones techxt's render side is concrete about.
pub use crate::render::RenderLang;
/// The language family [`RenderLang`] refines, and the trait an embedder reads that
/// bound through.
pub use techy::latexlike::LatexlikeLang;

/// The techy pieces a definition of your own is built from: an argument spec for
/// [`MacroDef::arg_spec`](crate::def::MacroDef::arg_spec) and a callable spec for
/// [`MacroDef::spec`](crate::def::MacroDef::spec).
pub use techy::core::specs::{ArgumentSpec, CallableSpec};
/// See [`ArgumentSpec`].
pub use techy::latexlike::EnvironmentSpec;
/// The parsing language every techxt type is concrete over (PLAN.md §11.1): techy's
/// latexlike preset carrying techy-xp's expanding token reader, which is what
/// `\newcommand` and `\def` are honored through (PLAN.md §16 M9).
///
/// The reader expands only what a *definer* has written into a definition scope, and
/// [`defs::preamble`](crate::defs::preamble) seeds techy-xp's definers into one — so a
/// document that defines a macro gets it expanded, and a document that defines none
/// parses exactly as techy's own `Latexlike` parses it. Turn the definers off with
/// [`ConverterBuilder::macro_definitions`].
pub use techy_xp::lang::LatexlikeXp;

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

/// Where a run of the output came from, for the [`regions`](Conversion::regions) table:
/// the range type and the tag it carries.
///
/// Both are defined where they are produced — [`OutputRegion`] by the
/// [layout engine](crate::layout), [`VerbatimProvenance`] on the
/// [flow item](crate::flow::FlowItem) that carries it — and re-exported here so that an
/// embedder reading a [`Conversion`] stays at one import.
pub use crate::{flow::VerbatimProvenance, layout::OutputRegion};

/// The result of a conversion (PLAN.md §11.1).
#[derive(Clone, Debug)]
pub struct Conversion {
    /// The converted text. Ends with exactly one newline unless it is empty.
    pub text: String,
    /// Everything the conversion reported, parse diagnostics first.
    pub diagnostics: Diagnostics<Option<String>>,
    /// The runs of [`text`](Self::text) that are *not* converted text but preformatted
    /// content copied through — a `verbatim` body, a construct kept as source, a
    /// formula in [`MathMode::Source`] — as byte ranges into
    /// [`text`](Self::text), in output order and never overlapping.
    ///
    /// This is a side table, not markup: the text is exactly what it would be without
    /// it, so anything the user copies or saves is untouched, and a consumer with no
    /// use for the regions ignores the field. It is also the one fact about the output
    /// that cannot be recovered from the string afterwards — `\$` converts to a `$`
    /// that no amount of scanning distinguishes from the `$` of a source-mode formula
    /// — which is why an embedder rendering into anything richer than a terminal (a
    /// GUI styling verbatim in monospace, an HTML backend, a math typesetter) needs it
    /// reported rather than inferred.
    ///
    /// ```
    /// use techxt::convert::{MathMode, VerbatimProvenance};
    /// use techxt::Converter;
    ///
    /// let converter = Converter::builder().math_mode(MathMode::Source).build()?;
    /// let out = converter.latex_to_text("but not these \\$3 and $x$ values")?;
    /// assert_eq!(out.text, "but not these $3 and $x$ values\n");
    /// // one region, and it is the formula rather than the escaped dollar
    /// assert_eq!(out.regions.len(), 1);
    /// assert_eq!(&out.text[out.regions[0].start..out.regions[0].end], "$x$");
    /// assert_eq!(
    ///     out.regions[0].kind,
    ///     VerbatimProvenance::MathSource { display: false }
    /// );
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    pub regions: Vec<OutputRegion>,
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
    language: Language<LatexlikeXp>,
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
        let (text, regions) = self.lay_out(&flow);
        Ok(Conversion {
            text,
            // Parse diagnostics come first: they describe what the tree even is.
            diagnostics: merge(&parsed.diagnostics, &render_diagnostics),
            regions,
        })
    }

    /// Convert a tree that has already been parsed — or transformed.
    ///
    /// Infallible: everything that can go wrong during rendering is a diagnostic.
    ///
    /// The tree's language is whatever parsed it: any [`RenderLang`], which is every
    /// latexlike language whose sources are origin-tagged and whose body slots are
    /// marked. A converter's own [`language`](Self::language) is [`LatexlikeXp`], and a
    /// tree it parsed is the inferred case; but rendering reads *payloads* (PLAN.md
    /// §1.6) and no techy-xp facility, so a tree parsed by anybody's latexlike language
    /// — plain techy's [`Latexlike`](techy::latexlike::Latexlike), a language of your
    /// own — converts through the same rules. What such a tree cannot carry is techxt's
    /// rules *inside* its specs, so its constructs are matched by name instead (PLAN.md
    /// §10.3 step 3).
    pub fn tree_to_text<LLL: RenderLang>(&self, tree: &NodeTree<LLL>) -> Conversion {
        let (flow, diagnostics) = self.tree_to_flow(tree);
        let (text, regions) = self.lay_out(&flow);
        Conversion {
            text,
            diagnostics,
            regions,
        }
    }

    /// Convert a tree to a [`Flow`], stopping short of layout.
    ///
    /// Use this to lay the result out with options of your own, to inspect the token
    /// stream, or to splice the flow into a larger one. It renders the tree of any
    /// [`RenderLang`], exactly as [`tree_to_text`](Self::tree_to_text) does — the two
    /// differ only in whether the flow is laid out.
    pub fn tree_to_flow<LLL: RenderLang>(
        &self,
        tree: &NodeTree<LLL>,
    ) -> (Flow, Diagnostics<Option<String>>) {
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
    pub fn language(&self) -> &Language<LatexlikeXp> {
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

    /// Lay a flow out with this converter's options, keeping the region table layout
    /// records as it writes.
    fn lay_out(&self, flow: &Flow) -> (String, Vec<OutputRegion>) {
        render_with_regions(
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
///
/// Rebuilding the collection is also where each entry passes through
/// [`at_techxt_severity`], which is what makes a techy-xp refusal a warning.
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
    // Demotions are counted because the replay below reasons from error *counts*: a
    // refusal that arrived as an error and is retained as a warning would otherwise look
    // like an error the cap ate, and be replayed as one.
    let mut demoted = 0;
    for diagnostic in first.iter().chain(second.iter()) {
        let entry = at_techxt_severity(diagnostic);
        if entry.severity() != diagnostic.severity() {
            demoted += 1;
        }
        merged.push(entry);
    }

    if suppressed > 0 {
        // How many of the suppressed entries were errors: the two collections counted
        // every error they were given, retained or not, so the difference is exactly
        // the errors that were dropped — once the demotions above are given back, since
        // those errors were not lost but reclassified.
        //
        // A *suppressed* refusal cannot be reclassified: the cap kept its severity and
        // threw away its identity, so it is replayed as the error it was recorded as.
        // Past the retention cap, then, a document made entirely of refusals can still
        // report errors; that is the price of a surplus techy no longer holds.
        let errors_lost = (first.error_count() + second.error_count())
            .saturating_sub(demoted)
            .saturating_sub(merged.error_count());
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

/// A parse diagnostic as techxt reports it: techy-xp's **refusals** demoted from error
/// severity to warning, everything else exactly as techy raised it (PLAN.md §10.6, as
/// amended for M9 phase 3).
///
/// # Why techxt overrules the severity techy-xp chose
///
/// A refusal is techy-xp saying *"this command is `\ifx`, and I do not implement
/// conditionals"*. It raises it through techy's recovery entry point, which under
/// [`Recovery::Strict`] must abort — so at the raise site it can only be an error, and
/// techy-xp is right to make it one: for a host expanding macros in earnest, a document
/// reaching a refusal has not been understood.
///
/// techxt is not that host. Its severity convention ([`diag`](crate::diag)) calls a
/// construct it has never heard of a **warning**: the document converted, one construct
/// in it did not. A command techxt understands well enough to *refuse by name* cannot
/// rank worse than one it has never heard of — that would make `\expandafter` fail a
/// conversion that `\qqq` passes, and would put a document whose text is entirely correct
/// behind a non-zero CLI exit code. So the identifier and the message are kept exactly as
/// techy-xp wrote them, and only the severity is techxt's.
///
/// This is the *refusals* boundary alone — the eight conditions of
/// [`Refusal`](techy_xp::presets::Refusal), one per recovery. The expansion budgets
/// (`techy-xp.expand.*-budget-exceeded`) stay errors deliberately: a budget is not a
/// missing feature, it is a document that was cut off, and the text really is incomplete.
///
/// # What the rebuild costs
///
/// techy has no public way to restamp a diagnostic's severity, so the demoted entry is a
/// new one built from the same condition payload, and the **traceback frames do not
/// survive** — the payload, the identifier, the message and the span all do. What is lost
/// is measurable and small: a refusal carries exactly one frame, titled *"unsupported
/// ‘\ifx’"*, which says what the message already says.
fn at_techxt_severity(diagnostic: &Diagnostic<Option<String>>) -> Diagnostic<Option<String>> {
    if diagnostic.severity() != Severity::Error {
        return diagnostic.clone();
    }
    let data = diagnostic.data();
    let span = diagnostic.span();
    // One arm per refusal condition, matched by *type* rather than by identifier string
    // (techy's rule, [`diag`](crate::diag)): the downcast is what makes the payload
    // available to rebuild with, and a techy-xp condition that is renamed or retyped
    // fails to compile here instead of quietly staying an error.
    // `defs_macros::every_refusal_condition_is_demoted` pins the list against techy-xp's
    // own `REFUSED_COMMANDS` table, so a refusal added upstream fails a test rather than
    // slipping through.
    as_warning::<CategoryCodeChangeUnsupported>(data, span)
        .or_else(|| as_warning::<ExpandAfterUnsupported>(data, span))
        .or_else(|| as_warning::<NoExpandUnsupported>(data, span))
        .or_else(|| as_warning::<CsNameUnsupported>(data, span))
        .or_else(|| as_warning::<RegisterUnsupported>(data, span))
        .or_else(|| as_warning::<RegisterAllocationUnsupported>(data, span))
        .or_else(|| as_warning::<CounterCommandUnsupported>(data, span))
        .or_else(|| as_warning::<ConditionalUnsupported>(data, span))
        .unwrap_or_else(|| diagnostic.clone())
}

/// The condition behind `data` as a warning, if `data` is a `T` at all (see
/// [`at_techxt_severity`]).
fn as_warning<T: DiagnosticInfo>(
    data: &dyn DiagnosticData,
    span: &SourceSpan<Option<String>>,
) -> Option<Diagnostic<Option<String>>> {
    data.downcast_ref::<T>()
        .map(|condition| Diagnostic::new(Severity::Warning, condition.clone(), span.clone()))
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
    macro_definitions: MacroDefinitions,
    expansion_depth_limit: usize,
    expansion_count_limit: usize,
    descent_guard: StdDescentGuardInit,
    source_resolver: Option<Arc<dyn techy::source::SourceResolver<Option<String>>>>,
}

impl core::fmt::Debug for ConverterBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConverterBuilder")
            .field("options", &self.options)
            .field("recovery", &self.recovery)
            .field("unknown_macro_resolution", &self.unknown_macro_resolution)
            .field("macro_definitions", &self.macro_definitions)
            .field("expansion_depth_limit", &self.expansion_depth_limit)
            .field("expansion_count_limit", &self.expansion_count_limit)
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
    /// The default of [`expansion_depth_limit`](Self::expansion_depth_limit): 64 nested
    /// expansions, where techy-xp's own default is 256.
    pub const DEFAULT_EXPANSION_DEPTH_LIMIT: usize = 64;

    /// The default of [`expansion_count_limit`](Self::expansion_count_limit): 2 000
    /// expansions per parse, where techy-xp's own default is 100 000. That method carries
    /// the measurement the number comes from — and the reason a library converting
    /// untrusted input cannot use the upstream one.
    pub const DEFAULT_EXPANSION_COUNT_LIMIT: usize = 2_000;

    /// A builder with the shipped definitions, [`Options::default`], and tolerant
    /// recovery.
    pub fn new() -> ConverterBuilder {
        ConverterBuilder {
            options: Options::default(),
            definitions: None,
            overrides: Vec::new(),
            recovery: Recovery::Tolerant,
            unknown_macro_resolution: UnknownMacroResolution::default(),
            macro_definitions: MacroDefinitions::default(),
            expansion_depth_limit: ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT,
            expansion_count_limit: ConverterBuilder::DEFAULT_EXPANSION_COUNT_LIMIT,
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

    /// Decide whether the macro-defining commands are *honoured* or only *declared*
    /// (PLAN.md §16 M9).
    ///
    /// The default, [`MacroDefinitions::Honored`], makes `\newcommand`, `\def` and their
    /// relatives define macros that later uses expand — and registers techy-xp's
    /// refusals, so `\expandafter` and the TeX conditionals are diagnosed by name rather
    /// than left unknown.
    ///
    /// ```
    /// use techxt::convert::MacroDefinitions;
    /// use techxt::Converter;
    ///
    /// let latex = r"\newcommand{\greet}[1]{Hello #1!}\greet{world}";
    ///
    /// let honored = Converter::standard();
    /// assert_eq!(honored.latex_to_text(latex)?.text, "Hello world!\n");
    ///
    /// // Declared: the definition is consumed and dropped, and `\greet` is an unknown
    /// // macro that takes no arguments — so only its `{world}` group survives.
    /// let declared = Converter::builder()
    ///     .macro_definitions(MacroDefinitions::Declared)
    ///     .build()?;
    /// assert_eq!(declared.latex_to_text(latex)?.text, "world\n");
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    ///
    /// # When you would want it off
    ///
    /// Expansion costs parse time and memory proportional to what a document's macros
    /// expand to, and it changes what the tree *is*: a defined macro's invocation is
    /// replaced by the tokens of its body, so a consumer that walks the tree looking for
    /// the author's own commands no longer finds them. Turn it off for a pipeline that
    /// wants the document's surface syntax, for a corpus where an adversarial input's
    /// expansion budget is not worth spending, or to reproduce techxt 0.1.0's output
    /// exactly.
    pub fn macro_definitions(mut self, macro_definitions: MacroDefinitions) -> ConverterBuilder {
        self.macro_definitions = macro_definitions;
        self
    }

    /// How deeply one expansion may nest inside another before the reader refuses
    /// (PLAN.md §16 M9). Default
    /// [`DEFAULT_EXPANSION_DEPTH_LIMIT`](Self::DEFAULT_EXPANSION_DEPTH_LIMIT), 64.
    ///
    /// A macro whose body invokes a macro whose body invokes a macro is three frames
    /// deep. Reaching the limit costs *that* expansion and nothing else: the reader
    /// reports `techy-xp.expand.expansion-depth-budget-exceeded` — an **error**, naming
    /// the macro — and the invocation stays in the document unexpanded, which then reads
    /// as an unknown macro. The parse continues.
    ///
    /// This budget is what catches a runaway that *nests* — `\def\a{\b x}\def\b{\a y}\a`,
    /// where each body has something left to serve after the invocation it names, so its
    /// frame is still live when the next one is pushed.
    /// [`expansion_count_limit`](Self::expansion_count_limit) catches the flat ones: a
    /// body that ends in its own invocation (`\def\a{\a}\a`, and mutual recursion spelled
    /// the same way) has its frame popped before the successor is pushed, and never grows
    /// the stack at all. techy-xp does no cycle detection; the two budgets are what
    /// replace it.
    ///
    /// 64 is enough for any nesting a document writes on purpose — the deepest macro
    /// chains in real preambles are a handful of frames — and techy-xp's own 256 is one
    /// call away for a document that needs it. Note that it does *not* bound the runaways
    /// [`expansion_count_limit`](Self::expansion_count_limit) describes: those are the
    /// count budget's business, and measurably indifferent to this one.
    pub fn expansion_depth_limit(mut self, limit: usize) -> ConverterBuilder {
        self.expansion_depth_limit = limit;
        self
    }

    /// How many expansions one parse may start before the reader refuses (PLAN.md §16
    /// M9). Default
    /// [`DEFAULT_EXPANSION_COUNT_LIMIT`](Self::DEFAULT_EXPANSION_COUNT_LIMIT), 2 000.
    ///
    /// One macro invocation is one expansion, so this is a ceiling on how much expanded
    /// text a document may produce — a *memory* bound as much as a time one, since a
    /// rewindable reader reclaims no frame it has entered. Reaching it reports
    /// `techy-xp.expand.expansion-count-budget-exceeded` — an **error** — and every
    /// later invocation is left unexpanded rather than the parse being abandoned. The
    /// budget is per *reader*, so an `\input`ed file gets an allowance of its own.
    ///
    /// ```
    /// use techxt::Converter;
    ///
    /// // A runaway definition is stopped by the budget rather than by cycle detection,
    /// // which techy-xp deliberately does not do.
    /// let converter = Converter::builder().expansion_count_limit(50).build()?;
    /// let conversion = converter.latex_to_text(r"\def\x{\x}\x")?;
    /// assert!(conversion.diagnostics.has_errors());
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    ///
    /// # Why the default is a fiftieth of techy-xp's
    ///
    /// techy-xp defaults to 100 000, which is the right number for a host that trusts
    /// its input. techxt converts documents it did not write, and at that allowance two
    /// one-line documents are a denial of service — measured through this converter, in
    /// an unoptimized build, on a 2 MiB thread stack:
    ///
    /// | document | at 2 000 | at 100 000 |
    /// |---|---|---|
    /// | `\def\x{\x}\x` (flat loop) | 0.09 s | over three minutes |
    /// | `\def\x{\x\x}\x` (doubling) | 0.23 s | **the process aborts** |
    /// | `\def\x{\x\x\x}\x` | 0.35 s | **the process aborts** |
    ///
    /// The first is upstream's known quadratic cost in position canonicalization (7.4 s
    /// at 20 000, and it grows with the square). The other two are worse than slow: the
    /// structure they build is dropped recursively, and the drop overflows the stack —
    /// which no guard catches, because [`descent_guard`](Self::descent_guard) bounds
    /// parsing and folding, not destruction. The abort threshold scales with the
    /// thread's stack, at roughly three thousand expansions per MiB (aborting between
    /// 6 000 and 8 000 on a 2 MiB stack, and between 3 000 and 4 000 on a 1 MiB one), so
    /// **2 000 is chosen to clear it on a small-stack thread**, not just on a generous
    /// one, and to keep every shape above well under a second even unoptimized.
    ///
    /// # It is a working ceiling too, and that is the cost
    ///
    /// 2 000 expansions is a document with two thousand macro invocations in it — a
    /// long, macro-heavy paper reaches that, and then converts with its remaining
    /// invocations unexpanded and an error in its diagnostics. That is the deliberate
    /// trade: a default that is safe on input nobody vouched for, and a knob for a caller
    /// who *does* vouch for it. Raising it to techy-xp's own 100 000 is one call, and
    /// costs what the table above says on a document that abuses it.
    pub fn expansion_count_limit(mut self, limit: usize) -> ConverterBuilder {
        self.expansion_count_limit = limit;
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
            macro_definitions,
            expansion_depth_limit,
            expansion_count_limit,
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

        // The driver is built *first*, before the definitions, because the definitions
        // read two things off it: the names of the two definition scopes the definer
        // specs write into, and the providers that seed those scopes. Both must be the
        // driver's own — the leak hook that lets `\gdef` escape a group keys on the
        // global scope's name, and a definer writing into a name the seed does not hold
        // is not an error anywhere (techy creates the scope innermost, silently
        // inverting techy-xp's ordering invariant). Reading them off one object is what
        // makes them agree by construction.
        //
        // The latexlike driver carries the parse-time behavior; wrapping it in
        // `XpDriver` is what installs the expanding token reader macro definitions are
        // honored through (PLAN.md §16 M9), and it does so whichever way
        // `macro_definitions` went: with no definer seeded the reader has nothing to
        // expand and parses byte for byte as the bare `LatexlikeDriver` did — techy-xp's
        // lockstep property. The two expansion budgets are techxt's own, lower than
        // techy-xp's defaults for the reason `expansion_count_limit` gives. The
        // resolver is handed to the wrapper, not the inner driver: the wrapper decides
        // where it is needed.
        let mut driver = XpDriver::wrapping(LatexlikeDriver::new(recovery))
            .with_expansion_depth_limit(expansion_depth_limit)
            .with_expansion_count_limit(expansion_count_limit);
        if let Some(resolver) = source_resolver.clone() {
            driver = driver.with_source_resolver(resolver);
        }
        let frame = match macro_definitions {
            MacroDefinitions::Declared => ScopeFrame::declared(),
            _ => ScopeFrame::honored(&driver),
        };
        let spec_cx = SpecBuildCx::for_converter(
            source_resolver,
            macro_definitions,
            driver.local_scope_name(),
            driver.global_scope_name(),
        );
        let BuiltDefinitions { state, fallback } =
            definitions.build(&spec_cx, unknown_macro_fallback, &frame)?;

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

/// Whether the macro-defining commands are honoured or only declared (PLAN.md §16 M9).
///
/// This is a *parsing* setting, chosen with
/// [`ConverterBuilder::macro_definitions`]: whether `\newcommand\greet{Hi}` installs a
/// macro that `\greet` later expands to is settled while the document is being read, by
/// the token reader techy-xp puts under techxt's parser.
///
/// # What "honoured" covers
///
/// `\newcommand`, `\renewcommand`, `\providecommand`, `\newenvironment`,
/// `\renewenvironment`, `\def`, `\gdef`, `\let`, `\edef`, `\xdef` and
/// `\NewDocumentCommand` — the definers techy-xp ships. Definitions are scoped as TeX
/// scopes them: `\def` reverts with its enclosing group (an environment body included),
/// `\gdef` escapes every group it is written in.
///
/// It also decides whether techy-xp's **refusals** are registered. They are the reason
/// `\expandafter` reports *"techy-xp does not implement expansion control"* instead of
/// being an unknown command, and they never shadow a name techxt declares itself.
/// techxt reports each of them as a **warning**, and reports it once — in place of the
/// `techxt.unknown-macro` the same construct would otherwise also earn, since the refusal
/// names the missing feature and the unknown-macro warning would only say that techxt has
/// no rule for it (PLAN.md §10.6).
///
/// # What stays out either way
///
/// techy-xp implements no conditionals, category codes, registers or counters, so
/// `\ifx`, `\catcode`, `\count` and `\setcounter` define nothing under either setting.
/// `\DeclareMathOperator` is techxt's own declaration and is likewise unaffected: it is
/// read and dropped, and the operator it names is not defined.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MacroDefinitions {
    /// Default. A definition made in the document takes effect, and a later use of the
    /// defined macro expands.
    #[default]
    Honored,
    /// The defining commands are *declared* only: their arguments are consumed so that a
    /// definition body cannot leak into the text, and nothing is defined — so a later use
    /// of the macro is an unknown command like any other. This is techxt 0.1.0's
    /// behaviour exactly, and no refusal is registered either.
    Declared,
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
