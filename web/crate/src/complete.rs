//! Completion: the shipped symbol table, the document scan, and the merged answer
//! (web/PLAN.md §4.9).
//!
//! [`complete_native`] is the whole of it. Like [`crate::diag::convert_native`] it takes
//! and returns ordinary Rust, so everything interesting here is testable without wasm;
//! [`Session::complete`](crate::Session::complete) only keeps the table alive across
//! calls and serializes what comes back.
//!
//! # Every rule lives here
//!
//! The app sends a prefix and renders what it gets: it does no matching, no merging and
//! no ranking. That is the point rather than an accident. There are two sources of
//! suggestions — the definitions techxt ships and the ones the document defines for
//! itself — and reconciling them in JavaScript would mean a second matcher to keep in
//! step with this one, a second copy of a fourteen-hundred-entry table to ship, and two
//! places to change when the ordering is wrong. So the binding answers with one list,
//! already merged and already in the order the chips should appear in.
//!
//! The order is, in full:
//!
//! 1. **An exact match on the prefix, before anything else**, whatever its kind and
//!    whichever source it came from. Someone who has typed the whole name is not asking
//!    to be shown something longer — and without this rule first, a name that is also
//!    the beginning of a more famous one could never be completed at all.
//! 2. **What the document defines, before what the library ships.** A name the author
//!    wrote themselves is the one they meant, and it is also the one that will actually
//!    fire: a `\renewcommand` in the document shadows the library's definition, so the
//!    document's entry *replaces* the shipped one rather than sitting above it.
//! 3. **The curated names, in the curated order.** [`curated_names`] is the hundred-odd
//!    macros people actually type, and it ranks each of them above everything else that
//!    shares its prefix. Its own order is the ranking and is never re-sorted.
//! 4. **Macros, then environments, then specials.** Completion is triggered by an escape
//!    character, and what follows one is usually a macro.
//! 5. **The shortest name**, which is the one closest to what has been typed, and then
//!    **alphabetically** so that the answer to a given prefix never depends on the order
//!    the table happened to be built in.
//!
//! # Why there is a curated list at all, and why it is here
//!
//! Rules 4 and 5 are the whole of what a table of definitions can say: they measure the
//! *name*, because a definition set knows nothing about the person typing it. Ranked by
//! them alone, `\alp` offers `\alph` before `\alpha` — defensible, and wrong. `\alph` is
//! LaTeX's alphabetic counter format, `\alpha` is a letter of the Greek alphabet, and
//! nothing measurable here separates them. So the separation is written down instead, as
//! a short explicit list, with rule 1 keeping `\alph` reachable by typing it in full.
//!
//! The list is short on purpose. A list long enough to cover the table is rule 5 again
//! with extra steps, and every name on it is a name whose ranking someone has to
//! justify.
//!
//! It lives in the binding rather than in `rust/techxt` because *what people type most*
//! is a fact about a completion UI, not about LaTeX: it changes with the audience,
//! nothing in the library could test it, and a converter is no better at converting for
//! knowing that `\alpha` is popular. What the library owed this feature it has already
//! given — `DefinitionSet::symbols()`, the readable half of a data structure that could
//! previously only be written to.
//!
//! # What the scan can and cannot see
//!
//! The document half is a linear scan for the definers LaTeX authors actually use, not a
//! parse: `Conversion` exposes the converted text and its diagnostics and no parsing
//! state, and having the binding run the parser a second time to read one out would be
//! far out of proportion to the difference it makes to a chip row. A scan therefore reads
//! what a definer *looks* like rather than what the parser would do with it, and the two
//! can disagree. Where the disagreement is cheap to remove it is removed — comments are
//! skipped, because `%` is unambiguous and the scan is walking escape sequences anyway —
//! and where it is not, it stands: a definer inside a `verbatim` body is offered, because
//! recognizing one would mean tracking environments, `\verb`, `lstlisting` and the rest,
//! which is the parse this design declined. Offering a name the document mentions but
//! does not define costs one chip; the tests pin both behaviours so that neither is a
//! surprise.

