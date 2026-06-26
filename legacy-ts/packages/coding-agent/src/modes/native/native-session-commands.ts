import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { getSessionsDir, getShareViewerUrl } from "../../config.ts";
import type { AgentSession } from "../../core/agent-session.ts";
import { SessionImportFileNotFoundError } from "../../core/agent-session-runtime.ts";
import { getChangelogPath, parseChangelog } from "../../utils/changelog.ts";
import { copyToClipboard } from "../../utils/clipboard.ts";
import type { NativeBuiltinContext } from "./native-builtins.ts";

export async function handleExport(arg: string, ctx: NativeBuiltinContext): Promise<void> {
	try {
		const filePath = arg.endsWith(".jsonl")
			? ctx.session.exportToJsonl(arg || undefined)
			: await ctx.session.exportToHtml(arg || undefined);
		ctx.notify(`Session exported to: ${filePath}`);
	} catch (error) {
		ctx.notify(`Failed to export session: ${error instanceof Error ? error.message : String(error)}`, "error");
	}
}

export async function handleImport(arg: string, ctx: NativeBuiltinContext): Promise<void> {
	if (!arg) {
		ctx.notify("Usage: /import <path.jsonl>", "warning");
		return;
	}
	const confirmed = await ctx.select("Import session", [`Replace current session with ${arg}`, "Cancel"]);
	if (!confirmed || confirmed === "Cancel") {
		ctx.notify("Import cancelled");
		return;
	}
	try {
		const result = await ctx.runtimeHost.importFromJsonl(arg);
		ctx.notify(result.cancelled ? "Import cancelled" : `Session imported from: ${arg}`);
	} catch (error) {
		if (error instanceof SessionImportFileNotFoundError) {
			ctx.notify(`Failed to import session: ${error.message}`, "error");
		} else {
			ctx.notify(`Failed to import session: ${error instanceof Error ? error.message : String(error)}`, "error");
		}
	}
}

export async function handleShare(ctx: NativeBuiltinContext): Promise<void> {
	const auth = spawnSync("gh", ["auth", "status"], { encoding: "utf-8" });
	if (auth.status !== 0) {
		ctx.notify("GitHub CLI is not logged in. Run 'gh auth login' first.", "error");
		return;
	}
	const tempPath = join(ctx.session.sessionManager.getCwd(), "temp", "session-share.html");
	try {
		await ctx.session.exportToHtml(tempPath);
		const result = spawnSync("gh", ["gist", "create", "--public=false", tempPath], { encoding: "utf-8" });
		if (result.status !== 0) {
			ctx.notify(`Failed to create gist: ${result.stderr.trim() || "Unknown error"}`, "error");
			return;
		}
		const gistUrl = result.stdout.trim();
		const gistId = gistUrl.split("/").pop();
		ctx.notify(gistId ? `Share URL: ${getShareViewerUrl(gistId)}\nGist: ${gistUrl}` : "Failed to parse gist ID");
	} catch (error) {
		ctx.notify(`Failed to share session: ${error instanceof Error ? error.message : String(error)}`, "error");
	}
}

export async function handleCopy(ctx: NativeBuiltinContext): Promise<void> {
	const text = ctx.session.getLastAssistantText();
	if (!text) {
		ctx.notify("No agent messages to copy yet.", "warning");
		return;
	}
	try {
		await copyToClipboard(text);
		ctx.notify("Copied last agent message to clipboard");
	} catch (error) {
		ctx.notify(error instanceof Error ? error.message : String(error), "error");
	}
}

export function formatChangelog(): string {
	const entries = parseChangelog(getChangelogPath());
	if (entries.length === 0) return "No changelog entries found.";
	return entries
		.reverse()
		.map((entry) => entry.content)
		.join("\n\n");
}

