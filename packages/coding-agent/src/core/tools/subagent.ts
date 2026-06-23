import { Text } from "@earendil-works/rozsa-tui";
import { type Static, Type } from "typebox";
import { type Theme, theme } from "../../modes/interactive/theme/theme.ts";
import type { ToolDefinition, ToolRenderResultOptions } from "../extensions/types.ts";
import { getTextOutput, invalidArgText, str } from "./render-utils.ts";

const subagentScopeSchema = Type.Optional(
	Type.Union(
		[
			Type.Literal("inherit"),
			Type.Literal("readonly"),
			Type.Object({
				type: Type.Literal("scoped"),
				paths: Type.Array(Type.String(), {
					description: "File/directory paths the subagent can read and write within",
				}),
			}),
			Type.Object({
				type: Type.Literal("custom"),
				tools: Type.Optional(
					Type.Array(Type.String(), { description: "Allowed tool names (e.g. ['read', 'edit', 'bash'])" }),
				),
				paths: Type.Optional(
					Type.Array(Type.String(), { description: "Allowed file/directory paths for read/write/edit" }),
				),
				bash_prefixes: Type.Optional(
					Type.Array(Type.String(), {
						description: "Allowed bash command prefixes (e.g. ['npm test', 'git status'])",
					}),
				),
				skills: Type.Optional(Type.Array(Type.String(), { description: "Allowed skill names" })),
			}),
		],
		{ description: "Scope/permission level. Default: 'inherit'" },
	),
);

const subagentSchema = Type.Object({
	action: Type.Union(
		[
			Type.Literal("spawn"),
			Type.Literal("send"),
			Type.Literal("wait"),
			Type.Literal("interrupt"),
			Type.Literal("list"),
		],
		{ description: "Subagent action to perform" },
	),
	id: Type.Optional(Type.String({ description: "Subagent id for send, wait, or interrupt" })),
	name: Type.Optional(Type.String({ description: "Short human-readable name for a new subagent" })),
	model: Type.Optional(
		Type.String({
			description:
				"Optional model reference for a new subagent. Supports the same model patterns as --model, including provider/model and optional :thinking suffix.",
		}),
	),
	thinking_level: Type.Optional(
		Type.Union(
			[
				Type.Literal("off"),
				Type.Literal("minimal"),
				Type.Literal("low"),
				Type.Literal("medium"),
				Type.Literal("high"),
				Type.Literal("xhigh"),
			],
			{ description: "Optional thinking level for a new subagent. Overrides any :thinking suffix in model." },
		),
	),
	scope: subagentScopeSchema,
	system_prompt: Type.Optional(Type.String({ description: "System prompt for a new subagent" })),
	prompt: Type.Optional(Type.String({ description: "User prompt to send to the subagent" })),
	wait: Type.Optional(Type.Boolean({ description: "Wait for the subagent to become idle before returning" })),
});

export type SubagentToolInput = Static<typeof subagentSchema>;

export interface SubagentToolDetails {
	action: SubagentToolInput["action"];
	id?: string;
	status?: string;
	model?: { provider: string; id: string };
	thinkingLevel?: string;
	subagents?: Array<{
		id: string;
		name: string;
		status: string;
		model?: { provider: string; id: string };
		thinkingLevel?: string;
	}>;
}

export interface SubagentToolOptions {
	execute: (
		params: SubagentToolInput,
		signal: AbortSignal | undefined,
	) => Promise<{ content: string; details: SubagentToolDetails }>;
}

function formatSubagentCall(args: Partial<SubagentToolInput> | undefined, uiTheme: Theme): string {
	const action = str(args?.action) ?? "subagent";
	const id = str(args?.id);
	const name = str(args?.name);
	const invalidArg = invalidArgText(uiTheme);
	const target = id ?? name ?? (action === "spawn" || action === "list" ? undefined : invalidArg);
	return [
		uiTheme.fg("toolTitle", uiTheme.bold("subagent")),
		uiTheme.fg("accent", action),
		target ? uiTheme.fg(target === invalidArg ? "error" : "muted", target) : undefined,
	]
		.filter((part): part is string => part !== undefined)
		.join(" ");
}

function formatSubagentResult(
	result: { content: Array<{ type: string; text?: string }> },
	options: ToolRenderResultOptions,
): string {
	const output = getTextOutput(result, false);
	if (!options.expanded) {
		return "";
	}
	return output ? `\n${theme.fg("toolOutput", output)}` : "";
}

export function createSubagentToolDefinition(
	options: SubagentToolOptions,
): ToolDefinition<typeof subagentSchema, SubagentToolDetails> {
	return {
		name: "subagent",
		label: "subagent",
		description:
			"Create, message, wait for, list, or interrupt independent subagents. Use spawn with a focused system_prompt and optional prompt to delegate work. Subagents run with their own transcript and can be inspected or interrupted by the user in the TUI.",
		promptSnippet: "Delegate work to independent subagents with custom system prompts",
		promptGuidelines: [
			"Use subagent for independent investigations or parallel work that should keep a separate context.",
			"Give each spawned subagent a focused system_prompt and a clear prompt.",
			"Use optional model and thinking_level on spawn when the task needs a cheaper, faster, or stronger subagent.",
			"Use wait when you need a subagent's current result before continuing.",
			'Use scope to restrict subagent capabilities: "readonly" for research-only, { type: "scoped", paths: [...] } for path-limited read/write, or { type: "custom", tools, paths, bash_prefixes, skills } for full control.',
		],
		parameters: subagentSchema,
		async execute(_toolCallId, params, signal) {
			const result = await options.execute(params, signal);
			return {
				content: [{ type: "text", text: result.content }],
				details: result.details,
			};
		},
		renderCall: (args, uiTheme) => new Text(formatSubagentCall(args, uiTheme), 1, 0),
		renderResult: (result, options) => new Text(formatSubagentResult(result, options), 1, 0),
	};
}
