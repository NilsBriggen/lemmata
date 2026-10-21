//! SymbolPalette.svelte: maths symbols rendered with the maths renderer; a click inserts one,
//! Shift-click keeps the palette open.

use crate::theme::{self, Palette};
use crate::widgets;
use crate::writing::{render, symbols};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

#[derive(Default)]
pub struct SymbolPalette {
    query: String,
    group: String,
}

/// What the user picked.
pub enum Picked {
    None,
    Close,
    /// The symbol's source, the package it needs, and whether to stay open.
    Insert(String, Option<&'static str>, bool),
}

impl SymbolPalette {
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette) -> Picked {
        let mut picked = Picked::None;
        if self.group.is_empty() {
            self.group = "All".into();
        }
        let r = widgets::modal_placed(
            ctx,
            p,
            "Symbols",
            Some("Click to insert. Hold Shift to keep this open."),
            640.0,
            true,
            |ui| self.body(ui, p, &mut picked),
            None,
        );
        if r.closed {
            return Picked::Close;
        }
        picked
    }

    fn body(&mut self, ui: &mut Ui, p: &Palette, picked: &mut Picked) {
        // .bar: the search box and the category, 160 px.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let w = ui.available_width() - 160.0 - 8.0;
            ui.allocate_ui(vec2(w, 32.0), |ui| {
                let r =
                    widgets::text_input(ui, p, &mut self.query, "Search by command name", false);
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Search by command name"));
            });
            let mut options: Vec<(String, String)> = vec![("All".into(), "All".into())];
            options.extend(
                symbols::groups()
                    .into_iter()
                    .map(|g| (g.to_owned(), g.to_owned())),
            );
            let opts: Vec<(&str, &str)> = options
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            ui.allocate_ui(vec2(160.0, 32.0), |ui| {
                widgets::select(ui, p, "Category", &mut self.group, &opts)
            });
        });
        ui.add_space(12.0);
        let q = self.query.to_lowercase();
        let q = q.trim_start_matches('\\');
        let shown: Vec<&symbols::Symbol> = symbols::symbols()
            .iter()
            .filter(|s| {
                (self.group == "All" || s.group == self.group)
                    && (q.is_empty() || s.tex.to_lowercase().contains(q))
            })
            .collect();
        let max_h = ui.ctx().content_rect().height() * 0.52;
        egui::ScrollArea::vertical()
            .max_height(max_h)
            .min_scrolled_height(max_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if shown.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("No symbol matches “{}”.", self.query))
                            .color(p.muted),
                    );
                    return;
                }
                // repeat(auto-fill, minmax(56px, 1fr)), 6 px apart.
                let width = ui.available_width() - 4.0;
                let cols = ((width + 6.0) / (56.0 + 6.0)).floor().max(1.0) as usize;
                let cell_w = (width - 6.0 * (cols as f32 - 1.0)) / cols as f32;
                let rows = shown.len().div_ceil(cols);
                let (area, _) = ui.allocate_exact_size(
                    vec2(width + 4.0, rows as f32 * 54.0 - 6.0 + 4.0),
                    Sense::hover(),
                );
                for (i, s) in shown.iter().enumerate() {
                    let (r, c) = (i / cols, i % cols);
                    let rect = Rect::from_min_size(
                        pos2(
                            area.left() + 2.0 + c as f32 * (cell_w + 6.0),
                            area.top() + 2.0 + r as f32 * 54.0,
                        ),
                        vec2(cell_w, 48.0),
                    );
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let resp = ui.interact(rect, ui.scope_id().with(("sym", i)), Sense::click());
                    let tip = match s.pkg {
                        Some(pkg) => format!("{}  (needs {pkg})", s.tex),
                        None => s.tex.clone(),
                    };
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &s.tex)
                    });
                    let hover = resp.hovered();
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(theme::R),
                        if hover { p.accent_soft } else { p.surface },
                        Stroke::new(1.0, if hover { p.accent } else { p.border }),
                        StrokeKind::Inside,
                    );
                    let clip = ui.painter().with_clip_rect(rect.shrink(1.0));
                    match render::render(ui.ctx(), &s.tex, false, 15.0 * 1.21, p.ink) {
                        Ok(m) => {
                            let r = Rect::from_center_size(rect.center(), m.size);
                            clip.image(
                                m.texture.id(),
                                r,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        Err(_) => {
                            clip.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &s.tex,
                                FontId::monospace(10.0),
                                p.muted,
                            );
                        }
                    }
                    let resp = resp
                        .on_hover_text(tip)
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.clicked() {
                        let keep = ui.input(|i| i.modifiers.shift);
                        *picked = Picked::Insert(s.tex.clone(), s.pkg, keep);
                    }
                }
            });
    }
}
