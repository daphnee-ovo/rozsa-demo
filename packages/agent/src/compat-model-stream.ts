/**
 * Browser-safe TypeScript AI compatibility stream boundary.
 *
 * Structure:
 * - streamCompatModel(): delegates to the legacy TS AI stream implementation.
 *
 * Related docs: ../../../docs/model/rozsa-model-migration.md
 */

import {
	type Api,
	type Context,
	cleanupSessionResources,
	type Model,
	type SimpleStreamOptions,
	streamSimple,
} from "@earendil-works/pi-ai";
import type { AssistantMessageEventStream } from "./types.ts";

export function streamCompatModel(
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	return streamSimple(model, context, options);
}

export function cleanupCompatModelSessionResources(sessionId?: string): void {
	cleanupSessionResources(sessionId);
}
