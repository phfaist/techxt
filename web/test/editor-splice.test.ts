/**
 * The mirror's incremental rebuild (web/PLAN.md §6.12; TODO item 7).
 *
 * `ui/panes.ts` keeps a record of the runs the mirror is holding and, on every repaint,
 * asks `chunkSplice` what changed. Two properties make that safe, and both are asserted
 * here on every case: **applying the splice to the old list gives the new list exactly**,
 * so the mirror's nodes never fall out of step with the record of them; and **a keystroke
 * produces a splice of one run**, which is the whole reason the rebuild is incremental
 * rather than a replacement.
 *
 * The second is a fact about the *window* as much as about the diff, and the case at the
 * bottom is where that shows: a window whose offsets stay put while the text under them
 * moves changes the hundred-kilobyte run at its far edge on every keystroke, and then the
 * splice is everything from the caret to the end of the document. Carrying the window
 * along with the edit is what makes it one run, and the two cases sit side by side so
 * that the next person to be tempted by the simpler version can see what it costs.
 */

import { describe, expect, it } from 'vitest';

import { chunkSplice, editorChunks, textEdit, tokenize } from '../src/highlight';
import type { EditorChunk } from '../src/highlight';

/** A run, spelled as briefly as a case needs. */
function run(text: string, token: EditorChunk['token'] = null): EditorChunk {
  return { text, token, inMath: false, severity: null };
}

/** What `ui/panes.ts` does with the answer: the DOM operation, on an array. */
function apply(before: readonly EditorChunk[], splice: ReturnType<typeof chunkSplice>): EditorChunk[] {
  const out = [...before];
  if (splice === null) return out;
  out.splice(splice.at, splice.removed, ...splice.inserted);
  return out;
}

describe('textEdit', () => {
  it('reads an insertion as a zero-width range at the caret', () => {
    expect(textEdit('abcdef', 'abcXdef')).toEqual({ prefix: 3, oldEnd: 3, delta: 1 });
  });

  it('reads a deletion as the range that went', () => {
    expect(textEdit('abcXdef', 'abcdef')).toEqual({ prefix: 3, oldEnd: 4, delta: -1 });
  });

  it('reads a replacement as the range it replaced', () => {
    expect(textEdit('abcXYZdef', 'abcQdef')).toEqual({ prefix: 3, oldEnd: 6, delta: -2 });
  });

  it('gives a zero-width edit at the end when nothing changed', () => {
    expect(textEdit('abc', 'abc')).toEqual({ prefix: 3, oldEnd: 3, delta: 0 });
  });

  it('does not let the common prefix and the common suffix claim the same character', () => {
    // `aa` → `aaa` is the case that catches an unbounded suffix scan: the prefix has
    // already taken both `a`s, so the suffix may take none, and the edit is one
    // character at the end rather than a negative-width range in the middle.
    const edit = textEdit('aa', 'aaa');
    expect(edit.prefix).toBe(2);
    expect(edit.oldEnd).toBeGreaterThanOrEqual(edit.prefix);
    expect(edit).toEqual({ prefix: 2, oldEnd: 2, delta: 1 });
  });

  it('handles an edit at the very start and one at the very end', () => {
    expect(textEdit('bcd', 'abcd')).toEqual({ prefix: 0, oldEnd: 0, delta: 1 });
    expect(textEdit('abc', 'abcd')).toEqual({ prefix: 3, oldEnd: 3, delta: 1 });
  });

  it('handles an empty side', () => {
    expect(textEdit('', 'abc')).toEqual({ prefix: 0, oldEnd: 0, delta: 3 });
    expect(textEdit('abc', '')).toEqual({ prefix: 0, oldEnd: 3, delta: -3 });
  });
});

