//! Reading a definition set back (PLAN.md §10.7).
//!
//! The public prose lives on [`SymbolIndex`], because this module is private and its own
//! documentation is never rendered — the same arrangement the entry builders use.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use techy::latexlike::Mode;

use super::set::DefinitionSet;
use super::{CallableKind, TextRule};

/// Which parsing modes a definition is visible in (PLAN.md §10.7).
///
/// A mode restriction is a parse-side gate and buys less than it looks like it does —
/// [`MacroDef::math_mode_only`](super::MacroDef::math_mode_only) says exactly what — but
/// it is still the honest answer to "where may I write this?", which is what a caller
/// listing the table wants: offering `^` in a paragraph is offering something that will
/// not fire there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModeVisibility {
    /// Visible in every mode, which is what almost every definition is.
    Anywhere,
    /// Text mode only: the ligatures, and anything that would be nonsense in a formula.
    TextOnly,
    /// Math mode only: `^` and `_`, and the symbols that mean nothing in a paragraph.
    MathOnly,
}

impl ModeVisibility {
    /// The visibility a definition's mode restriction describes.
    ///
    /// `None` — no restriction at all — is [`Anywhere`](Self::Anywhere), and so is a
    /// restriction naming both modes: a definition visible in text *and* in math is
    /// visible in every mode there is, and saying so is more useful than repeating the
    /// longer way it happens to be written.
    fn of(restriction: Option<&[Mode]>) -> ModeVisibility {
        let Some(modes) = restriction else {
            return ModeVisibility::Anywhere;
        };
        match (modes.contains(&Mode::Text), modes.contains(&Mode::Math)) {
            (true, false) => ModeVisibility::TextOnly,
            (false, true) => ModeVisibility::MathOnly,
            _ => ModeVisibility::Anywhere,
        }
    }
}

/// One name a definition set defines, as the set resolves it (PLAN.md §10.7).
///
/// Everything here borrows from the set the entry was read out of, so an entry is
/// `Copy` and costs nothing to pass around; the strings are the definition's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolEntry<'a> {
    /// The name, written as the definition writes it: without the escape character for a
    /// macro (`"alpha"`, not `"\\alpha"`), as in `\begin{…}` for an environment, and as
    /// the trigger characters themselves for a specials.
    pub name: &'a str,
    /// Which of the three kinds of callable this is — the other half of the key, since
    /// a macro and an environment may share a name and mean different things.
    pub kind: CallableKind,
    /// The name of the category the winning definition came from, which is also what
    /// techy names as the provider in a parse diagnostic.
    pub category: &'a str,
    /// What the construct renders as when its rule is a plain literal (`\alpha` → `α`),
    /// and `None` for every rule that has to be executed to find out — a template, a
    /// handler, `Content`, `Skip`, or no rule at all.
    pub replacement: Option<&'a str>,
    /// How many arguments the definition declares.
    ///
    /// The count is the entry's own declaration. An entry that hands its whole techy
    /// spec to a [`CallableSpecSource`](super::CallableSpecSource) may in the end parse
    /// something else, which is documented on
    /// [`MacroDef::spec`](super::MacroDef::spec).
    pub arity: usize,
    /// Which modes the definition is visible in.
    pub modes: ModeVisibility,
}

/// Every name a definition set defines, later categories shadowing earlier ones
/// (PLAN.md §10.7).
///
/// The entry builders are how a definitions library is *written*; this is how it is
/// *read*. The question it answers — what does this converter actually know? — is one
/// every embedder with a user interface in front of it has to ask: an editor offering
/// completions as the author types a command name, a `--list-symbols` flag, a reference
/// page generated from the table rather than maintained beside it.
///
/// # One name, one answer
///
/// A [`DefinitionSet`] resolves a name through techy's scope stack innermost-first, and
/// later categories sit further in, so a later category's `\ldots` shadows an earlier
/// one's and a later entry within a category shadows an earlier entry of the same name.
/// The index applies that rule as it is built and therefore holds **one entry per
/// (kind, name) pair**, not a list of candidates: what a reader wants to know is what
/// the converter will do, not everything that was written on the way to deciding it.
///
/// One simplification is worth naming, because it is the only place a single answer is
/// not the whole truth. techy's resolution is *mode-aware*: an entry hidden in the
/// current mode is skipped and an outer definition of the same name can answer instead.
/// So a set whose innermost `\foo` is math-only, over an outer `\foo` that is not, really
/// does resolve to two different definitions in the two modes, and the index reports the
/// innermost one. The shipped library never does this — the generated long tail is where
/// the mode restrictions live and every curated category sits above it — and
/// `the_shipped_library_never_shadows_a_name_with_a_narrower_one` in
/// `tests/def_symbols.rs` keeps it that way.
///
/// # Built once, then searched
///
/// [`DefinitionSet::symbols`] walks every category once and hands back a table sorted by
/// [`CallableKind`] and then by name, so the entries of one kind are contiguous, a prefix
/// scan is a subslice rather than a filter, and every question is a binary search. That
/// split is the type's reason to exist — the shipped library is around fourteen hundred
/// names, and a caller that answers each keystroke by rebuilding the index is doing
/// fourteen hundred times the work it needs to. Build one and keep it for as long as the
/// set lives.
///
/// ```
/// use techxt::def::{CallableKind, ModeVisibility};
///
/// let definitions = techxt::defs::standard();
/// let symbols = definitions.symbols();
///
/// let alpha = symbols.get(CallableKind::Macro, "alpha").expect("the library has it");
/// assert_eq!(alpha.replacement, Some("α"));
/// assert_eq!(alpha.arity, 0);
///
/// // A prefix scan is a slice of the same table.
/// let offered = symbols.starts_with(CallableKind::Macro, "alph");
/// assert!(offered.iter().any(|entry| entry.name == "alpha"));
///
/// // `^` is a specials, and it fires in math only.
/// let script = symbols.get(CallableKind::Specials, "^").expect("declared");
/// assert_eq!(script.modes, ModeVisibility::MathOnly);
/// ```
#[derive(Clone, Debug)]
pub struct SymbolIndex<'a> {
    /// Sorted by `(kind, name)`, one entry per key.
    entries: Vec<SymbolEntry<'a>>,
}

