/**
 * The primary bar and the "More options" disclosure (web/PLAN.md §5, §6.9).
 *
 * Two tiers, because three controls change the answer far more often than the other
 * thirteen: wrapping, math rendering and the display font live in a sticky bar that is
 * always visible, and everything else lives behind a `<details>`.
 *
 * Three rules run through the whole module:
 *
 * - **A default is not a choice.** A control sitting at the library's own default is
 *   deleted from the emitted {@link AppOptions} rather than written into it, which is
 *   what keeps a share link short and lets a future change of a library default be
 *   picked up rather than frozen (§6.4). The field is drawn as *unset* too.
 * - **Display font ≠ char styles.** `textFont` and `mathFont` choose the Unicode
 *   *alphabet* a letter is mapped into — they change the text. The display font is
 *   CSS and changes nothing. The labels and hints keep them apart (§5).
 * - **Every control is labelled** with a real `<label for>`; nothing here uses a
 *   placeholder as a label (§6.9).
 */

import { FONTS, MAX_SIZE, MIN_SIZE } from '../fonts';
import type { FontId } from '../fonts';
import { WRAP_PRESETS } from '../types';
import type { AppOptions, TodayMode, WrapMode } from '../types';
import type { Controls, ControlsInit } from './api';

/** The library options the panel exposes — everything in `AppOptions` but the three app-level keys. */
type LibKey = Exclude<keyof AppOptions, 'wrap' | 'todayMode' | 'todayCustom'>;

/**
 * The library's own defaults (Appendix A of the plan). A value equal to one of these
 * is never emitted: absent *is* the default, and re-typing it here would freeze it.
 */
const DEFAULTS: Record<LibKey, string | boolean> = {
  mathMode: 'fancy',
  mathExpressionIn: 'parens',
  matrixDelimiters: 'unicode',
  keepComments: false,
  headingStyle: 'numbered-underlined',
  footnoteStyle: 'collected',
  textFont: 'default',
  mathFont: 'italic',
  unknownMacro: 'skip',
  unknownEnv: 'render-body',
  unknownSpecials: 'emit-chars',
  recovery: 'tolerant',
};

const DEFAULT_CUSTOM_COLUMNS = 72;
const MIN_CUSTOM_COLUMNS = 20;
const MAX_CUSTOM_COLUMNS = 400;

interface Choice {
  value: string;
  label: string;
}

