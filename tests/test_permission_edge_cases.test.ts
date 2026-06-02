/**
 * 权限系统边界情况测试
 * 验证复合命令中的 sudo、特殊绕过手法、非标准格式
 */
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	PermissionAuditLogger,
	PermissionManager,
	splitShellSegments,
} from "../packages/coding-agent/src/core/permissions.ts";

const tempRoot = join(process.cwd(), "tmp", "test-permission-edge");

function makeWorkspace(name: string): string {
	const dir = join(tempRoot, `${name}-${Date.now()}-${Math.random().toString(36).slice(2)}`);
	mkdirSync(dir, { recursive: true });
	return dir;
}

describe("权限系统边界情况", () => {
	let workspaceRoot: string;

	beforeEach(() => {
		workspaceRoot = makeWorkspace("edge");
	});

	afterEach(() => {
		if (existsSync(tempRoot)) {
			rmSync(tempRoot, { recursive: true, force: true });
		}
	});

	describe("复合命令中的 sudo 检测", () => {
		it("ls && sudo rm 应被 segment 拆分后拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "ls && sudo rm -rf /tmp/data" },
				command: "ls && sudo rm -rf /tmp/data",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
			expect(decision.source).toBe("blacklist");
		});

		it("echo ok | sudo tee /etc/file 应被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "echo ok | sudo tee /etc/shadow" },
				command: "echo ok | sudo tee /etc/shadow",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
			expect(decision.source).toBe("blacklist");
		});
	});

	describe("git 危险命令检测", () => {
		it("git reset --hard 被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "git reset --hard HEAD~3" },
				command: "git reset --hard HEAD~3",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});

		it("git clean -fd 被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "git clean -fd" },
				command: "git clean -fd",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});

		it("git push -f 被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "git push -f origin main" },
				command: "git push -f origin main",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});

		it("git push --force 被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "git push --force origin main" },
				command: "git push --force origin main",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});
	});

	describe("dd 和 mkfs 等系统级危险命令", () => {
		it("dd 命令被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "dd if=/dev/zero of=/dev/sda bs=1M" },
				command: "dd if=/dev/zero of=/dev/sda bs=1M",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});

		it("mkfs 命令被拦截", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "mkfs.ext4 /dev/sda1" },
				command: "mkfs.ext4 /dev/sda1",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});
	});

	describe("free-permission 模式下硬核黑名单仍然生效", () => {
		it("free-permission 模式拒绝 sudo", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "free-permission",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "sudo apt install pkg" },
				command: "sudo apt install pkg",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});

		it("free-permission 模式拒绝 rm -rf /", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "free-permission",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "bash",
				args: { command: "rm -rf /" },
				command: "rm -rf /",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
		});
	});

	describe("workspace 外路径检测", () => {
		it("write 到 /etc 路径被拒绝", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "write",
				args: { path: "/etc/hosts", content: "evil" },
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("reject");
			expect(decision.reason).toContain("outside workspace");
		});

		it("workspace 内路径正常通过", async () => {
			const prompt = {
				request: vi.fn().mockResolvedValue({ choice: "approve_once" }),
			};
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			const decision = await manager.check({
				toolName: "write",
				args: { path: join(workspaceRoot, "test.txt"), content: "hello" },
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});
			expect(decision.decision).toBe("approve");
		});
	});

	describe("PermissionManager history 记录", () => {
		it("每次 check 都会记录到 permissionHistory", async () => {
			const prompt = {
				request: vi.fn().mockResolvedValue({ choice: "approve_once" }),
			};
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: {
					whitelist: { toolNames: ["read"] },
					blacklist: {},
				},
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			await manager.check({
				toolName: "read",
				args: { path: "a.txt" },
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});

			await manager.check({
				toolName: "write",
				args: { path: join(workspaceRoot, "b.txt"), content: "x" },
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});

			const history = manager.getPermissionHistory();
			expect(history.length).toBe(2);
			expect(history[0].toolName).toBe("read");
			expect(history[0].decision).toBe("approve");
			expect(history[0].source).toBe("whitelist");
			expect(history[1].toolName).toBe("write");
			expect(history[1].decision).toBe("approve");
			expect(history[1].source).toBe("user");
		});

		it("reject 也被记录到 history", async () => {
			const prompt = { request: vi.fn() };
			const manager = new PermissionManager({
				mode: "on-request",
				workspaceRoot,
				settings: { whitelist: {}, blacklist: {} },
				userPrompt: prompt,
				auditLogger: new PermissionAuditLogger(workspaceRoot, "session-1"),
			});

			await manager.check({
				toolName: "bash",
				args: { command: "rm -rf /" },
				command: "rm -rf /",
				workspaceRoot,
				cwd: workspaceRoot,
				sessionId: "session-1",
			});

			const history = manager.getPermissionHistory();
			expect(history.length).toBe(1);
			expect(history[0].decision).toBe("reject");
			expect(history[0].source).toBe("blacklist");
		});
	});

	describe("splitShellSegments 极端输入", () => {
		it("超长命令不崩溃", () => {
			const longCmd = "echo " + "a".repeat(10000) + " | grep a";
			const segments = splitShellSegments(longCmd);
			expect(segments.length).toBe(2);
		});

		it("只有运算符", () => {
			const segments = splitShellSegments("&&");
			expect(segments).toEqual([]);
		});

		it("连续多个分隔符", () => {
			const segments = splitShellSegments("a || || b");
			// 中间的空段会被 filter 掉
			expect(segments.length).toBeGreaterThanOrEqual(2);
			expect(segments[0]).toBe("a");
		});

		it("混合引号嵌套", () => {
			const segments = splitShellSegments(`echo "it's ok" | wc`);
			expect(segments).toEqual([`echo "it's ok"`, "wc"]);
		});
	});
});
