//! Shared helpers: app folders, time, and privacy scrubbing.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const APP_NAME: &str = "Stream Preflight";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Label shown in the app while we are in private testing.
/// "2.1% of one core" from a share of the whole CPU (how Preflight measures itself).
pub fn one_core(pct_of_all: f32) -> String {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
    let v = pct_of_all * cores;
    if v < 0.1 { "under 0.1% of one core".into() } else { format!("{v:.1}% of one core") }
}

pub const BUILD_LABEL: &str = "Beta 5";

/// Folders the app is allowed to write to. Everything lives under the
/// user's normal per-app data folder; nothing is written anywhere else.
pub struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub logs: PathBuf,
    pub reports: PathBuf,
    /// Frames for the viewer preview (latest test only).
    pub previews: PathBuf,
    /// The last full test for each game (no recordings, no paths).
    pub tests: PathBuf,
    /// Post-flight logs of past streams.
    pub streams: PathBuf,
}

static DIRS: OnceLock<Dirs> = OnceLock::new();

pub fn dirs() -> &'static Dirs {
    DIRS.get_or_init(|| {
        let base = directories::ProjectDirs::from("", "", "StreamPreflight");
        migrate_old_folders(&base);
        let (config, data) = match &base {
            Some(p) => (p.config_dir().to_path_buf(), p.data_dir().to_path_buf()),
            None => {
                let t = std::env::temp_dir().join("StreamPreflight");
                (t.join("config"), t.join("data"))
            }
        };
        let logs = data.join("logs");
        let reports = data.join("reports");
        let previews = data.join("previews");
        let tests = data.join("tests");
        let streams = data.join("streams");
        for d in [&config, &data, &logs, &reports, &previews, &streams, &tests] {
            let _ = std::fs::create_dir_all(d);
        }
        Dirs { config, data, logs, reports, previews, streams, tests }
    })
}

pub fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// "Nov 13, 19:16" for a unix time.
pub fn fmt_local(t: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(t, 0).single().map(|d| d.format("%b %-d, %H:%M").to_string()).unwrap_or_default()
}

pub fn now_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn unix_now() -> i64 {
    chrono::Local::now().timestamp()
}

/// Home folder of the current user, used for scrubbing paths.
pub fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

pub fn user_name() -> Option<String> {
    std::env::var("USERNAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .filter(|s| !s.is_empty())
}

/// Removes things that identify the person or the PC from text that may
/// end up in a log line or a bug report:
/// - the home folder and user name in paths
/// - IPv4 addresses and MAC addresses
/// - anything that looks like a stream key, token or password value
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    if let Some(home) = home_dir() {
        let h = home.to_string_lossy().to_string();
        if h.len() > 3 {
            out = out.replace(&h, "<home>");
            out = out.replace(&h.replace('\\', "/"), "<home>");
        }
    }
    if let Some(u) = user_name() {
        if u.len() >= 3 {
            for (a, b) in [
                (format!("\\Users\\{u}"), "\\Users\\<user>".to_string()),
                (format!("/Users/{u}"), "/Users/<user>".to_string()),
                (format!("/home/{u}"), "/home/<user>".to_string()),
            ] {
                out = out.replace(&a, &b);
            }
        }
    }
    out = scrub_secrets(&out);
    out = scrub_steam_ids(&out);
    out = scrub_ipv4(&out);
    out = scrub_mac(&out);
    out
}

