//! The font category: `\textbf`, `\mathbb`, `\emph`, `\operatorname` (PLAN.md §9.5).
//!
//! Unicode has thirteen mathematical alphabets, and techxt uses them to show a font
//! change in plain text: `\textbf{bold}` really does come out as `𝐛𝐨𝐥𝐝`. What is
//! interesting here is not the mapping — [`mathfmt`](crate::mathfmt) owns that — but
//! *where the style is applied and to what*.
//!
//! # The style goes down, not around
//!
//! A font macro does not transform its argument's output; it renders the argument
//! under a modified [`RenderState`](crate::render::RenderState), so the alphabet is
//! applied at the character leaves. Anything else loses nested styles: an italic `x`
//! has already become `𝑥`, which is outside the range the bold mapping knows, and the
//! bold would silently vanish.
//!
//! And the mapping applies to **document characters only** (DECISIONS.md D6). Text a
//! rule produced — `\sin` → `sin`, `\alpha` → `α` — never passes through an alphabet,
//! so an operator name stays upright next to an italic variable.
//!
//! # Two stacks, and the current mode picks one
//!
//! [`RenderState`](crate::render::RenderState) carries a text font and a math font,
//! derived independently, and a style macro writes to whichever stack the *mode* it
//! occurs in uses. That is what makes `\textit{… $\mathbf{a + \text{b}}$}` typeset the
//! `b` in italics again: `\mathbf` wrote the math stack, and `\text` went back to the
//! text stack, which `\textit` had set and the excursion into math never touched.
//!
//! A math font macro used outside a formula is an error in LaTeX; techxt honours it
//! anyway, against the text stack, because that is the stack governing the text it was
//! written in.
//!
//! # Upright is a style
//!
//! `\textrm`, `\mathrm` and `\operatorname` install
//! [`FontStyleKind::Upright`] (DECISIONS.md
//! D7), which maps letters to themselves but is still a style: a nested `\mathbf`
//! overrides it normally, so `\mathrm{a\mathbf{b}}` renders the `b` in bold.
//!
//! LaTeX varies family, weight and shape independently; unicode's alphabets do not, so
//! each entry here selects a whole font. `\textmd` — which in LaTeX only takes the
//! weight back to medium — therefore reads like `\textrm` and `\textup`, there being no
//! medium-weight alphabet for it to select.

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::def::{Category, MacroDef, TextHandler, TextRule};
use crate::flow::Flow;
use crate::mathfmt::{FontStyle, FontStyleKind};
use crate::render::{RenderCx, RenderError};

use super::handler;

/// The font category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("fontstyles");

    // The text-mode family. Every one of them takes one argument and installs one
    // whole font.
    for (name, kind) in [
        ("textrm", FontStyleKind::Upright),
        ("textup", FontStyleKind::Upright),
        ("textmd", FontStyleKind::Upright),
        ("textnormal", FontStyleKind::Upright),
        ("textbf", FontStyleKind::Bold),
        ("textit", FontStyleKind::Italic),
        // LaTeX's slanted shape is not italic, but unicode has no slanted alphabet.
        ("textsl", FontStyleKind::Italic),
        ("textsf", FontStyleKind::SansSerif),
        ("texttt", FontStyleKind::Monospace),
    ] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", TEXT)
                .rule(handler(SetFont(kind))),
        );
    }
    // Small capitals have no alphabet of their own, and techxt never changes a
    // letter's case (PLAN.md §9.2 says so for headings and the rule holds here too),
    // so the argument is rendered as it stands.
    category.add_macro(
        MacroDef::new("textsc")
            .arg("m", TEXT)
            .rule(TextRule::Content),
    );

    // The math-mode family.
    for (name, kind) in [
        ("mathrm", FontStyleKind::Upright),
        ("mathnormal", FontStyleKind::Italic),
        ("mathbf", FontStyleKind::Bold),
        ("mathit", FontStyleKind::Italic),
        ("mathsf", FontStyleKind::SansSerif),
        ("mathbb", FontStyleKind::DoubleStruck),
        ("mathtt", FontStyleKind::Monospace),
        ("mathcal", FontStyleKind::Script),
        ("mathscr", FontStyleKind::Script),
        ("mathfrak", FontStyleKind::Fraktur),
    ] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", TEXT)
                .rule(handler(SetFont(kind))),
        );
    }

    declarations(&mut category);

    category.add_macro(MacroDef::new("emph").arg("m", TEXT).rule(handler(Emphasis)));
    // `\operatorname*{…}` differs only in where the operator's limits are set, which
    // plain text cannot show either way.
    category.add_macro(
        MacroDef::new("operatorname")
            .star()
            .arg("m", TEXT)
            .rule(handler(SetFont(FontStyleKind::Upright))),
    );

    category
}

