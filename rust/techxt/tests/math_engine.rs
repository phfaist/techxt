//! The math engine's behaviour as the plan and the porting notes state it, exercised
//! through the public API with hand-built atoms (no parser, no renderer).
//!
//! The scenarios come from PLAN.md §9.5 and §15 and from the coverage checklist of the
//! pylatexenc porting notes: the seven spacing rules, unary minus, the script ladder
//! and its binding order, lazy delimiters, matrices, and the display-math box join.

use techxt::mathfmt::{
    baseline_join, classify_plain, display_matrix_atom, display_matrix_box, fmt_script_text,
    fmt_style, frac_atom, inline_matrix_atom, join_atoms, join_atoms_with, script_atom,
    segment_plain, sqrt_atom, unstyle, Atom, AtomBody, AtomClass, FontStyle, FontStyleKind,
    MathBox, MathWrapDelims, MatrixDelims, MatrixFence, ScriptKind,
};

/// Join for inline math with the default delimiters, as a single string.
fn join(atoms: Vec<Atom>) -> String {
    join_atoms(atoms, false).join_lines("\n")
}

/// Join for display math, keeping the line structure.
fn join_display(atoms: Vec<Atom>) -> Vec<String> {
    join_atoms(atoms, true)
        .lines
        .iter()
        .map(|l| l.to_string())
        .collect()
}

/// A finished-text atom with one class on both edges.
fn a(text: &str, cls: AtomClass) -> Atom {
    Atom::from_text(text, cls.both())
}

/// Plain text as it reaches the joiner with a math alphabet in force.
fn seg(s: &str) -> Vec<Atom> {
    segment_plain(s, true)
}

/// Math-italic styling, which is what the default `Options::math_font` does to the
/// document's own characters.
fn it(s: &str) -> String {
    fmt_style(s, FontStyleKind::Italic)
}

// ---------------------------------------------------------------------------
// PLAN.md §15 — the normative examples the joiner can answer on its own
// ---------------------------------------------------------------------------

#[test]
fn plan_example_10_scripts_and_a_binary_operator() {
    // $x^2 + y_i$  →  𝑥² + 𝑦ᵢ
    let mut atoms = seg(&it("x"));
    atoms.push(script_atom("2", ScriptKind::Superscript, true));
    atoms.extend(seg("+"));
    atoms.extend(seg(&it("y")));
    atoms.push(script_atom(&it("i"), ScriptKind::Subscript, true));
    assert_eq!(join(atoms), "𝑥² + 𝑦ᵢ");
}

#[test]
fn plan_example_11_a_fraction_before_a_named_operator() {
    // $\frac{4\pi c}{2}\sin(x+y)$  →  (4π𝑐)/2 sin(𝑥 + 𝑦)
    let numerator = String::from("4π") + &it("c");
    let mut atoms = vec![
        frac_atom(&numerator, "2", &MathWrapDelims::Parens),
        a("sin", AtomClass::Op),
    ];
    atoms.extend(seg(&format!("({}+{})", it("x"), it("y"))));
    assert_eq!(join(atoms), "(4π𝑐)/2 sin(𝑥 + 𝑦)");
}

#[test]
fn plan_example_12_a_sum_with_both_scripts() {
    // $\sum_{i=1}^n x_i$  →  ∑ᵢ₌₁ⁿ 𝑥ᵢ
    // the subscript's argument is a formula of its own, already joined: `𝑖 = 1`
    let inner = join(seg(&format!("{}=1", it("i"))));
    assert_eq!(inner, "𝑖 = 1");

    let mut atoms = seg("∑");
    atoms.push(script_atom(&inner, ScriptKind::Subscript, true));
    atoms.push(script_atom(&it("n"), ScriptKind::Superscript, true));
    atoms.extend(seg(&it("x")));
    atoms.push(script_atom(&it("i"), ScriptKind::Subscript, true));
    assert_eq!(join(atoms), "∑ᵢ₌₁ⁿ 𝑥ᵢ");
}

#[test]
fn plan_example_23_display_matrix_delimiter_styles() {
    let rows = [["1", "2"], ["30", "4"]];

    let unicode = display_matrix_box(&rows, MatrixFence::Parens, MatrixDelims::Unicode);
    assert_eq!(unicode.lines[0].as_ref(), "⎛  1  2 ⎞");
    assert_eq!(unicode.lines[1].as_ref(), "⎝ 30  4 ⎠");

    let ascii = display_matrix_box(&rows, MatrixFence::Parens, MatrixDelims::Ascii);
    assert_eq!(ascii.lines[0].as_ref(), "(  1  2 )");
    assert_eq!(ascii.lines[1].as_ref(), "( 30  4 )");
}

