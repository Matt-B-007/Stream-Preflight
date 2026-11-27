//! Your setup: every setting side by side.

use super::*;

/// One "Your setup" table: Setting · Now · Suggested · Why.
/// A setup section: a header you can fold, then one plain row per setting:
/// name, what it is now, a status, and "Fix this" where something should change.
pub(super) fn setup_table(
    ui: &mut Ui,
    title: &str,
    rows: &[analysis::SetupRow],
    note: Option<&str>,
    fixed: &[String],
    act: &mut Option<String>,
) {
    use analysis::Tag;
    let changes = rows.iter().filter(|r| r.tag == Tag::Change).count();
    let optional = rows.iter().filter(|r| r.tag == Tag::Optional).count();
    let sub = if changes > 0 {
        format!("{changes} to change")
    } else if optional > 0 {
        format!("All good · {optional} optional")
    } else if rows.is_empty() {
        String::new()
    } else {
        "All good".into()
    };
    let open_default = changes > 0 || optional > 0 || title.starts_with("In OBS");
    setup_section(
        ui,
        title,
        &sub,
        if changes > 0 { WARN } else { GOOD },
        open_default,
        |ui| {
            if let Some(n) = note {
                theme::small(ui, n);
                ui.add_space(4.0);
            }
            let mut sorted: Vec<&analysis::SetupRow> = rows.iter().collect();
            sorted.sort_by_key(|r| match r.tag {
                Tag::Change => 0,
                Tag::Optional => 1,
                Tag::Kept => 2,
                Tag::Keep => 3,
            });
            for r in sorted {
                ui.add(egui::Separator::default().spacing(10.0));
                let total = ui.available_width();
                let name_w = 170.0_f32.min(total * 0.3);
                let right_w = (total * 0.4).clamp(200.0, 420.0);
                let now_w =
                    (total - name_w - right_w - ui.spacing().item_spacing.x * 2.0 - 6.0).max(80.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(name_w, 22.0),
                        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
                        |ui| {
                            ui.set_width(name_w);
                            ui.label(RichText::new(&r.setting).color(MUTED))
                                .on_hover_text(&r.why);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(now_w, 22.0),
                        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
                        |ui| {
                            ui.set_width(now_w);
                            ui.add(egui::Label::new(RichText::new(&r.now)).wrap());
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, 22.0),
                        egui::Layout::right_to_left(egui::Align::Min),
                        |ui| {
                            ui.set_width(right_w);
                            if let Some(id) = &r.issue_id {
                                match r.tag {
                                    Tag::Change | Tag::Optional if fixed.contains(id) => {
                                        ui.label(RichText::new("Changed").size(13.0).color(GOOD))
                                            .on_hover_text("Check my changes confirms it.");
                                    }
                                    Tag::Change | Tag::Optional => {
                                        if outlined(ui, "Fix this").clicked() {
                                            *act = Some(format!("fix:{id}"));
                                        }
                                    }
                                    Tag::Kept => {
                                        if outlined(ui, "Fix it after all").clicked() {
                                            *act = Some(format!("unkeep:{id}"));
                                        }
                                    }
                                    Tag::Keep => {}
                                }
                            }
                            let (txt, col) = match r.tag {
                                Tag::Keep => ("Good".to_string(), GOOD),
                                Tag::Change => (r.suggested.clone(), WARN),
                                Tag::Optional => (format!("Optional: {}", r.suggested), INFO),
                                Tag::Kept => ("Kept as is".to_string(), MUTED),
                            };
                            ui.add(
                                egui::Label::new(RichText::new(txt).color(col))
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                            )
                            .on_hover_text(&r.why);
                        },
                    );
                });
                if matches!(r.tag, Tag::Change | Tag::Optional) {
                    ui.label(
                        RichText::new(if r.menu.is_empty() {
                            r.why.clone()
                        } else {
                            format!("{} · {}", r.why, r.menu)
                        })
                        .size(12.5)
                        .color(MUTED),
                    );
                }
            }
        },
    );
}

