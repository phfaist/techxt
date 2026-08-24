//! The node as a rule sees it: [`NodeView`], techxt's language-erased node handle.
//!
//! A [`TextHandler`](crate::def::TextHandler) is handed one of these rather than a
//! techy [`NodeRef`], and the difference is the whole point: a `NodeRef` names the
//! language its tree was parsed with, so a handler written against one language cannot
//! be called on a tree of another. techxt renders *any*
//! [`LatexlikeLang`](techy::latexlike::LatexlikeLang) tree (PLAN.md §11.1), so the
//! language is erased here — behind [`TreeView`], one object-safe trait implemented for
//! every such tree — and a handler compiled once runs on all of them.
//!
//! What the view offers is deliberately small and payload-only (PLAN.md §1.6): the
//! node's own payload, the tree's shape, and the reassembled LaTeX source of a subtree.
//! There is no way back to a `NodeRef`, and no span-content read anywhere: a handler
//! that wants content asks the [`RenderCx`](super::RenderCx) for it, which folds it
//! through the renderer.

use alloc::string::String;
use alloc::vec::Vec;

use techy::core::node::{NodeId, NodeRef, NodeTree};
use techy::latexlike::LatexlikeLang;
use techy::source::SourceSpan;

use crate::def::CallableKind;

use super::source::latex_source;

/// One parsed tree, with its language erased.
///
/// Implemented for every `NodeTree<LLL, ()>` whose language is latexlike and whose
/// sources are origin-tagged the way techxt's diagnostics are. Every method takes the
/// id of a node *of this tree*; ids reach it only through a [`NodeView`], which is
/// minted from a [`NodeRef`] and never leaves its tree, so the ids are always this
/// tree's own.
pub(crate) trait TreeView {
    /// The node's parent, or `None` at the root.
    fn parent(&self, id: NodeId) -> Option<NodeId>;

    /// How many children the node has.
    fn child_count(&self, id: NodeId) -> usize;

    /// The child at `index`, counting from zero.
    fn child(&self, id: NodeId, index: usize) -> Option<NodeId>;

    /// The callable's name, `None` for every other kind of node.
    fn name(&self, id: NodeId) -> Option<&str>;

    /// The characters node's text, `None` for every other kind of node.
    fn chars(&self, id: NodeId) -> Option<&str>;

    /// Which kind of callable the node is, `None` when it is not one.
    fn callable_kind(&self, id: NodeId) -> Option<CallableKind>;

    /// Where the node came from.
    fn span(&self, id: NodeId) -> &SourceSpan<Option<String>>;

    /// The LaTeX source the subtree was parsed from, reassembled from payloads.
    fn source(&self, id: NodeId) -> String;

    /// The reassembled source of the content of the argument called `name`.
    fn argument_source(&self, id: NodeId, name: &str) -> Option<String>;
}

impl<LLL> TreeView for NodeTree<LLL, ()>
where
    LLL: LatexlikeLang<SourceOrigin = Option<String>>,
{
    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent().map(|parent| parent.id())
    }

    fn child_count(&self, id: NodeId) -> usize {
        self.node(id).children().len()
    }

    fn child(&self, id: NodeId, index: usize) -> Option<NodeId> {
        self.node(id).child(index).map(|child| child.id())
    }

    fn name(&self, id: NodeId) -> Option<&str> {
        self.node(id).name()
    }

    fn chars(&self, id: NodeId) -> Option<&str> {
        self.node(id).chars()
    }

    fn callable_kind(&self, id: NodeId) -> Option<CallableKind> {
        CallableKind::of(self.node(id))
    }

    fn span(&self, id: NodeId) -> &SourceSpan<Option<String>> {
        self.node(id).span()
    }

    fn source(&self, id: NodeId) -> String {
        latex_source(self.node(id))
    }

    fn argument_source(&self, id: NodeId, name: &str) -> Option<String> {
        // `Err` is "no such argument declared", `Ok(None)` is "declared but absent";
        // neither has a source, and the caller wants to tell neither apart.
        let nodes = self.node(id).argument_content_nodes_named(name).ok()??;
        let mut source = String::new();
        for content in nodes.iter() {
            source.push_str(&latex_source(content));
        }
        Some(source)
    }
}

/// A node of the tree being converted, with its language erased (PLAN.md §10.4).
///
/// This is what a [`TextHandler`](crate::def::TextHandler) is handed. It is `Copy`, it
/// borrows the tree for as long as the fold does, and everything it answers is *payload*
/// (PLAN.md §1.6) — nothing here reads the source buffer behind a span, so a view works
/// on a transformed tree exactly as it does on a freshly parsed one.
///
/// A handler that wants the *rendering* of something — an argument, a body — asks the
/// [`RenderCx`](super::RenderCx) rather than this: the context folds the region through
/// the renderer, so whatever is nested inside gets the same treatment it would get
/// anywhere else. The view is for the questions a fold cannot answer: what is this
/// construct called, what surrounds it, what did it look like in the source.
#[derive(Clone, Copy)]
pub struct NodeView<'t> {
    tree: &'t dyn TreeView,
    id: NodeId,
}

