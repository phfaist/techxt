//! The core of mathematical notation (PLAN.md §9.5): Greek letters, operators and
//! relations, `\frac`, `\sqrt`, brackets, arrows, dots and delimiters.
//!
//! Most of this category is one line per symbol, and those lines do more than they look
//! like they do. A symbol's replacement text is *segmented* on its way into a formula
//! ([`segment_plain`](crate::mathfmt::segment_plain)), so choosing U+2264 `≤` for
//! `\leq` is also choosing that the joiner will space it as a relation, and choosing
//! U+2223 `∣` for `\mid` rather than an ASCII bar is what makes `$x \mid y$` read
//! `𝑥 ∣ 𝑦` instead of running together. The character *is* the class.
//!
//! # What cannot be a plain symbol
//!
//! Three kinds of entry declare their atom class themselves instead, because no
//! character carries it:
//!
//! - **operator names** (`\sin`, `\limsup`) are [`Op`](crate::mathfmt::AtomClass::Op)
//!   atoms of literal text. Segmentation would nearly do it — a run of upright latin
//!   letters reads as a function name — but only while variables are italicized, and
//!   `\limsup`'s `lim sup` would come apart at its space. An explicit atom holds
//!   whatever the options are;
//! - **`\frac` and `\sqrt`** decide from their own operands whether those need
//!   delimiters, and what they present to their neighbours depends on the answer
//!   ([`frac_atom`], [`sqrt_atom`]);
//! - **`\operatorname{…}`** renders its argument upright and then declares it an
//!   operator, which is exactly what the named operators above are shorthand for.
//!
//! # Fonts do not reach any of it
//!
//! None of the text this module produces is styled, because none of it is document
//! text (DECISIONS.md D6): `\sin` stays an upright `sin` next to an italic `𝑥`, and
//! `\alpha` stays `α`. Only the characters an author typed go through an alphabet.

use alloc::format;
use alloc::string::String;

use techy::core::node::NodeRef;
use techy::latexlike::Latexlike;

use crate::def::{Category, MacroDef, Template, TextHandler, TextRule};
use crate::flow::Flow;
use crate::mathfmt::{
    frac_atom, script_atom, sqrt_atom, Atom, AtomClass, FontStyle, FontStyleKind, ScriptKind,
};
use crate::render::{math, RenderCx, RenderError, RenderState};

use super::handler;

/// The mathcore category (PLAN.md §12.1).
///
/// ```
/// use techxt::Converter;
///
/// let converter = Converter::standard();
/// assert_eq!(
///     converter.latex_to_text(r"$\frac{4\pi c}{2}\sin(x+y)$")?.text,
///     "(4π𝑐)/2 sin(𝑥 + 𝑦)\n"
/// );
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
pub fn category() -> Category {
    let mut category = Category::new("mathcore");
    greek(&mut category);
    symbols(&mut category);
    operator_names(&mut category);
    fractions_and_roots(&mut category);
    stacked(&mut category);
    delimiters(&mut category);
    styles(&mut category);
    category
}

// --------------------------------------------------------------------------- greek

/// The Greek alphabet, in the four spellings LaTeX and its packages use.
///
/// Each letter yields `\alpha` and `\Alpha`, plus the `\upalpha`/`\Upalpha` forms that
/// `upgreek` and `newtxmath` define for the upright shape — which plain text cannot
/// distinguish from the ordinary one, so both give the same character.
///
/// Note which codepoints these are: `\epsilon` is the *lunate* epsilon `ϵ` and
/// `\varepsilon` the ordinary `ε`, `\phi` is the phi *symbol* `ϕ` and `\varphi` the
/// ordinary `φ`. That is LaTeX's own pairing, and reversing it would silently change
/// every physics paper that converts through techxt.
fn greek(category: &mut Category) {
    for (name, small, capital) in GREEK_LETTERS {
        category.add_macro(MacroDef::symbol(*name, *small));
        category.add_macro(MacroDef::symbol(capitalized(name), *capital));
        category.add_macro(MacroDef::symbol(format!("up{name}"), *small));
        category.add_macro(MacroDef::symbol(format!("Up{name}"), *capital));
    }
    for (name, character) in GREEK_VARIANTS {
        category.add_macro(MacroDef::symbol(*name, *character));
        category.add_macro(MacroDef::symbol(format!("up{name}"), *character));
    }
}

/// `name` with its first character in upper case, which is how LaTeX spells the capital
/// of a Greek letter.
fn capitalized(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
    }
    out.push_str(chars.as_str());
    out
}

/// The twenty-four letters, small and capital.
const GREEK_LETTERS: &[(&str, &str, &str)] = &[
    ("alpha", "\u{3b1}", "\u{391}"),   // α Α
    ("beta", "\u{3b2}", "\u{392}"),    // β Β
    ("gamma", "\u{3b3}", "\u{393}"),   // γ Γ
    ("delta", "\u{3b4}", "\u{394}"),   // δ Δ
    ("epsilon", "\u{3f5}", "\u{395}"), // ϵ Ε  (the lunate epsilon; see above)
    ("zeta", "\u{3b6}", "\u{396}"),    // ζ Ζ
    ("eta", "\u{3b7}", "\u{397}"),     // η Η
    ("theta", "\u{3b8}", "\u{398}"),   // θ Θ
    ("iota", "\u{3b9}", "\u{399}"),    // ι Ι
    ("kappa", "\u{3ba}", "\u{39a}"),   // κ Κ
    ("lambda", "\u{3bb}", "\u{39b}"),  // λ Λ
    ("mu", "\u{3bc}", "\u{39c}"),      // μ Μ
    ("nu", "\u{3bd}", "\u{39d}"),      // ν Ν
    ("xi", "\u{3be}", "\u{39e}"),      // ξ Ξ
    ("omicron", "\u{3bf}", "\u{39f}"), // ο Ο
    ("pi", "\u{3c0}", "\u{3a0}"),      // π Π
    ("rho", "\u{3c1}", "\u{3a1}"),     // ρ Ρ
    ("sigma", "\u{3c3}", "\u{3a3}"),   // σ Σ
    ("tau", "\u{3c4}", "\u{3a4}"),     // τ Τ
    ("upsilon", "\u{3c5}", "\u{3a5}"), // υ Υ
    ("phi", "\u{3d5}", "\u{3a6}"),     // ϕ Φ  (the phi symbol; see above)
    ("chi", "\u{3c7}", "\u{3a7}"),     // χ Χ
    ("psi", "\u{3c8}", "\u{3a8}"),     // ψ Ψ
    ("omega", "\u{3c9}", "\u{3a9}"),   // ω Ω
];

