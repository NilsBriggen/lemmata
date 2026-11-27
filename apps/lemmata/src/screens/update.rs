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
                        ui.horizontal_top(|ui| {
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
                        });
                        if t.progress.is_none() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    if Button::new("Install and restart")
                                        .kind(Kind::Primary)
                                        .size(Size::Sm)
                                        .show(ui, p)
                                        .clicked()
                                    {
                                        out = Some(Out::Install);
                                    }
                                    if Button::new("Later")
                                        .kind(Kind::Ghost)
                                        .size(Size::Sm)
                                        .show(ui, p)
                                        .clicked()
                                    {
                                        out = Some(Out::Later);
                                    }
                                },
                            );
                        }
                        if let Some(frac) = t.progress {
                            let bar_w = ui.available_width().clamp(40.0, 480.0);
                            let (bar_r, _) = ui
                                .allocate_exact_size(egui::vec2(bar_w, 5.0), egui::Sense::hover());
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn update_toast_keeps_action_buttons_inside_viewport_on_long_errors() {
        let toast = Toast {
            update: Update {
                version: "4.2.0".into(),
                url: "https://example.org/lemmata".into(),
                signature: String::new(),
            },
            progress: None,
            error: Some("Network error while downloading installer package: ".repeat(5)),
        };
        for &size in &[
            [1400.0, 900.0],
            [760.0, 600.0],
            [420.0, 400.0],
            [320.0, 320.0],
        ] {
            let mut h = crate::testing::cpu_harness_state(
                size,
                theme::LIGHT,
                |ui, t: &mut Toast| {
                    show(ui.ctx(), &theme::LIGHT, t, 0.0);
                },
                Toast {
                    update: toast.update.clone(),
                    progress: toast.progress,
                    error: toast.error.clone(),
                },
            );
            h.run_steps(2);
            crate::testing::assert_settles(&mut h, &format!("update toast {size:?}"));
            crate::testing::assert_ui_geometry(&h, &format!("update toast {size:?}"));
            let screen = h.ctx.content_rect();
            for lbl in ["Later", "Install and restart"] {
                let rect = h.get_by_label(lbl).rect();
                assert!(
                    rect.left() >= screen.left() - 1.0 && rect.right() <= screen.right() + 1.0,
                    "button {lbl:?} ({rect:?}) overflowed viewport {screen:?}"
                );
            }
        }
    }
}
