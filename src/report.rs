// SPDX-License-Identifier: GPL-3.0-or-later
//! Diagnostics: a self-test of every part of the app, and a privacy-safe
//! bug report the user previews (and can trim) before saving.

use crate::analysis::Analysis;
use crate::games::GameResult;
use crate::hardware::HardwareInfo;
use crate::livetest::TestResult;
use crate::network::NetResult;
use crate::obs::ObsSnapshot;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TestStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
pub struct SelfTest {
    pub name: String,
    pub status: TestStatus,
    pub detail: String,
    pub fix: String,
}

pub struct SelfTestInput<'a> {
    pub hw: Option<&'a HardwareInfo>,
    pub games: &'a [GameResult],
    pub obs: Option<&'a ObsSnapshot>,
    pub obs_error: Option<&'a str>,
    pub net: Option<&'a NetResult>,
}

pub fn self_test(i: &SelfTestInput) -> Vec<SelfTest> {
    let mut v = Vec::new();
    let t = |n: &str, s: TestStatus, d: String, f: &str| SelfTest {
        name: n.into(),
        status: s,
        detail: d,
        fix: f.into(),
    };

    // App folders
    let probe = crate::util::dirs().data.join(".write-test");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    v.push(if ok {
        t(
            "App data folder",
            TestStatus::Pass,
            "Settings and reports can be saved.".into(),
            "",
        )
    } else {
        t(
            "App data folder",
            TestStatus::Fail,
            "Preflight can't write to its data folder.".into(),
            "Check that your user folder isn't full or read-only.",
        )
    });

    match i.hw {
        Some(h) if !h.gpus.is_empty() => v.push(t(
            "Hardware detection",
            TestStatus::Pass,
            format!("{} · {} · {}", h.cpu, h.gpus.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(", "), h.encoders.join(", ")),
            "",
        )),
        Some(h) => v.push(t("Hardware detection", TestStatus::Warn, format!("CPU found ({}), but no graphics card was detected.", h.cpu), "On Linux, make sure your graphics driver is installed. Include a bug report so we can look.")),
        None => v.push(t("Hardware detection", TestStatus::Warn, "Not scanned yet.".into(), "Run a quick scan.")),
    }

    let mon = crate::hardware::Monitor::new();
    v.push(if mon.gpu_source.starts_with("not available") {
        t(
            "GPU usage readings",
            TestStatus::Warn,
            "GPU usage can't be read on this graphics card yet.".into(),
            "Tests will still measure CPU and OBS. Send a bug report so we can add support.",
        )
    } else {
        t(
            "GPU usage readings",
            TestStatus::Pass,
            format!("Reading GPU usage from {}.", mon.gpu_source),
            "",
        )
    });

    match (i.obs, i.obs_error) {
        (Some(o), _) if o.connected => v.push(t(
            "OBS connection",
            TestStatus::Pass,
            format!(
                "Connected to OBS {} (WebSocket {}).",
                o.obs_version, o.ws_version
            ),
            "",
        )),
        (Some(o), Some(e)) => v.push(t(
            "OBS connection",
            TestStatus::Warn,
            format!("Not connected; using {}. {}", o.source, e),
            "Open OBS and turn on Tools › WebSocket Server Settings › Enable WebSocket server.",
        )),
        (None, Some(e)) => v.push(t(
            "OBS connection",
            TestStatus::Fail,
            e.to_string(),
            "Open OBS and turn on Tools › WebSocket Server Settings › Enable WebSocket server.",
        )),
        _ => v.push(t(
            "OBS connection",
            TestStatus::Warn,
            "Not checked yet.".into(),
            "Run a quick scan.",
        )),
    }
    if let Some(o) = i.obs {
        if o.encoder_raw.is_empty() {
            v.push(t(
                "OBS encoder settings",
                TestStatus::Warn,
                "The encoder setting couldn't be read.".into(),
                "Send a bug report so we can see which OBS version you have.",
            ));
        } else {
            v.push(t(
                "OBS encoder settings",
                TestStatus::Pass,
                format!(
                    "{} ({}), bitrate {}, rate control {}, keyframes {}, preset {}, downscale {}.",
                    o.encoder_kind,
                    o.encoder_raw,
                    o.bitrate_kbps
                        .map(|b| format!("{b} kbps"))
                        .unwrap_or_else(|| "unknown".into()),
                    o.rate_control,
                    match o.keyint_sec {
                        Some(0) => "auto".to_string(),
                        Some(k) => format!("{k} s"),
                        None => "unknown".into(),
                    },
                    o.preset,
                    o.scale_type
                ),
                "",
            ));
        }
    }

    let found: Vec<&GameResult> = i.games.iter().filter(|g| g.relevant()).collect();
    if found.is_empty() {
        v.push(t("Game profiles", TestStatus::Warn, "None of the 7 supported games were found.".into(), "If a supported game is installed, send a bug report so we can see where it was looked for."));
    }
    for g in found {
        let read = g.files.iter().any(|f| f.read_ok);
        let errs: Vec<String> = g.files.iter().filter_map(|f| f.error.clone()).collect();
        let status = if read && g.unmatched_rules.len() < 3 {
            TestStatus::Pass
        } else if read || g.files.iter().any(|f| f.found) {
            TestStatus::Warn
        } else {
            // Installed but never launched (or no setting changed yet): normal, not a fault.
            TestStatus::Pass
        };
        let detail = if read {
            format!(
                "Read {} settings. {} finding{}.{}",
                g.files.iter().map(|f| f.settings_count).sum::<usize>(),
                g.findings.len(),
                if g.findings.len() == 1 { "" } else { "s" },
                if g.unmatched_rules.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Rules that matched nothing: {}.",
                        g.unmatched_rules.join(", ")
                    )
                }
            )
        } else if !errs.is_empty() {
            format!("Settings file found but not read: {}", errs.join("; "))
        } else {
            {
                let places: Vec<String> = g.files.iter().flat_map(|f| f.searched.clone()).collect();
                format!(
                    "Installed, but it hasn't saved a settings file yet, so it probably hasn't been launched on this PC. Launch it once and change any setting, then scan again. (Looked in {} place{}.)",
                    places.len(),
                    if places.len() == 1 { "" } else { "s" }
                )
            }
        };
        let fix = if status == TestStatus::Pass {
            ""
        } else {
            "Launch the game once and change any setting so it saves its file, then scan again. If it still fails, send a bug report."
        };
        v.push(t(&format!("Profile: {}", g.name), status, detail, fix));
    }

    match i.net {
        Some(n) if n.ok => v.push(t(
            "Network test",
            TestStatus::Pass,
            format!(
                "Upload {:.1} Mbps, latency {:.0} ms.",
                n.avg_mbps,
                n.latency_ms.unwrap_or(0.0)
            ),
            "",
        )),
        Some(n) => v.push(t(
            "Network test",
            TestStatus::Fail,
            n.error.clone().unwrap_or_default(),
            "Check your internet connection or firewall.",
        )),
        None => v.push(t(
            "Network test",
            TestStatus::Warn,
            "Not run yet.".into(),
            "Run a quick scan with the network test on.",
        )),
    }

    v.push(match crate::update::REPO {
        Some(_) => t(
            "Updates",
            TestStatus::Pass,
            "Update checks are set up.".into(),
            "",
        ),
        None => t(
            "Updates",
            TestStatus::Warn,
            "Update checks are off in this test build (no GitHub repository set yet).".into(),
            "Nothing to do.",
        ),
    });
    v
}

