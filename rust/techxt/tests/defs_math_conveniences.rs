//! `\abs`, `\norm` and `\coloneqq`: three curated conveniences in `defs::mathcore`
//! (root PLAN.md §9.5).
//!
//! The library has defined `\ket`, `\bra`, `\braket` and `\ketbra` since it ported v3's
//! table, and defined none of these three. That is the bug under test — not that any
//! one macro was missing, but that a document writing `\abs{x}` beside `\ket{\psi}` got
//! one of them right and the other one bare, for no reason its author could state. So
//! the assertions here are mostly *comparisons*: `\abs` against the `\lvert … \rvert`
//! it is shorthand for, `\norm` against `\lVert … \rVert`, `\abs` against the `\ket`
//! that already worked, and `\coloneqq` against the bare `≔` that a plain replacement
//! would have produced.

use techxt::diag::UnknownMacro;
use techxt::Converter;

/// Convert with the shipped library and the default options.
fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

/// The names of the macros `latex` used that nothing renders.
fn unknown(latex: &str) -> Vec<String> {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .diagnostics
        .conditions::<UnknownMacro>()
        .map(|condition| condition.name.clone())
        .collect()
}

// ------------------------------------------------------------------ what they render as

#[test]
fn abs_and_norm_are_their_delimiter_pair_written_once() {
    assert_eq!(text(r"$\abs{x}$"), "|𝑥|\n");
    assert_eq!(text(r"$\norm{x}$"), "‖𝑥‖\n");
    // Which is the whole definition: the same bars this module already resolves the
    // two halves to, on both sides of the argument. Written out by hand it is the same
    // string, character for character — so nobody has to wonder which `|` came out.
    assert_eq!(text(r"$\abs{x}$"), text(r"$\lvert x \rvert$"));
    assert_eq!(text(r"$\norm{x}$"), text(r"$\lVert x \rVert$"));
    assert_eq!(text(r"$\abs{x}$"), text(r"$\vert x \vert$"));
    assert_eq!(text(r"$\norm{x}$"), text(r"$\Vert x \Vert$"));
}

#[test]
fn coloneqq_is_the_one_character_unicode_spells_it_with() {
    assert_eq!(text(r"$a \coloneqq b$"), "𝑎 ≔ 𝑏\n");
}

#[test]
fn the_bars_hold_a_whole_expression() {
    // The argument is rendered by the math engine and then wrapped, so everything the
    // engine does inside a formula still happens inside the bars.
    assert_eq!(
        text(r"$\abs{a+b} \leq \abs{a} + \abs{b}$"),
        "|𝑎 + 𝑏| ≤ |𝑎| + |𝑏|\n"
    );
    assert_eq!(text(r"$\norm{\abs{x}}$"), "‖|𝑥|‖\n");
    assert_eq!(text(r"$\abs{x}^2$"), "|𝑥|²\n");
    assert_eq!(text(r"$\norm{x}_2$"), "‖𝑥‖₂\n");
}

// ----------------------------------------------------------------- the atom classes

#[test]
fn coloneqq_is_spaced_as_a_relation() {
    // `≔` is not in `segment_plain`'s relation table — that table is a faithful port of
    // v3's, and this macro is not a reason to grow it — so a character typed literally
    // is an ordinary atom and comes out tight. The macro declares `Rel` for itself,
    // which is the entire difference between these two lines and the reason `\coloneqq`
    // is a handler rather than a row of `SYMBOLS`.
    assert_eq!(text("$a ≔ b$"), "𝑎≔𝑏\n");
    assert_eq!(text(r"$a \coloneqq b$"), "𝑎 ≔ 𝑏\n");
    // And a relation is spaced whether or not the source was.
    assert_eq!(text(r"$a\coloneqq b$"), "𝑎 ≔ 𝑏\n");
    assert_eq!(text(r"$x\coloneqq y+1$"), "𝑥 ≔ 𝑦 + 1\n");
    // The same spacing every other curated relation gets.
    assert_eq!(text(r"$a \leq b$"), "𝑎 ≤ 𝑏\n");
}

