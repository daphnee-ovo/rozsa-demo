/**
 * UX 优化合集功能测试
 * 验证：Per-tool 展开/折叠 (T1), slash 命令定义 (T3/T5/T6/T7),
 *       高对比度主题 (T9), 主题选择器预览 (T10), dry-run 模式 (T13),
 *       CLI 参数解析 (--dry-run, --verbose-extensions)
 */
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

// --- T1: Per-tool 展开/折叠 ---
describe("T1: Per-tool 工具输出展开/折叠", () => {
	let ToolExecutionComponent: any;
	let initTheme: any;

	beforeAll(async () => {
		const toolMod = await import("../packages/coding-agent/src/modes/interactive/components/tool-execution.ts");
		const themeMod = await import("../packages/coding-agent/src/modes/interactive/theme/theme.ts");
		ToolExecutionComponent = toolMod.ToolExecutionComponent;
		initTheme = themeMod.initTheme;
		initTheme("dark");
	});

	function createFakeTui() {
		return { requestRender: () => {} } as any;
	}

	it("默认状态跟随全局 expanded", () => {
		const component = new ToolExecutionComponent(
			"read",
			"tool-1",
			{ path: "test.txt" },
			{},
			undefined,
			createFakeTui(),
			process.cwd(),
		);
		// 默认 expanded = false
		expect(component.isExpanded()).toBe(false);
	});

	it("setExpanded 设置全局展开，清除 local override", () => {
		const component = new ToolExecutionComponent(
			"read",
			"tool-2",
			{ path: "test.txt" },
			{},
			undefined,
			createFakeTui(),
			process.cwd(),
		);
		component.setExpanded(true);
		expect(component.isExpanded()).toBe(true);

		// 设置 local override
		component.toggleLocal(); // 从 true -> false
		expect(component.isExpanded()).toBe(false);

		// setExpanded 应该清除 local override
		component.setExpanded(true);
		expect(component.isExpanded()).toBe(true);
	});

	it("toggleLocal 独立切换单个工具的展开状态", () => {
		const component = new ToolExecutionComponent(
			"read",
			"tool-3",
			{ path: "test.txt" },
			{},
			undefined,
			createFakeTui(),
			process.cwd(),
		);
		// 全局 collapsed
		component.setExpanded(false);
		expect(component.isExpanded()).toBe(false);

		// 单独展开此 tool
		component.toggleLocal();
		expect(component.isExpanded()).toBe(true);

		// 再次 toggle 回去
		component.toggleLocal();
		expect(component.isExpanded()).toBe(false);
	});

	it("toggleLocal 不受后续全局 setExpanded 影响 —— 除非 setExpanded 被显式调用", () => {
		const component = new ToolExecutionComponent(
			"read",
			"tool-4",
			{ path: "test.txt" },
			{},
			undefined,
			createFakeTui(),
			process.cwd(),
		);
		component.setExpanded(false);
		component.toggleLocal(); // local = true
		expect(component.isExpanded()).toBe(true);

		// setExpanded 会清除 local，恢复全局
		component.setExpanded(false);
		expect(component.isExpanded()).toBe(false);
	});
});

