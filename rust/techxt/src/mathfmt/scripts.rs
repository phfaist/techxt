//! Sub- and superscripts: unicode script characters, with a fallback to `^(...)`.
//!
//! Unicode has a partial set of superscript and subscript characters — enough for
//! `𝑥²`, `𝑎ᵢ` and even `∑ᵢ₌₁ⁿ`, but with conspicuous holes: there is no superscript
//! `q`, no capital subscript at all, and no script space. So a script is rendered by a
//! three-step ladder, and only the last step falls back on LaTeX's own notation.

use alloc::string::{String, ToString};

use super::alphabets::unstyle_char;
use super::atom::{Atom, AtomBody, AtomClass, ScriptInfo, ScriptKind};

/// `_fmt_superscript_chars` — the 68 characters that have a superscript form.
///
/// Collected from unicode's "superscripts and subscripts" chart plus the modifier
/// letters of "spacing modifier letters" and "phonetic extensions". Missing on
/// purpose: lowercase `q` and the capitals `C F Q S X Y Z` — `MODIFIER LETTER CAPITAL
/// C/F/Q` do exist as of Unicode 14 but fonts do not have them. `−` (U+2212 MINUS
/// SIGN) maps to the same character as ASCII `-`, and the two variant greek forms
/// (`ϑ`/`θ`, `ϕ`/`φ`) share one modifier letter each, because `\phi` renders as `ϕ`
/// while `\varphi` renders as `φ`.
const SUPERSCRIPT_CHARS: &[(char, char)] = &[
    ('0', '\u{2070}'),
    ('1', '\u{00B9}'),
    ('2', '\u{00B2}'),
    ('3', '\u{00B3}'),
    ('4', '\u{2074}'),
    ('5', '\u{2075}'),
    ('6', '\u{2076}'),
    ('7', '\u{2077}'),
    ('8', '\u{2078}'),
    ('9', '\u{2079}'),
    ('+', '\u{207A}'),
    ('-', '\u{207B}'),
    ('\u{2212}', '\u{207B}'),
    ('=', '\u{207C}'),
    ('(', '\u{207D}'),
    (')', '\u{207E}'),
    ('a', '\u{1D43}'),
    ('b', '\u{1D47}'),
    ('c', '\u{1D9C}'),
    ('d', '\u{1D48}'),
    ('e', '\u{1D49}'),
    ('f', '\u{1DA0}'),
    ('g', '\u{1D4D}'),
    ('h', '\u{02B0}'),
    ('i', '\u{2071}'),
    ('j', '\u{02B2}'),
    ('k', '\u{1D4F}'),
    ('l', '\u{02E1}'),
    ('m', '\u{1D50}'),
    ('n', '\u{207F}'),
    ('o', '\u{1D52}'),
    ('p', '\u{1D56}'),
    ('r', '\u{02B3}'),
    ('s', '\u{02E2}'),
    ('t', '\u{1D57}'),
    ('u', '\u{1D58}'),
    ('v', '\u{1D5B}'),
    ('w', '\u{02B7}'),
    ('x', '\u{02E3}'),
    ('y', '\u{02B8}'),
    ('z', '\u{1DBB}'),
    ('A', '\u{1D2C}'),
    ('B', '\u{1D2E}'),
    ('D', '\u{1D30}'),
    ('E', '\u{1D31}'),
    ('G', '\u{1D33}'),
    ('H', '\u{1D34}'),
    ('I', '\u{1D35}'),
    ('J', '\u{1D36}'),
    ('K', '\u{1D37}'),
    ('L', '\u{1D38}'),
    ('M', '\u{1D39}'),
    ('N', '\u{1D3A}'),
    ('O', '\u{1D3C}'),
    ('P', '\u{1D3E}'),
    ('R', '\u{1D3F}'),
    ('T', '\u{1D40}'),
    ('U', '\u{1D41}'),
    ('V', '\u{2C7D}'),
    ('W', '\u{1D42}'),
    ('\u{03B2}', '\u{1D5D}'), // β
    ('\u{03B3}', '\u{1D5E}'), // γ
    ('\u{03B4}', '\u{1D5F}'), // δ
    ('\u{03B8}', '\u{1DBF}'), // θ
    ('\u{03D1}', '\u{1DBF}'), // ϑ, the greek theta symbol
    ('\u{03C6}', '\u{1D60}'), // φ
    ('\u{03D5}', '\u{1D60}'), // ϕ, the greek phi symbol
    ('\u{03C7}', '\u{1D61}'), // χ
];

