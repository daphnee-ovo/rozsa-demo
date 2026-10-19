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

app_args=("$@")
has_session_directory=false
for argument in "$@"; do
  case "$argument" in
    --session-dir|--session-dir=*) has_session_directory=true ;;
  esac
done
if [[ "$has_session_directory" == false ]]; then
  sample_directory="$(node devtools/prepare-debug-sessions.mjs)"
  app_args+=(--session-dir "$sample_directory")
  printf 'Loading synthetic debug sessions: %s\nExisting session files are preserved. Use --session-dir to load your own directory.\n' "$sample_directory"
fi

node devtools/update-builtin-skills-config.mjs
exec npm --prefix desktop start -- -- "${app_args[@]}"
