//! Constructors for the math constructs that build their own delimiters: `\frac`
//! and `\sqrt`.
//!
//! These are the mathfmt-level halves of the macros; the definitions themselves —
//! argument specs, named arguments, registration — belong to `techxt::defs`.

use alloc::string::String;

use super::atom::{Atom, AtomClass};
use super::join::MathWrapDelims;

/// Build the atom for `\frac{numerator}{denominator}`, with both operands already
/// rendered to text (v3 `_format_frac`; also `\nicefrac`, `\textfrac`).
///
/// **This is not a deferred wrappable** (DECISIONS.md D1). A fraction knows all by
/// itself that `(a+b)/c` needs parentheses — the reason is *internal*, it does not
/// depend on the neighbours — so the delimiters go on eagerly and the result is a
/// finished [`Str`](super::AtomBody::Str) atom. Making it a
/// [`Wrappable`](super::AtomBody::Wrappable) would wrap the whole fraction instead of
/// its operands and turn `$\frac{a}{b}c$` into `(𝑎/𝑏)𝑐` rather than the correct
/// `𝑎/𝑏 𝑐`.
///
/// The class pair follows from which operands got delimiters: an operand that wrapped
/// shows where it ends and presents its delimiter to the neighbour, while a bare one
/// leaves that side [open-ended](AtomClass::OpenEnd), so the joiner sets the neighbour
/// apart.
///
/// ```
/// use techxt::mathfmt::{frac_atom, AtomClass, MathWrapDelims};
///
/// let f = frac_atom("4π𝑐", "2", &MathWrapDelims::Parens);
/// assert_eq!(f.flatten(), "(4π𝑐)/2");
/// assert_eq!(f.cls, (AtomClass::Open, AtomClass::OpenEnd));
///
/// let g = frac_atom("𝑎", "𝑏", &MathWrapDelims::Parens);
/// assert_eq!(g.flatten(), "𝑎/𝑏");
/// assert_eq!(g.cls, (AtomClass::OpenEnd, AtomClass::OpenEnd));
/// ```
pub fn frac_atom(numerator: &str, denominator: &str, delims: &MathWrapDelims) -> Atom {
    let num = delims.wrap(numerator);
    let den = delims.wrap(denominator);

    let cls_left = if num != numerator {
        AtomClass::Open
    } else {
        AtomClass::OpenEnd
    };
    let cls_right = if den != denominator {
        AtomClass::Close
    } else {
        AtomClass::OpenEnd
    };

    let mut text = String::with_capacity(num.len() + 1 + den.len());
    text.push_str(&num);
    text.push('/');
    text.push_str(&den);

    Atom::from_text(text, (cls_left, cls_right))
}

/// SQUARE ROOT, U+221A.
const RADICAL: char = '\u{221A}';
/// CUBE ROOT, U+221B, and FOURTH ROOT, U+221C: the two degrees unicode draws into the
/// sign itself.
const ROOT_SIGNS: &[(&str, char)] = &[("3", '\u{221B}'), ("4", '\u{221C}')];

