/**
 * Default model stream boundary for the generic Agent package.
 *
 * Structure:
 * - streamDefaultModel(): routes all model requests to the Rust bridge.
 * - completeDefaultModel(): convenience wrapper returning the final message.
 *
 * Related docs: ../../../docs/model/rozsa-model-migration.md
 */

import type { Api, AssistantMessage, Context, Model, SimpleStreamOptions } from "@earendil-works/pi-model-types";
import { streamSimpleRustModel } from "./rozsa-model-client.ts";
import type { AssistantMessageEventStream } from "./types.ts";

export function streamDefaultModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	return streamSimpleRustModel(model, context, options);
}

export async function completeDefaultModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): Promise<AssistantMessage> {
	const stream = streamDefaultModel(model, context, options);
	return stream.result();
}
