//! Tauri commands the frontend calls. All file paths crossing the IPC
//! boundary are relative to the open project's root.

#[cfg(debug_assertions)]
mod automation;
mod bib;
mod export;
mod figures;
mod files;
mod git;
mod grammar;
mod history;
mod lsp;
mod online_templates;
mod open_with;
mod projects;
mod spell;
mod tectonic;
mod terminal;
mod tools;
mod updates;

use lemmata_project::{Config, Entry, Project};
use lemmata_tex::detect::Detection;
use lemmata_tex::os::{Instructions, OsInfo};
use lemmata_tex::{BuildOptions, Outcome};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::ipc::Response;
use tauri::{Emitter, Manager, State};
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

#[derive(Default)]
pub(crate) struct AppState {
    /// Held for the whole length of a build, so a new build starts only after
    /// the one it replaced (and its child processes) has fully stopped.
    build_lock: tokio::sync::Mutex<()>,
    detection: Mutex<Option<Detection>>,
    /// The detection started at launch, handed to the first `tex_status` (or
    /// build) so the webview doesn't start a second one.
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
    pub(crate) update: tokio::sync::Mutex<Option<tauri_plugin_updater::Update>>,
    /// Stops when dropped, i.e. when another project opens.
    watcher: Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>,
    /// What the app last wrote to each file, so its own saves aren't reported as outside edits.
    written: files::Written,
}

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
struct TexStatus {
    detection: Detection,
    os: OsInfo,
    /// Every platform's instructions, keyed by family, so the user can
    /// switch tabs.
    instructions: BTreeMap<String, Instructions>,
}

/// Detects TeX, remembers the result and gives latexmk another chance.
fn run_detection(state: &AppState) -> Detection {
    let extra = state.extra_tex_dirs.lock_safe().clone();
    let detection = lemmata_tex::detect(&extra);
    *state.detection.lock_safe() = Some(detection.clone());
    state.latexmk_broken.store(false, Ordering::Relaxed);
    detection
}

/// Starts detecting in the background at launch, so it's done or under way
/// when the webview asks.
fn start_detection(app: &tauri::AppHandle) {
    let cell = Arc::new(OnceLock::new());
    *app.state::<AppState>().startup_detection.lock_safe() = Some(cell.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        cell.get_or_init(|| run_detection(&app.state::<AppState>()));
    });
}

/// The launch detection if it hasn't been used yet (waiting for it when it's
/// still running), otherwise a fresh one.
fn first_or_fresh_detection(state: &AppState, take: bool) -> Detection {
    let cell = {
        let mut slot = state.startup_detection.lock_safe();
        if take { slot.take() } else { slot.clone() }
    };
    match cell {
        Some(cell) => cell.get_or_init(|| run_detection(state)).clone(),
        None => run_detection(state),
    }
}

#[tauri::command(async)]
fn tex_status(state: State<AppState>, extra_dir: Option<String>) -> TexStatus {
    let mut fresh = false;
    if let Some(dir) = extra_dir {
        let mut extra = state.extra_tex_dirs.lock_safe();
        let dir = PathBuf::from(dir);
        if !extra.contains(&dir) {
            extra.insert(0, dir);
        }
        fresh = true;
    }
    if fresh {
        state.startup_detection.lock_safe().take();
    }
    let detection = first_or_fresh_detection(&state, true);
    TexStatus {
        detection,
        os: lemmata_tex::os::detect(),
        instructions: lemmata_tex::os::all_instructions(),
    }
}

#[derive(Serialize)]
struct ProjectInfo {
    root: PathBuf,
    name: String,
    main: String,
    main_candidates: Vec<String>,
    config: Config,
    tree: Vec<Entry>,
}

fn rel_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn project_info(p: &Project) -> ProjectInfo {
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
        tree: lemmata_project::tree(&p.root, &p.config.outdir),
    }
}

