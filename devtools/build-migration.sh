#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
# Tool temporary files/caches and generated artifacts stay within this project.
mkdir -p tmp/migration/{go-cache,go-tmp,go-mod-cache,npm-cache,electron-cache,install-tmp} dist/migration
export GOCACHE="$project_root/tmp/migration/go-cache"
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export GOMODCACHE="$project_root/tmp/migration/go-mod-cache"
export TMPDIR="$project_root/tmp/migration/install-tmp"
export npm_config_cache="$project_root/tmp/migration/npm-cache"
export ELECTRON_CACHE="$project_root/tmp/migration/electron-cache"
export electron_config_cache="$project_root/tmp/migration/electron-cache"
backend_name=rozsa-backend
if [[ "$(go env GOOS)" == windows ]]; then backend_name=rozsa-backend.exe; fi
go -C backend build -o "$project_root/dist/migration/$backend_name" ./cmd/rozsa-backend
npm --prefix desktop ci --no-audit --no-fund
(cd desktop && npm exec -- install-electron)
npm --prefix desktop run build
# npm build generates the HTML template/shared CSS and source maps as well as JS.
printf 'Migration candidate prepared: %s\n' "$project_root/desktop"
