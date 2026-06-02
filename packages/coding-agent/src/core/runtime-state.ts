import { execFile } from "node:child_process";
import { basename, relative } from "node:path";
import type { AgentMessage } from "@earendil-works/pi-agent-core";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { PermissionDecision, PermissionMode, PermissionRiskLevel } from "./permissions.ts";

export interface ProjectInfo {
	projectName: string;
	workspaceRoot: string;
	currentWorkingDirectory: string;
	sessionName?: string;
}

export interface PermissionRuntimeState {
	mode: PermissionMode;
	whitelistSummary: string;
	blacklistSummary: string;
	sessionApprovals: number;
	lastDecision?: PermissionDecision;
}

export interface ModelUsageState {
	provider?: string;
	model?: string;
	reasoningEffort?: string;
	promptTokens: number;
	completionTokens: number;
	reasoningTokens: number;
	totalTokens: number;
	currentTurnTokens: number;
	sessionTotalTokens: number;
}

export interface GitStatus {
	enabled: boolean;
	branch?: string;
	headShortHash?: string;
	latestCommitMessage?: string;
	latestCommitAuthor?: string;
	latestCommitTime?: string;
	dirty: boolean;
	uncommittedChangesCount: number;
	// Project-wide uncommitted files (all uncommitted, not just session changes)
	uncommittedFiles?: FileChangeRecord[];
}

export type SubagentRuntimeStatus = "idle" | "running" | "failed" | "completed";

export interface SubagentStatus {
	id: string;
	name: string;
	status: SubagentRuntimeStatus;
	taskSummary: string;
	startTime?: number;
	endTime?: number;
	elapsedTime?: number;
	errorMessage?: string;
	tokens?: number;
}

export type FileChangeStatus = "added" | "modified" | "deleted" | "renamed";
export type FileChangeSource = "agent" | "user" | "unknown";

export interface FileChangeRecord {
	path: string;
	status: FileChangeStatus;
	additions?: number;
	deletions?: number;
	source: FileChangeSource;
}

export interface ToolCallStats {
	toolName: string;
	callCount: number;
	successCount: number;
	failureCount: number;
	rejectedCount: number;
	lastCalledTime?: string;
	riskLevel?: PermissionRiskLevel;
	permissionDecisionCounts: Record<string, number>;
	lastErrorMessage?: string;
}

export interface SidebarState {
	project: ProjectInfo;
	permission: PermissionRuntimeState;
	modelUsage: ModelUsageState;
	gitStatus: GitStatus;
	activeSubagents: SubagentStatus[];
	// 当前正在查看的 subagent id（undefined = main agent）
	viewingSubagentId?: string;
	changedFiles: FileChangeRecord[];
	toolCallStats: ToolCallStats[];
	editMode: "normal" | "think_first";
}

export interface RuntimeStateSnapshot extends SidebarState {}

type RuntimeListener = (state: RuntimeStateSnapshot) => void;

function emptyGitStatus(): GitStatus {
	return {
		enabled: false,
		dirty: false,
		uncommittedChangesCount: 0,
	};
}

function emptyUsage(): ModelUsageState {
	return {
		promptTokens: 0,
		completionTokens: 0,
		reasoningTokens: 0,
		totalTokens: 0,
		currentTurnTokens: 0,
		sessionTotalTokens: 0,
	};
}

function execGit(cwd: string, args: string[]): Promise<string | undefined> {
	return new Promise((resolvePromise) => {
		execFile("git", args, { cwd, encoding: "utf8" }, (error, stdout) => {
			resolvePromise(error ? undefined : stdout.trim());
		});
	});
}

function parsePorcelainStatus(stdout: string | undefined): {
	dirty: boolean;
	count: number;
	files: FileChangeRecord[];
} {
	if (!stdout) return { dirty: false, count: 0, files: [] };
	const lines = stdout.split("\n").filter((line) => line.trim().length > 0);
	const files: FileChangeRecord[] = [];
	for (const line of lines) {
		const match = line.match(/^([ MADRC]?)([MADRC]?)\s+(.+)$/);
		if (!match) continue;
		const indexStatus = match[1];
		const workTreeStatus = match[2];
		const path = match[3];
		let status: FileChangeStatus = "modified";
		if (indexStatus === "A" || workTreeStatus === "A") status = "added";
		else if (indexStatus === "D" || workTreeStatus === "D") status = "deleted";
		else if (indexStatus === "R" || workTreeStatus === "R") status = "renamed";
		// Untracked files: ?? path
		if (line.startsWith("??")) status = "added";
		files.push({ path, status, source: "unknown" });
	}
	return { dirty: lines.length > 0, count: lines.length, files };
}

