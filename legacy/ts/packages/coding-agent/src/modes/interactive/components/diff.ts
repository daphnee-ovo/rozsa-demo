import * as Diff from "diff";
import type { HighlightTheme } from "../../../utils/syntax-highlight.ts";
import { highlight, supportsLanguage } from "../../../utils/syntax-highlight.ts";
import { getLanguageFromPath, theme } from "../theme/theme.ts";

/**
 * Parse diff line to extract prefix, line number, and content.
 * Format: "+123 content" or "-123 content" or " 123 content" or "     ..."
 */
function parseDiffLine(line: string): { prefix: string; lineNum: string; content: string } | null {
	const match = line.match(/^([+-\s])(\s*\d*)\s(.*)$/);
	if (!match) return null;
	return { prefix: match[1], lineNum: match[2], content: match[3] };
}

/**
 * Replace tabs with spaces for consistent rendering.
 */
function replaceTabs(text: string): string {
	return text.replace(/\t/g, "   ");
}

// ============================================================================
// 语法高亮相关
// ============================================================================

/**
 * 构建 diff 中使用的语法高亮主题。
 * 使用主题中定义的 syntax* 颜色进行代码着色。
 */
function buildDiffHighlightTheme(): HighlightTheme {
	return {
		keyword: (s: string) => theme.fg("syntaxKeyword", s),
		built_in: (s: string) => theme.fg("syntaxType", s),
		literal: (s: string) => theme.fg("syntaxNumber", s),
		number: (s: string) => theme.fg("syntaxNumber", s),
		string: (s: string) => theme.fg("syntaxString", s),
		comment: (s: string) => theme.fg("syntaxComment", s),
		function: (s: string) => theme.fg("syntaxFunction", s),
		title: (s: string) => theme.fg("syntaxFunction", s),
		class: (s: string) => theme.fg("syntaxType", s),
		type: (s: string) => theme.fg("syntaxType", s),
		attr: (s: string) => theme.fg("syntaxVariable", s),
		variable: (s: string) => theme.fg("syntaxVariable", s),
		params: (s: string) => theme.fg("syntaxVariable", s),
		operator: (s: string) => theme.fg("syntaxOperator", s),
		punctuation: (s: string) => theme.fg("syntaxPunctuation", s),
	};
}

/**
 * 对单行代码应用语法高亮。
 * 如果语言无效或高亮失败，返回 null 表示回退到原始行为。
 */
function highlightLine(content: string, language: string | undefined): string | null {
	if (!language || !supportsLanguage(language)) {
		return null;
	}
	try {
		const highlighted = highlight(content, {
			language,
			ignoreIllegals: true,
			theme: buildDiffHighlightTheme(),
		});
		return highlighted;
	} catch {
		return null;
	}
}

/**
 * Compute word-level diff and render with inverse on changed parts.
 * Uses diffWords which groups whitespace with adjacent words for cleaner highlighting.
 * Strips leading whitespace from inverse to avoid highlighting indentation.
 *
 * 当提供 language 时，对未变更部分应用语法高亮。
 */
function renderIntraLineDiff(
	oldContent: string,
	newContent: string,
	language?: string,
): { removedLine: string; addedLine: string } {
	const wordDiff = Diff.diffWords(oldContent, newContent);

	let removedLine = "";
	let addedLine = "";
	let isFirstRemoved = true;
	let isFirstAdded = true;

	// 收集未变更部分的原始文本，用于后续语法高亮
	// 对变更部分使用 inverse 样式
	for (const part of wordDiff) {
		if (part.removed) {
			let value = part.value;
			// Strip leading whitespace from the first removed part
			if (isFirstRemoved) {
				const leadingWs = value.match(/^(\s*)/)?.[1] || "";
				value = value.slice(leadingWs.length);
				removedLine += leadingWs;
				isFirstRemoved = false;
			}
			if (value) {
				removedLine += theme.inverse(value);
			}
		} else if (part.added) {
			let value = part.value;
			// Strip leading whitespace from the first added part
			if (isFirstAdded) {
				const leadingWs = value.match(/^(\s*)/)?.[1] || "";
				value = value.slice(leadingWs.length);
				addedLine += leadingWs;
				isFirstAdded = false;
			}
			if (value) {
				addedLine += theme.inverse(value);
			}
		} else {
			// 未变更部分：应用语法高亮（如果可用）
			const highlighted = language ? highlightLine(part.value, language) : null;
			const rendered = highlighted ?? part.value;
			removedLine += rendered;
			addedLine += rendered;
		}
	}

	return { removedLine, addedLine };
}

