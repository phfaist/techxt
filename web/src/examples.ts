/**
 * The six sample documents of web/PLAN.md §6.7, inlined as string constants so they
 * cost no fetch and work offline.
 *
 * Each is at most about fifteen lines — a demo, not a corpus — and each was converted
 * with the library's own defaults before being pasted here: all six convert with **no
 * diagnostics at all**, which is the point. A sample that greets a first-time visitor
 * with a warning is teaching the wrong thing about the tool. (The first five were run
 * through `cargo run -q --bin techxt`; the sixth, added at M9, through the binding's
 * own `convert_native`, which is the same conversion the page performs.)
 *
 * `String.raw` is what keeps a LaTeX source readable in a TypeScript file: `\\` in the
 * `tabular` below is a LaTeX row break, not an escaped backslash.
 */

import type { ExampleDoc } from './types';

/**
 * The first visit's document (§6.7): a section, an accent, emphasis, an inline
 * formula, a footnote and a citation, in one screen.
 *
 * `\cite` resolves to techxt's `<cit.>` placeholder rather than a warning — the
 * bibliography is not part of the fragment — so this stays clean.
 */
const EINSTEIN = String.raw`\section{The Equivalence Principle}

Einstein's great insight was to treat gravity not as a force but a property of
spacetime itself. The \emph{equivalence principle} states that at any point in
spacetime, there is always a choice of a coordinate system that is locally the
flat Minkowski metric $\eta_{\mu\nu}$ at that point.\footnote{See
\cite{MyFavoriteGRBook} as well as
\href{https://en.wikipedia.org/wiki/Equivalence_principle}{Wikipedia} for more
details.}

\subsection{Curvature and the field equations}

The energy and momentum are the sources of the curvature of spacetime. This
relation is captured by \textbf{Einstein's equations}:
\begin{equation}
  R_{\mu\nu} - \tfrac{1}{2} R \, g_{\mu\nu} = \frac{8\pi G}{c^4} T_{\mu\nu} ,
\end{equation}
where $g_{\mu\nu}$ is the metric and $T_{\mu\nu}$ the stress-energy tensor.
`;

/** Sums with limits, a fraction, a root, Greek, a matrix and a display equation. */
const MATH = String.raw`The partial sums of $\sum_{k=1}^{n} 1/k^2$ converge, and the limit is
\begin{equation}
  \sum_{k=1}^{\infty} \frac{1}{k^{2}} = \frac{\pi^{2}}{6} .
\end{equation}
For $\alpha, \beta \in \mathbb{R}$ the norm is $\sqrt{\alpha^{2} + \beta^{2}}$,
and a rotation by $\theta$ acts as
\[
  R(\theta) = \begin{pmatrix} \cos\theta & -\sin\theta \\
                              \sin\theta & \cos\theta \end{pmatrix}
\]
`;

/**
 * A preamble's worth of shorthands, and what they expand to (§5, "Macro definitions").
 *
 * The one example whose point is the *parse* rather than the rendering: `\newcommand`
 * and `\newenvironment` written in the document take effect, so `\ket{0}` is `|0⟩` and
 * the `aside` environment is a quote block. Switching the option the aside names turns
 * every one of them back into an unknown command, which is the fastest way to see what
 * the setting does.
 */
const MACROS = String.raw`\section{A preamble of one's own}

\newcommand{\ket}[1]{|#1\rangle}
\newcommand{\braket}[2]{\langle #1 | #2 \rangle}
\newcommand{\Hilb}{\mathcal{H}}
\newenvironment{aside}{\begin{quote}\emph{Aside:} }{\end{quote}}

Definitions written in the document are expanded where they are used, so a qubit
lives in $\Hilb = \mathbb{C}^{2}$, is spanned by $\ket{0}$ and $\ket{1}$, and is
normalized when $\braket{\psi}{\psi} = 1$.

\begin{aside}
  Switch \textbf{Macro definitions} to \emph{read and dropped} in More options and
  every shorthand above becomes an unknown command again.
\end{aside}
`;

/** Nested lists, and a `tabular` whose columns line up in the output. */
const LISTS = String.raw`\begin{itemize}
  \item Nested lists keep their markers:
    \begin{enumerate}
      \item first, then
      \item second.
    \end{enumerate}
  \item Tables are laid out in columns:
\end{itemize}

\begin{tabular}{lrr}
  Method & Time (ms) & Memory (MB) \\
  \hline
  Baseline & 128 & 4.5 \\
  This work & 12 & 4.1 \\
\end{tabular}
`;

/**
 * LaTeX's opening double quote. Two backticks cannot appear literally inside a
 * template literal, so this one character sequence arrives by substitution.
 */
const OPEN_QUOTES = '``';

/** The long tail: accents, dashes, quotes, the Greek alphabet and arrows. */
const SYMBOLS = String.raw`Names keep their accents: G\"odel, Poincar\'e, Fran\c{c}ois, Erd\H{o}s and
Wei\ss{}enb\"ock. Punctuation is real punctuation---an em dash, an en dash
(1--12), ${OPEN_QUOTES}curly quotes'' and \dots\ ellipses.

Greek runs $\alpha, \beta, \gamma, \delta, \epsilon, \zeta, \eta, \theta,
\iota, \kappa, \lambda, \mu, \nu, \xi, \pi, \rho, \sigma, \tau, \upsilon,
\phi, \chi, \psi, \omega$, and the arrows point where you would expect:
$\to$, $\gets$, $\Rightarrow$, $\Leftrightarrow$, $\mapsto$, $\uparrow$.
`;

/** The case the font fallback chains of §8.2 exist for. */
const UNICODE = String.raw`Techxt copies text it does not recognize straight through, so a document can
mix scripts freely: \emph{kanji} 漢字とかな in Japanese, \emph{ivrit} עברית
written right to left, and an emoji 🎉 in the middle of a sentence.

\begin{itemize}
  \item Mathematics still works alongside it: $\sum_{i=1}^{n} x_i \le n$.
  \item So do accents beside passthrough: \"u, 東京, Fran\c{c}ois, \'a.
\end{itemize}
`;

/** The Load ▾ menu, in the order it shows them. The first one is the first visit's. */
export const EXAMPLES: readonly ExampleDoc[] = [
  {
    id: 'ep',
    title: 'The equivalence principle',
    blurb: 'Sections, emphasis, an accent, math, a footnote and a citation.',
    source: EINSTEIN,
  },
  {
    id: 'math',
    title: 'Mathematics',
    blurb: 'Sums with limits, fractions, roots, Greek letters and a display matrix.',
    source: MATH,
  },
  {
    id: 'macros',
    title: 'Macros of your own',
    blurb: '\\newcommand and \\newenvironment, expanded where they are used.',
    source: MACROS,
  },
  {
    id: 'lists',
    title: 'Lists and tables',
    blurb: 'Nested itemize and enumerate, and a tabular that lines up in text.',
    source: LISTS,
  },
  {
    id: 'symbols',
    title: 'Accents and symbols',
    blurb: 'Accented names, dashes, quotes, the Greek alphabet and arrows.',
    source: SYMBOLS,
  },
  {
    id: 'unicode',
    title: 'Unicode passthrough',
    blurb: 'LaTeX markup mixed with Japanese, Hebrew and an emoji.',
    source: UNICODE,
  },
];

/** What a first visit loads before the user has touched anything (§6.7). */
export const DEFAULT_EXAMPLE: ExampleDoc = EXAMPLES[0] as ExampleDoc;
