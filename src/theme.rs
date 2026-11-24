//! Colors, fonts sizes and small reusable widgets.

use crate::analysis::Status;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x06, 0x08, 0x10);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(8, 10, 16, 205);
pub const CARD: Color32 = Color32::from_rgba_premultiplied(12, 15, 23, 190);
pub const CARD_DARK: Color32 = Color32::from_rgba_premultiplied(7, 9, 14, 200);
pub const RAISED: Color32 = Color32::from_rgb(0x1E, 0x23, 0x2C);
pub const BORDER: Color32 = Color32::from_rgba_premultiplied(20, 20, 22, 20);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x33, 0x3A, 0x46);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xED);
pub const TEXT_SOFT: Color32 = Color32::from_rgb(0xC9, 0xCF, 0xD8);
pub const MUTED: Color32 = Color32::from_rgb(0x95, 0xA2, 0xB4);
pub const ACCENT: Color32 = Color32::from_rgb(0xC6, 0xF3, 0x6B);
pub const ACCENT_BG: Color32 = Color32::from_rgb(0x23, 0x2B, 0x17);
pub const GOOD: Color32 = Color32::from_rgb(0x5E, 0xF0, 0xB5);
pub const GOOD_BG: Color32 = Color32::from_rgb(0x14, 0x30, 0x1F);
pub const WARN: Color32 = Color32::from_rgb(0xFC, 0xD3, 0x4D);
pub const WARN_BG: Color32 = Color32::from_rgb(0x3A, 0x2E, 0x0E);
pub const BAD: Color32 = Color32::from_rgb(0xFC, 0xA5, 0xA5);
pub const BAD_BG: Color32 = Color32::from_rgb(0x3B, 0x17, 0x17);
pub const INFO: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xFC);
pub const TEAL: Color32 = Color32::from_rgb(0x5E, 0xF0, 0xB5);
pub const AMBER: Color32 = Color32::from_rgb(0xFF, 0xB5, 0x47);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    // Popups and menus stay solid so text over them is easy to read.
    v.window_fill = Color32::from_rgb(0x0E, 0x12, 0x1C);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = CARD_DARK;
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgb(0x3D, 0x4A, 0x22);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    // A clear outline on whatever has keyboard focus.
    v.widgets.hovered.fg_stroke.width = v.widgets.hovered.fg_stroke.width.max(1.5);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x28, 0x2E, 0x39);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x28, 0x2E, 0x39);
    v.widgets.active.bg_fill = Color32::from_rgb(0x30, 0x37, 0x44);
    // Avoid pure white text: some OpenGL drivers render it incorrectly.
    v.widgets.active.fg_stroke = Stroke::new(1.5, Color32::from_rgb(0xF2, 0xF3, 0xF5));
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::from_rgb(0xF2, 0xF3, 0xF5));
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    // Always dark, even when Windows or the desktop is in light mode.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(10.0, 8.0);
        s.spacing.button_padding = Vec2::new(14.0, 8.0);
        s.spacing.interact_size.y = 32.0;
        s.spacing.slider_width = 280.0;
        s.spacing.window_margin = Margin::same(18);
        use egui::{FontFamily, FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
        ]
        .into();
    });
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add).inner
        })
        .inner
}

pub fn inset<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(CARD_DARK)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add).inner
        })
        .inner
}

pub fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(28.0).strong());
}

pub fn subtitle(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(15.0).color(MUTED));
}

pub fn muted(ui: &mut Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).color(MUTED));
}

pub fn small(ui: &mut Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).size(12.5).color(MUTED));
}

pub fn primary(ui: &mut Ui, s: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(s).color(BG).strong())
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 40.0)),
    )
}


pub fn secondary(ui: &mut Ui, s: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(s).color(TEXT).strong())
            .fill(CARD)
            .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(40, 40, 44, 40)))
            .corner_radius(CornerRadius::same(10))
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

