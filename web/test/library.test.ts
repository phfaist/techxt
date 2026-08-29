/**
 * The library's model and its policy (web/PLAN.md §6.10).
 *
 * The point of putting the retention rules in a pure module is that they can be
 * asserted rather than argued about, so the tests that matter most here are the ones
 * that say what the app will *not* do: prune by age, cap the number of entries, or
 * touch a starred one.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  FORK_MIN_REMOVED,
  IDLE_GAP_MS,
  MAX_ENTRY_SOURCE,
  PREVIEW_MAX_CHARS,
  PREVIEW_MAX_LINES,
  QUOTA_WARN_RATIO,
  byRecency,
  createLibrary,
  describeOptions,
  adoptionOnOpen,
  describeSession,
  entryTitle,
  forksEntry,
  estimateBytes,
  filterEntries,
  fingerprint,
  formatBytes,
  makePreview,
  pruneProposal,
  prunableEntries,
  quotaPressure,
  readEntry,
  removedByEdit,
  sameContent,
  statsOf,
  titleIsAutomatic,
} from '../src/library';
import type { LibraryBackend, LibraryEntry } from '../src/library';
import { memoryBackend } from '../src/library-store';
import type { Clock } from '../src/state';
import type { AppOptions } from '../src/types';

/* ------------------------------------------------------------------- fixtures */

/** The same wind-forward clock `state.test.ts` uses; a 2 s debounce costs no time. */
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

/** An entry with everything filled in, so a test only says what it cares about. */
function entry(over: Partial<LibraryEntry> = {}): LibraryEntry {
  return {
    id: 'e1',
    createdAt: 1_000,
    updatedAt: 1_000,
    title: 'A document',
    source: '\\section{A document}',
    options: {},
    starred: false,
    preview: 'A document',
    ...over,
  };
}

let clock: FakeClock;
let backend: LibraryBackend;
/** The clock the library reads for timestamps; a test moves it explicitly. */
let stamp: number;

beforeEach(() => {
  clock = new FakeClock();
  backend = memoryBackend();
  stamp = 10_000;
});

function library(over: Partial<Parameters<typeof createLibrary>[0]> = {}) {
  let counter = 0;
  return createLibrary({
    backend,
    clock,
    delayMs: 2000,
    now: () => stamp,
    newId: () => `id${++counter}`,
    ...over,
  });
}

/* --------------------------------------------------------------------- titles */

describe('entryTitle', () => {
  it('prefers the document heading, then the first line, then the date', () => {
    expect(entryTitle('\\title{On the Electrodynamics}\nbody', new Date(0))).toBe(
      'On the Electrodynamics',
    );
    expect(entryTitle('\\section{Results}\n', new Date(0))).toBe('Results');
    expect(entryTitle('just some words\nand more', new Date(0))).toBe('just some words');
    expect(entryTitle('   \n\n  ', new Date(2026, 7, 28))).toBe('August 28, 2026');
  });

  it('reads through the LaTeX in a heading rather than showing it', () => {
    expect(entryTitle('\\section{The \\LaTeX\\ way}', new Date(0))).toBe('The way');
  });

  it('cuts a very long heading at a word', () => {
    const long = `\\title{${'alpha '.repeat(40)}}`;
    const title = entryTitle(long, new Date(0));
    expect(title.length).toBeLessThanOrEqual(81);
    expect(title.endsWith('…')).toBe(true);
  });
});

describe('titleIsAutomatic', () => {
  it('tells a name the document gave from one the user typed', () => {
    const auto = entry({ title: 'A document', source: '\\section{A document}' });
    expect(titleIsAutomatic(auto)).toBe(true);
    expect(titleIsAutomatic({ ...auto, title: 'My thesis' })).toBe(false);
  });
});

/* -------------------------------------------------------------------- preview */

describe('makePreview', () => {
  it('keeps the shorter of six lines and four hundred characters', () => {
    const many = Array.from({ length: 20 }, (_, index) => `line ${index}`).join('\n');
    expect(makePreview(many).split('\n')).toHaveLength(PREVIEW_MAX_LINES);

    const wide = 'x'.repeat(2000);
    expect(makePreview(wide)).toHaveLength(PREVIEW_MAX_CHARS);
  });

  it('is empty for empty output, and never carries trailing blanks', () => {
    expect(makePreview('')).toBe('');
    expect(makePreview('one\n\n\n')).toBe('one');
  });
});

