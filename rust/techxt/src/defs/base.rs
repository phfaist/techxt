//! The base category: escapes, spacing, ligatures, and the small text macros every
//! document uses (PLAN.md §9.3, §9.7, and §9.8's "Misc").
//!
//! This is the category that makes plain LaTeX *text* work — everything that is not a
//! symbol, an accent, a font, a heading, a list or a formula. Three groups of it are
//! worth reading about before the table:
//!
//! # Spacing is emitted as intent, not as characters
//!
//! `\,`, `\;`, `\:` and the control space all mean "one inter-word space here", so they
//! render as [`Glue`](crate::flow::FlowItem::Glue) — collapsible, and a place the
//! layout engine may break a line. `\quad` and `\qquad` mean a *measured* gap, so they
//! render as text: one and two EM SPACEs (U+2003), which is what a quad and a qquad
//! literally are and which no line break may fall inside. `~` is the same idea in
//! reverse — a space that must **not** break — and so is a no-break space character
//! rather than glue.
//!
//! # Three constructs read the render state
//!
//! `&`, `\\` and `\hline` mean different things in different places (PLAN.md §9.7), and
//! what they mean is decided when they are *rendered*, from
//! [`RenderState`](crate::render::RenderState), not when they are parsed. Their
//! definitions live here because they are ordinary base-language constructs; the table
//! and matrix handlers that give them their table meanings live in
//! [`defs::tables`](super::tables) and [`defs::mathenvs`](super::mathenvs).
//!
//! # Footnotes are collected, not inlined
//!
//! `\footnote` renders as a marker at the call site and puts its text at the end of
//! the document (PLAN.md §9.8). The renderer owns the collection — the numbering has to
//! be in document order across the whole run, which no single construct can see — so
//! the definition here does one thing: hand the note over and emit the number it was
//! given.
//!
//! # A curated minimum of text symbols
//!
//! The classic text symbols — `\l`, `\ss`, `\ldots`, `\S` and friends — are here rather
//! than in the generated [`symbols_extra`](super::symbols_extra) long tail, because
//! text as ordinary as `Sk\l odowska` must convert correctly with `base` alone. They
//! shadow the generated table, which is pushed first (PLAN.md §12.1).

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::sync::Arc;

use techy::core::constructs::OptionalGroupArgumentParser;
use techy::core::node::NodeRef;
use techy::core::specs::ArgumentSpec;
use techy::core::token::GroupRule;
use techy::core::ParsingStateDelta;
use techy::latexlike::{Event, GroupType, Mode};
use techy_xp::lang::LatexlikeXp;

use crate::convert::FootnoteStyle;
use crate::def::{Category, MacroDef, SpecialsDef, TextHandler, TextRule};
use crate::diag::{MisplacedAlignment, UnsupportedIgnored};
use crate::flow::{Flow, FlowItem};
use crate::layout::render_inline;
use crate::mathfmt::{fmt_script_text, ScriptKind};
use crate::render::{math, MathCtx, RenderCx, RenderError};

use super::handler;

/// The base category (PLAN.md §12.1): escapes, spacing, ligatures, `~`, `\\`, `\par`,
/// and the miscellaneous text macros of PLAN.md §9.8.
pub fn category() -> Category {
    let mut category = Category::new("base");
    escapes(&mut category);
    spacing(&mut category);
    breaks(&mut category);
    ligatures(&mut category);
    footnotes(&mut category);
    misc(&mut category);
    boxes(&mut category);
    typesetting_only(&mut category);
    text_symbols(&mut category);
    category
}

// ------------------------------------------------------------------------- escapes

/// `\{ \} \$ \& \# \_ \%`: the seven characters LaTeX reserves for itself.
///
/// Each is a macro whose *name* is the character, which is how techy registers a
/// single-character command, and each renders as that character.
fn escapes(category: &mut Category) {
    for character in ["{", "}", "$", "&", "#", "_", "%"] {
        category.add_macro(MacroDef::symbol(character, character));
    }
}

// ------------------------------------------------------------------------- spacing

