# rozsa-model Supported Providers

This document tracks provider support while `rozsa-model` replaces the TypeScript AI layer.

Related plan: [`rozsa-model-migration.md`](./rozsa-model-migration.md).

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
- The TypeScript `ModelRegistry` calls the Rust registry bridge by default in `ROZSA_MODEL_REGISTRY_BACKEND=auto` when the `rozsa-app` binary exists. `/model` then renders `getAvailable()` from this Rust-backed list, so NVIDIA shows only models discovered from the live endpoint unless the user explicitly configures custom models. Set `ROZSA_MODEL_REGISTRY_BACKEND=rust` to fail fast if the bridge is unavailable, or `ROZSA_MODEL_REGISTRY_BACKEND=ts` to force the old TypeScript registry.

Provider availability (auth check):

- Rust `ModelRegistry::provider_available()` checks whether each provider has configured credentials via environment variables (using `env_keys::get_env_api_key`) or via `models.json` `apiKey` field (literal value, env var reference, or `!command` marker).
- The bridge response includes `providerAvailable: Record<provider, {configured, source}>` alongside the full model list.
- TypeScript `ModelRegistry.hasConfiguredAuth()` uses the Rust-provided `providerAvailable` for API key auth, and separately checks TS-managed OAuth tokens (`AuthStorage`). A model is available if either path reports configured.
- When `ROZSA_MODEL_REGISTRY_BACKEND=ts`, Rust is not invoked and the TS side falls back to its original env var + models.json check logic.
- OAuth credential management (token storage, refresh, login flow) remains in TypeScript because it requires interactive browser flows and persistent encrypted storage that the Rust bridge cannot access.

Known current limits:

- `onPayload`/`onResponse` are TypeScript callback functions. Requests using those hooks route through the TypeScript provider until the bridge protocol supports callback round-trips.
- Network smoke tests are not part of the default unit tests; the live smoke test is ignored by default and requires explicit credentials or a running local model endpoint.
- In `auto` mode, a missing `rozsa-app` debug binary falls back to the TypeScript registry. `run.sh` builds `rozsa-app` and passes `ROZSA_APP_BINARY` to the TypeScript backend; standalone frontend runs should build `rozsa-app` or set `ROZSA_APP_BINARY` when validating Rust registry behavior.

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

| Provider/API | Planned order | Notes |
| --- | ---: | --- |
| Anthropic Messages | 3 | Implement direct HTTP/SSE. Preserve cache-control placement, thinking, fine-grained tool streaming, OAuth headers, Copilot headers, and Anthropic-compatible providers. |
| OpenAI Responses | 4 | Preserve Responses input conversion, reasoning signatures, tool-call IDs, prompt cache retention, and service tier behavior. |
| Azure OpenAI Responses | 5 | Build on Responses support. Preserve deployment mapping, endpoint normalization, and `api-version`. |
| Google Gemini | 6 | Implement direct REST using official protocol behavior as reference. Preserve image input, tool schema conversion, thinking, and thought signatures. |
| Google Vertex | 7 | Build after Gemini. Preserve project/location resolution and Application Default Credentials. |
| OpenAI Codex Responses | 8 | Special transport provider. Preserve SSE, WebSocket, cached WebSocket, account IDs, session IDs, retry-after handling, and OAuth boundary. |

## Deferred

| Provider/API | Reason |
| --- | --- |
| Mistral Conversations | No official Rust or Go SDK identified. Current TypeScript layer uses the official TypeScript SDK. Direct HTTP support can be added later if Mistral becomes required. |
| Provider-specific SDK adapters without Rust/Go support | Deferred unless the provider has a stable compatibility protocol or becomes product-critical. |

## Custom Providers

Custom providers should first use a supported compatibility protocol instead of requiring a new Rust adapter.

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
- Keep credentials explicit. For `Provider::Custom`, pass `api_key` in request options until custom credential resolution is designed.
- If custom headers contain secrets, pass them through caller-controlled configuration and avoid logging them.

## Maintenance Rules

- Update this document whenever a provider moves between support levels.
- Add a focused test under `tests/unit/model` for every supported provider or compatibility behavior.
- Keep provider implementation files under the 800-line pure-code limit.
- Document known limits directly in this file instead of implying unsupported parity.
