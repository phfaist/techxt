/**
 * The library export/import codec (web/PLAN.md §6.11), tested the way the share
 * codec is: a round trip, then everything a file can be that it should not be.
 *
 * The most important test in this file is the one that says an import never removes
 * an existing entry unless the user chose Replace on that particular import. It is a
 * property of `planImport` rather than of the UI, which is the point of `planImport`
 * existing at all.
 */

import { describe, expect, it } from 'vitest';

import {
  LIBRARY_FORMAT,
  LIBRARY_VERSION,
  MAX_IMPORT_CHARS,
  decodeLibrary,
  describeImport,
  encodeLibrary,
  libraryFileName,
  planImport,
} from '../src/library-io';
import { MAX_ENTRY_SOURCE } from '../src/library';
import type { LibraryEntry } from '../src/library';

function entry(over: Partial<LibraryEntry> = {}): LibraryEntry {
  return {
    id: 'e1',
    createdAt: Date.parse('2026-08-01T09:00:00.000Z'),
    updatedAt: Date.parse('2026-08-02T09:00:00.000Z'),
    title: 'A document',
    source: '\\section{A document}\nbody',
    options: {},
    starred: false,
    preview: 'A DOCUMENT\n\nbody',
    ...over,
  };
}

const NOW = Date.parse('2026-08-28T12:00:00.000Z');
const META = { exportedAt: new Date(NOW), techxt: '0.1.0' };

/** An id generator a test can predict. */
function ids(): () => string {
  let n = 0;
  return () => `new${++n}`;
}

/* ------------------------------------------------------------------ exporting */

describe('encodeLibrary', () => {
  it('writes the format of §6.11, with the previews in it', () => {
    const file = JSON.parse(encodeLibrary([entry({ starred: true })], META)) as Record<
      string,
      unknown
    >;
    expect(file['format']).toBe(LIBRARY_FORMAT);
    expect(file['v']).toBe(LIBRARY_VERSION);
    expect(file['app']).toBe('techxt-web');
    expect(file['techxt']).toBe('0.1.0');
    expect(file['exportedAt']).toBe('2026-08-28T12:00:00.000Z');

    const items = file['items'] as Record<string, unknown>[];
    expect(items).toHaveLength(1);
    expect(items[0]).toEqual({
      id: 'e1',
      createdAt: '2026-08-01T09:00:00.000Z',
      updatedAt: '2026-08-02T09:00:00.000Z',
      title: 'A document',
      source: '\\section{A document}\nbody',
      options: {},
      starred: true,
      preview: 'A DOCUMENT\n\nbody',
    });
  });

  it('prunes an option that is the app default, as everything else does', () => {
    const file = JSON.parse(encodeLibrary([entry({ options: { wrap: 'soft' } })], META)) as {
      items: { options: Record<string, unknown> }[];
    };
    expect(file.items[0]?.options).toEqual({});
  });

  it('names the file after the day it was written', () => {
    expect(libraryFileName(new Date(2026, 7, 3))).toBe('techxt-library-2026-08-03.json');
  });

  it('exports an empty library without complaint', () => {
    const decoded = decodeLibrary(encodeLibrary([], META), NOW);
    expect(decoded.ok && decoded.library.entries).toEqual([]);
  });
});

/* ------------------------------------------------------------------ the trip */

describe('a round trip', () => {
  it('brings every entry back unchanged, previews included', () => {
    const entries = [
      entry(),
      entry({ id: 'e2', starred: true, title: 'Starred one', options: { math: 'source' } }),
      entry({ id: 'e3', source: 'Grüße 𝕏 🎉\nמה קורה?', preview: 'Grüße 𝕏 🎉' }),
    ];
    const decoded = decodeLibrary(encodeLibrary(entries, META), NOW);
    expect(decoded.ok).toBe(true);
    expect(decoded.ok && decoded.library.entries).toEqual(entries);
    expect(decoded.ok && decoded.library.techxt).toBe('0.1.0');
    expect(decoded.ok && decoded.library.exportedAt).toBe('2026-08-28T12:00:00.000Z');
  });
});

