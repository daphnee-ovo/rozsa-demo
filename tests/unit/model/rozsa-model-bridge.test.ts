import { afterEach, describe, expect, it } from "vitest";
import {
	parseBridgeLine,
	resolveRustModelBinaryArgs,
	shouldUseRustModelProvider,
	streamSimpleRustModel,
} from "../../../packages/ai/src/providers/rozsa-model-bridge.ts";
import { getApiProvider } from "../../../packages/ai/src/api-registry.ts";
import { resetApiProviders } from "../../../packages/ai/src/providers/register-builtins.ts";
import type { Context, Model } from "../../../packages/ai/src/types.ts";

const originalBinary = process.env.ROZSA_MODEL_BINARY;
const originalBinaryArgs = process.env.ROZSA_MODEL_BINARY_ARGS;
const originalBackend = process.env.ROZSA_MODEL_BACKEND;

const model = {
	id: "gpt-test",
	name: "GPT Test",
	api: "openai-completions",
	provider: "openai",
	baseUrl: "https://api.openai.com/v1",
	reasoning: false,
	input: ["text"],
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
	contextWindow: 128000,
	maxTokens: 4096,
} satisfies Model<"openai-completions">;

const context = {
	messages: [{ role: "user", content: "hello", timestamp: 1 }],
} satisfies Context;

afterEach(() => {
	if (originalBinary === undefined) {
		delete process.env.ROZSA_MODEL_BINARY;
	} else {
		process.env.ROZSA_MODEL_BINARY = originalBinary;
	}
	if (originalBinaryArgs === undefined) {
		delete process.env.ROZSA_MODEL_BINARY_ARGS;
	} else {
		process.env.ROZSA_MODEL_BINARY_ARGS = originalBinaryArgs;
	}
	if (originalBackend === undefined) {
		delete process.env.ROZSA_MODEL_BACKEND;
	} else {
		process.env.ROZSA_MODEL_BACKEND = originalBackend;
	}
	delete process.env.ROZSA_MODEL_RUST_APIS;
});