// ---------------------------------------------------------------------------
// Bug report

#[derive(Clone, Debug)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub include: bool,
    pub content: String,
}

pub struct ReportInput<'a> {
    /// What the report is about, e.g. `Halo Infinite: "V-Sync is on"`.
    pub context: &'a str,
    pub description: &'a str,
    pub hw: Option<&'a HardwareInfo>,
    pub games: &'a [GameResult],
    pub obs: Option<&'a ObsSnapshot>,
    pub obs_error: Option<&'a str>,
    pub net: Option<&'a NetResult>,
    pub test: Option<&'a TestResult>,
    pub analysis: &'a Analysis,
    pub selftests: &'a [SelfTest],
    pub settings: &'a crate::config::Settings,
    pub encoders: &'a [crate::encoders::EncoderInfo],
    /// Sections the app puts together: post-flight, test history, one setup,
    /// what was on screen, the tray. (id, title, text) — numbers and
    /// Preflight's own words only.
    pub extra: &'a [(&'static str, &'static str, String)],
}

fn pretty<T: Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

pub fn build_sections(i: &ReportInput) -> Vec<Section> {
    let mut s = Vec::new();
    let failing: Vec<String> = i
        .selftests
        .iter()
        .filter(|t| t.status != TestStatus::Pass)
        .map(|t| format!("- {} ({:?}): {}", t.name, t.status, t.detail))
        .collect();
    s.push(Section {
        id: "summary",
        title: "Summary",
        include: true,
        content: format!(
            "About: {}\nUser's note: {}\nScores shown: Smoothness {} · Setup {} · {}\nLast test: {}\nSelf-test problems:\n{}\n",
            if i.context.is_empty() { "general report" } else { i.context },
            if i.description.trim().is_empty() { "(none)" } else { i.description.trim() },
            i.analysis.smoothness.map(|v| v.to_string()).unwrap_or_else(|| "not tested".into()),
            i.analysis.setup_score,
            i.analysis.verdict,
            i.test.map(|t| format!("{}{} · {:.0} s", t.game_running.clone().or_else(|| t.game_guess.clone()).unwrap_or_else(|| "no game seen".into()), if t.quick { " · quick re-test" } else { "" }, t.seconds)).unwrap_or_else(|| "none".into()),
            if failing.is_empty() { "- none".to_string() } else { failing.join("\n") }
        ),
    });
    s.push(Section {
        id: "about",
        title: "App and system",
        include: true,
        content: format!(
            "App: {} {} ({})\nOS: {}\nKernel: {}\nSession: {}\n",
            crate::util::APP_NAME,
            crate::util::VERSION,
            crate::util::BUILD_LABEL,
            i.hw.map(|h| h.os.clone()).unwrap_or_default(),
            i.hw.map(|h| h.kernel.clone()).unwrap_or_default(),
            i.hw.map(|h| h.session.clone()).unwrap_or_default()
        ),
    });
    s.push(Section {
        id: "selftest",
        title: "Self-test results",
        include: true,
        content: pretty(&i.selftests),
    });
    s.push(Section {
        id: "hardware",
        title: "Hardware",
        include: true,
        content: i.hw.map(pretty).unwrap_or_else(|| "not scanned".into()),
    });
    if !i.encoders.is_empty() {
        s.push(Section {
            id: "encoders",
            title: "Video encoders (program names only)",
            include: true,
            content: pretty(&i.encoders),
        });
    }
    let mut obs = i.obs.map(pretty).unwrap_or_else(|| "no OBS data".into());
    if let Some(e) = i.obs_error {
        obs.push_str(&format!("\nconnection error: {e}"));
    }
    s.push(Section {
        id: "obs",
        title: "Streaming app (OBS) — scene and source names removed",
        include: true,
        content: obs,
    });
    // Full setting-name lists (names only, never values) for the game the
    // report is about, and for games with rules that matched nothing, so the
    // missing setting can be found.
    let ctx = i.context.to_lowercase();
    let focus: Option<String> = i
        .games
        .iter()
        .find(|g| ctx.contains(&g.name.to_lowercase()))
        .map(|g| g.id.clone());
    let games: Vec<GameResult> = i
        .games
        .iter()
        .filter(|g| g.relevant())
        .map(|g| {
            let mut g = g.clone();
            // Setting names only help when something couldn't be read.
            if g.unreadable.is_empty() && focus.as_deref() != Some(g.id.as_str()) {
                g.key_names.clear();
            }
            g.key_names.truncate(200);
            g
        })
        .collect();
    s.push(Section {
        id: "games",
        title: "Game profiles (the settings Preflight checks, with their values)",
        include: true,
        content: pretty(&games),
    });
    s.push(Section {
        id: "gamefiles",
        title: "Game settings files (the tested game in full, user name removed from paths)",
        include: true,
        content: game_files_text(i),
    });
    s.push(Section {
        id: "network",
        title: "Network test",
        include: true,
        content: i.net.map(pretty).unwrap_or_else(|| "not run".into()),
    });
    s.push(Section {
        id: "test",
        title: "Last full test (raw measurements)",
        include: true,
        content: i.test.map(pretty).unwrap_or_else(|| "no test yet".into()),
    });
    s.push(Section {
        id: "analysis",
        title: "Score and issues shown",
        include: true,
        content: pretty(i.analysis),
    });
    let st = i.settings;
    s.push(Section {
        id: "prefs",
        title: "Preferences",
        include: true,
        content: format!(
            "style={} smooth_vs_stream={} fps_vs_visuals={} platform={} monitor_hz={} never={:?} advanced_view={}\n\
             default_style={:?} styles_by_game={}\n\
             platforms={:?} multistream_how={} measure_next_stream={} last_test={}\n\
             bandwidth_test_pending={} auto_games={}\n\
             minimize_to_tray={} close_to_tray={} tray_notifications={} notify_test_done={} network_test_on_scan={} auto_delete_recordings={}",
            st.style_name(),
            st.smooth_vs_stream,
            st.fps_vs_visuals,
            st.platform.name(),
            st.monitor_hz,
            st.never,
            st.advanced_view,
            st.default_style,
            st.style_by_game.iter().map(|(g, s)| format!("{g}={s:?}")).collect::<Vec<_>>().join(", "),
            st.platforms,
            if st.multistream_how.is_empty() { "-" } else { &st.multistream_how },
            st.measure_next_stream,
            st.last_test,
            st.bandwidth_test_pending,
            st.auto_games.len(),
            st.minimize_to_tray,
            st.close_to_tray,
            st.tray_notifications,
            st.notify_test_done,
            st.network_test_on_scan,
            st.auto_delete_recordings,
        ),
    });
    for (id, title, text) in i.extra {
        s.push(Section {
            id,
            title,
            include: true,
            content: redact_private_lines(text),
        });
    }
    s.push(Section {
        id: "logs",
        title: "Recent activity (last 150 log lines)",
        include: true,
        content: crate::logger::recent(150).join("\n"),
    });
    let crashes = crate::logger::pending_crashes();
    if !crashes.is_empty() {
        let text = crashes
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect::<Vec<_>>()
            .join("\n-----\n");
        s.push(Section {
            id: "crash",
            title: "Crash logs from earlier sessions",
            include: true,
            content: text,
        });
    }
    for sec in s.iter_mut() {
        sec.content = crate::util::scrub(&sec.content);
    }
    s
}