pub fn status_colors(s: Status) -> (Color32, Color32, &'static str) {
    match s {
        Status::Ok => (GOOD, GOOD_BG, "OK"),
        Status::Warn => (WARN, WARN_BG, "CHECK"),
        Status::Bad => (BAD, BAD_BG, "FIX"),
        Status::Unknown => (MUTED, RAISED, "UNKNOWN"),
    }
}

pub fn pill(ui: &mut Ui, fg: Color32, bg: Color32, text: &str) {
    Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.5).color(fg).strong());
        });
}

pub fn status_pill(ui: &mut Ui, s: Status, label: &str) {
    let (fg, bg, _) = status_colors(s);
    pill(ui, fg, bg, label);
}


pub fn measured_tag(ui: &mut Ui, measured: bool) {
    if measured {
        ui.label(RichText::new("Measured on this PC").size(12.0).color(GOOD));
    } else {
        ui.label(RichText::new("Estimate").size(12.0).color(MUTED));
    }
}

/// Tag for things read exactly from a settings file or OBS.
pub fn read_tag(ui: &mut Ui, from: &str) {
    ui.label(RichText::new(format!("Read from {from}")).size(12.0).color(INFO));
}

pub fn score_color(score: u8) -> Color32 {
    if score >= 90 {
        GOOD
    } else if score >= 70 {
        WARN
    } else {
        BAD
    }
}

/// A thin progress bar.
pub fn bar(ui: &mut Ui, frac: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(99), BORDER);
    let mut r = rect;
    r.set_width((w * frac.clamp(0.0, 1.0)).max(height));
    p.rect_filled(r, CornerRadius::same(99), color);
}

/// Simple bar graph of values 0..=max.
pub fn spark_bars(ui: &mut Ui, values: &[f32], max: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), CARD_DARK);
    if values.is_empty() {
        return;
    }
    let n = values.len().min(240);
    let vals = &values[values.len() - n..];
    let gap = 2.0;
    let bw = ((w - gap * (n as f32 - 1.0)) / n as f32).clamp(1.0, 28.0);
    for (i, v) in vals.iter().enumerate() {
        let h = (v / max).clamp(0.0, 1.0) * (height - 4.0);
        let x = rect.left() + i as f32 * (bw + gap);
        let r = egui::Rect::from_min_max(egui::pos2(x, rect.bottom() - h - 2.0), egui::pos2(x + bw, rect.bottom() - 2.0));
        p.rect_filled(r, CornerRadius::same(2), color);
    }
}


// ---------------------------------------------------------------------------
// Cockpit glass: backdrop, glowing card edge, dials, line graphs, readings.

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// Lime → teal → violet along the diagonal, like the design's card edge.
pub fn edge_color(t: f32, alpha: u8) -> Color32 {
    let lime = Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, alpha);
    let teal = Color32::from_rgba_unmultiplied(0x5E, 0xF0, 0xB5, (alpha as f32 * 0.45) as u8);
    let violet = Color32::from_rgba_unmultiplied(0x78, 0x5A, 0xFF, (alpha as f32 * 0.6) as u8);
    if t < 0.45 { lerp(lime, teal, t / 0.45) } else { lerp(teal, violet, (t - 0.45) / 0.55) }
}

/// The window background: deep navy, a green glow top right, a violet glow
/// bottom left, and a faint lime grid. Painted once behind everything.
pub fn paint_backdrop(ctx: &egui::Context) {
    let rect = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::background());
    p.rect_filled(rect, 0.0, BG);
    glow(&p, egui::pos2(rect.max.x - rect.width() * 0.12, rect.min.y), rect.width() * 0.5, Color32::from_rgba_unmultiplied(0x5E, 0xF0, 0xB5, 46));
    glow(&p, egui::pos2(rect.min.x + rect.width() * 0.35, rect.max.y + 40.0), rect.width() * 0.45, Color32::from_rgba_unmultiplied(0x78, 0x5A, 0xFF, 46));
    let line = Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 7);
    let step = 28.0;
    let mut x = rect.min.x;
    while x < rect.max.x {
        p.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], Stroke::new(1.0, line));
        x += step;
    }
    let mut y = rect.min.y;
    while y < rect.max.y {
        p.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], Stroke::new(1.0, line));
        y += step;
    }
}

