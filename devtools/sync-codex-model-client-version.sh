#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CODEX_REPO_URL="https://github.com/openai/codex.git"
TARGET="$PROJECT_ROOT/crates/rozsa-model/src/models_endpoint.rs"
GO_TARGET="$PROJECT_ROOT/backend/internal/model/modelsendpoint/models.go"
GO_TARGET_EXPLICIT=false
TARGET_EXPLICIT=false
CHECK_ONLY=false

usage() {
    cat <<'EOF'
Usage: sync-codex-model-client-version.sh [options]

Options:
  --repo-url URL     Codex git remote (default: openai/codex on GitHub)
  --target PATH      models_endpoint.rs to update (primarily for tests)
  --go-target PATH   Go models endpoint client to update (primarily for tests)
  --check            Verify the target already matches; do not write
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo-url)
            [[ $# -ge 2 ]] || { echo "ERROR: --repo-url requires a URL" >&2; exit 2; }
            CODEX_REPO_URL="$2"
            shift 2
            ;;
        --target)
            [[ $# -ge 2 ]] || { echo "ERROR: --target requires a path" >&2; exit 2; }
            TARGET="$2"
            TARGET_EXPLICIT=true
            shift 2
            ;;
        --go-target)
            [[ $# -ge 2 ]] || { echo "ERROR: --go-target requires a path" >&2; exit 2; }
            GO_TARGET="$2"
            GO_TARGET_EXPLICIT=true
            shift 2
            ;;
        --check)
            CHECK_ONLY=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ "$TARGET_EXPLICIT" == true && "$GO_TARGET_EXPLICIT" == false ]]; then
    GO_TARGET=""
fi

[[ -f "$TARGET" ]] || {
    echo "ERROR: target file not found: $TARGET" >&2
    exit 1
}
[[ -z "$GO_TARGET" || -f "$GO_TARGET" ]] || {
    echo "ERROR: Go target file not found: $GO_TARGET" >&2
    exit 1
}

LATEST_TAG=""
VERSION=""
BEST_MAJOR=-1
BEST_MINOR=-1
BEST_PATCH=-1
REMOTE_TAGS="$(git ls-remote --tags --refs "$CODEX_REPO_URL" 'refs/tags/rust-v*')" || {
    echo "ERROR: failed to read Codex release tags from $CODEX_REPO_URL" >&2
    exit 1
}
while read -r _ ref; do
    tag="${ref#refs/tags/}"
    if [[ "$tag" =~ ^rust-v([0-9]+)\.([0-9]+)\.([0-9]+)(-(alpha|beta)(\.[0-9]+)?)?$ ]]; then
        major=$((10#${BASH_REMATCH[1]}))
        minor=$((10#${BASH_REMATCH[2]}))
        patch=$((10#${BASH_REMATCH[3]}))
        if (( major > BEST_MAJOR ||
              (major == BEST_MAJOR && minor > BEST_MINOR) ||
              (major == BEST_MAJOR && minor == BEST_MINOR && patch > BEST_PATCH) )); then
            BEST_MAJOR=$major
            BEST_MINOR=$minor
            BEST_PATCH=$patch
            LATEST_TAG="$tag"
            VERSION="$major.$minor.$patch"
        fi
    fi
done <<< "$REMOTE_TAGS"

[[ -n "$VERSION" ]] || {
    echo "ERROR: no valid rust-v<major>.<minor>.<patch> Codex release tag found at $CODEX_REPO_URL" >&2
    exit 1
}

CURRENT_VERSION="$(awk -F'"' '/^const CODEX_MODELS_CLIENT_VERSION: &str = "[0-9]+\.[0-9]+\.[0-9]+";$/ { print $2 }' "$TARGET")"
[[ -n "$CURRENT_VERSION" ]] || {
    echo "ERROR: CODEX_MODELS_CLIENT_VERSION not found in $TARGET" >&2
    exit 1
}
CURRENT_GO_VERSION=""
if [[ -n "$GO_TARGET" ]]; then
    CURRENT_GO_VERSION="$(awk -F'"' '/^[[:space:]]*ClientVersion[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"$/ { print $2 }' "$GO_TARGET")"
    [[ -n "$CURRENT_GO_VERSION" ]] || {
        echo "ERROR: ClientVersion not found in $GO_TARGET" >&2
        exit 1
    }
fi

if [[ "$CURRENT_VERSION" == "$VERSION" && ( -z "$GO_TARGET" || "$CURRENT_GO_VERSION" == "$VERSION" ) ]]; then
    echo "Codex models client version is current: $VERSION ($LATEST_TAG)"
    exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
    echo "ERROR: Codex models client version is stale: Rust=$CURRENT_VERSION Go=${CURRENT_GO_VERSION:-not-checked} (expected $VERSION from $LATEST_TAG)" >&2
    exit 1
fi

TEMP_DIR="$PROJECT_ROOT/tmp/codex-model-version-sync"
mkdir -p "$TEMP_DIR"
TEMP_TARGET="$TEMP_DIR/rust.$$.tmp"
[[ ! -e "$TEMP_TARGET" ]] || {
    echo "ERROR: temporary file already exists: $TEMP_TARGET" >&2
    exit 1
}
awk -v version="$VERSION" '
    /^const CODEX_MODELS_CLIENT_VERSION: &str = "[0-9]+\.[0-9]+\.[0-9]+";$/ {
        print "const CODEX_MODELS_CLIENT_VERSION: &str = \"" version "\";"
        next
    }
    { print }
' "$TARGET" > "$TEMP_TARGET"
mv "$TEMP_TARGET" "$TARGET"

if [[ -n "$GO_TARGET" ]]; then
    GO_TEMP_TARGET="$TEMP_DIR/go.$$.tmp"
    [[ ! -e "$GO_TEMP_TARGET" ]] || {
        echo "ERROR: temporary file already exists: $GO_TEMP_TARGET" >&2
        exit 1
    }
    awk -v version="$VERSION" '
        /^[[:space:]]*ClientVersion[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"$/ {
            sub(/"[0-9]+\.[0-9]+\.[0-9]+"/, "\"" version "\"")
        }
        { print }
    ' "$GO_TARGET" > "$GO_TEMP_TARGET"
    mv "$GO_TEMP_TARGET" "$GO_TARGET"
fi

echo "Updated Codex models client version: Rust $CURRENT_VERSION, Go $CURRENT_GO_VERSION -> $VERSION ($LATEST_TAG)"
