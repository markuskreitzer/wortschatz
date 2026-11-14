# Linux / Omarchy port (not implemented yet — plan + stub)

Everything that matters is already portable:

- `core/` — the renderer (HTML/CSS/JS, bundled fonts, `data/lexicon.js`). No platform APIs.
- `engine/` — the Rust crate (`wortschatz-core`): lexicon model, validation, fair shuffle with
  persisted progress, card formatting. Usable as an `rlib` from a Rust runner, or through the C ABI
  in `engine/wortschatz.h`.

On macOS the saver draws cards natively (WebKit gets suspended inside `legacyScreenSaver`), but on
Linux a fullscreen WebView window is an ordinary foreground app, so the web renderer in `core/` is
the natural fit. Watch for the same failure mode anyway: if the compositor reports the window as
hidden or occluded, WebKitGTK may throttle it. A Linux shell needs:

## Plan for Omarchy (Hyprland + hypridle)

Omarchy has no XScreenSaver-style module API; "screensaver" = a fullscreen window launched on idle
and closed on input.

1. **`linux/wortschatz-linux` (Rust binary, to write):** a `tao` window + `wry` WebView
   (WebKitGTK 4.1 under the hood) that
   - loads `core/index.html` through a custom protocol (`wortschatz://`) serving the bundled
     files (avoids file-URL restrictions),
   - injects `window.WORTSCHATZ_HOST = { platform: "linux", driven: true, hold: 32 }`,
   - links `wortschatz-core` directly and calls `Wortschatz.show(<card json>)` every hold period,
     state in `$XDG_STATE_HOME/wortschatz/deck.json`,
   - goes fullscreen on every output (one window per monitor), hides the cursor, and exits on
     any key / mouse movement.
   Alternative to a normal fullscreen window: a layer-shell surface on the `overlay` layer
   (`gtk-layer-shell`) so it sits above everything like a real screensaver.
2. **Window rules** (`~/.config/hypr/hyprland.conf`):
   `windowrulev2 = fullscreen, class:^(wortschatz)$` and `windowrulev2 = noanim, class:^(wortschatz)$`.
3. **hypridle** (`~/.config/hypr/hypridle.conf`) — run it before the lock:
   ```
   listener {
       timeout = 300
       on-timeout = wortschatz-linux
       on-resume  = pkill -x wortschatz-linux
   }
   ```
   Omarchy ships its own screensaver listener (`omarchy-launch-screensaver`); replacing the command
   it launches is the least invasive hook.

Packages on Arch: `webkit2gtk-4.1`, `gtk3`, `gtk-layer-shell` (if layer-shell), Rust toolchain.

## Stub available now

`linux/wortschatz-preview.sh` opens the shared core fullscreen in a kiosk browser window. It is a
preview/test aid only (the browser drives the shuffle itself; no idle hook, no multi-monitor).
