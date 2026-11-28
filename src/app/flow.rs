// SPDX-License-Identifier: GPL-3.0-or-later
//! The parts every app in the family shares: keyboard shortcuts, Ctrl+K command
//! search, the shortcuts page, What's new, and the first-run tour.

use super::*;

/// The changelog, built in: What's new shows this version's entry.
const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// Shortcuts shown on the shortcuts page and in hover hints.
pub(super) const SHORTCUTS: [(&str, &str); 9] = [
    ("Ctrl+K", "Search for a command or page"),
    ("Ctrl+,", "Settings"),
    ("?", "This list of shortcuts"),
    ("Ctrl+Q", "Quit"),
    ("Ctrl+Enter", "Do the Next step shown at the bottom"),
    ("Alt+1 … Alt+5", "Home, Test, Fixes, Live, History"),
    ("Ctrl+B", "Fold the sidebar to icons"),
    ("Esc", "Close a dialog"),
    ("↑ ↓ Enter", "Pick a result in command search"),
];

/// One entry in command search.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Cmd {
    Home,
    Test,
    Fixes,
    Live,
    History,
    Settings(u8),
    Help,
    Report,
    WhatsNew,
    Shortcuts,
    Tour,
    CheckUpdates,
    ConnectObs,
}

pub(super) const COMMANDS: [(&str, Cmd); 21] = [
    ("Home", Cmd::Home),
    ("Test: before you stream", Cmd::Test),
    ("Fixes: what to change", Cmd::Fixes),
    ("Live", Cmd::Live),
    ("History: after you stream", Cmd::History),
    ("Settings", Cmd::Settings(settings::TAB_GENERAL)),
    (
        "Settings › Appearance (dark or light)",
        Cmd::Settings(settings::TAB_APPEARANCE),
    ),
    ("Settings › Updates", Cmd::Settings(settings::TAB_UPDATES)),
    (
        "Settings › Privacy and data",
        Cmd::Settings(settings::TAB_PRIVACY),
    ),
    (
        "Settings › OBS and your PCs",
        Cmd::Settings(settings::TAB_OBS),
    ),
    (
        "Settings › Tray and notifications",
        Cmd::Settings(settings::TAB_TRAY),
    ),
    ("Settings › Advanced", Cmd::Settings(settings::TAB_ADVANCED)),
    (
        "Settings › About and uninstall",
        Cmd::Settings(settings::TAB_ABOUT),
    ),
    ("Help", Cmd::Help),
    ("Report a problem", Cmd::Report),
    ("What's new", Cmd::WhatsNew),
    ("Keyboard shortcuts", Cmd::Shortcuts),
    ("Show the tour", Cmd::Tour),
    ("Check for updates", Cmd::CheckUpdates),
    ("Connect to OBS", Cmd::ConnectObs),
    (
        "Go back to the previous version",
        Cmd::Settings(settings::TAB_UPDATES),
    ),
];

/// Commands whose name contains every word typed (any order, any case).
pub(super) fn matches(query: &str) -> Vec<(&'static str, Cmd)> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    COMMANDS
        .iter()
        .filter(|(name, _)| {
            let n = name.to_lowercase();
            words.iter().all(|w| n.contains(w.as_str()))
        })
        .copied()
        .collect()
}

/// This version's changelog entry: a one-line summary and its bullets.
pub(super) fn whats_new() -> (String, Vec<String>) {
    let mut lines = CHANGELOG
        .lines()
        .skip_while(|l| !l.starts_with("## "))
        .skip(1)
        .take_while(|l| !l.starts_with("## "));
    let mut summary = String::new();
    let mut bullets = Vec::new();
    for l in lines.by_ref() {
        let t = l.trim();
        if let Some(b) = t.strip_prefix("- ") {
            bullets.push(b.replace("**", "").replace('`', ""));
        } else if !t.is_empty() && summary.is_empty() && bullets.is_empty() {
            summary = t.replace('`', "");
        }
    }
    (summary, bullets)
}

