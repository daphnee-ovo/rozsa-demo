/**
 * LSP Hook — LSP 生命周期管理与自动诊断集成
 *
 * 架构树:
 * packages/coding-agent/src/core/lsp/
 * ├── lsp-hook.ts          ← 本文件：钩入 agent 会话生命周期，自动收集诊断
 * └── lsp-core.ts          ← LSP 服务器管理核心（启动/通信/关闭）
 *
 * 职责:
 * - 追踪 agent 当前 turn 中被修改的文件
 * - 根据配置模式（agent_end / edit_write / disabled）收集并投递诊断
 * - 管理 LSP 服务器的空闲超时与懒重启
 * - 提供格式化的诊断消息用于注入对话
 */

import { readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { LSPManager } from "./lsp-core.ts";

// ─── 类型定义 ────────────────────────────────────────────────────────────────

/** 单条诊断信息 */
export interface LSPDiagnostic {
	line: number;
	column: number;
	severity: "error" | "warning" | "info" | "hint";
	message: string;
	source?: string;
}

/** 单文件的诊断结果 */
export interface LSPFileDiagnostics {
	path: string;
	diagnostics: LSPDiagnostic[];
}

/** 诊断收集的完整结果 */
export interface LSPDiagnosticsResult {
	files: LSPFileDiagnostics[];
	totalErrors: number;
	totalWarnings: number;
}

/** Hook 的配置模式 */
export type LSPHookMode = "agent_end" | "edit_write" | "disabled";

/** 构造选项 */
export interface LSPHookOptions {
	cwd: string;
	hookMode?: LSPHookMode;
}

// ─── 常量 ─────────────────────────────────────────────────────────────────────

/** 空闲超时时间（2 分钟） */
const IDLE_TIMEOUT_MS = 2 * 60 * 1000;

/** 诊断等待时间（默认，毫秒） */
const DEFAULT_DIAGNOSTICS_WAIT_MS = 3_000;

/** 慢语言诊断等待时间（Kotlin/Rust/Swift 等，毫秒） */
const SLOW_LANG_DIAGNOSTICS_WAIT_MS = 20_000;

/** 编辑后去抖延迟（毫秒） */
const DEBOUNCE_MS = 500;

/** 需要更长等待时间的语言扩展名 */
const SLOW_LANGUAGE_EXTENSIONS = new Set([".kt", ".kts", ".rs", ".swift", ".scala", ".java"]);

/** 写/编辑相关的工具名 */
const WRITE_TOOL_NAMES = new Set(["write", "edit", "Write", "Edit"]);

// ─── 主类 ─────────────────────────────────────────────────────────────────────

/**
 * LSPHook — 管理 LSP 生命周期并自动收集诊断
 *
 * 三种投递模式:
 * - agent_end: agent 结束后统一收集所有修改文件的诊断
 * - edit_write: 每次 write/edit 后立即附加诊断
 * - disabled: 不自动收集（LSP 工具仍可手动使用）
 */
export class LSPHook {
	private cwd: string;
	private hookMode: LSPHookMode;
	private lspManager: LSPManager | null = null;
	private touchedFiles: Set<string> = new Set();
	private idleTimer: ReturnType<typeof setTimeout> | null = null;
	private active = false;
	private starting = false;

	constructor(options: LSPHookOptions) {
		this.cwd = options.cwd;
		this.hookMode = options.hookMode ?? "agent_end";
	}

	// ─── 生命周期 ──────────────────────────────────────────────────────────

	/**
	 * 启动 LSP Hook，预热 LSP 服务器
	 */
	async start(): Promise<void> {
		if (this.active || this.starting) return;
		this.starting = true;

		try {
			this.lspManager = LSPManager.getOrCreateManager(this.cwd);
			this.active = true;
			this.resetIdleTimer();
		} catch {
			// LSP 服务器不可用时静默降级，不阻塞主流程
			this.lspManager = null;
			this.active = false;
		} finally {
			this.starting = false;
		}
	}

	/**
	 * 关闭 LSP Hook 及底层服务器
	 */
	async shutdown(): Promise<void> {
		this.clearIdleTimer();

		if (this.lspManager) {
			try {
				await this.lspManager.shutdownAll();
			} catch {
				// 关闭失败时忽略
			}
			this.lspManager = null;
		}

		this.active = false;
		this.touchedFiles.clear();
	}

	// ─── 事件处理 ──────────────────────────────────────────────────────────

	/**
	 * Agent 开始新 turn 时调用
	 * 清空已追踪的文件列表
	 */
	onAgentStart(): void {
		this.touchedFiles.clear();
		this.resetIdleTimer();
	}

	/**
	 * Agent 结束 turn 时调用
	 * 在 agent_end 模式下收集所有修改文件的诊断
	 *
	 * @returns 诊断结果，如果没有错误/警告或模式不匹配则返回 null
	 */
	async onAgentEnd(): Promise<LSPDiagnosticsResult | null> {
		this.resetIdleTimer();

		if (this.hookMode !== "agent_end") return null;
		if (this.touchedFiles.size === 0) return null;

		return this.collectDiagnosticsForFiles([...this.touchedFiles]);
	}

	/**
	 * 工具被调用时记录修改的文件
	 *
	 * @param toolName - 工具名称
	 * @param args - 工具参数
	 */
	onToolCall(toolName: string, args: Record<string, unknown>): void {
		if (!WRITE_TOOL_NAMES.has(toolName)) return;

		// 从参数中提取文件路径
		const filePath = this.extractFilePath(args);
		if (filePath) {
			const resolved = resolve(this.cwd, filePath);
			this.touchedFiles.add(resolved);
		}

		this.resetIdleTimer();
	}

	/**
	 * 工具执行完成后调用
	 * 在 edit_write 模式下返回诊断字符串附加到工具结果
	 *
	 * @param toolName - 工具名称
	 * @param args - 工具参数
	 * @returns 诊断字符串（附加到工具结果），或 null
	 */
	async onToolResult(toolName: string, args: Record<string, unknown>): Promise<string | null> {
		this.resetIdleTimer();

		if (this.hookMode !== "edit_write") return null;
		if (!WRITE_TOOL_NAMES.has(toolName)) return null;

		const filePath = this.extractFilePath(args);
		if (!filePath) return null;

		const resolved = resolve(this.cwd, filePath);
		const result = await this.collectDiagnosticsForFiles([resolved]);

		if (!result || (result.totalErrors === 0 && result.totalWarnings === 0)) {
			return null;
		}

		return this.formatDiagnosticsMessage(result);
	}

	// ─── 配置 ──────────────────────────────────────────────────────────────

	/**
	 * 获取当前 hook 模式
	 */
	getHookMode(): LSPHookMode {
		return this.hookMode;
	}

	/**
	 * 设置 hook 模式
	 */
	setHookMode(mode: LSPHookMode): void {
		this.hookMode = mode;
	}

	// ─── 状态查询 ──────────────────────────────────────────────────────────

	/**
	 * LSP Hook 是否处于活跃状态
	 */
	isActive(): boolean {
		return this.active;
	}

	/**
	 * 获取当前状态描述
	 */
	getStatus(): string {
		if (!this.active) {
			return "inactive";
		}
		const fileCount = this.touchedFiles.size;
		return `active | mode=${this.hookMode} | tracked_files=${fileCount}`;
	}

	// ─── 私有方法 ──────────────────────────────────────────────────────────

	/**
	 * 从工具参数中提取文件路径
	 * 支持 file_path / filePath / path 等常见参数名
	 */
	private extractFilePath(args: Record<string, unknown>): string | null {
		const candidates = ["file_path", "filePath", "path", "filename"];
		for (const key of candidates) {
			const value = args[key];
			if (typeof value === "string" && value.length > 0) {
				return value;
			}
		}
		return null;
	}

	/**
	 * 收集指定文件列表的诊断信息
	 */
	private async collectDiagnosticsForFiles(files: string[]): Promise<LSPDiagnosticsResult | null> {
		// 确保 LSP 服务器可用（懒启动）
		if (!this.active) {
			await this.ensureStarted();
		}

		if (!this.lspManager) {
			return null;
		}

		// 去抖等待：让 LSP 服务器有时间处理文件变更
		await this.delay(DEBOUNCE_MS);

		const result: LSPDiagnosticsResult = {
			files: [],
			totalErrors: 0,
			totalWarnings: 0,
		};

		for (const filePath of files) {
			const fileDiagnostics = await this.collectFileDignostics(filePath);
			if (fileDiagnostics && fileDiagnostics.diagnostics.length > 0) {
				result.files.push(fileDiagnostics);

				// 统计错误和警告数量
				for (const diag of fileDiagnostics.diagnostics) {
					if (diag.severity === "error") {
						result.totalErrors++;
					} else if (diag.severity === "warning") {
						result.totalWarnings++;
					}
				}
			}
		}

		// 没有诊断则返回 null
		if (result.files.length === 0) {
			return null;
		}

		return result;
	}

	/**
	 * 收集单个文件的诊断
	 */
	private async collectFileDignostics(filePath: string): Promise<LSPFileDiagnostics | null> {
		if (!this.lspManager) return null;

		try {
			// 读取文件内容并通知 LSP 服务器
			const content = readFileSync(filePath, "utf-8");
			await this.lspManager.touchFile(filePath, content);

			// 计算等待时间
			const waitMs = this.getDiagnosticsWaitTime(filePath);

			// 等待诊断结果（touchFile 后 LSP 服务器会异步推送）
			await this.delay(waitMs);

			// 获取诊断
			const rawDiagnostics = await this.lspManager.getDiagnostics(filePath);

			if (!rawDiagnostics || rawDiagnostics.length === 0) {
				return null;
			}

			// 转换诊断格式
			const diagnostics: LSPDiagnostic[] = rawDiagnostics.map(
				(d: {
					range: { start: { line: number; character: number } };
					severity?: number;
					message: string;
					source?: string | null;
				}) => ({
					line: d.range.start.line + 1,
					column: d.range.start.character + 1,
					severity: this.mapSeverity(d.severity),
					message: d.message,
					source: d.source ?? undefined,
				}),
			);

			return {
				path: filePath,
				diagnostics,
			};
		} catch {
			// 文件读取失败或 LSP 通信异常，静默跳过
			return null;
		}
	}

	/**
	 * 根据文件扩展名决定诊断等待时间
	 * 慢语言（Kotlin/Rust/Swift）使用更长的超时
	 */
	private getDiagnosticsWaitTime(filePath: string): number {
		const ext = this.getFileExtension(filePath);
		if (SLOW_LANGUAGE_EXTENSIONS.has(ext)) {
			return SLOW_LANG_DIAGNOSTICS_WAIT_MS;
		}
		return DEFAULT_DIAGNOSTICS_WAIT_MS;
	}

	/**
	 * 获取文件扩展名（含点号）
	 */
	private getFileExtension(filePath: string): string {
		const lastDot = filePath.lastIndexOf(".");
		if (lastDot === -1) return "";
		return filePath.slice(lastDot).toLowerCase();
	}

	/**
	 * 将 LSP severity 数值映射为字符串
	 * LSP 协议: 1=Error, 2=Warning, 3=Information, 4=Hint
	 */
	private mapSeverity(severity: number | undefined): "error" | "warning" | "info" | "hint" {
		switch (severity) {
			case 1:
				return "error";
			case 2:
				return "warning";
			case 3:
				return "info";
			case 4:
				return "hint";
			default:
				return "warning";
		}
	}

	/**
	 * 将诊断结果格式化为可注入对话的文本消息
	 */
	private formatDiagnosticsMessage(result: LSPDiagnosticsResult): string {
		const lines: string[] = [];

		// 标题行
		const parts: string[] = [];
		if (result.totalErrors > 0) {
			parts.push(`${result.totalErrors} error${result.totalErrors > 1 ? "s" : ""}`);
		}
		if (result.totalWarnings > 0) {
			parts.push(`${result.totalWarnings} warning${result.totalWarnings > 1 ? "s" : ""}`);
		}

		const fileCount = result.files.length;
		lines.push(`[LSP Diagnostics] Found ${parts.join(" and ")} in ${fileCount} file${fileCount > 1 ? "s" : ""}:`);
		lines.push("");

		// 逐文件列出诊断
		for (const file of result.files) {
			const relPath = relative(this.cwd, file.path);
			lines.push(`${relPath}:`);

			for (const diag of file.diagnostics) {
				const sourceTag = diag.source ? ` (${diag.source})` : "";
				lines.push(`  L${diag.line}:${diag.column} ${diag.severity}: ${diag.message}${sourceTag}`);
			}

			lines.push("");
		}

		return lines.join("\n").trimEnd();
	}

	// ─── 空闲超时管理 ──────────────────────────────────────────────────────

	/**
	 * 重置空闲计时器
	 * 任何活动（工具调用、agent 事件）都会刷新计时器
	 */
	private resetIdleTimer(): void {
		this.clearIdleTimer();

		if (!this.active) return;

		this.idleTimer = setTimeout(() => {
			this.handleIdleTimeout();
		}, IDLE_TIMEOUT_MS);
	}

	/**
	 * 清除空闲计时器
	 */
	private clearIdleTimer(): void {
		if (this.idleTimer !== null) {
			clearTimeout(this.idleTimer);
			this.idleTimer = null;
		}
	}

	/**
	 * 空闲超时处理：关闭 LSP 服务器以释放资源
	 * 下次需要时会懒重启
	 */
	private handleIdleTimeout(): void {
		if (this.lspManager) {
			// 异步关闭，不阻塞
			this.lspManager.shutdownAll().catch(() => {});
			this.lspManager = null;
		}
		this.active = false;
	}

	/**
	 * 确保 LSP 服务器已启动（懒重启）
	 */
	private async ensureStarted(): Promise<void> {
		if (this.active && this.lspManager) return;
		await this.start();
	}

	/**
	 * 延迟工具函数
	 */
	private delay(ms: number): Promise<void> {
		return new Promise((resolve) => setTimeout(resolve, ms));
	}
}

// ─── 工具函数 ─────────────────────────────────────────────────────────────────

/**
 * 从 LSPDiagnosticsResult 生成格式化的 follow-up 消息
 * 供外部调用者直接使用
 */
export function formatLSPDiagnostics(result: LSPDiagnosticsResult, cwd: string): string {
	const lines: string[] = [];

	const parts: string[] = [];
	if (result.totalErrors > 0) {
		parts.push(`${result.totalErrors} error${result.totalErrors > 1 ? "s" : ""}`);
	}
	if (result.totalWarnings > 0) {
		parts.push(`${result.totalWarnings} warning${result.totalWarnings > 1 ? "s" : ""}`);
	}

	const fileCount = result.files.length;
	lines.push(`[LSP Diagnostics] Found ${parts.join(" and ")} in ${fileCount} file${fileCount > 1 ? "s" : ""}:`);
	lines.push("");

	for (const file of result.files) {
		const relPath = relative(cwd, file.path);
		lines.push(`${relPath}:`);

		for (const diag of file.diagnostics) {
			const sourceTag = diag.source ? ` (${diag.source})` : "";
			lines.push(`  L${diag.line}:${diag.column} ${diag.severity}: ${diag.message}${sourceTag}`);
		}

		lines.push("");
	}

	return lines.join("\n").trimEnd();
}

/**
 * 判断诊断结果是否包含需要修复的错误
 * 用于决定是否需要触发额外的 agent turn
 */
export function hasActionableErrors(result: LSPDiagnosticsResult | null): boolean {
	if (!result) return false;
	return result.totalErrors > 0;
}
