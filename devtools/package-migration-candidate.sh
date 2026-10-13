#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/migration/{install-tmp,npm-cache,electron-cache,packager}
export TMPDIR="$project_root/tmp/migration/install-tmp"
export npm_config_cache="$project_root/tmp/migration/npm-cache"
export electron_config_cache="$project_root/tmp/migration/electron-cache"
# Dependencies must already be installed. Forge validates TS7 and builds Go first.
npm --prefix desktop run package -- "$@"
