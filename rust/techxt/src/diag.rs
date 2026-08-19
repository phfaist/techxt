//! The diagnostic conditions techxt raises (PLAN.md §10.6).
//!
//! techxt's third design rule is *nothing silent*: a construct it does not know, an
//! effect it cannot reproduce, a handler that failed — each produces a structured
//! diagnostic with a source position, alongside the converted text. Conversion always
//! continues; a diagnostic is a report, not a control-flow device. The one exception is
//! techy's descent limit, which aborts the fold and is itself reported as
//! [`RenderAborted`].
//!
//! # Reading diagnostics
//!
//! A conversion hands back a [`techy::error::Diagnostics`] collection. Match conditions
//! **by type**, never by comparing identifier strings:
//!
//! ```
//! use techxt::diag::UnknownEnvironment;
//! use techxt::Converter;
//!
//! let conversion = Converter::standard()
//!     .latex_to_text(r"\begin{myenv}inner\end{myenv}")
//!     .expect("tolerant parse");
//! assert_eq!(conversion.text, "inner\n");
//!
//! let unknown: Vec<&UnknownEnvironment> =
//!     conversion.diagnostics.conditions::<UnknownEnvironment>().collect();
//! assert_eq!(unknown.len(), 1);
//! assert_eq!(unknown[0].name, "myenv");
//! ```
//!
//! Each condition also carries the identifier from the table below as
//! `IDENTIFIER`, for machine-readable output and for filtering with
//! [`Diagnostics::with_identifier`](techy::error::Diagnostics::with_identifier).
//!
//! # Severity
//!
//! techy chooses a diagnostic's severity at the raise site, so the severity of a
//! condition is a convention rather than a property of its type. techxt fixes that
//! convention in one place, [`TechxtCondition::SEVERITY`], and raises every diagnostic
//! through [`TechxtCondition::diagnose`] so that the same condition never appears at two
//! different severities.
//!
//! | condition | identifier | severity |
//! |---|---|---|
//! | [`UnknownMacro`] | `techxt.unknown-macro` | warning |
//! | [`UnknownEnvironment`] | `techxt.unknown-environment` | warning |
//! | [`UnknownSpecials`] | `techxt.unknown-specials` | warning |
//! | [`HandlerFailed`] | `techxt.handler-failed` | error |
//! | [`UnsupportedIgnored`] | `techxt.unsupported-ignored` | warning |
//! | [`MisplacedAlignment`] | `techxt.misplaced-alignment` | warning |
//! | [`StrayItem`] | `techxt.stray-item` | warning |
//! | [`InputNotResolved`] | `techxt.input-not-resolved` | note |
//! | [`RenderAborted`] | `techxt.render-aborted` | error |
//!
//! Diagnostics raised by the *parser* keep techy's own identifiers
//! (`core.specs.unresolvable-command`, `latexlike.environments.unknown-environment`, …);
//! when techxt drives the parse itself they are merged into the returned collection
//! ahead of the render diagnostics.

use alloc::string::String;

use techy::error::{Diagnostic, DiagnosticInfo, Severity};
use techy::source::SourceSpan;

/// A techxt diagnostic condition, with the severity techxt always raises it at.
///
/// PLAN.md §10.6 fixes one severity per condition. techy takes the severity at the
/// raise site instead, so this trait is where the table is enforced: raise every
/// techxt diagnostic through [`diagnose`](Self::diagnose) and the two can never drift
/// apart.
///
/// ```
/// use techxt::diag::{StrayItem, TechxtCondition};
/// use techy::error::Severity;
///
/// assert_eq!(StrayItem::SEVERITY, Severity::Warning);
/// ```
pub trait TechxtCondition: DiagnosticInfo + Sized {
    /// The severity techxt raises this condition at.
    const SEVERITY: Severity;

    /// Raise this condition as a diagnostic positioned at `span`.
    ///
    /// The span is the offending node's span. Spans are provenance only — techxt never
    /// reads document content through them (PLAN.md §1.6).
    fn diagnose(self, span: SourceSpan<Option<String>>) -> Diagnostic<Option<String>> {
        Diagnostic::new(Self::SEVERITY, self, span)
    }
}

