// SPDX-License-Identifier: GPL-3.0-or-later
//! The page header, game switch and style handling shared by Test and Fixes.

use super::*;

impl App {
    /// "You're now playing X" banner and the "Showing: game" switcher.
    pub(super) fn ui_game_switch(&mut self, ui: &mut Ui) {
        let measuring = self
            .test_handle
            .as_ref()
            .map(|h| matches!(h.phase(), Phase::Measuring))
            .unwrap_or(false);
        let showing = self
            .test
            .as_ref()
            .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()));
        let running = self
            .games
            .iter()
            .find(|g| g.running)
            .map(|g| g.name.clone());
        let saved = self.saved_games();
        let mut act: Option<(u8, String)> = None;
        if let (Some(r), Some(sh)) = (&running, &showing)
            && r != sh
            && !measuring
            && self.switch_dismissed.as_ref() != Some(r)
        {
            egui::Frame::NONE
                .fill(CARD)
                .stroke(egui::Stroke::new(1.5, ACCENT))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!(
                            "You're now playing {r}. Your results are for {sh}."
                        ))
                        .size(17.0)
                        .strong(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if theme::secondary(ui, &format!("Test {r}")).clicked() {
                            act = Some((1, r.clone()));
                        }
                        if saved.contains(r)
                            && theme::secondary(ui, &format!("Show {r}'s last results")).clicked()
                        {
                            act = Some((2, r.clone()));
                        }
                        if theme::secondary(ui, &format!("Keep showing {sh}")).clicked() {
                            act = Some((3, r.clone()));
                        }
                    });
                });
            ui.add_space(10.0);
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

    // -----------------------------------------------------------------------
    // First-run setup: 3 short screens

    pub(super) fn on_style_changed(&mut self, ctx: &egui::Context) {
        let before: Option<String> = self
            .analysis
            .issues
            .iter()
            .find(|i| i.id.starts_with("gpu_"))
            .and_then(|i| i.options.iter().find(|o| o.suggested))
            .map(|o| o.name.clone());
        if let Some(g) = self.current_game() {
            self.st.style_by_game.insert(g, self.st.style);
        }
        self.st.save();
        self.picks.clear();
        self.recompute();
        let cur_id = self.fix_queue.get(self.fix_idx).map(|i| i.id.clone());
        self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
        self.fix_idx = cur_id
            .and_then(|id| self.fix_queue.iter().position(|i| i.id == id))
            .unwrap_or(0);
        let after: Option<String> = self
            .analysis
            .issues
            .iter()
            .find(|i| i.id.starts_with("gpu_"))
            .and_then(|i| i.options.iter().find(|o| o.suggested))
            .map(|o| o.name.clone());
        let name = match self.st.style {
            Style::Competitive => "Competitive",
            Style::Balanced => "Balanced",
            Style::Cinematic => "Cinematic",
            Style::Custom => "Custom",
        };
        let msg = match (&before, &after) {
            (_, None) => format!(
                "Style: {name}. Your GPU has room, so there's no game change to suggest right now."
            ),
            (Some(b), Some(a)) if b == a => format!(
                "Style: {name}. The first fix stays the same: {}.",
                lower_first(a.trim_end_matches('.'))
            ),
            (_, Some(a)) => format!(
                "Style: {name}. First fix is now: {}.",
                lower_first(a.trim_end_matches('.'))
            ),
        };
        log::info!("{msg}");
        self.toast(ctx, msg);
    }

    // -----------------------------------------------------------------------
    // Pre-flight

    pub(super) fn ui_preflight(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.step < 2 {
            self.step = 2;
        }
        self.page_header(ui, "Pre-flight");
        // Nothing is locked: without OBS, a note with a button, and the page stays readable.
        if !self.obs_connected() && self.step <= 3 {
            let mut open = false;
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(0xFF, 0xB5, 0x47, 18))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xB5, 0x47, 100)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_max_width((ui.available_width() - 200.0).max(240.0));
                            ui.label(RichText::new("Connect OBS first").strong());
                            ui.add(egui::Label::new(RichText::new(if self.step == 2 { "The test measures OBS while it records. You can still look around and read how it works." } else { "Fixes come from OBS's settings and a test. You can still read what's here." }).size(13.0).color(TEXT_SOFT)).wrap());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if outlined(ui, "Connect OBS").clicked() {
                                open = true;
                            }
                        });
                    });
                });
            ui.add_space(8.0);
            if open {
                self.home_act(HomeAct::ConnectObs);
            }
        }
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
    pub(super) fn page_header(&mut self, ui: &mut Ui, title: &str) {
        let (left, total) = self.fix_progress();
        let first_time = self.test.is_none() && self.st.game_memory.is_empty();
        let important = self
            .analysis
            .issues
            .iter()
            .filter(|i| {
                !analysis::is_optional(i)
                    && !self.st.kept_issues.contains(&i.id)
                    && !self.fixed.contains(&i.id)
            })
            .count();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let (kick, head) = match self.step {
                    2 if first_time => ("Test · before you stream".to_string(), "Your first test".to_string()),
                    2 => ("Test · before you stream".to_string(), "Test while you play".to_string()),
                    3 if left > 0 => (format!("Fixes · {} of {}", (total - left + 1).min(total), total), "Fix what matters".to_string()),
                    3 => ("Fixes".to_string(), "All fixes done".to_string()),
                    4 if self.live.is_some() => ("Home · you're live".to_string(), "Enjoy the stream".to_string()),
                    4 => ("Home · before you go live".to_string(), "Ready to go live".to_string()),
                    _ => ("Pre-flight".to_string(), title.to_string()),
                };
                ui.label(theme::caps(&kick, 12.0, ACCENT));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(head).size(30.0).strong().color(TEXT));
                    // One verdict, in words.
                    let (v, c) = if important > 0 {
                        (format!("{important} to fix"), theme::AMBER)
                    } else if self.test.is_none() && !self.returning {
                        ("Not tested yet".to_string(), MUTED)
                    } else if left > 0 {
                        (format!("Ready · {left} optional"), theme::TEAL)
                    } else {
                        ("Ready".to_string(), theme::TEAL)
                    };
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 26))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 140)))
                        .corner_radius(egui::CornerRadius::same(255))
                        .inner_margin(egui::Margin::symmetric(12, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new(v).size(13.0).color(c));
                        });
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                let why = ui.add(egui::Button::new(theme::caps("Why?", 12.0, ACCENT)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand);
                egui::Popup::from_toggle_button_response(&why).align(egui::RectAlign::BOTTOM_END).width(380.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.label(RichText::new("Smoothness").strong());
                    theme::small(ui, "How smoothly your game and stream ran in the test. Measured, so it needs a test.");
                    ui.label(RichText::new("Setup").strong());
                    theme::small(ui, "How well your OBS, game and PC settings suit streaming. Read from your settings.");
                    ui.separator();
                    self.ui_score_why(ui);
                });
                theme::dial(ui, Some(self.analysis.setup_score), "Setup", ACCENT);
                let sm = self.analysis.smoothness;
                theme::dial(ui, sm, "Smoothness", sm.map(theme::score_color).unwrap_or(MUTED));
            });
        });
        ui.add_space(12.0);
        // One-time hint on the very first test.
        if self.step == 2 && first_time && !self.st.hints_seen.iter().any(|h| h == "first_test") {
            let mut got = false;
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(0x5E, 0xF0, 0xB5, 20))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x5E, 0xF0, 0xB5, 100)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(14, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(theme::caps("First time", 11.5, theme::TEAL));
                        ui.label("Start your game and get into real gameplay, then click Start Recording in OBS. Preflight does the rest.");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if quiet_link(ui, "Got it").clicked() {
                                got = true;
                            }
                        });
                    });
                });
            if got {
                self.st.hints_seen.push("first_test".into());
                self.st.save();
            }
            ui.add_space(10.0);
        }
    }

    pub(super) fn ui_score_why(&self, ui: &mut Ui) {
        ui.label(RichText::new(&self.analysis.verdict).strong());
        ui.add_space(4.0);
        let mut open: Vec<&Issue> = self
            .analysis
            .issues
            .iter()
            .filter(|i| !self.st.kept_issues.contains(&i.id) && !analysis::unscored(&i.id))
            .collect();
        open.sort_by_key(|i| -analysis::points_of(i.priority));
        let tested = self.analysis.smoothness.is_some();
        ui.label(RichText::new("Smoothness: what the test measured").strong());
        if !tested {
            theme::muted(ui, "Run a test while you play to measure it.");
        } else {
            let m: Vec<&&Issue> = open.iter().filter(|i| i.measured).collect();
            if m.is_empty() {
                theme::muted(
                    ui,
                    "Nothing. No stutter, skipped frames or overload in the test.",
                );
            }
            for i in m {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("-{}", analysis::points_of(i.priority)))
                            .monospace()
                            .color(WARN),
                    );
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
                ui.label(
                    RichText::new(format!("-{}", analysis::points_of(i.priority)))
                        .monospace()
                        .color(WARN),
                );
                ui.label(&i.title);
            });
        }
        let kept: Vec<&Issue> = self
            .analysis
            .issues
            .iter()
            .filter(|i| self.st.kept_issues.contains(&i.id))
            .collect();
        if !kept.is_empty() {
            theme::small(
                ui,
                format!(
                    "Not counted (you chose to keep): {}",
                    kept.iter()
                        .map(|i| i.title.trim_end_matches('.'))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
        }
        let unknown = self
            .analysis
            .checks
            .iter()
            .filter(|c| c.status == Status::Unknown)
            .count();
        if unknown > 0 {
            theme::small(
                ui,
                format!(
                    "-{} for {unknown} check{} that couldn't run.",
                    unknown * 3,
                    if unknown == 1 { "" } else { "s" }
                ),
            );
        }
    }
}
