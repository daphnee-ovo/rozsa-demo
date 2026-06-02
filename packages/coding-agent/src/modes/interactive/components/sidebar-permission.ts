/**
 * Sidebar 内嵌权限审批组件。
 * 在 sidebar 底部显示权限请求，用户通过快捷键选择。
 * trust 选项展开子菜单让用户选择匹配粒度。
 */

import {
	type Component,
	getKeybindings,
	truncateToWidth,
	visibleWidth,
	wrapTextWithAnsi,
} from "@earendil-works/pi-tui";
import {
	generateTrustLevels,
	type PermissionPromptContext,
	type PermissionRequest,
} from "../../../core/permissions.ts";
import { theme } from "../theme/theme.ts";

export type SidebarPermissionChoice = "approve_once" | "approve_session" | "reject" | "reject_alternative";

export interface SidebarPermissionResult {
	choice: SidebarPermissionChoice;
	trustKey?: string;
}

export interface SidebarPermissionPromptData {
	request: PermissionRequest;
	context: PermissionPromptContext;
}

type UIState = "main" | "trust";

const MAIN_OPTIONS: { label: string; shortcut: string; choice: SidebarPermissionChoice }[] = [
	{ label: "approve", shortcut: "y", choice: "approve_once" },
	{ label: "trust", shortcut: "t", choice: "approve_session" },
	{ label: "reject", shortcut: "n", choice: "reject" },
	{ label: "alt", shortcut: "a", choice: "reject_alternative" },
];

export class SidebarPermissionComponent implements Component {
	private data: SidebarPermissionPromptData | null = null;
	private selectedIndex = 0;
	private onResolve: ((result: SidebarPermissionResult) => void) | null = null;
	private pendingQueue: {
		data: SidebarPermissionPromptData;
		resolve: (result: SidebarPermissionResult) => void;
	}[] = [];
	private timeoutHandle: ReturnType<typeof setTimeout> | null = null;
	private promptStartTime: number | null = null;
	private static readonly TIMEOUT_MS = 5 * 60 * 1000;
	private static readonly WARNING_MS = 4 * 60 * 1000;

	// trust 子菜单状态
	private uiState: UIState = "main";
	private trustLevels: { label: string; key: string }[] = [];

	invalidate(): void {}

	prompt(data: SidebarPermissionPromptData): Promise<SidebarPermissionResult> {
		if (this.data !== null) {
			return new Promise((resolve) => {
				this.pendingQueue.push({ data, resolve });
			});
		}
		this.data = data;
		this.selectedIndex = 0;
		this.uiState = "main";
		return new Promise((resolve) => {
			this.onResolve = resolve;
			this.startTimeout();
		});
	}

	private startTimeout(): void {
		this.clearTimeout();
		this.promptStartTime = Date.now();
		this.timeoutHandle = setTimeout(() => {
			this.resolve({ choice: "reject" });
		}, SidebarPermissionComponent.TIMEOUT_MS);
	}

	private clearTimeout(): void {
		if (this.timeoutHandle) {
			clearTimeout(this.timeoutHandle);
			this.timeoutHandle = null;
		}
	}

	isActive(): boolean {
		return this.data !== null;
	}

	handleInput(keyData: string): boolean {
		if (!this.data) return false;

		const kb = getKeybindings();

		if (this.uiState === "trust") {
			return this.handleTrustInput(keyData, kb);
		}

		return this.handleMainInput(keyData, kb);
	}

	private handleMainInput(keyData: string, kb: ReturnType<typeof getKeybindings>): boolean {
		// 快捷键
		if (keyData === "y" || keyData === "Y") {
			this.resolve({ choice: "approve_once" });
			return true;
		}
		if (keyData === "t" || keyData === "T") {
			this.enterTrustMenu();
			return true;
		}
		if (keyData === "n" || keyData === "N") {
			this.resolve({ choice: "reject" });
			return true;
		}
		if (keyData === "a" || keyData === "A") {
			this.resolve({ choice: "reject_alternative" });
			return true;
		}

		// 上下导航
		if (kb.matches(keyData, "tui.select.up") || keyData === "k") {
			this.selectedIndex = Math.max(0, this.selectedIndex - 1);
			return true;
		}
		if (kb.matches(keyData, "tui.select.down") || keyData === "j") {
			this.selectedIndex = Math.min(MAIN_OPTIONS.length - 1, this.selectedIndex + 1);
			return true;
		}

		// Enter
		if (kb.matches(keyData, "tui.select.confirm") || keyData === "\r" || keyData === "\n") {
			const opt = MAIN_OPTIONS[this.selectedIndex];
			if (opt.choice === "approve_session") {
				this.enterTrustMenu();
			} else {
				this.resolve({ choice: opt.choice });
			}
			return true;
		}

		return false;
	}

	private handleTrustInput(keyData: string, kb: ReturnType<typeof getKeybindings>): boolean {
		// Esc 返回主菜单
		if (kb.matches(keyData, "tui.select.cancel") || keyData === "\x1b") {
			this.uiState = "main";
			this.selectedIndex = 1; // 回到 trust 选项
			return true;
		}

		// 数字快捷键 1-9
		const num = Number.parseInt(keyData, 10);
		if (num >= 1 && num <= this.trustLevels.length) {
			this.resolve({ choice: "approve_session", trustKey: this.trustLevels[num - 1].key });
			return true;
		}

		// 上下导航
		if (kb.matches(keyData, "tui.select.up") || keyData === "k") {
			this.selectedIndex = Math.max(0, this.selectedIndex - 1);
			return true;
		}
		if (kb.matches(keyData, "tui.select.down") || keyData === "j") {
			this.selectedIndex = Math.min(this.trustLevels.length - 1, this.selectedIndex + 1);
			return true;
		}

		// Enter
		if (kb.matches(keyData, "tui.select.confirm") || keyData === "\r" || keyData === "\n") {
			const level = this.trustLevels[this.selectedIndex];
			if (level) {
				this.resolve({ choice: "approve_session", trustKey: level.key });
			}
			return true;
		}

		return false;
	}

