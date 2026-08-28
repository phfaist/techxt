/**
 * The app's state: what it starts from, how it is validated, where it is stored, and
 * how it is written into and read out of a share link (web/PLAN.md §5, §6.4).
 *
 * Nothing here touches the DOM. `localStorage` and the URL fragment arrive as
 * parameters, so every function in this file runs unchanged in vitest's `node`
 * environment — which is what makes the codec testable at all.
 *
 * Two properties are load-bearing and are worth stating before the code:
 *
 * - **Absent means "the library's default".** Only options the user actually changed
 *   are stored or shared, so a link stays short and a future change of a default in
 *   `techxt` is picked up rather than frozen (§6.4). {@link pruneOptions} is the one
 *   place that decides what "changed" means.
 * - **A read never throws.** Absent, truncated, corrupt and wrong-version data all
 *   fall back to the default. A share link is a string a stranger sent you.
 */

import { DEFAULT_FONT, DEFAULT_SIZE, MAX_SIZE, MIN_SIZE, isFontId } from './fonts';
import { APP_ONLY_KEYS } from './types';
import type { AppOptions, AppState, SharedState, UiState, WrapMode } from './types';
import type { OptionsPayload } from './worker/protocol';

/* --------------------------------------------------------------------- defaults */

/**
 * The app's starting options.
 *
 * `wrap: 'soft'` and `todayMode: 'browser'` are the two places the app answers a
 * question the library leaves open (§5). *Soft* keeps the library's own line breaks —
 * none — and folds the long lines to the pane for display, so a first-time reader gets
 * the library's text without the sideways scrolling that comes with it, and Copy and
 * Download still hand over exactly what the library produced. A browser has a clock
 * where the `no_std` library has only `<today>`. Every other key is absent, which *is*
 * the library's default.
 */
export const DEFAULT_OPTIONS: AppOptions = { wrap: 'soft', todayMode: 'browser' };

/** The presentation defaults. None of these changes a character of the output. */
export const DEFAULT_UI: UiState = {
  font: DEFAULT_FONT,
  size: DEFAULT_SIZE,
  split: 0.5,
  moreOpen: false,
  diagnosticsOpen: false,
  focus: null,
  keepFontsOffline: false,
};

/** A fresh {@link AppState}: an empty document under the defaults above. */
export function defaultState(): AppState {
  return { v: 1, doc: '', opts: { ...DEFAULT_OPTIONS }, ui: { ...DEFAULT_UI } };
}

/* ------------------------------------------------------------------------ today */

/** The month names `\today` spells out, matching `rust/techxt-cli/src/today.rs`. */
const MONTH_NAMES = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const;

/**
 * `\today` in the form `techxt-cli` produces: `"August 20, 2026"`.
 *
 * The CLI reads the clock in UTC to avoid depending on a time-zone database; a
 * browser already has one, and a person looking at their own screen has no use for
 * that caution, so this uses the **local** date (§5).
 */
export function formatToday(date: Date): string {
  const month = MONTH_NAMES[date.getMonth()] ?? `${date.getMonth() + 1}`;
  return `${month} ${date.getDate()}, ${date.getFullYear()}`;
}

/* ------------------------------------------------------------------ fit-to-pane */

/** The narrowest column count *Fit* will ask for, however narrow the pane gets. */
export const MIN_FIT_COLUMNS = 20;

/**
 * Turn a measured pane width into a column count for `wrapWidth` (§6.5).
 *
 * The 2 % margin plus the pane's horizontal scroll absorb the occasional long line a
 * proportional face produces; for a monospace face the mean advance *is* the advance
 * and the fit is exact. The **measurement** — rendering a sample in the output pane's
 * computed style and dividing by its length — belongs to `ui/panes.ts`, which owns
 * the pane; this is only the arithmetic, kept here so it can be unit-tested.
 */
export function columnsFor(paneContentWidth: number, meanAdvance: number): number {
  if (!Number.isFinite(paneContentWidth) || !Number.isFinite(meanAdvance) || meanAdvance <= 0) {
    return MIN_FIT_COLUMNS;
  }
  return Math.max(MIN_FIT_COLUMNS, Math.floor((paneContentWidth * 0.98) / meanAdvance));
}

/* ------------------------------------------------------------- option validation */

type Validator = (value: unknown) => unknown;

function oneOf(allowed: readonly string[]): Validator {
  return (value) => (typeof value === 'string' && allowed.includes(value) ? value : undefined);
}

