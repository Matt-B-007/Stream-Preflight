//! Test: the checklist, live readings, test recordings and the viewer preview.

use super::*;

/// One tile of the readings strip: name, value, sparkline, colour.
pub(super) type Reading = (&'static str, String, Vec<f32>, Color32);

impl App {
    /// The live readings strip: during a test or a stream it follows what's
    /// happening; otherwise it shows the last test. None when there's nothing yet.
    pub(super) fn readings(&self) -> Option<Vec<Reading>> {
        let fmt = |v: Option<f32>, unit: &str| {
            v.map(|x| format!("{x:.0}{unit}"))
                .unwrap_or_else(|| "–".into())
        };
        let measuring = self
            .test_handle
            .as_ref()
            .map(|h| h.phase() == Phase::Measuring)
            .unwrap_or(false);
        if (measuring || self.live.is_some()) && !self.reading_hist.is_empty() {
            let h = &self.reading_hist;
            let col = |f: fn(&ReadingPoint) -> Option<f32>| {
                let v: Vec<f32> = h.iter().filter_map(f).collect();
                v[v.len().saturating_sub(60)..].to_vec()
            };
            let last = h.back().cloned().unwrap_or_default();
            let gpu = col(|p| p.gpu);
            return Some(vec![
                (
                    "GPU",
                    fmt(last.gpu, "%"),
                    gpu.clone(),
                    if last.gpu.unwrap_or(0.0) >= 92.0 {
                        theme::AMBER
                    } else {
                        ACCENT
                    },
                ),
                ("Encoder", fmt(last.enc, "%"), col(|p| p.enc), ACCENT),
                (
                    "Stream",
                    last.kbps
                        .map(|k| format!("{k:.0} kbps"))
                        .unwrap_or_else(|| "not live".into()),
                    col(|p| p.kbps),
                    theme::TEAL,
                ),
                (
                    "Skipped",
                    last.skipped
                        .map(|s| format!("{s:.2}%"))
                        .unwrap_or_else(|| "–".into()),
                    col(|p| p.skipped),
                    theme::TEAL,
                ),
            ]);
        }
        let t = self.test.as_ref()?;
        let gpu: Vec<f32> = t.samples.iter().filter_map(|s| s.gpu).collect();
        let enc: Vec<f32> = t.samples.iter().filter_map(|s| s.enc).collect();
        let skipped = if t.output_total > 0 {
            Some(t.output_skipped as f32 * 100.0 / t.output_total as f32)
        } else {
            None
        };
        let kbps = self.obs.as_ref().and_then(|o| o.bitrate_kbps);
        Some(vec![
            (
                "GPU · last test",
                fmt(t.gpu_avg, "%"),
                gpu,
                if t.gpu_avg.unwrap_or(0.0) >= 92.0 {
                    theme::AMBER
                } else {
                    ACCENT
                },
            ),
            ("Encoder", fmt(t.enc_avg, "%"), enc, ACCENT),
            (
                "Stream",
                kbps.map(|k| format!("{k} kbps"))
                    .unwrap_or_else(|| "–".into()),
                Vec::new(),
                theme::TEAL,
            ),
            (
                "Skipped",
                skipped
                    .map(|s| format!("{s:.2}%"))
                    .unwrap_or_else(|| "–".into()),
                Vec::new(),
                theme::TEAL,
            ),
        ])
    }

    /// Adds one point a second to the readings history while testing or live.
    pub(super) fn track_readings(&mut self, now: f64) {
        if now - self.reading_at < 1.0 {
            return;
        }
        self.reading_at = now;
        let last = self
            .test_handle
            .as_ref()
            .filter(|h| h.phase() == Phase::Measuring)
            .and_then(|h| h.state.lock().ok().and_then(|s| s.last.clone()));
        if last.is_none() && self.live.is_none() {
            if !self.reading_hist.is_empty() && self.test_handle.is_none() {
                self.reading_hist.clear();
            }
            return;
        }
        let point = ReadingPoint {
            gpu: last
                .as_ref()
                .and_then(|s| s.gpu)
                .or_else(|| self.reading_hist.back().and_then(|p| p.gpu)),
            enc: last.as_ref().and_then(|s| s.enc).or_else(|| {
                self.encoders
                    .iter()
                    .filter_map(|e| e.busy)
                    .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b))))
            }),
            kbps: self.live.as_ref().and_then(|l| l.kbps),
            skipped: self.live.as_ref().map(|l| l.dropped_pct),
        };
        self.reading_hist.push_back(point);
        while self.reading_hist.len() > 600 {
            self.reading_hist.pop_front();
        }
    }

    /// Deletes one of Preflight's own test recordings (only on the user's click
    /// or their automatic clean-up choice).
    pub(super) fn delete_recording(&mut self, ctx: &egui::Context, path: &str) {
        if !self.st.pending_recordings.iter().any(|p| p.path == path) {
            return;
        }
        let r = std::fs::remove_file(path);
        self.st.pending_recordings.retain(|p| p.path != path);
        self.st.save();
        match r {
            Ok(_) => {
                log::info!("Test recording deleted");
                self.toast(ctx, "Test recording deleted");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => self.toast(ctx, format!("Couldn't delete it: {e}")),
        }
    }

    /// "mockrec.mkv · 845 MB · Desktop" with Delete, Show in folder and Keep.
    pub(super) fn recording_row(
        &self,
        ui: &mut Ui,
        rec: &crate::config::PendingRecording,
        title: &str,
    ) -> Option<(String, String)> {
        let mut act = None;
        let p = std::path::Path::new(&rec.path);
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let folder = p
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        theme::inset(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong());
                ui.label(
                    RichText::new(format!(
                        "{name} · {} · {folder}",
                        crate::util::fmt_bytes(rec.size)
                    ))
                    .color(TEXT_SOFT),
                );
            });
            ui.horizontal(|ui| {
                if theme::secondary(ui, "Delete it").clicked() {
                    act = Some(("delete".to_string(), rec.path.clone()));
                }
                if theme::secondary(ui, "Show in folder").clicked() {
                    report::show_file(p);
                }
                if link_small(ui, "Keep it") {
                    act = Some(("keep".to_string(), rec.path.clone()));
                }
            });
        });
        act
    }

    pub(super) fn do_recording_act(&mut self, ctx: &egui::Context, a: Option<(String, String)>) {
        match a {
            Some((k, p)) if k == "delete" => self.delete_recording(ctx, &p),
            Some((k, p)) if k == "keep" => {
                self.st.pending_recordings.retain(|x| x.path != p);
                self.st.save();
            }
            _ => {}
        }
    }

    /// The automatic clean-up, when the user turned it on: once the preview is made.
    pub(super) fn auto_clean_recording(&mut self, ctx: &egui::Context) {
        if !self.st.auto_delete_recordings {
            return;
        }
        if let Some(p) = self.test.as_ref().and_then(|t| t.recording_path.clone()) {
            self.delete_recording(ctx, &p);
        }
    }

    pub(super) fn ui_recording_popup(&mut self, ctx: &egui::Context) {
        if !self.show_recording_popup || self.report.is_some() {
            return;
        }
        let now = crate::util::unix_now();
        let Some(rec) = self
            .st
            .pending_recordings
            .iter()
            .find(|p| p.remind_after <= now)
            .cloned()
        else {
            self.show_recording_popup = false;
            return;
        };
        let mut action: Option<&str> = None;
        egui::Window::new("Your test recording is still saved")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label("You can delete it if you're done with it. Preflight only deletes this one file, and only if you click Delete.");
                let name = std::path::Path::new(&rec.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                theme::inset(ui, |ui| {
                    ui.label(RichText::new(name).strong());
                    theme::small(ui, format!("{} · {}", crate::util::fmt_bytes(rec.size), rec.path));
                });
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete").color(BG).strong()).fill(BAD)).clicked() {
                        action = Some("delete");
                    }
                    if theme::secondary(ui, "Keep").clicked() {
                        action = Some("keep");
                    }
                    if theme::secondary(ui, "Remind me later").clicked() {
                        action = Some("later");
                    }
                });
            });
        match action {
            Some("delete") => {
                let r = std::fs::remove_file(&rec.path);
                self.st.pending_recordings.retain(|p| p.path != rec.path);
                self.st.save();
                match r {
                    Ok(_) => {
                        log::info!("Test recording deleted by the user");
                        self.toast(ctx, "Test recording deleted");
                    }
                    Err(e) => self.toast(ctx, format!("Couldn't delete it: {e}")),
                }
                self.show_recording_popup = false;
            }
            Some("keep") => {
                self.st.pending_recordings.retain(|p| p.path != rec.path);
                self.st.save();
                self.show_recording_popup = false;
            }
            Some("later") => {
                for p in self.st.pending_recordings.iter_mut() {
                    if p.path == rec.path {
                        p.remind_after = now + 24 * 3600;
                    }
                }
                self.st.save();
                self.show_recording_popup = false;
            }
            _ => {}
        }
    }

    pub(super) fn scan_items(&self) -> Vec<(Status, String, &'static str, String)> {
        let mut v = Vec::new();
        match &self.hw {
            Some(h) => {
                let strong = h.ram_gb >= 15.0 && h.cpu_threads >= 8 && h.has_hw_encoder();
                let text = match (h.main_gpu(), h.encoders.first()) {
                    (Some(g), Some(e)) => {
                        format!("{} and {}, with {e} for streaming.", g.name, h.cpu)
                    }
                    (Some(g), None) => format!(
                        "{} and {}. No hardware encoder was found, so OBS would stream on the CPU.",
                        g.name, h.cpu
                    ),
                    (None, _) => format!(
                        "{}. No graphics card was found, so OBS would stream on the CPU.",
                        h.cpu
                    ),
                };
                v.push((
                    if strong { Status::Ok } else { Status::Warn },
                    if strong { "Strong" } else { "Limited" }.to_string(),
                    "Hardware",
                    text,
                ));
            }
            None => v.push((
                Status::Unknown,
                "Checking".into(),
                "Hardware",
                "Checking your hardware…".into(),
            )),
        }
        match (&self.obs, &self.obs_error) {
            (Some(o), _) if o.connected => v.push((
                Status::Ok,
                "Connected".into(),
                "OBS",
                format!(
                    "Streaming {} at {} fps, {}, using {}.",
                    o.output_res
                        .map(|r| format!("{}p", r.1))
                        .unwrap_or_else(|| "?".into()),
                    o.fps
                        .map(|f| (f.round() as u32).to_string())
                        .unwrap_or_else(|| "?".into()),
                    o.bitrate_kbps
                        .map(|b| format!("{b} kbps"))
                        .unwrap_or_else(|| "bitrate unknown".into()),
                    o.encoder_kind
                ),
            )),
            (_, Some(e)) => v.push((Status::Warn, "Not connected".into(), "OBS", e.clone())),
            _ => v.push((
                Status::Unknown,
                "Checking".into(),
                "OBS",
                "Looking for OBS…".into(),
            )),
        }
        let found: Vec<&GameResult> = self.games.iter().filter(|g| g.relevant()).collect();
        let read = found
            .iter()
            .filter(|g| g.files.iter().any(|f| f.read_ok))
            .count();
        let waiting = found.len() - read;
        v.push((
            if found.is_empty() {
                Status::Unknown
            } else if waiting > 0 {
                Status::Warn
            } else {
                Status::Ok
            },
            if found.is_empty() {
                "None found".to_string()
            } else if waiting > 0 {
                "Check".into()
            } else {
                "Ready".into()
            },
            "Games",
            if found.is_empty() {
                "None of the 7 supported games were found.".into()
            } else if waiting > 0 {
                format!(
                    "{} found. Settings read for {read}; {waiting} not launched on this PC yet.",
                    found.len()
                )
            } else {
                format!("{} found, and settings read for all of them.", found.len())
            },
        ));
        match &self.net {
            Some(n) if n.ok => {
                let b = self
                    .obs
                    .as_ref()
                    .and_then(|o| o.bitrate_kbps)
                    .unwrap_or(6000);
                let ratio = n.low_mbps / ((b as f32 + 160.0) / 1000.0);
                let s = if ratio >= 3.0 {
                    Status::Ok
                } else if ratio >= 1.0 {
                    Status::Warn
                } else {
                    Status::Bad
                };
                v.push((s, if ratio >= 3.0 { "Plenty" } else if ratio >= 1.0 { "Tight" } else { "Too slow" }.to_string(), "Internet", format!("{:.0} Mbps up, dipping to {:.0}. That's about {:.0} times what your stream needs.", n.avg_mbps, n.low_mbps, ratio)));
            }
            Some(n) => v.push((
                Status::Warn,
                "Not tested".into(),
                "Internet",
                n.error.clone().unwrap_or_default(),
            )),
            None => v.push((
                Status::Unknown,
                if self.scan.is_some() {
                    "Checking"
                } else {
                    "Not tested"
                }
                .to_string(),
                "Internet",
                if self.st.network_test_on_scan {
                    "Tested during the scan.".into()
                } else {
                    "Upload test is turned off in Settings.".into()
                },
            )),
        }
        v
    }

    pub(super) fn ui_step2(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut action: Option<&str> = None;
        let mut rec_act: Option<(String, String)> = None;
        let phase = self.test_handle.as_ref().map(|h| h.phase());
        let (elapsed, last, shots) = self
            .test_handle
            .as_ref()
            .and_then(|h| {
                h.state
                    .lock()
                    .ok()
                    .map(|s| (s.elapsed, s.last.clone(), s.shots))
            })
            .unwrap_or((0.0, None, 0));
        let show_done =
            self.test.is_some() && !self.want_new_test && !matches!(phase, Some(Phase::Measuring));
        theme::hero_card(ui, |ui| {
            ui.vertical(|ui| {
                do_next_label(ui);
                let running = self.games.iter().any(|g| g.running);
                let head = if !running {
                    "Start a game, then record in OBS"
                } else if self.show_bw_help {
                    "Start streaming and recording in OBS"
                } else {
                    "Start recording in OBS"
                };
                ui.label(RichText::new(head).size(24.0).strong());
            });
            ui.add_space(6.0);
            if matches!(phase, Some(Phase::Measuring)) {
                let target = self
                    .test_handle
                    .as_ref()
                    .and_then(|h| h.state.lock().ok().map(|s| s.target))
                    .unwrap_or(livetest::TARGET_SECONDS);
                let left = (target - elapsed).max(0.0);
                ui.horizontal(|ui| {
                    let head = if left > 0.0 {
                        "Measuring · keep playing".to_string()
                    } else if shots < livetest::PREVIEW_SHOTS {
                        format!(
                            "Grabbing preview frames ({shots} of {}) · keep playing",
                            livetest::PREVIEW_SHOTS
                        )
                    } else {
                        "All done · stop the recording in OBS".to_string()
                    };
                    ui.label(RichText::new(head).size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}:{:02} left",
                                (left as u32) / 60,
                                (left as u32) % 60
                            ))
                            .monospace(),
                        );
                    });
                });
                theme::bar(ui, elapsed / target, ACCENT, 10.0);
                if let Some(s) = last {
                    ui.columns(3, |c| {
                        let gpu = s
                            .gpu
                            .map(|g| {
                                if g >= 92.0 {
                                    format!("Maxed out · {g:.0}%")
                                } else {
                                    format!("Has room · {g:.0}%")
                                }
                            })
                            .unwrap_or_else(|| "Not readable".into());
                        theme::inset(&mut c[0], |ui| {
                            theme::small(ui, "GPU");
                            ui.label(RichText::new(gpu).strong());
                        });
                        theme::inset(&mut c[1], |ui| {
                            theme::small(ui, "CPU");
                            ui.label(
                                RichText::new(if s.cpu >= 85.0 {
                                    format!("Nearly full · {:.0}%", s.cpu)
                                } else {
                                    format!("Plenty spare · {:.0}%", s.cpu)
                                })
                                .strong(),
                            );
                        });
                        theme::inset(&mut c[2], |ui| {
                            theme::small(ui, "GPU temperature");
                            ui.label(
                                RichText::new(
                                    s.gpu_temp
                                        .map(|t| {
                                            if t >= 87.0 {
                                                format!("Hot · {t:.0}°C")
                                            } else {
                                                format!("Normal · {t:.0}°C")
                                            }
                                        })
                                        .unwrap_or_else(|| "Not readable".into()),
                                )
                                .strong(),
                            );
                        });
                    });
                }
                // Encoders working right now.
                let busy: Vec<String> = self
                    .encoders
                    .iter()
                    .filter(|e| e.in_use())
                    .map(|e| {
                        format!(
                            "{}: {}",
                            e.kind,
                            e.use_text().trim_start_matches("In use: ")
                        )
                    })
                    .collect();
                if !busy.is_empty() {
                    theme::small(ui, format!("Encoders: {}", busy.join(" · ")));
                }
                theme::small(
                    ui,
                    "When the bar fills, keep playing about 10 more seconds while Preflight grabs frames for the viewer preview. Then stop the recording in OBS. Preflight finishes by itself.",
                );
                if link_small(ui, "Stop the test now") {
                    action = Some("stop");
                }
                return;
            }
            if show_done {
                let t = self.test.clone().unwrap();
                ui.label(
                    RichText::new(format!(
                        "Test complete{} · {}m {:02}s",
                        t.game_running
                            .as_ref()
                            .map(|g| format!(" · {g}"))
                            .or_else(|| t
                                .game_guess
                                .as_ref()
                                .map(|g| format!(" · {g} (not a game Preflight knows yet)")))
                            .unwrap_or_default(),
                        (t.seconds as u32) / 60,
                        (t.seconds as u32) % 60
                    ))
                    .size(14.0)
                    .color(MUTED),
                );
                let bullets = self.test_summary(&t);
                // Verdict first.
                let bad: Vec<&(String, String, Color32)> =
                    bullets.iter().filter(|b| b.2 == BAD).collect();
                let warn: Vec<&(String, String, Color32)> =
                    bullets.iter().filter(|b| b.2 == WARN).collect();
                let (verdict, col) = if let Some(b) = bad.first() {
                    (
                        format!(
                            "Viewers would notice problems. Main one: {}",
                            lower_first(&b.0)
                        ),
                        BAD,
                    )
                } else if let Some(w) = warn.first() {
                    (
                        format!(
                            "Your stream and recording would mostly be smooth. One thing to watch: {}",
                            lower_first(&w.0)
                        ),
                        WARN,
                    )
                } else {
                    (
                        "Your stream and recording would be smooth.".to_string(),
                        GOOD,
                    )
                };
                ui.label(RichText::new(verdict).size(19.0).strong().color(col));
                let (imp, opt) = self.open_counts();
                if imp + opt > 0 {
                    ui.label(
                        RichText::new(format!(
                            "Your settings: {}. Step 2 goes through them one at a time.",
                            count_words(imp, opt)
                        ))
                        .color(TEXT_SOFT),
                    );
                }
                // "Check my changes": before and after.
                if let Some(c) = &self.compare {
                    let sk = |r: &TestResult| {
                        if r.output_total > 0 {
                            r.output_skipped as f32 * 100.0 / r.output_total as f32
                        } else {
                            0.0
                        }
                    };
                    let (sb, sa) = (sk(&c.before), sk(&c.after));
                    let better = c.after_score > c.before_score
                        || sa + 0.3 < sb
                        || maxed_pct(&c.after) + 10.0 < maxed_pct(&c.before);
                    let worse = c.after_score < c.before_score
                        || sa > sb + 0.3
                        || maxed_pct(&c.after) > maxed_pct(&c.before) + 10.0;
                    // No setting change seen, but a very different load: probably a different part of the game.
                    let gpu_gap =
                        (c.before.gpu_avg.unwrap_or(0.0) - c.after.gpu_avg.unwrap_or(0.0)).abs();
                    // What changed between the two tests, in OBS and in the game.
                    let obs_diff: Vec<String> =
                        match (&c.before.obs_settings, &c.after.obs_settings) {
                            (Some(b), Some(a)) => b.changes_to(a),
                            _ => Vec::new(),
                        };
                    let game_saved = c.before.settings_saved.is_some()
                        && c.before.settings_saved != c.after.settings_saved;
                    let same_settings = c.before.settings_saved.is_some()
                        && !game_saved
                        && obs_diff.is_empty()
                        && !self
                            .fixed
                            .iter()
                            .any(|f| analysis::place_of(f) != analysis::Place::Game);
                    let not_comparable = same_settings
                        && (gpu_gap > 15.0
                            || (maxed_pct(&c.before) - maxed_pct(&c.after)).abs() > 25.0);
                    let (line, fill, stroke) = match (better, worse) {
                        _ if not_comparable => (
                            "The two tests look too different to compare. The game didn't save a settings change, so this was probably a lighter or busier part of the game. Play the same spot and check again.",
                            CARD,
                            BORDER,
                        ),
                        (true, false) => (
                            "Your changes helped.",
                            GOOD_BG,
                            Color32::from_rgb(0x2F, 0x6B, 0x47),
                        ),
                        (false, true) => (
                            "It got worse after your changes. Check what you changed, or undo it.",
                            BAD_BG,
                            Color32::from_rgb(0x6B, 0x2B, 0x2B),
                        ),
                        (true, true) => ("Some things got better and some worse.", CARD, BORDER),
                        _ => (
                            "About the same as before. The change didn't make a measurable difference.",
                            CARD,
                            BORDER,
                        ),
                    };
                    egui::Frame::NONE
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0, stroke))
                        .corner_radius(egui::CornerRadius::same(12))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(format!(
                                    "Your changes, before and after: {}",
                                    lower_first(line)
                                ))
                                .strong(),
                            );
                            egui::Grid::new("compare")
                                .num_columns(3)
                                .spacing([24.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("");
                                    ui.label(RichText::new("Before").color(MUTED));
                                    ui.label(RichText::new("After").color(MUTED));
                                    ui.end_row();
                                    for (k, b, a) in [
                                        (
                                            "Smoothness",
                                            c.before_score.to_string(),
                                            c.after_score.to_string(),
                                        ),
                                        (
                                            "GPU maxed out",
                                            format!("{:.0}% of the test", maxed_pct(&c.before)),
                                            format!("{:.0}% of the test", maxed_pct(&c.after)),
                                        ),
                                        (
                                            "GPU average",
                                            c.before
                                                .gpu_avg
                                                .map(|g| format!("{g:.0}%"))
                                                .unwrap_or_else(|| "—".into()),
                                            c.after
                                                .gpu_avg
                                                .map(|g| format!("{g:.0}%"))
                                                .unwrap_or_else(|| "—".into()),
                                        ),
                                        (
                                            "Skipped frames (encoding lag)",
                                            format!("{:.2}%", sk(&c.before)),
                                            format!("{:.2}%", sk(&c.after)),
                                        ),
                                    ] {
                                        if b == "—" && a == "—" {
                                            continue;
                                        }
                                        ui.label(RichText::new(k).color(MUTED));
                                        ui.label(b);
                                        ui.label(RichText::new(a).strong());
                                        ui.end_row();
                                    }
                                });
                            let mut changed: Vec<String> =
                                obs_diff.iter().map(|d| format!("OBS: {d}")).collect();
                            if game_saved {
                                changed.push(format!(
                                    "{}: settings saved again",
                                    c.after
                                        .game_running
                                        .clone()
                                        .unwrap_or_else(|| "Game".into())
                                ));
                            }
                            if !changed.is_empty() {
                                theme::small(
                                    ui,
                                    format!("Changed between the tests: {}", changed.join(" · ")),
                                );
                            } else if c.before.obs_settings.is_some() {
                                theme::small(
                                    ui,
                                    "No OBS or game setting changed between the tests.",
                                );
                            }
                        });
                    ui.add_space(4.0);
                }
                match (&self.preview, self.preview_rx.is_some()) {
                    (_, true) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Making your viewer preview…");
                        });
                    }
                    (Some(p), _) => {
                        if let Some(db) = p.worst().and_then(|w| w.match_db) {
                            let (head, meaning) = crate::preview::meaning(db);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("Viewer preview: {head}.")).strong(),
                                );
                                ui.label(RichText::new(meaning).color(TEXT_SOFT));
                                if link_small(ui, "See it") {
                                    action = Some("results");
                                    self.scroll_to_preview = true;
                                }
                            });
                        } else if p.ffmpeg_missing {
                            theme::muted(
                                ui,
                                "The viewer preview needs ffmpeg to read your recording. See Your setup for how to get it.",
                            );
                        }
                    }
                    _ => {}
                }
                egui::CollapsingHeader::new(
                    RichText::new(format!("Details ({} measurements)", bullets.len())).strong(),
                )
                .id_salt("test-details")
                .show(ui, |ui| {
                    for n in &t.notes {
                        theme::small(ui, n);
                    }
                    for (head, meaning, col) in &bullets {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(head).strong().color(*col));
                            ui.label(RichText::new(meaning).color(TEXT_SOFT));
                        });
                    }
                });
                if let Some(rec) = t
                    .recording_path
                    .as_ref()
                    .and_then(|p| self.st.pending_recordings.iter().find(|r| &r.path == p))
                    .cloned()
                {
                    rec_act = self.recording_row(ui, &rec, "Test recording:");
                }
                ui.add_space(4.0);
                // You record with its own encoder but this test didn't stream:
                // offer the test that measures both.
                let separate_rec = self
                    .obs
                    .as_ref()
                    .map(|o| o.recording_uses_stream_settings == Some(false))
                    .unwrap_or(false);
                if t.streamed {
                    theme::small(
                        ui,
                        "This was a stream + record test: both were measured together.",
                    );
                } else if separate_rec {
                    ui.horizontal_wrapped(|ui| {
                        theme::small(ui, "This test measured your recording only. You record with its own encoder, so test both together for the real load.");
                        if link_small(ui, "Test streaming and recording together") {
                            action = Some("stream_record_now");
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "See your setup").clicked() {
                        action = Some("results");
                    }
                    let again = match self.st.last_test.as_str() {
                        "quick" => "Test again (quick check)",
                        "stream_record" => "Test again (stream + record)",
                        _ => "Test again",
                    };
                    if theme::secondary(ui, again).clicked() {
                        action = Some("again");
                    }
                });
                return;
            }
            // Which test? Remembered for next time.
            let separate_gpu_rec = self
                .obs
                .as_ref()
                .map(|o| {
                    o.recording_uses_stream_settings == Some(false)
                        && !o.rec_encoder_kind.is_empty()
                        && !o.rec_encoder_kind.contains("CPU")
                })
                .unwrap_or(false);
            let kind = if self.quick_next {
                "quick"
            } else if self.show_bw_help {
                "stream_record"
            } else {
                "full"
            };
            let mut chosen: Option<&str> = None;
            let (kname, kdesc) = match kind {
                "quick" => (
                    "Quick check",
                    "about 1 minute · compares with your last test",
                ),
                "stream_record" => (
                    "Stream + record",
                    "about 2 minutes · measures both together",
                ),
                _ => ("Full test", "about 2 minutes · recommended"),
            };
            if !self.test_choice_open {
                // The choice, folded to one line.
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 14))
                    .stroke(egui::Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 70),
                    ))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(theme::caps("Test", 11.0, MUTED));
                            ui.label(RichText::new(kname).strong());
                            ui.label(RichText::new(format!("· {kdesc}")).color(MUTED));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("Change").color(ACCENT).underline(),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        self.test_choice_open = true;
                                    }
                                },
                            );
                        });
                    });
            } else {
                ui.label(RichText::new("Which test?").strong());
                ui.columns(3, |cols| {
                let tiles = [
                    ("full", "Full test · 2 min", if separate_gpu_rec { "Measures your recording. Pick stream + record to include your stream too." } else { "Measures your stream settings while you play. Recommended." }),
                    ("stream_record", "Stream + record · 2 min", if separate_gpu_rec { "Recommended for you: you record with its own encoder, so this measures your real load." } else { "For when you stream and record at the same time (Twitch test mode)." }),
                    ("quick", "Quick check · 1 min", "After a change: a shorter test to compare with the last one."),
                ];
                for (i, (id, name, desc)) in tiles.into_iter().enumerate() {
                    if choice_tile(&mut cols[i], kind == id, name, desc) {
                        chosen = Some(id);
                    }
                }
            });
            }
            if let Some(id) = chosen {
                self.test_choice_open = false;
                self.quick_next = id == "quick";
                self.show_bw_help = id == "stream_record";
                self.st.last_test = id.to_string();
                self.st.save();
                // The watcher was started for the old choice: restart it so
                // the test length matches.
                if let Some(h) = &self.test_handle
                    && !matches!(h.phase(), Phase::Measuring)
                {
                    h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.test_handle = None;
                    self.watch_retry_at = 0.0;
                }
            }
            ui.add_space(6.0);
            // One checklist that ticks itself as Preflight sees each thing happen.
            let connected = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
            let (rec_on, stream_on) = self
                .test_handle
                .as_ref()
                .and_then(|h| h.state.lock().ok().map(|s| (s.rec_active, s.stream_active)))
                .unwrap_or((false, false));
            let waiting = matches!(phase, Some(Phase::WaitingForRecord));
            let platforms: Vec<String> = if self.st.platforms.is_empty() {
                vec![self.st.platform.name().to_string()]
            } else {
                self.st.platforms.clone()
            };
            let game_running = self
                .games
                .iter()
                .find(|g| g.running)
                .map(|g| g.name.clone());
            ui.add_space(2.0);
            let mut n = 0;
            let mut num = || {
                n += 1;
                n
            };
            // OBS
            let obs_line = format!("{}. OBS is open and connected", num());
            if connected {
                tick_row(
                    ui,
                    Tick::Done,
                    &obs_line,
                    &format!(
                        "OBS {}",
                        self.obs
                            .as_ref()
                            .map(|o| o.obs_version.clone())
                            .unwrap_or_default()
                    ),
                    |_| {},
                );
            } else {
                let e = self.obs_error.clone().unwrap_or_else(|| {
                    "Open OBS with its WebSocket server on. Preflight connects by itself.".into()
                });
                let (l, a) = err_action(&e).unwrap_or(("Show me how", NextAct::ObsHelp));
                tick_row(ui, Tick::Watch, &obs_line, &e, |ui| {
                    if outlined(ui, l).clicked() {
                        self.pending_act = Some(a);
                    }
                });
            }
            // Game
            let gl = format!("{}. Your game is running", num());
            match &game_running {
                Some(g) => tick_row(
                    ui,
                    Tick::Done,
                    &gl,
                    &format!("{g}. Get into real gameplay before recording."),
                    |_| {},
                ),
                None => tick_row(
                    ui,
                    Tick::Watch,
                    &gl,
                    "Start your game and Preflight spots it. Other games work too; they get general advice.",
                    |_| {},
                ),
            }
            // Recording must match the stream (normal test only).
            let o = self.obs.clone();
            if !self.show_bw_help
                && let Some(false) = o.as_ref().and_then(|o| o.recording_uses_stream_settings)
            {
                let own_gpu = o
                    .as_ref()
                    .map(|o| !o.rec_encoder_kind.is_empty() && !o.rec_encoder_kind.contains("CPU"))
                    .unwrap_or(false);
                if own_gpu {
                    tick_row(
                        ui,
                        Tick::Info,
                        "You record with its own encoder",
                        "A normal test measures only the recording. Stream + record measures your real load.",
                        |ui| {
                            if outlined(ui, "Use stream + record").clicked() {
                                self.show_bw_help = true;
                                self.st.last_test = "stream_record".into();
                                self.st.save();
                            }
                        },
                    );
                } else {
                    let adv = o
                        .as_ref()
                        .map(|o| o.output_mode.eq_ignore_ascii_case("advanced"))
                        .unwrap_or(false);
                    tick_row(
                        ui,
                        Tick::Warn,
                        "Fix first: recording doesn't use your stream settings",
                        if adv {
                            "OBS › Settings › Output › Recording › Video Encoder › (Use stream encoder)."
                        } else {
                            "OBS › Settings › Output › Recording Quality › Same as stream."
                        },
                        |_| {},
                    );
                }
            }
            if self.show_bw_help {
                // Test mode for each platform: the one step done by hand.
                for p in &platforms {
                    let ok = self.private_ok.contains(p)
                        || (p == "Twitch" && self.st.bandwidth_test_pending);
                    let st = if ok { Tick::Done } else { Tick::Wait };
                    match p.as_str() {
                        "Twitch" => {
                            tick_row(
                                ui,
                                st,
                                &format!(
                                    "{}. Twitch test mode: add ?bandwidthtest=true to the end of your stream key",
                                    num()
                                ),
                                "In OBS › Settings › Stream. Viewers won't see the test stream.",
                                |ui| {
                                    if outlined(ui, "Copy").clicked() {
                                        ui.ctx().copy_text("?bandwidthtest=true".into());
                                        action = Some("bw_copied");
                                    }
                                    if !ok && outlined(ui, "I added it").clicked() {
                                        action = Some("bw_added");
                                    }
                                },
                            );
                        }
                        "YouTube" => {
                            tick_row(
                                ui,
                                st,
                                &format!(
                                    "{}. YouTube test mode: set the stream to Unlisted",
                                    num()
                                ),
                                "YouTube Studio › Create › Go live, then use that stream's key.",
                                |ui| {
                                    if !ok && outlined(ui, "It's Unlisted").clicked() {
                                        self.private_ok.insert("YouTube".into());
                                    }
                                },
                            );
                        }
                        other => {
                            let other = other.to_string();
                            tick_row(
                                ui,
                                st,
                                &format!("{}. {other} has no hidden test mode", num()),
                                "This test would go out to your viewers.",
                                |ui| {
                                    if !ok {
                                        if outlined(ui, "Measure my next real stream").clicked() {
                                            action = Some("measure_next_stream");
                                        }
                                        if outlined(ui, "Test live anyway").clicked() {
                                            self.private_ok.insert(other.clone());
                                        }
                                    }
                                },
                            );
                        }
                    }
                }
                let sl = format!("{}. Start streaming in OBS", num());
                tick_row(
                    ui,
                    if stream_on {
                        Tick::Done
                    } else if connected && waiting {
                        Tick::Watch
                    } else {
                        Tick::Wait
                    },
                    &sl,
                    "",
                    |_| {},
                );
            }
            let rl = format!("{}. Start recording in OBS", num());
            tick_row(
                ui,
                if rec_on {
                    Tick::Done
                } else if connected && waiting {
                    Tick::Watch
                } else {
                    Tick::Wait
                },
                &rl,
                if self.show_bw_help {
                    "Measuring starts when both are running, in either order."
                } else {
                    "Measuring starts by itself. Your normal recording settings are fine."
                },
                |_| {},
            );
            if waiting && elapsed > 20.0 && !rec_on {
                ui.label(RichText::new("Still waiting. Check that OBS shows a red Record button and a running timer.").size(13.0).color(WARN));
            }
            if self.show_bw_help && platforms.iter().any(|p| p == "Twitch") {
                theme::small(
                    ui,
                    "Afterwards, remove ?bandwidthtest=true again, or your next stream won't reach viewers. Preflight keeps reminding you until you confirm.",
                );
            }
            ui.add_space(6.0);
            // What happened last time in this game (results are remembered per game).
            if let Some(g) = self.games.iter().find(|g| g.running)
                && let Some(m) = self.st.game_memory.get(&g.name)
            {
                let when = crate::util::fmt_local(m.time);
                theme::inset(ui, |ui| {
                    ui.label(RichText::new(format!("Last time in {} ({when})", g.name)).strong());
                    ui.label(
                        RichText::new(format!(
                            "Smoothness {} · Setup {}. {}",
                            m.smoothness, m.setup, m.verdict
                        ))
                        .color(TEXT_SOFT),
                    );
                    if !m.open.is_empty() {
                        theme::small(
                            ui,
                            format!(
                                "Still open then: {}",
                                m.open
                                    .iter()
                                    .take(3)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            ),
                        );
                    }
                });
            }
            // Before another test: offer to delete the last test recording.
            if !matches!(phase, Some(Phase::Measuring))
                && let Some(rec) = self
                    .st
                    .pending_recordings
                    .iter()
                    .max_by_key(|r| r.created)
                    .cloned()
            {
                rec_act = self.recording_row(ui, &rec, "Your last test recording is still saved:");
            }
            if self.quick_next {
                // Did the game save the change? And test the same spot.
                if let Some(prev) = self.prev_test.as_ref().map(|p| p.0.clone())
                    && let Some(g) = prev.game_running.clone()
                {
                    let now_saved = self.game_file_saved(&g);
                    let game_fix = self
                        .fixed
                        .iter()
                        .any(|f| analysis::place_of(f) == analysis::Place::Game);
                    if game_fix && prev.settings_saved.is_some() && now_saved == prev.settings_saved
                    {
                        status_row(
                            ui,
                            Status::Warn,
                            "Not saved",
                            &format!("{g} hasn't saved any change since your last test"),
                            "If you changed something in the game, press Apply (or leave its settings menu) so it saves, then test.",
                        );
                    }
                    theme::small(
                        ui,
                        format!("For a fair before/after, play the same part of {g} as last time."),
                    );
                }
            }
            ui.add_space(6.0);
            if link_small(
                ui,
                "Don't use OBS? Test without it (measures your PC for 2 minutes)",
            ) {
                action = Some("system");
            }
        });
        self.do_recording_act(ctx, rec_act);
        match action {
            Some("stop") => {
                if let Some(h) = &self.test_handle {
                    h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Some("to3") => self.set_step(3),
            Some("to4") => self.set_step(4),
            Some("results") => self.go(Screen::Results),
            Some("again") => {
                // The last kind of test the user picked.
                self.quick_next = self.st.last_test == "quick";
                self.show_bw_help = self.st.last_test == "stream_record";
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
            }
            Some("bw_help") => self.show_bw_help = !self.show_bw_help,
            Some("bw_copied") => self.toast(
                ctx,
                "Copied. Paste it at the very end of your stream key in OBS.".to_string(),
            ),
            Some("measure_next_stream") => {
                self.st.measure_next_stream = true;
                self.st.save();
                self.show_bw_help = false;
                self.toast(ctx, "OK. Preflight will measure your stream and recording together during your next real stream.".to_string());
            }
            Some("stream_record_now") => {
                self.show_bw_help = true;
                self.quick_next = false;
                self.st.last_test = "stream_record".into();
                self.st.save();
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
                if let Some(h) = &self.test_handle
                    && !matches!(h.phase(), Phase::Measuring)
                {
                    h.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.test_handle = None;
                }
            }
            Some("bw_added") => {
                self.st.bandwidth_test_pending = true;
                self.st.save();
                log::info!("User added the Twitch bandwidth-test flag for a stream + record test");
            }
            Some("system") => {
                self.stop_watching_if_idle();
                self.test_handle = None;
                self.want_new_test = true;
                self.start_test(ctx, true);
            }
            _ => {}
        }
    }

    pub(super) fn test_summary(&self, t: &TestResult) -> Vec<(String, String, Color32)> {
        let mut v = Vec::new();
        match t.gpu_avg {
            Some(g) if g >= 92.0 => v.push((
                format!("Your GPU was maxed out, at {g:.0}% on average."),
                "No room left for the stream encoder.".to_string(),
                BAD,
            )),
            Some(g) => v.push((
                format!("Your GPU averaged {g:.0}%."),
                "There's room left for the stream encoder.".to_string(),
                GOOD,
            )),
            None => v.push((
                "GPU usage couldn't be read on this card.".to_string(),
                "CPU, OBS and heat were still measured.".to_string(),
                MUTED,
            )),
        }
        if t.with_obs && t.output_total > 0 {
            let f = t.output_skipped as f32 / t.output_total as f32;
            if f > 0.001 {
                v.push((
                    format!(
                        "OBS skipped {:.1}% of stream frames, {}.",
                        f * 100.0,
                        if f > 0.0 {
                            format!("about 1 in every {}", (1.0 / f).round() as u32)
                        } else {
                            "none".into()
                        }
                    ),
                    "Viewers would see occasional stutters.".into(),
                    WARN,
                ));
            } else {
                v.push((
                    "OBS didn't skip any stream frames.".into(),
                    "The encoder kept up the whole time.".into(),
                    GOOD,
                ));
            }
        }
        if let (Some(m), Some(d)) = (t.mic_db, t.desktop_db) {
            if m < d - 3.0 {
                v.push((
                    format!(
                        "Your mic was quieter than the game, by about {:.0} dB.",
                        d - m
                    ),
                    analysis::mic_gap_meaning(d - m).into(),
                    WARN,
                ));
            } else {
                v.push((
                    "Your voice sits above the game audio.".into(),
                    "Viewers will hear you clearly.".into(),
                    GOOD,
                ));
            }
        }
        if t.cpu_avg >= 85.0 {
            v.push((
                format!(
                    "Your CPU was nearly maxed out, at {:.0}% on average.",
                    t.cpu_avg
                ),
                "The game and OBS were competing for it.".into(),
                BAD,
            ));
        }
        if let Some(e) = t.enc_avg {
            if e >= 85.0 {
                v.push((
                    format!("Your encoder chip was {e:.0}% busy on average."),
                    "It's close to its limit, so busy moments can make OBS skip frames.".into(),
                    WARN,
                ));
            } else {
                v.push((
                    format!("Your encoder chip was {e:.0}% busy on average."),
                    "It has room to spare.".into(),
                    GOOD,
                ));
            }
        }
        if let Some(o) = t.outputs.iter().find(|o| o.label == "Stream") {
            let f = o.dropped as f32 / o.total.max(1) as f32;
            v.push((
                format!("You streamed during the test, and your internet dropped {:.2}% of stream frames.", f * 100.0),
                if f < 0.001 { "Your connection handled the stream and recording together.".into() } else { "Your connection struggled while streaming.".into() },
                if f < 0.001 { GOOD } else { WARN },
            ));
        }
        // Every other output (separate recording, plugin outputs, extra streams), side by side.
        let others: Vec<String> = t
            .outputs
            .iter()
            .filter(|o| o.label != "Stream")
            .map(|o| {
                format!(
                    "{}: {:.2}% skipped",
                    o.label,
                    o.dropped as f32 * 100.0 / o.total.max(1) as f32
                )
            })
            .collect();
        if t.outputs.len() > 1 && !others.is_empty() {
            let bad = t
                .outputs
                .iter()
                .any(|o| o.dropped as f32 / o.total.max(1) as f32 > 0.005);
            v.push((
                format!("Each output: {}", others.join(" · ")),
                if bad {
                    "One of them fell behind: see which one above.".into()
                } else {
                    "All of them kept up.".into()
                },
                if bad { WARN } else { GOOD },
            ));
        }
        if !t.encodes_seen.is_empty() {
            v.push((
                format!(
                    "GPU encodes running: {}",
                    t.encodes_seen
                        .iter()
                        .map(|s| format!("{} {} {}", s.app, s.codec, s.res)
                            .trim()
                            .to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "Counted from your graphics driver.".into(),
                MUTED,
            ));
        }
        v
    }

    // -----------------------------------------------------------------------
    // Viewer preview (in Results)

    pub(super) fn ui_viewer_preview(&mut self, ui: &mut Ui) {
        if std::mem::take(&mut self.scroll_to_preview) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        let mut action: Option<&str> = None;
        let target = self.obs.as_ref().and_then(|o| o.bitrate_kbps);
        let separate_rec = self
            .obs
            .as_ref()
            .map(|o| !o.rec_encoder_raw.is_empty())
            .unwrap_or(false);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Viewer preview").size(21.0).strong());
            if separate_rec {
                ui.label(RichText::new("Your recording has its own encoder, so this shows your recording, not exactly what viewers get. For a true viewer preview, record with the stream encoder for the test.").size(13.0).color(WARN));
            }
            if self.test.is_none() {
                theme::muted(
                    ui,
                    "Run the test (Test in the sidebar). Afterwards you'll see your stream next to the original, as viewers would see it.",
                );
                return;
            }
            if self.preview_rx.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Making your viewer preview…");
                });
                return;
            }
            let Some(p) = self.preview.clone() else {
                theme::muted(
                    ui,
                    "This test was made before the viewer preview existed. Run another test to see one.",
                );
                return;
            };
            if p.pairs.is_empty() {
                theme::muted(ui, p.error.as_deref().unwrap_or("No preview frames."));
                return;
            }
            let n = p.pairs.len();
            let i = self.preview_idx.min(n - 1);
            let pair = p.pairs[i].clone();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(i > 0, egui::Button::new("Previous"))
                    .clicked()
                {
                    action = Some("prev");
                }
                ui.label(
                    RichText::new(format!(
                        "Moment {} of {n} · {} into the recording",
                        i + 1,
                        crate::streamwatch::fmt_t(pair.t_sec)
                    ))
                    .color(TEXT_SOFT),
                );
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new("Next"))
                    .clicked()
                {
                    action = Some("next");
                }
                ui.add_space(12.0);
                ui.checkbox(&mut self.preview_zoom, "Zoom to actual size");
            });
            match pair.match_db {
                Some(db) => {
                    let (head, meaning) = crate::preview::meaning(db);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{head}.")).size(18.0).strong().color(
                            if db >= 36.0 {
                                GOOD
                            } else if db >= 32.0 {
                                WARN
                            } else {
                                BAD
                            },
                        ));
                        ui.label(RichText::new(meaning).color(TEXT_SOFT));
                        ui.label(
                            RichText::new(format!("(picture match {db:.0} dB)"))
                                .size(12.5)
                                .color(MUTED),
                        );
                    });
                }
                None if pair.viewer.is_some() => {
                    ui.label(RichText::new("Preflight couldn't line these two frames up exactly (probably fast movement), so there's no score for this moment. Try another moment.").color(WARN));
                }
                None => {}
            }
            ui.add_space(6.0);
            if self.preview_tex.len() != n {
                self.preview_tex = vec![None; n];
            }
            if self.preview_tex[i].is_none() {
                let ctx = ui.ctx().clone();
                self.preview_tex[i] = Some((
                    load_tex(&ctx, &pair.original, &format!("orig-{i}")),
                    pair.viewer
                        .as_ref()
                        .and_then(|v| load_tex(&ctx, v, &format!("view-{i}"))),
                ));
            }
            let (orig_tex, view_tex) = self.preview_tex[i].clone().unwrap_or((None, None));
            let zoom = self.preview_zoom;
            let right_label = if separate_rec {
                "Your recording".to_string()
            } else {
                format!(
                    "Viewers{}",
                    target.map(|b| format!(" at {b} kbps")).unwrap_or_default()
                )
            };
            if let Some(o) = &orig_tex {
                let [tw, th] = o.size();
                let (tw, th) = (tw as f32, th as f32);
                let w = ui.available_width();
                let (size, uv) = if zoom {
                    let ww = w.min(tw);
                    let hh = (ww * 0.5).min(th);
                    let (fw, fh) = (ww / tw, hh / th);
                    (
                        Vec2::new(ww, hh),
                        egui::Rect::from_min_size(
                            egui::pos2(0.5 - fw / 2.0, 0.5 - fh / 2.0),
                            Vec2::new(fw, fh),
                        ),
                    )
                } else {
                    (
                        Vec2::new(w, w * th / tw),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    )
                };
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                if let (true, Some(pos)) = (
                    resp.dragged() || resp.clicked(),
                    resp.interact_pointer_pos(),
                ) {
                    self.preview_split = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                }
                let painter = ui.painter_at(rect);
                let split = if view_tex.is_some() {
                    self.preview_split
                } else {
                    1.0
                };
                let sx = rect.left() + rect.width() * split;
                let lr = egui::Rect::from_min_max(rect.min, egui::pos2(sx, rect.max.y));
                let luv = egui::Rect::from_min_max(
                    uv.min,
                    egui::pos2(uv.min.x + uv.width() * split, uv.max.y),
                );
                painter.image(o.id(), lr, luv, Color32::WHITE);
                if let Some(v) = &view_tex {
                    let rr = egui::Rect::from_min_max(egui::pos2(sx, rect.min.y), rect.max);
                    let ruv = egui::Rect::from_min_max(
                        egui::pos2(uv.min.x + uv.width() * split, uv.min.y),
                        uv.max,
                    );
                    painter.image(v.id(), rr, ruv, Color32::WHITE);
                    painter.line_segment(
                        [egui::pos2(sx, rect.min.y), egui::pos2(sx, rect.max.y)],
                        egui::Stroke::new(2.0, ACCENT),
                    );
                    let tag = |p: egui::Pos2, text: &str, align: egui::Align2| {
                        let g = painter.layout_no_wrap(
                            text.to_string(),
                            egui::FontId::proportional(14.0),
                            TEXT,
                        );
                        let r = align.anchor_size(p, g.size()).expand(5.0);
                        painter.rect_filled(r, 6.0, Color32::from_black_alpha(170));
                        painter.galley(r.min + Vec2::splat(5.0), g, TEXT);
                    };
                    tag(
                        rect.left_top() + Vec2::new(8.0, 8.0),
                        "Original (what OBS draws)",
                        egui::Align2::LEFT_TOP,
                    );
                    tag(
                        rect.right_top() + Vec2::new(-8.0, 8.0),
                        &right_label,
                        egui::Align2::RIGHT_TOP,
                    );
                }
                if view_tex.is_some() {
                    ui.add(
                        egui::Slider::new(&mut self.preview_split, 0.0..=1.0)
                            .show_value(false)
                            .text("Drag across the picture (or this slider) to compare"),
                    );
                }
            }
            if view_tex.is_none() {
                if p.ffmpeg_missing {
                    theme::inset(ui, |ui| {
                        ui.label(RichText::new("Preflight needs ffmpeg to read your test recording, to show the viewer side.").strong());
                        theme::muted(
                            ui,
                            if cfg!(windows) {
                                "Install it once: open Terminal and run  winget install ffmpeg  then click Try again."
                            } else {
                                "Install it once with your package manager (CachyOS/Arch: sudo pacman -S ffmpeg), then click Try again."
                            },
                        );
                        ui.horizontal(|ui| {
                            if theme::secondary(ui, "Try again").clicked() {
                                action = Some("retry");
                            }
                            if theme::secondary(ui, "Open the recording instead").clicked() {
                                action = Some("open");
                            }
                        });
                    });
                } else if let Some(e) = &p.error {
                    theme::muted(ui, e);
                }
            }
            if let Some(sim) = &p.simulated {
                theme::small(ui, sim);
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                theme::small(ui, "Preflight compares fine detail between the two pictures. Internet problems aren't included; the post-flight summary covers those.");
                if link_small(ui, "This looks wrong") {
                    action = Some("report");
                }
            });
        });
        match action {
            Some("prev") => self.preview_idx = self.preview_idx.saturating_sub(1),
            Some("next") => self.preview_idx += 1,
            Some("retry") => {
                if let Some(t) = self.test.clone() {
                    let ctx = ui.ctx().clone();
                    self.start_preview(&ctx, &t);
                }
            }
            Some("open") => {
                if let Some(p) = self.test.as_ref().and_then(|t| t.recording_path.clone()) {
                    crate::preview::open_in_player(&p);
                }
            }
            Some("report") => {
                let db = self
                    .preview
                    .as_ref()
                    .and_then(|p| p.pairs.get(self.preview_idx))
                    .and_then(|x| x.match_db);
                self.open_report(format!(
                    "Viewer preview: moment {} matched at {:?} dB",
                    self.preview_idx + 1,
                    db.map(|d| d.round())
                ));
            }
            _ => {}
        }
    }
}