/// The variant shapes, which have no capital form.
const GREEK_VARIANTS: &[(&str, &str)] = &[
    ("varepsilon", "\u{3b5}"), // ε  GREEK SMALL LETTER EPSILON
    ("vartheta", "\u{3d1}"),   // ϑ  GREEK THETA SYMBOL
    ("varpi", "\u{3d6}"),      // ϖ  GREEK PI SYMBOL
    ("varrho", "\u{3f1}"),     // ϱ  GREEK RHO SYMBOL
    ("varsigma", "\u{3c2}"),   // ς  GREEK SMALL LETTER FINAL SIGMA
    ("varphi", "\u{3c6}"),     // φ  GREEK SMALL LETTER PHI
];

// ------------------------------------------------------------------------- symbols

/// Every symbol that is one character and needs no class of its own.
fn symbols(category: &mut Category) {
    for (name, replacement) in SYMBOLS {
        category.add_macro(MacroDef::symbol(*name, *replacement));
    }
}

/// The curated symbol table (see the module documentation on why the character is the
/// class).
const SYMBOLS: &[(&str, &str)] = &[
    // Binary operators. Every one of them is in `segment_plain`'s `Bin` table,
    // so the joiner spaces them: `$a \pm b$` reads `𝑎 ± 𝑏`.
    ("pm", "\u{b1}"),                // ±  PLUS-MINUS SIGN
    ("mp", "\u{2213}"),              // ∓  MINUS-OR-PLUS SIGN
    ("times", "\u{d7}"),             // ×  MULTIPLICATION SIGN
    ("div", "\u{f7}"),               // ÷  DIVISION SIGN
    ("cdot", "\u{b7}"),              // ·  MIDDLE DOT
    ("ast", "\u{2217}"),             // ∗  ASTERISK OPERATOR
    ("star", "\u{22c6}"),            // ⋆  STAR OPERATOR
    ("circ", "\u{2218}"),            // ∘  RING OPERATOR
    ("bullet", "\u{2219}"),          // ∙  BULLET OPERATOR
    ("diamond", "\u{22c4}"),         // ⋄  DIAMOND OPERATOR
    ("cap", "\u{2229}"),             // ∩  INTERSECTION
    ("cup", "\u{222a}"),             // ∪  UNION
    ("sqcap", "\u{2293}"),           // ⊓  SQUARE CAP
    ("sqcup", "\u{2294}"),           // ⊔  SQUARE CUP
    ("vee", "\u{2228}"),             // ∨  LOGICAL OR
    ("lor", "\u{2228}"),             // ∨  LOGICAL OR
    ("wedge", "\u{2227}"),           // ∧  LOGICAL AND
    ("land", "\u{2227}"),            // ∧  LOGICAL AND
    ("setminus", "\u{2216}"),        // ∖  SET MINUS
    ("smallsetminus", "\u{2216}"),   // ∖  SET MINUS
    ("wr", "\u{2240}"),              // ≀  WREATH PRODUCT
    ("bigtriangleup", "\u{25b3}"),   // △  WHITE UP-POINTING TRIANGLE
    ("bigtriangledown", "\u{25bd}"), // ▽  WHITE DOWN-POINTING TRIANGLE
    ("triangleleft", "\u{25c3}"),    // ◃  WHITE LEFT-POINTING SMALL TRIANGLE
    ("triangleright", "\u{25b9}"),   // ▹  WHITE RIGHT-POINTING SMALL TRIANGLE
    ("oplus", "\u{2295}"),           // ⊕  CIRCLED PLUS
    ("ominus", "\u{2296}"),          // ⊖  CIRCLED MINUS
    ("otimes", "\u{2297}"),          // ⊗  CIRCLED TIMES
    ("oslash", "\u{2298}"),          // ⊘  CIRCLED DIVISION SLASH
    ("odot", "\u{2299}"),            // ⊙  CIRCLED DOT OPERATOR
    ("dagger", "\u{2020}"),          // †  DAGGER
    ("ddagger", "\u{2021}"),         // ‡  DOUBLE DAGGER
    ("amalg", "\u{2a3f}"),           // ⨿  AMALGAMATION OR COPRODUCT
    ("uplus", "\u{228e}"),           // ⊎  MULTISET UNION
    // Relations, which the joiner spaces most widely of all. `\mid` is U+2223
    // DIVIDES rather than a plain `|`: the ASCII bar is deliberately unclassified
    // (it is a delimiter as often as a relation), and this one is not.
    ("leq", "\u{2264}"),        // ≤  LESS-THAN OR EQUAL TO
    ("le", "\u{2264}"),         // ≤  LESS-THAN OR EQUAL TO
    ("geq", "\u{2265}"),        // ≥  GREATER-THAN OR EQUAL TO
    ("ge", "\u{2265}"),         // ≥  GREATER-THAN OR EQUAL TO
    ("neq", "\u{2260}"),        // ≠  NOT EQUAL TO
    ("ne", "\u{2260}"),         // ≠  NOT EQUAL TO
    ("equiv", "\u{2261}"),      // ≡  IDENTICAL TO
    ("sim", "\u{223c}"),        // ∼  TILDE OPERATOR
    ("simeq", "\u{2243}"),      // ≃  ASYMPTOTICALLY EQUAL TO
    ("asymp", "\u{224d}"),      // ≍  EQUIVALENT TO
    ("approx", "\u{2248}"),     // ≈  ALMOST EQUAL TO
    ("cong", "\u{2245}"),       // ≅  APPROXIMATELY EQUAL TO
    ("doteq", "\u{2250}"),      // ≐  APPROACHES THE LIMIT
    ("propto", "\u{221d}"),     // ∝  PROPORTIONAL TO
    ("models", "\u{22a8}"),     // ⊨  TRUE
    ("prec", "\u{227a}"),       // ≺  PRECEDES
    ("succ", "\u{227b}"),       // ≻  SUCCEEDS
    ("preceq", "\u{227c}"),     // ≼  PRECEDES OR EQUAL TO
    ("succeq", "\u{227d}"),     // ≽  SUCCEEDS OR EQUAL TO
    ("ll", "\u{226a}"),         // ≪  MUCH LESS-THAN
    ("gg", "\u{226b}"),         // ≫  MUCH GREATER-THAN
    ("subset", "\u{2282}"),     // ⊂  SUBSET OF
    ("supset", "\u{2283}"),     // ⊃  SUPERSET OF
    ("subseteq", "\u{2286}"),   // ⊆  SUBSET OF OR EQUAL TO
    ("supseteq", "\u{2287}"),   // ⊇  SUPERSET OF OR EQUAL TO
    ("sqsubset", "\u{228f}"),   // ⊏  SQUARE IMAGE OF
    ("sqsupset", "\u{2290}"),   // ⊐  SQUARE ORIGINAL OF
    ("sqsubseteq", "\u{2291}"), // ⊑  SQUARE IMAGE OF OR EQUAL TO
    ("sqsupseteq", "\u{2292}"), // ⊒  SQUARE ORIGINAL OF OR EQUAL TO
    ("in", "\u{2208}"),         // ∈  ELEMENT OF
    ("ni", "\u{220b}"),         // ∋  CONTAINS AS MEMBER
    ("owns", "\u{220b}"),       // ∋  CONTAINS AS MEMBER
    ("notin", "\u{2209}"),      // ∉  NOT AN ELEMENT OF
    ("vdash", "\u{22a2}"),      // ⊢  RIGHT TACK
    ("dashv", "\u{22a3}"),      // ⊣  LEFT TACK
    ("perp", "\u{22a5}"),       // ⊥  UP TACK
    ("mid", "\u{2223}"),        // ∣  DIVIDES
    ("parallel", "\u{2225}"),   // ∥  PARALLEL TO
    ("bowtie", "\u{22c8}"),     // ⋈  BOWTIE
    ("smile", "\u{2323}"),      // ⌣  SMILE
    ("frown", "\u{2322}"),      // ⌢  FROWN
    ("subsetneq", "\u{228a}"),  // ⊊  SUBSET OF WITH NOT EQUAL TO
    ("supsetneq", "\u{228b}"),  // ⊋  SUPERSET OF WITH NOT EQUAL TO
    ("nsubseteq", "\u{2288}"),  // ⊈  NEITHER A SUBSET OF NOR EQUAL TO
    ("nleq", "\u{2270}"),       // ≰  NEITHER LESS-THAN NOR EQUAL TO
    ("ngeq", "\u{2271}"),       // ≱  NEITHER GREATER-THAN NOR EQUAL TO
    ("lesssim", "\u{2272}"),    // ≲  LESS-THAN OR EQUIVALENT TO
    ("gtrsim", "\u{2273}"),     // ≳  GREATER-THAN OR EQUIVALENT TO
    ("colon", "\u{3a}"),        // :  COLON
    // Large operators, which take their limits as scripts and bind them tightly:
    // `$\sum_{i=1}^n x_i$` reads `∑ᵢ₌₁ⁿ 𝑥ᵢ`. `\bigoplus` and friends are the
    // n-ary U+2A0x characters, not the binary `⊕` — only the former are `Op`.
    ("sum", "\u{2211}"),       // ∑  N-ARY SUMMATION
    ("prod", "\u{220f}"),      // ∏  N-ARY PRODUCT
    ("coprod", "\u{2210}"),    // ∐  N-ARY COPRODUCT
    ("int", "\u{222b}"),       // ∫  INTEGRAL
    ("iint", "\u{222c}"),      // ∬  DOUBLE INTEGRAL
    ("iiint", "\u{222d}"),     // ∭  TRIPLE INTEGRAL
    ("oint", "\u{222e}"),      // ∮  CONTOUR INTEGRAL
    ("oiint", "\u{222f}"),     // ∯  SURFACE INTEGRAL
    ("bigcap", "\u{22c2}"),    // ⋂  N-ARY INTERSECTION
    ("bigcup", "\u{22c3}"),    // ⋃  N-ARY UNION
    ("bigsqcup", "\u{2a06}"),  // ⨆  N-ARY SQUARE UNION OPERATOR
    ("bigvee", "\u{22c1}"),    // ⋁  N-ARY LOGICAL OR
    ("bigwedge", "\u{22c0}"),  // ⋀  N-ARY LOGICAL AND
    ("bigoplus", "\u{2a01}"),  // ⨁  N-ARY CIRCLED PLUS OPERATOR
    ("bigotimes", "\u{2a02}"), // ⨂  N-ARY CIRCLED TIMES OPERATOR
    ("bigodot", "\u{2a00}"),   // ⨀  N-ARY CIRCLED DOT OPERATOR
    ("biguplus", "\u{2a04}"),  // ⨄  N-ARY UNION OPERATOR WITH PLUS
    // Arrows, which are relations.
    ("to", "\u{2192}"),                 // →  RIGHTWARDS ARROW
    ("rightarrow", "\u{2192}"),         // →  RIGHTWARDS ARROW
    ("gets", "\u{2190}"),               // ←  LEFTWARDS ARROW
    ("leftarrow", "\u{2190}"),          // ←  LEFTWARDS ARROW
    ("leftrightarrow", "\u{2194}"),     // ↔  LEFT RIGHT ARROW
    ("Rightarrow", "\u{21d2}"),         // ⇒  RIGHTWARDS DOUBLE ARROW
    ("Leftarrow", "\u{21d0}"),          // ⇐  LEFTWARDS DOUBLE ARROW
    ("Leftrightarrow", "\u{21d4}"),     // ⇔  LEFT RIGHT DOUBLE ARROW
    ("mapsto", "\u{21a6}"),             // ↦  RIGHTWARDS ARROW FROM BAR
    ("longmapsto", "\u{27fc}"),         // ⟼  LONG RIGHTWARDS ARROW FROM BAR
    ("longrightarrow", "\u{27f6}"),     // ⟶  LONG RIGHTWARDS ARROW
    ("longleftarrow", "\u{27f5}"),      // ⟵  LONG LEFTWARDS ARROW
    ("longleftrightarrow", "\u{27f7}"), // ⟷  LONG LEFT RIGHT ARROW
    ("Longrightarrow", "\u{27f9}"),     // ⟹  LONG RIGHTWARDS DOUBLE ARROW
    ("Longleftarrow", "\u{27f8}"),      // ⟸  LONG LEFTWARDS DOUBLE ARROW
    ("Longleftrightarrow", "\u{27fa}"), // ⟺  LONG LEFT RIGHT DOUBLE ARROW
    ("implies", "\u{27f9}"),            // ⟹  LONG RIGHTWARDS DOUBLE ARROW
    ("impliedby", "\u{27f8}"),          // ⟸  LONG LEFTWARDS DOUBLE ARROW
    ("iff", "\u{27fa}"),                // ⟺  LONG LEFT RIGHT DOUBLE ARROW
    ("uparrow", "\u{2191}"),            // ↑  UPWARDS ARROW
    ("downarrow", "\u{2193}"),          // ↓  DOWNWARDS ARROW
    ("updownarrow", "\u{2195}"),        // ↕  UP DOWN ARROW
    ("Uparrow", "\u{21d1}"),            // ⇑  UPWARDS DOUBLE ARROW
    ("Downarrow", "\u{21d3}"),          // ⇓  DOWNWARDS DOUBLE ARROW
    ("Updownarrow", "\u{21d5}"),        // ⇕  UP DOWN DOUBLE ARROW
    ("nearrow", "\u{2197}"),            // ↗  NORTH EAST ARROW
    ("searrow", "\u{2198}"),            // ↘  SOUTH EAST ARROW
    ("swarrow", "\u{2199}"),            // ↙  SOUTH WEST ARROW
    ("nwarrow", "\u{2196}"),            // ↖  NORTH WEST ARROW
    ("hookleftarrow", "\u{21a9}"),      // ↩  LEFTWARDS ARROW WITH HOOK
    ("hookrightarrow", "\u{21aa}"),     // ↪  RIGHTWARDS ARROW WITH HOOK
    ("rightharpoonup", "\u{21c0}"),     // ⇀  RIGHTWARDS HARPOON WITH BARB UPWARDS
    ("rightharpoondown", "\u{21c1}"),   // ⇁  RIGHTWARDS HARPOON WITH BARB DOWNWARDS
    ("leftharpoonup", "\u{21bc}"),      // ↼  LEFTWARDS HARPOON WITH BARB UPWARDS
    ("leftharpoondown", "\u{21bd}"),    // ↽  LEFTWARDS HARPOON WITH BARB DOWNWARDS
    ("rightleftharpoons", "\u{21cc}"),  // ⇌  RIGHTWARDS HARPOON OVER LEFTWARDS HARPOON
    ("leadsto", "\u{21dd}"),            // ⇝  RIGHTWARDS SQUIGGLE ARROW
    // Ordinary symbols: they join tightly, like a variable.
    ("infty", "\u{221e}"),         // ∞  INFINITY
    ("partial", "\u{2202}"),       // ∂  PARTIAL DIFFERENTIAL
    ("nabla", "\u{2207}"),         // ∇  NABLA
    ("forall", "\u{2200}"),        // ∀  FOR ALL
    ("exists", "\u{2203}"),        // ∃  THERE EXISTS
    ("nexists", "\u{2204}"),       // ∄  THERE DOES NOT EXIST
    ("neg", "\u{ac}"),             // ¬  NOT SIGN
    ("lnot", "\u{ac}"),            // ¬  NOT SIGN
    ("emptyset", "\u{2205}"),      // ∅  EMPTY SET
    ("varnothing", "\u{2205}"),    // ∅  EMPTY SET
    ("aleph", "\u{2135}"),         // ℵ  ALEF SYMBOL
    ("hbar", "\u{210f}"),          // ℏ  PLANCK CONSTANT OVER TWO PI
    ("hslash", "\u{210f}"),        // ℏ  PLANCK CONSTANT OVER TWO PI
    ("ell", "\u{2113}"),           // ℓ  SCRIPT SMALL L
    ("Re", "\u{211c}"),            // ℜ  BLACK-LETTER CAPITAL R
    ("Im", "\u{2111}"),            // ℑ  BLACK-LETTER CAPITAL I
    ("wp", "\u{2118}"),            // ℘  SCRIPT CAPITAL P
    ("prime", "\u{2032}"),         // ′  PRIME
    ("angle", "\u{2220}"),         // ∠  ANGLE
    ("measuredangle", "\u{2221}"), // ∡  MEASURED ANGLE
    ("triangle", "\u{25b3}"),      // △  WHITE UP-POINTING TRIANGLE
    ("surd", "\u{221a}"),          // √  SQUARE ROOT
    ("top", "\u{22a4}"),           // ⊤  DOWN TACK
    ("bot", "\u{22a5}"),           // ⊥  UP TACK
    ("flat", "\u{266d}"),          // ♭  MUSIC FLAT SIGN
    ("natural", "\u{266e}"),       // ♮  MUSIC NATURAL SIGN
    ("sharp", "\u{266f}"),         // ♯  MUSIC SHARP SIGN
    ("clubsuit", "\u{2663}"),      // ♣  BLACK CLUB SUIT
    ("diamondsuit", "\u{2662}"),   // ♢  WHITE DIAMOND SUIT
    ("heartsuit", "\u{2661}"),     // ♡  WHITE HEART SUIT
    ("spadesuit", "\u{2660}"),     // ♠  BLACK SPADE SUIT
    ("imath", "\u{131}"),          // ı  LATIN SMALL LETTER DOTLESS I
    ("jmath", "\u{237}"),          // ȷ  LATIN SMALL LETTER DOTLESS J
    ("Box", "\u{25a1}"),           // □  WHITE SQUARE
    ("square", "\u{25a1}"),        // □  WHITE SQUARE
    ("Diamond", "\u{25ca}"),       // ◊  LOZENGE
    ("checkmark", "\u{2713}"),     // ✓  CHECK MARK
    ("complement", "\u{2201}"),    // ∁  COMPLEMENT
    ("degree", "\u{b0}"),          // °  DEGREE SIGN
    ("circledR", "\u{ae}"),        // ®  REGISTERED SIGN
    ("therefore", "\u{2234}"),     // ∴  THEREFORE
    ("because", "\u{2235}"),       // ∵  BECAUSE
    // Dots. `\ldots` and `\dots` are text symbols and live in
    // [`defs::base`](super::base); these three are math-only.
    ("cdots", "\u{22ef}"), // ⋯  MIDLINE HORIZONTAL ELLIPSIS
    ("vdots", "\u{22ee}"), // ⋮  VERTICAL ELLIPSIS
    ("ddots", "\u{22f1}"), // ⋱  DOWN RIGHT DIAGONAL ELLIPSIS
    // Delimiter characters. The braces, brackets and parentheses need no entry:
    // they are written as themselves, and `\{`/`\}` are escapes
    // [`defs::base`](super::base) already provides.
    ("langle", "\u{27e8}"),    // ⟨  MATHEMATICAL LEFT ANGLE BRACKET
    ("rangle", "\u{27e9}"),    // ⟩  MATHEMATICAL RIGHT ANGLE BRACKET
    ("lfloor", "\u{230a}"),    // ⌊  LEFT FLOOR
    ("rfloor", "\u{230b}"),    // ⌋  RIGHT FLOOR
    ("lceil", "\u{2308}"),     // ⌈  LEFT CEILING
    ("rceil", "\u{2309}"),     // ⌉  RIGHT CEILING
    ("llbracket", "\u{27e6}"), // ⟦  MATHEMATICAL LEFT WHITE SQUARE BRACKET
    ("rrbracket", "\u{27e7}"), // ⟧  MATHEMATICAL RIGHT WHITE SQUARE BRACKET
    ("Vert", "\u{2016}"),      // ‖  DOUBLE VERTICAL LINE
    ("lVert", "\u{2016}"),     // ‖  DOUBLE VERTICAL LINE
    ("rVert", "\u{2016}"),     // ‖  DOUBLE VERTICAL LINE
];