/// The spacing macros of PLAN.md §9.3.
fn spacing(category: &mut Category) {
    // One inter-word space, and a place a line may break. `Literal(" ")` is not a
    // shortcut: rule text goes through the same word/space splitting as document text,
    // so a lone space *is* exactly one glue.
    for name in [",", ";", ":", " ", "\n"] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Literal(Cow::Borrowed(" "))));
    }
    // A negative thin space has no plain-text meaning at all.
    category.add_macro(MacroDef::new("!").rule(TextRule::Skip));
    // A measured gap, unlike the glue above: emitted as text so that it survives
    // exactly as written and no line break falls inside it.
    //
    // U+2003 EM SPACE, not a run of ASCII spaces (DECISIONS.md D14). A quad *is* one em
    // and a qquad two, so this is the typographically exact character; and it keeps
    // `FlowItem::Text`'s contract, which is that a text item holds no whitespace. An
    // ASCII-space item would put real trailing whitespace at the end of a line, which
    // the layout engine never trims because a text item is content.
    category.add_macro(MacroDef::new("quad").rule(handler(FixedText("\u{2003}"))));
    category.add_macro(MacroDef::new("qquad").rule(handler(FixedText("\u{2003}\u{2003}"))));
    // `~` binds the words on either side of it into one, which in the flow model is
    // simply what a text item does.
    category.add_specials(SpecialsDef::new("~").rule(handler(FixedText("\u{a0}"))));
}

// -------------------------------------------------------------------------- breaks

/// Paragraph and line breaks, and the two constructs whose meaning depends on where
/// they occur (PLAN.md §9.7).
fn breaks(category: &mut Category) {
    category.add_macro(MacroDef::new("par").rule(handler(Emit(FlowItem::ParagraphBreak))));
    category.add_macro(MacroDef::new("newline").rule(handler(Emit(FlowItem::HardBreak))));
    // DECISIONS.md D8: `\\` takes its optional `[len]`, parsed with the adjacency
    // requirement, and ignores it. Without the requirement `A=0 \\ [C,D]=0` would read
    // `[C,D]` as the length and swallow it.
    category.add_macro(
        MacroDef::new("\\")
            .star()
            .arg_spec(optional_length())
            .rule(handler(LineBreak)),
    );
    category.add_specials(SpecialsDef::new("&").rule(handler(Alignment)));
    category.add_macro(MacroDef::new("hline").rule(handler(HorizontalRule)));

    // Page breaks. Plain text has no pages, so the strongest break it can show is a
    // paragraph break — which is also what a reader wants to see where a new page
    // began. The optional `[0-4]` of `\pagebreak` is a strength nothing can render.
    for name in ["newpage", "clearpage", "cleardoublepage"] {
        category.add_macro(MacroDef::new(name).rule(handler(Emit(FlowItem::ParagraphBreak))));
    }
    category.add_macro(
        MacroDef::new("pagebreak")
            .star()
            .arg("o", "strength")
            .rule(handler(Emit(FlowItem::ParagraphBreak))),
    );
    // `\linebreak` is `\newline` with a strength; `\nopagebreak` asks for no break at
    // all, which is what plain text does anyway unless something asks otherwise.
    category.add_macro(
        MacroDef::new("linebreak")
            .arg("o", "strength")
            .rule(handler(Emit(FlowItem::HardBreak))),
    );
    category.add_macro(
        MacroDef::new("nopagebreak")
            .arg("o", "strength")
            .rule(TextRule::Skip),
    );
}

/// The `[len]` argument of `\\`: optional, and only if it comes *immediately*.
///
/// The adjacency requirement is what techy's
/// [`require_adjacent`](OptionalGroupArgumentParser::require_adjacent) provides
/// (pylatexenc's `allow_pre_space=False`), and it is the whole reason this argument
/// cannot be written as the `o` code.
fn optional_length() -> ArgumentSpec<LatexlikeXp> {
    let delimiters = Arc::new(GroupRule {
        group_type: GroupType::Content,
        open: String::from("["),
        close: String::from("]"),
    });
    ArgumentSpec::new(
        OptionalGroupArgumentParser::new(delimiters).require_adjacent(),
        "length",
    )
}

