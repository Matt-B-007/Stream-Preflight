// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared helpers: app folders, time, and privacy scrubbing.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const APP_NAME: &str = "Stream Preflight";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Label shown in the app while we are in private testing.
/// "2.1% of one core" from a share of the whole CPU (how Preflight measures itself).
pub fn one_core(pct_of_all: f32) -> String {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f32;
    let v = pct_of_all * cores;
    if v < 0.1 {
        "under 0.1% of one core".into()
    } else {
        format!("{v:.1}% of one core")
    }
}

pub const BUILD_LABEL: &str = "Beta 10";

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

/// The folder name on Linux, as in the other apps: ~/.config/stream-preflight,
/// ~/.local/share/stream-preflight and ~/.local/state/stream-preflight.
#[cfg(target_os = "linux")]
pub const LINUX_FOLDER: &str = "stream-preflight";

/// (settings, data, logs) folders.
#[cfg(target_os = "linux")]
fn base_folders() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let b = directories::BaseDirs::new()?;
    let state = b
        .state_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| b.home_dir().join(".local/state"));
    Some((
        b.config_dir().join(LINUX_FOLDER),
        b.data_dir().join(LINUX_FOLDER),
        state.join(LINUX_FOLDER),
    ))
}

#[cfg(not(target_os = "linux"))]
fn base_folders() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let base = directories::ProjectDirs::from("", "", "StreamPreflight");
    migrate_old_folders(&base);
    let p = base?;
    let data = p.data_dir().to_path_buf();
    Some((
        p.config_dir().to_path_buf(),
        data.clone(),
        data.join("logs"),
    ))
}

/// Folders used by beta.8 and older on Linux. After the move they're links to the
/// new folders, so going back to an older version still finds your settings.
#[cfg(target_os = "linux")]
pub fn legacy_folders() -> Vec<(PathBuf, PathBuf)> {
    let Some((config, data, _)) = base_folders() else {
        return Vec::new();
    };
    let Some(b) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let mut v = Vec::new();
    for old in ["streampreflight", "streampreflightcheck"] {
        v.push((b.config_dir().join(old), config.clone()));
        v.push((b.data_dir().join(old), data.clone()));
    }
    v
}

#[cfg(not(target_os = "linux"))]
pub fn legacy_folders() -> Vec<(PathBuf, PathBuf)> {
    Vec::new()
}

/// Moves an old folder to its new place. With `link`, the old path becomes a
/// link to the new folder. Nothing is ever deleted: if the move fails, it's
/// copied and the old folder stays.
pub fn move_folder(from: &std::path::Path, to: &std::path::Path, link: bool) -> bool {
    let is_link = std::fs::symlink_metadata(from)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if is_link || !from.is_dir() || to.exists() {
        return false;
    }
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let moved = std::fs::rename(from, to).is_ok();
    if !moved {
        if copy_dir(from, to).is_err() {
            return false;
        }
        return true;
    }
    #[cfg(unix)]
    if link {
        let _ = std::os::unix::fs::symlink(to, from);
    }
    #[cfg(not(unix))]
    let _ = link;
    true
}

/// Moves files from the old logs folder (inside the data folder) to the new one.
pub fn move_logs(old_logs: &std::path::Path, new_logs: &std::path::Path) {
    if old_logs == new_logs || !old_logs.is_dir() {
        return;
    }
    let _ = std::fs::create_dir_all(new_logs);
    if let Ok(rd) = std::fs::read_dir(old_logs) {
        for e in rd.flatten() {
            let t = new_logs.join(e.file_name());
            if !t.exists() && std::fs::rename(e.path(), &t).is_err() {
                let _ = std::fs::copy(e.path(), &t);
            }
        }
    }
    // Only removed when empty.
    let _ = std::fs::remove_dir(old_logs);
}

pub fn dirs() -> &'static Dirs {
    DIRS.get_or_init(|| {
        let (config, data, logs) = base_folders().unwrap_or_else(|| {
            let t = std::env::temp_dir().join("StreamPreflight");
            (t.join("config"), t.join("data"), t.join("logs"))
        });
        // Linux: beta.8 and older used ~/.config/streampreflight and
        // ~/.local/share/streampreflight (and logs inside the data folder).
        for (old, new) in legacy_folders() {
            let newest = old.ends_with("streampreflight");
            if move_folder(&old, &new, newest) {
                log::info!("Moved Preflight's folder to its new place");
            }
        }
        move_logs(&data.join("logs"), &logs);
        let reports = data.join("reports");
        let previews = data.join("previews");
        let tests = data.join("tests");
        let streams = data.join("streams");
        for d in [&config, &data, &logs, &reports, &previews, &streams, &tests] {
            let _ = std::fs::create_dir_all(d);
        }
        Dirs {
            config,
            data,
            logs,
            reports,
            previews,
            streams,
            tests,
        }
    })
}

