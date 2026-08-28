/**
 * The library: an automatic log of the documents that have been through the app
 * (web/PLAN.md §6.10).
 *
 * This file is the model and the policy — the shape of an entry, what a read of an
 * untrusted one is allowed to produce, when a new entry begins, and what the app is
 * and is not allowed to do when storage runs out. It touches no DOM and no
 * `indexedDB`: the backend arrives as a parameter (`src/library-store.ts` supplies
 * the real one), so every rule below is reachable from vitest's `node` environment.
 *
 * Three properties are load-bearing, and the rest of the design serves them:
 *
 * - **Nothing is ever deleted for tidiness.** There is no age cutoff, no entry cap
 *   and no scheduled prune anywhere in this file. An entry leaves the library
 *   because the user removed it, because they chose Replace on an import, or because
 *   they agreed — in as many words, in a dialog that named the entries — to give up
 *   the oldest unstarred ones when the disk was genuinely full.
 * - **A starred entry is never removed by any automatic mechanism.**
 *   {@link prunableEntries} is the only thing in the app that ever proposes a
 *   removal, and it cannot see a starred entry.
 * - **A read never throws.** {@link readEntry} is to the library what
 *   `sanitizeOptions` is to a share link: every field is checked, unknown fields are
 *   dropped, and a refusal says which rule the data broke.
 */

import { formatToday, pruneOptions, sanitizeOptions, systemClock } from './state';
import type { Clock } from './state';
import { MAX_TITLE, documentTitle, firstLine, shorten } from './title';
import type { AppOptions } from './types';

/* ---------------------------------------------------------------- the entry */

/**
 * One document the user converted.
 *
 * `options` is pruned, exactly as the stored and shared option objects are: absent
 * means the app's default, so an entry made today keeps its meaning when a default
 * changes. `preview` is a few lines of the rendered output — enough for a legible
 * card and cheap to export; the real rendering is always regenerable from `source`
 * and `options`.
 */
export interface LibraryEntry {
  id: string;
  /** Epoch milliseconds. */
  createdAt: number;
  updatedAt: number;
  title: string;
  source: string;
  options: AppOptions;
  starred: boolean;
  preview: string;
}

/**
 * The largest document the library will log, matching `MAX_STORED_DOC` in `state.ts`.
 *
 * Same reasoning, one step further: a huge paste must not be able to fill the quota
 * and take the *rest of the library* down with it. An oversize document is not logged
 * at all — never logged truncated — and the app says so in the status line.
 */
export const MAX_ENTRY_SOURCE = 512 * 1024;

/** The preview is the shorter of these two: six lines, or four hundred characters. */
export const PREVIEW_MAX_LINES = 6;
export const PREVIEW_MAX_CHARS = 400;

/** A gap this long between conversions ends the editing session (§6.10). */
export const IDLE_GAP_MS = 30 * 60 * 1000;

/** How long the current entry waits after a keystroke before it is written. */
export const RECORD_DELAY_MS = 2000;

/** The fraction of the quota past which the library header says so, once. */
export const QUOTA_WARN_RATIO = 0.8;

/* ------------------------------------------------- the per-event fork rule */

/**
 * How much of the document one input event may remove before it is read as a
 * *replacement* rather than an edit, and the draft forks into a new entry (§6.10).
 *
 * Low on purpose. A wrong fork costs one extra entry in a log that is filterable and
 * is only ever pruned deliberately; a wrong non-fork overwrites the document the user
 * had. That asymmetry is the whole argument, and it argues against cleverness as much
 * as it argues for a low number.
 */
export const FORK_REMOVED_RATIO = 0.3;

/**
 * The absolute floor under {@link FORK_REMOVED_RATIO}: an event that removes fewer
 * characters than this never forks, whatever share of the buffer they were.
 *
 * Without it a two-character document forks on a backspace, which is a new entry per
 * keystroke on a buffer nobody would miss. Above it the ratio decides alone — a
 * select-all over any real document removes hundreds of characters, so the floor is
 * never what stands between a replacement and its fork.
 */
export const FORK_MIN_REMOVED = 24;

/**
 * How much of `before` one edit took out, in characters.
 *
 * The measurement is the span between the longest common prefix and the longest
 * common suffix: for the single contiguous replacement that every `input` event on a
 * textarea is, that span *is* what was selected. Typing changes one character;
 * appending or pasting at the end shares the whole of `before` as its prefix and
 * removes nothing; a select-all-and-paste shares almost nothing and removes the lot.
 *
 * For the rarer event that changes two separate places at once — a browser's own undo
 * of a multi-part edit — the span covers both, which overstates the removal. That
 * errs toward forking, which is the direction this rule is supposed to err in.
 */
export function removedByEdit(before: string, after: string): number {
  const shorter = Math.min(before.length, after.length);
  let head = 0;
  while (head < shorter && before.charCodeAt(head) === after.charCodeAt(head)) head += 1;
  let tail = 0;
  while (
    tail < shorter - head &&
    before.charCodeAt(before.length - 1 - tail) === after.charCodeAt(after.length - 1 - tail)
  ) {
    tail += 1;
  }
  return before.length - head - tail;
}

