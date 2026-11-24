//! The app window: menu, first-run setup, the guided pre-flight, results,
//! history, settings, help, and the one-box problem report.

use crate::analysis::{self, Analysis, Issue, Status};
use crate::config::{HiddenOption, LastPreflight, Platform, Settings, Style};
use crate::games::GameResult;
use crate::hardware::HardwareInfo;
use crate::history::{self, History};
use crate::livetest::{self, Phase, TestHandle, TestResult};
use crate::monitors::Monitor;
use crate::network::NetResult;
use crate::obs::ObsSnapshot;
use crate::report::{self, SelfTest, TestStatus};
use crate::theme::{self, *};
use eframe::egui::{self, Color32, RichText, ScrollArea, Ui, Vec2};
use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

/// What the pinned Next button (and one-click error buttons) do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NextAct {
    Step(u8),
    Screen(Screen),
    NextIssue,
    CheckChanges,
    /// Open Settings at the OBS connection.
    ObsSettings,
    /// Open Help at "OBS won't connect".
    ObsHelp,
    /// A button of the current fix ("manual" = I changed it, "skip_optional").
    FixAction(&'static str),
    /// Hide the window to the system tray.
    ToTray,
    /// Start watching OBS for the test (it begins when OBS records).
    ArmTest,
}

/// Before/after numbers from "Check my changes".
#[derive(Clone, Debug)]
struct Compare {
    before_score: u8,
    after_score: u8,
    before: TestResult,
    after: TestResult,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Setup,
    Preflight,
    Results,
    PostFlight,
    History,
    Settings,
    Help,
}

enum ScanMsg {
    Step(String),
    Hardware(HardwareInfo),
    Monitors(Vec<Monitor>),
    Games(Vec<GameResult>),
    Procs(Vec<String>),
    Obs(Option<ObsSnapshot>, Option<String>),
    Net(NetResult),
    Done,
}

struct ScanJob {
    rx: Receiver<ScanMsg>,
    step: String,
    net_progress: Arc<Mutex<f32>>,
    _cancel: Arc<AtomicBool>,
}

/// Light refresh while the user works through steps 2–4: game settings,
/// running games and OBS, so Preflight can notice changes on its own.
struct PollJob {
    rx: Receiver<(Vec<GameResult>, Option<ObsSnapshot>, Option<String>)>,
}

struct ReportBox {
    context: String,
    note: String,
    saved: Option<(String, std::path::PathBuf)>,
    error: Option<String>,
    show_contents: bool,
    copied: bool,
}

/// After "I changed it" on a game fix: watch the game's settings file and
/// say whether the change really saved.
#[derive(Clone, Debug)]
struct ConfirmWatch {
    game: String,
    changes: Vec<analysis::Change>,
    /// When the game last saved before the click.
    since: i64,
    started: f64,
    /// (all saved, message)
    result: Option<(bool, String)>,
}

pub struct App {
    fleet: crate::fleet::Fleet,
    /// Every video encoder on the PC and who's using it (refreshed while
    /// Your setup or a test is on screen).
    encoders: Vec<crate::encoders::EncoderInfo>,
    enc_rx: Option<std::sync::mpsc::Receiver<Vec<crate::encoders::EncoderInfo>>>,
    enc_next: f64,
    encodes_seen: Vec<crate::encoders::EncSession>,
    /// Platforms whose private test mode the user has set up this session.
    private_ok: std::collections::BTreeSet<String>,
    confirm: Option<ConfirmWatch>,
    st: Settings,
    password: String,
    screen: Screen,
    setup_screen: u8,
    step: u8,
    hw: Option<HardwareInfo>,
    monitors: Vec<Monitor>,
    games: Vec<GameResult>,
    obs: Option<ObsSnapshot>,
    obs_error: Option<String>,
    other_recorders: Vec<String>,
    net: Option<NetResult>,
    test: Option<TestResult>,
    test_handle: Option<TestHandle>,
    watch_paused: bool,
    watch_retry_at: f64,
    want_new_test: bool,
    analysis: Analysis,
    scan: Option<ScanJob>,
    first_scan_done: bool,
    last_scan: Option<i64>,
    poll: Option<PollJob>,
    poll_next: f64,
    // Fix step
    fix_queue: Vec<Issue>,
    fix_idx: usize,
    fixed: Vec<String>,
    skipped: Vec<String>,
    detected: Option<String>,
    picks: std::collections::HashMap<String, String>,
    /// The user chose to go through the optional issues.
    optional_ok: bool,
    show_bw_help: bool,
    /// The test before "Check my changes", and the comparison once done.
    prev_test: Option<(TestResult, u8)>,
    compare: Option<Compare>,
    tree_pf_open: bool,
    pairing: Option<crate::pairing::Pairing>,
    /// "host" (streaming PC) or "client" (gaming PC) for the running pairing.
    pairing_kind: &'static str,
    pair_code_input: String,
    /// Streaming PC: showing a code for the gaming PC to type.
    pair_offer: bool,
    /// Gaming PC: the user clicked Pair.
    pair_try: bool,
    pair_status: String,
    side_measure: Option<crate::pairing::SideMeasure>,
    side_monitor: Option<crate::hardware::Monitor>,
    side_at: f64,
    /// Streaming PC: the gaming PC's numbers from the last test.
    side_test: Option<crate::pairing::SideTest>,
    test_counter: u64,
    reading_hist: std::collections::VecDeque<ReadingPoint>,
    reading_at: f64,
    tray: Option<crate::tray::Tray>,
    tray_tried: bool,
    win_hwnd: isize,
    hidden: bool,
    quitting: bool,
    /// Can the window be hidden (Windows, X11), or only minimized (Wayland)?
    can_hide: bool,
    window_known: bool,
    tray_hint_shown: bool,
    /// A stream started with the test flag still marked on: asking if it's real.
    flag_question: bool,
    /// The user said the running stream is real (flag removed).
    flag_real: bool,
    queued_fix_action: Option<String>,
    go_ready: bool,
    tree_post_open: bool,
    preview_split: f32,
    help_open: Option<usize>,
    pending_act: Option<NextAct>,
    /// The "Remove Preflight from this PC" window: recordings (path, size, delete?), remove the program, error.
    uninstall: Option<(Vec<(String, u64, bool)>, bool, Option<String>)>,
    scroll_to_preview: bool,
    /// The next test is a 1-minute quick re-test (Check my changes).
    quick_next: bool,
    /// Game whose "keep as is" choices are loaded in st.kept_issues.
    kept_game: Option<String>,
    /// The user chose to keep showing the old game's results while this game runs.
    switch_dismissed: Option<String>,
    /// Selected answer for "About what FPS do you get?" while editing.
    fps_ask_open: bool,
    show_other_fixes: bool,
    baseline_findings: BTreeSet<String>,
    baseline_mic_db: Option<f32>,
    baseline_obs: Option<ObsSnapshot>,
    // Final checks
    tick_dnd: bool,
    tick_title: bool,
    preflight_done: bool,
    returning: bool,
    // Other
    history: History,
    selftests: Vec<SelfTest>,
    report: Option<ReportBox>,
    fail_banner: Option<String>,
    update_rx: Option<Receiver<Result<crate::update::UpdateInfo, String>>>,
    update_result: Option<Result<crate::update::UpdateInfo, String>>,
    /// Installing an update: None while downloading, then the result.
    update_install: Option<Receiver<Result<std::path::PathBuf, String>>>,
    update_done: Option<Result<std::path::PathBuf, String>>,
    install_result: Option<Result<std::path::PathBuf, String>>,
    install_remove_old: bool,
    open_uninstall: bool,
    /// "Later" on the update banner (this session only).
    update_later: bool,
    update_notified: bool,
    conn_rx: Option<Receiver<(Option<ObsSnapshot>, Option<String>)>>,
    toast: Option<(String, f64)>,
    show_recording_popup: bool,
    pending_crashes: usize,
    confirm_clear: bool,
    setup_name: String,
    // Viewer preview
    preview: Option<crate::preview::Preview>,
    preview_rx: Option<Receiver<crate::preview::Preview>>,
    /// Loaded only when a moment is first shown.
    preview_tex: Vec<Option<(Option<egui::TextureHandle>, Option<egui::TextureHandle>)>>,
    preview_idx: usize,
    preview_zoom: bool,
    // Post-flight
    watch: Option<crate::streamwatch::WatchHandle>,
    live: Option<crate::streamwatch::LiveStatus>,
    stream_notice: Option<crate::streamwatch::StreamLog>,
    streams: Vec<crate::streamwatch::StreamLog>,
    stream_sel: usize,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, menu_added: bool, open_uninstall: bool) -> Self {
        theme::apply(&cc.egui_ctx);
        let st = Settings::load();
        cc.egui_ctx.set_zoom_factor(st.text_scale);
        let password = if st.obs_remember_password { crate::config::load_secret().unwrap_or_default() } else { String::new() };
        let now = crate::util::unix_now();
        let show_popup = st.remind_delete_recording && st.pending_recordings.iter().any(|p| p.remind_after <= now);
        let screen = if st.setup_done { Screen::Preflight } else { Screen::Setup };
        let pending_crashes = crate::logger::pending_crashes().len();
        // The last kind of test picked (a quick check only follows a change).
        let last_stream_record = st.last_test == "stream_record";
        let mut app = Self {
            st,
            password,
            screen,
            setup_screen: 1,
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
            window_known: false,
            tray_hint_shown: false,
            flag_question: false,
            flag_real: false,
            queued_fix_action: None,
            go_ready: false,
            tree_post_open: true,
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
            update_later: false,
            update_notified: false,
            conn_rx: None,
            toast: None,
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
            app.toast = Some(("Added Preflight to your app menu".into(), 6.0));
        }
        app
    }

    // -----------------------------------------------------------------------
    // Analysis helpers

    fn target_game(&self) -> Option<&GameResult> {
        let by_test = self.test.as_ref().and_then(|t| t.game_running.clone());
        let guess = self.test.as_ref().and_then(|t| t.game_guess.clone());
        self.games
            .iter()
            .find(|g| Some(&g.name) == by_test.as_ref())
            .or_else(|| guess.as_ref().and_then(|gs| self.games.iter().find(|g| g.processes.iter().any(|p| p == gs))))
            .or_else(|| self.games.iter().find(|g| g.running))
            .or_else(|| self.games.iter().find(|g| !g.findings.is_empty()))
    }

    fn heavy_findings(&self) -> BTreeSet<String> {
        self.target_game()
            .map(|g| {
                g.findings
                    .iter()
                    .filter(|f| matches!(f.effect.as_str(), "heavy" | "rt_on" | "uncapped_fps" | "upscaler_off"))
                    .map(|f| format!("{}={}", f.label, f.value))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn gpu_changed(&self) -> bool {
        self.test.is_some() && !self.baseline_findings.is_empty() && self.heavy_findings() != self.baseline_findings
    }

    /// The last test, with the mic level adjusted for any change the user
    /// made to the OBS mic slider since then.
    fn adjusted_test(&self) -> Option<TestResult> {
        let mut t = self.test.clone()?;
        if let (Some(b), Some(now), Some(m)) = (self.baseline_mic_db, self.obs.as_ref().and_then(|o| o.mic_volume_db()), t.mic_db) {
            t.mic_db = Some(m + (now - b));
        }
        if let (Some(b), Some(now), Some(d)) = (
            self.baseline_obs.as_ref().and_then(|o| o.desktop_volume_db()),
            self.obs.as_ref().and_then(|o| o.desktop_volume_db()),
            t.desktop_db,
        ) {
            t.desktop_db = Some(d + (now - b));
        }
        Some(t)
    }

    /// The game the results are about: the tested game, else the one running.
    fn current_game(&self) -> Option<String> {
        self.test.as_ref().and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone())).or_else(|| self.games.iter().find(|g| g.running).map(|g| g.name.clone()))
    }

    /// Program names of games Preflight has no profile for, seen in tests,
    /// so their settings are read even when they aren't running.
    fn game_hints(&self) -> Vec<String> {
        let mut v: Vec<String> = self.test.as_ref().and_then(|t| t.game_guess.clone()).into_iter().collect();
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
    fn name_unknown_game(&mut self, r: &mut TestResult) {
        if r.game_running.is_some() {
            return;
        }
        let Some(guess) = r.game_guess.clone() else { return };
        let matches = |g: &crate::games::GameResult| g.auto.is_some() && g.processes.iter().any(|p| p == &guess || crate::games::process_matches(&guess, p));
        if !self.games.iter().any(|g| matches(g)) {
            for g in crate::games::read_unknown(std::slice::from_ref(&guess)) {
                self.games.retain(|x| x.id != g.id);
                self.games.push(g);
            }
        }
        if let Some(g) = self.games.iter().find(|g| matches(g)) {
            log::info!("Game without a profile: {guess} is {} ({} settings read)", g.name, g.settings_read);
            r.game_running = Some(g.name.clone());
            if !self.st.auto_games.contains(&guess) {
                self.st.auto_games.push(guess.clone());
                if self.st.auto_games.len() > 20 {
                    self.st.auto_games.remove(0);
                }
            }
        }
    }

    fn game_test_path(game: &str) -> std::path::PathBuf {
        let slug: String = game.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
        crate::util::dirs().tests.join(format!("{slug}.json"))
    }

    /// Keeps each game's last full test, so switching games shows its results.
    fn save_game_test(t: &TestResult) {
        if let Some(g) = t.game_running.clone().or_else(|| t.game_guess.clone()) {
            if let Ok(j) = serde_json::to_string(t) {
                let _ = std::fs::write(Self::game_test_path(&g), j);
            }
        }
    }

    fn load_game_test(game: &str) -> Option<TestResult> {
        let t = std::fs::read_to_string(Self::game_test_path(game)).ok()?;
        serde_json::from_str(&t).ok()
    }

    /// Games with saved results.
    fn saved_games(&self) -> Vec<String> {
        self.st.game_memory.keys().filter(|g| Self::game_test_path(g).exists()).cloned().collect()
    }

    /// Shows another game's saved results (or none yet).
    fn show_game(&mut self, game: &str) {
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
    fn sync_kept(&mut self) {
        let game = self.current_game();
        if game == self.kept_game {
            if let Some(g) = &game {
                let ids: Vec<String> = self.st.kept_issues.iter().filter(|i| analysis::place_of(i) == analysis::Place::Game).cloned().collect();
                if self.st.kept_by_game.get(g) != Some(&ids) {
                    self.st.kept_by_game.insert(g.clone(), ids);
                }
            }
            return;
        }
        if let Some(old) = &self.kept_game {
            let ids: Vec<String> = self.st.kept_issues.iter().filter(|i| analysis::place_of(i) == analysis::Place::Game).cloned().collect();
            self.st.kept_by_game.insert(old.clone(), ids);
        }
        self.st.kept_issues.retain(|i| analysis::place_of(i) != analysis::Place::Game);
        if let Some(g) = &game {
            if let Some(ids) = self.st.kept_by_game.get(g).cloned() {
                self.st.kept_issues.extend(ids);
            }
            // Each game keeps its own style.
            let style = match self.st.style_by_game.get(g).copied() {
                Some(s) => Some(s),
                None if self.st.default_style != Style::Custom => {
                    // First time: start from the default style for new games.
                    self.st.style_by_game.insert(g.clone(), self.st.default_style);
                    Some(self.st.default_style)
                }
                None => None,
            };
            if let Some(style) = style {
                if style != self.st.style && style != Style::Custom {
                    self.st.apply_style(style);
                    log::info!("Style for {g}: {style:?}");
                }
            }
        }
        self.kept_game = game;
    }

    fn recompute(&mut self) {
        self.sync_kept();
        // One setup for all games: combine every game's saved test.
        let tests: Vec<TestResult> = self.saved_games().iter().filter_map(|g| Self::load_game_test(g)).collect();
        self.fleet = crate::fleet::combine(&tests, &self.games, self.obs.as_ref());
        self.st.fleet_motion = if self.fleet.tested.len() > 1 { self.fleet.motion.clone() } else { String::new() };
        self.st.fleet_busy = self.fleet.tested.len() > 1 && self.fleet.busy;
        let t = self.adjusted_test();
        let gc = self.gpu_changed();
        self.analysis = analysis::analyze(&self.st, self.hw.as_ref(), &self.games, self.obs.as_ref(), self.net.as_ref(), t.as_ref(), gc);
    }

    fn signature(&self) -> String {
        let obs = self.obs.as_ref().map(|o| o.signature()).unwrap_or_default();
        let games: Vec<String> = self.games.iter().filter(|g| g.relevant()).map(|g| format!("{}:{:?}", g.id, g.findings.iter().map(|f| (&f.label, &f.value)).collect::<Vec<_>>())).collect();
        format!("{obs}#{}#{}", games.join(";"), self.st.monitor_hz)
    }

    fn note_change(&mut self, text: String) {
        self.st.change_log.push(crate::config::ChangeNote { time: crate::util::unix_now(), text });
        let n = self.st.change_log.len();
        if n > 200 {
            self.st.change_log.drain(..n - 200);
        }
        self.st.save();
    }

    fn toast(&mut self, ctx: &egui::Context, s: impl Into<String>) {
        self.toast = Some((s.into(), ctx.input(|i| i.time) + 4.0));
    }

    fn pw(&self) -> Option<String> {
        if self.password.is_empty() { None } else { Some(self.password.clone()) }
    }

    fn open_report(&mut self, context: impl Into<String>) {
        self.report = Some(ReportBox { context: context.into(), note: String::new(), saved: None, error: None, show_contents: false, copied: false });
    }

    // -----------------------------------------------------------------------
    // Background work

    fn start_scan(&mut self, ctx: egui::Context, with_network: bool) {
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
        // Reuse a good upload result from the last 15 minutes instead of
        // testing again on every scan.
        let fresh = self.net.as_ref().map(|n| n.ok && crate::util::unix_now() - n.tested_at < 15 * 60).unwrap_or(false);
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
                send(ScanMsg::Games(crate::games::scan(&procs, &running, &hints, &mut sys)));
                send(ScanMsg::Step("Connecting to OBS…".into()));
                let (snap, err) = obs_now(hub.as_ref(), &host, port, pw.as_deref());
                send(ScanMsg::Obs(snap, err));
                if do_net {
                    send(ScanMsg::Step("Testing your upload speed (up to 15 seconds)…".into()));
                    send(ScanMsg::Net(crate::network::run(np, cn)));
                }
                send(ScanMsg::Done);
            })
            .expect("spawn scan");
        self.scan = Some(ScanJob { rx, step: "Starting…".into(), net_progress, _cancel: cancel });
        log::info!("Scan started (network test: {do_net})");
    }

    /// (Re)starts the post-flight watcher with the current OBS details.
    fn start_watch(&mut self, ctx: &egui::Context) {
        let profiles = crate::games::profiles();
        let cfg = crate::streamwatch::WatchConfig {
            host: self.st.obs_host.clone(),
            port: self.st.obs_port,
            password: self.pw(),
            game_processes: profiles.iter().map(|p| (p.name.clone(), p.processes.clone())).collect(),
        };
        self.watch = Some(crate::streamwatch::start(cfg, ctx.clone()));
    }

    fn start_preview(&mut self, ctx: &egui::Context, t: &TestResult) {
        if t.preview_shots.is_empty() {
            self.preview = Some(crate::preview::build(&[], None, 60.0, None));
            self.auto_clean_recording(ctx);
            return;
        }
        let (tx, rx) = channel();
        let shots = t.preview_shots.clone();
        let rec = t.recording_path.clone();
        let fps = t.stream_fps.unwrap_or(60.0);
        // The stream's settings, to re-create the viewer picture when the
        // test recording used different (higher-quality) settings.
        let sim = self.obs.as_ref().and_then(|o| {
            let (w, h) = o.output_res?;
            let rc = o.rec_rate_control.to_uppercase();
            let hq = (matches!(rc.as_str(), "CQP" | "CRF" | "ICQ" | "CQVBR" | "LOSSLESS") && o.rec_cq.map(|q| q <= 22).unwrap_or(true)) || o.rec_bitrate_kbps.map(|b| b >= 20000).unwrap_or(false);
            Some(crate::preview::StreamSim { bitrate_kbps: o.bitrate_kbps?, width: w, height: h, fps: o.fps.unwrap_or(fps), recording_differs: o.recording_uses_stream_settings == Some(false), recording_high_quality: hq })
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

    fn start_poll(&mut self, ctx: &egui::Context) {
        if self.poll.is_some() || self.scan.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (h, p, pw) = (self.st.obs_host.clone(), self.st.obs_port, self.pw());
        let hub = self.watch.as_ref().map(|w| w.cmd.clone());
        let hints = self.game_hints();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let mut sys = crate::hardware::new_system();
            let procs = crate::hardware::running_process_names(&mut sys);
            let running = crate::hardware::game_processes(&mut sys);
            let games = crate::games::scan(&procs, &running, &hints, &mut sys);
            let (o, e) = obs_now(hub.as_ref(), &h, p, pw.as_deref());
            let _ = tx.send((games, o, e));
            c.request_repaint();
        });
        self.poll = Some(PollJob { rx });
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        let mut done = false;
        let mut changed = false;
        if let Some(job) = self.scan.as_mut() {
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    ScanMsg::Step(s) => job.step = s,
                    ScanMsg::Hardware(h) => self.hw = Some(h),
                    ScanMsg::Monitors(m) => {
                        if self.st.monitor_auto {
                            if let Some(pick) = m.iter().find(|x| x.name == self.st.monitor_name).or_else(|| m.first()) {
                                self.st.monitor_name = pick.name.clone();
                                self.st.monitor_hz = pick.max_hz.max(pick.current_hz).max(30);
                            }
                        }
                        self.monitors = m;
                    }
                    ScanMsg::Games(g) => {
                        self.history.note_settings(&g);
                        self.games = g;
                    }
                    ScanMsg::Procs(p) => self.other_recorders = p,
                    ScanMsg::Obs(s, e) => {
                        self.obs = s;
                        self.obs_error = e;
                    }
                    ScanMsg::Net(n) => self.net = Some(n),
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
                    if recent && lp.signature == self.signature() && self.screen == Screen::Preflight {
                        self.returning = true;
                        self.step = 4;
                        log::info!("Returning user: nothing changed since the last pre-flight");
                    }
                }
            }
            self.st.save();
        }

        if let Some(p) = &self.poll {
            if let Ok((g, o, e)) = p.rx.try_recv() {
                self.poll = None;
                self.games = g;
                if o.is_some() || self.obs.is_none() {
                    self.obs = o;
                }
                self.obs_error = e;
                self.recompute();
                self.check_fix_detected();
                self.check_confirm(ctx);
            }
        }
        if let Some(rx) = &self.conn_rx {
            if let Ok((s, e)) = rx.try_recv() {
                self.obs = s;
                self.obs_error = e;
                self.conn_rx = None;
                self.recompute();
            }
        }
        if let Some(rx) = &self.update_install {
            if let Ok(r) = rx.try_recv() {
                self.update_install = None;
                if let Ok(p) = &r {
                    // Start the new version, then close this one.
                    if crate::install::launch(p) {
                        log::info!("Restarting into the new version");
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                self.update_done = Some(r);
            }
        }
        if self.open_uninstall {
            self.open_uninstall = false;
            let recs = self.st.pending_recordings.iter().filter(|r| std::path::Path::new(&r.path).exists()).map(|r| (r.path.clone(), r.size, false)).collect();
            self.uninstall = Some((recs, true, None));
        }
        // Once a day, if allowed: read GitHub's public release list. Nothing about you is sent.
        let now_unix = crate::util::unix_now();
        if self.st.updates_auto && self.update_rx.is_none() && self.update_result.is_none() && now_unix - self.st.last_update_check > 24 * 3600 {
            self.st.last_update_check = now_unix;
            self.st.save();
            let (tx, rx) = channel();
            let beta = self.st.updates_beta;
            std::thread::spawn(move || {
                let _ = tx.send(crate::update::check(beta));
            });
            self.update_rx = Some(rx);
        }
        if !self.update_notified {
            if let Some(Ok(u)) = &self.update_result {
                if u.newer {
                    self.update_notified = true;
                    let (t, b) = (format!("Stream Preflight v{} is available", u.latest), "Open Preflight and click Update now.".to_string());
                    self.notify_away(ctx, &t, &b);
                }
            }
        }
        if let Some(rx) = &self.update_rx {
            if let Ok(r) = rx.try_recv() {
                self.update_result = Some(r);
                self.update_rx = None;
            }
        }

        if let Some(rx) = &self.preview_rx {
            if let Ok(p) = rx.try_recv() {
                self.preview_rx = None;
                self.preview_tex = vec![None; p.pairs.len()];
                // Start on the moment that looks worst; that's where compression shows.
                self.preview_idx = p.worst().and_then(|w| p.pairs.iter().position(|x| x.t_sec == w.t_sec)).unwrap_or(0);
                self.preview = Some(p);
                self.auto_clean_recording(ctx);
            }
        }
        if let Some(w) = &self.watch {
            let (live, done) = match w.state.lock() {
                Ok(mut g) => (g.live.clone(), g.finished.take()),
                Err(_) => (None, None),
            };
            let started = self.live.is_none() && live.is_some();
            self.live = live;
            if started {
                let testing = self.test_handle.is_some();
                if self.st.bandwidth_test_pending && !testing {
                    // The test flag is still on, but no test is running: this may be a real stream nobody can see.
                    self.flag_question = true;
                    self.flag_real = false;
                    log::info!("Stream started with the bandwidth-test flag still marked on and no test running: asking");
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
                self.toast(ctx, format!("Stream ended · {}", p.grade));
                self.notify_away(ctx, if l.bandwidth_test { "Test stream ended" } else { "Stream ended" }, &format!("{} · {}. Your post-flight summary is ready.", crate::streamwatch::fmt_duration(p.duration), p.grade));
                if self.st.notify_test_done {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
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
                    if e.contains("Couldn't reach OBS") || e.contains("password") || e.contains("rejected") {
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
    fn game_file_saved(&self, game: &str) -> Option<i64> {
        self.games.iter().find(|g| g.name == game).and_then(|g| g.files.iter().filter_map(|f| f.modified).max())
    }

    fn on_test_done(&mut self, ctx: &egui::Context, r: TestResult) {
        let mut r = r;
        self.name_unknown_game(&mut r);
        if r.obs_settings.is_none() {
            r.obs_settings = self.obs.as_ref().filter(|o| o.connected).map(|o| o.enc_settings());
        }
        r.encodes_seen = std::mem::take(&mut self.encodes_seen);
        if !r.encodes_seen.is_empty() {
            log::info!("Encodes seen during the test: {}", r.encodes_seen.iter().map(|s| format!("{} {} {}", s.app, s.codec, s.res)).collect::<Vec<_>>().join(", "));
        }
        if let Some(g) = r.game_running.clone() {
            r.settings_saved = self.game_file_saved(&g);
        }
        // Re-read the game's settings soon, so what's shown matches what it saved.
        self.start_poll(ctx);
        if let Some(p) = &r.recording_path {
            let size = r.recording_size.unwrap_or(0);
            self.st.pending_recordings.retain(|x| &x.path != p);
            self.st.pending_recordings.push(crate::config::PendingRecording { path: p.clone(), size, created: crate::util::unix_now(), remind_after: 0 });
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
            skipped_pct: if r.output_total > 0 { Some(r.output_skipped as f32 * 100.0 / r.output_total as f32) } else { None },
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
                self.compare = Some(Compare { before_score, after_score, before, after: r.clone() });
                log::info!("Check my changes: Smoothness {before_score} -> {after_score}, Setup {}", self.analysis.setup_score);
            } else {
                log::info!("Check my changes: different game ({:?} -> {:?}), no before/after", g(&before), g(&r));
            }
        }
        self.quick_next = false;
        // Remember this game's results, so switching games later shows them.
        if let Some(g) = r.game_running.clone().or_else(|| r.game_guess.clone()) {
            let open: Vec<String> = self.analysis.issues.iter().filter(|i| !self.st.kept_issues.contains(&i.id)).map(|i| i.options.iter().find(|o| o.suggested).map(|o| o.name.clone()).unwrap_or_else(|| i.title.clone())).collect();
            self.st.game_memory.insert(g, crate::config::GameMemory { time: crate::util::unix_now(), smoothness: self.analysis.smoothness.unwrap_or(0), setup: self.analysis.setup_score, verdict: self.analysis.verdict.clone(), open });
            self.st.save();
        }
        if self.st.notify_test_done {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
            self.notify_away(ctx, "Test finished", &format!("{} Open Preflight to see what to fix.", self.analysis.verdict));
        }
        self.toast(ctx, format!("Test finished · {}", self.analysis.verdict));
    }

    fn start_test(&mut self, ctx: &egui::Context, system_only: bool) {
        self.test_counter += 1;
        self.side_test = None;
        let profiles = crate::games::profiles();
        let mut game_processes: Vec<(String, Vec<String>)> = profiles.iter().filter(|p| p.id != "minecraft").map(|p| (p.name.clone(), p.processes.clone())).collect();
        // Games without a profile that Preflight has already found. Minecraft
        // runs as Java, so it only counts when it's actually running.
        game_processes.extend(self.games.iter().filter(|g| g.auto.is_some() && (!g.id.starts_with("minecraft:") || g.running)).map(|g| (g.name.clone(), g.processes.clone())));
        let audio_roles = self.obs.as_ref().map(|o| o.audio.iter().map(|a| (a.name.clone(), a.role.clone())).collect()).unwrap_or_default();
        let target_secs = if self.quick_next { 60.0 } else { livetest::TARGET_SECONDS };
        let need_stream = self.show_bw_help && !system_only;
        let cfg = livetest::TestConfig { host: self.st.obs_host.clone(), port: self.st.obs_port, password: self.pw(), system_only, game_processes, audio_roles, target_secs, need_stream };
        log::info!("Watching for a recording (system only: {system_only})");
        self.fail_banner = None;
        self.test_handle = Some(livetest::start(cfg, ctx.clone()));
    }

    fn stop_watching_if_idle(&mut self) {
        if let Some(h) = &self.test_handle {
            if matches!(h.phase(), Phase::WaitingForRecord | Phase::Connecting) {
                h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    fn connect_obs(&mut self, ctx: &egui::Context) {
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
    fn open_counts(&self) -> (usize, usize) {
        let open: Vec<&Issue> = self.analysis.issues.iter().filter(|i| !self.st.kept_issues.contains(&i.id)).collect();
        let imp = open.iter().filter(|i| !analysis::is_optional(i)).count();
        (imp, open.len() - imp)
    }

    /// The fix being shown: always the latest version of that issue (a new
    /// style or a rescan changes its suggestion), found by its id.
    fn current_issue(&self) -> Option<&Issue> {
        if self.returning {
            return None;
        }
        let q = self.fix_queue.get(self.fix_idx)?;
        self.analysis.issues.iter().find(|i| i.id == q.id).or(Some(q))
    }

    /// Did the user's change make the current issue go away?
    /// The value a game shows now for a setting (by its menu name).
    fn game_value(g: &GameResult, setting: &str) -> Option<String> {
        let s = setting.to_lowercase();
        if s == "fps limit" {
            if g.findings.iter().any(|f| f.effect == "uncapped_fps") {
                return Some("Off".into());
            }
            return g.fps_cap.map(|c| c.to_string());
        }
        g.tunables.iter().find(|t| t.setting.to_lowercase() == s).map(|t| t.now.clone())
            .or_else(|| g.preset.as_ref().filter(|p| p.setting.to_lowercase() == s).map(|p| p.value.clone()))
            .or_else(|| g.current.iter().find(|c| c.setting.to_lowercase() == s).map(|c| c.value.clone()))
            .or_else(|| g.findings.iter().find(|f| f.label.to_lowercase().starts_with(&s)).map(|f| crate::games::pretty_value(&f.value)))
    }

    /// Did the game save the change the user said they made?
    fn check_confirm(&mut self, ctx: &egui::Context) {
        let Some(cw) = self.confirm.clone() else { return };
        let now_t = ctx.input(|i| i.time);
        let saved = self.game_file_saved(&cw.game).unwrap_or(0);
        if saved <= cw.since {
            // Not saved yet. After 90 seconds, say so (and keep watching).
            if now_t - cw.started > 90.0 && cw.result.is_none() {
                let msg = format!("{} hasn't saved its settings since you clicked. Most games save when you press Apply or leave the settings menu.", cw.game);
                self.confirm.as_mut().unwrap().result = Some((false, msg));
            }
            if now_t - cw.started > 600.0 {
                self.confirm = None;
            }
            return;
        }
        let Some(g) = self.games.iter().find(|g| g.name == cw.game) else { return };
        let same = |a: &str, b: &str| {
            let (x, y) = (crate::autoread::norm(a), crate::autoread::norm(b));
            x == y || (!x.is_empty() && y.contains(&x)) || (!y.is_empty() && x.contains(&y))
        };
        let mut done: Vec<String> = Vec::new();
        let mut stuck: Vec<String> = Vec::new();
        let mut moved: Vec<String> = Vec::new();
        for c in &cw.changes {
            let Some(v) = Self::game_value(g, &c.setting) else { continue };
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
            (false, format!("{} saved its settings at {when}, but {}. Did you press Apply in the game?", cw.game, stuck.join(", ")))
        } else if done.is_empty() && moved.is_empty() {
            (true, format!("{} saved its settings at {when}.", cw.game))
        } else {
            let mut parts = done;
            parts.extend(moved);
            (true, format!("Preflight sees it: {}. Saved by {} at {when}.", parts.join(", "), cw.game))
        };
        log::info!("Change check: {}", result.1);
        if result.0 {
            self.toast(ctx, result.1.clone());
        }
        self.confirm = Some(ConfirmWatch { since: saved, result: Some(result), ..cw });
    }

    fn check_fix_detected(&mut self) {
        if self.step != 3 || self.detected.is_some() {
            return;
        }
        let Some(cur) = self.current_issue().cloned() else { return };
        // Measured problems (from the test) can't be re-measured without a new
        // test, so for those a change to the relevant setting counts.
        // Any OBS encoder or video setting counts: preset, multipass, tuning,
        // rate control, keyframes, filter... not just encoder and bitrate.
        let obs_diff: Vec<String> = match (self.baseline_obs.as_ref().filter(|o| o.connected), self.obs.as_ref().filter(|o| o.connected)) {
            (Some(b), Some(n)) => b.enc_settings().changes_to(&n.enc_settings()),
            _ => Vec::new(),
        };
        let obs_changed = !obs_diff.is_empty();
        let game_changed = !self.baseline_findings.is_empty() && self.heavy_findings() != self.baseline_findings;
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
                let gone: Vec<String> = self.baseline_findings.difference(&now).map(|s| s.split('=').next().unwrap_or("").to_string()).collect();
                if gone.is_empty() { "your game settings changed.".to_string() } else { format!("you changed: {}.", gone.join(", ").to_lowercase()) }
            }
            "audio_balance" => {
                let d = match (self.baseline_mic_db, self.obs.as_ref().and_then(|o| o.mic_volume_db())) {
                    (Some(b), Some(n)) => n - b,
                    _ => 0.0,
                };
                if d > 0.5 { format!("your mic went up {d:.0} dB in OBS.") } else { "your mic and game levels are balanced now.".into() }
            }
            "encoder" => format!("OBS is now using {}.", self.obs.as_ref().map(|o| o.encoder_kind.clone()).unwrap_or_default()),
            "encoder_lag" | "cpu_high" if obs_changed => format!("you changed {} in OBS.", obs_diff.join(", ")),
            "cpu_high" | "heat" => "your game settings changed.".into(),
            "recording_mismatch" => "your recording now uses the stream's settings.".into(),
            "rate_control" => format!("rate control is now {}.", self.obs.as_ref().map(|o| o.rate_control.to_uppercase()).unwrap_or_default()),
            "keyframes" => format!("keyframes are now every {} seconds.", self.obs.as_ref().and_then(|o| o.keyint_sec).unwrap_or(0)),
            "preset" => format!("the encoder preset is now {}.", self.obs.as_ref().map(|o| o.preset.clone()).unwrap_or_default()),
            "downscale" => format!("the downscale filter is now {}.", self.obs.as_ref().map(|o| o.scale_type.clone()).unwrap_or_default()),
            "display_capture" => "your scene no longer uses full-screen capture.".into(),
            "bitrate_network" | "bitrate_platform" => format!("your bitrate is now {} kbps.", self.obs.as_ref().and_then(|o| o.bitrate_kbps).unwrap_or(0)),
            _ => "this looks fixed now.".into(),
        };
        log::info!("Fix detected for {}: {text}", cur.id);
        self.note_change(format!("{}: {}", analysis::impact_of(&cur.id), text.trim_end_matches('.')));
        self.fixed.push(cur.id.clone());
        self.detected = Some(text);
    }

    fn next_line(&self) -> String {
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
                        "You're live. Preflight watches quietly from the tray and writes your post-flight summary.".into()
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
            Screen::PostFlight => {
                if self.live.is_some() {
                    "You're live. Preflight watches quietly from the tray.".into()
                } else if self.streams.is_empty() {
                    "Leave Preflight open while you stream. You'll get a summary here when you end it.".into()
                } else {
                    "Here's how your stream went, and what to change next time.".into()
                }
            }
            Screen::History => "What you changed and what it did. It all stays on this PC.".into(),
            Screen::Settings => "Change anything here at any time.".into(),
            Screen::Help => "Something not working? Click Report a problem.".into(),
        }
    }
}

/// Reads OBS through the shared connection, or directly if it isn't running.
fn obs_now(hub: Option<&std::sync::mpsc::Sender<crate::streamwatch::Cmd>>, host: &str, port: u16, pw: Option<&str>) -> (Option<ObsSnapshot>, Option<String>) {
    match hub {
        Some(h) => crate::streamwatch::request_snapshot(h),
        None => read_obs(host, port, pw),
    }
}

fn read_obs(host: &str, port: u16, pw: Option<&str>) -> (Option<ObsSnapshot>, Option<String>) {
    match crate::obs::Client::connect(host, port, pw) {
        Ok(mut c) => (Some(crate::obs::snapshot_live(&mut c)), None),
        Err(e) => {
            log::info!("OBS not connected: {e}");
            (crate::obs::snapshot_saved(), Some(e.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        self.tray_logic(ctx);
        let t_now = ctx.input(|i| i.time);
        self.track_readings(t_now);
        self.pair_logic(ctx, t_now);
        let now = ctx.input(|i| i.time);
        // Watch for a recording automatically while on step 2.
        if self.screen == Screen::Preflight && self.step == 2 {
            let need = self.test_handle.is_none() && (self.test.is_none() || self.want_new_test) && !self.watch_paused && now >= self.watch_retry_at;
            let connected = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
            if need && connected && self.scan.is_none() {
                self.start_test(ctx, false);
            }
        }
        // Light refresh on steps 2–4 so changes are noticed (not while minimized).
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        // Encoder panel: refresh every 5 seconds on Your setup and during a test.
        let measuring_now = self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false);
        if let Some(rx) = &self.enc_rx {
            if let Ok(list) = rx.try_recv() {
                if measuring_now {
                    for e in &list {
                        for s in &e.sessions {
                            if !self.encodes_seen.contains(s) {
                                self.encodes_seen.push(s.clone());
                            }
                        }
                    }
                }
                self.encoders = list;
                self.enc_rx = None;
            }
        }
        if !minimized && self.enc_rx.is_none() && now >= self.enc_next && (self.screen == Screen::Results || measuring_now) {
            if let Some(hw) = self.hw.clone() {
                let (tx, rx) = std::sync::mpsc::channel();
                self.enc_rx = Some(rx);
                self.enc_next = now + 5.0;
                let c = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::encoders::snapshot(&hw));
                    c.request_repaint();
                });
            }
        }
        let watching_change = self.confirm.as_ref().map(|c| c.result.as_ref().map(|r| !r.0).unwrap_or(true)).unwrap_or(false);
        if !minimized && self.live.is_none() && ((self.screen == Screen::Preflight && self.step >= 2) || watching_change) && self.test_handle.as_ref().map(|h| h.phase() != Phase::Measuring).unwrap_or(true) && now >= self.poll_next {
            self.poll_next = now + 4.0;
            self.start_poll(ctx);
        }
        if self.scan.is_some() || self.test_handle.is_some() || self.poll.is_some() || (self.screen == Screen::Preflight && self.step >= 2) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.window_known {
            self.window_known = true;
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                match h.as_raw() {
                    #[allow(unused_variables)]
                    RawWindowHandle::Win32(w) => {
                        if let Some(t) = &mut self.tray {
                            t.set_window(w.hwnd.get());
                        }
                        self.win_hwnd = w.hwnd.get();
                    }
                    RawWindowHandle::Wayland(_) => self.can_hide = false,
                    _ => {}
                }
            }
            log::info!("Window: {}", if self.can_hide { "can hide to tray" } else { "Wayland: minimizes instead of hiding" });
        }
        theme::paint_backdrop(&ctx);
        if self.screen == Screen::Setup {
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(32, 24))).show(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let w = ui.available_width().min(860.0);
                    ui.set_max_width(w);
                    self.ui_setup(ui, &ctx);
                });
            });
            self.ui_toast(&ctx);
            return;
        }
        egui::Panel::left("menu")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(PANEL).inner_margin(egui::Margin::same(16)).stroke(egui::Stroke::new(1.0, BORDER)))
            .show(ui, |ui| self.ui_sidebar(ui));
        let strip = self.readings();
        egui::Panel::bottom("next-bar")
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(CARD)
                    .inner_margin(egui::Margin { left: 24, right: 14, top: 10, bottom: 10 })
                    .outer_margin(egui::Margin { left: 24, right: 24, top: 6, bottom: 16 })
                    .corner_radius(egui::CornerRadius::same(16))
                    .stroke(egui::Stroke::new(1.0, BORDER)),
            )
            .show(ui, |ui| self.ui_next_bar(ui));
        if let Some(tiles) = strip.filter(|_| matches!(self.screen, Screen::Preflight | Screen::Results | Screen::PostFlight)) {
            egui::Panel::top("readings").resizable(false).frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 24, top: 14, bottom: 0 })).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let gap = ui.spacing().item_spacing.x;
                    let w = ((ui.available_width() - gap * 3.0) / 4.0).max(120.0);
                    for (label, value, series, col) in &tiles {
                        theme::reading_tile(ui, label, value, series, *col, w);
                    }
                });
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(32, 22))).show(ui, |ui| {
            ScrollArea::vertical().id_salt(format!("main-{:?}", self.screen)).auto_shrink([false, false]).show(ui, |ui| {
                let w = ui.available_width().min(1040.0);
                ui.set_max_width(w);
                self.ui_banners(ui);
                self.ui_live_banners(ui);
                match self.screen {
                    Screen::Preflight => self.ui_preflight(ui, &ctx),
                    Screen::Results => self.ui_results(ui),
                    Screen::PostFlight => self.ui_postflight(ui),
                    Screen::History => self.ui_history(ui),
                    Screen::Settings => self.ui_settings(ui, &ctx),
                    Screen::Help => self.ui_help(ui),
                    Screen::Setup => {}
                }
                ui.add_space(24.0);
            });
        });
        if let Some(a) = self.pending_act.take() {
            self.do_act(&ctx, a);
        }
        self.ui_report_box(&ctx);
        self.ui_uninstall(&ctx);
        self.ui_recording_popup(&ctx);
        self.ui_toast(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.st.save();
        log::info!("Closing");
    }
}