/* ---------------------------------------------------------------- reading one */

describe('readEntry', () => {
  it('reads what this build writes, round trip', () => {
    const original = entry({ starred: true, options: { math: 'plain' } });
    const read = readEntry(JSON.parse(JSON.stringify(original)), 0);
    expect(read.ok && read.entry).toEqual(original);
  });

  it('accepts the ISO timestamps an export writes', () => {
    const read = readEntry(
      { source: 'x', createdAt: '2026-08-28T12:00:00.000Z', updatedAt: '2026-08-28T13:00:00.000Z' },
      0,
    );
    expect(read.ok && read.entry.createdAt).toBe(Date.parse('2026-08-28T12:00:00.000Z'));
    expect(read.ok && read.entry.updatedAt).toBe(Date.parse('2026-08-28T13:00:00.000Z'));
  });

  it('drops option values it does not know, exactly as a share link does', () => {
    const read = readEntry(
      { source: 'x', options: { math: 'interpretive-dance', keepComments: true, nope: 1 } },
      0,
    );
    expect(read.ok && read.entry.options).toEqual({ keepComments: true });
  });

  it('refuses what is not an entry, and says which rule was broken', () => {
    expect(readEntry(null, 0)).toEqual({ ok: false, reason: 'not-an-object' });
    expect(readEntry([1, 2], 0)).toEqual({ ok: false, reason: 'not-an-object' });
    expect(readEntry({ source: 42 }, 0)).toEqual({ ok: false, reason: 'no-source' });
    expect(readEntry({ source: '' }, 0)).toEqual({ ok: false, reason: 'no-source' });
    expect(readEntry({ source: 'x'.repeat(MAX_ENTRY_SOURCE + 1) }, 0)).toEqual({
      ok: false,
      reason: 'too-large',
    });
  });

  it('invents nothing it was not given but an id, a title and a time', () => {
    const read = readEntry({ source: 'hello world' }, 5_000, () => 'fresh');
    expect(read.ok && read.entry).toMatchObject({
      id: 'fresh',
      createdAt: 5_000,
      updatedAt: 5_000,
      title: 'hello world',
      starred: false,
      preview: '',
    });
  });

  it('never throws, whatever it is handed', () => {
    for (const raw of [undefined, 0, 'text', [], new Date(), { source: {} }]) {
      expect(() => readEntry(raw, 0)).not.toThrow();
    }
  });
});

/* ------------------------------------------------------- listing and searching */

describe('byRecency and filterEntries', () => {
  it('sorts most recently updated first', () => {
    const list = byRecency([
      entry({ id: 'a', updatedAt: 10 }),
      entry({ id: 'b', updatedAt: 30 }),
      entry({ id: 'c', updatedAt: 20 }),
    ]);
    expect(list.map((item) => item.id)).toEqual(['b', 'c', 'a']);
  });

  it('searches title and source, and filters to starred', () => {
    const entries = [
      entry({ id: 'a', title: 'Thesis', source: '\\section{Thesis}', starred: true }),
      entry({ id: 'b', title: 'Notes', source: 'the \\alpha particle' }),
    ];
    expect(filterEntries(entries, 'alpha', false).map((item) => item.id)).toEqual(['b']);
    expect(filterEntries(entries, 'THESIS', false).map((item) => item.id)).toEqual(['a']);
    expect(filterEntries(entries, '', true).map((item) => item.id)).toEqual(['a']);
    expect(filterEntries(entries, 'alpha', true)).toEqual([]);
  });
});

/* ----------------------------------------------------------- content identity */

describe('fingerprint and sameContent', () => {
  it('is the same for the same document under the same options', () => {
    const options: AppOptions = { math: 'plain' };
    expect(fingerprint('body', options)).toBe(fingerprint('body', { math: 'plain' }));
    expect(sameContent(entry({ options }), entry({ id: 'other', options }))).toBe(true);
  });

  it('separates two documents, and one document under two option sets', () => {
    expect(fingerprint('a', {})).not.toBe(fingerprint('b', {}));
    expect(fingerprint('a', {})).not.toBe(fingerprint('a', { math: 'source' }));
    expect(sameContent(entry({ source: 'a' }), entry({ source: 'b' }))).toBe(false);
  });

  it('ignores an option that is the default written out in full', () => {
    // Absent means the default everywhere else, so an entry that spells the default
    // out is the same entry as one that leaves it out (§6.4).
    expect(fingerprint('a', { wrap: 'soft' })).toBe(fingerprint('a', {}));
  });
});

