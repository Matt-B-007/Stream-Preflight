// SPDX-License-Identifier: GPL-3.0-or-later
//! Tray-only mode (`stream-preflight --tray`): the tray icon and the OBS
//! watcher, with no window. On KDE/GNOME Wayland an app can't hide its own
//! window, so "Start in tray", "Minimize to tray" and "Close to tray" run
//! this small mode instead. Clicking the icon hands over to the full window.
//!
//! Only one Preflight watches OBS at a time: the tray process writes
//! `tray.pid`; a window that starts asks it to step aside first (it saves an
//! unfinished stream, which the window picks up), and the tray process starts
//! the window when you click Open.

use crate::streamwatch::{self, WatchConfig};
use crate::tray::{Tray, TrayCmd, TrayState, TrayView};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn pid_file() -> PathBuf {
    crate::util::dirs().config.join("tray.pid")
}

fn handoff_file() -> PathBuf {
    crate::util::dirs().config.join("tray.handoff")
}

/// The tray-only process, if one is running.
pub fn running_tray() -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(pid_file())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if pid == std::process::id() {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let cmd = String::from_utf8_lossy(&cmd);
        if !cmd.contains("preflight") || !cmd.contains("--tray") {
            let _ = std::fs::remove_file(pid_file());
            return None;
        }
        Some(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Asks a running tray-only process to save its work and close, and waits for it.
pub fn ask_tray_to_exit() {
    if running_tray().is_none() {
        return;
    }
    log::info!("Asking the tray-only Preflight to hand over");
    let _ = std::fs::write(handoff_file(), "window");
    let t = Instant::now();
    while running_tray().is_some() && t.elapsed() < Duration::from_secs(4) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(handoff_file());
}

fn spawn_self(args: &[&str]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut c = std::process::Command::new(exe);
    c.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0008); // detached
    }
    c.spawn().is_ok()
}

/// From the window: start the tray-only mode (the window then closes, and
/// the tray-only copy waits for that instead of stepping aside).
pub fn start_tray_process() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .arg("--tray")
        .env(crate::instance::RESTART_ENV, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Runs the tray-only mode until the user opens the window or quits.
pub fn run() {
    if running_tray().is_some() {
        log::info!("Tray-only Preflight is already running");
        return;
    }
    // Handed over by a window that's closing: wait for it to go.
    if std::env::var_os(crate::instance::RESTART_ENV).is_some() {
        let t = std::time::Instant::now();
        while crate::instance::window_running() && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    // A window already has the tray icon and watches OBS.
    if crate::instance::window_running() {
        log::info!("A Preflight window is open, so the tray-only mode isn't needed");
        return;
    }
    let st = crate::config::Settings::load();
    let ctx = eframe::egui::Context::default();
    let Some(mut tray) = Tray::start(ctx.clone()) else {
        // No tray on this desktop: there's nowhere to live without a window.
        log::warn!("No system tray found; opening the window instead");
        spawn_self(&[]);
        return;
    };
    let _ = std::fs::write(pid_file(), std::process::id().to_string());
    let _ = std::fs::remove_file(handoff_file());
    let cfg = WatchConfig {
        host: st.obs_host.clone(),
        port: st.obs_port,
        password: crate::config::load_secret(),
        game_processes: crate::games::profiles()
            .iter()
            .map(|p| (p.name.clone(), p.processes.clone()))
            .collect(),
    };
    let watch = streamwatch::start(cfg, ctx);
    log::info!("Tray-only mode: watching OBS with no window");
    let mut was_live = false;
    let mut last_view = TrayView::default();
    let open_window = loop {
        std::thread::sleep(Duration::from_millis(1000));
        if handoff_file().exists() {
            break false;
        }
        let mut open = false;
        while let Some(c) = tray.poll() {
            match c {
                TrayCmd::Open => open = true,
                TrayCmd::Quit => {
                    watch.stop_and_wait(Duration::from_secs(3));
                    let _ = std::fs::remove_file(pid_file());
                    log::info!("Quit from the tray");
                    return;
                }
            }
        }
        if open {
            break true;
        }
        let (connected, live, finished) = match watch.state.lock() {
            Ok(mut g) => (g.connected, g.live.clone(), g.finished.take()),
            Err(_) => (false, None, None),
        };
        if let Some(l) = finished {
            let p = streamwatch::summarize(&l);
            if st.tray_notifications {
                tray.notify(
                    "Stream ended",
                    &format!(
                        "{} · {}. Open Preflight for your post-flight summary.",
                        streamwatch::fmt_duration(p.duration),
                        p.grade
                    ),
                );
            }
        }
        if live.is_some() && !was_live && st.live_opens_window {
            break true;
        }
        was_live = live.is_some();
        let view = view_for(connected, live.as_ref(), st.last_fix_count);
        if view != last_view {
            tray.set(view.clone());
            last_view = view;
        }
    };
    // Hand over: save an unfinished stream so the next Preflight carries on with it.
    watch.stop_and_wait(Duration::from_secs(3));
    let _ = std::fs::remove_file(pid_file());
    if open_window {
        log::info!("Opening the window from the tray");
        spawn_self(&[]);
    } else {
        log::info!("The window took over from the tray");
    }
}

/// What the tray icon shows: not connected, ready, things to fix, or live.
pub fn view_for(connected: bool, live: Option<&streamwatch::LiveStatus>, fixes: u32) -> TrayView {
    if let Some(l) = live {
        let rec = l
            .rec
            .as_ref()
            .filter(|r| r.active)
            .map(|_| " · recording")
            .unwrap_or("");
        TrayView {
            state: TrayState::Live,
            attention: false,
            status: format!("Live · {}{rec}", streamwatch::fmt_t(l.elapsed)),
        }
    } else if !connected {
        TrayView {
            state: TrayState::Idle,
            attention: false,
            status: "OBS not connected".into(),
        }
    } else if fixes > 0 {
        TrayView {
            state: TrayState::NeedsFixes,
            attention: false,
            status: format!("{fixes} to fix before you go live"),
        }
    } else {
        TrayView {
            state: TrayState::Watching,
            attention: false,
            status: "Watching OBS · ready".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_states() {
        assert_eq!(view_for(false, None, 0).state, TrayState::Idle);
        assert_eq!(view_for(true, None, 0).state, TrayState::Watching);
        let f = view_for(true, None, 2);
        assert_eq!(
            (f.state, f.status.as_str()),
            (TrayState::NeedsFixes, "2 to fix before you go live")
        );
        let l = streamwatch::LiveStatus {
            elapsed: 75.0,
            rec: Some(streamwatch::RecStatus {
                active: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let v = view_for(true, Some(&l), 2);
        assert_eq!(v.state, TrayState::Live);
        assert!(v.status.ends_with("· recording"), "{}", v.status);
    }
}