use serde::Serialize;

use techxt::def::{CallableKind, SymbolEntry};
use techxt::defs;

/// One suggestion, in the shape `web/src/worker/protocol.ts` declares.
///
/// `rename_all` is here for [`from_document`](Self::from_document) alone; the other four
/// field names are single lowercase words and Rust's spelling is the wire's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionDto {
    /// The name, written as a definition writes it: **without** the escape character, so
    /// `"alpha"` and not `"\\alpha"`. An environment is named as `\begin{…}` names it.
    pub name: String,
    /// Which of the three kinds of callable this is. A macro and an environment can
    /// share a name and mean different things, so the kind is half of the identity and
    /// not decoration.
    pub kind: CompletionKindDto,
    /// What it renders as, when it renders as a fixed literal (`\alpha` → `α`) — the
    /// part that makes a chip row worth reading. `null` for everything whose rule has to
    /// be executed to find out, and for every definition scanned out of the document,
    /// which the binding recognizes but does not evaluate.
    pub replacement: Option<String>,
    /// How many arguments it declares, for a chip that wants to insert braces.
    ///
    /// `u32` and not `usize` because `serde-wasm-bindgen` turns a 64-bit integer into a
    /// `BigInt`, and `interface Completion { arity: number }` is not a `BigInt`.
    pub arity: u32,
    /// `true` when this was scanned out of the user's own document rather than read from
    /// the library's table, which is what lets a chip say where it came from.
    pub from_document: bool,
}

/// Which kind of callable a suggestion is, lowercase on the wire as `protocol.ts` spells
/// it.
///
/// The ordering is meaningful and is relied on twice: the table is sorted by it, so the
/// entries of one kind are a subslice, and the ranking uses it to prefer a macro to an
/// environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionKindDto {
    /// `\emph`, `\alpha`: a macro.
    Macro,
    /// `itemize`, `align`: an environment, named as in `\begin{…}`.
    Environment,
    /// `~`, `---`: a construct triggered by the characters themselves.
    Specials,
}

impl From<CallableKind> for CompletionKindDto {
    fn from(kind: CallableKind) -> CompletionKindDto {
        match kind {
            CallableKind::Macro => CompletionKindDto::Macro,
            CallableKind::Environment => CompletionKindDto::Environment,
            CallableKind::Specials => CompletionKindDto::Specials,
        }
    }
}

/// Every name techxt ships, read out of its definitions once and kept (web/PLAN.md §4.9).
///
/// # Why this is not a `SymbolIndex`
///
/// [`DefinitionSet::symbols`](techxt::def::DefinitionSet::symbols) hands back a
/// `SymbolIndex<'a>` that borrows the set it was read from, which is exactly right for
/// the library and exactly wrong for a value a `Session` has to keep: a struct holding
/// both the set and an index into it is self-referencing, and there is no honest way to
/// write one. So the entries are copied out into owned strings — fourteen hundred names,
/// a few tens of kilobytes, paid once — and the set is dropped. The alternative,
/// rebuilding the index per keystroke, is fourteen hundred definitions resolved to answer
/// three letters.
///
/// The copy keeps the property the index exists for: the table is sorted by kind and
/// then by name, so the entries of one kind are contiguous and a prefix scan is a
/// subslice found by binary search rather than a filter over everything.
pub struct SymbolTable {
    /// Sorted by `(kind, name)`, one entry per key, exactly as `SymbolIndex` resolved it.
    entries: Vec<Symbol>,
}

