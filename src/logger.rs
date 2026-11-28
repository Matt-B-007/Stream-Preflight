// SPDX-License-Identifier: GPL-3.0-or-later
//! Logging: a small rolling log file plus an in-memory buffer for bug
//! reports. Every line is tagged with its component and scrubbed of
//! personal information before it is stored.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Lines kept in memory for bug reports.
const RING_LINES: usize = 500;
/// Each log file holds at most 1 MB; 3 files are kept (name.log, name.1.log, name.2.log).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const KEEP_FILES: usize = 3;
/// Crash files from the last 10 sessions are kept.
const KEEP_CRASHES: usize = 10;

pub static DETAILED: AtomicBool = AtomicBool::new(false);

struct Inner {
    ring: VecDeque<String>,
    file: Option<File>,
    written: u64,
    path: Option<std::path::PathBuf>,
}

struct Logger {
    inner: Mutex<Inner>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        if DETAILED.load(Ordering::Relaxed) {
            m.level() <= Level::Debug
        } else {
            m.level() <= Level::Info
        }
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let target = r.target();
        // Only keep our own lines plus warnings from libraries.
        // The crate name (it was stream_preflight_check before the rename).
        let ours = target.starts_with(env!("CARGO_CRATE_NAME"));
        if !ours && r.level() > Level::Warn {
            return;
        }
        let comp = target.rsplit("::").next().unwrap_or(target);
        let line = crate::util::scrub(&format!(
            "{} {:5} [{}] {}",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            r.level(),
            comp,
            r.args()
        ));
        if let Ok(mut g) = self.inner.lock() {
            if g.ring.len() >= RING_LINES {
                g.ring.pop_front();
            }
            g.ring.push_back(line.clone());
            let n = line.len() as u64 + 1;
            if g.written + n > MAX_FILE_BYTES
                && let Some(p) = g.path.clone()
            {
                // Full: start a new file and keep the last two.
                g.file = None;
                rotate(&p);
                g.file = open_new(&p);
                g.written = 0;
            }
            if let Some(f) = g.file.as_mut() {
                let _ = writeln!(f, "{line}");
            }
            g.written += n;
        }
    }

    fn flush(&self) {
        if let Ok(mut g) = self.inner.lock()
            && let Some(f) = g.file.as_mut()
        {
            let _ = f.flush();
        }
    }
}

pub fn init() {
    init_named("preflight");
}

/// The n-th older file: name.log → name.1.log → name.2.log.
fn numbered(cur: &std::path::Path, n: usize) -> std::path::PathBuf {
    let stem = cur
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    cur.with_file_name(format!("{stem}.{n}.log"))
}

/// Shifts name.log to name.1.log and so on; the oldest beyond 3 files is removed.
pub fn rotate(cur: &std::path::Path) {
    let _ = std::fs::remove_file(numbered(cur, KEEP_FILES - 1));
    for n in (1..KEEP_FILES - 1).rev() {
        let _ = std::fs::rename(numbered(cur, n), numbered(cur, n + 1));
    }
    if cur.exists() {
        let _ = std::fs::rename(cur, numbered(cur, 1));
    }
}

fn open_new(p: &std::path::Path) -> Option<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(p)
        .ok()
}

/// `name`.log, plus the two before it. Tray-only mode keeps its own log so
/// handing over to the window doesn't push the window's log aside.
pub fn init_named(name: &str) {
    let dir = &crate::util::dirs().logs;
    let cur = dir.join(format!("{name}.log"));
    // Each session starts a new file.
    rotate(&cur);
    // Older versions kept one "previous" file.
    let _ = std::fs::remove_file(dir.join(format!("{name}.previous.log")));
    let file = open_new(&cur);
    let logger = LOGGER.get_or_init(|| Logger {
        inner: Mutex::new(Inner {
            ring: VecDeque::with_capacity(RING_LINES),
            file,
            written: 0,
            path: Some(cur.clone()),
        }),
    });
    let _ = log::set_logger(logger);
    log::set_max_level(LevelFilter::Debug);
    cleanup_old_files();
}

