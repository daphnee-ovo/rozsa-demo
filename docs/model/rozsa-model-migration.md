# rozsa-model Migration Plan

This document defines the staged migration from the TypeScript AI layer in
`packages/ai` to a Rust model layer named `rozsa-model`.

Related code:

- `packages/ai/src/stream.ts`: current public AI call surface.
- `packages/ai/src/api-registry.ts`: current provider registry.
- `packages/ai/src/providers/`: current provider implementations.
- `packages/ai/src/models.generated.ts`: current model metadata.
- `packages/agent/src/agent-loop.ts`: current agent loop entry point into the AI layer.
- `packages/agent/src/event-stream.ts`: Agent-owned async event stream primitive，替代 `pi-ai` runtime EventStream 依赖。
- `packages/agent/src/tool-validation.ts`: Agent-owned tool argument validation，替代 `pi-ai` runtime validation 依赖。
- `packages/model-types/src/types.ts`: Shared model protocol type definitions extracted from pi-ai.
- `packages/agent/src/missing-model-stream.ts`: Agent-owned fail-fast stream boundary；未显式注入模型执行函数时不再隐式回退到 TS AI。
- `packages/agent/src/rozsa-model-client.ts`: Node-only `rozsa-model` JSONL client，直接管理 Rust child process 和 bridge protocol。
- `packages/agent/src/model-stream.ts`: 通用 Agent Node 模型请求边界，提供 `streamDefaultModel()` / `completeDefaultModel()`，无条件路由到 Rust bridge。
- `crates/rozsa-model/src/credentials.rs`: Rust-owned request credential/header resolver，读取 `auth.json`、`models.json`、环境变量和命令型 config value。
- `packages/coding-agent/src/core/model-stream.ts`: coding-agent 模型请求边界，提供 `streamResolvedModel()` / `completeResolvedModel()`，委托到 agent 的 Rust bridge。
- `packages/coding-agent/src/core/model-utils.ts`: coding-agent-owned model helper boundary，替代 `pi-ai` 的 model equality、thinking level clamp/support 和 context overflow helpers。

Related docs:

- [`supported-providers.md`](./supported-providers.md): current Rust provider support, scheduled providers, deferred providers, and custom provider entry points.

## Goal

Replace the TypeScript AI execution layer with Rust in stages:

1. Migrate individual provider protocol implementations into Rust.
2. Keep the current TypeScript `@earendil-works/pi-ai` API as a compatibility shell while provider migration is incomplete.
3. Move AI-layer methods such as `streamSimple`, `stream`, `completeSimple`, and `complete` behind the Rust implementation for supported Rust APIs.
4. Move provider execution registry, model metadata, compatibility metadata, and custom model metadata loading into Rust while keeping interactive OAuth/session flows in TypeScript.
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

The previous 2.3-2.9 provider rollout is delayed. The current milestone prioritizes finishing the Rust model-layer boundary, bridge routing, and registry ownership before adding more provider protocol adapters.

### 2.3 Deferred: Anthropic Messages

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

### 2.4 Deferred: OpenAI Responses

Migrate `openai-responses` after `openai-completions`.

Implementation requirements:

- Preserve Responses input conversion.
- Preserve reasoning content and reasoning replay signatures.
- Preserve tool-call ID handling.
- Preserve prompt cache key and retention behavior.
- Preserve `serviceTier` usage adjustments.
- Preserve GitHub Copilot and Cloudflare AI Gateway special headers when routed through Responses.

### 2.5 Deferred: Azure OpenAI Responses

Migrate `azure-openai-responses` after OpenAI Responses.

Implementation requirements:

- Preserve endpoint normalization.
- Preserve deployment name mapping.
- Preserve `api-version` handling.
- Preserve Azure-specific auth and base URL behavior.

### 2.6 Deferred: Google Gemini

Migrate `google-generative-ai`.

Implementation requirements:

- Preserve text and image input conversion.
- Preserve tool schema conversion.
- Preserve thinking level behavior.
- Preserve thought signatures.
- Preserve unsigned tool-call handling for Gemini 3 behavior currently covered by tests.

Rust can use direct REST. Go SDK behavior can be used as a protocol reference, but should not be part of runtime.

### 2.7 Deferred: Google Vertex

Migrate `google-vertex` after Gemini.

Implementation requirements:

- Preserve project/location resolution.
- Preserve Application Default Credentials behavior.
- Preserve explicit API key behavior where supported.
- Preserve custom base URL behavior.
- Preserve the same message/tool conversion as Gemini where applicable.

