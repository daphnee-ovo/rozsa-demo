import { afterEach, describe, expect, it } from "vitest";
import {
	parseBridgeLine,
	resolveRustModelBinaryArgs,
	streamSimpleRustModel,
} from "../../../packages/ai/src/providers/rozsa-model-bridge.ts";
import type { Context, Model } from "../../../packages/ai/src/types.ts";

const originalBinary = process.env.ROZSA_MODEL_BINARY;
const originalBinaryArgs = process.env.ROZSA_MODEL_BINARY_ARGS;

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
