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

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use techy::core::node::NodeTree;
use techy::core::{FinalizeError, Language, ParsingState, StdDescentGuardInit};
use techy::error::{Diagnostics, ParseError, Recovery};
use techy::latexlike::{ArgumentCodeError, Latexlike, LatexlikeDriver};
use techy::recompose::TreeRecomposer;
use techy::source::IntoSourceResolver;

use crate::def::{CallableKind, RuleTable, TemplateError, TextRule};
use crate::flow::Flow;
use crate::layout::{render, LayoutOptions};
use crate::render::{RenderConfig, RenderState, TextRenderer};

pub use crate::mathfmt::{FontStyle, FontStyleKind, MathWrapDelims, MatrixDelims};

/// How deeply the parser may nest before it refuses (PLAN.md §1.7: no document input
/// may cost the process its stack).
///
/// Recursive-descent parsing spends stack per nesting level, and the only thing between
/// a pathological document and a stack overflow — which is an abort, not a catchable
/// panic — is techy's descent guard. Its default is a *stack budget*, which is
/// adaptive but makes a document's fate depend on the build profile and on which
/// thread it is parsed on; techxt uses a fixed depth instead, so that the same document
/// behaves the same way everywhere.
///
/// The number is chosen against the *unoptimized* build, which is where a frame is
/// most expensive: measured against this techy revision, a parse descent costs on the
/// order of 12 KiB of stack in a debug build, so 64 descents fit in about 0.8 MiB and
/// leave better than a factor of two of headroom on Rust's 2 MiB default thread stack.
/// A parse costs roughly two descents per syntactic nesting level, so this admits
/// documents nesting some thirty levels deep — far past anything anyone writes, and
/// far short of what overflows. Optimized builds are several times cheaper again.
const PARSE_DESCENT_LIMIT: usize = 64;

/// How deeply the renderer may nest before it gives up (PLAN.md §10.4).
///
/// A traversal costs exactly one descent per tree nesting level (the re-entrant region
/// operations included), so a tree techxt parsed itself can never reach this: it is the
/// backstop for a tree handed to [`Converter::tree_to_text`] from somewhere else.
/// Reaching it is the one thing that abandons a conversion, and it is reported as
/// `techxt.render-aborted`.
const RENDER_DESCENT_LIMIT: usize = 64;

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
}

impl Converter {
    /// Start building a converter.
    pub fn builder() -> ConverterBuilder {
        ConverterBuilder::new()
    }

