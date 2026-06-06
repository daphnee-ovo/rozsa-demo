import type { Model } from "@earendil-works/pi-model-types";
import { afterEach, describe, expect, it } from "vitest";
import { streamDefaultModel } from "../src/model-stream.ts";

const originalBinary = process.env.ROZSA_MODEL_BINARY;
const originalBinaryArgs = process.env.ROZSA_MODEL_BINARY_ARGS;

afterEach(() => {
	restoreEnv("ROZSA_MODEL_BINARY", originalBinary);
	restoreEnv("ROZSA_MODEL_BINARY_ARGS", originalBinaryArgs);
});

describe("streamDefaultModel", () => {
	it("routes requests through the Rust bridge", async () => {
		const bridgeScript = `
			const readline = require("node:readline");
			const rl = readline.createInterface({ input: process.stdin });
			rl.on("line", (line) => {
				const input = JSON.parse(line);
				if (input.options?.apiKey !== "agent-key") {
					console.log(JSON.stringify({
						type: "error",
						id: input.id,
						message: JSON.stringify(input.options || {}),
						code: "assertion_failed",
					}));
					return;
				}
				const message = {
					role: "assistant",
					content: [{ type: "text", text: "agent rust" }],
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
		} satisfies Model<"openai-completions">;

		const stream = streamDefaultModel(
			model,
			{ messages: [{ role: "user", content: "hello", timestamp: 1 }] },
			{ apiKey: "agent-key" },
		);
		const result = await stream.result();

		expect(result.stopReason).toBe("stop");
		expect(result.content).toEqual([{ type: "text", text: "agent rust" }]);
	});
});

function restoreEnv(name: string, value: string | undefined): void {
	if (value === undefined) {
		delete process.env[name];
		return;
	}
	process.env[name] = value;
}
