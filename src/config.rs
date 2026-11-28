// SPDX-License-Identifier: GPL-3.0-or-later
//! User settings. Stored as JSON with a format version so later updates
//! can migrate them without losing anything.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SETTINGS_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    Competitive,
    Balanced,
    Cinematic,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Platform::Twitch => "Twitch",
            Platform::YouTube => "YouTube",
            Platform::Kick => "Kick",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NeverSuggest {
    pub lower_below_1080p: bool,
    pub disable_ray_tracing: bool,
    pub cap_below_refresh: bool,
    pub upscaling: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRecording {
    pub path: String,
    pub size: u64,
    pub created: i64,
    /// Unix time after which we remind again ("Remind me later").
    pub remind_after: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChangeNote {
    pub time: i64,
    pub text: String,
}

/// A fix option the user asked never to be suggested again.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HiddenOption {
    /// "issue_id:option_id"
    pub key: String,
    pub label: String,
}

/// What the last finished pre-flight looked like, so a returning user
/// with nothing changed can skip straight to the final checks.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LastPreflight {
    pub time: i64,
    pub signature: String,
    pub score: u8,
    pub game: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub settings_version: u32,
    pub setup_done: bool,
    pub style: Style,
    /// 0 = smooth gameplay, 100 = sharp stream
    pub smooth_vs_stream: u8,
    /// 0 = higher FPS, 100 = better-looking game
    pub fps_vs_visuals: u8,
    pub never: NeverSuggest,
    pub platform: Platform,
    pub monitor_hz: u32,
    pub obs_host: String,
    pub obs_port: u16,
    pub obs_remember_password: bool,
    pub advanced_view: bool,
    pub text_scale: f32,
    pub notify_test_done: bool,
    pub remind_delete_recording: bool,
    pub updates_auto: bool,
    pub updates_beta: bool,
    pub skipped_version: String,
    pub pending_recordings: Vec<PendingRecording>,
    /// Things moved aside for Undo, removed for good when the notice goes
    /// (or at the next start, if Preflight closed first).
    pub undo_trash: Vec<String>,
    /// A new install: show the tour once setup is done.
    pub tour_pending: bool,
    /// Settings › Appearance.
    pub theme: crate::theme::ThemeChoice,
    pub backdrop_grid: bool,
    pub reduce_motion: bool,
    /// Updates: if this PC's DNS can't find GitHub, ask Cloudflare or Google.
    pub dns_fallback: bool,
    /// The version whose "What's new" was last shown.
    pub seen_version: String,
    pub network_test_on_scan: bool,
    pub hidden_options: Vec<HiddenOption>,
    /// Issues the user chose to keep as they are (still listed, not counted in the score).
    pub kept_issues: Vec<String>,
    /// Go-live checks the user marked "It's fine" (mic, capture, webcam...).
    pub go_kept: Vec<String>,
    /// Test history for "what changed": change notes the user made via fixes.
    pub change_log: Vec<ChangeNote>,
    pub monitor_name: String,
    pub monitor_auto: bool,
    pub last_preflight: Option<LastPreflight>,
    /// The user added ?bandwidthtest=true to their stream key for a test and
    /// hasn't confirmed removing it yet.
    pub bandwidth_test_pending: bool,
    /// "About what FPS do you get?" answers, by game name.
    pub game_fps_said: std::collections::BTreeMap<String, u32>,
    /// Last results per game, so switching games shows that game's results.
    pub game_memory: std::collections::BTreeMap<String, GameMemory>,
    /// Delete test recordings once Preflight is done with them (off unless the user turns it on).
    pub auto_delete_recordings: bool,
    /// "Keep as is" choices for game fixes, per game (OBS and PC choices stay in kept_issues).
    pub kept_by_game: std::collections::BTreeMap<String, Vec<String>>,
    /// Program names of games without a profile seen in tests.
    pub auto_games: Vec<String>,
    /// The style picked for each game.
    pub style_by_game: std::collections::BTreeMap<String, Style>,
    /// The last kind of test picked: "full", "stream_record" or "quick".
    pub last_test: String,
    /// Where the user streams ("Twitch", "YouTube", "Kick"); empty = just `platform`.
    pub platforms: Vec<String>,
    /// How they reach several platforms: "plugin" (one OBS output each) or "service" (a restream service).
    pub multistream_how: String,
    /// Measure stream + recording together during the next real stream (post-flight).
    pub measure_next_stream: bool,
    /// Style for a game seen for the first time.
    pub default_style: Style,
    /// Minimizing the window sends Preflight to the system tray.
    pub minimize_to_tray: bool,
    /// The window's close button hides to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Show desktop notifications from the tray (test done, stream ended, flag reminder).
    pub tray_notifications: bool,
    /// The sidebar shows icons only.
    pub sidebar_collapsed: bool,
    /// One-time hints already shown ("first_test", "first_live", "rail"...).
    pub hints_seen: Vec<String>,
    /// Open the In-flight page by itself when a real stream starts.
    pub inflight_auto: bool,
    /// The user chose the beta setting themselves (beta builds turn betas on until then).
    pub updates_beta_set: bool,
    /// When updates were last checked (unix time), for the daily automatic check.
    pub last_update_check: i64,
    /// Which computer this is: "one" (everything here), "gaming" or "streaming".
    pub pc_role: String,
    /// "Looks like two PCs" was answered (so it's not asked again).
    pub two_pc_asked: bool,
    /// How fast OBS's recording grew during the last stream that recorded (kbps),
    /// so "recording space" uses a real number instead of a guess.
    pub rec_kbps_seen: Option<f32>,
    /// Start Preflight when you log in (off by default), and whether it starts in the tray.
    pub start_at_login: bool,
    pub start_in_tray: bool,
    /// When you go live while Preflight is in the tray: open the Live page (true)
    /// or stay in the tray and just change the icon (false).
    pub live_opens_window: bool,
    /// Fixes left at the last check, so the tray icon can say "things to fix".
    pub last_fix_count: u32,
    /// Paired mode: linked with Preflight on the other PC over the home network.
    pub paired: bool,
    /// The other PC's name as it introduced itself (no address kept here).
    pub paired_name: String,
    /// The other PC's monitor refresh rate, from paired mode (for capture judder).
    #[serde(skip)]
    pub other_monitor_hz: u32,
    /// Worked out from every tested game (not a user choice): the fastest
    /// motion, and whether any game kept the GPU or encoder busy.
    #[serde(skip)]
    pub fleet_motion: String,
    #[serde(skip)]
    pub fleet_busy: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameMemory {
    pub time: i64,
    pub smoothness: u8,
    pub setup: u8,
    pub verdict: String,
    /// Fix titles still open after that test.
    pub open: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            setup_done: false,
            style: Style::Balanced,
            smooth_vs_stream: 40,
            fps_vs_visuals: 45,
            never: NeverSuggest::default(),
            platform: Platform::Twitch,
            monitor_hz: 144,
            obs_host: "127.0.0.1".into(),
            obs_port: 4455,
            obs_remember_password: false,
            advanced_view: false,
            text_scale: 1.0,
            notify_test_done: true,
            remind_delete_recording: true,
            updates_auto: true,
            updates_beta: false,
            skipped_version: String::new(),
            pending_recordings: Vec::new(),
            undo_trash: Vec::new(),
            tour_pending: false,
            theme: crate::theme::ThemeChoice::System,
            backdrop_grid: true,
            reduce_motion: false,
            dns_fallback: true,
            seen_version: String::new(),
            network_test_on_scan: true,
            hidden_options: Vec::new(),
            kept_issues: Vec::new(),
            go_kept: Vec::new(),
            change_log: Vec::new(),
            monitor_name: String::new(),
            monitor_auto: true,
            last_preflight: None,
            bandwidth_test_pending: false,
            game_fps_said: Default::default(),
            game_memory: Default::default(),
            auto_delete_recordings: false,
            kept_by_game: Default::default(),
            auto_games: Vec::new(),
            style_by_game: Default::default(),
            last_test: "full".into(),
            platforms: Vec::new(),
            multistream_how: String::new(),
            measure_next_stream: false,
            default_style: Style::Balanced,
            minimize_to_tray: true,
            close_to_tray: false,
            tray_notifications: true,
            last_update_check: 0,
            sidebar_collapsed: false,
            hints_seen: Vec::new(),
            inflight_auto: true,
            updates_beta_set: false,
            pc_role: "one".into(),
            two_pc_asked: false,
            rec_kbps_seen: None,
            start_at_login: false,
            start_in_tray: true,
            live_opens_window: false,
            last_fix_count: 0,
            paired: false,
            paired_name: String::new(),
            other_monitor_hz: 0,
            fleet_motion: String::new(),
            fleet_busy: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::util::dirs().config.join("settings.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => match serde_json::from_str::<Settings>(&s) {
                Ok(mut v) => {
                    v.migrate();
                    v
                }
                Err(e) => {
                    let _ = std::fs::copy(&p, p.with_extension("broken.json"));
                    match newest_good_backup(&backup_dir()) {
                        Some((mut v, name)) => {
                            log::warn!(
                                "Settings file could not be read ({e}); kept a copy and used the backup {name}"
                            );
                            v.migrate();
                            v
                        }
                        None => {
                            log::warn!(
                                "Settings file could not be read ({e}); kept a copy and started fresh"
                            );
                            Self::default()
                        }
                    }
                }
            },
            Err(_) => Self::default(),
        }
    }

    fn migrate(&mut self) {
        if self.settings_version < SETTINGS_VERSION {
            log::info!(
                "Migrating settings from format {} to {}",
                self.settings_version,
                SETTINGS_VERSION
            );
            self.settings_version = SETTINGS_VERSION;
        }
        self.text_scale = self.text_scale.clamp(0.8, 1.6);
        // Earlier versions could save a web view or desktop program as a "game".
        let before = self.game_memory.len() + self.style_by_game.len() + self.kept_by_game.len();
        self.game_memory
            .retain(|g, _| !crate::hardware::is_not_game(g));
        self.style_by_game
            .retain(|g, _| !crate::hardware::is_not_game(g));
        self.kept_by_game
            .retain(|g, _| !crate::hardware::is_not_game(g));
        if self.game_memory.len() + self.style_by_game.len() + self.kept_by_game.len() < before {
            log::info!("Removed saved entries for programs that aren't games");
        }
    }

    /// Saves through a temporary file and a rename, so a crash or power cut
    /// never leaves a half-written file. The file being replaced is backed up
    /// first, at most every 15 minutes (5 backups are kept).
    pub fn save(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(s) => {
                backup_if_due(&Self::path(), &backup_dir(), crate::util::unix_now());
                if let Err(e) = write_atomic(&Self::path(), s.as_bytes()) {
                    log::warn!("Could not save settings: {e}");
                }
            }
            Err(e) => log::warn!("Could not save settings: {e}"),
        }
    }

    pub fn apply_style(&mut self, s: Style) {
        self.style = s;
        match s {
            Style::Competitive => {
                self.smooth_vs_stream = 15;
                self.fps_vs_visuals = 10;
            }
            Style::Balanced => {
                self.smooth_vs_stream = 40;
                self.fps_vs_visuals = 45;
            }
            Style::Cinematic => {
                self.smooth_vs_stream = 75;
                self.fps_vs_visuals = 90;
            }
            Style::Custom => {}
        }
    }

    /// Answer to "When your game and your stream compete, which wins?"
    pub fn q_game_stream(&self) -> u8 {
        match self.smooth_vs_stream {
            0..=27 => 0,
            28..=57 => 1,
            _ => 2,
        }
    }
    /// Answer to "Inside the game, what do you prefer?"
    pub fn q_fps_visuals(&self) -> u8 {
        match self.fps_vs_visuals {
            0..=27 => 0,
            28..=57 => 1,
            _ => 2,
        }
    }
    pub fn set_q_game_stream(&mut self, v: u8) {
        self.smooth_vs_stream = [15, 40, 75][v.min(2) as usize];
        self.style = Style::Custom;
    }
    pub fn set_q_fps_visuals(&mut self, v: u8) {
        self.fps_vs_visuals = [10, 45, 90][v.min(2) as usize];
        self.style = Style::Custom;
    }
    pub fn is_hidden(&self, issue: &str, opt: &str) -> bool {
        let k = format!("{issue}:{opt}");
        self.hidden_options.iter().any(|h| h.key == k)
    }

    pub fn style_name(&self) -> &'static str {
        match self.style {
            Style::Competitive => "Competitive",
            Style::Balanced => "Balanced",
            Style::Cinematic => "Cinematic",
            Style::Custom => "Custom",
        }
    }
}

