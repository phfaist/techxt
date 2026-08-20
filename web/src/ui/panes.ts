/**
 * The two panes: a textarea for the source, a `<pre>` for the answer (web/PLAN.md
 * §6.1, §6.3, §6.5, §6.6).
 *
 * Three properties of this module are load-bearing rather than decorative:
 *
 * - The output is `white-space: pre` and is written with `textContent`. The library
 *   decided the line breaks; a second, invisible wrapping by the browser would
 *   misrepresent the output, so *Wrap: Off* scrolls horizontally instead (§6.3).
 * - The fit-to-pane measurement (§6.5) is what makes *Wrap: Fit* mean anything: the
 *   pane's width in pixels becomes a column count the library can wrap to.
 * - The textarea turns off every helpful thing a phone does to prose. An editor that
 *   capitalises `\alpha` is worse than useless (§6.6).
 *
 * Nothing here knows what a conversion is. Every user action leaves through a
 * callback of {@link PanesInit}; every state change the app makes arrives through a
 * method of {@link Panes} and fires nothing.
 */

import { applyFont } from '../fonts';
import type { FontId } from '../fonts';
import { MIN_FIT_COLUMNS, columnsFor } from '../state';
import type { ExampleDoc } from '../types';
import type { Panes, PanesInit } from './api';

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

  const inFocus = focusButton('input');
  const inTools = el('div', 'pane-tools');
  inTools.append(menuWrap, inFocus);

  const inHead = el('header', 'pane-head');
  inHead.append(inTitle, inTools);

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

  inPane.append(inHead, inputLabel, input);

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

  const outFocus = focusButton('output');
  const outTools = el('div', 'pane-tools');
  outTools.append(copyButton, downloadButton, outFocus);

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
  requestAnimationFrame(measure);

  /* ------------------------------------------------------- the input events */

  input.addEventListener('input', (event) => {
    const inputType = (event as InputEvent).inputType ?? '';
    const bulk = inputType.startsWith('insertFromPaste') || inputType === 'insertFromDrop';
    init.onInput(input.value, bulk ? 'paste' : 'type');
  });

  input.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      init.onConvertNow();
    }
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
    },

    setOutput(value: string) {
      outputText = value;
      output.textContent = value;
    },

    getOutput: () => outputText,

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

    columns: () => (columnsValue > 0 ? columnsValue : measureQuietly() || MIN_FIT_COLUMNS),

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
 * Below 620 px the pane headers hide these (§6.6): three labelled buttons and a title
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
 * Where the character at `index` sits inside the textarea's scrollable content.
 *
 * A mirror `<div>` is given the textarea's box and text metrics and filled with the
 * text up to `index`; a marker span then reports its own offset, which — because
 * `offsetTop` is measured from the mirror's padding edge, exactly as `scrollTop` is —
 * can be compared with `scrollTop` directly. Soft wrapping is reproduced by
 * `white-space: pre-wrap` at the same content width, so a wrapped line counts as the
 * several visual rows it really occupies.
 */
function caretPosition(
  area: HTMLTextAreaElement,
  index: number,
): { top: number; height: number } | null {
  const style = getComputedStyle(area);
  const mirror = document.createElement('div');
  const copy = [
    'font-family',
    'font-size',
    'font-weight',
    'font-style',
    'font-variant',
    'font-feature-settings',
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
  // The resolved `width` of an element is its content box, which is what the mirror
  // needs once its own padding and border are set from the same source.
  mirror.style.setProperty('width', style.getPropertyValue('width'));
  mirror.style.setProperty('height', 'auto');
  mirror.style.setProperty('position', 'absolute');
  mirror.style.setProperty('top', '0');
  mirror.style.setProperty('left', '-10000px');
  mirror.style.setProperty('visibility', 'hidden');
  mirror.style.setProperty('white-space', 'pre-wrap');
  mirror.style.setProperty('overflow-wrap', 'break-word');
  mirror.style.setProperty('pointer-events', 'none');

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
