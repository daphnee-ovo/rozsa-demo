#!/usr/bin/env bash
# Launch Rozsa while forcing the TypeScript AI provider implementations.

set -euo pipefail

forward_args=()
for arg in "$@"; do
	if [[ "$arg" != "--local" ]]; then
		forward_args+=("$arg")
	fi
done

if ((${#forward_args[@]} > 0)); then
	ROZSA_MODEL_BACKEND=ts ROZSA_MODEL_REGISTRY_BACKEND=ts ROZSA_LLAMA_AUTOSTART=0 exec "$(dirname "$0")/run.sh" "${forward_args[@]}"
else
	ROZSA_MODEL_BACKEND=ts ROZSA_MODEL_REGISTRY_BACKEND=ts ROZSA_LLAMA_AUTOSTART=0 exec "$(dirname "$0")/run.sh"
fi
