# techxt — build plan: the `xp` cargo feature and a language-generic parse side

**Written:** 2026-11-09, against `main` at `a7c0b53` (techxt 0.1.1, techy 0.1.3, techy-xp 0.1.0).
**Audience:** a supervisor agent running in a cloud session, and the implementer and
reviewer agents it launches. Nobody is available to answer questions during the run, so
this document is meant to be complete. Where it is not, §9.6 says what to do.

Sections marked **(normative)** are decisions. Do not change them; if one proves
unimplementable, stop and report (§9.6). Everything else is guidance: deviate when the
code demands it, and record the deviation in the status log (§11).

---

## 1. Goal

Make `techy-xp` an optional dependency of the `techxt` crate, behind a cargo feature
`xp` that is on by default, **additively**:

- With `xp` **off**, all of techxt is available through an API that is generic over the
  parse language `L`. techy's plain `Latexlike` is a ready-made `L`. `techy-xp` is
  neither compiled nor linked.
- With `xp` **on** (the default), the same generic API is present unchanged, plus
  everything that needs techy-xp: `LatexlikeXp` as a parse language, the honoured
  defining commands (`\newcommand`, `\def`, …), the expansion budgets, and today's
  argument-free constructors (`Converter::standard()`, `Converter::builder()`).
- Existing users on default features (the CLI in `rust/techxt-cli`, the wasm binding in
  `web/crate`) keep compiling with at most the one change named in §5.10, and see **no
  behaviour change at all**.

### Why

Another project (FLM, a Rust crate built on techy with its own language type) wants
techxt's text conversion without techy-xp's token-level expansion. Today that is
impossible: the only way to obtain a renderer is `ConverterBuilder::build()`, which
always constructs an `XpDriver` and a `Language<LatexlikeXp>`. Measured on native
release+LTO builds, a binary that only ever calls `tree_to_text` on a tree parsed by
plain techy still carries about 550 KiB of code naming `techy_xp` (about 215 KiB of
techy-xp's own, the rest techy's parser instantiated a second time for `LatexlikeXp`).

### Non-goals (normative)

- **No behaviour change under default features.** Same parser, same output, same
  diagnostics.
- **The source-origin type stays `Option<String>`.** `RenderLang` keeps requiring it.
  Lifting it (which FLM's own language type would need) is a separate piece of work.
- **No dependency changes** beyond making `techy-xp` optional. No version bumps of
  techy, techy-xp or anything else; `rust/Cargo.lock` and `web/crate/Cargo.lock` should
  not change. No new dependencies.
- **No version bump of techxt.** The maintainer does that.
- **No changes to the web app's TypeScript**, to the wasm size ceilings in
  `.github/workflows/web.yml`, or to rendering behaviour.
- **No subtree conversion entry point, no serde, no bindings.** Unrelated.

---

## 2. Ground rules (normative)

1. **Never run `cargo fmt`**, not even `cargo fmt --check`. This is the repository's
   `CLAUDE.md` rule and it applies to every agent. Write rustfmt-shaped code by hand
   (default style: 4 spaces, 100 columns, trailing commas in multi-line lists), edit
   narrowly, and do not reflow code you are not changing. CI has a formatting job; the
   maintainer deals with it.
2. **Only this repository is editable.** `techy` and `techy-xp` come from crates.io and
   are read-only. No `[patch]`, no path or git dependencies. If the work seems to need an
   upstream change, stop and report it (§9.6); do not work around it.
3. **Work on a branch** named `xp-feature`, created from `main`. Commit at the end of
   each phase once its gates are green (§9.5). Do not push to `main`, merge, force-push,
   tag, publish to crates.io, or open a pull request unless the person who launched the
   session asked for that. If the cloud session delivers its work by pushing its own
   working branch, push `xp-feature` only.
4. **Never weaken a gate or a test to get to green.** A test expectation may change only
   where §5 changes an API shape on purpose; every such edit is listed in the status log.
5. **Report honestly.** A gate that could not be run (missing tool, no network) is
   reported as *not run*, never as passed.
6. **`missing_docs` is `deny`.** Every new public item gets a doc comment in the crate's
   existing voice: what it is, why it exists, what the reader can rely on.

---

## 3. Environment (cloud)

Assume a fresh clone and nothing else. The supervisor sets up in Phase 0:

```sh
rustup toolchain install stable --profile minimal --component clippy
rustup toolchain install 1.86 --profile minimal
rustup target add thumbv7em-none-eabihf wasm32-unknown-unknown
(cd rust && cargo fetch)
(cd web/crate && cargo fetch)
```

- The Rust workspace is in `rust/` (members `techxt`, `techxt-cli`). The wasm binding is
  a separate package in `web/crate/`, deliberately outside that workspace.
- Upstream sources are readable after `cargo fetch` under
  `~/.cargo/registry/src/index.crates.io-*/techy-0.1.3/` and `…/techy-xp-0.1.0/`
  (versions per `rust/Cargo.lock`). Read them freely; never edit them.
- The web gate (§7, G12) additionally needs Node 22, `npm ci` in `web/`, and
  `wasm-pack`. If these cannot be installed, G12 is *not run* and the final report says
  so.
- Sibling checkouts mentioned in older docs (`../techy`, `../techy-xp`, FLM) do not exist
  in the cloud. Nothing in this plan needs them.

---

## 4. Current state (facts verified on 2026-11-09 at `a7c0b53`)

- `cargo test --workspace` in `rust/`: **953 passed, 0 failed** (unit, integration and
  doc tests together). Re-measure in Phase 0; that number is the floor from then on.
- `techy-xp` is an unconditional dependency (`rust/techxt/Cargo.toml`); there are no
  cargo features.
- 8 of the 50 files in `rust/techxt/src` name the parse language `LatexlikeXp` outside
  test code, 76 mentions in all:

  | File | Mentions | Uses real techy-xp items, not just the type |
  |---|---|---|
  | `def/spec.rs` | 27 | scope-name constants, `RefusalSpec` |
  | `def/set.rs` | 17 | `XpDriver` (in `ScopeFrame::honored`), `refusals_package` |
  | `def/entry.rs` | 10 | no |
  | `defs/preamble.rs` | 9 | the definers |
  | `defs/base.rs` | 4 | no |
  | `defs/inputs.rs` | 4 | no |
  | `convert.rs` | 3 | `XpDriver`, eight refusal condition types |
  | `defs/verbatim.rs` | 2 | no |

- The render side is already language-generic: `render/*` is generic over the *tree's*
  language `LLL: RenderLang`, and handlers see a language-erased `NodeView` and
  `RenderCx`. `flow`, `layout`, `mathfmt`, `diag`, `def::{rule, template, symbols}` and
  15 of the 19 `defs` categories never name a language.
- `ConverterBuilder::build()` falls back to `defs::standard` by naming the function
  (`convert.rs:882`), which keeps the whole definitions library linked even when the
  caller supplies a smaller set.
- techy's latexlike preset is generic over `LLL: LatexlikeLang` throughout, and its role
  traits give constructors for the vocabulary: `LatexlikeGroupType::content_group()`,
  `LatexlikeCallableType::{macro_callable, environment_callable, specials_callable}()`,
  `LatexlikeMode::math_mode()`, `LatexlikeEvent::exit_math_context()`. There is
  deliberately **no** text-mode constructor, and `BodySlotExt` has no "not a body"
  constructor.
- techy-xp's pieces are generic too (`XpDriver<LLL>`, `DefinerSpec<LLL>`,
  `RefusalSpec<LLL>`, `refusals_package::<LLL>()`), bounded by `LLL: LatexlikeLang` plus
  `LLL::StateExt: ExpansionExt` where they expand.
