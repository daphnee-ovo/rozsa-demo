import { validatedBridgeFault, ProductFailure, type BridgeFault, type ModelOption, type SettingsScope, type SettingsSnapshot, type ThemeDefinition, type ThemeSummary } from "../../contracts/ipc/session.js";
export type { ModelOption, ModelUpdate, ModelDiagnostic, SettingsScope, SettingsSnapshot, ThemeDefinition, ThemeSummary } from "../../contracts/ipc/session.js";
import type { SessionUpdate } from "../../contracts/ipc/session.js";
import type { MessageEvent } from "../../contracts/ipc/session.js";
export { ProductFailure, RPCFailure, bridgeFault, remoteFault, transportFailure } from "../../contracts/ipc/session.js";
// Current slice only. Wire fields match Rust commands::SessionListEntry.
export interface SessionListEntry {
  id: string;
  path: string;
  name: string;
  modified: string;
  message_count: number;
}
export type SessionListReply =
  | { ok: true; sessions: SessionListEntry[] }
  | { ok: false; error: BridgeFault };
export interface SessionTarget { path: string; catalogId: string }
export type HistoryBlock =
  | { type: "text" | "thinking"; text: string }
  | { type: "image"; data?: string; mimeType?: string }
  | { type: "toolCall"; id?: string; name: string; arguments?: unknown };
export interface HistoryMessage {
  entryId: string; role: "user" | "assistant" | "system" | "toolResult";
  content: HistoryBlock[]; displayText?: string; toolCallId?: string; toolName?: string; details?: unknown;
  errorMessage?: string | null; stopReason?: string; timestamp?: number; isError?: boolean;
}
export interface SessionSnapshot extends SessionTarget {
  sessionId: string; cwd: string; messages: HistoryMessage[]; contextMessageCount: number;
}
export interface MessageOperation { operationId: string }
export interface MessageSubscription { subscriptionId: string; operationId: string }
export interface MessageEventResult extends MessageSubscription { event: MessageEvent }
export type SessionHistoryReply = { ok: true; snapshot: SessionSnapshot }
  | { ok: false; error: BridgeFault };