impl<'a> SymbolIndex<'a> {
    /// Resolve every name `set` defines, applying the set's own shadowing rule.
    ///
    /// The map does the shadowing: entries are visited in the order they were declared —
    /// categories in push order, definitions in declaration order — and a later one
    /// simply replaces the entry already keyed the same way, which is what techy's
    /// innermost-first resolution would have found. Its iteration order is the
    /// `(kind, name)` order the finished table promises, so nothing needs sorting
    /// afterwards.
    pub(crate) fn build(set: &'a DefinitionSet) -> SymbolIndex<'a> {
        let mut resolved: BTreeMap<(CallableKind, &'a str), SymbolEntry<'a>> = BTreeMap::new();
        for category in set.categories() {
            let name = category.name();
            for definition in category.macros() {
                insert(
                    &mut resolved,
                    SymbolEntry {
                        name: definition.name(),
                        kind: CallableKind::Macro,
                        category: name,
                        replacement: literal(definition.text_rule()),
                        arity: definition.arity(),
                        modes: ModeVisibility::of(definition.modes().as_deref()),
                    },
                );
            }
            for definition in category.environments() {
                insert(
                    &mut resolved,
                    SymbolEntry {
                        name: definition.name(),
                        kind: CallableKind::Environment,
                        category: name,
                        replacement: literal(definition.text_rule()),
                        arity: definition.arity(),
                        // An environment is never mode-restricted: `\begin` resolves the
                        // name, and it resolves it in both modes.
                        modes: ModeVisibility::Anywhere,
                    },
                );
            }
            for definition in category.specials() {
                insert(
                    &mut resolved,
                    SymbolEntry {
                        name: definition.trigger(),
                        kind: CallableKind::Specials,
                        category: name,
                        replacement: literal(definition.text_rule()),
                        arity: definition.arity(),
                        modes: ModeVisibility::of(definition.modes().as_deref()),
                    },
                );
            }
        }
        SymbolIndex {
            entries: resolved.into_values().collect(),
        }
    }

    /// Every name the set defines, sorted by kind and then by name.
    pub fn entries(&self) -> &[SymbolEntry<'a>] {
        &self.entries
    }

    /// How many names the set defines, after shadowing.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set defines nothing at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries of one kind, sorted by name.
    ///
    /// They are contiguous in the table, so this costs two binary searches and borrows
    /// rather than collecting.
    pub fn of_kind(&self, kind: CallableKind) -> &[SymbolEntry<'a>] {
        let start = self.entries.partition_point(|entry| entry.kind < kind);
        let end = self.entries.partition_point(|entry| entry.kind <= kind);
        &self.entries[start..end]
    }

    /// What the set resolves this name to, or `None` if it defines it in no category.
    pub fn get(&self, kind: CallableKind, name: &str) -> Option<&SymbolEntry<'a>> {
        let of_kind = self.of_kind(kind);
        let found = of_kind
            .binary_search_by(|entry| entry.name.cmp(name))
            .ok()?;
        of_kind.get(found)
    }

    /// Every name of this kind starting with `prefix`, sorted — the completion query.
    ///
    /// Names that share a prefix are adjacent in a sorted table, so the answer is a
    /// subslice found by binary search: an empty prefix is the whole kind, and a prefix
    /// nothing starts with is empty.
    pub fn starts_with(&self, kind: CallableKind, prefix: &str) -> &[SymbolEntry<'a>] {
        let of_kind = self.of_kind(kind);
        let start = of_kind.partition_point(|entry| entry.name < prefix);
        let length = of_kind[start..].partition_point(|entry| entry.name.starts_with(prefix));
        &of_kind[start..start + length]
    }
}

/// Record one definition, replacing whatever the same key already held.
///
/// Written out rather than inlined because the replacement *is* the shadowing rule, and
/// a bare `insert` at three call sites says nothing about why the last write wins.
fn insert<'a>(
    resolved: &mut BTreeMap<(CallableKind, &'a str), SymbolEntry<'a>>,
    entry: SymbolEntry<'a>,
) {
    resolved.insert((entry.kind, entry.name), entry);
}

/// The fixed text a rule renders to, when it renders to fixed text.
fn literal(rule: Option<&TextRule>) -> Option<&str> {
    match rule {
        Some(TextRule::Literal(text)) => Some(text),
        _ => None,
    }
}
