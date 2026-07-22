# rozsa-model Supported Providers

This document tracks provider support while `rozsa-model` replaces the TypeScript AI layer.

Related plan: [`rozsa-model-migration.md`](./rozsa-model-migration.md).

Related code:

- `packages/agent/src/event-stream.ts`: Agent-owned async event stream primitive，Agent loop 不再使用 `rozsa-ai` runtime EventStream。
- `packages/agent/src/tool-validation.ts`: Agent-owned tool argument validation，Agent loop 不再使用 `rozsa-ai` runtime validation helper。
- `packages/agent/src/types.ts`: Agent-owned structural `AssistantMessageEventStream` interface，`StreamFn` 不再绑定 `rozsa-ai` stream class。
- `packages/agent/src/compat-model-stream.ts`: Browser-safe TypeScript AI compatibility boundary，集中保留 legacy `streamSimple()` fallback。
- `packages/agent/src/missing-model-stream.ts`: 未注入模型执行函数时的 fail-fast 边界，避免 Agent 默认回退到 TS AI。
- `packages/agent/src/rozsa-model-client.ts`: Node-only `rozsa-model` JSONL client，agent/coding-agent Rust 执行路径不经过 TS AI provider bridge。
- `packages/agent/src/model-stream.ts`: Node 调用方显式注入后，把通用 Agent 的模型请求和 completion 请求分发到 `rozsa-model`。
- `crates/rozsa-model/src/credentials.rs`: Rust bridge request credential/header resolver，读取 `auth.json`、`models.json`、环境变量和命令型 config value。
- `packages/coding-agent/src/core/model-stream.ts`: Rust mode 下，把 coding-agent stream/completion 请求分发到 `rozsa-model`。
- `packages/coding-agent/src/core/model-utils.ts`: coding-agent-owned model helper boundary，替代 `rozsa-ai` 的 model equality、thinking level clamp/support 和 context overflow helpers。

## Support Levels

| Level | Meaning |
| --- | --- |
| Supported | Implemented in Rust, registered for program use, and covered by focused tests. |
| Scheduled | Planned by the migration order, but not implemented as a Rust provider yet. |
| Deferred | Not part of the current milestone. Support requires more protocol work or a product decision. |
| Custom | Usable through a supported compatibility protocol and caller-provided model metadata. |

## Supported

| Provider/API | Status | How it is supported |
| --- | --- | --- |
| OpenAI-compatible Chat Completions | Supported | `OpenAICompletionsProvider` implements `ApiProvider` for `Api::OpenAICompletions`. It builds `/chat/completions` payloads, sends HTTP requests, incrementally parses SSE `data:` chunks, forwards normalized stream events through the JSONL bridge, reports usage/cost, and can be registered with `register_provider` or `register_builtin_providers`. |

Current compatible provider coverage:

- OpenAI Chat Completions-compatible endpoints
- local OpenAI-compatible servers such as Ollama, vLLM, LM Studio, and llama.cpp
- provider endpoints that follow OpenAI Chat Completions streaming closely
- NVIDIA NIM / API Catalog OpenAI-compatible chat endpoints through `Provider::Nvidia`, `NVIDIA_API_KEY`, and `base_url` values such as `https://integrate.api.nvidia.com/v1`

Compatibility rules already modeled:

- provider/model/request header merging
- API key resolution from request options and known environment variables
- `max_tokens` vs `max_completion_tokens`
- `stream_options.include_usage`
- `store: false` for standard OpenAI-compatible providers
- system vs developer role selection
- OpenAI-style, DeepSeek-style, OpenRouter-style, Together-style, and Z.ai/Qwen-style reasoning payload fields
- tool schema conversion
- provider routing payloads for OpenRouter and Vercel AI Gateway
- Anthropic-style cache-control markers for compatible proxies
- Cloudflare AI Gateway placeholder URL expansion and gateway auth header
- GitHub Copilot dynamic request headers
- retry for transient HTTP status and transport errors
- `toolChoice` forwarding
- incremental SSE parsing and bridge event forwarding
- text, thinking, and tool-call stream event normalization
- env-gated live smoke test entrypoint for real OpenAI-compatible providers
- TS-vs-Rust parity test with a fake OpenAI-compatible server for payload, stream event, and final message equivalence

NVIDIA model discovery note:

- NVIDIA NIM exposes `GET /v1/models` for models currently loaded and available on that endpoint. NVIDIA API Catalog uses the same OpenAI-compatible base URL shape, but available hosted models can vary by account and over time. `rozsa-model` therefore does not hardcode a NVIDIA model list.
- `rozsa-app` owns the Rust `ModelRegistry`: it loads `packages/ai/src/models.generated.json`, merges optional `models.json`, and, when `NVIDIA_API_KEY` is set, merges live NVIDIA models from `GET https://integrate.api.nvidia.com/v1/models`.
- `rozsa-app` also owns generated image model metadata through `packages/ai/src/image-models.generated.json` and exposes it with the `list_image_models` bridge request.
- The TypeScript `ModelRegistry` calls the Rust registry bridge by default because `ROZSA_MODEL_REGISTRY_BACKEND` defaults to `rust`. `/model` renders `getAvailable()` from this Rust-backed list, so NVIDIA shows only models discovered from the live endpoint unless the user explicitly configures custom models. Set `ROZSA_MODEL_REGISTRY_BACKEND=ts` to force the old TypeScript registry. There is no `auto` fallback mode.

Provider availability (auth check):

- Rust `ModelRegistry::provider_available()` checks whether each provider has configured credentials via environment variables (using `env_keys::get_env_api_key`) or via `models.json` `apiKey` field (literal value, env var reference, or `!command` marker).
- On GUI startup, Rózsa asynchronously refreshes the `codex-oauth` model catalog only when persisted OAuth credentials include a ChatGPT account ID and a resolvable access token. The refresh uses the 24-hour cache in `refresh_models_if_needed`; when the refreshed registry differs from the in-memory registry, Rózsa atomically replaces it and emits `models-updated` so the model selector rerenders without restarting.
- The ChatGPT models endpoint requires a `client_version` query parameter with Codex CLI version semantics. Rózsa pins `CODEX_MODELS_CLIENT_VERSION` to the stable `major.minor.patch` portion of the newest Codex release tag, rather than sending the unrelated Rózsa package version. Run `./devtools/sync-codex-model-client-version.sh` to query `openai/codex` GitHub tags directly and update the constant, or add `--check` in verification-only workflows.
- The bridge response includes `providerAvailable: Record<provider, {configured, source}>` alongside the full model list.
- TypeScript `ModelRegistry.hasConfiguredAuth()` uses the Rust-provided `providerAvailable` for API key auth, and separately checks TS-managed OAuth tokens (`AuthStorage`). A model is available if either path reports configured.
- When `ROZSA_MODEL_REGISTRY_BACKEND=ts`, Rust is not invoked and the TS side uses its original env var + models.json check logic.
- Rust model execution reads request credentials from `auth.json` API keys, OAuth access tokens, `models.json` `apiKey`, environment variables, and `!command` config values.
- Rust bridge refreshes expired Anthropic、OpenAI Codex 和 GitHub Copilot OAuth credentials during request credential resolution. OAuth login remains in TypeScript.

Known current limits:

- `onPayload`/`onResponse` are TypeScript callback functions. In Rust execution mode, requests using those hooks fail clearly until the bridge protocol supports callback round-trips. Use `ROZSA_MODEL_BACKEND=ts` when those callbacks are required.
- Network smoke tests are not part of the default unit tests; the live smoke test is ignored by default and requires explicit credentials or a running local model endpoint.
- A missing `rozsa-app` debug binary is a startup/configuration error in Rust registry mode. `run.sh` builds `rozsa-app` and passes `ROZSA_APP_BINARY` to the TypeScript backend; standalone frontend runs should build `rozsa-app` or set `ROZSA_APP_BINARY` when validating Rust registry behavior.

| Anthropic Messages | Supported | `AnthropicProvider` implements `ApiProvider` for `Api::AnthropicMessages`. It builds `/v1/messages` payloads, sends HTTP requests with SSE streaming, incrementally parses Anthropic SSE events (message_start, content_block_start/delta/stop, message_delta, message_stop), forwards normalized stream events through the JSONL bridge, reports usage/cost, and can be registered with `register_provider` or `register_builtin_providers`. |

Current Anthropic Messages provider coverage:

- Anthropic Messages API (direct `api.anthropic.com` endpoint)
- Fireworks AI (Anthropic-compatible endpoint)
- MiniMax (Anthropic-compatible endpoint)
- Kimi Coding (Anthropic-compatible endpoint)
- Vercel AI Gateway (Anthropic-protocol proxy)
- Cloudflare AI Gateway (Anthropic-protocol proxy)
- GitHub Copilot (Anthropic-protocol proxy)

