#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
backend_name=rozsa-backend
if [[ "$(go env GOOS)" == windows ]]; then backend_name=rozsa-backend.exe; fi
for path in "dist/migration/$backend_name" desktop/dist/node/package.json desktop/dist/node/main/entry.js desktop/dist/node/main/entry.js.map desktop/dist/preload.cjs desktop/dist/preload.cjs.map desktop/dist/browser/renderer/index.html desktop/dist/browser/renderer/entry.js desktop/dist/browser/renderer/entry.js.map desktop/dist/browser/main/session-contract.js desktop/dist/browser/ui/tokens.css desktop/dist/browser/ui/sidebar.css desktop/node_modules/electron/path.txt; do
  if [[ ! -f "$path" ]]; then printf 'Missing candidate file: %s; run bash devtools/build-migration.sh\n' "$path" >&2; exit 1; fi
done
electron_binary="desktop/node_modules/electron/dist/$(cat desktop/node_modules/electron/path.txt)"
if [[ ! -f "$electron_binary" ]]; then printf 'Missing Electron binary: %s; rebuild candidate\n' "$electron_binary" >&2; exit 1; fi
if [[ "${1:-}" == --prepare-only ]]; then
  if [[ $# != 1 ]]; then printf '%s\n' '--prepare-only accepts no other arguments' >&2; exit 2; fi
  printf 'Migration candidate ready: %s\n' "$project_root/desktop"
  exit 0
fi
args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --session-dir)
      if [[ $# -lt 2 || -z "$2" || "$2" == --* ]]; then printf '%s\n' '--session-dir requires a directory' >&2; exit 2; fi
      args+=(--session-dir "$2"); shift 2 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
mkdir -p tmp/migration/electron-profile tmp/migration/install-tmp
export TMPDIR="$project_root/tmp/migration/install-tmp"
# This isolated candidate does not discover ~/.rozsa or any user sessions.
exec "$electron_binary" desktop --user-data-dir="$project_root/tmp/migration/electron-profile" "${args[@]}"