/**
 * Whether one input event replaced the document rather than edited it — the safety
 * net under the explicit verbs (§6.10).
 *
 * It is deliberately per *event*: the comparison is against the text as it was an
 * instant ago, never cumulatively against what the entry holds. A cumulative rule
 * drifts, because a long session that rewrites a section at a time crosses any
 * threshold while genuinely being one document.
 */
export function forksEntry(before: string, after: string): boolean {
  if (before.trim() === '') return false;
  const removed = removedByEdit(before, after);
  if (removed < FORK_MIN_REMOVED) return false;
  return removed > before.length * FORK_REMOVED_RATIO;
}

/* ------------------------------------------------------------ deriving a name */

/**
 * What to call an entry: the document's own heading, else its first line, else the
 * date. Never a prompt — the save is automatic, and asking someone to name something
 * they did not ask to save would be absurd. The library offers Rename instead.
 */
export function entryTitle(source: string, now: Date): string {
  const heading = documentTitle(source) ?? firstLine(source);
  return heading ? shorten(heading, MAX_TITLE) : formatToday(now);
}

/**
 * Whether an entry still wears the name the document gave it, rather than one the
 * user typed in the library.
 *
 * This is how an in-place update can follow a document that has just grown a
 * `\title` without ever undoing a rename, and it costs no field in the entry — the
 * previous source is right there, so the question "would this title have been derived
 * from it?" is answerable.
 */
export function titleIsAutomatic(entry: LibraryEntry): boolean {
  return entry.title === entryTitle(entry.source, new Date(entry.createdAt));
}

/** A few lines of rendered output, for the card and the export (§6.10). */
export function makePreview(text: string): string {
  const lines = text.split('\n').slice(0, PREVIEW_MAX_LINES).join('\n');
  const clipped = lines.length > PREVIEW_MAX_CHARS ? lines.slice(0, PREVIEW_MAX_CHARS) : lines;
  return clipped.trimEnd();
}

/**
 * The options in force, in words: `Math: source | Wrap: 72 columns`.
 *
 * Only what differs from the app's defaults is named, for the same reason only that
 * much is stored — a card saying "everything is as it always is" is a card saying
 * nothing.
 */
export function describeOptions(options: AppOptions): string {
  const pruned = pruneOptions(options) as Record<string, unknown>;
  const parts: string[] = [];
  for (const [key, value] of Object.entries(pruned)) {
    if (value === undefined || key === 'todayCustom') continue;
    parts.push(`${OPTION_LABELS[key] ?? key}: ${optionValue(key, value)}`);
  }
  return parts.length === 0 ? 'The default options' : parts.join(' · ');
}

const OPTION_LABELS: Record<string, string> = {
  math: 'Math',
  mathExpressionIn: 'Expression delimiters',
  matrixDelimiters: 'Matrix delimiters',
  keepComments: 'Comments',
  headingStyle: 'Headings',
  footnoteStyle: 'Footnotes',
  textFont: 'Text char styles',
  mathFont: 'Math char styles',
  unknownMacro: 'Unknown macros',
  unknownEnv: 'Unknown environments',
  unknownSpecials: 'Unknown specials',
  recovery: 'Recovery',
  macroDefinitions: 'Macro definitions',
  wrap: 'Wrap',
  todayMode: 'Today',
};

function optionValue(key: string, value: unknown): string {
  if (typeof value === 'number') return `${value} columns`;
  if (typeof value === 'boolean') return value ? 'kept' : 'dropped';
  if (key === 'wrap' && value === 'fit') return 'fit to the pane';
  return String(value);
}

/* ------------------------------------------------------------------ reading */

/** Why an entry from a file or a database was not accepted. */
export type EntryRefusal = 'not-an-object' | 'no-source' | 'too-large';

export type EntryRead = { ok: true; entry: LibraryEntry } | { ok: false; reason: EntryRefusal };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * A timestamp as this file writes them (epoch milliseconds) or as an export writes
 * them (an ISO 8601 string). Anything else — including a date the browser cannot
 * parse — falls back rather than becoming `NaN`, which sorts unpredictably and shows
 * up in the pane as "Invalid Date".
 */
