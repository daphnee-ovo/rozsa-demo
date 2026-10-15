import { sessionReply, historyReply, sessionTarget, ProductFailure, transportFailure, type ProductBridge, type SessionListEntry, type SessionTarget, type SessionSnapshot } from "../main/session-contract.js";

export interface ReceiveOptions { signal?: AbortSignal }
// Cancels UI delivery only. Neither AbortSignal nor an abort intent crosses IPC.
function receive<T>(operation: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  const cancelled = () => new ProductFailure({ kind: "ui_cancelled", message: "UI response reception cancelled" }, { cause: signal?.reason });
  if (signal?.aborted) return Promise.reject(cancelled());
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(cancelled());
    signal?.addEventListener("abort", abort, { once: true });
    Promise.resolve().then(() => {
      if (signal?.aborted) throw cancelled();
      return operation();
    }).then(resolve, error => reject(error instanceof ProductFailure ? error : transportFailure("io", error instanceof Error ? error.message : String(error), error)))
      .finally(() => signal?.removeEventListener("abort", abort));
  });
}
function validate<T>(check: () => T): T {
  try { return check(); }
  catch (error) { if (error instanceof ProductFailure) throw error; throw transportFailure("protocol", error instanceof Error ? error.message : String(error), error); }
}

export class ProductClient {
  constructor(private readonly bridge: Pick<ProductBridge, "getSessions"> & Partial<Pick<ProductBridge, "switchSession">>) {}
  async getSessions({ signal }: ReceiveOptions = {}): Promise<SessionListEntry[]> {
    const value = await receive(() => this.bridge.getSessions(), signal);
    const reply = validate(() => sessionReply(value));
    if (!reply.ok) throw new ProductFailure(reply.error);
    return reply.sessions;
  }
  async switchSession(input: SessionTarget, { signal }: ReceiveOptions = {}): Promise<SessionSnapshot> {
    if (!this.bridge.switchSession) throw new Error("Session switching is unavailable");
    const target = sessionTarget(input);
    const value = await receive(() => this.bridge.switchSession!(target), signal);
    const reply = validate(() => historyReply(value, target));
    if (!reply.ok) throw new ProductFailure(reply.error);
    return reply.snapshot;
  }
}
