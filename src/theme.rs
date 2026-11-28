// SPDX-License-Identifier: GPL-3.0-or-later
//! Colors, fonts sizes and small reusable widgets.

use crate::analysis::Status;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui, Vec2};

// ---------------------------------------------------------------------------
// Colour tokens. Every colour in the app comes from here (house standard v1.3):
// navy neutrals, lime for text and links (#3F6212 in the light theme), status
// colours that mean the same everywhere, and filled buttons with dark ink on
// lime (dark theme) or white on #3F6212 (light theme). The contrast test at
// the bottom checks every text colour on every surface in both themes.

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Window background.
    pub bg: Color32,
    /// Sidebar.
    pub sidebar: Color32,
    /// Cards.
    pub card: Color32,
    /// Insets inside cards, graph backgrounds.
    pub inset: Color32,
    /// Buttons, inputs and other controls.
    pub control: Color32,
    /// Controls under the pointer.
    pub hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_soft: Color32,
    pub muted: Color32,
    /// Lime: text, links, the active item.
    pub accent: Color32,
    /// Behind accent text (selected tab, pills).
    pub accent_bg: Color32,
    /// Filled buttons: fill and the text on it.
    pub fill: Color32,
    pub on_fill: Color32,
    pub success: Color32,
    pub success_bg: Color32,
    pub warn: Color32,
    pub warn_bg: Color32,
    pub bad: Color32,
    pub bad_bg: Color32,
    /// Text on a red (danger) button.
    pub on_bad: Color32,
    pub info: Color32,
    /// The tray's "needs attention" dot.
    pub amber: Color32,
    /// Faint lines: graph ticks, the backdrop grid.
    pub faint: Color32,
    /// Placeholder readings before a test.
    pub placeholder: Color32,
    pub dark: bool,
}

