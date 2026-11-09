//! The behaviour contract between the two parse languages (build plan §5.9).
//!
//! A converter for techy's plain `Latexlike` must convert every document exactly as a
//! converter for techy-xp's `LatexlikeXp` does with its definitions only *declared*
//! (`MacroDefinitions::Declared`): the same `text`, and the same diagnostics when they
//! are compared by severity and message. Spans are deliberately left out: the two
//! languages' span regimes differ by design (see `def_dispatch.rs`, "over plain techy").
//!
//! The corpus is a table, run under each of the three [`MathMode`]s, and every document
//! that disagrees is reported at once rather than the first one only.
//!
//! # Coverage of `defs::standard()`
//!
//! The 19 category modules under `src/defs/`, and the documents (by their label in
//! [`CORPUS`]) that reach each one:
//!
//! | Category          | Documents                                                    |
//! |-------------------|--------------------------------------------------------------|
//! | `symbols_extra`   | `symbols-text`, `symbols-math`                               |
//! | `base`            | `base-spacing`, `base-ligatures`, `base-footnotes`, `base-skips`, `ligature-in-math` |
//! | `accents`         | `accents`, `accents-braced`                                  |
//! | `fontstyles`      | `fonts-text`, `fonts-declarations`, `fonts-math`             |
//! | `mathcore`        | `math-inline`, `math-operators`, `math-display-bracket`, `math-text-in-math` |
//! | `mathenvs`        | `env-equation`, `env-align`, `env-gather`, `env-matrices`, `env-math-forms` |
//! | `subsuperscripts` | `scripts`, `math-display-dollars`                            |
//! | `sectioning`      | `sections`, `section-marks`                                  |
//! | `lists`           | `list-itemize-enumerate`, `list-description`                 |
//! | `verbatim`        | `verbatim-env`, `verbatim-inline`, `verbatim-others`         |
//! | `tables`          | `table-tabular`, `table-spans`, `table-float`                |
//! | `theorems`        | `theorem-proof`, `blocks`                                    |
//! | `refs`            | `refs`, `bibliography`                                       |
//! | `links`           | `links`                                                      |
//! | `graphics`        | `graphics`                                                   |
//! | `titling`         | `titling`                                                    |
//! | `preamble`        | `preamble-definers`, `definer-unbraced`, `definer-def`, `preamble-document` |
//! | `inputs`          | `input-resolved`, `input-unresolved`                         |
//! | `natbib`          | `natbib`                                                     |
//!
//! Beyond the categories: the three math delimiter forms (`math-inline`,
//! `math-display-bracket`, `math-display-dollars`, `math-paren`), a preamble with every
//! defining command (`preamble-definers`), unknown commands and environments
//! (`unknown`), and malformed input (`malformed-*`), comments and paragraphs.

use techy::latexlike::Latexlike;
use techy_xp::lang::LatexlikeXp;

use techxt::convert::{Conversion, MacroDefinitions, MapResolver, MathMode, Severity};
use techxt::def::{Category, DefinitionSet, MacroDef, ParseLang, Template, TextRule};
use techxt::{defs, Converter, ConverterBuilder};

