//! The joiner: assembling a sequence of atoms into a finished formula.
//!
//! Rendering a formula is not concatenation. `4\pi c` has to read `4π𝑐`, tight, while
//! `4\pi c x^q p` has to read `4π𝑐 𝑥^𝑞 𝑝`; `x_{abc}` needs no delimiters but
//! `x_{abc}^2` does. What decides it is how neighbours meet, so the joiner works on
//! [`Atom`]s — text plus edge classes — rather than on strings.
//!
//! The join happens in **two passes**, because whether a piece encloses itself in
//! delimiters depends on its neighbours while its edge classes depend on whether it
//! did. Pass 1 has each piece realize itself against the classes its neighbours
//! *declare* — the classes they present when they do not wrap, which is the
//! conservative direction and always terminates. Pass 2 computes the separators from
//! the classes the pieces actually presented.

use alloc::{boxed::Box, string::String, vec::Vec};

use super::atom::{Atom, AtomBody, AtomClass, ScriptInfo, ScriptKind};
use super::boxes::{baseline_join, MathBox};
use super::segment::classify_plain;

/// The delimiters a sub-expression is wrapped in when its extent would otherwise be
/// unclear (`Options::math_expression_in`, PLAN.md §11.3).
///
/// **Seam for M5b:** `techxt::convert` does not exist yet, so this enum lives here and
/// the joiner takes it as a parameter ([`join_atoms_with`]). When `Options` lands,
/// `Options::math_expression_in` should be *this* type — re-exported from `convert` —
/// rather than a second copy of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MathWrapDelims {
    /// `(` … `)`, the default.
    #[default]
    Parens,
    /// `{` … `}`, which looks more like the LaTeX source.
    Braces,
    /// Any other pair, opening and closing.
    Custom(Box<str>, Box<str>),
    /// Never wrap: a space is all the separation a sub-expression will get.
    None,
}

impl MathWrapDelims {
    /// The opening and closing delimiter, or `None` when wrapping is off.
    pub fn pair(&self) -> Option<(&str, &str)> {
        match self {
            MathWrapDelims::Parens => Some(("(", ")")),
            MathWrapDelims::Braces => Some(("{", "}")),
            MathWrapDelims::Custom(open, close) => Some((open, close)),
            MathWrapDelims::None => None,
        }
    }

    /// Enclose `text` in these delimiters (v3 `fmt_math_expression_in_delimiters`).
    ///
    /// A sub-expression of a **single character** is returned unchanged: there is
    /// nothing about `x^𝑞` that delimiters would clarify, and this short-circuit is
    /// load-bearing — it is why `$x^a_b$` comes out `𝑥_𝑏ᵃ` with no parentheses in
    /// sight even though a second script attaches to the same base. Length is counted
    /// in characters, not bytes.
    ///
    /// ```
    /// use techxt::mathfmt::MathWrapDelims;
    ///
    /// assert_eq!(MathWrapDelims::Parens.wrap("a+b"), "(a+b)");
    /// assert_eq!(MathWrapDelims::Parens.wrap("𝑞"), "𝑞");
    /// assert_eq!(MathWrapDelims::None.wrap("a+b"), "a+b");
    /// ```
    pub fn wrap(&self, text: &str) -> String {
        match self.pair() {
            Some((open, close)) if text.chars().count() > 1 => {
                let mut out = String::with_capacity(open.len() + text.len() + close.len());
                out.push_str(open);
                out.push_str(text);
                out.push_str(close);
                out
            }
            _ => String::from(text),
        }
    }
}

/// A joined formula: the assembled box, plus the class pair it presents to its own
/// neighbours.
///
/// A join is itself joinable — a group inside a formula is joined first and then meets
/// the atoms around it — so the result carries the left-edge class of its first
/// surviving atom and the right-edge class of its last one.
#[derive(Clone, Debug)]
pub struct JoinedMath {
    /// The assembled text, one line for inline math and possibly several for display
    /// math containing a matrix.
    pub math_box: MathBox,
    /// What the whole assembly presents to its left and right neighbours.
    pub cls: (AtomClass, AtomClass),
}

/// A binary operator whose left neighbour presents one of these classes — or which has
/// no left neighbour at all — is really a *unary* operator.
const UNARY_LEFT_CLASSES: [AtomClass; 4] = [
    AtomClass::Bin,
    AtomClass::Rel,
    AtomClass::Open,
    AtomClass::Punct,
];

