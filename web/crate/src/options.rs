//! The options DTO and its mapping onto [`techxt::convert::ConverterBuilder`]
//! (web/PLAN.md §4.2, §5).
//!
//! The app sends a plain JavaScript object; `serde-wasm-bindgen` deserializes it into
//! [`OptionsDto`], and [`build`] turns that into a [`Converter`]. The wire form is
//! normative in `web/src/worker/protocol.ts` — camelCase field names, lowercase
//! kebab-case enum values — so every rename here is a change there too.
//!
//! Two properties this module is written to have, both tested natively in
//! `tests/options.rs`:
//!
//! - **Total.** Every field of [`techxt::convert::Options`] — *and* every parse-time
//!   setting of [`ConverterBuilder`](techxt::convert::ConverterBuilder), which are not
//!   `Options` fields but change the answer just as surely — is either mapped by
//!   [`build`] or carries a `// not exposed:` comment there saying why. A setting the
//!   library gains later has neither, and its reviewer should notice. (M9's
//!   `macro_definitions` and its two expansion budgets are the reason this rule now
//!   says "builder" and not only "options": they arrived on the builder alone.)
//! - **Defaults are the library's.** Every DTO field is an [`Option`], and a `None` one
//!   never reaches a setter: the library's own default stands. The app sends only what
//!   the user changed (PLAN §6.4), so a default that changes in techxt is picked up
//!   rather than frozen into this file.
//!
//! Nothing here needs wasm: an `OptionsDto` is ordinary serde, and `build` is ordinary
//! Rust.

use serde::Deserialize;

use techxt::convert::{
    FontStyle, FontStyleKind, FootnoteStyle, HeadingStyle, MacroDefinitions, MathMode,
    MathWrapDelims, MatrixDelims, Recovery, StdDescentGuardInit, UnknownEnvPolicy,
    UnknownMacroPolicy, UnknownSpecialsPolicy,
};
use techxt::Converter;

/// What the app may ask for, as it arrives over `postMessage`.
///
/// Field names are camelCase on the wire and every field is optional: an absent field
/// (or an explicit `null`) means "whatever the library's default is". An *unknown*
/// field is refused rather than ignored, so a typo in the app is a visible error and
/// not a silently inert control.
///
/// [`wrap_width`](Self::wrap_width) and [`today`](Self::today) are the two fields whose
/// library default is itself a `None`; for them, absent, `null` and "the library's
/// default" all coincide, and there is deliberately no way for the app to say
/// "explicitly no wrapping" any differently from "unset".
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionsDto {
    /// [`techxt::convert::Options::math_mode`].
    #[serde(default)]
    pub math_mode: Option<MathModeDto>,
    /// [`techxt::convert::Options::math_expression_in`].
    #[serde(default)]
    pub math_expression_in: Option<MathWrapDelimsDto>,
    /// [`techxt::convert::Options::matrix_delimiters`].
    #[serde(default)]
    pub matrix_delimiters: Option<MatrixDelimsDto>,
    /// [`techxt::convert::Options::wrap_width`]: a column count, or `None` for no
    /// wrapping. Absent and `null` are the same answer, and it is the library's.
    #[serde(default)]
    pub wrap_width: Option<usize>,
    /// [`techxt::convert::Options::keep_comments`].
    #[serde(default)]
    pub keep_comments: Option<bool>,
    /// [`techxt::convert::Options::heading_style`].
    #[serde(default)]
    pub heading_style: Option<HeadingStyleDto>,
    /// [`techxt::convert::Options::footnote_style`].
    #[serde(default)]
    pub footnote_style: Option<FootnoteStyleDto>,
    /// [`techxt::convert::Options::text_font`] — the Unicode alphabet text starts in,
    /// nothing to do with the display font the page renders with (PLAN §5).
    #[serde(default)]
    pub text_font: Option<FontStyleDto>,
    /// [`techxt::convert::Options::math_font`].
    #[serde(default)]
    pub math_font: Option<FontStyleDto>,
    /// [`techxt::convert::Options::unknown_macro`].
    #[serde(default)]
    pub unknown_macro: Option<UnknownMacroPolicyDto>,
    /// [`techxt::convert::Options::unknown_env`].
    #[serde(default)]
    pub unknown_env: Option<UnknownEnvPolicyDto>,
    /// [`techxt::convert::Options::unknown_specials`].
    #[serde(default)]
    pub unknown_specials: Option<UnknownSpecialsPolicyDto>,
    /// [`techxt::convert::Options::today`]: what `\today` renders as. Absent and `null`
    /// are the same answer, and it is the library's `<today>` placeholder. The browser
    /// has a clock, so the app normally sends a real date here (PLAN §5).
    #[serde(default)]
    pub today: Option<String>,
    /// [`techxt::convert::ConverterBuilder::recovery`] — a parse-time setting rather
    /// than an [`Options`](techxt::convert::Options) field. `strict` is the "strict"
    /// checkbox of PLAN §5.
    #[serde(default)]
    pub recovery: Option<RecoveryDto>,
    /// [`techxt::convert::ConverterBuilder::macro_definitions`] — parse-time too, and
    /// the M9 addition: whether a `\newcommand` in the document defines a macro that
    /// later uses expand, or is merely read and dropped.
    #[serde(default)]
    pub macro_definitions: Option<MacroDefinitionsDto>,
}