impl SymbolTable {
    /// Read the standard definitions — the ones every conversion in this app uses.
    ///
    /// The set is `defs::standard()` rather than the session's own converter's, because
    /// the binding exposes no way to change the definitions (`options.rs` lists
    /// `definitions` among the settings it deliberately does not map), so the table is
    /// the same whatever options the document is being converted under.
    pub fn standard() -> SymbolTable {
        let definitions = defs::standard();
        let index = definitions.symbols();
        let mut entries: Vec<Symbol> = index.entries().iter().map(Symbol::from).collect();
        // Sorted here rather than trusted from the index: the index sorts by techxt's
        // `CallableKind`, this table searches by the wire's `CompletionKindDto`, and a
        // subslice search that silently assumed the two orders agree would fail by
        // returning nothing rather than by failing to compile.
        entries.sort_unstable_by(Symbol::order);
        SymbolTable { entries }
    }

    /// How many names the library defines, after shadowing.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty, which for the standard definitions it never is.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every name of one kind starting with `prefix`, in name order.
    ///
    /// Two binary searches for the kind and two more for the prefix: names sharing a
    /// prefix are adjacent in a sorted table, so the answer is a subslice. An empty
    /// prefix is the whole kind.
    fn starts_with(&self, kind: CompletionKindDto, prefix: &str) -> &[Symbol] {
        let start = self.entries.partition_point(|symbol| symbol.kind < kind);
        let end = self.entries.partition_point(|symbol| symbol.kind <= kind);
        let of_kind = &self.entries[start..end];
        let from = of_kind.partition_point(|symbol| symbol.name.as_ref() < prefix);
        let count = of_kind[from..].partition_point(|symbol| symbol.name.starts_with(prefix));
        &of_kind[from..from + count]
    }
}

/// One shipped definition, owned so that the table can outlive the set it was read from.
///
/// `Box<str>` rather than `String`: nothing here is ever appended to, and a thousand-odd
/// entries is enough for the saved capacity word to be worth having.
struct Symbol {
    /// The name, without the escape character.
    name: Box<str>,
    /// Which kind of callable it is.
    kind: CompletionKindDto,
    /// What it renders as, when that is a fixed literal.
    replacement: Option<Box<str>>,
    /// How many arguments it declares.
    arity: u32,
}

impl From<&SymbolEntry<'_>> for Symbol {
    fn from(entry: &SymbolEntry<'_>) -> Symbol {
        Symbol {
            name: entry.name.into(),
            kind: entry.kind.into(),
            replacement: entry.replacement.map(Into::into),
            arity: as_u32(entry.arity),
        }
    }
}

impl Symbol {
    /// The table's sort order: kind first, then name.
    fn order(left: &Symbol, right: &Symbol) -> core::cmp::Ordering {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
    }

    /// This symbol as a suggestion, which is to say with its strings copied for the wire.
    fn offer(&self) -> CompletionDto {
        CompletionDto {
            name: self.name.to_string(),
            kind: self.kind,
            replacement: self.replacement.as_ref().map(|text| text.to_string()),
            arity: self.arity,
            from_document: false,
        }
    }
}

/// What `latex` defines for itself: the names of its own macros and environments
/// (web/PLAN.md §4.9).
///
/// The entries come back in document order, with `from_document: true` and
/// `replacement: None` — the scan recognizes a definition, it does not evaluate one —
/// and one entry per `(kind, name)`, the last definition winning, so that a
/// `\newcommand` later corrected by a `\renewcommand` is offered once with the arity it
/// ended up with.
///
/// The scan is a walk over escape sequences: at a `\` it reads the control sequence, and
/// at a `%` it skips to the end of the line, which is what keeps a commented-out definer
/// out of the list. It does *not* skip a definition's body — so a `\newcommand` nested
/// inside one is found too — and it does not know what a `verbatim` environment is; see
/// this module's own documentation for why that line is drawn where it is.
pub fn document_definitions(latex: &str) -> Vec<CompletionDto> {
    let bytes = latex.as_bytes();
    let mut found: Vec<CompletionDto> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => at = end_of_line(latex, at),
            b'\\' => {
                let (name, after) = control_sequence(latex, at);
                at = after;
                if let Some(definer) = Definer::named(name) {
                    if let Some(defined) = definer.read(latex, after) {
                        record(&mut found, defined);
                    }
                }
            }
            // Every byte of a multi-byte character is ≥ 0x80 and so is neither of the two
            // above, which is why walking bytes here cannot land inside one.
            _ => at += 1,
        }
    }
    found
}