// ---------------------------------------------------------------------------
// Shared pieces

/// Share of a test's GPU samples at 95% or more, in percent.
fn maxed_pct(t: &TestResult) -> f32 {
    let v: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
    if v.is_empty() { 0.0 } else { v.iter().filter(|g| **g >= 95.0).count() as f32 * 100.0 / v.len() as f32 }
}

/// Important issues first, grouped by where you fix them (OBS, game, PC),
/// then the optional ones in the same grouping.
fn fix_order(issues: &[Issue], kept: &[String]) -> Vec<Issue> {
    let mut v: Vec<Issue> = issues.iter().filter(|i| !kept.contains(&i.id)).cloned().collect();
    v.sort_by_key(|i| (analysis::is_optional(i), analysis::place_of(&i.id), i.priority));
    v
}

/// Plain-English meanings for terms that appear in fixes.
const GLOSSARY: &[(&str, &str)] = &[
    ("bitrate", "How much data your stream sends each second, in kbps. Higher looks better but needs more upload speed."),
    ("kbps", "Kilobits per second: how bitrate is measured. 6000 kbps is 6 Mbps."),
    ("encoder", "The part that compresses your video so it can be streamed. It can run on your graphics card (fast) or your CPU (x264)."),
    ("nvenc", "NVIDIA's video encoder, a separate chip on GeForce cards. It encodes your stream without slowing the game much."),
    ("x264", "A software encoder that runs on your CPU. Good quality, but it competes with your game for the CPU."),
    ("cbr", "Constant bitrate: the stream sends the same amount of data every second. What Twitch, YouTube and Kick expect."),
    ("vbr", "Variable bitrate: the bitrate rises in busy scenes. Good for recordings, risky for live streams."),
    ("keyframe", "A full picture in the video. Viewers can only start watching at a keyframe. Every 2 seconds is standard."),
    ("preset", "How hard the encoder works on each frame. Slower presets (like P5) look better but take more work."),
    ("db", "Decibels: how loudness is measured. 6 dB quieter sounds about half as loud."),
    ("fps", "Frames per second: how many pictures per second the game or stream shows."),
    ("downscale", "Shrinking the picture from your screen size to your stream size."),
    ("enhanced broadcasting", "A Twitch/YouTube feature where your PC encodes several quality versions of the stream at once."),
    ("replay buffer", "OBS keeps the last few minutes in memory so you can save a clip after something happens."),
    ("rate control", "How the encoder decides how much data to use. CBR is the right choice for streaming."),
    ("dlss", "NVIDIA's upscaling: the game renders smaller and sharpens it, which frees up your GPU."),
    ("fsr", "AMD's upscaling (works on most cards): the game renders smaller and sharpens it, which frees up your GPU."),
];

/// Small chips under a fix for terms it uses; hover to see what they mean.
fn glossary_chips(ui: &mut Ui, text: &str) {
    let l = text.to_lowercase();
    let found: Vec<&(&str, &str)> = GLOSSARY.iter().filter(|(k, _)| {
        let k = *k;
        l.match_indices(k).any(|(i, _)| {
            let before = l[..i].chars().last().map(|c| !c.is_alphanumeric()).unwrap_or(true);
            let after = l[i + k.len()..].chars().next().map(|c| !c.is_alphanumeric() || k == "keyframe").unwrap_or(true);
            before && after
        })
    }).collect();
    if found.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("What do these mean?").size(12.5).color(MUTED));
        for (k, d) in found {
            let label = match *k {
                "db" => "dB".to_string(),
                "kbps" => "kbps".to_string(),
                "nvenc" | "cbr" | "vbr" | "fps" | "dlss" | "fsr" => k.to_uppercase(),
                other => other.to_string(),
            };
            ui.add(egui::Label::new(RichText::new(format!("{label} ?")).size(12.5).color(INFO).underline()).sense(egui::Sense::hover())).on_hover_text(*d);
        }
    });
}

fn cur_measured(id: &str) -> bool {
    matches!(id, "gpu_maxed" | "gpu_maybe" | "encoder_lag" | "cpu_high" | "heat" | "audio_balance")
}

/// One "Your setup" table: Setting · Now · Suggested · Why.
/// A setup section: a header you can fold, then one plain row per setting:
/// name, what it is now, a status, and "Fix this" where something should change.
fn setup_table(ui: &mut Ui, title: &str, rows: &[analysis::SetupRow], note: Option<&str>, fixed: &[String], act: &mut Option<String>) {
    use analysis::Tag;
    let changes = rows.iter().filter(|r| r.tag == Tag::Change).count();
    let optional = rows.iter().filter(|r| r.tag == Tag::Optional).count();
    let sub = if changes > 0 { format!("{changes} to change") } else if optional > 0 { format!("All good · {optional} optional") } else if rows.is_empty() { String::new() } else { "All good".into() };
    let open_default = changes > 0 || optional > 0 || title.starts_with("In OBS");
    setup_section(ui, title, &sub, if changes > 0 { WARN } else { GOOD }, open_default, |ui| {
        if let Some(n) = note {
            theme::small(ui, n);
            ui.add_space(4.0);
        }
        let mut sorted: Vec<&analysis::SetupRow> = rows.iter().collect();
        sorted.sort_by_key(|r| match r.tag {
            Tag::Change => 0,
            Tag::Optional => 1,
            Tag::Kept => 2,
            Tag::Keep => 3,
        });
        for r in sorted {
            ui.add(egui::Separator::default().spacing(10.0));
            let total = ui.available_width();
            let name_w = 170.0_f32.min(total * 0.3);
            let right_w = (total * 0.4).clamp(200.0, 420.0);
            let now_w = (total - name_w - right_w - ui.spacing().item_spacing.x * 2.0 - 6.0).max(80.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(name_w, 22.0), egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
                    ui.set_width(name_w);
                    ui.label(RichText::new(&r.setting).color(MUTED)).on_hover_text(&r.why);
                });
                ui.allocate_ui_with_layout(Vec2::new(now_w, 22.0), egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
                    ui.set_width(now_w);
                    ui.add(egui::Label::new(RichText::new(&r.now)).wrap());
                });
                ui.allocate_ui_with_layout(Vec2::new(right_w, 22.0), egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.set_width(right_w);
                    if let Some(id) = &r.issue_id {
                        match r.tag {
                            Tag::Change | Tag::Optional if fixed.contains(id) => {
                                ui.label(RichText::new("Changed").size(13.0).color(GOOD)).on_hover_text("Check my changes confirms it.");
                            }
                            Tag::Change | Tag::Optional => {
                                if outlined(ui, "Fix this").clicked() {
                                    *act = Some(format!("fix:{id}"));
                                }
                            }
                            Tag::Kept => {
                                if outlined(ui, "Fix it after all").clicked() {
                                    *act = Some(format!("unkeep:{id}"));
                                }
                            }
                            Tag::Keep => {}
                        }
                    }
                    let (txt, col) = match r.tag {
                        Tag::Keep => ("Good".to_string(), GOOD),
                        Tag::Change => (r.suggested.clone(), WARN),
                        Tag::Optional => (format!("Optional: {}", r.suggested), INFO),
                        Tag::Kept => ("Kept as is".to_string(), MUTED),
                    };
                    ui.add(egui::Label::new(RichText::new(txt).color(col)).wrap_mode(egui::TextWrapMode::Wrap)).on_hover_text(&r.why);
                });
            });
            if matches!(r.tag, Tag::Change | Tag::Optional) {
                ui.label(RichText::new(if r.menu.is_empty() { r.why.clone() } else { format!("{} · {}", r.why, r.menu) }).size(12.5).color(MUTED));
            }
        }
    });
}

/// A card with a header that folds: chevron, title, and a short note on the right.
fn setup_section(ui: &mut Ui, title: &str, sub: &str, sub_col: Color32, open_default: bool, body: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(20, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.make_persistent_id(("setup-sec", title));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open_default);
            let open = state.is_open();
            let head = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                let c = r.center();
                let pts = if open { vec![egui::pos2(c.x - 5.0, c.y - 2.5), egui::pos2(c.x, c.y + 2.5), egui::pos2(c.x + 5.0, c.y - 2.5)] } else { vec![egui::pos2(c.x - 2.5, c.y - 5.0), egui::pos2(c.x + 2.5, c.y), egui::pos2(c.x - 2.5, c.y + 5.0)] };
                ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(2.0, TEXT_SOFT)));
                ui.label(RichText::new(title).size(18.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(sub).color(sub_col));
                });
            });
            let r = head.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() {
                state.toggle(ui);
            }
            state.show_body_unindented(ui, |ui| {
                ui.add_space(4.0);
                body(ui);
            });
        });
    ui.add_space(10.0);
}

/// An outlined button: clickable, but not the main action (only the Next bar is lime).
fn outlined(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT).size(14.0))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x4A, 0x53, 0x61)))
            .corner_radius(egui::CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

/// The same Keep / Change / Optional / Kept label everywhere a fix appears.
fn tag_pill(ui: &mut Ui, t: analysis::Tag) {
    let (label, fg, bg) = match t {
        analysis::Tag::Keep => ("Keep", GOOD, GOOD_BG),
        analysis::Tag::Change => ("Change", WARN, Color32::from_rgb(0x3A, 0x2E, 0x10)),
        analysis::Tag::Optional => ("Optional", TEXT_SOFT, RAISED),
        analysis::Tag::Kept => ("Kept", MUTED, RAISED),
    };
    theme::pill(ui, fg, bg, label);
}

/// One clear action for a problem message, when there is one.
fn err_action(e: &str) -> Option<(&'static str, NextAct)> {
    let l = e.to_lowercase();
    if l.contains("password") || l.contains("rejected") || l.contains("authentication") {
        Some(("Enter the OBS password", NextAct::ObsSettings))
    } else if l.contains("couldn't reach obs") || l.contains("not running") || l.contains("websocket") || l.contains("open obs") || l.contains("connection refused") {
        Some(("Show me how", NextAct::ObsHelp))
    } else if l.contains("speed-test") || l.contains("upload test") {
        Some(("What to do", NextAct::Screen(Screen::Help)))
    } else {
        None
    }
}

/// A small score with its name, e.g. "Smoothness 92". Words carry the
/// meaning too, so colour is never the only signal.

/// "FPS limit  ·  Now 240  →  Change to 120", with where to find it.
fn change_row(ui: &mut Ui, c: &analysis::Change) {
    theme::inset(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&c.setting).strong());
            ui.label(RichText::new(&c.menu).size(12.5).color(MUTED));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::readout(ui, "Now", c.now.as_deref().unwrap_or("?"), false);
            let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 40.0), egui::Sense::hover());
            let y = r.max.y - 18.0;
            let p = ui.painter();
            let st = egui::Stroke::new(2.0, ACCENT);
            p.line_segment([egui::pos2(r.min.x + 6.0, y), egui::pos2(r.max.x - 6.0, y)], st);
            p.line_segment([egui::pos2(r.max.x - 13.0, y - 6.0), egui::pos2(r.max.x - 6.0, y)], st);
            p.line_segment([egui::pos2(r.max.x - 13.0, y + 6.0), egui::pos2(r.max.x - 6.0, y)], st);
            theme::readout(ui, "Change to", &c.to, true);
        });
    });
}

/// "2 to change and 6 optional", always the same wording.
fn count_words(imp: usize, opt: usize) -> String {
    match (imp, opt) {
        (0, 0) => "nothing to change".into(),
        (i, 0) => format!("{i} to change"),
        (0, o) => format!("{o} optional"),
        (i, o) => format!("{i} to change and {o} optional"),
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) if !s.starts_with("GPU") && !s.starts_with("OBS") && !s.starts_with("CPU") => format!("{}{}", f.to_lowercase(), c.as_str()),
        Some(_) => s.to_string(),
        None => String::new(),
    }
}

fn link_small(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).size(12.5).color(MUTED).underline()).frame(false)).clicked()
}

fn choice_tile(ui: &mut Ui, on: bool, title: &str, desc: &str) -> bool {
    egui::Frame::NONE
        .fill(if on { ACCENT_BG } else { CARD_DARK })
        .stroke(egui::Stroke::new(2.0, if on { ACCENT } else { BORDER }))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(RichText::new(title).strong());
                if !desc.is_empty() {
                    ui.label(RichText::new(desc).size(13.5).color(MUTED));
                }
            });
        })
        .response
        .interact(egui::Sense::click())
        .clicked()
}

/// One compact Go-live row: pill, title, detail, optional action button.
fn go_row(ui: &mut Ui, s: Status, label: &str, title: &str, detail: &str, button: Option<&str>, mut on_click: impl FnMut()) {
    egui::Frame::NONE.fill(CARD_DARK).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            theme::status_pill(ui, s, label);
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(detail).size(13.0).color(MUTED));
            if let Some(b) = button {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(b).clicked() {
                        on_click();
                    }
                });
            }
        });
    });
    ui.add_space(4.0);
}

/// A row the user ticks themselves. Returns true when clicked.
fn go_tick_row(ui: &mut Ui, done: bool, title: &str) -> bool {
    let r = egui::Frame::NONE
        .fill(if done { GOOD_BG } else { CARD_DARK })
        .stroke(egui::Stroke::new(1.0, if done { Color32::from_rgb(0x2F, 0x6B, 0x47) } else { BORDER_STRONG }))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if done {
                    theme::status_pill(ui, Status::Ok, "Done");
                } else {
                    theme::pill(ui, TEXT, RAISED, "Tick");
                }
                ui.label(RichText::new(title).strong().color(if done { TEXT_SOFT } else { TEXT }));
            });
        })
        .response
        .interact(egui::Sense::click());
    ui.add_space(4.0);
    r.clicked()
}

fn status_row(ui: &mut Ui, s: Status, label: &str, title: &str, detail: &str) {
    theme::inset(ui, |ui| {
        ui.horizontal(|ui| {
            theme::status_pill(ui, s, label);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).strong());
                if !detail.is_empty() {
                    theme::muted(ui, detail);
                }
            });
        });
    });
}

impl App {
    fn go(&mut self, s: Screen) {
        if self.screen == Screen::Preflight && s != Screen::Preflight {
            self.stop_watching_if_idle();
        }
        self.screen = s;
        crate::logger::activity(&format!("screen {s:?}"));
    }

    fn set_step(&mut self, n: u8) {
        if self.step == 2 && n != 2 {
            self.stop_watching_if_idle();
        }
        if n != 4 && self.returning {
            self.returning = false;
        }
        let not_started = self.fix_idx == 0 && self.fixed.is_empty() && self.skipped.is_empty();
        if n == 3 && (self.fix_queue.is_empty() || not_started) && !self.returning {
            self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
            self.fix_idx = 0;
            self.baseline_findings = self.heavy_findings();
            self.baseline_mic_db = self.obs.as_ref().and_then(|o| o.mic_volume_db());
            self.baseline_obs = self.obs.clone();
        }
        self.step = n;
    }

    fn tray_logic(&mut self, ctx: &egui::Context) {
        if !self.tray_tried {
            self.tray_tried = true;
            self.tray = crate::tray::Tray::start(ctx.clone());
            if let Some(t) = &mut self.tray {
                if self.win_hwnd != 0 {
                    t.set_window(self.win_hwnd);
                }
            }
        }
        // Commands from the tray menu.
        let cmds: Vec<crate::tray::TrayCmd> = self.tray.as_ref().map(|t| std::iter::from_fn(|| t.poll()).collect()).unwrap_or_default();
        for c in cmds {
            match c {
                crate::tray::TrayCmd::Open => self.show_from_tray(ctx),
                crate::tray::TrayCmd::Quit => {
                    self.quitting = true;
                    self.show_from_tray(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // Close button: quit, or hide to the tray if that's the setting.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.st.close_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
        // Minimize: to the tray, where the window can really hide.
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if minimized && !self.hidden && self.st.minimize_to_tray && self.tray.is_some() && self.can_hide {
            self.hide_to_tray(ctx);
        }
        if !minimized && self.hidden && !self.can_hide {
            // Wayland: the user brought the window back from the taskbar.
            self.hidden = false;
        }
        // What the icon shows.
        let measuring = self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false);
        let obs_on = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
        let (state, status) = if let Some(l) = &self.live {
            (crate::tray::TrayState::Live, format!("Live · {}", crate::streamwatch::fmt_t(l.elapsed)))
        } else if measuring {
            (crate::tray::TrayState::Testing, "Testing · keep playing".to_string())
        } else if self.test_handle.is_some() {
            (crate::tray::TrayState::Watching, "Waiting for OBS to start the test".to_string())
        } else if obs_on {
            (crate::tray::TrayState::Watching, "Watching OBS · ready".to_string())
        } else {
            (crate::tray::TrayState::Idle, "Idle · OBS not open".to_string())
        };
        let attention = self.flag_question || (self.st.bandwidth_test_pending && !measuring && self.live.is_none() && self.test_handle.is_none()) || self.fail_banner.is_some() || (self.hidden && self.stream_notice.is_some());
        let status = if self.flag_question { "Needs you: is this a real stream?".to_string() } else if attention && self.st.bandwidth_test_pending { format!("{status} · remove the test flag") } else { status };
        if let Some(t) = &mut self.tray {
            t.set(crate::tray::TrayView { state, attention, status });
        }
    }

    /// A desktop notification, only when the window isn't in front.
    fn notify_away(&self, ctx: &egui::Context, title: &str, body: &str) {
        if !self.st.tray_notifications {
            return;
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if self.hidden || !focused {
            if let Some(t) = &self.tray {
                t.notify(title, body);
            }
        }
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        let Some(t) = &self.tray else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            return;
        };
        if !t.show_window(false) {
            if self.can_hide {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        self.hidden = true;
        log::info!("Window sent to the tray");
        if !self.tray_hint_shown {
            self.tray_hint_shown = true;
            if self.st.tray_notifications {
                t.notify("Preflight is in the tray", "It keeps watching OBS. Click the icon to open it.");
            }
        }
    }

    fn show_from_tray(&mut self, ctx: &egui::Context) {
        let native = self.tray.as_ref().map(|t| t.show_window(true)).unwrap_or(false);
        if !native {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.hidden = false;
        ctx.request_repaint();
    }

    /// Extra bug-report sections: what the rest of the report can't see.
    /// Numbers, Preflight's own labels and setting ids only.
    fn report_extras(&self) -> Vec<(&'static str, &'static str, String)> {
        use std::fmt::Write;
        let mut v = Vec::new();
        // Post-flight
        let mut t = String::new();
        match &self.live {
            Some(l) => {
                let _ = writeln!(t, "Live now: {} · {:.2}% dropped · {}", crate::streamwatch::fmt_t(l.elapsed), l.dropped_pct, l.kbps.map(|k| format!("{k:.0} kbps")).unwrap_or_else(|| "kbps unknown".into()));
            }
            None => {
                let _ = writeln!(t, "Not live.");
            }
        }
        let _ = writeln!(t, "Real-stream question showing: {} · user said real: {}", self.flag_question, self.flag_real);
        if let Some(l) = self.streams.first() {
            let p = crate::streamwatch::summarize(l);
            let _ = writeln!(t, "\nLast stream: started {} · {} · {} (level {}) · labelled {}", history::fmt_time(l.started_at), crate::streamwatch::fmt_duration(p.duration), p.grade, p.grade_level, if l.bandwidth_test { "test stream" } else { "real stream" });
            let _ = writeln!(t, "Target {} kbps · avg {:.0} kbps · lowest 30 s {} · reconnects {}", l.target_kbps.map(|k| k.to_string()).unwrap_or_else(|| "?".into()), p.avg_kbps, p.low_kbps.map(|(k, at)| format!("{k:.0} kbps at {at:.0} s")).unwrap_or_else(|| "-".into()), p.reconnects);
            let _ = writeln!(t, "Frames lost to: network {:.3}% · encoder {:.3}% · rendering {:.3}%", p.net_frac * 100.0, p.enc_frac * 100.0, p.render_frac * 100.0);
            let _ = writeln!(t, "CPU {:.0}% · GPU {} · GPU temp max {} · encoder {} · Preflight's own CPU {} / {}", p.cpu_avg, p.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "-".into()), p.gpu_temp_max.map(|g| format!("{g:.0}°C")).unwrap_or_else(|| "-".into()), p.enc_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "-".into()), p.own_cpu_avg.map(|g| format!("{g:.1}%")).unwrap_or_else(|| "-".into()), p.own_mb_max.map(|g| format!("{g:.0} MB")).unwrap_or_else(|| "-".into()));
            let _ = writeln!(t, "Encoder: {} · samples: {}", l.encoder, l.samples.len());
            for o in &l.outputs {
                let (s0, t0) = o.1;
                let (s1, t1) = o.2;
                let _ = writeln!(t, "Output {}: skipped {} of {} frames during the stream", o.0, s1.saturating_sub(s0), t1.saturating_sub(t0));
            }
            for o in &p.outputs {
                let _ = writeln!(t, "  {o}");
            }
            for a in &p.advice {
                let _ = writeln!(t, "Advice shown: {}", a.title);
            }
        }
        if self.streams.len() > 1 {
            let _ = writeln!(t, "\nEarlier streams:");
            for l in self.streams.iter().skip(1).take(10) {
                let p = crate::streamwatch::summarize(l);
                let _ = writeln!(t, "- {} · {} · {}{}", history::fmt_time(l.started_at), crate::streamwatch::fmt_duration(p.duration), p.grade, if l.bandwidth_test { " · test" } else { "" });
            }
        }
        v.push(("postflight", "Post-flight (numbers only)", t));

        // Test history and what changed between tests
        let mut t = String::new();
        if let Some(c) = &self.compare {
            let _ = writeln!(t, "Last before/after: Smoothness {} → {}", c.before_score, c.after_score);
            if let (Some(a), Some(b)) = (&c.before.obs_settings, &c.after.obs_settings) {
                let ch = a.changes_to(b);
                let _ = writeln!(t, "Changed between the tests: {}", if ch.is_empty() { "nothing in OBS".into() } else { ch.join("; ") });
            }
        } else {
            let _ = writeln!(t, "No before/after comparison yet.");
        }
        if let Some((p, sc)) = &self.prev_test {
            let _ = writeln!(t, "Previous test: {} · {:.0} s · Smoothness {sc}", history::fmt_time(p.started_at), p.seconds);
        }
        for (g, m) in &self.st.game_memory {
            let _ = writeln!(t, "Saved results: {g} · {} · Smoothness {} · Setup {} · {} open", crate::util::fmt_local(m.time), m.smoothness, m.setup, m.open.len());
        }
        for c in self.st.change_log.iter().rev().take(15) {
            let _ = writeln!(t, "Change note {}: {}", history::fmt_time(c.time), c.text);
        }
        v.push(("tests", "Test history and changes", t));

        // One setup for all games
        let f = &self.fleet;
        let mut t = format!("Setup: {} · motion {} · busy {} · heaviest {} · warning {}\n", if f.setup.is_empty() { "-" } else { &f.setup }, f.motion, f.busy, f.heaviest.clone().unwrap_or_else(|| "-".into()), f.warning.clone().unwrap_or_else(|| "none".into()));
        for l in &f.tested {
            let _ = writeln!(t, "- {} · GPU {} · maxed {:.0}% · skipped {:.2}% · motion {} · {}", l.game, l.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "-".into()), l.maxed_pct, l.skipped_pct, l.motion, l.verdict);
        }
        if !f.untested.is_empty() {
            let _ = writeln!(t, "Not tested: {}", f.untested.join(", "));
        }
        v.push(("fleet", "One setup for all games", t));

        // What was on screen
        let mut t = String::new();
        let _ = writeln!(t, "Screen {:?} · step {} · banner {} · game shown {:?} · style {}", self.screen, self.step, self.banner_pick(), self.current_game(), style_label(self.st.style));
        let phase = self.test_handle.as_ref().map(|h| format!("{:?}", h.phase())).unwrap_or_else(|| "no test running".into());
        let (rec, stream, el) = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| (s.rec_active, s.stream_active, s.elapsed))).unwrap_or((false, false, 0.0));
        let _ = writeln!(t, "Test chosen: {} · phase {phase} · OBS recording {rec} · streaming {stream} · elapsed {el:.0} s · flag help open {}", self.st.last_test, self.show_bw_help);
        let _ = writeln!(t, "Checklist: test flag marked added {} · ready to go live {} · pre-flight done {}", self.st.bandwidth_test_pending, self.go_ready, self.preflight_done);
        let (left, total) = self.fix_progress();
        let _ = writeln!(t, "Fixes: {left} of {total} left · at {}", self.fix_idx);
        for (n, i) in self.fix_queue.iter().enumerate() {
            let st = if self.fixed.contains(&i.id) { "changed" } else if self.st.kept_issues.contains(&i.id) { "kept" } else if self.skipped.contains(&i.id) { "skipped" } else { "open" };
            let _ = writeln!(t, "  {}. {} · {st}{}", n + 1, i.id, if analysis::is_optional(i) { " · optional" } else { "" });
        }
        if let Some(c) = &self.confirm {
            let _ = writeln!(t, "Change-saved check: {} change(s) · {}", c.changes.len(), c.result.as_ref().map(|r| format!("{} · {}", if r.0 { "all saved" } else { "not all saved" }, r.1)).unwrap_or_else(|| "waiting".into()));
        }
        if let Some(p) = &self.preview {
            let aligned = p.pairs.iter().filter(|x| x.aligned).count();
            let _ = writeln!(t, "Viewer preview: {} moments · {aligned} lined up · simulated: {} · ffmpeg missing {} · error {}", p.pairs.len(), p.simulated.clone().unwrap_or_else(|| "no".into()), p.ffmpeg_missing, p.error.clone().unwrap_or_else(|| "none".into()));
            for x in &p.pairs {
                let _ = writeln!(t, "  {:.0} s · match {} · aligned {}", x.t_sec, x.match_db.map(|d| format!("{d:.1} dB")).unwrap_or_else(|| "-".into()), x.aligned);
            }
        }
        v.push(("ui", "What was on screen", t));

        // Tray and desktop
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let mut t = format!(
            "Desktop: {} {} · window can hide: {} · hidden now: {}\n",
            if desktop.is_empty() { std::env::consts::OS } else { &desktop },
            session,
            self.can_hide,
            self.hidden
        );
        match &self.tray {
            Some(tr) => {
                let tv = tr.last_view();
                let _ = writeln!(t, "Tray: {} · needs you {} · \"{}\"", tv.state.word(), tv.attention, tv.status);
            }
            None => {
                let _ = writeln!(t, "Tray: not available{}", if self.tray_tried { "" } else { " (not started yet)" });
            }
        }
        let notes: Vec<String> = crate::logger::recent(400).into_iter().filter(|l| l.contains("Notification:") || l.contains("Tray:")).rev().take(10).collect();
        for n in notes.iter().rev() {
            let _ = writeln!(t, "{n}");
        }
        v.push(("tray", "Tray and desktop session", t));
        // Two PCs (no addresses, names or keys)
        let mut t = format!("Role: {} · OBS on another computer: {} · paired: {} · link running: {}\n", self.st.pc_role, self.obs_remote(), self.st.paired, self.pairing_kind);
        if !self.pair_status.is_empty() {
            let _ = writeln!(t, "Pairing status: {}", self.pair_status);
        }
        if let Some(l) = self.peer_line() {
            let _ = writeln!(t, "{l}");
        }
        for c in self.obs.as_ref().map(|o| o.captures.clone()).unwrap_or_default() {
            let _ = writeln!(t, "Capture: {} · range {} · in scene {} · audio offset {:?} ms", c.describe(), if c.range.is_empty() { "default" } else { &c.range }, c.in_scene, c.audio_offset_ms);
        }
        if let Some(o) = &self.obs {
            let _ = writeln!(t, "OBS colour range {} · colour space {}", o.color_range, o.color_space);
        }
        if let Some(s) = &self.side_test {
            let _ = writeln!(t, "Gaming PC during the last test: {s:?}");
        }
        v.push(("twopc", "Two PCs", t));
        v
    }

    /// The live readings strip: during a test or a stream it follows what's
    /// happening; otherwise it shows the last test. None when there's nothing yet.
    fn readings(&self) -> Option<Vec<(&'static str, String, Vec<f32>, Color32)>> {
        let fmt = |v: Option<f32>, unit: &str| v.map(|x| format!("{x:.0}{unit}")).unwrap_or_else(|| "–".into());
        let measuring = self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false);
        if (measuring || self.live.is_some()) && !self.reading_hist.is_empty() {
            let h = &self.reading_hist;
            let col = |f: fn(&ReadingPoint) -> Option<f32>| h.iter().filter_map(f).collect::<Vec<f32>>();
            let last = h.back().cloned().unwrap_or_default();
            let gpu = col(|p| p.gpu);
            return Some(vec![
                ("GPU", fmt(last.gpu, "%"), gpu.clone(), if last.gpu.unwrap_or(0.0) >= 92.0 { theme::AMBER } else { ACCENT }),
                ("Encoder", fmt(last.enc, "%"), col(|p| p.enc), ACCENT),
                ("Stream", last.kbps.map(|k| format!("{k:.0} kbps")).unwrap_or_else(|| "not live".into()), col(|p| p.kbps), theme::TEAL),
                ("Skipped", last.skipped.map(|s| format!("{s:.2}%")).unwrap_or_else(|| "–".into()), col(|p| p.skipped), theme::TEAL),
            ]);
        }
        let t = self.test.as_ref()?;
        let gpu: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
        let enc: Vec<f32> = t.samples.iter().filter_map(|s| s.enc).collect();
        let skipped = if t.output_total > 0 { Some(t.output_skipped as f32 * 100.0 / t.output_total as f32) } else { None };
        let kbps = self.obs.as_ref().and_then(|o| o.bitrate_kbps);
        Some(vec![
            ("GPU · last test", fmt(t.gpu_avg, "%"), gpu, if t.gpu_avg.unwrap_or(0.0) >= 92.0 { theme::AMBER } else { ACCENT }),
            ("Encoder", fmt(t.enc_avg, "%"), enc, ACCENT),
            ("Stream", kbps.map(|k| format!("{k} kbps")).unwrap_or_else(|| "–".into()), Vec::new(), theme::TEAL),
            ("Skipped", skipped.map(|s| format!("{s:.2}%")).unwrap_or_else(|| "–".into()), Vec::new(), theme::TEAL),
        ])
    }

