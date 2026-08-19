//! The entry builders (PLAN.md §10.1): one definition, both sides.
//!
//! The public prose lives on [`MacroDef`], because this module is private and its own
//! documentation is never rendered.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use techy::core::specs::ArgumentSpec;
use techy::latexlike::{argument_specs_named, ArgumentCodeError, Latexlike, Mode};

use crate::render::ListKind;

use super::spec::{CallableSpecSource, EnvBodyKind};
use super::TextRule;

/// One declared argument, in declaration order.
#[derive(Clone, Debug)]
enum ArgDecl {
    /// An argument written as a techy argument code plus a name.
    Code {
        /// The techy argument code.
        code: Box<str>,
        /// What the definition calls this argument.
        name: Box<str>,
    },
    /// An argument built as a techy [`ArgumentSpec`] directly.
    Spec(Arc<ArgumentSpec<Latexlike>>),
}

/// The argument declarations of one entry, shared by the three builders.
#[derive(Clone, Debug, Default)]
struct Args(Vec<ArgDecl>);

impl Args {
    fn code(&mut self, code: &str, name: &str) {
        self.0.push(ArgDecl::Code {
            code: code.into(),
            name: name.into(),
        });
    }

    fn spec(&mut self, spec: ArgumentSpec<Latexlike>) {
        self.0.push(ArgDecl::Spec(Arc::new(spec)));
    }

    /// The techy argument specs, in declaration order.
    ///
    /// Codes are resolved through techy's own factory, one call for the whole entry so
    /// that an invalid code reports the position it was written at.
    fn build(&self) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
        let codes: Vec<(&str, &str)> = self
            .0
            .iter()
            .filter_map(|declaration| match declaration {
                ArgDecl::Code { code, name } => Some((&**code, &**name)),
                ArgDecl::Spec(_) => None,
            })
            .collect();
        let mut built = argument_specs_named(codes)?.into_iter();
        let mut specs = Vec::with_capacity(self.0.len());
        for declaration in &self.0 {
            match declaration {
                // `argument_specs_named` answers with exactly one spec per code, in
                // order; the iterator therefore cannot run dry here.
                ArgDecl::Code { .. } => specs.push(
                    built
                        .next()
                        .expect("techy returns one argument spec per argument code"),
                ),
                ArgDecl::Spec(spec) => specs.push(Arc::clone(spec)),
            }
        }
        Ok(specs)
    }

    /// The argument names, in declaration order, as templates refer to them.
    ///
    /// An argument built from an unnamed [`ArgumentSpec`] contributes an empty name: it
    /// still counts for index references, and no template can name it (an empty name is
    /// never valid).
    fn names(&self) -> Vec<Box<str>> {
        self.0
            .iter()
            .map(|declaration| match declaration {
                ArgDecl::Code { name, .. } => name.clone(),
                ArgDecl::Spec(spec) => spec.name.clone().unwrap_or_else(|| Box::from("")),
            })
            .collect()
    }
}

/// Which modes a definition is visible in.
///
/// `None` means every mode, which is what almost every definition wants; the
/// mode-restricted ones are the reason `^` does not fire in text and `---` does not
/// fire in math.
type Modes = Option<Vec<Mode>>;

/// A macro definition (PLAN.md §10.1).
///
/// One entry describes a construct completely — how it parses *and* how it renders —
/// and reads as one line of a table:
///
/// ```
/// use techxt::def::{Category, EnvDef, MacroDef, Template, TextRule};
///
/// let category = Category::new("links")
///     .with_macro(
///         MacroDef::new("href")
///             // A URL must never swallow a following expression, so it is a group and
///             // nothing else — that is what `BracedOnly` says and `m` does not.
///             .arg("BracedOnly", "url")
///             .arg("m", "text")
///             .rule(TextRule::Template(Template::new("{text} <{url}>"))),
///     )
///     .with_macro(MacroDef::symbol("ldots", "…"))
///     .with_env(EnvDef::new("quote").rule(TextRule::Content));
/// ```
///
/// # Every argument is named
///
/// [`arg`](Self::arg) takes a name as well as a code, and the name is not optional.
/// Names are what templates and handlers refer to, and a definition whose second
/// argument is called `text` keeps working when a third argument is added in front of
/// it — which is exactly the class of bug that positional argument lists in
/// pylatexenc's two independent databases produce.
///
/// # Argument codes
///
/// The codes are techy's — `m` (a mandatory `{…}` group, with TeX's single-expression
/// fallback), `o` (optional `[…]`), `s` (an optional `*`), `t<c>` (an optional marker
/// character), `r<c1><c2>` and `d<c1><c2>` (a group with given delimiters, required and
/// optional), `v` (a `\verb`-style verbatim run), `e{…}` (embellishments) — plus the
/// three word codes `AnyDelimited`, `AnyDelimitedOptional` and `BracedOnly` (a
/// mandatory group with the expression fallback *off*). Each call declares exactly one
/// argument, so the word codes — which techy accepts only in its list form, never
/// inside a compact argument string — work here just like the letters do.
///
/// An argument that needs more than a code can say (a state delta of its own, an
/// adjacency requirement) is declared with [`arg_spec`](Self::arg_spec), which takes a
/// techy [`ArgumentSpec`] built by hand.
#[derive(Clone, Debug)]
pub struct MacroDef {
    name: Box<str>,
    args: Args,
    rule: Option<TextRule>,
    modes: Modes,
    spec: Option<Arc<dyn CallableSpecSource>>,
}

