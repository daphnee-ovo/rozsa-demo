# rozsa-model Migration Plan

This document defines the staged migration from the TypeScript AI layer in
`packages/ai` to a Rust model layer named `rozsa-model`.

Related code:

- `packages/ai/src/stream.ts`: current public AI call surface.
- `packages/ai/src/api-registry.ts`: current provider registry.
- `packages/ai/src/providers/`: current provider implementations.
- `packages/ai/src/models.generated.ts`: current model metadata.
- `packages/agent/src/agent-loop.ts`: current agent loop entry point into the AI layer.

Related docs:

- [`supported-providers.md`](./supported-providers.md): current Rust provider support, scheduled providers, deferred providers, and custom provider entry points.

## Goal

Replace the TypeScript AI execution layer with Rust in stages:

1. Migrate individual provider protocol implementations into Rust.
2. Keep the current TypeScript `@earendil-works/pi-ai` API as a compatibility shell while provider migration is incomplete.
3. Move AI-layer methods such as `streamSimple`, `stream`, `completeSimple`, and `complete` behind the Rust implementation.
4. Move model registry, provider registry, credential resolution, and OAuth/session support when Rust coverage is ready.
5. Remove the TypeScript AI layer as an execution dependency.
6. Let the agent layer call `rozsa-model` directly.

The migration must preserve streaming behavior, tool-call semantics, cancellation, error reporting, usage accounting, and provider compatibility behavior.

## Current TS AI Layer

The current AI layer has a small public call surface:

- `stream(model, context, options)`
- `complete(model, context, options)`
- `streamSimple(model, context, options)`
- `completeSimple(model, context, options)`

Internally, `stream.ts` resolves `model.api` through the API provider registry and delegates to a provider implementation. The public `Model` object carries:

- `provider`
- `api`
- `baseUrl`
- `headers`
- `compat`
- model limits, cost, input modalities, and thinking metadata

The important design point is that the current system is not one SDK per provider. It uses a small set of protocol implementations and maps many providers onto them.

Current major API routes:

| API | Current role |
| --- | --- |
| `openai-completions` | OpenAI Chat Completions and most OpenAI-compatible providers |
| `openai-responses` | OpenAI Responses API and compatible routes |
| `azure-openai-responses` | Azure OpenAI Responses with Azure endpoint/deployment rules |
| `openai-codex-responses` | Codex subscription route with SSE/WebSocket transports |
| `anthropic-messages` | Anthropic Messages and Anthropic-compatible providers |
| `google-generative-ai` | Gemini API |
| `google-vertex` | Gemini through Vertex AI |
| `bedrock-converse-stream` | AWS Bedrock Converse Stream |
| `mistral-conversations` | Mistral chat stream |

Provider examples:

| Provider group | Current implementation route |
| --- | --- |
| OpenAI | `openai-responses` |
| Azure OpenAI | `azure-openai-responses` |
| OpenAI Codex | `openai-codex-responses` |
| Anthropic | `anthropic-messages` |
| Google Gemini | `google-generative-ai` |
| Google Vertex | `google-vertex` |
| AWS Bedrock | `bedrock-converse-stream` |
| Mistral | `mistral-conversations` |
| DeepSeek, Groq, Cerebras, xAI, OpenRouter, Together, Moonshot, Z.ai, Hugging Face, Cloudflare Workers AI, Xiaomi | `openai-completions` |
| Fireworks, MiniMax, Kimi Coding, Vercel AI Gateway | usually `anthropic-messages` |
| GitHub Copilot, OpenCode, Cloudflare AI Gateway | mixed routes depending on model |

## Migration Principles

- Keep TypeScript and Rust behavior observable at the same boundary before replacing a boundary.
- Migrate protocols before migrating callers.
- Keep rollback cheap. Every migrated provider must be selectable through a feature flag or configuration switch until parity is proven.
- Treat the TypeScript AI layer as a compatibility source, not the final architecture template.
- Use the bridge layer only for transition. It exists to preserve current communication while Rust provider coverage is incomplete.
- Let the final Rust model layer follow the project core principles: simple modules, explicit interfaces, transparent errors, centralized configuration, and clear ownership boundaries.
- Keep source files readable. A file's pure code content should not exceed 800 lines; split by responsibility before provider adapters or registries grow past that point.
- Every function/module should have a short description that covers its purpose, important parameters, return value, and error boundary when those are not obvious from the signature.
- Do not move model registry, OAuth, and custom provider loading first. Those are broad surfaces. Move provider execution first.
- Do not use FFI for the first migration. Streaming, cancellation, panic handling, async runtimes, and packaging are simpler over process IPC.
- Do not split the model layer across Rust and Go. Go SDKs can be used as protocol references, but runtime implementation should stay Rust.
- Prefer SDKs only where they reduce protocol risk. For OpenAI-compatible and Anthropic-compatible providers, direct HTTP/SSE adapters are acceptable and easier to normalize.