/// The suggestions for `prefix`, merged from both sources and ranked (web/PLAN.md §4.9).
///
/// `prefix` is the name being typed, without the escape character; one leading `\` is
/// accepted and ignored, because the app slices the prefix out of its own buffer and the
/// two spellings mean the same thing here. An empty prefix matches everything, which is
/// what a caller asking for "the first few" gets; `limit` is a hard cap and a `limit` of
/// zero is an empty answer.
///
/// Matching is by prefix and is case-sensitive, because LaTeX names are.
pub fn complete_native(
    symbols: &SymbolTable,
    latex: &str,
    prefix: &str,
    limit: usize,
) -> Vec<CompletionDto> {
    if limit == 0 {
        return Vec::new();
    }
    let prefix = prefix.strip_prefix('\\').unwrap_or(prefix);

    let mut offered: Vec<CompletionDto> = document_definitions(latex)
        .into_iter()
        .filter(|entry| entry.name.starts_with(prefix))
        .collect();

    for kind in [
        CompletionKindDto::Macro,
        CompletionKindDto::Environment,
        CompletionKindDto::Specials,
    ] {
        for symbol in symbols.starts_with(kind, prefix) {
            // The document's own definition of a name is the one that will fire, so it
            // replaces the shipped entry rather than being listed beside it.
            let shadowed = offered
                .iter()
                .any(|entry| entry.kind == kind && entry.name.as_str() == symbol.name.as_ref());
            if !shadowed {
                offered.push(symbol.offer());
            }
        }
    }

    // Each key is computed once and carried beside its entry rather than computed inside
    // the comparator: placing a name in the curated list is a walk over a hundred of
    // them, and a comparison sort would repeat that walk for every comparison the entry
    // takes part in. The alphabetical tiebreak is read off the entry instead of being
    // stored in the key, because a key borrowing the value it is paired with is a struct
    // that cannot be written.
    let mut ordered: Vec<(Rank, CompletionDto)> = offered
        .into_iter()
        .map(|entry| (rank(&entry, prefix), entry))
        .collect();
    ordered.sort_by(|(left_rank, left), (right_rank, right)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left.name.cmp(&right.name))
    });
    ordered.truncate(limit);
    ordered.into_iter().map(|(_, entry)| entry).collect()
}

/// A suggestion's sort key: this module's order down to, but not including, the
/// alphabetical tiebreak.
///
/// The two leading `bool`s read backwards on purpose: `false` sorts first, so the
/// negation is what puts an exact match, and then the document's own names, at the top.
type Rank = (bool, bool, usize, CompletionKindDto, usize);

/// A suggestion's sort key, in the order this module's documentation gives.
fn rank(entry: &CompletionDto, prefix: &str) -> Rank {
    (
        entry.name != prefix,
        !entry.from_document,
        curated_rank(entry),
        entry.kind,
        entry.name.len(),
    )
}

/// Where `entry` stands in [`curated_names`], or [`usize::MAX`] — which is to say last —
/// for the overwhelming majority of names, which are not on it.
///
/// Only a macro can match. The list is about what follows an escape character, and an
/// environment name is not typed there but inside `\begin{…}`, where this completion
/// does not fire; a macro and an environment can also share a name and mean different
/// things, so matching on the name alone would rank the wrong one.
fn curated_rank(entry: &CompletionDto) -> usize {
    if entry.kind != CompletionKindDto::Macro {
        return usize::MAX;
    }
    CURATED
        .iter()
        .position(|name| *name == entry.name)
        .unwrap_or(usize::MAX)
}

