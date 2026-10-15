#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
bash devtools/build-migration.sh
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
go -C backend test ./internal/app/session/... ./internal/transport/...
npm ci --prefix desktop/js-tests --no-audit --no-fund
npm --prefix desktop run typecheck
npm --prefix desktop run test:resources
npm --prefix desktop/js-tests test -- session-list
npm --prefix desktop/js-tests test -- session-history
bash devtools/run-migration.sh --prepare-only
