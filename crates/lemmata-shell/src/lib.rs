//! The app's backend without a UI: everything the Tauri shell's commands did, as methods on
//! [`Shell`], so the native app can call it directly. All file paths the UI passes are relative
//! to the open project's root.

pub mod bib;
pub mod export;
pub mod figures;
pub mod files;
pub mod git;
pub mod grammar;
pub mod history;
pub mod lsp;
pub mod online_templates;
pub mod open_with;
pub mod projects;
pub mod spell;
pub mod tectonic;
pub mod terminal;
pub mod tools;
pub mod update;

use lemmata_project::{Config, Entry, Project};
use lemmata_tex::detect::Detection;
use lemmata_tex::os::{Instructions, OsInfo};
use lemmata_tex::{BuildOptions, Outcome};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::oneshot;

/// Locking that survives a poisoned mutex. One panicking thread would
/// otherwise make every later command that touches the same state panic too,
/// leaving the app unusable; the guarded values are plain data, so carrying
/// on with what is there is the better outcome.
pub(crate) trait LockSafe<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockSafe<T> for Mutex<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Where the app keeps its files.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// Shipped with the app: has `templates/` and `dictionaries/`.
    pub resources: PathBuf,
    /// The app's data folder: user templates, downloaded dictionaries, Tectonic, the example.
    pub data: PathBuf,
    /// The app's cache folder: thumbnails and the template collection.
    pub cache: PathBuf,
    pub home: Option<PathBuf>,
}

/// What the backend tells the UI without being asked.
#[derive(Debug, Clone)]
pub enum Event {
    /// An event from one project generation.
    Project(u64, Box<Event>),
    BuildStarted,
    /// What the build status says now (Tectonic fetching packages).
    BuildProgress(String),
    /// Files changed or removed by another program.
    FilesChanged {
        changed: Vec<String>,
        removed: Vec<String>,
    },
    /// A message from texlab, tagged with the server's generation.
    LspMessage {
        generation: u64,
        body: String,
    },
    LspExit {
        generation: u64,
    },
    /// A second launch or the OS asked to open something.
    OpenRequest(open_with::Request),
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Shell {
    pub dirs: Dirs,
    sink: Sink,
    /// Held for the whole length of a build, so a new build starts only after
    /// the one it replaced (and its child processes) has fully stopped.
    build_lock: tokio::sync::Mutex<()>,
    detection: Mutex<Option<Detection>>,
    /// The detection started at launch, handed to the first `tex_status` (or
    /// build) so the UI doesn't start a second one.
    startup_detection: Mutex<Option<Arc<OnceLock<Detection>>>>,
    /// latexmk could not run earlier: builds go straight to the engine. Reset by every new detection.
    latexmk_broken: AtomicBool,
    /// Folders the user pointed us at in onboarding.
    extra_tex_dirs: Mutex<Vec<PathBuf>>,
    pub(crate) project: Mutex<Option<Project>>,
    /// Sending on this cancels the build in progress.
    cancel_build: Mutex<Option<oneshot::Sender<()>>>,
    /// Bumped by every build request and by Stop: a build still waiting for the lock gives up when it's no longer the latest.
    build_generation: std::sync::atomic::AtomicU64,
    lsp: tokio::sync::Mutex<Option<lsp::Server>>,
    lsp_generation: std::sync::atomic::AtomicU64,
    pub(crate) speller: Mutex<Option<lemmata_spell::Speller>>,
    /// Stops when dropped, i.e. when another project opens.
    watcher: Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>,
    /// What the app last wrote to each file, so its own saves aren't reported as outside edits.
    written: files::Written,
    /// The open request from the command line or a second launch, until the UI takes it.
    open_request: open_with::OpenRequest,
    /// Project jobs share a read guard; opening another project takes the write guard.
    operation: tokio::sync::RwLock<()>,
    generation: std::sync::atomic::AtomicU64,
}

impl Shell {
    /// Also remembers what this launch's command line asks to open, and tells detection where a
    /// downloaded Tectonic lives.
    pub fn new(dirs: Dirs, sink: Sink) -> Self {
        let shell = Shell {
            dirs,
            sink,
            build_lock: Default::default(),
            detection: Default::default(),
            startup_detection: Default::default(),
            latexmk_broken: Default::default(),
            extra_tex_dirs: Default::default(),
            project: Default::default(),
            cancel_build: Default::default(),
            build_generation: Default::default(),
            lsp: Default::default(),
            lsp_generation: Default::default(),
            speller: Default::default(),
            watcher: Default::default(),
            written: Default::default(),
            open_request: open_with::OpenRequest::from_command_line(),
            operation: Default::default(),
            generation: Default::default(),
        };
        tectonic::init(&shell);
        shell
    }

    fn emit(&self, event: Event) {
        (self.sink)(event)
    }

    fn emit_project(&self, generation: u64, event: Event) {
        self.emit(Event::Project(generation, Box::new(event)));
    }

    pub fn current_project(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn project_blocking_read(&self) -> tokio::sync::RwLockReadGuard<'_, ()> {
        self.operation.blocking_read()
    }

    pub async fn project_read(&self) -> tokio::sync::RwLockReadGuard<'_, ()> {
        self.operation.read().await
    }