/* ------------------------------------------------------- the retention rules */

describe('retention: nothing is ever dropped for tidiness', () => {
  const old = entry({ id: 'old', updatedAt: 1, starred: false });
  const older = entry({ id: 'older', updatedAt: 0, starred: false });
  const starredAncient = entry({ id: 'star', updatedAt: -1_000_000, starred: true });

  it('never proposes a starred entry, however old it is', () => {
    const proposal = prunableEntries([starredAncient, old, older], 10);
    expect(proposal.map((item) => item.id)).toEqual(['older', 'old']);
    expect(proposal.some((item) => item.starred)).toBe(false);
  });

  it('proposes nothing at all when everything is starred', () => {
    expect(prunableEntries([starredAncient, entry({ starred: true })], 10)).toEqual([]);
  });

  it('offers the oldest first, and only as many as it was asked for', () => {
    const many = Array.from({ length: 10 }, (_, index) =>
      entry({ id: `e${index}`, updatedAt: index }),
    );
    expect(prunableEntries(many, 3).map((item) => item.id)).toEqual(['e0', 'e1', 'e2']);
  });

  it('says what a proposal would cost and what would survive it', () => {
    const proposal = pruneProposal([starredAncient, old, older], 1);
    expect(proposal.entries.map((item) => item.id)).toEqual(['older']);
    expect(proposal.from).toBe(0);
    expect(proposal.to).toBe(0);
    expect(proposal.keeping).toBe(2);
    expect(proposal.keepingStarred).toBe(1);
  });
});

describe('quota', () => {
  it('warns only past the threshold, and says nothing without an estimate', () => {
    expect(quotaPressure(null)).toBe('unknown');
    expect(quotaPressure({ usage: 0, quota: 0 })).toBe('unknown');
    expect(quotaPressure({ usage: 10, quota: 100 })).toBe('ok');
    expect(quotaPressure({ usage: 100 * QUOTA_WARN_RATIO, quota: 100 })).toBe('tight');
  });

  it('counts and sizes a library for its header line', () => {
    const stats = statsOf([entry({ starred: true }), entry({ id: 'b' })]);
    expect(stats.count).toBe(2);
    expect(stats.starred).toBe(1);
    expect(stats.bytes).toBe(estimateBytes([entry({ starred: true }), entry({ id: 'b' })]));
    expect(stats.bytes).toBeGreaterThan(0);
  });

  it('formats a size the way the header shows it', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(900)).toBe('900 B');
    expect(formatBytes(3.1 * 1024 * 1024)).toBe('3.1 MB');
    expect(formatBytes(40 * 1024)).toBe('40 kB');
    expect(formatBytes(-1)).toBe('—');
  });
});

describe('describeOptions', () => {
  it('names only what differs from the app defaults', () => {
    expect(describeOptions({})).toBe('The default options');
    expect(describeOptions({ wrap: 'soft' })).toBe('The default options');
    expect(describeOptions({ math: 'source', wrap: 72 })).toBe('Math: source · Wrap: 72 columns');
  });
});

/* ---------------------------------------------------------- the session model */

