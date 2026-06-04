#!/usr/bin/env bash
set -euo pipefail

ROZSA_LLAMA_SERVER_PID=""
ROZSA_LLAMA_SERVER_BIN="${ROZSA_LLAMA_SERVER_BIN:-llama-server}"
ROZSA_LLAMA_MODEL_PATH="${ROZSA_LLAMA_MODEL_PATH:-$HOME/ballad/LLM/models/qwen2.5-coder-1.5b-instruct-q8_0.gguf}"
ROZSA_LLAMA_HOST="${ROZSA_LLAMA_HOST:-127.0.0.1}"
ROZSA_LLAMA_PORT="${ROZSA_LLAMA_PORT:-18080}"
ROZSA_LLAMA_CONTEXT_SIZE="${ROZSA_LLAMA_CONTEXT_SIZE:-8192}"
ROZSA_LLAMA_BASE_URL="${ROZSA_LLAMA_BASE_URL:-http://$ROZSA_LLAMA_HOST:$ROZSA_LLAMA_PORT/v1}"
ROZSA_LLAMA_AUTOSTART="${ROZSA_LLAMA_AUTOSTART:-0}"

forward_args=()
for arg in "$@"; do
	case "$arg" in
		--local)
			ROZSA_LLAMA_AUTOSTART=1
			;;
		*)
			forward_args+=("$arg")
			;;
	esac
done
if ((${#forward_args[@]} > 0)); then
	set -- "${forward_args[@]}"
else
	set --
fi

# Report launcher progress without mixing it with provider output.
log_launcher() {
	printf '[run.sh] %s\n' "$*" >&2
}

# Decide whether run.sh should manage a local llama.cpp server.
should_autostart_llama() {
	if [[ "$ROZSA_LLAMA_AUTOSTART" == "0" || "$ROZSA_LLAMA_AUTOSTART" == "false" ]]; then
		return 1
	fi
	if [[ "$ROZSA_LLAMA_AUTOSTART" == "1" || "$ROZSA_LLAMA_AUTOSTART" == "true" ]]; then
		return 0
	fi
	return 1
}

# Check whether the configured llama.cpp OpenAI-compatible endpoint is already reachable.
llama_server_ready() {
	command -v curl >/dev/null 2>&1 && curl -fsS "$ROZSA_LLAMA_BASE_URL/models" >/dev/null 2>&1
}

# Stop only the llama.cpp server started by this launcher.
cleanup_llama_server() {
	if [[ -n "$ROZSA_LLAMA_SERVER_PID" ]]; then
		kill "$ROZSA_LLAMA_SERVER_PID" >/dev/null 2>&1 || true
		wait "$ROZSA_LLAMA_SERVER_PID" >/dev/null 2>&1 || true
	fi
}

# Start llama.cpp for the local default model when no server is already running.
start_llama_server_if_needed() {
	if ! should_autostart_llama; then
		return
	fi
	if llama_server_ready; then
		log_launcher "using existing llama.cpp server at $ROZSA_LLAMA_BASE_URL"
		return
	fi
	if ! command -v "$ROZSA_LLAMA_SERVER_BIN" >/dev/null 2>&1; then
		printf 'llama.cpp default model is selected, but %s was not found.\n' "$ROZSA_LLAMA_SERVER_BIN" >&2
		exit 1
	fi
	if [[ ! -f "$ROZSA_LLAMA_MODEL_PATH" ]]; then
		printf 'llama.cpp default model is selected, but model file was not found: %s\n' "$ROZSA_LLAMA_MODEL_PATH" >&2
		exit 1
	fi

	log_launcher "starting llama.cpp server at $ROZSA_LLAMA_BASE_URL"
	"$ROZSA_LLAMA_SERVER_BIN" \
		-m "$ROZSA_LLAMA_MODEL_PATH" \
		--host "$ROZSA_LLAMA_HOST" \
		--port "$ROZSA_LLAMA_PORT" \
		-c "$ROZSA_LLAMA_CONTEXT_SIZE" \
		>/dev/null 2>&1 &
	ROZSA_LLAMA_SERVER_PID="$!"
}

# Wait for llama.cpp to expose its OpenAI-compatible models endpoint.
wait_for_llama_server() {
	if [[ -z "$ROZSA_LLAMA_SERVER_PID" ]]; then
		return
	fi
	for _ in {1..90}; do
		if llama_server_ready; then
			log_launcher "llama.cpp server is ready"
			return
		fi
		sleep 1
	done
	printf 'Timed out waiting for llama.cpp server at %s\n' "$ROZSA_LLAMA_BASE_URL" >&2
	exit 1
}

trap cleanup_llama_server EXIT

cargo build -p rozsa-tui -p rozsa-model -p rozsa-app
clear

if [ ! -x "node_modules/.bin/tsx" ]; then
	echo "tsx not found at node_modules/.bin/tsx. Run npm install --ignore-scripts first." >&2
	exit 1
fi

ts_args_json="$(node -e 'console.log(JSON.stringify(["packages/coding-agent/src/cli.ts", ...process.argv.slice(1)]))' -- "$@")"

start_llama_server_if_needed
wait_for_llama_server

ROZSA_NATIVE_TUI_BACKEND_COMMAND="$PWD/node_modules/.bin/tsx" \
ROZSA_NATIVE_TUI_BACKEND_ARGS_JSON="$ts_args_json" \
ROZSA_APP_BINARY="${ROZSA_APP_BINARY:-$PWD/target/debug/rozsa-app}" \
ROZSA_MODEL_BINARY="${ROZSA_MODEL_BINARY:-$PWD/target/debug/rozsa-model}" \
ROZSA_MODEL_BACKEND="${ROZSA_MODEL_BACKEND:-rust}" \
ROZSA_MODEL_RUST_APIS="${ROZSA_MODEL_RUST_APIS:-openai-completions}" \
target/debug/rozsa-tui