    /// A converter with the shipped definitions and [`Options::default`].
    ///
    /// **M3/M4 seam:** "the shipped definitions" is currently a hand-built minimal set
    /// (see [`ConverterBuilder::build`]), not the definitions library PLAN.md §12
    /// describes.
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
    /// nesting past techxt's parse descent limit, or a definition whose
    /// parsing hook failed. Ordinary malformedness is *not* an error: under the default
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
            .with_descent_guard_init(StdDescentGuardInit::depth_limit(RENDER_DESCENT_LIMIT))
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
/// eating the other's entries.
fn merge(
    first: &Diagnostics<Option<String>>,
    second: &Diagnostics<Option<String>>,
) -> Diagnostics<Option<String>> {
    let limit = core::cmp::max(
        Diagnostics::<Option<String>>::DEFAULT_LIMIT,
        first.len() + second.len(),
    );
    let mut merged = Diagnostics::with_limit(limit);
    for diagnostic in first.iter().chain(second.iter()) {
        merged.push(diagnostic.clone());
    }
    merged
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
    overrides: RuleTable,
    recovery: Recovery,
    source_resolver: Option<Arc<dyn techy::source::SourceResolver<Option<String>>>>,
}

impl core::fmt::Debug for ConverterBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConverterBuilder")
            .field("options", &self.options)
            .field("recovery", &self.recovery)
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
            overrides: RuleTable::new(),
            recovery: Recovery::Tolerant,
            source_resolver: None,
        }
    }

    /// Replace the whole options struct.
    pub fn options(mut self, options: Options) -> ConverterBuilder {
        self.options = options;
        self
    }

    /// Render a macro with `rule` instead of whatever its definition says (PLAN.md
    /// §10.3 step 1). The name carries no escape character.
    pub fn override_macro(mut self, name: impl Into<Box<str>>, rule: TextRule) -> ConverterBuilder {
        self.overrides.insert(CallableKind::Macro, name, rule);
        self
    }

    /// Render an environment with `rule` instead of whatever its definition says.
    pub fn override_environment(
        mut self,
        name: impl Into<Box<str>>,
        rule: TextRule,
    ) -> ConverterBuilder {
        self.overrides.insert(CallableKind::Environment, name, rule);
        self
    }

    /// Render a specials construct with `rule` instead of whatever its definition says.
    /// The name is the trigger characters.
    pub fn override_specials(
        mut self,
        name: impl Into<Box<str>>,
        rule: TextRule,
    ) -> ConverterBuilder {
        self.overrides.insert(CallableKind::Specials, name, rule);
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
    pub fn recovery(mut self, recovery: Recovery) -> ConverterBuilder {
        self.recovery = recovery;
        self
    }

    /// Build the converter, validating the definitions.
    ///
    /// **M3/M4 seam:** the definitions are a hand-built minimal set — `\emph`,
    /// `\textbf`, `\ldots`, `\label`, `\par`, `\verb`, `~`, `center` and `verbatim`,
    /// plus a few entries that deliberately have no rule so that the unknown-construct
    /// policies have something to act on. It exists to exercise every dispatch path and
    /// every rule kind; PLAN.md §12's definitions library replaces it, and
    /// `ConverterBuilder::definitions(DefinitionSet)` arrives with it.
    pub fn build(self) -> Result<Converter, BuildError> {
        let (package, fallback) = minimal::definitions()?;

        let mut driver = LatexlikeDriver::new(self.recovery);
        if let Some(resolver) = self.source_resolver {
            driver = driver.with_source_resolver(resolver);
        }
        let state = ParsingState::lang_initial_with_packages([package])?;
        let language = Language::new(driver, state)
            .with_descent_guard_init(StdDescentGuardInit::depth_limit(PARSE_DESCENT_LIMIT));

        Ok(Converter {
            inner: Arc::new(Inner {
                language,
                config: RenderConfig {
                    options: self.options,
                    overrides: self.overrides,
                    fallback,
                },
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
    /// Leave the formula as LaTeX, re-emitted from the parsed tree.
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

/// What to do with a macro no rule renders (PLAN.md §10.6).
///
/// Whichever it is, a `techxt.unknown-macro` diagnostic is raised.
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

/// The hand-built definition set that stands in for PLAN.md §12's library.
///
/// **M3/M4 seam.** This exists so that the renderer core can be built and tested
/// against something real: every entry here is chosen to exercise a dispatch path or a
/// rule kind, not to be a useful LaTeX vocabulary. When `techxt::defs` lands, this
/// module goes away and `ConverterBuilder::build` consumes a `DefinitionSet` instead.
mod minimal {
    use alloc::borrow::Cow;
    use alloc::sync::Arc;
    use alloc::vec::Vec;

    use techy::core::node::NodeRef;
    use techy::core::specs::{ArgumentSpec, Package};
    use techy::latexlike::{
        argument_specs_named, CallableType, EnvironmentSpec, Latexlike, MacroSpec, SpecialsSpec,
    };

    use crate::def::{
        CallableKind, EnvBodyKind, RuleTable, TechxtEnvironmentBehavior, TechxtMacroSpec,
        TechxtSpecialsSpec, Template, TextHandler, TextRule,
    };
    use crate::flow::{Flow, FlowItem};
    use crate::render::{RenderCx, RenderError};

    use super::BuildError;

    /// `\par`: a paragraph break, and nothing else.
    #[derive(Debug)]
    struct ParagraphBreak;

    impl TextHandler for ParagraphBreak {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            _cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            let mut flow = Flow::new();
            flow.push(FlowItem::ParagraphBreak);
            Ok(flow)
        }
    }

    /// `~`: a space that no line break may fall in.
    ///
    /// Emitted as text rather than as glue, which is precisely what makes it
    /// unbreakable: adjacent text items are one word to the layout engine.
    #[derive(Debug)]
    struct NoBreakSpace;

    impl TextHandler for NoBreakSpace {
        fn render(
            &self,
            _node: NodeRef<'_, Latexlike>,
            _cx: &mut RenderCx<'_, '_>,
        ) -> Result<Flow, RenderError> {
            Ok(Flow::text("\u{a0}"))
        }
    }

    /// One mandatory `{…}` argument under this name.
    fn one(name: &str) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, BuildError> {
        Ok(argument_specs_named([("m", name)])?)
    }

    /// The package the parser uses, and the name-keyed rules the renderer falls back
    /// on.
    pub(super) fn definitions() -> Result<(Package<Latexlike>, RuleTable), BuildError> {
        let mut package = Package::<Latexlike>::new("techxt-minimal");
        let mut fallback = RuleTable::new();

        // --- macros whose rule rides in the spec (dispatch step 2) ---------------

        let mut macro_entry =
            |name: &str, arguments: Vec<Arc<ArgumentSpec<Latexlike>>>, rule: TextRule| {
                fallback.insert(CallableKind::Macro, name, rule.clone());
                package.insert(
                    CallableType::Macro,
                    name,
                    TechxtMacroSpec::new(arguments, rule),
                );
            };

        // `TextRule::Content`: the argument's content, nothing else.
        macro_entry("emph", one("text")?, TextRule::Content);
        // `TextRule::Template`: the same thing said with a template, so that the
        // template path is exercised too. M3's parser turns "{text}" into this.
        macro_entry(
            "textbf",
            one("text")?,
            TextRule::Template(Template::named_argument("text")),
        );
        // The same template written positionally, so that the index form of an
        // argument reference is exercised as well.
        macro_entry(
            "textit",
            one("text")?,
            TextRule::Template(Template::positional_argument(1)),
        );
        // A template that is more than one reference, and the `BracedOnly` argument
        // code: a URL must never swallow a following expression the way `m` would.
        macro_entry(
            "href",
            argument_specs_named([("BracedOnly", "url"), ("m", "text")])?,
            TextRule::Template(Template::link()),
        );
        // `TextRule::Literal`.
        macro_entry(
            "ldots",
            Vec::new(),
            TextRule::Literal(Cow::Borrowed("\u{2026}")),
        );
        // `TextRule::Skip`: a label contributes nothing to the text.
        macro_entry("label", one("key")?, TextRule::Skip);
        // `TextRule::Handler`.
        macro_entry(
            "par",
            Vec::new(),
            TextRule::Handler(Arc::new(ParagraphBreak)),
        );
        // A verbatim argument: its characters must survive untouched.
        macro_entry(
            "verb",
            argument_specs_named([("v", "text")])?,
            TextRule::Content,
        );

        // --- specials ------------------------------------------------------------

        fallback.insert(
            CallableKind::Specials,
            "~",
            TextRule::Handler(Arc::new(NoBreakSpace)),
        );
        package.insert_specials(
            CallableType::Specials,
            "~",
            TechxtSpecialsSpec::new(Vec::new(), TextRule::Handler(Arc::new(NoBreakSpace))),
        );

        // --- environments --------------------------------------------------------

        let mut environment_entry = |name: &str, body: EnvBodyKind, rule: TextRule| {
            fallback.insert(CallableKind::Environment, name, rule.clone());
            package.insert(
                CallableType::Environment,
                name,
                TechxtEnvironmentBehavior::new(Vec::new(), body, rule).into_spec(),
            );
        };

        // A `{body}` template: the same result as `TextRule::Content` for an
        // environment, chosen here so that the body segment is exercised.
        environment_entry(
            "center",
            EnvBodyKind::Normal,
            TextRule::Template(Template::body()),
        );
        environment_entry("verbatim", EnvBodyKind::Verbatim, TextRule::Content);

        // --- entries that exist to exercise the dispatch chain's later steps ------

        // Registered with a *plain* techy spec, so the node carries no embedded rule
        // and dispatch has to reach step 3, the name fallback table. This is the shape
        // a tree parsed with someone else's definitions has.
        package.insert(CallableType::Macro, "TeX", MacroSpec::new(Vec::new()));
        fallback.insert(
            CallableKind::Macro,
            "TeX",
            TextRule::Literal(Cow::Borrowed("TeX")),
        );

        // Parses, but has no rule anywhere: dispatch falls through to the
        // unknown-macro policy (PLAN.md §10.6).
        package.insert(CallableType::Macro, "phantom", MacroSpec::new(one("text")?));
        // Likewise for specials, so that the unknown-specials policy has a construct.
        package.insert_specials(CallableType::Specials, "&", SpecialsSpec::new(Vec::new()));
        // ... and for an environment, so that the unknown-environment policy does too.
        package.insert(
            CallableType::Environment,
            "unknownenv",
            EnvironmentSpec::<Latexlike>::new(Vec::new()),
        );

        Ok((package, fallback))
    }
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

    /// A converter whose name fallback table has been tampered with, so that dispatch
    /// step 2 (the rule embedded in the node's spec) and step 3 (the table) can be told
    /// apart. Nothing in the public builder can do this, because in a well-formed
    /// definition set the two always agree.
    fn converter_with_altered_fallback(alter: impl FnOnce(&mut RuleTable)) -> Converter {
        let (package, mut fallback) = minimal::definitions().expect("the minimal set builds");
        alter(&mut fallback);
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([package]).expect("finalizes"),
        )
        .with_descent_guard_init(StdDescentGuardInit::depth_limit(PARSE_DESCENT_LIMIT));
        Converter {
            inner: Arc::new(Inner {
                language,
                config: RenderConfig {
                    options: Options::default(),
                    overrides: RuleTable::new(),
                    fallback,
                },
            }),
        }
    }

    #[test]
    fn the_embedded_rule_beats_the_name_fallback_table() {
        let converter = converter_with_altered_fallback(|fallback| {
            fallback.insert(
                CallableKind::Macro,
                "emph",
                TextRule::Literal(alloc::borrow::Cow::Borrowed("FROM-THE-TABLE")),
            );
        });
        // `\emph` carries a techxt spec, so its embedded `Content` rule wins and the
        // argument is rendered.
        assert_eq!(
            converter.latex_to_text(r"\emph{x}").expect("parses").text,
            "x\n"
        );
        // `\TeX` carries a plain techy spec, so the table is the only source of a rule.
        assert_eq!(
            converter.latex_to_text(r"\TeX").expect("parses").text,
            "TeX\n"
        );
    }

    #[test]
    fn without_any_rule_the_unknown_policy_decides() {
        let converter = converter_with_altered_fallback(|_| {});
        // Removing `\TeX` from the table is not possible through the builder, so use
        // the entry that never had one: the policy renders nothing and warns.
        let conversion = converter.latex_to_text(r"a\phantom{x}b").expect("parses");
        assert_eq!(conversion.text, "ab\n");
        assert_eq!(
            conversion
                .diagnostics
                .with_identifier("techxt.unknown-macro")
                .count(),
            1
        );
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