/// The documents, each with a label the failure report names it by.
const CORPUS: &[(&str, &str)] = &[
    // symbols_extra
    ("symbols-text", r"\AE\ \copyright\ \dag\ \S 3 \pounds 5"),
    ("symbols-math", r"$\Alpha \leq \Angle \infty \partial \nabla$"),
    // base
    ("base-spacing", r"a\quad b\qquad c\\ d\newline e\par f~g"),
    ("base-ligatures", r"``quoted'' --- and -- dash !` ?` `single'"),
    ("base-footnotes", r"Text\footnote{a note} more\footnote[7]{marked}."),
    ("base-skips", r"a\hspace{1cm}b\linebreak c\pagebreak d\vspace*{2pt}e"),
    ("ligature-in-math", r"$a--b$ and a--b"),
    // accents
    ("accents", r#"\'e \`a \^o \"u \~n \c{c} \v{s} \H{o} \k{a} \r{a} \={a} \.{z} \u{g}"#),
    ("accents-braced", r#"\'{E}cole na\"{\i}ve \t{oo} \b{x} \d{y}"#),
    // fontstyles
    ("fonts-text", r"\textbf{bold} \textit{it} \emph{em} \textsc{caps} \texttt{mono}"),
    ("fonts-declarations", r"{\bf old} {\itshape shape} \textsf{sans} \textrm{roman}"),
    ("fonts-math", r"$\mathbf{x} \mathcal{A} \mathbb{R} \mathfrak{g} \mathrm{d}x$"),
    // mathcore, and the delimiter forms
    ("math-inline", r"$\alpha + \beta \leq \sqrt{x} + \sqrt[3]{y} - \frac{a}{b}$"),
    ("math-operators", r"$a \bmod b$, $a \pmod{n}$, $x \coloneqq \vert y \vert$, $\operatorname{tr} A$"),
    ("math-display-bracket", r"before \[ \int_0^1 f(x)\,dx = \sum_{i=1}^{n} i \] after"),
    ("math-display-dollars", r"before $$ x^2 + y^2 = z^2 $$ after"),
    ("math-paren", r"inline \( a \cdot b \) here"),
    ("math-text-in-math", r"$x \text{ if } y$ and \ensuremath{\alpha}"),
    // mathenvs
    ("env-equation", r"\begin{equation} E = mc^2 \label{eq} \end{equation}"),
    ("env-align", r"\begin{align} a &= b \\ c &= d \nonumber \end{align}"),
    ("env-gather", r"\begin{align*} x &= 1 \end{align*}\begin{gather} y \end{gather}"),
    ("env-matrices", r"$\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}$ \begin{array}{cc} a & b \end{array}"),
    ("env-math-forms", r"\begin{math} x \end{math} \begin{displaymath} y \end{displaymath}"),
    // subsuperscripts
    ("scripts", r"$x^2 + y_i + z^{2n} + w_{ij}^{k}$ and a_b text a^b"),
    // sectioning
    ("sections", r"\section{Intro} text \subsection*{Sub} more \paragraph{Para} p"),
    ("section-marks", r"\markboth{a}{b}\markright{c}\addcontentsline{toc}{section}{x}z"),
    // lists
    ("list-itemize-enumerate", r"\begin{itemize}\item a \begin{enumerate}\item b \item[*] c\end{enumerate}\end{itemize}"),
    ("list-description", r"\begin{description}\item[Term] definition \item plain\end{description}"),
    // verbatim
    ("verbatim-env", "\\begin{verbatim}\n\\textbf{x} $y$ % not a comment\n\\end{verbatim}"),
    ("verbatim-inline", r"\verb|\foo{}| and \verb+a b+ and \verb*|x y|"),
    ("verbatim-others", "\\begin{comment} hidden \\end{comment}\\begin{lstlisting}[language=C]\nint x;\n\\end{lstlisting}"),
    // tables
    ("table-tabular", r"\begin{tabular}{|l|c|}\hline a & b \\ \hline c & d \\ \hline\end{tabular}"),
    ("table-spans", r"\begin{tabular}{ll}\toprule \multicolumn{2}{c}{wide} \\ \midrule \multirow{2}{*}{m} & x \\ \cline{1-2} & y \\ \bottomrule\end{tabular}"),
    ("table-float", r"\begin{table}\centering\begin{tabularx}{\textwidth}{XX} a & b\end{tabularx}\caption{Cap}\label{t}\end{table}"),
    // theorems
    ("theorem-proof", r"\begin{theorem}[Name] statement \end{theorem}\begin{proof} pf \end{proof}"),
    ("blocks", r"\begin{center} c \end{center}\begin{quote} q \end{quote}\begin{abstract} ab \end{abstract}\begin{minipage}{3cm} m \end{minipage}"),
    // refs
    ("refs", r"See \ref{a}, \eqref{b}, \pageref{c}, \cite{d}, \cite[p.~2]{e}.\label{f}"),
    ("bibliography", r"\begin{thebibliography}{9}\bibitem{k} Knuth. \bibitem[L]{l} Lamport.\end{thebibliography}"),
    // links
    ("links", r"\url{https://x.org} \href{https://y.org}{text} \nolinkurl{z} \path{/a/b}"),
    // graphics
    ("graphics", r"\includegraphics[width=2cm]{fig.png} \begin{figure}\includegraphics{a}\caption{F}\end{figure}"),
    // titling
    ("titling", r"\title{T}\author{A \and B\thanks{x}}\date{\today}\maketitle body"),
    // preamble: every defining command, declared
    ("preamble-definers", concat!(
        r"\newcommand{\greet}[1]{Hi #1}\renewcommand{\emph}[1]{#1}\providecommand{\x}{X}",
        r"\def\y#1{Y}\gdef\z{Z}\let\a=\b\edef\e{E}\xdef\f{F}",
        r"\NewDocumentCommand\n{m}{N}\DeclareMathOperator{\Tr}{Tr}",
        r"\newenvironment{foo}{[}{]}\renewenvironment{bar}{(}{)}",
        r"\greet{you} \x \y{1} \z \n{2} $\Tr A$ \begin{foo}in\end{foo}",
    )),
    ("definer-unbraced", r"\newcommand\greet{Hi}\greet"),
    ("definer-def", r"\def\x#1{[#1]}\x{a}"),
    ("preamble-document", r"\documentclass[11pt]{article}\usepackage[utf8]{inputenc}\usepackage{amsmath,amssymb}\begin{document}Hello.\end{document}"),
    // unknown constructs
    ("unknown", r"a \nosuchmacro{arg} b \begin{nosuchenv} c \end{nosuchenv} d"),
    // inputs
    ("input-resolved", r"before \input{part.tex} middle \include{part} after"),
    ("input-unresolved", r"before \input{missing.tex} after"),
    // natbib
    ("natbib", r"\citet{a} \citep[p.~3]{b} \citeauthor{c} \citeyear{d} \citenum{e} \citetext{f} \citealias{g}"),
    // the rest of a document
    ("comments", "a % a comment\n b %\n c"),
    ("paragraphs", "para one\nstill one\n\n\npara two\n\n"),
    ("malformed-unclosed-group", r"\textbf{unclosed and $x"),
    ("malformed-stray-close", r"a } b \end{itemize} c"),
    ("malformed-missing-argument", r"\frac{a} \textbf"),
];

/// A converter for `L` over the shipped definitions, in `math_mode`, with a resolver
/// that knows `part.tex` (and `part`, for `\include`).
fn converter<L: ParseLang>(
    builder: ConverterBuilder<L>,
    math_mode: MathMode,
) -> Converter<L> {
    let mut resolver = MapResolver::new();
    resolver.insert("part.tex", r"\textbf{inside} $y_1$");
    resolver.insert("part", r"\emph{included}");
    builder
        .source_resolver(resolver)
        .math_mode(math_mode)
        .build()
        .expect("builds")
}

/// What the contract compares: the text, and each diagnostic's severity and message.
fn observed(conversion: &Conversion) -> (String, Vec<(Severity, String)>) {
    let diagnostics = conversion
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.severity(), diagnostic.message()))
        .collect();
    (conversion.text.clone(), diagnostics)
}

