/**
 * The worker's lifecycle and the request discipline (web/PLAN.md §6.2).
 *
 * This file knows nothing about the DOM: it is handed a factory that makes a worker
 * and a set of callbacks, and everything it decides — when to convert, which answer
 * to believe, when a conversion has taken long enough to admit to — is a function of
 * messages and time. That is what lets the sequencing be tested with a fake worker
 * and a fake clock instead of a browser and a stopwatch.
 *
 * The rules, in one place:
 *
 * - One worker, created eagerly, so the wasm module is loading while the user reads
 *   the page.
 * - Requests carry a monotonic id and only the latest answer counts. There is no
 *   cancellation in wasm, so a superseded conversion runs to completion and its
 *   result is discarded.
 * - {@link DEBOUNCE_MS} after the last keystroke; immediately on an option change, a
 *   paste, or Ctrl/Cmd+Enter.
 * - {@link CONVERTING_MS} in, the caller is told the conversion is slow;
 *   {@link CANCELLABLE_MS} in, that it can be cancelled. Cancelling is terminating —
 *   the only cancellation wasm offers — followed by a respawn.
 * - A `fatal` message takes the same respawn path and is surfaced, because a panic is
 *   a techxt bug and the caller has the reproduction (§6.4).
 */

import { systemClock } from './state';
import type { Clock } from './state';
import type { BusyState } from './ui/api';
import type { ConversionResult, FromWorker, OptionsPayload, ToWorker } from './worker/protocol';

/** Quiet period after the last keystroke before a conversion is issued. */
export const DEBOUNCE_MS = 120;
/** How long a conversion may run before the status line admits it is working. */
export const CONVERTING_MS = 300;
/** How long before it offers a Cancel button. */
export const CANCELLABLE_MS = 1500;

/** Whether the request may wait for the debounce, or is a deliberate "now". */
export type RequestMode = 'debounced' | 'immediate';

export interface ConvertClientInit {
  /** Makes a worker. A factory rather than a worker so respawning is possible — and
   *  so a test can hand over something that is not a worker at all. */
  createWorker(): Worker;
  /** The result of the *latest* request; superseded ones never arrive here. */
  onResult(result: ConversionResult): void;
  /** `'idle'` → `'converting'` → `'cancellable'`, and back to `'idle'` on an answer. */
  onBusy(state: BusyState): void;
  /** The worker is up; carries the embedded techxt version (§6.8). */
  onReady?(version: string): void;
  /** A panic, a trap, or a worker that failed to start. The instance is gone. */
  onFatal(message: string): void;
  clock?: Clock;
  debounceMs?: number;
  convertingMs?: number;
  cancellableMs?: number;
}

export class ConvertClient {
  private readonly init: ConvertClientInit;
  private readonly clock: Clock;
  private readonly debounceMs: number;
  private readonly convertingMs: number;
  private readonly cancellableMs: number;

  private worker: Worker;
  private lastId = 0;
  private inFlight: number | null = null;
  private pending: { text: string; options: OptionsPayload } | null = null;
  private debounceHandle: number | null = null;
  private convertingHandle: number | null = null;
  private cancellableHandle: number | null = null;
  private busyState: BusyState = 'idle';
  private workerVersion: string | null = null;
  private disposed = false;

  constructor(init: ConvertClientInit) {
    this.init = init;
    this.clock = init.clock ?? systemClock;
    this.debounceMs = init.debounceMs ?? DEBOUNCE_MS;
    this.convertingMs = init.convertingMs ?? CONVERTING_MS;
    this.cancellableMs = init.cancellableMs ?? CANCELLABLE_MS;
    this.worker = this.spawn();
  }

  /** The techxt version the worker reported, or `null` before it is up. */
  get version(): string | null {
    return this.workerVersion;
  }

  get busy(): BusyState {
    return this.busyState;
  }

  /** Whether a request has been sent and not yet answered. */
  get running(): boolean {
    return this.inFlight !== null;
  }