This provider has more auth risk than plain Gemini. Do not combine it with Gemini migration unless the auth boundary is already isolated.

### 2.8 Deferred: OpenAI Codex Responses

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

### 2.9 Deferred: Mistral

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

Move the AI method implementation behind the bridge for Rust-supported APIs while preserving the TS public API.

Current TS methods:

- `stream`
- `complete`
- `streamSimple`
- `completeSimple`

Target behavior:

- `stream` and `streamSimple` call the Rust backend when the selected API is Rust-supported and explicitly listed in `ROZSA_MODEL_RUST_APIS`.
- `complete` and `completeSimple` remain wrappers that call the stream method and wait for `result()`.
- The TS `AssistantMessageEventStream` remains the compatibility surface for Node callers.

This stage should not change agent code.

Done criteria:

- Agent tests still import `@earendil-works/pi-ai`.
- Migrated APIs can run through Rust without agent-layer changes.
- Non-migrated APIs fail clearly in `ROZSA_MODEL_BACKEND=rust` instead of falling back to TS.
- `ROZSA_MODEL_BACKEND=ts` is the explicit rollback path.
- Bedrock and OpenAI-compatible APIs route through the Rust bridge when enabled.

Current status:

- Complete for the current model-layer milestone.
- `ROZSA_MODEL_BACKEND` accepts only `ts` or `rust`; `auto` has been removed.
- Rust execution and TS execution are separated by `ROZSA_MODEL_BACKEND`.
- Only APIs implemented by the Rust bridge can run in Rust mode.
- `stream`/`streamSimple` route `openai-completions` and `bedrock-converse-stream` through `rozsa-model` when enabled.
- `complete`/`completeSimple` inherit that routing through their existing stream wrappers.

## Stage 4: Move Registry And Metadata

Move registries after provider execution is stable enough for the current supported APIs.

Migration order:

1. Move provider execution registry.
2. Move model metadata loading.
3. Move compatibility metadata.
4. Move custom model loading.
5. Move image model registry metadata.

Keep `models.generated.ts` and `models.generated.json` traceable during the transition. Rust loads the generated JSON data; TypeScript keeps the generated TypeScript metadata for TS-only execution.

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

Do not add new provider discovery surfaces while provider protocol migration is paused.

Current status:

- Complete for registry and metadata ownership in the current model-layer milestone.
- `rozsa-app` owns the Rust model registry bridge.
- `ROZSA_MODEL_REGISTRY_BACKEND` defaults to `rust`; set it to `ts` to force the TypeScript registry.
- `auto` registry fallback has been removed. Missing or broken `rozsa-app` is a configuration error in Rust registry mode.
- Rust loads `packages/ai/src/models.generated.json`, merges optional `models.json`, preserves compatibility metadata, and can merge live NVIDIA model discovery when `NVIDIA_API_KEY` is configured.
- Rust loads `packages/ai/src/image-models.generated.json` and exposes image model metadata through the `list_image_models` app bridge request.
- TypeScript still owns interactive OAuth login flows and the default `auth.json` storage location; Rust execution reads and refreshes persisted credentials when handling model requests.

## Stage 5: Credential 和 OAuth 支持迁移

模型和 provider 路由稳定后，再迁移 request credential 解析。

需要保留的 credential 来源：

- CLI 提供的 API key。
- `auth.json` API key。
- 环境变量。
- custom provider 的 `models.json` API key。
- custom provider 的 `!command` shell key。
- OpenAI Codex、Anthropic subscription auth、GitHub Copilot 的 OAuth token。
- AWS ambient credentials。
- Google ADC credentials。

当前边界：

- Rust 已负责读取持久化 credential、`models.json` request config、provider headers 和 `authHeader`。
- Rust 已负责已知 OAuth provider 的过期 token refresh。
- TypeScript 仍负责交互式 OAuth login、runtime override、extension/in-memory auth storage。
- Rust 执行 `!command` 时必须失败透明；command 失败返回 credential error，不做静默 fallback。

当前状态：

- 对 Rust 已支持的执行 API，request credential/header 解析已迁到 Rust bridge 前置层。
- `crates/rozsa-model/src/credentials.rs` 现在读取：
  - `auth.json` 中的 API key。
  - `auth.json` 中未过期的 OAuth access token。
  - `auth.json` 中过期的 Anthropic、OpenAI Codex 和 GitHub Copilot OAuth token，并在 Rust 内刷新后写回。
  - `models.json` provider `apiKey`，包括环境变量引用和 `!command` shell 解析。
  - `models.json` provider `headers` 和 `authHeader`。