describe('the current entry', () => {
  it('logs nothing at all for an empty document', async () => {
    const log = library();
    log.record('   \n ', {}, '');
    clock.advance(5000);
    await log.flush();
    expect(await log.list()).toEqual([]);
  });

  it('creates one entry on the first conversion and updates it in place', async () => {
    const log = library();
    log.record('\\section{Draft}', {}, 'Draft');
    clock.advance(2000);
    await log.flush();

    stamp = 20_000;
    log.record('\\section{Draft}\nmore', {}, 'Draft\nmore');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.source).toBe('\\section{Draft}\nmore');
    expect(entries[0]?.createdAt).toBe(10_000);
    expect(entries[0]?.updatedAt).toBe(20_000);
  });

  it('does not write once per keystroke', async () => {
    const onWrite = vi.fn();
    const log = library({ onWrite });
    for (const text of ['a', 'ab', 'abc', 'abcd']) log.record(text, {}, text);
    clock.advance(2000);
    await log.flush();
    expect(onWrite).toHaveBeenCalledTimes(1);
    expect((await log.list())[0]?.source).toBe('abcd');
  });

  it('updates the current entry when only the options changed', async () => {
    const log = library();
    log.record('body', {}, 'body');
    clock.advance(2000);
    await log.flush();

    log.record('body', { math: 'source' }, 'body');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.options).toEqual({ math: 'source' });
  });

  it('starts a new entry when the document is replaced wholesale', async () => {
    const log = library();
    log.record('first', {}, 'first');
    clock.advance(2000);
    await log.flush();

    log.beginNewEntry();
    stamp = 20_000;
    log.record('second', {}, 'second');
    clock.advance(2000);
    await log.flush();

    expect((await log.list()).map((item) => item.source)).toEqual(['second', 'first']);
  });

  it('still files the last keystrokes of the old document in the old entry', async () => {
    const log = library();
    log.record('first', {}, 'first');
    clock.advance(2000);
    await log.flush();
    // Typed, and then Load ▾ before the two seconds were up.
    log.record('first, edited', {}, 'first, edited');
    log.beginNewEntry();
    log.record('second', {}, 'second');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.map((item) => item.source).sort()).toEqual(['first, edited', 'second']);
  });

  it('lets an entry go when another tab claims it, keeping what was typed here', async () => {
    const sessions: { entryId: string | null; sealed: boolean }[] = [];
    const log = library({ onSession: (session) => sessions.push({ ...session }) });
    log.record('shared', {}, 'shared');
    clock.advance(2000);
    await log.flush();
    const id = (await log.list())[0]!.id;

    // Typed here, and then the other tab claims the entry before the two seconds are
    // up. Those keystrokes belong to the entry they were typed into.
    log.record('shared, edited here', {}, 'shared, edited here');
    log.release();
    clock.advance(2000);
    await log.flush();

    expect((await log.get(id))?.source).toBe('shared, edited here');
    expect(log.session).toEqual({ entryId: null, sealed: false });
    expect(sessions.at(-1)).toEqual({ entryId: null, sealed: false });

    // And what comes next is this tab's own entry, so the two never put one record.
    stamp = 20_000;
    log.record('what this tab does next', {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries[0]?.id).not.toBe(id);
    expect((await log.get(id))?.source).toBe('shared, edited here');
  });

  it('starts a new entry after a long idle gap', async () => {
    const log = library();
    log.record('morning', {}, 'morning');
    clock.advance(2000);
    await log.flush();

    stamp += IDLE_GAP_MS + 1;
    log.record('afternoon', {}, 'afternoon');
    clock.advance(2000);
    await log.flush();

    expect(await log.list()).toHaveLength(2);
  });

  it('keeps one entry across a gap shorter than the idle one', async () => {
    const log = library();
    log.record('morning', {}, 'morning');
    clock.advance(2000);
    await log.flush();

    stamp += IDLE_GAP_MS - 1;
    log.record('a little later', {}, 'a little later');
    clock.advance(2000);
    await log.flush();

    expect(await log.list()).toHaveLength(1);
  });

  it('follows the document title until the entry is renamed, then stops', async () => {
    const log = library();
    log.record('no heading yet', {}, '');
    clock.advance(2000);
    await log.flush();
    expect((await log.list())[0]?.title).toBe('no heading yet');

    log.record('\\section{Now it has one}', {}, '');
    clock.advance(2000);
    await log.flush();
    expect((await log.list())[0]?.title).toBe('Now it has one');

    const id = (await log.list())[0]!.id;
    await log.rename(id, 'My own name');
    log.record('\\section{Changed again}', {}, '');
    clock.advance(2000);
    await log.flush();
    expect((await log.list())[0]?.title).toBe('My own name');
  });

  it('refuses to log a document over the cap, and says so once', async () => {
    const onOversize = vi.fn();
    const log = library({ onOversize });
    log.record('x'.repeat(MAX_ENTRY_SOURCE + 1), {}, '');
    clock.advance(2000);
    await log.flush();
    expect(await log.list()).toEqual([]);
    expect(onOversize).toHaveBeenCalledTimes(1);
    expect(onOversize).toHaveBeenCalledWith(true);

    log.record('small again', {}, '');
    clock.advance(2000);
    await log.flush();
    expect(onOversize).toHaveBeenLastCalledWith(false);
    expect(await log.list()).toHaveLength(1);
  });

  it('continues an entry that was opened from the pane', async () => {
    const log = library();
    log.record('an old document', {}, '');
    clock.advance(2000);
    await log.flush();
    const id = (await log.list())[0]!.id;

    log.beginNewEntry();
    log.record('something else', {}, '');
    clock.advance(2000);
    await log.flush();
    expect(await log.list()).toHaveLength(2);

    log.adopt(id);
    log.record('an old document, revisited', {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.id === id)?.source).toBe('an old document, revisited');
  });
});

