//! `wortschatz-saver`: the Wortschatz screen saver for Linux.
//!
//! The Rust engine (`engine/`) picks and formats each card; the shared web renderer (`core/`,
//! embedded in this binary) draws it in a WebKitGTK view on every monitor. `idle` waits for the
//! session to go idle and starts the saver. See linux/README.md for the design and AGENTS.md for
//! building and installing.

#[cfg(target_os = "linux")]
mod idle;
#[cfg(target_os = "linux")]
mod saver;

#[cfg(target_os = "linux")]
const USAGE: &str = "\
usage: wortschatz-saver [run]    show the screen saver on every monitor; any input ends it
       wortschatz-saver window   show it in an ordinary window (for testing; input does not end it)
       wortschatz-saver idle [--timeout SECONDS] [--skip-when COMMAND]
                                 stay running; when the session has been idle for SECONDS
                                 (default 300), start the saver, unless COMMAND (run with sh -c)
                                 succeeds, e.g. a check that the screen is locked. Needs a Wayland
                                 compositor with ext-idle-notify-v1 (Hyprland, Sway, KDE).
       wortschatz-saver --version

environment:
  WZ_IDLE_TIMEOUT, WZ_SKIP_WHEN   defaults for idle --timeout / --skip-when
  XDG_STATE_HOME                  deck position: $XDG_STATE_HOME/wortschatz/deck.json
";

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("wortschatz-saver runs on Linux only. On macOS, build the .saver: scripts/install-macos.sh");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("run") => saver::run(saver::Mode::Saver),
        Some("window") => saver::run(saver::Mode::Window),
        Some("idle") => idle::run(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            0
        }
        Some("-V" | "--version") => {
            println!("wortschatz-saver {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
