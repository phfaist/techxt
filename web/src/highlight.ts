/**
 * The editor's lexer, and the chunking that puts its answer on the screen
 * (web/PLAN.md §6.12).
 *
 * This is a **lexer, not a LaTeX grammar**. It knows five things — a comment, a control
 * sequence, a brace, a math delimiter and the environment name in a `\begin{…}` — and it
 * knows them from the characters alone. It does not resolve a macro, does not know which
 * `\end` closes which `\begin`, and does not know that the body of a `verbatim` is not
 * markup. That is the deal the highlighting is on: a dumb synchronous pass repaints with
 * the character, where anything that had to ask the parser would trail the cursor by the
 * conversion's debounce and its round trip through the worker (§6.12).
 *
 * The DOM half lives in `ui/panes.ts`; everything that decides *what* is coloured is here,
 * and so is everything that decides *what changed* — {@link textEdit} reads one edit as the
 * single range it replaced, and {@link chunkSplice} turns two paintings into the one splice
 * between them, which is what keeps a keystroke from rebuilding the mirror whole (§6.12).
 * Both are here because vitest runs in `node` with no DOM, and this is the half worth
 * testing.
 *
 * The one invariant, and the reason for every clamp below: **the chunks concatenate back
 * to the text they were cut from**. The mirror they are rendered into sits behind the
 * real `<textarea>` and every character in it has to land on the character above it, so
 * a chunking that added, dropped or reordered so much as a space would show up as text
 * that drifts out of alignment as you read down the pane.
 */

/** What a token is, which is also the only vocabulary the stylesheet has. */
export type TokenKind =
  /** `% …` to the end of the line. */
  | 'comment'
  /** A control sequence: `\alpha`, `\\`, `\%`. */
  | 'command'
  /** `{` or `}`. */
  | 'brace'
  /** `$`, `$$`, `\(`, `\)`, `\[`, `\]` — the delimiters themselves. */
  | 'delimiter'
  /** The name in `\begin{…}` or `\end{…}`. */
  | 'environment'
  /** A run of ordinary characters inside math, which is what tints a formula. */
  | 'math';

/** One lexed run of the source. Tokens are in order and never overlap. */
export interface Token {
  start: number;
  end: number;
  kind: TokenKind;
  /** Whether this token is inside `$…$`, `\(…\)`, `$$…$$` or `\[…\]`. */
  inMath: boolean;
}

/** An error or warning span to underline, as `ui/panes.ts` holds them. */
export interface Mark {
  start: number;
  end: number;
  severity: 'error' | 'warning';
}

/**
 * One run of the mirror: a slice of the text, what the lexer made of it, and whether a
 * diagnostic covers it.
 *
 * The two channels are deliberately independent (§6.12, §7): the token decides the
 * colour of the glyphs, the mark decides the tint behind them and the underline under
 * them, and a chunk can carry one, both or neither. They are cut together rather than
 * nested so that the mirror stays one flat list of spans — nesting would have to answer
 * what a diagnostic that starts inside a formula and ends outside it looks like, and the
 * answer would be a tree for no gain in what a reader sees.
 */
export interface EditorChunk {
  text: string;
  /** `null` for ordinary prose, which is the majority of most documents. */
  token: TokenKind | null;
  inMath: boolean;
  severity: 'error' | 'warning' | null;
}

const BACKSLASH = 0x5c;
const PERCENT = 0x25;
const DOLLAR = 0x24;
const OPEN_BRACE = 0x7b;
const CLOSE_BRACE = 0x7d;
const OPEN_PAREN = 0x28;
const CLOSE_PAREN = 0x29;
const OPEN_BRACKET = 0x5b;
const CLOSE_BRACKET = 0x5d;
const NEWLINE = 0x0a;
const STAR = 0x2a;

function isLetter(code: number): boolean {
  return (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a);
}

function isDigit(code: number): boolean {
  return code >= 0x30 && code <= 0x39;
}

/** What may appear in an environment name: `align`, `align*`, `figure*`, `enumerate2`. */
function isEnvironmentChar(code: number): boolean {
  return isLetter(code) || isDigit(code) || code === STAR;
}

/** Which delimiter opened the formula being read, so the right one can close it. */
type MathOpener = '$' | '$$' | '\\(' | '\\[';

