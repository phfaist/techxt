/**
 * MathJax, wrapped in four functions (web/PLAN.md §9.1).
 *
 * The rest of the app never imports MathJax, never names a MathJax option and never
 * touches `window.MathJax`. It asks for a formula to be typeset and gets a promise back.
 * That is the whole point of this module: MathJax is a large, globally-configured,
 * script-tag-shaped library, and everything awkward about it — the configuration that
 * has to exist before the script runs, the lazily fetched font ranges, the per-document
 * state that has to be cleared between conversions — is contained here.
 *
 * # It is fetched, not bundled into the app
 *
 * The TeX→CHTML bundle is 1.0 MB, and most visitors never turn the mode on. So it is
 * copied into `dist/` at a version-stamped path by the `techxt:mathjax` plugin in
 * `vite.config.ts` and pulled in with a `<script>` tag the first time
 * {@link loadMathJax} is called. The service worker holds it, and the metric ranges and
 * woff2 faces it asks for, in a `CacheFirst` route beside the one that holds the display
 * faces, so the second use is offline and so is every use after a reload.
 *
 * # CHTML, not SVG
 *
 * The mode shipped on the SVG output, chosen on the belief that an SVG bundle fetches no
 * fonts at runtime. That was wrong — MathJax 4 splits *both* font formats into character
 * ranges loaded on demand — and once the premise went, so did the argument. Measured at
 * the size pass (§9.1, §14): a reader who turns the mode on and reads all six shipped
 * examples fetches **414 244 B** here against 647 876 B under SVG, and the tree behind it
 * is 3.2 MB in `dist/` against 11.8 MB. SVG's ranges carry glyph outlines as path data;
 * CHTML's carry metrics and let the woff2 faces — which the browser fetches only for the
 * glyphs a formula actually reaches — do the drawing. Nothing outside this file and
 * `vite.config.ts` knew which it was.
 *
 * **Nothing here ever contacts a CDN.** MathJax 4 would, twice over, if left alone:
 * `loader.paths.fonts` defaults to jsdelivr, and the speech-rule engine fetches its
 * locale tables from there as well. Both are shut off below, deliberately and with the
 * offline promise of §9 in mind — a converter that quietly phones a third party the
 * first time it meets `\mathbb{R}` would make a liar of the About screen.
 *
 * # It is told where the mathematics is
 *
 * MathJax's own page scanning is off: no `$…$`, no `\[…\]`, no environments, no
 * `\(…\)`. It has to be, because techxt's output is *text* — a document writing `\$5`
 * produces a `$` that no amount of scanning can tell apart from the `$` of a formula.
 * The binding reports where the formulas are instead (§4.3), the caller wraps each range
 * in an element, and {@link typeset} converts exactly that element's content and nothing
 * else.
 *
 * # What it is told to understand
 *
 * {@link TEX_INPUT} is the TeX input configuration, and it is exported because
 * `tools/mathjax_coverage.mjs` typesets every name techxt defines under *this* object to
 * find the ones MathJax does not know (§9.1). A checker that read a copy of
 * the package list would be checking a copy.
 */

/** The version-stamped directory MathJax is served from; see `vite.config.ts`. */
declare const __MATHJAX_DIR__: string;

/**
 * Where the bundle, the extensions and the font package live, as an absolute path under
 * the app's base.
 *
 * A function rather than a module-level constant so that importing this module costs
 * nothing and needs nothing: `import.meta.env` and `__MATHJAX_DIR__` are Vite's, and
 * `tools/mathjax_coverage.mjs` imports this file under plain Node to read
 * {@link TEX_INPUT} out of it.
 */
function root(): string {
  return `${import.meta.env.BASE_URL}${__MATHJAX_DIR__}`;
}

/**
 * The parts of MathJax's global this module uses.
 *
 * Written out by hand rather than imported: the `mathjax` package ships components, not
 * types for the global the components install, and a structural interface of five
 * members is both smaller than the alternative and a list of exactly what this module
 * depends on.
 */
interface MathJaxGlobal {
  startup: {
    promise: Promise<unknown>;
    document: { clear(): void; updateDocument(): void };
  };
  /** Convert one formula. `display` chooses between `$…$` and `\[…\]` layout. */
  tex2chtmlPromise(tex: string, options: { display: boolean }): Promise<HTMLElement>;
  /** Forget the math typeset so far — the list, not the DOM. */
  typesetClear(): void;
  /** Reset the TeX input's per-document state: equation numbers and labels. */
  texReset(): void;
}

