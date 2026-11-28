// SPDX-License-Identifier: GPL-3.0-or-later
//! Help, the problem report box and what goes into a report.

use super::*;

impl App {
    /// Extra bug-report sections: what the rest of the report can't see.
    /// Numbers, Preflight's own labels and setting ids only.
    pub(super) fn report_extras(&self) -> Vec<(&'static str, &'static str, String)> {
        use std::fmt::Write;
        let mut v = Vec::new();
        // Post-flight
        let mut t = String::new();
        match &self.live {
            Some(l) => {
                let _ = writeln!(
                    t,
                    "Live now: {} · {:.2}% dropped · {}",
                    crate::streamwatch::fmt_t(l.elapsed),
                    l.dropped_pct,
                    l.kbps
                        .map(|k| format!("{k:.0} kbps"))
                        .unwrap_or_else(|| "kbps unknown".into())
                );
            }
            None => {
                let _ = writeln!(t, "Not live.");
            }
        }
        let _ = writeln!(
            t,
            "Real-stream question showing: {} · user said real: {}",
            self.flag_question, self.flag_real
        );
        if let Some(l) = self.streams.first() {
            let p = crate::streamwatch::summarize(l);
            let _ = writeln!(
                t,
                "\nLast stream: started {} · {} · {} (level {}) · labelled {}",
                history::fmt_time(l.started_at),
                crate::streamwatch::fmt_duration(p.duration),
                p.grade,
                p.grade_level,
                if l.bandwidth_test {
                    "test stream"
                } else {
                    "real stream"
                }
            );
            let _ = writeln!(
                t,
                "Target {} kbps · avg {:.0} kbps · lowest 30 s {} · reconnects {}",
                l.target_kbps
                    .map(|k| k.to_string())
                    .unwrap_or_else(|| "?".into()),
                p.avg_kbps,
                p.low_kbps
                    .map(|(k, at)| format!("{k:.0} kbps at {at:.0} s"))
                    .unwrap_or_else(|| "-".into()),
                p.reconnects
            );
            let _ = writeln!(
                t,
                "Frames lost to: network {:.3}% · encoder {:.3}% · rendering {:.3}%",
                p.net_frac * 100.0,
                p.enc_frac * 100.0,
                p.render_frac * 100.0
            );
            let _ = writeln!(
                t,
                "CPU {:.0}% · GPU {} · GPU temp max {} · encoder {} · Preflight's own CPU {} / {}",
                p.cpu_avg,
                p.gpu_avg
                    .map(|g| format!("{g:.0}%"))
                    .unwrap_or_else(|| "-".into()),
                p.gpu_temp_max
                    .map(|g| format!("{g:.0}°C"))
                    .unwrap_or_else(|| "-".into()),
                p.enc_avg
                    .map(|g| format!("{g:.0}%"))
                    .unwrap_or_else(|| "-".into()),
                p.own_cpu_avg
                    .map(|g| format!("{} ({g:.3}% of all)", crate::util::one_core(g)))
                    .unwrap_or_else(|| "-".into()),
                match (p.own_mb_start, p.own_mb_end, p.own_mb_max) {
                    (Some(a), Some(b), Some(m)) =>
                        format!("RAM {a:.0} MB at start, {b:.0} MB at end, {m:.0} MB peak"),
                    _ => "-".into(),
                }
            );
            let _ = writeln!(
                t,
                "Recording during the stream: {}",
                p.recording.clone().unwrap_or_else(|| "none".into())
            );
            let _ = writeln!(t, "Encoder: {} · samples: {}", l.encoder, l.samples.len());
            for o in &l.outputs {
                let (s0, t0) = o.1;
                let (s1, t1) = o.2;
                let _ = writeln!(
                    t,
                    "Output {}: skipped {} of {} frames during the stream",
                    o.0,
                    s1.saturating_sub(s0),
                    t1.saturating_sub(t0)
                );
            }
            for o in &p.outputs {
                let _ = writeln!(t, "  {o}");
            }
            for a in &p.advice {
                let _ = writeln!(t, "Advice shown: {}", a.title);
            }
        }
        if self.streams.len() > 1 {
            let _ = writeln!(t, "\nEarlier streams:");
            for l in self.streams.iter().skip(1).take(10) {
                let p = crate::streamwatch::summarize(l);
                let _ = writeln!(
                    t,
                    "- {} · {} · {}{}",
                    history::fmt_time(l.started_at),
                    crate::streamwatch::fmt_duration(p.duration),
                    p.grade,
                    if l.bandwidth_test { " · test" } else { "" }
                );
            }
        }
        v.push(("postflight", "Stream summary (numbers only)", t));

        // Test history and what changed between tests
        let mut t = String::new();
        if let Some(c) = &self.compare {
            let _ = writeln!(
                t,
                "Last before/after: Smoothness {} → {}",
                c.before_score, c.after_score
            );
            if let (Some(a), Some(b)) = (&c.before.obs_settings, &c.after.obs_settings) {
                let ch = a.changes_to(b);
                let _ = writeln!(
                    t,
                    "Changed between the tests: {}",
                    if ch.is_empty() {
                        "nothing in OBS".into()
                    } else {
                        ch.join("; ")
                    }
                );
            }
        } else {
            let _ = writeln!(t, "No before/after comparison yet.");
        }
        if let Some((p, sc)) = &self.prev_test {
            let _ = writeln!(
                t,
                "Previous test: {} · {:.0} s · Smoothness {sc}",
                history::fmt_time(p.started_at),
                p.seconds
            );
        }
        for (g, m) in &self.st.game_memory {
            let _ = writeln!(
                t,
                "Saved results: {g} · {} · Smoothness {} · Setup {} · {} open",
                crate::util::fmt_local(m.time),
                m.smoothness,
                m.setup,
                m.open.len()
            );
        }
        for c in self.st.change_log.iter().rev().take(15) {
            let _ = writeln!(t, "Change note {}: {}", history::fmt_time(c.time), c.text);
        }
        v.push(("tests", "Test history and changes", t));

        // One setup for all games
        let f = &self.fleet;
        let mut t = format!(
            "Setup: {} · motion {} · busy {} · heaviest {} · warning {}\n",
            if f.setup.is_empty() { "-" } else { &f.setup },
            f.motion,
            f.busy,
            f.heaviest.clone().unwrap_or_else(|| "-".into()),
            f.warning.clone().unwrap_or_else(|| "none".into())
        );
        for l in &f.tested {
            let _ = writeln!(
                t,
                "- {} · GPU {} · maxed {:.0}% · skipped {:.2}% · motion {} · {}",
                l.game,
                l.gpu_avg
                    .map(|g| format!("{g:.0}%"))
                    .unwrap_or_else(|| "-".into()),
                l.maxed_pct,
                l.skipped_pct,
                l.motion,
                l.verdict
            );
        }
        if !f.untested.is_empty() {
            let _ = writeln!(t, "Not tested: {}", f.untested.join(", "));
        }
        v.push(("fleet", "One setup for all games", t));

        // What was on screen
        let mut t = String::new();
        let _ = writeln!(
            t,
            "Screen {:?} · step {} · banner {} · game shown {:?} · style {}",
            self.screen,
            self.step,
            self.banner_pick(),
            self.current_game(),
            style_label(self.st.style)
        );
        let phase = self
            .test_handle
            .as_ref()
            .map(|h| format!("{:?}", h.phase()))
            .unwrap_or_else(|| "no test running".into());
        let (rec, stream, el) = self
            .test_handle
            .as_ref()
            .and_then(|h| {
                h.state
                    .lock()
                    .ok()
                    .map(|s| (s.rec_active, s.stream_active, s.elapsed))
            })
            .unwrap_or((false, false, 0.0));
        let _ = writeln!(
            t,
            "Test chosen: {} · phase {phase} · OBS recording {rec} · streaming {stream} · elapsed {el:.0} s · flag help open {}",
            self.st.last_test, self.show_bw_help
        );
        let _ = writeln!(
            t,
            "Checklist: test flag marked added {} · ready to go live {} · pre-flight done {}",
            self.st.bandwidth_test_pending, self.go_ready, self.preflight_done
        );
        let (left, total) = self.fix_progress();
        let _ = writeln!(t, "Fixes: {left} of {total} left · at {}", self.fix_idx);
        for (n, i) in self.fix_queue.iter().enumerate() {
            let st = if self.fixed.contains(&i.id) {
                "changed"
            } else if self.st.kept_issues.contains(&i.id) {
                "kept"
            } else if self.skipped.contains(&i.id) {
                "skipped"
            } else {
                "open"
            };
            let _ = writeln!(
                t,
                "  {}. {} · {st}{}",
                n + 1,
                i.id,
                if analysis::is_optional(i) {
                    " · optional"
                } else {
                    ""
                }
            );
        }
        if let Some(c) = &self.confirm {
            let _ = writeln!(
                t,
                "Change-saved check: {} change(s) · {}",
                c.changes.len(),
                c.result
                    .as_ref()
                    .map(|r| format!(
                        "{} · {}",
                        if r.0 { "all saved" } else { "not all saved" },
                        r.1
                    ))
                    .unwrap_or_else(|| "waiting".into())
            );
        }
        if let Some(p) = &self.preview {
            let aligned = p.pairs.iter().filter(|x| x.aligned).count();
            let _ = writeln!(
                t,
                "Viewer preview: {} moments · {aligned} lined up · simulated: {} · ffmpeg missing {} · error {}",
                p.pairs.len(),
                p.simulated.clone().unwrap_or_else(|| "no".into()),
                p.ffmpeg_missing,
                p.error.clone().unwrap_or_else(|| "none".into())
            );
            for x in &p.pairs {
                let _ = writeln!(
                    t,
                    "  {:.0} s · match {} · aligned {}",
                    x.t_sec,
                    x.match_db
                        .map(|d| format!("{d:.1} dB"))
                        .unwrap_or_else(|| "-".into()),
                    x.aligned
                );
            }
        }
        v.push(("ui", "What was on screen", t));

        // Tray and desktop
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let mut t = format!(
            "Desktop: {} {} · window can hide: {} · hidden now: {}\n",
            if desktop.is_empty() {
                std::env::consts::OS
            } else {
                &desktop
            },
            session,
            self.can_hide,
            self.hidden
        );
        match &self.tray {
            Some(tr) => {
                let tv = tr.last_view();
                let _ = writeln!(
                    t,
                    "Tray: {} · needs you {} · \"{}\"",
                    tv.state.word(),
                    tv.attention,
                    tv.status
                );
            }
            None => {
                let _ = writeln!(
                    t,
                    "Tray: not available{}",
                    if self.tray_tried {
                        ""
                    } else {
                        " (not started yet)"
                    }
                );
            }
        }
        let notes: Vec<String> = crate::logger::recent(400)
            .into_iter()
            .filter(|l| l.contains("Notification:") || l.contains("Tray:"))
            .rev()
            .take(10)
            .collect();
        for n in notes.iter().rev() {
            let _ = writeln!(t, "{n}");
        }
        v.push(("tray", "Tray and desktop session", t));
        // Two PCs (no addresses, names or keys)
        let mut t = format!(
            "Role: {} · OBS on another computer: {} · paired: {} · link running: {}\n",
            self.st.pc_role,
            self.obs_remote(),
            self.st.paired,
            self.pairing_kind
        );
        if !self.pair_status.is_empty() {
            let _ = writeln!(t, "Pairing status: {}", self.pair_status);
        }
        if let Some(l) = self.peer_line() {
            let _ = writeln!(t, "{l}");
        }
        for c in self
            .obs
            .as_ref()
            .map(|o| o.captures.clone())
            .unwrap_or_default()
        {
            let _ = writeln!(
                t,
                "Capture: {} · range {} · in scene {} · audio offset {:?} ms",
                c.describe(),
                if c.range.is_empty() {
                    "default"
                } else {
                    &c.range
                },
                c.in_scene,
                c.audio_offset_ms
            );
        }
        if let Some(o) = &self.obs {
            let _ = writeln!(
                t,
                "OBS colour range {} · colour space {}",
                o.color_range, o.color_space
            );
        }
        if let Some(s) = &self.side_test {
            let _ = writeln!(t, "Gaming PC during the last test: {s:?}");
        }
        v.push(("twopc", "Two PCs", t));
        v
    }

