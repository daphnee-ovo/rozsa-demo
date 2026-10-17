import type { BrowserWindow, IpcMain, IpcMainInvokeEvent } from "electron";
import type { BackendService } from "./backend-service.js";
import { MESSAGE_ACTIVE_CHANNEL, MESSAGE_CANCEL_CHANNEL, MESSAGE_CONTINUE_CHANNEL, MESSAGE_NEXT_CHANNEL, MESSAGE_START_CHANNEL, MESSAGE_SUBSCRIBE_CHANNEL, MESSAGE_UNSUBSCRIBE_CHANNEL, bridgeFault, messageEventResult, messageOperation, messageSubscription, record, transportFailure, type MessageSubscription } from "./session-contract.js";

// Message work belongs to Go. This bridge only validates the trusted renderer,
// forwards typed product calls, and owns the renderer's event subscription.
export function installMessageBridge(window: BrowserWindow, ipc: IpcMain, backend: BackendService, trustedURL: string): void {
  let generation = 0;
  let activeOperationId: string | undefined;
  let currentSubscription: MessageSubscription | undefined;
  let disposed = false;
  const channels = [MESSAGE_ACTIVE_CHANNEL, MESSAGE_START_CHANNEL, MESSAGE_CONTINUE_CHANNEL, MESSAGE_SUBSCRIBE_CHANNEL,
    MESSAGE_NEXT_CHANNEL, MESSAGE_UNSUBSCRIBE_CHANNEL, MESSAGE_CANCEL_CHANNEL];

  const release = async (subscription: MessageSubscription): Promise<void> => {
    try { await backend.unsubscribeMessageEvents(subscription.subscriptionId); }
    catch (error) { console.error("Message unsubscribe:", error); }
  };
  const invalidate = (): void => {
    generation++;
    const old = currentSubscription;
    currentSubscription = undefined;
    if (old) void release(old);
  };
  const handle = (channel: string, count: number, operation: (...args: unknown[]) => Promise<object>): void => {
    ipc.handle(channel, async (event: IpcMainInvokeEvent, ...args: unknown[]) => {
      if (disposed || window.isDestroyed() || event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame
        || event.senderFrame?.url !== trustedURL || args.length !== count) throw new Error("Unauthorized message request");
      const started = generation;
      try {
        const result = await operation(...args);
        if (started !== generation) throw transportFailure("closed", "Message page was replaced");
        return { ok: true, ...result };
      } catch (error) { return { ok: false, error: bridgeFault(error) }; }
    });
  };

  handle(MESSAGE_ACTIVE_CHANNEL, 0, async () => ({ active: activeOperationId ? { operationId: activeOperationId } : null }));
  handle(MESSAGE_START_CHANNEL, 1, async input => {
    if (!record(input) || Object.keys(input).length !== 1 || typeof input.text !== "string" || input.text.trim() === "") {
      throw transportFailure("protocol", "Invalid message text");
    }
    const operation = messageOperation(await backend.startMessage(input.text));
    activeOperationId = operation.operationId;
    return operation;
  });
  handle(MESSAGE_CONTINUE_CHANNEL, 0, async () => {
    const operation = messageOperation(await backend.continueMessage());
    activeOperationId = operation.operationId;
    return operation;
  });
  handle(MESSAGE_SUBSCRIBE_CHANNEL, 1, async input => {
    if (!record(input) || Object.keys(input).length !== 1 || typeof input.operationId !== "string" || input.operationId !== activeOperationId) {
      throw transportFailure("protocol", "Unknown message operation");
    }
    const old = currentSubscription;
    currentSubscription = undefined;
    if (old) await release(old);
    const subscription = messageSubscription(await backend.subscribeMessageEvents(input.operationId), input.operationId);
    currentSubscription = subscription;
    return subscription;
  });
  handle(MESSAGE_NEXT_CHANNEL, 1, async input => {
    if (!record(input) || Object.keys(input).length !== 1 || typeof input.subscriptionId !== "string"
      || !currentSubscription || input.subscriptionId !== currentSubscription.subscriptionId) {
      throw transportFailure("closed", "Message event subscription was replaced");
    }
    const subscription = currentSubscription;
    const result = messageEventResult(await backend.nextMessageEvent(subscription.subscriptionId), subscription);
    if (["completed", "error", "canceled"].includes(result.event.kind) && activeOperationId === result.operationId) activeOperationId = undefined;
    return result;
  });
  handle(MESSAGE_UNSUBSCRIBE_CHANNEL, 1, async input => {
    if (!record(input) || Object.keys(input).length !== 1 || typeof input.subscriptionId !== "string"
      || !currentSubscription || input.subscriptionId !== currentSubscription.subscriptionId) {
      throw transportFailure("closed", "Message event subscription was replaced");
    }
    const subscription = currentSubscription;
    currentSubscription = undefined;
    await backend.unsubscribeMessageEvents(subscription.subscriptionId);
    return {};
  });
  handle(MESSAGE_CANCEL_CHANNEL, 1, async input => {
    if (!record(input) || Object.keys(input).length !== 1 || typeof input.operationId !== "string" || input.operationId !== activeOperationId) {
      throw transportFailure("closed", "Message operation is no longer active");
    }
    await backend.cancelMessage(input.operationId);
    return {};
  });

  window.webContents.on("did-start-navigation", (_event, _url, _inPlace, mainFrame) => { if (mainFrame) invalidate(); });
  window.webContents.on("render-process-gone", () => invalidate());
  window.once("closed", () => {
    disposed = true;
    invalidate();
    for (const channel of channels) ipc.removeHandler(channel);
  });
}
