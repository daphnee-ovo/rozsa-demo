import type { Api, Model } from "@earendil-works/rozsa-model-types";
import type { AgentSession } from "../../core/agent-session.ts";
import type { AgentSessionRuntime } from "../../core/agent-session-runtime.ts";
import { formatHttpIdleTimeoutMs, HTTP_IDLE_TIMEOUT_CHOICES } from "../../core/http-dispatcher.ts";
import { findExactModelReferenceMatch } from "../../core/model-resolver.ts";
import { BUILT_IN_PROVIDER_DISPLAY_NAMES } from "../../core/provider-display-names.ts";
import type { SessionTreeNode } from "../../core/session-manager.ts";
import { BUILTIN_SLASH_COMMANDS } from "../../core/slash-commands.ts";
import type { NativeKeybindings } from "./native-keybindings.ts";
import {
	formatChangelog,
	handleCopy,
	handleExport,
	handleGc,
	handleImport,
	handleLsp,
	handleResume,
	handleSearch,
	handleShare,
} from "./native-session-commands.ts";

export interface NativeBuiltinContext {
	session: AgentSession;
	runtimeHost: AgentSessionRuntime;
	keybindings: NativeKeybindings;
	notify(message: string, level?: "info" | "warning" | "error"): void;
	select(title: string, options: string[], selectedIndex?: number): Promise<string | undefined>;
	listSessions(scope: "current" | "all"): void;
	listModels(): void;
	setInput(text: string): void;
	setActiveSubagent(id: string | undefined): void;
	activeSubagentId(): string | undefined;
	dispose(): Promise<void>;
}

export async function handleNativeBuiltinCommand(text: string, ctx: NativeBuiltinContext): Promise<boolean> {
	if (!text.startsWith("/")) return false;
	const [command = "", ...rest] = text.slice(1).split(/\s+/);
	const arg = rest.join(" ").trim();
	switch (command) {
		case "settings":
			await handleSettings(ctx);
			return true;
		case "help":
			ctx.notify(formatHelp(arg));
			return true;
		case "hotkeys":
			ctx.notify(formatHotkeys(ctx.keybindings));
			return true;
		case "permissions":
			ctx.notify(formatPermissions(ctx.session));
			return true;
		case "session":
			ctx.notify(formatSession(ctx.session));
			return true;
		case "main":
			ctx.setActiveSubagent(undefined);
			ctx.notify("Switched to main agent");
			return true;
		case "subagent":
		case "subagents":
			await handleSubagents(arg, ctx);
			return true;
		case "name":
			handleName(arg, ctx);
			return true;
		case "model":
			await handleModel(arg, ctx);
			return true;
		case "scoped-models":
			await handleScopedModels(ctx);
			return true;
		case "export":
			await handleExport(arg, ctx);
			return true;
		case "import":
			await handleImport(arg, ctx);
			return true;
		case "share":
			await handleShare(ctx);
			return true;
		case "copy":
			await handleCopy(ctx);
			return true;
		case "tree":
			await handleTree(ctx);
			return true;
		case "fork":
			await handleFork(ctx);
			return true;
		case "clone":
			await handleClone(ctx);
			return true;
		case "new":
			await ctx.runtimeHost.newSession();
			ctx.notify("Started new session");
			return true;
		case "compact":
			await ctx.session.compact(arg || undefined);
			ctx.notify("Compacted session");
			return true;
		case "reload":
			await ctx.session.reload();
			ctx.notify("Reloaded keybindings, extensions, skills, prompts, and themes");
			return true;
		case "changelog":
			ctx.notify(formatChangelog());
			return true;
		case "lsp":
			handleLsp(arg, ctx);
			return true;
		case "resume":
			await handleResume(ctx);
			return true;
		case "gc":
			await handleGc(arg, ctx);
			return true;
		case "search":
			handleSearch(arg, ctx);
			return true;
		case "quit":
			await ctx.dispose();
			return true;
		default:
			if (BUILTIN_SLASH_COMMANDS.some((item) => item.name === command)) {
				ctx.notify(`/${command} is not supported by the native TUI yet`, "warning");
				return true;
			}
			return false;
	}
}

