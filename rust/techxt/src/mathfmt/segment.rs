//! Splitting an ordinary string into the atoms it is made of.
//!
//! A formula does not arrive as a sequence of atoms. `x+y` reaches the renderer as one
//! characters node, and a rule replacement (`\leq` → `≤`) or the fallback for an
//! unknown macro can produce any string at all. Such a string is therefore split into
//! atoms, so that the joiner sees the `+` in the middle of it and the `≤` becomes a
//! relation. Only *plain strings* are split this way; an [`Atom`] a handler built
//! deliberately is opaque, which is the whole point of the atom type.
//!
//! The character sets below are spelled out explicitly rather than derived from unicode
//! character categories: this is a port of v3's tables, the sets are deliberately
//! conservative, and any character not listed is [`Ord`](AtomClass::Ord) — "join me
//! tightly", which is the safer error, a missing space being less damaging than a space
//! in the wrong place.

use alloc::{string::String, vec::Vec};

use super::atom::{Atom, AtomClass};

/// `_math_bin_chars` → [`Bin`](AtomClass::Bin), 30 characters.
///
/// `+` U+002B, `-` U+002D, `*` U+002A, `−` U+2212, `±` U+00B1, `∓` U+2213,
/// `×` U+00D7, `÷` U+00F7, `·` U+00B7, `⋅` U+22C5, `∗` U+2217, `⋆` U+22C6,
/// `∘` U+2218, `∙` U+2219, `⊕` U+2295, `⊖` U+2296, `⊗` U+2297, `⊘` U+2298,
/// `⊙` U+2299, `∪` U+222A, `∩` U+2229, `⊎` U+228E, `⊔` U+2294, `⊓` U+2293,
/// `∧` U+2227, `∨` U+2228, `∖` U+2216, `≀` U+2240, `†` U+2020, `‡` U+2021.
const BIN_CHARS: &[char] = &[
    '+', '-', '*', '−', '±', '∓', '×', '÷', '·', '⋅', '∗', '⋆', '∘', '∙', '⊕', '⊖', '⊗', '⊘', '⊙',
    '∪', '∩', '⊎', '⊔', '⊓', '∧', '∨', '∖', '≀', '†', '‡',
];

/// `_math_rel_chars` → [`Rel`](AtomClass::Rel), 53 characters.
///
/// `=` U+003D, `<` U+003C, `>` U+003E, `:` U+003A, then the comparison, equivalence,
/// membership, inclusion, order and turnstile relations and the arrows: `≠` U+2260 …
/// `⟼` U+27FC. (`:` is a relation because TeX's `\colon` spacing is what a bare `:`
/// gets in math mode.)
const REL_CHARS: &[char] = &[
    '=', '<', '>', ':', '≠', '≤', '≥', '≪', '≫', '≡', '≈', '≃', '≅', '∼', '≍', '≐', '≜', '∝', '∈',
    '∉', '∋', '⊂', '⊃', '⊆', '⊇', '⊏', '⊐', '⊑', '⊒', '≺', '≻', '≼', '≽', '⊢', '⊣', '⊥', '⊨', '∣',
    '∥', '→', '←', '↔', '⇒', '⇐', '⇔', '↦', '⟶', '⟵', '⟷', '⟹', '⟸', '⟺', '⟼',
];

/// `_math_open_chars` → [`Open`](AtomClass::Open), 9 characters.
///
/// `(` U+0028, `[` U+005B, `{` U+007B, `⟨` U+27E8, `〈` U+3008, `⌈` U+2308,
/// `⌊` U+230A, `⟦` U+27E6, `⟮` U+27EE.
const OPEN_CHARS: &[char] = &['(', '[', '{', '⟨', '〈', '⌈', '⌊', '⟦', '⟮'];

/// `_math_close_chars` → [`Close`](AtomClass::Close), 10 characters.
///
/// `)` U+0029, `]` U+005D, `}` U+007D, `!` U+0021 (the factorial, which closes off
/// what precedes it), `⟩` U+27E9, `〉` U+3009, `⌉` U+2309, `⌋` U+230B, `⟧` U+27E7,
/// `⟯` U+27EF.
const CLOSE_CHARS: &[char] = &[')', ']', '}', '!', '⟩', '〉', '⌉', '⌋', '⟧', '⟯'];

