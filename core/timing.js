/* Wortschatz card timing: the one source for both renderers.
 *
 * core/app.js reads window.WORTSCHATZ_TIMING. The macOS saver (macos/Sources/WortschatzView.swift)
 * reads this same file from its bundle and parses the object literal below as JSON, so keep it
 * strict JSON (double-quoted keys, numbers only, no comments or trailing commas inside the braces).
 *
 * Timeline of one card, from the moment it starts to fade in (t = 0):
 *   0 .................. fade in (opacity, fade_in_ms) and drift into place (drift_in_ms)
 *   0 .. progress_ms ... the colophon hairline fills
 *   cycle - exit_lead .. fade out (fade_out_ms) and drift down          (32.39 s)
 *   cycle_ms ........... the next card starts to fade in                (34.60 s)
 */
window.WORTSCHATZ_TIMING = {
  "fade_in_ms": 2600,
  "drift_in_ms": 4160,
  "progress_ms": 32000,
  "cycle_ms": 34600,
  "exit_lead_ms": 2210,
  "fade_out_ms": 2080
};
