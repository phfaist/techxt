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
//! Reassembly is not quite concatenation, though: a macro name has no terminator of its
//! own, so where two payloads that were never adjacent in any source end up adjacent
//! here, the emitter has to put the separator back — see [`Emitter`].
//!
//! The walk uses an explicit stack rather than recursion: a document nests as deeply as
//! its author likes, and no document may cost the process its stack.

use alloc::string::String;
use alloc::vec::Vec;

use techy::core::node::{NodeKind, NodeRef, SlotRole};
use techy::latexlike::{EnvironmentSyntax, LatexlikeInvocationSyntax, LatexlikeLang};

/// One pending piece of work: a subtree to emit, or text to emit after it.
enum Step<'t, LLL: LatexlikeLang> {
    /// Emit this node, and schedule what follows it.
    Node(NodeRef<'t, LLL>),
    /// Emit this text (an environment's `\end{…}`, a group's closing delimiter).
    Tail(String),
}

/// The reassembled source, built up piece by piece, with a macro name kept from running
/// into whatever is emitted after it.
///
/// A macro records the post-space it was written with, and that is usually the
/// separator its name needs: `\ldots more` re-emits the space it swallowed. But a macro
/// written at the very end of what it stood in recorded *no* post-space, and expansion
/// then puts it next to text it was never adjacent to. `\newcommand\ketx[1]{\lvert{#1}\rangle}`
/// invoked as `\ketx\phi x` is the case that bites: the body's `\rangle` ends the
/// definition, the `x` follows the expansion, and a plain concatenation says `\ranglex`
/// — one name that no longer exists, in place of two tokens that did.
///
/// So a macro emitted with a bare name *arms* the emitter with the characters that
/// would extend that name, and the next piece emitted gets a space in front of it if it
/// starts with one of them. Nothing else is inserted anywhere: every other spelling
/// here carries its own boundary.
struct Emitter<'t> {
    /// What has been emitted so far.
    out: String,
    /// Set when the source so far ends in a bare macro name: the characters that the
    /// tokenizer would read as *more of that name*.
    extenders: Option<&'t str>,
}

impl<'t> Emitter<'t> {
    /// An emitter with nothing emitted yet.
    fn new() -> Emitter<'t> {
        Emitter {
            out: String::new(),
            extenders: None,
        }
    }

    /// Emit `text`, separated from a preceding bare macro name where it would otherwise
    /// be read as part of it.
    ///
    /// Emitting nothing is not emitting: an absent post-space and the empty `\end` of an
    /// unterminated environment both arrive here as `""`, and neither may disarm the
    /// name that is still the last thing written.
    fn push(&mut self, text: &str) {
        let Some(first) = text.chars().next() else {
            return;
        };
        if self
            .extenders
            .take()
            .is_some_and(|chars| chars.contains(first))
        {
            self.out.push(' ');
        }
        self.out.push_str(text);
    }

    /// Emit one character, under the same rule as [`push`](Emitter::push).
    fn push_char(&mut self, c: char) {
        let mut buffer = [0u8; 4];
        self.push(c.encode_utf8(&mut buffer));
    }

    /// Note that the source so far ends in a macro name that `extenders` would grow.
    fn arm(&mut self, extenders: &'t str) {
        self.extenders = Some(extenders);
    }

    /// The source emitted so far.
    fn finish(self) -> String {
        self.out
    }
}

/// The LaTeX source `node` was parsed from, reassembled from node payloads.
///
/// Byte-exact for a parsed tree, tolerant-recovery shapes included; on a tree that was
/// synthesized or transformed it reproduces what the nodes now *say*, which is the only
/// answer that is meaningful there.
pub(crate) fn latex_source<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> String {
    let mut emitter = Emitter::new();
    emit_subtree(node, &mut emitter);
    emitter.finish()
}

/// The LaTeX source of a run of sibling subtrees, reassembled as one piece of source.
///
/// Not the concatenation of their [`latex_source`]s: the boundary between two of them is
/// a place a macro name can run on, exactly as a boundary inside one is.
pub(crate) fn latex_source_of<'t, LLL: LatexlikeLang>(
    nodes: impl IntoIterator<Item = NodeRef<'t, LLL>>,
) -> String {
    let mut emitter = Emitter::new();
    for node in nodes {
        emit_subtree(node, &mut emitter);
    }
    emitter.finish()
}