/// The macros people actually type, in the order a chip row should prefer them
/// (web/PLAN.md §4.9).
///
/// **A ranking overlay, never a source of entries.** Nothing is offered because it
/// appears here: every suggestion still comes out of the [`SymbolTable`] or the
/// document, and a name on this list that techxt does not define simply never appears in
/// an answer. That failure is silent, so `tests/completion.rs` resolves every name here
/// against the shipped definitions and a typo is a red test rather than a dead entry.
///
/// **The order below is the ranking.** It is grouped — the Greek alphabet, then the
/// mathematics one writes with it, then the text and structure macros — and within a
/// group it runs roughly from what is typed most to what is typed least, which is why
/// `\varepsilon` leads the variants although it is the longest of the six, and why
/// `\int` and `\infty` come before `\in`. Nothing re-sorts it: two names sharing a
/// prefix appear in a chip row in the order they appear here.
///
/// **`\begin` and `\end` are absent because they cannot be present.** They are the
/// obvious omission from a list of everyday macros, and techxt does not define either:
/// they are structure the parser handles itself, not entries in a `DefinitionSet`, so no
/// completion can offer them however they are ranked. `equation` and `align`, checked
/// for the same reason, *are* defined — as environments, which is why they are not here
/// either. `tests/completion.rs` pins both findings, so the day `\begin` becomes
/// completable someone is told rather than left to notice.
static CURATED: [&str; 99] = [
    // The Greek alphabet, in its own order: it is learned as a sequence and a reader
    // scanning for one letter expects to find it where the alphabet keeps it. All of it
    // but `\omicron`, which techxt does define and nobody types, because it renders as a
    // Latin `o` and a Latin `o` is one keystroke.
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "zeta",
    "eta",
    "theta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "pi",
    "rho",
    "sigma",
    "tau",
    "upsilon",
    "phi",
    "chi",
    "psi",
    "omega",
    // The capitals that have a shape of their own. The rest of the Greek capitals are
    // Latin letters — `\Alpha` is `A` — and a person types `A`.
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Upsilon",
    "Phi",
    "Psi",
    "Omega",
    // The variants, most-typed first, which is the opposite of shortest-first: unranked,
    // `\var` offers `\varpi` and buries `\varepsilon` behind five rarer letters.
    "varepsilon",
    "varphi",
    "vartheta",
    "varrho",
    "varsigma",
    "varpi",
    // Mathematics: the constructs that take arguments, then the decorations that go on
    // them.
    "frac",
    "sqrt",
    "left",
    "right",
    "sum",
    "int",
    "prod",
    "hat",
    "bar",
    "vec",
    "tilde",
    "dot",
    // The symbols that stand on their own.
    "infty",
    "partial",
    "nabla",
    "cdot",
    "times",
    "pm",
    "langle",
    "rangle",
    // Relations.
    "leq",
    "geq",
    "neq",
    "approx",
    "equiv",
    "sim",
    "propto",
    // Arrows.
    "to",
    "rightarrow",
    "Rightarrow",
    "leftarrow",
    // Sets and logic.
    "in",
    "notin",
    "subset",
    "subseteq",
    "cup",
    "cap",
    "forall",
    "exists",
    // Text: the font-changing macros, with `\text` first because it is the base of the
    // family and the one a formula reaches for.
    "text",
    "textbf",
    "textit",
    "texttt",
    "emph",
    // The mathematical alphabets.
    "mathbb",
    "mathcal",
    "mathrm",
    "mathbf",
    // Structure, and the cross-references that hang off it.
    "section",
    "subsection",
    "label",
    "ref",
    "eqref",
    "cite",
    "item",
    "footnote",
    "caption",
    "includegraphics",
    "newcommand",
];

/// The curated names, in the curated order (web/PLAN.md §4.9).
///
/// This is a ranking overlay and never a source of entries: a name here is offered only
/// if techxt defines it, and one that techxt does not define simply never appears. That
/// failure is silent, which is why the slice is public — `tests/completion.rs` walks it
/// and resolves every name in it, so a typo is a red test rather than a dead entry. The
/// list's own source is where the order, and what is deliberately missing from it, are
/// explained.
pub fn curated_names() -> &'static [&'static str] {
    &CURATED
}

