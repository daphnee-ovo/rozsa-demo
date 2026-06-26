import { APP_NAME } from "../config.ts";
import type { SourceInfo } from "./source-info.ts";

export type SlashCommandSource = "extension" | "prompt" | "skill";

export interface SlashCommandInfo {
	name: string;
	description?: string;
	source: SlashCommandSource;
	sourceInfo: SourceInfo;
}

export interface BuiltinSlashCommand {
	name: string;
	description: string;
	/** 命令用法说明，如 "/compact [prompt]" */
	usage?: string;
	/** 使用示例列表 */
	examples?: string[];
}

export const BUILTIN_SLASH_COMMANDS: ReadonlyArray<BuiltinSlashCommand> = [
	{ name: "settings", description: "Open settings menu" },
	{
		name: "model",
		description: "Select model (opens selector UI)",
		usage: "/model [name]",
		examples: ["/model sonnet:high", "/model"],
	},
	{ name: "scoped-models", description: "Enable/disable models for Ctrl+P cycling" },
	{
		name: "export",
		description: "Export session (HTML default, or specify path: .html/.jsonl)",
		usage: "/export [format|path]",
		examples: ["/export html", "/export md", "/export ./session.jsonl"],
	},
	{ name: "import", description: "Import and resume a session from a JSONL file" },
	{ name: "share", description: "Share session as a secret GitHub gist" },
	{ name: "copy", description: "Copy last agent message to clipboard" },
	{
		name: "name",
		description: "Set session display name",
		usage: "/name [session-name]",
		examples: ["/name auth-refactor", "/name"],
	},
	{
		name: "session",
		description: "Show session info and stats",
		usage: "/session [id]",
		examples: ["/session"],
	},
	{ name: "subagents", description: "List or switch subagent views" },
	{ name: "main", description: "Switch back to the main agent view" },
	{ name: "changelog", description: "Show changelog entries" },
	{
		name: "help",
		description: "Show help (topics: permissions, sessions, commands)",
		usage: "/help [topic|command]",
		examples: ["/help compact", "/help permissions", "/help"],
	},
	{ name: "hotkeys", description: "Show all keyboard shortcuts" },
	{ name: "fork", description: "Create a new fork from a previous user message" },
	{ name: "clone", description: "Duplicate the current session at the current position" },
	{ name: "tree", description: "Navigate session tree (switch branches)" },
	{ name: "graph", description: "Visual session timeline (git graph style)" },
	{ name: "login", description: "Configure provider authentication" },
	{ name: "logout", description: "Remove provider authentication" },
	{ name: "new", description: "Start a new session" },
	{
		name: "compact",
		description: "Manually compact the session context",
		usage: "/compact [prompt]",
		examples: ["/compact", "/compact focus on the auth refactor"],
	},
	{ name: "permissions", description: "Show permission decisions for this session" },
	{ name: "resume", description: "Resume a different session" },
	{ name: "reload", description: "Reload keybindings, extensions, skills, prompts, and themes" },
	{
		name: "search",
		description: "Search tool outputs for a pattern",
		usage: "/search <pattern>",
		examples: ["/search error", "/search TODO", "/search 'function.*init'"],
	},
	{ name: "quit", description: `Quit ${APP_NAME}` },
	{
		name: "gc",
		description: "Clean up old session files",
		usage: "/gc [days]",
		examples: ["/gc", "/gc 7"],
	},
	{
		name: "lsp",
		description: "Configure LSP auto-diagnostics mode",
		usage: "/lsp [agent_end|edit_write|disabled]",
		examples: ["/lsp", "/lsp agent_end", "/lsp disabled"],
	},
];
