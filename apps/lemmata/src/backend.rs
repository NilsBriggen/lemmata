//! The app's side of lemmata-shell: the shell's calls run off the UI thread, and their results
//! and the shell's events come back as messages, each one waking the UI.

use eframe::egui;
use lemmata_shell::{Dirs, Event, Shell, TexStatus};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// A job's result to apply to the open project on the UI thread.
pub type Job = Box<dyn FnOnce(&mut crate::workspace::Workspace, &Backend) + Send>;

/// What the Preferences dialog waits for.
pub enum PrefsMsg {
    Dictionaries(Vec<lemmata_spell::Available>),
    Downloaded(String, Result<(), String>),
    Grammar(Result<(), String>),
}

pub enum Msg {
    Event(Event),
    /// A job's result, applied to the open project on the UI thread (see `Workspace::job`).
    Apply(u64, Job),
    Tex(TexStatus),
    /// Answers for the Preferences dialog.
    Prefs(PrefsMsg),
    /// A folder or file the user picked, for the job that asked.
    Picked(Pick, Option<PathBuf>),
    Opened(String, Result<lemmata_shell::ProjectInfo, String>),
    Trashed(String, Result<(), String>),
    Tectonic(Result<(), String>),
    Terminal(Result<(), String>),
    Example(Result<PathBuf, String>),
    /// The template list; `refreshed` once the online collection has been fetched (or failed).
    Templates(
        Result<Vec<lemmata_shell::projects::Template>, String>,
        Option<Result<(), String>>,
    ),
    TemplateDeleted(String, Result<(), String>),
    Created(Result<PathBuf, String>),
    Cloned(Result<String, String>),
    ZipInfo(Result<lemmata_shell::projects::ZipInfo, String>),
    Imported(Result<Vec<PathBuf>, String>),
    TemplateSaved(String, bool, Result<String, String>),
    /// A file of the open project, read to show it.
    FileRead(String, Result<String, String>),
    /// A file read again because another program changed it.
    DiskText(String, Result<String, String>),
    FileWritten {
        /// `Backend::project` when the write started.
        project: u64,
        path: String,
        written: String,
        result: Result<(), String>,
    },
    Outline(Result<lemmata_project::Outline, String>),
    /// The project's info again: after a config change, or new files.
    Info(Result<lemmata_shell::ProjectInfo, String>),
    /// A command's result that only matters when it failed.
    Done(Result<(), String>),
    /// A build finished (the number is the workspace's build sequence).
    Built(u64, Result<lemmata_tex::Outcome, String>),
    /// The project's PDF, read to show it.
    Pdf(Result<Vec<u8>, String>),
    /// Whether the PDF on disk is up to date, when the project opens.
    PdfCurrent(bool),
    Synced(Result<Vec<lemmata_synctex::PdfBox>, String>),
    SourceHit(Result<Option<lemmata_shell::SourceHit>, String>),
    /// The raw TeX log, read after this build.
    Log(u64, Result<String, String>),
    Cleaned(Result<usize, String>),
    /// Where to save an export, or None when the dialog was cancelled.
    SaveTo(Save, Option<PathBuf>),
    /// An export finished: what, the file name, and how many files went in.
    Exported(Save, String, Result<usize, String>),
    PackageAdded(Result<(), String>),
    /// What completion and hover cards look up: labels, citation keys, package names.
    Labels(Result<Vec<lemmata_project::extras::Label>, String>),
    Cites(Result<Vec<lemmata_bib::Entry>, String>),
    Packages(Vec<String>),
    /// Whether the main file loads a package something just inserted needs.
    HasPackage(String, bool),
    /// texlab started (its generation) or why it didn't.
    LspStarted(Result<u64, String>),
    /// A message to texlab couldn't be sent: it stopped.
    LspLost(u64),
    /// The spell checker loaded a language (None: off), or failed to.
    SpellLoaded(Option<String>, Result<(), String>),
    /// The dictionary for a language isn't here yet: offer it.
    Dictionary(lemmata_spell::Available),
    SpellDownloaded(String, Result<(), String>),
    /// LanguageTool's findings for a paragraph text, asked of this server and language.
    GrammarFound(
        (String, String),
        String,
        Result<Vec<lemmata_grammar::Match>, String>,
    ),
    /// An image or PDF of the project, read for its viewer tab.
    Viewed(String, Result<Viewed, String>),
}

pub enum Viewed {
    Pdf(Vec<u8>),
    Image(egui::ColorImage),
}

