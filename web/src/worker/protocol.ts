/**
 * The message protocol and the data shapes shared by the app and the worker.
 *
 * This file is the contract between three parties: `web/crate` (which produces
 * `ConversionResult` and consumes `OptionsPayload`), the worker, and the UI. Every
 * string literal below is the wire form the Rust `OptionsDto` deserializes — kebab
 * case for enum values, camelCase for field names — so a change here is a change in
 * `web/crate/src/options.rs` too. See web/PLAN.md §4.2, §4.3 and §6.2.
 */

/* ------------------------------------------------------------------ options in */

/** `techxt::convert::MathMode`. */
export type MathMode = 'fancy' | 'plain' | 'source';

/** `techxt::convert::MathWrapDelims`, minus the `Custom` variant (not exposed). */
export type MathWrapDelims = 'parens' | 'braces' | 'none';

/** `techxt::convert::MatrixDelims`. */
export type MatrixDelims = 'unicode' | 'ascii';

/** `techxt::convert::HeadingStyle`. */
export type HeadingStyle = 'numbered-underlined' | 'underlined' | 'prefix' | 'plain';

/** `techxt::convert::FootnoteStyle`. */
export type FootnoteStyle = 'collected' | 'inline' | 'skip';

/** `techxt::convert::UnknownMacroPolicy`. */
export type UnknownMacroPolicy = 'skip' | 'render-args' | 'keep-source' | 'placeholder';

/** `techxt::convert::UnknownEnvPolicy`. */
export type UnknownEnvPolicy = 'render-body' | 'skip' | 'keep-source';

/** `techxt::convert::UnknownSpecialsPolicy`. */
export type UnknownSpecialsPolicy = 'emit-chars' | 'skip';

/** `techxt::convert::Recovery`. */
export type Recovery = 'tolerant' | 'strict';

/**
 * `techxt::convert::MacroDefinitions`: whether a `\newcommand` in the document defines
 * a macro that later uses expand (`'honored'`, the library's default), or is read and
 * dropped so a later use is an unknown command (`'declared'`).
 */
export type MacroDefinitions = 'honored' | 'declared';

/**
 * `techxt::convert::FontStyle`: `'off'` is `Disabled`, `'default'` is `Default`, and
 * every other value is `Style(FontStyleKind::…)` — the Unicode alphabet a letter is
 * mapped into. Nothing here has anything to do with the display font of §8.
 */
export type FontStyleValue =
  | 'off'
  | 'default'
  | 'bold'
  | 'italic'
  | 'bold-italic'
  | 'script'
  | 'bold-script'
  | 'fraktur'
  | 'bold-fraktur'
  | 'double-struck'
  | 'sans-serif'
  | 'sans-serif-bold'
  | 'sans-serif-italic'
  | 'sans-serif-bold-italic'
  | 'monospace'
  | 'upright';

/**
 * What the app sends to the binding: only the options the user changed.
 *
 * Every field is optional and an absent field means *the library's own default* —
 * the app never re-types a default, so a change of one in techxt is picked up rather
 * than frozen (PLAN §4.2, §6.4). `wrapWidth: null` and `today: null` are the
 * library's `None`, which is also what absent means for those two.
 */
export interface OptionsPayload {
  mathMode?: MathMode;
  mathExpressionIn?: MathWrapDelims;
  matrixDelimiters?: MatrixDelims;
  /** Column count, or `null`/absent for no wrapping. */
  wrapWidth?: number | null;
  keepComments?: boolean;
  headingStyle?: HeadingStyle;
  footnoteStyle?: FootnoteStyle;
  textFont?: FontStyleValue;
  mathFont?: FontStyleValue;
  unknownMacro?: UnknownMacroPolicy;
  unknownEnv?: UnknownEnvPolicy;
  unknownSpecials?: UnknownSpecialsPolicy;
  /** What `\today` renders as; `null`/absent leaves the library's `<today>`. */
  today?: string | null;
  recovery?: Recovery;
  macroDefinitions?: MacroDefinitions;
}

/** The keys of {@link OptionsPayload}, for exhaustive iteration in the state codec. */
export type OptionKey = keyof OptionsPayload;

/* ---------------------------------------------------------------- results out */

/** Diagnostic severities, in `techxt`'s own order (`Note < Warning < Error`). */
export type Severity = 'note' | 'warning' | 'error';

/**
 * A span in the document the user typed. Offsets are **UTF-16 code units**, mapped
 * from techy's UTF-8 byte offsets inside the binding (PLAN §4.4), so they can be
 * handed to `setSelectionRange` unchanged.
 */
export interface Span {
  start: number;
  end: number;
  /** 1-based. */
  line: number;
  /** 1-based, counted in characters. */
  column: number;
}