    /// Adds one point a second to the readings history while testing or live.
    fn track_readings(&mut self, now: f64) {
        if now - self.reading_at < 1.0 {
            return;
        }
        self.reading_at = now;
        let last = self.test_handle.as_ref().filter(|h| h.phase() == Phase::Measuring).and_then(|h| h.state.lock().ok().and_then(|s| s.last.clone()));
        if last.is_none() && self.live.is_none() {
            if !self.reading_hist.is_empty() && self.test_handle.is_none() {
                self.reading_hist.clear();
            }
            return;
        }
        let point = ReadingPoint {
            gpu: last.as_ref().and_then(|s| s.gpu).or_else(|| self.reading_hist.back().and_then(|p| p.gpu)),
            enc: last.as_ref().and_then(|s| s.enc).or_else(|| self.encoders.iter().filter_map(|e| e.busy).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b))))),
            kbps: self.live.as_ref().and_then(|l| l.kbps),
            skipped: self.live.as_ref().map(|l| l.dropped_pct),
        };
        self.reading_hist.push_back(point);
        while self.reading_hist.len() > 60 {
            self.reading_hist.pop_front();
        }
    }

    fn role(&self) -> crate::twopc::Role {
        crate::twopc::Role::from_str(&self.st.pc_role)
    }

    fn obs_remote(&self) -> bool {
        !crate::twopc::is_local_host(&self.st.obs_host)
    }

    /// Paired mode: start or stop the link, swap numbers, and measure along.
    fn pair_logic(&mut self, ctx: &egui::Context, now: f64) {
        use crate::twopc::Role;
        let role = self.role();
        let want = match role {
            Role::Streaming if self.st.paired || self.pair_offer => "host",
            Role::Gaming if self.st.paired || self.pair_try => "client",
            _ => "",
        };
        if want != self.pairing_kind {
            self.pairing = None;
            self.pairing_kind = "";
            if !want.is_empty() {
                let token = if self.st.paired { crate::config::load_pair_secret() } else { None };
                let started = if want == "host" { crate::pairing::start_host(token, ctx.clone()) } else { crate::pairing::start_client(self.st.obs_host.clone(), token, ctx.clone()) };
                match started {
                    Ok(p) => {
                        self.pairing = Some(p);
                        self.pairing_kind = want;
                    }
                    Err(e) => {
                        self.pair_status = e;
                        self.pair_offer = false;
                    }
                }
            }
        }
        let Some(p) = &self.pairing else { return };
        if now - self.side_at < 1.0 {
            return;
        }
        self.side_at = now;
        let measuring = self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false);
        let fixes = |place: &[analysis::Place]| -> Vec<String> {
            self.analysis.issues.iter().filter(|i| place.contains(&analysis::place_of(&i.id)) && !self.st.kept_issues.contains(&i.id)).map(|i| i.options.iter().find(|o| o.suggested).map(|o| o.name.clone()).unwrap_or_else(|| i.title.clone())).take(8).collect()
        };
        let mut new_token = None;
        let mut monitor_hz = None;
        let mut got_side_test = None;
        let mut side_sample = false;
        if let Ok(mut g) = p.state.lock() {
            if self.pairing_kind == "host" {
                if measuring && g.my_stream.test_id != self.test_counter {
                    // A new test started.
                }
                g.my_stream = crate::pairing::StreamSide {
                    obs_connected: self.obs.as_ref().map(|o| o.connected).unwrap_or(false),
                    live: self.live.is_some(),
                    kbps: self.live.as_ref().and_then(|l| l.kbps),
                    skipped_pct: self.live.as_ref().map(|l| l.dropped_pct),
                    encoder_busy: self.encoders.iter().filter_map(|e| e.busy).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b)))),
                    cpu: 0.0,
                    measuring,
                    test_id: self.test_counter,
                    fixes: fixes(&[analysis::Place::Obs]),
                };
                if let Some((gs, _)) = &g.peer_game {
                    monitor_hz = Some(gs.monitor_hz);
                    if let Some(t) = &gs.test {
                        if t.test_id == self.test_counter && self.side_test.as_ref().map(|s| s != t).unwrap_or(true) {
                            got_side_test = Some(t.clone());
                        }
                    }
                }
                if g.code.is_none() && self.pair_offer && g.new_token.is_none() && g.token.is_some() {
                    self.pair_offer = false;
                }
            } else {
                side_sample = true;
                let peer_measuring = g.peer_stream.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 10).map(|(s, _)| (s.measuring, s.test_id));
                match (peer_measuring, &mut self.side_measure) {
                    (Some((true, id)), None) => {
                        log::info!("Paired mode: the streaming PC started a test; measuring this PC too");
                        self.side_measure = Some(crate::pairing::SideMeasure { test_id: id, started: Some(std::time::Instant::now()), game: self.games.iter().find(|x| x.running).map(|x| x.name.clone()), ..Default::default() });
                    }
                    (Some((false, _)), Some(m)) | (None, Some(m)) => {
                        g.my_game.test = Some(m.summary());
                        self.side_measure = None;
                    }
                    _ => {}
                }
                g.my_game.game = self.games.iter().find(|x| x.running).map(|x| x.name.clone());
                g.my_game.monitor_hz = self.st.monitor_hz;
                g.my_game.fixes = fixes(&[analysis::Place::Game, analysis::Place::Pc]);
            }
            if let Some(t) = g.new_token.take() {
                new_token = Some(t);
            }
            if !g.status.is_empty() {
                self.pair_status = g.status.clone();
            }
        }
        // Gaming PC: a light reading of this PC every 2 s while paired.
        if side_sample && (now as u64) % 2 == 0 {
            let mon = self.side_monitor.get_or_insert_with(crate::hardware::Monitor::new);
            let s = mon.sample(0.0);
            if let Some(m) = &mut self.side_measure {
                if let Some(gp) = s.gpu {
                    m.gpu.push(gp);
                }
                m.cpu.push(s.cpu);
            }
            if let Some(p) = &self.pairing {
                if let Ok(mut g) = p.state.lock() {
                    g.my_game.gpu = s.gpu;
                    g.my_game.gpu_temp = s.gpu_temp;
                    g.my_game.cpu = s.cpu;
                }
            }
        }
        if let Some(t) = new_token {
            crate::config::save_pair_secret(&t);
            self.st.paired = true;
            self.st.paired_name = if self.pairing_kind == "host" { "Gaming PC".into() } else { "Streaming PC".into() };
            self.st.save();
            self.pair_offer = false;
            self.pair_try = false;
            self.pair_code_input.clear();
        }
        if let Some(hz) = monitor_hz {
            if hz != self.st.other_monitor_hz {
                self.st.other_monitor_hz = hz;
                self.recompute();
            }
        }
        if let Some(t) = got_side_test {
            log::info!("Paired mode: got the gaming PC's numbers for this test");
            self.side_test = Some(t);
        }
    }

    /// Streaming PC: show a new code. Gaming PC: send the typed code.
    fn pair_now(&mut self, ctx: &egui::Context) {
        match self.role() {
            crate::twopc::Role::Streaming => {
                self.pair_offer = true;
                self.pair_logic(ctx, f64::MAX);
                if let Some(p) = &self.pairing {
                    if let Ok(mut g) = p.state.lock() {
                        g.code = Some((crate::pairing::new_code(), std::time::Instant::now(), 5));
                        g.status = "Type this code in Preflight on your gaming PC".into();
                    }
                }
            }
            crate::twopc::Role::Gaming => {
                let code: String = self.pair_code_input.chars().filter(|c| c.is_ascii_digit()).collect();
                if code.len() != 6 {
                    self.pair_status = "The code has 6 digits.".into();
                    return;
                }
                self.pair_try = true;
                self.pair_logic(ctx, f64::MAX);
                if let Some(p) = &self.pairing {
                    if let Ok(mut g) = p.state.lock() {
                        g.pair_with = Some(code);
                        g.status = "Pairing…".into();
                    }
                }
            }
            crate::twopc::Role::One => {}
        }
    }

    /// The code on screen (streaming PC), if any.
    fn pair_code(&self) -> Option<String> {
        self.pairing.as_ref().and_then(|p| p.state.lock().ok().and_then(|g| g.code.as_ref().filter(|c| c.1.elapsed().as_secs() < 300).map(|c| c.0.clone())))
    }

    /// The other PC's latest numbers, if it was heard from in the last 15 seconds.
    fn peer_line(&self) -> Option<String> {
        let p = self.pairing.as_ref()?;
        let g = p.state.lock().ok()?;
        if let Some((gs, at)) = g.peer_game.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 15) {
            let _ = at;
            return Some(format!(
                "Gaming PC: {} · GPU {} · CPU {:.0}% · monitor {} Hz",
                gs.game.clone().unwrap_or_else(|| "no game running".into()),
                gs.gpu.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "–".into()),
                gs.cpu,
                gs.monitor_hz
            ));
        }
        if let Some((ss, _)) = g.peer_stream.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 15) {
            return Some(format!(
                "Streaming PC: OBS {} · {} · encoder {}",
                if ss.obs_connected { "connected" } else { "not open" },
                if ss.live { ss.kbps.map(|k| format!("live at {k:.0} kbps")).unwrap_or_else(|| "live".into()) } else { "not live".into() },
                ss.encoder_busy.map(|v| format!("{v:.0}% busy")).unwrap_or_else(|| "–".into())
            ));
        }
        None
    }

    /// The other PC's open fixes, labelled for the combined view.
    fn peer_fixes(&self) -> Vec<String> {
        let Some(p) = self.pairing.as_ref() else { return Vec::new() };
        let Ok(g) = p.state.lock() else { return Vec::new() };
        if let Some((gs, _)) = &g.peer_game {
            return gs.fixes.clone();
        }
        if let Some((ss, _)) = &g.peer_stream {
            return ss.fixes.clone();
        }
        Vec::new()
    }

    fn unpair(&mut self) {
        self.pairing = None;
        self.pairing_kind = "";
        self.pair_offer = false;
        self.st.paired = false;
        self.st.paired_name.clear();
        self.st.save();
        crate::config::forget_pair_secret();
        self.pair_status = "Unpaired".into();
        log::info!("Paired mode: unpaired");
    }

    /// Settings › Your PCs: one PC or two, and paired mode.
    fn ui_pcs_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        use crate::twopc::Role;
        let mut changed = false;
        let mut act: Option<&str> = None;
        theme::card(ui, |ui| {
            ui.label(RichText::new("Your PCs").size(16.0).strong());
            theme::small(ui, "Some streamers game on one PC and stream from another, with a capture card or NDI in between.");
            for r in [Role::One, Role::Gaming, Role::Streaming] {
                if ui.radio(self.role() == r, r.label()).clicked() && self.role() != r {
                    self.st.pc_role = r.as_str().into();
                    self.st.two_pc_asked = true;
                    changed = true;
                }
            }
            let role = self.role();
            if role == Role::Gaming && !self.obs_remote() {
                ui.label(RichText::new("Put your streaming PC's address under OBS address below, so Preflight can read OBS there.").color(WARN));
            }
            if role != Role::One {
                ui.add_space(6.0);
                ui.label(RichText::new("Paired mode").strong());
                theme::small(ui, "Run Preflight on both PCs and link them once with a code. They share game, GPU and OBS numbers over your home network only, never the internet. No names, keys or addresses are shared.");
                if self.st.paired {
                    let heard = self.peer_line();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("Paired with your {}", if role == Role::Streaming { "gaming PC" } else { "streaming PC" })).color(GOOD));
                        ui.label(RichText::new(if heard.is_some() { "· connected" } else { "· not heard from right now" }).color(MUTED));
                        if outlined(ui, "Unpair").clicked() {
                            act = Some("unpair");
                        }
                    });
                    if let Some(h) = heard {
                        theme::small(ui, h);
                    }
                } else if role == Role::Streaming {
                    match self.pair_code() {
                        Some(code) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{} {}", &code[..3], &code[3..])).monospace().size(28.0).color(ACCENT));
                                ui.label(RichText::new("Type this code in Preflight on your gaming PC (Settings › Your PCs). It works for 5 minutes.").color(TEXT_SOFT));
                            });
                        }
                        None => {
                            if outlined(ui, "Pair with my gaming PC").clicked() {
                                act = Some("pair");
                            }
                        }
                    }
                    theme::small(ui, "Your firewall may ask to let Preflight use the home network. Allow it on private networks.");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Code from your streaming PC");
                        ui.add(egui::TextEdit::singleline(&mut self.pair_code_input).desired_width(90.0).hint_text("123 456"));
                        if outlined(ui, "Pair").clicked() {
                            act = Some("pair");
                        }
                    });
                }
                if !self.pair_status.is_empty() && !self.st.paired {
                    theme::small(ui, &self.pair_status);
                }
            }
        });
        match act {
            Some("pair") => self.pair_now(ctx),
            Some("unpair") => self.unpair(),
            _ => {}
        }
        if changed {
            log::info!("PC role: {}", self.st.pc_role);
            if self.role() == crate::twopc::Role::One && self.st.paired {
                self.unpair();
            }
            self.st.save();
            self.recompute();
        }
    }

    /// Your setup: the two-PC picture (capture sources, the other PC, its fixes),
    /// or a one-time question when it looks like two PCs.
    fn ui_two_pc_section(&mut self, ui: &mut Ui) {
        use crate::twopc::Role;
        let role = self.role();
        let caps: Vec<crate::twopc::CaptureInfo> = self.obs.as_ref().map(|o| o.captures.clone()).unwrap_or_default();
        let game_cap = self.obs.as_ref().map(|o| o.uses_game_capture()).unwrap_or(false);
        if role == Role::One {
            if !self.st.two_pc_asked && crate::twopc::looks_two_pc(&caps, game_cap, self.obs_remote()) {
                let mut pick: Option<Role> = None;
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Do you stream from a second PC?").size(17.0).strong());
                    theme::muted(ui, if self.obs_remote() { "OBS is on another computer." } else { "OBS shows a capture card or network source, and no game capture." });
                    ui.horizontal_wrapped(|ui| {
                        if outlined(ui, "This is my streaming PC").clicked() {
                            pick = Some(Role::Streaming);
                        }
                        if outlined(ui, "This is my gaming PC").clicked() {
                            pick = Some(Role::Gaming);
                        }
                        if outlined(ui, "No, one PC").clicked() {
                            pick = Some(Role::One);
                        }
                    });
                });
                ui.add_space(10.0);
                if let Some(r) = pick {
                    self.st.pc_role = r.as_str().into();
                    self.st.two_pc_asked = true;
                    self.st.save();
                    self.recompute();
                }
            }
            return;
        }
        let peer = self.peer_line();
        let sub = if self.st.paired { if peer.is_some() { "Paired · connected" } else { "Paired · not heard from" } } else { "Not paired" };
        let peer_fixes = self.peer_fixes();
        let side = self.side_test.clone();
        let paired = self.st.paired;
        setup_section(ui, if role == Role::Gaming { "Two PCs · this is the gaming PC" } else { "Two PCs · this is the streaming PC" }, sub, if peer.is_some() { GOOD } else { MUTED }, true, |ui| {
            for c in caps.iter().filter(|c| c.in_scene) {
                ui.add(egui::Separator::default().spacing(10.0));
                ui.label(RichText::new(c.describe()).color(TEXT));
                if !c.range.is_empty() {
                    theme::small(ui, format!("Colour range: {}", c.range));
                }
            }
            if caps.iter().all(|c| !c.in_scene) {
                theme::small(ui, "No capture card or network source is visible in your current OBS scene.");
            }
            ui.add(egui::Separator::default().spacing(10.0));
            match &peer {
                Some(l) => {
                    ui.label(RichText::new(l).color(TEXT_SOFT));
                }
                None if paired => theme::small(ui, "The other PC isn't answering right now. Is Preflight open there?"),
                None => theme::small(ui, "Pair with Preflight on your other PC (Settings › Your PCs) to see both PCs here."),
            }
            if !peer_fixes.is_empty() {
                ui.label(RichText::new(format!("Open on your {}:", if role == Role::Gaming { "streaming PC" } else { "gaming PC" })).strong());
                for f in &peer_fixes {
                    theme::small(ui, format!("• {f}"));
                }
            }
            if let Some(t) = &side {
                ui.label(RichText::new(format!(
                    "Gaming PC during the last test{}: GPU {} on average, maxed out {:.0}% of the time, CPU {:.0}%.",
                    t.game.as_ref().map(|g| format!(" ({g})")).unwrap_or_default(),
                    t.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "–".into()),
                    t.gpu_maxed_pct,
                    t.cpu_avg
                )).color(TEXT_SOFT));
            }
        });
    }

    /// Fixes still open, and how many there were in this round.
    fn fix_progress(&self) -> (usize, usize) {
        let open = self.analysis.issues.iter().filter(|i| !self.st.kept_issues.contains(&i.id) && !self.fixed.contains(&i.id) && !self.skipped.contains(&i.id)).count();
        let total = self.fix_queue.len().max(open);
        (open, total)
    }

    /// Games to switch between: saved results, the one shown, and the one running.
    fn switch_options(&self) -> Vec<String> {
        let mut v = self.saved_games();
        for g in [self.current_game(), self.games.iter().find(|g| g.running).map(|g| g.name.clone())].into_iter().flatten() {
            if !v.contains(&g) {
                v.push(g);
            }
        }
        v
    }

    fn ui_sidebar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        draw_logo(ui, 36.0);
        ui.add_space(14.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        let mut go_to: Option<Screen> = None;
        let mut step_to: Option<u8> = None;

        // ---- Pre-flight branch
        let pf_here = matches!(self.screen, Screen::Preflight | Screen::Results);
        let r = tree_row(ui, TreeRow { text: "Pre-flight", branch: Some(self.tree_pf_open), strong: true, ..Default::default() });
        if r.clicked() {
            if !pf_here {
                self.tree_pf_open = true;
                go_to = Some(Screen::Preflight);
            } else {
                self.tree_pf_open = !self.tree_pf_open;
            }
        }
        if self.tree_pf_open {
            // Game row: which game this is about; click to switch.
            let game = self.current_game().or_else(|| self.analysis.setup.game_name.clone());
            let running = game.as_ref().map(|g| self.games.iter().any(|x| &x.name == g && x.running)).unwrap_or(false);
            let gtext = game.clone().unwrap_or_else(|| "No game yet".into());
            let gr = tree_row(ui, TreeRow { text: &gtext, indent: 14.0, dot: Some(if running { GOOD } else { Color32::from_rgb(0x5A, 0x65, 0x77) }), chevron: true, strong: true, ..Default::default() })
                .on_hover_text(if running { "Running now. Click to switch games." } else if game.is_some() { "Not running. Click to switch games." } else { "Start a game and Preflight spots it." });
            let options = self.switch_options();
            let mut pick_game: Option<String> = None;
            egui::Popup::menu(&gr).align(egui::RectAlign::RIGHT_START).width(260.0).show(|ui| {
                if options.is_empty() {
                    ui.label(RichText::new("No games yet. Start one and Preflight spots it.").color(MUTED));
                }
                for g in &options {
                    let is_run = self.games.iter().any(|x| &x.name == g && x.running);
                    let when = if is_run { " · running".to_string() } else { self.st.game_memory.get(g).map(|m| format!(" · tested {}", crate::util::fmt_local(m.time))).unwrap_or_default() };
                    if ui.selectable_label(Some(g) == game.as_ref(), format!("{g}{when}")).clicked() {
                        pick_game = Some(g.clone());
                    }
                }
            });
            if let Some(g) = pick_game {
                if Some(&g) != game.as_ref() {
                    let running_name = self.games.iter().find(|x| x.running).map(|x| x.name.clone());
                    self.show_game(&g);
                    self.switch_dismissed = running_name;
                }
            }
            // Style row, smaller, under the game.
            let sname = style_label(self.st.style);
            let stext = format!("{sname} style");
            let sr = tree_row(ui, TreeRow { text: &stext, indent: 40.0, chevron: true, small: true, ..Default::default() }).on_hover_text("How Preflight orders your fixes for this game");
            let mut pick_style: Option<Style> = None;
            egui::Popup::menu(&sr).align(egui::RectAlign::RIGHT_START).width(300.0).show(|ui| {
                for (st, what) in [
                    (Style::Competitive, "Highest frame rate first. Graphics fixes come first."),
                    (Style::Balanced, "Smooth game and a clean stream. Upscaling fixes first."),
                    (Style::Cinematic, "Best-looking game. A frame rate cap comes first."),
                ] {
                    let on = self.st.style == st;
                    let resp = ui.add(egui::RadioButton::new(on, RichText::new(style_label(st)).strong()));
                    ui.label(RichText::new(what).size(12.5).color(MUTED));
                    ui.add_space(4.0);
                    if resp.clicked() {
                        pick_style = Some(st);
                    }
                }
                ui.separator();
                ui.label(RichText::new("Re-sorts your fixes straight away. No new test needed.").size(12.5).color(MUTED));
            });
            if let Some(st) = pick_style {
                if st != self.st.style {
                    self.st.apply_style(st);
                    self.on_style_changed(&ctx);
                }
            }
            // Steps
            let on_pf = self.screen == Screen::Preflight;
            let (left, total) = self.fix_progress();
            let test_done = self.test.is_some() || self.returning;
            let fix_text = if left > 0 { format!("{left} of {total} left") } else if !test_done { "after the test".to_string() } else { String::new() };
            let rows: [(&str, bool, bool, String, u8); 3] = [
                ("Test", on_pf && self.step == 2, test_done, String::new(), 2),
                ("Fix", on_pf && self.step == 3, test_done && left == 0, fix_text, 3),
                ("Ready to go live", on_pf && self.step == 4, self.preflight_done || self.live.is_some(), String::new(), 4),
            ];
            for (i, (name, sel, done, note, n)) in rows.iter().enumerate() {
                if i == 2 {
                    let r = tree_row(ui, TreeRow { text: "Your setup", indent: 14.0, dash: true, selected: self.screen == Screen::Results, ..Default::default() });
                    if r.clicked() {
                        go_to = Some(Screen::Results);
                    }
                }
                let r = tree_row(ui, TreeRow { text: name, indent: 14.0, dash: true, selected: *sel, done: *done, note, ..Default::default() });
                if r.clicked() {
                    step_to = Some(*n);
                }
            }
        }

        // ---- Post-flight branch
        ui.add_space(6.0);
        let post_here = matches!(self.screen, Screen::PostFlight | Screen::History);
        let live_note = if self.live.is_some() { "live" } else { "" };
        let r = tree_row(ui, TreeRow { text: "Post-flight", branch: Some(self.tree_post_open), strong: true, note: live_note, ..Default::default() });
        if r.clicked() {
            if !post_here {
                self.tree_post_open = true;
                self.stream_sel = 0;
                go_to = Some(Screen::PostFlight);
            } else {
                self.tree_post_open = !self.tree_post_open;
            }
        }
        if self.tree_post_open {
            let last_note = if self.live.is_some() { "" } else { self.streams.first().map(|s| if s.bandwidth_test { "test" } else { "" }).unwrap_or("none yet") };
            let r = tree_row(ui, TreeRow { text: if self.live.is_some() { "Live now" } else { "Last stream" }, indent: 14.0, dash: true, selected: self.screen == Screen::PostFlight, note: last_note, ..Default::default() });
            if r.clicked() {
                self.stream_sel = 0;
                go_to = Some(Screen::PostFlight);
            }
            let r = tree_row(ui, TreeRow { text: "History", indent: 14.0, dash: true, selected: self.screen == Screen::History, ..Default::default() });
            if r.clicked() {
                go_to = Some(Screen::History);
            }
        }

        let update_line = matches!(&self.update_result, Some(Ok(u)) if u.newer);
        let rest = ui.available_height() - if update_line { 136.0 } else { 112.0 };
        if rest > 0.0 {
            ui.add_space(rest);
        }
        if tree_row(ui, TreeRow { text: "Settings", selected: self.screen == Screen::Settings, ..Default::default() }).clicked() {
            go_to = Some(Screen::Settings);
        }
        if tree_row(ui, TreeRow { text: "Help & report a problem", selected: self.screen == Screen::Help, ..Default::default() }).clicked() {
            go_to = Some(Screen::Help);
        }
        ui.add_space(6.0);
        let pw_needed = self.obs_error.as_deref().map(|e| e.contains("password")).unwrap_or(false);
        let (txt, col) = match &self.obs {
            Some(o) if o.connected => ("OBS connected", GOOD),
            _ if pw_needed => ("OBS needs its password", WARN),
            Some(_) => ("OBS closed", WARN),
            None => ("OBS not found", MUTED),
        };
        if let Some(Ok(u)) = &self.update_result {
            if u.newer && self.update_done.is_none() {
                let l = format!("v{} available", u.latest);
                if ui.add(egui::Button::new(RichText::new(l).size(12.0).color(ACCENT)).frame(false)).on_hover_text("See it in Settings").clicked() {
                    go_to = Some(Screen::Settings);
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(txt).size(12.0).color(col));
            ui.label(RichText::new(format!("· v{}", crate::util::VERSION)).size(12.0).color(MUTED));
        });

        // Keyboard: Alt+1..4 = Test, Fix, Your setup, Post-flight.
        let keys = ctx.input_mut(|i| {
            let mut k = None;
            for (n, key) in [(1u8, egui::Key::Num1), (2, egui::Key::Num2), (3, egui::Key::Num3), (4, egui::Key::Num4)] {
                if i.consume_key(egui::Modifiers::ALT, key) {
                    k = Some(n);
                }
            }
            k
        });
        match keys {
            Some(1) => step_to = Some(2),
            Some(2) => step_to = Some(3),
            Some(3) => go_to = Some(Screen::Results),
            Some(4) => {
                self.stream_sel = 0;
                go_to = Some(Screen::PostFlight)
            }
            _ => {}
        }
        if let Some(n) = step_to {
            self.tree_pf_open = true;
            if self.screen != Screen::Preflight {
                self.go(Screen::Preflight);
            }
            self.returning = false;
            self.set_step(n);
        } else if let Some(s) = go_to {
            self.go(s);
        }
    }

    /// The main next action for the current screen, if there's one button to press.
    fn next_button(&self) -> Option<(String, NextAct)> {
        match self.screen {
            Screen::Preflight => match self.step {
                2 => {
                    let measuring = self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false);
                    if self.test.is_some() && !self.want_new_test && !measuring {
                        let open: Vec<&Issue> = self.analysis.issues.iter().filter(|i| !self.st.kept_issues.contains(&i.id)).collect();
                        let important = open.iter().filter(|i| !analysis::is_optional(i)).count();
                        if open.is_empty() {
                            Some(("Ready to go live".into(), NextAct::Step(4)))
                        } else if important == 0 {
                            Some((format!("Next: {} optional improvement{}", open.len(), if open.len() == 1 { "" } else { "s" }), NextAct::Step(3)))
                        } else {
                            Some((format!("Next: {important} to change"), NextAct::Step(3)))
                        }
                    } else if !self.obs.as_ref().map(|o| o.connected).unwrap_or(false) {
                        Some(("How to connect OBS".into(), NextAct::ObsHelp))
                    } else if self.test_handle.is_none() {
                        Some(("Start the test".into(), NextAct::ArmTest))
                    } else {
                        None
                    }
                }
                3 => match (self.current_issue(), &self.detected) {
                    (Some(_), Some(_)) => Some(("Next fix".into(), NextAct::NextIssue)),
                    (Some(i), None) if analysis::is_optional(i) && !self.optional_ok => Some(("Ready to go live".into(), NextAct::FixAction("skip_optional"))),
                    (Some(_), None) => Some(("I changed it".into(), NextAct::FixAction("manual"))),
                    (None, _) if self.test.is_some() || self.returning => {
                        if self.fixed.iter().any(|id| cur_measured(id)) || !self.fixed.is_empty() {
                            Some(("Check my changes".into(), NextAct::CheckChanges))
                        } else {
                            Some(("Ready to go live".into(), NextAct::Step(4)))
                        }
                    }
                    (None, _) => Some(("Go to the test".into(), NextAct::Step(2))),
                },
                4 if self.go_ready || self.live.is_some() => Some(("Minimize to tray".into(), NextAct::ToTray)),
                _ => None,
            },
            Screen::Results => {
                if self.test.is_none() {
                    Some(("Run the test".into(), NextAct::Step(2)))
                } else if !self.fixed.is_empty() {
                    Some(("Check my changes".into(), NextAct::CheckChanges))
                } else if self.analysis.issues.iter().any(|i| !self.st.kept_issues.contains(&i.id)) {
                    Some(("Go to the next fix".into(), NextAct::Step(3)))
                } else {
                    Some(("Ready to go live".into(), NextAct::Step(4)))
                }
            }
            Screen::PostFlight if self.streams.is_empty() && self.live.is_none() => Some(("Start a pre-flight".into(), NextAct::Step(2))),
            Screen::History | Screen::Help | Screen::Settings => Some(("Back to pre-flight".into(), NextAct::Screen(Screen::Preflight))),
            _ => None,
        }
    }

    fn ui_next_bar(&mut self, ui: &mut Ui) {
        let line = self.next_line();
        let button = self.next_button();
        let mut clicked: Option<NextAct> = None;
        ui.horizontal(|ui| {
            ui.label(theme::caps("Next", 12.0, ACCENT));
            let room = ui.available_width() - if button.is_some() { 260.0 } else { 0.0 };
            ui.allocate_ui(Vec2::new(room.max(100.0), 40.0), |ui| {
                ui.label(RichText::new(line).color(TEXT));
            });
            if let Some((label, act)) = &button {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::glow_button(ui, label).on_hover_text("Ctrl+Enter").clicked() {
                        clicked = Some(*act);
                    }
                });
            }
        });
        // Keyboard: Ctrl+Enter does the Next step from anywhere.
        if clicked.is_none() && ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
            clicked = button.as_ref().map(|b| b.1);
        }
        if let Some(a) = clicked {
            let ctx = ui.ctx().clone();
            self.do_act(&ctx, a);
        }
    }

    fn do_act(&mut self, ctx: &egui::Context, a: NextAct) {
        match a {
            NextAct::Step(n) => {
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.returning = false;
                self.set_step(n);
            }
            NextAct::Screen(s) => self.go(s),
            NextAct::NextIssue => {
                self.detected = None;
                self.show_other_fixes = false;
                self.fix_idx += 1;
            }
            NextAct::CheckChanges => self.check_changes(ctx),
            NextAct::ObsSettings => self.go(Screen::Settings),
            NextAct::ObsHelp => {
                self.help_open = Some(0);
                self.go(Screen::Help);
            }
            NextAct::FixAction(a) => {
                self.queued_fix_action = Some(a.to_string());
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.step = 3;
            }
            NextAct::ToTray => self.hide_to_tray(ctx),
            NextAct::ArmTest => {
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.step = 2;
            }
        }
    }

    /// Starts a new test and compares it with the last one when it finishes.
    fn check_changes(&mut self, _ctx: &egui::Context) {
        if let Some(t) = self.test.clone() {
            self.prev_test = Some((t, self.analysis.smoothness.unwrap_or(self.analysis.score)));
        }
        self.compare = None;
        self.quick_next = true;
        self.show_bw_help = false;
        self.want_new_test = true;
        self.watch_retry_at = 0.0;
        self.returning = false;
        if self.screen != Screen::Preflight {
            self.go(Screen::Preflight);
        }
        self.set_step(2);
        log::info!("Check my changes: new test requested");
    }

    fn ui_banners(&mut self, ui: &mut Ui) {
        if let Some(job) = &self.scan {
            if !matches!(self.screen, Screen::Preflight | Screen::Results) {
                theme::inset(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&job.step);
                    });
                });
                ui.add_space(10.0);
            }
        }
        if self.banner_pick() != 3 {
            return;
        }
        if let Some(msg) = self.fail_banner.clone() {
            let mut report = false;
            let mut dismiss = false;
            egui::Frame::NONE
                .fill(BAD_BG)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x6B, 0x2B, 0x2B)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("Something went wrong. {msg}")).strong());
                        ui.label(RichText::new("The details are attached if you save a report.").color(TEXT_SOFT));
                    });
                    ui.horizontal(|ui| {
                        if let Some((l, a)) = err_action(&msg) {
                            if theme::secondary(ui, l).clicked() {
                                self.pending_act = Some(a);
                                dismiss = true;
                            }
                        }
                        if ui.add(egui::Button::new(RichText::new("Save report").color(BG).strong()).fill(BAD)).clicked() {
                            report = true;
                        }
                        if link_small(ui, "Dismiss") {
                            dismiss = true;
                        }
                    });
                });
            if report {
                self.open_report(msg.clone());
                self.fail_banner = None;
            }
            if dismiss {
                self.fail_banner = None;
            }
            ui.add_space(12.0);
        }
    }

    fn ui_toast(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some((msg, until)) = self.toast.clone() {
            if now > until {
                self.toast = None;
                return;
            }
            egui::Area::new(egui::Id::unique("toast-area")).anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0)).show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(RAISED)
                    .stroke(egui::Stroke::new(1.0, BORDER_STRONG))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).strong());
                    });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    // -----------------------------------------------------------------------
    // Report a problem (one box)

    fn ui_report_box(&mut self, ctx: &egui::Context) {
        let Some(mut rb) = self.report.take() else { return };
        let mut keep = true;
        let mut save = false;
        egui::Window::new(if rb.saved.is_some() { "Report saved" } else { "Report this problem" })
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 120.0))
            .default_width(480.0)
            .show(ctx, |ui| {
                if let Some((id, path)) = rb.saved.clone() {
                    ui.label(RichText::new(format!("Saved. Report ID {id}")).strong());
                    theme::small(ui, "A plain text file: drag it into the chat, or open it to read what's in it first.");
                    // The full path, selectable, so it can always be copied by hand.
                    let mut shown = path.display().to_string();
                    ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY).interactive(true));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if theme::secondary(ui, "Open report").clicked() {
                            report::open_file(&path);
                        }
                        if theme::secondary(ui, "Show in folder").clicked() {
                            report::show_file(&path);
                        }
                        if theme::secondary(ui, if rb.copied { "Copied" } else { "Copy file location" }).clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                            report::copy_fallback(&path.display().to_string());
                            rb.copied = true;
                        }
                        if theme::primary(ui, "  Done  ").clicked() {
                            keep = false;
                        }
                    });
                    return;
                }
                if !rb.context.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("About:").color(MUTED));
                        ui.label(&rb.context);
                    });
                }
                ui.label(RichText::new("Anything to add? (optional)").strong());
                ui.add(egui::TextEdit::multiline(&mut rb.note).desired_rows(3).desired_width(f32::INFINITY).hint_text("e.g. V-Sync is actually off in my game"));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Includes your PC specs, OBS settings and recent activity. Never passwords, stream keys or your name.").size(13.0).color(MUTED));
                    if link_small(ui, if rb.show_contents { "Hide details" } else { "What's included" }) {
                        rb.show_contents = !rb.show_contents;
                    }
                });
                if rb.show_contents {
                    theme::inset(ui, |ui| {
                        for s in [
                            "A summary: what this is about, your note, anything that failed",
                            "Your system: OS, CPU, graphics card, RAM, drivers",
                            "OBS: encoder and quality settings, bitrate, resolution, source types (no names, no stream key)",
                            "Games: which settings files were found, and their full contents (game settings and key bindings only)",
                            "Your last scan and test measurements",
                            "Recent app activity (user name removed from paths)",
                        ] {
                            theme::small(ui, format!("• {s}"));
                        }
                    });
                }
                if let Some(e) = &rb.error {
                    ui.label(RichText::new(format!("Couldn't save the report: {e}")).color(BAD));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary(ui, "  Save report  ").clicked() {
                        save = true;
                    }
                    if theme::secondary(ui, "Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if save {
            self.selftests = report::self_test(&report::SelfTestInput {
                hw: self.hw.as_ref(),
                games: &self.games,
                obs: self.obs.as_ref(),
                obs_error: self.obs_error.as_deref(),
                net: self.net.as_ref(),
            });
            let extra = self.report_extras();
            let sections = report::build_sections(&report::ReportInput {
                context: &rb.context,
                description: &rb.note,
                hw: self.hw.as_ref(),
                games: &self.games,
                obs: self.obs.as_ref(),
                obs_error: self.obs_error.as_deref(),
                net: self.net.as_ref(),
                test: self.test.as_ref(),
                analysis: &self.analysis,
                selftests: &self.selftests,
                settings: &self.st,
                encoders: &self.encoders,
                extra: &extra,
            });
            let id = crate::util::short_id();
            match report::save(&sections, &id) {
                Ok(p) => {
                    rb.saved = Some((id, p));
                    self.pending_crashes = crate::logger::pending_crashes().len();
                }
                Err(e) => rb.error = Some(e),
            }
        }
        if keep {
            self.report = Some(rb);
        }
    }

    /// "You're now playing X" banner and the "Showing: game" switcher.
    fn ui_game_switch(&mut self, ui: &mut Ui) {
        let measuring = self.test_handle.as_ref().map(|h| matches!(h.phase(), Phase::Measuring)).unwrap_or(false);
        let showing = self.test.as_ref().and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()));
        let running = self.games.iter().find(|g| g.running).map(|g| g.name.clone());
        let saved = self.saved_games();
        let mut act: Option<(u8, String)> = None;
        if let (Some(r), Some(sh)) = (&running, &showing) {
            if r != sh && !measuring && self.switch_dismissed.as_ref() != Some(r) {
                egui::Frame::NONE.fill(CARD).stroke(egui::Stroke::new(1.5, ACCENT)).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("You're now playing {r}. Your results are for {sh}.")).size(17.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        if theme::secondary(ui, &format!("Test {r}")).clicked() {
                            act = Some((1, r.clone()));
                        }
                        if saved.contains(r) && theme::secondary(ui, &format!("Show {r}'s last results")).clicked() {
                            act = Some((2, r.clone()));
                        }
                        if theme::secondary(ui, &format!("Keep showing {sh}")).clicked() {
                            act = Some((3, r.clone()));
                        }
                    });
                });
                ui.add_space(10.0);
            }
        }
        // Switching which game's results are shown is the game row in the sidebar.
        match act {
            Some((1, g)) => {
                // A fresh test for the new game: no before/after with the old one.
                self.show_game(&g);
                self.want_new_test = true;
                self.quick_next = false;
                self.watch_retry_at = 0.0;
                self.returning = false;
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.set_step(2);
            }
            Some((2, g)) => {
                self.show_game(&g);
                // Picked on purpose: don't nag about the game that's running.
                self.switch_dismissed = running.clone();
            }
            Some((3, g)) => self.switch_dismissed = Some(g),
            _ => {}
        }
    }

    fn ui_uninstall(&mut self, ctx: &egui::Context) {
        let Some((mut recs, mut program, mut error)) = self.uninstall.take() else { return };
        let mut keep = true;
        let mut go = false;
        egui::Window::new("Remove Preflight from this PC")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("This deletes:").strong());
                for (what, path) in crate::uninstall::own_folders() {
                    theme::inset(ui, |ui| {
                        ui.label(what);
                        theme::small(ui, crate::util::scrub(&path.to_string_lossy()));
                    });
                }
                let menu = crate::uninstall::menu_files();
                if !menu.is_empty() {
                    theme::inset(ui, |ui| {
                        ui.label("The app menu entry and its icon");
                        theme::small(ui, format!("{} files in {}", menu.len(), crate::util::scrub("~/.local/share/applications and ~/.local/share/icons")));
                    });
                }
                if let Some(exe) = crate::uninstall::program_file() {
                    ui.checkbox(&mut program, "Also delete the Preflight program file");
                    theme::small(ui, crate::util::scrub(&exe.to_string_lossy()));
                }
                if !recs.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Test recordings still saved (only the ticked ones are deleted):").strong());
                    for (path, size, del) in recs.iter_mut() {
                        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        ui.checkbox(del, format!("{name} · {}", crate::util::fmt_bytes(*size)));
                    }
                }
                ui.add_space(6.0);
                theme::small(ui, "OBS, your games, their settings and everything else stay exactly as they are.");
                if let Some(e) = &error {
                    ui.label(RichText::new(format!("Some files couldn't be removed: {e}")).color(BAD));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("  Remove and close  ").color(BG).strong()).fill(BAD)).clicked() {
                        go = true;
                    }
                    if theme::secondary(ui, "Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if go {
            let chosen: Vec<std::path::PathBuf> = recs.iter().filter(|r| r.2).map(|r| std::path::PathBuf::from(&r.0)).collect();
            match crate::uninstall::run(&chosen, program) {
                Ok(()) => std::process::exit(0),
                Err(e) => error = Some(e),
            }
        }
        if keep {
            self.uninstall = Some((recs, program, error));
        }
    }

    /// Deletes one of Preflight's own test recordings (only on the user's click
    /// or their automatic clean-up choice).
    fn delete_recording(&mut self, ctx: &egui::Context, path: &str) {
        if !self.st.pending_recordings.iter().any(|p| p.path == path) {
            return;
        }
        let r = std::fs::remove_file(path);
        self.st.pending_recordings.retain(|p| p.path != path);
        self.st.save();
        match r {
            Ok(_) => {
                log::info!("Test recording deleted");
                self.toast(ctx, "Test recording deleted");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => self.toast(ctx, format!("Couldn't delete it: {e}")),
        }
    }

    /// "mockrec.mkv · 845 MB · Desktop" with Delete, Show in folder and Keep.
    fn recording_row(&self, ui: &mut Ui, rec: &crate::config::PendingRecording, title: &str) -> Option<(String, String)> {
        let mut act = None;
        let p = std::path::Path::new(&rec.path);
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let folder = p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        theme::inset(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong());
                ui.label(RichText::new(format!("{name} · {} · {folder}", crate::util::fmt_bytes(rec.size))).color(TEXT_SOFT));
            });
            ui.horizontal(|ui| {
                if theme::secondary(ui, "Delete it").clicked() {
                    act = Some(("delete".to_string(), rec.path.clone()));
                }
                if theme::secondary(ui, "Show in folder").clicked() {
                    report::show_file(p);
                }
                if link_small(ui, "Keep it") {
                    act = Some(("keep".to_string(), rec.path.clone()));
                }
            });
        });
        act
    }

    fn do_recording_act(&mut self, ctx: &egui::Context, a: Option<(String, String)>) {
        match a {
            Some((k, p)) if k == "delete" => self.delete_recording(ctx, &p),
            Some((k, p)) if k == "keep" => {
                self.st.pending_recordings.retain(|x| x.path != p);
                self.st.save();
            }
            _ => {}
        }
    }

    /// The automatic clean-up, when the user turned it on: once the preview is made.
    fn auto_clean_recording(&mut self, ctx: &egui::Context) {
        if !self.st.auto_delete_recordings {
            return;
        }
        if let Some(p) = self.test.as_ref().and_then(|t| t.recording_path.clone()) {
            self.delete_recording(ctx, &p);
        }
    }

    fn ui_recording_popup(&mut self, ctx: &egui::Context) {
        if !self.show_recording_popup || self.report.is_some() {
            return;
        }
        let now = crate::util::unix_now();
        let Some(rec) = self.st.pending_recordings.iter().find(|p| p.remind_after <= now).cloned() else {
            self.show_recording_popup = false;
            return;
        };
        let mut action: Option<&str> = None;
        egui::Window::new("Your test recording is still saved")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label("You can delete it if you're done with it. Preflight only deletes this one file, and only if you click Delete.");
                let name = std::path::Path::new(&rec.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                theme::inset(ui, |ui| {
                    ui.label(RichText::new(name).strong());
                    theme::small(ui, format!("{} · {}", crate::util::fmt_bytes(rec.size), rec.path));
                });
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete").color(BG).strong()).fill(BAD)).clicked() {
                        action = Some("delete");
                    }
                    if theme::secondary(ui, "Keep").clicked() {
                        action = Some("keep");
                    }
                    if theme::secondary(ui, "Remind me later").clicked() {
                        action = Some("later");
                    }
                });
            });
        match action {
            Some("delete") => {
                let r = std::fs::remove_file(&rec.path);
                self.st.pending_recordings.retain(|p| p.path != rec.path);
                self.st.save();
                match r {
                    Ok(_) => {
                        log::info!("Test recording deleted by the user");
                        self.toast(ctx, "Test recording deleted");
                    }
                    Err(e) => self.toast(ctx, format!("Couldn't delete it: {e}")),
                }
                self.show_recording_popup = false;
            }
            Some("keep") => {
                self.st.pending_recordings.retain(|p| p.path != rec.path);
                self.st.save();
                self.show_recording_popup = false;
            }
            Some("later") => {
                for p in self.st.pending_recordings.iter_mut() {
                    if p.path == rec.path {
                        p.remind_after = now + 24 * 3600;
                    }
                }
                self.st.save();
                self.show_recording_popup = false;
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // First-run setup: 3 short screens

    fn style_cards(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        ui.columns(3, |cols| {
            for (i, (style, name, desc)) in [
                (Style::Competitive, "Competitive", "Highest, steadiest FPS. Stream quality comes second. (e.g. CS2 ranked)\nFirst fix: lower heavy graphics and keep your FPS high."),
                (Style::Balanced, "Balanced", "Smooth game and a clean stream, with small trade-offs. (e.g. Fortnite, Halo)\nFirst fix: DLSS/FSR, or an FPS cap."),
                (Style::Cinematic, "Cinematic", "The game and stream look their best. Lower FPS is fine. (e.g. Cyberpunk 2077)\nFirst fix: an FPS cap, so the graphics stay high."),
            ]
            .into_iter()
            .enumerate()
            {
                if choice_tile(&mut cols[i], self.st.style == style, name, desc) {
                    self.st.apply_style(style);
                    changed = true;
                }
            }
        });
        if let Some(g) = self.current_game() {
            theme::small(ui, format!("Saved for {g}. Each game remembers its own style."));
        }
        if changed {
            let ctx = ui.ctx().clone();
            self.on_style_changed(&ctx);
        }
        changed
    }

    /// A new style changes which fix comes first: redo the advice, keep the
    /// progress, and say what changed.
    fn on_style_changed(&mut self, ctx: &egui::Context) {
        let before: Option<String> = self.analysis.issues.iter().find(|i| i.id.starts_with("gpu_")).and_then(|i| i.options.iter().find(|o| o.suggested)).map(|o| o.name.clone());
        if let Some(g) = self.current_game() {
            self.st.style_by_game.insert(g, self.st.style);
        }
        self.st.save();
        self.picks.clear();
        self.recompute();
        let cur_id = self.fix_queue.get(self.fix_idx).map(|i| i.id.clone());
        self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
        self.fix_idx = cur_id.and_then(|id| self.fix_queue.iter().position(|i| i.id == id)).unwrap_or(0);
        let after: Option<String> = self.analysis.issues.iter().find(|i| i.id.starts_with("gpu_")).and_then(|i| i.options.iter().find(|o| o.suggested)).map(|o| o.name.clone());
        let name = match self.st.style {
            Style::Competitive => "Competitive",
            Style::Balanced => "Balanced",
            Style::Cinematic => "Cinematic",
            Style::Custom => "Custom",
        };
        let msg = match (&before, &after) {
            (_, None) => format!("Style: {name}. Your GPU has room, so there's no game change to suggest right now."),
            (Some(b), Some(a)) if b == a => format!("Style: {name}. The first fix stays the same: {}.", lower_first(a.trim_end_matches('.'))),
            (_, Some(a)) => format!("Style: {name}. First fix is now: {}.", lower_first(a.trim_end_matches('.'))),
        };
        log::info!("{msg}");
        self.toast(ctx, msg);
    }

    fn questions(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        ui.label(RichText::new("When your game and your stream compete for power, which wins?").strong());
        let q1 = self.st.q_game_stream();
        ui.columns(3, |c| {
            for (i, (t, d)) in [("My gameplay", "Keep the game smooth. The stream can be a little softer."), ("Keep both good", "Small trade-offs on each side."), ("My stream", "Sharpest picture for viewers, even if I lose some FPS.")].into_iter().enumerate() {
                if choice_tile(&mut c[i], q1 == i as u8, t, d) {
                    self.st.set_q_game_stream(i as u8);
                    changed = true;
                }
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("Inside the game, what do you prefer?").strong());
        let q2 = self.st.q_fps_visuals();
        ui.columns(3, |c| {
            for (i, (t, d)) in [("More FPS", "Lower graphics are fine if it feels faster."), ("A mix", "Good-looking and smooth."), ("Better graphics", "Ray tracing and high settings, even at lower FPS.")].into_iter().enumerate() {
                if choice_tile(&mut c[i], q2 == i as u8, t, d) {
                    self.st.set_q_fps_visuals(i as u8);
                    changed = true;
                }
            }
        });
        changed
    }

    fn monitor_picker(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        if self.monitors.is_empty() {
            theme::muted(ui, if self.scan.is_some() { "Looking for your monitors…" } else { "No monitors were detected automatically. Set your refresh rate below." });
        }
        for m in self.monitors.clone() {
            let on = self.st.monitor_name == m.name;
            let title = format!("{}{}", m.name, if m.primary { " (main display)" } else { "" });
            let mut desc = format!("{} × {} · running at {} Hz · supports up to {} Hz", m.width, m.height, m.current_hz, m.max_hz);
            if m.underclocked() {
                desc.push_str(&format!(
                    "\nSet to {} Hz but can do {} Hz. Change it in {}.",
                    m.current_hz,
                    m.max_hz,
                    if cfg!(windows) { "Windows Settings › System › Display › Advanced display" } else { "your system's display settings" }
                ));
            }
            if choice_tile(ui, on, &title, &desc) {
                self.st.monitor_name = m.name.clone();
                self.st.monitor_hz = m.max_hz.max(m.current_hz);
                self.st.monitor_auto = true;
                changed = true;
            }
            ui.add_space(6.0);
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Refresh rate used for suggestions").color(MUTED));
            egui::ComboBox::from_id_salt("hz").selected_text(format!("{} Hz", self.st.monitor_hz)).show_ui(ui, |ui| {
                for hz in [60, 75, 100, 120, 144, 165, 180, 200, 240, 280, 360, 480, 500] {
                    if ui.selectable_value(&mut self.st.monitor_hz, hz, format!("{hz} Hz")).changed() {
                        self.st.monitor_auto = false;
                        changed = true;
                    }
                }
            });
        });
        changed
    }

    fn ui_setup(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::util::SHORT_NAME).size(20.0).strong().color(ACCENT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (i, l) in [(3u8, "3 · Connect OBS"), (2, "2 · Your monitor"), (1, "1 · Your style")] {
                    let (fg, bg) = if self.setup_screen == i {
                        (Color32::from_rgb(0xD9, 0xF9, 0x9D), ACCENT_BG)
                    } else if self.setup_screen > i {
                        (GOOD, GOOD_BG)
                    } else {
                        (MUTED, CARD)
                    };
                    theme::pill(ui, fg, bg, l);
                }
            });
        });
        ui.add_space(14.0);
        let line = self.next_line();
        theme::inset(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Next").color(ACCENT).strong());
                ui.label(line);
            });
        });
        ui.add_space(12.0);
        match self.setup_screen {
            1 => {
                theme::title(ui, "What matters most when you stream?");
                theme::subtitle(ui, "This decides which fix Preflight suggests first. You'll always see every option.");
                ui.add_space(14.0);
                self.style_cards(ui);
                ui.add_space(12.0);
                egui::CollapsingHeader::new(RichText::new("Fine-tune (optional)").strong()).id_salt("finetune").show(ui, |ui| {
                    self.questions(ui);
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label("Streaming to");
                    for p in [Platform::Twitch, Platform::YouTube, Platform::Kick] {
                        ui.selectable_value(&mut self.st.platform, p, p.name());
                    }
                });
                ui.add_space(16.0);
                if theme::primary(ui, "  Continue  ").clicked() {
                    self.setup_screen = 2;
                }
            }
            2 => {
                theme::title(ui, "Which monitor do you play on?");
                theme::subtitle(ui, "Found automatically. FPS suggestions are based on its refresh rate.");
                ui.add_space(14.0);
                self.monitor_picker(ui);
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if theme::primary(ui, "  Continue  ").clicked() {
                        self.setup_screen = 3;
                    }
                    if theme::secondary(ui, "Back").clicked() {
                        self.setup_screen = 1;
                    }
                });
            }
            _ => {
                theme::title(ui, "Connect OBS");
                theme::subtitle(ui, "A one-time step so Preflight can read your stream settings. It never changes anything in OBS.");
                ui.add_space(10.0);
                ui.label("1. In OBS, open Tools › WebSocket Server Settings.");
                ui.label("2. Tick Enable WebSocket server.");
                ui.label("3. Click Show Connect Info, copy the password, and paste it below.");
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("OBS password").strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(280.0));
                        let busy = self.conn_rx.is_some();
                        if ui.add_enabled(!busy, egui::Button::new(if busy { "Connecting…" } else { "Test connection" })).clicked() {
                            self.connect_obs(ctx);
                        }
                    });
                    ui.checkbox(&mut self.st.obs_remember_password, "Remember it on this PC");
                    match (&self.obs, &self.obs_error) {
                        (Some(o), _) if o.connected => {
                            ui.label(
                                RichText::new(format!(
                                    "Connected to OBS {} · {} at {}{}, {}",
                                    o.obs_version,
                                    o.encoder_kind,
                                    o.output_res.map(|r| format!("{}p", r.1)).unwrap_or_default(),
                                    o.fps.map(|f| (f.round() as u32).to_string()).unwrap_or_default(),
                                    o.bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default()
                                ))
                                .color(GOOD)
                                .strong(),
                            );
                        }
                        (_, Some(e)) => {
                            ui.label(RichText::new(e).color(WARN));
                        }
                        _ => {}
                    }
                });
                ui.add_space(16.0);
                let mut finish = false;
                ui.horizontal(|ui| {
                    if theme::primary(ui, "  Finish and start my first pre-flight  ").clicked() {
                        finish = true;
                    }
                    if theme::secondary(ui, "Back").clicked() {
                        self.setup_screen = 2;
                    }
                    if ui.link("I don't use OBS / skip").clicked() {
                        finish = true;
                    }
                });
                if finish {
                    self.st.setup_done = true;
                    if self.st.obs_remember_password && !self.password.is_empty() {
                        crate::config::save_secret(&self.password);
                    }
                    self.st.save();
                    self.screen = Screen::Preflight;
                    self.step = 2;
                    self.start_scan(ctx.clone(), true);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pre-flight

    fn ui_preflight(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.step < 2 {
            self.step = 2;
        }
        self.page_header(ui, "Pre-flight");
        self.ui_game_switch(ui);
        if self.returning {
            let mut rerun = false;
            egui::Frame::NONE
                .fill(GOOD_BG)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2F, 0x6B, 0x47)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Welcome back. Nothing has changed since your last pre-flight.").strong());
                    let when = self.st.last_preflight.as_ref().map(|l| history::fmt_time(l.time)).unwrap_or_default();
                    ui.label(RichText::new(format!("Same game, settings and OBS scene as {when}, so you can go straight to Ready to go live.")).color(TEXT_SOFT));
                    let (imp, opt) = self.open_counts();
                    if imp + opt > 0 {
                        ui.label(RichText::new(format!("Still open from last time: {}. Click Fix to go through them.", count_words(imp, opt))).color(WARN));
                    }
                    if theme::secondary(ui, "Quick re-check").clicked() {
                        rerun = true;
                    }
                });
            if rerun {
                self.returning = false;
                self.step = 2;
                self.start_scan(ctx.clone(), true);
            }
            ui.add_space(10.0);
        }
        // The scan shows here only while it runs; its results live under Your setup.
        if let Some(job) = &self.scan {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(&job.step).color(TEXT_SOFT));
                if job.step.contains("upload") {
                    let p = job.net_progress.lock().map(|p| *p).unwrap_or(0.0);
                    ui.label(RichText::new(format!("{:.0}%", p * 100.0)).color(MUTED));
                }
            });
            ui.add_space(10.0);
        }
        match self.step {
            3 => self.ui_step3(ui),
            4 => self.ui_step4(ui, ctx),
            _ => self.ui_step2(ui, ctx),
        }
    }

    /// Page title on the left; the two scores and "Why?" on the right.
    fn page_header(&mut self, ui: &mut Ui, title: &str) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let (left, total) = self.fix_progress();
                let sub = if self.live.is_some() { "Pre-flight · you're live".to_string() } else if left > 0 { format!("Pre-flight · {left} of {total} fixes left") } else { "Pre-flight".to_string() };
                ui.label(theme::caps(&sub, 12.0, ACCENT));
                let head = match self.step {
                    2 => "Test while you play",
                    3 if left > 0 => "Fix what matters",
                    3 => "All fixes done",
                    4 if self.live.is_some() => "Live now",
                    4 => "Ready to go live",
                    _ => title,
                };
                ui.label(RichText::new(head).size(30.0).strong().color(TEXT));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                let why = ui.add(egui::Button::new(theme::caps("Why?", 12.0, ACCENT)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand);
                egui::Popup::from_toggle_button_response(&why).align(egui::RectAlign::BOTTOM_END).width(380.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    self.ui_score_why(ui);
                });
                theme::dial(ui, Some(self.analysis.setup_score), "Setup", ACCENT);
                let sm = self.analysis.smoothness;
                theme::dial(ui, sm, "Smoothness", sm.map(theme::score_color).unwrap_or(MUTED));
            });
        });
        ui.add_space(12.0);
    }

    fn ui_score_why(&self, ui: &mut Ui) {
        ui.label(RichText::new(&self.analysis.verdict).strong());
        ui.add_space(4.0);
                let mut open: Vec<&Issue> = self.analysis.issues.iter().filter(|i| !self.st.kept_issues.contains(&i.id) && !analysis::unscored(&i.id)).collect();
                open.sort_by_key(|i| -analysis::points_of(i.priority));
                let tested = self.analysis.smoothness.is_some();
                ui.label(RichText::new("Smoothness: what the test measured").strong());
                if !tested {
                    theme::muted(ui, "Run a test while you play to measure it.");
                } else {
                    let m: Vec<&&Issue> = open.iter().filter(|i| i.measured).collect();
                    if m.is_empty() {
                        theme::muted(ui, "Nothing. No stutter, skipped frames or overload in the test.");
                    }
                    for i in m {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("-{}", analysis::points_of(i.priority))).monospace().color(WARN));
                            ui.label(&i.title);
                        });
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Setup: settings that will help").strong());
                let st_issues: Vec<&&Issue> = open.iter().filter(|i| !(i.measured && tested)).collect();
                if st_issues.is_empty() {
                    theme::muted(ui, "Nothing. Every setting is fine or kept by your choice.");
                }
                for i in st_issues {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("-{}", analysis::points_of(i.priority))).monospace().color(WARN));
                        ui.label(&i.title);
                    });
                }
                let kept: Vec<&Issue> = self.analysis.issues.iter().filter(|i| self.st.kept_issues.contains(&i.id)).collect();
                if !kept.is_empty() {
                    theme::small(ui, format!("Not counted (you chose to keep): {}", kept.iter().map(|i| i.title.trim_end_matches('.')).collect::<Vec<_>>().join("; ")));
                }
                let unknown = self.analysis.checks.iter().filter(|c| c.status == Status::Unknown).count();
                if unknown > 0 {
                    theme::small(ui, format!("-{} for {unknown} check{} that couldn't run.", unknown * 3, if unknown == 1 { "" } else { "s" }));
                }
                }

    fn scan_items(&self) -> Vec<(Status, String, &'static str, String)> {
        let mut v = Vec::new();
        match &self.hw {
            Some(h) => {
                let strong = h.ram_gb >= 15.0 && h.cpu_threads >= 8 && h.has_hw_encoder();
                let text = match (h.main_gpu(), h.encoders.first()) {
                    (Some(g), Some(e)) => format!("{} and {}, with {e} for streaming.", g.name, h.cpu),
                    (Some(g), None) => format!("{} and {}. No hardware encoder was found, so OBS would stream on the CPU.", g.name, h.cpu),
                    (None, _) => format!("{}. No graphics card was found, so OBS would stream on the CPU.", h.cpu),
                };
                v.push((if strong { Status::Ok } else { Status::Warn }, if strong { "Strong" } else { "Limited" }.to_string(), "Hardware", text));
            }
            None => v.push((Status::Unknown, "Checking".into(), "Hardware", "Checking your hardware…".into())),
        }
        match (&self.obs, &self.obs_error) {
            (Some(o), _) if o.connected => v.push((
                Status::Ok,
                "Connected".into(),
                "OBS",
                format!(
                    "Streaming {} at {} fps, {}, using {}.",
                    o.output_res.map(|r| format!("{}p", r.1)).unwrap_or_else(|| "?".into()),
                    o.fps.map(|f| (f.round() as u32).to_string()).unwrap_or_else(|| "?".into()),
                    o.bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_else(|| "bitrate unknown".into()),
                    o.encoder_kind
                ),
            )),
            (_, Some(e)) => v.push((Status::Warn, "Not connected".into(), "OBS", e.clone())),
            _ => v.push((Status::Unknown, "Checking".into(), "OBS", "Looking for OBS…".into())),
        }
        let found: Vec<&GameResult> = self.games.iter().filter(|g| g.relevant()).collect();
        let read = found.iter().filter(|g| g.files.iter().any(|f| f.read_ok)).count();
        let waiting = found.len() - read;
        v.push((
            if found.is_empty() { Status::Unknown } else if waiting > 0 { Status::Warn } else { Status::Ok },
            if found.is_empty() { "None found".to_string() } else if waiting > 0 { "Check".into() } else { "Ready".into() },
            "Games",
            if found.is_empty() {
                "None of the 7 supported games were found.".into()
            } else if waiting > 0 {
                format!("{} found. Settings read for {read}; {waiting} not launched on this PC yet.", found.len())
            } else {
                format!("{} found, and settings read for all of them.", found.len())
            },
        ));
        match &self.net {
            Some(n) if n.ok => {
                let b = self.obs.as_ref().and_then(|o| o.bitrate_kbps).unwrap_or(6000);
                let ratio = n.low_mbps / ((b as f32 + 160.0) / 1000.0);
                let s = if ratio >= 3.0 { Status::Ok } else if ratio >= 1.0 { Status::Warn } else { Status::Bad };
                v.push((s, if ratio >= 3.0 { "Plenty" } else if ratio >= 1.0 { "Tight" } else { "Too slow" }.to_string(), "Internet", format!("{:.0} Mbps up, dipping to {:.0}. That's about {:.0} times what your stream needs.", n.avg_mbps, n.low_mbps, ratio)));
            }
            Some(n) => v.push((Status::Warn, "Not tested".into(), "Internet", n.error.clone().unwrap_or_default())),
            None => v.push((Status::Unknown, if self.scan.is_some() { "Checking" } else { "Not tested" }.to_string(), "Internet", if self.st.network_test_on_scan { "Tested during the scan.".into() } else { "Upload test is turned off in Settings.".into() })),
        }
        v
    }

    fn ui_step2(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut action: Option<&str> = None;
        let mut rec_act: Option<(String, String)> = None;
        let phase = self.test_handle.as_ref().map(|h| h.phase());
        let (elapsed, last, shots) = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| (s.elapsed, s.last.clone(), s.shots))).unwrap_or((0.0, None, 0));
        let show_done = self.test.is_some() && !self.want_new_test && !matches!(phase, Some(Phase::Measuring));
        theme::hero_card(ui, |ui| {
            ui.vertical(|ui| { do_next_label(ui); ui.label(RichText::new(match self.games.iter().find(|g| g.running) { Some(g) => format!("Run a test with {} open", g.name), None => "Pick a test, then start your game".to_string() }).size(24.0).strong()); });
            ui.add_space(6.0);
            if matches!(phase, Some(Phase::Measuring)) {
                let target = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| s.target)).unwrap_or(livetest::TARGET_SECONDS);
                let left = (target - elapsed).max(0.0);
                ui.horizontal(|ui| {
                    let head = if left > 0.0 {
                        "Measuring · keep playing".to_string()
                    } else if shots < livetest::PREVIEW_SHOTS {
                        format!("Grabbing preview frames ({shots} of {}) · keep playing", livetest::PREVIEW_SHOTS)
                    } else {
                        "All done · stop the recording in OBS".to_string()
                    };
                    ui.label(RichText::new(head).size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{}:{:02} left", (left as u32) / 60, (left as u32) % 60)).monospace());
                    });
                });
                theme::bar(ui, elapsed / target, ACCENT, 10.0);
                if let Some(s) = last {
                    ui.columns(3, |c| {
                        let gpu = s.gpu.map(|g| if g >= 92.0 { format!("Maxed out · {g:.0}%") } else { format!("Has room · {g:.0}%") }).unwrap_or_else(|| "Not readable".into());
                        theme::inset(&mut c[0], |ui| {
                            theme::small(ui, "GPU");
                            ui.label(RichText::new(gpu).strong());
                        });
                        theme::inset(&mut c[1], |ui| {
                            theme::small(ui, "CPU");
                            ui.label(RichText::new(if s.cpu >= 85.0 { format!("Nearly full · {:.0}%", s.cpu) } else { format!("Plenty spare · {:.0}%", s.cpu) }).strong());
                        });
                        theme::inset(&mut c[2], |ui| {
                            theme::small(ui, "GPU temperature");
                            ui.label(RichText::new(s.gpu_temp.map(|t| if t >= 87.0 { format!("Hot · {t:.0}°C") } else { format!("Normal · {t:.0}°C") }).unwrap_or_else(|| "Not readable".into())).strong());
                        });
                    });
                }
                // Encoders working right now.
                let busy: Vec<String> = self.encoders.iter().filter(|e| e.in_use()).map(|e| format!("{}: {}", e.kind, e.use_text().trim_start_matches("In use: "))).collect();
                if !busy.is_empty() {
                    theme::small(ui, format!("Encoders: {}", busy.join(" · ")));
                }
                theme::small(ui, "When the bar fills, keep playing about 10 more seconds while Preflight grabs frames for the viewer preview. Then stop the recording in OBS. Preflight finishes by itself.");
                if link_small(ui, "Stop the test now") {
                    action = Some("stop");
                }
                return;
            }
            if show_done {
                let t = self.test.clone().unwrap();
                ui.label(RichText::new(format!("Test complete{} · {}m {:02}s", t.game_running.as_ref().map(|g| format!(" · {g}")).or_else(|| t.game_guess.as_ref().map(|g| format!(" · {g} (not a game Preflight knows yet)"))).unwrap_or_default(), (t.seconds as u32) / 60, (t.seconds as u32) % 60)).size(14.0).color(MUTED));
                let bullets = self.test_summary(&t);
                // Verdict first.
                let bad: Vec<&(String, String, Color32)> = bullets.iter().filter(|b| b.2 == BAD).collect();
                let warn: Vec<&(String, String, Color32)> = bullets.iter().filter(|b| b.2 == WARN).collect();
                let (verdict, col) = if let Some(b) = bad.first() {
                    (format!("Viewers would notice problems. Main one: {}", lower_first(&b.0)), BAD)
                } else if let Some(w) = warn.first() {
                    (format!("Your stream and recording would mostly be smooth. One thing to watch: {}", lower_first(&w.0)), WARN)
                } else {
                    ("Your stream and recording would be smooth.".to_string(), GOOD)
                };
                ui.label(RichText::new(verdict).size(19.0).strong().color(col));
                let (imp, opt) = self.open_counts();
                if imp + opt > 0 {
                    ui.label(RichText::new(format!("Your settings: {}. Step 2 goes through them one at a time.", count_words(imp, opt))).color(TEXT_SOFT));
                }
                // "Check my changes": before and after.
                if let Some(c) = &self.compare {
                    let sk = |r: &TestResult| if r.output_total > 0 { r.output_skipped as f32 * 100.0 / r.output_total as f32 } else { 0.0 };
                    let (sb, sa) = (sk(&c.before), sk(&c.after));
                    let better = c.after_score > c.before_score || sa + 0.3 < sb || maxed_pct(&c.after) + 10.0 < maxed_pct(&c.before);
                    let worse = c.after_score < c.before_score || sa > sb + 0.3 || maxed_pct(&c.after) > maxed_pct(&c.before) + 10.0;
                    // No setting change seen, but a very different load: probably a different part of the game.
                    let gpu_gap = (c.before.gpu_avg.unwrap_or(0.0) - c.after.gpu_avg.unwrap_or(0.0)).abs();
                    // What changed between the two tests, in OBS and in the game.
                    let obs_diff: Vec<String> = match (&c.before.obs_settings, &c.after.obs_settings) {
                        (Some(b), Some(a)) => b.changes_to(a),
                        _ => Vec::new(),
                    };
                    let game_saved = c.before.settings_saved.is_some() && c.before.settings_saved != c.after.settings_saved;
                    let same_settings = c.before.settings_saved.is_some() && !game_saved && obs_diff.is_empty() && !self.fixed.iter().any(|f| analysis::place_of(f) != analysis::Place::Game);
                    let not_comparable = same_settings && (gpu_gap > 15.0 || (maxed_pct(&c.before) - maxed_pct(&c.after)).abs() > 25.0);
                    let (line, fill, stroke) = match (better, worse) {
                        _ if not_comparable => ("The two tests look too different to compare. The game didn't save a settings change, so this was probably a lighter or busier part of the game. Play the same spot and check again.", CARD, BORDER),
                        (true, false) => ("Your changes helped.", GOOD_BG, Color32::from_rgb(0x2F, 0x6B, 0x47)),
                        (false, true) => ("It got worse after your changes. Check what you changed, or undo it.", BAD_BG, Color32::from_rgb(0x6B, 0x2B, 0x2B)),
                        (true, true) => ("Some things got better and some worse.", CARD, BORDER),
                        _ => ("About the same as before. The change didn't make a measurable difference.", CARD, BORDER),
                    };
                    egui::Frame::NONE.fill(fill).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format!("Your changes, before and after: {}", lower_first(line))).strong());
                        egui::Grid::new("compare").num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                            ui.label("");
                            ui.label(RichText::new("Before").color(MUTED));
                            ui.label(RichText::new("After").color(MUTED));
                            ui.end_row();
                            for (k, b, a) in [
                                ("Smoothness", c.before_score.to_string(), c.after_score.to_string()),
                                ("GPU maxed out", format!("{:.0}% of the test", maxed_pct(&c.before)), format!("{:.0}% of the test", maxed_pct(&c.after))),
                                ("GPU average", c.before.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "—".into()), c.after.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "—".into())),
                                ("Skipped frames (encoding lag)", format!("{:.2}%", sk(&c.before)), format!("{:.2}%", sk(&c.after))),
                            ] {
                                if b == "—" && a == "—" {
                                    continue;
                                }
                                ui.label(RichText::new(k).color(MUTED));
                                ui.label(b);
                                ui.label(RichText::new(a).strong());
                                ui.end_row();
                            }
                        });
                        let mut changed: Vec<String> = obs_diff.iter().map(|d| format!("OBS: {d}")).collect();
                        if game_saved {
                            changed.push(format!("{}: settings saved again", c.after.game_running.clone().unwrap_or_else(|| "Game".into())));
                        }
                        if !changed.is_empty() {
                            theme::small(ui, format!("Changed between the tests: {}", changed.join(" · ")));
                        } else if c.before.obs_settings.is_some() {
                            theme::small(ui, "No OBS or game setting changed between the tests.");
                        }
                    });
                    ui.add_space(4.0);
                }
                match (&self.preview, self.preview_rx.is_some()) {
                    (_, true) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Making your viewer preview…");
                        });
                    }
                    (Some(p), _) => {
                        if let Some(db) = p.worst().and_then(|w| w.match_db) {
                            let (head, meaning) = crate::preview::meaning(db);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("Viewer preview: {head}.")).strong());
                                ui.label(RichText::new(meaning).color(TEXT_SOFT));
                                if link_small(ui, "See it") {
                                    action = Some("results");
                                    self.scroll_to_preview = true;
                                }
                            });
                        } else if p.ffmpeg_missing {
                            theme::muted(ui, "The viewer preview needs ffmpeg to read your recording. See Your setup for how to get it.");
                        }
                    }
                    _ => {}
                }
                egui::CollapsingHeader::new(RichText::new(format!("Details ({} measurements)", bullets.len())).strong()).id_salt("test-details").show(ui, |ui| {
                    for n in &t.notes {
                        theme::small(ui, n);
                    }
                    for (head, meaning, col) in &bullets {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(head).strong().color(*col));
                            ui.label(RichText::new(meaning).color(TEXT_SOFT));
                        });
                    }
                });
                if let Some(rec) = t.recording_path.as_ref().and_then(|p| self.st.pending_recordings.iter().find(|r| &r.path == p)).cloned() {
                    rec_act = self.recording_row(ui, &rec, "Test recording:");
                }
                ui.add_space(4.0);
                // You record with its own encoder but this test didn't stream:
                // offer the test that measures both.
                let separate_rec = self.obs.as_ref().map(|o| o.recording_uses_stream_settings == Some(false)).unwrap_or(false);
                if t.streamed {
                    theme::small(ui, "This was a stream + record test: both were measured together.");
                } else if separate_rec {
                    ui.horizontal_wrapped(|ui| {
                        theme::small(ui, "This test measured your recording only. You record with its own encoder, so test both together for the real load.");
                        if link_small(ui, "Test streaming and recording together") {
                            action = Some("stream_record_now");
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "See your setup").clicked() {
                        action = Some("results");
                    }
                    let again = match self.st.last_test.as_str() {
                        "quick" => "Test again (quick check)",
                        "stream_record" => "Test again (stream + record)",
                        _ => "Test again",
                    };
                    if theme::secondary(ui, again).clicked() {
                        action = Some("again");
                    }
                });
                return;
            }
            // Which test? Remembered for next time.
            let separate_gpu_rec = self.obs.as_ref().map(|o| o.recording_uses_stream_settings == Some(false) && !o.rec_encoder_kind.is_empty() && !o.rec_encoder_kind.contains("CPU")).unwrap_or(false);
            let kind = if self.quick_next { "quick" } else if self.show_bw_help { "stream_record" } else { "full" };
            ui.label(RichText::new("Which test?").strong());
            let mut chosen: Option<&str> = None;
            ui.columns(3, |cols| {
                let tiles = [
                    ("full", "Full test · 2 min", if separate_gpu_rec { "Measures your recording. Pick stream + record to include your stream too." } else { "Measures your stream settings while you play. Recommended." }),
                    ("stream_record", "Stream + record · 2 min", if separate_gpu_rec { "Recommended for you: you record with its own encoder, so this measures your real load." } else { "For when you stream and record at the same time (Twitch test mode)." }),
                    ("quick", "Quick check · 1 min", "After a change: a shorter test to compare with the last one."),
                ];
                for (i, (id, name, desc)) in tiles.into_iter().enumerate() {
                    if choice_tile(&mut cols[i], kind == id, name, desc) {
                        chosen = Some(id);
                    }
                }
            });
            if let Some(id) = chosen {
                self.quick_next = id == "quick";
                self.show_bw_help = id == "stream_record";
                self.st.last_test = id.to_string();
                self.st.save();
                // The watcher was started for the old choice: restart it so
                // the test length matches.
                if let Some(h) = &self.test_handle {
                    if !matches!(h.phase(), Phase::Measuring) {
                        h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                        self.test_handle = None;
                        self.watch_retry_at = 0.0;
                    }
                }
            }
            ui.add_space(6.0);
            // One checklist that ticks itself as Preflight sees each thing happen.
            let connected = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
            let (rec_on, stream_on) = self.test_handle.as_ref().and_then(|h| h.state.lock().ok().map(|s| (s.rec_active, s.stream_active))).unwrap_or((false, false));
            let waiting = matches!(phase, Some(Phase::WaitingForRecord));
            let platforms: Vec<String> = if self.st.platforms.is_empty() { vec![self.st.platform.name().to_string()] } else { self.st.platforms.clone() };
            let game_running = self.games.iter().find(|g| g.running).map(|g| g.name.clone());
            ui.label(RichText::new("Each line ticks itself when Preflight sees it. Preflight only watches OBS; it never controls it or touches your stream key.").color(TEXT_SOFT));
            ui.add_space(4.0);
            let mut n = 0;
            let mut num = || {
                n += 1;
                n
            };
            // OBS
            let obs_line = format!("{}. OBS is open and connected", num());
            if connected {
                tick_row(ui, Tick::Done, &obs_line, &format!("OBS {}", self.obs.as_ref().map(|o| o.obs_version.clone()).unwrap_or_default()), |_| {});
            } else {
                let e = self.obs_error.clone().unwrap_or_else(|| "Open OBS with its WebSocket server on. Preflight connects by itself.".into());
                let (l, a) = err_action(&e).unwrap_or(("Show me how", NextAct::ObsHelp));
                tick_row(ui, Tick::Watch, &obs_line, &e, |ui| {
                    if outlined(ui, l).clicked() {
                        self.pending_act = Some(a);
                    }
                });
            }
            // Game
            let gl = format!("{}. Your game is running", num());
            match &game_running {
                Some(g) => tick_row(ui, Tick::Done, &gl, &format!("{g}. Get into real gameplay before recording."), |_| {}),
                None => tick_row(ui, Tick::Watch, &gl, "Start your game and Preflight spots it. Other games work too; they get general advice.", |_| {}),
            }
            // Recording must match the stream (normal test only).
            let o = self.obs.clone();
            if !self.show_bw_help {
                if let Some(false) = o.as_ref().and_then(|o| o.recording_uses_stream_settings) {
                    let own_gpu = o.as_ref().map(|o| !o.rec_encoder_kind.is_empty() && !o.rec_encoder_kind.contains("CPU")).unwrap_or(false);
                    if own_gpu {
                        tick_row(ui, Tick::Info, "You record with its own encoder", "A normal test measures only the recording. Stream + record measures your real load.", |ui| {
                            if outlined(ui, "Use stream + record").clicked() {
                                self.show_bw_help = true;
                                self.st.last_test = "stream_record".into();
                                self.st.save();
                            }
                        });
                    } else {
                        let adv = o.as_ref().map(|o| o.output_mode.eq_ignore_ascii_case("advanced")).unwrap_or(false);
                        tick_row(ui, Tick::Warn, "Fix first: recording doesn't use your stream settings", if adv { "OBS › Settings › Output › Recording › Video Encoder › (Use stream encoder)." } else { "OBS › Settings › Output › Recording Quality › Same as stream." }, |_| {});
                    }
                }
            }
            if self.show_bw_help {
                // Test mode for each platform: the one step done by hand.
                for p in &platforms {
                    let ok = self.private_ok.contains(p) || (p == "Twitch" && self.st.bandwidth_test_pending);
                    let st = if ok { Tick::Done } else { Tick::Wait };
                    match p.as_str() {
                        "Twitch" => {
                            tick_row(ui, st, &format!("{}. Twitch test mode: add ?bandwidthtest=true to the end of your stream key", num()), "In OBS › Settings › Stream. Viewers won't see the test stream.", |ui| {
                                if outlined(ui, "Copy").clicked() {
                                    ui.ctx().copy_text("?bandwidthtest=true".into());
                                    action = Some("bw_copied");
                                }
                                if !ok && outlined(ui, "I added it").clicked() {
                                    action = Some("bw_added");
                                }
                            });
                        }
                        "YouTube" => {
                            tick_row(ui, st, &format!("{}. YouTube test mode: set the stream to Unlisted", num()), "YouTube Studio › Create › Go live, then use that stream's key.", |ui| {
                                if !ok && outlined(ui, "It's Unlisted").clicked() {
                                    self.private_ok.insert("YouTube".into());
                                }
                            });
                        }
                        other => {
                            let other = other.to_string();
                            tick_row(ui, st, &format!("{}. {other} has no hidden test mode", num()), "This test would go out to your viewers.", |ui| {
                                if !ok {
                                    if outlined(ui, "Measure my next real stream").clicked() {
                                        action = Some("measure_next_stream");
                                    }
                                    if outlined(ui, "Test live anyway").clicked() {
                                        self.private_ok.insert(other.clone());
                                    }
                                }
                            });
                        }
                    }
                }
                let sl = format!("{}. Start streaming in OBS", num());
                tick_row(ui, if stream_on { Tick::Done } else if connected && waiting { Tick::Watch } else { Tick::Wait }, &sl, "", |_| {});
            }
            let rl = format!("{}. Start recording in OBS", num());
            tick_row(ui, if rec_on { Tick::Done } else if connected && waiting { Tick::Watch } else { Tick::Wait }, &rl, if self.show_bw_help { "Measuring starts when both are running, in either order." } else { "Measuring starts by itself. Your normal recording settings are fine." }, |_| {});
            if waiting && elapsed > 20.0 && !rec_on {
                ui.label(RichText::new("Still waiting. Check that OBS shows a red Record button and a running timer.").size(13.0).color(WARN));
            }
            if self.show_bw_help && platforms.iter().any(|p| p == "Twitch") {
                theme::small(ui, "Afterwards, remove ?bandwidthtest=true again, or your next stream won't reach viewers. Preflight keeps reminding you until you confirm.");
            }
            ui.add_space(6.0);
            // What happened last time in this game (results are remembered per game).
            if let Some(g) = self.games.iter().find(|g| g.running) {
                if let Some(m) = self.st.game_memory.get(&g.name) {
                    let when = crate::util::fmt_local(m.time);
                    theme::inset(ui, |ui| {
                        ui.label(RichText::new(format!("Last time in {} ({when})", g.name)).strong());
                        ui.label(RichText::new(format!("Smoothness {} · Setup {}. {}", m.smoothness, m.setup, m.verdict)).color(TEXT_SOFT));
                        if !m.open.is_empty() {
                            theme::small(ui, format!("Still open then: {}", m.open.iter().take(3).cloned().collect::<Vec<_>>().join("; ")));
                        }
                    });
                }
            }
            // Before another test: offer to delete the last test recording.
            if !matches!(phase, Some(Phase::Measuring)) {
                if let Some(rec) = self.st.pending_recordings.iter().max_by_key(|r| r.created).cloned() {
                    rec_act = self.recording_row(ui, &rec, "Your last test recording is still saved:");
                }
            }
            if self.quick_next {
                // Did the game save the change? And test the same spot.
                if let Some(prev) = self.prev_test.as_ref().map(|p| p.0.clone()) {
                    if let Some(g) = prev.game_running.clone() {
                        let now_saved = self.game_file_saved(&g);
                        let game_fix = self.fixed.iter().any(|f| analysis::place_of(f) == analysis::Place::Game);
                        if game_fix && prev.settings_saved.is_some() && now_saved == prev.settings_saved {
                            status_row(ui, Status::Warn, "Not saved", &format!("{g} hasn't saved any change since your last test"), "If you changed something in the game, press Apply (or leave its settings menu) so it saves, then test.");
                        }
                        theme::small(ui, format!("For a fair before/after, play the same part of {g} as last time."));
                    }
                }

            }
            ui.add_space(6.0);
            if link_small(ui, "Don't use OBS? Test without it (measures your PC for 2 minutes)") {
                action = Some("system");
            }
        });
        self.do_recording_act(ctx, rec_act);
        match action {
            Some("stop") => {
                if let Some(h) = &self.test_handle {
                    h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Some("to3") => self.set_step(3),
            Some("to4") => self.set_step(4),
            Some("results") => self.go(Screen::Results),
            Some("again") => {
                // The last kind of test the user picked.
                self.quick_next = self.st.last_test == "quick";
                self.show_bw_help = self.st.last_test == "stream_record";
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
            }
            Some("bw_help") => self.show_bw_help = !self.show_bw_help,
            Some("bw_copied") => self.toast(ctx, "Copied. Paste it at the very end of your stream key in OBS.".to_string()),
            Some("measure_next_stream") => {
                self.st.measure_next_stream = true;
                self.st.save();
                self.show_bw_help = false;
                self.toast(ctx, "OK. Preflight will measure your stream and recording together during your next real stream.".to_string());
            }
            Some("stream_record_now") => {
                self.show_bw_help = true;
                self.quick_next = false;
                self.st.last_test = "stream_record".into();
                self.st.save();
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
                if let Some(h) = &self.test_handle {
                    if !matches!(h.phase(), Phase::Measuring) {
                        h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                        self.test_handle = None;
                    }
                }
            }
            Some("bw_added") => {
                self.st.bandwidth_test_pending = true;
                self.st.save();
                log::info!("User added the Twitch bandwidth-test flag for a stream + record test");
            }
            Some("system") => {
                self.stop_watching_if_idle();
                self.test_handle = None;
                self.want_new_test = true;
                self.start_test(ctx, true);
            }
            _ => {}
        }
    }

    fn test_summary(&self, t: &TestResult) -> Vec<(String, String, Color32)> {
        let mut v = Vec::new();
        match t.gpu_avg {
            Some(g) if g >= 92.0 => v.push((format!("Your GPU was maxed out, at {g:.0}% on average."), "No room left for the stream encoder.".to_string(), BAD)),
            Some(g) => v.push((format!("Your GPU averaged {g:.0}%."), "There's room left for the stream encoder.".to_string(), GOOD)),
            None => v.push(("GPU usage couldn't be read on this card.".to_string(), "CPU, OBS and heat were still measured.".to_string(), MUTED)),
        }
        if t.with_obs && t.output_total > 0 {
            let f = t.output_skipped as f32 / t.output_total as f32;
            if f > 0.001 {
                v.push((format!("OBS skipped {:.1}% of stream frames, {}.", f * 100.0, if f > 0.0 { format!("about 1 in every {}", (1.0 / f).round() as u32) } else { "none".into() }), "Viewers would see occasional stutters.".into(), WARN));
            } else {
                v.push(("OBS didn't skip any stream frames.".into(), "The encoder kept up the whole time.".into(), GOOD));
            }
        }
        if let (Some(m), Some(d)) = (t.mic_db, t.desktop_db) {
            if m < d - 3.0 {
                v.push((format!("Your mic was quieter than the game, by about {:.0} dB.", d - m), analysis::mic_gap_meaning(d - m).into(), WARN));
            } else {
                v.push(("Your voice sits above the game audio.".into(), "Viewers will hear you clearly.".into(), GOOD));
            }
        }
        if t.cpu_avg >= 85.0 {
            v.push((format!("Your CPU was nearly maxed out, at {:.0}% on average.", t.cpu_avg), "The game and OBS were competing for it.".into(), BAD));
        }
        if let Some(e) = t.enc_avg {
            if e >= 85.0 {
                v.push((format!("Your encoder chip was {e:.0}% busy on average."), "It's close to its limit, so busy moments can make OBS skip frames.".into(), WARN));
            } else {
                v.push((format!("Your encoder chip was {e:.0}% busy on average."), "It has room to spare.".into(), GOOD));
            }
        }
        if let Some(o) = t.outputs.iter().find(|o| o.label == "Stream") {
            let f = o.dropped as f32 / o.total.max(1) as f32;
            v.push((
                format!("You streamed during the test, and your internet dropped {:.2}% of stream frames.", f * 100.0),
                if f < 0.001 { "Your connection handled the stream and recording together.".into() } else { "Your connection struggled while streaming.".into() },
                if f < 0.001 { GOOD } else { WARN },
            ));
        }
        // Every other output (separate recording, plugin outputs, extra streams), side by side.
        let others: Vec<String> = t.outputs.iter().filter(|o| o.label != "Stream").map(|o| format!("{}: {:.2}% skipped", o.label, o.dropped as f32 * 100.0 / o.total.max(1) as f32)).collect();
        if t.outputs.len() > 1 && !others.is_empty() {
            let bad = t.outputs.iter().any(|o| o.dropped as f32 / o.total.max(1) as f32 > 0.005);
            v.push((format!("Each output: {}", others.join(" · ")), if bad { "One of them fell behind: see which one above.".into() } else { "All of them kept up.".into() }, if bad { WARN } else { GOOD }));
        }
        if !t.encodes_seen.is_empty() {
            v.push((format!("GPU encodes running: {}", t.encodes_seen.iter().map(|s| format!("{} {} {}", s.app, s.codec, s.res).trim().to_string()).collect::<Vec<_>>().join(", ")), "Counted from your graphics driver.".into(), MUTED));
        }
        v
    }

    fn ui_step3(&mut self, ui: &mut Ui) {
        let mut action: Option<String> = None;
        let total = self.fix_queue.len();
        let cur = self.current_issue().cloned();
        let detected = self.detected.clone();
        let pausing = cur.as_ref().map(|i| analysis::is_optional(i) && !self.optional_ok && detected.is_none()).unwrap_or(false);
        let advanced = self.st.advanced_view;
        theme::hero_card(ui, |ui| {
            ui.horizontal(|ui| {
                do_next_label(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if cur.is_some() && !pausing {
                        if link_small(ui, "This looks wrong") {
                            action = Some("report".into());
                        }
                    } else if cur.is_none() {
                        ui.label(RichText::new("All done").size(13.0).color(MUTED));
                    }
                });
            });
            // Did the last game change save?
            if let Some(cw) = &self.confirm {
                let (fg, bg, text) = match &cw.result {
                    Some((true, m)) => (GOOD, GOOD_BG, m.clone()),
                    Some((false, m)) => (WARN, WARN_BG, m.clone()),
                    None => (INFO, RAISED, format!("Waiting for {} to save your change… Press Apply in the game.", cw.game)),
                };
                egui::Frame::NONE.fill(bg).corner_radius(8.0).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(text).color(fg));
                        if cw.result.is_some() && ui.small_button("OK").clicked() {
                            action = Some("confirm_ok".into());
                        }
                    });
                });
                ui.add_space(6.0);
            }
            // Checklist for the place being fixed now (ticks itself as changes are spotted).
            if let Some(i) = cur.as_ref().filter(|_| !pausing) {
                let place = analysis::place_of(&i.id);
                let items: Vec<(usize, &Issue)> = self.fix_queue.iter().enumerate().filter(|(_, q)| analysis::place_of(&q.id) == place).collect();
                let done = items.iter().filter(|(_, q)| self.fixed.contains(&q.id)).count();
                egui::CollapsingHeader::new(RichText::new(format!("{} · {done} of {} done · see the list", place.label(), items.len())).color(TEXT_SOFT)).id_salt("fix-list").show(ui, |ui| {
                    // Keep it short: the one being fixed now and the next few. Done and skipped ones are counted.
                    let skipped_n = items.iter().filter(|(_, q)| self.skipped.contains(&q.id) && !self.fixed.contains(&q.id)).count();
                    let pending: Vec<(usize, &Issue)> = items.iter().copied().filter(|(idx, q)| *idx == self.fix_idx || (!self.fixed.contains(&q.id) && !self.skipped.contains(&q.id))).collect();
                    let shown = pending.len().min(4);
                    let more = pending.len() - shown;
                    for (idx, q) in pending.into_iter().take(shown) {
                        let (label, fg, bg) = if self.fixed.contains(&q.id) {
                            ("Done", GOOD, GOOD_BG)
                        } else if idx == self.fix_idx {
                            ("Now", BG, ACCENT)
                        } else if self.skipped.contains(&q.id) {
                            ("Skipped", MUTED, RAISED)
                        } else if analysis::is_optional(q) {
                            ("Optional", MUTED, RAISED)
                        } else {
                            ("To do", TEXT_SOFT, RAISED)
                        };
                        let r = ui.horizontal(|ui| {
                            theme::pill(ui, fg, bg, label);
                            ui.label(RichText::new(q.title.trim_end_matches('.')).color(if idx == self.fix_idx { TEXT } else { TEXT_SOFT }));
                        });
                        if idx != self.fix_idx && r.response.interact(egui::Sense::click()).clicked() {
                            action = Some(format!("jump:{idx}"));
                        }
                    }
                    let mut tail = Vec::new();
                    if more > 0 {
                        tail.push(format!("{more} more after these"));
                    }
                    if done > 0 {
                        tail.push(format!("{done} done"));
                    }
                    if skipped_n > 0 {
                        tail.push(format!("{skipped_n} skipped (still in Your setup)"));
                    }
                    let kept_n = self.st.kept_issues.iter().filter(|k| analysis::place_of(k) == place).count();
                    if kept_n > 0 {
                        tail.push(format!("{kept_n} kept as is (undo in Your setup)"));
                    }
                    if !tail.is_empty() {
                        theme::small(ui, tail.join(" · "));
                    }
                });
                ui.add_space(6.0);
            }
            let Some(issue) = cur.clone() else {
                let msg = if self.returning {
                    "Nothing to fix.".to_string()
                } else if total == 0 {
                    if self.test.is_some() { "Nothing to fix. Nice.".to_string() } else { "Run the test first, so Preflight can find what to fix.".to_string() }
                } else {
                    format!("{} fixed{}. Setup is now {}.", self.fixed.len(), if self.skipped.is_empty() { String::new() } else { format!(", {} skipped", self.skipped.len()) }, self.analysis.setup_score)
                };
                ui.label(RichText::new(msg).size(19.0).strong());
                if !self.fixed.is_empty() {
                    theme::muted(ui, "Click Check my changes to run a short test and see the difference, before and after.");
                }
                if !self.skipped.is_empty() {
                    theme::muted(ui, "Skipped issues stay in Your setup, so you can come back to them.");
                }
                let kept: Vec<&Issue> = self.analysis.issues.iter().filter(|i| self.st.kept_issues.contains(&i.id)).collect();
                if !kept.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("You chose to keep").strong());
                    for k in kept {
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(k.title.trim_end_matches('.'));
                            if ui.small_button("Fix it after all").clicked() {
                                action = Some(format!("unkeep:{}", k.id));
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if !self.fixed.is_empty() && theme::secondary(ui, "Check my changes").clicked() {
                        action = Some("check".into());
                    }
                    if (self.test.is_some() || self.returning) && theme::secondary(ui, "Ready to go live").clicked() {
                        action = Some("to4".into());
                    }
                });
                return;
            };
            // Before the first optional issue: a pause, so new users can stop here.
            if pausing {
                let left = self.fix_queue[self.fix_idx..].len();
                let skipped_imp: Vec<String> = self.fix_queue[..self.fix_idx].iter().filter(|q| self.skipped.contains(&q.id) && !self.fixed.contains(&q.id)).map(|q| q.id.clone()).collect();
                if skipped_imp.is_empty() {
                    ui.label(RichText::new("The important issues are done.").size(19.0).strong());
                } else {
                    ui.label(RichText::new(format!("You've been through the important ones. {} skipped, so Setup stays at {}.", skipped_imp.len(), self.analysis.setup_score)).size(19.0).strong());
                    if theme::secondary(ui, "Go back to the skipped ones").clicked() {
                        action = Some("back_skipped".into());
                    }
                }
                theme::muted(ui, format!("{left} optional improvement{} left. They're small wins; skipping them is fine.", if left == 1 { "" } else { "s" }));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "Go through them").clicked() {
                        action = Some("optional_ok".into());
                    }
                });
                return;
            }
            // One card: where, what to change (now → change to), one reason, two buttons.
            let picked = self.picks.get(&issue.id).cloned().or_else(|| issue.options.iter().find(|o| o.suggested).map(|o| o.id.clone())).or_else(|| issue.options.first().map(|o| o.id.clone()));
            let main = issue.options.iter().find(|o| Some(&o.id) == picked.as_ref()).cloned();
            let total = self.fix_queue.len();
            let place_name = match analysis::place_of(&issue.id) {
                analysis::Place::Game => self.test.as_ref().and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone())).unwrap_or_else(|| "your game".into()),
                analysis::Place::Obs => "OBS".into(),
                analysis::Place::Pc => "your PC".into(),
            };
            ui.horizontal(|ui| {
                tag_pill(ui, if analysis::is_optional(&issue) { analysis::Tag::Optional } else { analysis::Tag::Change });
                ui.label(RichText::new(format!("{} of {total} · In {place_name}", self.fix_idx + 1)).size(13.0).color(MUTED));
            });
            if let Some(where_) = crate::twopc::pc_of(analysis::place_of(&issue.id), self.role()) {
                ui.label(theme::caps(where_, 11.5, theme::TEAL));
            }
            let heading = main.as_ref().map(|o| o.name.clone()).unwrap_or_else(|| issue.title.clone());
            ui.label(RichText::new(heading).size(20.0).strong());
            if let Some(text) = detected {
                egui::Frame::NONE
                    .fill(GOOD_BG)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2F, 0x6B, 0x47)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format!("Spotted it: {text}")).color(GOOD).strong());
                        let note = if cur_measured(&issue.id) { "Check my changes at the end will measure the difference." } else { "Re-checked automatically." };
                        ui.label(RichText::new(note).size(13.0).color(TEXT_SOFT));
                    });
                return;
            }
            if issue.options.is_empty() {
                theme::muted(ui, "You've turned off every suggestion for this one. Restore them in Settings › You chose to keep.");
            }
            if let Some(o) = &main {
                ui.add_space(4.0);
                if o.changes.is_empty() {
                    ui.label(RichText::new(&o.menu).size(13.0).color(MUTED));
                }
                for c in &o.changes {
                    change_row(ui, c);
                }
            }
            // Preflight couldn't measure the game's FPS: ask instead of guessing.
            let game_for_fps = self.test.as_ref().and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()));
            let fps_known = self.test.as_ref().and_then(|t| t.game_fps).is_some() || game_for_fps.as_ref().map(|g| self.st.game_fps_said.contains_key(g)).unwrap_or(false);
            if matches!(issue.id.as_str(), "gpu_maxed" | "gpu_maybe" | "gpu_near") && main.as_ref().map(|o| o.id == "cap").unwrap_or(false) {
                if let Some(g) = &game_for_fps {
                    if !fps_known || self.fps_ask_open {
                        theme::inset(ui, |ui| {
                            ui.label(RichText::new(format!("About what FPS do you get in {g}?")).strong());
                            theme::small(ui, "Preflight couldn't measure it. Your answer makes the suggested limit fit your game.");
                            ui.horizontal_wrapped(|ui| {
                                for v in [45u32, 60, 90, 120, 144, 165, 200, 240] {
                                    if ui.button(if v == 240 { "240+".to_string() } else { format!("About {v}") }).clicked() {
                                        action = Some(format!("fps:{v}"));
                                    }
                                }
                                if ui.button("Not sure").clicked() {
                                    action = Some("fps:0".into());
                                }
                            });
                        });
                    } else if let Some(v) = self.st.game_fps_said.get(g) {
                        ui.horizontal(|ui| {
                            theme::small(ui, format!("Based on your answer: about {v} FPS in {g}."));
                            if link_small(ui, "Change") {
                                action = Some("fps_ask".into());
                            }
                        });
                    } else if let Some(t) = &self.test {
                        if let Some(f) = t.game_fps {
                            theme::small(ui, format!("Measured {f:.0} FPS in {g} during the test ({}).", t.game_fps_source));
                        }
                    }
                }
            }
            // One reason, with the measured number.
            ui.add_space(4.0);
            ui.label(RichText::new(&issue.title).color(TEXT_SOFT));
            if let Some(o) = &main {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("You get:").color(GOOD).strong());
                    ui.label(&o.gain);
                    ui.label(RichText::new("· Trade-off:").color(MUTED));
                    ui.label(RichText::new(&o.cost).color(MUTED));
                });
            }
            let watch = match issue.id.as_str() {
                "gpu_maxed" | "gpu_maybe" | "gpu_near" | "bitrate_effects" | "cpu_high" | "heat" => "Change it in the game and Preflight notices by itself (it reads the settings file).",
                "audio_balance" | "mic_quiet" | "no_game_audio" => "Change it in OBS and Preflight notices by itself.",
                "display_capture" => "Picking a different window inside PipeWire can't be seen, so click \"I changed it\".",
                "net_unsteady" | "hogs" | "disk_space" | "slow_disk" => "Preflight can't see this change. Click \"I changed it\" when you've done it.",
                _ => "Change it, click Apply or OK in OBS, and Preflight notices by itself.",
            };
            ui.label(RichText::new(watch).size(12.5).color(MUTED));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if theme::secondary(ui, "Keep as is").on_hover_text("Stays listed under \"You chose to keep\" and stops counting against your score.").clicked() {
                    action = Some("keep".into());
                }
                if theme::secondary(ui, "Skip for now").clicked() {
                    action = Some("skip".into());
                }
            });
            // Everything else, one click away.
            let others: Vec<_> = issue.options.iter().filter(|o| Some(&o.id) != picked.as_ref()).cloned().collect();
            egui::CollapsingHeader::new(if others.is_empty() { "Why?".to_string() } else { format!("Why? And {} other way{} to fix it", others.len(), if others.len() == 1 { "" } else { "s" }) })
                .id_salt(format!("why-{}", issue.id))
                .default_open(advanced)
                .show(ui, |ui| {
                    ui.label(RichText::new(&issue.why).color(TEXT_SOFT));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Fixing it:").color(MUTED));
                        ui.label(RichText::new(analysis::impact_of(&issue.id)).color(GOOD));
                        let which = if issue.measured && self.test.is_some() { "Smoothness" } else { "Setup" };
                        ui.label(RichText::new(format!("· +{} {which}", analysis::points_of(issue.priority))).color(MUTED));
                        if issue.measured {
                            theme::measured_tag(ui, true);
                        }
                    });
                    theme::muted(ui, &issue.learn);
                    theme::small(ui, &issue.who);
                    glossary_chips(ui, &format!("{} {} {}", issue.title, issue.why, issue.options.iter().map(|o| format!("{} {} {}", o.name, o.gain, o.cost)).collect::<Vec<_>>().join(" ")));
                    if let Some(o) = &main {
                        if link_small(ui, "Don't suggest this again") {
                            action = Some(format!("hide:{}", o.id));
                        }
                    }
                    for o in &others {
                        ui.add_space(4.0);
                        theme::inset(ui, |ui| {
                            ui.label(RichText::new(&o.name).strong());
                            if o.changes.is_empty() {
                                ui.label(RichText::new(&o.menu).size(12.5).color(MUTED));
                            }
                            for c in &o.changes {
                                change_row(ui, c);
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("You get:").color(GOOD));
                                ui.label(&o.gain);
                                ui.label(RichText::new("· Trade-off:").color(MUTED));
                                ui.label(RichText::new(&o.cost).color(MUTED));
                            });
                            ui.horizontal(|ui| {
                                if ui.small_button("Use this one").clicked() {
                                    action = Some(format!("pick:{}", o.id));
                                }
                                if link_small(ui, "Don't suggest this again") {
                                    action = Some(format!("hide:{}", o.id));
                                }
                            });
                        });
                    }
                });
        });
        let advance = |app: &mut Self| {
            app.detected = None;
            app.show_other_fixes = false;
            app.fix_idx += 1;
            // Don't show again what's already fixed or kept (after going back to a skipped one).
            while app.fix_idx < app.fix_queue.len() {
                let id = &app.fix_queue[app.fix_idx].id;
                if app.fixed.contains(id) || app.st.kept_issues.contains(id) {
                    app.fix_idx += 1;
                } else {
                    break;
                }
            }
        };
        if action.is_none() {
            action = self.queued_fix_action.take();
        }
        match action.as_deref() {
            Some("report") => {
                if let Some(i) = &cur {
                    self.open_report(format!("{}: \"{}\"", i.area, i.title));
                }
            }
            Some("to4") => self.set_step(4),
            Some("confirm_ok") => self.confirm = None,
            Some("check") => {
                let ctx = ui.ctx().clone();
                self.check_changes(&ctx);
            }
            Some("optional_ok") => self.optional_ok = true,
            Some("fps_ask") => self.fps_ask_open = true,
            Some(x) if x.starts_with("fps:") => {
                let v: u32 = x[4..].parse().unwrap_or(0);
                if let Some(g) = self.test.as_ref().and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone())) {
                    if v > 0 {
                        self.st.game_fps_said.insert(g, v);
                    } else {
                        self.st.game_fps_said.remove(&g);
                    }
                    self.st.save();
                }
                self.fps_ask_open = false;
                self.recompute();
            }
            Some("back_skipped") => {
                if let Some(idx) = self.fix_queue.iter().position(|q| self.skipped.contains(&q.id) && !self.fixed.contains(&q.id)) {
                    let id = self.fix_queue[idx].id.clone();
                    self.skipped.retain(|k| k != &id);
                    self.fix_idx = idx;
                    self.detected = None;
                    self.show_other_fixes = false;
                }
            }
            Some("toggle_others") => self.show_other_fixes = !self.show_other_fixes,
            Some("skip_optional") => {
                for i in &self.fix_queue[self.fix_idx..] {
                    self.skipped.push(i.id.clone());
                }
                self.fix_idx = self.fix_queue.len();
                self.set_step(4);
            }
            Some("skip") => {
                if let Some(i) = &cur {
                    self.skipped.push(i.id.clone());
                }
                advance(self);
            }
            Some("keep") => {
                if let Some(i) = &cur {
                    if !self.st.kept_issues.contains(&i.id) {
                        self.st.kept_issues.push(i.id.clone());
                        self.st.save();
                    }
                    log::info!("User kept {} as it is", i.id);
                    self.recompute();
                }
                advance(self);
            }
            Some("manual") => {
                if let Some(i) = &cur {
                    self.fixed.push(i.id.clone());
                    log::info!("User marked {} as changed", i.id);
                    let picked = self.picks.get(&i.id).cloned().or_else(|| i.options.iter().find(|o| o.suggested).map(|o| o.id.clone()));
                    // Game changes: watch the settings file to confirm they saved.
                    if analysis::place_of(&i.id) == analysis::Place::Game {
                        let changes: Vec<analysis::Change> = i.options.iter().find(|o| Some(&o.id) == picked.as_ref()).map(|o| o.changes.clone()).unwrap_or_default();
                        let in_game: Vec<analysis::Change> = changes.into_iter().filter(|c| !c.menu.starts_with("Steam ›") && !c.menu.starts_with("Heroic") && !c.menu.starts_with("Lutris") && !c.menu.starts_with("Bottles") && !c.menu.contains("NVIDIA app") && !c.menu.contains("AMD Software") && !c.menu.contains("Intel Graphics")).collect();
                        if let Some(g) = self.target_game().map(|g| g.name.clone()) {
                            if !in_game.is_empty() {
                                let since = self.game_file_saved(&g).unwrap_or(0);
                                self.confirm = Some(ConfirmWatch { game: g, changes: in_game, since, started: ui.ctx().input(|x| x.time), result: None });
                                self.poll_next = 0.0;
                            }
                        }
                    }
                    let what = i.options.iter().find(|o| Some(&o.id) == picked.as_ref()).map(|o| lower_first(o.name.trim_end_matches('.'))).unwrap_or_else(|| "a setting".into());
                    self.note_change(format!("You changed: {what} ({})", analysis::impact_of(&i.id).to_lowercase()));
                }
                advance(self);
            }
            Some(j) if j.starts_with("jump:") => {
                if let Ok(n) = j[5..].parse::<usize>() {
                    self.fix_idx = n;
                    self.detected = None;
                    self.show_other_fixes = false;
                }
            }
            Some(j) if j.starts_with("pick:") => {
                if let Some(i) = &cur {
                    self.picks.insert(i.id.clone(), j[5..].to_string());
                    self.show_other_fixes = false;
                }
            }
            Some(j) if j.starts_with("unkeep:") => {
                let id = j[7..].to_string();
                self.st.kept_issues.retain(|k| k != &id);
                self.st.save();
                self.recompute();
                self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
                self.fix_idx = self.fix_queue.iter().position(|i| i.id == id).unwrap_or(0);
            }
            Some(h) if h.starts_with("hide:") => {
                if let Some(i) = &cur {
                    let oid = &h[5..];
                    let label = i.options.iter().find(|o| o.id == oid).map(|o| o.name.clone()).unwrap_or_default();
                    self.st.hidden_options.push(HiddenOption { key: format!("{}:{}", i.id, oid), label });
                    self.st.save();
                    self.recompute();
                    if let Some(q) = self.fix_queue.get_mut(self.fix_idx) {
                        let st = &self.st;
                        let had = q.options.iter().any(|o| o.suggested);
                        q.options.retain(|o| !st.is_hidden(&q.id, &o.id));
                        if had && !q.options.iter().any(|o| o.suggested) {
                            if let Some(o) = q.options.first_mut() {
                                o.suggested = true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn ui_step4(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        // Problems Preflight found: (key, status, title, detail). Each can be
        // fixed in OBS (re-checked automatically) or marked "Keep as is".
        let mut problems: Vec<(&'static str, Status, String, String)> = Vec::new();
        let mut passed: Vec<String> = Vec::new();
        let o = self.obs.clone().filter(|o| o.connected);
        match &o {
            None => problems.push(("obs", Status::Unknown, "OBS isn't connected".into(), "Open OBS so Preflight can check your mic, audio and scene.".into())),
            Some(o) => {
                let mics = o.mics();
                if mics.is_empty() {
                    problems.push(("mic", Status::Bad, "There's no microphone in OBS".into(), "Sources › + › Audio Input Capture".into()));
                } else if mics.iter().any(|m| m.muted == Some(true)) {
                    problems.push(("mic", Status::Bad, "Your mic is muted in OBS".into(), "Unmute it in the Audio Mixer".into()));
                } else {
                    passed.push("Mic on".into());
                }
                if o.desktop_audio().is_empty() {
                    problems.push(("audio", Status::Warn, "There's no game audio source".into(), "Viewers may not hear the game".into()));
                } else {
                    passed.push("Game audio".into());
                }
                if o.uses_display_capture() && !o.uses_game_capture() {
                    problems.push(("capture", Status::Warn, "Your capture shows your whole screen".into(), "Viewers may see Discord and notifications".into()));
                } else if o.uses_game_capture() {
                    passed.push("Game capture".into());
                } else if o.uses_portal_capture() {
                    problems.push(("capture", Status::Unknown, "Check your PipeWire capture shows only the game".into(), "It can be a window or a whole screen; OBS doesn't say which".into()));
                }
                match o.webcam() {
                    Some(w) if w.enabled => passed.push("Webcam visible".into()),
                    Some(_) => problems.push(("cam", Status::Warn, "Your webcam is hidden in the scene".into(), "Fine if you're streaming without a camera".into())),
                    None => {}
                }
                if o.streaming_active || self.live.is_some() {
                    passed.push("Streaming".into());
                }
            }
        }
        if !self.other_recorders.is_empty() {
            problems.push(("rec", Status::Warn, format!("Other recorders are running: {}", self.other_recorders.join(", ")), "They compete with OBS for the encoder".into()));
        }
        let open: Vec<_> = problems.iter().filter(|p| !self.st.go_kept.iter().any(|k| k == p.0)).cloned().collect();
        let mut manual: Vec<(&'static str, String, bool)> = vec![
            ("dnd", if cfg!(windows) { "Do Not Disturb is on (Windows Settings › System › Notifications)".into() } else { "Do Not Disturb is on (system tray › Notifications)".into() }, self.tick_dnd),
            ("title", "Stream title and category are set".into(), self.tick_title),
        ];
        if self.st.bandwidth_test_pending {
            manual.push(("bw", "I removed ?bandwidthtest=true from my stream key".into(), false));
        }
        let left = open.len() + manual.iter().filter(|m| !m.2).count();
        let ready = left == 0;
        self.go_ready = ready;
        // Important fixes not made yet (skipped or never reached). Always shown, never blocking.
        let skipped_important = self.analysis.issues.iter().filter(|i| !analysis::is_optional(i) && !self.st.kept_issues.contains(&i.id) && !self.fixed.contains(&i.id)).count();
        let scene = o.as_ref().map(|o| o.current_scene.clone()).unwrap_or_default();
        let mut action: Option<(&'static str, &'static str)> = None;

        theme::hero_card(ui, |ui| {
            ui.horizontal(|ui| {
                let live = self.live.is_some();
                ui.vertical(|ui| {
                    if live {
                        ui.label(RichText::new("YOU'RE LIVE").size(13.0).strong().color(BAD));
                    } else if self.go_ready {
                        ui.label(RichText::new("YOU'RE READY").size(13.0).strong().color(GOOD));
                    } else {
                        do_next_label(ui);
                    }
                    ui.label(RichText::new(if live { "Enjoy the stream" } else if self.go_ready { "Everything's checked" } else { "A last look before you stream" }).size(24.0).strong());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if live {
                    } else if ready {
                        theme::status_pill(ui, Status::Ok, "Ready");
                    } else {
                        theme::status_pill(ui, Status::Warn, &format!("{left} left"));
                    }
                });
            });
            ui.add_space(6.0);
            // Status first.
            let (fill, stroke) = if ready || self.live.is_some() { (GOOD_BG, Color32::from_rgb(0x2F, 0x6B, 0x47)) } else { (CARD_DARK, BORDER) };
            egui::Frame::NONE.fill(fill).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(l) = &self.live {
                    ui.label(RichText::new(format!("You're live · {}", crate::streamwatch::fmt_t(l.elapsed))).size(20.0).strong().color(GOOD));
                    ui.label(RichText::new(format!("{:.2}% of frames dropped so far. Preflight watches quietly from the tray and writes your post-flight summary when you end the stream.", l.dropped_pct)).color(TEXT_SOFT));
                } else if ready {
                    ui.label(RichText::new("Ready. Start streaming in OBS.").size(20.0).strong());
                    let start = if scene.is_empty() { String::new() } else { format!(" You'll start on \"{scene}\".") };
                    ui.label(RichText::new(format!("{}{start} Preflight watches from the tray while you stream and writes a post-flight summary when you end it.", self.analysis.verdict)).color(TEXT_SOFT));
                } else {
                    ui.label(RichText::new(format!("{left} thing{} left before you go live", if left == 1 { "" } else { "s" })).size(20.0).strong());
                    ui.label(RichText::new("Fix them in OBS (Preflight re-checks by itself) or choose Keep as is, then tick the rest.").color(TEXT_SOFT));
                }
            });
            ui.add_space(8.0);
            // Problems
            for (key, st, title, detail) in open.iter().filter(|_| self.live.is_none()) {
                go_row(ui, *st, match st { Status::Bad => "Change", Status::Unknown => "Unknown", _ => "Check" }, title, detail, Some("Keep as is"), || action = Some(("ack", key)));
            }
            // Things to tick yourself (hidden once you're live)
            for (key, title, done) in manual.iter().filter(|_| self.live.is_none()) {
                if go_tick_row(ui, *done, title) {
                    action = Some(("tick", key));
                }
            }
            if skipped_important > 0 && self.live.is_none() {
                go_row(ui, Status::Warn, "Change", &format!("{skipped_important} change{} from Fix not made yet", if skipped_important == 1 { "" } else { "s" }), "You can still go live; making them keeps it smoother", Some("Go to Fix"), || action = Some(("review", "")));
            }
            // What Preflight already confirmed, in one line.
            let short = |k: &str| match k {
                "obs" => "OBS check",
                "mic" => "Mic",
                "audio" => "Game audio",
                "capture" => "Screen capture",
                "cam" => "Webcam",
                _ => "Other recorders",
            };
            let acked: Vec<String> = problems.iter().filter(|p| self.st.go_kept.iter().any(|k| k == p.0)).map(|p| format!("{}: kept as is", short(p.0))).collect();
            if !passed.is_empty() || !scene.is_empty() || !acked.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Checked by Preflight:").size(13.0).color(MUTED));
                    for p in &passed {
                        theme::pill(ui, GOOD, GOOD_BG, &format!("OK  {p}"));
                    }
                    if !scene.is_empty() {
                        theme::pill(ui, TEXT_SOFT, RAISED, &format!("Starting scene: {scene}"));
                    }
                    for a in &acked {
                        theme::pill(ui, MUTED, RAISED, a);
                    }
                });
            }
        });
        match action {
            Some(("ack", k)) => {
                if !self.st.go_kept.iter().any(|x| x == k) {
                    self.st.go_kept.push(k.to_string());
                    self.st.save();
                }
            }
            Some(("tick", "dnd")) => self.tick_dnd = !self.tick_dnd,
            Some(("tick", "title")) => self.tick_title = !self.tick_title,
            Some(("tick", "bw")) => {
                self.st.bandwidth_test_pending = false;
                self.st.save();
            }
            Some(("review", _)) => {
                if let Some(pos) = self.fix_queue.iter().position(|i| self.skipped.contains(&i.id) && !analysis::is_optional(i)) {
                    self.fix_idx = pos;
                    self.skipped.retain(|id| id != &self.fix_queue[pos].id);
                    self.detected = None;
                } else if self.fix_idx >= self.fix_queue.len() {
                    self.fix_queue.clear();
                }
                self.set_step(3);
                let first_open = self.fix_queue.iter().position(|i| !self.fixed.contains(&i.id));
                if self.fix_idx >= self.fix_queue.len() || self.fix_queue.get(self.fix_idx).map(|i| self.fixed.contains(&i.id)).unwrap_or(false) {
                    if let Some(p) = first_open {
                        self.fix_idx = p;
                    }
                }
            }
            _ => {}
        }
        if ready && !self.preflight_done {
            self.preflight_done = true;
            self.st.last_preflight = Some(LastPreflight {
                time: crate::util::unix_now(),
                signature: self.signature(),
                score: self.analysis.score,
                game: self.target_game().map(|g| g.name.clone()),
            });
            self.st.save();
            log::info!("Pre-flight complete: Smoothness {} · Setup {} · {}", self.analysis.smoothness.map(|v| v.to_string()).unwrap_or_else(|| "not tested".into()), self.analysis.setup_score, self.analysis.verdict);
            if self.st.remind_delete_recording && !self.st.pending_recordings.is_empty() {
                self.show_recording_popup = true;
            }
        } else if !ready {
            self.preflight_done = false;
        }
    }

    // -----------------------------------------------------------------------
    // Results

    fn ui_results(&mut self, ui: &mut Ui) {
        let a = self.analysis.clone();
        let mut report: Option<String> = None;
        let go_pf = false;
        let mut act: Option<String> = None;
        let mut rescan = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your setup").size(26.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.scan.is_some() {
                    ui.spinner();
                } else if ui.add(egui::Button::new(RichText::new("Read again").color(ACCENT)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    rescan = true;
                }
                if let Some(t) = self.last_scan {
                    ui.label(RichText::new(format!("Read {}", history::fmt_time(t))).color(MUTED));
                }
            });
        });
        ui.add_space(6.0);
        let all = a.setup.obs.iter().chain(a.setup.game.iter()).chain(a.setup.pc.iter());
        let changes = all.clone().filter(|r| r.tag == analysis::Tag::Change).count();
        let good = all.filter(|r| r.tag == analysis::Tag::Keep).count();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(changes.to_string()).strong());
            ui.label(RichText::new(if changes == 1 { "thing to change ·" } else { "things to change ·" }).color(TEXT_SOFT));
            ui.label(RichText::new(good.to_string()).strong());
            ui.label(RichText::new("look good ·").color(TEXT_SOFT));
            ui.label(RichText::new("\"Fix this\" shows you how. Preflight never changes your settings.").color(MUTED));
        });
        if let Some(c) = &self.compare {
            let sk = |r: &TestResult| if r.output_total > 0 { r.output_skipped as f32 * 100.0 / r.output_total as f32 } else { 0.0 };
            ui.label(
                RichText::new(format!(
                    "Checked after your changes: Smoothness {} to {} · GPU maxed out {:.0}% to {:.0}% of the test · skipped frames {:.2}% to {:.2}%",
                    c.before_score,
                    c.after_score,
                    maxed_pct(&c.before),
                    maxed_pct(&c.after),
                    sk(&c.before),
                    sk(&c.after)
                ))
                .color(GOOD),
            );
        }
        if self.test.is_none() {
            theme::muted(ui, "Based on your settings so far. Run the test while you play to measure it.");
        }
        ui.horizontal(|ui| {
            if !self.fixed.is_empty() && self.test.is_some() && link_small(ui, "Check my changes") {
                act = Some("check".into());
            }
            if link_small(ui, "Copy as text (no names, keys or scenes)") {
                ui.ctx().copy_text(analysis::setup_text(&a));
                act = Some("copied".into());
            }
        });
        if rescan {
            self.start_scan(ui.ctx().clone(), true);
        }
        ui.add_space(10.0);
        self.ui_two_pc_section(ui);
        setup_table(ui, "In OBS", &a.setup.obs, None, &self.fixed, &mut act);
        if let Some(g) = &a.setup.game_name {
            // Settings file status first, then any note, then "a report helps".
            let mut lines: Vec<String> = Vec::new();
            if let Some(f) = &a.setup.game_file {
                lines.push(f.clone());
            }
            if let Some(c) = &a.setup.game_coverage {
                lines.push(c.clone());
            }
            if let Some(n) = &a.setup.game_note {
                lines.push(n.clone());
            }
            if a.setup.report_helps {
                lines.push("Preflight doesn't fully understand this game's settings yet. A bug report (Help › Report a problem) includes the settings it found and helps fix it.".into());
            }
            let note = if lines.is_empty() { None } else { Some(lines.join("\n")) };
            setup_table(ui, &format!("In your game · {g}"), &a.setup.game, note.as_deref(), &self.fixed, &mut act);
        }
        if !a.setup.pc.is_empty() {
            setup_table(ui, "On your PC", &a.setup.pc, None, &self.fixed, &mut act);
        }
        // One setup for all your games.
        if !self.fleet.tested.is_empty() {
            let f = self.fleet.clone();
            let sub = format!("{} tested game{}", f.tested.len(), if f.tested.len() == 1 { "" } else { "s" });
            setup_section(ui, "One setup for all your games", &sub, if f.warning.is_some() { WARN } else { MUTED }, f.warning.is_some(), |ui| {
                let names: Vec<String> = f.tested.iter().map(|l| l.game.clone()).collect();
                ui.label(format!("Tested with {} game{}: {}.", names.len(), if names.len() == 1 { "" } else { "s" }, names.join(", ")));
                if !f.setup.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("OBS setup that suits all of them:").color(MUTED));
                        ui.label(RichText::new(&f.setup).strong());
                        if let Some(h) = &f.heaviest {
                            ui.label(RichText::new(format!("(the limit is set by {h})")).color(MUTED));
                        }
                    });
                }
                if let Some(w) = &f.warning {
                    ui.label(RichText::new(format!("{w} Keep one setup and change the game instead of switching OBS settings per game.")).color(WARN));
                }
                for l in &f.tested {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&l.game).strong());
                        ui.label(RichText::new(format!("GPU {}{}", l.gpu_avg.map(|g| format!("{g:.0}% avg")).unwrap_or_else(|| "not read".into()), if l.maxed_pct >= 1.0 { format!(", maxed {:.0}% of the time", l.maxed_pct) } else { String::new() }) + &if l.skipped_pct > 0.1 { format!(", {:.1}% frames skipped", l.skipped_pct) } else { String::new() }).color(MUTED));
                        ui.label(RichText::new(&l.verdict).color(if l.needs_room { WARN } else { GOOD }));
                    });
                }
                if !f.untested.is_empty() {
                    theme::small(ui, format!("Not checked yet: {}. Test them once and Preflight includes them.", f.untested.join(", ")));
                }
            });
        }
        // Every video encoder on the PC, what it can do, and who's using it.
        if !self.encoders.is_empty() {
            let busy: usize = self.encoders.iter().filter(|e| e.in_use()).count();
            let sub = if busy == 0 { "None in use right now".to_string() } else { format!("{busy} in use") };
            setup_section(ui, "Video encoders", &sub, if busy > 0 { ACCENT } else { MUTED }, false, |ui| {
                egui::Grid::new("encoders").num_columns(3).spacing([18.0, 6.0]).striped(false).show(ui, |ui| {
                    for e in &self.encoders {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.kind).strong());
                            theme::small(ui, &e.gpu);
                        });
                        ui.vertical(|ui| {
                            ui.label(format!("{} chip{} · {}", e.chips.unwrap_or(1), if e.chips.unwrap_or(1) == 1 { "" } else { "s" }, e.codecs.join(", ")));
                        });
                        ui.label(RichText::new(e.use_text()).color(if e.in_use() { ACCENT } else { MUTED }));
                        ui.end_row();
                    }
                });
                if let Some(t) = self.test.as_ref().filter(|t| !t.encodes_seen.is_empty()) {
                    theme::small(ui, format!("During your last test: {}", t.encodes_seen.iter().map(|s| format!("{} {} {}", s.app, s.codec, s.res).trim().to_string()).collect::<Vec<_>>().join(", ")));
                }
            });
        }
        theme::inset(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("What viewers will see:").color(MUTED));
                ui.label(RichText::new(&a.current_output).strong());
                if a.recommended_output != a.current_output && a.recommended_output != "—" {
                    ui.label(RichText::new(format!("(suggested: {})", a.recommended_output)).color(ACCENT));
                }
                if self.obs.as_ref().and_then(|o| o.multitrack) == Some(true) {
                    ui.label(RichText::new("Enhanced Broadcasting picks the versions and bitrates.").color(MUTED));
                } else {
                    ui.label(RichText::new(format!("{} allows up to {} kbps for 1080p60.", self.st.platform.name(), crate::network::limits(self.st.platform).max_kbps_1080p60)).color(MUTED));
                }
            });
        });
        // What the scan found: hardware, OBS connection, games, internet.
        let items = self.scan_items();
        let warn_n = items.iter().filter(|i| matches!(i.0, Status::Warn | Status::Bad)).count();
        let mut scan_act: Option<NextAct> = None;
        setup_section(ui, "PC and connections", &if warn_n == 0 { "All fine".to_string() } else { format!("{warn_n} to check") }, if warn_n == 0 { GOOD } else { WARN }, warn_n > 0, |ui| {
            for (st, label, name, detail) in &items {
                ui.add(egui::Separator::default().spacing(10.0));
                ui.horizontal_wrapped(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new(170.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| { ui.set_width(170.0); ui.label(RichText::new(*name).color(MUTED)); });
                    ui.label(RichText::new(label).color(theme::status_colors(*st).0));
                    if *name == "OBS" {
                        if let Some((l, a)) = err_action(detail) {
                            if outlined(ui, l).clicked() {
                                scan_act = Some(a);
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if link_small(ui, "This looks wrong") {
                            report = Some(format!("Scan result \"{name}\": {detail}"));
                        }
                    });
                });
                ui.label(RichText::new(detail).size(13.0).color(TEXT_SOFT));
            }
        });
        if let Some(a) = scan_act {
            self.do_act(&ui.ctx().clone(), a);
        }
        match act.as_deref() {
            Some("copied") => self.toast(&ui.ctx().clone(), "Copied. Paste it into Discord or anywhere.".to_string()),
            Some("fix") => self.do_act(&ui.ctx().clone(), NextAct::Step(3)),
            Some("check") => {
                let ctx = ui.ctx().clone();
                self.check_changes(&ctx);
                return;
            }
            Some(f) if f.starts_with("fix:") => {
                let id = f[4..].to_string();
                self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
                self.fix_idx = self.fix_queue.iter().position(|i| i.id == id).unwrap_or(0);
                self.optional_ok = true;
                self.detected = None;
                self.go(Screen::Preflight);
                self.step = 3;
                return;
            }
            Some(f) if f.starts_with("unkeep:") => {
                let id = f[7..].to_string();
                self.st.kept_issues.retain(|k| k != &id);
                self.st.save();
                self.recompute();
            }
            _ => {}
        }
        ui.add_space(12.0);
        self.ui_viewer_preview(ui);
        ui.add_space(12.0);
        egui::CollapsingHeader::new(RichText::new("Details: every check and the test measurements").strong()).id_salt("setup-details").show(ui, |ui| {
        for row in a.checks.chunks(2) {
            ui.columns(2, |c| {
                for (i, ch) in row.iter().enumerate() {
                    theme::card(&mut c[i], |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&ch.name).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| theme::status_pill(ui, ch.status, &ch.label));
                        });
                        ui.label(RichText::new(&ch.metric).size(16.0).strong());
                        theme::muted(ui, &ch.note);
                        ui.horizontal(|ui| {
                            if !ch.measured && ch.name == "Stream quality settings" {
                                theme::read_tag(ui, "OBS");
                            } else {
                                theme::measured_tag(ui, ch.measured);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if link_small(ui, "This looks wrong") {
                                    report = Some(format!("{}: \"{}\"", ch.name, ch.metric));
                                }
                            });
                        });
                    });
                }
            });
            ui.add_space(8.0);
        }
        if let Some(t) = self.test.clone() {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Test measurements · {}", history::fmt_time(t.started_at))).size(21.0).strong());
            let cpu: Vec<f32> = t.samples.iter().map(|s| s.cpu).collect();
            let gpu: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
            if !gpu.is_empty() {
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("GPU usage").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("avg {:.0}% · peak {:.0}%", t.gpu_avg.unwrap_or(0.0), t.gpu_max.unwrap_or(0.0))).monospace().color(TEXT_SOFT));
                        });
                    });
                    theme::spark_bars(ui, &gpu, 100.0, if t.gpu_avg.unwrap_or(0.0) >= 92.0 { BAD } else { INFO }, 60.0);
                });
                ui.add_space(6.0);
            }
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CPU usage").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("avg {:.0}% · peak {:.0}%", t.cpu_avg, t.cpu_max)).monospace().color(TEXT_SOFT));
                    });
                });
                theme::spark_bars(ui, &cpu, 100.0, ACCENT, 60.0);
            });
            ui.add_space(6.0);
            theme::card(ui, |ui| {
                egui::Grid::new("test-stats").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                    let row = |ui: &mut Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).color(MUTED));
                        ui.label(v);
                        ui.end_row();
                    };
                    row(ui, "GPU temperature (peak)", t.gpu_temp_max.map(|x| format!("{x:.0}°C")).unwrap_or_else(|| "—".into()));
                    row(ui, "CPU temperature (peak)", t.cpu_temp_max.map(|x| format!("{x:.0}°C")).unwrap_or_else(|| "not readable here".into()));
                    if t.with_obs && t.output_total > 0 {
                        let f = t.output_skipped as f32 / t.output_total as f32;
                        row(ui, "Stream frames skipped", if f > 0.0 { format!("{:.2}% (about 1 in every {})", f * 100.0, (1.0 / f).round() as u32) } else { "none".into() });
                        row(ui, "Mic level (speaking)", t.mic_db.map(|x| format!("{x:.0} dB")).unwrap_or_else(|| "not enough data".into()));
                        row(ui, "Game audio level", t.desktop_db.map(|x| format!("{x:.0} dB")).unwrap_or_else(|| "not enough data".into()));
                    }
                    if let Some(e) = t.enc_avg {
                        row(ui, "Encoder chip busy", format!("{e:.0}% average, {:.0}% peak", t.enc_max.unwrap_or(e)));
                    }
                    for o in &t.outputs {
                        let what = if o.label == "Stream" { "network" } else if o.label == "Recording" { "disk" } else { "output" };
                        row(ui, &format!("{} frames dropped ({what})", o.label), if o.dropped == 0 { "none".into() } else { format!("{} of {} ({:.2}%)", o.dropped, o.total, o.dropped as f32 * 100.0 / o.total.max(1) as f32) });
                    }
                    row(ui, "Background programs", if t.hogs.is_empty() { "none flagged".into() } else { t.hogs.iter().map(|h| h.label.clone()).collect::<Vec<_>>().join(", ") });
                    row(
                        ui,
                        "Preflight's own use",
                        match (t.own_cpu_avg, t.own_ram_mb) {
                            (Some(c), Some(m)) => format!("{c:.1}% CPU, {m:.0} MB RAM during the test"),
                            _ => "not measured".into(),
                        },
                    );
                });
            });
        }
        });
        if let Some(r) = report {
            self.open_report(r);
        }
        if go_pf {
            self.go(Screen::Preflight);
            if self.test.is_some() {
                self.set_step(3);
            }
        }
    }

    // -----------------------------------------------------------------------
    // History

    fn ui_history(&mut self, ui: &mut Ui) {
        theme::title(ui, "History");
        theme::subtitle(ui, "What you changed, and what it did.");
        ui.add_space(8.0);
        // Timeline: tests, changes and streams, newest first.
        enum Ev {
            Test(history::Entry, Option<u8>),
            Change(crate::config::ChangeNote),
            Stream(i64, String),
        }
        let tests: Vec<&history::Entry> = self.history.entries.iter().filter(|e| e.kind == "test").collect();
        let mut evs: Vec<(i64, Ev)> = Vec::new();
        for (i, t) in tests.iter().enumerate() {
            let prev = if i > 0 { Some(tests[i - 1].score) } else { None };
            evs.push((t.time, Ev::Test((*t).clone(), prev)));
        }
        for c in &self.st.change_log {
            evs.push((c.time, Ev::Change(c.clone())));
        }
        for st in &self.streams {
            let p = crate::streamwatch::summarize(st);
            evs.push((st.started_at, Ev::Stream(st.started_at, format!("{} · {}{}", crate::streamwatch::fmt_duration(p.duration), p.grade, if st.bandwidth_test { " (test stream)" } else { "" }))));
        }
        evs.sort_by_key(|e| -e.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("What changed").size(18.0).strong());
            if evs.is_empty() {
                theme::muted(ui, "Nothing yet. Run a test, make a fix, and test again: each step shows up here with what it did.");
            }
            for (_, e) in evs.iter().take(40) {
                ui.horizontal_wrapped(|ui| match e {
                    Ev::Test(t, prev) => {
                        ui.label(RichText::new(history::fmt_time(t.time)).monospace().color(MUTED));
                        theme::pill(ui, INFO, RAISED, "Test");
                        let delta = prev.map(|p| {
                            let d = t.score as i32 - p as i32;
                            if d == 0 { String::new() } else { format!(" ({d:+})") }
                        }).unwrap_or_default();
                        let scores = match (t.smooth, t.setup) {
                            (Some(sm), Some(se)) => format!("Smoothness {sm} · Setup {se}"),
                            _ => format!("score {}{delta}", t.score),
                        };
                        ui.label(RichText::new(format!("{}{scores}", t.game.as_ref().map(|g| format!("{g}: ")).unwrap_or_default())).strong());
                        let mut bits = Vec::new();
                        if let Some(m) = t.maxed_pct {
                            bits.push(format!("GPU maxed {m:.0}% of the time"));
                        } else if let Some(g) = t.gpu_avg {
                            bits.push(format!("GPU {g:.0}%"));
                        }
                        if let Some(sk) = t.skipped_pct {
                            bits.push(format!("skipped frames {sk:.2}%"));
                        }
                        ui.label(RichText::new(bits.join(" · ")).color(TEXT_SOFT));
                    }
                    Ev::Change(c) => {
                        ui.label(RichText::new(history::fmt_time(c.time)).monospace().color(MUTED));
                        tag_pill(ui, analysis::Tag::Change);
                        ui.label(&c.text);
                    }
                    Ev::Stream(t, text) => {
                        ui.label(RichText::new(history::fmt_time(*t)).monospace().color(MUTED));
                        theme::pill(ui, GOOD, GOOD_BG, "Stream");
                        ui.label(text);
                    }
                });
            }
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("More history: readiness chart, upload speed by hour, original game settings, saved setups").strong()).id_salt("more-history").show(ui, |ui| {
        let scores: Vec<f32> = self.history.entries.iter().filter(|e| e.score > 0).map(|e| e.score as f32).collect();
        theme::card(ui, |ui| {
            ui.label(RichText::new("Readiness over time").strong());
            if scores.is_empty() {
                theme::muted(ui, "No scans yet.");
            } else {
                theme::spark_bars(ui, &scores, 100.0, ACCENT, 80.0);
                theme::small(ui, format!("{} scans and tests", scores.len()));
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Your upload speed by time of day").strong());
            let hours = self.history.upload_by_hour();
            if hours.is_empty() {
                theme::muted(ui, "Scan at different times to see patterns, like your internet slowing down in the evening.");
            } else {
                egui::Grid::new("hours").num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Hour").color(MUTED));
                    ui.label(RichText::new("Slowest upload (average)").color(MUTED));
                    ui.label(RichText::new("Scans").color(MUTED));
                    ui.end_row();
                    for (h, v, n) in hours {
                        ui.label(format!("{h:02}:00"));
                        ui.label(format!("{v:.1} Mbps"));
                        ui.label(n.to_string());
                        ui.end_row();
                    }
                });
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Original game settings").strong());
            theme::muted(ui, "What each flagged setting was the first time Preflight saw it, so you can go back.");
            if self.history.originals.is_empty() {
                theme::muted(ui, "Nothing recorded yet.");
            }
            for (game, m) in &self.history.originals {
                ui.label(RichText::new(game).strong());
                egui::Grid::new(format!("orig-{game}")).num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                    for (k, o) in m {
                        ui.label(k);
                        ui.label(format!("was {}", o.first_value));
                        ui.label(RichText::new(format!("now {}", o.last_value)).color(if o.first_value != o.last_value { ACCENT } else { MUTED }));
                        ui.end_row();
                    }
                });
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Saved setups").strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.setup_name).hint_text("e.g. Ranked CS2").desired_width(220.0));
                if theme::secondary(ui, "Save current setup").clicked() && !self.setup_name.trim().is_empty() {
                    let s = history::SavedSetup {
                        name: self.setup_name.trim().to_string(),
                        time: crate::util::unix_now(),
                        score: self.analysis.score,
                        style: self.st.style_name().into(),
                        output: self.analysis.current_output.clone(),
                        encoder: self.obs.as_ref().map(|o| o.encoder_kind.clone()).unwrap_or_default(),
                        notes: self.test.as_ref().map(|t| format!("GPU avg {:?}, CPU avg {:.0}%", t.gpu_avg.map(|g| g.round()), t.cpu_avg)).unwrap_or_default(),
                    };
                    self.history.save_setup(s);
                    self.setup_name.clear();
                }
            });
            egui::Grid::new("setups").num_columns(4).spacing([20.0, 4.0]).show(ui, |ui| {
                for s in self.history.setups.iter().rev() {
                    ui.label(RichText::new(&s.name).strong());
                    ui.label(history::fmt_time(s.time));
                    ui.label(RichText::new(format!("{}", s.score)).color(theme::score_color(s.score)));
                    ui.label(format!("{} · {}", s.output, s.encoder));
                    ui.end_row();
                }
            });
        });
        });
    }

    // -----------------------------------------------------------------------
    // Settings: 3 tabs

    fn ui_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::title(ui, "Settings");
        theme::subtitle(ui, "The everyday settings first. Everything else is under Advanced.");
        ui.add_space(8.0);
        let mut save = false;
        let mut recompute = false;
        let mut act: Option<NextAct> = None;
            theme::card(ui, |ui| {
                ui.label(RichText::new("Style for new games").size(16.0).strong());
                theme::small(ui, "Used the first time Preflight sees a game. To change a game's style, click the style under its name in the sidebar.");
                ui.horizontal_wrapped(|ui| {
                    for st in [Style::Competitive, Style::Balanced, Style::Cinematic] {
                        if ui.radio(self.st.default_style == st, style_label(st)).clicked() {
                            self.st.default_style = st;
                            save = true;
                        }
                    }
                });
                ui.add_space(6.0);
                egui::CollapsingHeader::new("Fine-tune").id_salt("st-fine").show(ui, |ui| {
                    if self.questions(ui) {
                        save = true;
                    }
                });
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(RichText::new("Monitor you play on").size(16.0).strong());
                if self.monitor_picker(ui) {
                    save = true;
                }
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("System tray").size(16.0).strong());
                    if self.tray.is_none() {
                        ui.label(RichText::new("No tray found on this desktop").color(MUTED));
                    }
                });
                save |= ui.checkbox(&mut self.st.minimize_to_tray, "Minimize to the tray").changed();
                ui.horizontal(|ui| {
                    ui.label("Close button:");
                    if ui.radio(!self.st.close_to_tray, "Quits Preflight").clicked() {
                        self.st.close_to_tray = false;
                        save = true;
                    }
                    if ui.radio(self.st.close_to_tray, "Goes to the tray").clicked() {
                        self.st.close_to_tray = true;
                        save = true;
                    }
                });
                save |= ui.checkbox(&mut self.st.tray_notifications, "Notifications when a test finishes, a stream ends, or the test flag may still be on").changed();
                if !self.can_hide {
                    theme::small(ui, "On Wayland the window minimizes to the taskbar instead of hiding. The tray icon still shows what Preflight is doing.");
                }
                theme::small(ui, "Tray colours: grey idle · green watching OBS · amber testing · red live · white dot when Preflight needs you.");
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(RichText::new("Where do you stream?").size(16.0).strong());
                theme::small(ui, "Tick every platform you stream to at the same time.");
                ui.horizontal(|ui| {
                    for p in [Platform::Twitch, Platform::YouTube, Platform::Kick] {
                        let mut on = if self.st.platforms.is_empty() { self.st.platform == p } else { self.st.platforms.iter().any(|x| x == p.name()) };
                        if ui.checkbox(&mut on, p.name()).changed() {
                            let mut list: Vec<String> = if self.st.platforms.is_empty() { vec![self.st.platform.name().to_string()] } else { self.st.platforms.clone() };
                            list.retain(|x| x != p.name());
                            if on {
                                list.push(p.name().to_string());
                            }
                            if list.is_empty() {
                                list.push(p.name().to_string());
                            }
                            // The main stream follows the strictest platform's limits.
                            self.st.platform = [Platform::Twitch, Platform::Kick, Platform::YouTube].into_iter().find(|q| list.iter().any(|x| x == q.name())).unwrap_or(Platform::Twitch);
                            self.st.platforms = if list.len() > 1 { list } else { Vec::new() };
                            save = true;
                        }
                    }
                });
                if self.st.platforms.len() > 1 {
                    ui.add_space(4.0);
                    ui.label(RichText::new("How do you reach them?").strong());
                    let plugin_seen = self.obs.as_ref().map(|o| o.outputs.iter().any(|x| x.plugin && x.label.starts_with("Extra stream"))).unwrap_or(false);
                    ui.horizontal_wrapped(|ui| {
                        if ui.radio(self.st.multistream_how == "plugin", "An OBS plugin (a separate output for each platform)").clicked() {
                            self.st.multistream_how = "plugin".into();
                            save = true;
                        }
                        if ui.radio(self.st.multistream_how == "service", "A restream service (Restream or similar: one upload)").clicked() {
                            self.st.multistream_how = "service".into();
                            save = true;
                        }
                    });
                    if plugin_seen && self.st.multistream_how != "plugin" {
                        theme::small(ui, "OBS shows extra stream outputs from a plugin, so you probably use a plugin.");
                    }
                    theme::small(ui, format!("Your main stream follows {}'s limits; the advice covers each platform.", self.st.platform.name()));
                } else {
                    theme::small(ui, crate::network::limits(self.st.platform).note);
                }
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("OBS connection").size(16.0).strong());
                    match &self.obs {
                        Some(o) if o.connected => theme::status_pill(ui, Status::Ok, &format!("Connected · OBS {}", o.obs_version)),
                        _ => theme::status_pill(ui, Status::Warn, "Not connected"),
                    }
                });
                if let Some(o) = self.obs.as_ref().filter(|o| o.connected) {
                    theme::muted(
                        ui,
                        format!(
                            "Streaming {} at {} fps, {}, using {}.",
                            o.output_res.map(|r| format!("{}p", r.1)).unwrap_or_default(),
                            o.fps.map(|f| (f.round() as u32).to_string()).unwrap_or_default(),
                            o.bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default(),
                            o.encoder_kind
                        ),
                    );
                } else if let Some(e) = self.obs_error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&e).color(WARN));
                        if let Some((l, a)) = err_action(&e) {
                            if a != NextAct::ObsSettings && ui.small_button(l).clicked() {
                                act = Some(a);
                            }
                        }
                    });
                }
                egui::Grid::new("obs-conn").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Password");
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(260.0));
                    ui.end_row();
                });
                if ui.checkbox(&mut self.st.obs_remember_password, "Remember the password on this PC").changed() {
                    save = true;
                    if !self.st.obs_remember_password {
                        crate::config::forget_secret();
                    }
                }
                let busy = self.conn_rx.is_some();
                if ui.add_enabled(!busy, egui::Button::new(if busy { "Connecting…" } else { "Test connection" })).clicked() {
                    if self.st.obs_remember_password && !self.password.is_empty() {
                        crate::config::save_secret(&self.password);
                    }
                    self.connect_obs(ctx);
                }
                theme::small(ui, "Streamlabs Desktop and Meld Studio support is coming in a later build.");
            });
            ui.add_space(10.0);
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("Advanced: your PCs and pairing, OBS address, display, games, install and updates, notifications, privacy, remove Preflight").strong()).id_salt("settings-advanced").show(ui, |ui| {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("You chose to keep").size(16.0).strong());
                    theme::small(ui, "Fixes you decided not to make. They're never hidden: undo any of them here or in Your setup.");
                    let mut unkeep: Option<String> = None;
                    let mut ungo: Option<String> = None;
                    let mut restore: Option<usize> = None;
                    for id in self.st.kept_issues.clone() {
                        let title = self.analysis.issues.iter().find(|i| i.id == id).map(|i| i.title.trim_end_matches('.').to_string()).unwrap_or_else(|| format!("{} (not an issue right now)", id.replace('_', " ")));
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(title);
                            if ui.small_button("Fix it after all").clicked() {
                                unkeep = Some(id.clone());
                            }
                        });
                    }
                    for k in self.st.go_kept.clone() {
                        let label = match k.as_str() {
                            "mic" => "Ready to go live: microphone check",
                            "audio" => "Ready to go live: game audio check",
                            "capture" => "Ready to go live: whole-screen capture",
                            "cam" => "Ready to go live: hidden webcam",
                            "rec" => "Ready to go live: other recorders running",
                            _ => "Ready to go live: OBS check",
                        };
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(label);
                            if ui.small_button("Check it again").clicked() {
                                ungo = Some(k.clone());
                            }
                        });
                    }
                    for (i, h) in self.st.hidden_options.iter().enumerate() {
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(format!("Not suggested: {}", h.label));
                            if ui.small_button("Suggest again").clicked() {
                                restore = Some(i);
                            }
                        });
                    }
                    if self.st.kept_issues.is_empty() && self.st.go_kept.is_empty() && self.st.hidden_options.is_empty() {
                        theme::muted(ui, "Nothing. Every fix will be suggested when it fits.");
                    }
                    if let Some(id) = unkeep {
                        self.st.kept_issues.retain(|k| k != &id);
                        save = true;
                        recompute = true;
                    }
                    if let Some(k) = ungo {
                        self.st.go_kept.retain(|x| x != &k);
                        save = true;
                    }
                    if let Some(i) = restore {
                        self.st.hidden_options.remove(i);
                        save = true;
                        recompute = true;
                    }
                });
                ui.add_space(10.0);
                self.ui_pcs_card(ui, ctx);
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("OBS address").size(16.0).strong());
                    theme::small(ui, "Only change these if OBS runs on another PC or uses a different port.");
                    egui::Grid::new("obs-addr").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Address");
                    save |= ui.add(egui::TextEdit::singleline(&mut self.st.obs_host).desired_width(200.0)).changed();
                    ui.end_row();
                    ui.label("Port");
                    let mut port = self.st.obs_port.to_string();
                    if ui.add(egui::TextEdit::singleline(&mut port).desired_width(80.0)).changed() {
                        if let Ok(p) = port.parse() {
                            self.st.obs_port = p;
                            save = true;
                        }
                    }
                    ui.end_row();
                    });
                });
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Display").size(16.0).strong());
                    save |= ui.checkbox(&mut self.st.advanced_view, "Advanced view: show the technical reasons by default").changed();
                    ui.horizontal(|ui| {
                        ui.label("Text size");
                        for (v, l) in [(0.9f32, "Smaller"), (1.0, "Normal"), (1.2, "Larger"), (1.4, "Largest")] {
                            if ui.selectable_label((self.st.text_scale - v).abs() < 0.01, l).clicked() {
                                self.st.text_scale = v;
                                ctx.set_zoom_factor(v);
                                save = true;
                            }
                        }
                    });
                });
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Games").size(16.0).strong());
                    theme::small(ui, "Preflight only reads each game's settings file. It never changes it.");
                    let found: Vec<GameResult> = self.games.iter().filter(|g| g.relevant()).cloned().collect();
                    if found.is_empty() && !self.games.is_empty() {
                        theme::muted(ui, "None of the supported games were found on this PC.");
                    }
                    for g in found {
                        let (s, l, d) = if g.files.iter().any(|f| f.read_ok) {
                            (Status::Ok, "Ready", format!("Settings read ({} settings)", g.files.iter().map(|f| f.settings_count).sum::<usize>()))
                        } else if g.relevant() {
                            (Status::Warn, "Waiting", "Not launched on this PC yet. Launch it once and change any setting, and Preflight will read it.".to_string())
                        } else {
                            (Status::Unknown, "Not found", "Not installed on this PC.".to_string())
                        };
                        theme::inset(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&g.name).strong());
                                theme::status_pill(ui, s, l);
                            });
                            theme::muted(ui, &d);
                            if let Some(n) = &g.platform_note {
                                ui.label(RichText::new(n).size(13.0).color(WARN));
                            }
                            if g.relevant() {
                                egui::CollapsingHeader::new(RichText::new("Where Preflight looked").size(13.0)).id_salt(format!("where-{}", g.id)).show(ui, |ui| {
                                    for f in &g.files {
                                        theme::small(ui, format!("{} {}", if f.found { "Found:" } else { "Not found:" }, f.shown_path));
                                        if let Some(n) = &f.note {
                                            theme::small(ui, n);
                                        }
                                        if !f.found {
                                            for s in f.searched.iter().skip(1) {
                                                theme::small(ui, format!("  also checked {s}"));
                                            }
                                        }
                                    }
                                    theme::small(ui, format!("Profile v{} · {}", g.profile_version, g.verified));
                                });
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if self.games.is_empty() {
                        theme::muted(ui, "Scanning for games…");
                    }
                });
                        theme::card(ui, |ui| {
                    ui.label(RichText::new("Updates").size(16.0).strong());
                    ui.label(format!("{} v{} · {}", crate::util::APP_NAME, crate::util::VERSION, crate::util::BUILD_LABEL));
                    save |= ui.checkbox(&mut self.st.updates_auto, "Check for updates once a day").changed();
                    theme::small(ui, "Reads the public release list on GitHub (Matt-B-007/Stream-Preflight). Nothing about you or your PC is sent.");
                    if ui.checkbox(&mut self.st.updates_beta, "Include beta versions").changed() {
                        save = true;
                        self.update_result = None;
                    }
                    theme::small(ui, "Betas get new features first and may have rough edges. Turn this off to go back: you'll get the next stable version once it's newer than your beta.");
                    let busy = self.update_rx.is_some();
                    if ui.add_enabled(!busy, egui::Button::new(if busy { "Checking…" } else { "Check now" })).clicked() {
                        let (tx, rx) = channel();
                        let beta = self.st.updates_beta;
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::update::check(beta));
                        });
                        self.update_rx = Some(rx);
                    }
                    match &self.update_result {
                        Some(Ok(u)) if u.newer => {
                            let u = u.clone();
                            ui.label(RichText::new(format!("Update available: v{} to v{}{}", u.current, u.latest, if u.prerelease { " (beta)" } else { "" })).color(ACCENT).strong());
                            if !u.notes.trim().is_empty() {
                                egui::CollapsingHeader::new("What's new").id_salt("upd-notes").show(ui, |ui| {
                                    theme::muted(ui, &u.notes);
                                });
                            }
                            match (&self.update_install, &self.update_done) {
                                (Some(_), _) => {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Downloading and checking the update…");
                                    });
                                }
                                (None, Some(Ok(p))) => {
                                    ui.label(RichText::new(format!("v{} is installed. Restart Preflight to use it.", u.latest)).color(GOOD));
                                    if outlined(ui, "Restart now").clicked() {
                                        let p = p.clone();
                                        if crate::install::launch(&p) {
                                            self.quitting = true;
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                        }
                                    }
                                }
                                (None, Some(Err(e))) => {
                                    ui.label(RichText::new(e).color(BAD));
                                    ui.hyperlink_to("Open the release page", &u.url);
                                }
                                (None, None) => {
                                    ui.horizontal(|ui| {
                                        if outlined(ui, &format!("Update to v{}", u.latest)).clicked() {
                                            self.start_update();
                                        }
                                        ui.hyperlink_to("Release page", &u.url);
                                    });
                                }
                            }
                        }
                        Some(Ok(u)) => {
                            ui.label(RichText::new(format!("You're up to date (v{}).", u.current)).color(GOOD));
                        }
                        Some(Err(e)) => theme::muted(ui, e),
                        None => {}
                    }
                });
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Notifications").size(16.0).strong());
                    save |= ui.checkbox(&mut self.st.notify_test_done, "Tell me when a test finishes").changed();
                    save |= ui.checkbox(&mut self.st.remind_delete_recording, "Remind me I can delete test recordings").changed();
                });
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Privacy and data").size(16.0).strong());
                    theme::muted(ui, "Everything stays on this PC: your settings, history and any reports you save. Nothing is sent anywhere unless you send a report yourself.");
                    save |= ui
                        .checkbox(
                            &mut self.st.network_test_on_scan,
                            format!("Test my upload speed during scans (uses up to about {} each time)", crate::util::fmt_bytes(crate::network::MAX_TEST_BYTES)),
                        )
                        .changed();
                    // Storage: everything Preflight keeps, in plain sizes.
                    ui.add_space(4.0);
                    ui.label(RichText::new("Storage used by Preflight").strong());
                    let d = crate::util::dirs();
                    let size_of = |p: &std::path::Path| -> (usize, u64) {
                        std::fs::read_dir(p).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).fold((0usize, 0u64), |(n, b), m| (n + 1, b + m.len()))).unwrap_or((0, 0))
                    };
                    egui::Grid::new("storage").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                        for (name, dir, note) in [
                            ("Bug reports", d.reports.clone(), "Deleted after 14 days"),
                            ("Logs", d.logs.clone(), "Deleted after 14 days"),
                            ("Viewer preview frames", d.previews.clone(), "Latest test only"),
                            ("Saved results per game", d.tests.clone(), "One small file per game"),
                            ("Past streams", d.streams.clone(), "Post-flight summaries"),
                        ] {
                            let (n, b) = size_of(&dir);
                            ui.label(RichText::new(name).color(TEXT_SOFT));
                            ui.label(format!("{n} file{} · {}", if n == 1 { "" } else { "s" }, crate::util::fmt_bytes(b)));
                            ui.horizontal(|ui| {
                                theme::small(ui, note);
                                if n > 0 && ui.small_button("Open").clicked() {
                                    report::open_folder(&dir);
                                }
                            });
                            ui.end_row();
                        }
                    });
                    // Test recordings Preflight watched OBS make.
                    let recs: Vec<crate::config::PendingRecording> = self.st.pending_recordings.iter().filter(|r| std::path::Path::new(&r.path).exists()).cloned().collect();
                    let total: u64 = recs.iter().map(|r| r.size).sum();
                    ui.add_space(4.0);
                    ui.label(RichText::new(if recs.is_empty() { "Test recordings: none saved".to_string() } else { format!("Test recordings: {} file{}, {}", recs.len(), if recs.len() == 1 { "" } else { "s" }, crate::util::fmt_bytes(total)) }).strong());
                    let mut del: Vec<String> = Vec::new();
                    for r in &recs {
                        ui.horizontal(|ui| {
                            let name = std::path::Path::new(&r.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            ui.label(RichText::new(format!("{name} · {}", crate::util::fmt_bytes(r.size))).color(TEXT_SOFT));
                            if ui.small_button("Delete").clicked() {
                                del.push(r.path.clone());
                            }
                        });
                    }
                    if recs.len() > 1 && theme::secondary(ui, "Delete all test recordings").clicked() {
                        del.extend(recs.iter().map(|r| r.path.clone()));
                    }
                    for p in del {
                        self.delete_recording(ctx, &p);
                    }
                    save |= ui.checkbox(&mut self.st.auto_delete_recordings, "Delete test recordings automatically when Preflight is done with them").changed();
                    theme::small(ui, "Only the recordings Preflight's tests made. Your other recordings are never touched. With this on, you can't open a preview moment in your video player afterwards.");
                    ui.horizontal(|ui| {
                        if theme::secondary(ui, "Open data folder").clicked() {
                            report::open_folder(&crate::util::dirs().data);
                        }
                        if !self.confirm_clear {
                            if theme::secondary(ui, "Clear history").clicked() {
                                self.confirm_clear = true;
                            }
                        } else {
                            ui.label(RichText::new("Clear all history, past streams and preview frames?").color(WARN));
                            if ui.button("Yes, clear it").clicked() {
                                self.history.clear();
                                for d in [&crate::util::dirs().streams, &crate::util::dirs().previews] {
                                    if let Ok(rd) = std::fs::read_dir(d) {
                                        for e in rd.flatten() {
                                            let _ = std::fs::remove_file(e.path());
                                        }
                                    }
                                }
                                self.streams.clear();
                                self.preview = None;
                                self.preview_tex.clear();
                                self.confirm_clear = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_clear = false;
                            }
                        }
                        if theme::secondary(ui, "Forget OBS password").clicked() {
                            crate::config::forget_secret();
                            self.password.clear();
                            self.st.obs_remember_password = false;
                            save = true;
                        }
                    });
                });
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Install").size(16.0).strong());
                    let target = crate::install::target().map(|t| crate::util::scrub(&t.to_string_lossy())).unwrap_or_default();
                    if crate::install::is_installed() {
                        ui.label(RichText::new(format!("Installed for you at {target}.")).color(GOOD));
                        theme::small(ui, if cfg!(windows) { "It's in the Start menu, and in Settings › Apps for uninstalling." } else { "It's in your app menu." });
                    } else {
                        theme::small(ui, if cfg!(windows) { "Copies Preflight to your user folder, adds it to the Start menu and to Settings › Apps (so it can be uninstalled there). No admin rights needed." } else { "Copies Preflight to ~/.local/bin and adds it to your app menu. No admin rights needed." });
                        theme::small(ui, format!("Goes to {target}"));
                        if crate::install::old_program().is_some() {
                            ui.checkbox(&mut self.install_remove_old, "Also remove the old \"Stream Pre-Flight Check\" program file");
                        }
                        match &self.install_result {
                            Some(Ok(p)) => {
                                ui.label(RichText::new("Installed.").color(GOOD));
                                if outlined(ui, "Open the installed copy").clicked() {
                                    let p = p.clone();
                                    if crate::install::launch(&p) {
                                        self.quitting = true;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                ui.label(RichText::new(e).color(BAD));
                            }
                            None => {
                                if outlined(ui, "Install for this user").clicked() {
                                    self.install_result = Some(crate::install::install(self.install_remove_old));
                                }
                            }
                        }
                    }
                });
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Remove Preflight from this PC").size(16.0).strong());
                    theme::small(ui, "Deletes Preflight's own settings, saved password, history, logs and reports, and the program file if you want. Shows you the list first. Never touches OBS or your games.");
                    if theme::secondary(ui, "Remove Preflight…").clicked() {
                        let recs = self.st.pending_recordings.iter().filter(|r| std::path::Path::new(&r.path).exists()).map(|r| (r.path.clone(), r.size, false)).collect();
                        self.uninstall = Some((recs, true, None));
                    }
                });
        
        });
        if save {
            self.st.save();
        }
        if recompute || save {
            self.recompute();
        }
        if let Some(a) = act {
            self.do_act(ctx, a);
        }
    }

    // -----------------------------------------------------------------------
    // Help & report a problem

    fn ui_help(&mut self, ui: &mut Ui) {
        theme::title(ui, "Help & report a problem");
        ui.add_space(8.0);
        let mut report = false;
        theme::card(ui, |ui| {
            ui.label(RichText::new("Something not working?").size(17.0).strong());
            theme::muted(ui, "The quickest way is the \"This looks wrong\" link right next to the problem. For anything else:");
            if theme::secondary(ui, "  Report a problem  ").clicked() {
                report = true;
            }
        });
        if report {
            self.open_report("");
        }
        ui.add_space(10.0);
        ui.label(RichText::new("What to do if…").size(19.0).strong());
        ui.add_space(4.0);
        let topics: [(&str, &[&str]); 6] = [
            ("OBS won't connect or asks for a password", &[
                "In OBS, open Tools › WebSocket Server Settings.",
                "Tick \"Enable WebSocket server\". Click \"Show Connect Info\" and copy the password.",
                "Paste it in Preflight's Settings › OBS connection, then click Test connection.",
                "Still nothing? Make sure OBS is open and the port matches (4455 unless you changed it, under Settings › Advanced).",
            ]),
            ("My game isn't found, or says \"not launched yet\"", &[
                "Preflight reads a game's settings file, which only exists after you've launched the game once and changed a setting.",
                "Launch the game, open its graphics settings, change anything and change it back, then quit. Rescan in Preflight.",
                "Settings › Advanced › Games shows exactly where Preflight looked.",
                "Games without a profile still get tested: Preflight finds them by their GPU use while you play.",
            ]),
            ("The test won't start", &[
                "The test starts by itself when OBS starts recording. Check the OBS line in Step 1 says Connected.",
                "If OBS is connected, press Record in OBS (or Start Streaming). You can also press Start the test by hand.",
                "Play for at least 2 minutes so Preflight sees real game load.",
            ]),
            ("The viewer preview is missing", &[
                "The preview needs ffmpeg, a free video tool. Install it from your package manager (Linux) or ffmpeg.org (Windows), then restart Preflight.",
                "It also needs your test recording, so leave it where OBS saved it until the preview is made.",
                "\"Couldn't line up\" means the stream and recording frames didn't match. Run the test again with the same scene showing.",
            ]),
            ("The upload test failed", &[
                "Preflight sends a few short bursts of data to measure your upload. A firewall, VPN or busy connection can stop it.",
                "Pause big downloads or uploads and try again: click Details on the status line at the top of Pre-flight.",
                "For a real Twitch measurement, add ?bandwidthtest=true to your stream key and stream for a minute. Remove it afterwards: Preflight reminds you.",
            ]),
            ("Something looks wrong", &[
                "Click \"This looks wrong\" next to it, or Report a problem above.",
                "The report has no passwords, stream keys, names or scene names. You can read it before you send it.",
                "If it only happens sometimes, turn on \"Record more detail\" under Advanced tools below, make it happen, then report.",
            ]),
        ];
        for (i, (q, steps)) in topics.iter().enumerate() {
            let open = self.help_open == Some(i);
            let mut toggle = false;
            theme::card(ui, |ui| {
                let r = egui::CollapsingHeader::new(RichText::new(*q).strong()).id_salt(("help-q", i)).open(Some(open)).show(ui, |ui| {
                    for (n, st) in steps.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{}.", n + 1)).strong());
                            ui.label(*st);
                        });
                    }
                    if i == 0 && ui.button("Open OBS connection settings").clicked() {
                        self.go(Screen::Settings);
                    }
                    if i == 3 && crate::preview::ffmpeg_path().is_some() {
                        theme::small(ui, "ffmpeg is installed.");
                    }
                });
                if r.header_response.clicked() {
                    toggle = true;
                }
            });
            if toggle {
                self.help_open = if open { None } else { Some(i) };
            }
            ui.add_space(6.0);
        }
        theme::card(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("I want to remove Preflight").strong()).id_salt("help-remove").show(ui, |ui| {
                ui.label("Settings › Advanced › Remove Preflight from this PC deletes everything below in one go. Or delete these folders yourself:");
                for (what, path) in crate::uninstall::own_folders() {
                    theme::small(ui, format!("• {what}: {}", crate::util::scrub(&path.to_string_lossy())));
                }
                if cfg!(not(windows)) {
                    theme::small(ui, "• App menu entry and icon: ~/.local/share/applications/stream-preflight.desktop and ~/.local/share/icons/hicolor/*/apps/stream-preflight.png");
                }
                theme::small(ui, "Then delete the Preflight program file. That's all: Preflight doesn't install anything else.");
            });
            egui::CollapsingHeader::new(RichText::new("Using Preflight with the keyboard").strong()).id_salt("help-keys").show(ui, |ui| {
                theme::small(ui, "• Tab and Shift+Tab move between buttons; Enter or Space presses the one that's outlined.");
                theme::small(ui, "• Ctrl+Enter does the Next step shown at the bottom.");
                theme::small(ui, "• Alt+1 Test, Alt+2 Fix, Alt+3 Your setup, Alt+4 Post-flight.");
                theme::small(ui, "• Every colour also has a word next to it (Change, Keep, OK, Great…), so nothing depends on colour alone.");
            });
        });
        ui.add_space(6.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("How Preflight works").size(17.0).strong());
            ui.label("1. Test while you play: start your game, press Record in OBS and play for 2 minutes. Preflight measures by itself.");
            ui.label("2. Fix what matters: one change at a time, grouped by where you make it (OBS, your game, your PC). Preflight notices when you've changed it. Keep anything you'd rather not change.");
            ui.label("3. Ready to go live: a short checklist that ticks itself.");
            ui.label("Your setup shows every setting at a glance: what it is now, what to change, and why. Post-flight tells you how each stream went.");
            theme::small(ui, "Preflight only reads settings files and system stats. It never edits game or OBS settings, never touches a running game, and never controls OBS.");
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("Advanced tools").strong()).id_salt("adv-tools").show(ui, |ui| {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Self-test").strong());
                    if ui.button("Run self-test").clicked() {
                        self.selftests = report::self_test(&report::SelfTestInput {
                            hw: self.hw.as_ref(),
                            games: &self.games,
                            obs: self.obs.as_ref(),
                            obs_error: self.obs_error.as_deref(),
                            net: self.net.as_ref(),
                        });
                    }
                });
                for t in &self.selftests {
                    ui.horizontal(|ui| {
                        let (s, l) = match t.status {
                            TestStatus::Pass => (Status::Ok, "Pass"),
                            TestStatus::Warn => (Status::Warn, "Warning"),
                            TestStatus::Fail => (Status::Bad, "Fail"),
                        };
                        theme::status_pill(ui, s, l);
                        ui.label(RichText::new(&t.name).strong());
                    });
                    theme::small(ui, &t.detail);
                }
                ui.separator();
                let mut on = crate::logger::DETAILED.load(std::sync::atomic::Ordering::Relaxed);
                if ui.checkbox(&mut on, "Record more detail (for problems that only happen sometimes)").changed() {
                    crate::logger::set_detailed(on);
                }
                theme::small(ui, "Turn this on, make the problem happen, then report it. It turns off when you close Preflight.");
                if ui.button("Open reports folder").clicked() {
                    report::open_folder(&crate::util::dirs().reports);
                }
            });
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("About").size(17.0).strong());
            ui.label(format!("{} v{} · {}", crate::util::APP_NAME, crate::util::VERSION, crate::util::BUILD_LABEL));
            ui.add_space(4.0);
            ui.label(RichText::new("New in v0.1.0 (first release)").strong());
            for s in [
                "New name: Stream Preflight. Your settings, history and saved OBS password move over by themselves",
                "Cockpit glass look: live readings strip, score dials, a glowing \"Do this next\" card and one bright button in the Next bar",
                "The Test step is one checklist that ticks itself as OBS starts streaming and recording",
                "Two PCs: connect to OBS on your streaming PC, capture card checks (frame rate, resolution, colour range, MJPEG, HDR, refresh judder), and fixes labelled by PC",
                "Paired mode: Preflight on both PCs, linked once with a 6-digit code, home network only",
                "Install for this user, updates from GitHub with a checksum check, beta versions if you want them, and uninstall (also from Windows Settings › Apps)",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("From test build 13").strong());
            for s in [
                "A new sidebar: Pre-flight opens into your game, its style, and the steps Test, Fix, Your setup and Ready to go live. Post-flight holds Last stream and History",
                "Click the game name to switch games, and the style under it to re-sort your fixes (no new test needed)",
                "Each step shows one \"Do this next\" card. The only bright button is in the Next bar at the bottom",
                "Go live is now \"Ready to go live\", the last step of Pre-flight",
                "Your setup: plain rows you can fold, \"Fix this\" only where something should change, and the scan results under PC and connections",
                "System tray: grey idle, green watching OBS, amber testing, red live, and a white dot when Preflight needs you. Minimize and close options in Settings",
                "Notifications when a test finishes or a stream ends while Preflight is in the background",
                "If you start streaming while the bandwidth-test flag may still be on, Preflight asks whether it's a real stream",
                "One banner at a time, most important first",
                "Bug reports now include post-flight numbers, test history, one setup for all games, what was on screen, and the tray",
                "Keyboard: Alt+1 Test, Alt+2 Fix, Alt+3 Your setup, Alt+4 Post-flight. Ctrl+Enter presses the Next button",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 12").strong());
            for s in [
                "Stream + record test: a checklist that ticks itself as you start streaming and recording, with each platform's private test mode (Twitch test flag, YouTube Unlisted)",
                "Several platforms: tick Twitch, YouTube and Kick in Settings. Preflight suggests one shared encode, one each, or a restream service, and checks the upload",
                "Video encoders panel in Your setup: every GPU encoder, its chips and codecs, and which programs are using it right now",
                "OBS plugins: Source Record, vertical canvases and extra streams are counted as encodes and measured in the test",
                "One setup for all your games: OBS settings that suit every tested game; heavy games get in-game changes instead",
                "Minecraft: any launcher (CurseForge, Prism, Modrinth and more), the modpack's name, shaders and Distant Horizons",
                "Any OBS setting change counts as a change, tests remember their OBS settings, and P6 counts as fine",
                "Privacy: Steam IDs and account numbers are removed from reports",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 11").strong());
            for s in [
                "Reads games without a profile: works out the engine (Unity, Unreal, Source, Godot) and reads its settings, for Steam, Heroic, Lutris, Bottles, Epic and Game Pass games",
                "A fuller Counter-Strike 2 profile, and each game table says how many settings were read and how many the advice uses",
                "After \"I changed it\", Preflight checks that the game really saved the change",
                "Styles fixed: Competitive lowers graphics first, Balanced uses DLSS/FSR, Cinematic caps FPS first. Switch style on the Pre-flight screen; each game remembers its own",
                "Stream picture advice: the sharpest resolution for your bitrate and game, NVENC quality options scaled to your GPU's headroom, audio bitrate and sample rate",
                "In-game effects that cost bitrate (film grain, motion blur) are pointed out",
                "The viewer preview re-creates your stream from a high-quality recording, and skips moments it can't line up",
                "Pick your test: full, stream + record, or quick check. One Test again button, and a Storage list in Settings",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 10").strong());
            for s in [
                "Results per game: a banner when you switch games, a game switcher, and before/after only for the same game",
                "Only suggests settings a game really has; games without an FPS limit get one outside the game (Steam launch option or driver)",
                "Each game table shows whether its settings file was read, and when the game last saved it",
                "Cities: Skylines II reads your real settings; Linux game capture (vkcapture) is recognised",
                "Delete test recordings from the result, before the next test, or in Settings",
                "Bug reports include game settings files; Show in folder, Open report and Copy work on KDE Wayland",
                "On Linux, Preflight adds itself to your app menu with its icon",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 9").strong());
            for s in [
                "Fixes say exactly what to change: the setting, what it is now, and what to change it to",
                "Reads your real game settings, including the preset; with a Custom preset it names the heavy settings",
                "FPS-limit advice that fits: if your limit is too high to ever kick in, it suggests a lower one",
                "Measures your game's FPS (PresentMon on Windows, a MangoHud log on Linux) or asks you",
                "Two scores: Smoothness (what the test measured) and Setup (settings that will help)",
                "1-minute quick re-test for Check my changes, results remembered per game, Copy as text",
                "New game profiles (unverified): VALORANT, Apex, Warzone, League, Minecraft, Marvel Rivals, ELDEN RING, GTA V",
                "Bug reports saved as a named text file; Remove Preflight from this PC; a new app icon",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 8").strong());
            for s in [
                "Your setup: every OBS, game and PC setting with what it is now, what to change and why",
                "Pre-flight in 3 steps, with a Next button always at the bottom",
                "Fix one thing at a time, see how much each fix helps, or keep it as it is",
                "Check my changes: a quick re-test that shows before and after",
                "Viewer preview with a slider between the original and the stream",
                "A \"What changed\" timeline in History",
                "One-page Settings and \"What to do if…\" help",
                "Fixes from build 7: better game detection, long GPU-maxed periods, preview matching, Cyberpunk FPS limit",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test builds 4 to 7").strong());
            for s in [
                "A guided pre-flight: Scan, Test, Fix, Go live",
                "Setup in 3 short screens, with your monitors and refresh rates detected",
                "The test starts by itself when you press Record in OBS",
                "Preflight notices when you've made a fix",
                "Plain sentences with the real numbers everywhere",
                "New OBS quality checks: rate control, keyframe interval, encoder preset and downscale filter",
                "Viewer preview: your stream next to the original, from your test recording",
                "Post-flight: a summary of how each stream went, with the worst moments and what to change",
                "Recording while streaming: separate encoder, replay buffer and Enhanced Broadcasting checks, with preset advice for each side",
                "Encoder chip load and frames dropped per output (stream and recording)",
                "Recording space left, crash-safe recording format, and hard-drive warnings",
                "\"Your recommended OBS settings\" in one list, and issues grouped by where you fix them",
                "Hover any term (bitrate, CBR, keyframe...) for a plain-English meaning",
                "One shared OBS connection, and less background work while minimized",
                "\"This looks wrong\" links and a one-box problem report",
                "A simpler menu",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Not in this test build yet").strong());
            for s in [
                "Streamlabs Desktop and Meld Studio connections",
                "In-game frame times (PresentMon)",
                "System tray mode",
                "Installing updates in place (needs the GitHub repository)",
                "Keyring password storage on Linux",
            ] {
                theme::small(ui, format!("• {s}"));
            }
        });
    }
}

fn load_tex(ctx: &egui::Context, path: &std::path::Path, name: &str) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgb8();
    let (w, h) = img.dimensions();
    let ci = egui::ColorImage::from_rgb([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, ci, egui::TextureOptions::LINEAR))
}

fn grade_color(level: u8) -> Color32 {
    match level {
        0 | 1 => GOOD,
        2 => WARN,
        _ => BAD,
    }
}

impl App {
    /// Only one banner shows at a time, most important first:
    /// 1 real-stream question, 2 remove the test flag, 3 something went wrong,
    /// 4 stream ended, 5 you're live.
    fn banner_pick(&self) -> u8 {
        let testing = self.test_handle.is_some();
        if self.flag_question {
            1
        } else if self.st.bandwidth_test_pending && !testing {
            2
        } else if self.fail_banner.is_some() {
            3
        } else if self.stream_notice.is_some() && !(self.screen == Screen::PostFlight && self.stream_sel == 0) {
            4
        } else if self.live.is_some() && self.screen != Screen::PostFlight && !(self.screen == Screen::Preflight && self.step == 4) {
            5
        } else if self.update_banner() {
            6
        } else {
            0
        }
    }

    /// A newer version exists and the user hasn't put it off this session.
    fn update_banner(&self) -> bool {
        !self.update_later && matches!(&self.update_result, Some(Ok(u)) if u.newer) && self.live.is_none() && !self.test_handle.as_ref().map(|h| h.phase() == Phase::Measuring).unwrap_or(false)
    }

    /// Starts downloading and installing the update in the background.
    fn start_update(&mut self) {
        if let Some(Ok(u)) = &self.update_result {
            let (tx, rx) = channel();
            let uu = u.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::update::install(&uu));
            });
            self.update_install = Some(rx);
            self.update_done = None;
        }
    }

    fn ui_update_banner(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(Ok(u)) = self.update_result.clone() else { return };
        let mut act: Option<&str> = None;
        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 18))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 120)))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("Stream Preflight v{} is available{}", u.latest, if u.prerelease { " (beta)" } else { "" })).size(17.0).strong().color(ACCENT));
                        match (&self.update_install, &self.update_done) {
                            (Some(_), _) => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Downloading and checking it… Preflight restarts by itself when it's ready.").color(TEXT_SOFT));
                                });
                            }
                            (None, Some(Err(e))) => {
                                ui.label(RichText::new(format!("The update didn't install: {e}")).color(BAD));
                            }
                            _ => {
                                ui.label(RichText::new(format!("You have v{}. Updating takes a few seconds and keeps all your settings.", u.current)).color(TEXT_SOFT));
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.update_install.is_none() {
                            if outlined(ui, "Later").clicked() {
                                act = Some("later");
                            }
                            if !u.url.is_empty() && outlined(ui, "What's new").clicked() {
                                act = Some("notes");
                            }
                            if outlined(ui, if matches!(self.update_done, Some(Err(_))) { "Try again" } else { "Update now" }).clicked() {
                                act = Some("update");
                            }
                        }
                    });
                });
            });
        ui.add_space(10.0);
        match act {
            Some("update") => self.start_update(),
            Some("later") => self.update_later = true,
            Some("notes") => ctx.open_url(egui::OpenUrl::new_tab(&u.url)),
            _ => {}
        }
    }

    fn ui_live_banners(&mut self, ui: &mut Ui) {
        let mut go = false;
        let pick = self.banner_pick();
        if pick == 6 {
            let ctx = ui.ctx().clone();
            self.ui_update_banner(ui, &ctx);
        }
        if pick == 1 {
            let mut answer: Option<bool> = None;
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x3A, 0x2A, 0x10))
                .stroke(egui::Stroke::new(1.0, WARN))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Is this a real stream?").size(17.0).strong().color(WARN));
                    theme::muted(ui, "You started streaming, but Preflight still thinks ?bandwidthtest=true is in your stream key and no test is running. If the flag is still there, Twitch hides your stream from viewers. OBS › Settings › Stream › Stream Key.");
                    ui.horizontal_wrapped(|ui| {
                        if outlined(ui, "It's real: I removed the flag").clicked() {
                            answer = Some(true);
                        }
                        if outlined(ui, "It's a test stream").clicked() {
                            answer = Some(false);
                        }
                    });
                });
            match answer {
                Some(true) => {
                    self.flag_question = false;
                    self.flag_real = true;
                    self.st.bandwidth_test_pending = false;
                    self.st.save();
                    log::info!("Forgotten-flag check: user says the stream is real and the flag is removed");
                }
                Some(false) => {
                    self.flag_question = false;
                    log::info!("Forgotten-flag check: user says it's a test stream");
                }
                None => {}
            }
            ui.add_space(10.0);
        }
        if pick == 2 {
            let mut removed = false;
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x3A, 0x2A, 0x10))
                .stroke(egui::Stroke::new(1.0, WARN))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Remove ?bandwidthtest=true from your stream key before you go live.").strong().color(WARN));
                    theme::muted(ui, "While it's there, Twitch accepts your stream but doesn't show it to anyone. OBS › Settings › Stream › Stream Key.");
                    if theme::secondary(ui, "I removed it").clicked() {
                        removed = true;
                    }
                });
            if removed {
                self.st.bandwidth_test_pending = false;
                self.st.save();
                log::info!("User confirmed removing the bandwidth-test flag");
            }
            ui.add_space(10.0);
        }
        if let Some(l) = self.live.clone() {
            if pick == 5 {
                egui::Frame::NONE
                    .fill(GOOD_BG)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2F, 0x6B, 0x47)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("You're live · {}", crate::streamwatch::fmt_t(l.elapsed))).strong().color(GOOD));
                            ui.label(format!("{:.2}% of frames dropped so far{}.", l.dropped_pct, l.kbps.map(|k| format!(" · {k:.0} kbps")).unwrap_or_default()));
                            if link_small(ui, "Open post-flight") {
                                go = true;
                            }
                        });
                    });
                ui.add_space(10.0);
            }
        }
        if self.screen == Screen::PostFlight && self.stream_sel == 0 {
            self.stream_notice = None;
        }
        if let Some(n) = self.stream_notice.clone().filter(|_| pick == 4) {
            let p = crate::streamwatch::summarize(&n);
            let mut dismiss = false;
            theme::card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Your {} ended · {} · ", if n.bandwidth_test { "test stream" } else { "stream" }, crate::streamwatch::fmt_duration(p.duration))).strong());
                    ui.label(RichText::new(&p.grade).strong().color(grade_color(p.grade_level)));
                });
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "  See the post-flight summary  ").clicked() {
                        go = true;
                        dismiss = true;
                    }
                    if link_small(ui, "Dismiss") {
                        dismiss = true;
                    }
                });
            });
            ui.add_space(10.0);
            if dismiss {
                self.stream_notice = None;
            }
        }
        if go {
            self.stream_sel = 0;
            self.go(Screen::PostFlight);
        }
    }

    // -----------------------------------------------------------------------
    // Viewer preview (in Results)

    fn ui_viewer_preview(&mut self, ui: &mut Ui) {
        if std::mem::take(&mut self.scroll_to_preview) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        let mut action: Option<&str> = None;
        let target = self.obs.as_ref().and_then(|o| o.bitrate_kbps);
        let separate_rec = self.obs.as_ref().map(|o| !o.rec_encoder_raw.is_empty()).unwrap_or(false);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Viewer preview").size(21.0).strong());
            if separate_rec {
                ui.label(RichText::new("Your recording has its own encoder, so this shows your recording, not exactly what viewers get. For a true viewer preview, record with the stream encoder for the test.").size(13.0).color(WARN));
            }
            if self.test.is_none() {
                theme::muted(ui, "Run the test (Pre-flight, step 1). Afterwards you'll see your stream next to the original, as viewers would see it.");
                return;
            }
            if self.preview_rx.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Making your viewer preview…");
                });
                return;
            }
            let Some(p) = self.preview.clone() else {
                theme::muted(ui, "This test was made before the viewer preview existed. Run another test to see one.");
                return;
            };
            if p.pairs.is_empty() {
                theme::muted(ui, p.error.as_deref().unwrap_or("No preview frames."));
                return;
            }
            let n = p.pairs.len();
            let i = self.preview_idx.min(n - 1);
            let pair = p.pairs[i].clone();
            ui.horizontal(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("Previous")).clicked() {
                    action = Some("prev");
                }
                ui.label(RichText::new(format!("Moment {} of {n} · {} into the recording", i + 1, crate::streamwatch::fmt_t(pair.t_sec))).color(TEXT_SOFT));
                if ui.add_enabled(i + 1 < n, egui::Button::new("Next")).clicked() {
                    action = Some("next");
                }
                ui.add_space(12.0);
                ui.checkbox(&mut self.preview_zoom, "Zoom to actual size");
            });
            match pair.match_db {
                Some(db) => {
                    let (head, meaning) = crate::preview::meaning(db);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{head}.")).size(18.0).strong().color(if db >= 36.0 { GOOD } else if db >= 32.0 { WARN } else { BAD }));
                        ui.label(RichText::new(meaning).color(TEXT_SOFT));
                        ui.label(RichText::new(format!("(picture match {db:.0} dB)")).size(12.5).color(MUTED));
                    });
                }
                None if pair.viewer.is_some() => {
                    ui.label(RichText::new("Preflight couldn't line these two frames up exactly (probably fast movement), so there's no score for this moment. Try another moment.").color(WARN));
                }
                None => {}
            }
            ui.add_space(6.0);
            if self.preview_tex.len() != n {
                self.preview_tex = vec![None; n];
            }
            if self.preview_tex[i].is_none() {
                let ctx = ui.ctx().clone();
                self.preview_tex[i] = Some((load_tex(&ctx, &pair.original, &format!("orig-{i}")), pair.viewer.as_ref().and_then(|v| load_tex(&ctx, v, &format!("view-{i}")))));
            }
            let (orig_tex, view_tex) = self.preview_tex[i].clone().unwrap_or((None, None));
            let zoom = self.preview_zoom;
            let right_label = if separate_rec { "Your recording".to_string() } else { format!("Viewers{}", target.map(|b| format!(" at {b} kbps")).unwrap_or_default()) };
            if let Some(o) = &orig_tex {
                let [tw, th] = o.size();
                let (tw, th) = (tw as f32, th as f32);
                let w = ui.available_width();
                let (size, uv) = if zoom {
                    let ww = w.min(tw);
                    let hh = (ww * 0.5).min(th);
                    let (fw, fh) = (ww / tw, hh / th);
                    (Vec2::new(ww, hh), egui::Rect::from_min_size(egui::pos2(0.5 - fw / 2.0, 0.5 - fh / 2.0), Vec2::new(fw, fh)))
                } else {
                    (Vec2::new(w, w * th / tw), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)))
                };
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                if let (true, Some(pos)) = (resp.dragged() || resp.clicked(), resp.interact_pointer_pos()) {
                    self.preview_split = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                }
                let painter = ui.painter_at(rect);
                let split = if view_tex.is_some() { self.preview_split } else { 1.0 };
                let sx = rect.left() + rect.width() * split;
                let lr = egui::Rect::from_min_max(rect.min, egui::pos2(sx, rect.max.y));
                let luv = egui::Rect::from_min_max(uv.min, egui::pos2(uv.min.x + uv.width() * split, uv.max.y));
                painter.image(o.id(), lr, luv, Color32::WHITE);
                if let Some(v) = &view_tex {
                    let rr = egui::Rect::from_min_max(egui::pos2(sx, rect.min.y), rect.max);
                    let ruv = egui::Rect::from_min_max(egui::pos2(uv.min.x + uv.width() * split, uv.min.y), uv.max);
                    painter.image(v.id(), rr, ruv, Color32::WHITE);
                    painter.line_segment([egui::pos2(sx, rect.min.y), egui::pos2(sx, rect.max.y)], egui::Stroke::new(2.0, ACCENT));
                    let tag = |p: egui::Pos2, text: &str, align: egui::Align2| {
                        let g = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(14.0), TEXT);
                        let r = align.anchor_size(p, g.size()).expand(5.0);
                        painter.rect_filled(r, 6.0, Color32::from_black_alpha(170));
                        painter.galley(r.min + Vec2::splat(5.0), g, TEXT);
                    };
                    tag(rect.left_top() + Vec2::new(8.0, 8.0), "Original (what OBS draws)", egui::Align2::LEFT_TOP);
                    tag(rect.right_top() + Vec2::new(-8.0, 8.0), &right_label, egui::Align2::RIGHT_TOP);
                }
                if view_tex.is_some() {
                    ui.add(egui::Slider::new(&mut self.preview_split, 0.0..=1.0).show_value(false).text("Drag across the picture (or this slider) to compare"));
                }
            }
            if view_tex.is_none() {
                if p.ffmpeg_missing {
                    theme::inset(ui, |ui| {
                        ui.label(RichText::new("Preflight needs ffmpeg to read your test recording, to show the viewer side.").strong());
                        theme::muted(
                            ui,
                            if cfg!(windows) {
                                "Install it once: open Terminal and run  winget install ffmpeg  then click Try again."
                            } else {
                                "Install it once with your package manager (CachyOS/Arch: sudo pacman -S ffmpeg), then click Try again."
                            },
                        );
                        ui.horizontal(|ui| {
                            if theme::secondary(ui, "Try again").clicked() {
                                action = Some("retry");
                            }
                            if theme::secondary(ui, "Open the recording instead").clicked() {
                                action = Some("open");
                            }
                        });
                    });
                } else if let Some(e) = &p.error {
                    theme::muted(ui, e);
                }
            }
            if let Some(sim) = &p.simulated {
                theme::small(ui, sim);
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                theme::small(ui, "Preflight compares fine detail between the two pictures. Internet problems aren't included; the post-flight summary covers those.");
                if link_small(ui, "This looks wrong") {
                    action = Some("report");
                }
            });
        });
        match action {
            Some("prev") => self.preview_idx = self.preview_idx.saturating_sub(1),
            Some("next") => self.preview_idx += 1,
            Some("retry") => {
                if let Some(t) = self.test.clone() {
                    let ctx = ui.ctx().clone();
                    self.start_preview(&ctx, &t);
                }
            }
            Some("open") => {
                if let Some(p) = self.test.as_ref().and_then(|t| t.recording_path.clone()) {
                    crate::preview::open_in_player(&p);
                }
            }
            Some("report") => {
                let db = self.preview.as_ref().and_then(|p| p.pairs.get(self.preview_idx)).and_then(|x| x.match_db);
                self.open_report(format!("Viewer preview: moment {} matched at {:?} dB", self.preview_idx + 1, db.map(|d| d.round())));
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Post-flight

    fn ui_postflight(&mut self, ui: &mut Ui) {
        theme::title(ui, "Post-flight");
        theme::subtitle(ui, "How your stream actually went, and what to change next time.");
        ui.add_space(10.0);
        if let Some(l) = self.live.clone() {
            theme::card(ui, |ui| {
                ui.label(RichText::new(format!("Live now · {}", crate::streamwatch::fmt_t(l.elapsed))).size(21.0).strong().color(GOOD));
                ui.label(format!(
                    "{:.2}% of frames dropped so far{}.",
                    l.dropped_pct,
                    l.kbps.map(|k| format!(", sending {k:.0} kbps right now")).unwrap_or_default()
                ));
                theme::muted(ui, "Preflight only reads OBS's stream stats, every 5 seconds. It keeps watching from the tray; your summary appears here when you end the stream.");
            });
            ui.add_space(10.0);
        }
        if self.streams.is_empty() {
            if self.live.is_none() {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("No streams yet").size(18.0).strong());
                    theme::muted(ui, "Leave Preflight running while you stream (in the tray is fine). It watches OBS's stream stats without changing anything, and when you end the stream you'll get:");
                    for s in [
                        "How many frames your internet dropped, and when",
                        "Whether the encoder or the GPU fell behind",
                        "Your real bitrate, including any dips",
                        "The worst moments, with timestamps",
                        "What to change before your next stream",
                    ] {
                        theme::small(ui, format!("• {s}"));
                    }
                });
            }
            return;
        }
        let sel = self.stream_sel.min(self.streams.len() - 1);
        let l = self.streams[sel].clone();
        let p = crate::streamwatch::summarize(&l);
        let mut report = false;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&p.grade).size(30.0).strong().color(grade_color(p.grade_level)));
                    ui.label(
                        RichText::new(format!(
                            "{} · {}{}{}",
                            history::fmt_time(l.started_at),
                            crate::streamwatch::fmt_duration(p.duration),
                            l.game.as_ref().map(|g| format!(" · {g}")).unwrap_or_default(),
                            if l.output.is_empty() { String::new() } else { format!(" · {} with {}", l.output, l.encoder) }
                        ))
                        .color(TEXT_SOFT),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if link_small(ui, "This looks wrong") {
                        report = true;
                    }
                });
            });
            if let Some(n) = &l.partial {
                ui.label(RichText::new(n).size(13.0).color(WARN));
            }
            if l.bandwidth_test {
                ui.label(RichText::new("Twitch bandwidth test: nobody could see this stream.").size(13.0).color(INFO));
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("What to change next time").size(17.0).strong());
            if p.advice.is_empty() {
                theme::muted(ui, "Nothing. Keep the same settings next time.");
            }
            for a in &p.advice {
                theme::inset(ui, |ui| {
                    ui.horizontal(|ui| {
                        tag_pill(ui, analysis::Tag::Change);
                        ui.label(RichText::new(&a.title).strong());
                    });
                    theme::muted(ui, &a.why);
                    ui.label(RichText::new(&a.menu).size(12.5).color(MUTED));
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(10.0);
        let cards = [("Internet", p.net_sentence(), p.net_frac), ("Encoder", p.enc_sentence(), p.enc_frac), ("Drawing frames", p.render_sentence(), p.render_frac)];
        ui.columns(3, |c| {
            for (i, (name, (head, meaning), f)) in cards.iter().enumerate() {
                theme::card(&mut c[i], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(*name).strong());
                        let (s, lab) = if *f < 0.001 { (Status::Ok, "Good") } else if *f < 0.01 { (Status::Warn, "Minor") } else { (Status::Bad, "Problem") };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| theme::status_pill(ui, s, lab));
                    });
                    ui.label(RichText::new(head).strong());
                    theme::muted(ui, meaning);
                });
            }
        });
        if !p.outputs.is_empty() {
            ui.add_space(6.0);
            theme::inset(ui, |ui| {
                ui.label(RichText::new("Your other outputs during the stream").strong());
                theme::small(ui, p.outputs.join(" · "));
            });
        }
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Bitrate").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if p.reconnects > 0 {
                        theme::status_pill(ui, Status::Bad, &format!("{} reconnect{}", p.reconnects, if p.reconnects == 1 { "" } else { "s" }));
                    }
                });
            });
            let target = l.target_kbps.map(|b| format!(" of the {b} you set")).unwrap_or_default();
            ui.label(RichText::new(format!("Your stream averaged {:.0} kbps{target}.", p.avg_kbps)).strong());
            if let Some((low, t)) = p.low_kbps.filter(|(low, _)| *low < p.avg_kbps * 0.9) {
                theme::muted(ui, format!("The lowest 30 seconds averaged {low:.0} kbps, around {}.", crate::streamwatch::fmt_t(t)));
            }
            let max = l.target_kbps.map(|b| b as f32 * 1.3).unwrap_or_else(|| p.kbps_series.iter().cloned().fold(1.0, f32::max));
            theme::spark_bars(ui, &p.kbps_series, max, ACCENT, 60.0);
            theme::small(
                ui,
                format!(
                    "CPU averaged {:.0}%{}{}{}.{}",
                    p.cpu_avg,
                    p.gpu_avg.map(|g| format!(", GPU {g:.0}%")).unwrap_or_default(),
                    p.enc_avg.map(|e| format!(", encoder chip {e:.0}%")).unwrap_or_default(),
                    p.gpu_temp_max.map(|t| format!(", GPU peaked at {t:.0}°C")).unwrap_or_default(),
                    match (p.own_cpu_avg, p.own_mb_max) {
                        (Some(c), Some(m)) => format!(" Preflight itself used {c:.1}% CPU and {m:.0} MB of memory."),
                        _ => String::new(),
                    }
                ),
            );
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Worst moments").size(17.0).strong());
            if p.moments.is_empty() {
                theme::muted(ui, "No rough moments. Nice.");
            }
            for m in &p.moments {
                ui.horizontal_wrapped(|ui| {
                    let when = if m.secs > 0.0 { format!("{}–{}", crate::streamwatch::fmt_t(m.t), crate::streamwatch::fmt_t(m.t + m.secs)) } else { crate::streamwatch::fmt_t(m.t) };
                    ui.label(RichText::new(when).monospace().color(ACCENT));
                    ui.label(&m.text);
                });
            }
            theme::small(ui, "Times are from the start of the stream, so you can find them in your VOD.");
        });
        ui.add_space(10.0);
        if self.streams.len() > 1 {
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(RichText::new("Past streams").size(17.0).strong());
                let mut pick = None;
                for (i, s) in self.streams.iter().enumerate().take(20) {
                    let sp = crate::streamwatch::summarize(s);
                    let label = format!("{} · {} · {}{}", history::fmt_time(s.started_at), crate::streamwatch::fmt_duration(sp.duration), sp.grade, if s.bandwidth_test { " · test" } else { "" });
                    if ui.selectable_label(i == sel, label).clicked() {
                        pick = Some(i);
                    }
                }
                if let Some(i) = pick {
                    self.stream_sel = i;
                }
            });
        }
        if report {
            self.open_report(format!(
                "Post-flight for the stream at {}: {}, internet dropped {:.2}%, encoder skipped {:.2}%, rendering skipped {:.2}%, avg {:.0} kbps, {} reconnects",
                history::fmt_time(l.started_at),
                p.grade,
                p.net_frac * 100.0,
                p.enc_frac * 100.0,
                p.render_frac * 100.0,
                p.avg_kbps,
                p.reconnects
            ));
        }
    }
}

