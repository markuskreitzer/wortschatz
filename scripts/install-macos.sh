#!/usr/bin/env bash
# Build and install Wortschatz.saver into ~/Library/Screen Savers (current user only).
#   scripts/install-macos.sh            build + install
#   scripts/install-macos.sh --select   ...and open System Settings > Screen Saver to pick it
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/build-macos.sh"

DEST="$HOME/Library/Screen Savers"
mkdir -p "$DEST"
rm -rf "$DEST/Wortschatz.saver"
cp -R "${WZ_BUILD_DIR:-$HOME/Library/Caches/wortschatz-build}/Wortschatz.saver" "$DEST/"
# Locally built, never downloaded: make sure no quarantine flag sneaks in.
xattr -dr com.apple.quarantine "$DEST/Wortschatz.saver" 2>/dev/null || true
echo "Installed $DEST/Wortschatz.saver"

# The screen saver host caches loaded bundles; restart it so the new build is picked up.
killall legacyScreenSaver 2>/dev/null || true
killall "legacyScreenSaver (Wallpaper)" 2>/dev/null || true

if [[ "${1:-}" == "--select" ]]; then
  "$ROOT/scripts/select-macos.sh"
else
  echo "Select it in System Settings > Screen Saver > Other > Wortschatz (or rerun with --select to open that pane)."
fi
