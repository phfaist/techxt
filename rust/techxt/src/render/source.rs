//! Re-emitting a subtree as LaTeX source, from payloads only.
//!
//! Two features need the source text a node was parsed from: the `KeepSource` policies
//! for unknown constructs (PLAN.md §10.6) and math's `Source` mode (PLAN.md §9.5).
//!
//! Neither may reach for the source *bytes*. PLAN.md §1.6 is a hard rule, not a style
//! preference: on a tree that has been transformed, `span_content()` returns the
//! *pre-transform* text, `NodeSlice::source_text()` answers `None` across a run that
//! spans two sources — which a plain parse already produces at an `\input` boundary —
//! and resolving a payload against the wrong source panics outright. So the source is
//! reassembled from what each node *records*: its characters, its group delimiters, its
//! invocation syntax. That is what techy's own `SourceRecomposer` does, and this is the
//! same reconstruction, reachable from inside techxt's fold and producing techxt's
//! piece type.
//!
//! The walk uses an explicit stack rather than recursion: a document nests as deeply as
//! its author likes, and no document may cost the process its stack.

use alloc::string::String;
use alloc::vec::Vec;

use techy::core::node::{NodeKind, NodeRef, SlotRole};
use techy::latexlike::{EnvironmentSyntax, LatexlikeInvocationSyntax};
use techy_xp::lang::LatexlikeXp;

/// One pending piece of work: a subtree to emit, or text to emit after it.
enum Step<'t> {
    /// Emit this node, and schedule what follows it.
    Node(NodeRef<'t, LatexlikeXp>),
    /// Emit this text (an environment's `\end{…}`, a group's closing delimiter).
    Tail(String),
}

/// The LaTeX source `node` was parsed from, reassembled from node payloads.
///
/// Byte-exact for a parsed tree, tolerant-recovery shapes included; on a tree that was
/// synthesized or transformed it reproduces what the nodes now *say*, which is the only
/// answer that is meaningful there.
pub(crate) fn latex_source(node: NodeRef<'_, LatexlikeXp>) -> String {
    let mut out = String::new();
    let mut stack: Vec<Step<'_>> = alloc::vec![Step::Node(node)];
    // Reused across nodes so that a wide tree does not reallocate per callable.
    let mut children: Vec<NodeRef<'_, LatexlikeXp>> = Vec::new();

    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Tail(text) => {
                out.push_str(&text);
                continue;
            }
            Step::Node(node) => node,
        };

        // The node's own opening spelling goes out now; its closing spelling is
        // scheduled behind its children, which are pushed in reverse so that popping
        // yields them in document order.
        let tail = match node.kind() {
            NodeKind::Chars { .. } => {
                // `chars()` resolves against the node's own source: payload-safe.
                out.push_str(node.chars().unwrap_or(""));
                None
            }
            NodeKind::Comment(_) => {
                if let Some(comment) = node.comment() {
                    let source = node.span().source();
                    out.push_str(comment.start.resolve(source));
                    out.push_str(comment.content.resolve(source));
                    out.push_str(comment.post_space.resolve(source));
                }
                None
            }
            NodeKind::List => None,
            NodeKind::Group(_) => match node.group_delimiters() {
                Some((open, close)) => {
                    out.push_str(open);
                    Some(String::from(close))
                }
                None => None,
            },
            NodeKind::Callable(data) => {
                let source = node.span().source();
                let name: &str = &data.name;
                let syntax = &data.invocation_syntax;
                if let Some((escape_char, post_space)) = syntax.macro_syntax() {
                    out.push(escape_char);
                    out.push_str(name);
                    out.push_str(post_space.resolve(source));
                    None
                } else if let Some(environment) = syntax.environment_syntax() {
                    out.push_str(&environment.write_begin(name, source));
                    Some(environment.write_end(name, source))
                } else {
                    // Specials: the name as written *is* the spelling.
                    out.push_str(name);
                    None
                }
            }
        };

        if let Some(tail) = tail {
            stack.push(Step::Tail(tail));
        }
        children.clear();
        collect_scoped_children(node, &mut children);
        for child in children.iter().rev() {
            stack.push(Step::Node(*child));
        }
    }

    out
}

