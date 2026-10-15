import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createMessageConnection, StreamMessageReader, StreamMessageWriter, ResponseError, type MessageConnection } from "vscode-jsonrpc/node";
import { record, sessionEntries, sessionTarget, sessionSnapshot, sessionUpdate, subscriptionId, RPCFailure, remoteFault, transportFailure, ProductFailure, type SessionTarget, type SessionSnapshot, type SessionListEntry } from "./session-contract.js";
import type { SessionUpdate } from "../../contracts/ipc/session.js";
import type { SessionMethods } from "../../contracts/ipc/session.js";
export { RPCFailure } from "./session-contract.js";

export interface BackendOptions { requestTimeoutMs?: number }
export class BackendService {
  readonly process: ChildProcessWithoutNullStreams;
  private readonly connection: MessageConnection;
  private failure: ProductFailure | undefined;
  private stopping = false;
  private stopPromise: Promise<void> | undefined;
  private readonly closed: Promise<void>;
  private readonly timeoutMs: number;

  constructor(binary: string, directories: string[], options: BackendOptions = {}) {
    this.timeoutMs = options.requestTimeoutMs ?? 10000;
    if (!Number.isFinite(this.timeoutMs) || this.timeoutMs <= 0) throw new Error("Invalid backend request timeout");
    this.process = spawn(binary, directories.flatMap(dir => ["--session-dir", dir]), { stdio: "pipe", windowsHide: true });
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
          if ((fault.kind === "catalog" && method !== "get_sessions") || (fault.kind === "session" && !["switch_session", "subscribe_sessions", "next_session_event"].includes(method))) throw new Error("Backend error owner mismatch");
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
  async initialize(): Promise<void> {
    try {
      this.requireRunning();
      const value = await this.request("initialize", { protocolVersion: 1 });
      const capabilities = record(value) ? value.capabilities : undefined;
      if (!record(value) || value.protocolVersion !== 1 || !Array.isArray(capabilities)
        || !capabilities.every(item => typeof item === "string")
        || !["get_sessions", "switch_session", "subscribe_sessions", "next_session_event", "unsubscribe_sessions"].every(method => capabilities.includes(method))) {
        throw transportFailure("protocol", "Incompatible backend handshake");
      }
    } catch (error) { this.fail(error instanceof ProductFailure ? error : transportFailure("io", asError(error).message, error)); throw error; }
  }
  async getSessions(): Promise<SessionListEntry[]> {
    this.requireRunning();
    const value = await this.request("get_sessions", {});
    try { return sessionEntries(value); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async switchSession(input: SessionTarget): Promise<SessionSnapshot> {
    this.requireRunning();
    const target = sessionTarget(input);
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