#[tauri::command(async)]
fn open_project(
    app: tauri::AppHandle,
    state: State<AppState>,
    path: String,
) -> CmdResult<ProjectInfo> {
    cancel_running(&state);
    let project = Project::open(&path).map_err(err)?;
    let info = project_info(&project);
    // Without a watcher the app still works; it just won't notice outside edits.
    *state.watcher.lock_safe() = files::watch(
        app,
        project.root.clone(),
        project.config.outdir.clone(),
        state.written.clone(),
    )
    .ok();
    *state.project.lock_safe() = Some(project);
    Ok(info)
}

/// Opens a file or a web address with the system's program for it. From an AppImage it goes
/// through xdg-open with the user's environment: the opener plugin would hand the AppImage's
/// libraries on to the PDF viewer or browser, which can then crash.
fn open_external(app: &tauri::AppHandle, target: &str, url: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPDIR").is_some() {
        let mut child = lemmata_env::host(&mut std::process::Command::new("xdg-open"))
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || child.wait());
        return Ok(());
    }
    use tauri_plugin_opener::OpenerExt;
    let opener = app.opener();
    if url {
        opener.open_url(target, None::<&str>)
    } else {
        opener.open_path(target, None::<&str>)
    }
    .map_err(|e| e.to_string())
}

/// Opens a web page (the guide, a link in the PDF, a bug report) in the browser.
#[tauri::command(async)]
fn open_url(app: tauri::AppHandle, url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only web addresses can be opened.".into());
    }
    open_external(&app, &url, true).map_err(|e| format!("Could not open {url}: {e}"))
}

/// Opens the project's PDF with the system's default viewer. The path comes from the project, never the page.
#[tauri::command(async)]
fn open_pdf_externally(app: tauri::AppHandle, state: State<AppState>) -> CmdResult<()> {
    let pdf = project(&state)?.pdf_path();
    if !pdf.is_file() {
        return Err("There is no PDF yet. Compile the document first.".into());
    }
    open_external(&app, &pdf.to_string_lossy(), false)
        .map_err(|e| format!("Could not open the PDF: {e}"))
}

/// Opens a file inside the project (e.g. the PDF of tracked changes) with the system's default program.
#[tauri::command(async)]
fn open_project_file(app: tauri::AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    let root = project(&state)?.root;
    let full = lemmata_project::canonical(Path::new(&path))
        .map_err(|e| format!("Could not open {path}: {e}"))?;
    // Only files of the open project, never an arbitrary path the page sends.
    if !full.starts_with(&root) || !full.is_file() {
        return Err(format!("{path} is not a file of this project."));
    }
    open_external(&app, &full.to_string_lossy(), false)
        .map_err(|e| format!("Could not open {path}: {e}"))
}

/// False when the file watcher couldn't start (e.g. the system's watch limit is reached).
#[tauri::command(async)]
fn watching(state: State<AppState>) -> bool {
    state.watcher.lock_safe().is_some()
}

#[tauri::command(async)]
fn project_info_cmd(state: State<AppState>) -> CmdResult<ProjectInfo> {
    let guard = state.project.lock_safe();
    guard
        .as_ref()
        .map(project_info)
        .ok_or_else(|| "No project is open.".into())
}

#[tauri::command(async)]
fn set_config(state: State<AppState>, config: Config) -> CmdResult<ProjectInfo> {
    let mut guard = state.project.lock_safe();
    let p = guard.as_mut().ok_or("No project is open.")?;
    if let Some(main) = &config.main {
        p.main = PathBuf::from(main);
    }
    p.config = config;
    p.save_config().map_err(err)?;
    Ok(project_info(p))
}

/// Resolves a project-relative path and refuses anything outside the project.
/// The open project, cloned so the lock is released before any slow work.
pub(crate) fn project(state: &State<AppState>) -> CmdResult<Project> {
    state
        .project
        .lock_safe()
        .clone()
        .ok_or_else(|| "No project is open.".into())
}