/// The descent limit, in engine *descents* (web/PLAN.md §4.6).
///
/// **A depth limit rather than the byte budget §4.6 first specified.** That section
/// reasoned that techy's `StdDescentGuard` estimates stack use by address distance and
/// makes no operating-system call, so a fixed byte budget should work in wasm exactly
/// as it does natively. Calibration in a real browser at W7 showed otherwise, and the
/// measurement is the reason this constant is what it is:
///
/// - Address distance measures the **shadow stack** in linear memory — the region
///   `.cargo/config.toml` sizes. But only address-taken locals live there; an ordinary
///   Rust local becomes a wasm local, held in the engine's own call stack, which
///   linear memory cannot see and `-zstack-size` cannot size.
/// - So the shadow stack barely moves while the engine's stack runs out. Every shape
///   probed died as a `RangeError: Maximum call stack size exceeded` — an engine
///   limit of roughly 1 MB that a wasm module cannot raise — with the 6 MiB byte
///   budget untouched and the guard silent. It is not that the budget was too
///   generous; it was watching the wrong stack.
///
/// Measured in Chromium 140 at W7, the input nesting depth at which a conversion
/// killed the instance, per shape (binary-searched, one fresh module instance per
/// probe because a trap poisons the whole instance and not just the `Session`):
///
/// | shape | dies at |
/// |---|---|
/// | `\sqrt{…}` | 577 |
/// | `\textbf{…}`, `x^{…}` | 585 |
/// | `\frac{…}{2}` | 593 |
/// | `\begin{itemize}\item …` | 670 |
/// | `{…}`, `${…}$` | 993 |
///
/// A depth limit is engine-independent in a way a byte budget is not, which matters
/// because Safari's and Firefox's stack limits are not Chromium's and cannot be probed
/// from here. At 300 descents, the measured refusal depths are:
///
/// | shape | refuses at | descents per level | margin under the trap |
/// |---|---|---|---|
/// | `\sqrt`, `\textbf`, `\frac`, `x^{…}`, `itemize` | 100 | 3 | 5.8× |
/// | `{…}`, `${…}$` | 149–150 | 2 | 6.6× |
///
/// A margin of nearly 6× under the shallowest engine failure, and roughly an order of
/// magnitude beyond anything a real document nests. Verified at W7: a document 20 000
/// levels deep still comes back as this diagnostic rather than a trap, and the same
/// `Session` converts an ordinary document correctly afterwards. §4.6 says to lower
/// the limit rather than raise the stack if the margin looks thin; the stack cannot be
/// raised at all here, so the margin is all there is.
///
/// The 8 MiB shadow stack stays: it costs nothing, and address-taken data below the
/// parse still lives there.
const DESCENT_LIMIT: usize = 300;

