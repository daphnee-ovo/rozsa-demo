#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
if [[ "${1:-}" == --prepare-only ]]; then
  if [[ $# != 1 ]]; then printf '%s\n' '--prepare-only accepts no other arguments' >&2; exit 2; fi
  npm --prefix desktop run test:resources
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
export npm_config_cache="$project_root/tmp/migration/npm-cache"
export electron_config_cache="$project_root/tmp/migration/electron-cache"
# This isolated candidate does not discover ~/.rozsa or any user sessions.
exec npm --prefix desktop start -- -- --user-data-dir="$project_root/tmp/migration/electron-profile" "${args[@]}"
