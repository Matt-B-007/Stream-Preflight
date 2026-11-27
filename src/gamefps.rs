// SPDX-License-Identifier: GPL-3.0-or-later
//! The game's own frame rate, measured without touching the game.
//!
//! - Windows: Intel PresentMon, if it's installed, reads the system's
//!   frame-present events (the same events Windows performance tools use).
//!   No injection, no hooks, nothing anti-cheat cares about. It usually needs
//!   Preflight to run as administrator; if it can't start, we simply ask.
//! - Linux: if the player already logs with MangoHud, its CSV log from the
//!   test is read afterwards. Preflight never loads MangoHud itself.
//! - Otherwise the app asks "About what FPS do you get?".

use std::path::PathBuf;
#[cfg(windows)]
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct Measured {
    pub fps: f32,
    pub source: String,
}

/// Something measuring during the test.
pub struct Job {
    started: std::time::SystemTime,
    #[cfg(windows)]
    child: Option<std::process::Child>,
    /// exe name (lower case) -> (sum of frame times in ms, frames)
    #[cfg(windows)]
    acc: Arc<Mutex<std::collections::HashMap<String, (f64, u64)>>>,
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn start() -> Job {
    #[cfg(windows)]
    {
        let acc: Arc<Mutex<std::collections::HashMap<String, (f64, u64)>>> = Arc::default();
        let child = presentmon_path().and_then(|p| spawn_presentmon(&p, acc.clone()));
        if child.is_some() {
            log::info!("Game FPS: PresentMon started");
        }
        Job {
            started: std::time::SystemTime::now(),
            child,
            acc,
        }
    }
    #[cfg(not(windows))]
    Job {
        started: std::time::SystemTime::now(),
    }
}

/// Stops measuring and returns the game's average FPS, if any source had it.
/// `exes` are the game's process names (e.g. "cyberpunk2077.exe").
pub fn finish(job: Job, exes: &[String]) -> Option<Measured> {
    #[cfg(windows)]
    {
        let mut job = job;
        if let Some(mut c) = job.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let acc = job.acc.lock().ok()?;
        let wanted: Vec<String> = exes.iter().map(|e| e.to_lowercase()).collect();
        let (sum, n) = acc
            .iter()
            .filter(|(k, _)| wanted.iter().any(|w| k.as_str() == w.as_str()))
            .map(|(_, v)| *v)
            .fold((0.0, 0u64), |(a, b), (s, c)| (a + s, b + c));
        let _ = job.started;
        if n > 60 && sum > 0.0 {
            return Some(Measured {
                fps: (1000.0 * n as f64 / sum) as f32,
                source: "PresentMon".into(),
            });
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = exes;
        mangohud_fps(job.started)
    }
}

// ---------------------------------------------------------------------------
// Windows: PresentMon

#[cfg(windows)]
fn presentmon_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for d in std::env::split_paths(&path) {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if n.starts_with("presentmon") && n.ends_with(".exe") && !n.contains("service")
                    {
                        return Some(e.path());
                    }
                }
            }
        }
    }
    for base in [
        r"C:\Program Files\Intel\PresentMon",
        r"C:\Program Files (x86)\Intel\PresentMon",
    ] {
        if let Some(p) = find_exe(std::path::Path::new(base), 3) {
            return Some(p);
        }
    }
    None
}

#[cfg(windows)]
fn find_exe(dir: &std::path::Path, depth: u32) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().to_lowercase();
        if p.is_file()
            && n.starts_with("presentmon")
            && n.ends_with(".exe")
            && !n.contains("service")
            && !n.contains("capture")
        {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(x) = find_exe(&p, depth - 1) {
                return Some(x);
            }
        }
    }
    None
}

