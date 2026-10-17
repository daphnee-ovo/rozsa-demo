import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createMessageConnection, StreamMessageReader, StreamMessageWriter, ResponseError, type MessageConnection } from "vscode-jsonrpc/node";
import { record, sessionEntries, sessionTarget, sessionSnapshot, sessionUpdate, subscriptionId, messageOperation, messageSubscription, messageEventResult, settingsSnapshot, themeDefinition, themeSummary, RPCFailure, remoteFault, transportFailure, ProductFailure, type SessionTarget, type SessionSnapshot, type SessionListEntry, type MessageOperation, type MessageSubscription, type MessageEventResult } from "./session-contract.js";
import type { SettingsScope, SettingsSnapshot, ThemeDefinition, ThemeSummary } from "../../contracts/ipc/session.js";
import type { SessionUpdate } from "../../contracts/ipc/session.js";
import type { SessionMethods } from "../../contracts/ipc/session.js";
export { RPCFailure } from "./session-contract.js";

export interface BackendOptions { requestTimeoutMs?: number; env?: NodeJS.ProcessEnv }
export class BackendService {
  readonly process: ChildProcessWithoutNullStreams;
  private readonly connection: MessageConnection;
  private failure: ProductFailure | undefined;
  private initialization: Promise<void> | undefined;
  private prefetchedSessions: Promise<SessionListEntry[]> | undefined;
  private prefetchedStartupSession: { target: SessionTarget; snapshot: Promise<SessionSnapshot> } | undefined;
  private stopping = false;
  private stopPromise: Promise<void> | undefined;
  private readonly closed: Promise<void>;
  private readonly timeoutMs: number;