const fn rgb(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

pub const DARK: Palette = Palette {
    bg: rgb(0x0C1016),
    sidebar: rgb(0x090C11),
    card: rgb(0x121822),
    inset: rgb(0x0E131B),
    control: rgb(0x1A212C),
    hover: rgb(0x222B38),
    border: rgb(0x232C3A),
    border_strong: rgb(0x334052),
    text: rgb(0xECEFF4),
    text_soft: rgb(0xC6CEDA),
    muted: rgb(0x9CA8BA),
    accent: rgb(0xC6F36B),
    accent_bg: rgb(0x232D17),
    fill: rgb(0xC6F36B),
    on_fill: rgb(0x0C1016),
    success: rgb(0x6EE7B7),
    success_bg: rgb(0x123024),
    warn: rgb(0xFBBF24),
    warn_bg: rgb(0x33270B),
    bad: rgb(0xF87171),
    bad_bg: rgb(0x3A1618),
    on_bad: rgb(0x0C1016),
    info: rgb(0x7DD3FC),
    amber: rgb(0xFBBF24),
    faint: Color32::from_rgba_premultiplied(9, 11, 5, 9),
    placeholder: rgb(0x4A5566),
    dark: true,
};

pub const LIGHT: Palette = Palette {
    bg: rgb(0xF6F7F8),
    sidebar: rgb(0xEEF0F2),
    card: rgb(0xFFFFFF),
    inset: rgb(0xF4F6F8),
    control: rgb(0xF1F3F5),
    hover: rgb(0xE6E9ED),
    border: rgb(0xE2E5E9),
    border_strong: rgb(0xC9CED6),
    text: rgb(0x111317),
    text_soft: rgb(0x2E343E),
    muted: rgb(0x555C69),
    accent: rgb(0x3F6212),
    accent_bg: rgb(0xEAF3DA),
    fill: rgb(0x3F6212),
    on_fill: rgb(0xFFFFFF),
    success: rgb(0x047857),
    success_bg: rgb(0xDDF3EA),
    warn: rgb(0x975A06),
    warn_bg: rgb(0xFBF0D9),
    bad: rgb(0xB91C1C),
    bad_bg: rgb(0xFBE3E3),
    on_bad: rgb(0xFFFFFF),
    info: rgb(0x0369A1),
    amber: rgb(0xD97706),
    faint: Color32::from_rgba_premultiplied(4, 6, 1, 14),
    placeholder: rgb(0x9AA1AD),
    dark: false,
};

static LIGHT_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The colours in use right now.
pub fn pal() -> &'static Palette {
    if LIGHT_ON.load(std::sync::atomic::Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

/// A colour with a different opacity (0–255).
pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Fonts built into the program (never the computer's): Inter for text,
/// JetBrains Mono for numbers and logs, Space Grotesk for titles, Lucide icons.
/// egui's own fonts stay behind them for anything they don't cover.
pub const INTER: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
pub const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
pub const SPACE_GROTESK: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-SemiBold.ttf");
pub const LUCIDE: &[u8] = include_bytes!("../assets/fonts/lucide.ttf");

/// The font family for titles.
pub fn title_family() -> egui::FontFamily {
    egui::FontFamily::Name("title".into())
}

/// The font family for Lucide icons.
pub fn icon_family() -> egui::FontFamily {
    egui::FontFamily::Name("icons".into())
}

pub fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    use std::sync::Arc;
    let mut f = egui::FontDefinitions::default();
    for (name, bytes) in [
        ("inter", INTER),
        ("jetbrains-mono", JETBRAINS_MONO),
        ("space-grotesk", SPACE_GROTESK),
        ("lucide", LUCIDE),
    ] {
        f.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let proportional = f
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(v) = f.families.get_mut(&FontFamily::Proportional) {
        v.insert(0, "inter".into());
    }
    if let Some(v) = f.families.get_mut(&FontFamily::Monospace) {
        v.insert(0, "jetbrains-mono".into());
    }
    let mut title = vec!["space-grotesk".to_string(), "inter".to_string()];
    title.extend(proportional.iter().cloned());
    f.families.insert(title_family(), title);
    let mut icons = vec!["lucide".to_string(), "inter".to_string()];
    icons.extend(proportional);
    f.families.insert(icon_family(), icons);
    f
}

/// A Lucide icon as text, e.g. `ui.label(icon(icons::SETTINGS, 18.0))`.
pub fn icon(code: &str, size: f32) -> RichText {
    RichText::new(code).family(icon_family()).size(size)
}

/// egui's widget colours from a palette.
fn visuals(c: &Palette) -> egui::Visuals {
    let mut v = if c.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = Color32::TRANSPARENT;
    // Popups and menus stay solid so text over them is easy to read.
    v.window_fill = c.card;
    v.window_stroke = Stroke::new(1.0, c.border_strong);
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(12);
    v.extreme_bg_color = c.inset;
    v.faint_bg_color = c.card;
    v.override_text_color = Some(c.text);
    v.hyperlink_color = c.accent;
    v.selection.bg_fill = c.accent_bg;
    v.selection.stroke = Stroke::new(1.0, c.accent);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c.text);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(8);
    }
    v.widgets.inactive.bg_fill = c.control;
    v.widgets.inactive.weak_bg_fill = c.control;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, c.border_strong);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, c.text);
    v.widgets.hovered.bg_fill = c.hover;
    v.widgets.hovered.weak_bg_fill = c.hover;
    // A clear outline on whatever has keyboard focus or the pointer.
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, c.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, c.text);
    v.widgets.active.bg_fill = c.hover;
    v.widgets.active.weak_bg_fill = c.hover;
    v.widgets.active.fg_stroke = Stroke::new(1.5, c.text);
    v.widgets.open.bg_fill = c.hover;
    v.widgets.open.weak_bg_fill = c.hover;
    v
}

