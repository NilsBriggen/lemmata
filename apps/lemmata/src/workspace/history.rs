//! History (HistoryPanel.svelte and App.svelte's snapshots): snapshots of the project, a diff of
//! any changed file, one-file and whole-project restores, and reviewing the changes since a
//! snapshot as hunks to accept or reject. Snapshots live in git inside `.lemmata`.

use super::{Tone, Workspace};
use crate::backend::{Backend, Msg};
use crate::icons;
use crate::panels::hunks;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, Color32, CornerRadius, FontId, Margin, Rect, Stroke, Ui, pos2, vec2};
use lemmata_history::{Change, Snapshot};
use lemmata_shell::history::HistoryStatus;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// While the user edits, a snapshot every ten minutes.
const AUTO_SNAPSHOT: Duration = Duration::from_secs(10 * 60);
const REVIEWED: &str = "Reviewed";
/// Snapshots listed at first, and added by each "Show older snapshots".
const PAGE: usize = 200;

pub struct ReviewFile {
    pub path: String,
    pub old: String,
    pub hunks: Vec<hunks::Hunk>,
}

type DiffRows = Vec<(Option<usize>, String, u8)>;

pub struct History {
    pub status: Option<HistoryStatus>,
    pub snapshots: Vec<Snapshot>,
    pub error: Option<String>,
    pub on: bool,
    /// True once `ensure_history` has finished initializing or probing the store.
    pub ready: bool,
    open: Option<String>,
    compare: Option<String>,
    compared: Vec<Change>,
    /// A file's diff: snapshot id, path, the other snapshot (None: the current text), the texts.
    diff: Option<(String, String, Option<String>, String, String)>,
    diff_cache: Option<(String, String, Option<String>, DiffRows)>,
    #[cfg(test)]
    diff_computes: usize,
    confirm_restore: bool,
    /// The snapshot has no copy of the open diff's file, so restoring it removes the file.
    restore_removes: bool,
    editing: Option<(String, String)>,
    editing_focused: bool,
    confirm_all: Option<String>,
    /// "": since the last snapshot marked as reviewed.
    pub(crate) since: String,
    review: Option<(Snapshot, Vec<ReviewFile>)>,
    accepted: HashSet<String>,
    pub busy: Option<&'static str>,
    pdf: Option<(String, String)>,
    /// The "Save a snapshot" dialog's name, while it's open.
    pub label: Option<String>,
    label_focused: bool,
    /// When the last snapshot was taken (None: none yet this session, so the first save takes one).
    pub last: Option<Instant>,
    pub edited: bool,
    /// How many of the newest snapshots are listed.
    pub limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            status: None,
            snapshots: Vec::new(),
            error: None,
            on: true,
            ready: false,
            open: None,
            compare: None,
            compared: Vec::new(),
            diff: None,
            diff_cache: None,
            #[cfg(test)]
            diff_computes: 0,
            confirm_restore: false,
            restore_removes: false,
            editing: None,
            editing_focused: false,
            confirm_all: None,
            since: String::new(),
            review: None,
            accepted: HashSet::new(),
            busy: None,
            pdf: None,
            label: None,
            label_focused: false,
            last: None,
            edited: false,
            limit: PAGE,
        }
    }
}

