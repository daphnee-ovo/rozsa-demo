#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/migration/{install-tmp,packager}
export TMPDIR="$project_root/tmp/migration/install-tmp"
# Dependencies must already be installed. Forge validates TS7 and builds Go first.
npm --prefix desktop run package -- "$@"