/// Steam IDs (17 digits starting 7656119) and the account number in Steam's
/// "userdata/<number>" folder identify the player, so they're replaced.
pub fn scrub_steam_ids(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        // userdata/<digits> or userdata\<digits>
        if (s[i..].starts_with("userdata/") || s[i..].starts_with("userdata\\")) && i + 9 < b.len() && b[i + 9].is_ascii_digit() {
            let mut j = i + 9;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            out.push_str(&s[i..i + 9]);
            out.push_str("<id>");
            i = j;
            continue;
        }
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - i == 17 && s[i..].starts_with("7656119") {
                out.push_str("<steam id>");
                i = j;
                continue;
            }
            out.push_str(&s[i..j]);
            i = j;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn scrub_secrets(s: &str) -> String {
    // Only key/value lines whose KEY names a secret, e.g. `password=...`
    // or `"stream_key": "..."`. The value is replaced; the key is kept.
    let keys = ["password", "passwd", "token", "stream_key", "streamkey", "secret", "authentication"];
    let mut lines = Vec::new();
    for line in s.lines() {
        let cut = line.find(['=', ':']);
        let is_secret = cut
            .map(|c| {
                let k = line[..c].to_lowercase();
                keys.iter().any(|w| k.contains(w)) && k.len() < 60
            })
            .unwrap_or(false);
        if let (true, Some(c)) = (is_secret, cut) {
            lines.push(format!("{}{} <removed>", &line[..c], &line[c..c + 1]));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut r = lines.join("\n");
    if s.ends_with('\n') {
        r.push('\n');
    }
    r
}

fn scrub_ipv4(s: &str) -> String {
    let bytes: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        // try to match d{1,3}(.d{1,3}){3}
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != '.') {
            let mut j = i;
            let mut parts = 0;
            let mut ok = true;
            loop {
                let start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() && j - start < 4 {
                    j += 1;
                }
                let len = j - start;
                if len == 0 || len > 3 {
                    ok = false;
                    break;
                }
                let n: u32 = bytes[start..j].iter().collect::<String>().parse().unwrap_or(999);
                if n > 255 {
                    ok = false;
                    break;
                }
                parts += 1;
                if parts == 4 {
                    break;
                }
                if j < bytes.len() && bytes[j] == '.' {
                    j += 1;
                } else {
                    ok = false;
                    break;
                }
            }
            let next_ok = j >= bytes.len() || !(bytes[j].is_ascii_digit() || bytes[j] == '.');
            if ok && parts == 4 && next_ok {
                let ip: String = bytes[i..j].iter().collect();
                if ip == "127.0.0.1" || ip == "0.0.0.0" {
                    out.push_str(&ip);
                } else {
                    out.push_str("<ip-address>");
                }
                i = j;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn scrub_mac(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 17 <= chars.len() {
            let cand = &chars[i..i + 17];
            let is_mac = (0..17).all(|k| {
                if k % 3 == 2 {
                    cand[k] == ':' || cand[k] == '-'
                } else {
                    cand[k].is_ascii_hexdigit()
                }
            }) && cand[2] == cand[5];
            if is_mac {
                out.push_str("<mac-address>");
                i += 17;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A short, human-friendly ID like "PF-7K3Q-M2XD".
pub fn short_id() -> String {
    const A: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut s = String::from("PF-");
    for k in 0..8 {
        if k == 4 {
            s.push('-');
        }
        s.push(A[fastrand::usize(..A.len())] as char);
    }
    s
}

pub fn fmt_bytes(b: u64) -> String {
    let gb = b as f64 / 1_073_741_824.0;
    let mb = b as f64 / 1_048_576.0;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else if mb >= 1.0 {
        format!("{mb:.0} MB")
    } else if b > 0 {
        format!("{:.0} KB", (b as f64 / 1024.0).max(1.0))
    } else {
        "0 KB".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrubs_ip_and_mac_and_secrets() {
        let s = "connect 192.168.1.20 ok; mac aa:bb:cc:dd:ee:ff\npassword=hunter2\n\"stream_key\": \"live_abc\"\n\"key\": \"FrameRateLimit\"\nversion 1.2.3";
        let r = scrub(s);
        assert!(r.contains("<ip-address>"));
        assert!(r.contains("<mac-address>"));
        assert!(!r.contains("hunter2"));
        assert!(r.contains("version 1.2.3"));
        assert!(!r.contains("live_abc"));
        assert!(r.contains("FrameRateLimit"));
    }
}

/// The app used to be called "Stream Pre-Flight Check". The first time the new
/// name runs, its settings, saved password, history and stream summaries are
/// moved over so nothing is lost. Only ever Preflight's own folders.
fn migrate_old_folders(new: &Option<directories::ProjectDirs>) {
    let Some(new) = new else { return };
    let Some(old) = directories::ProjectDirs::from("", "", "StreamPreFlightCheck") else { return };
    let pairs = [(old.config_dir().to_path_buf(), new.config_dir().to_path_buf()), (old.data_dir().to_path_buf(), new.data_dir().to_path_buf())];
    for (from, to) in pairs {
        if from == to || !from.exists() || to.exists() {
            continue;
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Rename works on the same drive; copy as a fallback.
        if std::fs::rename(&from, &to).is_err() {
            let _ = copy_dir(&from, &to);
        }
    }
    // On Windows config and data share one parent folder; tidy the empty old one.
    if let Some(p) = old.config_dir().parent() {
        if p.to_string_lossy().contains("StreamPreFlightCheck") {
            let _ = std::fs::remove_dir(p);
        }
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t)?;
        }
    }
    Ok(())
}