/// The full settings files of the games found, the tested game first, so a
/// profile can be fixed from one report. Only the files the profiles list.
fn game_files_text(i: &ReportInput) -> String {
    const MAX: usize = 200 * 1024;
    let tested = i.test.and_then(|t| t.game_running.clone());
    let mut games: Vec<&GameResult> = i
        .games
        .iter()
        .filter(|g| g.files.iter().any(|f| f.read_ok || f.found))
        .collect();
    games.sort_by_key(|g| (Some(&g.name) != tested.as_ref(), !g.running));
    // Full text only for the game that matters (tested, running or picked); a
    // summary for the rest keeps reports small (one game can be 3,000 lines).
    let chosen = i
        .settings
        .last_preflight
        .as_ref()
        .and_then(|p| p.game.clone());
    let focus = |g: &GameResult| {
        Some(&g.name) == tested.as_ref() || g.running || Some(&g.name) == chosen.as_ref()
    };
    let any_focus = games.iter().any(|g| focus(g));
    let mut out = String::new();
    let mut skipped: Vec<String> = Vec::new();
    for g in games {
        if any_focus && !focus(g) {
            let n: usize = g
                .files
                .iter()
                .filter(|f| f.read_ok)
                .map(|f| f.settings_count)
                .sum();
            skipped.push(format!("{} ({n} settings read)", g.name));
            continue;
        }
        for f in g.files.iter().filter(|f| f.found) {
            if let Some(t) = &f.inline_text {
                // Registry values: already decoded and made safe.
                out.push_str(&format!("----- {} · {} -----\n", g.name, f.shown_path));
                out.push_str(&redact_private_lines(&crate::util::scrub(t)));
                out.push_str("\n\n");
                continue;
            }
            let Some(p) = &f.real_path else { continue };
            out.push_str(&format!("----- {} · {} -----\n", g.name, f.shown_path));
            match std::fs::read(p) {
                Ok(b) if b.len() > MAX => out.push_str(&format!(
                    "(file is {} KB, too big to include)\n\n",
                    b.len() / 1024
                )),
                Ok(b) => match String::from_utf8(b) {
                    Ok(t) => {
                        out.push_str(&redact_private_lines(&crate::util::scrub(&t)));
                        out.push_str("\n\n");
                    }
                    Err(_) => out.push_str("(not a text file, not included)\n\n"),
                },
                Err(e) => out.push_str(&format!("(couldn't open: {e})\n\n")),
            }
        }
    }
    if !skipped.is_empty() {
        out.push_str(&format!(
            "Not included (full text only for the game you tested or are playing): {}\n",
            skipped.join(", ")
        ));
    }
    if out.is_empty() {
        "no game settings files found".into()
    } else {
        out
    }
}