/** One entry of the include/expansion trace behind a diagnostic. */
export interface TraceFrame {
  title: string;
  span: Span | null;
}

export interface Diagnostic {
  severity: Severity;
  /** e.g. `"techxt.unknown-macro"`. */
  identifier: string;
  message: string;
  /** `Diagnostic::render()` — the same text `techxt-cli` prints. */
  rendered: string;
  /**
   * Where to select in the input, or `null` when neither this diagnostic nor anything
   * it came from is in the document that was converted (§4.5).
   */
  span: Span | null;
  /**
   * Whether {@link span} is the diagnostic's own position (`false`) or the nearest
   * enclosing macro invocation in the typed document (`true`).
   *
   * A diagnostic raised inside an expansion points into the macro's *body*, which is
   * not a place the textarea can select, so the binding substitutes the invocation
   * that expanded it and says so here (§4.5). Always `false` when `span` is `null`.
   */
  approx: boolean;
  frames: TraceFrame[];
}

/**
 * A run of the converted **output** that is a formula's own LaTeX source.
 *
 * Offsets are UTF-16 code units into {@link ConversionResult.text}, mapped in the
 * binding by the same single pass a diagnostic's span goes through (§4.4). The app
 * wraps each range in an element and hands the element to MathJax; the text itself is
 * untouched, so what the user copies, downloads or saves is still the library's own
 * string (§4.3, §6.3).
 *
 * The list is already filtered. techxt reports four kinds of preformatted run — a
 * source-mode formula, a formula it rendered and laid out itself, a construct kept as
 * source, a `verbatim` body — and only the first is LaTeX anyone can typeset, so the
 * binding drops the other three rather than making the app reason about a provenance.
 *
 * Three properties worth knowing before wrapping these in elements: a display formula's
 * range **excludes** the newline that ends its last line; a construct that renders to
 * nothing reports nothing; and a range may span a line break, so it is not guaranteed to
 * sit within one line of the output.
 */
export interface MathRegion {
  start: number;
  end: number;
  /** Display math (`\[…\]`, `equation`, `align`) rather than inline (`$…$`). */
  display: boolean;
}

export interface ConversionResult {
  /** `false` only for a hard parse failure (strict mode). */
  ok: boolean;
  /** `''` when `!ok`. */
  text: string;
  /** Conversion time in milliseconds, for the status line. */
  ms: number;
  /** In `Diagnostics::sorted_by_position()` order. */
  diagnostics: Diagnostic[];
  /** `Diagnostics::suppressed()` — the "and N more" count. */
  suppressed: number;
  truncated: boolean;
  /**
   * The formulas in {@link text}, in output order, never overlapping.
   *
   * Reported unconditionally — there is no option that turns it on — and empty for the
   * two math modes that do not re-emit source. The app ignores it unless the user asked
   * for MathJax.
   */
  regions: MathRegion[];
}

/* ------------------------------------------------------------- completions out */

/**
 * One completion the binding offers for a prefix, ranked and merged there.
 *
 * The JS side does no matching, no merging and no ranking: it sends a prefix and
 * renders what comes back, so every rule about what is offered and in what order lives
 * next to the symbol table it is drawn from (TODO item 5).
 */
export interface Completion {
  /** The name without its backslash, e.g. `alpha`. */
  name: string;
  kind: 'macro' | 'environment' | 'specials';
  /** What it renders as when the rule is a plain literal (`\alpha` → `α`), else null. */
  replacement: string | null;
  arity: number;
  /** Whether it came from a `\newcommand` and friends in the document being edited. */
  fromDocument: boolean;
}

/* -------------------------------------------------------------- the messages */

/**
 * Both requests carry a monotonic `id` and are answered under the same rule: a reply
 * whose id is not the latest is dropped by the client rather than rendered — but on a
 * counter of its own for each, so that a keystroke asking what `\alp` could be never
 * invalidates the conversion in flight beside it (PLAN §6.2, §6.13).
 *
 * `complete` answers the editor's completion chips. It carries the whole document
 * because the binding scans it for the user's own definitions, and it is never
 * debounced: it answers a keystroke that has already happened, and the answer is a
 * lookup in a table the session already holds.
 */
export type ToWorker =
  | {
      type: 'convert';
      id: number;
      text: string;
      options: OptionsPayload;
    }
  | {
      type: 'complete';
      id: number;
      /** The whole document, so the binding can scan it for the user's own definitions. */
      text: string;
      /** What has been typed after the backslash, without the backslash. */
      prefix: string;
      /** At most this many entries; the chip row shows a handful. */
      limit: number;
    };

export type FromWorker =
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; result: ConversionResult }
  | { type: 'completions'; id: number; items: Completion[] }
  | { type: 'fatal'; message: string };