/// The tour: 5 short steps.
pub(super) const TOUR: [(&str, &str); 5] = [
    (
        "Welcome to Stream Preflight",
        "It checks your game, OBS and PC before you go live, then explains what to change and where. It only reads settings; it never changes them.",
    ),
    (
        "The top bar",
        "Your OBS connection, the game, your style and where you stream. Click any of them to change it.",
    ),
    (
        "Test, then Fixes",
        "Test makes a short recording and measures how your stream would look. Fixes lists what to change, most important first.",
    ),
    (
        "Live and History",
        "While you stream, Live keeps an eye on dropped frames. History shows how past streams went.",
    ),
    (
        "Help is top right",
        "The ? menu has What's new, shortcuts and Report a problem. The gear opens Settings. Ctrl+K finds anything.",
    ),
];

impl App {
    /// Ctrl+K, Ctrl+, , ? and Ctrl+Q. `?` only when you're not typing.
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let (k, comma, q, question, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::K),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q),
                !typing
                    && (i.consume_key(egui::Modifiers::SHIFT, egui::Key::Questionmark)
                        || i.consume_key(egui::Modifiers::NONE, egui::Key::Questionmark)
                        || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Slash)),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if k {
            self.command_open = !self.command_open;
            self.command_query.clear();
            self.command_pick = 0;
        }
        if comma {
            self.go(Screen::Settings);
        }
        if question {
            self.shortcuts_open = true;
        }
        if q {
            log::info!("Quit from the keyboard");
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if esc {
            if self.command_open {
                self.command_open = false;
            } else if self.shortcuts_open {
                self.shortcuts_open = false;
            } else if self.whats_new_open {
                self.whats_new_open = false;
            } else if self.update_dialog {
                self.update_dialog = false;
            }
        }
    }

    pub(super) fn run_cmd(&mut self, c: Cmd) {
        match c {
            Cmd::Home => self.go(Screen::Home),
            Cmd::Test | Cmd::Fixes => {
                self.tree_pf_open = true;
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.returning = false;
                self.set_step(if c == Cmd::Test { 2 } else { 3 });
            }
            Cmd::Live => self.go(Screen::InFlight),
            Cmd::History => self.go(Screen::PostFlight),
            Cmd::Settings(t) => {
                self.settings_tab = t;
                self.go(Screen::Settings);
            }
            Cmd::Help => self.go(Screen::Help),
            Cmd::Report => self.open_report(""),
            Cmd::WhatsNew => self.whats_new_open = true,
            Cmd::Shortcuts => self.shortcuts_open = true,
            Cmd::Tour => self.tour = Some(0),
            Cmd::CheckUpdates => {
                self.settings_tab = settings::TAB_UPDATES;
                self.go(Screen::Settings);
                if self.update_rx.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let beta = self.st.updates_beta;
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::update::check(beta));
                    });
                    self.update_rx = Some(rx);
                }
            }
            Cmd::ConnectObs => self.home_act(HomeAct::ConnectObs),
        }
    }

    pub(super) fn ui_flow_windows(&mut self, ctx: &egui::Context) {
        self.handle_shortcuts(ctx);
        // The tour starts by itself only once, on a new install, after setup.
        if self.st.tour_pending && self.screen != Screen::Setup && self.st.setup_done {
            self.st.tour_pending = false;
            self.st.save();
            self.tour = Some(0);
        }
        self.ui_command(ctx);
        self.ui_shortcuts(ctx);
        self.ui_whats_new(ctx);
        self.ui_tour(ctx);
    }

    fn ui_command(&mut self, ctx: &egui::Context) {
        if !self.command_open {
            return;
        }
        let found = matches(&self.command_query);
        let (up, down, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        if down && self.command_pick + 1 < found.len() {
            self.command_pick += 1;
        }
        if up {
            self.command_pick = self.command_pick.saturating_sub(1);
        }
        self.command_pick = self.command_pick.min(found.len().saturating_sub(1));
        let mut run: Option<Cmd> = None;
        egui::Window::new("Search")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 90.0))
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::icon(crate::icons::SEARCH, 16.0).color(pal().muted));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.command_query)
                            .hint_text("Search for a page or command")
                            .desired_width(f32::INFINITY),
                    );
                    if r.changed() {
                        self.command_pick = 0;
                    }
                    r.request_focus();
                });
                ui.separator();
                if found.is_empty() {
                    theme::small(ui, "Nothing matches. Try fewer words.");
                }
                for (k, (name, cmd)) in found.iter().enumerate().take(10) {
                    let on = k == self.command_pick;
                    // Every row has the same frame, so the text lines up.
                    let r = ui.add(
                        egui::Button::new(RichText::new(*name).color(if on {
                            pal().text
                        } else {
                            pal().text_soft
                        }))
                        .fill(if on {
                            pal().accent_bg
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::NONE)
                        .frame(true)
                        .min_size(Vec2::new(ui.available_width(), 28.0)),
                    );
                    if r.clicked() {
                        run = Some(*cmd);
                    }
                }
                theme::small(ui, "↑ ↓ to pick · Enter to open · Esc to close");
            });
        if enter && let Some((_, c)) = found.get(self.command_pick) {
            run = Some(*c);
        }
        if let Some(c) = run {
            self.command_open = false;
            self.run_cmd(c);
        }
    }

    fn ui_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Keyboard shortcuts")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts")
                    .num_columns(2)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        for (keys, what) in SHORTCUTS {
                            ui.label(RichText::new(keys).monospace().color(pal().accent));
                            ui.label(what);
                            ui.end_row();
                        }
                    });
            });
        if !open {
            self.shortcuts_open = false;
        }
    }

    fn ui_whats_new(&mut self, ctx: &egui::Context) {
        if !self.whats_new_open {
            return;
        }
        let mut open = true;
        let mut tour = false;
        egui::Window::new(format!("What's new in {}", crate::util::VERSION))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(560.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let (summary, bullets) = whats_new();
                if !summary.is_empty() {
                    ui.label(RichText::new(summary).strong());
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for b in bullets {
                            ui.label(RichText::new(format!("• {b}")).color(pal().text_soft));
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "Show the tour").clicked() {
                        tour = true;
                    }
                    if ui.link("Full changelog").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(
                            "https://github.com/Matt-B-007/Stream-Preflight/blob/main/CHANGELOG.md",
                        ));
                    }
                });
            });
        if tour {
            self.whats_new_open = false;
            self.tour = Some(0);
        }
        if !open {
            self.whats_new_open = false;
        }
    }

    fn ui_tour(&mut self, ctx: &egui::Context) {
        let Some(step) = self.tour else {
            return;
        };
        let step = step.min(TOUR.len() - 1);
        let (title, body) = TOUR[step];
        let mut next = None;
        egui::Window::new("Tour")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -110.0))
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(theme::caps(
                    &format!("Step {} of {}", step + 1, TOUR.len()),
                    12.0,
                    pal().muted,
                ));
                ui.label(RichText::new(title).size(16.0).strong());
                ui.label(RichText::new(body).color(pal().text_soft));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let last = step + 1 == TOUR.len();
                    if theme::primary(ui, if last { "Done" } else { "Next" }).clicked() {
                        next = Some(if last { None } else { Some(step + 1) });
                    }
                    if step > 0 && theme::secondary(ui, "Back").clicked() {
                        next = Some(Some(step - 1));
                    }
                    if !last && ui.link("Skip the tour").clicked() {
                        next = Some(None);
                    }
                });
                theme::small(ui, "You can see it again from Help › Show the tour.");
            });
        if let Some(n) = next {
            self.tour = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_search_matches_words() {
        assert!(matches("").len() == COMMANDS.len());
        let r = matches("set upd");
        assert_eq!(
            r.first().map(|c| c.1),
            Some(Cmd::Settings(settings::TAB_UPDATES))
        );
        assert!(matches("report").iter().any(|c| c.1 == Cmd::Report));
        assert!(matches("zzz").is_empty());
    }

    #[test]
    fn whats_new_reads_this_versions_changelog() {
        let (summary, bullets) = whats_new();
        assert!(!summary.is_empty());
        assert!(bullets.len() >= 3);
        assert!(bullets.iter().all(|b| !b.contains("**")));
        // The top entry is this version.
        let top = CHANGELOG.lines().find(|l| l.starts_with("## ")).unwrap();
        assert_eq!(top.trim_start_matches("## v"), crate::util::VERSION);
    }

    #[test]
    fn tour_has_three_to_six_steps() {
        assert!((3..=6).contains(&TOUR.len()));
    }
}