- Integration tests in `rust/techxt/tests/`: 35 files. Four do not use the xp
  conveniences at all (`layout.rs`, `layout_proptest.rs`, `math_proptest.rs`,
  `math_engine.rs`). Several import `techy_xp` directly.

The design in §5 was prototyped before this plan was written: the language trait, both
implementations, a generic build and the generic spec types compile and run on the 1.86
toolchain against techy 0.1.3 and techy-xp 0.1.0, with the feature on and off
(Appendix A). The public API shape was compiled the same way together with every call
pattern the CLI and the web crate use today (Appendix B).

---

## 5. Target design (normative)

### 5.1 The feature

```toml
# rust/techxt/Cargo.toml
[features]
default = ["xp"]
xp = ["dep:techy-xp"]

[dependencies]
techy.workspace = true
techy-xp = { workspace = true, optional = true }
unicode-width.workspace = true
```

`techxt-cli` and `web/crate` keep depending on techxt with default features; both use
xp-only items.

### 5.2 Additivity

1. **The feature never changes the signature or the meaning of any item.** It only adds
   items, impls and default type parameters.
2. `#[cfg(not(feature = "xp"))]` may appear in non-test code in exactly two roles: the
   undefaulted twin of a feature-gated generic parameter (§5.4), and the fallback bodies
   of the two functions in §5.8.
3. With `xp` off there is **no default language**. A bare `Converter` in type position
   must fail to compile. Do not alias the default to `Latexlike` when the feature is
   off: that would make the same source mean two different parsers depending on what
   else is in the dependency graph.
4. Code written against the feature-off API must compile and behave identically with the
   feature on.

### 5.3 Two language bounds

**`RenderLang`** (exists, `render/lang.rs`, unchanged): a tree of this language can be
rendered.

**`ParseLang`** (new): techxt's definitions can parse with this language. Put it in a
new `def/lang.rs`, export it as `techxt::def::ParseLang`, and re-export it from
`techxt::convert` beside `RenderLang`.

```rust
pub trait ParseLang:
    RenderLang<ModeId = Mode, NodeExts: NodeExtTypes<SlotExt = BodyMarker, ArgumentExt: Default>>
{
    /// Parse-time settings this language adds to the common ones.
    /// `()` for a language with none.
    type Settings: Clone + Default + core::fmt::Debug + Send + Sync + 'static;

    /// The parse driver of one converter.
    fn make_driver(
        recovery: Recovery,
        source_resolver: Option<&Arc<dyn SourceResolver<Option<String>>>>,
        settings: &Self::Settings,
    ) -> Self::Driver;

    /// Providers placed under and over techxt's categories. Default: none.
    fn scope_frame(driver: &Self::Driver, settings: &Self::Settings) -> ScopeFrame<Self> { … }

    /// The spec a defining command parses through, if this language honours it.
    /// Default: `None`, and the entry's declared argument shape is registered instead.
    fn definer_spec(
        definer: Definer,
        cx: &SpecBuildCx<'_, Self>,
    ) -> Option<Arc<dyn CallableSpec<Self>>> { … }
}
```

Why each bound is there:

| Bound | Needed by |
|---|---|
| `ModeId = techy::latexlike::Mode` | `text_mode_only()` entries: techy's mode role trait has no text-mode constructor, and `Package::insert_in_modes` takes `Vec<L::ModeId>` |
| `SlotExt = BodyMarker` | `defs::inputs` passes `BodyMarker::not_body()` to `InputMacroSpec::new`; the slot trait cannot build that value |
| `ArgumentExt: Default` | `argument_specs_named`, which turns argument codes (`"m"`, `"o"`, …) into specs |
| `SourceOrigin = Option<String>`, `SlotExt: BodySlotExt` | inherited from `RenderLang` |

The hooks exist because generic code can only behave differently for one language
through a trait method.

- `ScopeFrame<L>` becomes public: two lists of `Arc<dyn SpecsProvider<L>>`, `below` and
  `above`. The provider stack stays exactly as today, bottom to top: the unknown-macro
  fallback (when enabled), techy's builtin package, `below`, the categories in order,
  `above`.
- `Definer` becomes a public `#[non_exhaustive]` enum in `def/lang.rs` (it is private in
  `defs/preamble.rs` today). Same variants as now.
- `SpecBuildCx` becomes `SpecBuildCx<'a, L>` and borrows what a spec source may read:
  `source_resolver()`, `driver() -> &L::Driver`, `settings() -> &L::Settings`. Give it a
  public constructor taking the driver and settings. Today's argument-free
  `SpecBuildCx::new()` and `Default` go away (nothing outside `src/` uses them).

Implementations:

| Language | When | `Settings` | Driver | Frame | Definers |
|---|---|---|---|---|---|
| `techy::latexlike::Latexlike` | always | `()` | `LatexlikeDriver::new(recovery)`, plus the resolver | none | none (declared only) |
| `techy_xp::lang::LatexlikeXp` | `xp` | `XpSettings` | today's `XpDriver::wrapping(…)` with both budgets, plus the resolver | when honoured: `refusals_package` below, `driver.definition_scope_providers()` above; when declared: none | when honoured: today's `Definer::spec`; when declared: `None` |

`XpSettings` is a new public xp-only type holding what `ConverterBuilder` holds today as
`macro_definitions`, `expansion_depth_limit` and `expansion_count_limit`, with today's
defaults (`Honored`, 64, 2 000). The definers must keep reading the two scope names off
the driver (`cx.driver().local_scope_name()` / `global_scope_name()`): the comment at
`convert.rs:891` explains why they must come from the same object that seeds the scopes.

### 5.4 Generic types and their defaults