// --- T3, T5, T6, T7: Slash 命令定义 ---
describe("Slash 命令定义完整性", () => {
	let BUILTIN_SLASH_COMMANDS: any;

	beforeAll(async () => {
		const mod = await import("../packages/coding-agent/src/core/slash-commands.ts");
		BUILTIN_SLASH_COMMANDS = mod.BUILTIN_SLASH_COMMANDS;
	});

	it("T3: /permissions 命令已注册", () => {
		const cmd = BUILTIN_SLASH_COMMANDS.find((c: any) => c.name === "permissions");
		expect(cmd).toBeDefined();
		expect(cmd.description).toBeTruthy();
	});

	it("T5: /help 命令有 usage 和 examples", () => {
		const cmd = BUILTIN_SLASH_COMMANDS.find((c: any) => c.name === "help");
		expect(cmd).toBeDefined();
		expect(cmd.usage).toContain("/help");
		expect(cmd.examples).toBeDefined();
		expect(cmd.examples.length).toBeGreaterThan(0);
	});

	it("T5: /compact 命令有 usage 和 examples", () => {
		const cmd = BUILTIN_SLASH_COMMANDS.find((c: any) => c.name === "compact");
		expect(cmd).toBeDefined();
		expect(cmd.usage).toContain("/compact");
		expect(cmd.examples).toBeDefined();
		expect(cmd.examples.length).toBeGreaterThan(0);
	});

	it("T6: /gc 命令已注册且有 usage", () => {
		const cmd = BUILTIN_SLASH_COMMANDS.find((c: any) => c.name === "gc");
		expect(cmd).toBeDefined();
		expect(cmd.usage).toContain("/gc");
		expect(cmd.examples).toBeDefined();
		expect(cmd.examples.some((e: string) => e.includes("/gc"))).toBe(true);
	});

	it("T7: /search 命令已注册且有 usage", () => {
		const cmd = BUILTIN_SLASH_COMMANDS.find((c: any) => c.name === "search");
		expect(cmd).toBeDefined();
		expect(cmd.usage).toContain("/search");
		expect(cmd.examples).toBeDefined();
		expect(cmd.examples.length).toBeGreaterThan(0);
	});

	it("所有命令都有非空 description", () => {
		for (const cmd of BUILTIN_SLASH_COMMANDS) {
			expect(cmd.description, `command /${cmd.name} should have description`).toBeTruthy();
		}
	});
});

// --- T9: 高对比度主题 ---
describe("T9: 高对比度主题", () => {
	it("high-contrast.json 存在且包含必要字段", () => {
		const themePath = join(
			process.cwd(),
			"packages/coding-agent/src/modes/interactive/theme/high-contrast.json",
		);
		expect(existsSync(themePath)).toBe(true);
		const content = JSON.parse(readFileSync(themePath, "utf-8"));
		expect(content.name).toBe("high-contrast");
		expect(content.vars).toBeDefined();
		expect(content.colors).toBeDefined();
	});

	it("high-contrast 主题可被主题系统识别", async () => {
		const { getAvailableThemes, initTheme } = await import(
			"../packages/coding-agent/src/modes/interactive/theme/theme.ts"
		);
		initTheme("dark"); // 先确保初始化
		const themes = getAvailableThemes();
		expect(themes).toContain("high-contrast");
	});

	it("high-contrast 主题有高对比度色彩值", () => {
		const themePath = join(
			process.cwd(),
			"packages/coding-agent/src/modes/interactive/theme/high-contrast.json",
		);
		const content = JSON.parse(readFileSync(themePath, "utf-8"));
		// 验证使用纯白/黑 + 高饱和度色
		expect(content.vars.white).toBe("#ffffff");
		expect(content.vars.black).toBe("#000000");
		// accent 应该是高饱和度颜色
		expect(content.colors.accent).toBeTruthy();
	});
});

// --- T10: 主题选择器预览 ---
describe("T10: 主题选择器预览", () => {
	it("ThemeSelectorComponent 接受 onPreview 回调", async () => {
		const { ThemeSelectorComponent } = await import(
			"../packages/coding-agent/src/modes/interactive/components/theme-selector.ts"
		);
		const { initTheme } = await import("../packages/coding-agent/src/modes/interactive/theme/theme.ts");
		initTheme("dark");

		const previewCalls: string[] = [];
		const component = new ThemeSelectorComponent(
			"dark",
			() => {}, // onSelect
			() => {}, // onCancel
			(themeName: string) => previewCalls.push(themeName), // onPreview
		);
		expect(component).toBeDefined();
		// ThemeSelectorComponent 应该内部存储 onPreview
		expect(component.getSelectList()).toBeDefined();
	});
});

