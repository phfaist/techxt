#!/usr/bin/env node
/**
 * Report which constructs techxt defines that MathJax does not understand.
 *
 * web/PLAN.md §9.1. The *Math: MathJax* mode hands the typesetter a
 * formula's own LaTeX, post-expansion (§4.3), so what MathJax has to understand is not
 * the user's macros but techxt's: the ~1 400 names `DefinitionSet::symbols()` reports.
 * Nobody had ever compared the two lists, and the answer turned out to be 770 names — so
 * this script exists for the same reason `coverage_check.py` does, and follows the same
 * policy: a hard gate on the core, a warning into the job summary for the long tail.
 *
 * Usage:
 *
 *     node web/tools/mathjax_coverage.mjs              # the report, on stdout
 *     node web/tools/mathjax_coverage.mjs --check      # the CI gate (§11)
 *     node web/tools/mathjax_coverage.mjs --symbols F  # reuse a dump, skipping cargo
 *
 * # What it does
 *
 * 1. Reads every name the library defines from `cargo run --example symbol_index` in
 *    `web/crate` — `techxt::defs::standard()` through `DefinitionSet::symbols()`, which
 *    is the table the completion row is drawn from as well (§4.9).
 * 2. Starts MathJax under Node with **the app's own TeX configuration**, imported from
 *    `src/mathjax.ts` rather than copied: the packages, the `configmacros` definitions
 *    and the scanning settings are one object, and a checker with a list of its own
 *    would happily pass a build that had changed the app's.
 * 3. Typesets one construct per name and classifies the answer.
 *
 * # How "unknown" is decided, which is the whole of the method
 *
 * `noundefined` is in the package list and stays there, so **MathJax never fails on an
 * unknown command**: it renders it as red text and reports success. A checker that
 * treated a settled promise as knowledge would report a coverage of 100 % on a
 * configuration that knows nothing. So the answer is read out of the MathML instead:
 *
 * - a macro is **unknown** when the output carries `noundefined`'s own marker for it,
 *   `<mtext mathcolor="red" data-latex="\name">` — the marker names the macro, so a red
 *   `\foo` inside the argument of a `\bar` that MathJax does know is not mistaken for
 *   `\bar` being unknown;
 * - an environment is **unknown** when the output is the `merror` MathJax raises for
 *   `Unknown environment 'name'`;
 * - any *other* `merror` — `Missing argument for \dddot`, `Illegal pream-token` — means
 *   the name is known and this script's filler argument was not what it wanted. Those
 *   are counted and reported, never gated on: they are a fact about the probe, not about
 *   MathJax.
 *
 * Two canaries hold the classifier to that: a name nobody defines must come back
 * unknown, and `\alpha` must come back known. If either fails the run aborts rather than
 * reporting a number nobody should trust — the same instinct as `coverage_check.py`
 * refusing an implausibly small table.
 *
 * # The three tiers
 *
 * **Core, gated.** Everything in techxt's own mathematics categories — `mathcore`,
 * `mathenvs`, `subsuperscripts` — plus the handful of names in {@link ALSO_CORE} that
 * are mathematics living elsewhere in the table. These are the constructs a *formula*
 * contains, and a formula is the only thing MathJax is ever handed, so a gap here is the
 * bug the owner reported. Anything unknown and not in {@link ACCEPTED_GAPS} fails
 * `--check`.
 *
 * **The long tail, warned.** The generated `symbols_extra` table and the categories that
 * are document structure — `\section`, `\cite`, `itemize`, `tabular`, the text accents,
 * the text font declarations. They reach MathJax only inside a formula that has no
 * business containing them, there are hundreds of them, and no package would close them.
 * They are counted per category and written to `$GITHUB_STEP_SUMMARY`, so that a future
 * reader can promote one rather than discover it.
 *
 * **Not measurable, excluded.** The 12 *specials* are character triggers (`~`, `^`,
 * `--`, `` ` ``), not control sequences. MathJax cannot report a character as undefined,
 * so there is no question here to answer; the count is printed so the exclusion is
 * visible rather than silent.
 *
 * # One more thing it checks, because only this script can
 *
 * A package the app names has to be a file the app *serves*: the browser bundle carries
 * seven TeX extensions and fetches every other one from `loader.paths.mathjax`, which is
 * our own origin. So after the walk this asks MathJax's loader what it actually loaded —
 * `mathtools` pulls in `boldsymbol`, which nothing in the configuration mentions — and
 * fails unless `MATHJAX_TEX_EXTENSIONS` in `vite.config.ts` copies each one into
 * `dist/`. That failure is not a coverage finding and does not wait for `--check`: it is
 * a configuration that works perfectly here and 404s in the browser, where the symptom
 * would be a package silently doing nothing.
 *
 * # Why this is a Node script and not a Python one
 *
 * `coverage_check.py` is the precedent for the shape, the `--check` gate and the
 * `$GITHUB_STEP_SUMMARY` reporting, and not for the language. The thing being measured
 * is a JavaScript library configured from a TypeScript object: run under Node it is the
 * same `mathjax` package at the same version that `vite.config.ts` copies into `dist/`,
 * reading the same configuration the browser gets. Any other language would have to
 * reimplement one of those two halves.
 */