// ------------------------------------------------------------- operator names

/// The named operators (`\sin`, `\lim`, `\gcd`) and the generic `\operatorname`.
///
/// Each is an [`Op`](AtomClass::Op) atom carrying its own text, which is what keeps
/// `\limsup`'s interior space intact — `$\limsup_n x_n$` reads `lim supₙ 𝑥ₙ`, one atom
/// with a space in it rather than two atoms that happen to be adjacent.
///
/// The `\var…` forms are alternative typesettings — a bar or an arrow instead of the
/// spelled-out words — that plain text cannot show, so each renders as the operator it
/// stands for.
fn operator_names(category: &mut Category) {
    for (name, text) in OPERATOR_NAMES {
        category.add_macro(MacroDef::new(*name).rule(handler(FixedAtom {
            text,
            cls: AtomClass::both(AtomClass::Op),
        })));
    }

    // `\operatorname*{…}` differs only in where the operator's limits are set, which
    // plain text cannot show either way. This shadows `defs::fontstyles`'s entry, which
    // gets the font right but cannot give the result its class.
    category.add_macro(
        MacroDef::new("operatorname")
            .star()
            .arg("m", OPERATOR_TEXT)
            .rule(handler(OperatorName)),
    );

    // The modulo family. `\bmod` is infix and wants a space on each side, so it is a
    // binary operator; the other two open with the word `mod` and are operators.
    category.add_macro(MacroDef::new("bmod").rule(handler(FixedAtom {
        text: "mod",
        cls: AtomClass::both(AtomClass::Bin),
    })));
    category.add_macro(
        MacroDef::new("pmod")
            .arg("m", MODULUS)
            .rule(handler(Modulo {
                parenthesized: true,
            })),
    );
    category.add_macro(MacroDef::new("mod").arg("m", MODULUS).rule(handler(Modulo {
        parenthesized: false,
    })));
}

