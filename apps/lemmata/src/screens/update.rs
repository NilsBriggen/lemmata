//! App.svelte's update toast: a newer Lemmata is available, then its download, or why it failed.

use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{Button, Kind, Size};
use eframe::egui::{self, Stroke, pos2};
use lemmata_shell::update::Update;

pub struct Toast {
    pub update: Update,
    /// While downloading: the fraction done.
    pub progress: Option<f32>,
    pub error: Option<String>,
}

pub enum Out {
    Later,
    Install,
}

/// `.toasts.right`: bottom right, offset upward when another bottom toast is visible.
pub fn show(ctx: &egui::Context, p: &Palette, t: &Toast, bottom_offset: f32) -> Option<Out> {
    let screen = ctx.content_rect();
    let mut out = None;
    egui::Area::new(egui::Id::unique("update-toast"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(
            screen.right() - 20.0,
            screen.bottom() - 20.0 - bottom_offset,
        ))
        .show(ctx, |ui| {
            ui.set_max_width((screen.width() * 0.9).min(520.0));
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(theme::R_LG)
                .shadow(p.shadow)
                .inner_margin(egui::Margin {
                    left: 12,
                    right: 8,
                    top: 8,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            icons::show(
                                ui,
                                if t.error.is_some() {
                                    icons::CIRCLE_ALERT
                                } else {
                                    icons::SPARKLES
                                },
                                16.0,
                                if t.error.is_some() { p.error } else { p.ok },
                            );
                            let v = &t.update.version;
                            let text = match (&t.error, t.progress) {
                                (Some(e), _) => format!("The update failed: {e}"),
                                (None, Some(f)) if f > 0.0 => {
                                    format!("Downloading Lemmata {v}… {}%", (f * 100.0).round())
                                }
                                // No size from the server, or nothing yet: no percentage to show.
                                (None, Some(_)) => format!("Downloading Lemmata {v}…"),
                                (None, None) => format!("Lemmata {v} is available."),
                            };
                            ui.add(egui::Label::new(egui::RichText::new(text).color(p.ink)).wrap());
                            if t.progress.is_none() {
                                if Button::new("Later")
                                    .kind(Kind::Ghost)
                                    .size(Size::Sm)
                                    .show(ui, p)
                                    .clicked()
                                {
                                    out = Some(Out::Later);
                                }
                                if Button::new("Install and restart")
                                    .kind(Kind::Primary)
                                    .size(Size::Sm)
                                    .show(ui, p)
                                    .clicked()
                                {
                                    out = Some(Out::Install);
                                }
                            }
                        });
                        if let Some(frac) = t.progress {
                            let (bar_r, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width().max(200.0), 5.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                bar_r,
                                egui::CornerRadius::same(3),
                                p.surface_3,
                            );
                            let fill = egui::Rect::from_min_size(
                                bar_r.min,
                                egui::vec2(bar_r.width() * frac.clamp(0.04, 1.0), bar_r.height()),
                            );
                            ui.painter()
                                .rect_filled(fill, egui::CornerRadius::same(3), p.accent);
                        }
                    });
                });
        });
    out
}