export function handleLsp(arg: string, ctx: NativeBuiltinContext): void {
	const validModes = ["agent_end", "edit_write", "disabled"];
	if (!arg) {
		ctx.notify(`LSP mode: ${ctx.session.lspHook?.getHookMode() ?? "disabled (not active)"}`);
		return;
	}
	if (!validModes.includes(arg)) {
		ctx.notify(`Invalid LSP mode "${arg}". Valid: ${validModes.join(", ")}`, "warning");
		return;
	}
	if (!ctx.session.lspHook) {
		ctx.notify("LSP hook not initialized", "warning");
		return;
	}
	ctx.session.lspHook.setHookMode(arg as "agent_end" | "edit_write" | "disabled");
	ctx.notify(`LSP mode set to: ${arg}`);
}

export async function handleResume(ctx: NativeBuiltinContext): Promise<void> {
	// 触发 list_sessions 协议消息，由 Rust TUI 渲染 session selector
	ctx.listSessions("current");
}

export async function handleGc(arg: string, ctx: NativeBuiltinContext): Promise<void> {
	const days = arg ? Number.parseInt(arg, 10) : 30;
	if (Number.isNaN(days) || days <= 0) {
		ctx.notify("Invalid days argument. Usage: /gc 7", "warning");
		return;
	}
	const stale = findStaleSessionFiles(days, ctx.session.sessionManager.getSessionFile());
	if (stale.length === 0) {
		ctx.notify(`No session files older than ${days} days`);
		return;
	}
	const mb = (stale.reduce((sum, file) => sum + file.size, 0) / (1024 * 1024)).toFixed(2);
	const confirmed = await ctx.select("Session GC", [`Trash ${stale.length} files (${mb} MB)`, "Cancel"]);
	if (!confirmed || confirmed === "Cancel") {
		ctx.notify("Session GC cancelled");
		return;
	}
	const result = spawnSync(
		"trash",
		stale.map((file) => file.path),
		{ encoding: "utf-8" },
	);
	if (result.status === 0) ctx.notify(`Trashed ${stale.length} old session files`);
	else ctx.notify(`Session GC failed: ${result.stderr.trim() || "trash exited with error"}`, "error");
}

export function handleSearch(arg: string, ctx: NativeBuiltinContext): void {
	if (!arg) {
		ctx.notify("Usage: /search <pattern>", "warning");
		return;
	}
	const regex = safeRegex(arg);
	const matches: string[] = [];
	for (const message of ctx.session.state.messages) {
		for (const line of extractMessageText(message).split("\n")) {
			regex.lastIndex = 0;
			if (regex.test(line)) {
				matches.push(line);
				if (matches.length >= 50) break;
			}
		}
		if (matches.length >= 50) break;
	}
	ctx.notify(matches.length > 0 ? `Search "${arg}"\n${matches.join("\n")}` : `No matches found for "${arg}"`);
}

function findStaleSessionFiles(
	days: number,
	currentSessionFile: string | undefined,
): Array<{ path: string; size: number }> {
	const root = getSessionsDir();
	if (!existsSync(root)) return [];
	const cutoff = Date.now() - days * 24 * 60 * 60 * 1000;
	const current = currentSessionFile ? resolve(currentSessionFile) : undefined;
	const files: Array<{ path: string; size: number }> = [];
	for (const entry of readdirSync(root, { withFileTypes: true })) {
		if (!entry.isDirectory()) continue;
		for (const file of readdirSync(join(root, entry.name))) {
			if (!file.endsWith(".jsonl")) continue;
			const filePath = join(root, entry.name, file);
			if (current && resolve(filePath) === current) continue;
			const stats = statSync(filePath);
			if (stats.mtime.getTime() < cutoff) files.push({ path: filePath, size: stats.size });
		}
	}
	return files;
}

function safeRegex(pattern: string): RegExp {
	try {
		return new RegExp(pattern, "gi");
	} catch {
		return new RegExp(pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi");
	}
}

function extractMessageText(message: AgentSession["state"]["messages"][number]): string {
	const content = (message as { content?: unknown }).content;
	if (typeof content === "string") return content;
	if (!Array.isArray(content)) return "";
	return content
		.map((part) => (typeof part === "object" && part !== null && "text" in part ? String(part.text ?? "") : ""))
		.join("\n");
}
