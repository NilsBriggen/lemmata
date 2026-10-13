//! Tauri commands the frontend calls. All file paths crossing the IPC
//! boundary are relative to the open project's root.

use lemmata_project::{Config, Entry, Project};
use lemmata_tex::detect::Detection;
use lemmata_tex::os::{Instructions, OsInfo};
use lemmata_tex::{BuildOptions, Outcome};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::Response;
use tauri::{Emitter, State};
use tokio::sync::oneshot;

#[derive(Default)]
struct AppState {
    detection: Mutex<Option<Detection>>,
    /// Folders the user pointed us at in onboarding.
    extra_tex_dirs: Mutex<Vec<PathBuf>>,
    project: Mutex<Option<Project>>,
    /// Sending on this cancels the build in progress.
    cancel_build: Mutex<Option<oneshot::Sender<()>>>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
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

#[tauri::command]
fn tex_status(state: State<AppState>, extra_dir: Option<String>) -> TexStatus {
    if let Some(dir) = extra_dir {
        let mut extra = state.extra_tex_dirs.lock().unwrap();
        let dir = PathBuf::from(dir);
        if !extra.contains(&dir) {
            extra.insert(0, dir);
        }
    }
    let extra = state.extra_tex_dirs.lock().unwrap().clone();
    let detection = lemmata_tex::detect(&extra);
    *state.detection.lock().unwrap() = Some(detection.clone());
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

#[tauri::command]
fn open_project(state: State<AppState>, path: String) -> CmdResult<ProjectInfo> {
    cancel_running(&state);
    let project = Project::open(&path).map_err(err)?;
    let info = project_info(&project);
    *state.project.lock().unwrap() = Some(project);
    Ok(info)
}

#[tauri::command]
fn project_info_cmd(state: State<AppState>) -> CmdResult<ProjectInfo> {
    let guard = state.project.lock().unwrap();
    guard
        .as_ref()
        .map(project_info)
        .ok_or_else(|| "No project is open.".into())
}

#[tauri::command]
fn set_config(state: State<AppState>, config: Config) -> CmdResult<ProjectInfo> {
    let mut guard = state.project.lock().unwrap();
    let p = guard.as_mut().ok_or("No project is open.")?;
    if let Some(main) = &config.main {
        p.main = PathBuf::from(main);
    }
    p.config = config;
    p.save_config().map_err(err)?;
    Ok(project_info(p))
}

/// Resolves a project-relative path and refuses anything outside the project.
fn resolve(state: &State<AppState>, rel: &str) -> CmdResult<PathBuf> {
    let guard = state.project.lock().unwrap();
    let root = &guard.as_ref().ok_or("No project is open.")?.root;
    let rel = Path::new(rel);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("{} is outside the project.", rel.display()));
    }
    Ok(root.join(rel))
}

#[tauri::command]
fn read_text(state: State<AppState>, path: String) -> CmdResult<String> {
    let abs = resolve(&state, &path)?;
    std::fs::read_to_string(&abs).map_err(|e| format!("Could not open {path}: {e}"))
}

#[tauri::command]
fn write_text(state: State<AppState>, path: String, content: String) -> CmdResult<()> {
    let abs = resolve(&state, &path)?;
    // Write to a sibling temp file first so a crash never leaves a half-written source file.
    let tmp = abs.with_extension("lemmata-tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("Could not save {path}: {e}"))?;
    std::fs::rename(&tmp, &abs).map_err(|e| format!("Could not save {path}: {e}"))
}

fn cancel_running(state: &State<AppState>) {
    if let Some(tx) = state.cancel_build.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
async fn build(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<Outcome> {
    cancel_running(&state);
    let (tx, rx) = oneshot::channel();
    *state.cancel_build.lock().unwrap() = Some(tx);

    let project = state
        .project
        .lock()
        .unwrap()
        .clone()
        .ok_or("No project is open.")?;
    let detection = match state.detection.lock().unwrap().clone() {
        Some(d) => d,
        None => lemmata_tex::detect(&state.extra_tex_dirs.lock().unwrap()),
    };
    let latexmk = detection
        .tool("latexmk")
        .ok_or("latexmk was not found. Install TeX Live first.")?;

    let _ = app.emit("build-started", ());
    let opts = BuildOptions {
        latexmk,
        bin_dir: detection.bin_dir.as_deref(),
    };
    let outcome = lemmata_tex::build(&project, &opts, rx).await.map_err(err)?;
    Ok(outcome)
}

#[tauri::command]
fn read_pdf(state: State<AppState>) -> CmdResult<Response> {
    let pdf = {
        let guard = state.project.lock().unwrap();
        guard.as_ref().ok_or("No project is open.")?.pdf_path()
    };
    let bytes =
        std::fs::read(&pdf).map_err(|_| "No PDF yet. Build the project first.".to_string())?;
    Ok(Response::new(bytes))
}

fn synctex_and_pdf(state: &State<AppState>) -> CmdResult<(PathBuf, PathBuf, PathBuf)> {
    let synctex = state
        .detection
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|d| d.tool("synctex").map(Path::to_path_buf))
        .ok_or("synctex was not found.")?;
    let guard = state.project.lock().unwrap();
    let p = guard.as_ref().ok_or("No project is open.")?;
    Ok((synctex, p.pdf_path(), p.root.clone()))
}

#[tauri::command]
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

#[tauri::command]
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            tex_status,
            open_project,
            project_info_cmd,
            set_config,
            read_text,
            write_text,
            build,
            read_pdf,
            synctex_forward,
            synctex_backward,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lemmata");
}