/// Game files can hold a player or account name (CS2's "name", a Steam ID...).
/// Lines that look like that keep their setting name, but the value is removed.
pub fn redact_private_lines(text: &str) -> String {
    const WORDS: [&str; 22] = [
        "\"name\"",
        "lastserver",
        "serveraddress",
        "playername",
        "player_name",
        "username",
        "user_name",
        "account",
        "email",
        "password",
        "token",
        "steamid",
        "steam_id",
        "nickname",
        "displayname",
        "gamertag",
        "profilename",
        "cl_name",
        "lobby",
        "friend",
        "clantag",
        "userid",
    ];
    // Settings files that list options as {"name": ..., "value": ...} use
    // "name" for the setting, not the player: keep those.
    let structural = text
        .lines()
        .filter(|l| l.trim_start().to_lowercase().starts_with("\"name\":"))
        .count()
        >= 10;
    text.lines()
        .map(|l| {
            let low = l.to_lowercase();
            let bare = low.trim().trim_matches('"');
            let name_line = !structural
                && (low.contains("\"name\"")
                    || bare.starts_with("name ")
                    || bare.starts_with("name=")
                    || bare.starts_with("name\""));
            if WORDS.iter().skip(1).any(|w| low.contains(w)) || name_line {
                let cut = l.find(['=', ':', '\t']).map(|i| i + 1).unwrap_or_else(|| {
                    l.len().min(
                        l.trim_start()
                            .find(' ')
                            .map(|p| p + (l.len() - l.trim_start().len()))
                            .unwrap_or(l.len()),
                    )
                });
                format!("{} (value removed by Preflight)", l[..cut].trim_end())
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// File name that says what it is: build, date and time, Report ID.
pub fn file_name(id: &str) -> String {
    let when = chrono::Local::now().format("%Y-%m-%d_%H%M");
    format!(
        "StreamPreflight-report_v{}_{when}_{id}.txt",
        crate::util::VERSION
    )
}

/// Writes the included sections to a plain text file in the reports folder.
pub fn save(sections: &[Section], id: &str) -> Result<std::path::PathBuf, String> {
    let path = crate::util::dirs().reports.join(file_name(id));
    let mut text = format!(
        "Stream Preflight bug report\nReport ID: {id}\nCreated: {}\nApp: {} {} ({})\n\n",
        crate::util::now_local(),
        crate::util::APP_NAME,
        crate::util::VERSION,
        crate::util::BUILD_LABEL
    );
    for s in sections.iter().filter(|s| s.include) {
        text.push_str(&format!("===== {} =====\n{}\n\n", s.title, s.content));
    }
    let removed: Vec<&str> = sections
        .iter()
        .filter(|s| !s.include)
        .map(|s| s.title)
        .collect();
    if !removed.is_empty() {
        text.push_str(&format!(
            "Sections removed by the user: {}\n",
            removed.join(", ")
        ));
    }
    std::fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
    // Crash files that are now in a report don't need to be offered again.
    if sections.iter().any(|s| s.id == "crash" && s.include) {
        for p in crate::logger::pending_crashes() {
            let _ = std::fs::rename(&p, p.with_extension("reported.txt"));
        }
    }
    log::info!("Bug report {id} saved");
    Ok(path)
}

/// Opens the folder with the file selected. On Linux this uses the standard
/// "show this file" request (Dolphin, Nautilus and others answer it and come
/// to the front); xdg-open on the folder is the fallback.
pub fn show_file(p: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", p.display()))
        .spawn();
    #[cfg(not(windows))]
    {
        let uri = format!("file://{}", p.display()).replace(' ', "%20");
        let tries: [(&str, Vec<String>); 2] = [
            (
                "gdbus",
                vec![
                    "call".into(),
                    "--session".into(),
                    "--dest".into(),
                    "org.freedesktop.FileManager1".into(),
                    "--object-path".into(),
                    "/org/freedesktop/FileManager1".into(),
                    "--method".into(),
                    "org.freedesktop.FileManager1.ShowItems".into(),
                    format!("['{uri}']"),
                    "".into(),
                ],
            ),
            (
                "dbus-send",
                vec![
                    "--session".into(),
                    "--print-reply".into(),
                    "--dest=org.freedesktop.FileManager1".into(),
                    "--type=method_call".into(),
                    "/org/freedesktop/FileManager1".into(),
                    "org.freedesktop.FileManager1.ShowItems".into(),
                    format!("array:string:{uri}"),
                    "string:".into(),
                ],
            ),
        ];
        for (cmd, args) in tries {
            if let Ok(st) = std::process::Command::new(cmd)
                .args(&args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                && st.success()
            {
                return;
            }
        }
        log::info!("Show in folder: no file manager answered the D-Bus request; using xdg-open");
        open_folder(p.parent().unwrap_or(p));
    }
}

/// Opens a file with its default app (the report .txt in a text editor).
pub fn open_file(p: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", &p.display().to_string()])
        .spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

/// Copies text with the system's own tool as well (wl-copy on Wayland,
/// xclip/xsel on X11), in case the app's clipboard doesn't reach other apps.
pub fn copy_fallback(text: &str) {
    #[cfg(not(windows))]
    {
        use std::io::Write;
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let tools: Vec<(&str, Vec<&str>)> = if wayland {
            vec![("wl-copy", vec![])]
        } else {
            vec![
                ("xclip", vec!["-selection", "clipboard"]),
                ("xsel", vec!["--clipboard", "--input"]),
            ]
        };
        for (cmd, args) in tools {
            if let Ok(mut c) = std::process::Command::new(cmd)
                .args(&args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                if let Some(mut i) = c.stdin.take() {
                    let _ = i.write_all(text.as_bytes());
                }
                // wl-copy stays running to serve the clipboard; don't wait.
                return;
            }
        }
    }
    #[cfg(windows)]
    let _ = text;
}

pub fn open_folder(p: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

#[cfg(test)]
mod name_tests {
    #[test]
    fn private_lines_are_removed() {
        let t = "\"name\"\t\t\"SomeStreamer\"\n\"fps_max\"\t\"400\"\nAccountName=someone\nvolume=1";
        let r = super::redact_private_lines(t);
        assert!(
            !r.contains("SomeStreamer") && !r.contains("someone\n") && !r.ends_with("someone"),
            "{r}"
        );
        assert!(r.contains("\"fps_max\"\t\"400\"") && r.contains("volume=1"));
    }

    #[test]
    fn report_file_name_says_build_date_and_id() {
        let n = super::file_name("PF-AB12-CD34");
        assert!(
            n.starts_with(&format!(
                "StreamPreflight-report_v{}_",
                crate::util::VERSION
            )),
            "{n}"
        );
        assert!(n.ends_with("_PF-AB12-CD34.txt"), "{n}");
    }
}

#[cfg(test)]
mod extra_tests {
    #[test]
    fn extra_sections_are_included_and_scrubbed() {
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().display().to_string())
            .unwrap_or_else(|| "/home/someone".into());
        let extra = vec![
            (
                "postflight",
                "Post-flight (numbers only)",
                "Last stream: 1:02:03 · Smooth · labelled test stream".to_string(),
            ),
            (
                "ui",
                "What was on screen",
                format!(
                    "\"name\"\t\"SomePlayer\"\nrecording at {home}/Videos/x.mkv\nAccountName=bob\nFixes: 2 of 5 left"
                ),
            ),
        ];
        let st = crate::config::Settings::default();
        let a = crate::analysis::Analysis::default();
        let secs = super::build_sections(&super::ReportInput {
            context: "",
            description: "",
            hw: None,
            games: &[],
            obs: None,
            obs_error: None,
            net: None,
            test: None,
            analysis: &a,
            selftests: &[],
            settings: &st,
            encoders: &[],
            extra: &extra,
        });
        let pf = secs
            .iter()
            .find(|s| s.id == "postflight")
            .expect("post-flight section");
        assert!(pf.content.contains("labelled test stream"));
        let ui = secs.iter().find(|s| s.id == "ui").expect("ui section");
        assert!(!ui.content.contains("SomePlayer"), "{}", ui.content);
        assert!(!ui.content.contains("bob"), "{}", ui.content);
        assert!(!ui.content.contains(&home), "{}", ui.content);
        assert!(ui.content.contains("2 of 5 left"));
        let prefs = secs.iter().find(|s| s.id == "prefs").unwrap();
        assert!(
            prefs.content.contains("close_to_tray=false")
                && prefs.content.contains("default_style=Balanced"),
            "{}",
            prefs.content
        );
    }
}