/// Add `entry`, replacing any earlier definition of the same name and kind.
///
/// A linear search rather than a map: a document with its own definitions has a handful,
/// the comparison stops at the first differing byte, and a `BTreeMap` here would cost an
/// allocation per definition to save nothing measurable.
fn record(found: &mut Vec<CompletionDto>, entry: CompletionDto) {
    match found
        .iter_mut()
        .find(|existing| existing.kind == entry.kind && existing.name == entry.name)
    {
        Some(existing) => *existing = entry,
        None => found.push(entry),
    }
}

/// The definers the scan recognizes, grouped by the shape of what follows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Definer {
    /// `\newcommand`, `\renewcommand`, `\providecommand`: a macro name, then an optional
    /// argument count.
    Command,
    /// `\def`: a macro name, then a parameter text whose `#`s are the argument count.
    Def,
    /// `\DeclareMathOperator`: a macro name, and never any arguments.
    MathOperator,
    /// `\newenvironment`: a braced environment name, then an optional argument count.
    Environment,
}

impl Definer {
    /// The definer this control sequence is, if it is one.
    ///
    /// The starred forms are not listed: a `*` is a separate token that
    /// [`read`](Self::read) skips, so `\newcommand*` and `\DeclareMathOperator*` arrive
    /// here as their unstarred names.
    fn named(control_sequence: &str) -> Option<Definer> {
        match control_sequence {
            "newcommand" | "renewcommand" | "providecommand" => Some(Definer::Command),
            "def" => Some(Definer::Def),
            "DeclareMathOperator" => Some(Definer::MathOperator),
            "newenvironment" => Some(Definer::Environment),
            _ => None,
        }
    }

    /// What this definer defines, reading from `at` — the offset just past its own name.
    ///
    /// `None` for anything that does not look like a definition after all, which is the
    /// scan's whole error handling: a definer whose argument it cannot read is a definer
    /// it says nothing about.
    fn read(self, latex: &str, at: usize) -> Option<CompletionDto> {
        match self {
            Definer::Command => {
                let at = skip_star(latex, at);
                let (name, after) = macro_name_maybe_braced(latex, at)?;
                Some(macro_defined(name, optional_arity(latex, after)))
            }
            Definer::MathOperator => {
                let at = skip_star(latex, at);
                let (name, _) = macro_name_maybe_braced(latex, at)?;
                Some(macro_defined(name, 0))
            }
            Definer::Def => {
                let (name, after) = macro_name(latex, skip_spaces(latex, at))?;
                Some(macro_defined(name, parameter_count(latex, after)))
            }
            Definer::Environment => {
                let at = skip_star(latex, at);
                let (name, after) = braced_word(latex, at)?;
                Some(CompletionDto {
                    name: name.to_string(),
                    kind: CompletionKindDto::Environment,
                    replacement: None,
                    arity: optional_arity(latex, after),
                    from_document: true,
                })
            }
        }
    }
}

/// A macro the document defines, as a suggestion.
fn macro_defined(name: &str, arity: u32) -> CompletionDto {
    CompletionDto {
        name: name.to_string(),
        kind: CompletionKindDto::Macro,
        replacement: None,
        arity,
        from_document: true,
    }
}

/// The offset just past the newline ending the line `at` is on, or the end of `latex`.
fn end_of_line(latex: &str, at: usize) -> usize {
    match latex[at..].find('\n') {
        Some(offset) => at + offset + 1,
        None => latex.len(),
    }
}

