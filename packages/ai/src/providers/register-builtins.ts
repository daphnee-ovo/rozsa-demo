import { clearApiProviders, registerApiProvider } from "../api-registry.ts";
import type {
	Api,
	AssistantMessage,
	AssistantMessageEvent,
	Context,
	Model,
	SimpleStreamOptions,
	StreamFunction,
	StreamOptions,
} from "../types.ts";
import { AssistantMessageEventStream } from "../utils/event-stream.ts";
import type { BedrockOptions } from "./amazon-bedrock.ts";
import type { AnthropicOptions } from "./anthropic.ts";
import type { AzureOpenAIResponsesOptions } from "./azure-openai-responses.ts";
import type { GoogleOptions } from "./google.ts";
import type { GoogleVertexOptions } from "./google-vertex.ts";
import type { MistralOptions } from "./mistral.ts";
import type { OpenAICodexResponsesOptions } from "./openai-codex-responses.ts";
import type { OpenAICompletionsOptions } from "./openai-completions.ts";
import type { OpenAIResponsesOptions } from "./openai-responses.ts";
import { isRustModelSupportedApi } from "./rust-supported-apis.ts";

interface LazyProviderModule<
	TApi extends Api,
	TOptions extends StreamOptions,
	TSimpleOptions extends SimpleStreamOptions,
> {
	stream: (model: Model<TApi>, context: Context, options?: TOptions) => AsyncIterable<AssistantMessageEvent>;
	streamSimple: (
		model: Model<TApi>,
		context: Context,
		options?: TSimpleOptions,
	) => AsyncIterable<AssistantMessageEvent>;
}

interface AnthropicProviderModule {
	streamAnthropic: StreamFunction<"anthropic-messages", AnthropicOptions>;
	streamSimpleAnthropic: StreamFunction<"anthropic-messages", SimpleStreamOptions>;
}

interface AzureOpenAIResponsesProviderModule {
	streamAzureOpenAIResponses: StreamFunction<"azure-openai-responses", AzureOpenAIResponsesOptions>;
	streamSimpleAzureOpenAIResponses: StreamFunction<"azure-openai-responses", SimpleStreamOptions>;
}

interface GoogleProviderModule {
	streamGoogle: StreamFunction<"google-generative-ai", GoogleOptions>;
	streamSimpleGoogle: StreamFunction<"google-generative-ai", SimpleStreamOptions>;
}

interface GoogleVertexProviderModule {
	streamGoogleVertex: StreamFunction<"google-vertex", GoogleVertexOptions>;
	streamSimpleGoogleVertex: StreamFunction<"google-vertex", SimpleStreamOptions>;
}

interface MistralProviderModule {
	streamMistral: StreamFunction<"mistral-conversations", MistralOptions>;
	streamSimpleMistral: StreamFunction<"mistral-conversations", SimpleStreamOptions>;
}

interface OpenAICodexResponsesProviderModule {
	streamOpenAICodexResponses: StreamFunction<"openai-codex-responses", OpenAICodexResponsesOptions>;
	streamSimpleOpenAICodexResponses: StreamFunction<"openai-codex-responses", SimpleStreamOptions>;
}

interface OpenAICompletionsProviderModule {
	streamOpenAICompletions: StreamFunction<"openai-completions", OpenAICompletionsOptions>;
	streamSimpleOpenAICompletions: StreamFunction<"openai-completions", SimpleStreamOptions>;
}

interface RustModelBridgeModule {
	streamRustModel: StreamFunction<Api, StreamOptions>;
	streamSimpleRustModel: StreamFunction<Api, SimpleStreamOptions>;
	shouldUseRustModelProvider: (api: Api) => boolean;
}

interface OpenAIResponsesProviderModule {
	streamOpenAIResponses: StreamFunction<"openai-responses", OpenAIResponsesOptions>;
	streamSimpleOpenAIResponses: StreamFunction<"openai-responses", SimpleStreamOptions>;
}