/* ------------------------------------------------------- what MathJax must understand */

/**
 * A TeX macro definition in MathJax's own spelling: the replacement text, or the
 * replacement text and the number of arguments `#1`… stand for.
 */
type MacroDefinition = string | [string, number];

/**
 * The TeX input configuration — the packages, and the definitions that fill the gaps
 * between what techxt defines and what those packages know (§9.1).
 *
 * **This object is the only authoritative copy**, which is what the export is for:
 * `tools/mathjax_coverage.mjs` imports it, typesets every name
 * `DefinitionSet::symbols()` reports under it, and fails CI when a name techxt calls
 * mathematics is one MathJax cannot read. A checker holding its own package list would
 * pass a build that had changed this one.
 */
export const TEX_INPUT: {
  packages: string[];
  macros: Record<string, MacroDefinition>;
  environments: Record<string, [string, string]>;
  inlineMath: string[][];
  displayMath: string[][];
  processEscapes: boolean;
  processEnvironments: boolean;
  processRefs: boolean;
} = {
  // `base` and `ams` are what the primitives techxt re-emits need — `\frac`, `\sum`,
  // `\sqrt` and the Greek from the first, and `equation`, `align`, `pmatrix`, `\mathbb`,
  // `\tfrac`, `\lvert`, `smallmatrix` from the second. `newcommand` and `configmacros`
  // cost nothing and cover a definition that survives into a formula; `noundefined` is
  // the one that earns its place, rendering an unknown command as red text instead of
  // throwing, because the document is the user's and techxt will happily re-emit a macro
  // no engine has heard of.
  //
  // The last two were chosen against the measurement recorded in §9.1 rather than against
  // a bug report, and each closes a group of names techxt defines and the five above do
  // not: `mathtools` the `psmallmatrix`/`bsmallmatrix`/`dcases` family — and
  // `\overbracket`, `\underbracket` and `\coloneqq` with them — and `upgreek` the 41
  // upright Greek letters of `\upalpha`…`\Upomega`. Together they close 46 of the 88
  // names the measurement found; the definitions below close the other 40.
  //
  // **`physics` was measured and rejected.** It closes the four Dirac names and changes
  // five it was never asked about, three of them into something techxt disagrees with:
  // `\div` becomes ∇· where techxt renders ÷, `\Im` and `\Re` become upright *Im* and
  // *Re* where techxt renders ℑ and ℜ, and `\Pr` and `\det` stop being operators, so a
  // subscript sits beside them instead of under.
  //
  // **`braket` was measured and not taken either**, which the item that chose it did not
  // expect: three of its four macros are exactly what techxt defines and its `\braket` is
  // a *different macro* — one argument with the bar inside it, `\braket{a|b}`, where
  // techxt takes two — so under it `\braket{\phi}{\psi}` typesets as ⟨ϕ⟩ψ while Fancy
  // mode renders ⟨ϕ|ψ⟩. A `configmacros` definition cannot correct that: a package's
  // macro map is consulted first whatever order the list is in, which was found by
  // watching the override do nothing. So the four are defined below instead — the three
  // that agreed carry the package's own bodies, byte for byte the same MathML — and the
  // extension is not fetched at all.
  //
  // `autoload` and `require` are deliberately *absent*: both fetch, and this app does
  // not. The two extensions above are loaded by name from our own origin instead
  // (`vite.config.ts` serves them, `loader.load` below asks for them).
  packages: ['base', 'ams', 'newcommand', 'configmacros', 'noundefined', 'mathtools', 'upgreek'],
  // What is left after the packages, supplied here rather than by pulling in a package
  // for two names — which is what `configmacros` is in the list for. Every one of these
  // is a name techxt defines, and where techxt's own rule is a literal the definition is
  // *that same literal*, so the two Math modes cannot drift apart: these are copied out
  // of `DefinitionSet::symbols()`, not invented.
  macros: {
    // Dirac notation, which is where the owner's report started. The first three are the
    // `braket` extension's own bodies, copied so that dropping the extension changes
    // nothing about them; `\braket` is techxt's two-argument form and *not* the
    // extension's one-argument one, which is the whole reason they are here.
    bra: ['{\\langle {#1} \\vert}', 1],
    ket: ['{\\vert {#1} \\rangle}', 1],
    braket: ['{\\langle {#1} \\vert {#2} \\rangle}', 2],
    ketbra: ['{\\vert {#1} \\rangle\\langle {#2} \\vert}', 2],
    // `\\abs` and `\\norm`, the other two delimiter pairs an author writes as one macro
    // over their content. They arrived in `defs::mathcore` the day the owner answered
    // item 9's second finding, and the gate turned red naming them the same afternoon —
    // which is what it is for. Only `physics` has them, and `physics` was measured and
    // rejected above, so they are definitions like the four before them, each the bar
    // techxt's own rule renders: the ASCII `|` that `\\lvert` and `\\rvert` are, and the
    // `‖` U+2016 that `\\lVert` and `\\rVert` are.
    //
    // **The starred spelling is a known gap.** techxt accepts `physics`'s `\\abs*{x}` and
    // `\\norm*{v}` for auto-sizing, and `configmacros` has no way to express an optional
    // star — a macro here takes brace arguments and nothing else — so a document that
    // writes one gets `|∗|𝑥` in this mode against Fancy's `|𝑥|`. Fixing it means either
    // `physics`, which changes five names it was never asked about, or a JavaScript
    // macro of our own, which is a MathJax extension and a larger thing than the case
    // deserves. Plain `\\abs{x}` is the spelling almost every document uses.
    abs: ['{\\lvert {#1} \\rvert}', 1],
    norm: ['{\\lVert {#1} \\rVert}', 1],
    // The Greek capitals that look like Latin letters. LaTeX itself defines none of
    // them — there is no `\Alpha`, because a capital alpha is an `A` — but techxt does,
    // rendering each as the Greek character, and a formula that writes one gets it.
    Alpha: 'Α',
    Beta: 'Β',
    Chi: 'Χ',
    Epsilon: 'Ε',
    Eta: 'Η',
    Iota: 'Ι',
    Kappa: 'Κ',
    Mu: 'Μ',
    Nu: 'Ν',
    Omicron: 'Ο',
    Rho: 'Ρ',
    Tau: 'Τ',
    Zeta: 'Ζ',
    // The same thirteen again in `upgreek`'s spelling: that package supplies the letters
    // whose upright form differs from the italic one and stops there, for the same
    // reason LaTeX does.
    Upalpha: 'Α',
    Upbeta: 'Β',
    Upchi: 'Χ',
    Upepsilon: 'Ε',
    Upeta: 'Η',
    Upiota: 'Ι',
    Upkappa: 'Κ',
    Upmu: 'Μ',
    Upnu: 'Ν',
    Upomicron: 'Ο',
    Uprho: 'Ρ',
    Uptau: 'Τ',
    Upzeta: 'Ζ',
    // Operators, set the way `\arccos` and `\sinh` are set beside them.
    arccosh: '\\operatorname{arccosh}',
    arcsinh: '\\operatorname{arcsinh}',
    arctanh: '\\operatorname{arctanh}',
    // Three literals, each the character techxt's own table renders the macro to. The
    // brackets carry their delimiter class as well, so that `\llbracket x \rrbracket`
    // spaces like the fence it is rather than like an ordinary symbol.
    degree: '°',
    llbracket: '\\mathopen{⟦}',
    rrbracket: '\\mathclose{⟧}',
    // techxt renders both of these through the same rule as `\frac`, so they are defined
    // as `\frac` here. `\nicefrac`'s own slant is a look, and the two modes agreeing is
    // worth more than the slant.
    nicefrac: ['\\frac{#1}{#2}', 2],
    textfrac: ['\\frac{#1}{#2}', 2],
  },
  // breqn's display environments, which MathJax has no port of. A `dmath` is a display
  // formula that breaks itself across lines; MathJax will not break it, and each display
  // formula already scrolls in a box of its own (§6.5), so a transparent wrapper renders
  // the mathematics and loses only the automatic break.
  environments: {
    dmath: ['', ''],
    'dmath*': ['', ''],
  },
  // Every form of automatic delimiter scanning, off. See the note at the top.
  inlineMath: [],
  displayMath: [],
  processEscapes: false,
  processEnvironments: false,
  processRefs: false,
};