Anthropic Messages compatibility rules already modeled:

- API key auth (`x-api-key` header)
- OAuth bearer token auth (`sk-ant-oat` prefix detection → `Authorization: Bearer`)
- GitHub Copilot auth (`Authorization: Bearer` + dynamic headers)
- Cloudflare AI Gateway auth (`cf-aig-authorization` header)
- Session affinity headers (`x-session-affinity`) for Fireworks/Cloudflare
- Cache control placement (last user message block + last tool)
- Long cache retention (`ttl: "1h"`) for providers that support it
- Thinking configuration: adaptive thinking (effort level) and budget-based thinking
- Interleaved thinking beta header
- Fine-grained tool streaming beta header (when eager streaming is unsupported)
- Tool input eager streaming
- OAuth stealth mode (Claude Code tool name rewriting)
- Tool call ID normalization (64-char limit, alphanumeric + `_`/`-`)
- Consecutive tool results merged into single user message
- Non-vision model image degradation to placeholder text
- System prompt handling (OAuth vs standard mode)
- Temperature vs thinking mutual exclusion
- `metadata.user_id` forwarding
- Stop reason mapping (end_turn/pause_turn→stop, max_tokens→length, tool_use→toolUse, refusal/sensitive→error)
- Usage calculation (input/output/cacheRead/cacheWrite/totalTokens/cost)
- Compat flags: `supportsEagerToolInputStreaming`, `supportsLongCacheRetention`, `sendSessionAffinityHeaders`, `supportsCacheControlOnTools`, `forceAdaptiveThinking`
- TS-vs-Rust parity test with a fake Anthropic SSE server for payload, stream event, and final message equivalence

Anthropic Messages known current limits:

- No HTTP proxy support
- No `onPayload`/`onResponse` callbacks (TypeScript-only)
- Network smoke tests require explicit credentials

| AWS Bedrock Converse Stream | Supported | `BedrockProvider` implements `ApiProvider` for `Api::BedrockConverseStream`. It uses the official `aws-sdk-bedrockruntime` crate, sends ConverseStream requests, incrementally parses SDK event stream events, forwards normalized stream events through the JSONL bridge, reports usage/cost, and can be registered with `register_provider` or `register_builtin_providers`. |

Current Bedrock provider coverage:

- AWS Bedrock ConverseStream API for all Bedrock-hosted models
- Credential resolution via `aws-config` default chain (env vars, profiles, IMDS, ECS task role)
- Bearer token auth (`AWS_BEARER_TOKEN_BEDROCK`)
- Skip auth mode (`AWS_BEDROCK_SKIP_AUTH=1`) for unauthenticated proxies
- Region resolution via aws-config default chain, fallback to `us-east-1`

Bedrock compatibility rules already modeled:

- Prompt cache points (system prompt + last user message) for Claude 3.5 Haiku / 3.7 Sonnet / 4.x
- `CacheRetention::Long` → `CacheTtl::OneHour`
- Adaptive thinking (Claude Opus 4.6+, Sonnet 4.6) with effort level mapping
- Budget-based thinking (Claude 3.7 Sonnet) with configurable budgets per level
- `thinkingDisplay: "summarized"` default (omitted for GovCloud)
- Interleaved thinking beta for non-adaptive Claude models
- Thinking signature passthrough for Claude models
- Tool call JSON incremental parsing
- Text, thinking, and tool-call stream event normalization
- Content block start/delta/stop → unified StreamEvent mapping
- TokenUsage → Usage + cost calculation
- Bedrock StopReason → normalized StopReason mapping
- Image content (JPEG, PNG, GIF, WebP) via base64 decode
- Consecutive tool results merged into single user message (Bedrock requirement)

Bedrock known current limits:

- No HTTP proxy support (first version)
- No custom endpoint override
- No `onPayload`/`onResponse` callbacks
- No GovCloud FIPS endpoint detection (thinking display is omitted for GovCloud model IDs)

## Scheduled

No additional provider protocols are scheduled for the current model-layer milestone. The previous 2.3-2.9 provider rollout is delayed so the Rust model layer, bridge routing, and registry ownership can settle first.

## Deferred