export interface ProductBridge {
  startAuth?(provider: import("../../contracts/ipc/session.js").OAuthProvider): Promise<MessageOperation>;
  submitAuth?(operationId: string, value: string): Promise<void>;
  nextAuthEvent?(operationId: string): Promise<import("../../contracts/ipc/session.js").AuthEvent>;
  cancelAuth?(operationId: string): Promise<void>;
  logoutAuth?(provider: import("../../contracts/ipc/session.js").OAuthProvider): Promise<void>;
  getSessions(): Promise<SessionListReply>;
  switchSession(target: SessionTarget): Promise<SessionHistoryReply>;
  subscribeSessions(): Promise<SessionSyncReply>;
  nextSessionEvent(subscriptionId: string): Promise<SessionSyncReply>;
  unsubscribeSessions(subscriptionId: string): Promise<SessionUnsubscribeReply>;
  startMessage(text: string): Promise<MessageOperation>;
  continueMessage(): Promise<MessageOperation>;
  getActiveMessage(): Promise<MessageOperation | null>;
  subscribeMessageEvents(operationId: string): Promise<MessageSubscription>;
  nextMessageEvent(subscriptionId: string): Promise<MessageEventResult>;
  unsubscribeMessageEvents(subscriptionId: string): Promise<void>;
  cancelMessage(operationId: string): Promise<void>;
  getSettings(scope: SettingsScope): Promise<SettingsSnapshot>;
  updateSetting(scope: SettingsScope, key: string, value: unknown): Promise<SettingsSnapshot>;
  resetSetting(scope: SettingsScope, key: string): Promise<SettingsSnapshot>;
  listThemes(scope: SettingsScope): Promise<ThemeSummary[]>;
  getTheme(id: string, mode: "light" | "dark"): Promise<ThemeDefinition>;
  saveTheme(theme: ThemeDefinition): Promise<void>;
  onModelsChanged?(receive: (update: import("../../contracts/ipc/session.js").ModelUpdate) => void): () => void;
}
export const MODELS_CHANGED_CHANNEL = "product:models-changed";
export const AUTH_LOGIN_CHANNEL = "product:auth-login";
export const AUTH_INPUT_CHANNEL = "product:auth-input";
export const AUTH_NEXT_CHANNEL = "product:auth-next";
export const AUTH_CANCEL_CHANNEL = "product:auth-cancel";
export const AUTH_LOGOUT_CHANNEL = "product:auth-logout";
export function oauthProvider(value: unknown): import("../../contracts/ipc/session.js").OAuthProvider {
  if (value !== "anthropic" && value !== "codex-oauth" && value !== "github-copilot") throw new Error("Unsupported OAuth provider");
  return value;
}
export function authEvent(value: unknown, operationId: string): import("../../contracts/ipc/session.js").AuthEvent {
  if (!record(value) || value.operationId !== operationId || !["authorization_url", "input_required", "progress", "complete", "error"].includes(value.kind as string)
    || [value.url, value.message, value.userCode].some(item => item !== undefined && typeof item !== "string")
    || (value.kind === "authorization_url" && (typeof value.url !== "string" || !value.url))) throw new Error("Invalid OAuth event");
  oauthProvider(value.provider);
  return value as unknown as import("../../contracts/ipc/session.js").AuthEvent;
}
export const SESSION_LIST_CHANNEL = "product:get-sessions";
export const SESSION_HISTORY_CHANNEL = "product:switch-session";
export const SESSION_SUBSCRIBE_CHANNEL = "product:subscribe-sessions";
export const SESSION_NEXT_CHANNEL = "product:next-session-event";
export const SESSION_UNSUBSCRIBE_CHANNEL = "product:unsubscribe-sessions";
export const MESSAGE_START_CHANNEL = "product:start-message";
export const MESSAGE_CONTINUE_CHANNEL = "product:continue-message";
export const MESSAGE_ACTIVE_CHANNEL = "product:get-active-message";
export const MESSAGE_SUBSCRIBE_CHANNEL = "product:subscribe-message-events";
export const MESSAGE_NEXT_CHANNEL = "product:next-message-event";
export const MESSAGE_UNSUBSCRIBE_CHANNEL = "product:unsubscribe-message-events";
export const MESSAGE_CANCEL_CHANNEL = "product:cancel-message";
export const SETTINGS_GET_CHANNEL = "product:get-settings";
export const SETTINGS_UPDATE_CHANNEL = "product:update-setting";
export const SETTINGS_RESET_CHANNEL = "product:reset-setting";
export const THEMES_LIST_CHANNEL = "product:list-themes";
export const THEME_GET_CHANNEL = "product:get-theme";
export const THEME_SAVE_CHANNEL = "product:save-theme";
export type SessionSyncReply = { ok: true; update: SessionUpdate } | { ok: false; error: BridgeFault };
export type SessionUnsubscribeReply = { ok: true } | { ok: false; error: BridgeFault };

export function subscriptionId(value: unknown): string {
  if (typeof value !== "string" || !value) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid session subscription" });
  return value;
}
export function sessionUpdate(value: unknown, id?: string): SessionUpdate {
  if (!record(value) || typeof value.subscriptionId !== "string" || !value.subscriptionId || (id !== undefined && value.subscriptionId !== id)) throw new Error("Invalid session subscription identity");
  const state = value.snapshot ?? value.event;
  if ((value.snapshot === undefined) === (value.event === undefined) || !record(state) || typeof state.epoch !== "string" || !state.epoch
    || !Number.isSafeInteger(value.snapshot ? state.watermark : state.sequence) || (value.snapshot ? state.watermark as number < 0 : state.sequence as number <= 0)) throw new Error("Invalid session synchronization state");
  if (state.active !== null) sessionSnapshot(state.active, sessionTarget(record(state.active) ? { path: state.active.path, catalogId: state.active.catalogId } : null));
  else if (!value.snapshot) throw new Error("Session event requires active history");
  return value as unknown as SessionUpdate;
}
export function syncReply(value: unknown, id?: string): SessionSyncReply {
  if (record(value) && value.ok === true) return { ok: true, update: sessionUpdate(value.update, id) };
  if (record(value) && value.ok === false) return { ok: false, error: validatedBridgeFault(value.error) };
  throw new Error("Invalid session sync reply");
}
export function unsubscribeReply(value: unknown): SessionUnsubscribeReply {
  if (record(value) && value.ok === true) return { ok: true };
  if (record(value) && value.ok === false) return { ok: false, error: validatedBridgeFault(value.error) };
  throw new Error("Invalid session unsubscribe reply");
}

