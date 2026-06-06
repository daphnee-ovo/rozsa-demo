export {
	type BashOperations,
	type BashSpawnContext,
	type BashSpawnHook,
	type BashToolDetails,
	type BashToolInput,
	type BashToolOptions,
	createBashTool,
	createBashToolDefinition,
	createLocalBashOperations,
} from "./bash.ts";
export {
	createEditTool,
	createEditToolDefinition,
	type EditOperations,
	type EditToolDetails,
	type EditToolInput,
	type EditToolOptions,
} from "./edit.ts";
export { withFileMutationQueue } from "./file-mutation-queue.ts";
export {
	createFindTool,
	createFindToolDefinition,
	type FindOperations,
	type FindToolDetails,
	type FindToolInput,
	type FindToolOptions,
} from "./find.ts";
export {
	createGrepTool,
	createGrepToolDefinition,
	type GrepOperations,
	type GrepToolDetails,
	type GrepToolInput,
	type GrepToolOptions,
} from "./grep.ts";
export {
	createLsTool,
	createLsToolDefinition,
	type LsOperations,
	type LsToolDetails,
	type LsToolInput,
	type LsToolOptions,
} from "./ls.ts";
export {
	createReadTool,
	createReadToolDefinition,
	type ReadOperations,
	type ReadToolDetails,
	type ReadToolInput,
	type ReadToolOptions,
} from "./read.ts";
export {
	createSubagentToolDefinition,
	type SubagentToolDetails,
	type SubagentToolInput,
	type SubagentToolOptions,
} from "./subagent.ts";
export {
	DEFAULT_MAX_BYTES,
	DEFAULT_MAX_LINES,
	formatSize,
	type TruncationOptions,
	type TruncationResult,
	truncateHead,
	truncateLine,
	truncateTail,
} from "./truncate.ts";
export {
	createWriteTool,
	createWriteToolDefinition,
	type WriteOperations,
	type WriteToolInput,
	type WriteToolOptions,
} from "./write.ts";

import type { AgentTool } from "@earendil-works/rozsa-agent-core";
import type { ToolDefinition } from "../extensions/types.ts";
import { type BashToolOptions, createBashTool, createBashToolDefinition } from "./bash.ts";
import { createEditTool, createEditToolDefinition, type EditToolOptions } from "./edit.ts";
import { createFindTool, createFindToolDefinition, type FindToolOptions } from "./find.ts";
import { createGrepTool, createGrepToolDefinition, type GrepToolOptions } from "./grep.ts";
import { createLsTool, createLsToolDefinition, type LsToolOptions } from "./ls.ts";
import { createReadTool, createReadToolDefinition, type ReadToolOptions } from "./read.ts";
import { createWriteTool, createWriteToolDefinition, type WriteToolOptions } from "./write.ts";

export type Tool = AgentTool<any>;
export type ToolDef = ToolDefinition<any, any>;
export type ToolName = "read" | "bash" | "edit" | "write" | "grep" | "find" | "ls";
export const allToolNames: Set<ToolName> = new Set(["read", "bash", "edit", "write", "grep", "find", "ls"]);

export interface ToolsOptions {
	read?: ReadToolOptions;
	bash?: BashToolOptions;
	write?: WriteToolOptions;
	edit?: EditToolOptions;
	grep?: GrepToolOptions;
	find?: FindToolOptions;
	ls?: LsToolOptions;
	/** 干跑模式：写操作 (bash/edit/write) 仅预览不执行 */
	dryRun?: boolean;
}

export function createToolDefinition(toolName: ToolName, cwd: string, options?: ToolsOptions): ToolDef {
	const dryRun = options?.dryRun;
	switch (toolName) {
		case "read":
			return createReadToolDefinition(cwd, options?.read);
		case "bash":
			return createBashToolDefinition(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) });
		case "edit":
			return createEditToolDefinition(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) });
		case "write":
			return createWriteToolDefinition(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) });
		case "grep":
			return createGrepToolDefinition(cwd, options?.grep);
		case "find":
			return createFindToolDefinition(cwd, options?.find);
		case "ls":
			return createLsToolDefinition(cwd, options?.ls);
		default:
			throw new Error(`Unknown tool name: ${toolName}`);
	}
}

export function createTool(toolName: ToolName, cwd: string, options?: ToolsOptions): Tool {
	const dryRun = options?.dryRun;
	switch (toolName) {
		case "read":
			return createReadTool(cwd, options?.read);
		case "bash":
			return createBashTool(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) });
		case "edit":
			return createEditTool(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) });
		case "write":
			return createWriteTool(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) });
		case "grep":
			return createGrepTool(cwd, options?.grep);
		case "find":
			return createFindTool(cwd, options?.find);
		case "ls":
			return createLsTool(cwd, options?.ls);
		default:
			throw new Error(`Unknown tool name: ${toolName}`);
	}
}

export function createCodingToolDefinitions(cwd: string, options?: ToolsOptions): ToolDef[] {
	const dryRun = options?.dryRun;
	return [
		createReadToolDefinition(cwd, options?.read),
		createBashToolDefinition(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) }),
		createEditToolDefinition(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) }),
		createWriteToolDefinition(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) }),
	];
}

export function createReadOnlyToolDefinitions(cwd: string, options?: ToolsOptions): ToolDef[] {
	return [
		createReadToolDefinition(cwd, options?.read),
		createGrepToolDefinition(cwd, options?.grep),
		createFindToolDefinition(cwd, options?.find),
		createLsToolDefinition(cwd, options?.ls),
	];
}

export function createAllToolDefinitions(cwd: string, options?: ToolsOptions): Record<ToolName, ToolDef> {
	const dryRun = options?.dryRun;
	return {
		read: createReadToolDefinition(cwd, options?.read),
		bash: createBashToolDefinition(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) }),
		edit: createEditToolDefinition(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) }),
		write: createWriteToolDefinition(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) }),
		grep: createGrepToolDefinition(cwd, options?.grep),
		find: createFindToolDefinition(cwd, options?.find),
		ls: createLsToolDefinition(cwd, options?.ls),
	};
}

export function createCodingTools(cwd: string, options?: ToolsOptions): Tool[] {
	const dryRun = options?.dryRun;
	return [
		createReadTool(cwd, options?.read),
		createBashTool(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) }),
		createEditTool(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) }),
		createWriteTool(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) }),
	];
}

export function createReadOnlyTools(cwd: string, options?: ToolsOptions): Tool[] {
	return [
		createReadTool(cwd, options?.read),
		createGrepTool(cwd, options?.grep),
		createFindTool(cwd, options?.find),
		createLsTool(cwd, options?.ls),
	];
}

export function createAllTools(cwd: string, options?: ToolsOptions): Record<ToolName, Tool> {
	const dryRun = options?.dryRun;
	return {
		read: createReadTool(cwd, options?.read),
		bash: createBashTool(cwd, { ...options?.bash, ...(dryRun !== undefined && { dryRun }) }),
		edit: createEditTool(cwd, { ...options?.edit, ...(dryRun !== undefined && { dryRun }) }),
		write: createWriteTool(cwd, { ...options?.write, ...(dryRun !== undefined && { dryRun }) }),
		grep: createGrepTool(cwd, options?.grep),
		find: createFindTool(cwd, options?.find),
		ls: createLsTool(cwd, options?.ls),
	};
}
