//! The controls of `app.css`: `.btn` and its variants, `.segmented`, `.badge`, `kbd`, `.label`.

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Default,
    Primary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Sm,
    #[default]
    Md,
    Lg,
}

pub struct Button<'a> {
    text: &'a str,
    icon: Option<Icon>,
    kind: Kind,
    size: Size,
    on: bool,
    /// `.on()` was called: announced as a toggle. An ordinary button is not.
    toggle: bool,
    enabled: bool,
    trailing: Option<Icon>,
    ink: Option<Color32>,
    fill: Option<Color32>,
    min_width: f32,
    angle: f32,
    radius: Option<CornerRadius>,
    icon_size: f32,
}

impl<'a> Button<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            icon: None,
            kind: Kind::Default,
            size: Size::Md,
            on: false,
            toggle: false,
            enabled: true,
            trailing: None,
            ink: None,
            fill: None,
            min_width: 0.0,
            angle: 0.0,
            radius: None,
            icon_size: 16.0,
        }
    }
    /// An icon-only button (`.btn.icon`, drawn with `show_icon`); `label` is its accessible name.
    pub fn icon(icon: Icon, label: &'a str) -> Self {
        Self {
            icon: Some(icon),
            kind: Kind::Ghost,
            ..Self::new(label)
        }
    }
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    /// An icon after the text, as "Continue →".
    pub fn trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }
    /// A fill (and border) other than the kind's, as `.danger-btn`.
    pub fn fill(mut self, color: Color32) -> Self {
        self.fill = Some(color);
        self
    }
    /// At least this wide, content centred (`width: 100%` buttons).
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }
    /// Other corners than `--r` (the two halves of a split button).
    pub fn radius(mut self, r: CornerRadius) -> Self {
        self.radius = Some(r);
        self
    }
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }
    /// Turns the leading icon, for a spinner.
    pub fn icon_angle(mut self, angle: f32) -> Self {
        self.angle = angle;
        self
    }
    /// A text colour other than the kind's (`.skip { color: var(--muted) }`).
    pub fn ink(mut self, color: Color32) -> Self {
        self.ink = Some(color);
        self
    }
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    /// A toggled button.
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self.toggle = true;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui, p: &Palette) -> Response {
        self.show_inner(ui, p, false)
    }

    /// Icon-only: square, text used only as the accessible name.
    pub fn show_icon(self, ui: &mut Ui, p: &Palette) -> Response {
        self.show_inner(ui, p, true)
    }

    fn show_inner(self, ui: &mut Ui, p: &Palette, icon_only: bool) -> Response {
        let (height, pad, font_size) = match self.size {
            Size::Sm => (26.0, 9.0, 12.0),
            Size::Md => (30.0, 12.0, 13.0),
            Size::Lg => (38.0, 18.0, 14.0),
        };
        let icon_size = self.icon_size;
        let font = FontId::new(font_size, theme::font::sans_500());
        let galley = (!icon_only).then(|| {
            ui.painter()
                .layout_no_wrap(self.text.to_owned(), font, Color32::PLACEHOLDER)
        });
        let width = if icon_only {
            height
        } else {
            let text_w = galley.as_ref().map_or(0.0, |g| g.size().x);
            let icons = self.icon.iter().chain(&self.trailing).count() as f32;
            pad * 2.0 + icons * (icon_size + 6.0) + text_w
                - if icons > 0.0 && text_w == 0.0 {
                    6.0
                } else {
                    0.0
                }
        };
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let content_w = width;
        let width = width.max(self.min_width);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
        response.widget_info(|| {
            if self.toggle {
                WidgetInfo::selected(egui::Role::Button, self.enabled, self.on, self.text)
            } else {
                WidgetInfo::labeled(egui::Role::Button, self.enabled, self.text)
            }
        });

        if ui.is_rect_visible(rect) {
            let hovered = self.enabled && response.hovered();
            let t = ui
                .ctx()
                .animate_bool_with_time(response.id, hovered, theme::ease_secs());
            let lerp = |a: Color32, b: Color32| a.lerp_to_gamma(b, t);
            let (fill, stroke, ink) = match self.kind {
                Kind::Default if self.on => (p.accent_soft, p.accent, p.accent),
                Kind::Default => (
                    lerp(p.surface, p.surface_2),
                    lerp(p.border_strong, p.faint),
                    p.ink,
                ),
                Kind::Primary => (
                    lerp(p.accent, p.accent_hover),
                    lerp(p.accent, p.accent_hover),
                    p.accent_ink,
                ),
                // `.btn.on`: a pressed toggle shows in the accent colour.
                // Labeled ghost buttons get a 1 px border inset (`.btn.ghost`); icon-only ghost
                // buttons stay borderless until hover (`.tb`, `.side-head .act`).
                Kind::Ghost => (
                    lerp(Color32::TRANSPARENT, p.surface_3),
                    if icon_only {
                        Color32::TRANSPARENT
                    } else {
                        p.border
                    },
                    if self.on { p.accent } else { p.ink },
                ),
                Kind::Danger => (
                    lerp(p.surface, p.surface_2),
                    lerp(p.border_strong, p.faint),
                    p.error,
                ),
            };
            let ink = self.ink.unwrap_or(ink);
            let (fill, stroke) = match self.fill {
                Some(f) => (f, f),
                None => (fill, stroke),
            };
            let alpha = if self.enabled { 1.0 } else { 0.5 };
            let (fill, stroke, ink) = (
                fill.gamma_multiply(alpha),
                stroke.gamma_multiply(alpha),
                ink.gamma_multiply(alpha),
            );
            let default_r = match self.size {
                Size::Sm => theme::R,
                Size::Md | Size::Lg => theme::R_MD,
            };
            let painter = ui.painter();
            painter.rect(
                rect,
                self.radius.unwrap_or(CornerRadius::same(default_r)),
                fill,
                Stroke::new(1.0, stroke),
                StrokeKind::Inside,
            );
            focus_ring(ui, &response, rect, p, default_r);

            let mut x = rect.left()
                + if icon_only {
                    (width - icon_size) / 2.0
                } else {
                    pad + (width - content_w) / 2.0
                };
            if let Some(icon) = self.icon {
                let r = egui::Rect::from_min_size(
                    egui::pos2(x, rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                );
                icons::paint_rotated(painter, r, icon, ink, self.angle);
                x += icon_size + 6.0;
            }
            if let Some(galley) = galley {
                let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
                x += galley.size().x + 6.0;
                painter.galley(pos, galley, ink);
            }
            if let Some(icon) = self.trailing {
                let r = egui::Rect::from_min_size(
                    egui::pos2(x, rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                );
                icons::paint(painter, r, icon, ink);
            }
        }
        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// `--focus`: a 2 px gap in the surface colour, then 2 px of accent, on keyboard focus only.
pub fn focus_ring(ui: &Ui, response: &Response, rect: egui::Rect, p: &Palette, radius: u8) {
    if response.has_focus()
        && ui.memory(|m| m.focused().is_some())
        && !response.is_pointer_button_down_on()
    {
        let painter = ui.painter();
        painter.rect_stroke(
            rect.expand(1.0),
            CornerRadius::same(radius + 1),
            Stroke::new(2.0, p.surface),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            rect.expand(3.0),
            CornerRadius::same(radius + 3),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    }
}

/// How wide `segmented` draws these options.
pub fn segmented_width(ui: &Ui, options: &[(Option<Icon>, &str)], icon_only: bool) -> f32 {
    let font = FontId::new(12.0, theme::font::sans_500());
    let widths: f32 = options
        .iter()
        .map(|(icon, t)| {
            let tw = if icon_only {
                0.0
            } else {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap((*t).to_owned(), font.clone(), Color32::PLACEHOLDER)
                        .size()
                        .x
                })
            };
            let gap = if tw > 0.0 { 6.0 } else { 0.0 };
            20.0 + if icon.is_some() { 14.0 + gap } else { 0.0 } + tw
        })
        .sum();
    widths + 2.0 * (options.len() as f32 - 1.0) + 6.0
}

/// `.segmented`: a track with one pressed option. Returns the clicked index, if any. With `icon_only`
/// the labels are only the options' accessible names, as the toolbar's layout switch does.
pub fn segmented(
    ui: &mut Ui,
    p: &Palette,
    options: &[(Option<Icon>, &str)],
    selected: usize,
    icon_only: bool,
) -> Option<usize> {
    let font = FontId::new(12.0, theme::font::sans_500());
    let shown = |t: &str| {
        if icon_only {
            String::new()
        } else {
            t.to_owned()
        }
    };
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, t)| {
            ui.painter()
                .layout_no_wrap(shown(t), font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let widths: Vec<f32> = options
        .iter()
        .zip(&galleys)
        .map(|((icon, _), g)| {
            let gap = if g.size().x > 0.0 { 6.0 } else { 0.0 };
            let icon_w = if icon.is_some() { 14.0 + gap } else { 0.0 };
            20.0 + icon_w + g.size().x
        })
        .collect();
    let total = widths.iter().sum::<f32>() + 2.0 * (options.len() as f32 - 1.0) + 6.0;
    let (track, _) = ui.allocate_exact_size(Vec2::new(total, 32.0), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(track, CornerRadius::same(9), p.surface_3);
    let mut clicked = None;
    let mut x = track.left() + 3.0;
    for (i, (((icon, text), galley), w)) in options.iter().zip(galleys).zip(&widths).enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(x, track.top() + 3.0), Vec2::new(*w, 26.0));
        let id = ui.scope_id().with(("segmented", i, *text));
        let response = ui.interact(rect, id, Sense::click());
        response
            .widget_info(|| WidgetInfo::selected(egui::Role::Button, true, i == selected, *text));
        let on = i == selected;
        let hovered = response.hovered();
        let ink = if on || hovered { p.ink } else { p.muted };
        if on {
            painter.add(p.shadow_card.as_shape(rect, CornerRadius::same(theme::R)));
            painter.rect(
                rect,
                CornerRadius::same(theme::R),
                p.surface,
                Stroke::new(1.0, p.card_hairline),
                StrokeKind::Inside,
            );
        }
        let mut cx = rect.left() + 10.0;
        if let Some(icon) = icon {
            icons::paint(
                &painter,
                egui::Rect::from_min_size(egui::pos2(cx, rect.center().y - 7.0), Vec2::splat(14.0)),
                icon,
                ink,
            );
            cx += 14.0 + 6.0;
        }
        painter.galley(
            egui::pos2(cx, rect.center().y - galley.size().y / 2.0),
            galley,
            ink,
        );
        focus_ring(ui, &response, rect, p, theme::R);
        if response.clicked() {
            clicked = Some(i);
        }
        let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        x += w + 2.0;
    }
    clicked
}

/// Colour tone for chips, pills and rail badges.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChipTone {
    Ok,
    Warn,
    Error,
    Accent,
    Neutral,
}

impl ChipTone {
    fn soft_colors(self, p: &Palette) -> (Color32, Color32) {
        match self {
            ChipTone::Ok => (p.ok_soft, p.ok),
            ChipTone::Warn => (p.warn_soft, p.warn),
            ChipTone::Error => (p.error_soft, p.error),
            ChipTone::Accent => (p.accent_soft, p.accent_text),
            ChipTone::Neutral => (p.surface_3, p.muted),
        }
    }
    fn solid_colors(self, p: &Palette) -> (Color32, Color32) {
        match self {
            ChipTone::Error => (p.error, p.accent_ink),
            ChipTone::Warn => (p.warn, p.accent_ink),
            ChipTone::Ok => (p.ok, p.accent_ink),
            ChipTone::Accent | ChipTone::Neutral => (p.accent, p.accent_ink),
        }
    }
}

/// `.sw`: a 36×20 toggle switch with a 16 px sliding knob, used in Settings.
pub fn switch(ui: &mut Ui, p: &Palette, value: &mut bool, label: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    if r.clicked() {
        *value = !*value;
    }
    r.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, *value, label));
    if ui.is_rect_visible(rect) {
        let t = ui
            .ctx()
            .animate_bool_with_time(r.id, *value, theme::ease_secs());
        let track = p.border_strong.lerp_to_gamma(p.accent, t);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(10), track);
        let knob_x = rect.left() + 10.0 + t * 16.0;
        let center = egui::pos2(knob_x, rect.center().y);
        painter.circle_filled(
            center + Vec2::new(0.0, 1.0),
            8.0,
            Color32::from_black_alpha(52),
        );
        painter.circle_filled(center, 8.0, Color32::WHITE);
        focus_ring(ui, &r, rect, p, 10);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `.chip` / `.pill` / `.badge-st`: compact rounded status tag.