/// The font and size *declarations*: `{\bfseries loud}` rather than `\textbf{loud}`.
///
/// A declaration takes no argument — it changes the font for the rest of the group it
/// stands in — and techxt's font state travels *down*, into a construct's arguments,
/// never sideways to a construct's siblings. There is therefore nothing here for a
/// declaration to change, and each of them renders as nothing: the styling is lost and
/// every character of the text is kept, which is the right way round. The argument
/// forms above are what carries a font into the output.
///
/// Sizes are declared for a second reason as well: plain text has one size, so `\huge`
/// could not change anything even if it did reach the characters after it.
fn declarations(category: &mut Category) {
    for name in [
        // LaTeX2e's family, series and shape declarations …
        "rmfamily",
        "sffamily",
        "ttfamily",
        "mdseries",
        "bfseries",
        "upshape",
        "itshape",
        "slshape",
        "scshape",
        "normalfont",
        "em",
        // … and the short forms of plain TeX and LaTeX 2.09 that documents still use.
        "rm",
        "sf",
        "tt",
        "bf",
        "it",
        "sl",
        "sc",
        // The ten standard sizes.
        "tiny",
        "scriptsize",
        "footnotesize",
        "small",
        "normalsize",
        "large",
        "Large",
        "LARGE",
        "huge",
        "Huge",
    ] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }
}

/// The name every entry here gives its argument.
const TEXT: &str = "text";

/// Render the argument with one font alphabet installed.
#[derive(Debug)]
struct SetFont(FontStyleKind);

impl TextHandler for SetFont {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let state = derive_font(cx, |_| FontStyle::Style(self.0));
        Ok(cx.arg_with_state(TEXT, state)?.unwrap_or_default())
    }
}

/// `\emph{…}`: italic where it is not, upright where it is.
///
/// Toggling rather than simply setting italic is what makes it compose: inside a bold
/// context `\emph` gives bold italic, and inside an italic one it gives the upright
/// shape back — which is what LaTeX's `\emph` is for.
#[derive(Debug)]
struct Emphasis;

impl TextHandler for Emphasis {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let state = derive_font(cx, |current| FontStyle::Style(toggle_italic(current)));
        Ok(cx.arg_with_state(TEXT, state)?.unwrap_or_default())
    }
}

/// Derive a state whose *mode-appropriate* font stack is `choose(current effective
/// style)`.
///
/// [`FontStyle::Disabled`] is left alone: it is the option-level off switch, and no
/// macro may turn alphabet mapping back on.
fn derive_font(
    cx: &RenderCx<'_, '_>,
    choose: impl FnOnce(Option<FontStyleKind>) -> FontStyle,
) -> crate::render::RenderState {
    let mut state = cx.state().clone();
    let options = cx.options();
    let (stack, fallback) = if state.in_math() {
        (&mut state.math_font, options.math_font)
    } else {
        (&mut state.text_font, options.text_font)
    };
    if *stack != FontStyle::Disabled {
        *stack = choose(stack.resolve(fallback));
    }
    state
}

/// The style `\emph` switches to, given the one in force (v3's
/// `_emph_toggle_fontstyles`).
///
/// Anything with no italic counterpart — script, fraktur, monospace — emphasizes as
/// plain italic, which is the best unicode can offer.
fn toggle_italic(current: Option<FontStyleKind>) -> FontStyleKind {
    match current {
        None | Some(FontStyleKind::Upright) => FontStyleKind::Italic,
        Some(FontStyleKind::Italic) => FontStyleKind::Upright,
        Some(FontStyleKind::Bold) => FontStyleKind::BoldItalic,
        Some(FontStyleKind::BoldItalic) => FontStyleKind::Bold,
        Some(FontStyleKind::SansSerif) => FontStyleKind::SansSerifItalic,
        Some(FontStyleKind::SansSerifItalic) => FontStyleKind::SansSerif,
        Some(FontStyleKind::SansSerifBold) => FontStyleKind::SansSerifBoldItalic,
        Some(FontStyleKind::SansSerifBoldItalic) => FontStyleKind::SansSerifBold,
        Some(_) => FontStyleKind::Italic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_toggles_rather_than_setting() {
        assert_eq!(toggle_italic(None), FontStyleKind::Italic);
        assert_eq!(
            toggle_italic(Some(FontStyleKind::Italic)),
            FontStyleKind::Upright
        );
        assert_eq!(
            toggle_italic(Some(FontStyleKind::Bold)),
            FontStyleKind::BoldItalic
        );
        assert_eq!(
            toggle_italic(Some(FontStyleKind::BoldItalic)),
            FontStyleKind::Bold
        );
        // No italic counterpart: plain italic is the best there is.
        assert_eq!(
            toggle_italic(Some(FontStyleKind::Fraktur)),
            FontStyleKind::Italic
        );
    }
}
