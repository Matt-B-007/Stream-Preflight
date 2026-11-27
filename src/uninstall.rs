// SPDX-License-Identifier: GPL-3.0-or-later
//! "Remove Preflight from this PC": deletes only Preflight's own folders,
//! the program file if asked, and test recordings the user ticks. Never
//! touches OBS, games or anything else.

use std::path::PathBuf;

/// What will be removed, in plain words, for the confirmation list.
pub fn own_folders() -> Vec<(String, PathBuf)> {
    let d = crate::util::dirs();
    let mut v = vec![(
        "Settings and the saved OBS password".to_string(),
        d.config.clone(),
    )];
    if d.data != d.config {
        v.push((
            "History, stream summaries, preview frames, logs and bug reports".to_string(),
            d.data.clone(),
        ));
    }
    v
}

/// The app menu entry and icons (Linux).
pub fn menu_files() -> Vec<PathBuf> {
    crate::desktop::files()
        .into_iter()
        .filter(|p| p.exists())
        .collect()
}

pub fn program_file() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// Removes everything chosen, then the app must close right away.
/// Returns an error message if something couldn't be removed.
pub fn run(recordings: &[PathBuf], remove_program: bool) -> Result<(), String> {
    run_with(recordings, remove_program, true)
}

/// Like `run`, but settings, history and logs are kept unless `delete_settings`.
/// The version kept for "Go back" is always removed with the program.
pub fn run_with(
    recordings: &[PathBuf],
    remove_program: bool,
    delete_settings: bool,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for r in recordings {
        if let Err(e) = std::fs::remove_file(r) {
            errors.push(format!("{}: {e}", r.display()));
        }
    }
    // Only ever Preflight's own folders and program file.
    let mut folders: Vec<PathBuf> = own_folders()
        .into_iter()
        .filter(|_| delete_settings)
        .map(|(_, p)| p)
        .filter(|p| {
            p.to_string_lossy()
                .to_lowercase()
                .contains("streampreflight")
        })
        .collect();
    if remove_program && !delete_settings {
        folders.push(crate::update::previous_dir());
    }
    let exe = if remove_program {
        program_file().filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase().contains("preflight"))
                .unwrap_or(false)
        })
    } else {
        None
    };
    log::info!("Removing Preflight's own files (program too: {remove_program})");
    // "Start at login" (the Linux entry is also in desktop::files()).
    let _ = crate::desktop::set_autostart(false, false);
    #[cfg(windows)]
    {
        crate::install::windows_unregister();
        // The install folder, when the program lives there and is being removed.
        let install_dir = exe
            .as_ref()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
            .filter(|d| d.ends_with("Stream Preflight"));
        // Windows keeps the running program and its open log file locked, so
        // a small hidden command finishes the job two seconds after we close.
        use std::os::windows::process::CommandExt;
        let mut cmd = String::from("timeout /t 2 /nobreak >nul");
        for f in &folders {
            cmd.push_str(&format!(" & rmdir /s /q \"{}\"", f.display()));
        }
        if let Some(e) = &exe {
            cmd.push_str(&format!(" & del /f /q \"{}\"", e.display()));
        }
        if let Some(d) = &install_dir {
            cmd.push_str(&format!(" & rmdir /q \"{}\"", d.display()));
        }
        std::process::Command::new("cmd")
            .args(["/c", &cmd])
            .creation_flags(0x0800_0000 | 0x0000_0008) // no window, detached
            .spawn()
            .map_err(|e| format!("couldn't start the clean-up: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        for f in menu_files() {
            let _ = std::fs::remove_file(f);
        }
        for f in &folders {
            if f.exists()
                && let Err(e) = std::fs::remove_dir_all(f)
            {
                errors.push(format!("{}: {e}", f.display()));
            }
        }
        if let Some(e) = &exe
            && let Err(err) = std::fs::remove_file(e)
        {
            errors.push(format!("{}: {err}", e.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