/// The argument name `\operatorname` gives its operator, matching
/// [`defs::fontstyles`](super::fontstyles) so that either definition can be overridden
/// without the other's handler losing its argument.
const OPERATOR_TEXT: &str = "text";

/// The argument name the modulo macros give their modulus.
const MODULUS: &str = "modulus";

/// v3's `_math_operator_names`, complete.
const OPERATOR_NAMES: &[(&str, &str)] = &[
    ("cos", "cos"),
    ("sin", "sin"),
    ("tan", "tan"),
    ("sec", "sec"),
    ("csc", "csc"),
    ("cot", "cot"),
    ("arccos", "arccos"),
    ("arcsin", "arcsin"),
    ("arctan", "arctan"),
    ("cosh", "cosh"),
    ("sinh", "sinh"),
    ("tanh", "tanh"),
    ("coth", "coth"),
    ("arccosh", "arccosh"),
    ("arcsinh", "arcsinh"),
    ("arctanh", "arctanh"),
    ("ln", "ln"),
    ("lg", "lg"),
    ("log", "log"),
    ("exp", "exp"),
    ("max", "max"),
    ("min", "min"),
    ("sup", "sup"),
    ("inf", "inf"),
    ("lim", "lim"),
    ("limsup", "lim sup"),
    ("liminf", "lim inf"),
    ("injlim", "inj lim"),
    ("projlim", "proj lim"),
    ("varlimsup", "lim sup"),
    ("varliminf", "lim inf"),
    ("varinjlim", "inj lim"),
    ("varprojlim", "proj lim"),
    ("arg", "arg"),
    ("deg", "deg"),
    ("det", "det"),
    ("dim", "dim"),
    ("gcd", "gcd"),
    ("hom", "hom"),
    ("ker", "ker"),
    ("Pr", "Pr"),
];