// ----------------------------------------------------------------------- ligatures

/// TeX's input ligatures, in text mode only (PLAN.md §9.3).
///
/// Mode-restricted because they are a property of the *text* font: `--` in a formula is
/// two minus signs, and `'` is a prime. techy matches the longest registered trigger,
/// so `---` wins over `--` and `''` over `'` without any ordering rule here.
fn ligatures(category: &mut Category) {
    for (trigger, replacement) in [
        ("``", "\u{201c}"),
        ("''", "\u{201d}"),
        ("`", "\u{2018}"),
        ("'", "\u{2019}"),
        ("---", "\u{2014}"),
        ("--", "\u{2013}"),
        ("!`", "\u{a1}"),
        ("?`", "\u{bf}"),
    ] {
        category.add_specials(
            SpecialsDef::new(trigger)
                .text_mode_only()
                .rule(TextRule::Literal(Cow::Borrowed(replacement))),
        );
    }
}

// ----------------------------------------------------------------------- footnotes

/// `\footnote` (PLAN.md §9.8).
///
/// The optional argument is LaTeX's own way of overriding the mark; techxt numbers its
/// notes itself, in document order, so it is parsed and dropped rather than used — a
/// hand-written mark would collide with the numbers of the notes around it.
fn footnotes(category: &mut Category) {
    category.add_macro(
        MacroDef::new("footnote")
            .arg("o", "mark")
            .arg("m", "note")
            .rule(handler(Footnote)),
    );
}

// ---------------------------------------------------------------------------- misc

/// PLAN.md §9.8's "Misc": the text macros that are neither symbols nor structure.
fn misc(category: &mut Category) {
    category.add_macro(
        MacroDef::new("texorpdfstring")
            .arg("m", "tex")
            .arg("m", "pdf")
            .rule(TextRule::Template(crate::def::Template::new("{tex}"))),
    );

    // Space reserved but not filled: nothing to show, and the argument must still be
    // parsed so that its contents do not leak into the text.
    for name in ["phantom", "hphantom", "vphantom"] {
        category.add_macro(MacroDef::new(name).arg("m", "content").rule(TextRule::Skip));
    }
    category.add_macro(
        MacroDef::new("hspace")
            .star()
            .arg("m", "length")
            .rule(TextRule::Skip),
    );
    category.add_macro(
        MacroDef::new("vspace")
            .star()
            .arg("m", "length")
            .rule(handler(Emit(FlowItem::ParagraphBreak))),
    );
    for name in ["smallskip", "medskip", "bigskip"] {
        category.add_macro(MacroDef::new(name).rule(handler(Emit(FlowItem::ParagraphBreak))));
    }
    for name in ["noindent", "centering", "raggedright", "raggedleft"] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }

    // `\text{…}` and `\mbox{…}` change the *mode* and nothing else — the font style in
    // force is deliberately left alone, which is what makes the text inside
    // `\textit{… $\mathbf{a + \text{b}}$}` italic again. Leaving math is an event on
    // the parse side and a state change on the render side, and both are needed:
    // the first is what makes `---` and `~` parse as text constructs in there.
    for name in ["text", "mbox"] {
        category.add_macro(
            MacroDef::new(name)
                .arg_spec(exiting_math("text"))
                .rule(handler(ModeShift { math: false })),
        );
    }
    category.add_macro(
        MacroDef::new("ensuremath")
            .arg_spec(entering_math("content"))
            .rule(handler(ModeShift { math: true })),
    );

    // Decorations drawn over or under an expression. Plain text can show none of them,
    // and dropping the decoration is far better than dropping what it decorates.
    for name in [
        "overline",
        "underline",
        "widehat",
        "widetilde",
        "wideparen",
        "overleftarrow",
        "overrightarrow",
        "overleftrightarrow",
        "underleftarrow",
        "underrightarrow",
        "underleftrightarrow",
        "overbrace",
        "underbrace",
        "overgroup",
        "undergroup",
        "overbracket",
        "underbracket",
        "overlinesegment",
        "underlinesegment",
        "overleftharpoon",
        "overrightharpoon",
    ] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", "content")
                .rule(TextRule::Content),
        );
    }
}

