//! Lemmata's look, ported token for token from `apps/desktop/src/app.css`: Overleaf's layout (dark
//! toolbar and file sidebar, light editor, grey PDF ground) in the identity of `docs/pitch.html`.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Visuals,
};
use std::sync::Arc;

/// The colour tokens of `app.css`, one field per CSS variable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub app: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub ink: Color32,
    pub ink_2: Color32,
    pub muted: Color32,
    pub faint: Color32,

    pub chrome: Color32,
    pub chrome_2: Color32,
    pub chrome_3: Color32,
    pub chrome_border: Color32,
    pub chrome_ink: Color32,
    pub chrome_muted: Color32,
    pub chrome_hover: Color32,
    pub chrome_active: Color32,

    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_ink: Color32,
    pub accent_soft: Color32,
    pub accent_on_dark: Color32,

    pub error: Color32,
    pub error_soft: Color32,
    pub warn: Color32,
    pub warn_soft: Color32,
    pub ok: Color32,
    pub ok_soft: Color32,

    pub code_bg: Color32,
    pub gutter: Color32,
    pub pdf_ground: Color32,
    pub atom: Color32,

    pub shadow: Shadow,
    pub shadow_lg: Shadow,
    pub dark: bool,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// `rgba(r, g, b, a)` from CSS.
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, (a * 255.0 + 0.5) as u8)
}

/// CSS box-shadow `0 y blur spread colour`.
const fn shadow(y: i8, blur: u8, spread: i8, color: Color32) -> Shadow {
    // epaint's spread grows the box; CSS's negative spread shrinks it, which epaint can't, so it is dropped
    // and the blur carries the look.
    let _ = spread;
    Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color,
    }
}

pub const LIGHT: Palette = Palette {
    app: hex(0xe8ebee),
    surface: hex(0xffffff),
    surface_2: hex(0xf6f7f8),
    surface_3: hex(0xeef0f2),
    border: hex(0xdfe3e7),
    border_strong: hex(0xc9cfd5),
    ink: hex(0x18212b),
    ink_2: hex(0x3a4652),
    muted: hex(0x5c6873),
    faint: hex(0x616b76),

    chrome: hex(0x1d2630),
    chrome_2: hex(0x232e39),
    chrome_3: hex(0x2b3743),
    chrome_border: hex(0x33404d),
    chrome_ink: hex(0xdbe3ea),
    chrome_muted: hex(0x8d9cab),
    chrome_hover: rgba(255, 255, 255, 0.06),
    chrome_active: rgba(111, 179, 227, 0.16),

    accent: hex(0x2a71a4),
    accent_hover: hex(0x2d6b9b),
    accent_ink: hex(0xffffff),
    accent_soft: hex(0xe4f0f8),
    accent_on_dark: hex(0x7cbbe6),

    error: hex(0xc4472f),
    error_soft: hex(0xfbece8),
    warn: hex(0x9a6b0a),
    warn_soft: hex(0xfbf3e1),
    ok: hex(0x2e7d55),
    ok_soft: hex(0xe5f3eb),

    code_bg: hex(0xffffff),
    gutter: hex(0xf6f7f8),
    pdf_ground: hex(0xc8cdd2),
    atom: hex(0x9a5b14),

    shadow: shadow(8, 24, -8, rgba(16, 24, 32, 0.25)),
    shadow_lg: shadow(24, 60, -20, rgba(16, 24, 32, 0.45)),
    dark: false,
};

pub const DARK: Palette = Palette {
    app: hex(0x0f1418),
    surface: hex(0x171d23),
    surface_2: hex(0x1b2229),
    surface_3: hex(0x222a32),
    border: hex(0x2a333c),
    border_strong: hex(0x38434e),
    ink: hex(0xe2e8ed),
    ink_2: hex(0xc3ccd4),
    muted: hex(0x93a0ac),
    faint: hex(0x86919b),

    chrome: hex(0x12181e),
    chrome_2: hex(0x161d24),
    chrome_3: hex(0x1e262e),
    chrome_border: hex(0x26303a),
    chrome_ink: LIGHT.chrome_ink,
    chrome_muted: LIGHT.chrome_muted,
    chrome_hover: LIGHT.chrome_hover,
    chrome_active: LIGHT.chrome_active,

    accent: hex(0x4a97cf),
    accent_hover: hex(0x5aa5db),
    accent_ink: hex(0x0b1620),
    accent_soft: hex(0x1b2e3e),
    accent_on_dark: hex(0x7cbbe6),

    error: hex(0xec7f69),
    error_soft: hex(0x3a211c),
    warn: hex(0xe4b55a),
    warn_soft: hex(0x362c16),
    ok: hex(0x67c493),
    ok_soft: hex(0x173126),

    code_bg: hex(0x171d23),
    gutter: hex(0x171d23),
    pdf_ground: hex(0x0b0f13),
    atom: hex(0xe0a85a),

    shadow: shadow(8, 24, -8, rgba(0, 0, 0, 0.6)),
    shadow_lg: shadow(24, 60, -20, rgba(0, 0, 0, 0.75)),
    dark: true,
};

/// `--shadow-sm`, the same in both themes.
pub const SHADOW_SM: Shadow = shadow(1, 2, 0, rgba(16, 24, 32, 0.08));