fn resolve(state: &State<AppState>, rel: &str) -> CmdResult<PathBuf> {
    let guard = state.project.lock_safe();
    let root = &guard.as_ref().ok_or("No project is open.")?.root;
    Ok(root.join(files::safe_rel(rel)?))
}

#[tauri::command(async)]
fn read_text(state: State<AppState>, path: String) -> CmdResult<String> {
    let abs = resolve(&state, &path)?;
    std::fs::read_to_string(&abs).map_err(|e| format!("Could not open {path}: {e}"))
}

/// Starts the error of a save refused because the file changed on disk since Lemmata last read or wrote it.
const CONFLICT: &str = "conflict:";

#[tauri::command(async)]
/// `expected` is the hash of the text Lemmata last read or wrote. If the file
/// on disk is different, another program changed it, and nothing is written.
fn write_text(
    state: State<AppState>,
    path: String,
    content: String,
    expected: Option<u32>,
) -> CmdResult<()> {
    let abs = resolve(&state, &path)?;
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
    let tmp = abs.with_file_name(format!(".{name}.lemmata-tmp"));
    // A file in a folder that doesn't exist yet (e.g. .lemmata/comments.json before the first build).
    if let Some(dir) = abs.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&tmp, &content).map_err(|e| format!("Could not save {path}: {e}"))?;
    std::fs::rename(&tmp, &abs).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Could not save {path}: {e}")
    })?;
    // From here on, an event for this exact content is our own doing.
    state
        .written
        .lock_safe()
        .insert(abs, files::hash_bytes(content.as_bytes()));
    Ok(())
}

/// Cancels the running build and any waiting one; returns the new generation.
fn cancel_running(state: &State<AppState>) -> u64 {
    let generation = state
        .build_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    if let Some(tx) = state.cancel_build.lock_safe().take() {
        let _ = tx.send(());
    }
    generation
}

#[tauri::command]
async fn build(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    shell_escape: bool,
    force: bool,
) -> CmdResult<Outcome> {
    let generation = cancel_running(&state);
    // Wait for the cancelled build to kill latexmk and its children before starting another in the same folder.
    let _running = state.build_lock.lock().await;
    // A newer build or Stop came while this one waited: don't run it at all.
    if state
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
    let (tx, rx) = oneshot::channel();
    *state.cancel_build.lock_safe() = Some(tx);

    let mut project = state
        .project
        .lock_safe()
        .clone()
        .ok_or("No project is open.")?;
    // A downloaded project's lemmata.toml can't run programs on its own: the user must have allowed it.
    project.config.shell_escape &= shell_escape;
    let known = state.detection.lock_safe().clone();
    let detection = match known {
        Some(d) => d,
        None => first_or_fresh_detection(&state, false),
    };
    // Tectonic when chosen, or as the fallback on a machine without TeX.
    if project.config.engine == lemmata_project::Engine::Tectonic
        || (detection.tool("latexmk").is_none() && detection.tectonic.is_some())
    {
        let exe = detection.tectonic.as_deref().ok_or(
            "Tectonic is not installed. Install it from the start screen, or choose another compiler in the project settings.",
        )?;
        let _ = app.emit("build-started", ());
        return lemmata_tex::tectonic::build(&project, exe, rx, force, &|n| {
            let _ = app.emit("build-progress", tectonic::fetch_message(n));
        })
        .await
        .map_err(err);
    }
    let latexmk = detection
        .tool("latexmk")
        .ok_or("latexmk was not found. Install TeX Live first.")?;

    let _ = app.emit("build-started", ());
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
    let skip_latexmk = state.latexmk_broken.load(Ordering::Relaxed);
    let outcome = lemmata_tex::build_with(&project, &opts, rx, skip_latexmk, force)
        .await
        .map_err(err)?;
    if outcome.latexmk_failed {
        state.latexmk_broken.store(true, Ordering::Relaxed);
    }
    Ok(outcome)
}