function readTime(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value) && value > 0) return Math.floor(value);
  if (typeof value === 'string') {
    const parsed = Date.parse(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

/**
 * Read one entry from data this build did not write: a database left by an older
 * build, or a file somebody sent. Every field is checked and every unknown one is
 * dropped, with `sanitizeOptions` doing for the options exactly what it does for a
 * share link.
 */
export function readEntry(raw: unknown, now: number, newId: () => string = randomId): EntryRead {
  if (!isRecord(raw)) return { ok: false, reason: 'not-an-object' };
  const source = raw['source'];
  if (typeof source !== 'string' || source === '') return { ok: false, reason: 'no-source' };
  if (source.length > MAX_ENTRY_SOURCE) return { ok: false, reason: 'too-large' };

  const createdAt = readTime(raw['createdAt'], now);
  const id = typeof raw['id'] === 'string' && raw['id'] !== '' ? raw['id'] : newId();
  const named = typeof raw['title'] === 'string' ? raw['title'].trim() : '';

  return {
    ok: true,
    entry: {
      id,
      createdAt,
      updatedAt: readTime(raw['updatedAt'], createdAt),
      title: named === '' ? entryTitle(source, new Date(createdAt)) : shorten(named, MAX_TITLE),
      source,
      options: pruneOptions(sanitizeOptions(raw['options'])),
      starred: raw['starred'] === true,
      preview: typeof raw['preview'] === 'string' ? makePreview(raw['preview']) : '',
    },
  };
}

/* --------------------------------------------------------------- identifiers */

/**
 * A short, unique-enough id: when it was made, then randomness.
 *
 * Sorting by id therefore sorts roughly by age, which makes a database dump readable,
 * and `crypto` is used where it exists rather than assumed to be there — this module
 * is loaded in node by vitest as well as in a browser.
 */
export function randomId(): string {
  const stamp = Date.now().toString(36);
  const random = globalThis.crypto?.getRandomValues
    ? [...globalThis.crypto.getRandomValues(new Uint8Array(6))]
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join('')
    : Math.random().toString(16).slice(2, 14);
  return `${stamp}-${random}`;
}

/* -------------------------------------------------------------- fingerprints */

/**
 * A cheap hash of what an entry *is* — its source and the options it was converted
 * under. Import's "skip items I already have" matches on this rather than on `id`,
 * because two libraries grown from the same export share ids by accident and two
 * copies of the same document generally do not.
 *
 * FNV-1a over UTF-16 code units. It is a hash, not a promise: {@link sameContent}
 * confirms every hit, so a collision costs a comparison rather than an entry.
 */
export function fingerprint(source: string, options: AppOptions): string {
  const text = `${JSON.stringify(pruneOptions(options))} ${source}`;
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${text.length.toString(36)}:${(hash >>> 0).toString(36)}`;
}

/** The fingerprint of an entry. */
export function entryFingerprint(entry: LibraryEntry): string {
  return fingerprint(entry.source, entry.options);
}

/** Whether two entries carry the same document under the same options. */
export function sameContent(a: LibraryEntry, b: LibraryEntry): boolean {
  return (
    a.source === b.source &&
    JSON.stringify(pruneOptions(a.options)) === JSON.stringify(pruneOptions(b.options))
  );
}

/* ------------------------------------------------------------------- sorting */

/** Most recently updated first — the order the library is always shown in. */
export function byRecency(entries: readonly LibraryEntry[]): LibraryEntry[] {
  return [...entries].sort((a, b) => b.updatedAt - a.updatedAt || b.createdAt - a.createdAt);
}

/** The all/starred filter and the text search, over title and source (§6.10). */
export function filterEntries(
  entries: readonly LibraryEntry[],
  query: string,
  starredOnly: boolean,
): LibraryEntry[] {
  const needle = query.trim().toLowerCase();
  return entries.filter((entry) => {
    if (starredOnly && !entry.starred) return false;
    if (needle === '') return true;
    return entry.title.toLowerCase().includes(needle) || entry.source.toLowerCase().includes(needle);
  });
}

/* -------------------------------------------------------- counting and quota */

export interface LibraryStats {
  count: number;
  starred: number;
  /** What the library itself holds, in bytes; see {@link estimateBytes}. */
  bytes: number;
}

/**
 * The library's own size, near enough for a header line.
 *
 * `navigator.storage.estimate()` answers a different question — everything this
 * origin stores, rounded and padded for privacy — which makes it the right number for
 * "how close am I to the quota" and the wrong one for "how big is my library". This
 * counts the entries instead: a UTF-16 code unit is counted as two bytes and the
 * per-entry overhead is a flat allowance, which is honest to a few percent for prose
 * and never pretends to be exact.
 */
export function estimateBytes(entries: readonly LibraryEntry[]): number {
  let total = 0;
  for (const entry of entries) {
    total += 2 * (entry.source.length + entry.preview.length + entry.title.length + entry.id.length);
    total += JSON.stringify(entry.options).length * 2 + 64;
  }
  return total;
}

export function statsOf(entries: readonly LibraryEntry[]): LibraryStats {
  return {
    count: entries.length,
    starred: entries.reduce((n, entry) => n + (entry.starred ? 1 : 0), 0),
    bytes: estimateBytes(entries),
  };
}

/** "3.1 MB" — one decimal above a kilobyte, none below it. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ['kB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** What `navigator.storage.estimate()` said, where it said anything at all. */
export interface StorageEstimate {
  usage: number;
  quota: number;
}

/**
 * How close this origin is to its quota: `'tight'` past {@link QUOTA_WARN_RATIO},
 * which earns one unobtrusive note in the library header naming Export as the remedy,
 * and nothing else. It is not a modal, it does not repeat every session, and it never
 * removes anything.
 */
export function quotaPressure(estimate: StorageEstimate | null): 'unknown' | 'ok' | 'tight' {
  if (!estimate || !Number.isFinite(estimate.quota) || estimate.quota <= 0) return 'unknown';
  return estimate.usage / estimate.quota >= QUOTA_WARN_RATIO ? 'tight' : 'ok';
}

/* ------------------------------------------------- the only removal proposal */

export interface PruneProposal {
  entries: LibraryEntry[];
  /** The oldest and newest `updatedAt` among them, for the dialog's date range. */
  from: number;
  to: number;
  /** How many entries would remain, and how many of those are starred. */
  keeping: number;
  keepingStarred: number;
}

/**
 * The oldest *unstarred* entries, oldest first — the only removal the app ever
 * proposes, and it proposes it rather than performing it.
 *
 * A starred entry cannot appear in the result whatever `count` says: starring is the
 * user saying "this one matters", and an automatic mechanism that could override it
 * would make starring worthless. If every entry is starred the proposal is empty, and
 * the app's answer to a full disk is Export and then stop logging.
 */
export function prunableEntries(
  entries: readonly LibraryEntry[],
  count = Number.POSITIVE_INFINITY,
): LibraryEntry[] {
  return entries
    .filter((entry) => !entry.starred)
    .sort((a, b) => a.updatedAt - b.updatedAt)
    .slice(0, Math.max(0, count));
}

/** {@link prunableEntries} plus everything the dialog has to be able to say. */
export function pruneProposal(entries: readonly LibraryEntry[], count: number): PruneProposal {
  const chosen = prunableEntries(entries, count);
  const chosenIds = new Set(chosen.map((entry) => entry.id));
  const times = chosen.map((entry) => entry.updatedAt);
  const remaining = entries.filter((entry) => !chosenIds.has(entry.id));
  return {
    entries: chosen,
    from: times.length > 0 ? Math.min(...times) : 0,
    to: times.length > 0 ? Math.max(...times) : 0,
    keeping: remaining.length,
    keepingStarred: remaining.reduce((n, entry) => n + (entry.starred ? 1 : 0), 0),
  };
}

/** How many entries a full disk offers to give up, at most. */
export const PRUNE_PROPOSAL_SIZE = 20;

/* --------------------------------------------------------------- the backend */

/**
 * The storage the library sits on. `src/library-store.ts` implements this over
 * IndexedDB; a test implements it over a `Map`.
 *
 * Every method may reject — a full disk, a private window, a database the browser
 * decided to close — and every caller in this file treats a rejection as a fact to
 * report rather than an exception to propagate.
 */
export interface LibraryBackend {
  readonly kind: 'indexeddb' | 'memory';
  /** Everything, in no particular order; the caller sorts. */
  all(): Promise<LibraryEntry[]>;
  get(id: string): Promise<LibraryEntry | null>;
  put(entries: readonly LibraryEntry[]): Promise<void>;
  remove(ids: readonly string[]): Promise<void>;
  clear(): Promise<void>;
}

/** Why a write did not happen. `'quota'` is the one the user can do something about. */
export interface WriteFailure {
  kind: 'quota' | 'error';
  message: string;
}

function failureOf(error: unknown): WriteFailure {
  const name = (error as { name?: string } | null)?.name ?? '';
  const message = error instanceof Error ? error.message : String(error);
  const quota = name === 'QuotaExceededError' || /quota|storage is full/i.test(message);
  return { kind: quota ? 'quota' : 'error', message };
}

/* ------------------------------------------------------- sealing an entry */

/**
 * Where the document on screen stands in the log (§6.10).
 *
 * *Sealing* is the one primitive under New, Save and ★: a sealed entry stops absorbing
 * edits, and the next change to the document starts a new one. It is a fact about the
 * editing session rather than about the entry — the entry itself is unchanged, and
 * opening it later from the pane writes into it again.
 */
export interface SessionState {
  /** The entry the document belongs to, or `null` before it has been logged. */
  entryId: string | null;
  /** Whether that entry is sealed: the next change to the document starts a new one. */
  sealed: boolean;
}

/**
 * What one input event did to the session (§6.10).
 *
 * `from` is always an entry the document has just *left*, and is what the app offers
 * to go back to: for a fork, {@link Library.mergeBack} folds the new draft into it;
 * for a seal that has just ended, it is simply what the chip stops naming.
 */
export type EditOutcome =
  | { kind: 'none' }
  | { kind: 'unsealed'; from: string }
  | { kind: 'forked'; from: string };

/** What the input pane's header says about {@link SessionState}, in words (§6.10). */
export interface SessionLabel {
  /** The short form, beside the pane's title. */
  label: string;
  /** The whole sentence, as a `title` and as the accessible name. */
  hint: string;
}

/**
 * The current entry, in words.
 *
 * The real complaint item 8 answers is *silence* — an entry was being overwritten and
 * nothing on screen said so — and no heuristic fixes that. This is the sentence that
 * does, so it is a pure function and is asserted rather than eyeballed.
 */
export function describeSession(state: SessionState, title: string | null): SessionLabel {
  if (state.entryId === null || title === null) {
    return {
      label: 'New entry',
      hint: 'What you convert next starts a new entry in your library.',
    };
  }
  if (state.sealed) {
    return {
      label: title,
      hint: `“${title}” is kept as it is. Editing the document starts a new entry.`,
    };
  }
  return {
    label: title,
    hint: `Saving into “${title}” in your library. Editing the document updates it.`,
  };
}

/* --------------------------------------------------------------- the library */

export interface LibraryInit {
  /** `null` where this browser has no IndexedDB, or would not open one. */
  backend: LibraryBackend | null;
  clock?: Clock;
  /** The debounce on the in-place update of the current entry. */
  delayMs?: number;
  idleGapMs?: number;
  now?(): number;
  newId?(): string;
  /**
   * Ask the browser to stop treating this data as evictable. Called once, before the
   * first write; the answer only ever decides what the pane's header says.
   */
  persist?(): Promise<boolean>;
  /** An entry was written. `created` is false for the in-place update of one. */
  onWrite?(entry: LibraryEntry, created: boolean): void;
  /**
   * Where the document stands in the log changed: a new entry, a seal, an opened
   * entry, an idle gap. The app puts it in the input pane's header, so that the entry
   * being written to is visible rather than guessed at, and keeps the unsealed half of
   * it in `localStorage` so a reload continues the session rather than starting a
   * second entry for the same document (§6.10).
   */
  onSession?(state: SessionState): void;
  /** A write failed and the entry was not stored. The app says so, loudly (§6.10). */
  onWriteFailure?(failure: WriteFailure): void;
  /** The document crossed {@link MAX_ENTRY_SOURCE}, and back. */
  onOversize?(oversize: boolean): void;
  /** The set of entries changed, so an open pane should re-read it. */
  onChange?(): void;
}

export interface Library {
  /** Whether there is anywhere to log to at all. */
  readonly available: boolean;
  /** Whether the user asked the app to stop logging (a declined prune, §6.10). */
  readonly paused: boolean;
  /** The entry this editing session is writing into, if it has one and it is open. */
  readonly currentId: string | null;
  /** Where the document on screen stands: which entry, and whether it is sealed. */
  readonly session: SessionState;
  /** Note the document as it stands; the write is debounced and may create an entry. */
  record(source: string, options: AppOptions, preview: string): void;
  /** Write anything pending now — for `pagehide`. */
  flush(): Promise<void>;
  /** The document was replaced wholesale: the next `record` starts a new entry. */
  beginNewEntry(): void;
  /**
   * One input event happened, from `before` to `after`. Answers what it did to the
   * session, so the app can say so and offer to undo it.
   *
   * This is where a seal ends and where {@link forksEntry} applies, rather than at the
   * conversion that follows: both are facts about an *edit*, and a document that fails
   * to convert is being replaced just as surely as one that succeeds.
   */
  noteEdit(before: string, after: string): EditOutcome;
  /** Keep logging into an existing entry — what opening one from the pane does. */
  adopt(id: string): void;
  /**
   * Come back to an entry that is already sealed: the document on screen is its
   * source, so nothing is written until that source changes — and the change starts a
   * new entry rather than editing what was kept.
   */
  adoptSealed(entry: LibraryEntry): void;
  /**
   * Stop this entry absorbing edits, and hand it back: what New, Save and ★ all do
   * first (§6.10).
   *
   * Whatever is pending is written into it now, so the entry holds the document as it
   * was when the button was pressed. The *next* entry is not created here — it is
   * created lazily, by the first {@link record} whose source differs, so that pressing
   * Save and walking away cannot leave an empty entry in the log.
   */
  seal(source: string, options: AppOptions, preview: string): Promise<LibraryEntry | null>;
  /**
   * Undo an automatic fork: the draft that was started belongs to `id` after all.
   *
   * What the draft has written is folded back into `id`, and the entry the fork made —
   * that one only, and never a starred one — is removed. A wrong guess by
   * {@link forksEntry} therefore costs a click rather than the user's work.
   */
  mergeBack(id: string): Promise<boolean>;
  /**
   * Star the entry the document belongs to, sealing it first if it is still open.
   *
   * On an already-sealed entry this is only the flag: ★ does not seal a second time.
   */
  starCurrent(
    source: string,
    options: AppOptions,
    preview: string,
  ): Promise<{ entry: LibraryEntry; starred: boolean } | null>;
  star(id: string, starred: boolean): Promise<LibraryEntry | null>;
  rename(id: string, title: string): Promise<LibraryEntry | null>;
  /** Remove one entry and hand it back, so Undo can put it where it was. */
  remove(id: string): Promise<LibraryEntry | null>;
  /** Put a removed entry back, unchanged — the other half of Undo. */
  restore(entry: LibraryEntry): Promise<boolean>;
  /** Remove several. Only ever called with the user's explicit agreement. */
  removeMany(ids: readonly string[]): Promise<boolean>;
  clear(): Promise<boolean>;
  /** Apply an import; `library-io.ts` decides what a plan may contain. */
  apply(put: readonly LibraryEntry[], remove: readonly string[]): Promise<boolean>;
  list(): Promise<LibraryEntry[]>;
  get(id: string): Promise<LibraryEntry | null>;
  /** Stop logging, because the user declined to give anything up (§6.10). */
  pause(): void;
  resume(): void;
  dispose(): void;
}

interface Pending {
  source: string;
  options: AppOptions;
  preview: string;
  at: number;
  /** The entry this write belongs to, captured when `record` was called. */
  target: string | null;
  /** Which editing session it belongs to; see {@link createLibrary}. */
  session: number;
}

/**
 * The log itself: one current entry per editing session, updated in place, plus the
 * operations the pane performs on the rest.
 *
 * The "current entry" is what keeps a log of documents from being a log of
 * keystrokes. It is created on the first conversion of a non-empty document and
 * updated from then on; it ends when the document is replaced wholesale
 * ({@link Library.beginNewEntry}), when another entry is opened
 * ({@link Library.adopt}), when the user seals it ({@link Library.seal}), when one
 * input event replaces the document ({@link forksEntry}), or when nothing has happened
 * for {@link IDLE_GAP_MS}.
 *
 * A *sealed* entry is the one state that is neither open nor gone: the document it
 * holds is still on screen, nothing is written to it, and the first {@link record}
 * whose source differs starts the next entry. That laziness is the point — an entry
 * created eagerly at the moment of sealing would be an empty one for anybody who
 * pressed Save and walked away.
 *
 * A pending write carries the entry it was made for and the session it belongs to, so
 * that ending a session is synchronous and still cannot misfile the keystrokes that
 * came before it: the last edit of the old document lands in the old entry even
 * though the app has already moved on.
 */
export function createLibrary(init: LibraryInit): Library {
  const clock = init.clock ?? systemClock;
  const delayMs = init.delayMs ?? RECORD_DELAY_MS;
  const idleGapMs = init.idleGapMs ?? IDLE_GAP_MS;
  const now = init.now ?? ((): number => Date.now());
  const newId = init.newId ?? randomId;
  const backend = init.backend;

  let handle: number | null = null;
  let pending: Pending | null = null;
  let currentId: string | null = null;
  /**
   * The entry the user sealed, while its document is still the one on screen: its id,
   * and the source it holds. The source is what makes the next entry lazy — a `record`
   * of the same text is not an edit, so it writes nothing at all.
   */
  let sealed: { id: string; source: string } | null = null;
  let session = 0;
  let lastRecordAt = 0;
  let oversize = false;
  let paused = false;
  let persistAsked = false;
  /** Writes are serialised: two overlapping updates of one entry would interleave. */
  let queue: Promise<unknown> = Promise.resolve();

  function serialise<T>(work: () => Promise<T>): Promise<T> {
    const next = queue.then(work, work);
    queue = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  /** What was last reported, so that a state that has not moved says nothing. */
  let told: SessionState = { entryId: null, sealed: false };

  function sessionState(): SessionState {
    return { entryId: currentId ?? sealed?.id ?? null, sealed: sealed !== null };
  }

  /**
   * Say where the document stands, if it has moved.
   *
   * Every change to `currentId` or `sealed` goes through here, so nothing can move the
   * session without the header being told — which is the whole of item 8's first
   * complaint.
   */
  function announce(): void {
    const state = sessionState();
    if (state.entryId === told.entryId && state.sealed === told.sealed) return;
    told = state;
    init.onSession?.(state);
  }

  /**
   * Write into this entry from now on, or into none.
   *
   * An open entry and a sealed one are exclusive states, so adopting one ends the
   * other: only {@link Library.seal} and {@link Library.adoptSealed} move the session
   * the other way.
   */
  function setCurrent(id: string | null): void {
    currentId = id;
    if (id !== null) sealed = null;
    announce();
  }

  /**
   * These entries are gone: if the session was pointing at one of them, it is now
   * pointing at nothing rather than at an id that finds no entry.
   */
  function forget(ids: readonly string[]): void {
    if (sealed !== null && ids.includes(sealed.id)) {
      sealed = null;
      announce();
    }
    if (currentId !== null && ids.includes(currentId)) setCurrent(null);
  }

  function cancelTimer(): void {
    if (handle !== null) {
      clock.clearTimeout(handle);
      handle = null;
    }
  }

  async function askToPersist(): Promise<void> {
    if (persistAsked || !init.persist) return;
    persistAsked = true;
    try {
      await init.persist();
    } catch {
      /* a browser that will not promise to keep the data simply does not promise */
    }
  }

  async function write(entries: readonly LibraryEntry[]): Promise<boolean> {
    if (!backend) return false;
    await askToPersist();
    try {
      await backend.put(entries);
      return true;
    } catch (error) {
      init.onWriteFailure?.(failureOf(error));
      return false;
    }
  }

  async function read(id: string): Promise<LibraryEntry | null> {
    if (!backend) return null;
    try {
      return await backend.get(id);
    } catch {
      return null;
    }
  }

  /**
   * Take whatever is pending, synchronously.
   *
   * Synchronously is the whole point: {@link commit} runs in a microtask, and a
   * `record` that lands between the decision to write and the write itself would
   * otherwise replace the snapshot — losing the last edit of the document that is
   * being navigated away from, which is exactly the keystroke a user would miss.
   */
  function take(): Pending | null {
    const snapshot = pending;
    pending = null;
    return snapshot;
  }

  /** Turn what `record` was last given into the entry that gets written. */
  async function commit(snapshot: Pending | null): Promise<void> {
    if (!snapshot || !backend || paused) return;

    const at = snapshot.at;
    const options = pruneOptions(snapshot.options);

    if (snapshot.target !== null) {
      const existing = await read(snapshot.target);
      if (existing) {
        const updated: LibraryEntry = {
          ...existing,
          updatedAt: at,
          source: snapshot.source,
          options,
          preview: snapshot.preview,
          // A renamed entry keeps the name it was given; an automatic one follows the
          // document, so a `\title` typed after the fact still names the entry.
          title: titleIsAutomatic(existing) ? entryTitle(snapshot.source, new Date(at)) : existing.title,
        };
        if (await write([updated])) init.onWrite?.(updated, false);
        return;
      }
      // The entry was deleted from the pane while it was still the current one; what
      // is being typed now deserves an entry of its own rather than silence.
    }

    const entry: LibraryEntry = {
      id: newId(),
      createdAt: at,
      updatedAt: at,
      title: entryTitle(snapshot.source, new Date(at)),
      source: snapshot.source,
      options,
      starred: false,
      preview: snapshot.preview,
    };
    if (!(await write([entry]))) return;
    // Only if the session it was recorded for is still the one being edited: a Load
    // or an Open between the keystroke and the write moved the app on.
    if (snapshot.session === session) setCurrent(entry.id);
    init.onWrite?.(entry, true);
    init.onChange?.();
  }

  function schedule(): void {
    cancelTimer();
    handle = clock.setTimeout(() => {
      handle = null;
      const snapshot = take();
      void serialise(() => commit(snapshot));
    }, delayMs);
  }

  async function patch(
    id: string,
    change: (entry: LibraryEntry) => LibraryEntry,
  ): Promise<LibraryEntry | null> {
    const existing = await read(id);
    if (!existing) return null;
    const updated = change(existing);
    if (!(await write([updated]))) return null;
    init.onChange?.();
    return updated;
  }

  /**
   * The document that was on screen is gone: whatever is pending is written into the
   * entry it belonged to, and the session is left holding nothing at all.
   */
  function endSession(): void {
    cancelTimer();
    // Whatever is pending belongs to the document being replaced: it is written into
    // that entry rather than dropped, which is what `target` is for.
    const snapshot = take();
    session += 1;
    sealed = null;
    setCurrent(null);
    lastRecordAt = 0;
    void serialise(() => commit(snapshot));
  }

  /**
   * Seal whatever is open and hand the entry back — the primitive under all three
   * verbs.
   *
   * The document as it stands is written first, so the entry holds what was on screen
   * when the button was pressed rather than what the last conversion happened to
   * catch. Nothing is created for what comes next: `record` does that, and only once
   * the text has actually changed.
   */
  function sealDraft(
    source: string,
    options: AppOptions,
    preview: string,
  ): Promise<LibraryEntry | null> {
    cancelTimer();
    if (source.trim() !== '' && source.length <= MAX_ENTRY_SOURCE && !paused) {
      pending = { source, options, preview, at: now(), target: currentId, session };
    }
    const snapshot = take();
    return serialise(async () => {
      await commit(snapshot);
      if (currentId === null) return null;
      const entry = await read(currentId);
      if (!entry) return null;
      currentId = null;
      sealed = { id: entry.id, source: entry.source };
      lastRecordAt = 0;
      session += 1;
      announce();
      return entry;
    });
  }

  return {
    get available() {
      return backend !== null;
    },
    get paused() {
      return paused;
    },
    get currentId() {
      return currentId;
    },
    get session() {
      return sessionState();
    },

    record(source, options, preview) {
      if (!backend || paused) return;
      // An empty document is not a document: the log starts at the first conversion
      // of something (§6.10).
      if (source.trim() === '') return;
      if (sealed !== null) {
        // The kept version, still on screen: there is nothing to write, and writing
        // anyway would be the in-place update the seal exists to stop.
        if (source === sealed.source) return;
        // And the first edit after it is where the next entry begins — here, rather
        // than at the moment of sealing, so that Save cannot leave an empty one.
        sealed = null;
        announce();
      }
      if (source.length > MAX_ENTRY_SOURCE) {
        // Never logged truncated, and never at the expense of anything else.
        if (!oversize) {
          oversize = true;
          init.onOversize?.(true);
        }
        return;
      }
      if (oversize) {
        oversize = false;
        init.onOversize?.(false);
      }
      const at = now();
      // A long gap means the last session is over: what comes next is a new entry,
      // not a late edit of an old one.
      if (currentId !== null && lastRecordAt > 0 && at - lastRecordAt > idleGapMs) {
        session += 1;
        setCurrent(null);
      }
      lastRecordAt = at;
      pending = { source, options, preview, at, target: currentId, session };
      schedule();
    },

    async flush() {
      cancelTimer();
      const snapshot = take();
      await serialise(() => commit(snapshot));
    },

    beginNewEntry: endSession,

    noteEdit(before, after) {
      if (!backend || paused) return { kind: 'none' };
      if (sealed !== null) {
        // Back to the text that was kept — an undo, or a typo and its correction —
        // so the seal stands.
        if (after === sealed.source) return { kind: 'none' };
        const from = sealed.id;
        sealed = null;
        announce();
        return { kind: 'unsealed', from };
      }
      if (currentId === null || !forksEntry(before, after)) return { kind: 'none' };
      const from = currentId;
      // Exactly what Load ▾ does, and for the same reason: the document that was
      // there has been replaced, and what was pending belongs to it.
      endSession();
      return { kind: 'forked', from };
    },

    adopt(id) {
      cancelTimer();
      const snapshot = take();
      session += 1;
      setCurrent(id);
      lastRecordAt = now();
      void serialise(() => commit(snapshot));
    },

    adoptSealed(entry) {
      cancelTimer();
      const snapshot = take();
      session += 1;
      currentId = null;
      sealed = { id: entry.id, source: entry.source };
      lastRecordAt = 0;
      announce();
      void serialise(() => commit(snapshot));
    },

    seal(source, options, preview) {
      if (!backend) return Promise.resolve(null);
      // Already sealed: the entry is what it is, and Save on it is a no-op rather
      // than a second seal of the same text.
      if (sealed !== null) return read(sealed.id);
      return sealDraft(source, options, preview);
    },

    mergeBack(id) {
      return serialise(async () => {
        if (!backend) return false;
        // What the fork created, if the two seconds since it happened were up.
        const stray = currentId;
        cancelTimer();
        let snapshot = take();
        if (snapshot === null && stray !== null && stray !== id) {
          // The draft is already an entry of its own: its text is what has to be
          // folded back, and the entry itself is what has to go.
          const written = await read(stray);
          if (written) {
            snapshot = {
              source: written.source,
              options: written.options,
              preview: written.preview,
              at: written.updatedAt,
              target: null,
              session,
            };
          }
        }
        session += 1;
        sealed = null;
        setCurrent(id);
        lastRecordAt = now();
        if (snapshot) await commit({ ...snapshot, target: id, session });
        if (stray !== null && stray !== id) {
          const written = await read(stray);
          // Never a starred entry, whatever else is true: starring is the user saying
          // this one matters, and no automatic mechanism may override it (§6.10).
          if (written && !written.starred) {
            try {
              await backend.remove([stray]);
            } catch (error) {
              init.onWriteFailure?.(failureOf(error));
            }
          }
        }
        init.onChange?.();
        return true;
      });
    },

    async starCurrent(source, options, preview) {
      if (!backend) return null;
      // ★ seals as well as stars — but only once: on an entry that is already sealed
      // it is the flag and nothing else (§6.10).
      const entry = sealed !== null ? await read(sealed.id) : await sealDraft(source, options, preview);
      if (!entry) return null;
      const updated = await serialise(() =>
        patch(entry.id, (existing) => ({ ...existing, starred: !existing.starred })),
      );
      return updated ? { entry: updated, starred: updated.starred } : null;
    },

    star(id, starred) {
      return serialise(() => patch(id, (entry) => ({ ...entry, starred })));
    },

    rename(id, title) {
      const cleaned = shorten(title.trim(), MAX_TITLE);
      if (cleaned === '') return Promise.resolve(null);
      return serialise(() => patch(id, (entry) => ({ ...entry, title: cleaned })));
    },

    remove(id) {
      return serialise(async () => {
        if (!backend) return null;
        const existing = await read(id);
        if (!existing) return null;
        try {
          await backend.remove([id]);
        } catch (error) {
          init.onWriteFailure?.(failureOf(error));
          return null;
        }
        forget([id]);
        init.onChange?.();
        return existing;
      });
    },

    restore(entry) {
      return serialise(async () => {
        const ok = await write([entry]);
        if (ok) init.onChange?.();
        return ok;
      });
    },

    removeMany(ids) {
      return serialise(async () => {
        if (!backend || ids.length === 0) return false;
        try {
          await backend.remove(ids);
        } catch (error) {
          init.onWriteFailure?.(failureOf(error));
          return false;
        }
        forget(ids);
        init.onChange?.();
        return true;
      });
    },

    clear() {
      return serialise(async () => {
        if (!backend) return false;
        try {
          await backend.clear();
        } catch (error) {
          init.onWriteFailure?.(failureOf(error));
          return false;
        }
        sealed = null;
        setCurrent(null);
        init.onChange?.();
        return true;
      });
    },

    apply(put, remove) {
      return serialise(async () => {
        if (!backend) return false;
        try {
          if (remove.length > 0) await backend.remove(remove);
          if (put.length > 0) {
            await askToPersist();
            await backend.put(put);
          }
        } catch (error) {
          init.onWriteFailure?.(failureOf(error));
          return false;
        }
        forget(remove);
        init.onChange?.();
        return true;
      });
    },

    async list() {
      if (!backend) return [];
      try {
        return byRecency(await backend.all());
      } catch {
        return [];
      }
    },

    get: read,

    pause() {
      paused = true;
      pending = null;
      cancelTimer();
    },

    resume() {
      paused = false;
    },

    dispose() {
      cancelTimer();
      pending = null;
    },
  };
}
