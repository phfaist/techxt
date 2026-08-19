//! The template mini-language (PLAN.md §10.5): its source syntax, its parser, and the
//! typed form the renderer executes.
//!
//! The public prose lives on [`Template`], because this module is private and its own
//! documentation is never rendered.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Replacement text with holes for a construct's arguments (PLAN.md §10.5).
///
/// A template is the data form of "this construct renders as that text, with these
/// arguments dropped into it": `\href{url}{text}` is `"{text} <{url}>"`. Templates are
/// what keeps the overwhelming majority of a definitions database a *table* rather than
/// code — a rule that only rearranges its arguments never needs a handler.
///
/// ```
/// use techxt::def::{Template, TextRule};
///
/// let rule = TextRule::Template(Template::new("{text} <{url}>"));
/// ```
///
/// # Grammar
///
/// ```text
/// template   := ( literal | escape | ref | cond )*
/// escape     := "{{" | "}}"                      # a literal brace
/// ref        := "{" name "}" | "{" integer "}"   # named argument | 1-based index
///                                                # "{body}" — environments only
/// cond       := "{?" name ":" branch ( "|" branch )? "}"
/// branch     := ( literal | escape | ref )*      # no nested conditionals, no "|" literal
/// ```
///
/// An argument that was not written renders as nothing, which is usually what an
/// optional argument should do; `{?name:then|else}` is there for when it is not. A
/// reference by index counts from **one**, matching how LaTeX documentation numbers a
/// macro's arguments. A `}` that closes nothing is an ordinary character, but `}}` is
/// the spelling to prefer.
///
/// # When errors surface
///
/// [`new`](Self::new) is infallible: a definitions table is data, and a syntax error in
/// one entry must not force every table in the library to be written as a fallible
/// expression. The error is *kept* and reported when the converter is built, together
/// with the validation that can only happen there — whether the names the template uses
/// are names the definition actually declares. Everything reaches the caller as
/// [`BuildError::Template`](crate::convert::BuildError::Template), which names the
/// definition and quotes the template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    /// The source text, kept so that a build error can quote what was written.
    source: Box<str>,
    /// The compiled form, or the syntax error that stopped it being compiled. Holding
    /// the error rather than returning it is what makes [`new`](Self::new) infallible;
    /// it is surfaced when the converter is built.
    parsed: Result<Vec<Seg>, TemplateError>,
}

impl Template {
    /// Compile a template from its source text.
    ///
    /// Never fails: a malformed template is reported when the converter that uses it is
    /// built (see [the type's documentation](Template#when-errors-surface)), or
    /// immediately through [`error`](Self::error).
    ///
    /// ```
    /// use techxt::def::Template;
    ///
    /// assert!(Template::new("{text} <{url}>").error().is_none());
    /// assert!(Template::new("{unterminated").error().is_some());
    /// ```
    pub fn new(source: impl Into<Box<str>>) -> Template {
        let source = source.into();
        let parsed = Parser::new(&source).template();
        Template { source, parsed }
    }

    /// The source text this template was written as.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The syntax error in this template's source, if it has one.
    ///
    /// Only the errors that can be seen without knowing the definition — a construct
    /// that is never closed, a conditional inside a conditional. Whether the names the
    /// template uses exist is decided when the converter is built.
    pub fn error(&self) -> Option<&TemplateError> {
        self.parsed.as_ref().err()
    }

    /// The compiled segments, in order; empty for a template that did not compile.
    ///
    /// Rendering an uncompiled template as nothing is deliberate: it cannot be reached
    /// through [`ConverterBuilder::build`](crate::ConverterBuilder::build), which
    /// refuses the whole converter instead, and a spec registered with techy by hand
    /// must still not be able to make the renderer panic.
    pub(crate) fn segments(&self) -> &[Seg] {
        match &self.parsed {
            Ok(segments) => segments,
            Err(_) => &[],
        }
    }

