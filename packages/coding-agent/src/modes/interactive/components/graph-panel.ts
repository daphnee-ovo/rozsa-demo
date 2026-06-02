import type { AgentMessage } from "@earendil-works/pi-agent-core";
import {
	type Component,
	Container,
	type Focusable,
	getKeybindings,
	Markdown,
	type MarkdownTheme,
	Spacer,
	TruncatedText,
	truncateToWidth,
} from "@earendil-works/pi-tui";
import type { SessionEntry } from "../../../core/session-manager.ts";
import { getMarkdownTheme, theme } from "../theme/theme.ts";
import { DynamicBorder } from "./dynamic-border.ts";

interface GraphNode {
	entry: SessionEntry;
	role: "user" | "assistant";
	summary: string;
	fullText: string;
	timestamp: string;
}

type PanelMode = "list" | "detail";

/**
 * Graph 面板核心：双模式
 * list 模式：左右分栏（节点列表 + 预览）
 * detail 模式：全屏显示选中消息内容，可上下滚动
 */
class GraphPanel implements Component {
	private nodes: GraphNode[] = [];
	private filteredNodes: GraphNode[] = [];
	private selectedIndex = 0;
	private terminalHeight: number;
	private searchQuery = "";
	private mode: PanelMode = "list";
	private detailScrollOffset = 0;
	private detailLines: string[] = [];
	private detailRenderWidth = 0;
	private detailSearchQuery = "";
	private detailSearchMatches: number[] = [];
	private detailSearchIndex = 0;
	private markdownTheme: MarkdownTheme = getMarkdownTheme();

	public onCancel?: () => void;

	constructor(entries: SessionEntry[], terminalHeight: number) {
		this.terminalHeight = terminalHeight;
		this.nodes = this.buildNodes(entries);
		this.filteredNodes = [...this.nodes];
		this.selectedIndex = Math.max(0, this.filteredNodes.length - 1);
	}

	private buildNodes(entries: SessionEntry[]): GraphNode[] {
		const result: GraphNode[] = [];
		for (const entry of entries) {
			if (entry.type !== "message") continue;
			const msg = entry.message;
			const role = this.classifyRole(msg);
			if (role !== "user" && role !== "assistant") continue;
			const fullText = this.extractFullText(msg);
			if (role === "assistant" && !fullText) continue;
			const summary = fullText.replace(/[\n\t]+/g, " ").trim();
			result.push({
				entry,
				role,
				summary,
				fullText,
				timestamp: this.formatTime(entry.timestamp),
			});
		}
		return result;
	}

	private classifyRole(msg: AgentMessage): "user" | "assistant" | "other" {
		if (msg.role === "user") return "user";
		if (msg.role === "assistant") return "assistant";
		return "other";
	}

	private extractFullText(msg: AgentMessage): string {
		const content = (msg as { content?: unknown }).content;
		if (typeof content === "string") return content.trim();
		if (Array.isArray(content)) {
			const parts: string[] = [];
			for (const block of content) {
				if (typeof block === "object" && block !== null && "type" in block) {
					if (block.type === "text" && "text" in block) {
						parts.push(String((block as { text: string }).text));
					}
				}
			}
			return parts.join("\n").trim();
		}
		return "";
	}

	private formatTime(timestamp: string): string {
		try {
			const d = new Date(timestamp);
			const h = d.getHours().toString().padStart(2, "0");
			const m = d.getMinutes().toString().padStart(2, "0");
			return `${h}:${m}`;
		} catch {
			return "";
		}
	}

	invalidate(): void {}

	getSearchQuery(): string {
		return this.searchQuery;
	}

	getMode(): PanelMode {
		return this.mode;
	}

	getDetailSearch(): string {
		return this.detailSearchQuery;
	}

	render(width: number): string[] {
		if (this.mode === "detail") {
			return this.renderDetail(width);
		}
		return this.renderList(width);
	}

