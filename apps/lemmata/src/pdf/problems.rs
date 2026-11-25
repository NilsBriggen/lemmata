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
    /// Split lines cached per build id for virtualized log rendering and search.
    log_lines: Option<(u64, Vec<String>)>,
    log_query: String,
    log_hits: Vec<usize>,
    log_hit_idx: usize,
    log_query_key: (u64, String),
    log_jump: Option<usize>,
    log_copied: Option<Instant>,
    log_focus_search: bool,
    asked: Option<u64>,
    copied: Option<Instant>,
}

/// The file as written in the log, without a leading `./`.
fn file_of(d: &Diagnostic) -> Option<&str> {
    d.file.as_deref().map(|f| f.strip_prefix("./").unwrap_or(f))
}

/// Formats `path[:line]` keeping the filename and line number visible when the pill is narrower
/// than the full path (e.g. `…/amsmath.sty:42`).
fn format_location(ui: &Ui, path: &str, line: Option<u32>, max_w: f32) -> String {
    let full = match line {
        Some(l) => format!("{path}:{l}"),
        None => path.to_owned(),
    };
    let font = FontId::monospace(11.5);
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            + 14.0
            <= max_w
    };
    if fits(&full) {
        return full;
    }
    let tail = match path.rsplit_once(['/', '\\']) {
        Some((_, base)) => match line {
            Some(l) => format!("…/{base}:{l}"),
            None => format!("…/{base}"),
        },
        None => full.clone(),
    };
    if fits(&tail) {
        return tail;
    }
    // Even the basename is wider than the pill: keep the right end (extension + `:line`).
    let chars: Vec<char> = full.chars().collect();
    let mut keep = chars.len();
    while keep > 4 {
        keep -= 1;
        let cand: String = std::iter::once('…')
            .chain(chars[chars.len() - keep..].iter().copied())
            .collect();
        if fits(&cand) {
            return cand;
        }
    }
    tail
}

impl Problems {
    pub fn set_log(&mut self, build: u64, log: Result<String, String>) {
        self.log_lines = log
            .as_ref()
            .ok()
            .map(|t| (build, t.lines().map(str::to_owned).collect()));
        self.log = Some((build, log));
        self.log_query_key = (0, String::new());
    }

    fn update_log_hits(&mut self, build: u64) {
        let q = self.log_query.trim().to_lowercase();
        if self.log_query_key == (build, q.clone()) {
            return;
        }
        self.log_query_key = (build, q.clone());
        self.log_hits.clear();
        self.log_hit_idx = 0;
        if q.is_empty() {
            return;
        }
        if let Some((_, lines)) = &self.log_lines {
            for (i, line) in lines.iter().enumerate() {
                if line.to_lowercase().contains(&q) {
                    self.log_hits.push(i);
                }
            }
        }
        if let Some(&first) = self.log_hits.first() {
            self.log_jump = Some(first);
        }
    }

