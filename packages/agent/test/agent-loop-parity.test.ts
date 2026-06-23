import {
	type AssistantMessage,
	type AssistantMessageEvent,
	EventStream,
	type Message,
	type Model,
	type UserMessage,
} from "@earendil-works/rozsa-ai";
import { Type } from "typebox";
import { describe, expect, it } from "vitest";
import { agentLoop } from "../src/agent-loop.ts";
import type { AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentTool } from "../src/types.ts";

class ParityAssistantStream extends EventStream<AssistantMessageEvent, AssistantMessage> {
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

function createUsage(): AssistantMessage["usage"] {
	return {
		input: 0,
		output: 0,
		cacheRead: 0,
		cacheWrite: 0,
		totalTokens: 0,
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
	};
}

function createModel(): Model<"openai-responses"> {
	return {
		id: "mock",
		name: "mock",
		api: "openai-responses",
		provider: "openai",
		baseUrl: "https://example.invalid",
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 8192,
		maxTokens: 2048,
	};
}

function createAssistantMessage(
	content: AssistantMessage["content"],
	stopReason: AssistantMessage["stopReason"] = "stop",
	errorMessage?: string,
): AssistantMessage {
	return {
		role: "assistant",
		content,
		api: "openai-responses",
		provider: "openai",
		model: "mock",
		usage: createUsage(),
		stopReason,
		errorMessage,
		timestamp: 1,
	};
}

function createUserMessage(text: string): UserMessage {
	return {
		role: "user",
		content: text,
		timestamp: 1,
	};
}

function identityConverter(messages: AgentMessage[]): Message[] {
	return messages.filter((m) => m.role === "user" || m.role === "assistant" || m.role === "toolResult") as Message[];
}

function defaultConfig(overrides: Partial<AgentLoopConfig> = {}): AgentLoopConfig {
	return {
		model: createModel(),
		convertToLlm: identityConverter,
		...overrides,
	};
}

async function collectLoop(
	context: AgentContext,
	config: AgentLoopConfig,
	streamFn: Parameters<typeof agentLoop>[4],
	prompt: AgentMessage = createUserMessage("hello"),
): Promise<{ events: AgentEvent[]; messages: AgentMessage[] }> {
	const events: AgentEvent[] = [];
	const stream = agentLoop([prompt], context, config, undefined, streamFn);

	for await (const event of stream) {
		events.push(event);
	}

	return { events, messages: await stream.result() };
}

function streamFromEvents(events: AssistantMessageEvent[]): ParityAssistantStream {
	const stream = new ParityAssistantStream();
	queueMicrotask(() => {
		for (const event of events) {
			stream.push(event);
		}
	});
	return stream;
}

function eventSignature(event: AgentEvent): string {
	if (event.type === "message_start" || event.type === "message_end") {
		return `${event.type}:${event.message.role}`;
	}
	if (event.type === "message_update") {
		return `${event.type}:${event.assistantMessageEvent.type}`;
	}
	if (event.type === "tool_execution_start") {
		return `${event.type}:${event.toolCallId}:${event.toolName}`;
	}
	if (event.type === "tool_execution_end") {
		return `${event.type}:${event.toolCallId}:${event.toolName}:${String(event.isError)}`;
	}
	if (event.type === "turn_end") {
		const msg = event.message as { stopReason?: string };
		return `${event.type}:${msg.stopReason ?? ""}:${event.toolResults.map((result) => result.toolCallId).join(",")}`;
	}
	return event.type;
}

describe("agent loop parity fixtures", () => {
	it("captures the no-tool prompt event order and persisted messages", async () => {
		const finalMessage = createAssistantMessage([{ type: "text", text: "hi" }]);
		const { events, messages } = await collectLoop(
			{ systemPrompt: "system", messages: [], tools: [] },
			defaultConfig(),
			() =>
				streamFromEvents([
					{ type: "start", partial: createAssistantMessage([]) },
					{ type: "text_delta", contentIndex: 0, delta: "hi", partial: finalMessage },
					{ type: "done", reason: "stop", message: finalMessage },
				]),
		);

		expect(events.map(eventSignature)).toEqual([
			"agent_start",
			"turn_start",
			"message_start:user",
			"message_end:user",
			"message_start:assistant",
			"message_update:text_delta",
			"message_end:assistant",
			"turn_end:stop:",
			"agent_end",
		]);
		expect(messages.map((message) => message.role)).toEqual(["user", "assistant"]);
		expect(messages[1]).toMatchObject({ role: "assistant", content: [{ type: "text", text: "hi" }] });
	});

	it("captures provider error and abort stop reasons without tool execution", async () => {
		const errorMessage = createAssistantMessage([], "error", "provider failed");
		const abortedMessage = createAssistantMessage([], "aborted", "aborted");

		const errorRun = await collectLoop({ systemPrompt: "", messages: [], tools: [] }, defaultConfig(), () =>
			streamFromEvents([{ type: "error", reason: "error", error: errorMessage }]),
		);
		const abortRun = await collectLoop({ systemPrompt: "", messages: [], tools: [] }, defaultConfig(), () =>
			streamFromEvents([{ type: "error", reason: "aborted", error: abortedMessage }]),
		);

		expect(errorRun.events.map(eventSignature)).toEqual([
			"agent_start",
			"turn_start",
			"message_start:user",
			"message_end:user",
			"message_start:assistant",
			"message_end:assistant",
			"turn_end:error:",
			"agent_end",
		]);
		expect(abortRun.events.map(eventSignature)).toContain("turn_end:aborted:");
		expect(errorRun.messages.at(-1)).toMatchObject({ role: "assistant", stopReason: "error" });
		expect(abortRun.messages.at(-1)).toMatchObject({ role: "assistant", stopReason: "aborted" });
	});

	it("captures beforeToolCall block behavior and converts it into a toolResult message", async () => {
		const toolSchema = Type.Object({ value: Type.String() });
		let executed = false;
		const tool: AgentTool<typeof toolSchema, { value: string }> = {
			name: "echo",
			label: "Echo",
			description: "Echo tool",
			parameters: toolSchema,
			async execute(_toolCallId: string, _params: { value: string }) {
				executed = true;
				return { content: [{ type: "text", text: "unused" }], details: { value: "" } };
			},
		};
		let callIndex = 0;

		const { events, messages } = await collectLoop(
			{ systemPrompt: "", messages: [], tools: [tool] },
			defaultConfig({
				beforeToolCall: async () => ({ block: true, reason: "blocked by policy" }),
			}),
			() =>
				streamFromEvents([
					callIndex++ === 0
						? {
								type: "done",
								reason: "toolUse",
								message: createAssistantMessage(
									[{ type: "toolCall", id: "tool-1", name: "echo", arguments: { value: "x" } }],
									"toolUse",
								),
							}
						: { type: "done", reason: "stop", message: createAssistantMessage([{ type: "text", text: "done" }]) },
				]),
		);

		const toolResult = messages.find((message) => message.role === "toolResult");
		expect(executed).toBe(false);
		expect(events.map(eventSignature)).toContain("tool_execution_end:tool-1:echo:true");
		expect(toolResult).toMatchObject({
			role: "toolResult",
			toolCallId: "tool-1",
			toolName: "echo",
			isError: true,
			content: [{ type: "text", text: "blocked by policy" }],
		});
	});

	it("captures steering queue insertion after a tool batch before the next model call", async () => {
		const toolSchema = Type.Object({ value: Type.String() });
		const tool: AgentTool<typeof toolSchema, { value: string }> = {
			name: "echo",
			label: "Echo",
			description: "Echo tool",
			parameters: toolSchema,
			async execute(_toolCallId, params) {
				return { content: [{ type: "text", text: params.value }], details: params };
			},
		};
		let callIndex = 0;
		let sawQueuedMessage = false;
		let queueDelivered = false;

		await collectLoop(
			{ systemPrompt: "", messages: [], tools: [tool] },
			defaultConfig({
				getSteeringMessages: async () => {
					if (queueDelivered) return [];
					queueDelivered = true;
					return [createUserMessage("interrupt")];
				},
			}),
			(_model, context) => {
				if (callIndex === 1) {
					sawQueuedMessage = context.messages.some(
						(message) => message.role === "user" && message.content === "interrupt",
					);
				}
				const stream = streamFromEvents([
					callIndex++ === 0
						? {
								type: "done",
								reason: "toolUse",
								message: createAssistantMessage(
									[{ type: "toolCall", id: "tool-1", name: "echo", arguments: { value: "x" } }],
									"toolUse",
								),
							}
						: { type: "done", reason: "stop", message: createAssistantMessage([{ type: "text", text: "done" }]) },
				]);
				return stream;
			},
		);

		expect(sawQueuedMessage).toBe(true);
	});

	it("captures terminate=true as the end of the loop after tool execution", async () => {
		const toolSchema = Type.Object({ value: Type.String() });
		const tool: AgentTool<typeof toolSchema, { value: string }> = {
			name: "exit",
			label: "Exit",
			description: "Exit tool",
			parameters: toolSchema,
			async execute(_toolCallId: string, _params: { value: string }) {
				return { content: [{ type: "text", text: "terminated" }], details: { value: "" }, terminate: true };
			},
		};
		let modelCalls = 0;

		const { events } = await collectLoop({ systemPrompt: "", messages: [], tools: [tool] }, defaultConfig(), () => {
			modelCalls += 1;
			return streamFromEvents([
				{
					type: "done",
					reason: "toolUse",
					message: createAssistantMessage(
						[{ type: "toolCall", id: "tool-1", name: "exit", arguments: { value: "x" } }],
						"toolUse",
					),
				},
			]);
		});

		expect(modelCalls).toBe(1);
		expect(events.map(eventSignature)).toEqual([
			"agent_start",
			"turn_start",
			"message_start:user",
			"message_end:user",
			"message_start:assistant",
			"message_end:assistant",
			"tool_execution_start:tool-1:exit",
			"tool_execution_end:tool-1:exit:false",
			"message_start:toolResult",
			"message_end:toolResult",
			"turn_end:toolUse:tool-1",
			"agent_end",
		]);
	});

	it("captures shouldStopAfterTurn before follow-up messages are requested", async () => {
		let followUpCalls = 0;
		const { events } = await collectLoop(
			{ systemPrompt: "", messages: [], tools: [] },
			defaultConfig({
				shouldStopAfterTurn: async () => true,
				getFollowUpMessages: async () => {
					followUpCalls += 1;
					return [createUserMessage("follow-up")];
				},
			}),
			() =>
				streamFromEvents([
					{ type: "done", reason: "stop", message: createAssistantMessage([{ type: "text", text: "done" }]) },
				]),
		);

		expect(followUpCalls).toBe(0);
		expect(events.map(eventSignature)).toEqual([
			"agent_start",
			"turn_start",
			"message_start:user",
			"message_end:user",
			"message_start:assistant",
			"message_end:assistant",
			"turn_end:stop:",
			"agent_end",
		]);
	});
});
