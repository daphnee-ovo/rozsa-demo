#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  "")
    launch_app=true
    ;;
  --prepare-only)
    launch_app=false
    ;;
  *)
    echo "usage: ./run.sh [--prepare-only]" >&2
    exit 64
    ;;
esac

project_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
target_dir="${CARGO_TARGET_DIR:-$project_dir/target}"
app_bundle="$target_dir/debug/Rózsa.app"
contents_dir="$app_bundle/Contents"

cd "$project_dir"
cargo build --package rozsa-cli --bin rozsa

mkdir -p "$contents_dir/MacOS" "$contents_dir/Resources"
cp "$target_dir/debug/rozsa" "$contents_dir/MacOS/rozsa"
cp "$project_dir/crates/rozsa-gui/icons/icon.icns" "$contents_dir/Resources/icon.icns"

cat >"$contents_dir/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Rózsa</string>
  <key>CFBundleExecutable</key>
  <string>rozsa</string>
  <key>CFBundleIconFile</key>
  <string>icon.icns</string>
  <key>CFBundleIdentifier</key>
  <string>dev.rozsa.app</string>
  <key>CFBundleName</key>
  <string>Rózsa</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

plutil -lint "$contents_dir/Info.plist" >/dev/null

if "$launch_app"; then
  pkill -x rozsa 2>/dev/null || true
  open -n "$app_bundle"
fi
