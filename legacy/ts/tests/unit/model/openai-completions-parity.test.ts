import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
	type OpenAICompletionsOptions,
	streamOpenAICompletions,
} from "../../../packages/ai/src/providers/openai-completions.ts";
import { streamRustModel } from "../../../packages/ai/src/providers/rozsa-model-bridge.ts";
import type { AssistantMessage, AssistantMessageEvent, Context, Model } from "../../../packages/ai/src/types.ts";

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
const liveIt = process.env.ROZSA_MODEL_LIVE_TS_BRIDGE === "1" ? it : it.skip;

afterEach(() => {
	restoreEnv("ROZSA_MODEL_BINARY", originalBinary);
	restoreEnv("ROZSA_MODEL_BINARY_ARGS", originalBinaryArgs);
});

describe("OpenAI completions TS/Rust parity", () => {
	it("matches request payloads and final message for text, reasoning, tool calls, and usage", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;

		const fake = await startOpenAICompatibleServer();
		try {
			const model = createModel(fake.baseUrl);
			const context = createContext();
			const options = {
				apiKey: "test-key",
				maxTokens: 64,
				temperature: 0.2,
				reasoningEffort: "high",
				toolChoice: "required",
				headers: { "X-Test-Header": "from-test" },
			} satisfies OpenAICompletionsOptions;

			const ts = await runAndCollect(streamOpenAICompletions(model, context, options));
			const rust = await runAndCollect(streamRustModel(model, context, options));

			expect(fake.requests).toHaveLength(2);
			const [tsRequest, rustRequest] = fake.requests;
			expect(tsRequest.path).toBe("/v1/chat/completions");
			expect(rustRequest.path).toBe("/v1/chat/completions");
			expect(tsRequest.body).toEqual(rustRequest.body);
			expect(tsRequest.headers["x-test-header"]).toBe("from-test");
			expect(rustRequest.headers["x-test-header"]).toBe("from-test");
			expect(tsRequest.headers.authorization).toBe("Bearer test-key");
			expect(rustRequest.headers.authorization).toBe("Bearer test-key");

			expect(stripVolatileMessage(ts.result)).toEqual(stripVolatileMessage(rust.result));
			expect(eventTypes(ts.events)).toEqual(eventTypes(rust.events));
		} finally {
			await fake.close();
		}
	});

	liveIt("streams through the TypeScript bridge into a local OpenAI-compatible server", async () => {
		const rustBinary = resolve(process.cwd(), "target", "debug", "rozsa-model");
		expect(existsSync(rustBinary)).toBe(true);
		process.env.ROZSA_MODEL_BINARY = rustBinary;
		delete process.env.ROZSA_MODEL_BINARY_ARGS;

		const baseUrl = requiredEnv("ROZSA_MODEL_LIVE_BASE_URL");
		const modelId = requiredEnv("ROZSA_MODEL_LIVE_MODEL");
		const apiKey = process.env.ROZSA_MODEL_LIVE_API_KEY || "dummy";
		const stream = streamRustModel(createLiveModel(baseUrl, modelId), createLiveContext(), {
			apiKey,
			maxTokens: 16,
			temperature: 0,
		});

		const collected = await runAndCollect(stream);
		expect(collected.events.some((event) => event.type === "error")).toBe(false);
		expect(messageText(collected.result).length).toBeGreaterThan(0);
		expect(collected.result.stopReason).not.toBe("error");
	});
});

/** Build a deterministic OpenAI-compatible model fixture for fake parity tests. */
function createModel(baseUrl: string): Model<"openai-completions"> {
	return {
		id: "test-model",
		name: "Test Model",
		api: "openai-completions",
		provider: "openai",
		baseUrl,
		reasoning: true,
		input: ["text"],
		cost: { input: 1, output: 2, cacheRead: 0.5, cacheWrite: 1.5 },
		contextWindow: 128000,
		maxTokens: 4096,
	};
}

/** Build the message context used by fake parity tests. */
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

