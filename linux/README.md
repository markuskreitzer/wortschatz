# Wortschatz on Linux

`wortschatz-saver` is one binary with two jobs:

- **`wortschatz-saver`**: the saver. It shows `core/index.html` (embedded in the binary,
  served from `wortschatz://localhost/`) in a WebKitGTK view on every monitor. The page runs
  host-driven: this process picks each card with `wortschatz-core` (the same engine the macOS
  saver links) and calls `Wortschatz.show(card)` on the schedule in `core/timing.js`. The deck
  position persists in `$XDG_STATE_HOME/wortschatz/deck.json`. Any key, click, scroll or real
  mouse movement ends it, and a second copy started while one runs exits at once.
- **`wortschatz-saver idle`**: the idle watcher, run by `wortschatz-idle.service` (a systemd user
  unit). It starts the saver when the session goes idle and stops it on activity.

`wortschatz-saver window` shows the saver in an ordinary window that input does not close, for
working on the renderer. To install, see `AGENTS.md`; the installer is `scripts/install-linux.sh`.

## Surfaces

| Compositor supports | What the saver draws |
|---|---|
| wlr-layer-shell (Hyprland, Sway, KDE, niri) and built with the `layer-shell` feature | an *overlay* layer surface per monitor: above the bar and every window, with exclusive keyboard focus, no window rules needed |
| otherwise (GNOME, X11, or built without the feature) | an undecorated full-screen window per monitor (`fullscreen_on_monitor`), app id / class `wortschatz` |

The choice is made at run time (`gtk_layer_shell::is_supported()`), so the same binary works on
both Omarchy and Ubuntu.

## Idle integration

### Omarchy (Hyprland), current releases

Omarchy has no hypridle any more. The **Omarchy shell's idle service**
(`/usr/share/omarchy/shell/plugins/services/idle/Service.qml`) does three things:

- runs `omarchy-launch-screensaver` after `idle.screensaver` seconds;
- locks after `idle.lock` seconds (both are set in `~/.config/omarchy/shell.json`);
- skips the screensaver when the flag file `~/.local/state/omarchy/toggles/screensaver-off`
  exists.

The launch command can't be replaced: `/usr/share/omarchy/bin` comes first in `PATH`. So the
installer:

1. turns Omarchy's screensaver off through that toggle (it records that it did, and
   `--uninstall` restores it);
2. runs its own watcher with the same timeout (`WZ_IDLE_TIMEOUT` in
   `~/.config/wortschatz/idle.env`, copied from `shell.json` on first install);
3. skips launching while the Omarchy lock screen is up
   (`WZ_SKIP_WHEN='[ "$(omarchy-shell lock isLocked)" = true ]'`).

The lock screen (ext-session-lock) still appears on schedule, above the saver.

The watcher uses **ext-idle-notify-v1**, the same protocol Omarchy's shell uses, so idle
inhibitors (a playing video, Omarchy's "stay awake") hold off both of them.

**Hyprland quirk:** Hyprland sends `resumed` about 0.25 s after the saver's surface appears
(the focus change counts as activity). The watcher ignores `resumed` for 3 s after each launch;
otherwise it would kill the saver at once. The saver still quits on input by itself.

Hyprland's config is Lua now (`~/.config/hypr/*.lua`, `o.window(...)` rules). The overlay layer
needs no rules. If you run the full-screen fallback on Hyprland and want rules, match class
`wortschatz`.

### Other wlroots / KDE sessions

The watcher works as is: the protocol is the same. Without a systemd user session, start
`wortschatz-saver idle --timeout 300` from the session's autostart.

### GNOME (Ubuntu's default session): next step

Status today: the **saver** builds and runs on Ubuntu 22.04+ and falls back to full-screen
windows (GNOME has no layer-shell). The **watcher** does not run on GNOME: Mutter does not
offer ext-idle-notify-v1. `idle` exits with status 3, which the unit does not restart on, and
the installer skips the unit on GNOME.

Plan:

1. Add a GNOME backend to `idle.rs`. Watch idle time over D-Bus with
   `org.gnome.Mutter.IdleMonitor` (`/org/gnome/Mutter/IdleMonitor/Core`, `AddIdleWatch(ms)` and
   `AddUserActiveWatch`, signal `WatchFired`), using `zbus`. Pick the backend at run time:
   ext-idle-notify-v1 if present, otherwise the Mutter D-Bus service.
2. Skip launching while locked: `org.gnome.ScreenSaver.GetActive` on the session bus.
3. Remind users to keep GNOME's *Settings → Power → Screen Blank* delay longer than
   `WZ_IDLE_TIMEOUT`, or the screen blanks before the saver appears.
4. Then remove the GNOME early exit from `scripts/install-linux.sh`.

Until then, Ubuntu users can bind `wortschatz-saver` to a key or start it from the launcher
entry.

## Known issues

- **NVIDIA:** WebKitGTK on the NVIDIA driver under Wayland sometimes shows a blank window. If
  that happens, try `WEBKIT_DISABLE_DMABUF_RENDERER=1 wortschatz-saver`. If that fixes it, add
  `Environment=WEBKIT_DISABLE_DMABUF_RENDERER=1` to the unit, or put the variable in `idle.env`.
  Not observed on the Intel + NVIDIA hybrid laptop this was developed on, where Hyprland renders
  on the Intel GPU.
- **Monitor hot-plug:** monitors are counted once, when the saver starts.