function formatHelp(arg: string): string {
	if (arg) {
		const command = BUILTIN_SLASH_COMMANDS.find((item) => item.name === arg.replace(/^\//, ""));
		if (!command) return `No help for ${arg}`;
		const lines = [`/${command.name}: ${command.description}`];
		if (command.usage) lines.push(`usage: ${command.usage}`);
		for (const example of command.examples ?? []) lines.push(`example: ${example}`);
		return lines.join("\n");
	}
	return BUILTIN_SLASH_COMMANDS.map((command) => `/${command.name} - ${command.description}`).join("\n");
}

function formatHotkeys(keybindings: NativeKeybindings): string {
	return Object.entries(keybindings)
		.filter(([, keys]) => keys.length > 0)
		.map(([action, keys]) => `${keys.join(", ")}  ${action}`)
		.join("\n");
}

function formatPermissions(session: AgentSession): string {
	const history = session.permissionManager.getPermissionHistory();
	const lines = [
		"Permission Decisions",
		`Mode: ${session.permissionManager.getMode()}`,
		`Session approvals: ${session.permissionManager.getSessionApprovalCount()}`,
		`Total decisions: ${history.length}`,
	];
	for (const entry of history.slice(-50).reverse()) {
		const command = entry.command ? ` ${entry.command.slice(0, 60)}` : "";
		const source = entry.userChoice ? `${entry.source}/${entry.userChoice}` : entry.source;
		lines.push(`${entry.timestamp} ${entry.toolName}${command} -> ${entry.decision} (${source})`);
	}
	return lines.join("\n");
}

function formatSession(session: AgentSession): string {
	const stats = session.getSessionStats();
	return [
		"Session Info",
		`Name: ${session.sessionManager.getSessionName() ?? "(unnamed)"}`,
		`File: ${stats.sessionFile ?? "In-memory"}`,
		`ID: ${stats.sessionId}`,
		`User messages: ${stats.userMessages}`,
		`Assistant messages: ${stats.assistantMessages}`,
		`Tool calls: ${stats.toolCalls}`,
		`Tool results: ${stats.toolResults}`,
	].join("\n");
}

function handleName(arg: string, ctx: NativeBuiltinContext): void {
	if (!arg) {
		ctx.notify(`Session name: ${ctx.session.sessionManager.getSessionName() ?? "(unnamed)"}`);
		return;
	}
	ctx.session.setSessionName(arg);
	ctx.notify(`Session name set: ${arg}`);
}

async function handleModel(arg: string, ctx: NativeBuiltinContext): Promise<void> {
	if (!arg) {
		ctx.listModels();
		return;
	}
	ctx.session.modelRegistry.refresh();
	const models = ctx.session.modelRegistry.getAvailable();
	const selected = findModel(models, arg);
	if (!selected) {
		ctx.notify(`Model not found: ${arg}`, "warning");
		return;
	}
	await ctx.session.setModel(selected);
	ctx.notify(`Model: [${providerDisplayName(selected.provider)}] ${selected.id}`);
}

async function handleScopedModels(ctx: NativeBuiltinContext): Promise<void> {
	const current = ctx.session.scopedModels.map((scoped) => `${scoped.model.provider}/${scoped.model.id}`);
	const choice = await ctx.select("Scoped models", ["Show current", "Enable all models"]);
	if (choice === "Enable all models") {
		ctx.session.setScopedModels([]);
		ctx.notify("Scoped model filter cleared");
	} else if (choice === "Show current") {
		ctx.notify(current.length > 0 ? current.join("\n") : "All models enabled");
	}
}

async function handleSettings(ctx: NativeBuiltinContext): Promise<void> {
	// 循环显示 settings dialog，选中项 toggle 后刷新面板，Esc 取消退出
	let lastIndex = 0;
	while (true) {
		const actions = buildSettingsActions(ctx);
		const selected = await ctx.select(
			"Settings (Enter to toggle, Esc to close)",
			actions.map((a) => a.label),
			lastIndex,
		);
		if (!selected) break;
		const idx = actions.findIndex((a) => a.label === selected);
		if (idx >= 0) {
			lastIndex = idx;
			await actions[idx]!.run();
		}
	}
}

function buildSettingsActions(ctx: NativeBuiltinContext): Array<{ label: string; run: () => Promise<void> | void }> {
	const settings = ctx.session.settingsManager;
	const onOff = (v: boolean) => (v ? "on" : "off");
	return [
		{
			label: `[AI] Auto compact: ${onOff(ctx.session.autoCompactionEnabled)}`,
			run: () => ctx.session.setAutoCompactionEnabled(!ctx.session.autoCompactionEnabled),
		},
		{
			label: `[AI] Thinking level: ${ctx.session.thinkingLevel}`,
			run: () => {
				const levels = ctx.session.getAvailableThinkingLevels();
				const idx = levels.indexOf(ctx.session.thinkingLevel);
				ctx.session.setThinkingLevel(levels[(idx + 1) % levels.length] ?? ctx.session.thinkingLevel);
			},
		},
		{
			label: `[AI] Steering mode: ${settings.getSteeringMode()}`,
			run: () => ctx.session.setSteeringMode(settings.getSteeringMode() === "all" ? "one-at-a-time" : "all"),
		},
		{
			label: `[AI] Follow-up mode: ${settings.getFollowUpMode()}`,
			run: () => ctx.session.setFollowUpMode(settings.getFollowUpMode() === "all" ? "one-at-a-time" : "all"),
		},
		{
			label: `[Network] Transport: ${settings.getTransport()}`,
			run: () => {
				const opts: Array<"sse" | "websocket" | "websocket-cached" | "auto"> = [
					"auto",
					"sse",
					"websocket",
					"websocket-cached",
				];
				const idx = opts.indexOf(settings.getTransport() as (typeof opts)[number]);
				settings.setTransport(opts[(idx + 1) % opts.length]!);
			},
		},
		{
			label: `[Permission] Permission mode: ${settings.getPermissionMode()}`,
			run: () => {
				const opts: Array<"on-request" | "auto-permission" | "free-permission"> = [
					"on-request",
					"auto-permission",
					"free-permission",
				];
				const idx = opts.indexOf(settings.getPermissionMode() as (typeof opts)[number]);
				const newMode = opts[(idx + 1) % opts.length]!;
				settings.setPermissionMode(newMode);
				ctx.session.permissionManager.setMode(newMode);
			},
		},
		{
			label: `[Display] Show images: ${onOff(settings.getShowImages())}`,
			run: () => settings.setShowImages(!settings.getShowImages()),
		},
		{
			label: `[Display] Auto resize images: ${onOff(settings.getImageAutoResize())}`,
			run: () => settings.setImageAutoResize(!settings.getImageAutoResize()),
		},
		{
			label: `[Display] Block images: ${onOff(settings.getBlockImages())}`,
			run: () => settings.setBlockImages(!settings.getBlockImages()),
		},
		{
			label: `[Editor] Skill commands: ${onOff(settings.getEnableSkillCommands())}`,
			run: () => settings.setEnableSkillCommands(!settings.getEnableSkillCommands()),
		},
		{
			label: `[Display] Hide thinking: ${onOff(settings.getHideThinkingBlock())}`,
			run: () => settings.setHideThinkingBlock(!settings.getHideThinkingBlock()),
		},
		{
			label: `[Display] Quiet startup: ${onOff(settings.getQuietStartup())}`,
			run: () => settings.setQuietStartup(!settings.getQuietStartup()),
		},
		{
			label: `[Display] Collapse changelog: ${onOff(settings.getCollapseChangelog())}`,
			run: () => settings.setCollapseChangelog(!settings.getCollapseChangelog()),
		},
		{
			label: `[Display] Terminal progress: ${onOff(settings.getShowTerminalProgress())}`,
			run: () => settings.setShowTerminalProgress(!settings.getShowTerminalProgress()),
		},
		{
			label: `[Editor] Double-escape: ${settings.getDoubleEscapeAction()}`,
			run: () => {
				const opts: Array<"graph" | "tree" | "fork" | "none"> = ["graph", "tree", "fork", "none"];
				const idx = opts.indexOf(settings.getDoubleEscapeAction());
				settings.setDoubleEscapeAction(opts[(idx + 1) % opts.length]!);
			},
		},
		{
			label: `[Editor] Tree filter: ${settings.getTreeFilterMode()}`,
			run: () => {
				const opts: Array<"default" | "no-tools" | "user-only" | "labeled-only" | "all"> = [
					"default",
					"no-tools",
					"user-only",
					"labeled-only",
					"all",
				];
				const idx = opts.indexOf(settings.getTreeFilterMode());
				settings.setTreeFilterMode(opts[(idx + 1) % opts.length]!);
			},
		},
		{
			label: `[Network] Install telemetry: ${onOff(settings.getEnableInstallTelemetry())}`,
			run: () => settings.setEnableInstallTelemetry(!settings.getEnableInstallTelemetry()),
		},
		{
			label: `[Network] HTTP idle timeout: ${formatHttpIdleTimeoutMs(settings.getHttpIdleTimeoutMs())}`,
			run: () => {
				const choices = HTTP_IDLE_TIMEOUT_CHOICES;
				const current = settings.getHttpIdleTimeoutMs();
				const idx = choices.findIndex((c) => c.timeoutMs === current);
				const next = choices[(idx + 1) % choices.length]!;
				settings.setHttpIdleTimeoutMs(next.timeoutMs);
			},
		},
	];
}

function providerDisplayName(provider: string): string {
	return BUILT_IN_PROVIDER_DISPLAY_NAMES[provider] ?? provider;
}

function findModel(models: Model<Api>[], reference: string): Model<Api> | undefined {
	return findExactModelReferenceMatch(reference, models);
}

async function handleTree(ctx: NativeBuiltinContext): Promise<void> {
	const rows = flattenTree(ctx.session.sessionManager.getTree());
	if (rows.length === 0) {
		ctx.notify("No entries in session");
		return;
	}
	const selected = await ctx.select(
		"Session tree",
		rows.map((row) => row.label),
	);
	const row = rows.find((candidate) => candidate.label === selected);
	if (!row) return;
	const result = await ctx.session.navigateTree(row.id, { summarize: false });
	if (result.editorText) ctx.setInput(result.editorText);
	if (result.cancelled) ctx.notify("Navigation cancelled", "warning");
	else ctx.notify("Navigated to selected point");
}

async function handleFork(ctx: NativeBuiltinContext): Promise<void> {
	const messages = ctx.session.getUserMessagesForForking();
	const options = messages.map((message) => `${message.entryId}: ${message.text.slice(0, 80)}`);
	const selected = await ctx.select("Fork from message", options);
	const entryId = selected?.split(":")[0];
	if (!entryId) return;
	const result = await ctx.runtimeHost.fork(entryId);
	if (result.selectedText) ctx.setInput(result.selectedText);
	ctx.notify(result.cancelled ? "Fork cancelled" : "Forked to new session");
}

async function handleClone(ctx: NativeBuiltinContext): Promise<void> {
	const leafId = ctx.session.sessionManager.getLeafId();
	if (!leafId) {
		ctx.notify("Nothing to clone yet", "warning");
		return;
	}
	const result = await ctx.runtimeHost.fork(leafId, { position: "at" });
	ctx.notify(result.cancelled ? "Clone cancelled" : "Cloned to new session");
}

async function handleSubagents(arg: string, ctx: NativeBuiltinContext): Promise<void> {
	const subagents = ctx.session.listSubagents();
	if (arg === "main") {
		ctx.setActiveSubagent(undefined);
		ctx.notify("Switched to main agent");
		return;
	}
	if (arg === "next" || arg === "previous") {
		switchSubagent(arg, ctx, subagents);
		return;
	}
	if (arg.startsWith("interrupt")) {
		const id = arg.slice("interrupt".length).trim() || ctx.activeSubagentId();
		if (!id) {
			ctx.notify("No subagent selected", "warning");
			return;
		}
		await ctx.session.abortSubagent(id);
		ctx.notify(`Interrupted ${id}`);
		return;
	}
	if (arg) {
		ctx.setActiveSubagent(arg);
		ctx.notify(`Switched to ${arg}`);
		return;
	}
	if (subagents.length === 0) {
		ctx.notify("No subagents");
		return;
	}
	const selected = await ctx.select("Subagents", [
		"main",
		...subagents.map((subagent) => `${subagent.id} ${subagent.status} ${subagent.name}`),
	]);
	if (!selected) return;
	if (selected === "main") ctx.setActiveSubagent(undefined);
	else ctx.setActiveSubagent(selected.split(" ")[0]);
}

function switchSubagent(
	direction: "next" | "previous",
	ctx: NativeBuiltinContext,
	subagents: ReturnType<AgentSession["listSubagents"]>,
): void {
	if (subagents.length === 0) {
		ctx.notify("No subagents", "warning");
		return;
	}
	const ids = [undefined, ...subagents.map((subagent) => subagent.id)];
	const current = ids.indexOf(ctx.activeSubagentId());
	const base = current === -1 ? 0 : current;
	const next = direction === "next" ? (base + 1) % ids.length : (base - 1 + ids.length) % ids.length;
	ctx.setActiveSubagent(ids[next]);
	ctx.notify(ids[next] ? `Switched to ${ids[next]}` : "Switched to main agent");
}

function flattenTree(tree: SessionTreeNode[], depth = 0): Array<{ id: string; label: string }> {
	const rows: Array<{ id: string; label: string }> = [];
	for (const node of tree) {
		const role = node.entry.type === "message" ? node.entry.message.role : node.entry.type;
		const label = node.label ? `${node.label} ` : "";
		rows.push({ id: node.entry.id, label: `${"  ".repeat(depth)}${label}${role} ${node.entry.id}` });
		rows.push(...flattenTree(node.children, depth + 1));
	}
	return rows;
}