/// "Clear cached files": removes aux files and logs so the next build starts clean.
#[tauri::command(async)]
fn clean_build(state: State<AppState>) -> CmdResult<usize> {
    cancel_running(&state);
    let guard = state.project.lock_safe();
    let project = guard.as_ref().ok_or("No project is open.")?;
    lemmata_tex::clean(project).map_err(err)
}

#[tauri::command(async)]
fn read_pdf(state: State<AppState>) -> CmdResult<Response> {
    let pdf = {
        let guard = state.project.lock_safe();
        guard.as_ref().ok_or("No project is open.")?.pdf_path()
    };
    let bytes =
        std::fs::read(&pdf).map_err(|_| "No PDF yet. Build the project first.".to_string())?;
    Ok(Response::new(bytes))
}

/// True when the open project's PDF is up to date, so opening it needn't build.
#[tauri::command(async)]
fn pdf_is_current(state: State<AppState>) -> CmdResult<bool> {
    // The build's own check: compared with when the last good build began, so an edit saved while it ran isn't taken as built.
    let project = state
        .project
        .lock_safe()
        .clone()
        .ok_or("No project is open.")?;
    Ok(lemmata_tex::up_to_date(&project))
}

fn synctex_and_pdf(state: &State<AppState>) -> CmdResult<(PathBuf, PathBuf, PathBuf)> {
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

#[tauri::command(async)]
fn synctex_forward(
    state: State<AppState>,
    path: String,
    line: u32,
    column: u32,
) -> CmdResult<Vec<lemmata_synctex::PdfBox>> {
    let (synctex, pdf, _) = synctex_and_pdf(&state)?;
    let abs = resolve(&state, &path)?;
    lemmata_synctex::forward(&synctex, &pdf, &abs, line, column).map_err(err)
}

#[derive(Serialize)]
struct SourceHit {
    /// Relative to the project root, or absolute when the source is outside it.
    path: String,
    line: u32,
    inside_project: bool,
}

#[tauri::command(async)]
fn synctex_backward(
    state: State<AppState>,
    page: u32,
    x: f64,
    y: f64,
) -> CmdResult<Option<SourceHit>> {
    let (synctex, pdf, root) = synctex_and_pdf(&state)?;
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

fn templates_dir(app: &tauri::AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().resource_dir().map_err(err)?.join("templates"))
}

/// Templates the user saved from their own projects.
fn user_templates_dir(app: &tauri::AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_data_dir().map_err(err)?.join("templates"))
}

/// The template collection's templates, kept so they work offline once seen.
fn online_templates_dir(app: &tauri::AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_cache_dir().map_err(err)?.join("templates"))
}

#[tauri::command(async)]
fn list_templates(app: tauri::AppHandle) -> CmdResult<Vec<projects::Template>> {
    Ok(projects::list_all(
        &templates_dir(&app)?,
        &online_templates_dir(&app)?,
        &user_templates_dir(&app)?,
    ))
}

/// Brings the template collection's templates up to date; list_templates then includes them.
#[tauri::command]
async fn refresh_templates(app: tauri::AppHandle) -> CmdResult<usize> {
    online_templates::refresh(&online_templates_dir(&app)?).await
}

/// Saves a project (the open one, or one from the library) as a template for new projects.
#[tauri::command(async)]
fn save_as_template(
    app: tauri::AppHandle,
    path: String,
    name: String,
    description: String,
) -> CmdResult<String> {
    let p = Project::open(&path).map_err(err)?;
    projects::save_as_template(
        &user_templates_dir(&app)?,
        &p.root,
        &p.config.outdir,
        &name,
        &description,
    )
}

#[tauri::command(async)]
fn delete_template(app: tauri::AppHandle, id: String) -> CmdResult<()> {
    projects::delete_user_template(&user_templates_dir(&app)?, &id)
}