/// The descent guard every converter this binding builds is configured with (§4.6).
///
/// It is not an option the app can set: it is a safety limit, and a page that could
/// turn it off would be a page that can lose the user's document to a stack overflow.
/// Leaving the library default (`fixed_stack_budget(250 KiB)`, *marked unconfigured*)
/// would be wrong twice over: it is deliberately tight, and its refusal text is
/// addressed to an embedder who has not chosen. We are that embedder.
fn descent_guard() -> StdDescentGuardInit {
    StdDescentGuardInit::depth_limit(DESCENT_LIMIT)
}

/// Build the converter these options ask for.
///
/// The `Err` case is a failure of techxt's own definitions — a library bug rather than
/// anything the app can have said — and is reported rather than panicked so that the
/// wasm instance survives it.
///
/// Every `if let Some(…)` below is the "defaults are the library's" rule in force: an
/// absent field calls no setter at all, rather than calling one with a value re-typed
/// from the library's documentation.
pub fn build(dto: &OptionsDto) -> Result<Converter, String> {
    // A safety limit, not a preference — hence set here rather than exposed (§4.6).
    let mut builder = Converter::builder().descent_guard(descent_guard());

    // --- the fourteen `Options` fields, in the order the library declares them ---
    if let Some(value) = dto.math_mode {
        builder = builder.math_mode(value.into());
    }
    if let Some(value) = dto.math_expression_in {
        builder = builder.math_expression_in(value.into());
    }
    if let Some(value) = dto.matrix_delimiters {
        builder = builder.matrix_delimiters(value.into());
    }
    if let Some(value) = dto.wrap_width {
        builder = builder.wrap_width(Some(value));
    }
    if let Some(value) = dto.keep_comments {
        builder = builder.keep_comments(value);
    }
    if let Some(value) = dto.heading_style {
        builder = builder.heading_style(value.into());
    }
    if let Some(value) = dto.footnote_style {
        builder = builder.footnote_style(value.into());
    }
    // not exposed: `list_style` — two arrays of strings (bullet characters and counter
    // formats) are a user interface in themselves, and rarely the thing anyone came to
    // this page to change (PLAN §5).
    if let Some(value) = dto.text_font {
        builder = builder.text_font(value.into());
    }
    if let Some(value) = dto.math_font {
        builder = builder.math_font(value.into());
    }
    if let Some(value) = dto.unknown_macro {
        builder = builder.unknown_macro(value.into());
    }
    if let Some(value) = dto.unknown_env {
        builder = builder.unknown_env(value.into());
    }
    if let Some(value) = dto.unknown_specials {
        builder = builder.unknown_specials(value.into());
    }
    if let Some(value) = &dto.today {
        builder = builder.today(Some(value.clone().into_boxed_str()));
    }

    // --- the parse-time builder settings, which are not `Options` fields ---
    if let Some(value) = dto.recovery {
        builder = builder.recovery(value.into());
    }
    if let Some(value) = dto.macro_definitions {
        builder = builder.macro_definitions(value.into());
    }
    // not exposed: `expansion_depth_limit`, `expansion_count_limit` — the same reason
    // `descent_guard` is set here rather than offered (§4.6): they are safety limits and
    // not preferences, and a page that could raise one would be a page a one-line
    // document can hang. The library's own defaults (64 nested expansions, 2 000
    // expansions per parse) are already the conservative ones — techxt lowers techy-xp's
    // count budget fiftyfold precisely because it converts documents it did not write,
    // which is this page's situation exactly — so there is nothing to re-type here
    // either (PLAN §4.6, §5).
    // not exposed: `unknown_macro_resolution` — its interaction with `recovery` is
    // subtle (it decouples "unknown command" from "strict"), and the strict checkbox
    // already covers the observable case (PLAN §5).
    // not exposed: `source_resolver` — there is no filesystem in a browser, and the
    // `techxt.input-not-resolved` note explains that to the reader far better than a
    // control that cannot work would (PLAN §1 non-goals).
    // not exposed: `definitions`, `override_macro`, `override_environment`,
    // `override_specials` — an extension API, not an option; a definitions playground
    // is a crate-docs subject (PLAN §16).

    builder
        .build()
        .map_err(|error| format!("the techxt definitions failed to build: {error}"))
}

