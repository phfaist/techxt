/**
 * The two panes: a textarea for the source, a `<pre>` for the answer (web/PLAN.md
 * §6.1, §6.3, §6.5, §6.6, §6.12, §6.13).
 *
 * Three properties of this module are load-bearing rather than decorative:
 *
 * - The output is `white-space: pre` and is written with `textContent`. The library
 *   decided the line breaks; a second, invisible wrapping by the browser would
 *   misrepresent the output, so *Wrap: Off* scrolls horizontally instead (§6.3).
 *   *Wrap: Soft* is the one exception, and an explicit one: it is the answer the app
 *   starts on, so {@link Panes.setSoftWrap} turns the folding on in CSS. The text
 *   itself is untouched either way — `getOutput` is what Copy and Download hand over.
 *   {@link Panes.markMath} may wrap a run of that text in an element *after* it has
 *   been set, for a typesetter to replace, and it too builds every node by hand.
 * - The fit-to-pane measurement (§6.5) is what makes *Wrap: Fit* mean anything: the
 *   pane's width in pixels becomes a column count the library can wrap to.
 * - The textarea turns off every helpful thing a phone does to prose. An editor that
 *   capitalises `\alpha` is worse than useless (§6.6).
 * - **The textarea stays a textarea.** Highlighting is a `<pre>`-like mirror behind it
 *   (§6.12), not a `contenteditable`, because `contenteditable` breaks
 *   `setSelectionRange` and {@link Panes.selectSpan} — the diagnostics' jump-to-source —
 *   depends on it. The same mirror carries the diagnostic underline, so the two share
 *   one element and cannot drift apart; they use different channels of it, colour for
 *   the lexer and a tint plus an underline for the diagnostics.
 *
 * Nothing here knows what a conversion is. Every user action leaves through a
 * callback of {@link PanesInit}; every state change the app makes arrives through a
 * method of {@link Panes} and fires nothing.
 */

import { candidatesFor, completionTrigger, nextInCycle } from '../completion';
import type { CompletionTrigger, TriggerKind } from '../completion';
import { applyFont } from '../fonts';
import type { FontId } from '../fonts';
import { chunkSplice, editorChunks, textEdit, tokenize } from '../highlight';
import type { EditorChunk, Mark, TextEdit } from '../highlight';
import { splitMathRuns } from '../math-regions';
import { MIN_FIT_COLUMNS, columnsFor } from '../state';
import type { ExampleDoc } from '../types';
import type { Completion, Diagnostic, MathRegion, Span } from '../worker/protocol';
import type { CompletionQuery, Panes, PanesInit } from './api';

/**
 * The 62 alphanumerics and a space (§6.5). Its mean advance is the advance itself for
 * a monospace face, and a good estimate for a proportional one.
 */
const SAMPLE =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZ' + 'abcdefghijklmnopqrstuvwxyz' + '0123456789' + ' ';

/** Everything that can change how wide `SAMPLE` renders, copied onto the gauge. */
const GAUGE_PROPS = [
  'font-family',
  'font-size',
  'font-weight',
  'font-style',
  'font-variant',
  'font-feature-settings',
  'font-variation-settings',
  'font-kerning',
  'letter-spacing',
  'word-spacing',
  'text-transform',
  'text-rendering',
  'tab-size',
] as const;

/** Matches the `max-width: 860px` breakpoint of `styles.css`. */
const DESKTOP_QUERY = '(min-width: 861px)';
const MIN_SPLIT = 0.2;
const MAX_SPLIT = 0.8;
/** A visual-viewport this much shorter than the layout viewport means a keyboard. */
const KEYBOARD_THRESHOLD = 120;

/**
 * Up to this many characters are lexed and spanned whole; past that, only a window
 * around what is on screen is (§6.12).
 *
 * Both numbers were measured rather than chosen. Building a span costs **5.3 µs**, and a
 * densely marked-up LaTeX document carries roughly 120 spans per kilobyte — so the cost of
 * highlighting is the size of the window and almost nothing else. A window of the
 * screenful in view plus this margin on each side is a few hundred spans; the whole of a
 * 20 KB document is 2 400 spans and 17 ms, which is a keystroke a typist would feel.
 *
 * The window survives the incremental rebuild that came after it, and for a second reason:
 * a repaint now replaces only the runs that changed, but the run *list* is still built
 * whole every time, so the window is still what keeps that list a thousand entries rather
 * than a hundred thousand.
 */
const HIGHLIGHT_WHOLE_LIMIT = 6_000;
/**
 * How much text on each side of the visible region is highlighted anyway — enough to
 * absorb both a scroll and the error in the estimate that places the window.
 *
 * That estimate is proportional (a character is a character's share of the content
 * height) because the alternative, measuring, means a forced layout inside the keystroke
 * that provoked it. Measured against the truth — a binary search with a `Range` over the
 * mirror — on a deliberately uneven 200 KB document, mixing wrapped prose with blocks of
 * short lines, it was wrong by at most **1 033 characters**, so this margin is that error
 * plus about a screenful of scrolling.
 */
const HIGHLIGHT_MARGIN = 3_000;

/** The chips the row shows, which is also the length of the Tab cycle (§6.13). */
const CHIP_CAP = 5;
/**
 * How many entries a macro query asks for. A few more than the row shows, because the
 * app folds `\begin` and `\end` in at the head and a cap applied before that would let
 * two literals push two real suggestions off the end.
 */
const MACRO_QUERY_LIMIT = 8;
/**
 * How many an environment query asks for. `complete()` takes no kind and ranks macros
 * above environments, so the only way to be sure the environments are in the answer at
 * all is to ask for enough of it to reach them (§6.13). It is a cap and not a count: a
 * prefix that matches twelve names answers with twelve.
 */
const ENVIRONMENT_QUERY_LIMIT = 250;