## Target Architecture

Initial target:

```text
agent layer
  -> @earendil-works/pi-ai compatibility shell
      -> RustModelBridge
          -> rozsa-model binary
              -> provider adapters
```

Final target:

```text
agent layer
  -> rozsa-model Rust API or process client
      -> provider adapters
```

The final shape can still expose a small TypeScript wrapper for Node consumers, but the wrapper should not own provider protocol behavior.

The final Rust architecture does not need to mirror the current TypeScript package layout. The current `api`, `provider`, registry, and compatibility shapes are useful migration inputs. They should be replaced when a Rust-native boundary is simpler, more explicit, or easier to test.

Final Rust boundaries should be organized around responsibilities:

- provider adapters own wire protocol details
- model registry owns metadata
- credential resolver owns auth source selection
- stream runtime owns event ordering, cancellation, and backpressure
- agent-facing client owns the stable request/response contract

## Bridge Strategy

Use a Rust child process with stdio JSONL for the first migration phase.

Reasons:

- Works with streaming events.
- Avoids N-API and FFI complexity.
- Keeps Rust panics isolated from the Node process.
- Allows per-provider rollout.
- Easy to test with fake binaries and golden JSONL.
- Does not require an HTTP port or local daemon lifecycle.

Use an HTTP sidecar only if there is a later need for a long-lived shared model service across multiple processes. Do not start with that.

## Wire Protocol

The bridge protocol must be line-delimited JSON. One JSON value per line.

TS to Rust request:

```json
{"type":"request","id":"req_1","method":"streamSimple","model":{},"context":{},"options":{}}
```

Rust to TS events:

```json
{"type":"event","id":"req_1","event":{"type":"start","partial":{}}}
{"type":"event","id":"req_1","event":{"type":"text_start","contentIndex":0,"partial":{}}}
{"type":"event","id":"req_1","event":{"type":"text_delta","contentIndex":0,"delta":"hello","partial":{}}}
{"type":"event","id":"req_1","event":{"type":"text_end","contentIndex":0,"content":{},"partial":{}}}
{"type":"event","id":"req_1","event":{"type":"done","reason":"stop","message":{}}}
```

TS to Rust cancellation:

```json
{"type":"cancel","id":"req_1"}
```

Rust to TS process-level error:

```json
{"type":"error","id":"req_1","message":"No API key for provider: openai","code":"missing_api_key"}
```

Protocol rules:

- `id` is required on every request, event, and cancellation.
- Events must use the existing `AssistantMessageEvent` shapes from `packages/ai/src/types.ts`.
- The final `done` event must include the final assistant message.
- Provider failures should become stream `error` events with an assistant message when the request has already entered streaming.
- Bridge/process failures before streaming can become bridge `error` messages.
- Cancellation must map to the existing `aborted` stop reason.
- Unknown fields are ignored by receivers.
- A protocol `version` field should be added before the protocol is used outside local development.

## Rust Crate Layout

Recommended crate split:

```text
crates/rozsa-model/
  src/
    lib.rs
    main.rs
    protocol.rs
    types.rs
    registry.rs
    stream.rs
    credentials.rs
    providers/
      mod.rs
      openai_chat.rs
      openai_responses.rs
      anthropic_messages.rs
      google_genai.rs
      google_vertex.rs
      bedrock.rs
      azure_openai.rs
      codex_responses.rs
      mistral.rs
```

Keep provider modules cohesive:

- request conversion
- provider-specific payload construction
- streaming response parsing
- usage conversion
- error conversion
- compatibility options

Do not put all provider logic into one large dispatcher.

File and description rules:

- Keep pure code content under 800 lines per file. Count executable code and declarations; ignore blank lines and comments when judging the limit.
- If a provider adapter approaches the limit, split it into focused files such as `payload.rs`, `stream.rs`, `types.rs`, `errors.rs`, or `compat.rs`.
- Add a short module-level description for each provider module.
- Add a short function description for public functions and for private functions whose purpose, parameters, return value, or error behavior is not clear from the name and signature.
- Keep descriptions concise. They should explain what the function/module owns, not narrate implementation line by line.