/// A card with a header that folds: chevron, title, and a short note on the right.
pub(super) fn setup_section(
    ui: &mut Ui,
    title: &str,
    sub: &str,
    sub_col: Color32,
    open_default: bool,
    body: impl FnOnce(&mut Ui),
) {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(20, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.make_persistent_id(("setup-sec", title));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                open_default,
            );
            let open = state.is_open();
            let head = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                let c = r.center();
                let pts = if open {
                    vec![
                        egui::pos2(c.x - 5.0, c.y - 2.5),
                        egui::pos2(c.x, c.y + 2.5),
                        egui::pos2(c.x + 5.0, c.y - 2.5),
                    ]
                } else {
                    vec![
                        egui::pos2(c.x - 2.5, c.y - 5.0),
                        egui::pos2(c.x + 2.5, c.y),
                        egui::pos2(c.x - 2.5, c.y + 5.0),
                    ]
                };
                ui.painter()
                    .add(egui::Shape::line(pts, egui::Stroke::new(2.0, TEXT_SOFT)));
                ui.label(RichText::new(title).size(18.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(sub).color(sub_col));
                });
            });
            let r = head
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() {
                state.toggle(ui);
            }
            state.show_body_unindented(ui, |ui| {
                ui.add_space(4.0);
                body(ui);
            });
        });
    ui.add_space(10.0);
}

// A small score with its name, e.g. "Smoothness 92". Words carry the
// meaning too, so colour is never the only signal.

/// "FPS limit  ·  Now 240  →  Change to 120", with where to find it.
pub(super) fn change_row(ui: &mut Ui, c: &analysis::Change) {
    theme::inset(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&c.setting).strong());
            ui.label(RichText::new(&c.menu).size(12.5).color(MUTED));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::readout(ui, "Now", c.now.as_deref().unwrap_or("?"), false);
            let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 40.0), egui::Sense::hover());
            let y = r.max.y - 18.0;
            let p = ui.painter();
            let st = egui::Stroke::new(2.0, ACCENT);
            p.line_segment(
                [egui::pos2(r.min.x + 6.0, y), egui::pos2(r.max.x - 6.0, y)],
                st,
            );
            p.line_segment(
                [
                    egui::pos2(r.max.x - 13.0, y - 6.0),
                    egui::pos2(r.max.x - 6.0, y),
                ],
                st,
            );
            p.line_segment(
                [
                    egui::pos2(r.max.x - 13.0, y + 6.0),
                    egui::pos2(r.max.x - 6.0, y),
                ],
                st,
            );
            theme::readout(ui, "Change to", &c.to, true);
        });
    });
}

impl App {
    // -----------------------------------------------------------------------
    // Results

