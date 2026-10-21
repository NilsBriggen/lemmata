//! History (HistoryPanel.svelte and App.svelte's snapshots): snapshots of the project, a diff of
//! any changed file, one-file and whole-project restores, and reviewing the changes since a
//! snapshot as hunks to accept or reject. Snapshots live in git inside `.lemmata`.

use super::{Tone, Workspace};
use crate::backend::{Backend, Msg};
use crate::icons;
use crate::panels::{bar, hunks};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, Color32, FontId, Margin, Stroke, Ui, vec2};
use lemmata_history::{Change, Snapshot};
use lemmata_shell::history::HistoryStatus;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// While the user edits, a snapshot every ten minutes.
const AUTO_SNAPSHOT: Duration = Duration::from_secs(10 * 60);
const REVIEWED: &str = "Reviewed";

pub struct ReviewFile {
    pub path: String,
    pub old: String,
    pub hunks: Vec<hunks::Hunk>,
}

pub struct History {
    pub status: Option<HistoryStatus>,
    pub snapshots: Vec<Snapshot>,
    pub error: Option<String>,
    pub on: bool,
    open: Option<String>,
    compare: Option<String>,
    compared: Vec<Change>,
    /// A file's diff: snapshot id, path, the other snapshot (None: the current text), the texts.
    diff: Option<(String, String, Option<String>, String, String)>,
    confirm_restore: bool,
    editing: Option<(String, String)>,
    confirm_all: Option<String>,
    /// "": since the last snapshot marked as reviewed.
    pub(crate) since: String,
    review: Option<(Snapshot, Vec<ReviewFile>)>,
    accepted: HashSet<String>,
    pub busy: Option<&'static str>,
    pdf: Option<(String, String)>,
    /// The "Save a snapshot" dialog's name, while it's open.
    pub label: Option<String>,
    /// When the last snapshot was taken (None: none yet this session, so the first save takes one).
    pub last: Option<Instant>,
    pub edited: bool,
}

impl Default for History {
    fn default() -> Self {
        Self {
            status: None,
            snapshots: Vec::new(),
            error: None,
            on: true,
            open: None,
            compare: None,
            compared: Vec::new(),
            diff: None,
            confirm_restore: false,
            editing: None,
            confirm_all: None,
            since: String::new(),
            review: None,
            accepted: HashSet::new(),
            busy: None,
            pdf: None,
            label: None,
            last: None,
            edited: false,
        }
    }
}

impl Workspace {
    /// On opening a project: history is turned on unless git is missing.
    pub fn ensure_history(&mut self, backend: &Backend) {
        Self::job(
            backend,
            |shell| {
                let s = shell.history_status()?;
                if matches!(s, HistoryStatus::NotARepo) {
                    shell.history_enable()?;
                }
                Ok::<_, String>(!matches!(s, HistoryStatus::NoGit { .. }))
            },
            |ws, b, r| match r {
                Ok(on) => {
                    ws.history.on = on;
                    ws.load_history(b);
                }
                Err(e) => {
                    ws.history.on = false;
                    ws.notice(format!("History is off for this project: {e}"), Tone::Info);
                }
            },
        );
    }

    pub fn load_history(&mut self, backend: &Backend) {
        Self::job(
            backend,
            |shell| {
                let s = shell.history_status()?;
                let list = if matches!(s, HistoryStatus::On) {
                    shell.history_list()?
                } else {
                    Vec::new()
                };
                Ok::<_, String>((s, list))
            },
            |ws, _, r| match r {
                Ok((s, list)) => {
                    ws.history.status = Some(s);
                    ws.history.snapshots = list;
                }
                Err(e) => ws.history.error = Some(e),
            },
        );
    }

    pub fn take_snapshot(&mut self, backend: &Backend, label: String) {
        self.history.last = Some(Instant::now());
        self.history.edited = false;
        Self::job(
            backend,
            move |shell| shell.history_snapshot(label),
            |ws, b, r| {
                if matches!(r, Ok(Some(_))) {
                    ws.load_history(b);
                }
            },
        );
    }

    /// After a save: the first of a session takes a snapshot, later ones every ten minutes.
    pub fn snapshot_after_save(&mut self, backend: &Backend) {
        self.history.edited = true;
        if self.history.on && self.history.last.is_none() {
            self.take_snapshot(backend, "Automatic snapshot".into());
        }
    }

    pub fn snapshot_timer(&mut self, backend: &Backend) {
        if self.history.on
            && self.history.edited
            && self
                .history
                .last
                .is_some_and(|t| t.elapsed() > AUTO_SNAPSHOT)
        {
            self.take_snapshot(backend, "Automatic snapshot".into());
        }
    }

