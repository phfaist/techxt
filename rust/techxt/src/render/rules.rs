//! Executing a [`TextRule`], and deciding what to do without one (PLAN.md §10.4, §10.6).

use alloc::string::String;

use techy::core::node::ParsedArgument;

use crate::convert::{UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy};
use crate::def::{ArgRef, CallableKind, Seg, Template, TextRule};
use crate::diag::{HandlerFailed, UnknownEnvironment, UnknownMacro, UnknownSpecials};
use crate::flow::{Flow, FlowItem};

use super::cx::{RenderCx, RenderError};

/// Render the callable `cx` is positioned on through `rule` (PLAN.md §10.4).
///
/// Never fails: a rule that cannot do its job costs its construct the output and raises
/// a `techxt.handler-failed` diagnostic, and the conversion continues. Losing one macro
/// must not cost the reader the rest of the document.
pub(crate) fn execute(rule: &TextRule, cx: &mut RenderCx<'_, '_>) -> Flow {
    match rule {
        TextRule::Literal(text) => literal(text, cx),
        TextRule::Template(template) => template_flow(template, cx),
        TextRule::Skip => Flow::new(),
        TextRule::Content => content(cx),
        TextRule::Handler(handler) => {
            let node = cx.node();
            match handler.render(node, cx) {
                Ok(flow) => flow,
                Err(error) => {
                    fail(cx, &error);
                    Flow::new()
                }
            }
        }
    }
}

/// Replacement text produced by a rule (PLAN.md §10.4).
///
/// It is treated exactly like document text — words and inter-word spaces — but it is
/// *not* document text in one respect: font styling never applies to it (DECISIONS.md
/// D6). `\sin` must come out as an upright `sin` next to an italic variable, and
/// running rule output through the math alphabets would italicize every operator name
/// in the document.
///
/// **M5b seam:** inside Fancy math this text must be segmented into math atoms
/// (`mathfmt::segment_plain`), exactly as v3 segments its replacement strings, so that
/// the joiner spaces `\to` as a relation. Until the math engine lands, math is rendered
/// in text mode and this path is the text one.
fn literal(text: &str, _cx: &mut RenderCx<'_, '_>) -> Flow {
    Flow::from_plain_text(text)
}

/// Substitute a template's argument references and treat the result as literal text
/// (PLAN.md §10.5).
fn template_flow(template: &Template, cx: &mut RenderCx<'_, '_>) -> Flow {
    let mut flow = Flow::new();
    render_segments(template.segments(), cx, &mut flow);
    flow
}

/// Append the rendering of `segments` to `flow`.
fn render_segments(segments: &[Seg], cx: &mut RenderCx<'_, '_>, flow: &mut Flow) {
    for segment in segments {
        match segment {
            Seg::Str(text) => flow.extend(literal(text, cx)),
            Seg::Arg(reference) => {
                if let Some(rendered) = arg_ref(reference, cx) {
                    flow.extend(rendered);
                }
            }
            Seg::Body => match cx.body() {
                Ok(body) => flow.extend(body),
                Err(error) => fail(cx, &error),
            },
            // `{?name:then|else}` (PLAN.md §10.5): which branch runs depends only on
            // whether the argument was *written*, never on what it rendered to — an
            // argument that renders as nothing is still present.
            Seg::IfPresent { arg, then, els } => {
                let branch = if arg_present(arg, cx) { then } else { els };
                render_segments(branch, cx, flow);
            }
        }
    }
}

/// Whether the argument a conditional tests was written.
fn arg_present(reference: &ArgRef, cx: &RenderCx<'_, '_>) -> bool {
    match reference {
        ArgRef::Name(name) => cx.arg_provided(name),
        // Template indices are 1-based; techy counts arguments from zero. An index the
        // definition does not have reads as absent, which is what the else branch is
        // for — build-time validation has already refused it for a real definition.
        ArgRef::Index(index) => index
            .checked_sub(1)
            .and_then(|zero_based| {
                cx.node()
                    .arguments()
                    .and_then(|arguments| arguments.get(zero_based))
            })
            .is_some_and(ParsedArgument::is_provided),
    }
}

/// Render one argument reference; `None` when the argument is absent, which renders as
/// nothing.
fn arg_ref(reference: &ArgRef, cx: &mut RenderCx<'_, '_>) -> Option<Flow> {
    let rendered = match reference {
        ArgRef::Name(name) => cx.arg(name),
        // Template indices are 1-based (PLAN.md §10.5); `arg_at` counts from zero.
        ArgRef::Index(index) => match index.checked_sub(1) {
            Some(zero_based) => cx.arg_at(zero_based),
            None => Err(RenderError::region_detail(
                "argument index 0: template indices count from 1",
            )),
        },
    };
    match rendered {
        Ok(flow) => flow,
        Err(error) => {
            fail(cx, &error);
            None
        }
    }
}

