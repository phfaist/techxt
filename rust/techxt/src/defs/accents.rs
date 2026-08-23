//! The accent category: `\'e`, `\c c`, `\~n` and the rest (PLAN.md §9.3).
//!
//! An accent macro installs a combining mark, and the mark is applied to the **first
//! base character** of its rendered argument — the rest of the argument is left exactly
//! as it was. That is deliberate and it is a fix: pylatexenc v3 applies the mark to
//! *every* character of the argument, so `\'{abc}` comes out as `ábć`. TeX accents one
//! character; so does techxt.
//!
//! Three steps, in this order:
//!
//! 1. **Dotless letters lose their dot.** `ı` and `ȷ` (what `\i` and `\j` produce)
//!    become `i` and `j` first, because `\^\i` means "an i with a circumflex", which is
//!    written `î` — the dotless form exists only so that the accent has somewhere to go.
//! 2. **The pair is composed.** `('e', ◌́)` becomes `é` through the generated table
//!    (`ACCENT_COMPOSITIONS`, PLAN.md §12.4) — the precomputed answer of unicode's NFC,
//!    which a `no_std` crate cannot compute for itself.
//! 3. **What does not compose stays a pair.** The base character followed by the
//!    combining mark is still correct text; it simply is not a single code point.
//!
//! An accent with an empty argument (`\'{}`, which is how a bare accent is written)
//! renders as the mark's own standalone form where unicode has one — `´` for `\'{}` —
//! and otherwise as a space followed by the mark.

use alloc::string::String;

use techy::core::node::NodeRef;
use techy_xp::lang::LatexlikeXp;

use crate::def::{Category, MacroDef, TextHandler};
use crate::flow::Flow;
use crate::render::{RenderCx, RenderError};

use super::accents_data::{ACCENTS, ACCENT_COMPOSITIONS, ACCENT_SPACING};
use super::handler;

/// The name of the accent macros' single argument.
const BASE: &str = "base";

/// `ı`, which `\i` produces: an `i` with the dot taken off, so an accent can sit there.
const DOTLESS_I: char = '\u{131}';

/// `ȷ`, the same for `j`.
const DOTLESS_J: char = '\u{237}';

/// The accents category (PLAN.md §12.1).
///
/// One entry per row of the generated table, all with the same shape: one mandatory
/// argument (so that `\c c` works through TeX's single-expression fallback as well as
/// `\c{c}`), and a handler carrying the combining mark.
pub fn category() -> Category {
    let mut category = Category::new("accents");
    for &(name, mark) in ACCENTS {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", BASE)
                .rule(handler(Accent(mark))),
        );
    }
    category
}

/// Apply `mark` to the first character of the argument.
#[derive(Debug)]
struct Accent(char);

impl TextHandler for Accent {
    fn render(
        &self,
        _node: NodeRef<'_, LatexlikeXp>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // The argument as one line: an accent's argument is a character, not a
        // paragraph, and a rendered `\i` has to arrive here as text before its dot can
        // be reasoned about.
        let base = cx.arg_text(BASE)?.unwrap_or_default();
        let mut characters = base.chars();
        let Some(first) = characters.next() else {
            return Ok(Flow::text(&standalone(self.0)));
        };
        let mut out = String::with_capacity(base.len() + 4);
        push_accented(&mut out, first, self.0);
        out.push_str(characters.as_str());
        Ok(Flow::from_plain_text(&out))
    }
}

/// Append `base` wearing `mark`, composed if unicode has a single character for it.
fn push_accented(out: &mut String, base: char, mark: char) {
    let base = match base {
        DOTLESS_I => 'i',
        DOTLESS_J => 'j',
        other => other,
    };
    match compose(base, mark) {
        Some(composed) => out.push(composed),
        None => {
            out.push(base);
            out.push(mark);
        }
    }
}

/// The single character `base` + `mark` composes to under NFC, if there is one.
fn compose(base: char, mark: char) -> Option<char> {
    ACCENT_COMPOSITIONS
        .binary_search_by_key(&(base, mark), |&(base, mark, _)| (base, mark))
        .ok()
        .map(|index| ACCENT_COMPOSITIONS[index].2)
}

/// How an accent with no argument at all renders (PLAN.md §9.3).
fn standalone(mark: char) -> String {
    match ACCENT_SPACING.binary_search_by_key(&mark, |&(mark, _)| mark) {
        Ok(index) => String::from(ACCENT_SPACING[index].1),
        // No standalone form exists for this mark; a space is something for it to sit
        // on, which is what pylatexenc does too.
        Err(_) => {
            let mut out = String::from(" ");
            out.push(mark);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn the_generated_tables_are_sorted_for_bisection() {
        let names: Vec<&str> = ACCENTS.iter().map(|&(name, _)| name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);

        assert!(ACCENT_SPACING.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(ACCENT_COMPOSITIONS
            .windows(2)
            .all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1)));
    }

    #[test]
    fn composition_finds_the_precomposed_character() {
        assert_eq!(compose('e', '\u{301}'), Some('é'));
        assert_eq!(compose('c', '\u{327}'), Some('ç'));
        assert_eq!(compose('n', '\u{303}'), Some('ñ'));
        assert_eq!(compose('=', '\u{338}'), Some('≠'));
        // Nothing composes with a right arrow above.
        assert_eq!(compose('v', '\u{20d7}'), None);
    }

    #[test]
    fn dotless_letters_get_their_dot_back_before_the_accent() {
        let mut out = String::new();
        push_accented(&mut out, DOTLESS_I, '\u{302}');
        assert_eq!(out, "î");

        let mut out = String::new();
        push_accented(&mut out, DOTLESS_J, '\u{30c}');
        assert_eq!(out, "ǰ");
    }

    #[test]
    fn a_pair_that_does_not_compose_stays_a_pair() {
        let mut out = String::new();
        push_accented(&mut out, 'q', '\u{301}');
        assert_eq!(out, "q\u{301}");
    }

    #[test]
    fn a_bare_accent_uses_the_standalone_form_when_there_is_one() {
        assert_eq!(standalone('\u{301}'), "´");
        assert_eq!(standalone('\u{300}'), "`");
        assert_eq!(standalone('\u{20d7}'), " \u{20d7}");
    }
}
