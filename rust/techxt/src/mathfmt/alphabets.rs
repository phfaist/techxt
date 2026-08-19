//! The unicode mathematical alphanumeric symbols: `\mathbf{x}` → `𝐱`.
//!
//! Unicode gives the latin alphabet a dozen mathematical variants — bold, italic,
//! script, fraktur, double-struck, sans-serif, monospace — in the U+1D400 block, laid
//! out as 26 uppercase letters followed by 26 lowercase ones. Styling a letter is
//! therefore an addition, with a table of exceptions for the letters that unicode had
//! already encoded elsewhere before the block existed (`ℎ`, `ℬ`, `ℜ`, `ℝ`, …), which
//! left holes in it.
//!
//! **Only ASCII letters are mapped.** There is no digit alphabet here: digits, greek
//! letters and punctuation pass through unchanged, so `\mathbf{123}` gives `123`.
//! Unicode does have bold and double-struck digits, but adding them would be a new
//! feature rather than a port, and PLAN.md §9.5 says letters only.
//!
//! **Where the mapping applies (DECISIONS.md D6 — easy to get wrong).** Styling
//! applies to *document character leaves only*: the characters the author typed. It
//! must never be applied to a string a rule produced — `\sin` → `"sin"`,
//! `\alpha` → `"α"`, a template's literal text. That is what makes `$\sin x$` come out
//! as `sin 𝑥`, with an upright function name next to an italic variable; running rule
//! output through the alphabet would italicize every operator name in the document.

use alloc::string::String;

/// Whether an alphabet applies to a stretch of text, and which one
/// (PLAN.md §9.5).
///
/// The three states are not "bold / italic / none": they are the three answers to
/// *where does the style come from*.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontStyle {
    /// No alphabet mapping at all, and style macros cannot turn it back on. This is
    /// the option-level off switch (`Options::text_font` / `Options::math_font` set to
    /// `Disabled`); no macro ever installs it.
    Disabled,
    /// Take the style from the surrounding defaults: upright in text, and in math the
    /// value of `Options::math_font` (italic by default). This is the state a stretch
    /// of text is in when no style macro has spoken.
    #[default]
    Default,
    /// An explicit alphabet, installed by a style macro (`\mathbf`, `\textit`, …) or by
    /// the options.
    Style(FontStyleKind),
}

impl FontStyle {
    /// Whether an explicit style is in effect, i.e. whether this is
    /// [`Style`](FontStyle::Style).
    ///
    /// This is how the caller derives `segment_plain`'s `upright_letters_are_op`
    /// (DECISIONS.md D4, v3's `bool(math_fontstyle)`): a run of upright latin letters
    /// may be read as a function name exactly when variables are being styled, and
    /// therefore stand out from it. Note that
    /// [`Style(Upright)`](FontStyleKind::Upright) counts as a style in effect even
    /// though it maps letters to themselves — D7 says so explicitly.
    pub fn is_style(self) -> bool {
        matches!(self, FontStyle::Style(_))
    }

    /// The alphabet to apply, given what the enclosing context resolves
    /// [`Default`](FontStyle::Default) to.
    ///
    /// `Disabled` maps to `None` whatever the fallback is — that is what makes it the
    /// off switch.
    pub fn resolve(self, fallback: FontStyle) -> Option<FontStyleKind> {
        match self {
            FontStyle::Disabled => None,
            FontStyle::Style(kind) => Some(kind),
            FontStyle::Default => match fallback {
                FontStyle::Style(kind) => Some(kind),
                _ => None,
            },
        }
    }
}