- `packages/coding-agent/src/core/sdk.ts` 在 Rust path 下传递 `authJsonPath` 和 `modelsJsonPath`，不再要求 TS 先解析 `auth.json` 或 `models.json` request credential。
- CLI/runtime override 和自定义/in-memory `AuthStorage` 仍由 TS 解析后作为 request `apiKey` 传入 Rust，因为 Rust bridge 不能读取进程内虚拟状态。
- OAuth interactive login 仍由 TypeScript `AuthStorage`/OAuth provider 执行；Rust bridge 只负责已登录 credential 的使用和刷新。
- Rust 写回 refreshed OAuth credential 时使用 `auth.json.lock` 文件保护；锁被占用时直接返回 credential error。
- Provider 自己拥有的 ambient credential 仍由 Rust provider 解析，例如 AWS Bedrock 通过 `aws-config` 解析。

Focused coverage：

- `tests/unit/model/protocol.rs` 验证 Rust resolver 能从 `auth.json` 解析 stored API key 和未过期 OAuth，并能从 `models.json` 解析 `apiKey`、provider headers 和 `authHeader`；过期但不支持 Rust refresh 的 OAuth provider 会 fail fast。
- `packages/coding-agent/test/model-stream.test.ts` 验证 Rust mode 下 custom OpenAI-compatible model 会带着 provider id、`baseUrl` 和 request options 进入 Rust JSONL bridge。

## Stage 6: Custom Provider 支持迁移

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

当前状态：

- 对使用 Rust 已支持 API 的 metadata-defined custom provider 已完成。
- `models.json` custom provider 已在 Stage 4 由 Rust registry bridge 加载，并成为普通 `Model` metadata。
- `ROZSA_MODEL_BACKEND=rust` 时，coding-agent 对 Rust 已支持 API 调用 `streamResolvedModel()`，不再进入 TS provider registry。带 `api: "openai-completions"` 的 custom provider 会带着 `baseUrl`、`compat`、headers、`modelsJsonPath` 和 `authJsonPath` 通过 `rozsa-model` 执行。
- Provider-level defaults、model-level overrides、per-model API override、compatibility merge rules 已由 Rust registry bridge 保留。
- Provider-level request headers、`authHeader` 和 `apiKey` 解析已由 Rust bridge 前置 resolver 接管。
- Extension 提供的动态 `streamSimple` handler 已从 `pi-ai` provider registry 迁出。`ModelRegistry` 现在在 coding-agent-owned registry 中保存 handler，`packages/coding-agent/src/core/sdk.ts` 的 stream boundary 会先检查 `getProviderStreamHandler(model.api)`，命中时直接执行该 handler；未命中时才进入 `streamResolvedModel()` / Rust bridge。

下一接手点：

- 若要让 extension stream handler 也完全 Rust-native，需要设计 Rust-side extension provider contract；当前替代方案是 coding-agent-owned local stream handler，不再依赖 TS AI provider registry。

## Stage 7: Agent 直接集成

Only after Rust owns provider execution, registry, metadata, credentials, and custom providers should the agent layer bypass `packages/ai`.

当前 Agent 依赖：

- `packages/agent/src/compat-model-stream.ts` is the only generic Agent source file that imports legacy `streamSimple` as the explicit browser-safe TypeScript fallback.
- `packages/agent/src/missing-model-stream.ts` is the default Agent stream boundary when a caller does not inject model execution; it returns a terminal error instead of falling back to TS AI.
- Agent and coding-agent still import message/model type definitions from `@earendil-works/pi-ai` until a separate public type boundary replaces them.

当前状态：