/// A soft round glow: a fan whose centre is coloured and edge transparent.
fn glow(p: &egui::Painter, c: egui::Pos2, r: f32, col: Color32) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let mut m = Mesh::default();
    let rings = [(0.0, 1.0), (0.35, 0.7), (0.7, 0.3), (1.0, 0.0)];
    let seg = 48;
    for (ri, (f, a)) in rings.iter().enumerate() {
        let colr = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (col.a() as f32 * a) as u8);
        if ri == 0 {
            m.vertices.push(Vertex { pos: c, uv: WHITE_UV, color: colr });
            continue;
        }
        for k in 0..seg {
            let ang = k as f32 / seg as f32 * std::f32::consts::TAU;
            m.vertices.push(Vertex { pos: egui::pos2(c.x + r * f * ang.cos(), c.y + r * f * ang.sin()), uv: WHITE_UV, color: colr });
        }
    }
    let ring = |i: usize| 1 + (i - 1) * seg as usize;
    for k in 0..seg as u32 {
        let a = 1 + k;
        let b = 1 + (k + 1) % seg as u32;
        m.indices.extend_from_slice(&[0, a, b]);
    }
    for i in 1..rings.len() - 1 {
        let (r0, r1) = (ring(i) as u32, ring(i + 1) as u32);
        for k in 0..seg as u32 {
            let k1 = (k + 1) % seg as u32;
            m.indices.extend_from_slice(&[r0 + k, r1 + k, r1 + k1, r0 + k, r1 + k1, r0 + k1]);
        }
    }
    p.add(egui::Shape::mesh(m));
}

/// Points around a rounded rectangle, clockwise from the top left.
fn rounded_outline(r: egui::Rect, rad: f32) -> Vec<egui::Pos2> {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let mut v = Vec::new();
    let corners = [
        (egui::pos2(r.min.x + rad, r.min.y + rad), 180.0f32),
        (egui::pos2(r.max.x - rad, r.min.y + rad), 270.0),
        (egui::pos2(r.max.x - rad, r.max.y - rad), 0.0),
        (egui::pos2(r.min.x + rad, r.max.y - rad), 90.0),
    ];
    for (c, a0) in corners {
        for k in 0..=8 {
            let a = (a0 + k as f32 * 90.0 / 8.0).to_radians();
            v.push(egui::pos2(c.x + rad * a.cos(), c.y + rad * a.sin()));
        }
    }
    v
}

/// A 1.2 px border around a rounded rectangle, coloured along the diagonal.
pub fn gradient_border(p: &egui::Painter, r: egui::Rect, rad: f32, alpha: u8) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let outer = rounded_outline(r, rad);
    let inner = rounded_outline(r.shrink(1.3), (rad - 1.3).max(0.0));
    let mut m = Mesh::default();
    let span = r.width() + r.height();
    for (o, i) in outer.iter().zip(inner.iter()) {
        let t = ((o.x - r.min.x) + (o.y - r.min.y)) / span;
        let c = edge_color(t, alpha);
        m.vertices.push(Vertex { pos: *o, uv: WHITE_UV, color: c });
        m.vertices.push(Vertex { pos: *i, uv: WHITE_UV, color: c });
    }
    let n = outer.len() as u32;
    for k in 0..n {
        let k1 = (k + 1) % n;
        let (a, b, c, d) = (2 * k, 2 * k + 1, 2 * k1, 2 * k1 + 1);
        m.indices.extend_from_slice(&[a, b, c, b, d, c]);
    }
    p.add(egui::Shape::mesh(m));
}