/* ------------------------------------------------------- a file to be careful of */

describe('decodeLibrary refuses, and says why', () => {
  it('an empty file', () => {
    expect(decodeLibrary('', NOW)).toEqual({ ok: false, reason: 'That file is empty.' });
    expect(decodeLibrary('   \n', NOW).ok).toBe(false);
  });

  it('a truncated one', () => {
    const text = encodeLibrary([entry()], META);
    const result = decodeLibrary(text.slice(0, text.length / 2), NOW);
    expect(result.ok).toBe(false);
    expect(result.ok === false && result.reason).toMatch(/truncated/);
  });

  it('a foreign one', () => {
    const result = decodeLibrary(JSON.stringify({ format: 'someone.else', v: 1, items: [] }), NOW);
    expect(result.ok === false && result.reason).toMatch(/not a techxt library/);
  });

  it('one that is JSON but not an object', () => {
    expect(decodeLibrary('[1, 2, 3]', NOW).ok).toBe(false);
    expect(decodeLibrary('"a string"', NOW).ok).toBe(false);
    expect(decodeLibrary('null', NOW).ok).toBe(false);
  });

  it('one from a future build, naming the version', () => {
    const result = decodeLibrary(
      JSON.stringify({ format: LIBRARY_FORMAT, v: 2, items: [] }),
      NOW,
    );
    expect(result.ok).toBe(false);
    expect(result.ok === false && result.reason).toMatch(/v2/);
    expect(result.ok === false && result.reason).toMatch(/v1/);
  });

  it('one with no version and one with no items', () => {
    expect(decodeLibrary(JSON.stringify({ format: LIBRARY_FORMAT, items: [] }), NOW).ok).toBe(false);
    expect(decodeLibrary(JSON.stringify({ format: LIBRARY_FORMAT, v: 1 }), NOW).ok).toBe(false);
  });

  it('one far larger than any library this app writes', () => {
    // Checked before `JSON.parse`, which is the point: a cap that only applies after
    // parsing is not a cap. The limit is a parameter so this test need not build a
    // 128 MB string to find out.
    const text = encodeLibrary([entry()], META);
    expect(decodeLibrary(text, NOW, { maxChars: 10 })).toEqual({
      ok: false,
      reason: 'That file is far larger than any library this app can have written.',
    });
    // And the real cap is far above anything this app could have written.
    expect(MAX_IMPORT_CHARS).toBeGreaterThan(100 * MAX_ENTRY_SOURCE);
    expect(decodeLibrary(text, NOW).ok).toBe(true);
  });

  it('one whose every item is unreadable', () => {
    const result = decodeLibrary(
      JSON.stringify({ format: LIBRARY_FORMAT, v: 1, items: [null, 5, { source: 42 }] }),
      NOW,
    );
    expect(result.ok === false && result.reason).toMatch(/unreadable/);
  });

  it('never throws, whatever the file is', () => {
    for (const text of ['', '{', '[]', 'undefined', ' ', '\u0000', '{"format":1}']) {
      expect(() => decodeLibrary(text, NOW)).not.toThrow();
    }
  });
});