/**
 * Lex `text`, reporting only the tokens that touch `[from, to)`.
 *
 * **The scan always starts at the beginning of the document**, whatever the window is,
 * because whether a given `$` opens or closes a formula is a fact about everything before
 * it. Only the *reporting* is windowed: a caller highlighting one screenful of a 200 KB
 * buffer pays a pass over the characters, which is a few milliseconds, and not tens of
 * thousands of objects and DOM nodes it would immediately throw away (§6.12). A token
 * that straddles a window edge is clipped to it, so the chunks still tile the window
 * exactly.
 *
 * Unterminated constructs are not errors here: a `$` with no partner runs to the end of
 * the document, which is what the character *says*, and the conversion's own diagnostics
 * are where a reader is told it is wrong.
 */
export function tokenize(text: string, from = 0, to = text.length): Token[] {
  const tokens: Token[] = [];
  const length = text.length;
  const windowStart = Math.max(0, Math.min(from, length));
  const windowEnd = Math.max(windowStart, Math.min(to, length));

  let math: MathOpener | null = null;
  let at = 0;

  /** Clip to the window and drop what falls outside it — the whole of the windowing. */
  const emit = (start: number, end: number, kind: TokenKind, inMath: boolean): void => {
    if (end <= windowStart || start >= windowEnd) return;
    tokens.push({
      start: Math.max(start, windowStart),
      end: Math.min(end, windowEnd),
      kind,
      inMath,
    });
  };

  while (at < length) {
    const code = text.charCodeAt(at);

    // Past the window and outside math, nothing can still be emitted: the scan only
    // continues at all to keep the math state honest, and once even that cannot matter
    // any more there is nothing left to learn.
    if (at >= windowEnd && math === null) break;

    if (code === PERCENT) {
      // A comment runs to the end of the line, and the newline is not part of it.
      let end = at + 1;
      while (end < length && text.charCodeAt(end) !== NEWLINE) end += 1;
      emit(at, end, 'comment', math !== null);
      at = end;
      continue;
    }

    if (code === BACKSLASH) {
      const next = at + 1 < length ? text.charCodeAt(at + 1) : -1;

      // `\(`, `\)`, `\[`, `\]` — the delimiters that are control symbols.
      if (math === null && (next === OPEN_PAREN || next === OPEN_BRACKET)) {
        emit(at, at + 2, 'delimiter', false);
        math = next === OPEN_PAREN ? '\\(' : '\\[';
        at += 2;
        continue;
      }
      if (
        (math === '\\(' && next === CLOSE_PAREN) ||
        (math === '\\[' && next === CLOSE_BRACKET)
      ) {
        emit(at, at + 2, 'delimiter', true);
        math = null;
        at += 2;
        continue;
      }

      if (next >= 0 && isLetter(next)) {
        let end = at + 1;
        while (end < length && isLetter(text.charCodeAt(end))) end += 1;
        const name = text.slice(at + 1, end);
        emit(at, end, 'command', math !== null);
        at = end;
        // `\begin{align*}` and its partner: the name is the environment, not an
        // argument like any other, and it is the one place this lexer looks past the
        // control sequence it just read.
        if (name === 'begin' || name === 'end') at = readEnvironmentName(text, at, emit, math !== null);
        continue;
      }

      // A control symbol: `\\`, `\%`, `\$`, `\{`. One character, whatever it is — which
      // is what keeps an escaped delimiter from opening a formula.
      emit(at, Math.min(at + 2, length), 'command', math !== null);
      at = Math.min(at + 2, length);
      continue;
    }

    if (code === DOLLAR) {
      const double = at + 1 < length && text.charCodeAt(at + 1) === DOLLAR;
      if (math === null) {
        emit(at, at + (double ? 2 : 1), 'delimiter', false);
        math = double ? '$$' : '$';
        at += double ? 2 : 1;
        continue;
      }
      if (math === '$$' && double) {
        emit(at, at + 2, 'delimiter', true);
        math = null;
        at += 2;
        continue;
      }
      if (math === '$') {
        emit(at, at + 1, 'delimiter', true);
        math = null;
        at += 1;
        continue;
      }
      // A single `$` inside `$$…$$`, or a `$$` inside `$…$`: not a close, so it is read
      // as part of the formula rather than guessed at.
      emit(at, at + 1, 'math', true);
      at += 1;
      continue;
    }

    if (code === OPEN_BRACE || code === CLOSE_BRACE) {
      emit(at, at + 1, 'brace', math !== null);
      at += 1;
      continue;
    }

    // Ordinary characters. Inside math they are the formula and get the tint; outside it
    // they are prose and get no token at all, which is what keeps the common case cheap.
    let end = at;
    while (end < length) {
      const run = text.charCodeAt(end);
      if (
        run === PERCENT ||
        run === BACKSLASH ||
        run === DOLLAR ||
        run === OPEN_BRACE ||
        run === CLOSE_BRACE
      ) {
        break;
      }
      end += 1;
    }
    if (math !== null) emit(at, end, 'math', true);
    at = end;
  }

  return tokens;
}

