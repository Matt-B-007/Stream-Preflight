// SPDX-License-Identifier: GPL-3.0-or-later
//! The Connect OBS and How you stream pages (opened from Get started).

use super::*;

pub(super) fn choice_tile(ui: &mut Ui, on: bool, title: &str, desc: &str) -> bool {
    egui::Frame::NONE
        .fill(if on { pal().accent_bg } else { pal().inset })
        .stroke(egui::Stroke::new(
            2.0,
            if on { pal().accent } else { pal().border },
        ))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(RichText::new(title).strong());
                if !desc.is_empty() {
                    ui.label(RichText::new(desc).size(12.0).color(pal().muted));
                }
            });
        })
        .response
        .interact(egui::Sense::click())
        .clicked()
}

impl App {
    /// First time: three short steps, then straight into the first test.
    pub(super) fn ui_setup(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let single = self.setup_single;
        let n = single.unwrap_or(self.setup_screen).clamp(1, 3);
        ui.horizontal(|ui| {
            draw_logo(ui, 34.0);
            ui.label(theme::caps("Stream Preflight", 16.0, pal().text));
            if single.is_some() {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (i, l) in [(3u8, "How you stream"), (2, "Connect OBS"), (1, "Welcome")] {
                    let col = if n == i {
                        pal().accent
                    } else if n > i {
                        theme::pal().success
                    } else {
                        pal().muted
                    };
                    ui.label(RichText::new(l).size(12.0).color(if n == i {
                        pal().text
                    } else {
                        pal().muted
                    }));
                    ui.label(
                        RichText::new(if n > i {
                            "done".to_string()
                        } else {
                            i.to_string()
                        })
                        .monospace()
                        .color(col),
                    );
                    if i > 1 {
                        ui.label(RichText::new("—").color(pal().muted));
                    }
                }
            });
        });
        ui.add_space(28.0);
        let mut go: Option<u8> = None;
        let mut finish = false;
        theme::hero_card(ui, |ui| {
            ui.label(theme::caps(
                &if single.is_some() {
                    "Get started".to_string()
                } else {
                    format!("Step {n} of 3")
                },
                12.0,
                pal().accent,
            ));
            match n {
                1 => {
                    ui.label(
                        RichText::new("See how your stream will run, before you go live")
                            .size(22.0)
                            .family(theme::title_family())
                            .strong(),
                    );
                    ui.label(RichText::new("Play for two minutes while OBS records. Preflight measures your game, OBS and PC, then shows the one thing to change next.").size(16.0).color(pal().text_soft));
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for (t, d) in [
                            ("A 2-minute test", "While you play, like a normal stream."),
                            ("One fix at a time", "Exactly where the setting is, and why."),
                            ("Only reads", "Never changes your settings or controls OBS. Safe with anti-cheat."),
                        ] {
                            egui::Frame::NONE.fill(pal().card).stroke(egui::Stroke::new(1.0, pal().border)).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                                ui.set_width(220.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(t).strong());
                                    ui.label(RichText::new(d).size(12.0).color(pal().text_soft));
                                });
                            });
                        }
                    });
                    theme::small(
                        ui,
                        "Nothing is sent anywhere. Your settings and results stay on this PC.",
                    );
                }
                2 => {
                    ui.label(
                        RichText::new("Connect OBS")
                            .size(22.0)
                            .family(theme::title_family())
                            .strong(),
                    );
                    ui.label(RichText::new("Preflight reads OBS through its WebSocket server. It's built into OBS; it just needs turning on.").color(pal().text_soft));
                    ui.label("1. In OBS, open Tools › WebSocket Server Settings");
                    ui.label("2. Tick Enable WebSocket server");
                    ui.label("3. Click Show Connect Info, copy the password, and paste it here");
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.password)
                                .password(true)
                                .desired_width(300.0)
                                .hint_text("OBS WebSocket password"),
                        );
                        let busy = self.conn_rx.is_some();
                        if ((r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                            || outlined(ui, if busy { "Connecting…" } else { "Connect" }).clicked())
                            && !busy
                        {
                            self.connect_obs(ctx);
                        }
                        if let Some(o) = self.obs.as_ref().filter(|o| o.connected) {
                            ui.label(
                                RichText::new(format!("Connected · OBS {}", o.obs_version))
                                    .color(theme::pal().success),
                            );
                        }
                    });
                    // On its own line so a long message wraps instead of widening the page.
                    if let (false, Some(e)) = (
                        self.obs.as_ref().map(|o| o.connected).unwrap_or(false),
                        &self.obs_error,
                    ) {
                        ui.add(egui::Label::new(RichText::new(e).color(pal().warn)).wrap());
                    }
                    ui.checkbox(
                        &mut self.st.obs_remember_password,
                        "Remember the password on this PC (stored encrypted)",
                    );
                    egui::CollapsingHeader::new("OBS runs on another PC (two-PC setup)").id_salt("setup-remote").show(ui, |ui| {
                        theme::small(ui, "Type that PC's address, and Preflight reads OBS there. You can pair the two PCs later in Settings.");
                        ui.horizontal(|ui| {
                            ui.label("Address");
                            if ui.add(egui::TextEdit::singleline(&mut self.st.obs_host).desired_width(200.0)).changed()
                                && !crate::twopc::is_local_host(&self.st.obs_host) {
                                    self.st.pc_role = "gaming".into();
                                    self.st.two_pc_asked = true;
                                }
                        });
                    });
                }
                _ => {
                    ui.label(
                        RichText::new("How do you stream?")
                            .size(22.0)
                            .family(theme::title_family())
                            .strong(),
                    );
                    ui.label(RichText::new("What matters most?").strong());
                    let mut picked: Option<Style> = None;
                    let cur = self.st.style;
                    ui.columns(3, |cols| {
                        for (i, (st, line)) in [
                            (
                                Style::Competitive,
                                "Highest frame rate. Lowers graphics first.",
                            ),
                            (
                                Style::Balanced,
                                "Smooth game and a clean stream. Upscaling first.",
                            ),
                            (
                                Style::Cinematic,
                                "Best-looking game. Caps the frame rate first.",
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let title = if st == Style::Balanced {
                                format!("{} · most people", style_label(st))
                            } else {
                                style_label(st).to_string()
                            };
                            if choice_tile(&mut cols[i], cur == st, &title, line) && cur != st {
                                picked = Some(st);
                            }
                        }
                    });
                    if let Some(st) = picked {
                        self.st.apply_style(st);
                        self.st.default_style = st;
                    }
                    ui.label(RichText::new("Where do you stream?").strong());
                    ui.horizontal(|ui| {
                        for name in ["Twitch", "YouTube", "Kick"] {
                            let mut on = self.st.platforms.iter().any(|p| p == name)
                                || (self.st.platforms.is_empty()
                                    && self.st.platform.name() == name);
                            if ui.checkbox(&mut on, name).changed() {
                                if self.st.platforms.is_empty() {
                                    self.st.platforms.push(self.st.platform.name().to_string());
                                }
                                self.st.platforms.retain(|p| p != name);
                                if on {
                                    self.st.platforms.push(name.to_string());
                                }
                                if let Some(first) = self.st.platforms.first() {
                                    self.st.platform = match first.as_str() {
                                        "YouTube" => Platform::YouTube,
                                        "Kick" => Platform::Kick,
                                        _ => Platform::Twitch,
                                    };
                                }
                            }
                        }
                    });
                    theme::small(
                        ui,
                        "Each game can have its own style later: change it from the bar at the top while that game is picked.",
                    );
                }
            }
        });
        ui.add_space(20.0);
        egui::Frame::NONE.fill(pal().card).stroke(egui::Stroke::new(1.0, pal().border)).corner_radius(egui::CornerRadius::same(16)).inner_margin(egui::Margin::symmetric(18, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if single.is_some() {
                    if outlined(ui, "Back to Home").clicked() {
                        self.setup_single = None;
                        self.screen = Screen::Home;
                    }
                } else if n > 1 && outlined(ui, "Back").clicked() {
                    go = Some(n - 1);
                }
                ui.label(RichText::new(if n == 2 { "No OBS yet? You can connect it later." } else { "You can change all of this later from the bar at the top or in Settings." }).size(12.0).color(pal().muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = match (single, n) {
                        (Some(_), _) => "Done",
                        (None, 1) => "Get started",
                        (None, 2) => "Continue",
                        _ => "Start my first pre-flight",
                    };
                    if theme::glow_button(ui, label).clicked() {
                        if single.is_some() || n == 3 { finish = true } else { go = Some(n + 1) }
                    }
                });
            });
        });
        if let Some(g) = go {
            self.setup_screen = g;
        }
        if finish {
            if self.st.obs_remember_password && !self.password.is_empty() {
                crate::config::save_secret(&self.password);
            }
            if n == 3 && !self.st.hints_seen.iter().any(|h| h == "picked_style") {
                self.st.hints_seen.push("picked_style".into());
            }
            if single.is_some() {
                // Back to Home; the Get started card ticks the step off by itself.
                self.setup_single = None;
                self.st.save();
                self.screen = Screen::Home;
                self.start_poll(ctx);
            } else {
                self.st.setup_done = true;
                self.st.save();
                self.screen = Screen::Preflight;
                self.step = 2;
                self.start_scan(ctx.clone(), true);
            }
        }
    }
}
