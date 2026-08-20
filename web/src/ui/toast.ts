import type { Toaster, ToastOptions } from './api';

/** Stub — replaced by the real implementation. */
export function initToast(_mount: HTMLElement): Toaster {
  return {
    show(_options: ToastOptions): void {},
    dismiss(): void {},
  };
}
