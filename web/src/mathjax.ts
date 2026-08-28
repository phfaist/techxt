/**
 * MathJax, wrapped in four functions (web/PLAN.md §9.1, TODO item 2).
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
 */

/** The version-stamped directory MathJax is served from; see `vite.config.ts`. */
declare const __MATHJAX_DIR__: string;

/** Where the bundle and the font package live, as an absolute path under the app's base. */
const ROOT = `${import.meta.env.BASE_URL}${__MATHJAX_DIR__}`;

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

/**
 * The configuration, which has to be in place *before* the script runs — MathJax reads
 * `window.MathJax` as it loads and replaces it with the live object.
 */
function configure(): void {
  (window as unknown as { MathJax: unknown }).MathJax = {
    loader: {
      // Nothing beyond what the bundle already contains. `autoload` and `require` are
      // in it and would happily fetch a TeX extension over the network; with an empty
      // package list below they are never consulted.
      load: [],
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
    tex: {
      // `base` and `ams` are what the primitives techxt re-emits need — `\frac`,
      // `\sum`, `\sqrt` and the Greek from the first, and `equation`, `align`,
      // `pmatrix`, `\mathbb`, `\tfrac`, `\lvert` from the second. `newcommand` and
      // `configmacros` cost nothing and cover a definition that survives into a
      // formula; `noundefined` is the one that earns its place, rendering an unknown
      // command as red text instead of throwing, because the document is the user's and
      // techxt will happily re-emit a macro no engine has heard of.
      //
      // `autoload` and `require` are deliberately *absent*: both fetch, and this app
      // does not.
      packages: ['base', 'ams', 'newcommand', 'configmacros', 'noundefined'],
      // Every form of automatic delimiter scanning, off. See the note at the top.
      inlineMath: [],
      displayMath: [],
      processEscapes: false,
      processEnvironments: false,
      processRefs: false,
    },
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
      // script is not one of the three trees `vite.config.ts` copies, and even if it were,
      // fetching locale tables is exactly what §9.1 says this app does not do. The
      // symptom is not an error: `tex2chtmlPromise` simply never settles.
      menuOptions: {
        settings: { enrich: false, speech: false, braille: false, explorer: false, collapsible: false },
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
    script.src = `${ROOT}/tex-chtml.js`;
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
