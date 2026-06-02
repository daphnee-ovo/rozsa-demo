import type { AgentMessage } from "@earendil-works/pi-agent-core";
import { describe, expect, test } from "vitest";
import { nativeMessagesWithStreaming } from "../src/modes/native/native-mode.ts";

describe("native mode state", () => {
	test("includes the in-flight streaming message after committed messages", () => {
		const committed: AgentMessage[] = [
			{
				role: "user",
				content: [{ type: "text", text: "hello" }],
				timestamp: 1,
			},
		];
		const streaming: AgentMessage = {
			role: "assistant",
			content: [{ type: "text", text: "partial" }],
			api: "anthropic-messages",
			provider: "anthropic",
			model: "claude-sonnet-4-5",
			usage: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
				totalTokens: 0,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
			stopReason: "stop",
			timestamp: 2,
		};

		expect(nativeMessagesWithStreaming(committed, streaming)).toEqual([...committed, streaming]);
		expect(nativeMessagesWithStreaming(committed)).toEqual(committed);
		expect(nativeMessagesWithStreaming(committed)).not.toBe(committed);
	});
});
