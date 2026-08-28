/**
 * The stylesheet's half of the editor overlay (web/PLAN.md §6.12).
 *
 * The invariant the overlay lives or dies by is a fact about pixels: **every character
 * in the mirror sits underneath the same character in the textarea, at every width, with
 * and without a scrollbar, in both wrapping states**. That cannot be asserted here —
 * vitest runs in `node`, there is no layout, and a mirror that wraps a column narrower
 * than the textarea looks exactly like one that does not until a browser measures both.
 * It was checked in Chromium instead, and the numbers are in the commit that added this
 * file.
 *
 * What *can* be asserted, and is, is the shape of the stylesheet that makes it true. The
 * regression this file exists for was a single property — the mirror hid its scrollbar
 * while the textarea showed one, so the two wrapped at columns fifteen pixels apart and
 * every wrapped line drifted a little further out of step down the document — and it was
 * invisible in review precisely because the two layers *look* like they share everything
 * by sharing a class. So: the properties that decide where a line breaks may be declared
 * only where both layers read them, and the mirror may not take its gutter back.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

const CSS = readFileSync(fileURLToPath(new URL('../src/styles.css', import.meta.url)), 'utf8');

/** One rule as the scanner below sees it: the selector list, and what it declares. */
interface Rule {
  selector: string;
  declarations: Map<string, string>;
}

/**
 * Every style rule in the sheet, with its at-rule wrappers dropped.
 *
 * A media query changes *when* a declaration applies, never *which* elements it can
 * reach, and every question below is about the latter — so `@media (pointer: coarse)`
 * raising the source pane to 16 px is read here as an ordinary rule on `.pane-input`,
 * which is what it is. Comments go first so that a property named in prose is not
 * mistaken for one that is set.
 */
function rules(css: string): Rule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, '');
  const found: Rule[] = [];
  let at = 0;
  let depth = 0;
  let start = 0;
  let prelude = '';
  while (at < text.length) {
    const char = text[at];
    if (char === '{') {
      if (depth === 0) {
        prelude = text.slice(start, at).trim();
        start = at + 1;
      }
      depth += 1;
    } else if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        const body = text.slice(start, at);
        // An at-rule's body is more rules; a style rule's body is declarations.
        if (prelude.startsWith('@')) found.push(...rules(body));
        else if (prelude !== '') found.push({ selector: prelude, declarations: declarations(body) });
        start = at + 1;
      }
      if (depth < 0) depth = 0;
    }
    at += 1;
  }
  return found;
}

function declarations(body: string): Map<string, string> {
  const out = new Map<string, string>();
  // A nested block (an at-rule inside a rule) is not this rule's own declarations.
  if (body.includes('{')) return out;
  for (const piece of body.split(';')) {
    const colon = piece.indexOf(':');
    if (colon < 0) continue;
    const name = piece.slice(0, colon).trim().toLowerCase();
    if (name === '' || name.startsWith('--')) continue;
    out.set(name, piece.slice(colon + 1).trim());
  }
  return out;
}

const ALL = rules(CSS);

/** Rules whose selector mentions `needle`, at all, anywhere in the list. */
function mentioning(needle: string): Rule[] {
  return ALL.filter((rule) => rule.selector.includes(needle));
}

/**
 * Everything that can move a glyph: where a line breaks, how wide a character is, and
 * how much box there is to put them in. A difference in any one of these between the two
 * layers is the bug this file guards against.
 */
const METRIC_PROPS = [
  'white-space',
  'white-space-collapse',
  'text-wrap',
  'text-wrap-mode',
  'text-wrap-style',
  'overflow-wrap',
  'word-wrap',
  'word-break',
  'line-break',
  'hyphens',
  'font',
  'font-family',
  'font-size',
  'font-weight',
  'font-style',
  'font-stretch',
  'font-variant',
  'font-variant-ligatures',
  'font-variant-numeric',
  'font-variant-caps',
  'font-feature-settings',
  'font-variation-settings',
  'font-kerning',
  'font-optical-sizing',
  'font-size-adjust',
  'letter-spacing',
  'word-spacing',
  'line-height',
  'tab-size',
  'text-indent',
  'text-transform',
  'text-rendering',
  'direction',
  'writing-mode',
  'padding',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'padding-inline',
  'padding-inline-start',
  'padding-inline-end',
  'padding-block',
  'border',
  'border-width',
  'border-left-width',
  'border-right-width',
  'border-top-width',
  'border-bottom-width',
  'box-sizing',
  'width',
  'max-width',
  'min-width',
  'inset',
  'left',
  'right',
  'scrollbar-width',
  'scrollbar-gutter',
];

