// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings and the uninstall dialog.

use super::*;

/// "Last checked 5 minutes ago" for Settings › Updates.
pub(super) fn last_checked(at: i64, now: i64) -> String {
    if at <= 0 {
        return "Not checked yet".into();
    }
    let ago = (now - at).max(0);
    let when = match ago {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let m = ago / 60;
            format!("{m} minute{} ago", if m == 1 { "" } else { "s" })
        }
        3600..=86_399 => {
            let h = ago / 3600;
            format!("{h} hour{} ago", if h == 1 { "" } else { "s" })
        }
        _ => {
            let d = ago / 86_400;
            format!("{d} day{} ago", if d == 1 { "" } else { "s" })
        }
    };
    format!("Last checked {when}")
}

pub(super) const TAB_GENERAL: u8 = 0;
pub(super) const TAB_APPEARANCE: u8 = 1;
pub(super) const TAB_UPDATES: u8 = 2;
pub(super) const TAB_PRIVACY: u8 = 3;
pub(super) const TAB_OBS: u8 = 4;
pub(super) const TAB_TRAY: u8 = 5;
pub(super) const TAB_ADVANCED: u8 = 6;
pub(super) const TAB_ABOUT: u8 = 7;

impl App {
    /// "Uninstall Stream Preflight?", laid out like the other apps.
    pub(super) fn ui_uninstall(&mut self, ctx: &egui::Context) {
        let Some(mut u) = self.uninstall.take() else {
            return;
        };
        let mut keep = true;
        let mut go = false;
        let title = if u.done {
            "Stream Preflight is uninstalled"
        } else {
            "Uninstall Stream Preflight?"
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                if u.done {
                    ui.label("Stream Preflight is uninstalled. Thanks for trying it.");
                    if !u.delete_settings {
                        theme::small(ui, "Your settings and logs are still there, in case you install it again.");
                    }
                    ui.add_space(6.0);
                    if theme::primary(ui, "Close").clicked() {
                        crate::logger::end_session();
                        std::process::exit(0);
                    }
                    return;
                }
                ui.label("Removes the program, its app menu entry and its icon. OBS, your games and their settings are never touched.");
                ui.add_space(4.0);
                ui.checkbox(&mut u.delete_settings, "Also delete my settings and logs");
                if u.delete_settings {
                    for (what, path) in crate::uninstall::own_folders() {
                        theme::small(
                            ui,
                            format!("{what}: {}", crate::util::scrub(&path.to_string_lossy())),
                        );
                    }
                }
                if let Some(exe) = crate::uninstall::program_file() {
                    ui.checkbox(&mut u.program, "Delete the program file");
                    theme::small(ui, crate::util::scrub(&exe.to_string_lossy()));
                }
                if !u.recs.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Test recordings Preflight made (only ticked ones are deleted):").strong());
                    for (path, size, del) in u.recs.iter_mut() {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        ui.checkbox(del, format!("{name} · {}", crate::util::fmt_bytes(*size)));
                    }
                }
                if let Some(e) = &u.error {
                    ui.label(RichText::new(format!("Some files couldn't be removed: {e}")).color(pal().bad));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::secondary(ui, "Cancel").clicked() {
                        keep = false;
                    }
                    if theme::danger(ui, "Uninstall").clicked() {
                        go = true;
                    }
                });
            });
        if go {
            let chosen: Vec<std::path::PathBuf> = u
                .recs
                .iter()
                .filter(|r| r.2)
                .map(|r| std::path::PathBuf::from(&r.0))
                .collect();
            match crate::uninstall::run_with(&chosen, u.program, u.delete_settings) {
                Ok(()) => u.done = true,
                Err(e) => u.error = Some(e),
            }
        }
        if keep {
            self.uninstall = Some(u);
        }
    }

    // -----------------------------------------------------------------------
    // Settings, in the same order as the other apps: General · Appearance ·
    // Updates · Privacy · (Preflight's own) · Advanced · About & uninstall.

    pub(super) fn ui_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.label(theme::caps("Settings", 12.0, pal().accent));
        let tabs = [
            "General",
            "Appearance",
            "Updates",
            "Privacy",
            "OBS & PCs",
            "Tray & notifications",
            "Advanced",
            "About & uninstall",
        ];
        let mut tab = (self.settings_tab as usize).min(tabs.len() - 1);
        ui.label(
            RichText::new(match tab as u8 {
                TAB_GENERAL => "General",
                TAB_APPEARANCE => "Appearance",
                TAB_UPDATES => "Updates",
                TAB_PRIVACY => "Privacy and data",
                TAB_OBS => "OBS and your PCs",
                TAB_TRAY => "Tray and notifications",
                TAB_ADVANCED => "Advanced",
                _ => "About and uninstall",
            })
            .size(22.0)
            .family(theme::title_family())
            .strong(),
        );
        ui.add_space(6.0);
        egui::Frame::NONE
            .fill(pal().card)
            .stroke(egui::Stroke::new(1.0, pal().border))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, t) in tabs.iter().enumerate() {
                        let on = tab == i;
                        let b = egui::Button::new(RichText::new(*t).color(if on {
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
                        if ui.add(b).clicked() {
                            tab = i;
                        }
                    }
                });
            });
        self.settings_tab = tab as u8;
        let tab = tab as u8;
        ui.add_space(12.0);
        let mut save = false;
        let mut recompute = false;
        let mut act: Option<NextAct> = None;
        if tab == TAB_GENERAL {
            theme::card(ui, |ui| {
                ui.label(RichText::new("Style for new games").size(16.0).strong());
                theme::small(
                    ui,
                    "Used the first time Preflight sees a game. To change one game's style, pick the game in the bar at the top, then its style next to it.",
                );
                ui.horizontal_wrapped(|ui| {
                    for st in [Style::Competitive, Style::Balanced, Style::Cinematic] {
                        if ui
                            .radio(self.st.default_style == st, style_label(st))
                            .clicked()
                        {
                            self.st.default_style = st;
                            save = true;
                        }
                    }
                });
                ui.add_space(6.0);
                egui::CollapsingHeader::new("Fine-tune")
                    .id_salt("st-fine")
                    .show(ui, |ui| {
                        if self.questions(ui) {
                            save = true;
                        }
                    });
            });
            ui.add_space(10.0);
        }
        if tab == TAB_GENERAL {
            theme::card(ui, |ui| {
                ui.label(RichText::new("Monitor you play on").size(16.0).strong());
                if self.monitor_picker(ui) {
                    save = true;
                }
            });
            ui.add_space(10.0);
        }
        if tab == TAB_TRAY {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("System tray").size(16.0).strong());
                    if self.tray.is_none() {
                        ui.label(RichText::new("No tray found on this desktop").color(pal().muted));
                    }
                });
                save |= ui
                    .checkbox(&mut self.st.minimize_to_tray, "Minimize to the tray")
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Close button:");
                    if ui
                        .radio(!self.st.close_to_tray, "Quits Preflight")
                        .clicked()
                    {
                        self.st.close_to_tray = false;
                        save = true;
                    }
                    if ui
                        .radio(self.st.close_to_tray, "Goes to the tray")
                        .clicked()
                    {
                        self.st.close_to_tray = true;
                        save = true;
                    }
                });
                save |= ui.checkbox(&mut self.st.tray_notifications, "Notifications when a test finishes, a stream ends, or the test flag may still be on").changed();
                ui.add_space(6.0);
                ui.label(RichText::new("When you go live").strong());
                ui.horizontal(|ui| {
                    if ui
                        .radio(
                            !self.st.live_opens_window,
                            "Stay in the tray (the icon turns red)",
                        )
                        .clicked()
                    {
                        self.st.live_opens_window = false;
                        save = true;
                    }
                    if ui
                        .radio(self.st.live_opens_window, "Open the Live page")
                        .clicked()
                    {
                        self.st.live_opens_window = true;
                        save = true;
                    }
                });
                if !self.can_hide {
                    theme::small(
                        ui,
                        "On Wayland a window can't hide itself, so going to the tray closes the window and a small tray-only Preflight keeps watching OBS. Click the icon to open the window again.",
                    );
                }
                theme::small(
                    ui,
                    "Tray icon: grey when OBS isn't connected · plain when ready · lime while testing or live · amber dot when something needs you · red dot when something is wrong.",
                );
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(RichText::new("Startup").size(16.0).strong());
                let mut login_changed = ui
                    .checkbox(
                        &mut self.st.start_at_login,
                        "Start Preflight when you log in",
                    )
                    .changed();
                ui.add_enabled_ui(self.st.start_at_login, |ui| {
                    login_changed |= ui
                        .checkbox(&mut self.st.start_in_tray, "Start in the tray (no window)")
                        .changed();
                });
                if login_changed {
                    save = true;
                    match crate::desktop::set_autostart(
                        self.st.start_at_login,
                        self.st.start_in_tray,
                    ) {
                        Ok(()) => log::info!(
                            "Start at login: {} (tray: {})",
                            self.st.start_at_login,
                            self.st.start_in_tray
                        ),
                        Err(e) => {
                            self.st.start_at_login = false;
                            self.fail_banner =
                                Some(format!("Couldn't set Preflight to start at login: {e}"));
                        }
                    }
                }
                theme::small(
                    ui,
                    "Off by default. Preflight adds its own startup entry and removes it when you turn this off or remove Preflight.",
                );
            });
            ui.add_space(10.0);
        }
        if tab == TAB_OBS {
            theme::card(ui, |ui| {
                ui.label(RichText::new("Where do you stream?").size(16.0).strong());
                theme::small(ui, "Tick every platform you stream to at the same time.");
                ui.horizontal(|ui| {
                    for p in [Platform::Twitch, Platform::YouTube, Platform::Kick] {
                        let mut on = if self.st.platforms.is_empty() {
                            self.st.platform == p
                        } else {
                            self.st.platforms.iter().any(|x| x == p.name())
                        };
                        if ui.checkbox(&mut on, p.name()).changed() {
                            let mut list: Vec<String> = if self.st.platforms.is_empty() {
                                vec![self.st.platform.name().to_string()]
                            } else {
                                self.st.platforms.clone()
                            };
                            list.retain(|x| x != p.name());
                            if on {
                                list.push(p.name().to_string());
                            }
                            if list.is_empty() {
                                list.push(p.name().to_string());
                            }
                            // The main stream follows the strictest platform's limits.
                            self.st.platform =
                                [Platform::Twitch, Platform::Kick, Platform::YouTube]
                                    .into_iter()
                                    .find(|q| list.iter().any(|x| x == q.name()))
                                    .unwrap_or(Platform::Twitch);
                            self.st.platforms = if list.len() > 1 { list } else { Vec::new() };
                            save = true;
                        }
                    }
                });
                if self.st.platforms.len() > 1 {
                    ui.add_space(4.0);
                    ui.label(RichText::new("How do you reach them?").strong());
                    let plugin_seen = self
                        .obs
                        .as_ref()
                        .map(|o| {
                            o.outputs
                                .iter()
                                .any(|x| x.plugin && x.label.starts_with("Extra stream"))
                        })
                        .unwrap_or(false);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .radio(
                                self.st.multistream_how == "plugin",
                                "An OBS plugin (a separate output for each platform)",
                            )
                            .clicked()
                        {
                            self.st.multistream_how = "plugin".into();
                            save = true;
                        }
                        if ui
                            .radio(
                                self.st.multistream_how == "service",
                                "A restream service (Restream or similar: one upload)",
                            )
                            .clicked()
                        {
                            self.st.multistream_how = "service".into();
                            save = true;
                        }
                    });
                    if plugin_seen && self.st.multistream_how != "plugin" {
                        theme::small(
                            ui,
                            "OBS shows extra stream outputs from a plugin, so you probably use a plugin.",
                        );
                    }
                    theme::small(
                        ui,
                        format!(
                            "Your main stream follows {}'s limits; the advice covers each platform.",
                            self.st.platform.name()
                        ),
                    );
                } else {
                    theme::small(ui, crate::network::limits(self.st.platform).note);
                }
            });
            ui.add_space(10.0);
        }
        if tab == TAB_OBS {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("OBS connection").size(16.0).strong());
                    match &self.obs {
                        Some(o) if o.connected => theme::status_pill(
                            ui,
                            Status::Ok,
                            &format!("Connected · OBS {}", o.obs_version),
                        ),
                        _ => theme::status_pill(ui, Status::Warn, "Not connected"),
                    }
                });
                if let Some(o) = self.obs.as_ref().filter(|o| o.connected) {
                    theme::muted(
                        ui,
                        format!(
                            "Streaming {} at {} fps, {}, using {}.",
                            o.output_res
                                .map(|r| format!("{}p", r.1))
                                .unwrap_or_default(),
                            o.fps
                                .map(|f| (f.round() as u32).to_string())
                                .unwrap_or_default(),
                            o.bitrate_kbps
                                .map(|b| format!("{b} kbps"))
                                .unwrap_or_default(),
                            o.encoder_kind
                        ),
                    );
                } else if let Some(e) = self.obs_error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&e).color(pal().warn));
                        if let Some((l, a)) = err_action(&e)
                            && a != NextAct::ObsSettings
                            && ui.small_button(l).clicked()
                        {
                            act = Some(a);
                        }
                    });
                }
                egui::Grid::new("obs-conn")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Password");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.password)
                                .password(true)
                                .desired_width(260.0),
                        );
                        ui.end_row();
                    });
                if ui
                    .checkbox(
                        &mut self.st.obs_remember_password,
                        "Remember the password on this PC",
                    )
                    .changed()
                {
                    save = true;
                    if !self.st.obs_remember_password {
                        crate::config::forget_secret();
                    }
                }
                let busy = self.conn_rx.is_some();
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(if busy {
                            "Connecting…"
                        } else {
                            "Test connection"
                        }),
                    )
                    .clicked()
                {
                    if self.st.obs_remember_password && !self.password.is_empty() {
                        crate::config::save_secret(&self.password);
                    }
                    self.connect_obs(ctx);
                }
                theme::small(
                    ui,
                    "Streamlabs Desktop and Meld Studio support is coming in a later build.",
                );
            });
            ui.add_space(10.0);
        }
        ui.add_space(10.0);
        {
            if tab == TAB_GENERAL {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("You chose to keep").size(16.0).strong());
                    theme::small(
                        ui,
                        "Fixes you decided not to make. They're never hidden: undo any of them here or in Your setup.",
                    );
                    let mut unkeep: Option<String> = None;
                    let mut ungo: Option<String> = None;
                    let mut restore: Option<usize> = None;
                    for id in self.st.kept_issues.clone() {
                        let title = self
                            .analysis
                            .issues
                            .iter()
                            .find(|i| i.id == id)
                            .map(|i| i.title.trim_end_matches('.').to_string())
                            .unwrap_or_else(|| {
                                format!("{} (not an issue right now)", id.replace('_', " "))
                            });
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(title);
                            if ui.small_button("Fix it after all").clicked() {
                                unkeep = Some(id.clone());
                            }
                        });
                    }
                    for k in self.st.go_kept.clone() {
                        let label = match k.as_str() {
                            "mic" => "Ready to go live: microphone check",
                            "audio" => "Ready to go live: game audio check",
                            "capture" => "Ready to go live: whole-screen capture",
                            "cam" => "Ready to go live: hidden webcam",
                            "rec" => "Ready to go live: other recorders running",
                            _ => "Ready to go live: OBS check",
                        };
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(label);
                            if ui.small_button("Check it again").clicked() {
                                ungo = Some(k.clone());
                            }
                        });
                    }
                    for (i, h) in self.st.hidden_options.iter().enumerate() {
                        ui.horizontal(|ui| {
                            tag_pill(ui, analysis::Tag::Kept);
                            ui.label(format!("Not suggested: {}", h.label));
                            if ui.small_button("Suggest again").clicked() {
                                restore = Some(i);
                            }
                        });
                    }
                    if self.st.kept_issues.is_empty()
                        && self.st.go_kept.is_empty()
                        && self.st.hidden_options.is_empty()
                    {
                        theme::muted(ui, "Nothing. Every fix will be suggested when it fits.");
                    }
                    if let Some(id) = unkeep {
                        self.st.kept_issues.retain(|k| k != &id);
                        save = true;
                        recompute = true;
                    }
                    if let Some(k) = ungo {
                        self.st.go_kept.retain(|x| x != &k);
                        save = true;
                    }
                    if let Some(i) = restore {
                        self.st.hidden_options.remove(i);
                        save = true;
                        recompute = true;
                    }
                });
                ui.add_space(10.0);
            }
            if tab == TAB_OBS {
                self.ui_pcs_card(ui, ctx);
                ui.add_space(10.0);
            }
            if tab == TAB_OBS {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("OBS address").size(16.0).strong());
                    theme::small(
                        ui,
                        "Only change these if OBS runs on another PC or uses a different port.",
                    );
                    egui::Grid::new("obs-addr")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Address");
                            save |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.st.obs_host)
                                        .desired_width(200.0),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Port");
                            let mut port = self.st.obs_port.to_string();
                            if ui
                                .add(egui::TextEdit::singleline(&mut port).desired_width(80.0))
                                .changed()
                                && let Ok(p) = port.parse()
                            {
                                self.st.obs_port = p;
                                save = true;
                            }
                            ui.end_row();
                        });
                });
                ui.add_space(10.0);
            }
            if tab == TAB_APPEARANCE {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Theme").size(16.0).strong());
                    ui.horizontal(|ui| {
                        let mut t = self.st.theme;
                        ui.selectable_value(&mut t, theme::ThemeChoice::System, "System");
                        ui.selectable_value(&mut t, theme::ThemeChoice::Dark, "Dark");
                        ui.selectable_value(&mut t, theme::ThemeChoice::Light, "Light");
                        if t != self.st.theme {
                            self.st.theme = t;
                            theme::set_choice(ctx, t, self.st.reduce_motion);
                            save = true;
                        }
                    });
                    theme::small(ui, "System follows your desktop's light or dark setting.");
                    let mut changed = ui
                        .checkbox(&mut self.st.backdrop_grid, "Background grid")
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.st.reduce_motion,
                            "Reduce motion (no animations, plain background)",
                        )
                        .changed();
                    if changed {
                        theme::set_choice(ctx, self.st.theme, self.st.reduce_motion);
                        theme::GRID.store(
                            self.st.backdrop_grid && !self.st.reduce_motion,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        save = true;
                    }
                });
                ui.add_space(10.0);
            }
            if tab == TAB_APPEARANCE {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Display").size(16.0).strong());
                    save |= ui
                        .checkbox(
                            &mut self.st.advanced_view,
                            "Advanced view: show the technical reasons by default",
                        )
                        .changed();
                    ui.horizontal(|ui| {
                        ui.label("Text size");
                        for (v, l) in [
                            (0.9f32, "Smaller"),
                            (1.0, "Normal"),
                            (1.2, "Larger"),
                            (1.4, "Largest"),
                        ] {
                            if ui
                                .selectable_label((self.st.text_scale - v).abs() < 0.01, l)
                                .clicked()
                            {
                                self.st.text_scale = v;
                                ctx.set_zoom_factor(v);
                                save = true;
                            }
                        }
                    });
                });
            }
            if tab == TAB_GENERAL {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Games").size(16.0).strong());
                    theme::small(
                        ui,
                        "Preflight only reads each game's settings file. It never changes it.",
                    );
                    let found: Vec<GameResult> = self
                        .games
                        .iter()
                        .filter(|g| g.relevant())
                        .cloned()
                        .collect();
                    if found.is_empty() && !self.games.is_empty() {
                        theme::muted(ui, "None of the supported games were found on this PC.");
                    }
                    for g in found {
                        let (s, l, d) = if g.files.iter().any(|f| f.read_ok) {
                            (
                                Status::Ok,
                                "Ready",
                                format!(
                                    "Settings read ({} settings)",
                                    g.files.iter().map(|f| f.settings_count).sum::<usize>()
                                ),
                            )
                        } else if g.relevant() {
                            (Status::Warn, "Waiting", "Not launched on this PC yet. Launch it once and change any setting, and Preflight will read it.".to_string())
                        } else {
                            (
                                Status::Unknown,
                                "Not found",
                                "Not installed on this PC.".to_string(),
                            )
                        };
                        theme::inset(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&g.name).strong());
                                theme::status_pill(ui, s, l);
                            });
                            theme::muted(ui, &d);
                            if let Some(n) = &g.platform_note {
                                ui.label(RichText::new(n).size(12.0).color(pal().warn));
                            }
                            if g.relevant() {
                                egui::CollapsingHeader::new(
                                    RichText::new("Where Preflight looked").size(12.0),
                                )
                                .id_salt(format!("where-{}", g.id))
                                .show(ui, |ui| {
                                    for f in &g.files {
                                        theme::small(
                                            ui,
                                            format!(
                                                "{} {}",
                                                if f.found { "Found:" } else { "Not found:" },
                                                f.shown_path
                                            ),
                                        );
                                        if let Some(n) = &f.note {
                                            theme::small(ui, n);
                                        }
                                        if !f.found {
                                            for s in f.searched.iter().skip(1) {
                                                theme::small(ui, format!("  also checked {s}"));
                                            }
                                        }
                                    }
                                    theme::small(
                                        ui,
                                        format!("Profile v{} · {}", g.profile_version, g.verified),
                                    );
                                });
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if self.games.is_empty() {
                        theme::muted(ui, "Scanning for games…");
                    }
                });
            }
            if tab == TAB_UPDATES {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Updates").size(16.0).strong());
                    ui.label(format!(
                        "{} v{} · {}",
                        crate::util::APP_NAME,
                        crate::util::VERSION,
                        crate::util::BUILD_LABEL
                    ));
                    save |= ui
                        .checkbox(&mut self.st.updates_auto, "Check for updates once a day")
                        .changed();
                    theme::small(
                        ui,
                        "Reads GitHub's public list of releases for Matt-B-007/Stream-Preflight; nothing about you or your PC is sent. An update is only installed when it's signed by the maintainer and passes its own self-test.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Get updates from:");
                        let mut beta = self.st.updates_beta;
                        ui.selectable_value(&mut beta, false, "Stable");
                        ui.selectable_value(&mut beta, true, "Beta");
                        if beta != self.st.updates_beta {
                            self.st.updates_beta = beta;
                            self.st.updates_beta_set = true;
                            save = true;
                            self.update_result = None;
                            self.st.last_update_check = 0;
                        }
                    });
                    theme::small(
                        ui,
                        "Betas get new features first and may have rough edges. They're named after the next version, like v0.1.0-beta.4 before v0.1.0.",
                    );
                    if crate::update::is_beta_version(crate::util::VERSION) && !self.st.updates_beta
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("You're on a beta.").color(pal().text_soft));
                            if self.stable_confirm {
                                ui.label(RichText::new("Install the latest stable version? It may be older than this beta; your settings stay.").color(pal().warn));
                                if outlined(ui, "Yes, switch").clicked() {
                                    self.stable_confirm = false;
                                    let (tx, rx) = channel();
                                    std::thread::spawn(move || {
                                        let r = crate::update::check_stable().and_then(|u| crate::update::install(&u));
                                        let _ = tx.send(r);
                                    });
                                    self.update_install = Some(rx);
                                }
                                if outlined(ui, "Cancel").clicked() {
                                    self.stable_confirm = false;
                                }
                            } else if self.update_install.is_none() && outlined(ui, "Switch back to stable").clicked() {
                                self.stable_confirm = true;
                            }
                        });
                        if self.update_install.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Downloading and checking… Preflight restarts by itself.");
                            });
                        }
                    }
                    let busy = self.update_rx.is_some();
                    let mut check = false;
                    ui.horizontal(|ui| {
                        check = ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(if busy { "Checking…" } else { "Check now" }),
                            )
                            .clicked();
                        theme::small(
                            ui,
                            last_checked(self.st.last_update_check, crate::util::unix_now()),
                        );
                    });
                    if check {
                        let (tx, rx) = channel();
                        let beta = self.st.updates_beta;
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::update::check(beta));
                        });
                        self.update_rx = Some(rx);
                    }
                    match &self.update_result {
                        Some(Ok(u)) if u.newer => {
                            let u = u.clone();
                            ui.label(
                                RichText::new(format!(
                                    "Update available: v{} to v{}{}",
                                    u.current,
                                    u.latest,
                                    if u.prerelease { " (beta)" } else { "" }
                                ))
                                .color(pal().accent)
                                .strong(),
                            );
                            if !u.notes.trim().is_empty() {
                                egui::CollapsingHeader::new("What's new")
                                    .id_salt("upd-notes")
                                    .show(ui, |ui| {
                                        theme::muted(ui, &u.notes);
                                    });
                            }
                            match (&self.update_install, &self.update_done) {
                                (Some(_), _) => {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Downloading and checking the update…");
                                    });
                                }
                                (None, Some(Ok(p))) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "v{} is installed. Restart Preflight to use it.",
                                            u.latest
                                        ))
                                        .color(pal().success),
                                    );
                                    if outlined(ui, "Restart now").clicked() {
                                        let p = p.clone();
                                        if crate::install::launch(&p) {
                                            self.quitting = true;
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                        }
                                    }
                                }
                                (None, Some(Err(e))) => {
                                    ui.label(RichText::new(e).color(pal().bad));
                                    ui.hyperlink_to("What's new", &u.url);
                                }
                                (None, None) => {
                                    ui.horizontal(|ui| {
                                        if outlined(ui, &format!("Update to v{}", u.latest))
                                            .clicked()
                                        {
                                            self.start_update();
                                        }
                                        ui.hyperlink_to("What's new", &u.url);
                                    });
                                }
                            }
                        }
                        Some(Ok(u)) => {
                            let older = matches!((semver::Version::parse(&u.latest), semver::Version::parse(&u.current)), (Ok(l), Ok(c)) if l < c);
                            if older {
                                ui.label(RichText::new(format!("No newer stable version yet. You're on v{}; the latest stable is v{}.", u.current, u.latest)).color(pal().text_soft));
                            } else {
                                ui.label(
                                    RichText::new(format!("You're up to date (v{}).", u.current))
                                        .color(pal().success),
                                );
                            }
                        }
                        Some(Err(e)) => theme::muted(ui, e),
                        None => {}
                    }
                    // Go back to the version kept from before the last update.
                    ui.add_space(8.0);
                    if let Some(Ok(r)) = self.rollback_rx.as_ref().map(|rx| rx.try_recv()) {
                        if r.is_ok() {
                            self.prev_version = crate::update::previous_version();
                        }
                        self.rollback_done = Some(r);
                        self.rollback_rx = None;
                    }
                    if let Some(m) = crate::update::managed_install() {
                        theme::small(ui, m);
                    } else if self.rollback_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Going back…");
                        });
                    } else if let Some(Ok((v, p))) = &self.rollback_done {
                        ui.label(
                            RichText::new(format!(
                                "v{v} is back in place. Restart Preflight to use it."
                            ))
                            .color(pal().success),
                        );
                        if outlined(ui, "Restart now").clicked() {
                            let p = p.clone();
                            if crate::install::launch(&p) {
                                self.quitting = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    } else {
                        if let Some(Err(e)) = &self.rollback_done {
                            ui.label(RichText::new(e).color(pal().bad));
                        }
                        match self.prev_version.clone() {
                            Some(pv) => {
                                ui.horizontal_wrapped(|ui| {
                                    if self.rollback_confirm {
                                        ui.label(RichText::new(format!("Go back to v{pv}? Your settings stay, and you can return to v{} the same way.", crate::util::VERSION)).color(pal().warn));
                                        if outlined(ui, "Yes, go back").clicked() {
                                            self.rollback_confirm = false;
                                            let (tx, rx) = channel();
                                            std::thread::spawn(move || {
                                                let _ = tx.send(crate::update::rollback());
                                            });
                                            self.rollback_rx = Some(rx);
                                        }
                                        if outlined(ui, "Cancel").clicked() {
                                            self.rollback_confirm = false;
                                        }
                                    } else if outlined(ui, &format!("Go back to v{pv}")).clicked() {
                                        self.rollback_confirm = true;
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    theme::small(
                                        ui,
                                        "Uses the copy kept on this PC from before the last update. Also from a terminal:",
                                    );
                                    // Monospace, so the two dashes don't join into one.
                                    ui.label(
                                        RichText::new("stream-preflight --rollback")
                                            .monospace()
                                            .size(12.0)
                                            .color(pal().text_soft),
                                    );
                                });
                            }
                            None => theme::small(
                                ui,
                                "Go back: after an update, the version you had is kept here so you can return to it.",
                            ),
                        }
                    }
                });
                ui.add_space(10.0);
            }
            if tab == TAB_TRAY {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Notifications").size(16.0).strong());
                    save |= ui
                        .checkbox(
                            &mut self.st.notify_test_done,
                            "Tell me when a test finishes",
                        )
                        .changed();
                    save |= ui
                        .checkbox(
                            &mut self.st.remind_delete_recording,
                            "Remind me I can delete test recordings",
                        )
                        .changed();
                });
                ui.add_space(10.0);
            }
            if tab == TAB_PRIVACY {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Privacy and data").size(16.0).strong());
                    theme::muted(
                        ui,
                        "Everything stays on this PC: your settings, history and any reports you save. Nothing is sent anywhere unless you send a report yourself.",
                    );
                    save |= ui
                        .checkbox(
                            &mut self.st.network_test_on_scan,
                            format!(
                                "Test my upload speed during scans (uses up to about {} each time)",
                                crate::util::fmt_bytes(crate::network::MAX_TEST_BYTES)
                            ),
                        )
                        .changed();
                    // Storage: everything Preflight keeps, in plain sizes.
                    ui.add_space(4.0);
                    ui.label(RichText::new("Storage used by Preflight").strong());
                    let d = crate::util::dirs();
                    let size_of = |p: &std::path::Path| -> (usize, u64) {
                        std::fs::read_dir(p)
                            .map(|rd| {
                                rd.flatten()
                                    .filter_map(|e| e.metadata().ok())
                                    .filter(|m| m.is_file())
                                    .fold((0usize, 0u64), |(n, b), m| (n + 1, b + m.len()))
                            })
                            .unwrap_or((0, 0))
                    };
                    egui::Grid::new("storage")
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (name, dir, note) in [
                                ("Bug reports", d.reports.clone(), "Deleted after 14 days"),
                                ("Logs", d.logs.clone(), "Deleted after 14 days"),
                                (
                                    "Viewer preview frames",
                                    d.previews.clone(),
                                    "Latest test only",
                                ),
                                (
                                    "Saved results per game",
                                    d.tests.clone(),
                                    "One small file per game",
                                ),
                                ("Past streams", d.streams.clone(), "Post-flight summaries"),
                            ] {
                                let (n, b) = size_of(&dir);
                                ui.label(RichText::new(name).color(pal().text_soft));
                                ui.label(format!(
                                    "{n} file{} · {}",
                                    if n == 1 { "" } else { "s" },
                                    crate::util::fmt_bytes(b)
                                ));
                                ui.horizontal(|ui| {
                                    theme::small(ui, note);
                                    if n > 0 && ui.small_button("Open").clicked() {
                                        report::open_folder(&dir);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    // Test recordings Preflight watched OBS make.
                    let recs: Vec<crate::config::PendingRecording> = self
                        .st
                        .pending_recordings
                        .iter()
                        .filter(|r| std::path::Path::new(&r.path).exists())
                        .cloned()
                        .collect();
                    let total: u64 = recs.iter().map(|r| r.size).sum();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(if recs.is_empty() {
                            "Test recordings: none saved".to_string()
                        } else {
                            format!(
                                "Test recordings: {} file{}, {}",
                                recs.len(),
                                if recs.len() == 1 { "" } else { "s" },
                                crate::util::fmt_bytes(total)
                            )
                        })
                        .strong(),
                    );
                    let mut del: Vec<String> = Vec::new();
                    for r in &recs {
                        ui.horizontal(|ui| {
                            let name = std::path::Path::new(&r.path)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(format!(
                                    "{name} · {}",
                                    crate::util::fmt_bytes(r.size)
                                ))
                                .color(pal().text_soft),
                            );
                            if ui.small_button("Delete").clicked() {
                                del.push(r.path.clone());
                            }
                        });
                    }
                    if recs.len() > 1
                        && theme::secondary(ui, "Delete all test recordings").clicked()
                    {
                        del.extend(recs.iter().map(|r| r.path.clone()));
                    }
                    for p in del {
                        self.delete_recording(ctx, &p);
                    }
                    save |= ui
                        .checkbox(
                            &mut self.st.auto_delete_recordings,
                            "Delete test recordings automatically when Preflight is done with them",
                        )
                        .changed();
                    theme::small(
                        ui,
                        "Only the recordings Preflight's tests made. Your other recordings are never touched. With this on, you can't open a preview moment in your video player afterwards.",
                    );
                    ui.horizontal(|ui| {
                        if theme::secondary(ui, "Open data folder").clicked() {
                            report::open_folder(&crate::util::dirs().data);
                        }
                        if !self.confirm_clear {
                            if theme::secondary(ui, "Clear history").clicked() {
                                self.confirm_clear = true;
                            }
                        } else {
                            ui.label(
                                RichText::new(
                                    "Clear all history, past streams and preview frames?",
                                )
                                .color(pal().warn),
                            );
                            if ui.button("Yes, clear it").clicked() {
                                let d = crate::util::dirs();
                                let stash = undo::stash(
                                    &crate::history::History::files(),
                                    &[&d.streams, &d.previews],
                                );
                                self.history.clear();
                                if let Some(stash) = stash {
                                    self.toast_undo(
                                        ctx,
                                        "History cleared",
                                        undo::Undo::History { stash },
                                    );
                                }
                                self.streams.clear();
                                self.preview = None;
                                self.preview_tex.clear();
                                self.confirm_clear = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_clear = false;
                            }
                        }
                        if theme::secondary(ui, "Forget OBS password").clicked() {
                            let undo = undo::Undo::Password {
                                pw: std::mem::take(&mut self.password),
                                remembered: self.st.obs_remember_password,
                            };
                            crate::config::forget_secret();
                            self.st.obs_remember_password = false;
                            save = true;
                            self.toast_undo(ctx, "OBS password forgotten", undo);
                        }
                    });
                });
                ui.add_space(10.0);
            }
            if tab == TAB_ADVANCED {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Install").size(16.0).strong());
                    let target = crate::install::target()
                        .map(|t| crate::util::scrub(&t.to_string_lossy()))
                        .unwrap_or_default();
                    if crate::install::is_installed() {
                        ui.label(
                            RichText::new(format!("Installed for you at {target}."))
                                .color(pal().success),
                        );
                        theme::small(
                            ui,
                            if cfg!(windows) {
                                "It's in the Start menu, and in Settings › Apps for uninstalling."
                            } else {
                                "It's in your app menu."
                            },
                        );
                    } else {
                        theme::small(
                            ui,
                            if cfg!(windows) {
                                "Copies Preflight to your user folder, adds it to the Start menu and to Settings › Apps (so it can be uninstalled there). No admin rights needed."
                            } else {
                                "Copies Preflight to ~/.local/bin and adds it to your app menu. No admin rights needed."
                            },
                        );
                        theme::small(ui, format!("Goes to {target}"));
                        if crate::install::old_program().is_some() {
                            ui.checkbox(
                                &mut self.install_remove_old,
                                "Also remove the old \"Stream Pre-Flight Check\" program file",
                            );
                        }
                        match &self.install_result {
                            Some(Ok(p)) => {
                                ui.label(RichText::new("Installed.").color(pal().success));
                                if outlined(ui, "Open the installed copy").clicked() {
                                    let p = p.clone();
                                    if crate::install::launch(&p) {
                                        self.quitting = true;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                ui.label(RichText::new(e).color(pal().bad));
                            }
                            None => {
                                if outlined(ui, "Install for this user").clicked() {
                                    self.install_result =
                                        Some(crate::install::install(self.install_remove_old));
                                }
                            }
                        }
                    }
                });
                ui.add_space(10.0);
            }
            if tab == TAB_ABOUT {
                self.ui_about_card(ui, ctx);
                ui.add_space(10.0);
                // The red Uninstall box is always last.
                theme::danger_card(ui, |ui| {
                    ui.label(
                        RichText::new("Uninstall")
                            .size(16.0)
                            .strong()
                            .color(pal().bad),
                    );
                    theme::small(
                        ui,
                        "Removes the program and its app menu entry. You choose whether your settings and logs go too. Never touches OBS or your games.",
                    );
                    if theme::danger(ui, "Uninstall…").clicked() {
                        self.open_uninstall_box();
                    }
                });
            }
        }
        if save {
            self.st.save();
        }
        if recompute || save {
            self.recompute();
        }
        if let Some(a) = act {
            self.do_act(ctx, a);
        }
    }

    /// About: version, licence, links, and where Preflight keeps its files.
    pub(super) fn ui_about_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::card(ui, |ui| {
            ui.label(RichText::new("About").size(16.0).strong());
            ui.label(format!(
                "{} v{} · {}",
                crate::util::APP_NAME,
                crate::util::VERSION,
                crate::util::BUILD_LABEL
            ));
            theme::small(
                ui,
                "Free software under the GNU General Public License, version 3 or later. It comes with no warranty.",
            );
            ui.horizontal_wrapped(|ui| {
                for (label, url) in [
                    (
                        "Project page",
                        "https://github.com/Matt-B-007/Stream-Preflight",
                    ),
                    (
                        "Changelog",
                        "https://github.com/Matt-B-007/Stream-Preflight/blob/main/CHANGELOG.md",
                    ),
                    (
                        "Licence",
                        "https://github.com/Matt-B-007/Stream-Preflight/blob/main/LICENSE",
                    ),
                ] {
                    if ui.link(label).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                    }
                }
            });
            ui.add_space(4.0);
            let d = crate::util::dirs();
            for (what, p) in [
                ("Settings", &d.config),
                ("Data", &d.data),
                ("Logs", &d.logs),
            ] {
                theme::small(
                    ui,
                    format!("{what}: {}", crate::util::scrub(&p.to_string_lossy())),
                );
            }
        });
    }
}
