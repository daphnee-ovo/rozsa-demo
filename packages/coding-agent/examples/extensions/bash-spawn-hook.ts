/**
 * Bash Spawn Hook Example
 *
 * Adjusts command, cwd, and env before execution.
 *
 * Usage:
 *   rozsa -e ./bash-spawn-hook.ts
 */

import type { ExtensionAPI } from "@earendil-works/rozsa-coding-agent";
import { createBashTool } from "@earendil-works/rozsa-coding-agent";

export default function (rozsa: ExtensionAPI) {
	const cwd = process.cwd();

	const bashTool = createBashTool(cwd, {
		spawnHook: ({ command, cwd, env }) => ({
			command: `source ~/.profile\n${command}`,
			cwd,
			env: { ...env, PI_SPAWN_HOOK: "1" },
		}),
	});

	rozsa.registerTool({
		...bashTool,
		execute: async (id, params, signal, onUpdate, _ctx) => {
			return bashTool.execute(id, params, signal, onUpdate);
		},
	});
}