#[test]
fn the_bars_bind_as_tightly_as_the_ket_they_are_modelled_on() {
    // Ordinary on both edges, like the bar itself: no space to the number in front,
    // none between two of them.
    assert_eq!(text(r"$2\abs{x}$"), "2|𝑥|\n");
    assert_eq!(text(r"$\norm{u}\norm{v}$"), "‖𝑢‖‖𝑣‖\n");
    // An argument whose own extent is unclear pushes the bars apart, exactly as it
    // pushes `\ket`'s delimiters apart. Same mechanism, same answer.
    assert_eq!(text(r"$\abs{\frac{a}{b}}$"), "| 𝑎/𝑏 |\n");
    assert_eq!(text(r"$\ket{\frac{a}{b}}$"), "| 𝑎/𝑏 ⟩\n");
}

// ------------------------------------------------------------------------ the star

#[test]
fn the_star_physics_gives_them_is_absorbed() {
    // `physics` writes `\abs*{x}` for the `\left…\right` auto-sizing form. Plain text
    // has one size, so the star changes nothing — but it has to be *declared*, or the
    // mandatory argument's single-expression fallback reads the `*` as the argument
    // and prints `|*|𝑥`.
    assert_eq!(text(r"$\abs*{x}$"), text(r"$\abs{x}$"));
    assert_eq!(text(r"$\norm*{v}$"), text(r"$\norm{v}$"));
    assert_eq!(text(r"$\abs*{x}$"), "|𝑥|\n");
}

// -------------------------------------------------------- the shape that was the bug

#[test]
fn nothing_in_the_three_is_an_unknown_macro_any_more() {
    // Each of them warned `no text rule for the macro` and rendered its argument bare.
    assert_eq!(
        unknown(r"$\norm{x}$ $\abs{y}$ $a \coloneqq b$"),
        Vec::<String>::new()
    );
    assert_eq!(unknown(r"$\abs*{x}$ $\norm*{v}$"), Vec::<String>::new());
    // The check is worth something only if this helper can see a missing macro at all.
    assert_eq!(unknown(r"$\nosuchmacro{x}$"), ["nosuchmacro"]);
}

#[test]
fn a_document_may_mix_a_ket_with_an_abs() {
    // The Cauchy–Schwarz line: one macro the library has always defined and two it did
    // not, in one formula. This is what an author saw — `⟨ϕ|ψ⟩` typeset and the norms
    // silently reduced to `ϕψ` — and it is the reason all three were defined together
    // rather than one at a time.
    let latex = r"$\abs{\braket{\phi}{\psi}} \leq \norm{\phi} \norm{\psi}$";
    assert_eq!(text(latex), "|⟨ϕ|ψ⟩| ≤ ‖ϕ‖‖ψ‖\n");
    assert_eq!(unknown(latex), Vec::<String>::new());

    let definition = r"$\braket{\phi}{\psi} \coloneqq \sum_i \abs{c_i}^2$";
    assert_eq!(text(definition), "⟨ϕ|ψ⟩ ≔ ∑ᵢ |𝑐ᵢ|²\n");
    assert_eq!(unknown(definition), Vec::<String>::new());
}

// ---------------------------------------------------------------------- both modes

#[test]
fn none_of_the_three_is_restricted_to_math_mode() {
    // A mode restriction is a parse-side gate, and with the catch-all fallback
    // registered a hidden entry still resolves — as a *zero-argument* callable, whose
    // braces are then read as an ordinary group. A hidden `\abs` would print `||x`.
    // So an entry with arguments is never restricted, and `\coloneqq` follows the rest
    // of `mathcore`, none of which is restricted either.
    assert_eq!(text(r"\abs{x} and \norm{y}"), "|x| and ‖y‖\n");
    assert_eq!(unknown(r"\abs{x} \norm{y} \coloneqq"), Vec::<String>::new());
    // `\coloneqq` too — and it eats the space after it exactly as every other atom
    // `mathcore` builds does in a paragraph. That is the module's existing text-mode
    // behavior, not something these three brought with them.
    assert_eq!(text(r"a \coloneqq b"), "a ≔b\n");
    assert_eq!(text(r"a \leq b"), "a ≤b\n");
}
