/**
 * Library export and import (web/PLAN.md §6.11).
 *
 * The format is versioned and boring on purpose: a JSON object with a `format` tag, a
 * version, when and by what it was written, and the items. It carries each entry's
 * preview so an imported library is legible before anything has been re-converted,
 * which is the strongest argument for keeping a preview genuinely small.
 *
 * Two rules govern this file, and both are worth stating before the code:
 *
 * - **An import never removes an existing entry unless the user chose Replace on
 *   that particular import.** {@link planImport} is the only thing that decides what
 *   an import does, and outside `mode: 'replace'` its `remove` list is empty by
 *   construction. There is no heuristic, no de-duplication pass and no exception:
 *   "skip items I already have" *skips the incoming one*, it never touches the one
 *   that is already there.
 * - **A file is hostile input.** It arrived from somewhere else and may be truncated,
 *   foreign, from a future build, or hand-edited. Every field goes through the same
 *   validators the share codec uses, unknown fields are dropped, sizes are capped and
 *   nothing here throws: a refusal says which rule the file broke, in words a person
 *   can act on.
 */

import { entryFingerprint, randomId, readEntry, sameContent } from './library';
import type { LibraryEntry } from './library';
import { pruneOptions } from './state';

/* -------------------------------------------------------------- the format */

/** The `format` tag. A file without exactly this string is not ours. */
export const LIBRARY_FORMAT = 'techxt.library';

/** The version this build writes, and the only one it reads. */
export const LIBRARY_VERSION = 1;

/** The app name written into an export, for a reader trying to place the file. */
export const LIBRARY_APP = 'techxt-web';

/**
 * The largest file this build will look at, in characters.
 *
 * The cap is not tidiness — it is the one number in this file that could refuse a
 * user's own data, so it is set where nothing a person could plausibly have made can
 * reach it and a browser could not hold the string anyway. `JSON.parse` over an
 * arbitrarily large string is a way to hang a tab, and a browser's own maximum string
 * length is not far above this.
 */
export const MAX_IMPORT_CHARS = 128 * 1024 * 1024;

/** One entry as it appears in a file: timestamps as ISO 8601, everything else plain. */
interface FileEntry {
  id: string;
  createdAt: string;
  updatedAt: string;
  title: string;
  source: string;
  options: Record<string, unknown>;
  starred: boolean;
  preview: string;
}

interface LibraryFile {
  format: typeof LIBRARY_FORMAT;
  v: number;
  exportedAt: string;
  app: string;
  techxt: string;
  items: FileEntry[];
}

/* --------------------------------------------------------------- exporting */

function iso(time: number): string {
  const date = new Date(time);
  return Number.isFinite(date.getTime()) ? date.toISOString() : new Date(0).toISOString();
}

/**
 * The whole library as one JSON document.
 *
 * Timestamps become ISO 8601 strings rather than epoch numbers: the file is meant to
 * be readable by a person who opens it in an editor a year from now, and
 * `"2026-08-28T12:00:00.000Z"` says what `1787832000000` does not.
 */
export function encodeLibrary(
  entries: readonly LibraryEntry[],
  meta: { exportedAt: Date; techxt: string },
): string {
  const file: LibraryFile = {
    format: LIBRARY_FORMAT,
    v: LIBRARY_VERSION,
    exportedAt: iso(meta.exportedAt.getTime()),
    app: LIBRARY_APP,
    techxt: meta.techxt,
    items: entries.map((entry) => ({
      id: entry.id,
      createdAt: iso(entry.createdAt),
      updatedAt: iso(entry.updatedAt),
      title: entry.title,
      source: entry.source,
      options: pruneOptions(entry.options) as Record<string, unknown>,
      starred: entry.starred,
      preview: entry.preview,
    })),
  };
  return `${JSON.stringify(file, null, 2)}\n`;
}

