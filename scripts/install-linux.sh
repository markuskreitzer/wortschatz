#!/usr/bin/env bash
# Build and install the Wortschatz screen saver for the current Linux user (no sudo).
#
#   scripts/install-linux.sh              build, install, start the idle watcher
#   scripts/install-linux.sh --no-idle    install the binary and launcher entry only
#   scripts/install-linux.sh --keep-omarchy-screensaver
#                                         on Omarchy, leave Omarchy's own screensaver enabled
#   scripts/install-linux.sh --uninstall  remove everything this script installed
#
# Installs:
#   ~/.local/bin/wortschatz-saver                         (override the dir with WZ_BIN_DIR)
#   ~/.local/share/applications/wortschatz-saver.desktop  launcher entry ("Wortschatz")
#   ~/.config/systemd/user/wortschatz-idle.service        idle watcher (Wayland ext-idle-notify)
#   ~/.config/wortschatz/idle.env                         idle settings; kept across reinstalls
# Builds in ${WZ_BUILD_DIR:-${XDG_CACHE_HOME:-~/.cache}/wortschatz-build}, outside the source tree.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${WZ_BIN_DIR:-$HOME/.local/bin}"
BIN="$BIN_DIR/wortschatz-saver"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
CONF="${XDG_CONFIG_HOME:-$HOME/.config}"
UNIT_DIR="$CONF/systemd/user"
UNIT=wortschatz-idle.service
ENV_FILE="$CONF/wortschatz/idle.env"
OMARCHY_TOGGLE="$HOME/.local/state/omarchy/toggles/screensaver-off"
OMARCHY_MARK="$CONF/wortschatz/omarchy-screensaver-disabled-by-wortschatz"

IDLE=1 KEEP_OMARCHY=0 UNINSTALL=0
for a in "$@"; do
  case "$a" in
    --no-idle) IDLE=0 ;;
    --keep-omarchy-screensaver) KEEP_OMARCHY=1 ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

is_omarchy() { command -v omarchy-launch-screensaver >/dev/null 2>&1; }
have_user_systemd() { systemctl --user show-environment >/dev/null 2>&1; }

if [ "$UNINSTALL" = 1 ]; then
  if have_user_systemd; then
    systemctl --user disable --now "$UNIT" 2>/dev/null || true
  fi
  rm -f "$UNIT_DIR/$UNIT" "$APPS/wortschatz-saver.desktop" "$BIN"
  have_user_systemd && systemctl --user daemon-reload || true
  if [ -f "$OMARCHY_MARK" ]; then
    rm -f "$OMARCHY_TOGGLE" "$OMARCHY_MARK"
    echo "Re-enabled Omarchy's own screensaver."
  fi
  echo "Uninstalled. Kept $ENV_FILE and the deck position in ${XDG_STATE_HOME:-$HOME/.local/state}/wortschatz/."
  exit 0
fi

# --- dependencies -----------------------------------------------------------------------------
. /etc/os-release 2>/dev/null || true
hint() {
  case " ${ID:-} ${ID_LIKE:-} " in
    *" arch "*)   echo "  sudo pacman -S --needed $1" ;;
    *" ubuntu "*|*" debian "*) echo "  sudo apt install $2" ;;
    *) echo "  (install the development packages for: $3)" ;;
  esac
}
command -v cargo >/dev/null || { echo "cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }
command -v pkg-config >/dev/null || { echo "pkg-config not found:" >&2; hint "pkgconf" "pkg-config" "pkg-config" >&2; exit 1; }
if ! pkg-config --exists webkit2gtk-4.1 gtk+-3.0; then
  echo "WebKitGTK 4.1 / GTK 3 development files are missing. Install them with:" >&2
  hint "webkit2gtk-4.1 gtk3" "build-essential libwebkit2gtk-4.1-dev libgtk-3-dev" "webkit2gtk-4.1, gtk+-3.0" >&2
  exit 1
fi
FEATURES=()
if ! pkg-config --exists 'gtk-layer-shell-0 >= 0.6'; then
  echo "note: gtk-layer-shell (GTK 3, 0.6 or newer) not found; building without it. The saver then uses a"
  echo "      full-screen window per monitor instead of an overlay above the bar. To get the overlay:"
  hint "gtk-layer-shell" "libgtk-layer-shell-dev" "gtk-layer-shell-0"
  FEATURES=(--no-default-features)
fi

# --- build + install ----------------------------------------------------------------------------
BUILD="${WZ_BUILD_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/wortschatz-build}"
export CARGO_TARGET_DIR="$BUILD/cargo-target"
echo "==> Building wortschatz-saver (${FEATURES[*]:-with layer-shell})"
cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p wortschatz-saver "${FEATURES[@]}"
install -Dm755 "$CARGO_TARGET_DIR/release/wortschatz-saver" "$BIN"
echo "==> Installed $BIN"
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) echo "note: $BIN_DIR is not on PATH" ;; esac