fn diff_rows(then: &str, now: &str) -> DiffRows {
    let hs = hunks::compute(then, now);
    let lines: Vec<&str> = now.split_inclusive('\n').collect();
    let mut rows = Vec::new();
    let mut line = 0usize;
    let push_same = |rows: &mut DiffRows, from: usize, to: usize, first: bool, last: bool| {
        let n = to - from;
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
    rows
}

impl History {
    pub fn has_viewer(&self) -> bool {
        self.diff.is_some() || self.review.is_some()
    }

    pub fn clear_viewer(&mut self) {
        self.diff = None;
        self.review = None;
    }

    pub(super) fn remap(&mut self, from: &str, to: &str) {
        let moved = |p: &str| super::files::remap(p, from, to);
        if let Some((_, p, _, _, _)) = &mut self.diff {
            *p = moved(p);
        }
        if let Some((_, p, _, _)) = &mut self.diff_cache {
            *p = moved(p);
        }
        if let Some((_, files)) = &mut self.review {
            for f in files {
                f.path = moved(&f.path);
            }
        }
    }

    fn diff_rows_cached(&mut self) -> &DiffRows {
        let Some((id, path, other, then, now)) = self.diff.as_ref() else {
            static EMPTY: DiffRows = Vec::new();
            return &EMPTY;
        };
        let changed = self
            .diff_cache
            .as_ref()
            .is_none_or(|(i, p, o, _)| i != id || p != path || o != other);
        if changed {
            let rows = diff_rows(then, now);
            self.diff_cache = Some((id.clone(), path.clone(), other.clone(), rows));
            #[cfg(test)]
            {
                self.diff_computes += 1;
            }
        }
        &self.diff_cache.as_ref().unwrap().3
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
                } else if matches!(s, HistoryStatus::On) {
                    // Outside changes made since the last session (a pull, a sync) get a snapshot
                    // of their own, apart from the first edit. A no-op if nothing changed.
                    let _ = shell.history_snapshot("Automatic snapshot".into());
                }
                Ok::<_, String>(!matches!(s, HistoryStatus::NoGit { .. }))
            },
            |ws, b, r| match r {
                Ok(on) => {
                    ws.history.ready = true;
                    ws.history.on = on;
                    if on && ws.history.edited && ws.history.last.is_none() {
                        ws.take_snapshot(b, "Automatic snapshot".into());
                    } else {
                        ws.load_history(b);
                    }
                }
                Err(e) => {
                    ws.history.ready = true;
                    ws.history.on = false;
                    ws.notice(format!("History is off for this project: {e}"), Tone::Info);
                }
            },
        );
    }

    pub fn load_history(&mut self, backend: &Backend) {
        let limit = self.history.limit;
        Self::job(
            backend,
            move |shell| {
                let s = shell.history_status()?;
                let list = if matches!(s, HistoryStatus::On) {
                    shell.history_list(limit)?
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
        let manual = label != "Automatic snapshot";
        let name = label.clone();
        Self::job(
            backend,
            move |shell| shell.history_snapshot(label),
            move |ws, b, r| {
                match r {
                Ok(Some(_)) => ws.load_history(b),
                Ok(None) if manual => ws.notice(
                    format!("Nothing changed since the last snapshot, so “{name}” wasn't saved. Rename the newest one instead."),
                    Tone::Info,
                ),
                Ok(None) => {}
                Err(e) => {
                    // The next timer tick tries again.
                    ws.history.edited = true;
                    ws.notice(format!("Couldn't save a snapshot: {e}"), Tone::Error);
                }
            }
            },
        );
    }

    /// After a save: the first of a session takes a snapshot, later ones every ten minutes.
    pub fn snapshot_after_save(&mut self, backend: &Backend) {
        self.history.edited = true;
        if self.history.ready && self.history.on && self.history.last.is_none() {
            self.take_snapshot(backend, "Automatic snapshot".into());
        }
    }

    pub fn snapshot_timer(&mut self, backend: &Backend) {
        if self.history.ready
            && self.history.on
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
            self.history.label_focused = false;
            return;
        };
        let mut enter = false;
        let mut save = false;
        let mut cancel = false;
        let need_focus = !self.history.label_focused;
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
                if need_focus {
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
        if need_focus {
            self.history.label_focused = true;
        }
        if r.closed || cancel {
            self.history.label = None;
            self.history.label_focused = false;
        } else if save || enter {
            self.history.label = None;
            self.history.label_focused = false;
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
            None => lemmata_env::fs::read_text(self.info.root.join(path)).ok(),
        }
    }

    /// Reads a file again from disk after a restore, a replace or a rename changed it. Edits made
    /// since stay unsaved, so the disk text then asks which version to keep instead of replacing them.
    pub(crate) fn reload(&mut self, backend: &Backend, path: &str) {
        if self.editors.contains_key(path) {
            let p = path.to_owned();
            self.spawn(backend, move |shell| {
                Msg::DiskText(p.clone(), shell.read_text(p))
            });
        }
    }

    pub(super) fn show_history_viewer(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface);
        if self.history.diff.is_some() {
            self.history_diff(ui, p, backend, settings);
        } else if self.history.review.is_some() {
            self.history_review(ui, p, backend, settings);
        }
    }

    pub(super) fn history_panel(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) -> bool {
        let mut settings_changed = false;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.app);
        let can_snap = !matches!(self.history.status, Some(HistoryStatus::NotARepo) | None);
        let mut snap = false;
        let rest = super::panels::side_head(ui, p, "History", |ui| {
            if can_snap {
                let btn = Button::new("Snapshot")
                    .with_icon(icons::PLUS)
                    .icon_size(13.0)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .show(ui, p);
                ui.ctx()
                    .accesskit_node_builder(btn.id, |n| n.set_label("Save a snapshot"));
                if btn
                    .on_hover_text("Save a snapshot (Ctrl+Shift+S)")
                    .clicked()
                {
                    snap = true;
                }
            }
        });
        if snap {
            self.history.label = Some(String::new());
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        let ui = &mut c;
        if let Some(e) = self.history.error.clone() {
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(e).size(12.0).color(p.error)).wrap(),
                    );
                });
        }
        let intro = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
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
                            ui.spacing_mut().item_spacing.y = 10.0;
                            add(ui);
                        });
                });
        };
        let muted = |ui: &mut Ui, t: &str| {
            ui.add(egui::Label::new(egui::RichText::new(t).size(12.5).color(p.muted)).wrap());
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
                if self.history.snapshots.is_empty() {
                    intro(ui, &mut |ui| {
                        muted(
                            ui,
                            "No snapshots yet. The first one is taken after your next save.",
                        )
                    });
                } else if self.history_list(ui, p, backend, settings) {
                    settings_changed = true;
                }
            }
        }
        settings_changed
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
    ) -> bool {
        let mut settings_changed = false;
        let busy = self.history.busy;
        let last_reviewed = self
            .history
            .snapshots
            .iter()
            .find(|s| s.label.starts_with(REVIEWED))
            .cloned();
        let base = self.base_snapshot();
        // Review card on quiet ground.
        let mut act: Option<&str> = None;
        egui::Frame::NONE
            .inner_margin(Margin {
                left: 10,
                right: 10,
                top: 4,
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
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.label(
                            egui::RichText::new("Review changes since")
                                .size(11.5)
                                .color(p.muted),
                        );
                        let first = match &last_reviewed {
                            Some(s) => {
                                format!("my last review · {}", crate::when::medium_secs(s.time))
                            }
                            None => "Choose a snapshot…".into(),
                        };
                        let mut opts: Vec<(String, String)> = vec![(String::new(), first)];
                        opts.extend(self.history.snapshots.iter().map(|s| {
                            (
                                s.id.clone(),
                                format!("{} · {}", s.label, crate::when::medium_secs(s.time)),
                            )
                        }));
                        let o: Vec<(&str, &str)> =
                            opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                        let sw = ui.available_width();
                        ui.allocate_ui(vec2(sw, 28.0), |ui| {
                            widgets::select(
                                ui,
                                p,
                                "Review changes since",
                                &mut self.history.since,
                                &o,
                            )
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                            if Button::new(if busy == Some("review") {
                                "Looking…"
                            } else {
                                "Review changes"
                            })
                            .size(Size::Sm)
                            .enabled(base.is_some() && busy.is_none())
                            .show(ui, p)
                            .clicked()
                            {
                                act = Some("review");
                            }
                            if Button::new(if busy == Some("pdf") {
                                "Making PDF…"
                            } else {
                                "PDF of the changes"
                            })
                            .with_icon(icons::FILE_TEXT)
                            .icon_size(13.0)
                            .kind(Kind::Ghost)
                            .size(Size::Sm)
                            .enabled(base.is_some() && busy.is_none() && self.trusted)
                            .show(ui, p)
                            .on_hover_text(if !self.trusted {
                                "Needs latexmk, which restricted projects don't run. Trust the project in its settings to use it."
                            } else if base.is_none() {
                                "Choose a snapshot first."
                            } else {
                                "Compiles the old and the new version with latexdiff into one PDF with the changes marked."
                            })
                            .clicked()
                            {
                                act = Some("pdf");
                            }
                            if let Some((id, path)) = self.history.pdf.clone()
                                && base.as_ref().is_some_and(|b| b.id == id)
                                && Button::new("Open PDF of the changes")
                                    .kind(Kind::Primary)
                                    .size(Size::Sm)
                                    .show(ui, p)
                                    .clicked()
                            {
                                self.spawn(backend, move |shell| {
                                    Msg::Done(shell.open_project_file(path))
                                });
                            }
                            if Button::new("Mark as reviewed")
                                .with_icon(icons::CHECK)
                                .icon_size(13.0)
                                .kind(Kind::Ghost)
                                .size(Size::Sm)
                                .enabled(busy.is_none() && !self.history.snapshots.is_empty())
                                .show(ui, p)
                                .on_hover_text(
                                    "Takes a snapshot called Reviewed, where the next review starts.",
                                )
                                .clicked()
                            {
                                act = Some("mark");
                            }
                        });
                    });
            });
        match act {
            Some("review") => {
                if settings.layout == "editor" {
                    settings.layout = "split".into();
                    settings_changed = true;
                }
                self.start_review(backend, settings);
            }
            Some("pdf") => self.make_pdf(backend, settings),
            Some("mark") => self.mark_reviewed(backend, settings),
            _ => {}
        }

        let snapshots = self.history.snapshots.clone();
        let mut show_diff = None;
        egui::ScrollArea::vertical()
            .id_salt("history")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: 10,
                        right: 10,
                        top: 2,
                        bottom: 14,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let total_snaps = snapshots.len();
                        for (idx, s) in snapshots.iter().enumerate() {
                            let open = self.history.open.as_deref() == Some(s.id.as_str());
                            let w = ui.available_width();
                            let (r, resp) =
                                ui.allocate_exact_size(vec2(w, 48.0), egui::Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::selected(egui::Role::Button, true, open, &s.label)
                            });
                            let fill = if open {
                                p.rail_active
                            } else if resp.hovered() {
                                p.surface_3
                            } else {
                                Color32::TRANSPARENT
                            };
                            if fill != Color32::TRANSPARENT {
                                ui.painter()
                                    .rect_filled(r, CornerRadius::same(theme::R_MD), fill);
                            }
                            // Timeline vertical stem & dot.
                            let dot_x = r.left() + 13.0;
                            let dot_y = r.top() + 17.0;
                            if idx > 0 {
                                ui.painter().vline(
                                    dot_x,
                                    r.top()..=(dot_y - 5.0),
                                    Stroke::new(1.0, p.border_strong),
                                );
                            }
                            if idx + 1 < total_snaps {
                                ui.painter().vline(
                                    dot_x,
                                    (dot_y + 5.0)..=r.bottom(),
                                    Stroke::new(1.0, p.border_strong),
                                );
                            }
                            ui.painter().circle_filled(
                                pos2(dot_x, dot_y),
                                3.5,
                                if open || idx == 0 {
                                    p.accent
                                } else {
                                    p.border_strong
                                },
                            );

                            let text_x = r.left() + 26.0;
                            let editing =
                                self.history.editing.as_ref().filter(|e| e.0 == s.id).cloned();
                            if let Some((_, mut text)) = editing {
                                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(
                                    Rect::from_min_size(
                                        pos2(text_x, r.top() + 5.0),
                                        vec2((w - 34.0).max(80.0), 24.0),
                                    ),
                                ));
                                let t = c.add(
                                    egui::TextEdit::singleline(&mut text)
                                        .desired_width((w - 34.0).max(80.0))
                                        .font(FontId::proportional(12.5)),
                                );
                                c.ctx().accesskit_node_builder(t.id, |n| {
                                    n.set_label("Snapshot name")
                                });
                                if !self.history.editing_focused {
                                    t.request_focus();
                                    self.history.editing_focused = true;
                                }
                                if t.lost_focus() {
                                    self.history.editing = None;
                                    self.history.editing_focused = false;
                                    let (id, l) = (s.id.clone(), text.trim().to_owned());
                                    if !l.is_empty()
                                        && !c.input(|i| i.key_pressed(egui::Key::Escape))
                                    {
                                        Self::job(
                                            backend,
                                            move |sh| sh.history_relabel(id, l),
                                            |ws, b, r| match r {
                                                Ok(()) => ws.load_history(b),
                                                Err(e) => ws.history.error = Some(e),
                                            },
                                        );
                                    }
                                } else {
                                    self.history.editing = Some((s.id.clone(), text));
                                }
                            } else {
                                let mut job = egui::text::LayoutJob::simple_singleline(
                                    s.label.clone(),
                                    FontId::new(12.5, theme::font::sans_500()),
                                    p.ink,
                                );
                                job.wrap.max_width = (r.right() - text_x - 8.0).max(40.0);
                                job.wrap.max_rows = 1;
                                job.wrap.overflow_character = Some('…');
                                let g = ui.fonts_mut(|f| f.layout_job(job));
                                ui.painter().galley(pos2(text_x, r.top() + 8.0), g, p.ink);
                            }
                            let n = s.changes.len();
                            ui.painter().text(
                                pos2(text_x, r.top() + 26.0),
                                egui::Align2::LEFT_TOP,
                                format!(
                                    "{} · {n} file{}",
                                    crate::when::medium_secs(s.time),
                                    if n == 1 { "" } else { "s" }
                                ),
                                FontId::proportional(11.5),
                                p.faint,
                            );
                            if resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.history.open = if open { None } else { Some(s.id.clone()) };
                                self.history.compare = None;
                                self.history.confirm_all = None;
                            }
                            if !open {
                                continue;
                            }
                            // Expanded snapshot card: actions, compare select, changed files.
                            let mut compare_changed = false;
                            egui::Frame::new()
                                .fill(p.surface)
                                .stroke(Stroke::new(1.0, p.card_hairline))
                                .corner_radius(theme::R_MD)
                                .shadow(p.shadow_card)
                                .inner_margin(Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                                        if Button::new("Rename")
                                            .with_icon(icons::PENCIL)
                                            .icon_size(13.0)
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            self.history.editing =
                                                Some((s.id.clone(), s.label.clone()));
                                            self.history.editing_focused = false;
                                        }
                                        if self.history.confirm_all.as_deref() == Some(s.id.as_str())
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(
                                                        "Replace all files? Your current work is saved as a snapshot first.",
                                                    )
                                                    .size(12.0)
                                                    .color(p.ink),
                                                )
                                                .wrap(),
                                            );
                                            if Button::new("Cancel")
                                                .size(Size::Sm)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                self.history.confirm_all = None;
                                            }
                                            if Button::new("Restore")
                                                .kind(Kind::Primary)
                                                .size(Size::Sm)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                self.restore_all(backend, settings, s.id.clone());
                                            }
                                        } else if Button::new("Restore project")
                                            .with_icon(icons::ROTATE_CCW)
                                            .icon_size(13.0)
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            self.history.confirm_all = Some(s.id.clone());
                                        }
                                    });
                                    ui.label(
                                        egui::RichText::new("Compare with")
                                            .size(11.5)
                                            .color(p.muted),
                                    );
                                    let mut opts: Vec<(String, String)> =
                                        vec![(String::new(), "this snapshot's own changes".into())];
                                    opts.extend(
                                        snapshots.iter().filter(|o| o.id != s.id).map(|o| {
                                            (
                                                o.id.clone(),
                                                format!(
                                                    "{} · {}",
                                                    o.label,
                                                    crate::when::medium_secs(o.time)
                                                ),
                                            )
                                        }),
                                    );
                                    let o: Vec<(&str, &str)> = opts
                                        .iter()
                                        .map(|(a, b)| (a.as_str(), b.as_str()))
                                        .collect();
                                    let mut cur = self.history.compare.clone().unwrap_or_default();
                                    let cw = ui.available_width();
                                    ui.allocate_ui(vec2(cw, 28.0), |ui| {
                                        if widgets::select(ui, p, "Compare with", &mut cur, &o) {
                                            compare_changed = true;
                                        }
                                    });
                                    self.history.compare = (!cur.is_empty()).then_some(cur);
                                    let files = if self.history.compare.is_some() {
                                        self.history.compared.clone()
                                    } else {
                                        s.changes.clone()
                                    };
                                    for ch in files {
                                        let (fr, fresp) = ui.allocate_exact_size(
                                            vec2(ui.available_width(), 24.0),
                                            egui::Sense::click(),
                                        );
                                        if fresp.hovered() {
                                            ui.painter().rect_filled(
                                                fr,
                                                CornerRadius::same(5),
                                                p.rail_active,
                                            );
                                        }
                                        let (letter, color) = match ch.status {
                                            "added" => ("A", p.ok),
                                            "deleted" => ("D", p.error),
                                            _ => ("M", p.warn),
                                        };
                                        ui.painter().text(
                                            fr.min + vec2(6.0, 12.0),
                                            egui::Align2::LEFT_CENTER,
                                            letter,
                                            FontId::new(11.5, theme::font::mono_500()),
                                            color,
                                        );
                                        ui.painter().text(
                                            fr.min + vec2(22.0, 12.0),
                                            egui::Align2::LEFT_CENTER,
                                            &ch.path,
                                            FontId::monospace(11.5),
                                            p.ink,
                                        );
                                        fresp.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::Role::Button,
                                                true,
                                                &ch.path,
                                            )
                                        });
                                        if fresp
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            show_diff = Some((s.id.clone(), ch.path.clone()));
                                        }
                                    }
                                });
                            if compare_changed {
                                self.history.compared.clear();
                                if let Some(other) = self.history.compare.clone() {
                                    let a = s.id.clone();
                                    Self::job(
                                        backend,
                                        move |sh| {
                                            sh.history_compare(a.clone(), other.clone())
                                                .map(|c| (a, other, c))
                                        },
                                        |ws, _, r| match r {
                                            Ok((a, b, c))
                                                if ws.history.open.as_deref() == Some(&a)
                                                    && ws.history.compare.as_deref() == Some(&b) =>
                                            {
                                                ws.history.compared = c
                                            }
                                            Ok(_) => {}
                                            Err(e) => ws.history.error = Some(e),
                                        },
                                    );
                                }
                            }
                        }
                        if snapshots.len() >= self.history.limit
                            && Button::new("Show older snapshots")
                                .kind(Kind::Ghost)
                                .size(Size::Sm)
                                .show(ui, p)
                                .clicked()
                        {
                            self.history.limit += PAGE;
                            self.load_history(backend);
                        }
                    });
            });
        if let Some((id, path)) = show_diff {
            if settings.layout == "editor" {
                settings.layout = "split".into();
                settings_changed = true;
            }
            let other = self.history.compare.clone();
            let now = if other.is_none() {
                self.current_text(&path)
            } else {
                None
            };
            Self::job(
                backend,
                move |sh| {
                    let then = sh.history_file(id.clone(), path.clone()).ok().flatten();
                    let removes = then.is_none();
                    let then = then.unwrap_or_default();
                    let now = match &other {
                        Some(o) => sh
                            .history_file(o.clone(), path.clone())
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                        None => now.unwrap_or_default(),
                    };
                    ((id, path, other, then, now), removes)
                },
                |ws, _, (d, removes)| {
                    ws.build.raw_log_open = false;
                    ws.history.review = None;
                    ws.history.diff = Some(d);
                    ws.history.restore_removes = removes;
                    ws.history.diff_cache = None;
                    ws.history.confirm_restore = false;
                },
            );
        }
        settings_changed
    }

    /// The diff of one file: removed lines red, added green, long unchanged stretches folded.
    fn history_diff(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        backend: &Backend,
        settings: &mut crate::store::Settings,
    ) {
        let Some((id, path, other)) = self
            .history
            .diff
            .as_ref()
            .map(|(id, path, other, _, _)| (id.clone(), path.clone(), other.clone()))
        else {
            return;
        };
        let mut back = false;
        let mut restore = false;
        let r = egui::Frame::new()
            .fill(p.surface)
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                if Button::new("Back").with_icon(icons::ARROW_LEFT).icon_size(14.0).kind(Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                    back = true;
                }
                let path_max = (ui.available_width() - 160.0).max(80.0);
                ui.add_sized(
                    vec2(path_max, 22.0),
                    egui::Label::new(
                        egui::RichText::new(&path)
                            .font(FontId::monospace(12.0))
                            .color(p.muted),
                    )
                    .truncate(),
                )
                .on_hover_text(&path);
                if self.history.confirm_restore {
                    let ask = if self.history.restore_removes {
                        "This file didn't exist then, so restoring removes it. The current version is kept as a snapshot."
                    } else {
                        "Replace the current file? The current version is kept as a snapshot."
                    };
                    ui.label(egui::RichText::new(ask).size(12.0).color(p.ink));
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
        if !hunks::is_text_path(&path) {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("This file isn't text, so there is no diff to show.")
                    .size(12.0)
                    .color(p.muted),
            );
            return;
        }
        let rows = self.history.diff_rows_cached();
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
                            r.min + vec2(36.0, 1.0),
                            egui::Align2::RIGHT_TOP,
                            n.to_string(),
                            font.clone(),
                            p.faint,
                        );
                    }
                    if *kind == 1 || *kind == 2 {
                        ui.painter().text(
                            r.min + vec2(44.0, 1.0),
                            egui::Align2::CENTER_TOP,
                            if *kind == 1 { "−" } else { "+" },
                            FontId::new(12.5, theme::font::mono_500()),
                            if *kind == 1 { p.error } else { p.ok },
                        );
                    }
                    if *kind == 3 {
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("⋯ {text}"),
                            FontId::proportional(12.0),
                            p.muted,
                        );
                    } else {
                        ui.painter().galley(r.min + vec2(54.0, 1.0), g, p.ink);
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
        let snapshots = self.history.snapshots.clone();
        let texts: Vec<(String, String)> = self
            .editors
            .iter()
            .map(|(p, e)| (p.clone(), e.doc.text()))
            .collect();
        let root = self.info.root.clone();
        self.after_saving(backend, settings, move |_, b| {
            Self::job(
                b,
                move |sh| {
                    // The work since the last snapshot counts too.
                    let _ = sh.history_snapshot("Before review".into());
                    // The window the user sees, plus the snapshot just taken and a timer one.
                    let list = sh.history_list(snapshots.len() + 2)?;
                    let idx = list.iter().position(|s| s.id == base.id).ok_or(
                        "That snapshot is no longer in the list. Reload History and try again.",
                    )?;
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
                            .history_file(base.id.clone(), path.clone())?
                            .unwrap_or_default();
                        let now = texts
                            .iter()
                            .find(|t| t.0 == path)
                            .map(|t| t.1.clone())
                            .or_else(|| lemmata_env::fs::read_text(root.join(&path)).ok())
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
                    Ok::<_, String>((base, list, files))
                },
                |ws, _, r| {
                    ws.history.busy = None;
                    match r {
                        Ok((base, list, files)) => {
                            ws.build.raw_log_open = false;
                            ws.history.diff = None;
                            ws.history.snapshots = list;
                            ws.history.accepted.clear();
                            ws.history.review = Some((base, files));
                        }
                        Err(e) => ws.history.error = Some(e),
                    }
                },
            );
        });
    }
    fn make_pdf(&mut self, backend: &Backend, settings: &crate::store::Settings) {
        let Some(base) = self.base_snapshot() else {
            return;
        };
        self.history.error = None;
        self.history.busy = Some("pdf");
        let id = base.id.clone();
        self.after_saving(backend, settings, move |_, b| {
            Self::job(
                b,
                move |sh| sh.history_latexdiff(id.clone()).map(|p| (id, p)),
                |ws, _, r| {
                    ws.history.busy = None;
                    match r {
                        Ok((id, p)) => {
                            ws.history.pdf = Some((id, p.to_string_lossy().into_owned()))
                        }
                        Err(e) => ws.history.error = Some(e),
                    }
                },
            );
        });
    }

    fn mark_reviewed(&mut self, backend: &Backend, settings: &crate::store::Settings) {
        self.history.error = None;
        self.history.busy = Some("mark");
        let latest = self.history.snapshots.first().cloned();
        self.after_saving(backend, settings, move |_, b| {
            Self::job(
                b,
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
        });
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
                    ws.spawn(b, |shell| Msg::Info(shell.project_info_cmd()));
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
                let auto = settings.auto_compile;
                Self::job(
                    backend,
                    move |shell| shell.write_text(&root, p, t, None),
                    move |ws, b, r| match r {
                        Ok(()) => {
                            ws.refresh_analysis(b);
                            ws.refresh_git_status(b);
                            if auto {
                                ws.build_soon(false, false);
                            }
                        }
                        Err(e) => ws.notice(e, Tone::Error),
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_rows_are_cached_until_the_diff_identity_changes() {
        let mut history = History {
            diff: Some((
                "a".into(),
                "main.tex".into(),
                None,
                "old\n".into(),
                "old\nnew\n".into(),
            )),
            ..Default::default()
        };
        assert_eq!(history.diff_rows_cached().len(), 2);
        assert_eq!(history.diff_rows_cached().len(), 2);
        assert_eq!(history.diff_computes, 1);
        history.diff.as_mut().unwrap().0 = "b".into();
        assert_eq!(history.diff_rows_cached().len(), 2);
        assert_eq!(history.diff_computes, 2);
    }
}
