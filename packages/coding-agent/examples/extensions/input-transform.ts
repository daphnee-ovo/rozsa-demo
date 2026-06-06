/**
 * Input Transform Example - demonstrates the `input` event for intercepting user input.
 *
 * Start rozsa with this extension:
 *   rozsa -e ./examples/extensions/input-transform.ts
 *
 * Then type these inside rozsa:
 *   ?quick What is TypeScript?  → "Respond briefly: What is TypeScript?"
 *   ping                        → "pong" (instant, no LLM)
 *   time                        → current time (instant, no LLM)
 */
import type { ExtensionAPI } from "@earendil-works/rozsa-coding-agent";

export default function (rozsa: ExtensionAPI) {
	rozsa.on("input", async (event, ctx) => {
		// Source-based logic: skip processing for extension-injected messages
		if (event.source === "extension") {
			return { action: "continue" };
		}

		// Transform: ?quick prefix for brief responses
		if (event.text.startsWith("?quick ")) {
			const query = event.text.slice(7).trim();
			if (!query) {
				ctx.ui.notify("Usage: ?quick <question>", "warning");
				return { action: "handled" };
			}
			return { action: "transform", text: `Respond briefly in 1-2 sentences: ${query}` };
		}

		// Handle: instant responses without LLM (extension shows its own feedback)
		if (event.text.toLowerCase() === "ping") {
			ctx.ui.notify("pong", "info");
			return { action: "handled" };
		}
		if (event.text.toLowerCase() === "time") {
			ctx.ui.notify(new Date().toLocaleString(), "info");
			return { action: "handled" };
		}

		return { action: "continue" };
	});
}
