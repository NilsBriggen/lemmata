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

/// `.toasts.right`: bottom right, over everything.
pub fn show(ctx: &egui::Context, p: &Palette, t: &Toast) -> Option<Out> {
    let screen = ctx.content_rect();
    let mut out = None;
    egui::Area::new(egui::Id::unique("update-toast"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(screen.right() - 20.0, screen.bottom() - 20.0))
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
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        icons::show(ui, icons::SPARKLES, 16.0, p.ok);
                        let v = &t.update.version;
                        let text = match (&t.error, t.progress) {
                            (Some(e), _) => format!("The update failed: {e}"),
                            (None, Some(f)) => {
                                format!("Downloading Lemmata {v}… {}%", (f * 100.0).round())
                            }
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
                });
        });
    out
}
