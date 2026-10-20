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
            let painter = ui.painter();
            painter.rect(
                rect,
                self.radius.unwrap_or(CornerRadius::same(theme::R)),
                fill,
                Stroke::new(1.0, stroke),
                StrokeKind::Inside,
            );
            focus_ring(ui, &response, rect, p, theme::R);

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
    widths + 2.0 * (options.len() as f32 - 1.0) + 4.0
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
        w.hovered.bg_stroke = Stroke::new(1.0, p.faint);
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
            // In a horizontal row a Label would not wrap at all, so it gets a box of its own.
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
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

/// What a modal reports back each frame.
pub struct ModalResponse<R> {
    pub inner: R,
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
) -> ModalResponse<R> {
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
) -> ModalResponse<R> {
    let screen = ctx.content_rect();
    let mut closed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
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
    let panel = egui::Area::new(egui::Id::unique(("modal", title)))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_CENTER)
        .fixed_pos(screen.center())
        .show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(12)
                .shadow(p.shadow_lg)
                .show(ui, |ui| {
                    // Inside the 1 px border; the header, body and footer frames add their own padding.
                    ui.set_width(width - 2.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: 20,
                            right: 20,
                            top: 18,
                            bottom: 4,
                        })
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(ui.available_width() - 26.0 - 12.0);
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
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Min),
                                    |ui| {
                                        if Button::icon(icons::X, "Close")
                                            .size(Size::Sm)
                                            .show_icon(ui, p)
                                            .clicked()
                                        {
                                            closed = true;
                                        }
                                    },
                                );
                            });
                        });
                    let inner = egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: 20,
                            right: 20,
                            top: 12,
                            bottom: 18,
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 14.0;
                            body(ui)
                        })
                        .inner;
                    if let Some(footer) = footer {
                        let r = egui::Frame::new()
                            .fill(p.surface_2)
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
                        let top = r.response.rect.top();
                        ui.painter().hline(
                            r.response.rect.x_range(),
                            top,
                            Stroke::new(1.0, p.border),
                        );
                    }
                    inner
                })
                .inner
        });
    if backdrop.inner.clicked()
        && !panel
            .response
            .rect
            .contains(backdrop.inner.interact_pointer_pos().unwrap_or_default())
    {
        closed = true;
    }
    ModalResponse {
        inner: panel.inner,
        closed,
    }
}

/// A row of a drop menu.
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
    let size: Vec2 = ctx
        .data(|d| d.get_temp(size_id))
        .unwrap_or(Vec2::new(220.0, 200.0));
    let left = if align_end {
        anchor.right() - size.x
    } else {
        anchor.left()
    };
    let top = if anchor.bottom() + size.y + 6.0 > screen.bottom() {
        anchor.top() - size.y - 6.0
    } else {
        anchor.bottom() + 6.0
    };
    let pos = egui::pos2(
        left.clamp(8.0, (screen.right() - size.x - 8.0).max(8.0)),
        top.max(8.0),
    );
    let mut result = MenuResult::Open;
    let font = FontId::proportional(13.0);
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
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
                        .fold(190.0f32, f32::max);
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
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::new(row_w, 22.0), Sense::hover());
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
                });
        });
    ctx.data_mut(|d| d.insert_temp(size_id, area.response.rect.size()));
    if matches!(result, MenuResult::Open) {
        let away = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p))
        });
        if away || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = MenuResult::Closed;
        }
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
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Splitter, true, label));
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
