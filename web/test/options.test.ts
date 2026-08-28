/**
 * `resolveOptions`, `formatToday` and `columnsFor` — the three pure functions the
 * conversion actually depends on (web/PLAN.md §5, §6.5, §13).
 */

import { describe, expect, it } from 'vitest';

import {
  DEFAULT_OPTIONS,
  MIN_FIT_COLUMNS,
  columnsFor,
  formatToday,
  mathJax,
  resolveOptions,
  softWraps,
} from '../src/state';
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

  it('treats an absent wrap as the app default, which is Soft', () => {
    expect(DEFAULT_OPTIONS.wrap).toBe('soft');
    // And Soft asks the library for nothing, so the app's own default no longer
    // changes a byte of the converted text (§5).
    expect('wrapWidth' in resolveOptions({}, 64, NOON)).toBe(false);
  });

  it('never asks for fewer columns than the pane arithmetic allows', () => {
    expect(resolveOptions({ wrap: 'fit' }, 3, NOON).wrapWidth).toBe(MIN_FIT_COLUMNS);
  });

  it('sends the library exactly the same thing for Soft as for Off', () => {
    // The whole point of the mode: the *text* is Wrap: Off's text, and only the pane
    // it is shown in differs (§6.3).
    expect(resolveOptions({ wrap: 'soft' }, 88, NOON)).toEqual(
      resolveOptions({ wrap: 'off' }, 88, NOON),
    );
    expect('wrapWidth' in resolveOptions({ wrap: 'soft' }, 88, NOON)).toBe(false);
  });
});

describe('softWraps', () => {
  it('is the one wrap setting the output pane folds for', () => {
    expect(softWraps({ wrap: 'soft' })).toBe(true);
    expect(softWraps({ wrap: 'off' })).toBe(false);
    expect(softWraps({ wrap: 'fit' })).toBe(false);
    expect(softWraps({ wrap: 72 })).toBe(false);
  });

  it('reads an absent wrap as the app default, exactly as resolveOptions does', () => {
    expect(softWraps({})).toBe(true);
  });
});

describe('resolveOptions: math', () => {
  it('sends the library its own three answers under its own key', () => {
    expect(resolveOptions({ math: 'fancy' }, 72, NOON).mathMode).toBe('fancy');
    expect(resolveOptions({ math: 'plain' }, 72, NOON).mathMode).toBe('plain');
    expect(resolveOptions({ math: 'source' }, 72, NOON).mathMode).toBe('source');
  });

  it('turns MathJax into Source, which is the whole of what the library is told', () => {
    // The app typesets what Source re-emits; the library has never heard of MathJax
    // and this is the one place that stays true (§5).
    expect(resolveOptions({ math: 'mathjax' }, 72, NOON)).toEqual(
      resolveOptions({ math: 'source' }, 72, NOON),
    );
  });

  it('omits mathMode when the setting was pruned away, so the library decides', () => {
    expect('mathMode' in resolveOptions({}, 72, NOON)).toBe(false);
  });
});

describe('mathJax', () => {
  it('is the one math setting the output pane typesets for', () => {
    expect(mathJax({ math: 'mathjax' })).toBe(true);
    expect(mathJax({ math: 'source' })).toBe(false);
    expect(mathJax({ math: 'fancy' })).toBe(false);
    expect(mathJax({})).toBe(false);
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
    math: 'plain',
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
    macroDefinitions: 'declared',
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
      // `math` is app-level, and the library's key for it is `mathMode` (§5).
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
      macroDefinitions: 'declared',
      wrapWidth: 40,
      today: 'the first of Never',
    });
  });

  it('sends nothing at all for the defaults, so the library decides', () => {
    const payload = resolveOptions({ wrap: 'off', todayMode: 'library' }, 72, NOON);
    expect(payload).toEqual({});
  });

  it('never lets the word mathjax reach an OptionsPayload, whatever else is set', () => {
    // The binding deserializes this object into `techxt::convert::Options`, where
    // `mathjax` is not a `MathMode` and never will be (§5).
    for (const payload of [
      resolveOptions({ math: 'mathjax' }, 72, NOON),
      resolveOptions({ math: 'mathjax', wrap: 'fit', todayMode: 'library' }, 40, NOON),
      resolveOptions({ ...everything, math: 'mathjax' }, 72, NOON),
    ]) {
      expect(JSON.stringify(payload)).not.toContain('mathjax');
      expect(Object.values(payload)).not.toContain('mathjax');
      expect('math' in payload).toBe(false);
      expect(payload.mathMode).toBe('source');
    }
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