pub fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// "Nov 13, 19:16" for a unix time.
pub fn fmt_local(t: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(t, 0)
        .single()
        .map(|d| d.format("%b %-d, %H:%M").to_string())
        .unwrap_or_default()
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

/// The PC's name, used for scrubbing.
pub fn host_name() -> Option<String> {
    #[cfg(unix)]
    let h = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()
        .map(|s| s.trim().to_string());
    #[cfg(not(unix))]
    let h = std::env::var("COMPUTERNAME").ok();
    h.filter(|s| !s.is_empty() && s != "localhost")
}

/// Removes things that identify the person or the PC from text that may end up
/// in a log line or a bug report. The same placeholders as the other apps:
/// - home folder → `~`, user name → `<user>`, PC name → `<host>`
/// - Windows-style paths too (`Z:\home\name`, `C:\Users\name`)
/// - Steam account IDs → `<steamid>` (game and app IDs stay)
/// - IP addresses → `<ip>`, MAC addresses → `<mac>` (version numbers stay)
/// - stream keys, tokens and passwords → `<secret>`
pub fn scrub(text: &str) -> String {
    scrub_with(text, home_dir(), user_name(), host_name())
}

/// `scrub` with the home folder, user name and PC name given (for tests).
pub fn scrub_with(
    text: &str,
    home: Option<PathBuf>,
    user: Option<String>,
    host: Option<String>,
) -> String {
    let mut out = text.to_string();
    if let Some(home) = home {
        let h = home.to_string_lossy().to_string();
        if h.len() > 3 {
            out = replace_home(&out, &h);
            out = replace_home(&out, &h.replace('\\', "/"));
        }
    }
    out = scrub_windows_paths(&out);
    out = scrub_home_folders(&out);
    if let Some(u) = user.filter(|u| u.len() >= 3) {
        out = replace_word(&out, &u, "<user>");
    }
    if let Some(h) = host.filter(|h| h.len() >= 3 && !common_host_name(h)) {
        out = replace_word(&out, &h, "<host>");
    }
    out = scrub_secrets(&out);
    out = scrub_emails(&out);
    out = scrub_steam_ids(&out);
    out = scrub_ipv4(&out);
    out = scrub_ipv6(&out);
    out = scrub_mac(&out);
    out
}

/// Replaces `word` where it isn't part of a longer word (any case).
fn replace_word(s: &str, word: &str, with: &str) -> String {
    let lower = s.to_lowercase();
    let w = word.to_lowercase();
    // Lower-casing can change byte lengths for some letters; skip then.
    if lower.len() != s.len() || w.len() != word.len() {
        return s.replace(word, with);
    }
    let is_word = |c: Option<char>| c.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(k) = lower[i..].find(&w) {
        let a = i + k;
        let b = a + w.len();
        let before = s[..a].chars().next_back();
        let after = s[b..].chars().next();
        out.push_str(&s[i..a]);
        if is_word(before) || is_word(after) {
            out.push_str(&s[a..b]);
        } else {
            out.push_str(with);
        }
        i = b;
    }
    out.push_str(&s[i..]);
    out
}

/// `/home/<name>` and `/Users/<name>` for any name (another account, a mounted drive).
/// Where a Windows or macOS user name ends. Those can hold spaces ("Sam Lee"),
/// so the name runs to the next path separator when one follows closely;
/// otherwise it ends at the first space.
fn spaced_name_end(tail: &str) -> usize {
    let hard = tail
        .find(|c: char| c == '\n' || "\"'`,;".contains(c))
        .unwrap_or(tail.len());
    let sep = tail.find(['/', '\\']).unwrap_or(usize::MAX);
    let ws = tail.find(char::is_whitespace).unwrap_or(tail.len());
    if sep <= hard && sep <= 40 && tail[..sep].matches(' ').count() <= 2 {
        sep
    } else {
        ws.min(hard).min(sep)
    }
}

fn scrub_home_folders(s: &str) -> String {
    let mut out = s.to_string();
    for prefix in ["/home/", "/Users/"] {
        let mut res = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(k) = rest.find(prefix) {
            let (head, tail) = rest.split_at(k + prefix.len());
            res.push_str(head);
            let end = if prefix == "/Users/" {
                spaced_name_end(tail)
            } else {
                tail.find(|c: char| {
                    c == '/' || c == '\\' || c.is_whitespace() || "\"'`,;:)]}".contains(c)
                })
                .unwrap_or(tail.len())
            };
            let name = &tail[..end];
            if name.is_empty() || name.starts_with('<') {
                res.push_str(name);
            } else {
                res.push_str("<user>");
            }
            rest = &tail[end..];
        }
        res.push_str(rest);
        out = res;
    }
    out
}

/// Wine/Proton and Windows paths: `Z:\\home\\name\\…` → `~\\…`, `C:\\Users\\name` → `C:\\Users\\<user>`.
fn scrub_windows_paths(s: &str) -> String {
    let b = s.as_bytes();
    let stop = |c: char| c == '\\' || c == '/' || c.is_whitespace() || "\"'`,;".contains(c);
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let drive = b[i].is_ascii_alphabetic()
            && i + 2 < b.len()
            && b[i + 1] == b':'
            && (b[i + 2] == b'\\' || b[i + 2] == b'/')
            && (i == 0 || !b[i - 1].is_ascii_alphanumeric());
        if drive {
            let sep = b[i + 2] as char;
            let rest = &s[i + 3..];
            let mut done = false;
            for (dir, is_home) in [("home", true), ("users", false)] {
                let matches = rest.len() > dir.len()
                    && rest[..dir.len()].eq_ignore_ascii_case(dir)
                    && rest[dir.len()..].starts_with(sep);
                if !matches {
                    continue;
                }
                let after = &rest[dir.len() + 1..];
                let end = if is_home {
                    after.find(stop).unwrap_or(after.len())
                } else {
                    spaced_name_end(after)
                };
                if end == 0 || after.starts_with('<') {
                    continue;
                }
                if is_home {
                    out.push('~');
                } else {
                    out.push_str(&s[i..i + 3 + dir.len() + 1]);
                    out.push_str("<user>");
                }
                i += 3 + dir.len() + 1 + end;
                done = true;
                break;
            }
            if done {
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
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
        if (s[i..].starts_with("userdata/") || s[i..].starts_with("userdata\\"))
            && i + 9 < b.len()
            && b[i + 9].is_ascii_digit()
        {
            let mut j = i + 9;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            out.push_str(&s[i..i + 9]);
            out.push_str("<steamid>");
            i = j;
            continue;
        }
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - i == 17 && s[i..].starts_with("7656119") {
                out.push_str("<steamid>");
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
    let keys = [
        "password",
        "passwd",
        "token",
        "stream_key",
        "streamkey",
        "stream key",
        "api_key",
        "apikey",
        "secret",
        "authentication",
        "authorization",
    ];
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
            lines.push(format!("{}{} <secret>", &line[..c], &line[c..c + 1]));
        } else {
            lines.push(scrub_inline_secrets(line));
        }
    }
    let mut r = lines.join("\n");
    if s.ends_with('\n') {
        r.push('\n');
    }
    r
}

/// Secrets inside a line: Twitch-style `live_…` keys and `Bearer …` tokens.
fn scrub_inline_secrets(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let live = rest.find("live_");
        let bearer = rest.find("Bearer ");
        let (k, skip) = match (live, bearer) {
            (Some(a), Some(b)) if b < a => (b, 7),
            (Some(a), _) => (a, 0),
            (None, Some(b)) => (b, 7),
            (None, None) => break,
        };
        let start = k + skip;
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || "_-.=+/".contains(c)))
            .unwrap_or(tail.len());
        let word_start = k == 0 || !rest[..k].ends_with(|c: char| c.is_ascii_alphanumeric());
        // Long enough to be a real key, not the word "live_" in a setting name.
        if word_start && end >= 16 {
            out.push_str(&rest[..start]);
            out.push_str("<secret>");
        } else {
            out.push_str(&rest[..start + end.max(1)]);
        }
        rest = &rest[start + end.max(1)..];
    }
    out.push_str(rest);
    out
}

