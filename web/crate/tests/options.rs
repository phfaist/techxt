//! The options DTO: what the wire form deserializes into, and what reaches the builder
//! (web/PLAN.md §4.2, §4.8).
//!
//! The documents under test are serde *data*, not JSON text: there is no JSON parser in
//! this crate's dependency list and no reason to add one, since what these tests are
//! about is serde's data model — the field names, the enum spellings, and the
//! `#[serde(default)]` / `deny_unknown_fields` behaviour — which is the same model
//! `serde-wasm-bindgen` drives a JavaScript object through. `serde::de::value`'s
//! `MapDeserializer` presents a `(key, value)` list as exactly that model.

use serde::de::value::{Error, MapDeserializer};
use serde::de::IntoDeserializer;
use serde::Deserialize;

use techxt::convert::{
    FontStyle, FontStyleKind, FootnoteStyle, HeadingStyle, MathMode, MathWrapDelims, MatrixDelims,
    Options, UnknownEnvPolicy, UnknownMacroPolicy, UnknownSpecialsPolicy,
};

use techxt_web::options::{self, OptionsDto};

/// Deserialize an `OptionsDto` from `(field, value)` pairs of one value type.
///
/// One type per call because a serde map is homogeneous in its value type; every field
/// of the DTO is reachable with `&str`, `bool` or `usize`, so a handful of calls covers
/// all fifteen.
fn dto<'de, V>(fields: impl IntoIterator<Item = (&'de str, V)>) -> Result<OptionsDto, Error>
where
    V: IntoDeserializer<'de, Error>,
{
    OptionsDto::deserialize(MapDeserializer::new(
        fields
            .into_iter()
            .map(|(name, value)| (name, Present(value.into_deserializer()))),
    ))
}

/// The same, for fields whose value is JavaScript `null`.
fn dto_null<'de>(fields: impl IntoIterator<Item = &'de str>) -> Result<OptionsDto, Error> {
    OptionsDto::deserialize(MapDeserializer::new(
        fields.into_iter().map(|name| (name, ())),
    ))
}

/// A present value, for an `Option<…>` field.
///
/// `serde::de::value`'s primitive deserializers answer `deserialize_option` by way of
/// `deserialize_any`, which offers the visitor a string where it expects an option;
/// every real format — `serde-wasm-bindgen` included — answers it with `visit_some`
/// instead. This restores that one behaviour, and forwards everything else untouched.
struct Present<D>(D);

impl<'de, D> serde::Deserializer<'de> for Present<D>
where
    D: serde::Deserializer<'de>,
{
    type Error = D::Error;

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_some(self.0)
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.0.deserialize_any(visitor)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}

impl<'de, D> IntoDeserializer<'de, D::Error> for Present<D>
where
    D: serde::Deserializer<'de>,
{
    type Deserializer = Present<D>;

    fn into_deserializer(self) -> Present<D> {
        self
    }
}

/// The empty object is every default, and every default is the library's own.
#[test]
fn an_empty_object_is_the_librarys_defaults() {
    let empty = dto::<&str>([]).expect("an empty object is valid");
    assert_eq!(empty, OptionsDto::default());

    let converter = options::build(&empty).expect("the standard definitions build");
    // A total comparison rather than fourteen assertions: `Options` is `non_exhaustive`
    // and has no `PartialEq`, and a field the library adds later must not quietly get a
    // value from this crate.
    assert_eq!(
        format!("{:?}", converter.options()),
        format!("{:?}", Options::default()),
    );
}

/// `wrapWidth` and `today` are the two fields whose library default is itself a `None`,
/// so absent, explicitly null, and "the library's answer" all coincide.
#[test]
fn absent_and_null_wrap_width_and_today_are_the_librarys_none() {
    let absent = dto::<&str>([]).expect("valid");
    // serde's unit deserializer is what a JavaScript `null` arrives as: `visit_none`
    // for an `Option<…>` field either way.
    let null = dto_null(["wrapWidth", "today"]).expect("null is valid for both");
    assert_eq!(absent, null);
    assert_eq!(null.wrap_width, None);
    assert_eq!(null.today, None);

    let converter = options::build(&null).expect("builds");
    assert_eq!(converter.options().wrap_width, None);
    assert_eq!(converter.options().today, None);
}

