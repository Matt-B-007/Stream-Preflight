//! The frame around every screen: sidebar, top status bar, Next bar, banners and toasts.

use super::*;

/// A chip in the top status bar: a dot, a word or two, and a small chevron.
pub(super) fn status_chip(
    ui: &mut Ui,
    dot: Option<Color32>,
    text: &str,
    sub: &str,
    warn: bool,
) -> egui::Response {
    let stroke = if warn {
        Color32::from_rgba_unmultiplied(0xFF, 0xB5, 0x47, 140)
    } else {
        BORDER
    };
    let inner = egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(0x10, 0x15, 0x20, 200))
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                if let Some(d) = dot {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, d);
                }
                ui.label(RichText::new(text).size(13.5).color(TEXT));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(13.5).color(MUTED));
                }
                let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
                let c = r.center();
                ui.painter().add(egui::Shape::line(
                    vec![
                        egui::pos2(c.x - 3.5, c.y - 1.5),
                        egui::pos2(c.x, c.y + 2.0),
                        egui::pos2(c.x + 3.5, c.y - 1.5),
                    ],
                    egui::Stroke::new(1.5, MUTED),
                ));
            });
        });
    let resp = ui
        .interact(
            inner.response.rect,
            inner.response.id.with("chip"),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    resp
}

