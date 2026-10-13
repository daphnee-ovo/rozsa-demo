// TS7 emits modules; build bundles the emitted JS into one sandbox preload.
// Electron remains the only external runtime require.
export namespace RozsaBridge {
  export function create(invoke: (channel: string, ...args: unknown[]) => Promise<unknown>): import("../main/session-contract").ProductBridge {
    return Object.freeze({
      async getSessions(...args: never[]): Promise<import("../main/session-contract").SessionListReply> {
        if (args.length !== 0) throw new Error("getSessions accepts no arguments");
        return await invoke("product:get-sessions") as import("../main/session-contract").SessionListReply;
      },
      async switchSession(...args: [import("../main/session-contract").SessionTarget]): Promise<import("../main/session-contract").SessionHistoryReply> {
        const target = args[0];
        if (args.length !== 1 || !target || typeof target !== "object" || Array.isArray(target)
          || Object.keys(target).length !== 2 || typeof target.path !== "string" || !target.path
          || typeof target.catalogId !== "string" || !target.catalogId) throw new Error("Invalid session target");
        return await invoke("product:switch-session", { path: target.path, catalogId: target.catalogId }) as import("../main/session-contract").SessionHistoryReply;
      },
    });
  }
}