// --------------------------------------------------------------------------- boxes

/// The boxes and the case-changing macros of PLAN.md §9.8's "Misc".
///
/// A box is a *typesetting* instruction — fix this width, draw a frame, lift this off
/// the baseline — and plain text can show none of it. What is inside one is ordinary
/// content, though, and losing that would lose real text, so every box here renders
/// its contents and nothing else, with the measurements parsed and discarded so that
/// they cannot leak into the reader's paragraph.
fn boxes(category: &mut Category) {
    category.add_macro(
        MacroDef::new("fbox")
            .arg("m", CONTENT)
            .rule(TextRule::Content),
    );
    category.add_macro(
        MacroDef::new("boxed")
            .arg("m", CONTENT)
            .rule(TextRule::Content),
    );
    for name in ["framebox", "makebox"] {
        category.add_macro(
            MacroDef::new(name)
                .arg("o", "width")
                .arg("o", "position")
                .arg("m", CONTENT)
                .rule(only_content()),
        );
    }
    category.add_macro(
        MacroDef::new("parbox")
            .arg("o", "position")
            .arg("o", "height")
            .arg("o", "inner")
            .arg("m", "width")
            .arg("m", CONTENT)
            .rule(only_content()),
    );
    category.add_macro(
        MacroDef::new("raisebox")
            .arg("m", "raise")
            .arg("o", "height")
            .arg("o", "depth")
            .arg("m", CONTENT)
            .rule(only_content()),
    );
    category.add_macro(
        MacroDef::new("smash")
            .arg("o", "part")
            .arg("m", CONTENT)
            .rule(only_content()),
    );
    // A rule is a black rectangle: nothing but geometry, and nothing to render.
    category.add_macro(
        MacroDef::new("rule")
            .arg("o", "raise")
            .arg("m", "width")
            .arg("m", "height")
            .rule(TextRule::Skip),
    );

    // Case is content (PLAN.md §9.2): techxt never changes a letter's case, here no
    // more than in a heading. The argument is rendered as it was written — which also
    // keeps `\MakeUppercase{\ss}` from producing a letter its author did not write.
    for name in ["uppercase", "lowercase", "MakeUppercase", "MakeLowercase"] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", CONTENT)
                .rule(TextRule::Content),
        );
    }

    // Text-mode scripts. Unicode has script characters for some strings and not for
    // others, so the rendering is the same all-or-nothing lookup a math script runs
    // ([`fmt_script_text`](crate::mathfmt::fmt_script_text)) — `1\textsuperscript{st}`
    // becomes `1ˢᵗ` — and what unicode cannot show is rendered as the plain text it
    // was, which is still the right words in the right order.
    category.add_macro(
        MacroDef::new("textsuperscript")
            .arg("m", CONTENT)
            .rule(handler(TextScript(ScriptKind::Superscript))),
    );
    category.add_macro(
        MacroDef::new("textsubscript")
            .arg("m", CONTENT)
            .rule(handler(TextScript(ScriptKind::Subscript))),
    );
}

/// The argument name every box-like entry gives the part that is actually rendered.
const CONTENT: &str = "content";

/// The rule "render the `content` argument and nothing else".
///
/// [`TextRule::Content`] renders *every* provided argument, which for a box would put
/// its width in the text; a template names the one argument that is content.
fn only_content() -> TextRule {
    TextRule::Template(crate::def::Template::new("{content}"))
}

// ---------------------------------------------------------- typesetting-only macros

