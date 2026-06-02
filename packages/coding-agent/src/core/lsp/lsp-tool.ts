/**
 * LSP 工具定义
 *
 * coding-agent/src/core/lsp/
 * └── lsp-tool.ts          ← 本文件：注册 LSP 查询工具供 AI 模型调用
 *     └── lsp-core.ts      ← LSP 管理器核心（连接、协议交互）
 *
 * 提供的 LSP 操作：
 *   definition / references / hover / symbols / diagnostics /
 *   workspace-diagnostics / signature / rename / codeAction
 */

import { readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { type Static, Type } from "typebox";
import type { ToolDefinition } from "../extensions/types.ts";
import { LSPManager } from "./lsp-core.ts";

// ============================================================================
// 参数 Schema（TypeBox）
// ============================================================================

const LspActionEnum = Type.Union(
	[
		Type.Literal("definition"),
		Type.Literal("references"),
		Type.Literal("hover"),
		Type.Literal("symbols"),
		Type.Literal("diagnostics"),
		Type.Literal("workspace-diagnostics"),
		Type.Literal("signature"),
		Type.Literal("rename"),
		Type.Literal("codeAction"),
	],
	{ description: "要执行的 LSP 操作类型" },
);

const LspParamsSchema = Type.Object({
	action: LspActionEnum,
	file: Type.String({ description: "目标文件路径（相对或绝对路径）" }),
	line: Type.Optional(Type.Integer({ description: "1-based 行号" })),
	column: Type.Optional(Type.Integer({ description: "1-based 列号" })),
	symbol: Type.Optional(Type.String({ description: "符号名称（当未提供 line/column 时用于定位）" })),
	newName: Type.Optional(Type.String({ description: "重命名操作的新名称（仅 rename action 需要）" })),
});

export type LspToolInput = Static<typeof LspParamsSchema>;

// ============================================================================
// 符号类型映射
// ============================================================================

const SYMBOL_KIND_MAP: Record<number, string> = {
	1: "File",
	2: "Module",
	3: "Namespace",
	4: "Package",
	5: "Class",
	6: "Method",
	7: "Property",
	8: "Field",
	9: "Constructor",
	10: "Enum",
	11: "Interface",
	12: "Function",
	13: "Variable",
	14: "Constant",
	15: "String",
	16: "Number",
	17: "Boolean",
	18: "Array",
	19: "Object",
	20: "Key",
	21: "Null",
	22: "EnumMember",
	23: "Struct",
	24: "Event",
	25: "Operator",
	26: "TypeParameter",
};

/** 诊断严重级别映射 */
const SEVERITY_MAP: Record<number, string> = {
	1: "Error",
	2: "Warning",
	3: "Information",
	4: "Hint",
};

// ============================================================================
// 辅助函数
// ============================================================================

/**
 * 将文件路径解析为绝对路径
 */
function resolveFilePath(file: string, cwd: string): string {
	if (file.startsWith("/")) return file;
	return resolve(cwd, file);
}

/**
 * 安全读取文件内容
 */
function safeReadFile(absolutePath: string): string | null {
	try {
		return readFileSync(absolutePath, "utf-8");
	} catch {
		return null;
	}
}

/**
 * 在文件内容中根据符号名搜索位置
 * 返回 0-based line 和 character
 */
function findSymbolPosition(content: string, symbol: string): { line: number; character: number } | null {
	const lines = content.split("\n");
	for (let i = 0; i < lines.length; i++) {
		const col = lines[i].indexOf(symbol);
		if (col !== -1) {
			return { line: i, character: col };
		}
	}
	return null;
}

/**
 * 格式化 Location 为可读字符串
 */
function formatLocation(loc: any, baseCwd: string): string {
	const uri = loc.uri || loc.targetUri || "";
	const filePath = uri.replace("file://", "");
	const display = filePath.startsWith(baseCwd) ? relative(baseCwd, filePath) : filePath;
	const range = loc.range || loc.targetRange;
	if (range) {
		const line = range.start.line + 1;
		const col = range.start.character + 1;
		return `${display}:${line}:${col}`;
	}
	return display;
}

/**
 * 格式化诊断信息
 */
function formatDiagnostic(diag: any): string {
	const severity = SEVERITY_MAP[diag.severity] || "Unknown";
	const line = (diag.range?.start?.line ?? 0) + 1;
	const col = (diag.range?.start?.character ?? 0) + 1;
	const source = diag.source ? `[${diag.source}] ` : "";
	const code = diag.code ? ` (${diag.code})` : "";
	return `  L${line}:${col} ${severity}: ${source}${diag.message}${code}`;
}

/**
 * 格式化文档符号（递归，带缩进）
 */
function formatDocumentSymbol(sym: any, indent = 0): string {
	const prefix = "  ".repeat(indent);
	const kind = SYMBOL_KIND_MAP[sym.kind] || `Kind(${sym.kind})`;
	const line = sym.range
		? `:${sym.range.start.line + 1}`
		: sym.location?.range
			? `:${sym.location.range.start.line + 1}`
			: "";
	let result = `${prefix}${kind} ${sym.name}${line}`;
	if (sym.children && sym.children.length > 0) {
		for (const child of sym.children) {
			result += `\n${formatDocumentSymbol(child, indent + 1)}`;
		}
	}
	return result;
}

/**
 * 格式化代码操作
 */
function formatCodeAction(action: any): string {
	const kind = action.kind ? ` [${action.kind}]` : "";
	const disabled = action.disabled ? ` (disabled: ${action.disabled.reason})` : "";
	return `  • ${action.title}${kind}${disabled}`;
}

// ============================================================================
// LSP 工具定义
// ============================================================================

export const lspTool: ToolDefinition<typeof LspParamsSchema> = {
	name: "lsp",
	label: "LSP",
	description:
		"查询语言服务器获取代码智能信息。支持操作：definition（跳转定义）、references（查找引用）、" +
		"hover（悬浮信息）、symbols（文档符号）、diagnostics（文件诊断）、workspace-diagnostics（工作区诊断）、" +
		"signature（函数签名）、rename（重命名预览）、codeAction（可用代码操作）。" +
		"需要提供文件路径，对于位置相关操作需提供 line/column 或 symbol 名称。",
	promptSnippet: "LSP: 查询语言服务器获取定义、引用、悬浮信息、诊断、符号、签名、重命名和代码操作",
	promptGuidelines: [
		"Use the lsp tool for precise code navigation — finding definitions, references, and type info is faster and more accurate than grep for structural queries.",
		"For position-based queries (definition, references, hover, signature, rename, codeAction), provide either line+column (1-based) or a symbol name to auto-locate.",
		"Use workspace-diagnostics to get an overview of all errors/warnings across open files; use diagnostics for a single file.",
		"The rename action shows a preview of edits without applying them — use it to understand impact before making changes.",
	],
	parameters: LspParamsSchema,
	executionMode: "parallel",

	async execute(_toolCallId, params, signal, _onUpdate, ctx) {
		try {
			// 检查 abort 信号
			if (signal?.aborted) {
				return { content: [{ type: "text", text: "操作已取消" }], details: undefined };
			}

			const cwd = ctx.cwd;
			const absolutePath = resolveFilePath(params.file, cwd);

			// 获取/创建 LSPManager 实例
			const manager = LSPManager.getOrCreateManager(cwd);

			// 读取文件内容并通知 LSP 服务器
			const fileContent = safeReadFile(absolutePath);
			if (fileContent === null && params.action !== "workspace-diagnostics") {
				return {
					content: [{ type: "text", text: `错误：无法读取文件 ${absolutePath}` }],
					details: undefined,
				};
			}

			if (fileContent !== null) {
				await manager.touchFile(absolutePath, fileContent);
			}

			// 解析位置（如果需要）
			let position: { line: number; character: number } | undefined;

			if (params.line !== undefined && params.column !== undefined) {
				// 用户提供了 1-based 行列号，转为 0-based
				position = { line: params.line - 1, character: params.column - 1 };
			} else if (params.symbol && fileContent) {
				// 先尝试通过 documentSymbols 精确定位
				const symbols = await manager.getDocumentSymbols(absolutePath);
				const found = findSymbolInTree(symbols, params.symbol);
				if (found) {
					position = found;
				} else {
					// 退而求其次：文本搜索
					position = findSymbolPosition(fileContent, params.symbol) ?? undefined;
				}
				if (!position) {
					return {
						content: [{ type: "text", text: `未找到符号 "${params.symbol}"（在 ${params.file} 中）` }],
						details: undefined,
					};
				}
			}

			// 检查需要位置的操作是否提供了位置
			const positionRequired = ["definition", "references", "hover", "signature", "rename", "codeAction"];
			if (positionRequired.includes(params.action) && !position) {
				return {
					content: [
						{
							type: "text",
							text: `错误：操作 "${params.action}" 需要提供 line+column 或 symbol 参数来定位位置`,
						},
					],
					details: undefined,
				};
			}

			// 再次检查 abort
			if (signal?.aborted) {
				return { content: [{ type: "text", text: "操作已取消" }], details: undefined };
			}

			// 分发到具体操作
			const result = await dispatchAction(manager, params, absolutePath, position, cwd, signal);
			return { content: [{ type: "text", text: result }], details: undefined };
		} catch (error: any) {
			const message = error?.message || String(error);
			return {
				content: [{ type: "text", text: `LSP 错误：${message}` }],
				details: undefined,
			};
		}
	},
};

// ============================================================================
// 操作分发
// ============================================================================

/**
 * 在符号树中递归查找符号名称，返回其位置
 */
function findSymbolInTree(symbols: any[] | null, name: string): { line: number; character: number } | null {
	if (!symbols) return null;
	for (const sym of symbols) {
		if (sym.name === name) {
			const range = sym.selectionRange || sym.range || sym.location?.range;
			if (range) {
				return { line: range.start.line, character: range.start.character };
			}
		}
		// 递归搜索子符号
		if (sym.children) {
			const found = findSymbolInTree(sym.children, name);
			if (found) return found;
		}
	}
	return null;
}

/**
 * 根据 action 类型分发到对应的 LSP 方法
 */
async function dispatchAction(
	manager: LSPManager,
	params: LspToolInput,
	absolutePath: string,
	position: { line: number; character: number } | undefined,
	cwd: string,
	signal: AbortSignal | undefined,
): Promise<string> {
	switch (params.action) {
		// ---- 跳转定义 ----
		case "definition": {
			const locations = await manager.getDefinition(absolutePath, position!.line, position!.character, signal);
			if (!locations || locations.length === 0) {
				return "未找到定义";
			}
			const lines = locations.map((loc: any) => formatLocation(loc, cwd));
			return `定义位置（${locations.length} 处）：\n${lines.join("\n")}`;
		}

		// ---- 查找引用 ----
		case "references": {
			const refs = await manager.getReferences(absolutePath, position!.line, position!.character, signal);
			if (!refs || refs.length === 0) {
				return "未找到引用";
			}
			const lines = refs.map((loc: any) => formatLocation(loc, cwd));
			return `引用位置（${refs.length} 处）：\n${lines.join("\n")}`;
		}

		// ---- 悬浮信息 ----
		case "hover": {
			const hoverResult = await manager.getHover(absolutePath, position!.line, position!.character, signal);
			if (!hoverResult) {
				return "无悬浮信息";
			}
			return hoverResult;
		}

		// ---- 文档符号 ----
		case "symbols": {
			const symbols = await manager.getDocumentSymbols(absolutePath);
			if (!symbols || symbols.length === 0) {
				return "未找到符号";
			}
			const lines = symbols.map((sym: any) => formatDocumentSymbol(sym));
			return `文档符号（${symbols.length} 个顶层）：\n${lines.join("\n")}`;
		}

		// ---- 文件诊断 ----
		case "diagnostics": {
			const diagnostics = await manager.getDiagnostics(absolutePath);
			if (!diagnostics || diagnostics.length === 0) {
				return "无诊断信息（文件无错误/警告）";
			}
			const lines = diagnostics.map(formatDiagnostic);
			return `诊断信息（${diagnostics.length} 条）：\n${lines.join("\n")}`;
		}

		// ---- 工作区诊断 ----
		case "workspace-diagnostics": {
			// 对当前文件调用 getDiagnostics 作为"工作区"诊断的近似
			const diagnostics = await manager.getDiagnostics(absolutePath, signal);
			if (!diagnostics || diagnostics.length === 0) {
				return "工作区无诊断信息";
			}
			const lines = diagnostics.map(formatDiagnostic);
			return `诊断信息（${diagnostics.length} 条）：\n${lines.join("\n")}`;
		}

		// ---- 函数签名 ----
		case "signature": {
			const sigResult = await manager.getSignatureHelp(absolutePath, position!.line, position!.character, signal);
			if (!sigResult) {
				return "无签名信息";
			}
			return sigResult;
		}

		// ---- 重命名预览 ----
		case "rename": {
			if (!params.newName) {
				return "错误：rename 操作需要提供 newName 参数";
			}
			const edits = await manager.rename(absolutePath, position!.line, position!.character, params.newName, signal);
			if (!edits || !edits.changes || Object.keys(edits.changes).length === 0) {
				if (edits?.documentChanges && edits.documentChanges.length > 0) {
					const lines = edits.documentChanges.map((dc: any) => {
						const uri = dc.textDocument?.uri || "";
						const fPath = uri.replace("file://", "");
						const editCount = dc.edits?.length || 0;
						return `  ${fPath}: ${editCount} 处修改`;
					});
					return `重命名预览（"${params.newName}"）：\n${lines.join("\n")}`;
				}
				return "无法执行重命名（可能不支持或位置无效）";
			}
			const lines: string[] = [];
			for (const [uri, fileEdits] of Object.entries(edits.changes) as [string, any[]][]) {
				const fPath = uri.replace("file://", "");
				lines.push(`  ${fPath}: ${fileEdits.length} 处修改`);
			}
			return `重命名预览（"${params.newName}"）：\n${lines.join("\n")}`;
		}

		// ---- 代码操作 ----
		case "codeAction": {
			const actions = await manager.getCodeActions(absolutePath, position!.line, position!.character, signal);
			if (!actions || actions.length === 0) {
				return "无可用代码操作";
			}
			const lines = actions.map(formatCodeAction);
			return `可用代码操作（${actions.length} 个）：\n${lines.join("\n")}`;
		}

		default:
			return `未知操作：${params.action}`;
	}
}