/** Build a local live model fixture from user-provided endpoint settings. */
function createLiveModel(baseUrl: string, modelId: string): Model<"openai-completions"> {
	return {
		id: modelId,
		name: modelId,
		api: "openai-completions",
		provider: "openai",
		baseUrl,
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 2048,
		maxTokens: 256,
	};
}

/** Build the minimal prompt used for live local model smoke tests. */
function createLiveContext(): Context {
	return {
		systemPrompt: "Be concise.",
		messages: [{ role: "user", content: "Reply with exactly: ok", timestamp: 1 }],
	};
}

/** Drain an assistant event stream and return both events and final message. */
async function runAndCollect(stream: AsyncIterable<AssistantMessageEvent> & { result(): Promise<AssistantMessage> }) {
	const events: AssistantMessageEvent[] = [];
	for await (const event of stream) {
		events.push(event);
	}
	return { events, result: await stream.result() };
}

/** Extract event type names for low-noise sequence comparisons. */
function eventTypes(events: AssistantMessageEvent[]): string[] {
	return events.map((event) => event.type);
}

/** Remove runtime-only fields before comparing provider outputs. */
function stripVolatileMessage(message: AssistantMessage): Omit<AssistantMessage, "timestamp"> {
	const { timestamp: _timestamp, ...rest } = message;
	return rest;
}

/** Concatenate text blocks from an assistant message. */
function messageText(message: AssistantMessage): string {
	return message.content
		.filter((block) => block.type === "text")
		.map((block) => block.text)
		.join("");
}

/** Start a fake OpenAI-compatible SSE server and capture incoming requests. */
async function startOpenAICompatibleServer(): Promise<FakeServer> {
	const requests: CapturedRequest[] = [];
	const server = createServer(async (request, response) => {
		const body = await readBody(request);
		requests.push({
			method: request.method,
			path: request.url,
			headers: request.headers,
			body: JSON.parse(body),
		});
		writeSseResponse(response);
	});
	await listen(server);
	const address = server.address();
	if (!address || typeof address === "string") {
		throw new Error("server did not bind to a TCP address");
	}
	return {
		baseUrl: `http://127.0.0.1:${address.port}/v1`,
		requests,
		close: () => new Promise((resolveClose) => server.close(() => resolveClose())),
	};
}

/** Bind an HTTP server to an ephemeral local port. */
function listen(server: Server): Promise<void> {
	return new Promise((resolveListen, rejectListen) => {
		server.on("error", rejectListen);
		server.listen(0, "127.0.0.1", () => resolveListen());
	});
}

/** Read a full HTTP request body as UTF-8 text. */
function readBody(request: IncomingMessage): Promise<string> {
	return new Promise((resolveBody, rejectBody) => {
		const chunks: Buffer[] = [];
		request.on("data", (chunk: Buffer) => chunks.push(chunk));
		request.on("error", rejectBody);
		request.on("end", () => resolveBody(Buffer.concat(chunks).toString("utf8")));
	});
}

/** Write a deterministic OpenAI-compatible streaming response. */
function writeSseResponse(response: ServerResponse): void {
	response.writeHead(200, { "Content-Type": "text/event-stream" });
	response.write(
		'data: {"id":"chatcmpl_parity","model":"served-model","choices":[{"delta":{"reasoning_content":"think"},"finish_reason":null}]}\n\n',
	);
	response.write('data: {"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}\n\n');
	response.write(
		'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"lookup","arguments":"{\\"key\\""}}]},"finish_reason":null}]}\n\n',
	);
	response.write(
		'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\\"value\\"}"}}]},"finish_reason":null}]}\n\n',
	);
	response.write(
		'data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":2,"cache_write_tokens":1}}}\n\n',
	);
	response.write("data: [DONE]\n\n");
	response.end();
}

/** Restore an environment variable to the value it had before the test. */
function restoreEnv(name: string, original: string | undefined): void {
	if (original === undefined) {
		delete process.env[name];
	} else {
		process.env[name] = original;
	}
}

/** Read a required live-test environment variable or fail fast. */
function requiredEnv(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`${name} is required when ROZSA_MODEL_LIVE_TS_BRIDGE=1`);
	}
	return value;
}
