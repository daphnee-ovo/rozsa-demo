#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
command -v go >/dev/null
command -v node >/dev/null
command -v npm >/dev/null
# Generated artifacts and test temporary files stay within this project.
# Tool caches retain their default configuration and locations.
mkdir -p tmp/migration/{go-tmp,install-tmp} dist
export GOTMPDIR="$project_root/tmp/go-tmp"
export TMPDIR="$project_root/tmp/install-tmp"
npm --prefix desktop ci --no-audit --no-fund
(cd desktop && npm exec -- install-electron)
# The packaging script injects the built-in skill inventory before Forge builds Go.
./devtools/package-desktop.sh package