// -------------------------------------------------------- fractions and roots

/// `\frac` and `\sqrt`, and the aliases that render identically.
fn fractions_and_roots(category: &mut Category) {
    // `\dfrac`/`\tfrac` only choose display or text size, and `\cfrac` only sets a
    // continued fraction more openly; plain text has one size and one shape.
    for name in ["frac", "dfrac", "tfrac", "cfrac", "nicefrac", "textfrac"] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", NUMERATOR)
                .arg("m", DENOMINATOR)
                .rule(handler(Fraction)),
        );
    }
    category.add_macro(
        MacroDef::new("sqrt")
            .arg("o", INDEX)
            .arg("m", RADICAND)
            .rule(handler(Root)),
    );
}

/// The binomial coefficients and the stacking macros (amsmath).
///
/// `\binom{n}{k}` has no plain-text notation of its own, so it renders as TeX's own
/// name for it: `(𝑛 choose 𝑘)`. The parentheses are what a binomial coefficient is
/// written with and the word is what the primitive `\choose` is called, which together
/// say exactly what the two operands are without inventing a notation the reader has
/// to guess at.
///
/// `\overset`, `\underset` and `\stackrel` set one expression over or under another,
/// and plain text has exactly one way to put something above or below a baseline:
/// unicode's script characters. So the annotation is rendered as a **script** on the
/// base — `\stackrel{\mathrm{def}}{=}` becomes `=ᵈᵉᶠ` — through the same all-or-nothing
/// ladder `^` and `_` run (PLAN.md §9.5), which falls back to `^`/`_` notation for an
/// annotation unicode cannot show. Nothing is dropped either way.
fn stacked(category: &mut Category) {
    for name in ["binom", "dbinom", "tbinom"] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", TOP)
                .arg("m", BOTTOM)
                .rule(TextRule::Template(Template::new("({top} choose {bottom})"))),
        );
    }
    for (name, kind) in [
        ("overset", ScriptKind::Superscript),
        ("stackrel", ScriptKind::Superscript),
        ("underset", ScriptKind::Subscript),
    ] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", ANNOTATION)
                .arg("m", BASE)
                .rule(handler(Stacked(kind))),
        );
    }
    // `\substack` stacks the lines of a limit (`\sum_{\substack{i<j \\ i,j \in S}}`).
    // Its body is a formula whose `\\` the inline joiner already renders as a space,
    // which is what a stack of conditions reads as on one line.
    category.add_macro(
        MacroDef::new("substack")
            .arg("m", "lines")
            .rule(TextRule::Content),
    );
}

