import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { resolve } from "node:path";
import { createInterface, type Interface } from "node:readline";
import type {
	Api,
	AssistantMessage,
	AssistantMessageEvent,
	Context,
	Model,
	SimpleStreamOptions,
	StreamFunction,
	StreamOptions,
} from "../types.ts";
import { AssistantMessageEventStream } from "../utils/event-stream.ts";
import { isRustModelSupportedApi } from "./rust-supported-apis.ts";

type BridgeMethod = "stream" | "streamSimple";

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

/**
 * Manages a long-lived rozsa-model process that handles concurrent requests via multiplexing.
 */
class RustModelProcess {
	private child: ChildProcessWithoutNullStreams | null = null;
	private pending: Map<string, AssistantMessageEventStream> = new Map();
	private readline: Interface | null = null;
	private stderrText = "";
	private nextRequestId = 0;
	private currentBinary: string | null = null;
	private currentArgs: string | null = null;

	/** Spawn or get the existing process. */
	private ensureProcess(): ChildProcessWithoutNullStreams {
		// Check if binary or args changed — restart if so
		const binary = resolveRustModelBinary();
		const args = JSON.stringify(resolveRustModelBinaryArgs());
		if (this.child && (this.currentBinary !== binary || this.currentArgs !== args)) {
			this.shutdown();
		}

		this.currentBinary = binary;
		this.currentArgs = args;
		if (this.child) {
			return this.child;
		}

		try {
			this.child = spawn(resolveRustModelBinary(), resolveRustModelBinaryArgs(), {
				stdio: ["pipe", "pipe", "pipe"],
			});
		} catch (error) {
			this.child = null;
			throw error;
		}

		this.stderrText = "";

		// Capture stderr for debugging
		this.child.stderr.on("data", (chunk: Buffer) => {
			this.stderrText = `${this.stderrText}${chunk.toString("utf8")}`.slice(-MAX_STDERR_CHARS);
		});

		// Parse stdout lines and route by request ID
		this.readline = createInterface({ input: this.child.stdout });
		this.readline.on("line", (line) => {
			const parsed = parseBridgeLine(line);
			if (!parsed) {
				return;
			}

			const stream = this.pending.get(parsed.id);
			if (!stream) {
				return;
			}

			if (parsed.type === "error") {
				pushBridgeError(
					stream,
					{
						api: "openai-completions",
						provider: "openai",
						id: "unknown",
					} as Model<Api>,
					parsed.message,
				);
				this.pending.delete(parsed.id);
				return;
			}

			stream.push(parsed.event);
			if (parsed.event.type === "done" || parsed.event.type === "error") {
				this.pending.delete(parsed.id);
			}
		});

		// On process error or exit: fail all pending requests
		const handleExit = (codeOrError?: number | Error | null, signal?: NodeJS.Signals | null) => {
			for (const [_id, stream] of this.pending) {
				const detail = this.stderrText.trim().length > 0 ? `: ${this.stderrText.trim()}` : "";
				const errorMsg =
					codeOrError instanceof Error
						? codeOrError.message
						: `rozsa-model exited with code ${codeOrError ?? "null"} signal ${signal ?? "null"}${detail}`;
				pushBridgeError(
					stream,
					{
						api: "openai-completions",
						provider: "openai",
						id: "unknown",
					} as Model<Api>,
					errorMsg,
				);
			}
			this.pending.clear();
			this.child = null;
			this.readline = null;
		};

		this.child.on("error", (error) => {
			handleExit(error);
		});

		this.child.on("close", (code, signal) => {
			handleExit(code, signal);
		});

		return this.child;
	}

	/** Stream a request through the long-lived process. */
	stream<TApi extends Api>(
		method: BridgeMethod,
		model: Model<TApi>,
		context: Context,
		options?: StreamOptions | SimpleStreamOptions,
	): AssistantMessageEventStream {
		const stream = new AssistantMessageEventStream();
		const requestId = `${Date.now().toString(36)}-${(this.nextRequestId++).toString(36)}`;

		let child: ChildProcessWithoutNullStreams;
		try {
			child = this.ensureProcess();
		} catch (error) {
			pushBridgeError(stream, model, error);
			return stream;
		}

		this.pending.set(requestId, stream);

		// Write request to stdin
		const request = {
			type: "request",
			id: requestId,
			method,
			model,
			context,
			options: serializeOptions(options),
		};

		try {
			child.stdin.write(`${JSON.stringify(request)}\n`);
		} catch (error) {
			this.pending.delete(requestId);
			pushBridgeError(stream, model, error);
			return stream;
		}

		// Handle abort signal: send cancel message
		if (options?.signal) {
			const abortHandler = () => {
				if (this.pending.has(requestId)) {
					this.pending.delete(requestId);
					pushBridgeError(stream, model, "Request was aborted", "aborted");
					try {
						child.stdin.write(`${JSON.stringify({ type: "cancel", id: requestId })}\n`);
					} catch {
						// If stdin write fails, the process is likely dead; error already sent
					}
				}
			};
			options.signal.addEventListener("abort", abortHandler, { once: true });
		}

		return stream;
	}

	/** Shut down the process gracefully. */
	shutdown(): void {
		if (this.child) {
			// Remove all listeners to prevent error handlers from firing
			this.child.removeAllListeners();
			if (this.readline) {
				this.readline.removeAllListeners();
				this.readline.close();
			}
			this.child.kill();
			this.child = null;
			this.readline = null;
		}
	}
}

/** Singleton process manager. */
const rustModelProcess = new RustModelProcess();

/** Stream a full TypeScript AI request through the Rust model bridge using the long-lived process. */
export const streamRustModel: StreamFunction<Api, StreamOptions> = (model, context, options) =>
	rustModelProcess.stream("stream", model, context, options);

/** Stream a simple TypeScript AI request through the Rust model bridge using the long-lived process. */
export const streamSimpleRustModel: StreamFunction<Api, SimpleStreamOptions> = (model, context, options) =>
	rustModelProcess.stream("streamSimple", model, context, options);

/** Decide whether a TypeScript API should route to the Rust model bridge. */
export function shouldUseRustModelProvider(api: Api): boolean {
	const backend = process.env.ROZSA_MODEL_BACKEND ?? "ts";
	if (backend === "ts") {
		return false;
	}
	if (backend !== "rust") {
		throw new Error('ROZSA_MODEL_BACKEND must be "ts" or "rust".');
	}
	return isRustModelSupportedApi(api);
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
	const stream = new AssistantMessageEventStream();
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

	const request = {
		type: "request",
		id: requestId,
		method,
		model,
		context,
		options: serializeOptions(options),
	};
	child.stdin.write(`${JSON.stringify(request)}\n`);
	child.stdin.end();

	return stream;
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

/** Parse one JSONL bridge output line and ignore unrelated line shapes. */
export function parseBridgeLine(line: string): BridgeLine | undefined {
	try {
		const parsed = JSON.parse(line) as BridgeLine;
		return parsed && (parsed.type === "event" || parsed.type === "error") ? parsed : undefined;
	} catch {
		return undefined;
	}
}

/** Push a terminal bridge error into the TypeScript assistant event stream. */
function pushBridgeError<TApi extends Api>(
	stream: AssistantMessageEventStream,
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