// ---------------------------------------------------------------------------
// Secret storage for the OBS WebSocket password.
//
// Windows: encrypted with the Windows Data Protection API (only this
// Windows account can decrypt it).
// Linux (test build): a private file readable only by this user. Keyring
// (Secret Service) support is planned for a later test build.

fn secret_path() -> PathBuf {
    crate::util::dirs().config.join("obs-secret.bin")
}

/// Writes a file through a temporary file and a rename.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Rolling backups of settings.json.
pub const BACKUP_EVERY_SECS: i64 = 15 * 60;
pub const BACKUPS_KEPT: usize = 5;

fn backup_dir() -> PathBuf {
    crate::util::dirs().config.join("backups")
}

fn backups(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("settings-") && n.ends_with(".json")
                        })
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    // Names hold the time as a number, so they sort oldest first.
    v.sort();
    v
}

/// Copies the current settings file into the backups folder when the newest
/// backup is 15 minutes old or more, then keeps only the newest 5.
pub fn backup_if_due(file: &std::path::Path, dir: &std::path::Path, now: i64) -> bool {
    if !file.exists() {
        return false;
    }
    let existing = backups(dir);
    let newest = existing.last().and_then(|p| {
        p.file_stem()?
            .to_string_lossy()
            .strip_prefix("settings-")?
            .parse::<i64>()
            .ok()
    });
    if newest.is_some_and(|t| now - t < BACKUP_EVERY_SECS && now >= t) {
        return false;
    }
    let _ = std::fs::create_dir_all(dir);
    if std::fs::copy(file, dir.join(format!("settings-{now:012}.json"))).is_err() {
        return false;
    }
    let all = backups(dir);
    if all.len() > BACKUPS_KEPT {
        for p in &all[..all.len() - BACKUPS_KEPT] {
            let _ = std::fs::remove_file(p);
        }
    }
    true
}