    // -----------------------------------------------------------------------
    // Report a problem (one box)

    /// Builds the report's sections from what Preflight knows right now.
    fn report_sections(&mut self, rb: &ReportBox) -> Vec<report::Section> {
        self.selftests = report::self_test(&report::SelfTestInput {
            hw: self.hw.as_ref(),
            games: &self.games,
            obs: self.obs.as_ref(),
            obs_error: self.obs_error.as_deref(),
            net: self.net.as_ref(),
        });
        let extra = self.report_extras();
        let description = match (rb.summary.trim(), rb.note.trim()) {
            ("", "") => String::new(),
            (s, "") => s.to_string(),
            ("", n) => n.to_string(),
            (s, n) => format!("{s}\n{n}"),
        };
        report::build_sections(&report::ReportInput {
            context: &rb.context,
            description: &description,
            hw: self.hw.as_ref(),
            games: &self.games,
            obs: self.obs.as_ref(),
            obs_error: self.obs_error.as_deref(),
            net: self.net.as_ref(),
            test: self.test.as_ref(),
            analysis: &self.analysis,
            selftests: &self.selftests,
            settings: &self.st,
            encoders: &self.encoders,
            extra: &extra,
        })
    }

    /// "Report a problem", laid out like the other apps. Nothing is sent anywhere.
    pub(super) fn ui_report_box(&mut self, ctx: &egui::Context) {
        let Some(mut rb) = self.report.take() else {
            return;
        };
        if rb.dirty {
            rb.dirty = false;
            let sections = self.report_sections(&rb);
            rb.text = report::render(&sections, &rb.id);
        }
        let mut keep = true;
        let mut save = false;
        let mut github = false;
        let (_, esc) = modal(ctx, "report-dialog", "Report a problem", 640.0, |ui| {
            if !rb.context.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("About:").color(pal().muted));
                    ui.label(&rb.context);
                });
            }
            ui.label(RichText::new("Short summary").strong());
            let r1 = ui.add(
                egui::TextEdit::singleline(&mut rb.summary)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. The test never finishes"),
            );
            ui.label(RichText::new("What happened, and what did you expect?").strong());
            let r2 = ui.add(
                egui::TextEdit::multiline(&mut rb.note)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. V-Sync is off in my game, but Preflight says it's on"),
            );
            // Rebuilt when you finish typing, not on every key.
            if r1.lost_focus() || r2.lost_focus() {
                rb.dirty = true;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Details that will be included").strong());
            theme::small(
                ui,
                "Your PC, OBS settings (no stream key, no scene or source names), game settings files and recent activity. User names, home folders, Steam IDs and addresses are already removed.",
            );
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let mut shown = rb.text.as_str();
                    ui.add(
                        egui::TextEdit::multiline(&mut shown)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            if let Some(e) = &rb.error {
                ui.label(RichText::new(format!("Couldn't save the report: {e}")).color(pal().bad));
            }
            if let Some(p) = &rb.saved {
                // Wrapped and selectable, so the whole path can be read and copied.
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Saved:").color(pal().success));
                    ui.add(
                        egui::Label::new(
                            RichText::new(p.display().to_string())
                                .monospace()
                                .size(12.0),
                        )
                        .wrap()
                        .selectable(true),
                    );
                });
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if theme::primary(ui, "Save to file").clicked() {
                    save = true;
                }
                if theme::secondary(ui, if rb.copied { "Copied" } else { "Copy report" }).clicked()
                {
                    ui.ctx().copy_text(rb.text.clone());
                    report::copy_fallback(&rb.text);
                    rb.copied = true;
                }
                if theme::secondary(ui, "Open log folder").clicked() {
                    report::open_folder(&crate::util::dirs().logs);
                }
                if let Some(p) = &rb.saved {
                    if theme::secondary(ui, "Open report").clicked() {
                        report::open_file(p);
                    }
                    if theme::secondary(ui, "Show in folder").clicked() {
                        report::show_file(p);
                    }
                }
                if theme::secondary(ui, "Open on GitHub…").clicked() {
                    rb.github_confirm = true;
                }
                if theme::secondary(ui, "Close").clicked() {
                    keep = false;
                }
            });
            if rb.github_confirm {
                theme::inset(ui, |ui| {
                    ui.label(
                        RichText::new("This creates a PUBLIC issue on GitHub.")
                            .strong()
                            .color(pal().warn),
                    );
                    theme::small(
                        ui,
                        "Only the summary and the app and system lines are filled in. The report is saved to a file first; attach it to the issue yourself after reading it.",
                    );
                    ui.horizontal(|ui| {
                        if theme::primary(ui, "Save and open GitHub").clicked() {
                            github = true;
                            save = true;
                        }
                        if theme::secondary(ui, "Cancel").clicked() {
                            rb.github_confirm = false;
                        }
                    });
                });
            }
            theme::small(ui, "Nothing is sent until you choose to.");
        });
        if save {
            let sections = self.report_sections(&rb);
            match report::save(&sections, &rb.id) {
                Ok(p) => {
                    rb.saved = Some(p);
                    rb.error = None;
                    self.pending_crashes = crate::logger::pending_crashes().len();
                }
                Err(e) => rb.error = Some(e),
            }
        }
        if github && rb.saved.is_some() {
            ctx.open_url(egui::OpenUrl::new_tab(issue_url(&rb.summary)));
            rb.github_confirm = false;
        }
        if keep && !esc {
            self.report = Some(rb);
        }
    }

    // -----------------------------------------------------------------------
    // Help & report a problem

    /// Preflight's own CPU (share of the whole CPU) and memory.
    pub(super) fn measure_own_usage(&mut self) {
        let Ok(pid) = sysinfo::get_current_pid() else {
            return;
        };
        let first = self.own_sys.is_none();
        if first {
            self.own_sys = crate::hardware::new_system();
        }
        let Some(sys) = self.own_sys.as_mut() else {
            return;
        };
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), false);
        if let Some(p) = sys.process(pid) {
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1) as f32;
            // The first reading has no CPU time to compare with yet.
            if !first {
                self.own_usage = Some((p.cpu_usage() / cores, p.memory() as f32 / 1_048_576.0));
            }
        }
    }

    pub(super) fn ui_help(&mut self, ui: &mut Ui) {
        theme::title(ui, "Help & report a problem");
        ui.add_space(8.0);
        let mut report = false;
        theme::card(ui, |ui| {
            ui.label(RichText::new("Something not working?").size(16.0).strong());
            theme::muted(
                ui,
                "The quickest way is the \"This looks wrong\" link right next to the problem. For anything else:",
            );
            if theme::secondary(ui, "  Report a problem  ").clicked() {
                report = true;
            }
        });
        if report {
            self.open_report("");
        }
        ui.add_space(10.0);
        ui.label(RichText::new("What to do if…").size(16.0).strong());
        ui.add_space(4.0);
        let topics: [(&str, &[&str]); 6] = [
            (
                "OBS won't connect or asks for a password",
                &[
                    "In OBS, open Tools › WebSocket Server Settings.",
                    "Tick \"Enable WebSocket server\". Click \"Show Connect Info\" and copy the password.",
                    "Paste it in Preflight's Settings › OBS connection, then click Test connection.",
                    "Still nothing? Make sure OBS is open and the port matches (4455 unless you changed it, under Settings › Advanced).",
                ],
            ),
            (
                "My game isn't found, or says \"not launched yet\"",
                &[
                    "Preflight reads a game's settings file, which only exists after you've launched the game once and changed a setting.",
                    "Launch the game, open its graphics settings, change anything and change it back, then quit. Rescan in Preflight.",
                    "Settings › Advanced › Games shows exactly where Preflight looked.",
                    "Games without a profile still get tested: Preflight finds them by their GPU use while you play.",
                ],
            ),
            (
                "The test won't start",
                &[
                    "The test starts by itself when OBS starts recording. Check the OBS line in Step 1 says Connected.",
                    "If OBS is connected, press Record in OBS (or Start Streaming). You can also press Start the test by hand.",
                    "Play for at least 2 minutes so Preflight sees real game load.",
                ],
            ),
            (
                "The viewer preview is missing",
                &[
                    "The preview needs ffmpeg, a free video tool. Install it from your package manager (Linux) or ffmpeg.org (Windows), then restart Preflight.",
                    "It also needs your test recording, so leave it where OBS saved it until the preview is made.",
                    "\"Couldn't line up\" means the stream and recording frames didn't match. Run the test again with the same scene showing.",
                ],
            ),
            (
                "The upload test failed",
                &[
                    "Preflight sends a few short bursts of data to measure your upload. A firewall, VPN or busy connection can stop it.",
                    "Pause big downloads or uploads and try again: run the network test again from Your setup.",
                    "For a real Twitch measurement, add ?bandwidthtest=true to your stream key and stream for a minute. Remove it afterwards: Preflight reminds you.",
                ],
            ),
            (
                "Something looks wrong",
                &[
                    "Click \"This looks wrong\" next to it, or Report a problem above.",
                    "The report has no passwords, stream keys, names or scene names. You can read it before you send it.",
                    "If it only happens sometimes, turn on \"Record more detail\" under Advanced tools below, make it happen, then report.",
                ],
            ),
        ];
        for (i, (q, steps)) in topics.iter().enumerate() {
            let open = self.help_open == Some(i);
            let mut toggle = false;
            theme::card(ui, |ui| {
                let r = egui::CollapsingHeader::new(RichText::new(*q).strong())
                    .id_salt(("help-q", i))
                    .open(Some(open))
                    .show(ui, |ui| {
                        for (n, st) in steps.iter().enumerate() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("{}.", n + 1)).strong());
                                ui.label(*st);
                            });
                        }
                        if i == 0 && ui.button("Open OBS connection settings").clicked() {
                            self.go(Screen::Settings);
                        }
                        if i == 3 && crate::preview::ffmpeg_path().is_some() {
                            theme::small(ui, "ffmpeg is installed.");
                        }
                    });
                if r.header_response.clicked() {
                    toggle = true;
                }
            });
            if toggle {
                self.help_open = if open { None } else { Some(i) };
            }
            ui.add_space(6.0);
        }
        theme::card(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("I want to remove Preflight").strong()).id_salt("help-remove").show(ui, |ui| {
                ui.label("Settings › About & uninstall › Uninstall removes the program, and with \"Also delete my settings and logs\" ticked, everything below. Or delete these folders yourself:");
                for (what, path) in crate::uninstall::own_folders() {
                    theme::small(ui, format!("• {what}: {}", crate::util::scrub(&path.to_string_lossy())));
                }
                if cfg!(not(windows)) {
                    theme::small(ui, "• App menu entry and icon: ~/.local/share/applications/stream-preflight.desktop and ~/.local/share/icons/hicolor/*/apps/stream-preflight.png");
                }
                theme::small(ui, "Then delete the Preflight program file. That's all: Preflight doesn't install anything else.");
            });
            egui::CollapsingHeader::new(RichText::new("Using Preflight with the keyboard").strong()).id_salt("help-keys").show(ui, |ui| {
                theme::small(ui, "• Tab and Shift+Tab move between buttons; Enter or Space presses the one that's outlined.");
                theme::small(ui, "• Ctrl+Enter does the Next step shown at the bottom.");
                theme::small(ui, "• Alt+1 Home, Alt+2 Test, Alt+3 Fixes, Alt+4 Live, Alt+5 History. Ctrl+B folds the sidebar.");
                theme::small(ui, "• Ctrl+K searches pages and commands, Ctrl+, opens Settings, ? lists every shortcut, Ctrl+Q quits.");
                theme::small(ui, "• Every colour also has a word next to it (Change, Keep, OK, Great…), so nothing depends on colour alone.");
            });
        });
        ui.add_space(6.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("How Preflight works").size(16.0).strong());
            ui.label("1. Test while you play: start your game, press Record in OBS and play for 2 minutes. Preflight measures by itself.");
            ui.label("2. Fix what matters: one change at a time, grouped by where you make it (OBS, your game, your PC). Preflight notices when you've changed it. Keep anything you'd rather not change.");
            ui.label("3. Ready to go live: a short checklist that ticks itself.");
            ui.label("Your setup (from Home) shows every setting at a glance: what it is now, what to change, and why. History tells you how each stream went.");
            theme::small(
                ui,
                "Preflight only reads settings files and system stats. It never edits game or OBS settings, never touches a running game, and never controls OBS.",
            );
        });
        ui.add_space(10.0);
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("About").size(16.0).strong());
            ui.label(format!(
                "{} v{} · {}",
                crate::util::APP_NAME,
                crate::util::VERSION,
                crate::util::BUILD_LABEL
            ));
            match self.own_usage {
                Some((c, m)) => theme::small(
                    ui,
                    format!(
                        "Preflight right now: {}, {m:.0} MB of memory.",
                        crate::util::one_core(c)
                    ),
                ),
                None => theme::small(ui, "Preflight right now: measuring…"),
            }
            ui.add_space(4.0);
            ui.add_space(4.0);
            if theme::secondary(ui, "What's new in this version").clicked() {
                self.whats_new_open = true;
            }
        });
    }
}

