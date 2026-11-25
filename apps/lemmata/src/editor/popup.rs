//! The completion list (CodeMirror's `.cm-tooltip-autocomplete` with Editor.svelte's theme): a
//! type icon, the label with the typed letters underlined, a rendered maths glyph, the detail in
//! italics, and an info panel beside the list for the selected item.

use super::complete::{Item, Kind};
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, text::LayoutJob, vec2,
};

/// An open completion list.
pub struct Completion {
    pub items: Vec<(Item, Vec<usize>)>,
    pub selected: usize,
    pub explicit: bool,
    /// What it was computed for: document version, cursor, lookups generation.
    pub key: (u64, usize, u64),
    /// The first row shown.
    pub top: usize,
    /// Wheel pixels not yet worth a whole row: smooth and touchpad scrolling come a few at a time.
    pub scrolled: f32,
    /// Where the list was drawn last frame.
    pub rect: Rect,
}

const ROWS: usize = 6;

impl Completion {
    pub fn step(&mut self, by: isize) {
        let n = self.items.len() as isize;
        if n == 0 {
            return;
        }
        // CodeMirror wraps around at the ends for single steps, stops for pages.
        let next = self.selected as isize + by;
        self.selected = if by.abs() == 1 {
            next.rem_euclid(n)
        } else {
            next.clamp(0, n - 1)
        } as usize;
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + ROWS {
            self.top = self.selected + 1 - ROWS;
        }
    }
}

/// 18×18 rounded kind badge matching Screen 6 (`.comp-item .k`).
fn kind_badge(ui: &Ui, p: &Palette, rect: Rect, item: &Item) {
    let (bg, fg, glyph) = match item.kind {
        Kind::Function => (p.surface_3, p.muted, "\\"),
        Kind::Class => (p.ok_soft, p.ok, "{}"),
        Kind::Constant => (p.warn_soft, p.warn, "#"),
        Kind::Keyword => (p.accent_soft, p.accent_text, "Σ"),
        Kind::Namespace => (p.surface_3, p.muted, "[]"),
        Kind::Text => {
            if matches!(item.apply, super::complete::Apply::Text(_)) {
                (p.accent_soft, p.accent_text, "@")
            } else {
                (p.surface_3, p.muted, "ab")
            }
        }
        Kind::File => (p.surface_3, p.muted, "/"),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(theme::R_SM), bg);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(10.0, theme::font::sans_600()),
        fg,
    );
}

/// What the user did with the list this frame.
pub enum Picked {
    None,
    Item(usize),
}

