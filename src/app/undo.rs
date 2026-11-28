// SPDX-License-Identifier: GPL-3.0-or-later
//! Undo for resets and deletions: the thing is moved aside, the notice offers
//! Undo for 10 seconds, and only then is it removed for good. Anything left
//! over after a crash is removed at the next start.

use super::*;
use std::path::{Path, PathBuf};

/// How long Undo is offered.
pub(super) const UNDO_SECS: f64 = 10.0;

pub(super) enum Undo {
    /// History, past streams and preview frames, moved into one folder.
    History { stash: PathBuf },
    /// The OBS password that was forgotten.
    Password { pw: String, remembered: bool },
    /// The pairing that was removed.
    Pair {
        secret: Option<String>,
        name: String,
    },
    /// A test recording, renamed next to where it was.
    Recording {
        moved: PathBuf,
        rec: crate::config::PendingRecording,
    },
}

pub(super) struct Toast {
    pub msg: String,
    pub until: f64,
    pub undo: Option<Undo>,
    /// Another short message that arrived while Undo was still offered.
    pub note: Option<String>,
}

/// Recordings waiting to be removed get this name: ".<name>.preflight-deleted".
pub(super) fn aside_name(p: &Path) -> Option<PathBuf> {
    let name = p.file_name()?.to_string_lossy().to_string();
    Some(p.with_file_name(format!(".{name}.preflight-deleted")))
}

/// Only Preflight's own leftovers can be removed this way.
fn is_ours(p: &Path) -> bool {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.ends_with(".preflight-deleted")
        || (name.starts_with("undo-") && p.starts_with(&crate::util::dirs().data))
}

/// Removes something that was moved aside, for good.
pub(super) fn remove_for_good(p: &Path) {
    if !is_ours(p) {
        return;
    }
    if p.is_dir() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Moves every file in `dirs` and every file in `files` into a new folder.
pub(super) fn stash(files: &[PathBuf], dirs: &[&PathBuf]) -> Option<PathBuf> {
    let stash = crate::util::dirs()
        .data
        .join(format!("undo-{}", crate::util::now_stamp()));
    std::fs::create_dir_all(&stash).ok()?;
    for f in files.iter().filter(|f| f.exists()) {
        let _ = std::fs::rename(f, stash.join(f.file_name()?));
    }
    for (k, d) in dirs.iter().enumerate() {
        let to = stash.join(format!("dir{k}"));
        let _ = std::fs::create_dir_all(&to);
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let _ = std::fs::rename(e.path(), to.join(e.file_name()));
            }
        }
    }
    Some(stash)
}

/// Puts back what `stash` moved.
pub(super) fn unstash(stash: &Path, files: &[PathBuf], dirs: &[&PathBuf]) {
    for f in files {
        if let Some(n) = f.file_name() {
            let _ = std::fs::rename(stash.join(n), f);
        }
    }
    for (k, d) in dirs.iter().enumerate() {
        if let Ok(rd) = std::fs::read_dir(stash.join(format!("dir{k}"))) {
            for e in rd.flatten() {
                let _ = std::fs::rename(e.path(), d.join(e.file_name()));
            }
        }
    }
    let _ = std::fs::remove_dir_all(stash);
}

impl App {
    /// Shows a notice with an Undo button.
    pub(super) fn toast_undo(&mut self, ctx: &egui::Context, s: impl Into<String>, undo: Undo) {
        self.finish_toast();
        if let Some(p) = undo_path(&undo) {
            self.st.undo_trash.push(p.to_string_lossy().to_string());
            self.st.save();
        }
        self.toast = Some(Toast {
            msg: s.into(),
            until: ctx.input(|i| i.time) + UNDO_SECS,
            undo: Some(undo),
            note: None,
        });
    }