const bool: Validator = (value) => (typeof value === 'boolean' ? value : undefined);
const str: Validator = (value) => (typeof value === 'string' ? value : undefined);

/** `'fit'`, `'off'`, `'soft'`, or a column count somebody typed into the custom field. */
const wrapMode: Validator = (value) => {
  if (value === 'fit' || value === 'off' || value === 'soft') return value;
  if (typeof value === 'number' && Number.isFinite(value) && value >= 1) {
    return Math.min(10_000, Math.floor(value));
  }
  return undefined;
};

/** Every `FontStyleValue` of the binding, for `textFont` and `mathFont`. */
const FONT_STYLE_VALUES = [
  'off',
  'default',
  'bold',
  'italic',
  'bold-italic',
  'script',
  'bold-script',
  'fraktur',
  'bold-fraktur',
  'double-struck',
  'sans-serif',
  'sans-serif-bold',
  'sans-serif-italic',
  'sans-serif-bold-italic',
  'monospace',
  'upright',
] as const;

/**
 * One validator per {@link AppOptions} key. Typing this as a total record over the
 * key set is what makes a new option a compile error here rather than a value that
 * silently fails to survive a reload.
 */
const OPTION_VALIDATORS: Record<keyof AppOptions, Validator> = {
  mathMode: oneOf(['fancy', 'plain', 'source']),
  mathExpressionIn: oneOf(['parens', 'braces', 'none']),
  matrixDelimiters: oneOf(['unicode', 'ascii']),
  keepComments: bool,
  headingStyle: oneOf(['numbered-underlined', 'underlined', 'prefix', 'plain']),
  footnoteStyle: oneOf(['collected', 'inline', 'skip']),
  textFont: oneOf(FONT_STYLE_VALUES),
  mathFont: oneOf(FONT_STYLE_VALUES),
  unknownMacro: oneOf(['skip', 'render-args', 'keep-source', 'placeholder']),
  unknownEnv: oneOf(['render-body', 'skip', 'keep-source']),
  unknownSpecials: oneOf(['emit-chars', 'skip']),
  recovery: oneOf(['tolerant', 'strict']),
  macroDefinitions: oneOf(['honored', 'declared']),
  wrap: wrapMode,
  todayMode: oneOf(['browser', 'library', 'custom']),
  todayCustom: str,
};

const OPTION_KEYS = Object.keys(OPTION_VALIDATORS) as (keyof AppOptions)[];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Keep the keys that are ours and the values that are ones we know, drop everything
 * else. A share link from a newer build, a half-written `localStorage` entry and a
 * hand-edited fragment all come through here.
 */
export function sanitizeOptions(raw: unknown): AppOptions {
  const out: Record<string, unknown> = {};
  if (isRecord(raw)) {
    for (const key of OPTION_KEYS) {
      const value = OPTION_VALIDATORS[key](raw[key]);
      if (value !== undefined) out[key] = value;
    }
  }
  return out as AppOptions;
}

/**
 * Drop every key whose value is the default anyway — absent is how a default is
 * spelled (§6.4), so this is what keeps a share link short.
 */
export function pruneOptions(opts: AppOptions): AppOptions {
  const out: Record<string, unknown> = {};
  for (const key of OPTION_KEYS) {
    const value = opts[key];
    if (value === undefined) continue;
    if (DEFAULT_OPTIONS[key] === value) continue;
    out[key] = value;
  }
  return out as AppOptions;
}

/** The app defaults with `opts` laid over them — the inverse of {@link pruneOptions}. */
export function withDefaults(opts: AppOptions): AppOptions {
  return { ...DEFAULT_OPTIONS, ...opts };
}

/* ---------------------------------------------------------------- ui validation */

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

function number(value: unknown, fallback: number, low: number, high: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? clamp(value, low, high) : fallback;
}

/** Every field checked against the range the UI can actually show. */
export function sanitizeUi(raw: unknown): UiState {
  if (!isRecord(raw)) return { ...DEFAULT_UI };
  const focus = raw['focus'];
  return {
    font: isFontId(raw['font']) ? raw['font'] : DEFAULT_UI.font,
    size: Math.round(number(raw['size'], DEFAULT_UI.size, MIN_SIZE, MAX_SIZE)),
    split: number(raw['split'], DEFAULT_UI.split, 0.2, 0.8),
    moreOpen: raw['moreOpen'] === true,
    diagnosticsOpen: raw['diagnosticsOpen'] === true,
    focus: focus === 'input' || focus === 'output' ? focus : null,
    keepFontsOffline: raw['keepFontsOffline'] === true,
  };
}

