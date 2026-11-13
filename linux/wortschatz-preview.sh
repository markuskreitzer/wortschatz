#!/usr/bin/env bash
# STUB — not a screensaver. Opens the shared core fullscreen in a kiosk browser window so the
# renderer can be checked on Linux before the real Rust/wry shell exists (see linux/README.md).
set -euo pipefail
CORE="$(cd "$(dirname "$0")/../core" && pwd)/index.html"
for b in chromium google-chrome-stable google-chrome brave; do
  if command -v "$b" >/dev/null; then
    exec "$b" --kiosk --app="file://$CORE" --allow-file-access-from-files --class=wortschatz
  fi
done
echo "No Chromium-based browser found; open file://$CORE in any browser and press F11." >&2
exit 1
