import type { Component } from "../tui.ts";
import { truncateToWidth, visibleWidth } from "../utils.ts";

/**
 * Columns - 水平并排渲染两个组件（左侧主内容 + 右侧边栏）
 */
export class Columns implements Component {
	private left: Component;
	private right: Component;
	private rightWidth: number;
	private gap: number;
	// 最小左侧宽度，低于此值时隐藏右侧
	private minLeftWidth: number;

	constructor(
		left: Component,
		right: Component,
		options?: { rightWidth?: number; gap?: number; minLeftWidth?: number },
	) {
		this.left = left;
		this.right = right;
		this.rightWidth = options?.rightWidth ?? 30;
		this.gap = options?.gap ?? 1;
		this.minLeftWidth = options?.minLeftWidth ?? 60;
	}

	invalidate(): void {
		this.left.invalidate?.();
		this.right.invalidate?.();
	}

	render(width: number): string[] {
		const leftWidth = width - this.rightWidth - this.gap;

		// 终端太窄时退化为仅左侧
		if (leftWidth < this.minLeftWidth) {
			return this.left.render(width);
		}

		const leftLines = this.left.render(leftWidth);
		const rightLines = this.right.render(this.rightWidth);

		const maxLines = Math.max(leftLines.length, rightLines.length);
		const gapStr = " ".repeat(this.gap);
		const result: string[] = [];

		for (let i = 0; i < maxLines; i++) {
			const leftLine = leftLines[i] ?? "";
			const rightLine = rightLines[i] ?? "";

			// 将左侧行填充到固定宽度
			const leftVisible = visibleWidth(leftLine);
			let paddedLeft: string;
			if (leftVisible >= leftWidth) {
				paddedLeft = truncateToWidth(leftLine, leftWidth);
			} else {
				paddedLeft = leftLine + " ".repeat(leftWidth - leftVisible);
			}

			result.push(paddedLeft + gapStr + rightLine);
		}

		return result;
	}
}