pub fn chip(ui: &mut Ui, p: &Palette, icon: Option<Icon>, text: &str, tone: ChipTone) -> Response {
    chip_inner(ui, p, icon, text, tone, 20.0, 9, false, None)
}

/// Clickable 28 px action chip (e.g. `"4 issues"` in the PDF card header).
pub fn chip_button(
    ui: &mut Ui,
    p: &Palette,
    icon: Option<Icon>,
    text: &str,
    a11y_label: &str,
    tone: ChipTone,
) -> Response {
    chip_inner(ui, p, icon, text, tone, 28.0, 7, true, Some(a11y_label))
}

#[allow(clippy::too_many_arguments)]
fn chip_inner(
    ui: &mut Ui,
    p: &Palette,
    icon: Option<Icon>,
    text: &str,
    tone: ChipTone,
    height: f32,
    radius: u8,
    clickable: bool,
    a11y_label: Option<&str>,
) -> Response {
    let (bg, fg) = tone.soft_colors(p);
    let font_size = if height >= 26.0 { 12.5 } else { 11.0 };
    let icon_size = if height >= 26.0 { 14.0 } else { 12.0 };
    let pad = if height >= 26.0 { 10.0 } else { 6.0 };
    let galley = (!text.is_empty()).then(|| {
        ui.painter().layout_no_wrap(
            text.to_owned(),
            FontId::new(font_size, theme::font::sans_600()),
            fg,
        )
    });
    let text_w = galley.as_ref().map_or(0.0, |g| g.size().x);
    let icon_w = if icon.is_some() {
        icon_size + if text_w > 0.0 { 5.0 } else { 0.0 }
    } else {
        0.0
    };
    let width = (pad * 2.0 + icon_w + text_w).max(height);
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    if clickable {
        let name = a11y_label.unwrap_or(text);
        r.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, name));
    }
    if ui.is_rect_visible(rect) {
        let fill = if clickable && r.hovered() {
            bg.lerp_to_gamma(fg, 0.08)
        } else {
            bg
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(radius), fill);
        let mut x = rect.left() + (width - (icon_w + text_w)) / 2.0;
        if let Some(ic) = icon {
            icons::paint(
                painter,
                Rect::from_min_size(
                    egui::pos2(x, rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                ),
                ic,
                fg,
            );
            x += icon_size + 5.0;
        }
        if let Some(g) = galley {
            painter.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, fg);
        }
        if clickable {
            focus_ring(ui, &r, rect, p, radius);
        }
    }
    if clickable {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        r
    }
}