    pub(super) fn ui_results(&mut self, ui: &mut Ui) {
        let a = self.analysis.clone();
        let mut report: Option<String> = None;
        let go_pf = false;
        let mut act: Option<String> = None;
        let mut rescan = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your setup").size(26.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.scan.is_some() {
                    ui.spinner();
                } else if ui
                    .add(egui::Button::new(RichText::new("Read again").color(ACCENT)).frame(false))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    rescan = true;
                }
                if let Some(t) = self.last_scan {
                    ui.label(RichText::new(format!("Read {}", history::fmt_time(t))).color(MUTED));
                }
            });
        });
        ui.add_space(6.0);
        let all = a
            .setup
            .obs
            .iter()
            .chain(a.setup.game.iter())
            .chain(a.setup.pc.iter());
        let changes = all
            .clone()
            .filter(|r| r.tag == analysis::Tag::Change)
            .count();
        let good = all.filter(|r| r.tag == analysis::Tag::Keep).count();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(changes.to_string()).strong());
            ui.label(
                RichText::new(if changes == 1 {
                    "thing to change ·"
                } else {
                    "things to change ·"
                })
                .color(TEXT_SOFT),
            );
            ui.label(RichText::new(good.to_string()).strong());
            ui.label(RichText::new("look good ·").color(TEXT_SOFT));
            ui.label(
                RichText::new("\"Fix this\" shows you how. Preflight never changes your settings.")
                    .color(MUTED),
            );
        });
        if let Some(c) = &self.compare {
            let sk = |r: &TestResult| {
                if r.output_total > 0 {
                    r.output_skipped as f32 * 100.0 / r.output_total as f32
                } else {
                    0.0
                }
            };
            ui.label(
                RichText::new(format!(
                    "Checked after your changes: Smoothness {} to {} · GPU maxed out {:.0}% to {:.0}% of the test · skipped frames {:.2}% to {:.2}%",
                    c.before_score,
                    c.after_score,
                    maxed_pct(&c.before),
                    maxed_pct(&c.after),
                    sk(&c.before),
                    sk(&c.after)
                ))
                .color(GOOD),
            );
        }
        if self.test.is_none() {
            theme::muted(
                ui,
                "Based on your settings so far. Run the test while you play to measure it.",
            );
        }
        ui.horizontal(|ui| {
            if !self.fixed.is_empty() && self.test.is_some() && link_small(ui, "Check my changes") {
                act = Some("check".into());
            }
            if link_small(ui, "Copy as text (no names, keys or scenes)") {
                ui.ctx().copy_text(analysis::setup_text(&a));
                act = Some("copied".into());
            }
        });
        if rescan {
            self.start_scan(ui.ctx().clone(), true);
        }
        ui.add_space(10.0);
        self.ui_two_pc_section(ui);
        setup_table(ui, "In OBS", &a.setup.obs, None, &self.fixed, &mut act);
        if let Some(g) = &a.setup.game_name {
            // Settings file status first, then any note, then "a report helps".
            let mut lines: Vec<String> = Vec::new();
            if let Some(f) = &a.setup.game_file {
                lines.push(f.clone());
            }
            if let Some(c) = &a.setup.game_coverage {
                lines.push(c.clone());
            }
            if let Some(n) = &a.setup.game_note {
                lines.push(n.clone());
            }
            if a.setup.report_helps {
                lines.push("Preflight doesn't fully understand this game's settings yet. A bug report (Help › Report a problem) includes the settings it found and helps fix it.".into());
            }
            let note = if lines.is_empty() {
                None
            } else {
                Some(lines.join("\n"))
            };
            setup_table(
                ui,
                &format!("In your game · {g}"),
                &a.setup.game,
                note.as_deref(),
                &self.fixed,
                &mut act,
            );
        }
        if !a.setup.pc.is_empty() {
            setup_table(ui, "On your PC", &a.setup.pc, None, &self.fixed, &mut act);
        }
        // One setup for all your games.
        if !self.fleet.tested.is_empty() {
            let f = self.fleet.clone();
            let sub = format!(
                "{} tested game{}",
                f.tested.len(),
                if f.tested.len() == 1 { "" } else { "s" }
            );
            setup_section(
                ui,
                "One setup for all your games",
                &sub,
                if f.warning.is_some() { WARN } else { MUTED },
                f.warning.is_some(),
                |ui| {
                    let names: Vec<String> = f.tested.iter().map(|l| l.game.clone()).collect();
                    ui.label(format!(
                        "Tested with {} game{}: {}.",
                        names.len(),
                        if names.len() == 1 { "" } else { "s" },
                        names.join(", ")
                    ));
                    if !f.setup.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("OBS setup that suits all of them:").color(MUTED),
                            );
                            ui.label(RichText::new(&f.setup).strong());
                            if let Some(h) = &f.heaviest {
                                ui.label(
                                    RichText::new(format!("(the limit is set by {h})"))
                                        .color(MUTED),
                                );
                            }
                        });
                    }
                    if let Some(w) = &f.warning {
                        ui.label(RichText::new(format!("{w} Keep one setup and change the game instead of switching OBS settings per game.")).color(WARN));
                    }
                    for l in &f.tested {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&l.game).strong());
                            ui.label(
                                RichText::new(
                                    format!(
                                        "GPU {}{}",
                                        l.gpu_avg
                                            .map(|g| format!("{g:.0}% avg"))
                                            .unwrap_or_else(|| "not read".into()),
                                        if l.maxed_pct >= 1.0 {
                                            format!(", maxed {:.0}% of the time", l.maxed_pct)
                                        } else {
                                            String::new()
                                        }
                                    ) + &if l.skipped_pct > 0.1 {
                                        format!(", {:.1}% frames skipped", l.skipped_pct)
                                    } else {
                                        String::new()
                                    },
                                )
                                .color(MUTED),
                            );
                            ui.label(RichText::new(&l.verdict).color(if l.needs_room {
                                WARN
                            } else {
                                GOOD
                            }));
                        });
                    }
                    if !f.untested.is_empty() {
                        theme::small(
                            ui,
                            format!(
                                "Not checked yet: {}. Test them once and Preflight includes them.",
                                f.untested.join(", ")
                            ),
                        );
                    }
                },
            );
        }
        // Every video encoder on the PC, what it can do, and who's using it.
        if !self.encoders.is_empty() {
            let busy: usize = self.encoders.iter().filter(|e| e.in_use()).count();
            let sub = if busy == 0 {
                "None in use right now".to_string()
            } else {
                format!("{busy} in use")
            };
            setup_section(
                ui,
                "Video encoders",
                &sub,
                if busy > 0 { ACCENT } else { MUTED },
                false,
                |ui| {
                    egui::Grid::new("encoders")
                        .num_columns(3)
                        .spacing([18.0, 6.0])
                        .striped(false)
                        .show(ui, |ui| {
                            for e in &self.encoders {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&e.kind).strong());
                                    theme::small(ui, &e.gpu);
                                });
                                ui.vertical(|ui| {
                                    ui.label(format!(
                                        "{} chip{} · {}",
                                        e.chips.unwrap_or(1),
                                        if e.chips.unwrap_or(1) == 1 { "" } else { "s" },
                                        e.codecs.join(", ")
                                    ));
                                });
                                ui.label(RichText::new(e.use_text()).color(if e.in_use() {
                                    ACCENT
                                } else {
                                    MUTED
                                }));
                                ui.end_row();
                            }
                        });
                    if let Some(t) = self.test.as_ref().filter(|t| !t.encodes_seen.is_empty()) {
                        theme::small(
                            ui,
                            format!(
                                "During your last test: {}",
                                t.encodes_seen
                                    .iter()
                                    .map(|s| format!("{} {} {}", s.app, s.codec, s.res)
                                        .trim()
                                        .to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        );
                    }
                },
            );
        }
        theme::inset(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("What viewers will see:").color(MUTED));
                ui.label(RichText::new(&a.current_output).strong());
                if a.recommended_output != a.current_output && a.recommended_output != "—" {
                    ui.label(
                        RichText::new(format!("(suggested: {})", a.recommended_output))
                            .color(ACCENT),
                    );
                }
                if self.obs.as_ref().and_then(|o| o.multitrack) == Some(true) {
                    ui.label(
                        RichText::new("Enhanced Broadcasting picks the versions and bitrates.")
                            .color(MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "{} allows up to {} kbps for 1080p60.",
                            self.st.platform.name(),
                            crate::network::limits(self.st.platform).max_kbps_1080p60
                        ))
                        .color(MUTED),
                    );
                }
            });
        });
        // What the scan found: hardware, OBS connection, games, internet.
        let items = self.scan_items();
        let warn_n = items
            .iter()
            .filter(|i| matches!(i.0, Status::Warn | Status::Bad))
            .count();
        let mut scan_act: Option<NextAct> = None;
        setup_section(
            ui,
            "PC and connections",
            &if warn_n == 0 {
                "All fine".to_string()
            } else {
                format!("{warn_n} to check")
            },
            if warn_n == 0 { GOOD } else { WARN },
            warn_n > 0,
            |ui| {
                for (st, label, name, detail) in &items {
                    ui.add(egui::Separator::default().spacing(10.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(170.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_width(170.0);
                                ui.label(RichText::new(*name).color(MUTED));
                            },
                        );
                        ui.label(RichText::new(label).color(theme::status_colors(*st).0));
                        if *name == "OBS"
                            && let Some((l, a)) = err_action(detail)
                            && outlined(ui, l).clicked()
                        {
                            scan_act = Some(a);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if link_small(ui, "This looks wrong") {
                                report = Some(format!("Scan result \"{name}\": {detail}"));
                            }
                        });
                    });
                    ui.label(RichText::new(detail).size(13.0).color(TEXT_SOFT));
                }
            },
        );
        if let Some(a) = scan_act {
            self.do_act(&ui.ctx().clone(), a);
        }
        match act.as_deref() {
            Some("copied") => self.toast(
                &ui.ctx().clone(),
                "Copied. Paste it into Discord or anywhere.".to_string(),
            ),
            Some("fix") => self.do_act(&ui.ctx().clone(), NextAct::Step(3)),
            Some("check") => {
                let ctx = ui.ctx().clone();
                self.check_changes(&ctx);
                return;
            }
            Some(f) if f.starts_with("fix:") => {
                let id = f[4..].to_string();
                self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
                self.fix_idx = self.fix_queue.iter().position(|i| i.id == id).unwrap_or(0);
                self.optional_ok = true;
                self.detected = None;
                self.go(Screen::Preflight);
                self.step = 3;
                return;
            }
            Some(f) if f.starts_with("unkeep:") => {
                let id = f[7..].to_string();
                self.st.kept_issues.retain(|k| k != &id);
                self.st.save();
                self.recompute();
            }
            _ => {}
        }
        ui.add_space(12.0);
        self.ui_viewer_preview(ui);
        ui.add_space(12.0);
        egui::CollapsingHeader::new(
            RichText::new("Details: every check and the test measurements").strong(),
        )
        .id_salt("setup-details")
        .show(ui, |ui| {
            for row in a.checks.chunks(2) {
                ui.columns(2, |c| {
                    for (i, ch) in row.iter().enumerate() {
                        theme::card(&mut c[i], |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&ch.name).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| theme::status_pill(ui, ch.status, &ch.label),
                                );
                            });
                            ui.label(RichText::new(&ch.metric).size(16.0).strong());
                            theme::muted(ui, &ch.note);
                            ui.horizontal(|ui| {
                                if !ch.measured && ch.name == "Stream quality settings" {
                                    theme::read_tag(ui, "OBS");
                                } else {
                                    theme::measured_tag(ui, ch.measured);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if link_small(ui, "This looks wrong") {
                                            report =
                                                Some(format!("{}: \"{}\"", ch.name, ch.metric));
                                        }
                                    },
                                );
                            });
                        });
                    }
                });
                ui.add_space(8.0);
            }
            if let Some(t) = self.test.clone() {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "Test measurements · {}",
                        history::fmt_time(t.started_at)
                    ))
                    .size(21.0)
                    .strong(),
                );
                let cpu: Vec<f32> = t.samples.iter().map(|s| s.cpu).collect();
                let gpu: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
                if !gpu.is_empty() {
                    theme::card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("GPU usage").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "avg {:.0}% · peak {:.0}%",
                                            t.gpu_avg.unwrap_or(0.0),
                                            t.gpu_max.unwrap_or(0.0)
                                        ))
                                        .monospace()
                                        .color(TEXT_SOFT),
                                    );
                                },
                            );
                        });
                        theme::spark_bars(
                            ui,
                            &gpu,
                            100.0,
                            if t.gpu_avg.unwrap_or(0.0) >= 92.0 {
                                BAD
                            } else {
                                INFO
                            },
                            60.0,
                        );
                    });
                    ui.add_space(6.0);
                }
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("CPU usage").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "avg {:.0}% · peak {:.0}%",
                                    t.cpu_avg, t.cpu_max
                                ))
                                .monospace()
                                .color(TEXT_SOFT),
                            );
                        });
                    });
                    theme::spark_bars(ui, &cpu, 100.0, ACCENT, 60.0);
                });
                ui.add_space(6.0);
                theme::card(ui, |ui| {
                    egui::Grid::new("test-stats")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            let row = |ui: &mut Ui, k: &str, v: String| {
                                ui.label(RichText::new(k).color(MUTED));
                                ui.label(v);
                                ui.end_row();
                            };
                            row(
                                ui,
                                "GPU temperature (peak)",
                                t.gpu_temp_max
                                    .map(|x| format!("{x:.0}°C"))
                                    .unwrap_or_else(|| "—".into()),
                            );
                            row(
                                ui,
                                "CPU temperature (peak)",
                                t.cpu_temp_max
                                    .map(|x| format!("{x:.0}°C"))
                                    .unwrap_or_else(|| "not readable here".into()),
                            );
                            if t.with_obs && t.output_total > 0 {
                                let f = t.output_skipped as f32 / t.output_total as f32;
                                row(
                                    ui,
                                    "Stream frames skipped",
                                    if f > 0.0 {
                                        format!(
                                            "{:.2}% (about 1 in every {})",
                                            f * 100.0,
                                            (1.0 / f).round() as u32
                                        )
                                    } else {
                                        "none".into()
                                    },
                                );
                                row(
                                    ui,
                                    "Mic level (speaking)",
                                    t.mic_db
                                        .map(|x| format!("{x:.0} dB"))
                                        .unwrap_or_else(|| "not enough data".into()),
                                );
                                row(
                                    ui,
                                    "Game audio level",
                                    t.desktop_db
                                        .map(|x| format!("{x:.0} dB"))
                                        .unwrap_or_else(|| "not enough data".into()),
                                );
                            }
                            if let Some(e) = t.enc_avg {
                                row(
                                    ui,
                                    "Encoder chip busy",
                                    format!("{e:.0}% average, {:.0}% peak", t.enc_max.unwrap_or(e)),
                                );
                            }
                            for o in &t.outputs {
                                let what = if o.label == "Stream" {
                                    "network"
                                } else if o.label == "Recording" {
                                    "disk"
                                } else {
                                    "output"
                                };
                                row(
                                    ui,
                                    &format!("{} frames dropped ({what})", o.label),
                                    if o.dropped == 0 {
                                        "none".into()
                                    } else {
                                        format!(
                                            "{} of {} ({:.2}%)",
                                            o.dropped,
                                            o.total,
                                            o.dropped as f32 * 100.0 / o.total.max(1) as f32
                                        )
                                    },
                                );
                            }
                            row(
                                ui,
                                "Background programs",
                                if t.hogs.is_empty() {
                                    "none flagged".into()
                                } else {
                                    t.hogs
                                        .iter()
                                        .map(|h| h.label.clone())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                },
                            );
                            row(
                                ui,
                                "Preflight's own use",
                                match (t.own_cpu_avg, t.own_ram_mb) {
                                    (Some(c), Some(m)) => format!(
                                        "{}, {m:.0} MB RAM during the test",
                                        crate::util::one_core(c)
                                    ),
                                    _ => "not measured".into(),
                                },
                            );
                        });
                });
            }
        });
        if let Some(r) = report {
            self.open_report(r);
        }
        if go_pf {
            self.go(Screen::Preflight);
            if self.test.is_some() {
                self.set_step(3);
            }
        }
    }
}