/// `\binom`'s two argument names.
const TOP: &str = "top";
/// See [`TOP`].
const BOTTOM: &str = "bottom";
/// `\overset`'s two argument names: the thing set over the base, and the base.
const ANNOTATION: &str = "annotation";
/// See [`ANNOTATION`].
const BASE: &str = "base";

// -------------------------------------------------------------- style declarations

/// The math declarations that choose a *size* or a *layout*, and render as nothing.
///
/// `\displaystyle` and its three siblings pick the size of what follows; `\limits` and
/// `\nolimits` decide whether an operator's limits go above it or beside it;
/// `\nonumber` and `\notag` suppress an equation number techxt never printed. Plain
/// text has one size, one place for a script and no equation numbers, so all of them
/// are declared — so that they are not reported as unknown — and render as nothing.
fn styles(category: &mut Category) {
    for name in [
        "displaystyle",
        "textstyle",
        "scriptstyle",
        "scriptscriptstyle",
        "limits",
        "nolimits",
        "nonumber",
        "notag",
    ] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }
    // `\intertext{…}` is ordinary prose written between two lines of a display; the
    // prose is content, and it is all that is left of it.
    category.add_macro(
        MacroDef::new("intertext")
            .arg("m", "text")
            .rule(TextRule::Content),
    );
}