/**
 * The configuration, which has to be in place *before* the script runs — MathJax reads
 * `window.MathJax` as it loads and replaces it with the live object.
 */
function configure(): void {
  const ROOT = root();
  (window as unknown as { MathJax: unknown }).MathJax = {
    loader: {
      // The two TeX extensions {@link TEX_INPUT} names that the combined bundle does not
      // carry — `tex-chtml.js` brings `ams`, `newcommand`, `configmacros`, `noundefined`,
      // `textmacros`, `require` and `autoload` and no more. They are loaded eagerly
      // rather than left to `autoload`, which is not in the package list because it
      // fetches whatever it likes; these two and the `boldsymbol` `mathtools` pulls in
      // with them are 27 KB, they come from the `paths` below like everything else here,
      // and the service worker holds them beside the bundle.
      load: ['[tex]/mathtools', '[tex]/upgreek'],
      paths: {
        // All three would otherwise resolve to `https://cdn.jsdelivr.net/npm/@mathjax`.
        // The last is the one that matters in practice: MathJax derives both
        // `chtml.fontURL` and the dynamic-range prefix from it, so it is where a
        // character outside the bundled ranges — a `\mathbb`, a `\mathcal` — has its
        // metrics fetched from, and where the browser is told to find the woff2 face
        // that draws it.
        mathjax: ROOT,
        fonts: ROOT,
        'mathjax-newcm': `${ROOT}/mathjax-newcm-font`,
      },
    },
    startup: {
      // The page is not a document full of `$…$` waiting to be found; `typeset` below
      // converts the elements it is given, one at a time.
      typeset: false,
    },
    // The packages and the definitions, above — the same object the coverage checker
    // measures, handed to MathJax unchanged.
    tex: TEX_INPUT,
    options: {
      // The contextual menu, and the semantic enrichment that feeds the speech, braille
      // and explorer layers. All of it is in the bundle, and the speech-rule engine
      // fetches its locale tables from a CDN the first time it is asked for one — reason
      // enough on its own. The output pane is a preview of converted *text*, and the
      // text itself, which is what a screen reader should be reading, is right there
      // beside it in the same pane.
      //
      // Every name here is one the bundle defines a default for. MathJax logs a warning
      // for an option it does not recognise, so this list is not a place to be
      // speculative.
      enableMenu: false,
      enableSpeech: false,
      enableBraille: false,
      enableEnrichment: false,
      enableExplorer: false,
      // **These five are not enough on their own**, which was found the only way it
      // could be — by watching a built page fail to typeset a single formula. The
      // contextual menu's own settings are applied to the document *after* the
      // configuration is, and its defaults turn enrichment, speech and braille straight
      // back on; `enableMenu: false` hides the menu without stopping that. The document
      // then reaches the `attachSpeech` render action, starts a web worker for the
      // speech-rule engine, and waits for an answer that never comes — the worker's
      // script is not one of the files `vite.config.ts` copies, and even if it were,
      // fetching locale tables is exactly what §9.1 says this app does not do. The
      // symptom is not an error: `tex2chtmlPromise` simply never settles.
      //
      // `explorer` was on this list and is not a menu setting the bundle knows — the
      // browser console said so on every run, in the one warning a page that promises a
      // clean console had left. Removing it turns nothing back on: `enableExplorer`
      // above is the switch, and a setting MathJax does not recognise was never doing
      // anything but printing that line.
      menuOptions: {
        settings: { enrich: false, speech: false, braille: false, collapsible: false },
      },
    },
  };
}

