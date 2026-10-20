//! The controls of `app.css`: `.btn` and its variants, `.segmented`, `.badge`, `kbd`, `.label`.

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Default,
    Primary,
    Ghost,
    Danger,
    /// A button on the dark toolbar or sidebar (`.chrome .btn`).
    Chrome,
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
    enabled: bool,
    trailing: Option<Icon>,
    ink: Option<Color32>,
}

impl<'a> Button<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            icon: None,
            kind: Kind::Default,
            size: Size::Md,
            on: false,
            enabled: true,
            trailing: None,
            ink: None,
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
    /// `.chrome .btn.on`: a toggled toolbar button.
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
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
        let icon_size = 16.0;
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
        };
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
        response.widget_info(|| {
            WidgetInfo::selected(egui::Role::Button, self.enabled, self.on, self.text)
        });

        if ui.is_rect_visible(rect) {
            let hovered = self.enabled && response.hovered();
            let t = ui
                .ctx()
                .animate_bool_with_time(response.id, hovered, theme::EASE_SECS);
            let lerp = |a: Color32, b: Color32| a.lerp_to_gamma(b, t);
            let (fill, stroke, ink) = match self.kind {
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
                Kind::Ghost => (
                    lerp(Color32::TRANSPARENT, p.surface_3),
                    Color32::TRANSPARENT,
                    p.ink,
                ),
                Kind::Danger => (
                    lerp(p.surface, p.surface_2),
                    lerp(p.border_strong, p.faint),
                    p.error,
                ),
                Kind::Chrome if self.on => {
                    (p.chrome_active, Color32::TRANSPARENT, p.accent_on_dark)
                }
                Kind::Chrome => (
                    lerp(Color32::TRANSPARENT, p.chrome_hover),
                    Color32::TRANSPARENT,
                    p.chrome_ink,
                ),
            };
            let ink = self.ink.unwrap_or(ink);
            let alpha = if self.enabled { 1.0 } else { 0.5 };
            let (fill, stroke, ink) = (
                fill.gamma_multiply(alpha),
                stroke.gamma_multiply(alpha),
                ink.gamma_multiply(alpha),
            );
            let painter = ui.painter();
            painter.rect(
                rect,
                CornerRadius::same(theme::R),
                fill,
                Stroke::new(1.0, stroke),
                StrokeKind::Inside,
            );
            focus_ring(ui, &response, rect, p, theme::R);

            let mut x = rect.left()
                + if icon_only {
                    (width - icon_size) / 2.0
                } else {
                    pad
                };
            if let Some(icon) = self.icon {
                let r = egui::Rect::from_min_size(
                    egui::pos2(x, rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                );
                icons::paint(painter, r, icon, ink);
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

/// `.segmented`: a track with one pressed option. Returns the clicked index, if any. With `icon_only`
/// the labels are only the options' accessible names, as the toolbar's layout switch does.
pub fn segmented(
    ui: &mut Ui,
    p: &Palette,
    options: &[(Option<Icon>, &str)],
    selected: usize,
    chrome: bool,
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
    let total = widths.iter().sum::<f32>() + 2.0 * (options.len() as f32 - 1.0) + 4.0;
    let (track, _) = ui.allocate_exact_size(Vec2::new(total, 30.0), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(
        track,
        CornerRadius::same(theme::R),
        if chrome { p.chrome_3 } else { p.surface_3 },
    );
    let mut clicked = None;
    let mut x = track.left() + 2.0;
    for (i, (((icon, text), galley), w)) in options.iter().zip(galleys).zip(&widths).enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(x, track.top() + 2.0), Vec2::new(*w, 26.0));
        let id = ui.scope_id().with(("segmented", i, *text));
        let response = ui.interact(rect, id, Sense::click());
        response
            .widget_info(|| WidgetInfo::selected(egui::Role::Button, true, i == selected, *text));
        let on = i == selected;
        let ink = match (chrome, on) {
            (true, true) => p.accent_on_dark,
            (true, false) => p.chrome_muted,
            (false, true) => p.ink,
            (false, false) => p.muted,
        };
        if on {
            if chrome {
                painter.rect_filled(rect, CornerRadius::same(theme::R_SM), p.chrome_active);
            } else {
                painter.add(theme::SHADOW_SM.as_shape(rect, CornerRadius::same(theme::R_SM)));
                painter.rect_filled(rect, CornerRadius::same(theme::R_SM), p.surface);
            }
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
        focus_ring(ui, &response, rect, p, theme::R_SM);
        if response.clicked() {
            clicked = Some(i);
        }
        x += w + 2.0;
    }
    clicked
}

#[derive(Clone, Copy)]
pub enum Tone {
    Error,
    Warn,
    Neutral,
}

/// `.badge`: an 18 px pill with a count or a short word.
pub fn badge(ui: &mut Ui, p: &Palette, text: &str, tone: Tone) -> Response {
    let font = FontId::new(11.0, theme::font::sans_600());
    let (fill, ink) = match tone {
        Tone::Error => (p.error, Color32::WHITE),
        Tone::Warn => (p.warn, Color32::WHITE),
        Tone::Neutral => (p.surface_3, p.muted),
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, ink);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 12.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, ink);
    response
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
        w.hovered.bg_stroke = Stroke::new(1.0, p.faint);
        let r = egui::ComboBox::from_id_salt(name)
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
    /// Underlines the last part added, offset like `text-underline-offset: 3px` (as close as epaint gets).
    pub fn underline(mut self) -> Self {
        if let Some(sec) = self.0.sections.last_mut() {
            sec.format.underline = Stroke::new(1.0, sec.format.color);
        }
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
    pub fn show(self, ui: &mut Ui) -> Response {
        // A Label wraps at the available width, so the job's own width becomes the Ui's.
        let width = self.0.wrap.max_width;
        if width.is_finite() {
            ui.scope(|ui| {
                ui.set_max_width(width);
                ui.add(egui::Label::new(self.0).selectable(false))
            })
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

/// A raised surface: `--surface` fill, `--border`, a radius and `--shadow`.
pub fn card<R>(
    ui: &mut Ui,
    p: &Palette,
    radius: u8,
    padding: egui::Margin,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius)
        .shadow(p.shadow)
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
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
        .hint_text(egui::RichText::new(hint).color(p.faint))
        .font(font)
        .text_color(p.ink)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::symmetric(10, 0))
        .align(egui::Align2::LEFT_CENTER)
        .min_size(Vec2::new(0.0, 32.0))
        .desired_width(ui.available_width())
        .show(ui);
    let response = out.response.response;
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    fn gallery(p: Palette) -> Harness<'static> {
        let mut harness = Harness::builder()
            .with_size([520.0, 220.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui(move |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0, p.app);
                ui.horizontal(|ui| {
                    Button::new("Cancel").show(ui, &p);
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
                    segmented(
                        ui,
                        &p,
                        &[
                            (Some(icons::PANEL_LEFT), "Editor"),
                            (Some(icons::COLUMNS_2), "Split"),
                            (None, "PDF"),
                        ],
                        1,
                        false,
                        false,
                    );
                    badge(ui, &p, "3", Tone::Error);
                    badge(ui, &p, "12", Tone::Warn);
                    badge(ui, &p, "draft", Tone::Neutral);
                    kbd(ui, &p, "Ctrl+Enter");
                });
                label(ui, &p, "Main file");
                egui::Frame::new()
                    .fill(p.chrome)
                    .inner_margin(6)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            Button::icon(icons::MENU, "Menu")
                                .kind(Kind::Chrome)
                                .show_icon(ui, &p);
                            Button::icon(icons::GIT_BRANCH, "Git")
                                .kind(Kind::Chrome)
                                .on(true)
                                .show_icon(ui, &p);
                            segmented(
                                ui,
                                &p,
                                &[
                                    (Some(icons::PANEL_LEFT), "Editor"),
                                    (Some(icons::COLUMNS_2), "Split"),
                                    (Some(icons::FILE_TEXT), "PDF"),
                                ],
                                1,
                                true,
                                true,
                            );
                        });
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
            harness.get_by_label("Git").accesskit_node().toggled(),
            Some(Toggled::True)
        );
        for split in harness.get_all_by_label("Split") {
            assert_eq!(split.accesskit_node().toggled(), Some(Toggled::True));
        }
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