impl App {
}


fn style_label(s: Style) -> &'static str {
    match s {
        Style::Competitive => "Competitive",
        Style::Balanced => "Balanced",
        Style::Cinematic => "Cinematic",
        Style::Custom => "Custom",
    }
}

/// The app logo: a paper plane over a check mark.
fn draw_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let s = size / 128.0;
    let at = |x: f32, y: f32| egui::pos2(rect.min.x + x * s, rect.min.y + y * s);
    p.rect_filled(rect, egui::CornerRadius::same((28.0 * s) as u8), Color32::from_rgb(0x1C, 0x22, 0x2B));
    p.add(egui::Shape::convex_polygon(vec![at(40.0, 62.0), at(100.0, 32.0), at(80.0, 78.0), at(70.0, 64.0)], ACCENT, egui::Stroke::NONE));
    p.add(egui::Shape::line(vec![at(30.0, 92.0), at(54.0, 114.0), at(104.0, 66.0)], egui::Stroke::new(10.0 * s, ACCENT)));
}

#[derive(Default)]
struct TreeRow<'a> {
    text: &'a str,
    indent: f32,
    /// A branch: Some(expanded).
    branch: Option<bool>,
    /// "–" before the text: a child step.
    dash: bool,
    selected: bool,
    done: bool,
    note: &'a str,
    /// A small chevron on the right: opens a menu.
    chevron: bool,
    dot: Option<Color32>,
    strong: bool,
    small: bool,
}