interface BedrockProviderModule {
	streamBedrock: (
		model: Model<"bedrock-converse-stream">,
		context: Context,
		options?: BedrockOptions,
	) => AsyncIterable<AssistantMessageEvent>;
	streamSimpleBedrock: (
		model: Model<"bedrock-converse-stream">,
		context: Context,
		options?: SimpleStreamOptions,
	) => AsyncIterable<AssistantMessageEvent>;
}

const importNodeOnlyProvider = (specifier: string): Promise<unknown> => {
	const runtimeSpecifier = import.meta.url.endsWith(".js") ? specifier.replace(/\.ts$/, ".js") : specifier;
	return import(runtimeSpecifier);
};

let anthropicProviderModulePromise:
	| Promise<LazyProviderModule<"anthropic-messages", AnthropicOptions, SimpleStreamOptions>>
	| undefined;
let azureOpenAIResponsesProviderModulePromise:
	| Promise<LazyProviderModule<"azure-openai-responses", AzureOpenAIResponsesOptions, SimpleStreamOptions>>
	| undefined;
let googleProviderModulePromise:
	| Promise<LazyProviderModule<"google-generative-ai", GoogleOptions, SimpleStreamOptions>>
	| undefined;
let googleVertexProviderModulePromise:
	| Promise<LazyProviderModule<"google-vertex", GoogleVertexOptions, SimpleStreamOptions>>
	| undefined;
let mistralProviderModulePromise:
	| Promise<LazyProviderModule<"mistral-conversations", MistralOptions, SimpleStreamOptions>>
	| undefined;
let openAICodexResponsesProviderModulePromise:
	| Promise<LazyProviderModule<"openai-codex-responses", OpenAICodexResponsesOptions, SimpleStreamOptions>>
	| undefined;
let openAICompletionsProviderModulePromise:
	| Promise<LazyProviderModule<"openai-completions", OpenAICompletionsOptions, SimpleStreamOptions>>
	| undefined;
let openAIResponsesProviderModulePromise:
	| Promise<LazyProviderModule<"openai-responses", OpenAIResponsesOptions, SimpleStreamOptions>>
	| undefined;
let bedrockProviderModuleOverride:
	| LazyProviderModule<"bedrock-converse-stream", BedrockOptions, SimpleStreamOptions>
	| undefined;
let bedrockProviderModulePromise:
	| Promise<LazyProviderModule<"bedrock-converse-stream", BedrockOptions, SimpleStreamOptions>>
	| undefined;

export function setBedrockProviderModule(module: BedrockProviderModule): void {
	bedrockProviderModuleOverride = {
		stream: module.streamBedrock,
		streamSimple: module.streamSimpleBedrock,
	};
}

function forwardStream(target: AssistantMessageEventStream, source: AsyncIterable<AssistantMessageEvent>): void {
	(async () => {
		for await (const event of source) {
			target.push(event);
		}
		target.end();
	})();
}

function createLazyLoadErrorMessage<TApi extends Api>(model: Model<TApi>, error: unknown): AssistantMessage {
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
		errorMessage: error instanceof Error ? error.message : String(error),
		timestamp: Date.now(),
	};
}

function createLazyStream<TApi extends Api, TOptions extends StreamOptions, TSimpleOptions extends SimpleStreamOptions>(
	loadModule: () => Promise<LazyProviderModule<TApi, TOptions, TSimpleOptions>>,
): StreamFunction<TApi, TOptions> {
	return (model, context, options) => {
		const outer = new AssistantMessageEventStream();

		loadModule()
			.then((module) => {
				const inner = module.stream(model, context, options);
				forwardStream(outer, inner);
			})
			.catch((error) => {
				const message = createLazyLoadErrorMessage(model, error);
				outer.push({ type: "error", reason: "error", error: message });
				outer.end(message);
			});

		return outer;
	};
}

function createLazySimpleStream<
	TApi extends Api,
	TOptions extends StreamOptions,
	TSimpleOptions extends SimpleStreamOptions,