/// Does the text end with a word that names a program or a version, so the
/// number after it is a version and not an address? Anything else stays an address.
fn after_version_word(before: &str) -> bool {
    const WORDS: [&str; 30] = [
        "version",
        "ver",
        "v",
        "release",
        "build",
        "driver",
        "drivers",
        "kernel",
        "obs",
        "nvidia",
        "amd",
        "intel",
        "mesa",
        "vulkan",
        "opengl",
        "proton",
        "wine",
        "steam",
        "runtime",
        "pipewire",
        "wireplumber",
        "glibc",
        "qt",
        "ffmpeg",
        "plugin",
        "websocket",
        "firmware",
        "bios",
        "directx",
        "dxvk",
    ];
    let word = before
        .trim_end_matches([' ', ':', '=', '(', '\t'])
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .next()
        .unwrap_or("")
        .to_lowercase();
    !word.is_empty() && (WORDS.contains(&word.as_str()) || word.ends_with("version"))
}

/// PC names that are also ordinary words in a report (the distro's name, "desktop"…).
/// Replacing those would garble the report without hiding anything.
fn common_host_name(h: &str) -> bool {
    const COMMON: [&str; 26] = [
        "cachyos",
        "arch",
        "archlinux",
        "linux",
        "localhost",
        "desktop",
        "laptop",
        "pc",
        "computer",
        "gaming",
        "home",
        "steamdeck",
        "ubuntu",
        "fedora",
        "debian",
        "nixos",
        "manjaro",
        "endeavouros",
        "bazzite",
        "nobara",
        "popos",
        "pop-os",
        "mint",
        "garuda",
        "opensuse",
        "user",
    ];
    let h = h.to_lowercase();
    COMMON.contains(&h.as_str())
        || std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("ID=")
                        .map(|v| v.trim_matches('"').to_lowercase())
                })
            })
            .is_some_and(|id| id == h)
}

