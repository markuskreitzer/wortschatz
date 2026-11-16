//! The saver: one WebKitGTK view per monitor showing `core/index.html`, driven by the engine.
//!
//! - `core/` is embedded and served from `wortschatz://localhost/` (the page's CSP allows it).
//! - The page runs host-driven (`WORTSCHATZ_HOST.driven`): this process picks each card with
//!   `wortschatz_core::Engine` and calls `Wortschatz.show(card)` on every view, on the schedule in
//!   `core/timing.js`. The deck position persists in `$XDG_STATE_HOME/wortschatz/deck.json`.
//! - Where the compositor supports wlr-layer-shell (Hyprland, Sway, KDE), each view is an
//!   overlay layer surface above the bar and all windows. Elsewhere (GNOME) it falls back to a
//!   full-screen window per monitor.
//! - In saver mode any key, click, scroll or real mouse movement ends the process.
use std::borrow::Cow;
use std::cell::RefCell;
use std::fs::File;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use include_dir::{include_dir, Dir};
use wortschatz_core::{Engine, Lexicon, Timing};
use wry::http::{header::CONTENT_TYPE, Request, Response};
use wry::{BackgroundThrottlingPolicy, PageLoadEvent, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix};

static CORE: Dir = include_dir!("$CARGO_MANIFEST_DIR/../core");

/// Matches `--bg` in core/style.css, so nothing flashes white before the page paints.
const BG: (u8, u8, u8, u8) = (0x0d, 0x0c, 0x0b, 0xff);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Full screen on every monitor; input quits.
    Saver,
    /// One ordinary window, for development; input does not quit.
    Window,
}

/// Runs before the page's own scripts: host-driven mode, and (saver only) quit on input.
fn init_script(mode: Mode) -> String {
    let mut js = String::from("window.WORTSCHATZ_HOST = { driven: true, platform: \"linux\" };\n");
    if mode == Mode::Saver {
        // A pointer that merely enters the new surface reports a motion event, so movement only
        // counts once it has travelled a few pixels from where it was first seen.
        js.push_str(
            r#"(() => {
  const quit = () => window.ipc.postMessage("quit");
  let origin = null;
  for (const t of ["keydown", "pointerdown", "wheel", "touchstart"]) addEventListener(t, quit, true);
  addEventListener("pointermove", (e) => {
    if (!origin) { origin = [e.screenX, e.screenY]; return; }
    if (Math.hypot(e.screenX - origin[0], e.screenY - origin[1]) > 16) quit();
  }, true);
})();
"#,
        );
    }
    js
}

fn core_file(path: &str) -> Option<&'static [u8]> {
    CORE.get_file(path).map(|f| f.contents())
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "woff2" => "font/woff2",
        "png" => "image/png",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match core_file(path) {
        Some(body) => Response::builder().header(CONTENT_TYPE, mime(path)).body(Cow::Borrowed(body)),
        None => Response::builder().status(404).body(Cow::Borrowed(&b"not found"[..])),
    }
    .expect("static response")
}

fn state_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("wortschatz"))
}

/// One saver at a time: a second launch (an idle watcher firing again, a keybinding) exits.
/// `Err` means another instance holds the lock.
fn single_instance() -> Result<Option<File>, ()> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let Ok(file) = File::create(dir.join("wortschatz-saver.lock")) else { return Ok(None) };
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Err(()),
        Err(std::fs::TryLockError::Error(e)) => {
            eprintln!("wortschatz-saver: cannot lock ({e}); continuing");
            Ok(Some(file))
        }
    }
}

struct Shared {
    engine: Engine,
    /// The card currently on screen, as JSON (also sent to views that finish loading late).
    card: String,
    views: Vec<WebView>,
}

impl Shared {
    fn advance(&mut self) {
        self.card = serde_json::to_string(&self.engine.next_card()).expect("card serializes");
    }
    fn show(&self, view: &WebView) {
        let js = format!("window.Wortschatz && window.Wortschatz.show({})", self.card);
        if let Err(e) = view.evaluate_script(&js) {
            eprintln!("wortschatz-saver: show failed: {e}");
        }
    }
}

/// Sends the next card to every view after `ms`, then keeps doing so every `cycle_ms`.
fn schedule(shared: Rc<RefCell<Shared>>, ms: f64, cycle_ms: f64) {
    glib::timeout_add_local_once(Duration::from_millis(ms.max(1000.0) as u64), move || {
        {
            let mut s = shared.borrow_mut();
            s.advance();
            for v in &s.views {
                s.show(v);
            }
        }
        schedule(shared, cycle_ms, cycle_ms);
    });
}

#[cfg(feature = "layer-shell")]
fn layer_shell_supported() -> bool {
    gtk_layer_shell::is_supported()
}
#[cfg(not(feature = "layer-shell"))]
fn layer_shell_supported() -> bool {
    false
}

#[cfg(feature = "layer-shell")]
fn make_overlay(win: &gtk::Window, monitor: &gtk::gdk::Monitor) {
    use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
    win.init_layer_shell();
    win.set_namespace("wortschatz");
    win.set_layer(Layer::Overlay);
    win.set_monitor(monitor);
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        win.set_anchor(edge, true);
    }
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(KeyboardMode::Exclusive);
}
#[cfg(not(feature = "layer-shell"))]
fn make_overlay(_: &gtk::Window, _: &gtk::gdk::Monitor) {}