// --- T13: Dry-run 模式 ---
describe("T13: Dry-run 模式", () => {
	describe("bash 工具 dry-run", () => {
		let createBashToolDefinition: any;

		beforeAll(async () => {
			const mod = await import("../packages/coding-agent/src/core/tools/bash.ts");
			createBashToolDefinition = mod.createBashToolDefinition;
		});

		it("dry-run 模式下 bash 不实际执行命令", async () => {
			const tool = createBashToolDefinition(process.cwd(), { dryRun: true });
			const result = await tool.execute("call-1", { command: "echo hello" });
			expect(result.content[0].text).toContain("[DRY-RUN]");
			expect(result.content[0].text).toContain("echo hello");
		});

		it("dry-run 模式下包含完整命令预览", async () => {
			const tool = createBashToolDefinition(process.cwd(), { dryRun: true });
			const result = await tool.execute("call-2", { command: "rm -rf /tmp/test" });
			expect(result.content[0].text).toContain("[DRY-RUN]");
			expect(result.content[0].text).toContain("rm -rf /tmp/test");
		});

		it("dry-run 模式下含 commandPrefix 时也显示完整命令", async () => {
			const tool = createBashToolDefinition(process.cwd(), {
				dryRun: true,
				commandPrefix: "set -e",
			});
			const result = await tool.execute("call-3", { command: "ls" });
			expect(result.content[0].text).toContain("[DRY-RUN]");
			expect(result.content[0].text).toContain("set -e");
			expect(result.content[0].text).toContain("ls");
		});
	});

	describe("write 工具 dry-run", () => {
		let createWriteToolDefinition: any;

		beforeAll(async () => {
			const mod = await import("../packages/coding-agent/src/core/tools/write.ts");
			createWriteToolDefinition = mod.createWriteToolDefinition;
		});

		it("dry-run 模式下 write 不实际写入文件", async () => {
			const tmpFile = join(process.cwd(), "tmp", "dry-run-test-write.txt");
			const tool = createWriteToolDefinition(process.cwd(), { dryRun: true });
			const result = await tool.execute("call-4", {
				path: tmpFile,
				content: "test content",
			});
			expect(result.content[0].text).toContain("[DRY-RUN]");
			expect(result.content[0].text).toContain("12 bytes"); // "test content" = 12 bytes
			// 文件不应该被实际创建
			expect(existsSync(tmpFile)).toBe(false);
		});
	});

	describe("edit 工具 dry-run", () => {
		let createEditToolDefinition: any;
		const tmpDir = join(process.cwd(), "tmp", "dry-run-edit-test");

		beforeAll(async () => {
			const mod = await import("../packages/coding-agent/src/core/tools/edit.ts");
			createEditToolDefinition = mod.createEditToolDefinition;
		});

		beforeEach(() => {
			mkdirSync(tmpDir, { recursive: true });
		});

		afterEach(() => {
			if (existsSync(tmpDir)) {
				rmSync(tmpDir, { recursive: true, force: true });
			}
		});

		it("dry-run 模式下 edit 生成 diff 预览但不修改文件", async () => {
			const filePath = join(tmpDir, "test.ts");
			writeFileSync(filePath, "const a = 1;\n");

			const tool = createEditToolDefinition(tmpDir, { dryRun: true });
			const result = await tool.execute("call-5", {
				path: "test.ts",
				edits: [{ oldText: "const a = 1;", newText: "const a = 2;" }],
			});
			expect(result.content[0].text).toContain("[DRY-RUN]");
			expect(result.content[0].text).toContain("test.ts");

			// 文件内容不应被修改
			const content = readFileSync(filePath, "utf-8");
			expect(content).toBe("const a = 1;\n");
		});
	});
});

// --- CLI 参数解析 ---
describe("CLI 参数解析", () => {
	let parseArgs: any;

	beforeAll(async () => {
		const mod = await import("../packages/coding-agent/src/cli/args.ts");
		parseArgs = mod.parseArgs;
	});

	it("--dry-run flag 正确解析", () => {
		const result = parseArgs(["--dry-run"]);
		expect(result.dryRun).toBe(true);
	});

	it("--dry-run 与其他 flag 组合", () => {
		const result = parseArgs(["--dry-run", "-p", "create a file"]);
		expect(result.dryRun).toBe(true);
		expect(result.print).toBe(true);
		expect(result.messages).toContain("create a file");
	});

	it("--verbose-extensions flag 正确解析", () => {
		const result = parseArgs(["--verbose-extensions"]);
		expect(result.verboseExtensions).toBe(true);
	});

	it("未指定 --dry-run 时 dryRun 为 undefined", () => {
		const result = parseArgs(["hello"]);
		expect(result.dryRun).toBeUndefined();
	});
});
