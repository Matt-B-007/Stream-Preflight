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
        v.push(("postflight", "Post-flight (numbers only)", t));

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

    pub(super) fn ui_report_box(&mut self, ctx: &egui::Context) {
        let Some(mut rb) = self.report.take() else {
            return;
        };
        let mut keep = true;
        let mut save = false;
        egui::Window::new(if rb.saved.is_some() { "Report saved" } else { "Report this problem" })
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 120.0))
            .default_width(480.0)
            .show(ctx, |ui| {
                if let Some((id, path)) = rb.saved.clone() {
                    ui.label(RichText::new(format!("Saved. Report ID {id}")).strong());
                    theme::small(ui, "A plain text file: drag it into the chat, or open it to read what's in it first.");
                    // The full path, selectable, so it can always be copied by hand.
                    let mut shown = path.display().to_string();
                    ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY).interactive(true));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if theme::secondary(ui, "Open report").clicked() {
                            report::open_file(&path);
                        }
                        if theme::secondary(ui, "Show in folder").clicked() {
                            report::show_file(&path);
                        }
                        if theme::secondary(ui, if rb.copied { "Copied" } else { "Copy file location" }).clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                            report::copy_fallback(&path.display().to_string());
                            rb.copied = true;
                        }
                        if theme::primary(ui, "  Done  ").clicked() {
                            keep = false;
                        }
                    });
                    return;
                }
                if !rb.context.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("About:").color(MUTED));
                        ui.label(&rb.context);
                    });
                }
                ui.label(RichText::new("Anything to add? (optional)").strong());
                ui.add(egui::TextEdit::multiline(&mut rb.note).desired_rows(3).desired_width(f32::INFINITY).hint_text("e.g. V-Sync is actually off in my game"));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Includes your PC specs, OBS settings and recent activity. Never passwords, stream keys or your name.").size(13.0).color(MUTED));
                    if link_small(ui, if rb.show_contents { "Hide details" } else { "What's included" }) {
                        rb.show_contents = !rb.show_contents;
                    }
                });
                if rb.show_contents {
                    theme::inset(ui, |ui| {
                        for s in [
                            "A summary: what this is about, your note, anything that failed",
                            "Your system: OS, CPU, graphics card, RAM, drivers",
                            "OBS: encoder and quality settings, bitrate, resolution, source types (no names, no stream key)",
                            "Games: which settings files were found, and their full contents (game settings and key bindings only)",
                            "Your last scan and test measurements",
                            "Recent app activity (user name removed from paths)",
                        ] {
                            theme::small(ui, format!("• {s}"));
                        }
                    });
                }
                if let Some(e) = &rb.error {
                    ui.label(RichText::new(format!("Couldn't save the report: {e}")).color(BAD));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary(ui, "  Save report  ").clicked() {
                        save = true;
                    }
                    if theme::secondary(ui, "Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if save {
            self.selftests = report::self_test(&report::SelfTestInput {
                hw: self.hw.as_ref(),
                games: &self.games,
                obs: self.obs.as_ref(),
                obs_error: self.obs_error.as_deref(),
                net: self.net.as_ref(),
            });
            let extra = self.report_extras();
            let sections = report::build_sections(&report::ReportInput {
                context: &rb.context,
                description: &rb.note,
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
            });
            let id = crate::util::short_id();
            match report::save(&sections, &id) {
                Ok(p) => {
                    rb.saved = Some((id, p));
                    self.pending_crashes = crate::logger::pending_crashes().len();
                }
                Err(e) => rb.error = Some(e),
            }
        }
        if keep {
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
            ui.label(RichText::new("Something not working?").size(17.0).strong());
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
        ui.label(RichText::new("What to do if…").size(19.0).strong());
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
                ui.label("Settings › Advanced › Remove Preflight from this PC deletes everything below in one go. Or delete these folders yourself:");
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
                theme::small(ui, "• Every colour also has a word next to it (Change, Keep, OK, Great…), so nothing depends on colour alone.");
            });
        });
        ui.add_space(6.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("How Preflight works").size(17.0).strong());
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
        egui::CollapsingHeader::new(RichText::new("Advanced tools").strong()).id_salt("adv-tools").show(ui, |ui| {
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
                if ui.checkbox(&mut on, "Record more detail (for problems that only happen sometimes)").changed() {
                    crate::logger::set_detailed(on);
                }
                theme::small(ui, "Turn this on, make the problem happen, then report it. It turns off when you close Preflight.");
                if ui.button("Open reports folder").clicked() {
                    report::open_folder(&crate::util::dirs().reports);
                }
            });
        });
        ui.add_space(10.0);
        theme::card(ui, |ui| {
            ui.label(RichText::new("About").size(17.0).strong());
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
            ui.label(RichText::new("New in v0.1.0-beta.7").strong());
            theme::small(
                ui,
                "• More tidying inside the code, and stricter automatic checks before each release. Nothing looks or works differently.",
            );
            ui.add_space(4.0);
            ui.label(RichText::new("From v0.1.0-beta.6").strong());
            theme::small(
                ui,
                "• A tidy-up inside the code to make the next updates safer. Nothing looks or works differently.",
            );
            ui.add_space(4.0);
            ui.label(RichText::new("From v0.1.0-beta.5").strong());
            for s in [
                "Opens on Home: a Get started card you can do in any order, then one plain answer, your top fixes, last test and last stream",
                "A simpler sidebar (Home, Test, Fixes, Live, History) and a top bar for OBS, your game, style and platforms",
                "Nothing is locked: pages that need OBS show a note with a Connect OBS button",
                "Much lighter while you game: no redrawing when minimized or in the tray, at most once a second while the game is in front",
                "Recording while streaming is shown live (Hybrid MOV/MP4 too), with its timer, rate and a line in the summary",
                "Start at login, start in the tray, and choose what happens when you go live. On Wayland a tray-only Preflight keeps watching OBS",
                "Fixes from your reports: web views no longer count as games, capture card size, doubled desktop audio, mic vs game, preset advice, fuller logs, smaller reports",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("From v0.1.0-beta.4").strong());
            for s in [
                "Install with one line, and updates come straight from the GitHub repository: no release pages needed",
                "Each download is checked against its SHA-256 before it's installed",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("From v0.1.0-beta.3").strong());
            for s in [
                "A three-step welcome the first time you open Preflight, and clearer empty screens before your first test",
                "In-flight: its own page while you're live, opening by itself when you start streaming",
                "The sidebar has Pre-flight, In-flight and Post-flight, and folds to icons (Ctrl+B)",
                "Simpler Test and Fix screens, one verdict at the top, and Settings in five tabs",
                "Betas: beta builds look for betas, and you can switch back to stable",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("From v0.1.0").strong());
            for s in [
                "New name: Stream Preflight. Your settings, history and saved OBS password move over by themselves",
                "Cockpit glass look: live readings strip, score dials, a glowing \"Do this next\" card and one bright button in the Next bar",
                "The Test step is one checklist that ticks itself as OBS starts streaming and recording",
                "Two PCs: connect to OBS on your streaming PC, capture card checks (frame rate, resolution, colour range, MJPEG, HDR, refresh judder), and fixes labelled by PC",
                "Paired mode: Preflight on both PCs, linked once with a 6-digit code, home network only",
                "Install for this user, updates from GitHub with a checksum check, beta versions if you want them, and uninstall (also from Windows Settings › Apps)",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("From test build 13").strong());
            for s in [
                "A new sidebar: Pre-flight opens into your game, its style, and the steps Test, Fix, Your setup and Ready to go live. Post-flight holds Last stream and History",
                "Click the game name to switch games, and the style under it to re-sort your fixes (no new test needed)",
                "Each step shows one \"Do this next\" card. The only bright button is in the Next bar at the bottom",
                "Go live is now \"Ready to go live\", the last step of Pre-flight",
                "Your setup: plain rows you can fold, \"Fix this\" only where something should change, and the scan results under PC and connections",
                "System tray: grey idle, green watching OBS, amber testing, red live, and a white dot when Preflight needs you. Minimize and close options in Settings",
                "Notifications when a test finishes or a stream ends while Preflight is in the background",
                "If you start streaming while the bandwidth-test flag may still be on, Preflight asks whether it's a real stream",
                "One banner at a time, most important first",
                "Bug reports now include post-flight numbers, test history, one setup for all games, what was on screen, and the tray",
                "Keyboard: Alt+1 Test, Alt+2 Fix, Alt+3 Your setup, Alt+4 Post-flight. Ctrl+Enter presses the Next button",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 12").strong());
            for s in [
                "Stream + record test: a checklist that ticks itself as you start streaming and recording, with each platform's private test mode (Twitch test flag, YouTube Unlisted)",
                "Several platforms: tick Twitch, YouTube and Kick in Settings. Preflight suggests one shared encode, one each, or a restream service, and checks the upload",
                "Video encoders panel in Your setup: every GPU encoder, its chips and codecs, and which programs are using it right now",
                "OBS plugins: Source Record, vertical canvases and extra streams are counted as encodes and measured in the test",
                "One setup for all your games: OBS settings that suit every tested game; heavy games get in-game changes instead",
                "Minecraft: any launcher (CurseForge, Prism, Modrinth and more), the modpack's name, shaders and Distant Horizons",
                "Any OBS setting change counts as a change, tests remember their OBS settings, and P6 counts as fine",
                "Privacy: Steam IDs and account numbers are removed from reports",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 11").strong());
            for s in [
                "Reads games without a profile: works out the engine (Unity, Unreal, Source, Godot) and reads its settings, for Steam, Heroic, Lutris, Bottles, Epic and Game Pass games",
                "A fuller Counter-Strike 2 profile, and each game table says how many settings were read and how many the advice uses",
                "After \"I changed it\", Preflight checks that the game really saved the change",
                "Styles fixed: Competitive lowers graphics first, Balanced uses DLSS/FSR, Cinematic caps FPS first. Switch style on the Pre-flight screen; each game remembers its own",
                "Stream picture advice: the sharpest resolution for your bitrate and game, NVENC quality options scaled to your GPU's headroom, audio bitrate and sample rate",
                "In-game effects that cost bitrate (film grain, motion blur) are pointed out",
                "The viewer preview re-creates your stream from a high-quality recording, and skips moments it can't line up",
                "Pick your test: full, stream + record, or quick check. One Test again button, and a Storage list in Settings",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 10").strong());
            for s in [
                "Results per game: a banner when you switch games, a game switcher, and before/after only for the same game",
                "Only suggests settings a game really has; games without an FPS limit get one outside the game (Steam launch option or driver)",
                "Each game table shows whether its settings file was read, and when the game last saved it",
                "Cities: Skylines II reads your real settings; Linux game capture (vkcapture) is recognised",
                "Delete test recordings from the result, before the next test, or in Settings",
                "Bug reports include game settings files; Show in folder, Open report and Copy work on KDE Wayland",
                "On Linux, Preflight adds itself to your app menu with its icon",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 9").strong());
            for s in [
                "Fixes say exactly what to change: the setting, what it is now, and what to change it to",
                "Reads your real game settings, including the preset; with a Custom preset it names the heavy settings",
                "FPS-limit advice that fits: if your limit is too high to ever kick in, it suggests a lower one",
                "Measures your game's FPS (PresentMon on Windows, a MangoHud log on Linux) or asks you",
                "Two scores: Smoothness (what the test measured) and Setup (settings that will help)",
                "1-minute quick re-test for Check my changes, results remembered per game, Copy as text",
                "New game profiles (unverified): VALORANT, Apex, Warzone, League, Minecraft, Marvel Rivals, ELDEN RING, GTA V",
                "Bug reports saved as a named text file; Remove Preflight from this PC; a new app icon",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test build 8").strong());
            for s in [
                "Your setup: every OBS, game and PC setting with what it is now, what to change and why",
                "Pre-flight in 3 steps, with a Next button always at the bottom",
                "Fix one thing at a time, see how much each fix helps, or keep it as it is",
                "Check my changes: a quick re-test that shows before and after",
                "Viewer preview with a slider between the original and the stream",
                "A \"What changed\" timeline in History",
                "One-page Settings and \"What to do if…\" help",
                "Fixes from build 7: better game detection, long GPU-maxed periods, preview matching, Cyberpunk FPS limit",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("New in test builds 4 to 7").strong());
            for s in [
                "A guided pre-flight: Scan, Test, Fix, Go live",
                "Setup in 3 short screens, with your monitors and refresh rates detected",
                "The test starts by itself when you press Record in OBS",
                "Preflight notices when you've made a fix",
                "Plain sentences with the real numbers everywhere",
                "New OBS quality checks: rate control, keyframe interval, encoder preset and downscale filter",
                "Viewer preview: your stream next to the original, from your test recording",
                "Post-flight: a summary of how each stream went, with the worst moments and what to change",
                "Recording while streaming: separate encoder, replay buffer and Enhanced Broadcasting checks, with preset advice for each side",
                "Encoder chip load and frames dropped per output (stream and recording)",
                "Recording space left, crash-safe recording format, and hard-drive warnings",
                "\"Your recommended OBS settings\" in one list, and issues grouped by where you fix them",
                "Hover any term (bitrate, CBR, keyframe...) for a plain-English meaning",
                "One shared OBS connection, and less background work while minimized",
                "\"This looks wrong\" links and a one-box problem report",
                "A simpler menu",
            ] {
                theme::small(ui, format!("• {s}"));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Not in this test build yet").strong());
            for s in [
                "Streamlabs Desktop and Meld Studio connections",
                "In-game frame times (PresentMon)",
                "System tray mode",
                "Installing updates in place (needs the GitHub repository)",
                "Keyring password storage on Linux",
            ] {
                theme::small(ui, format!("• {s}"));
            }
        });
    }
}