/// Small corner brackets just inside a card, like a heads-up display.
pub fn brackets(p: &egui::Painter, r: egui::Rect, len: f32, col: Color32) {
    let s = Stroke::new(2.0, col);
    let r = r.shrink(8.0);
    for (c, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
        p.line_segment([c, egui::pos2(c.x + dx * len, c.y)], s);
        p.line_segment([c, egui::pos2(c.x, c.y + dy * len)], s);
    }
}

/// The "Do this next" card: dark glass, a glowing gradient edge and corner brackets.
pub fn hero_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let inner = Frame::NONE
        .fill(Color32::from_rgba_premultiplied(8, 11, 18, 228))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::symmetric(28, 24))
        .shadow(egui::epaint::Shadow { offset: [0, 0], blur: 28, spread: 0, color: Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 20) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add).inner
        });
    let r = inner.response.rect;
    let p = ui.painter();
    gradient_border(p, r, 18.0, 200);
    brackets(p, r, 12.0, Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 220));
    inner.inner
}

/// Upper-case label with wide letter spacing, for the cockpit feel.
pub fn caps(s: &str, size: f32, col: Color32) -> RichText {
    RichText::new(s.to_uppercase()).size(size).color(col).extra_letter_spacing(size * 0.16).strong()
}

/// The lime-to-teal main button with a soft glow. Only the Next bar uses it.
pub fn glow_button(ui: &mut Ui, text: &str) -> egui::Response {
    let font = egui::FontId::proportional(15.0);
    let label = text.to_uppercase();
    let mut job = egui::text::LayoutJob::simple_singleline(label.clone(), font.clone(), Color32::from_rgb(0x06, 0x10, 0x0A));
    if let Some(sec) = job.sections.first_mut() {
        sec.format.extra_letter_spacing = 1.2;
    }
    let galley = ui.painter().layout_job(job);
    let size = Vec2::new(galley.size().x + 52.0, 46.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let hot = resp.hovered();
    let glow_shape = egui::epaint::Shadow { offset: [0, 6], blur: if hot { 34 } else { 26 }, spread: 0, color: Color32::from_rgba_unmultiplied(0x8D, 0xF5, 0x9A, if hot { 120 } else { 80 }) }.as_shape(rect, CornerRadius::same(12));
    p.add(glow_shape);
    // Gradient fill: a fan over the rounded outline, coloured left to right.
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let pts = rounded_outline(rect, 12.0);
    let mut m = Mesh::default();
    let left = Color32::from_rgb(0xD6, 0xFF, 0x8F);
    let right = Color32::from_rgb(0x5E, 0xF0, 0xB5);
    let col_at = |x: f32| {
        let c = lerp(left, right, (x - rect.min.x) / rect.width());
        if hot { lerp(c, Color32::WHITE, 0.12) } else { c }
    };
    m.vertices.push(Vertex { pos: rect.center(), uv: WHITE_UV, color: col_at(rect.center().x) });
    for q in &pts {
        m.vertices.push(Vertex { pos: *q, uv: WHITE_UV, color: col_at(q.x) });
    }
    let n = pts.len() as u32;
    for k in 0..n {
        m.indices.extend_from_slice(&[0, 1 + k, 1 + (k + 1) % n]);
    }
    p.add(egui::Shape::mesh(m));
    if resp.has_focus() {
        p.rect_stroke(rect.expand(3.0), CornerRadius::same(14), Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
    }
    p.galley(rect.center() - galley.size() / 2.0, galley, Color32::BLACK);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    resp
}

/// A round instrument dial with tick marks, for a 0–100 score.
pub fn dial(ui: &mut Ui, value: Option<u8>, label: &str, col: Color32) -> egui::Response {
    let size = 84.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(size + 8.0, size + 10.0), egui::Sense::hover());
    let p = ui.painter();
    let c = egui::pos2(rect.center().x, rect.min.y + size / 2.0);
    let r = size / 2.0 - 8.0;
    let (start, sweep) = (135.0f32, 270.0f32);
    let pt = |a: f32, rr: f32| {
        let a = a.to_radians();
        egui::pos2(c.x + rr * a.cos(), c.y + rr * a.sin())
    };
    for i in 0..=10 {
        let a = start + sweep * i as f32 / 10.0;
        p.line_segment([pt(a, r + 5.0), pt(a, r + 1.5)], Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 36)));
    }
    let arc = |a0: f32, a1: f32| -> Vec<egui::Pos2> {
        let n = ((a1 - a0) / 4.0).ceil().max(2.0) as usize;
        (0..=n).map(|k| pt(a0 + (a1 - a0) * k as f32 / n as f32, r)).collect()
    };
    p.add(egui::Shape::line(arc(start, start + sweep), Stroke::new(6.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18))));
    if let Some(v) = value {
        let end = start + sweep * v as f32 / 100.0;
        // A wider faint pass under the arc reads as a glow.
        p.add(egui::Shape::line(arc(start, end), Stroke::new(12.0, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 40))));
        p.add(egui::Shape::line(arc(start, end), Stroke::new(6.0, col)));
    }
    let txt = value.map(|v| v.to_string()).unwrap_or_else(|| "–".into());
    p.text(c, egui::Align2::CENTER_CENTER, txt, egui::FontId::monospace(22.0), TEXT);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_uppercase(), egui::FontId::proportional(10.5), MUTED);
    if let Some(sec) = job.sections.first_mut() {
        sec.format.extra_letter_spacing = 1.6;
    }
    let g = p.layout_job(job);
    p.galley(egui::pos2(rect.center().x - g.size().x / 2.0, rect.max.y - g.size().y), g, MUTED);
    resp.on_hover_text(match value {
        Some(v) => format!("{label} {v} of 100"),
        None => format!("{label}: not measured yet"),
    })
}