These gain a type parameter `L`:

| Module | Items |
|---|---|
| `convert` | `Converter`, `ConverterBuilder` |
| `def` | `MacroDef`, `EnvDef`, `SpecialsDef`, `Category`, `DefinitionSet`, `SpecBuildCx`, `TechxtMacroSpec`, `TechxtSpecialsSpec`, `TechxtEnvironmentBehavior`, trait `CallableSpecSource` |
| `defs` | `standard()` and every module's `category()` become generic functions |

These stay non-generic: `TextRule`, `TextHandler`, `Template`, `CallableKind`,
`SymbolIndex`, `SymbolEntry`, `Options`, `Conversion`, and everything in `flow`,
`layout`, `mathfmt`, `diag` and `render`.

Under `xp` each generic *type and trait* defaults `L` to `LatexlikeXp`; without it there
is no default. The pattern (verified on 1.86, including with a leading lifetime and on a
trait):

```rust
pub struct MacroDef<#[cfg(feature = "xp")] L = LatexlikeXp, #[cfg(not(feature = "xp"))] L>
where
    L: ParseLang,
{ … }

pub struct SpecBuildCx<'a, #[cfg(feature = "xp")] L = LatexlikeXp, #[cfg(not(feature = "xp"))] L>
where
    L: ParseLang,
{ … }
```

Bound each type by the least it needs. The three spec types
(`TechxtMacroSpec<L>`, `TechxtSpecialsSpec<L>`, `TechxtEnvironmentBehavior<L>`) must be
bounded by `LatexlikeLang` only, because the render side names them for an arbitrary
tree language (§5.7).

Inside the crate, never rely on the default: every signature spells `<L>`. The
feature-off build enforces this from Phase 3 on.

### 5.5 Constructors

**Generic door, always present** (`impl<L: ParseLang> ConverterBuilder<L>`):

- `ConverterBuilder::with_definitions(set: DefinitionSet<L>) -> Self` is the constructor.
  It takes the definition set explicitly, so the generic `build()` never names
  `defs::standard`.
- Every existing setter that is not xp-specific (`definitions`, `override_macro` and
  siblings, `recovery`, `descent_guard`, `source_resolver`, the unknown-macro
  resolution, all per-option setters), plus a new `language_settings(L::Settings)`.
- `build() -> Result<Converter<L>, BuildError>`.
- On `Converter<L>`: `latex_to_text`, `tree_to_text`, `tree_to_flow`, `language() ->
  &Language<L>`, `options`, `renderer`. `tree_to_text`/`tree_to_flow` stay generic over
  the tree's own language `LLL: RenderLang`, independent of `L`.

**xp conveniences** (`impl … <LatexlikeXp>` only, the way `HashMap::new()` exists for
one hasher):

- `Converter::standard()`, `Converter::builder()`, `ConverterBuilder::new()`,
  `impl Default for ConverterBuilder<LatexlikeXp>`.
- `macro_definitions`, `expansion_depth_limit`, `expansion_count_limit`, and the
  constants `DEFAULT_EXPANSION_DEPTH_LIMIT` / `DEFAULT_EXPANSION_COUNT_LIMIT`.
- These constructors default to `defs::standard()` **lazily**, as today: store the
  choice (a set, or a function that makes the default set) and resolve it in `build()`.
  The generic code path must hold only what it was given.

A method name must exist in exactly one of the two impl blocks. That is what lets
`Converter::standard()` and `ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT` resolve
without a type annotation.

### 5.6 What is xp-only (exhaustive)

1. `impl ParseLang for LatexlikeXp` and `XpSettings`.
2. The definers: the `Definer` → techy-xp spec mapping and techxt's own
   `\DeclareMathOperator` definer (`MathOperatorDefiner`), both moved out of
   `defs/preamble.rs`.
3. The xp conveniences of §5.5 and the `MacroDefinitions` enum.
4. Compatibility accessors on `SpecBuildCx<'_, LatexlikeXp>`: `macro_definitions()`,
   `local_scope_name()`, `global_scope_name()`.
5. The `techxt::convert::LatexlikeXp` re-export.
6. The default type parameters of §5.4.
7. The two gated function bodies of §5.8.

Keep these in as few places as possible: ideally one `#[cfg(feature = "xp")] mod xp;`
holding items 1–4, plus the unavoidable attributes for 5–7. The reviewer of Phase 3 lists
every occurrence of `feature = "xp"` in `src/` and justifies each one.

### 5.7 Definition tables

- **Data tables do not change**: `symbols_extra_data.rs`, `accents_data.rs`, and the
  `DEFINERS` / `DECLARATIONS` statics in `preamble.rs`.
- **Signatures only** in the 19 categories: `pub fn category() -> Category` becomes
  `pub fn category<L: ParseLang>() -> Category<L>`, and the 28 private helpers that take
  or return an entry type follow. Bodies, templates and handlers are untouched.
- **Six hand-built specs** switch to the role-trait constructors:

  | Function | Change |
  |---|---|
  | `optional_length` (`base.rs:169`), `adjacent_options` (`verbatim.rs:85`), `raw_group` (`preamble.rs:229`) | `GroupType::Content` → `L::GroupTypeId::content_group()` |
  | `exiting_math` (`base.rs:492`) | the same, and `Event::ExitMathContext` → `L::Event::exit_math_context()` |
  | `entering_math` (`base.rs:501`) | the group type as above; `Mode::Math` may stay |
  | `Resolving` (`inputs.rs:119`) | `impl<L: ParseLang> CallableSpecSource<L>`; `LatexlikeXp` → `L` |

  `def/set.rs` likewise uses `L::CallableTypeId::macro_callable()` and siblings instead
  of `CallableType::Macro`.
- **Preamble**: the `Definers` spec source stops deciding by itself and asks the
  language: `L::definer_spec(self.0, cx)`. `None` means the argument codes written
  beside the entry in `DEFINERS` are registered, which is exactly today's
  `MacroDefinitions::Declared` behaviour.

### 5.8 Render-side dispatch

- `embedded_rule<LLL>` and `TechxtEnvironmentBehavior::of` downcast to the spec type
  instantiated at the **tree's** language: `TechxtMacroSpec<LLL>`,
  `EnvironmentSpec<LLL>`, `TechxtEnvironmentBehavior<LLL>`. Consequence, and a
  requirement: a tree parsed by any techxt converter carries rules that any other
  converter's renderer finds, whatever language either was built for.
- `is_refusal<LLL>`: under `xp`, test for `RefusalSpec<LLL>`; otherwise return `false`.
- `at_techxt_severity` (`convert.rs:464`): under `xp`, today's re-levelling of the eight
  techy-xp refusal conditions; otherwise return the diagnostic unchanged.

