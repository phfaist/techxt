# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The library version is untouched by any of this: `web/` is an application built on
techxt, not a change to it.

### Added

- **The browser app (`web/`).** A static, installable single-page tool that converts
  LaTeX-like markup to plain text entirely on the device, and doubles as the
  project's home page at <https://phfaist.github.io/techxt/>. Two panes with live,
  debounced conversion in a Web Worker; the conversion options of
  [`web/PLAN.md`](web/PLAN.md) §5; techxt's diagnostics shown as the structured,
  positioned things they are, with click-to-select in the input; five self-hosted
  unsubsetted display faces behind per-glyph fallback chains; a service worker, so
  it installs and works with the network off. No document ever leaves the browser
  and the page contacts nothing third-party.
- **The wasm binding (`web/crate/`).** A standalone `wasm-bindgen` package —
  deliberately outside the `rust/` workspace — exposing a cached `Converter`, an
  options DTO whose absent fields mean *the library's* defaults, and diagnostics
  remapped from UTF-8 byte offsets to the UTF-16 offsets a `<textarea>` wants. It is
  app-private and is not the `js/` package [`PLAN.md`](PLAN.md) §17 anticipates.
- **`.github/workflows/web.yml`.** `fmt`, `clippy` and `cargo test` for the binding
  (the gates it loses by living outside `rust/`), a wasm size budget, a glyph
  coverage gate on the default display face, a font size budget, and deployment to
  GitHub Pages on a push to `main`. `ci.yml` is untouched and stays `rust/`-scoped.

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