/// Dark, light, or follow the desktop (the default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.all_styles_mut(|s| {
        // Spacing 4/8/12/16/24/32, as in the other apps.
        s.spacing.item_spacing = Vec2::new(12.0, 8.0);
        s.spacing.button_padding = Vec2::new(16.0, 8.0);
        s.spacing.interact_size.y = 32.0;
        s.spacing.slider_width = 280.0;
        s.spacing.window_margin = Margin::same(24);
        s.spacing.menu_margin = Margin::same(8);
        // 120 ms (fast) animations; "Reduce motion" turns them off.
        s.animation_time = 0.12;
        use egui::{FontFamily, FontId, TextStyle};
        // Sizes 12 small / 14 body / 16 large / 22 title.
        s.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(16.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
        ]
        .into();
    });
}

/// Applies the user's choices; call when they change.
pub fn set_choice(ctx: &egui::Context, choice: ThemeChoice, reduce_motion: bool) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
        ThemeChoice::Light => egui::ThemePreference::Light,
    });
    ctx.all_styles_mut(|s| s.animation_time = if reduce_motion { 0.0 } else { 0.12 });
}

/// Picks the palette for this frame from egui's resolved theme (it follows the
/// desktop when the choice is System). Call at the start of every frame.
pub fn sync(ctx: &egui::Context) {
    let light = ctx.theme() == egui::Theme::Light;
    LIGHT_ON.store(light, std::sync::atomic::Ordering::Relaxed);
}

/// Background grid on or off (Settings › Appearance).
pub static GRID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// A card: 1 px lime edge at about 18%, corner radius 12.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(pal().card)
        .stroke(Stroke::new(1.0, alpha(pal().accent, 46)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add)
                .inner
        })
        .inner
}

pub fn inset<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(pal().inset)
        .stroke(Stroke::new(1.0, pal().border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add)
                .inner
        })
        .inner
}

pub fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(22.0).family(title_family()));
}

pub fn subtitle(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(14.0).color(pal().muted));
}

pub fn muted(ui: &mut Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).color(pal().muted));
}

pub fn small(ui: &mut Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).size(12.0).color(pal().muted));
}

/// A filled button: dark ink on lime (dark theme), white on #3F6212 (light).
pub fn primary(ui: &mut Ui, s: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(s).color(pal().on_fill).strong())
            .fill(pal().fill)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

pub fn secondary(ui: &mut Ui, s: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(s).color(pal().text).strong())
            .fill(pal().control)
            .stroke(Stroke::new(1.0, pal().border_strong))
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

/// A red button, for Uninstall and other things that can't be undone.
pub fn danger(ui: &mut Ui, s: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(s).color(pal().on_bad).strong())
            .fill(pal().bad)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

/// A card with a red edge (the Uninstall box).
pub fn danger_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(pal().card)
        .stroke(Stroke::new(1.0, pal().bad))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add)
                .inner
        })
        .inner
}

pub fn status_colors(s: Status) -> (Color32, Color32, &'static str) {
    match s {
        Status::Ok => (pal().success, pal().success_bg, "OK"),
        Status::Warn => (pal().warn, pal().warn_bg, "CHECK"),
        Status::Bad => (pal().bad, pal().bad_bg, "FIX"),
        Status::Unknown => (pal().muted, pal().control, "UNKNOWN"),
    }
}

pub fn pill(ui: &mut Ui, fg: Color32, bg: Color32, text: &str) {
    Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(fg).strong());
        });
}

pub fn status_pill(ui: &mut Ui, s: Status, label: &str) {
    let (fg, bg, _) = status_colors(s);
    pill(ui, fg, bg, label);
}

pub fn measured_tag(ui: &mut Ui, measured: bool) {
    if measured {
        ui.label(
            RichText::new("Measured on this PC")
                .size(12.0)
                .color(pal().success),
        );
    } else {
        ui.label(RichText::new("Estimate").size(12.0).color(pal().muted));
    }
}

/// Tag for things read exactly from a settings file or OBS.
pub fn read_tag(ui: &mut Ui, from: &str) {
    ui.label(
        RichText::new(format!("Read from {from}"))
            .size(12.0)
            .color(pal().info),
    );
}

pub fn score_color(score: u8) -> Color32 {
    if score >= 90 {
        pal().success
    } else if score >= 70 {
        pal().warn
    } else {
        pal().bad
    }
}

