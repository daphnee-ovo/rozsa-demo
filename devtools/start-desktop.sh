#!/usr/bin/env bash
# Start Electron + Go with a repository-local config root unless the caller overrides it.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

for command_name in go node npm; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s. Install the project prerequisites described in CONTRIBUTING.md.\n' "$command_name" >&2
    exit 69
  fi
done

if [[ ! -x desktop/node_modules/.bin/electron-forge ]]; then
  printf 'Desktop dependencies are not installed. Run: bash devtools/build-desktop.sh\n' >&2
  exit 69
fi

export ROZSA_CONFIG_DIR="${ROZSA_CONFIG_DIR-$project_root/tmp/desktop-debug}"

node devtools/update-builtin-skills-config.mjs
exec npm --prefix desktop start -- -- "$@"
