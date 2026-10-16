export interface RequestOptions { signal?: AbortSignal }
export type Disposer = () => void;
export interface UIAdapter {
  request(intent: string, data?: unknown, options?: RequestOptions): unknown | Promise<unknown>;
  subscribe(event: string, receive: (payload: unknown) => void): Disposer | Promise<Disposer>;
}

// Cancelling delivery does not imply cancelling a product operation.
// Product abort remains an explicit intent handled by the host.
export function createPort(adapter: UIAdapter) {
  if (typeof adapter?.request !== "function" || typeof adapter?.subscribe !== "function") {
    throw new TypeError("UI adapter requires request and subscribe");
  }
  const lifetime = new AbortController();
  const subscriptions = new Set<Disposer>();
  const assertActive = () => lifetime.signal.throwIfAborted();

  async function request(intent: string, data?: unknown, { signal }: RequestOptions = {}): Promise<unknown> {
    assertActive();
    signal?.throwIfAborted();
    const pending = new AbortController();
    const cancel = () => pending.abort(signal?.reason);
    const dispose = () => pending.abort(lifetime.signal.reason);
    signal?.addEventListener("abort", cancel, { once: true });
    lifetime.signal.addEventListener("abort", dispose, { once: true });
    let rejectAbort!: (reason: unknown) => void;
    const aborted = new Promise<never>((_, reject) => { rejectAbort = reject; });
    const onAbort = () => rejectAbort(pending.signal.reason);
    pending.signal.addEventListener("abort", onAbort, { once: true });
    try {
      return await Promise.race([
        Promise.resolve().then(() => {
          pending.signal.throwIfAborted();
          return adapter.request(intent, data, { signal: pending.signal });
        }),
        aborted,
      ]);
    } finally {
      signal?.removeEventListener("abort", cancel);
      lifetime.signal.removeEventListener("abort", dispose);
      pending.signal.removeEventListener("abort", onAbort);
    }
  }

  async function subscribe(event: string, receive: (payload: unknown) => void): Promise<Disposer> {
    assertActive();
    let active = true;
    const detach = await adapter.subscribe(event, payload => {
      if (active && !lifetime.signal.aborted) receive(payload);
    });
    if (typeof detach !== "function") throw new TypeError("UI subscription requires a disposer");
    const unsubscribe = () => {
      if (!active) return;
      active = false;
      subscriptions.delete(unsubscribe);
      detach();
    };
    if (lifetime.signal.aborted) {
      unsubscribe();
      assertActive();
    }
    subscriptions.add(unsubscribe);
    return unsubscribe;
  }

  function dispose(): void {
    if (lifetime.signal.aborted) return;
    lifetime.abort(new DOMException("UI host disposed", "AbortError"));
    const errors: unknown[] = [];
    for (const unsubscribe of [...subscriptions]) {
      try { unsubscribe(); } catch (error) { errors.push(error); }
    }
    if (errors.length) throw new AggregateError(errors, "UI subscription cleanup failed");
  }

  return Object.freeze({ request, subscribe, dispose });
}

// Product owns revision authority; failed rendering must not commit revision.
export function applyRevision<T extends { revision?: unknown }>(state: { revision: number }, snapshot: T, validate: (snapshot: T) => boolean, render: (snapshot: T) => void): boolean {
  const revision = Number(snapshot?.revision);
  if (!Number.isSafeInteger(revision) || revision <= state.revision || !validate(snapshot)) return false;
  render(snapshot);
  state.revision = revision;
  return true;
}
