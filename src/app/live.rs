//! Live: the page shown while you stream.

use super::*;

pub(super) fn out_line(ui: &mut Ui, name: &str, value: &str, note: &str) {
    ui.add(egui::Separator::default().spacing(8.0));
    ui.horizontal(|ui| {
        ui.label(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(note).size(12.5).color(MUTED));
            ui.label(RichText::new(value).monospace());
        });
    });
}

pub(super) fn alert_line(ui: &mut Ui, text: &str, col: Color32) {
    ui.add_space(6.0);
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            col.r(),
            col.g(),
            col.b(),
            22,
        ))
        .stroke(egui::Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 110),
        ))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).size(13.5));
        });
}

/// The bitrate over time, with the target as a dashed line.
pub(super) fn bitrate_graph(ui: &mut Ui, kbps: &[f32], target: Option<f32>, h: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::hover());
    let p = ui.painter();
    if kbps.len() < 2 {
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "The graph fills in as you stream",
            egui::FontId::proportional(13.0),
            MUTED,
        );
        return;
    }
    let top = kbps.iter().cloned().fold(target.unwrap_or(0.0), f32::max) * 1.08;
    let y = |v: f32| rect.max.y - (v / top.max(1.0)) * rect.height();
    let n = kbps.len().max(2);
    let pts: Vec<egui::Pos2> = kbps
        .iter()
        .enumerate()
        .map(|(i, v)| egui::pos2(rect.min.x + i as f32 * rect.width() / (n - 1) as f32, y(*v)))
        .collect();
    let fill = Color32::from_rgba_unmultiplied(0x5E, 0xF0, 0xB5, 34);
    for pair in pts.windows(2) {
        p.add(egui::Shape::convex_polygon(
            vec![
                pair[0],
                pair[1],
                egui::pos2(pair[1].x, rect.max.y),
                egui::pos2(pair[0].x, rect.max.y),
            ],
            fill,
            egui::Stroke::NONE,
        ));
    }
    if let Some(t) = target {
        let ty = y(t);
        let mut x = rect.min.x;
        while x < rect.max.x {
            p.line_segment(
                [egui::pos2(x, ty), egui::pos2((x + 5.0).min(rect.max.x), ty)],
                egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 140)),
            );
            x += 9.0;
        }
    }
    p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, theme::TEAL)));
}

