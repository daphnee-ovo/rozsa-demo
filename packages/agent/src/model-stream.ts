/**
 * Default model stream boundary for the generic Agent package.
 *
 * Structure:
 * - shouldStreamViaRustModel(): checks the configured Rust backend/API gate.
 * - streamDefaultModel(): dispatches model requests to Rust or TS.
 *
 * Related docs: ../../../docs/model/rozsa-model-migration.md
 */

import type { Api, AssistantMessage, Context, Model, SimpleStreamOptions } from "@earendil-works/pi-ai";
import { streamCompatModel } from "./compat-model-stream.ts";
import { shouldUseRustModelProvider, streamSimpleRustModel } from "./rozsa-model-client.ts";
import type { AssistantMessageEventStream } from "./types.ts";

export function shouldStreamViaRustModel(model: Model<Api>): boolean {
	return shouldUseRustModelProvider(model.api);
}

export function streamDefaultModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	if (shouldStreamViaRustModel(model)) {
		return streamSimpleRustModel(model, context, options);
	}
	return streamCompatModel(model, context, options);
}

export async function completeDefaultModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): Promise<AssistantMessage> {
	const stream = await streamDefaultModel(model, context, options);
	return stream.result();
}
