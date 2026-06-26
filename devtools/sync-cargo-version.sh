#!/usr/bin/env bash
# 将 workspace Cargo.toml [workspace.package] 的 version 同步为指定值
# 用法: ./devtools/sync-cargo-version.sh 1.0.5
# /iterate 在版本号 bump 时调用，使所有 crate 通过 workspace 继承统一升级

set -euo pipefail

VERSION="${1:?Usage: sync-cargo-version.sh <version>}"
CARGO_TOML="Cargo.toml"

if [[ ! -f "$CARGO_TOML" ]]; then
    echo "ERROR: $CARGO_TOML not found (run from workspace root)"
    exit 1
fi

if ! grep -q '^\[workspace.package\]' "$CARGO_TOML"; then
    echo "ERROR: [workspace.package] not found in $CARGO_TOML"
    exit 1
fi

# 仅替换 [workspace.package] 段内的 version 行（限定到段首到下一个 [ 之间）
awk -v ver="$VERSION" '
    /^\[workspace\.package\]/ { in_section=1; print; next }
    /^\[/ && !/^\[workspace\.package\]/ { in_section=0 }
    in_section && /^version = / { print "version = \"" ver "\""; next }
    { print }
' "$CARGO_TOML" > "$CARGO_TOML.tmp"
mv "$CARGO_TOML.tmp" "$CARGO_TOML"

echo "Updated workspace version to $VERSION"
cargo metadata --no-deps --format-version=1 \
    | grep -oE '"name":"rozsa-[^"]*","version":"[^"]*"' \
    || true
