#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod analysis;
mod app;
mod autoread;
mod config;
mod desktop;
mod encoders;
mod fleet;
mod gamefps;
mod games;
mod hardware;
mod history;
mod install;
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
    let tray_only = std::env::args().any(|a| a == "--tray");
    if tray_only && cfg!(target_os = "linux") {
        // Tray icon and OBS watcher, no window (see traymode.rs).
        logger::init_named("preflight-tray");
        logger::install_crash_handler();
        log::info!("{} {} starting in the tray", util::APP_NAME, util::VERSION);
        traymode::run();
        log::logger().flush();
        return;
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
    if std::env::args().any(|a| a == "--version") {
        println!("{}", util::VERSION);
        return;
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
    if let Err(e) = eframe::run_native(
        util::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, menu_added, open_uninstall)))),
    ) {
        log::error!("Window could not start: {e}");
        eprintln!("Stream Preflight could not open its window: {e}");
    }
    log::logger().flush();
}
