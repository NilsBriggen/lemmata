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
    /// The open editors' versions when the results came: lines are counted in that text.
    versions: std::collections::HashMap<String, u64>,
    error: Option<String>,
    message: Option<String>,
    /// The search runs 200 ms after the last keystroke.
    due: Option<Instant>,
    seq: u64,
    /// The newest search that has come back.
    got: u64,
    /// The query and options the results are for: lines left out stay left out while they hold.
    searched: String,
    focus: bool,
    /// Pending confirmation for destructive project-wide replace: `(occurrences, files)`.
    pub(super) confirm_replace: Option<(usize, usize)>,
}

impl ProjectSearch {
    #[cfg(test)]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

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

    pub fn focus(&mut self) {
        self.focus = true;
    }

    fn options(&self) -> SearchOptions {
        SearchOptions {
            query: self.query.clone(),
            regex: self.regex,
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        }
    }

    /// What the hits depend on, apart from the replacement text.
    fn key(&self) -> String {
        format!(
            "{}\u{0}{}{}{}",
            self.query, self.regex, self.case_sensitive, self.whole_word
        )
    }

    /// The search hit the file's limit: more lines match than are listed.
    fn truncated(&self) -> bool {
        self.results.iter().map(|f| f.matches.len()).sum::<usize>()
            >= lemmata_shell::files::MAX_MATCHES
    }

