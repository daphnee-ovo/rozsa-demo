#!/usr/bin/env bash
# Remote Ollama smoke test for the Rozsa model bridge.
# Covers: authenticated LAN proxy -> models.json -> TS AI layer -> Rust bridge -> remote Ollama.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

PROVIDER="${ROZSA_REMOTE_OLLAMA_PROVIDER:-ollama-lan}"
MODEL_ID="${ROZSA_REMOTE_OLLAMA_MODEL:-qwen3.5:latest}"
AGENT_DIR="${ROZSA_AGENT_DIR:-$HOME/.rozsa/agent}"
MODELS_JSON="${ROZSA_MODELS_JSON:-$AGENT_DIR/models.json}"
RUST_BINARY="${ROZSA_MODEL_BINARY:-$REPO_ROOT/target/debug/rozsa-model}"

# Print one high-signal progress line.
log() {
	printf '[remote-ollama-smoke] %s\n' "$*"
}

# Fail fast when a required command is unavailable.
require_command() {
	local command_name="$1"
	if ! command -v "$command_name" >/dev/null 2>&1; then
		printf 'Missing required command: %s\n' "$command_name" >&2
		exit 1
	fi
}

# Read one provider field from models.json using the same provider/model names as the test.
model_config_field() {
	local field="$1"
	ROZSA_MODELS_JSON="$MODELS_JSON" \
	ROZSA_REMOTE_OLLAMA_PROVIDER="$PROVIDER" \
	ROZSA_REMOTE_OLLAMA_MODEL="$MODEL_ID" \
	ROZSA_CONFIG_FIELD="$field" \
	node -e '
const fs = require("node:fs");
const config = JSON.parse(fs.readFileSync(process.env.ROZSA_MODELS_JSON, "utf8"));
const provider = config.providers?.[process.env.ROZSA_REMOTE_OLLAMA_PROVIDER];
if (!provider) throw new Error(`provider not found: ${process.env.ROZSA_REMOTE_OLLAMA_PROVIDER}`);
const model = provider.models?.find((entry) => entry.id === process.env.ROZSA_REMOTE_OLLAMA_MODEL);
if (!model) throw new Error(`model not found: ${process.env.ROZSA_REMOTE_OLLAMA_MODEL}`);
const values = { baseUrl: provider.baseUrl, apiKey: provider.apiKey, modelId: model.id };
const value = values[process.env.ROZSA_CONFIG_FIELD];
if (!value) throw new Error(`missing field: ${process.env.ROZSA_CONFIG_FIELD}`);
process.stdout.write(value);
'
}

# Build the Rust bridge binary used by TypeScript bridge tests.
build_rust_binary() {
	log "building rozsa-model"
	cargo build -p rozsa-model
}

# Verify the proxy rejects unauthenticated LAN requests.
verify_auth_required() {
	local base_url="$1"
	local status
	status="$(curl -sS -o /dev/null -w '%{http_code}' "$base_url/models")"
	if [[ "$status" != "401" ]]; then
		printf 'Expected unauthenticated /models to return 401, got %s\n' "$status" >&2
		exit 1
	fi
	log "unauthenticated request returned 401"
}

# Verify the proxy accepts the configured API key and exposes the model list.
verify_models_endpoint() {
	local base_url="$1"
	local api_key="$2"
	curl -sS -H "Authorization: Bearer $api_key" "$base_url/models" | grep -q "$MODEL_ID"
	log "authenticated /models contains $MODEL_ID"
}

# Verify TS AI streaming receives remote Ollama reasoning deltas through the Rust bridge.
verify_streaming_events() {
	log "running TS AI -> Rust bridge -> remote Ollama streaming check"
	ROZSA_MODEL_BACKEND=rust \
	ROZSA_MODEL_RUST_APIS=openai-completions \
	ROZSA_MODEL_BINARY="$RUST_BINARY" \
	ROZSA_MODELS_JSON="$MODELS_JSON" \
	ROZSA_AGENT_DIR="$AGENT_DIR" \
	ROZSA_REMOTE_OLLAMA_PROVIDER="$PROVIDER" \
	ROZSA_REMOTE_OLLAMA_MODEL="$MODEL_ID" \
	node --import tsx -e '
import { streamSimple } from "./packages/ai/src/stream.ts";
import { AuthStorage } from "./packages/coding-agent/src/core/auth-storage.ts";
import { ModelRegistry } from "./packages/coding-agent/src/core/model-registry.ts";
const registry = ModelRegistry.create(
  AuthStorage.create(`${process.env.ROZSA_AGENT_DIR}/auth.json`),
  process.env.ROZSA_MODELS_JSON
);
const error = registry.getError();
if (error) throw new Error(error);
const model = registry.find(process.env.ROZSA_REMOTE_OLLAMA_PROVIDER, process.env.ROZSA_REMOTE_OLLAMA_MODEL);
if (!model) throw new Error("model not found");
const auth = await registry.getApiKeyAndHeaders(model);
if (!auth.ok) throw new Error(auth.error);
const stream = streamSimple(
  model,
  { systemPrompt: "Be concise.", messages: [{ role: "user", content: "Reply with exactly: ok", timestamp: 1 }] },
  { apiKey: auth.apiKey, headers: auth.headers, maxTokens: 32, temperature: 0 }
);
let thinkingDeltas = 0;
for await (const event of stream) {
  if (event.type === "thinking_delta") thinkingDeltas++;
}
const result = await stream.result();
if (thinkingDeltas === 0) throw new Error("no thinking_delta events received");
if (result.stopReason === "error") throw new Error(JSON.stringify(result));
console.log(JSON.stringify({ stopReason: result.stopReason, thinkingDeltas }));
'
}

require_command cargo
require_command curl
require_command node

BASE_URL="$(model_config_field baseUrl)"
API_KEY="$(model_config_field apiKey)"

build_rust_binary
verify_auth_required "$BASE_URL"
verify_models_endpoint "$BASE_URL" "$API_KEY"
verify_streaming_events

log "all remote Ollama smoke checks passed"
