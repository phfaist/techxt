/**
 * The state codec (web/PLAN.md §6.4): defaults, validation, what gets stored, and
 * where a document and its options come from on load.
 *
 * Everything here runs in vitest's `node` environment, which is the point of taking
 * `localStorage` and the fragment as parameters: nothing in `state.ts` may need a
 * browser to be tested.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_FONT, DEFAULT_SIZE } from '../src/fonts';
import {
  DEFAULT_OPTIONS,
  DEFAULT_UI,
  MAX_STORED_DOC,
  STORAGE_KEYS,
  createPersistence,
  defaultState,
  encodeShare,
  loadState,
  pruneOptions,
  sanitizeOptions,
  sanitizeUi,
  withDefaults,
} from '../src/state';
import type { Clock, StorageLike } from '../src/state';
import type { AppOptions, UiState } from '../src/types';

/* ------------------------------------------------------------------- fixtures */

/** A clock a test can wind forward, so a 500 ms debounce costs no wall time. */
class FakeClock implements Clock {
  private now = 0;
  private next = 1;
  private readonly timers = new Map<number, { at: number; run: () => void }>();

  setTimeout(handler: () => void, ms: number): number {
    const id = this.next++;
    this.timers.set(id, { at: this.now + ms, run: handler });
    return id;
  }

  clearTimeout(handle: number): void {
    this.timers.delete(handle);
  }

  advance(ms: number): void {
    const until = this.now + ms;
    for (;;) {
      const due = [...this.timers.entries()]
        .filter(([, timer]) => timer.at <= until)
        .sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      this.timers.delete(due[0]);
      this.now = due[1].at;
      due[1].run();
    }
    this.now = until;
  }
}

class MemoryStorage implements StorageLike {
  readonly map = new Map<string, string>();

  getItem(key: string): string | null {
    return this.map.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.map.set(key, value);
  }

  removeItem(key: string): void {
    this.map.delete(key);
  }
}

let storage: MemoryStorage;
let clock: FakeClock;

beforeEach(() => {
  storage = new MemoryStorage();
  clock = new FakeClock();
});

/* ------------------------------------------------------------------- defaults */

describe('defaults', () => {
  it('answers exactly two questions for the user, and says which', () => {
    expect(DEFAULT_OPTIONS).toEqual({ wrap: 'soft', todayMode: 'browser' });
  });

  it('starts from an empty document and the ui defaults', () => {
    expect(defaultState()).toEqual({
      v: 1,
      doc: '',
      opts: { wrap: 'soft', todayMode: 'browser' },
      ui: {
        font: DEFAULT_FONT,
        size: DEFAULT_SIZE,
        split: 0.5,
        moreOpen: false,
        diagnosticsOpen: false,
        focus: null,
        keepFontsOffline: false,
      },
    });
  });

  it('hands out a fresh object every time', () => {
    const first = defaultState();
    first.opts.mathMode = 'plain';
    expect(defaultState().opts.mathMode).toBeUndefined();
  });
});

/* ----------------------------------------------------------------- validation */

describe('sanitizeOptions', () => {
  it('keeps what it knows', () => {
    expect(sanitizeOptions({ mathMode: 'plain', wrap: 'off', keepComments: true })).toEqual({
      mathMode: 'plain',
      wrap: 'off',
      keepComments: true,
    });
  });

  it('drops keys and values it does not know', () => {
    expect(
      sanitizeOptions({
        mathMode: 'interpretive-dance',
        keepComments: 'yes',
        wrap: '80',
        listStyle: ['*'],
        __proto__: { evil: true },
      }),
    ).toEqual({});
  });

  it('keeps Off, so a link that turns the folding off reproduces it', () => {
    expect(sanitizeOptions({ wrap: 'off' }).wrap).toBe('off');
    // …and still refuses anything that only looks like one of ours.
    expect(sanitizeOptions({ wrap: 'softly' }).wrap).toBeUndefined();
  });

  it('accepts a custom column count and normalises it', () => {
    expect(sanitizeOptions({ wrap: 63.7 }).wrap).toBe(63);
    expect(sanitizeOptions({ wrap: 0 }).wrap).toBeUndefined();
    expect(sanitizeOptions({ wrap: 1e9 }).wrap).toBe(10_000);
  });

  it('is unbothered by anything at all', () => {
    for (const raw of [null, undefined, 42, 'options', [], [1, 2], new Date()]) {
      expect(sanitizeOptions(raw)).toEqual({});
    }
  });
});

