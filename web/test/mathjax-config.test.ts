/**
 * The MathJax TeX configuration, held to the three properties a unit test can reach
 * (web/PLAN.md §9.1).
 *
 * The measurement itself — which of techxt's ~1 400 names MathJax understands — needs
 * MathJax, the symbol table and half a second, and lives in
 * `tools/mathjax_coverage.mjs` behind the `MathJax coverage` step in CI. What is here is
 * what can be checked without any of that, and it is checked here because vitest runs on
 * every change while that step runs on a push: the promise `noundefined` makes, the
 * arithmetic of a macro's arguments, and the fact that every package named is one the
 * browser can actually load.
 */

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

import { describe, expect, it } from 'vitest';

import { TEX_INPUT } from '../src/mathjax';
import { MATHJAX_TEX_EXTENSIONS } from '../vite.config';

const require = createRequire(import.meta.url);

/**
 * The TeX extensions the combined bundle carries, read out of `tex-chtml.js` itself.
 *
 * MathJax's loader records what a component provides in a `"provides"` map; a package on
 * that list is in the file the app loads and needs nothing else. Read rather than
 * written down, because the list is MathJax's to change at an upgrade.
 */
function bundledExtensions(): Set<string> {
  const source = readFileSync(require.resolve('mathjax/tex-chtml.js'), 'utf8');
  const map = /"provides",(\{.*?\})\)/s.exec(source)?.[1];
  expect(map, 'tex-chtml.js has a loader "provides" map').toBeTypeOf('string');
  const names = [...(map ?? '').matchAll(/\[tex\]\/([\w-]+)/g)].map((match) => match[1] ?? '');
  expect(names.length, 'the "provides" map lists the bundled TeX extensions').toBeGreaterThan(3);
  return new Set(names);
}

describe('the MathJax TeX configuration', () => {
  it('keeps noundefined, whatever else the package list gains', () => {
    // An unknown construct must render as a marker rather than kill the formula: the
    // document is the user's, and techxt re-emits a macro no engine has heard of. This
    // was true before the package list was chosen against a measurement and is not one
    // of the things that measurement was allowed to change.
    expect(TEX_INPUT.packages).toContain('noundefined');
  });

  it('scans for no delimiters of its own', () => {
    // techxt's output is text, and `\$5` produces a `$` that no scanner can tell from
    // the `$` of a formula (§9.1). The app hands MathJax one element per math region.
    expect(TEX_INPUT.inlineMath).toEqual([]);
    expect(TEX_INPUT.displayMath).toEqual([]);
    expect(TEX_INPUT.processEscapes).toBe(false);
    expect(TEX_INPUT.processEnvironments).toBe(false);
    expect(TEX_INPUT.processRefs).toBe(false);
  });

  it('names only packages the browser can load', () => {
    // A package that is neither in the bundle nor copied into `dist/` is a 404 at
    // startup, and MathJax reports a package it could not load by quietly not having it
    // — which looks exactly like the gap this configuration exists to close.
    const available = new Set([...bundledExtensions(), ...MATHJAX_TEX_EXTENSIONS, 'base']);
    for (const name of TEX_INPUT.packages) {
      expect(available, `\`${name}\` is bundled or served from our own origin`).toContain(name);
    }
  });

  it('declares as many arguments as each definition uses', () => {
    // `['\\frac{#1}{#2}', 2]`: the count is MathJax's contract for how much of the
    // formula the macro eats. One too few and the second argument is left in the output;
    // one too many and it swallows what follows.
    for (const [name, definition] of Object.entries(TEX_INPUT.macros)) {
      const [body, count] = typeof definition === 'string' ? [definition, 0] : definition;
      const used = [...body.matchAll(/#(\d)/g)].map((match) => Number(match[1]));
      const highest = used.length === 0 ? 0 : Math.max(...used);
      expect(highest, `\\${name} uses no argument beyond the ${count} it declares`).toBeLessThanOrEqual(
        count,
      );
      expect(count, `\\${name} declares no argument it never uses`).toBe(highest);
    }
  });

  it('defines every environment as a pair of begin and end code', () => {
    for (const [name, definition] of Object.entries(TEX_INPUT.environments)) {
      expect(definition, `{${name}} is [begin, end]`).toHaveLength(2);
      for (const half of definition) expect(typeof half).toBe('string');
    }
  });
});
