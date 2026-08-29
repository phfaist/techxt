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
  PRUNE_PROPOSAL_SIZE,
  adoptionOnOpen,
  createLibrary,
  describeSession,
  makePreview,
  pruneProposal,
  quotaPressure,
  statsOf,
} from './library';
import type { Library, LibraryEntry } from './library';
import { decodeLibrary, describeImport, encodeLibrary, libraryFileName, planImport } from './library-io';
import { isPersisted, openLibraryBackend, requestPersistence, storageEstimate } from './library-store';
import { openLibraryChannel } from './library-sync';
import type { LibraryChannel } from './library-sync';
import { loadMathJax, resetMathJax, typeset } from './mathjax';
import {
  DEFAULT_OPTIONS,
  SHARE_LENGTH_LIMIT,
  browserStorage,
  createPersistence,
  encodeShare,
  encodeShareSettingsOnly,
  loadState,
  mathJax,
  migrateCurrentEntryId,
  readCurrentEntryId,
  readLibraryHints,
  resolveOptions,
  sanitizeOptions,
  shouldPulseLibrary,
  softWraps,
  tabStorage,
  withDefaults,
  writeCurrentEntryId,
  writeLibraryHints,
} from './state';
import { downloadName, sourceFileName } from './title';
import type { AppOptions, ExampleDoc } from './types';
import type {
  BusyState,
  Controls,
  DiagnosticsPanel,
  LibraryPane,
  Panes,
  Toaster,
} from './ui/api';
import { initControls } from './ui/controls';
import { initDiagnostics } from './ui/diagnostics';
import { initLibraryPane } from './ui/library-pane';
import { initPanes } from './ui/panes';
import { initSheets } from './ui/sheets';
import { initToast } from './ui/toast';
import type { ConversionResult, Diagnostic } from './worker/protocol';

/** The status-line aside for a document the browser will not keep for us (§6.4). */
const DOC_TOO_LARGE =
  'too large to save locally — this document will not survive a reload, and is not logged in your library';

/** The aside for a library the user asked to stop growing (§6.10). */
const LIBRARY_PAUSED = 'the library has stopped logging new documents — nothing in it was removed';

/** The aside for a library another tab has upgraded out from under this one (§6.10). */
const LIBRARY_STALE = 'the library was updated in another tab — reload to use it here';

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