    /// Check this template against the definition it belongs to (PLAN.md §10.5).
    ///
    /// This is the half of the validation that needs the definition: which argument
    /// names exist, how many arguments there are, and whether there is a body to refer
    /// to. It also re-reports the syntax error of a template that never compiled, so
    /// that one call surfaces everything.
    pub(crate) fn validate(&self, scope: TemplateScope<'_>) -> Result<(), TemplateError> {
        let segments = match &self.parsed {
            Ok(segments) => segments,
            Err(error) => return Err(error.clone()),
        };
        validate_segments(segments, scope, true)
    }
}

/// What a template is allowed to refer to: the definition's side of validation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TemplateScope<'a> {
    /// The definition's argument names, in declaration order — or `None` when they are
    /// not known, which is the case for a
    /// [`ConverterBuilder::override_macro`](crate::ConverterBuilder::override_macro)
    /// rule: an override is written against whichever definition it lands on, so
    /// checking its references against one definition's names would be wrong. Such a
    /// reference is checked when it is rendered instead, and a miss becomes a
    /// `techxt.handler-failed` diagnostic.
    pub(crate) names: Option<&'a [Box<str>]>,
    /// Whether `{body}` means anything here — environments only.
    pub(crate) has_body: bool,
}

impl<'a> TemplateScope<'a> {
    /// A scope for a definition whose arguments are known.
    pub(crate) fn declared(names: &'a [Box<str>], has_body: bool) -> TemplateScope<'a> {
        TemplateScope {
            names: Some(names),
            has_body,
        }
    }

    /// A scope for a rule that is not attached to one definition: references are not
    /// checked, only the shape of the template is.
    pub(crate) fn unchecked(has_body: bool) -> TemplateScope<'a> {
        TemplateScope {
            names: None,
            has_body,
        }
    }
}

/// Validate one segment list; `top_level` is false inside a conditional's branches.
fn validate_segments(
    segments: &[Seg],
    scope: TemplateScope<'_>,
    top_level: bool,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Seg::Str(_) => {}
            Seg::Arg(reference) => validate_ref(reference, scope)?,
            Seg::Body => {
                if !scope.has_body {
                    return Err(TemplateError::BodyOutsideEnvironment);
                }
            }
            Seg::IfPresent { arg, then, els } => {
                // The parser already refuses a conditional inside a branch; the check
                // is repeated here so that the invariant holds for any segment list,
                // however it was built.
                if !top_level {
                    return Err(TemplateError::NestedConditional);
                }
                validate_ref(arg, scope)?;
                validate_segments(then, scope, false)?;
                validate_segments(els, scope, false)?;
            }
        }
    }
    Ok(())
}

/// Validate one argument reference against the definition's arguments.
fn validate_ref(reference: &ArgRef, scope: TemplateScope<'_>) -> Result<(), TemplateError> {
    match reference {
        ArgRef::Name(name) => {
            // An empty name is rejected whatever the scope: no definition can declare
            // an argument called "", so `{}` is a typo under any circumstances.
            if name.is_empty() {
                return Err(TemplateError::UnknownArgumentName { name: name.clone() });
            }
            match scope.names {
                Some(names) if !names.iter().any(|declared| declared == name) => {
                    Err(TemplateError::UnknownArgumentName { name: name.clone() })
                }
                _ => Ok(()),
            }
        }
        ArgRef::Index(index) => {
            let count = scope.names.map_or(usize::MAX, <[Box<str>]>::len);
            // Indices count from one, so 0 is out of range for every definition —
            // including one whose argument count is not known.
            if *index == 0 || *index > count {
                return Err(TemplateError::ArgumentIndexOutOfRange {
                    index: *index,
                    count: scope.names.map_or(0, <[Box<str>]>::len),
                });
            }
            Ok(())
        }
    }
}