export function initPanes(init: PanesInit): Panes {
  const { mount, ui, examples } = init;
  mount.classList.add('panes-host');

  /* ---------------------------------------------------------------- state */

  let split = clamp(ui.split, MIN_SPLIT, MAX_SPLIT);
  let focus: 'input' | 'output' | null = ui.focus;
  let fontId: FontId = ui.font;
  let fontSize = ui.size;
  let outputText = '';
  let columnsValue = 0;
  let reportedColumns = 0;
  /** Mean advance per (font, size); a face that has not loaded yet is re-measured. */
  const advanceCache = new Map<string, number>();

  /** The latest error/warning diagnostics with a span — what §7's markers paint. */
  let paintDiagnostics: Array<Diagnostic & { span: Span }> = [];
  /** The text `paintDiagnostics`' offsets are valid for — see `remapPaintDiagnostics`. */
  let paintDiagnosticsText = '';
  /** One gutter button per painted diagnostic, in the same order. */
  let gutterButtons: HTMLButtonElement[] = [];
  /** Each button's vertical offset within the *content*, independent of scrolling. */
  let gutterContentTops: number[] = [];
  let gutterLineHeight = 0;

  /**
   * Whether the syntax colours are painted at all (§6.12).
   *
   * One flag for the whole feature, on purpose: it gates the lexing and the class that
   * makes the textarea's own glyphs transparent together, so turning it off leaves the
   * pane exactly as it was before highlighting existed. That is the escape hatch if a
   * real device ever disagrees with the overlay.
   */
  let highlighting = true;
  /**
   * Whether an IME is composing right now, which suspends the colours until it is done.
   *
   * The composing run is the one thing in the textarea the mirror cannot reproduce: the
   * browser draws it with its own underline and its own candidate window, and hiding it
   * behind a transparent-text overlay is how an overlay editor eats an input method. So
   * composition puts the real text back on screen for as long as it lasts, which costs a
   * colourless second rather than a broken editor.
   */
  let composing = false;
  /** The window the mirror is currently spanned over, in characters — see `takeWindow`. */
  let windowFrom = 0;
  let windowTo = 0;
  /**
   * The runs the mirror is holding right now, one per child node of `backdrop` and in
   * the same order.
   *
   * This is what makes the rebuild incremental (§6.12): the next repaint is diffed
   * against it, and only the runs that actually changed are touched. It is the mirror's
   * own record of itself, so anything that replaces the mirror's children by another
   * route has to replace this too, or the two fall out of step — `paintBackdrop` checks
   * that they agree before believing either.
   */
  let backdropChunks: EditorChunk[] = [];
  /** The text `backdropChunks` and the window were computed for — see `paintBackdrop`. */
  let paintedText = '';

  /**
   * The pane's geometry as of the last time it was cheap to ask, which is what places the
   * highlight window (§6.12).
   *
   * Reading `scrollTop`, `clientHeight` or `scrollHeight` from an element whose text has
   * just changed makes the browser lay the whole document out before it can answer, and
   * inside a keystroke that is the most expensive thing the pane does. So the keystroke
   * reads this instead, and the record is refreshed where a layout is already happening
   * or already paid for: on a scroll, in the frame after an edit, in the debounced
   * relayout that measures the gutter, and on a paste — which can change the document's
   * height by a factor and is not something anyone does sixty times a second.
   *
   * It is allowed to be a frame out of date. What it decides is which characters get
   * *colour*, and the margin around the window is three thousand characters wide.
   */
  let geometry = { scrollTop: 0, clientHeight: 0, scrollHeight: 0 };
  function readGeometry(): void {
    geometry = {
      scrollTop: input.scrollTop,
      clientHeight: input.clientHeight,
      scrollHeight: input.scrollHeight,
    };
  }

  /* ------------------------------------------------------------------ DOM */

  const root = el('div', 'panes');
  root.dataset.focus = focus ?? 'none';

  /* --- input pane */

  const inPane = el('section', 'pane pane-in');
  inPane.setAttribute('aria-labelledby', 'pane-title-in');
  const inTitle = el('h2', 'pane-title', 'LaTeX');
  inTitle.id = 'pane-title-in';

  const loadButton = el('button', 'btn btn-labelled menu-button');
  loadButton.type = 'button';
  loadButton.setAttribute('aria-haspopup', 'menu');
  loadButton.setAttribute('aria-expanded', 'false');
  loadButton.append(label('Load'), caret('▾'));

  const menu = el('div', 'menu-popup');
  menu.setAttribute('role', 'menu');
  menu.setAttribute('aria-label', 'Example documents');
  menu.hidden = true;
  const menuItems: HTMLButtonElement[] = [];
  for (const example of examples) {
    menuItems.push(menuItem(example));
  }
  menu.append(...menuItems);

  const menuWrap = el('div', 'menu');
  menuWrap.append(loadButton, menu);

  /**
   * **New** (§6.10): seal the entry the document is being logged into and clear the
   * input. It sits beside `Load ▾` because that is the moment it is reached for — the
   * user is about to type something new and is looking at the source pane — not
   * because of what it acts on.
   */
  const newButton = el('button', 'btn btn-labelled');
  newButton.type = 'button';
  newButton.append(label('New'));
  newButton.title = 'Keep this document as it is and start an empty one';
  newButton.addEventListener('click', () => init.onNew());

  /**
   * Which library entry the keystrokes are going into, in the header where they are
   * going in (§6.10).
   *
   * The complaint item 8 answers is that the app was silent about this, so the chip is
   * a button rather than a label: it opens the library at that entry, which makes the
   * current entry not only visible but reachable. It is hidden where there is no
   * library at all.
   */
  const entryChip = el('button', 'btn pane-entry');
  entryChip.type = 'button';
  entryChip.hidden = true;
  const entryGlyph = el('span', 'pane-entry-glyph', '●');
  entryGlyph.setAttribute('aria-hidden', 'true');
  const entryLabel = el('span', 'pane-entry-label');
  entryChip.append(entryGlyph, entryLabel);
  entryChip.addEventListener('click', () => init.onShowEntry());

  const inFocus = focusButton('input');
  const inTools = el('div', 'pane-tools');
  inTools.append(newButton, menuWrap, inFocus);

  const inHead = el('header', 'pane-head');
  inHead.append(inTitle, entryChip, inTools);

  const inputLabel = el('label', 'sr-only', 'LaTeX source');
  inputLabel.htmlFor = 'techxt-input';

  const input = el('textarea', 'pane-body pane-input');
  input.id = 'techxt-input';
  input.setAttribute('autocapitalize', 'off');
  input.setAttribute('autocorrect', 'off');
  input.setAttribute('autocomplete', 'off');
  input.setAttribute('inputmode', 'text');
  input.setAttribute('wrap', 'soft');
  input.spellcheck = false;

  /**
   * A gentle underline for the error/warning spans, laid behind `input` (§7). A
   * `<textarea>` cannot style part of its own text, so this mirrors it instead: same
   * classes, so the same padding and font keep every character lined up with the
   * real one above it; transparent text of its own, so only the highlight shows.
   */
  const backdrop = el('div', 'pane-body pane-input pane-input-backdrop');
  backdrop.setAttribute('aria-hidden', 'true');

  const editorArea = el('div', 'pane-editor-area');
  editorArea.append(backdrop, input);

  /** One clickable bar per error/warning span, aligned to the line it points at. */
  const gutter = el('div', 'pane-gutter');

  const editorShell = el('div', 'pane-editor');
  editorShell.append(gutter, editorArea);

  /* --- the completion chips (§6.13) */

  /**
   * A row under the input, never a popup: it works the same on a desktop and on a
   * phone, it never covers what is being typed, and it is nothing at all when there is
   * nothing to suggest.
   */
  const completionRow = el('div', 'completion-row');
  completionRow.hidden = true;
  completionRow.setAttribute('role', 'group');
  completionRow.setAttribute('aria-label', 'Completions');
  const completionChips = el('div', 'completion-chips');
  const completionHint = el('span', 'completion-hint', 'Tab to cycle');
  completionHint.setAttribute('aria-hidden', 'true');
  /** What a screen reader is told as the cycle moves; the chips themselves are visual. */
  const completionStatus = el('span', 'sr-only');
  completionStatus.setAttribute('aria-live', 'polite');
  completionRow.append(completionChips, completionHint, completionStatus);

  inPane.append(inHead, inputLabel, editorShell, completionRow);

  /* --- divider */

  const divider = el('div', 'pane-divider');
  divider.setAttribute('role', 'separator');
  divider.setAttribute('aria-orientation', 'vertical');
  divider.setAttribute('aria-label', 'Width of the source pane');
  divider.setAttribute('aria-valuemin', String(Math.round(MIN_SPLIT * 100)));
  divider.setAttribute('aria-valuemax', String(Math.round(MAX_SPLIT * 100)));
  divider.tabIndex = 0;
  divider.append(el('span', 'pane-divider-grip'));

  /* --- output pane */

  const outPane = el('section', 'pane pane-out');
  outPane.setAttribute('aria-labelledby', 'pane-title-out');
  const outTitle = el('h2', 'pane-title', 'Text');
  outTitle.id = 'pane-title-out';

  const fontNote = el('span', 'pane-note', 'loading font…');
  fontNote.hidden = true;

  const staleNote = el('span', 'pane-flag', 'out of date');
  staleNote.hidden = true;

  const copyButton = el('button', 'btn btn-labelled');
  copyButton.type = 'button';
  copyButton.append(
    svgIcon(
      ['rect', { x: '5.25', y: '5.25', width: '8.5', height: '9.5', rx: '1.75' }],
      ['path', { d: 'M10.75 5.25V3.5A1.25 1.25 0 0 0 9.5 2.25H3.5A1.25 1.25 0 0 0 2.25 3.5v8A1.25 1.25 0 0 0 3.5 12.75h1.75' }],
    ),
    label('Copy'),
  );
  copyButton.title = 'Copy the converted text';
  copyButton.addEventListener('click', () => init.onCopy());

  const downloadButton = el('button', 'btn');
  downloadButton.type = 'button';
  downloadButton.append(
    svgIcon(
      ['path', { d: 'M8 2.25v7.5m0 0 2.75-2.75M8 9.75 5.25 7' }],
      ['path', { d: 'M2.5 11.5v1.25c0 .55.45 1 1 1h9c.55 0 1-.45 1-1V11.5' }],
    ),
    label('Download'),
  );
  downloadButton.title = 'Save the converted text as a file';
  downloadButton.addEventListener('click', () => init.onDownload());

  /**
   * **Save** (§6.10). Everything is already saved — the log is automatic — so what
   * this really does is stop *this* version being edited: it seals the entry and the
   * next edit starts a new one. The tooltip carries that truth, because the word on
   * the button cannot.
   *
   * It is here rather than beside New because of the moment it is reached for: the
   * user is happy with a result and is looking at the output.
   */
  const saveButton = el('button', 'btn btn-labelled');
  saveButton.type = 'button';
  saveButton.hidden = true;
  saveButton.append(label('Save'));
  saveButton.title = 'Keep this version — further edits start a new entry';
  saveButton.addEventListener('click', () => init.onSave());

  /**
   * **★**: seal the entry *and* star it, so it can be filtered to and is never
   * removed by anything automatic (§6.10).
   *
   * Icon-only, and deliberately: this header already carries four controls plus the ⇅
   * Focus button on a phone, and a fifth word would not fit. The glyph is the whole
   * control, so its name lives in `aria-label` and its meaning in the tooltip.
   */
  const starButton = el('button', 'btn pane-star');
  starButton.type = 'button';
  starButton.hidden = true;
  starButton.setAttribute('aria-pressed', 'false');
  const starGlyph = el('span', 'pane-star-glyph', '☆');
  starGlyph.setAttribute('aria-hidden', 'true');
  starButton.append(starGlyph);
  starButton.addEventListener('click', () => init.onStar());

  const outFocus = focusButton('output');
  const outTools = el('div', 'pane-tools');
  outTools.append(copyButton, downloadButton, saveButton, starButton, outFocus);

  const outHead = el('header', 'pane-head');
  outHead.append(outTitle, fontNote, staleNote, outTools);

  const output = el('pre', 'pane-body pane-output');
  output.id = 'techxt-output';
  output.tabIndex = 0;
  output.setAttribute('aria-label', 'Converted text');

  /** The hidden span the fit-to-pane measurement is taken from (§6.5). */
  const gauge = el('span', 'pane-gauge');
  gauge.setAttribute('aria-hidden', 'true');
  gauge.textContent = SAMPLE;

  outPane.append(outHead, output, gauge);
  root.append(inPane, divider, outPane);
  mount.append(root);

  applySplit();
  applyFocus();
  editorArea.classList.toggle('is-highlighted', highlighting);

  /* ------------------------------------------------------- the Load ▾ menu */

  function menuItem(example: ExampleDoc): HTMLButtonElement {
    const item = el('button', 'menu-item');
    item.type = 'button';
    item.setAttribute('role', 'menuitem');
    item.append(
      el('span', 'menu-item-title', example.title),
      el('span', 'menu-item-blurb', example.blurb),
    );
    item.addEventListener('click', () => {
      closeMenu(true);
      init.onLoadExample(example);
    });
    return item;
  }

  function openMenu(): void {
    if (!menu.hidden) return;
    menu.hidden = false;
    loadButton.setAttribute('aria-expanded', 'true');
    menuItems[0]?.focus();
    document.addEventListener('pointerdown', onDocumentPointer, true);
  }

  function closeMenu(restoreFocus: boolean): void {
    if (menu.hidden) return;
    menu.hidden = true;
    loadButton.setAttribute('aria-expanded', 'false');
    document.removeEventListener('pointerdown', onDocumentPointer, true);
    if (restoreFocus) loadButton.focus();
  }

  function onDocumentPointer(event: Event): void {
    if (!(event.target instanceof Node) || !menuWrap.contains(event.target)) {
      closeMenu(false);
    }
  }

  loadButton.addEventListener('click', () => {
    if (menu.hidden) openMenu();
    else closeMenu(true);
  });

  menuWrap.addEventListener('keydown', (event) => {
    if (menu.hidden) {
      // Enter and Space already click the button; only Down needs a hand.
      if (event.key === 'ArrowDown' && event.target === loadButton) {
        event.preventDefault();
        openMenu();
      }
      return;
    }
    const index = menuItems.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      closeMenu(true);
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      menuItems[Math.min(index + 1, menuItems.length - 1)]?.focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (index <= 0) closeMenu(true);
      else menuItems[index - 1]?.focus();
    } else if (event.key === 'Home') {
      event.preventDefault();
      menuItems[0]?.focus();
    } else if (event.key === 'End') {
      event.preventDefault();
      menuItems[menuItems.length - 1]?.focus();
    } else if (event.key === 'Tab') {
      closeMenu(false);
    }
  });

  /* ------------------------------------------------------------ the ⇅ button */

  function focusButton(which: 'input' | 'output'): HTMLButtonElement {
    const button = el('button', 'btn focus-button');
    button.type = 'button';
    button.append(caret('⇅'), label('Focus'));
    button.title = 'Give this pane the whole screen';
    button.addEventListener('click', () => {
      const next = focus === which ? null : which;
      focus = next;
      applyFocus();
      scheduleMeasure();
      init.onFocusChange(next);
    });
    return button;
  }

  function applyFocus(): void {
    root.dataset.focus = focus ?? 'none';
    inFocus.setAttribute('aria-pressed', String(focus === 'input'));
    outFocus.setAttribute('aria-pressed', String(focus === 'output'));
  }

  /* ----------------------------------------------------------- the divider */

  function applySplit(): void {
    root.style.setProperty('--split-a', `${split}fr`);
    root.style.setProperty('--split-b', `${1 - split}fr`);
    divider.setAttribute('aria-valuenow', String(Math.round(split * 100)));
    divider.setAttribute('aria-valuetext', `${Math.round(split * 100)} percent`);
  }

  let dragging = false;

  divider.addEventListener('pointerdown', (event) => {
    if (!window.matchMedia(DESKTOP_QUERY).matches) return;
    dragging = true;
    root.classList.add('is-dragging');
    divider.setPointerCapture(event.pointerId);
    event.preventDefault();
  });

  divider.addEventListener('pointermove', (event) => {
    if (!dragging) return;
    const rect = root.getBoundingClientRect();
    if (rect.width <= 0) return;
    split = clamp((event.clientX - rect.left) / rect.width, MIN_SPLIT, MAX_SPLIT);
    applySplit();
  });

  function endDrag(event: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    root.classList.remove('is-dragging');
    if (divider.hasPointerCapture(event.pointerId)) divider.releasePointerCapture(event.pointerId);
    scheduleMeasure();
    init.onSplitChange(split);
  }

  divider.addEventListener('pointerup', endDrag);
  divider.addEventListener('pointercancel', endDrag);

  divider.addEventListener('dblclick', () => {
    split = 0.5;
    applySplit();
    scheduleMeasure();
    init.onSplitChange(split);
  });

  divider.addEventListener('keydown', (event) => {
    const step = event.key === 'PageDown' || event.key === 'PageUp' ? 0.1 : 0.02;
    let next = split;
    if (event.key === 'ArrowLeft' || event.key === 'PageUp') next = split - step;
    else if (event.key === 'ArrowRight' || event.key === 'PageDown') next = split + step;
    else if (event.key === 'Home') next = MIN_SPLIT;
    else if (event.key === 'End') next = MAX_SPLIT;
    else if (event.key === 'Enter') next = 0.5;
    else return;
    event.preventDefault();
    split = clamp(next, MIN_SPLIT, MAX_SPLIT);
    applySplit();
    scheduleMeasure();
    init.onSplitChange(split);
  });

  /* ------------------------------------------------ fit-to-pane measurement */

  function paneContentWidth(): number {
    const style = getComputedStyle(output);
    const pad = parseFloat(style.paddingLeft || '0') + parseFloat(style.paddingRight || '0');
    return output.clientWidth - pad;
  }

  function meanAdvance(): number {
    const key = `${fontId}|${fontSize}`;
    const cached = advanceCache.get(key);
    if (cached !== undefined) return cached;
    const style = getComputedStyle(output);
    for (const prop of GAUGE_PROPS) {
      gauge.style.setProperty(prop, style.getPropertyValue(prop));
    }
    const width = gauge.getBoundingClientRect().width;
    if (!(width > 0)) return 0;
    const advance = width / SAMPLE.length;
    advanceCache.set(key, advance);
    return advance;
  }

  /** Measure, and report only a genuine change in the column count (§6.5). */
  function measure(): void {
    const known = reportedColumns;
    const next = measureQuietly();
    if (next === 0 || next === known) return;
    init.onColumnsChange(next);
  }

  /**
   * Measure without announcing it — for a `columns()` call, whose return value tells
   * the caller the number anyway, so a callback on top of it would only provoke a
   * second conversion for a number the caller already has.
   */
  function measureQuietly(): number {
    const width = paneContentWidth();
    if (!(width > 0)) return 0; // hidden pane (mobile focus mode): keep the last value
    const advance = meanAdvance();
    if (!(advance > 0)) return 0;
    columnsValue = columnsFor(width, advance);
    reportedColumns = columnsValue;
    return columnsValue;
  }

  let measureTimer = 0;
  function scheduleMeasure(): void {
    window.clearTimeout(measureTimer);
    measureTimer = window.setTimeout(measure, 100);
  }

  const observer = new ResizeObserver(scheduleMeasure);
  observer.observe(output);
  window.addEventListener('orientationchange', scheduleMeasure);
  screen.orientation?.addEventListener?.('change', scheduleMeasure);
  // A face that swaps in after `document.fonts.load` resolved changes the advance.
  document.fonts?.addEventListener?.('loadingdone', () => {
    advanceCache.clear();
    scheduleMeasure();
  });
  requestAnimationFrame(() => {
    measure();
    // The first honest answer about the pane's geometry, which every later keystroke
    // reads instead of asking again.
    readGeometry();
  });

  /* ------------------------------------------------------ diagnostics in the editor */

  /**
   * `paintDiagnostics` are only ever as fresh as the last conversion result, but the
   * debounce that keeps that cheap also means a keystroke can land well before one
   * arrives. Left alone, a span past the caret would sit on the wrong characters
   * every time an earlier edit shifted it — not just briefly, but until the next
   * result — so every edit nudges the cached offsets along with it, the same way an
   * editor's own decorations track a live document. Offsets in the common prefix or
   * suffix around the edit move by its length delta; a span the edit actually landed
   * inside cannot be repositioned in any way that means anything, so it is dropped
   * until the real result replaces it.
   */
  function remapPaintDiagnostics(before: string, after: string): void {
    if (paintDiagnostics.length === 0 || before === after) return;
    const { prefix, oldEnd: oldEditEnd, delta } = textEdit(before, after);

    const next: Array<Diagnostic & { span: Span }> = [];
    for (const diagnostic of paintDiagnostics) {
      const { span } = diagnostic;
      if (span.start >= oldEditEnd) {
        next.push({ ...diagnostic, span: { ...span, start: span.start + delta, end: span.end + delta } });
      } else if (span.end <= prefix) {
        next.push(diagnostic);
      }
      // Otherwise the edit falls inside this span — no ink for it until it's real.
    }
    paintDiagnostics = next;
  }

  /** The diagnostics as the mirror wants them: clamped ranges with a severity. */
  function marks(text: string): Mark[] {
    return paintDiagnostics
      .map((d) => ({
        start: clamp(d.span.start, 0, text.length),
        end: clamp(d.span.end, 0, text.length),
        severity: d.severity === 'error' ? ('error' as const) : ('warning' as const),
      }))
      .filter((mark) => mark.end > mark.start);
  }

  /**
   * Which characters are on screen, estimated from the scroll offset (§6.12).
   *
   * A character is a character's share of the content height. The alternative — asking
   * the browser where a character actually is — means a forced layout, and on a 200 KB
   * document that is the single most expensive thing a keystroke can do, so the estimate
   * is taken from `geometry`, which the pane refreshes where a layout is already being
   * paid for. Measured against the truth (a binary search with a `Range` over the mirror)
   * on a deliberately uneven 200 KB document it was wrong by at most 1 033 characters,
   * which is what {@link HIGHLIGHT_MARGIN} is sized for.
   */
  function seenRange(text: string): { from: number; to: number } | null {
    if (!(geometry.scrollHeight > 0)) return null;
    const perPixel = text.length / geometry.scrollHeight;
    return {
      from: Math.round(geometry.scrollTop * perPixel),
      to: Math.round((geometry.scrollTop + geometry.clientHeight) * perPixel),
    };
  }

  /**
   * Settle `windowFrom`/`windowTo`: which characters are worth turning into elements
   * (§6.12).
   *
   * A short document is all of them: it is a few hundred spans and the whole question
   * does not arise. Past {@link HIGHLIGHT_WHOLE_LIMIT} the mirror still holds every
   * character — the alignment with the textarea above it depends on that — but only a
   * window of them is *spanned*, and the rest is one text node per side.
   *
   * **The window is over characters, not over offsets, and it stays over the same ones
   * until the screen leaves it.** That is what lets the rebuild be a splice. Re-deriving
   * the window from the estimate on every keystroke moves both its edges by a fraction of
   * a character each time, and an edge that moves rewrites the hundred-kilobyte text node
   * on that side of it — so the mirror would be rebuilt whole again by a different route.
   * So the edges are instead carried along by the edit, exactly as the diagnostics' spans
   * are, and only a screen that has scrolled out of the window makes it move. The margin
   * is what makes that safe: by the time the screen reaches an edge there are three
   * thousand characters of coloured text beyond it.
   */
  function takeWindow(text: string, edit: TextEdit | null): void {
    if (text.length <= HIGHLIGHT_WHOLE_LIMIT) {
      windowFrom = 0;
      windowTo = text.length;
      return;
    }
    if (edit !== null) {
      // An edge the edit happened before moves with it; one it happened after does not.
      if (edit.oldEnd <= windowFrom) windowFrom += edit.delta;
      if (edit.oldEnd <= windowTo) windowTo += edit.delta;
    }
    windowTo = clamp(windowTo, 0, text.length);
    windowFrom = clamp(windowFrom, 0, windowTo);
    const seen = seenRange(text);
    if (seen === null) {
      // Nothing laid out yet — the top of the document is the only honest guess.
      windowFrom = 0;
      windowTo = Math.min(text.length, HIGHLIGHT_WHOLE_LIMIT);
      return;
    }
    // Keeping the window still is only worth anything while it is still about the right
    // size. It grows a character every time an edit lands inside it, and it can start out
    // far too wide if the geometry it was first derived from was a document ago — and a
    // window that covers the screen can never be *left*, so without this it would stay
    // too wide for as long as the document was open, spanning text nobody is looking at.
    const roomy = seen.to - seen.from + 3 * HIGHLIGHT_MARGIN;
    if (
      windowTo > windowFrom &&
      seen.from >= windowFrom &&
      seen.to <= windowTo &&
      windowTo - windowFrom <= roomy
    ) {
      return;
    }
    windowFrom = Math.max(0, seen.from - HIGHLIGHT_MARGIN);
    windowTo = Math.min(text.length, seen.to + HIGHLIGHT_MARGIN);
  }

  /** The runs the mirror should be holding for `text`, in order, tiling it exactly. */
  function backdropRuns(text: string, edit: TextEdit | null): EditorChunk[] {
    const painting = highlighting && !composing;
    if (painting) takeWindow(text, edit);
    else {
      windowFrom = 0;
      windowTo = 0;
    }

    const tokens = painting ? tokenize(text, windowFrom, windowTo) : [];
    // Outside the window the diagnostics still have to be painted: there are a handful
    // of them, they are the older of the two channels, and a warning that stopped being
    // underlined when the document grew would be a regression.
    const spans = marks(text);
    const cuts = painting
      ? [
          { from: 0, to: windowFrom, tokens: [] as ReturnType<typeof tokenize> },
          { from: windowFrom, to: windowTo, tokens },
          { from: windowTo, to: text.length, tokens: [] as ReturnType<typeof tokenize> },
        ]
      : [{ from: 0, to: text.length, tokens: [] as ReturnType<typeof tokenize> }];

    const runs: EditorChunk[] = [];
    for (const cut of cuts) {
      if (cut.to <= cut.from) continue;
      for (const chunk of editorChunks(text, cut.tokens, spans, cut.from, cut.to)) runs.push(chunk);
    }
    // A textarea shows a trailing blank line when the value ends in `\n`; without
    // this, the mirror's last line falls a row short and every marker below it drifts.
    if (text === '' || text.endsWith('\n')) {
      runs.push({ text: ' ', token: null, inMath: false, severity: null });
    }
    return runs;
  }

  /** One run, as the node the mirror holds it in. */
  function runNode(chunk: EditorChunk): Node {
    if (chunk.token === null && chunk.severity === null) return document.createTextNode(chunk.text);
    const classes: string[] = [];
    if (chunk.token !== null) {
      classes.push(`tk-${chunk.token}`);
      if (chunk.inMath) classes.push('tk-in-math');
    }
    if (chunk.severity !== null) classes.push(`hl-${chunk.severity}`);
    return el('span', classes.join(' '), chunk.text);
  }

  /**
   * Repaint the mirror: the lexer's colours and the diagnostics' underline, in one flat
   * list of spans (§6.12, §7).
   *
   * Every node is built here from slices of the textarea's own value — no markup is
   * parsed and no `innerHTML` is assigned — and the slices tile the text exactly, which
   * is what keeps every character in the mirror underneath the character it belongs to.
   *
   * **What changes is spliced in; what did not change is left alone.** A keystroke moves
   * one run of a document, so `chunkSplice` finds the head and the tail the two paintings
   * share and this touches only the gap between them. That saves building the hundreds of
   * elements a window carries, and it saves the browser the style and layout work of
   * adopting them — on a 200 KB document the second of those was most of what a keystroke
   * cost. The mirror still holds every character either way; only the number of nodes
   * that had to be replaced to get there is different.
   */
  function paintBackdrop(): void {
    const text = input.value;
    const edit = text === paintedText ? null : textEdit(paintedText, text);
    paintedText = text;
    const runs = backdropRuns(text, edit);
    // If anything has replaced the mirror's children behind this module's back, the
    // record of what it holds is a lie and the safe answer is to build it again.
    if (backdrop.childNodes.length !== backdropChunks.length) {
      backdrop.replaceChildren(...runs.map(runNode));
      backdropChunks = runs;
      return;
    }
    const splice = chunkSplice(backdropChunks, runs);
    backdropChunks = runs;
    if (splice === null) return;
    const { at, removed, inserted } = splice;
    for (let i = 0; i < removed; i += 1) {
      const node = backdrop.childNodes[at];
      if (node === undefined) break;
      backdrop.removeChild(node);
    }
    if (inserted.length > 0) {
      const fragment = document.createDocumentFragment();
      for (const chunk of inserted) fragment.append(runNode(chunk));
      backdrop.insertBefore(fragment, backdrop.childNodes[at] ?? null);
    }
  }

  /** Rebuild the gutter's buttons and cache each one's position within the content. */
  function rebuildGutter(): void {
    gutter.replaceChildren();
    gutterButtons = [];
    gutterContentTops = [];
    // Hidden (0-width) in mobile focus mode: nothing to measure, and `buildMirror`
    // would copy a stale or percentage width. It rebuilds correctly once shown again.
    if (paintDiagnostics.length === 0 || input.clientWidth === 0) return;

    gutterContentTops = caretOffsets(
      input,
      paintDiagnostics.map((d) => d.span.start),
    );
    gutterLineHeight = parseFloat(getComputedStyle(input).lineHeight) || 16;

    for (const diagnostic of paintDiagnostics) {
      const button = el('button', 'gutter-marker');
      button.type = 'button';
      button.dataset.sev = diagnostic.severity;
      // `via macro` for a span the binding substituted for one inside an expansion
      // (§4.5): the marker sits at the macro call, and says so rather than implying the
      // message was raised there.
      const line = diagnostic.span ? `line ${diagnostic.span.line}` : '';
      const via = diagnostic.approx ? ', via macro' : '';
      const where = line ? ` (${line}${via})` : '';
      button.title = `${diagnostic.severity}${where}: ${diagnostic.message}`;
      button.setAttribute('aria-label', button.title);
      button.addEventListener('click', () => init.onMarkerSelect(diagnostic));
      gutter.append(button);
      gutterButtons.push(button);
    }
    positionGutter();
  }

  /** Cheap re-layout for a scroll: reuses the cached content offsets. */
  function positionGutter(): void {
    const top = input.scrollTop;
    for (const [index, button] of gutterButtons.entries()) {
      const contentTop = gutterContentTops[index];
      if (contentTop === undefined) continue;
      button.style.top = `${contentTop - top}px`;
      button.style.height = `${gutterLineHeight}px`;
    }
  }

  /**
   * Put the mirror where the textarea is, in the next frame rather than in this
   * keystroke (§6.12).
   *
   * `backdrop.scrollTop = input.scrollTop` looks free and is not: it reads a scroll
   * offset from an element whose text the edit has just invalidated, so the browser has
   * to lay the whole document out before it can answer, and then lay it out a second time
   * for the frame. Asked in a `requestAnimationFrame` instead, the question arrives when
   * the browser was going to do that work anyway, and it is done once. The scroll offset
   * has not changed in the meantime unless the caret moved the view, and if it did, the
   * scroll event that says so is dispatched before this callback runs.
   */
  let scrollSyncPending = 0;
  function scheduleScrollSync(): void {
    if (scrollSyncPending) return;
    scrollSyncPending = requestAnimationFrame(() => {
      scrollSyncPending = 0;
      readGeometry();
      backdrop.scrollTop = geometry.scrollTop;
      backdrop.scrollLeft = input.scrollLeft;
    });
  }

  /** The cheap half of a relayout: string slicing and a handful of DOM nodes. */
  function syncBackdrop(): void {
    paintBackdrop();
    scheduleScrollSync();
  }

  function relayoutDiagnostics(): void {
    // This one is not in a keystroke and measures the gutter in a throwaway mirror
    // anyway, so the layout it forces is already paid for: take the geometry while it
    // is fresh, which is what keeps the window estimate honest between bursts.
    readGeometry();
    paintBackdrop();
    backdrop.scrollTop = input.scrollTop;
    backdrop.scrollLeft = input.scrollLeft;
    rebuildGutter();
  }

  // `rebuildGutter` measures in a throwaway mirror (a forced layout) — worth
  // debouncing on a resize. `syncBackdrop` is cheap and runs on every keystroke
  // instead (below): with the debounce, a fast typist would never let it fire, and
  // the highlight would sit frozen on stale, unwrapped text for the whole burst.
  let diagLayoutTimer = 0;
  function scheduleRelayoutDiagnostics(): void {
    window.clearTimeout(diagLayoutTimer);
    diagLayoutTimer = window.setTimeout(relayoutDiagnostics, 80);
  }

  input.addEventListener('scroll', () => {
    // A scroll event is dispatched with the layout already up to date, so this is the
    // one place the geometry can be had for nothing — and it is the place where it
    // changes.
    readGeometry();
    backdrop.scrollTop = geometry.scrollTop;
    backdrop.scrollLeft = input.scrollLeft;
    positionGutter();
    // A document large enough to be windowed can be scrolled out of its window, and
    // the colours have to follow (§6.12). Cheap to ask, and the margin means the answer
    // is almost always no.
    if (highlighting && !composing && input.value.length > HIGHLIGHT_WHOLE_LIMIT) {
      const seen = seenRange(input.value);
      if (seen !== null && (seen.from < windowFrom || seen.to > windowTo)) {
        paintBackdrop();
        // A repaint replaces nodes under a scroll offset that was set a moment ago, so
        // say it again once the browser has had a chance to disagree.
        scheduleScrollSync();
      }
    }
  });

  // Wrapping depends on the pane's width, so every resize — the split drag, a focus
  // toggle, an orientation change — moves every line after the first wrapped one.
  const editorObserver = new ResizeObserver(scheduleRelayoutDiagnostics);
  editorObserver.observe(input);

  /* --------------------------------------------------- completion (§6.13) */

  /** What the row is currently about, kept in step with the caret. */
  let trigger: CompletionTrigger | null = null;
  /** The query whose answer the row is waiting for; the object itself is the token. */
  let pendingQuery: CompletionQuery | null = null;
  /** What the row shows, left to right, which is the order Tab walks. */
  let candidates: Completion[] = [];
  let chips: HTMLButtonElement[] = [];
  /**
   * The cycle, which exists only between its first Tab and whatever ends it.
   *
   * `typed` is the user's own text, kept so that both ends of the ring come back to it;
   * `end` moves as each candidate replaces the last one, since that is the range the
   * next press has to replace. The candidate list is *not* re-queried while this lives:
   * re-filtering on the text a press just inserted would collapse it to that one entry
   * and the press after it would have nowhere to go.
   */
  let cycle: {
    kind: TriggerKind;
    start: number;
    end: number;
    typed: string;
    index: number | null;
  } | null = null;
  /** A Tab pressed while the answer was still in flight, honoured when it lands. */
  let queuedStep: 1 | -1 | null = null;
  /** Where Escape put the row away; it stays away until a different escape character. */
  let dismissedAt: number | null = null;
  /**
   * The answers already given about the name being typed *now*, so that a backspace is
   * not a round trip through the worker (§6.13).
   *
   * Deliberately no bigger than one name: it is emptied the moment the trigger moves to
   * a different `\`, which is what makes it impossible for it to answer with a table
   * that predates a definition the document has since gained — while the caret sits in
   * one name, the only thing changing in the document is that name.
   *
   * It is worth having because the worker is shared with the conversion, and on a 200 KB
   * document a completion issued just after the debounce fired waits about a quarter of a
   * second behind it. What it cannot do is make *new* prefixes faster: a letter nobody
   * has typed yet is a question nobody has asked yet. That is measured in §6.13, and it
   * is why the answer to a slow document is not a second wasm instance.
   */
  const answers = new Map<string, Completion[]>();
  let answersFor: number | null = null;
  /** The caret this pane set itself, so that its own selection change is not a move. */
  let expectedCaret = -1;
  /** True while the pane is editing the buffer itself; see {@link replaceRange}. */
  let applying = false;

  function hideRow(): void {
    completionRow.hidden = true;
    completionRow.classList.remove('is-pending');
    completionChips.replaceChildren();
    completionStatus.textContent = '';
    chips = [];
    candidates = [];
    cycle = null;
    pendingQuery = null;
    queuedStep = null;
  }

  /** One chip: the name, what it renders as if that is a fixed thing, and its source. */
  function chipFor(item: Completion, index: number): HTMLButtonElement {
    const chip = el('button', 'completion-chip');
    chip.type = 'button';
    // Not a tab stop: Tab belongs to the cycle while the row is up, and a row of five
    // more tab stops would be five more places for a keyboard user to be surprised.
    chip.tabIndex = -1;
    const spelling = trigger?.kind === 'environment' ? item.name : `\\${item.name}`;
    chip.append(el('span', 'completion-chip-name', spelling));
    if (item.replacement !== null && item.replacement !== '') {
      chip.append(el('span', 'completion-chip-replacement', item.replacement));
    }
    if (item.fromDocument) {
      chip.classList.add('is-from-document');
      chip.append(el('span', 'sr-only', ' — defined in this document'));
      chip.title = `${spelling} — defined in this document`;
    } else {
      chip.title = spelling;
    }
    // Taking the pointer without taking the focus: a chip that blurred the textarea
    // would close the phone's keyboard to insert three characters into it.
    chip.addEventListener('pointerdown', (event) => event.preventDefault());
    chip.addEventListener('mousedown', (event) => event.preventDefault());
    chip.addEventListener('click', () => applyChip(index));
    return chip;
  }

  function renderChips(): void {
    chips = candidates.map((item, index) => chipFor(item, index));
    completionChips.replaceChildren(...chips);
    completionRow.classList.remove('is-pending');
    completionRow.hidden = false;
    markChips();
  }

  /** The highlight follows the cycle, so that a third Tab is a visible act. */
  function markChips(): void {
    const current = cycle?.index ?? null;
    for (const [index, chip] of chips.entries()) {
      const on = index === current;
      chip.classList.toggle('is-current', on);
      chip.setAttribute('aria-pressed', String(on));
    }
  }

  /**
   * Ask what the caret is asking, if anything, and put the row in the state that
   * answers it. Called after every keystroke; never during a cycle, whose list is
   * frozen by design.
   */
  function refreshCompletions(): void {
    const found = completionTrigger(input.value, input.selectionStart, input.selectionEnd);
    trigger = found;
    if (!found) {
      dismissedAt = null;
      hideRow();
      return;
    }
    if (dismissedAt !== null && dismissedAt !== found.start) dismissedAt = null;
    if (dismissedAt !== null) {
      hideRow();
      return;
    }
    if (pendingQuery && pendingQuery.kind === found.kind && pendingQuery.prefix === found.prefix) {
      return;
    }
    if (answersFor !== found.start) {
      answers.clear();
      answersFor = found.start;
    }
    const remembered = answers.get(`${found.kind}\u0000${found.prefix}`);
    if (remembered) {
      pendingQuery = null;
      show(found, remembered);
      return;
    }
    const query: CompletionQuery = {
      kind: found.kind,
      prefix: found.prefix,
      limit: found.kind === 'environment' ? ENVIRONMENT_QUERY_LIMIT : MACRO_QUERY_LIMIT,
    };
    pendingQuery = query;
    // The chips already up stay up, dimmed, until the new answer replaces them: the
    // round trip is a millisecond, and blanking the row on every keystroke would make
    // the pane jump under the hands of anyone typing a long macro name.
    if (!completionRow.hidden) completionRow.classList.add('is-pending');
    init.onCompletionQuery(query);
  }

  /** Put an answer on the screen, or take the row away if it turns out to be empty. */
  function show(asked: CompletionTrigger, items: readonly Completion[]): void {
    candidates = candidatesFor(items, asked.kind, asked.prefix, CHIP_CAP);
    if (candidates.length === 0) {
      hideRow();
      return;
    }
    renderChips();
    // A Tab pressed while the answer was in flight: the user asked for the first
    // candidate before there was one, and now there is.
    if (queuedStep !== null) {
      const direction = queuedStep;
      queuedStep = null;
      step(direction);
    }
  }

  /** Tab, or Shift-Tab: one position around the ring of candidates and typed text. */
  function step(direction: 1 | -1): void {
    // Mid-flight: the row is showing yesterday's chips and applying one of them would
    // insert something the prefix no longer matches. Remember the press instead — the
    // answer is milliseconds away — rather than let the focus escape the textarea.
    if (pendingQuery !== null) {
      queuedStep = direction;
      return;
    }
    if (!trigger || candidates.length === 0) return;
    if (!cycle) {
      cycle = {
        kind: trigger.kind,
        start: trigger.start,
        end: trigger.end,
        typed: trigger.prefix,
        index: null,
      };
    }
    applyIndex(nextInCycle(cycle.index, candidates.length, direction));
  }

  /**
   * Replace `[start, end)` with `name` in a way the browser's own undo can see.
   *
   * `execCommand('insertText')` is deprecated and is used anyway, because it is the only
   * way a script can edit a textarea and leave Ctrl+Z working: `setRangeText` and an
   * assignment to `value` both drop the undo stack on the floor, so a Tab would cost the
   * user every keystroke they had typed before it. Where it is refused — it returns
   * `false` rather than throwing — the assignment is the fallback, and Shift-Tab back
   * through the cycle is then the only undo there is (§6.13).
   *
   * It fires an `input` event of its own, which `applying` tells the handler to leave
   * alone: this edit does its own bookkeeping below, and is not a keystroke that should
   * end the cycle it is part of.
   */
  function replaceRange(start: number, end: number, name: string): void {
    applying = true;
    try {
      input.focus({ preventScroll: true });
      input.setSelectionRange(start, end);
      let inserted = false;
      try {
        inserted = document.execCommand('insertText', false, name);
      } catch {
        inserted = false;
      }
      if (!inserted) input.setRangeText(name, start, end, 'end');
    } finally {
      applying = false;
    }
  }

  /** Put candidate `index` — or, for `null`, the user's own text — into the buffer. */
  function applyIndex(index: number | null): void {
    if (!cycle) return;
    const name = index === null ? cycle.typed : candidates[index]?.name;
    if (name === undefined) return;
    const caret = cycle.start + name.length;
    replaceRange(cycle.start, cycle.end, name);
    cycle.end = caret;
    cycle.index = index;
    expectedCaret = caret;
    trigger = trigger ? { ...trigger, end: caret } : null;

    // Everything the `input` handler does for a keystroke, because this *is* an edit —
    // one the handler was told to ignore, since it must not end the cycle it belongs to.
    remapPaintDiagnostics(paintDiagnosticsText, input.value);
    paintDiagnosticsText = input.value;
    syncBackdrop();
    scheduleRelayoutDiagnostics();
    init.onInput(input.value, 'type');

    markChips();
    // The chips are visual; this is the same news for a screen reader.
    completionStatus.textContent =
      index === null
        ? `${cycle.typed}, as you typed it`
        : cycle.kind === 'environment'
          ? name
          : `\\${name}`;
  }

  /** A click or a tap: that chip, and the cycle is over — this was a choice. */
  function applyChip(index: number): void {
    if (!trigger) return;
    if (!cycle) {
      cycle = {
        kind: trigger.kind,
        start: trigger.start,
        end: trigger.end,
        typed: trigger.prefix,
        index: null,
      };
    }
    const start = cycle.start;
    applyIndex(index);
    dismissedAt = start;
    hideRow();
    input.focus({ preventScroll: true });
  }

  function endCycle(): void {
    if (!cycle) return;
    cycle = null;
    markChips();
  }

  /* ------------------------------------------------------- the input events */

  input.addEventListener('input', (event) => {
    // The cycle's own edit, which announces itself through `execCommand`: it has already
    // done everything below, and it must not be read as the keystroke that ends it.
    if (applying) return;
    const inputType = (event as InputEvent).inputType ?? '';
    const bulk = inputType.startsWith('insertFromPaste') || inputType === 'insertFromDrop';
    // A keystroke is the one thing that ends a cycle *and* may start the next row: the
    // text the cycle inserted is now just text, and what is under the caret now is a
    // fresh question (§6.13).
    endCycle();
    // The diagnostics themselves are stale until the next result arrives — same as
    // the panel above — but their *offsets* still have to track every keystroke, or
    // an edit before a span paints the highlight over the wrong characters until
    // that result lands (§7). Debouncing that redraw (as `scheduleRelayoutDiagnostics`
    // does for the gutter, below) would defeat the point: a fast typist never leaves
    // an 80ms gap for it to fire, so the backdrop would sit frozen — stale text,
    // stale wrapping, stale everything — for the whole burst. So this part runs now.
    remapPaintDiagnostics(paintDiagnosticsText, input.value);
    paintDiagnosticsText = input.value;
    // A paste is not a keystroke. It can change the document's height by a factor, and
    // the window is placed from a cached geometry that would then be a different document
    // old — so this one edit pays for the layout a keystroke is not allowed to force.
    if (bulk) readGeometry();
    syncBackdrop();
    init.onInput(input.value, bulk ? 'paste' : 'type');
    // Only the gutter still waits: it measures in a throwaway mirror, which forces a
    // layout, so it stays debounced rather than paying for that on every keystroke.
    scheduleRelayoutDiagnostics();
    refreshCompletions();
  });

  input.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      init.onConvertNow();
      return;
    }
    // Enter, space and everything else are never intercepted: the row hangs off Tab
    // precisely so that the user's newlines stay their own (§6.13).
    if (event.key === 'Escape' && !completionRow.hidden) {
      // "Stop bothering me", not "undo": whatever the cycle applied stays where it is,
      // and the row does not come back until the next escape character.
      event.preventDefault();
      dismissedAt = trigger?.start ?? null;
      endCycle();
      hideRow();
      return;
    }
    if (event.key !== 'Tab' || event.altKey || event.ctrlKey || event.metaKey) return;
    // The obligation this rule exists for: while there is no row, Tab moves focus out
    // of the textarea exactly as it always did. A keyboard-only user who could not
    // leave the editor would be trapped in it (§6.9, §6.13).
    if (completionRow.hidden) return;
    event.preventDefault();
    step(event.shiftKey ? -1 : 1);
  });

  /* ------------------------------------------------------- composition (§6.12) */

  input.addEventListener('compositionstart', () => {
    composing = true;
    editorArea.classList.add('is-composing');
    syncBackdrop();
    hideRow();
  });

  input.addEventListener('compositionend', () => {
    composing = false;
    editorArea.classList.remove('is-composing');
    syncBackdrop();
  });

  /* ----------------------------------------------- the caret, and losing focus */

  /**
   * A cursor move ends the cycle (§6.13). The caret this pane put there itself does
   * not: applying a candidate moves the selection as a matter of course, and a cycle
   * that ended on its own first step would be a cycle of one.
   */
  function onCaretMoved(): void {
    if (input.selectionStart === expectedCaret && input.selectionEnd === expectedCaret) return;
    if (cycle) {
      endCycle();
      hideRow();
    }
  }

  document.addEventListener('selectionchange', () => {
    if (document.activeElement !== input) return;
    onCaretMoved();
  });
  // Safari has been unreliable about `selectionchange` for form controls, and a cycle
  // that outlived an arrow key would apply its next candidate somewhere else entirely.
  input.addEventListener('keyup', (event) => {
    if (event.key.startsWith('Arrow') || event.key === 'Home' || event.key === 'End') {
      onCaretMoved();
    }
  });
  input.addEventListener('pointerup', onCaretMoved);

  input.addEventListener('blur', () => {
    // A chip takes the pointer without taking the focus (see `chipFor`), so a blur here
    // really is the user leaving the editor.
    endCycle();
    hideRow();
  });

  /* ----------------------------------------------- the on-screen keyboard (§6.6) */

  const viewport = window.visualViewport;
  if (viewport) {
    let pending = 0;
    let keyboardUp = false;
    const sync = (): void => {
      pending = 0;
      document.documentElement.style.setProperty('--vvh', `${Math.round(viewport.height)}px`);
      const up = window.innerHeight - viewport.height > KEYBOARD_THRESHOLD;
      if (up === keyboardUp) return;
      keyboardUp = up;
      document.documentElement.classList.toggle('kbd-up', up);
      // Once, on the way up: put the app's top edge at the top of the layout
      // viewport so the primary bar and the Copy button stay reachable.
      if (up) {
        const host = mount.closest('.app') ?? mount;
        window.scrollTo({ top: host.getBoundingClientRect().top + window.scrollY });
      }
      scheduleMeasure();
    };
    const onViewport = (): void => {
      if (!pending) pending = requestAnimationFrame(sync);
    };
    viewport.addEventListener('resize', onViewport);
    viewport.addEventListener('scroll', onViewport);
    sync();
  }

  /* --------------------------------------------------------------- the handle */

  return {
    input,

    getDocument: () => input.value,

    setDocument(value: string) {
      input.value = value;
      input.scrollTop = 0;
      // A whole new document invalidates the last diagnostics outright — unlike a
      // typed edit, this is not "stale until the next result", it is a different
      // document, and the old spans would light up unrelated text.
      paintDiagnostics = [];
      paintDiagnosticsText = value;
      // The window is carried along by an edit, but this is not an edit: a different
      // document has to be looked at again rather than coloured where the last one was
      // being read (§6.12).
      windowFrom = 0;
      windowTo = 0;
      paintedText = value;
      // The row was about a name in the document that has just been replaced.
      trigger = null;
      dismissedAt = null;
      answers.clear();
      answersFor = null;
      hideRow();
      relayoutDiagnostics();
    },

    setCompletions(query: CompletionQuery, items: readonly Completion[]) {
      // The query object is the token: an answer to anything but the question the row
      // is waiting for is an answer to a keystroke that has been typed over (§6.2).
      if (query !== pendingQuery) return;
      pendingQuery = null;
      const asked = trigger;
      if (!asked) {
        hideRow();
        return;
      }
      answers.set(`${query.kind}\u0000${query.prefix}`, items.slice());
      show(asked, items);
    },

    setOutput(value: string) {
      outputText = value;
      output.textContent = value;
    },

    getOutput: () => outputText,

    /**
     * The one thing that is ever put in the output pane besides text: an element per
     * formula, wrapped around the source that is already there (§6.3).
     *
     * Every node is built here, from a slice of `outputText` — no markup is parsed, no
     * `innerHTML` is assigned, and `outputText` itself is not touched, so Copy,
     * Download and the library are unaffected by anything that happens to these
     * elements afterwards. The caller gets them back to hand to a typesetter; until it
     * does, each one still reads as the LaTeX it wraps, which is the readable state
     * the pane sits in while MathJax loads.
     */
    markMath(regions: readonly MathRegion[]): HTMLElement[] {
      const elements: HTMLElement[] = [];
      const nodes: Node[] = [];
      for (const run of splitMathRuns(outputText, regions)) {
        if (!run.math) {
          nodes.push(document.createTextNode(run.text));
          continue;
        }
        const span = el('span', run.math.display ? 'math math-display' : 'math math-inline');
        span.append(document.createTextNode(run.text));
        nodes.push(span);
        elements.push(span);
      }
      output.replaceChildren(...nodes);
      return elements;
    },

    /**
     * Select `[start, end)` and make it visible. A textarea does not scroll a
     * programmatic selection into view, so the position is measured in a mirror that
     * carries the textarea's own metrics and wrapping, and `scrollTop` is set from it.
     */
    selectSpan(start: number, end: number) {
      // A hidden textarea cannot take focus: leave the mobile focus mode first.
      if (focus === 'output') {
        focus = null;
        applyFocus();
      }
      const length = input.value.length;
      const from = clamp(Math.trunc(start), 0, length);
      const to = clamp(Math.trunc(end), from, length);
      input.focus({ preventScroll: true });
      input.setSelectionRange(from, to);
      const place = caretPosition(input, from);
      if (!place) return;
      const view = input.clientHeight;
      const above = place.top < input.scrollTop;
      const below = place.top + place.height > input.scrollTop + view;
      if (above || below) {
        input.scrollTop = Math.max(0, place.top - Math.max(0, view - place.height) / 2);
      }
      input.scrollIntoView({ block: 'nearest' });
    },

    setDiagnostics(diagnostics: readonly Diagnostic[]) {
      paintDiagnostics = diagnostics.filter(
        (d): d is Diagnostic & { span: Span } =>
          d.span !== null && (d.severity === 'error' || d.severity === 'warning'),
      );
      // The new offsets are authoritative for whatever the document is right now —
      // this replaces anything `remapPaintDiagnostics` had been approximating.
      paintDiagnosticsText = input.value;
      relayoutDiagnostics();
    },

    columns: () => (columnsValue > 0 ? columnsValue : measureQuietly() || MIN_FIT_COLUMNS),

    setSoftWrap(enabled: boolean) {
      output.classList.toggle('is-soft-wrapped', enabled);
    },

    async setFont(font: FontId, size: number) {
      fontId = font;
      fontSize = size;
      await applyFont(font, size);
      // The face may only now be available: the advance measured before it arrived
      // was the fallback's.
      advanceCache.delete(`${font}|${size}`);
      measure();
    },

    setSplit(value: number) {
      split = clamp(value, MIN_SPLIT, MAX_SPLIT);
      applySplit();
      scheduleMeasure();
    },

    setFocus(value: 'input' | 'output' | null) {
      focus = value;
      applyFocus();
      scheduleMeasure();
    },

    setFontLoading(loading: boolean) {
      fontNote.hidden = !loading;
    },

    setEntryState(state) {
      // `null` is a browser with nowhere to keep a library: a control that cannot do
      // anything is worse than no control (§6.10).
      saveButton.hidden = state === null;
      starButton.hidden = state === null;
      entryChip.hidden = state === null;
      if (!state) return;

      entryLabel.textContent = state.label;
      entryChip.title = state.hint;
      entryChip.setAttribute('aria-label', state.hint);
      entryChip.dataset.state = state.id === null ? 'new' : state.sealed ? 'sealed' : 'open';
      // ● while the entry is taking the edits, ✓ once it has been sealed and is being
      // kept as it is — the two states the header exists to tell apart.
      entryGlyph.textContent = state.id === null ? '+' : state.sealed ? '✓' : '●';
      // Nothing to show yet, and nothing to open: an empty document has no entry.
      entryChip.disabled = state.id === null;

      starGlyph.textContent = state.starred ? '★' : '☆';
      starButton.classList.toggle('is-starred', state.starred);
      starButton.setAttribute('aria-pressed', String(state.starred));
      starButton.setAttribute('aria-label', state.starred ? 'Starred' : 'Star this version');
      starButton.title = state.starred
        ? 'Starred — remove the star'
        : 'Keep this version and star it — starred entries are never removed automatically';
      // Sealed already: Save has nothing left to do to this one, and saying so is
      // better than a button that answers a click with nothing visible.
      saveButton.disabled = state.sealed;
      saveButton.title = state.sealed
        ? 'This version is kept — further edits start a new entry'
        : 'Keep this version — further edits start a new entry';
    },

    setStale(stale: boolean) {
      output.classList.toggle('is-stale', stale);
      staleNote.hidden = !stale;
    },
  };
}