/// A thin progress bar.
pub fn bar(ui: &mut Ui, frac: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(99), pal().border);
    let mut r = rect;
    r.set_width((w * frac.clamp(0.0, 1.0)).max(height));
    p.rect_filled(r, CornerRadius::same(99), color);
}

/// Simple bar graph of values 0..=max.
pub fn spark_bars(ui: &mut Ui, values: &[f32], max: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), pal().inset);
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
        let r = egui::Rect::from_min_max(
            egui::pos2(x, rect.bottom() - h - 2.0),
            egui::pos2(x + bw, rect.bottom() - 2.0),
        );
        p.rect_filled(r, CornerRadius::same(2), color);
    }
}

// ---------------------------------------------------------------------------
// Cockpit glass: backdrop, glowing card edge, dials, line graphs, readings.

/// The window background: deep navy, a green glow top right, a violet glow
/// bottom left, and a faint lime grid. Painted once behind everything.
/// Set while Preflight's window is hidden in the tray.
pub static HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Asks for a redraw in `ms`, but at most once a second while another window
/// (the game) is in front, and not at all while minimized or in the tray.
/// New readings and OBS events still wake the window when they arrive.
pub fn repaint_in(ctx: &egui::Context, ms: u64) {
    let (focused, minimized) = ctx.input(|i| {
        (
            i.viewport().focused.unwrap_or(true),
            i.viewport().minimized.unwrap_or(false),
        )
    });
    if minimized || HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let ms = if focused { ms } else { ms.max(1000) };
    ctx.request_repaint_after(std::time::Duration::from_millis(ms));
}

pub fn paint_backdrop(ctx: &egui::Context) {
    // Built once per window size, theme and grid setting, then reused every frame.
    let rect = ctx.content_rect();
    let grid = GRID.load(std::sync::atomic::Ordering::Relaxed);
    let dark = pal().dark;
    let key = egui::Id::unique("preflight_backdrop");
    type Cached = (egui::Rect, bool, bool, std::sync::Arc<egui::epaint::Mesh>);
    let cached: Option<Cached> = ctx.data(|d| d.get_temp(key));
    let mesh = match cached {
        Some((r, g, d, m)) if r == rect && g == grid && d == dark => m,
        _ => {
            let m = std::sync::Arc::new(backdrop_mesh(rect, grid));
            ctx.data_mut(|d| d.insert_temp(key, (rect, grid, dark, m.clone())));
            m
        }
    };
    ctx.layer_painter(egui::LayerId::background())
        .add(egui::Shape::Mesh(mesh));
}

/// The window background: the hull colour, a faint lime glow top right (dark
/// theme only), and an optional fine grid.
fn backdrop_mesh(rect: egui::Rect, grid: bool) -> egui::epaint::Mesh {
    use egui::epaint::Mesh;
    let c = pal();
    let mut m = Mesh::default();
    m.add_colored_rect(rect, c.bg);
    if c.dark {
        glow(
            &mut m,
            egui::pos2(rect.max.x - rect.width() * 0.12, rect.min.y),
            rect.width() * 0.5,
            alpha(c.accent, 18),
        );
    }
    if !grid {
        return m;
    }
    let line = c.faint;
    let step = 28.0;
    let mut x = rect.min.x;
    while x < rect.max.x {
        m.add_colored_rect(
            egui::Rect::from_min_max(
                egui::pos2(x - 0.5, rect.min.y),
                egui::pos2(x + 0.5, rect.max.y),
            ),
            line,
        );
        x += step;
    }
    let mut y = rect.min.y;
    while y < rect.max.y {
        m.add_colored_rect(
            egui::Rect::from_min_max(
                egui::pos2(rect.min.x, y - 0.5),
                egui::pos2(rect.max.x, y + 0.5),
            ),
            line,
        );
        y += step;
    }
    m
}

