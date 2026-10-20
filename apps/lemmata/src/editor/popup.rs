//! The completion list (CodeMirror's `.cm-tooltip-autocomplete` with Editor.svelte's theme): a
//! type icon, the label with the typed letters underlined, a rendered maths glyph, the detail in
//! italics, and an info panel beside the list for the selected item.

use super::complete::{Item, Kind};
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    pos2, text::LayoutJob, vec2,
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

/// `.cm-completionIcon`: ƒ, ○, 𝐶, abc, ▢ and a key, drawn so they don't depend on fonts.
fn icon(ui: &Ui, rect: Rect, kind: Kind, color: Color32) {
    let p = ui.painter();
    let c = rect.center();
    let s = Stroke::new(1.0, color);
    match kind {
        Kind::Function => {
            p.text(
                c,
                Align2::CENTER_CENTER,
                "ƒ",
                FontId::proportional(11.7),
                color,
            );
        }
        Kind::Class => {
            p.circle_stroke(c, 3.6, s);
        }
        Kind::Constant => {
            let mut job = LayoutJob::default();
            job.append(
                "C",
                0.0,
                egui::TextFormat {
                    font_id: FontId::proportional(11.7),
                    color,
                    italics: true,
                    ..Default::default()
                },
            );
            let g = ui.fonts_mut(|f| f.layout_job(job));
            p.galley(c - g.size() / 2.0, g, color);
        }
        Kind::Text => {
            p.text(
                c,
                Align2::CENTER_CENTER,
                "abc",
                FontId::proportional(6.5),
                color,
            );
        }
        Kind::Namespace => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(7.0, 7.0)),
                1,
                s,
                StrokeKind::Inside,
            );
        }
        Kind::Keyword => {
            // A small key.
            p.circle_stroke(c + vec2(-2.0, -1.5), 2.2, s);
            p.line_segment([c + vec2(-0.5, 0.0), c + vec2(3.5, 4.0)], s);
            p.line_segment([c + vec2(2.0, 2.5), c + vec2(3.2, 1.3)], s);
        }
        Kind::File => {}
    }
}

/// What the user did with the list this frame.
pub enum Picked {
    None,
    Item(usize),
}