import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const WEB = dirname(dirname(fileURLToPath(import.meta.url)));

/** Techxt's own mathematics: the categories whose names a formula is made of. */
const MATH_CATEGORIES = new Set(['mathcore', 'mathenvs', 'subsuperscripts']);

/**
 * Names gated with the mathematics categories although they live elsewhere in the table.
 *
 * The list is short on purpose and every entry says why it is mathematics, because the
 * alternative — gating the whole table — is a check nobody can keep green and a baseline
 * of six hundred accepted gaps nobody can read.
 */
const ALSO_CORE = new Map([
  // Dirac notation. techxt keeps these in the *generated* long tail, which is an
  // accident of where the table came from and not a statement about them; they are what
  // the owner's report was about, and they are mathematics in anybody's book.
  ['macro:bra', 'Dirac notation'],
  ['macro:ket', 'Dirac notation'],
  ['macro:braket', 'Dirac notation'],
  ['macro:ketbra', 'Dirac notation'],
  // Square-bracket versions of `\overbrace`/`\underbrace`, which techxt files under
  // `base` beside `\overline`. A formula is where they occur, and `mathtools` supplies
  // them, so gating them is what makes dropping that package a red build rather than a
  // quiet regression.
  ['macro:overbracket', 'a decoration over a sub-formula'],
  ['macro:underbracket', 'a decoration under a sub-formula'],
]);

/**
 * The core gaps that are accepted, each with the measurement that says why it stays.
 *
 * `coverage_check.py`'s discipline, for the same reason: a gate that is red on the day it
 * is written is a gate people learn to ignore, and the alternative to a recorded baseline
 * is either no gate or a definition invented to fit. Every entry here has to earn its
 * place in a sentence, and a gap that later closes is reported so that the entry can go.
 */
const ACCEPTED_GAPS = new Map([
  [
    'macro:intertext',
    'amsmath prose between the rows of a display. Defined as `\\text{#1}` it typesets, ' +
      'but the prose lands in the first column of the next row and pushes the formula ' +
      'sideways — measured. A red marker in one place beats a silently misaligned display.',
  ],
  [
    'environment:subequations',
    "amsmath's numbering wrapper. A transparent definition works only around content " +
      'nobody writes: with the `align` it is always wrapped around, MathJax answers ' +
      '"Erroneous nesting of equation structures" — measured. There is nothing to map it to.',
  ],
]);

/** Below this many names the dump is not the table, and no report from it is worth reading. */
const MIN_SYMBOLS = 1200;