>(loadModule: () => Promise<LazyProviderModule<TApi, TOptions, TSimpleOptions>>): StreamFunction<TApi, TSimpleOptions> {
	return (model, context, options) => {
		const outer = new AssistantMessageEventStream();

		loadModule()
			.then((module) => {
				const inner = module.streamSimple(model, context, options);
				forwardStream(outer, inner);
			})
			.catch((error) => {
				const message = createLazyLoadErrorMessage(model, error);
				outer.push({ type: "error", reason: "error", error: message });
				outer.end(message);
			});

		return outer;
	};
}

function loadAnthropicProviderModule(): Promise<
	LazyProviderModule<"anthropic-messages", AnthropicOptions, SimpleStreamOptions>
> {
	anthropicProviderModulePromise ||= import("./anthropic.ts").then((module) => {
		const provider = module as AnthropicProviderModule;
		return {
			stream: provider.streamAnthropic,
			streamSimple: provider.streamSimpleAnthropic,
		};
	});
	return anthropicProviderModulePromise;
}

function loadAzureOpenAIResponsesProviderModule(): Promise<
	LazyProviderModule<"azure-openai-responses", AzureOpenAIResponsesOptions, SimpleStreamOptions>
> {
	azureOpenAIResponsesProviderModulePromise ||= import("./azure-openai-responses.ts").then((module) => {
		const provider = module as AzureOpenAIResponsesProviderModule;
		return {
			stream: provider.streamAzureOpenAIResponses,
			streamSimple: provider.streamSimpleAzureOpenAIResponses,
		};
	});
	return azureOpenAIResponsesProviderModulePromise;
}

function loadGoogleProviderModule(): Promise<
	LazyProviderModule<"google-generative-ai", GoogleOptions, SimpleStreamOptions>
> {
	googleProviderModulePromise ||= import("./google.ts").then((module) => {
		const provider = module as GoogleProviderModule;
		return {
			stream: provider.streamGoogle,
			streamSimple: provider.streamSimpleGoogle,
		};
	});
	return googleProviderModulePromise;
}

function loadGoogleVertexProviderModule(): Promise<
	LazyProviderModule<"google-vertex", GoogleVertexOptions, SimpleStreamOptions>
> {
	googleVertexProviderModulePromise ||= import("./google-vertex.ts").then((module) => {
		const provider = module as GoogleVertexProviderModule;
		return {
			stream: provider.streamGoogleVertex,
			streamSimple: provider.streamSimpleGoogleVertex,
		};
	});
	return googleVertexProviderModulePromise;
}

function loadMistralProviderModule(): Promise<
	LazyProviderModule<"mistral-conversations", MistralOptions, SimpleStreamOptions>
> {
	mistralProviderModulePromise ||= import("./mistral.ts").then((module) => {
		const provider = module as MistralProviderModule;
		return {
			stream: provider.streamMistral,
			streamSimple: provider.streamSimpleMistral,
		};
	});
	return mistralProviderModulePromise;
}

function loadOpenAICodexResponsesProviderModule(): Promise<
	LazyProviderModule<"openai-codex-responses", OpenAICodexResponsesOptions, SimpleStreamOptions>
> {
	openAICodexResponsesProviderModulePromise ||= import("./openai-codex-responses.ts").then((module) => {
		const provider = module as OpenAICodexResponsesProviderModule;
		return {
			stream: provider.streamOpenAICodexResponses,
			streamSimple: provider.streamSimpleOpenAICodexResponses,
		};
	});
	return openAICodexResponsesProviderModulePromise;
}

function loadOpenAICompletionsProviderModule(): Promise<
	LazyProviderModule<"openai-completions", OpenAICompletionsOptions, SimpleStreamOptions>