	private renderList(width: number): string[] {
		const margin = 1;
		const sep = ` ${theme.fg("borderMuted", "│")} `;
		const sepWidth = 3;
		const leftWidth = Math.min(Math.floor((width - margin - sepWidth) * 0.38), 46);
		const rightWidth = width - margin - leftWidth - sepWidth;
		const availableHeight = Math.max(6, this.terminalHeight - 10);

		const leftLines = this.renderNodeList(leftWidth, availableHeight);
		const rightLines = this.renderPreview(rightWidth, availableHeight);

		const lines: string[] = [];
		const maxLines = Math.max(leftLines.length, rightLines.length);
		const pad = " ".repeat(margin);

		for (let i = 0; i < maxLines; i++) {
			const left = i < leftLines.length ? leftLines[i] : "";
			const right = i < rightLines.length ? rightLines[i] : "";
			const leftVisible = this.visibleLength(left);
			const leftPadded = left + " ".repeat(Math.max(0, leftWidth - leftVisible));
			lines.push(truncateToWidth(`${pad}${leftPadded}${sep}${right}`, width));
		}

		return lines;
	}

	private renderNodeList(width: number, maxLines: number): string[] {
		const lines: string[] = [];

		if (this.filteredNodes.length === 0) {
			lines.push(theme.fg("muted", "  (empty)"));
			return lines;
		}

		const listHeight = maxLines;

		const startIndex = Math.max(
			0,
			Math.min(this.selectedIndex - Math.floor(listHeight / 2), this.filteredNodes.length - listHeight),
		);
		const endIndex = Math.min(startIndex + listHeight, this.filteredNodes.length);

		for (let i = startIndex; i < endIndex; i++) {
			const node = this.filteredNodes[i];
			const isSelected = i === this.selectedIndex;

			const roleIcon = node.role === "user" ? "›" : "◆";
			const nodeColor = node.role === "user" ? "accent" : "success";
			const icon = theme.fg(nodeColor as "accent" | "success", roleIcon);

			const timeStr = theme.fg("dim", node.timestamp);
			const dot = theme.fg("borderMuted", "·");

			// "▸ › HH:MM · " = ~14 字符
			const fixedLen = 14;
			const summaryMax = Math.max(5, width - fixedLen);
			const summary = node.summary.length > summaryMax ? `${node.summary.slice(0, summaryMax - 1)}…` : node.summary;

			const cursor = isSelected ? theme.fg("accent", "▸ ") : "  ";
			let line = `${cursor}${icon} ${timeStr} ${dot} ${theme.fg(isSelected ? "text" : "muted", summary)}`;
			if (isSelected) {
				line = theme.bg("selectedBg", line);
			}
			lines.push(truncateToWidth(line, width));
		}

		return lines;
	}

	private renderPreview(width: number, maxLines: number): string[] {
		if (this.filteredNodes.length === 0) return [];

		const node = this.filteredNodes[this.selectedIndex];
		if (!node) return [];

		const lines: string[] = [];

		// 头部：角色标签 + 时间 + 位置指示
		const roleLabel = node.role === "user" ? "USER" : "ASSISTANT";
		const roleColor = node.role === "user" ? "accent" : "success";
		const roleTag = theme.bold(theme.fg(roleColor as "accent" | "success", `▎${roleLabel}`));
		const posInfo = theme.fg("dim", `${this.selectedIndex + 1}/${this.filteredNodes.length}`);
		lines.push(`${roleTag}  ${theme.fg("dim", node.timestamp)}  ${posInfo}`);
		lines.push(theme.fg("borderMuted", "╌".repeat(Math.min(width, 40))));

		// 内容：用 Markdown 渲染
		const md = new Markdown(node.fullText, 0, 0, this.markdownTheme);
		const contentLines = md.render(width);
		const contentMax = maxLines - 3;
		for (let i = 0; i < Math.min(contentLines.length, contentMax); i++) {
			lines.push(contentLines[i]);
		}
		if (contentLines.length > contentMax) {
			const more = contentLines.length - contentMax;
			lines.push("");
			lines.push(theme.fg("dim", `  ↓ ${more} more lines`) + theme.fg("muted", " · Enter to expand"));
		}

		return lines;
	}