describe("rozsa-model bridge", () => {
	it("parses bridge lines defensively", () => {
		expect(parseBridgeLine("not json")).toBeUndefined();
		expect(parseBridgeLine('{"type":"unknown"}')).toBeUndefined();
		expect(parseBridgeLine('{"type":"error","id":"req","message":"failed"}')).toEqual({
			type: "error",
			id: "req",
			message: "failed",
		});
	});

	describe("shouldUseRustModelProvider backend semantics", () => {
		it("backend=rust returns true for supported API", () => {
			process.env.ROZSA_MODEL_BACKEND = "rust";
			process.env.ROZSA_MODEL_BINARY = "/nonexistent/path/rozsa-model";
			expect(shouldUseRustModelProvider("openai-completions")).toBe(true);
			expect(shouldUseRustModelProvider("anthropic-messages")).toBe(true);
			expect(shouldUseRustModelProvider("bedrock-converse-stream")).toBe(true);
		});

		it("backend=rust returns false for unsupported API", () => {
			process.env.ROZSA_MODEL_BACKEND = "rust";
			expect(shouldUseRustModelProvider("google-generative-ai")).toBe(false);
		});

		it("backend=auto is rejected", () => {
			process.env.ROZSA_MODEL_BACKEND = "auto";
			expect(() => shouldUseRustModelProvider("openai-completions")).toThrow(
				'ROZSA_MODEL_BACKEND must be "ts" or "rust".',
			);
		});

		it("backend=ts returns false regardless", () => {
			process.env.ROZSA_MODEL_BACKEND = "ts";
			expect(shouldUseRustModelProvider("openai-completions")).toBe(false);
		});

		it("backend unset returns false", () => {
			delete process.env.ROZSA_MODEL_BACKEND;
			expect(shouldUseRustModelProvider("openai-completions")).toBe(false);
		});
	});

	describe("backend=rust strict mode rejects unmigrated APIs", () => {
		it("unmigrated API returns error stream", async () => {
			process.env.ROZSA_MODEL_BACKEND = "rust";

			resetApiProviders();

			const provider = getApiProvider("anthropic-messages");
			expect(provider).toBeDefined();

			const bedrockModel = {
				id: "claude-test",
				name: "Claude Test",
				api: "anthropic-messages" as const,
				provider: "anthropic",
				baseUrl: "https://api.anthropic.com",
				reasoning: false,
				input: ["text" as const],
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
				contextWindow: 200000,
				maxTokens: 4096,
			};
			const stream = provider!.stream(bedrockModel, context);
			const result = await stream.result();
			expect(result.stopReason).toBe("error");
			expect(result.errorMessage).toContain("not available through the Rust bridge");
		});

		it("migrated API is not blocked", () => {
			process.env.ROZSA_MODEL_BACKEND = "rust";

			resetApiProviders();

			const provider = getApiProvider("openai-completions");
			expect(provider).toBeDefined();
			expect(provider!.stream).not.toBe(getApiProvider("anthropic-messages")!.stream);
		});

		it("routes Bedrock through the Rust bridge when listed", async () => {
			const bridgeScript = `
				const readline = require("node:readline");
				const rl = readline.createInterface({ input: process.stdin });
				rl.on("line", (line) => {
					const input = JSON.parse(line);
					const message = {
						role: "assistant",
						content: [{ type: "text", text: "hello from bedrock rust" }],
						api: input.model.api,
						provider: input.model.provider,
						model: input.model.id,
						usage: {
							input: 1,
							output: 2,
							cacheRead: 0,
							cacheWrite: 0,
							totalTokens: 3,
							cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
						},
						stopReason: "stop",
						timestamp: 123,
					};
					console.log(JSON.stringify({
						type: "event",
						id: input.id,
						event: { type: "done", reason: "stop", message },
					}));
				});
			`;
			process.env.ROZSA_MODEL_BACKEND = "rust";
			process.env.ROZSA_MODEL_BINARY = process.execPath;
			process.env.ROZSA_MODEL_BINARY_ARGS = JSON.stringify(["-e", bridgeScript]);
			resetApiProviders();

			const provider = getApiProvider("bedrock-converse-stream");
			expect(provider).toBeDefined();

			const bedrockModel = {
				id: "anthropic.claude-test",
				name: "Claude Test",
				api: "bedrock-converse-stream" as const,
				provider: "amazon-bedrock",
				baseUrl: "",
				reasoning: false,
				input: ["text" as const],
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
				contextWindow: 200000,
				maxTokens: 4096,
			};
			const stream = provider!.stream(bedrockModel, context);
			const result = await stream.result();

			expect(result.stopReason).toBe("stop");
			expect(result.content).toEqual([{ type: "text", text: "hello from bedrock rust" }]);
		});

	});

	it("streams through a JSONL bridge process", async () => {
		const bridgeScript = `
			const readline = require("node:readline");
			const rl = readline.createInterface({ input: process.stdin });
			rl.on("line", (line) => {
				const input = JSON.parse(line);
				const message = {
					role: "assistant",
					content: [{ type: "text", text: "hello from rust" }],
					api: input.model.api,
					provider: input.model.provider,
					model: input.model.id,
					usage: {
						input: 1,
						output: 2,
						cacheRead: 0,
						cacheWrite: 0,
						totalTokens: 3,
						cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
					},
					stopReason: "stop",
					timestamp: 123,
				};
				console.log(JSON.stringify({
					type: "event",
					id: input.id,
					event: { type: "done", reason: "stop", message },
				}));
			});
		`;
		process.env.ROZSA_MODEL_BINARY = process.execPath;
		process.env.ROZSA_MODEL_BINARY_ARGS = JSON.stringify(["-e", bridgeScript]);

		expect(resolveRustModelBinaryArgs()).toHaveLength(2);
		const stream = streamSimpleRustModel(model, context, { apiKey: "test-key" });
		const result = await stream.result();

		expect(result.stopReason).toBe("stop");
		expect(result.content).toEqual([{ type: "text", text: "hello from rust" }]);
		expect(result.usage.totalTokens).toBe(3);
	});
});
