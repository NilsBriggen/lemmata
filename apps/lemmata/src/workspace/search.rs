//! Project-wide search and replace (SearchPanel.svelte), in the sidebar's place. With the replace
//! box open, every hit shows how its line will read, and single lines can be left out.

use super::Workspace;
use crate::backend::Backend;
use crate::icons;
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use lemmata_shell::files::{FileMatches, ReplaceTarget, SearchOptions};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ProjectSearch {
    query: String,
    replacement: String,
    show_replace: bool,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    results: Vec<FileMatches>,
    /// Lines left out of a replacement.
    excluded: HashSet<(String, u32)>,
    error: Option<String>,
    message: Option<String>,
    /// The search runs 200 ms after the last keystroke.
    due: Option<Instant>,
    seq: u64,
    focus: bool,
}

impl ProjectSearch {
    /// Every hit shows its replaced line.
    #[cfg(test)]
    pub fn previewed(&self, text: &str) -> bool {
        !self.results.is_empty()
            && self
                .results
                .iter()
                .flat_map(|f| &f.matches)
                .all(|m| m.replaced.as_ref().is_some_and(|r| r.contains(text)))
    }

    /// Just opened: the sidebar must show.
    pub fn wants_sidebar(&self) -> bool {
        self.focus
    }

    fn options(&self) -> SearchOptions {
        SearchOptions {
            query: self.query.clone(),
            regex: self.regex,
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        }
    }

    fn soon(&mut self) {
        self.due = Some(Instant::now() + Duration::from_millis(200));
    }

    fn total(&self) -> usize {
        self.results
            .iter()
            .flat_map(|f| &f.matches)
            .map(|m| m.ranges.len())
            .sum()
    }

    /// The lines to replace on, by file, and how many occurrences that is.
    fn chosen(&self) -> (Vec<ReplaceTarget>, usize) {
        let mut n = 0;
        let mut targets = Vec::new();
        for f in &self.results {
            let lines: Vec<u32> = f
                .matches
                .iter()
                .filter(|m| !self.excluded.contains(&(f.path.clone(), m.line)))
                .inspect(|m| n += m.ranges.len())
                .map(|m| m.line)
                .collect();
            if !lines.is_empty() {
                targets.push(ReplaceTarget {
                    path: f.path.clone(),
                    lines,
                });
            }
        }
        (targets, n)
    }
}

/// The line split around the match ranges (characters), with long leading context cut so the
/// hit stays visible.
fn parts(text: &str, ranges: &[(usize, usize)]) -> Vec<(String, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let cut = |a: usize, b: usize| {
        chars[a.min(chars.len())..b.min(chars.len())]
            .iter()
            .collect::<String>()
    };
    let mut out = Vec::new();
    let mut at = 0;
    for &(a, b) in ranges {
        if a > at {
            out.push((cut(at, a), false));
        }
        out.push((cut(a, b), true));
        at = b;
    }
    out.push((cut(at, chars.len()), false));
    if let Some((t, false)) = out.first_mut()
        && t.chars().count() > 40
    {
        let tail: String = t
            .chars()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        *t = format!("…{tail}");
    }
    out
}

/// A checkbox on the dark sidebar.
fn check(
    ui: &mut Ui,
    p: &Palette,
    at: egui::Pos2,
    on: bool,
    label: &str,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Response {
    let bx = Rect::from_min_size(at, vec2(13.0, 13.0));
    let r = ui.interact(bx.expand(3.0), ui.scope_id().with(salt), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, label));
    let pt = ui.painter();
    if on {
        pt.rect_filled(bx, CornerRadius::same(3), p.accent_on_dark);
        icons::paint(pt, bx.shrink(2.0), icons::CHECK, p.chrome);
    } else {
        pt.rect(
            bx,
            CornerRadius::same(3),
            p.chrome_3,
            Stroke::new(1.0, p.chrome_muted),
            StrokeKind::Inside,
        );
    }
    r
}

impl Workspace {
    /// Ctrl+Shift+F, or "Search the project" on a hover card with the name to look for.
    pub fn open_search(&mut self, query: Option<String>) {
        let mut s = self.search.take().unwrap_or_default();
        if let Some(q) = query {
            s.query = q;
            s.due = Some(Instant::now());
        }
        s.focus = true;
        self.search = Some(s);
    }