describe('decodeLibrary keeps what it can', () => {
  it('drops the items it cannot read and counts them', () => {
    const good = JSON.parse(encodeLibrary([entry()], META)) as { items: unknown[] };
    const result = decodeLibrary(
      JSON.stringify({
        format: LIBRARY_FORMAT,
        v: 1,
        items: [...good.items, null, { source: '' }, { source: 'x'.repeat(MAX_ENTRY_SOURCE + 1) }],
      }),
      NOW,
    );
    expect(result.ok).toBe(true);
    expect(result.ok && result.library.entries).toHaveLength(1);
    expect(result.ok && result.library.dropped).toEqual({ malformed: 2, oversize: 1 });
  });

  it('drops an option value it does not know and keeps the entry', () => {
    const result = decodeLibrary(
      JSON.stringify({
        format: LIBRARY_FORMAT,
        v: 1,
        items: [{ source: 'body', options: { math: 'interpretive-dance', keepComments: true } }],
      }),
      NOW,
    );
    expect(result.ok && result.library.entries[0]?.options).toEqual({ keepComments: true });
  });

  it('gives an item with no id one, rather than refusing the file', () => {
    const result = decodeLibrary(
      JSON.stringify({ format: LIBRARY_FORMAT, v: 1, items: [{ source: 'body' }] }),
      NOW,
    );
    expect(result.ok && result.library.entries[0]?.id).toBeTruthy();
    expect(result.ok && result.library.entries[0]?.createdAt).toBe(NOW);
  });

  it('drops the unknown fields somebody added to a file', () => {
    const result = decodeLibrary(
      JSON.stringify({
        format: LIBRARY_FORMAT,
        v: 1,
        items: [{ source: 'body', mischief: 'hello', __proto__: { evil: true } }],
      }),
      NOW,
    );
    const decoded = result.ok ? result.library.entries[0] : null;
    expect(decoded && Object.keys(decoded).sort()).toEqual([
      'createdAt',
      'id',
      'options',
      'preview',
      'source',
      'starred',
      'title',
      'updatedAt',
    ]);
  });
});

/* ------------------------------------------------- what an import is allowed to do */

describe('planImport never removes anything unless Replace was chosen', () => {
  const existing = [entry({ id: 'mine1' }), entry({ id: 'mine2', starred: true })];

  it('holds for Add', () => {
    const plan = planImport(existing, [entry({ id: 'theirs' })], {
      mode: 'add',
      skipExisting: false,
    });
    expect(plan.remove).toEqual([]);
    expect(plan.replaced).toBe(0);
  });

  it('holds for Add with "skip what I have" — a skip skips the incoming one', () => {
    const plan = planImport(existing, [entry({ id: 'theirs' })], {
      mode: 'add',
      skipExisting: true,
    });
    // The incoming entry is the same document as `mine1`, so it is skipped — and
    // `mine1` is still there, untouched.
    expect(plan.remove).toEqual([]);
    expect(plan.put).toEqual([]);
    expect(plan.skipped).toBe(1);
  });

  it('holds when the file is empty, or full of things already there', () => {
    expect(planImport(existing, [], { mode: 'add', skipExisting: false }).remove).toEqual([]);
    expect(planImport(existing, existing, { mode: 'add', skipExisting: true }).remove).toEqual([]);
  });

  it('holds when every id in the file collides with one already here', () => {
    const plan = planImport(
      existing,
      [entry({ id: 'mine1', source: 'a different document' })],
      { mode: 'add', skipExisting: false },
      { newId: ids() },
    );
    expect(plan.remove).toEqual([]);
    expect(plan.put[0]?.id).toBe('new1');
    expect(plan.put[0]?.source).toBe('a different document');
  });

  it('and Replace is the one path that does, naming what it costs', () => {
    const plan = planImport(existing, [entry({ id: 'theirs' })], {
      mode: 'replace',
      skipExisting: false,
    });
    expect(plan.remove).toEqual(['mine1', 'mine2']);
    expect(plan.replaced).toBe(2);
    expect(plan.losing).toEqual({ count: 2, starred: 1 });
  });
});

