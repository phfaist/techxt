/**
 * When the chip row fires, what it offers, and where Tab goes next (web/PLAN.md §6.13).
 *
 * The row itself is elements and a keyboard handler in `ui/panes.ts`; the rules it is
 * judged by are here, because they are rules about strings and indices. Three of them
 * are worth stating as the reason this file exists at all: the row never fires in the
 * middle of a word, the two names the binding cannot offer are folded in without
 * disturbing the order it did give, and the Tab cycle comes back to the user's own text
 * at *both* ends.
 */

import { describe, expect, it } from 'vitest';

import { candidatesFor, completionTrigger, nextInCycle, withLiterals } from '../src/completion';
import type { Completion } from '../src/worker/protocol';

/** A binding answer, spelled the short way. */
function entry(
  name: string,
  extra: Partial<Completion> = {},
): Completion {
  return {
    name,
    kind: 'macro',
    replacement: null,
    arity: 0,
    fromDocument: false,
    ...extra,
  };
}

/** Where the caret is, written as a `|` in the text — the way a case reads best. */
function at(spelled: string): { text: string; caret: number } {
  const caret = spelled.indexOf('|');
  return { text: spelled.slice(0, caret) + spelled.slice(caret + 1), caret };
}

function triggerFor(spelled: string) {
  const { text, caret } = at(spelled);
  return completionTrigger(text, caret, caret);
}

describe('completionTrigger', () => {
  it('fires on a backslash and at least one letter', () => {
    expect(triggerFor('A state \\alp|')).toEqual({
      kind: 'macro',
      prefix: 'alp',
      start: 9,
      end: 12,
    });
  });

  it('never fires on a backslash alone', () => {
    // A row that appeared on every escape character would be a row that is always open.
    expect(triggerFor('A state \\|')).toBeNull();
  });

  it('never fires in the middle of a word', () => {
    // The caret sits inside `\alpha`: the name is being edited, not written, and a row
    // under the cursor there is an interruption.
    expect(triggerFor('\\al|pha')).toBeNull();
  });

  it('fires at the end of a name that something else follows', () => {
    expect(triggerFor('\\alp| + b')?.prefix).toBe('alp');
    expect(triggerFor('{\\emp|}')?.prefix).toBe('emp');
  });

  it('does not fire after an escaped backslash', () => {
    // `\\alpha` is a line break and then the word *alpha*; completing it would be
    // completing prose.
    expect(triggerFor('a \\\\alp|')).toBeNull();
    // …but a third backslash starts a control sequence again.
    expect(triggerFor('a \\\\\\alp|')?.prefix).toBe('alp');
  });

  it('does not fire on a selection, which is not a caret', () => {
    const text = '\\alpha';
    expect(completionTrigger(text, 1, 4)).toBeNull();
  });

  it('fires inside `\\begin{` on an environment name', () => {
    expect(triggerFor('\\begin{ali|')).toEqual({
      kind: 'environment',
      prefix: 'ali',
      start: 7,
      end: 10,
    });
    // An environment name may carry digits and a star, and the group may already be
    // closed — the caret is still writing the name.
    expect(triggerFor('\\begin{align*|}')?.prefix).toBe('align*');
  });

  it('does not confuse a group that is not a `\\begin` for one that is', () => {
    expect(triggerFor('\\emph{ali|')).toBeNull();
    expect(triggerFor('{ali|')).toBeNull();
  });

  it('reads `\\begin` itself as a macro prefix until the brace is typed', () => {
    expect(triggerFor('\\beg|')).toEqual({ kind: 'macro', prefix: 'beg', start: 1, end: 4 });
  });

  it('handles the ends of the buffer', () => {
    expect(completionTrigger('', 0, 0)).toBeNull();
    expect(completionTrigger('\\al', 3, 3)?.prefix).toBe('al');
    expect(completionTrigger('\\al', 99, 99)).toBeNull();
  });
});

