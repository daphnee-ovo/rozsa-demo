import type { ProductBridge, SessionListEntry, SessionSnapshot } from "../../../main/session-contract.js";
import type { Disposer, RequestOptions, UIAdapter } from "../../scripts/core/ports.ts";

type LegacySession = SessionListEntry & { first_message: string; last_modified: string };

export function createElectronAdapter(bridge: ProductBridge): UIAdapter {
  let sessions: SessionListEntry[] = [];
  let active: SessionSnapshot | null = null;

  async function request(intent: string, data?: unknown, { signal }: RequestOptions = {}): Promise<unknown> {
    signal?.throwIfAborted();
    switch (intent) {
      case "get_sessions": {
        const reply = await bridge.getSessions();
        signal?.throwIfAborted();
        if (!reply.ok) throw new Error(reply.error.message);
        sessions = reply.sessions;
        return sessions.map((entry): LegacySession => ({
          ...entry, first_message: "", last_modified: entry.modified,
        }));
      }
      case "switch_session": {
        const path = record(data) ? data.path : undefined;
        const entry = sessions.find(session => session.path === path);
        if (!entry) throw new Error("Session is not in the current catalog; refresh the session list first");
        const reply = await bridge.switchSession({ path: entry.path, catalogId: entry.id });
        signal?.throwIfAborted();
        if (!reply.ok) throw new Error(reply.error.message);
        active = reply.snapshot;
        return true;
      }
      case "get_state":
        if (!active) return { sessionId: "", sessionName: "Rózsa", messages: [], isStreaming: false };
        return legacyState(active, sessions.find(session => session.path === active?.path)?.name);
      default:
        throw new Error(`Electron backend does not support ${intent} yet`);
    }
  }

  return Object.freeze({
    request,
    subscribe(event: string, _receive: (payload: unknown) => void): Disposer {
      throw new Error(`Electron backend event ${event} is not implemented yet`);
    },
    emit(event: string): Promise<void> {
      return Promise.reject(new Error(`Electron backend event ${event} is not implemented yet`));
    },
  });
}

function legacyState(snapshot: SessionSnapshot, sessionName?: string) {
  return {
    sessionId: snapshot.sessionId,
    sessionName: sessionName || snapshot.sessionId,
    messages: snapshot.messages.map(message => ({
      kind: "standard",
      message: {
        role: message.role,
        content: message.content.map(block => block.type === "toolCall"
          ? { type: "toolCall", id: `${message.entryId}:${block.name}`, name: block.name, input: {} }
          : block.type === "image" ? { type: "image" } : { type: block.type, text: block.text }),
        toolCallId: message.entryId,
        toolName: message.toolName,
        isError: message.isError,
      },
    })),
    isStreaming: false,
    contextUsage: { messages: snapshot.contextMessageCount, totalMessages: snapshot.messages.length },
  };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