impl MacroDef {
    /// A macro called `name`, written **without** its escape character (`"emph"`, not
    /// `"\\emph"`), taking no arguments and having no rule yet.
    pub fn new(name: impl Into<Box<str>>) -> MacroDef {
        MacroDef {
            name: name.into(),
            args: Args::default(),
            rule: None,
            modes: None,
            spec: None,
        }
    }

    /// An argument-less macro that renders as fixed text: `\alpha` → `α`.
    ///
    /// The overwhelming majority of a definitions library is this one line.
    pub fn symbol(name: impl Into<Box<str>>, replacement: impl Into<Box<str>>) -> MacroDef {
        let replacement: Box<str> = replacement.into();
        MacroDef::new(name).rule(TextRule::Literal(Cow::Owned(String::from(replacement))))
    }

    /// Declare the next argument: a techy argument code and the name rules refer to it
    /// by.
    pub fn arg(mut self, code: &str, name: &str) -> MacroDef {
        self.args.code(code, name);
        self
    }

    /// Declare the next argument as a techy [`ArgumentSpec`] built by hand.
    ///
    /// For the arguments a code cannot describe: one carrying a parsing-state delta of
    /// its own (`\text{…}`, which leaves math for the extent of its argument), or one
    /// whose parser is configured (`\\`'s optional `[len]`, which must refuse a
    /// preceding space).
    pub fn arg_spec(mut self, spec: ArgumentSpec<Latexlike>) -> MacroDef {
        self.args.spec(spec);
        self
    }

    /// Register a techy [`CallableSpec`](techy::core::specs::CallableSpec) of `source`'s
    /// making instead of the one techxt builds from this entry's arguments
    /// (DECISIONS.md D15).
    ///
    /// This is [`arg_spec`](Self::arg_spec)'s hatch one level up: it hands the *whole*
    /// spec to a construct techy itself implements, which is what `\input` needs —
    /// techy resolves an inclusion at parse time only for a macro registered with its
    /// own input spec, and no combination of argument codes can ask for that.
    ///
    /// The source is consulted once, when the converter is built, and may decline (see
    /// [`CallableSpecSource`]), in which case this entry registers exactly what it would
    /// have without the hatch. Either way the entry's [`rule`](Self::rule) still applies:
    /// a foreign spec cannot carry a rule into the tree, so the rule is found by name at
    /// dispatch step 3 instead of by downcast at step 2 (PLAN.md §10.3). The arguments
    /// declared with [`arg`](Self::arg) still name what the foreign spec parses, for the
    /// templates and handlers that read them — a foreign spec whose argument names
    /// differ is why a handler should tolerate both spellings.
    pub fn spec(mut self, source: impl CallableSpecSource + 'static) -> MacroDef {
        self.spec = Some(Arc::new(source));
        self
    }

    /// Declare an optional `*` argument named `star` — the same as
    /// `.arg("s", "star")`.
    pub fn star(self) -> MacroDef {
        self.arg("s", "star")
    }

    /// Render this macro through `rule`.
    ///
    /// A macro with no rule parses but does not render: it falls through to the name
    /// fallback table and then to
    /// [`Options::unknown_macro`](crate::Options::unknown_macro).
    pub fn rule(mut self, rule: TextRule) -> MacroDef {
        self.rule = Some(rule);
        self
    }

    /// Make this macro visible in text mode only.
    pub fn text_mode_only(mut self) -> MacroDef {
        self.modes = Some(alloc::vec![Mode::Text]);
        self
    }

    /// Make this macro visible in math mode only.
    pub fn math_mode_only(mut self) -> MacroDef {
        self.modes = Some(alloc::vec![Mode::Math]);
        self
    }

    /// The macro's name, without its escape character.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The techy argument specs, in declaration order.
    pub(crate) fn arg_specs(&self) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
        self.args.build()
    }

    /// The argument names, in declaration order.
    pub(crate) fn arg_names(&self) -> Vec<Box<str>> {
        self.args.names()
    }

    /// The rule this macro renders through, if it has one.
    pub(crate) fn text_rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }

    /// The modes this macro is visible in, or `None` for all of them.
    pub(crate) fn modes(&self) -> Option<Vec<Mode>> {
        self.modes.clone()
    }

    /// What supplies this macro's techy spec, when it is not techxt that does.
    pub(crate) fn spec_source(&self) -> Option<&Arc<dyn CallableSpecSource>> {
        self.spec.as_ref()
    }
}