// ---------------------------------------------------------------------------
// Segmentation
// ---------------------------------------------------------------------------

#[test]
fn segmentation_classifies_each_kind_of_character() {
    let texts: Vec<String> = seg("sin(3.5+x)≤∑,").iter().map(|a| a.flatten()).collect();
    assert_eq!(texts, ["sin", "(", "3.5", "+", "x", ")", "≤", "∑", ","]);

    let classes: Vec<AtomClass> = seg("sin(3.5+x)≤∑,").iter().map(|a| a.cls.0).collect();
    assert_eq!(
        classes,
        [
            AtomClass::Op,
            AtomClass::Open,
            AtomClass::Ord,
            AtomClass::Bin,
            AtomClass::Ord,
            AtomClass::Close,
            AtomClass::Rel,
            AtomClass::Op,
            AtomClass::Punct,
        ]
    );
}

#[test]
fn the_two_unclassified_characters_stay_ordinary() {
    assert_eq!(classify_plain("|", true), (AtomClass::Ord, AtomClass::Ord));
    assert_eq!(classify_plain("/", true), (AtomClass::Ord, AtomClass::Ord));
    // which is what keeps a/b tight
    assert_eq!(join(seg(&format!("{}/{}", it("a"), it("b")))), "𝑎/𝑏");
}

// ---------------------------------------------------------------------------
// The joiner
// ---------------------------------------------------------------------------

#[test]
fn implicit_multiplication_and_source_whitespace() {
    // `$4\pi c$`, `$4 \pi c$` and `$  4  \pi  c  $` all reach the joiner the same way:
    // math mode drops the source's whitespace before the atoms are built
    assert_eq!(join(seg(&format!("4π{}", it("c")))), "4π𝑐");
    assert_eq!(join(seg(&format!("2{}{}", it("x"), it("y")))), "2𝑥𝑦");
}

#[test]
fn text_fragments_are_opaque_and_never_doubly_spaced() {
    let atoms = vec![
        a("if ", AtomClass::Text),
        a("𝑥", AtomClass::Ord),
        a(">", AtomClass::Rel),
        a("0", AtomClass::Ord),
    ];
    assert_eq!(join(atoms), "if 𝑥 > 0");

    let atoms = vec![
        a("𝑥", AtomClass::Ord),
        a(" if ", AtomClass::Text),
        a("𝑦", AtomClass::Ord),
    ];
    assert_eq!(join(atoms), "𝑥 if 𝑦");
}