/** The load in flight, or the settled one. `null` until the first {@link loadMathJax}. */
let loading: Promise<void> | null = null;
/** Set once the bundle has installed its global and finished starting up. */
let ready = false;

/** MathJax's global, once {@link ready}. */
function api(): MathJaxGlobal {
  return (window as unknown as { MathJax: MathJaxGlobal }).MathJax;
}

/**
 * Fetch and start MathJax, once.
 *
 * Idempotent in both directions: calling it while a load is in flight returns that same
 * promise, and calling it afterwards returns immediately. A failed load rejects and
 * *stays* failed — the caller decides whether to say so — rather than retrying on every
 * keystroke against a network that is not there.
 */
export function loadMathJax(): Promise<void> {
  if (loading) return loading;
  loading = new Promise<void>((resolve, reject) => {
    configure();
    const script = document.createElement('script');
    script.src = `${root()}/tex-chtml.js`;
    script.async = true;
    script.addEventListener('load', () => {
      // The script installs the global synchronously but finishes starting up in a
      // promise of its own, and `tex2chtmlPromise` does not exist until it has.
      api()
        .startup.promise.then(() => {
          ready = true;
          resolve();
        })
        .catch(reject);
    });
    script.addEventListener('error', () => {
      reject(new Error('MathJax could not be loaded'));
    });
    document.head.appendChild(script);
  });
  return loading;
}

