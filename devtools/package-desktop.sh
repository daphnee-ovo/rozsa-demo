#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/migration/{install-tmp,packager}
export TMPDIR="$project_root/tmp/migration/install-tmp"
# Dependencies must already be installed. Forge validates TS7 and builds Go first.
command="package"
if [[ "${1:-}" == "make" || "${1:-}" == "package" ]]; then command="$1"; shift; fi
npm --prefix desktop run "$command" -- "$@"