> {
	if (shouldAttemptRustModelProvider("openai-completions")) {
		return importNodeOnlyProvider("./rozsa-model-bridge.ts").then((module) => {
			const bridge = module as RustModelBridgeModule;
			if (bridge.shouldUseRustModelProvider("openai-completions")) {
				const isStrictRust = process.env.ROZSA_MODEL_BACKEND === "rust";
				return {
					stream: (model, context, options) => {
						if (isStrictRust && hasPayloadOrResponseHook(options)) {
							return createUnsupportedRustCallbackStream(model);
						}
						if (!isStrictRust && hasPayloadOrResponseHook(options)) {
							return streamViaTypeScriptOpenAICompletions(model, context, options);
						}
						return bridge.streamRustModel(model, context, options);
					},
					streamSimple: (model, context, options) => {
						if (!isStrictRust && hasPayloadOrResponseHook(options)) {
							return streamSimpleViaTypeScriptOpenAICompletions(model, context, options);
						}
						return bridge.streamSimpleRustModel(model, context, options);
					},
				};
			}
			return loadTypeScriptOpenAICompletionsProviderModule();
		});
	}
	return loadTypeScriptOpenAICompletionsProviderModule();
}

function loadTypeScriptOpenAICompletionsProviderModule(): Promise<
	LazyProviderModule<"openai-completions", OpenAICompletionsOptions, SimpleStreamOptions>
> {
	openAICompletionsProviderModulePromise ||= import("./openai-completions.ts").then((module) => {
		const provider = module as OpenAICompletionsProviderModule;
		return {
			stream: provider.streamOpenAICompletions,
			streamSimple: provider.streamSimpleOpenAICompletions,
		};
	});
	return openAICompletionsProviderModulePromise;
}

function hasPayloadOrResponseHook(options?: StreamOptions): boolean {
	return !!options?.onPayload || !!options?.onResponse;
}

function createUnsupportedRustCallbackStream<TApi extends Api>(model: Model<TApi>): AssistantMessageEventStream {
	const stream = new AssistantMessageEventStream();
	const message = createLazyLoadErrorMessage(
		model,
		"ROZSA_MODEL_BACKEND=rust does not support onPayload/onResponse callbacks yet.",
	);
	stream.push({ type: "error", reason: "error", error: message });
	stream.end(message);
	return stream;
}

function streamViaTypeScriptOpenAICompletions(
	model: Model<"openai-completions">,
	context: Context,
	options?: OpenAICompletionsOptions,
): AssistantMessageEventStream {
	const outer = new AssistantMessageEventStream();
	loadTypeScriptOpenAICompletionsProviderModule()
		.then((module) => forwardStream(outer, module.stream(model, context, options)))
		.catch((error) => {
			const message = createLazyLoadErrorMessage(model, error);
			outer.push({ type: "error", reason: "error", error: message });
			outer.end(message);
		});
	return outer;
}

function streamSimpleViaTypeScriptOpenAICompletions(
	model: Model<"openai-completions">,
	context: Context,
	options?: SimpleStreamOptions,
): AssistantMessageEventStream {
	const outer = new AssistantMessageEventStream();
	loadTypeScriptOpenAICompletionsProviderModule()
		.then((module) => forwardStream(outer, module.streamSimple(model, context, options)))
		.catch((error) => {
			const message = createLazyLoadErrorMessage(model, error);
			outer.push({ type: "error", reason: "error", error: message });
			outer.end(message);
		});
	return outer;
}

function shouldAttemptRustModelProvider(api: Api): boolean {
	if (typeof process === "undefined") return false;
	const backend = process.env.ROZSA_MODEL_BACKEND ?? "ts";
	if (backend === "ts") return false;
	if (backend !== "rust") {
		throw new Error('ROZSA_MODEL_BACKEND must be "ts" or "rust".');
	}
	if (!isRustModelSupportedApi(api)) return false;
	const rawApis = process.env.ROZSA_MODEL_RUST_APIS;
	if (!rawApis) return false;
	if (
		!rawApis
			.split(",")
			.map((candidate) => candidate.trim())
			.includes(api)
	) {
		return false;
	}
	return true;
}

function loadOpenAIResponsesProviderModule(): Promise<
	LazyProviderModule<"openai-responses", OpenAIResponsesOptions, SimpleStreamOptions>
