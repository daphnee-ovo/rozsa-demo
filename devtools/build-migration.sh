#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
# Generated artifacts and test temporary files stay within this project.
# Tool caches retain their default configuration and locations.
mkdir -p tmp/migration/{go-tmp,install-tmp} dist/migration
export GOTMPDIR="$project_root/tmp/migration/go-tmp"
export TMPDIR="$project_root/tmp/migration/install-tmp"
npm --prefix desktop ci --no-audit --no-fund
(cd desktop && npm exec -- install-electron)
# Forge hooks check TS7 and build Go; Vite alone emits application code/resources.
npm --prefix desktop run package
