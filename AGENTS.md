# Wortschatz: agent guide

A German-vocabulary screen saver. One Rust engine (`engine/`) picks and formats cards for two
shells: a native macOS `.saver` (`macos/`, Swift + CoreText) and a Linux saver
(`linux/`, GTK 3 + WebKitGTK showing the web renderer in `core/`). `README.md` is the user-facing
overview; this file is how to build, install and change it.

## Install on the user's machine

Pick the branch for the host OS. Done means the branch's **verify** step passes, not just a
finished build.

### Linux (Omarchy / Hyprland today; Sway and KDE should work; Ubuntu GNOME partly)

1. System packages: see **Linux system packages** below. Installing them is the only step that
   needs root. Check first with the `pkg-config` line there, and install only what is missing.
2. `scripts/install-linux.sh` (no sudo). It builds in `~/.cache/wortschatz-build`, installs
   `~/.local/bin/wortschatz-saver` and a launcher entry, and enables the systemd user unit
   `wortschatz-idle.service`. On Omarchy it also turns off Omarchy's own screensaver toggle so
   the two don't both start. `--help` lists the options (`--no-idle`, `--uninstall`, ...).
3. Verify:
   - `systemctl --user is-active wortschatz-idle` prints `active`. The exception is GNOME, where
     the installer skips the watcher and says so; see `linux/README.md` → GNOME.
   - `wortschatz-saver` shows a card full screen within a few seconds, and a key press ends it.
     Ask the user to confirm, or check `hyprctl clients` for class `wortschatz` on Hyprland.

Idle timeout and the lock-screen check live in `~/.config/wortschatz/idle.env`. After editing
it, run `systemctl --user restart wortschatz-idle`.

#### Linux system packages

These are the only root-level changes. Everything else installs per user.

| Purpose | Arch / Omarchy (`pacman -S --needed`) | Ubuntu 22.04+ / Debian (`apt install`) | pkg-config name | Required |
|---|---|---|---|---|
| C toolchain, pkg-config | `base-devel pkgconf` | `build-essential pkg-config` | | yes |
| Web view | `webkit2gtk-4.1` | `libwebkit2gtk-4.1-dev` | `webkit2gtk-4.1` | yes |
| GTK 3 | `gtk3` | `libgtk-3-dev` | `gtk+-3.0` | yes |
| Overlay above the bar (wlr-layer-shell) | `gtk-layer-shell` | `libgtk-layer-shell-dev` | `gtk-layer-shell-0 >= 0.6` | optional |
| Rust 1.89+ | `rustup` (then `rustup default stable`) or `rust` | rustup from <https://rustup.rs> (the distro's rustc is too old on 22.04/24.04) | | yes |

- Check what is missing:
  `pkg-config --modversion webkit2gtk-4.1 gtk+-3.0 gtk-layer-shell-0; cargo --version`
- Without `gtk-layer-shell`, `install-linux.sh` builds with `--no-default-features`. The saver
  then uses full-screen windows. That works, but on Hyprland the bar can show for a moment and
  focus changes. After installing the package, re-run the installer to get the overlay.
- Only the build needs these development packages. At run time the binary needs the matching
  shared libraries: `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libgtk-layer-shell0` on Ubuntu. Keep
  that in mind for future release builds that ship only the binary.
- Root access: never guess a sudo password. Ask the user to run the command, either typed as
  `! sudo …` in the agent prompt or in a root shell they provide. Then confirm with the
  `pkg-config` line above.

### macOS (13+, Apple silicon; Intel with `rustup target add x86_64-apple-darwin`)

1. Needs the Xcode Command Line Tools (`xcode-select --install`) and Rust (rustup).
2. `scripts/install-macos.sh --select`: builds, ad-hoc signs, copies to
   `~/Library/Screen Savers`, restarts `legacyScreenSaver`, and opens System Settings.
3. Verify: the user picks **Wortschatz** under System Settings → Screen Saver → Other, and the
   preview shows a card. A stale build may stay loaded until the user logs out and back in.

## Checks before committing

Run all of these from the repo root. The work is done when every one passes. CI will run the
same commands.

```sh
cargo test --workspace                  # add --no-default-features if gtk-layer-shell is not installed
cargo clippy --workspace --all-targets  # same flag rule; no warnings
cargo run -q --release -p wortschatz-core --bin wortschatz -- validate core/data/lexicon.json
node tools/check-parity.mjs             # core/app.js formats every card exactly like the engine
```

Also check that the other platform still builds:
- On Linux: `rustup target add aarch64-apple-darwin && cargo check --workspace --all-targets --target aarch64-apple-darwin`.
- macOS changes in `macos/` can only be built on a Mac (`scripts/build-macos.sh`). If you
  changed Swift without one, say so.

## Keeping platforms in sync

The same behaviour lives in more than one place. Change every copy together:

- **Card formatting and shuffling:** `engine/src/lib.rs` is the source of truth. `core/app.js`
  mirrors it for the standalone browser page; `tools/check-parity.mjs` catches drift in
  formatting.
- **Card design:** `core/style.css` is the source of truth. `macos/Sources/CardRenderer.swift`
  re-implements it natively, because WebKit gets suspended inside macOS's `legacyScreenSaver`.
  Linux uses `core/` directly.
- **Timing:** `core/timing.js` only. The engine (`Timing::from_js`, used by Linux) and the macOS
  saver parse its last `WORTSCHATZ_TIMING = {` object as strict JSON.
- **Lexicon:** `core/data/lexicon.json` and `lexicon.js` are generated; never edit them by hand.
  To rebuild, see `data-build/README.md`. It takes several GB and a manual review.
- **Linux shell:** its dependencies are gated to `cfg(target_os = "linux")`, so the workspace
  builds on macOS. Keep new Linux-only crates under that `[target.…]` table, and desktop-specific
  behaviour (Hyprland, GNOME) behind run-time detection, so one binary serves both Omarchy and
  Ubuntu.

Design, desktop integration and known gaps for Linux: `linux/README.md`.
Data sources and licences: `DATA_SOURCES.md`.

## Conventions

- Builds go outside the source tree (`~/.cache/wortschatz-build`, or
  `~/Library/Caches/wortschatz-build` on macOS; override with `WZ_BUILD_DIR`), because
  cloud-synced folders corrupt Rust build artefacts.
- Card content comes only from the sources in `DATA_SOURCES.md`. Nothing is machine-translated or
  invented; a wrong entry goes into `data-build/exclude.tsv`, and its fields are never rewritten.
- The bundled fonts are subsets, which makes them OFL "modified versions". Their internal names
  must not use a Reserved Font Name (the IPA font is renamed "Wz IPA" for this reason). Check
  `core/fonts/OFL-*.txt` before adding or re-subsetting a font.
