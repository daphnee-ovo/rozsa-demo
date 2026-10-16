#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ ${ROZSA_MIGRATION_SOURCE_ROOT+x} ]]; then
  if [[ "$ROZSA_MIGRATION_SOURCE_ROOT" != /* ]]; then
    printf '%s\n' 'ROZSA_MIGRATION_SOURCE_ROOT must be an absolute acceptance source directory' >&2
    exit 64
  fi
  project_root="$ROZSA_MIGRATION_SOURCE_ROOT"
fi
cd "$project_root"
for manifest in backend/go.mod desktop/package.json desktop/tests/package.json; do
  if [[ ! -f "$manifest" ]]; then
    printf 'Missing acceptance source manifest: %s/%s\n' "$project_root" "$manifest" >&2
    exit 66
  fi
done
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
bash devtools/build-migration.sh
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
go -C backend test -count=1 ./internal/app/session/... ./internal/transport/...
npm ci --prefix desktop/tests --no-audit --no-fund
npm --prefix desktop/tests test
bash devtools/run-migration.sh --prepare-only
