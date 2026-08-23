//! Sub- and superscripts (PLAN.md §9.5): `^` and `_`.
//!
//! Both are specials taking one expression argument, registered **math mode only**, so
//! that `a_b` in running text stays the characters it was written as. The mode
//! restriction is the parse side of that promise: outside math the trigger is not
//! registered at all, so the `_` never becomes a construct and never swallows a `b`.
//!
//! # Why a script is not simply text
//!
//! Unicode has script characters for some of what LaTeX can put in a script and not for
//! the rest, so a script has to choose its rendering, and the choice changes how it
//! joins to what surrounds it. [`script_atom`] runs that ladder — unicode characters,
//! then a retry with the joiner's own spaces taken back out, then LaTeX's `^`/`_`
//! notation — and marks the atom with what it settled on, which is what lets the joiner
//! put a space in front of the *base* when the notation fell back (`4π𝑐 𝑥^𝑞 𝑝`) and
//! none when it did not (`4π𝑐𝑥²𝑝`).

use techy::core::node::NodeRef;
use techy_xp::lang::LatexlikeXp;

use crate::def::{Category, SpecialsDef, TextHandler, TextRule};
use crate::flow::Flow;
use crate::mathfmt::{script_atom, ScriptKind};
use crate::render::{math, RenderCx, RenderError};

use super::handler;

/// The name both specials give their one argument.
const SCRIPT: &str = "script";

/// The subsuperscripts category (PLAN.md §12.1).
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::standard();
/// assert_eq!(converter.latex_to_text("$x^2 + y_i$")?.text, "𝑥² + 𝑦ᵢ\n");
/// // Outside math the characters are just characters.
/// assert_eq!(converter.latex_to_text("a_b and a^b")?.text, "a_b and a^b\n");
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub fn category() -> Category {
    let mut category = Category::new("subsuperscripts");
    for (trigger, kind) in [("^", ScriptKind::Superscript), ("_", ScriptKind::Subscript)] {
        category.add_specials(
            SpecialsDef::new(trigger)
                .math_mode_only()
                // `m` carries TeX's single-expression fallback, which is exactly the
                // rule a script argument follows: `x^2` takes the one token, `x^{2n}`
                // the whole group.
                .arg("m", SCRIPT)
                .rule(script_rule(kind)),
        );
    }
    category
}

/// The rule one of the two scripts renders through.
fn script_rule(kind: ScriptKind) -> TextRule {
    handler(Script(kind))
}

/// `^{…}` and `_{…}` (PLAN.md §9.5).
#[derive(Debug)]
struct Script(ScriptKind);

impl TextHandler for Script {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let argument = cx.arg(SCRIPT)?.unwrap_or_default();
        // The argument is a formula of its own: `_{i=1}` has to be joined before it can
        // be looked up as script characters, or the `=` would not have become `₌`.
        let text = math::inline_text(&argument, cx.state(), cx.options());
        // Only a joiner puts spaces into a script's argument, so only where one ran is
        // there anything to take back out (`\sum_{i=1}^n` → `∑ᵢ₌₁ⁿ`).
        let strip_joiner_spaces = math::atoms_in_use(cx.state(), cx.options());
        let atom = script_atom(&text, self.0, strip_joiner_spaces);
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}
