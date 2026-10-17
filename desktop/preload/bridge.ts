import { sessionTarget, sessionReply, historyReply, subscriptionId, syncReply, unsubscribeReply, messageSuccess, messageOperation, messageSubscription, messageEventResult, MESSAGE_START_CHANNEL, MESSAGE_CONTINUE_CHANNEL, MESSAGE_ACTIVE_CHANNEL, MESSAGE_SUBSCRIBE_CHANNEL, MESSAGE_NEXT_CHANNEL, MESSAGE_UNSUBSCRIBE_CHANNEL, MESSAGE_CANCEL_CHANNEL, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, SESSION_UNSUBSCRIBE_CHANNEL } from "../main/session-contract.js";
// Vite bundles this module into one sandbox preload; Electron is external.
export namespace RozsaBridge {
  export function create(invoke: (channel: string, ...args: unknown[]) => Promise<unknown>): import("../main/session-contract").ProductBridge {
    const messageSubscriptions = new Map<string, import("../main/session-contract").MessageSubscription>();
    return Object.freeze({
      async getSessions(...args: never[]): Promise<import("../main/session-contract").SessionListReply> {
        if (args.length !== 0) throw new Error("getSessions accepts no arguments");
        return sessionReply(await invoke("product:get-sessions"));
      },
      async switchSession(...args: [import("../main/session-contract").SessionTarget]): Promise<import("../main/session-contract").SessionHistoryReply> {
        if (args.length !== 1) throw new Error("Invalid session target");
        const target = sessionTarget(args[0]);
        return historyReply(await invoke("product:switch-session", target), target);
      },
      async subscribeSessions(...args: never[]) {
        if (args.length !== 0) throw new Error("subscribeSessions accepts no arguments");
        return syncReply(await invoke(SESSION_SUBSCRIBE_CHANNEL));
      },
      async nextSessionEvent(...args: [string]) {
        if (args.length !== 1) throw new Error("Invalid session subscription");
        const id = subscriptionId(args[0]);
        return syncReply(await invoke(SESSION_NEXT_CHANNEL, id), id);
      },
      async unsubscribeSessions(...args: [string]) {
        if (args.length !== 1) throw new Error("Invalid session subscription");
        return unsubscribeReply(await invoke(SESSION_UNSUBSCRIBE_CHANNEL, subscriptionId(args[0])));
      },
      async startMessage(...args: [string]) {
        if (args.length !== 1 || typeof args[0] !== "string" || args[0].trim() === "") throw new Error("Invalid message text");
        return messageOperation(messageSuccess(await invoke(MESSAGE_START_CHANNEL, { text: args[0] })));
      },
      async continueMessage(...args: never[]) {
        if (args.length !== 0) throw new Error("continueMessage accepts no arguments");
        return messageOperation(messageSuccess(await invoke(MESSAGE_CONTINUE_CHANNEL)));
      },
      async getActiveMessage(...args: never[]) {
        if (args.length !== 0) throw new Error("getActiveMessage accepts no arguments");
        const value = messageSuccess(await invoke(MESSAGE_ACTIVE_CHANNEL));
        if (typeof value !== "object" || value === null || !("active" in value)) throw new Error("Invalid active message response");
        return value.active === null ? null : messageOperation(value.active);
      },
      async subscribeMessageEvents(...args: [string]) {
        if (args.length !== 1 || typeof args[0] !== "string" || !args[0]) throw new Error("Invalid message operation");
        const subscription = messageSubscription(messageSuccess(await invoke(MESSAGE_SUBSCRIBE_CHANNEL, { operationId: args[0] })), args[0]);
        messageSubscriptions.set(subscription.subscriptionId, subscription);
        return subscription;
      },
      async nextMessageEvent(...args: [string]) {
        if (args.length !== 1) throw new Error("Invalid message subscription");
        const id = subscriptionId(args[0]);
        const subscription = messageSubscriptions.get(id);
        if (!subscription) throw new Error("Unknown message event subscription");
        const value = messageSuccess(await invoke(MESSAGE_NEXT_CHANNEL, { subscriptionId: id }));
        return messageEventResult(value, subscription);
      },
      async unsubscribeMessageEvents(...args: [string]) {
        if (args.length !== 1) throw new Error("Invalid message subscription");
        const id = subscriptionId(args[0]);
        const result = messageSuccess(await invoke(MESSAGE_UNSUBSCRIBE_CHANNEL, id));
        if (result !== true && !(typeof result === "object" && result !== null && Object.keys(result).length === 0)) throw new Error("Invalid message unsubscribe response");
        messageSubscriptions.delete(id);
      },
      async cancelMessage(...args: [string]) {
        if (args.length !== 1 || typeof args[0] !== "string" || !args[0]) throw new Error("Invalid message operation");
        const result = messageSuccess(await invoke(MESSAGE_CANCEL_CHANNEL, { operationId: args[0] }));
        if (result !== true && !(typeof result === "object" && result !== null && Object.keys(result).length === 0)) throw new Error("Invalid message cancellation response");
      },
    });
  }
}
