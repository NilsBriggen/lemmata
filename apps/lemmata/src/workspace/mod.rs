//! The editor screen for one open project: what App.svelte did once a project was open. The tabs
//! and their editors, saving with conflict checks, outside changes, the session, and the layout
//! of toolbar, sidebar, code column and PDF pane.

mod bib;
pub mod buffers;
mod build;
mod chrome;
pub mod files;
mod history;
mod insert;
pub mod palette;
pub mod panels;
mod project_settings;
mod rename;
mod search;
pub mod session;
mod sheets;
mod symbols;
pub mod tree;
pub mod viewer;

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
    Preferences,
    /// Preferences' snippet editor on its own.
    Snippets,
    /// "Report a problem…".
    Report,
    /// "Save as template…" for this project's folder.
    SaveTemplate(String),
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
    /// A button that does something about it; such notices stay until dismissed, like errors.
    pub action: Option<NoticeAction>,
}

impl Notice {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            at: Instant::now(),
            action: None,
        }
    }
    pub fn action(mut self, a: NoticeAction) -> Self {
        self.action = Some(a);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NoticeAction {
    /// Adds `\usepackage{…}` for something just inserted.
    AddPackage(String),
    /// Downloads a spell-check dictionary.
    Download(String),
    /// Export for arXiv although the build failed (flattened or not).
    ArxivAnyway(bool),
    /// Build normally again after a draft export.
    DraftOff,
    /// Loads zotero.bib in the main file.
    LinkZotero,
    /// Puts back the pasted text a table was made from: the file, where, the table, the text.
    PasteAsText(String, usize, String, String),
    /// Undoes the last file operation in the tree.
    UndoFiles,
    /// Lets this project's latexmkrc run (and shell escape, when it asks for it).
    TrustProject,
}

impl NoticeAction {
    fn label(&self) -> &'static str {
        match self {
            NoticeAction::AddPackage(_) => "Add it",
            NoticeAction::Download(_) => "Download",
            NoticeAction::ArxivAnyway(_) => "Export anyway",
            NoticeAction::DraftOff => "Switch to Normal",
            NoticeAction::LinkZotero => "Add it",
            NoticeAction::PasteAsText(..) => "Paste as text instead",
            NoticeAction::UndoFiles => "Undo",
            NoticeAction::TrustProject => "Use it",
        }
    }
}

/// Which drop-down is open in the editor screen.
#[derive(Clone, PartialEq)]
pub enum Menu {
    Main(Rect),
    Compile(Rect),
    Sort(Rect),
    Heading(Rect),
    Tab(String, Rect),
    /// The tree's right-click menu: on a row, or on its empty space.
    Tree(Option<String>, Rect),
}

/// Work that waits for the writes in flight.
pub type AfterWrites = Box<dyn FnOnce(&mut Workspace, &Backend)>;

