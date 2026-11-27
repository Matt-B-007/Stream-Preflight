// SPDX-License-Identifier: GPL-3.0-or-later
//! Two PCs: roles, paired mode and the two-PC settings cards.

use super::*;

impl App {
    pub(super) fn role(&self) -> crate::twopc::Role {
        crate::twopc::Role::from_str(&self.st.pc_role)
    }

    pub(super) fn obs_remote(&self) -> bool {
        !crate::twopc::is_local_host(&self.st.obs_host)
    }

    /// Paired mode: start or stop the link, swap numbers, and measure along.
    pub(super) fn pair_logic(&mut self, ctx: &egui::Context, now: f64) {
        use crate::twopc::Role;
        let role = self.role();
        let want = match role {
            Role::Streaming if self.st.paired || self.pair_offer => "host",
            Role::Gaming if self.st.paired || self.pair_try => "client",
            _ => "",
        };
        if want != self.pairing_kind {
            self.pairing = None;
            self.pairing_kind = "";
            if !want.is_empty() {
                let token = if self.st.paired {
                    crate::config::load_pair_secret()
                } else {
                    None
                };
                let started = if want == "host" {
                    crate::pairing::start_host(token, ctx.clone())
                } else {
                    crate::pairing::start_client(self.st.obs_host.clone(), token, ctx.clone())
                };
                match started {
                    Ok(p) => {
                        self.pairing = Some(p);
                        self.pairing_kind = want;
                    }
                    Err(e) => {
                        self.pair_status = e;
                        self.pair_offer = false;
                    }
                }
            }
        }
        let Some(p) = &self.pairing else { return };
        if now - self.side_at < 1.0 {
            return;
        }
        self.side_at = now;
        let measuring = self
            .test_handle
            .as_ref()
            .map(|h| h.phase() == Phase::Measuring)
            .unwrap_or(false);
        let fixes = |place: &[analysis::Place]| -> Vec<String> {
            self.analysis
                .issues
                .iter()
                .filter(|i| {
                    place.contains(&analysis::place_of(&i.id))
                        && !self.st.kept_issues.contains(&i.id)
                })
                .map(|i| {
                    i.options
                        .iter()
                        .find(|o| o.suggested)
                        .map(|o| o.name.clone())
                        .unwrap_or_else(|| i.title.clone())
                })
                .take(8)
                .collect()
        };
        let mut new_token = None;
        let mut monitor_hz = None;
        let mut got_side_test = None;
        let mut side_sample = false;
        if let Ok(mut g) = p.state.lock() {
            if self.pairing_kind == "host" {
                if measuring && g.my_stream.test_id != self.test_counter {
                    // A new test started.
                }
                g.my_stream = crate::pairing::StreamSide {
                    obs_connected: self.obs.as_ref().map(|o| o.connected).unwrap_or(false),
                    live: self.live.is_some(),
                    kbps: self.live.as_ref().and_then(|l| l.kbps),
                    skipped_pct: self.live.as_ref().map(|l| l.dropped_pct),
                    encoder_busy: self
                        .encoders
                        .iter()
                        .filter_map(|e| e.busy)
                        .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b)))),
                    cpu: 0.0,
                    measuring,
                    test_id: self.test_counter,
                    fixes: fixes(&[analysis::Place::Obs]),
                };
                if let Some((gs, _)) = &g.peer_game {
                    monitor_hz = Some(gs.monitor_hz);
                    if let Some(t) = &gs.test
                        && t.test_id == self.test_counter
                        && self.side_test.as_ref().map(|s| s != t).unwrap_or(true)
                    {
                        got_side_test = Some(t.clone());
                    }
                }
                if g.code.is_none() && self.pair_offer && g.new_token.is_none() && g.token.is_some()
                {
                    self.pair_offer = false;
                }
            } else {
                side_sample = true;
                let peer_measuring = g
                    .peer_stream
                    .as_ref()
                    .filter(|(_, at)| at.elapsed().as_secs() < 10)
                    .map(|(s, _)| (s.measuring, s.test_id));
                match (peer_measuring, &mut self.side_measure) {
                    (Some((true, id)), None) => {
                        log::info!(
                            "Paired mode: the streaming PC started a test; measuring this PC too"
                        );
                        self.side_measure = Some(crate::pairing::SideMeasure {
                            test_id: id,
                            started: Some(std::time::Instant::now()),
                            game: self
                                .games
                                .iter()
                                .find(|x| x.running)
                                .map(|x| x.name.clone()),
                            ..Default::default()
                        });
                    }
                    (Some((false, _)), Some(m)) | (None, Some(m)) => {
                        g.my_game.test = Some(m.summary());
                        self.side_measure = None;
                    }
                    _ => {}
                }
                g.my_game.game = self
                    .games
                    .iter()
                    .find(|x| x.running)
                    .map(|x| x.name.clone());
                g.my_game.monitor_hz = self.st.monitor_hz;
                g.my_game.fixes = fixes(&[analysis::Place::Game, analysis::Place::Pc]);
            }
            if let Some(t) = g.new_token.take() {
                new_token = Some(t);
            }
            if !g.status.is_empty() {
                self.pair_status = g.status.clone();
            }
        }
        // Gaming PC: a light reading of this PC every 2 s while paired.
        if side_sample && (now as u64).is_multiple_of(2) {
            let mon = self
                .side_monitor
                .get_or_insert_with(crate::hardware::Monitor::new);
            let s = mon.sample(0.0);
            if let Some(m) = &mut self.side_measure {
                if let Some(gp) = s.gpu {
                    m.gpu.push(gp);
                }
                m.cpu.push(s.cpu);
            }
            if let Some(p) = &self.pairing
                && let Ok(mut g) = p.state.lock()
            {
                g.my_game.gpu = s.gpu;
                g.my_game.gpu_temp = s.gpu_temp;
                g.my_game.cpu = s.cpu;
            }
        }
        if let Some(t) = new_token {
            crate::config::save_pair_secret(&t);
            self.st.paired = true;
            self.st.paired_name = if self.pairing_kind == "host" {
                "Gaming PC".into()
            } else {
                "Streaming PC".into()
            };
            self.st.save();
            self.pair_offer = false;
            self.pair_try = false;
            self.pair_code_input.clear();
        }
        if let Some(hz) = monitor_hz
            && hz != self.st.other_monitor_hz
        {
            self.st.other_monitor_hz = hz;
            self.recompute();
        }
        if let Some(t) = got_side_test {
            log::info!("Paired mode: got the gaming PC's numbers for this test");
            self.side_test = Some(t);
        }
    }

    /// Streaming PC: show a new code. Gaming PC: send the typed code.
    pub(super) fn pair_now(&mut self, ctx: &egui::Context) {
        match self.role() {
            crate::twopc::Role::Streaming => {
                self.pair_offer = true;
                self.pair_logic(ctx, f64::MAX);
                if let Some(p) = &self.pairing
                    && let Ok(mut g) = p.state.lock()
                {
                    g.code = Some((crate::pairing::new_code(), std::time::Instant::now(), 5));
                    g.status = "Type this code in Preflight on your gaming PC".into();
                }
            }
            crate::twopc::Role::Gaming => {
                let code: String = self
                    .pair_code_input
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                if code.len() != 6 {
                    self.pair_status = "The code has 6 digits.".into();
                    return;
                }
                self.pair_try = true;
                self.pair_logic(ctx, f64::MAX);
                if let Some(p) = &self.pairing
                    && let Ok(mut g) = p.state.lock()
                {
                    g.pair_with = Some(code);
                    g.status = "Pairing…".into();
                }
            }
            crate::twopc::Role::One => {}
        }
    }

    /// The code on screen (streaming PC), if any.
    pub(super) fn pair_code(&self) -> Option<String> {
        self.pairing.as_ref().and_then(|p| {
            p.state.lock().ok().and_then(|g| {
                g.code
                    .as_ref()
                    .filter(|c| c.1.elapsed().as_secs() < 300)
                    .map(|c| c.0.clone())
            })
        })
    }

    /// The other PC's latest numbers, if it was heard from in the last 15 seconds.
    pub(super) fn peer_line(&self) -> Option<String> {
        let p = self.pairing.as_ref()?;
        let g = p.state.lock().ok()?;
        if let Some((gs, at)) = g
            .peer_game
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_secs() < 15)
        {
            let _ = at;
            return Some(format!(
                "Gaming PC: {} · GPU {} · CPU {:.0}% · monitor {} Hz",
                gs.game.clone().unwrap_or_else(|| "no game running".into()),
                gs.gpu
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "–".into()),
                gs.cpu,
                gs.monitor_hz
            ));
        }
        if let Some((ss, _)) = g
            .peer_stream
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_secs() < 15)
        {
            return Some(format!(
                "Streaming PC: OBS {} · {} · encoder {}",
                if ss.obs_connected {
                    "connected"
                } else {
                    "not open"
                },
                if ss.live {
                    ss.kbps
                        .map(|k| format!("live at {k:.0} kbps"))
                        .unwrap_or_else(|| "live".into())
                } else {
                    "not live".into()
                },
                ss.encoder_busy
                    .map(|v| format!("{v:.0}% busy"))
                    .unwrap_or_else(|| "–".into())
            ));
        }
        None
    }

    /// The other PC's open fixes, labelled for the combined view.
    pub(super) fn peer_fixes(&self) -> Vec<String> {
        let Some(p) = self.pairing.as_ref() else {
            return Vec::new();
        };
        let Ok(g) = p.state.lock() else {
            return Vec::new();
        };
        if let Some((gs, _)) = &g.peer_game {
            return gs.fixes.clone();
        }
        if let Some((ss, _)) = &g.peer_stream {
            return ss.fixes.clone();
        }
        Vec::new()
    }

    pub(super) fn unpair(&mut self) {
        self.pairing = None;
        self.pairing_kind = "";
        self.pair_offer = false;
        self.st.paired = false;
        self.st.paired_name.clear();
        self.st.save();
        crate::config::forget_pair_secret();
        self.pair_status = "Unpaired".into();
        log::info!("Paired mode: unpaired");
    }

    /// Settings › Your PCs: one PC or two, and paired mode.
    pub(super) fn ui_pcs_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        use crate::twopc::Role;
        let mut changed = false;
        let mut act: Option<&str> = None;
        theme::card(ui, |ui| {
            ui.label(RichText::new("Your PCs").size(16.0).strong());
            theme::small(
                ui,
                "Some streamers game on one PC and stream from another, with a capture card or NDI in between.",
            );
            for r in [Role::One, Role::Gaming, Role::Streaming] {
                if ui.radio(self.role() == r, r.label()).clicked() && self.role() != r {
                    self.st.pc_role = r.as_str().into();
                    self.st.two_pc_asked = true;
                    changed = true;
                }
            }
            let role = self.role();
            if role == Role::Gaming && !self.obs_remote() {
                ui.label(RichText::new("Put your streaming PC's address under OBS address below, so Preflight can read OBS there.").color(WARN));
            }
            if role != Role::One {
                ui.add_space(6.0);
                ui.label(RichText::new("Paired mode").strong());
                theme::small(
                    ui,
                    "Run Preflight on both PCs and link them once with a code. They share game, GPU and OBS numbers over your home network only, never the internet. No names, keys or addresses are shared.",
                );
                if self.st.paired {
                    let heard = self.peer_line();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Paired with your {}",
                                if role == Role::Streaming {
                                    "gaming PC"
                                } else {
                                    "streaming PC"
                                }
                            ))
                            .color(GOOD),
                        );
                        ui.label(
                            RichText::new(if heard.is_some() {
                                "· connected"
                            } else {
                                "· not heard from right now"
                            })
                            .color(MUTED),
                        );
                        if outlined(ui, "Unpair").clicked() {
                            act = Some("unpair");
                        }
                    });
                    if let Some(h) = heard {
                        theme::small(ui, h);
                    }
                } else if role == Role::Streaming {
                    match self.pair_code() {
                        Some(code) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{} {}", &code[..3], &code[3..])).monospace().size(28.0).color(ACCENT));
                                ui.label(RichText::new("Type this code in Preflight on your gaming PC (Settings › Your PCs). It works for 5 minutes.").color(TEXT_SOFT));
                            });
                        }
                        None => {
                            if outlined(ui, "Pair with my gaming PC").clicked() {
                                act = Some("pair");
                            }
                        }
                    }
                    theme::small(
                        ui,
                        "Your firewall may ask to let Preflight use the home network. Allow it on private networks.",
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Code from your streaming PC");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pair_code_input)
                                .desired_width(90.0)
                                .hint_text("123 456"),
                        );
                        if outlined(ui, "Pair").clicked() {
                            act = Some("pair");
                        }
                    });
                }
                if !self.pair_status.is_empty() && !self.st.paired {
                    theme::small(ui, &self.pair_status);
                }
            }
        });
        match act {
            Some("pair") => self.pair_now(ctx),
            Some("unpair") => self.unpair(),
            _ => {}
        }
        if changed {
            log::info!("PC role: {}", self.st.pc_role);
            if self.role() == crate::twopc::Role::One && self.st.paired {
                self.unpair();
            }
            self.st.save();
            self.recompute();
        }
    }

    /// Your setup: the two-PC picture (capture sources, the other PC, its fixes),
    /// or a one-time question when it looks like two PCs.
    pub(super) fn ui_two_pc_section(&mut self, ui: &mut Ui) {
        use crate::twopc::Role;
        let role = self.role();
        let caps: Vec<crate::twopc::CaptureInfo> = self
            .obs
            .as_ref()
            .map(|o| o.captures.clone())
            .unwrap_or_default();
        let game_cap = self
            .obs
            .as_ref()
            .map(|o| o.uses_game_capture())
            .unwrap_or(false);
        if role == Role::One {
            if !self.st.two_pc_asked
                && crate::twopc::looks_two_pc(&caps, game_cap, self.obs_remote())
            {
                let mut pick: Option<Role> = None;
                theme::card(ui, |ui| {
                    ui.label(
                        RichText::new("Do you stream from a second PC?")
                            .size(17.0)
                            .strong(),
                    );
                    theme::muted(
                        ui,
                        if self.obs_remote() {
                            "OBS is on another computer."
                        } else {
                            "OBS shows a capture card or network source, and no game capture."
                        },
                    );
                    ui.horizontal_wrapped(|ui| {
                        if outlined(ui, "This is my streaming PC").clicked() {
                            pick = Some(Role::Streaming);
                        }
                        if outlined(ui, "This is my gaming PC").clicked() {
                            pick = Some(Role::Gaming);
                        }
                        if outlined(ui, "No, one PC").clicked() {
                            pick = Some(Role::One);
                        }
                    });
                });
                ui.add_space(10.0);
                if let Some(r) = pick {
                    self.st.pc_role = r.as_str().into();
                    self.st.two_pc_asked = true;
                    self.st.save();
                    self.recompute();
                }
            }
            return;
        }
        let peer = self.peer_line();
        let sub = if self.st.paired {
            if peer.is_some() {
                "Paired · connected"
            } else {
                "Paired · not heard from"
            }
        } else {
            "Not paired"
        };
        let peer_fixes = self.peer_fixes();
        let side = self.side_test.clone();
        let paired = self.st.paired;
        setup_section(
            ui,
            if role == Role::Gaming {
                "Two PCs · this is the gaming PC"
            } else {
                "Two PCs · this is the streaming PC"
            },
            sub,
            if peer.is_some() { GOOD } else { MUTED },
            true,
            |ui| {
                for c in caps.iter().filter(|c| c.in_scene) {
                    ui.add(egui::Separator::default().spacing(10.0));
                    ui.label(RichText::new(c.describe()).color(TEXT));
                    if !c.range.is_empty() {
                        theme::small(ui, format!("Colour range: {}", c.range));
                    }
                }
                if caps.iter().all(|c| !c.in_scene) {
                    theme::small(
                        ui,
                        "No capture card or network source is visible in your current OBS scene.",
                    );
                }
                ui.add(egui::Separator::default().spacing(10.0));
                match &peer {
                    Some(l) => {
                        ui.label(RichText::new(l).color(TEXT_SOFT));
                    }
                    None if paired => theme::small(
                        ui,
                        "The other PC isn't answering right now. Is Preflight open there?",
                    ),
                    None => theme::small(
                        ui,
                        "Pair with Preflight on your other PC (Settings › Your PCs) to see both PCs here.",
                    ),
                }
                if !peer_fixes.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Open on your {}:",
                            if role == Role::Gaming {
                                "streaming PC"
                            } else {
                                "gaming PC"
                            }
                        ))
                        .strong(),
                    );
                    for f in &peer_fixes {
                        theme::small(ui, format!("• {f}"));
                    }
                }
                if let Some(t) = &side {
                    ui.label(RichText::new(format!(
                    "Gaming PC during the last test{}: GPU {} on average, maxed out {:.0}% of the time, CPU {:.0}%.",
                    t.game.as_ref().map(|g| format!(" ({g})")).unwrap_or_default(),
                    t.gpu_avg.map(|g| format!("{g:.0}%")).unwrap_or_else(|| "–".into()),
                    t.gpu_maxed_pct,
                    t.cpu_avg
                )).color(TEXT_SOFT));
                }
            },
        );
    }
}
