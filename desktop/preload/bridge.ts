import { sessionTarget, sessionReply, historyReply, subscriptionId, syncReply, unsubscribeReply, messageSuccess, messageOperation, messageSubscription, messageEventResult, MESSAGE_START_CHANNEL, MESSAGE_CONTINUE_CHANNEL, MESSAGE_ACTIVE_CHANNEL, MESSAGE_SUBSCRIBE_CHANNEL, MESSAGE_NEXT_CHANNEL, MESSAGE_UNSUBSCRIBE_CHANNEL, MESSAGE_CANCEL_CHANNEL, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, SESSION_UNSUBSCRIBE_CHANNEL, SETTINGS_GET_CHANNEL, SETTINGS_UPDATE_CHANNEL, SETTINGS_RESET_CHANNEL, THEMES_LIST_CHANNEL, THEME_GET_CHANNEL, THEME_SAVE_CHANNEL } from "../main/session-contract.js";
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
      async getSettings(...args: ["global" | "project"]) {
        if (args.length !== 1 || !validScope(args[0])) throw new Error("Invalid settings scope");
        return settingsSnapshot(messageSuccess(await invoke(SETTINGS_GET_CHANNEL, args[0])));
      },
      async updateSetting(...args: ["global" | "project", string, unknown]) {
        if (args.length !== 3 || !validScope(args[0]) || !validKey(args[1])) throw new Error("Invalid settings update");
        return settingsSnapshot(messageSuccess(await invoke(SETTINGS_UPDATE_CHANNEL, args[0], args[1], args[2])));
      },
      async resetSetting(...args: ["global" | "project", string]) {
        if (args.length !== 2 || !validScope(args[0]) || !validKey(args[1])) throw new Error("Invalid settings reset");
        return settingsSnapshot(messageSuccess(await invoke(SETTINGS_RESET_CHANNEL, args[0], args[1])));
      },
      async listThemes(...args: ["global" | "project"]) {
        if (args.length !== 1 || !validScope(args[0])) throw new Error("Invalid settings scope");
        const value = messageSuccess(await invoke(THEMES_LIST_CHANNEL, args[0]));
        if (typeof value !== "object" || value === null || !("themes" in value) || !Array.isArray(value.themes) || !value.themes.every(validThemeSummary)) throw new Error("Invalid theme list response");
        return value.themes;
      },
      async getTheme(...args: [string, "light" | "dark"]) {
        if (args.length !== 2 || !validKey(args[0]) || !validThemeMode(args[1])) throw new Error("Invalid theme request");
        return themeDefinition(messageSuccess(await invoke(THEME_GET_CHANNEL, args[0], args[1])));
      },
      async saveTheme(...args: [import("../../contracts/ipc/session.js").ThemeDefinition]) {
        if (args.length !== 1 || !validThemeDefinition(args[0])) throw new Error("Invalid theme definition");
        const value = messageSuccess(await invoke(THEME_SAVE_CHANNEL, args[0]));
        if (!(typeof value === "object" && value !== null && Object.keys(value).length === 0)) throw new Error("Invalid theme save response");
      },
    });
  }
}

function validScope(value: unknown): value is "global" | "project" { return value === "global" || value === "project"; }
function validKey(value: unknown): value is string { return typeof value === "string" && value.length > 0 && value.trim() === value; }
function validThemeMode(value: unknown): value is "light" | "dark" { return value === "light" || value === "dark"; }
function validThemeDefinition(value: unknown): value is import("../../contracts/ipc/session.js").ThemeDefinition {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const theme = value as Record<string, unknown>;
  return validKey(theme.id) && typeof theme.name === "string" && validThemeMode(theme.mode)
    && [theme.accent, theme.background, theme.foreground, theme.uiFont, theme.codeFont].every(item => typeof item === "string")
    && typeof theme.variables === "object" && theme.variables !== null && !Array.isArray(theme.variables)
    && Object.entries(theme.variables as Record<string, unknown>).every(([key, item]) => key.startsWith("--") && typeof item === "string");
}
function themeDefinition(value: unknown): import("../../contracts/ipc/session.js").ThemeDefinition {
  if (!validThemeDefinition(value)) throw new Error("Invalid theme response");
  return value;
}
function validThemeSummary(value: unknown): value is import("../../contracts/ipc/session.js").ThemeSummary {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const summary = value as Record<string, unknown>;
  return validKey(summary.id) && typeof summary.name === "string" && validThemeMode(summary.mode);
}
function settingsSnapshot(value: unknown): import("../../contracts/ipc/session.js").SettingsSnapshot {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Invalid settings response");
  const snapshot = value as Record<string, unknown>;
  const appearance = snapshot.appearance;
  if (!validScope(snapshot.scope) || typeof snapshot.settings !== "object" || snapshot.settings === null || Array.isArray(snapshot.settings)
    || typeof appearance !== "object" || appearance === null || Array.isArray(appearance) || !Array.isArray(snapshot.themes)) throw new Error("Invalid settings response");
  const current = appearance as Record<string, unknown>;
  if (!(["system", "light", "dark"].includes(current.themeMode as string)) || !Number.isInteger(current.fontSize)
    || typeof current.translucentSidebar !== "boolean" || !validThemeDefinition(current.lightTheme) || !validThemeDefinition(current.darkTheme)
    || !snapshot.themes.every(validThemeSummary)) throw new Error("Invalid settings response");
  return value as import("../../contracts/ipc/session.js").SettingsSnapshot;
}
