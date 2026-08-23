//! Theorem-like environments, and the other blocks that shape a body (PLAN.md §9.8).
//!
//! `theorem`, `proposition`, `lemma`, `corollary`, `definition`, `conjecture`,
//! `remark`, `example` and `proof`, with their short aliases, plus the block
//! environments they sit among: `center`, `flushleft`, `flushright`, `quote`,
//! `quotation`, `verse`, `abstract`, `figure`, `table` and `\caption`.
//!
//! # What a block environment does, and what it deliberately does not
//!
//! Each of these environments does one of three things: start a block of its own
//! (`center` and the other alignment environments), start an *indented* block
//! (`quote`, `quotation`, `verse`, and the body of an `abstract`), or set a piece of
//! state its contents can read (`figure` and `table`, which tell `\caption` what to
//! call itself).
//!
//! Centring is **not** simulated. Padding lines with spaces to fake a centred paragraph
//! would misalign the moment the reader's window is a different width than techxt
//! guessed, and it would put trailing whitespace on every line; `center` therefore
//! renders as an ordinary block. Nothing is numbered either: a theorem says `Theorem.`
//! and not `Theorem 2.4.`, because numbering means resolving counters that a document
//! can redefine (PLAN.md §17).
//!
//! # The label is generated text
//!
//! `Theorem`, `Proof` and `Abstract` are English words techxt writes itself, from the
//! fixed alias table below — the environment's own name is a keyword, not a label, and
//! `\begin{thm}` must read as `Theorem.` and not as `thm.`. Localizing them is future
//! work (PLAN.md §17); a document that needs other words overrides the rule.

use alloc::string::String;

use techy::core::node::NodeRef;
use techy_xp::lang::LatexlikeXp;

use crate::def::{Category, EnvDef, MacroDef, TextHandler, TextRule};
use crate::flow::{BlockKind, Flow, FlowItem};
use crate::layout::render_inline;
use crate::render::{FloatKind, RenderCx, RenderError};

use super::handler;

/// The indent a quotation and an abstract's body are set in.
const QUOTE_INDENT: &str = "    ";

/// The end-of-proof mark appended to a `proof` body (U+25A1 WHITE SQUARE).
const QED: &str = "\u{25a1}";

/// The environments that render as a theorem, and the word each is called by.
///
/// The short aliases are the ones `\newtheorem{thm}{Theorem}` conventionally declares;
/// techxt cannot read that declaration (it discards `\newtheorem`'s arguments, PLAN.md
/// §9.8), so the common spellings are simply known.
const THEOREM_ENVIRONMENTS: [(&str, &str); 15] = [
    ("theorem", "Theorem"),
    ("proposition", "Proposition"),
    ("lemma", "Lemma"),
    ("corollary", "Corollary"),
    ("definition", "Definition"),
    ("conjecture", "Conjecture"),
    ("remark", "Remark"),
    ("example", "Example"),
    ("thm", "Theorem"),
    ("prop", "Proposition"),
    ("lem", "Lemma"),
    ("cor", "Corollary"),
    ("defn", "Definition"),
    ("conj", "Conjecture"),
    ("rem", "Remark"),
];

/// The theorems category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("theorems");
    blocks(&mut category);
    floats(&mut category);
    theorems(&mut category);
    category
}

/// The environments that only change the shape of their body.
fn blocks(category: &mut Category) {
    // Alignment: a block of its own, no indent, no centring simulation.
    for name in ["center", "flushleft", "flushright"] {
        category.add_env(EnvDef::new(name).rule(handler(Block { indent: "" })));
    }
    // Quotations: set in from the margin, which is what a reader of plain text reads as
    // "this is quoted".
    for name in ["quote", "quotation", "verse"] {
        category.add_env(EnvDef::new(name).rule(handler(Block {
            indent: QUOTE_INDENT,
        })));
    }
    // A `minipage` is a box set as a small page of its own, and a `titlepage` is a
    // page; both are blocks of text with no shape plain text can show, and both keep
    // every word inside them. `\begin{minipage}[t]{0.4\textwidth}`'s position and width
    // are parsed so that they cannot leak.
    category.add_env(
        EnvDef::new("minipage")
            .arg("o", "position")
            .arg("o", "height")
            .arg("o", "inner")
            .arg("m", "width")
            .rule(handler(Block { indent: "" })),
    );
    category.add_env(EnvDef::new("titlepage").rule(handler(Block { indent: "" })));
    // `multicols` (the `multicol` package) sets its body in columns, and `spacing`
    // (`setspace`) sets it at a different leading. Plain text has one column and one
    // line spacing; both bodies are ordinary text.
    category.add_env(
        EnvDef::new("multicols")
            .arg("m", "columns")
            .arg("o", "heading")
            .rule(handler(Block { indent: "" })),
    );
    category.add_env(
        EnvDef::new("spacing")
            .arg("m", "factor")
            .rule(TextRule::Content),
    );
    // `sloppypar` only changes how its paragraphs are broken into lines, which is the
    // layout engine's decision here; its body is ordinary text.
    category.add_env(EnvDef::new("sloppypar").rule(TextRule::Content));
    category.add_env(EnvDef::new("abstract").rule(handler(Abstract)));
}