/// `\frac`'s two argument names.
const NUMERATOR: &str = "numerator";
/// See [`NUMERATOR`].
const DENOMINATOR: &str = "denominator";
/// `\sqrt`'s two argument names (PLAN.md §9.5).
const INDEX: &str = "index";
/// See [`INDEX`].
const RADICAND: &str = "radicand";

// ---------------------------------------------------------------- delimiters

/// The delimiter-sizing macros, which take the delimiter as their argument.
///
/// Plain text has one size, so all that is left to do is render the delimiter the macro
/// was pointed at — and to drop `.`, which is LaTeX's way of writing "no delimiter on
/// this side" in `\left. … \right)`.
fn delimiters(category: &mut Category) {
    for name in [
        "left", "right", "middle", "big", "Big", "bigg", "Bigg", "bigl", "Bigl", "biggl", "Biggl",
        "bigr", "Bigr", "biggr", "Biggr", "bigm", "Bigm", "biggm", "Biggm",
    ] {
        category.add_macro(
            MacroDef::new(name)
                .arg("m", DELIMITER)
                .rule(handler(SizedDelimiter)),
        );
    }
    // `\vert` and `\lvert`/`\rvert` are the ASCII bar, deliberately: it is what an
    // absolute value looks like, and `segment_plain` leaves it unclassified precisely
    // because it is a delimiter as often as a relation.
    for name in ["vert", "lvert", "rvert"] {
        category.add_macro(MacroDef::symbol(name, "|"));
    }
    // `\|` is the double bar, and `\backslash` the character `\` itself.
    category.add_macro(MacroDef::symbol("|", "\u{2016}"));
    category.add_macro(MacroDef::symbol("backslash", "\\"));
    // The bracket macros, for the two brackets that are also LaTeX syntax.
    category.add_macro(MacroDef::symbol("lbrack", "["));
    category.add_macro(MacroDef::symbol("rbrack", "]"));
    category.add_macro(MacroDef::symbol("lbrace", "{"));
    category.add_macro(MacroDef::symbol("rbrace", "}"));
}

/// The argument name the sizing macros give their delimiter.
const DELIMITER: &str = "delimiter";

// ------------------------------------------------------------------ handlers

/// A construct whose rendering is fixed text of a fixed atom class: an operator name,
/// `\bmod`.
#[derive(Debug)]
struct FixedAtom {
    /// The text, which is never styled — it is not document text (DECISIONS.md D6).
    text: &'static str,
    /// What it presents to its neighbours on each side.
    cls: (AtomClass, AtomClass),
}

impl TextHandler for FixedAtom {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let atom = Atom::from_text(self.text, self.cls);
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}

/// `\operatorname{…}` (PLAN.md §9.5).
///
/// Two things at once, and both are needed: the name is set upright, and the result is
/// declared an operator so that `$2\operatorname{tr}A$` reads `2 tr 𝐴`.
#[derive(Debug)]
struct OperatorName;

impl TextHandler for OperatorName {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let upright = upright_state(cx);
        let rendered = cx
            .arg_with_state(OPERATOR_TEXT, upright)?
            .unwrap_or_default();
        let text = math::inline_text(&rendered, cx.state(), cx.options());
        let atom = Atom::from_text(text, AtomClass::both(AtomClass::Op));
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}

/// The current state with the mode-appropriate font stack set upright (DECISIONS.md D7).
///
/// [`FontStyle::Disabled`] is left alone: it is the option-level off switch, and no
/// macro may turn alphabet mapping back on.
fn upright_state(cx: &RenderCx<'_, '_>) -> RenderState {
    let mut state = cx.state().clone();
    let stack = if state.in_math() {
        &mut state.math_font
    } else {
        &mut state.text_font
    };
    if *stack != FontStyle::Disabled {
        *stack = FontStyle::Style(FontStyleKind::Upright);
    }
    state
}

/// `\pmod{n}` → `(mod 𝑛)` and `\mod{n}` → `mod 𝑛`.
#[derive(Debug)]
struct Modulo {
    /// Whether the whole thing is parenthesized, which also closes it off on the right.
    parenthesized: bool,
}

impl TextHandler for Modulo {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let rendered = cx.arg(MODULUS)?.unwrap_or_default();
        let modulus = math::inline_text(&rendered, cx.state(), cx.options());
        let (text, cls) = if self.parenthesized {
            (format!("(mod {modulus})"), AtomClass::Close)
        } else {
            (format!("mod {modulus}"), AtomClass::Ord)
        };
        let atom = Atom::from_text(text, (AtomClass::Op, cls));
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}

