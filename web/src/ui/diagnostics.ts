/**
 * The status strip and the diagnostics panel (web/PLAN.md §7, §6.2, §6.9).
 *
 * This is the module that distinguishes techxt from a macro stripper. The library
 * does not merely drop what it cannot render: it says what it dropped, where, and
 * why, and it renders that report itself. So the panel's job is to carry all of that
 * through without editing it —
 *
 * - the counts, coloured by severity **and** marked by letter, because colour alone
 *   is not a distinction everyone can make;
 * - `identifier` in monospace, because it is a stable name a bug report can quote;
 * - `line:column`, and a jump into the textarea for the spans that point at the
 *   document the user typed — including a diagnostic raised inside a macro expansion,
 *   whose position is the *call* and is labelled `via macro` rather than passed off as
 *   the message's own place (§4.5); a row with no position at all says so instead of
 *   misleading;
 * - and, behind a per-row expander, techy's own `rendered` text with its caret line
 *   and trace frames: the same thing `techxt-cli` prints, so a screenshot from this
 *   app is directly comparable to a terminal report.
 *
 * Collapsed, the strip is a one-line summary. The live region announces *counts*, and
 * only when they change, so typing does not turn a screen reader into a metronome.
 */

import type { BusyState, DiagnosticsInit, DiagnosticsPanel } from './api';
import type { ConversionResult, Diagnostic, Severity } from '../worker/protocol';

const SEVERITIES: readonly Severity[] = ['error', 'warning', 'note'];

/** The letter each severity chip carries, so hue is never the only signal (§6.9). */
const MARK: Record<Severity, string> = { error: 'E', warning: 'W', note: 'N' };

const PLURAL: Record<Severity, [string, string]> = {
  error: ['error', 'errors'],
  warning: ['warning', 'warnings'],
  note: ['note', 'notes'],
};

/**
 * Why a row cannot be clicked (§4.5). The binding already substitutes the macro call
 * for a diagnostic raised inside an expansion, so what is left here is the residue:
 * something with no position in the document at all.
 */
const NO_SPAN_REASON = 'this arose inside an expansion with no call in your document to point at';

/** Marks a position that is the macro call rather than the message's own place. */
const APPROX_MARK = 'via macro';

/** The same fact, in a sentence, above techy's own report. */
const APPROX_REASON =
  'The position above is where the macro is used; the report below is positioned inside its expansion.';

const ANNOUNCE_DELAY = 400;

