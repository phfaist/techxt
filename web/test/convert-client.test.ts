/**
 * Worker-protocol sequencing with a mocked worker (web/PLAN.md §6.2, §13).
 *
 * The client is the only part of the app that has to be right about *time* — a
 * debounce, two escalations and a respawn — so both the worker and the clock are
 * fakes here and every interval is stepped through rather than waited out.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  CANCELLABLE_MS,
  CONVERTING_MS,
  ConvertClient,
  DEBOUNCE_MS,
} from '../src/convert-client';
import type { Clock } from '../src/state';
import type { Completion, ConversionResult, FromWorker, ToWorker } from '../src/worker/protocol';

/* ------------------------------------------------------------------- fixtures */

class FakeClock implements Clock {
  private now = 0;
  private next = 1;
  private readonly timers = new Map<number, { at: number; run: () => void }>();

  setTimeout(handler: () => void, ms: number): number {
    const id = this.next++;
    this.timers.set(id, { at: this.now + ms, run: handler });
    return id;
  }

  clearTimeout(handle: number): void {
    this.timers.delete(handle);
  }

  advance(ms: number): void {
    const until = this.now + ms;
    for (;;) {
      const due = [...this.timers.entries()]
        .filter(([, timer]) => timer.at <= until)
        .sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      this.timers.delete(due[0]);
      this.now = due[1].at;
      due[1].run();
    }
    this.now = until;
  }
}

/** Enough of a `Worker` for the client: it records what it was sent and can answer. */
class FakeWorker {
  static readonly all: FakeWorker[] = [];
  readonly sent: ToWorker[] = [];
  terminated = false;
  private readonly listeners = new Map<string, Set<(event: unknown) => void>>();

  constructor() {
    FakeWorker.all.push(this);
  }

  postMessage(message: ToWorker): void {
    this.sent.push(message);
  }

  terminate(): void {
    this.terminated = true;
  }

  addEventListener(type: string, handler: (event: unknown) => void): void {
    const set = this.listeners.get(type) ?? new Set();
    set.add(handler);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, handler: (event: unknown) => void): void {
    this.listeners.get(type)?.delete(handler);
  }

  /** Deliver a message as the real worker would. */
  answer(message: FromWorker): void {
    for (const handler of this.listeners.get('message') ?? []) handler({ data: message });
  }

  fail(message: string): void {
    for (const handler of this.listeners.get('error') ?? []) handler({ message });
  }

  get last(): ToWorker | undefined {
    return this.sent[this.sent.length - 1];
  }
}

function result(text: string): ConversionResult {
  return {
    ok: true,
    text,
    ms: 1,
    diagnostics: [],
    suppressed: 0,
    truncated: false,
    regions: [],
  };
}

interface Harness {
  client: ConvertClient;
  clock: FakeClock;
  onResult: ReturnType<typeof vi.fn>;
  onBusy: ReturnType<typeof vi.fn>;
  onReady: ReturnType<typeof vi.fn>;
  onFatal: ReturnType<typeof vi.fn>;
  worker(): FakeWorker;
  workers: FakeWorker[];
}

function harness(): Harness {
  const clock = new FakeClock();
  const onResult = vi.fn();
  const onBusy = vi.fn();
  const onReady = vi.fn();
  const onFatal = vi.fn();
  const client = new ConvertClient({
    createWorker: () => new FakeWorker() as unknown as Worker,
    onResult,
    onBusy,
    onReady,
    onFatal,
    clock,
  });
  return {
    client,
    clock,
    onResult,
    onBusy,
    onReady,
    onFatal,
    workers: FakeWorker.all,
    worker: () => FakeWorker.all[FakeWorker.all.length - 1] as FakeWorker,
  };
}

beforeEach(() => {
  FakeWorker.all.length = 0;
});

/* ---------------------------------------------------------------------- tests */

describe('the worker itself', () => {
  it('is created eagerly, before anything is asked of it', () => {
    harness();
    expect(FakeWorker.all).toHaveLength(1);
  });

  it('reports the version it announces', () => {
    const app = harness();
    app.worker().answer({ type: 'ready', version: '0.1.0' });
    expect(app.onReady).toHaveBeenCalledWith('0.1.0');
    expect(app.client.version).toBe('0.1.0');
  });
});

