#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLED="$ROOT/target/bundled"

declare -a binaries=(
  "$BUNDLED/CC-22.app/Contents/MacOS/CC-22"
  "$BUNDLED/CC-22.vst3/Contents/MacOS/CC-22"
  "$BUNDLED/CC-22.clap/Contents/MacOS/CC-22"
)

for binary in "${binaries[@]}"; do
  test -x "$binary"
  archs="$(lipo -archs "$binary")"
  [[ " $archs " == *" x86_64 "* ]] || { echo "Missing x86_64: $binary" >&2; exit 1; }
  [[ " $archs " == *" arm64 "* ]] || { echo "Missing arm64: $binary" >&2; exit 1; }
  file "$binary"
  otool -L "$binary"
done

for bundle in "$BUNDLED/CC-22.app" "$BUNDLED/CC-22.vst3" "$BUNDLED/CC-22.clap"; do
  plutil -lint "$bundle/Contents/Info.plist"
  codesign --verify --deep --strict --verbose=2 "$bundle"
done

# Exercise argument parsing and executable startup without requiring an audio device.
"${binaries[0]}" --help >/dev/null

echo "All macOS bundle checks passed."
