/**
 * JSONL client for the `rozsa-core` Rust bridge binary.
 *
 * Internal Framework:
 * rust-core-client.ts
 * ├── BridgeInput types             # TS → Rust messages (start_run, cancel, tool_result)
 * ├── BridgeOutput types            # Rust → TS messages (agent_event, tool_request, run_done, run_error)
 * ├── RustCoreClient                # manages bridge child process and protocol messaging
 * │   ├── ensureProcess()           # spawn or get existing bridge process
 * │   ├── startRun()                # send start_run, returns async iterable of bridge outputs
 * │   ├── sendToolResult()          # send tool_result for a pending tool_request
 * │   ├── cancelRun()               # send cancel for the active run
 * │   └── shutdown()                # kill the bridge process
 * └── RustAgentLoopBackend          # AgentLoopBackend impl using RustCoreClient
 *
 * Related Docs:
 * - [Protocol](../../crates/rozsa-core/src/protocol.rs)
 * - [Bridge Binary](../../crates/rozsa-core/src/bin/bridge.rs)
 * - [Model Bridge Client](./rozsa-model-client.ts)
 */

import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { resolve } from "node:path";
import { createInterface, type Interface } from "node:readline";
import type { AgentLoopBackend } from "./backend.ts";
import { TsAgentLoopBackend } from "./backend.ts";
import { validateToolArguments } from "./tool-validation.ts";
import type { AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, StreamFn } from "./types.ts";

// --- Protocol Constants ---

const PROTOCOL_VERSION = 1;
const MAX_STDERR_CHARS = 4000;
const DEFAULT_RUST_CORE_BINARY = resolve(process.cwd(), "target", "debug", "rozsa-core");

// --- Protocol Types: TS → Rust ---

interface StartRunInput {
	type: "start_run";
	version: number;
	run_id: string;
	mode: "prompt" | "continue";
	prompts?: unknown[];
	context: BridgeContext;
	config: BridgeConfig;
}

interface CancelInput {
	type: "cancel";
	version: number;
	run_id: string;
}

interface ToolResultInput {
	type: "tool_result";
	version: number;
	run_id: string;
	request_id: string;
	result: ToolHostResult;
}

type BridgeInput = StartRunInput | CancelInput | ToolResultInput;

interface BridgeContext {
	system_prompt: string;
	messages: unknown[];
	tools: BridgeToolSchema[];
}

interface BridgeToolSchema {
	name: string;
	description: string;
	parameters: unknown;
}

interface BridgeConfig {
	model: unknown;
	reasoning?: unknown;
	stream_options: Record<string, unknown>;
	tool_execution: "sequential" | "parallel";
}

interface ToolHostResult {
	content: unknown[];
	is_error: boolean;
	terminate: boolean;
}

// --- Protocol Types: Rust → TS ---

interface AgentEventOutput {
	type: "agent_event";
	version: number;
	run_id: string;
	event: RustAgentEvent;
}

interface ToolRequestOutput {
	type: "tool_request";
	version: number;
	run_id: string;
	request_id: string;
	tool_call_id: string;
	tool_name: string;
	args: Record<string, unknown>;
	assistant_message: unknown;
	context: unknown;
}

interface RunDoneOutput {
	type: "run_done";
	version: number;
	run_id: string;
}

interface RunErrorOutput {
	type: "run_error";
	version: number;
	run_id: string;
	error: string;
}

type BridgeOutput = AgentEventOutput | ToolRequestOutput | RunDoneOutput | RunErrorOutput;

/**
 * Rust-side agent events map to TS AgentEvent.
 * The Rust side uses snake_case enum tags.
 */
type RustAgentEvent =
	| { type: "agent_start" }
	| { type: "agent_end"; messages: unknown[] }
	| { type: "turn_start" }
	| { type: "turn_end"; message: unknown; tool_results: unknown[] }
	| { type: "message_start"; message: unknown }
	| { type: "message_update"; message: unknown; stream_event: unknown }
	| { type: "message_end"; message: unknown }
	| { type: "tool_execution_start"; tool_call_id: string; tool_name: string; args: unknown }
	| { type: "tool_execution_update"; tool_call_id: string; tool_name: string; args: unknown; partial_result: unknown }
	| { type: "tool_execution_end"; tool_call_id: string; tool_name: string; result: unknown };

// --- Run State ---

interface PendingRun {
	runId: string;
	resolve: (messages: AgentMessage[]) => void;
	reject: (error: Error) => void;
	emit: (event: AgentEvent) => Promise<void> | void;
	config: AgentLoopConfig;
	context: AgentContext;
	collectedMessages: AgentMessage[];
}

