//! Building and the PDF (the runBuild part of App.svelte): builds after saves or on request,
//! stopping, clearing the cache, the logs, SyncTeX both ways, exports and the library thumbnail.

use super::{Notice, NoticeAction, Tone, Workspace};
use crate::backend::{Backend, Msg, Save};
use crate::pdf::pane::{self, Action as PaneAction};
use crate::store::Settings;
use lemmata_log::{Diagnostic, Kind, Severity};
use lemmata_tex::{Outcome, Status};
use std::time::{Duration, Instant};

/// Automatic builds wait this long after the last autosave, so a build isn't started and
/// cancelled again while typing.
const AUTO_COMPILE_DELAY: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct BuildState {
    pub building: bool,
    /// Bumped by every build and Stop; a result for an older one is dropped.
    pub seq: u64,
    pub outcome: Option<Outcome>,
    pub finished_at: Option<std::time::SystemTime>,
    pub diagnostics: Vec<Diagnostic>,
    /// Bumped whenever `diagnostics` is replaced, so editors show the new ones.
    pub diag_gen: u64,
    pub logs_open: bool,
    /// What the engine says it is doing (Tectonic fetching packages).
    pub note: Option<String>,
    /// A build waits until then; `force` with it.
    pub deadline: Option<(Instant, bool)>,
    told_no_tex: bool,
    last_thumbnail: Option<Instant>,
    /// Build once the project's settings come back from lemmata.toml.
    pub after_config: bool,
    /// An arXiv export waits for this build (and whether it flattens).
    arxiv: Option<(u64, bool)>,
    /// Closed files' text for the excerpts under errors, read once per build (`diag_gen`), not
    /// on every frame.
    sources: std::cell::RefCell<(u64, std::collections::HashMap<String, Option<String>>)>,
}

impl BuildState {
    pub fn errors(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }
    /// The last build ended without writing a PDF.
    pub fn failed(&self) -> bool {
        self.outcome
            .as_ref()
            .is_some_and(|o| o.status != Status::Success && o.pdf.is_none())
    }
}

impl Workspace {
    /// The last build's problems in `path`, for its editor: line, severity, message with hint.
    pub fn problems_of(&self, path: &str) -> Vec<(usize, crate::editor::view::Severity, String)> {
        use crate::editor::view::Severity as S;
        self.build
            .diagnostics
            .iter()
            .filter(|d| d.file.as_deref().map(|f| f.strip_prefix("./").unwrap_or(f)) == Some(path))
            .filter_map(|d| {
                let line = d.line? as usize;
                let sev = match d.severity {
                    Severity::Error => S::Error,
                    Severity::Warning => S::Warning,
                    Severity::Info => S::Info,
                };
                let msg = match &d.hint {
                    Some(h) => format!("{}\n{h}", d.message),
                    None => d.message.clone(),
                };
                Some((line, sev, msg))
            })
            .collect()
    }

    /// Builds once the saves under way are done: now, or after the autosave delay.
    pub fn build_soon(&mut self, auto: bool, force: bool) {
        let at = Instant::now()
            + if auto {
                AUTO_COMPILE_DELAY
            } else {
                Duration::ZERO
            };
        let force = force || self.build.deadline.is_some_and(|(_, f)| f);
        self.build.deadline = Some((at, force));
    }

    /// Recompile: everything saved, then built even if nothing changed.
    pub fn recompile(&mut self, backend: &Backend, settings: &Settings) {
        self.save_all_quietly(backend, settings);
        self.build_soon(false, true);
    }

    /// Saves without the build a save would start.
    pub fn save_all_quietly(&mut self, backend: &Backend, settings: &Settings) {
        for p in self.buffers.unsaved() {
            self.no_build.insert(p.clone());
            self.write(backend, &p, settings);
        }
    }