#[tauri::command(async)]
fn create_project(
    app: tauri::AppHandle,
    state: State<AppState>,
    template: String,
    parent: String,
    name: String,
) -> CmdResult<PathBuf> {
    let dir = projects::create_from_template(
        &templates_dir(&app)?,
        &online_templates_dir(&app)?,
        &user_templates_dir(&app)?,
        &template,
        Path::new(&parent),
        &name,
    )?;
    // Only Tectonic can build here, so the project should say so.
    let detection = match state.detection.lock_safe().clone() {
        Some(d) => d,
        None => first_or_fresh_detection(&state, false),
    };
    if !detection.tex_ready && detection.tectonic.is_some() {
        projects::use_tectonic(&dir)?;
    }
    Ok(dir)
}

/// Starts a folder that has no main document: writes a minimal main.tex, never over an existing one.
#[tauri::command(async)]
fn create_main(dir: String) -> CmdResult<()> {
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
#[tauri::command(async)]
fn create_example(app: tauri::AppHandle) -> CmdResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(err)?
        .join(projects::EXAMPLE);
    projects::ensure_example(&templates_dir(&app)?, &dir)
}

/// Stops the build in progress, if any.
#[tauri::command(async)]
fn stop_build(state: State<AppState>) {
    cancel_running(&state);
}

/// The full TeX log of the last build, for the "Raw log" view.
#[tauri::command(async)]
fn read_log(state: State<AppState>) -> CmdResult<String> {
    let path = state
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
#[tauri::command(async)]
fn add_files(state: State<AppState>, paths: Vec<String>, folder: String) -> CmdResult<Vec<String>> {
    let root = state
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

#[tauri::command(async)]
fn import_overleaf_zip(zip: String, parent: String, name: String) -> CmdResult<Vec<PathBuf>> {
    projects::import_zip(Path::new(&zip), Path::new(&parent), &name)
}

#[tauri::command(async)]
fn inspect_zip(zip: String) -> CmdResult<projects::ZipInfo> {
    projects::inspect_zip(Path::new(&zip))
}

/// Undoes `encodeURIComponent` (header values must be ASCII).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(h) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(h);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn thumbnail_path(app: &tauri::AppHandle, root: &str) -> CmdResult<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut h);
    let dir = app.path().app_cache_dir().map_err(err)?.join("thumbnails");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.join(format!("{:016x}.png", h.finish())))
}

/// Stores page 1 of a project's PDF, rendered by the frontend, for the library.
#[tauri::command(async)]
fn save_thumbnail(app: tauri::AppHandle, request: tauri::ipc::Request) -> CmdResult<()> {
    let root = request
        .headers()
        .get("x-project-root")
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .ok_or("Missing project root.")?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected PNG bytes.".into());
    };
    std::fs::write(thumbnail_path(&app, &root)?, bytes).map_err(err)
}

#[tauri::command(async)]
fn load_thumbnail(app: tauri::AppHandle, root: String) -> CmdResult<Response> {
    let bytes =
        std::fs::read(thumbnail_path(&app, &root)?).map_err(|_| "No thumbnail yet.".to_string())?;
    Ok(Response::new(bytes))
}

