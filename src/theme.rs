//! Shared visual language for the picker, charts, and standard egui controls.
use eframe::egui::{self, Color32, FontId, RichText, Stroke, vec2};

pub const CANVAS: Color32 = Color32::WHITE;
pub const SURFACE: Color32 = Color32::from_rgb(248, 246, 252);
pub const ACCENT: Color32 = Color32::from_rgb(153, 112, 222);
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(130, 94, 192);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(239, 231, 252);
pub const HOVER: Color32 = Color32::from_rgb(242, 235, 251);
pub const INK: Color32 = Color32::from_rgb(42, 38, 52);
pub const MUTED: Color32 = Color32::from_rgb(119, 113, 129);
pub const BORDER: Color32 = Color32::from_rgb(231, 226, 240);
pub const WARNING: Color32 = Color32::from_rgb(157, 91, 39);
pub const CORAL: Color32 = Color32::from_rgb(206, 134, 98);
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const CARD_RADIUS: f32 = 12.0;
pub const WINDOW_RADIUS: f32 = 18.0;

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::light();
    visuals.panel_fill = CANVAS;
    visuals.window_fill = CANVAS;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.hyperlink_color = ACCENT_STRONG;
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_STRONG);
    visuals.override_text_color = None;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.5, INK);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = ACCENT_SOFT;
    visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_STRONG);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, ACCENT_STRONG);
    visuals.widgets.open = visuals.widgets.active;
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(12.0, 7.0);
    // Tree rows stay compact; action buttons have an explicit shared height.
    style.spacing.interact_size = vec2(32.0, 24.0);
    style.spacing.combo_width = 170.0;
    style.animation_time = 0.18;
    for (kind, size) in [
        (egui::TextStyle::Small, 11.0),
        (egui::TextStyle::Body, 14.0),
        (egui::TextStyle::Button, 13.0),
        (egui::TextStyle::Heading, 18.0),
    ] {
        style.text_styles.insert(kind, FontId::proportional(size));
    }
    ctx.set_style_of(egui::Theme::Light, style);
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(16.0)
        .corner_radius(CARD_RADIUS)
}

pub fn button(text: &str) -> egui::Button<'_> {
    egui::Button::new(text).min_size(vec2(0.0, CONTROL_HEIGHT))
}

pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(CANVAS))
        .min_size(vec2(0.0, CONTROL_HEIGHT))
        .fill(ACCENT_STRONG)
        .stroke(Stroke::NONE)
}

pub fn segment(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(if selected { ACCENT_STRONG } else { MUTED }))
            .min_size(vec2(0.0, CONTROL_HEIGHT))
            .fill(if selected { ACCENT_SOFT } else { CANVAS })
            .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER })),
    )
}

pub fn section(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(18.0).strong().color(INK));
    if !description.is_empty() {
        ui.label(RichText::new(description).size(12.0).color(MUTED));
    }
    ui.add_space(4.0);
}