	private renderDetail(width: number): string[] {
		const availableHeight = Math.max(6, this.terminalHeight - 10);

		// 用 Markdown 渲染（带缓存，宽度变化时重新渲染）
		if (this.detailLines.length === 0 || this.detailRenderWidth !== width) {
			this.detailRenderWidth = width;
			const node = this.filteredNodes[this.selectedIndex];
			if (!node) return [];
			const md = new Markdown(node.fullText, 0, 0, this.markdownTheme);
			this.detailLines = md.render(width - 2);
		}

		const node = this.filteredNodes[this.selectedIndex];
		const lines: string[] = [];

		// 头部：角色 + 滚动位置百分比
		if (node) {
			const roleLabel = node.role === "user" ? "USER" : "ASSISTANT";
			const roleColor = node.role === "user" ? "accent" : "success";
			const tag = theme.bold(theme.fg(roleColor as "accent" | "success", `▎${roleLabel}`));
			const percent =
				this.detailLines.length > 0
					? Math.round((this.detailScrollOffset / Math.max(1, this.detailLines.length - availableHeight)) * 100)
					: 0;
			const posLabel = this.detailScrollOffset === 0 ? "TOP" : percent >= 100 ? "END" : `${Math.min(99, percent)}%`;
			const posStr = theme.fg("dim", posLabel);
			const lineInfo = theme.fg("dim", `${this.detailLines.length} lines`);
			lines.push(` ${tag}  ${theme.fg("dim", node.timestamp)}  ${lineInfo}  ${posStr}`);
			lines.push(` ${theme.fg("borderMuted", "╌".repeat(Math.min(width - 2, 50)))}`);
		}

		const contentHeight = availableHeight - 2;
		const endIndex = Math.min(this.detailScrollOffset + contentHeight, this.detailLines.length);
		for (let i = this.detailScrollOffset; i < endIndex; i++) {
			lines.push(` ${truncateToWidth(this.detailLines[i], width - 1)}`);
		}

		while (lines.length < availableHeight) {
			lines.push("");
		}

		return lines;
	}

	private enterDetail(): void {
		const node = this.filteredNodes[this.selectedIndex];
		if (!node) return;
		this.mode = "detail";
		this.detailScrollOffset = 0;
		this.detailLines = [];
		this.detailRenderWidth = 0;
	}

	private exitDetail(): void {
		this.mode = "list";
		this.detailLines = [];
		this.detailScrollOffset = 0;
	}

	handleInput(keyData: string): void {
		if (this.mode === "detail") {
			this.handleDetailInput(keyData);
		} else {
			this.handleListInput(keyData);
		}
	}

