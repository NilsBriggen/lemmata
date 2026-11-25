//! Left-sidebar tool panels: review comments, TODOs, Git, plus the `SidePanel` switcher that
//! keeps every workspace tool beside the editor without replacing the PDF.
//! History lives in history.rs; Search in search.rs; Problems in pdf/problems.rs.

use super::{Tone, Workspace, chrome, tree};
use crate::backend::Backend;
use crate::icons;
use crate::panels::review;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, ChipTone, Kind, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Rect, Sense, Stroke, Ui, pos2, vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Files,
    Outline,
    Search,
    Problems,
    Git,
    History,
    Comments,
    Todos,
}

impl SidePanel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Outline => "outline",
            Self::Search => "search",
            Self::Problems => "problems",
            Self::Git => "git",
            Self::History => "history",
            Self::Comments => "comments",
            Self::Todos => "todos",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "outline" => Self::Outline,
            "search" => Self::Search,
            "problems" => Self::Problems,
            "git" => Self::Git,
            "history" => Self::History,
            "comments" => Self::Comments,
            "todos" => Self::Todos,
            _ => Self::Files,
        }
    }

    /// Files and Outline use the compact 232 px width; tool panels use 312 px.
    pub fn is_wide(self) -> bool {
        !matches!(self, Self::Files | Self::Outline)
    }
}

/// 32 px quiet sidebar header on `--app` with uppercase 11 px `faint` title and right actions.
/// Returns the remaining rect underneath.
pub(crate) fn side_head(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    right: impl FnOnce(&mut Ui),
) -> Rect {
    let rect = ui.max_rect();
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), 32.0));
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &title.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.0, theme::font::sans_600()),
            color: p.faint,
            extra_letter_spacing: 0.77,
            ..Default::default()
        },
    );
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(head.left() + 12.0, head.top()),
                pos2(head.right() - 8.0, head.bottom()),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    left.label(job);
    let mut acts = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(head.left() + 12.0, head.top()),
                pos2(head.right() - 8.0, head.bottom()),
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    acts.spacing_mut().item_spacing.x = 2.0;
    right(&mut acts);
    Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max)
}

/// The comments of the project and what the review panel is doing.
#[derive(Default)]
pub struct Comments {
    pub threads: Vec<review::Thread>,
    pub active: Option<String>,
    /// A new comment being written: file, line, quote.
    pub draft: Option<(String, usize, String)>,
    pub ui: review::Review,
    /// Bumped when threads or the active one change, so editors mark them again.
    pub generation: u64,
    /// comments.json exists but couldn't be read: saving would replace every comment in it.
    pub broken: Option<String>,
    /// A save is under way; `again` asks for one more with the newest threads when it ends, so
    /// an older save can never land last.
    saving: bool,
    again: bool,
}

#[derive(Default)]
pub struct Todos {
    pub list: Option<Result<Vec<lemmata_shell::tools::Todo>, String>>,
    pub busy: bool,
}

#[derive(Default)]
pub struct Git {
    pub overview: Option<lemmata_git::Overview>,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub commits: Vec<lemmata_git::Commit>,
    pub stashes: Vec<String>,
    pub message: String,
    /// The text of a commit under way: back in the box if the commit didn't happen.
    sent: Option<String>,
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    pub info: Option<String>,
    pub new_branch: Option<String>,
    pub add_remote: Option<(String, String)>,
    pub confirm_delete_branch: Option<String>,
    pub stash_msg: Option<String>,
    /// Inline diff viewer for a changed file: `(path, staged, diff_text)`.
    pub diff_view: Option<(String, bool, String)>,
    /// Files with changes git hasn't committed, for the tree and the toolbar's count.
    pub status: Vec<(String, String)>,
    pub status_pending: bool,
    pub status_dirty: bool,
}

impl Git {
    pub fn clear_repo(&mut self) {
        self.overview = None;
        self.branches.clear();
        self.remote_branches.clear();
        self.commits.clear();
        self.stashes.clear();
        self.diff_view = None;
        self.add_remote = None;
        self.confirm_delete_branch = None;
        self.stash_msg = None;
        self.status.clear();
        self.status_pending = false;
        self.status_dirty = false;
    }
}

impl Workspace {
    /// Opens a panel in the left sidebar without replacing the PDF pane.
    pub fn open_side(
        &mut self,
        panel: SidePanel,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        self.side_panel = panel;
        self.focus_mode = false;
        settings.sidebar_open = true;
        settings.side_panel = panel.as_str().into();
        match panel {
            SidePanel::Todos => self.load_todos(backend),
            SidePanel::Git => self.load_git(backend),
            SidePanel::History => self.load_history(backend),
            SidePanel::Search => {
                if self.search.is_none() {
                    self.search = Some(Default::default());
                }
                if let Some(s) = &mut self.search {
                    s.focus();
                }
            }
            SidePanel::Files | SidePanel::Outline | SidePanel::Problems | SidePanel::Comments => {}
        }
        true
    }

    pub fn toggle_side(
        &mut self,
        panel: SidePanel,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        if settings.sidebar_open && !self.focus_mode && self.side_panel == panel {
            settings.sidebar_open = false;
            self.comments.draft = None;
            true
        } else {
            self.open_side(panel, backend, settings)
        }
    }

