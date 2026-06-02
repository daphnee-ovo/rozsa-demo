import { type KeybindingsConfig, KeybindingsManager } from "../../core/keybindings.ts";

export type NativeKeybindings = Record<string, string[]>;

export function loadNativeKeybindings(): NativeKeybindings {
	return normalizeKeybindings(KeybindingsManager.create().getEffectiveConfig());
}

function normalizeKeybindings(config: KeybindingsConfig): NativeKeybindings {
	const result: NativeKeybindings = {};
	for (const [action, keys] of Object.entries(config)) {
		if (typeof keys === "string") {
			result[action] = [keys];
		} else if (Array.isArray(keys)) {
			result[action] = keys;
		} else {
			result[action] = [];
		}
	}
	return result;
}