/// `.rail-btn`: 36×36 workspace rail button on `p.app`, with optional top-right count badge.
pub fn rail_button(
    ui: &mut Ui,
    p: &Palette,
    icon: Icon,
    label: &str,
    on: bool,
    toggle: bool,
    badge: Option<(usize, ChipTone)>,
) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
    r.widget_info(|| {
        if toggle {
            WidgetInfo::selected(egui::Role::Button, true, on, label)
        } else {
            WidgetInfo::labeled(egui::Role::Button, true, label)
        }
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rad = CornerRadius::same(theme::R_MD);
        if on {
            painter.add(p.shadow_card.as_shape(rect, rad));
            painter.rect(
                rect,
                rad,
                p.rail_active,
                Stroke::new(1.0, p.card_hairline),
                StrokeKind::Inside,
            );
        } else if r.hovered() {
            painter.rect_filled(rect, rad, p.surface_3);
        }
        let icon_color = if on {
            p.accent
        } else if r.hovered() {
            p.ink
        } else {
            p.muted
        };
        icons::paint(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
            icon,
            icon_color,
        );
        if let Some((count, tone)) = badge
            && count > 0
        {
            let (bg, fg) = tone.solid_colors(p);
            let txt = if count > 99 {
                "99+".to_owned()
            } else {
                count.to_string()
            };
            let g = painter.layout_no_wrap(txt, FontId::new(10.0, theme::font::sans_600()), fg);
            let bw = (g.size().x + 8.0).max(15.0);
            let br = Rect::from_min_size(
                egui::pos2(rect.right() - bw - 1.0, rect.top() + 2.0),
                Vec2::new(bw, 15.0),
            );
            painter.rect_filled(br, CornerRadius::same(8), bg);
            painter.galley(br.center() - g.size() / 2.0, g, fg);
        }
        focus_ring(ui, &r, rect, p, theme::R_MD);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `.row`: 30 px full-width navigation/list row used in sidebars, library nav and settings nav.
pub fn side_row(
    ui: &mut Ui,
    p: &Palette,
    icon: Option<Icon>,
    icon_color: Option<Color32>,
    text: &str,
    on: bool,
    trailing: Option<&str>,
) -> Response {
    let w = ui.available_width().max(60.0);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    r.widget_info(|| WidgetInfo::selected(egui::Role::Button, true, on, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rad = CornerRadius::same(7);
        if on {
            painter.add(p.shadow_card.as_shape(rect, rad));
            painter.rect(
                rect,
                rad,
                p.rail_active,
                Stroke::new(1.0, p.card_hairline),
                StrokeKind::Inside,
            );
        } else if r.hovered() {
            painter.rect_filled(rect, rad, p.surface_3);
        }
        let mut x = rect.left() + 8.0;
        let trail_g = trailing.map(|t| {
            painter.layout_no_wrap(
                t.to_owned(),
                FontId::new(11.5, theme::font::mono_500()),
                p.faint,
            )
        });
        let right_edge = rect.right() - 8.0 - trail_g.as_ref().map_or(0.0, |g| g.size().x + 8.0);
        if let Some(ic) = icon {
            let ic_col = if on {
                p.accent
            } else {
                icon_color.unwrap_or(p.faint)
            };
            icons::paint(
                painter,
                Rect::from_min_size(egui::pos2(x, rect.center().y - 7.5), Vec2::splat(15.0)),
                ic,
                ic_col,
            );
            x += 15.0 + 7.0;
        }
        let font = if on {
            FontId::new(13.0, theme::font::sans_500())
        } else {
            FontId::proportional(13.0)
        };
        let ink = if on { p.ink } else { p.ink_2 };
        let avail_text = (right_edge - x).max(16.0);
        let mut job = egui::text::LayoutJob::single_section(
            text.to_owned(),
            egui::TextFormat {
                font_id: font,
                color: ink,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: avail_text,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let g = ui.fonts_mut(|f| f.layout_job(job));
        painter.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, ink);
        if let Some(tg) = trail_g {
            painter.galley(
                egui::pos2(
                    rect.right() - 8.0 - tg.size().x,
                    rect.center().y - tg.size().y / 2.0,
                ),
                tg,
                p.faint,
            );
        }
        focus_ring(ui, &r, rect, p, 7);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Paints a rounded white workspace card directly onto `rect` (Editor card, PDF card, Library main).
pub fn paint_card(ui: &Ui, p: &Palette, rect: Rect) {
    let rad = CornerRadius::same(theme::R_LG);
    let painter = ui.painter();
    painter.add(p.shadow_card.as_shape(rect, rad));
    painter.rect(
        rect,
        rad,
        p.surface,
        Stroke::new(1.0, p.card_hairline),
        StrokeKind::Inside,
    );
}

/// Cached 128×128 Typeset Split app icon texture for crisp rendering in the rail and dialogs.
pub fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::unique("lemmata-logo-tex");
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return tex;
    }
    let img = image::load_from_memory(include_bytes!("../icons/128x128.png"))
        .expect("bundled 128x128 logo PNG")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture("lemmata-logo", color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Draws the Lemmata Typeset Split logo at `size × size`.
pub fn logo_mark(ui: &mut Ui, size: f32) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        let tex = logo_texture(ui.ctx());
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    r
}

/// Clickable Lemmata Typeset Split logo button with focus ring and accessible label.
pub fn logo_button(ui: &mut Ui, p: &Palette, size: f32, label: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, label));
    if ui.is_rect_visible(rect) {
        let tex = logo_texture(ui.ctx());
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        focus_ring(ui, &r, rect, p, theme::R_MD);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `kbd`: 11 px mono on the surface, with a 2 px bottom border.
pub fn kbd(ui: &mut Ui, p: &Palette, keys: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(keys.to_owned(), FontId::monospace(11.0), p.ink);
    let size = galley.size() + Vec2::new(12.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    let r = CornerRadius::same(theme::R_SM);
    painter.rect(rect, r, p.border_strong, Stroke::NONE, StrokeKind::Inside);
    painter.rect(
        rect.with_max_y(rect.bottom() - 2.0)
            .shrink2(Vec2::new(1.0, 0.0))
            .with_min_y(rect.top() + 1.0),
        r,
        p.surface,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    painter.galley(
        egui::pos2(rect.left() + 6.0, rect.top() + 1.0),
        galley,
        p.ink,
    );
    response
}

/// `.label`: 11 px, weight 600, uppercase, letter-spacing 0.06em, muted.
pub fn label(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.0, theme::font::sans_600()),
            color: p.muted,
            extra_letter_spacing: 0.06 * 11.0,
            ..Default::default()
        },
    );
    ui.label(job)
}

/// Text centred in a rect, for the splash and empty states.
pub fn centered_text(ui: &Ui, rect: egui::Rect, text: &str, font: FontId, color: Color32) {
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, text, font, color);
}

/// `.search`: a 32 px box with a magnifier and an input without its own frame.
pub fn search_input(ui: &mut Ui, p: &Palette, text: &mut String, hint: &str) -> Response {
    let rect = ui
        .available_rect_before_wrap()
        .with_max_y(ui.available_rect_before_wrap().top() + 32.0);
    let slot = ui.painter().add(egui::Shape::Noop);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    icons::show(&mut child, icons::SEARCH, 15.0, p.muted);
    let r = child.add(
        egui::TextEdit::singleline(text)
            .hint_text(egui::RichText::new(hint).color(p.faint))
            .font(FontId::proportional(13.0))
            .text_color(p.ink)
            .frame(egui::Frame::NONE)
            .align(egui::Align2::LEFT_CENTER)
            .min_size(Vec2::new(0.0, 32.0))
            .desired_width(child.available_width()),
    );
    ui.ctx().accesskit_node_builder(r.id, |n| n.set_label(hint));
    let focused = r.has_focus();
    if focused {
        ui.painter().rect_stroke(
            rect.expand(1.5),
            CornerRadius::same(theme::R + 2),
            Stroke::new(3.0, Palette::mix(p.accent, 22.0)),
            StrokeKind::Outside,
        );
    }
    ui.painter().set(
        slot,
        egui::Shape::rect_filled(rect, CornerRadius::same(theme::R), p.surface),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::R),
        Stroke::new(1.0, if focused { p.accent } else { p.border_strong }),
        StrokeKind::Inside,
    );
    ui.allocate_rect(rect, Sense::hover());
    if !r.has_focus()
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|pt| rect.contains(pt))
        })
    {
        r.request_focus();
    }
    r
}

/// `select.input`: a 32 px drop-down of `(value, label)` options, named `name` for screen readers.
/// True when the value changed.
pub fn select(
    ui: &mut Ui,
    p: &Palette,
    name: &str,
    value: &mut String,
    options: &[(&str, &str)],
) -> bool {
    let current = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or(value.as_str(), |(_, l)| *l)
        .to_owned();
    let before = value.clone();
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 32.0;
        ui.spacing_mut().button_padding = Vec2::new(10.0, 0.0);
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.weak_bg_fill = p.surface;
            state.bg_fill = p.surface;
            state.expansion = 0.0;
        }
        // `select.input`: --border-strong, --faint on hover, the accent while open.
        w.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
        w.hovered.bg_stroke = Stroke::new(1.0, p.faint);
        w.active.bg_stroke = Stroke::new(1.0, p.accent);
        w.open.bg_stroke = Stroke::new(1.0, p.accent);
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.corner_radius = CornerRadius::same(theme::R);
        }
        let ink = p.ink;
        let r = egui::ComboBox::from_id_salt(name)
            .icon(move |ui, rect, _, _| {
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
                    icons::CHEVRON_DOWN,
                    ink,
                )
            })
            .selected_text(egui::RichText::new(current).size(13.0).color(p.ink))
            .width(ui.available_width().min(ui.max_rect().width()))
            .show_ui(ui, |ui| {
                for (v, l) in options {
                    ui.selectable_value(value, (*v).to_owned(), *l);
                }
            });
        ui.ctx()
            .accesskit_node_builder(r.response.id, |n| n.set_label(name));
    });
    *value != before
}

