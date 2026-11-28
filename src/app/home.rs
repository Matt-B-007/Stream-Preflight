// SPDX-License-Identifier: GPL-3.0-or-later
//! Home: the Get started card, then one plain answer and the latest results.

use super::*;

// ---------------------------------------------------------------------------
// Home, the top status bar and the simple sidebar (v0.1.0-beta.5)

/// One step of the Get started card.
pub(super) struct StartStep {
    pub(super) done: bool,
    pub(super) title: &'static str,
    pub(super) sub: String,
    pub(super) button: &'static str,
    pub(super) act: HomeAct,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum HomeAct {
    PickStyle,
    ConnectObs,
    Games,
    FirstTest,
}

/// One row of the Get started card.
pub(super) fn start_row(ui: &mut Ui, n: usize, s: &StartStep, primary: bool) -> bool {
    let mut clicked = false;
    let border = if primary {
        alpha(pal().accent, 90)
    } else {
        pal().border
    };
    egui::Frame::NONE
        .fill(pal().card)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                let c = r.center();
                let p = ui.painter();
                if s.done {
                    p.circle_filled(c, 14.0, pal().success_bg);
                    p.add(egui::Shape::line(
                        vec![
                            egui::pos2(c.x - 5.0, c.y),
                            egui::pos2(c.x - 1.5, c.y + 3.5),
                            egui::pos2(c.x + 5.0, c.y - 3.5),
                        ],
                        egui::Stroke::new(2.0, theme::pal().success),
                    ));
                } else {
                    p.circle_stroke(
                        c,
                        13.0,
                        egui::Stroke::new(
                            1.5,
                            if primary {
                                pal().accent
                            } else {
                                pal().placeholder
                            },
                        ),
                    );
                    p.text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        n.to_string(),
                        egui::FontId::monospace(13.0),
                        if primary { pal().accent } else { pal().muted },
                    );
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(s.title).strong().color(if s.done {
                        pal().text_soft
                    } else {
                        pal().text
                    }));
                    ui.label(RichText::new(&s.sub).size(12.0).color(pal().muted));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if s.done {
                        ui.label(RichText::new("Done").size(12.0).color(theme::pal().success));
                    } else if primary {
                        clicked = theme::glow_button(ui, s.button).clicked();
                    } else {
                        clicked = outlined(ui, s.button).clicked();
                    }
                });
            });
        });
    clicked
}

pub(super) fn home_stat(ui: &mut Ui, label: &str, value: &str, sub: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(130.0);
        ui.label(theme::caps(label, 12.0, pal().muted));
        ui.label(
            RichText::new(value)
                .monospace()
                .size(22.0)
                .color(pal().text),
        );
        ui.label(RichText::new(sub).size(12.0).color(pal().text_soft));
    });
}

impl App {
    pub(super) fn start_steps(&self) -> Vec<StartStep> {
        let style_done =
            self.st.setup_done || self.st.hints_seen.iter().any(|h| h == "picked_style");
        let platforms = if self.st.platforms.is_empty() {
            self.st.platform.name().to_string()
        } else {
            self.st.platforms.join(" + ")
        };
        let game_running = self
            .games
            .iter()
            .find(|g| g.running)
            .map(|g| g.name.clone());
        let tested = self.test.is_some() || !self.st.game_memory.is_empty();
        vec![
            StartStep {
                done: style_done,
                title: "Pick how you stream",
                sub: format!(
                    "{} · {platforms}. Change it any time from the bar at the top.",
                    style_label(self.st.style)
                ),
                button: "Choose",
                act: HomeAct::PickStyle,
            },
            StartStep {
                done: self.obs_connected(),
                title: "Connect OBS",
                sub: "Preflight only reads OBS. It never changes your settings.".into(),
                button: "Connect OBS",
                act: HomeAct::ConnectObs,
            },
            StartStep {
                done: game_running.is_some() || tested,
                title: "Start your game",
                sub: match &game_running {
                    Some(g) => format!("{g} is running."),
                    None => "Preflight spots it by itself and tailors the advice. Any game works."
                        .into(),
                },
                button: "Which games?",
                act: HomeAct::Games,
            },
            StartStep {
                done: tested,
                title: "Run your first test",
                sub: if self.obs_connected() {
                    "About 2 minutes while you play.".into()
                } else {
                    "About 2 minutes while you play. Needs OBS connected.".into()
                },
                button: "Open Test",
                act: HomeAct::FirstTest,
            },
        ]
    }