/// A small line graph with a faint fill under it.
pub fn sparkline(ui: &mut Ui, values: &[f32], w: f32, h: f32, col: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::hover());
    if values.len() < 2 {
        return;
    }
    let (mn, mx) = values.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let span = (mx - mn).max(1e-3);
    let pts: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| egui::pos2(rect.min.x + i as f32 * w / (values.len() - 1) as f32, rect.max.y - 1.0 - (v - mn) / span * (h - 2.0)))
        .collect();
    let p = ui.painter();
    let fill = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 30);
    for pair in pts.windows(2) {
        let quad = vec![pair[0], pair[1], egui::pos2(pair[1].x, rect.max.y), egui::pos2(pair[0].x, rect.max.y)];
        p.add(egui::Shape::convex_polygon(quad, fill, Stroke::NONE));
    }
    p.add(egui::Shape::line(pts, Stroke::new(1.5, col)));
}

/// One tile of the live readings strip: label, value and its line graph.
pub fn reading_tile(ui: &mut Ui, label: &str, value: &str, series: &[f32], col: Color32, width: f32) {
    Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(width - 30.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(caps(label, 10.5, MUTED));
                    ui.label(RichText::new(value).monospace().size(15.0).color(TEXT));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    sparkline(ui, series, 64.0, 22.0, col);
                });
            });
        });
}

/// "Now" and "Change to" as instrument read-outs; the new value glows.
pub fn readout(ui: &mut Ui, label: &str, value: &str, hot: bool) {
    ui.vertical(|ui| {
        ui.label(caps(label, 11.0, if hot { ACCENT } else { MUTED }));
        let (fill, stroke, shadow) = if hot {
            (Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 34), Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 140)), egui::epaint::Shadow { offset: [0, 0], blur: 22, spread: 0, color: Color32::from_rgba_unmultiplied(0xC6, 0xF3, 0x6B, 70) })
        } else {
            (CARD, Stroke::new(1.0, BORDER), egui::epaint::Shadow::NONE)
        };
        Frame::NONE.fill(fill).stroke(stroke).shadow(shadow).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(18, 9)).show(ui, |ui| {
            ui.label(RichText::new(value.to_uppercase()).monospace().size(20.0).color(if hot { TEXT } else { TEXT_SOFT }));
        });
    });
}
