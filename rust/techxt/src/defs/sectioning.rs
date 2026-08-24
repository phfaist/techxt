//! The sectioning category: the seven heading commands and how they number themselves
//! (PLAN.md §9.2).
//!
//! All seven parse identically — `\section*[toc entry]{Title}`, spelled `s o m` and
//! named `("star", "toctitle", "title")` — and the table-of-contents title is accepted
//! and ignored, since plain text has no table of contents.
//!
//! # Numbering
//!
//! The run carries one counter per level. Incrementing level *L* raises `counters[L]`
//! and zeroes every deeper counter, so a new section restarts its subsections. Two
//! levels are special:
//!
//! - **`\part`** has its own counter, rendered in uppercase Roman, and resets nothing —
//!   parts group chapters, they do not renumber them.
//! - **`\chapter`** decides where a section number starts. The dotted number joins
//!   `counters[first..=L]`, where `first` is the chapter level once any `\chapter` has
//!   appeared in the run and the section level otherwise. So `\section` is `1` in an
//!   article and `3.1` in the third chapter of a book, with no document class to ask.
//!
//! A **starred** heading neither increments a counter nor shows a number, exactly as in
//! LaTeX, and levels 5 and 6 (`\paragraph`, `\subparagraph`) are never numbered.
//!
//! # Rendering
//!
//! Four styles, chosen by [`Options::heading_style`](crate::Options::heading_style);
//! the default numbers and underlines. The underline is the style's character repeated
//! to the heading line's *display width* — measured with
//! [`render_inline`](crate::layout::render_inline), the one place in techxt a width is
//! computed — so a heading with an accent or a CJK character still gets a rule of
//! exactly the right length.
//!
//! Nothing is ever uppercased. Case is content.

use alloc::string::String;

use crate::convert::HeadingStyle;
use crate::def::{Category, MacroDef, TextRule};
use crate::flow::{display_width, Flow, FlowItem};
use crate::render::{NodeView, RenderCx, RenderError};

use super::handler;

/// Level 0: `\part`, which counts in Roman numerals and resets nothing.
const PART: usize = 0;

/// Level 1: `\chapter`, the level whose presence decides where a section number starts.
const CHAPTER: usize = 1;

/// Level 2: `\section`, the shallowest level of an article.
const SECTION: usize = 2;

/// Level 4: `\subsubsection`, the deepest level that is ever numbered.
const DEEPEST_NUMBERED: usize = 4;

/// The sectioning category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("sectioning");
    for (level, name) in [
        "part",
        "chapter",
        "section",
        "subsection",
        "subsubsection",
        "paragraph",
        "subparagraph",
    ]
    .into_iter()
    .enumerate()
    {
        category.add_macro(
            MacroDef::new(name)
                .star()
                .arg("o", "toctitle")
                .arg("m", "title")
                .rule(handler(Heading { level })),
        );
    }
    structure(&mut category);
    category
}

/// The document-structure commands that surround the headings.
///
/// None of them prints anything a converter can produce. A table of contents, a list of
/// figures and a list of tables are *generated* from a document that has already been
/// numbered and paginated; running heads name a page nobody is turning; and
/// `\addcontentsline` writes into a file plain text has no equivalent of. Every one is
/// declared — so that its arguments cannot leak — and renders as nothing.
///
/// `\appendix` is the interesting one: it switches section numbering to letters, which
/// techxt does not do. Headings after it keep counting in the numbers they were up to
/// (PLAN.md §17 leaves numbering to a later version), and the appendix's own heading
/// text is rendered exactly as it was written.
fn structure(category: &mut Category) {
    for name in [
        "tableofcontents",
        "listoffigures",
        "listoftables",
        "appendix",
        "frontmatter",
        "mainmatter",
        "backmatter",
    ] {
        category.add_macro(MacroDef::new(name).rule(TextRule::Skip));
    }
    category.add_macro(
        MacroDef::new("markboth")
            .arg("m", "left")
            .arg("m", "right")
            .rule(TextRule::Skip),
    );
    category.add_macro(
        MacroDef::new("markright")
            .arg("m", "right")
            .rule(TextRule::Skip),
    );
    category.add_macro(
        MacroDef::new("addcontentsline")
            .arg("m", "file")
            .arg("m", "level")
            .arg("m", "entry")
            .rule(TextRule::Skip),
    );
    category.add_macro(
        MacroDef::new("addtocontents")
            .arg("m", "file")
            .arg("m", "content")
            .rule(TextRule::Skip),
    );
}

/// One heading command, identified by its level.
#[derive(Debug)]
struct Heading {
    /// 0 for `\part`, 6 for `\subparagraph`.
    level: usize,
}

impl crate::def::TextHandler for Heading {
    fn render(&self, _node: NodeView<'_>, cx: &mut RenderCx<'_, '_>) -> Result<Flow, RenderError> {
        let starred = cx.arg_provided("star");
        let style = cx.options().heading_style;
        // Only the numbered style counts. Under the others no number is ever shown, so
        // there is nothing for a counter to be.
        let numbered = !starred && style == HeadingStyle::NumberedUnderlined && self.numberable();

        let number = self.count(cx, numbered);

        // The title is rendered after the counters are settled, because rendering it
        // may itself have side effects (a footnote inside a heading is numbered where
        // it stands) and those must happen in document order.
        let title = cx.arg_text("title")?.unwrap_or_default();
        let line = self.heading_line(style, number.as_deref(), &title);

        let mut flow = Flow::new();
        // A heading is its own block: a blank line on either side, which layout
        // normalizes against whatever the neighbours asked for.
        flow.push(FlowItem::ParagraphBreak);
        // One text item, not a word sequence: the underline below is measured from
        // this exact string, and a line break inside the heading would make the two
        // disagree.
        flow.push(FlowItem::Text(line.clone().into_boxed_str()));
        if let Some(character) = self.underline_char(style) {
            flow.push(FlowItem::HardBreak);
            let width = display_width(&line);
            let rule: String = core::iter::repeat_n(character, width).collect();
            flow.push(FlowItem::Text(rule.into_boxed_str()));
        }
        flow.push(FlowItem::ParagraphBreak);
        Ok(flow)
    }
}

