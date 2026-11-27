// SPDX-License-Identifier: GPL-3.0-or-later
//! Small drawing helpers shared by every screen.

use super::*;

/// Plain-English meanings for terms that appear in fixes.
pub(super) const GLOSSARY: &[(&str, &str)] = &[
    (
        "bitrate",
        "How much data your stream sends each second, in kbps. Higher looks better but needs more upload speed.",
    ),
    (
        "kbps",
        "Kilobits per second: how bitrate is measured. 6000 kbps is 6 Mbps.",
    ),
    (
        "encoder",
        "The part that compresses your video so it can be streamed. It can run on your graphics card (fast) or your CPU (x264).",
    ),
    (
        "nvenc",
        "NVIDIA's video encoder, a separate chip on GeForce cards. It encodes your stream without slowing the game much.",
    ),
    (
        "x264",
        "A software encoder that runs on your CPU. Good quality, but it competes with your game for the CPU.",
    ),
    (
        "cbr",
        "Constant bitrate: the stream sends the same amount of data every second. What Twitch, YouTube and Kick expect.",
    ),
    (
        "vbr",
        "Variable bitrate: the bitrate rises in busy scenes. Good for recordings, risky for live streams.",
    ),
    (
        "keyframe",
        "A full picture in the video. Viewers can only start watching at a keyframe. Every 2 seconds is standard.",
    ),
    (
        "preset",
        "How hard the encoder works on each frame. Slower presets (like P5) look better but take more work.",
    ),
    (
        "db",
        "Decibels: how loudness is measured. 6 dB quieter sounds about half as loud.",
    ),
    (
        "fps",
        "Frames per second: how many pictures per second the game or stream shows.",
    ),
    (
        "downscale",
        "Shrinking the picture from your screen size to your stream size.",
    ),
    (
        "enhanced broadcasting",
        "A Twitch/YouTube feature where your PC encodes several quality versions of the stream at once.",
    ),
    (
        "replay buffer",
        "OBS keeps the last few minutes in memory so you can save a clip after something happens.",
    ),
    (
        "rate control",
        "How the encoder decides how much data to use. CBR is the right choice for streaming.",
    ),
    (
        "dlss",
        "NVIDIA's upscaling: the game renders smaller and sharpens it, which frees up your GPU.",
    ),
    (
        "fsr",
        "AMD's upscaling (works on most cards): the game renders smaller and sharpens it, which frees up your GPU.",
    ),
];

/// Small chips under a fix for terms it uses; hover to see what they mean.
pub(super) fn glossary_chips(ui: &mut Ui, text: &str) {
    let l = text.to_lowercase();
    let found: Vec<&(&str, &str)> = GLOSSARY
        .iter()
        .filter(|(k, _)| {
            let k = *k;
            l.match_indices(k).any(|(i, _)| {
                let before = l[..i]
                    .chars()
                    .last()
                    .map(|c| !c.is_alphanumeric())
                    .unwrap_or(true);
                let after = l[i + k.len()..]
                    .chars()
                    .next()
                    .map(|c| !c.is_alphanumeric() || k == "keyframe")
                    .unwrap_or(true);
                before && after
            })
        })
        .collect();
    if found.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("What do these mean?").size(12.5).color(MUTED));
        for (k, d) in found {
            let label = match *k {
                "db" => "dB".to_string(),
                "kbps" => "kbps".to_string(),
                "nvenc" | "cbr" | "vbr" | "fps" | "dlss" | "fsr" => k.to_uppercase(),
                other => other.to_string(),
            };
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{label} ?"))
                        .size(12.5)
                        .color(INFO)
                        .underline(),
                )
                .sense(egui::Sense::hover()),
            )
            .on_hover_text(*d);
        }
    });
}

/// An outlined button: clickable, but not the main action (only the Next bar is lime).
pub(super) fn outlined(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT).size(14.0))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x4A, 0x53, 0x61)))
            .corner_radius(egui::CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

/// The same Keep / Change / Optional / Kept label everywhere a fix appears.
pub(super) fn tag_pill(ui: &mut Ui, t: analysis::Tag) {
    let (label, fg, bg) = match t {
        analysis::Tag::Keep => ("Keep", GOOD, GOOD_BG),
        analysis::Tag::Change => ("Change", WARN, Color32::from_rgb(0x3A, 0x2E, 0x10)),
        analysis::Tag::Optional => ("Optional", TEXT_SOFT, RAISED),
        analysis::Tag::Kept => ("Kept", MUTED, RAISED),
        analysis::Tag::Unknown => ("Unknown", MUTED, RAISED),
    };
    theme::pill(ui, fg, bg, label);
}

/// "2 to change and 6 optional", always the same wording.
pub(super) fn count_words(imp: usize, opt: usize) -> String {
    match (imp, opt) {
        (0, 0) => "nothing to change".into(),
        (i, 0) => format!("{i} to change"),
        (0, o) => format!("{o} optional"),
        (i, o) => format!("{i} to change and {o} optional"),
    }
}

pub(super) fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) if !s.starts_with("GPU") && !s.starts_with("OBS") && !s.starts_with("CPU") => {
            format!("{}{}", f.to_lowercase(), c.as_str())
        }
        Some(_) => s.to_string(),
        None => String::new(),
    }
}

