//! The app: which screen shows, and what the screens' actions do. This is what App.svelte did.

use crate::backend::{Backend, Msg, Pick, PrefsMsg};
use crate::screens::{dialogs, library, onboarding, prefs, report, welcome, whats_new};
use crate::store::{Dirs, Groups, Recents, Settings};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::workspace::{self, Workspace};
use eframe::egui::{self, FontId};
use lemmata_shell::open_with::Request;
use lemmata_shell::{Event, InstallRequest, TexStatus};

pub struct App {
    dirs: Dirs,
    settings: Settings,
    recents: Recents,
    groups: Groups,
    palette: Palette,
    backend: Backend,
    tex: Option<TexStatus>,
    checking: bool,
    /// The first-run welcome, and its step.
    welcome: Option<usize>,
    onboarding: Option<onboarding::State>,
    /// "Continue without compiling": projects can be edited, nothing compiles until TeX is found.
    without_tex: bool,
    library: library::State,
    open_error: Option<String>,
    no_main_in: Option<String>,
    project: Option<Workspace>,
    /// Startup has decided between the welcome, the last project and the library.
    started: bool,
    dialog: Option<Dialog>,
    prefs: Option<prefs::Prefs>,
    /// "Report a problem": the system summary and the errors.
    report: Option<(String, String)>,
    /// A file the OS asked to open, once its project has opened.
    open_file_after: Option<String>,
    /// What's new since the version used last, once after an update.
    whats_new: Vec<&'static whats_new::Release>,
    quit: Quit,
}

/// Closing the window with unsaved files (App.svelte onCloseRequested).
#[derive(Default, PartialEq)]
enum Quit {
    #[default]
    No,
    /// Saving first; the window closes once everything is on disk.
    Saving,
    /// Asking: these files aren't saved (and why, when saving failed).
    Asking(Vec<String>, Option<String>),
    /// Closing now.
    Closing,
}

