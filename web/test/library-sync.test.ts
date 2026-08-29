/**
 * What one tab tells the others (web/PLAN.md §6.10).
 *
 * Node has a `BroadcastChannel` with the same delivery rule the browser has, so the
 * one property the claim rule rests on — *the sender never hears itself* — is
 * asserted here against a real channel rather than described in a comment. Get that
 * wrong and every tab releases the entry it just claimed, which is a bug no unit test
 * of the message codec would ever catch.
 */

import { afterEach, describe, expect, it } from 'vitest';

import { openLibraryChannel, readMessage } from '../src/library-sync';
import type { LibraryChannel, LibraryMessage } from '../src/library-sync';

/* ------------------------------------------------------------------- fixtures */

const open: LibraryChannel[] = [];

/** A tab: its channel, and everything it has heard from the others. */
function tab(): { channel: LibraryChannel; heard: LibraryMessage[] } {
  const heard: LibraryMessage[] = [];
  const channel = openLibraryChannel((message) => heard.push(message));
  if (!channel) throw new Error('this node has no BroadcastChannel');
  open.push(channel);
  return { channel, heard };
}

/** Messages are delivered on a later task; one turn of the loop is enough. */
function delivered(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
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
    await delivered();

    expect(b.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
    expect(c.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
  });

  it('never delivers to the tab that posted — the whole of the claim rule', async () => {
    const a = tab();
    const b = tab();

    a.channel.post({ kind: 'claim', id: 'e1' });
    b.channel.post({ kind: 'changed' });
    await delivered();

    expect(a.heard).toEqual([{ kind: 'changed' }]);
    expect(b.heard).toEqual([{ kind: 'claim', id: 'e1' }]);
  });

  it('says nothing to a tab that has closed its channel', async () => {
    const a = tab();
    const b = tab();

    b.channel.close();
    a.channel.post({ kind: 'changed' });
    await delivered();

    expect(b.heard).toEqual([]);
  });

  it('is silent rather than fatal when posting after a close', () => {
    const a = tab();
    a.channel.close();
    expect(() => a.channel.post({ kind: 'changed' })).not.toThrow();
    expect(() => a.channel.close()).not.toThrow();
  });
});
