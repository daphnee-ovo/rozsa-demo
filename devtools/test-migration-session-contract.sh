#!/usr/bin/env bash
# T010 orchestration only; assertions live beside the Go/TS implementations.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/migration/{go-tmp,install-tmp} dist/migration
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
go -C backend test -count=1 ./internal/app/session/... ./internal/transport/...
backend_name=rozsa-backend
if [[ "$(go env GOOS)" == windows ]]; then backend_name=rozsa-backend.exe; fi
go -C backend build -o "$project_root/dist/migration/$backend_name" ./cmd/rozsa-backend
npm --prefix desktop run typecheck
npm --prefix desktop run package
npm --prefix desktop run test:resources
npm --prefix desktop/js-tests run test:session-contract
npm --prefix desktop/js-tests test -- session-list
npm --prefix desktop/js-tests test -- session-history
bash devtools/run-migration.sh --prepare-only
# No GUI startup, full-project TEST, or production data in this focused target.