    fn run_search(&mut self, backend: &Backend) {
        let Some(s) = &mut self.search else { return };
        s.due = None;
        s.seq += 1;
        let (seq, options) = (s.seq, s.options());
        let replacement = s.show_replace.then(|| s.replacement.clone());
        Workspace::job(
            backend,
            move |shell| shell.search_project(options, replacement),
            move |ws, _, r| {
                let Some(s) = &mut ws.search else { return };
                if s.seq != seq {
                    return;
                }
                s.error = None;
                s.message = None;
                s.excluded.clear();
                match r {
                    Ok(res) => s.results = res,
                    Err(e) => {
                        s.error = Some(e);
                        s.results.clear();
                    }
                }
            },
        );
    }

    fn replace_all(&mut self, backend: &Backend, settings: &Settings) {
        let Some(s) = &self.search else { return };
        let (targets, _) = s.chosen();
        if targets.is_empty() {
            return;
        }
        let (options, replacement) = (s.options(), s.replacement.clone());
        self.save_all_quietly(backend, settings);
        self.after_writes.push(Box::new(move |_, b| {
            let paths: Vec<String> = targets.iter().map(|t| t.path.clone()).collect();
            let files = targets.len();
            Workspace::job(
                b,
                move |shell| {
                    let saved = matches!(shell.history_snapshot("Before replace".into()), Ok(Some(_)));
                    let n = shell.replace_in_files(options.clone(), replacement.clone(), targets);
                    (saved, n, shell.search_project(options, Some(replacement)))
                },
                move |ws, b, (saved, n, again)| {
                    if n.is_ok() {
                        for p in &paths {
                            ws.reload(b, p);
                        }
                        ws.build_soon(false, false);
                    }
                    let Some(s) = &mut ws.search else { return };
                    s.excluded.clear();
                    match n {
                        Ok(n) => {
                            let undo = if saved {
                                "A snapshot was saved first, so History can bring the old text back."
                            } else {
                                "History is off (git isn't installed), so this can't be undone."
                            };
                            s.message = Some(format!(
                                "Replaced {n} occurrence{} in {files} file{}. {undo}",
                                if n == 1 { "" } else { "s" },
                                if files == 1 { "" } else { "s" },
                            ));
                            if let Ok(r) = again {
                                s.results = r;
                            }
                        }
                        Err(e) => s.error = Some(e),
                    }
                },
            );
        }));
    }

