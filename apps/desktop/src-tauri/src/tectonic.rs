//! The optional Tectonic engine: a TeX compiler Lemmata downloads on request,
//! for people without TeX Live or MiKTeX.

use crate::LockSafe;
use lemmata_tex::tectonic as tex;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize)]
pub(crate) struct TectonicStatus {
    installed: bool,
    path: Option<String>,
    version: Option<String>,
    /// False on platforms the app has no pinned download for.
    supported: bool,
    /// Size of the download, in megabytes.
    download_mb: Option<u64>,
}

fn install_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(crate::err)?
        .join("tectonic"))
}

/// Tells detection where the downloaded binary lives. Call in `setup`, before detection starts.
pub(crate) fn init(app: &tauri::AppHandle) {
    if let Ok(dir) = install_dir(app) {
        tex::set_install_dir(dir);
    }
}

fn status() -> TectonicStatus {
    let path = tex::find();
    TectonicStatus {
        installed: path.is_some(),
        version: path.as_deref().and_then(tex::version),
        path: path.map(|p| p.to_string_lossy().into_owned()),
        supported: tex::release().is_some(),
        download_mb: tex::release().map(|r| r.size.div_ceil(1_000_000)),
    }
}

#[tauri::command(async)]
pub(crate) fn tectonic_status() -> TectonicStatus {
    status()
}

/// Downloads and verifies the pinned Tectonic. Blocks until done (tens of MB).
#[tauri::command(async)]
pub(crate) fn tectonic_install(app: tauri::AppHandle) -> Result<TectonicStatus, String> {
    let dir = install_dir(&app)?;
    tex::set_install_dir(dir.clone());
    tex::install(&dir).map_err(crate::err)?;
    // Builds use the cached detection: refresh it so they find Tectonic now.
    let state = app.state::<crate::AppState>();
    *state.startup_detection.lock_safe() = None;
    crate::run_detection(&state);
    Ok(status())
}