/// An atom whose right edge presents one of these already shows where it ends, so a
/// script attaching to it needs no retroactive space in front of its base.
const SELF_DELIMITED_CLASSES: [AtomClass; 2] = [AtomClass::Close, AtomClass::Block];

/// Whether two neighbouring edges have to be separated by a space (v3
/// `_math_needs_space`).
///
/// This is TeX's own inter-atom spacing table, collapsed from four widths down to two,
/// which is why it is this short and why it gives conventional-looking results. Unary
/// operators have already been turned into ordinary atoms by the caller, so this
/// function never sees one.
///
/// `left` is the **right edge** class of the piece on the left and `right` the **left
/// edge** class of the piece on the right. The seven rules, first match wins:
///
/// 1. Either side is a binary operator or a relation: `𝑎 + 𝑏`, `𝑥 = 𝑦`.
/// 2. Either side has no visible extent on that edge: `√𝑥 2`, `𝑥^𝑞 𝑝`.
/// 3. The left side is a large operator or a text fragment, *unless a delimiter
///    follows*: `sin 𝑥` but `sin(𝑥)`. A delimiter hugs what it encloses on both sides,
///    and a function name immediately followed by a *closing* delimiter essentially
///    never happens in real mathematics — where it does, no space is the better
///    rendering anyway. (Note this rule excludes both `Open` and `Close`, while rule 4
///    excludes only `Open`; the asymmetry is deliberate.)
/// 4. The right side is a large operator or a text fragment, unless an *opening*
///    delimiter precedes: `2 sin 𝑥` but `(sin 𝑥)`.
/// 5. The left side is punctuation — never merely because the right side is, so
///    `𝑓(𝑥, 𝑦)` gets its space after the comma and not before it.
/// 6. Both adjacent characters are ASCII digits, which would otherwise run together
///    into a single number: `{1}2` → `1 2`, while `{a}2` → `𝑎2` stays tight.
/// 7. Otherwise no space — implicit multiplication, `4π𝑐`.
fn needs_space(left: AtomClass, right: AtomClass, text_left: &str, text_right: &str) -> bool {
    use AtomClass::*;

    // A matrix carries its own delimiters and really does delimit itself on both
    // sides, so its left edge behaves exactly like an opening delimiter and its right
    // edge exactly like a closing one: `sin[ 1 2 ]` hugs the way `sin(x)` does. The
    // class is kept distinct all the same, so that a rule that wants to tell a block
    // from a delimiter still can.
    let left = if left == Block { Close } else { left };
    let right = if right == Block { Open } else { right };

    if matches!(left, Bin | Rel) || matches!(right, Bin | Rel) {
        return true; // 1
    }
    if left == OpenEnd || right == OpenEnd {
        return true; // 2
    }
    if matches!(left, Op | Text) && !matches!(right, Open | Close) {
        return true; // 3
    }
    if matches!(right, Op | Text) && left != Open {
        return true; // 4
    }
    if left == Punct {
        return true; // 5
    }
    match (text_left.chars().next_back(), text_right.chars().next()) {
        (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => true, // 6
        _ => false,                                                             // 7
    }
}

/// The separator to place between two neighbouring pieces (v3 `_math_separator`).
///
/// A space that is already there — at the end of the left rendering or at the start of
/// the right one — counts. That is what keeps `$\text{if } x > 0$` from ending up with
/// two spaces after the `if`, and it is where the crate's "no doubled spaces" invariant
/// comes from.
fn separator(left: AtomClass, right: AtomClass, text_left: &str, text_right: &str) -> &'static str {
    if text_left.ends_with(' ') || text_right.starts_with(' ') {
        return "";
    }
    if needs_space(left, right, text_left, text_right) {
        " "
    } else {
        ""
    }
}