export function initDiagnostics(init: DiagnosticsInit): DiagnosticsPanel {
  const { mount } = init;
  mount.classList.add('diagnostics-host');

  /* ---------------------------------------------------------------- state */

  let result: ConversionResult | null = null;
  let open = init.open;
  let note: string | null = null;
  let listDirty = true;
  let announceTimer = 0;
  let announced = '';
  let flashTimer = 0;
  const showing: Record<Severity, boolean> = { error: true, warning: true, note: true };
  /**
   * Which rows have their `rendered` detail open, by position in the list.
   *
   * Position rather than content: two rows can carry the same identifier, message and
   * (since M9, routinely) the same substituted span, and a content key would collapse
   * them into one — one expander then opening both details, and {@link reveal} landing
   * on whichever was rendered last.
   */
  const expanded = new Set<number>();
  /** The rows now on screen, in the order {@link renderList} appended them. */
  let rows: HTMLLIElement[] = [];
  /** What those rows are showing, so {@link reveal} can find one by identity. */
  let showingRows: readonly Diagnostic[] = [];

  /* ------------------------------------------------------------------ DOM */

  const root = el('section', 'diagnostics');
  root.setAttribute('aria-label', 'Diagnostics');

  const strip = el('div', 'status');
  strip.dataset.busy = 'idle';

  const toggle = el('button', 'status-toggle status-item');
  toggle.type = 'button';
  toggle.id = 'diag-toggle';
  toggle.setAttribute('aria-expanded', String(open));
  toggle.setAttribute('aria-controls', 'diag-panel');
  const toggleCaret = el('span', 'caret', '▸');
  toggleCaret.setAttribute('aria-hidden', 'true');
  const counts = el('span', 'status-counts');
  toggle.append(toggleCaret, counts);
  toggle.addEventListener('click', () => {
    setOpen(!open);
    init.onToggle(open);
  });

  const timing = el('span', 'status-meta status-item');
  const chars = el('span', 'status-meta status-item');
  const noteLine = el('span', 'status-note status-item');
  noteLine.hidden = true;

  const busyLine = el('span', 'status-busy status-item');
  const busyDot = el('span', 'busy-dot');
  busyDot.setAttribute('aria-hidden', 'true');
  busyLine.append(busyDot, document.createTextNode('converting…'));
  busyLine.hidden = true;

  const cancel = el('button', 'btn btn-quiet status-cancel status-item');
  cancel.type = 'button';
  cancel.textContent = 'Cancel';
  cancel.hidden = true;
  cancel.addEventListener('click', () => init.onCancel());

  /** Announces counts only, and only when they change (§6.9). */
  const live = el('span', 'sr-only');
  live.setAttribute('role', 'status');
  live.setAttribute('aria-live', 'polite');

  strip.append(toggle, timing, chars, noteLine, busyLine, cancel, live);

  const panel = el('div', 'diag-panel');
  panel.id = 'diag-panel';
  panel.hidden = !open;

  const filters = el('div', 'diag-filters');
  filters.setAttribute('role', 'group');
  filters.setAttribute('aria-label', 'Show which severities');
  const filterButtons = new Map<Severity, HTMLButtonElement>();
  for (const severity of SEVERITIES) {
    const button = el('button', 'chip chip-filter');
    button.type = 'button';
    button.dataset.sev = severity;
    button.setAttribute('aria-pressed', 'true');
    button.addEventListener('click', () => {
      showing[severity] = !showing[severity];
      button.setAttribute('aria-pressed', String(showing[severity]));
      renderList();
    });
    filterButtons.set(severity, button);
    filters.append(button);
  }

  const list = el('ul', 'diag-list');
  const empty = el('p', 'diag-empty');
  const truncated = el('p', 'diag-truncated');
  truncated.hidden = true;

  // The list first, the filters that shape it below — the panel opens onto the
  // report itself rather than onto a row of controls (§7).
  panel.append(list, empty, truncated, filters);
  root.append(strip, panel);
  mount.append(root);

  renderStatus();

  /* --------------------------------------------------------------- render */

  function countOf(severity: Severity): number {
    if (!result) return 0;
    let n = 0;
    for (const diagnostic of result.diagnostics) {
      if (diagnostic.severity === severity) n += 1;
    }
    return n;
  }

  function renderStatus(): void {
    counts.replaceChildren();
    const totals = SEVERITIES.map((severity) => [severity, countOf(severity)] as const);
    const any = totals.some(([, n]) => n > 0);

    if (!result) {
      counts.append(el('span', 'status-clean', 'ready'));
    } else if (!any) {
      const clean = el('span', 'status-clean');
      const tick = el('span', 'clean-mark', '✓');
      tick.setAttribute('aria-hidden', 'true');
      clean.append(tick, document.createTextNode('no diagnostics'));
      counts.append(clean);
    } else {
      let first = true;
      for (const [severity, n] of totals) {
        if (n === 0) continue;
        if (!first) counts.append(separator());
        first = false;
        counts.append(count(severity, n));
      }
    }

    if (result && !result.ok) {
      const failed = el('span', 'status-failed', 'parse failed');
      counts.append(failed);
    }

    timing.textContent = result ? formatMs(result.ms) : '';
    timing.hidden = !result;
    chars.textContent = result ? `${formatCount(result.text.length)} chars` : '';
    chars.hidden = !result;
    noteLine.textContent = note ?? '';
    noteLine.hidden = note === null;

    for (const severity of SEVERITIES) {
      const button = filterButtons.get(severity);
      if (!button) continue;
      const n = countOf(severity);
      button.replaceChildren(mark(severity), document.createTextNode(words(severity, n)));
      button.disabled = n === 0;
    }

    announceLater();
  }

  function renderList(): void {
    if (!open) {
      listDirty = true;
      return;
    }
    listDirty = false;
    list.replaceChildren();
    rows = [];

    const diagnostics = result?.diagnostics ?? [];
    const visible = diagnostics.filter((d) => showing[d.severity]);
    showingRows = visible;
    for (const [index, diagnostic] of visible.entries()) {
      const item = row(diagnostic, index);
      rows.push(item);
      list.append(item);
    }

    if (diagnostics.length === 0) {
      empty.textContent = result
        ? 'Nothing to report — the document converted cleanly.'
        : 'No conversion yet.';
      empty.hidden = false;
    } else if (visible.length === 0) {
      empty.textContent = 'Every diagnostic is hidden by the filters above.';
      empty.hidden = false;
    } else {
      empty.hidden = true;
    }

    const suppressed = result?.truncated ? result.suppressed : 0;
    truncated.textContent = `and ${formatCount(suppressed)} more (retention limit)`;
    truncated.hidden = suppressed === 0;
  }

  function row(diagnostic: Diagnostic, index: number): HTMLLIElement {
    const item = el('li', 'diag-row');
    item.dataset.sev = diagnostic.severity;

    // An `approx` row is as clickable as any other: it *has* a span — the macro call
    // the expansion came from — and only the position column says which (§4.5).
    const jumpable = diagnostic.span !== null;
    const main = document.createElement(jumpable ? 'button' : 'div');
    main.className = jumpable ? 'drow-main' : 'drow-main is-inert';
    if (main instanceof HTMLButtonElement) {
      main.type = 'button';
      main.addEventListener('click', () => init.onSelect(diagnostic));
      main.title = diagnostic.approx
        ? 'Select the macro call this came from'
        : 'Select this in the source';
    }

    main.append(
      mark(diagnostic.severity),
      el('span', 'sr-only', `${PLURAL[diagnostic.severity][0]}: `),
      el('code', 'drow-id', diagnostic.identifier),
      el('span', 'drow-message', diagnostic.message),
    );
    const at = diagnostic.span ? `${diagnostic.span.line}:${diagnostic.span.column}` : null;
    const where = at === null ? 'no position' : diagnostic.approx ? `${at} (${APPROX_MARK})` : at;
    main.append(el('span', 'drow-where', where));
    if (!jumpable) {
      main.append(el('span', 'drow-why', NO_SPAN_REASON));
    }

    const detailId = `diag-detail-${index}`;
    const detail = el('div', 'drow-detail');
    detail.id = detailId;
    if (diagnostic.approx) detail.append(el('p', 'drow-detail-note', APPROX_REASON));
    const report = el('pre', 'drow-report', diagnostic.rendered);
    // The full report scrolls sideways, so it has to be reachable by keyboard.
    report.tabIndex = 0;
    detail.append(report);
    detail.hidden = !expanded.has(index);

    const expander = el('button', 'drow-expander');
    expander.type = 'button';
    expander.setAttribute('aria-expanded', String(expanded.has(index)));
    expander.setAttribute('aria-controls', detailId);
    const expanderCaret = el('span', 'caret');
    expanderCaret.setAttribute('aria-hidden', 'true');
    expander.append(expanderCaret, el('span', 'sr-only', 'Full report'));
    expander.title = 'The full report, as techxt-cli prints it';
    expander.addEventListener('click', () => {
      const next = detail.hidden;
      detail.hidden = !next;
      expander.setAttribute('aria-expanded', String(next));
      if (next) expanded.add(index);
      else expanded.delete(index);
    });

    item.append(main, expander, detail);
    return item;
  }

  /* ------------------------------------------------------------- announce */

  function announceLater(): void {
    window.clearTimeout(announceTimer);
    announceTimer = window.setTimeout(() => {
      const text = summary();
      if (text === announced) return;
      announced = text;
      live.textContent = text;
    }, ANNOUNCE_DELAY);
  }

  function summary(): string {
    if (!result) return '';
    const parts = SEVERITIES.map((severity) => [severity, countOf(severity)] as const)
      .filter(([, n]) => n > 0)
      .map(([severity, n]) => words(severity, n));
    if (parts.length === 0) return result.ok ? 'No diagnostics.' : 'Parse failed.';
    return `${parts.join(', ')}.`;
  }

  /* --------------------------------------------------------------- handle */

  function setOpen(next: boolean): void {
    open = next;
    panel.hidden = !next;
    toggle.setAttribute('aria-expanded', String(next));
    root.classList.toggle('is-open', next);
    if (next && listDirty) renderList();
  }

  return {
    setResult(next: ConversionResult) {
      result = next;
      renderStatus();
      listDirty = true;
      renderList();
    },

    setBusy(state: BusyState) {
      strip.dataset.busy = state;
      busyLine.hidden = state === 'idle';
      cancel.hidden = state !== 'cancellable';
    },

    setNote(next: string | null) {
      note = next;
      noteLine.textContent = next ?? '';
      noteLine.hidden = next === null;
    },

    setOpen,

    reveal(diagnostic: Diagnostic) {
      // A gutter marker can point at a severity the filters are currently hiding.
      if (!showing[diagnostic.severity]) {
        showing[diagnostic.severity] = true;
        filterButtons.get(diagnostic.severity)?.setAttribute('aria-pressed', 'true');
        listDirty = true;
      }
      setOpen(true); // also renders the list, if `listDirty` made that necessary
      // By identity: the gutter marker was painted from this very object, and two
      // diagnostics can otherwise be indistinguishable field for field.
      const li = rows[showingRows.indexOf(diagnostic)];
      if (!li) return;
      li.scrollIntoView({ block: 'nearest' });
      window.clearTimeout(flashTimer);
      li.classList.add('is-flash');
      flashTimer = window.setTimeout(() => li.classList.remove('is-flash'), 1200);
      li.querySelector<HTMLElement>('.drow-main')?.focus();
    },
  };

  /* -------------------------------------------------------------- helpers */

  function count(severity: Severity, n: number): HTMLSpanElement {
    const node = el('span', 'status-count');
    node.dataset.sev = severity;
    node.append(mark(severity), document.createTextNode(words(severity, n)));
    return node;
  }
}

/* ---------------------------------------------------------------- helpers */

/** The severity chip: a tinted shape carrying a letter, never colour alone. */
function mark(severity: Severity): HTMLSpanElement {
  const chip = el('span', 'chip-mark', MARK[severity]);
  chip.dataset.sev = severity;
  chip.setAttribute('aria-hidden', 'true');
  return chip;
}

/** The `·` of `2 errors · 3 warnings` (§7). */
function separator(): HTMLSpanElement {
  const node = el('span', 'status-dot', '·');
  node.setAttribute('aria-hidden', 'true');
  return node;
}

function words(severity: Severity, n: number): string {
  const [one, many] = PLURAL[severity];
  return `${n} ${n === 1 ? one : many}`;
}

function formatMs(ms: number): string {
  if (!Number.isFinite(ms)) return '';
  if (ms < 1) return '<1 ms';
  return `${Math.round(ms)} ms`;
}

function formatCount(n: number): string {
  return new Intl.NumberFormat().format(n);
}

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
