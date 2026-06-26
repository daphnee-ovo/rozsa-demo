import { mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { RuntimeStateStore } from "../src/core/runtime-state.ts";
import { SidebarComponent } from "../src/modes/interactive/components/sidebar.ts";
import { initTheme } from "../src/modes/interactive/theme/theme.ts";

const tempRoot = join(tmpdir(), "runtime-state-sidebar-test");

describe("runtime state and sidebar", () => {
	let workspaceRoot: string;

	beforeAll(() => {
		initTheme("dark");
	});

	beforeEach(() => {
		workspaceRoot = join(tempRoot, `${Date.now()}-${Math.random().toString(36).slice(2)}`);
		mkdirSync(workspaceRoot, { recursive: true });
	});

	afterEach(() => {
		rmSync(tempRoot, { recursive: true, force: true });
	});

	it("records tool stats, permission state, token usage, subagents, and changed files", () => {
		const store = new RuntimeStateStore({
			workspaceRoot,
			cwd: workspaceRoot,
			permissionMode: "on-request",
			sessionName: "session",
		});

		store.recordToolRequested("write", "write");
		store.recordToolFinished("write", false);
		store.recordToolRequested("bash", "shell");
		store.recordToolFinished("bash", true, "permission denied by user");
		store.updatePermission({ mode: "free-permission", sessionApprovals: 1 });
		store.recordModelMessage({
			role: "assistant",
			content: [{ type: "text", text: "ok" }],
			api: "anthropic-messages",
			provider: "faux",
			model: "faux-1",
			usage: {
				input: 10,
				output: 5,
				cacheRead: 2,
				cacheWrite: 1,
				totalTokens: 18,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
			stopReason: "stop",
			timestamp: Date.now(),
		});
		store.recordSubagentStarted({ id: "subagent-1", name: "worker", taskSummary: "do work" });
		store.recordSubagentFinished("subagent-1", "completed");
		store.recordFileChanged(join(workspaceRoot, "src", "a.ts"), "modified", "agent");

		const snapshot = store.getSnapshot();
		expect(snapshot.permission.mode).toBe("free-permission");
		expect(snapshot.modelUsage.sessionTotalTokens).toBe(18);
		expect(snapshot.toolCallStats.find((tool) => tool.toolName === "bash")?.rejectedCount).toBe(1);
		expect(snapshot.activeSubagents[0]?.status).toBe("completed");
		expect(snapshot.changedFiles[0]).toMatchObject({ path: "src/a.ts", status: "modified", source: "agent" });
	});

	it("sidebar renders real RuntimeState and degrades in narrow terminals", () => {
		const store = new RuntimeStateStore({
			workspaceRoot,
			cwd: workspaceRoot,
			permissionMode: "free-permission",
		});
		store.updatePermission({ mode: "free-permission" });
		store.setCurrentModel("faux", "faux-1", "high");
		store.recordFileChanged(join(workspaceRoot, "a.ts"), "added", "agent");
		store.recordToolRequested("write", "write");

		const sidebar = new SidebarComponent(
			() => store.getSnapshot(),
			() => undefined,
		);
		const wide = sidebar.render(100).join("\n");
		expect(wide).toContain("free");
		expect(wide).toContain("faux-1");
		expect(wide).toContain("a.ts");
		expect(wide).toContain("write");

		const narrow = sidebar.render(60).join("\n");
		expect(narrow).toContain("free");
		expect(narrow).toContain("faux-1");
	});

	it("non-git refresh falls back to disabled git status", async () => {
		const store = new RuntimeStateStore({
			workspaceRoot,
			cwd: workspaceRoot,
			permissionMode: "on-request",
		});
		await store.refreshGitStatus();
		expect(store.getSnapshot().gitStatus.enabled).toBe(false);
	});
});