/// One row of the sidebar tree. Every row is clickable and shows it on hover.
fn tree_row(ui: &mut Ui, r: TreeRow) -> egui::Response {
    let h = if r.small { 30.0 } else { 34.0 };
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let row = egui::Rect::from_min_max(egui::pos2(rect.min.x + r.indent, rect.min.y), rect.max);
    if r.selected {
        p.rect_filled(row, egui::CornerRadius::same(8), Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 26));
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: 12, spread: 0, color: Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 90) }.as_shape(egui::Rect::from_min_size(row.min, Vec2::new(3.0, row.height())), egui::CornerRadius::same(2)));
        p.rect_filled(egui::Rect::from_min_size(row.min, Vec2::new(3.0, row.height())), egui::CornerRadius::same(2), ACCENT);
    } else if resp.hovered() {
        p.rect_filled(row, egui::CornerRadius::same(8), RAISED);
    }
    if resp.has_focus() {
        p.rect_stroke(row, egui::CornerRadius::same(8), egui::Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
    }
    let cy = row.center().y;
    let mut x = row.min.x + 10.0;
    let soft = Color32::from_rgb(0xB7, 0xC0, 0xCC);
    let arrow = Color32::from_rgb(0x9A, 0xA4, 0xB2);
    if let Some(open) = r.branch {
        let pts = if open { vec![egui::pos2(x, cy - 3.0), egui::pos2(x + 5.0, cy + 2.0), egui::pos2(x + 10.0, cy - 3.0)] } else { vec![egui::pos2(x + 2.0, cy - 5.0), egui::pos2(x + 7.0, cy), egui::pos2(x + 2.0, cy + 5.0)] };
        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, soft)));
        x += 18.0;
    }
    if r.dash {
        p.text(egui::pos2(x, cy), egui::Align2::LEFT_CENTER, "–", egui::FontId::proportional(14.0), Color32::from_rgb(0x5A, 0x65, 0x77));
        x += 14.0;
    }
    if let Some(c) = r.dot {
        p.circle_filled(egui::pos2(x + 4.0, cy), 4.0, c);
        x += 14.0;
    }
    let col = if r.selected || r.strong { TEXT } else { soft };
    let font = egui::FontId::proportional(if r.small { 13.5 } else { 14.5 });
    if r.branch.is_some() {
        // Section names: upper case, spaced out.
        let mut job = egui::text::LayoutJob::simple_singleline(r.text.to_uppercase(), egui::FontId::proportional(12.5), TEXT);
        if let Some(sec) = job.sections.first_mut() {
            sec.format.extra_letter_spacing = 2.0;
        }
        let g = ui.painter().layout_job(job);
        let gy = cy - g.size().y / 2.0;
        ui.painter().galley(egui::pos2(x, gy), g.clone(), TEXT);
        ui.painter().galley(egui::pos2(x + 0.4, gy), g, TEXT);
        if !r.note.is_empty() {
            let ng = ui.painter().layout_no_wrap(r.note.to_string(), egui::FontId::proportional(11.5), arrow);
            let nw = ng.size().x;
            ui.painter().galley(egui::pos2(row.max.x - 10.0 - nw, cy - ng.size().y / 2.0), ng, arrow);
        }
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, r.text));
        return resp;
    }
    let mut right = row.max.x - 10.0;
    if r.chevron {
        p.add(egui::Shape::line(vec![egui::pos2(right - 9.0, cy - 2.0), egui::pos2(right - 5.0, cy + 2.0), egui::pos2(right - 1.0, cy - 2.0)], egui::Stroke::new(2.0, arrow)));
        right -= 16.0;
    }
    if r.done {
        p.add(egui::Shape::line(vec![egui::pos2(right - 12.0, cy), egui::pos2(right - 8.0, cy + 4.0), egui::pos2(right - 1.0, cy - 4.0)], egui::Stroke::new(2.2, GOOD)));
        right -= 18.0;
    }
    if !r.note.is_empty() {
        let g = p.layout_no_wrap(r.note.to_string(), egui::FontId::proportional(11.5), arrow);
        let nw = g.size().x;
        p.galley(egui::pos2(right - nw, cy - g.size().y / 2.0), g, arrow);
        right -= nw + 8.0;
    }
    // Text, cut with "…" if it doesn't fit.
    let room = (right - x).max(10.0);
    let mut job = egui::text::LayoutJob::simple_singleline(r.text.to_string(), font, col);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let g = ui.painter().layout_job(job);
    let gy = cy - g.size().y / 2.0;
    ui.painter().galley(egui::pos2(x, gy), g, col);
    if r.strong || r.selected {
        // A second pass 0.4 px over reads as semi-bold with the default font.
        let mut job = egui::text::LayoutJob::simple_singleline(r.text.to_string(), egui::FontId::proportional(if r.small { 13.5 } else { 14.5 }), col);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(x + 0.4, gy), g, col);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, r.text));
    resp
}

