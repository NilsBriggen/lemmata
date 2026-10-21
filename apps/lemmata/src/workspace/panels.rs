//! The panels that take the PDF's place (App.svelte `panel`): review comments, TODOs and Git.
//! History lives in history.rs.

use super::{Tone, Workspace, tree};
use crate::backend::Backend;
use crate::icons;
use crate::panels::{bar, review};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, Color32, FontId, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Panel {
    Review,
    History,
    Git,
    Todos,
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
    pub commits: Vec<lemmata_git::Commit>,
    pub message: String,
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    pub info: Option<String>,
    pub new_branch: Option<String>,
    /// Files with changes git hasn't committed, for the tree and the toolbar's count.
    pub status: Vec<(String, String)>,
}

impl Workspace {
    /// Shows a panel where the PDF is, leaving focus mode and "Editor only" so it can be seen.
    pub fn open_panel(
        &mut self,
        panel: Panel,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        self.panel = Some(panel);
        self.focus_mode = false;
        match panel {
            Panel::Todos => self.load_todos(backend),
            Panel::Git => self.load_git(backend),
            Panel::History => self.load_history(backend),
            Panel::Review => {}
        }
        if settings.layout == "editor" {
            settings.layout = "split".into();
            return true;
        }
        false
    }

    pub fn toggle_panel(
        &mut self,
        panel: Panel,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        if self.panel == Some(panel) {
            self.panel = None;
            self.comments.draft = None;
            false
        } else {
            self.open_panel(panel, backend, settings)
        }
    }