function parseNumstat(stdout: string | undefined): { additions?: number; deletions?: number } {
	const line = stdout?.split("\n").find((entry) => entry.trim().length > 0);
	if (!line) return {};
	const [additions, deletions] = line.split(/\s+/);
	const parsedAdditions = Number(additions);
	const parsedDeletions = Number(deletions);
	return {
		additions: Number.isFinite(parsedAdditions) ? parsedAdditions : undefined,
		deletions: Number.isFinite(parsedDeletions) ? parsedDeletions : undefined,
	};
}

function normalizeChangedPath(pathValue: string, workspaceRoot: string): string {
	const rel = relative(workspaceRoot, pathValue);
	return rel && !rel.startsWith("..") ? rel : pathValue;
}

export class RuntimeStateStore {
	private state: RuntimeStateSnapshot;
	private listeners = new Set<RuntimeListener>();
	private changedFiles = new Map<string, FileChangeRecord>();
	private toolStats = new Map<string, ToolCallStats>();
	private subagents = new Map<string, SubagentStatus>();

	constructor(options: { workspaceRoot: string; cwd: string; permissionMode: PermissionMode; sessionName?: string }) {
		this.state = {
			project: {
				projectName: basename(options.workspaceRoot),
				workspaceRoot: options.workspaceRoot,
				currentWorkingDirectory: options.cwd,
				sessionName: options.sessionName,
			},
			permission: {
				mode: options.permissionMode,
				whitelistSummary: "none",
				blacklistSummary: "none",
				sessionApprovals: 0,
			},
			modelUsage: emptyUsage(),
			gitStatus: emptyGitStatus(),
			activeSubagents: [],
			changedFiles: [],
			toolCallStats: [],
			editMode: "normal",
		};
	}

	subscribe(listener: RuntimeListener): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	getSnapshot(): RuntimeStateSnapshot {
		return structuredClone(this.state);
	}

	updateProject(project: Partial<ProjectInfo>): void {
		this.state.project = { ...this.state.project, ...project };
		this.notify();
	}

	updatePermission(permission: Partial<PermissionRuntimeState>): void {
		this.state.permission = { ...this.state.permission, ...permission };
		this.notify();
	}

	setEditMode(mode: "normal" | "think_first"): void {
		this.state.editMode = mode;
		this.notify();
	}

	recordPermissionDecision(toolName: string, decision: PermissionDecision, sessionApprovals: number): void {
		this.updatePermission({
			mode: decision.mode,
			lastDecision: decision,
			sessionApprovals,
		});
		const stats = this.getToolStats(toolName);
		stats.permissionDecisionCounts[decision.source] = (stats.permissionDecisionCounts[decision.source] ?? 0) + 1;
		this.flushToolStats();
	}

	recordToolRequested(toolName: string, riskLevel?: PermissionRiskLevel): void {
		const stats = this.getToolStats(toolName);
		stats.callCount += 1;
		stats.lastCalledTime = new Date().toISOString();
		stats.riskLevel = riskLevel ?? stats.riskLevel;
		this.flushToolStats();
	}

	recordToolFinished(toolName: string, isError: boolean, errorMessage?: string): void {
		const stats = this.getToolStats(toolName);
		if (isError) {
			stats.failureCount += 1;
			stats.lastErrorMessage = errorMessage;
			if (errorMessage?.includes("permission denied")) {
				stats.rejectedCount += 1;
			}
		} else {
			stats.successCount += 1;
		}
		stats.lastCalledTime = new Date().toISOString();
		this.flushToolStats();
	}

	recordToolRejected(toolName: string): void {
		const stats = this.getToolStats(toolName);
		stats.rejectedCount += 1;
		this.flushToolStats();
	}

	recordModelMessage(message: AgentMessage, currentTurn = false): void {
		if (message.role !== "assistant") return;
		const assistant = message as AssistantMessage;
		const usage = assistant.usage;
		const promptTokens = usage.input + usage.cacheRead + usage.cacheWrite;
		const completionTokens = usage.output;
		const totalTokens = usage.totalTokens ?? promptTokens + completionTokens;
		const reasoningTokens = 0;
		this.state.modelUsage = {
			...this.state.modelUsage,
			provider: assistant.provider,
			model: assistant.model,
			promptTokens: this.state.modelUsage.promptTokens + promptTokens,
			completionTokens: this.state.modelUsage.completionTokens + completionTokens,
			reasoningTokens: this.state.modelUsage.reasoningTokens + reasoningTokens,
			totalTokens: this.state.modelUsage.totalTokens + totalTokens,
			currentTurnTokens: currentTurn ? totalTokens : this.state.modelUsage.currentTurnTokens + totalTokens,
			sessionTotalTokens: this.state.modelUsage.sessionTotalTokens + totalTokens,
		};
		this.notify();
	}

	setCurrentModel(provider: string | undefined, model: string | undefined, reasoningEffort: string | undefined): void {
		this.state.modelUsage = { ...this.state.modelUsage, provider, model, reasoningEffort };
		this.notify();
	}