  constructor(binary: string, directories: string[], options: BackendOptions = {}) {
    this.timeoutMs = options.requestTimeoutMs ?? 10000;
    if (!Number.isFinite(this.timeoutMs) || this.timeoutMs <= 0) throw new Error("Invalid backend request timeout");
    this.process = spawn(binary, directories.flatMap(dir => ["--session-dir", dir]), {
      stdio: "pipe", windowsHide: true, ...(options.env ? { env: { ...process.env, ...options.env } } : {}),
    });
    this.connection = createMessageConnection(new StreamMessageReader(this.process.stdout), new StreamMessageWriter(this.process.stdin));
    this.process.once("spawn", () => console.info(`Backend started: ${this.process.pid}`));
    this.closed = new Promise(resolve => this.process.once("close", () => {
      console.info(`Backend stopped: ${this.process.pid} (${this.process.exitCode ?? this.process.signalCode})`);
      this.fail(transportFailure("closed", `Backend exited (${this.process.exitCode ?? this.process.signalCode})`));
      resolve();
    }));
    const ioFailure = (error: Error): void => this.fail(transportFailure("io", error.message, error));
    this.process.on("error", ioFailure);
    this.process.stdin.on("error", ioFailure);
    this.process.stdout.on("error", ioFailure);
    this.process.stderr.on("data", (chunk: Buffer) => process.stderr.write(chunk));
    this.connection.onError(([error]) => this.fail(transportFailure("protocol", error.message, error)));
    this.connection.onClose(() => this.fail(transportFailure("closed", "Backend pipe closed")));
    this.connection.listen();
  }
  private fail(error: ProductFailure): void {
    if (this.failure) return;
    this.failure = error;
    // The library rejects its pending requests on dispose. There is no retry,
    // restart, or request replay, even after a timeout with an unknown outcome.
    this.connection.dispose();
    // A broken protocol/pipe cannot become usable again. Reap only our process;
    // keep the failure visible to the window and never restart/replay work.
    void this.stop().catch(error => console.error("Backend cleanup:", error));
  }
  private async request<M extends keyof SessionMethods>(method: M, params: SessionMethods[M]["params"], timeoutMs = this.timeoutMs): Promise<unknown> {
    if (method !== "initialize") {
      if (!this.initialization) throw transportFailure("protocol", "Backend initialize has not started");
      await this.initialization;
    }
    if (this.failure) throw this.failure;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const expired = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        this.fail(transportFailure("timeout", `Backend request timed out: ${method}`));
        reject(this.failure);
      }, timeoutMs);
    });
    try {
      // vscode-jsonrpc owns IDs, pending correlation, encoding and framing.
      return await Promise.race([this.connection.sendRequest(method, params), expired]);
    } catch (error) {
      if (this.failure) throw this.failure;
      if (error instanceof ResponseError) {
        try {
          const fault = remoteFault(error.code, error.message, error.data);
          if ((fault.kind === "catalog" && method !== "get_sessions")
            || (fault.kind === "session" && !["switch_session", "subscribe_sessions", "next_session_event"].includes(method))
            || (fault.kind === "message" && !["start_message", "continue_message", "subscribe_message_events", "next_message_event", "cancel_message"].includes(method))
            || (fault.kind === "runtime_initialization" && method !== "runtime_status")
            || (fault.kind === "settings" && !["get_settings", "update_setting", "reset_setting", "list_themes", "get_theme", "save_theme"].includes(method))) throw new Error("Backend error owner mismatch");
          throw new RPCFailure(fault);
        } catch (fault) {
          if (fault instanceof RPCFailure) throw fault;
          const failure = transportFailure("protocol", asError(fault).message, fault);
          this.fail(failure); throw failure;
        }
      }
      const failure = transportFailure("io", asError(error).message, error);
      this.fail(failure); throw failure;
    } finally { clearTimeout(timer); }
  }
  private requireRunning(): void {
    if (this.failure) throw this.failure;
    if (this.stopping) throw transportFailure("closed", "Backend is closing");
  }
  initialize(): Promise<void> {
    if (this.initialization) return this.initialization;
    this.initialization = this.initializeHandshake();
    return this.initialization;
  }
  private async initializeHandshake(): Promise<void> {
    try {
      this.requireRunning();
      const value = await this.request("initialize", { protocolVersion: 1 });
      const capabilities = record(value) ? value.capabilities : undefined;
      if (!record(value) || value.protocolVersion !== 1 || !Array.isArray(capabilities)
        || !capabilities.every(item => typeof item === "string")
        || !["get_sessions", "switch_session", "subscribe_sessions", "next_session_event", "unsubscribe_sessions",
          "runtime_status", "start_message", "continue_message", "subscribe_message_events", "next_message_event", "unsubscribe_message_events", "cancel_message",
          "get_settings", "update_setting", "reset_setting", "list_themes", "get_theme", "save_theme"].every(method => capabilities.includes(method))) {
        throw transportFailure("protocol", "Incompatible backend handshake");
      }
    } catch (error) { this.fail(error instanceof ProductFailure ? error : transportFailure("io", asError(error).message, error)); throw error; }
  }
  async waitForRuntime(): Promise<void> {
    this.requireRunning();
    for (;;) {
      const value = await this.request("runtime_status", {});
      if (!record(value) || !["initializing", "ready"].includes(value.state as string)) {
        const failure = transportFailure("protocol", "Invalid runtime status response"); this.fail(failure); throw failure;
      }
      if (value.state === "ready") return;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }
  async getSessions(): Promise<SessionListEntry[]> {
    this.requireRunning();
    if (this.prefetchedSessions) {
      const prefetched = this.prefetchedSessions;
      this.prefetchedSessions = undefined;
      return prefetched;
    }
    return this.readSessions();
  }
  async prefetchSessions(): Promise<SessionListEntry[]> {
    this.requireRunning();
    if (!this.prefetchedSessions) this.prefetchedSessions = this.readSessions();
    return this.prefetchedSessions;
  }
  async prefetchLatestSession(entries: SessionListEntry[]): Promise<void> {
    this.requireRunning();
    const latest = entries[0];
    if (!latest) return;
    const target = sessionTarget({ path: latest.path, catalogId: latest.id });
    if (!this.prefetchedStartupSession) {
      this.prefetchedStartupSession = { target, snapshot: this.readSessionSnapshot(target) };
    }
    await this.prefetchedStartupSession.snapshot;
  }
  private async readSessions(): Promise<SessionListEntry[]> {
    const value = await this.request("get_sessions", {});
    try { return sessionEntries(value); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async switchSession(input: SessionTarget): Promise<SessionSnapshot> {
    this.requireRunning();
    const target = sessionTarget(input);
    const prefetched = this.prefetchedStartupSession;
    if (prefetched && prefetched.target.path === target.path && prefetched.target.catalogId === target.catalogId) {
      this.prefetchedStartupSession = undefined;
      return prefetched.snapshot;
    }
    return this.readSessionSnapshot(target);
  }
  private async readSessionSnapshot(target: SessionTarget): Promise<SessionSnapshot> {
    const value = await this.request("switch_session", target);
    try { return sessionSnapshot(value, target); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  private update(value: unknown, id?: string): SessionUpdate {
    try { return sessionUpdate(value, id); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async subscribeSessions(): Promise<SessionUpdate> {
    this.requireRunning();
    const update = this.update(await this.request("subscribe_sessions", {}));
    if (!update.snapshot) { const failure = transportFailure("protocol", "Subscription requires an initial snapshot"); this.fail(failure); throw failure; }
    return update;
  }
  async nextSessionEvent(input: string): Promise<SessionUpdate> {
    this.requireRunning();
    const id = subscriptionId(input);
    // Go's long-poll has a 30s request deadline. Idle is not a pipe failure.
    return this.update(await this.request("next_session_event", { subscriptionId: id }, Math.max(this.timeoutMs, 35000)), id);
  }
  async unsubscribeSessions(input: string): Promise<void> {
    this.requireRunning();
    if (await this.request("unsubscribe_sessions", { subscriptionId: subscriptionId(input) }) !== true) {
      const failure = transportFailure("protocol", "Invalid unsubscribe response"); this.fail(failure); throw failure;
    }
  }
  async startMessage(text: string): Promise<MessageOperation> {
    this.requireRunning();
    if (typeof text !== "string" || text.trim() === "") throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Message text must not be empty" });
    const value = await this.request("start_message", { text });
    try { return messageOperation(value); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async continueMessage(): Promise<MessageOperation> {
    this.requireRunning();
    const value = await this.request("continue_message", {});
    try { return messageOperation(value); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async subscribeMessageEvents(input: string): Promise<MessageSubscription> {
    this.requireRunning();
    if (!input) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid message operation" });
    const value = await this.request("subscribe_message_events", { operationId: input });
    try { return messageSubscription(value, input); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async nextMessageEvent(input: string): Promise<MessageEventResult> {
    this.requireRunning();
    const id = subscriptionId(input);
    const value = await this.request("next_message_event", { subscriptionId: id }, Math.max(this.timeoutMs, 35000));
    try {
      if (!record(value) || typeof value.operationId !== "string" || typeof value.subscriptionId !== "string") throw new Error("Invalid message event identity");
      return messageEventResult(value, { subscriptionId: id, operationId: value.operationId });
    } catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async unsubscribeMessageEvents(input: string): Promise<void> {
    this.requireRunning();
    if (await this.request("unsubscribe_message_events", { subscriptionId: subscriptionId(input) }) !== true) {
      const failure = transportFailure("protocol", "Invalid message unsubscribe response"); this.fail(failure); throw failure;
    }
  }
  async cancelMessage(input: string): Promise<void> {
    this.requireRunning();
    if (typeof input !== "string" || !input) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid message operation" });
    if (await this.request("cancel_message", { operationId: input }) !== true) {
      const failure = transportFailure("protocol", "Invalid message cancellation response"); this.fail(failure); throw failure;
    }
  }
  async getSettings(scope: SettingsScope): Promise<SettingsSnapshot> {
    if (scope !== "global" && scope !== "project") throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid settings scope" });
    return settingsSnapshot(await this.request("get_settings", { scope }));
  }
  async updateSetting(scope: SettingsScope, key: string, value: unknown): Promise<SettingsSnapshot> {
    if ((scope !== "global" && scope !== "project") || typeof key !== "string" || !key) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid setting update" });
    return settingsSnapshot(await this.request("update_setting", { scope, key, value }));
  }
  async resetSetting(scope: SettingsScope, key: string): Promise<SettingsSnapshot> {
    if ((scope !== "global" && scope !== "project") || typeof key !== "string" || !key) throw new ProductFailure({ kind: "invalid_params", code: -32602, message: "Invalid setting reset" });
    return settingsSnapshot(await this.request("reset_setting", { scope, key }));
  }
  async listThemes(scope: SettingsScope): Promise<ThemeSummary[]> {
    const value = await this.request("list_themes", { scope });
    if (!Array.isArray(value)) { const failure = transportFailure("protocol", "Invalid theme list response"); this.fail(failure); throw failure; }
    try { return value.map(themeSummary); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async getTheme(id: string, mode: "light" | "dark"): Promise<ThemeDefinition> {
    try { return themeDefinition(await this.request("get_theme", { id, mode })); }
    catch (error) {
      if (error instanceof ProductFailure) throw error;
      const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure;
    }
  }
  async saveTheme(theme: ThemeDefinition): Promise<void> {
    const value = await this.request("save_theme", theme);
    if (value !== true) { const failure = transportFailure("protocol", "Invalid theme save response"); this.fail(failure); throw failure; }
  }
  stop(): Promise<void> { this.stopPromise ??= this.close(); return this.stopPromise; }
  private async close(): Promise<void> {
    this.stopping = true;
    if (this.process.exitCode !== null || this.process.signalCode !== null || !this.process.pid) { await this.closed; return; }
    const deadline = setTimeout(() => {
      this.fail(transportFailure("timeout", "Backend shutdown timed out"));
      console.error("Backend shutdown timed out; terminating owned process", this.process.pid);
      this.process.kill("SIGKILL");
    }, 3000);
    try {
      if (!this.failure && await this.request("shutdown", {}) !== true) throw transportFailure("protocol", "Invalid backend shutdown response");
    } catch (error) { console.error("Backend shutdown:", asError(error).message); }
    finally {
      this.process.stdin.end();
      await this.closed;
      clearTimeout(deadline);
      this.fail(transportFailure("closed", "Backend closed"));
    }
  }
}
function asError(error: unknown): Error { return error instanceof Error ? error : new Error(String(error)); }