/// Emit `node` and everything under it into `emitter`.
fn emit_subtree<'t, LLL: LatexlikeLang>(node: NodeRef<'t, LLL>, emitter: &mut Emitter<'t>) {
    let mut stack: Vec<Step<'t, LLL>> = alloc::vec![Step::Node(node)];
    // Reused across nodes so that a wide tree does not reallocate per callable.
    let mut children: Vec<NodeRef<'t, LLL>> = Vec::new();

    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Tail(text) => {
                emitter.push(&text);
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
                emitter.push(node.chars().unwrap_or(""));
                None
            }
            NodeKind::Comment(_) => {
                if let Some(comment) = node.comment() {
                    let source = node.span().source();
                    emitter.push(comment.start.resolve(source));
                    emitter.push(comment.content.resolve(source));
                    emitter.push(comment.post_space.resolve(source));
                }
                None
            }
            NodeKind::List => None,
            NodeKind::Group(_) => match node.group_delimiters() {
                Some((open, close)) => {
                    emitter.push(open);
                    Some(String::from(close))
                }
                None => None,
            },
            NodeKind::Callable(data) => {
                let source = node.span().source();
                let name: &str = &data.name;
                let syntax = &data.invocation_syntax;
                if let Some((escape_char, post_space)) = syntax.macro_syntax() {
                    let post_space = post_space.resolve(source);
                    emitter.push_char(escape_char);
                    emitter.push(name);
                    emitter.push(post_space);
                    // With a post-space of its own the name is already terminated;
                    // without one, whatever comes next has to be kept off it.
                    if post_space.is_empty() {
                        if let Some(extenders) = name_extenders(node, escape_char, name) {
                            emitter.arm(extenders);
                        }
                    }
                    None
                } else if let Some(environment) = syntax.environment_syntax() {
                    emitter.push(&environment.write_begin(name, source));
                    Some(environment.write_end(name, source))
                } else {
                    // Specials: the name as written *is* the spelling.
                    emitter.push(name);
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
}

/// The characters a following one would be swallowed into `name` as more of, when the
/// macro is re-emitted with nothing after its name — `None` when the name is one
/// nothing extends.
///
/// This is the tokenizer's own rule (techy's `scan_command`), asked of the state the
/// node was parsed in: a name grows over its command rule's name characters only when
/// its *first* character is one of them, which is what separates `\rangle`, where a
/// letter would be read as a seventh character of the name, from `\%`, where the next
/// character is always a new token. The rules are read off the node's parsing state
/// rather than assumed, so a language that spells its commands its own way — a
/// different escape character, digits in names — is answered in its own terms, and
/// payload is all that is consulted (PLAN.md §1.6).
fn name_extenders<'t, LLL: LatexlikeLang>(
    node: NodeRef<'t, LLL>,
    escape_char: char,
    name: &str,
) -> Option<&'t str> {
    let first = name.chars().next()?;
    // The first rule whose escape character matches is the one the tokenizer would have
    // read this command under, and so the one whose name characters decide.
    let rule = node
        .parsing_state()
        .rules()
        .command_rules()
        .iter()
        .find(|rule| rule.escape_char == escape_char)?;
    if rule.name_chars.contains(first) {
        Some(&rule.name_chars)
    } else {
        None
    }
}

/// The children techy's own `Concat` would fold: everything except the contents of
/// `Attached` and `Hidden` slots.
///
/// Skipping attached content is what makes `\input{a.tex}` re-emit as the invocation it
/// was written as, rather than as the file it pulled in.
fn collect_scoped_children<'t, LLL: LatexlikeLang>(
    node: NodeRef<'t, LLL>,
    out: &mut Vec<NodeRef<'t, LLL>>,
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
    fn an_expanded_macro_name_is_kept_off_what_follows_it() {
        // The bug this guards: a macro at the end of a `\newcommand` body recorded no
        // post-space, because in the definition nothing followed it. Expansion then puts
        // it in front of the text after the invocation, and a plain concatenation reads
        // back as one longer name — `\ranglex` where the tree says `\rangle` then `x`.
        assert_eq!(
            round_trip(r"\newcommand\ketx[1]{\lvert{#1}\rangle}$\ketx\phi x$"),
            r"\newcommand\ketx[1]{\lvert{#1}\rangle}$\lvert{\phi}\rangle x$"
        );
        assert_eq!(
            round_trip(r"\newcommand\aa{\alpha}$\aa b$"),
            r"\newcommand\aa{\alpha}$\alpha b$"
        );
    }

    #[test]
    fn only_a_name_that_would_grow_gets_a_separator() {
        // A space costs nothing to read but is not free to write: it lands in the output
        // the app hands to MathJax, so it goes in exactly where the tokenizer would
        // otherwise read one token where the tree holds two.
        //
        // An escape character ends a name, so the next macro needs nothing between.
        assert_eq!(
            round_trip(r"\newcommand\aa{\alpha}$\aa\beta$"),
            r"\newcommand\aa{\alpha}$\alpha\beta$"
        );
        // So does a digit, a brace, and anything else outside the name characters.
        assert_eq!(
            round_trip(r"\newcommand\aa{\alpha}$\aa 12$"),
            r"\newcommand\aa{\alpha}$\alpha12$"
        );
        assert_eq!(
            round_trip(r"\newcommand\aa{\alpha}$\aa{}b$"),
            r"\newcommand\aa{\alpha}$\alpha{}b$"
        );
        // And a one-character name is never extended in the first place: `\%x` is the
        // two tokens it looks like, where `\alphax` would not be.
        assert_eq!(
            round_trip(r"\newcommand\pc{\%}$\pc x$"),
            r"\newcommand\pc{\%}$\%x$"
        );
    }

    #[test]
    fn a_deep_tree_costs_no_stack() {
        // The walk uses an explicit stack, so nesting is bounded by memory rather than
        // by the thread's stack — a document that parses always re-emits.
        let deep = alloc::format!("{}x{}", "{".repeat(30), "}".repeat(30));
        assert_eq!(round_trip(&deep), deep);
    }
}
