//! Update checks against the manifest on lemmata.briggen.dev, which
//! redirects to the latest GitHub release (see docs/website-handoff.md).

use crate::{AppState, CmdResult, err};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

const BASE: &str = "https://lemmata.briggen.dev/update";

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
}

#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<UpdateInfo>> {
    let url = format!("{BASE}/latest.json").parse().map_err(err)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(err)?
        .build()
        .map_err(err)?;
    let update = updater.check().await.map_err(err)?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
    });
    *state.update.lock().await = update;
    Ok(info)
}

/// Downloads and installs the update found by `update_check`, emitting
/// `update-progress` with the fraction done. The app restarts afterwards
/// when the user chooses to.
#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let update = state
        .update
        .lock()
        .await
        .take()
        .ok_or("No update is waiting.")?;
    let mut done: u64 = 0;
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                if let Some(total) = total {
                    let _ = app.emit("update-progress", done as f64 / total as f64);
                }
            },
            || {},
        )
        .await
        .map_err(err)
}