### 5.9 Behaviour contract

- **Default features:** identical to today. The existing test suite is the proof; it
  passes without edits to its expectations.
- **`Converter<Latexlike>`:** for any input and options, the same `text` as
  `Converter<LatexlikeXp>` built with `MacroDefinitions::Declared`, and the same
  diagnostics compared by severity and message. Do not compare spans: the two languages'
  span regimes differ by design (see the comment in `tests/def_dispatch.rs` near
  "over plain techy").
- `Converter<L>` is `Clone` for every `L`. It is `Send + Sync` for `Latexlike` and
  `LatexlikeXp`; assert both in tests.
- The crate stays `no_std + alloc` in both feature states.

### 5.10 The one source change for existing users

`web/crate/src/complete.rs:162` calls `defs::standard()` and only asks it for symbols,
so nothing fixes `L`. It becomes:

```rust
let definitions: DefinitionSet = defs::standard();
```

Every other call pattern in `rust/techxt-cli` and `web/crate` compiles as written
(checked: `Converter::builder()`, `Converter::standard()`, bare `Converter` in
signatures and fields, `ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT`,
`MacroDefinitions`). If another existing call site stops compiling, treat that as a
defect in the implementation first, not as a second allowed change.

---

## 6. Known traps

1. **`derive` adds bounds on `L`.** `#[derive(Clone, Debug, Default)]` on a generic type
   demands `L: Clone + Debug + Default`. Languages are marker types that owe nobody
   those. Write the impls by hand, as techy and techy-xp do for their drivers, so that
   `CallableSpec<L>` (which needs `Debug`) holds for every `L`.
2. **Duplicate method names across impl blocks** make `Type::method()` ambiguous or a
   hard error. One name, one block (§5.5).
3. **Type inference.** A generic function called for a result that never reaches a typed
   place cannot infer `L` (that is §5.10). Inside the crate's own tests and docs, prefer
   an annotation on the binding over a turbofish.
4. **Work bottom-up in Phase 1.** With an (at that point unconditional) default of
   `LatexlikeXp`, each layer compiles before the next one changes: spec types, then
   entries, then the set, then the tables, then `convert`.
5. **Unused imports under one feature state.** Clippy runs with `-D warnings` in both.
   Keeping xp-only code in its own module avoids most of it.
6. **The vocabulary.** Any remaining `GroupType::`, `CallableType::` or `Event::` in
   non-test code outside the xp module is a hidden assumption that `L` uses techy's own
   vocabulary types. `Mode::` is fine (§5.3).
7. **Lifetimes come before type parameters**, so the default goes last:
   `SpecBuildCx<'a, L = …>`.
8. **Doc links.** Docs on always-compiled items may link to xp-only items; the
   documentation gate runs with default features only (§7). Do not contort docs to make
   `cargo doc --no-default-features` link-clean.
9. **Doctests use the xp conveniences**, so they are not run in the feature-off gate.
10. **MSRV is 1.86.** Everything in §5 was checked there. No newer language features.

---

## 7. Gates

The **supervisor** runs these itself; an agent's claim that they pass is not evidence.
From `rust/` unless stated.

| | Command | From |
|---|---|---|
| G1 | `cargo clippy --workspace --all-targets -- -D warnings` | Phase 0 |
| G2 | `cargo test --workspace` (passed count ≥ the recorded floor) | Phase 0 |
| G3 | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | Phase 0 |
| G4 | `cargo +1.86 check --workspace --all-targets` | Phase 0 |
| G5 | `cargo build -p techxt --target thumbv7em-none-eabihf` | Phase 0 |
| G6 | in `web/crate`: `cargo clippy --all-targets -- -D warnings` and `cargo test` | Phase 0 |
| G7 | `cargo clippy -p techxt --no-default-features --all-targets -- -D warnings` | Phase 3 |
| G8 | `cargo test -p techxt --no-default-features --lib --tests` | Phase 3 |
| G9 | `cargo +1.86 check -p techxt --no-default-features --all-targets` | Phase 3 |
| G10 | `cargo build -p techxt --no-default-features --target thumbv7em-none-eabihf` | Phase 3 |
| G11 | `cargo tree -p techxt --no-default-features -e normal` contains no `techy-xp` | Phase 3 |
| G12 | in `web/`: `npm ci`, `npm run wasm`, the size check from `web.yml`, `npm run typecheck`, `npm test` | Phase 4 only, if the tools exist |

`cargo fmt` is not a gate and is never run (§2).

---

## 8. Phases

Each phase leaves every gate that applies to it green and ends with one commit.

### Phase 0 — setup and baseline (supervisor, no subagent)

1. Create `xp-feature` from `main`. Install toolchains (§3).
2. Run G1–G6 on the untouched tree. Record the G2 passed count in the status log; that is
   the floor. (Expected: 953.)
3. If the baseline is not green, stop and report. Do not fix unrelated breakage.

### Phase 1 — the generic parse side, behaviour-neutral

techy-xp is still a mandatory dependency in this phase and the default type parameter
`L = LatexlikeXp` is unconditional. `LatexlikeXp` is the only `ParseLang`.

Scope:

1. `def/lang.rs`: `ParseLang`, `ScopeFrame<L>`, `Definer`, `SpecBuildCx<'a, L>` (§5.3).
2. A new xp module (not yet feature-gated) with `impl ParseLang for LatexlikeXp`,
   `XpSettings`, and the definer code moved from `defs/preamble.rs`.
3. `def/spec.rs`, `def/entry.rs`, `def/set.rs` generic (§5.4); render-side downcasts per
   §5.8 (without the cfg yet).
4. The definition tables per §5.7.
5. `convert.rs` per §5.5: `Converter<L>`, `ConverterBuilder<L>`, the generic door and
   `build()`, the xp conveniences in their own impl block, lazy default definitions.
6. `web/crate/src/complete.rs:162` per §5.10.

Exit criteria:

- G1–G6 green; G2 count ≥ floor.
- No test expectation changed. No edits under `rust/techxt/tests/` or in `#[cfg(test)]`
  modules except mechanical ones forced by §5.3's `SpecBuildCx` reshape, each listed.
- In non-test code, `LatexlikeXp` and `techy_xp::` appear only in the xp module, in the
  default type parameters, in the xp-conveniences impl block, in `is_refusal` and
  `at_techxt_severity`, and in re-exports. Check with `grep`.

Review focus: the provider stack order is unchanged; the `Declared` path registers
exactly what it did; scope names still come off the driver; nothing on the render side
gained a bound beyond `RenderLang`/`LatexlikeLang`; hand-written `Clone`/`Debug`; docs on
every new public item.

