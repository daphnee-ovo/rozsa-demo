import { describe, expect, test } from "vitest";
import type { AgentSession } from "../src/core/agent-session.ts";
import type { AgentSessionRuntime } from "../src/core/agent-session-runtime.ts";
import { handleNativeBuiltinCommand, type NativeBuiltinContext } from "../src/modes/native/native-builtins.ts";

function makeContext(overrides: Partial<NativeBuiltinContext> = {}): NativeBuiltinContext {
	const settingsManager = {
		getShowImages: () => true,
		getImageAutoResize: () => true,
		getBlockImages: () => false,
		getEnableSkillCommands: () => true,
		getQuietStartup: () => false,
		getShowTerminalProgress: () => false,
		getSteeringMode: () => "one-at-a-time",
		getFollowUpMode: () => "one-at-a-time",
	};
	const session = {
		settingsManager,
		autoCompactionEnabled: true,
		thinkingLevel: "medium",
		getAvailableThinkingLevels: () => ["off", "medium", "high"],
	} as unknown as AgentSession;
	return {
		session,
		runtimeHost: {} as unknown as AgentSessionRuntime,
		keybindings: {},
		notify: () => {},
		select: async () => undefined,
		listSessions: () => {},
		setInput: () => {},
		setActiveSubagent: () => {},
		activeSubagentId: () => undefined,
		dispose: async () => {},
		...overrides,
	};
}

describe("native builtin commands", () => {
	test("/settings opens a native settings selector instead of falling through to the agent", async () => {
		let selectedTitle = "";
		const handled = await handleNativeBuiltinCommand(
			"/settings",
			makeContext({
				select: async (title) => {
					selectedTitle = title;
					return undefined;
				},
			}),
		);

		expect(handled).toBe(true);
		expect(selectedTitle).toBe("Settings");
	});

	test("known but unsupported builtin commands do not fall through to the agent", async () => {
		let warning = "";
		const handled = await handleNativeBuiltinCommand(
			"/login",
			makeContext({
				notify: (message) => {
					warning = message;
				},
			}),
		);

		expect(handled).toBe(true);
		expect(warning).toContain("not supported");
	});
});
