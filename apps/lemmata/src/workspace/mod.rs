//! The editor screen for one open project: what App.svelte did once a project was open. The tabs
//! and their editors, saving with conflict checks, outside changes, the session, and the layout
//! of toolbar, sidebar, code column and PDF pane.

pub mod buffers;
mod chrome;
pub mod session;
pub mod tree;

use crate::backend::{Backend, Msg};
use crate::editor::commands::Indent;
use crate::editor::view::{self, Editor};
use crate::store::{Dirs, Settings};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};
use buffers::{Buffers, OutsideChange, WriteResult};
use eframe::egui::{self, FontId, Rect, Stroke, Ui, pos2, vec2};
use lemmata_project::Heading;
use lemmata_shell::ProjectInfo;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// What the editor screen asks of the app.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Back to all projects.
    Library,
    /// A setting changed (layout, widths, tree sort…); save it.
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Info,
    Ok,
    Error,
}

pub struct Notice {
    pub text: String,
    pub tone: Tone,
    pub at: Instant,
}

/// Which drop-down is open in the editor screen.
#[derive(Clone, PartialEq)]
pub enum Menu {
    Main(Rect),
    Compile(Rect),
    Sort(Rect),
    Heading(Rect),
    Tab(String, Rect),
}

pub struct Workspace {
    pub info: ProjectInfo,
    pub root: String,
    pub tabs: Vec<String>,
    pub active: Option<String>,
    pub pinned: Vec<String>,
    /// Recently closed tabs, newest first.
    pub closed: Vec<String>,
    pub editors: HashMap<String, Editor>,
    pub buffers: Buffers,
    /// Writes under way, so a file is never written twice at once.
    in_flight: HashSet<String>,
    /// Tabs that close once their save succeeds.
    closing_after_save: HashSet<String>,
    pub headings: Vec<Heading>,
    pub total_words: u32,
    pub collapsed: HashSet<String>,
    pub tree_cursor: Option<String>,
    pub outline_open: bool,
    pub outline_folded: HashSet<String>,
    pub open_error: Option<String>,
    pub selection_words: usize,
    pub focus_mode: bool,
    pub menu: Option<Menu>,
    pub notices: Vec<Notice>,
    /// The line the cursor is on, for the status bar and the outline.
    pub cursor: (String, usize),
    /// Lines the session wants the cursor on, applied when the file opens.
    pending_line: HashMap<String, usize>,
    session_pdf: serde_json::Value,
    /// Files to open when the session is restored, in order, and which is active.
    restoring: Option<(Vec<String>, Option<String>)>,
}

