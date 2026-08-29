/**
 * The dynamic half of the below-the-fold section (web/PLAN.md §6.8).
 *
 * The prose, the snippets and the links are static and live in `index.html`. Three
 * things cannot: the embedded techxt version, which only the worker knows; the font
 * credits, which are a function of the registry in `fonts.ts` and would rot the
 * moment a face was added or removed; and the update control, which is a button
 * whose whole text is a report on something only the service worker knows (§9).
 *
 * Everything here is built with `createElement` and `textContent`. The page never
 * assigns `innerHTML`, and a credit line is not the place to make an exception.
 */

import { FONTS, INTERFACE_FONT } from './fonts';

/**
 * What one press of **Check for updates** found (§9).
 *
 * `main.ts` owns the service worker and answers this; this module only says it. The
 * two middle answers are different facts and are worth different words: `'ready'` is
 * a new version already on the device, waiting for nothing but a reload, and
 * `'downloading'` is one the browser has only just started fetching — which is what
 * the check normally returns when it finds something, because the fetch is a moment
 * behind the question.
 */
export type UpdateCheck = 'ready' | 'downloading' | 'current' | 'failed';

export interface AboutInit {
  /**
   * Ask the server whether it has a newer build, now.
   *
   * The section stays hidden until {@link AboutSection.enableUpdates} says there is a
   * service worker to ask, so this is never called without one.
   */
  checkForUpdate(): Promise<UpdateCheck>;
  /** Take the version that is ready. `main.ts` decides what a reload involves. */
  onReload(): void;
}

export interface AboutSection {
  /** Fill in the version the worker reported (§6.2's `ready` message). */
  setVersion(version: string): void;
  /**
   * There is a service worker, so there is something to ask: show the section.
   *
   * Hidden until then rather than shown-and-apologetic. Without a worker — `vite dev`,
   * a browser that has none, a registration that failed — the button could only ever
   * report that it could not look, and a control that cannot do its one job is worse
   * than no control.
   */
  enableUpdates(): void;
  /** A new version has arrived, however it was noticed: say so, and offer the reload. */
  updateReady(): void;
}

/** One sentence per answer, in the fine print beside the button. */
const UPDATE_WORDS: Record<UpdateCheck, string> = {
  ready: 'A new version is ready.',
  downloading: 'A new version is on its way — you will be offered a reload when it is ready.',
  current: 'This is the latest version.',
  failed: 'Could not check just now — this device may be offline.',
};

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

/* ----------------------------------------------------------------- the updates */

/**
 * Wire `#update-check`: one button, one line of fine print beside it (§9).
 *
 * The button has two jobs and is only ever doing one of them — it asks the question
 * until the answer is yes, and from then on it *is* the answer, because a note saying
 * a new version is ready with no way to take it would be a tease. The reload toast
 * says the same thing and follows the sheet (`ui/toast.ts`), but it can be dismissed,
 * and a user who came here to look for an update should find one here.
 *
 * Returns the two handles `initAbout` hands on: show the section, and go to the ready
 * state without having been asked.
 */
function initUpdates(
  root: ParentNode,
  init: AboutInit,
): { enable(): void; ready(): void } {
  const section = root.querySelector<HTMLElement>('#update-check');
  const button = root.querySelector<HTMLButtonElement>('#check-update');
  const note = root.querySelector('#update-note');
  if (!section || !button || !note) return { enable: () => {}, ready: () => {} };

  /** Whether the button has stopped being a question and become the reload. */
  let isReady = false;
  /** Whether a check is in flight; a second press must not start another. */
  let checking = false;

  // Arrow constants rather than declarations, so that the three elements the guard
  // above proved to exist are still known to exist in here.
  const becomeReady = (): void => {
    isReady = true;
    checking = false;
    button.disabled = false;
    button.textContent = 'Reload';
    button.classList.add('btn-accent');
    note.textContent = UPDATE_WORDS.ready;
  };

  const settle = (outcome: UpdateCheck): void => {
    checking = false;
    button.disabled = false;
    // The version arrived while this check was still in flight — which is the *normal*
    // way a `'downloading'` ends. The answer in hand is the older news of the two, and
    // writing it over the offer of a reload would take the offer away.
    if (isReady) return;
    if (outcome === 'ready') becomeReady();
    else note.textContent = UPDATE_WORDS[outcome];
  };

  button.addEventListener('click', () => {
    if (isReady) {
      init.onReload();
      return;
    }
    if (checking) return;
    checking = true;
    button.disabled = true;
    note.textContent = 'Checking…';
    // A rejection is the same news as `'failed'` — the question could not be put —
    // and an unhandled rejection is not a way to tell anybody anything.
    init.checkForUpdate().then(settle, () => settle('failed'));
  });

  return {
    enable(): void {
      section.hidden = false;
    },
    ready: becomeReady,
  };
}

/**
 * Fill `#font-credits` from the registry and return a handle for the version.
 *
 * `root` is a parameter so the section can be rendered into a fragment in a test or a
 * future page; it defaults to the document the app is running in.
 */
export function initAbout(init: AboutInit, root: ParentNode = document): AboutSection {
  const credits = root.querySelector('#font-credits');
  if (credits) {
    const items = FONTS
      // The system entry ships no file and credits nobody: there is nothing to say.
      .filter((font) => font.credit.length > 0)
      .map((font) => creditItem(font.credit, font.licence, font.licenceFile));
    credits.replaceChildren(...items);
  }

  // The interface face is credited beside them, in its own line: it is not one of the
  // five, and putting it in the same list would say it was (§8.7).
  const uiCredit = root.querySelector('#ui-font-credit');
  if (uiCredit) {
    uiCredit.replaceChildren(
      creditItem(INTERFACE_FONT.credit, INTERFACE_FONT.licence, INTERFACE_FONT.licenceFile),
    );
  }

  const updates = initUpdates(root, init);

  return {
    setVersion(version: string): void {
      const element = root.querySelector('#techxt-version');
      if (!element) return;
      const trimmed = version.trim();
      element.textContent = trimmed ? (/^\d/.test(trimmed) ? `v${trimmed}` : trimmed) : '—';
    },
    enableUpdates: updates.enable,
    updateReady: updates.ready,
  };
}
