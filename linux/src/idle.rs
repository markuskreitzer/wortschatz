//! `wortschatz-saver idle`: start the saver when the Wayland session goes idle.
//!
//! Uses the ext-idle-notify-v1 protocol (Hyprland, Sway, KDE Plasma, niri, ...). The
//! compositor sends `idled` after the timeout and `resumed` on the next input; the saver is
//! started on the first and stopped on the second (it also quits on input by itself). Idle
//! inhibitors such as a playing video are respected.
//!
//! Hyprland reports `resumed` a fraction of a second after the saver's surfaces appear (the
//! focus change counts as activity), so a `resumed` within [`GRACE`] of the launch is ignored;
//! the saver itself still quits on real input.
//!
//! GNOME (Ubuntu's default session) does not offer ext-idle-notify-v1; there `idle` exits with
//! status 3 and a message. A GNOME backend (org.gnome.Mutter.IdleMonitor over D-Bus) is the
//! planned addition; see linux/README.md.
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

/// Exit status for "this session cannot be watched"; the systemd unit does not restart on it.
const UNSUPPORTED: i32 = 3;

/// `resumed` this soon after starting the saver is the saver's own appearance, not the user.
const GRACE: Duration = Duration::from_secs(3);

struct State {
    child: Option<Child>,
    started: Instant,
    skip_when: Option<String>,
}

impl State {
    fn running(&mut self) -> bool {
        match self.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            _ => {
                self.child = None;
                false
            }
        }
    }

    fn idled(&mut self) {
        if self.running() {
            return;
        }
        if let Some(cmd) = &self.skip_when {
            if Command::new("sh").arg("-c").arg(cmd).status().is_ok_and(|s| s.success()) {
                return;
            }
        }
        let exe = std::env::current_exe().unwrap_or_else(|_| "wortschatz-saver".into());
        match Command::new(exe).arg("run").spawn() {
            Ok(c) => {
                eprintln!("wortschatz-saver idle: idle, starting the saver (pid {})", c.id());
                self.child = Some(c);
                self.started = Instant::now();
            }
            Err(e) => eprintln!("wortschatz-saver idle: cannot start the saver: {e}"),
        }
    }

    fn resumed(&mut self) {
        if !self.running() || self.started.elapsed() < GRACE {
            return;
        }
        eprintln!("wortschatz-saver idle: activity, stopping the saver");
        let Some(mut child) = self.child.take() else { return };
        // SIGTERM first so the saver exits normally; SIGKILL if it has not gone within 2 s.
        let _ = Command::new("kill").args(["-TERM", &child.id().to_string()]).status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(state: &mut Self, _: &ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.idled(),
            ext_idle_notification_v1::Event::Resumed => state.resumed(),
            _ => {}
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn run(args: &[String]) -> i32 {
    let mut timeout = env("WZ_IDLE_TIMEOUT");
    let mut skip_when = env("WZ_SKIP_WHEN");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--timeout" => timeout = it.next().cloned(),
            "--skip-when" => skip_when = it.next().cloned(),
            other => {
                eprintln!("wortschatz-saver idle: unknown option {other}");
                return 2;
            }
        }
    }
    let secs: u32 = match timeout.as_deref().map(str::parse).unwrap_or(Ok(300)) {
        Ok(s) if s > 0 => s,
        _ => {
            eprintln!("wortschatz-saver idle: --timeout needs a positive number of seconds");
            return 2;
        }
    };

    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("wortschatz-saver idle: not in a Wayland session ({e})");
            return UNSUPPORTED;
        }
    };
    let (globals, mut queue) = match registry_queue_init::<State>(&conn) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("wortschatz-saver idle: Wayland registry: {e}");
            return 1;
        }
    };
    let qh = queue.handle();
    let Ok(seat) = globals.bind::<WlSeat, _, _>(&qh, 1..=1, ()) else {
        eprintln!("wortschatz-saver idle: no seat");
        return UNSUPPORTED;
    };
    let Ok(notifier) = globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=1, ()) else {
        eprintln!(
            "wortschatz-saver idle: this compositor does not support ext-idle-notify-v1. \
             GNOME is not supported yet (see linux/README.md); run `wortschatz-saver` from your \
             desktop's own idle hook instead."
        );
        return UNSUPPORTED;
    };
    let _notification = notifier.get_idle_notification(secs.saturating_mul(1000), &seat, &qh, ());
    eprintln!("wortschatz-saver idle: starting the saver after {secs} s idle");

    let mut state = State { child: None, started: Instant::now(), skip_when };
    loop {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            eprintln!("wortschatz-saver idle: Wayland connection lost: {e}");
            state.resumed();
            return 1;
        }
    }
}
