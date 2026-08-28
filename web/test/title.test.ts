/**
 * What a document calls itself (web/PLAN.md §6.3, §6.10) — the regex Download and the
 * library both name a thing by.
 */

import { describe, expect, it } from 'vitest';

import { documentTitle, downloadName, firstLine, shorten, sourceFileName } from '../src/title';

describe('documentTitle', () => {
  it('takes the first \\title or \\section', () => {
    expect(documentTitle('\\title{Quantum error correction}\n\\section{Intro}')).toBe(
      'Quantum error correction',
    );
    expect(documentTitle('some prose\n\\section*{Results}\n')).toBe('Results');
  });

  it('skips the optional short-title argument', () => {
    expect(documentTitle('\\section[Short]{The long one}')).toBe('The long one');
  });

  it('gives up rather than guessing at a heading with structure in it', () => {
    // A brace inside is where this regex stops being honest; the caller falls back.
    expect(documentTitle('\\section{The \\emph{hard} case}')).toBeNull();
  });

  it('has nothing to say about a document with no heading', () => {
    expect(documentTitle('just prose')).toBeNull();
    expect(documentTitle('')).toBeNull();
  });
});

describe('firstLine', () => {
  it('is the first line with anything on it', () => {
    expect(firstLine('\n\n   \nhello there\nmore')).toBe('hello there');
  });

  it('reads a comment as a line, without its per cent sign', () => {
    expect(firstLine('% a note to self\nbody')).toBe('a note to self');
  });

  it('is null for a document with nothing in it', () => {
    expect(firstLine('   \n\t\n')).toBeNull();
  });

  it('leaves words rather than markup when the line is mostly formula', () => {
    expect(firstLine('The sum $\\sum_{k=1}^n 1/k^2$ converges.')).toBe(
      'The sum k=1 n 1/k 2 converges.',
    );
  });
});

describe('shorten', () => {
  it('leaves a short string alone', () => {
    expect(shorten('short', 80)).toBe('short');
  });

  it('cuts at a word where it can, and marks the cut', () => {
    const cut = shorten('alpha beta gamma delta epsilon', 20);
    expect(cut).toBe('alpha beta gamma…');
  });

  it('cuts mid-word rather than losing most of the text', () => {
    expect(shorten('a verylongsinglewordthatneverends', 12)).toBe('a verylongsi…');
  });
});

describe('downloadName', () => {
  it('is the heading as a slug, or the plain fallback', () => {
    expect(downloadName('\\title{Grüße, Welt!}')).toBe('grüße-welt.txt');
    expect(downloadName('\\section{Results and Discussion}')).toBe('results-and-discussion.txt');
    expect(downloadName('no heading here')).toBe('converted.txt');
  });

  it('never produces a name that is only punctuation', () => {
    expect(downloadName('\\title{!!!}')).toBe('converted.txt');
  });
});

describe('sourceFileName', () => {
  it('names an entry\'s own LaTeX after it', () => {
    expect(sourceFileName('Quantum error correction')).toBe('quantum-error-correction.tex');
    expect(sourceFileName('...')).toBe('document.tex');
  });
});
