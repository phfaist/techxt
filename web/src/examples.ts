/**
 * The five sample documents of web/PLAN.md §6.7, inlined as string constants so they
 * cost no fetch and work offline.
 *
 * Each is at most about fifteen lines — a demo, not a corpus — and each was run
 * through `cargo run -q --bin techxt` before being pasted here: all five convert with
 * exit code 0 and **no diagnostics at all**, which is the point. A sample that greets
 * a first-time visitor with a warning is teaching the wrong thing about the tool.
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
const PAPER = String.raw`\section{A theory of everything}

We revisit Poincar\'e's argument in the light of \emph{modern} notation. The
central estimate is $E = mc^2$, which the next section sharpens to an
inequality.\footnote{The constant is not optimal; see \cite{poincare1905}.}

\subsection{Notation}

Throughout, $c$ is the speed of light and $m$ the rest mass.
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
const UNICODE = String.raw`techxt copies text it does not recognise straight through, so a document can
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
    id: 'paper',
    title: 'A paper fragment',
    blurb: 'Sections, emphasis, an accent, an inline formula, a footnote and a citation.',
    source: PAPER,
  },
  {
    id: 'math',
    title: 'Mathematics',
    blurb: 'Sums with limits, fractions, roots, Greek letters and a display matrix.',
    source: MATH,
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