## Shared Type Boundary

The first bridge should serialize the current TypeScript types directly:

- `Model`
- `Context`
- `Message`
- `Tool`
- `SimpleStreamOptions`
- `ProviderStreamOptions`
- `AssistantMessage`
- `AssistantMessageEvent`

This avoids changing the agent layer during provider migration.

Rust should define equivalent `serde` structs. Use explicit enums where possible. For provider-specific unknown payloads, use `serde_json::Value` only at the boundary where the provider requires flexible JSON.

This is a transition boundary, not a long-term constraint. Do not create a separate Rust-only message model until provider parity is proven, but do not preserve the TypeScript shape after that only for compatibility. A Rust-native representation is appropriate when it gives clearer invariants, less weak typing, or simpler provider conversions.

## Stage 0: Inventory And Test Baseline

Before implementation:

1. Record current provider-to-API mapping from `models.generated.ts`.
2. Record supported stream event types from `AssistantMessageEvent`.
3. Identify provider tests that can run without live credentials.
4. Add bridge-level tests with a fake Rust process.
5. Add Rust golden tests for request/response/event JSON.

Minimum baseline:

- Tool call streaming.
- Text streaming.
- Thinking streaming.
- Image input preservation.
- Tool result image preservation.
- Usage and cost fields.
- Error and abort behavior.
- Cache-retention/session fields.
- Provider custom headers.

Do not migrate a provider until there is a focused test that can compare TS and Rust behavior at the stream-event or final-message boundary.

## Stage 1: Add Rust Bridge Without Changing Defaults

Add a TypeScript bridge provider that can call `rozsa-model`, but keep all default providers on TS.

Suggested files:

- `packages/ai/src/providers/rozsa-model-bridge.ts`
- `tests/unit/model/rozsa-model-bridge.test.ts`

Runtime selection:

```bash
ROZSA_MODEL_BACKEND=ts
ROZSA_MODEL_BACKEND=rust
ROZSA_MODEL_RUST_APIS=openai-completions,bedrock-converse-stream
ROZSA_MODEL_BINARY=/path/to/rozsa-model
ROZSA_MODEL_BINARY_ARGS='["--optional-test-arg"]'
```

Selection rules:

- Default must remain TS.
- If `ROZSA_MODEL_BACKEND=rust`, route only APIs listed in `ROZSA_MODEL_RUST_APIS`.
- If the Rust binary is unavailable, fail clearly. Do not silently fall back unless a separate explicit fallback flag exists.
- Keep `onPayload` and `onResponse` behavior for migrated providers or document exactly when a migrated provider does not support them yet.

Done criteria:

- The bridge can stream fake events into `AssistantMessageEventStream`.
- Cancellation returns an aborted error and terminates the bridge process.
- Child process exit produces transparent errors.
- No built-in provider routes to Rust by default.

## Stage 2: Migrate Provider Protocols One By One

Provider migration order should prioritize coverage and implementation risk.

### 2.1 OpenAI-compatible Chat Completions

Migrate `openai-completions` first because it covers many providers.

Covered providers include:

- DeepSeek
- Groq
- Cerebras
- xAI
- OpenRouter
- Together
- Moonshot
- Z.ai
- Hugging Face
- Cloudflare Workers AI
- Xiaomi
- local OpenAI-compatible servers such as Ollama, vLLM, LM Studio, llama.cpp

Implementation requirements:

- Convert current `Context` to chat completion messages.
- Preserve system/developer role compatibility.
- Preserve tool schema conversion.
- Preserve streaming text deltas.
- Preserve streaming tool-call partial JSON.
- Preserve provider-specific `reasoning` formats:
  - OpenAI-style `reasoning_effort`
  - DeepSeek-style reasoning fields
  - OpenRouter nested `reasoning`
  - Together nested `reasoning`
  - Z.ai/Qwen `enable_thinking`
- Preserve `max_tokens` vs `max_completion_tokens`.
- Preserve cache-control behavior for OpenRouter Anthropic models.
- Preserve session affinity headers.
- Preserve custom headers and `baseUrl`.

Initial Rust implementation should use `reqwest` plus direct SSE parsing, not an SDK. The OpenAI-compatible surface has many provider-specific differences, and direct HTTP gives better control.

### 2.2 AWS Bedrock

Migrate `bedrock-converse-stream` early because Rust has an official AWS SDK.

Implementation requirements:

- Use `aws-sdk-bedrockruntime`.
- Preserve region and endpoint override behavior.
- Preserve AWS credential source support.
- Preserve Bedrock Claude cache point behavior.
- Preserve thinking payload conversion.
- Preserve Converse Stream event parsing.
- Preserve proxy-related behavior currently supported by environment variables.

### 2.3 Anthropic Messages

Migrate `anthropic-messages` after OpenAI-compatible and Bedrock.

Covered providers include:

- Anthropic
- Fireworks Anthropic-compatible models
- MiniMax Anthropic-compatible models
- Kimi Coding
- Vercel AI Gateway Anthropic routes
- Cloudflare AI Gateway Anthropic passthrough
- GitHub Copilot Claude models

Implementation requirements:

- Convert messages and tools to Anthropic Messages payloads.
- Preserve cache-control block placement.
- Preserve thinking/interleaved-thinking behavior.
- Preserve fine-grained tool streaming beta handling.
- Preserve eager tool input behavior.
- Preserve tool-call name and argument normalization.
- Preserve OAuth token headers for Claude subscription auth.
- Preserve Copilot dynamic headers.
- Preserve Cloudflare AI Gateway auth headers.

Rust should use direct HTTP/SSE. There is no official Rust SDK, and the current TS layer already has important compatibility logic around headers and streaming.

### 2.4 OpenAI Responses

Migrate `openai-responses` after `openai-completions`.

Implementation requirements:

- Preserve Responses input conversion.
- Preserve reasoning content and reasoning replay signatures.
- Preserve tool-call ID handling.
- Preserve prompt cache key and retention behavior.
- Preserve `serviceTier` usage adjustments.
- Preserve GitHub Copilot and Cloudflare AI Gateway special headers when routed through Responses.

### 2.5 Azure OpenAI Responses

Migrate `azure-openai-responses` after OpenAI Responses.

Implementation requirements:

- Preserve endpoint normalization.
- Preserve deployment name mapping.
- Preserve `api-version` handling.
- Preserve Azure-specific auth and base URL behavior.

### 2.6 Google Gemini

Migrate `google-generative-ai`.

Implementation requirements:

- Preserve text and image input conversion.
- Preserve tool schema conversion.
- Preserve thinking level behavior.
- Preserve thought signatures.
- Preserve unsigned tool-call handling for Gemini 3 behavior currently covered by tests.

Rust can use direct REST. Go SDK behavior can be used as a protocol reference, but should not be part of runtime.

### 2.7 Google Vertex

Migrate `google-vertex` after Gemini.

Implementation requirements:

- Preserve project/location resolution.
- Preserve Application Default Credentials behavior.
- Preserve explicit API key behavior where supported.
- Preserve custom base URL behavior.
- Preserve the same message/tool conversion as Gemini where applicable.

This provider has more auth risk than plain Gemini. Do not combine it with Gemini migration unless the auth boundary is already isolated.

### 2.8 OpenAI Codex Responses

Migrate `openai-codex-responses` only after the generic stream protocol is stable.

Implementation requirements:

- Preserve SSE transport.
- Preserve WebSocket transport.
- Preserve cached WebSocket sessions.
- Preserve account ID handling.
- Preserve session IDs and request IDs.
- Preserve retry-after handling.
- Preserve OAuth token refresh boundary.

This is a special provider. It should not block replacing common API-key providers.

### 2.9 Mistral

Keep Mistral later or experimental unless it becomes a required provider.

Reason:

- No official Rust or Go SDK was identified.
- Current TS layer uses the official TypeScript SDK.
- Rust can still implement direct HTTP, but it should not be part of the first parity milestone.

Minimum requirements when migrated:

- Preserve Mistral tool-call ID length normalization.
- Preserve `promptMode` and `reasoningEffort`.
- Preserve stream usage conversion.

## Stage 3: Move AI Methods Behind Rust

After several providers are migrated, move the AI method implementation behind the bridge while preserving the TS public API.

Current TS methods:

- `stream`
- `complete`
- `streamSimple`
- `completeSimple`

Target behavior:

- `stream` and `streamSimple` call the Rust backend when the selected API is Rust-enabled.
- `complete` and `completeSimple` remain wrappers that call the stream method and wait for `result()`.
- The TS `AssistantMessageEventStream` remains the compatibility surface for Node callers.

This stage should not change agent code.

Done criteria:

- Agent tests still import `@earendil-works/pi-ai`.
- Migrated APIs can run through Rust without agent-layer changes.
- Non-migrated APIs still run through TS.
- Per-API fallback is explicit and observable.