/// One piece of a [`Template`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Seg {
    /// Literal text.
    Str(Box<str>),
    /// The content of one argument, empty when the argument is absent.
    Arg(ArgRef),
    /// The environment's body (rejected at build time for macros and specials).
    Body,
    /// `{?name:then|else}`: one branch or the other, depending on whether the argument
    /// was written. Branches hold no conditionals of their own.
    IfPresent {
        /// The argument whose presence decides.
        arg: ArgRef,
        /// Rendered when the argument was written.
        then: Vec<Seg>,
        /// Rendered when it was not; empty when the template gave no else branch.
        els: Vec<Seg>,
    },
}

/// How a [`Seg`] names an argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArgRef {
    /// By the name the definition gave it.
    Name(Box<str>),
    /// By 1-based position in the declaration order.
    Index(usize),
}

/// Why a template could not be compiled (PLAN.md §10.5).
///
/// Templates are compiled when they are written and validated when the converter is
/// built, so these surface as
/// [`BuildError::Template`](crate::convert::BuildError::Template) and never at
/// conversion time.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// `{foo}`, where the definition declares no argument named `foo`.
    UnknownArgumentName {
        /// The name the template used.
        name: Box<str>,
    },
    /// `{0}`, or `{3}` on a two-argument definition. Indices are 1-based.
    ArgumentIndexOutOfRange {
        /// The index the template used.
        index: usize,
        /// How many arguments the definition declares.
        count: usize,
    },
    /// `{body}` in a macro's or a specials' template: only environments have a body.
    BodyOutsideEnvironment,
    /// A conditional inside a conditional's branch, which the grammar does not allow.
    NestedConditional,
    /// A `{…}` reference or a `{?…}` conditional that never closes — or that closes
    /// somewhere the grammar does not allow, such as a conditional with a third branch.
    UnterminatedConstruct,
}

impl core::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TemplateError::UnknownArgumentName { name } => {
                write!(f, "no argument named ‘{name}’")
            }
            TemplateError::ArgumentIndexOutOfRange { index, count } => write!(
                f,
                "argument index {index} is out of range (1..={count}; indices are 1-based)"
            ),
            TemplateError::BodyOutsideEnvironment => {
                write!(
                    f,
                    "‘{{body}}’ is only available in an environment's template"
                )
            }
            TemplateError::NestedConditional => {
                write!(f, "a conditional may not contain another conditional")
            }
            TemplateError::UnterminatedConstruct => {
                write!(f, "unterminated ‘{{…}}’ construct")
            }
        }
    }
}

impl core::error::Error for TemplateError {}

// ------------------------------------------------------------------------ the parser

/// A recursive-descent parser over a template's source text.
///
/// It descends exactly one level — a conditional's branches, which may not nest — so
/// no input can make it recurse deeply, and it needs no descent guard of its own.
struct Parser<'a> {
    source: &'a str,
    /// Byte offset of the next character; always on a character boundary.
    pos: usize,
}