/** The TeX extensions `tex-chtml.js` carries, read from the bundle rather than believed. */
function bundledExtensions() {
  const bundle = join(WEB, 'node_modules', 'mathjax', 'tex-chtml.js');
  const source = readFileSync(bundle, 'utf8');
  const provides = /"provides",(\{.*?\})\)/s.exec(source);
  if (provides === null) {
    throw new Error(`${bundle}: no loader "provides" map — MathJax's packaging changed`);
  }
  const names = [...provides[1].matchAll(/\[tex\]\/([\w-]+)/g)].map((match) => match[1]);
  if (names.length < 4) {
    throw new Error(
      `${bundle}: the "provides" map lists only ${names.length} TeX extensions, which ` +
        'cannot be right — this parser has stopped matching the file',
    );
  }
  return new Set(names);
}

/** Every name the library defines, as `symbol_index` prints it. */
function symbolTable(dump) {
  const json = dump
    ? readFileSync(dump, 'utf8')
    : execFileSync('cargo', ['run', '--quiet', '--example', 'symbol_index'], {
        cwd: join(WEB, 'crate'),
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
      });
  const symbols = JSON.parse(json);
  if (symbols.length < MIN_SYMBOLS) {
    throw new Error(
      `the symbol table has only ${symbols.length} entries, expected at least ` +
        `${MIN_SYMBOLS} — refusing to report a vacuous pass`,
    );
  }
  return symbols;
}

/**
 * One construct per name: the macro with a filler for each declared argument, the
 * environment with a body that suits a matrix or an alignment.
 *
 * The filler does not have to be what the construct wants. An unknown macro is marked
 * unknown whatever follows it, and a known one that dislikes `{x}` says so in a way this
 * script can tell apart (see the header).
 */
function construct(entry) {
  if (entry.kind === 'macro') return `\\${entry.name}${'{x}'.repeat(entry.arity)}`;
  const argument = entry.arity > 0 ? '{cc}' : '';
  return `\\begin{${entry.name}}${argument} a & b \\\\ c & d \\end{${entry.name}}`;
}

/** MathJax, started under Node with the app's own TeX configuration. */
async function mathjax(texInput) {
  const { init } = await import('mathjax');
  const engine = await init({
    loader: {
      // The browser gets `tex-chtml.js`, which carries a fixed set of extensions and
      // fetches the rest; under Node every package is a file, so each one is named here.
      // `tex-base` rather than `tex`, because the combined input component adds its own
      // packages to the list and this script must measure the app's list and no other.
      load: [
        'input/tex-base',
        ...texInput.packages.filter((name) => name !== 'base').map((name) => `[tex]/${name}`),
        'adaptors/liteDOM',
      ],
    },
    tex: texInput,
    startup: { typeset: false },
  });
  const effective = JSON.stringify(engine.config.tex.packages);
  if (effective !== JSON.stringify(texInput.packages)) {
    throw new Error(
      `MathJax is running with ${effective}, not the app's package list — a measurement ` +
        'under a different configuration is not a measurement of the app',
    );
  }
  return engine;
}

/**
 * What MathJax made of one construct: `unknown`, or an error message, or neither.
 *
 * The output is MathML, which is enough: whether the TeX input can read a name is a
 * question about the input processor, and CHTML or SVG is what happens afterwards.
 */
function classify(engine, entry) {
  const mml = engine.tex2mml(construct(entry), { display: entry.kind === 'environment' });
  const unknown =
    entry.kind === 'macro'
      ? mml.includes(`<mtext mathcolor="red" data-latex="\\${entry.name}"`)
      : mml.includes(`Unknown environment '${entry.name}'`);
  const error = /data-mjx-error="([^"]*)"/.exec(mml);
  return { unknown, error: unknown ? null : (error?.[1] ?? null) };
}

