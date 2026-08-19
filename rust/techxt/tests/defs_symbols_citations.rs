//! `defs::natbib`: the citation family (PLAN.md §9.8, §12.1).
//!
//! Two things are being tested. The obvious one is that every command renders
//! `<cit.>`. The one that matters is that none of them *leaks*: natbib's shapes carry a
//! star and up to two bracketed notes, and a definition that does not declare them
//! leaves `[see]` and `[p. 5]` standing in the reader's sentence.

use techxt::Converter;

fn text(latex: &str) -> String {
    Converter::standard()
        .latex_to_text(latex)
        .expect("parses")
        .text
}

#[test]
fn every_citation_command_renders_the_marker() {
    for name in [
        "cite",
        "citet",
        "citep",
        "citealt",
        "citealp",
        "citeauthor",
        "citefullauthor",
        "Citet",
        "Citep",
        "Citealt",
        "Citealp",
        "Citeauthor",
        "Citefullauthor",
        "citeyear",
        "citeyearpar",
        "citetalias",
        "citepalias",
        "citenum",
    ] {
        assert_eq!(text(&format!("a \\{name}{{knuth1984}} b")), "a <cit.> b\n");
    }
}

#[test]
fn the_starred_forms_take_their_star() {
    for name in ["cite", "citet", "citep", "citealt", "citealp", "citeauthor"] {
        assert_eq!(text(&format!("a \\{name}*{{knuth1984}} b")), "a <cit.> b\n");
        let capitalized = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
        if capitalized != "Cite" {
            assert_eq!(
                text(&format!("a \\{capitalized}*{{knuth1984}} b")),
                "a <cit.> b\n",
            );
        }
    }
}

#[test]
fn the_optional_notes_never_leak() {
    assert_eq!(text(r"see \citep[p.~5]{knuth1984}."), "see <cit.>.\n");
    assert_eq!(text(r"see \citep[see][p.~5]{knuth1984}."), "see <cit.>.\n",);
    assert_eq!(
        text(r"see \citet*[e.g.][chap.~2]{knuth1984}."),
        "see <cit.>.\n",
    );
    // The two forms with notes but no star.
    assert_eq!(text(r"\citeyear[e.g.][p.~5]{knuth1984}"), "<cit.>\n");
    assert_eq!(text(r"\citetalias[e.g.][p.~5]{knuth1984}"), "<cit.>\n");
}

#[test]
fn a_key_list_is_one_argument_however_many_keys_it_holds() {
    assert_eq!(text(r"\citep{a,b,c} rest"), "<cit.> rest\n");
}

/// `\citetext`'s argument is the author's own prose, not a key, so it is rendered.
#[test]
fn citetext_renders_its_argument() {
    assert_eq!(text(r"(\citetext{priv.\ comm.})"), "(priv. comm.)\n");
}

/// `\defcitealias` is a declaration: it names an alias and prints nothing.
#[test]
fn the_declarations_print_nothing() {
    assert_eq!(
        text(r"\defcitealias{knuth1984}{TeXbook}A \citetalias{knuth1984} B"),
        "A <cit.> B\n",
    );
    assert_eq!(text(r"\shortcites{knuth1984}A"), "A\n");
}

/// PLAN.md §12.1: `natbib` is pushed last, so its `\cite` shadows the plain one in
/// [`defs::refs`], which declares only `[m]` and would spill the second note.
#[test]
fn natbib_shadows_the_plain_cite_of_refs() {
    assert_eq!(text(r"\cite[see][p.~5]{knuth1984}."), "<cit.>.\n");
    // The rest of `defs::refs` is untouched by the shadowing.
    assert_eq!(
        text(r"\ref{eq:main} and \eqref{eq:main}"),
        "<ref> and (<ref>)\n"
    );
}

/// A citation is not a formula, but it may stand in one.
#[test]
fn citations_work_inside_math() {
    assert_eq!(text(r"$x = 1$ \citep{knuth1984}"), "𝑥 = 1 <cit.>\n");
}
