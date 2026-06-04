#!/usr/bin/env bash
# Local llama.cpp smoke test for the Rozsa model bridge.
# Covers: models.json -> TS AI layer -> Rust bridge -> Rust openai-completions provider -> llama.cpp.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

LLAMA_SERVER_BIN="${LLAMA_SERVER_BIN:-llama-server}"
MODEL_PATH="${ROZSA_MODEL_LIVE_MODEL_PATH:-$HOME/ballad/LLM/models/qwen2.5-coder-1.5b-instruct-q8_0.gguf}"
MODEL_ID="${ROZSA_MODEL_LIVE_MODEL:-$(basename "$MODEL_PATH")}"
HOST="${ROZSA_MODEL_LIVE_HOST:-127.0.0.1}"
PORT="${ROZSA_MODEL_LIVE_PORT:-18080}"
BASE_URL="${ROZSA_MODEL_LIVE_BASE_URL:-http://$HOST:$PORT/v1}"
API_KEY="${ROZSA_MODEL_LIVE_API_KEY:-dummy}"
CTX_SIZE="${ROZSA_MODEL_LIVE_CONTEXT_SIZE:-8192}"
AGENT_DIR="${ROZSA_AGENT_DIR:-$HOME/.rozsa/agent}"
MODELS_JSON="${ROZSA_MODELS_JSON:-$AGENT_DIR/models.json}"
RUST_BINARY="${ROZSA_MODEL_BINARY:-$REPO_ROOT/target/debug/rozsa-model}"
SERVER_PID=""

# Print one high-signal progress line.
log() {
	printf '[local-llamacpp-smoke] %s\n' "$*"
}

# Fail fast when a required command is unavailable.
require_command() {
	local command_name="$1"
	if ! command -v "$command_name" >/dev/null 2>&1; then
		printf 'Missing required command: %s\n' "$command_name" >&2
		exit 1
	fi
}

# Check whether the configured llama.cpp OpenAI-compatible endpoint is reachable.
server_ready() {
	curl -fsS "$BASE_URL/models" >/dev/null 2>&1
}

# Stop a llama.cpp server started by this script.
cleanup() {
	if [[ -n "$SERVER_PID" ]]; then
		kill "$SERVER_PID" >/dev/null 2>&1 || true
		wait "$SERVER_PID" >/dev/null 2>&1 || true
	fi
}

# Start llama.cpp only when the configured endpoint is not already running.
start_llama_server_if_needed() {
	if server_ready; then
		log "llama.cpp server already reachable at $BASE_URL"
		return
	fi

	if [[ ! -f "$MODEL_PATH" ]]; then
		printf 'Model file not found: %s\n' "$MODEL_PATH" >&2
		exit 1
	fi

	log "starting llama.cpp server on $HOST:$PORT"
	"$LLAMA_SERVER_BIN" -m "$MODEL_PATH" --host "$HOST" --port "$PORT" -c "$CTX_SIZE" >/dev/null 2>&1 &
	SERVER_PID="$!"
}

# Wait until llama.cpp reports a usable OpenAI-compatible /models endpoint.
wait_for_server() {
	local attempts=90
	local i
	for ((i = 1; i <= attempts; i++)); do
		if server_ready; then
			log "llama.cpp server is ready"
			return
		fi
		sleep 1
	done

	printf 'Timed out waiting for llama.cpp server at %s\n' "$BASE_URL" >&2
	exit 1
}

# Build the Rust bridge binary used by TypeScript bridge tests.
build_rust_binary() {
	log "building rozsa-model"
	cargo build -p rozsa-model
}

# Upsert the local llama.cpp provider into ~/.rozsa/agent/models.json.
write_models_json() {
	log "writing $MODELS_JSON"
	ROZSA_MODELS_JSON="$MODELS_JSON" \
	ROZSA_MODEL_LIVE_BASE_URL="$BASE_URL" \
	ROZSA_MODEL_LIVE_MODEL="$MODEL_ID" \
	ROZSA_MODEL_LIVE_API_KEY="$API_KEY" \
	node -e '
const fs = require("node:fs");
const path = require("node:path");
const modelsPath = process.env.ROZSA_MODELS_JSON;
const baseUrl = process.env.ROZSA_MODEL_LIVE_BASE_URL;
const modelId = process.env.ROZSA_MODEL_LIVE_MODEL;
const apiKey = process.env.ROZSA_MODEL_LIVE_API_KEY;
let config = { providers: {} };
if (fs.existsSync(modelsPath)) {
  config = JSON.parse(fs.readFileSync(modelsPath, "utf8"));
}
config.providers ||= {};
config.providers.llamacpp = {
  name: "llama.cpp Local",
  baseUrl,
  api: "openai-completions",
  apiKey,
  compat: {
    supportsStore: false,
    supportsDeveloperRole: false,
    supportsReasoningEffort: false,
    supportsUsageInStreaming: true,
    maxTokensField: "max_tokens"
  },
  models: [{
    id: modelId,
    name: "Qwen2.5 Coder 1.5B Instruct Q8 (llama.cpp)",
    reasoning: false,
    input: ["text"],
    contextWindow: 8192,
    maxTokens: 1024,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }
  }]
};
fs.mkdirSync(path.dirname(modelsPath), { recursive: true });
fs.writeFileSync(modelsPath, JSON.stringify(config, null, 2) + "\n", { mode: 0o644 });
'
}