export interface RenderDiffOptions {
	/** 文件路径，用于推断语言以进行语法高亮 */
	filePath?: string;
}

/**
 * Render a diff string with colored lines and intra-line change highlighting.
 * - Context lines: 语法高亮（如果可用），否则 dim/gray
 * - Removed lines: 前缀红色 + 语法高亮内容，word-level 变更用 inverse
 * - Added lines: 前缀绿色 + 语法高亮内容，word-level 变更用 inverse
 */
export function renderDiff(diffText: string, options: RenderDiffOptions = {}): string {
	const lines = diffText.split("\n");
	const result: string[] = [];

	// 从文件路径推断语言
	const language = options.filePath ? getLanguageFromPath(options.filePath) : undefined;

	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		const parsed = parseDiffLine(line);

		if (!parsed) {
			result.push(theme.fg("toolDiffContext", line));
			i++;
			continue;
		}

		if (parsed.prefix === "-") {
			// Collect consecutive removed lines
			const removedLines: { lineNum: string; content: string }[] = [];
			while (i < lines.length) {
				const p = parseDiffLine(lines[i]);
				if (!p || p.prefix !== "-") break;
				removedLines.push({ lineNum: p.lineNum, content: p.content });
				i++;
			}

			// Collect consecutive added lines
			const addedLines: { lineNum: string; content: string }[] = [];
			while (i < lines.length) {
				const p = parseDiffLine(lines[i]);
				if (!p || p.prefix !== "+") break;
				addedLines.push({ lineNum: p.lineNum, content: p.content });
				i++;
			}

			// Only do intra-line diffing when there's exactly one removed and one added line
			// (indicating a single line modification). Otherwise, show lines as-is.
			if (removedLines.length === 1 && addedLines.length === 1) {
				const removed = removedLines[0];
				const added = addedLines[0];

				const { removedLine, addedLine } = renderIntraLineDiff(
					replaceTabs(removed.content),
					replaceTabs(added.content),
					language,
				);

				// 前缀（-/+ 和行号）使用 diff 颜色，内容使用语法高亮
				result.push(`${theme.fg("toolDiffRemoved", `-${removed.lineNum}`)} ${removedLine}`);
				result.push(`${theme.fg("toolDiffAdded", `+${added.lineNum}`)} ${addedLine}`);
			} else {
				// 批量删除/添加行：前缀使用 diff 颜色，内容使用语法高亮
				for (const removed of removedLines) {
					const content = replaceTabs(removed.content);
					const highlighted = highlightLine(content, language);
					if (highlighted !== null) {
						result.push(`${theme.fg("toolDiffRemoved", `-${removed.lineNum}`)} ${highlighted}`);
					} else {
						result.push(theme.fg("toolDiffRemoved", `-${removed.lineNum} ${content}`));
					}
				}
				for (const added of addedLines) {
					const content = replaceTabs(added.content);
					const highlighted = highlightLine(content, language);
					if (highlighted !== null) {
						result.push(`${theme.fg("toolDiffAdded", `+${added.lineNum}`)} ${highlighted}`);
					} else {
						result.push(theme.fg("toolDiffAdded", `+${added.lineNum} ${content}`));
					}
				}
			}
		} else if (parsed.prefix === "+") {
			// Standalone added line：前缀使用 diff 颜色，内容使用语法高亮
			const content = replaceTabs(parsed.content);
			const highlighted = highlightLine(content, language);
			if (highlighted !== null) {
				result.push(`${theme.fg("toolDiffAdded", `+${parsed.lineNum}`)} ${highlighted}`);
			} else {
				result.push(theme.fg("toolDiffAdded", `+${parsed.lineNum} ${content}`));
			}
			i++;
		} else {
			// Context line：应用语法高亮，回退到 dim 颜色
			const content = replaceTabs(parsed.content);
			const highlighted = highlightLine(content, language);
			if (highlighted !== null) {
				result.push(`${theme.fg("toolDiffContext", ` ${parsed.lineNum}`)} ${highlighted}`);
			} else {
				result.push(theme.fg("toolDiffContext", ` ${parsed.lineNum} ${content}`));
			}
			i++;
		}
	}

	return result.join("\n");
}