/// Whether a [wrappable](AtomBody::Wrappable) body has to reach for its delimiters,
/// given what its right neighbour declares (v3 `_MathWrappablePiece.needs_wrapping`).
///
/// Deliberately short: wrap only when a separating space would still leave the extent
/// of the contents ambiguous.
///
/// 1. Another **script attaches to the same base** — `x_{abc}^2`, where a space would
///    not tell the subscript and the superscript apart.
/// 2. The contents **contain a space**, so a space around them would not be seen.
/// 3. The contents are themselves **open-ended**. Per DECISIONS.md D5 the inner class
///    is computed here rather than carried on the atom, by classifying the inner text;
///    a plain classification never yields `OpenEnd`, so in practice this arm is the
///    seam a future non-default inner class would use, exactly as in v3 where no call
///    site overrides it either. (The classification's `upright_letters_are_op` flag
///    cannot change the outcome for the same reason, so it is fixed at `true` here.)
fn needs_wrapping(inner: &str, right_declared: Option<AtomClass>) -> bool {
    if right_declared == Some(AtomClass::Script) {
        return true;
    }
    if inner.contains(' ') {
        return true;
    }
    classify_plain(inner, true).1 == AtomClass::OpenEnd
}

/// One atom after it has rendered itself against its neighbours.
struct Realized {
    body: MathBox,
    cls_left: AtomClass,
    cls_right: AtomClass,
    script: Option<ScriptInfo>,
}

/// Render one atom against the class its right neighbour declares.
///
/// The class its *left* neighbour declares is deliberately not consulted: no atom kind
/// needs it today, and v3 accepts it for symmetry only.
fn realize(
    atom: &Atom,
    right_declared: Option<AtomClass>,
    display: bool,
    delims: &MathWrapDelims,
) -> Realized {
    let (body, cls_left, cls_right) = match &atom.body {
        AtomBody::Str(text) => (MathBox::single_line(text.clone()), atom.cls.0, atom.cls.1),
        AtomBody::Wrappable { inner, prefix } => {
            let wrapped = delims.wrap(inner);
            // A one-character body can never wrap, whatever follows it, because
            // `wrap` returns it unchanged — hence the comparison before the question.
            if wrapped != **inner && needs_wrapping(inner, right_declared) {
                let mut text = String::from(&**prefix);
                text.push_str(&wrapped);
                // The wrapped class pair defaults to (cls.0, Close): a closing
                // delimiter is now the last thing in the rendering (DECISIONS.md D5).
                (MathBox::single_line(text), atom.cls.0, AtomClass::Close)
            } else {
                let mut text = String::from(&**prefix);
                text.push_str(inner);
                (MathBox::single_line(text), atom.cls.0, atom.cls.1)
            }
        }
        AtomBody::Block(math_box) => {
            let body = if display {
                math_box.clone()
            } else {
                // Inline math is one line by definition, so a multi-row block that
                // reached an inline join is squeezed onto one. Handlers build the
                // inline rendering directly (`inline_matrix_atom`), so this is a
                // safety net rather than a path.
                MathBox::single_line(math_box.join_lines(" "))
            };
            (body, atom.cls.0, atom.cls.1)
        }
    };
    Realized {
        body,
        cls_left,
        cls_right,
        script: atom.script,
    }
}

/// Drop the atoms that are already known to render to nothing (v3
/// `_math_pieces_prepare`).
///
/// v3 drops empty *strings* here, before the reordering below, and empty *pieces*
/// later, once they have rendered themselves. techxt's currency is atoms all the way
/// through, so the two cases meet in one: an atom whose body is the empty string is
/// dropped up front. That is the case v3's early drop covers — an empty script
/// argument (`$x^{}$`) reaches it as a plain empty string — and dropping it here is
/// what keeps it from wedging itself between a superscript and the subscript that
/// ought to swap with it. Atoms that only *turn out* to render to nothing are dropped
/// after they realize.
fn drop_empty(atoms: &mut Vec<Atom>) {
    atoms.retain(|atom| !matches!(&atom.body, AtomBody::Str(text) if text.is_empty()));
}

/// Reorder the atoms before joining (v3 `_math_pieces_prepare`).
///
/// A superscript immediately followed by a subscript reads far better the other way
/// around, because the subscript then attaches to the base first and the two scripts
/// do not run together: `x^a_b` comes out `𝑥_𝑏ᵃ` rather than `𝑥ᵃ_𝑏`. (The trick is
/// taken from the `flatlatex` package.) The pass walks forward and keeps going after a
/// swap, so a run cascades: `[sup, sub, sub]` becomes `[sub, sub, sup]`.
fn reorder_scripts(atoms: &mut [Atom]) {
    let kind_of = |a: &Atom| a.script.map(|s| s.kind);
    for i in 0..atoms.len().saturating_sub(1) {
        if kind_of(&atoms[i]) == Some(ScriptKind::Superscript)
            && kind_of(&atoms[i + 1]) == Some(ScriptKind::Subscript)
        {
            atoms.swap(i, i + 1);
        }
    }
}