/**
 * After a `\begin` or an `\end`: the braces and the name between them, if that is what
 * follows. Returns where the scan should carry on from.
 *
 * Whitespace is allowed before the brace because LaTeX allows it, and nothing else is:
 * `\begin` followed by something that is not a group is left alone rather than guessed
 * at, and the ordinary rules pick the characters up again.
 */
function readEnvironmentName(
  text: string,
  at: number,
  emit: (start: number, end: number, kind: TokenKind, inMath: boolean) => void,
  inMath: boolean,
): number {
  let cursor = at;
  while (cursor < text.length) {
    const code = text.charCodeAt(cursor);
    if (code === 0x20 || code === 0x09 || code === NEWLINE || code === 0x0d) cursor += 1;
    else break;
  }
  if (cursor >= text.length || text.charCodeAt(cursor) !== OPEN_BRACE) return at;
  const open = cursor;
  let end = open + 1;
  while (end < text.length && isEnvironmentChar(text.charCodeAt(end))) end += 1;
  if (end >= text.length || text.charCodeAt(end) !== CLOSE_BRACE) return at;
  emit(open, open + 1, 'brace', inMath);
  if (end > open + 1) emit(open + 1, end, 'environment', inMath);
  emit(end, end + 1, 'brace', inMath);
  return end + 1;
}

/**
 * Cut `[from, to)` of `text` into the runs the mirror renders, one span per run that
 * carries anything and a bare text node for the rest.
 *
 * `tokens` are what {@link tokenize} reported for the same window; `marks` are the
 * diagnostics, which are *not* windowed — there are a handful of them and clamping is
 * cheaper than reasoning about which ones are on screen. Where two marks overlap the
 * worse severity wins, which is the rule the panel's own colours follow (§7).
 *
 * The runs tile `[from, to)` exactly and in order, so joining their text gives the slice
 * back. Callers render the text before `from` and after `to` as plain nodes.
 */
export function editorChunks(
  text: string,
  tokens: readonly Token[],
  marks: readonly Mark[],
  from = 0,
  to = text.length,
): EditorChunk[] {
  const start = Math.max(0, Math.min(from, text.length));
  const end = Math.max(start, Math.min(to, text.length));
  if (end === start) return [];

  const clipped = marks
    .map((mark) => ({
      start: Math.max(mark.start, start),
      end: Math.min(mark.end, end),
      rank: mark.severity === 'error' ? 2 : 1,
    }))
    .filter((mark) => mark.end > mark.start);

  const bounds = new Set<number>([start, end]);
  for (const token of tokens) {
    if (token.end <= start || token.start >= end) continue;
    bounds.add(Math.max(token.start, start));
    bounds.add(Math.min(token.end, end));
  }
  for (const mark of clipped) {
    bounds.add(mark.start);
    bounds.add(mark.end);
  }
  const edges = Array.from(bounds).sort((a, b) => a - b);

  const chunks: EditorChunk[] = [];
  // The tokens are in order and disjoint, so one moving index walks them alongside the
  // boundaries rather than a search per chunk.
  let cursor = 0;
  for (let i = 0; i < edges.length - 1; i += 1) {
    const left = edges[i];
    const right = edges[i + 1];
    if (left === undefined || right === undefined || right <= left) continue;
    while (cursor < tokens.length) {
      const token = tokens[cursor];
      if (token === undefined || token.end > left) break;
      cursor += 1;
    }
    const token = tokens[cursor];
    const covering = token !== undefined && token.start <= left && token.end >= right ? token : null;
    let rank = 0;
    for (const mark of clipped) {
      if (mark.start <= left && mark.end >= right) rank = Math.max(rank, mark.rank);
    }
    chunks.push({
      text: text.slice(left, right),
      token: covering ? covering.kind : null,
      inMath: covering ? covering.inMath : false,
      severity: rank === 2 ? 'error' : rank === 1 ? 'warning' : null,
    });
  }
  return chunks;
}