/// A macro was invoked that no rule renders.
///
/// Note that this is *not* the usual fate of a macro the document database has never
/// heard of: the parser cannot even shape such an invocation, so a tolerant parse turns
/// `\foo` into literal characters and reports its own
/// `core.specs.unresolvable-command`. This condition is about the other half of the
/// problem — a macro that *parses* (its arguments are declared somewhere) but has no
/// text rule, which is what happens on a tree parsed with a foreign definition set or
/// with definitions added for their parsing behaviour alone.
///
/// How the invocation is rendered is chosen by
/// [`Options::unknown_macro`](crate::Options::unknown_macro); the diagnostic is raised
/// whatever the policy says.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.unknown-macro",
    message = "no text rule for the macro ‘\\{name}’"
)]
pub struct UnknownMacro {
    /// The macro's name, without the escape character (`emph`).
    pub name: String,
}

impl TechxtCondition for UnknownMacro {
    const SEVERITY: Severity = Severity::Warning;
}

/// An environment was used that no rule renders.
///
/// How its body is treated is chosen by
/// [`Options::unknown_env`](crate::Options::unknown_env) — by default the body is still
/// rendered, since an unknown wrapper rarely means the text inside it is unwanted. The
/// diagnostic is raised whatever the policy says.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.unknown-environment",
    message = "no text rule for the environment ‘{name}’"
)]
pub struct UnknownEnvironment {
    /// The environment's name, as written in `\begin{…}` (`itemize`).
    pub name: String,
}

impl TechxtCondition for UnknownEnvironment {
    const SEVERITY: Severity = Severity::Warning;
}

/// A specials construct was parsed that no rule renders.
///
/// Specials are keyed by the characters that trigger them (`~`, `---`, `&`), so this
/// says that those characters were recognized as a construct by the parsing database
/// but have no text rule. How they are rendered is chosen by
/// [`Options::unknown_specials`](crate::Options::unknown_specials).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.unknown-specials",
    message = "no text rule for the specials ‘{chars}’"
)]
pub struct UnknownSpecials {
    /// The trigger characters, as written.
    pub chars: String,
}

impl TechxtCondition for UnknownSpecials {
    const SEVERITY: Severity = Severity::Warning;
}

/// A [`TextHandler`](crate::def::TextHandler) returned an error.
///
/// The construct renders as nothing and the conversion continues: one broken handler
/// must not cost the reader the rest of the document. This is an error rather than a
/// warning because, unlike an unknown construct, it means techxt's own definitions and
/// its own code disagreed.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.handler-failed",
    message = "‘{construct}’ could not be rendered: {detail}"
)]
pub struct HandlerFailed {
    /// The construct as written (`\href`, `tabular`).
    pub construct: String,
    /// What went wrong, in human-readable form.
    pub detail: String,
}

impl TechxtCondition for HandlerFailed {
    const SEVERITY: Severity = Severity::Error;
}

/// A construct was understood, but part of what it asks for cannot be expressed in
/// plain text and was dropped.
///
/// This is the honest report behind techxt's deliberate omissions: a `\multicolumn`
/// span, a list package's key-value options, a length argument. The construct still
/// renders; the diagnostic names the part that did not survive.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.unsupported-ignored",
    message = "‘{construct}’: {what} is not supported and was ignored"
)]
pub struct UnsupportedIgnored {
    /// The construct as written (`\multicolumn`, `tabular`).
    pub construct: String,
    /// What exactly was dropped (`the column span`).
    pub what: String,
}

impl TechxtCondition for UnsupportedIgnored {
    const SEVERITY: Severity = Severity::Warning;
}

/// An alignment character (`&`) appeared outside any table or matrix.
///
/// Alignment is meaningful only inside a table or matrix body, where the enclosing
/// handler consumes it to split cells. Anywhere else there is no column structure to
/// join, so the character is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.misplaced-alignment",
    message = "‘&’ outside a table or matrix"
)]
pub struct MisplacedAlignment;

impl TechxtCondition for MisplacedAlignment {
    const SEVERITY: Severity = Severity::Warning;
}

/// An `\item` appeared outside any list environment.
///
/// The item is still rendered — with a generic `- ` marker — because dropping it would
/// lose the text that follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "techxt.stray-item", message = "‘\\item’ outside a list")]
pub struct StrayItem;

impl TechxtCondition for StrayItem {
    const SEVERITY: Severity = Severity::Warning;
}