/// One of the unicode mathematical alphabets (PLAN.md §9.5, DECISIONS.md D7).
///
/// The thirteen alphabets unicode provides, plus [`Upright`](Self::Upright), which is
/// the *absence* of an alphabet expressed as one.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyleKind {
    /// `𝐛𝐨𝐥𝐝` — `\mathbf`, `\textbf`.
    Bold,
    /// `𝑖𝑡𝑎𝑙𝑖𝑐` — `\mathit`, `\textit`, and the default math font.
    Italic,
    /// `𝒃𝒐𝒍𝒅 𝒊𝒕𝒂𝒍𝒊𝒄` — `\emph` inside `\textbf`.
    BoldItalic,
    /// `𝓈𝒸𝓇𝒾𝓅𝓉` — `\mathcal`, `\mathscr`.
    Script,
    /// `𝓫𝓸𝓵𝓭 𝓼𝓬𝓻𝓲𝓹𝓽`.
    BoldScript,
    /// `𝔣𝔯𝔞𝔨𝔱𝔲𝔯` — `\mathfrak`.
    Fraktur,
    /// `𝕕𝕠𝕦𝕓𝕝𝕖-𝕤𝕥𝕣𝕦𝕔𝕜` — `\mathbb`.
    DoubleStruck,
    /// `𝖇𝖔𝖑𝖉 𝖋𝖗𝖆𝖐𝖙𝖚𝖗`.
    BoldFraktur,
    /// `𝗌𝖺𝗇𝗌-𝗌𝖾𝗋𝗂𝖿` — `\mathsf`, `\textsf`.
    SansSerif,
    /// `𝘀𝗮𝗻𝘀 𝗯𝗼𝗹𝗱`.
    SansSerifBold,
    /// `𝘴𝘢𝘯𝘴 𝘪𝘵𝘢𝘭𝘪𝘤` — `\emph` inside `\textsf`.
    SansSerifItalic,
    /// `𝙨𝙖𝙣𝙨 𝙗𝙤𝙡𝙙 𝙞𝙩𝙖𝙡𝙞𝙘`.
    SansSerifBoldItalic,
    /// `𝚖𝚘𝚗𝚘𝚜𝚙𝚊𝚌𝚎` — `\mathtt`, `\texttt`.
    Monospace,
    /// Upright: letters map to themselves (DECISIONS.md D7).
    ///
    /// This is what `\mathrm`, `\textrm`, `\textup`, `\textmd` and `\operatorname`
    /// install. It is a *style*, not the absence of one, which is exactly the
    /// difference that matters: a nested style macro overrides it normally, so
    /// `\mathrm{a\mathbf{b}}` still renders the `b` in bold, while
    /// [`Disabled`](FontStyle::Disabled) would swallow the `\mathbf` too.
    Upright,
}

/// The thirteen unicode alphabet blocks, as `(base of A, base of a)`
/// (v3 `_fmt_math_style_offsets`).
///
/// The order is unicode code-point order, which is also v3's dict order.
const STYLE_OFFSETS: [(FontStyleKind, u32, u32); 13] = [
    (FontStyleKind::Bold, 0x1D400, 0x1D41A),
    (FontStyleKind::Italic, 0x1D434, 0x1D44E),
    (FontStyleKind::BoldItalic, 0x1D468, 0x1D482),
    (FontStyleKind::Script, 0x1D49C, 0x1D4B6),
    (FontStyleKind::BoldScript, 0x1D4D0, 0x1D4EA),
    (FontStyleKind::Fraktur, 0x1D504, 0x1D51E),
    (FontStyleKind::DoubleStruck, 0x1D538, 0x1D552),
    (FontStyleKind::BoldFraktur, 0x1D56C, 0x1D586),
    (FontStyleKind::SansSerif, 0x1D5A0, 0x1D5BA),
    (FontStyleKind::SansSerifBold, 0x1D5D4, 0x1D5EE),
    (FontStyleKind::SansSerifItalic, 0x1D608, 0x1D622),
    (FontStyleKind::SansSerifBoldItalic, 0x1D63C, 0x1D656),
    (FontStyleKind::Monospace, 0x1D670, 0x1D68A),
];