/// A soft round glow: a fan whose centre is coloured and edge transparent.
fn glow(m: &mut egui::epaint::Mesh, c: egui::Pos2, r: f32, col: Color32) {
    use egui::epaint::{Vertex, WHITE_UV};
    let base = m.vertices.len() as u32;
    let rings = [(0.0, 1.0), (0.35, 0.7), (0.7, 0.3), (1.0, 0.0)];
    let seg = 48;
    for (ri, (f, a)) in rings.iter().enumerate() {
        let colr = alpha(col, (col.a() as f32 * a) as u8);
        if ri == 0 {
            m.vertices.push(Vertex {
                pos: c,
                uv: WHITE_UV,
                color: colr,
            });
            continue;
        }
        for k in 0..seg {
            let ang = k as f32 / seg as f32 * std::f32::consts::TAU;
            m.vertices.push(Vertex {
                pos: egui::pos2(c.x + r * f * ang.cos(), c.y + r * f * ang.sin()),
                uv: WHITE_UV,
                color: colr,
            });
        }
    }
    let ring = |i: usize| 1 + (i - 1) * seg as usize;
    for k in 0..seg as u32 {
        let a = 1 + k;
        let b = 1 + (k + 1) % seg as u32;
        m.indices.extend_from_slice(&[base, base + a, base + b]);
    }
    for i in 1..rings.len() - 1 {
        let (r0, r1) = (base + ring(i) as u32, base + ring(i + 1) as u32);
        for k in 0..seg as u32 {
            let k1 = (k + 1) % seg as u32;
            m.indices
                .extend_from_slice(&[r0 + k, r1 + k, r1 + k1, r0 + k, r1 + k1, r0 + k1]);
        }
    }
}

/// Small corner brackets just inside a card, like a heads-up display.
pub fn brackets(p: &egui::Painter, r: egui::Rect, len: f32, col: Color32) {
    let s = Stroke::new(2.0, col);
    let r = r.shrink(8.0);
    for (c, dx, dy) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.left_bottom(), 1.0, -1.0),
        (r.right_bottom(), -1.0, -1.0),
    ] {
        p.line_segment([c, egui::pos2(c.x + dx * len, c.y)], s);
        p.line_segment([c, egui::pos2(c.x, c.y + dy * len)], s);
    }
}

/// The "Do this next" card: corner radius 16 and a 1 px lime edge. Only the
/// active card (where the next step is) gets the brighter edge and the corner
/// brackets.
pub fn hero_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    hero_card_active(ui, true, add)
}

pub fn hero_card_active<R>(ui: &mut Ui, active: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let c = pal();
    let inner = Frame::NONE
        .fill(c.card)
        .stroke(Stroke::new(
            1.0,
            alpha(c.accent, if active { 120 } else { 46 }),
        ))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(24, 24))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add)
                .inner
        });
    if active {
        brackets(
            ui.painter(),
            inner.response.rect,
            12.0,
            alpha(c.accent, 220),
        );
    }
    inner.inner
}

/// Upper-case label with wide letter spacing, for the cockpit feel.
pub fn caps(s: &str, size: f32, col: Color32) -> RichText {
    RichText::new(s.to_uppercase())
        .size(size)
        .color(col)
        .extra_letter_spacing(size * 0.16)
        .strong()
}

/// The main button of a screen, the only thing that glows: a flat fill (lime
/// with dark ink, or #3F6212 with white in the light theme) and a soft glow.
pub fn glow_button(ui: &mut Ui, text: &str) -> egui::Response {
    let c = pal();
    let font = egui::FontId::proportional(14.0);
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_uppercase(), font, c.on_fill);
    if let Some(sec) = job.sections.first_mut() {
        sec.format.extra_letter_spacing = 1.2;
    }
    let galley = ui.painter().layout_job(job);
    let size = Vec2::new(galley.size().x + 48.0, 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let hot = resp.hovered();
    p.add(
        egui::epaint::Shadow {
            offset: [0, 4],
            blur: if hot { 28 } else { 20 },
            spread: 0,
            color: alpha(
                c.fill,
                if c.dark {
                    if hot { 110 } else { 70 }
                } else {
                    50
                },
            ),
        }
        .as_shape(rect, CornerRadius::same(8)),
    );
    let fill = if hot {
        if c.dark {
            Color32::from_rgb(
                c.fill.r().saturating_add(14),
                c.fill.g().saturating_add(8),
                c.fill.b().saturating_add(20),
            )
        } else {
            Color32::from_rgb(
                c.fill.r().saturating_sub(10),
                c.fill.g().saturating_sub(14),
                c.fill.b().saturating_sub(4),
            )
        }
    } else {
        c.fill
    };
    p.rect_filled(rect, CornerRadius::same(8), fill);
    if resp.has_focus() {
        p.rect_stroke(
            rect.expand(3.0),
            CornerRadius::same(10),
            Stroke::new(1.5, c.accent),
            egui::StrokeKind::Outside,
        );
    }
    p.galley(rect.center() - galley.size() / 2.0, galley, c.on_fill);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    resp
}

