//! Completion as the app receives it (web/PLAN.md §4.9).
//!
//! What is under test is the shape *and the order* of the answer. The order is the part
//! worth testing hardest: the JavaScript side does no ranking of its own, so a chip row
//! is exactly what these functions return, and "which suggestion does Tab take first,
//! and what does the Tab after it take?" is decided here and nowhere else.

use techxt_web::complete::{
    complete_native, curated_names, document_definitions, CompletionKindDto, SymbolTable,
};

/// The shipped table, built once for the whole file.
///
/// Reading `defs::standard()` costs a few milliseconds, and every test below wants the
/// same table, so they share one rather than each building its own — which is also the
/// arrangement a `Session` uses.
fn shipped() -> &'static SymbolTable {
    use std::sync::OnceLock;
    static TABLE: OnceLock<SymbolTable> = OnceLock::new();
    TABLE.get_or_init(SymbolTable::standard)
}

/// The suggestions for `prefix` in an empty document.
fn offered(prefix: &str, limit: usize) -> Vec<String> {
    complete_native(shipped(), "", prefix, limit)
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

/// The library defines a large table, and it is read back whole.
///
/// The count is deliberately a floor rather than the exact 1 406 that `defs::standard()`
/// resolves to today: that number is the library's own to pin, and
/// `rust/techxt/tests/def_symbols.rs` pins it. What this side needs to know is that the
/// table arrived, since a silently empty one would make every completion test below pass
/// by offering nothing.
#[test]
fn the_shipped_table_is_read_back_whole() {
    let table = shipped();
    assert!(!table.is_empty());
    assert!(table.len() > 1_000, "{} names", table.len());
}

/// The plain case: a prefix, and the symbols it matches with what they render as.
#[test]
fn a_prefix_matching_shipped_symbols_comes_back_with_their_replacements() {
    let items = complete_native(shipped(), "", "alpha", 8);
    let alpha = items
        .iter()
        .find(|entry| entry.name == "alpha")
        .expect("the library ships it");

    assert_eq!(alpha.kind, CompletionKindDto::Macro);
    assert_eq!(alpha.replacement.as_deref(), Some("α"));
    assert_eq!(alpha.arity, 0);
    assert!(!alpha.from_document);
}

/// A symbol whose rule has to be executed to know what it produces has no replacement to
/// show, and says so with `null` rather than with an empty string — which a chip row
/// would otherwise render as a name followed by nothing.
#[test]
fn a_symbol_with_no_literal_has_no_replacement() {
    let items = complete_native(shipped(), "", "emph", 4);
    let emph = items
        .iter()
        .find(|entry| entry.name == "emph")
        .expect("the library ships it");
    assert_eq!(emph.replacement, None);
    assert_eq!(emph.arity, 1);
}

/// All three kinds are reachable, and an environment and a specials come back named the
/// way they are written rather than as if they were macros.
#[test]
fn environments_and_specials_are_offered_too() {
    let environment = complete_native(shipped(), "", "itemize", 4);
    assert!(environment
        .iter()
        .any(|entry| entry.name == "itemize" && entry.kind == CompletionKindDto::Environment));

    // `--` is a specials: its name is the trigger characters themselves, and it has the
    // literal it renders as.
    let specials = complete_native(shipped(), "", "--", 4);
    let dash = specials
        .iter()
        .find(|entry| entry.name == "--")
        .expect("the library ships it");
    assert_eq!(dash.kind, CompletionKindDto::Specials);
    assert_eq!(dash.replacement.as_deref(), Some("–"));
}

/// The rule the whole design turns on: a name the author defined themselves is the one
/// they meant, so it comes first — and, because it is also the definition that will
/// actually fire, it *replaces* the library's rather than being listed beside it.
///
/// `\ket` is the example the TODO uses, and it is a better one than it looks: techxt
/// ships a `\ket` of its own, so this document is a `\newcommand` that shadows a real
/// definition and not one that invents a name.
#[test]
fn the_documents_own_definition_outranks_and_replaces_the_librarys() {
    let latex =
        r"\newcommand{\ket}[1]{\lvert #1 \rangle}".to_string() + "\nA state $\\ket{\\psi}$.\n";
    let items = complete_native(shipped(), &latex, "ke", 8);

    assert_eq!(items[0].name, "ket");
    assert!(items[0].from_document);
    assert_eq!(items[0].arity, 1);
    assert_eq!(items[0].replacement, None, "the scan does not evaluate");

    assert_eq!(
        items.iter().filter(|entry| entry.name == "ket").count(),
        1,
        "the document's `\\ket` replaces the shipped one rather than doubling it",
    );
    assert!(
        items[1..].iter().all(|entry| !entry.from_document),
        "everything else here is the library's",
    );
    assert!(
        items.iter().any(|entry| entry.name == "ketbra"),
        "the shipped neighbours are still offered",
    );
}

/// `\def` and `\newenvironment` are both found, with the arity each of them declares —
/// `\def` from the `#`s of its parameter text, `\newenvironment` from its `[n]`.
#[test]
fn a_def_and_a_newenvironment_are_both_found() {
    let latex = "\\def\\foo#1#2{(#1,#2)}\n\\newenvironment{bar}[1]{a}{b}\n";

    // `\footnote` and its relatives share the prefix; the document's own name leads.
    let foo = complete_native(shipped(), latex, "foo", 4);
    assert_eq!(foo[0].name, "foo");
    assert_eq!(foo[0].kind, CompletionKindDto::Macro);
    assert_eq!(foo[0].arity, 2);
    assert!(foo[0].from_document);

    let bar = complete_native(shipped(), latex, "bar", 4);
    assert_eq!(bar[0].name, "bar");
    assert_eq!(bar[0].kind, CompletionKindDto::Environment);
    assert_eq!(bar[0].arity, 1);
    assert!(bar[0].from_document);
}

/// Every definer the scan claims to know, in the spellings a document actually uses:
/// braced and bare, starred and plain.
#[test]
fn every_definer_the_scan_claims_is_recognized() {
    let latex = concat!(
        r"\newcommand{\ket}[1]{x}",
        "\n",
        r"\renewcommand*\bra[2]{y}",
        "\n",
        r"\providecommand{\id}{1}",
        "\n",
        r"\def\foo#1{z}",
        "\n",
        r"\DeclareMathOperator*{\argmax}{arg\,max}",
        "\n",
        r"\newenvironment*{proof}{a}{b}",
        "\n",
    );

    let found: Vec<(String, CompletionKindDto, u32)> = document_definitions(latex)
        .into_iter()
        .map(|entry| {
            assert!(entry.from_document);
            assert_eq!(entry.replacement, None);
            (entry.name, entry.kind, entry.arity)
        })
        .collect();

    assert_eq!(
        found,
        vec![
            (String::from("ket"), CompletionKindDto::Macro, 1),
            (String::from("bra"), CompletionKindDto::Macro, 2),
            (String::from("id"), CompletionKindDto::Macro, 0),
            (String::from("foo"), CompletionKindDto::Macro, 1),
            (String::from("argmax"), CompletionKindDto::Macro, 0),
            (String::from("proof"), CompletionKindDto::Environment, 0),
        ],
    );
}

/// A name defined twice is offered once, with the arity of the definition that won —
/// which is the last one, exactly as LaTeX would have it.
#[test]
fn a_later_definition_of_the_same_name_replaces_the_earlier_one() {
    let latex = "\\newcommand{\\ket}[1]{a}\n\\renewcommand{\\ket}[2]{b}\n";
    let found = document_definitions(latex);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "ket");
    assert_eq!(found[0].arity, 2);
}