/// The letters unicode had already encoded before the U+1D400 block existed, which
/// therefore sit outside their alphabet's range and leave a hole in it
/// (v3 `_fmt_math_style_exceptions`, 24 entries over 4 styles).
///
/// Every other style — bold, bold-italic, bold-script, bold-fraktur, the four
/// sans-serif ones and monospace — has a complete, contiguous block and no exceptions.
const STYLE_EXCEPTIONS: &[(FontStyleKind, char, char)] = &[
    // italic: PLANCK CONSTANT
    (FontStyleKind::Italic, 'h', '\u{210E}'),
    // script: the "letterlike symbols" block
    (FontStyleKind::Script, 'B', '\u{212C}'),
    (FontStyleKind::Script, 'E', '\u{2130}'),
    (FontStyleKind::Script, 'F', '\u{2131}'),
    (FontStyleKind::Script, 'H', '\u{210B}'),
    (FontStyleKind::Script, 'I', '\u{2110}'),
    (FontStyleKind::Script, 'L', '\u{2112}'),
    (FontStyleKind::Script, 'M', '\u{2133}'),
    (FontStyleKind::Script, 'R', '\u{211B}'),
    (FontStyleKind::Script, 'e', '\u{212F}'),
    (FontStyleKind::Script, 'g', '\u{210A}'),
    (FontStyleKind::Script, 'o', '\u{2134}'),
    // fraktur: the black-letter capitals
    (FontStyleKind::Fraktur, 'C', '\u{212D}'),
    (FontStyleKind::Fraktur, 'H', '\u{210C}'),
    (FontStyleKind::Fraktur, 'I', '\u{2111}'),
    (FontStyleKind::Fraktur, 'R', '\u{211C}'),
    (FontStyleKind::Fraktur, 'Z', '\u{2128}'),
    // double-struck: the number sets
    (FontStyleKind::DoubleStruck, 'C', '\u{2102}'),
    (FontStyleKind::DoubleStruck, 'H', '\u{210D}'),
    (FontStyleKind::DoubleStruck, 'N', '\u{2115}'),
    (FontStyleKind::DoubleStruck, 'P', '\u{2119}'),
    (FontStyleKind::DoubleStruck, 'Q', '\u{211A}'),
    (FontStyleKind::DoubleStruck, 'R', '\u{211D}'),
    (FontStyleKind::DoubleStruck, 'Z', '\u{2124}'),
];

/// The `(A, a)` code-point bases of a style's block, or `None` for
/// [`Upright`](FontStyleKind::Upright), which has no block of its own.
fn offsets(kind: FontStyleKind) -> Option<(u32, u32)> {
    STYLE_OFFSETS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, up, lo)| (*up, *lo))
}

/// Style one character (v3 `_fmt_math_style_char`).
///
/// ASCII letters are mapped; every other character — digits, greek, punctuation,
/// already-styled letters — is returned unchanged, because there is no alphabet to
/// map it into.
///
/// ```
/// use techxt::mathfmt::{fmt_style_char, FontStyleKind};
///
/// assert_eq!(fmt_style_char('x', FontStyleKind::Bold), '𝐱');
/// assert_eq!(fmt_style_char('h', FontStyleKind::Italic), 'ℎ');  // the exception
/// assert_eq!(fmt_style_char('7', FontStyleKind::Bold), '7');    // no digit alphabet
/// assert_eq!(fmt_style_char('x', FontStyleKind::Upright), 'x');
/// ```
pub fn fmt_style_char(c: char, kind: FontStyleKind) -> char {
    for (k, ascii, replacement) in STYLE_EXCEPTIONS {
        if *k == kind && *ascii == c {
            return *replacement;
        }
    }
    let Some((offset_up, offset_lo)) = offsets(kind) else {
        return c; // Upright, and any future kind without a block
    };
    let base = if c.is_ascii_uppercase() {
        offset_up + (c as u32 - 'A' as u32)
    } else if c.is_ascii_lowercase() {
        offset_lo + (c as u32 - 'a' as u32)
    } else {
        return c;
    };
    // Every letter of every block is an assigned code point, so this cannot fail; a
    // future table typo would fall back on the unstyled character rather than panic.
    char::from_u32(base).unwrap_or(c)
}

/// Style a string, character by character (v3 `fmt_math_text_style`).
///
/// Remember DECISIONS.md D6: call this on document text only, never on a rule's
/// replacement string.
///
/// ```
/// use techxt::mathfmt::{fmt_style, FontStyleKind};
///
/// assert_eq!(fmt_style("bold", FontStyleKind::Bold), "𝐛𝐨𝐥𝐝");
/// assert_eq!(fmt_style("R and α", FontStyleKind::DoubleStruck), "ℝ 𝕒𝕟𝕕 α");
/// ```
pub fn fmt_style(text: &str, kind: FontStyleKind) -> String {
    text.chars().map(|c| fmt_style_char(c, kind)).collect()
}

