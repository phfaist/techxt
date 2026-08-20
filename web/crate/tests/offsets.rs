//! Byte offsets to UTF-16 offsets, lines and columns (web/PLAN.md §4.4, §4.8).
//!
//! The property under test is the one the diagnostics panel depends on: for every
//! `char` boundary of a document, the offset the binding reports is the offset
//! JavaScript would compute for the same prefix — `prefix.length` in JavaScript, which
//! is `prefix.encode_utf16().count()` here. Get it wrong and clicking a warning selects
//! the wrong text, one surrogate pair at a time.
//!
//! The generator is a small LCG over a fixed alphabet rather than `proptest`: this
//! crate's dependency list is a design property (PLAN Appendix D), and a deterministic
//! generator has the nicer failure mode anyway — a failing seed is reproducible without
//! a regression file.

use techxt_web::diag::{OffsetMap, Position};

/// The pieces documents are assembled from: one, two, three and four byte characters, a
/// combining mark, an emoji, and all three line endings.
const ALPHABET: &[&str] = &[
    "a",
    "Z",
    " ",
    "\t",
    "é",         // two bytes, one UTF-16 unit
    "€",         // three bytes, one UTF-16 unit
    "𝕏",         // four bytes, a surrogate *pair* — two UTF-16 units
    "😀",        // likewise
    "e\u{0301}", // e + combining acute: two chars that render as one
    "\n",
    "\r\n",
    "\r",
];

/// What JavaScript would say about the prefix `latex[..byte]`, computed independently
/// of the code under test.
fn expected(latex: &str, byte: usize) -> Position {
    let prefix = &latex[..byte];
    let line = 1 + prefix.matches('\n').count();
    let column = 1 + match prefix.rfind('\n') {
        Some(newline) => prefix[newline + 1..].chars().count(),
        None => prefix.chars().count(),
    };
    Position {
        // `String.prototype.length` and `setSelectionRange` count UTF-16 code units.
        utf16: prefix.encode_utf16().count(),
        line,
        column,
    }
}

/// Assert the mapping over every `char` boundary of `latex`, the end of the string
/// included.
fn check_every_boundary(latex: &str) {
    let boundaries: Vec<usize> = latex
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([latex.len()])
        .collect();
    let map = OffsetMap::build(latex, boundaries.iter().copied());
    for byte in boundaries {
        assert_eq!(
            map.position(byte),
            expected(latex, byte),
            "at byte {byte} of {latex:?}",
        );
    }
}

/// The property of §4.4, over documents built from astral characters, combining marks
/// and every line ending.
#[test]
fn every_char_boundary_maps_to_the_offset_javascript_computes() {
    // A 64-bit LCG (Knuth's multiplier): reproducible, and one line of arithmetic.
    let mut state: u64 = 0x5eed_1234_5678_9abc;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };

    for case in 0..400 {
        let length = case % 24;
        let mut latex = String::new();
        for _ in 0..length {
            latex.push_str(ALPHABET[next() % ALPHABET.len()]);
        }
        check_every_boundary(&latex);
    }
}

/// The hand-written cases the generator would only reach by luck.
#[test]
fn the_awkward_documents_map_correctly() {
    for latex in [
        "",
        "\n",
        "\r\n",
        "𝕏",
        "😀𝕏",
        "a\r\nb\r\nc",
        "\r\n\r\n",
        "e\u{0301}\u{0301}x",
        "\u{0301}",
        "𝕏\n𝕏",
        "line\rstill line one\nline two",
    ] {
        check_every_boundary(latex);
    }
}

/// A `\r\n` ends one line, not two, and an astral character is one column but two
/// offsets.
#[test]
fn line_and_column_count_what_the_panel_shows() {
    let latex = "a\r\n𝕏b\nc";
    let map = OffsetMap::build(
        latex,
        (0..=latex.len()).filter(|b| latex.is_char_boundary(*b)),
    );

    // The `a`.
    assert_eq!(
        map.position(0),
        Position {
            utf16: 0,
            line: 1,
            column: 1
        },
    );
    // The `𝕏`, on line two: a `\r\n` moved one line on, not two.
    let x = latex.find('𝕏').expect("it is there");
    assert_eq!(
        map.position(x),
        Position {
            utf16: 3,
            line: 2,
            column: 1
        },
    );
    // The `b` after it: one column further, but *two* UTF-16 units further.
    assert_eq!(
        map.position(x + '𝕏'.len_utf8()),
        Position {
            utf16: 5,
            line: 2,
            column: 2
        },
    );
    // The `c`, on line three.
    let c = latex.rfind('c').expect("it is there");
    assert_eq!(
        map.position(c),
        Position {
            utf16: 7,
            line: 3,
            column: 1
        },
    );
    // One past the end of the document.
    assert_eq!(
        map.position(latex.len()),
        Position {
            utf16: 8,
            line: 3,
            column: 2
        },
    );
}

/// An offset the caller should never produce is answered, not panicked over: the wasm
/// instance is worth more than the arithmetic.
#[test]
fn an_impossible_offset_clamps_instead_of_panicking() {
    let latex = "a𝕏b";
    let inside_the_surrogate_pair = 2;
    assert!(!latex.is_char_boundary(inside_the_surrogate_pair));

    let map = OffsetMap::build(latex, [inside_the_surrogate_pair, 9999, latex.len()]);
    // An offset inside a character clamps forward to the next boundary...
    assert_eq!(
        map.position(inside_the_surrogate_pair),
        expected(latex, latex.find('b').expect("it is there")),
    );
    // ...and one past the end of the document maps to its end.
    assert_eq!(map.position(9999), expected(latex, latex.len()));
    // An offset that was never collected is answered too, from the nearest one below.
    assert_eq!(map.position(0), Position::START);
}

/// Offsets may be handed over in any order, repeated, and asked back in any other.
#[test]
fn the_map_does_not_care_about_order() {
    let latex = "one\ntwo\nthree";
    let sorted = OffsetMap::build(latex, [0, 4, 8, 13]);
    let shuffled = OffsetMap::build(latex, [13, 8, 4, 8, 0, 13]);
    for byte in [13, 0, 8, 4] {
        assert_eq!(sorted.position(byte), shuffled.position(byte));
        assert_eq!(sorted.position(byte), expected(latex, byte));
    }
}
