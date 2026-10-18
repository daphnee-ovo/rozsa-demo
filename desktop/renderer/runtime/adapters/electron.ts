import type { ProductBridge, SessionListEntry, SessionSnapshot, SessionTarget, SettingsSnapshot, ThemeDefinition } from "../../../main/session-contract.js";
import { ProductClient } from "../../product-client.js";
import type { Disposer, RequestOptions, UIAdapter } from "../../scripts/core/ports.ts";

type LegacySession = SessionListEntry & { first_message: string; last_modified: string };
type LegacyContent = { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "image"; data?: string; mimeType?: string }
  | { type: "toolCall"; id?: string; name: string; arguments?: unknown };
type LegacyMessage = { kind: "standard"; entryId: string; message: {
  role: string; content: LegacyContent[]; toolCallId?: string; toolName?: string;
  isError?: boolean; errorMessage?: string | null; [key: string]: unknown;
} };
type LegacyState = { sessionId: string; sessionName: string; cwd: string; messages: LegacyMessage[]; isStreaming: boolean; [key: string]: unknown };
type UIReceiver = (payload: unknown) => void;
type MessageEvent = Awaited<ReturnType<ProductClient["nextMessageEvent"]>>["event"];
type OperationState = {
  operationId: string;
  target: SessionTarget;
  userText: string;
  baseEntryIds: Set<string>;
  streamText: string;
  error: string;
  sequence: number;
  status: "running" | "completed" | "failed" | "canceled";
  errorDelivered: boolean;
};