/// Moves a whole project folder to the system trash, from the library.
#[tauri::command(async)]
fn trash_project(app: tauri::AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    let dir = projects::deletable_project(Path::new(&path), app.path().home_dir().ok().as_deref())?;
    if state
        .project
        .lock_safe()
        .as_ref()
        .is_some_and(|p| p.root == dir)
    {
        cancel_running(&state);
        *state.watcher.lock_safe() = None;
        *state.project.lock_safe() = None;
    }
    // Windows refuses while a process still has the folder open; the language server was only just told to quit.
    let mut tries = 0;
    while let Err(e) = trash::delete(&dir) {
        tries += 1;
        if tries == 5 {
            return Err(format!(
                "Could not move {} to the trash: {e}",
                dir.display()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    if let Ok(thumb) = thumbnail_path(&app, &path) {
        let _ = std::fs::remove_file(thumb);
    }
    Ok(())
}

/// Adds `\usepackage{name}` to the main file's preamble, after the last
/// \usepackage (or after \documentclass).
#[tauri::command(async)]
fn add_package(state: State<AppState>, package: String) -> CmdResult<()> {
    if !package
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Not a package name.".into());
    }
    let main = state
        .project
        .lock_safe()
        .as_ref()
        .map(|p| p.main_abs())
        .ok_or("No project is open.")?;
    let text = std::fs::read_to_string(&main).map_err(err)?;
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
    std::fs::write(&main, joined).map_err(err)
}

#[tauri::command(async)]
fn outline(state: State<AppState>) -> CmdResult<lemmata_project::Outline> {
    Ok(lemmata_project::outline(&project(&state)?))
}

#[tauri::command(async)]
fn labels(state: State<AppState>) -> CmdResult<Vec<lemmata_project::extras::Label>> {
    Ok(lemmata_project::extras::labels(&project(&state)?))
}

#[tauri::command(async)]
fn word_count(state: State<AppState>) -> CmdResult<lemmata_project::extras::Counts> {
    Ok(lemmata_project::extras::counts(&project(&state)?))
}

#[tauri::command(async)]
fn package_names() -> Vec<String> {
    lemmata_pkgmap::package_names().to_vec()
}

/// Starts (or restarts) texlab for the open project. Returns the server's
/// generation, which tags every `lsp-message` event it sends.
#[tauri::command]
async fn lsp_start(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<u64> {
    let root = state
        .project
        .lock_safe()
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or("No project is open.")?;
    let bin = state
        .detection
        .lock_safe()
        .as_ref()
        .and_then(|d| d.bin_dir.clone());
    let generation = state
        .lsp_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    let mut slot = state.lsp.lock().await;
    if let Some(old) = slot.take() {
        old.stop().await;
    }
    *slot = Some(lsp::start(app, &root, bin.as_deref(), generation).await?);
    Ok(generation)
}

/// Stops texlab when its project closes, unless a newer one has started since.
#[tauri::command]
async fn lsp_stop(state: State<'_, AppState>, generation: u64) -> CmdResult<()> {
    let mut slot = state.lsp.lock().await;
    if slot.as_ref().is_some_and(|s| s.generation == generation)
        && let Some(server) = slot.take()
    {
        server.stop().await;
    }
    Ok(())
}

#[tauri::command]
async fn lsp_send(state: State<'_, AppState>, generation: u64, body: String) -> CmdResult<()> {
    let slot = state.lsp.lock().await;
    match slot.as_ref() {
        Some(server) if server.generation == generation => server.send(&body).await,
        _ => Err("The language server was restarted.".into()),
    }
}

/// What to install in a terminal. The command itself is always built here
/// from Lemmata's own data, never taken from the webview.
#[derive(serde::Deserialize)]
#[serde(tag = "what", rename_all = "lowercase")]
enum InstallRequest {
    /// The onboarding command for a platform ("recommended" or "full").
    Tex { platform: String, full: bool },
    /// The package providing a missing file, e.g. `pgfplots.sty`.
    Package { file: String },
}

#[tauri::command(async)]
fn open_install_terminal(state: State<AppState>, request: InstallRequest) -> CmdResult<()> {
    let command = match request {
        InstallRequest::Tex { platform, full } => {
            let all = lemmata_tex::os::all_instructions();
            let i = all.get(&platform).ok_or("Unknown platform.")?;
            let cmd = if full { &i.full } else { &i.recommended };
            if cmd.starts_with("Install ") {
                return Err(
                    "There's no single command for this platform. Follow the link instead.".into(),
                );
            }
            cmd.clone()
        }
        InstallRequest::Package { file } => {
            let detection = state
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // A second launch focuses the running app instead of fighting over its settings. Not in debug
    // builds, so a test build can run next to an installed Lemmata.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
        // A second launch with a file or folder opens it in the running app (and focuses it).
        open_with::on_second_launch(app, &args, &cwd);
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .manage(AppState::default())
        .setup(|app| {
            // lemmata:// links. macOS takes the scheme from the bundle; Linux (AppImage) and Windows
            // need it registered at run time. Release builds only: a test build must not take them over.
            #[cfg(all(not(debug_assertions), any(target_os = "linux", windows)))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }
            #[cfg(debug_assertions)]
            automation::start(app.handle());
            tectonic::init(app.handle());
            start_detection(app.handle());
            open_with::init(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(debug_assertions)]
            automation::automation_reply,
            tex_status,
            pdf_is_current,
            open_project,
            project_info_cmd,
            watching,
            open_pdf_externally,
            open_url,
            open_with::take_open_request,
            history::history_latexdiff,
            open_project_file,
            tectonic::tectonic_status,
            tectonic::tectonic_install,
            git::git_overview,
            git::git_init,
            git::git_stage,
            git::git_unstage,
            git::git_stage_all,
            git::git_commit,
            git::git_fetch,
            git::git_pull,
            git::git_push,
            git::git_branches,
            git::git_switch,
            git::git_create_branch,
            git::git_log,
            git::git_clone,
            set_config,
            read_text,
            write_text,
            build,
            read_pdf,
            synctex_forward,
            synctex_backward,
            open_install_terminal,
            lsp_start,
            lsp_stop,
            lsp_send,
            outline,
            clean_build,
            files::copy_path,
            files::read_binary,
            labels,
            word_count,
            package_names,
            labels,
            word_count,
            package_names,
            list_templates,
            refresh_templates,
            save_as_template,
            delete_template,
            create_project,
            import_overleaf_zip,
            inspect_zip,
            create_example,
            create_main,
            stop_build,
            read_log,
            add_files,
            save_thumbnail,
            load_thumbnail,
            trash_project,
            history::history_status,
            history::git_status,
            history::history_enable,
            history::history_snapshot,
            history::history_list,
            history::history_file,
            history::history_restore,
            history::history_restore_all,
            history::history_compare,
            history::history_relabel,
            spell::spell_set_language,
            spell::spell_check,
            spell::spell_suggest,
            spell::spell_add,
            spell::spell_available,
            spell::spell_download,
            updates::update_check,
            updates::update_install,
            bib::bib_entries,
            bib::bib_search_online,
            bib::bib_library,
            bib::bib_remove_entry,
            bib::bib_rename_key,
            tools::todos,
            tools::format_tex,
            grammar::grammar_check,
            grammar::grammar_status,
            bib::bib_add_reference,
            bib::zotero_status,
            bib::zotero_pick,
            bib::zotero_sync,
            bib::zotero_link,
            figures::import_figure,
            figures::save_pasted_image,
            figures::has_package,
            add_package,
            files::search_project,
            files::replace_in_files,
            files::create_path,
            files::trash_path,
            files::find_references,
            files::rename_path,
            files::move_paths,
            files::trash_paths,
            files::restore_trash,
            files::trash_batches,
            files::purge_trash,
            files::unused_files,
            files::symbol_defs,
            files::symbol_uses,
            files::rename_symbol,
            export::export_pdf,
            export::export_arxiv,
            export::export_zip,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lemmata")
        .run(|app, event| {
            // Quitting mid-build must not leave latexmk, pdflatex and biber running.
            // macOS hands files over as an event, not as arguments.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                open_with::on_opened(app, urls);
            }
            if let tauri::RunEvent::Exit = event {
                cancel_running(&app.state::<AppState>());
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_encoded_paths() {
        assert_eq!(
            super::percent_decode("%2Fhome%2Fa%2FMa%C3%AEtrise%20x"),
            "/home/a/Maîtrise x"
        );
    }
}
