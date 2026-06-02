#!/usr/bin/env bash
set -euo pipefail

cargo build -p rozsa-tui -p rozsa-model
clear

if [ ! -x "node_modules/.bin/tsx" ]; then
	echo "tsx not found at node_modules/.bin/tsx. Run npm install --ignore-scripts first." >&2
	exit 1
fi

ts_args_json="$(node -e 'console.log(JSON.stringify(["packages/coding-agent/src/cli.ts", ...process.argv.slice(1)]))' "$@")"

ROZSA_NATIVE_TUI_BACKEND_COMMAND="$PWD/node_modules/.bin/tsx" \
ROZSA_NATIVE_TUI_BACKEND_ARGS_JSON="$ts_args_json" \
ROZSA_MODEL_BINARY="${ROZSA_MODEL_BINARY:-$PWD/target/debug/rozsa-model}" \
ROZSA_MODEL_BACKEND="${ROZSA_MODEL_BACKEND:-auto}" \
ROZSA_MODEL_RUST_APIS="${ROZSA_MODEL_RUST_APIS:-openai-completions}" \
target/debug/rozsa-tui