impl<'t> NodeView<'t> {
    /// The view of a techy node, whatever language its tree is in.
    pub(crate) fn of<LLL>(node: NodeRef<'t, LLL, ()>) -> NodeView<'t>
    where
        LLL: LatexlikeLang<SourceOrigin = Option<String>>,
    {
        NodeView {
            tree: node.tree(),
            id: node.id(),
        }
    }

    /// The same tree's view of another node.
    fn sibling_view(&self, id: NodeId) -> NodeView<'t> {
        NodeView {
            tree: self.tree,
            id,
        }
    }

    /// The node's identity within its tree.
    ///
    /// Two views compare equal by this exactly when they are the same node of the same
    /// tree, which is what a walk that must not count a node twice keys on.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The callable's name — `emph` for `\emph`, `itemize` for the environment, the
    /// trigger characters for a specials — and `None` for every other kind of node.
    pub fn name(&self) -> Option<&'t str> {
        let tree: &'t dyn TreeView = self.tree;
        tree.name(self.id)
    }

    /// Which of the three kinds of callable this is, or `None` when it is not a
    /// callable at all (PLAN.md §10.3).
    pub fn callable_kind(&self) -> Option<CallableKind> {
        self.tree.callable_kind(self.id)
    }

    /// The text of a characters node, and `None` for every other kind of node.
    ///
    /// The node's own payload, resolved against the node's own source: the one
    /// text-reading operation PLAN.md §1.6 permits.
    pub fn chars(&self) -> Option<&'t str> {
        let tree: &'t dyn TreeView = self.tree;
        tree.chars(self.id)
    }

    /// Where in the source this node came from — for a diagnostic's position, which is
    /// the only thing a span may be used for (PLAN.md §1.6).
    pub fn span(&self) -> &'t SourceSpan<Option<String>> {
        let tree: &'t dyn TreeView = self.tree;
        tree.span(self.id)
    }

    /// The node this one hangs under, or `None` at the root of the tree.
    pub fn parent(&self) -> Option<NodeView<'t>> {
        let this = *self;
        self.tree
            .parent(self.id)
            .map(move |id| this.sibling_view(id))
    }

    /// The node's children, in document order.
    pub fn children(&self) -> impl Iterator<Item = NodeView<'t>> + 't {
        let this = *self;
        let count = self.tree.child_count(self.id);
        (0..count).filter_map(move |index| {
            this.tree
                .child(this.id, index)
                .map(|id| this.sibling_view(id))
        })
    }

    /// Everything under this node, in document order (preorder, this node excluded).
    ///
    /// The same order techy's own `NodeRef::descendants` walks in, and walked with an
    /// explicit stack for the same reason the source recomposer uses one: a document
    /// nests as deeply as its author likes, and no document may cost the process its
    /// stack.
    pub fn descendants(&self) -> impl Iterator<Item = NodeView<'t>> + 't {
        let mut stack = Vec::new();
        push_children_reversed(self.tree, self.id, &mut stack);
        Descendants {
            tree: self.tree,
            stack,
        }
    }

    /// The LaTeX source this subtree was parsed from, reassembled from node payloads.
    ///
    /// Never read out of the source buffer, so it works on transformed trees too
    /// (PLAN.md §1.6); on a tree that was synthesized or transformed it reproduces what
    /// the nodes now *say*, which is the only answer that is meaningful there.
    pub fn source(&self) -> String {
        self.tree.source(self.id)
    }

    /// The reassembled source of the content of the argument called `name`.
    ///
    /// `None` when the callable declares no such argument, and when it declares one that
    /// was not written — an absent optional argument has no source either way. This is
    /// what a rule reads when the argument's *spelling* is what matters and its
    /// rendering would destroy it: a table's column specification, where `p{3cm}`
    /// renders as `p3cm` and the `c` would silently invent a centred column.
    pub fn argument_source(&self, name: &str) -> Option<String> {
        self.tree.argument_source(self.id, name)
    }
}

impl core::fmt::Debug for NodeView<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NodeView")
            .field("id", &self.id)
            .field("name", &self.name())
            .finish_non_exhaustive()
    }
}

/// Push a node's children onto the walk's stack, rightmost first, so that popping
/// yields them left to right.
fn push_children_reversed(tree: &dyn TreeView, id: NodeId, stack: &mut Vec<NodeId>) {
    for index in (0..tree.child_count(id)).rev() {
        if let Some(child) = tree.child(id, index) {
            stack.push(child);
        }
    }
}

/// [`NodeView::descendants`]'s walk.
struct Descendants<'t> {
    tree: &'t dyn TreeView,
    /// Nodes not yet yielded, next on top.
    stack: Vec<NodeId>,
}

impl<'t> Iterator for Descendants<'t> {
    type Item = NodeView<'t>;

    fn next(&mut self) -> Option<NodeView<'t>> {
        let id = self.stack.pop()?;
        push_children_reversed(self.tree, id, &mut self.stack);
        Some(NodeView {
            tree: self.tree,
            id,
        })
    }
}
