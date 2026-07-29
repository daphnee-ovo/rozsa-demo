# rozsa-model supported providers

`rozsa-model` registers the provider implementations in
[`providers/mod.rs`](../../crates/rozsa-model/src/providers/mod.rs). A protocol
is supported when it has a Rust `ApiProvider` implementation and is registered
by `register_builtin_providers`.

| API protocol | Implementation | Notes |
| --- | --- | --- |
| OpenAI Chat Completions | `providers/openai_completions.rs` | Streaming chat completions, compatible model metadata, and SSE normalization |
| OpenAI Responses | `providers/openai_responses/` | `POST /v1/responses`, streaming response events, tools, and reasoning conversion |
| Anthropic Messages | `providers/anthropic/` | Messages API payloads, SSE, tool calls, and OAuth token handling |
| AWS Bedrock Converse Stream | `providers/bedrock/` | Bedrock SDK ConverseStream payload and event conversion |

## Credentials and models

`rozsa-app` owns model metadata discovery and configuration. `rozsa-model`
resolves request credentials from the configured local files and environment at
request time; errors are reported to the caller instead of selecting an
unrelated provider.

Custom OpenAI-compatible models use the `OpenAICompletions` protocol and their
configured base URL, headers, and credentials. They do not require a separate
compatibility process.

## Provider work not yet implemented

New protocols are product work, not compatibility placeholders. Add one only
with a stable provider contract, focused payload/stream tests, credential
handling, and explicit registration in `register_builtin_providers`.

Related documentation: [model configuration](./models-config.md) and [OAuth
architecture](./oauth-architecture.md).
