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
    focused: bool,
    selected: usize,
    scroll: bool,
    last_cols: usize,
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
            Some("Click or press Enter to insert. Hold Shift to keep this open."),
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
        let before_q = self.query.clone();
        let before_g = self.group.clone();
        // .bar: the search box and the category, 160 px.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let w = (ui.available_width() - 160.0 - 8.0).max(120.0);
            ui.allocate_ui(vec2(w, 32.0), |ui| {
                let r =
                    widgets::text_input(ui, p, &mut self.query, "Search by command name", false);
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Search by command name"));
                if !self.focused {
                    r.request_focus();
                    self.focused = true;
                }
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
        if self.query != before_q || self.group != before_g {
            self.selected = 0;
            self.scroll = true;
        }
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
        let cols_step = self.last_cols.max(1);
        if !shown.is_empty() {
            if self.selected >= shown.len() {
                self.selected = shown.len() - 1;
            }
            let shift = ui.input(|i| i.modifiers.shift);
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    self.selected = (self.selected + cols_step).min(shown.len() - 1);
                    self.scroll = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.selected = self.selected.saturating_sub(cols_step);
                    self.scroll = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
                    && self.query.is_empty()
                {
                    self.selected = (self.selected + 1).min(shown.len() - 1);
                    self.scroll = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
                    && self.query.is_empty()
                {
                    self.selected = self.selected.saturating_sub(1);
                    self.scroll = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)
                {
                    let s = shown[self.selected];
                    *picked = Picked::Insert(s.tex.clone(), s.pkg, shift);
                }
            });
        }
        let vh = ui.ctx().content_rect().height();
        let raw_h = (vh - 210.0).clamp(162.0, vh * 0.52);
        // Snap to a whole multiple of the 54 px tile row stride so the last row isn't sliced.
        let max_h = (raw_h / 54.0).floor().max(3.0) * 54.0;
        egui::ScrollArea::vertical()
            .max_height(max_h)
            .min_scrolled_height(max_h)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
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
                let width = (ui.available_width() - 12.0).max(60.0);
                let cols = ((width + 6.0) / (56.0 + 6.0)).floor().max(1.0) as usize;
                self.last_cols = cols;
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
                    if i == self.selected && std::mem::take(&mut self.scroll) {
                        ui.scroll_to_rect(rect.expand(4.0), None);
                    }
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let resp = ui.interact(rect, ui.scope_id().with(("sym", i)), Sense::click());
                    let tip = match s.pkg {
                        Some(pkg) => format!("{}  (needs {pkg})", s.tex),
                        None => s.tex.clone(),
                    };
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            i == self.selected,
                            &s.tex,
                        )
                    });
                    let hover = resp.hovered();
                    if hover && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                        self.selected = i;
                    }
                    let active = hover || i == self.selected;
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(theme::R),
                        if active { p.accent_soft } else { p.surface },
                        Stroke::new(1.0, if active { p.accent } else { p.border }),
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