/// `_fmt_subscript_chars` — the 40 characters that have a subscript form.
///
/// Missing on purpose: the lowercase letters `b c d f g q w y z`, and **every** capital
/// — unicode has no capital subscripts at all, which is why `$x_N$` has to fall back on
/// `x_N`.
const SUBSCRIPT_CHARS: &[(char, char)] = &[
    ('0', '\u{2080}'),
    ('1', '\u{2081}'),
    ('2', '\u{2082}'),
    ('3', '\u{2083}'),
    ('4', '\u{2084}'),
    ('5', '\u{2085}'),
    ('6', '\u{2086}'),
    ('7', '\u{2087}'),
    ('8', '\u{2088}'),
    ('9', '\u{2089}'),
    ('+', '\u{208A}'),
    ('-', '\u{208B}'),
    ('\u{2212}', '\u{208B}'),
    ('=', '\u{208C}'),
    ('(', '\u{208D}'),
    (')', '\u{208E}'),
    ('a', '\u{2090}'),
    ('e', '\u{2091}'),
    ('h', '\u{2095}'),
    ('i', '\u{1D62}'),
    ('j', '\u{2C7C}'),
    ('k', '\u{2096}'),
    ('l', '\u{2097}'),
    ('m', '\u{2098}'),
    ('n', '\u{2099}'),
    ('o', '\u{2092}'),
    ('p', '\u{209A}'),
    ('r', '\u{1D63}'),
    ('s', '\u{209B}'),
    ('t', '\u{209C}'),
    ('u', '\u{1D64}'),
    ('v', '\u{1D65}'),
    ('x', '\u{2093}'),
    ('\u{03B2}', '\u{1D66}'), // β
    ('\u{03B3}', '\u{1D67}'), // γ
    ('\u{03C1}', '\u{1D68}'), // ρ
    ('\u{03F1}', '\u{1D68}'), // ϱ, the greek rho symbol
    ('\u{03C6}', '\u{1D69}'), // φ
    ('\u{03D5}', '\u{1D69}'), // ϕ, the greek phi symbol
    ('\u{03C7}', '\u{1D6A}'), // χ
];

/// The table for one script kind.
fn table(kind: ScriptKind) -> &'static [(char, char)] {
    match kind {
        ScriptKind::Superscript => SUPERSCRIPT_CHARS,
        ScriptKind::Subscript => SUBSCRIPT_CHARS,
    }
}

/// Render `text` in unicode script characters, or `None` if any character has none
/// (v3 `fmt_subsuperscript_text`).
///
/// The lookup is **all-or-nothing**: one unmappable character fails the whole string,
/// because half a script is worse than none — `x^(ab)` is readable, `xᵃb` is a lie.
///
/// Every character is first put through [`unstyle_char`](super::unstyle_char): unicode
/// has no *styled* script letters, so a math-italic `𝑖` is looked up as `i`. The style
/// is lost and the script is kept, which is the better trade — and it costs nothing on
/// text that carries no style.
///
/// ```
/// use techxt::mathfmt::{fmt_script_text, ScriptKind};
///
/// assert_eq!(fmt_script_text("2", ScriptKind::Superscript).as_deref(), Some("²"));
/// assert_eq!(fmt_script_text("𝑖=1", ScriptKind::Subscript).as_deref(), Some("ᵢ₌₁"));
/// assert_eq!(fmt_script_text("q", ScriptKind::Superscript), None);
/// ```
pub fn fmt_script_text(text: &str, kind: ScriptKind) -> Option<String> {
    let table = table(kind);
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let c = unstyle_char(c);
        let mapped = table
            .iter()
            .find(|(src, _)| *src == c)
            .map(|(_, dst)| *dst)?;
        out.push(mapped);
    }
    Some(out)
}

