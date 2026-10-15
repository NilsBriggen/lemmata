//! Tauri commands the frontend calls. All file paths crossing the IPC
//! boundary are relative to the open project's root.

#[cfg(debug_assertions)]
mod automation;
mod bib;
mod export;
mod figures;
mod files;
mod history;
mod lsp;
mod projects;
mod spell;
mod terminal;
mod updates;

use lemmata_project::{Config, Entry, Project};
use lemmata_tex::detect::Detection;
use lemmata_tex::os::{Instructions, OsInfo};
use lemmata_tex::{BuildOptions, Outcome};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

#[tauri::command(async)]
fn tex_status(state: State<AppState>, extra_dir: Option<String>) -> TexStatus {
    if let Some(dir) = extra_dir {
        let mut extra = state.extra_tex_dirs.lock_safe();
        let dir = PathBuf::from(dir);
        if !extra.contains(&dir) {
            extra.insert(0, dir);
        }
    }
    let extra = state.extra_tex_dirs.lock_safe().clone();
    let detection = lemmata_tex::detect(&extra);
    *state.detection.lock_safe() = Some(detection.clone());
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
    let detection = match state.detection.lock_safe().clone() {
        Some(d) => d,
        None => lemmata_tex::detect(&state.extra_tex_dirs.lock_safe()),
    };
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
    let outcome = lemmata_tex::build(&project, &opts, rx).await.map_err(err)?;
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

fn synctex_and_pdf(state: &State<AppState>) -> CmdResult<(PathBuf, PathBuf, PathBuf)> {
    let synctex = state
        .detection
        .lock_safe()
        .as_ref()
        .and_then(|d| d.tool("synctex").map(Path::to_path_buf))
        .ok_or("synctex was not found.")?;
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
    let file = loc.file.canonicalize().unwrap_or(loc.file);
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

#[tauri::command(async)]
fn list_templates(app: tauri::AppHandle) -> CmdResult<Vec<projects::Template>> {
    Ok(projects::list_all(
        &templates_dir(&app)?,
        &user_templates_dir(&app)?,
    ))
}

/// Saves the open project as a template for new projects.
#[tauri::command(async)]
fn save_as_template(
    app: tauri::AppHandle,
    state: State<AppState>,
    name: String,
    description: String,
) -> CmdResult<String> {
    let p = state
        .project
        .lock_safe()
        .clone()
        .ok_or("No project is open.")?;
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
    template: String,
    parent: String,
    name: String,
) -> CmdResult<PathBuf> {
    projects::create_from_template(
        &templates_dir(&app)?,
        &user_templates_dir(&app)?,
        &template,
        Path::new(&parent),
        &name,
    )
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
    trash::delete(&dir)
        .map_err(|e| format!("Could not move {} to the trash: {e}", dir.display()))?;
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
        old.stop();
    }
    *slot = Some(lsp::start(app, &root, bin.as_deref(), generation).await?);
    Ok(generation)
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
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            #[cfg(debug_assertions)]
            automation::start(app.handle());
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(debug_assertions)]
            automation::automation_reply,
            tex_status,
            open_project,
            project_info_cmd,
            watching,
            set_config,
            read_text,
            write_text,
            build,
            read_pdf,
            synctex_forward,
            synctex_backward,
            open_install_terminal,
            lsp_start,
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
            updates::update_check,
            updates::update_install,
            bib::bib_entries,
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
