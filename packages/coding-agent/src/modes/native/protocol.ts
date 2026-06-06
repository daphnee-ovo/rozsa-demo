import type { AgentMessage } from "@earendil-works/rozsa-agent-core";
import type { Api, ImageContent, Model } from "@earendil-works/rozsa-model-types";
import type { SessionStats } from "../../core/agent-session.ts";
import type { ContextUsage } from "../../core/extensions/types.ts";
import type { PermissionPromptContext, PermissionRequest, UserPermissionChoice } from "../../core/permissions.ts";
import type { RuntimeStateSnapshot } from "../../core/runtime-state.ts";
import type { NativeKeybindings } from "./native-keybindings.ts";

export interface NativeUiState {
	appName: string;
	version: string;
	cwd: string;
	sessionName?: string;
	model?: Model<Api>;
	thinkingLevel: string;
	isStreaming: boolean;
	isCompacting: boolean;
	hideThinking: boolean;
	showImages: boolean;
	messages: AgentMessage[];
	pendingMessages: string[];
	status: Record<string, string>;
	widgetsAbove: Record<string, string[]>;
	widgetsBelow: Record<string, string[]>;
	stats: SessionStats;
	runtimeState: RuntimeStateSnapshot;
	contextUsage?: ContextUsage;
	keybindings: NativeKeybindings;
	error?: string;
}

export interface NativeSessionEntry {
	path: string;
	name?: string;
	firstMessage: string;
	cwd: string;
	messageCount: number;
	lastModified: string;
	parentSessionPath?: string;
	allMessagesText: string;
}

export interface NativeModelEntry {
	id: string;
	provider: string;
	is_current: boolean;
}

export interface NativeAutocompleteItem {
	value: string;
	label: string;
	description?: string;
}

export type HostToNativeMessage =
	| { type: "state"; state: NativeUiState }
	| {
			type: "dialog";
			id: string;
			kind: "select" | "confirm" | "input" | "editor";
			title: string;
			message?: string;
			options?: string[];
			text?: string;
	  }
	| { type: "notify"; level: "info" | "warning" | "error"; message: string }
	| { type: "set_title"; title: string }
	| { type: "set_input"; text: string }
	| { type: "autocomplete"; id: number; prefix: string; items: NativeAutocompleteItem[] }
	| { type: "permission"; prompt: NativePermissionPrompt }
	| { type: "graph"; nodes: NativeGraphNode[] }
	| { type: "sessions"; entries: NativeSessionEntry[]; currentSessionPath: string }
	| { type: "session_deleted"; path: string; method: "trash" | "unlink"; error?: string }
	| { type: "models"; entries: NativeModelEntry[] }
	| { type: "compacting"; active: boolean }
	| { type: "retry"; seconds: number; reason: string }
	| { type: "shutdown" };

export type NativeToHostMessage =
	| { type: "submit"; text: string; images?: ImageContent[] }
	| { type: "autocomplete_request"; id: number; text: string; cursor: number; force: boolean }
	| { type: "follow_up"; text: string; images?: ImageContent[] }
	| { type: "steer"; text: string; images?: ImageContent[] }
	| { type: "bash"; command: string }
	| { type: "abort" }
	| { type: "compact" }
	| { type: "cycle_model"; direction: "forward" | "backward" }
	| { type: "cycle_thinking" }
	| { type: "cycle_edit_mode" }
	| { type: "dialog_response"; id: string; value?: string; confirmed?: boolean; cancelled?: boolean }
	| { type: "permission_response"; id: string; choice: UserPermissionChoice; trustKey?: string }
	| { type: "switch_agent"; id: string }
	| { type: "switch_model"; provider?: string; id: string }
	| { type: "switch_session"; path: string }
	| { type: "delete_session"; path: string }
	| { type: "rename_session"; path: string; name: string }
	| { type: "list_sessions"; scope?: "current" | "all" }
	| { type: "list_models" }
	| { type: "exit" };

export interface NativePermissionPrompt {
	id: string;
	request: PermissionRequest;
	context: PermissionPromptContext;
	trustLevels: { label: string; key: string }[];
}

export interface NativeGraphNode {
	role: "user" | "assistant";
	summary: string;
	fullText: string;
	timestamp: string;
}
