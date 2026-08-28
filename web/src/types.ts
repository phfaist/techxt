/**
 * The app-level types shared by the state codec, the wiring and the UI modules.
 *
 * Two settings live here rather than in {@link OptionsPayload} because they are the
 * *app* being helpful rather than the library offering a choice (web/PLAN.md §5):
 * `wrap: 'fit'` measures the output pane and sends the resulting column count, and
 * `todayMode: 'browser'` sends the date the browser knows where the `no_std` library
 * can only render `<today>`. Both are resolved into real library options by
 * `resolveOptions` in `state.ts`, so the worker never sees an app-level value.
 */

import type { FontId } from './fonts';
import type { OptionsPayload } from './worker/protocol';

/**
 * `'fit'` measures the pane (§6.5), `'off'` and `'soft'` are both the library's own
 * default (no wrapping at all) and differ only in what the *pane* does with the long
 * lines that result — `'off'` scrolls sideways, `'soft'` folds them to the pane's
 * width for display only (§6.3) — and a number is an explicit column count.
 *
 * `'soft'` is the one wrap value that never reaches the library: it is a property of
 * the HTML the answer is shown in, not of the answer. Copy and Download hand over the
 * text the library produced, so what leaves the app is identical under `'off'`.
 */
export type WrapMode = 'fit' | 'off' | 'soft' | number;

/** The column presets the primary bar offers besides *Fit* and *Off*. */
export const WRAP_PRESETS: readonly number[] = [40, 60, 72, 80, 100, 120];

/** Where `\today` comes from: the browser's clock, the library's `<today>`, or a literal. */
export type TodayMode = 'browser' | 'library' | 'custom';

/**
 * Everything that changes the converted text: the library options the user changed,
 * plus the two app-level settings above. Absent means "the library's default" for
 * every {@link OptionsPayload} key, which is what keeps a share link short and lets a
 * future change of a library default be picked up rather than frozen (§6.4).
 *
 * `wrapWidth` and `today` are deliberately **not** set here — they are computed from
 * `wrap`, `todayMode` and `todayCustom` at conversion time.
 */
export interface AppOptions extends Omit<OptionsPayload, 'wrapWidth' | 'today'> {
  wrap?: WrapMode;
  todayMode?: TodayMode;
  todayCustom?: string;
}

/** The keys of {@link AppOptions} that are app-level rather than library options. */
export const APP_ONLY_KEYS = ['wrap', 'todayMode', 'todayCustom'] as const;

/** Presentation preferences. None of these changes a single character of the output. */
export interface UiState {
  font: FontId;
  /** Pane font size in CSS pixels, between `MIN_SIZE` and `MAX_SIZE`. */
  size: number;
  /** The input pane's share of the horizontal split, clamped to 0.2–0.8. */
  split: number;
  /** Whether the "More options" disclosure is open. */
  moreOpen: boolean;
  /** Whether the diagnostics panel is expanded. */
  diagnosticsOpen: boolean;
  /** Which pane the mobile ⇅ Focus button maximises; `null` shows both. */
  focus: 'input' | 'output' | null;
  /** Whether the user asked for every face to be fetched (§8.3). */
  keepFontsOffline: boolean;
}

/**
 * The one versioned object (§6.4). `ui` is persisted but never shared: a link
 * reproduces a *session*, not somebody else's window.
 */
export interface AppState {
  v: 1;
  doc: string;
  opts: AppOptions;
  ui: UiState;
}

/** What a share fragment carries. */
export interface SharedState {
  v: 1;
  doc: string;
  opts: AppOptions;
}

/** One of the five inlined sample documents (§6.7). */
export interface ExampleDoc {
  id: string;
  /** Menu entry, e.g. "A paper fragment". */
  title: string;
  /** One line saying what it demonstrates. */
  blurb: string;
  source: string;
}