#[test]
fn latexlike_converts_as_latexlike_xp_with_declared_definitions() {
    assert!(
        CORPUS.len() >= 40,
        "the corpus has {} documents",
        CORPUS.len()
    );

    let mut failures = Vec::new();
    for math_mode in [MathMode::Fancy, MathMode::Plain, MathMode::Source] {
        let plain: Converter<Latexlike> = converter(
            ConverterBuilder::with_definitions(defs::standard()),
            math_mode,
        );
        let xp: Converter<LatexlikeXp> = converter(
            Converter::builder().macro_definitions(MacroDefinitions::Declared),
            math_mode,
        );
        for (label, latex) in CORPUS {
            let plain = plain.latex_to_text(latex).map(|c| observed(&c));
            let xp = xp.latex_to_text(latex).map(|c| observed(&c));
            match (&plain, &xp) {
                (Ok(plain), Ok(xp)) if plain == xp => {}
                (Err(plain), Err(xp)) if plain.to_string() == xp.to_string() => {}
                _ => failures.push(format!(
                    "{label} ({math_mode:?}): {latex:?}\n  Latexlike:   {plain:?}\n  \
                     LatexlikeXp: {xp:?}"
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreement(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_corpus_is_not_trivial() {
    // Parity over documents that convert to nothing would prove nothing. Every document
    // must produce some text under the default math mode, and the corpus as a whole
    // must exercise the diagnostics half of the contract, at more than one severity.
    let xp: Converter<LatexlikeXp> = converter(
        Converter::builder().macro_definitions(MacroDefinitions::Declared),
        MathMode::Fancy,
    );
    let mut empty = Vec::new();
    let mut severities = Vec::new();
    for (label, latex) in CORPUS {
        let conversion = xp.latex_to_text(latex).expect("parses");
        if conversion.text.trim().is_empty() {
            empty.push(*label);
        }
        for diagnostic in conversion.diagnostics.iter() {
            if !severities.contains(&diagnostic.severity()) {
                severities.push(diagnostic.severity());
            }
        }
    }
    assert!(
        empty.is_empty(),
        "documents converting to nothing: {empty:?}"
    );
    assert!(severities.len() >= 2, "only {severities:?} reported");
}

// ------------------------------------------- dispatch step 2, across the two languages

/// The definitions holding only `\mark{text}`, rendered through a template no shipped
/// definition writes.
fn marks<L: ParseLang>() -> DefinitionSet<L> {
    DefinitionSet::new().with(
        Category::new("marks").with_macro(
            MacroDef::new("mark")
                .arg("m", "text")
                .rule(TextRule::Template(Template::new("<<{text}>>"))),
        ),
    )
}

/// A converter for `L` with no definitions at all: an empty override map, an empty name
/// table, and the default `Skip` policy for whatever reaches step 4.
fn empty<L: ParseLang>() -> Converter<L> {
    ConverterBuilder::with_definitions(DefinitionSet::new())
        .build()
        .expect("builds")
}

// Both directions below render a tree with a converter whose definitions are empty, so
// `<<x>>` can only come from the `TechxtMacroSpec` the parsing converter embedded in the
// tree (dispatch step 2): with nothing at steps 1 and 3, the `Skip` policy at step 4
// would print `ab`. And both would fail if the downcast happened at the *renderer's*
// language rather than the tree's: the embedded spec is a `TechxtMacroSpec<Tree>`, and
// downcasting it to `TechxtMacroSpec<Renderer>` — a different type, since the two
// languages differ — finds nothing, which drops the node to step 4 and prints `ab`.

#[test]
fn a_latexlike_tree_carries_its_rule_to_a_latexlike_xp_renderer() {
    let parsing: Converter<Latexlike> = ConverterBuilder::with_definitions(marks())
        .build()
        .expect("builds");
    let tree = parsing
        .language()
        .parse(r"a\mark{x}b")
        .expect("parses")
        .tree;

    let rendering: Converter<LatexlikeXp> = empty();
    let conversion = rendering.tree_to_text(&tree);
    assert_eq!(conversion.text, "a<<x>>b\n");
    assert!(conversion.diagnostics.is_empty());
}

#[test]
fn a_latexlike_xp_tree_carries_its_rule_to_a_latexlike_renderer() {
    // Parsed with the definers honoured (`MacroDefinitions::Honored`, the default): the
    // expanding reader is in place, and the tree it builds still embeds techxt's spec.
    let parsing: Converter<LatexlikeXp> = ConverterBuilder::with_definitions(marks())
        .macro_definitions(MacroDefinitions::Honored)
        .build()
        .expect("builds");
    let tree = parsing
        .language()
        .parse(r"a\mark{x}b")
        .expect("parses")
        .tree;

    let rendering: Converter<Latexlike> = empty();
    let conversion = rendering.tree_to_text(&tree);
    assert_eq!(conversion.text, "a<<x>>b\n");
    assert!(conversion.diagnostics.is_empty());
}