/// Why a segment list stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The source ran out.
    End,
    /// A `|` ended a conditional's first branch.
    Pipe,
    /// A `}` closed the conditional.
    Close,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Parser<'a> {
        Parser { source, pos: 0 }
    }

    /// The whole template: segments up to the end of the source.
    fn template(mut self) -> Result<Vec<Seg>, TemplateError> {
        let (segments, stop) = self.segments(false)?;
        match stop {
            Stop::End => Ok(segments),
            // Unreachable: only a branch stops on `|` or `}`.
            Stop::Pipe | Stop::Close => Err(TemplateError::UnterminatedConstruct),
        }
    }

    /// The character at the cursor.
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// The character just after the one at the cursor.
    fn peek_second(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next();
        chars.next()
    }

    /// Step over the character at the cursor.
    fn bump(&mut self) {
        if let Some(ch) = self.peek() {
            self.pos += ch.len_utf8();
        }
    }

    /// A run of segments. Inside a branch (`in_branch`), `|` and `}` end the run and a
    /// conditional is refused; at the top level they are ordinary characters.
    fn segments(&mut self, in_branch: bool) -> Result<(Vec<Seg>, Stop), TemplateError> {
        let mut segments = Vec::new();
        let mut literal = String::new();
        loop {
            let Some(ch) = self.peek() else {
                flush(&mut literal, &mut segments);
                return Ok((segments, Stop::End));
            };
            match ch {
                '{' if self.peek_second() == Some('{') => {
                    literal.push('{');
                    self.bump();
                    self.bump();
                }
                '{' => {
                    flush(&mut literal, &mut segments);
                    self.bump();
                    if self.peek() == Some('?') {
                        if in_branch {
                            return Err(TemplateError::NestedConditional);
                        }
                        self.bump();
                        segments.push(self.conditional()?);
                    } else {
                        segments.push(self.reference(in_branch)?);
                    }
                }
                '}' if self.peek_second() == Some('}') => {
                    literal.push('}');
                    self.bump();
                    self.bump();
                }
                '}' if in_branch => {
                    flush(&mut literal, &mut segments);
                    self.bump();
                    return Ok((segments, Stop::Close));
                }
                // A lone `}` outside any construct is text. `}}` is the spelling to
                // prefer, but refusing the template would only turn a harmless
                // character into a build failure.
                '}' => {
                    literal.push('}');
                    self.bump();
                }
                '|' if in_branch => {
                    flush(&mut literal, &mut segments);
                    self.bump();
                    return Ok((segments, Stop::Pipe));
                }
                _ => {
                    literal.push(ch);
                    self.bump();
                }
            }
        }
    }

    /// A `{name}` / `{index}` / `{body}` reference; the `{` is already consumed.
    fn reference(&mut self, in_branch: bool) -> Result<Seg, TemplateError> {
        let start = self.pos;
        loop {
            match self.peek() {
                None => return Err(TemplateError::UnterminatedConstruct),
                Some('}') => {
                    let name = &self.source[start..self.pos];
                    self.bump();
                    return Ok(segment_for(name));
                }
                // A `{` inside a reference, or a branch separator: the reference has no
                // closing brace where one is possible.
                Some('{') => return Err(TemplateError::UnterminatedConstruct),
                Some('|') if in_branch => return Err(TemplateError::UnterminatedConstruct),
                Some(_) => self.bump(),
            }
        }
    }

    /// A `{?name:then|else}` conditional; the `{?` is already consumed.
    fn conditional(&mut self) -> Result<Seg, TemplateError> {
        let start = self.pos;
        let name = loop {
            match self.peek() {
                Some(':') => {
                    let name = &self.source[start..self.pos];
                    self.bump();
                    break name;
                }
                // Anything else that could end the construct means there is no
                // condition to test.
                None | Some('}') | Some('|') | Some('{') => {
                    return Err(TemplateError::UnterminatedConstruct)
                }
                Some(_) => self.bump(),
            }
        };
        let arg = arg_ref(name);

        let (then, stop) = self.segments(true)?;
        let els = match stop {
            Stop::Close => Vec::new(),
            Stop::Pipe => {
                let (els, stop) = self.segments(true)?;
                if stop != Stop::Close {
                    // Either the source ran out, or a second `|` appeared where the
                    // grammar allows at most one.
                    return Err(TemplateError::UnterminatedConstruct);
                }
                els
            }
            Stop::End => return Err(TemplateError::UnterminatedConstruct),
        };
        Ok(Seg::IfPresent { arg, then, els })
    }
}

/// Move the accumulated literal text into the segment list.
fn flush(literal: &mut String, segments: &mut Vec<Seg>) {
    if !literal.is_empty() {
        segments.push(Seg::Str(core::mem::take(literal).into_boxed_str()));
    }
}

/// What `{name}` refers to.
fn segment_for(name: &str) -> Seg {
    if name == "body" {
        Seg::Body
    } else {
        Seg::Arg(arg_ref(name))
    }
}

