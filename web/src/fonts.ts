/**
 * The display-font registry (web/PLAN.md §8).
 *
 * A display font is CSS and nothing else: it changes how the converted text *looks*,
 * never what it says. The Unicode character styles of the More-options panel
 * (`textFont`, `mathFont`) are a different thing entirely and live in the binding.
 *
 * Each entry is a **chain**, not a face (§8.2). techxt copies input text straight
 * through, so a document can contain any script at all; browsers fall back per glyph,
 * so the chosen face renders what it has and the platform supplies the rest. Every
 * chain therefore ends in a system stack that covers CJK and emoji.
 *
 * Loading is the browser's own laziness (§8.3): the `@font-face` rules in
 * `styles.css` declare all five display faces, and a declared face that nothing applies is
 * never fetched. There is no loader here beyond `document.fonts.load`, which exists
 * only so the pane header can say "loading font…".
 */

/** The identifier persisted in `localStorage` and in a share link. */
export type FontId = 'julia' | 'firamath' | 'lmmath' | 'stix' | 'libertinus' | 'system';

export interface FontEntry {
  id: FontId;
  /** What the selector shows. */
  label: string;
  /**
   * The `font-family` of the `@font-face` rule in `styles.css`, or `null` for the
   * system entry, which declares no face and downloads nothing.
   */
  family: string | null;
  /** The full CSS chain, face first and platform coverage last (§8.2). */
  stack: string;
  /** One line for the About section; empty for the system entry. */
  credit: string;
  /** Licence name, and the file under `fonts/licences/` that carries it verbatim. */
  licence: string;
  licenceFile: string | null;
}

/**
 * The platform tails. A serif face falls back to a serif stack so the mix a
 * multi-script document produces does not clash (§8.2).
 */
const MONO_TAIL =
  'ui-monospace, SFMono-Regular, Menlo, Consolas, "DejaVu Sans Mono", ' +
  '"Noto Sans Mono CJK JP", "Noto Sans CJK JP", "Noto Sans", ' +
  '"Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", monospace';

const SANS_TAIL =
  'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, ' +
  '"Noto Sans CJK JP", "Noto Sans", ' +
  '"Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif';

const SERIF_TAIL =
  'ui-serif, Georgia, "Times New Roman", "Noto Serif CJK JP", "Noto Serif", ' +
  '"Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", serif';

/**
 * The five self-hosted faces plus the system stack, in the order the selector shows
 * them. Adding a face here without the conversation of Appendix D is out of scope:
 * the app's weight is a design property.
 */
export const FONTS: readonly FontEntry[] = [
  {
    id: 'julia',
    label: 'JuliaMono',
    family: 'JuliaMono Web',
    stack: `"JuliaMono Web", ${MONO_TAIL}`,
    credit: 'JuliaMono by Cormullion',
    licence: 'SIL Open Font License 1.1',
    licenceFile: 'JuliaMono-OFL.txt',
  },
  {
    id: 'firamath',
    label: 'Fira Math',
    family: 'Fira Math Web',
    stack: `"Fira Math Web", ${SANS_TAIL}`,
    credit: 'Fira Math by Xiangdong Zeng, after Mozilla’s Fira Sans',
    licence: 'SIL Open Font License 1.1',
    licenceFile: 'FiraMath-OFL.txt',
  },
  {
    id: 'lmmath',
    label: 'Latin Modern',
    family: 'Latin Modern Math Web',
    stack: `"Latin Modern Math Web", ${SERIF_TAIL}`,
    credit: 'Latin Modern Math by GUST e-foundry, after Knuth’s Computer Modern',
    licence: 'GUST Font License (LPPL-like)',
    licenceFile: 'LatinModernMath-GUST-FONT-LICENSE.txt',
  },
  {
    id: 'stix',
    label: 'STIX Two',
    family: 'STIX Two Math Web',
    stack: `"STIX Two Math Web", ${SERIF_TAIL}`,
    credit: 'STIX Two Math by the STI Pub companies',
    licence: 'SIL Open Font License 1.1',
    licenceFile: 'STIXTwoMath-OFL.txt',
  },
  {
    id: 'libertinus',
    label: 'Libertinus',
    family: 'Libertinus Math Web',
    stack: `"Libertinus Math Web", ${SERIF_TAIL}`,
    credit: 'Libertinus Math by Khaled Hosny, after Philipp Poll’s Linux Libertine',
    licence: 'SIL Open Font License 1.1',
    licenceFile: 'LibertinusMath-OFL.txt',
  },
  {
    id: 'system',
    label: 'System',
    family: null,
    stack: MONO_TAIL,
    credit: '',
    licence: '',
    licenceFile: null,
  },
];

/**
 * The face the app's own chrome is set in (§8.7).
 *
 * It is **not** a display font and deliberately not part of {@link FONTS}: it never
 * renders converted text, it is not offered in the selector, and no state ever names
 * it. It is here only because the credit in the About sheet should come from the same
 * place as the others rather than be typed into the prose and left to rot.
 */
export const INTERFACE_FONT = {
  /** The `@font-face` family `styles.css` declares for it. */
  family: 'Commissioner Web',
  credit: 'Commissioner by Kostas Bartsokas',
  licence: 'SIL Open Font License 1.1',
  licenceFile: 'Commissioner-OFL.txt',
} as const;

/** The default face: a monospace grid is what the layout engine's columns assume. */
export const DEFAULT_FONT: FontId = 'julia';

/** The size range of the control beside the selector, in CSS pixels. */
export const MIN_SIZE = 12;
export const MAX_SIZE = 20;
export const DEFAULT_SIZE = 15;

const BY_ID = new Map(FONTS.map((f) => [f.id, f]));

/** The registry entry for `id`, or the default face's if `id` is not one of ours. */
export function fontEntry(id: string): FontEntry {
  return BY_ID.get(id as FontId) ?? BY_ID.get(DEFAULT_FONT)!;
}

/** Whether `id` is a font this build ships. */
export function isFontId(id: unknown): id is FontId {
  return typeof id === 'string' && BY_ID.has(id as FontId);
}

/**
 * Apply a face to the panes by setting the two custom properties `styles.css` reads,
 * and resolve once the face is actually available.
 *
 * Applying is what triggers the download; the returned promise is only for the
 * "loading font…" state. It never rejects — a face that fails to arrive leaves the
 * chain of §8.2 rendering, which is a fine outcome and not an error to report.
 */
export async function applyFont(id: FontId, sizePx: number, root: HTMLElement = document.documentElement): Promise<void> {
  const entry = fontEntry(id);
  root.style.setProperty('--pane-font', entry.stack);
  root.style.setProperty('--pane-size', `${sizePx}px`);
  if (!entry.family || typeof document.fonts?.load !== 'function') return;
  try {
    await document.fonts.load(`${sizePx}px "${entry.family}"`);
  } catch {
    /* the chain renders; nothing to report */
  }
}

/**
 * Fetch every self-hosted face, for someone about to lose their network ("keep all
 * fonts offline", §8.3). Resolves when all of them have settled, however they did.
 */
export async function preloadAllFonts(sizePx: number = DEFAULT_SIZE): Promise<void> {
  if (typeof document.fonts?.load !== 'function') return;
  await Promise.allSettled(
    FONTS.filter((f) => f.family).map((f) => document.fonts.load(`${sizePx}px "${f.family}"`)),
  );
}