impl Heading {
    /// Whether this level can carry a number at all (PLAN.md §9.2: 5 and 6 never do).
    fn numberable(&self) -> bool {
        self.level <= DEEPEST_NUMBERED
    }

    /// Update the run's counters and hand back the number to display, if any.
    fn count(&self, cx: &mut RenderCx<'_, '_>, numbered: bool) -> Option<String> {
        if self.level == CHAPTER {
            // "Any `\chapter` has occurred" is what decides where a section number
            // starts, and a `\chapter*` is still a chapter: it says the document is
            // organized in chapters, which is the question being asked. Its counter
            // stays where it is, so a section following an unnumbered opening chapter
            // numbers as `0.1` — which is what LaTeX itself produces.
            *cx.chapter_seen_mut() = true;
        }
        if !numbered {
            return None;
        }

        let chapters = *cx.chapter_seen_mut();
        let counters = cx.heading_counters_mut();
        counters[self.level] += 1;
        if self.level != PART {
            // A part groups what follows; it does not renumber it.
            for deeper in counters.iter_mut().skip(self.level + 1) {
                *deeper = 0;
            }
        }

        if self.level == PART {
            return Some(roman(counters[PART]));
        }
        let first = if chapters { CHAPTER } else { SECTION };
        let mut number = String::new();
        for counter in &counters[first..=self.level] {
            if !number.is_empty() {
                number.push('.');
            }
            let _ = core::fmt::Write::write_fmt(&mut number, format_args!("{counter}"));
        }
        Some(number)
    }

    /// The heading's one line of text, per PLAN.md §9.2's table.
    fn heading_line(&self, style: HeadingStyle, number: Option<&str>, title: &str) -> String {
        let mut line = String::new();
        match style {
            // A starred heading reaches here with no number, and then renders exactly
            // as the `Underlined` style would: the title alone, over its rule.
            HeadingStyle::NumberedUnderlined => {
                if let Some(number) = number {
                    if self.level == PART {
                        line.push_str("Part ");
                    }
                    line.push_str(number);
                    if self.level == PART {
                        line.push(':');
                    }
                    line.push(' ');
                }
            }
            HeadingStyle::Prefix => line.push_str(self.prefix()),
            HeadingStyle::Underlined | HeadingStyle::Plain => {}
        }
        line.push_str(title);
        line
    }

    /// The fixed label the `Prefix` style writes instead of a number.
    fn prefix(&self) -> &'static str {
        match self.level {
            PART => "Part: ",
            CHAPTER => "Chapter: ",
            SECTION => "\u{a7} ",
            3 => "\u{a7}.\u{a7} ",
            DEEPEST_NUMBERED => "\u{a7}.\u{a7}.\u{a7} ",
            // `\paragraph` and `\subparagraph` are run-in headings in LaTeX; a plain
            // line of their own is as much as plain text can say about them.
            _ => "",
        }
    }

    /// The character this heading is underlined with, if it is underlined at all.
    fn underline_char(&self, style: HeadingStyle) -> Option<char> {
        match style {
            HeadingStyle::NumberedUnderlined | HeadingStyle::Underlined => match self.level {
                PART | CHAPTER => Some('='),
                SECTION => Some('-'),
                3 => Some('~'),
                _ => None,
            },
            HeadingStyle::Prefix | HeadingStyle::Plain => None,
        }
    }
}

/// `value` in uppercase Roman numerals, as `\part` is numbered.
///
/// Zero has no Roman numeral; it cannot occur here (a part is numbered only after its
/// counter has been incremented), and `0` is the honest answer if it ever does. Values
/// past 3999 simply accumulate `M`s, which is the classical convention.
fn roman(value: u32) -> String {
    const PIECES: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    if value == 0 {
        return String::from("0");
    }
    let mut left = value;
    let mut out = String::new();
    for (amount, numeral) in PIECES {
        while left >= amount {
            out.push_str(numeral);
            left -= amount;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_count_in_roman_numerals() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(9), "IX");
        assert_eq!(roman(14), "XIV");
        assert_eq!(roman(1987), "MCMLXXXVII");
        assert_eq!(roman(0), "0");
    }

    #[test]
    fn the_prefix_style_repeats_a_section_sign_per_level() {
        assert_eq!(Heading { level: 2 }.prefix(), "§ ");
        assert_eq!(Heading { level: 3 }.prefix(), "§.§ ");
        assert_eq!(Heading { level: 4 }.prefix(), "§.§.§ ");
        assert_eq!(Heading { level: 5 }.prefix(), "");
    }

    #[test]
    fn only_the_underlining_styles_underline() {
        let section = Heading { level: 2 };
        assert_eq!(
            section.underline_char(HeadingStyle::NumberedUnderlined),
            Some('-')
        );
        assert_eq!(section.underline_char(HeadingStyle::Underlined), Some('-'));
        assert_eq!(section.underline_char(HeadingStyle::Prefix), None);
        assert_eq!(section.underline_char(HeadingStyle::Plain), None);
        // Nothing below a subsection is underlined.
        assert_eq!(
            Heading { level: 4 }.underline_char(HeadingStyle::Underlined),
            None
        );
    }
}