pub(super) fn link_small(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).size(12.5).color(MUTED).underline()).frame(false))
        .clicked()
}

/// One compact Go-live row: pill, title, detail, optional action button.
pub(super) fn go_row(
    ui: &mut Ui,
    s: Status,
    label: &str,
    title: &str,
    detail: &str,
    button: Option<&str>,
    mut on_click: impl FnMut(),
) {
    egui::Frame::NONE
        .fill(CARD_DARK)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::status_pill(ui, s, label);
                ui.label(RichText::new(title).strong());
                ui.label(RichText::new(detail).size(13.0).color(MUTED));
                if let Some(b) = button {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(b).clicked() {
                            on_click();
                        }
                    });
                }
            });
        });
    ui.add_space(4.0);
}

/// A row the user ticks themselves. Returns true when clicked.
pub(super) fn go_tick_row(ui: &mut Ui, done: bool, title: &str) -> bool {
    let r = egui::Frame::NONE
        .fill(if done { GOOD_BG } else { CARD_DARK })
        .stroke(egui::Stroke::new(
            1.0,
            if done {
                Color32::from_rgb(0x2F, 0x6B, 0x47)
            } else {
                BORDER_STRONG
            },
        ))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if done {
                    theme::status_pill(ui, Status::Ok, "Done");
                } else {
                    theme::pill(ui, TEXT, RAISED, "Tick");
                }
                ui.label(
                    RichText::new(title)
                        .strong()
                        .color(if done { TEXT_SOFT } else { TEXT }),
                );
            });
        })
        .response
        .interact(egui::Sense::click());
    ui.add_space(4.0);
    r.clicked()
}

pub(super) fn status_row(ui: &mut Ui, s: Status, label: &str, title: &str, detail: &str) {
    theme::inset(ui, |ui| {
        ui.horizontal(|ui| {
            theme::status_pill(ui, s, label);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).strong());
                if !detail.is_empty() {
                    theme::muted(ui, detail);
                }
            });
        });
    });
}

pub(super) fn load_tex(
    ctx: &egui::Context,
    path: &std::path::Path,
    name: &str,
) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgb8();
    let (w, h) = img.dimensions();
    let ci = egui::ColorImage::from_rgb([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, ci, egui::TextureOptions::LINEAR))
}

pub(super) fn grade_color(level: u8) -> Color32 {
    match level {
        0 | 1 => GOOD,
        2 => WARN,
        _ => BAD,
    }
}

pub(super) fn style_label(s: Style) -> &'static str {
    match s {
        Style::Competitive => "Competitive",
        Style::Balanced => "Balanced",
        Style::Cinematic => "Cinematic",
        Style::Custom => "Custom",
    }
}

/// The app logo: a paper plane over a check mark.
pub(super) fn draw_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let s = size / 128.0;
    let at = |x: f32, y: f32| egui::pos2(rect.min.x + x * s, rect.min.y + y * s);
    p.rect_filled(
        rect,
        egui::CornerRadius::same((28.0 * s) as u8),
        Color32::from_rgb(0x1C, 0x22, 0x2B),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            at(40.0, 62.0),
            at(100.0, 32.0),
            at(80.0, 78.0),
            at(70.0, 64.0),
        ],
        ACCENT,
        egui::Stroke::NONE,
    ));
    p.add(egui::Shape::line(
        vec![at(30.0, 92.0), at(54.0, 114.0), at(104.0, 66.0)],
        egui::Stroke::new(10.0 * s, ACCENT),
    ));
}

/// The small "DO THIS NEXT" label over a step's title.
pub(super) fn do_next_label(ui: &mut Ui) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(r, egui::CornerRadius::same(2), ACCENT);
        ui.label(theme::caps("Do this next", 12.0, ACCENT));
    });
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tick {
    /// Not yet, and not being watched yet.
    Wait,
    /// Preflight is watching for this right now.
    Watch,
    Done,
    /// Needs fixing before the test.
    Warn,
    /// Worth knowing.
    Info,
}