/// Build the atom for `\sqrt[index]{radicand}`, with both arguments already rendered
/// to text (v3 `_format_sqrt`).
///
/// The radicand is wrapped in delimiters when it is longer than one character, since
/// plain text cannot draw the bar that shows how far the root reaches. An `index` of
/// `3` or `4` is drawn into the sign (`∛`, `∜`); any other index is written in front
/// of it, where LaTeX puts it. The index is compared as a string, which is why it
/// works under the italic default: digits are never styled.
///
/// The class pair is `(Ord, …)`: the root sign closes the expression off on the left,
/// so `2\sqrt{x}` stays tight as `2√𝑥`. On the right the radicand's delimiters decide —
/// without them the extent is open and `\sqrt{x}2` has to become `√𝑥 2`.
///
/// ```
/// use techxt::mathfmt::{sqrt_atom, AtomClass, MathWrapDelims};
///
/// let r = sqrt_atom("𝑥", None, &MathWrapDelims::Parens);
/// assert_eq!(r.flatten(), "√𝑥");
/// assert_eq!(r.cls, (AtomClass::Ord, AtomClass::OpenEnd));
///
/// assert_eq!(sqrt_atom("𝑥+𝑦", Some("3"), &MathWrapDelims::Parens).flatten(), "∛(𝑥+𝑦)");
/// assert_eq!(sqrt_atom("𝑥", Some("𝑛"), &MathWrapDelims::Parens).flatten(), "𝑛√𝑥");
/// ```
pub fn sqrt_atom(radicand: &str, index: Option<&str>, delims: &MathWrapDelims) -> Atom {
    let wrapped = delims.wrap(radicand);
    let cls_right = if wrapped != radicand {
        AtomClass::Close
    } else {
        AtomClass::OpenEnd
    };

    let index = index.map(str::trim).filter(|i| !i.is_empty());
    let mut sign = RADICAL;
    let mut prefix = "";
    if let Some(index) = index {
        match ROOT_SIGNS.iter().find(|(degree, _)| *degree == index) {
            Some((_, drawn)) => sign = *drawn,
            None => prefix = index,
        }
    }

    let mut text = String::with_capacity(prefix.len() + sign.len_utf8() + wrapped.len());
    text.push_str(prefix);
    text.push(sign);
    text.push_str(&wrapped);

    Atom::from_text(text, (AtomClass::Ord, cls_right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mathfmt::{join_atoms_with, segment_plain, AtomBody};
    use alloc::{vec, vec::Vec};

    fn joined(atoms: Vec<Atom>) -> String {
        join_atoms_with(atoms, false, &MathWrapDelims::Parens)
            .math_box
            .join_lines("\n")
    }

    #[test]
    fn frac_wraps_only_the_operands_that_need_it() {
        let f = frac_atom("𝑎+𝑏", "𝑐", &MathWrapDelims::Parens);
        assert_eq!(f.flatten(), "(𝑎+𝑏)/𝑐");
        assert_eq!(f.cls, (AtomClass::Open, AtomClass::OpenEnd));

        let g = frac_atom("𝑎", "𝑏+𝑐", &MathWrapDelims::Parens);
        assert_eq!(g.flatten(), "𝑎/(𝑏+𝑐)");
        assert_eq!(g.cls, (AtomClass::OpenEnd, AtomClass::Close));

        assert_eq!(
            frac_atom("𝑎+𝑏", "𝑐", &MathWrapDelims::Braces).flatten(),
            "{𝑎+𝑏}/𝑐"
        );
        assert_eq!(
            frac_atom("𝑎+𝑏", "𝑐", &MathWrapDelims::None).flatten(),
            "𝑎+𝑏/𝑐"
        );
    }

    #[test]
    fn frac_is_eager_so_a_following_factor_is_only_spaced() {
        // DECISIONS.md D1: a true wrappable would give `(𝑎/𝑏)𝑐`
        let atoms = vec![
            frac_atom("𝑎", "𝑏", &MathWrapDelims::Parens),
            Atom::from_text("𝑐", AtomClass::both(AtomClass::Ord)),
        ];
        assert_eq!(joined(atoms), "𝑎/𝑏 𝑐");
        assert!(matches!(
            frac_atom("𝑎", "𝑏", &MathWrapDelims::Parens).body,
            AtomBody::Str(_)
        ));
    }

    #[test]
    fn frac_next_to_a_factor_on_the_left() {
        let atoms = vec![
            Atom::from_text("𝑐", AtomClass::both(AtomClass::Ord)),
            frac_atom("𝑎", "𝑏", &MathWrapDelims::Parens),
        ];
        assert_eq!(joined(atoms), "𝑐 𝑎/𝑏");
    }

    #[test]
    fn sqrt_degrees_and_delimiters() {
        assert_eq!(
            sqrt_atom("𝑥", Some("4"), &MathWrapDelims::Parens).flatten(),
            "∜𝑥"
        );
        assert_eq!(
            sqrt_atom("𝑥+𝑦", Some("𝑛"), &MathWrapDelims::Parens).flatten(),
            "𝑛√(𝑥+𝑦)"
        );
        // an empty or whitespace index is no index
        assert_eq!(
            sqrt_atom("𝑥", Some("  "), &MathWrapDelims::Parens).flatten(),
            "√𝑥"
        );
    }

    #[test]
    fn sqrt_hugs_on_the_left_and_opens_on_the_right() {
        let two = Atom::from_text("2", AtomClass::both(AtomClass::Ord));
        let root = sqrt_atom("𝑥", None, &MathWrapDelims::Parens);
        assert_eq!(joined(vec![two.clone(), root.clone()]), "2√𝑥");
        assert_eq!(joined(vec![root, two]), "√𝑥 2");
    }

    #[test]
    fn a_wrapped_radicand_closes_the_expression() {
        let root = sqrt_atom("𝑥+𝑦", None, &MathWrapDelims::Parens);
        assert_eq!(root.cls, (AtomClass::Ord, AtomClass::Close));
        let atoms = vec![root, Atom::from_text("2", AtomClass::both(AtomClass::Ord))];
        assert_eq!(joined(atoms), "√(𝑥+𝑦)2");
    }

    #[test]
    fn plan_example_11_the_fraction_next_to_a_named_operator() {
        // $\frac{4\pi c}{2}\sin(x+y)$  →  (4π𝑐)/2 sin(𝑥 + 𝑦)
        let mut atoms = vec![
            frac_atom("4π𝑐", "2", &MathWrapDelims::Parens),
            Atom::from_text("sin", AtomClass::both(AtomClass::Op)),
        ];
        atoms.extend(segment_plain("(", true));
        atoms.extend(segment_plain("𝑥+𝑦", true));
        atoms.extend(segment_plain(")", true));
        assert_eq!(joined(atoms), "(4π𝑐)/2 sin(𝑥 + 𝑦)");
    }
}
