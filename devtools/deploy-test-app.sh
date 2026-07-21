#!/usr/bin/env bash

# Build the current CLI, stage it as a disposable macOS .app under temp/, sign
# it ad hoc, and launch a fresh product-validation instance. Generated files
# stay under temp/; this script does not modify a system-installed application.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="debug"
open_inspector=false

usage() {
  echo "Usage: devtools/deploy-test-app.sh [--release] [--inspector]"
}

while (($# > 0)); do
  case "$1" in
    --release)
      profile="release"
      ;;
    --inspector)
      open_inspector=true
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

build_args=(build --offline -p rozsa-cli)
if [[ "$profile" == "release" ]]; then
  build_args+=(--release)
fi

cd "$repo_root"
cargo "${build_args[@]}"

app_path="$repo_root/temp/rozsa-product-validation/RozsaProduct.app"
contents_path="$app_path/Contents"
executable_path="$repo_root/target/$profile/rozsa"
version="$(awk '$0 == "[workspace.package]" { active = 1; next } active && /^\[/ { exit } active && /^version = / { gsub(/"/, "", $3); print $3; exit }' "$repo_root/Cargo.toml")"

mkdir -p "$contents_path/MacOS"
install -m 755 "$executable_path" "$contents_path/MacOS/rozsa"
cat >"$contents_path/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Rózsa Product Validation</string>
  <key>CFBundleExecutable</key>
  <string>rozsa</string>
  <key>CFBundleIdentifier</key>
  <string>dev.rozsa.app</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Rózsa Product Validation</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$app_path"

open_args=(-n -F)
if [[ "$open_inspector" == true ]]; then
  open_args+=(--env ROZSA_WEB_INSPECTOR=1)
fi
/usr/bin/open "${open_args[@]}" "$app_path"

echo "Launched $app_path"
if [[ "$open_inspector" == false ]]; then
  echo "Web Inspector disabled; pass --inspector to enable it."
fi
