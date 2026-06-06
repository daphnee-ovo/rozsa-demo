/**
 * Fail-fast stream boundary for Agent callers that did not inject model execution.
 *
 * Structure:
 * - missingModelStream(): returns a terminal assistant error stream.
 *
 * Related docs: ../../../docs/model/rozsa-model-migration.md
 */

import type {
	Api,
	AssistantMessage,
	AssistantMessageEvent,
	Context,
	Model,
	SimpleStreamOptions,
} from "@earendil-works/rozsa-model-types";
import { EventStream } from "./event-stream.ts";
import type { AssistantMessageEventStream } from "./types.ts";

function createMissingStreamMessage(model: Model<Api>): AssistantMessage {
	return {
		role: "assistant",
		content: [],
		api: model.api,
		provider: model.provider,
		model: model.id,
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "error",
		errorMessage:
			"Agent model execution requires an explicit streamFn. Use @earendil-works/rozsa-agent-core/node for the Rust model bridge.",
		timestamp: Date.now(),
	};
}

class MissingModelStream
	extends EventStream<AssistantMessageEvent, AssistantMessage>
	implements AssistantMessageEventStream
{
	constructor(message: AssistantMessage) {
		super(
			(event) => event.type === "done" || event.type === "error",
			(event) => {
				if (event.type === "done") return event.message;
				if (event.type === "error") return event.error;
				throw new Error("Unexpected event type");
			},
		);
		queueMicrotask(() => {
			this.push({ type: "error", reason: "error", error: message });
		});
	}
}

export function missingModelStream(
	model: Model<Api>,
	_context: Context,
	_options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	return new MissingModelStream(createMissingStreamMessage(model));
}
