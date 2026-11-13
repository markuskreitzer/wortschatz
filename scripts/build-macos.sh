#!/usr/bin/env bash
# Build Wortschatz.saver (Rust engine + Swift ScreenSaverView + shared core), ad-hoc signed.
# Needs: Xcode or Command Line Tools (swiftc, codesign), Rust (cargo).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Build outside the source tree (default ~/Library/Caches/wortschatz-build, override with
# WZ_BUILD_DIR): cloud-synced folders (iCloud Drive, Dropbox, File Provider sync clients) can
# corrupt or thrash fast-changing build artefacts such as Rust proc-macro dylibs.
BUILD="${WZ_BUILD_DIR:-$HOME/Library/Caches/wortschatz-build}"
export CARGO_TARGET_DIR="$BUILD/cargo-target"
SAVER="$BUILD/Wortschatz.saver"
MIN_MACOS=13.0

command -v swiftc >/dev/null || { echo "swiftc not found: install Xcode or run 'xcode-select --install'" >&2; exit 1; }
command -v cargo  >/dev/null || { echo "cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }

ARCHS=(arm64)
if rustup target list --installed 2>/dev/null | grep -q '^x86_64-apple-darwin$'; then ARCHS+=(x86_64); fi

rm -rf "$SAVER"
mkdir -p "$SAVER/Contents/MacOS" "$SAVER/Contents/Resources" "$BUILD/obj"

BINS=()
for arch in "${ARCHS[@]}"; do
  case "$arch" in arm64) triple=aarch64-apple-darwin;; x86_64) triple=x86_64-apple-darwin;; esac
  echo "==> Rust engine ($triple)"
  MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS cargo build --release --quiet \
    --manifest-path "$ROOT/engine/Cargo.toml" --lib --target "$triple"
  LIB="$CARGO_TARGET_DIR/$triple/release/libwortschatz_core.a"
  echo "==> Swift shell ($arch)"
  swiftc -O -parse-as-library -module-name Wortschatz \
    -target "$arch-apple-macos$MIN_MACOS" \
    -import-objc-header "$ROOT/macos/Sources/Bridging.h" \
    "$ROOT"/macos/Sources/*.swift "$LIB" \
    -framework ScreenSaver -framework WebKit -framework AppKit \
    -Xlinker -bundle -o "$BUILD/obj/Wortschatz-$arch"
  BINS+=("$BUILD/obj/Wortschatz-$arch")
done
lipo -create "${BINS[@]}" -output "$SAVER/Contents/MacOS/Wortschatz"

echo "==> Resources"
cp "$ROOT/macos/Resources/Info.plist" "$SAVER/Contents/Info.plist"
mkdir -p "$SAVER/Contents/Resources/core/data" "$SAVER/Contents/Resources/core/fonts"
cp "$ROOT"/core/index.html "$ROOT"/core/style.css "$ROOT"/core/app.js "$SAVER/Contents/Resources/core/"
cp "$ROOT"/core/data/lexicon.json "$ROOT"/core/data/lexicon.js "$SAVER/Contents/Resources/core/data/"
cp "$ROOT"/core/fonts/*.woff2 "$ROOT"/core/fonts/OFL-*.txt "$SAVER/Contents/Resources/core/fonts/"
cp "$ROOT/DATA_SOURCES.md" "$ROOT/LICENSE" "$SAVER/Contents/Resources/"
[ -f "$ROOT/macos/Resources/thumbnail.png" ] && cp "$ROOT/macos/Resources/thumbnail.png" "$ROOT/macos/Resources/thumbnail@2x.png" "$SAVER/Contents/Resources/" 2>/dev/null || true

echo "==> Ad-hoc signing"
codesign --force --deep --sign - --timestamp=none "$SAVER"
codesign --verify --strict --verbose=2 "$SAVER"
file "$SAVER/Contents/MacOS/Wortschatz"
echo "Built $SAVER"