/// One checklist line: a circle that ticks itself, the text, a note, and any buttons.
pub(super) fn tick_row(ui: &mut Ui, state: Tick, text: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(0x12, 0x16, 0x20, 200))
        .stroke(egui::Stroke::new(
            1.0,
            if state == Tick::Warn {
                WARN
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 14)
            },
        ))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                let c = r.center();
                let p = ui.painter();
                match state {
                    Tick::Done => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x1F, 0x3A, 0x28));
                        p.add(egui::Shape::line(
                            vec![
                                egui::pos2(c.x - 4.5, c.y),
                                egui::pos2(c.x - 1.0, c.y + 3.5),
                                egui::pos2(c.x + 5.0, c.y - 3.5),
                            ],
                            egui::Stroke::new(2.4, GOOD),
                        ));
                    }
                    Tick::Watch => {
                        // An arc that steps round once a second: "watching" (one redraw a second, not 20).
                        let t = ui.input(|i| i.time).floor() as f32;
                        let pts: Vec<egui::Pos2> = (0..=20)
                            .map(|k| {
                                let a = t * std::f32::consts::FRAC_PI_4 + k as f32 / 20.0 * 4.2;
                                egui::pos2(c.x + 9.5 * a.cos(), c.y + 9.5 * a.sin())
                            })
                            .collect();
                        p.circle_stroke(
                            c,
                            9.5,
                            egui::Stroke::new(2.0, Color32::from_rgb(0x2A, 0x33, 0x40)),
                        );
                        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, ACCENT)));
                        theme::repaint_in(ui.ctx(), 1000);
                    }
                    Tick::Wait => {
                        for k in 0..12 {
                            let a0 = k as f32 / 12.0 * std::f32::consts::TAU;
                            let a1 = a0 + 0.28;
                            p.line_segment(
                                [
                                    egui::pos2(c.x + 9.5 * a0.cos(), c.y + 9.5 * a0.sin()),
                                    egui::pos2(c.x + 9.5 * a1.cos(), c.y + 9.5 * a1.sin()),
                                ],
                                egui::Stroke::new(1.8, Color32::from_rgb(0x4A, 0x53, 0x61)),
                            );
                        }
                    }
                    Tick::Warn => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x3A, 0x2A, 0x10));
                        p.text(
                            c,
                            egui::Align2::CENTER_CENTER,
                            "!",
                            egui::FontId::proportional(15.0),
                            WARN,
                        );
                    }
                    Tick::Info => {
                        p.circle_filled(c, 10.5, Color32::from_rgb(0x1C, 0x2A, 0x3A));
                        p.text(
                            c,
                            egui::Align2::CENTER_CENTER,
                            "i",
                            egui::FontId::proportional(14.0),
                            INFO,
                        );
                    }
                }
                // Leave room on the right for "Watching…" and a button, so long text wraps.
                let text_w = (ui.available_width() - 260.0).max(200.0);
                ui.vertical(|ui| {
                    ui.set_max_width(text_w);
                    ui.add(
                        egui::Label::new(RichText::new(text).color(if state == Tick::Done {
                            TEXT_SOFT
                        } else {
                            TEXT
                        }))
                        .wrap(),
                    );
                    if !sub.is_empty() {
                        ui.add(egui::Label::new(RichText::new(sub).size(13.0).color(MUTED)).wrap());
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state == Tick::Done {
                        ui.label(RichText::new("Done").size(13.0).color(GOOD));
                    } else if state == Tick::Watch {
                        ui.label(RichText::new("Watching…").size(13.0).color(MUTED));
                    }
                    add(ui);
                });
            });
        });
    ui.add_space(6.0);
}

#[derive(Clone, Default)]
pub(super) struct ReadingPoint {
    pub(super) gpu: Option<f32>,
    pub(super) enc: Option<f32>,
    pub(super) kbps: Option<f32>,
    pub(super) skipped: Option<f32>,
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Home,
    Test,
    Fix,
    Live,
    History,
    Settings,
    Help,
    Expand,
}

#[derive(Clone, Copy)]
pub(super) enum Badge {
    None,
    Count(usize),
    Tick,
    Dot(Color32),
}

