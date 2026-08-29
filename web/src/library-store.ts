/**
 * The library's storage: IndexedDB, and the browser facts around it (web/PLAN.md
 * §6.10).
 *
 * This is the only file in the app that opens a database. Everything above it works
 * against {@link LibraryBackend} in `library.ts`, which is why the retention policy,
 * the import codec and the pane are all testable in node while this file is not.
 *
 * Why IndexedDB and not `localStorage`: the session state and the library must not be
 * able to exhaust each other. `localStorage` holds a few kilobytes of settings and a
 * document, synchronously, on a budget of about five megabytes; the library holds
 * whole documents and previews and wants the origin's real quota. Keeping them in
 * separate stores means a large library can never cost the user their settings.
 *
 * Everything here degrades the way `browserStorage()` does for `localStorage`: a
 * browser without IndexedDB, or one that refuses to open a database — a locked-down
 * profile, some private windows — produces `null` rather than an exception, and the
 * app shows an honest "not available here" state instead of a broken button.
 */

import type { LibraryBackend, LibraryEntry, StorageEstimate } from './library';

/** One database, one object store, one version. */
export const DB_NAME = 'techxt';
export const DB_VERSION = 1;
export const STORE_NAME = 'library';

/**
 * The record as it goes into the store: the entry, plus one derived field.
 *
 * `star` exists because IndexedDB cannot index a boolean — a key may be a number, a
 * string, a date, binary or an array of those, and a record whose indexed value is
 * none of them is simply left out of the index. Storing 0/1 beside the boolean is
 * what makes "the starred ones" a query rather than a scan. It is stripped on the way
 * out so nothing above this file ever sees it, and nothing above this file may write
 * it.
 */
interface StoredEntry extends LibraryEntry {
  star: 0 | 1;
}

function toStored(entry: LibraryEntry): StoredEntry {
  return { ...entry, star: entry.starred ? 1 : 0 };
}

function fromStored(raw: unknown): LibraryEntry | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const { star: _star, ...entry } = raw as StoredEntry;
  return entry;
}

/** A request as a promise, with its error rather than an `Event` on the failure. */
function ask<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = (): void => resolve(request.result);
    request.onerror = (): void => reject(request.error ?? new Error('IndexedDB request failed'));
  });
}

/**
 * A transaction as a promise that settles when it *completes*, not when the last
 * request succeeds.
 *
 * The difference is the whole point: a quota failure surfaces on the transaction's
 * `abort`/`error`, after every individual `put` has reported success. Resolving early
 * would mean telling the user their work was saved and being wrong.
 */
function done(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = (): void => resolve();
    transaction.onabort = (): void =>
      reject(transaction.error ?? new Error('the write was aborted'));
    transaction.onerror = (): void =>
      reject(transaction.error ?? new Error('the write failed'));
  });
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = (): void => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, { keyPath: 'id' });
        // Recency is the order the pane always shows, and starred is its one filter.
        store.createIndex('updatedAt', 'updatedAt');
        store.createIndex('star', 'star');
      }
    };
    request.onsuccess = (): void => resolve(request.result);
    request.onerror = (): void => reject(request.error ?? new Error('IndexedDB refused to open'));
    // Firefox in a private window, and a profile with storage disabled, answer by
    // never firing any handler; the caller's timeout below is what saves the app from
    // waiting for a database that is not coming.
    //
    // `blocked` is *not* one of those cases and must not be treated as a failure: it
    // says another tab is holding an older version open, and every tab from this
    // build on closes its connection when it hears `versionchange` below. So the open
    // is a moment away from succeeding, and rejecting here would hand a user with two
    // tabs the "no library in this browser" state on the day `DB_VERSION` first
    // moves. A tab running a build too old to let go is the case the timeout covers,
    // and it is then the honest answer rather than a wrong one.
    request.onblocked = (): void => {
      /* waiting for the other tab to close; `withTimeout` bounds the wait */
    };
  });
}

/** How long to wait for a database before deciding this browser has not got one. */
const OPEN_TIMEOUT_MS = 4000;

function withTimeout<T>(work: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('IndexedDB did not answer')), ms);
    work.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error instanceof Error ? error : new Error(String(error)));
      },
    );
  });
}

/**
 * The real backend, or `null` where this browser will not give us a database.
 *
 * `null` is not an error state: it is the honest answer for a locked-down profile,
 * and the pane says so rather than offering buttons that cannot work.
 *
 * `onClosed` is called if the connection is given up while the page is still running,
 * which happens for exactly one reason: another tab is upgrading the database and
 * this one is in its way. Holding on would leave that tab with no library at all
 * (its open fires `blocked` and never completes), so this one lets go — and every
 * call on the backend fails from then on, which is what the callback exists to say
 * before the failures start arriving as toasts.
 */