/// `.field`: an uppercase label over its control, 5 px apart.
pub fn field<R>(ui: &mut Ui, p: &Palette, text: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        label(ui, p, text);
        // The input inside takes the label as its accessible name, as `<label for>` gives it.
        ui.data_mut(|d| d.insert_temp(field_label_id(), text.to_owned()));
        let r = add(ui);
        ui.data_mut(|d| d.remove::<String>(field_label_id()));
        r
    })
    .inner
}

fn field_label_id() -> egui::Id {
    egui::Id::unique("field-label")
}

/// `.check`: a 15 px box in the accent colour and its label; the whole row toggles.
pub fn checkbox(ui: &mut Ui, p: &Palette, value: &mut bool, text: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(13.0), p.ink);
    let (rect, r) = ui.allocate_exact_size(
        Vec2::new(15.0 + 8.0 + galley.size().x, 18.0f32.max(galley.size().y)),
        Sense::click(),
    );
    if r.clicked() {
        *value = !*value;
    }
    r.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, *value, text));
    let bx = Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - 7.5),
        Vec2::splat(15.0),
    );
    if *value {
        ui.painter()
            .rect_filled(bx, CornerRadius::same(3), p.accent);
        icons::paint(ui.painter(), bx.shrink(2.0), icons::CHECK, p.accent_ink);
    } else {
        ui.painter().rect(
            bx,
            CornerRadius::same(3),
            p.surface,
            Stroke::new(1.0, p.faint),
            StrokeKind::Inside,
        );
    }
    ui.painter().galley(
        egui::pos2(bx.right() + 8.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.ink,
    );
    focus_ring(ui, &r, bx, p, 3);
    r
}

/// Mixed-style text (`<b>`, `<code>`, colours) as one wrapping paragraph.
#[derive(Default)]
pub struct Rich(pub egui::text::LayoutJob);

impl Rich {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(mut self, text: &str, font: FontId, color: Color32) -> Self {
        self.0.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font,
                color,
                ..Default::default()
            },
        );
        self
    }
    /// Line height in points, as CSS `line-height` × font size.
    pub fn line_height(mut self, px: f32) -> Self {
        for s in &mut self.0.sections {
            s.format.line_height = Some(px);
        }
        self
    }
    pub fn wrap(mut self, width: f32) -> Self {
        self.0.wrap.max_width = width;
        self
    }
    /// `text-align: center`.
    pub fn centered(mut self) -> Self {
        self.0.halign = egui::Align::Center;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        // A Label wraps at the available width, so the job's own width becomes the Ui's.
        let width = self.0.wrap.max_width;
        if width.is_finite() {
            // In a horizontal row a Label would not wrap at all, so it gets a box of its own.
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                egui::Layout::top_down(self.0.halign),
                |ui| {
                    ui.set_max_width(width);
                    ui.add(egui::Label::new(self.0).selectable(false).wrap())
                },
            )
            .inner
        } else {
            ui.add(egui::Label::new(self.0).selectable(false).extend())
        }
    }
}

/// A plain paragraph: one font and colour, wrapped at `width`.
pub fn paragraph(
    ui: &mut Ui,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
    width: f32,
) -> Response {
    let size = font.size;
    Rich::new()
        .add(text, font, color)
        .line_height(size * line_height)
        .wrap(width)
        .show(ui)
}

/// A column of at most `width` points, centred in the available space.
pub fn column<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let w = width.min(avail.width());
    let rect = egui::Rect::from_min_size(
        egui::pos2(avail.center().x - w / 2.0, avail.top()),
        Vec2::new(w, avail.height()),
    );
    let mut inner = None;
    let response = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.set_width(w);
            inner = Some(add(ui));
        },
    );
    // Claim the column's full height so a surrounding scroll area knows how tall it is.
    ui.allocate_rect(
        egui::Rect::from_min_max(
            avail.min,
            egui::pos2(avail.max.x, response.response.rect.bottom()),
        ),
        Sense::hover(),
    );
    inner.unwrap()
}

/// CSS `radial-gradient(rx ry at center, color, transparent stop)`, drawn as a fan of triangles.
pub fn radial_glow(
    painter: &egui::Painter,
    center: egui::Pos2,
    radii: Vec2,
    color: Color32,
    stop: f32,
) {
    const SEGMENTS: usize = 64;
    const RINGS: usize = 32;
    let mut mesh = egui::Mesh::default();
    let ring = |k: usize| k as f32 / RINGS as f32 * stop;
    let fade = |t: f32| color.gamma_multiply(1.0 - (t / stop).clamp(0.0, 1.0));
    for r in 0..=RINGS {
        let t = ring(r);
        for i in 0..SEGMENTS {
            let a = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                center + Vec2::new(a.cos() * radii.x, a.sin() * radii.y) * t,
                fade(t),
            );
        }
    }
    for r in 0..RINGS as u32 {
        for i in 0..SEGMENTS as u32 {
            let n = SEGMENTS as u32;
            let (a, b) = (r * n + i, r * n + (i + 1) % n);
            let (c, d) = (a + n, b + n);
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// `.input`: 32 px tall, 10 px padding, `--border-strong`, accent border and a soft glow on focus.
pub fn text_input(ui: &mut Ui, p: &Palette, text: &mut String, hint: &str, mono: bool) -> Response {
    let font = if mono {
        FontId::monospace(12.5)
    } else {
        FontId::proportional(13.0)
    };
    // The fill goes under the text, so its slot is taken before the text is drawn.
    let fill_slot = ui.painter().add(egui::Shape::Noop);
    let out = egui::TextEdit::singleline(text)
        .hint_text(egui::RichText::new(hint).font(font.clone()).color(p.faint))
        .font(font)
        .text_color(p.ink)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 0)))
        .align(egui::Align2::LEFT_CENTER)
        .min_size(Vec2::new(0.0, 32.0))
        .desired_width(ui.available_width())
        .show(ui);
    let response = out.response.response;
    if let Some(name) = ui.data(|d| d.get_temp::<String>(field_label_id())) {
        ui.ctx()
            .accesskit_node_builder(response.id, |n| n.set_label(name));
    }
    let rect = response.rect;
    let focused = response.has_focus();
    let hovered = response.hovered();
    let painter = ui.painter();
    if focused {
        painter.rect_stroke(
            rect.expand(1.5),
            CornerRadius::same(theme::R + 2),
            Stroke::new(3.0, Palette::mix(p.accent, 22.0)),
            StrokeKind::Outside,
        );
    }
    let border = if focused {
        p.accent
    } else if hovered {
        p.faint
    } else {
        p.border_strong
    };
    painter.rect(
        rect,
        CornerRadius::same(theme::R),
        Color32::TRANSPARENT,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    painter.set(
        fill_slot,
        egui::Shape::rect_filled(rect, CornerRadius::same(theme::R), p.surface),
    );
    response
}

/// `textarea.input`: the multi-line field, styled like `text_input`, `rows` tall.
pub fn text_area(
    ui: &mut Ui,
    p: &Palette,
    text: &mut String,
    hint: &str,
    rows: usize,
    enabled: bool,
) -> Response {
    let fill_slot = ui.painter().add(egui::Shape::Noop);
    let out = ui.add_enabled(
        enabled,
        egui::TextEdit::multiline(text)
            .hint_text(egui::RichText::new(hint).color(p.faint))
            .font(FontId::proportional(13.0))
            .text_color(p.ink)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 7)))
            .desired_rows(rows)
            .desired_width(ui.available_width()),
    );
    ui.ctx()
        .accesskit_node_builder(out.id, |n| n.set_label(hint));
    let rect = out.rect;
    let focused = out.has_focus();
    if focused {
        ui.painter().rect_stroke(
            rect.expand(1.5),
            CornerRadius::same(theme::R + 2),
            Stroke::new(3.0, Palette::mix(p.accent, 22.0)),
            StrokeKind::Outside,
        );
    }
    let border = if focused {
        p.accent
    } else if out.hovered() {
        p.faint
    } else {
        p.border_strong
    };
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::R),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    ui.painter().set(
        fill_slot,
        egui::Shape::rect_filled(rect, CornerRadius::same(theme::R), p.surface),
    );
    out
}

