import type { BrowserWindow, IpcMain, IpcMainInvokeEvent } from "electron";
import type { BackendService } from "./backend-service.js";
import { applySidebarVibrancy } from "./native-appearance.js";
import { MODELS_CHANGED_CHANNEL } from "./session-contract.js";
import { AUTH_LOGIN_CHANNEL, AUTH_INPUT_CHANNEL, AUTH_NEXT_CHANNEL, AUTH_CANCEL_CHANNEL, AUTH_LOGOUT_CHANNEL, oauthProvider } from "./session-contract.js";
import { SESSION_LIST_CHANNEL, SESSION_HISTORY_CHANNEL, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, SESSION_UNSUBSCRIBE_CHANNEL, SETTINGS_GET_CHANNEL, SETTINGS_UPDATE_CHANNEL, SETTINGS_RESET_CHANNEL, THEMES_LIST_CHANNEL, THEME_GET_CHANNEL, THEME_SAVE_CHANNEL, sessionTarget, subscriptionId, bridgeFault, transportFailure, RPCFailure } from "./session-contract.js";

// One trusted session page, one Go subscription. Navigation invalidates pending
// delivery before a replacement renderer can subscribe; no product work replay.
export function installSessionBridge(window: BrowserWindow, ipc: IpcMain, backend: BackendService, trustedURL: string, openBrowser?: (url: string) => Promise<void>): void {
  let generation = 0, revision = 0, current: string | undefined, disposed = false;
  const authOperations = new Set<string>();
  const detachModels = backend.onModelsChanged?.(update => {
    if (!disposed && !window.isDestroyed() && window.webContents.mainFrame.url === trustedURL) window.webContents.send(MODELS_CHANGED_CHANNEL, update);
  });
  const release = async (id: string): Promise<void> => {
    try { await backend.unsubscribeSessions(id); }
    catch (error) {
      if (!(error instanceof RPCFailure && error.remote.kind === "resync_required")) console.error("Session unsubscribe:", error);
    }
  };
  const invalidate = (): void => {
    generation++; revision++;
    for (const id of authOperations) void backend.cancelAuth(id).catch(error => console.error("OAuth cancellation:", error));
    authOperations.clear();
    const id = current; current = undefined;
    if (id) void release(id);
  };
  const channels = [SESSION_LIST_CHANNEL, SESSION_HISTORY_CHANNEL, SESSION_SUBSCRIBE_CHANNEL, SESSION_NEXT_CHANNEL, SESSION_UNSUBSCRIBE_CHANNEL,
    SETTINGS_GET_CHANNEL, SETTINGS_UPDATE_CHANNEL, SETTINGS_RESET_CHANNEL, THEMES_LIST_CHANNEL, THEME_GET_CHANNEL, THEME_SAVE_CHANNEL,
    AUTH_LOGIN_CHANNEL, AUTH_INPUT_CHANNEL, AUTH_NEXT_CHANNEL, AUTH_CANCEL_CHANNEL, AUTH_LOGOUT_CHANNEL];
  const handle = (channel: string, count: number, operation: (...args: unknown[]) => Promise<object>): void => {
    ipc.handle(channel, async (event: IpcMainInvokeEvent, ...args: unknown[]) => {
      if (disposed || window.isDestroyed() || event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame
        || event.senderFrame?.url !== trustedURL || args.length !== count) throw new Error("Unauthorized session request");
      const started = generation;
      try {
        const result = await operation(...args);
        if (started !== generation) throw transportFailure("closed", "Session page was replaced");
        return { ok: true, ...result };
      } catch (error) { return { ok: false, error: bridgeFault(error) }; }
    });
  };
  const ownedId = (input: unknown): string => {
    const id = subscriptionId(input);
    if (id !== current) throw transportFailure("closed", "Session subscription was replaced");
    return id;
  };
  handle(SESSION_LIST_CHANNEL, 0, async () => ({ sessions: await backend.getSessions() }));
  handle(SESSION_HISTORY_CHANNEL, 1, async input => ({ snapshot: await backend.switchSession(sessionTarget(input)) }));
  handle(SESSION_SUBSCRIBE_CHANNEL, 0, async () => {
    const started = ++revision;
    const old = current; current = undefined;
    if (old) void release(old);
    const update = await backend.subscribeSessions();
    if (started !== revision) {
      void release(update.subscriptionId);
      throw transportFailure("closed", "Session page was replaced");
    }
    current = update.subscriptionId;
    return { update };
  });
  handle(SESSION_NEXT_CHANNEL, 1, async input => {
    const id = ownedId(input), update = await backend.nextSessionEvent(id);
    if (id !== current) throw transportFailure("closed", "Session subscription was replaced");
    return { update };
  });
  handle(SESSION_UNSUBSCRIBE_CHANNEL, 1, async input => {
    const id = ownedId(input); current = undefined; revision++;
    await backend.unsubscribeSessions(id);
    return {};
  });
  const settingsHandle = (channel: string, count: number, operation: (...args: unknown[]) => Promise<unknown>): void => {
    ipc.handle(channel, async (event: IpcMainInvokeEvent, ...args: unknown[]) => {
      if (disposed || window.isDestroyed() || event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame
        || event.senderFrame?.url !== trustedURL || args.length !== count) throw new Error("Unauthorized settings request");
      try {
        const result = await operation(...args);
        if (Array.isArray(result)) return { ok: true, themes: result };
        if (typeof result === "object" && result !== null) return { ok: true, ...result };
        return { ok: true };
      } catch (error) { return { ok: false, error: bridgeFault(error) }; }
    });
  };
  const validScope = (scope: unknown): scope is "global" | "project" => scope === "global" || scope === "project";
  const validKey = (key: unknown): key is string => typeof key === "string" && key.length > 0 && key.trim() === key;
  settingsHandle(AUTH_LOGIN_CHANNEL, 1, async provider => {
    const started = generation;
    const operation = await backend.startAuth(oauthProvider(provider));
    if (started !== generation) { await backend.cancelAuth(operation.operationId); throw transportFailure("closed", "OAuth page was replaced"); }
    authOperations.add(operation.operationId);
    return operation;
  });
  settingsHandle(AUTH_INPUT_CHANNEL, 2, async (id, value) => {
    if (!validKey(id) || !validKey(value)) throw new Error("Invalid OAuth input");
    await backend.submitAuth(id, value);
  });
  settingsHandle(AUTH_NEXT_CHANNEL, 1, async id => {
    if (!validKey(id)) throw new Error("Invalid OAuth operation");
    const started = generation;
    const event = await backend.nextAuthEvent(id);
    if (started !== generation) throw transportFailure("closed", "OAuth page was replaced");
    if (event.kind === "complete" || event.kind === "error") authOperations.delete(id);
    if (event.kind === "authorization_url" && openBrowser) {
      const url = new URL(event.url!);
      const hosts = event.provider === "anthropic" ? ["claude.ai", "console.anthropic.com", "platform.claude.com"] : event.provider === "codex-oauth" ? ["auth.openai.com"] : ["github.com"];
      try {
        if (url.protocol !== "https:" || url.username || url.password || !hosts.includes(url.hostname)) throw new Error("OAuth authorization URL is not trusted");
        await openBrowser(url.href);
      } catch (error) { await backend.cancelAuth(id); authOperations.delete(id); throw error; }
    }
    return event;
  });
  settingsHandle(AUTH_CANCEL_CHANNEL, 1, async id => { if (!validKey(id)) throw new Error("Invalid OAuth operation"); await backend.cancelAuth(id); authOperations.delete(id); });
  settingsHandle(AUTH_LOGOUT_CHANNEL, 1, async provider => { await backend.logoutAuth(oauthProvider(provider)); });
  settingsHandle(SETTINGS_GET_CHANNEL, 1, async scope => {
    if (!validScope(scope)) throw new Error("Invalid settings scope");
    const snapshot = await backend.getSettings(scope);
    applySidebarVibrancy(window, snapshot.appearance.translucentSidebar);
    return snapshot;
  });
  settingsHandle(SETTINGS_UPDATE_CHANNEL, 3, async (scope, key, value) => {
    if (!validScope(scope) || !validKey(key)) throw new Error("Invalid setting update");
    const snapshot = await backend.updateSetting(scope, key, value);
    if (key === "appearance_translucent_sidebar") applySidebarVibrancy(window, snapshot.appearance.translucentSidebar);
    return snapshot;
  });
  settingsHandle(SETTINGS_RESET_CHANNEL, 2, async (scope, key) => {
    if (!validScope(scope) || !validKey(key)) throw new Error("Invalid setting reset");
    const snapshot = await backend.resetSetting(scope, key);
    if (key === "appearance_translucent_sidebar") applySidebarVibrancy(window, snapshot.appearance.translucentSidebar);
    return snapshot;
  });
  settingsHandle(THEMES_LIST_CHANNEL, 1, async scope => {
    if (!validScope(scope)) throw new Error("Invalid settings scope");
    return backend.listThemes(scope);
  });
  settingsHandle(THEME_GET_CHANNEL, 2, async (id, mode) => {
    if (!validKey(id) || (mode !== "light" && mode !== "dark")) throw new Error("Invalid theme request");
    return backend.getTheme(id, mode);
  });
  settingsHandle(THEME_SAVE_CHANNEL, 1, async theme => {
    if (typeof theme !== "object" || theme === null) throw new Error("Invalid theme definition");
    await backend.saveTheme(theme as import("../../contracts/ipc/session.js").ThemeDefinition);
  });
  window.webContents.on("did-start-navigation", (_event, _url, _inPlace, mainFrame) => { if (mainFrame) invalidate(); });
  window.webContents.on("render-process-gone", () => invalidate());
  window.once("closed", () => {
    detachModels?.();
    disposed = true; invalidate();
    for (const channel of channels) ipc.removeHandler(channel);
  });
}
