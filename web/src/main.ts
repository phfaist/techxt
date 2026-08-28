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
  createLibrary,
  makePreview,
  pruneProposal,
  quotaPressure,
  statsOf,
} from './library';
import type { Library, LibraryEntry } from './library';
import { decodeLibrary, describeImport, encodeLibrary, libraryFileName, planImport } from './library-io';
import { isPersisted, openLibraryBackend, requestPersistence, storageEstimate } from './library-store';
import {
  DEFAULT_OPTIONS,
  SHARE_LENGTH_LIMIT,
  browserStorage,
  createPersistence,
  encodeShare,
  encodeShareSettingsOnly,
  loadState,
  readCurrentEntryId,
  readLibraryHints,
  resolveOptions,
  sanitizeOptions,
  shouldPulseLibrary,
  softWraps,
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
      // The library is a log of what was converted, so a conversion is exactly when
      // it hears about a document. The write itself is debounced (§6.10).
      library?.record(state.doc, state.opts, makePreview(result.text));
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
    onStar() {
      void toggleStar();
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
    libraryNew: shouldPulseLibrary(hints),
    // The version only exists once the worker has answered `ready`, and the controls
    // are built before that so the page is usable while wasm loads. `about.setVersion`
    // fills the About section in when it arrives.
    version: client.version ?? '',
    onOptionsChange(next: AppOptions) {
      state.opts = withDefaults(sanitizeOptions(next));
      persistence.options(state.opts);
      // The display half of `wrap: 'soft'` (§6.3): the library is told nothing about
      // it — `resolveOptions` drops it — so the pane has to be told directly.
      panes.setSoftWrap(softWraps(state.opts));
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
    const backend = await openLibraryBackend();
    if (!backend) {
      // Honest and inert, the way `browserStorage()` is for localStorage (§6.10).
      panes.setStarred(null);
      libraryPane.setUnavailable(
        'This browser will not let the page keep a library here. Everything else works as usual.',
      );
      return;
    }

    library = createLibrary({
      backend,
      persist: requestPersistence,
      onWrite(entry, created) {
        if (entry.id === library?.currentId) panes.setStarred(entry.starred);
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
      onSession(currentId) {
        writeCurrentEntryId(storage, currentId);
      },
      onChange() {
        void refreshLibrary();
      },
    });

    panes.setStarred(false);
    // A reload is not a new document: if the editing session was writing into an
    // entry and the pane came back with that same session's text in it, the log
    // continues there rather than growing a second copy (§6.10).
    const previous = readCurrentEntryId(storage);
    if (loaded.source === 'storage' && previous !== null && (await library.get(previous))) {
      library.adopt(previous);
    } else {
      writeCurrentEntryId(storage, null);
    }
    // Anything converted while the database was opening still belongs in the log.
    recordCurrent();
    await refreshLibrary();
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
    if (!library) return;
    const entries = await library.list();
    libraryPane.setEntries(entries, statsOf(entries));
    const current = library.currentId;
    panes.setStarred(entries.some((entry) => entry.id === current && entry.starred));
    libraryPane.setNotice(await storageNotice());
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
    const previousId = library?.currentId ?? null;

    library?.adopt(entry.id);
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
          // idiom, and a replaced document deserves one level of undo (§6.7).
          if (previousId !== null) library?.adopt(previousId);
          else library?.beginNewEntry();
          applyOptions(previousOpts);
          setDocument(previousDoc, previousId === null);
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

  /** ⭐ Save, from the output pane: star this session's entry, creating one if needed. */
  async function toggleStar(): Promise<void> {
    if (!library) return;
    const result = await library.starCurrent(
      state.doc,
      state.opts,
      makePreview(panes.getOutput() || lastGoodOutput),
    );
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
    panes.setStarred(result.starred);
    await refreshLibrary();
    toast.show({
      message: result.starred
        ? `Starred “${result.entry.title}” — it stays in your library.`
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
   * `false`: the entry it just adopted *is* where those keystrokes belong.
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
  window.addEventListener('pagehide', flush);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') flush();
  });

  // The pane got `ui` at init, so the split, the focus and the size are already its
  // own; the document is the one piece `PanesInit` does not carry.
  registerFileHandler();

  panes.setDocument(state.doc);
  panes.setSoftWrap(softWraps(state.opts));
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