### Phase 2 — a second language, and the tests that prove the seam

Scope:

1. `impl ParseLang for Latexlike`, always compiled, with full docs.
2. `tests/lang_generic.rs`, using **only** the generic API and `Latexlike`, so that it
   compiles in both feature states. At least:
   - end-to-end conversion through `ConverterBuilder::<Latexlike>::with_definitions(defs::standard())`;
   - a defining command is declared, not honoured (`\newcommand\greet{Hi}\greet` consumes
     its arguments, `\greet` is reported as unknown);
   - a text-only and a math-only entry, a verbatim environment, a list, a table;
   - `\input` through a `MapResolver`;
   - **dispatch step 2 across converters**: parse with a `Converter<Latexlike>` that has a
     custom definition, render the tree with a *different* converter built from an empty
     `DefinitionSet`, and get the custom rule's output (it can only have come from the
     spec embedded in the tree);
   - `Clone`, `Send`, `Sync` for `Converter<Latexlike>`.
3. `tests/lang_parity.rs` (needs xp): the §5.9 parity contract over at least 40 documents
   that between them touch every category of `defs::standard()`, all three math modes, a
   preamble with defining commands, an unknown macro, and `\input`.
4. Fix whatever the second instantiation exposes.

Exit criteria: G1–G6 green; both new files pass; G2 count = floor + the new tests.

Review focus: would each new test fail if the thing it claims were false (for instance,
if `Converter<Latexlike>` quietly expanded macros, or if step 2 fell through to the name
table)? Is the parity corpus broad, or forty variations of one thing? If parity fails
because of upstream behaviour, that is a finding to report, not a test to relax.

### Phase 3 — the feature

Scope:

