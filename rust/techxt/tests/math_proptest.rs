//! Property tests for the joiner invariants of PLAN.md §14.2.
//!
//! The plan asks for three guarantees, for *every* sequence of atoms:
//!
//! 1. the joiner never produces a doubled space — it inserts a separator only where
//!    neither side already carries one;
//! 2. atoms that render to nothing drop out without leaving a separator behind;
//! 3. the result is stable under re-chunking adjacent plain text.
//!
//! Guarantee 3 needs a word of precision. Re-chunking is *not* invariant in general,
//! and deliberately so: `segment_plain` is what decides where the atoms are, so
//! splitting `12` into `1` and `2` produces two atoms and the joiner then separates
//! them (`1 2`) — that is the documented rendering of `${1}2$`. What must hold is that
//! splitting a plain string **at an atom boundary** changes nothing, i.e. the joiner
//! sees the atoms and not the chunks they arrived in.
//!
//! And a fourth, unwritten one: never panic, on anything.

use proptest::prelude::*;
use techxt::mathfmt::{
    join_atoms, join_atoms_with, segment_plain, Atom, AtomBody, AtomClass, MathBox, MathWrapDelims,
    ScriptInfo, ScriptKind,
};

/// Characters atoms are built from: letters, digits, operators, delimiters, a styled
/// letter and the two deliberately unclassified characters. **No spaces** — a space in
/// an atom's own text is the caller's content, and the invariants below are about
/// spaces the *joiner* inserts.
const ATOM_CHARS: &[char] = &[
    'a', 'b', 'x', '1', '2', '+', '=', '(', ')', ',', 'π', '𝑐', '𝑥', '/', '|', '∑',
];

const ALL_CLASSES: &[AtomClass] = &[
    AtomClass::Ord,
    AtomClass::Op,
    AtomClass::Bin,
    AtomClass::Rel,
    AtomClass::Open,
    AtomClass::Close,
    AtomClass::Punct,
    AtomClass::OpenEnd,
    AtomClass::Text,
    AtomClass::Script,
    AtomClass::Block,
];

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ATOM_CHARS), 0..5)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Never empty, so that a leading and a trailing space cannot land next to each other
/// and produce an input that already holds two spaces in a row.
fn nonempty_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ATOM_CHARS), 1..5)
        .prop_map(|chars| chars.into_iter().collect())
}

fn cls_pair() -> impl Strategy<Value = (AtomClass, AtomClass)> {
    (
        prop::sample::select(ALL_CLASSES),
        prop::sample::select(ALL_CLASSES),
    )
}

fn script() -> impl Strategy<Value = Option<ScriptInfo>> {
    prop::option::of(
        (
            prop::sample::select(&[ScriptKind::Superscript, ScriptKind::Subscript][..]),
            any::<bool>(),
        )
            .prop_map(|(kind, latex_notation)| ScriptInfo {
                kind,
                latex_notation,
            }),
    )
}

/// Atoms with a single-line body: what inline math is made of.
fn flat_atom() -> impl Strategy<Value = Atom> {
    let body = prop_oneof![
        text().prop_map(|t| AtomBody::Str(t.into_boxed_str())),
        (text(), prop::sample::select(&["", "^", "_"][..])).prop_map(|(inner, prefix)| {
            AtomBody::Wrappable {
                inner: inner.into_boxed_str(),
                prefix: prefix.into(),
            }
        }),
    ];
    (body, cls_pair(), script()).prop_map(|(body, cls, script)| Atom { body, cls, script })
}

/// Atoms that may also be multi-row blocks: what display math is made of.
fn any_atom() -> impl Strategy<Value = Atom> {
    prop_oneof![
        4 => flat_atom(),
        1 => (
            prop::collection::vec(text(), 1..4),
            0usize..3,
            cls_pair(),
        )
            .prop_map(|(lines, baseline, cls)| Atom {
                body: AtomBody::Block(MathBox {
                    baseline: baseline.min(lines.len() - 1),
                    lines: lines.into_iter().map(String::into_boxed_str).collect(),
                }),
                cls,
                script: None,
            }),
    ]
}