/// The exports that ask where to save.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Save {
    Pdf,
    Zip,
    Arxiv { flatten: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    ProjectFolder,
    TexFolder,
    OverleafZip,
    /// Where a new, cloned or imported project goes.
    Parent,
}

pub struct Backend {
    pub shell: Arc<Shell>,
    runtime: tokio::runtime::Runtime,
    tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    ctx: egui::Context,
    /// Counts the projects opened: a result started for one is dropped once another is open.
    pub project: std::sync::atomic::AtomicU64,
}

/// Where the bundled templates/ and dictionaries/ are: next to the program in a package, the
/// repository's own folders (through apps/lemmata/resources) in a development build.
pub fn resources_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LEMMATA_RESOURCES") {
        return dir.into();
    }
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("resources")));
    match packaged {
        Some(d) if d.is_dir() => d,
        // shortcut: development builds read the repository through symlinks; packaging (M9) copies the folders.
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    }
}

impl Backend {
    pub fn new(ctx: egui::Context, dirs: &crate::store::Dirs) -> Self {
        let (tx, rx) = channel();
        let sink_tx = tx.clone();
        let sink_ctx = ctx.clone();
        let shell = Arc::new(Shell::new(
            Dirs {
                resources: resources_dir(),
                data: dirs.data.clone(),
                cache: dirs.cache.clone(),
                home: dirs::home_dir(),
            },
            Arc::new(move |e| {
                let _ = sink_tx.send(Msg::Event(e));
                sink_ctx.request_repaint();
            }),
        ));
        shell.start_detection();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");
        Self {
            shell,
            runtime,
            tx,
            rx,
            ctx,
            project: Default::default(),
        }
    }

    /// The project now open, to tag work started for it.
    pub fn project(&self) -> u64 {
        self.project.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Runs `job` on a blocking worker and sends what it returns.
    pub fn spawn(&self, job: impl FnOnce(&Shell) -> Msg + Send + 'static) {
        let (shell, tx, ctx) = (self.shell.clone(), self.tx.clone(), self.ctx.clone());
        self.runtime.spawn_blocking(move || {
            let _ = tx.send(job(&shell));
            ctx.request_repaint();
        });
    }

    /// Runs one of the shell's async calls and sends what it returns.
    pub fn spawn_async<F, Fut>(&self, job: F)
    where
        F: FnOnce(Arc<Shell>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Msg> + Send + 'static,
    {
        let (shell, tx, ctx) = (self.shell.clone(), self.tx.clone(), self.ctx.clone());
        self.runtime.spawn(async move {
            let _ = tx.send(job(shell).await);
            ctx.request_repaint();
        });
    }

    /// A native folder or file chooser, without blocking the UI.
    pub fn pick(&self, what: Pick) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.runtime.spawn(async move {
            let dialog = rfd::AsyncFileDialog::new();
            let picked = match what {
                Pick::ProjectFolder => {
                    dialog
                        .set_title("Open a LaTeX project folder")
                        .pick_folder()
                        .await
                }
                Pick::TexFolder => {
                    dialog
                        .set_title("Choose the folder that contains pdflatex and latexmk")
                        .pick_folder()
                        .await
                }
                Pick::Parent => {
                    dialog
                        .set_title("Choose where to put the project")
                        .pick_folder()
                        .await
                }
                Pick::OverleafZip => {
                    dialog
                        .set_title("Choose a zip downloaded from Overleaf")
                        .add_filter("Zip", &["zip"])
                        .pick_file()
                        .await
                }
            };
            let _ = tx.send(Msg::Picked(what, picked.map(|h| h.path().to_path_buf())));
            ctx.request_repaint();
        });
    }

    /// A native save dialog for an export, without blocking the UI.
    pub fn save(&self, what: Save, name: String) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.runtime.spawn(async move {
            let (title, filter, ext) = match what {
                Save::Pdf => ("Export PDF", "PDF", "pdf"),
                Save::Zip => ("Export the project as a zip", "Zip", "zip"),
                Save::Arxiv { .. } => ("Export for arXiv", "Zip", "zip"),
            };
            let picked = rfd::AsyncFileDialog::new()
                .set_title(title)
                .set_file_name(name)
                .add_filter(filter, &[ext])
                .save_file()
                .await;
            let _ = tx.send(Msg::SaveTo(what, picked.map(|h| h.path().to_path_buf())));
            ctx.request_repaint();
        });
    }

    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }
}