/// An environment definition (PLAN.md §10.1).
///
/// Beyond arguments and a rule, an environment declares what its *body* is, which is
/// what keeps the renderer from having to guess and what gives the parser the state it
/// needs: [`math_body`](Self::math_body) parses the body as math,
/// [`verbatim_body`](Self::verbatim_body) parses it as raw characters, and
/// [`list_body`](Self::list_body) brings `\item` into scope for it.
#[derive(Clone, Debug)]
pub struct EnvDef {
    name: Box<str>,
    args: Args,
    body: EnvBodyKind,
    rule: Option<TextRule>,
}

impl EnvDef {
    /// An environment called `name`, as it is written in `\begin{…}`, with an ordinary
    /// body, no arguments and no rule yet.
    pub fn new(name: impl Into<Box<str>>) -> EnvDef {
        EnvDef {
            name: name.into(),
            args: Args::default(),
            body: EnvBodyKind::Normal,
            rule: None,
        }
    }

    /// Declare the next argument of `\begin{name}` — see [`MacroDef::arg`].
    pub fn arg(mut self, code: &str, name: &str) -> EnvDef {
        self.args.code(code, name);
        self
    }

    /// Declare the next argument as a techy [`ArgumentSpec`] — see
    /// [`MacroDef::arg_spec`].
    pub fn arg_spec(mut self, spec: ArgumentSpec<Latexlike>) -> EnvDef {
        self.args.spec(spec);
        self
    }

    /// Render this environment through `rule`.
    pub fn rule(mut self, rule: TextRule) -> EnvDef {
        self.rule = Some(rule);
        self
    }

    /// Parse the body in math mode (`equation`, `align`).
    pub fn math_body(mut self) -> EnvDef {
        self.body = EnvBodyKind::Math;
        self
    }

    /// Parse the body as raw characters, with no markup at all (`verbatim`).
    pub fn verbatim_body(mut self) -> EnvDef {
        self.body = EnvBodyKind::Verbatim;
        self
    }

    /// Parse the body as a list of `kind`, with `\item` in scope for its extent
    /// (PLAN.md §9.4).
    ///
    /// The `\item` brought into scope is the one the definition set itself defines, so
    /// a library that customizes `\item` customizes it inside every list too.
    pub fn list_body(mut self, kind: ListKind) -> EnvDef {
        self.body = EnvBodyKind::List(kind);
        self
    }

    /// The environment's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The techy argument specs, in declaration order.
    pub(crate) fn arg_specs(&self) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
        self.args.build()
    }

    /// The argument names, in declaration order.
    pub(crate) fn arg_names(&self) -> Vec<Box<str>> {
        self.args.names()
    }

    /// What this environment's body is.
    pub(crate) fn body_kind(&self) -> EnvBodyKind {
        self.body
    }

    /// The rule this environment renders through, if it has one.
    pub(crate) fn text_rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }
}

/// A specials definition (PLAN.md §10.1): a construct triggered by the characters
/// themselves — `~`, `&`, `---`, `^`, `_`.
///
/// techy matches the **longest** registered trigger, and mode visibility is what keeps
/// `---` out of math and `^` out of text.
#[derive(Clone, Debug)]
pub struct SpecialsDef {
    chars: Box<str>,
    args: Args,
    rule: Option<TextRule>,
    modes: Modes,
}

impl SpecialsDef {
    /// A specials triggered by exactly these characters.
    pub fn new(chars: impl Into<Box<str>>) -> SpecialsDef {
        SpecialsDef {
            chars: chars.into(),
            args: Args::default(),
            rule: None,
            modes: None,
        }
    }

    /// Declare the next argument — see [`MacroDef::arg`]. `^` and `_` each take one.
    pub fn arg(mut self, code: &str, name: &str) -> SpecialsDef {
        self.args.code(code, name);
        self
    }

    /// Declare the next argument as a techy [`ArgumentSpec`] — see
    /// [`MacroDef::arg_spec`].
    pub fn arg_spec(mut self, spec: ArgumentSpec<Latexlike>) -> SpecialsDef {
        self.args.spec(spec);
        self
    }

    /// Render this specials through `rule`.
    pub fn rule(mut self, rule: TextRule) -> SpecialsDef {
        self.rule = Some(rule);
        self
    }

    /// Make this specials fire in text mode only — the ligatures (`---`, ` `` `).
    pub fn text_mode_only(mut self) -> SpecialsDef {
        self.modes = Some(alloc::vec![Mode::Text]);
        self
    }

    /// Make this specials fire in math mode only — `^` and `_`.
    pub fn math_mode_only(mut self) -> SpecialsDef {
        self.modes = Some(alloc::vec![Mode::Math]);
        self
    }

    /// The trigger characters.
    pub fn trigger(&self) -> &str {
        &self.chars
    }

    /// The techy argument specs, in declaration order.
    pub(crate) fn arg_specs(&self) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
        self.args.build()
    }

    /// The argument names, in declaration order.
    pub(crate) fn arg_names(&self) -> Vec<Box<str>> {
        self.args.names()
    }

    /// The rule this specials renders through, if it has one.
    pub(crate) fn text_rule(&self) -> Option<&TextRule> {
        self.rule.as_ref()
    }

    /// The modes this specials is visible in, or `None` for all of them.
    pub(crate) fn modes(&self) -> Option<Vec<Mode>> {
        self.modes.clone()
    }
}