/// The small "DO THIS NEXT" label over a step's title.
fn do_next_label(ui: &mut Ui) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().rect_filled(r, egui::CornerRadius::same(2), ACCENT);
        ui.label(theme::caps("Do this next", 12.0, ACCENT));
    });
}


#[derive(Clone, Copy, PartialEq)]
enum Tick {
    /// Not yet, and not being watched yet.
    Wait,
    /// Preflight is watching for this right now.
    Watch,
    Done,
    /// Needs fixing before the test.
    Warn,
    /// Worth knowing.
    Info,
}

/// One checklist line: a circle that ticks itself, the text, a note, and any buttons.
fn tick_row(ui: &mut Ui, state: Tick, text: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(0x12, 0x16, 0x20, 200))
        .stroke(egui::Stroke::new(1.0, if state == Tick::Warn { WARN } else { Color32::from_rgba_unmultiplied(255, 255, 255, 14) }))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                let c = r.center();
                let p = ui.painter();
                match state {
                    Tick::Done => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x1F, 0x3A, 0x28));
                        p.add(egui::Shape::line(vec![egui::pos2(c.x - 4.5, c.y), egui::pos2(c.x - 1.0, c.y + 3.5), egui::pos2(c.x + 5.0, c.y - 3.5)], egui::Stroke::new(2.4, GOOD)));
                    }
                    Tick::Watch => {
                        // A slowly turning arc: "watching".
                        let t = ui.input(|i| i.time) as f32;
                        let pts: Vec<egui::Pos2> = (0..=20).map(|k| {
                            let a = t * 2.5 + k as f32 / 20.0 * 4.2;
                            egui::pos2(c.x + 9.5 * a.cos(), c.y + 9.5 * a.sin())
                        }).collect();
                        p.circle_stroke(c, 9.5, egui::Stroke::new(2.0, Color32::from_rgb(0x2A, 0x33, 0x40)));
                        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, ACCENT)));
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                    }
                    Tick::Wait => {
                        for k in 0..12 {
                            let a0 = k as f32 / 12.0 * std::f32::consts::TAU;
                            let a1 = a0 + 0.28;
                            p.line_segment([egui::pos2(c.x + 9.5 * a0.cos(), c.y + 9.5 * a0.sin()), egui::pos2(c.x + 9.5 * a1.cos(), c.y + 9.5 * a1.sin())], egui::Stroke::new(1.8, Color32::from_rgb(0x4A, 0x53, 0x61)));
                        }
                    }
                    Tick::Warn => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x3A, 0x2A, 0x10));
                        p.text(c, egui::Align2::CENTER_CENTER, "!", egui::FontId::proportional(15.0), WARN);
                    }
                    Tick::Info => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x1C, 0x2A, 0x3A));
                        p.text(c, egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(14.0), INFO);
                    }
                }
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(text).color(if state == Tick::Done { TEXT_SOFT } else { TEXT }));
                    });
                    if !sub.is_empty() {
                        ui.label(RichText::new(sub).size(13.0).color(MUTED));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state == Tick::Done {
                        ui.label(RichText::new("Done").size(13.0).color(GOOD));
                    } else if state == Tick::Watch {
                        ui.label(RichText::new("Watching…").size(13.0).color(MUTED));
                    }
                    add(ui);
                });
            });
        });
    ui.add_space(6.0);
}

#[derive(Clone, Default)]
struct ReadingPoint {
    gpu: Option<f32>,
    enc: Option<f32>,
    kbps: Option<f32>,
    skipped: Option<f32>,
}
