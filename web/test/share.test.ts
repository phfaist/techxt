/**
 * The share codec (web/PLAN.md §6.4): what a link carries, and what happens to a
 * link that arrives damaged.
 *
 * Node 18+ has `CompressionStream`, so the compressed path is exercised for real
 * here; the uncompressed fallback is reached by taking the global away, which is
 * exactly what an older browser looks like from inside this module.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  SHARE_LENGTH_LIMIT,
  SHARE_PREFIX_DEFLATE,
  SHARE_PREFIX_PLAIN,
  STORAGE_KEYS,
  canCompress,
  decodeShare,
  encodeShare,
  encodeShareSettingsOnly,
  loadState,
  sharedText,
} from '../src/state';
import type { StorageLike } from '../src/state';
import type { AppOptions } from '../src/types';

/** The two methods `StorageLike` needs, backed by a Map. */
function memoryStorage(): StorageLike {
  const map = new Map<string, string>();
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/** Every field of AppOptions, each set to something that is not its default. */
const EVERY_OPTION: AppOptions = {
  math: 'source',
  mathExpressionIn: 'none',
  matrixDelimiters: 'ascii',
  keepComments: true,
  headingStyle: 'underlined',
  footnoteStyle: 'skip',
  textFont: 'off',
  mathFont: 'fraktur',
  unknownMacro: 'keep-source',
  unknownEnv: 'skip',
  unknownSpecials: 'skip',
  recovery: 'strict',
  macroDefinitions: 'declared',
  wrap: 'off',
  todayMode: 'custom',
  todayCustom: 'March 3, 1999',
};

describe('encodeShare / decodeShare', () => {
  it('round-trips every option and a document with astral characters', async () => {
    const doc = '\\section{Grüße 𝕏 🎉}\nמה קורה?\n漢字\n';
    const fragment = await encodeShare({ v: 1, doc, opts: EVERY_OPTION });
    expect(fragment.startsWith(SHARE_PREFIX_DEFLATE)).toBe(canCompress());

    const decoded = await decodeShare(fragment);
    expect(decoded).toEqual({ v: 1, doc, opts: EVERY_OPTION });
  });

  it('serializes only the options that differ from the default', async () => {
    const fragment = await encodeShare({
      v: 1,
      doc: 'hi',
      // `wrap: 'soft'` and `todayMode: 'browser'` are the app's own defaults.
      opts: { wrap: 'soft', todayMode: 'browser', math: 'plain' },
    });
    const decoded = await decodeShare(fragment);
    expect(decoded?.opts).toEqual({ math: 'plain' });
  });

  it('carries the MathJax mode, which is the app’s answer and not the library’s', async () => {
    // A link reproduces the session it was made in, and the pane the sender was
    // looking at was typesetting (§5). The value survives; it is `resolveOptions`
    // that never lets it reach the binding, and options.test.ts holds it to that.
    const fragment = await encodeShare({ v: 1, doc: '$a+b$', opts: { math: 'mathjax' } });
    expect((await decodeShare(fragment))?.opts).toEqual({ math: 'mathjax' });
  });

  it('compresses: a repetitive document makes a much shorter link', async () => {
    if (!canCompress()) return;
    const doc = 'The same sentence, over and over. '.repeat(400);
    const fragment = await encodeShare({ v: 1, doc, opts: {} });
    expect(fragment.length).toBeLessThan(doc.length / 4);
    expect((await decodeShare(fragment))?.doc).toBe(doc);
  });

  it('falls back to an uncompressed fragment where CompressionStream is missing', async () => {
    vi.stubGlobal('CompressionStream', undefined);
    vi.stubGlobal('DecompressionStream', undefined);
    expect(canCompress()).toBe(false);

    const fragment = await encodeShare({ v: 1, doc: 'plain and simple', opts: { math: 'plain' } });
    expect(fragment.startsWith(SHARE_PREFIX_PLAIN)).toBe(true);
    expect(await decodeShare(fragment)).toEqual({
      v: 1,
      doc: 'plain and simple',
      opts: { math: 'plain' },
    });
  });

  it('reads an uncompressed fragment on a browser that can compress', async () => {
    vi.stubGlobal('CompressionStream', undefined);
    const fragment = await encodeShare({ v: 1, doc: 'from an old browser', opts: {} });
    vi.unstubAllGlobals();

    expect(canCompress()).toBe(true);
    expect((await decodeShare(fragment))?.doc).toBe('from an old browser');
  });

  it('cannot read a compressed fragment without DecompressionStream, and says so', async () => {
    if (!canCompress()) return;
    const fragment = await encodeShare({ v: 1, doc: 'compressed', opts: {} });
    vi.stubGlobal('DecompressionStream', undefined);
    expect(await decodeShare(fragment)).toBeNull();
  });

  it('accepts a fragment with or without its leading hash', async () => {
    const fragment = await encodeShare({ v: 1, doc: 'hash me', opts: {} });
    expect((await decodeShare(fragment.slice(1)))?.doc).toBe('hash me');
  });
});

describe('decodeShare: damage', () => {
  it('returns null rather than throwing, whatever it is handed', async () => {
    const nonsense = [
      undefined,
      null,
      '',
      '#',
      '#nothing-of-ours',
      '#d=',
      '#d=!!!!not base64!!!!',
      '#u=!!!!not base64!!!!',
      `${SHARE_PREFIX_PLAIN}${btoa('not json at all')}`,
      `${SHARE_PREFIX_PLAIN}${btoa('[1,2,3]')}`,
      `${SHARE_PREFIX_PLAIN}${btoa('"a string"')}`,
    ];
    for (const fragment of nonsense) {
      await expect(decodeShare(fragment)).resolves.toBeNull();
    }
  });

  it('survives a truncated compressed payload', async () => {
    const fragment = await encodeShare({ v: 1, doc: 'x'.repeat(4000), opts: {} });
    const half = fragment.slice(0, Math.floor(fragment.length / 2));
    await expect(decodeShare(half)).resolves.toBeNull();
  });

  it('ignores a version this build does not know', async () => {
    const payload = JSON.stringify({ v: 2, doc: 'from the future', opts: {} });
    await expect(decodeShare(SHARE_PREFIX_PLAIN + btoa(payload))).resolves.toBeNull();
    const missing = JSON.stringify({ doc: 'no version at all', opts: {} });
    await expect(decodeShare(SHARE_PREFIX_PLAIN + btoa(missing))).resolves.toBeNull();
  });

  it('drops option values it does not recognise but keeps the rest', async () => {
    const payload = JSON.stringify({
      v: 1,
      doc: 'still fine',
      opts: { math: 'interpretive-dance', headingStyle: 'plain', nonsense: 12 },
    });
    const decoded = await decodeShare(SHARE_PREFIX_PLAIN + btoa(payload));
    expect(decoded).toEqual({ v: 1, doc: 'still fine', opts: { headingStyle: 'plain' } });
  });

  it('accepts a payload whose document is missing', async () => {
    const payload = JSON.stringify({ v: 1, opts: { math: 'plain' } });
    expect(await decodeShare(SHARE_PREFIX_PLAIN + btoa(payload))).toEqual({
      v: 1,
      doc: '',
      opts: { math: 'plain' },
    });
  });
});

describe('the length the app decides on', () => {
  it('a long document exceeds the limit while its settings alone do not', async () => {
    // Pseudo-random text, so `deflate` has nothing to find and the fragment is as
    // long as a real 20 KB document's would be.
    let seed = 123_456_789;
    let doc = '';
    for (let index = 0; index < 20_000; index += 1) {
      seed = (seed * 1_103_515_245 + 12_345) & 0x7fff_ffff;
      doc += String.fromCharCode(33 + ((seed >>> 8) % 90));
    }
    const full = await encodeShare({ v: 1, doc, opts: EVERY_OPTION });
    expect(full.length).toBeGreaterThan(SHARE_LENGTH_LIMIT);

    const settingsOnly = await encodeShareSettingsOnly(EVERY_OPTION);
    expect(settingsOnly.length).toBeLessThan(SHARE_LENGTH_LIMIT);
    expect((await decodeShare(settingsOnly))?.doc).toBe('');
    expect((await decodeShare(settingsOnly))?.opts).toEqual(EVERY_OPTION);
  });
});

/* ------------------------------------------------------- the share target (W8) */

describe('sharedText', () => {
  it('reads the text a GET share_target carried', () => {
    expect(sharedText('?text=%5Csection%7BHi%7D')).toBe('\\section{Hi}');
    expect(sharedText('text=%5Cemph%7Bx%7D')).toBe('\\emph{x}');
  });

  it('is null for a share with nothing to convert', () => {
    // A share sheet sends whatever it has; only `text` is a document, so a share
    // carrying just a title or a URL falls through to the ordinary load path rather
    // than pasting a link into the editor.
    expect(sharedText('?title=Paper&url=https://example.org')).toBeNull();
    expect(sharedText('?text=')).toBeNull();
    expect(sharedText('?text=%20%20')).toBeNull();
    expect(sharedText('')).toBeNull();
    expect(sharedText(null)).toBeNull();
    expect(sharedText(undefined)).toBeNull();
  });
});

describe('loadState with a share target', () => {
  it('takes the shared document ahead of anything stored', async () => {
    const storage = memoryStorage();
    storage.setItem(STORAGE_KEYS.doc, 'the stored document');
    storage.setItem(STORAGE_KEYS.opts, JSON.stringify({ math: 'plain' }));

    const loaded = await loadState({ query: '?text=shared%20markup', storage });

    expect(loaded.source).toBe('share-target');
    expect(loaded.state.doc).toBe('shared markup');
    expect(loaded.firstVisit).toBe(false);
    // The document is the share's; the settings stay the ones this browser had.
    expect(loaded.state.opts.math).toBe('plain');
  });

  it('lets an ordinary visit through untouched', async () => {
    const storage = memoryStorage();
    storage.setItem(STORAGE_KEYS.doc, 'the stored document');
    const loaded = await loadState({ query: '?utm_source=whatever', storage });
    expect(loaded.source).toBe('storage');
    expect(loaded.state.doc).toBe('the stored document');
  });
});