1. `Cargo.toml` per §5.1, and the manifest comments that became false ("Exactly three
   runtime dependencies", the workspace note listing the runtime set).
2. Gate the xp module, the re-export, the default type parameters (§5.4 pattern) and the
   two function bodies of §5.8.
3. Remove every in-crate reliance on the default type parameter.
4. Test gating: `#![cfg(feature = "xp")]` at the top of each integration test file that
   needs xp; `#[cfg(all(test, feature = "xp"))]` (or per-test attributes) for unit-test
   modules in `src/` that do. Do not gate `layout.rs`, `layout_proptest.rs`,
   `math_proptest.rs`, `math_engine.rs` or `lang_generic.rs`.
5. `.github/workflows/ci.yml`: add the feature-off counterparts G7–G10 to the clippy,
   test, MSRV and no_std jobs. Leave the formatting job alone.

Exit criteria: G1–G11 green. With default features the G2 count has not dropped
(gating must not silently remove a test from the default run).

Review focus: an additivity audit against §5.2, occurrence by occurrence; that a bare
`Converter` does not compile with the feature off (try it in a scratch file, do not
commit it); that `lang_generic.rs` compiles and passes in both states; that nothing
changed meaning between states.

### Phase 4 — documentation, consumers, final verification

Scope:

1. Make the prose true again. Known stale statements:
   - `rust/techxt/src/lib.rs`: "Converting a tree parsed elsewhere" (a converter no longer
     parses with exactly one language), "Crate dependencies and panic policy" ("There
     are no cargo features", the dependency list, the dead-code-elimination claim), and
     the "Generalization of the parse side" bullet under "Deliberate omissions" (now
     done, except the origin type and the tree annotation). Add a short section on
     languages and the `xp` feature, with an example written against the generic API
     only. By §5.2 rule 4 it compiles under default features too, so it runs as an
     ordinary doctest.
   - Module docs of `def`, `convert` and `defs::preamble`.
   - `README.md` near line 130 ("there are no cargo features").
   - `dev-docs/PLAN.md`: add amendments in its existing "**Amended for …**" style at the
     two places that state the runtime set and the concrete parse side (near lines 89 and
     113). Do not rewrite the plan.
2. Final measurements, **report only, not gates**. In a throwaway crate outside the
   repository, build (release, LTO) a binary that calls `latex_to_text` through
   `Converter<Latexlike>` with `default-features = false`, and one through
   `Converter::standard()` with default features. Report both sizes and whether any
   symbol naming `techy_xp` survives in the first (`nm -C … | grep -c techy_xp`).
3. G12 if the tools are available; report the wasm size against the ceilings in
   `web.yml` without touching them.

Exit criteria: G1–G11 green, G12 green or reported as not run, final report written
(§10).

Review focus: is every statement in the changed docs true of the code as it now is; did
anything stale survive (`grep -rn "no cargo features\|exactly three\|Exactly three"`).

---

## 9. Supervisor protocol

### 9.1 Roles

- **Supervisor (the main agent):** owns the branch, runs the gates, triages review
  findings, commits, keeps the status log, writes the final report. Does not write
  product code itself beyond trivial integration fixes.
- **Implementer:** one per phase, launched as a subagent on the **Opus** model
  (`model: "opus"`). Does the phase's scope.
- **Reviewer:** one per phase, a **separate, fresh** subagent on the **Opus** model. Reads
  and runs, does not edit.

Phases are sequential and share one working tree. Do not run two implementers at once.

### 9.2 What an implementer is told

Give it: the path of this file; its phase number; the instruction to read §1–§7, its own
phase in §8, and the appendices before touching code; the current commit; and these
standing rules verbatim: *never run `cargo fmt`; edit only this repository; do not
commit; do not change a test's expectation; when a normative rule cannot be met, stop
and say so instead of working around it.* Ask it to finish with: what it changed, what it
deviated from and why, every test file it touched and why, and which gates it ran with
their actual results.

### 9.3 What a reviewer is told

Give it: the path of this file, the phase number, and the commit range to review
(`git diff <phase base>..HEAD` plus the working tree). **Do not give it the implementer's
summary**; it should form its own reading. Ask for findings ranked by severity, each with
file and line, the concrete failure it would cause, and whether it violates a normative
rule (naming the section). It should check the phase's review focus in §8, the ground
rules in §2, and that the phase's exit criteria are really met. It may run any gate
except `cargo fmt`.

### 9.4 The loop, per phase

1. Launch the implementer. When it returns, run the phase's gates yourself.
2. Red gate: send the failing output back to the implementer and repeat.
3. Green: launch the reviewer.
4. Triage. Every finding that is a correctness defect or breaks a normative rule gets
   fixed. A finding you decline is recorded in the status log with the reason.
5. Fixes go to an implementer (the same one if still addressable, otherwise a new one
   given the findings). Re-run the gates. For non-trivial fixes, have a reviewer check
   the fix diff.
6. At most two fix rounds per phase. If the phase is still not acceptable, stop (§9.6).
7. Commit, update the status log, move on.

### 9.5 Commits

One commit per phase on `xp-feature`, made by the supervisor after the gates are green,
with a short imperative subject in the style of `git log`. Include the status-log update
in the same commit. A clearly labelled work-in-progress commit is acceptable as a safety
checkpoint before a risky step or when stopping early.

### 9.6 When to stop

Stop the run, commit what exists as clearly labelled work in progress, and write the
final report, if any of these holds:

- the work needs a change in techy or techy-xp;
- a normative rule in §5 cannot be implemented as written;
- a phase is not acceptable after two fix rounds;
- the Phase 0 baseline is not green;
- the parity contract of §5.9 fails for a reason that lies upstream.

Say exactly what blocked, what you tried, and what decision is needed. Do not guess at a
normative question to keep going.

---

## 10. Final report

Written by the supervisor at the end (or at a stop), as the last status-log entry and as
its closing message:

- Phases completed, with commit hashes.
- Each gate: passed, failed (with output), or not run (with reason).
- Test counts: the floor, and the final count in both feature states.
- Every public API change, as a list a maintainer could paste into release notes. This
  work changes inference for generic functions and removes `SpecBuildCx::new()`, so it
  belongs in a 0.2.0; say so, and leave the version alone.
- Every deviation from this plan and every declined review finding.
- The Phase 4 measurements.
- Anything a maintainer must still do by hand: run rustfmt, bump the version, push.

---

## Appendix A — the seam, as prototyped

Abridged from a prototype that compiled and ran on 1.86 against techy 0.1.3 and
techy-xp 0.1.0, with the feature on and off. It shows the shape; the compiler is the
authority on details. With the feature off, the dependency tree held techy only.

```rust
pub trait ParseLang:
    RenderLang<ModeId = Mode, NodeExts: NodeExtTypes<SlotExt = BodyMarker, ArgumentExt: Default>>
{
    type Settings: Clone + Default + fmt::Debug + Send + Sync + 'static;
    fn make_driver(recovery: Recovery, resolver: Option<&Resolver>, settings: &Self::Settings)
        -> Self::Driver;
    fn scope_frame(_driver: &Self::Driver, _settings: &Self::Settings) -> ScopeFrame<Self> {
        ScopeFrame::none()
    }
    fn definer_spec(_definer: Definer, _cx: &SpecBuildCx<'_, Self>)
        -> Option<Arc<dyn CallableSpec<Self>>> {
        None
    }
}

impl ParseLang for Latexlike {
    type Settings = ();
    fn make_driver(recovery: Recovery, resolver: Option<&Resolver>, _: &()) -> Self::Driver {
        let driver = LatexlikeDriver::new(recovery);
        match resolver {
            Some(r) => driver.with_source_resolver(Arc::clone(r)),
            None => driver,
        }
    }
}

#[cfg(feature = "xp")]
impl ParseLang for LatexlikeXp {
    type Settings = XpSettings;
    fn make_driver(recovery: Recovery, resolver: Option<&Resolver>, s: &XpSettings) -> Self::Driver {
        let driver = XpDriver::wrapping(LatexlikeDriver::new(recovery))
            .with_expansion_depth_limit(s.depth)
            .with_expansion_count_limit(s.count);
        match resolver {
            Some(r) => driver.with_source_resolver(Arc::clone(r)),
            None => driver,
        }
    }
    fn scope_frame(driver: &Self::Driver, s: &XpSettings) -> ScopeFrame<Self> {
        if !s.honored {
            return ScopeFrame::none();
        }
        ScopeFrame {
            below: vec![refusals_package::<LatexlikeXp>()],
            above: driver.definition_scope_providers().to_vec(),
        }
    }
    fn definer_spec(definer: Definer, cx: &SpecBuildCx<'_, Self>)
        -> Option<Arc<dyn CallableSpec<Self>>> {
        if !cx.settings().honored {
            return None;
        }
        let (local, global) = (cx.driver().local_scope_name(), cx.driver().global_scope_name());
        let spec = match definer {
            Definer::NewCommand => new_command_definer(),
            Definer::Def => def_definer(),
        };
        Some(Arc::new(
            spec.with_definition_scopes(local, global).with_redefinition(RedefinitionRule::Always),
        ))
    }
}

// Spec types are bounded by `LatexlikeLang` only, with hand-written `Debug`.
pub struct TechxtMacroSpec<L: LatexlikeLang> {
    arguments: Vec<Arc<ArgumentSpec<L>>>,
    rule: Option<TextRule>,
}
impl<L: LatexlikeLang> SerializableObject<L> for TechxtMacroSpec<L> {}
impl<L: LatexlikeLang> CallableSpec<L> for TechxtMacroSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

// The render side downcasts at the tree's own language.
pub fn embedded_rule<LLL: LatexlikeLang>(node: NodeRef<'_, LLL>) -> Option<&TextRule> {
    let spec = node.spec()?;
    (&**spec as &dyn Any).downcast_ref::<TechxtMacroSpec<LLL>>()?.rule.as_ref()
}

// An environment behaviour reaches math mode through the role trait.
impl<L: LatexlikeLang> EnvironmentBehavior<L> for MathEnv<L> {
    fn body_state_delta(&self, _i: EnvironmentInvocation<'_, L>)
        -> Result<Option<ParsingStateDelta<L>>, ParseError<L::SourceOrigin>> {
        Ok(Some(ParsingStateDelta::new().mode(L::ModeId::math_mode())))
    }
}

// What `ConverterBuilder::<L>::build` and `DefinitionSet::<L>::build` do between them.
pub fn build<L: ParseLang>(recovery: Recovery, settings: &L::Settings) -> Language<L> {
    let driver = L::make_driver(recovery, None, settings);
    let frame = L::scope_frame(&driver, settings);
    let cx = SpecBuildCx::<L> { source_resolver: None, driver: &driver, settings };

    let mut providers: Vec<Arc<dyn SpecsProvider<L>>> = Vec::new();
    let mut unknown = FallbackProvider::<L>::new("techxt-unknown");
    let ruleless: Arc<dyn CallableSpec<L>> = Arc::new(MacroSpec::new(Vec::new()));
    unknown.set(L::CallableTypeId::macro_callable(), ruleless);
    providers.push(Arc::new(unknown));
    providers.push(builtin_package());
    providers.extend(frame.below);

    let mut package = Package::<L>::new("a-category");
    let arguments = argument_specs_named::<L, _, _, _>([("m", "text")]).expect("codes");
    let bold: Arc<dyn CallableSpec<L>> = Arc::new(TechxtMacroSpec { arguments, rule: … });
    package.insert_in_modes(L::CallableTypeId::macro_callable(), "textbf", bold, Some(vec![Mode::Text]));
    // The language claims the definer, or the declared form is registered.
    let spec = L::definer_spec(Definer::NewCommand, &cx).unwrap_or_else(|| declared_form::<L>());
    package.insert(L::CallableTypeId::macro_callable(), "newcommand", spec);
    providers.push(Arc::new(package));
    providers.extend(frame.above);

    let delta = ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(providers));
    let state = ParsingState::<L>::lang_initial().expect("initial").derived(&delta).expect("derived");
    Language::new(driver, state).with_descent_guard_init(StdDescentGuardInit::default())
}
```

Parsing `\newcommand{\greet}{Hi \textbf{x}}\greet` with this prototype gave: for
`Latexlike`, and identically for `LatexlikeXp` with definers declared, the callables
`newcommand, greet, textbf, greet` (arguments consumed, `\greet` left alone); for
`LatexlikeXp` with definers honoured, `newcommand, textbf` (`\greet` expanded). The
embedded rule was found in all three trees.

## Appendix B — the public API shape, as checked

Compiled on 1.86 and on stable, feature on and off, with stand-in types:

```rust
// always
impl<L: ParseLang> ConverterBuilder<L> {
    pub fn with_definitions(definitions: DefinitionSet<L>) -> Self { … }
    pub fn definitions(self, definitions: DefinitionSet<L>) -> Self { … }
    pub fn build(self) -> Result<Converter<L>, BuildError> { … }
}

// xp only
impl ConverterBuilder<LatexlikeXp> {
    pub const DEFAULT_EXPANSION_DEPTH_LIMIT: usize = 64;
    pub fn new() -> Self { … }
    pub fn expansion_depth_limit(self, limit: usize) -> Self { … }
}
impl Converter<LatexlikeXp> {
    pub fn builder() -> ConverterBuilder { ConverterBuilder::new() }
    pub fn standard() -> Converter { … }
}

// Feature-off code, valid in both states:
let converter: Converter<Latexlike> =
    ConverterBuilder::with_definitions(defs::standard().with(mine)).build()?;

// Today's code, valid unchanged with the feature on:
fn build_converter() -> Result<Converter, String> { … }          // bare type, default applies
let builder = Converter::builder().expansion_depth_limit(ConverterBuilder::DEFAULT_EXPANSION_DEPTH_LIMIT);
let mine = MacroDef::new("keyword");                               // `L` inferred from the builder it reaches
let converter = Converter::standard();

// The one pattern that needs an annotation (§5.10); without it: E0283, "type annotations needed".
let definitions: DefinitionSet = defs::standard();
let index = definitions.symbols();
```

---

## 11. Status log

The supervisor appends one entry per phase boundary: date, phase, commit, each gate's
result, the test count, deviations, declined findings. The final report (§10) is the last
entry.

### 2026-11-09 — Phase 0 (setup and baseline)

- Branch: the cloud session's designated working branch `claude/focused-cori-m6rbe1`
  (created from `main` at `a7c0b53`; it carries this plan as `95b3df0`). Deviation from
  §2.3's name `xp-feature`: the session harness requires its own branch name; `xp-feature`
  is created at the same final commit so both names resolve to this work.
- Toolchains installed per §3: stable (clippy), 1.86, targets `thumbv7em-none-eabihf` and
  `wasm32-unknown-unknown`; `cargo fetch` in `rust/` and `web/crate/`.
- Gates on the untouched tree: G1 pass, G2 pass, G3 pass, G4 pass, G5 pass, G6 pass
  (both the clippy and the test half).
- **G2 floor: 953 passed, 0 failed** (42 `test result:` lines across unit, integration
  and doc tests).

### 2026-11-09 — Phase 1 (the generic parse side, behaviour-neutral)

- Commit: see the next entry's "previous commit" or `git log` (the commit holding this
  entry is the Phase 1 commit).
- Gates (run by the supervisor): G1 pass, G2 pass (**953 passed, 0 failed**, = floor),
  G3 pass, G4 pass, G5 pass, G6 pass (clippy and 61 tests). No `Cargo.lock` changed.
- Reviewer verdict: no Critical/Major code defect; behaviour-neutral under default
  features (provider stack order, `Declared` registration, scope names off the driver,
  render-side bounds, hand-written `Clone`/`Debug`, docs all verified).
- **Deviations:**
  1. *Type annotations in tests, doctests and one example beyond the §8 Phase 1 exit
     criterion.* §5.4 makes `defs::standard()`, `category()`, `DefinitionSet::new()` and
     `Category::new()` generic, and a default type parameter never drives inference of a
     function's type parameter (§6 trap 3), so every binding whose value never reaches a
     typed place needs `: DefinitionSet` / `: Category`. No expectation changed. Sites:
     `tests/def_build.rs:73`; `tests/def_symbols.rs:27,47,313,378,415`; `#[cfg(test)]` in
     `src/defs/mod.rs` (two tests; one binding split out of a method chain) and
     `src/defs/symbols_extra.rs:221`; doctests in `src/def/mod.rs`, `src/def/symbols.rs`,
     `src/def/entry.rs`, `src/def/set.rs`.
  2. *A second web-side change beyond §5.10:* `web/crate/examples/symbol_index.rs:24,27`
     is the identical `defs::standard()` → `.symbols()` pattern, missed by the plan's
     survey; it received the same one-line annotation plus the `DefinitionSet` import.
     Allowed by the supervisor as the same pattern, not a new kind of change.
  3. `MacroDefinitions` and the xp-conveniences impl blocks live in `convert.rs`, not in
     the xp module (Phase 1 scope item 5 permits this; Phase 3 gates them in place).
  4. `SpecBuildCx::with_source_resolver` is crate-private; only the driver+settings
     constructor is public, as §5.3 asks. Open question for the maintainer: a hand-built
     context cannot carry a resolver.
- **API notes for the release notes:** `TechxtEnvironmentBehavior::of` is now
  `of(NodeRef<'_, L>)` on `impl<L>` instead of a method-level `of<LLL>` (a turbofish
  caller must change; plain calls infer); `ConverterBuilder`'s `Debug` output shows one
  `language_settings` field in place of the three xp fields.
- **Declined / deferred findings:** rustfmt-shape nits (three `arg_specs` signatures in
  `def/entry.rs` now fit one line; `is_refusal`'s closure shape) are fixed in the Phase 2
  commit rather than a Phase 1 fix round; stale prose (`Converter::language()` "Send +
  Sync", `lib.rs` "exactly one language") is Phase 4 scope. `def/rule.rs:36-40`
  `CallableKind::from_callable_type(CallableType)` keeps naming techy's concrete type: it
  is pre-existing public API that nothing in the crate calls, left as is.

### 2026-11-09 — Phase 2 (a second language, and the tests that prove the seam)

- Previous commit (Phase 1): `546eb4d`. The commit holding this entry is the Phase 2 commit.
- Gates (run by the supervisor): G1 pass, G2 pass (**967 passed, 0 failed** = floor 953 +
  14 new tests), G3 pass, G4 pass, G5 pass, G6 pass (61 tests). No `Cargo.lock` changed.
- Added: `impl ParseLang for Latexlike` in `def/lang.rs` (always compiled);
  `tests/lang_generic.rs` (10 tests, generic API + `Latexlike` only);
  `tests/lang_parity.rs` (4 tests: the §5.9 parity contract over 54 documents × the three
  `MathMode`s, a non-triviality check, and the two §5.8 cross-language dispatch tests).
  Also the two rustfmt-shape nits deferred from Phase 1 (`def/entry.rs`, `def/spec.rs`).
- Parity held on every document. The parity test discriminates: against an *honoured*
  xp converter, three documents differ (`preamble-definers`, `definer-def`,
  `definer-unbraced`).
- Reviewer verdict: no Critical/Major defect. One fix round (test files only): the §5.8
  cross-language case (a `Latexlike` tree rendered by a `LatexlikeXp` converter built from
  an empty set, and the reverse) gained tests; a mutation check (pinning the downcast in
  `embedded_rule` to one language) showed each direction fails exactly when it should.
  Two tightenings: the "empty" renderer in `lang_generic.rs` is now a literally empty
  `DefinitionSet::new()`; the parity `(Err, Err)` arm compares the two messages.
- **Deviations:**
  1. *The plan's §8 example `\newcommand\greet{Hi}\greet` is false as written.* The
     declared shape in `DEFINERS` reads the name as a braced group, so the unbraced form
     yields `"Hi\n"` with two missing-argument errors and two unknown-macro warnings, under
     `Latexlike` and under xp-`Declared` identically (pre-existing behaviour; the parity
     corpus pins it as `definer-unbraced` and `definer-def`). `lang_generic.rs` therefore
     uses the braced form `\newcommand{\greet}{Hi}\greet`. **For the maintainer:** plain
     `Latexlike` users now meet this declared shape by default; whether the unbraced
     `\newcommand\x{…}` / `\def\x#1{…}` shapes should consume a bare control-sequence
     name is a behaviour decision outside this plan's non-goals (no behaviour change).
  2. "All three math modes" was read as the three `MathMode` options with the whole corpus
     run under each; the math delimiter forms (`$`, `\(`, `\[`, `$$`) and the math
     environments are covered by separate documents.
  3. The Phase 2 implementer ran `rustfmt --emit stdout` on *copies* of its new files in
     the scratchpad (never `cargo fmt`, never on repository files) and applied the
     differences by hand. Reported here because the §2.1 rule's intent may cover it; later
     agents were instructed not to run rustfmt in any form. A few lines of the parity
     corpus table and of the module-doc coverage table exceed 100 columns (string literals
     and comments, which rustfmt leaves alone).
- Declined findings: none.

### 2026-11-09 — Phase 3 (the feature)

- Previous commit (Phase 2): `5069d9f`. The commit holding this entry is the Phase 3 commit.
- Gates (run by the supervisor): G1–G6 pass with **G2 = 967 passed, 0 failed** (no test left
  the default run); G7 pass, **G8 = 255 passed, 0 failed** (191 unit tests + `lang_generic`
  10, `layout` 9, `layout_proptest` 8, `math_engine` 32, `math_proptest` 5), G9 pass,
  G10 pass, G11 pass (feature-off tree: techy, hashbrown, techy-derive, unicode-width; no
  techy-xp). G12 was also run early on this tree (tooling check, see Phase 4). No
  `Cargo.lock` changed.
- Done: `[features] default = ["xp"]`, `xp = ["dep:techy-xp"]`, techy-xp optional; the 12
  default type parameters use the §5.4 pattern; the xp module, both re-exports, the xp
  conveniences, `MacroDefinitions` and the two §5.8 bodies are gated; 32 integration test
  files carry `#![cfg(feature = "xp")]` (the five named in §8 are ungated); unit-test
  gating on the smallest unit (four whole modules, two single tests, one statement);
  `ci.yml` gained G7–G10 steps in the clippy, test, MSRV and no_std jobs.
- Verified by implementer and reviewer in scratch crates outside the repository: a bare
  `Converter` (and `ConverterBuilder`, `DefinitionSet`) fails with E0107 "missing generics"
  when the feature is off; `Converter<Latexlike>` via `with_definitions(defs::standard())`
  compiles and gives the same text in both states, equal to `Converter::standard()`'s
  with the feature on; `lang_generic.rs` passes in both states (10/10).
- Reviewer verdict: no Critical/Major defect; additivity audit clean (every
  `feature = "xp"` occurrence justified under §5.6; `cfg(not(feature = "xp"))` appears only
  as the 12 undefaulted twins and the two §5.8 fallbacks).
- **Known limitation (reviewer Minor 1, a property of §5.8 as written, not of the
  implementation):** §5.8's fallbacks make one behaviour differ between feature states for a
  consumer that depends on techy-xp *directly* while building techxt with the feature off:
  rendering a `LatexlikeXp` tree it parsed itself, techy-xp refusal diagnostics are not
  re-levelled to warnings (`at_techxt_severity` returns them unchanged) and `is_refusal`
  is `false`. Reproduced: `A \ifx\a\b yes\fi B` parsed with `xp_language` and rendered
  by `Converter<Latexlike>::tree_to_text` gives the same text in both states but two
  "no text rule" warnings with the feature off and none with it on. The fallback cannot
  recognise `RefusalSpec` without linking techy-xp, so §5.2 rule 4 cannot hold for this
  one case. Phase 4 documents it in the `xp` feature section.
- **Deviations:** (1) `DefinitionsChoice::resolve()` is a new private method: with only the
  `Given` arm left in the feature-off build, the former `match` in `build()` trips clippy's
  `infallible_destructuring_match`; the method body is the old match moved unchanged.
  (2) `as_warning` (helper of `at_techxt_severity`) is gated and uses full paths rather
  than a gated import: one `cfg` beyond the literal §5.6 list, dead code otherwise.
  (3) Five doctests were switched from the bare default (`DefinitionSet`) to explicit
  `DefinitionSet<LatexlikeXp>` on the supervisor's over-broad instruction; Phase 4 restores
  the user-facing form (§5.10 / Appendix B), since doctests run under default features
  only (§6 trap 9). (4) No CI step for G11 (`cargo tree`): the plan asks for G7–G10 only.
- Declined findings: none.
