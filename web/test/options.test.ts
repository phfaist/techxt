/**
 * `resolveOptions`, `formatToday` and `columnsFor` — the three pure functions the
 * conversion actually depends on (web/PLAN.md §5, §6.5, §13).
 */

import { describe, expect, it } from 'vitest';

import { DEFAULT_OPTIONS, MIN_FIT_COLUMNS, columnsFor, formatToday, resolveOptions } from '../src/state';
import { APP_ONLY_KEYS } from '../src/types';
import type { AppOptions } from '../src/types';

/** A fixed instant, built from local parts so the assertion holds in any time zone. */
const NOON = new Date(2026, 7, 20, 12, 0, 0);

describe('formatToday', () => {
  it('spells the month out, the way techxt-cli does', () => {
    expect(formatToday(NOON)).toBe('August 20, 2026');
    // The example web/PLAN.md §5 gives, and the one rust/techxt-cli/src/today.rs tests.
    expect(formatToday(new Date(2026, 7, 19))).toBe('August 19, 2026');
  });

  it('does not pad the day and handles the ends of the calendar', () => {
    expect(formatToday(new Date(1970, 0, 1))).toBe('January 1, 1970');
    expect(formatToday(new Date(2024, 1, 29))).toBe('February 29, 2024');
    expect(formatToday(new Date(2026, 11, 31))).toBe('December 31, 2026');
  });

  it('reads the local date, not the UTC one', () => {
    // Late on the 20th locally is already the 21st in UTC east of Greenwich; the app
    // shows the date the person's own clock shows (§5).
    expect(formatToday(new Date(2026, 7, 20, 23, 30))).toBe('August 20, 2026');
  });
});

describe('resolveOptions: wrapping', () => {
  it('turns Fit into the measured column count', () => {
    expect(resolveOptions({ wrap: 'fit' }, 88, NOON).wrapWidth).toBe(88);
  });

  it('omits wrapWidth for Off, which is the library default', () => {
    const payload = resolveOptions({ wrap: 'off' }, 88, NOON);
    expect('wrapWidth' in payload).toBe(false);
  });

  it('passes an explicit column count through', () => {
    expect(resolveOptions({ wrap: 72 }, 88, NOON).wrapWidth).toBe(72);
  });

  it('treats an absent wrap as the app default, which is Fit', () => {
    expect(DEFAULT_OPTIONS.wrap).toBe('fit');
    expect(resolveOptions({}, 64, NOON).wrapWidth).toBe(64);
  });

  it('never asks for fewer columns than the pane arithmetic allows', () => {
    expect(resolveOptions({ wrap: 'fit' }, 3, NOON).wrapWidth).toBe(MIN_FIT_COLUMNS);
  });
});

describe('resolveOptions: today', () => {
  it('sends the browser date for the browser mode', () => {
    expect(resolveOptions({ todayMode: 'browser' }, 72, NOON).today).toBe('August 20, 2026');
  });

  it('omits today for the library mode, leaving <today>', () => {
    expect('today' in resolveOptions({ todayMode: 'library' }, 72, NOON)).toBe(false);
  });

  it('sends the literal for the custom mode', () => {
    const payload = resolveOptions({ todayMode: 'custom', todayCustom: 'yesterday' }, 72, NOON);
    expect(payload.today).toBe('yesterday');
  });

  it('omits today for the custom mode with nothing typed yet', () => {
    expect('today' in resolveOptions({ todayMode: 'custom' }, 72, NOON)).toBe(false);
  });

  it('defaults to the browser date when the mode was pruned away', () => {
    expect(resolveOptions({}, 72, NOON).today).toBe('August 20, 2026');
  });
});

describe('resolveOptions: what reaches the worker', () => {
  const everything: AppOptions = {
    mathMode: 'plain',
    mathExpressionIn: 'braces',
    matrixDelimiters: 'ascii',
    keepComments: true,
    headingStyle: 'prefix',
    footnoteStyle: 'inline',
    textFont: 'off',
    mathFont: 'upright',
    unknownMacro: 'placeholder',
    unknownEnv: 'keep-source',
    unknownSpecials: 'skip',
    recovery: 'strict',
    wrap: 40,
    todayMode: 'custom',
    todayCustom: 'the first of Never',
  };

  it('strips every app-only key', () => {
    const payload = resolveOptions(everything, 72, NOON) as Record<string, unknown>;
    for (const key of APP_ONLY_KEYS) {
      expect(key in payload).toBe(false);
    }
  });

  it('passes every library option through unchanged', () => {
    const payload = resolveOptions(everything, 72, NOON);
    expect(payload).toMatchObject({
      mathMode: 'plain',
      mathExpressionIn: 'braces',
      matrixDelimiters: 'ascii',
      keepComments: true,
      headingStyle: 'prefix',
      footnoteStyle: 'inline',
      textFont: 'off',
      mathFont: 'upright',
      unknownMacro: 'placeholder',
      unknownEnv: 'keep-source',
      unknownSpecials: 'skip',
      recovery: 'strict',
      wrapWidth: 40,
      today: 'the first of Never',
    });
  });

  it('sends nothing at all for the defaults, so the library decides', () => {
    const payload = resolveOptions({ wrap: 'off', todayMode: 'library' }, 72, NOON);
    expect(payload).toEqual({});
  });
});

describe('columnsFor', () => {
  // The measurement itself belongs to ui/panes.ts; this is only the arithmetic of
  // web/PLAN.md §6.5: max(20, floor(width * 0.98 / advance)).
  it('divides the usable width by the mean advance', () => {
    expect(columnsFor(800, 8.4)).toBe(93);
    expect(columnsFor(1000, 10)).toBe(98);
  });

  it('never goes below twenty columns', () => {
    expect(columnsFor(100, 10)).toBe(MIN_FIT_COLUMNS);
    expect(columnsFor(0, 10)).toBe(MIN_FIT_COLUMNS);
  });

  it('survives a measurement that failed', () => {
    expect(columnsFor(800, 0)).toBe(MIN_FIT_COLUMNS);
    expect(columnsFor(800, Number.NaN)).toBe(MIN_FIT_COLUMNS);
    expect(columnsFor(Number.POSITIVE_INFINITY, 8)).toBe(MIN_FIT_COLUMNS);
  });
});
