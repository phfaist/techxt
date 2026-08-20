# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **`MathMode::Source` is now honored by every spelling of a formula.** It was
  answered by the math *groups* alone — `$…$`, `\(…\)`, `\[…\]`, `$$…$$` — so a
  formula written as a math environment (`equation`, `align`, `gather`, a bare
  matrix, …) or as `\ensuremath` was converted instead of re-emitted, and one
  document could show the same equation both ways. Every construct that opens a math
  scope now re-emits its LaTeX in that mode and does not enter its body; display
  formulas take a block of their own and inline ones stay in the running text, as the
  groups already did. `Fancy` and `Plain` are unchanged.

## [0.1.0] — 2026-08-19

First version: everything [`PLAN.md`](PLAN.md) specifies, implemented and tested.
Both crates (`techxt`, `techxt-cli`) carry this version.

Not published to crates.io — `techxt` depends on `techy` through a pinned git
revision, and a crates.io release cannot carry a git dependency. Use it as a git
dependency until `techy` itself is published.

### Added

- **The conversion pipeline.** `Converter` (`Clone + Send + Sync`, reusable across
  documents and threads) with three entry points: `latex_to_text` for text in and
  text out, `tree_to_text`/`tree_to_flow` for a `techy` node tree you parsed or
  transformed yourself, and `Converter::renderer` for driving the fold by hand.
- **The flow model and the layout engine.** Handlers emit typed tokens — words,
  breakable glue, vertical separation requests, blocks, verbatim runs — and one pass
  decides every line break, blank line and indent. Guarantees: at most one blank line
  anywhere, none at either end; no line ending in collapsible whitespace; verbatim
  content byte for byte; wrapping that crosses macro boundaries but never splits a
  word.
- **The definitions model.** One entry carries both a construct's parse-time argument
  structure and its text rule, so the parser and the renderer cannot disagree.
  Rules are a fixed literal, a template over named arguments, "render the content",
  "skip", or a handler that runs code. Categories stack into a `DefinitionSet` in
  which later categories shadow earlier ones, and `ConverterBuilder::override_macro`
  (and its environment and specials siblings) replaces a single rule.
- **The default definitions library** (`techxt::defs`), one module per category:
  escapes, spacing and ligatures; accents composed to precomposed characters; font
  styles through unicode's mathematical alphabets; sectioning with numbering and
  underlines; lists with per-depth markers; verbatim; tables aligned in columns;
  theorem environments, floats, captions, quotations and abstracts; footnotes
  (collected, inline or skipped); cross-references and citations including natbib;
  links; graphics placeholders; the title block; the preamble and the `document`
  environment; `\input`/`\include` against a caller-supplied resolver; and a
  generated ~1000-entry symbol table ported from pylatexenc.
- **Mathematics**, in three modes (`Fancy`, `Plain`, `Source`): an atom model with
  TeX's spacing classes, sub- and superscripts through unicode's script characters
  (with the space-stripping retry that makes `$\sum_{i=1}^n x_i$` come out
  `∑ᵢ₌₁ⁿ 𝑥ᵢ`), `\frac` and `\sqrt` with automatic operand wrapping, the display
  environments, and matrices — inline as one line, displayed as a box with
  delimiters drawn to height in unicode or ASCII.
- **Diagnostics.** Every unknown construct, dropped content and handler failure is
  reported as a structured condition with a source position, alongside the converted
  text; policies decide what an unknown macro, environment or specials *renders* as,
  independently of it being reported.
- **`techxt-cli`**, the `techxt` command: file or stdin to stdout or `-o`, with
  `--math-mode`, `--math-wrap`, `--matrix-delims`, `--wrap`, `--keep-comments`,
  `--heading-style`, `--footnote-style`, `--unknown-macro`, `--strict`, `-q`/`-v`,
  and `--input-dir` for sandboxed `\input` resolution (canonical-path containment,
  `.tex`/`.latex` fallback, include-cycle guard). Exit codes: 0 clean, 1 converted
  with errors reported, 2 not converted.
- **techy's types re-exported** from `techxt::convert` — `Diagnostics`, `Severity`,
  `Recovery`, `ParseError`, `SourceResolver`, `NodeRef`, `NodeTree`, `ArgumentSpec`,
  `TreeRecomposer`, the descent guard and the rest — so that embedding techxt takes
  one dependency and not two.

### Notes

- `no_std` + `alloc` for the library (CI builds it for `thumbv7em-none-eabihf`);
  runtime dependencies are exactly `techy` and `unicode-width`; no cargo features.
- MSRV 1.86, edition 2021, workspace `resolver = "3"` so that dependency resolution
  respects the MSRV.
- The library never panics on document input, however malformed.
- Deliberate omissions, documented in the crate docs rather than half-implemented:
  label/reference resolution, `\newcommand` expansion, numbering beyond headings,
  table spans and width-budgeted cells, centring simulation, localization of
  generated words, serde, subtree conversion entry points, generalization over the
  language, and the Python and JavaScript bindings.