/// The commands that instruct the typesetter and say nothing to the reader.
///
/// Every one of them is *declared* rather than left unknown, and that is the point: an
/// unknown command takes no arguments (see
/// [`Options::unknown_macro`](crate::Options::unknown_macro)), so an undeclared
/// `\settowidth{\x}{ab}` would print `ab`, and an undeclared `\hspace*{\fill}` would
/// print nothing but warn about a construct techxt does in fact understand. Declaring
/// them with [`TextRule::Skip`] is the complete and correct answer, and it is silent
/// on purpose (`techxt.unknown-macro` means "techxt does not know this", not "this
/// renders as nothing").
///
/// # Counters are not tracked
///
/// `\arabic{section}` and its siblings would print a counter's value, and techxt keeps
/// no counters other than the heading numbers of [`defs::sectioning`](super::sectioning)
/// — document numbering is on PLAN.md §17's list — so they render as nothing rather
/// than as a wrong number.
fn typesetting_only(category: &mut Category) {
    for name in [
        // Glue and leaders: horizontal or vertical space, filled or not.
        "hfill",
        "vfill",
        "hrulefill",
        "dotfill",
        "smallskipamount",
        "medskipamount",
        "bigskipamount",
        // Do-nothing primitives that documents genuinely contain.
        "null",
        "relax",
        "protect",
        "allowbreak",
        "indent",
        "unskip",
        // Line-breaking policy, and the italic correction — adjustments to *how* the
        // same characters are set. (The discretionary hyphen `\-` is not here: the
        // generated table renders it as U+00AD SOFT HYPHEN, which is invisible and
        // marks the break it permits.)
        "sloppy",
        "fussy",
        "/",
        // `\makeatletter` … `\makeatother` change what counts as a letter in a command
        // name; techy's parser reads `@` as part of a name regardless.
        "makeatletter",
        "makeatother",
        // The standard lengths, which appear as the values of the arguments above.
        "textwidth",
        "linewidth",
        "columnwidth",
        "textheight",
        "paperwidth",
        "paperheight",
        "baselineskip",
        "parindent",
        "parskip",
        "fill",
    ] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }
    // Counter commands: the ones that change a counter, and the ones that would print
    // one. See this function's documentation for why printing one renders nothing.
    for name in [
        "stepcounter",
        "refstepcounter",
        "arabic",
        "roman",
        "Roman",
        "alph",
        "Alph",
        "fnsymbol",
        "value",
    ] {
        category.add_macro(MacroDef::new(name).arg("m", "counter").rule(TextRule::Skip));
    }
}

/// An argument parsed with math left behind (`\text{…}`).
///
/// The exit is an [`Event`](techy::latexlike::Event) rather than a mode assignment
/// (techy restores the enclosing non-math context, rules and all), and it is harmless
/// outside math, where the enclosing context is the one being restored.
fn exiting_math(name: &str) -> ArgumentSpec<LatexlikeXp> {
    ArgumentSpec::new(
        techy::core::constructs::GroupArgumentParser::new(GroupType::Content),
        name,
    )
    .with_state_delta(ParsingStateDelta::new().event(Event::ExitMathContext))
}

/// An argument parsed as math (`\ensuremath{…}`).
fn entering_math(name: &str) -> ArgumentSpec<LatexlikeXp> {
    ArgumentSpec::new(
        techy::core::constructs::GroupArgumentParser::new(GroupType::Content),
        name,
    )
    .with_state_delta(ParsingStateDelta::new().mode(Mode::Math))
}

// ------------------------------------------------------------------- text symbols