    /// Draws the active left-sidebar panel into `ui`. Returns true if settings changed.
    pub fn show_side_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        match self.side_panel {
            SidePanel::Files => {
                chrome::files_panel(ui, p, self, settings, backend);
                false
            }
            SidePanel::Outline => {
                chrome::outline_panel(ui, p, self, backend);
                false
            }
            SidePanel::Search => {
                self.search_panel(ui, p, backend, settings);
                false
            }
            SidePanel::Problems => self.problems_sidebar(ui, p, backend, settings),
            SidePanel::Git => {
                self.git_panel(ui, p, backend, settings);
                false
            }
            SidePanel::History => self.history_panel(ui, p, backend, settings),
            SidePanel::Comments => {
                self.review_panel(ui, p, backend, settings);
                false
            }
            SidePanel::Todos => {
                self.todos_panel(ui, p, backend);
                false
            }
        }
    }

    // ── Comments ──────────────────────────────────────────────────────────

    pub fn load_comments(&mut self, backend: &Backend) {
        let gen_then = self.comments.generation;
        Self::job(
            backend,
            |shell| shell.read_text(review::COMMENTS_FILE.into()),
            move |ws, _, r| {
                // A save or another load came first: this read is older than what is held.
                if ws.comments.generation != gen_then {
                    return;
                }
                let exists = ws.info.root.join(review::COMMENTS_FILE).exists();
                let (threads, broken) = match r.map(|t| review::parse(&t)) {
                    Ok(Some((t, false))) => (t, None),
                    Ok(Some((t, true))) => (t, Some("has entries Lemmata can't read".to_owned())),
                    Ok(None) => (Vec::new(), Some("isn't valid JSON".to_owned())),
                    Err(e) if exists => (Vec::new(), Some(e)),
                    Err(_) => (Vec::new(), None),
                };
                ws.comments.threads = threads;
                ws.comments.broken = broken;
                ws.comments.generation += 1;
            },
        );
    }

    pub(super) fn save_comments(&mut self, backend: &Backend, threads: Vec<review::Thread>) {
        self.comments.threads = threads;
        self.comments.generation += 1;
        // Shown, but not written over a file that couldn't be read.
        if let Some(why) = &self.comments.broken {
            let text = format!(
                "{} {why}, so comments aren't saved: that would replace the ones in it. Fix or remove the file, then reopen the project.",
                review::COMMENTS_FILE
            );
            self.notice(text, Tone::Error);
            return;
        }
        if self.comments.saving {
            self.comments.again = true;
            return;
        }
        self.comments.saving = true;
        let text = review::serialize(&self.comments.threads);
        let root = self.info.root.clone();
        Self::job(
            backend,
            move |shell| shell.write_text(&root, review::COMMENTS_FILE.into(), text, None),
            |ws, b, r| {
                ws.comments.saving = false;
                if let Err(e) = r {
                    ws.notice(format!("Comments could not be saved: {e}"), Tone::Error);
                }
                if std::mem::take(&mut ws.comments.again) {
                    ws.save_comments(b, ws.comments.threads.clone());
                }
            },
        );
    }

    /// Starts a comment on the selection (or the cursor's line).
    pub fn add_comment(
        &mut self,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        let target = self
            .active
            .clone()
            .filter(|a| !tree::is_viewer_file(a))
            .and_then(|a| {
                let ed = self.editors.get(&a)?;
                let r = ed.doc.selection.main();
                let n = ed.doc.line_of(r.from());
                let quote = if r.is_empty() {
                    ed.doc.line(n)
                } else {
                    ed.doc.slice(r.from(), r.to().min(r.from() + 2000))
                };
                (!quote.trim().is_empty()).then_some((a, n + 1, quote))
            });
        match target {
            Some(t) => {
                self.comments.draft = Some(t);
                self.comments.ui.clear_draft();
                self.open_side(SidePanel::Comments, backend, settings)
            }
            None => {
                self.notice(
                    "Select the text you want to comment on, then choose Add comment.",
                    Tone::Info,
                );
                false
            }
        }
    }

    /// Goes to a thread's text.
    pub fn show_thread(&mut self, backend: &Backend, t: &review::Thread) {
        self.comments.active = Some(t.id.clone());
        self.comments.generation += 1;
        let line = self
            .editors
            .get(&t.file)
            .and_then(|e| review::locate(&e.doc.text(), t.line, &t.quote))
            .map_or(t.line, |l| l.2);
        self.goto(backend, &t.file, line);
    }

    /// The open file's comment ranges (unresolved ones), for its editor.
    pub fn comment_ranges(&self, path: &str) -> Vec<(String, usize, usize, bool)> {
        let Some(ed) = self.editors.get(path) else {
            return Vec::new();
        };
        let mine: Vec<&review::Thread> = self
            .comments
            .threads
            .iter()
            .filter(|t| t.file == path && !t.resolved)
            .collect();
        if mine.is_empty() {
            return Vec::new();
        }
        let text = ed.doc.text();
        mine.into_iter()
            .filter_map(|t| {
                let (a, b, _) = review::locate(&text, t.line, &t.quote)?;
                (b > a).then(|| {
                    (
                        t.id.clone(),
                        a,
                        b,
                        self.comments.active.as_deref() == Some(t.id.as_str()),
                    )
                })
            })
            .collect()
    }

    /// After a save: keep the anchors' lines current, so they're found again after big edits.
    pub fn move_comments(&mut self, backend: &Backend, path: &str, text: &str) {
        let mut moved = false;
        let next: Vec<review::Thread> = self
            .comments
            .threads
            .iter()
            .map(|t| {
                let mut t = t.clone();
                if t.file == path
                    && let Some((_, _, line)) = review::locate(text, t.line, &t.quote)
                    && line != t.line
                {
                    t.line = line;
                    moved = true;
                }
                t
            })
            .collect();
        if moved {
            self.save_comments(backend, next);
        }
    }

    fn review_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        _settings: &mut crate::store::Settings,
    ) {
        let draft = self.comments.draft.clone();
        let threads = self.comments.threads.clone();
        let active = self.comments.active.clone();
        let actions = self.comments.ui.show(
            ui,
            p,
            &threads,
            active.as_deref(),
            draft.as_ref().map(|(f, l, q)| (f.as_str(), *l, q.as_str())),
        );
        let now = crate::when::now_ms();
        for a in actions {
            match a {
                review::Action::Add(body) => {
                    if let Some((file, line, quote)) = self.comments.draft.take() {
                        let t = review::Thread {
                            id: review::new_id(now),
                            file,
                            line,
                            quote,
                            body,
                            created: now,
                            resolved: false,
                            replies: Vec::new(),
                        };
                        self.comments.active = Some(t.id.clone());
                        let mut next = self.comments.threads.clone();
                        next.push(t);
                        self.save_comments(backend, next);
                    }
                }
                review::Action::CancelDraft => self.comments.draft = None,
                review::Action::Select(t) => self.show_thread(backend, &t),
                review::Action::Reply(id, body) => {
                    let next = threads
                        .iter()
                        .map(|t| {
                            let mut t = t.clone();
                            if t.id == id {
                                t.replies.push(review::Reply {
                                    body: body.clone(),
                                    created: now,
                                });
                            }
                            t
                        })
                        .collect();
                    self.save_comments(backend, next);
                }
                review::Action::Resolve(id, resolved) => {
                    let next = threads
                        .iter()
                        .map(|t| {
                            let mut t = t.clone();
                            if t.id == id {
                                t.resolved = resolved;
                            }
                            t
                        })
                        .collect();
                    self.save_comments(backend, next);
                }
                review::Action::Delete(id) => self.save_comments(
                    backend,
                    threads.iter().filter(|t| t.id != id).cloned().collect(),
                ),
            }
        }
    }

    // ── TODOs ─────────────────────────────────────────────────────────────

    pub fn load_todos(&mut self, backend: &Backend) {
        self.todos.busy = true;
        Self::job(
            backend,
            |shell| shell.todos(),
            |ws, _, r| {
                ws.todos.busy = false;
                ws.todos.list = Some(r);
            },
        );
    }

    /// Todos panel: the project's TODO, FIXME and XXX comments and `\todo` notes, by file.
    fn todos_panel(&mut self, ui: &mut Ui, p: &Palette, backend: &Backend) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.app);
        let count = self
            .todos
            .list
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map_or(0, Vec::len);
        let busy = self.todos.busy;
        let mut refresh = false;
        let rest = side_head(ui, p, "To-dos", |ui| {
            if Button::icon(icons::REFRESH_CW, "Refresh TODOs")
                .size(Size::Sm)
                .icon_size(14.0)
                .enabled(!busy)
                .show_icon(ui, p)
                .on_hover_text("Refresh")
                .clicked()
            {
                refresh = true;
            }
            if count > 0 {
                widgets::chip(ui, p, None, &count.to_string(), ChipTone::Neutral);
            }
        });
        if refresh {
            self.load_todos(backend);
        }
        let mut go = None;
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        egui::ScrollArea::vertical()
            .id_salt("todos")
            .auto_shrink([false, false])
            .show(&mut c, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: 10,
                        right: 10,
                        top: 6,
                        bottom: 16,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &self.todos.list {
                            Some(Err(e)) => {
                                ui.label(egui::RichText::new(e).size(12.5).color(p.error));
                            }
                            Some(Ok(list)) if list.is_empty() && !busy => {
                                ui.label(
                                    egui::RichText::new("No TODOs in the project.")
                                        .size(12.5)
                                        .color(p.muted),
                                );
                            }
                            Some(Ok(list)) => {
                                let mut groups: Vec<(&str, Vec<&lemmata_shell::tools::Todo>)> =
                                    Vec::new();
                                for t in list {
                                    match groups.last_mut() {
                                        Some((f, v)) if *f == t.file => v.push(t),
                                        _ => groups.push((&t.file, vec![t])),
                                    }
                                }
                                for (gi, (file, todos)) in groups.iter().enumerate() {
                                    if gi > 0 {
                                        ui.add_space(10.0);
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(*file)
                                                .font(FontId::new(11.5, theme::font::sans_600()))
                                                .color(p.ink_2),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(todos.len().to_string())
                                                        .font(FontId::monospace(10.5))
                                                        .color(p.faint),
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);
                                    for t in todos {
                                        let w = ui.available_width();
                                        let text = if t.text.is_empty() {
                                            "(no text)"
                                        } else {
                                            t.text.as_str()
                                        };
                                        let kg = ui.painter().layout_no_wrap(
                                            t.kind.clone(),
                                            FontId::new(10.0, theme::font::mono_500()),
                                            p.accent_text,
                                        );
                                        let kw = kg.size().x + 10.0;
                                        let line_lbl = format!(":{}", t.line);
                                        let lw = ui
                                            .painter()
                                            .layout_no_wrap(
                                                line_lbl.clone(),
                                                FontId::monospace(11.0),
                                                p.faint,
                                            )
                                            .size()
                                            .x;
                                        let galley = ui.painter().layout(
                                            text.to_owned(),
                                            FontId::proportional(12.5),
                                            p.ink,
                                            (w - kw - lw - 28.0).max(60.0),
                                        );
                                        let h = galley.size().y.max(18.0) + 12.0;
                                        let (r, resp) =
                                            ui.allocate_exact_size(vec2(w, h), Sense::click());
                                        resp.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::Role::Button,
                                                true,
                                                text,
                                            )
                                        });
                                        if resp.hovered() {
                                            ui.painter().rect_filled(
                                                r,
                                                CornerRadius::same(6),
                                                p.rail_active,
                                            );
                                        }
                                        let kr = Rect::from_min_size(
                                            pos2(r.left() + 6.0, r.top() + 6.0),
                                            vec2(kw, kg.size().y + 3.0),
                                        );
                                        ui.painter().rect_filled(
                                            kr,
                                            CornerRadius::same(4),
                                            p.accent_soft,
                                        );
                                        ui.painter().galley(
                                            kr.min + vec2(5.0, 1.5),
                                            kg,
                                            p.accent_text,
                                        );
                                        ui.painter().galley(
                                            pos2(kr.right() + 7.0, r.top() + 6.0),
                                            galley,
                                            p.ink,
                                        );
                                        ui.painter().text(
                                            pos2(r.right() - 6.0, r.top() + 6.0),
                                            egui::Align2::RIGHT_TOP,
                                            line_lbl,
                                            FontId::monospace(11.0),
                                            p.faint,
                                        );
                                        if resp
                                            .on_hover_text(format!("Go to line {}", t.line))
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            go = Some((t.file.clone(), t.line));
                                        }
                                        ui.add_space(2.0);
                                    }
                                }
                            }
                            None => {}
                        }
                    });
            });
        if let Some((f, l)) = go {
            self.goto(backend, &f, l);
        }
    }

    // ── Git ───────────────────────────────────────────────────────────────

    /// The tree's markers and the toolbar's count, a moment after files change.
    pub fn refresh_git_status(&mut self, backend: &Backend) {
        // A restricted project runs no git: its repository could bring hooks and filters.
        if !self.trusted {
            return;
        }
        if self.git.status_pending {
            self.git.status_dirty = true;
            return;
        }
        self.git.status_pending = true;
        Self::job(
            backend,
            |shell| shell.git_status().unwrap_or_default(),
            |ws, b, list| {
                ws.git.status_pending = false;
                ws.git.status = list
                    .into_iter()
                    .map(|f| (f.path, f.status.to_string()))
                    .collect();
                if std::mem::take(&mut ws.git.status_dirty) {
                    ws.refresh_git_status(b);
                }
            },
        );
        if self.side_panel == SidePanel::Git && self.git.busy.is_none() {
            self.load_git(backend);
        }
    }

    pub fn load_git(&mut self, backend: &Backend) {
        // A restricted project never runs git in the workspace repository.
        if !self.trusted {
            self.git.clear_repo();
            return;
        }
        Self::job_async(
            backend,
            |shell| async move {
                let o = shell.git_overview().await;
                let more = match &o {
                    Ok(lemmata_git::Overview::Repo(_)) => Some((
                        shell.git_branches().await.unwrap_or_default(),
                        shell.git_remote_branches().await.unwrap_or_default(),
                        shell.git_log().await.unwrap_or_default(),
                        shell.git_stash_list().await.unwrap_or_default(),
                    )),
                    _ => None,
                };
                (o, more)
            },
            |ws, _, (o, more)| {
                if !ws.trusted {
                    ws.git.clear_repo();
                    return;
                }
                match o {
                    Ok(o) => ws.git.overview = Some(o),
                    Err(e) => ws.git.error = Some(e),
                }
                if let Some((b, rb, c, st)) = more {
                    ws.git.branches = b;
                    ws.git.remote_branches = rb;
                    ws.git.commits = c;
                    ws.git.stashes = st;
                }
            },
        );
    }

    /// Runs one git action: shows what it's doing, reports the outcome, reloads.
    fn git_act<F, Fut>(
        &mut self,
        backend: &Backend,
        settings: &crate::store::Settings,
        label: &'static str,
        changed: bool,
        f: F,
    ) where
        F: FnOnce(std::sync::Arc<lemmata_shell::Shell>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Option<String>, String>> + Send + 'static,
    {
        if !self.trusted || self.git.busy.is_some() {
            return;
        }
        self.git.error = None;
        self.git.info = None;
        self.git.busy = Some(label);
        self.after_saving(backend, settings, move |ws, b| {
            if !ws.trusted {
                ws.git.busy = None;
                return;
            }
            Self::job_async(b, f, move |ws, b, r| {
                ws.git.busy = None;
                match r {
                    Ok(Some(out)) => ws.git.info = Some(out),
                    Ok(None) => {}
                    Err(e) => ws.git.error = Some(e),
                }
                if changed {
                    ws.after_git_change(b);
                }
                ws.load_git(b);
                ws.refresh_git_status(b);
            });
        });
    }

    /// After a commit, pull or branch switch, files may differ on disk.
    fn after_git_change(&mut self, backend: &Backend) {
        Self::job(
            backend,
            |shell| shell.project_info_cmd(),
            |ws, b, r| {
                if let Ok(info) = r {
                    ws.info = info;
                    ws.tree_rows = None;
                }
                let open: Vec<String> = ws
                    .editors
                    .keys()
                    .filter(|p| !ws.buffers.is_dirty(p))
                    .cloned()
                    .collect();
                for path in open {
                    let p = path.clone();
                    b.spawn(move |shell| {
                        crate::backend::Msg::DiskText(p.clone(), shell.read_text(p))
                    });
                }
                ws.load_comments(b);
                ws.refresh_outline(b);
                ws.build_soon(false, false);
            },
        );
    }

    /// Git sidebar panel: status, commit, pull and push, stashes, branches and recent commits.
    fn git_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &crate::store::Settings,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.app);
        let busy = self.git.busy;
        if busy.is_none()
            && let Some(sent) = self.git.sent.take()
            && self.git.info.as_deref() != Some("Committed.")
            && self.git.message.is_empty()
        {
            self.git.message = sent;
        }
        let mut refresh = false;
        let trusted = self.trusted;
        let rest = side_head(ui, p, "Git", |ui| {
            if trusted
                && Button::icon(icons::REFRESH_CW, "Refresh")
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .enabled(busy.is_none())
                    .show_icon(ui, p)
                    .on_hover_text("Refresh")
                    .clicked()
            {
                refresh = true;
            }
            if let Some(b) = busy {
                ui.label(egui::RichText::new(b).size(11.5).color(p.muted));
            }
        });
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        if !self.trusted {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(10, 8))
                .show(&mut c, |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.card_hairline))
                        .corner_radius(theme::R_MD)
                        .shadow(p.shadow_card)
                        .inner_margin(Margin::symmetric(12, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 10.0;
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(
                                        "Git is turned off in restricted projects because a repository can run hooks and filters. Trust this project in Project settings to use Git.",
                                    )
                                    .size(12.5)
                                    .color(p.muted),
                                )
                                .wrap(),
                            );
                            if Button::new("Project settings")
                                .kind(Kind::Primary)
                                .size(Size::Sm)
                                .show(ui, p)
                                .clicked()
                            {
                                self.open_project_trust_settings();
                            }
                        });
                });
            return;
        }
        if refresh {
            self.git_act(backend, settings, "Refreshing…", false, |_| async {
                Ok(None)
            });
        }
        let note = |ui: &mut Ui, t: &str, color: Color32| {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(t).size(12.0).color(color)).wrap());
                });
        };
        if let Some(e) = self.git.error.clone() {
            note(&mut c, &e, p.error);
        }
        if let Some(i) = self.git.info.clone() {
            note(&mut c, &i, p.muted);
        }
        let intro = |ui: &mut Ui, t: &str| {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.card_hairline))
                        .corner_radius(theme::R_MD)
                        .shadow(p.shadow_card)
                        .inner_margin(Margin::symmetric(12, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(egui::RichText::new(t).size(12.5).color(p.muted))
                                    .wrap(),
                            );
                        });
                });
        };
        match self.git.overview.clone() {
            None => intro(&mut c, "Loading…"),
            Some(lemmata_git::Overview::NoGit { message }) => intro(&mut c, &message),
            Some(lemmata_git::Overview::NotARepo) => {
                intro(
                    &mut c,
                    "This project isn't in a git repository. A repository keeps every commit you make, lets you go back to any of them, and lets you sync with GitHub, GitLab or Overleaf.",
                );
                c.horizontal(|ui| {
                    ui.add_space(10.0);
                    if Button::new("Turn this project into a git repository")
                        .kind(Kind::Primary)
                        .size(Size::Sm)
                        .enabled(busy.is_none())
                        .show(ui, p)
                        .clicked()
                    {
                        self.git_act(
                            backend,
                            settings,
                            "Setting up git…",
                            true,
                            |shell| async move {
                                shell
                                    .git_init()
                                    .await
                                    .map(|_| Some("This project is now a git repository.".into()))
                            },
                        );
                    }
                });
            }
            Some(lemmata_git::Overview::Repo(repo)) => {
                self.git_repo(&mut c, p, backend, settings, &repo)
            }
        }
    }

    fn git_repo(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &crate::store::Settings,
        repo: &lemmata_git::Status,
    ) {
        let busy = self.git.busy.is_some();
        let section = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            egui::Frame::NONE
                .inner_margin(Margin {
                    left: 10,
                    right: 10,
                    top: 0,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.card_hairline))
                        .corner_radius(theme::R_MD)
                        .shadow(p.shadow_card)
                        .inner_margin(Margin::symmetric(10, 9))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            add(ui);
                        });
                });
        };
        let dirty =
            !repo.staged.is_empty() || !repo.unstaged.is_empty() || !repo.conflicted.is_empty();
        egui::ScrollArea::vertical()
            .id_salt("git")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                // In-progress merge / rebase / cherry-pick banner with Abort button.
                if let Some(op) = repo.operation.as_deref() {
                    let mut abort = false;
                    section(ui, &mut |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                            ui.label(
                                egui::RichText::new(format!("A git {op} is in progress."))
                                    .size(12.0)
                                    .color(p.warn),
                            );
                            if Button::new(&format!("Abort {op}"))
                                .kind(Kind::Danger)
                                .size(Size::Sm)
                                .enabled(!busy)
                                .show(ui, p)
                                .clicked()
                            {
                                abort = true;
                            }
                        });
                    });
                    if abort {
                        self.git_act(backend, settings, "Aborting…", true, |s| async move {
                            s.git_abort().await.map(Some)
                        });
                    }
                }
                // Branch selector, new branch button, upstream status & add remote.
                let mut switch = None;
                let mut toggle_new = false;
                let mut toggle_remote = false;
                section(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        icons::show(ui, icons::GIT_BRANCH, 14.0, p.muted);
                        let mut cur = repo.branch.clone().unwrap_or_default();
                        let mut opts: Vec<(String, String)> = self
                            .git
                            .branches
                            .iter()
                            .map(|b| (b.clone(), b.clone()))
                            .collect();
                        if repo.branch.is_none() {
                            opts.insert(0, (String::new(), "(detached HEAD)".into()));
                        }
                        let o: Vec<(&str, &str)> =
                            opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                        let w = (ui.available_width() - 34.0).max(80.0);
                        ui.allocate_ui(vec2(w, 28.0), |ui| {
                            if widgets::select(ui, p, "Branch", &mut cur, &o) && !cur.is_empty() {
                                switch = Some(cur.clone());
                            }
                        });
                        if Button::icon(icons::PLUS, "New branch")
                            .size(Size::Sm)
                            .icon_size(14.0)
                            .enabled(!busy)
                            .show_icon(ui, p)
                            .on_hover_text("New branch from here")
                            .clicked()
                        {
                            toggle_new = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                        let ab = [(repo.ahead, "↑"), (repo.behind, "↓")]
                            .iter()
                            .filter(|(n, _)| *n > 0)
                            .map(|(n, a)| format!("{a}{n}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let sync = if repo.upstream.is_some() {
                            if ab.is_empty() {
                                "up to date".into()
                            } else {
                                ab
                            }
                        } else {
                            "no upstream".to_owned()
                        };
                        let tip = repo
                            .upstream
                            .as_ref()
                            .map_or("Not pushed yet".to_owned(), |u| format!("Tracking {u}"));
                        ui.label(egui::RichText::new(sync).size(11.5).color(p.muted))
                            .on_hover_text(tip);
                        if repo.upstream.is_none()
                            && Button::new("Add remote…")
                                .kind(Kind::Ghost)
                                .size(Size::Sm)
                                .enabled(!busy)
                                .show(ui, p)
                                .clicked()
                        {
                            toggle_remote = true;
                        }
                    });
                });
                if let Some(name) = switch {
                    self.git_act(
                        backend,
                        settings,
                        "Switching…",
                        true,
                        move |shell| async move { shell.git_switch(name).await.map(|_| None) },
                    );
                }
                if toggle_new {
                    self.git.new_branch = if self.git.new_branch.is_some() {
                        None
                    } else {
                        Some(String::new())
                    };
                }
                if toggle_remote {
                    self.git.add_remote = if self.git.add_remote.is_some() {
                        None
                    } else {
                        Some(("origin".into(), String::new()))
                    };
                }
                if let Some((mut rname, mut rurl)) = self.git.add_remote.clone() {
                    let mut submit_remote = false;
                    let mut cancel_remote = false;
                    section(ui, &mut |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            let nw = 90.0;
                            ui.allocate_ui(vec2(nw, 32.0), |ui| {
                                widgets::text_input(ui, p, &mut rname, "origin", true);
                            });
                            let uw = (ui.available_width() - 80.0).max(80.0);
                            ui.allocate_ui(vec2(uw, 32.0), |ui| {
                                let r = widgets::text_input(
                                    ui,
                                    p,
                                    &mut rurl,
                                    "https://github.com/user/repo.git",
                                    true,
                                );
                                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    submit_remote = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    cancel_remote = true;
                                }
                            });
                            if Button::new("Add")
                                .kind(Kind::Primary)
                                .size(Size::Sm)
                                .enabled(
                                    !busy && !rname.trim().is_empty() && !rurl.trim().is_empty(),
                                )
                                .show(ui, p)
                                .clicked()
                            {
                                submit_remote = true;
                            }
                        });
                    });
                    self.git.add_remote = (!cancel_remote).then_some((rname.clone(), rurl.clone()));
                    if submit_remote && !rname.trim().is_empty() && !rurl.trim().is_empty() {
                        self.git.add_remote = None;
                        self.git_act(
                            backend,
                            settings,
                            "Adding remote…",
                            false,
                            move |s| async move {
                                s.git_add_remote(rname, rurl)
                                    .await
                                    .map(|_| Some("Remote added.".into()))
                            },
                        );
                    }
                }
                if dirty && self.git.branches.len() > 1 {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 4))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Commit or stash your changes before switching branches.",
                                )
                                .size(12.0)
                                .color(p.muted),
                            );
                        });
                }
                if let Some(mut name) = self.git.new_branch.clone() {
                    let mut create = false;
                    let mut cancel = false;
                    section(ui, &mut |ui| {
                        ui.horizontal(|ui| {
                            let w = (ui.available_width() - 80.0).max(80.0);
                            ui.allocate_ui(vec2(w, 32.0), |ui| {
                                let r =
                                    widgets::text_input(ui, p, &mut name, "New branch name", false);
                                ui.ctx().accesskit_node_builder(r.id, |n| {
                                    n.set_label("New branch name")
                                });
                                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    create = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    cancel = true;
                                }
                            });
                            if Button::new("Create")
                                .kind(Kind::Primary)
                                .size(Size::Sm)
                                .enabled(!busy && !name.trim().is_empty())
                                .show(ui, p)
                                .clicked()
                            {
                                create = true;
                            }
                        });
                    });
                    self.git.new_branch = (!cancel).then_some(name.clone());
                    if create && !name.trim().is_empty() {
                        self.git.new_branch = None;
                        self.git_act(
                            backend,
                            settings,
                            "Creating branch…",
                            true,
                            move |shell| async move {
                                shell.git_create_branch(name).await.map(|_| None)
                            },
                        );
                    }
                }
                // Fetch, pull (including merge/rebase on divergence), push.
                let mut act: Option<u8> = None;
                section(ui, &mut |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        if Button::new("Fetch")
                            .size(Size::Sm)
                            .enabled(!busy)
                            .show(ui, p)
                            .clicked()
                        {
                            act = Some(0);
                        }
                        let pull = if repo.behind > 0 {
                            format!("Pull {}", repo.behind)
                        } else {
                            "Pull".into()
                        };
                        if Button::new(&pull)
                            .with_icon(icons::ARROW_DOWN)
                            .icon_size(13.0)
                            .size(Size::Sm)
                            .enabled(!busy)
                            .show(ui, p)
                            .clicked()
                        {
                            act = Some(1);
                        }
                        if repo.ahead > 0 && repo.behind > 0 {
                            if Button::new("Pull (merge)")
                                .size(Size::Sm)
                                .enabled(!busy)
                                .show(ui, p)
                                .clicked()
                            {
                                act = Some(3);
                            }
                            if Button::new("Pull (rebase)")
                                .size(Size::Sm)
                                .enabled(!busy)
                                .show(ui, p)
                                .clicked()
                            {
                                act = Some(4);
                            }
                        }
                        let push = if repo.ahead > 0 {
                            format!("Push {}", repo.ahead)
                        } else {
                            "Push".into()
                        };
                        if Button::new(&push)
                            .with_icon(icons::ARROW_UP)
                            .icon_size(13.0)
                            .size(Size::Sm)
                            .enabled(!busy)
                            .show(ui, p)
                            .clicked()
                        {
                            act = Some(2);
                        }
                    });
                });
                match act {
                    Some(0) => {
                        self.git_act(backend, settings, "Fetching…", false, |s| async move {
                            s.git_fetch().await.map(Some)
                        })
                    }
                    Some(1) => self.git_act(backend, settings, "Pulling…", true, |s| async move {
                        s.git_pull().await.map(Some)
                    }),
                    Some(2) => self.git_act(backend, settings, "Pushing…", false, |s| async move {
                        s.git_push().await.map(Some)
                    }),
                    Some(3) => self.git_act(backend, settings, "Merging…", true, |s| async move {
                        s.git_pull_mode(lemmata_git::PullMode::Merge)
                            .await
                            .map(Some)
                    }),
                    Some(4) => self.git_act(backend, settings, "Rebasing…", true, |s| async move {
                        s.git_pull_mode(lemmata_git::PullMode::Rebase)
                            .await
                            .map(Some)
                    }),
                    _ => {}
                }
                // The changes, to tick into the commit (clicking a filename opens an inline diff).
                let mut toggle: Option<(lemmata_git::FileChange, bool)> = None;
                let mut open_diff: Option<(String, bool)> = None;
                let mut stage_all = false;
                section(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if repo.staged.is_empty()
                        && repo.unstaged.is_empty()
                        && repo.untracked.is_empty()
                        && repo.conflicted.is_empty()
                    {
                        ui.label(
                            egui::RichText::new("No changes. Everything is committed.")
                                .size(12.5)
                                .color(p.muted),
                        );
                    }
                    let head =
                        |ui: &mut Ui, title: &str, n: usize, all: bool, stage_all: &mut bool| {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let mut job = egui::text::LayoutJob::default();
                                job.append(
                                    &title.to_uppercase(),
                                    0.0,
                                    egui::TextFormat {
                                        font_id: FontId::new(11.0, theme::font::sans_600()),
                                        color: p.muted,
                                        extra_letter_spacing: 0.44,
                                        ..Default::default()
                                    },
                                );
                                job.append(
                                    &format!(" {n}"),
                                    0.0,
                                    egui::TextFormat {
                                        font_id: FontId::proportional(11.0),
                                        color: p.muted,
                                        ..Default::default()
                                    },
                                );
                                ui.label(job);
                                if all {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if Button::new("Stage all")
                                                .kind(Kind::Ghost)
                                                .size(Size::Sm)
                                                .enabled(!busy)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                *stage_all = true;
                                            }
                                        },
                                    );
                                }
                            });
                        };
                    let color = |s: &str| match s {
                        "M" => p.warn,
                        "A" | "?" => p.ok,
                        "D" | "U" => p.error,
                        "R" => p.accent,
                        _ => p.ink,
                    };
                    if !repo.conflicted.is_empty() {
                        head(
                            ui,
                            "Conflicts",
                            repo.conflicted.len(),
                            false,
                            &mut stage_all,
                        );
                        ui.label(
                            egui::RichText::new(
                                "Resolve the conflict markers in these files, then stage them.",
                            )
                            .size(12.0)
                            .color(p.muted),
                        );
                        for c in &repo.conflicted {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("U")
                                        .font(FontId::monospace(12.0))
                                        .color(p.error),
                                );
                                ui.label(
                                    egui::RichText::new(&c.path)
                                        .font(FontId::monospace(12.0))
                                        .color(p.ink),
                                );
                            });
                        }
                    }
                    for (title, list, staged, tracked) in [
                        ("Staged", &repo.staged, true, true),
                        ("Changes", &repo.unstaged, false, true),
                        ("Untracked", &repo.untracked, false, false),
                    ] {
                        if list.is_empty() {
                            continue;
                        }
                        head(ui, title, list.len(), !staged, &mut stage_all);
                        for c in list {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let mut on = staged;
                                let r =
                                    ui.add_enabled(!busy, egui::Checkbox::without_text(&mut on));
                                ui.ctx()
                                    .accesskit_node_builder(r.id, |n| n.set_label(c.path.clone()));
                                ui.label(
                                    egui::RichText::new(&c.status)
                                        .font(FontId::new(12.0, theme::font::mono_500()))
                                        .color(color(&c.status)),
                                );
                                let tip = match &c.from {
                                    Some(f) => format!("{f} → {}", c.path),
                                    None if tracked => format!("{} (click to view diff)", c.path),
                                    None => c.path.clone(),
                                };
                                let lbl = ui
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new(&c.path)
                                                .font(FontId::monospace(12.0))
                                                .color(p.ink),
                                        )
                                        .truncate()
                                        .sense(
                                            if tracked {
                                                Sense::click()
                                            } else {
                                                Sense::hover()
                                            },
                                        ),
                                    )
                                    .on_hover_text(tip);
                                if tracked
                                    && lbl
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                {
                                    open_diff = Some((c.path.clone(), staged));
                                }
                                if r.changed() {
                                    toggle = Some((c.clone(), staged));
                                }
                            });
                        }
                    }
                });
                if let Some((path, staged)) = open_diff {
                    let p_clone = path.clone();
                    Self::job_async(
                        backend,
                        move |s| async move { s.git_diff(Some(p_clone), staged).await },
                        move |ws, _, r| match r {
                            Ok(diff) => ws.git.diff_view = Some((path, staged, diff)),
                            Err(e) => ws.git.error = Some(e),
                        },
                    );
                }
                if let Some((dpath, dstaged, dtext)) = self.git.diff_view.clone() {
                    let mut close_diff = false;
                    section(ui, &mut |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{} ({})",
                                        dpath,
                                        if dstaged { "staged" } else { "unstaged" }
                                    ))
                                    .font(FontId::new(12.0, theme::font::mono_500()))
                                    .color(p.ink),
                                )
                                .truncate(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if Button::icon(icons::X, "Close diff")
                                        .kind(Kind::Ghost)
                                        .size(Size::Sm)
                                        .icon_size(13.0)
                                        .show_icon(ui, p)
                                        .clicked()
                                    {
                                        close_diff = true;
                                    }
                                },
                            );
                        });
                        egui::Frame::new()
                            .fill(p.code_bg)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(crate::theme::R_SM)
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                egui::ScrollArea::both()
                                    .id_salt("git-diff-view")
                                    .max_height(220.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        if dtext.trim().is_empty() {
                                            ui.label(
                                                egui::RichText::new("No text differences.")
                                                    .size(11.5)
                                                    .color(p.muted),
                                            );
                                        } else {
                                            for line in dtext.lines().take(500) {
                                                let col = if line.starts_with('+')
                                                    && !line.starts_with("+++")
                                                {
                                                    p.ok
                                                } else if line.starts_with('-')
                                                    && !line.starts_with("---")
                                                {
                                                    p.error
                                                } else if line.starts_with("@@") {
                                                    p.accent
                                                } else {
                                                    p.ink
                                                };
                                                ui.label(
                                                    egui::RichText::new(line)
                                                        .font(FontId::monospace(11.5))
                                                        .color(col),
                                                );
                                            }
                                        }
                                    });
                            });
                    });
                    if close_diff {
                        self.git.diff_view = None;
                    }
                }
                if stage_all {
                    self.git_act(backend, settings, "Staging…", false, |s| async move {
                        s.git_stage_all().await.map(|_| None)
                    });
                }
                if let Some((c, staged)) = toggle {
                    self.git_act(backend, settings, "Updating…", false, move |s| async move {
                        if staged {
                            let mut paths = vec![c.path];
                            paths.extend(c.from);
                            s.git_unstage(paths).await.map(|_| None)
                        } else {
                            s.git_stage(vec![c.path]).await.map(|_| None)
                        }
                    });
                }
                // The commit box.
                let mut commit = false;
                let blocker = if repo.staged.is_empty() {
                    Some("Tick the files to include in the commit.")
                } else if self.git.message.trim().is_empty() {
                    Some("Write a commit message.")
                } else {
                    None
                };
                section(ui, &mut |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let r = widgets::text_area(
                        ui,
                        p,
                        &mut self.git.message,
                        "Commit message (Ctrl+Enter to commit)",
                        3,
                        !busy,
                    );
                    ui.ctx()
                        .accesskit_node_builder(r.id, |n| n.set_label("Commit message"));
                    if r.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command)
                        && blocker.is_none()
                    {
                        commit = true;
                    }
                    let label = if repo.staged.is_empty() {
                        "Commit".to_owned()
                    } else {
                        format!(
                            "Commit {} file{}",
                            repo.staged.len(),
                            if repo.staged.len() == 1 { "" } else { "s" }
                        )
                    };
                    let b = Button::new(&label)
                        .kind(Kind::Primary)
                        .enabled(blocker.is_none() && !busy)
                        .min_width(ui.available_width())
                        .show(ui, p);
                    let b = if let Some(t) = blocker {
                        b.on_disabled_hover_text(t)
                    } else {
                        b
                    };
                    if b.clicked() {
                        commit = true;
                    }
                });
                if commit {
                    let message = std::mem::take(&mut self.git.message);
                    self.git.sent = Some(message.clone());
                    self.git_act(
                        backend,
                        settings,
                        "Committing…",
                        true,
                        move |s| async move {
                            s.git_commit(message)
                                .await
                                .map(|_| Some("Committed.".into()))
                        },
                    );
                }
                // Stashes & branch management.
                let mut stash_push: Option<Option<String>> = None;
                let mut stash_pop = false;
                let mut checkout_remote: Option<String> = None;
                let mut delete_branch: Option<String> = None;
                if dirty || !self.git.stashes.is_empty() {
                    section(ui, &mut |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!("STASHES {}", self.git.stashes.len()))
                                .font(FontId::new(11.0, theme::font::sans_600()))
                                .color(p.muted),
                        )
                        .id_salt("git-stashes")
                        .default_open(!self.git.stashes.is_empty())
                        .show(ui, |ui| {
                            if dirty {
                                if let Some(mut msg) = self.git.stash_msg.clone() {
                                    let mut go = false;
                                    let mut cancel = false;
                                    ui.horizontal(|ui| {
                                        let w = (ui.available_width() - 80.0).max(80.0);
                                        ui.allocate_ui(vec2(w, 28.0), |ui| {
                                            let r = widgets::text_input(
                                                ui,
                                                p,
                                                &mut msg,
                                                "Optional message",
                                                false,
                                            );
                                            if r.lost_focus()
                                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                            {
                                                go = true;
                                            }
                                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                                cancel = true;
                                            }
                                        });
                                        if Button::new("Stash")
                                            .kind(Kind::Primary)
                                            .size(Size::Sm)
                                            .enabled(!busy)
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            go = true;
                                        }
                                    });
                                    if cancel {
                                        self.git.stash_msg = None;
                                    } else if go {
                                        self.git.stash_msg = None;
                                        let m = msg.trim();
                                        stash_push = Some((!m.is_empty()).then(|| m.to_owned()));
                                    } else {
                                        self.git.stash_msg = Some(msg);
                                    }
                                } else if Button::new("Stash changes")
                                    .size(Size::Sm)
                                    .enabled(!busy)
                                    .show(ui, p)
                                    .clicked()
                                {
                                    self.git.stash_msg = Some(String::new());
                                }
                            }
                            for (i, entry) in self.git.stashes.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(entry)
                                                .font(FontId::monospace(11.5))
                                                .color(p.ink),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(entry);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if i == 0
                                                && Button::new("Pop")
                                                    .kind(Kind::Ghost)
                                                    .size(Size::Sm)
                                                    .enabled(!busy)
                                                    .show(ui, p)
                                                    .clicked()
                                            {
                                                stash_pop = true;
                                            }
                                        },
                                    );
                                });
                            }
                        });
                    });
                }
                let other_local: Vec<String> = self
                    .git
                    .branches
                    .iter()
                    .filter(|b| repo.branch.as_deref() != Some(b.as_str()))
                    .cloned()
                    .collect();
                if !other_local.is_empty() || !self.git.remote_branches.is_empty() {
                    section(ui, &mut |ui| {
                        egui::CollapsingHeader::new(
                            egui::RichText::new("BRANCHES")
                                .font(FontId::new(11.0, theme::font::sans_600()))
                                .color(p.muted),
                        )
                        .id_salt("git-branches")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            for b in &other_local {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(b)
                                                .font(FontId::monospace(12.0))
                                                .color(p.ink),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if self.git.confirm_delete_branch.as_deref()
                                                == Some(b.as_str())
                                            {
                                                if Button::new("Cancel")
                                                    .kind(Kind::Ghost)
                                                    .size(Size::Sm)
                                                    .show(ui, p)
                                                    .clicked()
                                                {
                                                    self.git.confirm_delete_branch = None;
                                                }
                                                if Button::new("Delete")
                                                    .kind(Kind::Danger)
                                                    .size(Size::Sm)
                                                    .enabled(!busy)
                                                    .show(ui, p)
                                                    .clicked()
                                                {
                                                    self.git.confirm_delete_branch = None;
                                                    delete_branch = Some(b.clone());
                                                }
                                            } else if Button::icon(
                                                icons::TRASH,
                                                &format!("Delete branch {b}"),
                                            )
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .icon_size(13.0)
                                            .enabled(!busy)
                                            .show_icon(ui, p)
                                            .on_hover_text("Delete merged branch")
                                            .clicked()
                                            {
                                                self.git.confirm_delete_branch = Some(b.clone());
                                            }
                                        },
                                    );
                                });
                            }
                            if !self.git.remote_branches.is_empty() {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("REMOTE")
                                        .font(FontId::new(10.5, theme::font::sans_600()))
                                        .color(p.muted),
                                );
                                for rb in &self.git.remote_branches {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(rb)
                                                    .font(FontId::monospace(12.0))
                                                    .color(p.muted),
                                            )
                                            .truncate(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if Button::new("Checkout")
                                                    .kind(Kind::Ghost)
                                                    .size(Size::Sm)
                                                    .enabled(!busy)
                                                    .show(ui, p)
                                                    .clicked()
                                                {
                                                    checkout_remote = Some(rb.clone());
                                                }
                                            },
                                        );
                                    });
                                }
                            }
                        });
                    });
                }
                if let Some(msg) = stash_push {
                    let message = msg.unwrap_or_default();
                    self.git_act(backend, settings, "Stashing…", true, move |s| async move {
                        s.git_stash_push(message).await.map(Some)
                    });
                }
                if stash_pop {
                    self.git_act(backend, settings, "Applying stash…", true, |s| async move {
                        s.git_stash_pop().await.map(Some)
                    });
                }
                if let Some(rb) = checkout_remote {
                    self.git_act(
                        backend,
                        settings,
                        "Checking out…",
                        true,
                        move |s| async move { s.git_checkout_remote(rb).await.map(|_| None) },
                    );
                }
                if let Some(b) = delete_branch {
                    self.git_act(
                        backend,
                        settings,
                        "Deleting branch…",
                        false,
                        move |s| async move {
                            s.git_delete_branch(b)
                                .await
                                .map(|_| Some("Branch deleted.".into()))
                        },
                    );
                }
                // The latest commits.
                egui::Frame::NONE
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!("HISTORY {}", self.git.commits.len()))
                                .font(FontId::new(11.0, theme::font::sans_600()))
                                .color(p.muted),
                        )
                        .id_salt("git-log")
                        .default_open(true)
                        .show(ui, |ui| {
                            if self.git.commits.is_empty() {
                                ui.label(
                                    egui::RichText::new("No commits yet.")
                                        .size(12.0)
                                        .color(p.muted),
                                );
                            }
                            for c in &self.git.commits {
                                ui.add_space(5.0);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&c.subject).color(p.ink))
                                        .truncate(),
                                )
                                .on_hover_text(&c.subject);
                                let mut job = egui::text::LayoutJob::default();
                                job.append(
                                    &c.hash,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: FontId::monospace(11.0),
                                        color: p.muted,
                                        ..Default::default()
                                    },
                                );
                                job.append(
                                    &format!(" · {} · {}", c.author, c.date),
                                    0.0,
                                    egui::TextFormat {
                                        font_id: FontId::proportional(11.0),
                                        color: p.muted,
                                        ..Default::default()
                                    },
                                );
                                ui.label(job);
                                ui.add_space(5.0);
                                let y = ui.cursor().top();
                                ui.painter().hline(
                                    ui.max_rect().x_range(),
                                    y,
                                    Stroke::new(1.0, p.border),
                                );
                            }
                        });
                    });
            });
    }
}