/** `techxt-library-2026-08-28.json`, in the local date the person is having. */
export function libraryFileName(date: Date): string {
  const pad = (n: number): string => String(n).padStart(2, '0');
  return `techxt-library-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}.json`;
}

/* --------------------------------------------------------------- importing */

/** What a file turned out to hold, once every item had been through the validators. */
export interface DecodedLibrary {
  entries: LibraryEntry[];
  /** When the file says it was written, verbatim, or `null`. */
  exportedAt: string | null;
  /** Which techxt wrote it, or `null`. */
  techxt: string | null;
  /** Items that were in the file and are not in `entries`, and why. */
  dropped: { malformed: number; oversize: number };
}

export type DecodeResult = { ok: true; library: DecodedLibrary } | { ok: false; reason: string };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Read a file the user chose. Never throws; a refusal names what was wrong with it.
 *
 * The refusals are deliberately specific. "That file is not a techxt library" and
 * "that file was written by a newer version of techxt" send a person to different
 * places, and a single "could not read that" would send them nowhere.
 */
export function decodeLibrary(
  text: string,
  now: number,
  // A parameter so the cap can be exercised without building a file the size of the
  // cap, the way `now` and `newId` are parameters elsewhere in this app.
  options: { maxChars?: number } = {},
): DecodeResult {
  if (typeof text !== 'string' || text.trim() === '') {
    return { ok: false, reason: 'That file is empty.' };
  }
  if (text.length > (options.maxChars ?? MAX_IMPORT_CHARS)) {
    return {
      ok: false,
      reason: 'That file is far larger than any library this app can have written.',
    };
  }

  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return {
      ok: false,
      reason: 'That file is not readable as JSON — it may have been truncated or edited.',
    };
  }

  if (!isRecord(raw)) return { ok: false, reason: 'That file does not contain a library.' };
  if (raw['format'] !== LIBRARY_FORMAT) {
    return { ok: false, reason: 'That file is not a techxt library export.' };
  }
  const version = raw['v'];
  if (typeof version !== 'number' || !Number.isFinite(version)) {
    return { ok: false, reason: 'That library file does not say which format version it is.' };
  }
  if (version > LIBRARY_VERSION) {
    return {
      ok: false,
      reason: `That library was written in format v${version}; this build reads v${LIBRARY_VERSION}. Update techxt and try again.`,
    };
  }
  if (version < LIBRARY_VERSION) {
    return {
      ok: false,
      reason: `That library is in format v${version}, which this build no longer reads.`,
    };
  }
  const items = raw['items'];
  if (!Array.isArray(items)) {
    return { ok: false, reason: 'That library file has no list of items in it.' };
  }

  const entries: LibraryEntry[] = [];
  const dropped = { malformed: 0, oversize: 0 };
  for (const item of items) {
    const read = readEntry(item, now);
    if (read.ok) {
      entries.push(read.entry);
    } else if (read.reason === 'too-large') {
      // Never truncated to fit: an entry that will not fit is reported, not mangled.
      dropped.oversize += 1;
    } else {
      dropped.malformed += 1;
    }
  }

  if (entries.length === 0 && items.length > 0) {
    return { ok: false, reason: 'Every item in that library file was unreadable.' };
  }

  return {
    ok: true,
    library: {
      entries,
      exportedAt: typeof raw['exportedAt'] === 'string' ? raw['exportedAt'] : null,
      techxt: typeof raw['techxt'] === 'string' ? raw['techxt'] : null,
      dropped,
    },
  };
}

/* ------------------------------------------------------------- the decision */

/** What the user chose in the import dialog. */
export type ImportMode = 'add' | 'replace';

export interface ImportChoice {
  mode: ImportMode;
  /**
   * Skip an incoming item the library already has, matched on the *content* — the
   * source and the options — rather than on the id. Only meaningful for `'add'`;
   * after a Replace there is nothing left to have.
   */
  skipExisting: boolean;
}

