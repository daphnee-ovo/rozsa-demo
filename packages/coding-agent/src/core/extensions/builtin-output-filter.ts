/**
 * 内置输出过滤器 — 在工具结果返回给 LLM 前清洗敏感信息。
 * 作为 extension factory 注入，始终启用。
 */

import type { ExtensionAPI, ExtensionFactory } from "./types.ts";

interface SensitivePattern {
	pattern: RegExp;
	replacement: string;
}

// API key / token 模式
const SENSITIVE_PATTERNS: SensitivePattern[] = [
	// OpenAI
	{ pattern: /\b(sk-[a-zA-Z0-9]{20,})\b/g, replacement: "[OPENAI_KEY_REDACTED]" },
	// Anthropic
	{ pattern: /\b(sk-ant-[a-zA-Z0-9_-]{20,})\b/g, replacement: "[ANTHROPIC_KEY_REDACTED]" },
	// OpenRouter
	{ pattern: /\b(sk-or-v1-[a-zA-Z0-9_-]{20,})\b/g, replacement: "[OPENROUTER_KEY_REDACTED]" },
	// Google
	{ pattern: /\b(AIza[a-zA-Z0-9_-]{30,})\b/g, replacement: "[GOOGLE_KEY_REDACTED]" },
	// Cloudflare tokens
	{ pattern: /\b(cf(?:k|ut|at)_[a-zA-Z0-9_-]{41,})\b/g, replacement: "[CLOUDFLARE_TOKEN_REDACTED]" },
	// npm
	{ pattern: /\b(npm_[a-zA-Z0-9]{20,})\b/g, replacement: "[NPM_TOKEN_REDACTED]" },
	// GitLab
	{ pattern: /\b(glpat-[a-zA-Z0-9_-]{20,})\b/g, replacement: "[GITLAB_TOKEN_REDACTED]" },
	// GitHub
	{ pattern: /\b(gh[pousr]_[a-zA-Z0-9]{36,})\b/g, replacement: "[GITHUB_TOKEN_REDACTED]" },
	// Slack
	{ pattern: /\b(xox[baprs]-[a-zA-Z0-9-]{10,})\b/g, replacement: "[SLACK_TOKEN_REDACTED]" },
	// AWS
	{ pattern: /\b(AKIA[A-Z0-9]{16})\b/g, replacement: "[AWS_KEY_REDACTED]" },
	// 通用 key=value（含复合名 client_secret、api-token 等）
	{ pattern: /\b(api[_-]?key|apikey)\s*[=:]\s*['"]?([a-zA-Z0-9_-]{20,})['"]?/gi, replacement: "$1=[REDACTED]" },
	{
		pattern: /[_-]?(secret|token|password|passwd|pwd|credential)\s*["']?\s*[=:]\s*['"]?([^\s'"]{8,})['"]?/gi,
		replacement: "$1=[REDACTED]",
	},
	// JSON 格式的敏感字段："client_secret": "value"
	{
		pattern:
			/["']([^"']*?(secret|token|password|passwd|pwd|credential|api_key|apikey)[^"']*?)["']\s*:\s*["']([^"']{8,})["']/gi,
		replacement: '"$1": "[REDACTED]"',
	},
	// Bearer token
	{ pattern: /\b(bearer)\s+([a-zA-Z0-9._-]{20,})\b/gi, replacement: "Bearer [REDACTED]" },
	// JWT
	{ pattern: /\beyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b/g, replacement: "[JWT_REDACTED]" },
	// 数据库连接串
	{ pattern: /(mongodb(\+srv)?:\/\/[^:]+:)[^@]+(@)/gi, replacement: "$1[REDACTED]$3" },
	{ pattern: /(postgres(ql)?:\/\/[^:]+:)[^@]+(@)/gi, replacement: "$1[REDACTED]$3" },
	{ pattern: /(mysql:\/\/[^:]+:)[^@]+(@)/gi, replacement: "$1[REDACTED]$3" },
	{ pattern: /(redis:\/\/[^:]+:)[^@]+(@)/gi, replacement: "$1[REDACTED]$3" },
	// 私钥
	{
		pattern: /-----BEGIN (RSA |EC |OPENSSH |)PRIVATE KEY-----[\s\S]*?-----END \1PRIVATE KEY-----/g,
		replacement: "[PRIVATE_KEY_REDACTED]",
	},
];

// 敏感文件路径 — 读取时整体屏蔽
const SENSITIVE_FILE_PATTERNS = [
	/(^|\/)\.env$/,
	/(^|\/)\.env\.(?!example$)[^/]+$/,
	/(^|\/)\.dev\.vars($|\.[^/]+$)/,
	/(^|\/)secrets?\.(json|ya?ml|toml)$/i,
	/(^|\/)credentials/i,
	/(^|\/)auth\.json$/i,
	/(^|\/)\.npmrc$/,
	/(^|\/)id_rsa/,
	/(^|\/)\.ssh\//,
];

function redactText(text: string): { text: string; modified: boolean } {
	let result = text;
	let modified = false;

	for (const { pattern, replacement } of SENSITIVE_PATTERNS) {
		// 重置 lastIndex（正则带 g 标志需要）
		pattern.lastIndex = 0;
		const redacted = result.replace(pattern, replacement);
		if (redacted !== result) {
			modified = true;
			result = redacted;
		}
	}

	return { text: result, modified };
}

// 检测 bash 命令是否在读取敏感文件（通过 cat/python/node 等间接手段）
const SENSITIVE_READ_CMD_RE =
	/\b(cat|less|more|head|tail|python3?|node|ruby|perl|php)\b[^\n]*?(\.env|id_rsa|credentials?|tokens?|auth\.json|\.npmrc|secrets?)\b/i;
// 备选：命令中引用了敏感文件路径（包括字符串拼接场景）
const SENSITIVE_PATH_IN_CMD_RE = /(['"`]|\/)[^\s'"]*?(\.env|auth\.json|id_rsa|credentials?|\.npmrc|secrets?)\b/i;
// 命令中通过代码访问敏感环境变量（os.environ、process.env 等）
// 变量名中含 KEY/TOKEN/SECRET 等（不要求 \b 因为可能在 AWS_SECRET_KEY 中间）
const SENSITIVE_ENV_ACCESS_RE =
	/\b(os\.environ|process\.env|getenv|ENV)\b[^\n]*?(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTH|BEARER|PRIVATE)/i;

// 环境变量 dump 格式检测：KEY=value 行（多行输出中）
const ENV_DUMP_RE = /^[A-Z_]{2,}[A-Z0-9_]*=.+/;
// 敏感变量名
const SENSITIVE_ENV_KEY_RE =
	/^(.*_)?(KEY|TOKEN|SECRET|PASSWORD|PASSWD|PWD|CREDENTIAL|AUTH|PRIVATE|API_KEY|APIKEY|BEARER)(_.+)?$/i;

/**
 * 检测输出中是否包含环境变量 dump（多个 KEY=value 行）。
 * 如有敏感变量，对其值做脱敏。
 */
function redactEnvDump(text: string): { text: string; modified: boolean } {
	const lines = text.split("\n");
	let modified = false;
	const result = lines.map((line) => {
		if (!ENV_DUMP_RE.test(line)) return line;
		const eqIdx = line.indexOf("=");
		if (eqIdx < 0) return line;
		const key = line.slice(0, eqIdx);
		if (SENSITIVE_ENV_KEY_RE.test(key)) {
			modified = true;
			return `${key}=[REDACTED]`;
		}
		return line;
	});
	return { text: result.join("\n"), modified };
}

/**
 * 高熵字符串检测 — 长随机串大概率是密钥/token。
 * 条件：32+ 字符，字符集混合（大小写+数字），不像常见路径/URL path。
 */
function redactHighEntropy(text: string): { text: string; modified: boolean } {
	// 匹配独立的长随机串（非 URL path，非已知安全模式）
	const highEntropyRe = /(?<![/a-zA-Z0-9])([a-zA-Z0-9_-]{40,})(?![/a-zA-Z0-9])/g;
	let modified = false;
	const result = text.replace(highEntropyRe, (match) => {
		// 排除全小写或全大写（可能是 hash/变量名），要求混合字符集
		const hasUpper = /[A-Z]/.test(match);
		const hasLower = /[a-z]/.test(match);
		const hasDigit = /[0-9]/.test(match);
		if ((hasUpper && hasLower && hasDigit) || (hasUpper && hasDigit && match.length >= 48)) {
			modified = true;
			return "[HIGH_ENTROPY_REDACTED]";
		}
		return match;
	});
	return { text: result, modified };
}

export const builtinOutputFilter: ExtensionFactory = (rozsa: ExtensionAPI) => {
	rozsa.on("tool_result", async (event) => {
		if (event.isError) return undefined;

		// 敏感文件整体屏蔽 — 对 read 工具按路径判断
		if (event.toolName === "read" && typeof event.input.path === "string") {
			const filePath = event.input.path;

			if (/(^|\/)\.env\.example$/i.test(filePath)) {
				return undefined;
			}

			for (const pattern of SENSITIVE_FILE_PATTERNS) {
				if (pattern.test(filePath)) {
					return {
						content: [{ type: "text", text: `[Contents of ${filePath} redacted — sensitive file]` }],
					};
				}
			}
		}

		// bash 命令读取敏感文件或敏感环境变量 — 整体屏蔽输出
		if (event.toolName === "bash" && typeof event.input.command === "string") {
			const cmd = event.input.command;
			if (SENSITIVE_READ_CMD_RE.test(cmd) || SENSITIVE_PATH_IN_CMD_RE.test(cmd)) {
				return {
					content: [{ type: "text", text: `[Output redacted — command accesses sensitive file]` }],
				};
			}
			if (SENSITIVE_ENV_ACCESS_RE.test(cmd)) {
				return {
					content: [{ type: "text", text: `[Output redacted — command accesses sensitive environment variable]` }],
				};
			}
		}

		// 对所有工具输出做多层级脱敏
		let wasModified = false;
		const content = event.content.map((item) => {
			if (item.type !== "text") return item;
			let text = item.text;

			// 1. 已知 token/key 模式
			const r1 = redactText(text);
			if (r1.modified) {
				wasModified = true;
				text = r1.text;
			}

			// 2. 环境变量 dump 中的敏感值
			const r2 = redactEnvDump(text);
			if (r2.modified) {
				wasModified = true;
				text = r2.text;
			}

			// 3. 高熵字符串（疑似密钥）
			const r3 = redactHighEntropy(text);
			if (r3.modified) {
				wasModified = true;
				text = r3.text;
			}

			return wasModified ? { ...item, text } : item;
		});

		if (wasModified) {
			return { content };
		}

		return undefined;
	});
};