describe('the debounce', () => {
  it('coalesces a burst of keystrokes into one conversion', () => {
    const app = harness();
    app.client.convert('a', {});
    app.clock.advance(50);
    app.client.convert('ab', {});
    app.clock.advance(50);
    app.client.convert('abc', {});
    expect(app.worker().sent).toHaveLength(0);

    app.clock.advance(DEBOUNCE_MS);
    expect(app.worker().sent).toHaveLength(1);
    expect(app.worker().last?.text).toBe('abc');
  });

  it('does not wait when the caller says now', () => {
    const app = harness();
    app.client.convert('typed', {}, 'debounced');
    app.client.convert('pasted', {}, 'immediate');
    expect(app.worker().sent).toHaveLength(1);
    expect(app.worker().last?.text).toBe('pasted');

    // The debounce that was pending must not fire a second conversion afterwards.
    app.clock.advance(1000);
    expect(app.worker().sent).toHaveLength(1);
  });

  it('gives every request a new id', () => {
    const app = harness();
    app.client.convert('one', {}, 'immediate');
    app.client.convert('two', {}, 'immediate');
    expect(app.worker().sent.map((message) => message.id)).toEqual([1, 2]);
  });
});

describe('stale answers', () => {
  it('drops the result of a conversion that was superseded', () => {
    const app = harness();
    app.client.convert('first', {}, 'immediate');
    app.client.convert('second', {}, 'immediate');

    app.worker().answer({ type: 'result', id: 1, result: result('stale') });
    expect(app.onResult).not.toHaveBeenCalled();

    app.worker().answer({ type: 'result', id: 2, result: result('fresh') });
    expect(app.onResult).toHaveBeenCalledTimes(1);
    expect(app.onResult.mock.calls[0]?.[0]).toMatchObject({ text: 'fresh' });
  });
});

describe('the escalation', () => {
  it('says nothing about a conversion that answers quickly', () => {
    const app = harness();
    app.client.convert('quick', {}, 'immediate');
    app.clock.advance(CONVERTING_MS - 1);
    app.worker().answer({ type: 'result', id: 1, result: result('done') });
    app.clock.advance(CANCELLABLE_MS);
    expect(app.onBusy).not.toHaveBeenCalled();
    expect(app.client.busy).toBe('idle');
  });

  it('admits to converting at 300 ms and offers a cancel at 1500 ms', () => {
    const app = harness();
    app.client.convert('slow', {}, 'immediate');

    app.clock.advance(CONVERTING_MS - 1);
    expect(app.onBusy).not.toHaveBeenCalled();
    app.clock.advance(1);
    expect(app.onBusy).toHaveBeenLastCalledWith('converting');

    app.clock.advance(CANCELLABLE_MS - CONVERTING_MS - 1);
    expect(app.onBusy).toHaveBeenLastCalledWith('converting');
    app.clock.advance(1);
    expect(app.onBusy).toHaveBeenLastCalledWith('cancellable');
    expect(app.client.busy).toBe('cancellable');
  });

  it('returns to idle when the answer finally arrives', () => {
    const app = harness();
    app.client.convert('slow', {}, 'immediate');
    app.clock.advance(CANCELLABLE_MS);
    app.worker().answer({ type: 'result', id: 1, result: result('eventually') });
    expect(app.onBusy).toHaveBeenLastCalledWith('idle');
    expect(app.client.busy).toBe('idle');
    expect(app.onResult).toHaveBeenCalledTimes(1);
  });

  it('restarts the escalation for each request', () => {
    const app = harness();
    app.client.convert('one', {}, 'immediate');
    app.clock.advance(CONVERTING_MS - 10);
    app.client.convert('two', {}, 'immediate');
    app.clock.advance(20);
    expect(app.onBusy).not.toHaveBeenCalled();
    app.clock.advance(CONVERTING_MS);
    expect(app.onBusy).toHaveBeenLastCalledWith('converting');
  });
});

describe('cancel', () => {
  it('terminates the worker, spawns another, and goes quiet', () => {
    const app = harness();
    app.client.convert('endless', {}, 'immediate');
    app.clock.advance(CANCELLABLE_MS);
    expect(app.client.busy).toBe('cancellable');

    const first = app.worker();
    app.client.cancel();
    expect(first.terminated).toBe(true);
    expect(app.workers).toHaveLength(2);
    expect(app.client.busy).toBe('idle');
    expect(app.onBusy).toHaveBeenLastCalledWith('idle');
    expect(app.client.running).toBe(false);
  });

  it('ignores an answer from the worker it threw away', () => {
    const app = harness();
    app.client.convert('endless', {}, 'immediate');
    const first = app.worker();
    app.client.cancel();
    first.answer({ type: 'result', id: 1, result: result('too late') });
    expect(app.onResult).not.toHaveBeenCalled();
  });

  it('converts again on the new worker', () => {
    const app = harness();
    app.client.convert('endless', {}, 'immediate');
    app.client.cancel();
    app.client.convert('a fresh start', {}, 'immediate');

    const second = app.worker();
    expect(second.sent).toHaveLength(1);
    expect(second.last?.text).toBe('a fresh start');
    second.answer({
      type: 'result',
      id: second.last?.id ?? -1,
      result: result('converted'),
    });
    expect(app.onResult).toHaveBeenCalledTimes(1);
  });
});

