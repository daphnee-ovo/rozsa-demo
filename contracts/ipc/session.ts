// Business contract only. Go/Node protocol libraries own envelopes and IDs.
import type { SessionListEntry, SessionSnapshot, SessionTarget } from "../../desktop/main/session-contract.js";

export interface SessionMethods {
  initialize: { params: { protocolVersion: 1 }; result: { protocolVersion: 1; capabilities: string[] } };
  get_sessions: { params: Record<string, never> | null | undefined; result: SessionListEntry[] };
  switch_session: { params: SessionTarget; result: SessionSnapshot };
  shutdown: { params: Record<string, never> | null | undefined; result: true };
}
export type SessionRequest = { [M in keyof SessionMethods]: { method: M; params: SessionMethods[M]["params"] } }[keyof SessionMethods];
export type RemoteFault =
  | { kind: "parse"; code: -32700; message: string }
  | { kind: "invalid_request"; code: -32600; message: string }
  | { kind: "method_not_found"; code: -32601; message: string }
  | { kind: "invalid_params"; code: -32602; message: string }
  | { kind: "internal"; code: -32603; message: string }
  | { kind: "catalog" | "session"; code: -32001; message: string }
  | { kind: "not_initialized"; code: -32002; message: string }
  | { kind: "closing"; code: -32003; message: string };
export type TransportFault = { kind: "transport"; reason: "timeout" | "closed" | "io" | "protocol"; message: string };
export type BridgeFault = RemoteFault | TransportFault;
export type ProductFault = BridgeFault | { kind: "ui_cancelled"; message: string };

export class ProductFailure extends Error {
  constructor(readonly failure: ProductFault, options?: ErrorOptions) {
    super(failure.message, options); this.name = "ProductFailure";
  }
}
export class RPCFailure extends ProductFailure {
  constructor(readonly remote: RemoteFault) { super(remote); this.name = "RPCFailure"; }
  get code(): number { return this.remote.code; }
}
export function transportFailure(reason: TransportFault["reason"], message: string, cause?: unknown): ProductFailure {
  return new ProductFailure({ kind: "transport", reason, message }, { cause });
}
export function bridgeFault(error: unknown): BridgeFault {
  if (error instanceof ProductFailure && error.failure.kind !== "ui_cancelled") return error.failure;
  return { kind: "transport", reason: "io", message: error instanceof Error ? error.message : String(error) };
}
export function object(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// Standard errors may originate in the Go library before the business handler;
// only the product-specific -32001 code needs data to distinguish its owner.
export function remoteFault(code: unknown, message: unknown, data?: unknown): RemoteFault {
  if (!Number.isInteger(code) || typeof message !== "string" || (data !== undefined && !object(data))) throw new Error("Invalid backend error");
  let kind: RemoteFault["kind"];
  switch (code) {
    case -32700: kind = "parse"; break;
    case -32600: kind = "invalid_request"; break;
    case -32601: kind = "method_not_found"; break;
    case -32602: kind = "invalid_params"; break;
    case -32603: kind = "internal"; break;
    case -32002: kind = "not_initialized"; break;
    case -32003: kind = "closing"; break;
    case -32001:
      if (!object(data) || (data.kind !== "catalog" && data.kind !== "session")) throw new Error("Invalid backend error kind");
      kind = data.kind; break;
    default: throw new Error("Unknown backend error code");
  }
  if (object(data) && data.kind !== kind) throw new Error("Backend error code/kind mismatch");
  return { code, kind, message } as RemoteFault;
}
export function validatedBridgeFault(value: unknown): BridgeFault {
  if (!object(value) || typeof value.message !== "string") throw new Error("Invalid product error");
  if (value.kind === "transport") {
    if (!["timeout", "closed", "io", "protocol"].includes(value.reason as string) || "code" in value) throw new Error("Invalid transport error");
    return value as TransportFault;
  }
  return remoteFault(value.code, value.message, { kind: value.kind });
}
