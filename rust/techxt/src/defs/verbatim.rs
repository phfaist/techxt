//! Verbatim text (PLAN.md §9.8): the `verbatim` environment family and `\verb`.
//!
//! Verbatim content reaches the flow as [`Verbatim`](crate::flow::FlowItem::Verbatim)
//! and [`InlineVerbatim`](crate::flow::FlowItem::InlineVerbatim) items, which the
//! layout engine emits byte for byte: never wrapped, never trimmed, never styled.
//!
//! # The definitions do all of it
//!
//! There is no verbatim *handler* here, and that is the point of declaring a construct
//! once for both sides. An environment declared with
//! [`verbatim_body`](crate::def::EnvDef::verbatim_body) parses its body as a single run
//! of raw characters (techy's `VerbatimBehavior`, DECISIONS.md C6), and the renderer
//! asks the same declaration what kind of body it is before deciding how to emit the
//! characters it finds — so a rule of [`Content`](crate::def::TextRule::Content) is
//! enough, and nothing has to guess from the text whether markup in it was meant
//! literally.
//!
//! `\verb` works the same way one level down: its argument is declared with the `v`
//! code, techy parses it as a delimiter-matched verbatim run, and the characters land
//! in a group the renderer recognizes.
//!
//! # What is deliberately not distinguished
//!
//! `verbatim*` prints spaces as `␣` in LaTeX and `alltt` still expands commands inside
//! its body. Plain text has no use for the first and techxt does not implement the
//! second: both render exactly like `verbatim`, which loses a document nothing that a
//! reader of the *text* would have seen.

use alloc::string::String;
use alloc::sync::Arc;

use techy::core::constructs::OptionalGroupArgumentParser;
use techy::core::specs::ArgumentSpec;
use techy::core::token::GroupRule;
use techy::latexlike::{GroupType, Latexlike};

use crate::def::{Category, EnvDef, MacroDef, Template, TextRule};

/// The verbatim category (PLAN.md §12.1).
pub fn category() -> Category {
    let mut category = Category::new("verbatim");
    for name in ["verbatim", "verbatim*", "alltt"] {
        category.add_env(EnvDef::new(name).verbatim_body().rule(TextRule::Content));
    }
    // `lstlisting`'s key-value options say how the *listing* is typeset — which
    // language to colour it as, whether to number its lines — so there is nothing in
    // them for plain text and no diagnostic worth raising: dropping them loses no
    // content. They are still parsed, or they would show up in the code.
    category.add_env(
        EnvDef::new("lstlisting")
            .arg_spec(adjacent_options())
            .verbatim_body()
            .rule(TextRule::Content),
    );
    // The star is `\verb*`, which shows spaces; it changes nothing here, but declaring
    // it keeps the `*` from being taken for the delimiter of the verbatim run. Only the
    // run itself is rendered, which is why this is a template rather than
    // `TextRule::Content` — the latter would render every provided argument, the star
    // included.
    category.add_macro(
        MacroDef::new("verb")
            .star()
            .arg("v", "text")
            .rule(TextRule::Template(Template::new("{text}"))),
    );
    category
}

/// `lstlisting`'s `[options]`: optional, and only if it comes *immediately*.
///
/// The adjacency requirement is not a nicety. Everything after the environment's
/// arguments is raw listing text, so a listing whose first line begins with `[` —
/// an array literal, a Markdown link, a BibTeX-ish record — would otherwise have that
/// line parsed as the option list and silently eaten. techy's
/// [`require_adjacent`](OptionalGroupArgumentParser::require_adjacent) (DECISIONS.md D8)
/// says what LaTeX means here: `\begin{lstlisting}[language=C]` has options,
/// `\begin{lstlisting}` followed by a newline and a `[` does not. pylatexenc spells the
/// same requirement `allow_pre_space=False`, for the same reason.
fn adjacent_options() -> ArgumentSpec<Latexlike> {
    let delimiters = Arc::new(GroupRule {
        group_type: GroupType::Content,
        open: String::from("["),
        close: String::from("]"),
    });
    ArgumentSpec::new(
        OptionalGroupArgumentParser::new(delimiters).require_adjacent(),
        "options",
    )
}