/// An `\input`-like macro had no resolved content attached to it.
///
/// Source resolution happens at *parse* time and is opt-in twice over: the parser needs
/// a source resolver, and the resolver has to answer. When either half is missing the
/// invocation is staged with no attached content, and the renderer has nothing to
/// render. This is a note rather than a warning because running without a resolver is a
/// perfectly ordinary configuration — the parser has already reported the failing
/// lookups it did attempt.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.input-not-resolved",
    message = "‘{target}’ was not resolved and could not be included"
)]
pub struct InputNotResolved {
    /// The reference as written in the document (`chapters/intro.tex`).
    pub target: String,
}

impl TechxtCondition for InputNotResolved {
    const SEVERITY: Severity = Severity::Note;
}

/// The renderer gave up on the document.
///
/// This is the one non-continuable condition, and it has exactly one cause: techy's
/// descent guard refused to go deeper, because the tree nests further than the
/// configured limit. The conversion returns empty text plus this diagnostic. The same
/// condition is also raised — before anything is lost — when the guard *warns* that the
/// run is approaching its limit, so that a document about to fail says so.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "techxt.render-aborted",
    message = "rendering was aborted: {detail}"
)]
pub struct RenderAborted {
    /// What the descent guard reported.
    pub detail: String,
}

impl TechxtCondition for RenderAborted {
    const SEVERITY: Severity = Severity::Error;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::sync::Arc;
    use techy::error::Diagnostics;
    use techy::source::Source;

    fn span() -> SourceSpan<Option<String>> {
        let source: Arc<Source<Option<String>>> = Arc::new(Source::new("abc"));
        SourceSpan::entire(&source)
    }

    #[test]
    fn identifiers_match_the_plan_table() {
        assert_eq!(UnknownMacro::IDENTIFIER, "techxt.unknown-macro");
        assert_eq!(UnknownEnvironment::IDENTIFIER, "techxt.unknown-environment");
        assert_eq!(UnknownSpecials::IDENTIFIER, "techxt.unknown-specials");
        assert_eq!(HandlerFailed::IDENTIFIER, "techxt.handler-failed");
        assert_eq!(UnsupportedIgnored::IDENTIFIER, "techxt.unsupported-ignored");
        assert_eq!(MisplacedAlignment::IDENTIFIER, "techxt.misplaced-alignment");
        assert_eq!(StrayItem::IDENTIFIER, "techxt.stray-item");
        assert_eq!(InputNotResolved::IDENTIFIER, "techxt.input-not-resolved");
        assert_eq!(RenderAborted::IDENTIFIER, "techxt.render-aborted");
    }

    #[test]
    fn severities_match_the_plan_table() {
        assert_eq!(UnknownMacro::SEVERITY, Severity::Warning);
        assert_eq!(UnknownEnvironment::SEVERITY, Severity::Warning);
        assert_eq!(UnknownSpecials::SEVERITY, Severity::Warning);
        assert_eq!(HandlerFailed::SEVERITY, Severity::Error);
        assert_eq!(UnsupportedIgnored::SEVERITY, Severity::Warning);
        assert_eq!(MisplacedAlignment::SEVERITY, Severity::Warning);
        assert_eq!(StrayItem::SEVERITY, Severity::Warning);
        assert_eq!(InputNotResolved::SEVERITY, Severity::Note);
        assert_eq!(RenderAborted::SEVERITY, Severity::Error);
    }

    #[test]
    fn diagnose_carries_severity_message_and_payload() {
        let mut diagnostics: Diagnostics<Option<String>> = Diagnostics::new();
        diagnostics.push(UnknownMacro::new("emph").diagnose(span()));
        diagnostics.push(MisplacedAlignment.diagnose(span()));

        let raised = diagnostics.as_slice();
        assert_eq!(raised[0].severity(), Severity::Warning);
        assert_eq!(raised[0].identifier(), "techxt.unknown-macro");
        assert!(raised[0].message().contains("\\emph"));
        assert_eq!(raised[1].identifier(), "techxt.misplaced-alignment");

        let found: Vec<&UnknownMacro> = diagnostics.conditions::<UnknownMacro>().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "emph");
    }

    #[test]
    fn conditions_are_displayable() {
        assert_eq!(
            UnsupportedIgnored::new("\\multicolumn", "the column span").to_string(),
            "‘\\multicolumn’: the column span is not supported and was ignored"
        );
        assert_eq!(StrayItem.to_string(), "‘\\item’ outside a list");
    }
}
