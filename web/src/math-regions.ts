/**
 * Cutting the converted text into the runs the output pane wraps in elements
 * (web/PLAN.md §4.3, §6.3).
 *
 * The binding says where the formulas are as a side table of `{start, end, display}`
 * ranges beside the text; the pane sets the text as it always has and then wraps each
 * of those ranges in an element for MathJax. The arithmetic of "which slices, in what
 * order" is here rather than in `ui/panes.ts` for one reason: vitest runs in `node`
 * with no DOM, and this is the half of the job that is worth testing — the DOM half is
 * four lines of `createElement` and `createTextNode`.
 *
 * The one invariant, and the reason for every clamp below: **the runs concatenate back
 * to the text they were cut from, byte for byte**. Wrapping a region in an element must
 * not add, drop or reorder a character, because the same string is what Copy, Download
 * and the library hand over.
 *
 * The ranges arrive from wasm rather than from a user, so they are already in order and
 * already disjoint. They are nevertheless treated as input that could be neither: a
 * region past the end of the text, or one overlapping its neighbour, would otherwise
 * turn into text quietly duplicated or lost in the pane, and a conversion result is not
 * worth trusting that far when distrusting it costs a `Math.min`.
 */

import type { MathRegion } from './worker/protocol';

/** One slice of the converted text: ordinary text, or one formula. */
export interface OutputRun {
  /** The slice itself, exactly as it appears in the output. */
  text: string;
  /**
   * `null` for ordinary text; for a formula, how it should be laid out — `display`
   * meaning `\[…\]`, `equation`, `align` rather than `$…$`.
   */
  math: { display: boolean } | null;
}

/**
 * Cut `text` at every region boundary, in output order.
 *
 * Empty and out-of-range regions are dropped (a construct that renders to nothing
 * reports nothing, but a clamp is cheaper than trusting that), and where two regions
 * overlap the first one wins — the second is dropped rather than nested or split, since
 * a formula inside a formula is not a thing techxt reports and half of one is not
 * something MathJax can read.
 *
 * A region may contain a newline: an inline formula's payload can keep one, so a run is
 * not guaranteed to sit within a single line of the output (§4.3). Nothing here cares,
 * and neither does the element it becomes.
 */
export function splitMathRuns(text: string, regions: readonly MathRegion[]): OutputRun[] {
  const runs: OutputRun[] = [];
  let at = 0;

  for (const region of ordered(regions, text.length)) {
    if (region.start < at) continue; // overlaps the region before it
    if (region.start > at) runs.push({ text: text.slice(at, region.start), math: null });
    runs.push({
      text: text.slice(region.start, region.end),
      math: { display: region.display === true },
    });
    at = region.end;
  }

  if (at < text.length || runs.length === 0) {
    runs.push({ text: text.slice(at), math: null });
  }
  return runs;
}

/** The regions that name a real, non-empty range of a text `length` long, in order. */
function ordered(regions: readonly MathRegion[], length: number): MathRegion[] {
  const kept: MathRegion[] = [];
  for (const region of regions) {
    const start = clamp(region?.start, length);
    const end = clamp(region?.end, length);
    if (end > start) kept.push({ start, end, display: region.display === true });
  }
  return kept.sort((a, b) => a.start - b.start || a.end - b.end);
}

function clamp(value: unknown, length: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
  return Math.min(length, Math.max(0, Math.floor(value)));
}
