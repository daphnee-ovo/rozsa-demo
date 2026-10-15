#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
bash devtools/build-migration.sh
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
go -C backend test -count=1 ./internal/app/session/... ./internal/transport/...
npm ci --prefix desktop/js-tests --no-audit --no-fund
npm --prefix desktop/js-tests test -- session-list
npm --prefix desktop/js-tests test -- session-history
npm --prefix desktop/js-tests run test:session-contract
desktop/js-tests/node_modules/.bin/tsx desktop/js-tests/bridge.test.ts
desktop/js-tests/node_modules/.bin/tsx desktop/js-tests/lifecycle.test.ts
bash devtools/run-migration.sh --prepare-only
