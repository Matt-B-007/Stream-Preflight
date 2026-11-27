// SPDX-License-Identifier: GPL-3.0-or-later
//! Installing Preflight for this user (no admin rights needed), and putting
//! it where the menu and, on Windows, "Installed apps" can find it.
//!
//! Linux:   ~/.local/bin/stream-preflight, plus the app-menu entry and icon.
//! Windows: %LOCALAPPDATA%\Programs\Stream Preflight\stream-preflight.exe,
//!          a Start menu shortcut, and an entry in Settings › Apps so it can
//!          be uninstalled from there.
//! Only Preflight's own files are written.

use std::path::PathBuf;

/// Where the installed program lives.
pub fn target() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("LOCALAPPDATA").ok().map(|d| {
            PathBuf::from(d)
                .join("Programs")
                .join("Stream Preflight")
                .join("stream-preflight.exe")
        })
    }
    #[cfg(not(windows))]
    {
        crate::util::home_dir().map(|h| h.join(".local/bin/stream-preflight"))
    }
}

/// The old name's program file, from before the rename.
pub fn old_program() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(windows))]
    {
        crate::util::home_dir()
            .map(|h| h.join(".local/bin/stream-preflight-check"))
            .filter(|p| p.exists())
    }
}

/// Running from the installed place already?
pub fn is_installed() -> bool {
    match (
        std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok()),
        target().and_then(|p| p.canonicalize().ok()),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Copies this program to the install place and sets up the menu entry.
/// Returns the installed path.
pub fn install(remove_old: bool) -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let to = target().ok_or("Couldn't find your home folder.")?;
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("Couldn't create {}: {e}", d.display()))?;
    }
    if me.canonicalize().ok() != to.canonicalize().ok() {
        // Copy next to it first, then swap in, so a half-written file is never left.
        let tmp = to.with_extension("new");
        std::fs::copy(&me, &tmp).map_err(|e| format!("Couldn't copy the program: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
        }
        #[cfg(windows)]
        if to.exists() {
            let _ = std::fs::rename(&to, to.with_extension("old"));
        }
        std::fs::rename(&tmp, &to)
            .map_err(|e| format!("Couldn't put the program in place: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        crate::desktop::write_entry(&to);
        if remove_old && let Some(o) = old_program() {
            let _ = std::fs::remove_file(o);
        }
    }
    #[cfg(windows)]
    {
        let _ = remove_old;
        windows_shortcut(&to);
        windows_register(&to);
    }
    log::info!("Installed for this user");
    Ok(to)
}

/// Starts the installed copy (after installing or updating).
/// Starts `p` as the replacement for this window (after an update or Go back).
/// It waits for this window to close instead of bringing it forward.
pub fn launch(p: &std::path::Path) -> bool {
    std::process::Command::new(p)
        .env(crate::instance::RESTART_ENV, "1")
        .spawn()
        .is_ok()
}

#[cfg(windows)]
pub fn start_menu_shortcut() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|d| {
        PathBuf::from(d).join("Microsoft\\Windows\\Start Menu\\Programs\\Stream Preflight.lnk")
    })
}

#[cfg(windows)]
fn windows_shortcut(exe: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let Some(lnk) = start_menu_shortcut() else {
        return;
    };
    let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Description='Check your game, OBS and stream before you go live';$s.Save()",
        q(&lnk),
        q(exe),
        q(exe.parent().unwrap_or(exe)),
        q(exe)
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000)
        .status();
}

#[cfg(windows)]
const UNINSTALL_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\StreamPreflight";

#[cfg(windows)]
fn windows_register(exe: &std::path::Path) {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok((k, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY) else {
        return;
    };
    let e = exe.display().to_string();
    let _ = k.set_value("DisplayName", &crate::util::APP_NAME);
    let _ = k.set_value("DisplayVersion", &crate::util::VERSION);
    let _ = k.set_value("Publisher", &"Stream Preflight");
    let _ = k.set_value("DisplayIcon", &format!("{e},0"));
    let _ = k.set_value("UninstallString", &format!("\"{e}\" --uninstall"));
    let _ = k.set_value(
        "InstallLocation",
        &exe.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );
    let _ = k.set_value(
        "URLInfoAbout",
        &"https://github.com/Matt-B-007/Stream-Preflight",
    );
    let _ = k.set_value("NoModify", &1u32);
    let _ = k.set_value("NoRepair", &1u32);
}

/// Windows: the Start menu shortcut and the Installed apps entry (for uninstall).
#[cfg(windows)]
pub fn windows_unregister() {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
    if let Some(l) = start_menu_shortcut() {
        let _ = std::fs::remove_file(l);
    }
}

/// Windows: keeps the Installed apps entry's version current after an update.
#[cfg(windows)]
pub fn refresh_version() {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(UNINSTALL_KEY, winreg::enums::KEY_SET_VALUE)
    {
        let _ = k.set_value("DisplayVersion", &crate::util::VERSION);
    }
}

/// Removes the previous program file left by an update (Windows can't
/// replace a running .exe, so it was renamed to .old).
pub fn tidy_after_update() {
    if let Ok(me) = std::env::current_exe() {
        let old = me.with_extension("old");
        if old.exists() {
            let _ = std::fs::remove_file(old);
        }
    }
    #[cfg(windows)]
    if is_installed() {
        refresh_version();
    }
}