> {
	openAIResponsesProviderModulePromise ||= import("./openai-responses.ts").then((module) => {
		const provider = module as OpenAIResponsesProviderModule;
		return {
			stream: provider.streamOpenAIResponses,
			streamSimple: provider.streamSimpleOpenAIResponses,
		};
	});
	return openAIResponsesProviderModulePromise;
}

function loadBedrockProviderModule(): Promise<
	LazyProviderModule<"bedrock-converse-stream", BedrockOptions, SimpleStreamOptions>
> {
	if (bedrockProviderModuleOverride) {
		return Promise.resolve(bedrockProviderModuleOverride);
	}
	if (shouldAttemptRustModelProvider("bedrock-converse-stream")) {
		return importNodeOnlyProvider("./rozsa-model-bridge.ts").then((module) => {
			const bridge = module as RustModelBridgeModule;
			if (bridge.shouldUseRustModelProvider("bedrock-converse-stream")) {
				return {
					stream: bridge.streamRustModel as StreamFunction<"bedrock-converse-stream", BedrockOptions>,
					streamSimple: bridge.streamSimpleRustModel as StreamFunction<
						"bedrock-converse-stream",
						SimpleStreamOptions
					>,
				};
			}
			return loadTypeScriptBedrockProviderModule();
		});
	}
	return loadTypeScriptBedrockProviderModule();
}

function loadTypeScriptBedrockProviderModule(): Promise<
	LazyProviderModule<"bedrock-converse-stream", BedrockOptions, SimpleStreamOptions>
> {
	bedrockProviderModulePromise ||= importNodeOnlyProvider("./amazon-bedrock.ts").then((module) => {
		const provider = module as BedrockProviderModule;
		return {
			stream: provider.streamBedrock,
			streamSimple: provider.streamSimpleBedrock,
		};
	});
	return bedrockProviderModulePromise;
}

export const streamAnthropic = createLazyStream(loadAnthropicProviderModule);
export const streamSimpleAnthropic = createLazySimpleStream(loadAnthropicProviderModule);
export const streamAzureOpenAIResponses = createLazyStream(loadAzureOpenAIResponsesProviderModule);
export const streamSimpleAzureOpenAIResponses = createLazySimpleStream(loadAzureOpenAIResponsesProviderModule);
export const streamGoogle = createLazyStream(loadGoogleProviderModule);
export const streamSimpleGoogle = createLazySimpleStream(loadGoogleProviderModule);
export const streamGoogleVertex = createLazyStream(loadGoogleVertexProviderModule);
export const streamSimpleGoogleVertex = createLazySimpleStream(loadGoogleVertexProviderModule);
export const streamMistral = createLazyStream(loadMistralProviderModule);
export const streamSimpleMistral = createLazySimpleStream(loadMistralProviderModule);
export const streamOpenAICodexResponses = createLazyStream(loadOpenAICodexResponsesProviderModule);
export const streamSimpleOpenAICodexResponses = createLazySimpleStream(loadOpenAICodexResponsesProviderModule);
export const streamOpenAICompletions = createLazyStream(loadOpenAICompletionsProviderModule);
export const streamSimpleOpenAICompletions = createLazySimpleStream(loadOpenAICompletionsProviderModule);
export const streamOpenAIResponses = createLazyStream(loadOpenAIResponsesProviderModule);
export const streamSimpleOpenAIResponses = createLazySimpleStream(loadOpenAIResponsesProviderModule);
const streamBedrockLazy = createLazyStream(loadBedrockProviderModule);
const streamSimpleBedrockLazy = createLazySimpleStream(loadBedrockProviderModule);

