/**
 * Bootstrap and wiring (web/PLAN.md §6, §9).
 *
 * This file owns everything the UI modules deliberately do not: the state that
 * outlives a keystroke, the worker, the clipboard, the keyboard shortcuts, the share
 * link and the service worker. The modules under `src/ui/` build the page and report
 * user actions; this decides what those actions mean.
 *
 * It is wiring, and it is meant to stay that way. Where a piece of it grows a
 * decision — what a share link contains, when a conversion is worth issuing, what
 * "changed from the default" means — that decision lives in `state.ts` or
 * `convert-client.ts` and only its *use* is here.
 */

import './styles.css';
import { registerSW } from 'virtual:pwa-register';

import { initAbout } from './about';
import { ConvertClient } from './convert-client';
import type { RequestMode } from './convert-client';
import { DEFAULT_EXAMPLE, EXAMPLES } from './examples';
import { applyFont, preloadAllFonts } from './fonts';
import type { FontId } from './fonts';
import {
  SHARE_LENGTH_LIMIT,
  browserStorage,
  createPersistence,
  encodeShare,
  encodeShareSettingsOnly,
  loadState,
  resolveOptions,
  sanitizeOptions,
  withDefaults,
} from './state';
import type { AppOptions, ExampleDoc } from './types';
import type { BusyState, Controls, DiagnosticsPanel, Panes, Toaster } from './ui/api';
import { initControls } from './ui/controls';
import { initDiagnostics } from './ui/diagnostics';
import { initPanes } from './ui/panes';
import { initSheets } from './ui/sheets';
import { initToast } from './ui/toast';
import type { ConversionResult, Diagnostic } from './worker/protocol';

/** The status-line aside for a document the browser will not keep for us (§6.4). */
const DOC_TOO_LARGE = 'too large to save locally — this document will not survive a reload';

const ISSUE_URL = 'https://github.com/phfaist/techxt/issues/new';

function mountPoint(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) throw new Error(`techxt: the page is missing #${id}`);
  return element;
}

/* ------------------------------------------------------------------- clipboard */

/**
 * `navigator.clipboard` where it exists and is allowed, and the old
 * `<textarea>` + `execCommand` dance where it is not — which on iOS below 13.4 is
 * the only thing there is (§6.3).
 */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through: a permission refusal is not the end of the story */
  }
  try {
    const carrier = document.createElement('textarea');
    carrier.value = text;
    carrier.setAttribute('readonly', '');
    carrier.style.position = 'fixed';
    carrier.style.top = '0';
    carrier.style.left = '-9999px';
    document.body.appendChild(carrier);
    carrier.focus();
    carrier.select();
    carrier.setSelectionRange(0, text.length);
    const copied = document.execCommand('copy');
    carrier.remove();
    return copied;
  } catch {
    return false;
  }
}

/* -------------------------------------------------------------------- download */

/** The first `\title` or `\section` of the document, if it has one (§6.3). */
const TITLE_PATTERN = /\\(?:title|section)\*?\s*(?:\[[^\]]*\])?\{([^{}]{1,120})\}/;

function slugify(raw: string): string {
  return raw
    .replace(/\\[a-zA-Z]+\s*/g, ' ')
    .replace(/[{}$\\]/g, ' ')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+/, '')
    .slice(0, 48)
    .replace(/-+$/, '');
}

function downloadName(document_: string): string {
  const match = TITLE_PATTERN.exec(document_);
  const slug = match?.[1] ? slugify(match[1]) : '';
  return slug ? `${slug}.txt` : 'converted.txt';
}