## Stage 4: Move Registry And Metadata

Move registries only after provider execution is stable.

Migration order:

1. Move provider execution registry.
2. Move model metadata loading.
3. Move compatibility metadata.
4. Move custom model loading.
5. Move image model registry if image generation remains in scope.

Keep `models.generated.ts` as the source of truth until Rust can load equivalent generated data.

Target Rust metadata should preserve:

- provider ID
- model ID
- API type
- base URL
- headers
- input modalities
- context window
- max output tokens
- cost
- reasoning support
- thinking level map
- provider compatibility flags

Do not rewrite model discovery and provider metadata at the same time as provider protocol migration.

## Stage 5: Move Credential And OAuth Support

Move credential resolution after model/provider routing is stable.

Credential sources to preserve:

- CLI-provided API key.
- `auth.json` API key.
- environment variables.
- custom provider `models.json` API key resolution.
- shell command key resolution for custom providers.
- OAuth tokens for OpenAI Codex, Anthropic subscription auth, and GitHub Copilot.
- AWS ambient credentials.
- Google ADC credentials.

Important boundary:

- Rust should not execute arbitrary shell commands for custom provider credentials until a clear policy exists.
- If shell command resolution remains TS-owned during transition, pass resolved credentials to Rust.
- If Rust owns shell command resolution later, implement transparent errors and avoid hidden stale fallback behavior.

## Stage 6: Move Custom Provider Support

Current custom provider support allows providers to define:

- `baseUrl`
- `api`
- `apiKey`
- `headers`
- `authHeader`
- `models`
- `modelOverrides`
- `compat`

Rust replacement must preserve:

- provider-level defaults
- model-level overrides
- per-model API override
- compatibility merge rules
- hot reload behavior if `/model` still expects it
- extension-provided custom stream handlers, or a documented replacement

Do not remove extension custom provider behavior until there is a compatible replacement or a confirmed product decision to drop it.

## Stage 7: Agent Direct Integration

Only after Rust owns provider execution, registry, metadata, credentials, and custom providers should the agent layer bypass `packages/ai`.

Current agent dependency:

- `packages/agent/src/agent-loop.ts` imports `streamSimple`, `Context`, `AssistantMessage`, and validation helpers from `@earendil-works/pi-ai`.

Direct integration requires:

- Rust-backed client exposed to the agent layer.
- Equivalent TS types or generated bindings for agent compilation.
- Tool argument validation replacement.
- Stream event conversion into `AgentEvent`.
- Clear ownership of `Context` and `Message` types.

Recommended final agent boundary:

```text
AgentContext
  -> explicit model request
  -> rozsa-model stream client
  -> AgentEvent stream
```

The agent layer should not know provider SDK details. It should only know:

- selected model
- messages
- tools
- reasoning setting
- cancellation signal
- stream events

At this stage, do not preserve `@earendil-works/pi-ai` method names or type shapes just because they existed during migration. Keep them only if they remain the simplest stable interface. The direct agent boundary should be designed from the agent's needs and the Rust model layer's responsibilities.

## Stage 8: Remove TS AI Execution Layer

Remove TypeScript provider implementations only after all required routes have Rust equivalents.

Removal checklist:

- No production caller imports provider functions from `packages/ai/src/providers/*`.
- `@earendil-works/pi-ai` compatibility wrapper no longer owns provider protocol code.
- Agent and coding-agent no longer require TS provider execution.
- Tests for migrated providers run against Rust.
- Packaging includes `rozsa-model` binary for supported platforms.
- Extension API migration path is documented.
- Docs point to `rozsa-model` as the owner of provider behavior.

After this stage, `packages/ai` can become one of:

- a thin TypeScript client for `rozsa-model`
- a compatibility package for external Node users
- removed from internal execution path

## Provider Support Policy

First-class support:

- Providers with official Rust SDKs.
- Providers with official Go SDKs where direct Rust HTTP implementation is stable and protocol behavior is clear.
- Providers with stable OpenAI-compatible or Anthropic-compatible REST surfaces.

Deferred support:

- Providers without official Rust/Go SDKs and without stable compatibility surfaces.
- Providers that require private protocol behavior.
- Providers whose current support depends heavily on a TypeScript-only SDK.

Current practical classification:

