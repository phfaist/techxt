/**
 * What one copy of the app tells the others (web/PLAN.md §6.10).
 *
 * Two tabs on the same origin share one library and one `localStorage`, and until
 * this file existed they shared them by standing on each other. Nothing here makes
 * them collaborate — that would be a document-sync problem, and this is not one. It
 * makes them stop overwriting each other, which needs exactly two messages:
 *
 * - **`changed`** — the set of entries moved, so a pane still showing the old set
 *   should re-read it. Sent for a create, a delete, a star, a rename, an import and a
 *   clear. Deliberately *not* sent for the two-second in-place update of the entry
 *   being typed into: that would have every other tab load every entry in full
 *   (§6.10, "the pane loads every entry") every two seconds, to refresh a preview
 *   that §6.10 already allows to be stale.
 * - **`claim`** — this tab is now writing into this entry. A tab that hears a claim
 *   on the entry *it* is writing into gives it up. This is the one that matters: two
 *   tabs updating one entry in place is the only interference that costs the user
 *   work, because a write puts the whole record back and the loser's document, title
 *   and star go with it.
 *
 * **A `BroadcastChannel` never delivers to the object that posted**, which is what
 * makes the claim rule safe to state so bluntly: a tab cannot hear — and so cannot
 * release on — its own claim. One channel object per tab is therefore load-bearing,
 * and `openLibraryChannel` is the only thing in the app that makes one.
 *
 * Everything degrades the way `browserStorage()` and `openLibraryBackend()` do: a
 * browser without `BroadcastChannel`, or one that refuses to open one, produces
 * `null` rather than an exception, and the app is then exactly as good as it was
 * before this file — one tab per library, last writer wins.
 */

/** The channel's name carries its message set's version, so a v2 is free to differ. */
export const CHANNEL_NAME = 'techxt.library.v1';

/**
 * The set of entries changed, or one tab took over an entry.
 *
 * There is no `from` field and no tab identity anywhere in this protocol: the sender
 * never hears itself, so "who sent this" is only ever "one of the others", and that
 * is the whole of what either rule needs to know.
 */
export type LibraryMessage = { kind: 'changed' } | { kind: 'claim'; id: string };

/**
 * A message from another tab, or `null`.
 *
 * A read never throws here for the same reason `readEntry` does not: the sender is
 * same-origin but it is not necessarily the *same build*, and a message shape this
 * one does not know is a message this one ignores rather than one that breaks it.
 */
export function readMessage(raw: unknown): LibraryMessage | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const kind = (raw as { kind?: unknown }).kind;
  if (kind === 'changed') return { kind: 'changed' };
  if (kind === 'claim') {
    const id = (raw as { id?: unknown }).id;
    // The same bound `readCurrentEntryId` puts on a stored id: an id is a short
    // opaque string, and anything else is not one.
    if (typeof id !== 'string' || id === '' || id.length >= 200) return null;
    return { kind: 'claim', id };
  }
  return null;
}

export interface LibraryChannel {
  post(message: LibraryMessage): void;
  close(): void;
}

/**
 * The channel, or `null` where this browser has not got one.
 *
 * `post` swallows its failures on purpose: a message the other tabs do not get costs
 * a stale pane, and there is nothing the user could do with the news.
 */
export function openLibraryChannel(
  onMessage: (message: LibraryMessage) => void,
): LibraryChannel | null {
  try {
    if (typeof BroadcastChannel === 'undefined') return null;
    const channel = new BroadcastChannel(CHANNEL_NAME);
    channel.onmessage = (event: MessageEvent): void => {
      const message = readMessage(event.data);
      if (message) onMessage(message);
    };
    return {
      post(message) {
        try {
          channel.postMessage(message);
        } catch {
          /* a message the others do not get is a stale pane, and nothing worse */
        }
      },
      close() {
        try {
          channel.close();
        } catch {
          /* a channel that will not close is a channel the page is done with */
        }
      },
    };
  } catch {
    return null;
  }
}