    /// The notice is gone: whatever was moved aside is removed for good.
    pub(super) fn finish_toast(&mut self) {
        if let Some(Toast { undo: Some(u), .. }) = self.toast.take()
            && let Some(p) = undo_path(&u)
        {
            remove_for_good(&p);
            let s = p.to_string_lossy().to_string();
            self.st.undo_trash.retain(|t| *t != s);
            self.st.save();
        }
    }

    /// Undo was clicked.
    pub(super) fn run_undo(&mut self, ctx: &egui::Context) {
        let Some(Toast { undo: Some(u), .. }) = self.toast.take() else {
            return;
        };
        if let Some(p) = undo_path(&u) {
            let s = p.to_string_lossy().to_string();
            self.st.undo_trash.retain(|t| *t != s);
        }
        match u {
            Undo::History { stash } => {
                let d = crate::util::dirs();
                unstash(
                    &stash,
                    &crate::history::History::files(),
                    &[&d.streams, &d.previews],
                );
                self.history = crate::history::History::load();
                self.streams = crate::streamwatch::past_streams();
                log::info!("History restored (undo)");
            }
            Undo::Password { pw, remembered } => {
                self.password = pw;
                self.st.obs_remember_password = remembered;
                if remembered {
                    crate::config::save_secret(&self.password);
                }
                log::info!("OBS password restored (undo)");
            }
            Undo::Pair { secret, name } => {
                if let Some(s) = secret {
                    crate::config::save_pair_secret(&s);
                }
                self.st.paired = true;
                self.st.paired_name = name;
                self.pair_status.clear();
                log::info!("Paired mode restored (undo)");
            }
            Undo::Recording { moved, rec } => {
                let back = PathBuf::from(&rec.path);
                if std::fs::rename(&moved, &back).is_ok() {
                    self.st.pending_recordings.push(rec);
                    log::info!("Test recording restored (undo)");
                }
            }
        }
        self.st.save();
        self.toast(ctx, "Undone");
    }
}

fn undo_path(u: &Undo) -> Option<PathBuf> {
    match u {
        Undo::History { stash } => Some(stash.clone()),
        Undo::Recording { moved, .. } => Some(moved.clone()),
        _ => None,
    }
}

/// At start: removes what an earlier session moved aside but couldn't finish.
pub(super) fn clean_leftovers(st: &mut crate::config::Settings) {
    if st.undo_trash.is_empty() {
        return;
    }
    for t in std::mem::take(&mut st.undo_trash) {
        remove_for_good(Path::new(&t));
    }
    st.save();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_and_unstash_round_trip() {
        let base = std::env::temp_dir().join(format!("pf-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (files_dir, streams) = (base.join("f"), base.join("streams"));
        std::fs::create_dir_all(&files_dir).unwrap();
        std::fs::create_dir_all(&streams).unwrap();
        let f = files_dir.join("history.json");
        std::fs::write(&f, "h").unwrap();
        std::fs::write(streams.join("s1.json"), "s").unwrap();
        let stash = base.join("undo-1");
        std::fs::create_dir_all(&stash).unwrap();
        // Same steps as stash(), into a folder of our choosing.
        std::fs::rename(&f, stash.join("history.json")).unwrap();
        std::fs::create_dir_all(stash.join("dir0")).unwrap();
        std::fs::rename(streams.join("s1.json"), stash.join("dir0/s1.json")).unwrap();
        assert!(!f.exists());
        unstash(&stash, std::slice::from_ref(&f), &[&streams]);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "h");
        assert_eq!(
            std::fs::read_to_string(streams.join("s1.json")).unwrap(),
            "s"
        );
        assert!(!stash.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_removes_its_own_leftovers() {
        let p = std::path::Path::new("/tmp/My Video.mkv");
        let a = aside_name(p).unwrap();
        assert_eq!(
            a,
            std::path::Path::new("/tmp/.My Video.mkv.preflight-deleted")
        );
        assert!(is_ours(&a));
        assert!(!is_ours(p));
        assert!(!is_ours(std::path::Path::new("/home/x/undo-notes")));
    }
}
