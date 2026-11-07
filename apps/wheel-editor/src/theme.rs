//! Editor colors: one accent, high contrast, small corner radii (no pills).

use eframe::egui::{
    Color32, Context, CornerRadius, Stroke, TextStyle, Theme, ThemePreference, Visuals, vec2,
};
use wheel_core::ThemeChoice;

/// The single accent color of the editor.
pub fn accent(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(92, 172, 255)
    } else {
        Color32::from_rgb(0, 92, 175)
    }
}

/// Text color on top of the accent.
pub fn on_accent(dark: bool) -> Color32 {
    if dark { Color32::BLACK } else { Color32::WHITE }
}

fn visuals(dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let (text, panel, window, field, inactive, hovered) = if dark {
        (
            Color32::from_rgb(245, 246, 248),
            Color32::from_rgb(24, 26, 30),
            Color32::from_rgb(32, 35, 40),
            Color32::from_rgb(14, 15, 18),
            Color32::from_rgb(52, 56, 64),
            Color32::from_rgb(70, 76, 86),
        )
    } else {
        (
            Color32::from_rgb(10, 12, 16),
            Color32::from_rgb(246, 247, 249),
            Color32::WHITE,
            Color32::WHITE,
            Color32::from_rgb(226, 229, 234),
            Color32::from_rgb(210, 214, 221),
        )
    };
    let accent = accent(dark);
    v.panel_fill = panel;
    v.window_fill = window;
    v.extreme_bg_color = field;
    v.faint_bg_color = inactive;
    v.hyperlink_color = accent;
    v.selection.bg_fill = accent;
    v.selection.stroke = Stroke::new(1.0, on_accent(dark));
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.fg_stroke.color = text;
        state.corner_radius = CornerRadius::same(3);
    }
    w.noninteractive.bg_fill = window;
    w.inactive.bg_fill = inactive;
    w.inactive.weak_bg_fill = inactive;
    w.hovered.bg_fill = hovered;
    w.hovered.weak_bg_fill = hovered;
    w.hovered.bg_stroke = Stroke::new(1.0, accent);
    w.active.bg_stroke = Stroke::new(1.5, accent);
    v
}

/// Installs the visuals for both themes and selects the one from the options.
pub fn install(ctx: &Context, choice: ThemeChoice) {
    ctx.set_visuals_of(Theme::Dark, visuals(true));
    ctx.set_visuals_of(Theme::Light, visuals(false));
    ctx.global_style_mut(|style| {
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.item_spacing = vec2(8.0, 6.0);
        for (text_style, size) in [
            (TextStyle::Body, 14.5),
            (TextStyle::Button, 14.5),
            (TextStyle::Small, 12.5),
            (TextStyle::Heading, 20.0),
        ] {
            if let Some(font) = style.text_styles.get_mut(&text_style) {
                font.size = size;
            }
        }
    });
    apply(ctx, choice);
}

pub fn apply(ctx: &Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => ThemePreference::System,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::Dark => ThemePreference::Dark,
    });
}