/** Fail loudly if the classifier itself has stopped working. */
function checkCanaries(engine) {
  const nonsense = { name: 'techxtNoSuchMacroZZ', kind: 'macro', arity: 1 };
  if (!classify(engine, nonsense).unknown) {
    throw new Error(
      `\\${nonsense.name} was not reported as unknown — the classifier no longer ` +
        "recognises `noundefined`'s marker, so every name would look known",
    );
  }
  const known = { name: 'alpha', kind: 'macro', arity: 0 };
  if (classify(engine, known).unknown) {
    throw new Error(
      '\\alpha was reported as unknown — MathJax is not running with the package list ' +
        'it was given, so every name would look unknown',
    );
  }
}

/** `\name` or `{name}`, as the report writes it. */
function label(entry) {
  return entry.kind === 'environment' ? `{${entry.name}}` : `\\${entry.name}`;
}

/** The report, the gate, and the exit code CI reads. */
async function main(argv) {
  const check = argv.includes('--check');
  const dumpAt = argv.indexOf('--symbols');
  const dump = dumpAt === -1 ? null : resolve(argv[dumpAt + 1] ?? '');

  // Both files are TypeScript, and Node runs TypeScript by stripping the types (22.18
  // and later). That is what lets the checker read the configuration the app ships
  // instead of a copy of it, so a Node too old to do it is worth saying out loud rather
  // than failing as an unknown file extension.
  let TEX_INPUT;
  let MATHJAX_TEX_EXTENSIONS;
  try {
    ({ TEX_INPUT } = await import(pathToFileURL(join(WEB, 'src', 'mathjax.ts')).href));
    ({ MATHJAX_TEX_EXTENSIONS } = await import(pathToFileURL(join(WEB, 'vite.config.ts')).href));
  } catch (failure) {
    throw new Error(
      `could not read the app's configuration out of src/mathjax.ts: ${failure}\n` +
        `This needs a Node that strips types from an imported .ts file (22.18 or later; ` +
        `this is ${process.version}).`,
    );
  }
  const symbols = symbolTable(dump);
  const engine = await mathjax(TEX_INPUT);
  checkCanaries(engine);

  const unknown = [];
  const errors = [];
  let walked = 0;
  let specials = 0;
  for (const entry of symbols) {
    if (entry.kind === 'specials') {
      specials += 1;
      continue;
    }
    walked += 1;
    const answer = classify(engine, entry);
    if (answer.unknown) unknown.push(entry);
    else if (answer.error) errors.push([entry, answer.error]);
  }

  // Everything the loader pulled in has to be a file the app serves from its own origin,
  // or one the browser bundle already carries. A package added to `TEX_INPUT.packages`
  // and not to `MATHJAX_TEX_EXTENSIONS` works perfectly here and 404s in the browser.
  const bundled = bundledExtensions();
  const served = new Set(MATHJAX_TEX_EXTENSIONS);
  const loaded = [...engine.loader.versions.keys()]
    .map((path) => /input[/\\]tex[/\\]extensions[/\\]([\w-]+)\.js$/.exec(path)?.[1])
    .filter((name) => name !== undefined);
  const unserved = loaded.filter((name) => !bundled.has(name) && !served.has(name));

  const core = [];
  const tail = [];
  for (const entry of unknown) {
    const key = `${entry.kind}:${entry.name}`;
    (MATH_CATEGORIES.has(entry.category) || ALSO_CORE.has(key) ? core : tail).push(entry);
  }
  const regressions = core.filter((entry) => !ACCEPTED_GAPS.has(`${entry.kind}:${entry.name}`));
  const stale = [...ACCEPTED_GAPS.keys()].filter(
    (key) => !core.some((entry) => `${entry.kind}:${entry.name}` === key),
  );

  console.log(
    `packages: ${TEX_INPUT.packages.join(', ')}\n` +
      `definitions in the config: ${Object.keys(TEX_INPUT.macros).length} macros, ` +
      `${Object.keys(TEX_INPUT.environments).length} environments\n` +
      `walked ${walked} names (${symbols.length} in the table, ${specials} specials not ` +
      'measurable — a character trigger cannot be an undefined control sequence)\n',
  );
  console.log(
    `unknown to MathJax: ${unknown.length} — ${core.length} in the core, ` +
      `${tail.length} in the long tail`,
  );
  console.log(`known but unhappy with this script's filler argument: ${errors.length}\n`);

  for (const entry of core) {
    const key = `${entry.kind}:${entry.name}`;
    const accepted = ACCEPTED_GAPS.get(key);
    console.log(`    ${label(entry)} (${entry.category})  ${accepted ? '— accepted: ' + accepted : '** NEW **'}`);
  }

  const byCategory = new Map();
  for (const entry of tail) {
    byCategory.set(entry.category, [...(byCategory.get(entry.category) ?? []), entry]);
  }
  const ordered = [...byCategory.entries()].sort((a, b) => b[1].length - a[1].length);
  console.log('\nthe long tail, by category:');
  for (const [category, entries] of ordered) {
    console.log(`    ${String(entries.length).padStart(4)}  ${category}`);
  }

  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  const lines = [
    '## MathJax coverage of techxt\'s symbols (web/PLAN.md §9.1)',
    '',
    `Walked **${walked}** names under \`${TEX_INPUT.packages.join(', ')}\`; ` +
      `**${unknown.length}** are unknown to MathJax, **${core.length}** of them in the core.`,
    '',
    'These reach MathJax only inside a formula that has no business containing them, ' +
      'and no package would close them. Listed so that a name worth promoting to the ' +
      'core is found rather than discovered.',
    '',
  ];
  for (const [category, entries] of ordered) {
    lines.push(`### ${category} — ${entries.length} unknown`, '', '```');
    lines.push(entries.map(label).join(' '));
    lines.push('```', '');
  }
  if (errors.length > 0) {
    lines.push(
      `### ${errors.length} names MathJax knows and this probe asked for badly`,
      '',
      'A fact about the filler argument, not about coverage: `\\Big` wants a delimiter ' +
        'and `\\begin{alignat}` a column count, and neither gets one here.',
      '',
      '```',
      ...errors.map(([entry, message]) => `${label(entry)}: ${message}`),
      '```',
      '',
    );
  }
  if (summaryPath) {
    appendFileSync(summaryPath, lines.join('\n') + '\n', 'utf8');
    console.log('\nthe long tail was written to $GITHUB_STEP_SUMMARY');
  } else {
    console.log('\n' + lines.join('\n'));
  }

  if (stale.length > 0) {
    // Not a failure: a gap that closed is good news, and the only cost of noticing it
    // late is a stale comment.
    console.log(
      `\n${stale.length} accepted gap(s) have closed — drop them from ACCEPTED_GAPS: ` +
        stale.join(', '),
    );
  }
  if (unserved.length > 0) {
    console.error(
      `\nMathJax loaded ${unserved.map((name) => `${name}.js`).join(', ')}, which ` +
        '`vite.config.ts` does not copy into `dist/`. In the browser that is a 404 and a ' +
        'package that silently does nothing: add it to MATHJAX_TEX_EXTENSIONS.',
    );
    return 1;
  }
  if (regressions.length > 0) {
    console.error(
      `\n${regressions.length} construct(s) techxt defines as mathematics are unknown to ` +
        'MathJax and are not in ACCEPTED_GAPS (web/PLAN.md §9.1): ' +
        regressions.map(label).join(', ') +
        '\nEither give the name a definition in `TEX_INPUT.macros` in `src/mathjax.ts`, ' +
        'or add the package that has it, or record it as an accepted gap with the reason.',
    );
    return check ? 1 : 0;
  }
  console.log(
    `\nMathJax understands every construct techxt calls mathematics` +
      (ACCEPTED_GAPS.size > 0 ? ` (${ACCEPTED_GAPS.size} recorded gaps aside).` : '.'),
  );
  return 0;
}

process.exitCode = await main(process.argv.slice(2));