describe('pruneOptions', () => {
  it('drops the values that are the app default anyway', () => {
    expect(pruneOptions({ wrap: 'soft', todayMode: 'browser', mathMode: 'plain' })).toEqual({
      mathMode: 'plain',
    });
  });

  it('keeps a value that differs from the default, including a falsy one', () => {
    expect(pruneOptions({ wrap: 'off', keepComments: false })).toEqual({
      wrap: 'off',
      keepComments: false,
    });
    expect(pruneOptions({ wrap: 'fit' })).toEqual({ wrap: 'fit' });
  });

  it('is the inverse of withDefaults', () => {
    const opts = withDefaults({ mathMode: 'source' });
    expect(opts).toEqual({ wrap: 'soft', todayMode: 'browser', mathMode: 'source' });
    expect(withDefaults(pruneOptions(opts))).toEqual(opts);
  });
});

describe('sanitizeUi', () => {
  it('falls back to the defaults for anything unusable', () => {
    expect(sanitizeUi(null)).toEqual(DEFAULT_UI);
    expect(sanitizeUi({ font: 'comic-sans', size: 'big', split: null })).toEqual(DEFAULT_UI);
  });

  it('clamps the ranges the controls can actually show', () => {
    const wild = sanitizeUi({ size: 400, split: 0.99, focus: 'sideways' });
    expect(wild.size).toBe(20);
    expect(wild.split).toBe(0.8);
    expect(wild.focus).toBeNull();
    expect(sanitizeUi({ size: 2, split: -3 }).split).toBe(0.2);
  });

  it('keeps a state it recognises', () => {
    const ui: UiState = {
      font: 'stix',
      size: 18,
      split: 0.35,
      moreOpen: true,
      diagnosticsOpen: true,
      focus: 'output',
      keepFontsOffline: true,
    };
    expect(sanitizeUi(ui)).toEqual(ui);
  });
});

/* ---------------------------------------------------------------- persistence */

describe('persistence', () => {
  it('writes nothing until the debounce has passed, then writes once', () => {
    const persistence = createPersistence({ storage, clock });
    persistence.document('one');
    clock.advance(400);
    persistence.document('two');
    clock.advance(400);
    expect(storage.map.get(STORAGE_KEYS.doc)).toBeUndefined();
    clock.advance(100);
    expect(storage.map.get(STORAGE_KEYS.doc)).toBe('two');
  });

  it('writes the three keys of §6.4 and prunes the options on the way', () => {
    const persistence = createPersistence({ storage, clock });
    persistence.document('\\section{hi}');
    persistence.options({ wrap: 'soft', todayMode: 'browser', mathMode: 'plain' });
    persistence.ui({ ...DEFAULT_UI, font: 'stix' });
    persistence.flush();

    expect(storage.map.get(STORAGE_KEYS.doc)).toBe('\\section{hi}');
    expect(JSON.parse(storage.map.get(STORAGE_KEYS.opts) ?? '')).toEqual({ mathMode: 'plain' });
    expect(JSON.parse(storage.map.get(STORAGE_KEYS.ui) ?? '').font).toBe('stix');
  });

  it('refuses to store a document over the cap, and tells the caller', () => {
    const oversize = vi.fn();
    const persistence = createPersistence({ storage, clock, onDocumentOversize: oversize });

    persistence.document('small');
    persistence.flush();
    expect(storage.map.get(STORAGE_KEYS.doc)).toBe('small');
    expect(oversize).not.toHaveBeenCalled();

    persistence.document('x'.repeat(MAX_STORED_DOC + 1));
    persistence.flush();
    // Nothing is stored at all: half a document coming back after a reload is worse
    // than none, and the status line says so.
    expect(storage.map.has(STORAGE_KEYS.doc)).toBe(false);
    expect(oversize).toHaveBeenCalledWith(true);

    persistence.document('small again');
    persistence.flush();
    expect(storage.map.get(STORAGE_KEYS.doc)).toBe('small again');
    expect(oversize).toHaveBeenLastCalledWith(false);
    expect(oversize).toHaveBeenCalledTimes(2);
  });

  it('stores a document that is exactly at the cap', () => {
    const persistence = createPersistence({ storage, clock });
    persistence.document('y'.repeat(MAX_STORED_DOC));
    persistence.flush();
    expect(storage.map.get(STORAGE_KEYS.doc)?.length).toBe(MAX_STORED_DOC);
  });

  it('swallows a storage that refuses to be written to', () => {
    const hostile: StorageLike = {
      getItem: () => null,
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
      removeItem: () => {
        throw new Error('nope');
      },
    };
    const persistence = createPersistence({ storage: hostile, clock });
    persistence.document('x'.repeat(MAX_STORED_DOC + 1));
    persistence.options({ mathMode: 'plain' });
    expect(() => persistence.flush()).not.toThrow();
  });

  it('does nothing at all without a storage', () => {
    const persistence = createPersistence({ storage: null, clock });
    persistence.document('nowhere to go');
    expect(() => persistence.flush()).not.toThrow();
  });

  it('forgets what it was about to write when disposed', () => {
    const persistence = createPersistence({ storage, clock });
    persistence.document('never written');
    persistence.dispose();
    clock.advance(5000);
    expect(storage.map.has(STORAGE_KEYS.doc)).toBe(false);
  });
});

