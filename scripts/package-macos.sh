#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLED="$ROOT/target/bundled"
APP="$BUNDLED/CC-22.app"
VST3="$BUNDLED/CC-22.vst3"
CLAP="$BUNDLED/CC-22.clap"
ICONSET="$ROOT/target/cc22.iconset"
ICNS="$ROOT/target/cc22.icns"
DIST="$ROOT/target/macos-dist"

for bundle in "$APP" "$VST3" "$CLAP"; do
  if [[ ! -d "$bundle" ]]; then
    echo "Missing macOS bundle: $bundle" >&2
    exit 1
  fi
done

rm -rf "$ICONSET" "$DIST"
mkdir -p "$ICONSET" "$DIST"

for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/assets/cc22-logo.png" \
    --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  retina=$((size * 2))
  sips -z "$retina" "$retina" "$ROOT/assets/cc22-logo.png" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$ICNS"
for bundle in "$APP" "$VST3" "$CLAP"; do
  mkdir -p "$bundle/Contents/Resources"
  cp "$ICNS" "$bundle/Contents/Resources/cc22.icns"
done

PLISTBUDDY=/usr/libexec/PlistBuddy
set_plist() {
  local plist="$1" key="$2" type="$3" value="$4"
  "$PLISTBUDDY" -c "Delete :$key" "$plist" >/dev/null 2>&1 || true
  "$PLISTBUDDY" -c "Add :$key $type $value" "$plist"
}

set_plist "$APP/Contents/Info.plist" CFBundleIdentifier string com.rafaaudio.cc22.standalone
set_plist "$APP/Contents/Info.plist" CFBundleIconFile string cc22.icns
set_plist "$APP/Contents/Info.plist" NSMicrophoneUsageDescription string "CC-22 needs audio input access for standalone processing."
set_plist "$VST3/Contents/Info.plist" CFBundleIdentifier string com.rafaaudio.cc22.vst3
set_plist "$VST3/Contents/Info.plist" CFBundleIconFile string cc22.icns
set_plist "$CLAP/Contents/Info.plist" CFBundleIdentifier string com.rafaaudio.cc22.clap
set_plist "$CLAP/Contents/Info.plist" CFBundleIconFile string cc22.icns

for bundle in "$APP" "$VST3" "$CLAP"; do
  plutil -lint "$bundle/Contents/Info.plist"
  codesign --force --deep --sign - "$bundle"
  codesign --verify --deep --strict --verbose=2 "$bundle"
done

cp -R "$APP" "$VST3" "$CLAP" "$DIST/"
cp "$ROOT/docs/MACOS_TESTING.md" "$DIST/LEIA-ME-MACOS.md"

# ditto preserves executable bits, resource forks, and macOS bundle metadata.
ditto -c -k --sequesterRsrc --keepParent "$DIST" "$ROOT/target/CC-22-1.0.0-macOS-universal.zip"
echo "Created target/CC-22-1.0.0-macOS-universal.zip"
