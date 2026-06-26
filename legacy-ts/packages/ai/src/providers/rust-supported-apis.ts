import type { Api } from "../types.ts";

const RUST_MODEL_SUPPORTED_APIS = new Set<Api>(["openai-completions", "bedrock-converse-stream", "anthropic-messages"]);

/** Return whether an API has a Rust provider implementation behind the bridge. */
export function isRustModelSupportedApi(api: Api): boolean {
	return RUST_MODEL_SUPPORTED_APIS.has(api);
}