/// The newest backup that reads correctly, and its file name.
fn newest_good_backup(dir: &std::path::Path) -> Option<(Settings, String)> {
    backups(dir).into_iter().rev().find_map(|p| {
        let v = serde_json::from_str::<Settings>(&std::fs::read_to_string(&p).ok()?).ok()?;
        Some((v, p.file_name()?.to_string_lossy().to_string()))
    })
}

pub fn save_secret(pw: &str) {
    let data = protect(pw.as_bytes());
    if let Some(d) = data {
        let p = secret_path();
        if std::fs::write(&p, d).is_ok() {
            restrict_permissions(&p);
        }
    }
}

pub fn load_secret() -> Option<String> {
    let d = std::fs::read(secret_path()).ok()?;
    unprotect(&d).and_then(|b| String::from_utf8(b).ok())
}

pub fn forget_secret() {
    let _ = std::fs::remove_file(secret_path());
}

/// The paired-mode key shared with the other PC, stored like the OBS password.
fn pair_path() -> PathBuf {
    crate::util::dirs().config.join("pair-secret.bin")
}

pub fn save_pair_secret(token: &str) {
    if let Some(d) = protect(token.as_bytes()) {
        let p = pair_path();
        if std::fs::write(&p, d).is_ok() {
            restrict_permissions(&p);
        }
    }
}

