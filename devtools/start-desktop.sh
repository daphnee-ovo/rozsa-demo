#!/usr/bin/env bash
# Start the current Electron + Go Rózsa development app and forward app arguments.
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

node devtools/update-builtin-skills-config.mjs
exec npm --prefix desktop start -- -- "$@"
