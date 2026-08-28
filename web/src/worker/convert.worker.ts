/**
 * The conversion worker (web/PLAN.md §6.2).
 *
 * It owns the wasm instance and does exactly three things: load the module, answer
 * `convert` and `complete` requests from the one `Session` it keeps, and — if anything
 * at all goes wrong — say so once and stop. A panic or a stack overflow leaves a wasm
 * instance unusable (§4.6), so there is no continuing after a `fatal`: the client
 * discards this worker and spawns another.
 *
 * The message handler is installed *before* the module is loaded, because a request
 * can arrive during that load. Only the most recent conversion is kept: the client drops
 * every result but the latest anyway, so an older queued request has no answer worth
 * computing. A completion arriving that early is dropped outright rather than queued —
 * it answers a keystroke, and a keystroke that has been waiting for the module to load
 * has long since been followed by another.
 */

import init, { Session, techxt_version } from '../../crate/pkg/techxt_web.js';
import type { Completion, ConversionResult, FromWorker, ToWorker } from './protocol';

// `lib.dom` and `lib.webworker` both declare `self`; this is the worker's.
const ctx = self as unknown as DedicatedWorkerGlobalScope;

/** The request that is worth queueing while the module loads; see the note above. */
type ConvertRequest = Extract<ToWorker, { type: 'convert' }>;
type CompleteRequest = Extract<ToWorker, { type: 'complete' }>;

let session: Session | null = null;
let ready = false;
/** Set by the first failure, and never unset — see the note about panics above. */
let broken = false;
let queued: ConvertRequest | null = null;

function post(message: FromWorker): void {
  ctx.postMessage(message);
}

function describe(error: unknown): string {
  if (error instanceof Error) return error.stack ?? `${error.name}: ${error.message}`;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

function fail(error: unknown): void {
  if (broken) return;
  broken = true;
  session = null;
  post({ type: 'fatal', message: describe(error) });
}

function handle(request: ConvertRequest): void {
  if (broken || !session) return;
  try {
    const result = session.convert(request.text, request.options) as ConversionResult;
    post({ type: 'result', id: request.id, result });
  } catch (error) {
    // Both a Rust panic and a trap arrive here, and both mean this instance is done.
    fail(error);
  }
}

/**
 * A completion request: the same `Session`, the same one-failure rule, and no queue.
 *
 * The two requests share this worker on purpose (§6.13). Completion is a table lookup —
 * microseconds after the first call, which builds the table — so a second wasm instance
 * to answer it would be a megabyte of memory for a nicety. What they do share is the
 * thread, so a completion issued while a conversion is running waits for it; the client
 * is where that is measured and, if it ever matters, cached.
 */
function suggest(request: CompleteRequest): void {
  if (broken || !session) return;
  try {
    const items = session.complete(request.text, request.prefix, request.limit) as Completion[];
    post({ type: 'completions', id: request.id, items });
  } catch (error) {
    fail(error);
  }
}

ctx.addEventListener('message', (event: MessageEvent<ToWorker>) => {
  const message = event.data;
  if (!message || broken) return;
  if (message.type === 'complete') {
    if (ready) suggest(message);
    return;
  }
  if (message.type !== 'convert') return;
  if (!ready) {
    queued = message;
    return;
  }
  handle(message);
});

// Top-level await: this is a module worker (`{ type: 'module' }`, `worker.format:
// 'es'` in vite.config.ts), so the glue's `init()` can simply be awaited here.
try {
  await init();
  session = new Session();
  ready = true;
  post({ type: 'ready', version: techxt_version() });
  if (queued) {
    const request = queued;
    queued = null;
    handle(request);
  }
} catch (error) {
  fail(error);
}