describe('the source pane and the mirror behind it', () => {
  it('has a rule both layers wear, and a mirror that is one of them', () => {
    // The mirror is a `<div>` carrying the textarea's own classes: that is the whole
    // mechanism by which they share their metrics, and `ui/panes.ts` builds it that way.
    expect(mentioning('.pane-input').length).toBeGreaterThan(0);
    const shared = ALL.find((rule) => rule.selector === '.pane-input');
    expect(shared, 'a bare `.pane-input` rule, which both the textarea and the mirror match').toBeDefined();
  });

  it('declares the metric-deciding properties only where both layers read them', () => {
    // A rule that names the mirror reaches the mirror alone. Colour is its whole job —
    // the lexer's palette and the diagnostics' tint — and colour cannot move a glyph.
    for (const rule of mentioning('.pane-input-backdrop')) {
      for (const prop of rule.declarations.keys()) {
        expect(
          METRIC_PROPS.includes(prop),
          `\`${prop}\` is set on \`${rule.selector}\`, which is the mirror and not the textarea`,
        ).toBe(false);
      }
    }
  });

  it('reserves the scrollbar gutter on both layers rather than hiding one', () => {
    const shared = ALL.find((rule) => rule.selector === '.pane-input');
    // The regression itself. A classic scrollbar takes its width out of the *content*
    // box, so a textarea tall enough to need one wraps narrower than a mirror without
    // one. Reserving the gutter on the rule both layers read makes the wrap column a
    // fact about the pane's width alone.
    expect(shared?.declarations.get('scrollbar-gutter')).toBe('stable');
  });

  it('does not let the mirror give its gutter back', () => {
    // `scrollbar-width: none` is the trap: it reads as "hide a scrollbar nobody can use
    // anyway" and it silently takes the reserved gutter with it, because the gutter is
    // the scrollbar's own width. Same for the WebKit pseudo-element that zeroes it.
    for (const rule of mentioning('.pane-input-backdrop')) {
      expect(
        rule.declarations.get('scrollbar-width'),
        `\`${rule.selector}\` hides the mirror's scrollbar, which un-reserves its gutter`,
      ).toBeUndefined();
      expect(
        rule.selector.includes('::-webkit-scrollbar') && rule.declarations.get('display') === 'none',
        `\`${rule.selector}\` removes the mirror's scrollbar, which un-reserves its gutter`,
      ).toBe(false);
    }
  });

  it('paints the mirror without moving anything in it', () => {
    // The lexer's colours and the diagnostics' marks are spans *inside* the mirror. They
    // may change the ink and the wash behind it; an italic, a weight or a letter-spacing
    // among them would re-wrap the mirror on its own, one span at a time.
    const inked = ALL.filter((rule) => /(^|[\s,])\.(tk|hl)-/.test(rule.selector));
    expect(inked.length, 'the token and severity classes are in the sheet').toBeGreaterThan(0);
    const allowed = new Set([
      'color',
      'background',
      'background-color',
      'border-radius',
      'text-decoration',
      'text-decoration-line',
      'text-decoration-color',
      'text-decoration-style',
      'text-decoration-thickness',
      'text-underline-offset',
      'opacity',
    ]);
    for (const rule of inked) {
      for (const prop of rule.declarations.keys()) {
        expect(allowed.has(prop), `\`${prop}\` on \`${rule.selector}\` can move a glyph in the mirror`).toBe(true);
      }
    }
  });
});