describe('withLiterals', () => {
  it('offers `\\begin` and `\\end`, which the binding cannot', () => {
    // techxt defines neither: they are parser structure, not entries in a
    // `DefinitionSet`, so `\begi` comes back from `complete()` empty (§4.9).
    expect(withLiterals([], 'begi').map((item) => item.name)).toEqual(['begin']);
    expect(withLiterals([], 'e').map((item) => item.name)).toEqual(['end']);
    expect(withLiterals([], 'b').map((item) => item.name)).toEqual(['begin']);
  });

  it('puts them at the head, ahead of what the binding ranked', () => {
    const items = [entry('emph'), entry('em')];
    expect(withLiterals(items, 'e').map((item) => item.name)).toEqual(['end', 'emph', 'em']);
  });

  it('leaves the binding’s answer alone when neither literal matches', () => {
    const items = [entry('alpha'), entry('alph')];
    expect(withLiterals(items, 'alp')).toEqual(items);
  });

  it('keeps the binding’s first rule intact: an exact match outranks a literal', () => {
    // A name typed in full is not a request to be shown something longer, and that rule
    // is the binding's own — folding two names in must not move it (§4.9).
    const items = [entry('e'), entry('emph')];
    expect(withLiterals(items, 'e').map((item) => item.name)).toEqual(['e', 'end', 'emph']);
  });

  it('applies that same rule to a literal that is itself the exact match', () => {
    expect(withLiterals([entry('endinput')], 'end').map((item) => item.name)).toEqual([
      'end',
      'endinput',
    ]);
  });

  it('never lists a name twice, whichever source it came from', () => {
    // If techxt ever does define `\begin`, the row shows one chip and not two.
    const names = withLiterals([entry('begin'), entry('beginner')], 'begi').map((i) => i.name);
    expect(names).toEqual(['begin', 'beginner']);
  });
});

describe('candidatesFor', () => {
  it('caps the row, which is also the length of the cycle', () => {
    const items = ['a1', 'a2', 'a3', 'a4', 'a5', 'a6'].map((name) => entry(name));
    expect(candidatesFor(items, 'macro', 'a', 3)).toHaveLength(3);
    expect(candidatesFor(items, 'macro', 'a', 0)).toHaveLength(0);
  });

  it('keeps the environments, in the binding order, for an environment trigger', () => {
    // `complete()` takes no kind and ranks macros first, so the app asks for a long
    // answer and keeps the entries the trigger is about — a filter, never a re-sort.
    const items = [
      entry('align'), // a macro that happens to share the name
      entry('alignment'),
      entry('align', { kind: 'environment' }),
      entry('aligned', { kind: 'environment' }),
      entry('alignat', { kind: 'environment' }),
    ];
    expect(candidatesFor(items, 'environment', 'ali', 5).map((item) => item.name)).toEqual([
      'align',
      'aligned',
      'alignat',
    ]);
    expect(candidatesFor(items, 'environment', 'ali', 5).every((i) => i.kind === 'environment')).toBe(
      true,
    );
  });

  it('does not offer the two literals inside `\\begin{`, where they are not names', () => {
    expect(candidatesFor([], 'environment', 'e', 5)).toEqual([]);
  });

  it('renders the binding order for a macro trigger, whatever it is', () => {
    // The document's own name ranks where the binding put it; the app does not re-rank.
    const items = [entry('ket'), entry('ketstate', { fromDocument: true }), entry('ketbra')];
    expect(candidatesFor(items, 'macro', 'ket', 5).map((item) => item.name)).toEqual([
      'ket',
      'ketstate',
      'ketbra',
    ]);
  });
});

describe('nextInCycle', () => {
  it('walks the candidates from the user’s text and back to it', () => {
    expect(nextInCycle(null, 3, 1)).toBe(0);
    expect(nextInCycle(0, 3, 1)).toBe(1);
    expect(nextInCycle(1, 3, 1)).toBe(2);
    // Past the last candidate is the user's own text again, not the first candidate.
    expect(nextInCycle(2, 3, 1)).toBeNull();
  });

  it('steps back through them, and restores what was typed from the first', () => {
    // The undo half: someone who pressed Tab by accident gets their `\alp` back.
    expect(nextInCycle(0, 3, -1)).toBeNull();
    expect(nextInCycle(2, 3, -1)).toBe(1);
    expect(nextInCycle(null, 3, -1)).toBe(2);
  });

  it('comes round in both directions', () => {
    const ring: Array<number | null> = [];
    let index: number | null = null;
    for (let i = 0; i < 8; i += 1) {
      index = nextInCycle(index, 3, 1);
      ring.push(index);
    }
    expect(ring).toEqual([0, 1, 2, null, 0, 1, 2, null]);
  });

  it('has nowhere to go with no candidates', () => {
    expect(nextInCycle(null, 0, 1)).toBeNull();
    expect(nextInCycle(null, 0, -1)).toBeNull();
  });
});