export function initControls(init: ControlsInit): Controls {
  const { mount } = init;
  mount.classList.add('controls-host');

  /* ----------------------------------------------------------------- state */

  let font: FontId = init.ui.font;
  let size = init.ui.size;
  let customColumns = DEFAULT_CUSTOM_COLUMNS;
  let customToday = init.options.todayCustom ?? '';
  /**
   * The open state the app itself last set. The `toggle` event of `<details>` is
   * queued asynchronously, so a flag raised around the assignment is already lowered
   * by the time the event runs; comparing against this instead is what keeps a
   * programmatic open from being reported as a user action.
   */
  let lastOpen = init.ui.moreOpen;

  const selects = new Map<LibKey, HTMLSelectElement>();
  const flags: { node: HTMLElement; changed: () => boolean }[] = [];

  /* ------------------------------------------------------------------- DOM */

  const root = el('div', 'controls');
  const bar = el('div', 'bar');
  bar.setAttribute('role', 'group');
  bar.setAttribute('aria-label', 'Conversion options');

  /* --- Wrap */

  const wrapChoices: Choice[] = [
    { value: 'fit', label: 'Fit the pane' },
    { value: 'off', label: 'Off (default)' },
    ...WRAP_PRESETS.map((n) => ({ value: String(n), label: `${n} columns` })),
    { value: 'custom', label: 'Custom…' },
  ];
  const wrapSelect = select('opt-wrap', wrapChoices);
  const wrapField = field('opt-wrap', 'Wrap', wrapSelect, {
    hint: 'Fit measures the pane and wraps to it; Off is the library’s own default, which is no wrapping at all.',
    className: 'field-wrap',
    bar: true,
  });
  const wrapCustom = el('input', 'control control-number');
  wrapCustom.type = 'number';
  wrapCustom.id = 'opt-wrap-columns';
  wrapCustom.min = String(MIN_CUSTOM_COLUMNS);
  wrapCustom.max = String(MAX_CUSTOM_COLUMNS);
  wrapCustom.step = '1';
  wrapCustom.hidden = true;
  const wrapCustomLabel = el('label', 'sr-only', 'Wrap at how many columns');
  wrapCustomLabel.htmlFor = wrapCustom.id;
  wrapField.insertBefore(wrapCustomLabel, wrapField.querySelector('.hint'));
  wrapField.insertBefore(wrapCustom, wrapField.querySelector('.hint'));
  track(wrapField, () => readWrap() !== 'off');

  /* --- Math */

  const mathSelect = registerSelect('mathMode', 'opt-math', [
    { value: 'fancy', label: 'Fancy (default)' },
    { value: 'plain', label: 'Plain' },
    { value: 'source', label: 'Source' },
  ]);
  const mathField = field('opt-math', 'Math', mathSelect, {
    hint: 'Fancy spaces a formula the way a typesetter would; Plain runs the pieces together; Source leaves the LaTeX.',
    className: 'field-math',
    bar: true,
  });

  /* --- Display font */

  const fontSelect = select(
    'opt-font',
    FONTS.map((entry) => ({ value: entry.id, label: entry.label })),
  );
  const sizeInput = el('input', 'control control-range');
  sizeInput.type = 'range';
  sizeInput.id = 'opt-font-size';
  sizeInput.min = String(MIN_SIZE);
  sizeInput.max = String(MAX_SIZE);
  sizeInput.step = '1';
  const sizeLabel = el('label', 'sr-only', 'Display font size, in pixels');
  sizeLabel.htmlFor = sizeInput.id;
  const sizeOut = el('output', 'size-out');
  sizeOut.setAttribute('for', sizeInput.id);

  const fontRow = el('div', 'font-row');
  fontRow.append(fontSelect, sizeLabel, sizeInput, sizeOut);
  const fontField = field('opt-font', 'Display font', fontRow, {
    hint: 'CSS only — how the converted text is drawn, never what it says.',
    className: 'field-font',
    bar: true,
  });

  /* --- More options */

  const more = el('details', 'more');
  const summary = el('summary', 'btn more-summary');
  summary.append(glyph('▸'), document.createTextNode('More options'));
  const moreBody = el('div', 'more-body');
  const moreGrid = el('div', 'more-grid');

  /* Layout */
  const layout = group('Layout');
  layout.append(
    field(
      'opt-heading',
      'Heading style',
      registerSelect('headingStyle', 'opt-heading', [
        { value: 'numbered-underlined', label: 'Number, title, underline (default)' },
        { value: 'underlined', label: 'Title and underline' },
        { value: 'prefix', label: 'Number and title on one line' },
        { value: 'plain', label: 'Title alone' },
      ]),
    ),
    field(
      'opt-footnote',
      'Footnote style',
      registerSelect('footnoteStyle', 'opt-footnote', [
        { value: 'collected', label: 'Collected at the end (default)' },
        { value: 'inline', label: 'Inline, where it was written' },
        { value: 'skip', label: 'Dropped entirely' },
      ]),
    ),
    checkField('opt-comments', 'Keep comments', 'A % comment becomes a line of its own.'),
    field(
      'opt-text-font',
      'Text char styles',
      registerSelect('textFont', 'opt-text-font', [
        { value: 'default', label: 'On — \\textbf gives 𝐛𝐨𝐥𝐝 (default)' },
        { value: 'off', label: 'Off — letters stay as typed' },
      ]),
      { hint: 'Which Unicode alphabet text is mapped into. Not the display font.' },
    ),
    field(
      'opt-today',
      '\\today renders as',
      todaySelect(),
      { hint: 'The library has no clock; this browser does.' },
    ),
    checkField(
      'opt-offline',
      'Keep all fonts offline',
      'Fetches all five display faces now, so they work with the network off.',
    ),
  );

  /* Math */
  const math = group('Math');
  math.append(
    field(
      'opt-mathdelims',
      'Expression delimiters',
      registerSelect('mathExpressionIn', 'opt-mathdelims', [
        { value: 'parens', label: '( … ) parentheses (default)' },
        { value: 'braces', label: '{ … } braces' },
        { value: 'none', label: 'None — a space is the separation' },
      ]),
      { hint: 'What a sub-expression is wrapped in when its extent would be unclear.' },
    ),
    field(
      'opt-matrix',
      'Matrix delimiters',
      registerSelect('matrixDelimiters', 'opt-matrix', [
        { value: 'unicode', label: 'Unicode pieces, full height (default)' },
        { value: 'ascii', label: 'ASCII, repeated on every row' },
      ]),
    ),
    field('opt-math-font', 'Math char styles', mathFontSelect(), {
      hint: 'The Unicode alphabet variables are mapped into. Not the display font.',
    }),
  );

  /* Parsing */
  const parsing = group('Parsing');
  parsing.append(
    field(
      'opt-unknown-macro',
      'Unknown macros',
      registerSelect('unknownMacro', 'opt-unknown-macro', [
        { value: 'skip', label: 'Render nothing (default)' },
        { value: 'render-args', label: 'Render its arguments' },
        { value: 'keep-source', label: 'Keep the LaTeX source' },
        { value: 'placeholder', label: 'Show <name>' },
      ]),
    ),
    field(
      'opt-unknown-env',
      'Unknown environments',
      registerSelect('unknownEnv', 'opt-unknown-env', [
        { value: 'render-body', label: 'Render the body (default)' },
        { value: 'skip', label: 'Render nothing at all' },
        { value: 'keep-source', label: 'Keep the LaTeX source' },
      ]),
    ),
    field(
      'opt-unknown-specials',
      'Unknown specials',
      registerSelect('unknownSpecials', 'opt-unknown-specials', [
        { value: 'emit-chars', label: 'Emit the characters (default)' },
        { value: 'skip', label: 'Render nothing at all' },
      ]),
    ),
    checkField(
      'opt-strict',
      'Strict',
      'An unknown command becomes a hard parse error instead of a warning.',
    ),
  );

  moreGrid.append(layout, math, parsing);

  // The panel's one line of chrome: which techxt these options are being given to.
  const versionLine = el('p', 'more-note');
  const versionCode = el('code', 'more-version', init.version || '—');
  versionLine.append(document.createTextNode('techxt '), versionCode);

  const moreFoot = el('div', 'more-foot');
  moreFoot.append(versionLine);
  moreBody.append(moreGrid, moreFoot);
  more.append(summary, moreBody);

  bar.append(wrapField, mathField, fontField, more);
  root.append(bar);
  mount.append(root);

  /* ---------------------------------------------------- the checkbox handles */

  const keepComments = checkbox('opt-comments');
  const keepOffline = checkbox('opt-offline');
  const strict = checkbox('opt-strict');
  const todaySel = mount.querySelector<HTMLSelectElement>('#opt-today')!;
  const todayInput = mount.querySelector<HTMLInputElement>('#opt-today-custom')!;

  track(bar.querySelector<HTMLElement>('#opt-comments')!.closest<HTMLElement>('.field')!, () => keepComments.checked);
  track(bar.querySelector<HTMLElement>('#opt-strict')!.closest<HTMLElement>('.field')!, () => strict.checked);
  track(bar.querySelector<HTMLElement>('#opt-offline')!.closest<HTMLElement>('.field')!, () => keepOffline.checked);
  track(todaySel.closest<HTMLElement>('.field')!, () => todaySel.value !== 'library');

  /* ------------------------------------------------------------- behaviour */

  function readWrap(): WrapMode {
    const value = wrapSelect.value;
    if (value === 'fit') return 'fit';
    if (value === 'off') return 'off';
    if (value === 'custom') {
      const typed = Math.round(Number(wrapCustom.value));
      if (Number.isFinite(typed) && typed >= MIN_CUSTOM_COLUMNS) {
        customColumns = Math.min(typed, MAX_CUSTOM_COLUMNS);
      }
      return customColumns;
    }
    const preset = Number(value);
    return Number.isFinite(preset) ? preset : 'off';
  }

  function writeWrap(wrap: WrapMode | undefined): void {
    // Absent means the *app's* default, not the library's: `DEFAULT_OPTIONS` in
    // state.ts is `{ wrap: 'fit', todayMode: 'browser' }`, and `resolveOptions` reads
    // an absent key the same way (§5). Both are emitted explicitly below, so this
    // only decides what an empty option object looks like.
    const value: WrapMode = wrap ?? 'fit';
    if (value === 'fit' || value === 'off') {
      wrapSelect.value = value;
    } else {
      customColumns = value;
      const preset = WRAP_PRESETS.includes(value);
      wrapSelect.value = preset ? String(value) : 'custom';
      wrapCustom.value = String(value);
    }
    wrapCustom.hidden = wrapSelect.value !== 'custom';
    if (!wrapCustom.value) wrapCustom.value = String(customColumns);
  }

  /** The whole option object, with every library default deleted rather than written. */
  function read(): AppOptions {
    const out: AppOptions = {};
    for (const [key, control] of selects) {
      if (control.value !== DEFAULTS[key]) set(out, key, control.value);
    }
    if (keepComments.checked !== DEFAULTS.keepComments) set(out, 'keepComments', keepComments.checked);
    if (strict.checked) set(out, 'recovery', 'strict');

    // The two app-level keys (§5) are always explicit: they have no library default to
    // fall back to, and `resolveOptions` reads them directly.
    out.wrap = readWrap();
    const mode = todaySel.value as TodayMode;
    out.todayMode = mode;
    if (mode === 'custom') {
      customToday = todayInput.value;
      out.todayCustom = customToday;
    }
    return out;
  }

  function refresh(): void {
    for (const flag of flags) flag.node.dataset.set = String(flag.changed());
    wrapCustom.hidden = wrapSelect.value !== 'custom';
    todayInput.hidden = todaySel.value !== 'custom';
    sizeOut.textContent = `${size} px`;
  }

  function emit(): void {
    refresh();
    init.onOptionsChange(read());
  }

  const emitSoon = debounce(emit, 300);

  for (const control of selects.values()) {
    control.addEventListener('change', emit);
  }
  wrapSelect.addEventListener('change', () => {
    if (wrapSelect.value === 'custom') {
      wrapCustom.hidden = false;
      if (!wrapCustom.value) wrapCustom.value = String(customColumns);
      wrapCustom.focus();
    }
    emit();
  });
  wrapCustom.addEventListener('input', emitSoon);
  wrapCustom.addEventListener('change', () => {
    emit();
    // Show what was actually accepted, so a rejected 5 does not sit there looking used.
    wrapCustom.value = String(customColumns);
  });
  keepComments.addEventListener('change', emit);
  strict.addEventListener('change', emit);
  todaySel.addEventListener('change', () => {
    if (todaySel.value === 'custom') {
      todayInput.hidden = false;
      todayInput.focus();
    }
    emit();
  });
  todayInput.addEventListener('input', emitSoon);
  todayInput.addEventListener('change', emit);

  keepOffline.addEventListener('change', () => {
    refresh();
    init.onKeepFontsOffline(keepOffline.checked);
  });

  fontSelect.addEventListener('change', () => {
    font = fontSelect.value as FontId;
    init.onFontChange(font, size);
  });
  sizeInput.addEventListener('input', () => {
    size = Number(sizeInput.value);
    sizeOut.textContent = `${size} px`;
    init.onFontChange(font, size);
  });

  more.addEventListener('toggle', () => {
    summary.setAttribute('aria-expanded', String(more.open));
    if (more.open) document.addEventListener('pointerdown', onOutsidePointer, true);
    else document.removeEventListener('pointerdown', onOutsidePointer, true);
    if (more.open === lastOpen) return;
    lastOpen = more.open;
    init.onMoreToggle(more.open);
  });

  more.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || !more.open) return;
    event.stopPropagation();
    more.open = false;
    summary.focus();
    // Escape *is* a user action, so this one is reported.
  });

  function onOutsidePointer(event: Event): void {
    if (event.target instanceof Node && more.contains(event.target)) return;
    more.open = false;
  }

  /* ------------------------------------------------------- initial values */

  applyOptions(init.options);
  showFont(init.ui.font, init.ui.size);
  keepOffline.checked = init.ui.keepFontsOffline;
  more.open = init.ui.moreOpen;
  summary.setAttribute('aria-expanded', String(more.open));
  refresh();

  function applyOptions(options: AppOptions): void {
    for (const [key, control] of selects) {
      const value = options[key];
      control.value = value === undefined ? String(DEFAULTS[key]) : String(value);
    }
    keepComments.checked = options.keepComments ?? false;
    strict.checked = options.recovery === 'strict';
    writeWrap(options.wrap);
    todaySel.value = options.todayMode ?? 'browser';
    customToday = options.todayCustom ?? customToday;
    todayInput.value = customToday;
    todayInput.hidden = todaySel.value !== 'custom';
  }

  /** Reflect a font choice in the two controls. The CSS side is `fonts.applyFont`. */
  function showFont(next: FontId, nextSize: number): void {
    font = next;
    size = nextSize;
    fontSelect.value = next;
    sizeInput.value = String(nextSize);
    sizeOut.textContent = `${nextSize} px`;
  }

  /* ------------------------------------------------------------ the handle */

  return {
    setOptions(options: AppOptions) {
      applyOptions(options);
      refresh();
    },
    setFont(next: FontId, nextSize: number) {
      showFont(next, nextSize);
    },
    setMoreOpen(open: boolean) {
      lastOpen = open;
      more.open = open;
    },
    setKeepFontsOffline(enabled: boolean) {
      keepOffline.checked = enabled;
      refresh();
    },
    setVersion(version: string) {
      versionCode.textContent = version || '—';
    },
    close() {
      lastOpen = false;
      more.open = false;
    },
  };

  /* ------------------------------------------------------------- builders */

  function registerSelect(key: LibKey, id: string, choices: Choice[]): HTMLSelectElement {
    const control = select(id, choices);
    selects.set(key, control);
    return control;
  }

  function todaySelect(): HTMLElement {
    const control = select('opt-today', [
      { value: 'browser', label: 'Today’s date, from this browser' },
      { value: 'library', label: '<today> (library default)' },
      { value: 'custom', label: 'Custom text…' },
    ]);
    const custom = el('input', 'control control-text');
    custom.type = 'text';
    custom.id = 'opt-today-custom';
    custom.placeholder = 'August 20, 2026';
    custom.hidden = true;
    const label = el('label', 'sr-only', 'What \\today renders as');
    label.htmlFor = custom.id;
    const wrap = el('div', 'today-row');
    wrap.append(control, label, custom);
    return wrap;
  }

  function mathFontSelect(): HTMLSelectElement {
    const control = el('select', 'control');
    control.id = 'opt-math-font';
    const common = el('optgroup');
    common.label = 'Usual choices';
    for (const choice of [
      { value: 'italic', label: 'Italic 𝑥 (default)' },
      { value: 'upright', label: 'Upright x' },
      { value: 'off', label: 'Off — letters stay as typed' },
      { value: 'default', label: 'Automatic, per context' },
    ]) {
      common.append(option(choice));
    }
    const alphabets = el('optgroup');
    alphabets.label = 'Other Unicode alphabets';
    for (const choice of [
      { value: 'bold', label: 'Bold 𝐱' },
      { value: 'bold-italic', label: 'Bold italic 𝒙' },
      { value: 'script', label: 'Script 𝓍' },
      { value: 'bold-script', label: 'Bold script 𝔁' },
      { value: 'fraktur', label: 'Fraktur 𝔵' },
      { value: 'bold-fraktur', label: 'Bold fraktur 𝖝' },
      { value: 'double-struck', label: 'Double-struck 𝕩' },
      { value: 'sans-serif', label: 'Sans-serif 𝗑' },
      { value: 'sans-serif-bold', label: 'Sans-serif bold 𝘅' },
      { value: 'sans-serif-italic', label: 'Sans-serif italic 𝘹' },
      { value: 'sans-serif-bold-italic', label: 'Sans-serif bold italic 𝙭' },
      { value: 'monospace', label: 'Monospace 𝚡' },
    ]) {
      alphabets.append(option(choice));
    }
    control.append(common, alphabets);
    selects.set('mathFont', control);
    return control;
  }

  function checkField(id: string, label: string, hint: string): HTMLElement {
    const box = el('input', 'control control-check');
    box.type = 'checkbox';
    box.id = id;
    const text = el('label', 'check-label', label);
    text.htmlFor = id;
    const row = el('div', 'check-row');
    row.append(box, text);
    const wrap = el('div', 'field field-check');
    wrap.dataset.set = 'false';
    wrap.append(row);
    if (hint) {
      const note = el('p', 'hint', hint);
      note.id = `${id}-hint`;
      box.setAttribute('aria-describedby', note.id);
      wrap.append(note);
    }
    return wrap;
  }

  function field(
    id: string,
    label: string,
    control: HTMLElement,
    extra?: { hint?: string; className?: string; bar?: boolean },
  ): HTMLElement {
    const wrap = el('div', `field${extra?.className ? ` ${extra.className}` : ''}`);
    wrap.dataset.set = 'false';
    const caption = el('label', 'field-label', label);
    caption.htmlFor = id;
    wrap.append(caption, control);
    if (extra?.hint) {
      const note = el('p', 'hint', extra.hint);
      note.id = `${id}-hint`;
      const target = control.id === id ? control : control.querySelector(`#${CSS.escape(id)}`);
      target?.setAttribute('aria-describedby', note.id);
      // The primary bar hides its labels below 700 px (§6.6), so the control carries
      // the whole story as a tooltip as well.
      if (extra.bar) target?.setAttribute('title', `${label} — ${extra.hint}`);
      wrap.append(note);
    }
    const key = keyOf(id);
    if (key) track(wrap, () => selects.get(key)!.value !== DEFAULTS[key]);
    return wrap;
  }

  function keyOf(id: string): LibKey | null {
    for (const [key, control] of selects) {
      if (control.id === id) return key;
    }
    return null;
  }

  function track(node: HTMLElement, changed: () => boolean): void {
    node.dataset.set = 'false';
    flags.push({ node, changed });
  }

  function checkbox(id: string): HTMLInputElement {
    return mount.querySelector<HTMLInputElement>(`#${id}`)!;
  }
}

/* ---------------------------------------------------------------- helpers */

function set(out: AppOptions, key: LibKey, value: string | boolean): void {
  (out as Record<string, unknown>)[key] = value;
}

function select(id: string, choices: Choice[]): HTMLSelectElement {
  const control = document.createElement('select');
  control.className = 'control';
  control.id = id;
  for (const choice of choices) control.append(option(choice));
  return control;
}

function option(choice: Choice): HTMLOptionElement {
  const node = document.createElement('option');
  node.value = choice.value;
  node.textContent = choice.label;
  return node;
}

function group(legend: string): HTMLFieldSetElement {
  const set_ = document.createElement('fieldset');
  set_.className = 'group';
  const caption = document.createElement('legend');
  caption.textContent = legend;
  set_.append(caption);
  return set_;
}

function glyph(mark: string): HTMLSpanElement {
  const node = document.createElement('span');
  node.className = 'glyph';
  node.setAttribute('aria-hidden', 'true');
  node.textContent = mark;
  return node;
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

function debounce(fn: () => void, ms: number): () => void {
  let timer = 0;
  return () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(fn, ms);
  };
}
