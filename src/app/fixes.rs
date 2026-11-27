//! Fixes (one at a time) and Ready to go live.

use super::*;

impl App {
    /// Fixes still open, and how many there were in this round.
    pub(super) fn fix_progress(&self) -> (usize, usize) {
        let open = self
            .analysis
            .issues
            .iter()
            .filter(|i| {
                !self.st.kept_issues.contains(&i.id)
                    && !self.fixed.contains(&i.id)
                    && !self.skipped.contains(&i.id)
            })
            .count();
        let total = self.fix_queue.len().max(open);
        (open, total)
    }

    /// Games to switch between: saved results, the one shown, and the one running.
    pub(super) fn switch_options(&self) -> Vec<String> {
        let mut v = self.saved_games();
        for g in [
            self.current_game(),
            self.games
                .iter()
                .find(|g| g.running)
                .map(|g| g.name.clone()),
        ]
        .into_iter()
        .flatten()
        {
            if !v.contains(&g) {
                v.push(g);
            }
        }
        v
    }

    pub(super) fn questions(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        ui.label(
            RichText::new("When your game and your stream compete for power, which wins?").strong(),
        );
        let q1 = self.st.q_game_stream();
        ui.columns(3, |c| {
            for (i, (t, d)) in [
                (
                    "My gameplay",
                    "Keep the game smooth. The stream can be a little softer.",
                ),
                ("Keep both good", "Small trade-offs on each side."),
                (
                    "My stream",
                    "Sharpest picture for viewers, even if I lose some FPS.",
                ),
            ]
            .into_iter()
            .enumerate()
            {
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
            for (i, (t, d)) in [
                ("More FPS", "Lower graphics are fine if it feels faster."),
                ("A mix", "Good-looking and smooth."),
                (
                    "Better graphics",
                    "Ray tracing and high settings, even at lower FPS.",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                if choice_tile(&mut c[i], q2 == i as u8, t, d) {
                    self.st.set_q_fps_visuals(i as u8);
                    changed = true;
                }
            }
        });
        changed
    }

    pub(super) fn monitor_picker(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        if self.monitors.is_empty() {
            theme::muted(
                ui,
                if self.scan.is_some() {
                    "Looking for your monitors…"
                } else {
                    "No monitors were detected automatically. Set your refresh rate below."
                },
            );
        }
        for m in self.monitors.clone() {
            let on = self.st.monitor_name == m.name;
            let title = format!(
                "{}{}",
                m.name,
                if m.primary { " (main display)" } else { "" }
            );
            let mut desc = format!(
                "{} × {} · running at {} Hz · supports up to {} Hz",
                m.width, m.height, m.current_hz, m.max_hz
            );
            if m.underclocked() {
                desc.push_str(&format!(
                    "\nSet to {} Hz but can do {} Hz. Change it in {}.",
                    m.current_hz,
                    m.max_hz,
                    if cfg!(windows) {
                        "Windows Settings › System › Display › Advanced display"
                    } else {
                        "your system's display settings"
                    }
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
            egui::ComboBox::from_id_salt("hz")
                .selected_text(format!("{} Hz", self.st.monitor_hz))
                .show_ui(ui, |ui| {
                    for hz in [
                        60, 75, 100, 120, 144, 165, 180, 200, 240, 280, 360, 480, 500,
                    ] {
                        if ui
                            .selectable_value(&mut self.st.monitor_hz, hz, format!("{hz} Hz"))
                            .changed()
                        {
                            self.st.monitor_auto = false;
                            changed = true;
                        }
                    }
                });
        });
        changed
    }

    pub(super) fn ui_step3(&mut self, ui: &mut Ui) {
        let mut action: Option<String> = None;
        let total = self.fix_queue.len();
        let cur = self.current_issue().cloned();
        let detected = self.detected.clone();
        let pausing = cur
            .as_ref()
            .map(|i| analysis::is_optional(i) && !self.optional_ok && detected.is_none())
            .unwrap_or(false);
        let advanced = self.st.advanced_view;
        theme::hero_card(ui, |ui| {
            ui.horizontal(|ui| {
                do_next_label(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if cur.is_some() && !pausing {
                        let n = self.fix_queue.len();
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(if self.show_all_fixes {
                                        "Hide the list".to_string()
                                    } else {
                                        format!("All fixes ({n})")
                                    })
                                    .size(13.0)
                                    .color(ACCENT),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.show_all_fixes = !self.show_all_fixes;
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
                    None => (
                        INFO,
                        RAISED,
                        format!(
                            "Waiting for {} to save your change… Press Apply in the game.",
                            cw.game
                        ),
                    ),
                };
                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
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
            if let Some(i) = cur.as_ref().filter(|_| !pausing && self.show_all_fixes) {
                let place = analysis::place_of(&i.id);
                let items: Vec<(usize, &Issue)> = self
                    .fix_queue
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| analysis::place_of(&q.id) == place)
                    .collect();
                let done = items
                    .iter()
                    .filter(|(_, q)| self.fixed.contains(&q.id))
                    .count();
                egui::CollapsingHeader::new(
                    RichText::new(format!(
                        "All fixes {} · {done} of {} done",
                        place.label().to_lowercase(),
                        items.len()
                    ))
                    .color(TEXT_SOFT),
                )
                .id_salt("fix-list")
                .default_open(true)
                .show(ui, |ui| {
                    // Keep it short: the one being fixed now and the next few. Done and skipped ones are counted.
                    let skipped_n = items
                        .iter()
                        .filter(|(_, q)| {
                            self.skipped.contains(&q.id) && !self.fixed.contains(&q.id)
                        })
                        .count();
                    let pending: Vec<(usize, &Issue)> = items
                        .iter()
                        .copied()
                        .filter(|(idx, q)| {
                            *idx == self.fix_idx
                                || (!self.fixed.contains(&q.id) && !self.skipped.contains(&q.id))
                        })
                        .collect();
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
                            ui.label(
                                RichText::new(q.title.trim_end_matches('.'))
                                    .color(if idx == self.fix_idx { TEXT } else { TEXT_SOFT }),
                            );
                        });
                        if idx != self.fix_idx
                            && r.response.interact(egui::Sense::click()).clicked()
                        {
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
                    let kept_n = self
                        .st
                        .kept_issues
                        .iter()
                        .filter(|k| analysis::place_of(k) == place)
                        .count();
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
                    if self.test.is_some() {
                        "Nothing to fix. Nice.".to_string()
                    } else {
                        "Run the test first, so Preflight can find what to fix.".to_string()
                    }
                } else {
                    format!(
                        "{} fixed{}. Setup is now {}.",
                        self.fixed.len(),
                        if self.skipped.is_empty() {
                            String::new()
                        } else {
                            format!(", {} skipped", self.skipped.len())
                        },
                        self.analysis.setup_score
                    )
                };
                ui.label(RichText::new(msg).size(19.0).strong());
                if !self.fixed.is_empty() {
                    theme::muted(
                        ui,
                        "Click Check my changes to run a short test and see the difference, before and after.",
                    );
                }
                if !self.skipped.is_empty() {
                    theme::muted(
                        ui,
                        "Skipped issues stay in Your setup, so you can come back to them.",
                    );
                }
                let kept: Vec<&Issue> = self
                    .analysis
                    .issues
                    .iter()
                    .filter(|i| self.st.kept_issues.contains(&i.id))
                    .collect();
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
                    if !self.fixed.is_empty() && theme::secondary(ui, "Check my changes").clicked()
                    {
                        action = Some("check".into());
                    }
                    if (self.test.is_some() || self.returning)
                        && theme::secondary(ui, "Ready to go live").clicked()
                    {
                        action = Some("to4".into());
                    }
                });
                return;
            };
            // Before the first optional issue: a pause, so new users can stop here.
            if pausing {
                let left = self.fix_queue[self.fix_idx..].len();
                let skipped_imp: Vec<String> = self.fix_queue[..self.fix_idx]
                    .iter()
                    .filter(|q| self.skipped.contains(&q.id) && !self.fixed.contains(&q.id))
                    .map(|q| q.id.clone())
                    .collect();
                if skipped_imp.is_empty() {
                    ui.label(
                        RichText::new("The important issues are done.")
                            .size(19.0)
                            .strong(),
                    );
                } else {
                    ui.label(RichText::new(format!("You've been through the important ones. {} skipped, so Setup stays at {}.", skipped_imp.len(), self.analysis.setup_score)).size(19.0).strong());
                    if theme::secondary(ui, "Go back to the skipped ones").clicked() {
                        action = Some("back_skipped".into());
                    }
                }
                theme::muted(
                    ui,
                    format!(
                        "{left} optional improvement{} left. They're small wins; skipping them is fine.",
                        if left == 1 { "" } else { "s" }
                    ),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "Go through them").clicked() {
                        action = Some("optional_ok".into());
                    }
                });
                return;
            }
            // One card: where, what to change (now → change to), one reason, two buttons.
            let picked = self
                .picks
                .get(&issue.id)
                .cloned()
                .or_else(|| {
                    issue
                        .options
                        .iter()
                        .find(|o| o.suggested)
                        .map(|o| o.id.clone())
                })
                .or_else(|| issue.options.first().map(|o| o.id.clone()));
            let main = issue
                .options
                .iter()
                .find(|o| Some(&o.id) == picked.as_ref())
                .cloned();
            let total = self.fix_queue.len();
            let place_name = match analysis::place_of(&issue.id) {
                analysis::Place::Game => self
                    .test
                    .as_ref()
                    .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()))
                    .unwrap_or_else(|| "your game".into()),
                analysis::Place::Obs => "OBS".into(),
                analysis::Place::Pc => "your PC".into(),
            };
            let _ = total;
            let mut where_line = format!(
                "In {place_name}{}",
                if analysis::is_optional(&issue) {
                    " · optional"
                } else {
                    ""
                }
            );
            if let Some(w) = crate::twopc::pc_of(analysis::place_of(&issue.id), self.role()) {
                where_line = format!("{w} · {where_line}");
            }
            ui.label(theme::caps(&where_line, 11.5, theme::TEAL));
            // The instruction as a sentence: "Set Recording Format to Hybrid MP4".
            let heading = match &main {
                Some(o) if o.name.split_whitespace().count() <= 3 && o.changes.len() == 1 => {
                    format!("Set {} to {}", o.changes[0].setting, o.changes[0].to)
                }
                Some(o) if o.name.split_whitespace().count() <= 3 => format!("Use {}", o.name),
                Some(o) => o.name.clone(),
                None => issue.title.clone(),
            };
            ui.label(RichText::new(heading).size(24.0).strong());
            if let Some(text) = detected {
                egui::Frame::NONE
                    .fill(GOOD_BG)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2F, 0x6B, 0x47)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(format!("Spotted it: {text}"))
                                .color(GOOD)
                                .strong(),
                        );
                        let note = if cur_measured(&issue.id) {
                            "Check my changes at the end will measure the difference."
                        } else {
                            "Re-checked automatically."
                        };
                        ui.label(RichText::new(note).size(13.0).color(TEXT_SOFT));
                    });
                return;
            }
            if issue.options.is_empty() {
                theme::muted(
                    ui,
                    "You've turned off every suggestion for this one. Restore them in Settings › You chose to keep.",
                );
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
            let game_for_fps = self
                .test
                .as_ref()
                .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()));
            let fps_known = self.test.as_ref().and_then(|t| t.game_fps).is_some()
                || game_for_fps
                    .as_ref()
                    .map(|g| self.st.game_fps_said.contains_key(g))
                    .unwrap_or(false);
            if matches!(issue.id.as_str(), "gpu_maxed" | "gpu_maybe" | "gpu_near")
                && main.as_ref().map(|o| o.id == "cap").unwrap_or(false)
                && let Some(g) = &game_for_fps
            {
                if !fps_known || self.fps_ask_open {
                    theme::inset(ui, |ui| {
                        ui.label(
                            RichText::new(format!("About what FPS do you get in {g}?")).strong(),
                        );
                        theme::small(
                            ui,
                            "Preflight couldn't measure it. Your answer makes the suggested limit fit your game.",
                        );
                        ui.horizontal_wrapped(|ui| {
                            for v in [45u32, 60, 90, 120, 144, 165, 200, 240] {
                                if ui
                                    .button(if v == 240 {
                                        "240+".to_string()
                                    } else {
                                        format!("About {v}")
                                    })
                                    .clicked()
                                {
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
                } else if let Some(t) = &self.test
                    && let Some(f) = t.game_fps
                {
                    theme::small(
                        ui,
                        format!(
                            "Measured {f:.0} FPS in {g} during the test ({}).",
                            t.game_fps_source
                        ),
                    );
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
                "gpu_maxed" | "gpu_maybe" | "gpu_near" | "bitrate_effects" | "cpu_high"
                | "heat" => {
                    "Change it in the game and Preflight notices by itself (it reads the settings file)."
                }
                "audio_balance" | "mic_quiet" | "no_game_audio" | "double_desktop_audio" => {
                    "Change it in OBS and Preflight notices by itself."
                }
                "display_capture" => {
                    "Picking a different window inside PipeWire can't be seen, so click \"I changed it\"."
                }
                "net_unsteady" | "hogs" | "disk_space" | "slow_disk" => {
                    "Preflight can't see this change. Click \"I changed it\" when you've done it."
                }
                _ => "Change it, click Apply or OK in OBS, and Preflight notices by itself.",
            };
            ui.label(RichText::new(watch).size(12.5).color(MUTED));
            // Quiet choices along the bottom; the main button is in the Next bar.
            ui.add(egui::Separator::default().spacing(8.0));
            ui.horizontal(|ui| {
                if quiet_link(ui, "Keep as is").on_hover_text("Stays listed under \"You chose to keep\" and stops counting against your score.").clicked() {
                    action = Some("keep".into());
                }
                if quiet_link(ui, "Skip for now").clicked() {
                    action = Some("skip".into());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if quiet_link(ui, "This looks wrong").on_hover_text("Saves a bug report you can look at first").clicked() {
                        action = Some("report".into());
                    }
                });
            });
            // Everything else, one click away.
            let others: Vec<_> = issue
                .options
                .iter()
                .filter(|o| Some(&o.id) != picked.as_ref())
                .cloned()
                .collect();
            egui::CollapsingHeader::new(if others.is_empty() {
                "Why?".to_string()
            } else {
                format!(
                    "Why? And {} other way{} to fix it",
                    others.len(),
                    if others.len() == 1 { "" } else { "s" }
                )
            })
            .id_salt(format!("why-{}", issue.id))
            .default_open(advanced)
            .show(ui, |ui| {
                ui.label(RichText::new(&issue.why).color(TEXT_SOFT));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Fixing it:").color(MUTED));
                    ui.label(RichText::new(analysis::impact_of(&issue.id)).color(GOOD));
                    let which = if issue.measured && self.test.is_some() {
                        "Smoothness"
                    } else {
                        "Setup"
                    };
                    ui.label(
                        RichText::new(format!(
                            "· +{} {which}",
                            analysis::points_of(issue.priority)
                        ))
                        .color(MUTED),
                    );
                    if issue.measured {
                        theme::measured_tag(ui, true);
                    }
                });
                theme::muted(ui, &issue.learn);
                theme::small(ui, &issue.who);
                glossary_chips(
                    ui,
                    &format!(
                        "{} {} {}",
                        issue.title,
                        issue.why,
                        issue
                            .options
                            .iter()
                            .map(|o| format!("{} {} {}", o.name, o.gain, o.cost))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                );
                if let Some(o) = &main
                    && link_small(ui, "Don't suggest this again")
                {
                    action = Some(format!("hide:{}", o.id));
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
                if let Some(g) = self
                    .test
                    .as_ref()
                    .and_then(|t| t.game_running.clone().or_else(|| t.game_guess.clone()))
                {
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
                if let Some(idx) = self
                    .fix_queue
                    .iter()
                    .position(|q| self.skipped.contains(&q.id) && !self.fixed.contains(&q.id))
                {
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
                    let picked =
                        self.picks.get(&i.id).cloned().or_else(|| {
                            i.options.iter().find(|o| o.suggested).map(|o| o.id.clone())
                        });
                    // Game changes: watch the settings file to confirm they saved.
                    if analysis::place_of(&i.id) == analysis::Place::Game {
                        let changes: Vec<analysis::Change> = i
                            .options
                            .iter()
                            .find(|o| Some(&o.id) == picked.as_ref())
                            .map(|o| o.changes.clone())
                            .unwrap_or_default();
                        let in_game: Vec<analysis::Change> = changes
                            .into_iter()
                            .filter(|c| {
                                !c.menu.starts_with("Steam ›")
                                    && !c.menu.starts_with("Heroic")
                                    && !c.menu.starts_with("Lutris")
                                    && !c.menu.starts_with("Bottles")
                                    && !c.menu.contains("NVIDIA app")
                                    && !c.menu.contains("AMD Software")
                                    && !c.menu.contains("Intel Graphics")
                            })
                            .collect();
                        if let Some(g) = self.target_game().map(|g| g.name.clone())
                            && !in_game.is_empty()
                        {
                            let since = self.game_file_saved(&g).unwrap_or(0);
                            self.confirm = Some(ConfirmWatch {
                                game: g,
                                changes: in_game,
                                since,
                                started: ui.ctx().input(|x| x.time),
                                result: None,
                            });
                            self.poll_next = 0.0;
                        }
                    }
                    let what = i
                        .options
                        .iter()
                        .find(|o| Some(&o.id) == picked.as_ref())
                        .map(|o| lower_first(o.name.trim_end_matches('.')))
                        .unwrap_or_else(|| "a setting".into());
                    self.note_change(format!(
                        "You changed: {what} ({})",
                        analysis::impact_of(&i.id).to_lowercase()
                    ));
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
                    let label = i
                        .options
                        .iter()
                        .find(|o| o.id == oid)
                        .map(|o| o.name.clone())
                        .unwrap_or_default();
                    self.st.hidden_options.push(HiddenOption {
                        key: format!("{}:{}", i.id, oid),
                        label,
                    });
                    self.st.save();
                    self.recompute();
                    if let Some(q) = self.fix_queue.get_mut(self.fix_idx) {
                        let st = &self.st;
                        let had = q.options.iter().any(|o| o.suggested);
                        q.options.retain(|o| !st.is_hidden(&q.id, &o.id));
                        if had
                            && !q.options.iter().any(|o| o.suggested)
                            && let Some(o) = q.options.first_mut()
                        {
                            o.suggested = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn ui_step4(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        // Problems Preflight found: (key, status, title, detail). Each can be
        // fixed in OBS (re-checked automatically) or marked "Keep as is".
        let mut problems: Vec<(&'static str, Status, String, String)> = Vec::new();
        let mut passed: Vec<String> = Vec::new();
        let o = self.obs.clone().filter(|o| o.connected);
        match &o {
            None => problems.push((
                "obs",
                Status::Unknown,
                "OBS isn't connected".into(),
                "Open OBS so Preflight can check your mic, audio and scene.".into(),
            )),
            Some(o) => {
                let mics = o.mics();
                if mics.is_empty() {
                    problems.push((
                        "mic",
                        Status::Bad,
                        "There's no microphone in OBS".into(),
                        "Sources › + › Audio Input Capture".into(),
                    ));
                } else if mics.iter().any(|m| m.muted == Some(true)) {
                    problems.push((
                        "mic",
                        Status::Bad,
                        "Your mic is muted in OBS".into(),
                        "Unmute it in the Audio Mixer".into(),
                    ));
                } else {
                    passed.push("Mic on".into());
                }
                if o.desktop_audio().is_empty() {
                    problems.push((
                        "audio",
                        Status::Warn,
                        "There's no game audio source".into(),
                        "Viewers may not hear the game".into(),
                    ));
                } else {
                    passed.push("Game audio".into());
                }
                if o.uses_display_capture() && !o.uses_game_capture() {
                    problems.push((
                        "capture",
                        Status::Warn,
                        "Your capture shows your whole screen".into(),
                        "Viewers may see Discord and notifications".into(),
                    ));
                } else if o.uses_game_capture() {
                    passed.push("Game capture".into());
                } else if o.uses_portal_capture() {
                    problems.push((
                        "capture",
                        Status::Unknown,
                        "Check your PipeWire capture shows only the game".into(),
                        "It can be a window or a whole screen; OBS doesn't say which".into(),
                    ));
                }
                match o.webcam() {
                    Some(w) if w.enabled => passed.push("Webcam visible".into()),
                    Some(_) => problems.push((
                        "cam",
                        Status::Warn,
                        "Your webcam is hidden in the scene".into(),
                        "Fine if you're streaming without a camera".into(),
                    )),
                    None => {}
                }
                if o.streaming_active || self.live.is_some() {
                    passed.push("Streaming".into());
                }
            }
        }
        if !self.other_recorders.is_empty() {
            problems.push((
                "rec",
                Status::Warn,
                format!(
                    "Other recorders are running: {}",
                    self.other_recorders.join(", ")
                ),
                "They compete with OBS for the encoder".into(),
            ));
        }
        let open: Vec<_> = problems
            .iter()
            .filter(|p| !self.st.go_kept.iter().any(|k| k == p.0))
            .cloned()
            .collect();
        let mut manual: Vec<(&'static str, String, bool)> = vec![
            (
                "dnd",
                if cfg!(windows) {
                    "Do Not Disturb is on (Windows Settings › System › Notifications)".into()
                } else {
                    "Do Not Disturb is on (system tray › Notifications)".into()
                },
                self.tick_dnd,
            ),
            (
                "title",
                "Stream title and category are set".into(),
                self.tick_title,
            ),
        ];
        if self.st.bandwidth_test_pending {
            manual.push((
                "bw",
                "I removed ?bandwidthtest=true from my stream key".into(),
                false,
            ));
        }
        let left = open.len() + manual.iter().filter(|m| !m.2).count();
        let ready = left == 0;
        self.go_ready = ready;
        // Important fixes not made yet (skipped or never reached). Always shown, never blocking.
        let skipped_important = self
            .analysis
            .issues
            .iter()
            .filter(|i| {
                !analysis::is_optional(i)
                    && !self.st.kept_issues.contains(&i.id)
                    && !self.fixed.contains(&i.id)
            })
            .count();
        let scene = o
            .as_ref()
            .map(|o| o.current_scene.clone())
            .unwrap_or_default();
        let mut action: Option<(&'static str, &'static str)> = None;

        theme::hero_card(ui, |ui| {
            ui.horizontal(|ui| {
                let live = self.live.is_some();
                ui.vertical(|ui| {
                    if live {
                        ui.label(RichText::new("YOU'RE LIVE").size(13.0).strong().color(BAD));
                    } else if self.go_ready {
                        ui.label(
                            RichText::new("YOU'RE READY")
                                .size(13.0)
                                .strong()
                                .color(GOOD),
                        );
                    } else {
                        do_next_label(ui);
                    }
                    ui.label(
                        RichText::new(if live {
                            "Enjoy the stream"
                        } else if self.go_ready {
                            "Everything's checked"
                        } else {
                            "A last look before you stream"
                        })
                        .size(24.0)
                        .strong(),
                    );
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
            let (fill, stroke) = if ready || self.live.is_some() {
                (GOOD_BG, Color32::from_rgb(0x2F, 0x6B, 0x47))
            } else {
                (CARD_DARK, BORDER)
            };
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
                go_row(
                    ui,
                    *st,
                    match st {
                        Status::Bad => "Change",
                        Status::Unknown => "Unknown",
                        _ => "Check",
                    },
                    title,
                    detail,
                    Some("Keep as is"),
                    || action = Some(("ack", key)),
                );
            }
            // Things to tick yourself (hidden once you're live)
            for (key, title, done) in manual.iter().filter(|_| self.live.is_none()) {
                if go_tick_row(ui, *done, title) {
                    action = Some(("tick", key));
                }
            }
            if skipped_important > 0 && self.live.is_none() {
                go_row(
                    ui,
                    Status::Warn,
                    "Change",
                    &format!(
                        "{skipped_important} change{} from Fix not made yet",
                        if skipped_important == 1 { "" } else { "s" }
                    ),
                    "You can still go live; making them keeps it smoother",
                    Some("Go to Fix"),
                    || action = Some(("review", "")),
                );
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
            let acked: Vec<String> = problems
                .iter()
                .filter(|p| self.st.go_kept.iter().any(|k| k == p.0))
                .map(|p| format!("{}: kept as is", short(p.0)))
                .collect();
            if !passed.is_empty() || !scene.is_empty() || !acked.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Checked by Preflight:")
                            .size(13.0)
                            .color(MUTED),
                    );
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
                if let Some(pos) = self
                    .fix_queue
                    .iter()
                    .position(|i| self.skipped.contains(&i.id) && !analysis::is_optional(i))
                {
                    self.fix_idx = pos;
                    self.skipped.retain(|id| id != &self.fix_queue[pos].id);
                    self.detected = None;
                } else if self.fix_idx >= self.fix_queue.len() {
                    self.fix_queue.clear();
                }
                self.set_step(3);
                let first_open = self
                    .fix_queue
                    .iter()
                    .position(|i| !self.fixed.contains(&i.id));
                if (self.fix_idx >= self.fix_queue.len()
                    || self
                        .fix_queue
                        .get(self.fix_idx)
                        .map(|i| self.fixed.contains(&i.id))
                        .unwrap_or(false))
                    && let Some(p) = first_open
                {
                    self.fix_idx = p;
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
            log::info!(
                "Pre-flight complete: Smoothness {} · Setup {} · {}",
                self.analysis
                    .smoothness
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "not tested".into()),
                self.analysis.setup_score,
                self.analysis.verdict
            );
            if self.st.remind_delete_recording && !self.st.pending_recordings.is_empty() {
                self.show_recording_popup = true;
            }
        } else if !ready {
            self.preflight_done = false;
        }
    }
}