/* ------------------------------------------------------- the per-event fork rule */

describe('removedByEdit', () => {
  it('is zero for an insertion, wherever it lands', () => {
    expect(removedByEdit('hello world', 'hello world')).toBe(0);
    expect(removedByEdit('hello world', 'hello world, and more')).toBe(0);
    expect(removedByEdit('hello world', 'well, hello world')).toBe(0);
    expect(removedByEdit('hello world', 'hello brave world')).toBe(0);
  });

  it('counts what one edit took out, not what the two texts differ by', () => {
    expect(removedByEdit('abcdef', 'abdef')).toBe(1);
    expect(removedByEdit('abcdef', '')).toBe(6);
    // Select all, then type one character over it.
    expect(removedByEdit('a long document indeed', 'x')).toBe(22);
  });
});

describe('forksEntry', () => {
  const document = '\\section{Notes}\n'.repeat(40);

  it('never trips on ordinary typing', () => {
    expect(forksEntry(document, `${document}x`)).toBe(false);
    expect(forksEntry(document, document.slice(0, -1))).toBe(false);
    // A paragraph deleted from a long document is still that document.
    expect(forksEntry(document, document.slice(0, document.length - 40))).toBe(false);
  });

  it('never trips on appending or pasting at the end', () => {
    expect(forksEntry(document, document + document)).toBe(false);
  });

  it('trips on select-all-and-paste and on select-all-and-delete', () => {
    expect(forksEntry(document, 'a completely different document, pasted over it')).toBe(true);
    expect(forksEntry(document, '')).toBe(true);
  });

  it('does not fork a buffer too small to be worth an entry', () => {
    const scratch = 'x'.repeat(FORK_MIN_REMOVED - 1);
    expect(forksEntry(scratch, '')).toBe(false);
    expect(forksEntry('ab', 'a')).toBe(false);
  });

  it('measures the event, not the distance from where the document started', () => {
    // A session that rewrites a section at a time crosses any cumulative threshold
    // while genuinely being one document; each of these events is small.
    let text = document;
    for (let round = 0; round < 20; round += 1) {
      const next = text.replace('\\section{Notes}', '\\section{Rewritten}');
      expect(forksEntry(text, next)).toBe(false);
      text = next;
    }
    expect(text).not.toBe(document);
  });
});

/* ------------------------------------------------------------ sealing an entry */

