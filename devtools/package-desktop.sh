#!/usr/bin/env bash
# Package the current Electron + Go desktop tree, or build its installers with `make`.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
mkdir -p tmp/{install-tmp,packager}
export TMPDIR="$project_root/tmp/install-tmp"
# Dependencies must already be installed. Forge checks TS7 and builds Go before packaging.
command="package"
if [[ "${1:-}" == "make" || "${1:-}" == "package" ]]; then command="$1"; shift; fi
npm --prefix desktop run "$command" -- "$@"