/// Render a construct's own content (PLAN.md §10.4, `TextRule::Content`).
///
/// For a macro that is every *provided* argument's content in declaration order, with
/// nothing inserted between them — `\emph{a}` renders `a`, and `\textcolor{red}{text}`
/// would render `redtext`, which is why a rule that wants to drop an argument uses a
/// template rather than this. For an environment it is the body, and for specials the
/// trigger characters themselves.
fn content(cx: &mut RenderCx<'_, '_>) -> Flow {
    let node = cx.node();
    match CallableKind::of(node) {
        Some(CallableKind::Environment) => match cx.body() {
            Ok(body) => body,
            Err(error) => {
                fail(cx, &error);
                Flow::new()
            }
        },
        Some(CallableKind::Specials) => Flow::from_plain_text(node.name().unwrap_or("")),
        _ => arguments_in_order(cx),
    }
}

/// Every provided argument's content, in declaration order.
///
/// Declaration order is not a detail: the fold's side effects — footnote numbering,
/// heading counters, `\title` capture — happen while a region is being folded, so
/// rendering argument 2 before argument 1 would reorder them.
fn arguments_in_order(cx: &mut RenderCx<'_, '_>) -> Flow {
    let count = cx.node().arguments().map_or(0, |arguments| arguments.len());
    let mut flow = Flow::new();
    for index in 0..count {
        match cx.arg_at(index) {
            Ok(Some(rendered)) => flow.extend(rendered),
            Ok(None) => {}
            Err(error) => fail(cx, &error),
        }
    }
    flow
}

/// Render a callable no rule was found for, per the configured policy (PLAN.md §10.6).
///
/// The diagnostic is raised whatever the policy says: the policy decides what the
/// reader sees, not whether the problem is reported.
pub(crate) fn unknown(kind: Option<CallableKind>, cx: &mut RenderCx<'_, '_>) -> Flow {
    let node = cx.node();
    let name = String::from(node.name().unwrap_or(""));
    match kind {
        Some(CallableKind::Macro) => {
            cx.report(UnknownMacro::new(name.clone()));
            match cx.options().unknown_macro {
                UnknownMacroPolicy::Skip => Flow::new(),
                UnknownMacroPolicy::RenderArgs => arguments_in_order(cx),
                UnknownMacroPolicy::KeepSource => keep_source(cx, false),
                UnknownMacroPolicy::Placeholder => Flow::text(&alloc::format!("<{name}>")),
            }
        }
        Some(CallableKind::Environment) => {
            cx.report(UnknownEnvironment::new(name));
            match cx.options().unknown_env {
                UnknownEnvPolicy::RenderBody => match cx.body() {
                    Ok(body) => body,
                    Err(error) => {
                        fail(cx, &error);
                        Flow::new()
                    }
                },
                UnknownEnvPolicy::Skip => Flow::new(),
                UnknownEnvPolicy::KeepSource => keep_source(cx, true),
            }
        }
        Some(CallableKind::Specials) => {
            cx.report(UnknownSpecials::new(name.clone()));
            match cx.options().unknown_specials {
                UnknownSpecialsPolicy::EmitChars => Flow::from_plain_text(&name),
                UnknownSpecialsPolicy::Skip => Flow::new(),
            }
        }
        // A callable of a kind techxt has no vocabulary for: it cannot be named in any
        // policy, so it renders as nothing.
        None => Flow::new(),
    }
}

/// The construct's LaTeX source, as preformatted text that layout will not touch.
///
/// An environment is a block, so its source is emitted as a verbatim *block*; a macro
/// or a specials sits in running text, so its source is an unbreakable inline run.
fn keep_source(cx: &mut RenderCx<'_, '_>, block: bool) -> Flow {
    let node = cx.node();
    match cx.source_of(node) {
        Ok(source) => {
            let mut flow = Flow::new();
            flow.push(if block {
                FlowItem::Verbatim(source.into())
            } else {
                FlowItem::InlineVerbatim(source.into())
            });
            flow
        }
        Err(error) => {
            fail(cx, &error);
            Flow::new()
        }
    }
}

/// Report that rendering a construct failed, and render nothing.
fn fail(cx: &mut RenderCx<'_, '_>, error: &RenderError) {
    let construct = construct_name(cx);
    let detail = String::from(error.detail());
    cx.report(HandlerFailed::new(construct, detail));
}

/// How the construct being rendered is written, for a diagnostic message.
fn construct_name(cx: &RenderCx<'_, '_>) -> String {
    let node = cx.node();
    let name = node.name().unwrap_or("");
    match CallableKind::of(node) {
        Some(kind) => kind.write_construct(name),
        None => String::from(name),
    }
}
