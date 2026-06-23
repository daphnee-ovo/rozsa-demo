import type { AgentMessage } from "@earendil-works/rozsa-agent-core";
import type { SessionEntry } from "../../core/session-manager.ts";
import type { HostToNativeMessage, NativeGraphNode } from "./protocol.ts";

export function graphMessageFromEntries(entries: SessionEntry[]): HostToNativeMessage {
	const nodes = graphNodesFromEntries(entries);
	if (nodes.length === 0) {
		return { type: "notify", level: "info", message: "No messages in session" };
	}
	return { type: "graph", nodes };
}

function graphNodesFromEntries(entries: SessionEntry[]): NativeGraphNode[] {
	const nodes: NativeGraphNode[] = [];
	for (const entry of entries) {
		if (entry.type !== "message") continue;
		const role = classifyRole(entry.message);
		if (!role) continue;
		const fullText = extractFullText(entry.message);
		if (role === "assistant" && !fullText) continue;
		nodes.push({
			role,
			fullText,
			summary: fullText.replace(/[\n\t]+/g, " ").trim(),
			timestamp: formatTime(entry.timestamp),
		});
	}
	return nodes;
}

function classifyRole(message: AgentMessage): "user" | "assistant" | undefined {
	if (message.role === "user") return "user";
	if (message.role === "assistant") return "assistant";
	return undefined;
}

function extractFullText(message: AgentMessage): string {
	const content = (message as { content?: unknown }).content;
	if (typeof content === "string") return content.trim();
	if (!Array.isArray(content)) return "";
	const parts: string[] = [];
	for (const block of content) {
		if (typeof block === "object" && block !== null && "type" in block) {
			const typed = block as { type?: string; text?: unknown };
			if (typed.type === "text" && typeof typed.text === "string") {
				parts.push(typed.text);
			}
		}
	}
	return parts.join("\n").trim();
}

function formatTime(timestamp: string): string {
	const date = new Date(timestamp);
	if (Number.isNaN(date.valueOf())) return "";
	const h = date.getHours().toString().padStart(2, "0");
	const m = date.getMinutes().toString().padStart(2, "0");
	return `${h}:${m}`;
}
