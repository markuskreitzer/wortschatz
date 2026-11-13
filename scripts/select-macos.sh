#!/usr/bin/env bash
# Open the Screen Saver settings so Wortschatz can be picked.
#
# Since macOS 14 the screen saver choice lives in the wallpaper store
# (~/Library/Application Support/com.apple.wallpaper/Store/Index.plist, per display and per
# Space, with opaque encoded blobs). The old `defaults -currentHost ... moduleDict` key is no
# longer honoured, and editing the store by hand risks resetting your wallpapers. So this script
# does not change the selection itself. It opens the right pane and you click Wortschatz.
set -euo pipefail
SAVER="$HOME/Library/Screen Savers/Wortschatz.saver"
[ -d "$SAVER" ] || { echo "not installed: $SAVER (run scripts/install-macos.sh)" >&2; exit 1; }
echo "System Settings > Screen Saver > (scroll to 'Other') > Wortschatz"
open "x-apple.systempreferences:com.apple.ScreenSaver-Settings.extension" 2>/dev/null \
  || open -b com.apple.systempreferences