/// Replaces the home folder where it's a whole path part: "/home/sam/x" → "~/x",
/// but "/home/sam2" and "/var/home/sam" are left for the next step.
fn replace_home(s: &str, home: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find(home) {
        let before = rest[..k].chars().next_back();
        let after = rest[k + home.len()..].chars().next();
        let start_ok = !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
        let end_ok =
            !after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
        out.push_str(&rest[..k]);
        out.push_str(if start_ok && end_ok { "~" } else { home });
        rest = &rest[k + home.len()..];
    }
    out.push_str(rest);
    out
}

/// Email addresses → `<email>`.
fn scrub_emails(s: &str) -> String {
    let ok = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut copied = 0;
    while i < chars.len() {
        if chars[i] == '@' {
            let mut a = i;
            while a > copied && ok(chars[a - 1]) {
                a -= 1;
            }
            let mut b = i + 1;
            while b < chars.len() && ok(chars[b]) {
                b += 1;
            }
            let domain: String = chars[i + 1..b].iter().collect();
            let domain = domain.trim_end_matches('.');
            let tld = domain.rsplit('.').next().unwrap_or("");
            let is_email = a < i
                && domain.contains('.')
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic());
            if is_email {
                out.extend(&chars[copied..a]);
                out.push_str("<email>");
                let end = i + 1 + domain.chars().count();
                copied = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.extend(&chars[copied..]);
    out
}

fn scrub_ipv4(s: &str) -> String {
    let bytes: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        // try to match d{1,3}(.d{1,3}){3}
        // After a letter, '.', '=', '-' or '_' it's a version (v1.2.3.4, ==0.10.0.1, py-0.11.0.1).
        if bytes[i].is_ascii_digit()
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && !".=-_".contains(bytes[i - 1]))
        {
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
                let n: u32 = bytes[start..j]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(999);
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
                // 0.x.x.x isn't a real address, so it's a version number, and so
                // is a number right after a program or version word ("obs 31.0.2.1").
                let before: String = bytes[..i].iter().collect();
                if ip == "127.0.0.1" || ip.starts_with("0.") || after_version_word(&before) {
                    out.push_str(&ip);
                } else {
                    out.push_str("<ip>");
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

/// IPv6 addresses: hex groups with "::" or at least 5 colons, standing on their own.
/// Times (12:34:56), Rust paths (fleet::add) and MAC addresses don't match.
fn scrub_ipv6(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let part = |c: char| c.is_ascii_hexdigit() || c == ':';
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if part(chars[i]) && (i == 0 || !(word(chars[i - 1]) || chars[i - 1] == ':')) {
            let mut j = i;
            while j < chars.len() && part(chars[j]) {
                j += 1;
            }
            let bounded = j == chars.len() || !word(chars[j]);
            let t: String = chars[i..j].iter().collect();
            let colons = t.matches(':').count();
            let groups: Vec<&str> = t.split(':').collect();
            let groups_ok = groups.iter().all(|g| g.len() <= 4);
            let filled = groups.iter().filter(|g| !g.is_empty()).count();
            let mac = colons == 5 && groups.iter().all(|g| g.len() == 2);
            // A real address has a digit and at least 4 hex characters; written
            // in full it has exactly 8 groups. "a::b" and "1920:1080:60:1:2:3" don't.
            let hex = t.chars().filter(|c| *c != ':').count();
            let digit = t.chars().any(|c| c.is_ascii_digit());
            let looks = groups_ok
                && !mac
                && t != "::1"
                && digit
                && hex >= 4
                && ((t.contains("::") && filled >= 2) || (colons == 7 && filled == 8));
            if bounded && looks {
                out.push_str("<ip>");
                i = j;
                continue;
            }
            out.extend(&chars[i..j]);
            i = j;
            continue;
        }
        out.push(chars[i]);
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
                out.push_str("<mac>");
                i += 17;
                continue;
            }
        }
        // Cisco style: aabb.ccdd.eeff (with at least one letter, so it isn't a number).
        if i + 14 <= chars.len() && (i == 0 || !chars[i - 1].is_ascii_alphanumeric()) {
            let cand = &chars[i..i + 14];
            let shape = (0..14).all(|k| {
                if k == 4 || k == 9 {
                    cand[k] == '.'
                } else {
                    cand[k].is_ascii_hexdigit()
                }
            });
            let ends = i + 14 == chars.len() || !chars[i + 14].is_ascii_alphanumeric();
            if shape && ends && cand.iter().any(|c| c.is_ascii_alphabetic()) {
                out.push_str("<mac>");
                i += 14;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Starts a helper program (a file manager, a text editor) and collects it
/// when it ends, so no finished-but-uncollected process is left behind.
pub fn run_and_forget(cmd: &mut std::process::Command) {
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
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

/// The app used to be called "Stream Pre-Flight Check". The first time the new
/// name runs, its settings, saved password, history and stream summaries are
/// moved over so nothing is lost. Only ever Preflight's own folders.
#[cfg(not(target_os = "linux"))]
fn migrate_old_folders(new: &Option<directories::ProjectDirs>) {
    let Some(new) = new else { return };
    let Some(old) = directories::ProjectDirs::from("", "", "StreamPreFlightCheck") else {
        return;
    };
    let pairs = [
        (
            old.config_dir().to_path_buf(),
            new.config_dir().to_path_buf(),
        ),
        (old.data_dir().to_path_buf(), new.data_dir().to_path_buf()),
    ];
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
    if let Some(p) = old.config_dir().parent()
        && p.to_string_lossy().contains("StreamPreFlightCheck")
    {
        let _ = std::fs::remove_dir(p);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrubs_ip_and_mac_and_secrets() {
        let s = "connect 192.168.1.20 ok; mac aa:bb:cc:dd:ee:ff\npassword=hunter2\n\"stream_key\": \"live_abc\"\n\"key\": \"FrameRateLimit\"\nversion 1.2.3";
        let r = scrub(s);
        assert!(r.contains("<ip>"));
        assert!(r.contains("<mac>"));
        assert!(r.contains("password= <secret>"));
        assert!(!r.contains("hunter2"));
        assert!(r.contains("version 1.2.3"));
        assert!(!r.contains("live_abc"));
        assert!(r.contains("FrameRateLimit"));
    }

    fn sc(s: &str) -> String {
        scrub_with(
            s,
            Some(PathBuf::from("/home/jordan")),
            Some("jordan".into()),
            Some("battlestation".into()),
        )
    }

    #[test]
    fn standard_placeholders() {
        assert_eq!(sc("/home/jordan/.config/obs"), "~/.config/obs");
        assert_eq!(
            sc("signed in as jordan on battlestation"),
            "signed in as <user> on <host>"
        );
        // Part of a longer word stays.
        assert_eq!(sc("jordansoft battlestations"), "jordansoft battlestations");
        assert_eq!(sc("/home/guest/Videos"), "/home/<user>/Videos");
        assert_eq!(sc("/Users/sam/Movies"), "/Users/<user>/Movies");
        assert_eq!(
            sc("Z:\\home\\jordan\\Games\\x.ini and Z:\\home\\other\\y"),
            "~\\Games\\x.ini and ~\\y"
        );
        assert_eq!(
            sc("C:\\Users\\steamuser\\AppData"),
            "C:\\Users\\<user>\\AppData"
        );
        assert_eq!(sc("D:\\Games\\Cyberpunk"), "D:\\Games\\Cyberpunk");
        assert_eq!(sc("id 76561198012345678 app 730"), "id <steamid> app 730");
        assert_eq!(sc("userdata/12345678/730"), "userdata/<steamid>/730");
        assert_eq!(
            sc("from 10.0.0.5 and fe80::1ff:fe23:4567:890a"),
            "from <ip> and <ip>"
        );
        assert_eq!(sc("mac 3c:7c:3f:aa:01:02"), "mac <mac>");
        assert_eq!(sc("Authorization: Bearer abc"), "Authorization: <secret>");
        assert_eq!(
            sc("rtmp://live.twitch.tv/app/live_123456789_AbCdEfGhIjKlMnOp ok"),
            "rtmp://live.twitch.tv/app/<secret> ok"
        );
    }

    #[test]
    fn beta10_redaction_fixes() {
        // Versions after a program word stay; bare addresses still go.
        assert_eq!(sc("obs 31.0.2.1 from 31.0.2.1"), "obs 31.0.2.1 from <ip>");
        assert_eq!(sc("OBS version: 30.1.2.3"), "OBS version: 30.1.2.3");
        assert_eq!(sc("ws://192.168.1.20:4455"), "ws://<ip>:4455");
        // A PC named after the distro doesn't garble the report.
        let f = |s: &str| scrub_with(s, None, None, Some("cachyos".into()));
        assert_eq!(
            f("CachyOS kernel 6.12-cachyos"),
            "CachyOS kernel 6.12-cachyos"
        );
        // Names with spaces, and home as part of a longer name.
        assert_eq!(
            sc("C:\\Users\\Sam Lee\\Documents"),
            "C:\\Users\\<user>\\Documents"
        );
        assert_eq!(sc("/mnt/c/Users/Sam Lee/x"), "/mnt/c/Users/<user>/x");
        assert_eq!(sc("/Users/sam and more"), "/Users/<user> and more");
        assert_eq!(sc("/home/jordan2/x"), "/home/<user>/x");
        assert_eq!(sc("/var/home/jordan/x"), "/var/home/<user>/x");
        assert_eq!(sc("/home/jordan"), "~");
        // Code and ratios aren't addresses; real IPv6 still is.
        assert_eq!(
            sc("a::b add::fee 1920:1080:60:1:2:3"),
            "a::b add::fee 1920:1080:60:1:2:3"
        );
        assert_eq!(
            sc("2001:db8::1 and 2001:0db8:85a3:0000:0000:8a2e:0370:7334"),
            "<ip> and <ip>"
        );
        // Emails and Cisco-style MACs.
        assert_eq!(sc("mail me@example.com, ok"), "mail <email>, ok");
        assert_eq!(sc("jordan@battlestation:~$"), "<user>@<host>:~$");
        assert_eq!(sc("mac aabb.ccdd.eeff"), "mac <mac>");
        assert_eq!(sc("size 1234.5678.9012"), "size 1234.5678.9012");
    }

    #[test]
    fn leaves_versions_times_and_code_alone() {
        for keep in [
            "shellcheck-py 0.11.0.1",
            "shellcheck-py==0.11.0.1",
            "version v1.2.3.4",
            "build 0.10.0.2",
            "at 14:22:01.123",
            "fleet::add and crate::util::scrub",
            "ratio 16:9",
            "localhost 127.0.0.1",
            "live_view = true",
            "\"key\": \"FrameRateLimit\"",
            "Steam app 1172470",
        ] {
            assert_eq!(sc(keep), keep);
        }
    }
}
