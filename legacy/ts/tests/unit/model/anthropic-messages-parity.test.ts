import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { streamSimpleAnthropic } from "../../../packages/ai/src/providers/anthropic.ts";
import { streamSimpleRustModel } from "../../../packages/ai/src/providers/rozsa-model-bridge.ts";
import type { AssistantMessage, AssistantMessageEvent, Context, Model, SimpleStreamOptions } from "../../../packages/ai/src/types.ts";

interface CapturedRequest {
	method?: string;
	path?: string;
	headers: Record<string, string | string[] | undefined>;
	body: unknown;
}

interface FakeServer {
	baseUrl: string;
	requests: CapturedRequest[];
	close: () => Promise<void>;
}

const originalBinary = process.env.ROZSA_MODEL_BINARY;
const originalBinaryArgs = process.env.ROZSA_MODEL_BINARY_ARGS;

afterEach(() => {
	restoreEnv("ROZSA_MODEL_BINARY", originalBinary);
	restoreEnv("ROZSA_MODEL_BINARY_ARGS", originalBinaryArgs);
});

describe("Anthropic messages TS/Rust parity", () => {
	it("matches request payloads and final message for text, thinking, tool calls, and usage", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		const fake = await startAnthropicFakeServer();
		try {
			const model = createModel(fake.baseUrl);
			const context = createContext();
			const options: SimpleStreamOptions = {
				apiKey: "sk-ant-test-key-123",
				maxTokens: 64,
				temperature: 0.2,
				headers: { "X-Test-Header": "from-test" },
			};

			const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
			const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

			expect(fake.requests).toHaveLength(2);
			const [tsRequest, rustRequest] = fake.requests;
			expect(tsRequest.path).toBe("/v1/messages");
			expect(rustRequest.path).toBe("/v1/messages");

			const tsBody = tsRequest.body as Record<string, unknown>;
			const rustBody = rustRequest.body as Record<string, unknown>;
			expect(tsBody.model).toBe(rustBody.model);
			expect(tsBody.max_tokens).toBe(rustBody.max_tokens);
			expect(tsBody.temperature).toBe(rustBody.temperature);
			expect(tsBody.stream).toBe(true);
			expect(rustBody.stream).toBe(true);
			expect(normalizeMessages(tsBody.messages)).toEqual(normalizeMessages(rustBody.messages));
			expect(normalizeTools(tsBody.tools)).toEqual(normalizeTools(rustBody.tools));

			expect(tsRequest.headers["x-api-key"]).toBe("sk-ant-test-key-123");
			expect(rustRequest.headers["x-api-key"]).toBe("sk-ant-test-key-123");
			expect(tsRequest.headers["x-test-header"]).toBe("from-test");
			expect(rustRequest.headers["x-test-header"]).toBe("from-test");
			expect(tsRequest.headers["anthropic-version"]).toBe("2023-06-01");
			expect(rustRequest.headers["anthropic-version"]).toBe("2023-06-01");

			expect(stripVolatileMessage(ts.result)).toEqual(stripVolatileMessage(rust.result));
			expect(eventTypes(ts.events)).toEqual(eventTypes(rust.events));
		} finally {
			await fake.close();
		}
	});

	it("matches OAuth bearer auth and headers", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		const fake = await startAnthropicFakeServer();
		try {
			const model = createModel(fake.baseUrl);
			const context = createMinimalContext();
			const options: SimpleStreamOptions = {
				apiKey: "sk-ant-oat-oauth-token-test",
				maxTokens: 32,
			};

			const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
			const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

			expect(fake.requests).toHaveLength(2);
			const [tsRequest, rustRequest] = fake.requests;

			expect(tsRequest.headers.authorization).toBe("Bearer sk-ant-oat-oauth-token-test");
			expect(rustRequest.headers.authorization).toBe("Bearer sk-ant-oat-oauth-token-test");
			expect(tsRequest.headers["x-api-key"]).toBeUndefined();
			expect(rustRequest.headers["x-api-key"]).toBeUndefined();

			expect(stripVolatileMessage(ts.result)).toEqual(stripVolatileMessage(rust.result));
		} finally {
			await fake.close();
		}
	});

	it("matches thinking-enabled response with signature", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		const fake = await startAnthropicFakeServer({ thinking: true });
		try {
			const model = createThinkingModel(fake.baseUrl);
			const context = createMinimalContext();
			const options: SimpleStreamOptions = {
				apiKey: "sk-ant-test-key-123",
				maxTokens: 128,
				reasoning: "high",
			};

			const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
			const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

			expect(fake.requests).toHaveLength(2);
			const [tsRequest, rustRequest] = fake.requests;

			const tsBody = tsRequest.body as Record<string, unknown>;
			const rustBody = rustRequest.body as Record<string, unknown>;
			expect(tsBody.thinking).toBeDefined();
			expect(rustBody.thinking).toBeDefined();

			expect(stripVolatileMessage(ts.result)).toEqual(stripVolatileMessage(rust.result));
			expect(eventTypes(ts.events)).toEqual(eventTypes(rust.events));
		} finally {
			await fake.close();
		}
	});

	it("matches stop_reason mapping", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		for (const stopReason of ["end_turn", "max_tokens", "tool_use"]) {
			const fake = await startAnthropicFakeServer({ stopReason });
			try {
				const model = createModel(fake.baseUrl);
				const context = createMinimalContext();
				const options: SimpleStreamOptions = {
					apiKey: "sk-ant-test-key-123",
					maxTokens: 32,
				};

				const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
				const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

				expect(ts.result.stopReason).toBe(rust.result.stopReason);
			} finally {
				await fake.close();
			}
		}
	});

	it("matches usage and cost calculation", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		const fake = await startAnthropicFakeServer();
		try {
			const model = createModel(fake.baseUrl);
			const context = createMinimalContext();
			const options: SimpleStreamOptions = {
				apiKey: "sk-ant-test-key-123",
				maxTokens: 32,
			};

			const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
			const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

			expect(ts.result.usage).toEqual(rust.result.usage);
		} finally {
			await fake.close();
		}
	});
	it("matches custom provider (Fireworks compat) payload and response", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;


		const fake = await startAnthropicFakeServer();
		try {
			const model = createFireworksModel(fake.baseUrl);
			const context = createContext();
			const options: SimpleStreamOptions = {
				apiKey: "fw-test-key-abc",
				maxTokens: 64,
				sessionId: "sess-fw-001",
			};

			const ts = await runAndCollect(streamSimpleAnthropic(model, context, options));
			const rust = await runAndCollect(streamSimpleRustModel(model, context, options));

			expect(fake.requests).toHaveLength(2);
			const [tsRequest, rustRequest] = fake.requests;

			const tsBody = tsRequest.body as Record<string, unknown>;
			const rustBody = rustRequest.body as Record<string, unknown>;

			// Fireworks: no cache_control on tools
			const tsTools = tsBody.tools as Record<string, unknown>[];
			const rustTools = rustBody.tools as Record<string, unknown>[];
			for (const tool of tsTools) expect(tool.cache_control).toBeUndefined();
			for (const tool of rustTools) expect(tool.cache_control).toBeUndefined();

			// Fireworks: no eager_input_streaming
			for (const tool of tsTools) expect(tool.eager_input_streaming).toBeUndefined();
			for (const tool of rustTools) expect(tool.eager_input_streaming).toBeUndefined();

			// Session affinity header present
			expect(tsRequest.headers["x-session-affinity"]).toBe("sess-fw-001");
			expect(rustRequest.headers["x-session-affinity"]).toBe("sess-fw-001");

			// Standard API key auth (not OAuth)
			expect(tsRequest.headers["x-api-key"]).toBe("fw-test-key-abc");
			expect(rustRequest.headers["x-api-key"]).toBe("fw-test-key-abc");

			expect(stripVolatileMessage(ts.result)).toEqual(stripVolatileMessage(rust.result));
			expect(eventTypes(ts.events)).toEqual(eventTypes(rust.events));
		} finally {
			await fake.close();
		}
	});
});

