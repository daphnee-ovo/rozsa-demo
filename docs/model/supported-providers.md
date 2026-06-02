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

Known current limits:

- `onPayload`/`onResponse` are TypeScript callback functions. Requests using those hooks route through the TypeScript provider until the bridge protocol supports callback round-trips.
- Network smoke tests are not part of the default unit tests; the live smoke test is ignored by default and requires explicit credentials.

## Scheduled

| Provider/API | Planned order | Notes |
| --- | ---: | --- |
| AWS Bedrock Converse Stream | 2 | Use the official Rust AWS SDK. Preserve region, endpoint override, credential sources, Converse Stream parsing, and Claude cache points. |
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