/// The classic text symbols (see the module documentation for why they are here).
fn text_symbols(category: &mut Category) {
    for (name, replacement) in [
        // letters no ASCII keyboard has
        ("l", "\u{142}"),
        ("L", "\u{141}"),
        ("o", "\u{f8}"),
        ("O", "\u{d8}"),
        ("i", "\u{131}"),
        ("j", "\u{237}"),
        ("ss", "\u{df}"),
        ("SS", "\u{1e9e}"),
        ("aa", "\u{e5}"),
        ("AA", "\u{c5}"),
        ("ae", "\u{e6}"),
        ("AE", "\u{c6}"),
        ("oe", "\u{153}"),
        ("OE", "\u{152}"),
        ("dh", "\u{f0}"),
        ("DH", "\u{d0}"),
        ("th", "\u{fe}"),
        ("TH", "\u{de}"),
        ("dj", "\u{111}"),
        ("DJ", "\u{110}"),
        ("ng", "\u{14b}"),
        ("NG", "\u{14a}"),
        // punctuation and marks
        ("dots", "\u{2026}"),
        ("ldots", "\u{2026}"),
        ("textellipsis", "\u{2026}"),
        ("S", "\u{a7}"),
        ("P", "\u{b6}"),
        ("dag", "\u{2020}"),
        ("ddag", "\u{2021}"),
        ("textdagger", "\u{2020}"),
        ("textdaggerdbl", "\u{2021}"),
        ("copyright", "\u{a9}"),
        ("textcopyright", "\u{a9}"),
        ("pounds", "\u{a3}"),
        ("textsterling", "\u{a3}"),
        ("texteuro", "\u{20ac}"),
        ("textdegree", "\u{b0}"),
        ("textregistered", "\u{ae}"),
        ("texttrademark", "\u{2122}"),
        ("textbullet", "\u{2022}"),
        ("textperiodcentered", "\u{b7}"),
        // characters that would otherwise have to be escaped
        ("textbackslash", "\\"),
        ("textasciitilde", "~"),
        ("textasciicircum", "^"),
        ("textbar", "|"),
        ("textless", "<"),
        ("textgreater", ">"),
        ("textunderscore", "_"),
        ("textbraceleft", "{"),
        ("textbraceright", "}"),
        // the quotation marks and dashes the ligatures also produce
        ("textquoteleft", "\u{2018}"),
        ("textquoteright", "\u{2019}"),
        ("textquotedblleft", "\u{201c}"),
        ("textquotedblright", "\u{201d}"),
        ("textendash", "\u{2013}"),
        ("textemdash", "\u{2014}"),
        ("guillemotleft", "\u{ab}"),
        ("guillemotright", "\u{bb}"),
        // logos
        ("TeX", "TeX"),
        ("LaTeX", "LaTeX"),
        ("LaTeXe", "LaTeX2e"),
    ] {
        category.add_macro(MacroDef::symbol(name, replacement));
    }
}

// ------------------------------------------------------------------------ handlers

/// Text that must survive exactly as written, as one unbreakable item.
///
/// The difference from [`TextRule::Literal`] is the whole point: a literal's text is
/// split into words and inter-word spaces, so `Literal("  ")` would collapse to one
/// breakable space, while `\quad` means a gap of a definite size.
#[derive(Debug)]
struct FixedText(&'static str);

impl TextHandler for FixedText {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        _cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        Ok(Flow::text(self.0))
    }
}

/// `\textsuperscript{…}` and `\textsubscript{…}`.
///
/// The argument is rendered first — it is ordinary text, and may hold a macro of its
/// own — then looked up in unicode's script characters as a whole. A string unicode
/// cannot show in a script renders as itself: the alternative, `^`/`_` notation, reads
/// as mathematics in the middle of a sentence, which running text is.
#[derive(Debug)]
struct TextScript(ScriptKind);

impl TextHandler for TextScript {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let argument = cx.arg(CONTENT)?.unwrap_or_default();
        let text = render_inline(&argument);
        match fmt_script_text(&text, self.0) {
            // One text item: a script is a single token, and no line break may fall
            // inside it.
            Some(script) => Ok(Flow::text(&script)),
            None => Ok(argument),
        }
    }
}

/// One flow item, whatever it is: the whole rendering of `\par` and friends.
#[derive(Debug)]
struct Emit(FlowItem);

impl TextHandler for Emit {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        _cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut flow = Flow::new();
        flow.push(self.0.clone());
        Ok(flow)
    }
}

/// `\\` (PLAN.md §9.7).
///
/// Four meanings, decided by where it occurs: a row separator in a table body or a
/// matrix, a new display line in a multi-line formula, nothing much in inline math, and
/// a forced line break in running text. The optional `[len]` is parsed (so that it
/// cannot leak into the text) and ignored — plain text has no vertical measure.
#[derive(Debug)]
struct LineBreak;