/// A sidebar row: icon, name and a short description, with a badge on the right.
pub(super) fn nav_row(
    ui: &mut Ui,
    icon: Icon,
    text: &str,
    sub: &str,
    selected: bool,
    badge: Badge,
) -> egui::Response {
    let h = if sub.is_empty() { 36.0 } else { 46.0 };
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    if selected {
        p.rect_filled(
            rect,
            egui::CornerRadius::same(10),
            Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 30),
        );
        p.rect_filled(
            egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            egui::CornerRadius::same(2),
            ACCENT,
        );
    } else if resp.hovered() {
        p.rect_filled(rect, egui::CornerRadius::same(10), RAISED);
    }
    if resp.has_focus() {
        p.rect_stroke(
            rect,
            egui::CornerRadius::same(10),
            egui::Stroke::new(1.5, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let col = if selected {
        TEXT
    } else {
        Color32::from_rgb(0xB7, 0xC0, 0xCC)
    };
    paint_icon(p, egui::pos2(rect.min.x + 22.0, rect.center().y), icon, col);
    let tx = rect.min.x + 44.0;
    if sub.is_empty() {
        p.text(
            egui::pos2(tx, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(15.0),
            col,
        );
    } else {
        p.text(
            egui::pos2(tx, rect.center().y - 8.0),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(15.0),
            col,
        );
        p.text(
            egui::pos2(tx, rect.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(12.0),
            MUTED,
        );
    }
    let bx = rect.max.x - 16.0;
    match badge {
        Badge::None => {}
        Badge::Count(n) => {
            let c = egui::pos2(bx - 2.0, rect.center().y);
            p.circle_filled(c, 10.0, theme::AMBER);
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                n.min(99).to_string(),
                egui::FontId::monospace(11.0),
                Color32::from_rgb(0x1A, 0x12, 0x06),
            );
        }
        Badge::Tick => {
            let b = egui::pos2(bx, rect.center().y);
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(b.x - 5.0, b.y),
                    egui::pos2(b.x - 1.5, b.y + 3.5),
                    egui::pos2(b.x + 5.0, b.y - 3.5),
                ],
                egui::Stroke::new(2.0, theme::TEAL),
            ));
        }
        Badge::Dot(dc) => {
            p.circle_filled(egui::pos2(bx, rect.center().y), 4.5, dc);
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    resp
}

impl App {
    pub(super) fn go(&mut self, s: Screen) {
        if self.screen == Screen::Preflight && s != Screen::Preflight {
            self.stop_watching_if_idle();
        }
        self.screen = s;
        crate::logger::activity(&format!("screen {s:?}"));
    }

    pub(super) fn set_step(&mut self, n: u8) {
        if self.step == 2 && n != 2 {
            self.stop_watching_if_idle();
        }
        if n != 4 && self.returning {
            self.returning = false;
        }
        let not_started = self.fix_idx == 0 && self.fixed.is_empty() && self.skipped.is_empty();
        if n == 3 && (self.fix_queue.is_empty() || not_started) && !self.returning {
            self.fix_queue = fix_order(&self.analysis.issues, &self.st.kept_issues);
            self.fix_idx = 0;
            self.baseline_findings = self.heavy_findings();
            self.baseline_mic_db = self.obs.as_ref().and_then(|o| o.mic_volume_db());
            self.baseline_obs = self.obs.clone();
        }
        self.step = n;
    }

    /// The game menu (switch which game this is about), shared by both sidebars.
    pub(super) fn game_menu(&mut self, resp: &egui::Response) {
        let game = self
            .current_game()
            .or_else(|| self.analysis.setup.game_name.clone());
        let options = self.switch_options();
        let mut pick_game: Option<String> = None;
        egui::Popup::menu(resp)
            .align(egui::RectAlign::BOTTOM_START)
            .width(300.0)
            .show(|ui| {
                if options.is_empty() {
                    ui.label(
                        RichText::new("No games yet. Start one and Preflight spots it.")
                            .color(MUTED),
                    );
                }
                for g in &options {
                    let is_run = self.games.iter().any(|x| &x.name == g && x.running);
                    let when = if is_run {
                        " · running".to_string()
                    } else {
                        self.st
                            .game_memory
                            .get(g)
                            .map(|m| format!(" · tested {}", crate::util::fmt_local(m.time)))
                            .unwrap_or_default()
                    };
                    if ui
                        .selectable_label(Some(g) == game.as_ref(), format!("{g}{when}"))
                        .clicked()
                    {
                        pick_game = Some(g.clone());
                    }
                }
            });
        if let Some(g) = pick_game
            && Some(&g) != game.as_ref()
        {
            let running_name = self
                .games
                .iter()
                .find(|x| x.running)
                .map(|x| x.name.clone());
            self.show_game(&g);
            self.switch_dismissed = running_name;
        }
    }

    /// The style menu, shared by both sidebars.
    pub(super) fn style_menu(&mut self, resp: &egui::Response, ctx: &egui::Context) {
        let mut pick_style: Option<Style> = None;
        egui::Popup::menu(resp)
            .align(egui::RectAlign::BOTTOM_START)
            .width(300.0)
            .show(|ui| {
                for (st, what) in [
                    (
                        Style::Competitive,
                        "Highest frame rate first. Graphics fixes come first.",
                    ),
                    (
                        Style::Balanced,
                        "Smooth game and a clean stream. Upscaling fixes first.",
                    ),
                    (
                        Style::Cinematic,
                        "Best-looking game. A frame rate cap comes first.",
                    ),
                ] {
                    let on = self.st.style == st;
                    let r = ui.add(egui::RadioButton::new(
                        on,
                        RichText::new(style_label(st)).strong(),
                    ));
                    ui.label(RichText::new(what).size(12.5).color(MUTED));
                    ui.add_space(4.0);
                    if r.clicked() {
                        pick_style = Some(st);
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new("Re-sorts your fixes straight away. No new test needed.")
                        .size(12.5)
                        .color(MUTED),
                );
            });
        if let Some(st) = pick_style
            && st != self.st.style
        {
            self.st.apply_style(st);
            self.on_style_changed(ctx);
        }
    }

    /// The sidebar is folded to icons: chosen by the user, or a narrow window.
    pub(super) fn sidebar_folded(&self, width: f32) -> bool {
        self.st.sidebar_collapsed || width < 1100.0
    }

    pub(super) fn ui_sidebar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let folded = self.sidebar_folded(ctx.content_rect().width());
        let mut go_to: Option<Screen> = None;
        let mut step_to: Option<u8> = None;
        let mut toggle = false;
        let on_pf = self.screen == Screen::Preflight;
        let (left, total) = self.fix_progress();
        let test_done = self.test.is_some() || self.returning;
        let game = self
            .current_game()
            .or_else(|| self.analysis.setup.game_name.clone());
        let running = game
            .as_ref()
            .map(|g| self.games.iter().any(|x| &x.name == g && x.running))
            .unwrap_or(false);
        let gtext = game.clone().unwrap_or_else(|| "No game running yet".into());
        let live = self.live.is_some();
        let pw_needed = self
            .obs_error
            .as_deref()
            .map(|e| e.contains("password"))
            .unwrap_or(false);
        let (obs_txt, obs_col) = match &self.obs {
            Some(o) if o.connected => ("OBS connected", GOOD),
            _ if pw_needed => ("OBS needs its password", WARN),
            Some(_) => ("OBS closed", WARN),
            None => ("OBS not found", MUTED),
        };
        let update_line =
            matches!(&self.update_result, Some(Ok(u)) if u.newer && self.update_done.is_none());

        // Five places: Home, Test, Fixes, Live, History. Game and style live in the top bar.
        let home_here = matches!(self.screen, Screen::Home | Screen::Results | Screen::Setup)
            || (on_pf && (self.step == 4 || self.step < 2));
        let test_here = on_pf && self.step == 2;
        let fix_here = on_pf && self.step == 3;
        let hist_here = matches!(self.screen, Screen::PostFlight | Screen::History);
        let live_badge = if live {
            Badge::Dot(Color32::from_rgb(0xFF, 0x5C, 0x6C))
        } else {
            Badge::None
        };
        let fix_badge = if left > 0 {
            Badge::Count(left)
        } else if test_done {
            Badge::Tick
        } else {
            Badge::None
        };
        let _ = (&game, running, &gtext, total);
        if folded {
            // ---- Icon rail
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                draw_logo(ui, 34.0);
                ui.add_space(12.0);
                if rail_icon(ui, Icon::Home, "Home", home_here, Badge::None).clicked() {
                    go_to = Some(Screen::Home);
                }
                ui.add_space(6.0);
                if rail_icon(
                    ui,
                    Icon::Test,
                    if test_done {
                        "Test · done"
                    } else {
                        "Test · before you stream"
                    },
                    test_here,
                    if test_done { Badge::Tick } else { Badge::None },
                )
                .clicked()
                {
                    step_to = Some(2);
                }
                let fix_tip = if left > 0 {
                    format!("Fixes · {left} left")
                } else {
                    "Fixes".to_string()
                };
                if rail_icon(ui, Icon::Fix, &fix_tip, fix_here, fix_badge).clicked() {
                    step_to = Some(3);
                }
                let itip = match &self.live {
                    Some(l) => format!("Live · {}", crate::streamwatch::fmt_t(l.elapsed)),
                    None => "Live · not streaming".to_string(),
                };
                if rail_icon(
                    ui,
                    Icon::Live,
                    &itip,
                    self.screen == Screen::InFlight,
                    live_badge,
                )
                .clicked()
                {
                    go_to = Some(Screen::InFlight);
                }
                if rail_icon(
                    ui,
                    Icon::History,
                    "History · after you stream",
                    hist_here,
                    Badge::None,
                )
                .clicked()
                {
                    self.stream_sel = 0;
                    go_to = Some(Screen::PostFlight);
                }
                let rest = ui.available_height() - 150.0;
                if rest > 0.0 {
                    ui.add_space(rest);
                }
                if rail_icon(
                    ui,
                    Icon::Settings,
                    if update_line {
                        "Settings · an update is available"
                    } else {
                        "Settings"
                    },
                    self.screen == Screen::Settings,
                    if update_line {
                        Badge::Count(1)
                    } else {
                        Badge::None
                    },
                )
                .clicked()
                {
                    go_to = Some(Screen::Settings);
                }
                if rail_icon(
                    ui,
                    Icon::Help,
                    "Help & report a problem",
                    self.screen == Screen::Help,
                    Badge::None,
                )
                .clicked()
                {
                    go_to = Some(Screen::Help);
                }
                let (r, resp) = ui.allocate_exact_size(Vec2::new(48.0, 14.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, obs_col);
                resp.on_hover_text(obs_txt);
                let narrow = ctx.content_rect().width() < 1100.0;
                let t = rail_icon(
                    ui,
                    Icon::Expand,
                    if narrow {
                        "The window is narrow, so the sidebar shows icons"
                    } else {
                        "Show names (Ctrl+B)"
                    },
                    false,
                    Badge::None,
                );
                if t.clicked() && !narrow {
                    toggle = true;
                }
            });
        } else {
            // ---- Full sidebar
            draw_logo(ui, 36.0);
            ui.add_space(16.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            if nav_row(
                ui,
                Icon::Home,
                "Home",
                "Your overview",
                home_here,
                Badge::None,
            )
            .clicked()
            {
                go_to = Some(Screen::Home);
            }
            ui.add_space(8.0);
            if nav_row(
                ui,
                Icon::Test,
                "Test",
                "Before you stream",
                test_here,
                if test_done { Badge::Tick } else { Badge::None },
            )
            .clicked()
            {
                step_to = Some(2);
            }
            if nav_row(
                ui,
                Icon::Fix,
                "Fixes",
                "What to change",
                fix_here,
                fix_badge,
            )
            .clicked()
            {
                step_to = Some(3);
            }
            let live_sub = match &self.live {
                Some(l) => format!("Live · {}", crate::streamwatch::fmt_t(l.elapsed)),
                None => "While you stream".to_string(),
            };
            if nav_row(
                ui,
                Icon::Live,
                "Live",
                &live_sub,
                self.screen == Screen::InFlight,
                live_badge,
            )
            .clicked()
            {
                go_to = Some(Screen::InFlight);
            }
            if nav_row(
                ui,
                Icon::History,
                "History",
                "After you stream",
                hist_here,
                Badge::None,
            )
            .clicked()
            {
                self.stream_sel = 0;
                go_to = Some(Screen::PostFlight);
            }
            let rest = ui.available_height() - if update_line { 140.0 } else { 116.0 };
            if rest > 0.0 {
                ui.add_space(rest);
            }
            if nav_row(
                ui,
                Icon::Settings,
                "Settings",
                "",
                self.screen == Screen::Settings,
                if update_line {
                    Badge::Count(1)
                } else {
                    Badge::None
                },
            )
            .clicked()
            {
                go_to = Some(Screen::Settings);
            }
            if nav_row(
                ui,
                Icon::Help,
                "Help",
                "",
                self.screen == Screen::Help,
                Badge::None,
            )
            .on_hover_text("Help and report a problem")
            .clicked()
            {
                go_to = Some(Screen::Help);
            }
            ui.add_space(6.0);
            if let Some(Ok(u)) = &self.update_result
                && update_line
            {
                let l = format!("v{} available", u.latest);
                if ui
                    .add(egui::Button::new(RichText::new(l).size(12.0).color(ACCENT)).frame(false))
                    .on_hover_text("Update from the banner, or in Settings › Updates")
                    .clicked()
                {
                    go_to = Some(Screen::Settings);
                    self.settings_tab = 4;
                }
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("v{}", crate::util::VERSION))
                        .size(12.0)
                        .monospace()
                        .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("«").size(14.0).color(MUTED))
                                .frame(false),
                        )
                        .on_hover_text("Show icons only (Ctrl+B)")
                        .clicked()
                    {
                        toggle = true;
                    }
                });
            });
        }

        // Keyboard: Alt+1..5 = Home, Test, Fixes, Live, History; Ctrl+B folds the sidebar.
        let (keys, ctrl_b) = ctx.input_mut(|i| {
            let mut k = None;
            for (n, key) in [
                (1u8, egui::Key::Num1),
                (2, egui::Key::Num2),
                (3, egui::Key::Num3),
                (4, egui::Key::Num4),
                (5, egui::Key::Num5),
            ] {
                if i.consume_key(egui::Modifiers::ALT, key) {
                    k = Some(n);
                }
            }
            // Ctrl+B however the platform reports it (some keyboards send Ctrl+Shift states).
            let b = i.consume_key(egui::Modifiers::COMMAND, egui::Key::B)
                || (i.modifiers.command && i.key_pressed(egui::Key::B));
            (k, b)
        });
        if ctrl_b {
            toggle = true;
        }
        if toggle {
            self.st.sidebar_collapsed = !self.st.sidebar_collapsed;
            self.st.save();
            if self.st.sidebar_collapsed && !self.st.hints_seen.iter().any(|h| h == "rail") {
                self.st.hints_seen.push("rail".into());
                self.st.save();
                self.toast(
                    &ctx,
                    "Icons only. Hover an icon to see its name; Ctrl+B brings the names back."
                        .to_string(),
                );
            }
        }
        match keys {
            Some(1) => go_to = Some(Screen::Home),
            Some(2) => step_to = Some(2),
            Some(3) => step_to = Some(3),
            Some(4) => go_to = Some(Screen::InFlight),
            Some(5) => go_to = Some(Screen::PostFlight),
            _ => {}
        }
        if let Some(n) = step_to {
            self.tree_pf_open = true;
            if self.screen != Screen::Preflight {
                self.go(Screen::Preflight);
            }
            self.returning = false;
            self.set_step(n);
        } else if let Some(s) = go_to {
            self.go(s);
        }
    }

    /// The main next action for the current screen, if there's one button to press.
    pub(super) fn next_button(&self) -> Option<(String, NextAct)> {
        match self.screen {
            Screen::InFlight => {
                if self.live.is_some() {
                    Some(("Minimize to tray".into(), NextAct::ToTray))
                } else {
                    Some(("Go to Test".into(), NextAct::Step(2)))
                }
            }
            Screen::Preflight => match self.step {
                2 => {
                    let measuring = self
                        .test_handle
                        .as_ref()
                        .map(|h| h.phase() == Phase::Measuring)
                        .unwrap_or(false);
                    if self.test.is_some() && !self.want_new_test && !measuring {
                        let open: Vec<&Issue> = self
                            .analysis
                            .issues
                            .iter()
                            .filter(|i| !self.st.kept_issues.contains(&i.id))
                            .collect();
                        let important = open.iter().filter(|i| !analysis::is_optional(i)).count();
                        if open.is_empty() {
                            Some(("Ready to go live".into(), NextAct::Step(4)))
                        } else if important == 0 {
                            Some((
                                format!(
                                    "Next: {} optional improvement{}",
                                    open.len(),
                                    if open.len() == 1 { "" } else { "s" }
                                ),
                                NextAct::Step(3),
                            ))
                        } else {
                            Some((format!("Next: {important} to change"), NextAct::Step(3)))
                        }
                    } else if !self.obs.as_ref().map(|o| o.connected).unwrap_or(false) {
                        Some(("Connect OBS".into(), NextAct::ConnectObs))
                    } else if self.test_handle.is_none() {
                        Some(("Start the test".into(), NextAct::ArmTest))
                    } else if !measuring {
                        Some(("Help with OBS".into(), NextAct::ObsHelp))
                    } else {
                        None
                    }
                }
                3 => match (self.current_issue(), &self.detected) {
                    (Some(_), Some(_)) => Some(("Next fix".into(), NextAct::NextIssue)),
                    (Some(i), None) if analysis::is_optional(i) && !self.optional_ok => Some((
                        "Ready to go live".into(),
                        NextAct::FixAction("skip_optional"),
                    )),
                    (Some(_), None) => Some(("I changed it".into(), NextAct::FixAction("manual"))),
                    (None, _) if self.test.is_some() || self.returning => {
                        if self.fixed.iter().any(|id| cur_measured(id)) || !self.fixed.is_empty() {
                            Some(("Check my changes".into(), NextAct::CheckChanges))
                        } else {
                            Some(("Ready to go live".into(), NextAct::Step(4)))
                        }
                    }
                    (None, _) => Some(("Go to the test".into(), NextAct::Step(2))),
                },
                4 if self.go_ready || self.live.is_some() => {
                    Some(("Minimize to tray".into(), NextAct::ToTray))
                }
                _ => None,
            },
            Screen::Results => {
                if self.test.is_none() {
                    Some(("Run the test".into(), NextAct::Step(2)))
                } else if !self.fixed.is_empty() {
                    Some(("Check my changes".into(), NextAct::CheckChanges))
                } else if self
                    .analysis
                    .issues
                    .iter()
                    .any(|i| !self.st.kept_issues.contains(&i.id))
                {
                    Some(("Go to the next fix".into(), NextAct::Step(3)))
                } else {
                    Some(("Ready to go live".into(), NextAct::Step(4)))
                }
            }
            Screen::PostFlight if self.streams.is_empty() && self.live.is_none() => {
                Some(("Start a pre-flight".into(), NextAct::Step(2)))
            }
            Screen::Home => {
                let (imp, opt) = self.open_counts();
                // While the Get started card shows, its glowing button is the one to press.
                let card = !self.st.hints_seen.iter().any(|h| h == "getstarted_hidden")
                    && !self.start_steps().iter().all(|s| s.done);
                let verdict = self.test.is_some()
                    || (self.obs_connected() && imp + opt > 0)
                    || !self.st.game_memory.is_empty();
                if card || verdict {
                    // Home has its own glowing button; the Next bar only gives the line.
                    None
                } else if !self.obs_connected() {
                    Some(("Connect OBS".into(), NextAct::ConnectObs))
                } else if self.test.is_none() || self.want_new_test {
                    Some(("Open Test".into(), NextAct::Step(2)))
                } else if imp > 0 {
                    Some((format!("Next: {imp} to change"), NextAct::Step(3)))
                } else if opt > 0 {
                    Some(("See the optional fixes".into(), NextAct::Step(3)))
                } else {
                    Some(("Minimize to tray".into(), NextAct::ToTray))
                }
            }
            Screen::History | Screen::Help | Screen::Settings => {
                Some(("Back to Home".into(), NextAct::Screen(Screen::Home)))
            }
            _ => None,
        }
    }

    pub(super) fn ui_next_bar(&mut self, ui: &mut Ui) {
        let line = self.next_line();
        let button = self.next_button();
        let mut clicked: Option<NextAct> = None;
        ui.horizontal(|ui| {
            ui.label(theme::caps("Next", 12.0, ACCENT));
            let room = ui.available_width() - if button.is_some() { 260.0 } else { 0.0 };
            ui.allocate_ui(Vec2::new(room.max(100.0), 40.0), |ui| {
                ui.label(RichText::new(line).color(TEXT));
            });
            if let Some((label, act)) = &button {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::glow_button(ui, label)
                        .on_hover_text("Ctrl+Enter")
                        .clicked()
                    {
                        clicked = Some(*act);
                    }
                });
            }
        });
        // Keyboard: Ctrl+Enter does the Next step from anywhere.
        if clicked.is_none()
            && ui
                .ctx()
                .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
        {
            clicked = button.as_ref().map(|b| b.1);
        }
        if let Some(a) = clicked {
            let ctx = ui.ctx().clone();
            self.do_act(&ctx, a);
        }
    }

    pub(super) fn do_act(&mut self, ctx: &egui::Context, a: NextAct) {
        match a {
            NextAct::Step(n) => {
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.returning = false;
                self.set_step(n);
            }
            NextAct::Screen(s) => self.go(s),
            NextAct::NextIssue => {
                self.detected = None;
                self.show_other_fixes = false;
                self.fix_idx += 1;
            }
            NextAct::CheckChanges => self.check_changes(ctx),
            NextAct::ObsSettings => self.go(Screen::Settings),
            NextAct::ObsHelp => {
                self.help_open = Some(0);
                self.go(Screen::Help);
            }
            NextAct::FixAction(a) => {
                self.queued_fix_action = Some(a.to_string());
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.step = 3;
            }
            NextAct::ToTray => self.hide_to_tray(ctx),
            NextAct::ConnectObs => self.home_act(HomeAct::ConnectObs),
            NextAct::ArmTest => {
                self.want_new_test = true;
                self.watch_retry_at = 0.0;
                if self.screen != Screen::Preflight {
                    self.go(Screen::Preflight);
                }
                self.step = 2;
            }
        }
    }

    /// Starts a new test and compares it with the last one when it finishes.
    pub(super) fn check_changes(&mut self, _ctx: &egui::Context) {
        if let Some(t) = self.test.clone() {
            self.prev_test = Some((t, self.analysis.smoothness.unwrap_or(self.analysis.score)));
        }
        self.compare = None;
        self.quick_next = true;
        self.show_bw_help = false;
        self.want_new_test = true;
        self.watch_retry_at = 0.0;
        self.returning = false;
        if self.screen != Screen::Preflight {
            self.go(Screen::Preflight);
        }
        self.set_step(2);
        log::info!("Check my changes: new test requested");
    }

    pub(super) fn ui_banners(&mut self, ui: &mut Ui) {
        if let Some(job) = &self.scan
            && !matches!(self.screen, Screen::Preflight | Screen::Results)
        {
            theme::inset(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&job.step);
                });
            });
            ui.add_space(10.0);
        }
        if self.banner_pick() != 3 {
            return;
        }
        if let Some(msg) = self.fail_banner.clone() {
            let mut report = false;
            let mut dismiss = false;
            egui::Frame::NONE
                .fill(BAD_BG)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x6B, 0x2B, 0x2B)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("Something went wrong. {msg}")).strong());
                        ui.label(
                            RichText::new("The details are attached if you save a report.")
                                .color(TEXT_SOFT),
                        );
                    });
                    ui.horizontal(|ui| {
                        if let Some((l, a)) = err_action(&msg)
                            && theme::secondary(ui, l).clicked()
                        {
                            self.pending_act = Some(a);
                            dismiss = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Save report").color(BG).strong())
                                    .fill(BAD),
                            )
                            .clicked()
                        {
                            report = true;
                        }
                        if link_small(ui, "Dismiss") {
                            dismiss = true;
                        }
                    });
                });
            if report {
                self.open_report(msg.clone());
                self.fail_banner = None;
            }
            if dismiss {
                self.fail_banner = None;
            }
            ui.add_space(12.0);
        }
    }

    pub(super) fn ui_toast(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some((msg, until)) = self.toast.clone() {
            if now > until {
                self.toast = None;
                return;
            }
            egui::Area::new(egui::Id::unique("toast-area"))
                .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0))
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(RAISED)
                        .stroke(egui::Stroke::new(1.0, BORDER_STRONG))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.label(RichText::new(msg).strong());
                        });
                });
            theme::repaint_in(ctx, 250);
        }
    }

    /// Only one banner shows at a time, most important first:
    /// 1 real-stream question, 2 remove the test flag, 3 something went wrong,
    /// 4 stream ended, 5 you're live.
    pub(super) fn banner_pick(&self) -> u8 {
        let testing = self.test_handle.is_some();
        if self.flag_question {
            1
        } else if self.st.bandwidth_test_pending && !testing {
            2
        } else if self.fail_banner.is_some() {
            3
        } else if self.stream_notice.is_some()
            && self.live.is_none()
            && !(self.screen == Screen::PostFlight && self.stream_sel == 0)
        {
            4
        } else if self.update_banner() {
            6
        } else {
            0
        }
    }

    /// A newer version exists and the user hasn't put it off this session.
    pub(super) fn update_banner(&self) -> bool {
        !self.update_later
            && matches!(&self.update_result, Some(Ok(u)) if u.newer)
            && self.live.is_none()
            && !self
                .test_handle
                .as_ref()
                .map(|h| h.phase() == Phase::Measuring)
                .unwrap_or(false)
    }

    /// Starts downloading and installing the update in the background.
    pub(super) fn start_update(&mut self) {
        if let Some(Ok(u)) = &self.update_result {
            let (tx, rx) = channel();
            let uu = u.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::update::install(&uu));
            });
            self.update_install = Some(rx);
            self.update_done = None;
        }
    }

    pub(super) fn ui_update_banner(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(Ok(u)) = self.update_result.clone() else {
            return;
        };
        let mut act: Option<&str> = None;
        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 18))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 120)))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("Stream Preflight v{} is available{}", u.latest, if u.prerelease { " (beta)" } else { "" })).size(17.0).strong().color(ACCENT));
                        match (&self.update_install, &self.update_done) {
                            (Some(_), _) => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Downloading and checking it… Preflight restarts by itself when it's ready.").color(TEXT_SOFT));
                                });
                            }
                            (None, Some(Err(e))) => {
                                ui.label(RichText::new(format!("The update didn't install: {e}")).color(BAD));
                            }
                            _ => {
                                ui.label(RichText::new(format!("You have v{}. Updating takes a few seconds and keeps all your settings.", u.current)).color(TEXT_SOFT));
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.update_install.is_none() {
                            if outlined(ui, "Later").clicked() {
                                act = Some("later");
                            }
                            if !u.url.is_empty() && outlined(ui, "What's new").clicked() {
                                act = Some("notes");
                            }
                            if outlined(ui, if matches!(self.update_done, Some(Err(_))) { "Try again" } else { "Update now" }).clicked() {
                                act = Some("update");
                            }
                        }
                    });
                });
            });
        ui.add_space(10.0);
        match act {
            Some("update") => self.start_update(),
            Some("later") => self.update_later = true,
            Some("notes") => ctx.open_url(egui::OpenUrl::new_tab(&u.url)),
            _ => {}
        }
    }

    pub(super) fn ui_live_banners(&mut self, ui: &mut Ui) {
        let mut go = false;
        let pick = self.banner_pick();
        if pick == 6 {
            let ctx = ui.ctx().clone();
            self.ui_update_banner(ui, &ctx);
        }
        if pick == 1 {
            let mut answer: Option<bool> = None;
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x3A, 0x2A, 0x10))
                .stroke(egui::Stroke::new(1.0, WARN))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Is this a real stream?").size(17.0).strong().color(WARN));
                    theme::muted(ui, "You started streaming, but Preflight still thinks ?bandwidthtest=true is in your stream key and no test is running. If the flag is still there, Twitch hides your stream from viewers. OBS › Settings › Stream › Stream Key.");
                    ui.horizontal_wrapped(|ui| {
                        if outlined(ui, "It's real: I removed the flag").clicked() {
                            answer = Some(true);
                        }
                        if outlined(ui, "It's a test stream").clicked() {
                            answer = Some(false);
                        }
                    });
                });
            match answer {
                Some(true) => {
                    self.flag_question = false;
                    self.flag_real = true;
                    self.st.bandwidth_test_pending = false;
                    self.st.save();
                    log::info!(
                        "Forgotten-flag check: user says the stream is real and the flag is removed"
                    );
                }
                Some(false) => {
                    self.flag_question = false;
                    log::info!("Forgotten-flag check: user says it's a test stream");
                }
                None => {}
            }
            ui.add_space(10.0);
        }
        if pick == 2 {
            let mut removed = false;
            slim_banner(
                ui,
                theme::AMBER,
                "Remove ?bandwidthtest=true from your stream key before you go live.",
                "While it's there, Twitch accepts your stream but shows it to no one. OBS › Settings › Stream › Stream Key.",
                "I removed it",
                &mut removed,
            );
            if removed {
                self.st.bandwidth_test_pending = false;
                self.st.save();
                log::info!("User confirmed removing the bandwidth-test flag");
            }
            ui.add_space(10.0);
        }
        if self.screen == Screen::PostFlight && self.stream_sel == 0 {
            self.stream_notice = None;
        }
        if let Some(n) = self.stream_notice.clone().filter(|_| pick == 4) {
            let p = crate::streamwatch::summarize(&n);
            let mut dismiss = false;
            let mut see = false;
            let text = format!(
                "Your {} ended · {} · {}",
                if n.bandwidth_test {
                    "test stream"
                } else {
                    "stream"
                },
                crate::streamwatch::fmt_duration(p.duration),
                p.grade
            );
            slim_banner(
                ui,
                grade_color(p.grade_level),
                &text,
                "Your post-flight summary is ready",
                "See the summary",
                &mut see,
            );
            if see {
                go = true;
                dismiss = true;
            }
            ui.add_space(10.0);
            if dismiss {
                self.stream_notice = None;
            }
        }
        if go {
            self.stream_sel = 0;
            self.go(Screen::PostFlight);
        }
    }

    pub(super) fn obs_connected(&self) -> bool {
        self.obs.as_ref().map(|o| o.connected).unwrap_or(false)
    }

    /// The top bar: OBS, game, style and platforms, each one click from changing.
    pub(super) fn ui_status_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let pw_needed = self
            .obs_error
            .as_deref()
            .map(|e| e.contains("password"))
            .unwrap_or(false);
        let connected = self.obs_connected();
        let game = self
            .current_game()
            .or_else(|| self.analysis.setup.game_name.clone());
        let running = game
            .as_ref()
            .map(|g| self.games.iter().any(|x| &x.name == g && x.running))
            .unwrap_or(false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (otext, osub) = if connected {
                ("OBS connected".to_string(), String::new())
            } else if pw_needed {
                (
                    "OBS needs its password".to_string(),
                    "· Connect".to_string(),
                )
            } else {
                ("OBS not connected".to_string(), "· Connect".to_string())
            };
            let r = status_chip(
                ui,
                Some(if connected { theme::TEAL } else { WARN }),
                &otext,
                &osub,
                !connected,
            )
            .on_hover_text(if connected {
                "Connection and two-PC settings"
            } else {
                "Turn on OBS's WebSocket server and connect"
            });
            if r.clicked() {
                if connected {
                    self.settings_tab = 1;
                    self.go(Screen::Settings);
                } else {
                    self.home_act(HomeAct::ConnectObs);
                }
            }
            let gtext = game.clone().unwrap_or_else(|| "No game running".into());
            let gr = status_chip(
                ui,
                if running { Some(theme::TEAL) } else { None },
                &gtext,
                "",
                false,
            )
            .on_hover_text("Switch game");
            self.game_menu(&gr);
            let sr = status_chip(ui, None, style_label(self.st.style), "", false)
                .on_hover_text("How Preflight orders your fixes");
            self.style_menu(&sr, &ctx);
            let platforms = if self.st.platforms.is_empty() {
                self.st.platform.name().to_string()
            } else {
                self.st.platforms.join(" + ")
            };
            if status_chip(ui, None, &platforms, "", false)
                .on_hover_text("Where you stream")
                .clicked()
            {
                self.settings_tab = 0;
                self.go(Screen::Settings);
            }
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match &self.live {
                    Some(l) => {
                        ui.label(
                            RichText::new(format!(
                                "Live · {}",
                                crate::streamwatch::fmt_t(l.elapsed)
                            ))
                            .size(13.0)
                            .color(BAD),
                        );
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, BAD);
                    }
                    None => {
                        ui.label(RichText::new("Not live").size(13.0).color(MUTED));
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            r.center(),
                            4.0,
                            Color32::from_rgb(0x4A, 0x53, 0x61),
                        );
                    }
                },
            );
        });
    }
}