/* ---------------------------------------------------------------- helpers */

function clamp(value: number, low: number, high: number): number {
  if (!Number.isFinite(value)) return low;
  return Math.min(high, Math.max(low, value));
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

/**
 * A button's word, in a span of its own.
 *
 * Below 620 px the pane headers hide these (§6.6): four labelled buttons and a title
 * do not fit a phone, and the hiding is the `.sr-only` treatment rather than
 * `display: none`, so the button keeps its accessible name and its 44 px target.
 */
function label(value: string): HTMLSpanElement {
  return el('span', 'btn-label', value);
}

function caret(glyph: string): HTMLSpanElement {
  const node = el('span', 'glyph', glyph);
  node.setAttribute('aria-hidden', 'true');
  return node;
}

const SVG_NS = 'http://www.w3.org/2000/svg';

/** A tiny stroked icon, built element by element — no icon library, no innerHTML. */
function svgIcon(...shapes: [string, Record<string, string>][]): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('viewBox', '0 0 16 16');
  svg.setAttribute('width', '15');
  svg.setAttribute('height', '15');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.4');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('focusable', 'false');
  svg.classList.add('icon');
  for (const [tag, attrs] of shapes) {
    const shape = document.createElementNS(SVG_NS, tag);
    for (const [name, value] of Object.entries(attrs)) shape.setAttribute(name, value);
    svg.append(shape);
  }
  return svg;
}

