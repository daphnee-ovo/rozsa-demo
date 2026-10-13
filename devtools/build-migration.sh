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
npm --prefix desktop ci --no-audit --no-fund
(cd desktop && npm exec -- install-electron)
# Forge hooks check TS7 and build Go; Vite alone emits application code/resources.
npm --prefix desktop run package
