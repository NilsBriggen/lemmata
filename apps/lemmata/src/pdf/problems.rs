//! Problems.svelte: errors, warnings and layout notes from the last build as cards, and the raw
//! TeX log. Shown in place of the PDF, like Overleaf's logs view.

use super::pane::{Action, Build};
use crate::icons;
use crate::screens::onboarding::command_box;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Rich, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use lemmata_log::{Diagnostic, Kind, Severity};
use lemmata_tex::Status;
use std::time::Instant;

#[derive(Default)]
pub struct Problems {
    log_tab: bool,
    show_info: bool,
    /// The raw log and the build it was read after.
    log: Option<(u64, Result<String, String>)>,
    asked: Option<u64>,
    copied: Option<Instant>,
}

/// The file as written in the log, without a leading `./`.
fn file_of(d: &Diagnostic) -> Option<&str> {
    d.file.as_deref().map(|f| f.strip_prefix("./").unwrap_or(f))
}

impl Problems {
    pub fn set_log(&mut self, build: u64, log: Result<String, String>) {
        self.log = Some((build, log));
    }

    pub fn show(&mut self, ui: &mut Ui, p: &Palette, b: &Build) -> Vec<Action> {
        let mut out = Vec::new();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface_2);
        let errors: Vec<&Diagnostic> = b
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        let warnings: Vec<&Diagnostic> = b
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect();
        let infos: Vec<&Diagnostic> = b
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Info)
            .collect();

        // .tabs
        let tabs = Rect::from_min_size(rect.min, vec2(rect.width(), 40.0));
        ui.painter().rect_filled(tabs, 0, p.surface);
        ui.painter().hline(
            tabs.x_range(),
            tabs.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
        let mut x = tabs.left() + 12.0;
        for (i, label) in ["Problems", "Raw log"].into_iter().enumerate() {
            let on = self.log_tab == (i == 1);
            let font = FontId::new(13.0, theme::font::sans_500());
            let g =
                ui.painter()
                    .layout_no_wrap(label.into(), font, if on { p.ink } else { p.muted });
            let badges: Vec<(usize, Color32)> = if i == 0 {
                [(errors.len(), p.error), (warnings.len(), p.warn)]
                    .into_iter()
                    .filter(|(n, _)| *n > 0)
                    .collect()
            } else {
                Vec::new()
            };
            let bw: Vec<f32> = badges
                .iter()
                .map(|(n, _)| {
                    ui.painter()
                        .layout_no_wrap(
                            n.to_string(),
                            FontId::new(11.0, theme::font::sans_600()),
                            Color32::WHITE,
                        )
                        .size()
                        .x
                        + 12.0
                })
                .collect();
            let w = 24.0 + g.size().x + bw.iter().map(|w| w + 6.0).sum::<f32>();
            let r = Rect::from_min_size(pos2(x, tabs.bottom() - 32.0), vec2(w, 32.0));
            let resp = ui.interact(r, ui.scope_id().with(("logs-tab", i)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, label));
            if resp.clicked() {
                self.log_tab = i == 1;
            }
            let painter = ui.painter();
            painter.galley(
                pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0),
                g.clone(),
                if on { p.ink } else { p.muted },
            );
            let mut bx = r.left() + 12.0 + g.size().x + 6.0;
            for ((n, fill), w) in badges.iter().zip(&bw) {
                let br = Rect::from_min_size(pos2(bx, r.center().y - 9.0), vec2(*w, 18.0));
                painter.rect_filled(br, CornerRadius::same(9), *fill);
                painter.text(
                    br.center(),
                    egui::Align2::CENTER_CENTER,
                    n.to_string(),
                    FontId::new(11.0, theme::font::sans_600()),
                    Color32::WHITE,
                );
                bx = br.right() + 6.0;
            }
            if on {
                painter.hline(r.x_range(), r.bottom() - 1.0, Stroke::new(2.0, p.accent));
            }
            x = r.right() + 2.0;
        }

        let body = Rect::from_min_max(pos2(rect.left(), tabs.bottom()), rect.max);
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body));
        if self.log_tab {
            c.painter().rect_filled(body, 0, p.surface);
            if self.log.as_ref().is_none_or(|(id, _)| *id != b.id) && self.asked != Some(b.id) {
                self.asked = Some(b.id);
                out.push(Action::ReadLog);
            }
            egui::ScrollArea::both()
                .id_salt("raw-log")
                .auto_shrink([false, false])
                .show(&mut c, |ui| {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            match &self.log {
                                Some((_, Ok(t))) => {
                                    // The log is for copying from, like the Svelte app's <pre>.
                                    let job = Rich::new()
                                        .add(t, FontId::monospace(12.0), p.ink_2)
                                        .line_height(18.0)
                                        .wrap(ui.available_width())
                                        .0;
                                    ui.add(egui::Label::new(job).selectable(true));
                                }
                                Some((_, Err(e))) => {
                                    ui.label(egui::RichText::new(e).color(p.muted));
                                }
                                None => {
                                    ui.label(egui::RichText::new("Loading…").color(p.muted));
                                }
                            }
                        });
                });
            return out;
        }

        let mut shown: Vec<&Diagnostic> = errors.iter().chain(&warnings).copied().collect();
        if self.show_info {
            shown.extend(&infos);
        }
        egui::ScrollArea::vertical().id_salt("problems").auto_shrink([false, false]).show(&mut c, |ui| {
            egui::Frame::NONE.inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                if errors.is_empty() && warnings.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space(24.0);
                        match b.status {
                            Some(Status::Success) | Some(Status::Failed) => {
                                icons::show(ui, icons::CIRCLE_CHECK, 28.0, p.ok);
                                ui.label(egui::RichText::new("No errors or warnings.").font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                            }
                            s => {
                                let t = match s {
                                    Some(Status::Cancelled) => "The last build was stopped before it finished.",
                                    Some(Status::Timeout) => "The last build took too long and was stopped.",
                                    _ => "Nothing has been built yet.",
                                };
                                ui.label(egui::RichText::new(t).font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                            }
                        }
                        if !infos.is_empty() {
                            let n = infos.len();
                            ui.label(egui::RichText::new(format!("{n} layout note{} about overfull or underfull lines below.", if n == 1 { "" } else { "s" })).color(p.muted));
                        }
                        ui.add_space(8.0);
                    });
                }
                for (i, d) in shown.iter().enumerate() {
                    self.card(ui, p, d, i, b, &mut out);
                }
                if !infos.is_empty() {
                    let n = infos.len();
                    let text = format!("Show {n} layout note{}", if n == 1 { "" } else { "s" });
                    widgets::checkbox(ui, p, &mut self.show_info, &text);
                }
            });
        });
        out
    }

    fn card(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        d: &Diagnostic,
        i: usize,
        b: &Build,
        out: &mut Vec<Action>,
    ) {
        let (edge, icon) = match d.severity {
            Severity::Error => (p.error, icons::CIRCLE_ALERT),
            Severity::Warning => (p.warn, icons::TRIANGLE_ALERT),
            Severity::Info => (p.faint, icons::INFO),
        };
        let frame = egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(theme::R)
            .inner_margin(Margin {
                left: 14,
                right: 12,
                top: 10,
                bottom: 10,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                let w = ui.available_width();
                // header: icon, message, location
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(0.0);
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        icons::show(ui, icon, 16.0, edge);
                    });
                    let loc = file_of(d).map(|f| match d.line {
                        Some(l) => format!("{f}:{l}"),
                        None => f.to_owned(),
                    });
                    let loc_w = loc.as_ref().map_or(0.0, |l| {
                        let g = ui.painter().layout_no_wrap(
                            l.clone(),
                            FontId::monospace(11.5),
                            p.accent,
                        );
                        (g.size().x + 14.0).min(w * 0.45)
                    });
                    let msg_w =
                        (w - 24.0 - if loc.is_some() { loc_w + 8.0 } else { 0.0 }).max(40.0);
                    Rich::new()
                        .add(
                            &d.message,
                            FontId::new(13.0, theme::font::sans_500()),
                            p.ink,
                        )
                        .line_height(18.85)
                        .wrap(msg_w)
                        .show(ui);
                    if let Some(l) = loc {
                        let (r, resp) = ui.allocate_exact_size(
                            vec2(loc_w, 20.0),
                            if d.line.is_some() {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, d.line.is_some(), &l)
                        });
                        let resp = resp.on_hover_text("Go to this line");
                        let border = if resp.hovered() && d.line.is_some() {
                            p.accent
                        } else {
                            p.border
                        };
                        ui.painter().rect(
                            r,
                            CornerRadius::same(theme::R_SM),
                            p.surface_2,
                            Stroke::new(1.0, border),
                            StrokeKind::Inside,
                        );
                        let g = ui.painter().layout_no_wrap(
                            l,
                            FontId::monospace(11.5),
                            if d.line.is_some() { p.accent } else { p.muted },
                        );
                        ui.painter()
                            .with_clip_rect(r.shrink2(vec2(6.0, 0.0)))
                            .galley(
                                pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0),
                                g,
                                p.accent,
                            );
                        if resp.clicked()
                            && let (Some(f), Some(line)) = (file_of(d), d.line)
                        {
                            out.push(Action::Goto(f.to_owned(), line as usize));
                        }
                    }
                });
                let indent = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
                    ui.horizontal_top(|ui| {
                        ui.add_space(24.0);
                        ui.vertical(|ui| add(ui));
                    });
                };
                if d.severity == Severity::Error
                    && i < 12
                    && let (Some(f), Some(line)) = (file_of(d), d.line)
                    && let Some(src) = (b.source)(f)
                {
                    indent(ui, &mut |ui| excerpt(ui, p, &src, line as usize));
                }
                match &d.fix {
                    Some(fix) => {
                        indent(ui, &mut |ui| {
                            Rich::new()
                                .add(&fix.explanation, FontId::proportional(13.0), p.muted)
                                .line_height(18.85)
                                .wrap(ui.available_width())
                                .show(ui);
                        });
                        if let Some(pkg) = &fix.package {
                            indent(ui, &mut |ui| {
                                let t = format!("Add \\usepackage{{{pkg}}}");
                                if Button::new(&t).size(Size::Sm).show(ui, p).clicked() {
                                    out.push(Action::AddPackage(pkg.clone()));
                                }
                            });
                        }
                        if let (Some(cmd), Kind::MissingFile { name }) = (&fix.command, &d.kind) {
                            let mut copied = self.copied;
                            indent(ui, &mut |ui| {
                                if command_box(ui, p, cmd, &mut copied) {
                                    out.push(Action::OpenTerminal(name.clone()));
                                }
                                ui.label(
                                    egui::RichText::new("After installing, recompile.")
                                        .size(12.0)
                                        .color(p.muted),
                                );
                            });
                            self.copied = copied;
                        }
                    }
                    None => {
                        if let Some(h) = &d.hint {
                            indent(ui, &mut |ui| {
                                Rich::new()
                                    .add(h, FontId::proportional(13.0), p.muted)
                                    .line_height(18.85)
                                    .wrap(ui.available_width())
                                    .show(ui);
                            });
                        }
                    }
                }
            });
        // The 3 px coloured left edge.
        let r = frame.response.rect;
        let edge_rect = Rect::from_min_max(r.min, pos2(r.left() + 3.0, r.bottom()));
        ui.painter().rect_filled(
            edge_rect,
            CornerRadius {
                nw: theme::R,
                sw: theme::R,
                ne: 0,
                se: 0,
            },
            edge,
        );
    }
}

/// Excerpt.svelte: two lines of source either side of the problem's line.
fn excerpt(ui: &mut Ui, p: &Palette, src: &str, line: usize) {
    let lines: Vec<&str> = src.split('\n').collect();
    let from = line.saturating_sub(2).max(1);
    let to = (line + 2).min(lines.len());
    if from > to {
        return;
    }
    let row_h = 12.0 * 1.55;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        vec2(w, (to - from + 1) as f32 * row_h + 12.0),
        Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::R_SM),
        p.code_bg,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    for (k, n) in (from..=to).enumerate() {
        let y = rect.top() + 6.0 + k as f32 * row_h;
        let row = Rect::from_min_size(pos2(rect.left() + 1.0, y), vec2(w - 2.0, row_h));
        let hit = n == line;
        if hit {
            clip.rect_filled(row, 0, p.error_soft);
        }
        clip.text(
            pos2(row.left() + 28.0, row.center().y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            FontId::monospace(12.0),
            p.faint,
        );
        clip.text(
            pos2(row.left() + 38.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            lines[n - 1].trim_end_matches('\r'),
            FontId::monospace(12.0),
            if hit { p.ink } else { p.ink_2 },
        );
    }
}