- 当前 model-layer milestone 内，coding-agent 生产执行路径已完成；通用 Agent 包提供 Node-only direct bridge helper。
- `packages/agent/src/rozsa-model-client.ts` 是 agent-side Rust JSONL client，已从 `packages/ai/src/providers/rozsa-model-bridge.ts` 旁路出来，agent/coding-agent 的 Rust 执行不再依赖 TS AI provider bridge。
- `packages/agent/src/model-stream.ts` 提供 Node-only `streamDefaultModel()` 和 `completeDefaultModel()`。`ROZSA_MODEL_BACKEND=rust` 且 model API 被列入 `ROZSA_MODEL_RUST_APIS` 时，它直接调用 agent-side `streamSimpleRustModel()`；`ROZSA_MODEL_BACKEND=ts` 时才调用 `streamCompatModel()`。
- `packages/agent/src/node.ts` 导出 `streamDefaultModel()`，Node 调用方可以显式使用该 direct bridge boundary。
- `packages/agent/src/harness/agent-harness.ts` 新增 `streamFn` 注入点。默认路径是 `missingModelStream()` fail-fast；Node 调用方可以从 `packages/agent/src/node.ts` 注入 `streamDefaultModel()` 进入 Rust direct bridge。
- `packages/agent/src/harness/compaction/compaction.ts` 和 `packages/agent/src/harness/compaction/branch-summarization.ts` 已改成基于 `StreamFn.result()` 的 completion helper。默认路径是 `missingModelStream()` fail-fast；注入 `streamDefaultModel()` 时总结请求会走 Rust direct bridge。
- `packages/agent/src/agent.ts`、`packages/agent/src/agent-loop.ts`、AgentHarness 和 harness compaction 都不再默认导入或调用 `streamCompatModel()`；legacy TS fallback 必须由调用方显式选择。
- `packages/agent/src/agent-loop.ts` 不再从 `@earendil-works/pi-ai` 导入 runtime `EventStream` 或 `validateToolArguments`。这两个运行时边界已分别迁到 `packages/agent/src/event-stream.ts` 和 `packages/agent/src/tool-validation.ts`。
- `packages/agent/src/proxy.ts` 也改用本地 `EventStream`，避免 proxy runtime 继续依赖 `pi-ai` stream primitive。
- `packages/agent/src/types.ts` 的 `StreamFn` 已改成 agent-owned 结构化 `AssistantMessageEventStream` 接口，不再通过 `typeof streamSimple` 绑定到 `pi-ai` 的 stream class 类型。
- `packages/coding-agent/src/core/sdk.ts` 在 Rust path 下传递 `authJsonPath`、`modelsJsonPath`、retry settings、attribution headers 和 session options 后调用 `streamResolvedModel()`；TS/custom handler path 才调用 `ModelRegistry.getApiKeyAndHeaders()`。
- `packages/coding-agent/src/core/agent-session.ts` 不再导入 `streamSimple` 做函数身份判断；SDK 创建的生产 session 通过 `streamFnResolvesAuth: true` 显式声明 model stream boundary 会在请求内解析 auth。
- `packages/coding-agent/src/core/model-utils.ts` 已接管 `modelsAreEqual()`、`getSupportedThinkingLevels()`、`clampThinkingLevel()` 和 `isContextOverflow()`，coding-agent 不再从 `pi-ai` runtime 导入这些通用 helper。
- `packages/coding-agent/src/core/compaction/compaction.ts`、`packages/coding-agent/src/core/compaction/branch-summarization.ts` 和 `packages/coding-agent/src/core/permissions.ts` 已改用 `completeResolvedModel()`，生产 summary、branch summary 和 auto permission reviewer 不再直接调用 `completeSimple()`。
- `streamResolvedModel()` 通过 `@earendil-works/pi-agent-core/node` 复用通用 Agent Node 边界；Rust request auth/header 由 Rust bridge 根据 `authJsonPath`/`modelsJsonPath` 解析。
- `ROZSA_MODEL_BACKEND=ts` 仍是明确 rollback path，并通过 `streamCompatModel()` 调用 legacy `streamSimple()`。
- Extension 提供的动态 TS provider handler 仍保留在 `ModelRegistry.registerProvider(... streamSimple ...)`，但 handler 已由 coding-agent-owned registry 和 SDK stream boundary 执行，不再注册到 `pi-ai` provider registry。
- `resetApiProviders()` 已从 coding-agent runtime 路径移除。`cleanupSessionResources()` 已收窄到 `packages/agent/src/compat-model-stream.ts`，只服务显式 TS fallback 的 Codex WebSocket session 清理。
- 完全移除 `@earendil-works/pi-ai` 的 compile-time type utilities 和 TS fallback 需要单独的 public API/type boundary 变更。

Focused coverage：