/// Recent log lines for the Diagnostics screen and bug reports.
pub fn recent(n: usize) -> Vec<String> {
    LOGGER
        .get()
        .and_then(|l| {
            l.inner
                .lock()
                .ok()
                .map(|g| g.ring.iter().rev().take(n).rev().cloned().collect())
        })
        .unwrap_or_default()
}

pub fn set_detailed(on: bool) {
    DETAILED.store(on, Ordering::Relaxed);
    log::info!(
        "Detailed logging {}",
        if on { "turned on" } else { "turned off" }
    );
}

/// Saved reports and old crash files are removed after 14 days, and only the
/// crash files of the last 10 sessions are kept.
fn cleanup_old_files() {
    let crashes = pending_crashes();
    if crashes.len() > KEEP_CRASHES {
        for p in &crashes[..crashes.len() - KEEP_CRASHES] {
            let _ = std::fs::remove_file(p);
        }
    }
    let limit = std::time::Duration::from_secs(14 * 24 * 3600);
    for dir in [&crate::util::dirs().reports, &crate::util::dirs().logs] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let keep = p
                    .file_name()
                    .map(|n| n.to_string_lossy().contains(".keep"))
                    .unwrap_or(false);
                if keep {
                    continue;
                }
                if let Ok(m) = e.metadata()
                    && let Ok(age) = m
                        .modified()
                        .and_then(|t| t.elapsed().map_err(std::io::Error::other))
                    && age > limit
                {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Crash handling

static LAST_ACTIVITY: Mutex<String> = Mutex::new(String::new());

static LAST_BY_KEY: std::sync::LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Logs a line only when it differs from the last one with the same key, so
/// checks that repeat every few seconds don't flood the log.
pub fn info_changed(key: &str, msg: String) {
    let target = key.split(':').next().unwrap_or("app");
    if let Ok(mut m) = LAST_BY_KEY.lock() {
        if m.get(key) == Some(&msg) {
            return;
        }
        m.insert(key.to_string(), msg.clone());
    }
    let target = match target {
        "obs" => "obs",
        _ => "games",
    };
    log::info!(target: target, "{msg}");
}

/// Records what the app is doing, so a crash file can say so.
pub fn activity(s: &str) {
    if let Ok(mut g) = LAST_ACTIVITY.lock() {
        *g = s.to_string();
    }
}

pub fn install_crash_handler() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let what = LAST_ACTIVITY.lock().map(|g| g.clone()).unwrap_or_default();
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown".into());
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let body = crate::util::scrub(&format!(
            "Stream Preflight crash\nversion: {} ({})\ntime: {}\nthread: {}\nwas doing: {}\nwhere: {}\nmessage: {}\n\nrecent log:\n{}\n",
            crate::util::VERSION,
            crate::util::BUILD_LABEL,
            crate::util::now_local(),
            thread,
            what,
            loc,
            msg,
            recent(80).join("\n")
        ));
        let path = crate::util::dirs()
            .logs
            .join(format!("crash-{}.txt", crate::util::now_stamp()));
        let _ = std::fs::write(&path, body);
        log::error!("Crash in thread {thread} at {loc}: {msg}");
        default(info);
    }));
}

// ---------------------------------------------------------------------------
// "Closed unexpectedly" notice: a marker file exists while the window is open.

/// What the last window session left behind, if it didn't end cleanly.
#[derive(Clone, Debug, Default)]
pub struct Unclean {
    /// The newest crash file written during that session, if any.
    pub crash_file: Option<std::path::PathBuf>,
}

static UNCLEAN: OnceLock<Option<Unclean>> = OnceLock::new();

fn marker_path() -> std::path::PathBuf {
    crate::util::dirs().logs.join("running")
}

fn last_start_path() -> std::path::PathBuf {
    crate::util::dirs().logs.join("last-start")
}

/// Checks how the last session ended, then marks this one as running.
/// Call once, when the window is about to open.
pub fn start_session() {
    let unclean = session_check(&marker_path(), &last_start_path(), &pending_crashes());
    if unclean.is_some() {
        log::warn!("The last session didn't close normally");
    }
    let _ = UNCLEAN.set(unclean);
    let now = crate::util::unix_now().to_string();
    let _ = std::fs::write(marker_path(), crate::util::VERSION);
    let _ = std::fs::write(last_start_path(), now);
    close_cleanly_on_signals();
}