/// A definer inside a comment is not a definition, and the scan knows it: it walks escape
/// sequences, so a `%` that has not itself been escaped starts a comment and the rest of
/// the line is skipped.
///
/// The second half is the reason the scan cannot simply search for `%`: `\%` is a
/// control symbol, not a comment, and a document whose macro body prints a percent sign
/// must still have its own macro offered.
#[test]
fn a_commented_out_definer_is_not_offered_and_an_escaped_percent_is_not_a_comment() {
    let latex = concat!(
        r"% \newcommand{\ghost}{boo}",
        "\n",
        r"\newcommand{\pct}{100\% of it} % \newcommand{\alsoghost}{boo}",
        "\n",
    );
    let names: Vec<String> = document_definitions(latex)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec![String::from("pct")]);
}

/// **A known false positive, pinned rather than fixed.** A `\newcommand` inside a
/// `verbatim` body is printed, not executed, so the document does not define it — but the
/// scan offers it anyway.
///
/// Recognizing the body would mean tracking `\begin`/`\end` pairs, `\verb` with its
/// arbitrary delimiter, and every listing package a document might use, which is the
/// parse the design declined (item 5 of `web/TODO.md`, and §4.9). Comments are filtered
/// because `%` is one unambiguous character and the scan is already walking escapes;
/// verbatim is not, because it is not. The cost is one chip offering a name that will not
/// fire, and this test is here so that the day someone decides that is too high, they
/// change a test that says what it is doing rather than one that looks like a bug.
#[test]
fn a_definer_inside_a_verbatim_body_is_a_known_false_positive() {
    let latex = concat!(
        "\\begin{verbatim}\n",
        r"\newcommand{\notreally}{x}",
        "\n\\end{verbatim}\n",
    );
    let names: Vec<String> = document_definitions(latex)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec![String::from("notreally")]);
}