/// `techxt::convert::MathMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MathModeDto {
    /// Unicode symbols, scripts and alphabets, spaced like a typesetter would.
    Fancy,
    /// The same symbols, concatenated without the typesetter's spacing.
    Plain,
    /// The formula's LaTeX source, left as it was written.
    Source,
}

impl From<MathModeDto> for MathMode {
    fn from(dto: MathModeDto) -> MathMode {
        match dto {
            MathModeDto::Fancy => MathMode::Fancy,
            MathModeDto::Plain => MathMode::Plain,
            MathModeDto::Source => MathMode::Source,
        }
    }
}

/// `techxt::convert::MathWrapDelims`, minus its `Custom` variant.
///
/// `Custom(open, close)` is a pair of arbitrary strings and would need two more
/// controls to spell; the app does not offer it, so the DTO cannot say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MathWrapDelimsDto {
    /// `(a+b)/2`.
    Parens,
    /// `{a+b}/2`.
    Braces,
    /// `a+b/2` — nothing at all, even where the extent is then unclear.
    None,
}

impl From<MathWrapDelimsDto> for MathWrapDelims {
    fn from(dto: MathWrapDelimsDto) -> MathWrapDelims {
        match dto {
            MathWrapDelimsDto::Parens => MathWrapDelims::Parens,
            MathWrapDelimsDto::Braces => MathWrapDelims::Braces,
            MathWrapDelimsDto::None => MathWrapDelims::None,
        }
    }
}

/// `techxt::convert::MatrixDelims`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatrixDelimsDto {
    /// Multi-row box-drawing brackets, assembled from the unicode extension pieces.
    Unicode,
    /// One-character ASCII brackets on every row.
    Ascii,
}

impl From<MatrixDelimsDto> for MatrixDelims {
    fn from(dto: MatrixDelimsDto) -> MatrixDelims {
        match dto {
            MatrixDelimsDto::Unicode => MatrixDelims::Unicode,
            MatrixDelimsDto::Ascii => MatrixDelims::Ascii,
        }
    }
}

/// `techxt::convert::HeadingStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeadingStyleDto {
    /// A section number, the title, and a rule of underline characters.
    NumberedUnderlined,
    /// The title and its underline, without a number.
    Underlined,
    /// The number and the title on one line, with no underline.
    Prefix,
    /// The title alone.
    Plain,
}

impl From<HeadingStyleDto> for HeadingStyle {
    fn from(dto: HeadingStyleDto) -> HeadingStyle {
        match dto {
            HeadingStyleDto::NumberedUnderlined => HeadingStyle::NumberedUnderlined,
            HeadingStyleDto::Underlined => HeadingStyle::Underlined,
            HeadingStyleDto::Prefix => HeadingStyle::Prefix,
            HeadingStyleDto::Plain => HeadingStyle::Plain,
        }
    }
}

/// `techxt::convert::FootnoteStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FootnoteStyleDto {
    /// A marker in the text, and the notes gathered into a block at the end.
    Collected,
    /// The note's text where the footnote was.
    Inline,
    /// Neither marker nor text.
    Skip,
}

impl From<FootnoteStyleDto> for FootnoteStyle {
    fn from(dto: FootnoteStyleDto) -> FootnoteStyle {
        match dto {
            FootnoteStyleDto::Collected => FootnoteStyle::Collected,
            FootnoteStyleDto::Inline => FootnoteStyle::Inline,
            FootnoteStyleDto::Skip => FootnoteStyle::Skip,
        }
    }
}