#[cfg(windows)]
fn spawn_presentmon(
    exe: &std::path::Path,
    acc: Arc<Mutex<std::collections::HashMap<String, (f64, u64)>>>,
) -> Option<std::process::Child> {
    use std::io::BufRead;
    use std::os::windows::process::CommandExt;
    let mut child = std::process::Command::new(exe)
        .args([
            "--output_stdout",
            "--stop_existing_session",
            "--session_name",
            "PreFlightFps",
            "--no_console_stats",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // no console window
        .spawn()
        .map_err(|e| log::info!("Game FPS: PresentMon couldn't start: {e}"))
        .ok()?;
    let out = child.stdout.take()?;
    std::thread::spawn(move || {
        let mut app_col = None;
        let mut ms_col = None;
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            let cols: Vec<&str> = line.split(',').collect();
            if app_col.is_none() {
                app_col = cols
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case("Application"));
                ms_col = cols
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case("MsBetweenPresents"))
                    .or_else(|| {
                        cols.iter()
                            .position(|c| c.eq_ignore_ascii_case("FrameTime"))
                    })
                    .or_else(|| {
                        cols.iter()
                            .position(|c| c.eq_ignore_ascii_case("msBetweenPresents"))
                    });
                continue;
            }
            let (Some(a), Some(m)) = (app_col, ms_col) else {
                continue;
            };
            let (Some(app), Some(ms)) =
                (cols.get(a), cols.get(m).and_then(|v| v.parse::<f64>().ok()))
            else {
                continue;
            };
            if ms <= 0.0 || ms > 1000.0 {
                continue;
            }
            if let Ok(mut g) = acc.lock() {
                let e = g.entry(app.to_lowercase()).or_insert((0.0, 0));
                e.0 += ms;
                e.1 += 1;
            }
        }
    });
    Some(child)
}

// ---------------------------------------------------------------------------
// Linux: MangoHud log written during the test

#[cfg(not(windows))]
fn mangohud_fps(started: std::time::SystemTime) -> Option<Measured> {
    let file = mangohud_logs()
        .into_iter()
        .filter(|(_, m)| *m >= started)
        .max_by_key(|(_, m)| *m)
        .map(|(p, _)| p)?;
    let text = std::fs::read_to_string(&file).ok()?;
    let fps = mangohud_avg(&text)?;
    log::info!("Game FPS from MangoHud log: {fps:.0}");
    Some(Measured {
        fps,
        source: "MangoHud log".into(),
    })
}

/// Average of the fps column of a MangoHud CSV log.
#[cfg_attr(windows, allow(dead_code))]
pub fn mangohud_avg(text: &str) -> Option<f32> {
    let mut col = None;
    let mut sum = 0.0f64;
    let mut n = 0u64;
    for line in text.lines() {
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if col.is_none() {
            col = cols.iter().position(|c| c.eq_ignore_ascii_case("fps"));
            continue;
        }
        if let Some(v) = col
            .and_then(|c| cols.get(c))
            .and_then(|v| v.parse::<f64>().ok())
            && v > 0.0
            && v < 2000.0
        {
            sum += v;
            n += 1;
        }
    }
    (n >= 5).then(|| (sum / n as f64) as f32)
}

#[cfg(not(windows))]
fn mangohud_logs() -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(h) = crate::util::home_dir() {
        let cfg = h.join(".config/MangoHud");
        if let Ok(rd) = std::fs::read_dir(&cfg) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "conf").unwrap_or(false)
                    && let Ok(t) = std::fs::read_to_string(e.path())
                {
                    for l in t.lines() {
                        if let Some(v) = l.trim().strip_prefix("output_folder") {
                            let v = v.trim_start_matches([' ', '=']).trim();
                            let p = if let Some(r) = v.strip_prefix("~/") {
                                h.join(r)
                            } else {
                                PathBuf::from(v)
                            };
                            dirs.push(p);
                        }
                    }
                }
            }
        }
        dirs.push(h.clone());
    }
    if let Ok(env) = std::env::var("MANGOHUD_CONFIG") {
        for part in env.split(',') {
            if let Some(v) = part.trim().strip_prefix("output_folder=") {
                dirs.push(PathBuf::from(v));
            }
        }
    }
    dirs.push(PathBuf::from("/tmp"));
    let mut out = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_lowercase();
                if name.ends_with(".csv")
                    && !name.ends_with("_summary.csv")
                    && let Ok(m) = e.metadata().and_then(|m| m.modified())
                {
                    out.push((p, m));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn mangohud_csv() {
        let t = "os,cpu,gpu,ram,kernel,driver,cpuscheduler\nArch,Ryzen,RTX,64,6.x,570,\nfps,frametime,cpu_load,gpu_load\n100,10,20,90\n110,9,20,90\n90,11,20,90\n100,10,20,90\n100,10,20,90\n";
        let v = super::mangohud_avg(t).unwrap();
        assert!((v - 100.0).abs() < 0.1);
    }
}
