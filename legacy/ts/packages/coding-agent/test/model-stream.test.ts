import type { Context, Model } from "@earendil-works/rozsa-model-types";
import { afterEach, describe, expect, it } from "vitest";
import { streamResolvedModel } from "../src/core/model-stream.ts";

const originalBinary = process.env.ROZSA_MODEL_BINARY;
const originalBinaryArgs = process.env.ROZSA_MODEL_BINARY_ARGS;

const context = {
	messages: [{ role: "user", content: "hello", timestamp: 1 }],
} satisfies Context;

afterEach(() => {
	restoreEnv("ROZSA_MODEL_BINARY", originalBinary);
	restoreEnv("ROZSA_MODEL_BINARY_ARGS", originalBinaryArgs);
});

describe("streamResolvedModel", () => {
	it("sends resolved custom provider credentials and headers directly to the Rust bridge", async () => {
		const bridgeScript = `
			const readline = require("node:readline");
			const rl = readline.createInterface({ input: process.stdin });
			rl.on("line", (line) => {
				const input = JSON.parse(line);
				const options = input.options || {};
				const headers = options.headers || {};
				const modelHeaders = input.model.headers || {};
				const ok =
					input.model.provider === "custom-openai" &&
					input.model.baseUrl === "https://custom.example/v1" &&
					options.apiKey === "resolved-key" &&
					headers.Authorization === "Bearer resolved-key" &&
					modelHeaders["x-model-header"] === "model" &&
					headers["x-option-header"] === "option";
				if (!ok) {
					console.log(JSON.stringify({
						type: "error",
						id: input.id,
						message: JSON.stringify({ model: input.model, options }),
						code: "assertion_failed",
					}));
					return;
				}
				const message = {
					role: "assistant",
					content: [{ type: "text", text: "ok" }],
					api: input.model.api,
					provider: input.model.provider,
					model: input.model.id,
					usage: {
						input: 1,
						output: 1,
						cacheRead: 0,
						cacheWrite: 0,
						totalTokens: 2,
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

		const model = {
			id: "custom-model",
			name: "Custom Model",
			api: "openai-completions",
			provider: "custom-openai",
			baseUrl: "https://custom.example/v1",
			reasoning: false,
			input: ["text"],
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
			contextWindow: 128000,
			maxTokens: 4096,
			headers: { "x-model-header": "model" },
		} satisfies Model<"openai-completions">;

		const stream = streamResolvedModel(model, context, {
			apiKey: "resolved-key",
			headers: {
				Authorization: "Bearer resolved-key",
				"x-option-header": "option",
			},
		});
		const result = await stream.result();

		expect(result.stopReason).toBe("stop");
		expect(result.provider).toBe("custom-openai");
		expect(result.content).toEqual([{ type: "text", text: "ok" }]);
	});
});

function restoreEnv(name: string, value: string | undefined): void {
	if (value === undefined) {
		delete process.env[name];
		return;
	}
	process.env[name] = value;
}