pub struct Workspace {
    pub info: ProjectInfo,
    pub root: String,
    pub tabs: Vec<String>,
    pub active: Option<String>,
    pub pinned: Vec<String>,
    /// Recently closed tabs, newest first.
    pub closed: Vec<String>,
    pub editors: HashMap<String, Editor>,
    /// Images and PDFs open in tabs.
    pub viewers: HashMap<String, viewer::Viewer>,
    pub buffers: Buffers,
    /// Writes under way, so a file is never written twice at once.
    in_flight: HashSet<String>,
    /// Tabs that close once their save succeeds.
    closing_after_save: HashSet<String>,
    pub headings: Vec<Heading>,
    pub total_words: u32,
    pub collapsed: HashSet<String>,
    /// The file tree's rows as last drawn, until the tree, its order or the open folders change.
    pub tree_rows: Option<tree::Rows>,
    pub tree_cursor: Option<String>,
    pub sel: files::TreeSel,
    pub tree_drag: Option<files::TreeDrag>,
    /// The tree folder files dragged in from the computer are over ("" is the root).
    pub os_drop: Option<String>,
    pub files: files::FileActions,
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
    /// Folds the session remembers for files not open yet (first lines, 1-based).
    pending_folds: HashMap<String, Vec<usize>>,
    session_pdf: serde_json::Value,
    /// Files to open when the session is restored, in order, and which is active.
    restoring: Option<(Vec<String>, Option<String>)>,
    pub pane: crate::pdf::pane::Pane,
    pub build: build::BuildState,
    /// TeX was found, so builds can run; and whether it's an old TeX Live.
    pub tex_ready: bool,
    pub tex_outdated: bool,
    /// Saves that don't start a build (Recompile, exports and settings changes build themselves).
    no_build: HashSet<String>,
    /// Saves the autosave timer started; their build waits a little longer.
    auto_writes: HashSet<String>,
    /// A `\usepackage` to add once the main file is saved.
    adding_package: Option<String>,
    /// The indentation setting, for edits made from outside the editor (toolbar, dialogs).
    pub indent: Indent,
    /// Labels, citation keys, packages, files and snippets, for completion and hover cards.
    pub lookups: crate::editor::complete::Lookups,
    /// Packages already offered this session, so the offer comes once.
    offered: HashSet<String>,
    /// The symbol palette, while open.
    pub symbols: Option<symbols::SymbolPalette>,
    pub cite: Option<bib::CitePicker>,
    /// Project search, in the sidebar's place.
    pub search: Option<search::ProjectSearch>,
    pub bib_manager: Option<bib::BibManager>,
    pub figure: Option<insert::FigureDialog>,
    pub rename: Option<rename::RenameSymbol>,
    pub project_settings: Option<project_settings::ProjectSettings>,
    pub palette: Option<palette::Palette>,
    /// Presenting the PDF full screen.
    pub presentation: Option<crate::pdf::present::Presentation>,
    pub words: Option<sheets::Words>,
    /// The shortcuts sheet, with its search.
    pub shortcut_sheet: Option<String>,
    pub table: Option<insert::TableDialog>,
    /// Run once every write in flight has finished (changes on disk that must not lose edits).
    pub after_writes: Vec<AfterWrites>,
    /// "Use it" on the latexmkrc notice: the next frame puts the project on the trust list.
    pub trust_requested: bool,
    /// `Backend::project` for this project: results tagged otherwise are dropped.
    pub project: u64,
    /// Spelling and grammar, shared by the editors.
    pub checks: crate::writing::Checks,
    /// The spell-check language last asked for, with the project's words then.
    spell_asked: Option<(String, Vec<String>)>,
    /// Languages whose missing dictionary was offered this session.
    dictionaries_offered: HashSet<String>,
    /// The main file's language from babel or polyglossia, for "auto".
    detected: &'static str,
    /// The panel shown where the PDF is, if any.
    pub panel: Option<panels::Panel>,
    pub comments: panels::Comments,
    pub todos: panels::Todos,
    pub git: panels::Git,
    pub history: history::History,
    /// Paragraphs waiting for LanguageTool, and how many requests are out.
    grammar_queue: Vec<String>,
    grammar_out: usize,
    /// texlab: completion, hover, definitions and references.
    pub lsp: crate::lsp::Lsp,
    /// Find references' list: file, line (1-based), column, and the line's text.
    pub references: Option<Vec<(String, usize, usize, String)>>,
    /// Messages to texlab go through one task, so they arrive in the order they were made.
    lsp_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
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
    pub fn open(info: ProjectInfo, dirs: &Dirs, backend: &Backend, tex_ready: bool) -> Self {
        let root = info.root.to_string_lossy().into_owned();
        let root_for_lsp = root.clone();
        let saved = session::load(dirs, &root);
        let mut ws = Self {
            info,
            root,
            tabs: Vec::new(),
            active: None,
            pinned: Vec::new(),
            closed: Vec::new(),
            editors: HashMap::new(),
            viewers: HashMap::new(),
            buffers: Buffers::default(),
            in_flight: HashSet::new(),
            closing_after_save: HashSet::new(),
            headings: Vec::new(),
            total_words: 0,
            collapsed: HashSet::new(),
            tree_rows: None,
            tree_cursor: None,
            sel: Default::default(),
            tree_drag: None,
            os_drop: None,
            files: Default::default(),
            outline_open: true,
            outline_folded: HashSet::new(),
            open_error: None,
            selection_words: 0,
            focus_mode: false,
            menu: None,
            notices: Vec::new(),
            cursor: (String::new(), 1),
            pending_line: HashMap::new(),
            pending_folds: HashMap::new(),
            session_pdf: serde_json::Value::Null,
            restoring: None,
            pane: crate::pdf::pane::Pane::new(backend.ctx()),
            build: build::BuildState::default(),
            tex_ready,
            tex_outdated: false,
            no_build: HashSet::new(),
            auto_writes: HashSet::new(),
            adding_package: None,
            indent: Indent::Spaces(2),
            lookups: Default::default(),
            offered: HashSet::new(),
            symbols: None,
            cite: None,
            search: None,
            bib_manager: None,
            figure: None,
            rename: None,
            project_settings: None,
            palette: None,
            presentation: None,
            words: None,
            shortcut_sheet: None,
            table: None,
            after_writes: Vec::new(),
            trust_requested: false,
            project: backend
                .project
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1,
            checks: crate::writing::Checks::default(),
            spell_asked: None,
            dictionaries_offered: HashSet::new(),
            detected: "en",
            panel: None,
            comments: Default::default(),
            todos: Default::default(),
            git: Default::default(),
            history: Default::default(),
            grammar_queue: Vec::new(),
            grammar_out: 0,
            lsp: crate::lsp::Lsp::new(&root_for_lsp),
            references: None,
            lsp_tx: None,
        };
        // Where you left off, or the main file.
        let (tabs, active) = match saved {
            Some(s) if !s.tabs.is_empty() => {
                ws.collapsed = s.collapsed.unwrap_or_default().into_iter().collect();
                ws.pinned = s.pinned.unwrap_or_default();
                ws.pending_folds = s.folds.clone().unwrap_or_default().into_iter().collect();
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
            ws.load(backend, t);
        }
        ws.restoring = Some((tabs, active));
        ws.refresh_outline(backend);
        ws.refresh_lookups(backend, true);
        // texlab starts in the background (downloaded the first time); completion and hover
        // gain its answers once it's ready.
        ws.lsp.state = crate::lsp::State::Starting;
        backend.spawn_async(|shell| async move { Msg::LspStarted(shell.lsp_start().await) });
        if let Ok(main) = std::fs::read_to_string(ws.info.root.join(&ws.info.main)) {
            Self::set_macros(&main);
            ws.detected = crate::writing::spell::detect_language(&main);
        }
        ws.checks.spell.shell = Some(backend.shell.clone());
        ws.load_comments(backend);
        ws.ensure_history(backend);
        ws.refresh_git_status(backend);
        ws.pane.view.set_position(&ws.session_pdf);
        // The PDF on disk shows right away; it is only built again when something changed since.
        backend.spawn(|shell| Msg::Pdf(shell.read_pdf()));
        backend.spawn(|shell| Msg::PdfCurrent(shell.pdf_is_current().unwrap_or(false)));
        ws
    }

    fn read(&self, backend: &Backend, path: String) {
        backend.spawn(move |shell| Msg::FileRead(path.clone(), shell.read_text(path)));
    }

    /// Reads a file for its tab: text for the editor, or an image or PDF for a viewer.
    fn load(&mut self, backend: &Backend, path: &str) {
        if !tree::is_viewer_file(path) {
            self.read(backend, path.into());
            return;
        }
        self.viewers.insert(path.into(), viewer::Viewer::Loading);
        let p = path.to_owned();
        backend.spawn(move |shell| {
            let r = shell.read_binary(p.clone()).and_then(|bytes| {
                if p.to_lowercase().ends_with(".pdf") {
                    Ok(crate::backend::Viewed::Pdf(bytes))
                } else {
                    viewer::decode(&p, &bytes).map(crate::backend::Viewed::Image)
                }
            });
            Msg::Viewed(p, r)
        });
    }

    fn lsp_send(&self, _backend: &Backend, messages: Vec<String>) {
        if let Some(tx) = &self.lsp_tx {
            for body in messages {
                let _ = tx.send(body);
            }
        }
    }

    /// Ends texlab with the project.
    pub fn stop_lsp(&self, backend: &Backend) {
        if let Some(generation) = self.lsp.generation {
            backend.spawn_async(
                move |shell| async move { Msg::Done(shell.lsp_stop(generation).await) },
            );
        }
    }

    /// Tells texlab about the open files' current text.
    fn lsp_sync(&mut self, backend: &Backend) {
        if self.lsp.state != crate::lsp::State::Ready {
            return;
        }
        #[allow(clippy::type_complexity)]
        let texts: Vec<(String, u64, Box<dyn Fn() -> String + '_>)> = self
            .editors
            .iter()
            .map(|(p, e)| {
                (
                    p.clone(),
                    e.doc.version,
                    Box::new(move || e.doc.text()) as Box<dyn Fn() -> String>,
                )
            })
            .collect();
        let docs: Vec<(String, u64, &dyn Fn() -> String)> = texts
            .iter()
            .map(|(p, v, f)| (p.clone(), *v, f.as_ref()))
            .collect();
        let msgs = self.lsp.sync(&docs);
        drop(docs);
        drop(texts);
        self.lsp_send(backend, msgs);
    }

    fn on_lsp(&mut self, backend: &Backend, a: crate::lsp::Answer) {
        use crate::lsp::Answer;
        match a {
            Answer::Ready => self.lsp_sync(backend),
            Answer::Completion {
                path,
                head,
                from,
                items,
            } => {
                self.lookups.server = Some((path, head, from, items));
                self.lookups.generation += 1;
            }
            Answer::Hover {
                path,
                line,
                col,
                text,
            } => {
                self.lookups.hover = Some((path, line, col, text));
            }
            Answer::Locations {
                references: false,
                places,
            } => {
                if let Some((path, line, _)) = places.into_iter().next() {
                    self.goto(backend, &path, line);
                }
            }
            Answer::Locations {
                references: true,
                places,
            } => {
                let rows = places
                    .into_iter()
                    .map(|(path, line, col)| {
                        let text = match self.editors.get(&path) {
                            Some(e) if line >= 1 && line <= e.doc.line_count() => {
                                e.doc.line(line - 1)
                            }
                            _ => std::fs::read_to_string(self.info.root.join(&path))
                                .ok()
                                .and_then(|t| {
                                    t.lines().nth(line.saturating_sub(1)).map(str::to_owned)
                                })
                                .unwrap_or_default(),
                        };
                        (path, line, col, text.trim().to_owned())
                    })
                    .collect();
                self.references = Some(rows);
            }
        }
    }

    /// The spell-check language: a `% !TeX spellcheck` comment in the open file, the project's
    /// choice, or the app's default (guessed from babel or polyglossia for "auto").
    pub(super) fn spell_language(&self, settings: &Settings) -> String {
        let magic = self
            .active
            .as_ref()
            .filter(|a| tree::is_tex(a))
            .and_then(|a| self.editors.get(a))
            .and_then(|e| {
                let head: Vec<String> = (0..e.doc.line_count().min(30))
                    .map(|n| e.doc.line(n))
                    .collect();
                crate::writing::spell::magic_language(&head.join("\n"))
            });
        let fallback = if settings.spell_default == "auto" {
            self.detected.to_owned()
        } else {
            settings.spell_default.clone()
        };
        magic
            .or_else(|| self.info.config.spellcheck.clone())
            .unwrap_or(fallback)
    }

    /// Loads another spell-check language when the file, project or settings ask for one, and
    /// points grammar checking at the same language.
    fn apply_language(&mut self, backend: &Backend, settings: &Settings) {
        let lang = self.spell_language(settings);
        let lang = if crate::writing::spell::LANGUAGES
            .iter()
            .any(|(c, _)| *c == lang)
        {
            lang
        } else {
            "off".to_owned()
        };
        let words = self.info.config.words.clone();
        if self.spell_asked.as_ref() != Some(&(lang.clone(), words.clone())) {
            self.spell_asked = Some((lang.clone(), words.clone()));
            let l = lang.clone();
            backend.spawn(move |shell| {
                let r = shell.spell_set_language(l.clone(), words);
                Msg::SpellLoaded((l != "off").then_some(l), r)
            });
            // A language whose dictionary isn't here yet is offered, once a session.
            if lang != "off" && self.dictionaries_offered.insert(lang.clone()) {
                backend.spawn(move |shell| match shell.spell_available() {
                    Ok(list) => match list.into_iter().find(|d| d.code == lang && !d.installed) {
                        Some(d) => Msg::Dictionary(d),
                        None => Msg::Done(Ok(())),
                    },
                    Err(_) => Msg::Done(Ok(())),
                });
            }
        }
        let lang = self
            .spell_asked
            .as_ref()
            .map_or("off".to_owned(), |a| a.0.clone());
        self.checks
            .grammar
            .configure(&settings.grammar, &settings.grammar_url, &lang);
        self.checks
            .grammar
            .set_ignored(&self.info.config.grammar_ignore);
    }

    /// Sends waiting paragraphs to LanguageTool, two at a time.
    fn pump_grammar(&mut self, backend: &Backend) {
        let Some(target) = self.checks.grammar.target.clone() else {
            self.grammar_queue.clear();
            return;
        };
        while self.grammar_out < 2 && !self.grammar_queue.is_empty() {
            let text = self.grammar_queue.remove(0);
            self.grammar_out += 1;
            let t = target.clone();
            backend.spawn_async(move |shell| async move {
                let r = shell
                    .grammar_check(t.0.clone(), t.1.clone(), text.clone())
                    .await;
                Msg::GrammarFound(t, text, r)
            });
        }
    }

    /// Runs `work` off the UI thread, then `then` with its result on the open project.
    pub fn job<T: Send + 'static>(
        backend: &Backend,
        work: impl FnOnce(&lemmata_shell::Shell) -> T + Send + 'static,
        then: impl FnOnce(&mut Workspace, &Backend, T) + Send + 'static,
    ) {
        let project = backend.project();
        backend.spawn(move |shell| {
            let r = work(shell);
            Msg::Apply(project, Box::new(move |ws, b| then(ws, b, r)))
        });
    }

    /// The same for the shell's async calls.
    pub fn job_async<T: Send + 'static, Fut: std::future::Future<Output = T> + Send + 'static>(
        backend: &Backend,
        work: impl FnOnce(std::sync::Arc<lemmata_shell::Shell>) -> Fut + Send + 'static,
        then: impl FnOnce(&mut Workspace, &Backend, T) + Send + 'static,
    ) {
        let project = backend.project();
        backend.spawn_async(move |shell| async move {
            let r = work(shell).await;
            Msg::Apply(
                project,
                Box::new(move |ws: &mut Workspace, b: &Backend| then(ws, b, r))
                    as crate::backend::Job,
            )
        });
    }

    /// Something just inserted needs `pkg`: offer to load it, once, if the document doesn't.
    pub fn needs_package(&mut self, backend: &Backend, pkg: String) {
        if self.offered.insert(pkg.clone()) {
            backend.spawn(move |shell| {
                let has = shell.has_package(pkg.clone()).unwrap_or(true);
                Msg::HasPackage(pkg, has)
            });
        }
    }

    /// The main file's `\newcommand`s for the maths previews (setMathMacros).
    fn set_macros(text: &str) {
        let preamble = text.split("\\begin{document}").next().unwrap_or("");
        crate::writing::render::set_macros(crate::writing::math::collect_macros(preamble));
    }

    pub fn refresh_outline(&self, backend: &Backend) {
        backend.spawn(|shell| Msg::Outline(shell.outline()));
    }

    /// Reads labels and citation keys again (after saves and outside changes); the package list
    /// only once.
    pub fn refresh_lookups(&mut self, backend: &Backend, packages: bool) {
        backend.spawn(|shell| Msg::Labels(shell.labels()));
        backend.spawn(|shell| Msg::Cites(shell.bib_entries().map(|b| b.entries)));
        if packages {
            backend.spawn(|shell| Msg::Packages(shell.package_names()));
        }
        fn files(list: &[lemmata_project::Entry], out: &mut Vec<String>) {
            for e in list {
                if e.is_dir {
                    files(&e.children, out);
                } else {
                    out.push(e.path.clone());
                }
            }
        }
        let mut f = Vec::new();
        files(&self.info.tree, &mut f);
        self.lookups.files = f;
        self.lookups.root = self.info.root.clone();
        self.lookups.generation += 1;
    }

    pub fn notice(&mut self, text: impl Into<String>, tone: Tone) {
        self.notices.push(Notice::new(text, tone));
    }

    /// Shows a file: its tab if it's open, otherwise it's read first.
    pub fn open_file(&mut self, backend: &Backend, path: &str) {
        if self.editors.contains_key(path) || self.viewers.contains_key(path) {
            if !self.tabs.iter().any(|t| t == path) {
                self.tabs.push(path.into());
            }
            self.active = Some(path.into());
            return;
        }
        if !tree::is_text_file(path) && !tree::is_viewer_file(path) {
            return;
        }
        if !self.tabs.iter().any(|t| t == path) {
            self.tabs.push(path.into());
        }
        self.active = Some(path.into());
        self.load(backend, path);
    }

    /// A file the OS asked to open with the project: it joins the session's tabs being restored
    /// and is the one shown.
    pub fn open_requested(&mut self, backend: &Backend, path: &str) {
        if let Some((tabs, active)) = &mut self.restoring {
            if !tabs.iter().any(|t| t == path) {
                tabs.push(path.into());
            }
            *active = Some(path.into());
        }
        self.open_file(backend, path);
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
        self.viewers.remove(path);
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
        let (root, project) = (self.info.root.clone(), self.project);
        backend.spawn(move |shell| {
            let result = shell.write_text(&root, p.clone(), text.clone(), expected);
            Msg::FileWritten {
                project,
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
            folds: Some(
                self.editors
                    .iter()
                    .map(|(p, e)| (p.clone(), e.folds.lines(&e.doc)))
                    .chain(self.pending_folds.clone())
                    .filter(|(_, l)| !l.is_empty())
                    .collect(),
            ),
            pdf: if self.pane.has_pdf() {
                self.pane.view.position()
            } else {
                self.session_pdf.clone()
            },
        }
    }

    /// A message for the open project; others are handed back.
    pub fn on_msg(&mut self, backend: &Backend, settings: &Settings, msg: Msg) -> Option<Msg> {
        match msg {
            // Read twice (two quick opens): the editor from the first read keeps what was typed in it.
            Msg::FileRead(path, Ok(_)) if self.editors.contains_key(&path) => {}
            Msg::FileRead(path, Ok(text)) => {
                let tex = tree::is_tex(&path);
                let mut ed = Editor::new(&text, tex);
                ed.path = path.clone();
                if let Some(lines) = self.pending_folds.remove(&path) {
                    ed.set_folds(&lines);
                }
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
            Msg::Viewed(path, result) => {
                if self.viewers.contains_key(&path) {
                    let v = match result {
                        Ok(crate::backend::Viewed::Pdf(bytes)) => {
                            let mut view = crate::pdf::view::PdfView::new(
                                backend.ctx(),
                                egui::Id::unique(("viewer", &path)),
                            );
                            view.load(bytes);
                            viewer::Viewer::Pdf(Box::new(view))
                        }
                        Ok(crate::backend::Viewed::Image(img)) => viewer::Viewer::Image {
                            size: img.size,
                            image: Some(img),
                            tex: None,
                            actual: false,
                        },
                        Err(e) => viewer::Viewer::Error(e),
                    };
                    self.viewers.insert(path, v);
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
            Msg::FileWritten { project, .. } if project != self.project => {}
            Msg::FileWritten {
                path,
                written,
                result,
                ..
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
                        self.after_save(backend, settings, &path);
                        self.move_comments(backend, &path, &written);
                        self.snapshot_after_save(backend);
                        self.refresh_git_status(backend);
                        if self.panel == Some(panels::Panel::Todos) {
                            self.load_todos(backend);
                        }
                        if path.ends_with(".tex") || path.ends_with(".bib") {
                            self.refresh_lookups(backend, false);
                        }
                        if path == self.info.main {
                            Self::set_macros(&written);
                            self.detected = crate::writing::spell::detect_language(&written);
                        }
                        if path.ends_with(".tex") {
                            self.refresh_outline(backend);
                        }
                        if self.closing_after_save.remove(&path) && !self.buffers.is_dirty(&path) {
                            self.drop_tab(backend, &path);
                        }
                    }
                    WriteResult::Conflict => {
                        self.no_build.remove(&path);
                        self.closing_after_save.remove(&path);
                    }
                    WriteResult::Error(e) => {
                        self.no_build.remove(&path);
                        self.adding_package = None;
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
            // Started for a project that has closed since.
            Msg::Apply(project, _) if project != self.project => {}
            Msg::Apply(_, f) => f(self, backend),
            Msg::LspStarted(Ok(generation)) => {
                self.lsp.generation = Some(generation);
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                self.lsp_tx = Some(tx);
                backend.spawn_async(move |shell| async move {
                    while let Some(body) = rx.recv().await {
                        if shell.lsp_send(generation, body).await.is_err() {
                            return Msg::LspLost(generation);
                        }
                    }
                    Msg::Done(Ok(()))
                });
                let init = self.lsp.initialize();
                self.lsp_send(backend, vec![init]);
            }
            Msg::LspStarted(Err(e)) => self.lsp.state = crate::lsp::State::Failed(e),
            Msg::LspLost(generation) => {
                if Some(generation) == self.lsp.generation
                    && self.lsp.state == crate::lsp::State::Ready
                {
                    self.lsp.state = crate::lsp::State::Failed("texlab stopped.".into());
                    self.notice(
                        "The language server (texlab) stopped answering, so completion and hover may be missing. Reopen the project to restart it.",
                        Tone::Info,
                    );
                }
            }
            Msg::Event(lemmata_shell::Event::LspMessage { generation, body }) => {
                if Some(generation) == self.lsp.generation {
                    let (answer, replies) = self.lsp.on_message(&body);
                    self.lsp_send(backend, replies);
                    if let Some(a) = answer {
                        self.on_lsp(backend, a);
                    }
                }
            }
            Msg::Event(lemmata_shell::Event::LspExit { generation }) => {
                if Some(generation) == self.lsp.generation {
                    self.lsp.state = crate::lsp::State::Failed("texlab stopped.".into());
                }
            }
            Msg::SpellLoaded(lang, Ok(())) => {
                if self
                    .spell_asked
                    .as_ref()
                    .map(|a| (a.0 != "off").then(|| a.0.clone()))
                    == Some(lang.clone())
                {
                    self.checks.spell.set_loaded(lang);
                }
            }
            Msg::SpellLoaded(_, Err(_)) => {
                // Allow a retry; until then no word is marked.
                self.spell_asked = None;
                self.checks.spell.set_loaded(None);
            }
            Msg::Dictionary(d) => {
                let size = (d.size_kb / 1024).max(1);
                self.notices.push(
                    Notice::new(
                        format!(
                            "Spell check in {} needs its dictionary (downloads {size} MB).",
                            d.label
                        ),
                        Tone::Info,
                    )
                    .action(NoticeAction::Download(d.code)),
                );
            }
            Msg::SpellDownloaded(code, Ok(())) => {
                self.spell_asked = None;
                self.notice(
                    format!(
                        "Spell check in {} is on.",
                        crate::writing::spell::label(&code)
                    ),
                    Tone::Ok,
                );
            }
            Msg::SpellDownloaded(_, Err(e)) => self.notice(e, Tone::Error),
            Msg::GrammarFound(target, text, result) => {
                self.grammar_out = self.grammar_out.saturating_sub(1);
                if self.checks.grammar.target.as_ref() == Some(&target) {
                    match result {
                        Ok(m) => self.checks.grammar.store(text, m),
                        Err(e) => {
                            // Stops at the first failure; a later edit retries.
                            self.checks.grammar.asked.remove(&text);
                            self.checks.grammar.error = Some(e);
                            self.grammar_queue.clear();
                        }
                    }
                }
                self.pump_grammar(backend);
            }
            Msg::Labels(Ok(l)) => {
                self.lookups.labels = l;
                self.lookups.generation += 1;
            }
            Msg::Cites(Ok(c)) => {
                self.lookups.cites = c;
                self.lookups.generation += 1;
            }
            Msg::Labels(Err(_)) | Msg::Cites(Err(_)) => {}
            Msg::Packages(p) => {
                self.lookups.packages = p;
                self.lookups.generation += 1;
            }
            Msg::HasPackage(pkg, has) => {
                if !has {
                    self.notices.push(
                        Notice::new(format!("This needs the {pkg} package, which your document doesn't load yet."), Tone::Info)
                            .action(NoticeAction::AddPackage(pkg)),
                    );
                }
            }
            Msg::Info(Ok(info)) => {
                self.info = info;
                self.tree_rows = None;
                self.refresh_lookups(backend, false);
                if std::mem::take(&mut self.build.after_config) {
                    self.build_soon(false, false);
                }
            }
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
            other => return self.on_build_msg(backend, settings, other),
        }
        None
    }

    /// Files or folders deleted by another program: their tabs close quietly, without saving.
    fn removed_outside(&mut self, backend: &Backend, removed: &[String]) {
        if !self.close_gone(backend, removed) {
            return;
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

    /// Closes the tabs of files that are gone, without saving. False when none was open.
    fn close_gone(&mut self, backend: &Backend, removed: &[String]) -> bool {
        let gone = |p: &str| {
            removed
                .iter()
                .any(|r| p == r || p.starts_with(&format!("{r}/")))
        };
        let closing: Vec<String> = self.editors.keys().filter(|p| gone(p)).cloned().collect();
        if closing.is_empty() {
            return false;
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
        true
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
            self.auto_writes.insert(p.clone());
            self.write(backend, &p, settings);
        }
        if let Some(t) = self.buffers.next_deadline() {
            ui.ctx()
                .request_repaint_after(t.saturating_duration_since(now));
        }
    }

    /// A dialog is open over the workspace.
    pub fn modal_open(&self) -> bool {
        self.symbols.is_some()
            || self.cite.is_some()
            || self.bib_manager.is_some()
            || self.figure.is_some()
            || self.table.is_some()
            || self.rename.is_some()
            || self.files.dialog.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.presentation.is_some()
            || self.words.is_some()
            || self.shortcut_sheet.is_some()
            || self.history.label.is_some()
    }

    fn shortcuts(
        &mut self,
        ctx: &egui::Context,
        backend: &Backend,
        settings: &mut Settings,
        action: &mut Option<Action>,
    ) {
        let ctx = ctx.clone();
        if crate::shortcuts::consume(&ctx, "save") {
            self.save_all(backend, settings);
        }
        if crate::shortcuts::consume(&ctx, "close-tab")
            && let Some(a) = self.active.clone()
        {
            self.close_tab(backend, &a, settings);
        }
        if crate::shortcuts::consume(&ctx, "recompile") {
            self.recompile(backend, settings);
        }
        if self.active.is_some() && crate::shortcuts::consume(&ctx, "symbols") {
            self.symbols = Some(Default::default());
        }
        if crate::shortcuts::consume(&ctx, "snapshot") {
            self.history.label = Some(String::new());
        }
        if self.active.is_some()
            && crate::shortcuts::consume(&ctx, "add-comment")
            && self.add_comment(backend, settings)
        {
            *action = Some(Action::Settings);
        }
        if self.active.is_some() && crate::shortcuts::consume(&ctx, "cite") {
            self.open_cite(backend, None);
        }
        if self.active.is_some() && crate::shortcuts::consume(&ctx, "rename") {
            self.open_rename(backend);
        }
        for (id, kind) in [
            ("palette", palette::Kind::Commands),
            ("quick-open", palette::Kind::Files),
            ("goto-heading", palette::Kind::Headings),
        ] {
            if crate::shortcuts::consume(&ctx, id) {
                self.open_palette(kind, settings);
            }
        }
        if crate::shortcuts::consume(&ctx, "shortcuts") {
            self.shortcut_sheet = Some(String::new());
        }
        if crate::shortcuts::consume(&ctx, "search") {
            self.open_search(None);
        }
        if crate::shortcuts::consume(&ctx, "sync") {
            self.sync_forward(backend);
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
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &mut Settings,
        backend: &Backend,
    ) -> Option<Action> {
        let mut action = None;
        self.indent = Indent::from_setting(&settings.indent);
        self.apply_language(backend, settings);
        if self.lookups.snippets != settings.snippets {
            self.lookups.snippets = settings.snippets.clone();
            self.lookups.generation += 1;
        }
        self.run_autosaves(ui, backend, settings);
        let ctx = ui.ctx().clone();
        self.poll_build(&ctx, backend, settings);

        // The app's own keys come before the editor's; an open dialog keeps them.
        self.tree_keys(&ctx, backend, settings);
        if !self.modal_open() {
            self.shortcuts(&ctx, backend, settings, &mut action);
        }
        self.snapshot_timer(backend);
        if std::mem::take(&mut self.trust_requested)
            && !settings.shell_escape_projects.contains(&self.root)
        {
            settings.shell_escape_projects.push(self.root.clone());
            action = Some(Action::Settings);
            self.build_soon(false, true);
        }
        if !self.writing() && !self.after_writes.is_empty() {
            for then in std::mem::take(&mut self.after_writes) {
                then(self, backend);
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
        // The search lives in the sidebar: make sure it can be seen.
        if self.search.as_ref().is_some_and(|s| s.wants_sidebar()) && !settings.sidebar_open {
            settings.sidebar_open = true;
            action = Some(Action::Settings);
        }
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

        if show_sidebar && self.search.is_some() {
            self.search_panel(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(side)),
                p,
                backend,
                settings,
            );
        } else if show_sidebar {
            chrome::sidebar(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(side)),
                p,
                self,
                settings,
                backend,
            );
        }
        if show_editor
            && self.code_column(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(code)),
                p,
                settings,
                backend,
            )
        {
            action = Some(Action::Settings);
        }
        if show_pdf {
            // The pane starts right of its 1 px border.
            let inner = if show_editor {
                pdf.with_min_x(pdf.left() + 1.0)
            } else {
                pdf
            };
            if self.panel.is_some() {
                // The PDF pane keeps its state while a panel covers it.
                self.show_panel(
                    &mut ui.new_child(egui::UiBuilder::new().max_rect(inner)),
                    p,
                    backend,
                    settings,
                );
            } else if self.pdf_pane(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(inner)),
                p,
                backend,
                settings,
            ) {
                action = Some(Action::Settings);
            }
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
                1400.0_f32.min(panes.width() - side_w - 320.0).max(300.0),
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
        self.bib_dialogs(&ctx, p, backend, settings);
        self.insert_dialogs(&ctx, p, backend);
        self.rename_dialog(&ctx, p, backend, settings);
        self.file_dialogs(&ctx, p, backend, settings);
        self.project_settings_dialog(&ctx, p, backend, settings, &mut action);
        self.sheets(&ctx, p);
        if let Some(show) = &mut self.presentation
            && !show.show(&ctx)
        {
            self.presentation = None;
        }
        if let Some(a) = self.palette_ui(&ctx, p, backend, settings) {
            action = Some(a);
        }
        self.snapshot_dialog(&ctx, p, backend);
        if let Some(pal) = &mut self.symbols {
            match pal.show(&ctx, p) {
                symbols::Picked::None => {}
                symbols::Picked::Close => self.symbols = None,
                symbols::Picked::Insert(tex, pkg, keep) => {
                    let template = crate::editor::complete::template_of(&tex);
                    let indent = self.indent;
                    if let Some(ed) = self.active_editor() {
                        ed.insert_template(&template, indent);
                    }
                    if let Some(pkg) = pkg {
                        self.needs_package(backend, pkg.to_owned());
                    }
                    if !keep {
                        self.symbols = None;
                    }
                }
            }
        }
        chrome::menus(&ctx, p, self, settings, backend, &mut action);
        self.toasts(&ctx, p, backend, settings);
        action
    }

    fn code_column(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &mut Settings,
        backend: &Backend,
    ) -> bool {
        let mut changed = false;
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
        let (sync, comment, cite) = chrome::edit_bar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(bar)),
            p,
            self,
        );
        if sync {
            self.sync_forward(backend);
        }
        if comment && self.add_comment(backend, settings) {
            changed = true;
        }
        if cite {
            self.open_cite(backend, None);
        }
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
        let mut body = Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), status.top()));
        // .cm-lsp-reference-panel: Find references' list, under the text.
        if let Some(refs) = self.references.clone() {
            let row = 13.0 * 1.4;
            let mut files = refs.iter().map(|r| &r.0).collect::<Vec<_>>();
            files.dedup();
            let h = ((refs.len() + files.len()) as f32 * row + 6.0).min(120.0);
            let panel = Rect::from_min_max(pos2(body.left(), body.bottom() - h), body.max);
            body.max.y = panel.top();
            ui.painter().rect_filled(panel, 0, p.surface_2);
            ui.painter().hline(
                panel.x_range(),
                panel.top() + 0.5,
                Stroke::new(1.0, p.border),
            );
            let mut c =
                ui.new_child(egui::UiBuilder::new().max_rect(panel.shrink2(vec2(6.0, 3.0))));
            let mut go = None;
            egui::ScrollArea::vertical()
                .id_salt("references")
                .auto_shrink([false, false])
                .show(&mut c, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut cur: Option<&String> = None;
                    for (i, (path, line, col, text)) in refs.iter().enumerate() {
                        if cur != Some(path) {
                            cur = Some(path);
                            ui.label(
                                egui::RichText::new(path)
                                    .font(FontId::new(13.0, theme::font::mono_500()))
                                    .color(p.ink),
                            );
                        }
                        let (r, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width(), row),
                            egui::Sense::click(),
                        );
                        if i == 0 {
                            ui.painter().rect_filled(
                                r,
                                0,
                                egui::Color32::from_rgba_unmultiplied(0x00, 0x77, 0xee, 0x44),
                            );
                        }
                        let mut job = egui::text::LayoutJob::default();
                        let f = |c: egui::Color32| egui::TextFormat {
                            font_id: FontId::monospace(13.0),
                            color: c,
                            ..Default::default()
                        };
                        job.append(
                            &format!("{:>5}", format!("{line}: ")),
                            0.0,
                            f(p.ink.gamma_multiply(0.7)),
                        );
                        job.append(text, 0.0, f(p.ink));
                        let g = ui.fonts_mut(|fo| fo.layout_job(job));
                        ui.painter().galley(
                            pos2(r.left(), r.center().y - g.size().y / 2.0),
                            g,
                            p.ink,
                        );
                        if resp
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            go = Some((path.clone(), *line, *col));
                        }
                    }
                });
            if let Some((path, line, _)) = go {
                self.goto(backend, &path, line);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.references = None;
            }
        }
        match self
            .active
            .clone()
            .and_then(|a| self.editors.contains_key(&a).then_some(a))
        {
            Some(path) => {
                let style = editor_style(ui.ctx(), settings, self.focus_mode);
                if self.editors[&path].problems_gen != self.build.diag_gen {
                    let list = self.problems_of(&path);
                    let generation = self.build.diag_gen;
                    self.editors
                        .get_mut(&path)
                        .unwrap()
                        .set_problems(generation, &list);
                }
                let key = (self.comments.generation, self.editors[&path].doc.version);
                if self.editors[&path].comments_key != key {
                    let ranges = self.comment_ranges(&path);
                    let ed = self.editors.get_mut(&path).unwrap();
                    ed.comments = ranges;
                    ed.comments_key = key;
                }
                let ed = self.editors.get_mut(&path).unwrap();
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let out = view::show(
                    &mut c,
                    p,
                    ed,
                    &style,
                    egui::Id::unique(("editor", &path)),
                    &format!("Text of {}", tree::base_name(&path)),
                    &self.lookups,
                    &mut self.checks,
                );
                let needs = out.needs_package.clone();
                let comment_click = out.comment_click.clone();
                let hover_search = out.hover_search.clone();
                let wrap_environment = out.wrap_environment;
                let pasted_table = out.pasted_table.clone();
                let pasted_image = out.pasted_image.clone();
                let dropped_files = out.dropped_files.clone();
                let (add_word, ignore_rule, grammar_check) = (
                    out.add_word.clone(),
                    out.ignore_rule.clone(),
                    out.grammar_check.clone(),
                );
                let lsp_asks = (out.server_complete, out.server_hover, out.server_goto);
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
                if let Some(pkg) = needs {
                    self.needs_package(backend, pkg);
                }
                if let Some((files, at)) = dropped_files {
                    self.drop_figures(backend, &path, files, at);
                }
                if let Some(img) = pasted_image {
                    self.paste_image(backend, &path, &img);
                }
                if let Some((from, table, text)) = pasted_table {
                    self.notices.push(
                        Notice::new("Pasted as a table.", Tone::Info)
                            .action(NoticeAction::PasteAsText(path.clone(), from, table, text)),
                    );
                }
                if wrap_environment {
                    self.open_palette(palette::Kind::Environments, settings);
                }
                match hover_search {
                    Some(crate::editor::hover::Search::Online(key)) => {
                        self.open_cite(backend, Some(key))
                    }
                    Some(crate::editor::hover::Search::Project(name)) => {
                        self.open_search(Some(name))
                    }
                    None => {}
                }
                if let Some(id) = comment_click {
                    self.comments.active = Some(id);
                    self.comments.generation += 1;
                    if self.open_panel(panels::Panel::Review, backend, settings) {
                        changed = true;
                    }
                }
                // Words and rules go into the project's settings, where they can be undone.
                if let Some(w) = add_word {
                    self.update_config(backend, |c| {
                        c.words.push(w);
                        c.words.sort();
                        c.words.dedup();
                    });
                }
                if let Some(id) = ignore_rule {
                    self.checks.grammar.ignored.insert(id.clone());
                    self.checks.grammar.generation += 1;
                    self.update_config(backend, |c| {
                        c.grammar_ignore.push(id);
                        c.grammar_ignore.sort();
                        c.grammar_ignore.dedup();
                    });
                }
                for t in grammar_check {
                    if self.checks.grammar.asked.insert(t.clone()) {
                        self.grammar_queue.push(t);
                    }
                }
                self.pump_grammar(backend);
                self.lsp_sync(backend);
                let ed = &self.editors[&path];
                let at = |line: usize, col: usize| crate::lsp::utf16_col(&ed.doc.line(line), col);
                let mut msgs = Vec::new();
                if let Some((head, from)) = lsp_asks.0 {
                    let n = ed.doc.line_of(head);
                    let u = at(n, head - ed.doc.line_start(n));
                    msgs.extend(self.lsp.complete(&path, n, u, head, from));
                }
                if let Some((line, col)) = lsp_asks.1
                    && self
                        .lookups
                        .hover
                        .as_ref()
                        .is_none_or(|h| h.0 != path || h.1 != line || h.2 != col)
                {
                    let u = at(line, col);
                    self.lookups.hover = Some((path.clone(), line, col, String::new()));
                    msgs.extend(self.lsp.hover(&path, line, u, col));
                }
                if let Some((line, col, references)) = lsp_asks.2 {
                    let u = at(line, col);
                    if references {
                        msgs.extend(self.lsp.references(&path, line, u));
                    } else {
                        msgs.extend(self.lsp.definition(&path, line, u));
                    }
                }
                self.lsp_send(backend, msgs);
            }
            None if self
                .active
                .as_ref()
                .is_some_and(|a| self.viewers.contains_key(a)) =>
            {
                let path = self.active.clone().unwrap();
                let v = self.viewers.get_mut(&path).unwrap();
                viewer::show(
                    &mut ui.new_child(egui::UiBuilder::new().max_rect(body)),
                    p,
                    &path,
                    v,
                );
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
        if chrome::status_bar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(status)),
            p,
            self,
            settings,
            backend,
        ) {
            changed = true;
        }
        changed
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

    /// `.toast`: the newest notice at the bottom centre. Errors and offers to act stay until
    /// dismissed; the rest go after 8 seconds.
    fn toasts(&mut self, ctx: &egui::Context, p: &Palette, backend: &Backend, settings: &Settings) {
        self.notices.retain(|n| {
            n.action.is_some() || n.tone == Tone::Error || n.at.elapsed() < Duration::from_secs(8)
        });
        let Some(n) = self.notices.last() else { return };
        if n.action.is_none() && n.tone != Tone::Error {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let (text, tone, act) = (n.text.clone(), n.tone, n.action.clone());
        let screen = ctx.content_rect();
        let mut dismissed = false;
        let mut run = None;
        egui::Area::new(egui::Id::unique("toast"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(screen.center().x, screen.bottom() - 20.0))
            .show(ctx, |ui| {
                ui.set_max_width((screen.width() * 0.9).min(720.0));
                let border = if tone == Tone::Error {
                    p.error
                } else {
                    p.border
                };
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(theme::R_LG)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 8,
                        top: 8,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let (icon, color) = match tone {
                                Tone::Ok => (crate::icons::CIRCLE_CHECK, p.ok),
                                Tone::Error => (crate::icons::INFO, p.error),
                                Tone::Info => (crate::icons::INFO, p.accent),
                            };
                            crate::icons::show(ui, icon, 16.0, color);
                            ui.add(
                                egui::Label::new(egui::RichText::new(&text).color(p.ink)).wrap(),
                            );
                            if let Some(a) = &act
                                && Button::new(a.label())
                                    .size(widgets::Size::Sm)
                                    .show(ui, p)
                                    .clicked()
                            {
                                run = Some(a.clone());
                            }
                            if Button::icon(crate::icons::X, "Dismiss")
                                .size(widgets::Size::Sm)
                                .icon_size(14.0)
                                .show_icon(ui, p)
                                .clicked()
                            {
                                dismissed = true;
                            }
                        });
                    });
            });
        if dismissed || run.is_some() {
            self.notices.pop();
        }
        if let Some(a) = run {
            self.notice_action(backend, settings, a);
        }
    }
}

/// The editor's look from the settings: Plex Mono at the font size, the line height, indentation.
pub fn editor_style(ctx: &egui::Context, s: &Settings, focus: bool) -> view::Style {
    let size = s.font_size as f32;
    view::Style {
        font: FontId::new(size, crate::theme::editor_family(ctx, &s.font_family)),
        line_height: size * s.line_height as f32,
        indent: Indent::from_setting(&s.indent),
        wrap: s.word_wrap,
        auto_close: s.auto_close,
        folding: s.folding,
        env_sync: s.env_sync,
        autocomplete: s.autocomplete,
        math_preview: true,
        focus,
    }
}

impl Workspace {
    /// "Present full screen": the built PDF, from the page on show.
    pub fn present(&mut self, ctx: &egui::Context) {
        match self.pane.view.shown_bytes() {
            Some(bytes) => {
                let page = self.pane.view.current_page.max(1);
                self.presentation = Some(crate::pdf::present::Presentation::new(ctx, bytes, page));
            }
            None => self.notice(
                "There is no PDF to present yet. Build the project first.",
                Tone::Info,
            ),
        }
    }

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