/// The floats, and the caption that reads their state.
fn floats(category: &mut Category) {
    for (name, kind) in [
        ("figure", FloatKind::Figure),
        ("figure*", FloatKind::Figure),
        ("table", FloatKind::Table),
        ("table*", FloatKind::Table),
    ] {
        // The `[htbp]` placement says where the float should go on a *page*; text has
        // no pages, and the float renders where it was written.
        category.add_env(
            EnvDef::new(name)
                .arg("o", "placement")
                .rule(handler(Float { kind })),
        );
    }
    // The optional short caption is the list-of-figures entry; plain text has no such
    // list, so it is parsed and dropped.
    category.add_macro(
        MacroDef::new("caption")
            .star()
            .arg("o", "short")
            .arg("m", "text")
            .rule(handler(Caption)),
    );
}

/// The theorem-like environments.
fn theorems(category: &mut Category) {
    for (name, label) in THEOREM_ENVIRONMENTS {
        category.add_env(
            EnvDef::new(name)
                .arg("o", "note")
                .rule(handler(Theorem { label, qed: false })),
        );
    }
    category.add_env(EnvDef::new("proof").arg("o", "note").rule(handler(Theorem {
        label: "Proof",
        qed: true,
    })));
}

// --------------------------------------------------------------------------- handlers

/// An environment whose body becomes a block, optionally indented.
#[derive(Debug)]
struct Block {
    /// The prefix every line of the block gets, `""` for none.
    indent: &'static str,
}

impl TextHandler for Block {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        Ok(indented(cx.body()?, self.indent))
    }
}

/// `abstract`: the word, then the body set in from the margin.
#[derive(Debug)]
struct Abstract;

impl TextHandler for Abstract {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let body = cx.body()?;
        let mut flow = Flow::new();
        flow.push(FlowItem::ParagraphBreak);
        flow.push(FlowItem::Text("Abstract".into()));
        flow.push(FlowItem::ParagraphBreak);
        flow.extend(indented(body, QUOTE_INDENT));
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}

/// `figure` and `table`: the body, rendered knowing which float it is in.
#[derive(Debug)]
struct Float {
    /// What a `\caption` inside this float calls itself.
    kind: FloatKind,
}

impl TextHandler for Float {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut state = cx.state().clone();
        state.float = Some(self.kind);
        cx.body_with_state(state)
    }
}

/// `\caption`: a paragraph of its own, named after the float it is in.
#[derive(Debug)]
struct Caption;

impl TextHandler for Caption {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // A caption outside any float still has something to say; it just cannot say
        // what it is a caption *of*.
        let label = match cx.state().float {
            Some(FloatKind::Figure) => "Figure:",
            Some(FloatKind::Table) => "Table:",
            None => "Caption:",
        };
        let text = cx.arg("text")?.unwrap_or_default();

        let mut flow = Flow::new();
        flow.push(FlowItem::ParagraphBreak);
        flow.push(FlowItem::Text(label.into()));
        flow.push(FlowItem::Glue);
        flow.extend(text);
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}

/// A theorem-like environment: a label, an optional note, and the body after it.
#[derive(Debug)]
struct Theorem {
    /// The word this environment is called by (`Theorem`, `Proof`).
    label: &'static str,
    /// Whether the body ends with an end-of-proof mark.
    qed: bool,
}

impl TextHandler for Theorem {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // The note is rendered before the body, which is the order it is written in and
        // therefore the order its own side effects (a footnote in a theorem's note)
        // must happen in.
        let note = cx
            .arg("note")?
            .filter(|note| !render_inline(note).is_empty());
        let body = cx.body()?;

        let mut flow = Flow::new();
        // Its own paragraph: a theorem is a unit, and running it into the sentence
        // before it would lose exactly the boundary the environment was marking.
        flow.push(FlowItem::ParagraphBreak);
        match note {
            Some(note) => {
                flow.push(FlowItem::Text(self.label.into()));
                flow.push(FlowItem::Glue);
                // The parentheses glue to the note's own first and last words, so a
                // multi-word note still reads as one parenthesis.
                flow.push(FlowItem::Text("(".into()));
                flow.extend(note);
                flow.push(FlowItem::Text(").".into()));
            }
            None => flow.push(FlowItem::Text(alloc::format!("{}.", self.label).into())),
        }
        flow.push(FlowItem::Glue);
        flow.extend(body);
        if self.qed {
            flow.push(FlowItem::Glue);
            flow.push(FlowItem::Text(QED.into()));
        }
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}

/// Wrap a flow in a block, indented by `indent` (which may be empty).
fn indented(body: Flow, indent: &str) -> Flow {
    let first: String = String::from(indent);
    let mut flow = Flow::new();
    flow.push(FlowItem::BlockStart(BlockKind::Indent {
        first: first.clone().into_boxed_str(),
        cont: first.into_boxed_str(),
    }));
    flow.extend(body);
    flow.push(FlowItem::BlockEnd);
    flow
}