| Provider/API | Rust migration priority | Notes |
| --- | --- | --- |
| `openai-completions` | High | Largest provider coverage through compatibility route |
| `bedrock-converse-stream` | High | Official Rust SDK available |
| `anthropic-messages` | High | Broad provider coverage; direct HTTP needed |
| `openai-responses` | High | Core OpenAI route |
| `azure-openai-responses` | Medium | Depends on Responses parity plus Azure endpoint logic |
| `google-generative-ai` | Medium | Official Go SDK exists; direct Rust REST acceptable |
| `google-vertex` | Medium | Auth/project/location complexity |
| `openai-codex-responses` | Medium | Special transport and OAuth behavior |
| `mistral-conversations` | Low/Deferred | No official Rust/Go SDK identified |

## Verification Strategy

Use three levels of verification.

### Unit Tests

Rust:

- message conversion
- tool schema conversion
- provider payload construction
- stream event parsing
- error parsing
- usage conversion
- compatibility flags

TypeScript:

- bridge process lifecycle
- JSONL parsing
- cancellation
- stream event forwarding
- explicit fallback behavior

### Parity Tests

For each migrated provider:

- same input context
- same model metadata
- compare payload sent to provider when possible
- compare normalized stream events
- compare final `AssistantMessage`
- compare error shape
- compare abort behavior

Use local fake provider servers for non-e2e coverage.

### Live Smoke Tests

Live tests should remain opt-in and credential-gated.

Minimum live smoke set:

- one OpenAI-compatible API-key provider
- one Anthropic Messages provider
- one Bedrock model when AWS credentials exist
- one Gemini model when credentials exist
- one OAuth provider only when local auth is configured

Do not make paid or credentialed e2e tests part of the default check path.

## Rollout Flags

Use explicit flags during migration.

Suggested flags:

```bash
ROZSA_MODEL_BACKEND=ts
ROZSA_MODEL_BACKEND=rust
ROZSA_MODEL_BACKEND=auto
ROZSA_MODEL_RUST_APIS=openai-completions,anthropic-messages
ROZSA_MODEL_BINARY=/absolute/path/to/rozsa-model
ROZSA_MODEL_TRACE=1
```

Semantics:

- `ts`: always use TS provider implementation.
- `rust`: use Rust for listed APIs; fail if Rust cannot serve them.
- `auto`: use Rust for listed APIs when available, otherwise use TS and emit an observable warning.

Do not make `auto` the default until parity is strong.

## Error Handling Requirements

Rust errors must preserve current expectations:

- Missing API key: fail fast with provider name.
- Missing cloud config: identify missing variable or credential source.
- Provider HTTP error: include provider, status code, and bounded response body.
- Streaming parse error: include event type or parser location when possible.
- Cancellation: return `stopReason: "aborted"`.
- Unknown provider API: return a clear unsupported API error.
- Bridge crash: report child process exit status and stderr excerpt.

Do not hide Rust failures behind silent TS fallback unless the user explicitly enabled an auto fallback mode.

## Packaging Strategy

Initial development:

- Build Rust binary with Cargo.
- TS bridge locates binary through `ROZSA_MODEL_BINARY`.

Local repo integration:

- Add a root build script for `rozsa-model`.
- Add package scripts only after the binary is wired into tests.

Release integration:

- Ship platform-specific binaries or build from source during package release.
- Keep binary selection deterministic.
- Fail clearly when no binary exists for the current platform.

Avoid lifecycle scripts that download or build binaries during install unless there is an explicit security review.

## Completed

- Current TypeScript AI layer call surface has been identified.
- Current provider mapping strategy has been identified.
- Preferred bridge strategy is defined as stdio JSONL.
- `rozsa-model` exposes a stdio JSONL bridge binary.
- TypeScript AI can route `openai-completions` through Rust by opt-in env flags.
- OpenAI-compatible Chat Completions has the first Rust provider adapter.
- OpenAI-compatible Chat Completions now supports incremental stream output, retry, provider routing, Cloudflare Gateway, Copilot dynamic headers, and proxy cache-control payloads.
- Requests using `onPayload` or `onResponse` keep TypeScript provider routing so callback behavior remains available during the bridge phase.
- An ignored live smoke entrypoint exists under `tests/unit/model` for explicit credential-backed checks.
- Rust protocol and fake TypeScript bridge tests live under `tests/unit/model`.
- Provider migration order is defined.
- Final agent direct integration boundary is defined.

## Remaining

- Define exact `AssistantMessageEvent` JSON schema version.
- Design callback round-trips if `onPayload`/`onResponse` must execute inside the Rust bridge instead of using the TypeScript compatibility route.
- Decide packaging strategy for platform binaries.
- Decide final compatibility story for external `@earendil-works/pi-ai` consumers.