function downloadText(text: string, name: string, type = 'text/plain;charset=utf-8'): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
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
  // Every sheet is a dialog over the tool (§6.8). A modal makes everything behind it
  // inert, so the disclosure has to be told to put itself away first — otherwise it
  // is still open, and still open when the sheet closes.
  const sheets = initSheets({
    onOpen(id) {
      controls.close();
      state.ui.moreOpen = false;
      persistence.ui(state.ui);
      if (id === 'library') openedLibrary();
    },
  });
  registerServiceWorker(toast);

  const storage = browserStorage();
  // What this *tab* remembers, as against what the origin does: one fact, the entry
  // being written into (§6.10). The migration runs before anything reads it, and
  // hands the id previous builds kept for the whole origin to whichever tab gets here
  // first.
  const tab = tabStorage();
  migrateCurrentEntryId(tab, storage);
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
  let libraryPane: LibraryPane;
  /**
   * The library (§6.10). It is `null` until IndexedDB has answered, which is a task
   * or two after the page is usable and deliberately not something the first
   * conversion waits for; {@link openLibrary} below is what fills it in.
   */
  let library: Library | null = null;
  /**
   * The line to the app's other tabs, or `null` where this browser has not got one
   * (§6.10). Opened beside the library and for the library's sake alone: nothing else
   * in the app is shared between two copies of it.
   */
  let channel: LibraryChannel | null = null;
  /**
   * Whether the database has been given up to another tab's upgrade. It is not the
   * same as a paused library and must not be described as one: the user asked for
   * neither, and only one of the two is fixed by a reload.
   */
  let libraryStale = false;

  /** The last output we are willing to show: what a cancel or a crash falls back to. */
  let lastGoodOutput = '';
  /** The pane's latest fit-to-pane measurement; the pane owns the measuring (§6.5). */
  let measuredColumns = 72;
  /** The column count the last request was issued with; negative before the first. */
  let lastColumns = -1;

  /** What the app remembers about introducing the library, and one more session. */
  const hints = readLibraryHints(storage);
  hints.sessions += 1;
  writeLibraryHints(storage, hints);

  /**
   * The two asides the status line can carry at once — a document too large to keep,
   * and a library that has stopped logging. Both are facts about *this* session that
   * the user can act on, and neither may hide the other.
   */
  const notes: { doc: string | null; library: string | null } = { doc: null, library: null };

  function updateNote(): void {
    const shown = [notes.doc, notes.library].filter((note) => note !== null);
    diagnostics.setNote(shown.length === 0 ? null : shown.join(' · '));
  }

  const persistence = createPersistence({
    storage,
    onDocumentOversize(oversize) {
      notes.doc = oversize ? DOC_TOO_LARGE : null;
      updateNote();
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
      typesetMath(result);
      // The library is a log of what was converted, so a conversion is exactly when
      // it hears about a document. The write itself is debounced (§6.10).
      library?.record(state.doc, state.opts, makePreview(result.text));
    } else {
      // A hard parse failure has no text of its own; the previous output stays,
      // dimmed, with the diagnostics saying why (§6.3).
      panes.setStale(true);
    }
  }

  /* -------------------------------------------------------------- typesetting */

  /**
   * The latest typeset pass. Conversions already carry a monotonic id and a stale
   * answer is dropped (§6.2); typesetting is the same problem one step later — it is
   * asynchronous, it can be slow on a large document, and by the time it finishes the
   * result it was rendering may have been superseded — so it is given the same
   * discipline rather than a second one of its own.
   */
  let typesetPass = 0;
  /**
   * The pass in flight. MathJax is one engine with one document's worth of state
   * (equation numbers, labels), so passes are chained rather than raced, and a pass
   * that was superseded while waiting for its turn never starts.
   */
  let typesetting: Promise<void> = Promise.resolve();
  /** Whether this session has already admitted that MathJax could not be fetched. */
  let mathJaxUnavailable = false;

  /**
   * Wrap this result's formulas in elements and hand them to MathJax (§6.3, §9.1).
   *
   * The pane is never blocked for this: `setOutput` has already put the text on the
   * screen, and each element still reads as its own LaTeX until the moment MathJax
   * replaces it. Nothing here changes the text, so Copy, Download and the library keep
   * handing over the library's own bytes.
   */
  function typesetMath(result: ConversionResult): void {
    // Bumped even when there is nothing to typeset: switching to *Fancy* mid-pass is
    // exactly the case where the pass in flight has to be dropped.
    const pass = (typesetPass += 1);
    if (!mathJax(state.opts) || result.regions.length === 0) return;
    const elements = panes.markMath(result.regions);
    if (elements.length === 0) return;
    typesetting = typesetting
      .then(async () => {
        // Superseded while an earlier pass was still running: these elements are not
        // in the pane any more, and the newer pass is right behind this one.
        if (pass !== typesetPass) return;
        resetMathJax();
        await typeset(elements);
      })
      .catch(mathJaxFailed);
  }

  /**
   * Ask for the typesetter as soon as the mode is chosen, rather than when the first
   * formula needs it — the fetch is 1.8 MB and the conversion it belongs to is
   * already on its way. Idempotent, so every path that can select the mode may call
   * it: a click on the control, a share link, a library entry, a reload (§9.1).
   */
  function prepareMathJax(): void {
    if (!mathJax(state.opts)) return;
    void loadMathJax().catch(mathJaxFailed);
  }

  /**
   * An installed copy fetches it once in the background instead, whether or not the
   * mode is ever selected: an app that was installed to work offline should not
   * discover the first time it is opened on a train that the typesetter is a download
   * away. On the web the same call would be 1.8 MB spent on the great majority of
   * visitors who never turn the mode on, which is exactly what §9.1's runtime route
   * exists to avoid — so it is asked for only where "keep it" is the whole point.
   *
   * The service worker holds it after the first run, so later runs are a cache read.
   */
  function prefetchMathJaxWhenInstalled(): void {
    if (mathJax(state.opts)) return; // `prepareMathJax` already asked for it
    if (!window.matchMedia?.('(display-mode: standalone)').matches) return;
    const fetchIt = (): void => {
      void loadMathJax().catch(() => {
        // Silent: nobody asked for this, and a copy that is offline right now will
        // pick it up on a later run.
      });
    };
    if (typeof window.requestIdleCallback === 'function') {
      window.requestIdleCallback(() => fetchIt());
    } else {
      window.setTimeout(fetchIt, 3000);
    }
  }

  /** Once per session: the formulas are still readable, they are just not typeset. */
  function mathJaxFailed(): void {
    if (mathJaxUnavailable) return;
    mathJaxUnavailable = true;
    toast.show({
      message: 'MathJax could not be loaded, so the formulas are shown as LaTeX source.',
      tone: 'alert',
      timeoutMs: 8000,
    });
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
      const before = state.doc;
      state.doc = text;
      persistence.document(text);
      // A paste is a decision, not a keystroke, so it does not wait (§6.2).
      requestConversion(cause === 'paste' ? 'immediate' : 'debounced');
      // The library is told about the *edit*, not just about the conversion it
      // provokes: a select-all-and-paste has replaced the document whether or not the
      // new one converts (§6.10).
      noteEdit(before, text);
    },
    onColumnsChange(columns) {
      measuredColumns = columns;
      // Only *Fit* cares, only a real change is worth a conversion, and nothing at
      // all before the first conversion has been issued (§6.5).
      if (lastColumns < 0 || columns === lastColumns) return;
      if ((state.opts.wrap ?? DEFAULT_OPTIONS.wrap) !== 'fit') return;
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
    onNew() {
      void newDocument();
    },
    onSave() {
      void saveVersion();
    },
    onStar() {
      void toggleStar();
    },
    onShowEntry() {
      const id = library?.session.entryId ?? null;
      sheets.open('library');
      if (id !== null) libraryPane.select(id);
    },
    onConvertNow() {
      requestConversion('immediate');
    },
    onMarkerSelect(diagnostic) {
      state.ui.diagnosticsOpen = true;
      persistence.ui(state.ui);
      diagnostics.reveal(diagnostic);
    },
    onCompletionQuery(query) {
      // Wiring, and nothing more: the pane decides when to ask, the binding decides what
      // the answer is and in what order (§4.9), and this carries the question to the
      // worker and the answer back. The document goes with it because the binding scans
      // it for the user's own `\newcommand`s, which is why `state.doc` is the argument
      // rather than the prefix alone (§6.13).
      client.complete(state.doc, query.prefix, query.limit, (items) => {
        panes.setCompletions(query, items);
      });
    },
  });

  /* ------------------------------------------------------------ the controls */

  controls = initControls({
    mount: mountPoint('controls-mount'),
    ui: state.ui,
    options: state.opts,
    libraryNew: shouldPulseLibrary(hints),
    // The version only exists once the worker has answered `ready`, and the controls
    // are built before that so the page is usable while wasm loads. `about.setVersion`
    // fills the About section in when it arrives.
    version: client.version ?? '',
    onOptionsChange(next: AppOptions) {
      state.opts = withDefaults(sanitizeOptions(next));
      persistence.options(state.opts);
      // The display half of `wrap: 'soft'` (§6.3): the library is told nothing about
      // it — `resolveOptions` drops it — so the pane has to be told directly. The same
      // is true of `math: 'mathjax'`, whose display half is the typesetter, and which
      // is fetched the moment it is selected rather than when a formula arrives.
      panes.setSoftWrap(softWraps(state.opts));
      prepareMathJax();
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
      // One "keep everything offline" rather than a checkbox per asset (§8.3, §9.1):
      // the display faces and the typesetter are the two things the app fetches after
      // the first load, and someone ticking this is answering the same question about
      // both. MathJax's failure is not worth a toast here — it was not asked for by
      // name — so the fonts' promise is what the message reports.
      void loadMathJax().catch(() => {
        /* the mode itself says so, loudly, if it is ever selected */
      });
      void preloadAllFonts(state.ui.size).then(() => {
        toast.show({ message: 'The display fonts and the typesetter are cached for offline use.' });
      });
    },
    onOpenLibrary() {
      sheets.open('library');
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

  /* ----------------------------------------------------------------- the library */

  libraryPane = initLibraryPane({
    mount: mountPoint('library-mount'),
    onOpenEntry(entry) {
      openEntry(entry);
    },
    onStar(entry, starred) {
      void (async () => {
        await library?.star(entry.id, starred);
        await refreshLibrary();
      })();
    },
    onRename(entry, title) {
      void (async () => {
        await library?.rename(entry.id, title);
        await refreshLibrary();
      })();
    },
    onDelete(entry) {
      void deleteEntry(entry);
    },
    onCopySource(entry) {
      void (async () => {
        const copied = await copyText(entry.source);
        toast.show(
          copied
            ? { message: `Copied the LaTeX of “${entry.title}”.` }
            : { message: 'Could not reach the clipboard.', tone: 'alert' },
        );
      })();
    },
    onDownloadSource(entry) {
      const name = sourceFileName(entry.title);
      downloadText(entry.source, name);
      toast.show({ message: `Saved ${name}` });
    },
    onExport() {
      void exportLibrary();
    },
    onImportFile(file) {
      void importLibraryFile(file);
    },
    onClear() {
      void (async () => {
        const emptied = (await library?.clear()) ?? false;
        await refreshLibrary();
        if (emptied) toast.show({ message: 'The library is empty.' });
      })();
    },
  });

  /**
   * Open the database and start logging.
   *
   * Deliberately not awaited by anything the first paint depends on: a browser that
   * takes a moment over IndexedDB — or one that never answers at all — must not be a
   * browser where the converter is slow to appear. Whatever has already been
   * converted by the time this resolves is logged then.
   */
  async function openLibrary(): Promise<void> {
    const backend = await openLibraryBackend(() => libraryClosed());
    if (!backend) {
      // Honest and inert, the way `browserStorage()` is for localStorage (§6.10).
      panes.setEntryState(null);
      libraryPane.setUnavailable(
        'This browser will not let the page keep a library here. Everything else works as usual.',
      );
      return;
    }

    // Before the library, so that the claim the adoption below makes is one the other
    // tabs actually hear.
    channel = openLibraryChannel((message) => {
      if (message.kind === 'changed') {
        // Another tab added, removed, starred, renamed or imported something. The
        // pane may be open on the old set, and `refreshLibrary` is what it already
        // does for this tab's own writes.
        void refreshLibrary();
        return;
      }
      entryTakenOver(message.id);
    });

    library = createLibrary({
      backend,
      persist: requestPersistence,
      onWrite(entry, created) {
        // The header names this entry, so it follows the title the document grows.
        if (entry.id === library?.session.entryId) {
          shownEntry = entry;
          syncEntry();
        }
        if (!created) return;
        void refreshLibrary();
        // Once, ever: the library only helps if people know it is there (§6.10).
        if (hints.toldAboutFirstSave) return;
        hints.toldAboutFirstSave = true;
        writeLibraryHints(storage, hints);
        toast.show({
          message: 'Saved to your library — every document you convert is kept here.',
          timeoutMs: 8000,
          action: { label: 'Open library', onSelect: () => sheets.open('library') },
        });
      },
      onWriteFailure(failure) {
        if (failure.kind === 'quota') {
          void proposePrune();
          return;
        }
        toast.show({
          message: 'That document could not be added to your library.',
          tone: 'alert',
          timeoutMs: 0,
          action: { label: 'Export library', onSelect: () => void exportLibrary() },
        });
      },
      onSession(session) {
        // Only an *open* entry is worth continuing after a reload. A sealed one is not
        // being written to, and is found again below by what it holds rather than by
        // an id that would invite the next keystroke straight back into it (§6.10).
        const open = session.sealed ? null : session.entryId;
        writeCurrentEntryId(tab, open);
        // And the other tabs are told, for the same reason and on the same terms: an
        // entry being written into is claimed, a sealed one is not being written to
        // and so is nobody's to lose.
        if (open !== null) channel?.post({ kind: 'claim', id: open });
        syncEntry();
      },
      onChange() {
        void refreshLibrary();
        channel?.post({ kind: 'changed' });
      },
    });

    syncEntry();
    // A reload is not a new document: if the editing session was writing into an
    // entry and the pane came back with that same session's text in it, the log
    // continues there rather than growing a second copy (§6.10).
    const previous = readCurrentEntryId(tab);
    const resumed =
      loaded.source === 'storage' && previous !== null ? await library.get(previous) : null;
    if (resumed) {
      library.adopt(resumed.id);
    } else {
      writeCurrentEntryId(tab, null);
      // A reload after Save has no id to resume — a sealed entry keeps none — but it
      // does have the document, and a document already in the log *verbatim* does not
      // deserve a second copy of it. Coming back to it sealed is also the conservative
      // half of the guess: the worst it costs is one extra entry when editing resumes,
      // where adopting it outright would let the next keystroke overwrite the version
      // the user asked to keep.
      const kept =
        loaded.source === 'storage' && state.doc.trim() !== ''
          ? ((await library.list()).find((entry) => entry.source === state.doc) ?? null)
          : null;
      if (kept) library.adoptSealed(kept);
    }
    // Anything converted while the database was opening still belongs in the log.
    recordCurrent();
    await refreshLibrary();
  }

  /**
   * Another tab has taken over the entry this one was writing into (§6.10).
   *
   * Giving it up is not a courtesy, it is the point: two tabs updating one entry in
   * place put the whole record each time, so the second write silently replaces the
   * first tab's document, its title and its star. Whatever is pending here was typed
   * here and is written into the entry it was typed into; what comes next starts an
   * entry of its own.
   *
   * Said out loud, briefly, because the user did nothing to cause it and the header
   * is about to stop naming the entry they were saving into.
   */
  function entryTakenOver(id: string): void {
    // A library this tab has already let go of has nothing to hand over, and the
    // persistent toast `libraryClosed` raised has already said so once.
    if (!library || libraryStale) return;
    const session = library.session;
    // Not this tab's entry, or this tab is only holding a sealed version of it —
    // which is nobody's to lose, since nothing is being written to it.
    if (session.sealed || session.entryId !== id) return;
    const name = shownEntry?.id === id ? shownEntry.title : null;
    library.release();
    toast.show({
      message:
        name === null
          ? 'Another tab is now saving into this entry. What you type here starts a new one.'
          : `Another tab is now saving into “${name}”. What you type here starts a new entry.`,
      timeoutMs: 8000,
    });
  }

  /**
   * The database was given up while the page is still running (§6.10).
   *
   * One cause: another tab is upgrading it, and this one was in the way. Holding on
   * would have left *that* tab with no library at all, so `library-store.ts` let go —
   * which means every call from here on fails. Logging stops on purpose rather than
   * by a stream of failed writes, and the offer is the only one that helps: reload,
   * and come back on the new schema.
   */
  function libraryClosed(): void {
    if (libraryStale) return;
    libraryStale = true;
    library?.pause();
    notes.library = LIBRARY_STALE;
    updateNote();
    // Not a refresh: the database is closed, so a read would answer "no entries" and
    // the pane would report an empty library rather than an unreachable one.
    libraryPane.setUnavailable(
      'Another tab updated the library. Reload this tab to use it here — nothing was lost.',
    );
    panes.setEntryState(null);
    toast.show({
      message: 'The library was updated in another tab.',
      timeoutMs: 0,
      action: { label: 'Reload', onSelect: () => window.location.reload() },
    });
  }

  /** Note the document as it stands, if there is a library and an answer to note. */
  function recordCurrent(): void {
    if (!library || state.doc.trim() === '') return;
    library.record(state.doc, state.opts, makePreview(panes.getOutput() || lastGoodOutput));
  }

  /**
   * Re-read the library and tell the pane where the user stands — the count, the
   * size, and the one line of warning that is ever shown about storage (§6.10).
   */
  async function refreshLibrary(): Promise<void> {
    // A closed database answers every read with nothing, and a pane that reported
    // that would be saying the library is empty when it is only out of reach.
    if (!library || libraryStale) return;
    const entries = await library.list();
    libraryPane.setEntries(entries, statsOf(entries));
    const current = library.session.entryId;
    shownEntry = entries.find((entry) => entry.id === current) ?? null;
    syncEntry();
    libraryPane.setNotice(await storageNotice());
  }

  /**
   * The entry the header is naming, so that naming it costs no read. It is whatever
   * the session points at; `null` means the next conversion will make one.
   */
  let shownEntry: LibraryEntry | null = null;

  /**
   * Say, in the input pane's header, which entry the keystrokes are going into
   * (§6.10).
   *
   * This is the whole answer to item 8's first complaint. The library was silent about
   * the current entry, so nothing on screen contradicted the assumption that a paste
   * over a document starts a fresh one — and no heuristic underneath can fix a silence.
   */
  function syncEntry(): void {
    if (!library?.available) {
      panes.setEntryState(null);
      return;
    }
    const session = library.session;
    if (shownEntry !== null && shownEntry.id !== session.entryId) shownEntry = null;
    const words = describeSession(session, shownEntry?.title ?? null);
    panes.setEntryState({
      id: session.entryId,
      label: words.label,
      hint: words.hint,
      sealed: session.sealed,
      starred: shownEntry?.starred === true,
    });
    // An entry the header does not know yet — one just adopted, or just sealed. The
    // pane is already correct about the *state*; this fills in the name.
    if (session.entryId === null || shownEntry !== null) return;
    const wanted = session.entryId;
    void library.get(wanted).then((entry) => {
      if (!entry || library?.session.entryId !== wanted) return;
      shownEntry = entry;
      syncEntry();
    });
  }

  /**
   * What one input event did to the session, said out loud (§6.10).
   *
   * The automatic fork is the safety net under New, Save and ★, and it is the one that
   * can be wrong — so it is also the one that comes with an Undo. Getting it wrong then
   * costs a click; not having it at all costs the document.
   */
  function noteEdit(before: string, after: string): void {
    const outcome = library?.noteEdit(before, after) ?? { kind: 'none' as const };
    if (outcome.kind === 'none') return;
    syncEntry();
    if (outcome.kind !== 'forked') return;
    const from = outcome.from;
    void (async () => {
      const left = await library?.get(from);
      toast.show({
        message: left
          ? `That replaced the document, so this is a new entry. “${left.title}” is still in your library.`
          : 'That replaced the document, so this is a new entry.',
        timeoutMs: 8000,
        action: {
          label: 'Undo',
          onSelect: () => {
            void (async () => {
              await library?.mergeBack(from);
              await refreshLibrary();
            })();
          },
        },
      });
    })();
  }

  /**
   * A browser quota that small, with no promise to keep the data, is a private window
   * or something very like one. It is a guess, so it is worded as one.
   */
  const EPHEMERAL_QUOTA = 120 * 1024 * 1024;

  async function storageNotice(): Promise<{ tone: 'info' | 'warn'; message: string } | null> {
    if (library?.paused === true) {
      return {
        tone: 'warn',
        message:
          'New documents are no longer being logged, at your request. Everything already here is untouched — delete what you no longer need, and the log resumes next time you open the app.',
      };
    }
    const estimate = await storageEstimate();
    if (quotaPressure(estimate) === 'tight') {
      return {
        tone: 'warn',
        message:
          'Storage for this site is nearly full. Export your library now and nothing has to be lost later.',
      };
    }
    if (!(await isPersisted()) && estimate !== null && estimate.quota < EPHEMERAL_QUOTA) {
      return {
        tone: 'info',
        message:
          '⚠️ This browsing session will probably not keep these — a private window usually forgets them. Export the library if you want any of it to last.',
      };
    }
    return null;
  }

  /* --------------------------------------------------- what a full disk may do */

  /** One proposal per session: a dialog per failed write would be a tantrum. */
  let pruneProposed = false;

  /**
   * Storage ran out. The app *proposes*; it does not act (§6.10).
   *
   * Export comes first and prominently, so nothing has to be lost at all. Only the
   * oldest unstarred entries are ever offered, only with the user's explicit
   * agreement in the dialog that named them, and a refusal is a complete answer —
   * the library stops growing and says so. A library that has stopped growing is a
   * nuisance; a library that ate the user's work is a betrayal.
   */
  async function proposePrune(): Promise<void> {
    if (!library || pruneProposed) return;
    pruneProposed = true;
    const entries = await library.list();
    const proposal = pruneProposal(entries, PRUNE_PROPOSAL_SIZE);
    const answer = await libraryPane.askPrune(proposal);

    if (answer === 'remove' && proposal.entries.length > 0) {
      const ids = proposal.entries.map((entry) => entry.id);
      if (await library.removeMany(ids)) {
        toast.show({ message: `Removed ${ids.length} old entries. The library is logging again.` });
        // The disk has room again, so a later squeeze deserves a fresh proposal.
        pruneProposed = false;
      }
      await refreshLibrary();
      return;
    }

    library.pause();
    notes.library = LIBRARY_PAUSED;
    updateNote();
    toast.show({
      message: 'Nothing was removed. New documents will not be logged for the rest of this session.',
      timeoutMs: 8000,
      action: { label: 'Export library', onSelect: () => void exportLibrary() },
    });
    await refreshLibrary();
  }

  /* ------------------------------------------------------- the library's actions */

  /** Opening an entry restores its document *and* the options it was converted under. */
  function openEntry(entry: LibraryEntry): void {
    const previousDoc = state.doc;
    const previousOpts = { ...state.opts };
    const previousSession = library?.session ?? { entryId: null, sealed: false };
    const previousEntry = shownEntry;

    // Sealed unless this *is* the entry being written to (§6.10): a version the user
    // moved on from comes back to be read and copied from, and the first edit to it
    // starts a new entry instead of overwriting what they kept.
    if (adoptionOnOpen(previousSession, entry.id) === 'open') library?.adopt(entry.id);
    else library?.adoptSealed(entry);
    applyOptions(withDefaults(entry.options));
    setDocument(entry.source, false);
    sheets.close();
    void refreshLibrary();

    toast.show({
      message: `Opened “${entry.title}”.`,
      action: {
        label: 'Undo',
        onSelect: () => {
          // The settings that were replaced belong to an entry that is itself in the
          // log, so this is a convenience rather than a rescue — but it is the app's
          // idiom, and a replaced document deserves one level of undo (§6.7). The
          // session comes back as it was, sealed included: a version the user had
          // kept must not start absorbing edits because they looked at another entry.
          if (previousSession.sealed && previousEntry !== null) {
            library?.adoptSealed(previousEntry);
          } else if (previousSession.entryId !== null && !previousSession.sealed) {
            library?.adopt(previousSession.entryId);
          } else {
            library?.beginNewEntry();
          }
          applyOptions(previousOpts);
          setDocument(previousDoc, false);
          void refreshLibrary();
        },
      },
    });
  }

  /** Put a whole option set in force — the controls, the pane and the worker. */
  function applyOptions(next: AppOptions): void {
    state.opts = withDefaults(sanitizeOptions(next));
    controls.setOptions(state.opts);
    panes.setSoftWrap(softWraps(state.opts));
    prepareMathJax();
    persistence.options(state.opts);
  }

  async function deleteEntry(entry: LibraryEntry): Promise<void> {
    const removed = await library?.remove(entry.id);
    await refreshLibrary();
    if (!removed) {
      toast.show({ message: 'That entry could not be removed.', tone: 'alert' });
      return;
    }
    toast.show({
      message: `Deleted “${removed.title}”.`,
      action: {
        label: 'Undo',
        onSelect: () => {
          void (async () => {
            await library?.restore(removed);
            await refreshLibrary();
          })();
        },
      },
    });
  }

  /* ------------------------------------------------------- export and import */

  async function exportLibrary(): Promise<void> {
    const entries = (await library?.list()) ?? [];
    const now = new Date();
    const text = encodeLibrary(entries, { exportedAt: now, techxt: client.version ?? '' });
    const name = libraryFileName(now);
    downloadText(text, name, 'application/json');
    toast.show({
      message: `Saved ${name} — ${entries.length} ${entries.length === 1 ? 'entry' : 'entries'}.`,
    });
  }

  /**
   * A file the user chose, treated as what it is: something that arrived from
   * elsewhere. It is decoded before anything is asked, so the question the dialog
   * puts is about a file that has already been shown to be readable.
   */
  async function importLibraryFile(file: File): Promise<void> {
    if (!library) return;
    let text: string;
    try {
      text = await file.text();
    } catch {
      toast.show({ message: 'That file could not be read.', tone: 'alert' });
      return;
    }

    const decoded = decodeLibrary(text, Date.now());
    if (!decoded.ok) {
      toast.show({ message: decoded.reason, tone: 'alert', timeoutMs: 8000 });
      return;
    }

    const existing = await library.list();
    const stats = statsOf(existing);
    const choice = await libraryPane.askImport({
      incoming: decoded.library.entries.length,
      exportedAt: decoded.library.exportedAt,
      dropped: decoded.library.dropped,
      existing: { count: stats.count, starred: stats.starred },
    });
    if (!choice) return;

    const plan = planImport(existing, decoded.library.entries, choice);
    if (!(await library.apply(plan.put, plan.remove))) {
      toast.show({
        message: 'That import could not be written — nothing in your library was changed.',
        tone: 'alert',
        timeoutMs: 0,
        action: { label: 'Export library', onSelect: () => void exportLibrary() },
      });
      await refreshLibrary();
      return;
    }

    // Replace took the current entry with everything else; what is being typed now
    // deserves an entry of its own rather than a stale id.
    if (plan.mode === 'replace') library.beginNewEntry();
    await refreshLibrary();
    toast.show({ message: describeImport(plan, decoded.library.dropped), timeoutMs: 6000 });
  }

  /** The first time the pane is opened it is no longer news (§6.10). */
  function openedLibrary(): void {
    controls.setLibraryNew(false);
    if (!hints.opened) {
      hints.opened = true;
      writeLibraryHints(storage, hints);
    }
    void refreshLibrary();
  }

  /* --------------------------------------------- New, Save and ★: three verbs */

  /**
   * The three buttons differ only in what happens after the seal (§6.10): **New**
   * clears the input, **Save** leaves the document on screen, and **★** stars it as
   * well. Sealing itself is one primitive in `library.ts`, and none of them creates
   * the next entry — the first edit does.
   */
  function currentPreview(): string {
    return makePreview(panes.getOutput() || lastGoodOutput);
  }

  /** **New**: keep what is there, and start with an empty document. */
  async function newDocument(): Promise<void> {
    if (state.doc === '') {
      toast.show({ message: 'The document is already empty.' });
      return;
    }
    const previous = state.doc;
    const kept = (await library?.seal(previous, state.opts, currentPreview())) ?? null;
    setDocument('');
    await refreshLibrary();
    toast.show({
      // Without a library there is nothing to have kept, and a toast that says there
      // is would be the app lying about the one thing this item is about.
      message: kept ? `Kept “${kept.title}” in your library. This is a new document.` : 'A new document.',
      action: {
        label: 'Undo',
        onSelect: () => {
          // The app's single-level undo, as Load ▾ offers it (§6.7): the document
          // comes back, and so does the entry it was being written into.
          if (kept) library?.adopt(kept.id);
          setDocument(previous, false);
          void refreshLibrary();
        },
      },
    });
  }

  /**
   * **Save**: seal this version and leave it on screen.
   *
   * The name is slightly a lie and the tooltip carries the truth, because everything
   * is already saved: what the button really does is stop *this* version changing.
   */
  async function saveVersion(): Promise<void> {
    if (!library) return;
    const entry = await library.seal(state.doc, state.opts, currentPreview());
    await refreshLibrary();
    if (!entry) {
      toast.show({
        message:
          state.doc.trim() === ''
            ? 'There is nothing to keep yet.'
            : 'That document could not be added to your library.',
        tone: state.doc.trim() === '' ? 'status' : 'alert',
      });
      return;
    }
    toast.show({
      message: `Kept “${entry.title}” — further edits start a new entry.`,
      action: { label: 'Open library', onSelect: () => sheets.open('library') },
    });
  }

  /** **★**: seal it and star it — or, on an entry already sealed, just the star. */
  async function toggleStar(): Promise<void> {
    if (!library) return;
    const result = await library.starCurrent(state.doc, state.opts, currentPreview());
    if (!result) {
      toast.show({
        message:
          state.doc.trim() === ''
            ? 'There is nothing to keep yet.'
            : 'That document could not be added to your library.',
        tone: state.doc.trim() === '' ? 'status' : 'alert',
      });
      return;
    }
    await refreshLibrary();
    toast.show({
      message: result.starred
        ? `Starred “${result.entry.title}” — it is kept, and nothing automatic will ever remove it.`
        : `Removed the star from “${result.entry.title}”.`,
      action: { label: 'Open library', onSelect: () => sheets.open('library') },
    });
  }

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

  /**
   * Replace the document. Every caller of this is a point where the app already knows
   * the user has moved on to something else — Load ▾, the file handler, opening a
   * library entry, an Undo — which is exactly where a new library entry begins
   * (§6.10). Opening an entry from the library is the one exception, and passes
   * `false`: it has just settled the session itself — writing into that entry when it
   * is the one already open, sealed on to it otherwise — and this must not overwrite
   * that answer with a third.
   */
  function setDocument(text: string, startNewEntry = true): void {
    state.doc = text;
    if (startNewEntry) library?.beginNewEntry();
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
    // Best effort, and honest about it: a page being torn down may not get its
    // IndexedDB transaction finished. The 2 s debounce is what makes that a rare
    // couple of seconds' typing rather than a session (§6.10).
    void library?.flush();
  };
  window.addEventListener('pagehide', (event) => {
    flush();
    // `persisted` is the whole of the condition: a page going into the back/forward
    // cache is coming back, with the entry it was writing into and every reason to
    // still be hearing the other tabs. Only a page actually going away lets go.
    if (event.persisted) return;
    channel?.close();
    channel = null;
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') flush();
  });

  // The pane got `ui` at init, so the split, the focus and the size are already its
  // own; the document is the one piece `PanesInit` does not carry.
  registerFileHandler();

  panes.setDocument(state.doc);
  panes.setSoftWrap(softWraps(state.opts));
  // The mode may already be in force — a stored setting, a share link — in which case
  // the typesetter is wanted now rather than when the first result lands.
  prepareMathJax();
  prefetchMathJaxWhenInstalled();
  measuredColumns = panes.columns();
  // The font is applied without being awaited: the first conversion should not wait
  // for a face to arrive, and the pane re-measures when it does (§6.5).
  void useFont(state.ui.font, state.ui.size, false);
  if (state.ui.keepFontsOffline) void preloadAllFonts(state.ui.size);
  requestConversion('immediate');
  // Last, and unawaited: the library is the one part of the app that may take a
  // moment to become available, and nothing above it waits for that (§6.10).
  void openLibrary();
}

void start();