/// Draws one of the sidebar's line icons centred on `c`.
pub(super) fn paint_icon(p: &egui::Painter, c: egui::Pos2, icon: Icon, col: Color32) {
    let s = egui::Stroke::new(1.7, col);
    let pt = |x: f32, y: f32| egui::pos2(c.x + x, c.y + y);
    let line = |pts: Vec<egui::Pos2>| egui::Shape::line(pts, s);
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| {
        let n = 16;
        egui::Shape::line(
            (0..=n)
                .map(|k| {
                    let a = (a0 + (a1 - a0) * k as f32 / n as f32).to_radians();
                    pt(cx + r * a.cos(), cy + r * a.sin())
                })
                .collect(),
            s,
        )
    };
    match icon {
        Icon::Home => {
            p.add(line(vec![pt(-9.0, -1.0), pt(0.0, -8.5), pt(9.0, -1.0)]));
            p.add(line(vec![
                pt(-6.5, -3.0),
                pt(-6.5, 8.0),
                pt(6.5, 8.0),
                pt(6.5, -3.0),
            ]));
            p.add(line(vec![
                pt(-2.0, 8.0),
                pt(-2.0, 3.0),
                pt(2.0, 3.0),
                pt(2.0, 8.0),
            ]));
        }
        Icon::Test => {
            p.add(arc(0.0, 1.5, 8.0, 0.0, 360.0));
            p.add(line(vec![pt(0.0, 1.5), pt(0.0, -3.0)]));
            p.add(line(vec![pt(0.0, 1.5), pt(3.0, 3.5)]));
            p.add(line(vec![pt(-2.5, -9.0), pt(2.5, -9.0)]));
        }
        Icon::Fix => {
            p.add(line(vec![pt(-7.0, 7.0), pt(1.0, -1.0)]));
            p.add(arc(4.0, -4.0, 4.5, 90.0, 360.0));
        }
        Icon::Live => {
            p.circle_filled(c, 2.2, col);
            p.add(arc(0.0, 0.0, 5.5, 135.0, 225.0));
            p.add(arc(0.0, 0.0, 5.5, -45.0, 45.0));
            p.add(arc(0.0, 0.0, 9.0, 135.0, 225.0));
            p.add(arc(0.0, 0.0, 9.0, -45.0, 45.0));
        }
        Icon::History => {
            p.add(arc(0.0, 0.0, 8.5, 0.0, 360.0));
            p.add(line(vec![pt(0.0, -4.5), pt(0.0, 0.0), pt(3.0, 2.0)]));
        }
        Icon::Settings => {
            p.circle_stroke(c, 3.0, s);
            for k in 0..8 {
                let a = (k as f32 * 45.0).to_radians();
                p.add(line(vec![
                    pt(5.5 * a.cos(), 5.5 * a.sin()),
                    pt(8.5 * a.cos(), 8.5 * a.sin()),
                ]));
            }
        }
        Icon::Help => {
            p.add(arc(0.0, 0.0, 8.5, 0.0, 360.0));
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(13.0),
                col,
            );
        }
        Icon::Expand => {
            p.add(line(vec![pt(-2.0, -5.0), pt(3.0, 0.0), pt(-2.0, 5.0)]));
        }
    }
}

/// One icon in the folded sidebar: drawn with lines, a name on hover, and a badge.
pub(super) fn rail_icon(
    ui: &mut Ui,
    icon: Icon,
    tip: &str,
    selected: bool,
    badge: Badge,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(48.0, 42.0), egui::Sense::click());
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip);
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
    let c = rect.center();
    let col = if selected {
        TEXT
    } else {
        Color32::from_rgb(0xB7, 0xC0, 0xCC)
    };
    paint_icon(p, c, icon, col);
    match badge {
        Badge::None => {}
        Badge::Count(n) => {
            let bc = egui::pos2(rect.max.x - 9.0, rect.min.y + 9.0);
            p.circle_filled(bc, 8.0, theme::AMBER);
            p.text(
                bc,
                egui::Align2::CENTER_CENTER,
                n.min(99).to_string(),
                egui::FontId::monospace(10.0),
                Color32::from_rgb(0x1A, 0x12, 0x06),
            );
        }
        Badge::Tick => {
            let b = egui::pos2(rect.max.x - 9.0, rect.max.y - 9.0);
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(b.x - 4.0, b.y),
                    egui::pos2(b.x - 1.0, b.y + 3.0),
                    egui::pos2(b.x + 4.0, b.y - 3.0),
                ],
                egui::Stroke::new(2.0, GOOD),
            ));
        }
        Badge::Dot(dc) => {
            p.circle_filled(egui::pos2(rect.max.x - 9.0, rect.max.y - 9.0), 4.0, dc);
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp
}

pub(super) fn quiet_link(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(14.0).color(TEXT_SOFT).underline()).frame(false),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A one-line banner: a coloured dot, the message (details on hover), and one button.
pub(super) fn slim_banner(
    ui: &mut Ui,
    col: Color32,
    text: &str,
    detail: &str,
    button: &str,
    clicked: &mut bool,
) {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            col.r(),
            col.g(),
            col.b(),
            24,
        ))
        .stroke(egui::Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 120),
        ))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin {
            left: 14,
            right: 8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, col);
                ui.label(RichText::new(text).color(TEXT))
                    .on_hover_text(detail);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if outlined(ui, button).clicked() {
                        *clicked = true;
                    }
                });
            });
        });
}