/// Logging out, shutting down or `kill` ask the program to stop with a signal.
/// That's a normal end, not a crash: the marker is removed before stopping.
#[cfg(unix)]
fn close_cleanly_on_signals() {
    use std::os::unix::ffi::OsStrExt;
    static MARKER: OnceLock<std::ffi::CString> = OnceLock::new();
    extern "C" fn on_signal(sig: libc::c_int) {
        // Only signal-safe calls: remove the marker, then stop the default way.
        unsafe {
            if let Some(p) = MARKER.get() {
                libc::unlink(p.as_ptr());
            }
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }
    let Ok(path) = std::ffi::CString::new(marker_path().as_os_str().as_bytes()) else {
        return;
    };
    let _ = MARKER.set(path);
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        unsafe {
            libc::signal(
                sig,
                on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
}

#[cfg(not(unix))]
fn close_cleanly_on_signals() {}

/// The marker is removed when the window closes normally.
pub fn end_session() {
    let _ = std::fs::remove_file(marker_path());
}

/// Some(..) when the last session ended unexpectedly: the marker was left behind,
/// or a crash file was written after it started.
pub fn unclean_last_time() -> Option<Unclean> {
    UNCLEAN.get().cloned().flatten()
}

fn session_check(
    marker: &std::path::Path,
    last_start: &std::path::Path,
    crashes: &[std::path::PathBuf],
) -> Option<Unclean> {
    let started: Option<std::time::SystemTime> = std::fs::read_to_string(last_start)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t));
    let crash_file = crashes
        .iter()
        .filter(|p| {
            let m = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            match (m, started) {
                (Some(m), Some(st)) => m >= st,
                _ => false,
            }
        })
        .max()
        .cloned();
    if marker.exists() || crash_file.is_some() {
        Some(Unclean { crash_file })
    } else {
        None
    }
}

/// Crash files from earlier sessions that haven't been reported yet.
pub fn pending_crashes() -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(&crate::util::dirs().logs)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("crash-") && n.ends_with(".txt")
                        })
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_an_unclean_end() {
        let dir = std::env::temp_dir().join(format!("pf-mark-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (m, ls) = (dir.join("running"), dir.join("last-start"));
        // First run ever: nothing to report.
        assert!(session_check(&m, &ls, &[]).is_none());
        // Clean exit last time: no marker.
        std::fs::write(&ls, (crate::util::unix_now() - 60).to_string()).unwrap();
        assert!(session_check(&m, &ls, &[]).is_none());
        // Marker left behind (killed, power cut).
        std::fs::write(&m, "x").unwrap();
        let u = session_check(&m, &ls, &[]).unwrap();
        assert!(u.crash_file.is_none());
        // A crash file from that session is named.
        let crash = dir.join("crash-1.txt");
        std::fs::write(&crash, "boom").unwrap();
        std::fs::remove_file(&m).unwrap();
        let u = session_check(&m, &ls, std::slice::from_ref(&crash)).unwrap();
        assert_eq!(u.crash_file.as_deref(), Some(crash.as_path()));
        // An old crash file from before that session doesn't count.
        std::fs::write(&ls, (crate::util::unix_now() + 60).to_string()).unwrap();
        assert!(session_check(&m, &ls, &[crash]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_three_log_files() {
        let dir = std::env::temp_dir().join(format!("pf-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cur = dir.join("preflight.log");
        for k in 0..5 {
            std::fs::write(&cur, format!("session {k}")).unwrap();
            rotate(&cur);
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        // name.log is opened next; the two newest older sessions are kept.
        assert_eq!(names, ["preflight.1.log", "preflight.2.log"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("preflight.1.log")).unwrap(),
            "session 4"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("preflight.2.log")).unwrap(),
            "session 3"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crate_name_matches_module_paths() {
        // Our log lines come from targets like "stream_preflight::app".
        assert!(module_path!().starts_with(env!("CARGO_CRATE_NAME")));
    }
}