function downloadText(text: string, name: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.rel = 'noopener';
  document.body.appendChild(link);
  link.click();
  link.remove();
  // Long enough for every browser to have started reading it, short enough not to
  // hold a copy of the output for the rest of the session.
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

/**
 * `launchQueue`, which no TypeScript DOM library declares yet. Only the two members
 * the file handler of §9 uses are named here — inventing more would be inventing.
 */
interface LaunchParams {
  files?: FileSystemFileHandle[];
}
interface LaunchQueue {
  setConsumer(consumer: (params: LaunchParams) => void): void;
}

/* ------------------------------------------------------------ service worker */

function registerServiceWorker(toast: Toaster): void {
  const update = registerSW({
    immediate: true,
    // `registerType: 'autoUpdate'` is configured in vite.config.ts, and in that mode
    // the plugin calls `onNeedReload` — the hook that *replaces* its automatic
    // reload — rather than `onNeedRefresh`. Both are wired so the toast of §9
    // appears whichever mode a future build chooses.
    onNeedReload: () => announce(),
    onNeedRefresh: () => announce(),
  });

  function announce(): void {
    toast.show({
      message: 'A new version is ready.',
      timeoutMs: 0,
      action: {
        label: 'Reload',
        onSelect: () => {
          void update(true).finally(() => {
            window.location.reload();
          });
        },
      },
    });
  }
}

/* ------------------------------------------------------------------- the app */

async function start(): Promise<void> {
  const toast = initToast(mountPoint('toast-mount'));
  const about = initAbout();
  // About and Install are dialogs over the tool (§6.8). A modal makes everything
  // behind it inert, so the disclosure has to be told to put itself away first —
  // otherwise it is still open, and still open when the sheet closes.
  initSheets({
    onOpen() {
      controls.close();
      state.ui.moreOpen = false;
      persistence.ui(state.ui);
    },
  });
  registerServiceWorker(toast);

  const storage = browserStorage();
  const loaded = await loadState({
    fragment: window.location.hash,
    query: window.location.search,
    storage,
  });
  const state = loaded.state;
  // An empty output pane is a bad first impression and a bad demo (§6.7).
  if (loaded.firstVisit && state.doc === '') state.doc = DEFAULT_EXAMPLE.source;

  // Assigned below, before anything can call back into them: the UI modules are built
  // synchronously and the worker only answers on a later task.
  let panes: Panes;
  let controls: Controls;
  let diagnostics: DiagnosticsPanel;

  /** The last output we are willing to show: what a cancel or a crash falls back to. */
  let lastGoodOutput = '';
  /** The pane's latest fit-to-pane measurement; the pane owns the measuring (§6.5). */
  let measuredColumns = 72;
  /** The column count the last request was issued with; negative before the first. */
  let lastColumns = -1;

  const persistence = createPersistence({
    storage,
    onDocumentOversize(oversize) {
      diagnostics.setNote(oversize ? DOC_TOO_LARGE : null);
    },
  });

  /* ------------------------------------------------------------- conversions */

  function requestConversion(mode: RequestMode): void {
    lastColumns = measuredColumns;
    client.convert(state.doc, resolveOptions(state.opts, lastColumns), mode);
  }

  function handleResult(result: ConversionResult): void {
    diagnostics.setResult(result);
    panes.setDiagnostics(result.diagnostics);
    if (result.ok) {
      lastGoodOutput = result.text;
      panes.setOutput(result.text);
      panes.setStale(false);
    } else {
      // A hard parse failure has no text of its own; the previous output stays,
      // dimmed, with the diagnostics saying why (§6.3).
      panes.setStale(true);
    }
  }

  function handleFatal(message: string): void {
    panes.setOutput(lastGoodOutput);
    panes.setStale(true);
    diagnostics.setBusy('idle');
    toast.show({
      message: 'The converter crashed on that document. That is a techxt bug, not yours.',
      tone: 'alert',
      timeoutMs: 0,
      action: {
        label: 'Report it',
        onSelect: () => {
          void reportBug(message);
        },
      },
    });
  }

  /** The bug report a panic deserves: the link *is* the reproduction (§6.2). */
  async function reportBug(message: string): Promise<void> {
    const fragment = await encodeShare({ v: 1, doc: state.doc, opts: state.opts });
    let link = shareUrl(fragment);
    if (fragment.length > SHARE_LENGTH_LIMIT) {
      const settingsOnly = await encodeShareSettingsOnly(state.opts);
      link = `${shareUrl(settingsOnly)}\n(the document itself was too long to put in a link)`;
    }
    const url = new URL(ISSUE_URL);
    url.searchParams.set('title', 'web: the converter crashed on a document');
    url.searchParams.set(
      'body',
      [
        'The web app reported a fatal error while converting.',
        '',
        `techxt version: ${client.version ?? 'unknown'}`,
        `user agent: ${navigator.userAgent}`,
        '',
        'Reproduction (this link carries the document and the options):',
        link,
        '',
        'Error:',
        '```',
        message,
        '```',
      ].join('\n'),
    );
    window.open(url.toString(), '_blank', 'noopener');
  }

  const client = new ConvertClient({
    createWorker: () =>
      new Worker(new URL('./worker/convert.worker.ts', import.meta.url), { type: 'module' }),
    onReady(version) {
      // The version exists only once wasm has loaded, and both places that show it
      // were built before that (§6.8, and the bug-report line of the More panel).
      about.setVersion(version);
      controls.setVersion(version);
    },
    onResult: handleResult,
    onBusy(busy: BusyState) {
      diagnostics.setBusy(busy);
    },
    onFatal: handleFatal,
  });

  /* --------------------------------------------------------------- the fonts */

  async function useFont(font: FontId, size: number, persist: boolean): Promise<void> {
    state.ui.font = font;
    state.ui.size = size;
    if (persist) persistence.ui(state.ui);
    panes.setFontLoading(true);
    try {
      // `applyFont` sets the two custom properties `styles.css` reads; `panes.setFont`
      // is the pane's own hook for it (re-measuring the fit-to-pane sample, §6.5).
      // Both are idempotent, and the pane owns the measurement.
      await applyFont(font, size);
      await panes.setFont(font, size);
    } finally {
      panes.setFontLoading(false);
    }
  }

  /* --------------------------------------------------------------- the panes */

  panes = initPanes({
    mount: mountPoint('panes-mount'),
    ui: state.ui,
    examples: EXAMPLES,
    onInput(text, cause) {
      state.doc = text;
      persistence.document(text);
      // A paste is a decision, not a keystroke, so it does not wait (§6.2).
      requestConversion(cause === 'paste' ? 'immediate' : 'debounced');
    },
    onColumnsChange(columns) {
      measuredColumns = columns;
      // Only *Fit* cares, only a real change is worth a conversion, and nothing at
      // all before the first conversion has been issued (§6.5).
      if (lastColumns < 0 || columns === lastColumns) return;
      if ((state.opts.wrap ?? 'fit') !== 'fit') return;
      requestConversion('immediate');
    },
    onSplitChange(split) {
      state.ui.split = split;
      persistence.ui(state.ui);
    },
    onFocusChange(focus) {
      state.ui.focus = focus;
      persistence.ui(state.ui);
    },
    onCopy() {
      void copyOutput();
    },
    onDownload() {
      const text = panes.getOutput() || lastGoodOutput;
      const name = downloadName(state.doc);
      downloadText(text, name);
      toast.show({ message: `Saved ${name}` });
    },
    onLoadExample(example) {
      loadExample(example);
    },
    onConvertNow() {
      requestConversion('immediate');
    },
    onMarkerSelect(diagnostic) {
      state.ui.diagnosticsOpen = true;
      persistence.ui(state.ui);
      diagnostics.reveal(diagnostic);
    },
  });

  /* ------------------------------------------------------------ the controls */

  controls = initControls({
    mount: mountPoint('controls-mount'),
    ui: state.ui,
    options: state.opts,
    // The version only exists once the worker has answered `ready`, and the controls
    // are built before that so the page is usable while wasm loads. `about.setVersion`
    // fills the About section in when it arrives.
    version: client.version ?? '',
    onOptionsChange(next: AppOptions) {
      state.opts = withDefaults(sanitizeOptions(next));
      persistence.options(state.opts);
      requestConversion('immediate');
    },
    onFontChange(font, size) {
      void useFont(font, size, true);
    },
    onMoreToggle(open) {
      state.ui.moreOpen = open;
      persistence.ui(state.ui);
    },
    onKeepFontsOffline(enabled) {
      state.ui.keepFontsOffline = enabled;
      persistence.ui(state.ui);
      if (!enabled) return;
      void preloadAllFonts(state.ui.size).then(() => {
        toast.show({ message: 'Every display font is cached for offline use.' });
      });
    },
  });

  /* --------------------------------------------------------- the status strip */

  diagnostics = initDiagnostics({
    mount: mountPoint('status-mount'),
    open: state.ui.diagnosticsOpen,
    onToggle(open) {
      state.ui.diagnosticsOpen = open;
      persistence.ui(state.ui);
    },
    onSelect(diagnostic: Diagnostic) {
      if (diagnostic.span) panes.selectSpan(diagnostic.span.start, diagnostic.span.end);
    },
    onCancel() {
      client.cancel();
      panes.setOutput(lastGoodOutput);
      panes.setStale(true);
      diagnostics.setBusy('idle');
      toast.show({ message: 'Conversion cancelled; the last result is still shown.' });
    },
  });

  /* ------------------------------------------------------------------ actions */

  async function copyOutput(): Promise<void> {
    const text = panes.getOutput() || lastGoodOutput;
    const copied = await copyText(text);
    toast.show(
      copied
        ? { message: 'Text copied.' }
        : { message: 'Could not reach the clipboard — select the text and copy it.', tone: 'alert' },
    );
  }

  function shareUrl(fragment: string): string {
    const { origin, pathname, search } = window.location;
    return `${origin}${pathname}${search}${fragment}`;
  }

  /** Loading an example throws work away, so it comes with one level of undo (§6.7). */
  function loadExample(example: ExampleDoc): void {
    const previous = state.doc;
    setDocument(example.source);
    toast.show({
      message: `Loaded “${example.title}”.`,
      action: {
        label: 'Undo',
        onSelect: () => {
          setDocument(previous);
        },
      },
    });
  }

  function setDocument(text: string): void {
    state.doc = text;
    panes.setDocument(text);
    persistence.document(text);
    requestConversion('immediate');
  }

  /**
   * The manifest's `file_handlers` (§9, W8): an installed copy can be asked to open a
   * `.tex`, and the file arrives through `launchQueue` rather than through any URL.
   * Additive — a browser without it simply never calls this — and it replaces the
   * document, so it offers the same single-level undo the Load menu does (§6.7).
   */
  function registerFileHandler(): void {
    const queue = (window as unknown as { launchQueue?: LaunchQueue }).launchQueue;
    if (!queue || typeof queue.setConsumer !== 'function') return;
    queue.setConsumer((params) => {
      void (async () => {
        const handle = params?.files?.[0];
        if (!handle) return;
        let text: string;
        let name: string;
        try {
          const file = await handle.getFile();
          text = await file.text();
          name = file.name;
        } catch {
          toast.show({ message: 'That file could not be read.', tone: 'alert' });
          return;
        }
        const previous = state.doc;
        setDocument(text);
        toast.show({
          message: `Opened “${name}”.`,
          action: { label: 'Undo', onSelect: () => setDocument(previous) },
        });
      })();
    });
  }

  /* ----------------------------------------------------------- the keyboard */

  document.addEventListener('keydown', (event: KeyboardEvent) => {
    // A module that has already acted on this key owns it — the pane's own
    // Ctrl/Cmd+Enter, the Load menu's Escape.
    if (event.defaultPrevented) return;
    if (event.key === 'Escape') {
      controls.close();
      state.ui.moreOpen = false;
      persistence.ui(state.ui);
      return;
    }
    if (!(event.ctrlKey || event.metaKey)) return;
    if (event.key === 'Enter') {
      event.preventDefault();
      requestConversion('immediate');
      return;
    }
    if (event.shiftKey && (event.key === 'c' || event.key === 'C')) {
      event.preventDefault();
      void copyOutput();
    }
  });

  /* -------------------------------------------------------------- the finish */

  const flush = (): void => {
    persistence.flush();
  };
  window.addEventListener('pagehide', flush);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') flush();
  });

  // The pane got `ui` at init, so the split, the focus and the size are already its
  // own; the document is the one piece `PanesInit` does not carry.
  registerFileHandler();

  panes.setDocument(state.doc);
  measuredColumns = panes.columns();
  // The font is applied without being awaited: the first conversion should not wait
  // for a face to arrive, and the pane re-measures when it does (§6.5).
  void useFont(state.ui.font, state.ui.size, false);
  if (state.ui.keepFontsOffline) void preloadAllFonts(state.ui.size);
  requestConversion('immediate');
}

void start();
