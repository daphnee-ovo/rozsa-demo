import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createMessageConnection, StreamMessageReader, StreamMessageWriter, ResponseError, type MessageConnection } from "vscode-jsonrpc/node";
import { record, sessionEntries, sessionTarget, sessionSnapshot, RPCFailure, remoteFault, transportFailure, ProductFailure, type SessionTarget, type SessionSnapshot, type SessionListEntry } from "./session-contract.js";
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
    this.failure ??= error;
    // The library rejects its pending requests on dispose. There is no retry,
    // restart, or request replay, even after a timeout with an unknown outcome.
    this.connection.dispose();
  }
  private async request<M extends keyof SessionMethods>(method: M, params: SessionMethods[M]["params"]): Promise<unknown> {
    if (this.failure) throw this.failure;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const expired = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        this.fail(transportFailure("timeout", `Backend request timed out: ${method}`));
        reject(this.failure);
      }, this.timeoutMs);
    });
    try {
      // vscode-jsonrpc owns IDs, pending correlation, encoding and framing.
      return await Promise.race([this.connection.sendRequest(method, params), expired]);
    } catch (error) {
      if (this.failure) throw this.failure;
      if (error instanceof ResponseError) {
        try {
          const fault = remoteFault(error.code, error.message, error.data);
          if ((fault.kind === "catalog" && method !== "get_sessions") || (fault.kind === "session" && method !== "switch_session")) throw new Error("Backend error owner mismatch");
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
  async initialize(): Promise<void> {
    const value = await this.request("initialize", { protocolVersion: 1 });
    if (!record(value) || value.protocolVersion !== 1 || !Array.isArray(value.capabilities)
      || !value.capabilities.every(item => typeof item === "string")
      || !value.capabilities.includes("get_sessions") || !value.capabilities.includes("switch_session")) {
      const error = transportFailure("protocol", "Incompatible backend handshake"); this.fail(error); throw error;
    }
  }
  async getSessions(): Promise<SessionListEntry[]> {
    if (this.stopping) throw transportFailure("closed", "Backend is closing");
    const value = await this.request("get_sessions", {});
    try { return sessionEntries(value); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
  }
  async switchSession(input: SessionTarget): Promise<SessionSnapshot> {
    if (this.stopping) throw transportFailure("closed", "Backend is closing");
    const target = sessionTarget(input);
    const value = await this.request("switch_session", target);
    try { return sessionSnapshot(value, target); }
    catch (error) { const failure = transportFailure("protocol", asError(error).message, error); this.fail(failure); throw failure; }
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