/// The offset of the first byte at or after `at` that is not whitespace.
fn skip_spaces(latex: &str, at: usize) -> usize {
    let bytes = latex.as_bytes();
    let mut at = at;
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// Past any whitespace and one optional `*`, which is how the starred definers differ
/// from the plain ones as far as a name scan is concerned.
fn skip_star(latex: &str, at: usize) -> usize {
    let at = skip_spaces(latex, at);
    if latex.as_bytes().get(at) == Some(&b'*') {
        skip_spaces(latex, at + 1)
    } else {
        at
    }
}

/// The control sequence beginning at `at` — which must be a `\` — and the offset past it.
///
/// A control *word* is the run of ASCII letters after the escape; a control *symbol* is
/// the single character after it, taken as a whole character so that `\é` does not split
/// one in half.
fn control_sequence(latex: &str, at: usize) -> (&str, usize) {
    let rest = &latex[(at + 1).min(latex.len())..];
    let letters = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters > 0 {
        return (&rest[..letters], at + 1 + letters);
    }
    match rest.chars().next() {
        Some(character) => (&rest[..character.len_utf8()], at + 1 + character.len_utf8()),
        None => ("", latex.len()),
    }
}

/// The macro name at `at`, and the offset past it.
///
/// Only a control word counts: `\@foo` under `\makeatletter` and `\\` are both things a
/// document can define, and neither is something a chip row should offer.
fn macro_name(latex: &str, at: usize) -> Option<(&str, usize)> {
    if latex.as_bytes().get(at) != Some(&b'\\') {
        return None;
    }
    let (name, after) = control_sequence(latex, at);
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    Some((name, after))
}

/// The macro name at `at`, written either as `{\ket}` or bare as `\ket`.
///
/// Both spellings are legal LaTeX and both are common, so both are read.
fn macro_name_maybe_braced(latex: &str, at: usize) -> Option<(&str, usize)> {
    if latex.as_bytes().get(at) != Some(&b'{') {
        return macro_name(latex, at);
    }
    let (name, after) = macro_name(latex, skip_spaces(latex, at + 1))?;
    let close = skip_spaces(latex, after);
    if latex.as_bytes().get(close) != Some(&b'}') {
        return None;
    }
    Some((name, close + 1))
}

/// The word inside the braces at `at`, and the offset past the closing brace.
///
/// Anything with a brace, a backslash, a comment character or whitespace inside it is
/// refused rather than guessed at: `\newenvironment{proof}` is what this is for, and a
/// group containing arbitrary LaTeX is not an environment name.
fn braced_word(latex: &str, at: usize) -> Option<(&str, usize)> {
    if latex.as_bytes().get(at) != Some(&b'{') {
        return None;
    }
    let rest = &latex[at + 1..];
    let close = rest.find('}')?;
    let name = rest[..close].trim();
    if name.is_empty()
        || name.contains(|character: char| {
            character.is_whitespace() || matches!(character, '\\' | '{' | '%')
        })
    {
        return None;
    }
    Some((name, at + close + 2))
}

/// The `[n]` argument count at `at`, or zero when there is none.
fn optional_arity(latex: &str, at: usize) -> u32 {
    let at = skip_spaces(latex, at);
    if latex.as_bytes().get(at) != Some(&b'[') {
        return 0;
    }
    let rest = &latex[at + 1..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || rest.as_bytes().get(digits) != Some(&b']') {
        return 0;
    }
    rest[..digits].parse().unwrap_or(0)
}

/// How many parameters `\def`'s parameter text declares: the `#`s before the body.
///
/// The text runs from the macro's name to the opening brace of the body, and a delimited
/// parameter can put anything at all in between — `\def\at#1@#2{…}` takes two — so what
/// is counted is the `#`s and not the shape.
fn parameter_count(latex: &str, at: usize) -> u32 {
    let mut count = 0;
    for byte in &latex.as_bytes()[at.min(latex.len())..] {
        match byte {
            b'{' | b'\\' | b'%' => break,
            b'#' => count += 1,
            _ => {}
        }
    }
    count
}

/// An arity as a JavaScript-friendly `u32`, saturating rather than wrapping.
///
/// Nothing declares four billion arguments; the conversion exists because
/// `serde-wasm-bindgen` would otherwise send a `BigInt`, as `diag.rs` explains at more
/// length.
fn as_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
