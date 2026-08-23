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
}

/* -------------------------------------------------------------- the messages */

export type ToWorker = {
  type: 'convert';
  id: number;
  text: string;
  options: OptionsPayload;
};

export type FromWorker =
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; result: ConversionResult }
  | { type: 'fatal'; message: string };
