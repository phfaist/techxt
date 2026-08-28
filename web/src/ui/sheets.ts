/**
 * The sheets over the tool — About, Install and the library (web/PLAN.md §6.8, §6.10).
 *
 * The page does not scroll: the tool is exactly one viewport tall, and everything
 * that is not the tool is a `<dialog>` over it. `showModal()` is doing real work
 * here — the top layer, the backdrop, the Escape key, the focus ring around the
 * dialog and the inertness of everything behind it are all the platform's, and none
 * of them is reimplemented below.
 *
 * The prose itself lives in `index.html`, as all the app's prose does. This module
 * owns only what prose cannot know: which sheet is open, and which set of install
 * steps is the one for the device reading them. The library sheet is the one whose
 * body is not prose at all — `ui/library-pane.ts` builds it into the mount the page
 * provides — but its shell, its title and its close button are the other two's, and
 * so is everything this module does with it.
 */

/** The sheets, by the `data-sheet` value that opens them and their element id. */
export type SheetId = 'about' | 'install' | 'library';

export interface SheetsInit {
  /** A sheet is opening: the caller's own popups should get out of the way. */
  onOpen?(id: SheetId): void;
}

export interface Sheets {
  open(id: SheetId): void;
  close(): void;
}

/* --------------------------------------------------------------- the install prompt */

/**
 * The `beforeinstallprompt` event, if this browser fired one.
 *
 * It is captured at module scope, not in {@link initSheets}, because Chrome fires it
 * once and early — around load, well before anyone opens the Install sheet — and an
 * event nobody was listening for is simply gone. `preventDefault()` is what stops the
 * browser's own mini-infobar and lets us keep the prompt for a button (§6.8).
 */
interface InstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

let deferredPrompt: InstallPromptEvent | null = null;
let promptListener: (() => void) | null = null;

if (typeof window !== 'undefined') {
  window.addEventListener('beforeinstallprompt', (event: Event) => {
    event.preventDefault();
    deferredPrompt = event as InstallPromptEvent;
    promptListener?.();
  });
  window.addEventListener('appinstalled', () => {
    deferredPrompt = null;
    promptListener?.();
  });
}

/* ------------------------------------------------------------------ the platform */

type Platform = 'ios' | 'android' | 'chromium' | 'safari' | 'firefox' | 'generic';

/**
 * Which set of steps to show first.
 *
 * This is user-agent sniffing, and it is the right tool exactly once: "tap Share,
 * then Add to Home Screen" is true of Safari on iOS and of nothing else, and no
 * feature test can tell you where the menu item is. Every other section stays one
 * disclosure away, so a wrong guess costs a click rather than the answer.
 */