/// Every field the app may send reaches the option it names.
#[test]
fn every_mapped_field_reaches_its_option() {
    let strings = dto([
        ("mathMode", "plain"),
        ("mathExpressionIn", "braces"),
        ("matrixDelimiters", "ascii"),
        ("headingStyle", "prefix"),
        ("footnoteStyle", "inline"),
        ("textFont", "off"),
        ("mathFont", "fraktur"),
        ("unknownMacro", "keep-source"),
        ("unknownEnv", "skip"),
        ("unknownSpecials", "skip"),
        ("today", "August 20, 2026"),
        ("recovery", "strict"),
        ("macroDefinitions", "declared"),
    ])
    .expect("valid");
    let converter = options::build(&strings).expect("builds");
    let options = converter.options();
    assert_eq!(options.math_mode, MathMode::Plain);
    assert_eq!(options.math_expression_in, MathWrapDelims::Braces);
    assert_eq!(options.matrix_delimiters, MatrixDelims::Ascii);
    assert_eq!(options.heading_style, HeadingStyle::Prefix);
    assert_eq!(options.footnote_style, FootnoteStyle::Inline);
    assert_eq!(options.text_font, FontStyle::Disabled);
    assert_eq!(options.math_font, FontStyle::Style(FontStyleKind::Fraktur));
    assert_eq!(options.unknown_macro, UnknownMacroPolicy::KeepSource);
    assert_eq!(options.unknown_env, UnknownEnvPolicy::Skip);
    assert_eq!(options.unknown_specials, UnknownSpecialsPolicy::Skip);
    assert_eq!(options.today.as_deref(), Some("August 20, 2026"));
    // `recovery` is a parse-time setting and not an `Options` field, so it is observed
    // through what it does: strict refuses an unknown command outright.
    assert!(converter.latex_to_text(r"\nosuchmacro").is_err());

    let width = dto([("wrapWidth", 40usize)]).expect("valid");
    assert_eq!(
        options::build(&width).expect("builds").options().wrap_width,
        Some(40),
    );

    let comments = dto([("keepComments", true)]).expect("valid");
    assert!(
        options::build(&comments)
            .expect("builds")
            .options()
            .keep_comments
    );

    // `macroDefinitions` is a parse-time setting too, and is observed the same way: with
    // the definers only *declared*, `\greet` is an unknown macro that takes no arguments,
    // so its `{world}` group is all that survives.
    let latex = r"\newcommand{\greet}[1]{Hello #1!}\greet{world}";
    let declared = dto([("macroDefinitions", "declared")]).expect("valid");
    assert_eq!(
        options::build(&declared)
            .expect("builds")
            .latex_to_text(latex)
            .expect("tolerant")
            .text,
        "world\n",
    );
    // The absent field is the library's own answer — `Honored` — rather than one this
    // crate re-types.
    assert_eq!(
        options::build(&dto::<&str>([]).expect("valid"))
            .expect("builds")
            .latex_to_text(latex)
            .expect("tolerant")
            .text,
        "Hello world!\n",
    );
}

/// Every enum spelling `web/src/worker/protocol.ts` lists is accepted, and the one it
/// does not list is not.
#[test]
fn every_enum_string_round_trips() {
    // The field, the values `protocol.ts` declares for it, and a value it does not.
    let cases: &[(&str, &[&str])] = &[
        ("mathMode", &["fancy", "plain", "source"]),
        ("mathExpressionIn", &["parens", "braces", "none"]),
        ("matrixDelimiters", &["unicode", "ascii"]),
        (
            "headingStyle",
            &["numbered-underlined", "underlined", "prefix", "plain"],
        ),
        ("footnoteStyle", &["collected", "inline", "skip"]),
        (
            "unknownMacro",
            &["skip", "render-args", "keep-source", "placeholder"],
        ),
        ("unknownEnv", &["render-body", "skip", "keep-source"]),
        ("unknownSpecials", &["emit-chars", "skip"]),
        ("recovery", &["tolerant", "strict"]),
        ("macroDefinitions", &["honored", "declared"]),
        (
            "textFont",
            &[
                "off",
                "default",
                "bold",
                "italic",
                "bold-italic",
                "script",
                "bold-script",
                "fraktur",
                "bold-fraktur",
                "double-struck",
                "sans-serif",
                "sans-serif-bold",
                "sans-serif-italic",
                "sans-serif-bold-italic",
                "monospace",
                "upright",
            ],
        ),
    ];

    for (field, values) in cases {
        for value in *values {
            let parsed = dto([(*field, *value)]);
            assert!(parsed.is_ok(), "{field} = {value:?}: {:?}", parsed.err());
            // And it reaches a converter rather than only a DTO.
            let dto = parsed.expect("just asserted");
            assert!(options::build(&dto).is_ok(), "{field} = {value:?}");
        }
    }

    // `mathFont` takes the same sixteen values as `textFont`.
    for value in cases.last().expect("textFont is last").1 {
        assert!(dto([("mathFont", *value)]).is_ok(), "mathFont = {value:?}");
    }
}

/// A value that is not one of the listed spellings is a clean `Err`, not a panic — and
/// the message names the field, so the app's own bug is findable.
#[test]
fn an_unknown_enum_string_is_a_clean_error() {
    let error = dto([("mathMode", "shiny")]).expect_err("‘shiny’ is not a math mode");
    let message = error.to_string();
    assert!(message.contains("shiny"), "{message}");
    assert!(message.contains("fancy"), "{message}");

    // The kebab-case spelling is the only one: the Rust variant name is not accepted.
    assert!(dto([("headingStyle", "NumberedUnderlined")]).is_err());
    assert!(dto([("headingStyle", "numbered_underlined")]).is_err());
    // Nor is a `MathWrapDelims::Custom`, which the DTO deliberately cannot say.
    assert!(dto([("mathExpressionIn", "custom")]).is_err());
    // `MacroDefinitions` is `#[non_exhaustive]` in the library; the DTO is closed, and a
    // variant nobody has added to it yet is not sayable over the wire.
    let error = dto([("macroDefinitions", "expanded")]).expect_err("not a spelling");
    assert!(error.to_string().contains("honored"), "{error}");
}

/// An unknown *field* is refused rather than ignored, so a typo in the app is a visible
/// error and not a control that silently does nothing.
#[test]
fn an_unknown_field_is_rejected() {
    let error = dto([("wrapWith", "72")]).expect_err("‘wrapWith’ is not a field");
    assert!(error.to_string().contains("wrapWith"), "{error}");
    // snake_case is the Rust spelling, not the wire spelling.
    assert!(dto([("math_mode", "plain")]).is_err());
    assert!(dto([("macro_definitions", "declared")]).is_err());
    // Neither expansion budget is exposed (§4.6), so neither is a field the app can send.
    assert!(dto([("expansionDepthLimit", 8usize)]).is_err());
    assert!(dto([("expansionCountLimit", 8usize)]).is_err());
}