/// Map a styled letter back to the plain ASCII letter it was made from
/// (v3 `_fmt_math_style_plain_chars`).
///
/// This is the inverse of [`fmt_style_char`], and it is what lets a script survive a
/// font macro: unicode has no styled superscripts, so `$x^{\mathbf{a}}$` has to look up
/// `a` rather than `𝐚`. The style is lost, but the script can still be typeset —
/// `xᵃ` beats `x^(𝐚)`.
///
/// It is derived from the very same tables as the forward mapping, so the two cannot
/// drift apart. Characters that are not styled letters come back unchanged.
///
/// ```
/// use techxt::mathfmt::unstyle_char;
///
/// assert_eq!(unstyle_char('𝐚'), 'a');
/// assert_eq!(unstyle_char('ℝ'), 'R');
/// assert_eq!(unstyle_char('α'), 'α');
/// ```
pub fn unstyle_char(c: char) -> char {
    let cp = c as u32;
    for (_, offset_up, offset_lo) in STYLE_OFFSETS {
        if (offset_up..offset_up + 26).contains(&cp) {
            // 26 letters from 'A', so the result is always a valid ASCII letter
            return (b'A' + (cp - offset_up) as u8) as char;
        }
        if (offset_lo..offset_lo + 26).contains(&cp) {
            return (b'a' + (cp - offset_lo) as u8) as char;
        }
    }
    // The exceptions live outside every block (U+2102..U+2134), so this scan can never
    // disagree with the one above.
    for (_, ascii, replacement) in STYLE_EXCEPTIONS {
        if *replacement == c {
            return *ascii;
        }
    }
    c
}

