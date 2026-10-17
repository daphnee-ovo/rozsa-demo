import type { ProductBridge, SessionListEntry, SessionSnapshot, SettingsSnapshot, ThemeDefinition } from "../../../main/session-contract.js";
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

export function createElectronAdapter(bridge: ProductBridge): UIAdapter {
  const client = new ProductClient(bridge);
  let sessions: SessionListEntry[] = [];
  let sessionsLoading: Promise<SessionListEntry[]> | null = null;
  let active: SessionSnapshot | null = null;
  let activeOperationId: string | null = null;
  let pendingUserText = "";
  let streamText = "";
  let streamError = "";
  let messageSequence = 0;
  let pumpController: AbortController | null = null;
  let eventPump: Promise<void> | null = null;
  const receivers = new Map<string, Set<UIReceiver>>();

  const publish = (): void => deliver("ui-state", currentState());
  const deliver = (event: string, payload: unknown): void => {
    for (const receive of receivers.get(event) || []) receive(payload);
  };

  function currentState() {
    const selectedName = active ? sessions.find(item => item.path === active?.path)?.name : undefined;
    const state = active
      ? legacyState(active, selectedName)
      : { sessionId: activeOperationId || "", sessionName: "Rózsa", cwd: "", messages: [], isStreaming: false };
    const messages: LegacyMessage[] = state.messages.slice();
    if (activeOperationId) {
      const lastUser = [...messages].reverse().find(item => item.message.role === "user");
      const lastText = lastUser?.message.content?.filter(block => block.type === "text").map(block => block.text).join("\n");
      if (pendingUserText && lastText !== pendingUserText) {
        messages.push({ kind: "standard", entryId: `pending-user:${activeOperationId}`, message: {
          role: "user", content: [{ type: "text", text: pendingUserText }], toolCallId: undefined,
          toolName: undefined, isError: false, errorMessage: undefined,
        } });
      }
      messages.push({ kind: "standard", entryId: `stream:${activeOperationId}`, message: {
        role: "assistant",
        content: streamText ? [{ type: "text", text: streamText }] : [],
        errorMessage: streamError || undefined,
        toolCallId: undefined, toolName: undefined, isError: false,
      } });
      state.isStreaming = true;
    }
    state.messages = messages;
    return state;
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
    active = await client.switchSession({ path: latest.path, catalogId: latest.id });
  }

  async function refreshActiveSession(): Promise<void> {
    const entries = await reloadSessions();
    const selected = active && entries.find(item => item.path === active?.path);
    const latest = selected || entries[0];
    if (latest) active = await client.switchSession({ path: latest.path, catalogId: latest.id });
  }

  async function applyMessageEvent(event: MessageEvent, operationId: string): Promise<boolean> {
    if (event.sequence !== messageSequence + 1) throw new Error(`Message event sequence gap: expected ${messageSequence + 1}, received ${event.sequence}`);
    messageSequence = event.sequence;
    if (event.kind === "text_delta") {
      streamText += event.delta;
      publish();
      return false;
    }
    if (event.kind === "error") streamError = event.error;
    try { await refreshActiveSession(); }
    catch (error) {
      if (event.kind !== "error") streamError = error instanceof Error ? error.message : String(error);
      deliver("error", streamError);
    }
    if (activeOperationId === operationId) activeOperationId = null;
    pendingUserText = "";
    streamText = "";
    if (event.kind === "error") deliver("error", event.error);
    publish();
    return true;
  }

  async function pumpMessages(signal: AbortSignal): Promise<void> {
    let subscriptionId: string | undefined;
    try {
      const activeMessage = activeOperationId ? { operationId: activeOperationId } : await client.getActiveMessage();
      if (!activeMessage || signal.aborted) return;
      const operationId = activeMessage.operationId;
      activeOperationId = operationId;
      messageSequence = 0;
      const subscription = await client.subscribeMessageEvents(operationId);
      subscriptionId = subscription.subscriptionId;
      publish();
      while (!signal.aborted && activeOperationId === operationId) {
        const update = await client.nextMessageEvent(subscriptionId, { signal });
        if (update.operationId !== operationId || update.subscriptionId !== subscriptionId) {
          throw new Error("Message event identity changed during subscription");
        }
        if (await applyMessageEvent(update.event, operationId)) break;
      }
    } catch (error) {
      if (!signal.aborted) {
        streamError = error instanceof Error ? error.message : String(error);
        deliver("error", streamError);
        publish();
      }
    } finally {
      if (subscriptionId) {
        try { await client.unsubscribeMessageEvents(subscriptionId); }
        catch (error) { if (!signal.aborted) deliver("error", error instanceof Error ? error.message : String(error)); }
      }
    }
  }

  function ensureMessagePump(): void {
    if (eventPump) return;
    const controller = new AbortController();
    pumpController = controller;
    eventPump = pumpMessages(controller.signal).finally(() => {
      if (pumpController === controller) pumpController = null;
      eventPump = null;
      if (activeOperationId && receivers.get("ui-state")?.size) ensureMessagePump();
    });
  }

  function stopMessagePump(): void {
    pumpController?.abort("Message UI subscription disposed");
    pumpController = null;
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
        active = await client.switchSession({ path: entry.path, catalogId: entry.id });
        signal?.throwIfAborted();
        publish();
        return true;
      }
      case "get_state": {
        await restoreLatestSession();
        const running = activeOperationId ? { operationId: activeOperationId } : await client.getActiveMessage();
        if (running) {
          activeOperationId = running.operationId;
          ensureMessagePump();
        }
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
        if (activeOperationId) throw new Error("A message is already running");
        pendingUserText = text;
        streamText = "";
        streamError = "";
        messageSequence = 0;
        const operation = await client.startMessage(text);
        activeOperationId = operation.operationId;
        publish();
        ensureMessagePump();
        return operation;
      }
      case "abort": {
        if (!activeOperationId) throw new Error("There is no active message to cancel");
        await client.cancelMessage(activeOperationId);
        return;
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
      if (event === "ui-state") ensureMessagePump();
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        listeners.delete(receive);
        if (!listeners.size) receivers.delete(event);
        if (!receivers.get("ui-state")?.size) stopMessagePump();
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
