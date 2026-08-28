/**
 * What a document calls itself (web/PLAN.md §6.3, §6.10).
 *
 * A document's own heading is the only name the app ever has for it, and two features
 * want it: Download names the file after it, and the library names an entry after it.
 * Both used to be one regex in `main.ts`; a second caller made it worth its own file,
 * where it is pure and a test can reach it.
 *
 * Nothing here parses LaTeX. The regex looks for the first `\title` or `\section` and
 * gives up on anything with a brace inside it — a heading that says
 * `\section{The \emph{hard} case}` simply has no title as far as this file is
 * concerned, and the caller falls back. Reaching for the real parser would mean
 * waiting for a conversion before a file could be named.
 */

/** The first `\title` or `\section`, if the document has one this simple. */
const TITLE_PATTERN = /\\(?:title|section)\*?\s*(?:\[[^\]]*\])?\{([^{}]{1,120})\}/;

/** The longest title the library keeps; longer ones are cut at a word if it can. */
export const MAX_TITLE = 80;

/**
 * Drop the LaTeX from a line and leave the words: `\emph{Grüße}` reads as `Grüße`.
 *
 * The characters removed are the ones that are markup wherever they appear —
 * `$ \ { } ~ ^ _ & #`. In a title or a first line they are never punctuation, and
 * leaving them in produces names like `_ k=1 ^ n` out of a formula.
 */
function words(raw: string): string {
  return raw
    .replace(/\\[a-zA-Z]+\s*/g, ' ')
    .replace(/[{}$\\~^_&#]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** The document's own heading, or `null` where it has none this file can read. */
export function documentTitle(source: string): string | null {
  const match = TITLE_PATTERN.exec(source);
  const cleaned = match?.[1] ? words(match[1]) : '';
  return cleaned === '' ? null : cleaned;
}

/** The first line with anything on it, cleaned the same way — the second-best name. */
export function firstLine(source: string): string | null {
  for (const line of source.split('\n', 200)) {
    // A comment is still a line somebody wrote, so it counts; the `%` does not.
    const cleaned = words(line.replace(/^\s*%+/, ''));
    if (cleaned !== '') return cleaned;
  }
  return null;
}

/** Cut to {@link MAX_TITLE}, at a space where there is one near the end. */
export function shorten(text: string, max = MAX_TITLE): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const space = cut.lastIndexOf(' ');
  return `${(space > max * 0.6 ? cut.slice(0, space) : cut).trimEnd()}…`;
}

/** A file name: lower case, words joined by hyphens, nothing exotic left in it. */
export function slugify(raw: string): string {
  return raw
    .replace(/\\[a-zA-Z]+\s*/g, ' ')
    .replace(/[{}$\\]/g, ' ')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+/, '')
    .slice(0, 48)
    .replace(/-+$/, '');
}

/** What Download calls the converted text (§6.3). */
export function downloadName(source: string): string {
  const title = documentTitle(source);
  const slug = title ? slugify(title) : '';
  return slug ? `${slug}.txt` : 'converted.txt';
}

/** What Download calls one library entry's LaTeX source. */
export function sourceFileName(title: string): string {
  const slug = slugify(title);
  return slug ? `${slug}.tex` : 'document.tex';
}
