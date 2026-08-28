/**
 * The editor's lexer and its chunking (web/PLAN.md §6.12; TODO item 5).
 *
 * The DOM half is a loop over `editorChunks` in `ui/panes.ts`; this is the half worth
 * testing, and the property every case re-checks is the one the overlay rests on: **the
 * chunks concatenate back to the text they were cut from**. The mirror sits behind the
 * real textarea, character for character, so a chunking that added, dropped or reordered
 * anything would show up as colour that drifts away from the text it belongs to.
 */

import { describe, expect, it } from 'vitest';

import { editorChunks, tokenize } from '../src/highlight';
import type { EditorChunk, Mark, Token } from '../src/highlight';

/** A token as `kind:text`, which is what a case is actually asserting about. */
function spell(text: string, tokens: readonly Token[]): string[] {
  return tokens.map((token) => `${token.kind}${token.inMath ? '*' : ''}:${text.slice(token.start, token.end)}`);
}

function rejoin(chunks: readonly EditorChunk[]): string {
  return chunks.map((chunk) => chunk.text).join('');
}

describe('tokenize', () => {
  it('reads a control sequence, its braces and the prose between them', () => {
    const text = 'A \\emph{word} here.';
    expect(spell(text, tokenize(text))).toEqual(['command:\\emph', 'brace:{', 'brace:}']);
  });

  it('stops a control word at the first character that is not a letter', () => {
    const text = '\\alpha2 \\beta.';
    expect(spell(text, tokenize(text))).toEqual(['command:\\alpha', 'command:\\beta']);
  });

  it('reads a control symbol as exactly one character, so an escape is not a delimiter', () => {
    // `\$` is a dollar sign in the output, not the start of a formula: if this were read
    // as an opener, the whole rest of the document would be tinted as mathematics.
    const text = 'costs \\$5 and \\% too';
    expect(spell(text, tokenize(text))).toEqual(['command:\\$', 'command:\\%']);
  });

  it('reads `\\\\` as one control symbol, leaving what follows as prose', () => {
    // The word after a line break is a word: `\\\\alpha` is not a macro call, which is
    // the same reading `completionTrigger` takes when it declines to offer one there.
    const text = 'a \\\\alpha b';
    expect(spell(text, tokenize(text))).toEqual(['command:\\\\']);
  });

  it('takes a comment to the end of the line and no further', () => {
    const text = 'text % a comment \\alpha\nmore \\emph{x}';
    expect(spell(text, tokenize(text))).toEqual([
      'comment:% a comment \\alpha',
      'command:\\emph',
      'brace:{',
      'brace:}',
    ]);
  });

  it('tints a formula, its delimiters and everything inside it', () => {
    const text = 'see $a + \\alpha$ here';
    expect(spell(text, tokenize(text))).toEqual([
      'delimiter:$',
      'math*:a + ',
      'command*:\\alpha',
      'delimiter*:$',
    ]);
  });

  it('knows the four openers and closes each with its own partner', () => {
    const inline = '\\(x\\)';
    expect(spell(inline, tokenize(inline))).toEqual(['delimiter:\\(', 'math*:x', 'delimiter*:\\)']);
    const display = '\\[x\\]';
    expect(spell(display, tokenize(display))).toEqual(['delimiter:\\[', 'math*:x', 'delimiter*:\\]']);
    const double = '$$x$$';
    expect(spell(double, tokenize(double))).toEqual(['delimiter:$$', 'math*:x', 'delimiter*:$$']);
  });

  it('does not let a single `$` close a `$$` formula', () => {
    const text = '$$a $ b$$';
    const kinds = tokenize(text).map((token) => token.kind);
    expect(kinds[0]).toBe('delimiter');
    expect(kinds[kinds.length - 1]).toBe('delimiter');
    // The lone `$` in the middle is part of the formula, not the end of it.
    expect(tokenize(text).filter((t) => t.kind === 'delimiter')).toHaveLength(2);
  });

  it('runs an unterminated formula to the end rather than guessing', () => {
    const text = 'open $a + b and then nothing';
    const tokens = tokenize(text);
    expect(tokens[tokens.length - 1]?.end).toBe(text.length);
    expect(tokens.every((token, i) => i === 0 || token.inMath)).toBe(true);
  });

  it('names the environment in `\\begin{…}` and in `\\end{…}`', () => {
    const text = '\\begin{align*}\nx\n\\end{align*}';
    expect(spell(text, tokenize(text))).toEqual([
      'command:\\begin',
      'brace:{',
      'environment:align*',
      'brace:}',
      'command:\\end',
      'brace:{',
      'environment:align*',
      'brace:}',
    ]);
  });

  it('leaves a `\\begin` that is not followed by a group alone', () => {
    const text = '\\begin and then';
    expect(spell(text, tokenize(text))).toEqual(['command:\\begin']);
  });

  it('reports only the tokens that touch the window, clipped to it', () => {
    const text = 'aaa \\alpha bbb \\beta ccc';
    const from = text.indexOf('bbb');
    const to = text.indexOf('ccc');
    expect(spell(text, tokenize(text, from, to))).toEqual(['command:\\beta']);
    // A token straddling the edge is cut at it, so the window is still tiled exactly.
    const inside = text.indexOf('lpha');
    expect(spell(text, tokenize(text, inside, to))).toEqual(['command:lpha', 'command:\\beta']);
  });

  it('lexes from the beginning whatever the window is, so math state is not guessed', () => {
    // The window starts well inside a formula opened long before it: the run has to be
    // reported as mathematics, which is only knowable from the `$` above.
    const text = `$${'x'.repeat(50)}\\alpha${'y'.repeat(50)}$`;
    const at = text.indexOf('\\alpha');
    const tokens = tokenize(text, at, at + 6);
    expect(tokens).toHaveLength(1);
    expect(tokens[0]?.inMath).toBe(true);
  });
});

