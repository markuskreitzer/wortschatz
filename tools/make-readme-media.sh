#!/usr/bin/env bash
# Regenerate docs/images/wortschatz.{gif,mp4} from the real renderer (core/), deterministically.
# Needs: node + `npm i puppeteer-core`, Chrome/Chromium (CHROME=...), ffmpeg.
# The still screenshots in docs/images/card-*.png come from tools/snapshot.swift on macOS, which
# loads the built .saver exactly like the screen saver host does.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRAMES="$(mktemp -d)"
node "$ROOT/tools/capture-animation.js" "$FRAMES/raw" 1440 900 25 1
# Rotate the loop so the first frame (the still shown before playback) is a fully visible card.
# The capture starts and ends on an empty background, so the loop stays seamless.
mkdir -p "$FRAMES/rot"; n=$(ls "$FRAMES/raw" | wc -l); k=110; i=0
for f in $(ls "$FRAMES/raw" | tail -n $((n - k))) $(ls "$FRAMES/raw" | head -n $k); do
  ln -s "$FRAMES/raw/$f" "$FRAMES/rot/$(printf 'f%04d.png' $i)"; i=$((i + 1)); done
ffmpeg -loglevel error -y -framerate 25 -i "$FRAMES/rot/f%04d.png" -c:v libx264 -preset slow -crf 20 \
  -pix_fmt yuv420p -movflags +faststart -an "$ROOT/docs/images/wortschatz.mp4"
ffmpeg -loglevel error -y -framerate 25 -i "$FRAMES/rot/f%04d.png" -vf \
  "fps=15,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=full[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" \
  -loop 0 "$ROOT/docs/images/wortschatz.gif"
rm -rf "$FRAMES"
ls -la "$ROOT/docs/images"