/// Rough word count of a selection: words outside comments and commands (App.svelte countWords).
pub fn count_words(text: &str) -> usize {
    let stripped: String = text
        .lines()
        .map(|l| match crate::editor::commands::comment_start(l) {
            Some(i) => l.chars().take(i).collect::<String>(),
            None => l.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    // Commands go, then the special characters.
    let mut out = String::with_capacity(stripped.len());
    let chars: Vec<char> = stripped.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            while chars.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
                i += 1;
            }
            if chars.get(i) == Some(&'*') {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        out.push(if "{}$&_^~\\[]".contains(chars[i]) {
            ' '
        } else {
            chars[i]
        });
        i += 1;
    }
    out.split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count()
}

impl Workspace {
    pub fn open(info: ProjectInfo, dirs: &Dirs, backend: &Backend) -> Self {
        let root = info.root.to_string_lossy().into_owned();
        let saved = session::load(dirs, &root);
        let mut ws = Self {
            info,
            root,
            tabs: Vec::new(),
            active: None,
            pinned: Vec::new(),
            closed: Vec::new(),
            editors: HashMap::new(),
            buffers: Buffers::default(),
            in_flight: HashSet::new(),
            closing_after_save: HashSet::new(),
            headings: Vec::new(),
            total_words: 0,
            collapsed: HashSet::new(),
            tree_cursor: None,
            outline_open: true,
            outline_folded: HashSet::new(),
            open_error: None,
            selection_words: 0,
            focus_mode: false,
            menu: None,
            notices: Vec::new(),
            cursor: (String::new(), 1),
            pending_line: HashMap::new(),
            session_pdf: serde_json::Value::Null,
            restoring: None,
        };
        // Where you left off, or the main file.
        let (tabs, active) = match saved {
            Some(s) if !s.tabs.is_empty() => {
                ws.collapsed = s.collapsed.unwrap_or_default().into_iter().collect();
                ws.pinned = s.pinned.unwrap_or_default();
                ws.session_pdf = s.pdf;
                if let Some(a) = &s.active {
                    ws.pending_line.insert(a.clone(), s.line);
                }
                // Files deleted since aren't reopened.
                let exists = |p: &String| tree::find(&ws.info.tree, p).is_some();
                let tabs: Vec<String> = s.tabs.into_iter().filter(exists).collect();
                let active = s.active.filter(exists).or_else(|| tabs.last().cloned());
                (tabs, active)
            }
            _ => (vec![ws.info.main.clone()], Some(ws.info.main.clone())),
        };
        for t in &tabs {
            ws.read(backend, t.clone());
        }
        ws.restoring = Some((tabs, active));
        ws.refresh_outline(backend);
        ws
    }

    fn read(&self, backend: &Backend, path: String) {
        backend.spawn(move |shell| Msg::FileRead(path.clone(), shell.read_text(path)));
    }

    pub fn refresh_outline(&self, backend: &Backend) {
        backend.spawn(|shell| Msg::Outline(shell.outline()));
    }

    pub fn notice(&mut self, text: impl Into<String>, tone: Tone) {
        self.notices.push(Notice {
            text: text.into(),
            tone,
            at: Instant::now(),
        });
    }

    /// Shows a file: its tab if it's open, otherwise it's read first.
    pub fn open_file(&mut self, backend: &Backend, path: &str) {
        if self.editors.contains_key(path) {
            if !self.tabs.iter().any(|t| t == path) {
                self.tabs.push(path.into());
            }
            self.active = Some(path.into());
            return;
        }
        if !tree::is_text_file(path) {
            // shortcut: images and PDFs open in the read-only viewer, which lands with the PDF pane (M4).
            return;
        }
        if !self.tabs.iter().any(|t| t == path) {
            self.tabs.push(path.into());
        }
        self.active = Some(path.into());
        self.read(backend, path.into());
    }

    /// Goes to a line of a file, opening it first when needed.
    pub fn goto(&mut self, backend: &Backend, file: &str, line: usize) {
        match self.editors.get_mut(file) {
            Some(ed) => ed.goto_line(line),
            None => {
                self.pending_line.insert(file.into(), line);
            }
        }
        self.open_file(backend, file);
    }

    /// Pinned tabs always come first, in the order they were pinned.
    fn pin_first(&mut self) {
        let pinned: Vec<String> = self
            .pinned
            .iter()
            .filter(|p| self.tabs.contains(p))
            .cloned()
            .collect();
        let rest: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| !self.pinned.contains(t))
            .cloned()
            .collect();
        self.tabs = pinned.into_iter().chain(rest).collect();
    }

    pub fn toggle_pin(&mut self, path: &str) {
        if self.pinned.iter().any(|p| p == path) {
            self.pinned.retain(|p| p != path);
        } else {
            self.pinned.push(path.into());
        }
        self.pin_first();
    }

    /// Closes a tab, saving it first; a failed save keeps the tab and its text.
    pub fn close_tab(&mut self, backend: &Backend, path: &str, settings: &Settings) {
        if self.buffers.is_dirty(path) {
            if self.buffers.conflicts.iter().any(|c| c == path) {
                self.notice(
                    format!(
                        "{path} changed on disk: choose which version to keep before closing it."
                    ),
                    Tone::Info,
                );
                return;
            }
            // Saved now; the tab closes once the write succeeds.
            self.write(backend, path, settings);
            self.closing_after_save.insert(path.into());
            return;
        }
        self.drop_tab(backend, path);
    }

    fn drop_tab(&mut self, backend: &Backend, path: &str) {
        if self.tabs.iter().any(|t| t == path) {
            self.closed.retain(|c| c != path);
            self.closed.insert(0, path.into());
            self.closed.truncate(10);
        }
        self.pinned.retain(|p| p != path);
        self.tabs.retain(|t| t != path);
        self.buffers.forget(path);
        self.editors.remove(path);
        if self.active.as_deref() == Some(path) {
            self.active = None;
            if let Some(next) = self.tabs.last().cloned() {
                self.open_file(backend, &next);
            }
        }
    }

    pub fn reopen_closed(&mut self, backend: &Backend) {
        while !self.closed.is_empty() {
            let path = self.closed.remove(0);
            // A file deleted since can't come back.
            if tree::find(&self.info.tree, &path).is_some() {
                self.open_file(backend, &path);
                return;
            }
        }
        self.notice("No closed tab to reopen.", Tone::Info);
    }

    fn autosave_ms(settings: &Settings) -> u64 {
        settings.autosave_ms.max(0.0) as u64
    }

    /// Starts writing `path` if it has unsaved edits and isn't in conflict.
    pub fn write(&mut self, backend: &Backend, path: &str, settings: &Settings) {
        if self.in_flight.contains(path) {
            // Saved again once the write under way ends.
            self.buffers
                .schedule(path, Instant::now(), Self::autosave_ms(settings).max(50));
            return;
        }
        let Some(ed) = self.editors.get(path) else {
            return;
        };
        let Some(expected) = self.buffers.begin_write(path) else {
            return;
        };
        let text = ed.doc.text();
        self.in_flight.insert(path.into());
        let p = path.to_owned();
        backend.spawn(move |shell| {
            let result = shell.write_text(p.clone(), text.clone(), expected);
            Msg::FileWritten {
                path: p,
                written: text,
                result,
            }
        });
    }

    /// Ctrl+S: every unsaved file now.
    pub fn save_all(&mut self, backend: &Backend, settings: &Settings) {
        for p in self.buffers.unsaved() {
            self.write(backend, &p, settings);
        }
    }

    /// A write is under way.
    pub fn writing(&self) -> bool {
        !self.in_flight.is_empty()
    }

    /// The session to remember for this project.
    pub fn session(&self) -> session::Session {
        let line = self
            .active
            .as_ref()
            .and_then(|a| self.editors.get(a))
            .map_or(1, |e| e.cursor_line());
        session::Session {
            tabs: self.tabs.clone(),
            active: self.active.clone(),
            line,
            collapsed: Some(self.collapsed.iter().cloned().collect()),
            pinned: Some(self.pinned.clone()),
            folds: None,
            pdf: self.session_pdf.clone(),
        }
    }

    /// A message for the open project; others are handed back.
    pub fn on_msg(&mut self, backend: &Backend, settings: &Settings, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::FileRead(path, Ok(text)) => {
                let tex = tree::is_tex(&path);
                let mut ed = Editor::new(&text, tex);
                if let Some(line) = self.pending_line.remove(&path) {
                    ed.goto_line(line.max(1));
                }
                self.buffers.loaded(&path, &text);
                self.editors.insert(path.clone(), ed);
                if let Some((tabs, active)) = &self.restoring {
                    // All of the session's files are in: show them in their order.
                    if tabs
                        .iter()
                        .all(|t| self.editors.contains_key(t) || !tree::is_text_file(t))
                    {
                        self.tabs = tabs.clone();
                        self.active = active.clone();
                        self.pin_first();
                        self.restoring = None;
                    }
                }
            }
            Msg::FileRead(path, Err(e)) => {
                self.tabs.retain(|t| t != &path);
                if self.active.as_deref() == Some(path.as_str()) {
                    self.active = self.tabs.last().cloned();
                }
                if let Some((tabs, _)) = &mut self.restoring {
                    tabs.retain(|t| t != &path);
                }
                self.open_error = Some(e);
            }
            Msg::FileWritten {
                path,
                written,
                result,
            } => {
                self.in_flight.remove(&path);
                let current = self
                    .editors
                    .get(&path)
                    .map(|e| e.doc.text())
                    .unwrap_or_else(|| written.clone());
                match self.buffers.finish_write(
                    &path,
                    &written,
                    &current,
                    result,
                    Instant::now(),
                    Self::autosave_ms(settings),
                ) {
                    WriteResult::Saved => {
                        if path.ends_with(".tex") {
                            self.refresh_outline(backend);
                        }
                        if self.closing_after_save.remove(&path) && !self.buffers.is_dirty(&path) {
                            self.drop_tab(backend, &path);
                        }
                    }
                    WriteResult::Conflict => {
                        self.closing_after_save.remove(&path);
                    }
                    WriteResult::Error(e) => {
                        self.closing_after_save.remove(&path);
                        self.notice(format!("{path} was not saved: {e}"), Tone::Error);
                        self.open_error = Some(e);
                    }
                }
            }
            Msg::DiskText(path, Ok(disk)) => {
                let current = self.editors.get(&path).map(|e| e.doc.text());
                if self
                    .buffers
                    .outside_change(&path, &disk, current.as_deref())
                    == OutsideChange::Reload
                    && let Some(ed) = self.editors.get_mut(&path)
                {
                    // One undo step, so an outside change can be undone like any edit.
                    ed.doc.replace_all(&disk);
                    ed.doc.break_group();
                    self.buffers.loaded(&path, &disk);
                    self.buffers.dirty.remove(&path);
                }
            }
            Msg::DiskText(path, Err(e)) => self.notice(
                format!("{path} changed outside Lemmata but could not be read: {e}"),
                Tone::Error,
            ),
            Msg::Outline(Ok(o)) => {
                self.headings = o.headings;
                self.total_words = o.total_words;
            }
            Msg::Outline(Err(_)) => {}
            Msg::Info(Ok(info)) => self.info = info,
            Msg::Info(Err(e)) => self.notice(e, Tone::Error),
            Msg::Event(lemmata_shell::Event::FilesChanged { changed, removed }) => {
                backend.spawn(|shell| Msg::Info(shell.project_info_cmd()));
                self.removed_outside(backend, &removed);
                for path in changed {
                    if self.editors.contains_key(&path) {
                        let p = path.clone();
                        backend.spawn(move |shell| Msg::DiskText(p.clone(), shell.read_text(p)));
                    }
                }
                self.refresh_outline(backend);
            }
            other => return Some(other),
        }
        None
    }

    /// Files or folders deleted by another program: their tabs close quietly, without saving.
    fn removed_outside(&mut self, backend: &Backend, removed: &[String]) {
        let gone = |p: &str| {
            removed
                .iter()
                .any(|r| p == r || p.starts_with(&format!("{r}/")))
        };
        let closing: Vec<String> = self.editors.keys().filter(|p| gone(p)).cloned().collect();
        if closing.is_empty() {
            return;
        }
        for p in &closing {
            self.buffers.forget(p);
            self.editors.remove(p);
        }
        let was_active = self.active.clone();
        self.tabs.retain(|t| !gone(t));
        if was_active.as_deref().is_some_and(gone) {
            self.active = None;
            if let Some(next) = self.tabs.last().cloned() {
                self.open_file(backend, &next);
            }
        }
        let text = if removed.len() == 1 {
            format!("{} was removed outside Lemmata.", removed[0])
        } else {
            format!(
                "{} files or folders were removed outside Lemmata.",
                removed.len()
            )
        };
        self.notice(text, Tone::Info);
    }

    /// Index of the heading the cursor is under.
    pub fn current_heading(&self) -> Option<usize> {
        let (file, line) = &self.cursor;
        self.headings
            .iter()
            .enumerate()
            .rfind(|(_, h)| &h.file == file && h.line as usize <= *line)
            .map(|(i, _)| i)
    }

    /// Autosave deadlines that passed.
    fn run_autosaves(&mut self, ui: &Ui, backend: &Backend, settings: &Settings) {
        let now = Instant::now();
        for p in self.buffers.due(now) {
            self.write(backend, &p, settings);
        }
        if let Some(t) = self.buffers.next_deadline() {
            ui.ctx()
                .request_repaint_after(t.saturating_duration_since(now));
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &mut Settings,
        backend: &Backend,
    ) -> Option<Action> {
        let mut action = None;
        self.run_autosaves(ui, backend, settings);
        let ctx = ui.ctx().clone();

        // The app's own keys come before the editor's.
        if crate::shortcuts::consume(&ctx, "save") {
            self.save_all(backend, settings);
        }
        if crate::shortcuts::consume(&ctx, "close-tab")
            && let Some(a) = self.active.clone()
        {
            self.close_tab(backend, &a, settings);
        }
        if crate::shortcuts::consume(&ctx, "reopen-tab") {
            self.reopen_closed(backend);
        }
        if crate::shortcuts::consume(&ctx, "focus") {
            self.focus_mode = !self.focus_mode;
        }
        for (id, step) in [("next-tab", 1isize), ("prev-tab", -1)] {
            if crate::shortcuts::consume(&ctx, id)
                && let Some(a) = &self.active
                && let Some(i) = self.tabs.iter().position(|t| t == a)
            {
                let n = self.tabs.len() as isize;
                let next = self.tabs[((i as isize + step).rem_euclid(n)) as usize].clone();
                self.open_file(backend, &next);
            }
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, p.surface);
        let toolbar = Rect::from_min_size(full.min, vec2(full.width(), 46.0));
        if let Some(a) = chrome::toolbar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(toolbar)),
            p,
            self,
            settings,
            backend,
        ) {
            action = Some(a);
        }

        // The panes under the toolbar.
        let panes = Rect::from_min_max(pos2(full.left(), toolbar.bottom()), full.max);
        let show_editor = settings.layout != "pdf" || self.focus_mode;
        let show_pdf = settings.layout != "editor" && !self.focus_mode;
        let show_sidebar = settings.sidebar_open && !self.focus_mode && settings.layout != "pdf";
        let side_w = if show_sidebar {
            settings.sidebar_width as f32
        } else {
            0.0
        };
        let pdf_w = if show_pdf && show_editor {
            // The editor keeps at least 320 px.
            (settings.pdf_width as f32)
                .min(panes.width() - side_w - 320.0)
                .max(0.0)
        } else if show_pdf {
            panes.width()
        } else {
            0.0
        };
        let side = Rect::from_min_size(panes.min, vec2(side_w, panes.height()));
        let code = Rect::from_min_max(
            pos2(side.right(), panes.top()),
            pos2(panes.right() - pdf_w, panes.bottom()),
        );
        let pdf = Rect::from_min_max(pos2(panes.right() - pdf_w, panes.top()), panes.max);

        if show_sidebar {
            chrome::sidebar(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(side)),
                p,
                self,
                settings,
                backend,
            );
        }
        if show_editor {
            self.code_column(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(code)),
                p,
                settings,
                backend,
            );
        }
        if show_pdf {
            // shortcut: the PDF pane (pdf.js's replacement) lands in M4; until then its ground shows.
            ui.painter().rect_filled(pdf, 0, p.pdf_ground);
            if show_editor {
                ui.painter()
                    .vline(pdf.left() + 0.5, pdf.y_range(), Stroke::new(1.0, p.border));
            }
        }
        // Splitters over the pane edges.
        if show_sidebar {
            match widgets::splitter(
                ui,
                p,
                egui::Id::unique("split-side"),
                side.right(),
                panes.y_range(),
                side_w,
                180.0,
                480.0,
                false,
                "Resize the file panel",
            ) {
                Some(w) if w.is_nan() => {
                    settings.sidebar_width = Settings::default().sidebar_width;
                    action = Some(Action::Settings);
                }
                Some(w) => {
                    settings.sidebar_width = w as f64;
                    action = Some(Action::Settings);
                }
                None => {}
            }
        }
        if show_pdf && show_editor {
            match widgets::splitter(
                ui,
                p,
                egui::Id::unique("split-pdf"),
                pdf.left(),
                panes.y_range(),
                pdf_w,
                300.0,
                1400.0,
                true,
                "Resize the PDF panel",
            ) {
                Some(w) if w.is_nan() => {
                    settings.pdf_width = Settings::default().pdf_width;
                    action = Some(Action::Settings);
                }
                Some(w) => {
                    settings.pdf_width = w as f64;
                    action = Some(Action::Settings);
                }
                None => {}
            }
        }

        self.dialogs(&ctx, p, backend, settings);
        chrome::menus(&ctx, p, self, settings, backend, &mut action);
        self.toasts(&ctx, p);
        action
    }

    fn code_column(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &mut Settings,
        backend: &Backend,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.code_bg);
        let mut y = rect.top();
        if !self.focus_mode {
            let strip = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 36.0));
            chrome::tabs(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(strip)),
                p,
                self,
                settings,
                backend,
            );
            y = strip.bottom();
        }
        let bar = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 37.0));
        chrome::edit_bar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(bar)),
            p,
            self,
        );
        y = bar.bottom();
        if let Some(e) = self.open_error.clone() {
            // .err-bar
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 32.0));
            ui.painter().rect_filled(r, 0, p.error_soft);
            ui.painter()
                .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
            let mut c = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(r.shrink2(vec2(8.0, 0.0)))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if Button::icon(crate::icons::X, "Dismiss")
                .size(widgets::Size::Sm)
                .show_icon(&mut c, p)
                .clicked()
            {
                self.open_error = None;
            }
            c.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(&e).color(p.error));
            });
            y = r.bottom();
        }
        let status = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 26.0), rect.max);
        let body = Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), status.top()));
        match self
            .active
            .clone()
            .and_then(|a| self.editors.contains_key(&a).then_some(a))
        {
            Some(path) => {
                let style = editor_style(settings, self.focus_mode);
                let ed = self.editors.get_mut(&path).unwrap();
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let out = view::show(
                    &mut c,
                    p,
                    ed,
                    &style,
                    egui::Id::unique(("editor", &path)),
                    &format!("Text of {}", tree::base_name(&path)),
                );
                if out.changed {
                    self.buffers
                        .edit(&path, Instant::now(), Self::autosave_ms(settings));
                }
                if let Some(line) = out.cursor_line {
                    self.cursor = (path.clone(), line);
                } else if self.cursor.0 != path {
                    self.cursor = (path.clone(), ed.cursor_line());
                }
                if let Some(sel) = out.selection {
                    self.selection_words = if sel.is_empty() { 0 } else { count_words(&sel) };
                }
            }
            None if self.active.is_none() => {
                // .no-file
                let mut c = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(body)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                c.add_space(body.height() / 2.0 - 22.0);
                c.label(egui::RichText::new("No file is open.").color(p.ink));
                c.label(
                    egui::RichText::new(format!(
                        "Pick one in the file tree, or press {} to find it.",
                        crate::shortcuts::display(crate::shortcuts::keys_of("quick-open"))
                    ))
                    .color(p.muted),
                );
            }
            // Still being read.
            None => {}
        }
        chrome::status_bar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(status)),
            p,
            self,
            settings,
        );
    }

    fn dialogs(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        // Another program changed a file with unsaved edits: the user picks a version.
        if let Some(path) = self.buffers.conflicts.first().cloned() {
            let mut choice = None;
            widgets::modal(
                ctx,
                p,
                &format!("{path} changed on disk"),
                480.0,
                |ui| {
                    widgets::paragraph(
                        ui,
                        "Another program changed this file while it had unsaved edits in Lemmata. Nothing is saved until you choose which version to keep.",
                        FontId::proportional(13.0),
                        p.ink,
                        1.45,
                        ui.available_width(),
                    );
                },
                Some(&mut |ui: &mut Ui| {
                    if Button::new("Keep my version")
                        .kind(Kind::Primary)
                        .show(ui, p)
                        .clicked()
                    {
                        choice = Some(true);
                    }
                    if Button::new("Load the version on disk")
                        .show(ui, p)
                        .clicked()
                    {
                        choice = Some(false);
                    }
                }),
            );
            match choice {
                Some(true) => {
                    self.buffers.resolve(&path, true);
                    self.write(backend, &path, settings);
                }
                Some(false) => {
                    self.buffers.resolve(&path, false);
                    // Read again; the editor takes the disk's text as one undo step.
                    self.buffers.dirty.remove(&path);
                    let pth = path.clone();
                    backend.spawn(move |shell| Msg::DiskText(pth.clone(), shell.read_text(pth)));
                }
                None => {}
            }
        }
    }

    fn toasts(&mut self, ctx: &egui::Context, p: &Palette) {
        self.notices
            .retain(|n| n.at.elapsed() < Duration::from_secs(6));
        let Some(n) = self.notices.last() else { return };
        ctx.request_repaint_after(Duration::from_millis(500));
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("toast"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(screen.center().x, screen.bottom() - 20.0))
            .show(ctx, |ui| {
                ui.set_max_width((screen.width() * 0.9).min(720.0));
                let (fill, ink) = match n.tone {
                    Tone::Error => (p.error_soft, p.error),
                    Tone::Ok => (p.ok_soft, p.ok),
                    Tone::Info => (p.surface, p.ink),
                };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(theme::R_LG)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| ui.label(egui::RichText::new(&n.text).color(ink)));
            });
    }
}

/// The editor's look from the settings: Plex Mono at the font size, the line height, indentation.
pub fn editor_style(s: &Settings, focus: bool) -> view::Style {
    let size = s.font_size as f32;
    view::Style {
        // shortcut: "system" and named editor fonts need system font loading (Preferences, M6); Plex Mono for now.
        font: FontId::monospace(size),
        line_height: size * s.line_height as f32,
        indent: Indent::from_setting(&s.indent),
        wrap: s.word_wrap,
        auto_close: s.auto_close,
        focus,
    }
}

impl Workspace {
    /// The editor of the open file.
    pub fn active_editor(&mut self) -> Option<&mut Editor> {
        let a = self.active.clone()?;
        self.editors.get_mut(&a)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn counts_prose_words_like_the_svelte_app() {
        assert_eq!(super::count_words(""), 0);
        assert_eq!(
            super::count_words("\\section{Two words} and $x$ % not this"),
            4
        );
        assert_eq!(super::count_words("Grüße, \\emph{Welt}!"), 2);
    }
}