describe('chunkSplice', () => {
  it('says nothing to do when the two lists are the same', () => {
    const runs = [run('a'), run('\\emph', 'command'), run('b')];
    expect(chunkSplice(runs, [...runs])).toBeNull();
  });

  it('finds one run when one run changed', () => {
    const before = [run('a'), run('bbb'), run('c')];
    const after = [run('a'), run('bXbb'), run('c')];
    const splice = chunkSplice(before, after);
    expect(splice).toEqual({ at: 1, removed: 1, inserted: [run('bXbb')] });
    expect(apply(before, splice)).toEqual(after);
  });

  it('notices a run whose text is the same but whose colour is not', () => {
    const before = [run('a'), run('$', 'delimiter')];
    const after = [run('a'), { text: '$', token: 'delimiter' as const, inMath: true, severity: null }];
    const splice = chunkSplice(before, after);
    expect(splice?.removed).toBe(1);
    expect(apply(before, splice)).toEqual(after);
  });

  it('notices a diagnostic arriving over text that did not change', () => {
    const before = [run('a'), run('bad')];
    const after = [run('a'), { text: 'bad', token: null, inMath: false, severity: 'error' as const }];
    expect(apply(before, chunkSplice(before, after))).toEqual(after);
  });

  it('inserts without removing, and removes without inserting', () => {
    const grow = chunkSplice([run('a'), run('c')], [run('a'), run('b'), run('c')]);
    expect(grow).toEqual({ at: 1, removed: 0, inserted: [run('b')] });
    const shrink = chunkSplice([run('a'), run('b'), run('c')], [run('a'), run('c')]);
    expect(shrink).toEqual({ at: 1, removed: 1, inserted: [] });
  });

  it('does not let the head and the tail claim the same run', () => {
    // Two runs of the same text: the head matches one and the tail would match it
    // again, and a splice that counted it twice would delete text that is still wanted.
    const before = [run('a')];
    const after = [run('a'), run('a')];
    const splice = chunkSplice(before, after);
    expect(apply(before, splice)).toEqual(after);
    expect(splice?.removed).toBe(0);
  });

  it('replaces everything when everything changed, and copes with an empty side', () => {
    expect(apply([run('a')], chunkSplice([run('a')], [run('z')]))).toEqual([run('z')]);
    expect(apply([], chunkSplice([], [run('z')]))).toEqual([run('z')]);
    expect(apply([run('a')], chunkSplice([run('a')], []))).toEqual([]);
  });
});

/**
 * The mirror's own arithmetic, without a DOM: how `ui/panes.ts` cuts a windowed document
 * into runs, and what a keystroke does to that list.
 */
function paint(text: string, from: number, to: number): EditorChunk[] {
  const runs: EditorChunk[] = [];
  for (const cut of [
    { from: 0, to: from, tokens: [] as ReturnType<typeof tokenize> },
    { from, to, tokens: tokenize(text, from, to) },
    { from: to, to: text.length, tokens: [] as ReturnType<typeof tokenize> },
  ]) {
    if (cut.to <= cut.from) continue;
    runs.push(...editorChunks(text, cut.tokens, [], cut.from, cut.to));
  }
  return runs;
}

describe('a keystroke in a windowed document', () => {
  const unit = 'Some \\emph{prose} with $x^2$ in it, and a \\footnote{note}.\n';
  const text = unit.repeat(1_200); // ~68 KB, far past the whole-document limit
  const from = 30_000;
  const to = 36_000;
  const at = 33_000; // the caret, inside the window
  const typed = `${text.slice(0, at)}Z${text.slice(at)}`;

  it('changes one run when the window is carried along by the edit', () => {
    const edit = textEdit(text, typed);
    expect(edit.delta).toBe(1);
    const before = paint(text, from, to);
    const after = paint(typed, from, to + edit.delta);
    const splice = chunkSplice(before, after);
    expect(splice).not.toBeNull();
    expect(splice?.removed).toBe(1);
    expect(splice?.inserted).toHaveLength(1);
    expect(apply(before, splice)).toEqual(after);
    // And the runs still tile the document exactly, which is the invariant the whole
    // overlay rests on: the mirror holds every character, whatever was spliced.
    expect(after.map((r) => r.text).join('')).toBe(typed);
  });

  it('changes everything after the caret when the window is left where it was', () => {
    // The version this design was tempting: hold the window at fixed offsets and let the
    // text slide under it. The run past the window then begins one character earlier
    // than it did, so the tail of the list matches nothing and the splice is the rest of
    // the document — several hundred nodes, on every keystroke.
    const before = paint(text, from, to);
    const after = paint(typed, from, to);
    const splice = chunkSplice(before, after);
    expect(splice?.removed).toBeGreaterThan(100);
    expect(apply(before, splice)).toEqual(after);
  });
});