    /// New results. A new replacement text finds the same lines: the ones left out stay out.
    fn take_results(&mut self, key: String, res: Vec<FileMatches>) {
        if self.searched != key {
            self.searched = key;
            self.excluded.clear();
        }
        self.excluded.retain(|(p, l)| {
            res.iter()
                .any(|f| f.path == *p && f.matches.iter().any(|m| m.line == *l))
        });
        self.results = res;
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

/// A checkbox on the quiet sidebar ground.
fn check(
    ui: &mut Ui,
    p: &Palette,
    at: egui::Pos2,
    on: bool,
    label: &str,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Response {
    let bx = Rect::from_min_size(at, vec2(13.0, 13.0));
    let r = ui
        .interact(bx.expand(3.0), ui.scope_id().with(salt), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, label));
    let pt = ui.painter();
    if on {
        pt.rect_filled(bx, CornerRadius::same(3), p.accent);
        icons::paint(pt, bx.shrink(2.0), icons::CHECK, p.accent_ink);
    } else {
        pt.rect(
            bx,
            CornerRadius::same(3),
            p.surface,
            Stroke::new(1.0, p.border_strong),
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
        self.side_panel = super::panels::SidePanel::Search;
        self.focus_mode = false;
    }

    /// `note`: shown with the new results (the old message goes when they come).
    fn run_search(&mut self, backend: &Backend, note: Option<String>) {
        let Some(s) = &mut self.search else { return };
        s.due = None;
        s.seq += 1;
        let (seq, options, key) = (s.seq, s.options(), s.key());
        let replacement = s.show_replace.then(|| s.replacement.clone());
        Workspace::job(
            backend,
            move |shell| shell.search_project(options, replacement),
            move |ws, _, r| {
                let versions = ws.editor_versions();
                let Some(s) = &mut ws.search else { return };
                if s.seq != seq {
                    return;
                }
                s.got = seq;
                s.versions = versions;
                s.error = None;
                s.message = note;
                match r {
                    Ok(res) => s.take_results(key, res),
                    Err(e) => {
                        s.error = Some(e);
                        s.results.clear();
                    }
                }
            },
        );
    }

    /// Each open editor's text version.
    fn editor_versions(&self) -> std::collections::HashMap<String, u64> {
        self.editors
            .iter()
            .map(|(p, e)| (p.clone(), e.doc.version))
            .collect()
    }

    pub(crate) fn replace_all(&mut self, backend: &Backend, settings: &Settings) {
        let versions = self.editor_versions();
        let Some(s) = &mut self.search else { return };
        let (targets, _) = s.chosen();
        // The results must be those of the fields as they are now.
        if targets.is_empty() || s.due.is_some() || s.got != s.seq {
            return;
        }
        // Lines added or removed since the search would move the hits (and the ones left out)
        // onto other lines: the results are brought up to date first.
        // A file opened since then and not edited is at version 0, as loaded.
        let moved = targets.iter().any(|t| {
            versions
                .get(&t.path)
                .is_some_and(|v| s.versions.get(&t.path).copied().unwrap_or(0) != *v)
        });
        // Unsaved edits: the hits are lines of the disk text, so save and search again first.
        let unsaved = targets.iter().any(|t| self.buffers.is_dirty(&t.path));
        if moved || unsaved {
            let note = Some("The files changed since the search, so the results were updated. Check them and Replace again.".to_owned());
            if unsaved {
                self.after_saving(backend, settings, move |ws, b| ws.run_search(b, note));
            } else {
                self.run_search(backend, note);
            }
            return;
        }
        let (options, replacement) = (s.options(), s.replacement.clone());
        self.after_saving(backend, settings, move |_, b| {
            let paths: Vec<String> = targets.iter().map(|t| t.path.clone()).collect();
            let files = targets.len();
            Workspace::job(
                b,
                move |shell| {
                    // An empty snapshot still means History holds the state.
                    let saved = shell.history_snapshot("Before replace".into()).is_ok();
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
        });
    }

    pub(super) fn search_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        _settings: &Settings,
    ) {
        if self
            .search
            .as_ref()
            .and_then(|s| s.due)
            .is_some_and(|d| Instant::now() >= d)
        {
            self.run_search(backend, None);
        }
        if self.search.is_none() {
            self.search = Some(ProjectSearch::default());
        }
        let Some(s) = &mut self.search else { return };
        if let Some(d) = s.due {
            ui.ctx()
                .request_repaint_after(d.saturating_duration_since(Instant::now()));
        }
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.app);
        // 32 px quiet sidebar header.
        let rest = super::panels::side_head(ui, p, "Search", |_| {});
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
            pos2(rest.left() + 10.0, rest.top() + 4.0),
            pos2(rest.right() - 10.0, rest.bottom()),
        )));
        let ui = &mut body;
        ui.spacing_mut().item_spacing.y = 6.0;
        let mut replace = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (r, t) = ui.allocate_exact_size(vec2(16.0, 30.0), Sense::click());
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
                p.muted,
            );
            if t.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                s.show_replace = !s.show_replace;
                s.soon();
            }
            let before = s.query.clone();
            let iw = ui.available_width();
            let r = ui
                .allocate_ui(vec2(iw, 30.0), |ui| {
                    widgets::text_input(ui, p, &mut s.query, "Search the project", false)
                })
                .inner;
            ui.ctx()
                .accesskit_node_builder(r.id, |n| n.set_label("Search the project"));
            if std::mem::take(&mut s.focus) {
                r.request_focus();
            }
            if s.query != before {
                s.soon();
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, tip, which) in [
                ("Aa", "Match case", 0),
                ("ab", "Whole words", 1),
                (".*", "Regular expression", 2),
            ] {
                let on = [s.case_sensitive, s.whole_word, s.regex][which];
                let g = ui
                    .painter()
                    .layout_no_wrap(label.into(), FontId::monospace(11.0), p.muted);
                let (r, t) = ui.allocate_exact_size(vec2(g.size().x + 14.0, 22.0), Sense::click());
                t.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, tip));
                let pt = ui.painter();
                pt.rect(
                    r,
                    CornerRadius::same(theme::R_SM),
                    if on {
                        p.accent_soft
                    } else if t.hovered() {
                        p.surface_3
                    } else {
                        p.surface
                    },
                    Stroke::new(1.0, if on { p.accent } else { p.border }),
                    StrokeKind::Inside,
                );
                pt.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::monospace(11.0),
                    if on { p.accent_text } else { p.muted },
                );
                if t.on_hover_text(tip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
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
                ui.add_space(20.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                let label = if chosen == total && !s.truncated() {
                    "Replace all".to_owned()
                } else if chosen == total {
                    "Replace listed".to_owned()
                } else {
                    format!("Replace {chosen}")
                };
                let bw = ui
                    .painter()
                    .layout_no_wrap(
                        label.clone(),
                        FontId::new(12.0, theme::font::sans_500()),
                        p.ink,
                    )
                    .size()
                    .x
                    + 18.0;
                ui.allocate_ui(
                    vec2((ui.available_width() - bw - 4.0).max(60.0), 30.0),
                    |ui| {
                        let before = s.replacement.clone();
                        let hint = if s.regex {
                            "Replace ($1 for groups)"
                        } else {
                            "Replace with"
                        };
                        let r = widgets::text_input(ui, p, &mut s.replacement, hint, false);
                        ui.ctx().accesskit_node_builder(r.id, |n| n.set_label(hint));
                        if s.replacement != before {
                            s.soon();
                        }
                    },
                );
                if Button::new(&label)
                    .kind(Kind::Primary)
                    .size(Size::Sm)
                    .enabled(chosen > 0)
                    .show(ui, p)
                    .on_hover_text(
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
                ui.add_space(2.0);
                ui.add(egui::Label::new(RichText::new(t).size(size).color(c)).wrap());
            });
        };
        if let Some(e) = &s.error {
            note(ui, e, p.error, 12.0);
        }
        if let Some(m) = &s.message {
            note(ui, m, p.ok, 12.0);
        }
        if !s.query.is_empty() && s.error.is_none() && s.due.is_none() && total == 0 {
            let narrowed = s.regex || s.case_sensitive || s.whole_word;
            note(
                ui,
                if narrowed {
                    "Nothing found. Turning off match case, whole word or regex may find more."
                } else {
                    "Nothing found in the project's files."
                },
                p.muted,
                11.5,
            );
        } else if !s.query.is_empty() && s.error.is_none() {
            let n = s.results.len();
            note(
                ui,
                &format!(
                    "{total} result{} in {n} file{}{}",
                    if total == 1 { "" } else { "s" },
                    if n == 1 { "" } else { "s" },
                    if s.truncated() {
                        " (the first ones; more match)"
                    } else {
                        ""
                    }
                ),
                p.faint,
                11.5,
            );
        }
        let mut open: Option<(String, u32)> = None;
        let list = Rect::from_min_max(pos2(rect.left(), ui.cursor().top() + 2.0), rect.max);
        let mut lc = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical()
            .id_salt("project-search")
            .auto_shrink([false, false])
            .show(&mut lc, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let w = ui.available_width();
                for f in &s.results {
                    let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
                    let mut x = r.left() + 12.0;
                    if s.show_replace {
                        let all = f
                            .matches
                            .iter()
                            .all(|m| !s.excluded.contains(&(f.path.clone(), m.line)));
                        if check(
                            ui,
                            p,
                            pos2(x, r.top() + 7.0),
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
                        pos2(x, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        &f.path,
                        FontId::new(11.5, theme::font::sans_600()),
                        p.ink_2,
                    );
                    ui.painter().text(
                        pos2(r.right() - 12.0, r.center().y),
                        egui::Align2::RIGHT_CENTER,
                        f.matches.len().to_string(),
                        FontId::monospace(10.5),
                        p.faint,
                    );
                    for m in &f.matches {
                        let two = s.show_replace && m.replaced.is_some();
                        let (r, row) = ui.allocate_exact_size(
                            vec2(w, if two { 40.0 } else { 24.0 }),
                            Sense::click(),
                        );
                        row.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                format!("{}:{} {}", f.path, m.line, m.text.trim()),
                            )
                        });
                        if !ui.is_rect_visible(r) && !row.clicked() {
                            continue;
                        }
                        let key = (f.path.clone(), m.line);
                        let off = s.show_replace && s.excluded.contains(&key);
                        let fade = |c: Color32| if off { c.gamma_multiply(0.45) } else { c };
                        let mut x = r.left() + 12.0;
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
                            x += 19.0;
                        } else {
                            x += 2.0;
                        }
                        let hover = row.hovered();
                        if hover {
                            ui.painter().rect_filled(
                                Rect::from_min_max(
                                    pos2(r.left() + 6.0, r.top()),
                                    pos2(r.right() - 6.0, r.bottom()),
                                ),
                                CornerRadius::same(6),
                                p.rail_active,
                            );
                        }
                        let font = FontId::monospace(11.5);
                        let ink = if hover { p.ink } else { p.ink_2 };
                        let num_w = 11.5 * 0.6 * 2.5 * 1.2;
                        ui.painter().text(
                            pos2(x + num_w, r.top() + 4.0),
                            egui::Align2::RIGHT_TOP,
                            m.line.to_string(),
                            font.clone(),
                            fade(p.faint),
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
                                    color: fade(if hit { p.ink } else { ink }),
                                    background: if hit {
                                        fade(p.warn_soft)
                                    } else {
                                        Color32::TRANSPARENT
                                    },
                                    ..Default::default()
                                },
                            );
                        }
                        clip.galley(
                            pos2(tx, r.top() + 4.0),
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
                                pos2(tx, r.top() + 21.0),
                                egui::Align2::LEFT_TOP,
                                format!("→ {shown}"),
                                font.clone(),
                                fade(p.ok),
                            );
                        }
                        if row
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            open = Some((f.path.clone(), m.line));
                        }
                    }
                }
                ui.add_space(12.0);
            });
        if replace && let Some(s) = &mut self.search {
            let (targets, count) = s.chosen();
            if !targets.is_empty() && s.due.is_none() && s.got == s.seq {
                s.confirm_replace = Some((count, targets.len()));
            }
        }
        if let Some((path, line)) = open {
            self.goto(backend, &path, line as usize);
        }
    }

    pub(super) fn search_dialog(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        let Some((count, files)) = self.search.as_ref().and_then(|s| s.confirm_replace) else {
            return;
        };
        let history_on = self.history.on;
        let title = format!(
            "Replace {count} occurrence{} in {files} file{}?",
            if count == 1 { "" } else { "s" },
            if files == 1 { "" } else { "s" }
        );
        let mut confirm = false;
        let mut cancel = false;
        let r = widgets::modal_with(
            ctx,
            p,
            &title,
            None,
            440.0,
            |ui| {
                if history_on {
                    widgets::paragraph(
                        ui,
                        "A snapshot will be saved first, so you can undo these changes from History.",
                        FontId::proportional(13.0),
                        p.muted,
                        1.45,
                        ui.available_width(),
                    );
                } else {
                    widgets::paragraph(
                        ui,
                        "History is off (git is unavailable or history is disabled), so this replacement cannot be undone.",
                        FontId::proportional(13.0),
                        p.error,
                        1.45,
                        ui.available_width(),
                    );
                }
            },
            Some(&mut |ui: &mut Ui| {
                let btn = Button::new("Replace").kind(if history_on {
                    Kind::Primary
                } else {
                    Kind::Danger
                });
                if btn.show(ui, p).clicked() {
                    confirm = true;
                }
                if Button::new("Cancel").show(ui, p).clicked() {
                    cancel = true;
                }
            }),
        );
        if confirm {
            if let Some(s) = &mut self.search {
                s.confirm_replace = None;
            }
            self.replace_all(backend, settings);
        } else if (r.closed || cancel)
            && let Some(s) = &mut self.search
        {
            s.confirm_replace = None;
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

    #[test]
    fn a_new_replacement_keeps_the_lines_left_out_but_a_new_query_does_not() {
        let hits = || {
            let re = regex::Regex::new("foo").unwrap();
            vec![FileMatches {
                path: "a.tex".into(),
                matches: lemmata_shell::files::search_in("foo\nfoo\n", &re),
            }]
        };
        let mut s = ProjectSearch::default();
        s.take_results("foo".into(), hits());
        s.excluded.insert(("a.tex".into(), 2));
        s.take_results("foo".into(), hits());
        assert!(s.excluded.contains(&("a.tex".into(), 2)));
        s.take_results("bar".into(), hits());
        assert!(s.excluded.is_empty());
    }
}