    /// Draws the open panel into `ui`.
    pub fn show_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        match self.panel {
            Some(Panel::Review) => self.review_panel(ui, p, backend, settings),
            Some(Panel::Todos) => self.todos_panel(ui, p, backend),
            Some(Panel::Git) => self.git_panel(ui, p, backend),
            Some(Panel::History) => self.history_panel(ui, p, backend, settings),
            None => {}
        }
    }

    // ── Comments ──────────────────────────────────────────────────────────

    pub fn load_comments(&mut self, backend: &Backend) {
        Self::job(
            backend,
            |shell| shell.read_text(review::COMMENTS_FILE.into()),
            |ws, _, r| {
                let exists = ws.info.root.join(review::COMMENTS_FILE).exists();
                let (threads, broken) = match r.map(|t| review::parse(&t)) {
                    Ok(Some(t)) => (t, None),
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

    fn save_comments(&mut self, backend: &Backend, threads: Vec<review::Thread>) {
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
                self.open_panel(Panel::Review, backend, settings)
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
                review::Action::Close => {
                    self.panel = None;
                    self.comments.draft = None;
                }
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

    /// Todos.svelte: the project's TODO, FIXME and XXX comments and `\todo` notes, by file.
    fn todos_panel(&mut self, ui: &mut Ui, p: &Palette, backend: &Backend) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface_2);
        let count = self
            .todos
            .list
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map_or(0, Vec::len);
        let busy = self.todos.busy;
        let mut refresh = false;
        let b = bar(ui, p, "TODOs", "Close TODOs", &mut |ui| {
            ui.label(
                egui::RichText::new(count.to_string())
                    .size(13.0)
                    .color(p.faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::icon(icons::REFRESH_CW, "Refresh TODOs")
                    .size(Size::Sm)
                    .icon_size(15.0)
                    .enabled(!busy)
                    .show_icon(ui, p)
                    .on_hover_text("Refresh")
                    .clicked()
                {
                    refresh = true;
                }
            });
        });
        if b.closed {
            self.panel = None;
        }
        if refresh {
            self.load_todos(backend);
        }
        let mut go = None;
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(b.rest));
        egui::ScrollArea::vertical()
            .id_salt("todos")
            .auto_shrink([false, false])
            .show(&mut c, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: 12,
                        right: 12,
                        top: 10,
                        bottom: 16,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &self.todos.list {
                            Some(Err(e)) => {
                                ui.label(egui::RichText::new(e).size(13.0).color(p.error));
                            }
                            Some(Ok(list)) if list.is_empty() && !busy => {
                                ui.label(
                                    egui::RichText::new("No TODOs in the project.")
                                        .size(13.0)
                                        .color(p.muted),
                                );
                            }
                            Some(Ok(list)) => {
                                // Grouped by file, in the order they come (sorted by file, then line).
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
                                        let y = ui.cursor().top();
                                        ui.painter().hline(
                                            ui.max_rect().x_range(),
                                            y,
                                            Stroke::new(1.0, p.border),
                                        );
                                        ui.add_space(10.0);
                                    }
                                    widgets::Rich::new()
                                        .add(
                                            file,
                                            FontId::new(12.0, theme::font::sans_600()),
                                            p.muted,
                                        )
                                        .add(
                                            &format!(" {}", todos.len()),
                                            FontId::new(12.0, theme::font::sans_500()),
                                            p.faint,
                                        )
                                        .wrap(ui.available_width())
                                        .show(ui);
                                    ui.add_space(4.0);
                                    for t in todos {
                                        let w = ui.available_width();
                                        let text = if t.text.is_empty() {
                                            "(no text)"
                                        } else {
                                            t.text.as_str()
                                        };
                                        let galley = ui.painter().layout(
                                            text.to_owned(),
                                            FontId::proportional(13.0),
                                            p.ink,
                                            w - 120.0,
                                        );
                                        let h = galley.size().y.max(18.0) + 10.0;
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
                                            ui.painter().rect_filled(r, 5, p.surface_3);
                                        }
                                        // .kind: a small outlined accent tag.
                                        let kg = ui.painter().layout_no_wrap(
                                            t.kind.clone(),
                                            FontId::new(10.0, theme::font::mono_500()),
                                            p.accent,
                                        );
                                        let kr = Rect::from_min_size(
                                            pos2(r.left() + 6.0, r.top() + 6.0),
                                            kg.size() + vec2(10.0, 2.0),
                                        );
                                        ui.painter().rect_stroke(
                                            kr,
                                            4,
                                            Stroke::new(1.0, p.accent),
                                            egui::StrokeKind::Inside,
                                        );
                                        ui.painter().galley(kr.min + vec2(5.0, 1.0), kg, p.accent);
                                        ui.painter().galley(
                                            pos2(kr.right() + 8.0, r.top() + 5.0),
                                            galley,
                                            p.ink,
                                        );
                                        ui.painter().text(
                                            pos2(r.right() - 6.0, r.top() + 5.0),
                                            egui::Align2::RIGHT_TOP,
                                            format!(":{}", t.line),
                                            FontId::monospace(12.0),
                                            p.faint,
                                        );
                                        if resp
                                            .on_hover_text(format!("Go to line {}", t.line))
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
        Self::job(
            backend,
            |shell| shell.git_status().unwrap_or_default(),
            |ws, _, list| {
                ws.git.status = list
                    .into_iter()
                    .map(|f| (f.path, f.status.to_string()))
                    .collect()
            },
        );
        if self.panel == Some(Panel::Git) && self.git.busy.is_none() {
            self.load_git(backend);
        }
    }

    pub fn load_git(&mut self, backend: &Backend) {
        Self::job_async(
            backend,
            |shell| async move {
                let o = shell.git_overview().await;
                let more = match &o {
                    Ok(lemmata_git::Overview::Repo(_)) => Some((
                        shell.git_branches().await.unwrap_or_default(),
                        shell.git_log().await.unwrap_or_default(),
                    )),
                    _ => None,
                };
                (o, more)
            },
            |ws, _, (o, more)| {
                match o {
                    Ok(o) => ws.git.overview = Some(o),
                    Err(e) => ws.git.error = Some(e),
                }
                if let Some((b, c)) = more {
                    ws.git.branches = b;
                    ws.git.commits = c;
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
        if self.git.busy.is_some() {
            return;
        }
        self.git.busy = Some(label);
        self.git.error = None;
        self.git.info = None;
        // Unsaved edits are saved first, so git sees them.
        self.save_all_quietly(backend, settings);
        Self::job_async(backend, f, move |ws, b, r| {
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
                ws.refresh_outline(b);
                ws.build_soon(false, false);
            },
        );
    }

    /// GitPanel.svelte: status, commit, pull and push, branches and recent commits.
    fn git_panel(&mut self, ui: &mut Ui, p: &Palette, backend: &Backend) {
        let settings = crate::store::Settings::default();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface_2);
        let busy = self.git.busy;
        let mut refresh = false;
        let b = bar(ui, p, "Git", "Close Git", &mut |ui| {
            if let Some(b) = busy {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(b).size(12.0).color(p.muted));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::icon(icons::REFRESH_CW, "Refresh")
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .enabled(busy.is_none())
                    .show_icon(ui, p)
                    .on_hover_text("Refresh")
                    .clicked()
                {
                    refresh = true;
                }
            });
        });
        if b.closed {
            self.panel = None;
        }
        if refresh {
            self.git_act(backend, &settings, "Refreshing…", false, |_| async {
                Ok(None)
            });
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(b.rest));
        let note = |ui: &mut Ui, t: &str, color: Color32| {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(t).size(12.5).color(color)).wrap());
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
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_max_width(480.0);
                    ui.add(egui::Label::new(egui::RichText::new(t).color(p.muted)).wrap());
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
                    ui.add_space(14.0);
                    if Button::new("Turn this project into a git repository")
                        .kind(Kind::Primary)
                        .enabled(busy.is_none())
                        .show(ui, p)
                        .clicked()
                    {
                        self.git_act(
                            backend,
                            &settings,
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
            Some(lemmata_git::Overview::Repo(repo)) => self.git_repo(&mut c, p, backend, &repo),
        }
    }

    fn git_repo(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        repo: &lemmata_git::Status,
    ) {
        let settings = crate::store::Settings::default();
        let busy = self.git.busy.is_some();
        let section = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            let r = egui::Frame::NONE
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                });
            ui.painter().hline(
                r.response.rect.x_range(),
                r.response.rect.bottom() - 0.5,
                Stroke::new(1.0, p.border),
            );
        };
        let dirty =
            !repo.staged.is_empty() || !repo.unstaged.is_empty() || !repo.conflicted.is_empty();
        egui::ScrollArea::vertical()
            .id_salt("git")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // The branch, a new branch, and where it stands against its upstream.
                let mut switch = None;
                let mut toggle_new = false;
                section(ui, &mut |ui| {
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
                        let w = ui.available_width() - 28.0 - 90.0;
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
                        ui.label(egui::RichText::new(sync).size(12.0).color(p.muted))
                            .on_hover_text(tip);
                    });
                });
                if let Some(name) = switch {
                    self.git_act(
                        backend,
                        &settings,
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
                if dirty && self.git.branches.len() > 1 {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 4))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Commit your changes before switching branches.",
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
                            let w = ui.available_width() - 80.0;
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
                            &settings,
                            "Creating branch…",
                            true,
                            move |shell| async move {
                                shell.git_create_branch(name).await.map(|_| None)
                            },
                        );
                    }
                }
                // Fetch, pull, push.
                let mut act: Option<u8> = None;
                section(ui, &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
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
                        self.git_act(backend, &settings, "Fetching…", false, |s| async move {
                            s.git_fetch().await.map(Some)
                        })
                    }
                    Some(1) => self.git_act(backend, &settings, "Pulling…", true, |s| async move {
                        s.git_pull().await.map(Some)
                    }),
                    Some(2) => {
                        self.git_act(backend, &settings, "Pushing…", false, |s| async move {
                            s.git_push().await.map(Some)
                        })
                    }
                    _ => {}
                }
                // The changes, to tick into the commit.
                let mut toggle: Option<(lemmata_git::FileChange, bool)> = None;
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
                            egui::RichText::new("Resolve these in a terminal, then stage them.")
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
                    for (title, list, staged) in [
                        ("Staged", &repo.staged, true),
                        ("Changes", &repo.unstaged, false),
                        ("Untracked", &repo.untracked, false),
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
                                    None => c.path.clone(),
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&c.path)
                                            .font(FontId::monospace(12.0))
                                            .color(p.ink),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(tip);
                                if r.changed() {
                                    toggle = Some((c.clone(), staged));
                                }
                            });
                        }
                    }
                });
                if stage_all {
                    self.git_act(backend, &settings, "Staging…", false, |s| async move {
                        s.git_stage_all().await.map(|_| None)
                    });
                }
                if let Some((c, staged)) = toggle {
                    self.git_act(
                        backend,
                        &settings,
                        "Updating…",
                        false,
                        move |s| async move {
                            if staged {
                                let mut paths = vec![c.path];
                                paths.extend(c.from);
                                s.git_unstage(paths).await.map(|_| None)
                            } else {
                                s.git_stage(vec![c.path]).await.map(|_| None)
                            }
                        },
                    );
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
                    self.git_act(
                        backend,
                        &settings,
                        "Committing…",
                        true,
                        move |s| async move {
                            s.git_commit(message)
                                .await
                                .map(|_| Some("Committed.".into()))
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