/**
 * An invisible `<div>` given the textarea's box and text metrics, so a marker span
 * placed inside it reports the same `offsetTop` a character at that position would
 * scroll to in the real textarea — `offsetTop` is measured from the padding edge,
 * exactly as `scrollTop` is, so the two are directly comparable. Soft wrapping is
 * reproduced by `white-space: pre-wrap` at the same content width, so a wrapped line
 * counts as the several visual rows it really occupies. Caller fills it in, measures,
 * and removes it — the mirror carries no state of its own between calls.
 */
function buildMirror(area: HTMLTextAreaElement): HTMLDivElement {
  const style = getComputedStyle(area);
  const mirror = document.createElement('div');
  const copy = [
    'font-family',
    'font-size',
    'font-weight',
    'font-style',
    'font-variant',
    'font-feature-settings',
    // Copied for the same reason `styles.css` sets it on `.pane-input`: a `<textarea>`
    // does not inherit it, and a `<div>` dropped into the page does.
    'font-variation-settings',
    'letter-spacing',
    'word-spacing',
    'line-height',
    'text-transform',
    'text-indent',
    'tab-size',
    'padding-top',
    'padding-right',
    'padding-bottom',
    'padding-left',
    'border-top-width',
    'border-right-width',
    'border-bottom-width',
    'border-left-width',
    'word-break',
  ];
  for (const prop of copy) mirror.style.setProperty(prop, style.getPropertyValue(prop));
  mirror.style.setProperty('box-sizing', 'content-box');
  // The content width, measured rather than declared. Neither obvious way of asking
  // gives it: the *resolved* `width` is the used value of the `width` property, which
  // under this app's `box-sizing: border-box` is the border box, and it says nothing
  // about the scrollbar; `clientWidth` is the padding box, already less whatever a
  // classic scrollbar or a reserved gutter has taken out of it. So it is `clientWidth`
  // minus the padding — which is also why this needs no opinion about
  // `scrollbar-gutter` (§6.12) beyond letting the textarea answer for itself.
  const padding =
    parseFloat(style.getPropertyValue('padding-left') || '0') +
    parseFloat(style.getPropertyValue('padding-right') || '0');
  mirror.style.setProperty('width', `${Math.max(0, area.clientWidth - padding)}px`);
  mirror.style.setProperty('height', 'auto');
  mirror.style.setProperty('position', 'absolute');
  mirror.style.setProperty('top', '0');
  mirror.style.setProperty('left', '-10000px');
  mirror.style.setProperty('visibility', 'hidden');
  mirror.style.setProperty('white-space', 'pre-wrap');
  mirror.style.setProperty('overflow-wrap', 'break-word');
  mirror.style.setProperty('pointer-events', 'none');
  return mirror;
}