/// A round instrument dial with tick marks, for a 0–100 score.
pub fn dial(ui: &mut Ui, value: Option<u8>, label: &str, col: Color32) -> egui::Response {
    let size = 84.0;
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(size + 8.0, size + 10.0), egui::Sense::hover());
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
        p.line_segment(
            [pt(a, r + 5.0), pt(a, r + 1.5)],
            Stroke::new(1.5, alpha(pal().text, 36)),
        );
    }
    let arc = |a0: f32, a1: f32| -> Vec<egui::Pos2> {
        let n = ((a1 - a0) / 4.0).ceil().max(2.0) as usize;
        (0..=n)
            .map(|k| pt(a0 + (a1 - a0) * k as f32 / n as f32, r))
            .collect()
    };
    p.add(egui::Shape::line(
        arc(start, start + sweep),
        Stroke::new(6.0, alpha(pal().text, 18)),
    ));
    if let Some(v) = value {
        let end = start + sweep * v as f32 / 100.0;
        p.add(egui::Shape::line(arc(start, end), Stroke::new(6.0, col)));
    }
    match value {
        Some(v) => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                v.to_string(),
                egui::FontId::monospace(22.0),
                pal().text,
            );
        }
        None => {
            p.text(
                egui::pos2(c.x, c.y - 6.0),
                egui::Align2::CENTER_CENTER,
                "NOT",
                egui::FontId::proportional(10.5),
                pal().muted,
            );
            p.text(
                egui::pos2(c.x, c.y + 6.0),
                egui::Align2::CENTER_CENTER,
                "TESTED",
                egui::FontId::proportional(10.5),
                pal().muted,
            );
        }
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_uppercase(),
        egui::FontId::proportional(12.0),
        pal().muted,
    );
    if let Some(sec) = job.sections.first_mut() {
        sec.format.extra_letter_spacing = 1.6;
    }
    let g = p.layout_job(job);
    p.galley(
        egui::pos2(rect.center().x - g.size().x / 2.0, rect.max.y - g.size().y),
        g,
        pal().muted,
    );
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
    let (mn, mx) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let span = (mx - mn).max(1e-3);
    let pts: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                rect.min.x + i as f32 * w / (values.len() - 1) as f32,
                rect.max.y - 1.0 - (v - mn) / span * (h - 2.0),
            )
        })
        .collect();
    let p = ui.painter();
    let fill = alpha(col, 30);
    for pair in pts.windows(2) {
        let quad = vec![
            pair[0],
            pair[1],
            egui::pos2(pair[1].x, rect.max.y),
            egui::pos2(pair[0].x, rect.max.y),
        ];
        p.add(egui::Shape::convex_polygon(quad, fill, Stroke::NONE));
    }
    p.add(egui::Shape::line(pts, Stroke::new(1.5, col)));
}