export function sessionTarget(value: unknown): SessionTarget {
  if (!record(value) || Object.keys(value).length !== 2 || typeof value.path !== "string" || !value.path
    || typeof value.catalogId !== "string" || !value.catalogId) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid session target" });
  return { path: value.path, catalogId: value.catalogId };
}
export function sessionSnapshot(value: unknown, target: SessionTarget): SessionSnapshot {
  const block = (b: unknown): boolean => record(b) && (
    ((b.type === "text" || b.type === "thinking") && typeof b.text === "string")
    || (b.type === "image" && (b.data === undefined || typeof b.data === "string")
      && (b.mimeType === undefined || typeof b.mimeType === "string"))
    || (b.type === "toolCall" && typeof b.name === "string"
      && (b.id === undefined || typeof b.id === "string")));
  const message = (m: unknown): boolean => record(m) && typeof m.entryId === "string"
    && ["user", "assistant", "system", "toolResult"].includes(m.role as string)
    && Array.isArray(m.content) && m.content.every(block)
    && (m.displayText === undefined || typeof m.displayText === "string")
    && (m.toolName === undefined || typeof m.toolName === "string")
    && (m.toolCallId === undefined || typeof m.toolCallId === "string")
    && (m.errorMessage === undefined || m.errorMessage === null || typeof m.errorMessage === "string")
    && (m.stopReason === undefined || typeof m.stopReason === "string")
    && (m.timestamp === undefined || typeof m.timestamp === "number")
    && (m.isError === undefined || typeof m.isError === "boolean");
  if (!record(value) || value.path !== target.path || value.catalogId !== target.catalogId
    || typeof value.sessionId !== "string" || !value.sessionId || typeof value.cwd !== "string"
    || !Array.isArray(value.messages) || !value.messages.every(message)
    || new Set(value.messages.map(m => m.entryId)).size !== value.messages.length
    || !Number.isSafeInteger(value.contextMessageCount) || (value.contextMessageCount as number) < 0
    || (value.contextMessageCount as number) > value.messages.length) throw new Error("Invalid session history response or identity");
  return value as unknown as SessionSnapshot;
}
export function historyReply(value: unknown, target: SessionTarget): SessionHistoryReply {
  if (record(value) && value.ok === true) return { ok: true, snapshot: sessionSnapshot(value.snapshot, target) };
  if (record(value) && value.ok === false) {
    const error = validatedBridgeFault(value.error);
    if (error.kind === "catalog") throw new Error("Invalid session history error owner");
    return { ok: false, error };
  }
  throw new Error("Invalid session history reply");
}

export function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function themeDefinition(value: unknown): ThemeDefinition {
  if (!record(value) || typeof value.id !== "string" || !value.id || typeof value.name !== "string"
    || (value.mode !== "light" && value.mode !== "dark")
    || ![value.accent, value.background, value.foreground, value.uiFont, value.codeFont].every(item => typeof item === "string")
    || !record(value.variables) || !Object.entries(value.variables).every(([key, item]) => key.startsWith("--") && typeof item === "string")) throw new Error("Invalid theme definition");
  return value as unknown as ThemeDefinition;
}
export function themeSummary(value: unknown): ThemeSummary {
  if (!record(value) || typeof value.id !== "string" || !value.id || typeof value.name !== "string" || (value.mode !== "light" && value.mode !== "dark")) throw new Error("Invalid theme summary");
  return value as unknown as ThemeSummary;
}
export function settingsSnapshot(value: unknown): SettingsSnapshot {
  if (!record(value)) throw new Error("Invalid settings response");
  if (value.scope !== "global" && value.scope !== "project") throw new Error("Invalid settings scope response");
  if (!record(value.settings)) throw new Error("Invalid settings values response");
  if (!record(value.appearance)) throw new Error("Invalid appearance response");
  if (!Array.isArray(value.themes)) throw new Error("Invalid theme catalog response");
  if (!Array.isArray(value.models)) throw new Error("Invalid model catalog response");
  modelUpdate({ models: value.models, diagnostics: value.modelDiagnostics ?? [] });
  const appearance = value.appearance;
  if (!["system", "light", "dark"].includes(appearance.themeMode as string) || !Number.isInteger(appearance.fontSize)
    || typeof appearance.translucentSidebar !== "boolean") throw new Error("Invalid appearance response");
  themeDefinition(appearance.lightTheme);
  themeDefinition(appearance.darkTheme);
  value.themes.forEach(themeSummary);
  if (!value.models.every(model => record(model) && typeof model.id === "string" && model.id.length > 0
    && typeof model.name === "string" && typeof model.provider === "string" && typeof model.reasoning === "boolean"
    && Number.isSafeInteger(model.contextWindow) && (model.contextWindow as number) > 0)) throw new Error("Invalid model catalog response");
  return value as unknown as SettingsSnapshot;
}