/// `\overset{…}{…}`, `\underset{…}{…}` and `\stackrel{…}{…}`.
///
/// The base is emitted first and the annotation follows it as a script atom, which is
/// exactly the shape `x^2` has — so the joiner binds the two the way it binds any base
/// and script, and an annotation unicode cannot set falls back to `^`/`_` notation
/// instead of being lost.
#[derive(Debug)]
struct Stacked(ScriptKind);

impl TextHandler for Stacked {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // Declaration order, as everywhere: the fold's side effects happen while a
        // region is folded, and the annotation is written first.
        let annotation = cx.arg(ANNOTATION)?.unwrap_or_default();
        let base = cx.arg(BASE)?.unwrap_or_default();
        let annotation = math::inline_text(&annotation, cx.state(), cx.options());
        let atom = script_atom(
            &annotation,
            self.0,
            math::atoms_in_use(cx.state(), cx.options()),
        );
        let mut flow = base;
        flow.extend(math::atom(atom, cx.state(), cx.options()));
        Ok(flow)
    }
}

/// `\frac{…}{…}` and its aliases (PLAN.md §9.5, DECISIONS.md D1).
///
/// The delimiters go on **eagerly**, around whichever operand needs them, and the result
/// is finished text. A fraction knows by itself that `(𝑎+𝑏)/𝑐` needs parentheses — the
/// reason is internal, not a matter of what follows — so deferring the decision would
/// wrap the whole fraction instead of its operand and turn `$\frac{a}{b}c$` into
/// `(𝑎/𝑏)𝑐` rather than the correct `𝑎/𝑏 𝑐`.
#[derive(Debug)]
struct Fraction;

impl TextHandler for Fraction {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        // Declaration order: the fold's side effects happen while a region is folded.
        let numerator = cx.arg(NUMERATOR)?.unwrap_or_default();
        let denominator = cx.arg(DENOMINATOR)?.unwrap_or_default();
        let numerator = math::inline_text(&numerator, cx.state(), cx.options());
        let denominator = math::inline_text(&denominator, cx.state(), cx.options());
        let atom = frac_atom(&numerator, &denominator, &cx.options().math_expression_in);
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}

/// `\sqrt[index]{radicand}` (PLAN.md §9.5).
#[derive(Debug)]
struct Root;

impl TextHandler for Root {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let index = cx.arg(INDEX)?;
        let radicand = cx.arg(RADICAND)?.unwrap_or_default();
        let index = index.map(|flow| math::inline_text(&flow, cx.state(), cx.options()));
        let radicand = math::inline_text(&radicand, cx.state(), cx.options());
        let atom = sqrt_atom(
            &radicand,
            index.as_deref(),
            &cx.options().math_expression_in,
        );
        Ok(math::atom(atom, cx.state(), cx.options()))
    }
}

/// `\left(`, `\bigl[`, `\right.` — render the delimiter, forget the size.
#[derive(Debug)]
struct SizedDelimiter;

impl TextHandler for SizedDelimiter {
    fn render(
        &self,
        _node: NodeRef<'_, Latexlike>,
        cx: &mut RenderCx<'_, '_>,
    ) -> Result<Flow, RenderError> {
        let rendered = cx.arg(DELIMITER)?.unwrap_or_default();
        // `\left.` and `\right.` mark a side that has no delimiter at all.
        if math::inline_text(&rendered, cx.state(), cx.options()) == "." {
            return Ok(Flow::new());
        }
        // Handed back as it came: the delimiter's own atom already carries the class
        // that makes a bracket hug what it encloses.
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use crate::Converter;

    fn text(latex: &str) -> alloc::string::String {
        Converter::standard()
            .latex_to_text(latex)
            .expect("parses")
            .text
    }

    #[test]
    fn the_greek_spellings_all_resolve() {
        // Small, capital, and the two upright-package forms of each.
        assert_eq!(text(r"$\alpha\Alpha\upalpha\Upalpha$"), "αΑαΑ\n");
        // The var-shapes, and LaTeX's own pairing of the two epsilons and phis.
        assert_eq!(text(r"$\epsilon\varepsilon\phi\varphi$"), "ϵεϕφ\n");
    }

    #[test]
    fn a_symbols_character_is_also_its_class() {
        // `≤` is in the relation table, so the joiner spaces it; `α` is not.
        assert_eq!(text(r"$a\leq b$"), "𝑎 ≤ 𝑏\n");
        assert_eq!(text(r"$4\pi c$"), "4π𝑐\n");
        assert_eq!(text(r"$x\mid y$"), "𝑥 ∣ 𝑦\n");
    }

    #[test]
    fn an_operator_name_is_one_atom_even_when_it_contains_a_space() {
        assert_eq!(text(r"$\limsup_n x_n$"), "lim supₙ 𝑥ₙ\n");
        assert_eq!(text(r"$\projlim A$"), "proj lim 𝐴\n");
        // The `\var…` spellings render as the operator they stand for.
        assert_eq!(text(r"$\varprojlim A$"), text(r"$\projlim A$"));
    }

    #[test]
    fn the_modulo_family() {
        assert_eq!(text(r"$x\bmod y$"), "𝑥 mod 𝑦\n");
        assert_eq!(text(r"$a \equiv b \pmod{n}$"), "𝑎 ≡ 𝑏 (mod 𝑛)\n");
        assert_eq!(text(r"$a \equiv b \mod{n}$"), "𝑎 ≡ 𝑏 mod 𝑛\n");
    }

    #[test]
    fn a_sizing_macro_keeps_the_delimiter_and_drops_the_size() {
        assert_eq!(text(r"$\left[a\right]$"), "[𝑎]\n");
        // `.` is LaTeX's "no delimiter on this side".
        assert_eq!(text(r"$\left.a\right)$"), "𝑎)\n");
    }
}