#[derive(Default)]
struct ModalFocus(Vec<(u8, egui::Id)>);

impl egui::Plugin for ModalFocus {
    fn debug_name(&self) -> &'static str {
        "restore modal focus"
    }

    fn output_hook(&mut self, ctx: &egui::Context, _output: &mut egui::FullOutput) {
        for (delay, id) in std::mem::take(&mut self.0) {
            if delay == 0 {
                ctx.memory_mut(|m| m.request_focus(id));
            } else {
                self.0.push((delay - 1, id));
            }
        }
    }
}

/// What a modal reports back each frame.
pub struct ModalResponse {
    /// Esc, the × button or a click on the backdrop.
    pub closed: bool,
}

/// Modal.svelte: a dimmed backdrop and a panel with a title, a body and an optional footer on
/// `--surface-2`. Esc, × and clicking the backdrop close it.
pub fn modal<R>(
    ctx: &egui::Context,
    p: &Palette,
    title: &str,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
    footer: Option<&mut dyn FnMut(&mut Ui)>,
) -> ModalResponse {
    modal_with(ctx, p, title, None, width, body, footer)
}

/// `modal` with a muted line under the title.
pub fn modal_with<R>(
    ctx: &egui::Context,
    p: &Palette,
    title: &str,
    subtitle: Option<&str>,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
    footer: Option<&mut dyn FnMut(&mut Ui)>,
) -> ModalResponse {
    modal_placed(ctx, p, title, subtitle, width, false, body, footer)
}

/// Records the previously focused widget when a custom modal enters, so closing restores it.
pub fn modal_focus_enter(ctx: &egui::Context, id_salt: &str) -> Option<egui::Id> {
    let entered_id = egui::Id::unique(("modal-entered", id_salt));
    let restore_id = egui::Id::unique(("modal-restore", id_salt));
    let seen_id = egui::Id::unique(("modal-seen", id_salt));
    let pass = ctx.cumulative_pass_nr();
    let drawn_just_before = ctx.data_mut(|d| {
        let last = d.get_temp::<u64>(seen_id);
        d.insert_temp(seen_id, pass);
        last.is_some_and(|l| l + 1 >= pass)
    });
    let entered =
        drawn_just_before && ctx.data_mut(|d| d.get_temp::<bool>(entered_id).unwrap_or(false));
    if entered {
        ctx.data_mut(|d| {
            d.get_temp::<Option<egui::Id>>(restore_id)
                .unwrap_or_default()
        })
    } else {
        let focused = ctx.memory(|m| m.focused());
        ctx.data_mut(|d| {
            d.insert_temp(entered_id, true);
            d.insert_temp(restore_id, focused);
        });
        focused
    }
}

/// Restores focus saved by `modal_focus_enter` when a custom modal closes.
pub fn modal_focus_restore(ctx: &egui::Context, id_salt: &str, previous: Option<egui::Id>) {
    let entered_id = egui::Id::unique(("modal-entered", id_salt));
    ctx.data_mut(|d| d.insert_temp(entered_id, false));
    if let Some(id) = previous {
        ctx.add_plugin(ModalFocus::default());
        ctx.with_plugin(|plugin: &mut ModalFocus| plugin.0.push((1, id)));
    }
}

/// Restores focus saved by `modal_focus_enter` or `modal_full` using only the modal's salt/title.
pub fn modal_focus_restore_by_salt(ctx: &egui::Context, id_salt: &str) {
    let restore_id = egui::Id::unique(("modal-restore", id_salt));
    let previous = ctx.data_mut(|d| {
        d.get_temp::<Option<egui::Id>>(restore_id)
            .unwrap_or_default()
    });
    modal_focus_restore(ctx, id_salt, previous);
}

/// `modal_with` for a dialog the user types into: a click beside the panel doesn't close it (and
/// drop the edits); Esc, × and the buttons do.
pub fn modal_editing<R>(
    ctx: &egui::Context,
    p: &Palette,
    title: &str,
    subtitle: Option<&str>,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
    footer: Option<&mut dyn FnMut(&mut Ui)>,
) -> ModalResponse {
    modal_full(ctx, p, title, subtitle, width, false, false, body, footer)
}

/// `top`: `.backdrop.top`, the panel near the top (11 % down) instead of centred, for dialogs
/// whose height changes while typing.
#[allow(clippy::too_many_arguments)]
pub fn modal_placed<R>(
    ctx: &egui::Context,
    p: &Palette,
    title: &str,
    subtitle: Option<&str>,
    width: f32,
    top: bool,
    body: impl FnOnce(&mut Ui) -> R,
    footer: Option<&mut dyn FnMut(&mut Ui)>,
) -> ModalResponse {
    modal_full(ctx, p, title, subtitle, width, top, true, body, footer)
}

