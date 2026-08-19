# techxt — LaTeX-like markup to plain (unicode) text

`techxt` converts documents written in a LaTeX-like language into readable plain
text: `\emph{hi}` becomes `hi`, `\"o` becomes `ö`, `\frac{1}{2}` becomes `(1/2)`,
an `itemize` becomes a bulleted list, a `tabular` becomes an aligned text table.
It is built on the [`techy`](https://github.com/phfaist/techy) parser, and is a
from-scratch redesign of the capabilities of
[`pylatexenc.latex2text`](https://github.com/phfaist/pylatexenc) — pylatexenc is an
idea source and a porting reference, not a compatibility target.

Two things set it apart from the usual "strip the macros" converter: content
conversion and *text layout* are separate stages, so wrapping, blank lines and
indentation are decided once from complete information instead of macro by macro;
and every macro has a single definition carrying both its parse-time argument
structure and its rendering rule, so the parser and the renderer cannot disagree.
Unknown constructs and skipped content produce structured diagnostics with source
positions rather than silent surprises.

See [`PLAN.md`](PLAN.md) for the normative design and the milestone list.

## Repository layout

The repository root is language-neutral; each language binding lives in its own
sibling folder and does not talk to the others' build systems.

```
rust/          Cargo workspace
  techxt/        library crate (no_std + alloc)
  techxt-cli/    binary crate (std), installs as the `techxt` command
tools/         dev-only scripts (symbol-table generation)
python/        planned: maturin extension
js/            planned: wasm/Node bindings
```

The Rust workspace is built from `rust/`:

```sh
cd rust
cargo test --workspace
cargo run --bin techxt -- --help
```

## Status

Early development, version 0.1.0, and **not published to crates.io**: `techxt`
depends on `techy` through a pinned git revision, and a crates.io release cannot
carry a git dependency. Publication waits until `techy` itself is published.

## License and author

MIT. Copyright © Philippe Faist.