/// Draws the list under `anchor` (the start of the completed word on its line, `line_h` tall);
/// `glyph` renders an item's maths preview, `info` renders the documentation footer or side card.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ctx: &egui::Context,
    p: &Palette,
    font: &FontId,
    anchor: Pos2,
    line_h: f32,
    c: &mut Completion,
    glyph: &mut dyn FnMut(&Ui, &str, Pos2, f32) -> f32,
    info: &mut dyn FnMut(&mut Ui, &Item),
) -> Picked {
    let mut picked = Picked::None;
    let size = font.size;
    let row_h = (size * 1.2 + 12.0).round().max(28.0);
    let rows = c.items.len().min(ROWS);
    let screen = ctx.content_rect();
    let list_pad = 4.0;
    let list_h = rows as f32 * row_h + list_pad * 2.0 + 2.0;
    let has_info = c
        .items
        .get(c.selected)
        .is_some_and(|(it, _)| it.info.is_some() || it.preview.is_some());
    let est_h = list_h + if has_info { 42.0 } else { 0.0 };
    // Below the line, or above it when there is no room.
    let below = anchor.y + line_h + 4.0 + est_h <= screen.bottom();
    let top_left = if below {
        pos2(anchor.x - 4.0, anchor.y + line_h + 4.0)
    } else {
        pos2(anchor.x - 4.0, anchor.y - est_h - 4.0)
    };
    let mono = FontId::monospace(size);
    let mono_bold = FontId::new(size, theme::font::mono_500());
    let det_font = FontId::proportional(12.0);
    let mut list_rect = Rect::NOTHING;
    let selected_item = c.items.get(c.selected).map(|(it, _)| it.clone());
    egui::Area::new(egui::Id::unique("completion"))
        .order(egui::Order::Tooltip)
        .fixed_pos(top_left)
        .constrain_to(screen.shrink(4.0))
        .show(ctx, |ui| {
            // Width: widest of the shown rows, between 280 and 480 px (clamped to viewport).
            let max_w = (screen.width() - 12.0).clamp(180.0, 480.0);
            let width = c.items[c.top..(c.top + rows).min(c.items.len())]
                .iter()
                .map(|(it, _)| {
                    let label = it.display.as_deref().unwrap_or(&it.label);
                    let w = ui.fonts_mut(|f| {
                        f.layout_no_wrap(label.into(), mono.clone(), p.ink).size().x
                    });
                    let d = it.detail.as_deref().map_or(0.0, |d| {
                        ui.fonts_mut(|f| {
                            f.layout_no_wrap(d.into(), det_font.clone(), p.faint)
                                .size()
                                .x
                        }) + 16.0
                    });
                    let g = if it.preview.is_some() { 48.0 } else { 0.0 };
                    48.0 + w + g + d
                })
                .fold(280.0f32.min(max_w), f32::max)
                .min(max_w);
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(theme::R_LG)
                .shadow(p.shadow)
                .show(ui, |ui| {
                    ui.set_width(width - 2.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (rect, list) = ui.allocate_exact_size(
                        vec2(width - 2.0, rows as f32 * row_h + list_pad * 2.0),
                        Sense::hover(),
                    );
                    list.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::ListBox, true, "Suggestions")
                    });
                    list_rect = rect;
                    let clip = rect;
                    for (k, i) in (c.top..(c.top + rows).min(c.items.len())).enumerate() {
                        let (it, hits) = &c.items[i];
                        let r = Rect::from_min_size(
                            pos2(
                                rect.left() + list_pad,
                                rect.top() + list_pad + k as f32 * row_h,
                            ),
                            vec2(rect.width() - list_pad * 2.0, row_h),
                        );
                        let resp =
                            ui.interact(r, egui::Id::unique(("completion-row", i)), Sense::click());
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::ListBoxOption,
                                true,
                                it.display.as_deref().unwrap_or(&it.label),
                            )
                        });
                        let selected = i == c.selected;
                        if selected {
                            ui.painter().with_clip_rect(clip).rect_filled(
                                r,
                                CornerRadius::same(theme::R),
                                p.accent_soft,
                            );
                        }
                        if resp.clicked() {
                            picked = Picked::Item(i);
                        }
                        let mut x = r.left() + 8.0;
                        let badge_rect =
                            Rect::from_min_size(pos2(x, r.center().y - 9.0), vec2(18.0, 18.0));
                        kind_badge(ui, p, badge_rect, it);
                        x = badge_rect.right() + 8.0;
                        let label = it.display.as_deref().unwrap_or(&it.label);
                        let base_ink = if selected { p.ink } else { p.ink_2 };
                        let mut job = LayoutJob::default();
                        for (ci, ch) in label.chars().enumerate() {
                            let matched = hits.contains(&ci);
                            let f = egui::TextFormat {
                                font_id: if matched {
                                    mono_bold.clone()
                                } else {
                                    mono.clone()
                                },
                                color: if matched { p.accent_text } else { base_ink },
                                ..Default::default()
                            };
                            job.append(&ch.to_string(), 0.0, f);
                        }
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        let gy = r.center().y - g.size().y / 2.0;
                        let gw = g.size().x;
                        let right_edge = r.right() - 8.0;
                        let detail_g = it.detail.as_deref().map(|d| {
                            ui.fonts_mut(|f| f.layout_no_wrap(d.into(), det_font.clone(), p.faint))
                        });
                        let det_w = detail_g
                            .as_ref()
                            .map_or(0.0, |dg| dg.size().x.min(r.width() * 0.4) + 10.0);
                        let label_clip = Rect::from_min_max(
                            pos2(x, r.top()),
                            pos2((right_edge - det_w).max(x + 20.0), r.bottom()),
                        );
                        ui.painter()
                            .with_clip_rect(label_clip)
                            .galley(pos2(x, gy), g, base_ink);
                        x += gw;
                        if let Some(tex) = &it.preview
                            && x + 10.0 < right_edge - det_w
                        {
                            x += 10.0;
                            let _ = glyph(ui, tex, pos2(x, r.center().y), size * 0.95);
                        }
                        if let Some(dg) = detail_g {
                            let dw = dg.size().x.min(r.width() * 0.4);
                            let dclip = Rect::from_min_max(
                                pos2(right_edge - dw, r.top()),
                                pos2(right_edge, r.bottom()),
                            );
                            ui.painter().with_clip_rect(dclip).galley(
                                pos2(
                                    right_edge - dg.size().x.min(dw),
                                    r.center().y - dg.size().y / 2.0,
                                ),
                                dg,
                                p.faint,
                            );
                        }
                    }
                    // Thin scrollbar when more rows exist.
                    if c.items.len() > rows {
                        let track = Rect::from_min_max(
                            pos2(rect.right() - 4.0, rect.top() + 4.0),
                            pos2(rect.right() - 1.0, rect.bottom() - 4.0),
                        );
                        let frac = rows as f32 / c.items.len() as f32;
                        let h = (track.height() * frac).max(12.0);
                        let grab = ui.interact(
                            track.with_max_x(rect.right()),
                            ui.scope_id().with("completion-bar"),
                            Sense::click_and_drag(),
                        );
                        if let Some(at) = grab.interact_pointer_pos()
                            && (grab.dragged() || grab.clicked())
                        {
                            let span = (track.height() - h).max(1.0);
                            let t = ((at.y - track.top() - h / 2.0) / span).clamp(0.0, 1.0);
                            c.top = (t * (c.items.len() - rows) as f32).round() as usize;
                        }
                        let y = track.top()
                            + (track.height() - h) * (c.top as f32 / (c.items.len() - rows) as f32);
                        ui.painter().rect_filled(
                            Rect::from_min_size(pos2(track.left(), y), vec2(3.0, h)),
                            2,
                            p.faint.gamma_multiply(0.55),
                        );
                    }
                    // Wheel scrolling.
                    if ui.rect_contains_pointer(rect) {
                        c.scrolled -= ui.input(|i| i.smooth_scroll_delta.y);
                        let steps = (c.scrolled / row_h).trunc();
                        c.scrolled -= steps * row_h;
                        let max = c.items.len().saturating_sub(rows);
                        c.top = (c.top as isize + steps as isize).clamp(0, max as isize) as usize;
                    }
                    // Integrated documentation footer for the selected item (`.comp-doc`).
                    if let Some(it) = &selected_item
                        && (it.info.is_some() || it.preview.is_some())
                    {
                        let y = ui.cursor().top();
                        ui.painter()
                            .hline(rect.x_range(), y + 0.5, Stroke::new(1.0, p.border));
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.set_width((width - 26.0).max(120.0));
                                info(ui, it);
                            });
                    }
                });
        });
    c.rect = list_rect;
    picked
}
