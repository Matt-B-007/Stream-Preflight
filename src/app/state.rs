// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's state and background work: scans, polls, tests, OBS, fix tracking.

use super::*;

/// Reads OBS through the shared connection, or directly if it isn't running.
pub(super) fn obs_now(
    hub: Option<&std::sync::mpsc::Sender<crate::streamwatch::Cmd>>,
    host: &str,
    port: u16,
    pw: Option<&str>,
) -> (Option<ObsSnapshot>, Option<String>) {
    match hub {
        Some(h) => crate::streamwatch::request_snapshot(h),
        None => read_obs(host, port, pw),
    }
}

pub(super) fn read_obs(
    host: &str,
    port: u16,
    pw: Option<&str>,
) -> (Option<ObsSnapshot>, Option<String>) {
    match crate::obs::Client::connect(host, port, pw) {
        Ok(mut c) => (Some(crate::obs::snapshot_live(&mut c)), None),
        Err(e) => {
            log::info!("OBS not connected: {e}");
            (crate::obs::snapshot_saved(), Some(e.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Shared pieces

/// Share of a test's GPU samples at 95% or more, in percent.
pub(super) fn maxed_pct(t: &TestResult) -> f32 {
    let v: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
    if v.is_empty() {
        0.0
    } else {
        v.iter().filter(|g| **g >= 95.0).count() as f32 * 100.0 / v.len() as f32
    }
}

/// Important issues first, grouped by where you fix them (OBS, game, PC),
/// then the optional ones in the same grouping.
pub(super) fn fix_order(issues: &[Issue], kept: &[String]) -> Vec<Issue> {
    let mut v: Vec<Issue> = issues
        .iter()
        .filter(|i| !kept.contains(&i.id))
        .cloned()
        .collect();
    v.sort_by_key(|i| {
        (
            analysis::is_optional(i),
            analysis::place_of(&i.id),
            i.priority,
        )
    });
    v
}

pub(super) fn cur_measured(id: &str) -> bool {
    matches!(
        id,
        "gpu_maxed" | "gpu_maybe" | "encoder_lag" | "cpu_high" | "heat" | "audio_balance"
    )
}

/// One clear action for a problem message, when there is one.
pub(super) fn err_action(e: &str) -> Option<(&'static str, NextAct)> {
    let l = e.to_lowercase();
    if l.contains("password") || l.contains("rejected") || l.contains("authentication") {
        Some(("Enter the OBS password", NextAct::ObsSettings))
    } else if l.contains("couldn't reach obs")
        || l.contains("not running")
        || l.contains("websocket")
        || l.contains("open obs")
        || l.contains("connection refused")
    {
        Some(("Show me how", NextAct::ObsHelp))
    } else if l.contains("speed-test") || l.contains("upload test") {
        Some(("What to do", NextAct::Screen(Screen::Help)))
    } else {
        None
    }
}

/// Running programs that could be games, plus each settings file's change time.
/// The same text means nothing worth re-reading changed.
pub(super) fn poll_signature(
    procs: &[(String, f32)],
    running: &[crate::hardware::GameProc],
    files: &[std::path::PathBuf],
) -> String {
    let known: Vec<String> = crate::games::profiles()
        .into_iter()
        .flat_map(|p| p.processes)
        .collect();
    let mut names: Vec<&str> = procs
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| known.iter().any(|k| crate::games::process_matches(n, k)))
        .collect();
    names.extend(running.iter().map(|g| g.exe.as_str()));
    names.sort_unstable();
    names.dedup();
    let mut sig = names.join(",");
    for f in files {
        let t = std::fs::metadata(f)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        sig.push_str(&format!("|{t}"));
    }
    sig
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, menu_added: bool, open_uninstall: bool) -> Self {
        theme::apply(&cc.egui_ctx);
        // No settings file yet means a new install: the tour starts after setup.
        let new_install = !Settings::path().exists();
        let mut st = Settings::load();
        if new_install {
            st.tour_pending = true;
        }
        undo::clean_leftovers(&mut st);
        // A beta build looks for betas until the user decides otherwise.
        if crate::update::is_beta_version(crate::util::VERSION) && !st.updates_beta_set {
            st.updates_beta = true;
        }
        cc.egui_ctx.set_zoom_factor(st.text_scale);
        crate::doh::ENABLED.store(st.dns_fallback, std::sync::atomic::Ordering::Relaxed);
        theme::set_choice(&cc.egui_ctx, st.theme, st.reduce_motion);
        theme::GRID.store(
            st.backdrop_grid && !st.reduce_motion,
            std::sync::atomic::Ordering::Relaxed,
        );
        let password = if st.obs_remember_password {
            crate::config::load_secret().unwrap_or_default()
        } else {
            String::new()
        };
        let now = crate::util::unix_now();
        let show_popup = st.remind_delete_recording
            && st.pending_recordings.iter().any(|p| p.remind_after <= now);
        // Always open on Home: nothing is locked behind a welcome.
        let screen = Screen::Home;
        let pending_crashes = crate::logger::pending_crashes().len();
        // The last kind of test picked (a quick check only follows a change).
        let last_stream_record = st.last_test == "stream_record";
        let mut app = Self {
            st,
            password,
            screen,
            setup_screen: 1,
            setup_single: None,
            home_games_open: false,
            step: 2,
            hw: None,
            monitors: Vec::new(),
            games: Vec::new(),
            obs: None,
            obs_error: None,
            other_recorders: Vec::new(),
            net: None,
            test: None,
            test_handle: None,
            watch_paused: false,
            watch_retry_at: 0.0,
            want_new_test: false,
            analysis: Analysis::default(),
            scan: None,
            first_scan_done: false,
            last_scan: None,
            poll: None,
            poll_next: 0.0,
            fix_queue: Vec::new(),
            fix_idx: 0,
            fixed: Vec::new(),
            skipped: Vec::new(),
            detected: None,
            picks: Default::default(),
            confirm: None,
            fleet: Default::default(),
            encoders: Vec::new(),
            enc_rx: None,
            enc_next: 0.0,
            encodes_seen: Vec::new(),
            private_ok: Default::default(),
            optional_ok: false,
            show_bw_help: last_stream_record,
            prev_test: None,
            compare: None,
            tree_pf_open: true,
            show_all_fixes: false,
            test_choice_open: false,
            settings_tab: 0,
            pairing: None,
            pairing_kind: "",
            pair_code_input: String::new(),
            pair_offer: false,
            pair_try: false,
            pair_status: String::new(),
            side_measure: None,
            side_monitor: None,
            side_at: 0.0,
            side_test: None,
            test_counter: 0,
            reading_hist: Default::default(),
            reading_at: 0.0,
            tray: None,
            tray_tried: false,
            win_hwnd: 0,
            hidden: false,
            quitting: false,
            can_hide: true,
            start_hidden: std::env::args().any(|a| a == "--tray"),
            window_known: false,
            tray_hint_shown: false,
            flag_question: false,
            flag_real: false,
            queued_fix_action: None,
            go_ready: false,
            preview_split: 0.5,
            help_open: None,
            pending_act: None,
            uninstall: None,
            scroll_to_preview: false,
            quick_next: false,
            kept_game: None,
            switch_dismissed: None,
            fps_ask_open: false,
            show_other_fixes: false,
            baseline_findings: BTreeSet::new(),
            baseline_mic_db: None,
            baseline_obs: None,
            tick_dnd: false,
            tick_title: false,
            preflight_done: false,
            returning: false,
            history: History::load(),
            selftests: Vec::new(),
            report: None,
            fail_banner: None,
            update_rx: None,
            update_result: None,
            update_install: None,
            update_done: None,
            install_result: None,
            install_remove_old: true,
            open_uninstall,
            stable_confirm: false,
            prev_version: crate::update::previous_version(),
            rollback_confirm: false,
            rollback_rx: None,
            rollback_done: None,
            update_later: false,
            update_dialog: false,
            whats_new_open: false,
            shortcuts_open: false,
            command_open: false,
            command_query: String::new(),
            command_pick: 0,
            tour: None,
            update_notified: false,
            conn_rx: None,
            toast: None,
            crash_notice: crate::logger::unclean_last_time(),
            show_recording_popup: show_popup,
            pending_crashes,
            confirm_clear: false,
            setup_name: String::new(),
            preview: None,
            preview_rx: None,
            preview_tex: Vec::new(),
            preview_idx: 0,
            preview_zoom: false,
            watch: None,
            live: None,
            poll_sig: String::new(),
            poll_full_at: 0.0,
            own_usage: None,
            own_sys: None,
            own_next: 0.0,
            stream_notice: None,
            streams: crate::streamwatch::past_streams(),
            stream_sel: 0,
        };
        if pending_crashes > 0 {
            app.fail_banner = Some("Preflight closed unexpectedly last time.".into());
        }
        app.recompute();
        app.start_watch(&cc.egui_ctx);
        app.start_scan(cc.egui_ctx.clone(), app.st.setup_done);
        if menu_added {
            app.toast = Some(undo::Toast {
                msg: "Added Preflight to your app menu".into(),
                until: 6.0,
                undo: None,
                note: None,
            });
        }
        app
    }

    // -----------------------------------------------------------------------
    // Analysis helpers

    pub(super) fn target_game(&self) -> Option<&GameResult> {
        let by_test = self.test.as_ref().and_then(|t| t.game_running.clone());
        let guess = self.test.as_ref().and_then(|t| t.game_guess.clone());
        self.games
            .iter()
            .find(|g| Some(&g.name) == by_test.as_ref())
            .or_else(|| {
                guess.as_ref().and_then(|gs| {
                    self.games
                        .iter()
                        .find(|g| g.processes.iter().any(|p| p == gs))
                })
            })
            .or_else(|| self.games.iter().find(|g| g.running))
            .or_else(|| self.games.iter().find(|g| !g.findings.is_empty()))
    }

    pub(super) fn heavy_findings(&self) -> BTreeSet<String> {
        self.target_game()
            .map(|g| {
                g.findings
                    .iter()
                    .filter(|f| {
                        matches!(
                            f.effect.as_str(),
                            "heavy" | "rt_on" | "uncapped_fps" | "upscaler_off"
                        )
                    })
                    .map(|f| format!("{}={}", f.label, f.value))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn gpu_changed(&self) -> bool {
        self.test.is_some()
            && !self.baseline_findings.is_empty()
            && self.heavy_findings() != self.baseline_findings
    }

    /// The last test, with the mic level adjusted for any change the user
    /// made to the OBS mic slider since then.
    pub(super) fn adjusted_test(&self) -> Option<TestResult> {
        let mut t = self.test.clone()?;
        if let (Some(b), Some(now), Some(m)) = (
            self.baseline_mic_db,
            self.obs.as_ref().and_then(|o| o.mic_volume_db()),
            t.mic_db,
        ) {
            t.mic_db = Some(m + (now - b));
        }
        if let (Some(b), Some(now), Some(d)) = (
            self.baseline_obs
                .as_ref()
                .and_then(|o| o.desktop_volume_db()),
            self.obs.as_ref().and_then(|o| o.desktop_volume_db()),
            t.desktop_db,
        ) {
            t.desktop_db = Some(d + (now - b));
        }
        Some(t)
    }

    /// The game the results are about: the tested game, else the one running.
    pub(super) fn current_game(&self) -> Option<String> {
        self.test
            .as_ref()
            .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()))
            .or_else(|| {
                self.games
                    .iter()
                    .find(|g| g.running)
                    .map(|g| g.name.clone())
            })
    }

    /// Program names of games Preflight has no profile for, seen in tests,
    /// so their settings are read even when they aren't running.
    pub(super) fn game_hints(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .test
            .as_ref()
            .and_then(|t| t.game_guess.clone())
            .into_iter()
            .collect();
        for g in self.games.iter().filter(|g| g.auto.is_some()) {
            v.extend(g.processes.iter().cloned());
        }
        for k in self.st.game_memory.keys() {
            if k.ends_with(".exe") && !v.contains(k) {
                v.push(k.clone());
            }
        }
        v.extend(self.st.auto_games.iter().cloned());
        v.sort();
        v.dedup();
        v
    }

    /// A game without a profile: use its real name (from Steam, Heroic,
    /// Lutris or its folder) instead of the program file name.
    pub(super) fn name_unknown_game(&mut self, r: &mut TestResult) {
        if r.game_running.is_some() {
            return;
        }
        let Some(guess) = r.game_guess.clone() else {
            return;
        };
        let matches = |g: &crate::games::GameResult| {
            g.auto.is_some()
                && g.processes
                    .iter()
                    .any(|p| p == &guess || crate::games::process_matches(&guess, p))
        };
        if !self.games.iter().any(&matches) {
            for g in crate::games::read_unknown(std::slice::from_ref(&guess)) {
                self.games.retain(|x| x.id != g.id);
                self.games.push(g);
            }
        }
        if let Some(g) = self.games.iter().find(|g| matches(g)) {
            log::info!(
                "Game without a profile: {guess} is {} ({} settings read)",
                g.name,
                g.settings_read
            );
            r.game_running = Some(g.name.clone());
            if !self.st.auto_games.contains(&guess) {
                self.st.auto_games.push(guess.clone());
                if self.st.auto_games.len() > 20 {
                    self.st.auto_games.remove(0);
                }
            }
        }
    }

    pub(super) fn game_test_path(game: &str) -> std::path::PathBuf {
        let slug: String = game
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        crate::util::dirs().tests.join(format!("{slug}.json"))
    }

    /// Keeps each game's last full test, so switching games shows its results.
    pub(super) fn save_game_test(t: &TestResult) {
        if let Some(g) = t.game_running.clone().or_else(|| t.game_guess.clone())
            && let Ok(j) = serde_json::to_string(t)
        {
            let _ = crate::config::write_atomic(&Self::game_test_path(&g), j.as_bytes());
        }
    }

    pub(super) fn load_game_test(game: &str) -> Option<TestResult> {
        let t = std::fs::read_to_string(Self::game_test_path(game)).ok()?;
        serde_json::from_str(&t).ok()
    }

    /// Games with saved results.
    pub(super) fn saved_games(&self) -> Vec<String> {
        self.st
            .game_memory
            .keys()
            .filter(|g| Self::game_test_path(g).exists())
            .cloned()
            .collect()
    }

    /// Shows another game's saved results (or none yet).
    pub(super) fn show_game(&mut self, game: &str) {
        self.prev_test = None;
        self.compare = None;
        self.test = Self::load_game_test(game);
        self.preview = None;
        self.preview_tex.clear();
        self.recompute();
        self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
        self.fix_idx = 0;
        self.fixed.clear();
        self.skipped.clear();
        self.detected = None;
        self.switch_dismissed = None;
    }

    /// "Keep as is" for game fixes belongs to that game; OBS and PC choices
    /// apply everywhere. Swap the game's choices in when the game changes.
    pub(super) fn sync_kept(&mut self) {
        let game = self.current_game();
        if game == self.kept_game {
            if let Some(g) = &game {
                let ids: Vec<String> = self
                    .st
                    .kept_issues
                    .iter()
                    .filter(|i| analysis::place_of(i) == analysis::Place::Game)
                    .cloned()
                    .collect();
                if self.st.kept_by_game.get(g) != Some(&ids) {
                    self.st.kept_by_game.insert(g.clone(), ids);
                }
            }
            return;
        }
        if let Some(old) = &self.kept_game {
            let ids: Vec<String> = self
                .st
                .kept_issues
                .iter()
                .filter(|i| analysis::place_of(i) == analysis::Place::Game)
                .cloned()
                .collect();
            self.st.kept_by_game.insert(old.clone(), ids);
        }
        self.st
            .kept_issues
            .retain(|i| analysis::place_of(i) != analysis::Place::Game);
        if let Some(g) = &game {
            if let Some(ids) = self.st.kept_by_game.get(g).cloned() {
                self.st.kept_issues.extend(ids);
            }
            // Each game keeps its own style.
            let style = match self.st.style_by_game.get(g).copied() {
                Some(s) => Some(s),
                None if self.st.default_style != Style::Custom => {
                    // First time: start from the default style for new games.
                    self.st
                        .style_by_game
                        .insert(g.clone(), self.st.default_style);
                    Some(self.st.default_style)
                }
                None => None,
            };
            if let Some(style) = style
                && style != self.st.style
                && style != Style::Custom
            {
                self.st.apply_style(style);
                log::info!("Style for {g}: {style:?}");
            }
        }
        self.kept_game = game;
    }

    pub(super) fn recompute(&mut self) {
        self.sync_kept();
        // One setup for all games: combine every game's saved test.
        let tests: Vec<TestResult> = self
            .saved_games()
            .iter()
            .filter_map(|g| Self::load_game_test(g))
            .collect();
        self.fleet = crate::fleet::combine(&tests, &self.games, self.obs.as_ref());
        self.st.fleet_motion = if self.fleet.tested.len() > 1 {
            self.fleet.motion.clone()
        } else {
            String::new()
        };
        self.st.fleet_busy = self.fleet.tested.len() > 1 && self.fleet.busy;
        let t = self.adjusted_test();
        let gc = self.gpu_changed();
        self.analysis = analysis::analyze(
            &self.st,
            self.hw.as_ref(),
            &self.games,
            self.obs.as_ref(),
            self.net.as_ref(),
            t.as_ref(),
            gc,
        );
        // For the tray icon, also when Preflight later runs tray-only.
        self.st.last_fix_count = self.open_counts().0 as u32;
    }

    pub(super) fn signature(&self) -> String {
        let obs = self.obs.as_ref().map(|o| o.signature()).unwrap_or_default();
        let games: Vec<String> = self
            .games
            .iter()
            .filter(|g| g.relevant())
            .map(|g| {
                format!(
                    "{}:{:?}",
                    g.id,
                    g.findings
                        .iter()
                        .map(|f| (&f.label, &f.value))
                        .collect::<Vec<_>>()
                )
            })
            .collect();
        format!("{obs}#{}#{}", games.join(";"), self.st.monitor_hz)
    }

    pub(super) fn note_change(&mut self, text: String) {
        self.st.change_log.push(crate::config::ChangeNote {
            time: crate::util::unix_now(),
            text,
        });
        let n = self.st.change_log.len();
        if n > 200 {
            self.st.change_log.drain(..n - 200);
        }
        self.st.save();
    }

    pub(super) fn toast(&mut self, ctx: &egui::Context, s: impl Into<String>) {
        // While Undo is still offered, a new message is shown beside it rather
        // than ending the offer early.
        if let Some(t) = self.toast.as_mut()
            && t.undo.is_some()
        {
            t.note = Some(s.into());
            return;
        }
        self.toast = Some(undo::Toast {
            msg: s.into(),
            until: ctx.input(|i| i.time) + 4.0,
            undo: None,
            note: None,
        });
    }

    /// A dialog is open: pages and shortcuts behind it wait.
    pub(super) fn dialog_open(&self) -> bool {
        self.report.is_some() || self.uninstall.is_some() || self.update_dialog
    }

    pub(super) fn open_uninstall_box(&mut self) {
        let recs = self
            .st
            .pending_recordings
            .iter()
            .filter(|r| std::path::Path::new(&r.path).exists())
            .map(|r| (r.path.clone(), r.size, false))
            .collect();
        self.uninstall = Some(UninstallBox {
            recs,
            program: true,
            delete_settings: false,
            error: None,
            done: false,
        });
    }

    pub(super) fn pw(&self) -> Option<String> {
        if self.password.is_empty() {
            None
        } else {
            Some(self.password.clone())
        }
    }

    pub(super) fn open_report(&mut self, context: impl Into<String>) {
        self.report = Some(ReportBox {
            context: context.into(),
            summary: String::new(),
            note: String::new(),
            id: crate::util::short_id(),
            text: String::new(),
            dirty: true,
            saved: None,
            error: None,
            copied: false,
            github_confirm: false,
        });
    }

    // -----------------------------------------------------------------------
    // Background work

    pub(super) fn start_scan(&mut self, ctx: egui::Context, with_network: bool) {
        if self.scan.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let net_progress = Arc::new(Mutex::new(0.0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (np, cn) = (net_progress.clone(), cancel.clone());
        let host = self.st.obs_host.clone();
        let port = self.st.obs_port;
        let pw = self.pw();
        let need_hw = self.hw.is_none();
        let hints = self.game_hints();
        let hub = self.watch.as_ref().map(|w| w.cmd.clone());
        // Reuse a good upload result from the last 6 hours (kept across restarts)
        // instead of testing again on every scan.
        if self.net.is_none() {
            self.net = crate::network::load_recent();
        }
        let fresh = self
            .net
            .as_ref()
            .map(|n| n.reusable(crate::util::unix_now()))
            .unwrap_or(false);
        let do_net = with_network && self.st.network_test_on_scan && !fresh;
        let c2 = ctx.clone();
        std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || {
                let send = |m: ScanMsg| {
                    let _ = tx.send(m);
                    c2.request_repaint();
                };
                if need_hw {
                    send(ScanMsg::Step("Checking your hardware…".into()));
                    send(ScanMsg::Hardware(crate::hardware::scan()));
                }
                send(ScanMsg::Monitors(crate::monitors::detect()));
                send(ScanMsg::Step("Reading your game settings…".into()));
                let mut sys = crate::hardware::new_system();
                let procs = crate::hardware::running_process_names(&mut sys);
                send(ScanMsg::Procs(crate::hardware::other_recorders(&procs)));
                let running = crate::hardware::game_processes(&mut sys);
                send(ScanMsg::Games(crate::games::scan(
                    &procs, &running, &hints, &mut sys,
                )));
                send(ScanMsg::Step("Connecting to OBS…".into()));
                let (snap, err) = obs_now(hub.as_ref(), &host, port, pw.as_deref());
                send(ScanMsg::Obs(snap.map(Box::new), err));
                if do_net {
                    send(ScanMsg::Step(
                        "Testing your upload speed (up to 15 seconds)…".into(),
                    ));
                    send(ScanMsg::Net(crate::network::run(np, cn)));
                }
                send(ScanMsg::Done);
            })
            .expect("spawn scan");
        self.scan = Some(ScanJob {
            rx,
            step: "Starting…".into(),
            net_progress,
            _cancel: cancel,
        });
        log::info!("Scan started (network test: {do_net})");
    }

    /// (Re)starts the post-flight watcher with the current OBS details.
    pub(super) fn start_watch(&mut self, ctx: &egui::Context) {
        let profiles = crate::games::profiles();
        let cfg = crate::streamwatch::WatchConfig {
            host: self.st.obs_host.clone(),
            port: self.st.obs_port,
            password: self.pw(),
            game_processes: profiles
                .iter()
                .map(|p| (p.name.clone(), p.processes.clone()))
                .collect(),
        };
        self.watch = Some(crate::streamwatch::start(cfg, ctx.clone()));
    }

    pub(super) fn start_preview(&mut self, ctx: &egui::Context, t: &TestResult) {
        if t.preview_shots.is_empty() {
            self.preview = Some(crate::preview::build(&[], None, 60.0, None));
            self.auto_clean_recording(ctx);
            return;
        }
        let (tx, rx) = channel();
        let shots = t.preview_shots.clone();
        // A test recording, or the user's own recording that was already running (never deleted).
        let rec = t.recording_path.clone().or_else(|| t.live_rec_path.clone());
        let fps = t.stream_fps.unwrap_or(60.0);
        // The stream's settings, to re-create the viewer picture when the
        // test recording used different (higher-quality) settings.
        let sim = self.obs.as_ref().and_then(|o| {
            let (w, h) = o.output_res?;
            let rc = o.rec_rate_control.to_uppercase();
            let hq = (matches!(rc.as_str(), "CQP" | "CRF" | "ICQ" | "CQVBR" | "LOSSLESS")
                && o.rec_cq.map(|q| q <= 22).unwrap_or(true))
                || o.rec_bitrate_kbps.map(|b| b >= 20000).unwrap_or(false);
            Some(crate::preview::StreamSim {
                bitrate_kbps: o.bitrate_kbps?,
                width: w,
                height: h,
                fps: o.fps.unwrap_or(fps),
                recording_differs: o.recording_uses_stream_settings == Some(false),
                recording_high_quality: hq,
            })
        });
        let c = ctx.clone();
        std::thread::spawn(move || {
            let p = crate::preview::build(&shots, rec.as_deref(), fps, sim.as_ref());
            let _ = tx.send(p);
            c.request_repaint();
        });
        self.preview = None;
        self.preview_tex.clear();
        self.preview_idx = 0;
        self.preview_rx = Some(rx);
    }

    pub(super) fn start_poll(&mut self, ctx: &egui::Context) {
        if self.poll.is_some() || self.scan.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (h, p, pw) = (self.st.obs_host.clone(), self.st.obs_port, self.pw());
        let hub = self.watch.as_ref().map(|w| w.cmd.clone());
        let hints = self.game_hints();
        let c = ctx.clone();
        // Re-reading every game's settings files is the heavy part: only do it when a
        // game started or stopped, a settings file changed, or a minute has passed.
        let files: Vec<std::path::PathBuf> = self
            .games
            .iter()
            .flat_map(|g| g.files.iter().filter_map(|f| f.real_path.clone()))
            .collect();
        let prev_sig = self.poll_sig.clone();
        let now = ctx.input(|i| i.time);
        let force = self.games.is_empty() || now >= self.poll_full_at;
        if force {
            self.poll_full_at = now + 60.0;
        }
        std::thread::spawn(move || {
            let mut sys = crate::hardware::new_system();
            let procs = crate::hardware::running_process_names(&mut sys);
            let running = crate::hardware::game_processes(&mut sys);
            let sig = poll_signature(&procs, &running, &files);
            let games = if force || sig != prev_sig {
                Some(crate::games::scan(&procs, &running, &hints, &mut sys))
            } else {
                None
            };
            let (o, e) = obs_now(hub.as_ref(), &h, p, pw.as_deref());
            let _ = tx.send((games, sig, o, e));
            c.request_repaint();
        });
        self.poll = Some(PollJob { rx });
    }

    pub(super) fn poll_jobs(&mut self, ctx: &egui::Context) {
        let mut done = false;
        let mut changed = false;
        if let Some(job) = self.scan.as_mut() {
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    ScanMsg::Step(s) => job.step = s,
                    ScanMsg::Hardware(h) => self.hw = Some(h),
                    ScanMsg::Monitors(m) => {
                        if self.st.monitor_auto
                            && let Some(pick) = m
                                .iter()
                                .find(|x| x.name == self.st.monitor_name)
                                .or_else(|| m.first())
                        {
                            self.st.monitor_name = pick.name.clone();
                            self.st.monitor_hz = pick.max_hz.max(pick.current_hz).max(30);
                        }
                        self.monitors = m;
                    }
                    ScanMsg::Games(g) => {
                        self.history.note_settings(&g);
                        self.games = g;
                    }
                    ScanMsg::Procs(p) => self.other_recorders = p,
                    ScanMsg::Obs(s, e) => {
                        self.obs = s.map(|b| *b);
                        self.obs_error = e;
                    }
                    ScanMsg::Net(n) => {
                        crate::network::save(&n);
                        self.net = Some(n);
                    }
                    ScanMsg::Done => done = true,
                }
                changed = true;
            }
        }
        if changed {
            self.recompute();
        }
        if done {
            self.scan = None;
            self.last_scan = Some(crate::util::unix_now());
            let a = &self.analysis;
            self.history.add(history::Entry {
                time: crate::util::unix_now(),
                kind: "scan".into(),
                score: a.score,
                upload_avg: self.net.as_ref().filter(|n| n.ok).map(|n| n.avg_mbps),
                upload_low: self.net.as_ref().filter(|n| n.ok).map(|n| n.low_mbps),
                gpu_avg: None,
                cpu_avg: None,
                game: None,
                skipped_pct: None,
                maxed_pct: None,
                smooth: None,
                setup: Some(a.setup_score),
            });
            if !self.first_scan_done {
                self.first_scan_done = true;
                // Returning user with nothing changed: go straight to the final checks.
                if let Some(lp) = &self.st.last_preflight {
                    let recent = crate::util::unix_now() - lp.time < 7 * 24 * 3600;
                    if recent
                        && lp.signature == self.signature()
                        && self.screen == Screen::Preflight
                    {
                        self.returning = true;
                        self.step = 4;
                        log::info!("Returning user: nothing changed since the last pre-flight");
                    }
                }
            }
            self.st.save();
        }

        if let Some(p) = &self.poll
            && let Ok((g, sig, o, e)) = p.rx.try_recv()
        {
            self.poll = None;
            self.poll_sig = sig;
            if let Some(g) = g {
                self.games = g;
            }
            if o.is_some() || self.obs.is_none() {
                self.obs = o;
            }
            self.obs_error = e;
            self.recompute();
            self.check_fix_detected();
            self.check_confirm(ctx);
        }
        if let Some(rx) = &self.conn_rx
            && let Ok((s, e)) = rx.try_recv()
        {
            self.obs = s;
            self.obs_error = e;
            self.conn_rx = None;
            self.recompute();
        }
        if let Some(rx) = &self.update_install
            && let Ok(r) = rx.try_recv()
        {
            self.update_install = None;
            if r.is_ok() {
                log::info!("Update installed; waiting for Restart");
                // The version you had is now kept for Go back.
                self.prev_version = crate::update::previous_version();
            }
            self.update_done = Some(r);
        }
        if self.open_uninstall {
            self.open_uninstall = false;
            self.open_uninstall_box();
        }
        // Once a day, if allowed: read GitHub's public release list. Nothing about you is sent.
        let now_unix = crate::util::unix_now();
        if self.st.updates_auto
            && self.update_rx.is_none()
            && self.update_result.is_none()
            && now_unix - self.st.last_update_check > 24 * 3600
        {
            self.st.last_update_check = now_unix;
            self.st.save();
            let (tx, rx) = channel();
            let beta = self.st.updates_beta;
            std::thread::spawn(move || {
                let _ = tx.send(crate::update::check(beta));
            });
            self.update_rx = Some(rx);
        }
        if !self.update_notified
            && let Some(Ok(u)) = &self.update_result
            && u.newer
        {
            self.update_notified = true;
            let (t, b) = (
                format!("Stream Preflight v{} is available", u.latest),
                "Open Preflight and click Update now.".to_string(),
            );
            self.notify_away(ctx, &t, &b);
        }
        if let Some(rx) = &self.update_rx
            && let Ok(r) = rx.try_recv()
        {
            if r.is_ok() {
                self.st.last_update_check = crate::util::unix_now();
                self.st.save();
            }
            self.update_result = Some(r);
            self.update_rx = None;
        }

        if let Some(rx) = &self.preview_rx
            && let Ok(p) = rx.try_recv()
        {
            self.preview_rx = None;
            self.preview_tex = vec![None; p.pairs.len()];
            // Start on the moment that looks worst; that's where compression shows.
            self.preview_idx = p
                .worst()
                .and_then(|w| p.pairs.iter().position(|x| x.t_sec == w.t_sec))
                .unwrap_or(0);
            self.preview = Some(p);
            self.auto_clean_recording(ctx);
        }
        if let Some(w) = &self.watch {
            let (live, done) = match w.state.lock() {
                Ok(mut g) => (g.live.clone(), g.finished.take()),
                Err(_) => (None, None),
            };
            let started = self.live.is_none() && live.is_some();
            self.live = live;
            if started {
                // A stream started for a stream + record test is a test, not a real stream.
                let testing = self
                    .test_handle
                    .as_ref()
                    .map(|h| h.phase() == Phase::Measuring)
                    .unwrap_or(false)
                    || (self.test_handle.is_some() && self.show_bw_help);
                if !testing && self.st.inflight_auto && self.screen != Screen::Setup {
                    // Inside Preflight's own window only; it never pops over the game.
                    self.go(Screen::InFlight);
                }
                if self.st.bandwidth_test_pending && !testing {
                    // The test flag is still on, but no test is running: this may be a real stream nobody can see.
                    self.flag_question = true;
                    self.flag_real = false;
                    log::info!(
                        "Stream started with the bandwidth-test flag still marked on and no test running: asking"
                    );
                    self.notify_away(ctx, "Is this a real stream?", "The bandwidth-test flag may still be in your stream key. If it is, viewers can't see you. Open Preflight to check.");
                }
            }
            if let Some(mut l) = done {
                if self.st.bandwidth_test_pending {
                    l.bandwidth_test = true;
                    crate::streamwatch::resave(&l);
                }
                if self.flag_real {
                    // The user said this was a real stream: never label it a test.
                    l.bandwidth_test = false;
                    crate::streamwatch::resave(&l);
                }
                self.flag_question = false;
                self.flag_real = false;
                let p = crate::streamwatch::summarize(&l);
                if let Some(k) = l.rec_kbps.filter(|_| l.rec_seconds >= 120.0) {
                    self.st.rec_kbps_seen = Some(k);
                    self.st.save();
                    self.recompute();
                }
                self.toast(ctx, format!("Stream ended · {}", p.grade));
                self.notify_away(
                    ctx,
                    if l.bandwidth_test {
                        "Test stream ended"
                    } else {
                        "Stream ended"
                    },
                    &format!(
                        "{} · {}. Your stream summary is ready.",
                        crate::streamwatch::fmt_duration(p.duration),
                        p.grade
                    ),
                );
                if self.st.notify_test_done {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                        egui::UserAttentionType::Informational,
                    ));
                }
                self.streams = crate::streamwatch::past_streams();
                self.stream_sel = 0;
                self.stream_notice = Some(l);
            }
        }

        // Test finished or failed?
        let finished = self.test_handle.as_ref().and_then(|h| {
            let s = h.state.lock().ok()?;
            match &s.phase {
                Phase::Finished => s.result.clone().map(Ok),
                Phase::Failed(e) => Some(Err(e.clone())),
                _ => None,
            }
        });
        if let Some(f) = finished {
            self.test_handle = None;
            match f {
                Ok(r) => self.on_test_done(ctx, r),
                Err(e) if e == livetest::CANCELLED => {}
                Err(e) => {
                    let now = ctx.input(|i| i.time);
                    if e.contains("Couldn't reach OBS")
                        || e.contains("password")
                        || e.contains("rejected")
                    {
                        // Keep watching quietly; OBS may just not be open yet.
                        self.watch_retry_at = now + 5.0;
                    } else {
                        self.fail_banner = Some(format!("The test stopped early: {e}"));
                        self.watch_retry_at = now + 5.0;
                    }
                }
            }
        }
    }

    /// When the game's settings file was last saved (from the latest scan).
    pub(super) fn game_file_saved(&self, game: &str) -> Option<i64> {
        self.games
            .iter()
            .find(|g| g.name == game)
            .and_then(|g| g.files.iter().filter_map(|f| f.modified).max())
    }

    pub(super) fn on_test_done(&mut self, ctx: &egui::Context, r: TestResult) {
        let mut r = r;
        self.name_unknown_game(&mut r);
        if r.obs_settings.is_none() {
            r.obs_settings = self
                .obs
                .as_ref()
                .filter(|o| o.connected)
                .map(|o| o.enc_settings());
        }
        r.encodes_seen = std::mem::take(&mut self.encodes_seen);
        if !r.encodes_seen.is_empty() {
            log::info!(
                "Encodes seen during the test: {}",
                r.encodes_seen
                    .iter()
                    .map(|s| format!("{} {} {}", s.app, s.codec, s.res))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let Some(g) = r.game_running.clone() {
            r.settings_saved = self.game_file_saved(&g);
        }
        // Re-read the game's settings soon, so what's shown matches what it saved.
        self.start_poll(ctx);
        if let Some(p) = &r.recording_path {
            let size = r.recording_size.unwrap_or(0);
            self.st.pending_recordings.retain(|x| &x.path != p);
            self.st
                .pending_recordings
                .push(crate::config::PendingRecording {
                    path: p.clone(),
                    size,
                    created: crate::util::unix_now(),
                    remind_after: 0,
                });
            self.st.save();
        }
        self.start_preview(ctx, &r);
        self.test = Some(r.clone());
        self.want_new_test = false;
        self.baseline_findings = self.heavy_findings();
        self.baseline_mic_db = self.obs.as_ref().and_then(|o| o.mic_volume_db());
        self.baseline_obs = self.obs.clone();
        self.recompute();
        self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
        self.fix_idx = 0;
        self.fixed.clear();
        self.skipped.clear();
        self.detected = None;
        self.history.add(history::Entry {
            time: crate::util::unix_now(),
            kind: "test".into(),
            score: self.analysis.score,
            upload_avg: None,
            upload_low: None,
            gpu_avg: r.gpu_avg,
            cpu_avg: Some(r.cpu_avg),
            game: r.game_running.clone().or_else(|| r.game_guess.clone()),
            skipped_pct: if r.output_total > 0 {
                Some(r.output_skipped as f32 * 100.0 / r.output_total as f32)
            } else {
                None
            },
            maxed_pct: Some(maxed_pct(&r)),
            smooth: self.analysis.smoothness,
            setup: Some(self.analysis.setup_score),
        });
        Self::save_game_test(&r);
        // "Check my changes": compare with the test before, only for the same game.
        if let Some((before, before_score)) = self.prev_test.take() {
            let g = |t: &TestResult| t.game_running.clone().or_else(|| t.game_guess.clone());
            if g(&before) == g(&r) {
                let after_score = self.analysis.smoothness.unwrap_or(self.analysis.score);
                self.compare = Some(Compare {
                    before_score,
                    after_score,
                    before,
                    after: r.clone(),
                });
                log::info!(
                    "Check my changes: Smoothness {before_score} -> {after_score}, Setup {}",
                    self.analysis.setup_score
                );
            } else {
                log::info!(
                    "Check my changes: different game ({:?} -> {:?}), no before/after",
                    g(&before),
                    g(&r)
                );
            }
        }
        self.quick_next = false;
        // Remember this game's results, so switching games later shows them.
        if let Some(g) = r.game_running.clone().or_else(|| r.game_guess.clone()) {
            let open: Vec<String> = self
                .analysis
                .issues
                .iter()
                .filter(|i| !self.st.kept_issues.contains(&i.id))
                .map(|i| {
                    i.options
                        .iter()
                        .find(|o| o.suggested)
                        .map(|o| o.name.clone())
                        .unwrap_or_else(|| i.title.clone())
                })
                .collect();
            self.st.game_memory.insert(
                g,
                crate::config::GameMemory {
                    time: crate::util::unix_now(),
                    smoothness: self.analysis.smoothness.unwrap_or(0),
                    setup: self.analysis.setup_score,
                    verdict: self.analysis.verdict.clone(),
                    open,
                },
            );
            self.st.save();
        }
        if self.st.notify_test_done {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
            self.notify_away(
                ctx,
                "Test finished",
                &format!(
                    "{} Open Preflight to see what to fix.",
                    self.analysis.verdict
                ),
            );
        }
        self.toast(ctx, format!("Test finished · {}", self.analysis.verdict));
    }

    pub(super) fn start_test(&mut self, ctx: &egui::Context, system_only: bool) {
        self.test_counter += 1;
        self.side_test = None;
        let profiles = crate::games::profiles();
        let mut game_processes: Vec<(String, Vec<String>)> = profiles
            .iter()
            .filter(|p| p.id != "minecraft")
            .map(|p| (p.name.clone(), p.processes.clone()))
            .collect();
        // Games without a profile that Preflight has already found. Minecraft
        // runs as Java, so it only counts when it's actually running.
        game_processes.extend(
            self.games
                .iter()
                .filter(|g| g.auto.is_some() && (!g.id.starts_with("minecraft:") || g.running))
                .map(|g| (g.name.clone(), g.processes.clone())),
        );
        let audio_roles = self
            .obs
            .as_ref()
            .map(|o| {
                o.audio
                    .iter()
                    .map(|a| (a.name.clone(), a.role.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let target_secs = if self.quick_next {
            60.0
        } else {
            livetest::TARGET_SECONDS
        };
        let need_stream = self.show_bw_help && !system_only;
        let cfg = livetest::TestConfig {
            host: self.st.obs_host.clone(),
            port: self.st.obs_port,
            password: self.pw(),
            system_only,
            game_processes,
            audio_roles,
            target_secs,
            need_stream,
        };
        log::info!("Watching for a recording (system only: {system_only})");
        self.fail_banner = None;
        self.test_handle = Some(livetest::start(cfg, ctx.clone()));
    }

    pub(super) fn stop_watching_if_idle(&mut self) {
        if let Some(h) = &self.test_handle
            && matches!(h.phase(), Phase::WaitingForRecord | Phase::Connecting)
        {
            h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub(super) fn connect_obs(&mut self, ctx: &egui::Context) {
        self.start_watch(ctx);
        let (tx, rx) = channel();
        let (h, p, pw) = (self.st.obs_host.clone(), self.st.obs_port, self.pw());
        let hub = self.watch.as_ref().map(|w| w.cmd.clone());
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = obs_now(hub.as_ref(), &h, p, pw.as_deref());
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.conn_rx = Some(rx);
    }

    /// Open fixes (not kept by the user): (important, optional).
    pub(super) fn open_counts(&self) -> (usize, usize) {
        let open: Vec<&Issue> = self
            .analysis
            .issues
            .iter()
            .filter(|i| !self.st.kept_issues.contains(&i.id))
            .collect();
        let imp = open.iter().filter(|i| !analysis::is_optional(i)).count();
        (imp, open.len() - imp)
    }

    /// The fix being shown: always the latest version of that issue (a new
    /// style or a rescan changes its suggestion), found by its id.
    pub(super) fn current_issue(&self) -> Option<&Issue> {
        if self.returning {
            return None;
        }
        let q = self.fix_queue.get(self.fix_idx)?;
        self.analysis
            .issues
            .iter()
            .find(|i| i.id == q.id)
            .or(Some(q))
    }

    /// Did the user's change make the current issue go away?
    /// The value a game shows now for a setting (by its menu name).
    pub(super) fn game_value(g: &GameResult, setting: &str) -> Option<String> {
        let s = setting.to_lowercase();
        if s == "fps limit" {
            if g.findings.iter().any(|f| f.effect == "uncapped_fps") {
                return Some("Off".into());
            }
            return g.fps_cap.map(|c| c.to_string());
        }
        g.tunables
            .iter()
            .find(|t| t.setting.to_lowercase() == s)
            .map(|t| t.now.clone())
            .or_else(|| {
                g.preset
                    .as_ref()
                    .filter(|p| p.setting.to_lowercase() == s)
                    .map(|p| p.value.clone())
            })
            .or_else(|| {
                g.current
                    .iter()
                    .find(|c| c.setting.to_lowercase() == s)
                    .map(|c| c.value.clone())
            })
            .or_else(|| {
                g.findings
                    .iter()
                    .find(|f| f.label.to_lowercase().starts_with(&s))
                    .map(|f| crate::games::pretty_value(&f.value))
            })
    }

    /// Did the game save the change the user said they made?
    pub(super) fn check_confirm(&mut self, ctx: &egui::Context) {
        let Some(cw) = self.confirm.clone() else {
            return;
        };
        let now_t = ctx.input(|i| i.time);
        let saved = self.game_file_saved(&cw.game).unwrap_or(0);
        if saved <= cw.since {
            // Not saved yet. After 90 seconds, say so (and keep watching).
            if now_t - cw.started > 90.0 && cw.result.is_none() {
                let msg = format!(
                    "{} hasn't saved its settings since you clicked. Most games save when you press Apply or leave the settings menu.",
                    cw.game
                );
                self.confirm.as_mut().unwrap().result = Some((false, msg));
            }
            if now_t - cw.started > 600.0 {
                self.confirm = None;
            }
            return;
        }
        let Some(g) = self.games.iter().find(|g| g.name == cw.game) else {
            return;
        };
        let same = |a: &str, b: &str| {
            let (x, y) = (crate::autoread::norm(a), crate::autoread::norm(b));
            x == y || (!x.is_empty() && y.contains(&x)) || (!y.is_empty() && x.contains(&y))
        };
        let mut done: Vec<String> = Vec::new();
        let mut stuck: Vec<String> = Vec::new();
        let mut moved: Vec<String> = Vec::new();
        for c in &cw.changes {
            let Some(v) = Self::game_value(g, &c.setting) else {
                continue;
            };
            if same(&v, &c.to) {
                done.push(format!("{} is now {v}", c.setting));
            } else if c.now.as_deref().map(|n| same(n, &v)).unwrap_or(false) {
                stuck.push(format!("{} is still {v}", c.setting));
            } else {
                moved.push(format!("{} is now {v} (suggested: {})", c.setting, c.to));
            }
        }
        let when = crate::util::fmt_local(saved);
        let result = if !stuck.is_empty() {
            (
                false,
                format!(
                    "{} saved its settings at {when}, but {}. Did you press Apply in the game?",
                    cw.game,
                    stuck.join(", ")
                ),
            )
        } else if done.is_empty() && moved.is_empty() {
            (true, format!("{} saved its settings at {when}.", cw.game))
        } else {
            let mut parts = done;
            parts.extend(moved);
            (
                true,
                format!(
                    "Preflight sees it: {}. Saved by {} at {when}.",
                    parts.join(", "),
                    cw.game
                ),
            )
        };
        log::info!("Change check: {}", result.1);
        if result.0 {
            self.toast(ctx, result.1.clone());
        }
        self.confirm = Some(ConfirmWatch {
            since: saved,
            result: Some(result),
            ..cw
        });
    }

    pub(super) fn check_fix_detected(&mut self) {
        if self.step != 3 || self.detected.is_some() {
            return;
        }
        let Some(cur) = self.current_issue().cloned() else {
            return;
        };
        // Measured problems (from the test) can't be re-measured without a new
        // test, so for those a change to the relevant setting counts.
        // Any OBS encoder or video setting counts: preset, multipass, tuning,
        // rate control, keyframes, filter... not just encoder and bitrate.
        let obs_diff: Vec<String> = match (
            self.baseline_obs.as_ref().filter(|o| o.connected),
            self.obs.as_ref().filter(|o| o.connected),
        ) {
            (Some(b), Some(n)) => b.enc_settings().changes_to(&n.enc_settings()),
            _ => Vec::new(),
        };
        let obs_changed = !obs_diff.is_empty();
        let game_changed =
            !self.baseline_findings.is_empty() && self.heavy_findings() != self.baseline_findings;
        let setting_changed = match cur.id.as_str() {
            "encoder_lag" => obs_changed,
            "gpu_maxed" | "gpu_maybe" | "gpu_near" => game_changed,
            "cpu_high" => obs_changed || game_changed,
            "heat" => game_changed,
            _ => false,
        };
        if !setting_changed && self.analysis.issues.iter().any(|i| i.id == cur.id) {
            return;
        }
        let text = match cur.id.as_str() {
            "gpu_maxed" | "gpu_maybe" | "gpu_near" => {
                let now = self.heavy_findings();
                let gone: Vec<String> = self
                    .baseline_findings
                    .difference(&now)
                    .map(|s| s.split('=').next().unwrap_or("").to_string())
                    .collect();
                if gone.is_empty() {
                    "your game settings changed.".to_string()
                } else {
                    format!("you changed: {}.", gone.join(", ").to_lowercase())
                }
            }
            "audio_balance" => {
                let d = match (
                    self.baseline_mic_db,
                    self.obs.as_ref().and_then(|o| o.mic_volume_db()),
                ) {
                    (Some(b), Some(n)) => n - b,
                    _ => 0.0,
                };
                if d > 0.5 {
                    format!("your mic went up {d:.0} dB in OBS.")
                } else {
                    "your mic and game levels are balanced now.".into()
                }
            }
            "encoder" => format!(
                "OBS is now using {}.",
                self.obs
                    .as_ref()
                    .map(|o| o.encoder_kind.clone())
                    .unwrap_or_default()
            ),
            "encoder_lag" | "cpu_high" if obs_changed => {
                format!("you changed {} in OBS.", obs_diff.join(", "))
            }
            "cpu_high" | "heat" => "your game settings changed.".into(),
            "recording_mismatch" => "your recording now uses the stream's settings.".into(),
            "rate_control" => format!(
                "rate control is now {}.",
                self.obs
                    .as_ref()
                    .map(|o| o.rate_control.to_uppercase())
                    .unwrap_or_default()
            ),
            "keyframes" => format!(
                "keyframes are now every {} seconds.",
                self.obs.as_ref().and_then(|o| o.keyint_sec).unwrap_or(0)
            ),
            "preset" => format!(
                "the encoder preset is now {}.",
                self.obs
                    .as_ref()
                    .map(|o| o.preset.clone())
                    .unwrap_or_default()
            ),
            "downscale" => format!(
                "the downscale filter is now {}.",
                self.obs
                    .as_ref()
                    .map(|o| o.scale_type.clone())
                    .unwrap_or_default()
            ),
            "display_capture" => "your scene no longer uses full-screen capture.".into(),
            "bitrate_network" | "bitrate_platform" => format!(
                "your bitrate is now {} kbps.",
                self.obs.as_ref().and_then(|o| o.bitrate_kbps).unwrap_or(0)
            ),
            _ => "this looks fixed now.".into(),
        };
        log::info!("Fix detected for {}: {text}", cur.id);
        self.note_change(format!(
            "{}: {}",
            analysis::impact_of(&cur.id),
            text.trim_end_matches('.')
        ));
        self.fixed.push(cur.id.clone());
        self.detected = Some(text);
    }

    pub(super) fn next_line(&self) -> String {
        match self.screen {
            Screen::Setup => match self.setup_screen {
                1 => "Pick the style closest to how you stream, then click Continue.".into(),
                2 => "Check that the monitor you play on is selected.".into(),
                _ => {
                    if self.obs.as_ref().map(|o| o.connected).unwrap_or(false) {
                        "All set. Start your first pre-flight.".into()
                    } else {
                        "Paste your OBS password and click Test connection.".into()
                    }
                }
            },
            Screen::Preflight => match self.step {
                1 => {
                    if self.scan.is_some() {
                        "Scanning your setup. This takes about 30 seconds.".into()
                    } else {
                        "Your scan is done. Continue to the test.".into()
                    }
                }
                2 => {
                    let phase = self.test_handle.as_ref().map(|h| h.phase());
                    match phase {
                        Some(Phase::Measuring) => {
                            let (el, shots) = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| (s.elapsed, s.shots))).unwrap_or((0.0, 0));
                            let target = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| s.target)).unwrap_or(livetest::TARGET_SECONDS);
                            if el < target {
                                "Keep playing. Preflight is measuring.".into()
                            } else if shots < livetest::PREVIEW_SHOTS {
                                "Keep playing a few more seconds while Preflight grabs frames for the viewer preview.".into()
                            } else {
                                "All done. Stop recording in OBS.".into()
                            }
                        }
                        Some(Phase::WaitingForRecord) if self.show_bw_help => "Start your game, then Start Streaming and Start Recording in OBS (any order).".into(),
                        Some(Phase::WaitingForRecord) => "Start your game, get into gameplay, then press Record in OBS.".into(),
                        _ if self.test.is_some() && !self.want_new_test => "Test done. Continue to fix what it found.".into(),
                        _ if !self.obs.as_ref().map(|o| o.connected).unwrap_or(false) => "Open OBS (with its WebSocket server on) so Preflight can watch for your recording.".into(),
                        _ => "Start your game, get into gameplay, then press Record in OBS.".into(),
                    }
                }
                3 => match (self.current_issue(), &self.detected) {
                    (Some(_), Some(_)) => "Nice. Go to the next issue.".into(),
                    (Some(i), None) if analysis::is_optional(i) && !self.optional_ok => "You've been through the important issues. Go through the optional ones, or continue to Ready to go live.".into(),
                    (Some(_), None) => "Make the change shown above. Preflight usually notices by itself.".into(),
                    (None, _) => "All issues handled. Next: Ready to go live.".into(),
                },
                _ => {
                    if self.live.is_some() {
                        "You're live. Preflight watches quietly from the tray and writes your stream summary.".into()
                    } else if self.preflight_done {
                        "Start streaming in OBS whenever you like.".into()
                    } else {
                        "Go down the checklist: fix what Preflight found or keep it as is, then tick the rest.".into()
                    }
                }
            },
            Screen::Results => {
                let (imp, opt) = self.open_counts();
                if imp + opt == 0 { "Nothing to change right now.".into() } else { format!("{}. Go through them one at a time in your pre-flight.", count_words(imp, opt)) }
            }
            Screen::InFlight => {
                if self.live.is_some() {
                    "Nothing to do. Preflight writes your stream summary when you end the stream.".into()
                } else {
                    "Not live. This page opens by itself when you start streaming in OBS.".into()
                }
            }
            Screen::PostFlight => {
                if self.live.is_some() {
                    "You're live. Preflight watches quietly from the tray.".into()
                } else if self.streams.is_empty() {
                    "Leave Preflight open while you stream. You'll get a summary here when you end it.".into()
                } else {
                    "Here's how your stream went, and what to change next time.".into()
                }
            }
            Screen::Home => {
                let (imp, opt) = self.open_counts();
                if !self.obs_connected() {
                    "Connect OBS to get started. Everything else works without it.".into()
                } else if self.test.is_none() && !self.st.game_memory.is_empty() {
                    "Run a test with this game to measure it, or go through the fixes OBS's settings show.".into()
                } else if self.test.is_none() {
                    "Start your game, then run your first test.".into()
                } else if imp + opt > 0 {
                    format!("{}. Go through them one at a time.", count_words(imp, opt))
                } else {
                    "Ready. Start streaming in OBS whenever you like.".into()
                }
            }
            Screen::History => "What you changed and what it did. It all stays on this PC.".into(),
            Screen::Settings => "Change anything here at any time.".into(),
            Screen::Help => "Something not working? Click Report a problem.".into(),
        }
    }
}