    pub(super) fn search_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        if self
            .search
            .as_ref()
            .and_then(|s| s.due)
            .is_some_and(|d| Instant::now() >= d)
        {
            self.run_search(backend);
        }
        let Some(s) = &mut self.search else { return };
        if let Some(d) = s.due {
            ui.ctx()
                .request_repaint_after(d.saturating_duration_since(Instant::now()));
        }
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.chrome_2);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0, p.chrome_border),
        );
        let rect = Rect::from_min_max(rect.min, pos2(rect.right() - 1.0, rect.bottom()));
        // .bar
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 36.0));
        ui.painter().text(
            pos2(bar.left() + 14.0, bar.center().y),
            egui::Align2::LEFT_CENTER,
            "SEARCH",
            FontId::new(11.0, theme::font::sans_600()),
            p.chrome_muted,
        );
        let mut close = false;
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(vec2(6.0, 0.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if Button::icon(icons::X, "Close search")
            .kind(Kind::Chrome)
            .size(Size::Sm)
            .icon_size(14.0)
            .ink(p.chrome_muted)
            .show_icon(&mut c, p)
            .clicked()
        {
            close = true;
        }
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
            pos2(rect.left() + 10.0, bar.bottom() + 2.0),
            pos2(rect.right() - 10.0, rect.bottom()),
        )));
        let ui = &mut body;
        ui.spacing_mut().item_spacing.y = 6.0;
        let mut replace = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (r, t) = ui.allocate_exact_size(vec2(14.0, 28.0), Sense::click());
            t.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, true, s.show_replace, "Show replace")
            });
            icons::paint(
                ui.painter(),
                Rect::from_center_size(r.center(), vec2(12.0, 12.0)),
                if s.show_replace {
                    icons::CHEVRON_DOWN
                } else {
                    icons::CHEVRON_RIGHT
                },
                p.chrome_muted,
            );
            if t.clicked() {
                s.show_replace = !s.show_replace;
                s.soon();
            }
            let before = s.query.clone();
            let r = widgets::chrome_input(ui, p, &mut s.query, "Search the project");
            if std::mem::take(&mut s.focus) {
                r.request_focus();
            }
            if s.query != before {
                s.soon();
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, tip, which) in [
                ("Aa", "Match case", 0),
                ("ab", "Whole words", 1),
                (".*", "Regular expression", 2),
            ] {
                let on = [s.case_sensitive, s.whole_word, s.regex][which];
                let g = ui.painter().layout_no_wrap(
                    label.into(),
                    FontId::monospace(11.0),
                    p.chrome_muted,
                );
                let (r, t) = ui.allocate_exact_size(vec2(g.size().x + 14.0, 22.0), Sense::click());
                t.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, tip));
                let pt = ui.painter();
                pt.rect(
                    r,
                    CornerRadius::same(theme::R_SM),
                    if on {
                        p.chrome_active
                    } else {
                        Color32::TRANSPARENT
                    },
                    Stroke::new(
                        1.0,
                        if on {
                            p.accent_on_dark
                        } else {
                            p.chrome_border
                        },
                    ),
                    StrokeKind::Inside,
                );
                pt.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::monospace(11.0),
                    if on { p.accent_on_dark } else { p.chrome_muted },
                );
                if t.on_hover_text(tip).clicked() {
                    match which {
                        0 => s.case_sensitive = !s.case_sensitive,
                        1 => s.whole_word = !s.whole_word,
                        _ => s.regex = !s.regex,
                    }
                    s.soon();
                }
            }
        });
        let total = s.total();
        let (_, chosen) = s.chosen();
        if s.show_replace {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let label = if chosen == total {
                    "Replace all".to_owned()
                } else {
                    format!("Replace {chosen}")
                };
                let g = ui.painter().layout_no_wrap(
                    label.clone(),
                    FontId::proportional(12.0),
                    p.chrome_ink,
                );
                let bw = g.size().x + 16.0;
                ui.allocate_ui(vec2(ui.available_width() - bw - 4.0, 28.0), |ui| {
                    let before = s.replacement.clone();
                    widgets::chrome_input(
                        ui,
                        p,
                        &mut s.replacement,
                        if s.regex {
                            "Replace ($1 for groups)"
                        } else {
                            "Replace with"
                        },
                    );
                    if s.replacement != before {
                        s.soon();
                    }
                });
                let (r, t) = ui.allocate_exact_size(
                    vec2(bw, 28.0),
                    if chosen > 0 {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                t.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, chosen > 0, &label));
                let pt = ui.painter();
                pt.rect(
                    r,
                    CornerRadius::same(theme::R_SM),
                    if t.hovered() && chosen > 0 {
                        p.chrome_hover
                    } else {
                        p.chrome_3
                    },
                    Stroke::new(1.0, p.chrome_border),
                    StrokeKind::Inside,
                );
                pt.galley(
                    r.center() - g.size() / 2.0,
                    g,
                    if chosen > 0 {
                        p.chrome_ink
                    } else {
                        p.chrome_muted
                    },
                );
                if t.on_hover_text(
                    "Replace the checked lines. History keeps a snapshot of the text before.",
                )
                .clicked()
                {
                    replace = true;
                }
            });
        }
        ui.add_space(2.0);
        let note = |ui: &mut Ui, t: &str, c: Color32, size: f32| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(t).size(size).color(c)).wrap());
            });
        };
        if let Some(e) = &s.error {
            note(ui, e, p.error, 12.0);
        }
        if let Some(m) = &s.message {
            note(ui, m, p.ok, 12.0);
        }
        if !s.query.is_empty() && s.error.is_none() {
            let n = s.results.len();
            note(
                ui,
                &format!(
                    "{total} result{} in {n} file{}",
                    if total == 1 { "" } else { "s" },
                    if n == 1 { "" } else { "s" }
                ),
                p.chrome_muted,
                11.5,
            );
        }
        let mut open: Option<(String, u32)> = None;
        let list = Rect::from_min_max(pos2(rect.left(), ui.cursor().top()), rect.max);
        let mut lc = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical()
            .id_salt("project-search")
            .auto_shrink([false, false])
            .show(&mut lc, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let w = ui.available_width();
                for f in &s.results {
                    // .file
                    let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
                    let mut x = r.left() + 14.0;
                    if s.show_replace {
                        let all = f
                            .matches
                            .iter()
                            .all(|m| !s.excluded.contains(&(f.path.clone(), m.line)));
                        if check(
                            ui,
                            p,
                            pos2(x, r.top() + 9.0),
                            all,
                            &format!("Replace in {}", f.path),
                            ("file", &f.path),
                        )
                        .clicked()
                        {
                            for m in &f.matches {
                                if all {
                                    s.excluded.insert((f.path.clone(), m.line));
                                } else {
                                    s.excluded.remove(&(f.path.clone(), m.line));
                                }
                            }
                        }
                        x += 19.0;
                    }
                    ui.painter().text(
                        pos2(x, r.top() + 8.0),
                        egui::Align2::LEFT_TOP,
                        &f.path,
                        FontId::new(12.0, theme::font::sans_600()),
                        p.chrome_ink,
                    );
                    for m in &f.matches {
                        let key = (f.path.clone(), m.line);
                        let off = s.show_replace && s.excluded.contains(&key);
                        let two = s.show_replace && m.replaced.is_some();
                        let (r, row) = ui.allocate_exact_size(
                            vec2(w, if two { 40.0 } else { 23.0 }),
                            Sense::click(),
                        );
                        row.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                format!("{}:{} {}", f.path, m.line, m.text.trim()),
                            )
                        });
                        let fade = |c: Color32| if off { c.gamma_multiply(0.45) } else { c };
                        let mut x = r.left() + 14.0;
                        if s.show_replace {
                            if check(
                                ui,
                                p,
                                pos2(x, r.top() + 6.0),
                                !off,
                                &format!("Replace on line {}", m.line),
                                ("line", &f.path, m.line),
                            )
                            .clicked()
                            {
                                if off {
                                    s.excluded.remove(&key);
                                } else {
                                    s.excluded.insert(key.clone());
                                }
                            }
                            x += 13.0 + 6.0;
                        } else {
                            x += 2.0;
                        }
                        let hover = row.hovered();
                        if hover {
                            ui.painter().rect_filled(
                                Rect::from_min_max(pos2(x - 4.0, r.top()), r.max),
                                0,
                                p.chrome_hover,
                            );
                        }
                        let font = FontId::monospace(12.0);
                        let ink = if hover { p.chrome_ink } else { p.chrome_muted };
                        // The line number, right-aligned in 2.5em.
                        let num_w = 12.0 * 0.6 * 2.5 * 1.2;
                        ui.painter().text(
                            pos2(x + num_w, r.top() + 3.0),
                            egui::Align2::RIGHT_TOP,
                            m.line.to_string(),
                            font.clone(),
                            fade(p.chrome_muted.gamma_multiply(0.7)),
                        );
                        let tx = x + num_w + 8.0;
                        let clip = ui.painter().with_clip_rect(Rect::from_min_max(
                            pos2(tx, r.top()),
                            pos2(r.right() - 12.0, r.bottom()),
                        ));
                        let mut job = egui::text::LayoutJob::default();
                        for (t, hit) in parts(&m.text, &m.ranges) {
                            job.append(
                                &t,
                                0.0,
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color: fade(if hit { p.chrome_ink } else { ink }),
                                    background: if hit {
                                        fade(Color32::from_rgba_unmultiplied(
                                            p.warn.r(),
                                            p.warn.g(),
                                            p.warn.b(),
                                            102,
                                        ))
                                    } else {
                                        Color32::TRANSPARENT
                                    },
                                    ..Default::default()
                                },
                            );
                        }
                        clip.galley(
                            pos2(tx, r.top() + 3.0),
                            ui.fonts_mut(|fo| fo.layout_job(job)),
                            ink,
                        );
                        if two && let Some(new) = &m.replaced {
                            let shown: String = if new.chars().count() > 90 {
                                new.chars().take(90).collect::<String>() + "…"
                            } else {
                                new.clone()
                            };
                            clip.text(
                                pos2(tx, r.top() + 20.0),
                                egui::Align2::LEFT_TOP,
                                format!("→ {shown}"),
                                font.clone(),
                                fade(p.ok),
                            );
                        }
                        if row.clicked() {
                            open = Some((f.path.clone(), m.line));
                        }
                    }
                }
                ui.add_space(12.0);
            });
        if close {
            self.search = None;
        }
        if replace {
            self.replace_all(backend, settings);
        }
        if let Some((path, line)) = open {
            self.goto(backend, &path, line as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_mark_the_hits_and_cut_long_context() {
        let p = parts("see \\ref{a} and \\ref{b}", &[(4, 11), (16, 23)]);
        let hits: Vec<&str> = p.iter().filter(|x| x.1).map(|x| x.0.as_str()).collect();
        assert_eq!(hits, ["\\ref{a}", "\\ref{b}"]);
        let long = format!("{}x", "a".repeat(50));
        let p = parts(&long, &[(50, 51)]);
        assert_eq!(p[0].0.chars().count(), 31);
        assert!(p[0].0.starts_with('…'));
    }
}