export function registerBuiltInApiProviders(): void {
	validateRustModelBackend();
	const strict = isStrictRustMode();

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("anthropic-messages")
			? {
					api: "anthropic-messages",
					stream: createRustGuardStream("anthropic-messages"),
					streamSimple: createRustGuardStream("anthropic-messages"),
				}
			: { api: "anthropic-messages", stream: streamAnthropic, streamSimple: streamSimpleAnthropic },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("openai-completions")
			? {
					api: "openai-completions",
					stream: createRustGuardStream("openai-completions"),
					streamSimple: createRustGuardStream("openai-completions"),
				}
			: { api: "openai-completions", stream: streamOpenAICompletions, streamSimple: streamSimpleOpenAICompletions },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("mistral-conversations")
			? {
					api: "mistral-conversations",
					stream: createRustGuardStream("mistral-conversations"),
					streamSimple: createRustGuardStream("mistral-conversations"),
				}
			: { api: "mistral-conversations", stream: streamMistral, streamSimple: streamSimpleMistral },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("openai-responses")
			? {
					api: "openai-responses",
					stream: createRustGuardStream("openai-responses"),
					streamSimple: createRustGuardStream("openai-responses"),
				}
			: { api: "openai-responses", stream: streamOpenAIResponses, streamSimple: streamSimpleOpenAIResponses },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("azure-openai-responses")
			? {
					api: "azure-openai-responses",
					stream: createRustGuardStream("azure-openai-responses"),
					streamSimple: createRustGuardStream("azure-openai-responses"),
				}
			: {
					api: "azure-openai-responses",
					stream: streamAzureOpenAIResponses,
					streamSimple: streamSimpleAzureOpenAIResponses,
				},
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("openai-codex-responses")
			? {
					api: "openai-codex-responses",
					stream: createRustGuardStream("openai-codex-responses"),
					streamSimple: createRustGuardStream("openai-codex-responses"),
				}
			: {
					api: "openai-codex-responses",
					stream: streamOpenAICodexResponses,
					streamSimple: streamSimpleOpenAICodexResponses,
				},
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("google-generative-ai")
			? {
					api: "google-generative-ai",
					stream: createRustGuardStream("google-generative-ai"),
					streamSimple: createRustGuardStream("google-generative-ai"),
				}
			: { api: "google-generative-ai", stream: streamGoogle, streamSimple: streamSimpleGoogle },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("google-vertex")
			? {
					api: "google-vertex",
					stream: createRustGuardStream("google-vertex"),
					streamSimple: createRustGuardStream("google-vertex"),
				}
			: { api: "google-vertex", stream: streamGoogleVertex, streamSimple: streamSimpleGoogleVertex },
	);

	registerApiProvider(
		strict && !shouldAttemptRustModelProvider("bedrock-converse-stream")
			? {
					api: "bedrock-converse-stream",
					stream: createRustGuardStream("bedrock-converse-stream"),
					streamSimple: createRustGuardStream("bedrock-converse-stream"),
				}
			: { api: "bedrock-converse-stream", stream: streamBedrockLazy, streamSimple: streamSimpleBedrockLazy },
	);
}

function validateRustModelBackend(): void {
	const backend = process.env.ROZSA_MODEL_BACKEND;
	if (backend === undefined || backend === "ts" || backend === "rust") return;
	throw new Error('ROZSA_MODEL_BACKEND must be "ts" or "rust".');
}

function isStrictRustMode(): boolean {
	return typeof process !== "undefined" && process.env.ROZSA_MODEL_BACKEND === "rust";
}

function createRustGuardStream<TApi extends Api>(api: TApi): StreamFunction<TApi, StreamOptions> {
	return (model) => {
		const stream = new AssistantMessageEventStream();
		const message: AssistantMessage = {
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
			errorMessage: `ROZSA_MODEL_BACKEND=rust but API "${api}" is not available through the Rust bridge. Use ROZSA_MODEL_BACKEND=ts or add a Rust provider before listing this API in ROZSA_MODEL_RUST_APIS.`,
			timestamp: Date.now(),
		};
		stream.push({ type: "error", reason: "error", error: message });
		stream.end(message);
		return stream;
	};
}

export function resetApiProviders(): void {
	clearApiProviders();
	registerBuiltInApiProviders();
}

registerBuiltInApiProviders();
