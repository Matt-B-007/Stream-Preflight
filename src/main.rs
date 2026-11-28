// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod analysis;
mod app;
mod autoread;
mod cli;
mod config;
mod desktop;
mod doh;
mod encoders;
mod fleet;
mod gamefps;
mod games;
mod hardware;
mod history;
mod icons;
mod install;
mod instance;
mod livetest;
mod logger;
mod minecraft;
mod monitors;
mod network;
mod obs;
mod pairing;
mod preview;
mod report;
mod streamwatch;
mod theme;
mod tray;
mod traymode;
mod twopc;
mod uninstall;
mod update;
mod util;
#[cfg(windows)]
mod winsys;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Command-line flags (--version, --self-test, --report, --update, --rollback,
    // --uninstall). They don't open a window or touch the window's log file.
    if let Some(code) = cli::handle(&args) {
        std::process::exit(code);
    }
    let tray_only = std::env::args().any(|a| a == "--tray");
    if tray_only && cfg!(target_os = "linux") {
        // Already covered by another tray-only copy or an open window: leave
        // their logs alone and stop here (a hand-over waits in traymode::run).
        if traymode::running_tray().is_some()
            || (instance::window_running() && std::env::var_os(instance::RESTART_ENV).is_none())
        {
            return;
        }
        // Tray icon and OBS watcher, no window (see traymode.rs).
        logger::init_named("preflight-tray");
        logger::install_crash_handler();
        log::info!("{} {} starting in the tray", util::APP_NAME, util::VERSION);
        traymode::run();
        log::logger().flush();
        return;
    }
    // One window at a time: a second launch brings the first one forward.
    // (--install and --self-check never open a window, so they skip this.)
    let windowless = args.iter().any(|a| a == "--install" || a == "--self-check");
    match if windowless {
        instance::Claim::Existing
    } else {
        instance::claim()
    } {
        instance::Claim::Existing if windowless => {}
        instance::Claim::Existing => {
            // Before the log starts, so the open window's log isn't touched.
            println!("Stream Preflight is already open; bringing it forward.");
            return;
        }
        #[cfg(unix)]
        instance::Claim::First(l) => {
            if let Some(l) = l {
                instance::hold(l);
            }
        }
        #[cfg(not(unix))]
        instance::Claim::First => {}
    }
    logger::init();
    logger::install_crash_handler();
    log::info!(
        "{} {} ({}) starting on {}",
        util::APP_NAME,
        util::VERSION,
        util::BUILD_LABEL,
        std::env::consts::OS
    );

    // Used by the one-line install commands: put Preflight in place, add the
    // menu entry (and on Windows the Start menu and Installed apps entries), no window.
    if std::env::args().any(|a| a == "--install") {
        match install::install(true) {
            Ok(p) => {
                println!(
                    "Stream Preflight {} installed: {}",
                    util::VERSION,
                    util::scrub(&p.to_string_lossy())
                );
                return;
            }
            Err(e) => {
                eprintln!("Install failed: {e}");
                std::process::exit(1);
            }
        }
    }
    if std::env::args().any(|a| a == "--self-check") {
        // Headless check used for testing builds: scan without a window.
        let hw = hardware::scan();
        let mut sys = hardware::new_system();
        let procs = hardware::running_process_names(&mut sys);
        let running = hardware::game_processes(&mut sys);
        let hints: Vec<String> = std::env::args()
            .skip_while(|a| a != "--game")
            .nth(1)
            .into_iter()
            .collect();
        let games = games::scan(&procs, &running, &hints, &mut sys);
        println!("{}", serde_json::to_string_pretty(&hw).unwrap_or_default());
        for g in games.iter().filter(|g| g.relevant()) {
            println!(
                "game: {} installed={} files={} read={} used={} auto={:?} gap={:?}",
                g.name,
                g.installed,
                g.files.len(),
                g.settings_read,
                g.settings_used,
                g.auto,
                g.read_gap
            );
            if g.auto.is_some() {
                for f in &g.files {
                    println!(
                        "  file: {} ({} settings{})",
                        f.shown_path,
                        f.settings_count,
                        f.error
                            .as_ref()
                            .map(|e| format!(", {e}"))
                            .unwrap_or_default()
                    );
                }
                for c in &g.current {
                    println!(
                        "  {}: {}{}",
                        c.setting,
                        c.value,
                        c.change_to
                            .as_ref()
                            .filter(|_| c.flagged)
                            .map(|t| format!(" -> {t}"))
                            .unwrap_or_default()
                    );
                }
                if let Some(p) = &g.preset {
                    println!("  preset: {} lower={:?}", p.value, p.lower);
                }
                println!(
                    "  fps_cap={:?} findings={:?}",
                    g.fps_cap,
                    g.findings.iter().map(|f| &f.effect).collect::<Vec<_>>()
                );
            }
        }
        println!("self-check ok");
        return;
    }

    install::tidy_after_update();
    // Only one Preflight watches OBS: a tray-only copy steps aside for the window.
    traymode::ask_tray_to_exit();
    // Linux: app menu entry and icon (so KDE/GNOME show the icon), automatically.
    let menu_added = desktop::ensure();
    // Windows "Installed apps › Uninstall" starts Preflight with --uninstall.
    let open_uninstall = std::env::args().any(|a| a == "--uninstall");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(util::APP_NAME)
            .with_app_id("stream-preflight")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(std::sync::Arc::new(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon-256.png"))
                    .unwrap_or_default(),
            )),
        ..Default::default()
    };
    logger::start_session();
    if let Err(e) = eframe::run_native(
        util::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, menu_added, open_uninstall)))),
    ) {
        log::error!("Window could not start: {e}");
        eprintln!("Stream Preflight could not open its window: {e}");
        logger::end_session();
        log::logger().flush();
        std::process::exit(1);
    }
    logger::end_session();
    log::logger().flush();
}
