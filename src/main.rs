#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod analysis;
mod autoread;
mod encoders;
mod fleet;
mod minecraft;
mod app;
mod config;
mod desktop;
mod gamefps;
mod games;
mod hardware;
mod history;
mod livetest;
mod logger;
mod monitors;
mod network;
mod obs;
mod preview;
mod report;
mod streamwatch;
mod theme;
mod tray;
mod twopc;
mod pairing;
mod install;
mod uninstall;
mod update;
mod util;
#[cfg(windows)]
mod winsys;

fn main() {
    logger::init();
    logger::install_crash_handler();
    log::info!("{} {} ({}) starting on {}", util::APP_NAME, util::VERSION, util::BUILD_LABEL, std::env::consts::OS);

    if std::env::args().any(|a| a == "--self-check") {
        // Headless check used for testing builds: scan without a window.
        let hw = hardware::scan();
        let mut sys = hardware::new_system();
        let procs = hardware::running_process_names(&mut sys);
        let running = hardware::game_processes(&mut sys);
        let hints: Vec<String> = std::env::args().skip_while(|a| a != "--game").nth(1).into_iter().collect();
        let games = games::scan(&procs, &running, &hints, &mut sys);
        println!("{}", serde_json::to_string_pretty(&hw).unwrap_or_default());
        for g in games.iter().filter(|g| g.relevant()) {
            println!("game: {} installed={} files={} read={} used={} auto={:?} gap={:?}", g.name, g.installed, g.files.len(), g.settings_read, g.settings_used, g.auto, g.read_gap);
            if g.auto.is_some() {
                for f in &g.files {
                    println!("  file: {} ({} settings{})", f.shown_path, f.settings_count, f.error.as_ref().map(|e| format!(", {e}")).unwrap_or_default());
                }
                for c in &g.current {
                    println!("  {}: {}{}", c.setting, c.value, c.change_to.as_ref().filter(|_| c.flagged).map(|t| format!(" -> {t}")).unwrap_or_default());
                }
                if let Some(p) = &g.preset {
                    println!("  preset: {} lower={:?}", p.value, p.lower);
                }
                println!("  fps_cap={:?} findings={:?}", g.fps_cap, g.findings.iter().map(|f| &f.effect).collect::<Vec<_>>());
            }
        }
        println!("self-check ok");
        return;
    }

    install::tidy_after_update();
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
            .with_icon(std::sync::Arc::new(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon-256.png")).unwrap_or_default())),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(util::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, menu_added, open_uninstall))))) {
        log::error!("Window could not start: {e}");
        eprintln!("Stream Preflight could not open its window: {e}");
    }
    log::logger().flush();
}