/// One tile of the live readings strip: label, value and its line graph.
pub fn reading_tile(
    ui: &mut Ui,
    label: &str,
    value: &str,
    series: &[f32],
    col: Color32,
    width: f32,
) {
    Frame::NONE
        .fill(pal().card)
        .stroke(Stroke::new(1.0, pal().border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(width - 30.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(caps(label, 12.0, pal().muted));
                    ui.label(
                        RichText::new(value)
                            .monospace()
                            .size(15.0)
                            .color(pal().text),
                    );
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
        ui.label(caps(
            label,
            12.0,
            if hot { pal().accent } else { pal().muted },
        ));
        let (fill, stroke, shadow) = if hot {
            (
                alpha(pal().accent, 34),
                Stroke::new(1.0, alpha(pal().accent, 140)),
                egui::epaint::Shadow::NONE,
            )
        } else {
            (
                pal().card,
                Stroke::new(1.0, pal().border),
                egui::epaint::Shadow::NONE,
            )
        };
        Frame::NONE
            .fill(fill)
            .stroke(stroke)
            .shadow(shadow)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(18, 9))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(value.to_uppercase())
                        .monospace()
                        .size(16.0)
                        .color(if hot { pal().text } else { pal().text_soft }),
                );
            });
    });
}

/// Contrast ratio between two opaque colours (WCAG 2).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    fn lum(c: Color32) -> f32 {
        let ch = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }
    let (x, y) = (lum(a), lum(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// Every text colour on every surface in both themes, plus filled buttons,
/// badges and status pills, below 4.5:1. Empty when all pass (checked by the
/// tests and by --self-test).
pub fn contrast_problems() -> Vec<String> {
    let mut fails = Vec::new();
    for (theme, c) in [("dark", DARK), ("light", LIGHT)] {
        let surfaces = [
            ("background", c.bg),
            ("sidebar", c.sidebar),
            ("card", c.card),
            ("control", c.control),
            ("hover", c.hover),
        ];
        let texts = [
            ("text", c.text),
            ("soft text", c.text_soft),
            ("muted text", c.muted),
            ("accent", c.accent),
            ("success", c.success),
            ("warning", c.warn),
            ("error", c.bad),
            ("info", c.info),
        ];
        for (sn, s) in surfaces {
            for (tn, t) in texts {
                let r = contrast(t, s);
                if r < 4.5 {
                    fails.push(format!("{theme}: {tn} on {sn} is {r:.2}:1"));
                }
            }
        }
        for (what, fg, bg) in [
            ("filled button", c.on_fill, c.fill),
            ("red button", c.on_bad, c.bad),
            ("amber badge", DARK.on_fill, c.amber),
            ("accent on its tint", c.accent, c.accent_bg),
            ("success pill", c.success, c.success_bg),
            ("warning pill", c.warn, c.warn_bg),
            ("error pill", c.bad, c.bad_bg),
            ("muted on inset", c.muted, c.inset),
        ] {
            let r = contrast(fg, bg);
            if r < 4.5 {
                fails.push(format!("{theme}: {what} is {r:.2}:1"));
            }
        }
    }
    fails
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    /// Every text colour on every surface, in both themes, at least 4.5:1.
    #[test]
    fn every_text_colour_passes_on_every_surface() {
        let fails = contrast_problems();
        assert!(fails.is_empty(), "Below 4.5:1:\n{}", fails.join("\n"));
    }

    #[test]
    fn contrast_maths() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
    }

    /// The colours the house standard fixes for Stream Preflight.
    #[test]
    fn house_standard_colours() {
        assert_eq!(DARK.accent, rgb(0xC6F36B));
        assert_eq!(LIGHT.accent, rgb(0x3F6212));
        assert_eq!(DARK.fill, rgb(0xC6F36B));
        assert_eq!(DARK.on_fill, rgb(0x0C1016));
        assert_eq!(LIGHT.fill, rgb(0x3F6212));
        assert_eq!(LIGHT.on_fill, rgb(0xFFFFFF));
        assert_eq!(
            (DARK.success, LIGHT.success),
            (rgb(0x6EE7B7), rgb(0x047857))
        );
        assert_eq!((DARK.warn, LIGHT.warn), (rgb(0xFBBF24), rgb(0x975A06)));
        assert_eq!((DARK.bad, LIGHT.bad), (rgb(0xF87171), rgb(0xB91C1C)));
    }
}