/* ----------------------------------------------------------------------- load */

describe('loadState', () => {
  it('is the defaults, and a first visit, with nothing anywhere', async () => {
    const loaded = await loadState({ storage: null, fragment: null });
    expect(loaded.state).toEqual(defaultState());
    expect(loaded.source).toBe('defaults');
    expect(loaded.firstVisit).toBe(true);
  });

  it('reads what was stored, and is no longer a first visit', async () => {
    storage.setItem(STORAGE_KEYS.doc, '\\emph{stored}');
    storage.setItem(STORAGE_KEYS.opts, JSON.stringify({ mathMode: 'plain' }));
    storage.setItem(STORAGE_KEYS.ui, JSON.stringify({ font: 'stix', size: 17 }));

    const loaded = await loadState({ storage, fragment: '' });
    expect(loaded.state.doc).toBe('\\emph{stored}');
    expect(loaded.state.opts).toEqual({ wrap: 'soft', todayMode: 'browser', mathMode: 'plain' });
    expect(loaded.state.ui.font).toBe('stix');
    expect(loaded.state.ui.size).toBe(17);
    expect(loaded.source).toBe('storage');
    expect(loaded.firstVisit).toBe(false);
  });

  it('treats a deliberately emptied document as a document', async () => {
    storage.setItem(STORAGE_KEYS.doc, '');
    const loaded = await loadState({ storage });
    expect(loaded.firstVisit).toBe(false);
    expect(loaded.state.doc).toBe('');
  });

  it('lets a fragment win over storage, but never over the ui', async () => {
    storage.setItem(STORAGE_KEYS.doc, 'the stored one');
    storage.setItem(STORAGE_KEYS.opts, JSON.stringify({ mathMode: 'plain' }));
    storage.setItem(STORAGE_KEYS.ui, JSON.stringify({ font: 'stix' }));
    const fragment = await encodeShare({
      v: 1,
      doc: 'the shared one',
      opts: { headingStyle: 'plain', font: 'libertinus' } as unknown as AppOptions,
    });

    const loaded = await loadState({ storage, fragment });
    expect(loaded.state.doc).toBe('the shared one');
    expect(loaded.state.opts).toEqual({
      wrap: 'soft',
      todayMode: 'browser',
      headingStyle: 'plain',
    });
    // A link reproduces a session, not somebody else's window (§6.4).
    expect(loaded.state.ui.font).toBe('stix');
    expect(loaded.source).toBe('fragment');
  });

  it('ignores a fragment that is not ours and falls through to storage', async () => {
    storage.setItem(STORAGE_KEYS.doc, 'the stored one');
    const loaded = await loadState({ storage, fragment: '#section-3' });
    expect(loaded.state.doc).toBe('the stored one');
    expect(loaded.source).toBe('storage');
  });

  it('never throws over corrupt storage', async () => {
    storage.setItem(STORAGE_KEYS.opts, '{"mathMode": "pla');
    storage.setItem(STORAGE_KEYS.ui, 'not json either');
    storage.setItem(STORAGE_KEYS.doc, 'still here');

    const loaded = await loadState({ storage });
    expect(loaded.state.doc).toBe('still here');
    expect(loaded.state.opts).toEqual(DEFAULT_OPTIONS);
    expect(loaded.state.ui).toEqual(DEFAULT_UI);
  });

  it('never throws over a storage that will not answer', async () => {
    const hostile: StorageLike = {
      getItem: () => {
        throw new Error('SecurityError');
      },
      setItem: () => undefined,
      removeItem: () => undefined,
    };
    const loaded = await loadState({ storage: hostile });
    expect(loaded.state).toEqual(defaultState());
    expect(loaded.firstVisit).toBe(true);
  });
});
