// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux: puts Preflight in the app menu with its icon, automatically, on
//! every launch. KDE and GNOME (on Wayland especially) take the window and
//! taskbar icon from this menu entry, matched by the app id. Only Preflight's
//! own entry and icon files are ever written; Uninstall deletes them.

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
    let Some(base) = data_home() else {
        return Vec::new();
    };
    let mut v = vec![base.join(format!("applications/{APP_ID}.desktop"))];
    for (s, _) in ICONS {
        v.push(base.join(format!("icons/hicolor/{s}x{s}/apps/{APP_ID}.png")));
    }
    if let Some(a) = autostart_file() {
        v.push(a);
    }
    v
}

/// ~/.config/autostart/stream-preflight.desktop: "Start at login".
#[cfg(not(windows))]
pub fn autostart_file() -> Option<std::path::PathBuf> {
    let cfg = match std::env::var("XDG_CONFIG_HOME") {
        Ok(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => crate::util::home_dir()?.join(".config"),
    };
    Some(cfg.join(format!("autostart/{APP_ID}.desktop")))
}

/// The program to start at login: the installed copy if there is one.
fn login_exe() -> Option<std::path::PathBuf> {
    crate::install::target()
        .filter(|t| t.exists())
        .or_else(|| std::env::current_exe().ok())
}

/// Turns "Start at login" on or off. Writes or removes Preflight's own
/// autostart entry only. `tray` starts it in the tray, with no window.
#[cfg(not(windows))]
pub fn set_autostart(on: bool, tray: bool) -> Result<(), String> {
    let f = autostart_file().ok_or("No home folder")?;
    if !on {
        let _ = std::fs::remove_file(&f);
        return Ok(());
    }
    let exe = login_exe().ok_or("Couldn't find the program")?;
    let es = exe.to_string_lossy();
    let quoted = if es.contains(' ') {
        format!("\"{es}\"")
    } else {
        es.to_string()
    };
    let args = if tray { " --tray" } else { "" };
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &f,
        format!("[Desktop Entry]\nType=Application\nName=Stream Preflight\nComment=Watches OBS so your streams are checked\nExec={quoted}{args}\nIcon={APP_ID}\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n"),
    )
    .map_err(|e| e.to_string())
}

/// Windows: the per-user Run entry (HKCU), Preflight's own value only.
#[cfg(windows)]
pub fn set_autostart(on: bool, tray: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut c = std::process::Command::new("reg");
    if on {
        let exe = login_exe().ok_or("Couldn't find the program")?;
        let val = format!("\"{}\"{}", exe.display(), if tray { " --tray" } else { "" });
        c.args([
            "add",
            key,
            "/v",
            "StreamPreflight",
            "/t",
            "REG_SZ",
            "/d",
            &val,
            "/f",
        ]);
    } else {
        c.args(["delete", key, "/v", "StreamPreflight", "/f"]);
    }
    let ok = c
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok || !on {
        Ok(())
    } else {
        Err("Windows didn't accept the startup entry".into())
    }
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
    let quoted = if es.contains(' ') {
        format!("\"{es}\"")
    } else {
        es.to_string()
    };
    let desktop = base.join(format!("applications/{APP_ID}.desktop"));
    if let Some(d) = desktop.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=Stream Preflight\nComment=Check your stream and recording before you go live\nExec={quoted}\nIcon={APP_ID}\nTerminal=false\nCategories=Utility;AudioVideo;\nStartupWMClass={APP_ID}\n"
        ),
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
    if let Some(t) = crate::install::target()
        && t.exists()
        && exe.canonicalize().ok() != t.canonicalize().ok()
    {
        return false;
    }
    let Some(base) = data_home() else {
        return false;
    };
    // The old name's menu entry and icons ("Stream Pre-Flight Check").
    let _ = std::fs::remove_file(base.join("applications/stream-preflight-check.desktop"));
    for (s, _) in ICONS {
        let _ = std::fs::remove_file(base.join(format!(
            "icons/hicolor/{s}x{s}/apps/stream-preflight-check.png"
        )));
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
    let quoted = if es.contains(' ') {
        format!("\"{es}\"")
    } else {
        es.to_string()
    };
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Stream Preflight\nComment=Check your stream and recording before you go live\nExec={quoted}\nIcon={APP_ID}\nTerminal=false\nCategories=Utility;AudioVideo;\nStartupWMClass={APP_ID}\n"
    );
    if std::fs::read_to_string(&desktop)
        .map(|t| t != text)
        .unwrap_or(true)
    {
        if let Some(d) = desktop.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if std::fs::write(&desktop, &text).is_ok() {
            log::info!(
                "App menu entry {}",
                if first {
                    "added"
                } else {
                    "updated (program moved)"
                }
            );
        }
    }
    first && desktop.exists()
}

#[cfg(windows)]
pub fn ensure() -> bool {
    false
}