function createModel(baseUrl: string): Model<"anthropic-messages"> {
	return {
		id: "claude-sonnet-4-20250514",
		name: "Claude Sonnet 4",
		api: "anthropic-messages",
		provider: "anthropic",
		baseUrl,
		reasoning: false,
		input: ["text", "image"],
		cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 },
		contextWindow: 200000,
		maxTokens: 8192,
	};
}

function createFireworksModel(baseUrl: string): Model<"anthropic-messages"> {
	return {
		id: "accounts/fireworks/models/claude-sonnet-4",
		name: "Claude Sonnet 4 (Fireworks)",
		api: "anthropic-messages",
		provider: "fireworks",
		baseUrl,
		reasoning: false,
		input: ["text"],
		cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 },
		contextWindow: 200000,
		maxTokens: 8192,
		compat: {
			supportsEagerToolInputStreaming: false,
			supportsLongCacheRetention: false,
			sendSessionAffinityHeaders: true,
			supportsCacheControlOnTools: false,
			forceAdaptiveThinking: false,
		},
	};
}

function createThinkingModel(baseUrl: string): Model<"anthropic-messages"> {
	return {
		id: "claude-sonnet-4-20250514",
		name: "Claude Sonnet 4",
		api: "anthropic-messages",
		provider: "anthropic",
		baseUrl,
		reasoning: true,
		input: ["text", "image"],
		cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 },
		contextWindow: 200000,
		maxTokens: 8192,
	};
}

function createContext(): Context {
	return {
		systemPrompt: "Be concise.",
		messages: [{ role: "user", content: "Use the lookup tool.", timestamp: 1 }],
		tools: [
			{
				name: "lookup",
				description: "Lookup a value",
				parameters: { type: "object", properties: { key: { type: "string" } }, required: ["key"] },
			},
		],
	};
}