export async function openLibraryBackend(
  onClosed?: () => void,
): Promise<LibraryBackend | null> {
  try {
    if (typeof indexedDB === 'undefined') return null;
    const db = await withTimeout(openDatabase(), OPEN_TIMEOUT_MS);
    // A connection that stays open across a version change blocks the tab making it
    // *indefinitely*: there is no timeout on the other side of `blocked`, only this
    // handler. Closing is not optional politeness, it is the whole of the protocol.
    db.onversionchange = (): void => {
      db.close();
      onClosed?.();
    };
    return indexedDbBackend(db);
  } catch {
    return null;
  }
}

function indexedDbBackend(db: IDBDatabase): LibraryBackend {
  function store(mode: IDBTransactionMode): { transaction: IDBTransaction; store: IDBObjectStore } {
    const transaction = db.transaction(STORE_NAME, mode);
    return { transaction, store: transaction.objectStore(STORE_NAME) };
  }

  return {
    kind: 'indexeddb',

    async all() {
      const { store: objects } = store('readonly');
      const raw = await ask(objects.getAll());
      const entries: LibraryEntry[] = [];
      for (const record of raw) {
        // A record an older build wrote, or a hand-edited one, is skipped rather than
        // allowed to break the whole listing.
        const entry = fromStored(record);
        if (entry && typeof entry.id === 'string' && typeof entry.source === 'string') {
          entries.push(entry);
        }
      }
      return entries;
    },

    async get(id) {
      const { store: objects } = store('readonly');
      return fromStored(await ask(objects.get(id)));
    },

    async put(entries) {
      const { transaction, store: objects } = store('readwrite');
      for (const entry of entries) objects.put(toStored(entry));
      await done(transaction);
    },

    async remove(ids) {
      const { transaction, store: objects } = store('readwrite');
      for (const id of ids) objects.delete(id);
      await done(transaction);
    },

    async clear() {
      const { transaction, store: objects } = store('readwrite');
      objects.clear();
      await done(transaction);
    },
  };
}

/**
 * A backend in memory, for tests and for nothing else.
 *
 * It is deliberately *not* the fallback for a browser without IndexedDB: a library
 * that forgets everything on reload while the pane says it saved would be a lie, and
 * the honest inert state is the better answer.
 */
export function memoryBackend(seed: readonly LibraryEntry[] = []): LibraryBackend {
  const entries = new Map<string, LibraryEntry>(seed.map((entry) => [entry.id, entry]));
  return {
    kind: 'memory',
    all: () => Promise.resolve([...entries.values()].map((entry) => ({ ...entry }))),
    get: (id) => Promise.resolve(entries.has(id) ? { ...entries.get(id)! } : null),
    put: (put) => {
      for (const entry of put) entries.set(entry.id, { ...entry });
      return Promise.resolve();
    },
    remove: (ids) => {
      for (const id of ids) entries.delete(id);
      return Promise.resolve();
    },
    clear: () => {
      entries.clear();
      return Promise.resolve();
    },
  };
}

/* ------------------------------------------------------------ the quota story */

/**
 * Ask the browser to stop treating this origin's data as evictable.
 *
 * An installed PWA usually gets this without asking; asking costs nothing and is not
 * a permission prompt in any current browser. A `false` is worth knowing — it is one
 * of the signals that this is a session whose data will not survive — but it is never
 * a reason to refuse to store anything.
 */
export async function requestPersistence(): Promise<boolean> {
  try {
    if (!navigator.storage?.persist) return false;
    if (await navigator.storage.persisted?.()) return true;
    return await navigator.storage.persist();
  } catch {
    return false;
  }
}

/** Whether the browser has already promised to keep this origin's data. */
export async function isPersisted(): Promise<boolean> {
  try {
    return (await navigator.storage?.persisted?.()) ?? false;
  } catch {
    return false;
  }
}

/**
 * `navigator.storage.estimate()`, or `null` where there is no answer.
 *
 * The numbers are deliberately fuzzy — browsers round and pad them so a page cannot
 * fingerprint a disk — which is fine for the one question they are asked here: is
 * this origin near enough to its ceiling to be worth one line of warning?
 */
export async function storageEstimate(): Promise<StorageEstimate | null> {
  try {
    const estimate = await navigator.storage?.estimate?.();
    if (!estimate || typeof estimate.quota !== 'number' || typeof estimate.usage !== 'number') {
      return null;
    }
    return { usage: estimate.usage, quota: estimate.quota };
  } catch {
    return null;
  }
}