/* --------------------------------------------------------------- resolve options */

/**
 * Turn the app's settings into the options the worker understands — the one place
 * `wrap` and `todayMode` stop existing (§5).
 *
 * `columns` is the fit-to-pane measurement of §6.5; it is ignored unless the user is
 * actually on *Fit*. `now` is a parameter so the date is testable.
 */
export function resolveOptions(
  opts: AppOptions,
  columns: number,
  now: Date = new Date(),
): OptionsPayload {
  const payload: Record<string, unknown> = {};
  const appOnly: readonly string[] = APP_ONLY_KEYS;
  for (const key of OPTION_KEYS) {
    if (appOnly.includes(key)) continue;
    const value = opts[key];
    if (value !== undefined) payload[key] = value;
  }

  // Absent means the app default, not the library's: `wrap` and `todayMode` are
  // pruned when they equal it, so a link that omits them still means Soft and the
  // browser's date.
  const wrap: WrapMode = opts.wrap ?? DEFAULT_OPTIONS.wrap ?? 'soft';
  if (wrap === 'fit') payload['wrapWidth'] = Math.max(MIN_FIT_COLUMNS, Math.floor(columns));
  else if (typeof wrap === 'number') payload['wrapWidth'] = wrap;
  // 'off' and 'soft' both omit `wrapWidth` entirely — the library's own default is no
  // wrapping, and 'soft' differs from 'off' only in the pane the answer is shown in,
  // which is not something the library can be told about. `softWraps` below is where
  // that half of the setting is read back out.

  const todayMode = opts.todayMode ?? DEFAULT_OPTIONS.todayMode ?? 'browser';
  if (todayMode === 'browser') payload['today'] = formatToday(now);
  else if (todayMode === 'custom' && typeof opts.todayCustom === 'string') {
    payload['today'] = opts.todayCustom;
  }
  // 'library' omits `today`, which is what leaves `\today` rendering as `<today>`.

  return payload as OptionsPayload;
}

/**
 * Whether the output pane should fold long lines to its own width rather than scroll
 * sideways (§6.3) — the display half of `wrap: 'soft'`, and the only part of a wrap
 * setting `resolveOptions` deliberately drops.
 *
 * Absent reads as the app default, exactly as it does there.
 */
export function softWraps(opts: AppOptions): boolean {
  return (opts.wrap ?? DEFAULT_OPTIONS.wrap) === 'soft';
}

/* --------------------------------------------------------------------- the clock */

/**
 * The two timer functions this module and the conversion client need, injectable so
 * a test can drive time instead of waiting for it.
 */
export interface Clock {
  setTimeout(handler: () => void, ms: number): number;
  clearTimeout(handle: number): void;
}

/** The real one. */
export const systemClock: Clock = {
  setTimeout: (handler, ms) => setTimeout(handler, ms) as unknown as number,
  clearTimeout: (handle) => clearTimeout(handle as unknown as ReturnType<typeof setTimeout>),
};

/* ------------------------------------------------------------------- persistence */

export const STORAGE_KEYS = {
  doc: 'techxt.doc.v1',
  opts: 'techxt.opts.v1',
  ui: 'techxt.ui.v1',
  libraryHints: 'techxt.library.hints.v1',
  libraryCurrent: 'techxt.library.current.v1',
} as const;

/**
 * The document size beyond which nothing is stored (§6.4).
 *
 * The cap exists so a huge paste cannot exhaust the quota and take the *settings*
 * down with it. Above it the document key is cleared rather than written short: a
 * silently truncated document coming back after a reload is worse than an honest
 * empty pane plus the note the status line shows.
 */
export const MAX_STORED_DOC = 512 * 1024;

/** The two methods of `localStorage` this app uses, so a test can supply a Map. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * `localStorage`, or `null` where it is absent or refuses to be touched — Safari in
 * private mode throws on *access*, not only on write.
 */
export function browserStorage(): StorageLike | null {
  try {
    if (typeof localStorage === 'undefined') return null;
    const probe = '__techxt_probe__';
    localStorage.setItem(probe, '1');
    localStorage.removeItem(probe);
    return localStorage;
  } catch {
    return null;
  }
}