describe('fatal', () => {
  it('respawns and tells the caller, so it can invite a bug report', () => {
    const app = harness();
    app.client.convert('a panic waiting to happen', {}, 'immediate');
    const first = app.worker();

    first.answer({ type: 'fatal', message: 'panicked at src/lib.rs:1' });
    expect(app.onFatal).toHaveBeenCalledWith('panicked at src/lib.rs:1');
    expect(first.terminated).toBe(true);
    expect(app.workers).toHaveLength(2);
    expect(app.client.busy).toBe('idle');
  });

  it('does not retry the document that killed the worker', () => {
    const app = harness();
    app.client.convert('the bad one', {}, 'immediate');
    app.worker().answer({ type: 'fatal', message: 'trap' });
    app.clock.advance(10_000);
    expect(app.worker().sent).toHaveLength(0);
  });

  it('treats a worker that fails to start the same way', () => {
    const app = harness();
    app.worker().fail('failed to load the module');
    expect(app.onFatal).toHaveBeenCalledWith('failed to load the module');
    expect(app.workers).toHaveLength(2);
  });

  it('stops the escalation when the worker dies mid-conversion', () => {
    const app = harness();
    app.client.convert('doomed', {}, 'immediate');
    app.clock.advance(CONVERTING_MS);
    expect(app.onBusy).toHaveBeenLastCalledWith('converting');
    app.worker().answer({ type: 'fatal', message: 'boom' });
    expect(app.onBusy).toHaveBeenLastCalledWith('idle');
    app.clock.advance(10_000);
    expect(app.onBusy).toHaveBeenLastCalledWith('idle');
  });
});

describe('dispose', () => {
  it('terminates the worker and stops answering', () => {
    const app = harness();
    const worker = app.worker();
    app.client.convert('pending', {}, 'immediate');
    app.client.dispose();
    expect(worker.terminated).toBe(true);

    worker.answer({ type: 'result', id: 1, result: result('ignored') });
    app.client.convert('ignored too', {}, 'immediate');
    app.clock.advance(10_000);
    expect(app.onResult).not.toHaveBeenCalled();
    expect(worker.sent).toHaveLength(1);
  });
});

describe('completions', () => {
  const items: Completion[] = [
    { name: 'alpha', kind: 'macro', replacement: 'α', arity: 0, fromDocument: false },
  ];

  it('asks at once — a keystroke has already happened, and the answer is a lookup', () => {
    const app = harness();
    app.client.complete('doc', 'alp', 6, vi.fn());
    expect(app.worker().last).toEqual({ type: 'complete', id: 1, text: 'doc', prefix: 'alp', limit: 6 });
  });

  it('counts on its own, so a completion never invalidates the conversion beside it', () => {
    const app = harness();
    app.client.convert('doc', {}, 'immediate');
    const onItems = vi.fn();
    app.client.complete('doc', 'alp', 6, onItems);

    // The conversion's own id is untouched by the completion that overtook it.
    app.worker().answer({ type: 'result', id: 1, result: result('out') });
    expect(app.onResult).toHaveBeenCalledTimes(1);
    app.worker().answer({ type: 'completions', id: 1, items });
    expect(onItems).toHaveBeenCalledWith(items);
  });

  it('answers only the latest question', () => {
    const app = harness();
    const first = vi.fn();
    const second = vi.fn();
    app.client.complete('doc', 'al', 6, first);
    app.client.complete('doc', 'alp', 6, second);

    app.worker().answer({ type: 'completions', id: 1, items });
    expect(first).not.toHaveBeenCalled();
    app.worker().answer({ type: 'completions', id: 2, items });
    expect(second).toHaveBeenCalledWith(items);
  });

  it('forgets whoever was waiting when the worker is replaced', () => {
    const app = harness();
    const onItems = vi.fn();
    app.client.complete('doc', 'alp', 6, onItems);
    app.worker().fail('gone');
    // The new worker's first completion has a new id; the old answer is stale by
    // definition and its callback is never called.
    app.workers[0]?.answer({ type: 'completions', id: 1, items });
    expect(onItems).not.toHaveBeenCalled();
  });
});