/// `techxt::convert::UnknownMacroPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownMacroPolicyDto {
    /// Render nothing.
    Skip,
    /// Render the macro's arguments as if the macro were transparent.
    RenderArgs,
    /// Emit the macro's LaTeX source.
    KeepSource,
    /// Emit `<name>`.
    Placeholder,
}

impl From<UnknownMacroPolicyDto> for UnknownMacroPolicy {
    fn from(dto: UnknownMacroPolicyDto) -> UnknownMacroPolicy {
        match dto {
            UnknownMacroPolicyDto::Skip => UnknownMacroPolicy::Skip,
            UnknownMacroPolicyDto::RenderArgs => UnknownMacroPolicy::RenderArgs,
            UnknownMacroPolicyDto::KeepSource => UnknownMacroPolicy::KeepSource,
            UnknownMacroPolicyDto::Placeholder => UnknownMacroPolicy::Placeholder,
        }
    }
}

/// `techxt::convert::UnknownEnvPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownEnvPolicyDto {
    /// Render the body and drop the wrapper.
    RenderBody,
    /// Render nothing at all, body included.
    Skip,
    /// Emit the environment's LaTeX source as a preformatted block.
    KeepSource,
}

impl From<UnknownEnvPolicyDto> for UnknownEnvPolicy {
    fn from(dto: UnknownEnvPolicyDto) -> UnknownEnvPolicy {
        match dto {
            UnknownEnvPolicyDto::RenderBody => UnknownEnvPolicy::RenderBody,
            UnknownEnvPolicyDto::Skip => UnknownEnvPolicy::Skip,
            UnknownEnvPolicyDto::KeepSource => UnknownEnvPolicy::KeepSource,
        }
    }
}

/// `techxt::convert::UnknownSpecialsPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownSpecialsPolicyDto {
    /// Emit the trigger characters themselves.
    EmitChars,
    /// Render nothing at all.
    Skip,
}

impl From<UnknownSpecialsPolicyDto> for UnknownSpecialsPolicy {
    fn from(dto: UnknownSpecialsPolicyDto) -> UnknownSpecialsPolicy {
        match dto {
            UnknownSpecialsPolicyDto::EmitChars => UnknownSpecialsPolicy::EmitChars,
            UnknownSpecialsPolicyDto::Skip => UnknownSpecialsPolicy::Skip,
        }
    }
}

/// `techxt::convert::Recovery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryDto {
    /// Convert a malformed document as best as possible and report what was wrong.
    Tolerant,
    /// Refuse it instead: the conversion comes back with `ok: false` and one error.
    Strict,
}

impl From<RecoveryDto> for Recovery {
    fn from(dto: RecoveryDto) -> Recovery {
        match dto {
            RecoveryDto::Tolerant => Recovery::Tolerant,
            RecoveryDto::Strict => Recovery::Strict,
        }
    }
}

/// `techxt::convert::MacroDefinitions`.
///
/// The library's enum is `#[non_exhaustive]`, so it may gain a variant; this one is the
/// app's own closed copy of the two spellings `protocol.ts` lists. A third variant is
/// then something a person adds here and to the select in `controls.ts` deliberately,
/// rather than a value the wire form silently already accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacroDefinitionsDto {
    /// The library default: `\newcommand`, `\def` and their relatives define macros,
    /// and a later use of one expands.
    Honored,
    /// The definers are read and dropped — their arguments consumed so a body cannot
    /// leak into the text — and a later use is an unknown macro like any other.
    Declared,
}

impl From<MacroDefinitionsDto> for MacroDefinitions {
    fn from(dto: MacroDefinitionsDto) -> MacroDefinitions {
        match dto {
            MacroDefinitionsDto::Honored => MacroDefinitions::Honored,
            MacroDefinitionsDto::Declared => MacroDefinitions::Declared,
        }
    }
}

