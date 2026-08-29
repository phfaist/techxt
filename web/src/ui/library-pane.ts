/**
 * The library pane (web/PLAN.md §6.10, §6.11).
 *
 * A `<dialog>` sheet over the tool, like About and Install: the page never scrolls, so
 * a scrolling list of entries belongs inside a dialog, and the sheet machinery in
 * `sheets.ts` already gives Escape, the backdrop, focus handling and inertness for
 * free. The shell — the dialog, its title and its close button — is in `index.html`
 * with the app's other sheets; everything inside `#library-mount` is built here.
 *
 * Like every other module under `ui/`, this one knows nothing about storage. It is
 * given entries and it reports what the user did with them; whether that reaches
 * IndexedDB, and what happens if it does not, is `main.ts`'s problem.
 *
 * Its three dialogs — import, the two-step Clear, and the storage-full proposal — are
 * built here rather than in the page because each one has to *name* things it can only
 * know at the moment it opens: how many entries an import would replace, how many of
 * them are starred, which dates a proposal would give up. A dialog that cannot say
 * what it is about is exactly the dialog this app must not show.
 *
 * Every node is made with `createElement` and `textContent`. Entry titles and previews
 * are the user's own text and an imported file's text; neither goes near `innerHTML`.
 */

import { describeOptions, filterEntries, formatBytes } from '../library';
import type { LibraryEntry, LibraryStats, PruneProposal } from '../library';
import type {
  ImportRequest,
  LibraryNotice,
  LibraryPane,
  LibraryPaneInit,
  PruneAnswer,
} from './api';
import type { ImportChoice } from '../library-io';

/* --------------------------------------------------------------- small helpers */

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  content?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (content !== undefined) node.textContent = content;
  return node;
}

function button(className: string, text: string, title?: string): HTMLButtonElement {
  const node = el('button', className, text);
  node.type = 'button';
  if (title) node.title = title;
  return node;
}

/** A date a person recognises: the time today, the date this year, otherwise both. */
function when(time: number, now = Date.now()): string {
  const date = new Date(time);
  if (!Number.isFinite(date.getTime())) return '—';
  const sameDay = new Date(now).toDateString() === date.toDateString();
  if (sameDay) {
    return `Today, ${date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}`;
  }
  const sameYear = new Date(now).getFullYear() === date.getFullYear();
  return date.toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
}

/** "142 entries · 8 starred · 3.1 MB" — where the user stands, unasked (§6.10). */
function statsLine(stats: LibraryStats): string {
  const entries = `${stats.count} ${stats.count === 1 ? 'entry' : 'entries'}`;
  return `${entries} · ${stats.starred} starred · ${formatBytes(stats.bytes)}`;
}

/* -------------------------------------------------------------- the dialogs */

/**
 * A modal built here rather than in the page, because everything it says depends on
 * the moment it opens.
 *
 * It is appended to `<body>` on first use and reused after that. A `<dialog>` opened
 * over another `<dialog>` stacks in the top layer, so these work from inside the
 * library sheet without it having to close first — which matters: closing the sheet
 * to ask a question would lose the list the question is about.
 */
function makeDialog(className: string): HTMLDialogElement {
  const dialog = el('dialog', `sheet sheet-ask ${className}`);
  document.body.append(dialog);
  dialog.addEventListener('click', (event: MouseEvent) => {
    if (event.target === dialog) dialog.close();
  });
  return dialog;
}

/* ----------------------------------------------------------------- the module */

