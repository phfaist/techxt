/**
 * The dynamic half of the below-the-fold section (web/PLAN.md §6.8).
 *
 * The prose, the snippets and the links are static and live in `index.html`. Two
 * things cannot: the embedded techxt version, which only the worker knows, and the
 * font credits, which are a function of the registry in `fonts.ts` and would rot the
 * moment a face was added or removed.
 *
 * Everything here is built with `createElement` and `textContent`. The page never
 * assigns `innerHTML`, and a credit line is not the place to make an exception.
 */

import { FONTS } from './fonts';

export interface AboutSection {
  /** Fill in the version the worker reported (§6.2's `ready` message). */
  setVersion(version: string): void;
}

/** Where the licence files the packaging script copies verbatim end up (§8.4). */
const LICENCE_PATH = 'fonts/licences/';

function baseUrl(): string {
  const base = import.meta.env?.BASE_URL ?? '/';
  return base.endsWith('/') ? base : `${base}/`;
}

function creditItem(credit: string, licence: string, licenceFile: string | null): HTMLLIElement {
  const item = document.createElement('li');
  item.appendChild(document.createTextNode(credit));
  if (!licence) return item;
  item.appendChild(document.createTextNode(' — '));
  if (licenceFile) {
    const link = document.createElement('a');
    link.href = `${baseUrl()}${LICENCE_PATH}${licenceFile}`;
    link.textContent = licence;
    link.rel = 'noopener';
    item.appendChild(link);
  } else {
    item.appendChild(document.createTextNode(licence));
  }
  return item;
}

/**
 * Fill `#font-credits` from the registry and return a handle for the version.
 *
 * `root` is a parameter so the section can be rendered into a fragment in a test or a
 * future page; it defaults to the document the app is running in.
 */
export function initAbout(root: ParentNode = document): AboutSection {
  const credits = root.querySelector('#font-credits');
  if (credits) {
    const items = FONTS
      // The system entry ships no file and credits nobody: there is nothing to say.
      .filter((font) => font.credit.length > 0)
      .map((font) => creditItem(font.credit, font.licence, font.licenceFile));
    credits.replaceChildren(...items);
  }

  return {
    setVersion(version: string): void {
      const element = root.querySelector('#techxt-version');
      if (!element) return;
      const trimmed = version.trim();
      element.textContent = trimmed ? (/^\d/.test(trimmed) ? `v${trimmed}` : trimmed) : '—';
    },
  };
}
