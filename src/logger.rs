//! Logging: a small rolling log file plus an in-memory buffer for bug
//! reports. Every line is tagged with its component and scrubbed of
//! personal information before it is stored.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const RING_LINES: usize = 600;
const MAX_FILE_BYTES: u64 = 512 * 1024;

pub static DETAILED: AtomicBool = AtomicBool::new(false);

struct Inner {
    ring: VecDeque<String>,
    file: Option<File>,
    written: u64,
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
            if g.written < MAX_FILE_BYTES {
                let n = line.len() as u64 + 1;
                if let Some(f) = g.file.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                g.written += n;
            }
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

/// `name`.log (and `name`.previous.log). Tray-only mode keeps its own log so
/// handing over to the window doesn't push the window's log aside.
pub fn init_named(name: &str) {
    let dir = &crate::util::dirs().logs;
    // Keep the previous session's log, replace the one before it.
    let cur = dir.join(format!("{name}.log"));
    let prev = dir.join(format!("{name}.previous.log"));
    if cur.exists() {
        let _ = std::fs::rename(&cur, &prev);
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&cur)
        .ok();
    let logger = LOGGER.get_or_init(|| Logger {
        inner: Mutex::new(Inner {
            ring: VecDeque::with_capacity(RING_LINES),
            file,
            written: 0,
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

/// Saved reports and old crash files are removed after 14 days.
fn cleanup_old_files() {
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
    #[test]
    fn crate_name_matches_module_paths() {
        // Our log lines come from targets like "stream_preflight::app".
        assert!(module_path!().starts_with(env!("CARGO_CRATE_NAME")));
    }
}