/// The cap is a cap, and asking for nothing gets nothing.
#[test]
fn the_limit_is_respected() {
    assert_eq!(offered("a", 3).len(), 3);
    assert_eq!(offered("a", 1).len(), 1);
    assert!(offered("a", 0).is_empty());
    // A limit larger than the answer is not padded out to it.
    assert!(offered("alpha", 50).len() < 50);
}

/// An empty prefix matches everything, which the cap then makes finite. The app never
/// sends one — the chip row triggers on a `\` plus at least one letter — but a binding
/// that answered a keystroke with fourteen hundred entries, or panicked on the empty
/// slice, would be a poor way to find that out.
#[test]
fn an_empty_prefix_offers_the_first_few_names() {
    let items = complete_native(shipped(), "", "", 5);
    assert_eq!(items.len(), 5);
    assert!(items.iter().all(|entry| !entry.name.is_empty()));
}

/// A prefix nothing starts with is an empty answer, not an error and not everything.
#[test]
fn a_prefix_matching_nothing_is_empty() {
    assert!(offered("qqzzxx", 8).is_empty());
    // And the document's own names are held to the same prefix.
    let latex = r"\newcommand{\ket}[1]{x}";
    assert!(complete_native(shipped(), latex, "qqzzxx", 8).is_empty());
}

/// The prefix is the name without its escape character, and a leading `\` is tolerated
/// because the app slices the prefix out of its own buffer and the two spellings mean the
/// same thing here.
#[test]
fn the_prefix_may_carry_its_escape_character() {
    assert_eq!(offered("alpha", 6), offered(r"\alpha", 6));
}

/// Matching is case-sensitive, because LaTeX names are: `\Delta` and `\delta` are two
/// different symbols and a completion list that conflated them would be offering the
/// wrong one half the time.
#[test]
fn matching_is_case_sensitive() {
    let upper = complete_native(shipped(), "", "Delta", 4);
    assert!(upper.iter().any(|entry| entry.name == "Delta"));
    assert!(!upper.iter().any(|entry| entry.name == "delta"));
}

/// An exact match comes first whatever else shares its prefix, because someone who has
/// typed the whole name is not asking to be shown something longer.
#[test]
fn an_exact_match_comes_first() {
    let items = offered("text", 8);
    assert_eq!(items[0], "text");
    assert!(items.len() > 1, "and it is not the only one: {items:?}");
}

/// Macros are offered before environments, which is the order an escape character
/// implies: what follows one is usually a macro.
#[test]
fn macros_come_before_environments() {
    let items = complete_native(shipped(), "", "al", 8);
    let first_environment = items
        .iter()
        .position(|entry| entry.kind == CompletionKindDto::Environment)
        .expect("`align` and `alltt` are both here");
    assert!(items[..first_environment]
        .iter()
        .all(|entry| entry.kind == CompletionKindDto::Macro));
    assert!(first_environment > 0, "there are macros too: {items:?}");
}

/// **The TODO's example, and the reversal it took to make it true.** Its *Done when*
/// line says typing `\alp` offers `\alpha  α`, and for as long as the ranking measured
/// only the name it offered `\alph` instead: LaTeX's alphabetic counter format is a real
/// macro and a shorter completion of the same prefix, so shortest-first put it first.
///
/// Nothing measurable separates the two — that is still true, and it is why the
/// separation is now written down as a list rather than derived. The objection that kept
/// the old rule, that ranking `\alpha` first would mean nobody can ever complete
/// `\alph`, is answered by the rule above it rather than by argument: an exact match
/// comes first, so `\alph` is one keystroke away and the test below pins it.
#[test]
fn a_curated_name_outranks_a_shorter_neighbour() {
    let items = offered("alp", 8);
    assert_eq!(items[0], "alpha");
    assert_eq!(items[1], "alph");
}

/// The other half of the reversal: typing the shorter name *in full* still puts it
/// first, because an exact match outranks the curated list.
///
/// Without this rule the curated list would not have fixed the ranking, it would have
/// moved the bug: `\alph` would be a macro no amount of typing could complete.
#[test]
fn a_prefix_typed_in_full_still_leads_even_against_a_curated_name() {
    let items = offered("alph", 8);
    assert_eq!(items[0], "alph");
    assert_eq!(
        items[1], "alpha",
        "and the famous one is still right behind it"
    );
}