/// Draws the list under `anchor` (the start of the completed word on its line, `line_h` tall);
/// `glyph` renders an item's maths preview, `info` the side panel.
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
    let row_h = (size * 1.2 + 6.0).round();
    let rows = c.items.len().min(ROWS);
    let screen = ctx.content_rect();
    let list_h = rows as f32 * row_h + 2.0;
    // Below the line, or above it when there is no room.
    let below = anchor.y + line_h + 2.0 + list_h <= screen.bottom();
    let top_left = if below {
        pos2(anchor.x - 4.0, anchor.y + line_h + 2.0)
    } else {
        pos2(anchor.x - 4.0, anchor.y - list_h - 2.0)
    };
    let mono = FontId::monospace(size);
    let mut list_rect = Rect::NOTHING;
    egui::Area::new(egui::Id::unique("completion"))
        .order(egui::Order::Tooltip)
        .fixed_pos(top_left)
        .show(ctx, |ui| {
            // Width: the widest of the rows shown, between 250 and 700 px.
            let width = c.items[c.top..(c.top + rows).min(c.items.len())]
                .iter()
                .map(|(it, _)| {
                    let label = it.display.as_deref().unwrap_or(&it.label);
                    let w = ui.fonts_mut(|f| {
                        f.layout_no_wrap(label.into(), mono.clone(), p.ink).size().x
                    });
                    let d = it.detail.as_deref().map_or(0.0, |d| {
                        ui.fonts_mut(|f| f.layout_no_wrap(d.into(), mono.clone(), p.ink).size().x)
                            + size * 0.5
                    });
                    let g = if it.preview.is_some() {
                        12.0 + 40.0
                    } else {
                        0.0
                    };
                    20.0 + size * 1.4 + w + g + d
                })
                .fold(250.0f32, f32::max)
                .min(700.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width, list_h), Sense::hover());
            list_rect = rect;
            let painter = ui.painter();
            painter.add(p.shadow.as_shape(rect, CornerRadius::same(8)));
            painter.rect(
                rect,
                CornerRadius::same(8),
                p.surface,
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
            let clip = rect.shrink(1.0);
            for (k, i) in (c.top..(c.top + rows).min(c.items.len())).enumerate() {
                let (it, hits) = &c.items[i];
                let r = Rect::from_min_size(
                    pos2(rect.left() + 1.0, rect.top() + 1.0 + k as f32 * row_h),
                    vec2(rect.width() - 2.0, row_h),
                );
                let resp = ui.interact(r, egui::Id::unique(("completion-row", i)), Sense::click());
                if i == c.selected {
                    ui.painter().with_clip_rect(clip).rect_filled(
                        r,
                        if k == 0 { 6 } else { 0 },
                        p.accent_soft,
                    );
                }
                if resp.clicked() {
                    picked = Picked::Item(i);
                }
                let mut x = r.left() + 10.0;
                let icon_w = size * 0.9 * 0.8;
                icon(
                    ui,
                    Rect::from_min_size(pos2(x, r.top()), vec2(icon_w, row_h)),
                    it.kind,
                    p.ink.gamma_multiply(0.6),
                );
                x += icon_w + size * 0.9 * 0.6;
                // The label, typed letters underlined (.cm-completionMatchedText).
                let label = it.display.as_deref().unwrap_or(&it.label);
                let mut job = LayoutJob::default();
                for (ci, ch) in label.chars().enumerate() {
                    let mut f = egui::TextFormat {
                        font_id: mono.clone(),
                        color: p.ink,
                        ..Default::default()
                    };
                    if hits.contains(&ci) {
                        f.underline = Stroke::new(1.0, p.ink);
                    }
                    job.append(&ch.to_string(), 0.0, f);
                }
                let g = ui.fonts_mut(|f| f.layout_job(job));
                let gy = r.center().y - g.size().y / 2.0;
                let gw = g.size().x;
                ui.painter()
                    .with_clip_rect(clip)
                    .galley(pos2(x, gy), g, p.ink);
                x += gw;
                if let Some(tex) = &it.preview {
                    x += 12.0;
                    x += glyph(ui, tex, pos2(x, r.center().y), size * 0.95);
                }
                if let Some(d) = &it.detail {
                    let mut job = LayoutJob::default();
                    job.append(
                        d,
                        size * 0.5,
                        egui::TextFormat {
                            font_id: mono.clone(),
                            color: p.ink,
                            italics: true,
                            ..Default::default()
                        },
                    );
                    let g = ui.fonts_mut(|f| f.layout_job(job));
                    ui.painter().with_clip_rect(clip).galley(
                        pos2(x, r.center().y - g.size().y / 2.0),
                        g,
                        p.ink,
                    );
                }
            }
            // A thin scrollbar when there are more rows than fit.
            if c.items.len() > rows {
                let track = Rect::from_min_max(
                    pos2(rect.right() - 5.0, rect.top() + 2.0),
                    pos2(rect.right() - 2.0, rect.bottom() - 2.0),
                );
                let frac = rows as f32 / c.items.len() as f32;
                let h = (track.height() * frac).max(12.0);
                let y = track.top()
                    + (track.height() - h) * (c.top as f32 / (c.items.len() - rows) as f32);
                ui.painter().rect_filled(
                    Rect::from_min_size(pos2(track.left(), y), vec2(3.0, h)),
                    2,
                    p.faint.gamma_multiply(0.6),
                );
            }
            // The wheel scrolls the rows.
            if ui.rect_contains_pointer(rect) {
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                if dy.abs() > 0.5 {
                    let max = c.items.len().saturating_sub(rows);
                    let steps = (-dy / row_h).round() as isize;
                    c.top = (c.top as isize + steps).clamp(0, max as isize) as usize;
                }
            }
        });

    // .cm-completionInfo beside the list, for the selected item.
    if let Some((it, _)) = c.items.get(c.selected)
        && (it.info.is_some() || it.preview.is_some())
    {
        let right = list_rect.right() + 4.0 + 300.0 <= screen.right();
        let at = if right {
            pos2(list_rect.right() + 4.0, list_rect.top())
        } else {
            pos2(list_rect.left() - 4.0, list_rect.top())
        };
        egui::Area::new(egui::Id::unique("completion-info"))
            .order(egui::Order::Tooltip)
            .fixed_pos(at)
            .pivot(if right {
                Align2::LEFT_TOP
            } else {
                Align2::RIGHT_TOP
            })
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(theme::R)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(300.0);
                        info(ui, it);
                    });
            });
    }
    let _ = Shape::Noop;
    picked
}
