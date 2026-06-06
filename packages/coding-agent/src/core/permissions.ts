import { appendFileSync, mkdirSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import type { AgentToolResult } from "@earendil-works/pi-agent-core";
import type { Model, TextContent } from "@earendil-works/pi-model-types";
import type { ModelRegistry } from "./model-registry.ts";
import { completeResolvedModel } from "./model-stream.ts";
import type { SettingsManager } from "./settings-manager.ts";

export type PermissionMode = "on-request" | "auto-permission" | "free-permission";
export type PermissionRiskLevel = "read" | "write" | "shell" | "network" | "git" | "destructive" | "unknown";
export type PermissionDecisionValue = "approve" | "reject";
export type PermissionDecisionSource = "whitelist" | "blacklist" | "user" | "reviewer" | "free-permission";
export type UserPermissionChoice = "approve_once" | "approve_session" | "reject" | "reject_alternative";

export interface PermissionRuleSettings {
	toolNames?: string[];
	toolPrefixes?: string[];
	commandExact?: string[];
	commandPrefixes?: string[];
	commandPatterns?: string[];
	pathScopes?: string[];
	pathPatterns?: string[];
	riskLevels?: PermissionRiskLevel[];
}

export interface PermissionSettings {
	whitelist?: PermissionRuleSettings;
	blacklist?: PermissionRuleSettings;
}

export interface AutoPermissionReviewerSettings {
	enabled?: boolean;
	provider?: string;
	model?: string;
	temperature?: number;
	maxTokens?: number;
}

export interface PermissionRequest {
	toolName: string;
	toolCallId?: string;
	args: unknown;
	command?: string;
	riskLevel?: PermissionRiskLevel;
	affectedPaths?: string[];
	reason?: string;
	workspaceRoot: string;
	cwd: string;
	sessionId: string;
	turnId?: string;
	gitBranch?: string;
	currentTaskSummary?: string;
	recentContextSummary?: string;
	/** 来源标识：如 "subagent-1 (researcher)" */
	source?: string;
}

export interface PermissionDecision {
	decision: PermissionDecisionValue;
	riskLevel: PermissionRiskLevel;
	source: PermissionDecisionSource;
	reason: string;
	saferAlternative?: string;
	userChoice?: UserPermissionChoice;
	/** approve_session 时的 trust 匹配 key */
	trustKey?: string;
	reviewerModel?: string;
	reviewerReason?: string;
	ruleReason?: string;
	isWorkspaceScoped: boolean;
	mode: PermissionMode;
}

export type ReviewerDecision = "approve" | "reject" | "uncertain";

export interface PermissionReviewResult {
	decision: ReviewerDecision;
	risk_level: PermissionRiskLevel;
	is_workspace_scoped: boolean;
	reason: string;
	safer_alternative?: string;
}

export interface AutoPermissionReviewer {
	review(request: PermissionRequest, mode: PermissionMode): Promise<PermissionReviewResult>;
	getReviewerModel?(): string | undefined;
}

export interface UserPermissionPrompt {
	request(
		request: PermissionRequest,
		decisionContext: PermissionPromptContext,
	): Promise<{
		choice: UserPermissionChoice;
		reason?: string;
		/** approve_session 时用户选择的 trust key（匹配模式） */
		trustKey?: string;
	}>;
}

export interface PermissionPromptContext {
	mode: PermissionMode;
	riskLevel: PermissionRiskLevel;
	argumentsPreview: string;
	affectedPaths: string[];
	workspaceRoot: string;
	ruleReason?: string;
}

export interface PermissionAuditEntry {
	timestamp: string;
	session_id: string;
	turn_id?: string;
	permission_mode: PermissionMode;
	tool_name: string;
	command?: string;
	arguments_preview: string;
	risk_level: PermissionRiskLevel;
	affected_paths: string[];
	decision: PermissionDecisionValue;
	decision_source: PermissionDecisionSource;
	reviewer_model?: string;
	reviewer_reason?: string;
	user_choice?: UserPermissionChoice;
	final_status: string;
	error_message?: string;
}

/** 会话内权限决策历史记录（内存中，用于 /permissions 命令展示） */
export interface PermissionHistoryEntry {
	timestamp: string;
	toolName: string;
	command?: string;
	decision: PermissionDecisionValue;
	source: PermissionDecisionSource;
	userChoice?: UserPermissionChoice;
	trustKey?: string;
}

const VALID_PERMISSION_MODES = new Set<PermissionMode>(["on-request", "auto-permission", "free-permission"]);
const SECRET_KEY_RE = /(api[_-]?key|token|secret|password|credential|authorization|id_rsa|private[_-]?key)/i;
const SECRET_FILE_RE =
	/(^|[/\\])(\.env|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|.*secret.*|.*token.*)(\.|$|[/\\])/i;

// workspace 内无条件放行的只读工具（内置工具，非 bash 命令）
const WORKSPACE_READ_TOOLS = new Set(["read", "grep", "find", "ls"]);

// 绝对禁止的硬核黑名单 — 不可被用户覆盖
const HARDCODED_BLACKLIST: PermissionRuleSettings = {
	commandPatterns: [
		String.raw`\brm\s+-[^\n;]*r[^\n;]*f[^\n;]*(/|~|\$HOME|\.|\*)(\b|$)`,
		// rm 后跟通配符或目录（不带 -rf 也危险）
		String.raw`\brm\s+[^\n;]*\*`,
		String.raw`\brm\s+-[^\n;]*\s+\.(?:\b|$|/)`,
		String.raw`^\s*sudo\b`,
		String.raw`\bgit\s+reset\s+--hard\b`,
		String.raw`\bgit\s+clean\s+-fd\b`,
		String.raw`\bgit\s+push\b[^\n;]*(--force|-f)\b`,
		String.raw`\bdd\b`,
		String.raw`\bmkfs\b`,
		String.raw`\bdiskutil\s+erase\b`,
	],
};

// 出厂默认规则 — 初始化时写到 user settings，用户可修改
export const DEFAULT_USER_WHITELIST: PermissionRuleSettings = {
	toolNames: ["subagent"],
	commandPrefixes: [
		// 纯信息查询
		"ls",
		"pwd",
		"which",
		"type",
		// 只读文件操作（敏感文件已被黑名单拦截）
		"cat",
		"head",
		"tail",
		"wc",
		"sort",
		"diff",
		"echo",
		// 搜索
		"grep",
		"find",
		// 只读 git 操作
		"git status",
		"git log",
		"git diff",
		"git branch",
		"git show",
	],
};

export const DEFAULT_USER_BLACKLIST: PermissionRuleSettings = {
	commandPatterns: [
		String.raw`\bchmod\s+-R\b`,
		String.raw`\bchown\s+-R\b`,
		String.raw`\bpowershell\b[^\n;]*Remove-Item\b[^\n;]*-Recurse\b[^\n;]*-Force\b`,
		// printenv/env/set 不跟变量名参数时视为全量 dump（允许 printenv HOME、env VAR=val cmd）
		String.raw`^\s*(printenv|env|set)\b(?!\s+[A-Za-z_])`,
		String.raw`\b(cat|less|more|head|tail|open|pbcopy)\b[^\n;]*(\.env|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|secrets?)\b`,
	],
	pathPatterns: [
		String.raw`(^|[/\\])\.git($|[/\\])`,
		String.raw`(^|[/\\])(\.env|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|secrets?)($|[/\\]|\.)`,
	],
};

function normalizeMode(value: string | undefined): PermissionMode | undefined {
	return value && VALID_PERMISSION_MODES.has(value as PermissionMode) ? (value as PermissionMode) : undefined;
}

export function parsePermissionMode(value: string | undefined): PermissionMode | undefined {
	return normalizeMode(value);
}

export function isValidPermissionMode(value: string): value is PermissionMode {
	return VALID_PERMISSION_MODES.has(value as PermissionMode);
}

export function redactText(value: string, limit = 800): string {
	const compact = value.replace(/\s+/g, " ").trim();
	const redacted = compact
		.replace(
			/(api[_-]?key|token|secret|password|authorization|credential)(["'\s:=]+)([^"',\s}]+)/gi,
			"$1$2[REDACTED]",
		)
		.replace(/(Bearer\s+)[A-Za-z0-9._~+/=-]+/gi, "$1[REDACTED]");
	return redacted.length > limit ? `${redacted.slice(0, limit)}...` : redacted;
}

export function previewArguments(args: unknown): string {
	return redactText(JSON.stringify(redactUnknown(args), null, 0) ?? "");
}

function redactUnknown(value: unknown): unknown {
	if (Array.isArray(value)) {
		return value.map((item) => redactUnknown(item));
	}
	if (!value || typeof value !== "object") {
		if (typeof value === "string" && SECRET_KEY_RE.test(value)) return "[REDACTED]";
		return value;
	}
	const result: Record<string, unknown> = {};
	for (const [key, entry] of Object.entries(value)) {
		result[key] = SECRET_KEY_RE.test(key) ? "[REDACTED]" : redactUnknown(entry);
	}
	return result;
}

export function isPathInside(path: string, root: string): boolean {
	const rel = relative(resolve(root), resolve(path));
	return rel === "" || (rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel));
}

function resolveMaybePath(pathValue: string, cwd: string): string {
	return isAbsolute(pathValue) ? resolve(pathValue) : resolve(cwd, pathValue);
}

function extractStringPaths(args: unknown, cwd: string): string[] {
	if (!args || typeof args !== "object") return [];
	const record = args as Record<string, unknown>;
	const candidates = [record.path, record.file_path, record.filePath, record.cwd, record.dir, record.directory];
	return candidates
		.filter((value): value is string => typeof value === "string")
		.map((value) => resolveMaybePath(value, cwd));
}

/**
 * 从 shell 命令中提取写入目标路径：
 * - 重定向（>, >>, 2>, &>）
 * - tee
 * - cp/mv/ln/install 的目标参数（最后一个非 flag 参数）
 */
function extractRedirectPaths(command: string, cwd: string): string[] {
	const paths: string[] = [];
	// >, >>, 2>, 2>>, &>, &>> 后面的路径
	const redirectRe = /(?:^|[^\\])(?:>>?|2>>?|&>>?)\s*([^\s;|&><"']+|"[^"]*"|'[^']*')/g;
	for (const m of command.matchAll(redirectRe)) {
		const target = m[1].replace(/^["']|["']$/g, "");
		if (target && !target.startsWith("/dev/")) {
			paths.push(resolveMaybePath(target, cwd));
		}
	}
	// tee [-a] file
	const teeRe = /\btee\s+(?:-[a-zA-Z]*\s+)*([^\s;|&><"']+|"[^"]*"|'[^']*')/g;
	for (const m of command.matchAll(teeRe)) {
		const target = m[1].replace(/^["']|["']$/g, "");
		if (target && !target.startsWith("-") && !target.startsWith("/dev/")) {
			paths.push(resolveMaybePath(target, cwd));
		}
	}
	// cp/mv/ln/install — 目标是最后一个非 flag 参数
	const segments = splitShellSegments(firstEffectiveLine(command));
	for (const seg of segments) {
		const fileCmdMatch = seg.match(/^\s*(cp|mv|ln|install)\b(.*)$/);
		if (fileCmdMatch) {
			const argsStr = fileCmdMatch[2].trim();
			// 提取所有非 flag 参数（不以 - 开头的 token，排除重定向）
			const tokens = argsStr.match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];
			const nonFlags = tokens
				.filter((t) => !t.startsWith("-") && !t.startsWith(">") && !t.startsWith("&>"))
				.map((t) => t.replace(/^["']|["']$/g, ""));
			// 最后一个非 flag 参数就是目标路径
			if (nonFlags.length >= 2) {
				const target = nonFlags[nonFlags.length - 1];
				if (target) paths.push(resolveMaybePath(target, cwd));
			}
		}
	}
	return paths;
}

function commandStartsWith(command: string, prefix: string): boolean {
	return command.trim().startsWith(prefix.trim());
}

function matchesPattern(value: string, pattern: string): boolean {
	try {
		return new RegExp(pattern, "i").test(value);
	} catch {
		return value.includes(pattern);
	}
}

function mergeRules(
	base: PermissionRuleSettings | undefined,
	extra: PermissionRuleSettings | undefined,
): PermissionRuleSettings {
	return {
		toolNames: [...(base?.toolNames ?? []), ...(extra?.toolNames ?? [])],
		toolPrefixes: [...(base?.toolPrefixes ?? []), ...(extra?.toolPrefixes ?? [])],
		commandExact: [...(base?.commandExact ?? []), ...(extra?.commandExact ?? [])],
		commandPrefixes: [...(base?.commandPrefixes ?? []), ...(extra?.commandPrefixes ?? [])],
		commandPatterns: [...(base?.commandPatterns ?? []), ...(extra?.commandPatterns ?? [])],
		pathScopes: [...(base?.pathScopes ?? []), ...(extra?.pathScopes ?? [])],
		pathPatterns: [...(base?.pathPatterns ?? []), ...(extra?.pathPatterns ?? [])],
		riskLevels: [...(base?.riskLevels ?? []), ...(extra?.riskLevels ?? [])],
	};
}

export function inferPermissionRequest(input: PermissionRequest): PermissionRequest {
	const argPaths = input.affectedPaths ?? extractStringPaths(input.args, input.cwd);
	// shell 命令额外提取重定向目标路径
	const redirectPaths = input.command ? extractRedirectPaths(input.command, input.cwd) : [];
	const affectedPaths = [...argPaths, ...redirectPaths];
	return {
		...input,
		affectedPaths,
		riskLevel:
			input.riskLevel ??
			inferRiskLevel(input.toolName, input.args, input.command, affectedPaths, input.workspaceRoot),
	};
}

export function inferRiskLevel(
	toolName: string,
	args: unknown,
	command: string | undefined,
	affectedPaths: string[],
	workspaceRoot: string,
): PermissionRiskLevel {
	if (affectedPaths.some((pathValue) => !isPathInside(pathValue, workspaceRoot) || SECRET_FILE_RE.test(pathValue))) {
		return "destructive";
	}
	if (command) {
		const commandText = command.trim();
		if (ruleMatches({ command: commandText, affectedPaths, riskLevel: "shell", toolName }, HARDCODED_BLACKLIST)) {
			return "destructive";
		}
		if (/^\s*git\b/.test(commandText)) return "git";
		if (
			/\b(curl|wget|ssh|scp|rsync|npm\s+(install|publish)|pnpm\s+(install|publish)|yarn\s+(add|publish)|bun\s+(install|publish))\b/i.test(
				commandText,
			)
		) {
			return "network";
		}
		return "shell";
	}
	if (toolName === "read" || toolName === "grep" || toolName === "find" || toolName === "ls") return "read";
	if (toolName === "write" || toolName === "edit") return "write";
	if (toolName === "bash") {
		const maybeCommand =
			typeof (args as { command?: unknown })?.command === "string" ? (args as { command: string }).command : "";
		if (maybeCommand) {
			return inferRiskLevel(toolName, args, maybeCommand, affectedPaths, workspaceRoot);
		}
		return "shell";
	}
	if (toolName === "subagent") return "unknown";
	return "unknown";
}

function ruleMatches(
	request: {
		toolName: string;
		command?: string;
		affectedPaths: string[];
		riskLevel: PermissionRiskLevel;
	},
	rules: PermissionRuleSettings | undefined,
): string | undefined {
	if (!rules) return undefined;
	if (rules.toolNames?.includes(request.toolName)) return `tool exact match: ${request.toolName}`;
	if (rules.toolPrefixes?.some((prefix) => request.toolName.startsWith(prefix)))
		return `tool prefix match: ${request.toolName}`;
	if (request.command && rules.commandExact?.includes(request.command.trim()))
		return `command exact match: ${request.command.trim()}`;
	if (request.command && rules.commandPrefixes?.some((prefix) => commandStartsWith(request.command!, prefix))) {
		return `command prefix match: ${request.command.trim()}`;
	}
	if (request.command && rules.commandPatterns?.some((pattern) => matchesPattern(request.command!, pattern))) {
		return `command pattern match: ${request.command.trim()}`;
	}
	if (rules.riskLevels?.includes(request.riskLevel)) return `risk level match: ${request.riskLevel}`;
	if (
		rules.pathScopes?.some((scope) =>
			request.affectedPaths.some((pathValue) => isPathInside(pathValue, resolveMaybePath(scope, process.cwd()))),
		)
	) {
		return "path scope match";
	}
	if (
		rules.pathPatterns?.some((pattern) =>
			request.affectedPaths.some((pathValue) => matchesPattern(pathValue, pattern)),
		)
	) {
		return "path pattern match";
	}
	return undefined;
}

/**
 * 提取命令的第一行有效内容（跳过 # 注释行和空行）。
 */
function firstEffectiveLine(command: string): string {
	for (const line of command.split("\n")) {
		const trimmed = line.trim();
		if (trimmed && !trimmed.startsWith("#")) return trimmed;
	}
	return command.split("\n")[0].trim();
}

/**
 * 将 shell 命令按 `|`、`&&`、`||` 拆分为独立段。
 * 不拆分引号/转义内的运算符。返回各段 trim 后的字符串。
 */
export function splitShellSegments(command: string): string[] {
	const segments: string[] = [];
	let current = "";
	let i = 0;
	let inSingle = false;
	let inDouble = false;

	while (i < command.length) {
		const ch = command[i];

		// 转义字符：跳过下一个
		if (ch === "\\" && !inSingle && i + 1 < command.length) {
			current += ch + command[i + 1];
			i += 2;
			continue;
		}

		// 引号状态切换
		if (ch === "'" && !inDouble) {
			inSingle = !inSingle;
			current += ch;
			i++;
			continue;
		}
		if (ch === '"' && !inSingle) {
			inDouble = !inDouble;
			current += ch;
			i++;
			continue;
		}

		// 不在引号内才识别运算符
		if (!inSingle && !inDouble) {
			if (ch === "|" && command[i + 1] !== "|") {
				// 管道 |
				segments.push(current);
				current = "";
				i++;
				continue;
			}
			if (ch === "|" && command[i + 1] === "|") {
				// ||
				segments.push(current);
				current = "";
				i += 2;
				continue;
			}
			if (ch === "&" && command[i + 1] === "&") {
				// &&
				segments.push(current);
				current = "";
				i += 2;
				continue;
			}
			if (ch === ";") {
				// 分号分隔
				segments.push(current);
				current = "";
				i++;
				continue;
			}
		}

		current += ch;
		i++;
	}
	segments.push(current);

	return segments.map((s) => s.trim()).filter((s) => s.length > 0);
}

/**
 * 生成多层级的 trust 匹配 key。
 * 如 bash 执行 "uv run a.py"：
 *   level 0: "bash:uv run a.py"  (精确匹配完整首行)
 *   level 1: "bash:uv run"       (命令+子命令)
 *   level 2: "bash:uv"           (仅程序名)
 *
 * 对于含 | 或 && 的复合命令，额外生成各段的信任选项：
 *   "grep -n foo | python a.py" →
 *     level 0: 完整命令
 *     segment: "bash:grep -n foo"  (信任管道左侧)
 *     segment: "bash:python a.py"  (信任管道右侧)
 *     ... 以及各段的前缀缩短
 *
 * 对于 edit/read/write 等工具，用 affectedPaths 生成：
 *   level 0: "edit:/full/path"   (精确文件)
 *   level 1: "edit:/dir/"        (目录前缀)
 */
export function generateTrustLevels(request: PermissionRequest): { label: string; key: string }[] {
	const tool = request.toolName;
	const command = request.command?.trim();

	if (command) {
		const firstLine = firstEffectiveLine(command);
		const levels: { label: string; key: string }[] = [];
		const seen = new Set<string>();

		// 精确匹配完整首行
		levels.push({ label: firstLine, key: `${tool}:${firstLine}` });
		seen.add(`${tool}:${firstLine}`);

		// 检测是否为复合命令
		const segments = splitShellSegments(firstLine);
		const isCompound = segments.length > 1;

		if (isCompound) {
			// 对每个段生成独立的信任选项
			for (const seg of segments) {
				const segParts = seg.split(/\s+/);
				// 段的完整命令
				const segKey = `${tool}:${seg}`;
				if (!seen.has(segKey)) {
					levels.push({ label: seg, key: segKey });
					seen.add(segKey);
				}
				// 段的逐级缩短前缀
				for (let i = segParts.length - 1; i >= 1; i--) {
					const prefix = segParts.slice(0, i).join(" ");
					const prefixKey = `${tool}:${prefix}`;
					if (!seen.has(prefixKey)) {
						levels.push({ label: `${prefix} *`, key: prefixKey });
						seen.add(prefixKey);
					}
				}
			}
		} else {
			// 单条命令：逐级缩短前缀
			const parts = firstLine.split(/\s+/);
			for (let i = parts.length - 1; i >= 1; i--) {
				const prefix = parts.slice(0, i).join(" ");
				const prefixKey = `${tool}:${prefix}`;
				if (!seen.has(prefixKey)) {
					levels.push({ label: `${prefix} *`, key: prefixKey });
					seen.add(prefixKey);
				}
			}
		}

		return levels;
	}

	// 无 command 的工具（edit/read/write）用路径
	const paths = request.affectedPaths ?? [];
	const levels: { label: string; key: string }[] = [];
	const cwd = request.workspaceRoot.endsWith("/") ? request.workspaceRoot : `${request.workspaceRoot}/`;

	// 显示用：workspace 内路径用相对路径，外部用绝对路径
	const displayPath = (p: string) => (p.startsWith(cwd) ? p.slice(cwd.length) : p);

	if (paths.length === 1) {
		levels.push({ label: displayPath(paths[0]), key: `${tool}:${paths[0]}` });
		// 逐级向上取目录（不超过 workspace root）
		let dir = paths[0].replace(/\/[^/]+$/, "/");
		const seen = new Set<string>();
		while (dir.length >= cwd.length && dir !== paths[0] && !seen.has(dir)) {
			seen.add(dir);
			levels.push({ label: `${displayPath(dir)}*`, key: `${tool}:${dir}` });
			dir = dir.slice(0, -1).replace(/\/[^/]+$/, "/");
		}
	}

	return levels;
}

function matchesSingleCommand(approvals: Set<string>, tool: string, cmd: string): boolean {
	const trimmed = cmd.trim();
	if (!trimmed) return false;
	// 精确匹配
	if (approvals.has(`${tool}:${trimmed}`)) return true;
	// 前缀匹配
	const parts = trimmed.split(/\s+/);
	for (let i = parts.length - 1; i >= 1; i--) {
		const prefix = parts.slice(0, i).join(" ");
		if (approvals.has(`${tool}:${prefix}`)) return true;
	}
	return false;
}

/**
 * 对复合命令的每个段独立做黑名单匹配。
 * 解决 "env | grep key" 绕过 `\b(env)\b\s*$` 的问题 —— 拆开后 "env" 单独检测。
 */
function checkSegmentBlacklist(
	command: string,
	toolName: string,
	affectedPaths: string[],
	riskLevel: PermissionRiskLevel,
	blacklist: PermissionRuleSettings,
): string | undefined {
	const segments = splitShellSegments(firstEffectiveLine(command));
	if (segments.length <= 1) return undefined;
	for (const seg of segments) {
		const reason = ruleMatches({ toolName, command: seg, affectedPaths, riskLevel }, blacklist);
		if (reason) return `segment "${seg}" blocked: ${reason}`;
	}
	return undefined;
}

/**
 * 提取 $(...) 和 `...` 内的子命令（仅顶层，不递归嵌套）。
 */
function extractSubcommands(command: string): string[] {
	const results: string[] = [];
	// $(...) — 简单括号平衡
	let i = 0;
	while (i < command.length) {
		if (command[i] === "$" && command[i + 1] === "(") {
			let depth = 1;
			let j = i + 2;
			while (j < command.length && depth > 0) {
				if (command[j] === "(") depth++;
				else if (command[j] === ")") depth--;
				j++;
			}
			if (depth === 0) {
				results.push(command.slice(i + 2, j - 1));
			}
			i = j;
		} else {
			i++;
		}
	}
	// `...` — 反引号
	const backtickRe = /`([^`]+)`/g;
	for (const m of command.matchAll(backtickRe)) {
		results.push(m[1]);
	}
	return results;
}

// 敏感文件名（用于变量间接引用检测）
const SENSITIVE_FILENAMES_RE = /\.(env|dev\.vars)|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|secrets?/i;
// 文件读取命令
const READ_COMMANDS_RE = /\b(cat|less|more|head|tail|open|pbcopy|source|\.)\b/;
// 变量使用（$VAR 或 ${VAR}）
const VAR_USAGE_RE = /\$[{]?[a-zA-Z_][a-zA-Z0-9_]*[}]?/;

/**
 * 深层命令安全检查 — 检测静态模式匹配无法覆盖的绕过手法：
 * 1. $() 或反引号内嵌的子命令
 * 2. 变量间接引用敏感文件
 * 3. 网络外传 + 环境变量泄露组合
 */
function checkCommandDeep(
	command: string,
	toolName: string,
	affectedPaths: string[],
	riskLevel: PermissionRiskLevel,
	blacklist: PermissionRuleSettings,
): string | undefined {
	// 1. 检查 $() 和反引号内的子命令
	const subs = extractSubcommands(command);
	for (const sub of subs) {
		// 对子命令做完整黑名单检查
		const reason = ruleMatches({ toolName, command: sub.trim(), affectedPaths, riskLevel }, blacklist);
		if (reason) return `subcommand "$(${sub.trim()})" blocked: ${reason}`;
		// 子命令可能也是复合的
		const segments = splitShellSegments(sub.trim());
		for (const seg of segments) {
			const segReason = ruleMatches({ toolName, command: seg, affectedPaths, riskLevel }, blacklist);
			if (segReason) return `subcommand segment "${seg}" blocked: ${segReason}`;
		}
	}

	// 2. 变量间接引用检测：命令里同时含有敏感文件名（在赋值中）和变量读取
	if (SENSITIVE_FILENAMES_RE.test(command) && READ_COMMANDS_RE.test(command) && VAR_USAGE_RE.test(command)) {
		// 检查是否有 "VAR=sensitive; read_cmd $VAR" 模式
		const assignRe =
			/\b([a-zA-Z_][a-zA-Z0-9_]*)=\S*?(\.env|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|secrets?)/i;
		const assignMatch = assignRe.exec(command);
		if (assignMatch) {
			const varName = assignMatch[1];
			// 检查命令后续是否使用了这个变量
			const usageRe = new RegExp(`\\$\\{?${varName}\\}?`);
			if (usageRe.test(command)) {
				return `variable indirection bypass: ${varName} assigned sensitive path "${assignMatch[2]}" then used via $${varName}`;
			}
		}
	}

	// 3. echo/printf 展开敏感环境变量
	const SENSITIVE_VAR_RE = /\$\{?([a-zA-Z_][a-zA-Z0-9_]*)\}?/g;
	const SENSITIVE_VAR_NAMES_RE =
		/^(.*_)?(KEY|TOKEN|SECRET|PASSWORD|PASSWD|PWD|CREDENTIAL|AUTH|PRIVATE|API_KEY|APIKEY)(_.+)?$/i;
	if (/\b(echo|printf)\b/.test(command)) {
		for (const varMatch of command.matchAll(SENSITIVE_VAR_RE)) {
			if (SENSITIVE_VAR_NAMES_RE.test(varMatch[1])) {
				return `sensitive variable leak: echo/printf expands $${varMatch[1]} which looks like a secret`;
			}
		}
	}

	// 4. 网络命令 + 环境变量/敏感数据外传
	const networkCmdRe = /\b(curl|wget|nc|ncat|socat)\b/;
	if (networkCmdRe.test(command)) {
		// 检查是否包含 $(env...) 或 $(printenv...) 或 $(cat .env) 之类的子命令
		for (const sub of subs) {
			if (/\b(env|printenv|set)\b/.test(sub) || SENSITIVE_FILENAMES_RE.test(sub)) {
				return `data exfiltration risk: network command with sensitive subcommand "$(${sub.trim()})"`;
			}
		}
		// 网络命令 + 展开敏感变量
		const netVarRe = /\$\{?([a-zA-Z_][a-zA-Z0-9_]*)\}?/g;
		for (const varMatch of command.matchAll(netVarRe)) {
			if (SENSITIVE_VAR_NAMES_RE.test(varMatch[1])) {
				return `data exfiltration risk: network command references sensitive variable $${varMatch[1]}`;
			}
		}
	}

	return undefined;
}

function inferSaferAlternative(command: string | undefined, reason: string): string {
	if (command && /\brm\b/.test(command)) {
		return "Use `trash` (trash-cli) instead of `rm` for safe, recoverable deletion.";
	}
	if (reason.includes("sensitive variable") || reason.includes("exfiltration")) {
		return "Do not read or transmit secret environment variables. Use a secrets manager or .env file reference instead.";
	}
	return "Choose a workspace-scoped, non-destructive command or tool call.";
}

function matchesSessionApproval(approvals: Set<string>, request: PermissionRequest): boolean {
	const tool = request.toolName;
	const command = request.command?.trim();

	if (command) {
		const firstLine = firstEffectiveLine(command);
		// 精确匹配完整命令
		if (approvals.has(`${tool}:${firstLine}`)) return true;

		// 检查是否为复合命令（&& || |），所有段均已被信任则整体通过
		const segments = splitShellSegments(firstLine);
		if (segments.length > 1) {
			if (segments.every((seg) => matchesSingleCommand(approvals, tool, seg))) {
				return true;
			}
			// 复合命令不走整体前缀匹配（避免第一段前缀误匹配后续段）
			return false;
		}

		// 单条命令的前缀匹配
		if (matchesSingleCommand(approvals, tool, firstLine)) return true;
	} else {
		// 路径匹配
		const paths = request.affectedPaths ?? [];
		for (const p of paths) {
			if (approvals.has(`${tool}:${p}`)) return true;
			// 前缀目录匹配
			for (const key of approvals) {
				if (key.startsWith(`${tool}:`) && key.endsWith("/") && p.startsWith(key.slice(tool.length + 1))) {
					return true;
				}
			}
		}
	}

	return false;
}

export class PermissionAuditLogger {
	private readonly filePath: string;

	constructor(workspaceRoot: string, sessionId: string) {
		this.filePath = join(workspaceRoot, ".rozsa-agent", "sessions", `${sessionId}.jsonl`);
	}

	get path(): string {
		return this.filePath;
	}

	write(entry: PermissionAuditEntry): void {
		mkdirSync(dirname(this.filePath), { recursive: true });
		appendFileSync(this.filePath, `${JSON.stringify(entry)}\n`, "utf-8");
	}
}

export class PermissionDeniedError extends Error {
	readonly decision: PermissionDecision;

	constructor(decision: PermissionDecision) {
		const suffix = decision.saferAlternative ? ` Safer alternative: ${decision.saferAlternative}` : "";
		super(
			`permission denied by ${decision.source}: ${decision.reason}.${suffix} Ask the agent to choose another safe approach.`,
		);
		this.name = "PermissionDeniedError";
		this.decision = decision;
	}
}

export class PermissionManager {
	private readonly workspaceRoot: string;
	private mode: PermissionMode;
	private readonly settings: PermissionSettings;
	private reviewer?: AutoPermissionReviewer;
	private readonly userPrompt?: UserPermissionPrompt;
	private readonly auditLogger: PermissionAuditLogger;
	private readonly settingsManager?: SettingsManager;
	private readonly sessionApprovals = new Set<string>();
	private lastDecision?: PermissionDecision;
	/** 会话内权限决策历史（内存） */
	private readonly _permissionHistory: PermissionHistoryEntry[] = [];

	constructor(options: {
		mode: PermissionMode;
		workspaceRoot: string;
		settings?: PermissionSettings;
		reviewer?: AutoPermissionReviewer;
		userPrompt?: UserPermissionPrompt;
		auditLogger: PermissionAuditLogger;
		settingsManager?: SettingsManager;
	}) {
		this.mode = options.mode;
		this.workspaceRoot = options.workspaceRoot;
		this.settings = options.settings ?? {};
		this.reviewer = options.reviewer;
		this.userPrompt = options.userPrompt;
		this.auditLogger = options.auditLogger;
		this.settingsManager = options.settingsManager;
	}

	getMode(): PermissionMode {
		return this.mode;
	}

	setMode(mode: PermissionMode): void {
		this.mode = mode;
	}

	setReviewer(reviewer: AutoPermissionReviewer | undefined): void {
		this.reviewer = reviewer;
	}

	hasReviewer(): boolean {
		return this.reviewer !== undefined;
	}

	getSessionApprovalCount(): number {
		return this.sessionApprovals.size;
	}

	getLastDecision(): PermissionDecision | undefined {
		return this.lastDecision;
	}

	/** 获取会话内所有权限决策历史 */
	getPermissionHistory(): ReadonlyArray<PermissionHistoryEntry> {
		return this._permissionHistory;
	}

	getSummaries(): { whitelistSummary: string; blacklistSummary: string } {
		return {
			whitelistSummary: summarizeRules(this.settings.whitelist),
			blacklistSummary: summarizeRules(mergeRules(HARDCODED_BLACKLIST, this.settings.blacklist)),
		};
	}

	async check(rawRequest: PermissionRequest): Promise<PermissionDecision> {
		const request = inferPermissionRequest(rawRequest);
		const riskLevel = request.riskLevel ?? "unknown";
		const affectedPaths = request.affectedPaths ?? [];
		const isWorkspaceScoped = affectedPaths.every((pathValue) => isPathInside(pathValue, this.workspaceRoot));
		const argumentsPreview = previewArguments(request.args);
		const blacklist = mergeRules(HARDCODED_BLACKLIST, this.settings.blacklist);
		const blacklistReason =
			ruleMatches({ toolName: request.toolName, command: request.command, affectedPaths, riskLevel }, blacklist) ||
			// 复合命令逐段检查黑名单 — 防止 env | grep 这类绕过
			(request.command
				? checkSegmentBlacklist(request.command, request.toolName, affectedPaths, riskLevel, blacklist)
				: undefined) ||
			// 深层检查：$() 子命令、变量间接引用、网络外传
			(request.command
				? checkCommandDeep(request.command, request.toolName, affectedPaths, riskLevel, blacklist)
				: undefined) ||
			(!isWorkspaceScoped ? "path outside workspace" : undefined);
		const whitelist = this.settings.whitelist;
		const whitelistReason = ruleMatches(
			{ toolName: request.toolName, command: request.command, affectedPaths, riskLevel },
			whitelist,
		);

		let decision: PermissionDecision;
		if (blacklistReason) {
			decision = {
				decision: "reject",
				riskLevel,
				source: "blacklist",
				reason: blacklistReason,
				isWorkspaceScoped,
				mode: this.mode,
				ruleReason: blacklistReason,
				saferAlternative: inferSaferAlternative(request.command, blacklistReason),
			};
		} else if (WORKSPACE_READ_TOOLS.has(request.toolName) && isWorkspaceScoped) {
			decision = {
				decision: "approve",
				riskLevel,
				source: "whitelist",
				reason: "workspace-scoped read tool",
				isWorkspaceScoped,
				mode: this.mode,
			};
		} else if (this.mode === "free-permission") {
			decision = {
				decision: "approve",
				riskLevel,
				source: "free-permission",
				reason: "free-permission mode approves tool and command calls",
				isWorkspaceScoped,
				mode: this.mode,
			};
		} else if (whitelistReason || matchesSessionApproval(this.sessionApprovals, request)) {
			decision = {
				decision: "approve",
				riskLevel,
				source: "whitelist",
				reason: whitelistReason ?? "session approval rule matched",
				isWorkspaceScoped,
				mode: this.mode,
				ruleReason: whitelistReason,
			};
		} else if (this.mode === "auto-permission") {
			decision = await this.checkWithReviewer(request, riskLevel, isWorkspaceScoped);
		} else {
			decision = await this.checkWithUser(request, riskLevel, isWorkspaceScoped, argumentsPreview);
		}

		if (decision.userChoice === "approve_session" && decision.trustKey) {
			this.sessionApprovals.add(decision.trustKey);
			// 持久化到 project settings，下次会话不再重复询问
			this.settingsManager?.addTrustedCommand(decision.trustKey);
		}
		this.lastDecision = decision;

		// 记录到内存历史（供 /permissions 命令使用）
		this._permissionHistory.push({
			timestamp: new Date().toISOString(),
			toolName: request.toolName,
			command: request.command ? redactText(request.command, 120) : undefined,
			decision: decision.decision,
			source: decision.source,
			userChoice: decision.userChoice,
			trustKey: decision.trustKey,
		});

		this.auditLogger.write({
			timestamp: new Date().toISOString(),
			session_id: request.sessionId,
			turn_id: request.turnId,
			permission_mode: this.mode,
			tool_name: request.toolName,
			command: request.command ? redactText(request.command) : undefined,
			arguments_preview: argumentsPreview,
			risk_level: riskLevel,
			affected_paths: affectedPaths.map((pathValue) => redactText(pathValue, 300)),
			decision: decision.decision,
			decision_source: decision.source,
			reviewer_model: decision.reviewerModel,
			reviewer_reason: decision.reviewerReason,
			user_choice: decision.userChoice,
			final_status: decision.decision === "approve" ? "approved" : "rejected",
		});
		return decision;
	}

	private async checkWithReviewer(
		request: PermissionRequest,
		riskLevel: PermissionRiskLevel,
		isWorkspaceScoped: boolean,
	): Promise<PermissionDecision> {
		if (!this.reviewer) {
			return this.checkWithUser(request, riskLevel, isWorkspaceScoped, previewArguments(request.args), {
				ruleReason: "auto-permission reviewer is not configured; falling back to on-request",
			});
		}

		try {
			const result = await this.reviewer.review(request, this.mode);

			// 明确 approve 且在 workspace 内：自动批准
			if (result.decision === "approve" && result.is_workspace_scoped !== false) {
				return {
					decision: "approve",
					riskLevel: result.risk_level ?? riskLevel,
					source: "reviewer",
					reason: result.reason,
					isWorkspaceScoped: result.is_workspace_scoped,
					mode: this.mode,
					reviewerModel: this.reviewer.getReviewerModel?.(),
					reviewerReason: result.reason,
				};
			}

			// 明确 reject 且有理由：自动拒绝
			if (result.decision === "reject" && result.reason) {
				return {
					decision: "reject",
					riskLevel: result.risk_level ?? riskLevel,
					source: "reviewer",
					reason: result.reason,
					saferAlternative: result.safer_alternative,
					isWorkspaceScoped: result.is_workspace_scoped,
					mode: this.mode,
					reviewerModel: this.reviewer.getReviewerModel?.(),
					reviewerReason: result.reason,
				};
			}

			// 不确定（uncertain、workspace 外等）：交给用户确认，无用户则拒绝
			if (this.userPrompt) {
				return this.checkWithUser(
					request,
					result.risk_level ?? riskLevel,
					isWorkspaceScoped,
					previewArguments(request.args),
					{
						ruleReason: `reviewer uncertain: ${result.reason || "no clear decision"}`,
					},
				);
			}
			return {
				decision: "reject",
				riskLevel: result.risk_level ?? riskLevel,
				source: "reviewer",
				reason: `reviewer uncertain and no user prompt available: ${result.reason || "no clear decision"}`,
				saferAlternative: result.safer_alternative,
				isWorkspaceScoped: result.is_workspace_scoped,
				mode: this.mode,
				reviewerModel: this.reviewer.getReviewerModel?.(),
				reviewerReason: result.reason,
			};
		} catch (error) {
			// reviewer 失败：fallback 到用户确认而非直接拒绝
			return this.checkWithUser(request, riskLevel, isWorkspaceScoped, previewArguments(request.args), {
				ruleReason: `reviewer error: ${error instanceof Error ? error.message : String(error)}`,
			});
		}
	}

	private async checkWithUser(
		request: PermissionRequest,
		riskLevel: PermissionRiskLevel,
		isWorkspaceScoped: boolean,
		argumentsPreview: string,
		options?: { ruleReason?: string },
	): Promise<PermissionDecision> {
		if (!this.userPrompt) {
			return {
				decision: "reject",
				riskLevel,
				source: "user",
				reason: "permission confirmation UI is unavailable",
				saferAlternative: "Run with a configured reviewer, whitelist this call, or choose a read-only approach.",
				isWorkspaceScoped,
				mode: this.mode,
			};
		}
		const response = await this.userPrompt.request(request, {
			mode: this.mode,
			riskLevel,
			argumentsPreview,
			affectedPaths: request.affectedPaths ?? [],
			workspaceRoot: request.workspaceRoot,
			ruleReason: options?.ruleReason,
		});
		const approved = response.choice === "approve_once" || response.choice === "approve_session";
		return {
			decision: approved ? "approve" : "reject",
			riskLevel,
			source: "user",
			reason: response.reason ?? (approved ? "approved by user" : "rejected by user"),
			saferAlternative:
				response.choice === "reject_alternative"
					? (response.reason ?? "Choose another safer approach.")
					: undefined,
			userChoice: response.choice,
			trustKey: response.trustKey,
			isWorkspaceScoped,
			mode: this.mode,
		};
	}
}

function summarizeRules(rules: PermissionRuleSettings | undefined): string {
	if (!rules) return "none";
	const count =
		(rules.toolNames?.length ?? 0) +
		(rules.toolPrefixes?.length ?? 0) +
		(rules.commandExact?.length ?? 0) +
		(rules.commandPrefixes?.length ?? 0) +
		(rules.commandPatterns?.length ?? 0) +
		(rules.pathScopes?.length ?? 0) +
		(rules.pathPatterns?.length ?? 0) +
		(rules.riskLevels?.length ?? 0);
	return count === 0 ? "none" : `${count} rule${count === 1 ? "" : "s"}`;
}

export function createPermissionDeniedToolResult(decision: PermissionDecision): AgentToolResult<undefined> {
	return {
		content: [
			{
				type: "text",
				text: `permission denied by ${decision.source}: ${decision.reason}${
					decision.saferAlternative ? `\nsafer alternative: ${decision.saferAlternative}` : ""
				}\nask the agent to choose another safe approach`,
			},
		],
		details: undefined,
	};
}

function extractTextContent(message: { content: Array<TextContent | { type: string }> }): string {
	return message.content
		.filter((content): content is TextContent => content.type === "text")
		.map((content) => content.text)
		.join("")
		.trim();
}

function parseReviewerJson(text: string): PermissionReviewResult {
	// 尝试多种方式提取 JSON
	const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/i)?.[1];
	const jsonBlock = text.match(/\{[\s\S]*\}/)?.[0];
	const raw = fenced ?? jsonBlock ?? text;

	let parsed: any;
	try {
		parsed = JSON.parse(raw.trim());
	} catch {
		throw new Error(`reviewer returned unparseable response: ${text.slice(0, 200)}`);
	}

	if (!parsed || typeof parsed !== "object") {
		throw new Error(`reviewer returned non-object: ${text.slice(0, 200)}`);
	}

	// 三种决策：approve / reject / uncertain
	const rawDecision = String(parsed.decision ?? "").toLowerCase();
	const decision: ReviewerDecision =
		rawDecision === "approve" ? "approve" : rawDecision === "reject" ? "reject" : "uncertain";
	const reason = typeof parsed.reason === "string" ? parsed.reason : "no reason provided";
	const isWorkspaceScoped = typeof parsed.is_workspace_scoped === "boolean" ? parsed.is_workspace_scoped : true; // 缺失时默认安全

	return {
		decision,
		risk_level: parsed.risk_level ?? "unknown",
		is_workspace_scoped: isWorkspaceScoped,
		reason,
		safer_alternative: typeof parsed.safer_alternative === "string" ? parsed.safer_alternative : undefined,
	};
}

export class ModelAutoPermissionReviewer implements AutoPermissionReviewer {
	private readonly model: Model<any>;
	private readonly modelRegistry: ModelRegistry;
	private readonly settings: AutoPermissionReviewerSettings;

	constructor(model: Model<any>, modelRegistry: ModelRegistry, settings: AutoPermissionReviewerSettings) {
		this.model = model;
		this.modelRegistry = modelRegistry;
		this.settings = settings;
	}

	getReviewerModel(): string {
		return `${this.model.provider}/${this.model.id}`;
	}

	async review(request: PermissionRequest, mode: PermissionMode): Promise<PermissionReviewResult> {
		const auth = await this.modelRegistry.getApiKeyAndHeaders(this.model);
		if (!auth.ok) {
			throw new Error(auth.error);
		}
		const prompt = [
			"You are a permission reviewer for an AI coding agent.",
			'Return strict JSON only with keys: decision ("approve", "reject", or "uncertain"), risk_level, is_workspace_scoped, reason, safer_alternative.',
			'Use "approve" for safe workspace-scoped operations (file read/write/edit within workspace, safe shell commands, git status/log/diff).',
			'Use "reject" ONLY for clearly dangerous operations: destructive commands (rm -rf, git reset --hard, git clean -fd, force push, dd, mkfs, sudo, chmod -R, chown -R), secret-leaking, or operations outside workspace.',
			'Use "uncertain" when you cannot confidently determine safety. Most normal development operations (running tests, installing packages, building) should be approved.',
			JSON.stringify(
				{
					permission_mode: mode,
					tool_name: request.toolName,
					arguments_preview: previewArguments(request.args),
					command_preview: request.command ? redactText(request.command) : undefined,
					risk_level: request.riskLevel,
					affected_paths: request.affectedPaths,
					workspace_root: request.workspaceRoot,
					current_working_directory: request.cwd,
					target_inside_workspace: request.affectedPaths?.every((pathValue) =>
						isPathInside(pathValue, request.workspaceRoot),
					),
					git_branch: request.gitBranch,
					reason: request.reason,
					recent_context_summary: request.recentContextSummary,
					current_task_summary: request.currentTaskSummary,
				},
				null,
				2,
			),
		].join("\n\n");
		const response = await completeResolvedModel(
			this.model,
			{
				systemPrompt: "You review tool and shell permissions. Output strict JSON only.",
				messages: [{ role: "user", content: [{ type: "text", text: prompt }], timestamp: Date.now() }],
			},
			{
				apiKey: auth.apiKey,
				headers: auth.headers,
				temperature: this.settings.temperature,
				maxTokens: this.settings.maxTokens,
			},
		);
		return parseReviewerJson(extractTextContent(response));
	}
}

export function createAutoPermissionReviewerFromSettings(options: {
	settingsManager: SettingsManager;
	modelRegistry: ModelRegistry;
	fallbackModel?: Model<any>;
}): AutoPermissionReviewer | undefined {
	const settings = options.settingsManager.getAutoPermissionReviewerSettings();
	const mode = options.settingsManager.getPermissionMode();

	// 有明确指定 reviewer 模型
	if (settings.enabled && settings.provider && settings.model) {
		const model = options.modelRegistry.find(settings.provider, settings.model);
		if (model) {
			return new ModelAutoPermissionReviewer(model, options.modelRegistry, settings);
		}
	}

	// auto-permission 模式但未指定 reviewer：用 fallback 模型
	if (mode === "auto-permission" && options.fallbackModel) {
		return new ModelAutoPermissionReviewer(options.fallbackModel, options.modelRegistry, {
			enabled: true,
			temperature: 0,
			maxTokens: 512,
		});
	}

	return undefined;
}