impl TextHandler for LineBreak {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let state = cx.state();
        let mut flow = Flow::new();
        match (state.table.is_some(), state.math) {
            (true, _) => flow.push(FlowItem::RowSep),
            (false, Some(MathCtx { matrix: true, .. })) => flow.push(FlowItem::RowSep),
            // A line break in a formula: a new display line, or — inline, where there
            // are no lines to break — merely a space.
            (false, Some(MathCtx { display, .. })) => flow.push(if display {
                FlowItem::HardBreak
            } else {
                FlowItem::Glue
            }),
            (false, None) => flow.push(FlowItem::HardBreak),
        }
        Ok(flow)
    }
}

/// `&` (PLAN.md §9.7).
///
/// A cell separator where there are cells, nothing in a formula that has none (the
/// joiner spaces the relation the `&` was aligning), and a reported mistake in running
/// text — where it is still rendered as a space, because the words on either side of it
/// were meant to be separate.
#[derive(Debug)]
struct Alignment;

impl TextHandler for Alignment {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let state = cx.state();
        let mut flow = Flow::new();
        match (state.table.is_some(), state.math) {
            (true, _) => flow.push(FlowItem::CellSep),
            (false, Some(MathCtx { matrix: true, .. })) => flow.push(FlowItem::CellSep),
            (false, Some(_)) => {}
            (false, None) => {
                cx.report(MisplacedAlignment);
                flow.push(FlowItem::Glue);
            }
        }
        Ok(flow)
    }
}

/// `\hline` (PLAN.md §9.7): a rule marker inside a table, and nothing anywhere else.
#[derive(Debug)]
struct HorizontalRule;

impl TextHandler for HorizontalRule {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let mut flow = Flow::new();
        if cx.state().table.is_some() {
            flow.push(FlowItem::RuleMark);
        } else {
            cx.report(UnsupportedIgnored::new(
                "\\hline",
                "a horizontal rule outside a table",
            ));
        }
        Ok(flow)
    }
}

/// `\footnote{…}` (PLAN.md §9.8).
///
/// Three [styles](crate::convert::FootnoteStyle), and the default collects: the marker
/// goes where the note was written and the text goes after the document, which is what
/// a footnote *is*. The marker is emitted as a text item and nothing precedes it, so it
/// glues to the word it annotates — `Fact[1] holds.`, never `Fact [1] holds.`
#[derive(Debug)]
struct Footnote;

impl TextHandler for Footnote {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        match cx.options().footnote_style {
            FootnoteStyle::Collected => {
                let note = cx.arg("note")?.unwrap_or_default();
                // The number is the collection index, assigned as the note is folded —
                // which is document order, because the fold is.
                let number = cx.push_footnote(note);
                Ok(Flow::text(&alloc::format!("[{number}]")))
            }
            FootnoteStyle::Inline => {
                let mut flow = Flow::text("[");
                flow.extend(cx.arg("note")?.unwrap_or_default());
                flow.extend(Flow::text("]"));
                Ok(flow)
            }
            // Nothing is emitted and nothing is collected: the note is not rendered at
            // all, so a footnote's own side effects do not happen either.
            FootnoteStyle::Skip => Ok(Flow::new()),
        }
    }
}

/// `\text{…}`, `\mbox{…}` and `\ensuremath{…}`: render the argument in the other mode.
///
/// The font style is deliberately untouched — these macros change what the content
/// *is*, not what it looks like.
#[derive(Debug)]
struct ModeShift {
    /// Whether the argument is rendered as math.
    math: bool,
}

impl TextHandler for ModeShift {
    fn render(
        &self,
        node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // An `\ensuremath` written outside a formula *opens* one, so it answers `Source`
        // mode the way `$…$` and `\begin{equation}` do: re-emitted, argument and all,
        // rather than rendered. `\text` never opens one, and one written inside a
        // formula is not reached in that mode at all.
        if self.math && cx.state().math.is_none() {
            if let Some(flow) = math::source_scope(node, false, cx.options()) {
                return Ok(flow);
            }
        }

        let mut state = cx.state().clone();
        state.math = self.math.then_some(MathCtx {
            // An `\ensuremath` is inline by construction: it is what a macro writes so
            // that its body works in running text as well as in a formula.
            display: false,
            matrix: false,
        });
        let name = if self.math { "content" } else { "text" };
        Ok(cx.arg_with_state(name, state)?.unwrap_or_default())
    }
}
