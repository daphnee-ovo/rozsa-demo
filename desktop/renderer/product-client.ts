import { sessionReply, historyReply, syncReply, unsubscribeReply, sessionTarget, ProductFailure, transportFailure, type ProductBridge, type SessionListEntry, type SessionTarget, type SessionSnapshot, type SessionSyncReply } from "../main/session-contract.js";

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
  constructor(private readonly bridge: Pick<ProductBridge, "getSessions"> & Partial<Omit<ProductBridge, "getSessions">>) {}
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
  async syncSessions(deliver: (active: SessionSnapshot | null) => void, report: (error: unknown, resynchronizing: boolean) => void, { signal }: ReceiveOptions = {}): Promise<void> {
    let id: string | undefined, epoch: string | undefined, watermark = -1;
    try {
      const { subscribeSessions, nextSessionEvent } = this.bridge;
      if (!subscribeSessions || !nextSessionEvent || !this.bridge.unsubscribeSessions) throw new Error("Session synchronization is unavailable");
      while (!signal?.aborted) {
        let reply: SessionSyncReply;
        try {
          const value = await receive(() => id ? nextSessionEvent(id) : subscribeSessions(), signal);
          reply = validate(() => syncReply(value, id));
          if (!reply.ok) throw new ProductFailure(reply.error);
        } catch (error) {
          if (id && error instanceof ProductFailure && error.failure.kind === "timeout") continue; // Idle Go long-poll deadline.
          if (id && error instanceof ProductFailure && error.failure.kind === "resync_required") {
            report(error, true); id = undefined; continue; // Read-only resubscription, never reissue a switch.
          }
          throw error;
        }
        const update = reply.update;
        id = update.subscriptionId;
        if (update.snapshot) {
          const state = update.snapshot;
          if (epoch !== undefined && (state.epoch !== epoch || state.watermark < watermark)) throw transportFailure("protocol", "Session snapshot moved backwards");
          epoch = state.epoch; watermark = state.watermark;
          deliver(state.active);
        } else {
          const event = update.event;
          if (event.epoch !== epoch || event.sequence <= watermark) continue;
          if (event.sequence !== watermark + 1) {
            report(transportFailure("protocol", "Session event sequence gap"), true);
            id = undefined; continue;
          }
          watermark = event.sequence;
          deliver(event.active);
        }
      }
    } catch (error) { if (!signal?.aborted) report(error, false); }
    finally {
      if (id && this.bridge.unsubscribeSessions) {
        try {
          const value = await this.bridge.unsubscribeSessions(id);
          const reply = validate(() => unsubscribeReply(value));
          if (!reply.ok) throw new ProductFailure(reply.error);
        } catch (error) { if (!signal?.aborted) report(error, false); }
      }
    }
  }
}