	resetCurrentTurnTokens(): void {
		this.state.modelUsage = { ...this.state.modelUsage, currentTurnTokens: 0 };
		this.notify();
	}

	recordSubagentStarted(input: { id: string; name: string; taskSummary: string }): void {
		this.subagents.set(input.id, {
			id: input.id,
			name: input.name,
			status: "running",
			taskSummary: input.taskSummary,
			startTime: Date.now(),
		});
		this.flushSubagents();
	}

	recordSubagentFinished(id: string, status: "completed" | "failed", errorMessage?: string): void {
		const current = this.subagents.get(id);
		if (!current) return;
		const endTime = Date.now();
		this.subagents.set(id, {
			...current,
			status,
			endTime,
			elapsedTime: current.startTime ? endTime - current.startTime : undefined,
			errorMessage,
		});
		this.flushSubagents();
	}

	recordFileChanged(pathValue: string, status: FileChangeStatus, source: FileChangeSource): void {
		const path = normalizeChangedPath(pathValue, this.state.project.workspaceRoot);
		this.changedFiles.set(path, {
			...(this.changedFiles.get(path) ?? { path, status, source }),
			path,
			status,
			source,
		});
		this.flushChangedFiles();
	}

	async refreshGitStatus(): Promise<void> {
		const cwd = this.state.project.currentWorkingDirectory;
		const root = await execGit(cwd, ["rev-parse", "--show-toplevel"]);
		if (!root) {
			this.state.gitStatus = emptyGitStatus();
			this.notify();
			return;
		}
		const [branch, head, latest, porcelain] = await Promise.all([
			execGit(cwd, ["branch", "--show-current"]),
			execGit(cwd, ["rev-parse", "--short", "HEAD"]),
			execGit(cwd, ["log", "-1", "--pretty=format:%s%x00%an%x00%cI"]),
			execGit(cwd, ["status", "--porcelain"]),
		]);
		const [message, author, time] = latest?.split("\0") ?? [];
		const status = parsePorcelainStatus(porcelain);
		// Fetch per-file line counts for all uncommitted files
		if (status.files.length > 0) {
			const numstatOutput = await execGit(cwd, ["diff", "--numstat", "--", ...status.files.map((f) => f.path)]);
			const numstatLines = numstatOutput?.split("\n") ?? [];
			const fileNumstat = new Map<string, { additions?: number; deletions?: number }>();
			for (const line of numstatLines) {
				const parts = line.split("\t");
				if (parts.length >= 3) {
					const [additions, deletions, path] = parts;
					const parsedAdditions = Number(additions);
					const parsedDeletions = Number(deletions);
					fileNumstat.set(path, {
						additions: Number.isFinite(parsedAdditions) ? parsedAdditions : undefined,
						deletions: Number.isFinite(parsedDeletions) ? parsedDeletions : undefined,
					});
				}
			}
			for (const file of status.files) {
				const nums = fileNumstat.get(file.path);
				if (nums) {
					file.additions = nums.additions;
					file.deletions = nums.deletions;
				}
			}
		}
		this.state.gitStatus = {
			enabled: true,
			branch: branch || "detached",
			headShortHash: head,
			latestCommitMessage: message,
			latestCommitAuthor: author,
			latestCommitTime: time,
			dirty: status.dirty,
			uncommittedChangesCount: status.count,
			uncommittedFiles: status.files,
		};
		this.notify();
	}

	async refreshFileDiff(pathValue: string): Promise<void> {
		const path = normalizeChangedPath(pathValue, this.state.project.workspaceRoot);
		const diff = await execGit(this.state.project.currentWorkingDirectory, ["diff", "--numstat", "--", path]);
		const current = this.changedFiles.get(path);
		if (!current) return;
		this.changedFiles.set(path, {
			...current,
			...parseNumstat(diff),
		});
		this.flushChangedFiles();
	}

	private getToolStats(toolName: string): ToolCallStats {
		const current = this.toolStats.get(toolName);
		if (current) return current;
		const next: ToolCallStats = {
			toolName,
			callCount: 0,
			successCount: 0,
			failureCount: 0,
			rejectedCount: 0,
			permissionDecisionCounts: {},
		};
		this.toolStats.set(toolName, next);
		return next;
	}

	private flushToolStats(): void {
		this.state.toolCallStats = Array.from(this.toolStats.values());
		this.notify();
	}

	private flushSubagents(): void {
		this.state.activeSubagents = Array.from(this.subagents.values());
		this.notify();
	}

	setViewingSubagent(id: string | undefined): void {
		this.state.viewingSubagentId = id;
		this.notify();
	}

	private flushChangedFiles(): void {
		this.state.changedFiles = Array.from(this.changedFiles.values());
		this.notify();
	}

	private notify(): void {
		const snapshot = this.getSnapshot();
		for (const listener of this.listeners) {
			listener(snapshot);
		}
	}
}