pub fn run(mode: Mode) -> i32 {
    let _lock = if mode == Mode::Saver {
        match single_instance() {
            Ok(lock) => lock,
            Err(()) => {
                eprintln!("wortschatz-saver: already running");
                return 0;
            }
        }
    } else {
        None
    };

    let lexicon = match core_file("data/lexicon.json").map(String::from_utf8_lossy) {
        Some(json) => match Lexicon::from_json(&json) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("wortschatz-saver: {e}");
                return 1;
            }
        },
        None => {
            eprintln!("wortschatz-saver: lexicon missing from the build");
            return 1;
        }
    };
    let timing = core_file("timing.js")
        .ok_or_else(|| "timing.js missing from the build".to_string())
        .and_then(|b| Timing::from_js(&String::from_utf8_lossy(b)))
        .unwrap_or_else(|e| {
            eprintln!("wortschatz-saver: {e}; using built-in timing");
            Timing::default()
        });
    let engine = Engine::new(lexicon, state_dir().map(|d| d.join("deck.json")));

    // Wayland app id / X11 class of the windows (used by window rules in the fallback mode).
    glib::set_prgname(Some("wortschatz"));
    if let Err(e) = gtk::init() {
        eprintln!("wortschatz-saver: cannot open a display: {e}");
        return 1;
    }
    let Some(display) = gtk::gdk::Display::default() else {
        eprintln!("wortschatz-saver: no display");
        return 1;
    };
    let monitors: Vec<gtk::gdk::Monitor> = (0..display.n_monitors()).filter_map(|i| display.monitor(i)).collect();
    let overlay = mode == Mode::Saver && layer_shell_supported();

    let shared = Rc::new(RefCell::new(Shared { engine, card: String::new(), views: Vec::new() }));
    shared.borrow_mut().advance();

    // One WebKit context for every view; the wortschatz:// scheme is registered on it once.
    let mut context = WebContext::new(None);
    let targets: Vec<Option<&gtk::gdk::Monitor>> = match mode {
        Mode::Saver if !monitors.is_empty() => monitors.iter().map(Some).collect(),
        _ => vec![None],
    };
    let mut windows = Vec::new();
    for (i, monitor) in targets.into_iter().enumerate() {
        let win = gtk::Window::new(gtk::WindowType::Toplevel);
        win.set_title("Wortschatz");
        match (mode, monitor) {
            (Mode::Saver, Some(m)) if overlay => make_overlay(&win, m),
            (Mode::Saver, m) => {
                win.set_decorated(false);
                if let Some(m) = m {
                    let g = m.geometry();
                    win.set_default_size(g.width(), g.height());
                    if let Some(screen) = gtk::prelude::GtkWindowExt::screen(&win) {
                        win.fullscreen_on_monitor(&screen, i as i32);
                    }
                } else {
                    win.fullscreen();
                }
            }
            (Mode::Window, _) => win.set_default_size(1280, 800),
        }
        win.connect_delete_event(|_, _| {
            gtk::main_quit();
            glib::Propagation::Proceed
        });
        if mode == Mode::Saver {
            // Hide the pointer from the start; the page's `cursor: none` only applies once the
            // pointer moves over it.
            win.connect_realize(|w| {
                if let Some(gw) = w.window() {
                    let blank = gtk::gdk::Cursor::for_display(&gw.display(), gtk::gdk::CursorType::BlankCursor);
                    gw.set_cursor(blank.as_ref());
                }
            });
            // Until the page has loaded (or if it never does), keys still end the saver.
            win.connect_key_press_event(|_, _| {
                gtk::main_quit();
                glib::Propagation::Stop
            });
        }
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        win.add(&container);

        let weak = Rc::downgrade(&shared);
        let mut builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_url("wortschatz://localhost/index.html")
            .with_background_color(BG)
            .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
            .with_initialization_script(init_script(mode))
            .with_ipc_handler(|req: Request<String>| {
                if req.body() == "quit" {
                    gtk::main_quit();
                }
            })
            .with_on_page_load_handler(move |event, _url| {
                if matches!(event, PageLoadEvent::Finished) {
                    if let Some(s) = weak.upgrade() {
                        let s = s.borrow();
                        if let Some(v) = s.views.get(i) {
                            s.show(v);
                        }
                    }
                }
            });
        if i == 0 {
            builder = builder.with_custom_protocol("wortschatz".into(), |_id, req| serve(req));
        }
        match builder.build_gtk(&container) {
            Ok(view) => shared.borrow_mut().views.push(view),
            Err(e) => {
                eprintln!("wortschatz-saver: cannot create the web view: {e}");
                return 1;
            }
        }
        win.show_all();
        windows.push(win);
    }

    schedule(shared.clone(), timing.host_show_after_ms(), timing.cycle_ms);
    gtk::main();
    drop(windows);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_core_has_everything_the_page_loads() {
        for f in ["index.html", "style.css", "app.js", "timing.js", "data/lexicon.json", "data/lexicon.js"] {
            assert!(core_file(f).is_some(), "{f} not embedded");
        }
        let html = String::from_utf8_lossy(core_file("index.html").unwrap());
        assert!(html.contains("wortschatz:"), "the page CSP must allow the wortschatz: scheme");
        let css = String::from_utf8_lossy(core_file("style.css").unwrap());
        for url in css.split("url(\"").skip(1).filter_map(|s| s.split('"').next()) {
            assert!(core_file(url).is_some(), "style.css references missing {url}");
        }
        assert!(css.contains(&format!("--bg: #{:02x}{:02x}{:02x}", BG.0, BG.1, BG.2)), "BG must match --bg in style.css");
    }

    #[test]
    fn serves_index_for_root_and_404_for_unknown() {
        let get = |p: &str| serve(Request::get(format!("wortschatz://localhost{p}")).body(Vec::new()).unwrap());
        assert_eq!(get("/").headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(get("/fonts/Inter.woff2").headers()[CONTENT_TYPE], "font/woff2");
        assert_eq!(get("/nope.js").status(), 404);
    }
}
