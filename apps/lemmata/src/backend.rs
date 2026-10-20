//! The app's side of lemmata-shell: the shell's calls run off the UI thread, and their results
//! and the shell's events come back as messages, each one waking the UI.

use eframe::egui;
use lemmata_shell::{Dirs, Event, Shell, TexStatus};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Msg {
    Event(Event),
    Tex(TexStatus),
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
        path: String,
        written: String,
        result: Result<(), String>,
    },
    Outline(Result<lemmata_project::Outline, String>),
    /// The project's info again: after a config change, or new files.
    Info(Result<lemmata_shell::ProjectInfo, String>),
    /// A command's result that only matters when it failed.
    Done(Result<(), String>),
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
        }
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
}
