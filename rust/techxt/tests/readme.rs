//! The repository README's Rust examples, compiled and run.
//!
//! The README is the first thing a reader of this project sees, and an example in it
//! that no longer holds is worse than no example at all. These are the same two
//! snippets, character for character below the `use` line.

use techxt::Converter;

#[test]
fn readme_library_example() {
    let converter = Converter::standard();
    let conversion = converter
        .latex_to_text(r"\section{Intro} A fact\footnote{Why.}.")
        .expect("a tolerant parse of well-formed input succeeds");

    assert_eq!(
        conversion.text,
        "1 Intro\n-------\n\nA fact[1].\n\n---\n[1] Why.\n",
    );
    assert!(conversion.diagnostics.is_empty());
}

#[test]
fn readme_builder_example() {
    let _converter = Converter::builder()
        .wrap_width(Some(72))
        .keep_comments(true)
        .build()
        .expect("a well-formed definition set builds");
}