/// Join a sequence of atoms into a finished formula (PLAN.md §9.5).
///
/// The default [`MathWrapDelims::Parens`] are used for sub-expressions that need
/// delimiters; call [`join_atoms_with`] to choose others, and to obtain the class pair
/// of the result.
///
/// `display` says whether this is display math: only there can the result be more than
/// one line high, because only a display matrix produces a multi-row box.
///
/// ```
/// use techxt::mathfmt::{join_atoms, segment_plain};
///
/// let joined = join_atoms(segment_plain("x+y", false), false);
/// assert_eq!(joined.join_lines("\n"), "x + y");
/// ```
pub fn join_atoms(atoms: Vec<Atom>, display: bool) -> MathBox {
    join_atoms_with(atoms, display, &MathWrapDelims::Parens).math_box
}

/// Join a sequence of atoms, choosing the delimiters sub-expressions wrap themselves
/// in and reporting the class pair of the result.
///
/// See [`join_atoms`] for the common case, and [`MathWrapDelims`] for the seam that
/// `delims` represents.
///
/// The steps, in order:
///
/// 1. drop the atoms that are empty already, then reorder superscript/subscript pairs;
/// 2. realize every atom against its neighbours' declared classes, dropping the ones
///    that turn out to render to nothing — a dropped atom cannot leave a spurious
///    separator behind;
/// 3. reclassify unary operators: a `Bin` that opens the formula, or follows a `Bin`,
///    `Rel`, `Open` or `Punct`, is a sign rather than an operation, so it binds tightly
///    to what follows (`-𝑥`, `(-𝑎, -𝑏)`) and does not push a preceding delimiter away.
///    All of them are identified before any is rewritten, so `$--x$` does not depend on
///    the iteration order;
/// 4. propagate the class of a base through its scripts: what `\sum_i` presents to
///    whatever follows is the large operator's class, not the subscript's, so `∑ᵢ 𝑦`
///    keeps its space. A script with no visible extent of its own (the `^…` fallback)
///    stays open-ended whatever its base was;
/// 5. compute the separators, then let every script hug its base — and, where the
///    script had to use LaTeX notation, put a space in front of the base as well so
///    that one can see where the base begins;
/// 6. assemble, aligning baselines ([`baseline_join`]).
pub fn join_atoms_with(atoms: Vec<Atom>, display: bool, delims: &MathWrapDelims) -> JoinedMath {
    let mut atoms = atoms;
    drop_empty(&mut atoms);
    reorder_scripts(&mut atoms);

    // Pass 1: every atom realizes itself against what its neighbours declare. The
    // declared classes come from the *unrealized* atoms, on purpose: that is the
    // conservative direction, and it is what makes the two passes terminate.
    let mut items: Vec<Realized> = Vec::with_capacity(atoms.len());
    for (i, atom) in atoms.iter().enumerate() {
        let right_declared = atoms.get(i + 1).map(|a| a.cls.0);
        let realized = realize(atom, right_declared, display, delims);
        if realized.body.is_blank() {
            continue;
        }
        items.push(realized);
    }

    let num = items.len();
    if num == 0 {
        return JoinedMath {
            math_box: MathBox::empty(),
            cls: AtomClass::both(AtomClass::Ord),
        };
    }

    // Unary operators.
    let is_unary: Vec<bool> = (0..num)
        .map(|i| {
            items[i].cls_left == AtomClass::Bin
                && (i == 0 || UNARY_LEFT_CLASSES.contains(&items[i - 1].cls_right))
        })
        .collect();
    for i in 0..num {
        if is_unary[i] {
            items[i].cls_left = AtomClass::Ord;
            // Only demote the right edge if it too was Bin, which is the case for a
            // single-character `+`/`-` atom.
            if items[i].cls_right == AtomClass::Bin {
                items[i].cls_right = AtomClass::Ord;
            }
        }
    }

    // Script class propagation. The loop walks left to right and rewrites in place, so
    // the base's class travels down a whole chain of scripts (`∑`, `_ᵢ₌₁`, `^ⁿ` all
    // end up presenting `Op`).
    for i in 1..num {
        if items[i].cls_left == AtomClass::Script && items[i].cls_right != AtomClass::OpenEnd {
            items[i].cls_right = items[i - 1].cls_right;
        }
    }

    // Pass 2: the separators.
    let mut seps: Vec<&'static str> = (0..num.saturating_sub(1))
        .map(|i| {
            separator(
                items[i].cls_right,
                items[i + 1].cls_left,
                items[i].body.baseline_text(),
                items[i + 1].body.baseline_text(),
            )
        })
        .collect();

    // Scripts attach spacelessly, and a fallback-notation script also pushes a space
    // in front of the base it applies to.
    for i in 1..num {
        if items[i].cls_left != AtomClass::Script {
            continue;
        }
        seps[i - 1] = "";
        let Some(script) = items[i].script else {
            continue;
        };
        if !script.latex_notation {
            continue;
        }
        if i < 2 {
            continue; // no base-1 to put a space in front of
        }
        if items[i - 1].cls_left == AtomClass::Script {
            continue; // scripts chain onto one base
        }
        if SELF_DELIMITED_CLASSES.contains(&items[i - 1].cls_right) {
            continue; // the base already shows where it ends
        }
        if !seps[i - 2].is_empty() {
            continue; // already separated
        }
        if items[i - 2].body.baseline_text().ends_with(' ')
            || items[i - 1].body.baseline_text().starts_with(' ')
        {
            continue; // a literal space is there already
        }
        seps[i - 2] = " ";
    }

    let cls = (items[0].cls_left, items[num - 1].cls_right);
    let boxes: Vec<MathBox> = items.into_iter().map(|it| it.body).collect();
    JoinedMath {
        math_box: baseline_join(&boxes, &seps),
        cls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mathfmt::segment_plain;
    use alloc::{vec, vec::Vec};

    /// Join atoms and read the result back as one string.
    fn join(atoms: Vec<Atom>) -> String {
        join_atoms(atoms, false).join_lines("\n")
    }

    /// An atom with the same class on both edges.
    fn a(text: &str, cls: AtomClass) -> Atom {
        Atom::from_text(text, cls.both())
    }

    /// Segment a plain string as the caller would, with a math alphabet in force.
    fn seg(s: &str) -> Vec<Atom> {
        segment_plain(s, true)
    }

    #[test]
    fn wrap_short_circuits_on_a_single_character() {
        assert_eq!(MathWrapDelims::Parens.wrap("ab"), "(ab)");
        assert_eq!(MathWrapDelims::Parens.wrap("𝑞"), "𝑞");
        assert_eq!(MathWrapDelims::Braces.wrap("ab"), "{ab}");
        assert_eq!(
            MathWrapDelims::Custom("[".into(), "]".into()).wrap("ab"),
            "[ab]"
        );
        assert_eq!(MathWrapDelims::None.wrap("ab"), "ab");
    }

    #[test]
    fn rule_1_binary_operators_and_relations() {
        assert_eq!(join(seg("x+y")), "x + y");
        assert_eq!(join(seg("a=b")), "a = b");
        assert_eq!(join(seg("a≤b")), "a ≤ b");
    }

    #[test]
    fn rule_2_open_ended_edges() {
        let frac = Atom::from_text("a/b", (AtomClass::OpenEnd, AtomClass::OpenEnd));
        assert_eq!(join(vec![frac.clone(), a("c", AtomClass::Ord)]), "a/b c");
        assert_eq!(join(vec![a("c", AtomClass::Ord), frac]), "c a/b");
    }

    #[test]
    fn rule_3_a_large_operator_on_the_left_unless_a_delimiter_follows() {
        assert_eq!(join(seg("sin x")), "sin x"); // the literal space is its own atom
        assert_eq!(
            join(vec![a("sin", AtomClass::Op), a("x", AtomClass::Ord)]),
            "sin x"
        );
        assert_eq!(join(seg("sin(x)")), "sin(x)");
        // ... and a closing delimiter hugs too
        assert_eq!(
            join(vec![a("sin", AtomClass::Op), a(")", AtomClass::Close)]),
            "sin)"
        );
    }

    #[test]
    fn rule_4_a_large_operator_on_the_right_unless_an_opening_delimiter_precedes() {
        assert_eq!(
            join(vec![a("2", AtomClass::Ord), a("sin", AtomClass::Op)]),
            "2 sin"
        );
        assert_eq!(
            join(vec![a("(", AtomClass::Open), a("sin", AtomClass::Op)]),
            "(sin"
        );
        // a text fragment behaves the same way
        assert_eq!(
            join(vec![a("2", AtomClass::Ord), a("kg", AtomClass::Text)]),
            "2 kg"
        );
    }

    #[test]
    fn rule_5_punctuation_separates_after_itself_only() {
        assert_eq!(join(seg("f(x,y)")), "f(x, y)");
        assert_eq!(join(seg("a,b;c")), "a, b; c");
    }

    #[test]
    fn rule_6_two_digits_would_run_together() {
        // separate atoms, as `{1}2` produces
        assert_eq!(
            join(vec![a("1", AtomClass::Ord), a("2", AtomClass::Ord)]),
            "1 2"
        );
        assert_eq!(
            join(vec![a("12", AtomClass::Ord), a("34", AtomClass::Ord)]),
            "12 34"
        );
        // one atom stays one number
        assert_eq!(join(seg("12")), "12");
        // a letter next to a digit does not need the space
        assert_eq!(
            join(vec![a("𝑎", AtomClass::Ord), a("2", AtomClass::Ord)]),
            "𝑎2"
        );
    }

    #[test]
    fn rule_7_implicit_multiplication_stays_tight() {
        assert_eq!(join(seg("4π𝑐")), "4π𝑐");
        assert_eq!(join(seg("f(x)")), "f(x)");
    }

    #[test]
    fn unary_minus_binds_to_what_follows() {
        assert_eq!(join(seg("-x")), "-x");
        assert_eq!(join(seg("x=-y")), "x = -y");
        assert_eq!(join(seg("-x+(-y)")), "-x + (-y)");
        assert_eq!(join(seg("(-a,-b)")), "(-a, -b)");
        assert_eq!(join(seg("a-b")), "a - b");
        // both signs of `--x` are identified before either is rewritten
        assert_eq!(join(seg("--x")), "--x");
    }

    #[test]
    fn an_empty_atom_does_not_block_the_script_swap() {
        // it is dropped before the reordering, exactly as an empty string is in v3
        let atoms = vec![
            a("𝑥", AtomClass::Ord),
            Atom {
                script: Some(ScriptInfo {
                    kind: ScriptKind::Superscript,
                    latex_notation: false,
                }),
                ..a("ᵃ", AtomClass::Script)
            },
            a("", AtomClass::Ord),
            Atom {
                cls: (AtomClass::Script, AtomClass::Ord),
                script: Some(ScriptInfo {
                    kind: ScriptKind::Subscript,
                    latex_notation: false,
                }),
                ..a("ᵦ", AtomClass::Script)
            },
        ];
        assert_eq!(join(atoms), "𝑥ᵦᵃ");
    }

    #[test]
    fn empty_atoms_are_dropped_without_leaving_a_separator() {
        let atoms = vec![
            a("𝑎", AtomClass::Ord),
            a("", AtomClass::Bin),
            a("𝑏", AtomClass::Ord),
        ];
        assert_eq!(join(atoms), "𝑎𝑏");
        assert_eq!(join(vec![]), "");
        assert_eq!(join(vec![a("", AtomClass::Ord)]), "");
    }

    #[test]
    fn no_doubled_space_around_text_that_carries_its_own() {
        let atoms = vec![
            a("if ", AtomClass::Text),
            a("𝑥", AtomClass::Ord),
            a(">", AtomClass::Rel),
            a("0", AtomClass::Ord),
        ];
        assert_eq!(join(atoms), "if 𝑥 > 0");
    }

    #[test]
    fn the_joined_class_pair_is_that_of_the_surviving_edges() {
        let joined = join_atoms_with(seg("(x+y)"), false, &MathWrapDelims::Parens);
        assert_eq!(joined.cls, (AtomClass::Open, AtomClass::Close));
        let empty = join_atoms_with(vec![], false, &MathWrapDelims::Parens);
        assert_eq!(empty.cls, (AtomClass::Ord, AtomClass::Ord));
    }
}