function detectPlatform(): Platform {
  const ua = navigator.userAgent;
  // iPadOS 13+ reports itself as a Mac; the touch points are what give it away.
  const iOS =
    /iPad|iPhone|iPod/.test(ua) ||
    (/Macintosh/.test(ua) && typeof navigator.maxTouchPoints === 'number' && navigator.maxTouchPoints > 1);
  if (iOS) return 'ios';
  if (/Android/.test(ua)) return 'android';
  if (/Firefox\/|FxiOS/.test(ua)) return 'firefox';
  if (/Edg\/|Chrome\/|Chromium\//.test(ua)) return 'chromium';
  if (/Safari\//.test(ua)) return 'safari';
  return 'generic';
}

/** Whether the app is running as an installed app rather than in a browser tab. */
function isInstalled(): boolean {
  return (
    window.matchMedia?.('(display-mode: standalone)').matches === true ||
    // Safari on iOS predates `display-mode` and says it this way instead.
    (navigator as { standalone?: boolean }).standalone === true
  );
}

/* -------------------------------------------------------------------- the module */

export function initSheets(init: SheetsInit = {}): Sheets {
  const dialogs = new Map<SheetId, HTMLDialogElement>();
  for (const id of ['about', 'install', 'library'] as const) {
    const dialog = document.getElementById(`sheet-${id}`);
    if (dialog instanceof HTMLDialogElement) dialogs.set(id, dialog);
  }

  /** The control that opened the sheet, so focus can go back where it came from. */
  let opener: HTMLElement | null = null;

  for (const trigger of document.querySelectorAll<HTMLElement>('[data-sheet]')) {
    trigger.addEventListener('click', (event) => {
      // Some triggers are <a href="#"> inline in prose text, not <button>s.
      event.preventDefault();
      const id = trigger.dataset.sheet as SheetId;
      opener = trigger;
      open(id);
    });
  }

  for (const [, dialog] of dialogs) {
    for (const close of dialog.querySelectorAll<HTMLElement>('[data-close-sheet]')) {
      close.addEventListener('click', () => dialog.close());
    }
    // A click on the backdrop lands on the dialog element itself — its children stop
    // there — which is the whole test.
    dialog.addEventListener('click', (event: MouseEvent) => {
      if (event.target === dialog) dialog.close();
    });
    dialog.addEventListener('close', () => {
      opener?.focus();
      opener = null;
    });
  }

  buildInstallSheet();

  function open(id: SheetId): void {
    const dialog = dialogs.get(id);
    if (!dialog) return;
    for (const [, other] of dialogs) {
      if (other !== dialog && other.open) other.close();
    }
    init.onOpen?.(id);
    if (id === 'install') refreshInstallState();
    dialog.showModal();
    // A sheet always opens at its beginning, however far the last reader scrolled.
    dialog.querySelector('.sheet-body')?.scrollTo({ top: 0 });
  }

  /* ------------------------------------------------------------ the Install sheet */

  function buildInstallSheet(): void {
    const steps = document.getElementById('install-steps');
    const other = document.getElementById('install-other');
    const otherBody = document.getElementById('install-other-body');
    if (!steps || !other || !otherBody) return;

    const platform = detectPlatform();
    const sections = [...steps.querySelectorAll<HTMLElement>('.steps')];
    const mine = sections.find((section) => section.dataset.platform === platform);
    // Nothing matched — a browser we have no words for — so the generic section is
    // the one to show, and there is nothing left to hide.
    const shown = mine ?? sections.find((section) => section.dataset.platform === 'generic');

    for (const section of sections) {
      if (section === shown) {
        section.hidden = false;
      } else {
        section.hidden = false;
        otherBody.append(section);
      }
    }
    other.hidden = otherBody.children.length === 0;

    const button = document.getElementById('install-now');
    button?.addEventListener('click', () => {
      void runInstallPrompt();
    });
    promptListener = refreshInstallState;
    refreshInstallState();
  }

  /** Show the browser's own prompt, and say what it answered — in the sheet (§6.8). */
  async function runInstallPrompt(): Promise<void> {
    const prompt = deferredPrompt;
    const result = document.getElementById('install-result');
    if (!prompt) return;
    // A deferred prompt may be used once; the browser fires a fresh event if the
    // offer is still open afterwards.
    deferredPrompt = null;
    try {
      await prompt.prompt();
      const { outcome } = await prompt.userChoice;
      if (result) {
        result.textContent =
          outcome === 'accepted' ? 'Installing…' : 'No problem — the steps below still work.';
      }
    } catch {
      if (result) result.textContent = 'The browser would not show its install prompt.';
    }
    refreshInstallState();
  }

  function refreshInstallState(): void {
    const action = document.getElementById('install-action');
    const done = document.getElementById('install-done');
    const steps = document.getElementById('install-steps');
    const other = document.getElementById('install-other');
    const installed = isInstalled();
    if (action) action.hidden = installed || deferredPrompt === null;
    if (done) done.hidden = !installed;
    // Steps for installing something already installed are noise.
    if (steps) steps.hidden = installed;
    if (other) other.hidden = installed || other.querySelector('#install-other-body')?.children.length === 0;
  }

  return {
    open,
    close(): void {
      for (const [, dialog] of dialogs) if (dialog.open) dialog.close();
    },
  };
}