/// `_math_punct_chars` → [`Punct`](AtomClass::Punct): `,` U+002C and `;` U+003B.
const PUNCT_CHARS: &[char] = &[',', ';'];

/// `_math_op_chars` → [`Op`](AtomClass::Op), 16 characters: the large operators.
///
/// `∑` U+2211, `∏` U+220F, `∐` U+2210, `∫` U+222B, `∬` U+222C, `∭` U+222D,
/// `∮` U+222E, `⋃` U+22C3, `⋂` U+22C2, `⋀` U+22C0, `⋁` U+22C1, `⨁` U+2A01,
/// `⨂` U+2A02, `⨀` U+2A00, `⨆` U+2A06, `⨄` U+2A04.
const OP_CHARS: &[char] = &[
    '∑', '∏', '∐', '∫', '∬', '∭', '∮', '⋃', '⋂', '⋀', '⋁', '⨁', '⨂', '⨀', '⨆', '⨄',
];

/// The explicit class of a character, or `None` for the [`Ord`](AtomClass::Ord)
/// default.
///
/// Two characters are missing from the tables on purpose. `|` (U+007C) renders both
/// `\mid`, which is a relation, and the absolute-value bars, which are delimiters;
/// nothing at this level can tell the two apart, so it stays ordinary. `/` (U+002F) is
/// ordinary in TeX itself, which is what keeps `a/b` tight.
fn char_class(c: char) -> Option<AtomClass> {
    for (table, cls) in [
        (BIN_CHARS, AtomClass::Bin),
        (REL_CHARS, AtomClass::Rel),
        (OPEN_CHARS, AtomClass::Open),
        (CLOSE_CHARS, AtomClass::Close),
        (PUNCT_CHARS, AtomClass::Punct),
        (OP_CHARS, AtomClass::Op),
    ] {
        if table.contains(&c) {
            return Some(cls);
        }
    }
    None
}

/// Whether `c` is one of the 52 upright latin letters, `a`–`z` or `A`–`Z`.
///
/// Deliberately not `char::is_alphabetic`: the point of the test is that a letter is
/// *upright*, and the math alphabets put italic variables outside this range.
fn is_upright_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// Split a plain string into the atoms it is made of (PLAN.md §9.5, v3
/// `_segment_plain_str`).
///
/// Three rules, applied left to right:
///
/// 1. **A run of two or more upright latin letters becomes a single
///    [`Op`](AtomClass::Op) atom** — the function-name rule, which is what makes
///    `\sin x` come out as `sin 𝑥`. It is sound only because variables are italicized:
///    the math-italic letters live outside `a`–`z`/`A`–`Z`, so an upright run stands
///    out as a name. A *single* upright letter is not an operator; a lone letter in
///    math is a variable. When no math alphabet is in force the heuristic would misfire
///    on `xy`, so the caller passes `upright_letters_are_op = false` and every letter
///    becomes its own [`Ord`](AtomClass::Ord); function names still come out right
///    because `\sin` and friends declare themselves `Op` explicitly. Per DECISIONS.md
///    D4 the flag is `true` exactly when the effective math font is a
///    [`Style`](super::FontStyle::Style).
/// 2. **A run of digits, absorbing a decimal point that has digits on both sides,
///    becomes one [`Ord`](AtomClass::Ord)**, so `3.14` is a single number while `3.` is
///    `3` then `.`, and `.5` is `.` then `5`.
/// 3. **Everything else is one atom per character**, classified by the tables above and
///    [`Ord`](AtomClass::Ord) by default. Adjacent ordinary characters are deliberately
///    *not* grouped: the spacing is the same either way, but the joiner puts its
///    retroactive space in front of the atom a script attaches to, and that atom has to
///    be the single character the script really applies to — `4π𝑐 𝑥^𝑞`, not `4π 𝑐𝑥^𝑞`.
///
/// Digits are ASCII digits only: `²` and `٣` are not digits here.
///
/// ```
/// use techxt::mathfmt::{segment_plain, AtomClass};
///
/// let atoms = segment_plain("sin(3.5+x)", true);
/// let shape: Vec<_> = atoms.iter().map(|a| (a.flatten(), a.cls.0)).collect();
/// assert_eq!(shape[0], ("sin".to_string(), AtomClass::Op));
/// assert_eq!(shape[1], ("(".to_string(), AtomClass::Open));
/// assert_eq!(shape[2], ("3.5".to_string(), AtomClass::Ord));
/// assert_eq!(shape[3], ("+".to_string(), AtomClass::Bin));
/// assert_eq!(shape[4], ("x".to_string(), AtomClass::Ord));
/// assert_eq!(shape[5], (")".to_string(), AtomClass::Close));
/// ```
pub fn segment_plain(s: &str, upright_letters_are_op: bool) -> Vec<Atom> {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    let mut out = Vec::new();
    let mut i = 0;

    while i < n {
        let c = cs[i];

        // (a) upright latin letters: a run of two or more is a function name
        if is_upright_latin_letter(c) {
            if upright_letters_are_op {
                let mut j = i;
                while j < n && is_upright_latin_letter(cs[j]) {
                    j += 1;
                }
                if j - i >= 2 {
                    out.push(atom(&cs[i..j], AtomClass::Op));
                    i = j;
                    continue;
                }
            }
            out.push(atom(&cs[i..i + 1], AtomClass::Ord));
            i += 1;
            continue;
        }

        // (b) digit runs, absorbing an interior decimal point
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n {
                if cs[j].is_ascii_digit() {
                    j += 1;
                    continue;
                }
                if cs[j] == '.' && j + 1 < n && cs[j + 1].is_ascii_digit() {
                    j += 1;
                    continue;
                }
                break;
            }
            out.push(atom(&cs[i..j], AtomClass::Ord));
            i = j;
            continue;
        }

        // (c) one atom per character, from the explicit tables or Ord
        out.push(atom(&cs[i..i + 1], char_class(c).unwrap_or(AtomClass::Ord)));
        i += 1;
    }

    out
}