describe('sealing', () => {
  it('stops the entry absorbing edits, and the next edit starts a new one', async () => {
    const log = library();
    log.record('the first document', {}, '');
    clock.advance(2000);
    await log.flush();

    const kept = await log.seal('the first document', {}, '');
    expect(kept?.source).toBe('the first document');
    expect(log.session).toEqual({ entryId: kept?.id, sealed: true });

    stamp = 20_000;
    log.noteEdit('the first document', 'a second document');
    log.record('a second document', {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries.map((item) => item.source)).toEqual(['a second document', 'the first document']);
  });

  it('creates no entry for what comes next until something is typed', async () => {
    const log = library();
    log.record('a document', {}, '');
    clock.advance(2000);
    await log.flush();

    await log.seal('a document', {}, '');
    // Pressing Save and walking away: conversions keep arriving — an option changed,
    // the pane was re-measured — and none of them may leave an empty entry behind.
    log.record('a document', {}, '');
    log.record('a document', { math: 'source' }, '');
    clock.advance(5000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.source).toBe('a document');
  });

  it('writes what is on screen before it seals, not what the last conversion caught', async () => {
    const log = library();
    log.record('half a document', {}, '');
    clock.advance(2000);
    await log.flush();

    // Typed on, and Save pressed before the debounce was up.
    const kept = await log.seal('half a document, finished', {}, '');
    expect(kept?.source).toBe('half a document, finished');
    expect(await log.list()).toHaveLength(1);
  });

  it('seals a document that has not been logged yet, so the button always shows', async () => {
    const log = library();
    const kept = await log.seal('\\section{Never converted twice}', {}, '');
    expect(kept).not.toBeNull();
    expect((await log.list())[0]?.source).toBe('\\section{Never converted twice}');
    expect(log.session.sealed).toBe(true);
  });

  it('is a no-op on an entry that is already sealed', async () => {
    const log = library();
    log.record('a document', {}, '');
    clock.advance(2000);
    await log.flush();
    const first = await log.seal('a document', {}, '');
    const again = await log.seal('a document', {}, '');
    expect(again?.id).toBe(first?.id);
    expect(await log.list()).toHaveLength(1);
  });

  it('says where the document stands, every time it moves', async () => {
    const onSession = vi.fn();
    const log = library({ onSession });
    log.record('a document', {}, '');
    clock.advance(2000);
    await log.flush();
    expect(onSession).toHaveBeenLastCalledWith({ entryId: 'id1', sealed: false });

    await log.seal('a document', {}, '');
    expect(onSession).toHaveBeenLastCalledWith({ entryId: 'id1', sealed: true });

    log.noteEdit('a document', 'a different one');
    expect(onSession).toHaveBeenLastCalledWith({ entryId: null, sealed: false });
  });

  it('keeps the seal when an edit is undone back to the text that was kept', async () => {
    const log = library();
    log.record('kept', {}, '');
    clock.advance(2000);
    await log.flush();
    await log.seal('kept', {}, '');

    expect(log.noteEdit('kept', 'kept?')).toEqual({ kind: 'unsealed', from: 'id1' });
    expect(log.noteEdit('kept?', 'kept')).toEqual({ kind: 'none' });
  });

  it('leaves the sealed entry alone even when a conversion arrives for it', async () => {
    const log = library();
    log.record('kept', {}, '');
    clock.advance(2000);
    await log.flush();
    await log.seal('kept', {}, '');
    log.record('kept', {}, 'a newer rendering');
    clock.advance(2000);
    await log.flush();
    expect((await log.list())[0]?.preview).toBe('');
  });

  it('keeps an entry the user opened from the pane, and starts a new one on the edit', async () => {
    const log = library();
    log.record('the version that was kept', {}, '');
    clock.advance(2000);
    await log.flush();
    const kept = (await log.list())[0]!;

    // What `main.ts` now does for every entry but the live draft (§6.10): the document
    // comes back on screen, and nothing is written until it changes.
    log.adoptSealed(kept);
    expect(log.session).toEqual({ entryId: kept.id, sealed: true });

    expect(log.noteEdit('the version that was kept', 'the version that was kept, edited')).toEqual({
      kind: 'unsealed',
      from: kept.id,
    });
    log.record('the version that was kept, edited', {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.id === kept.id)?.source).toBe('the version that was kept');
  });

  it('comes back to a sealed entry after a reload without logging a second copy', async () => {
    const log = library();
    log.record('a kept document', {}, '');
    clock.advance(2000);
    await log.flush();
    const kept = (await log.list())[0]!;

    // What `main.ts` does when the page comes back holding a document that is already
    // in the log verbatim: continue it, sealed, rather than copy it.
    let made = 0;
    const next = library({ newId: () => `later${(made += 1)}` });
    next.adoptSealed(kept);
    next.record('a kept document', {}, '');
    clock.advance(2000);
    await next.flush();
    expect(await next.list()).toHaveLength(1);

    next.noteEdit('a kept document', 'a kept document, edited');
    next.record('a kept document, edited', {}, '');
    clock.advance(2000);
    await next.flush();
    const entries = await next.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.id === kept.id)?.source).toBe('a kept document');
  });
});

/* --------------------------------------------------------- the automatic fork */