  /**
   * Ask for a conversion. `'debounced'` is what a keystroke wants; `'immediate'` is
   * what an option change, a paste and Ctrl/Cmd+Enter want (§6.2).
   */
  convert(text: string, options: OptionsPayload, mode: RequestMode = 'debounced'): void {
    if (this.disposed) return;
    this.pending = { text, options };
    if (mode === 'immediate') {
      this.flush();
      return;
    }
    if (this.debounceHandle !== null) this.clock.clearTimeout(this.debounceHandle);
    this.debounceHandle = this.clock.setTimeout(() => {
      this.debounceHandle = null;
      this.flush();
    }, this.debounceMs);
  }

  /** Send whatever is waiting on the debounce right now, if anything is. */
  flush(): void {
    if (this.disposed) return;
    if (this.debounceHandle !== null) {
      this.clock.clearTimeout(this.debounceHandle);
      this.debounceHandle = null;
    }
    const request = this.pending;
    if (!request) return;
    this.pending = null;
    this.lastId += 1;
    this.inFlight = this.lastId;
    const message: ToWorker = {
      type: 'convert',
      id: this.lastId,
      text: request.text,
      options: request.options,
    };
    this.startEscalation();
    this.worker.postMessage(message);
  }

  /**
   * Terminate the conversion in flight. wasm cannot be interrupted, so the worker is
   * killed and a fresh one spawned; the caller restores its last good output.
   */
  cancel(): void {
    if (this.disposed) return;
    this.respawn();
  }

  /** Stop everything. The worker is terminated and no further callback fires. */
  dispose(): void {
    this.disposed = true;
    this.clearTimers();
    this.pending = null;
    this.inFlight = null;
    try {
      this.worker.terminate();
    } catch {
      /* already gone */
    }
  }

  /* ------------------------------------------------------------------ internals */

  private spawn(): Worker {
    const worker = this.init.createWorker();
    // Every handler checks that the worker it belongs to is still the current one:
    // a terminated worker should be silent, but a *fake* one in a test need not be,
    // and a straggler must not be mistaken for an answer.
    worker.addEventListener('message', (event: MessageEvent<FromWorker>) => {
      if (worker !== this.worker || this.disposed) return;
      this.receive(event.data);
    });
    worker.addEventListener('error', (event: ErrorEvent) => {
      if (worker !== this.worker || this.disposed) return;
      // A module that failed to load, or an error that escaped the worker's own
      // handler: the same dead instance a `fatal` describes.
      this.respawn();
      this.init.onFatal(event.message || 'the conversion worker stopped');
    });
    return worker;
  }

  private receive(message: FromWorker | undefined): void {
    if (!message) return;
    switch (message.type) {
      case 'ready':
        this.workerVersion = message.version;
        this.init.onReady?.(message.version);
        return;
      case 'result':
        // Not the latest request: a conversion we superseded finished anyway.
        if (message.id !== this.lastId) return;
        this.inFlight = null;
        this.stopEscalation();
        this.setBusy('idle');
        this.init.onResult(message.result);
        return;
      case 'fatal':
        this.respawn();
        this.init.onFatal(message.message);
        return;
    }
  }

  private respawn(): void {
    this.clearTimers();
    // Bump the id so an answer already in the message queue is stale by definition.
    this.lastId += 1;
    this.inFlight = null;
    this.pending = null;
    try {
      this.worker.terminate();
    } catch {
      /* already gone */
    }
    this.worker = this.spawn();
    this.setBusy('idle');
  }

  private startEscalation(): void {
    this.stopEscalation();
    this.convertingHandle = this.clock.setTimeout(() => {
      this.convertingHandle = null;
      if (this.inFlight !== null) this.setBusy('converting');
    }, this.convertingMs);
    this.cancellableHandle = this.clock.setTimeout(() => {
      this.cancellableHandle = null;
      if (this.inFlight !== null) this.setBusy('cancellable');
    }, this.cancellableMs);
  }

  private stopEscalation(): void {
    if (this.convertingHandle !== null) this.clock.clearTimeout(this.convertingHandle);
    if (this.cancellableHandle !== null) this.clock.clearTimeout(this.cancellableHandle);
    this.convertingHandle = null;
    this.cancellableHandle = null;
  }

  private clearTimers(): void {
    this.stopEscalation();
    if (this.debounceHandle !== null) this.clock.clearTimeout(this.debounceHandle);
    this.debounceHandle = null;
  }

  private setBusy(state: BusyState): void {
    if (state === this.busyState) return;
    this.busyState = state;
    this.init.onBusy(state);
  }
}