	private handleDetailInput(keyData: string): void {
		const kb = getKeybindings();
		const pageSize = Math.max(1, this.terminalHeight - 10);
		const maxOffset = Math.max(0, this.detailLines.length - pageSize);
		const fastStep = 5;

		// 搜索模式：有搜索内容时，特殊处理
		if (this.detailSearchQuery) {
			if (kb.matches(keyData, "tui.select.cancel")) {
				this.detailSearchQuery = "";
				this.detailSearchMatches = [];
				return;
			}
			if (kb.matches(keyData, "tui.select.confirm") || keyData === "n") {
				// 下一个匹配
				if (this.detailSearchMatches.length > 0) {
					this.detailSearchIndex = (this.detailSearchIndex + 1) % this.detailSearchMatches.length;
					this.detailScrollOffset = Math.min(maxOffset, this.detailSearchMatches[this.detailSearchIndex]);
				}
				return;
			}
			if (keyData === "N") {
				// 上一个匹配
				if (this.detailSearchMatches.length > 0) {
					this.detailSearchIndex =
						(this.detailSearchIndex - 1 + this.detailSearchMatches.length) % this.detailSearchMatches.length;
					this.detailScrollOffset = Math.min(maxOffset, this.detailSearchMatches[this.detailSearchIndex]);
				}
				return;
			}
			if (kb.matches(keyData, "tui.editor.deleteCharBackward")) {
				this.detailSearchQuery = this.detailSearchQuery.slice(0, -1);
				if (this.detailSearchQuery) {
					this.applyDetailSearch();
				} else {
					this.detailSearchMatches = [];
				}
				return;
			}
			// 继续输入搜索字符
			const hasControlChars = [...keyData].some((ch) => {
				const code = ch.charCodeAt(0);
				return code < 32 || code === 0x7f || (code >= 0x80 && code <= 0x9f);
			});
			if (!hasControlChars && keyData.length > 0) {
				this.detailSearchQuery += keyData;
				this.applyDetailSearch();
				return;
			}
		}

		// 普通导航
		if (kb.matches(keyData, "tui.select.up")) {
			this.detailScrollOffset = Math.max(0, this.detailScrollOffset - 1);
		} else if (kb.matches(keyData, "tui.select.down")) {
			this.detailScrollOffset = Math.min(maxOffset, this.detailScrollOffset + 1);
		} else if (keyData === "k") {
			this.detailScrollOffset = Math.max(0, this.detailScrollOffset - fastStep);
		} else if (keyData === "j") {
			this.detailScrollOffset = Math.min(maxOffset, this.detailScrollOffset + fastStep);
		} else if (kb.matches(keyData, "tui.select.pageUp")) {
			this.detailScrollOffset = Math.max(0, this.detailScrollOffset - pageSize);
		} else if (kb.matches(keyData, "tui.select.pageDown")) {
			this.detailScrollOffset = Math.min(maxOffset, this.detailScrollOffset + pageSize);
		} else if (kb.matches(keyData, "tui.editor.cursorLineStart")) {
			// Home
			this.detailScrollOffset = 0;
		} else if (kb.matches(keyData, "tui.editor.cursorLineEnd")) {
			// End
			this.detailScrollOffset = maxOffset;
		} else if (keyData === "g") {
			// gg = 跳到顶部 (vim style)
			this.detailScrollOffset = 0;
		} else if (keyData === "G") {
			// G = 跳到底部
			this.detailScrollOffset = maxOffset;
		} else if (keyData >= "1" && keyData <= "9") {
			// 数字跳转：按 1-9 跳到 10%-90% 位置
			const percent = parseInt(keyData, 10) * 10;
			this.detailScrollOffset = Math.min(maxOffset, Math.floor((maxOffset * percent) / 100));
		} else if (keyData === "/") {
			// 进入搜索模式
			this.detailSearchQuery = "/";
			this.detailSearchMatches = [];
		} else if (kb.matches(keyData, "tui.select.cancel") || kb.matches(keyData, "tui.select.confirm")) {
			this.exitDetail();
		}
	}

