import {
	type AutocompleteItem,
	type AutocompleteProvider,
	CombinedAutocompleteProvider,
	fuzzyFilter,
	type SlashCommand,
} from "@earendil-works/rozsa-tui";
import type { AgentSession } from "../../core/agent-session.ts";
import type { AutocompleteProviderFactory } from "../../core/extensions/index.ts";
import { BUILTIN_SLASH_COMMANDS } from "../../core/slash-commands.ts";
import type { NativeAutocompleteItem } from "./protocol.ts";

export async function getNativeAutocomplete(
	session: AgentSession,
	wrappers: AutocompleteProviderFactory[],
	request: { text: string; cursor: number; force: boolean },
	fdPath?: string | null,
): Promise<{ prefix: string; items: NativeAutocompleteItem[] }> {
	let provider = createNativeAutocompleteProvider(session, fdPath ?? null);
	for (const wrapper of wrappers) {
		provider = wrapper(provider);
	}
	const cursorCol = charIndexToOffset(request.text, request.cursor);
	const suggestions = await provider.getSuggestions([request.text], 0, cursorCol, {
		signal: new AbortController().signal,
		force: request.force,
	});
	return {
		prefix: suggestions?.prefix ?? "",
		items:
			suggestions?.items.map((item) => ({
				value: item.value,
				label: item.label,
				description: item.description,
			})) ?? [],
	};
}

// Rust TUI 本地命令，需要出现在 autocomplete 列表但由 Rust 端拦截执行
const NATIVE_LOCAL_COMMANDS: ReadonlyArray<{ name: string; description: string }> = [
	{ name: "theme", description: "Toggle dark/light theme" },
];

function createNativeAutocompleteProvider(session: AgentSession, fdPath: string | null): AutocompleteProvider {
	const slashCommands: SlashCommand[] = [
		...BUILTIN_SLASH_COMMANDS.map((command) => ({
			name: command.name,
			description: command.description,
			...(command.usage && { argumentHint: command.usage }),
		})),
		...NATIVE_LOCAL_COMMANDS.map((command) => ({
			name: command.name,
			description: command.description,
		})),
	];

	const modelCommand = slashCommands.find((command) => command.name === "model");
	if (modelCommand) {
		modelCommand.getArgumentCompletions = (prefix: string): AutocompleteItem[] | null => {
			const models =
				session.scopedModels.length > 0
					? session.scopedModels.map((scoped) => scoped.model)
					: session.modelRegistry.getAvailable();
			const filtered = fuzzyFilter(
				models.map((model) => ({
					id: model.id,
					provider: model.provider,
					label: `${model.provider}/${model.id}`,
				})),
				prefix,
				(item) => `${item.id} ${item.provider}`,
			);
			return filtered.length === 0
				? null
				: filtered.map((item) => ({
						value: item.label,
						label: item.id,
						description: item.provider,
					}));
		};
	}

	const builtinCommandNames = new Set(slashCommands.map((command) => command.name));
	const extensionCommands: SlashCommand[] = session.extensionRunner
		.getRegisteredCommands()
		.filter((command) => !builtinCommandNames.has(command.name))
		.map((command) => ({
			name: command.invocationName,
			description: command.description,
			getArgumentCompletions: command.getArgumentCompletions,
		}));
	const templateCommands: SlashCommand[] = session.promptTemplates.map((template) => ({
		name: template.name,
		description: template.description,
		...(template.argumentHint && { argumentHint: template.argumentHint }),
	}));
	const skillCommands: SlashCommand[] = session.settingsManager.getEnableSkillCommands()
		? session.resourceLoader.getSkills().skills.map((skill) => ({
				name: `skill:${skill.name}`,
				description: skill.description,
			}))
		: [];

	return new CombinedAutocompleteProvider(
		[...slashCommands, ...templateCommands, ...extensionCommands, ...skillCommands],
		session.sessionManager.getCwd(),
		fdPath,
	);
}

function charIndexToOffset(text: string, charIndex: number): number {
	return Array.from(text).slice(0, charIndex).join("").length;
}