export function modelUpdate(value: unknown): import("../../contracts/ipc/session.js").ModelUpdate {
  if (!record(value) || !Array.isArray(value.models) || !Array.isArray(value.diagnostics)
    || !value.models.every(model => record(model) && typeof model.id === "string" && model.id.length > 0
      && typeof model.name === "string" && typeof model.provider === "string" && typeof model.reasoning === "boolean"
      && Number.isSafeInteger(model.contextWindow) && (model.contextWindow as number) > 0)
    || !value.diagnostics.every(item => record(item) && typeof item.provider === "string" && typeof item.message === "string")) throw new Error("Invalid model catalog update");
  return value as unknown as import("../../contracts/ipc/session.js").ModelUpdate;
}

export function messageSuccess(value: unknown): unknown {
  if (!record(value) || typeof value.ok !== "boolean") throw new Error("Invalid message bridge reply");
  if (value.ok === false) throw new ProductFailure(validatedBridgeFault(value.error));
  const { ok: _ok, ...payload } = value;
  return payload;
}

export function messageOperation(value: unknown): MessageOperation {
  if (!record(value) || typeof value.operationId !== "string" || value.operationId.length === 0) throw new Error("Invalid message operation");
  return { operationId: value.operationId };
}

export function messageSubscription(value: unknown, operationId?: string): MessageSubscription {
  if (!record(value) || typeof value.subscriptionId !== "string" || value.subscriptionId.length === 0
    || typeof value.operationId !== "string" || value.operationId.length === 0
    || (operationId !== undefined && value.operationId !== operationId)) throw new Error("Invalid message event subscription");
  return { subscriptionId: value.subscriptionId, operationId: value.operationId };
}

export function messageEventResult(value: unknown, subscription: MessageSubscription): MessageEventResult {
  if (!record(value) || value.subscriptionId !== subscription.subscriptionId || value.operationId !== subscription.operationId
    || !record(value.event) || !Number.isSafeInteger(value.event.sequence) || (value.event.sequence as number) <= 0) {
    throw new Error("Invalid message event identity");
  }
  const event = value.event;
  let validated: MessageEvent;
  if (event.kind === "text_delta" && typeof event.delta === "string" && event.error === undefined) {
    validated = { sequence: event.sequence as number, kind: "text_delta", delta: event.delta };
  } else if (event.kind === "completed" && event.delta === undefined && event.error === undefined) {
    validated = { sequence: event.sequence as number, kind: "completed" };
  } else if (event.kind === "error" && typeof event.error === "string" && event.delta === undefined) {
    validated = { sequence: event.sequence as number, kind: "error", error: event.error };
  } else if (event.kind === "canceled" && event.delta === undefined && event.error === undefined) {
    validated = { sequence: event.sequence as number, kind: "canceled" };
  } else throw new Error("Invalid message event payload");
  return { ...subscription, event: validated };
}

export function sessionEntries(value: unknown): SessionListEntry[] {
  if (!Array.isArray(value) || !value.every(entry => record(entry)
    && ["id", "path", "name", "modified"].every(key => typeof entry[key] === "string")
    && typeof entry.message_count === "number" && Number.isInteger(entry.message_count)
    && entry.message_count >= 0 && entry.message_count <= 0xffffffff)) {
    throw new Error("Invalid session list response");
  }
  return value as SessionListEntry[];
}
export function sessionReply(value: unknown): SessionListReply {
  if (record(value) && value.ok === true) return { ok: true, sessions: sessionEntries(value.sessions) };
  if (record(value) && value.ok === false) {
    const error = validatedBridgeFault(value.error);
    if (error.kind === "session") throw new Error("Invalid session list error owner");
    return { ok: false, error };
  }
  throw new Error("Invalid session list reply");
}