function readJson(storage: StorageLike | null, key: string): unknown {
  if (!storage) return undefined;
  try {
    const raw = storage.getItem(key);
    if (raw === null) return undefined;
    return JSON.parse(raw) as unknown;
  } catch {
    return undefined;
  }
}

function write(storage: StorageLike | null, key: string, value: string): boolean {
  if (!storage) return false;
  try {
    storage.setItem(key, value);
    return true;
  } catch {
    // A full quota is not an error the user can act on; the note in the status line
    // is all the report this deserves.
    return false;
  }
}

export interface PersistenceInit {
  storage: StorageLike | null;
  /** Debounce for every write. Default 500 ms (§6.4). */
  delayMs?: number;
  clock?: Clock;
  /** Called when the document crosses the {@link MAX_STORED_DOC} cap, and back. */
  onDocumentOversize?(oversize: boolean): void;
}

export interface Persistence {
  document(text: string): void;
  options(opts: AppOptions): void;
  ui(ui: UiState): void;
  /** Write whatever is pending now — for `pagehide`. */
  flush(): void;
  dispose(): void;
}

/**
 * Debounced writes of the three keys. Each is written only when it changed, and a
 * failure is swallowed: losing a saved document is a disappointment, throwing in an
 * input handler is a broken app.
 */
export function createPersistence(init: PersistenceInit): Persistence {
  const clock = init.clock ?? systemClock;
  const delayMs = init.delayMs ?? 500;
  let handle: number | null = null;
  let pendingDoc: string | null = null;
  let pendingOpts: AppOptions | null = null;
  let pendingUi: UiState | null = null;
  let oversize = false;

  const schedule = (): void => {
    if (handle !== null) clock.clearTimeout(handle);
    handle = clock.setTimeout(flush, delayMs);
  };

  function flush(): void {
    if (handle !== null) {
      clock.clearTimeout(handle);
      handle = null;
    }
    if (pendingDoc !== null) {
      const doc = pendingDoc;
      pendingDoc = null;
      const tooLarge = doc.length > MAX_STORED_DOC;
      if (tooLarge) {
        try {
          init.storage?.removeItem(STORAGE_KEYS.doc);
        } catch {
          /* nothing to do about it */
        }
      } else {
        write(init.storage, STORAGE_KEYS.doc, doc);
      }
      if (tooLarge !== oversize) {
        oversize = tooLarge;
        init.onDocumentOversize?.(tooLarge);
      }
    }
    if (pendingOpts !== null) {
      const opts = pendingOpts;
      pendingOpts = null;
      write(init.storage, STORAGE_KEYS.opts, JSON.stringify(pruneOptions(opts)));
    }
    if (pendingUi !== null) {
      const ui = pendingUi;
      pendingUi = null;
      write(init.storage, STORAGE_KEYS.ui, JSON.stringify(ui));
    }
  }

  return {
    document(text) {
      pendingDoc = text;
      schedule();
    },
    options(opts) {
      pendingOpts = { ...opts };
      schedule();
    },
    ui(ui) {
      pendingUi = { ...ui };
      schedule();
    },
    flush,
    dispose() {
      if (handle !== null) clock.clearTimeout(handle);
      handle = null;
      pendingDoc = pendingOpts = pendingUi = null;
    },
  };
}

/* ---------------------------------------------------------------- library hints */

/**
 * What the app remembers about *introducing* the library (§6.10) — not about the
 * library itself, which lives in IndexedDB and is none of this file's business.
 *
 * These two counters are the whole of the discoverability machinery: the library only
 * helps if people know it is there, and an app that keeps saying so is an app people
 * learn to ignore.
 */
export interface LibraryHints {
  /** How many sessions have started. The library button pulses for the first few. */
  sessions: number;
  /** Whether the "Saved to your library" toast has been shown. It is shown once. */
  toldAboutFirstSave: boolean;
  /** Whether the pane has ever been opened. Once it has, nothing pulses again. */
  opened: boolean;
}

/** How many sessions the library button draws attention to itself for. */
export const LIBRARY_PULSE_SESSIONS = 3;

export const DEFAULT_LIBRARY_HINTS: LibraryHints = {
  sessions: 0,
  toldAboutFirstSave: false,
  opened: false,
};