# Verify the coding-agent ModelRegistry can load the local model configuration.
verify_models_json() {
	log "verifying ModelRegistry can read llamacpp/$MODEL_ID"
	ROZSA_MODELS_JSON="$MODELS_JSON" \
	ROZSA_AGENT_DIR="$AGENT_DIR" \
	ROZSA_MODEL_LIVE_MODEL="$MODEL_ID" \
	node --import tsx -e '
import { AuthStorage } from "./packages/coding-agent/src/core/auth-storage.ts";
import { ModelRegistry } from "./packages/coding-agent/src/core/model-registry.ts";
const modelsPath = process.env.ROZSA_MODELS_JSON;
const agentDir = process.env.ROZSA_AGENT_DIR;
const modelId = process.env.ROZSA_MODEL_LIVE_MODEL;
const registry = ModelRegistry.create(AuthStorage.create(`${agentDir}/auth.json`), modelsPath);
const error = registry.getError();
if (error) throw new Error(error);
const model = registry.find("llamacpp", modelId);
if (!model) throw new Error(`model not found: llamacpp/${modelId}`);
if (!registry.getAvailable().some((entry) => entry.provider === "llamacpp" && entry.id === modelId)) {
  throw new Error(`model is not available: llamacpp/${modelId}`);
}
console.log(JSON.stringify({ provider: model.provider, id: model.id, api: model.api, baseUrl: model.baseUrl }));
'
}

# Run the Rust provider live smoke against llama.cpp.
run_rust_live_smoke() {
	log "running Rust openai-completions live smoke"
	ROZSA_MODEL_LIVE_OPENAI_COMPLETIONS=1 \
	ROZSA_MODEL_LIVE_BASE_URL="$BASE_URL" \
	ROZSA_MODEL_LIVE_MODEL="$MODEL_ID" \
	ROZSA_MODEL_LIVE_API_KEY="$API_KEY" \
	cargo test -p rozsa-model --test unit_model_openai_completions live_openai_completions_smoke_when_enabled -- --ignored --exact
}

# Run fake parity and live TS bridge tests through Vitest.
run_ts_bridge_smoke() {
	log "running TypeScript bridge parity and live smoke"
	ROZSA_MODEL_LIVE_TS_BRIDGE=1 \
	ROZSA_MODEL_LIVE_BASE_URL="$BASE_URL" \
	ROZSA_MODEL_LIVE_MODEL="$MODEL_ID" \
	ROZSA_MODEL_LIVE_API_KEY="$API_KEY" \
	ROZSA_MODEL_BINARY="$RUST_BINARY" \
	node node_modules/vitest/dist/cli.js --run --api.host 127.0.0.1 tests/unit/model/openai-completions-parity.test.ts tests/unit/model/rozsa-model-bridge.test.ts
}

# Verify the real agent-facing path reads models.json and completes via Rust bridge.
run_registry_complete_smoke() {
	log "running models.json -> TS AI -> Rust bridge completion smoke"
	ROZSA_MODEL_BACKEND=rust \
	ROZSA_MODEL_RUST_APIS=openai-completions \
	ROZSA_MODEL_BINARY="$RUST_BINARY" \
	ROZSA_MODELS_JSON="$MODELS_JSON" \
	ROZSA_AGENT_DIR="$AGENT_DIR" \
	ROZSA_MODEL_LIVE_MODEL="$MODEL_ID" \
	node --import tsx -e '
import { completeSimple } from "./packages/ai/src/stream.ts";
import { AuthStorage } from "./packages/coding-agent/src/core/auth-storage.ts";
import { ModelRegistry } from "./packages/coding-agent/src/core/model-registry.ts";
const registry = ModelRegistry.create(
  AuthStorage.create(`${process.env.ROZSA_AGENT_DIR}/auth.json`),
  process.env.ROZSA_MODELS_JSON
);
const model = registry.find("llamacpp", process.env.ROZSA_MODEL_LIVE_MODEL);
if (!model) throw new Error("model not found");
const auth = await registry.getApiKeyAndHeaders(model);
if (!auth.ok) throw new Error(auth.error);
const result = await completeSimple(
  model,
  { systemPrompt: "Be concise.", messages: [{ role: "user", content: "Reply with exactly: ok", timestamp: 1 }] },
  { apiKey: auth.apiKey, headers: auth.headers, maxTokens: 16, temperature: 0 }
);
const text = result.content.filter((block) => block.type === "text").map((block) => block.text).join("");
if (result.stopReason === "error" || !text.trim()) {
  throw new Error(JSON.stringify(result));
}
console.log(JSON.stringify({ provider: result.provider, model: result.model, stopReason: result.stopReason, text }));
'
}

trap cleanup EXIT

require_command "$LLAMA_SERVER_BIN"
require_command cargo
require_command curl
require_command node

build_rust_binary
write_models_json
start_llama_server_if_needed
wait_for_server
verify_models_json
run_rust_live_smoke
run_ts_bridge_smoke
run_registry_complete_smoke

log "all local llama.cpp smoke checks passed"