| Provider/API | Reason |
| --- | --- |
| Anthropic Messages | Moved to Supported. See above. |
| OpenAI Responses | Deferred from the previous 2.4 slot. It still needs Responses input conversion, reasoning signatures, tool-call IDs, prompt cache retention, and service tier behavior. |
| Azure OpenAI Responses | Deferred from the previous 2.5 slot. It depends on OpenAI Responses parity plus Azure endpoint and deployment mapping. |
| Google Gemini | Deferred from the previous 2.6 slot. Direct REST support still needs image input, tool schema conversion, thinking, and thought signatures. |
| Google Vertex | Deferred from the previous 2.7 slot. It depends on Gemini behavior plus project/location and Application Default Credentials support. |
| OpenAI Codex Responses | Deferred from the previous 2.8 slot. It has special SSE/WebSocket transports, account/session handling, retry-after handling, and OAuth boundaries. |
| Mistral Conversations | No official Rust or Go SDK identified. Current TypeScript layer uses the official TypeScript SDK. Direct HTTP support can be added later if Mistral becomes required. |
| Provider-specific SDK adapters without Rust/Go support | Deferred unless the provider has a stable compatibility protocol or becomes product-critical. |

## Custom Providers

Custom providers should first use a supported compatibility protocol instead of requiring a new Rust adapter.

当前支持：

- Rust mode 支持使用 `api: "openai-completions"` 的 metadata-defined custom provider。
- `rozsa-app` Rust registry bridge 合并 custom model metadata、provider-level headers、model-level headers、`compat` 和 model overrides。
- `rozsa-model` Rust bridge 根据 `authJsonPath` / `modelsJsonPath` 解析 stored API key、已登录 OAuth credential、custom provider `apiKey`、`headers`、`authHeader` 和 command-backed config value。
- Rust bridge 会刷新已过期的 Anthropic、OpenAI Codex 和 GitHub Copilot OAuth credential；交互式 OAuth login 仍由 TypeScript CLI/UI 负责。
- `streamResolvedModel()` 会通过 JSONL bridge 发送 model metadata、custom `provider`、custom `baseUrl`、`compat` metadata、`authJsonPath` 和 `modelsJsonPath`。
- Focused coverage：`tests/unit/model/protocol.rs`、`packages/agent/test/model-stream.test.ts` 和 `packages/coding-agent/test/model-stream.test.ts`。

当前限制：

- Extension 提供的动态 `streamSimple` provider handler 仍然是 TypeScript handler，但已由 coding-agent-owned registry 和 SDK stream boundary 执行，不再注册到 `rozsa-ai` provider registry。
- `packages/agent/src/compat-model-stream.ts` 仍保留 legacy `streamSimple()` 作为显式 TS rollback path；这不是 Rust-supported API 的默认 coding-agent 生产执行路径。

For OpenAI-compatible Chat Completions, create a `Model` with:

- `api: Api::OpenAICompletions`
- `provider: Provider::Custom("<provider-id>")` or a known built-in provider enum
- `base_url` pointing at the endpoint root, usually ending in `/v1`
- `headers` for static provider headers
- `compat` overrides when the endpoint differs from standard OpenAI behavior
- `api_key` passed through `SimpleStreamOptions.base.api_key` or provided by a known environment variable for built-in providers

Minimal Rust example:

```rust
use rozsa_model::providers::openai_completions::OpenAICompletionsProvider;
use rozsa_model::registry::{register_provider, ApiProvider};
use rozsa_model::types::{Api, Model, Provider};

register_provider(Box::new(OpenAICompletionsProvider::new()));

let model = Model {
    id: "local-model".to_string(),
    name: "Local Model".to_string(),
    api: Api::OpenAICompletions,
    provider: Provider::Custom("local-openai".to_string()),
    base_url: "http://localhost:11434/v1".to_string(),
    // fill remaining metadata fields from the caller's model registry
    // before passing the model to stream_simple.
    // ...
};
```

Custom provider rules:

- Prefer `Api::OpenAICompletions` when the provider is OpenAI Chat Completions-compatible.
- Do not add a new provider adapter unless a compatibility protocol cannot represent the provider correctly.
- Keep provider-specific compatibility in `compat` metadata when possible.
- Prefer `models.json` `apiKey`/`headers`/`authHeader` for static custom provider credentials; runtime-only credentials may still be passed through request options.
- If custom headers contain secrets, pass them through caller-controlled configuration and avoid logging them.

## Maintenance Rules

- Update this document whenever a provider moves between support levels.
- Add a focused test under `tests/unit/model` for every supported provider or compatibility behavior.
- Keep provider implementation files under the 800-line pure-code limit.
- Document known limits directly in this file instead of implying unsupported parity.
