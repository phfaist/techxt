/**
 * The contract between `main.ts` and the UI modules.
 *
 * Five of the six are here. The sixth, `ui/sheets.ts`, keeps its own: it talks to
 * the app through one optional callback and answers no question about a conversion,
 * so an entry in this file would be a table of contents and nothing more.
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
import type { LibraryEntry, LibraryStats, PruneProposal } from '../library';
import type { ImportChoice } from '../library-io';
import type { AppOptions, ExampleDoc, UiState } from '../types';
import type { ConversionResult, Diagnostic, MathRegion } from '../worker/protocol';

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
  /** ⭐ Save, beside Copy and Download: star this document's library entry (§6.10). */
  onStar(): void;
  /** Ctrl/Cmd+Enter — convert now, skipping the debounce. */
  onConvertNow(): void;
  /** A gutter marker was clicked: reveal that diagnostic in the panel. */
  onMarkerSelect(diagnostic: Diagnostic): void;
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
  /**
   * Wrap each math region of the text last given to {@link setOutput} in an element,
   * and hand those elements back in output order for the caller to typeset (§6.3).
   *
   * The text does not change — the elements are built with `createElement` and
   * `createTextNode` around slices of the string that is already there, so
   * {@link getOutput} still returns exactly what was set and Copy, Download and the
   * library still hand over the library's own bytes. Whoever typesets the elements is
   * `main.ts`; the pane has no idea what MathJax is.
   */
  markMath(regions: readonly MathRegion[]): HTMLElement[];
  /** Focus the textarea and select `[start, end)` in UTF-16 code units (§4.4). */
  selectSpan(start: number, end: number): void;
  /**
   * The latest diagnostics, for the in-editor underline and the gutter markers
   * (error/warning only — a note has nothing to point at that is worth the ink).
   */
  setDiagnostics(diagnostics: readonly Diagnostic[]): void;
  /** The current fit-to-pane column count for the output pane (§6.5). */
  columns(): number;
  /**
   * Fold the output's long lines to the pane's own width instead of scrolling
   * sideways — CSS only, and never a change to the text `getOutput` returns (§6.3).
   */
  setSoftWrap(enabled: boolean): void;
  /** Apply a display font; resolves when the face has loaded (or failed to). */
  setFont(font: FontId, size: number): Promise<void>;
  setSplit(split: number): void;
  setFocus(focus: 'input' | 'output' | null): void;
  /** Show or clear the "loading font…" state in the output pane header. */
  setFontLoading(loading: boolean): void;
  /**
   * Whether this document's library entry is starred, for the ⭐ Save button. `null`
   * where there is no library to star into at all, which hides the button.
   */
  setStarred(starred: boolean | null): void;
  /** A hard parse failure dims the (stale) output rather than blanking it. */
  setStale(stale: boolean): void;
}

/* ---------------------------------------------------------------- ui/controls.ts */

export interface ControlsInit {
  mount: HTMLElement;
  /** Whether the library button should catch the eye this session (§6.10). */
  libraryNew?: boolean;
  ui: UiState;
  /** The options the user has changed; everything else is a library default. */
  options: AppOptions;
  /**
   * The techxt version, for the "report a bug" line of the More panel — `''` at
   * construction, because the bar is built before wasm has loaded so the page is
   * usable while it does. {@link Controls.setVersion} fills it in on `ready`.
   */
  version: string;
  /** A whole new option object — the caller diffs and re-converts. */
  onOptionsChange(options: AppOptions): void;
  onFontChange(font: FontId, size: number): void;
  onMoreToggle(open: boolean): void;
  /**
   * "Keep everything offline" was ticked or unticked (§8.3): the display faces, and
   * the MathJax bundle that is the app's other lazily fetched asset (§9.1). The name
   * keeps `Fonts` because the stored `UiState` key does.
   */
  onKeepFontsOffline(enabled: boolean): void;
  /** The Library button beside *More options* — the second door of §6.10. */
  onOpenLibrary(): void;
}

export interface Controls {
  /** Reflect options the app changed itself (a share link, a reset). */
  setOptions(options: AppOptions): void;
  setFont(font: FontId, size: number): void;
  setMoreOpen(open: boolean): void;
  setKeepFontsOffline(enabled: boolean): void;
  /** Fill in the embedded techxt version once the worker has reported it. */
  setVersion(version: string): void;
  /** Stop drawing attention to the library button; it has been noticed (§6.10). */
  setLibraryNew(pulse: boolean): void;
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
  /** A gutter marker was clicked: open the panel and scroll that row into view. */
  reveal(diagnostic: Diagnostic): void;
}

/* ----------------------------------------------------------- ui/library-pane.ts */

/** A line in the library header about the storage itself, not about an entry. */
export interface LibraryNotice {
  tone: 'info' | 'warn';
  message: string;
}

/** What an import dialog has to be able to say before the user answers it. */
export interface ImportRequest {
  /** How many entries the file holds, after every one has been validated. */
  incoming: number;
  /** What the file says about when it was written, or `null`. */
  exportedAt: string | null;
  /** What the file lost on the way in, so the dialog can admit it. */
  dropped: { malformed: number; oversize: number };
  /** What is here already — Replace has to name what it would cost. */
  existing: { count: number; starred: number };
}

/** The two answers to a full disk. There is no third: nothing is removed silently. */
export type PruneAnswer = 'remove' | 'decline';

export interface LibraryPaneInit {
  mount: HTMLElement;
  /** Load this entry's document *and* its options into the editor (§6.10). */
  onOpenEntry(entry: LibraryEntry): void;
  onStar(entry: LibraryEntry, starred: boolean): void;
  onRename(entry: LibraryEntry, title: string): void;
  onDelete(entry: LibraryEntry): void;
  onCopySource(entry: LibraryEntry): void;
  onDownloadSource(entry: LibraryEntry): void;
  /** Save the whole library as one file (§6.11). */
  onExport(): void;
  /** A file the user chose; the app decodes it and asks the pane what to do with it. */
  onImportFile(file: File): void;
  /** Remove everything — only ever called after the pane's own typed confirmation. */
  onClear(): void;
}

export interface LibraryPane {
  /** The entries to show and the header line above them. */
  setEntries(entries: readonly LibraryEntry[], stats: LibraryStats): void;
  /** One unobtrusive line about the storage: near the quota, or a private window. */
  setNotice(notice: LibraryNotice | null): void;
  /** Turn the pane into an honest inert state: no IndexedDB here (§6.10). */
  setUnavailable(reason: string): void;
  /** Select an entry, or `null` for the list. */
  select(id: string | null): void;
  focusSearch(): void;
  /** Ask what an import should do. Resolves to `null` if the user backed out. */
  askImport(request: ImportRequest): Promise<ImportChoice | null>;
  /**
   * The full-disk proposal: what the app *offers* to remove, with Export first. It
   * removes nothing itself, and a `'decline'` is a complete answer (§6.10).
   */
  askPrune(proposal: PruneProposal): Promise<PruneAnswer>;
}