impl App {
    /// While you're live: big, calm numbers and plain alerts. Nothing to click.
    pub(super) fn ui_inflight(&mut self, ui: &mut Ui) {
        let Some(l) = self.live.clone() else {
            ui.label(theme::caps("Live · while you stream", 12.0, MUTED));
            ui.label(RichText::new("Not live right now").size(30.0).strong());
            ui.add_space(6.0);
            theme::card(ui, |ui| {
                ui.label(
                    RichText::new("This page opens by itself when you start streaming in OBS.")
                        .color(TEXT_SOFT),
                );
                ui.label(RichText::new("You'll see your bitrate, dropped and skipped frames and your PC's load as you stream, with a plain note if anything goes wrong.").color(TEXT_SOFT));
                if let Some(s) = self.streams.first() {
                    let p = crate::streamwatch::summarize(s);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Last stream: {} · {} · {}",
                                history::fmt_time(s.started_at),
                                crate::streamwatch::fmt_duration(p.duration),
                                p.grade
                            ))
                            .color(TEXT),
                        );
                        if outlined(ui, "See the summary").clicked() {
                            self.stream_sel = 0;
                            self.pending_act = Some(NextAct::Screen(Screen::PostFlight));
                        }
                    });
                }
                ui.checkbox(
                    &mut self.st.inflight_auto,
                    "Open this page by itself when I go live",
                );
            });
            return;
        };
        let h: Vec<ReadingPoint> = self.reading_hist.iter().cloned().collect();
        let kbps: Vec<f32> = h.iter().filter_map(|p| p.kbps).collect();
        let target = self
            .obs
            .as_ref()
            .and_then(|o| o.bitrate_kbps)
            .map(|b| b as f32);
        let dropped = l.dropped_pct;
        // From OBS every 5 s while live (the snapshot from before the stream is stale).
        let skipped_now = l.enc_skipped_pct;
        let gpu = h.last().and_then(|p| p.gpu);
        let enc = h.last().and_then(|p| p.enc);
        // One verdict.
        let low_bitrate = match (target, kbps.last()) {
            (Some(t), Some(k)) => *k < t * 0.75,
            _ => false,
        };
        let (verdict, vcol, vline) = if dropped >= 2.0 || skipped_now.unwrap_or(0.0) >= 2.0 {
            (
                "Rough",
                BAD,
                "Viewers are seeing stutter. The note below says why.",
            )
        } else if dropped >= 0.5 || skipped_now.unwrap_or(0.0) >= 0.5 || low_bitrate {
            (
                "Watch it",
                theme::AMBER,
                "A few problems; most viewers won't notice yet.",
            )
        } else {
            (
                "All smooth",
                theme::TEAL,
                "Viewers are getting a clean stream",
            )
        };
        let platforms = if self.st.platforms.is_empty() {
            self.st.platform.name().to_string()
        } else {
            self.st.platforms.join(" and ")
        };
        ui.label(theme::caps(
            &format!("Live on {platforms}"),
            12.0,
            Color32::from_rgb(0xFF, 0x5C, 0x6C),
        ));
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
            let t = ui.input(|i| i.time) as f32;
            // The halo breathes in step with the clock: one redraw a second.
            let pulse = if (t as i64) % 2 == 0 { 1.0 } else { 0.35 };
            ui.painter().circle_filled(
                r.center(),
                9.0 + pulse * 3.0,
                Color32::from_rgba_unmultiplied(
                    0xFF,
                    0x5C,
                    0x6C,
                    (40.0 * (1.0 - pulse)) as u8 + 20,
                ),
            );
            ui.painter()
                .circle_filled(r.center(), 7.0, Color32::from_rgb(0xFF, 0x5C, 0x6C));
            ui.label(
                RichText::new(crate::streamwatch::fmt_t(l.elapsed))
                    .monospace()
                    .size(38.0)
                    .color(TEXT),
            );
            theme::repaint_in(ui.ctx(), 1000);
            ui.add_space(16.0);
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(
                    vcol.r(),
                    vcol.g(),
                    vcol.b(),
                    30,
                ))
                .stroke(egui::Stroke::new(1.0, vcol))
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(16, 6))
                .show(ui, |ui| {
                    ui.label(theme::caps(verdict, 13.0, vcol));
                });
            ui.label(RichText::new(vline).color(MUTED));
        });
        ui.add_space(12.0);
        // Four big numbers.
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let w = ((ui.available_width() - gap * 3.0) / 4.0).max(150.0);
            // Asked from OBS every 5 s while live (not the snapshot from before the stream).
            let rec = self.live.as_ref().and_then(|l| l.rec.clone());
            let (rec_value, rec_sub) = match &rec {
                Some(r) if r.active && r.paused => (
                    "Paused".to_string(),
                    format!("Paused at {}", crate::streamwatch::fmt_t(r.seconds)),
                ),
                Some(r) if r.active => (
                    crate::streamwatch::fmt_t(r.seconds),
                    r.kbps
                        .map(|k| format!("Recording · about {:.0} Mbps", k / 1000.0))
                        .unwrap_or_else(|| "Recording alongside the stream".into()),
                ),
                Some(_) => ("Off".to_string(), "Not recording".to_string()),
                None => ("–".to_string(), "OBS didn't say".to_string()),
            };
            // (name, value, unit, note, sparkline, colour)
            type Tile<'a> = (&'a str, String, &'a str, String, Vec<f32>, Color32);
            let tiles: [Tile; 4] = [
                (
                    "Bitrate",
                    kbps.last()
                        .map(|k| format!("{k:.0}"))
                        .unwrap_or_else(|| "–".into()),
                    "kbps",
                    target.map(|t| format!("Target {t:.0}")).unwrap_or_default(),
                    kbps.iter().rev().take(60).rev().cloned().collect(),
                    theme::TEAL,
                ),
                (
                    "Dropped · internet",
                    format!("{dropped:.2}"),
                    "%",
                    if dropped < 0.1 {
                        "Your connection is steady".into()
                    } else {
                        "Frames lost on the way to the platform".into()
                    },
                    {
                        let v: Vec<f32> = h.iter().filter_map(|p| p.skipped).collect();
                        v[v.len().saturating_sub(60)..].to_vec()
                    },
                    theme::AMBER,
                ),
                (
                    "Skipped · encoder",
                    skipped_now
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "–".into()),
                    "%",
                    if skipped_now.unwrap_or(0.0) < 0.1 {
                        "The encoder is keeping up".into()
                    } else {
                        "The encoder is falling behind".into()
                    },
                    Vec::new(),
                    ACCENT,
                ),
                (
                    "Recording",
                    rec_value,
                    "",
                    rec_sub,
                    Vec::new(),
                    Color32::from_rgb(0xFF, 0x5C, 0x6C),
                ),
            ];
            for (label, value, unit, sub, series, col) in tiles {
                egui::Frame::NONE
                    .fill(CARD)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(w - 34.0);
                        ui.vertical(|ui| {
                            ui.label(theme::caps(label, 11.0, MUTED));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(value).monospace().size(28.0).color(TEXT));
                                ui.label(RichText::new(unit).color(MUTED));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        theme::sparkline(ui, &series, 56.0, 20.0, col);
                                    },
                                );
                            });
                            ui.label(RichText::new(sub).size(13.0).color(TEXT_SOFT));
                        });
                    });
            }
        });
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            let wide = (ui.available_width() * 0.62).max(360.0);
            egui::Frame::NONE.fill(CARD).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(egui::CornerRadius::same(16)).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
                ui.set_width(wide - 38.0);
                ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps("Bitrate · last 10 minutes", 11.0, MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if target.is_some() {
                            ui.label(RichText::new("dashed line: your target").size(12.0).color(MUTED));
                        }
                    });
                });
                bitrate_graph(ui, &kbps, target, 150.0);
                // Plain notes: only when something happened.
                let dip = target.and_then(|t| kbps.iter().rposition(|k| *k < t * 0.75).map(|i| (i, t)));
                if let Some((i, _)) = dip {
                    let ago = (kbps.len() - i) as f32;
                    let steady = kbps.iter().rev().take(10).all(|k| target.map(|t| *k >= t * 0.85).unwrap_or(true));
                    alert_line(ui, &format!("Your bitrate dipped about {} ago{}", crate::streamwatch::fmt_t(ago), if steady { ". It's steady again; nothing to do." } else { ". If it keeps dipping, your upload is struggling: lower the bitrate a little next time." }), theme::AMBER);
                } else if dropped >= 0.5 {
                    alert_line(ui, "Your internet is dropping frames. If it keeps happening, lower the bitrate next time or use a cable instead of Wi-Fi.", theme::AMBER);
                } else if skipped_now.unwrap_or(0.0) >= 0.5 {
                    alert_line(ui, "The encoder is falling behind: the game is using the whole GPU. Capping the game's frame rate helps.", theme::AMBER);
                }
                });
            });
            egui::Frame::NONE.fill(CARD).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(egui::CornerRadius::same(16)).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
                ui.set_width(ui.available_width() - 2.0);
                ui.vertical(|ui| {
                ui.label(theme::caps("Each output", 11.0, MUTED));
                // Read from OBS every 10 s while live.
                let outs: Vec<crate::obs::OutputInfo> = l.outputs.clone();
                if outs.is_empty() {
                    theme::small(ui, "OBS will list each stream and recording here.");
                }
                for o in &outs {
                    let pct = match (o.skipped, o.total) {
                        (Some(s), Some(t)) if t > 0 => format!("{:.2}%", s as f32 * 100.0 / t as f32),
                        _ => "–".into(),
                    };
                    out_line(ui, &o.platform_hint.clone().unwrap_or_else(|| o.label.clone()), &pct, "skipped");
                }
                ui.add_space(8.0);
                ui.label(theme::caps("This PC", 11.0, MUTED));
                out_line(ui, "GPU", &gpu.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "–".into()), if gpu.unwrap_or(0.0) >= 95.0 { "maxed out" } else { "has room" });
                out_line(ui, "Encoder", &enc.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "–".into()), "busy");
                });
            });
        });
    }
}
