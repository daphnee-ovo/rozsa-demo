/**
 * Model stream boundary for coding-agent requests.
 *
 * Structure:
 * - streamResolvedModel(): dispatches resolved model requests to the Rust bridge.
 * - completeResolvedModel(): convenience wrapper returning the final message.
 *
 * Related docs: ../../../../docs/model/rozsa-model-migration.md
 */

import type { AssistantMessageEventStream } from "@earendil-works/rozsa-agent-core";
import { streamDefaultModel } from "@earendil-works/rozsa-agent-core/node";
import type { Api, AssistantMessage, Context, Model, SimpleStreamOptions } from "@earendil-works/rozsa-model-types";

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
	const stream = streamResolvedModel(model, context, options);
	return stream.result();
}
