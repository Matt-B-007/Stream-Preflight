//! The system tray: status, hiding to the tray, and the Wayland hand-over.

use super::*;

impl App {
    pub(super) fn tray_logic(&mut self, ctx: &egui::Context) {
        if !self.tray_tried {
            self.tray_tried = true;
            self.tray = crate::tray::Tray::start(ctx.clone());
            if let Some(t) = &mut self.tray
                && self.win_hwnd != 0
            {
                t.set_window(self.win_hwnd);
            }
        }
        // Commands from the tray menu.
        let cmds: Vec<crate::tray::TrayCmd> = self
            .tray
            .as_ref()
            .map(|t| std::iter::from_fn(|| t.poll()).collect())
            .unwrap_or_default();
        for c in cmds {
            match c {
                crate::tray::TrayCmd::Open => self.show_from_tray(ctx),
                crate::tray::TrayCmd::Quit => {
                    self.quitting = true;
                    self.show_from_tray(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // Close button: quit, or hide to the tray if that's the setting.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting
            && self.st.close_to_tray
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
        // Minimize: to the tray, where the window can really hide.
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if minimized
            && !self.hidden
            && self.st.minimize_to_tray
            && self.tray.is_some()
            && (self.can_hide || self.test_handle.is_none())
            && !self.quitting
        {
            self.hide_to_tray(ctx);
        }
        // Windows started with --tray (at login): go straight to the tray.
        if self.start_hidden && self.tray.is_some() {
            self.start_hidden = false;
            self.hide_to_tray(ctx);
        }
        if !minimized && self.hidden && !self.can_hide {
            // Wayland: the user brought the window back from the taskbar.
            self.hidden = false;
            theme::HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        // What the icon shows.
        let measuring = self
            .test_handle
            .as_ref()
            .map(|h| h.phase() == Phase::Measuring)
            .unwrap_or(false);
        let obs_on = self.obs.as_ref().map(|o| o.connected).unwrap_or(false);
        let (state, status) = if let Some(l) = &self.live {
            (
                crate::tray::TrayState::Live,
                format!("Live · {}", crate::streamwatch::fmt_t(l.elapsed)),
            )
        } else if measuring {
            (
                crate::tray::TrayState::Testing,
                "Testing · keep playing".to_string(),
            )
        } else if self.test_handle.is_some() {
            (
                crate::tray::TrayState::Watching,
                "Waiting for OBS to start the test".to_string(),
            )
        } else if obs_on && self.st.last_fix_count > 0 {
            (
                crate::tray::TrayState::NeedsFixes,
                format!("{} to fix before you go live", self.st.last_fix_count),
            )
        } else if obs_on {
            (
                crate::tray::TrayState::Watching,
                "Watching OBS · ready".to_string(),
            )
        } else {
            (
                crate::tray::TrayState::Idle,
                "OBS not connected".to_string(),
            )
        };
        let attention = self.flag_question
            || (self.st.bandwidth_test_pending
                && !measuring
                && self.live.is_none()
                && self.test_handle.is_none())
            || self.fail_banner.is_some()
            || (self.hidden && self.stream_notice.is_some());
        let status = if self.flag_question {
            "Needs you: is this a real stream?".to_string()
        } else if attention && self.st.bandwidth_test_pending {
            format!("{status} · remove the test flag")
        } else {
            status
        };
        if let Some(t) = &mut self.tray {
            t.set(crate::tray::TrayView {
                state,
                attention,
                status,
            });
        }
    }

    /// A desktop notification, only when the window isn't in front.
    pub(super) fn notify_away(&self, ctx: &egui::Context, title: &str, body: &str) {
        if !self.st.tray_notifications {
            return;
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if (self.hidden || !focused)
            && let Some(t) = &self.tray
        {
            t.notify(title, body);
        }
    }

    /// Wayland: a window can't hide itself, so close it and let a small
    /// tray-only Preflight keep watching OBS (unless a test is running).
    pub(super) fn hand_over_to_tray(&mut self, ctx: &egui::Context) -> bool {
        if self.test_handle.is_some() || self.tray.is_none() || !cfg!(target_os = "linux") {
            return false;
        }
        self.st.save();
        if let Some(w) = &self.watch {
            w.stop_and_wait(std::time::Duration::from_secs(3));
        }
        if !crate::traymode::start_tray_process() {
            log::warn!("Couldn't start the tray-only Preflight; minimizing instead");
            return false;
        }
        log::info!("Handed over to the tray-only Preflight");
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        true
    }

    pub(super) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        if !self.can_hide && self.hand_over_to_tray(ctx) {
            return;
        }
        let Some(t) = &self.tray else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            return;
        };
        if !t.show_window(false) {
            if self.can_hide {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        self.hidden = true;
        theme::HIDDEN.store(true, std::sync::atomic::Ordering::Relaxed);
        log::info!("Window sent to the tray");
        if !self.tray_hint_shown {
            self.tray_hint_shown = true;
            if self.st.tray_notifications {
                t.notify(
                    "Preflight is in the tray",
                    "It keeps watching OBS. Click the icon to open it.",
                );
            }
        }
    }

    pub(super) fn show_from_tray(&mut self, ctx: &egui::Context) {
        let native = self
            .tray
            .as_ref()
            .map(|t| t.show_window(true))
            .unwrap_or(false);
        if !native {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.hidden = false;
        theme::HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        ctx.request_repaint();
    }
}
