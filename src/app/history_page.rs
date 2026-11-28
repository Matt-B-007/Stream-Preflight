// SPDX-License-Identifier: GPL-3.0-or-later
//! History: the last stream and everything you tested and changed.

use super::*;

impl App {
    // -----------------------------------------------------------------------
    // History

    pub(super) fn ui_history(&mut self, ui: &mut Ui) {
        ui.label(theme::caps(
            "History · after you stream",
            12.0,
            pal().accent,
        ));
        theme::title(ui, "All tests and changes");
        theme::subtitle(ui, "What you changed, and what it did.");
        ui.add_space(6.0);
        self.history_switch(ui);
        // Timeline: tests, changes and streams, newest first.
        enum Ev {
            Test(history::Entry, Option<u8>),
            Change(crate::config::ChangeNote),
            Stream(i64, String),
        }
        let tests: Vec<&history::Entry> = self
            .history
            .entries
            .iter()
            .filter(|e| e.kind == "test")
            .collect();
        let mut evs: Vec<(i64, Ev)> = Vec::new();
        for (i, t) in tests.iter().enumerate() {
            let prev = if i > 0 {
                Some(tests[i - 1].score)
            } else {
                None
            };
            evs.push((t.time, Ev::Test((*t).clone(), prev)));
        }
        for c in &self.st.change_log {
            evs.push((c.time, Ev::Change(c.clone())));
        }
        for st in &self.streams {
            let p = crate::streamwatch::summarize(st);
            evs.push((
                st.started_at,
                Ev::Stream(
                    st.started_at,
                    format!(
                        "{} · {}{}",
                        crate::streamwatch::fmt_duration(p.duration),
                        p.grade,
                        if st.bandwidth_test {
                            " (test stream)"
                        } else {
                            ""
                        }
                    ),
                ),
            ));
        }
        evs.sort_by_key(|e| -e.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("What changed").size(16.0).strong());
            if evs.is_empty() {
                theme::muted(
                    ui,
                    "Nothing yet. Run a test, make a fix, and test again: each step shows up here with what it did.",
                );
            }
            for (_, e) in evs.iter().take(40) {
                ui.horizontal_wrapped(|ui| match e {
                    Ev::Test(t, prev) => {
                        ui.label(
                            RichText::new(history::fmt_time(t.time))
                                .monospace()
                                .color(pal().muted),
                        );
                        theme::pill(ui, pal().info, pal().control, "Test");
                        let delta = prev
                            .map(|p| {
                                let d = t.score as i32 - p as i32;
                                if d == 0 {
                                    String::new()
                                } else {
                                    format!(" ({d:+})")
                                }
                            })
                            .unwrap_or_default();
                        let scores = match (t.smooth, t.setup) {
                            (Some(sm), Some(se)) => format!("Smoothness {sm} · Setup {se}"),
                            _ => format!("score {}{delta}", t.score),
                        };
                        ui.label(
                            RichText::new(format!(
                                "{}{scores}",
                                t.game
                                    .as_ref()
                                    .map(|g| format!("{g}: "))
                                    .unwrap_or_default()
                            ))
                            .strong(),
                        );
                        let mut bits = Vec::new();
                        if let Some(m) = t.maxed_pct {
                            bits.push(format!("GPU maxed {m:.0}% of the time"));
                        } else if let Some(g) = t.gpu_avg {
                            bits.push(format!("GPU {g:.0}%"));
                        }
                        if let Some(sk) = t.skipped_pct {
                            bits.push(format!("skipped frames {sk:.2}%"));
                        }
                        ui.label(RichText::new(bits.join(" · ")).color(pal().text_soft));
                    }
                    Ev::Change(c) => {
                        ui.label(
                            RichText::new(history::fmt_time(c.time))
                                .monospace()
                                .color(pal().muted),
                        );
                        tag_pill(ui, analysis::Tag::Change);
                        ui.label(&c.text);
                    }
                    Ev::Stream(t, text) => {
                        ui.label(
                            RichText::new(history::fmt_time(*t))
                                .monospace()
                                .color(pal().muted),
                        );
                        theme::pill(ui, pal().success, pal().success_bg, "Stream");
                        ui.label(text);
                    }
                });
            }
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("More history: readiness chart, upload speed by hour, original game settings, saved setups").strong()).id_salt("more-history").show(ui, |ui| {
        let scores: Vec<f32> = self.history.entries.iter().filter(|e| e.score > 0).map(|e| e.score as f32).collect();
        theme::card(ui, |ui| {
            ui.label(RichText::new("Readiness over time").strong());
            if scores.is_empty() {
                theme::muted(ui, "No scans yet.");
            } else {
                theme::spark_bars(ui, &scores, 100.0, pal().accent, 80.0);
                theme::small(ui, format!("{} scans and tests", scores.len()));
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Your upload speed by time of day").strong());
            let hours = self.history.upload_by_hour();
            if hours.is_empty() {
                theme::muted(ui, "Scan at different times to see patterns, like your internet slowing down in the evening.");
            } else {
                egui::Grid::new("hours").num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Hour").color(pal().muted));
                    ui.label(RichText::new("Slowest upload (average)").color(pal().muted));
                    ui.label(RichText::new("Scans").color(pal().muted));
                    ui.end_row();
                    for (h, v, n) in hours {
                        ui.label(format!("{h:02}:00"));
                        ui.label(format!("{v:.1} Mbps"));
                        ui.label(n.to_string());
                        ui.end_row();
                    }
                });
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Original game settings").strong());
            theme::muted(ui, "What each flagged setting was the first time Preflight saw it, so you can go back.");
            if self.history.originals.is_empty() {
                theme::muted(ui, "Nothing recorded yet.");
            }
            for (game, m) in &self.history.originals {
                ui.label(RichText::new(game).strong());
                egui::Grid::new(format!("orig-{game}")).num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                    for (k, o) in m {
                        ui.label(k);
                        ui.label(format!("was {}", o.first_value));
                        ui.label(RichText::new(format!("now {}", o.last_value)).color(if o.first_value != o.last_value { pal().accent } else { pal().muted }));
                        ui.end_row();
                    }
                });
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Saved setups").strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.setup_name).hint_text("e.g. Ranked CS2").desired_width(220.0));
                if theme::secondary(ui, "Save current setup").clicked() && !self.setup_name.trim().is_empty() {
                    let s = history::SavedSetup {
                        name: self.setup_name.trim().to_string(),
                        time: crate::util::unix_now(),
                        score: self.analysis.score,
                        style: self.st.style_name().into(),
                        output: self.analysis.current_output.clone(),
                        encoder: self.obs.as_ref().map(|o| o.encoder_kind.clone()).unwrap_or_default(),
                        notes: self.test.as_ref().map(|t| format!("GPU avg {:?}, CPU avg {:.0}%", t.gpu_avg.map(|g| g.round()), t.cpu_avg)).unwrap_or_default(),
                    };
                    self.history.save_setup(s);
                    self.setup_name.clear();
                }
            });
            egui::Grid::new("setups").num_columns(4).spacing([20.0, 4.0]).show(ui, |ui| {
                for s in self.history.setups.iter().rev() {
                    ui.label(RichText::new(&s.name).strong());
                    ui.label(history::fmt_time(s.time));
                    ui.label(RichText::new(format!("{}", s.score)).color(theme::score_color(s.score)));
                    ui.label(format!("{} · {}", s.output, s.encoder));
                    ui.end_row();
                }
            });
        });
        });
    }

    // -----------------------------------------------------------------------
    // Post-flight

    /// Last stream | All history: the two views behind History in the sidebar.
    pub(super) fn history_switch(&mut self, ui: &mut Ui) {
        let mut pick: Option<Screen> = None;
        egui::Frame::NONE
            .fill(pal().card)
            .stroke(egui::Stroke::new(1.0, pal().border))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (t, sc) in [
                        ("Last stream", Screen::PostFlight),
                        ("All tests and changes", Screen::History),
                    ] {
                        let on = self.screen == sc;
                        let b = egui::Button::new(RichText::new(t).color(if on {
                            pal().text
                        } else {
                            pal().text_soft
                        }))
                        .fill(if on {
                            alpha(pal().accent, 40)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if on {
                            egui::Stroke::new(1.0, alpha(pal().accent, 120))
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(egui::CornerRadius::same(9));
                        if ui.add(b).clicked() && !on {
                            pick = Some(sc);
                        }
                    }
                });
            });
        ui.add_space(8.0);
        if let Some(sc) = pick {
            self.stream_sel = 0;
            self.go(sc);
        }
    }

    pub(super) fn ui_postflight(&mut self, ui: &mut Ui) {
        ui.label(theme::caps(
            "History · after you stream",
            12.0,
            pal().accent,
        ));
        theme::title(ui, "Last stream");
        theme::subtitle(
            ui,
            "How your stream actually went, and what to change next time.",
        );
        ui.add_space(6.0);
        self.history_switch(ui);
        if let Some(l) = self.live.clone() {
            theme::card(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Live now · {}",
                        crate::streamwatch::fmt_t(l.elapsed)
                    ))
                    .size(16.0)
                    .strong()
                    .color(pal().success),
                );
                ui.label(format!(
                    "{:.2}% of frames dropped so far{}.",
                    l.dropped_pct,
                    l.kbps
                        .map(|k| format!(", sending {k:.0} kbps right now"))
                        .unwrap_or_default()
                ));
                theme::muted(
                    ui,
                    "Preflight only reads OBS's stream stats, every 5 seconds. It keeps watching from the tray; your summary appears here when you end the stream.",
                );
            });
            ui.add_space(10.0);
        }
        if self.streams.is_empty() {
            if self.live.is_none() {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("No streams yet").size(16.0).strong());
                    theme::muted(
                        ui,
                        "Leave Preflight running while you stream (in the tray is fine). It watches OBS's stream stats without changing anything, and when you end the stream you'll get:",
                    );
                    for s in [
                        "How many frames your internet dropped, and when",
                        "Whether the encoder or the GPU fell behind",
                        "Your real bitrate, including any dips",
                        "The worst moments, with timestamps",
                        "What to change before your next stream",
                    ] {
                        theme::small(ui, format!("• {s}"));
                    }
                });
            }
            return;
        }
        let sel = self.stream_sel.min(self.streams.len() - 1);
        let l = self.streams[sel].clone();
        let p = crate::streamwatch::summarize(&l);
        let mut report = false;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&p.grade)
                            .size(22.0)
                            .family(theme::title_family())
                            .strong()
                            .color(grade_color(p.grade_level)),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {}{}{}",
                            history::fmt_time(l.started_at),
                            crate::streamwatch::fmt_duration(p.duration),
                            l.game
                                .as_ref()
                                .map(|g| format!(" · {g}"))
                                .unwrap_or_default(),
                            if l.output.is_empty() {
                                String::new()
                            } else {
                                format!(" · {} with {}", l.output, l.encoder)
                            }
                        ))
                        .color(pal().text_soft),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if link_small(ui, "This looks wrong") {
                        report = true;
                    }
                });
            });
            if let Some(n) = &l.partial {
                ui.label(RichText::new(n).size(12.0).color(pal().warn));
            }
            if l.bandwidth_test {
                ui.label(
                    RichText::new("Twitch bandwidth test: nobody could see this stream.")
                        .size(12.0)
                        .color(pal().info),
                );
            }
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(
                RichText::new("What to change next time")
                    .size(16.0)
                    .strong(),
            );
            if p.advice.is_empty() {
                // Say so when a number below was less than good, so the two agree.
                let watch: Vec<&str> = [
                    ("your internet", p.net_frac),
                    ("the encoder", p.enc_frac),
                    ("drawing frames", p.render_frac),
                ]
                .iter()
                .filter(|(_, f)| *f >= 0.001)
                .map(|(n, _)| *n)
                .collect();
                if watch.is_empty() {
                    theme::muted(ui, "Nothing. Keep the same settings next time.");
                } else {
                    theme::muted(
                        ui,
                        format!(
                            "No setting to change yet. Keep an eye on {}: the numbers below show a few lost frames. If it happens again, Preflight will suggest a fix.",
                            watch.join(" and ")
                        ),
                    );
                }
            }
            for a in &p.advice {
                theme::inset(ui, |ui| {
                    ui.horizontal(|ui| {
                        tag_pill(ui, analysis::Tag::Change);
                        ui.label(RichText::new(&a.title).strong());
                    });
                    theme::muted(ui, &a.why);
                    ui.label(RichText::new(&a.menu).size(12.0).color(pal().muted));
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(10.0);
        let cards = [
            ("Internet", p.net_sentence(), p.net_frac),
            ("Encoder", p.enc_sentence(), p.enc_frac),
            ("Drawing frames", p.render_sentence(), p.render_frac),
        ];
        ui.columns(3, |c| {
            for (i, (name, (head, meaning), f)) in cards.iter().enumerate() {
                theme::card(&mut c[i], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(*name).strong());
                        let (s, lab) = if *f < 0.001 {
                            (Status::Ok, "Good")
                        } else if *f < 0.01 {
                            (Status::Warn, "Check")
                        } else {
                            (Status::Bad, "Fix")
                        };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            theme::status_pill(ui, s, lab)
                        });
                    });
                    ui.label(RichText::new(head).strong());
                    theme::muted(ui, meaning);
                });
            }
        });
        if !p.outputs.is_empty() || p.recording.is_some() {
            ui.add_space(6.0);
            theme::inset(ui, |ui| {
                ui.label(RichText::new("Your other outputs during the stream").strong());
                if let Some(r) = &p.recording {
                    theme::small(ui, r);
                }
                if !p.outputs.is_empty() {
                    theme::small(ui, p.outputs.join(" · "));
                }
            });
        }
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Bitrate").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if p.reconnects > 0 {
                        theme::status_pill(
                            ui,
                            Status::Bad,
                            &format!(
                                "{} reconnect{}",
                                p.reconnects,
                                if p.reconnects == 1 { "" } else { "s" }
                            ),
                        );
                    }
                });
            });
            let target = l
                .target_kbps
                .map(|b| format!(" of the {b} you set"))
                .unwrap_or_default();
            ui.label(
                RichText::new(format!(
                    "Your stream averaged {:.0} kbps{target}.",
                    p.avg_kbps
                ))
                .strong(),
            );
            if let Some((low, t)) = p.low_kbps.filter(|(low, _)| *low < p.avg_kbps * 0.9) {
                theme::muted(
                    ui,
                    format!(
                        "The lowest 30 seconds averaged {low:.0} kbps, around {}.",
                        crate::streamwatch::fmt_t(t)
                    ),
                );
            }
            let max = l
                .target_kbps
                .map(|b| b as f32 * 1.3)
                .unwrap_or_else(|| p.kbps_series.iter().cloned().fold(1.0, f32::max));
            theme::spark_bars(ui, &p.kbps_series, max, pal().accent, 60.0);
            theme::small(
                ui,
                format!(
                    "CPU averaged {:.0}%{}{}{}.{}",
                    p.cpu_avg,
                    p.gpu_avg
                        .map(|g| format!(", GPU {g:.0}%"))
                        .unwrap_or_default(),
                    p.enc_avg
                        .map(|e| format!(", encoder chip {e:.0}%"))
                        .unwrap_or_default(),
                    p.gpu_temp_max
                        .map(|t| format!(", GPU peaked at {t:.0}°C"))
                        .unwrap_or_default(),
                    match (p.own_cpu_avg, p.own_mb_max) {
                        (Some(c), Some(m)) => format!(
                            " Preflight itself used {} and {m:.0} MB of memory.",
                            crate::util::one_core(c)
                        ),
                        _ => String::new(),
                    }
                ),
            );
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("Worst moments").size(16.0).strong());
            if p.moments.is_empty() {
                theme::muted(ui, "No rough moments. Nice.");
            }
            for m in &p.moments {
                ui.horizontal_wrapped(|ui| {
                    let when = if m.secs > 0.0 {
                        format!(
                            "{}–{}",
                            crate::streamwatch::fmt_t(m.t),
                            crate::streamwatch::fmt_t(m.t + m.secs)
                        )
                    } else {
                        crate::streamwatch::fmt_t(m.t)
                    };
                    ui.label(RichText::new(when).monospace().color(pal().accent));
                    ui.label(&m.text);
                });
            }
            theme::small(
                ui,
                "Times are from the start of the stream, so you can find them in your VOD.",
            );
        });
        ui.add_space(10.0);
        if self.streams.len() > 1 {
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(RichText::new("Past streams").size(16.0).strong());
                let mut pick = None;
                for (i, s) in self.streams.iter().enumerate().take(20) {
                    let sp = crate::streamwatch::summarize(s);
                    let label = format!(
                        "{} · {} · {}{}",
                        history::fmt_time(s.started_at),
                        crate::streamwatch::fmt_duration(sp.duration),
                        sp.grade,
                        if s.bandwidth_test { " · test" } else { "" }
                    );
                    if ui.selectable_label(i == sel, label).clicked() {
                        pick = Some(i);
                    }
                }
                if let Some(i) = pick {
                    self.stream_sel = i;
                }
            });
        }
        if report {
            self.open_report(format!(
                "Summary for the stream at {}: {}, internet dropped {:.2}%, encoder skipped {:.2}%, rendering skipped {:.2}%, avg {:.0} kbps, {} reconnects",
                history::fmt_time(l.started_at),
                p.grade,
                p.net_frac * 100.0,
                p.enc_frac * 100.0,
                p.render_frac * 100.0,
                p.avg_kbps,
                p.reconnects
            ));
        }
    }
}