describe('editorChunks', () => {
  const marks = (...spans: Array<[number, number, 'error' | 'warning']>): Mark[] =>
    spans.map(([start, end, severity]) => ({ start, end, severity }));

  it('gives back the text it was handed, whatever it did to it', () => {
    const text = '% c\n\\emph{a} $x$ \\begin{align}y\\end{align}\n';
    const chunks = editorChunks(text, tokenize(text), marks([2, 9, 'error']));
    expect(rejoin(chunks)).toBe(text);
  });

  it('carries the two channels independently, and both at once where they overlap', () => {
    const text = 'a \\emph b';
    const chunks = editorChunks(text, tokenize(text), marks([2, 7, 'warning']));
    const command = chunks.find((chunk) => chunk.token === 'command');
    expect(command?.text).toBe('\\emph');
    expect(command?.severity).toBe('warning');
    expect(rejoin(chunks)).toBe(text);
  });

  it('splits a token where a diagnostic starts inside it', () => {
    const text = '\\emphasis';
    const chunks = editorChunks(text, tokenize(text), marks([0, 3, 'error']));
    expect(chunks.map((chunk) => [chunk.text, chunk.token, chunk.severity])).toEqual([
      ['\\em', 'command', 'error'],
      ['phasis', 'command', null],
    ]);
    expect(rejoin(chunks)).toBe(text);
  });

  it('lets the worse severity win where two diagnostics overlap', () => {
    const text = 'abcdef';
    const chunks = editorChunks(text, [], marks([0, 4, 'warning'], [2, 6, 'error']));
    expect(chunks.map((chunk) => [chunk.text, chunk.severity])).toEqual([
      ['ab', 'warning'],
      ['cd', 'error'],
      ['ef', 'error'],
    ]);
  });

  it('clamps a diagnostic that runs past the text rather than dropping the text', () => {
    const text = 'short';
    const chunks = editorChunks(text, [], marks([2, 999, 'error']));
    expect(rejoin(chunks)).toBe(text);
    expect(chunks.map((chunk) => chunk.severity)).toEqual([null, 'error']);
  });

  it('tiles the window exactly and nothing outside it', () => {
    const text = 'aaaa\\emph bbbb';
    const chunks = editorChunks(text, tokenize(text, 4, 9), [], 4, 9);
    expect(rejoin(chunks)).toBe(text.slice(4, 9));
  });

  it('is empty for an empty window and for empty text', () => {
    expect(editorChunks('', [], [])).toEqual([]);
    expect(editorChunks('abc', [], [], 2, 2)).toEqual([]);
  });
});