export function createElectronAdapter(bridge: ProductBridge): UIAdapter {
  const client = new ProductClient(bridge);
  let sessions: SessionListEntry[] = [];
  let sessionsLoading: Promise<SessionListEntry[]> | null = null;
  let active: SessionSnapshot | null = null;
  const operations = new Map<string, OperationState>();
  const operationIdsBySession = new Map<string, Set<string>>();
  const runningBySession = new Map<string, string>();
  const eventPumps = new Map<string, Promise<void>>();
  let selectionQueue: Promise<void> = Promise.resolve();
  const receivers = new Map<string, Set<UIReceiver>>();

  const publish = (): void => deliver("ui-state", currentState());
  const deliver = (event: string, payload: unknown): void => {
    for (const receive of receivers.get(event) || []) receive(payload);
  };

  function currentState() {
    const selectedName = active ? sessions.find(item => item.path === active?.path)?.name : undefined;
    const state = active
      ? legacyState(active, selectedName)
      : { sessionId: "", sessionName: "Rózsa", cwd: "", messages: [], isStreaming: false };
    const messages: LegacyMessage[] = state.messages.slice();
    const sessionOperations = active ? operationsForSession(active.path) : [];
    for (const operation of sessionOperations) {
      const lastUser = [...messages].reverse().find(item => item.message.role === "user");
      const lastText = lastUser?.message.content?.filter(block => block.type === "text").map(block => block.text).join("\n");
      if (operation.status === "running" && operation.userText && lastText !== operation.userText) {
        messages.push({ kind: "standard", entryId: `pending-user:${operation.operationId}`, message: {
          role: "user", content: [{ type: "text", text: operation.userText }], toolCallId: undefined,
          toolName: undefined, isError: false, errorMessage: undefined,
        } });
      }
      if (operation.status === "running" || operation.streamText || operation.error || operation.status === "canceled") {
        messages.push({ kind: "standard", entryId: `stream:${operation.operationId}`, message: {
          role: "assistant",
          content: operation.streamText ? [{ type: "text", text: operation.streamText }] : [],
          errorMessage: operation.error || undefined,
          toolCallId: undefined, toolName: undefined, isError: operation.status === "failed",
        } });
      }
    }
    state.messages = messages;
    state.isStreaming = sessionOperations.some(operation => operation.status === "running");
    return state;
  }

  function operationsForSession(path: string): OperationState[] {
    return [...(operationIdsBySession.get(path) || [])]
      .map(operationId => operations.get(operationId))
      .filter((operation): operation is OperationState => operation !== undefined);
  }

  function enqueueSelection<T>(action: () => Promise<T>): Promise<T> {
    const result = selectionQueue.then(action);
    selectionQueue = result.then(() => undefined, () => undefined);
    return result;
  }

  function registerOperation(operationId: string, snapshot: SessionSnapshot, userText: string): OperationState {
    const operation: OperationState = {
      operationId,
      target: { path: snapshot.path, catalogId: snapshot.catalogId },
      userText,
      baseEntryIds: new Set(snapshot.messages.map(message => message.entryId)),
      streamText: "",
      error: "",
      sequence: 0,
      status: "running",
      errorDelivered: false,
    };
    operations.set(operationId, operation);
    const operationIds = operationIdsBySession.get(snapshot.path) || new Set<string>();
    operationIds.add(operationId);
    operationIdsBySession.set(snapshot.path, operationIds);
    runningBySession.set(snapshot.path, operationId);
    return operation;
  }

  function reconcilePersistedOperations(snapshot: SessionSnapshot): void {
    for (const operation of operationsForSession(snapshot.path)) {
      if (operation.status === "running") continue;
      const appended = snapshot.messages.filter(message => !operation.baseEntryIds.has(message.entryId));
      const hasPrompt = !operation.userText || appended.some(message => message.role === "user"
        && historyMessageText(message) === operation.userText);
      const hasTerminal = appended.some(message => message.role === "assistant"
        && (operation.status === "failed" ? message.errorMessage === operation.error : true));
      if (hasPrompt && hasTerminal) removeOperation(operation);
    }
  }

  function removeOperation(operation: OperationState): void {
    operations.delete(operation.operationId);
    const operationIds = operationIdsBySession.get(operation.target.path);
    operationIds?.delete(operation.operationId);
    if (!operationIds?.size) operationIdsBySession.delete(operation.target.path);
    if (runningBySession.get(operation.target.path) === operation.operationId) runningBySession.delete(operation.target.path);
  }

  async function reloadSessions(): Promise<SessionListEntry[]> {
    if (sessionsLoading) return sessionsLoading;
    sessionsLoading = client.getSessions().then(entries => {
      sessions = entries;
      return sessions;
    }).finally(() => { sessionsLoading = null; });
    return sessionsLoading;
  }

  async function restoreLatestSession(): Promise<void> {
    if (active) return;
    const entries = await reloadSessions();
    if (!entries.length) return;
    const latest = entries[0];
    await enqueueSelection(async () => {
      if (active) return;
      active = await client.switchSession({ path: latest.path, catalogId: latest.id });
      reconcilePersistedOperations(active);
    });
  }

  async function refreshOperationSession(operation: OperationState): Promise<void> {
    await enqueueSelection(async () => {
      if (active?.path !== operation.target.path) return;
      const snapshot = await client.switchSession(operation.target);
      if (active?.path !== operation.target.path) return;
      active = snapshot;
      reconcilePersistedOperations(snapshot);
      publish();
    });
  }

  async function applyMessageEvent(operation: OperationState, event: MessageEvent): Promise<boolean> {
    if (event.sequence !== operation.sequence + 1) throw new Error(`Message event sequence gap: expected ${operation.sequence + 1}, received ${event.sequence}`);
    operation.sequence = event.sequence;
    if (event.kind === "text_delta") {
      operation.streamText += event.delta;
      if (active?.path === operation.target.path) publish();
      return false;
    }
    operation.status = event.kind === "completed" ? "completed" : event.kind === "error" ? "failed" : "canceled";
    if (event.kind === "error") operation.error = event.error;
    if (runningBySession.get(operation.target.path) === operation.operationId) runningBySession.delete(operation.target.path);
    if (active?.path === operation.target.path) {
      if (operation.error) {
        operation.errorDelivered = true;
        deliver("error", operation.error);
      }
      try { await refreshOperationSession(operation); }
      catch (error) {
        if (!operation.error) {
          operation.error = error instanceof Error ? error.message : String(error);
          deliver("error", operation.error);
        }
        publish();
      }
    }
    return true;
  }

  async function pumpMessages(operation: OperationState): Promise<void> {
    let subscriptionId: string | undefined;
    try {
      const subscription = await client.subscribeMessageEvents(operation.operationId);
      subscriptionId = subscription.subscriptionId;
      if (active?.path === operation.target.path) publish();
      while (operation.status === "running") {
        const update = await client.nextMessageEvent(subscriptionId);
        if (update.operationId !== operation.operationId || update.subscriptionId !== subscriptionId) {
          throw new Error("Message event identity changed during subscription");
        }
        if (await applyMessageEvent(operation, update.event)) break;
      }
    } catch (error) {
      if (operation.status === "running") {
        operation.error = error instanceof Error ? error.message : String(error);
        if (active?.path === operation.target.path) deliver("error", operation.error);
        publish();
      }
    } finally {
      if (subscriptionId) {
        try { await client.unsubscribeMessageEvents(subscriptionId); }
        catch (error) {
          if (active?.path === operation.target.path) deliver("error", error instanceof Error ? error.message : String(error));
        }
      }
    }
  }

  function ensureMessagePump(operation: OperationState): void {
    if (operation.status !== "running" || eventPumps.has(operation.operationId)) return;
    const pump = pumpMessages(operation).finally(() => {
      if (eventPumps.get(operation.operationId) === pump) eventPumps.delete(operation.operationId);
    });
    eventPumps.set(operation.operationId, pump);
  }

  async function request(intent: string, data?: unknown, { signal }: RequestOptions = {}): Promise<unknown> {
    signal?.throwIfAborted();
    switch (intent) {
      case "get_sessions": {
        const entries = await reloadSessions();
        signal?.throwIfAborted();
        return entries.map((entry): LegacySession => ({ ...entry, first_message: "", last_modified: entry.modified }));
      }
      case "switch_session": {
        const path = isRecord(data) ? data.path : undefined;
        const entry = sessions.find(session => session.path === path);
        if (!entry) throw new Error("Session is not in the current catalog; refresh the session list first");
        await enqueueSelection(async () => {
          const snapshot = await client.switchSession({ path: entry.path, catalogId: entry.id });
          active = snapshot;
          reconcilePersistedOperations(snapshot);
          for (const operation of operationsForSession(snapshot.path)) {
            if (operation.status === "running") ensureMessagePump(operation);
            else if (operation.error && !operation.errorDelivered) {
              operation.errorDelivered = true;
              deliver("error", operation.error);
            }
          }
          publish();
        });
        signal?.throwIfAborted();
        return true;
      }
      case "get_state": {
        await restoreLatestSession();
        await enqueueSelection(async () => {
          if (!active) return;
          const snapshot = active;
          const knownOperationId = runningBySession.get(snapshot.path);
          const running = knownOperationId ? { operationId: knownOperationId } : await client.getActiveMessage();
          if (running && !operations.has(running.operationId)) registerOperation(running.operationId, snapshot, "");
          const operation = running && operations.get(running.operationId);
          if (operation) ensureMessagePump(operation);
        });
        signal?.throwIfAborted();
        return currentState();
      }
      case "get_settings": {
        const snapshot = await client.getSettings("global", { signal });
        return legacySettings(snapshot);
      }
      case "list_themes":
        return client.listThemes("global", { signal });
      case "update_setting": {
        if (!isRecord(data) || typeof data.key !== "string" || typeof data.value !== "string") throw new TypeError("Invalid setting update");
        const value = settingValue(data.key, data.value);
        return client.updateSetting("global", data.key, value, { signal });
      }
      case "get_theme": {
        if (!isRecord(data) || typeof data.id !== "string" || (data.mode !== "light" && data.mode !== "dark")) throw new TypeError("Invalid theme request");
        return client.getTheme(data.id, data.mode, { signal });
      }
      case "save_theme": {
        if (!isRecord(data) || !isRecord(data.theme)) throw new TypeError("Invalid theme definition");
        await client.saveTheme(data.theme as unknown as ThemeDefinition, { signal });
        return true;
      }
      case "send_message": {
        if (!isRecord(data) || typeof data.message !== "string") throw new TypeError("Invalid message request");
        const text = data.message.trim();
        if (!text) return;
        await restoreLatestSession();
        return enqueueSelection(async () => {
          const selected = active;
          if (!selected) throw new Error("Select a session before sending a message");
          if (runningBySession.has(selected.path)) throw new Error("A message is already running in this session");
          const operation = await client.startMessage(text);
          const state = registerOperation(operation.operationId, selected, text);
          publish();
          ensureMessagePump(state);
          return operation;
        });
      }
      case "abort": {
        return enqueueSelection(async () => {
          const operationId = active && runningBySession.get(active.path);
          if (!operationId) throw new Error("There is no active message to cancel in this session");
          await client.cancelMessage(operationId);
        });
      }
      default:
        throw new Error(`Electron backend does not support ${intent} yet`);
    }
  }

  return Object.freeze({
    request,
    subscribe(event: string, receive: UIReceiver): Disposer {
      if (event !== "ui-state" && event !== "error") throw new Error(`Electron backend event ${event} is not implemented yet`);
      const listeners = receivers.get(event) || new Set<UIReceiver>();
      listeners.add(receive);
      receivers.set(event, listeners);
      if (event === "ui-state") {
        for (const operation of operations.values()) ensureMessagePump(operation);
      }
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        listeners.delete(receive);
        if (!listeners.size) receivers.delete(event);
      };
    },
    emit(event: string): Promise<void> {
      return Promise.reject(new Error(`Electron backend event ${event} is not implemented yet`));
    },
  });
}