#[allow(clippy::too_many_arguments)]
fn modal_full<R>(
    ctx: &egui::Context,
    p: &Palette,
    title: &str,
    subtitle: Option<&str>,
    width: f32,
    top: bool,
    backdrop_closes: bool,
    body: impl FnOnce(&mut Ui) -> R,
    footer: Option<&mut dyn FnMut(&mut Ui)>,
) -> ModalResponse {
    let screen = ctx.content_rect();
    let any_popup_open = egui::Popup::is_any_open(ctx);
    let entered_id = egui::Id::unique(("modal-entered", title));
    let restore_id = egui::Id::unique(("modal-restore", title));
    // Closed by a footer button the caller handled: the dialog was not drawn since, so this is a
    // new opening (first focus, and the focus to return to, are taken again).
    let seen_id = egui::Id::unique(("modal-seen", title));
    let pass = ctx.cumulative_pass_nr();
    let drawn_just_before = ctx.data_mut(|d| {
        let last = d.get_temp::<u64>(seen_id);
        d.insert_temp(seen_id, pass);
        last.is_some_and(|l| l + 1 >= pass)
    });
    let entered =
        drawn_just_before && ctx.data_mut(|d| d.get_temp::<bool>(entered_id).unwrap_or(false));
    let previous_focus = if entered {
        ctx.data_mut(|d| {
            d.get_temp::<Option<egui::Id>>(restore_id)
                .unwrap_or_default()
        })
    } else {
        let focused = ctx.memory(|m| m.focused());
        ctx.data_mut(|d| {
            d.insert_temp(entered_id, true);
            d.insert_temp(restore_id, focused);
        });
        focused
    };
    let mut closed = false;
    // The backdrop: rgba(12, 17, 22, .42) over everything, swallowing clicks.
    let backdrop = egui::Area::new(egui::Id::unique(("modal-backdrop", title)))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0, Color32::from_rgba_unmultiplied(12, 17, 22, 107));
            r
        });
    let width = width.min(screen.width() - 48.0);
    let chrome_id = egui::Id::unique(("modal-chrome-h", title));
    let body_h_id = egui::Id::unique(("modal-body-h", title));
    let has_footer = footer.is_some();
    let chrome_h: f32 = ctx
        .data(|d| d.get_temp(chrome_id))
        .unwrap_or(if has_footer { 114.0 } else { 56.0 });
    let prev_body_h: f32 = ctx.data(|d| d.get_temp(body_h_id)).unwrap_or(0.0);
    let top_y = screen.top() + screen.height() * 0.11;
    let max_modal_h = if top {
        (screen.bottom() - 16.0 - top_y).max(200.0)
    } else {
        (screen.height() - 32.0).max(200.0)
    };
    let body_max_h = (max_modal_h - chrome_h - 32.0).max(120.0);
    let needs_scroll = prev_body_h > body_max_h + 1.0;
    let area = egui::Area::new(egui::Id::unique(("modal", title)))
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .constrain_to(screen.shrink(8.0))
        .pivot(if top {
            egui::Align2::CENTER_TOP
        } else {
            egui::Align2::CENTER_CENTER
        })
        .fixed_pos(if top {
            egui::pos2(screen.center().x, top_y)
        } else {
            screen.center()
        });
    let layer = area.layer();
    ctx.memory_mut(|m| m.set_modal_layer(layer));
    let mut measured_chrome = 0.0f32;
    let mut measured_body = 0.0f32;
    let panel = area.show(ctx, |ui| {
        ui.set_width(width);
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(theme::R_XL)
            .shadow(p.shadow_lg)
            .show(ui, |ui| {
                // Inside the 1 px border; the header, body and footer frames add their own padding.
                ui.set_width(width - 2.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let head = egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 22,
                        right: 20,
                        top: 18,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width() - 32.0 - 12.0);
                                ui.label(
                                    egui::RichText::new(title)
                                        .font(FontId::new(16.0, theme::font::sans_600()))
                                        .color(p.ink),
                                );
                                if let Some(sub) = subtitle {
                                    ui.add_space(3.0);
                                    paragraph(
                                        ui,
                                        sub,
                                        FontId::proportional(13.0),
                                        p.muted,
                                        1.45,
                                        ui.available_width(),
                                    );
                                }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                // Not given the first focus: Enter or Space right after opening
                                // would close a dialog that only wants an answer.
                                let close = Button::icon(icons::X, "Close")
                                    .size(Size::Md)
                                    .show_icon(ui, p);
                                if close.clicked() {
                                    closed = true;
                                }
                            });
                        });
                    });
                measured_chrome += head.response.rect.height();
                let body_frame = egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 20,
                        right: 20,
                        top: 12,
                        bottom: 18,
                    })
                    .show(ui, |ui| {
                        if needs_scroll {
                            let out = egui::ScrollArea::vertical()
                                .id_salt(("modal-body", title))
                                .max_height(body_max_h)
                                .min_scrolled_height(body_max_h)
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                )
                                .show(ui, |ui| {
                                    ui.set_width((ui.available_width() - 8.0).max(120.0));
                                    ui.spacing_mut().item_spacing.y = 14.0;
                                    body(ui)
                                });
                            measured_body = out.content_size.y;
                            out.inner
                        } else {
                            ui.spacing_mut().item_spacing.y = 14.0;
                            body(ui)
                        }
                    });
                if !needs_scroll {
                    measured_body = (body_frame.response.rect.height() - 30.0).max(0.0);
                }
                let inner = body_frame.inner;
                if let Some(footer) = footer {
                    let r = egui::Frame::new()
                        .fill(p.surface_2)
                        // Inside the panel's R_XL (14 px) corners.
                        .corner_radius(CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: theme::R_XL - 1,
                            se: theme::R_XL - 1,
                        })
                        .inner_margin(egui::Margin::symmetric(20, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    footer(ui);
                                },
                            );
                        });
                    measured_chrome += r.response.rect.height();
                    let top = r.response.rect.top();
                    ui.painter()
                        .hline(r.response.rect.x_range(), top, Stroke::new(1.0, p.border));
                }
                inner
            })
            .inner
    });
    if (measured_chrome - chrome_h).abs() > 1.0 {
        ctx.data_mut(|d| d.insert_temp(chrome_id, measured_chrome));
        ctx.request_repaint();
    }
    if (measured_body - prev_body_h).abs() > 1.0 {
        ctx.data_mut(|d| d.insert_temp(body_h_id, measured_body));
        if (measured_body > body_max_h + 1.0) != needs_scroll {
            ctx.request_repaint();
        }
    }
    panel
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Dialog, true, title));
    if backdrop.inner.clicked()
        && !any_popup_open
        && !panel
            .response
            .rect
            .contains(backdrop.inner.interact_pointer_pos().unwrap_or_default())
        && backdrop_closes
    {
        closed = true;
    }
    if ctx.memory(|m| m.top_modal_layer() == Some(layer))
        && !any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        closed = true;
    }
    if closed {
        ctx.data_mut(|d| d.insert_temp(entered_id, false));
        if let Some(id) = previous_focus {
            ctx.add_plugin(ModalFocus::default());
            ctx.with_plugin(|plugin: &mut ModalFocus| plugin.0.push((1, id)));
        }
    }
    ModalResponse { closed }
}

/// A row of a drop menu.
#[derive(Clone)]
pub enum MenuItem<'a> {
    Item {
        icon: Option<Icon>,
        label: &'a str,
        hint: Option<&'a str>,
        danger: bool,
        /// Some for radio and check items: a tick column, ticked when true.
        check: Option<bool>,
        enabled: bool,
    },
    /// `.menu .heading`: an uppercase group title.
    Heading(&'a str),
    Separator,
}

impl<'a> MenuItem<'a> {
    pub fn new(icon: Icon, label: &'a str) -> Self {
        Self::Item {
            icon: Some(icon),
            label,
            hint: None,
            danger: false,
            check: None,
            enabled: true,
        }
    }
    /// A text-only item.
    pub fn plain(label: &'a str) -> Self {
        Self::Item {
            icon: None,
            label,
            hint: None,
            danger: false,
            check: None,
            enabled: true,
        }
    }
    /// A radio or check item with the tick column.
    pub fn check(label: &'a str, on: bool) -> Self {
        Self::Item {
            icon: None,
            label,
            hint: None,
            danger: false,
            check: Some(on),
            enabled: true,
        }
    }
    pub fn hint(mut self, text: &'a str) -> Self {
        if let Self::Item { hint, .. } = &mut self {
            *hint = Some(text);
        }
        self
    }
    pub fn danger(mut self) -> Self {
        if let Self::Item { danger, .. } = &mut self {
            *danger = true;
        }
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        if let Self::Item { enabled, .. } = &mut self {
            *enabled = on;
        }
        self
    }
}

pub enum MenuResult {
    Open,
    /// The index of the item clicked; the menu is closed.
    Clicked(usize),
    /// Esc or a click elsewhere.
    Closed,
}

