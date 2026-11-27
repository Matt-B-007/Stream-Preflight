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

mod fixes;
mod help;
mod history_page;
mod home;
mod live;
mod pcs;
mod preflight;
mod results;
mod settings;
mod setup;
mod shell;
mod state;
mod test;
mod tray;
mod widgets;

use self::home::*;
use self::results::*;
use self::setup::*;
use self::state::*;
use self::widgets::*;

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
    /// The Connect OBS page (password and how to turn on the WebSocket server).
    ConnectObs,
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
    /// The overview: Get started, then one plain answer and the latest results.
    Home,
    Setup,
    Preflight,
    Results,
    PostFlight,
    /// While you're live.
    InFlight,
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
    // Boxed: a snapshot is large, and the other messages are small.
    Obs(Option<Box<ObsSnapshot>>, Option<String>),
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
    /// Games (None when nothing changed since the last poll), what was compared, OBS.
    rx: Receiver<PollResult>,
}

type PollResult = (
    Option<Vec<GameResult>>,
    String,
    Option<ObsSnapshot>,
    Option<String>,
);

/// The uninstall window: recordings (path, size, delete?), remove the program, error.
type UninstallBox = (Vec<(String, u64, bool)>, bool, Option<String>);

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
    /// A single page of the old welcome opened from Home (2 = connect OBS, 3 = how you stream).
    setup_single: Option<u8>,
    home_games_open: bool,
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
    show_all_fixes: bool,
    test_choice_open: bool,
    settings_tab: u8,
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
    /// Started with --tray (Windows; Linux uses the tray-only mode instead).
    start_hidden: bool,
    window_known: bool,
    tray_hint_shown: bool,
    /// A stream started with the test flag still marked on: asking if it's real.
    flag_question: bool,
    /// The user said the running stream is real (flag removed).
    flag_real: bool,
    queued_fix_action: Option<String>,
    go_ready: bool,
    preview_split: f32,
    help_open: Option<usize>,
    pending_act: Option<NextAct>,
    /// The "Remove Preflight from this PC" window: recordings (path, size, delete?), remove the program, error.
    uninstall: Option<UninstallBox>,
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
    stable_confirm: bool,
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
    /// What the last poll compared (running games, settings file times), and when
    /// the next full re-read of the game files is due.
    poll_sig: String,
    poll_full_at: f64,
    /// Preflight's own use (share of all CPU, MB), measured only while Help is open.
    own_usage: Option<(f32, f32)>,
    own_sys: Option<sysinfo::System>,
    own_next: f64,
    stream_notice: Option<crate::streamwatch::StreamLog>,
    streams: Vec<crate::streamwatch::StreamLog>,
    stream_sel: usize,
}

// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        self.tray_logic(ctx);
        let t_now = ctx.input(|i| i.time);
        self.track_readings(t_now);
        if self.screen == Screen::Help && t_now >= self.own_next {
            self.own_next = t_now + 3.0;
            self.measure_own_usage();
            theme::repaint_in(ctx, 3000);
        }
        self.pair_logic(ctx, t_now);
        let now = ctx.input(|i| i.time);
        // Watch for a recording automatically while on step 2.
        if self.screen == Screen::Preflight && self.step == 2 {
            let need = self.test_handle.is_none()
                && (self.test.is_none() || self.want_new_test)
                && !self.watch_paused
                && now >= self.watch_retry_at;
            let connected = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
            if need && connected && self.scan.is_none() {
                self.start_test(ctx, false);
            }
        }
        // Light refresh on steps 2–4 so changes are noticed (not while minimized).
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        // Encoder panel: refresh every 5 seconds on Your setup and during a test.
        let measuring_now = self
            .test_handle
            .as_ref()
            .map(|h| h.phase() == Phase::Measuring)
            .unwrap_or(false);
        if let Some(rx) = &self.enc_rx
            && let Ok(list) = rx.try_recv()
        {
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
        if !minimized
            && self.enc_rx.is_none()
            && now >= self.enc_next
            && (self.screen == Screen::Results || measuring_now)
            && let Some(hw) = self.hw.clone()
        {
            let (tx, rx) = std::sync::mpsc::channel();
            self.enc_rx = Some(rx);
            self.enc_next = now + 5.0;
            let c = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::encoders::snapshot(&hw));
                c.request_repaint();
            });
        }
        let watching_change = self
            .confirm
            .as_ref()
            .map(|c| c.result.as_ref().map(|r| !r.0).unwrap_or(true))
            .unwrap_or(false);
        if !minimized
            && self.live.is_none()
            && ((self.screen == Screen::Preflight && self.step >= 2) || watching_change)
            && self
                .test_handle
                .as_ref()
                .map(|h| h.phase() != Phase::Measuring)
                .unwrap_or(true)
            && now >= self.poll_next
        {
            self.poll_next = now + 10.0;
            self.start_poll(ctx);
        }
        if self.scan.is_some()
            || self.test_handle.is_some()
            || self.poll.is_some()
            || (self.screen == Screen::Preflight && self.step >= 2)
        {
            theme::repaint_in(ctx, 1000);
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
            log::info!(
                "Window: {}",
                if self.can_hide {
                    "can hide to tray"
                } else {
                    "Wayland: minimizes instead of hiding"
                }
            );
        }
        theme::paint_backdrop(&ctx);
        if self.screen == Screen::Setup {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(32, 24)))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Centred, like a welcome screen.
                            let w = ui.available_width().min(880.0);
                            let pad = ((ui.available_width() - w) / 2.0).max(0.0);
                            let top = ((ui.available_height() - 560.0) / 2.0).clamp(0.0, 120.0);
                            ui.add_space(top);
                            ui.horizontal(|ui| {
                                ui.add_space(pad);
                                ui.vertical(|ui| {
                                    ui.set_max_width(w);
                                    self.ui_setup(ui, &ctx);
                                });
                            });
                        });
                });
            self.ui_toast(&ctx);
            return;
        }
        let folded = self.sidebar_folded(ctx.content_rect().width());
        egui::Panel::left("menu")
            .exact_size(if folded { 76.0 } else { 236.0 })
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(PANEL)
                    .inner_margin(if folded {
                        egui::Margin::symmetric(14, 14)
                    } else {
                        egui::Margin::same(16)
                    })
                    .stroke(egui::Stroke::new(1.0, BORDER)),
            )
            .show(ui, |ui| self.ui_sidebar(ui));
        egui::Panel::top("status-bar")
            .resizable(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 32,
                right: 32,
                top: 14,
                bottom: 2,
            }))
            .show(ui, |ui| self.ui_status_bar(ui));
        let strip = self.readings();
        egui::Panel::bottom("next-bar")
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(CARD)
                    .inner_margin(egui::Margin {
                        left: 24,
                        right: 14,
                        top: 10,
                        bottom: 10,
                    })
                    .outer_margin(egui::Margin {
                        left: 24,
                        right: 24,
                        top: 6,
                        bottom: 16,
                    })
                    .corner_radius(egui::CornerRadius::same(16))
                    .stroke(egui::Stroke::new(1.0, BORDER)),
            )
            .show(ui, |ui| self.ui_next_bar(ui));
        if strip.is_none() && self.screen == Screen::Preflight && self.step == 2 {
            egui::Panel::top("readings")
                .resizable(false)
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    left: 24,
                    right: 24,
                    top: 14,
                    bottom: 0,
                }))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let gap = ui.spacing().item_spacing.x;
                        let w = ((ui.available_width() - gap * 3.0) / 4.0).max(120.0);
                        for l in ["GPU", "Encoder", "Stream", "Skipped"] {
                            egui::Frame::NONE
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                                ))
                                .corner_radius(egui::CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 8))
                                .show(ui, |ui| {
                                    ui.set_width(w - 30.0);
                                    ui.label(theme::caps(
                                        l,
                                        10.5,
                                        Color32::from_rgb(0x4A, 0x55, 0x66),
                                    ));
                                    ui.label(
                                        RichText::new("–")
                                            .monospace()
                                            .color(Color32::from_rgb(0x4A, 0x55, 0x66)),
                                    );
                                });
                        }
                    });
                    ui.label(
                        RichText::new("Live readings fill in during the test.")
                            .size(12.5)
                            .color(MUTED),
                    );
                });
        }
        if let Some(tiles) =
            strip.filter(|_| matches!(self.screen, Screen::Preflight | Screen::Results))
        {
            egui::Panel::top("readings")
                .resizable(false)
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    left: 24,
                    right: 24,
                    top: 14,
                    bottom: 0,
                }))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let gap = ui.spacing().item_spacing.x;
                        let w = ((ui.available_width() - gap * 3.0) / 4.0).max(120.0);
                        for (label, value, series, col) in &tiles {
                            theme::reading_tile(ui, label, value, series, *col, w);
                        }
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(32, 22)))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt(format!("main-{:?}", self.screen))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let w = ui.available_width().min(1040.0);
                        ui.set_max_width(w);
                        self.ui_banners(ui);
                        self.ui_live_banners(ui);
                        match self.screen {
                            Screen::Home => self.ui_home(ui),
                            Screen::Preflight => self.ui_preflight(ui, &ctx),
                            Screen::Results => self.ui_results(ui),
                            Screen::PostFlight => self.ui_postflight(ui),
                            Screen::InFlight => self.ui_inflight(ui),
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
