/**
 * Node-only JSONL client for the `rozsa-model` Rust binary.
 *
 * Structure:
 * - streamSimpleRustModel(): sends a streamSimple request to Rust.
 * - createRustModelBridgeStream(): owns child process lifecycle and JSONL parsing.
 * - shouldUseRustModelProvider(): applies the explicit Rust backend/API gate.
 *
 * Related docs: ../../../docs/model/rozsa-model-migration.md
 */

import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { resolve } from "node:path";
import { createInterface } from "node:readline";
import type {
	Api,
	AssistantMessage,
	AssistantMessageEvent,
	Context,
	Model,
	SimpleStreamOptions,
	StreamOptions,
} from "@earendil-works/pi-ai";
import { EventStream } from "./event-stream.ts";
import type { AssistantMessageEventStream } from "./types.ts";

type BridgeMethod = "streamSimple";

interface BridgeEventLine {
	type: "event";
	id: string;
	event: AssistantMessageEvent;
}

interface BridgeErrorLine {
	type: "error";
	id: string;
	message: string;
	code?: string;
}

type BridgeLine = BridgeEventLine | BridgeErrorLine;

const DEFAULT_RUST_MODEL_BINARY = resolve(process.cwd(), "target", "debug", "rozsa-model");
const MAX_STDERR_CHARS = 4000;
const RUST_MODEL_SUPPORTED_APIS = new Set<Api>(["openai-completions", "bedrock-converse-stream"]);

class AgentAssistantMessageEventStream
	extends EventStream<AssistantMessageEvent, AssistantMessage>
	implements AssistantMessageEventStream
{
	constructor() {
		super(
			(event) => event.type === "done" || event.type === "error",
			(event) => {
				if (event.type === "done") return event.message;
				if (event.type === "error") return event.error;
				throw new Error("Unexpected event type");
			},
		);
	}
}

/** Stream a simple model request through the Rust model bridge. */
export const streamSimpleRustModel = (
	model: Model<any>,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream => createRustModelBridgeStream("streamSimple", model, context, options);

/** Decide whether an API should route to the Rust model bridge. */
export function shouldUseRustModelProvider(api: Api): boolean {
	const backend = process.env.ROZSA_MODEL_BACKEND ?? "ts";
	if (backend === "ts") {
		return false;
	}
	if (backend !== "rust") {
		throw new Error('ROZSA_MODEL_BACKEND must be "ts" or "rust".');
	}
	if (!RUST_MODEL_SUPPORTED_APIS.has(api)) return false;
	return rustApiSet().has(api);
}

/** Resolve the bridge executable path from env or the Cargo dev target. */
export function resolveRustModelBinary(): string {
	return process.env.ROZSA_MODEL_BINARY || DEFAULT_RUST_MODEL_BINARY;
}

/** Resolve optional bridge process arguments for local debugging and focused tests. */
export function resolveRustModelBinaryArgs(): string[] {
	const rawArgs = process.env.ROZSA_MODEL_BINARY_ARGS;
	if (!rawArgs) {
		return [];
	}
	const parsed = JSON.parse(rawArgs) as unknown;
	if (!Array.isArray(parsed) || parsed.some((value) => typeof value !== "string")) {
		throw new Error("ROZSA_MODEL_BINARY_ARGS must be a JSON string array");
	}
	return parsed;
}

/** Spawn the Rust JSONL bridge and expose its output as an assistant event stream. */
export function createRustModelBridgeStream<TApi extends Api>(
	method: BridgeMethod,
	model: Model<TApi>,
	context: Context,
	options?: StreamOptions | SimpleStreamOptions,
): AssistantMessageEventStream {
	const stream = new AgentAssistantMessageEventStream();
	const requestId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
	let terminalEventSeen = false;
	let stderrText = "";

	let child: ChildProcessWithoutNullStreams;
	try {
		child = spawn(resolveRustModelBinary(), resolveRustModelBinaryArgs(), {
			stdio: ["pipe", "pipe", "pipe"],
		});
	} catch (error) {
		pushBridgeError(stream, model, error);
		return stream;
	}

	child.stderr.on("data", (chunk: Buffer) => {
		stderrText = `${stderrText}${chunk.toString("utf8")}`.slice(-MAX_STDERR_CHARS);
	});

	createInterface({ input: child.stdout }).on("line", (line) => {
		const parsed = parseBridgeLine(line);
		if (!parsed || parsed.id !== requestId) {
			return;
		}
		if (parsed.type === "error") {
			terminalEventSeen = true;
			pushBridgeError(stream, model, parsed.message);
			return;
		}
		stream.push(parsed.event);
		if (parsed.event.type === "done" || parsed.event.type === "error") {
			terminalEventSeen = true;
		}
	});

	child.on("error", (error) => {
		if (!terminalEventSeen) {
			terminalEventSeen = true;
			pushBridgeError(stream, model, error);
		}
	});

	child.on("close", (code, signal) => {
		if (!terminalEventSeen) {
			const detail = stderrText.trim().length > 0 ? `: ${stderrText.trim()}` : "";
			terminalEventSeen = true;
			pushBridgeError(
				stream,
				model,
				`rozsa-model exited with code ${code ?? "null"} signal ${signal ?? "null"}${detail}`,
			);
		}
	});

	options?.signal?.addEventListener(
		"abort",
		() => {
			if (!terminalEventSeen) {
				terminalEventSeen = true;
				child.kill();
				pushBridgeError(stream, model, "Request was aborted", "aborted");
			}
		},
		{ once: true },
	);

	child.stdin.write(
		`${JSON.stringify({
			type: "request",
			id: requestId,
			method,
			model,
			context,
			options: serializeOptions(options),
		})}\n`,
	);
	child.stdin.end();

	return stream;
}

/** Parse one JSONL bridge output line and ignore unrelated line shapes. */
export function parseBridgeLine(line: string): BridgeLine | undefined {
	try {
		const parsed = JSON.parse(line) as BridgeLine;
		return parsed && (parsed.type === "event" || parsed.type === "error") ? parsed : undefined;
	} catch {
		return undefined;
	}
}

/** Parse the Rust-enabled API allow-list from the environment. */
function rustApiSet(): Set<string> {
	const raw = process.env.ROZSA_MODEL_RUST_APIS;
	if (!raw) {
		return new Set();
	}
	return new Set(
		raw
			.split(",")
			.map((api) => api.trim())
			.filter((api) => api.length > 0),
	);
}

/** Serialize stream options while removing Node-only callbacks and signals. */
function serializeOptions(options?: StreamOptions | SimpleStreamOptions): Record<string, unknown> {
	if (!options) {
		return {};
	}
	const output: Record<string, unknown> = {};
	for (const [key, value] of Object.entries(options)) {
		if (key === "signal" || key === "onPayload" || key === "onResponse") {
			continue;
		}
		if (value !== undefined) {
			output[key] = value;
		}
	}
	return output;
}

/** Push a terminal bridge error into the assistant event stream. */
function pushBridgeError<TApi extends Api>(
	stream: AgentAssistantMessageEventStream,
	model: Model<TApi>,
	error: unknown,
	stopReason: "error" | "aborted" = "error",
): void {
	const message: AssistantMessage = {
		role: "assistant",
		content: [],
		api: model.api,
		provider: model.provider,
		model: model.id,
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason,
		errorMessage: error instanceof Error ? error.message : String(error),
		timestamp: Date.now(),
	};
	stream.push({ type: "error", reason: stopReason, error: message });
}