/** Whether MathJax is loaded and {@link typeset} will not have to fetch anything. */
export function mathJaxLoaded(): boolean {
  return ready;
}

/**
 * Typeset each element, replacing its text with the rendered formula.
 *
 * Each element's `textContent` must be one complete formula *with its delimiters* —
 * which is exactly what techxt's source mode emits and what a math region names. The
 * delimiters are stripped here rather than found by MathJax; an element whose content is
 * not a delimited formula is left exactly as it is, since the alternative is to guess.
 *
 * Loading is implicit: the first call fetches the bundle. The elements are converted in
 * order, so a document whose formulas need a font range MathJax has not got fetches it
 * once rather than once per formula.
 */
export async function typeset(elements: readonly HTMLElement[]): Promise<void> {
  if (elements.length === 0) return;
  await loadMathJax();
  const mathjax = api();
  for (const element of elements) {
    const formula = formulaIn(element.textContent ?? '');
    if (!formula) continue;
    const rendered = await mathjax.tex2chtmlPromise(formula.tex, { display: formula.display });
    element.replaceChildren(rendered);
  }
  // `tex2chtmlPromise` converts without touching the page, so MathJax's own stylesheet —
  // which is what gives `mjx-container` its layout — is only inserted when it is asked
  // to bring the document up to date. Clearing first keeps its list of typeset math from
  // growing over a session of editing.
  mathjax.startup.document.clear();
  mathjax.startup.document.updateDocument();
}

/**
 * Drop the per-document state MathJax accumulates: equation numbers, labels, and the
 * list of formulas it has typeset.
 *
 * Called between conversions. Without it the second conversion of a document would
 * number its equations as though they followed the first one's, and MathJax would hold
 * references to elements the output pane has already replaced.
 */
export function resetMathJax(): void {
  if (!ready) return;
  const mathjax = api();
  mathjax.typesetClear();
  mathjax.texReset();
}

/* --------------------------------------------------------------- the delimiters */

/** One formula: the TeX between the delimiters, and how it should be laid out. */
interface Formula {
  tex: string;
  display: boolean;
}

/**
 * The delimiter pairs techxt's source mode re-emits, longest opener first so that `$$`
 * is recognised before `$`.
 *
 * Source mode reproduces what the document wrote rather than normalising it, so all four
 * of these really do occur — `$…$` and `\(…\)` for inline, `$$…$$` and `\[…\]` for
 * display — alongside the `\begin{…}` environments handled separately below.
 */
const DELIMITERS: readonly { open: string; close: string; display: boolean }[] = [
  { open: '$$', close: '$$', display: true },
  { open: '\\[', close: '\\]', display: true },
  { open: '\\(', close: '\\)', display: false },
  { open: '$', close: '$', display: false },
];

/** A whole environment — `\begin{align} … \end{align}` — which MathJax takes as it is. */
const ENVIRONMENT = /^\\begin\{([^}]*)\}[\s\S]*\\end\{\1\}$/;

/**
 * The formula `text` is, or `null` if it is not a delimited formula at all.
 *
 * Whitespace around it is ignored: a display formula inside a list carries the
 * continuation indent layout gave it, on the first line and on every line after.
 */
function formulaIn(text: string): Formula | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (ENVIRONMENT.test(trimmed)) return { tex: trimmed, display: true };
  for (const { open, close, display } of DELIMITERS) {
    if (trimmed.length < open.length + close.length) continue;
    if (!trimmed.startsWith(open) || !trimmed.endsWith(close)) continue;
    return { tex: trimmed.slice(open.length, trimmed.length - close.length), display };
  }
  return null;
}
