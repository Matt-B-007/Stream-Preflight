// SPDX-License-Identifier: GPL-3.0-or-later
//! Command-line flags shared by the family of apps:
//!
//!   --help                    list these flags
//!   --version                 print the version
//!   --self-test [--quick]     check the program works (quick: seconds, no network, no window)
//!   --report                  save a problem report and print where it went
//!   --update                  install the newest version (checked and signed)
//!   --rollback                go back to the version kept from before the last update
//!   --uninstall [--delete-settings] [--yes]
//!                             remove the program, menu entry and login entry
//!                             (settings, history and logs stay unless --delete-settings;
//!                             asks first unless --yes)
//!   --install                 put the program in place and add the menu entry (used by install.sh)
//!   --tray                    run in the tray only (see traymode.rs)
//!
//! Returns Some(exit code) when a flag was handled, None to open the window.

use crate::util;

fn has(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

/// Every flag the program understands.
const KNOWN: [&str; 15] = [
    "--help",
    "-h",
    "--version",
    "-V",
    "--self-test",
    "--quick",
    "--report",
    "--update",
    "--rollback",
    "--uninstall",
    "--delete-settings",
    "--yes",
    "--install",
    "--self-check",
    "--tray",
];

pub const HELP: &str = "Usage: stream-preflight [OPTION]

With no option, opens the Stream Preflight window.

  --help                 Show this list
  --version              Print the version
  --self-test [--quick]  Check that the program and its files are in order
  --report               Save a bug report and print where it is
  --update               Install the newest version (checksum, signature and self-test checked)
  --rollback             Go back to the version you had before the last update
  --uninstall            Remove the program and its menu entry; asks first
      --delete-settings  ...and remove settings, history and logs too
      --yes              ...without asking
  --tray                 Start in the tray, without a window";

/// The first argument the program doesn't know (file hints for --self-check are allowed).
fn unknown_flag(args: &[String]) -> Option<&String> {
    if has(args, "--self-check") {
        return None;
    }
    args.iter().skip(1).find(|a| !KNOWN.contains(&a.as_str()))
}

pub fn handle(args: &[String]) -> Option<i32> {
    if has(args, "--help") || has(args, "-h") {
        println!("{HELP}");
        return Some(0);
    }
    if let Some(bad) = unknown_flag(args) {
        eprintln!("Unknown option: {bad}\n\n{HELP}");
        return Some(2);
    }
    if has(args, "--version") || has(args, "-V") {
        println!("{}", util::VERSION);
        return Some(0);
    }
    if has(args, "--self-test") {
        return Some(if has(args, "--quick") {
            self_test_quick(true)
        } else {
            self_test_full()
        });
    }
    if has(args, "--report") {
        return Some(report());
    }
    if has(args, "--update") {
        return Some(update());
    }
    if has(args, "--rollback") {
        return Some(rollback());
    }
    // Windows "Installed apps › Uninstall" passes --uninstall too; there it opens the window's dialog.
    if has(args, "--uninstall") && !cfg!(windows) {
        return Some(uninstall(
            has(args, "--delete-settings"),
            has(args, "--yes"),
        ));
    }
    None
}

fn line(ok: bool, what: &str, detail: &str) -> bool {
    println!(
        "  {}  {what}{}",
        if ok { "ok  " } else { "FAIL" },
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    );
    ok
}

/// Can the program start, load its resources and read settings? No network,
/// no window, no external tools; a few seconds at most. Run before an update swap.
pub fn self_test_quick(header: bool) -> i32 {
    if header {
        println!("{} {} self-test (quick)", util::APP_NAME, util::VERSION);
    }
    let mut ok = true;
    // Settings: read and understood (not changed).
    let p = crate::config::Settings::path();
    ok &= match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<crate::config::Settings>(&s) {
            Ok(_) => line(true, "Settings", "read"),
            Err(e) => line(
                false,
                "Settings",
                &format!("couldn't be read ({e}); Preflight would keep a copy and start fresh"),
            ),
        },
        Err(_) => line(true, "Settings", "none yet (first start)"),
    };
    // Built-in game profiles.
    let n = crate::games::profiles().len();
    ok &= line(
        n == crate::games::profile_file_count(),
        "Game profiles",
        &format!("{n} loaded"),
    );
    // Window icon and tray pictures.
    ok &= line(
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon-256.png")).is_ok(),
        "App icon",
        "",
    );
    // Fonts are built in, so text looks the same on every PC.
    ok &= line(
        [
            crate::theme::INTER,
            crate::theme::JETBRAINS_MONO,
            crate::theme::SPACE_GROTESK,
            crate::theme::LUCIDE,
        ]
        .iter()
        .all(|f| f.len() > 1000 && (f.starts_with(&[0, 1, 0, 0]) || f.starts_with(b"OTTO"))),
        "Built-in fonts",
        "Inter, JetBrains Mono, Space Grotesk, Lucide",
    );
    // Every text colour readable (4.5:1) on every surface, dark and light.
    let contrast = crate::theme::contrast_problems();
    ok &= line(
        contrast.is_empty(),
        "Colour contrast",
        &if contrast.is_empty() {
            "dark and light themes pass".to_string()
        } else {
            contrast.join("; ")
        },
    );
    // Update keys.
    let keys = crate::update::PUBLIC_KEYS
        .iter()
        .all(|(_, k)| minisign_verify::PublicKey::from_base64(k).is_ok());
    ok &= line(
        keys,
        "Update signing keys",
        if keys { "both readable" } else { "unreadable" },
    );
    // Data folders can be found.
    let d = util::dirs();
    ok &= line(
        !d.data.as_os_str().is_empty() && !d.config.as_os_str().is_empty(),
        "App folders",
        "",
    );
    if header {
        println!(
            "{}",
            if ok {
                "Self-test passed."
            } else {
                "Self-test FAILED."
            }
        );
    }
    if ok { 0 } else { 1 }
}

