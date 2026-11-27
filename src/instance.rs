// SPDX-License-Identifier: GPL-3.0-or-later
//! One window at a time. The first window listens on a small local socket
//! (only this user can reach it); a second launch tells it to come forward
//! and exits. On Wayland the desktop decides whether the window may take
//! focus, so bringing it forward is best effort (KDE may flash its taskbar
//! entry instead).

use std::sync::atomic::{AtomicBool, Ordering};

static SHOW: AtomicBool = AtomicBool::new(false);

/// Set on the new copy started by a restart: it waits for the old window to
/// close instead of asking it to come forward.
pub const RESTART_ENV: &str = "STREAM_PREFLIGHT_RESTART";

/// Set when another launch asked this window to come forward.
pub fn take_show_request() -> bool {
    SHOW.swap(false, Ordering::Relaxed)
}

pub enum Claim {
    /// This is the only window: keep the listener for `serve` (None if the
    /// socket couldn't be made; the window still opens).
    #[cfg(unix)]
    First(Option<std::os::unix::net::UnixListener>),
    #[cfg(not(unix))]
    First,
    /// Another window is running and was asked to come forward.
    Existing,
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() => std::path::PathBuf::from(d).join("stream-preflight.sock"),
        _ => crate::util::dirs().data.join("window.sock"),
    }
}

/// True when a Preflight window is running (it isn't asked to do anything).
#[cfg(unix)]
pub fn window_running() -> bool {
    std::os::unix::net::UnixStream::connect(socket_path()).is_ok()
}
#[cfg(not(unix))]
pub fn window_running() -> bool {
    false
}

#[cfg(unix)]
pub fn claim() -> Claim {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};
    let p = socket_path();
    if std::env::var_os(RESTART_ENV).is_some() {
        // Restarting after an update: give the old window up to 10 s to close.
        let t = std::time::Instant::now();
        while UnixStream::connect(&p).is_ok() && t.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // SAFETY: called at start-up before any other threads exist.
        unsafe { std::env::remove_var(RESTART_ENV) };
    }
    if let Ok(mut s) = UnixStream::connect(&p) {
        let _ = s.write_all(b"show\n");
        return Claim::Existing;
    }
    // Nobody answered: a file left by a window that closed. Replace it.
    let _ = std::fs::remove_file(&p);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    match UnixListener::bind(&p) {
        Ok(l) => {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
            Claim::First(Some(l))
        }
        Err(e) => {
            // Can't listen (unusual): still open, just without the single-window check.
            log::warn!("Single-window check unavailable: {e}");
            Claim::First(None)
        }
    }
}

#[cfg(not(unix))]
pub fn claim() -> Claim {
    Claim::First
}

#[cfg(unix)]
static LISTENER: std::sync::Mutex<Option<std::os::unix::net::UnixListener>> =
    std::sync::Mutex::new(None);

/// Keeps the listener until the window starts.
#[cfg(unix)]
pub fn hold(l: std::os::unix::net::UnixListener) {
    if let Ok(mut g) = LISTENER.lock() {
        *g = Some(l);
    }
}

/// Starts answering "come forward" requests (called once the window exists).
pub fn serve(ctx: eframe::egui::Context) {
    #[cfg(unix)]
    {
        use std::io::{BufRead, BufReader};
        let Some(l) = LISTENER.lock().ok().and_then(|mut g| g.take()) else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("single-window".into())
            .spawn(move || {
                for s in l.incoming().flatten() {
                    let mut line = String::new();
                    let _ = BufReader::new(s).read_line(&mut line);
                    if line.trim() == "show" {
                        log::info!("Another launch asked this window to come forward");
                        SHOW.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                }
            });
    }
    #[cfg(not(unix))]
    let _ = ctx;
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn second_launch_reaches_the_first() {
        let dir = std::env::temp_dir().join(format!("sp-inst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: tests in this module don't run alongside other code that reads this variable.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let first = super::claim();
        let super::Claim::First(Some(l)) = first else {
            panic!("the first launch should be first")
        };
        super::hold(l);
        super::serve(eframe::egui::Context::default());
        assert!(super::window_running());
        assert!(
            matches!(super::claim(), super::Claim::Existing),
            "a second launch finds the first"
        );
        let t = std::time::Instant::now();
        while !super::take_show_request() {
            assert!(
                t.elapsed().as_secs() < 3,
                "the first window got the request"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