    pub(super) fn home_act(&mut self, a: HomeAct) {
        match a {
            HomeAct::PickStyle => {
                self.setup_single = Some(3);
                self.setup_screen = 3;
                self.go(Screen::Setup);
            }
            HomeAct::ConnectObs => {
                self.setup_single = Some(2);
                self.setup_screen = 2;
                self.go(Screen::Setup);
            }
            HomeAct::Games => self.home_games_open = !self.home_games_open,
            HomeAct::FirstTest => self.pending_act = Some(NextAct::Step(2)),
        }
    }

    /// Home: the overview. Get started for new users, one plain answer after a test.
    pub(super) fn ui_home(&mut self, ui: &mut Ui) {
        let steps = self.start_steps();
        let all_done = steps.iter().all(|s| s.done);
        let hidden = self.st.hints_seen.iter().any(|h| h == "getstarted_hidden");
        let has_test = self.test.is_some();
        ui.label(theme::caps("Home", 12.0, pal().accent));
        ui.label(
            RichText::new(if has_test || !self.streams.is_empty() {
                "Welcome back"
            } else {
                "Welcome to Stream Preflight"
            })
            .size(22.0)
            .family(theme::title_family())
            .strong(),
        );
        ui.add_space(8.0);
        let mut act: Option<HomeAct> = None;
        if !all_done && !hidden {
            let done = steps.iter().filter(|s| s.done).count();
            let first_open = steps.iter().position(|s| !s.done);
            theme::hero_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps("Get started", 12.0, pal().accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if quiet_link(ui, "Hide")
                            .on_hover_text("You can still do all of this from the sidebar")
                            .clicked()
                        {
                            self.st.hints_seen.push("getstarted_hidden".into());
                            self.st.save();
                        }
                        ui.label(
                            RichText::new(format!("{done} of 4 done · do them in any order"))
                                .size(12.0)
                                .color(pal().muted),
                        );
                    });
                });
                ui.label(
                    RichText::new("Four quick things, then you'll know if your stream is ready")
                        .size(16.0)
                        .strong(),
                );
                ui.add_space(4.0);
                for (i, s) in steps.iter().enumerate() {
                    if start_row(ui, i + 1, s, Some(i) == first_open) {
                        act = Some(s.act);
                    }
                    if s.act == HomeAct::Games && self.home_games_open {
                        let names: Vec<String> = crate::games::profiles()
                            .iter()
                            .map(|p| p.name.clone())
                            .collect();
                        theme::small(
                            ui,
                            format!(
                                "Tailored advice for: {}. Any other game gets general advice from the test.",
                                names.join(", ")
                            ),
                        );
                    }
                    ui.add_space(4.0);
                }
            });
            ui.add_space(12.0);
        }
        let (imp, opt) = self.open_counts();
        // Lead with the answer once there's anything to say: a test, fixes from the scan, or earlier tests.
        if has_test || (self.obs_connected() && imp + opt > 0) || !self.st.game_memory.is_empty() {
            // Only one card is active: Get started while it's showing.
            self.home_verdict(ui, all_done || hidden);
            ui.add_space(12.0);
            self.home_panels(ui);
        } else {
            theme::card(ui, |ui| {
                ui.label(theme::caps("What Preflight does", 12.0, pal().muted));
                ui.add_space(4.0);
                ui.columns(3, |c| {
                    for (i, (icon, t, d)) in [
                        (
                            Icon::Test,
                            "Tests before you stream",
                            "Plays alongside your game and measures what viewers would see.",
                        ),
                        (
                            Icon::Fix,
                            "Tells you what to change",
                            "One fix at a time, with where to find it in OBS.",
                        ),
                        (
                            Icon::Live,
                            "Watches while you're live",
                            "Dropped frames, bitrate dips and what caused them.",
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        c[i].horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                r,
                                egui::CornerRadius::same(10),
                                alpha(pal().accent, 20),
                            );
                            paint_icon(ui.painter(), r.center(), icon, pal().accent);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(t).strong());
                                ui.label(RichText::new(d).size(12.0).color(pal().muted));
                            });
                        });
                    }
                });
                ui.add_space(4.0);
                theme::small(
                    ui,
                    "Only reads. Never changes OBS or game settings, never touches anti-cheat, sends nothing unless you choose to.",
                );
            });
            if let Some(s) = self.streams.first().cloned() {
                ui.add_space(12.0);
                self.home_last_stream(ui, &s);
            }
        }
        if let Some(a) = act {
            self.home_act(a);
        }
    }

    pub(super) fn home_verdict(&mut self, ui: &mut Ui, active: bool) {
        let (imp, opt) = self.open_counts();
        let a = &self.analysis;
        let (smooth, setup) = (a.smoothness, a.setup_score);
        let head = if imp > 0 {
            format!(
                "{imp} thing{} to fix before you go live",
                if imp == 1 { "" } else { "s" }
            )
        } else if opt > 0 {
            format!("Ready to go live · {opt} optional")
        } else {
            "Ready to go live".to_string()
        };
        let when = match self.test.as_ref() {
            Some(t) => {
                let game = t.game_running.clone().or_else(|| t.game_guess.clone());
                Some(format!(
                    "Tested {}{}.",
                    history::fmt_time(t.started_at),
                    game.map(|g| format!(" with {g}"))
                        .unwrap_or_else(|| " (no game running)".into())
                ))
            }
            None => Some(
                match self
                    .current_game()
                    .or_else(|| self.analysis.setup.game_name.clone())
                {
                    Some(g) => format!("From OBS's settings; not tested with {g} yet."),
                    None => "From OBS's settings; no test yet.".to_string(),
                },
            ),
        };
        let tested = self.test.is_some();
        let mut go: Option<NextAct> = None;
        theme::hero_card_active(ui, active, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 210.0).max(300.0));
                    ui.label(theme::caps("Your stream right now", 12.0, pal().accent));
                    ui.label(
                        RichText::new(&head)
                            .size(22.0)
                            .family(theme::title_family())
                            .strong(),
                    );
                    if let Some(w) = &when {
                        // The verdict only adds something once there's a test.
                        let v = if tested {
                            format!("{w} {}", a.verdict)
                        } else {
                            w.clone()
                        };
                        ui.add(egui::Label::new(RichText::new(v).color(pal().text_soft)).wrap());
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let main = if imp + opt > 0 {
                            "See the fixes"
                        } else {
                            "See your setup"
                        };
                        // Only the active card's main button glows.
                        let clicked = if active {
                            theme::glow_button(ui, main).clicked()
                        } else {
                            theme::primary(ui, main).clicked()
                        };
                        if clicked {
                            go = Some(if imp + opt > 0 {
                                NextAct::Step(3)
                            } else {
                                NextAct::Screen(Screen::Results)
                            });
                        }
                        if outlined(ui, if tested { "Test again" } else { "Run a test" }).clicked()
                        {
                            go = Some(NextAct::ArmTest);
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::dial(ui, Some(setup), "SETUP", pal().accent);
                    theme::dial(ui, smooth, "SMOOTHNESS", theme::pal().success);
                });
            });
        });
        if let Some(g) = go {
            self.pending_act = Some(g);
        }
    }

    pub(super) fn home_panels(&mut self, ui: &mut Ui) {
        let fixes = fix_order(&self.analysis.issues, &self.st.kept_issues);
        let mut go: Option<NextAct> = None;
        ui.columns(2, |c| {
            theme::card(&mut c[0], |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps("Top fixes", 12.0, pal().muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !fixes.is_empty() && quiet_link(ui, "All fixes").clicked() {
                            go = Some(NextAct::Step(3));
                        }
                    });
                });
                if fixes.is_empty() {
                    ui.label(RichText::new("Nothing to change.").color(pal().text_soft));
                }
                for i in fixes.iter().take(2) {
                    let col = if analysis::is_optional(i) {
                        pal().muted
                    } else {
                        theme::pal().amber
                    };
                    let r = ui.horizontal(|ui| {
                        let (d, _) =
                            ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                        ui.painter().circle_filled(d.center(), 4.0, col);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&i.title).color(pal().text));
                            ui.label(
                                RichText::new(analysis::impact_of(&i.id))
                                    .size(12.0)
                                    .color(pal().muted),
                            );
                        });
                    });
                    if ui
                        .interact(
                            r.response.rect,
                            egui::Id::unique(("homefix", &i.id)),
                            egui::Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        go = Some(NextAct::Step(3));
                    }
                }
                if fixes.len() > 2 {
                    theme::small(ui, format!("Plus {} more", fixes.len() - 2));
                }
            });
            let t = self.test.clone();
            theme::card(&mut c[1], |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps("Last test", 12.0, pal().muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if quiet_link(ui, "Your setup").clicked() {
                            go = Some(NextAct::Screen(Screen::Results));
                        }
                    });
                });
                if t.is_none() {
                    ui.label(RichText::new("No test with this game yet.").color(pal().text_soft));
                    theme::small(
                        ui,
                        "About 2 minutes while you play. It measures what viewers would see.",
                    );
                }
                if let Some(t) = &t {
                    let skipped = if t.output_total > 0 {
                        t.output_skipped as f32 * 100.0 / t.output_total as f32
                    } else {
                        0.0
                    };
                    ui.horizontal(|ui| {
                        home_stat(
                            ui,
                            "Skipped",
                            &format!("{skipped:.1}%"),
                            if skipped < 0.1 {
                                "encoder · fine"
                            } else {
                                "encoder · busy"
                            },
                        );
                        home_stat(
                            ui,
                            "GPU",
                            &t.gpu_avg
                                .map(|g| format!("{g:.0}%"))
                                .unwrap_or_else(|| "–".into()),
                            "average",
                        );
                        home_stat(
                            ui,
                            "Length",
                            &crate::streamwatch::fmt_t(t.seconds),
                            "of measuring",
                        );
                    });
                }
            });
        });
        if let Some(s) = self.streams.first().cloned() {
            ui.add_space(12.0);
            self.home_last_stream(ui, &s);
        }
        if let Some(g) = go {
            self.pending_act = Some(g);
        }
    }

    pub(super) fn home_last_stream(&mut self, ui: &mut Ui, s: &crate::streamwatch::StreamLog) {
        let p = crate::streamwatch::summarize(s);
        let mut open = false;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::caps("Last stream", 12.0, pal().muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if quiet_link(ui, "History").clicked() {
                        open = true;
                    }
                });
            });
            ui.horizontal(|ui| {
                home_stat(
                    ui,
                    "Result",
                    &p.grade,
                    &format!(
                        "{} · {}",
                        history::fmt_time(s.started_at),
                        crate::streamwatch::fmt_duration(p.duration)
                    ),
                );
                home_stat(
                    ui,
                    "Dropped",
                    &format!("{:.2}%", p.net_frac * 100.0),
                    "internet",
                );
                home_stat(ui, "Bitrate", &format!("{:.0}", p.avg_kbps), "kbps average");
                if let Some(r) = &p.recording {
                    home_stat(ui, "Recording", "On", r);
                }
            });
        });
        if open {
            self.stream_sel = 0;
            self.pending_act = Some(NextAct::Screen(Screen::PostFlight));
        }
    }
}
