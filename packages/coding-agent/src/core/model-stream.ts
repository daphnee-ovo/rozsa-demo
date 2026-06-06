/**
 * Model stream boundary for coding-agent requests.
 *
 * Structure:
 * - shouldStreamViaRustModel(): checks the configured Rust backend/API gate.
 * - streamResolvedModel(): dispatches resolved model requests to Rust or TS.
 *
 * Related docs: ../../../../docs/model/rozsa-model-migration.md
 */

import type { AssistantMessageEventStream } from "@earendil-works/pi-agent-core";
import { shouldStreamViaRustModel, streamDefaultModel } from "@earendil-works/pi-agent-core/node";
import type { Api, AssistantMessage, Context, Model, SimpleStreamOptions } from "@earendil-works/pi-ai";

export { shouldStreamViaRustModel };

export function streamResolvedModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	return streamDefaultModel(model, context, options);
}

export async function completeResolvedModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): Promise<AssistantMessage> {
	const stream = await streamResolvedModel(model, context, options);
	return stream.result();
}