	private enterTrustMenu(): void {
		if (!this.data) return;
		this.trustLevels = generateTrustLevels(this.data.request);
		// 只有一层时直接信任，无需子菜单
		if (this.trustLevels.length <= 1) {
			this.resolve({ choice: "approve_session", trustKey: this.trustLevels[0]?.key });
			return;
		}
		this.uiState = "trust";
		this.selectedIndex = 0;
	}

	private resolve(result: SidebarPermissionResult): void {
		this.clearTimeout();
		this.promptStartTime = null;
		const cb = this.onResolve;
		this.onResolve = null;
		this.data = null;
		this.uiState = "main";

		const next = this.pendingQueue.shift();
		if (next) {
			this.data = next.data;
			this.selectedIndex = 0;
			this.uiState = "main";
			this.onResolve = next.resolve;
			this.startTimeout();
		}

		if (cb) {
			cb(result);
		}
	}

	render(width: number): string[] {
		if (!this.data) return [];

		const b = theme.fg("borderMuted", "│");
		const inner = width - 2;
		const lines: string[] = [];

		lines.push(b);
		const source = this.data.request.source;
		const header = source
			? `${theme.fg("warning", theme.bold("PERM"))} ${theme.fg("dim", source)}`
			: theme.fg("warning", theme.bold("PERMISSION"));
		lines.push(`${b} ${truncateToWidth(header, inner, "...")}`);

		// 工具名和命令（长命令自动换行，最多 3 行）
		const toolName = this.data.request.toolName;
		const command = this.data.request.command;
		lines.push(`${b} ${truncateToWidth(toolName, inner, "...")}`);
		if (command) {
			const shortCmd = command.split("\n")[0].trim();
			const cmdStyled = theme.fg("dim", shortCmd);
			const wrapped = wrapTextWithAnsi(cmdStyled, inner);
			const maxCmdLines = 3;
			for (let i = 0; i < Math.min(wrapped.length, maxCmdLines); i++) {
				const suffix = i === maxCmdLines - 1 && wrapped.length > maxCmdLines ? "…" : "";
				lines.push(`${b} ${wrapped[i]}${suffix}`);
			}
		}
		lines.push(b);

		// 超时预警：最后1分钟显示倒计时
		if (this.promptStartTime !== null) {
			const elapsed = Date.now() - this.promptStartTime;
			if (elapsed >= SidebarPermissionComponent.WARNING_MS) {
				const remaining = Math.max(0, Math.ceil((SidebarPermissionComponent.TIMEOUT_MS - elapsed) / 1000));
				lines.push(`${b} ${theme.fg("error", `⚠ ${remaining}s 后自动拒绝`)}`);
			}
		}

		if (this.uiState === "trust") {
			this.renderTrustMenu(lines, b, inner);
		} else {
			this.renderMainMenu(lines, b, inner);
		}

		// pad 到宽度
		return lines.map((line) => {
			const vis = visibleWidth(line);
			if (vis < width) return line + " ".repeat(width - vis);
			return truncateToWidth(line, width, "...");
		});
	}

	private renderMainMenu(lines: string[], b: string, inner: number): void {
		for (let i = 0; i < MAIN_OPTIONS.length; i++) {
			const opt = MAIN_OPTIONS[i];
			const isSelected = i === this.selectedIndex;
			const key = theme.fg("accent", `[${opt.shortcut}]`);
			const label = isSelected ? `${theme.fg("accent", "→")} ${key} ${opt.label}` : `  ${key} ${opt.label}`;
			lines.push(`${b} ${truncateToWidth(label, inner, "...")}`);
		}

		if (this.pendingQueue.length > 0) {
			lines.push(`${b} ${theme.fg("dim", `+${this.pendingQueue.length} queued`)}`);
		}
	}

	private static readonly MAX_VISIBLE_TRUST_ITEMS = 5;

	private renderTrustMenu(lines: string[], b: string, inner: number): void {
		lines.push(`${b} ${theme.fg("accent", "Trust scope")}`);

		const total = this.trustLevels.length;
		const maxVisible = SidebarPermissionComponent.MAX_VISIBLE_TRUST_ITEMS;

		// 计算可视窗口范围
		let start = 0;
		let end = total;
		if (total > maxVisible) {
			start = Math.max(0, this.selectedIndex - Math.floor(maxVisible / 2));
			end = start + maxVisible;
			if (end > total) {
				end = total;
				start = end - maxVisible;
			}
		}

		if (start > 0) {
			lines.push(`${b} ${theme.fg("dim", `  ↑ ${start} more`)}`);
		}

		for (let i = start; i < end; i++) {
			const level = this.trustLevels[i];
			const isSelected = i === this.selectedIndex;
			const num = theme.fg("accent", `[${i + 1}]`);
			const label = isSelected ? `${theme.fg("accent", "→")} ${num} ${level.label}` : `  ${num} ${level.label}`;
			lines.push(`${b} ${truncateToWidth(label, inner, "...")}`);
		}

		if (end < total) {
			lines.push(`${b} ${theme.fg("dim", `  ↓ ${total - end} more`)}`);
		}

		lines.push(`${b} ${theme.fg("dim", "[Esc] 返回")}`);
	}
}
