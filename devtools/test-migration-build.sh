#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
bash devtools/build-migration.sh
npm ci --prefix desktop/js-tests --cache "$project_root/tmp/migration/npm-cache" --no-audit --no-fund
npm --prefix desktop run typecheck
npm --prefix desktop run test:resources
npm --prefix desktop/js-tests test -- session-list
npm --prefix desktop/js-tests test -- session-history
bash devtools/run-migration.sh --prepare-only