#[test]
fn an_operator_name_keeps_its_interior_space_as_one_atom() {
    // `\limsup` renders "lim sup" as a single Op atom, not as text to be re-segmented
    let mut atoms = vec![a("lim sup", AtomClass::Op)];
    atoms.push(script_atom(&it("n"), ScriptKind::Subscript, true));
    atoms.extend(seg(&it("x")));
    atoms.push(script_atom(&it("n"), ScriptKind::Subscript, true));
    assert_eq!(join(atoms), "lim supₙ 𝑥ₙ");
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

#[test]
fn the_script_ladder_step_by_step() {
    // 1. unicode characters
    assert_eq!(
        script_atom("2", ScriptKind::Superscript, true).flatten(),
        "²"
    );
    // 1'. after the math-alphabet normalization
    assert_eq!(
        script_atom(
            &fmt_style("a", FontStyleKind::Bold),
            ScriptKind::Superscript,
            true
        )
        .flatten(),
        "ᵃ"
    );
    // 2. after the joiner's spaces are taken back out
    assert_eq!(
        script_atom("𝑖 = 1", ScriptKind::Subscript, true).flatten(),
        "ᵢ₌₁"
    );
    // 3. LaTeX notation, because unicode has no superscript q
    let fallback = script_atom(&it("q"), ScriptKind::Superscript, true);
    assert_eq!(fallback.flatten(), "^𝑞");
    assert!(matches!(fallback.body, AtomBody::Wrappable { .. }));
}

#[test]
fn a_fallback_script_pushes_a_space_in_front_of_its_base() {
    // $4\pi c x^q p$  →  4π𝑐 𝑥^𝑞 𝑝
    let mut atoms = seg(&format!("4π{}{}", it("c"), it("x")));
    atoms.push(script_atom(&it("q"), ScriptKind::Superscript, true));
    atoms.extend(seg(&it("p")));
    assert_eq!(join(atoms), "4π𝑐 𝑥^𝑞 𝑝");

    // ... but a unicode script needs no such padding
    let mut atoms = seg(&format!("4π{}{}", it("c"), it("x")));
    atoms.push(script_atom("2", ScriptKind::Superscript, true));
    atoms.extend(seg(&it("p")));
    assert_eq!(join(atoms), "4π𝑐𝑥²𝑝");
}

#[test]
fn the_subscript_binds_first() {
    // $x^a_b$  →  𝑥_𝑏ᵃ : the superscript and the subscript are swapped
    let mut atoms = seg(&it("x"));
    atoms.push(script_atom(&it("a"), ScriptKind::Superscript, true));
    atoms.push(script_atom(&it("b"), ScriptKind::Subscript, true));
    assert_eq!(join(atoms), "𝑥_𝑏ᵃ");
}

#[test]
fn a_script_presents_the_class_of_its_base() {
    // ∑ is a large operator, so `∑ᵢ 𝑦` keeps the space the operator calls for
    let mut atoms = seg("∑");
    atoms.push(script_atom(&it("i"), ScriptKind::Subscript, true));
    atoms.extend(seg(&it("y")));
    assert_eq!(join(atoms), "∑ᵢ 𝑦");
}

#[test]
fn an_empty_script_disappears() {
    let mut atoms = seg(&it("x"));
    atoms.push(script_atom("", ScriptKind::Superscript, true));
    assert_eq!(join(atoms), "𝑥");
}

// ---------------------------------------------------------------------------
// Wrappables — the four design cases of a lazy delimiter
// ---------------------------------------------------------------------------

#[test]
fn lazy_delimiters_appear_only_when_a_space_would_not_do() {
    let base = || seg(&it("x"));
    let sub = || script_atom(&it("abc"), ScriptKind::Subscript, true);

    // nothing follows: bare
    assert_eq!(
        join({
            let mut v = base();
            v.push(sub());
            v
        }),
        "𝑥_𝑎𝑏𝑐"
    );

    // an ordinary neighbour: a space suffices
    assert_eq!(
        join({
            let mut v = base();
            v.push(sub());
            v.extend(seg(&it("p")));
            v
        }),
        "𝑥_𝑎𝑏𝑐 𝑝"
    );

    // a relation already separates
    assert_eq!(
        join({
            let mut v = base();
            v.push(sub());
            v.extend(seg("=1"));
            v
        }),
        "𝑥_𝑎𝑏𝑐 = 1"
    );

    // a second script on the same base: delimiters
    assert_eq!(
        join({
            let mut v = base();
            v.push(sub());
            v.push(script_atom("2", ScriptKind::Superscript, true));
            v
        }),
        "𝑥_(𝑎𝑏𝑐)²"
    );
}

#[test]
fn the_delimiter_pair_is_configurable() {
    let atoms = || {
        let mut v = seg(&it("x"));
        v.push(script_atom(&it("ab"), ScriptKind::Subscript, true));
        v.push(script_atom("2", ScriptKind::Superscript, true));
        v
    };
    let text = |delims: MathWrapDelims| {
        join_atoms_with(atoms(), false, &delims)
            .math_box
            .join_lines("\n")
    };
    assert_eq!(text(MathWrapDelims::Parens), "𝑥_(𝑎𝑏)²");
    assert_eq!(text(MathWrapDelims::Braces), "𝑥_{𝑎𝑏}²");
    assert_eq!(
        text(MathWrapDelims::Custom("[".into(), "]".into())),
        "𝑥_[𝑎𝑏]²"
    );
    assert_eq!(text(MathWrapDelims::None), "𝑥_𝑎𝑏²");
}

// ---------------------------------------------------------------------------
// Roots and fractions
// ---------------------------------------------------------------------------

#[test]
fn a_root_closes_on_the_left_and_opens_on_the_right() {
    let root = || sqrt_atom(&it("x"), None, &MathWrapDelims::Parens);
    assert_eq!(join(vec![a("2", AtomClass::Ord), root()]), "2√𝑥");
    assert_eq!(join(vec![root(), a("2", AtomClass::Ord)]), "√𝑥 2");
    assert_eq!(
        sqrt_atom(&it("x"), Some("3"), &MathWrapDelims::Parens).flatten(),
        "∛𝑥"
    );
}

#[test]
fn a_fraction_is_wrapped_eagerly_not_lazily() {
    // DECISIONS.md D1: `$\frac{a}{b}c$` is `𝑎/𝑏 𝑐`, never `(𝑎/𝑏)𝑐`
    let atoms = vec![
        frac_atom(&it("a"), &it("b"), &MathWrapDelims::Parens),
        a(&it("c"), AtomClass::Ord),
    ];
    assert_eq!(join(atoms), "𝑎/𝑏 𝑐");
}

// ---------------------------------------------------------------------------
// Matrices
// ---------------------------------------------------------------------------

#[test]
fn an_inline_matrix_is_opaque_and_self_delimiting() {
    let matrix = || inline_matrix_atom(&[["1", "2"]], MatrixFence::Parens);
    assert_eq!(matrix().flatten(), "( 1  2 )");

    // it hugs its neighbours on both sides ...
    assert_eq!(
        join(vec![
            a("𝑥", AtomClass::Ord),
            matrix(),
            a("𝑦", AtomClass::Ord)
        ]),
        "𝑥( 1  2 )𝑦"
    );
    // ... including a function name on the left
    assert_eq!(join(vec![a("sin", AtomClass::Op), matrix()]), "sin( 1  2 )");
}

#[test]
fn a_matrix_base_needs_no_retroactive_space() {
    let atoms = vec![
        a("2", AtomClass::Ord),
        inline_matrix_atom(&[["1", "2"]], MatrixFence::Brackets),
        script_atom(&it("qr"), ScriptKind::Superscript, true),
    ];
    assert_eq!(join(atoms), "2[ 1  2 ]^𝑞𝑟");
}

#[test]
fn inline_matrix_delimiters_come_from_the_environment() {
    let rows = [["1", "2"]];
    let text =
        |env: &str| inline_matrix_atom(&rows, MatrixFence::for_environment(env).unwrap()).flatten();
    assert_eq!(text("matrix"), "1  2");
    assert_eq!(text("smallmatrix"), "1  2");
    assert_eq!(text("pmatrix"), "( 1  2 )");
    assert_eq!(text("bmatrix"), "[ 1  2 ]");
    assert_eq!(text("array"), "[ 1  2 ]");
    assert_eq!(text("Bmatrix"), "{ 1  2 }");
    assert_eq!(text("cases"), "{ 1  2 }");
    assert_eq!(text("rcases*"), "{ 1  2 }");
    assert_eq!(text("vmatrix"), "| 1  2 |");
    assert_eq!(text("Vmatrix"), "‖ 1  2 ‖");
}

#[test]
fn a_multi_row_inline_matrix_uses_semicolons() {
    let m = inline_matrix_atom(&[["1", "2"], ["30", "4"]], MatrixFence::Brackets);
    assert_eq!(m.flatten(), "[  1  2; 30  4 ]");
}

#[test]
fn an_empty_matrix_renders_and_does_not_panic() {
    let empty: [[&str; 0]; 0] = [];
    assert_eq!(
        inline_matrix_atom(&empty, MatrixFence::Brackets).flatten(),
        "[ ]"
    );
    let display = display_matrix_box(&empty, MatrixFence::Brackets, MatrixDelims::Unicode);
    assert_eq!(display.lines.len(), 1);
    assert_eq!(display.lines[0].as_ref(), "[ ]");
    // and an empty matrix inside a formula simply disappears from the join
    assert_eq!(
        join(vec![
            a("𝑥", AtomClass::Ord),
            inline_matrix_atom(&empty, MatrixFence::None)
        ]),
        "𝑥"
    );
}

#[test]
fn a_one_row_display_matrix_uses_the_plain_delimiters() {
    let b = display_matrix_box(&[["1", "2"]], MatrixFence::Parens, MatrixDelims::Unicode);
    assert_eq!(b.lines, vec!["( 1  2 )".into()]);
    assert_eq!(b.baseline, 0);
}

#[test]
fn display_matrix_baseline_is_the_middle_row() {
    for rows in 1..6usize {
        let cells: Vec<[&str; 1]> = (0..rows).map(|_| ["1"]).collect();
        let b = display_matrix_box(&cells, MatrixFence::Brackets, MatrixDelims::Unicode);
        assert_eq!(b.lines.len(), rows);
        assert_eq!(b.baseline, (rows - 1) / 2, "{rows} rows");
    }
}

#[test]
fn display_matrices_compose_by_baseline_join() {
    let matrix = display_matrix_atom(
        &[["1", "2"], ["30", "4"]],
        MatrixFence::Parens,
        MatrixDelims::Unicode,
    );
    let atoms = vec![a("𝐴", AtomClass::Ord), a("=", AtomClass::Rel), matrix];
    assert_eq!(
        join_display(atoms),
        vec!["𝐴 = ⎛  1  2 ⎞".to_string(), "    ⎝ 30  4 ⎠".to_string()]
    );
}

#[test]
fn an_inline_join_never_produces_more_than_one_line() {
    // a display box that reached an inline join is squeezed onto one line rather than
    // breaking the caller's single-line contract
    let matrix = display_matrix_atom(&[["1"], ["2"]], MatrixFence::Brackets, MatrixDelims::Ascii);
    let joined = join_atoms(vec![matrix], false);
    assert_eq!(joined.lines.len(), 1);
}

#[test]
fn baseline_join_pads_the_off_baseline_rows_to_the_same_width() {
    let left = MathBox {
        lines: vec!["ab".into(), "c".into()],
        baseline: 0,
    };
    let right = MathBox {
        lines: vec!["X".into(), "YZ".into()],
        baseline: 0,
    };
    let joined = baseline_join(&[left, right], &[" "]);
    assert_eq!(joined.lines[0].as_ref(), "ab X");
    assert_eq!(joined.lines[1].as_ref(), "c  YZ");
    // no line ever ends in whitespace
    for line in &joined.lines {
        assert!(!line.ends_with(' '), "{line:?}");
    }
}

// ---------------------------------------------------------------------------
// Font alphabets
// ---------------------------------------------------------------------------

#[test]
fn every_alphabet_maps_and_unmaps_the_whole_latin_range() {
    let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    for kind in [
        FontStyleKind::Bold,
        FontStyleKind::Italic,
        FontStyleKind::BoldItalic,
        FontStyleKind::Script,
        FontStyleKind::BoldScript,
        FontStyleKind::Fraktur,
        FontStyleKind::DoubleStruck,
        FontStyleKind::BoldFraktur,
        FontStyleKind::SansSerif,
        FontStyleKind::SansSerifBold,
        FontStyleKind::SansSerifItalic,
        FontStyleKind::SansSerifBoldItalic,
        FontStyleKind::Monospace,
        FontStyleKind::Upright,
    ] {
        let styled = fmt_style(letters, kind);
        assert_eq!(styled.chars().count(), letters.chars().count(), "{kind:?}");
        assert_eq!(unstyle(&styled), letters, "{kind:?}");
    }
}

#[test]
fn the_golden_alphabets_of_the_porting_notes() {
    assert_eq!(fmt_style("abc", FontStyleKind::Bold), "𝐚𝐛𝐜");
    assert_eq!(fmt_style("h", FontStyleKind::Italic), "ℎ");
    assert_eq!(fmt_style("BEFHILMR", FontStyleKind::Script), "ℬℰℱℋℐℒℳℛ");
    assert_eq!(fmt_style("CHIRZ", FontStyleKind::Fraktur), "ℭℌℑℜℨ");
    assert_eq!(fmt_style("CHNPQRZ", FontStyleKind::DoubleStruck), "ℂℍℕℙℚℝℤ");
    assert_eq!(fmt_style("xy", FontStyleKind::Monospace), "𝚡𝚢");
    // digits and greek are not styled: there is no such alphabet
    assert_eq!(fmt_style("123 α", FontStyleKind::Bold), "123 α");
    // and Upright is the identity
    assert_eq!(fmt_style("dx", FontStyleKind::Upright), "dx");
}

#[test]
fn the_upright_letters_are_op_flag_follows_the_font() {
    // DECISIONS.md D4: the flag is true exactly when a style is in effect
    assert!(FontStyle::Style(FontStyleKind::Italic).is_style());
    assert!(FontStyle::Style(FontStyleKind::Upright).is_style());
    assert!(!FontStyle::Default.is_style());
    assert!(!FontStyle::Disabled.is_style());

    // with an alphabet: `xy` is read as a function name; without: two variables
    assert_eq!(join(segment_plain("xy", true)), "xy");
    assert_eq!(
        segment_plain("xy", true)[0].cls,
        (AtomClass::Op, AtomClass::Op)
    );
    assert_eq!(segment_plain("xy", false).len(), 2);
}

#[test]
fn a_script_survives_a_font_macro() {
    // $x^{\mathbf{a}}$ → 𝑥ᵃ : the styled letter is normalized back to ASCII
    assert_eq!(
        fmt_script_text(
            &fmt_style("a", FontStyleKind::Bold),
            ScriptKind::Superscript
        )
        .as_deref(),
        Some("ᵃ")
    );
}