describe('the automatic fork', () => {
  const first = '\\section{The first document}\nwith a body of its own.\n';
  const second = '\\section{Something else entirely}\npasted over the top.\n';

  it('keeps the document that was pasted over — the whole point of item 8', async () => {
    const log = library();
    log.record(first, {}, '');
    clock.advance(2000);
    await log.flush();

    expect(log.noteEdit(first, second)).toEqual({ kind: 'forked', from: 'id1' });
    stamp = 20_000;
    log.record(second, {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries.map((item) => item.source)).toEqual([second, first]);
  });

  it('files the keystrokes that were still pending in the document they belong to', async () => {
    const log = library();
    log.record(first, {}, '');
    clock.advance(2000);
    await log.flush();
    // Typed on, and then pasted over before the two seconds were up.
    log.record(`${first}one more line\n`, {}, '');
    log.noteEdit(`${first}one more line\n`, second);
    log.record(second, {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.id === 'id1')?.source).toBe(`${first}one more line\n`);
  });

  it('does nothing at all when there is no entry to fork away from', () => {
    const log = library();
    expect(log.noteEdit(first, second)).toEqual({ kind: 'none' });
  });

  it('merges back into the previous entry when the fork was wrong', async () => {
    const log = library();
    log.record(first, {}, '');
    clock.advance(2000);
    await log.flush();

    log.noteEdit(first, second);
    log.record(second, {}, '');
    clock.advance(2000);
    await log.flush();
    expect(await log.list()).toHaveLength(2);

    expect(await log.mergeBack('id1')).toBe(true);
    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.id).toBe('id1');
    expect(entries[0]?.source).toBe(second);
    expect(log.session).toEqual({ entryId: 'id1', sealed: false });
  });

  it('merges back before the new draft has been written at all', async () => {
    const log = library();
    log.record(first, {}, '');
    clock.advance(2000);
    await log.flush();

    log.noteEdit(first, second);
    log.record(second, {}, '');
    // The undo comes before the debounce is up: nothing new exists to remove.
    expect(await log.mergeBack('id1')).toBe(true);
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.source).toBe(second);
  });

  it('will not remove a starred entry to merge, whatever else it does', async () => {
    const log = library();
    log.record(first, {}, '');
    clock.advance(2000);
    await log.flush();

    log.noteEdit(first, second);
    log.record(second, {}, '');
    clock.advance(2000);
    await log.flush();
    const forked = (await log.list()).find((item) => item.source === second)!;
    await log.star(forked.id, true);

    await log.mergeBack('id1');
    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.id === forked.id)?.starred).toBe(true);
  });
});

/* ------------------------------------------------------------ the star button */

describe('★', () => {
  it('creates the entry if there is not one yet, so pressing it always shows', async () => {
    const log = library();
    const result = await log.starCurrent('\\section{Unlogged}', {}, 'Unlogged');
    expect(result?.starred).toBe(true);
    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.starred).toBe(true);
  });

  it('is the same button both ways, and seals only once', async () => {
    const log = library();
    await log.starCurrent('body', {}, '');
    expect(log.session.sealed).toBe(true);
    const off = await log.starCurrent('body', {}, '');
    expect(off?.starred).toBe(false);
    const entries = await log.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.starred).toBe(false);
  });

  it('keeps the version it starred: what comes after is a new entry', async () => {
    const log = library();
    await log.starCurrent('body', {}, '');
    log.noteEdit('body', 'body, edited');
    log.record('body, edited', {}, '');
    clock.advance(2000);
    await log.flush();

    const entries = await log.list();
    expect(entries).toHaveLength(2);
    expect(entries.find((item) => item.starred)?.source).toBe('body');
    expect(entries.find((item) => !item.starred)?.source).toBe('body, edited');
  });
});

/* --------------------------------------------------- what opening an entry does */

describe('adoptionOnOpen', () => {
  it('seals every entry but the one being written into', () => {
    // A version the user moved on from: reading it must not put it back under the
    // keyboard, which is the whole of item 8's complaint.
    expect(adoptionOnOpen({ entryId: 'e1', sealed: false }, 'e2')).toBe('sealed');
    expect(adoptionOnOpen({ entryId: null, sealed: false }, 'e2')).toBe('sealed');
    // Already sealed on to it, and opened again: still sealed, not un-sealed by a look.
    expect(adoptionOnOpen({ entryId: 'e1', sealed: true }, 'e1')).toBe('sealed');
  });

  it('leaves the live draft live', () => {
    // Opening the document you are already writing is not moving on from it, so this
    // is the one case that must not fork on the next keystroke.
    expect(adoptionOnOpen({ entryId: 'e1', sealed: false }, 'e1')).toBe('open');
  });
});

/* ------------------------------------------------------ the header's own words */