/// The dialog open over the welcome or the library.
enum Dialog {
    NewProject(dialogs::NewProject),
    Clone(dialogs::Clone),
    Import(dialogs::Import),
    SaveTemplate(dialogs::SaveTemplate),
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_dirs(cc, Dirs::system())
    }

    pub fn with_dirs(cc: &eframe::CreationContext<'_>, dirs: Dirs) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let settings = Settings::load(&dirs);
        let palette = palette_for(&cc.egui_ctx, &settings);
        theme::apply(&cc.egui_ctx, &palette);
        let backend = Backend::new(cc.egui_ctx.clone(), &dirs);
        #[cfg(not(debug_assertions))]
        {
            // Later launches hand their files and links to this one, which comes to the front.
            let (shell, ctx) = (backend.shell.clone(), cc.egui_ctx.clone());
            let _ = crate::instance::listen(&dirs.cache, move |args, cwd| {
                shell.on_second_launch(&args, &cwd);
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            });
            std::thread::spawn(crate::instance::register_scheme);
        }
        let tex_dir = settings.tex_dir.clone();
        backend.spawn(move |shell| Msg::Tex(shell.tex_status(tex_dir)));
        Self {
            recents: Recents::load(&dirs),
            groups: Groups::load(&dirs),
            dirs,
            settings,
            palette,
            backend,
            tex: None,
            checking: true,
            welcome: None,
            onboarding: None,
            without_tex: false,
            library: library::State::default(),
            open_error: None,
            no_main_in: None,
            project: None,
            started: false,
            dialog: None,
            prefs: None,
            report: None,
            open_file_after: None,
            whats_new: Vec::new(),
            quit: Quit::No,
        }
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings.save(&self.dirs);
        let palette = palette_for(ctx, &self.settings);
        if palette != self.palette {
            self.palette = palette;
            theme::apply(ctx, &palette);
        }
    }

    fn recheck(&mut self, dir: Option<String>) {
        self.checking = true;
        self.backend
            .spawn(move |shell| Msg::Tex(shell.tex_status(dir)));
    }

    fn open_project(&mut self, path: String) {
        self.open_error = None;
        self.no_main_in = None;
        let p = path.clone();
        self.backend
            .spawn(move |shell| Msg::Opened(p.clone(), shell.open_project(p)));
    }

    fn handle_request(&mut self, request: Request) {
        match request {
            Request::Open(target) => {
                self.open_file_after = target.file;
                self.open_project(target.project_root.to_string_lossy().into_owned())
            }
            // The dialogs live on the start screen: the project closes (saving) first.
            Request::Clone { url } => {
                self.close_project();
                self.dialog = Some(Dialog::Clone(dialogs::Clone::new(
                    self.default_parent(),
                    url,
                )))
            }
            Request::NewProject { template } => {
                self.close_project();
                self.open_new_project(template)
            }
        }
    }

    /// A sensible place for new projects: next to the last one, else Documents.
    fn default_parent(&self) -> String {
        match self.recents.0.first() {
            Some(r) => {
                let p = r.path.trim_end_matches(['/', '\\']);
                p.rfind(['/', '\\'])
                    .map_or(String::new(), |i| p[..i].to_owned())
            }
            None => dirs::document_dir()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    fn open_new_project(&mut self, initial: String) {
        self.dialog = Some(Dialog::NewProject(dialogs::NewProject::new(
            self.default_parent(),
            initial,
        )));
        // The list on disk first, then the collection's templates arrive in the background.
        self.backend
            .spawn(|shell| Msg::Templates(shell.list_templates(), None));
        self.backend.spawn_async(|shell| async move {
            let refreshed = shell.refresh_templates().await.map(|_| ());
            Msg::Templates(shell.list_templates(), Some(refreshed))
        });
    }

    /// New, cloned and imported projects land in the folder the library shows, and open.
    fn added(&mut self, paths: Vec<String>) {
        self.recents.remember_all(&paths, now_ms());
        self.recents.save(&self.dirs);
        if self.library.file(&mut self.groups, &paths) {
            self.groups.save(&self.dirs);
        }
        self.welcome = None;
        if !self.settings.first_run_done {
            self.settings.first_run_done = true;
            self.settings.save(&self.dirs);
        }
        if paths.len() == 1 {
            self.open_project(paths[0].clone());
        } else {
            // shortcut: the Svelte app showed this as a toast; toasts come with the editor's notices (M3).
            self.library.template_note = Some((
                format!("Imported {} projects.", paths.len()),
                std::time::Instant::now(),
            ));
        }
    }

    fn on_dialog_msg(&mut self, msg: Msg) -> Option<Msg> {
        let d = self.dialog.as_mut();
        match (msg, d) {
            (Msg::Templates(list, refreshed), Some(Dialog::NewProject(st))) => {
                let first = st.templates.is_empty();
                if let Ok(list) = list {
                    st.set_templates(list, first);
                }
                if let Some(r) = refreshed {
                    st.online_loading = false;
                    st.online_error = r.err();
                }
            }
            (Msg::TemplateDeleted(id, r), Some(Dialog::NewProject(st))) => match r {
                Ok(()) => st.removed(&id),
                Err(e) => st.error = Some(e),
            },
            (Msg::Created(r), Some(Dialog::NewProject(st))) => match r {
                Ok(path) => {
                    self.dialog = None;
                    self.added(vec![path.to_string_lossy().into_owned()]);
                }
                Err(e) => {
                    st.busy = false;
                    st.error = Some(e);
                }
            },
            (Msg::Cloned(r), Some(Dialog::Clone(st))) => match r {
                Ok(path) => {
                    self.dialog = None;
                    self.added(vec![path]);
                }
                Err(e) => {
                    st.busy = false;
                    st.error = Some(e);
                }
            },
            (Msg::ZipInfo(r), Some(Dialog::Import(st))) => match r {
                Ok(info) => st.set_info(info),
                Err(e) => st.error = Some(e),
            },
            (Msg::Imported(r), Some(Dialog::Import(st))) => match r {
                Ok(paths) => {
                    self.dialog = None;
                    self.added(
                        paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect(),
                    );
                }
                Err(e) => {
                    st.busy = false;
                    st.error = Some(e);
                }
            },
            (Msg::TemplateSaved(path, trash, r), Some(Dialog::SaveTemplate(st))) => match r {
                Ok(_) => {
                    let name = st.name.trim().to_owned();
                    self.dialog = None;
                    self.library.template_note = Some((
                        format!(
                            "Template “{name}” saved. It's offered under My templates when you create a project."
                        ),
                        std::time::Instant::now(),
                    ));
                    if trash {
                        self.library.deleting = Some(library::Deleting {
                            path: path.clone(),
                            busy: true,
                            error: None,
                        });
                        self.backend.spawn(move |shell| {
                            Msg::Trashed(path.clone(), shell.trash_project(path))
                        });
                    }
                }
                Err(e) => {
                    st.busy = false;
                    st.error = Some(e);
                }
            },
            (Msg::Picked(Pick::Parent, Some(dir)), Some(d)) => {
                let dir = dir.to_string_lossy().into_owned();
                match d {
                    Dialog::NewProject(st) => st.parent = dir,
                    Dialog::Clone(st) => st.parent = dir,
                    Dialog::Import(st) => st.parent = dir,
                    Dialog::SaveTemplate(_) => {}
                }
            }
            // A dialog closed since: its answer has nowhere to go.
            (
                Msg::Templates(..)
                | Msg::TemplateDeleted(..)
                | Msg::Created(_)
                | Msg::Cloned(_)
                | Msg::ZipInfo(_)
                | Msg::Imported(_)
                | Msg::TemplateSaved(..)
                | Msg::Picked(Pick::Parent, _),
                _,
            ) => {}
            (other, _) => return Some(other),
        }
        None
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        let Some(d) = &mut self.dialog else { return };
        let action = match d {
            Dialog::NewProject(st) => dialogs::new_project(ctx, &p, st),
            Dialog::Clone(st) => dialogs::clone(ctx, &p, st),
            Dialog::Import(st) => dialogs::import(ctx, &p, st),
            Dialog::SaveTemplate(st) => dialogs::save_template(ctx, &p, st),
        };
        let Some(action) = action else { return };
        match (action, d) {
            (dialogs::Action::Close, _) => self.dialog = None,
            (dialogs::Action::ChooseParent, _) => self.backend.pick(Pick::Parent),
            (dialogs::Action::DeleteTemplate(id), _) => {
                self.backend.spawn(move |shell| {
                    Msg::TemplateDeleted(id.clone(), shell.delete_template(id))
                });
            }
            (dialogs::Action::Submit, Dialog::NewProject(st)) => {
                st.busy = true;
                st.error = None;
                let (t, parent, name) = (st.chosen.clone(), st.parent.clone(), st.name.clone());
                self.backend
                    .spawn(move |shell| Msg::Created(shell.create_project(t, parent, name)));
            }
            (dialogs::Action::Submit, Dialog::Clone(st)) => {
                st.busy = true;
                st.error = None;
                let (url, parent, name) = (st.url.clone(), st.parent.clone(), st.name());
                self.backend.spawn_async(move |shell| async move {
                    Msg::Cloned(shell.git_clone(url, parent, name).await)
                });
            }
            (dialogs::Action::Submit, Dialog::Import(st)) => {
                st.busy = true;
                st.error = None;
                let (zip, parent, name) = (st.zip.clone(), st.parent.clone(), st.name.clone());
                self.backend.spawn(move |shell| {
                    Msg::Imported(shell.import_overleaf_zip(zip, parent, name))
                });
            }
            (dialogs::Action::Submit, Dialog::SaveTemplate(st)) => {
                st.busy = true;
                st.error = None;
                let (path, name, desc, trash) = (
                    st.path.clone(),
                    st.name.clone(),
                    st.description.clone(),
                    st.trash,
                );
                self.backend.spawn(move |shell| {
                    Msg::TemplateSaved(
                        path.clone(),
                        trash,
                        shell.save_as_template(path, name, desc),
                    )
                });
            }
        }
    }

    fn on_msg(&mut self, ctx: &egui::Context, msg: Msg) {
        let Some(msg) = self.on_dialog_msg(msg) else {
            return;
        };
        // The open project's answers go to it.
        let msg = match &mut self.project {
            Some(ws) => match ws.on_msg(&self.backend, &self.settings, msg) {
                Some(m) => m,
                None => return,
            },
            None => msg,
        };
        match msg {
            Msg::Tex(status) => {
                self.checking = false;
                if !status.detection.ready && self.onboarding.is_none() {
                    self.onboarding = Some(onboarding::State::new(&status));
                }
                self.tex = Some(status);
                if !self.started {
                    self.started = true;
                    self.start();
                }
            }
            Msg::Opened(path, Ok(info)) => {
                self.recents.remember(&path, now_ms());
                self.recents.save(&self.dirs);
                self.close_project();
                let ready = self.tex.as_ref().is_some_and(|t| t.detection.ready);
                let mut ws = Workspace::open(info, &self.dirs, &self.backend, ready);
                // A project's latexmkrc is a Perl program: it runs only once the user says so.
                let rc = ["latexmkrc", ".latexmkrc"]
                    .iter()
                    .any(|f| ws.info.root.join(f).is_file());
                if rc && !self.settings.shell_escape_projects.contains(&ws.root) {
                    ws.notices.push(
                        workspace::Notice::new(
                            "This project has a latexmkrc, which can run programs on your computer, so builds leave it out. Use it only if you trust the project.",
                            workspace::Tone::Info,
                        )
                        .action(workspace::NoticeAction::TrustProject),
                    );
                }
                if let Some(file) = self.open_file_after.take() {
                    ws.open_requested(&self.backend, &file);
                }
                self.project = Some(ws);
            }
            Msg::Opened(path, Err(e)) => {
                self.open_file_after = None;
                // A folder without a main document can get one.
                if e.contains("main")
                    && std::path::Path::new(&path).is_dir()
                    && !std::path::Path::new(&path).join("main.tex").exists()
                {
                    self.no_main_in = Some(path);
                }
                self.open_error = Some(e);
            }
            Msg::Picked(Pick::ProjectFolder, Some(dir)) => {
                let dir = dir.to_string_lossy().into_owned();
                if self
                    .library
                    .file(&mut self.groups, std::slice::from_ref(&dir))
                {
                    self.groups.save(&self.dirs);
                }
                self.open_project(dir);
            }
            Msg::Picked(Pick::TexFolder, Some(dir)) => {
                let dir = dir.to_string_lossy().into_owned();
                self.settings.tex_dir = Some(dir.clone());
                self.save_settings(ctx);
                self.recheck(Some(dir));
            }
            Msg::Picked(Pick::OverleafZip, Some(zip)) => {
                let zip = zip.to_string_lossy().into_owned();
                self.dialog = Some(Dialog::Import(dialogs::Import::new(
                    zip.clone(),
                    self.default_parent(),
                )));
                self.backend
                    .spawn(move |shell| Msg::ZipInfo(shell.inspect_zip(zip)));
            }
            Msg::Picked(..) => {}
            // Dialog answers arrive here only when no dialog is open.
            Msg::Templates(..)
            | Msg::TemplateDeleted(..)
            | Msg::Created(_)
            | Msg::Cloned(_)
            | Msg::ZipInfo(_)
            | Msg::Imported(_)
            | Msg::TemplateSaved(..) => {}
            Msg::Trashed(path, Ok(())) => {
                self.library.deleting = None;
                self.library.thumbs.remove(&path);
                self.groups.forget(&path);
                self.groups.save(&self.dirs);
                self.recents.forget(&path);
                self.recents.save(&self.dirs);
            }
            Msg::Trashed(path, Err(e)) => {
                self.library.deleting = Some(library::Deleting {
                    path,
                    busy: false,
                    error: Some(e),
                });
            }
            Msg::Tectonic(r) => {
                if let Some(o) = &mut self.onboarding {
                    o.installing = false;
                    o.tectonic_error = r.as_ref().err().cloned();
                }
                if r.is_ok() {
                    self.recheck(None);
                }
            }
            Msg::Terminal(r) => {
                if let Some(o) = &mut self.onboarding {
                    o.terminal_error = r.err();
                }
            }
            Msg::Example(Ok(dir)) => self.open_project(dir.to_string_lossy().into_owned()),
            Msg::Example(Err(e)) => self.open_error = Some(e),
            Msg::Event(Event::OpenRequest(r)) => {
                if !self.settings.first_run_done {
                    self.settings.first_run_done = true;
                    self.save_settings(ctx);
                }
                self.welcome = None;
                self.handle_request(r);
            }
            Msg::Done(Err(e)) => {
                if let Some(ws) = &mut self.project {
                    ws.notice(e, workspace::Tone::Error);
                }
            }
            Msg::Event(_) | Msg::Done(Ok(())) => {}
            // Answers for a project that has closed since.
            Msg::FileRead(..)
            | Msg::DiskText(..)
            | Msg::FileWritten { .. }
            | Msg::Outline(_)
            | Msg::Info(_)
            | Msg::Built(..)
            | Msg::Pdf(_)
            | Msg::PdfCurrent(_)
            | Msg::Synced(_)
            | Msg::SourceHit(_)
            | Msg::Log(..)
            | Msg::Cleaned(_)
            | Msg::SaveTo(..)
            | Msg::Exported(..)
            | Msg::PackageAdded(_)
            | Msg::Viewed(..)
            | Msg::Labels(_)
            | Msg::Cites(_)
            | Msg::Packages(_)
            | Msg::HasPackage(..)
            | Msg::LspStarted(_)
            | Msg::LspLost(_)
            | Msg::SpellLoaded(..)
            | Msg::Dictionary(_)
            | Msg::SpellDownloaded(..)
            | Msg::GrammarFound(..)
            | Msg::Apply(..) => {}
            Msg::Prefs(m) => self.on_prefs_msg(ctx, m),
        }
    }

    fn open_prefs(&mut self) {
        self.prefs = Some(prefs::Prefs::default());
        self.backend.spawn(|shell| {
            Msg::Prefs(PrefsMsg::Dictionaries(
                shell.spell_available().unwrap_or_default(),
            ))
        });
    }

    fn on_prefs_msg(&mut self, ctx: &egui::Context, m: PrefsMsg) {
        let Some(st) = &mut self.prefs else { return };
        match m {
            PrefsMsg::Dictionaries(d) => st.dicts = d,
            PrefsMsg::Downloaded(code, Ok(())) => {
                st.downloading = None;
                self.settings.spell_default = code;
                self.save_settings(ctx);
                self.backend.spawn(|shell| {
                    Msg::Prefs(PrefsMsg::Dictionaries(
                        shell.spell_available().unwrap_or_default(),
                    ))
                });
            }
            PrefsMsg::Downloaded(_, Err(e)) => {
                st.downloading = None;
                st.spell_error = Some(e);
            }
            PrefsMsg::Grammar(r) => {
                st.grammar_status = Some(r.err().unwrap_or_else(|| "connected".into()))
            }
        }
    }

    fn show_prefs(&mut self, ctx: &egui::Context) {
        let Some(st) = &mut self.prefs else { return };
        let system_dark = ctx.system_theme() == Some(egui::Theme::Dark);
        let p = self.palette;
        match prefs::show(
            ctx,
            &p,
            st,
            &mut self.settings,
            self.tex.as_ref(),
            system_dark,
        ) {
            None => {}
            Some(prefs::Out::Changed) => self.save_settings(ctx),
            Some(prefs::Out::Close) => self.prefs = None,
            Some(prefs::Out::ChooseTex) => self.backend.pick(Pick::TexFolder),
            Some(prefs::Out::AutoTex) => {
                self.settings.tex_dir = None;
                self.save_settings(ctx);
                self.recheck(None);
            }
            Some(prefs::Out::Download(code)) => self.backend.spawn_async(move |shell| async move {
                let r = shell.spell_download(code.clone()).await;
                Msg::Prefs(PrefsMsg::Downloaded(code, r))
            }),
            Some(prefs::Out::TestGrammar) => {
                let url = if self.settings.grammar == "custom" {
                    self.settings.grammar_url.trim().to_owned()
                } else {
                    crate::writing::grammar::LOCAL_URL.to_owned()
                };
                self.backend.spawn_async(move |shell| async move {
                    Msg::Prefs(PrefsMsg::Grammar(shell.grammar_status(url).await))
                });
            }
        }
    }

    /// Existing users (with projects) never see the welcome. A file or folder opened from the OS
    /// wins over the last project.
    fn start(&mut self) {
        // Once after an update, what the new version can do. A fresh install gets the welcome instead.
        let used_before = self.settings.first_run_done || !self.recents.0.is_empty();
        let version = env!("CARGO_PKG_VERSION");
        if self.settings.last_seen_version != version {
            self.whats_new =
                whats_new::to_show(&self.settings.last_seen_version, version, used_before);
            self.settings.last_seen_version = version.into();
            self.settings.save(&self.dirs);
        }
        let request = self.backend.shell.take_open_request();
        if let Some(r) = request {
            self.settings.first_run_done = true;
            self.settings.save(&self.dirs);
            self.handle_request(r);
        } else if !self.settings.first_run_done && self.recents.0.is_empty() {
            self.welcome = Some(0);
        } else {
            if !self.settings.first_run_done {
                self.settings.first_run_done = true;
                self.settings.save(&self.dirs);
            }
            let ready = self.tex.as_ref().is_some_and(|t| t.detection.ready);
            if let (true, Some(last)) = (ready, self.recents.0.first()) {
                let path = last.path.clone();
                self.open_project(path);
            }
        }
    }

    /// Closing the window saves first. With autosave on that is silent; otherwise, or when saving
    /// fails, it asks.
    fn closing(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        if ctx.input(|i| i.viewport().close_requested()) && self.quit != Quit::Closing {
            let unsaved = self
                .project
                .as_ref()
                .map(|w| w.buffers.unsaved())
                .unwrap_or_default();
            if !unsaved.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.settings.autosave_ms > 0.0 {
                    if let Some(ws) = &mut self.project {
                        ws.save_all(&self.backend, &self.settings);
                    }
                    self.quit = Quit::Saving;
                } else {
                    self.quit = Quit::Asking(unsaved, None);
                }
            } else {
                self.close_project();
            }
        }
        if self.quit == Quit::Saving
            && let Some(ws) = &mut self.project
            && !ws.writing()
        {
            let unsaved = ws.buffers.unsaved();
            if !unsaved.is_empty()
                && ws.buffers.failed.is_empty()
                && ws.buffers.conflicts.is_empty()
            {
                // Typed while the last save ran: save that too.
                ws.save_all(&self.backend, &self.settings);
            } else if unsaved.is_empty() {
                self.quit = Quit::Closing;
                self.close_project();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                let failed = ws.buffers.failed.clone();
                let why = if ws.buffers.conflicts.is_empty() {
                    failed.join(", ")
                } else {
                    "they changed on disk".into()
                };
                self.quit = Quit::Asking(unsaved, Some(why));
            }
        }
        let Quit::Asking(files, error) = &self.quit else {
            return;
        };
        let (files, error) = (files.clone(), error.clone());
        let mut choice = None;
        widgets::modal(
            ctx,
            &p,
            "Close Lemmata?",
            480.0,
            |ui| {
                let text = match &error {
                    Some(e) => format!("These files could not be saved: {e}"),
                    None => "These files have changes that aren't saved:".into(),
                };
                ui.label(egui::RichText::new(text).color(p.ink));
                for f in &files {
                    ui.label(
                        egui::RichText::new(format!("• {f}"))
                            .font(FontId::monospace(12.0))
                            .color(p.ink),
                    );
                }
            },
            Some(&mut |ui: &mut egui::Ui| {
                if error.is_none()
                    && widgets::Button::new("Save and close")
                        .kind(widgets::Kind::Primary)
                        .show(ui, &p)
                        .clicked()
                {
                    choice = Some(2);
                }
                if widgets::Button::new("Close without saving")
                    .kind(widgets::Kind::Primary)
                    .fill(p.error)
                    .ink(egui::Color32::WHITE)
                    .show(ui, &p)
                    .clicked()
                {
                    choice = Some(1);
                }
                if widgets::Button::new("Cancel").show(ui, &p).clicked() {
                    choice = Some(0);
                }
            }),
        );
        match choice {
            Some(0) => self.quit = Quit::No,
            Some(1) => {
                // Nothing more is saved; the session is still remembered.
                if let Some(ws) = self.project.take() {
                    workspace::session::save(&self.dirs, &ws.root, &ws.session());
                }
                self.quit = Quit::Closing;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(_) => {
                if let Some(ws) = &mut self.project {
                    ws.save_all(&self.backend, &self.settings);
                }
                self.quit = Quit::Saving;
            }
            None => {}
        }
    }

    /// Saves what's unsaved and remembers where the user was, then shows the library.
    fn close_project(&mut self) {
        if let Some(mut ws) = self.project.take() {
            ws.save_all(&self.backend, &self.settings);
            ws.stop_lsp(&self.backend);
            workspace::session::save(&self.dirs, &ws.root, &ws.session());
        }
    }

    fn finish_welcome(&mut self, ctx: &egui::Context, example: bool) {
        self.welcome = None;
        self.settings.first_run_done = true;
        self.save_settings(ctx);
        if example {
            self.backend
                .spawn(|shell| Msg::Example(shell.create_example()));
        }
    }

    fn on_library(&mut self, ctx: &egui::Context, action: library::Action) {
        use library::Action as A;
        match action {
            A::Open(path) => self.open_project(path),
            A::Forget(path) => {
                self.recents.forget(&path);
                self.recents.save(&self.dirs);
                self.groups.save(&self.dirs);
            }
            A::OpenFolder => self.backend.pick(Pick::ProjectFolder),
            A::Import => self.backend.pick(Pick::OverleafZip),
            A::Example => self.finish_welcome(ctx, true),
            A::Welcome => self.welcome = Some(0),
            A::Reveal(path) => {
                // shortcut: opens the project folder itself; Tauri's reveal selected it in its parent.
                let _ = open::that_detached(&path);
            }
            A::Trash(path) => {
                let p = path.clone();
                self.backend
                    .spawn(move |shell| Msg::Trashed(p.clone(), shell.trash_project(p)));
            }
            A::CreateMain(dir) => {
                if let Err(e) = self.backend.shell.create_main(dir.clone()) {
                    self.open_error = Some(e);
                } else {
                    self.open_project(dir);
                }
            }
            A::Settings => self.save_settings(ctx),
            A::Groups => self.groups.save(&self.dirs),
            A::NewProject => self.open_new_project(String::new()),
            A::Clone => {
                self.dialog = Some(Dialog::Clone(dialogs::Clone::new(
                    self.default_parent(),
                    String::new(),
                )))
            }
            A::SaveTemplate(path) => {
                let name = library::name_of(&path).to_owned();
                self.dialog = Some(Dialog::SaveTemplate(dialogs::SaveTemplate::new(path, name)));
            }
            A::Preferences => self.open_prefs(),
        }
    }
}

/// The palette the theme setting asks for, given the system's scheme.
fn palette_for(ctx: &egui::Context, s: &Settings) -> Palette {
    if s.dark(ctx.system_theme() == Some(egui::Theme::Dark)) {
        theme::DARK
    } else {
        theme::LIGHT
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(msg) = self.backend.rx.try_recv() {
            self.on_msg(&ctx, msg);
        }
        // The system's scheme can change while the app runs.
        let palette = palette_for(&ctx, &self.settings);
        if palette != self.palette {
            self.palette = palette;
            theme::apply(&ctx, &palette);
        }
        self.screen(ui);
        self.show_dialog(&ctx);
        self.show_prefs(&ctx);
        if !self.whats_new.is_empty()
            && self.welcome.is_none()
            && whats_new::show(&ctx, &self.palette, &self.whats_new)
        {
            self.whats_new.clear();
        }
        if let Some((system, log)) = &self.report {
            match report::show(&ctx, &self.palette, system, log) {
                None => {}
                Some(report::Out::Close) => self.report = None,
                Some(report::Out::Open(url)) => {
                    self.report = None;
                    self.backend
                        .spawn(move |shell| Msg::Done(shell.open_url(url)));
                }
            }
        }
        self.closing(&ctx);
    }

    fn on_exit(&mut self) {
        // Quitting mid-build must not leave latexmk, pdflatex and biber running.
        self.backend.shell.stop_build();
    }
}

impl App {
    fn screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = self.palette;
        let rect = ui.max_rect();
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let ui = &mut ui;

        let Some(tex) = &self.tex else {
            // The boot splash of index.html: the wordmark in Young Serif 40 px, accent on --app.
            ui.painter().rect_filled(rect, 0, p.app);
            widgets::centered_text(
                ui,
                rect,
                "Lemmata",
                FontId::new(40.0, theme::font::display()),
                p.accent,
            );
            return;
        };

        if let Some(step) = &mut self.welcome
            && tex.detection.ready
        {
            {
                if let Some(a) = welcome::show(ui, &p, &tex.detection, step, &mut self.settings) {
                    match a {
                        welcome::Action::Done => self.finish_welcome(&ctx, false),
                        welcome::Action::Example => self.finish_welcome(&ctx, true),
                        welcome::Action::OpenFolder => {
                            self.welcome = None;
                            self.backend.pick(Pick::ProjectFolder);
                        }
                        welcome::Action::Import => self.backend.pick(Pick::OverleafZip),
                        welcome::Action::Settings => self.save_settings(&ctx),
                        welcome::Action::NewProject => self.open_new_project(String::new()),
                    }
                }
                return;
            }
        }
        if !tex.detection.ready && !self.without_tex {
            let st = self
                .onboarding
                .get_or_insert_with(|| onboarding::State::new(tex));
            let action = onboarding::show(ui, &p, tex, self.checking, true, st);
            match action {
                Some(onboarding::Action::Recheck(dir)) => self.recheck(dir),
                Some(onboarding::Action::ChooseFolder) => self.backend.pick(Pick::TexFolder),
                Some(onboarding::Action::Skip) => self.without_tex = true,
                Some(onboarding::Action::UseTectonic) => {
                    st.installing = true;
                    self.backend
                        .spawn(|shell| Msg::Tectonic(shell.tectonic_install().map(|_| ())));
                }
                Some(onboarding::Action::OpenTerminal { platform, full }) => {
                    self.backend.spawn(move |shell| {
                        Msg::Terminal(shell.open_install_terminal(InstallRequest::Tex {
                            platform: platform.clone(),
                            full,
                        }))
                    });
                }
                None => {}
            }
            return;
        }

        if let Some(ws) = &mut self.project {
            ws.tex_ready = tex.detection.ready;
            ws.tex_outdated = tex.detection.outdated;
            match ws.show(ui, &p, &mut self.settings, &self.backend) {
                Some(workspace::Action::Library) => self.close_project(),
                Some(workspace::Action::Settings) => self.save_settings(&ctx),
                Some(workspace::Action::Preferences) => self.open_prefs(),
                Some(workspace::Action::Snippets) => {
                    self.open_prefs();
                    if let Some(st) = &mut self.prefs {
                        st.snippets = Some(self.settings.snippets.clone());
                        st.only_snippets = true;
                    }
                }
                Some(workspace::Action::Report) => {
                    let log = self
                        .project
                        .as_ref()
                        .map(|w| report::error_summary(&w.build.diagnostics))
                        .unwrap_or_default();
                    self.report = Some((
                        report::system_summary(env!("CARGO_PKG_VERSION"), self.tex.as_ref()),
                        log,
                    ));
                }
                Some(workspace::Action::SaveTemplate(path)) => {
                    let name = library::name_of(&path).to_owned();
                    self.dialog =
                        Some(Dialog::SaveTemplate(dialogs::SaveTemplate::new(path, name)));
                }
                None => {}
            }
            return;
        }

        let shell = self.backend.shell.clone();
        let thumbnail = move |root: &str| shell.load_thumbnail(root.into()).ok();
        let mut c = library::Ctx {
            recents: &self.recents,
            groups: &mut self.groups,
            settings: &mut self.settings,
            error: self.open_error.as_deref(),
            no_main_in: self.no_main_in.as_deref(),
            thumbnail: &thumbnail,
            now_ms: now_ms(),
        };
        if let Some(a) = library::show(ui, &p, &mut self.library, &mut c) {
            self.on_library(&ctx, a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn dirs(root: &std::path::Path) -> Dirs {
        Dirs {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    fn app<'a>(root: &std::path::Path) -> Harness<'a, App> {
        let d = dirs(root);
        Harness::builder()
            .with_size(crate::testing::WINDOW)
            .build_eframe(move |cc| {
                cc.egui_ctx.add_font_provider(std::sync::Arc::new(
                    egui_system_font_provider::SystemFontProvider::new(),
                ));
                App::with_dirs(cc, d)
            })
    }

    /// Steps until `found` holds; the backend answers from other threads.
    fn wait_for(h: &mut Harness<App>, what: &str, found: impl Fn(&Harness<App>) -> bool) {
        let start = std::time::Instant::now();
        while !found(h) {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(30),
                "timed out waiting for {what}"
            );
            std::thread::sleep(std::time::Duration::from_millis(30));
            h.step();
        }
    }

    #[test]
    fn first_run_goes_from_welcome_or_onboarding_to_the_library() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = app(tmp.path());
        wait_for(&mut h, "the first screen", |h| {
            h.query_by_label("Skip").is_some() || h.query_by_label("Check again").is_some()
        });
        if h.query_by_label("Check again").is_some() {
            // No TeX on this machine: onboarding, and "Continue without compiling" gets past it.
            h.get_by_label("Continue without compiling").click();
            wait_for(&mut h, "the library", |h| {
                h.query_by_label("New project").is_some()
            });
            return;
        }
        h.get_by_label("Skip").click();
        wait_for(&mut h, "the empty library", |h| {
            h.query_by_label_contains("No projects yet").is_some()
        });
        assert!(Settings::load(&dirs(tmp.path())).first_run_done);
    }

    /// A returning user with one project in `tmp`, back in the library; new projects default to `tmp`.
    fn in_library(tmp: &std::path::Path) -> Option<Harness<'static, App>> {
        let project = tmp.join("paper");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n",
        )
        .unwrap();
        let d = dirs(tmp);
        let mut recents = Recents::default();
        recents.remember(&project.to_string_lossy(), 1.0);
        recents.save(&d);
        Settings {
            first_run_done: true,
            last_seen_version: env!("CARGO_PKG_VERSION").into(),
            ..Settings::default()
        }
        .save(&d);
        let mut h = app(tmp);
        wait_for(&mut h, "the project or onboarding", |h| {
            h.query_by_label("Back to all projects").is_some()
                || h.query_by_label("Check again").is_some()
        });
        // Without TeX the library stays behind the install guide.
        h.query_by_label("Back to all projects")?;
        h.get_by_label("Back to all projects").click();
        wait_for(&mut h, "the library", |h| {
            h.query_by_label("Your projects").is_some()
        });
        Some(h)
    }

    #[test]
    fn a_new_project_from_the_gallery_is_created_and_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(mut h) = in_library(tmp.path()) else {
            return;
        };
        h.get_all_by_label("New project").next().unwrap().click();
        // The collection's templates can reorder the categories; wait until they're in.
        wait_for(&mut h, "the categories", |h| {
            h.query_by_label("Papers").is_some()
                && h.query_by_label_contains("Getting more templates")
                    .is_none()
        });
        h.get_by_label("Papers").click();
        wait_for(&mut h, "the Article template", |h| {
            h.query_by_label("Article").is_some()
        });
        h.get_by_label("Article").click();
        // The dialog re-centres a frame after its content changes; let it settle before clicking by position.
        h.run_steps(3);
        h.get_by_label("Folder name").click();
        h.run_steps(1);
        h.get_by_label("Folder name").type_text("thesis-draft");
        h.run_steps(2);
        h.get_by_label("Create project").click();
        wait_for(&mut h, "the new project", |h| {
            h.query_by_label("thesis-draft").is_some()
                && h.query_by_label("Back to all projects").is_some()
        });
        assert!(
            tmp.path().join("thesis-draft/main.tex").is_file(),
            "the template was copied next to the last project"
        );
        assert!(
            Recents::load(&dirs(tmp.path())).0[0]
                .path
                .ends_with("thesis-draft")
        );
    }

    #[test]
    fn a_repository_is_cloned_and_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("remote/origin");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repo.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n",
        )
        .unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !(git(&["init", "-q"])
            && git(&["add", "."])
            && git(&[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "x",
            ]))
        {
            return; // No git here.
        }
        let Some(mut h) = in_library(tmp.path()) else {
            return;
        };
        h.get_by_label("Clone a repository…").click();
        wait_for(&mut h, "the clone dialog", |h| {
            h.query_by_label("Repository URL").is_some()
        });
        h.get_by_label("Repository URL")
            .type_text(&repo.to_string_lossy());
        h.run_steps(2);
        h.get_by_label("Clone").click();
        wait_for(&mut h, "the cloned project", |h| {
            h.query_by_label("Back to all projects").is_some()
        });
        assert!(tmp.path().join("origin/main.tex").is_file());
        // The folder name followed the URL; the clone sits next to the last project.
        assert!(
            Recents::load(&dirs(tmp.path())).0[0]
                .path
                .ends_with("origin")
        );
    }

    #[test]
    fn an_overleaf_zip_is_imported_and_opened() {
        let tmp = tempfile::tempdir().unwrap();
        // An Overleaf "Download → Source" zip: the project's files at the top level.
        let zip_path = tmp.path().join("Seminar paper.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            z.start_file("main.tex", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(
                &mut z,
                b"\\documentclass{article}\n\\begin{document}\nz\n\\end{document}\n",
            )
            .unwrap();
            z.finish().unwrap();
        }
        let Some(mut h) = in_library(tmp.path()) else {
            return;
        };
        // What the file picker hands back after "Import from Overleaf…".
        let ctx = h.ctx.clone();
        h.state_mut()
            .on_msg(&ctx, Msg::Picked(Pick::OverleafZip, Some(zip_path)));
        wait_for(&mut h, "the zip's contents", |h| {
            h.query_by_label("Folder name").is_some()
        });
        h.run_steps(3);
        h.get_by_label("Import").click();
        wait_for(&mut h, "the imported project", |h| {
            h.query_by_label("Back to all projects").is_some()
        });
        let opened = &Recents::load(&dirs(tmp.path())).0[0].path;
        assert!(
            std::path::Path::new(opened).join("main.tex").is_file(),
            "{opened}"
        );
    }

    #[test]
    fn a_project_saved_as_template_is_offered_for_new_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(mut h) = in_library(tmp.path()) else {
            return;
        };
        h.get_by_label("Actions for paper").click();
        wait_for(&mut h, "the project menu", |h| {
            h.query_by_label("Save as template…").is_some()
        });
        h.get_by_label("Save as template…").click();
        wait_for(&mut h, "the dialog", |h| {
            h.query_by_label("Save template").is_some()
        });
        h.run_steps(3);
        h.get_by_label("Save template").click();
        wait_for(&mut h, "the saved note", |h| {
            h.query_by_label_contains("Template “paper” saved")
                .is_some()
        });
        h.get_all_by_label("New project").next().unwrap().click();
        wait_for(&mut h, "My templates", |h| {
            h.query_by_label("My templates").is_some()
        });
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let t = to.join(e.file_name());
            if e.path().is_dir() {
                copy_dir(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), t).unwrap();
            }
        }
    }

    /// The article fixture as the last project, opened by the app; None without TeX.
    fn in_article(tmp: &std::path::Path) -> Option<(Harness<'static, App>, std::path::PathBuf)> {
        in_article_with(tmp, Settings::default())
    }

    fn in_article_with(
        tmp: &std::path::Path,
        settings: Settings,
    ) -> Option<(Harness<'static, App>, std::path::PathBuf)> {
        let project = tmp.join("rl-thesis");
        copy_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article"),
            &project,
        );
        let d = dirs(tmp);
        let mut recents = Recents::default();
        recents.remember(&project.to_string_lossy(), 1.0);
        recents.save(&d);
        Settings {
            first_run_done: true,
            last_seen_version: env!("CARGO_PKG_VERSION").into(),
            ..settings
        }
        .save(&d);
        let mut h = app(tmp);
        let opened = |h: &Harness<App>| h.query_all_by_label("main.tex").next().is_some();
        wait_for(&mut h, "the project or onboarding", |h| {
            opened(h) || h.query_by_label("Check again").is_some()
        });
        if !opened(&h) {
            return None;
        }
        wait_for(&mut h, "the main file", |h| {
            h.query_by_label("Text of main.tex").is_some()
        });
        wait_for(&mut h, "the outline", |h| {
            h.query_by_label_contains("words").is_some()
        });
        h.run_steps(3);
        Some((h, project))
    }

    #[test]
    fn the_editor_screen_shows_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, _)) = in_article(tmp.path()) else {
            return;
        };
        // Opening builds; the snapshot shows the PDF it made.
        wait_for(&mut h, "the build", |h| {
            h.query_by_label_contains("Compiled in").is_some()
        });
        wait_for(&mut h, "the PDF", |h| {
            h.state().project.as_ref().unwrap().pane.view.rendered()
        });
        h.run_steps(3);
        let _ = h.try_snapshot("editor-split-light");
        assert!(h.query_by_label("Recompile").is_some());
        assert!(
            h.query_by_label("method.tex").is_some(),
            "the tree lists the chapters"
        );
    }

    #[test]
    fn opening_builds_and_the_pdf_shows_with_its_logs_and_synctex() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        wait_for(&mut h, "the build", |h| {
            h.query_by_label_contains("Compiled in").is_some()
                || h.query_by_label_contains("error").is_some()
        });
        wait_for(&mut h, "the PDF", |h| h.query_by_label("Page").is_some());
        let _ = project;
        let ws = h.state().project.as_ref().unwrap();
        assert_eq!(ws.pane.view.page_count(), 1);
        assert!(ws.pane.view.outline.is_empty(), "no hyperref, no bookmarks");

        // The logs replace the PDF, and come back.
        h.get_by_label("Logs").click();
        h.run_steps(2);
        assert!(h.query_by_label("Problems").is_some());
        h.get_by_label("Raw log").click();
        wait_for(&mut h, "the raw log", |h| {
            h.query_by_label_contains("This is").is_some()
        });
        h.get_by_label("Logs").click();
        h.run_steps(2);
        assert!(h.query_by_label("Problems").is_none());

        // Ctrl+J on \maketitle shows that line in the PDF; a click on that box goes back.
        {
            let app = h.state_mut();
            let ws = app.project.as_mut().unwrap();
            ws.goto(&app.backend, "main.tex", 10);
        }
        h.run_steps(2);
        {
            let app = h.state_mut();
            app.project.as_mut().unwrap().sync_forward(&app.backend);
        }
        wait_for(&mut h, "the SyncTeX box", |h| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .pane
                .view
                .highlighted()
                .is_some()
        });

        // Searching the PDF finds the title.
        h.state_mut()
            .project
            .as_mut()
            .unwrap()
            .pane
            .view
            .search("expected return", true);
        wait_for(&mut h, "the search", |h| {
            h.state().project.as_ref().unwrap().pane.view.match_count() > 0
        });
        h.run_steps(2);
        let _ = h.try_snapshot("editor-split-built-light");
    }

    #[test]
    fn a_broken_document_lists_its_problems_with_the_source_around_them() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        wait_for(&mut h, "the first build", |h| {
            h.query_by_label_contains("Compiled in").is_some()
        });
        // The library thumbnail is written after the first build.
        wait_for(&mut h, "the thumbnail", |h| {
            h.state()
                .backend
                .shell
                .load_thumbnail(project.to_string_lossy().into())
                .is_ok()
        });
        let main = project.join("main.tex");
        let text = std::fs::read_to_string(&main).unwrap();
        std::fs::write(
            &main,
            text.replace("\\maketitle", "\\maketitle\n\\notacommand"),
        )
        .unwrap();
        wait_for(&mut h, "the outside change", |h| {
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .doc
                .text()
                .contains("notacommand")
        });
        h.get_by_label("Recompile").click();
        wait_for(&mut h, "the failed build", |h| {
            let b = &h.state().project.as_ref().unwrap().build;
            !b.building && b.errors() > 0
        });
        h.run_steps(2);
        {
            let ed = &h.state().project.as_ref().unwrap().editors["main.tex"];
            assert!(
                ed.problems.iter().any(|p| ed.doc.line_of(p.from) == 10),
                "the editor marks line 11"
            );
        }
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        // F8 steps from problem to problem (there is a warning on line 5 too).
        let line = |h: &Harness<App>| {
            let ed = &h.state().project.as_ref().unwrap().editors["main.tex"];
            ed.doc.line_of(ed.doc.selection.main().head)
        };
        let mut seen = Vec::new();
        for _ in 0..4 {
            h.key_press(egui::Key::F8);
            h.run_steps(2);
            seen.push(line(&h));
        }
        assert!(
            seen.contains(&10),
            "F8 reaches the error on line 11: {seen:?}"
        );
        let _ = h.try_snapshot("editor-problem-light");
        h.get_by_label("Logs").click();
        h.run_steps(3);
        assert!(
            h.query_by_label("main.tex:11").is_some(),
            "the card links to the line"
        );
        let _ = h.try_snapshot("logs-light");
        h.get_by_label("main.tex:11").click();
        h.run_steps(2);
        assert_eq!(h.state().project.as_ref().unwrap().cursor.1, 11);
    }

    #[test]
    fn images_and_pdfs_of_the_project_open_in_a_viewer_tab() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        // A 40×30 PNG, half transparent.
        let img = image::RgbaImage::from_fn(40, 30, |x, _| {
            image::Rgba([200, 60, 60, if x < 20 { 255 } else { 0 }])
        });
        img.save(project.join("plot.png")).unwrap();
        {
            let app = h.state_mut();
            app.project
                .as_mut()
                .unwrap()
                .open_file(&app.backend, "plot.png");
        }
        wait_for(&mut h, "the image", |h| {
            matches!(
                h.state().project.as_ref().unwrap().viewers.get("plot.png"),
                Some(super::workspace::viewer::Viewer::Image { .. })
            )
        });
        h.run_steps(2);
        let _ = h.try_snapshot("viewer-image-light");
        // The build's own PDF, opened as a file of the project.
        wait_for(&mut h, "the build", |h| {
            h.query_by_label_contains("Compiled in").is_some()
        });
        let pdf = h.state().backend.shell.read_pdf().unwrap();
        std::fs::write(project.join("copy.pdf"), pdf).unwrap();
        {
            let app = h.state_mut();
            app.project
                .as_mut()
                .unwrap()
                .open_file(&app.backend, "copy.pdf");
        }
        wait_for(
            &mut h,
            "the PDF viewer",
            |h| matches!(h.state().project.as_ref().unwrap().viewers.get("copy.pdf"), Some(super::workspace::viewer::Viewer::Pdf(v)) if v.page_count() == 1),
        );
    }

    #[test]
    fn dark_pages_in_a_narrow_pane_move_the_buttons_into_more() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            theme: "dark".into(),
            pdf_dark: true,
            pdf_width: 500.0,
            ..Settings::default()
        };
        let Some((mut h, _)) = in_article_with(tmp.path(), settings) else {
            return;
        };
        wait_for(&mut h, "the PDF", |h| {
            h.state()
                .project
                .as_ref()
                .is_some_and(|w| w.pane.view.rendered())
        });
        assert!(h.query_by_label("Zoom in").is_none() && h.query_by_label("More").is_some());
        h.get_by_label("More").click();
        h.run_steps(3);
        assert!(h.query_by_label("Page thumbnails").is_some());
        h.get_by_label("Page thumbnails").click();
        h.run_steps(3);
        wait_for(&mut h, "the thumbnail", |h| {
            h.state().project.as_ref().unwrap().pane.view.page_count() == 1
        });
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            h.step();
        }
        let _ = h.try_snapshot("pdf-narrow-dark");
    }

    #[test]
    fn the_symbol_palette_inserts_what_is_clicked() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, _)) = in_article(tmp.path()) else {
            return;
        };
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        h.get_by_label("Symbols").click();
        h.run_steps(4);
        let _ = h.try_snapshot("symbols-light");
        h.get_by_label("\\alpha").click();
        h.run_steps(3);
        assert!(
            h.state().project.as_ref().unwrap().symbols.is_none(),
            "a click closes it"
        );
        let text = h.state().project.as_ref().unwrap().editors["main.tex"]
            .doc
            .text();
        assert!(text.contains("\\alpha"), "{text}");
    }

    #[test]
    fn texlab_adds_commands_and_goes_to_definitions() {
        if lemmata_shell::lsp::find_texlab(std::path::Path::new("/nonexistent")).is_none() {
            return; // no texlab on this machine, and tests don't download it
        }
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, _)) = in_article(tmp.path()) else {
            return;
        };
        wait_for(&mut h, "texlab", |h| {
            h.state().project.as_ref().unwrap().lsp.state == crate::lsp::State::Ready
        });
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        // A command only texlab knows.
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let end = ed.doc.line_end(9);
            ed.doc.selection = crate::editor::buffer::Selection::cursor(end);
        }
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        h.get_by_label("Text of main.tex").type_text("\\sectionma");
        wait_for(&mut h, "texlab's completions", |h| {
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .completion
                .as_ref()
                .is_some_and(|c| {
                    c.items
                        .iter()
                        .any(|i| i.0.display.as_deref() == Some("\\sectionmark"))
                })
        });
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        // F12 on \input{chapters/method} opens that file.
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let n = (0..ed.doc.line_count())
                .find(|&n| ed.doc.line(n).contains("\\input"))
                .unwrap();
            let at = ed.doc.line_start(n) + 10;
            ed.doc.selection = crate::editor::buffer::Selection::cursor(at);
        }
        h.run_steps(2);
        h.key_press(egui::Key::F12);
        wait_for(&mut h, "the definition", |h| {
            h.state().project.as_ref().unwrap().active.as_deref() == Some("chapters/method.tex")
        });
    }

    #[test]
    fn misspelled_words_are_marked_and_can_be_added_to_the_dictionary() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        wait_for(&mut h, "the dictionary", |h| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .checks
                .spell
                .lang
                .is_some()
        });
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let at = ed.doc.line_end(9);
            ed.doc.selection = crate::editor::buffer::Selection::cursor(at);
        }
        h.key_press(egui::Key::Enter);
        h.get_by_label("Text of main.tex")
            .type_text("Teh quick brwon fox.");
        h.run_steps(3);
        assert!(
            h.state_mut()
                .project
                .as_mut()
                .unwrap()
                .checks
                .spell
                .bad("brwon")
        );
        assert!(
            !h.state_mut()
                .project
                .as_mut()
                .unwrap()
                .checks
                .spell
                .bad("quick")
        );
        let _ = h.try_snapshot("spell-light");
        h.state_mut()
            .project
            .as_mut()
            .unwrap()
            .checks
            .spell
            .add("brwon");
        assert!(
            !h.state_mut()
                .project
                .as_mut()
                .unwrap()
                .checks
                .spell
                .bad("brwon")
        );
        // Adding through the workspace keeps the word in lemmata.toml.
        {
            let app = h.state_mut();
            let ws = app.project.as_mut().unwrap();
            ws.update_config(&app.backend, |c| c.words.push("brwon".into()));
        }
        wait_for(&mut h, "lemmata.toml", |_| {
            std::fs::read_to_string(project.join("lemmata.toml")).is_ok_and(|t| t.contains("brwon"))
        });
    }

    #[test]
    fn the_review_todo_git_and_history_panels_work() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        // A comment on the selected title.
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let n = (0..ed.doc.line_count())
                .find(|&n| ed.doc.line(n).starts_with("\\title"))
                .unwrap();
            let a = ed.doc.line_start(n) + 7;
            ed.doc.selection = crate::editor::buffer::Selection::single(
                crate::editor::buffer::Range::new(a, a + 7),
            );
        }
        h.run_steps(1);
        h.get_by_label("Comment").click();
        h.run_steps(3);
        h.get_by_label("Write a comment")
            .type_text("Is this the final title?");
        h.run_steps(2);
        // The panel's button, not the editor toolbar's.
        h.query_all_by_label("Comment").last().unwrap().click();
        wait_for(&mut h, "comments.json", |_| {
            std::fs::read_to_string(project.join(".lemmata/comments.json"))
                .is_ok_and(|t| t.contains("final title"))
        });
        h.run_steps(3);
        assert!(
            !h.state().project.as_ref().unwrap().editors["main.tex"]
                .comments
                .is_empty(),
            "the text is marked"
        );
        let _ = h.try_snapshot("review-light");

        // TODOs.
        std::fs::write(
            project.join("notes.tex"),
            "% TODO check the proof\n\\todo{cite more}\n",
        )
        .unwrap();
        h.get_by_label("Close comments").click();
        h.run_steps(2);
        {
            let app = h.state_mut();
            let mut s = app.settings.clone();
            app.project.as_mut().unwrap().open_panel(
                crate::workspace::panels::Panel::Todos,
                &app.backend,
                &mut s,
            );
        }
        wait_for(&mut h, "the TODOs", |h| {
            h.query_by_label("check the proof").is_some()
        });
        let _ = h.try_snapshot("todos-light");

        // Git: not a repository yet, then one.
        h.get_by_label("Close TODOs").click();
        h.run_steps(2);
        h.get_by_label("Git").click();
        wait_for(&mut h, "the git overview", |h| {
            h.query_by_label("Turn this project into a git repository")
                .is_some()
        });
        h.get_by_label("Turn this project into a git repository")
            .click();
        wait_for(&mut h, "the repository", |h| {
            h.query_by_label("Commit message").is_some()
        });
        let _ = h.try_snapshot("git-light");

        // History: the session's first save took a snapshot.
        h.get_by_label("Close Git").click();
        h.run_steps(2);
        h.get_by_label("History").click();
        wait_for(&mut h, "a snapshot", |h| {
            !h.state()
                .project
                .as_ref()
                .unwrap()
                .history
                .snapshots
                .is_empty()
        });
        h.run_steps(2);
        let _ = h.try_snapshot("history-light");
    }

    #[test]
    fn citations_the_bibliography_and_project_search_work() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let n = (0..ed.doc.line_count())
                .find(|&n| ed.doc.line(n).starts_with("\\maketitle"))
                .unwrap();
            ed.doc.selection = crate::editor::buffer::Selection::cursor(ed.doc.line_end(n));
        }
        // The edit bar's Cite opens the picker; Enter inserts the one match.
        h.get_by_label("Cite").click();
        wait_for(&mut h, "the entries", |h| {
            h.query_by_label("schulman2017ppo").is_some()
        });
        h.get_by_label("Search the bibliography").type_text("ppo");
        h.run_steps(2);
        let _ = h.try_snapshot("cite-light");
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert!(
            h.state().project.as_ref().unwrap().cite.is_none(),
            "the picker closed"
        );
        let text = h.state().project.as_ref().unwrap().editors["main.tex"]
            .doc
            .text();
        assert!(
            text.contains("\\maketitle\\cite{schulman2017ppo}"),
            "{text}"
        );

        // Two entries share a key: the manager renames the second.
        let bib = project.join("refs.bib");
        let mut b = std::fs::read_to_string(&bib).unwrap();
        b.push_str("\n@misc{schulman2017ppo,\n  title = {A copy},\n}\n");
        std::fs::write(&bib, &b).unwrap();
        {
            let app = h.state_mut();
            let s = app.settings.clone();
            app.project
                .as_mut()
                .unwrap()
                .open_bib_manager(&app.backend, &s);
        }
        wait_for(&mut h, "the duplicate", |h| {
            h.query_all_by_label("Rename key…").count() == 2
        });
        // The dialog grew: let it settle in the middle.
        h.run_steps(3);
        let _ = h.try_snapshot("bib-manager-light");
        h.query_all_by_label("Rename key…").last().unwrap().click();
        h.run_steps(4);
        h.get_by_label("Rename").click();
        wait_for(&mut h, "the new key", |_| {
            std::fs::read_to_string(&bib)
                .unwrap()
                .contains("schulman2017ppoa")
        });
        h.key_press(egui::Key::Escape);
        h.run_steps(2);

        // Search the project, then replace on the checked lines.
        h.state_mut()
            .project
            .as_mut()
            .unwrap()
            .open_search(Some("Equation~".into()));
        wait_for(&mut h, "a hit", |h| {
            h.query_by_label_contains("chapters/method.tex:6").is_some()
        });
        h.get_by_label("Show replace").click();
        h.run_steps(2);
        h.get_by_label("Replace with").click();
        h.run_steps(1);
        h.get_by_label("Replace with").type_text("Eq.~");
        wait_for(&mut h, "the preview", |h| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .search
                .as_ref()
                .is_some_and(|s| s.previewed("as in Eq.~"))
        });
        let _ = h.try_snapshot("search-light");
        h.get_by_label("Replace all").click();
        wait_for(&mut h, "the replacement", |_| {
            std::fs::read_to_string(project.join("chapters/method.tex"))
                .unwrap()
                .contains("as in Eq.~\\ref{eq:objective}")
        });
    }

    #[test]
    fn figures_tables_and_pasted_cells_become_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        std::fs::create_dir_all(project.join("figures")).unwrap();
        image::RgbaImage::new(4, 4)
            .save(project.join("figures/plot.png"))
            .unwrap();
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        let to_line = |h: &mut Harness<App>, start: &str| {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("main.tex")
                .unwrap();
            let n = (0..ed.doc.line_count())
                .find(|&n| ed.doc.line(n).starts_with(start))
                .unwrap();
            ed.doc.selection = crate::editor::buffer::Selection::cursor(ed.doc.line_end(n));
        };
        let text = |h: &Harness<App>| {
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .doc
                .text()
        };

        // A table of 2 columns by 2 rows; its cells are tab stops.
        to_line(&mut h, "\\maketitle");
        h.get_by_label("Table").click();
        h.run_steps(3);
        h.get_by_label("2 columns by 2 rows").click();
        h.run_steps(2);
        let _ = h.try_snapshot("table-light");
        h.get_by_label("Insert").click();
        h.run_steps(2);
        let t = text(&h);
        assert!(
            t.contains(
                "\\maketitle\n\\begin{table}[htbp]\n    \\centering\n    \\begin{tabular}{ll}"
            ),
            "{t}"
        );
        assert!(t.contains("        Header 1 & Header 2 \\\\\n        \\midrule\n        Cell 1.1 & Cell 1.2 \\\\"), "{t}");
        assert_eq!(
            h.state().project.as_ref().unwrap().editors["main.tex"].selected_text(),
            "Header 1"
        );

        // A figure from the project's images, with a caption.
        to_line(&mut h, "\\input");
        wait_for(&mut h, "the image in the tree", |h| {
            h.query_by_label("plot.png").is_some()
        });
        h.get_by_label("Figure").click();
        h.run_steps(3);
        h.get_by_label("Caption").click();
        h.run_steps(1);
        h.get_by_label("Caption").type_text("A plot");
        h.run_steps(2);
        let _ = h.try_snapshot("figure-light");
        h.get_by_label("Insert").click();
        h.run_steps(2);
        let t = text(&h);
        assert!(t.contains("\\includegraphics[width=0.8\\linewidth]{figures/plot.png}\n    \\caption{A plot}\n    \\label{fig:a-plot}\n\\end{figure}"), "{t}");

        // Cells from a spreadsheet: a table, and the toast turns it back into the text.
        to_line(&mut h, "\\printbibliography");
        h.event(egui::Event::Paste("a\tb\n1\t2".into()));
        h.run_steps(2);
        assert!(text(&h).contains("\\begin{tabular}{rr}"));
        h.get_by_label("Paste as text instead").click();
        h.run_steps(2);
        let t = text(&h);
        assert!(
            t.contains("\\printbibliographya\tb\n1\t2") && !t.contains("{rr}"),
            "{t}"
        );

        // A pasted screenshot is saved in the project and becomes a figure.
        h.event(egui::Event::PasteImage(std::sync::Arc::new(
            egui::ColorImage::filled([3, 3], egui::Color32::RED),
        )));
        wait_for(&mut h, "the pasted figure", |h| {
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .doc
                .text()
                .contains("\\caption{}\n")
        });
    }

    #[test]
    fn f2_renames_a_label_everywhere() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        h.get_by_label("method.tex").click();
        wait_for(&mut h, "method.tex", |h| {
            h.query_by_label("Text of method.tex").is_some()
        });
        h.get_by_label("Text of method.tex").click();
        h.run_steps(2);
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("chapters/method.tex")
                .unwrap();
            let at = ed.doc.text().find("ref{eq:objective}").unwrap() + 6;
            ed.doc.selection = crate::editor::buffer::Selection::cursor(at);
        }
        h.run_steps(1);
        h.key_press(egui::Key::F2);
        wait_for(&mut h, "the uses", |h| {
            h.query_by_label("2 places in 1 file:").is_some()
        });
        let _ = h.try_snapshot("rename-light");
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_label("New name").type_text("eq:loss");
        h.run_steps(2);
        h.get_by_label("Rename").click();
        wait_for(&mut h, "the rename", |_| {
            let t = std::fs::read_to_string(project.join("chapters/method.tex")).unwrap();
            t.contains("\\label{eq:loss}") && t.contains("\\ref{eq:loss}")
        });
    }

    #[test]
    fn the_tree_creates_renames_deletes_and_undoes() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        let menu_on = |h: &mut Harness<App>, row: Option<&str>| {
            let app = h.state_mut();
            let ws = app.project.as_mut().unwrap();
            ws.menu = Some(crate::workspace::Menu::Tree(
                row.map(str::to_owned),
                egui::Rect::from_min_size(egui::pos2(120.0, 300.0), egui::Vec2::ZERO),
            ));
        };
        // New file, in the project folder; .tex is added.
        menu_on(&mut h, None);
        h.run_steps(2);
        // The menu's item, not the header's button.
        h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "New file")
            .click();
        wait_for(&mut h, "the dialog", |h| h.query_by_label("Name").is_some());
        h.get_by_label("Name").type_text("intro");
        h.run_steps(1);
        h.get_by_label("Create").click();
        wait_for(&mut h, "the new file", |h| {
            h.query_by_label("Text of intro.tex").is_some()
        });
        assert!(project.join("intro.tex").exists());

        // Rename: the name without its extension is selected, so typing replaces it.
        menu_on(&mut h, Some("intro.tex"));
        h.run_steps(2);
        h.get_by_label("Rename…").click();
        wait_for(&mut h, "the rename dialog", |h| {
            h.query_by_label("New name").is_some()
        });
        h.run_steps(1);
        h.get_by_label("New name").type_text("introduction");
        h.run_steps(1);
        let _ = h.try_snapshot("file-rename-light");
        h.get_by_label("Rename").click();
        wait_for(&mut h, "the renamed file", |_| {
            project.join("introduction.tex").exists()
        });
        wait_for(&mut h, "the tab", |h| {
            h.query_by_label("Text of introduction.tex").is_some()
        });

        // Duplicate, then delete the copy and undo that.
        menu_on(&mut h, Some("introduction.tex"));
        h.run_steps(2);
        h.get_by_label("Duplicate").click();
        wait_for(&mut h, "the copy", |_| {
            project.join("introduction copy.tex").exists()
        });
        menu_on(&mut h, Some("introduction copy.tex"));
        h.run_steps(2);
        h.get_by_label("Delete").click();
        wait_for(&mut h, "the deletion", |_| {
            !project.join("introduction copy.tex").exists()
        });
        wait_for(&mut h, "the undo button", |h| {
            h.query_by_label("Undo").is_some()
        });
        h.get_by_label("Undo").click();
        wait_for(&mut h, "the restored file", |_| {
            project.join("introduction copy.tex").exists()
        });

        // The keyboard: select a row, F2 opens Rename.
        wait_for(&mut h, "the restored row", |h| {
            h.query_by_label("introduction copy.tex").is_some()
        });
        h.run_steps(2);
        h.get_by_label("refs.bib").click();
        h.run_steps(2);
        h.key_press(egui::Key::F2);
        wait_for(&mut h, "F2's rename dialog", |h| {
            h.query_by_label("Rename refs.bib").is_some()
        });
    }

    #[test]
    fn preferences_snippets_and_project_settings_save() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        // Preferences from the menu: a checkbox saves at once.
        h.get_by_label("Menu").click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Preferences")
            .click();
        wait_for(&mut h, "the preferences", |h| {
            h.query_by_label("Wrap long lines").is_some()
        });
        h.run_steps(3);
        let folding = h.state().settings.folding;
        h.get_by_label("Code folding for sections and environments")
            .click();
        h.run_steps(2);
        assert_eq!(h.state().settings.folding, !folding);
        let _ = h.try_snapshot("preferences-light");

        // A snippet: incomplete ones are left out on Save.
        h.get_by_label("Add…").click();
        wait_for(&mut h, "the snippets", |h| {
            h.query_by_label("New snippet").is_some()
        });
        h.run_steps(3);
        h.get_by_label("New snippet").click();
        h.run_steps(3);
        h.get_by_label("Trigger").click();
        h.get_by_label("Trigger").type_text("thm");
        h.get_by_label("Text to insert").click();
        h.get_by_label("Text to insert")
            .type_text("\\begin{theorem}${}\\end{theorem}");
        h.run_steps(2);
        h.get_by_label("New snippet").click();
        h.run_steps(3);
        let _ = h.try_snapshot("snippets-light");
        h.get_by_label("Save").click();
        wait_for(&mut h, "the preferences again", |h| {
            h.query_by_label("Edit 1…").is_some()
        });
        h.run_steps(3);
        assert_eq!(h.state().settings.snippets[0].trigger, "thm");
        h.get_by_label("Done").click();
        h.run_steps(2);
        assert!(h.state().prefs.is_none());

        // Project settings from the status bar: shell escape is allowed for this computer too.
        h.get_by_label("pdfLaTeX").click();
        wait_for(&mut h, "the project settings", |h| {
            h.query_by_label("Allow the document to run programs (shell escape)")
                .is_some()
        });
        h.run_steps(3);
        h.get_by_label("Allow the document to run programs (shell escape)")
            .click();
        h.run_steps(3);
        let _ = h.try_snapshot("project-settings-light");
        h.get_by_label("Save").click();
        wait_for(&mut h, "lemmata.toml", |_| {
            std::fs::read_to_string(project.join("lemmata.toml"))
                .is_ok_and(|t| t.contains("shell_escape = true"))
        });
        let root = h.state().project.as_ref().unwrap().root.clone();
        assert!(h.state().settings.shell_escape_projects.contains(&root));
    }

    #[test]
    fn the_palette_quick_open_sheets_and_presenting() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, _project)) = in_article(tmp.path()) else {
            return;
        };
        let palette_kind = |h: &Harness<App>| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .palette
                .as_ref()
                .map(|p| p.kind)
        };
        // Quick open: type, Enter opens the file.
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
        h.run_steps(3);
        assert_eq!(
            palette_kind(&h),
            Some(crate::workspace::palette::Kind::Files)
        );
        h.get_by_label("Open a file by name").type_text("meth");
        h.run_steps(2);
        let _ = h.try_snapshot("quick-open-light");
        h.key_press(egui::Key::Enter);
        wait_for(&mut h, "method.tex", |h| {
            h.query_by_label("Text of method.tex").is_some()
        });

        // The command palette runs a command by name: the shortcuts sheet.
        h.key_press_modifiers(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::P,
        );
        h.run_steps(3);
        h.get_by_label("Type a command").type_text("keyboard");
        h.run_steps(2);
        let _ = h.try_snapshot("palette-light");
        h.key_press(egui::Key::Enter);
        wait_for(&mut h, "the sheet", |h| {
            h.query_by_label("Search shortcuts").is_some()
        });
        h.run_steps(3);
        h.get_by_label("Search shortcuts").type_text("presenter");
        h.run_steps(2);
        assert!(
            h.query_by_label("Go to a section").is_none(),
            "the search filters"
        );
        h.key_press(egui::Key::Escape);
        h.run_steps(2);

        // Word count from the menu.
        h.get_by_label("Menu").click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Word count")
            .click();
        wait_for(&mut h, "the counts", |h| {
            h.query_by_label("Display maths").is_some()
        });
        h.run_steps(3);
        let _ = h.try_snapshot("word-count-light");
        h.get_by_label("Done").click();
        h.run_steps(2);

        // Wrap the first line in an environment from the editor's key.
        h.get_by_label("Text of method.tex").click();
        h.run_steps(1);
        {
            let app = h.state_mut();
            let ed = app
                .project
                .as_mut()
                .unwrap()
                .editors
                .get_mut("chapters/method.tex")
                .unwrap();
            ed.doc.selection = crate::editor::buffer::Selection::cursor(ed.doc.line_start(1));
        }
        h.key_press_modifiers(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::E,
        );
        h.run_steps(3);
        assert_eq!(
            palette_kind(&h),
            Some(crate::workspace::palette::Kind::Environments)
        );
        h.get_by_label("Wrap the selection in…").type_text("center");
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        let line = h.state().project.as_ref().unwrap().editors["chapters/method.tex"]
            .doc
            .line(1);
        assert_eq!(line, "\\begin{center}");

        // Present the built PDF; Esc ends it.
        wait_for(&mut h, "the PDF", |h| h.query_by_label("Page").is_some());
        let ctx = h.ctx.clone();
        h.state_mut().project.as_mut().unwrap().present(&ctx);
        wait_for(&mut h, "the slide", |h| {
            h.query_by_label("Slide 1 of 1").is_some()
        });
        wait_for(&mut h, "the slide's image", |h| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .presentation
                .as_ref()
                .is_some_and(|p| p.rendered())
        });
        h.run_steps(2);
        let _ = h.try_snapshot("present-light");
        // P: the presenter view, with the timer and the notes hint.
        h.key_press(egui::Key::P);
        wait_for(&mut h, "the presenter view", |h| {
            h.query_by_label("End presentation").is_some()
        });
        h.run_steps(3);
        // The presenter view as its own window shows it.
        let bytes = h
            .state()
            .project
            .as_ref()
            .unwrap()
            .pane
            .view
            .shown_bytes()
            .unwrap();
        let mut show: Option<crate::pdf::present::Presentation> = None;
        let mut ph = egui_kittest::Harness::builder()
            .with_size([1180.0, 740.0])
            .with_context_setup(|ctx| {
                ctx.add_font_provider(std::sync::Arc::new(
                    egui_system_font_provider::SystemFontProvider::new(),
                ));
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, &crate::theme::LIGHT);
            })
            .wgpu()
            .build_ui(move |ui| {
                let s = show.get_or_insert_with(|| {
                    crate::pdf::present::Presentation::new(ui.ctx(), bytes.clone(), 1)
                });
                s.receive(ui.ctx());
                s.presenter_ui(ui);
            });
        for _ in 0..40 {
            ph.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = ph.try_snapshot("presenter-light");
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().project.as_ref().unwrap().presentation.is_none());
    }

    #[test]
    fn whats_new_shows_once_after_an_update() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dirs(tmp.path());
        Settings {
            first_run_done: true,
            last_seen_version: "1.4.0".into(),
            ..Settings::default()
        }
        .save(&d);
        let mut h = app(tmp.path());
        wait_for(&mut h, "the news", |h| h.query_by_label("Got it").is_some());
        h.run_steps(3);
        assert!(
            h.query_by_label("LEMMATA 1.5").is_some(),
            "both releases, labelled"
        );
        let _ = h.try_snapshot("whats-new-light");
        h.get_by_label("Got it").click();
        h.run_steps(2);
        assert!(h.query_by_label("Got it").is_none());
        assert_eq!(
            Settings::load(&d).last_seen_version,
            env!("CARGO_PKG_VERSION")
        );
    }

    /// A file dropped from the computer, as egui-winit hands it over.
    #[derive(Debug)]
    struct Dropped(std::path::PathBuf);
    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }

    #[test]
    fn files_drag_in_the_tree_and_drop_from_the_computer() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        let press = |h: &mut Harness<App>, pos: egui::Pos2, pressed: bool| {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        };
        // Drag refs.bib onto the chapters folder.
        let from = h
            .get_by_role_and_label(egui::accesskit::Role::TreeItem, "refs.bib")
            .rect()
            .center();
        let to = h
            .get_by_role_and_label(egui::accesskit::Role::TreeItem, "chapters")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(from));
        press(&mut h, from, true);
        h.run_steps(1);
        for k in 1..=8 {
            h.event(egui::Event::PointerMoved(
                from + (to - from) * (k as f32 / 8.0),
            ));
            h.run_steps(1);
        }
        let _ = h.try_snapshot("tree-drag-light");
        assert_eq!(
            h.state()
                .project
                .as_ref()
                .unwrap()
                .tree_drag
                .as_ref()
                .and_then(|d| d.target.clone())
                .as_deref(),
            Some("chapters")
        );
        press(&mut h, to, false);
        h.run_steps(2);
        // main.tex loads refs.bib, so the move asks whether to update that line.
        wait_for(&mut h, "the move dialog", |h| {
            h.query_by_label("Move").is_some()
        });
        h.run_steps(3);
        h.get_by_label("Move").click();
        wait_for(&mut h, "the moved file", |_| {
            project.join("chapters/refs.bib").exists()
        });
        wait_for(&mut h, "the updated reference", |_| {
            std::fs::read_to_string(project.join("main.tex"))
                .unwrap()
                .contains("chapters/refs.bib")
        });

        // An image dropped below the rows goes into the project folder.
        let img = tmp.path().join("photo.png");
        image::RgbaImage::new(2, 2).save(&img).unwrap();
        let below = h
            .get_by_role_and_label(egui::accesskit::Role::TreeItem, "main.tex")
            .rect()
            .center()
            + egui::vec2(0.0, 120.0);
        h.event(egui::Event::PointerMoved(below));
        h.input_mut()
            .dropped_files
            .push(std::sync::Arc::new(Dropped(img.clone())));
        h.run_steps(2);
        wait_for(&mut h, "the added file", |_| {
            project.join("photo.png").exists()
        });

        // Dropped on the text of main.tex: a figure there.
        h.get_by_role_and_label(egui::accesskit::Role::TreeItem, "main.tex")
            .click();
        wait_for(&mut h, "main.tex", |h| {
            h.query_by_label("Text of main.tex").is_some()
        });
        h.run_steps(2);
        let text = h.get_by_label("Text of main.tex").rect();
        h.event(egui::Event::PointerMoved(
            text.left_top() + egui::vec2(120.0, 20.0),
        ));
        h.input_mut()
            .dropped_files
            .push(std::sync::Arc::new(Dropped(img)));
        h.run_steps(2);
        wait_for(&mut h, "the figure", |h| {
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .doc
                .text()
                .contains("\\includegraphics[width=0.8\\linewidth]{")
        });
    }

    #[test]
    fn a_second_launch_opens_its_file() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        // The magic comment names the project's main file.
        let path = project.join("chapters/method.tex");
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("% !TEX root = ../main.tex\n{text}")).unwrap();
        let file = path.to_string_lossy().into_owned();
        h.state()
            .backend
            .shell
            .on_second_launch(&["lemmata".into(), file], "/");
        wait_for(&mut h, "method.tex", |h| {
            h.state()
                .project
                .as_ref()
                .is_some_and(|w| w.active.as_deref() == Some("chapters/method.tex"))
        });
    }

    #[test]
    fn a_projects_latexmkrc_waits_for_consent() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("paper");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n",
        )
        .unwrap();
        std::fs::write(project.join(".latexmkrc"), "system('touch pwned');").unwrap();
        let d = dirs(tmp.path());
        let mut recents = Recents::default();
        recents.remember(&project.to_string_lossy(), 1.0);
        recents.save(&d);
        Settings {
            first_run_done: true,
            last_seen_version: env!("CARGO_PKG_VERSION").into(),
            ..Settings::default()
        }
        .save(&d);
        let mut h = app(tmp.path());
        wait_for(&mut h, "the project or onboarding", |h| {
            h.state().project.is_some() || h.query_by_label("Check again").is_some()
        });
        if h.state().project.is_none() {
            return;
        }
        let asks = |h: &Harness<App>| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .notices
                .iter()
                .any(|n| n.action == Some(crate::workspace::NoticeAction::TrustProject))
        };
        assert!(asks(&h), "the notice offers to use it");
        wait_for(&mut h, "the notice", |h| {
            h.query_by_label("Use it").is_some()
        });
        assert!(h.state().settings.shell_escape_projects.is_empty());
        h.get_by_label("Use it").click();
        h.run_steps(3);
        let root = h.state().project.as_ref().unwrap().root.clone();
        assert_eq!(h.state().settings.shell_escape_projects, [root]);
    }

    #[test]
    fn a_reload_keeps_unsaved_text_and_asks() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            autosave_ms: 0.0,
            ..Settings::default()
        };
        let Some((mut h, project)) = in_article_with(tmp.path(), settings) else {
            return;
        };
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        h.get_by_label("Text of main.tex").type_text("% mine");
        h.run_steps(2);
        // Another job (a replace, a rename) changed the file on disk and reads it again.
        std::fs::write(
            project.join("main.tex"),
            "\\documentclass{article}\n% theirs\n",
        )
        .unwrap();
        {
            let app = h.state_mut();
            let (ws, backend) = (app.project.as_mut().unwrap(), &app.backend);
            ws.reload(backend, "main.tex");
        }
        wait_for(&mut h, "the conflict", |h| {
            h.state()
                .project
                .as_ref()
                .unwrap()
                .buffers
                .conflicts
                .iter()
                .any(|c| c == "main.tex")
        });
        assert!(
            h.state().project.as_ref().unwrap().editors["main.tex"]
                .doc
                .text()
                .contains("% mine")
        );
    }

    #[test]
    fn typing_saves_after_the_delay_and_the_file_changes_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        h.get_by_label("Text of main.tex").type_text("% typed here");
        h.run_steps(2);
        assert!(h.query_by_label("Unsaved").is_some());
        wait_for(&mut h, "the autosave", |h| {
            h.query_by_label("Saved").is_some()
        });
        let text = std::fs::read_to_string(project.join("main.tex")).unwrap();
        assert!(text.contains("% typed here"), "{text}");
    }

    #[test]
    fn an_outside_change_with_unsaved_edits_asks_which_version_to_keep() {
        let tmp = tempfile::tempdir().unwrap();
        let Some((mut h, project)) = in_article(tmp.path()) else {
            return;
        };
        // Autosave off, so the edit stays unsaved.
        h.state_mut().settings.autosave_ms = 0.0;
        h.get_by_label("Text of main.tex").click();
        h.run_steps(2);
        h.get_by_label("Text of main.tex").type_text("% mine");
        h.run_steps(2);
        std::fs::write(
            project.join("main.tex"),
            "\\documentclass{article}\n% theirs\n",
        )
        .unwrap();
        wait_for(&mut h, "the conflict dialog", |h| {
            h.query_by_label_contains("changed on disk").is_some()
        });
        h.get_by_label("Load the version on disk").click();
        wait_for(&mut h, "the disk's text", |h| {
            h.state()
                .project
                .as_ref()
                .is_some_and(|w| w.editors["main.tex"].doc.text().contains("% theirs"))
        });
    }

    #[test]
    fn a_returning_user_lands_in_the_last_project() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("paper");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n",
        )
        .unwrap();
        let d = dirs(tmp.path());
        let mut recents = Recents::default();
        recents.remember(&project.to_string_lossy(), 1.0);
        recents.save(&d);
        // This version was used before: no "What's new" in front of the project.
        Settings {
            last_seen_version: env!("CARGO_PKG_VERSION").into(),
            ..Settings::default()
        }
        .save(&d);
        let mut h = app(tmp.path());
        wait_for(&mut h, "the project or onboarding", |h| {
            h.query_by_label("Back to all projects").is_some()
                || h.query_by_label("Check again").is_some()
        });
        if h.query_by_label("Back to all projects").is_some() {
            // Back to the library, which lists it.
            h.get_by_label("Back to all projects").click();
            wait_for(&mut h, "the library", |h| {
                h.query_by_label("Your projects").is_some()
            });
            assert!(h.query_by_label("paper").is_some());
            assert!(Recents::load(&d).0[0].last_opened > 1.0);
        }
    }
}