	private applyDetailSearch(): void {
		// 去掉开头的 /
		const query = this.detailSearchQuery.startsWith("/")
			? this.detailSearchQuery.slice(1).toLowerCase()
			: this.detailSearchQuery.toLowerCase();
		if (!query) {
			this.detailSearchMatches = [];
			return;
		}
		this.detailSearchMatches = [];
		for (let i = 0; i < this.detailLines.length; i++) {
			const lineText = this.detailLines[i].replace(/\x1b\[[0-9;]*m/g, "").toLowerCase();
			if (lineText.includes(query)) {
				this.detailSearchMatches.push(i);
			}
		}
		// 跳到第一个匹配
		this.detailSearchIndex = 0;
		if (this.detailSearchMatches.length > 0) {
			const pageSize = Math.max(1, this.terminalHeight - 10);
			const maxOffset = Math.max(0, this.detailLines.length - pageSize);
			this.detailScrollOffset = Math.min(maxOffset, this.detailSearchMatches[0]);
		}
	}

	private handleListInput(keyData: string): void {
		const kb = getKeybindings();
		if (kb.matches(keyData, "tui.select.up")) {
			this.selectedIndex = this.selectedIndex === 0 ? this.filteredNodes.length - 1 : this.selectedIndex - 1;
		} else if (kb.matches(keyData, "tui.select.down")) {
			this.selectedIndex = this.selectedIndex === this.filteredNodes.length - 1 ? 0 : this.selectedIndex + 1;
		} else if (kb.matches(keyData, "tui.select.pageUp")) {
			this.selectedIndex = Math.max(0, this.selectedIndex - 5);
		} else if (kb.matches(keyData, "tui.select.pageDown")) {
			this.selectedIndex = Math.min(this.filteredNodes.length - 1, this.selectedIndex + 5);
		} else if (kb.matches(keyData, "tui.select.confirm")) {
			this.enterDetail();
		} else if (kb.matches(keyData, "tui.select.cancel")) {
			if (this.searchQuery) {
				this.searchQuery = "";
				this.applyFilter();
			} else {
				this.onCancel?.();
			}
		} else if (kb.matches(keyData, "tui.editor.deleteCharBackward")) {
			if (this.searchQuery.length > 0) {
				this.searchQuery = this.searchQuery.slice(0, -1);
				this.applyFilter();
			}
		} else {
			const hasControlChars = [...keyData].some((ch) => {
				const code = ch.charCodeAt(0);
				return code < 32 || code === 0x7f || (code >= 0x80 && code <= 0x9f);
			});
			if (!hasControlChars && keyData.length > 0) {
				this.searchQuery += keyData;
				this.applyFilter();
			}
		}
	}

	private applyFilter(): void {
		const tokens = this.searchQuery.toLowerCase().split(/\s+/).filter(Boolean);
		if (tokens.length === 0) {
			this.filteredNodes = [...this.nodes];
		} else {
			this.filteredNodes = this.nodes.filter((node) => {
				const text = `${node.role} ${node.fullText}`.toLowerCase();
				return tokens.every((t) => text.includes(t));
			});
		}
		if (this.selectedIndex >= this.filteredNodes.length) {
			this.selectedIndex = Math.max(0, this.filteredNodes.length - 1);
		}
	}

	private visibleLength(text: string): number {
		return text.replace(/\x1b\[[0-9;]*m/g, "").length;
	}
}

/** 状态栏：powerline 风格分段式提示 */
class GraphStatusLine implements Component {
	private panel: GraphPanel;
	constructor(panel: GraphPanel) {
		this.panel = panel;
	}
	invalidate(): void {}
	render(width: number): string[] {
		const sep = theme.fg("borderMuted", " › ");
		if (this.panel.getMode() === "detail") {
			const search = this.panel.getDetailSearch();
			if (search) {
				const parts = [
					theme.fg("accent", ` ${search} `),
					theme.fg("muted", "n/N next"),
					theme.fg("muted", "Esc clear"),
				];
				return [this.center(parts.join(sep), width)];
			}
			const parts = [
				theme.fg("muted", "↑↓/j/k scroll"),
				theme.fg("muted", "PgUp/Dn"),
				theme.fg("muted", "1-9 jump"),
				theme.fg("muted", "g/G"),
				theme.fg("muted", "/ search"),
				theme.fg("muted", "Esc back"),
			];
			return [this.center(parts.join(sep), width)];
		}
		const query = this.panel.getSearchQuery();
		if (query) {
			return [this.center(`${theme.fg("borderMuted", "filter:")} ${theme.fg("accent", query)}`, width)];
		}
		const parts = [
			theme.fg("muted", "↑↓ navigate"),
			theme.fg("muted", "Enter expand"),
			theme.fg("muted", "type to filter"),
			theme.fg("muted", "Esc close"),
		];
		return [this.center(parts.join(sep), width)];
	}
	private center(text: string, width: number): string {
		const visible = text.replace(/\x1b\[[0-9;]*m/g, "").length;
		const left = Math.max(0, Math.floor((width - visible) / 2));
		return " ".repeat(left) + text;
	}
	handleInput(_keyData: string): void {}
}

/**
 * Graph Panel 全屏模态组件
 */
export class GraphPanelComponent extends Container implements Focusable {
	private graphPanel: GraphPanel;

	private _focused = false;
	get focused(): boolean {
		return this._focused;
	}
	set focused(value: boolean) {
		this._focused = value;
	}

	constructor(entries: SessionEntry[], terminalHeight: number, onCancel: () => void) {
		super();

		this.graphPanel = new GraphPanel(entries, terminalHeight);
		this.graphPanel.onCancel = onCancel;

		this.addChild(new Spacer(1));
		this.addChild(new DynamicBorder((s) => theme.fg("borderAccent", s)));
		this.addChild(new TruncatedText(theme.bold("  Session Graph"), 1, 0));
		this.addChild(new GraphStatusLine(this.graphPanel));
		this.addChild(new DynamicBorder());
		this.addChild(new Spacer(1));
		this.addChild(this.graphPanel);
		this.addChild(new Spacer(1));
		this.addChild(new DynamicBorder());
	}

	handleInput(keyData: string): void {
		this.graphPanel.handleInput(keyData);
	}
}