impl App {
    /// Settings › Advanced: self-test, extra logging and the reports folder.
    pub(super) fn ui_advanced_tools(&mut self, ui: &mut Ui) {
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Self-test").strong());
                if ui.button("Run self-test").clicked() {
                    self.selftests = report::self_test(&report::SelfTestInput {
                        hw: self.hw.as_ref(),
                        games: &self.games,
                        obs: self.obs.as_ref(),
                        obs_error: self.obs_error.as_deref(),
                        net: self.net.as_ref(),
                    });
                }
            });
            for t in &self.selftests {
                ui.horizontal(|ui| {
                    let (s, l) = match t.status {
                        TestStatus::Pass => (Status::Ok, "Pass"),
                        TestStatus::Warn => (Status::Warn, "Warning"),
                        TestStatus::Fail => (Status::Bad, "Fail"),
                    };
                    theme::status_pill(ui, s, l);
                    ui.label(RichText::new(&t.name).strong());
                });
                theme::small(ui, &t.detail);
            }
            ui.separator();
            let mut on = crate::logger::DETAILED.load(std::sync::atomic::Ordering::Relaxed);
            if ui
                .checkbox(
                    &mut on,
                    "Record more detail (for problems that only happen sometimes)",
                )
                .changed()
            {
                crate::logger::set_detailed(on);
            }
            theme::small(
                ui,
                "Turn this on, make the problem happen, then report it. It turns off when you close Preflight.",
            );
            if ui.button("Open reports folder").clicked() {
                report::open_folder(&crate::util::dirs().reports);
            }
        });
    }
}

/// A new issue on GitHub with the summary and the report header filled in.
pub(super) fn issue_url(summary: &str) -> String {
    let header = report::header();
    let field = |name: &str| {
        header
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name}: ")))
            .unwrap_or("")
            .to_string()
    };
    let system = format!(
        "{}, {} {}",
        field("Distro"),
        field("Desktop"),
        field("Session")
    );
    let title = if summary.trim().is_empty() {
        "Bug report".to_string()
    } else {
        summary.trim().to_string()
    };
    format!(
        "https://github.com/Matt-B-007/Stream-Preflight/issues/new?template=bug_report.yml&title={}&version={}&system={}",
        url_encode(&crate::util::scrub(&title)),
        url_encode(crate::util::VERSION),
        url_encode(&system)
    )
}

/// Percent-encoding for a URL query value.
pub(super) fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_link_is_encoded() {
        assert_eq!(url_encode("a b&c/ü"), "a%20b%26c%2F%C3%BC");
        let u = issue_url("Test never ends");
        assert!(u.starts_with("https://github.com/Matt-B-007/Stream-Preflight/issues/new?template=bug_report.yml&title=Test%20never%20ends&version="));
        assert!(!u.contains(' '));
    }
}