- `packages/agent/test/model-stream.test.ts` 验证 Node-only `streamDefaultModel()` 会在 Rust mode 下使用 Rust bridge，并转发 request `apiKey`。
- `packages/agent/test/agent-loop.test.ts` 和 `packages/agent/test/agent.test.ts` 覆盖迁移后的本地 EventStream/tool validation 边界。
- `packages/coding-agent/test/model-stream.test.ts` 验证 coding-agent 会把 resolved registry auth 和 custom provider metadata 发送到 Rust bridge。
- `packages/coding-agent/test/compaction-summary-reasoning.test.ts` 通过 fake `StreamFn` 覆盖 compaction summary 参数构造，避免继续 mock `completeSimple()`。

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
ROZSA_MODEL_RUST_APIS=openai-completions,bedrock-converse-stream
ROZSA_MODEL_REGISTRY_BACKEND=rust
ROZSA_MODEL_REGISTRY_BACKEND=ts
ROZSA_MODEL_BINARY=/absolute/path/to/rozsa-model
ROZSA_MODEL_TRACE=1
```

Semantics:

- `ts`: always use TS provider implementation.
- `rust`: use Rust for listed APIs; fail if Rust cannot serve them.

`auto` is not supported. Use `ts` for rollback and `rust` for Rust-owned provider execution. `ROZSA_MODEL_REGISTRY_BACKEND` defaults to `rust`; set it to `ts` only when explicitly isolating the TypeScript registry.

## Error Handling Requirements

Rust errors must preserve current expectations:

- Missing API key: fail fast with provider name.
- Missing cloud config: identify missing variable or credential source.
- Provider HTTP error: include provider, status code, and bounded response body.
- Streaming parse error: include event type or parser location when possible.
- Cancellation: return `stopReason: "aborted"`.
- Unknown provider API: return a clear unsupported API error.
- Bridge crash: report child process exit status and stderr excerpt.

Do not hide Rust failures behind silent TS fallback. The rollback path is explicit `ROZSA_MODEL_BACKEND=ts` or `ROZSA_MODEL_REGISTRY_BACKEND=ts`.

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
- AWS Bedrock Converse Stream has a Rust provider adapter and can route through the Rust bridge.
- `stream`, `streamSimple`, `complete`, and `completeSimple` route supported Rust APIs through `rozsa-model` when `ROZSA_MODEL_BACKEND=rust`.
- The TypeScript registry bridge defaults to Rust via `ROZSA_MODEL_REGISTRY_BACKEND=rust`.
- Rust registry loading covers generated model metadata, compatibility metadata, custom model metadata, image model metadata, provider auth availability, and NVIDIA live discovery.
- Rust bridge resolves request credentials from `auth.json`, `models.json`, environment variables, and command-backed config values for Rust-executed requests.
- Rust registry bridge preserves provider-level custom headers, model-level headers, and provider `authHeader` behavior for Rust-executed custom providers.
- Rust and TypeScript provider execution are explicitly separated by `ROZSA_MODEL_BACKEND`; `auto` mode has been removed.
- Rust and TypeScript model registry ownership are explicitly separated by `ROZSA_MODEL_REGISTRY_BACKEND`; `auto` mode has been removed.
- Model protocol types extracted to `@earendil-works/pi-model-types`, breaking the compile-time type dependency on `@earendil-works/pi-ai`.
- `rozsa-model` changed to a long-lived singleton process with concurrent request support via multiplexed JSONL.
- `ROZSA_MODEL_BACKEND` and `ROZSA_MODEL_RUST_APIS` gates removed from agent/coding-agent; model execution always routes through Rust.
- `ROZSA_MODEL_REGISTRY_BACKEND` gate removed; model registry always loads from Rust.
- All type imports in agent/coding-agent migrated from `@earendil-works/pi-ai` to `@earendil-works/pi-model-types`.
- Extension loader registers `@earendil-works/pi-model-types` as a virtual module for extension compatibility.
- `@earendil-works/pi-ai` retained only for OAuth runtime and extension bundling (off the model execution path).
- Agent、AgentHarness 和 agent harness compaction 默认 fail fast，不再隐式回落到 legacy TS `streamSimple()`；coding-agent session auth 判断、model helper、compaction、branch summary 和 auto permission reviewer 已接入 coding-agent-owned boundary，不再直接调用或判断 `completeSimple()` / `streamSimple()`。
- Requests using `onPayload` or `onResponse` fail clearly in Rust mode until callback round-trips exist.
- An ignored live smoke entrypoint exists under `tests/unit/model` for explicit credential-backed checks.
- Rust protocol and fake TypeScript bridge tests live under `tests/unit/model`.
- Provider migration order is defined, but previous 2.3-2.9 provider rollout is deferred.
- Final agent direct integration boundary is defined.

## Remaining

- Define exact `AssistantMessageEvent` JSON schema version.
- Design callback round-trips if `onPayload`/`onResponse` must execute inside the Rust bridge instead of using the TypeScript compatibility route.
- Decide packaging strategy for platform binaries.
- Decide final compatibility story for external `@earendil-works/pi-ai` consumers.
- Move interactive OAuth login and provider-specific OAuth model mutation into Rust only if OAuth needs to become fully Rust-native.