    fn step_log_hit(&mut self, dir: isize) {
        let n = self.log_hits.len() as isize;
        if n > 0 {
            self.log_hit_idx = (self.log_hit_idx as isize + dir).rem_euclid(n) as usize;
            self.log_jump = Some(self.log_hits[self.log_hit_idx]);
        }
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
            if crate::shortcuts::consume(c.ctx(), "pdf-search")
                || c.ctx()
                    .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
            {
                self.log_focus_search = true;
            }
            let toolbar = Rect::from_min_size(body.min, vec2(body.width(), 36.0));
            c.painter().rect_filled(toolbar, 0, p.surface_2);
            c.painter().hline(
                toolbar.x_range(),
                toolbar.bottom() - 0.5,
                Stroke::new(1.0, p.border),
            );
            let mut tb = c.new_child(
                egui::UiBuilder::new()
                    .max_rect(toolbar.shrink2(vec2(10.0, 4.0)))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            tb.spacing_mut().item_spacing.x = 6.0;
            let copied_recently = self
                .log_copied
                .is_some_and(|t| t.elapsed().as_secs_f32() < 2.0);
            if copied_recently {
                tb.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
            let raw_ok = self.log.as_ref().and_then(|(_, r)| r.as_ref().ok());
            if Button::new(if copied_recently {
                "Copied"
            } else {
                "Copy log"
            })
            .with_icon(if copied_recently {
                icons::CHECK
            } else {
                icons::COPY
            })
            .size(Size::Sm)
            .enabled(raw_ok.is_some())
            .show(&mut tb, p)
            .clicked()
                && let Some(t) = raw_ok
            {
                tb.ctx().copy_text(t.clone());
                self.log_copied = Some(Instant::now());
            }
            let hit_count = self.log_hits.len();
            if Button::icon(icons::CHEVRON_DOWN, "Next log match")
                .size(Size::Sm)
                .icon_size(14.0)
                .enabled(hit_count > 0)
                .show_icon(&mut tb, p)
                .clicked()
            {
                self.step_log_hit(1);
            }
            if Button::icon(icons::CHEVRON_UP, "Previous log match")
                .size(Size::Sm)
                .icon_size(14.0)
                .enabled(hit_count > 0)
                .show_icon(&mut tb, p)
                .clicked()
            {
                self.step_log_hit(-1);
            }
            let status = if self.log_query.trim().is_empty() {
                String::new()
            } else if hit_count > 0 {
                format!("{} of {hit_count}", self.log_hit_idx + 1)
            } else {
                "No matches".into()
            };
            if !status.is_empty() {
                tb.label(egui::RichText::new(status).size(11.5).color(p.muted));
            }
            tb.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let r = widgets::text_input(ui, p, &mut self.log_query, "Search raw log…", false);
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Search raw log"));
                if std::mem::take(&mut self.log_focus_search) {
                    r.request_focus();
                }
                if r.has_focus() || r.lost_focus() {
                    let (enter, shift, f3) = ui.input_mut(|i| {
                        let f3 = if i.consume_key(egui::Modifiers::SHIFT, egui::Key::F3) {
                            Some(-1)
                        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::F3) {
                            Some(1)
                        } else {
                            None
                        };
                        (i.key_pressed(egui::Key::Enter), i.modifiers.shift, f3)
                    });
                    if let Some(dir) = f3 {
                        self.step_log_hit(dir);
                    } else if enter {
                        self.update_log_hits(b.id);
                        self.step_log_hit(if shift { -1 } else { 1 });
                        r.request_focus();
                    }
                }
            });
            self.update_log_hits(b.id);
            let log_area = Rect::from_min_max(pos2(body.left(), toolbar.bottom()), body.max);
            let mut lc = c.new_child(egui::UiBuilder::new().max_rect(log_area));
            match (&self.log, &self.log_lines) {
                (Some((_, Ok(_))), Some((_, lines))) => {
                    let row_h = 18.0;
                    let font = FontId::monospace(12.0);
                    let cw = lc.fonts_mut(|f| f.glyph_width(&font, '0'));
                    let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(80);
                    let content_w = ((max_chars as f32 * cw) + 40.0).max(log_area.width() - 28.0);
                    let mut scroll = egui::ScrollArea::both()
                        .id_salt("raw-log")
                        .auto_shrink([false, false]);
                    if let Some(target_line) = self.log_jump.take() {
                        scroll = scroll.vertical_scroll_offset(
                            (target_line as f32 * row_h - log_area.height() / 3.0).max(0.0),
                        );
                    }
                    let current_hit = self.log_hits.get(self.log_hit_idx).copied();
                    let query_lc = self.log_query.trim().to_lowercase();
                    scroll.show_rows(&mut lc, row_h, lines.len(), |ui, range| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.set_min_width(content_w);
                        for idx in range {
                            let line = &lines[idx];
                            let (row_r, _) =
                                ui.allocate_exact_size(vec2(content_w, row_h), Sense::hover());
                            if current_hit == Some(idx) {
                                ui.painter().rect_filled(row_r, 0, p.accent_soft);
                            } else if !query_lc.is_empty()
                                && line.to_lowercase().contains(&query_lc)
                            {
                                ui.painter().rect_filled(
                                    row_r,
                                    0,
                                    Color32::from_rgba_unmultiplied(
                                        p.warn.r(),
                                        p.warn.g(),
                                        p.warn.b(),
                                        48,
                                    ),
                                );
                            }
                            let mut row_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(Rect::from_min_max(
                                        pos2(row_r.left() + 14.0, row_r.top()),
                                        row_r.max,
                                    ))
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            row_ui.add(
                                egui::Label::new(
                                    egui::RichText::new(line).font(font.clone()).color(p.ink_2),
                                )
                                .extend(),
                            );
                        }
                    });
                }
                (Some((_, Err(e))), _) => {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(&mut lc, |ui| {
                            ui.label(egui::RichText::new(e).color(p.muted));
                        });
                }
                _ => {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(&mut lc, |ui| {
                            ui.label(egui::RichText::new("Loading…").color(p.muted));
                        });
                }
            }
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
                            Some(Status::Success) => {
                                icons::show(ui, icons::CIRCLE_CHECK, 28.0, p.ok);
                                ui.label(egui::RichText::new("No errors or warnings.").font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                            }
                            Some(Status::Failed) => {
                                icons::show(ui, icons::CIRCLE_ALERT, 28.0, p.error);
                                ui.label(egui::RichText::new("The build failed, but no error was found in the log.").font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                                if Button::new("Open the raw log").kind(widgets::Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                                    self.log_tab = true;
                                }
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
                    let max_loc_w = (w * 0.45).max(80.0);
                    let loc = file_of(d).map(|f| {
                        let full = match d.line {
                            Some(l) => format!("{f}:{l}"),
                            None => f.to_owned(),
                        };
                        let shown = format_location(ui, f, d.line, max_loc_w);
                        (shown, full)
                    });
                    let loc_w = loc.as_ref().map_or(0.0, |(shown, _)| {
                        let g = ui.painter().layout_no_wrap(
                            shown.clone(),
                            FontId::monospace(11.5),
                            p.accent,
                        );
                        (g.size().x + 14.0).min(max_loc_w)
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
                    if let Some((shown, full)) = loc {
                        let clickable = d.line.is_some();
                        let (r, resp) = ui.allocate_exact_size(
                            vec2(loc_w, 20.0),
                            if clickable {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, clickable, &full)
                        });
                        let tip = if clickable {
                            if shown != full {
                                format!("Go to {full}")
                            } else {
                                "Go to this line".into()
                            }
                        } else if shown != full {
                            full
                        } else {
                            String::new()
                        };
                        let resp = if tip.is_empty() {
                            resp
                        } else {
                            resp.on_hover_text(tip)
                        };
                        let resp = if clickable {
                            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
                        } else {
                            resp
                        };
                        let border = if resp.hovered() && clickable {
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
                            shown,
                            FontId::monospace(11.5),
                            if clickable { p.accent } else { p.muted },
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
                {
                    let line = line as usize;
                    let from = line.saturating_sub(2).max(1);
                    let lines = (b.source)(f, from, line + 2);
                    indent(ui, &mut |ui| excerpt(ui, p, &lines, from, line));
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
                        if let Some(cmd) = &fix.command {
                            let mut copied = self.copied;
                            indent(ui, &mut |ui| {
                                if command_box(ui, p, cmd, &mut copied) {
                                    // A missing file is looked up again for this computer's installer.
                                    out.push(match &d.kind {
                                        Kind::MissingFile { name } => {
                                            Action::OpenTerminal(name.clone())
                                        }
                                        _ => Action::OpenCommand(cmd.clone()),
                                    });
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
/// `lines` are lines `from..` of the file, around `line`.
fn excerpt(ui: &mut Ui, p: &Palette, lines: &[String], from: usize, line: usize) {
    if lines.is_empty() {
        return;
    }
    let to = from + lines.len() - 1;
    let font = FontId::monospace(12.0);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let row_h = 12.0 * 1.55;
    let w = ui.available_width();
    // Dynamic gutter width so 5+ digit line numbers never collide with code.
    let num_right = (to.to_string().len().max(3) as f32 * cw + 10.0).ceil();
    let code_x = num_right + 10.0;
    let code_w = (w - code_x - 10.0).max(40.0);
    let galleys: Vec<_> = (from..=to)
        .map(|n| {
            let hit = n == line;
            let mut job = egui::text::LayoutJob::single_section(
                lines[n - from].clone(),
                egui::TextFormat {
                    font_id: font.clone(),
                    color: if hit { p.ink } else { p.ink_2 },
                    line_height: Some(row_h),
                    ..Default::default()
                },
            );
            job.wrap.max_width = code_w;
            job.wrap.break_anywhere = true;
            ui.fonts_mut(|f| f.layout_job(job))
        })
        .collect();
    let total_h: f32 = galleys.iter().map(|g| g.size().y.max(row_h)).sum::<f32>() + 12.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, total_h), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::R_SM),
        p.code_bg,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    let mut y = rect.top() + 6.0;
    for (k, n) in (from..=to).enumerate() {
        let g = &galleys[k];
        let h = g.size().y.max(row_h);
        let row = Rect::from_min_size(pos2(rect.left() + 1.0, y), vec2(w - 2.0, h));
        let hit = n == line;
        if hit {
            clip.rect_filled(row, 0, p.error_soft);
        }
        clip.text(
            pos2(row.left() + num_right, y + row_h / 2.0),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            font.clone(),
            p.faint,
        );
        clip.galley(
            pos2(row.left() + code_x, y + (row_h - 12.0) * 0.25),
            g.clone(),
            if hit { p.ink } else { p.ink_2 },
        );
        y += h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_location_preserves_filename_and_line_on_long_external_paths() {
        let mut results = (String::new(), String::new());
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            results.0 = format_location(
                ui,
                "/usr/share/texmf-dist/tex/latex/hyperref/hyperref.sty",
                Some(4210),
                180.0,
            );
            results.1 = format_location(ui, "chapters/method.tex", Some(7), 240.0);
        });
        out.textures_delta.clear();
        assert!(results.0.starts_with('…'), "{}", results.0);
        assert!(results.0.ends_with("hyperref.sty:4210"), "{}", results.0);
        assert_eq!(results.1, "chapters/method.tex:7");
    }
}