    pub fn run_build(&mut self, backend: &Backend, settings: &Settings, force: bool) {
        self.build.deadline = None;
        if !self.tex_ready {
            if !self.build.told_no_tex {
                self.build.told_no_tex = true;
                self.notices.push(Notice::new(
                    "TeX isn't installed yet, so nothing is compiled. Your edits are saved as usual.",
                    Tone::Info,
                ));
            }
            return;
        }
        self.build.seq += 1;
        self.build.building = true;
        self.build.note = None;
        let seq = self.build.seq;
        // A downloaded project's lemmata.toml can't run programs on its own: the user must allow it.
        let escape = settings.shell_escape_projects.contains(&self.root);
        backend.spawn_async(move |shell| async move {
            Msg::Built(seq, shell.build(escape, force).await)
        });
    }

    pub fn stop_build(&mut self, backend: &Backend) {
        self.build.seq += 1;
        self.build.building = false;
        self.build.deadline = None;
        backend.spawn(|shell| {
            shell.stop_build();
            Msg::Done(Ok(()))
        });
    }

    pub fn clean(&mut self, backend: &Backend) {
        backend.spawn(|shell| Msg::Cleaned(shell.clean_build()));
    }

    /// Builds that were waiting for their time, and the thumbnail when it's drawn.
    pub fn poll_build(
        &mut self,
        ctx: &eframe::egui::Context,
        backend: &Backend,
        settings: &Settings,
    ) {
        if let Some((at, force)) = self.build.deadline {
            let now = Instant::now();
            if now >= at && !self.writing() {
                self.run_build(backend, settings, force);
            } else {
                ctx.request_repaint_after(
                    at.saturating_duration_since(now)
                        .max(Duration::from_millis(30)),
                );
            }
        }
        if let Some(png) = self.pane.view.png.take() {
            let root = self.root.clone();
            backend.spawn(move |shell| Msg::Done(shell.save_thumbnail(&root, &png)));
        }
    }

