/**
 * What one tab tells the others (web/PLAN.md §6.10).
 *
 * Node has a `BroadcastChannel` with the same delivery rule the browser has, so the
 * one property the claim rule rests on — *the sender never hears itself* — is
 * asserted here against a real channel rather than described in a comment. Get that
 * wrong and every tab releases the entry it just claimed, which is a bug no unit test
 * of the message codec would ever catch.
 *
 * **Nothing below waits a fixed number of turns of the event loop**, and the history
 * is worth keeping: it used to, and the file failed about one full-suite run in five.
 * A message posted here is delivered on a later task, and one `setTimeout(…, 0)` is
 * enough to see it — measured, five hundred times out of five hundred — right up
 * until the process is busy. Vitest runs test files in parallel workers in one
 * process, so this file's turn of the loop is shared with everything else running,
 * and under that load the timer fires before the channel's own task has been drained.
 * The tab helper below therefore waits for *the message*, and the timeout it carries
 * exists to make a message that never arrives read as a failure rather than a hang.
 */

import { afterEach, describe, expect, it } from 'vitest';

import { openLibraryChannel, readMessage } from '../src/library-sync';
import type { LibraryChannel, LibraryMessage } from '../src/library-sync';

/* ------------------------------------------------------------------- fixtures */

const open: LibraryChannel[] = [];

/**
 * How long a message is given before the test calls it lost.
 *
 * Generous on purpose, because it is not a wait: a passing test never spends any of
 * it, and what it buys is a broken channel reported as a failure naming what was
 * heard, rather than as a suite that hangs until vitest's own timeout kills it.
 */
const DELIVERY_TIMEOUT = 2000;

interface Tab {
  channel: LibraryChannel;
  /** Everything this tab has heard from the others, in order. */
  heard: LibraryMessage[];
  /** Resolves once this tab has heard `count` messages; rejects if it never does. */
  hears(count?: number): Promise<void>;
}

/** A tab: its channel, everything it has heard, and a way to wait for the next thing. */
function tab(): Tab {
  const heard: LibraryMessage[] = [];
  const waiting = new Set<() => void>();
  const channel = openLibraryChannel((message) => {
    heard.push(message);
    for (const wake of [...waiting]) wake();
  });
  if (!channel) throw new Error('this node has no BroadcastChannel');
  open.push(channel);

  return {
    channel,
    heard,
    hears(count = 1) {
      return new Promise<void>((resolve, reject) => {
        const enough = (): boolean => heard.length >= count;
        // Already there: a message can arrive before anyone asks to wait for it.
        if (enough()) {
          resolve();
          return;
        }
        const wake = (): void => {
          if (!enough()) return;
          waiting.delete(wake);
          clearTimeout(timer);
          resolve();
        };
        const timer = setTimeout(() => {
          waiting.delete(wake);
          reject(
            new Error(
              `waited ${DELIVERY_TIMEOUT} ms for ${count} message(s), heard ${heard.length}: ${JSON.stringify(heard)}`,
            ),
          );
        }, DELIVERY_TIMEOUT);
        waiting.add(wake);
      });
    },
  };
}

/**
 * A few turns of the loop, for anything that should *not* arrive to arrive.
 *
 * Used only after the message a test is waiting for has already been heard, and only
 * to catch a delivery the channel should never have made at all. Too short a settle
 * can let such a bug pass; it cannot turn a working channel into a failure, which is
 * the only direction that makes a test flaky.
 */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 3; turn += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

afterEach(() => {
  for (const channel of open.splice(0)) channel.close();
});

/* -------------------------------------------------------------------- the codec */

describe('readMessage', () => {
  it('reads the two messages there are', () => {
    expect(readMessage({ kind: 'changed' })).toEqual({ kind: 'changed' });
    expect(readMessage({ kind: 'claim', id: 'e1' })).toEqual({ kind: 'claim', id: 'e1' });
  });

  it('drops everything else rather than throwing, the way a stored entry is read', () => {
    expect(readMessage(null)).toBeNull();
    expect(readMessage(undefined)).toBeNull();
    expect(readMessage('claim')).toBeNull();
    expect(readMessage(42)).toBeNull();
    expect(readMessage([])).toBeNull();
    // A build this one does not know, which is the case the codec exists for.
    expect(readMessage({ kind: 'reticulate', id: 'e1' })).toBeNull();
  });

  it('refuses a claim without a usable id', () => {
    expect(readMessage({ kind: 'claim' })).toBeNull();
    expect(readMessage({ kind: 'claim', id: '' })).toBeNull();
    expect(readMessage({ kind: 'claim', id: 7 })).toBeNull();
    expect(readMessage({ kind: 'claim', id: 'x'.repeat(200) })).toBeNull();
  });

  it('keeps only the fields it knows', () => {
    expect(readMessage({ kind: 'claim', id: 'e1', evil: true })).toEqual({
      kind: 'claim',
      id: 'e1',
    });
  });
});

/* ------------------------------------------------------------------ the channel */

describe('openLibraryChannel', () => {
  it('carries a message to the other tabs', async () => {
    const a = tab();
    const b = tab();
    const c = tab();

    a.channel.post({ kind: 'claim', id: 'e1' });
    await Promise.all([b.hears(), c.hears()]);

    expect(b.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
    expect(c.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
  });

  it('never delivers to the tab that posted — the whole of the claim rule', async () => {
    const a = tab();
    const b = tab();

    a.channel.post({ kind: 'claim', id: 'e1' });
    b.channel.post({ kind: 'changed' });
    // Each has the one message it should have. A tab hearing itself would be a
    // *second* one, so the assertions are on the whole array after a settle rather
    // than on the first thing to arrive.
    await Promise.all([a.hears(), b.hears()]);
    await settle();

    expect(a.heard).toEqual([{ kind: 'changed' }]);
    expect(b.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
  });

  it('says nothing to a tab that has closed its channel', async () => {
    const a = tab();
    const b = tab();
    // The one assertion here has nothing of its own to wait for, so a third tab does
    // the waiting: once the witness has the message, this post has been delivered,
    // and b's silence is a fact about its closed channel rather than about how soon
    // the test looked. Opened after b so that it is behind b in the delivery order —
    // and the settle after it covers the case where it is not.
    const witness = tab();

    b.channel.close();
    a.channel.post({ kind: 'changed' });
    await witness.hears();
    await settle();

    expect(b.heard).toEqual([]);
  });

  it('is silent rather than fatal when posting after a close', () => {
    const a = tab();
    a.channel.close();
    expect(() => a.channel.post({ kind: 'changed' })).not.toThrow();
    expect(() => a.channel.close()).not.toThrow();
  });
});