fn delims() -> impl Strategy<Value = MathWrapDelims> {
    prop_oneof![
        Just(MathWrapDelims::Parens),
        Just(MathWrapDelims::Braces),
        Just(MathWrapDelims::None),
        Just(MathWrapDelims::Custom("<".into(), ">".into())),
    ]
}

/// Plain strings for the re-chunking property, over the same characters.
fn plain() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ATOM_CHARS), 0..12)
        .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    /// No two spaces in a row: every space in the output is either one the caller put
    /// in an atom (impossible here — the generated texts hold none) or a single
    /// separator the joiner inserted.
    #[test]
    fn joiner_never_doubles_a_space(atoms in prop::collection::vec(flat_atom(), 0..8),
                                    delims in delims()) {
        let joined = join_atoms_with(atoms, false, &delims);
        let text = joined.math_box.join_lines("\n");
        prop_assert!(!text.contains("  "), "doubled space in {text:?}");
    }

    /// A separator is never inserted next to a space that is already there. The atoms
    /// here may carry one leading and one trailing space of their own, so two spaces
    /// can meet — but a third could only come from the joiner.
    #[test]
    fn joiner_never_adds_to_an_existing_space(
        parts in prop::collection::vec(
            (nonempty_text(), any::<bool>(), any::<bool>(), cls_pair()), 0..8)
    ) {
        let atoms: Vec<Atom> = parts
            .into_iter()
            .map(|(t, lead, trail, cls)| {
                let mut s = String::new();
                if lead { s.push(' '); }
                s.push_str(&t);
                if trail { s.push(' '); }
                Atom::from_text(s, cls)
            })
            .collect();
        let text = join_atoms(atoms, false).join_lines("\n");
        prop_assert!(!text.contains("   "), "tripled space in {text:?}");
    }

    /// Atoms that render to nothing are dropped, and dropping them cannot change the
    /// spacing around them: inserting empty atoms anywhere is a no-op.
    #[test]
    fn empty_atoms_are_transparent(atoms in prop::collection::vec(flat_atom(), 0..6),
                                   positions in prop::collection::vec(0usize..7, 0..4),
                                   delims in delims()) {
        let reference = join_atoms_with(atoms.clone(), false, &delims)
            .math_box
            .join_lines("\n");

        let mut padded = atoms;
        for pos in positions {
            let at = pos.min(padded.len());
            padded.insert(at, Atom::from_text("", AtomClass::both(AtomClass::Bin)));
        }
        let with_empties = join_atoms_with(padded, false, &delims)
            .math_box
            .join_lines("\n");
        prop_assert_eq!(with_empties, reference);
    }

    /// Splitting a plain string at an atom boundary changes nothing: the joiner sees
    /// the atoms, not the chunks they arrived in.
    #[test]
    fn re_chunking_at_atom_boundaries_is_invariant(s in plain(), upright_op in any::<bool>()) {
        let atoms = segment_plain(&s, upright_op);
        let reference = join_atoms(atoms.clone(), false).join_lines("\n");

        for k in 0..=atoms.len() {
            let head: String = atoms[..k].iter().map(|a| a.flatten()).collect();
            let tail: String = atoms[k..].iter().map(|a| a.flatten()).collect();
            let mut split = segment_plain(&head, upright_op);
            split.extend(segment_plain(&tail, upright_op));
            let rejoined = join_atoms(split, false).join_lines("\n");
            prop_assert_eq!(&rejoined, &reference, "split after atom {}", k);
        }
    }

    /// Whatever the atoms, the joiner terminates, produces no line ending in
    /// whitespace, and keeps inline math on a single line.
    #[test]
    fn joining_is_total(atoms in prop::collection::vec(any_atom(), 0..8),
                        display in any::<bool>(),
                        delims in delims()) {
        let joined = join_atoms_with(atoms, display, &delims);
        for line in &joined.math_box.lines {
            prop_assert!(!line.ends_with(' '), "trailing space in {line:?}");
        }
        if !display {
            prop_assert!(joined.math_box.lines.len() <= 1);
        }
        prop_assert!(joined.math_box.baseline < joined.math_box.lines.len().max(1));
    }
}