function legacyState(snapshot: SessionSnapshot, sessionName?: string): LegacyState {
  return {
    sessionId: snapshot.sessionId,
    sessionName: sessionName || snapshot.sessionId,
    cwd: snapshot.cwd,
    messages: snapshot.messages.map(message => ({
      kind: "standard",
      entryId: message.entryId,
      message: {
        role: message.role,
        content: message.content.map(block => block.type === "toolCall"
          ? { type: "toolCall", id: block.id || `${message.entryId}:${block.name}`, name: block.name, arguments: block.arguments || {} }
          : block.type === "image" ? { type: "image", data: block.data, mimeType: block.mimeType }
            : { type: block.type, text: block.text }),
        toolCallId: message.toolCallId || message.entryId,
        toolName: message.toolName,
        isError: message.isError,
        errorMessage: message.errorMessage,
      },
    })),
    isStreaming: false,
    contextUsage: { messages: snapshot.contextMessageCount, totalMessages: snapshot.messages.length },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function historyMessageText(message: SessionSnapshot["messages"][number]): string {
  return message.content.flatMap(block => block.type === "text" || block.type === "thinking" ? [block.text] : []).join("\n");
}

function legacySettings(snapshot: SettingsSnapshot): Record<string, unknown> {
  const values = snapshot.settings;
  const appearance = isRecord(values.appearance) ? values.appearance : {};
  const compaction = isRecord(values.compaction) ? values.compaction : {};
  const retry = isRecord(values.retry) ? values.retry : {};
  return {
    thinkingEffort: values.defaultThinkingEffort,
    model_id: values.defaultModel,
    model_name: values.defaultModel,
    model_provider: values.defaultProvider,
    small_model: values.smallModel,
    block_images: values.blockImages,
    hide_thinking: values.hideThinking,
    transport: values.transport,
    auto_compact: compaction.enabled,
    compaction_trigger_ratio: compaction.triggerRatio,
    compaction_target_ratio: compaction.targetRatio,
    retry_timeout_ms: retry.timeoutMs,
    retry_max_retries: retry.maxRetries,
    retry_max_delay_ms: retry.maxRetryDelayMs,
    auto_session_naming: values.autoSessionNaming,
    steering_mode: values.steeringMode,
    follow_up_mode: values.followUpMode,
    running_send_mode: values.runningSendMode,
    appearance: { ...appearance, isMacos: navigator.userAgent.includes("Mac") },
  };
}

function settingValue(key: string, value: string): unknown {
  if (["auto_compact", "auto_session_naming", "block_images", "hide_thinking", "appearance_translucent_sidebar", "appearance_show_rate_limits", "appearance_show_hourly_rate_limit", "appearance_show_weekly_rate_limit"].includes(key)) {
    if (value !== "true" && value !== "false") throw new TypeError(`Invalid boolean setting: ${key}`);
    return value === "true";
  }
  if (["compaction_trigger_ratio", "compaction_target_ratio", "appearance_font_size", "retry_timeout_ms", "retry_max_retries", "retry_max_delay_ms"].includes(key)) {
    if (value.trim() === "") return null;
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) throw new TypeError(`Invalid numeric setting: ${key}`);
    return parsed;
  }
  return value;
}
