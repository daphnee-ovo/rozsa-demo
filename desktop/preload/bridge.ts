import { sessionTarget, sessionReply, historyReply, subscriptionId, syncReply, unsubscribeReply, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, SESSION_UNSUBSCRIBE_CHANNEL } from "../main/session-contract.js";
// Vite bundles this module into one sandbox preload; Electron is external.
export namespace RozsaBridge {
  export function create(invoke: (channel: string, ...args: unknown[]) => Promise<unknown>): import("../main/session-contract").ProductBridge {
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
    });
  }
}
