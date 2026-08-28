/**
 * When the completion row fires, what it offers, and where the Tab cycle goes next
 * (web/PLAN.md §6.13).
 *
 * Everything here is a pure function of the buffer and the caret, for the usual reason:
 * vitest runs in `node` with no DOM, and the rules a chip row is judged by — that it
 * never fires in the middle of a word, that Tab walks its own frozen list and comes back
 * to what the user typed at both ends — are rules about strings and indices. `ui/panes.ts`
 * owns the elements and the keyboard; this owns the decisions.
 *
 * **What is deliberately *not* here: matching, merging and ranking.** The binding answers
 * with one list, already merged from both its sources and already in the order the chips
 * appear in (§4.9), and the app renders it in that order. The two exceptions below are
 * exceptions to that rule and are named as such, because each one exists only where the
 * binding cannot answer at all: `\begin` and `\end` are not definitions techxt has, and
 * `complete()` has no way to be asked for environments alone.
 */

import type { Completion } from './worker/protocol';

/** Which of the two triggers fired: `\al…` or `\begin{al…`. */
export type TriggerKind = 'macro' | 'environment';

/** A live trigger: what was typed, and the range of the buffer the name occupies. */
export interface CompletionTrigger {
  kind: TriggerKind;
  /** What has been typed after the `\` or the `{`, without either. */
  prefix: string;
  /** Where the name starts — just after the `\` or the `{`. */
  start: number;
  /** Where it ends, which is the caret. */
  end: number;
}

const BACKSLASH = 0x5c;

function isLetter(code: number): boolean {
  return (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a);
}

function isEnvironmentChar(code: number): boolean {
  return isLetter(code) || (code >= 0x30 && code <= 0x39) || code === 0x2a;
}

/**
 * Whether the `\` at `at` starts a control sequence, or is itself the second half of an
 * escaped one. `\\alpha` is a line break followed by the word *alpha*, and a row that
 * offered `\alpha` there would be completing prose.
 */
function startsControlSequence(text: string, at: number): boolean {
  let before = at - 1;
  let count = 0;
  while (before >= 0 && text.charCodeAt(before) === BACKSLASH) {
    count += 1;
    before -= 1;
  }
  return count % 2 === 0;
}

/**
 * What, if anything, the caret is asking to complete.
 *
 * Two triggers, one row (§6.13). A `\` followed by **at least one letter** is the first;
 * the second is the same idea one level in, inside `\begin{`, where what follows is an
 * environment name and not a macro. Neither fires in the middle of a word — if the
 * character after the caret continues the name, the user is editing it rather than
 * writing it, and a row appearing under the cursor there is an interruption.
 *
 * A selection is not a caret and never triggers, and `\` alone never triggers: a row
 * that appeared on every escape character would be a row that is always open.
 */
export function completionTrigger(
  text: string,
  selectionStart: number,
  selectionEnd: number,
): CompletionTrigger | null {
  if (selectionStart !== selectionEnd) return null;
  const caret = selectionStart;
  if (caret < 0 || caret > text.length) return null;

  const after = caret < text.length ? text.charCodeAt(caret) : -1;

  // `\begin{ali|` — the name is letters, digits and stars, and the group is still open.
  let start = caret;
  while (start > 0 && isEnvironmentChar(text.charCodeAt(start - 1))) start -= 1;
  if (start < caret && !(after >= 0 && isEnvironmentChar(after))) {
    const opener = '\\begin{';
    const from = start - opener.length;
    if (from >= 0 && text.startsWith(opener, from) && startsControlSequence(text, from)) {
      return { kind: 'environment', prefix: text.slice(start, caret), start, end: caret };
    }
  }

  // `\al|` — a macro name is letters only, which is also what makes `\alpha2` unambiguous.
  start = caret;
  while (start > 0 && isLetter(text.charCodeAt(start - 1))) start -= 1;
  if (start === caret) return null;
  if (after >= 0 && isLetter(after)) return null;
  if (start === 0 || text.charCodeAt(start - 1) !== BACKSLASH) return null;
  if (!startsControlSequence(text, start - 1)) return null;
  return { kind: 'macro', prefix: text.slice(start, caret), start, end: caret };
}

/**
 * `\begin` and `\end`, the two names the binding cannot offer.
 *
 * They are structure the parser handles itself rather than entries in a `DefinitionSet`,
 * so `complete()` answers `\begi` with nothing at all however it is ranked — which is
 * arguably the most useful completion in LaTeX missing (§4.9). They are therefore the
 * app's own literals, and the *only* names it invents: everything else in the row still
 * comes out of the table or the document.
 */
const LITERALS: readonly Completion[] = [
  { name: 'begin', kind: 'macro', replacement: null, arity: 1, fromDocument: false },
  { name: 'end', kind: 'macro', replacement: null, arity: 1, fromDocument: false },
];

/**
 * Put the literals into the binding's answer without disturbing its order.
 *
 * They go at the head, where a curated list would have started, because `\begin` is the
 * macro a LaTeX document has most of — with one exception, which is the binding's own
 * first rule: an **exact match on what was typed** outranks everything, so `\em` typed in
 * full still leads with `\em` and `\end` follows it. Where the literal *is* the exact
 * match it leads, which is the same rule applied to it.
 */
export function withLiterals(items: readonly Completion[], prefix: string): Completion[] {
  const literals = LITERALS.filter((literal) => literal.name.startsWith(prefix));
  if (literals.length === 0) return items.slice();
  const rest = items.filter((item) => !literals.some((literal) => literal.name === item.name));
  const exactFirst = rest.length > 0 && rest[0]?.name === prefix;
  if (exactFirst && !literals.some((literal) => literal.name === prefix)) {
    return [rest[0] as Completion, ...literals, ...rest.slice(1)];
  }
  return [...literals, ...rest];
}

/**
 * The chips to show, from the answer to one query — the row as the cycle will walk it.
 *
 * For a macro trigger this is the binding's list with the two literals folded in, in the
 * order it gave. For an environment trigger it is the environments out of that same list:
 * `complete()` takes no kind, and it ranks macros above environments, so the app asks for
 * a long answer and keeps the entries the trigger is about. That is a filter and not a
 * ranking — the environments stay in the binding's own order — and it is the one thing
 * the JS side does to a list besides render it.
 */
export function candidatesFor(
  items: readonly Completion[],
  kind: TriggerKind,
  prefix: string,
  cap: number,
): Completion[] {
  const chosen =
    kind === 'environment'
      ? items.filter((item) => item.kind === 'environment')
      : withLiterals(items, prefix);
  return chosen.slice(0, Math.max(0, cap));
}

/**
 * Where the Tab cycle goes next: a ring of the candidates *and the user's own text*.
 *
 * `null` is the user's text and every other position is a candidate, so both ends of the
 * cycle come back to what was typed — Shift-Tab from the first candidate is the undo for
 * a Tab pressed by accident, and Tab past the last wraps to it rather than sticking.
 * Whichever direction you keep pressing in, your own text comes round again.
 */
export function nextInCycle(
  current: number | null,
  count: number,
  direction: 1 | -1,
): number | null {
  if (count <= 0) return null;
  if (direction === 1) {
    if (current === null) return 0;
    return current + 1 >= count ? null : current + 1;
  }
  if (current === null) return count - 1;
  return current === 0 ? null : current - 1;
}