/** A read never throws here either: a corrupt hint is a hint that was never given. */
export function readLibraryHints(storage: StorageLike | null): LibraryHints {
  const raw = readJson(storage, STORAGE_KEYS.libraryHints);
  if (!isRecord(raw)) return { ...DEFAULT_LIBRARY_HINTS };
  const sessions = raw['sessions'];
  return {
    sessions:
      typeof sessions === 'number' && Number.isFinite(sessions) && sessions >= 0
        ? Math.min(1000, Math.floor(sessions))
        : 0,
    toldAboutFirstSave: raw['toldAboutFirstSave'] === true,
    opened: raw['opened'] === true,
  };
}

export function writeLibraryHints(storage: StorageLike | null, hints: LibraryHints): void {
  write(storage, STORAGE_KEYS.libraryHints, JSON.stringify(hints));
}

/** Whether the library button should still catch the eye this session. */
export function shouldPulseLibrary(hints: LibraryHints): boolean {
  return !hints.opened && hints.sessions <= LIBRARY_PULSE_SESSIONS;
}

/**
 * Which library entry the editing session was writing into, so that a reload
 * continues it rather than logging the same document twice (§6.10).
 *
 * A reload is not "a new document": the pane comes back with the same text in it, and
 * an entry per reload would turn the log into a pile of copies. This is the one fact
 * that has to outlive the page for the session to survive it, and it is a bare id —
 * the entry itself lives in IndexedDB, and a stale id simply finds nothing.
 */
export function readCurrentEntryId(storage: StorageLike | null): string | null {
  try {
    const raw = storage?.getItem(STORAGE_KEYS.libraryCurrent) ?? null;
    return raw !== null && raw !== '' && raw.length < 200 ? raw : null;
  } catch {
    return null;
  }
}

export function writeCurrentEntryId(storage: StorageLike | null, id: string | null): void {
  if (id === null) {
    try {
      storage?.removeItem(STORAGE_KEYS.libraryCurrent);
    } catch {
      /* nothing to do about it */
    }
    return;
  }
  write(storage, STORAGE_KEYS.libraryCurrent, id);
}

/* -------------------------------------------------------------------- share link */

/** `deflate-raw`, base64url. */
export const SHARE_PREFIX_DEFLATE = '#d=';
/** The same JSON, base64url, for a browser without `CompressionStream`. */
export const SHARE_PREFIX_PLAIN = '#u=';

/**
 * The fragment length past which a link stops surviving chat clients and mail
 * gateways, and the app offers "copy settings only" instead (§6.4).
 */
export const SHARE_LENGTH_LIMIT = 8000;

const BASE64URL_CHUNK = 0x2000;

function bytesToBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (let index = 0; index < bytes.length; index += BASE64URL_CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(index, index + BASE64URL_CHUNK));
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlToBytes(text: string): Uint8Array {
  const padded = text.replace(/-/g, '+').replace(/_/g, '/');
  const binary = atob(padded + '='.repeat((4 - (padded.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

async function through(bytes: Uint8Array, stream: { readable: ReadableStream; writable: WritableStream }): Promise<Uint8Array> {
  const writer = stream.writable.getWriter();
  // A corrupt payload errors *both* halves of the transform; the read side is where
  // it is reported, so the write side's rejection is swallowed rather than left
  // unhandled.
  const written = (async () => {
    try {
      await writer.write(bytes);
      await writer.close();
    } catch {
      /* the reader below throws too, and that is the one the caller sees */
    }
  })();
  const reader = stream.readable.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = (await reader.read()) as { done: boolean; value?: Uint8Array };
    if (done) break;
    if (value) {
      chunks.push(value);
      total += value.length;
    }
  }
  await written;
  const out = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  return out;
}

/** Whether this browser can compress; exported so a test can exercise both paths. */
export function canCompress(): boolean {
  return typeof CompressionStream === 'function' && typeof DecompressionStream === 'function';
}

/**
 * The fragment for `shared`: `#d=` compressed, or `#u=` where `CompressionStream` is
 * missing. The two prefixes are what makes decoding unambiguous, and the `v` inside
 * the payload is what makes a future format recognisable rather than garbage.
 */
export async function encodeShare(shared: SharedState): Promise<string> {
  const payload: SharedState = { v: 1, doc: shared.doc, opts: pruneOptions(shared.opts) };
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  if (canCompress()) {
    try {
      return SHARE_PREFIX_DEFLATE + bytesToBase64Url(await through(bytes, new CompressionStream('deflate-raw')));
    } catch {
      /* fall through to the uncompressed form */
    }
  }
  return SHARE_PREFIX_PLAIN + bytesToBase64Url(bytes);
}

/** The same link without the document — what the app offers when one is too long. */
export function encodeShareSettingsOnly(opts: AppOptions): Promise<string> {
  return encodeShare({ v: 1, doc: '', opts });
}

/**
 * Read a fragment back, or `null` for anything this build cannot make sense of: a
 * fragment that is not ours, a truncated base64 tail, bytes that are not deflate,
 * JSON that is not an object, or a version this build does not know.
 */
export async function decodeShare(fragment: string | null | undefined): Promise<SharedState | null> {
  if (typeof fragment !== 'string' || fragment.length === 0) return null;
  const hash = fragment.startsWith('#') ? fragment : `#${fragment}`;
  const compressed = hash.startsWith(SHARE_PREFIX_DEFLATE);
  if (!compressed && !hash.startsWith(SHARE_PREFIX_PLAIN)) return null;
  const body = hash.slice(SHARE_PREFIX_DEFLATE.length);
  try {
    let bytes = base64UrlToBytes(body);
    if (compressed) {
      if (!canCompress()) return null;
      bytes = await through(bytes, new DecompressionStream('deflate-raw'));
    }
    const raw: unknown = JSON.parse(new TextDecoder().decode(bytes));
    if (!isRecord(raw)) return null;
    if (raw['v'] !== 1) return null;
    return {
      v: 1,
      doc: typeof raw['doc'] === 'string' ? raw['doc'] : '',
      opts: sanitizeOptions(raw['opts']),
    };
  } catch {
    return null;
  }
}

/* ------------------------------------------------------------------------- load */

export interface LoadInit {
  /** `location.hash`, or whatever a test wants to hand over. */
  fragment?: string | null;
  /**
   * `location.search`. The manifest's `share_target` is a GET target (§9, W8), so
   * Android's share sheet arrives as `?text=…` — an explicit act by the person
   * sharing, and therefore ahead of anything stored, like a fragment.
   */
  query?: string | null;
  storage?: StorageLike | null;
}

/**
 * The document a `share_target` GET carried, or `null`.
 *
 * `title` and `url` are accepted by the manifest because a share sheet sends whatever
 * it has and a target that rejects them gets skipped, but only `text` is a document;
 * a share with no `text` falls through to the ordinary load path rather than pasting
 * a URL into the editor.
 */
export function sharedText(query: string | null | undefined): string | null {
  if (!query) return null;
  try {
    const text = new URLSearchParams(query.startsWith('?') ? query.slice(1) : query).get('text');
    return text && text.trim() !== '' ? text : null;
  } catch {
    return null;
  }
}

export interface LoadedState {
  state: AppState;
  /** Which of the three sources the document and options came from. */
  source: 'share-target' | 'fragment' | 'storage' | 'defaults';
  /** No fragment and no stored document: the app loads example 1 (§6.7). */
  firstVisit: boolean;
}

/**
 * Fragment > `localStorage` > defaults (§6.4).
 *
 * The `ui` half never comes from a fragment: a link reproduces a *session*, not
 * somebody else's window. The fragment itself is left in place by the caller, so a
 * reload reproduces it.
 */
export async function loadState(init: LoadInit = {}): Promise<LoadedState> {
  const storage = init.storage ?? null;
  const ui = sanitizeUi(readJson(storage, STORAGE_KEYS.ui));

  const fromShareSheet = sharedText(init.query);
  if (fromShareSheet !== null) {
    return {
      state: {
        v: 1,
        doc: fromShareSheet,
        opts: withDefaults(sanitizeOptions(readJson(storage, STORAGE_KEYS.opts))),
        ui,
      },
      source: 'share-target',
      firstVisit: false,
    };
  }

  const shared = await decodeShare(init.fragment);
  if (shared) {
    return {
      state: { v: 1, doc: shared.doc, opts: withDefaults(shared.opts), ui },
      source: 'fragment',
      firstVisit: false,
    };
  }

  let storedDoc: string | null = null;
  try {
    storedDoc = storage?.getItem(STORAGE_KEYS.doc) ?? null;
  } catch {
    storedDoc = null;
  }
  const storedOpts = readJson(storage, STORAGE_KEYS.opts);
  const opts = withDefaults(sanitizeOptions(storedOpts));
  const known = storedDoc !== null || storedOpts !== undefined;

  return {
    state: { v: 1, doc: storedDoc ?? '', opts, ui },
    source: known ? 'storage' : 'defaults',
    firstVisit: storedDoc === null,
  };
}