/// DropMenu.svelte + `.menu`: opens below `anchor` (above when there is no room), aligned to its
/// start or end edge.
pub fn drop_menu(
    ctx: &egui::Context,
    p: &Palette,
    id: egui::Id,
    anchor: egui::Rect,
    align_end: bool,
    items: &[MenuItem],
) -> MenuResult {
    let screen = ctx.content_rect();
    let size_id = id.with("size");
    let content_h: f32 = items
        .iter()
        .map(|it| match it {
            MenuItem::Separator => 11.0,
            MenuItem::Heading(_) => 22.0,
            MenuItem::Item { .. } => 32.0,
        })
        .sum();
    let max_menu_h = (screen.height() - 16.0).max(120.0);
    let scrollable = content_h + 12.0 > max_menu_h;
    let size: Vec2 = ctx
        .data(|d| d.get_temp(size_id))
        .unwrap_or(Vec2::new(220.0, (content_h + 12.0).min(max_menu_h)));
    let menu_h = (content_h + 12.0).min(max_menu_h);
    let left = if align_end {
        anchor.right() - size.x
    } else {
        anchor.left()
    };
    let avail_below = screen.bottom() - anchor.bottom() - 14.0;
    let avail_above = anchor.top() - screen.top() - 14.0;
    let top = if menu_h <= avail_below {
        anchor.bottom() + 6.0
    } else if menu_h <= avail_above {
        anchor.top() - menu_h - 6.0
    } else if avail_below >= avail_above {
        (anchor.bottom() + 6.0).min((screen.bottom() - menu_h - 8.0).max(screen.top() + 8.0))
    } else {
        (anchor.top() - menu_h - 6.0).max(screen.top() + 8.0)
    };
    let pos = egui::pos2(
        left.clamp(
            screen.left() + 8.0,
            (screen.right() - size.x - 8.0).max(screen.left() + 8.0),
        ),
        top.clamp(
            screen.top() + 8.0,
            (screen.bottom() - menu_h - 8.0).max(screen.top() + 8.0),
        ),
    );
    let mut result = MenuResult::Open;
    let font = FontId::proportional(13.0);
    // The first frame of this opening: remember what had focus, to give it back on closing.
    let seen_id = id.with("seen");
    let frame = ctx.cumulative_frame_nr();
    let seen: Option<(Option<egui::Id>, u64)> = ctx.data(|d| d.get_temp(seen_id));
    let first = seen.is_none_or(|(_, f)| frame > f + 1);
    let opener = if first {
        ctx.memory(|m| m.focused())
    } else {
        seen.and_then(|s| s.0)
    };
    ctx.data_mut(|d| d.insert_temp(seen_id, (opener, frame)));
    let mut rows: Vec<egui::Id> = Vec::new();
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .constrain_to(screen.shrink(4.0))
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(theme::R_LG)
                .shadow(p.shadow)
                .inner_margin(5)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.ctx()
                        .accesskit_node_builder(ui.scope_id(), |n| n.set_role(egui::Role::Menu));
                    let has_icons = items.iter().any(|it| {
                        matches!(
                            it,
                            MenuItem::Item { icon: Some(_), .. }
                                | MenuItem::Item { check: Some(_), .. }
                        )
                    });
                    let lead = if has_icons { 25.0 } else { 0.0 };
                    let row_w = items
                        .iter()
                        .map(|it| match it {
                            MenuItem::Item { label, hint, .. } => {
                                let l = ui.fonts_mut(|f| {
                                    f.layout_no_wrap((*label).into(), font.clone(), p.ink)
                                        .size()
                                        .x
                                });
                                let h = hint.map_or(0.0, |h| {
                                    ui.fonts_mut(|f| {
                                        f.layout_no_wrap(h.into(), FontId::monospace(11.0), p.faint)
                                            .size()
                                            .x
                                    }) + 10.0
                                });
                                20.0 + lead + l + h
                            }
                            _ => 0.0,
                        })
                        .fold(190.0f32, f32::max)
                        .min((screen.width() - 32.0).max(160.0));
                    let mut draw_items = |ui: &mut Ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, it) in items.iter().enumerate() {
                            match it {
                                MenuItem::Separator => {
                                    ui.add_space(5.0);
                                    let y = ui.cursor().top();
                                    ui.painter().hline(
                                        (ui.max_rect().left() + 2.0)
                                            ..=(ui.max_rect().left() + row_w - 2.0),
                                        y,
                                        Stroke::new(1.0, p.border),
                                    );
                                    ui.add_space(6.0);
                                }
                                MenuItem::Heading(text) => {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(row_w, 22.0),
                                        Sense::hover(),
                                    );
                                    let mut job = egui::text::LayoutJob::default();
                                    job.append(
                                        &text.to_uppercase(),
                                        0.0,
                                        egui::TextFormat {
                                            font_id: FontId::new(11.0, theme::font::sans_600()),
                                            color: p.faint,
                                            extra_letter_spacing: 0.66,
                                            ..Default::default()
                                        },
                                    );
                                    let g = ui.fonts_mut(|f| f.layout_job(job));
                                    ui.painter().galley(
                                        egui::pos2(
                                            rect.left() + 10.0,
                                            rect.bottom() - 3.0 - g.size().y,
                                        ),
                                        g,
                                        p.faint,
                                    );
                                }
                                MenuItem::Item {
                                    icon,
                                    label,
                                    hint,
                                    danger,
                                    check,
                                    enabled,
                                } => {
                                    let sense = if *enabled {
                                        Sense::click()
                                    } else {
                                        Sense::hover()
                                    };
                                    let (rect, r) =
                                        ui.allocate_exact_size(Vec2::new(row_w, 32.0), sense);
                                    r.widget_info(|| match check {
                                        Some(on) => egui::WidgetInfo::selected(
                                            egui::Role::MenuItemRadio,
                                            *enabled,
                                            *on,
                                            *label,
                                        ),
                                        None => egui::WidgetInfo::labeled(
                                            egui::Role::MenuItem,
                                            *enabled,
                                            *label,
                                        ),
                                    });
                                    if *enabled {
                                        rows.push(r.id);
                                    }
                                    if *enabled && (r.hovered() || r.has_focus()) {
                                        ui.painter().rect_filled(
                                            rect,
                                            CornerRadius::same(theme::R_SM),
                                            p.surface_3,
                                        );
                                    }
                                    let alpha = if *enabled { 1.0 } else { 0.5 };
                                    let (ink, icon_ink) = if *danger {
                                        (p.error, p.error)
                                    } else {
                                        (p.ink, p.muted)
                                    };
                                    let (ink, icon_ink) =
                                        (ink.gamma_multiply(alpha), icon_ink.gamma_multiply(alpha));
                                    let mut x = rect.left() + 10.0;
                                    match (icon, check) {
                                        (Some(icon), _) => icons::paint(
                                            ui.painter(),
                                            egui::Rect::from_min_size(
                                                egui::pos2(x, rect.center().y - 7.5),
                                                Vec2::splat(15.0),
                                            ),
                                            icon,
                                            icon_ink,
                                        ),
                                        (None, Some(true)) => icons::paint(
                                            ui.painter(),
                                            egui::Rect::from_min_size(
                                                egui::pos2(x + 1.0, rect.center().y - 7.0),
                                                Vec2::splat(14.0),
                                            ),
                                            icons::CHECK,
                                            p.accent,
                                        ),
                                        _ => {}
                                    }
                                    if has_icons {
                                        x += lead;
                                    }
                                    ui.painter().text(
                                        egui::pos2(x, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        *label,
                                        font.clone(),
                                        ink,
                                    );
                                    if let Some(h) = hint {
                                        ui.painter().text(
                                            egui::pos2(rect.right() - 10.0, rect.center().y),
                                            Align2::RIGHT_CENTER,
                                            *h,
                                            FontId::monospace(11.0),
                                            p.faint,
                                        );
                                    }
                                    if r.clicked() {
                                        result = MenuResult::Clicked(i);
                                    }
                                }
                            }
                        }
                    };
                    if scrollable {
                        let sh = (max_menu_h - 12.0).max(100.0);
                        egui::ScrollArea::vertical()
                            .id_salt(id.with("scroll"))
                            .max_height(sh)
                            .min_scrolled_height(sh)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .show(ui, draw_items);
                    } else {
                        draw_items(ui);
                    }
                });
        });
    ctx.data_mut(|d| d.insert_temp(size_id, area.response.rect.size()));
    // Keyboard: the first item gets focus when the menu opens; egui moves it on the arrow keys.
    if first && let Some(&top) = rows.first() {
        ctx.memory_mut(|m| m.request_focus(top));
    }
    let key = |k| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
    let (home, end) = (key(egui::Key::Home), key(egui::Key::End));
    if home && let Some(&r) = rows.first() {
        ctx.memory_mut(|m| m.request_focus(r));
    }
    if end && let Some(&r) = rows.last() {
        ctx.memory_mut(|m| m.request_focus(r));
    }
    if matches!(result, MenuResult::Open) {
        let away = !first
            && ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p))
            });
        // Esc closes the menu and nothing under it.
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if away || esc {
            result = MenuResult::Closed;
        }
        if esc && let Some(opener) = opener {
            ctx.memory_mut(|m| m.request_focus(opener));
        }
    }
    if !matches!(result, MenuResult::Open) {
        ctx.data_mut(|d| d.remove::<(Option<egui::Id>, u64)>(seen_id));
    }
    result
}