/**
 * What one edit did to the text, as the single replaced range it can always be read as.
 *
 * `prefix` is where the two versions first differ, `oldEnd` is where the replaced range
 * ends in the *old* text, and `delta` is how much longer the text got. An offset at or
 * after `oldEnd` moves by `delta`; an offset at or before `prefix` does not move at all;
 * an offset between them was inside what the edit replaced and has no answer.
 *
 * It is the smallest description of a keystroke that both of the things that track one
 * need: the diagnostics' cached spans, which have to stay on their own characters until
 * the next conversion result arrives (§7), and the highlight window, which has to stay
 * over the same characters so that the mirror's rebuild can be a splice rather than a
 * replacement (§6.12).
 */
export interface TextEdit {
  prefix: number;
  oldEnd: number;
  delta: number;
}

/** See {@link TextEdit}. Two identical strings give a zero-width edit at their end. */
export function textEdit(before: string, after: string): TextEdit {
  const maxCommon = Math.min(before.length, after.length);
  let prefix = 0;
  while (prefix < maxCommon && before.charCodeAt(prefix) === after.charCodeAt(prefix)) prefix += 1;
  let suffix = 0;
  const maxSuffix = maxCommon - prefix;
  while (
    suffix < maxSuffix &&
    before.charCodeAt(before.length - 1 - suffix) === after.charCodeAt(after.length - 1 - suffix)
  ) {
    suffix += 1;
  }
  return { prefix, oldEnd: before.length - suffix, delta: after.length - before.length };
}

/**
 * The one contiguous stretch of runs in which two chunk lists differ — the whole of the
 * mirror's incremental rebuild (§6.12).
 *
 * A keystroke changes one run of the document and leaves every other run exactly as it
 * was, so the list the mirror should be holding shares a long head and a long tail with
 * the list it is holding. This finds both and reports the gap between them as a splice:
 * drop `removed` runs starting at `at`, put `inserted` in their place. `null` means the
 * two lists are the same and the mirror does not have to be touched at all.
 *
 * It is deliberately a *single* splice rather than a minimal edit script. Two runs of the
 * same text are interchangeable — a chunk carries no identity beyond what it says — so a
 * cleverer diff would spend its time proving that the middle of a paragraph can be
 * reused, on a list where the middle is exactly what changed. A head-and-tail scan is
 * O(n) in the runs and, on the edit a keystroke actually makes, finds a splice of one.
 */
export interface ChunkSplice {
  /** Index of the first run that differs. */
  at: number;
  /** How many runs to drop, starting at {@link at}. */
  removed: number;
  /** What to put in their place. */
  inserted: EditorChunk[];
}

function sameChunk(a: EditorChunk, b: EditorChunk): boolean {
  return (
    a.text === b.text && a.token === b.token && a.inMath === b.inMath && a.severity === b.severity
  );
}

/** See {@link ChunkSplice}. `null` when the two lists are identical. */
export function chunkSplice(
  before: readonly EditorChunk[],
  after: readonly EditorChunk[],
): ChunkSplice | null {
  const shorter = Math.min(before.length, after.length);
  let head = 0;
  while (head < shorter) {
    const a = before[head];
    const b = after[head];
    if (a === undefined || b === undefined || !sameChunk(a, b)) break;
    head += 1;
  }
  let tail = 0;
  // The head and the tail may not claim the same run twice, or a splice would delete
  // text that is still wanted.
  const maxTail = shorter - head;
  while (tail < maxTail) {
    const a = before[before.length - 1 - tail];
    const b = after[after.length - 1 - tail];
    if (a === undefined || b === undefined || !sameChunk(a, b)) break;
    tail += 1;
  }
  const removed = before.length - head - tail;
  const inserted = after.slice(head, after.length - tail);
  if (removed === 0 && inserted.length === 0) return null;
  return { at: head, removed, inserted };
}