    /// Ctrl+Shift+S's dialog: "Save a snapshot".
    pub fn snapshot_dialog(&mut self, ctx: &egui::Context, p: &Palette, backend: &Backend) {
        let Some(mut label) = self.history.label.clone() else {
            return;
        };
        let mut enter = false;
        let mut save = false;
        let mut cancel = false;
        let r = widgets::modal_with(
            ctx,
            p,
            "Save a snapshot",
            Some("Give it a name you'll recognise later, like “Sent to supervisor”."),
            440.0,
            |ui| {
                let r = widgets::text_input(ui, p, &mut label, "Snapshot", false);
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Snapshot name"));
                if !r.has_focus() && label.is_empty() {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    enter = true;
                }
            },
            Some(&mut |ui: &mut Ui| {
                if Button::new("Save snapshot")
                    .kind(Kind::Primary)
                    .show(ui, p)
                    .clicked()
                {
                    save = true;
                }
                if Button::new("Cancel").show(ui, p).clicked() {
                    cancel = true;
                }
            }),
        );
        if r.closed || cancel {
            self.history.label = None;
        } else if save || enter {
            self.history.label = None;
            let l = label.trim();
            self.take_snapshot(
                backend,
                if l.is_empty() {
                    "Snapshot".into()
                } else {
                    l.to_owned()
                },
            );
        } else {
            self.history.label = Some(label);
        }
    }

    /// The current text of a file: the editor's (with unsaved edits) or the disk's.
    fn current_text(&self, path: &str) -> Option<String> {
        match self.editors.get(path) {
            Some(e) => Some(e.doc.text()),
            None => std::fs::read_to_string(self.info.root.join(path)).ok(),
        }
    }

    /// A restore runs once everything is saved (so it's in the safety snapshot); if something
    /// couldn't be saved, nothing is restored.
    fn after_saving(
        &mut self,
        backend: &Backend,
        settings: &crate::store::Settings,
        restore: impl FnOnce(&mut Workspace, &Backend) + 'static,
    ) {
        self.save_all_quietly(backend, settings);
        self.after_writes.push(Box::new(move |ws, b| {
            let left = ws.buffers.unsaved();
            if left.is_empty() {
                restore(ws, b);
            } else {
                ws.history.error = Some(format!(
                    "{} could not be saved, so nothing was restored.",
                    left.join(", ")
                ));
            }
        }));
    }

    /// Reads a file again from disk after a restore, a replace or a rename changed it. Edits made
    /// since stay unsaved, so the disk text then asks which version to keep instead of replacing them.
    pub(crate) fn reload(&mut self, backend: &Backend, path: &str) {
        if self.editors.contains_key(path) {
            let p = path.to_owned();
            backend.spawn(move |shell| Msg::DiskText(p.clone(), shell.read_text(p)));
        }
    }

