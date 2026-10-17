import { sessionReply, historyReply, syncReply, unsubscribeReply, sessionTarget, subscriptionId, messageOperation, messageSubscription, messageEventResult, settingsSnapshot, themeDefinition, themeSummary, ProductFailure, transportFailure, type ProductBridge, type SessionListEntry, type SessionTarget, type SessionSnapshot, type SessionSyncReply, type MessageOperation, type MessageSubscription, type MessageEventResult, type SettingsScope, type SettingsSnapshot, type ThemeDefinition, type ThemeSummary } from "../main/session-contract.js";

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
  private readonly messageSubscriptions = new Map<string, MessageSubscription>();
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
  async startMessage(text: string): Promise<MessageOperation> {
    if (typeof text !== "string" || text.trim() === "") throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Message text must not be empty" });
    if (!this.bridge.startMessage) throw new Error("Message sending is unavailable");
    return messageOperation(await this.bridge.startMessage(text));
  }
  async continueMessage(): Promise<MessageOperation> {
    if (!this.bridge.continueMessage) throw new Error("Message continuation is unavailable");
    return messageOperation(await this.bridge.continueMessage());
  }
  async getActiveMessage(): Promise<MessageOperation | null> {
    if (!this.bridge.getActiveMessage) throw new Error("Active message state is unavailable");
    const value = await this.bridge.getActiveMessage();
    return value === null ? null : messageOperation(value);
  }
  async cancelMessage(input: string): Promise<void> {
    if (!this.bridge.cancelMessage) throw new Error("Message cancellation is unavailable");
    if (typeof input !== "string" || input === "") throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid message operation" });
    await this.bridge.cancelMessage(input);
  }
  async getSettings(scope: SettingsScope, { signal }: ReceiveOptions = {}): Promise<SettingsSnapshot> {
    if (!this.bridge.getSettings) throw new Error("Settings are unavailable");
    const value = await receive(() => this.bridge.getSettings!(scope), signal);
    return validate(() => settingsSnapshot(value));
  }
  async updateSetting(scope: SettingsScope, key: string, value: unknown, { signal }: ReceiveOptions = {}): Promise<SettingsSnapshot> {
    if (!this.bridge.updateSetting) throw new Error("Settings updates are unavailable");
    const result = await receive(() => this.bridge.updateSetting!(scope, key, value), signal);
    return validate(() => settingsSnapshot(result));
  }
  async resetSetting(scope: SettingsScope, key: string, { signal }: ReceiveOptions = {}): Promise<SettingsSnapshot> {
    if (!this.bridge.resetSetting) throw new Error("Settings reset is unavailable");
    const value = await receive(() => this.bridge.resetSetting!(scope, key), signal);
    return validate(() => settingsSnapshot(value));
  }
  async listThemes(scope: SettingsScope, { signal }: ReceiveOptions = {}): Promise<ThemeSummary[]> {
    if (!this.bridge.listThemes) throw new Error("Theme catalog is unavailable");
    const value = await receive(() => this.bridge.listThemes!(scope), signal);
    return validate(() => { if (!Array.isArray(value)) throw new Error("Invalid theme catalog"); return value.map(themeSummary); });
  }
  async getTheme(id: string, mode: "light" | "dark", { signal }: ReceiveOptions = {}): Promise<ThemeDefinition> {
    if (!this.bridge.getTheme) throw new Error("Theme loading is unavailable");
    const value = await receive(() => this.bridge.getTheme!(id, mode), signal);
    return validate(() => themeDefinition(value));
  }
  async saveTheme(theme: ThemeDefinition, { signal }: ReceiveOptions = {}): Promise<void> {
    if (!this.bridge.saveTheme) throw new Error("Theme saving is unavailable");
    await receive(() => this.bridge.saveTheme!(theme), signal);
  }
  async subscribeMessageEvents(input: string): Promise<MessageSubscription> {
    if (!this.bridge.subscribeMessageEvents) throw new Error("Message events are unavailable");
    const subscription = messageSubscription(await this.bridge.subscribeMessageEvents(input), input);
    this.messageSubscriptions.set(subscription.subscriptionId, subscription);
    return subscription;
  }
  async nextMessageEvent(input: string, { signal }: ReceiveOptions = {}): Promise<MessageEventResult> {
    if (!this.bridge.nextMessageEvent) throw new Error("Message events are unavailable");
    const id = subscriptionId(input);
    const subscription = this.messageSubscriptions.get(id);
    if (!subscription) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Unknown message event subscription" });
    const value = await receive(() => this.bridge.nextMessageEvent!(id), signal);
    return messageEventResult(value, subscription);
  }
  async unsubscribeMessageEvents(input: string): Promise<void> {
    if (!this.bridge.unsubscribeMessageEvents) throw new Error("Message events are unavailable");
    const id = subscriptionId(input);
    await this.bridge.unsubscribeMessageEvents(id);
    this.messageSubscriptions.delete(id);
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