/// A reference by name, or by index when the name is written as digits.
fn arg_ref(name: &str) -> ArgRef {
    if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()) {
        // An index too large for `usize` is out of range for every definition, so
        // saturating loses nothing: validation refuses it either way.
        ArgRef::Index(name.parse().unwrap_or(usize::MAX))
    } else {
        ArgRef::Name(name.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use proptest::prelude::*;

    fn parse(source: &str) -> Result<Vec<Seg>, TemplateError> {
        Template::new(source).parsed
    }

    fn name(text: &str) -> ArgRef {
        ArgRef::Name(text.into())
    }

    fn str_seg(text: &str) -> Seg {
        Seg::Str(text.into())
    }

    // ------------------------------------------------------------------ parsing

    #[test]
    fn plain_text_is_one_literal_segment() {
        assert_eq!(parse("hello"), Ok(vec![str_seg("hello")]));
    }

    #[test]
    fn the_empty_template_has_no_segments() {
        assert_eq!(parse(""), Ok(Vec::new()));
    }

    #[test]
    fn braces_are_escaped_by_doubling() {
        assert_eq!(parse("{{a}}"), Ok(vec![str_seg("{a}")]));
        assert_eq!(parse("{{{a}"), Ok(vec![str_seg("{"), Seg::Arg(name("a"))]));
    }

    #[test]
    fn a_lone_closing_brace_is_text() {
        assert_eq!(parse("a}b"), Ok(vec![str_seg("a}b")]));
    }

    #[test]
    fn references_come_out_typed() {
        assert_eq!(parse("{text}"), Ok(vec![Seg::Arg(name("text"))]));
        assert_eq!(parse("{2}"), Ok(vec![Seg::Arg(ArgRef::Index(2))]));
        assert_eq!(parse("{body}"), Ok(vec![Seg::Body]));
    }

    #[test]
    fn a_reference_and_literals_interleave() {
        assert_eq!(
            parse("{text} <{url}>"),
            Ok(vec![
                Seg::Arg(name("text")),
                str_seg(" <"),
                Seg::Arg(name("url")),
                str_seg(">"),
            ])
        );
    }

    #[test]
    fn a_conditional_has_two_branches() {
        assert_eq!(
            parse("{?star:s|p}"),
            Ok(vec![Seg::IfPresent {
                arg: name("star"),
                then: vec![str_seg("s")],
                els: vec![str_seg("p")],
            }])
        );
    }

    #[test]
    fn a_conditional_may_omit_the_else_branch() {
        assert_eq!(
            parse("{?opt:[{opt}]}"),
            Ok(vec![Seg::IfPresent {
                arg: name("opt"),
                then: vec![str_seg("["), Seg::Arg(name("opt")), str_seg("]")],
                els: Vec::new(),
            }])
        );
    }

    #[test]
    fn a_conditional_may_test_an_index() {
        assert_eq!(
            parse("{?1:yes}"),
            Ok(vec![Seg::IfPresent {
                arg: ArgRef::Index(1),
                then: vec![str_seg("yes")],
                els: Vec::new(),
            }])
        );
    }

    #[test]
    fn a_branch_may_contain_escaped_braces() {
        assert_eq!(
            parse("{?a:{{|}}}"),
            Ok(vec![Seg::IfPresent {
                arg: name("a"),
                then: vec![str_seg("{")],
                els: vec![str_seg("}")],
            }])
        );
    }

    // ------------------------------------------------------------ syntax errors

    #[test]
    fn an_unterminated_reference_is_refused() {
        assert_eq!(parse("{text"), Err(TemplateError::UnterminatedConstruct));
        assert_eq!(parse("a{b{c}"), Err(TemplateError::UnterminatedConstruct));
    }

    #[test]
    fn an_unterminated_conditional_is_refused() {
        assert_eq!(parse("{?a:x"), Err(TemplateError::UnterminatedConstruct));
        assert_eq!(parse("{?a:x|y"), Err(TemplateError::UnterminatedConstruct));
        assert_eq!(parse("{?a}"), Err(TemplateError::UnterminatedConstruct));
        assert_eq!(parse("{?a"), Err(TemplateError::UnterminatedConstruct));
    }

    #[test]
    fn a_third_branch_is_refused() {
        assert_eq!(
            parse("{?a:x|y|z}"),
            Err(TemplateError::UnterminatedConstruct)
        );
    }

    #[test]
    fn a_conditional_inside_a_branch_is_refused() {
        assert_eq!(parse("{?a:{?b:x}}"), Err(TemplateError::NestedConditional));
        assert_eq!(
            parse("{?a:x|{?b:y}}"),
            Err(TemplateError::NestedConditional)
        );
    }

    // -------------------------------------------------------------- validation

    fn names(list: &[&str]) -> Vec<Box<str>> {
        list.iter().map(|name| Box::<str>::from(*name)).collect()
    }

    fn check(source: &str, declared: &[&str], has_body: bool) -> Result<(), TemplateError> {
        let declared = names(declared);
        Template::new(source).validate(TemplateScope::declared(&declared, has_body))
    }

    #[test]
    fn a_declared_name_validates() {
        assert_eq!(check("{a}-{b}", &["a", "b"], false), Ok(()));
    }

    #[test]
    fn an_undeclared_name_is_refused() {
        assert_eq!(
            check("{c}", &["a", "b"], false),
            Err(TemplateError::UnknownArgumentName { name: "c".into() })
        );
    }

    #[test]
    fn an_empty_name_is_refused_even_without_declared_names() {
        assert_eq!(
            Template::new("{}").validate(TemplateScope::unchecked(false)),
            Err(TemplateError::UnknownArgumentName { name: "".into() })
        );
    }

    #[test]
    fn index_zero_is_refused() {
        assert_eq!(
            check("{0}", &["a"], false),
            Err(TemplateError::ArgumentIndexOutOfRange { index: 0, count: 1 })
        );
    }

    #[test]
    fn an_index_past_the_last_argument_is_refused() {
        assert_eq!(
            check("{3}", &["a", "b"], false),
            Err(TemplateError::ArgumentIndexOutOfRange { index: 3, count: 2 })
        );
        assert_eq!(check("{2}", &["a", "b"], false), Ok(()));
    }

    #[test]
    fn an_index_too_large_for_usize_is_out_of_range() {
        assert_eq!(
            check("{99999999999999999999999}", &["a"], false),
            Err(TemplateError::ArgumentIndexOutOfRange {
                index: usize::MAX,
                count: 1
            })
        );
    }

    #[test]
    fn body_needs_an_environment() {
        assert_eq!(check("{body}", &[], true), Ok(()));
        assert_eq!(
            check("{body}", &[], false),
            Err(TemplateError::BodyOutsideEnvironment)
        );
    }

    #[test]
    fn validation_reaches_inside_a_conditional() {
        assert_eq!(
            check("{?a:{c}}", &["a"], false),
            Err(TemplateError::UnknownArgumentName { name: "c".into() })
        );
        assert_eq!(
            check("{?a:x|{c}}", &["a"], false),
            Err(TemplateError::UnknownArgumentName { name: "c".into() })
        );
        assert_eq!(
            check("{?c:x}", &["a"], false),
            Err(TemplateError::UnknownArgumentName { name: "c".into() })
        );
    }

    #[test]
    fn validation_re_reports_a_syntax_error() {
        assert_eq!(
            check("{a", &["a"], false),
            Err(TemplateError::UnterminatedConstruct)
        );
    }

    #[test]
    fn an_unchecked_scope_accepts_names_it_cannot_know() {
        assert_eq!(
            Template::new("{whatever}{7}").validate(TemplateScope::unchecked(false)),
            Ok(())
        );
        // ... but never index 0, which no definition can have, nor a body where there
        // is none.
        assert_eq!(
            Template::new("{0}").validate(TemplateScope::unchecked(false)),
            Err(TemplateError::ArgumentIndexOutOfRange { index: 0, count: 0 })
        );
        assert_eq!(
            Template::new("{body}").validate(TemplateScope::unchecked(false)),
            Err(TemplateError::BodyOutsideEnvironment)
        );
    }

    // ------------------------------------------------------------- round trips

    /// Write segments back out as template source.
    ///
    /// Exact for everything the parser produces, which is what makes it a round-trip
    /// witness. Literal text inside a conditional's branch is the one thing the grammar
    /// cannot express in general — there is no escape for `|` — so the generator below
    /// keeps branch text free of it.
    fn unparse(segments: &[Seg]) -> String {
        let mut out = String::new();
        for segment in segments {
            match segment {
                Seg::Str(text) => {
                    for ch in text.chars() {
                        match ch {
                            '{' => out.push_str("{{"),
                            '}' => out.push_str("}}"),
                            _ => out.push(ch),
                        }
                    }
                }
                Seg::Arg(ArgRef::Name(name)) => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
                Seg::Arg(ArgRef::Index(index)) => {
                    out.push_str(&alloc::format!("{{{index}}}"));
                }
                Seg::Body => out.push_str("{body}"),
                Seg::IfPresent { arg, then, els } => {
                    out.push_str("{?");
                    match arg {
                        ArgRef::Name(name) => out.push_str(name),
                        ArgRef::Index(index) => out.push_str(&alloc::format!("{index}")),
                    }
                    out.push(':');
                    out.push_str(&unparse(then));
                    if !els.is_empty() {
                        out.push('|');
                        out.push_str(&unparse(els));
                    }
                    out.push('}');
                }
            }
        }
        out
    }

    #[test]
    fn unparsing_a_parsed_template_reproduces_its_meaning() {
        for source in [
            "",
            "plain",
            "{{a}}",
            "{text} <{url}>",
            "{1}{2}",
            "{body} tail",
            "{?star:*|}",
            "{?opt:[{opt}] }rest",
        ] {
            let segments = parse(source).expect("well-formed");
            let round_tripped = parse(&unparse(&segments)).expect("unparse stays valid");
            assert_eq!(segments, round_tripped, "source: {source}");
        }
    }

    /// One `{…}` reference.
    fn reference_source() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-z]{1,4}".prop_map(|name: String| alloc::format!("{{{name}}}")),
            (1usize..5).prop_map(|index| alloc::format!("{{{index}}}")),
            Just(String::from("{body}")),
        ]
    }

    /// One branch of a conditional: literals and references, no `|`, no conditional.
    fn branch_source() -> impl Strategy<Value = String> {
        proptest::collection::vec(prop_oneof!["[a-z {}]{0,4}", reference_source()], 0..3)
            .prop_map(|parts: Vec<String>| parts.concat())
    }

    /// Template source built from the grammar, so that the round-trip property has
    /// well-formed input to work on (PLAN.md §14.2).
    fn template_source() -> impl Strategy<Value = String> {
        let conditional = ("[a-z]{1,4}", branch_source(), branch_source()).prop_map(
            |(name, then, els): (String, String, String)| {
                alloc::format!("{{?{name}:{then}|{els}}}")
            },
        );
        proptest::collection::vec(
            prop_oneof!["[a-z {}]{0,6}", reference_source(), conditional],
            0..5,
        )
        .prop_map(|parts: Vec<String>| parts.concat())
    }

    proptest! {
        /// Parsing is stable under writing the parse back out (PLAN.md §14.2).
        #[test]
        fn parsing_round_trips_through_the_source_form(source in template_source()) {
            // The generator escapes nothing, so raw braces in its literals may make a
            // construct; whatever the source means, writing the parse back out and
            // parsing that again must mean the same thing.
            if let Ok(segments) = parse(&source) {
                let again = parse(&unparse(&segments));
                prop_assert_eq!(Ok(segments), again);
            }
        }

        /// No string, however malformed, may make the parser panic.
        #[test]
        fn parsing_never_panics(source in ".{0,80}") {
            let _ = Template::new(source);
        }
    }
}
