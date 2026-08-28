/**
 * `splitMathRuns` — the region → element mapping, minus the elements (web/PLAN.md
 * §4.3, §6.3; TODO item 2).
 *
 * The pane's own half is four lines of `createElement`; this is the half worth
 * testing, and the property every case below re-checks is the one the whole design
 * rests on: **the runs concatenate back to the text they were cut from**. If that ever
 * stops being true, the pane is showing something the library did not say.
 */

import { describe, expect, it } from 'vitest';

import { splitMathRuns } from '../src/math-regions';
import type { OutputRun } from '../src/math-regions';
import type { MathRegion } from '../src/worker/protocol';

/** What every case asserts, whatever else it asserts. */
function rejoin(runs: readonly OutputRun[]): string {
  return runs.map((run) => run.text).join('');
}

/** The math runs' text, in order — what would be handed to a typesetter. */
function formulas(runs: readonly OutputRun[]): string[] {
  return runs.filter((run) => run.math !== null).map((run) => run.text);
}

function inline(start: number, end: number): MathRegion {
  return { start, end, display: false };
}

describe('splitMathRuns', () => {
  it('cuts one inline formula out of a sentence', () => {
    // Verified fact 2 of the TODO, which is the reason a region table exists at all:
    // the two `\$` became plain dollars and nothing in the text can tell them from
    // the formula's own.
    const text = 'Support mathjax math like this $a+b-c$ but not these $3 and $4 values.';
    const runs = splitMathRuns(text, [inline(31, 38)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$a+b-c$']);
    expect(runs).toHaveLength(3);
    expect(runs[0]?.text).toBe('Support mathjax math like this ');
    expect(runs[2]?.text).toBe(' but not these $3 and $4 values.');
  });

  it('reports display and inline separately', () => {
    const text = 'before\n\n  \\[ x^2 \\]\n\nafter $y$ end';
    const runs = splitMathRuns(text, [
      { start: 10, end: 19, display: true },
      inline(27, 30),
    ]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['\\[ x^2 \\]', '$y$']);
    expect(runs.filter((run) => run.math?.display === true)).toHaveLength(1);
  });

  it('leaves the newline that ends a display block outside the formula', () => {
    // L1's rule, and the reason a display formula's element does not swallow the
    // blank line after it: the range stops at the last character of the last line.
    const text = 'a\n\n\\[x\\]\n\nb\n';
    const runs = splitMathRuns(text, [{ start: 3, end: 8, display: true }]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['\\[x\\]']);
    expect(runs[2]?.text).toBe('\n\nb\n');
  });

  it('keeps a region that spans a line break in one run', () => {
    // An `InlineVerbatim` payload can contain a newline, so a formula is not
    // guaranteed to sit within one line of the output (§4.3).
    const text = 'see $a +\nb$ there';
    const runs = splitMathRuns(text, [inline(4, 11)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$a +\nb$']);
  });

  it('is the whole text, once, when there are no regions', () => {
    const text = 'nothing mathematical here';
    expect(splitMathRuns(text, [])).toEqual([{ text, math: null }]);
    expect(splitMathRuns('', [])).toEqual([{ text: '', math: null }]);
  });

  it('handles a formula at either end of the text', () => {
    const text = '$a$ and $b$';
    const runs = splitMathRuns(text, [inline(0, 3), inline(8, 11)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$a$', '$b$']);
    expect(runs).toHaveLength(3);
  });

  it('puts the regions in output order however they arrive', () => {
    const text = '$a$ $b$ $c$';
    const runs = splitMathRuns(text, [inline(8, 11), inline(0, 3), inline(4, 7)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$a$', '$b$', '$c$']);
  });

  it('drops an empty region rather than emitting an empty element', () => {
    const text = 'a $x$ b';
    const runs = splitMathRuns(text, [inline(2, 2), inline(2, 5)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$x$']);
  });

  it('clamps a region that runs past the end of the text', () => {
    // Nothing should ever send one; the point is that the text survives if it does,
    // because the alternative is a pane quietly missing its last characters.
    const text = 'short $x$';
    const runs = splitMathRuns(text, [inline(6, 900)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$x$']);
  });

  it('drops a region that overlaps the one before it, and keeps the text whole', () => {
    const text = 'one $a+b$ two';
    const runs = splitMathRuns(text, [inline(4, 9), inline(6, 12)]);

    expect(rejoin(runs)).toBe(text);
    expect(formulas(runs)).toEqual(['$a+b$']);
  });

  it('survives nonsense offsets without losing a character', () => {
    const text = 'still every byte of this';
    const runs = splitMathRuns(text, [
      { start: Number.NaN, end: 4, display: false },
      { start: 12, end: 6, display: true },
      { start: -3, end: -1, display: false },
      { start: 18, end: Number.POSITIVE_INFINITY, display: false },
    ]);

    expect(rejoin(runs)).toBe(text);
  });
});