// --- RustCoreClient ---

/**
 * Manages a long-lived rozsa-core bridge process.
 * Follows the same pattern as RustModelProcess in rozsa-model-client.ts.
 */
class RustCoreClient {
	private child: ChildProcessWithoutNullStreams | null = null;
	private readline: Interface | null = null;
	private stderrText = "";
	private pendingRun: PendingRun | null = null;
	private nextRunId = 0;

	/** Resolve the bridge executable path from env or the Cargo dev target. */
	private resolveBinary(): string {
		return process.env.ROZSA_CORE_BINARY || DEFAULT_RUST_CORE_BINARY;
	}

	/** Resolve optional bridge process arguments. */
	private resolveBinaryArgs(): string[] {
		const rawArgs = process.env.ROZSA_CORE_BINARY_ARGS;
		if (!rawArgs) {
			return [];
		}
		const parsed = JSON.parse(rawArgs) as unknown;
		if (!Array.isArray(parsed) || parsed.some((value) => typeof value !== "string")) {
			throw new Error("ROZSA_CORE_BINARY_ARGS must be a JSON string array");
		}
		return parsed;
	}

	/** Spawn or get the existing bridge process. */
	private ensureProcess(): ChildProcessWithoutNullStreams {
		if (this.child) {
			return this.child;
		}

		try {
			this.child = spawn(this.resolveBinary(), this.resolveBinaryArgs(), {
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

		// Parse stdout lines and dispatch
		this.readline = createInterface({ input: this.child.stdout });
		this.readline.on("line", (line) => {
			this.handleOutputLine(line);
		});

		// On process error or exit: fail the pending run
		const handleExit = (codeOrError?: number | Error | null, signal?: NodeJS.Signals | null) => {
			if (this.pendingRun) {
				const detail = this.stderrText.trim().length > 0 ? `: ${this.stderrText.trim()}` : "";
				const errorMsg =
					codeOrError instanceof Error
						? codeOrError.message
						: `rozsa-core exited with code ${codeOrError ?? "null"} signal ${signal ?? "null"}${detail}`;
				this.pendingRun.reject(new Error(errorMsg));
				this.pendingRun = null;
			}
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

	/** Handle a parsed output line from the bridge. */
	private handleOutputLine(line: string): void {
		let parsed: BridgeOutput;
		try {
			parsed = JSON.parse(line) as BridgeOutput;
		} catch {
			return;
		}

		if (!parsed || !parsed.type) {
			return;
		}

		const run = this.pendingRun;
		if (!run) {
			return;
		}

		if (parsed.run_id !== run.runId) {
			return;
		}

		switch (parsed.type) {
			case "agent_event":
				this.handleAgentEvent(run, parsed.event);
				break;

			case "tool_request":
				this.handleToolRequest(run, parsed);
				break;

			case "run_done":
				this.pendingRun = null;
				run.resolve(run.collectedMessages);
				break;

			case "run_error":
				this.pendingRun = null;
				run.reject(new Error(parsed.error));
				break;
		}
	}

	/** Convert a Rust-side AgentEvent to a TS AgentEvent and forward to emit. */
	private handleAgentEvent(run: PendingRun, event: RustAgentEvent): void {
		const tsEvent = this.convertEvent(event, run);
		if (tsEvent) {
			// Collect messages from agent_end
			if (tsEvent.type === "agent_end") {
				run.collectedMessages = tsEvent.messages as AgentMessage[];
			}
			void Promise.resolve(run.emit(tsEvent));
		}
	}

	/**
	 * Convert a Rust agent event to a TS AgentEvent.
	 * The Rust side serializes events with snake_case tags; TS uses camelCase-ish
	 * event types but the actual tag names match (agent_start, agent_end, etc.)
	 */
	private convertEvent(event: RustAgentEvent, _run: PendingRun): AgentEvent | null {
		switch (event.type) {
			case "agent_start":
				return { type: "agent_start" };
			case "agent_end":
				return { type: "agent_end", messages: event.messages as AgentMessage[] };
			case "turn_start":
				return { type: "turn_start" };
			case "turn_end":
				return {
					type: "turn_end",
					message: event.message as AgentMessage,
					toolResults: event.tool_results as any[],
				};
			case "message_start":
				return { type: "message_start", message: event.message as AgentMessage };
			case "message_update":
				return {
					type: "message_update",
					message: event.message as AgentMessage,
					assistantMessageEvent: event.stream_event as any,
				};
			case "message_end":
				return { type: "message_end", message: event.message as AgentMessage };
			case "tool_execution_start":
				return {
					type: "tool_execution_start",
					toolCallId: event.tool_call_id,
					toolName: event.tool_name,
					args: event.args,
				};
			case "tool_execution_update":
				return {
					type: "tool_execution_update",
					toolCallId: event.tool_call_id,
					toolName: event.tool_name,
					args: event.args,
					partialResult: event.partial_result,
				};
			case "tool_execution_end":
				return {
					type: "tool_execution_end",
					toolCallId: event.tool_call_id,
					toolName: event.tool_name,
					result: event.result,
					isError: false,
				};
			default:
				return null;
		}
	}

	/**
	 * Handle a tool_request from the bridge:
	 * 1. Run beforeToolCall hook
	 * 2. Execute the tool
	 * 3. Run afterToolCall hook
	 * 4. Send tool_result back
	 */
	private async handleToolRequest(run: PendingRun, request: ToolRequestOutput): Promise<void> {
		const { request_id, tool_call_id, tool_name, args } = request;

		const tool = run.context.tools?.find((t) => t.name === tool_name);
		if (!tool) {
			this.sendToolResult(run.runId, request_id, {
				content: [{ type: "text", text: `Tool ${tool_name} not found` }],
				is_error: true,
				terminate: false,
			});
			return;
		}

		// Validate arguments
		let validatedArgs: unknown;
		try {
			const toolCall = { id: tool_call_id, type: "toolCall" as const, name: tool_name, arguments: args };
			validatedArgs = validateToolArguments(tool, toolCall);
		} catch (error) {
			this.sendToolResult(run.runId, request_id, {
				content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }],
				is_error: true,
				terminate: false,
			});
			return;
		}

		// beforeToolCall hook
		if (run.config.beforeToolCall) {
			try {
				const beforeResult = await run.config.beforeToolCall(
					{
						assistantMessage: request.assistant_message as any,
						toolCall: { id: tool_call_id, type: "toolCall", name: tool_name, arguments: args } as any,
						args: validatedArgs,
						context: (request.context as AgentContext) ?? run.context,
					},
					undefined,
				);
				if (beforeResult?.block) {
					this.sendToolResult(run.runId, request_id, {
						content: [{ type: "text", text: beforeResult.reason || "Tool execution was blocked" }],
						is_error: true,
						terminate: false,
					});
					return;
				}
			} catch (error) {
				this.sendToolResult(run.runId, request_id, {
					content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }],
					is_error: true,
					terminate: false,
				});
				return;
			}
		}

		// Execute the tool
		let result: { content: unknown[]; details: unknown; terminate?: boolean };
		let isError = false;
		try {
			result = await tool.execute(tool_call_id, validatedArgs as never, undefined, undefined);
		} catch (error) {
			result = {
				content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }],
				details: {},
			};
			isError = true;
		}

