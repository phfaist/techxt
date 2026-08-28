/**
 * Toasts (web/PLAN.md §6.7, §9).
 *
 * One message at a time, with at most one action — "Undo" after a Load or a library
 * Delete, "Reload" after an update, "Report" after a `fatal`. Showing a second toast
 * replaces the first: two stacked notices about a document nobody asked to be
 * notified about is noise, and the caller always has the more important thing to say
 * last.
 *
 * **A toast follows the modal it belongs to.** A `<dialog>` opened with
 * `showModal()` makes the rest of the document inert, so a toast raised from inside
 * the library sheet would be drawn under it and, worse, its Undo would not answer a
 * click. Nothing in the top layer escapes that — a popover shown over a modal is
 * painted above it and is still inert — so the toast is *moved into* the open dialog
 * for as long as one is open, and moved home when it closes. It is `position: fixed`,
 * so it lands in exactly the same place either way.
 *
 * The module owns no app state. It never reads the clipboard, the worker or storage;
 * it shows what it is given and calls back when the user presses the action.
 */

import type { ToastOptions, Toaster } from './api';

const DEFAULT_TIMEOUT = 4000;

/**
 * Build the toast region into `mount` (`#toast-mount`).
 *
 * Two live regions are created up front, one polite and one assertive, because a live
 * region has to exist before the text lands in it to be announced reliably. `tone`
 * picks which one the message is inserted into.
 */
export function initToast(mount: HTMLElement): Toaster {
  mount.classList.add('toast-mount');

  const polite = makeRegion('status', 'polite');
  const assertive = makeRegion('alert', 'assertive');
  mount.append(polite, assertive);

  /** Where the toast lives when no sheet is open — the page's own mount point. */
  const home = mount.parentElement ?? document.body;

  let timer = 0;
  let current: HTMLElement | null = null;

  /** The modal on top, if there is one. `:modal` where the browser knows it. */
  function openModal(): HTMLDialogElement | null {
    for (const dialog of document.querySelectorAll('dialog')) {
      try {
        if (dialog.matches(':modal')) return dialog;
      } catch {
        if (dialog.open) return dialog;
      }
    }
    return null;
  }

  /**
   * Put the toast where it will be seen and can be pressed.
   *
   * The regions move with it, and the message is inserted after the move, which is
   * the order a live region needs to announce reliably.
   */
  function reparent(): void {
    const modal = openModal();
    const target: HTMLElement = modal ?? home;
    if (mount.parentElement === target) return;
    target.append(mount);
    // A dialog that closes takes its children out of the layout with it, so the
    // toast comes home rather than disappearing with the sheet.
    modal?.addEventListener(
      'close',
      () => {
        if (mount.parentElement === modal) home.append(mount);
      },
      { once: true },
    );
  }

  function clearTimer(): void {
    if (timer) {
      window.clearTimeout(timer);
      timer = 0;
    }
  }

  function dismiss(): void {
    clearTimer();
    if (current) {
      current.remove();
      current = null;
    }
  }

  function show(options: ToastOptions): void {
    dismiss();
    reparent();

    const tone = options.tone ?? 'status';
    const box = document.createElement('div');
    box.className = 'toast';
    box.dataset.tone = tone;

    const message = document.createElement('p');
    message.className = 'toast-message';
    message.textContent = options.message;
    box.append(message);

    if (options.action) {
      const action = options.action;
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'btn btn-accent toast-action';
      button.textContent = action.label;
      button.addEventListener('click', () => {
        dismiss();
        action.onSelect();
      });
      box.append(button);
    }

    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'btn btn-quiet toast-close';
    close.setAttribute('aria-label', 'Dismiss this message');
    close.append(span('toast-x', '×'));
    close.addEventListener('click', dismiss);
    box.append(close);

    (tone === 'alert' ? assertive : polite).append(box);
    current = box;

    // One frame later so the entrance transition has a starting state to leave.
    requestAnimationFrame(() => box.classList.add('is-shown'));

    const timeout = options.timeoutMs ?? DEFAULT_TIMEOUT;
    if (timeout > 0) {
      timer = window.setTimeout(dismiss, timeout);
    }
  }

  return { show, dismiss };
}

function makeRegion(role: string, live: string): HTMLDivElement {
  const region = document.createElement('div');
  region.className = 'toast-region';
  region.setAttribute('role', role);
  region.setAttribute('aria-live', live);
  region.setAttribute('aria-atomic', 'true');
  return region;
}

function span(className: string, text: string): HTMLSpanElement {
  const node = document.createElement('span');
  node.className = className;
  node.setAttribute('aria-hidden', 'true');
  node.textContent = text;
  return node;
}