function createMinimalContext(): Context {
	return {
		systemPrompt: "Be concise.",
		messages: [{ role: "user", content: "Reply with exactly: ok", timestamp: 1 }],
	};
}

async function runAndCollect(stream: AsyncIterable<AssistantMessageEvent> & { result(): Promise<AssistantMessage> }) {
	const events: AssistantMessageEvent[] = [];
	for await (const event of stream) {
		events.push(event);
	}
	return { events, result: await stream.result() };
}

function eventTypes(events: AssistantMessageEvent[]): string[] {
	return events.map((event) => event.type);
}

function stripVolatileMessage(message: AssistantMessage): Omit<AssistantMessage, "timestamp"> {
	const { timestamp: _timestamp, ...rest } = message;
	return rest;
}

function normalizeMessages(messages: unknown): unknown {
	if (!Array.isArray(messages)) return messages;
	return messages.map((msg) => {
		const { cache_control: _cc, ...rest } = msg as Record<string, unknown>;
		if (Array.isArray(rest.content)) {
			rest.content = (rest.content as Record<string, unknown>[]).map((block) => {
				const { cache_control: _bcc, ...brest } = block;
				return brest;
			});
		}
		return rest;
	});
}

function normalizeTools(tools: unknown): unknown {
	if (!Array.isArray(tools)) return tools;
	return tools.map((tool) => {
		const { cache_control: _cc, eager_input_streaming: _eis, ...rest } = tool as Record<string, unknown>;
		return rest;
	});
}

interface FakeServerOptions {
	thinking?: boolean;
	stopReason?: string;
}

async function startAnthropicFakeServer(opts?: FakeServerOptions): Promise<FakeServer> {
	const requests: CapturedRequest[] = [];
	const server = createServer(async (request, response) => {
		const body = await readBody(request);
		requests.push({
			method: request.method,
			path: request.url,
			headers: request.headers,
			body: JSON.parse(body),
		});
		writeAnthropicSseResponse(response, opts);
	});
	await listen(server);
	const address = server.address();
	if (!address || typeof address === "string") {
		throw new Error("server did not bind to a TCP address");
	}
	return {
		baseUrl: `http://127.0.0.1:${address.port}`,
		requests,
		close: () => new Promise((resolveClose) => server.close(() => resolveClose())),
	};
}

function listen(server: Server): Promise<void> {
	return new Promise((resolveListen, rejectListen) => {
		server.on("error", rejectListen);
		server.listen(0, "127.0.0.1", () => resolveListen());
	});
}

function readBody(request: IncomingMessage): Promise<string> {
	return new Promise((resolveBody, rejectBody) => {
		const chunks: Buffer[] = [];
		request.on("data", (chunk: Buffer) => chunks.push(chunk));
		request.on("error", rejectBody);
		request.on("end", () => resolveBody(Buffer.concat(chunks).toString("utf8")));
	});
}

function writeAnthropicSseResponse(response: ServerResponse, opts?: FakeServerOptions): void {
	response.writeHead(200, { "Content-Type": "text/event-stream" });
	const stopReason = opts?.stopReason ?? "end_turn";

	response.write(
		'event: message_start\ndata: {"type":"message_start","message":{"id":"msg_parity_001","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-20250514","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":5,"cache_creation_input_tokens":2}}}\n\n',
	);

	if (opts?.thinking) {
		response.write(
			'event: content_block_start\ndata: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}\n\n',
		);
		response.write(
			'event: content_block_delta\ndata: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}\n\n',
		);
		response.write(
			'event: content_block_delta\ndata: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_abc123"}}\n\n',
		);
		response.write('event: content_block_stop\ndata: {"type":"content_block_stop","index":0}\n\n');

		response.write(
			'event: content_block_start\ndata: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}\n\n',
		);
		response.write(
			'event: content_block_delta\ndata: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hello"}}\n\n',
		);
		response.write('event: content_block_stop\ndata: {"type":"content_block_stop","index":1}\n\n');
	} else {
		response.write(
			'event: content_block_start\ndata: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}\n\n',
		);
		response.write(
			'event: content_block_delta\ndata: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}\n\n',
		);
		response.write('event: content_block_stop\ndata: {"type":"content_block_stop","index":0}\n\n');

		if (stopReason === "tool_use") {
			response.write(
				'event: content_block_start\ndata: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_parity_001","name":"lookup","input":{}}}\n\n',
			);
			response.write(
				'event: content_block_delta\ndata: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\\"key\\":\\"value\\"}"}}\n\n',
			);
			response.write('event: content_block_stop\ndata: {"type":"content_block_stop","index":1}\n\n');
		}
	}

	response.write(
		`event: message_delta\ndata: {"type":"message_delta","delta":{"stop_reason":"${stopReason}"},"usage":{"output_tokens":7}}\n\n`,
	);
	response.write('event: message_stop\ndata: {"type":"message_stop"}\n\n');
	response.end();
}

function restoreEnv(name: string, original: string | undefined): void {
	if (original === undefined) {
		delete process.env[name];
	} else {
		process.env[name] = original;
	}
}