/// Build one finished atom out of a run of characters.
fn atom(chars: &[char], cls: AtomClass) -> Atom {
    let text: String = chars.iter().collect();
    Atom::from_text(text, cls.both())
}

/// The class pair a plain string presents: the class of its first atom and the class
/// of its last one (v3 `_classify_plain_str`).
///
/// An empty string is ordinary on both edges.
///
/// ```
/// use techxt::mathfmt::{classify_plain, AtomClass};
///
/// assert_eq!(classify_plain("(x+y)", true), (AtomClass::Open, AtomClass::Close));
/// assert_eq!(classify_plain("", true), (AtomClass::Ord, AtomClass::Ord));
/// ```
pub fn classify_plain(s: &str, upright_letters_are_op: bool) -> (AtomClass, AtomClass) {
    let atoms = segment_plain(s, upright_letters_are_op);
    match (atoms.first(), atoms.last()) {
        (Some(first), Some(last)) => (first.cls.0, last.cls.1),
        _ => AtomClass::both(AtomClass::Ord),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::ToString, vec};

    /// `(text, left class)` for each atom, which is all the tests below care about.
    fn shape(s: &str, upright_letters_are_op: bool) -> Vec<(String, AtomClass)> {
        segment_plain(s, upright_letters_are_op)
            .iter()
            .map(|a| (a.flatten(), a.cls.0))
            .collect()
    }

    #[test]
    fn tables_have_the_documented_sizes() {
        assert_eq!(BIN_CHARS.len(), 30);
        assert_eq!(REL_CHARS.len(), 53);
        assert_eq!(OPEN_CHARS.len(), 9);
        assert_eq!(CLOSE_CHARS.len(), 10);
        assert_eq!(PUNCT_CHARS.len(), 2);
        assert_eq!(OP_CHARS.len(), 16);
    }

    #[test]
    fn no_character_is_classified_twice() {
        let tables = [
            BIN_CHARS,
            REL_CHARS,
            OPEN_CHARS,
            CLOSE_CHARS,
            PUNCT_CHARS,
            OP_CHARS,
        ];
        let mut seen: Vec<char> = Vec::new();
        for table in tables {
            for c in table {
                assert!(!seen.contains(c), "{c:?} appears in two class tables");
                seen.push(*c);
            }
        }
        assert_eq!(seen.len(), 30 + 53 + 9 + 10 + 2 + 16);
    }

    #[test]
    fn every_class_table_is_reachable() {
        assert_eq!(shape("+", true), vec![("+".to_string(), AtomClass::Bin)]);
        assert_eq!(shape("≤", true), vec![("≤".to_string(), AtomClass::Rel)]);
        assert_eq!(shape("⌈", true), vec![("⌈".to_string(), AtomClass::Open)]);
        assert_eq!(shape("!", true), vec![("!".to_string(), AtomClass::Close)]);
        assert_eq!(shape(";", true), vec![(";".to_string(), AtomClass::Punct)]);
        assert_eq!(shape("∮", true), vec![("∮".to_string(), AtomClass::Op)]);
        // anything unlisted is ordinary
        assert_eq!(shape("π", true), vec![("π".to_string(), AtomClass::Ord)]);
    }

    #[test]
    fn pipe_and_slash_stay_ordinary() {
        assert_eq!(shape("|", true), vec![("|".to_string(), AtomClass::Ord)]);
        assert_eq!(shape("/", true), vec![("/".to_string(), AtomClass::Ord)]);
    }

    #[test]
    fn two_or_more_upright_letters_are_one_operator() {
        assert_eq!(shape("sin", true), vec![("sin".to_string(), AtomClass::Op)]);
        assert_eq!(shape("xy", true), vec![("xy".to_string(), AtomClass::Op)]);
        // a lone letter is a variable, not a function name
        assert_eq!(shape("x", true), vec![("x".to_string(), AtomClass::Ord)]);
    }

    #[test]
    fn the_operator_rule_is_off_without_a_math_alphabet() {
        assert_eq!(
            shape("xy", false),
            vec![
                ("x".to_string(), AtomClass::Ord),
                ("y".to_string(), AtomClass::Ord)
            ]
        );
    }

    #[test]
    fn styled_letters_are_not_upright_letters() {
        // 𝑐 (U+1D450) is math-italic and must not join a function-name run
        assert_eq!(
            shape("a𝑐", true),
            vec![
                ("a".to_string(), AtomClass::Ord),
                ("𝑐".to_string(), AtomClass::Ord)
            ]
        );
    }

    #[test]
    fn digit_runs_absorb_an_interior_decimal_point_only() {
        assert_eq!(
            shape("3.14", true),
            vec![("3.14".to_string(), AtomClass::Ord)]
        );
        assert_eq!(
            shape("3.", true),
            vec![
                ("3".to_string(), AtomClass::Ord),
                (".".to_string(), AtomClass::Ord)
            ]
        );
        assert_eq!(
            shape(".5", true),
            vec![
                (".".to_string(), AtomClass::Ord),
                ("5".to_string(), AtomClass::Ord)
            ]
        );
        assert_eq!(
            shape("1.2.3", true),
            vec![("1.2.3".to_string(), AtomClass::Ord)]
        );
        // only ASCII digits count
        assert_eq!(
            shape("2²", true),
            vec![
                ("2".to_string(), AtomClass::Ord),
                ("²".to_string(), AtomClass::Ord)
            ]
        );
    }

    #[test]
    fn ordinary_characters_are_never_grouped() {
        assert_eq!(
            shape("𝑐𝑥", true),
            vec![
                ("𝑐".to_string(), AtomClass::Ord),
                ("𝑥".to_string(), AtomClass::Ord)
            ]
        );
    }

    #[test]
    fn empty_input_yields_no_atoms() {
        assert!(segment_plain("", true).is_empty());
        assert_eq!(classify_plain("", true), (AtomClass::Ord, AtomClass::Ord));
    }

    #[test]
    fn classification_reads_the_two_edges() {
        assert_eq!(classify_plain("sin", true), (AtomClass::Op, AtomClass::Op));
        assert_eq!(
            classify_plain("(a+b)", true),
            (AtomClass::Open, AtomClass::Close)
        );
        assert_eq!(classify_plain(" ", true), (AtomClass::Ord, AtomClass::Ord));
    }

    #[test]
    fn segmentation_is_a_partition_of_the_input() {
        for s in ["sin(3.5+x)", "a - b", "∑_{i}", "|x|/2", "", "  ", "x^q p"] {
            let joined: String = segment_plain(s, true).iter().map(|a| a.flatten()).collect();
            assert_eq!(joined, s);
        }
    }
}
