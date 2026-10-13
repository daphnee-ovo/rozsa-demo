#!/usr/bin/env bash
# T003 cross-language completion target. Assertions belong in Go/TS modules.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
mkdir -p tmp/migration/{go-cache,go-tmp,go-mod-cache,install-tmp,npm-cache} dist/migration
export GOCACHE="$project_root/tmp/migration/go-cache"
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export GOMODCACHE="$project_root/tmp/migration/go-mod-cache"
export TMPDIR="$project_root/tmp/migration/install-tmp"
export npm_config_cache="$project_root/tmp/migration/npm-cache"
go -C backend test ./internal/app/session/... ./internal/transport/...
backend_name=rozsa-backend
if [[ "$(go env GOOS)" == windows ]]; then backend_name=rozsa-backend.exe; fi
go -C backend build -o "$project_root/dist/migration/$backend_name" ./cmd/rozsa-backend
npm --prefix desktop run typecheck
npm --prefix desktop run package
npm --prefix desktop/js-tests test -- session-history
npm --prefix desktop/js-tests test -- session-list
bash devtools/run-migration.sh --prepare-only
# Native GUI acceptance is explicit and separate from task completion.