export function initLibraryPane(init: LibraryPaneInit): LibraryPane {
  const { mount } = init;
  mount.classList.add('library-mount');

  let entries: readonly LibraryEntry[] = [];
  let stats: LibraryStats = { count: 0, starred: 0, bytes: 0 };
  let selectedId: string | null = null;
  let query = '';
  let starredOnly = false;
  let unavailable: string | null = null;
  /** Which column a narrow screen is showing; ignored on a wide one. */
  let view: 'list' | 'detail' = 'list';

  /* ------------------------------------------------------------------- DOM */

  const root = el('div', 'library');

  /* --- header */

  const statsText = el('p', 'library-stats');
  const noticeBox = el('p', 'library-notice');
  noticeBox.hidden = true;

  const exportButton = button('btn', 'Export', 'Save the whole library as one file');
  exportButton.addEventListener('click', () => init.onExport());

  const importInput = el('input', 'sr-only');
  importInput.type = 'file';
  importInput.accept = 'application/json,.json';
  importInput.id = 'library-import-file';
  importInput.addEventListener('change', () => {
    const file = importInput.files?.[0];
    // The same file twice in a row still has to fire a change event.
    importInput.value = '';
    if (file) init.onImportFile(file);
  });
  const importButton = button('btn', 'Import', 'Read a library file into this one');
  importButton.addEventListener('click', () => importInput.click());

  /*
   * "Clear library" on a screen with room for it, "Clear" on a phone, where the word
   * is what pushes this row off the stats line beside it. One extra span and a media
   * query rather than a resize listener — and the hidden half is `display: none`, so
   * the accessible name narrows to "Clear" with it rather than saying one thing and
   * showing another.
   */
  const clearButton = button('btn btn-quiet library-clear', '');
  const clearLabel = el('span', 'library-clear-label', 'Clear');
  clearLabel.append(el('span', 'library-clear-word', ' library'));
  clearButton.append(clearLabel);
  clearButton.addEventListener('click', () => {
    void askClear();
  });

  const actions = el('div', 'library-actions');
  actions.append(exportButton, importButton, importInput, clearButton);

  const search = el('input', 'control library-search');
  search.type = 'search';
  search.id = 'library-search';
  search.placeholder = 'Search titles and source';
  search.setAttribute('aria-label', 'Search the library');
  search.addEventListener('input', () => {
    query = search.value;
    renderList();
  });

  const filterAll = button('btn library-filter is-on', 'All');
  const filterStarred = button('btn library-filter', '★ Starred');
  filterAll.setAttribute('aria-pressed', 'true');
  filterStarred.setAttribute('aria-pressed', 'false');
  filterAll.addEventListener('click', () => setStarredOnly(false));
  filterStarred.addEventListener('click', () => setStarredOnly(true));

  const filters = el('div', 'library-filters');
  const filterGroup = el('div', 'library-filter-group');
  filterGroup.setAttribute('role', 'group');
  filterGroup.setAttribute('aria-label', 'Which entries to show');
  filterGroup.append(filterAll, filterStarred);
  filters.append(search, filterGroup);

  const head = el('div', 'library-head');
  // Source order is the order it reads in: where the user stands, what can be done to
  // all of it, anything the app has to say about storage, then what narrows the list.
  // A wide screen puts the first two side by side with grid areas; a narrow one lays
  // this row out with flex, where source order *is* the order (§6.10).
  head.append(statsText, actions, noticeBox, filters);

  /* --- the list */

  const list = el('ol', 'library-list');
  list.setAttribute('aria-label', 'Saved documents');
  const empty = el('p', 'library-empty');

  const listPane = el('div', 'library-pane library-pane-list');
  listPane.append(list, empty);

  /* --- the detail */

  const backButton = button('btn library-back', '‹ All entries');
  backButton.addEventListener('click', () => {
    view = 'list';
    applyView();
    search.focus();
  });

  const detailTitle = el('h3', 'library-detail-title');
  const renameInput = el('input', 'control library-rename');
  renameInput.type = 'text';
  renameInput.id = 'library-rename';
  renameInput.hidden = true;
  renameInput.setAttribute('aria-label', 'Rename this entry');

  const detailDates = el('p', 'library-detail-dates');
  const detailOptions = el('p', 'library-detail-options');

  /**
   * The two things there are to read about an entry, in the order they are wanted:
   * the stored rendering first — it is a handful of lines and it is what the user
   * recognises the document by — and the whole `source` under it (§6.10).
   *
   * Neither scrolls. The detail column is one scroll container and these are laid out
   * inside it at their full height, because a pane that scrolls and holds two boxes
   * that also scroll is three answers to one flick of the wheel. The captions are two
   * words apiece for the same reason: what each block is, is obvious on sight, and a
   * sentence of label above a block of text is a sentence in the way.
   *
   * An entry that cannot be read is a filing cabinet with the drawers welded shut, and
   * nothing new has to be stored for it: an entry has always kept the whole `source`.
   * The stored `preview` is the *rendered* output and stays what it is — a few lines,
   * allowed to be stale, and never the document.
   */
  const previewCaption = el('p', 'library-caption', 'Preview');
  const detailPreview = el('pre', 'library-preview');

  const sourceCaption = el('p', 'library-caption', 'LaTeX source');
  const detailSource = el('pre', 'library-source');

  const openButton = button('btn btn-accent', 'Open');
  const starButton = button('btn', '☆ Star');
  starButton.title = 'Keep this one — starred entries are never removed automatically';
  const renameButton = button('btn', 'Rename');
  const copyButton = button('btn', 'Copy source');
  const downloadButton = button('btn', 'Download source');
  const deleteButton = button('btn library-delete', 'Delete');

  const detailActions = el('div', 'library-detail-actions');
  detailActions.append(
    openButton,
    starButton,
    renameButton,
    copyButton,
    downloadButton,
    deleteButton,
  );

  const detailBody = el('div', 'library-detail-body');
  // The actions come before the two reading regions rather than after them: on a phone
  // the source alone is a screenful, and Open would otherwise be a scroll away.
  detailBody.append(
    detailTitle,
    renameInput,
    detailDates,
    detailOptions,
    detailActions,
    previewCaption,
    detailPreview,
    sourceCaption,
    detailSource,
  );

  const detailEmpty = el('p', 'library-empty', 'Select an entry to see it.');

  const detailPane = el('div', 'library-pane library-pane-detail');
  detailPane.append(backButton, detailBody, detailEmpty);

  const body = el('div', 'library-body');
  body.append(listPane, detailPane);

  root.append(head, body);
  mount.append(root);

  /* -------------------------------------------------------------- behaviour */

  function selected(): LibraryEntry | null {
    return entries.find((entry) => entry.id === selectedId) ?? null;
  }

  function setStarredOnly(next: boolean): void {
    starredOnly = next;
    filterAll.classList.toggle('is-on', !next);
    filterStarred.classList.toggle('is-on', next);
    filterAll.setAttribute('aria-pressed', String(!next));
    filterStarred.setAttribute('aria-pressed', String(next));
    renderList();
  }

  function applyView(): void {
    root.dataset.view = view;
  }

  function card(entry: LibraryEntry): HTMLLIElement {
    const item = el('li', 'library-card');
    item.dataset.id = entry.id;
    if (entry.id === selectedId) item.classList.add('is-selected');

    const pick = button('library-card-button', '');
    pick.setAttribute('aria-label', `Open the details of ${entry.title}`);

    const line = el('span', 'library-card-line');
    if (entry.starred) {
      const star = el('span', 'library-card-star', '★');
      star.title = 'Starred';
      line.append(star);
    }
    line.append(el('span', 'library-card-title', entry.title));

    const meta = el('span', 'library-card-meta', when(entry.updatedAt));
    const preview = el('span', 'library-card-preview', firstPreviewLine(entry));

    pick.append(line, meta, preview);
    pick.addEventListener('click', () => {
      selectedId = entry.id;
      view = 'detail';
      applyView();
      renderList();
      renderDetail();
    });

    item.append(pick);
    return item;
  }

  /** One line of the preview for the card; the whole thing is in the detail. */
  function firstPreviewLine(entry: LibraryEntry): string {
    const line = entry.preview.split('\n').find((text) => text.trim() !== '') ?? '';
    return line.length > 90 ? `${line.slice(0, 90)}…` : line;
  }

  function renderList(): void {
    const shown = filterEntries(entries, query, starredOnly);
    list.replaceChildren(...shown.map(card));
    const nothing = shown.length === 0;
    list.hidden = nothing;
    empty.hidden = !nothing;
    empty.textContent = emptyMessage();
  }

  function emptyMessage(): string {
    if (unavailable !== null) return unavailable;
    if (entries.length === 0) {
      return 'Nothing here yet. Every document you convert is logged automatically.';
    }
    if (starredOnly && query.trim() === '') return 'No starred entries. The ★ button marks one.';
    return 'Nothing matches that search.';
  }

  /**
   * Which entry the detail column is currently showing, so a re-render provoked by
   * something *else* — a star, a rename, an import landing — does not throw away where
   * the reader had got to in a long source. Only a different entry scrolls back up.
   */
  let shownId: string | null = null;

  function renderDetail(): void {
    const entry = selected();
    detailBody.hidden = entry === null;
    detailEmpty.hidden = entry !== null;
    if (!entry) {
      shownId = null;
      return;
    }

    detailTitle.textContent = entry.title;
    detailDates.textContent = `Updated ${when(entry.updatedAt)} · first converted ${when(entry.createdAt)}`;
    detailOptions.textContent = describeOptions(entry.options);
    detailSource.textContent = entry.source;
    detailPreview.textContent = entry.preview === '' ? '(no preview was stored)' : entry.preview;
    // Back to the top: the column is reused for every entry, and a new document opened
    // scrolled to where the last one was read is a small lie about what it is.
    if (entry.id !== shownId) {
      shownId = entry.id;
      detailPane.scrollTop = 0;
    }
    starButton.textContent = entry.starred ? '★ Starred' : '☆ Star';
    starButton.title = entry.starred ? 'Remove the star' : 'Keep this one';
  }

  /** Whether the title is being edited right now; see {@link endRename}. */
  let renaming = false;

  /**
   * Finish a rename, committing it or abandoning it.
   *
   * The flag is not bookkeeping: hiding a focused input fires `blur`, and the blur
   * handler is the one that commits — so an Escape that merely hid the field would
   * commit the very edit it was abandoning. Whoever gets here first ends the rename;
   * the second call has nothing left to do.
   */
  function endRename(commit: boolean): void {
    if (!renaming) return;
    renaming = false;
    const entry = selected();
    renameInput.hidden = true;
    detailTitle.hidden = false;
    if (commit && entry && renameInput.value.trim() !== '') {
      init.onRename(entry, renameInput.value);
    }
  }

  openButton.addEventListener('click', () => {
    const entry = selected();
    if (entry) init.onOpenEntry(entry);
  });
  starButton.addEventListener('click', () => {
    const entry = selected();
    if (entry) init.onStar(entry, !entry.starred);
  });
  renameButton.addEventListener('click', () => {
    const entry = selected();
    if (!entry) return;
    renaming = true;
    renameInput.value = entry.title;
    renameInput.hidden = false;
    detailTitle.hidden = true;
    renameInput.focus();
    renameInput.select();
  });
  renameInput.addEventListener('keydown', (event: KeyboardEvent) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      endRename(true);
    } else if (event.key === 'Escape') {
      // Ours before the dialog's: Escape here abandons the rename, not the sheet.
      event.preventDefault();
      event.stopPropagation();
      endRename(false);
      renameButton.focus();
    }
  });
  renameInput.addEventListener('blur', () => endRename(true));
  copyButton.addEventListener('click', () => {
    const entry = selected();
    if (entry) init.onCopySource(entry);
  });
  downloadButton.addEventListener('click', () => {
    const entry = selected();
    if (entry) init.onDownloadSource(entry);
  });
  deleteButton.addEventListener('click', () => {
    const entry = selected();
    if (entry) init.onDelete(entry);
  });

  applyView();
  renderList();
  renderDetail();

  /* --------------------------------------------------------- the import dialog */

  let importDialog: HTMLDialogElement | null = null;

  function askImport(request: ImportRequest): Promise<ImportChoice | null> {
    const dialog = (importDialog ??= makeDialog('library-ask-import'));
    return new Promise((resolve) => {
      let answered: ImportChoice | null = null;

      const heading = el('h2', 'sheet-title', 'Import a library');
      const close = button('btn sheet-close', '✕');
      close.setAttribute('aria-label', 'Close');
      close.addEventListener('click', () => dialog.close());
      const headRow = el('div', 'sheet-head');
      headRow.append(heading, close);

      const summary = el('p', 'ask-lede');
      const items = `${request.incoming} ${request.incoming === 1 ? 'entry' : 'entries'}`;
      summary.textContent =
        request.exportedAt === null
          ? `That file holds ${items}.`
          : `That file holds ${items}, exported ${when(Date.parse(request.exportedAt))}.`;

      const dropped = request.dropped.malformed + request.dropped.oversize;
      const droppedLine = el('p', 'ask-note');
      droppedLine.hidden = dropped === 0;
      droppedLine.textContent = `${dropped} item${dropped === 1 ? '' : 's'} in the file could not be read and will not be imported.`;

      const choices = el('div', 'ask-choices');
      const addRadio = radio('library-import-mode', 'add', 'Add to my library', true);
      const replaceRadio = radio(
        'library-import-mode',
        'replace',
        'Replace my library',
        false,
      );
      const addNote = el('p', 'ask-note', 'Everything already here is kept. An entry from the file that shares an id with one of yours is given a new id rather than overwriting it.');
      const replaceNote = el('p', 'ask-note ask-warn');
      replaceNote.textContent = `Everything already here is removed first: ${request.existing.count} ${request.existing.count === 1 ? 'entry' : 'entries'}, ${request.existing.starred} of them starred.`;

      const skipWrap = el('label', 'ask-check');
      const skip = el('input');
      skip.type = 'checkbox';
      skip.id = 'library-import-skip';
      skipWrap.append(skip, el('span', 'check-label', 'Skip items I already have'));
      const skipNote = el('p', 'ask-note', 'Matched on the document and its options, not on the id. It skips the incoming copy; nothing already here is touched either way.');

      choices.append(addRadio.row, addNote, skipWrap, skipNote, replaceRadio.row, replaceNote);

      function syncSkip(): void {
        // Replace leaves nothing to have already, so the checkbox has no meaning.
        skip.disabled = replaceRadio.input.checked;
        skipWrap.classList.toggle('is-disabled', skip.disabled);
        skipNote.hidden = skip.disabled;
      }
      addRadio.input.addEventListener('change', syncSkip);
      replaceRadio.input.addEventListener('change', syncSkip);
      syncSkip();

      const cancel = button('btn', 'Cancel');
      cancel.addEventListener('click', () => dialog.close());
      const confirm = button('btn btn-accent', 'Import');
      confirm.addEventListener('click', () => {
        void (async () => {
          const mode = replaceRadio.input.checked ? 'replace' : 'add';
          if (mode === 'replace') {
            const sure = await askConfirm({
              title: 'Replace the whole library?',
              lede: `This removes ${request.existing.count} ${request.existing.count === 1 ? 'entry' : 'entries'}, ${request.existing.starred} of them starred, and puts the ${request.incoming} from the file in their place.`,
              note: 'Export the library first if you want to keep it. This cannot be undone.',
              confirmLabel: 'Replace the library',
              typed: 'replace',
              danger: true,
            });
            if (!sure) return;
          }
          answered = { mode, skipExisting: mode === 'add' && skip.checked };
          dialog.close();
        })();
      });
      const foot = el('div', 'ask-foot');
      foot.append(cancel, confirm);

      const bodyBox = el('div', 'sheet-body ask-body');
      bodyBox.append(summary, droppedLine, choices, foot);

      dialog.replaceChildren(headRow, bodyBox);
      dialog.addEventListener(
        'close',
        () => {
          resolve(answered);
        },
        { once: true },
      );
      dialog.showModal();
    });
  }

  function radio(
    name: string,
    value: string,
    label: string,
    checked: boolean,
  ): { row: HTMLLabelElement; input: HTMLInputElement } {
    const row = el('label', 'ask-radio');
    const input = el('input');
    input.type = 'radio';
    input.name = name;
    input.value = value;
    input.checked = checked;
    row.append(input, el('span', 'check-label', label));
    return { row, input };
  }

  /* ------------------------------------------------------- the confirm dialog */

  let confirmDialog: HTMLDialogElement | null = null;

  interface ConfirmRequest {
    title: string;
    lede: string;
    note?: string;
    confirmLabel: string;
    /** A word the user has to type. Nothing this asks for is undoable. */
    typed?: string;
    danger?: boolean;
    /** An extra action offered before the destructive one — always Export. */
    escape?: { label: string; onSelect(): void };
  }

  function askConfirm(request: ConfirmRequest): Promise<boolean> {
    const dialog = (confirmDialog ??= makeDialog('library-ask-confirm'));
    return new Promise((resolve) => {
      let agreed = false;

      const heading = el('h2', 'sheet-title', request.title);
      const close = button('btn sheet-close', '✕');
      close.setAttribute('aria-label', 'Close');
      close.addEventListener('click', () => dialog.close());
      const headRow = el('div', 'sheet-head');
      headRow.append(heading, close);

      const lede = el('p', 'ask-lede', request.lede);
      const note = el('p', 'ask-note ask-warn', request.note ?? '');
      note.hidden = request.note === undefined;

      const confirm = button(
        `btn ${request.danger === true ? 'btn-danger' : 'btn-accent'}`,
        request.confirmLabel,
      );
      const typedField = el('div', 'ask-typed');
      typedField.hidden = request.typed === undefined;
      if (request.typed !== undefined) {
        const word = request.typed;
        const label = el('label', 'ask-typed-label', `Type ${word} to confirm`);
        const input = el('input', 'control');
        input.type = 'text';
        input.id = 'library-confirm-word';
        input.autocomplete = 'off';
        label.htmlFor = input.id;
        confirm.disabled = true;
        input.addEventListener('input', () => {
          confirm.disabled = input.value.trim().toLowerCase() !== word;
        });
        typedField.append(label, input);
      }

      const cancel = button('btn', 'Cancel');
      cancel.addEventListener('click', () => dialog.close());
      confirm.addEventListener('click', () => {
        agreed = true;
        dialog.close();
      });

      const foot = el('div', 'ask-foot');
      if (request.escape) {
        // Offered first and prominently: nothing has to be lost at all (§6.10).
        const escape = request.escape;
        const rescue = button('btn btn-accent ask-escape', escape.label);
        rescue.addEventListener('click', () => {
          escape.onSelect();
          rescue.textContent = `${escape.label} ✓`;
        });
        foot.append(rescue);
      }
      foot.append(cancel, confirm);

      const bodyBox = el('div', 'sheet-body ask-body');
      bodyBox.append(lede, note, typedField, foot);

      dialog.replaceChildren(headRow, bodyBox);
      dialog.addEventListener(
        'close',
        () => {
          resolve(agreed);
        },
        { once: true },
      );
      dialog.showModal();
      if (request.typed === undefined) confirm.focus();
    });
  }

  async function askClear(): Promise<void> {
    if (entries.length === 0) return;
    const agreed = await askConfirm({
      title: 'Clear the library?',
      lede: `This removes all ${stats.count} ${stats.count === 1 ? 'entry' : 'entries'}, including the ${stats.starred} you starred.`,
      note: 'Export the library first if you want to keep any of it. This cannot be undone.',
      confirmLabel: 'Delete everything',
      typed: 'delete',
      danger: true,
      escape: { label: 'Export library', onSelect: () => init.onExport() },
    });
    if (agreed) init.onClear();
  }

  /* -------------------------------------------------- the storage-full proposal */

  function askPrune(proposal: PruneProposal): Promise<PruneAnswer> {
    const count = proposal.entries.length;
    const range =
      count === 0
        ? ''
        : ` They were last converted between ${when(proposal.from)} and ${when(proposal.to)}.`;
    const lede =
      count === 0
        ? 'Storage for this site is full and the library cannot grow. Every entry you have is starred, so there is nothing here the app is willing to remove.'
        : `Storage for this site is full and the library cannot grow. The app can remove the ${count} oldest unstarred ${count === 1 ? 'entry' : 'entries'}, keeping ${proposal.keeping} — including all ${proposal.keepingStarred} starred.${range}`;

    return askConfirm({
      title: 'Storage is full',
      lede,
      note:
        count === 0
          ? 'Export the library, then delete what you no longer need from the list.'
          : 'Export the library first and nothing has to be lost at all. If you decline, the app stops logging new documents and leaves everything you have alone.',
      confirmLabel: count === 0 ? 'Close' : `Remove those ${count}`,
      danger: count > 0,
      escape: { label: 'Export library', onSelect: () => init.onExport() },
    }).then((agreed) => (count > 0 && agreed ? 'remove' : 'decline'));
  }

  /* ------------------------------------------------------------- the handle */

  return {
    setEntries(next, nextStats) {
      entries = next;
      stats = nextStats;
      if (selectedId !== null && !next.some((entry) => entry.id === selectedId)) {
        selectedId = null;
        view = 'list';
        applyView();
      }
      statsText.textContent = statsLine(nextStats);
      renderList();
      renderDetail();
    },

    setNotice(notice: LibraryNotice | null) {
      noticeBox.hidden = notice === null;
      noticeBox.textContent = notice?.message ?? '';
      noticeBox.dataset.tone = notice?.tone ?? 'info';
    },

    setUnavailable(reason) {
      unavailable = reason;
      root.dataset.unavailable = 'true';
      for (const control of [exportButton, importButton, clearButton, search, filterAll, filterStarred]) {
        control.disabled = true;
      }
      statsText.textContent = reason;
      renderList();
    },

    select(id) {
      selectedId = id;
      view = id === null ? 'list' : 'detail';
      applyView();
      renderList();
      renderDetail();
    },

    focusSearch() {
      if (unavailable === null) search.focus();
    },

    askImport,
    askPrune,
  };
}
