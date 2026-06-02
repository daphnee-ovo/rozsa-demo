import { type Component, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import type { ContextUsage } from "../../../core/extensions/types.ts";
import type { RuntimeStateSnapshot } from "../../../core/runtime-state.ts";
import { theme } from "../theme/theme.ts";
import type { SidebarPermissionComponent } from "./sidebar-permission.ts";

function fmtNumber(value: number | undefined): string {
	if (!value) return "0";
	if (value < 1000) return String(value);
	if (value < 1000000) return `${(value / 1000).toFixed(value < 10000 ? 1 : 0)}k`;
	return `${(value / 1000000).toFixed(1)}M`;
}

function fmtDuration(ms: number | undefined): string {
	if (!ms) return "0s";
	if (ms < 60000) return `${Math.round(ms / 1000)}s`;
	return `${Math.round(ms / 60000)}m`;
}

export class SidebarComponent implements Component {
	private getState: () => RuntimeStateSnapshot;
	private getContextUsage: () => ContextUsage | undefined;
	private permissionComponent: SidebarPermissionComponent | null = null;

	constructor(getState: () => RuntimeStateSnapshot, getContextUsage: () => ContextUsage | undefined) {
		this.getState = getState;
		this.getContextUsage = getContextUsage;
	}

	setPermissionComponent(comp: SidebarPermissionComponent | null): void {
		this.permissionComponent = comp;
	}

	invalidate(): void {}

	render(width: number): string[] {
		if (width < 16) {
			return [];
		}
		const panel = this.renderPanel(width);
		// 在底部追加权限审批（如果有）
		if (this.permissionComponent?.isActive()) {
			const permLines = this.permissionComponent.render(width);
			panel.push(...permLines);
		}
		return panel;
	}

	private renderPanel(width: number): string[] {
		const state = this.getState();
		const ctx = this.getContextUsage();
		const b = theme.fg("borderMuted", "│");
		const inner = width - 2;
		const lines: string[] = [];

		// ─── PROJECT TITLE ───────────────────
		const projectName = (state.project.sessionName ?? state.project.projectName).toUpperCase();
		lines.push(`${b} ${truncateToWidth(theme.bold(theme.fg("accent", projectName)), inner, "...")}`);

		// git
		if (state.gitStatus.enabled) {
			const branch = state.gitStatus.branch ?? "detached";
			const changes = state.gitStatus.uncommittedChangesCount;
			const changesStr = changes > 0 ? ` ·${theme.fg("success", String(changes))} changes` : "";
			lines.push(`${b} ${truncateToWidth(branch + changesStr, inner, "...")}`);
		}

		// model
		const modelName = this.formatModelName(state.modelUsage.model ?? "no-model");
		const thinkLevel = state.modelUsage.reasoningEffort ?? "off";
		const thinkStr = thinkLevel.charAt(0).toUpperCase() + thinkLevel.slice(1);
		lines.push(`${b} ${truncateToWidth(`${modelName}|${thinkStr}`, inner, "...")}`);

		// mode
		const permFull =
			state.permission.mode === "free-permission"
				? theme.fg("warning", "free")
				: state.permission.mode === "auto-permission"
					? theme.fg("success", "auto")
					: "on-request";
		const editModeStr =
			state.editMode === "think_first" ? theme.fg("warning", "think_first") : theme.fg("dim", "normal");
		lines.push(`${b} ▶ ${permFull} · ${editModeStr}`);
		lines.push(b);

		// ─── CONTEXT ─────────────────────────
		const ctxPercent = ctx?.percent ?? 0;
		const ctxLabel = ctx?.percent !== null ? `${Math.round(ctxPercent)}%` : "—";
		lines.push(`${b} ${theme.fg("muted", "CONTEXT")}`);
		lines.push(this.progressBar(width, ctxPercent, ctxLabel));
		lines.push(b);

		// ─── TOKENS ──────────────────────────
		const tokIn = fmtNumber(state.modelUsage.promptTokens);
		const tokOut = fmtNumber(state.modelUsage.completionTokens);
		const tokTotal = fmtNumber(state.modelUsage.sessionTotalTokens);
		lines.push(`${b} ${theme.fg("muted", "TOKENS")} ${theme.fg("dim", `[${tokTotal}]`)}`);
		lines.push(`${b} In ${tokIn} · Out ${tokOut}`);
		lines.push(b);

		// ─── AGENTS（有数据时显示，高亮当前查看的 agent）─────────
		if (state.activeSubagents.length > 0) {
			lines.push(`${b} ${theme.fg("muted", "AGENTS")}`);
			const viewingMain = !state.viewingSubagentId;
			const mainLabel = viewingMain ? theme.bold(theme.fg("accent", "main")) : "main";
			const mainIcon = viewingMain ? theme.fg("accent", "▶") : theme.fg("success", "●");
			lines.push(`${b} ${mainIcon} ${mainLabel}`);
			for (const agent of state.activeSubagents.slice(0, 5)) {
				const elapsed = fmtDuration(agent.elapsedTime ?? (agent.startTime ? Date.now() - agent.startTime : 0));
				const icon =
					agent.id === state.viewingSubagentId
						? theme.fg("accent", "▶")
						: this.agentIcon(agent.status, agent.errorMessage);
				const name = agent.id === state.viewingSubagentId ? theme.bold(theme.fg("accent", agent.name)) : agent.name;
				const suffix = agent.status === "completed" ? theme.fg("dim", "(done)") : theme.fg("dim", elapsed);
				lines.push(`${b} ${icon} ${truncateToWidth(name, inner - 12, "...")} ${suffix}`);
			}
			if (state.activeSubagents.length > 5) {
				lines.push(`${b} ${theme.fg("dim", `  …${state.activeSubagents.length - 5} more`)}`);
			}
			lines.push(b);
		}

		// ─── FILES（项目级未提交变更）────────────────────
		if (state.gitStatus.enabled && state.gitStatus.uncommittedFiles?.length) {
			const totalAdd = state.gitStatus.uncommittedFiles.reduce((s, f) => s + (f.additions ?? 0), 0);
			const totalDel = state.gitStatus.uncommittedFiles.reduce((s, f) => s + (f.deletions ?? 0), 0);
			const statStr = [
				totalAdd > 0 ? theme.fg("success", `+${totalAdd}`) : "",
				totalDel > 0 ? theme.fg("error", `-${totalDel}`) : "",
			]
				.filter(Boolean)
				.join(" ");
			const header = statStr
				? `${theme.fg("muted", "PROJECT FILES")} ${statStr}`
				: theme.fg("muted", "PROJECT FILES");
			lines.push(`${b} ${header}`);
			for (const file of state.gitStatus.uncommittedFiles.slice(0, 10)) {
				const name = file.path.split("/").pop() ?? file.path;
				const icon =
					file.status === "added"
						? theme.fg("success", "+")
						: file.status === "deleted"
							? theme.fg("error", "−")
							: theme.fg("warning", "~");
				const lineStats =
					file.additions !== undefined || file.deletions !== undefined
						? " " +
							(file.additions !== undefined ? theme.fg("success", `+${file.additions}`) : "") +
							(file.deletions !== undefined ? theme.fg("error", `-${file.deletions}`) : "")
						: "";
				lines.push(`${b} ${icon} ${truncateToWidth(name, inner - 3, "...")}${lineStats}`);
			}
			if (state.gitStatus.uncommittedFiles.length > 10) {
				lines.push(`${b} ${theme.fg("dim", `  …${state.gitStatus.uncommittedFiles.length - 10} more`)}`);
			}
			lines.push(b);
		}

		// ─── FILES（当前会话变更）──────────────────────
		if (state.changedFiles.length > 0) {
			const totalAdd = state.changedFiles.reduce((s, f) => s + (f.additions ?? 0), 0);
			const totalDel = state.changedFiles.reduce((s, f) => s + (f.deletions ?? 0), 0);
			const statStr = [
				totalAdd > 0 ? theme.fg("success", `+${totalAdd}`) : "",
				totalDel > 0 ? theme.fg("error", `-${totalDel}`) : "",
			]
				.filter(Boolean)
				.join(" ");
			const filesHeader = statStr
				? `${theme.fg("muted", "SESSION FILES")} ${statStr}`
				: theme.fg("muted", "SESSION FILES");
			lines.push(`${b} ${filesHeader}`);
			for (const file of state.changedFiles.slice(0, 5)) {
				const name = file.path.split("/").pop() ?? file.path;
				const icon =
					file.status === "added"
						? theme.fg("success", "+")
						: file.status === "deleted"
							? theme.fg("error", "−")
							: theme.fg("warning", "~");
				lines.push(`${b} ${icon} ${truncateToWidth(name, inner - 3, "...")}`);
			}
			if (state.changedFiles.length > 5) {
				lines.push(`${b} ${theme.fg("dim", `  …${state.changedFiles.length - 5} more`)}`);
			}
			lines.push(b);
		}

		// ─── TOOLS（有调用时才显示）──────────
		if (state.toolCallStats.length > 0) {
			lines.push(`${b} ${theme.fg("muted", "TOOLS")}`);
			const sorted = [...state.toolCallStats].sort((a, b) => b.callCount - a.callCount).slice(0, 4);
			const toolParts = sorted.map((t) => `${t.toolName} ×${t.callCount}`);
			let current = "";
			for (const part of toolParts) {
				if (current.length === 0) {
					current = part;
				} else if (current.length + 2 + part.length <= inner - 1) {
					current += `  ${part}`;
				} else {
					lines.push(`${b} ${current}`);
					current = part;
				}
			}
			if (current.length > 0) {
				lines.push(`${b} ${current}`);
			}
			lines.push(b);
		}

		return lines.map((line) => {
			const vis = visibleWidth(line);
			if (vis < width) return line + " ".repeat(width - vis);
			return truncateToWidth(line, width, "...");
		});
	}

	private progressBar(width: number, percent: number, label: string): string {
		const b = theme.fg("borderMuted", "│");
		const labelWidth = label.length + 1;
		const barWidth = width - 3 - labelWidth;
		const filled = Math.round((percent / 100) * barWidth);
		const empty = barWidth - filled;

		let color: "success" | "warning" | "error" = "success";
		if (percent > 90) color = "error";
		else if (percent > 70) color = "warning";

		const labelStr = this.colorByPercent(label, percent);
		return `${b} ${theme.fg(color, "▓".repeat(filled))}${theme.fg("dim", "░".repeat(empty))} ${labelStr}`;
	}

	private colorByPercent(text: string, percent: number): string {
		if (percent > 90) return theme.fg("error", text);
		if (percent > 70) return theme.fg("warning", text);
		return theme.fg("success", text);
	}

	private agentIcon(status: string, errorMessage?: string): string {
		switch (status) {
			case "completed":
				return theme.fg("success", "●");
			case "failed":
				return theme.fg("error", "○");
			case "running":
				if (errorMessage?.includes("permission")) {
					return theme.fg("warning", "○");
				}
				return "○";
			default:
				return "●";
		}
	}

	private formatModelName(model: string): string {
		const parts = model.split(".");
		let name = parts[parts.length - 1];
		name = name.replace(/-v\d+$/, "");
		name = name.replace(/(\d+)-(\d+)$/, "$1.$2");
		return name;
	}
}
