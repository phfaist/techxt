# techxt — LaTeX to plain unicode text

**Try it in the browser: <https://phfaist.github.io/techxt/>** — paste LaTeX, read
plain text. It runs entirely on your device, works offline, and is the fastest
explanation of what techxt does.

`techxt` converts documents written in a LaTeX-like language into readable plain
text: `\emph{hi}` becomes `ℎ𝑖`, `\"o` becomes `ö`, `$\sum_{i=1}^n x_i$` becomes
`∑ᵢ₌₁ⁿ 𝑥ᵢ`, an `itemize` becomes a bulleted list, and a `tabular` becomes an
aligned text table. It is built on the [`techy`](https://github.com/phfaist/techy)
parser, and is a from-scratch redesign of the capabilities of
[`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc) — pylatexenc is an
idea source and a porting reference, not a compatibility target.

Two things set it apart from the usual "strip the macros" converter. Content
conversion and *text layout* are separate stages, so wrapping, blank lines and
indentation are decided once from complete information instead of macro by macro —
which means a `\textbf{ccc ddd}` can wrap between its two words, a `verbatim` body
survives byte for byte, and no combination of constructs produces a double blank
line. And every macro has a single definition carrying both its parse-time argument
structure and its rendering rule, so the parser and the renderer cannot disagree
about what a macro's arguments are. Unknown constructs and skipped content produce
structured diagnostics with source positions rather than silent surprises.

See [`PLAN.md`](PLAN.md) for the normative design, and the crate documentation
(`cargo doc --open -p techxt`) for the API.

## The library

```rust
use techxt::Converter;

// One converter, any number of documents, from any number of threads.
let converter = Converter::standard();

let conversion = converter
    .latex_to_text(r"\section{Intro} A fact\footnote{Why.}.")
    .expect("a tolerant parse of well-formed input succeeds");

assert_eq!(
    conversion.text,
    "1 Intro\n-------\n\nA fact[1].\n\n---\n[1] Why.\n",
);
assert!(conversion.diagnostics.is_empty());
```

Configure one through the builder — wrapping, how formulas are rendered, heading and
footnote styles, what an unknown macro becomes, a resolver for `\input`:

```rust
use techxt::Converter;

let converter = Converter::builder()
    .wrap_width(Some(72))
    .keep_comments(true)
    .build()
    .expect("a well-formed definition set builds");
```

Extend it with definitions of your own — a category pushed after the shipped library
shadows it — or with a handler that runs code, or by wrapping techxt's recomposer to
take over the nodes you care about. The crate documentation has a worked example of
each.

## The command-line program

```sh
cd rust
cargo run --bin techxt -- --help
cargo run --bin techxt -- --wrap 78 paper.tex -o paper.txt
```

```
techxt [OPTIONS] [FILE]        # FILE or stdin → stdout (or -o FILE)
  -o, --output <FILE>
      --math-mode <fancy|plain|source>
      --math-wrap <parens|braces|none>
      --matrix-delims <unicode|ascii>
  -w, --wrap <COLS>
      --keep-comments
      --heading-style <numbered-underlined|underlined|prefix|plain>
      --footnote-style <collected|inline|skip>
      --unknown-macro <skip|render-args|keep-source|placeholder>
      --input-dir <DIR>       # sandboxed \input resolution, rooted here
      --strict
      --no-macro-definitions  # read \newcommand and \def without honouring them
      --expansion-depth-limit <N>   # default 64
      --expansion-count-limit <N>   # default 2000
  -q / -v                     # -q: no diagnostics; default: warnings; -v: notes too
```

The two expansion budgets are what bound a runaway definition — techy-xp detects no
cycles — and they default well below techy-xp's own (256 / 100 000) because techxt
converts documents it did not write. Raise them for a long document that reports
having run out of budget.

Diagnostics go to standard error. Exit code 0 means the document converted and
nothing was wrong with it, 1 that it converted but the diagnostics contain errors,
and 2 that it could not be converted at all (a strict-mode parse failure, or an I/O
error).

## Repository layout

The repository root is language-neutral; each language binding lives in its own
sibling folder and does not talk to the others' build systems.

```
rust/          Cargo workspace
  techxt/        library crate (no_std + alloc)
  techxt-cli/    binary crate (std), installs as the `techxt` command
web/           the browser app: a wasm binding, a static single-page tool, and
               the project's home page — see web/README.md
tools/         dev-only scripts (symbol-table generation)
python/        planned: maturin extension
js/            planned: wasm/Node bindings
```

`web/` brings its own toolchain (npm, `wasm-pack`) and its own CI workflow, and its
`web/crate/` binding is deliberately *not* a member of the `rust/` workspace, so
`cd rust && cargo test` neither sees it nor is slowed by it.

The Rust workspace is built from `rust/`:

```sh
cd rust
cargo test --workspace
cargo build -p techxt --target thumbv7em-none-eabihf   # the no_std proof
```

The library is `no_std` + `alloc` and its runtime dependencies are exactly `techy`,
[`techy-xp`](https://github.com/phfaist/techy-xp) (techy's latexlike preset carrying
macro expansion) and `unicode-width`; there are no cargo features. Every `techy` or
`techy-xp` type that appears in techxt's public API is re-exported from
`techxt::convert`, so an embedder takes one dependency and not three — the
command-line program in `techxt-cli/` is the proof of it, and depends on `techxt`,
`clap` and `stacker` alone.

## Status and availability

Version 0.1.0: everything [`PLAN.md`](PLAN.md) specifies is implemented, and CI holds
the tree to `cargo fmt --check`, `clippy -D warnings`, denied rustdoc warnings, the
1.86 MSRV, and a `no_std` build for `thumbv7em-none-eabihf`.

**Not published to crates.io.** `techxt` depends on `techy` and `techy-xp` through
pinned git revisions, and a crates.io release cannot carry a git dependency — so
techxt is used from a git dependency of its own until those are published:

```toml
[dependencies]
techxt = { git = "https://github.com/phfaist/techxt", rev = "..." }
```

See [`CHANGELOG.md`](CHANGELOG.md) for what is in this version.

## License and author

MIT — see [`LICENSE`](LICENSE). Copyright © Philippe Faist.
