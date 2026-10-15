#!/usr/bin/env bash
# T067 cross-language acceptance; assertions remain in the Go/TS modules.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/migration/{go-tmp,install-tmp}
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
# Keep Go/npm/Electron caches at their default locations.
go -C backend version
go -C backend env GOTOOLCHAIN GOEXPERIMENT
go -C backend mod verify
go -C backend test -count=1 ./...
go -C backend list -f '{{.ImportPath}}: {{.GoFiles}}' encoding/json github.com/go-json-experiment/json github.com/go-json-experiment/json/jsontext
npm --prefix desktop run package
npm --prefix desktop/js-tests run test:go-toolchain
npm --prefix desktop/js-tests run test:session-contract
npm --prefix desktop/js-tests test -- session-list
npm --prefix desktop/js-tests test -- session-history
bash devtools/run-migration.sh --prepare-only