describe('describeSession', () => {
  it('names the entry being written to, and says what that means', () => {
    const open = describeSession({ entryId: 'e1', sealed: false }, 'Lecture notes');
    expect(open.label).toBe('Lecture notes');
    expect(open.hint).toContain('updates it');

    const sealed = describeSession({ entryId: 'e1', sealed: true }, 'Lecture notes');
    expect(sealed.label).toBe('Lecture notes');
    expect(sealed.hint).toContain('starts a new entry');
  });

  it('says so when there is no entry yet', () => {
    expect(describeSession({ entryId: null, sealed: false }, null).label).toBe('New entry');
    // An entry whose title has not been read back yet is not a wrong name.
    expect(describeSession({ entryId: 'e1', sealed: false }, null).label).toBe('New entry');
  });
});

/* ------------------------------------------------------------- what can fail */

describe('a browser that will not store anything', () => {
  it('is inert rather than broken', async () => {
    const log = createLibrary({ backend: null, clock });
    expect(log.available).toBe(false);
    log.record('body', {}, '');
    clock.advance(5000);
    await expect(log.flush()).resolves.toBeUndefined();
    expect(await log.list()).toEqual([]);
    expect(await log.starCurrent('body', {}, '')).toBeNull();
    expect(await log.remove('nope')).toBeNull();
    expect(await log.clear()).toBe(false);
  });
});

describe('a write that fails', () => {
  function failing(kind: string): LibraryBackend {
    return {
      ...memoryBackend(),
      kind: 'memory',
      put: () => Promise.reject(Object.assign(new Error('nope'), { name: kind })),
    };
  }

  it('is reported rather than swallowed, and the entry is not pretended to exist', async () => {
    const onWriteFailure = vi.fn();
    const log = library({ backend: failing('QuotaExceededError'), onWriteFailure });
    log.record('body', {}, '');
    clock.advance(2000);
    await log.flush();
    expect(onWriteFailure).toHaveBeenCalledWith({ kind: 'quota', message: 'nope' });
    expect(log.currentId).toBeNull();
  });

  it('tells a full disk from a broken one', async () => {
    const onWriteFailure = vi.fn();
    const log = library({ backend: failing('InvalidStateError'), onWriteFailure });
    log.record('body', {}, '');
    clock.advance(2000);
    await log.flush();
    expect(onWriteFailure).toHaveBeenCalledWith({ kind: 'error', message: 'nope' });
  });
});

describe('pausing', () => {
  it('stops logging and keeps what is already there, until it is resumed', async () => {
    const log = library();
    log.record('kept', {}, '');
    clock.advance(2000);
    await log.flush();

    log.pause();
    log.record('not logged', {}, '');
    clock.advance(2000);
    await log.flush();
    // The nuisance, deliberately: the library stopped growing, and nothing that was
    // in it was touched (§6.10).
    expect((await log.list()).map((item) => item.source)).toEqual(['kept']);

    log.resume();
    log.record('logged again', {}, '');
    clock.advance(2000);
    await log.flush();
    expect((await log.list()).map((item) => item.source)).toEqual(['logged again']);
  });
});

/* -------------------------------------------------------- the pane's actions */

describe('the operations the pane performs', () => {
  it('removes one entry and hands it back for Undo', async () => {
    const log = library();
    log.record('body', {}, 'preview');
    clock.advance(2000);
    await log.flush();
    const [only] = await log.list();

    const removed = await log.remove(only!.id);
    expect(removed).toEqual(only);
    expect(await log.list()).toEqual([]);

    expect(await log.restore(removed!)).toBe(true);
    expect(await log.list()).toEqual([only]);
  });

  it('renames, stars and clears', async () => {
    const log = library();
    log.record('body', {}, '');
    clock.advance(2000);
    await log.flush();
    const id = (await log.list())[0]!.id;

    expect((await log.rename(id, '  A better name  '))?.title).toBe('A better name');
    expect((await log.rename(id, '   '))).toBeNull();
    expect((await log.star(id, true))?.starred).toBe(true);
    expect(await log.clear()).toBe(true);
    expect(await log.list()).toEqual([]);
  });

  it('applies an import as one operation', async () => {
    const log = library();
    const incoming = [entry({ id: 'i1' }), entry({ id: 'i2', title: 'Second' })];
    expect(await log.apply(incoming, [])).toBe(true);
    expect((await log.list()).map((item) => item.id).sort()).toEqual(['i1', 'i2']);

    expect(await log.apply([], ['i1'])).toBe(true);
    expect((await log.list()).map((item) => item.id)).toEqual(['i2']);
  });
});