		// afterToolCall hook
		if (run.config.afterToolCall) {
			try {
				const afterResult = await run.config.afterToolCall(
					{
						assistantMessage: request.assistant_message as any,
						toolCall: { id: tool_call_id, type: "toolCall", name: tool_name, arguments: args } as any,
						args: validatedArgs,
						result: result as any,
						isError,
						context: (request.context as AgentContext) ?? run.context,
					},
					undefined,
				);
				if (afterResult) {
					if (afterResult.content !== undefined) {
						result.content = afterResult.content;
					}
					if (afterResult.terminate !== undefined) {
						result.terminate = afterResult.terminate;
					}
					if (afterResult.isError !== undefined) {
						isError = afterResult.isError;
					}
				}
			} catch (error) {
				result = {
					content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }],
					details: {},
				};
				isError = true;
			}
		}

		this.sendToolResult(run.runId, request_id, {
			content: result.content as unknown[],
			is_error: isError,
			terminate: result.terminate ?? false,
		});
	}

	/** Send a tool_result message to the bridge. */
	private sendToolResult(runId: string, requestId: string, result: ToolHostResult): void {
		const message: ToolResultInput = {
			type: "tool_result",
			version: PROTOCOL_VERSION,
			run_id: runId,
			request_id: requestId,
			result,
		};
		this.writeStdin(message);
	}

	/** Send a cancel message to the bridge. */
	cancelRun(runId: string): void {
		const message: CancelInput = {
			type: "cancel",
			version: PROTOCOL_VERSION,
			run_id: runId,
		};
		this.writeStdin(message);
	}

	/**
	 * Start a run on the bridge.
	 * Returns a promise that resolves with the collected messages when run_done arrives.
	 */
	startRun(
		mode: "prompt" | "continue",
		prompts: AgentMessage[],
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
	): Promise<AgentMessage[]> {
		try {
			this.ensureProcess();
		} catch (error) {
			return Promise.reject(error instanceof Error ? error : new Error(String(error)));
		}

		const runId = `${Date.now().toString(36)}-${(this.nextRunId++).toString(36)}`;

		return new Promise<AgentMessage[]>((resolveRun, rejectRun) => {
			this.pendingRun = {
				runId,
				resolve: resolveRun,
				reject: rejectRun,
				emit,
				config,
				context,
				collectedMessages: [],
			};

			// Handle abort signal
			if (signal) {
				const abortHandler = () => {
					this.cancelRun(runId);
				};
				signal.addEventListener("abort", abortHandler, { once: true });
			}

			// Build bridge context
			const bridgeContext: BridgeContext = {
				system_prompt: context.systemPrompt,
				messages: context.messages as unknown[],
				tools: (context.tools || []).map((tool) => ({
					name: tool.name,
					description: tool.description,
					parameters: tool.parameters ?? {},
				})),
			};

			// Build bridge config — forward stream options for Rust model_stream
			const bridgeConfig: BridgeConfig = {
				model: config.model,
				reasoning: config.reasoning,
				stream_options: {
					temperature: config.temperature,
					max_tokens: config.maxTokens,
					transport: config.transport,
					cache_retention: config.cacheRetention,
					session_id: config.sessionId,
					timeout_ms: config.timeoutMs,
					max_retries: config.maxRetries,
					max_retry_delay_ms: config.maxRetryDelayMs,
				},
				tool_execution: config.toolExecution ?? "parallel",
			};

			// Send start_run
			const input: StartRunInput = {
				type: "start_run",
				version: PROTOCOL_VERSION,
				run_id: runId,
				mode,
				prompts: mode === "prompt" ? (prompts as unknown[]) : undefined,
				context: bridgeContext,
				config: bridgeConfig,
			};

			this.writeStdin(input);
		});
	}

	/** Write a JSON line to the bridge stdin. */
	private writeStdin(message: BridgeInput): void {
		if (!this.child) {
			return;
		}
		try {
			this.child.stdin.write(`${JSON.stringify(message)}\n`);
		} catch {
			// If stdin write fails, the process is likely dead; error is handled by exit handler
		}
	}

	/** Shut down the bridge process gracefully. */
	shutdown(): void {
		if (this.child) {
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

/** Singleton client instance. Created on first use. */
let rustCoreClientInstance: RustCoreClient | null = null;

function getRustCoreClient(): RustCoreClient {
	if (!rustCoreClientInstance) {
		rustCoreClientInstance = new RustCoreClient();
	}
	return rustCoreClientInstance;
}

// --- RustAgentLoopBackend ---

/**
 * AgentLoopBackend implementation that delegates to the Rust rozsa-core bridge.
 *
 * The bridge spawns as a child process and communicates via JSONL protocol.
 * Tool execution is delegated back to TS via tool_request/tool_result messages.
 */
export class RustAgentLoopBackend implements AgentLoopBackend {
	private client = getRustCoreClient();

	async runPrompt(
		prompts: AgentMessage[],
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		_streamFn?: StreamFn,
	): Promise<AgentMessage[]> {
		return this.client.startRun("prompt", prompts, context, config, emit, signal);
	}

	async runContinue(
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		_streamFn?: StreamFn,
	): Promise<AgentMessage[]> {
		return this.client.startRun("continue", [], context, config, emit, signal);
	}
}

/**
 * Resolve the appropriate AgentLoopBackend based on ROZSA_CORE_BACKEND env var.
 * - "rust": returns RustAgentLoopBackend (requires Node.js, spawns Rust bridge process)
 * - "ts" or unset: returns TsAgentLoopBackend (TS-native, existing behavior)
 *
 * Use this at application startup to pass the backend into AgentOptions.
 */
export function resolveAgentLoopBackend(): AgentLoopBackend {
	if (process.env.ROZSA_CORE_BACKEND === "rust") {
		return new RustAgentLoopBackend();
	}
	// Lazy import to keep this module self-contained — TsAgentLoopBackend has no Node deps
	return new TsAgentLoopBackend();
}