/// Everything the quick test does, plus hardware, games and OBS (on this PC).
fn self_test_full() -> i32 {
    println!("{} {} self-test", util::APP_NAME, util::VERSION);
    let quick = self_test_quick(false);
    let (hw, games, obs, obs_err) = gather();
    let tests = crate::report::self_test(&crate::report::SelfTestInput {
        hw: Some(&hw),
        games: &games,
        obs: obs.as_ref(),
        obs_error: obs_err.as_deref(),
        net: None,
    });
    let mut failed = quick != 0;
    for t in &tests {
        let mark = match t.status {
            crate::report::TestStatus::Pass => "ok  ",
            crate::report::TestStatus::Warn => "note",
            crate::report::TestStatus::Fail => {
                failed = true;
                "FAIL"
            }
        };
        println!("  {mark}  {}: {}", t.name, t.detail);
    }
    println!("  (The network test only runs in the app, so it's skipped here.)");
    println!(
        "{}",
        if failed {
            "Self-test FAILED."
        } else {
            "Self-test passed."
        }
    );
    if failed { 1 } else { 0 }
}

type Gathered = (
    crate::hardware::HardwareInfo,
    Vec<crate::games::GameResult>,
    Option<crate::obs::ObsSnapshot>,
    Option<String>,
);

/// Hardware, games and OBS, read the same way the window does (no network test).
fn gather() -> Gathered {
    let st = crate::config::Settings::load();
    let hw = crate::hardware::scan();
    let mut sys = crate::hardware::new_system();
    let procs = crate::hardware::running_process_names(&mut sys);
    let running = crate::hardware::game_processes(&mut sys);
    let games = crate::games::scan(&procs, &running, &[], &mut sys);
    let pw = crate::config::load_secret();
    let (obs, err) = match crate::obs::Client::connect(&st.obs_host, st.obs_port, pw.as_deref()) {
        Ok(mut c) => (Some(crate::obs::snapshot_live(&mut c)), None),
        Err(e) => (crate::obs::snapshot_saved(), Some(e.to_string())),
    };
    (hw, games, obs, err)
}

fn report() -> i32 {
    let st = crate::config::Settings::load();
    let (hw, games, obs, obs_err) = gather();
    let analysis =
        crate::analysis::analyze(&st, Some(&hw), &games, obs.as_ref(), None, None, false);
    let selftests = crate::report::self_test(&crate::report::SelfTestInput {
        hw: Some(&hw),
        games: &games,
        obs: obs.as_ref(),
        obs_error: obs_err.as_deref(),
        net: None,
    });
    let sections = crate::report::build_sections(&crate::report::ReportInput {
        context: "report from the command line (--report)",
        description: "",
        hw: Some(&hw),
        games: &games,
        obs: obs.as_ref(),
        obs_error: obs_err.as_deref(),
        net: None,
        test: None,
        analysis: &analysis,
        selftests: &selftests,
        settings: &st,
        encoders: &[],
        extra: &[],
    });
    let id = util::short_id();
    match crate::report::save(&sections, &id) {
        Ok(p) => {
            println!("Report {id} saved: {}", p.display());
            println!(
                "Nothing is sent until you choose to. Attach this file to an issue if you'd like help."
            );
            0
        }
        Err(e) => {
            eprintln!("Couldn't save the report: {e}");
            1
        }
    }
}

fn update() -> i32 {
    let st = crate::config::Settings::load();
    crate::doh::ENABLED.store(st.dns_fallback, std::sync::atomic::Ordering::Relaxed);
    let beta =
        st.updates_beta || (crate::update::is_beta_version(util::VERSION) && !st.updates_beta_set);
    println!(
        "Checking GitHub for updates ({})...",
        if beta { "beta" } else { "stable" }
    );
    let u = match crate::update::check(beta) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    if !u.newer {
        println!("You're up to date (v{}).", u.current);
        return 0;
    }
    println!("Installing v{} (you have v{})...", u.latest, u.current);
    match crate::update::install(&u) {
        Ok(_) => {
            println!("Update installed. Restart Stream Preflight to use it.");
            println!("To go back, run: stream-preflight --rollback");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn rollback() -> i32 {
    match crate::update::rollback() {
        Ok((v, _)) => {
            println!("Went back to v{v}. Restart Stream Preflight to use it.");
            println!("The version you had is kept, so you can run --rollback again to return.");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn uninstall(delete_settings: bool, yes: bool) -> i32 {
    println!(
        "This removes Stream Preflight: the program, its menu entry and its login entry{}. OBS, your games and their settings are not touched.",
        if delete_settings {
            ", plus its settings, history and logs"
        } else {
            " (your settings, history and logs stay)"
        }
    );
    if !yes {
        use std::io::Write;
        print!("Uninstall Stream Preflight? Type yes to continue: ");
        let _ = std::io::stdout().flush();
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim().to_lowercase().as_str(), "yes" | "y") {
            println!("Nothing was removed.");
            return 1;
        }
    }
    match crate::uninstall::run_with(&[], true, delete_settings) {
        Ok(()) => {
            println!("Stream Preflight is uninstalled. Thanks for trying it.");
            0
        }
        Err(e) => {
            eprintln!("Some files couldn't be removed: {e}");
            1
        }
    }
}