/// `techxt::convert::FontStyle`: the Unicode *character* alphabet a letter is mapped
/// into.
///
/// [`Off`](Self::Off) is `FontStyle::Disabled` — style macros stop working altogether,
/// so `\textbf{a}` renders `a` rather than `𝐚` — and [`Default`](Self::Default) is
/// `FontStyle::Default`. Everything else is `FontStyle::Style(FontStyleKind::…)`, one
/// per alphabet, and none of it has anything to do with the display font the page
/// renders the output in (PLAN §5, §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontStyleDto {
    /// No alphabet mapping at all, and style macros cannot turn it back on.
    Off,
    /// Take the style from the surrounding defaults.
    Default,
    /// `𝐛𝐨𝐥𝐝`.
    Bold,
    /// `𝑖𝑡𝑎𝑙𝑖𝑐` — the default for math.
    Italic,
    /// `𝒃𝒐𝒍𝒅 𝒊𝒕𝒂𝒍𝒊𝒄`.
    BoldItalic,
    /// `𝓈𝒸𝓇𝒾𝓅𝓉`.
    Script,
    /// `𝓫𝓸𝓵𝓭 𝓼𝓬𝓻𝓲𝓹𝓽`.
    BoldScript,
    /// `𝔣𝔯𝔞𝔨𝔱𝔲𝔯`.
    Fraktur,
    /// `𝖇𝖔𝖑𝖉 𝖋𝖗𝖆𝖐𝖙𝖚𝖗`.
    BoldFraktur,
    /// `𝕕𝕠𝕦𝕓𝕝𝕖-𝕤𝕥𝕣𝕦𝕔𝕜`.
    DoubleStruck,
    /// `𝗌𝖺𝗇𝗌-𝗌𝖾𝗋𝗂𝖿`.
    SansSerif,
    /// `𝘀𝗮𝗻𝘀 𝗯𝗼𝗹𝗱`.
    SansSerifBold,
    /// `𝘴𝘢𝘯𝘴 𝘪𝘵𝘢𝘭𝘪𝘤`.
    SansSerifItalic,
    /// `𝙨𝙖𝙣𝙨 𝙗𝙤𝙡𝙙 𝙞𝙩𝙖𝙡𝙞𝙘`.
    SansSerifBoldItalic,
    /// `𝚖𝚘𝚗𝚘𝚜𝚙𝚊𝚌𝚎`.
    Monospace,
    /// Upright: letters map to themselves, and a nested style macro still overrides it.
    Upright,
}

impl From<FontStyleDto> for FontStyle {
    fn from(dto: FontStyleDto) -> FontStyle {
        match dto {
            FontStyleDto::Off => FontStyle::Disabled,
            FontStyleDto::Default => FontStyle::Default,
            FontStyleDto::Bold => FontStyle::Style(FontStyleKind::Bold),
            FontStyleDto::Italic => FontStyle::Style(FontStyleKind::Italic),
            FontStyleDto::BoldItalic => FontStyle::Style(FontStyleKind::BoldItalic),
            FontStyleDto::Script => FontStyle::Style(FontStyleKind::Script),
            FontStyleDto::BoldScript => FontStyle::Style(FontStyleKind::BoldScript),
            FontStyleDto::Fraktur => FontStyle::Style(FontStyleKind::Fraktur),
            FontStyleDto::BoldFraktur => FontStyle::Style(FontStyleKind::BoldFraktur),
            FontStyleDto::DoubleStruck => FontStyle::Style(FontStyleKind::DoubleStruck),
            FontStyleDto::SansSerif => FontStyle::Style(FontStyleKind::SansSerif),
            FontStyleDto::SansSerifBold => FontStyle::Style(FontStyleKind::SansSerifBold),
            FontStyleDto::SansSerifItalic => FontStyle::Style(FontStyleKind::SansSerifItalic),
            FontStyleDto::SansSerifBoldItalic => {
                FontStyle::Style(FontStyleKind::SansSerifBoldItalic)
            }
            FontStyleDto::Monospace => FontStyle::Style(FontStyleKind::Monospace),
            FontStyleDto::Upright => FontStyle::Style(FontStyleKind::Upright),
        }
    }
}
