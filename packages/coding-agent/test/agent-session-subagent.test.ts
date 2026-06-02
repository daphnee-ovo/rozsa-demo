import type { AssistantMessage, Model } from "@earendil-works/pi-ai";
import { afterEach, describe, expect, it } from "vitest";
import { createHarness, type Harness } from "./test-harness.ts";

describe("AgentSession subagents", () => {
	let harness: Harness;

	afterEach(() => {
		harness?.cleanup();
	});

	it("spawns a subagent with an injected system prompt and waits for its result", async () => {
		harness = createHarness({
			responses: [
				{
					toolCalls: [
						{
							name: "subagent",
							args: {
								action: "spawn",
								name: "research",
								system_prompt: "You are the research subagent.",
								prompt: "Find the answer.",
								wait: true,
							},
						},
					],
				},
				"child done",
				"main done",
			],
		});

		await harness.session.prompt("delegate");

		const subagents = harness.session.listSubagents();
		expect(subagents).toHaveLength(1);
		expect(subagents[0].name).toBe("research");
		expect(subagents[0].systemPrompt).toBe("You are the research subagent.");
		expect(subagents[0].status).toBe("idle");

		const snapshot = harness.session.getSubagentSnapshot(subagents[0].id);
		expect(snapshot?.messages.map((message) => message.role)).toEqual(["user", "assistant"]);
		expect((snapshot?.messages[1] as AssistantMessage).content).toEqual([{ type: "text", text: "child done" }]);

		expect(harness.faux.contexts[1].systemPrompt).toBe("You are the research subagent.");
		expect(harness.session.messages.some((message) => message.role === "assistant")).toBe(true);
		expect(harness.eventsOfType("subagent_created")).toHaveLength(1);
		expect(harness.eventsOfType("subagent_event").length).toBeGreaterThan(0);
	});

	it("can spawn a subagent with a requested model and thinking level", async () => {
		harness = createHarness({
			responses: ["child done"],
		});
		const childModel: Model<"anthropic-messages"> = {
			id: "child-thinking",
			name: "Child Thinking",
			api: "anthropic-messages",
			provider: "child-provider",
			baseUrl: "http://localhost:0",
			reasoning: true,
			input: ["text"],
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
			contextWindow: 128000,
			maxTokens: 16384,
		};
		harness.session.modelRegistry.registerProvider("child-provider", {
			api: "anthropic-messages",
			apiKey: "child-key",
			baseUrl: "http://localhost:0",
			models: [childModel],
		});

		const result = await harness.session.executeSubagentTool({
			action: "spawn",
			name: "specialist",
			system_prompt: "You are the specialist subagent.",
			prompt: "Use the requested model.",
			model: "child-provider/child-thinking",
			thinking_level: "high",
			wait: true,
		});

		const snapshot = harness.session.getSubagentSnapshot(result.details.id!);
		expect(snapshot?.info.model).toEqual({ provider: "child-provider", id: "child-thinking" });
		expect(snapshot?.info.thinkingLevel).toBe("high");
		expect(result.details.model).toEqual({ provider: "child-provider", id: "child-thinking" });
		expect(result.details.thinkingLevel).toBe("high");
		expect(harness.faux.models[0]).toMatchObject({ provider: "child-provider", id: "child-thinking" });
		expect(harness.faux.options[0]?.reasoning).toBe("high");
	});

	it("can interrupt a running subagent", async () => {
		harness = createHarness({
			responses: [{ text: "slow child", delayMs: 100 }],
		});

		const result = await harness.session.executeSubagentTool({
			action: "spawn",
			name: "slow",
			system_prompt: "You are slow.",
			prompt: "Wait.",
			wait: false,
		});
		const id = result.details.id;
		expect(id).toBeDefined();

		await harness.session.abortSubagent(id!);

		const snapshot = harness.session.getSubagentSnapshot(id!);
		expect(snapshot?.info.status).toBe("aborted");
	});
});
