//! Linux: puts Preflight in the app menu with its icon, automatically, on
//! every launch. KDE and GNOME (on Wayland especially) take the window and
//! taskbar icon from this menu entry, matched by the app id. Only Preflight's
//! own entry and icon files are ever written; "Remove Preflight" deletes them.

pub const APP_ID: &str = "stream-preflight";

#[cfg(not(windows))]
const ICONS: [(u32, &[u8]); 8] = [
    (16, include_bytes!("../assets/icon/icon-16.png")),
    (24, include_bytes!("../assets/icon/icon-24.png")),
    (32, include_bytes!("../assets/icon/icon-32.png")),
    (48, include_bytes!("../assets/icon/icon-48.png")),
    (64, include_bytes!("../assets/icon/icon-64.png")),
    (128, include_bytes!("../assets/icon/icon-128.png")),
    (256, include_bytes!("../assets/icon/icon-256.png")),
    (512, include_bytes!("../assets/icon/icon-512.png")),
];

/// The files this module writes (for removal and for Help's folder list).
#[cfg(not(windows))]
pub fn files() -> Vec<std::path::PathBuf> {
    let Some(base) = data_home() else { return Vec::new() };
    let mut v = vec![base.join(format!("applications/{APP_ID}.desktop"))];
    for (s, _) in ICONS {
        v.push(base.join(format!("icons/hicolor/{s}x{s}/apps/{APP_ID}.png")));
    }
    v
}

#[cfg(windows)]
pub fn files() -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[cfg(not(windows))]
fn data_home() -> Option<std::path::PathBuf> {
    match std::env::var("XDG_DATA_HOME") {
        Ok(d) if !d.is_empty() => Some(std::path::PathBuf::from(d)),
        _ => crate::util::home_dir().map(|h| h.join(".local/share")),
    }
}

/// Writes the menu entry and icons for a program at `exe`.
#[cfg(not(windows))]
pub fn write_entry(exe: &std::path::Path) {
    let Some(base) = data_home() else { return };
    for (s, png) in ICONS {
        let p = base.join(format!("icons/hicolor/{s}x{s}/apps/{APP_ID}.png"));
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&p, png);
    }
    let es = exe.to_string_lossy();
    let quoted = if es.contains(' ') { format!("\"{es}\"") } else { es.to_string() };
    let desktop = base.join(format!("applications/{APP_ID}.desktop"));
    if let Some(d) = desktop.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(
        &desktop,
        format!("[Desktop Entry]\nType=Application\nName=Stream Preflight\nComment=Check your stream and recording before you go live\nExec={quoted}\nIcon={APP_ID}\nTerminal=false\nCategories=Utility;AudioVideo;\nStartupWMClass={APP_ID}\n"),
    );
}

/// Makes sure the menu entry and icon exist and point at this program.
/// Returns true the first time it adds the entry.
#[cfg(not(windows))]
pub fn ensure() -> bool {
    if std::env::var("FLATPAK_ID").is_ok() {
        return false;
    }
    // An AppImage moves its files on every run: point at the AppImage itself.
    let exe = match std::env::var("APPIMAGE") {
        Ok(a) if !a.is_empty() => std::path::PathBuf::from(a),
        _ => match std::env::current_exe() {
            Ok(e) => e,
            Err(_) => return false,
        },
    };
    let es = exe.to_string_lossy();
    if es.starts_with("/tmp/") || es.contains("/target/debug/") {
        return false;
    }
    // An installed copy owns the menu entry; a downloaded copy run from
    // somewhere else doesn't take it over.
    if let Some(t) = crate::install::target() {
        if t.exists() && exe.canonicalize().ok() != t.canonicalize().ok() {
            return false;
        }
    }
    let Some(base) = data_home() else { return false };
    // The old name's menu entry and icons ("Stream Pre-Flight Check").
    let _ = std::fs::remove_file(base.join("applications/stream-preflight-check.desktop"));
    for (s, _) in ICONS {
        let _ = std::fs::remove_file(base.join(format!("icons/hicolor/{s}x{s}/apps/stream-preflight-check.png")));
    }
    let desktop = base.join(format!("applications/{APP_ID}.desktop"));
    let first = !desktop.exists();
    for (s, png) in ICONS {
        let p = base.join(format!("icons/hicolor/{s}x{s}/apps/{APP_ID}.png"));
        if std::fs::read(&p).map(|b| b != png).unwrap_or(true) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, png);
        }
    }
    let quoted = if es.contains(' ') { format!("\"{es}\"") } else { es.to_string() };
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Stream Preflight\nComment=Check your stream and recording before you go live\nExec={quoted}\nIcon={APP_ID}\nTerminal=false\nCategories=Utility;AudioVideo;\nStartupWMClass={APP_ID}\n"
    );
    if std::fs::read_to_string(&desktop).map(|t| t != text).unwrap_or(true) {
        if let Some(d) = desktop.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if std::fs::write(&desktop, &text).is_ok() {
            log::info!("App menu entry {}", if first { "added" } else { "updated (program moved)" });
        }
    }
    first && desktop.exists()
}

#[cfg(windows)]
pub fn ensure() -> bool {
    false
}