    pub(super) fn history_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface_2);
        let can_snap = !matches!(self.history.status, Some(HistoryStatus::NotARepo) | None);
        let mut snap = false;
        let b = bar(ui, p, "History", "Close history", &mut |ui| {
            ui.add_space(4.0);
            if can_snap
                && Button::new("Save a snapshot")
                    .with_icon(icons::PLUS)
                    .icon_size(13.0)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .show(ui, p)
                    .clicked()
            {
                snap = true;
            }
        });
        if b.closed {
            self.panel = None;
        }
        if snap {
            self.history.label = Some(String::new());
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(b.rest));
        let ui = &mut c;
        if let Some(e) = self.history.error.clone() {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(e).color(p.error)).wrap());
                });
        }
        let intro = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    add(ui);
                });
        };
        let muted = |ui: &mut Ui, t: &str| {
            ui.add(egui::Label::new(egui::RichText::new(t).color(p.muted)).wrap());
        };
        match self.history.status.clone() {
            None => {}
            Some(HistoryStatus::NotARepo) => {
                let mut enable = false;
                intro(ui, &mut |ui| {
                    muted(
                        ui,
                        "History keeps snapshots of your project so you can see what changed and bring back earlier versions. Snapshots are taken every 10 minutes while you work, and whenever you press Ctrl+Shift+S.",
                    );
                    muted(
                        ui,
                        "It uses git and keeps the snapshots in the project's .lemmata folder; the project folder itself doesn't change.",
                    );
                    if Button::new("Turn on history")
                        .kind(Kind::Primary)
                        .show(ui, p)
                        .clicked()
                    {
                        enable = true;
                    }
                });
                if enable {
                    Self::job(
                        backend,
                        |s| s.history_enable(),
                        |ws, b, r| match r {
                            Ok(()) => ws.load_history(b),
                            Err(e) => ws.history.error = Some(e),
                        },
                    );
                }
            }
            Some(HistoryStatus::NoGit { message }) => intro(ui, &mut |ui| muted(ui, &message)),
            Some(HistoryStatus::On) => {
                if self.history.diff.is_some() {
                    self.history_diff(ui, p, backend, settings);
                } else if self.history.review.is_some() {
                    self.history_review(ui, p, backend, settings);
                } else if self.history.snapshots.is_empty() {
                    intro(ui, &mut |ui| {
                        muted(
                            ui,
                            "No snapshots yet. The first one is taken after your next save.",
                        )
                    });
                } else {
                    self.history_list(ui, p, backend, settings);
                }
            }
        }
    }

    fn base_snapshot(&self) -> Option<Snapshot> {
        let h = &self.history;
        if h.since.is_empty() {
            h.snapshots
                .iter()
                .find(|s| s.label.starts_with(REVIEWED))
                .cloned()
        } else {
            h.snapshots.iter().find(|s| s.id == h.since).cloned()
        }
    }

    fn history_list(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        let busy = self.history.busy;
        let last_reviewed = self
            .history
            .snapshots
            .iter()
            .find(|s| s.label.starts_with(REVIEWED))
            .cloned();
        let base = self.base_snapshot();
        // The review bar.
        let mut act: Option<&str> = None;
        let r = egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.label(egui::RichText::new("Review changes since").size(12.0).color(p.muted));
                let first = match &last_reviewed {
                    Some(s) => format!("my last review · {}", crate::when::medium_secs(s.time)),
                    None => "Choose a snapshot…".into(),
                };
                let mut opts: Vec<(String, String)> = vec![(String::new(), first)];
                opts.extend(self.history.snapshots.iter().map(|s| (s.id.clone(), format!("{} · {}", s.label, crate::when::medium_secs(s.time)))));
                let o: Vec<(&str, &str)> = opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                ui.allocate_ui(vec2(220.0, 26.0), |ui| widgets::select(ui, p, "Review changes since", &mut self.history.since, &o));
                if Button::new(if busy == Some("review") { "Looking…" } else { "Review changes" }).size(Size::Sm).enabled(base.is_some() && busy.is_none()).show(ui, p).clicked() {
                    act = Some("review");
                }
                if Button::new(if busy == Some("pdf") { "Making the PDF…" } else { "PDF of the changes" })
                    .with_icon(icons::FILE_TEXT)
                    .icon_size(13.0)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .enabled(base.is_some() && busy.is_none())
                    .show(ui, p)
                    .on_hover_text("Compiles the old and the new version with latexdiff into one PDF with the changes marked.")
                    .clicked()
                {
                    act = Some("pdf");
                }
                if let Some((id, path)) = self.history.pdf.clone()
                    && base.as_ref().is_some_and(|b| b.id == id)
                    && Button::new("Open PDF of the changes").kind(Kind::Primary).size(Size::Sm).show(ui, p).clicked()
                {
                    backend.spawn(move |shell| Msg::Done(shell.open_project_file(path)));
                }
                if Button::new("Mark as reviewed")
                    .with_icon(icons::CHECK)
                    .icon_size(13.0)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .enabled(busy.is_none())
                    .show(ui, p)
                    .on_hover_text("Takes a snapshot called Reviewed, where the next review starts.")
                    .clicked()
                {
                    act = Some("mark");
                }
            });
        });
        ui.painter().hline(
            r.response.rect.x_range(),
            r.response.rect.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
        match act {
            Some("review") => self.start_review(backend, settings),
            Some("pdf") => self.make_pdf(backend, settings),
            Some("mark") => self.mark_reviewed(backend, settings),
            _ => {}
        }

        let snapshots = self.history.snapshots.clone();
        let mut show_diff = None;
        egui::ScrollArea::vertical().id_salt("history").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for s in &snapshots {
                let open = self.history.open.as_deref() == Some(s.id.as_str());
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), egui::Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, &s.label));
                let fill = if open {
                    p.accent_soft
                } else if resp.hovered() {
                    p.surface_3
                } else {
                    p.surface
                };
                ui.painter().rect_filled(r, 0, fill);
                ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
                let editing = self.history.editing.as_ref().filter(|e| e.0 == s.id).cloned();
                if let Some((_, mut text)) = editing {
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(r.min + vec2(14.0, 6.0), vec2(w - 28.0, 24.0))));
                    let t = c.add(egui::TextEdit::singleline(&mut text).desired_width(w - 28.0).font(FontId::proportional(13.0)));
                    c.ctx().accesskit_node_builder(t.id, |n| n.set_label("Snapshot name"));
                    t.request_focus();
                    if t.lost_focus() {
                        self.history.editing = None;
                        let (id, l) = (s.id.clone(), text.trim().to_owned());
                        if !l.is_empty() && !c.input(|i| i.key_pressed(egui::Key::Escape)) {
                            Self::job(backend, move |sh| sh.history_relabel(id, l), |ws, b, r| match r {
                                Ok(()) => ws.load_history(b),
                                Err(e) => ws.history.error = Some(e),
                            });
                        }
                    } else {
                        self.history.editing = Some((s.id.clone(), text));
                    }
                } else {
                    ui.painter().text(r.min + vec2(14.0, 10.0), egui::Align2::LEFT_TOP, &s.label, FontId::new(13.0, theme::font::sans_500()), p.ink);
                }
                let n = s.changes.len();
                ui.painter().text(
                    r.min + vec2(14.0, 30.0),
                    egui::Align2::LEFT_TOP,
                    format!("{} · {n} file{}", crate::when::medium_secs(s.time), if n == 1 { "" } else { "s" }),
                    FontId::proportional(12.0),
                    p.muted,
                );
                if resp.clicked() {
                    self.history.open = if open { None } else { Some(s.id.clone()) };
                    self.history.compare = None;
                    self.history.confirm_all = None;
                }
                if !open {
                    continue;
                }
                // .tools: rename, restore the project, compare with another snapshot.
                let mut compare_changed = false;
                let r = egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        if Button::new("Rename").with_icon(icons::PENCIL).icon_size(13.0).kind(Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                            self.history.editing = Some((s.id.clone(), s.label.clone()));
                        }
                        if self.history.confirm_all.as_deref() == Some(s.id.as_str()) {
                            ui.label(egui::RichText::new("Replace all files? Your current work is saved as a snapshot first.").size(12.0).color(p.ink));
                            if Button::new("Cancel").size(Size::Sm).show(ui, p).clicked() {
                                self.history.confirm_all = None;
                            }
                            if Button::new("Restore").kind(Kind::Primary).size(Size::Sm).show(ui, p).clicked() {
                                self.restore_all(backend, settings, s.id.clone());
                            }
                        } else if Button::new("Restore project").with_icon(icons::ROTATE_CCW).icon_size(13.0).kind(Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                            self.history.confirm_all = Some(s.id.clone());
                        }
                        ui.label(egui::RichText::new("Compare with").size(12.0).color(p.muted));
                        let mut opts: Vec<(String, String)> = vec![(String::new(), "this snapshot's own changes".into())];
                        opts.extend(snapshots.iter().filter(|o| o.id != s.id).map(|o| (o.id.clone(), format!("{} · {}", o.label, crate::when::medium_secs(o.time)))));
                        let o: Vec<(&str, &str)> = opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                        let mut cur = self.history.compare.clone().unwrap_or_default();
                        ui.allocate_ui(vec2(220.0, 26.0), |ui| {
                            if widgets::select(ui, p, "Compare with", &mut cur, &o) {
                                compare_changed = true;
                            }
                        });
                        self.history.compare = (!cur.is_empty()).then_some(cur);
                    });
                });
                ui.painter().hline(r.response.rect.x_range(), r.response.rect.bottom() - 0.5, Stroke::new(1.0, p.border));
                if compare_changed {
                    self.history.compared.clear();
                    if let Some(other) = self.history.compare.clone() {
                        let a = s.id.clone();
                        Self::job(backend, move |sh| sh.history_compare(a.clone(), other.clone()).map(|c| (a, other, c)), |ws, _, r| match r {
                            Ok((a, b, c)) if ws.history.open.as_deref() == Some(&a) && ws.history.compare.as_deref() == Some(&b) => ws.history.compared = c,
                            Ok(_) => {}
                            Err(e) => ws.history.error = Some(e),
                        });
                    }
                }
                // The changed files: a click shows the diff.
                let files = if self.history.compare.is_some() { self.history.compared.clone() } else { s.changes.clone() };
                for ch in files {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), egui::Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0, Palette::mix(p.accent, 8.0));
                    }
                    let (letter, color) = match ch.status {
                        "added" => ("A", p.ok),
                        "deleted" => ("D", p.error),
                        _ => ("M", p.warn),
                    };
                    ui.painter().text(r.min + vec2(26.0, 11.0), egui::Align2::LEFT_CENTER, letter, FontId::monospace(12.0), color);
                    ui.painter().text(r.min + vec2(46.0, 11.0), egui::Align2::LEFT_CENTER, &ch.path, FontId::monospace(12.0), p.ink);
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &ch.path));
                    if resp.clicked() {
                        show_diff = Some((s.id.clone(), ch.path.clone()));
                    }
                }
            }
        });
        if let Some((id, path)) = show_diff {
            let other = self.history.compare.clone();
            let now = if other.is_none() {
                self.current_text(&path)
            } else {
                None
            };
            Self::job(
                backend,
                move |sh| {
                    let then = sh
                        .history_file(id.clone(), path.clone())
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let now = match &other {
                        Some(o) => sh
                            .history_file(o.clone(), path.clone())
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                        None => now.unwrap_or_default(),
                    };
                    (id, path, other, then, now)
                },
                |ws, _, d| {
                    ws.history.diff = Some(d);
                    ws.history.confirm_restore = false;
                },
            );
        }
    }

    /// The diff of one file: removed lines red, added green, long unchanged stretches folded.
    fn history_diff(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        let Some((id, path, other, then, now)) = self.history.diff.clone() else {
            return;
        };
        let mut back = false;
        let mut restore = false;
        let r = egui::Frame::NONE.inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                if Button::new("Back").with_icon(icons::ARROW_LEFT).icon_size(14.0).kind(Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                    back = true;
                }
                ui.label(egui::RichText::new(&path).font(FontId::monospace(12.0)).color(p.muted));
                if self.history.confirm_restore {
                    ui.label(egui::RichText::new("Replace the current file? The current version is kept as a snapshot.").size(12.0).color(p.ink));
                    if Button::new("Cancel").size(Size::Sm).show(ui, p).clicked() {
                        self.history.confirm_restore = false;
                    }
                    if Button::new("Restore").kind(Kind::Primary).size(Size::Sm).show(ui, p).clicked() {
                        restore = true;
                    }
                } else if Button::new("Restore this version").size(Size::Sm).show(ui, p).clicked() {
                    self.history.confirm_restore = true;
                }
            });
        });
        ui.painter().hline(
            r.response.rect.x_range(),
            r.response.rect.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
        egui::Frame::NONE
            .inner_margin(Margin::symmetric(12, 4))
            .show(ui, |ui| {
                let other_name = if other.is_some() {
                    "the other snapshot"
                } else {
                    "the current version"
                };
                ui.label(
                    egui::RichText::new(format!(
                        "Red lines were in the snapshot, green lines are in {other_name}."
                    ))
                    .size(11.0)
                    .color(p.muted),
                );
            });
        if back {
            self.history.diff = None;
            return;
        }
        if restore {
            let (i, pa) = (id.clone(), path.clone());
            self.after_saving(backend, settings, move |_, b| {
                Self::job(
                    b,
                    move |sh| sh.history_restore(i, pa.clone()).map(|_| pa),
                    |ws, b, r| match r {
                        Ok(path) => {
                            ws.reload(b, &path);
                            ws.history.diff = None;
                            ws.history.confirm_restore = false;
                            ws.load_history(b);
                        }
                        Err(e) => ws.history.error = Some(e),
                    },
                );
            });
        }
        let rest = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rest, 0, p.code_bg);
        let hs = hunks::compute(&then, &now);
        let lines: Vec<&str> = now.split_inclusive('\n').collect();
        // Rows: (line number in the new text or None, text, kind) with kind 0 same, 1 removed, 2 added, 3 folded.
        let mut rows: Vec<(Option<usize>, String, u8)> = Vec::new();
        let mut line = 0usize; // index into `lines`
        let push_same = |rows: &mut Vec<(Option<usize>, String, u8)>,
                         from: usize,
                         to: usize,
                         first: bool,
                         last: bool| {
            let n = to - from;
            // collapseUnchanged: { margin: 3, minSize: 6 }
            let keep_head = if first { 0 } else { 3 };
            let keep_tail = if last { 0 } else { 3 };
            if n >= keep_head + keep_tail + 6 {
                for (k, l) in lines.iter().enumerate().take(from + keep_head).skip(from) {
                    rows.push((Some(k + 1), l.trim_end_matches('\n').to_owned(), 0));
                }
                rows.push((
                    None,
                    format!("{} unchanged lines", n - keep_head - keep_tail),
                    3,
                ));
                for (k, l) in lines.iter().enumerate().take(to).skip(to - keep_tail) {
                    rows.push((Some(k + 1), l.trim_end_matches('\n').to_owned(), 0));
                }
            } else {
                for (k, l) in lines.iter().enumerate().take(to).skip(from) {
                    rows.push((Some(k + 1), l.trim_end_matches('\n').to_owned(), 0));
                }
            }
        };
        for (i, h) in hs.iter().enumerate() {
            let start = now[..h.new_from].matches('\n').count();
            push_same(&mut rows, line, start, i == 0, false);
            for l in h.old_text.split_inclusive('\n') {
                rows.push((None, l.trim_end_matches('\n').to_owned(), 1));
            }
            let n = h.new_text.split_inclusive('\n').count();
            for (k, l) in lines.iter().enumerate().take(start + n).skip(start) {
                rows.push((Some(k + 1), l.trim_end_matches('\n').to_owned(), 2));
            }
            line = start + n;
        }
        push_same(&mut rows, line, lines.len(), hs.is_empty(), true);
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        egui::ScrollArea::vertical()
            .id_salt("history-diff")
            .auto_shrink([false, false])
            .show(&mut c, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let font = FontId::monospace(12.5);
                for (n, text, kind) in rows {
                    let w = ui.available_width();
                    let g = ui.painter().layout(
                        text.clone(),
                        font.clone(),
                        p.ink,
                        (w - 56.0).max(40.0),
                    );
                    let h = g.size().y.max(18.0) + 2.0;
                    let (r, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
                    let fill = match kind {
                        1 => Color32::from_rgba_unmultiplied(0xee, 0x44, 0x33, 40),
                        2 => Color32::from_rgba_unmultiplied(0x22, 0xbb, 0x22, 34),
                        3 => p.surface_3,
                        _ => Color32::TRANSPARENT,
                    };
                    ui.painter().rect_filled(r, 0, fill);
                    if let Some(n) = n {
                        ui.painter().text(
                            r.min + vec2(40.0, 1.0),
                            egui::Align2::RIGHT_TOP,
                            n.to_string(),
                            font.clone(),
                            p.muted,
                        );
                    }
                    if kind == 3 {
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("⋯ {text}"),
                            FontId::proportional(12.0),
                            p.muted,
                        );
                    } else {
                        ui.painter().galley(r.min + vec2(52.0, 1.0), g, p.ink);
                    }
                }
            });
    }

    fn start_review(&mut self, backend: &Backend, settings: &crate::store::Settings) {
        let Some(base) = self.base_snapshot() else {
            return;
        };
        self.history.error = None;
        self.history.busy = Some("review");
        self.save_all_quietly(backend, settings);
        let snapshots = self.history.snapshots.clone();
        // The open files' text, so unsaved edits count.
        let texts: Vec<(String, String)> = self
            .editors
            .iter()
            .map(|(p, e)| (p.clone(), e.doc.text()))
            .collect();
        let root = self.info.root.clone();
        Self::job(
            backend,
            move |sh| {
                // The work since the last snapshot counts too.
                let _ = sh.history_snapshot("Before review".into());
                let list = sh.history_list().unwrap_or(snapshots);
                let idx = list.iter().position(|s| s.id == base.id).unwrap_or(0);
                let mut paths: Vec<String> = list[..idx]
                    .iter()
                    .flat_map(|s| s.changes.iter().map(|c| c.path.clone()))
                    .filter(|p| hunks::is_text_path(p))
                    .collect();
                paths.sort();
                paths.dedup();
                let mut files = Vec::new();
                for path in paths {
                    let old = sh
                        .history_file(base.id.clone(), path.clone())
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let now = texts
                        .iter()
                        .find(|t| t.0 == path)
                        .map(|t| t.1.clone())
                        .or_else(|| std::fs::read_to_string(root.join(&path)).ok())
                        .unwrap_or_default();
                    let h = hunks::compute(&old, &now);
                    if !h.is_empty() {
                        files.push(ReviewFile {
                            path,
                            old,
                            hunks: h,
                        });
                    }
                }
                (base, list, files)
            },
            |ws, _, (base, list, files)| {
                ws.history.busy = None;
                ws.history.snapshots = list;
                ws.history.accepted.clear();
                ws.history.review = Some((base, files));
            },
        );
    }

    fn make_pdf(&mut self, backend: &Backend, settings: &crate::store::Settings) {
        let Some(base) = self.base_snapshot() else {
            return;
        };
        self.history.error = None;
        self.history.busy = Some("pdf");
        self.save_all_quietly(backend, settings);
        let id = base.id.clone();
        Self::job(
            backend,
            move |sh| sh.history_latexdiff(id.clone()).map(|p| (id, p)),
            |ws, _, r| {
                ws.history.busy = None;
                match r {
                    Ok((id, p)) => ws.history.pdf = Some((id, p.to_string_lossy().into_owned())),
                    Err(e) => ws.history.error = Some(e),
                }
            },
        );
    }

    fn mark_reviewed(&mut self, backend: &Backend, settings: &crate::store::Settings) {
        self.history.error = None;
        self.history.busy = Some("mark");
        self.save_all_quietly(backend, settings);
        let latest = self.history.snapshots.first().cloned();
        Self::job(
            backend,
            move |sh| {
                if sh.history_snapshot(REVIEWED.into())?.is_none()
                    && let Some(l) = latest
                    && !l.label.starts_with(REVIEWED)
                {
                    // Nothing changed since the newest snapshot: it is the reviewed state.
                    let label = if l.label == "Snapshot" || l.label == "Automatic snapshot" {
                        REVIEWED.to_owned()
                    } else {
                        format!("{REVIEWED}: {}", l.label)
                    };
                    sh.history_relabel(l.id, label)?;
                }
                Ok::<_, String>(())
            },
            |ws, b, r| {
                ws.history.busy = None;
                ws.history.since.clear();
                ws.history.review = None;
                if let Err(e) = r {
                    ws.history.error = Some(e);
                }
                ws.load_history(b);
            },
        );
    }

    fn restore_all(&mut self, backend: &Backend, settings: &crate::store::Settings, id: String) {
        self.after_saving(backend, settings, move |_, backend| {
            Self::restore_all_now(backend, id)
        });
    }

    fn restore_all_now(backend: &Backend, id: String) {
        Self::job(
            backend,
            move |sh| sh.history_restore_all(id),
            |ws, b, r| match r {
                Ok(_) => {
                    ws.history.confirm_all = None;
                    let open: Vec<String> = ws.editors.keys().cloned().collect();
                    for p in open {
                        ws.reload(b, &p);
                    }
                    b.spawn(|shell| Msg::Info(shell.project_info_cmd()));
                    ws.load_history(b);
                    ws.build_soon(false, false);
                }
                Err(e) => ws.history.error = Some(e),
            },
        );
    }

    /// The hunks to accept or reject.
    fn history_review(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &crate::store::Settings,
    ) {
        let Some((base, files)) = self.history.review.take() else {
            return;
        };
        let accepted = self.history.accepted.clone();
        let pending = |f: &ReviewFile| -> Vec<hunks::Hunk> {
            f.hunks
                .iter()
                .filter(|h| !accepted.contains(&format!("{}\0{}", f.path, h.key)))
                .cloned()
                .collect()
        };
        let shown: Vec<usize> = (0..files.len())
            .filter(|&i| !pending(&files[i]).is_empty())
            .collect();
        let busy = self.history.busy;
        let mut back = false;
        let mut accept: Vec<(String, Vec<hunks::Hunk>)> = Vec::new();
        let mut reject: Vec<(String, Vec<hunks::Hunk>)> = Vec::new();
        let mut mark = false;
        let r = egui::Frame::NONE
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                    if Button::new("Back")
                        .with_icon(icons::ARROW_LEFT)
                        .icon_size(14.0)
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(ui, p)
                        .clicked()
                    {
                        back = true;
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Changes since {} · {}",
                            base.label,
                            crate::when::medium_secs(base.time)
                        ))
                        .font(FontId::monospace(12.0))
                        .color(p.muted),
                    );
                    if Button::new("Accept all")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .enabled(!shown.is_empty())
                        .show(ui, p)
                        .clicked()
                    {
                        accept.extend(
                            shown
                                .iter()
                                .map(|&i| (files[i].path.clone(), files[i].hunks.clone())),
                        );
                    }
                    if Button::new("Reject all")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .enabled(!shown.is_empty() && busy.is_none())
                        .show(ui, p)
                        .clicked()
                    {
                        reject.extend(
                            shown
                                .iter()
                                .map(|&i| (files[i].path.clone(), pending(&files[i]))),
                        );
                    }
                    if Button::new("Mark as reviewed")
                        .with_icon(icons::CHECK)
                        .icon_size(13.0)
                        .size(Size::Sm)
                        .enabled(busy.is_none())
                        .show(ui, p)
                        .clicked()
                    {
                        mark = true;
                    }
                });
            });
        ui.painter().hline(
            r.response.rect.x_range(),
            r.response.rect.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
        egui::Frame::NONE.inner_margin(Margin::symmetric(12, 4)).show(ui, |ui| {
            ui.label(egui::RichText::new("Accept keeps the current text. Reject puts the snapshot's text back for that change only; undo it with Ctrl+Z in the file.").size(11.0).color(p.muted));
        });
        egui::ScrollArea::vertical()
            .id_salt("history-review")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if shown.is_empty() {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(ui, |ui| {
                            let t = if files.is_empty() {
                                "Nothing changed since then."
                            } else {
                                "You have been through every change."
                            };
                            ui.label(egui::RichText::new(t).color(p.muted));
                        });
                }
                for &i in &shown {
                    let f = &files[i];
                    let pend = pending(f);
                    egui::Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&f.path)
                                        .font(FontId::new(12.0, theme::font::mono_500()))
                                        .color(p.ink),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} change{}",
                                        pend.len(),
                                        if pend.len() == 1 { "" } else { "s" }
                                    ))
                                    .size(11.0)
                                    .color(p.muted),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if Button::new("Reject all")
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .enabled(busy.is_none())
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            reject.push((f.path.clone(), pend.clone()));
                                        }
                                        if Button::new("Accept all")
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            accept.push((f.path.clone(), f.hunks.clone()));
                                        }
                                    },
                                );
                            });
                        });
                    for h in &pend {
                        egui::Frame::NONE
                            .inner_margin(Margin {
                                left: 12,
                                right: 12,
                                top: 4,
                                bottom: 8,
                            })
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("line {}", h.line))
                                            .size(11.0)
                                            .color(p.muted),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if Button::new("Reject")
                                                .kind(Kind::Ghost)
                                                .size(Size::Sm)
                                                .enabled(busy.is_none())
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                reject.push((f.path.clone(), vec![h.clone()]));
                                            }
                                            if Button::new("Accept")
                                                .kind(Kind::Ghost)
                                                .size(Size::Sm)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                accept.push((f.path.clone(), vec![h.clone()]));
                                            }
                                        },
                                    );
                                });
                                for (text, color) in [(&h.old_text, p.error), (&h.new_text, p.ok)] {
                                    if text.is_empty() {
                                        continue;
                                    }
                                    let lines: Vec<&str> =
                                        text.trim_end_matches('\n').split('\n').collect();
                                    let shown_text = if lines.len() > 14 {
                                        format!(
                                            "{}\n… {} more lines",
                                            lines[..14].join("\n"),
                                            lines.len() - 14
                                        )
                                    } else {
                                        lines.join("\n")
                                    };
                                    egui::Frame::new()
                                        .fill(Palette::mix(color, 14.0))
                                        .corner_radius(3)
                                        .inner_margin(Margin::symmetric(8, 3))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(shown_text)
                                                        .font(FontId::monospace(12.0))
                                                        .color(p.ink),
                                                )
                                                .wrap(),
                                            );
                                        });
                                    ui.add_space(2.0);
                                }
                            });
                    }
                    ui.painter().hline(
                        ui.max_rect().x_range(),
                        ui.cursor().top(),
                        Stroke::new(1.0, p.border),
                    );
                }
            });
        for (path, hs) in accept {
            for h in hs {
                self.history.accepted.insert(format!("{path}\0{}", h.key));
            }
        }
        let mut files = files;
        for (path, hs) in reject {
            // Through the editor, so it's undoable and saved like an edit; read again first.
            let Some(now) = self.current_text(&path) else {
                continue;
            };
            let Some(f) = files.iter_mut().find(|f| f.path == path) else {
                continue;
            };
            let keys: HashSet<String> = hs.iter().map(|h| h.key.clone()).collect();
            let fresh: Vec<hunks::Hunk> = hunks::compute(&f.old, &now)
                .into_iter()
                .filter(|h| keys.contains(&h.key))
                .collect();
            let after = hunks::reject(&now, &fresh);
            self.apply_text(backend, settings, &path, &after);
            f.hunks = hunks::compute(&f.old, &after);
        }
        if back {
            return;
        }
        self.history.review = Some((base, files));
        if mark {
            self.mark_reviewed(backend, settings);
        }
    }

    /// Puts text into a file through its editor (one undo step) and saves it.
    pub fn apply_text(
        &mut self,
        backend: &Backend,
        settings: &crate::store::Settings,
        path: &str,
        text: &str,
    ) {
        match self.editors.get_mut(path) {
            Some(ed) => {
                ed.doc.replace_all(text);
                ed.doc.break_group();
                self.buffers.edit(path, Instant::now(), 0);
                self.write(backend, path, settings);
            }
            None => {
                let (p, t, root) = (path.to_owned(), text.to_owned(), self.info.root.clone());
                backend.spawn(move |shell| Msg::Done(shell.write_text(&root, p, t, None)));
            }
        }
    }
}