describe('planImport: adding', () => {
  it('keeps an incoming id that is free', () => {
    const plan = planImport([entry({ id: 'mine' })], [entry({ id: 'theirs' })], {
      mode: 'add',
      skipExisting: false,
    });
    expect(plan.put.map((item) => item.id)).toEqual(['theirs']);
    expect(plan.added).toBe(1);
  });

  it('gives a colliding id a fresh one rather than overwriting the entry', () => {
    const mine = entry({ id: 'same', source: 'mine', title: 'Mine' });
    const theirs = entry({ id: 'same', source: 'theirs', title: 'Theirs' });
    const plan = planImport([mine], [theirs], { mode: 'add', skipExisting: false }, { newId: ids() });
    expect(plan.put).toHaveLength(1);
    expect(plan.put[0]?.id).toBe('new1');
    expect(plan.put[0]?.source).toBe('theirs');
    // Nothing about the existing entry appears in the plan at all.
    expect(plan.remove).toEqual([]);
  });

  it('separates two incoming entries that share an id', () => {
    const plan = planImport(
      [],
      [entry({ id: 'dup', source: 'one' }), entry({ id: 'dup', source: 'two' })],
      { mode: 'add', skipExisting: false },
      { newId: ids() },
    );
    expect(plan.put.map((item) => item.id)).toEqual(['dup', 'new1']);
  });

  it('imports a library into itself as a second copy, which is what Add means', () => {
    const mine = [entry({ id: 'a' }), entry({ id: 'b', source: 'other' })];
    const plan = planImport(mine, mine, { mode: 'add', skipExisting: false }, { newId: ids() });
    expect(plan.added).toBe(2);
    expect(plan.remove).toEqual([]);
    expect(plan.put.map((item) => item.id)).toEqual(['new1', 'new2']);
  });
});

describe('planImport: skipping what I already have', () => {
  it('matches on the content, not the id', () => {
    const mine = entry({ id: 'mine', title: 'My name for it' });
    const theirs = entry({ id: 'theirs', title: 'Their name for it' });
    const plan = planImport([mine], [theirs], { mode: 'add', skipExisting: true });
    expect(plan.skipped).toBe(1);
    expect(plan.added).toBe(0);
  });

  it('does not match a document converted under different options', () => {
    const mine = entry({ id: 'mine', options: {} });
    const theirs = entry({ id: 'theirs', options: { math: 'source' } });
    const plan = planImport([mine], [theirs], { mode: 'add', skipExisting: true });
    expect(plan.added).toBe(1);
    expect(plan.skipped).toBe(0);
  });

  it('lands a file that repeats one document exactly once', () => {
    const twice = [entry({ id: 'a' }), entry({ id: 'b' })];
    const plan = planImport([], twice, { mode: 'add', skipExisting: true });
    expect(plan.added).toBe(1);
    expect(plan.skipped).toBe(1);
  });
});

describe('planImport: replacing', () => {
  it('writes every incoming entry and removes every existing one, and nothing else', () => {
    const mine = [entry({ id: 'a' }), entry({ id: 'b' })];
    const theirs = [entry({ id: 'c' }), entry({ id: 'd' })];
    const plan = planImport(mine, theirs, { mode: 'replace', skipExisting: true });
    expect(plan.put.map((item) => item.id)).toEqual(['c', 'd']);
    expect(plan.remove).toEqual(['a', 'b']);
    expect(plan.skipped).toBe(0);
  });

  it('reports an empty library as costing nothing', () => {
    const plan = planImport([], [entry()], { mode: 'replace', skipExisting: false });
    expect(plan.losing).toEqual({ count: 0, starred: 0 });
    expect(plan.remove).toEqual([]);
  });
});

describe('describeImport', () => {
  it('says what happened, in the words §6.11 asks for', () => {
    const plan = planImport([], Array.from({ length: 12 }, (_, i) => entry({ id: `e${i}`, source: `s${i}` })), {
      mode: 'add',
      skipExisting: false,
    });
    expect(describeImport(plan)).toBe('12 added, 0 skipped, 0 replaced.');
  });

  it('mentions what the file lost on the way in', () => {
    const plan = planImport([], [entry()], { mode: 'add', skipExisting: false });
    expect(describeImport(plan, { malformed: 2, oversize: 1 })).toBe(
      '1 added, 0 skipped, 0 replaced. 3 items in the file could not be read.',
    );
    expect(describeImport(plan, { malformed: 1, oversize: 0 })).toMatch(/1 item in the file/);
  });
});
