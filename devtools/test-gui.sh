#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mode="${1:---all}"
case "$mode" in
  --all|--js|--rust) ;;
  --help)
    printf '%s\n' 'Usage: bash devtools/test-gui.sh [--all|--js|--rust]' \
      'Runs the GUI JavaScript suite and/or Rust tests (library and integration).' \
      'Native UI and visual acceptance require a separate app verification.'
    exit 0 ;;
  *) printf 'Unknown option: %s\n' "$mode" >&2; exit 2 ;;
esac
cd -- "$repo_dir"
if [[ "$mode" != --js ]]; then
  command -v cargo >/dev/null
fi
if [[ "$mode" != --rust ]]; then
  command -v node >/dev/null
  command -v npm >/dev/null
  if [[ ! -d crates/rozsa-gui/js-tests/node_modules/jsdom ]]; then
    printf '%s\n' 'Missing JS test dependencies. Run: npm ci --prefix crates/rozsa-gui/js-tests' >&2
    exit 1
  fi
  npm test --prefix crates/rozsa-gui/js-tests
fi
if [[ "$mode" != --js ]]; then
  cargo test -p rozsa-gui --tests
fi