/// Splitter.svelte: a 6 px handle over the edge between two panes; an accent line on hover and
/// while dragging, double-click resets. Returns the new size when it changed.
#[allow(clippy::too_many_arguments)]
pub fn splitter(
    ui: &mut Ui,
    p: &Palette,
    id: egui::Id,
    x: f32,
    y_range: egui::Rangef,
    size: f32,
    min: f32,
    max: f32,
    after: bool,
    label: &str,
) -> Option<f32> {
    let rect = egui::Rect::from_x_y_ranges((x - 3.0)..=(x + 3.0), y_range);
    let r = ui.interact(rect, id, Sense::click_and_drag());
    r.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::Splitter, true, label);
        info.value = Some(f64::from(size));
        info
    });
    focus_ring(ui, &r, rect, p, 2);
    // From the keyboard: arrows move the edge 16 px (64 with Shift).
    if r.has_focus() {
        let step = ui.input_mut(|i| {
            let big = if i.modifiers.shift { 64.0 } else { 16.0 };
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowLeft)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
            {
                -big
            } else if i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowRight)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
            {
                big
            } else {
                0.0
            }
        });
        if step != 0.0 {
            let next = (size + if after { -step } else { step }).clamp(min, max);
            if next != size {
                return Some(next);
            }
        }
    }
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        ui.painter()
            .rect_filled(rect.shrink2(Vec2::new(2.0, 0.0)), 0, p.accent);
    }
    if r.double_clicked() {
        return Some(f32::NAN);
    }
    if r.dragged() {
        let dx = r.drag_delta().x;
        let next = (size + if after { -dx } else { dx }).clamp(min, max);
        if next != size {
            return Some(next);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn a_drop_menu_is_driven_by_the_arrow_keys() {
        let p = theme::LIGHT;
        let id = egui::Id::unique("menu");
        let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
        let out = seen.clone();
        let mut h = Harness::builder()
            .with_size([400.0, 300.0])
            .with_context_setup(theme::install_fonts)
            .build_ui(move |ui| {
                let items = [
                    MenuItem::plain("One"),
                    MenuItem::plain("Two"),
                    MenuItem::plain("Three"),
                ];
                let anchor = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), Vec2::splat(20.0));
                let r = drop_menu(ui.ctx(), &p, id, anchor, false, &items);
                if let MenuResult::Clicked(i) = r {
                    *out.borrow_mut() = Some(i);
                }
            });
        h.run();
        // Focus starts on the first item; Down moves to the second; Enter picks it.
        h.key_press(egui::Key::ArrowDown);
        h.run();
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(*seen.borrow(), Some(1));
    }

    #[test]
    fn escape_closes_a_drop_menu_and_is_used_up() {
        let p = theme::LIGHT;
        let id = egui::Id::unique("menu");
        let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
        let out = seen.clone();
        let mut h = Harness::builder()
            .with_size([400.0, 300.0])
            .with_context_setup(theme::install_fonts)
            .build_ui(move |ui| {
                let items = [MenuItem::plain("One")];
                let anchor = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), Vec2::splat(20.0));
                let r = drop_menu(ui.ctx(), &p, id, anchor, false, &items);
                if matches!(r, MenuResult::Closed) {
                    let left = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
                    *out.borrow_mut() = Some(left);
                }
            });
        h.run();
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(*seen.borrow(), Some(false));
    }

    fn gallery(p: Palette) -> Harness<'static> {
        let mut harness = Harness::builder()
            .with_size([560.0, 290.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui(move |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0, p.app);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    Button::new("Cancel").kind(Kind::Ghost).show(ui, &p);
                    Button::new("Recompile")
                        .with_icon(icons::REFRESH_CW)
                        .kind(Kind::Primary)
                        .show(ui, &p);
                    Button::new("Delete")
                        .kind(Kind::Danger)
                        .size(Size::Sm)
                        .show(ui, &p);
                    Button::new("Disabled").enabled(false).show(ui, &p);
                    Button::icon(icons::SETTINGS, "Settings").show_icon(ui, &p);
                });
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    segmented(
                        ui,
                        &p,
                        &[
                            (Some(icons::SQUARE_PEN), "Editor"),
                            (Some(icons::COLUMNS_2), "Split"),
                            (Some(icons::FILE_TEXT), "PDF"),
                        ],
                        1,
                        false,
                    );
                    kbd(ui, &p, "Ctrl+Enter");
                });
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let mut sw_on = true;
                    let mut sw_off = false;
                    switch(ui, &p, &mut sw_on, "Auto compile");
                    switch(ui, &p, &mut sw_off, "Dark PDF");
                    chip(ui, &p, Some(icons::CHECK), "Compiled", ChipTone::Ok);
                    chip_button(
                        ui,
                        &p,
                        Some(icons::TRIANGLE_ALERT),
                        "4 issues",
                        "Problems",
                        ChipTone::Warn,
                    );
                    chip(ui, &p, Some(icons::CIRCLE_X), "1", ChipTone::Error);
                });
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    logo_mark(ui, 32.0);
                    rail_button(ui, &p, icons::FILES, "Files", true, true, None);
                    rail_button(
                        ui,
                        &p,
                        icons::CIRCLE_ALERT,
                        "Problems",
                        false,
                        true,
                        Some((1, ChipTone::Error)),
                    );
                    rail_button(
                        ui,
                        &p,
                        icons::GIT_BRANCH,
                        "Git",
                        true,
                        true,
                        Some((3, ChipTone::Accent)),
                    );
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        side_row(
                            ui,
                            &p,
                            Some(icons::FILE_TEXT),
                            None,
                            "method.tex",
                            true,
                            None,
                        );
                        side_row(
                            ui,
                            &p,
                            Some(icons::FOLDER),
                            Some(p.atom),
                            "chapters",
                            false,
                            Some("3"),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    label(ui, &p, "Main file");
                });
            });
        harness.run();
        harness
    }

    #[test]
    fn buttons_have_accessible_names_and_toggle_state() {
        let harness = gallery(theme::LIGHT);
        harness.get_by_label("Recompile");
        harness.get_by_label("Settings");
        use eframe::egui::accesskit::Toggled;
        assert_eq!(
            harness.get_by_label("Cancel").accesskit_node().toggled(),
            None,
            "an ordinary button is not a toggle"
        );
        assert_eq!(
            harness.get_by_label("Git").accesskit_node().toggled(),
            Some(Toggled::True)
        );
        for split in harness.get_all_by_label("Split") {
            assert_eq!(split.accesskit_node().toggled(), Some(Toggled::True));
        }
    }

    #[test]
    fn modal_has_dialog_semantics_and_restores_focus() {
        let p = theme::LIGHT;
        let mut open = false;
        let mut initially_focused = false;
        let mut h = egui_kittest::Harness::builder()
            .with_size([640.0, 480.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .build_ui(move |ui| {
                let background = Button::new("Background").show(ui, &p);
                if !initially_focused {
                    background.request_focus();
                    initially_focused = true;
                }
                if background.clicked() {
                    open = true;
                }
                if open {
                    let response = modal(
                        ui.ctx(),
                        &p,
                        "Example dialog",
                        400.0,
                        |ui| {
                            Button::new("Inside").show(ui, &p);
                        },
                        None,
                    );
                    if response.closed {
                        open = false;
                    }
                }
            });
        let expected_focus = h.ctx.memory(|m| m.focused());
        h.get_by_label("Background").click();
        h.run();
        h.get_by_role_and_label(egui::Role::Dialog, "Example dialog");
        for _ in 0..8 {
            h.key_press(egui::Key::Tab);
            h.run();
            assert!(
                !h.get_by_label("Background").is_focused(),
                "Tab escaped into the background"
            );
        }
        h.key_press(egui::Key::Escape);
        h.run();
        h.run();
        assert_eq!(h.ctx.memory(|m| m.focused()), expected_focus);
    }

    #[test]
    fn a_click_beside_a_dialog_with_edits_keeps_it_open() {
        for (editing, closes) in [(true, false), (false, true)] {
            let p = theme::LIGHT;
            let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = closed.clone();
            let mut h = egui_kittest::Harness::builder()
                .with_size([640.0, 480.0])
                .with_context_setup(move |ctx| {
                    theme::install_fonts(ctx);
                    theme::apply(ctx, &p);
                })
                .build_ui(move |ui| {
                    let r = if editing {
                        modal_editing(ui.ctx(), &p, "Edit", None, 300.0, |_| {}, None)
                    } else {
                        modal(ui.ctx(), &p, "Edit", 300.0, |_| {}, None)
                    };
                    if r.closed {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            h.run();
            let pos = egui::pos2(5.0, 5.0);
            h.hover_at(pos);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            h.run();
            assert_eq!(closed.load(std::sync::atomic::Ordering::Relaxed), closes);
        }
    }

    #[test]
    fn escape_closes_only_the_top_modal() {
        let p = theme::LIGHT;
        let depth = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
        let shown = depth.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size([640.0, 480.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .build_ui(move |ui| {
                for (level, title) in [(1, "Outer"), (2, "Inner")] {
                    if shown.load(std::sync::atomic::Ordering::Relaxed) < level {
                        continue;
                    }
                    let response = modal(ui.ctx(), &p, title, 400.0, |_| {}, None);
                    if response.closed {
                        shown.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });
        h.run();
        depth.store(2, std::sync::atomic::Ordering::Relaxed);
        h.run();
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(depth.load(std::sync::atomic::Ordering::Relaxed), 1);
        h.get_by_role_and_label(egui::Role::Dialog, "Outer");
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(depth.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn gallery_light() {
        gallery(theme::LIGHT).snapshot("gallery_light");
    }

    #[test]
    fn gallery_dark() {
        gallery(theme::DARK).snapshot("gallery_dark");
    }
}
