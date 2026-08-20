# techxt — LaTeX to plain unicode text

**Try it in the browser: <https://phfaist.github.io/techxt/>**

*Techxt* converts LaTeX code into readable plain
text: `\emph{hi}` becomes `ℎ𝑖`, `\"o` becomes `ö`, `$\sum_{i=1}^n x_i$` becomes
`∑ᵢ₌₁ⁿ 𝑥ᵢ`, an `itemize` becomes a bulleted list, and a `tabular` becomes an
aligned text table. It is built on the [`techy`](https://github.com/phfaist/techy)
parser, and is a from-scratch Rust-based redesign of the capabilities of
[`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc).

*Techxt* is:
- A web app (here: https://phfaist.github.io/techxt/) that you can install and
  run offline as a *Progressive Web App*;
- A Rust library;
- A command-line tool.

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

Note that everything is still considered in active alpha development stage — expect
breaking API changes anytime for now.

## The command-line program

Experiment in this repo as:
```sh
cd rust
cargo run --bin techxt -- --help
cargo run --bin techxt -- --wrap 78 paper.tex -o paper.txt
```

Install it with:
```sh
cargo install --path rust/techxt-cli
```

Command-line options:
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
  -q / -v                     # -q: no diagnostics; default: warnings; -v: notes too
```

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

The library is `no_std` + `alloc` and its runtime dependencies are exactly `techy`
and `unicode-width`; there are no cargo features. Every `techy` type that appears in
techxt's public API is re-exported from `techxt::convert`, so an embedder takes one
dependency and not two — the command-line program in `techxt-cli/` is the proof of
it, and depends on `techxt`, `clap` and `stacker` alone.