/// Build the atom for a sub- or superscript whose argument has already been rendered
/// to `arg_text` (PLAN.md §9.5, v3 `_subsuperscript_formatter`).
///
/// The three-step ladder:
///
/// 1. **Unicode script characters.** `x^2` → `𝑥²`. The atom is
///    `(Script, Ord)`: a script binds to its base on the left, and on the right it is
///    an ordinary, visible thing.
/// 2. **Retry with the spaces taken out**, when `strip_joiner_spaces` is set. Unicode
///    has no script space, so a single space fails the whole lookup and `\sum_{i=1}^n`
///    would come out `∑_(𝑖 = 1)ⁿ` even though `ᵢ`, `₌` and `₁` all exist. Any space in
///    there was put in by the joiner — math mode drops the source's own whitespace — so
///    taking it back out simply undoes that, and `∑ᵢ₌₁ⁿ` shows the structure far
///    better. Pass `true` in `MathMode::Fancy`, where a joiner runs; in `Plain` mode no
///    joiner ever inserted a space, so there is nothing to undo.
/// 3. **LaTeX's own notation**, as a [wrappable](AtomBody::Wrappable) with `^` or `_`
///    as its prefix: `x^q`, and `x_(abc)` where the extent would otherwise be unclear.
///    The atom is `(Script, OpenEnd)` — nothing shows where the script ends — and it is
///    marked [`latex_notation`](ScriptInfo::latex_notation), which also makes the
///    joiner put a space in front of the *base*, turning `4π𝑐𝑥^𝑞𝑝` into `4π𝑐 𝑥^𝑞 𝑝`.
///
/// An empty argument (`$x^{}$`) produces an empty atom, which the joiner drops.
///
/// ```
/// use techxt::mathfmt::{script_atom, AtomClass, ScriptKind};
///
/// let s = script_atom("2", ScriptKind::Superscript, true);
/// assert_eq!(s.flatten(), "²");
/// assert_eq!(s.cls, (AtomClass::Script, AtomClass::Ord));
///
/// let fallback = script_atom("q", ScriptKind::Superscript, true);
/// assert_eq!(fallback.flatten(), "^q");
/// assert_eq!(fallback.cls, (AtomClass::Script, AtomClass::OpenEnd));
/// ```
pub fn script_atom(arg_text: &str, kind: ScriptKind, strip_joiner_spaces: bool) -> Atom {
    let arg = arg_text.trim();
    if arg.is_empty() {
        return Atom::from_text("", AtomClass::both(AtomClass::Ord));
    }

    let unicode = fmt_script_text(arg, kind).or_else(|| {
        if strip_joiner_spaces && arg.contains(' ') {
            let compact: String = arg.chars().filter(|c| *c != ' ').collect();
            fmt_script_text(&compact, kind)
        } else {
            None
        }
    });

    match unicode {
        Some(text) => Atom {
            body: AtomBody::Str(text.into_boxed_str()),
            cls: (AtomClass::Script, AtomClass::Ord),
            script: Some(ScriptInfo {
                kind,
                latex_notation: false,
            }),
        },
        None => Atom {
            body: AtomBody::Wrappable {
                inner: arg.to_string().into_boxed_str(),
                prefix: kind.latex_char().to_string().into_boxed_str(),
            },
            cls: (AtomClass::Script, AtomClass::OpenEnd),
            script: Some(ScriptInfo {
                kind,
                latex_notation: true,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn tables_have_the_documented_sizes_and_no_duplicate_source() {
        assert_eq!(SUPERSCRIPT_CHARS.len(), 68);
        assert_eq!(SUBSCRIPT_CHARS.len(), 40);
        for table in [SUPERSCRIPT_CHARS, SUBSCRIPT_CHARS] {
            let mut seen: Vec<char> = Vec::new();
            for (src, _) in table {
                assert!(!seen.contains(src), "{src:?} listed twice");
                seen.push(*src);
            }
        }
    }

    #[test]
    fn the_documented_holes_are_really_holes() {
        assert_eq!(fmt_script_text("q", ScriptKind::Superscript), None);
        for c in ["C", "F", "Q", "S", "X", "Y", "Z"] {
            assert_eq!(fmt_script_text(c, ScriptKind::Superscript), None, "{c}");
        }
        // no capital has a subscript
        for c in 'A'..='Z' {
            assert_eq!(
                fmt_script_text(&alloc::string::String::from(c), ScriptKind::Subscript),
                None,
                "{c}"
            );
        }
    }

    #[test]
    fn literal_renderings() {
        assert_eq!(
            fmt_script_text("10", ScriptKind::Superscript).as_deref(),
            Some("¹⁰")
        );
        assert_eq!(
            fmt_script_text("-1", ScriptKind::Superscript).as_deref(),
            Some("⁻¹")
        );
        assert_eq!(
            fmt_script_text("a+b", ScriptKind::Superscript).as_deref(),
            Some("ᵃ⁺ᵇ")
        );
        assert_eq!(
            fmt_script_text("AB", ScriptKind::Superscript).as_deref(),
            Some("ᴬᴮ")
        );
        assert_eq!(
            fmt_script_text("n+1", ScriptKind::Subscript).as_deref(),
            Some("ₙ₊₁")
        );
        assert_eq!(
            fmt_script_text("max", ScriptKind::Subscript).as_deref(),
            Some("ₘₐₓ")
        );
        // the minus sign and the hyphen agree
        assert_eq!(
            fmt_script_text("\u{2212}", ScriptKind::Subscript),
            fmt_script_text("-", ScriptKind::Subscript)
        );
    }

    #[test]
    fn styled_letters_are_normalized_before_the_lookup() {
        // 𝐚 (bold) and 𝑖 (italic) are looked up as 'a' and 'i'
        assert_eq!(
            fmt_script_text("𝐚", ScriptKind::Superscript).as_deref(),
            Some("ᵃ")
        );
        assert_eq!(
            fmt_script_text("𝑖", ScriptKind::Subscript).as_deref(),
            Some("ᵢ")
        );
    }

    #[test]
    fn the_lookup_is_all_or_nothing() {
        assert_eq!(fmt_script_text("aqb", ScriptKind::Superscript), None);
        assert_eq!(fmt_script_text("a b", ScriptKind::Superscript), None);
    }

    #[test]
    fn the_space_stripping_retry_is_opt_in() {
        let with = script_atom("𝑖 = 1", ScriptKind::Subscript, true);
        assert_eq!(with.flatten(), "ᵢ₌₁");
        assert_eq!(with.cls, (AtomClass::Script, AtomClass::Ord));

        let without = script_atom("𝑖 = 1", ScriptKind::Subscript, false);
        assert_eq!(without.flatten(), "_𝑖 = 1");
        assert_eq!(without.cls, (AtomClass::Script, AtomClass::OpenEnd));
    }

    #[test]
    fn an_empty_script_renders_as_nothing() {
        let a = script_atom("   ", ScriptKind::Superscript, true);
        assert_eq!(a.flatten(), "");
        assert!(a.script.is_none());
    }

    #[test]
    fn script_metadata_records_the_notation_used() {
        let unicode = script_atom("2", ScriptKind::Superscript, true);
        assert_eq!(
            unicode.script,
            Some(ScriptInfo {
                kind: ScriptKind::Superscript,
                latex_notation: false
            })
        );
        let fallback = script_atom("N", ScriptKind::Subscript, true);
        assert_eq!(
            fallback.script,
            Some(ScriptInfo {
                kind: ScriptKind::Subscript,
                latex_notation: true
            })
        );
        assert_eq!(fallback.flatten(), "_N");
    }
}