/// Map every styled letter of a string back to plain ASCII; see [`unstyle_char`].
pub fn unstyle(text: &str) -> String {
    text.chars().map(unstyle_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::ToString, vec::Vec};

    const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";

    /// The golden alphabet strings of the porting notes §7.4, one per style.
    const GOLDEN: &[(FontStyleKind, &str, &str)] = &[
        (
            FontStyleKind::Bold,
            "𝐀𝐁𝐂𝐃𝐄𝐅𝐆𝐇𝐈𝐉𝐊𝐋𝐌𝐍𝐎𝐏𝐐𝐑𝐒𝐓𝐔𝐕𝐖𝐗𝐘𝐙",
            "𝐚𝐛𝐜𝐝𝐞𝐟𝐠𝐡𝐢𝐣𝐤𝐥𝐦𝐧𝐨𝐩𝐪𝐫𝐬𝐭𝐮𝐯𝐰𝐱𝐲𝐳",
        ),
        (
            FontStyleKind::Italic,
            "𝐴𝐵𝐶𝐷𝐸𝐹𝐺𝐻𝐼𝐽𝐾𝐿𝑀𝑁𝑂𝑃𝑄𝑅𝑆𝑇𝑈𝑉𝑊𝑋𝑌𝑍",
            "𝑎𝑏𝑐𝑑𝑒𝑓𝑔ℎ𝑖𝑗𝑘𝑙𝑚𝑛𝑜𝑝𝑞𝑟𝑠𝑡𝑢𝑣𝑤𝑥𝑦𝑧",
        ),
        (
            FontStyleKind::BoldItalic,
            "𝑨𝑩𝑪𝑫𝑬𝑭𝑮𝑯𝑰𝑱𝑲𝑳𝑴𝑵𝑶𝑷𝑸𝑹𝑺𝑻𝑼𝑽𝑾𝑿𝒀𝒁",
            "𝒂𝒃𝒄𝒅𝒆𝒇𝒈𝒉𝒊𝒋𝒌𝒍𝒎𝒏𝒐𝒑𝒒𝒓𝒔𝒕𝒖𝒗𝒘𝒙𝒚𝒛",
        ),
        (
            FontStyleKind::Script,
            "𝒜ℬ𝒞𝒟ℰℱ𝒢ℋℐ𝒥𝒦ℒℳ𝒩𝒪𝒫𝒬ℛ𝒮𝒯𝒰𝒱𝒲𝒳𝒴𝒵",
            "𝒶𝒷𝒸𝒹ℯ𝒻ℊ𝒽𝒾𝒿𝓀𝓁𝓂𝓃ℴ𝓅𝓆𝓇𝓈𝓉𝓊𝓋𝓌𝓍𝓎𝓏",
        ),
        (
            FontStyleKind::BoldScript,
            "𝓐𝓑𝓒𝓓𝓔𝓕𝓖𝓗𝓘𝓙𝓚𝓛𝓜𝓝𝓞𝓟𝓠𝓡𝓢𝓣𝓤𝓥𝓦𝓧𝓨𝓩",
            "𝓪𝓫𝓬𝓭𝓮𝓯𝓰𝓱𝓲𝓳𝓴𝓵𝓶𝓷𝓸𝓹𝓺𝓻𝓼𝓽𝓾𝓿𝔀𝔁𝔂𝔃",
        ),
        (
            FontStyleKind::Fraktur,
            "𝔄𝔅ℭ𝔇𝔈𝔉𝔊ℌℑ𝔍𝔎𝔏𝔐𝔑𝔒𝔓𝔔ℜ𝔖𝔗𝔘𝔙𝔚𝔛𝔜ℨ",
            "𝔞𝔟𝔠𝔡𝔢𝔣𝔤𝔥𝔦𝔧𝔨𝔩𝔪𝔫𝔬𝔭𝔮𝔯𝔰𝔱𝔲𝔳𝔴𝔵𝔶𝔷",
        ),
        (
            FontStyleKind::DoubleStruck,
            "𝔸𝔹ℂ𝔻𝔼𝔽𝔾ℍ𝕀𝕁𝕂𝕃𝕄ℕ𝕆ℙℚℝ𝕊𝕋𝕌𝕍𝕎𝕏𝕐ℤ",
            "𝕒𝕓𝕔𝕕𝕖𝕗𝕘𝕙𝕚𝕛𝕜𝕝𝕞𝕟𝕠𝕡𝕢𝕣𝕤𝕥𝕦𝕧𝕨𝕩𝕪𝕫",
        ),
        (
            FontStyleKind::BoldFraktur,
            "𝕬𝕭𝕮𝕯𝕰𝕱𝕲𝕳𝕴𝕵𝕶𝕷𝕸𝕹𝕺𝕻𝕼𝕽𝕾𝕿𝖀𝖁𝖂𝖃𝖄𝖅",
            "𝖆𝖇𝖈𝖉𝖊𝖋𝖌𝖍𝖎𝖏𝖐𝖑𝖒𝖓𝖔𝖕𝖖𝖗𝖘𝖙𝖚𝖛𝖜𝖝𝖞𝖟",
        ),
        (
            FontStyleKind::SansSerif,
            "𝖠𝖡𝖢𝖣𝖤𝖥𝖦𝖧𝖨𝖩𝖪𝖫𝖬𝖭𝖮𝖯𝖰𝖱𝖲𝖳𝖴𝖵𝖶𝖷𝖸𝖹",
            "𝖺𝖻𝖼𝖽𝖾𝖿𝗀𝗁𝗂𝗃𝗄𝗅𝗆𝗇𝗈𝗉𝗊𝗋𝗌𝗍𝗎𝗏𝗐𝗑𝗒𝗓",
        ),
        (
            FontStyleKind::SansSerifBold,
            "𝗔𝗕𝗖𝗗𝗘𝗙𝗚𝗛𝗜𝗝𝗞𝗟𝗠𝗡𝗢𝗣𝗤𝗥𝗦𝗧𝗨𝗩𝗪𝗫𝗬𝗭",
            "𝗮𝗯𝗰𝗱𝗲𝗳𝗴𝗵𝗶𝗷𝗸𝗹𝗺𝗻𝗼𝗽𝗾𝗿𝘀𝘁𝘂𝘃𝘄𝘅𝘆𝘇",
        ),
        (
            FontStyleKind::SansSerifItalic,
            "𝘈𝘉𝘊𝘋𝘌𝘍𝘎𝘏𝘐𝘑𝘒𝘓𝘔𝘕𝘖𝘗𝘘𝘙𝘚𝘛𝘜𝘝𝘞𝘟𝘠𝘡",
            "𝘢𝘣𝘤𝘥𝘦𝘧𝘨𝘩𝘪𝘫𝘬𝘭𝘮𝘯𝘰𝘱𝘲𝘳𝘴𝘵𝘶𝘷𝘸𝘹𝘺𝘻",
        ),
        (
            FontStyleKind::SansSerifBoldItalic,
            "𝘼𝘽𝘾𝘿𝙀𝙁𝙂𝙃𝙄𝙅𝙆𝙇𝙈𝙉𝙊𝙋𝙌𝙍𝙎𝙏𝙐𝙑𝙒𝙓𝙔𝙕",
            "𝙖𝙗𝙘𝙙𝙚𝙛𝙜𝙝𝙞𝙟𝙠𝙡𝙢𝙣𝙤𝙥𝙦𝙧𝙨𝙩𝙪𝙫𝙬𝙭𝙮𝙯",
        ),
        (
            FontStyleKind::Monospace,
            "𝙰𝙱𝙲𝙳𝙴𝙵𝙶𝙷𝙸𝙹𝙺𝙻𝙼𝙽𝙾𝙿𝚀𝚁𝚂𝚃𝚄𝚅𝚆𝚇𝚈𝚉",
            "𝚊𝚋𝚌𝚍𝚎𝚏𝚐𝚑𝚒𝚓𝚔𝚕𝚖𝚗𝚘𝚙𝚚𝚛𝚜𝚝𝚞𝚟𝚠𝚡𝚢𝚣",
        ),
    ];

    #[test]
    fn golden_alphabets() {
        assert_eq!(GOLDEN.len(), STYLE_OFFSETS.len());
        for (kind, upper, lower) in GOLDEN {
            assert_eq!(&fmt_style(UPPER, *kind), upper, "{kind:?} uppercase");
            assert_eq!(&fmt_style(LOWER, *kind), lower, "{kind:?} lowercase");
        }
    }

    #[test]
    fn round_trip_over_every_style_and_letter() {
        let mut kinds: Vec<FontStyleKind> = STYLE_OFFSETS.iter().map(|(k, _, _)| *k).collect();
        kinds.push(FontStyleKind::Upright);
        for kind in kinds {
            for c in UPPER.chars().chain(LOWER.chars()) {
                let styled = fmt_style_char(c, kind);
                assert_eq!(unstyle_char(styled), c, "{kind:?} {c}");
            }
        }
    }

    #[test]
    fn every_exception_round_trips_and_is_documented() {
        assert_eq!(STYLE_EXCEPTIONS.len(), 24);
        for (kind, ascii, replacement) in STYLE_EXCEPTIONS {
            assert_eq!(fmt_style_char(*ascii, *kind), *replacement);
            assert_eq!(unstyle_char(*replacement), *ascii);
            // the exceptions sit outside the U+1D400 blocks, which is why they exist
            assert!((*replacement as u32) < 0x1D400);
        }
    }

    #[test]
    fn non_letters_pass_through() {
        assert_eq!(fmt_style("0123 α, +", FontStyleKind::Bold), "0123 α, +");
        assert_eq!(unstyle("0123 α, +"), "0123 α, +");
    }

    #[test]
    fn upright_is_the_identity_but_still_a_style() {
        assert_eq!(fmt_style(UPPER, FontStyleKind::Upright), UPPER);
        assert_eq!(fmt_style(LOWER, FontStyleKind::Upright), LOWER);
        assert!(FontStyle::Style(FontStyleKind::Upright).is_style());
        assert!(!FontStyle::Default.is_style());
        assert!(!FontStyle::Disabled.is_style());
    }

    #[test]
    fn default_resolves_to_the_fallback_and_disabled_never_does() {
        let italic = FontStyle::Style(FontStyleKind::Italic);
        assert_eq!(
            FontStyle::Default.resolve(italic),
            Some(FontStyleKind::Italic)
        );
        assert_eq!(FontStyle::Default.resolve(FontStyle::Default), None);
        assert_eq!(FontStyle::Disabled.resolve(italic), None);
        assert_eq!(
            FontStyle::Style(FontStyleKind::Bold).resolve(italic),
            Some(FontStyleKind::Bold)
        );
        assert_eq!(FontStyle::default(), FontStyle::Default);
    }

    #[test]
    fn the_notes_full_alphabet_sample_matches() {
        // porting notes §10.14: the leading/trailing '-' and the space pass through
        let input = "-ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz-";
        let expected = "-".to_string()
            + "𝔸𝔹ℂ𝔻𝔼𝔽𝔾ℍ𝕀𝕁𝕂𝕃𝕄ℕ𝕆ℙℚℝ𝕊𝕋𝕌𝕍𝕎𝕏𝕐ℤ"
            + " "
            + "𝕒𝕓𝕔𝕕𝕖𝕗𝕘𝕙𝕚𝕛𝕜𝕝𝕞𝕟𝕠𝕡𝕢𝕣𝕤𝕥𝕦𝕧𝕨𝕩𝕪𝕫"
            + "-";
        assert_eq!(fmt_style(input, FontStyleKind::DoubleStruck), expected);
    }
}