    /// Show this line in the PDF (Ctrl+J, "Go to PDF").
    pub fn sync_forward(&mut self, backend: &Backend) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let Some(line) = self.editors.get(&path).map(|e| e.cursor_line()) else {
            return;
        };
        backend.spawn(move |shell| Msg::Synced(shell.synctex_forward(path, line as u32, 0)));
    }

    pub fn export(&mut self, backend: &Backend, settings: &Settings, what: Save) {
        let name = self.info.name.clone();
        match what {
            Save::Pdf => backend.save(what, format!("{name}.pdf")),
            Save::Zip => {
                self.save_all_quietly(backend, settings);
                backend.save(what, format!("{name}.zip"));
            }
            Save::Arxiv { flatten } => {
                // arXiv compiles the upload itself; a document that fails here fails there too.
                self.save_all_quietly(backend, settings);
                self.build_soon(false, false);
                self.build.arxiv = Some((self.build.seq + 1, flatten));
            }
        }
    }

    /// The PDF pane, and what its buttons ask for.
    pub fn pdf_pane(
        &mut self,
        ui: &mut eframe::egui::Ui,
        p: &crate::theme::Palette,
        backend: &Backend,
        settings: &mut Settings,
    ) -> bool {
        self.pane.view.dark = settings.pdf_dark && p.dark;
        self.pane.view.layout = crate::pdf::view::Layout::from_setting(&settings.pdf_layout);
        let (editors, root) = (&self.editors, &self.info.root);
        let (sources, generation) = (&self.build.sources, self.build.diag_gen);
        let source = |f: &str, from: usize, to: usize| -> Vec<String> {
            if let Some(e) = editors.get(f) {
                let to = to.min(e.doc.line_count());
                return (from..=to).map(|n| e.doc.line(n - 1)).collect();
            }
            let mut cache = sources.borrow_mut();
            if cache.0 != generation {
                *cache = (generation, Default::default());
            }
            let text = cache
                .1
                .entry(f.to_owned())
                .or_insert_with(|| std::fs::read_to_string(root.join(f)).ok());
            text.as_deref().map_or(Vec::new(), |t| {
                t.split('\n')
                    .skip(from - 1)
                    .take(to + 1 - from)
                    .map(|l| l.trim_end_matches('\r').to_owned())
                    .collect()
            })
        };
        let b = pane::Build {
            diagnostics: &self.build.diagnostics,
            building: self.build.building,
            note: self.build.note.as_deref(),
            failed: self.build.failed(),
            status: self.build.outcome.as_ref().map(|o| o.status),
            id: self.build.seq,
            logs_open: self.build.logs_open,
            source: &source,
        };
        let actions = self.pane.show(ui, p, &b);
        let mut settings_changed = false;
        for a in actions {
            match a {
                PaneAction::Logs(open) => {
                    self.build.logs_open = open;
                    if !open && !self.pane.has_pdf() {
                        backend.spawn(|shell| Msg::Pdf(shell.read_pdf()));
                    }
                }
                PaneAction::SourceClick(page, x, y) => {
                    backend.spawn(move |shell| {
                        Msg::SourceHit(shell.synctex_backward(page as u32, x as f64, y as f64))
                    });
                }
                PaneAction::Goto(file, line) => self.goto(backend, &file, line),
                PaneAction::AddPackage(pkg) => self.add_package(backend, settings, pkg),
                PaneAction::Export => self.export(backend, settings, Save::Pdf),
                PaneAction::OpenExternally => {
                    backend.spawn(|shell| Msg::Done(shell.open_pdf_externally()));
                }
                PaneAction::Present => self.present(ui.ctx()),
                PaneAction::Layout(l) => {
                    settings.pdf_layout = l.into();
                    settings_changed = true;
                }
                PaneAction::ReadLog => {
                    let id = self.build.seq;
                    backend.spawn(move |shell| Msg::Log(id, shell.read_log()));
                }
                PaneAction::OpenTerminal(file) => {
                    backend.spawn(move |shell| {
                        Msg::Done(
                            shell.open_install_terminal(lemmata_shell::InstallRequest::Package {
                                file,
                            }),
                        )
                    });
                }
            }
        }
        settings_changed
    }

    fn add_package(&mut self, backend: &Backend, settings: &Settings, pkg: String) {
        // The main file is saved first, so the package goes into what the user sees.
        let main = self.info.main.clone();
        if self.buffers.is_dirty(&main) {
            self.no_build.insert(main.clone());
            self.write(backend, &main, settings);
            self.adding_package = Some(pkg);
        } else {
            backend.spawn(move |shell| Msg::PackageAdded(shell.add_package(pkg)));
        }
    }

    /// A file was saved: the build it asks for, and a package waiting for the main file.
    pub fn after_save(&mut self, backend: &Backend, settings: &Settings, path: &str) {
        let quiet = self.no_build.remove(path);
        let auto = self.auto_writes.remove(path);
        if settings.auto_compile && !quiet {
            self.build_soon(auto, false);
        }
        if *path == self.info.main
            && let Some(pkg) = self.adding_package.take()
        {
            backend.spawn(move |shell| Msg::PackageAdded(shell.add_package(pkg)));
        }
    }

    /// A build's, the PDF's or an export's message; others are handed back.
    pub fn on_build_msg(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        msg: Msg,
    ) -> Option<Msg> {
        match msg {
            Msg::Built(seq, result) => {
                if seq == self.build.seq {
                    self.build.building = false;
                }
                let arxiv = self.build.arxiv.filter(|(s, _)| *s == seq);
                if arxiv.is_some() {
                    self.build.arxiv = None;
                }
                if seq != self.build.seq {
                    return None;
                }
                match result {
                    Ok(o) if o.status == Status::Cancelled => {}
                    Ok(o) => {
                        self.build.diagnostics = o.diagnostics.clone();
                        self.build.diag_gen += 1;
                        self.build.finished_at = Some(std::time::SystemTime::now());
                        // Like Overleaf: a failed build with no PDF opens the logs.
                        if matches!(o.status, Status::Failed | Status::Timeout) && o.pdf.is_none() && self.build.errors() > 0 {
                            self.build.logs_open = true;
                        }
                        let wrote = o.pdf.is_some();
                        let ok = o.status == Status::Success;
                        self.build.outcome = Some(o);
                        if wrote {
                            backend.spawn(|shell| Msg::Pdf(shell.read_pdf()));
                            // The library thumbnail, at most every two minutes.
                            if self.build.last_thumbnail.is_none_or(|t| t.elapsed() >= Duration::from_secs(120)) {
                                self.build.last_thumbnail = Some(Instant::now());
                                self.pane.view.request_png();
                            }
                        } else if !self.pane.has_pdf() {
                            // No new PDF (up to date, or it failed), but an older one may exist.
                            backend.spawn(|shell| Msg::Pdf(shell.read_pdf()));
                        }
                        if let Some((_, flatten)) = arxiv {
                            if ok {
                                backend.save(Save::Arxiv { flatten }, format!("{}-arxiv.zip", self.info.name));
                            } else {
                                self.notices.push(Notice::new(
                                    "The document doesn't build without errors, so arXiv would likely reject it. Fix the errors in Logs first.",
                                    Tone::Info,
                                ).action(NoticeAction::ArxivAnyway(flatten)));
                            }
                        }
                    }
                    Err(e) => {
                        self.build.outcome = None;
                        self.build.diagnostics = vec![Diagnostic {
                            severity: Severity::Error,
                            kind: Kind::Other,
                            file: None,
                            line: None,
                            message: e,
                            hint: None,
                            fix: None,
                        }];
                        self.build.diag_gen += 1;
                        self.build.logs_open = true;
                    }
                }
            }
            Msg::Pdf(Ok(bytes)) => self.pane.view.load(bytes),
            // No PDF has been built yet: the pane explains that.
            Msg::Pdf(Err(_)) => {}
            Msg::PdfCurrent(current) => {
                if !current {
                    self.run_build(backend, settings, false);
                }
            }
            Msg::Synced(Ok(boxes)) => {
                if let Some(b) = boxes.first() {
                    self.build.logs_open = false;
                    let ctx = backend.ctx().clone();
                    self.pane.view.show_box(&ctx, b.page as usize, b.h as f32, b.v as f32, b.width as f32, b.height as f32);
                }
            }
            Msg::Synced(Err(_)) => {}
            Msg::SourceHit(Ok(Some(hit))) => {
                if hit.inside_project {
                    self.goto(backend, &hit.path, hit.line as usize);
                }
            }
            Msg::SourceHit(Ok(None)) => {}
            Msg::SourceHit(Err(e)) => self.notice(format!("Couldn't jump to the source: {e}"), Tone::Info),
            Msg::Log(id, log) => self.pane.problems.set_log(id, log),
            Msg::Cleaned(Ok(n)) => {
                self.notice(if n > 0 { "Cleared the cached files." } else { "Nothing to clear." }, Tone::Ok);
                self.save_all_quietly(backend, settings);
                self.build_soon(false, false);
            }
            Msg::Cleaned(Err(e)) => self.notice(e, Tone::Info),
            Msg::SaveTo(_, None) => {}
            Msg::SaveTo(what, Some(dest)) => {
                let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let dest = dest.to_string_lossy().into_owned();
                backend.spawn(move |shell| {
                    let r = match what {
                        Save::Pdf => shell.export_pdf(dest).map(|_| 1),
                        Save::Zip => shell.export_zip(dest, true),
                        Save::Arxiv { flatten } => shell.export_arxiv(dest, true, Some(flatten)),
                    };
                    Msg::Exported(what, name.clone(), r)
                });
            }
            Msg::Exported(what, name, Ok(n)) => match what {
                Save::Pdf if self.info.config.draft => self.notices.push(
                    Notice::new(format!("Saved {name}, but it was built in Fast (draft) mode, so figures are empty boxes."), Tone::Info)
                        .action(NoticeAction::DraftOff),
                ),
                Save::Pdf => self.notice(format!("Saved {name}."), Tone::Info),
                Save::Zip => self.notice(format!("Saved {n} files, with the latest PDF, to {name}."), Tone::Ok),
                Save::Arxiv { flatten: true } => self.notice(format!("Exported {n} files with the sources merged into one .tex file and comments removed."), Tone::Info),
                Save::Arxiv { flatten: false } => self.notice(format!("Exported {n} files with comments removed, including the compiled bibliography."), Tone::Info),
            },
            Msg::Exported(_, _, Err(e)) => self.notice(e, Tone::Info),
            Msg::PackageAdded(Ok(())) => {
                // Read the main file again, with its new \usepackage; text typed meanwhile asks first.
                let main = self.info.main.clone();
                self.reload(backend, &main);
                self.run_build(backend, settings, false);
            }
            Msg::PackageAdded(Err(e)) => self.open_error = Some(e),
            Msg::Event(lemmata_shell::Event::BuildProgress(note)) => self.build.note = Some(note),
            Msg::Event(lemmata_shell::Event::BuildStarted) => {}
            Msg::Done(Err(e)) => self.notice(e, Tone::Error),
            other => return Some(other),
        }
        None
    }

    /// Runs a toast's button.
    pub fn notice_action(&mut self, backend: &Backend, settings: &Settings, a: NoticeAction) {
        match a {
            NoticeAction::ArxivAnyway(flatten) => backend.save(
                Save::Arxiv { flatten },
                format!("{}-arxiv.zip", self.info.name),
            ),
            NoticeAction::DraftOff => self.set_config(backend, settings, |c| c.draft = false),
            NoticeAction::LinkZotero => self.link_zotero(backend, settings),
            NoticeAction::UndoFiles => self.undo_files(backend),
            NoticeAction::TrustProject => self.trust_requested = true,
            NoticeAction::PasteAsText(path, from, table, text) => {
                if let Some(ed) = self.editors.get_mut(&path) {
                    let to = from + table.chars().count();
                    if to <= ed.doc.len() && ed.doc.slice(from, to) == table {
                        ed.replace_range(from, to, &text);
                        ed.refocus = true;
                    }
                }
            }
            NoticeAction::AddPackage(pkg) => self.add_package(backend, settings, pkg),
            NoticeAction::Download(code) => {
                self.notice(
                    format!(
                        "Downloading the {} dictionary…",
                        crate::writing::spell::label(&code)
                    ),
                    Tone::Info,
                );
                backend.spawn_async(move |shell| async move {
                    let r = shell.spell_download(code.clone()).await;
                    Msg::SpellDownloaded(code, r)
                });
            }
        }
    }

    /// Changes lemmata.toml without building (words, ignored rules).
    pub fn update_config(
        &mut self,
        backend: &Backend,
        change: impl FnOnce(&mut lemmata_project::Config),
    ) {
        let mut next = self.info.config.clone();
        change(&mut next);
        if next != self.info.config {
            self.info.config = next.clone();
            backend.spawn(move |shell| Msg::Info(shell.set_config(next)));
        }
    }

    /// Changes lemmata.toml, then saves everything and builds with the new settings.
    pub fn set_config(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        change: impl FnOnce(&mut lemmata_project::Config),
    ) {
        let mut next = self.info.config.clone();
        change(&mut next);
        if next == self.info.config {
            return;
        }
        self.info.config = next.clone();
        self.save_all_quietly(backend, settings);
        self.build.after_config = true;
        backend.spawn(move |shell| Msg::Info(shell.set_config(next)));
    }
}
