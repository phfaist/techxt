/**
 * The contract between `main.ts` and the four UI modules.
 *
 * Each module owns a region of the page: it builds its own markup into the mount it
 * is given, owns the class names it styles in `styles.css`, and talks to the rest of
 * the app only through the callbacks of its `*Init` object and the methods of the
 * handle it returns. Nothing in `ui/` reads or writes `localStorage`, spawns the
 * worker, or knows what a conversion is — that is `main.ts`'s job.
 *
 * Every `on…` callback reports a *user action*, never a state change the app made
 * itself: calling `panes.setDocument(…)` must not fire `onInput`.
 */

import type { FontId } from '../fonts';
import type { AppOptions, ExampleDoc, UiState } from '../types';
import type { ConversionResult, Diagnostic } from '../worker/protocol';

/* ------------------------------------------------------------------ ui/toast.ts */

export interface ToastAction {
  label: string;
  onSelect(): void;
}

export interface ToastOptions {
  message: string;
  /** A single optional action — "Undo", "Reload", "Report". */
  action?: ToastAction;
  /** Auto-dismiss delay; `0` keeps it until dismissed. Default 4000. */
  timeoutMs?: number;
  /** `'alert'` for a failure the user must notice; default `'status'`. */
  tone?: 'status' | 'alert';
}

export interface Toaster {
  show(options: ToastOptions): void;
  dismiss(): void;
}

/* ------------------------------------------------------------------ ui/panes.ts */

/** What the pane region reports upwards. */
export interface PanesInit {
  mount: HTMLElement;
  ui: UiState;
  /** The Load ▾ menu's entries. */
  examples: readonly ExampleDoc[];
  /** Fired on every keystroke or paste; debouncing belongs upstream. */
  onInput(text: string, cause: 'type' | 'paste'): void;
  /** Fired when the measured fit-to-pane column count changes (§6.5). */
  onColumnsChange(columns: number): void;
  onSplitChange(split: number): void;
  onFocusChange(focus: 'input' | 'output' | null): void;
  onCopy(): void;
  onDownload(): void;
  onLoadExample(example: ExampleDoc): void;
  /** Ctrl/Cmd+Enter — convert now, skipping the debounce. */
  onConvertNow(): void;
}

export interface Panes {
  /** The textarea itself, for focus and selection from the diagnostics panel. */
  readonly input: HTMLTextAreaElement;
  getDocument(): string;
  /** Replace the document without firing `onInput`. */
  setDocument(text: string): void;
  /** Assign the converted text (`textContent` only — never `innerHTML`). */
  setOutput(text: string): void;
  getOutput(): string;
  /** Focus the textarea and select `[start, end)` in UTF-16 code units (§4.4). */
  selectSpan(start: number, end: number): void;
  /** The current fit-to-pane column count for the output pane (§6.5). */
  columns(): number;
  /** Apply a display font; resolves when the face has loaded (or failed to). */
  setFont(font: FontId, size: number): Promise<void>;
  setSplit(split: number): void;
  setFocus(focus: 'input' | 'output' | null): void;
  /** Show or clear the "loading font…" state in the output pane header. */
  setFontLoading(loading: boolean): void;
  /** A hard parse failure dims the (stale) output rather than blanking it. */
  setStale(stale: boolean): void;
}

/* ---------------------------------------------------------------- ui/controls.ts */

export interface ControlsInit {
  mount: HTMLElement;
  ui: UiState;
  /** The options the user has changed; everything else is a library default. */
  options: AppOptions;
  /** The techxt version, for the "report a bug" line of the More panel. */
  version: string;
  /** A whole new option object — the caller diffs and re-converts. */
  onOptionsChange(options: AppOptions): void;
  onFontChange(font: FontId, size: number): void;
  onMoreToggle(open: boolean): void;
  /** "Keep all fonts offline" was ticked or unticked (§8.3). */
  onKeepFontsOffline(enabled: boolean): void;
  /** "Copy link" — the caller builds the fragment and writes the clipboard (§6.4). */
  onShare(): void;
}

export interface Controls {
  /** Reflect options the app changed itself (a share link, a reset). */
  setOptions(options: AppOptions): void;
  setFont(font: FontId, size: number): void;
  setMoreOpen(open: boolean): void;
  setKeepFontsOffline(enabled: boolean): void;
  /** Close the disclosure — what Escape does (§6.9). */
  close(): void;
}

/* ------------------------------------------------------------- ui/diagnostics.ts */

/** What the status strip shows while a conversion is in flight (§6.2). */
export type BusyState = 'idle' | 'converting' | 'cancellable';

export interface DiagnosticsInit {
  mount: HTMLElement;
  open: boolean;
  onToggle(open: boolean): void;
  /** A row with a span was activated: jump to it in the input. */
  onSelect(diagnostic: Diagnostic): void;
  /** The Cancel button of the `cancellable` state was pressed. */
  onCancel(): void;
}

export interface DiagnosticsPanel {
  /** The whole status line and panel are a function of the latest result. */
  setResult(result: ConversionResult): void;
  setBusy(state: BusyState): void;
  /** A persistent aside in the status line, e.g. "too large to save locally". */
  setNote(note: string | null): void;
  setOpen(open: boolean): void;
}