/// **The test the curated list cannot do without.** It is a ranking overlay and never a
/// source of entries: a name on it is offered only because the library defines it, so a
/// name the library does *not* define is not a wrong suggestion but no suggestion at all
/// — a typo that ranks nothing and says nothing.
///
/// So every name is resolved here, as a macro, against the shipped definitions. The kind
/// is part of the assertion because it is part of the identity: `equation` is a name
/// techxt defines, and curating it as a macro would rank a macro that does not exist.
#[test]
fn every_curated_name_is_a_macro_the_library_defines() {
    let undefined: Vec<&str> = curated_names()
        .iter()
        .copied()
        .filter(|name| {
            !complete_native(shipped(), "", name, 200)
                .iter()
                .any(|entry| entry.name == *name && entry.kind == CompletionKindDto::Macro)
        })
        .collect();
    assert!(
        undefined.is_empty(),
        "curated names the library does not define as macros: {undefined:?}",
    );
}

/// A name listed twice would be ranked by its first appearance and dead at its second,
/// which is exactly the kind of quiet wrongness a hand-written list collects.
#[test]
fn the_curated_list_names_each_macro_once() {
    let mut names: Vec<&str> = curated_names().to_vec();
    let listed = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        listed,
        "a name appears twice in the curated list"
    );
}

/// **The curated order is the list's own**, not a re-sort of it: the six Greek variants
/// come back in the order they are written down, which is the order they are typed in.
///
/// It is the clearest case there is, because it is the exact inverse of the rule that
/// would otherwise apply: `\varepsilon` is the longest of the six and by far the most
/// used, and shortest-first buries it behind five rarer letters, `\varpi` leading.
#[test]
fn the_curated_order_is_not_re_sorted() {
    let items = offered("var", 6);
    assert_eq!(
        items,
        vec![
            "varepsilon",
            "varphi",
            "vartheta",
            "varrho",
            "varsigma",
            "varpi",
        ],
    );
}

/// Past the curated names the old rule is untouched: everything else is shortest first
/// and then alphabetical. The list re-ranks a hundred names; it does not re-rank the
/// table.
#[test]
fn names_outside_the_curated_list_are_still_shortest_first() {
    let items = offered("var", 12);
    assert_eq!(
        &items[6..],
        [
            "varkappa",   // 8
            "varinjlim",  // 9, and then alphabetically
            "varliminf",  // 9
            "varlimsup",  // 9
            "varnothing", // 10
            "varprojlim", // 10
        ],
    );
}

/// The document still outranks the curated list, which is the order the two rules have
/// to be in: a name the author defined is a name that will fire, and no amount of
/// popularity beats that.
#[test]
fn the_documents_own_definition_outranks_a_curated_name() {
    let latex = r"\newcommand{\alphabet}{ABC}";
    let items: Vec<String> = complete_native(shipped(), latex, "alp", 8)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(items, vec!["alphabet", "alpha", "alph"]);
}

/// **Why `\begin` and `\end` are not on the curated list**, although they are the two
/// macros a LaTeX document contains most of: techxt does not define them. They are
/// structure the parser handles itself rather than entries in a `DefinitionSet`, so no
/// ranking can offer them and putting them on the list would only have made the test
/// above red.
///
/// What `\begin{…}` names is defined, and is an environment rather than a macro — which
/// is why `equation` and `align` are not curated either: the chip row fires on an escape
/// character, and an environment name is not typed after one.
///
/// This test is the notice. If `\begin` ever becomes a name the library defines, it goes
/// red, and whoever is holding it should put `\begin` and `\end` at the head of the list
/// where they belong.
#[test]
fn begin_and_end_are_not_names_the_library_defines() {
    assert!(offered("begin", 8).is_empty());
    assert!(!offered("end", 8).iter().any(|name| name == "end"));

    for environment in ["equation", "align"] {
        let items = complete_native(shipped(), "", environment, 8);
        let found = items
            .iter()
            .find(|entry| entry.name == environment)
            .expect("the library ships it");
        assert_eq!(found.kind, CompletionKindDto::Environment);
    }
}

/// Names of the same kind and length are alphabetical, so the answer to a given prefix
/// never depends on the order the table happened to be built in.
#[test]
fn ties_are_broken_alphabetically() {
    let items = offered("eqs", 8);
    assert_eq!(items, vec!["eqslantgtr", "eqslantless"]);
}
