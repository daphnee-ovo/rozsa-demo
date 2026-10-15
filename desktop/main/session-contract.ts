import { validatedBridgeFault, ProductFailure, type BridgeFault } from "../../contracts/ipc/session.js";
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
export type HistoryBlock = { type: "text"; text: string } | { type: "thinking"; text: string } | { type: "image" } | { type: "toolCall"; name: string };
export interface HistoryMessage {
  entryId: string; role: "user" | "assistant" | "toolResult";
  content: HistoryBlock[]; displayText?: string; toolName?: string; isError?: boolean;
}
export interface SessionSnapshot extends SessionTarget {
  sessionId: string; cwd: string; messages: HistoryMessage[]; contextMessageCount: number;
}
export type SessionHistoryReply = { ok: true; snapshot: SessionSnapshot }
  | { ok: false; error: BridgeFault };
export interface ProductBridge {
  getSessions(): Promise<SessionListReply>;
  switchSession(target: SessionTarget): Promise<SessionHistoryReply>;
}
export const SESSION_LIST_CHANNEL = "product:get-sessions";
export const SESSION_HISTORY_CHANNEL = "product:switch-session";

export function sessionTarget(value: unknown): SessionTarget {
  if (!record(value) || Object.keys(value).length !== 2 || typeof value.path !== "string" || !value.path
    || typeof value.catalogId !== "string" || !value.catalogId) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid session target" });
  return { path: value.path, catalogId: value.catalogId };
}
export function sessionSnapshot(value: unknown, target: SessionTarget): SessionSnapshot {
  const block = (b: unknown): boolean => record(b) && (
    ((b.type === "text" || b.type === "thinking") && typeof b.text === "string")
    || b.type === "image" || (b.type === "toolCall" && typeof b.name === "string"));
  const message = (m: unknown): boolean => record(m) && typeof m.entryId === "string"
    && ["user", "assistant", "toolResult"].includes(m.role as string)
    && Array.isArray(m.content) && m.content.every(block)
    && (m.displayText === undefined || typeof m.displayText === "string")
    && (m.toolName === undefined || typeof m.toolName === "string")
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