mkdir -p "$APPS"
sed "s|@BIN@|$BIN|g" "$ROOT/linux/dist/wortschatz-saver.desktop" > "$APPS/wortschatz-saver.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" 2>/dev/null || true

[ "$IDLE" = 1 ] || { echo "Done (no idle watcher). Run: wortschatz-saver"; exit 0; }

# --- idle watcher -------------------------------------------------------------------------------
case "${XDG_CURRENT_DESKTOP:-}" in
  *GNOME*|*Unity*)
    echo "GNOME has no ext-idle-notify-v1, so the idle watcher cannot run here yet (see linux/README.md)."
    echo "The saver itself works: run wortschatz-saver, or bind it to a key."
    exit 0 ;;
esac
have_user_systemd || { echo "No systemd user session; start '$BIN idle' from your session's autostart instead."; exit 0; }

if [ ! -f "$ENV_FILE" ]; then
  mkdir -p "$(dirname "$ENV_FILE")"
  timeout=300
  if is_omarchy && [ -f "$CONF/omarchy/shell.json" ] && command -v jq >/dev/null; then
    timeout=$(jq -r '.idle.screensaver // 300' "$CONF/omarchy/shell.json")
  fi
  {
    echo "# Wortschatz idle watcher settings (read by $UNIT). Restart after editing:"
    echo "#   systemctl --user restart $UNIT"
    echo "WZ_IDLE_TIMEOUT=$timeout"
    if is_omarchy; then
      echo "# Do not start while the Omarchy lock screen is up."
      echo "WZ_SKIP_WHEN='[ \"\$(omarchy-shell lock isLocked 2>/dev/null)\" = true ]'"
    fi
  } > "$ENV_FILE"
  echo "==> Wrote $ENV_FILE (idle timeout ${timeout}s)"
fi

mkdir -p "$UNIT_DIR"
sed -e "s|@BIN@|$BIN|g" -e "s|@DOC@|$ROOT/linux/README.md|g" "$ROOT/linux/dist/$UNIT" > "$UNIT_DIR/$UNIT"
systemctl --user daemon-reload
systemctl --user enable "$UNIT" >/dev/null 2>&1
systemctl --user restart "$UNIT"
echo "==> Enabled $UNIT"

if is_omarchy && [ "$KEEP_OMARCHY" = 0 ] && [ ! -f "$OMARCHY_TOGGLE" ]; then
  mkdir -p "$(dirname "$OMARCHY_TOGGLE")"
  touch "$OMARCHY_TOGGLE" "$OMARCHY_MARK"
  echo "==> Turned off Omarchy's own screensaver (it would start on the same idle timeout)."
  echo "    --uninstall turns it back on."
fi

sleep 1
if systemctl --user is-active --quiet "$UNIT"; then
  echo "Done. The saver starts after $(sed -n 's/^WZ_IDLE_TIMEOUT=//p' "$ENV_FILE") s idle; run 'wortschatz-saver' to see it now."
else
  echo "The idle watcher did not stay up:" >&2
  journalctl --user -u "$UNIT" -n 5 --no-pager >&2 || true
  exit 1
fi