    /// Starts detecting in the background at launch, so it's done or under way
    /// when the UI asks.
    pub fn start_detection(self: &Arc<Self>) {
        let cell = Arc::new(OnceLock::new());
        *self.startup_detection.lock_safe() = Some(cell.clone());
        let shell = self.clone();
        std::thread::spawn(move || {
            cell.get_or_init(|| run_detection(&shell));
        });
    }
}

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct TexStatus {
    pub detection: Detection,
    pub os: OsInfo,
    /// Every platform's instructions, keyed by family, so the user can
    /// switch tabs.
    pub instructions: BTreeMap<String, Instructions>,
}

/// Detects TeX, remembers the result and gives latexmk another chance.
/// HTTPS with rustls on ring, as lemmata-bib and lemmata-spell set it up.
pub(crate) fn http_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    static PROVIDER: std::sync::Once = std::sync::Once::new();
    PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(timeout)
        .user_agent(concat!("Lemmata/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

fn run_detection(state: &Shell) -> Detection {
    let extra = state.extra_tex_dirs.lock_safe().clone();
    let detection = lemmata_tex::detect(&extra);
    *state.detection.lock_safe() = Some(detection.clone());
    state.latexmk_broken.store(false, Ordering::Relaxed);
    detection
}

/// The launch detection if it hasn't been used yet (waiting for it when it's
/// still running), otherwise a fresh one.
fn first_or_fresh_detection(state: &Shell, take: bool) -> Detection {
    let cell = {
        let mut slot = state.startup_detection.lock_safe();
        if take { slot.take() } else { slot.clone() }
    };
    match cell {
        Some(cell) => cell.get_or_init(|| run_detection(state)).clone(),
        None => run_detection(state),
    }
}

impl Shell {
    pub fn tex_status(&self, extra_dir: Option<String>) -> TexStatus {
        let mut fresh = false;
        if let Some(dir) = extra_dir {
            let mut extra = self.extra_tex_dirs.lock_safe();
            let dir = PathBuf::from(dir);
            if !extra.contains(&dir) {
                extra.insert(0, dir);
            }
            fresh = true;
        }
        if fresh {
            self.startup_detection.lock_safe().take();
        }
        let detection = first_or_fresh_detection(self, true);
        TexStatus {
            detection,
            os: lemmata_tex::os::detect(),
            instructions: lemmata_tex::os::all_instructions(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub name: String,
    pub main: String,
    pub main_candidates: Vec<String>,
    pub config: Config,
    pub tree: Vec<Entry>,
}

#[derive(Serialize)]
pub struct ProjectAnalysis {
    pub info: ProjectInfo,
    pub outline: lemmata_project::Outline,
    pub labels: Vec<lemmata_project::extras::Label>,
    pub cites: Vec<lemmata_bib::Entry>,
    pub pdf_current: bool,
}

fn rel_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn project_info(p: &Project) -> ProjectInfo {
    project_info_with_tree(p, lemmata_project::tree(&p.root, &p.config.outdir))
}

fn project_info_with_tree(p: &Project, tree: Vec<Entry>) -> ProjectInfo {
    ProjectInfo {
        root: p.root.clone(),
        name: p
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        main: rel_string(&p.main),
        main_candidates: p.main_candidates.iter().map(|c| rel_string(c)).collect(),
        config: p.config.clone(),
        tree,
    }
}

fn analysis_of(p: &Project, tree: Vec<Entry>) -> ProjectAnalysis {
    analysis_of_with(p, tree, |path| std::fs::read_to_string(path))
}

fn analysis_of_with(
    p: &Project,
    tree: Vec<Entry>,
    read: impl FnMut(&Path) -> std::io::Result<String>,
) -> ProjectAnalysis {
    let sources = lemmata_project::source_snapshot_with(p, &tree, read);
    let analysis = lemmata_project::analyze_with_sources(p, &tree, &sources);
    let files = lemmata_bib::bib_files_with_sources(p, &tree, &sources);
    let cites = lemmata_bib::entries(&p.root, &files);
    let pdf_current = lemmata_tex::up_to_date_with_tree(p, &tree);
    ProjectAnalysis {
        info: project_info_with_tree(p, tree),
        outline: analysis.outline,
        labels: analysis.labels,
        cites,
        pdf_current,
    }
}

impl Shell {
    pub fn analyze_project(&self) -> CmdResult<ProjectAnalysis> {
        let p = project(self)?;
        let tree = lemmata_project::tree(&p.root, &p.config.outdir);
        Ok(analysis_of(&p, tree))
    }

    pub fn open_project(&self, generation: u64, path: String) -> CmdResult<ProjectAnalysis> {
        self.generation
            .fetch_max(generation, std::sync::atomic::Ordering::Relaxed);
        let _opening = self.operation.blocking_write();
        if self.current_project() != generation {
            return Err("A newer project open superseded this one.".into());
        }
        lemmata_synctex::forget();
        cancel_running(self);
        let (project, tree) = Project::open_with_tree(&path).map_err(err)?;
        let analysis = analysis_of(&project, tree);
        if self.current_project() != generation {
            return Err("A newer project open superseded this one.".into());
        }
        let sink = self.sink.clone();
        let watcher_sink: Sink = Arc::new(move |event| {
            sink(Event::Project(generation, Box::new(event)));
        });
        // Without a watcher the app still works; it just won't notice outside edits.
        *self.watcher.lock_safe() = files::watch(
            watcher_sink,
            project.root.clone(),
            project.config.outdir.clone(),
            self.written.clone(),
        )
        .ok();
        *self.project.lock_safe() = Some(project);
        Ok(analysis)
    }
}

/// Opens a file or a web address with the system's program for it. From an AppImage it goes
/// through xdg-open with the user's environment: the plain opener would hand the AppImage's
/// libraries on to the PDF viewer or browser, which can then crash.
fn open_external(target: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPDIR").is_some() {
        let mut child = lemmata_env::host(&mut std::process::Command::new("xdg-open"))
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || child.wait());
        return Ok(());
    }
    open::that_detached(target).map_err(|e| e.to_string())
}

impl Shell {
    /// Opens a web page (the guide, a link in the PDF, a bug report) in the browser.
    pub fn open_url(&self, url: String) -> CmdResult<()> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("Only web addresses can be opened.".into());
        }
        open_external(&url).map_err(|e| format!("Could not open {url}: {e}"))
    }

    /// Opens the project's PDF with the system's default viewer. The path comes from the project, never the page.
    pub fn open_pdf_externally(&self) -> CmdResult<()> {
        let pdf = project(self)?.pdf_path();
        if !pdf.is_file() {
            return Err("There is no PDF yet. Compile the document first.".into());
        }
        open_external(&pdf.to_string_lossy()).map_err(|e| format!("Could not open the PDF: {e}"))
    }

    /// Opens a file inside the project (e.g. the PDF of tracked changes) with the system's default program.
    pub fn open_project_file(&self, path: String) -> CmdResult<()> {
        let root = project(self)?.root;
        let full = lemmata_project::canonical(Path::new(&path))
            .map_err(|e| format!("Could not open {path}: {e}"))?;
        // Only files of the open project, never an arbitrary path the page sends.
        if !full.starts_with(&root) || !full.is_file() {
            return Err(format!("{path} is not a file of this project."));
        }
        open_external(&full.to_string_lossy()).map_err(|e| format!("Could not open {path}: {e}"))
    }

    /// False when the file watcher couldn't start (e.g. the system's watch limit is reached).
    pub fn watching(&self) -> bool {
        self.watcher.lock_safe().is_some()
    }

    pub fn project_info_cmd(&self) -> CmdResult<ProjectInfo> {
        let guard = self.project.lock_safe();
        guard
            .as_ref()
            .map(project_info)
            .ok_or_else(|| "No project is open.".into())
    }

    pub fn set_config(&self, config: Config) -> CmdResult<ProjectInfo> {
        let mut guard = self.project.lock_safe();
        let p = guard.as_mut().ok_or("No project is open.")?;
        if let Some(main) = &config.main {
            p.main = PathBuf::from(main);
        }
        p.config = config;
        p.save_config().map_err(err)?;
        Ok(project_info(p))
    }
}

/// Resolves a project-relative path and refuses anything outside the project.
/// The open project, cloned so the lock is released before any slow work.
pub(crate) fn project(state: &Shell) -> CmdResult<Project> {
    state
        .project
        .lock_safe()
        .clone()
        .ok_or_else(|| "No project is open.".into())
}

fn resolve(state: &Shell, rel: &str) -> CmdResult<PathBuf> {
    let guard = state.project.lock_safe();
    let root = &guard.as_ref().ok_or("No project is open.")?.root;
    Ok(root.join(files::safe_rel(rel)?))
}

impl Shell {
    pub fn read_text(&self, path: String) -> CmdResult<String> {
        // A data file of hundreds of megabytes would make the editor unusable, not editable.
        const MAX_TEXT: u64 = 32 * 1024 * 1024;
        let abs = resolve(self, &path)?;
        if std::fs::metadata(&abs).is_ok_and(|m| m.len() > MAX_TEXT) {
            return Err(format!(
                "{path} is larger than 32 MB, too large to edit here. Open it in another program."
            ));
        }
        std::fs::read_to_string(&abs).map_err(|e| format!("Could not open {path}: {e}"))
    }
}

/// Starts the error of a save refused because the file changed on disk since Lemmata last read or wrote it.
const CONFLICT: &str = "conflict:";

impl Shell {
    /// `expected` is the hash of the text Lemmata last read or wrote. If the file
    /// on disk is different, another program changed it, and nothing is written.
    /// `root` is the project the text belongs to: a save that runs after another
    /// project opened still lands in its own.
    pub fn write_text(
        &self,
        root: &Path,
        path: String,
        content: String,
        expected: Option<u32>,
    ) -> CmdResult<()> {
        let abs = root.join(files::safe_rel(&path)?);
        // Through a symlink (a preamble shared between projects) to the file it names.
        let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
        if let (Some(expected), Ok(disk)) = (expected, std::fs::read(&abs))
            && files::content_hash(&disk) != expected
        {
            return Err(format!("{CONFLICT} {path} was changed by another program."));
        }
        // Write to a hidden sibling first so a crash never leaves a half-written
        // source file; the watcher ignores hidden files. The name keeps the full
        // file name, so main.tex and main.bib can't share a temp file.
        let name = abs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Unique per save, so two saves of one file in flight can't mix their temp files.
        static SAVES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SAVES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = abs.with_file_name(format!(".{name}.{n}.lemmata-tmp"));
        let old = std::fs::metadata(&abs).ok();
        // A file in a folder that doesn't exist yet (e.g. .lemmata/comments.json before the first build).
        if let Some(dir) = abs.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        files::write_synced(&tmp, content.as_bytes())
            .map_err(|e| format!("Could not save {path}: {e}"))?;
        // The file keeps its permissions (an edited script stays executable).
        if let Some(m) = old {
            let _ = std::fs::set_permissions(&tmp, m.permissions());
        }
        files::put_in_place(&tmp, &abs, &content)
            .map_err(|e| format!("Could not save {path}: {e}"))?;
        // From here on, an event for this exact content is our own doing.
        self.written
            .lock_safe()
            .insert(abs, files::hash_bytes(content.as_bytes()));
        Ok(())
    }
}

/// Cancels the running build and any waiting one; returns the new generation.
fn cancel_running(state: &Shell) -> u64 {
    let generation = state
        .build_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    if let Some(tx) = state.cancel_build.lock_safe().take() {
        let _ = tx.send(());
    }
    generation
}

impl Shell {
    pub async fn build(&self, shell_escape: bool, force: bool) -> CmdResult<Outcome> {
        let generation = cancel_running(self);
        // Wait for the cancelled build to kill latexmk and its children before starting another in the same folder.
        let _running = self.build_lock.lock().await;
        // Reachable by Stop before the check below: a Stop between the two still cancels.
        let (tx, rx) = oneshot::channel();
        *self.cancel_build.lock_safe() = Some(tx);
        // A newer build or Stop came while this one waited: don't run it at all.
        if self
            .build_generation
            .load(std::sync::atomic::Ordering::SeqCst)
            != generation
        {
            return Ok(Outcome {
                status: lemmata_tex::Status::Cancelled,
                duration_ms: 0,
                pdf: None,
                diagnostics: Vec::new(),
                latexmk_failed: false,
            });
        }
        let mut project = self
            .project
            .lock_safe()
            .clone()
            .ok_or("No project is open.")?;
        // A downloaded project's lemmata.toml can't run programs on its own: the user must have allowed it.
        project.config.shell_escape &= shell_escape;
        // The same consent lets the project's latexmkrc run.
        project.trusted = shell_escape;
        let known = self.detection.lock_safe().clone();
        let detection = match known {
            Some(d) => d,
            None => first_or_fresh_detection(self, false),
        };
        // Tectonic when chosen, or as the fallback on a machine without TeX.
        if project.config.engine == lemmata_project::Engine::Tectonic
            || (detection.tool("latexmk").is_none() && detection.tectonic.is_some())
        {
            let exe = detection.tectonic.as_deref().ok_or(
            "Tectonic is not installed. Install it from the start screen, or choose another compiler in the project settings.",
        )?;
            self.emit_project(self.current_project(), Event::BuildStarted);
            return lemmata_tex::tectonic::build(&project, exe, rx, force, &|n| {
                self.emit_project(
                    self.current_project(),
                    Event::BuildProgress(tectonic::fetch_message(n)),
                );
            })
            .await
            .map_err(err);
        }
        let latexmk = detection
            .tool("latexmk")
            .ok_or("latexmk was not found. Install TeX Live first.")?;

        self.emit_project(self.current_project(), Event::BuildStarted);
        let opts = BuildOptions {
            latexmk,
            bin_dir: detection.bin_dir.as_deref(),
            installer: Some(detection.installer(lemmata_tex::os::detect().family)),
            miktex: matches!(
                detection.distribution,
                lemmata_tex::Distribution::Miktex { .. }
            ),
            perl_dir: detection.tool("perl").and_then(|p| p.parent()),
        };
        let skip_latexmk = self.latexmk_broken.load(Ordering::Relaxed);
        let outcome = lemmata_tex::build_with(&project, &opts, rx, skip_latexmk, force)
            .await
            .map_err(err)?;
        if outcome.latexmk_failed {
            self.latexmk_broken.store(true, Ordering::Relaxed);
        }
        Ok(outcome)
    }

    /// "Clear cached files": removes aux files and logs so the next build starts clean.
    pub fn clean_build(&self) -> CmdResult<usize> {
        cancel_running(self);
        let guard = self.project.lock_safe();
        let project = guard.as_ref().ok_or("No project is open.")?;
        lemmata_tex::clean(project).map_err(err)
    }

    pub fn read_pdf(&self) -> CmdResult<Vec<u8>> {
        let pdf = {
            let guard = self.project.lock_safe();
            guard.as_ref().ok_or("No project is open.")?.pdf_path()
        };
        let bytes =
            std::fs::read(&pdf).map_err(|_| "No PDF yet. Build the project first.".to_string())?;
        Ok(bytes)
    }

    /// True when the open project's PDF is up to date, so opening it needn't build.
    pub fn pdf_is_current(&self) -> CmdResult<bool> {
        // The build's own check: compared with when the last good build began, so an edit saved while it ran isn't taken as built.
        let project = self
            .project
            .lock_safe()
            .clone()
            .ok_or("No project is open.")?;
        Ok(lemmata_tex::up_to_date(&project))
    }
}

fn synctex_and_pdf(state: &Shell) -> CmdResult<(PathBuf, PathBuf, PathBuf)> {
    let synctex = state
        .detection
        .lock_safe()
        .as_ref()
        .and_then(|d| d.tool("synctex").map(Path::to_path_buf))
        // The .synctex.gz is read in process; the program is only a fallback when that fails.
        .unwrap_or_else(|| PathBuf::from("synctex"));
    let guard = state.project.lock_safe();
    let p = guard.as_ref().ok_or("No project is open.")?;
    Ok((synctex, p.pdf_path(), p.root.clone()))
}

impl Shell {
    pub fn synctex_forward(
        &self,
        path: String,
        line: u32,
        column: u32,
    ) -> CmdResult<Vec<lemmata_synctex::PdfBox>> {
        let (synctex, pdf, _) = synctex_and_pdf(self)?;
        let abs = resolve(self, &path)?;
        lemmata_synctex::forward(&synctex, &pdf, &abs, line, column).map_err(err)
    }
}

#[derive(Serialize)]
pub struct SourceHit {
    /// Relative to the project root, or absolute when the source is outside it.
    pub path: String,
    pub line: u32,
    pub inside_project: bool,
}

impl Shell {
    pub fn synctex_backward(&self, page: u32, x: f64, y: f64) -> CmdResult<Option<SourceHit>> {
        let (synctex, pdf, root) = synctex_and_pdf(self)?;
        let Some(loc) = lemmata_synctex::backward(&synctex, &pdf, page, x, y).map_err(err)? else {
            return Ok(None);
        };
        // Plain form, like the project root: with `\\?\` it would never be inside the project on Windows.
        let file = lemmata_project::canonical(&loc.file).unwrap_or(loc.file);
        let (path, inside_project) = match file.strip_prefix(&root) {
            Ok(rel) => (rel_string(rel), true),
            Err(_) => (file.to_string_lossy().into_owned(), false),
        };
        Ok(Some(SourceHit {
            path,
            line: loc.line,
            inside_project,
        }))
    }
}

fn templates_dir(state: &Shell) -> CmdResult<PathBuf> {
    Ok(state.dirs.resources.join("templates"))
}

/// Templates the user saved from their own projects.
fn user_templates_dir(state: &Shell) -> CmdResult<PathBuf> {
    Ok(state.dirs.data.join("templates"))
}

/// The template collection's templates, kept so they work offline once seen.
fn online_templates_dir(state: &Shell) -> CmdResult<PathBuf> {
    Ok(state.dirs.cache.join("templates"))
}

impl Shell {
    pub fn list_templates(&self) -> CmdResult<Vec<projects::Template>> {
        Ok(projects::list_all(
            &templates_dir(self)?,
            &online_templates_dir(self)?,
            &user_templates_dir(self)?,
        ))
    }

    /// Brings the template collection's templates up to date; list_templates then includes them.
    pub async fn refresh_templates(&self) -> CmdResult<usize> {
        online_templates::refresh(&online_templates_dir(self)?).await
    }

    /// Saves a project (the open one, or one from the library) as a template for new projects.
    pub fn save_as_template(
        &self,
        path: String,
        name: String,
        description: String,
    ) -> CmdResult<String> {
        let p = Project::open(&path).map_err(err)?;
        projects::save_as_template(
            &user_templates_dir(self)?,
            &p.root,
            &p.config.outdir,
            &name,
            &description,
        )
    }

    pub fn delete_template(&self, id: String) -> CmdResult<()> {
        projects::delete_user_template(&user_templates_dir(self)?, &id)
    }

    pub fn create_project(
        &self,
        template: String,
        parent: String,
        name: String,
    ) -> CmdResult<PathBuf> {
        let dir = projects::create_from_template(
            &templates_dir(self)?,
            &online_templates_dir(self)?,
            &user_templates_dir(self)?,
            &template,
            Path::new(&parent),
            &name,
        )?;
        // Only Tectonic can build here, so the project should say so.
        let detection = match self.detection.lock_safe().clone() {
            Some(d) => d,
            None => first_or_fresh_detection(self, false),
        };
        if !detection.tex_ready && detection.tectonic.is_some() {
            projects::use_tectonic(&dir)?;
        }
        Ok(dir)
    }

    /// Starts a folder that has no main document: writes a minimal main.tex, never over an existing one.
    pub fn create_main(&self, dir: String) -> CmdResult<()> {
        let dir = Path::new(&dir);
        if !dir.is_dir() {
            return Err(format!("{} is not a folder.", dir.display()));
        }
        let main = dir.join("main.tex");
        std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&main)
        .and_then(|mut f| {
            std::io::Write::write_all(
                &mut f,
                b"\\documentclass{article}\n\n\\begin{document}\n\nStart writing here.\n\n\\end{document}\n",
            )
        })
        .map_err(|e| format!("Could not create {}: {e}", main.display()))
    }

    /// The example document, created in Lemmata's data folder on first use.
    pub fn create_example(&self) -> CmdResult<PathBuf> {
        let dir = self.dirs.data.join(projects::EXAMPLE);
        projects::ensure_example(&templates_dir(self)?, &dir)
    }

    /// Stops the build in progress, if any.
    pub fn stop_build(&self) {
        cancel_running(self);
    }

    /// The full TeX log of the last build, for the "Raw log" view.
    pub fn read_log(&self) -> CmdResult<String> {
        let path = self
            .project
            .lock_safe()
            .as_ref()
            .map(|p| p.log_path())
            .ok_or("No project is open.")?;
        std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|_| "There's no log yet. Build the project first.".into())
    }

    /// Copies files chosen by the user into a project folder (`folder` is
    /// project-relative; empty for the root). Returns the new relative paths.
    pub fn add_files(&self, paths: Vec<String>, folder: String) -> CmdResult<Vec<String>> {
        let root = self
            .project
            .lock_safe()
            .as_ref()
            .map(|p| p.root.clone())
            .ok_or("No project is open.")?;
        let rel = Path::new(&folder);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("That folder is outside the project.".into());
        }
        let dest_dir = root.join(rel);
        std::fs::create_dir_all(&dest_dir).map_err(err)?;
        let mut added = Vec::new();
        for p in paths {
            let src = PathBuf::from(&p);
            let name = src.file_name().ok_or("Not a file.")?;
            let dest = dest_dir.join(name);
            if dest.exists() {
                return Err(format!(
                    "{} already exists in the project.",
                    name.to_string_lossy()
                ));
            }
            std::fs::copy(&src, &dest).map_err(|e| format!("Could not copy {p}: {e}"))?;
            added.push(
                dest.strip_prefix(&root)
                    .unwrap_or(&dest)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        Ok(added)
    }

    pub fn import_overleaf_zip(
        &self,
        zip: String,
        parent: String,
        name: String,
    ) -> CmdResult<Vec<PathBuf>> {
        projects::import_zip(Path::new(&zip), Path::new(&parent), &name)
    }

    pub fn inspect_zip(&self, zip: String) -> CmdResult<projects::ZipInfo> {
        projects::inspect_zip(Path::new(&zip))
    }
}