/// The children techy's own `Concat` would fold: everything except the contents of
/// `Attached` and `Hidden` slots.
///
/// Skipping attached content is what makes `\input{a.tex}` re-emit as the invocation it
/// was written as, rather than as the file it pulled in.
fn collect_scoped_children<'t>(
    node: NodeRef<'t, LatexlikeXp>,
    out: &mut Vec<NodeRef<'t, LatexlikeXp>>,
) {
    let children = node.children();
    if children.is_empty() {
        return;
    }
    // Slot regions are global node-index ranges; a child's global index is its
    // position in the children slice offset by where that slice starts.
    let start = children.range().start;
    let mut excluded: Vec<core::ops::Range<u32>> = Vec::new();
    if let Some(slots) = node.slots() {
        for slot in slots.iter() {
            let skipped = matches!(slot.role, SlotRole::Attached | SlotRole::Hidden);
            // A staged region has no node-index range yet; every region read from a
            // finished tree is resolved, and asking an unresolved one would panic.
            if skipped && slot.region.is_resolved() {
                excluded.push(slot.region.children());
            }
        }
    }
    for index in 0..children.len() {
        let global = start + index as u32;
        if excluded.iter().any(|range| range.contains(&global)) {
            continue;
        }
        if let Some(child) = children.get(index) {
            out.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{Converter, StdDescentGuardInit};

    /// The LaTeX source of the whole document, reassembled from payloads.
    ///
    /// The descent guard is configured explicitly (DECISIONS.md D9): techy's default is
    /// a *stack budget*, so how deep a document may nest depends on the build profile,
    /// and the deep-nesting test below needs a limit that does not.
    fn round_trip(latex: &str) -> String {
        let converter = Converter::builder()
            .descent_guard(StdDescentGuardInit::depth_limit(200))
            .build()
            .expect("the placeholder definitions build");
        let tree = converter.language().parse(latex).expect("parses").tree;
        latex_source(tree.root())
    }

    #[test]
    fn plain_text_and_groups_round_trip() {
        assert_eq!(round_trip("a  b\n\nc"), "a  b\n\nc");
        assert_eq!(round_trip("a {b {c}} d"), "a {b {c}} d");
    }

    #[test]
    fn comments_keep_their_delimiter_and_trailing_space() {
        assert_eq!(round_trip("A% note\nB"), "A% note\nB");
    }

    #[test]
    fn macros_keep_their_escape_character_name_and_post_space() {
        assert_eq!(round_trip(r"\emph{x}"), r"\emph{x}");
        assert_eq!(round_trip(r"\ldots more"), r"\ldots more");
        assert_eq!(round_trip(r"\href{u}{t}"), r"\href{u}{t}");
    }

    #[test]
    fn environments_keep_both_ends() {
        assert_eq!(
            round_trip(r"\begin{center}body\end{center}"),
            r"\begin{center}body\end{center}"
        );
        assert_eq!(
            round_trip("\\begin{verbatim}\n raw \n\\end{verbatim}"),
            "\\begin{verbatim}\n raw \n\\end{verbatim}"
        );
    }

    #[test]
    fn specials_and_math_keep_their_spelling() {
        assert_eq!(round_trip("a~b"), "a~b");
        assert_eq!(round_trip("$x^2$"), "$x^2$");
        assert_eq!(round_trip(r"\[ a \]"), r"\[ a \]");
        assert_eq!(round_trip(r"\verb|x_1|"), r"\verb|x_1|");
    }

    #[test]
    fn an_unterminated_environment_re_emits_without_its_terminator() {
        assert_eq!(round_trip(r"\begin{center}x"), r"\begin{center}x");
    }

    #[test]
    fn a_deep_tree_costs_no_stack() {
        // The walk uses an explicit stack, so nesting is bounded by memory rather than
        // by the thread's stack — a document that parses always re-emits.
        let deep = alloc::format!("{}x{}", "{".repeat(30), "}".repeat(30));
        assert_eq!(round_trip(&deep), deep);
    }
}