pub fn load_pair_secret() -> Option<String> {
    let d = std::fs::read(pair_path()).ok()?;
    unprotect(&d).and_then(|b| String::from_utf8(b).ok())
}

pub fn forget_pair_secret() {
    let _ = std::fs::remove_file(pair_path());
}

#[cfg(unix)]
fn restrict_permissions(p: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict_permissions(_p: &std::path::Path) {}

#[cfg(not(windows))]
fn protect(b: &[u8]) -> Option<Vec<u8>> {
    Some(b.to_vec())
}
#[cfg(not(windows))]
fn unprotect(b: &[u8]) -> Option<Vec<u8>> {
    Some(b.to_vec())
}

#[cfg(windows)]
fn protect(b: &[u8]) -> Option<Vec<u8>> {
    crate::winsys::protect(b)
}
#[cfg(windows)]
fn unprotect(b: &[u8]) -> Option<Vec<u8>> {
    crate::winsys::unprotect(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_settings_backups() {
        let dir = std::env::temp_dir().join(format!("pf-bak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        let bdir = dir.join("backups");
        assert!(!backup_if_due(&file, &bdir, 1000), "nothing to back up yet");
        write_atomic(&file, b"{\"settings_version\": 1}").unwrap();
        assert!(backup_if_due(&file, &bdir, 1000));
        // Not again within 15 minutes.
        assert!(!backup_if_due(&file, &bdir, 1000 + BACKUP_EVERY_SECS - 1));
        for k in 1..=7 {
            assert!(backup_if_due(&file, &bdir, 1000 + k * BACKUP_EVERY_SECS));
        }
        assert_eq!(backups(&bdir).len(), BACKUPS_KEPT);
        // A broken newest backup is skipped.
        std::fs::write(
            bdir.join(format!("settings-{:012}.json", 999_999)),
            "{broken",
        )
        .unwrap();
        assert!(newest_good_backup(&bdir).is_some());
        assert!(
            !dir.join("settings.tmp").exists(),
            "no temporary file left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