export interface ImportPlan {
  mode: ImportMode;
  /** The entries to write. */
  put: LibraryEntry[];
  /**
   * The ids to remove. **Empty unless the user chose Replace** — see this file's
   * header, and the test that holds it to that.
   */
  remove: string[];
  added: number;
  skipped: number;
  /** How many existing entries Replace is giving up; zero for every other mode. */
  replaced: number;
  /** What Replace would cost, for the confirmation that has to name it. */
  losing: { count: number; starred: number };
}

/**
 * Turn a decoded file plus the user's answer into exactly what will be written and
 * what — only under Replace — will be removed.
 *
 * The interesting cases are all id collisions, and none of them overwrite anything:
 * an incoming entry whose id is already taken gets a fresh one, so importing a
 * library into itself doubles it rather than silently merging it. That is the
 * behaviour "Add to my library" promises, and de-duplication is a *choice* the user
 * makes with the checkbox rather than something inferred on their behalf.
 */
export function planImport(
  existing: readonly LibraryEntry[],
  incoming: readonly LibraryEntry[],
  choice: ImportChoice,
  options: { newId?: () => string } = {},
): ImportPlan {
  const newId = options.newId ?? randomId;
  const losing = {
    count: existing.length,
    starred: existing.reduce((n, entry) => n + (entry.starred ? 1 : 0), 0),
  };

  if (choice.mode === 'replace') {
    // The one path that removes anything, and only because the user chose it by name
    // in a dialog that told them what it would cost.
    const put: LibraryEntry[] = [];
    const taken = new Set<string>();
    for (const entry of incoming) {
      const id = taken.has(entry.id) ? newId() : entry.id;
      taken.add(id);
      put.push({ ...entry, id });
    }
    return {
      mode: 'replace',
      put,
      remove: existing.map((entry) => entry.id),
      added: put.length,
      skipped: 0,
      replaced: existing.length,
      losing,
    };
  }

  const taken = new Set(existing.map((entry) => entry.id));
  const byContent = new Map<string, LibraryEntry[]>();
  if (choice.skipExisting) {
    for (const entry of existing) {
      const key = entryFingerprint(entry);
      const bucket = byContent.get(key);
      if (bucket) bucket.push(entry);
      else byContent.set(key, [entry]);
    }
  }

  const put: LibraryEntry[] = [];
  let skipped = 0;
  for (const entry of incoming) {
    if (choice.skipExisting) {
      const bucket = byContent.get(entryFingerprint(entry));
      // The fingerprint is a hash; the comparison is what makes the match a fact.
      if (bucket?.some((candidate) => sameContent(candidate, entry))) {
        skipped += 1;
        continue;
      }
    }
    const id = taken.has(entry.id) ? newId() : entry.id;
    taken.add(id);
    const copy = { ...entry, id };
    put.push(copy);
    if (choice.skipExisting) {
      const key = entryFingerprint(copy);
      const bucket = byContent.get(key);
      // A file that repeats the same document twice under "skip what I have" should
      // land once, so what has just been added counts as had.
      if (bucket) bucket.push(copy);
      else byContent.set(key, [copy]);
    }
  }

  return {
    mode: 'add',
    put,
    remove: [],
    added: put.length,
    skipped,
    replaced: 0,
    losing,
  };
}

/** "12 added, 3 skipped, 0 replaced." — what the app says when an import lands. */
export function describeImport(plan: ImportPlan, dropped?: DecodedLibrary['dropped']): string {
  const parts = [
    `${plan.added} added`,
    `${plan.skipped} skipped`,
    `${plan.replaced} replaced`,
  ];
  let sentence = `${parts.join(', ')}.`;
  const unreadable = (dropped?.malformed ?? 0) + (dropped?.oversize ?? 0);
  if (unreadable > 0) {
    sentence += ` ${unreadable} item${unreadable === 1 ? '' : 's'} in the file could not be read.`;
  }
  return sentence;
}