/** Where the character at `index` sits inside the textarea's scrollable content. */
function caretPosition(
  area: HTMLTextAreaElement,
  index: number,
): { top: number; height: number } | null {
  const mirror = buildMirror(area);
  mirror.textContent = area.value.slice(0, index);
  const marker = document.createElement('span');
  marker.textContent = area.value.slice(index, index + 1) || '.';
  mirror.append(marker);
  document.body.append(mirror);
  const top = marker.offsetTop;
  const height = marker.offsetHeight;
  mirror.remove();
  if (!Number.isFinite(top)) return null;
  return { top, height };
}

/**
 * {@link caretPosition}'s `top`, for several indices at once — one mirror, one
 * reflow, however many gutter markers there are, rather than one of each per marker.
 */
function caretOffsets(area: HTMLTextAreaElement, indices: readonly number[]): number[] {
  if (indices.length === 0) return [];
  const text = area.value;
  const mirror = buildMirror(area);
  const order = indices
    .map((index, position) => ({ index: clamp(index, 0, text.length), position }))
    .sort((a, b) => a.index - b.index);
  const markers: HTMLSpanElement[] = new Array(indices.length);
  let cursor = 0;
  for (const { index, position } of order) {
    if (index > cursor) mirror.append(document.createTextNode(text.slice(cursor, index)));
    const marker = document.createElement('span');
    marker.textContent = text.slice(index, index + 1) || '.';
    mirror.append(marker);
    markers[position] = marker;
    cursor = Math.max(cursor, index + 1);
  }
  document.body.append(mirror);
  const offsets = markers.map((marker) => marker.offsetTop);
  mirror.remove();
  return offsets;
}