pub const R_SM: u8 = 4;
pub const R: u8 = 6;
pub const R_LG: u8 = 10;

/// The 0.12 s hover and pop transitions.
pub const EASE_SECS: f32 = 0.12;

impl Palette {
    /// `--indent-guide` follows `--border-strong` in every theme.
    pub fn indent_guide(&self) -> Color32 {
        self.border_strong
    }

    /// `color-mix(in srgb, c pct%, transparent)`.
    pub fn mix(c: Color32, pct: f32) -> Color32 {
        c.gamma_multiply(pct / 100.0)
    }

    /// `::selection`, 28 % accent.
    pub fn selection(&self) -> Color32 {
        Self::mix(self.accent, 28.0)
    }
}

/// Font families. egui has no weights, so each weight the CSS uses is its own family.
pub mod font {
    use super::FontFamily;
    pub fn display() -> FontFamily {
        FontFamily::Name("display".into())
    }
    pub fn sans_500() -> FontFamily {
        FontFamily::Name("sans-500".into())
    }
    pub fn sans_600() -> FontFamily {
        FontFamily::Name("sans-600".into())
    }
    pub fn mono_500() -> FontFamily {
        FontFamily::Name("mono-500".into())
    }
}

macro_rules! fonts {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_bytes!(concat!("../assets/fonts/", $name, ".ttf")) as &'static [u8])),*]
    };
}

/// The latin and latin-ext subsets the Svelte bundle ships. Glyphs outside them fall through to the
/// system fonts eframe's `system_fonts` feature adds.
const FONT_FILES: [(&str, &[u8]); 12] = fonts!(
    "young-serif-latin-400",
    "young-serif-latin-ext-400",
    "ibm-plex-sans-latin-400",
    "ibm-plex-sans-latin-ext-400",
    "ibm-plex-sans-latin-500",
    "ibm-plex-sans-latin-ext-500",
    "ibm-plex-sans-latin-600",
    "ibm-plex-sans-latin-ext-600",
    "ibm-plex-mono-latin-400",
    "ibm-plex-mono-latin-ext-400",
    "ibm-plex-mono-latin-500",
    "ibm-plex-mono-latin-ext-500",
);

pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    for (name, bytes) in FONT_FILES {
        defs.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let set = |defs: &mut FontDefinitions, family: FontFamily, font: &str, weight: u16| {
        defs.families.insert(
            family,
            vec![
                format!("{font}-latin-{weight}"),
                format!("{font}-latin-ext-{weight}"),
            ],
        );
    };
    set(&mut defs, FontFamily::Proportional, "ibm-plex-sans", 400);
    set(&mut defs, font::sans_500(), "ibm-plex-sans", 500);
    set(&mut defs, font::sans_600(), "ibm-plex-sans", 600);
    set(&mut defs, FontFamily::Monospace, "ibm-plex-mono", 400);
    set(&mut defs, font::mono_500(), "ibm-plex-mono", 500);
    set(&mut defs, font::display(), "young-serif", 400);
    ctx.set_fonts(defs);
}

/// The egui style for one theme: body text 13 px, controls as `.btn` and `.input`, menus as `.menu`.
pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut visuals = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = p.app;
    visuals.window_fill = p.surface;
    visuals.extreme_bg_color = p.surface;
    visuals.faint_bg_color = p.surface_2;
    visuals.code_bg_color = p.surface_3;
    visuals.override_text_color = Some(p.ink);
    visuals.hyperlink_color = p.accent;
    visuals.selection.bg_fill = p.selection();
    visuals.selection.stroke = Stroke::new(1.0, p.ink);
    visuals.error_fg_color = p.error;
    visuals.warn_fg_color = p.warn;
    visuals.window_stroke = Stroke::new(1.0, p.border);
    visuals.window_corner_radius = CornerRadius::same(R_LG);
    visuals.menu_corner_radius = CornerRadius::same(R_LG);
    visuals.window_shadow = p.shadow_lg;
    visuals.popup_shadow = p.shadow;

    let w = &mut visuals.widgets;
    for (state, fill, stroke) in [
        (&mut w.inactive, p.surface, p.border_strong),
        (&mut w.hovered, p.surface_2, p.faint),
        (&mut w.active, p.surface_3, p.faint),
        (&mut w.open, p.surface_3, p.border_strong),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.0, p.ink);
        state.corner_radius = CornerRadius::same(R);
        state.expansion = 0.0;
    }
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
    w.noninteractive.corner_radius = CornerRadius::same(R);

    let theme = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, visuals);
    ctx.style_mut_of(theme, |s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::new(13.0, font::sans_500())),
            (TextStyle::Heading, FontId::new(14.0, font::sans_600())),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.button_padding = egui::vec2(12.0, 0.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.menu_margin = Margin::same(5);
        s.spacing.item_spacing = egui::vec2(6.0, 6.0);
        s.animation_time = EASE_SECS;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_app_css() {
        assert_eq!(LIGHT.accent, Color32::from_rgb(0x2a, 0x71, 0xa4));
        assert_eq!(DARK.app, Color32::from_rgb(0x0f, 0x14, 0x18));
        // rgba(255,255,255,.06) rounds to alpha 15.
        assert_eq!(LIGHT.chrome_hover.a(), 15);
        assert_eq!(DARK.chrome_ink, LIGHT.chrome_ink);
    }
}