fn thumbnail_path(state: &Shell, root: &str) -> CmdResult<PathBuf> {
    use std::hash::{Hash, Hasher};
    // The library asks with the path as it was opened, the project saves with its canonical root:
    // on Windows a short name (RUNNER~1) or another case would otherwise miss the thumbnail.
    let root = lemmata_project::canonical(Path::new(root))
        .map_or_else(|_| root.to_owned(), |p| p.to_string_lossy().into_owned());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut h);
    let dir = state.dirs.cache.join("thumbnails");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.join(format!("{:016x}.png", h.finish())))
}

impl Shell {
    /// Stores page 1 of a project's PDF, rendered by the frontend, for the library.
    pub fn save_thumbnail(&self, root: &str, png: &[u8]) -> CmdResult<()> {
        std::fs::write(thumbnail_path(self, root)?, png).map_err(err)
    }

    pub fn load_thumbnail(&self, root: String) -> CmdResult<Vec<u8>> {
        let bytes = std::fs::read(thumbnail_path(self, &root)?)
            .map_err(|_| "No thumbnail yet.".to_string())?;
        Ok(bytes)
    }

    /// Moves a whole project folder to the system trash, from the library.
    pub fn trash_project(&self, path: String) -> CmdResult<()> {
        let dir = projects::deletable_project(Path::new(&path), self.dirs.home.as_deref())?;
        if self
            .project
            .lock_safe()
            .as_ref()
            .is_some_and(|p| p.root == dir)
        {
            cancel_running(self);
            *self.watcher.lock_safe() = None;
            *self.project.lock_safe() = None;
        }
        // Its thumbnail's name comes from the folder, which is gone afterwards.
        let thumb = thumbnail_path(self, &path);
        // Windows refuses while a process still has a file in the folder open: the language server
        // was only just told to quit, and virus scanners and the indexer open new files for a moment.
        let mut tries = 0;
        while let Err(e) = trash::delete(&dir) {
            tries += 1;
            if tries == 16 {
                return Err(format!(
                    "Could not move {} to the trash: {e}",
                    dir.display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        if let Ok(thumb) = thumb {
            let _ = std::fs::remove_file(thumb);
        }
        Ok(())
    }

    /// Adds `\usepackage{name}` to the main file's preamble, after the last
    /// \usepackage (or after \documentclass).
    pub fn add_package(&self, package: String) -> CmdResult<()> {
        self.add_package_with(package, |p| {
            std::fs::read_to_string(p.main_abs()).map_err(err)
        })
    }

    fn add_package_with(
        &self,
        package: String,
        read: impl FnOnce(&lemmata_project::Project) -> CmdResult<String>,
    ) -> CmdResult<()> {
        if !package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("Not a package name.".into());
        }
        let p = project(self)?;
        let text = read(&p)?;
        let lines: Vec<&str> = text.lines().collect();
        let begin = lines
            .iter()
            .position(|l| l.contains("\\begin{document}"))
            .unwrap_or(lines.len());
        let after = lines[..begin]
            .iter()
            .rposition(|l| l.trim_start().starts_with("\\usepackage"))
            .or_else(|| {
                lines[..begin]
                    .iter()
                    .position(|l| l.contains("\\documentclass"))
            })
            .ok_or("The main file has no \\documentclass.")?;
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out.insert(after + 1, format!("\\usepackage{{{package}}}"));
        let mut joined = out.join("\n");
        if text.ends_with('\n') {
            joined.push('\n');
        }
        self.write_text(
            &p.root,
            rel_string(&p.main),
            joined,
            Some(files::content_hash(text.as_bytes())),
        )
    }

    pub fn outline(&self) -> CmdResult<lemmata_project::Outline> {
        Ok(lemmata_project::outline(&project(self)?))
    }

    pub fn labels(&self) -> CmdResult<Vec<lemmata_project::extras::Label>> {
        Ok(lemmata_project::extras::labels(&project(self)?))
    }

    pub fn word_count(&self) -> CmdResult<lemmata_project::extras::Counts> {
        Ok(lemmata_project::extras::counts(&project(self)?))
    }

    pub fn package_names(&self) -> Vec<String> {
        lemmata_pkgmap::package_names().to_vec()
    }

    /// Starts (or restarts) texlab for the open project. Returns the server's
    /// generation, which tags every `lsp-message` event it sends.
    pub async fn lsp_start(&self) -> CmdResult<u64> {
        let project = self.current_project();
        let root = self
            .project
            .lock_safe()
            .as_ref()
            .map(|p| p.root.clone())
            .ok_or("No project is open.")?;
        let bin = self
            .detection
            .lock_safe()
            .as_ref()
            .and_then(|d| d.bin_dir.clone());
        let generation = self
            .lsp_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if let Some(old) = self.lsp.lock().await.take() {
            old.stop().await;
        }
        if self.current_project() != project {
            return Err("The project was closed.".into());
        }
        let sink = self.sink.clone();
        let lsp_sink: Sink = Arc::new(move |event| {
            sink(Event::Project(project, Box::new(event)));
        });
        let server =
            lsp::start(lsp_sink, &root, &self.dirs.data, bin.as_deref(), generation).await?;
        if self.current_project() != project {
            drop(server);
            return Err("The project was closed.".into());
        }
        *self.lsp.lock().await = Some(server);
        Ok(generation)
    }

    /// Stops texlab when its project closes, unless a newer one has started since.
    pub async fn lsp_stop(&self, generation: u64) -> CmdResult<()> {
        let mut slot = self.lsp.lock().await;
        if slot.as_ref().is_some_and(|s| s.generation == generation)
            && let Some(server) = slot.take()
        {
            server.stop().await;
        }
        Ok(())
    }

    pub async fn lsp_send(&self, generation: u64, body: String) -> CmdResult<()> {
        let slot = self.lsp.lock().await;
        match slot.as_ref() {
            Some(server) if server.generation == generation => server.send(&body).await,
            _ => Err("The language server was restarted.".into()),
        }
    }
}

/// What to install in a terminal. The command itself is always built here
/// from Lemmata's own data, never taken from the webview.
#[derive(serde::Deserialize)]
#[serde(tag = "what", rename_all = "lowercase")]
pub enum InstallRequest {
    /// The onboarding command for a platform ("recommended" or "full").
    Tex { platform: String, full: bool },
    /// The package providing a missing file, e.g. `pgfplots.sty`.
    Package { file: String },
}

impl Shell {
    pub fn open_install_terminal(&self, request: InstallRequest) -> CmdResult<()> {
        let command = match request {
            InstallRequest::Tex { platform, full } => {
                let all = lemmata_tex::os::all_instructions();
                let i = all.get(&platform).ok_or("Unknown platform.")?;
                let cmd = if full { &i.full } else { &i.recommended };
                if cmd.starts_with("Install ") {
                    return Err(
                        "There's no single command for this platform. Follow the link instead."
                            .into(),
                    );
                }
                cmd.clone()
            }
            InstallRequest::Package { file } => {
                let detection = self
                    .detection
                    .lock_safe()
                    .clone()
                    .ok_or("TeX hasn't been detected yet.")?;
                let installer = detection.installer(lemmata_tex::os::detect().family);
                lemmata_pkgmap::fix_for(&file, installer)
                    .and_then(|f| f.command)
                    .ok_or_else(|| format!("No install command is known for {file}."))?
            }
        };
        terminal::open_with_command(&command)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn analysis_reads_each_source_once_and_keeps_one_snapshot() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("chapters")).unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\section{Intro}\\label{sec:i}\n\\input{chapters/method}\n\\end{document}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("chapters/method.tex"),
            "\\section{Method}\\label{sec:m}\n",
        )
        .unwrap();
        std::fs::write(d.path().join("broken.tex"), [0xff]).unwrap();
        std::fs::write(d.path().join("refs.bib"), "@misc{key, title={Title}}\n").unwrap();
        let p = lemmata_project::Project::open(d.path()).unwrap();
        let tree = lemmata_project::tree(&p.root, &p.config.outdir);
        let pdf_current = lemmata_tex::up_to_date_with_tree(&p, &tree);
        let mut reads = Vec::new();
        let analysis = crate::analysis_of_with(&p, tree, |path| {
            reads.push(path.to_owned());
            let text = std::fs::read_to_string(path)?;
            if path.ends_with("main.tex") {
                std::fs::write(path, "changed during analysis\n").unwrap();
            }
            Ok(text)
        });
        reads.sort();
        assert_eq!(
            reads,
            [
                p.root.join("broken.tex"),
                p.root.join("chapters/method.tex"),
                p.root.join("main.tex"),
            ]
        );
        assert_eq!(analysis.outline.headings.len(), 2);
        assert_eq!(analysis.labels.len(), 2);
        assert_eq!(analysis.cites.len(), 1);
        assert_eq!(analysis.cites[0].key, "key");
        assert_eq!(analysis.pdf_current, pdf_current);
    }

    #[test]
    fn a_thumbnail_is_found_under_any_spelling_of_its_folder() {
        let d = tempfile::tempdir().unwrap();
        let project = d.path().join("paper");
        std::fs::create_dir(&project).unwrap();
        let dirs = crate::Dirs {
            resources: d.path().into(),
            data: d.path().into(),
            cache: d.path().into(),
            home: None,
        };
        let shell = crate::Shell::new(dirs, std::sync::Arc::new(|_| {}));
        let canonical = lemmata_project::canonical(&project).unwrap();
        shell
            .save_thumbnail(&canonical.to_string_lossy(), b"png")
            .unwrap();
        // As the library may have it: not canonical (here `.` and `..` in it).
        let other = project.join(".").join("..").join("paper");
        assert_eq!(
            shell
                .load_thumbnail(other.to_string_lossy().into_owned())
                .unwrap(),
            b"png"
        );
    }

    #[test]
    fn add_package_conflicts_preserve_the_external_version() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}\n").unwrap();
        let shell = crate::Shell::new(
            crate::Dirs {
                resources: d.path().into(),
                data: d.path().into(),
                cache: d.path().into(),
                home: None,
            },
            std::sync::Arc::new(|_| {}),
        );
        shell
            .open_project(1, d.path().to_string_lossy().into_owned())
            .unwrap();
        let error = shell
            .add_package_with("amsmath".into(), |project| {
                let main = project.main_abs();
                let original = std::fs::read_to_string(&main).unwrap();
                std::fs::write(&main, "external version\n").unwrap();
                Ok(original)
            })
            .unwrap_err();
        assert!(error.starts_with("conflict:"), "{error}");
        assert_eq!(
            std::fs::read_to_string(d.path().join("main.tex")).unwrap(),
            "external version\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn add_package_write_failure_preserves_the_original_file() {
        use std::os::unix::fs::PermissionsExt;

        let d = tempfile::tempdir().unwrap();
        let original = "\\documentclass{article}\n";
        std::fs::write(d.path().join("main.tex"), original).unwrap();
        let shell = crate::Shell::new(
            crate::Dirs {
                resources: d.path().into(),
                data: d.path().into(),
                cache: d.path().into(),
                home: None,
            },
            std::sync::Arc::new(|_| {}),
        );
        shell
            .open_project(1, d.path().to_string_lossy().into_owned())
            .unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = shell.add_package("amsmath".into());
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(d.path().join("main.tex")).unwrap(),
            original
        );
    }

    #[test]
    fn add_package_inserts_after_documentclass_and_keeps_the_final_newline() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}\n").unwrap();
        let shell = crate::Shell::new(
            crate::Dirs {
                resources: d.path().into(),
                data: d.path().into(),
                cache: d.path().into(),
                home: None,
            },
            std::sync::Arc::new(|_| {}),
        );
        shell
            .open_project(1, d.path().to_string_lossy().into_owned())
            .unwrap();
        shell.add_package("amsmath".into()).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("main.tex")).unwrap(),
            "\\documentclass{article}\n\\usepackage{amsmath}\n"
        );
    }

    #[test]
    fn an_older_open_cannot_replace_a_newer_project() {
        let d = tempfile::tempdir().unwrap();
        for name in ["a", "b"] {
            let root = d.path().join(name);
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("main.tex"), "\\documentclass{article}").unwrap();
        }
        let dirs = crate::Dirs {
            resources: d.path().into(),
            data: d.path().into(),
            cache: d.path().into(),
            home: None,
        };
        let shell = crate::Shell::new(dirs, std::sync::Arc::new(|_| {}));
        shell
            .open_project(2, d.path().join("b").to_string_lossy().into_owned())
            .unwrap();
        assert!(
            shell
                .open_project(1, d.path().join("a").to_string_lossy().into_owned())
                .is_err()
        );
        assert_eq!(shell.current_project(), 2);
        assert_eq!(
            crate::project(&shell).unwrap().root,
            lemmata_project::canonical(&d.path().join("b")).unwrap()
        );
    }
}
