//! The app: which screen shows, and what the screens' actions do. This is what App.svelte did.

use crate::backend::{Backend, Msg, Pick};
use crate::screens::{library, onboarding, welcome};
use crate::store::{Dirs, Groups, Recents, Settings};
use crate::theme::{self, Palette};
use crate::widgets;
use eframe::egui::{self, FontId};
use lemmata_shell::open_with::Request;
use lemmata_shell::{Event, InstallRequest, ProjectInfo, TexStatus};

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
    project: Option<ProjectInfo>,
    /// Startup has decided between the welcome, the last project and the library.
    started: bool,
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
                self.open_project(target.project_root.to_string_lossy().into_owned())
            }
            // shortcut: the Clone and New project dialogs land with the rest of the library's dialogs.
            Request::Clone { .. } | Request::NewProject { .. } => {}
        }
    }

    fn on_msg(&mut self, ctx: &egui::Context, msg: Msg) {
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
                self.project = Some(info);
            }
            Msg::Opened(path, Err(e)) => {
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
            // shortcut: the import dialog lands with the rest of the library's dialogs.
            Msg::Picked(Pick::OverleafZip, _) | Msg::Picked(_, None) => {}
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
            Msg::Event(_) => {}
        }
    }

    /// Existing users (with projects) never see the welcome. A file or folder opened from the OS
    /// wins over the last project.
    fn start(&mut self) {
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
            // shortcut: New project, Clone, Save as template and Preferences are the next dialogs to port.
            A::NewProject | A::Clone | A::SaveTemplate(_) | A::Preferences => {}
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
                        welcome::Action::NewProject => {}
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

        if let Some(project) = &self.project {
            // shortcut: the editor arrives in M3; until then an open project shows its name and a way back.
            ui.painter().rect_filled(rect, 0, p.app);
            let name = project.name.clone();
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() / 2.0 - 40.0);
                ui.label(
                    egui::RichText::new(&name)
                        .font(FontId::new(26.0, theme::font::display()))
                        .color(p.ink),
                );
                if widgets::Button::new("Back to all projects")
                    .show(ui, &p)
                    .clicked()
                {
                    self.project = None;
                }
            });
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

    fn on_exit(&mut self) {
        // Quitting mid-build must not leave latexmk, pdflatex and biber running.
        self.backend.shell.stop_build();
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
