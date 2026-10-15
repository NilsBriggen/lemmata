//! Git panel commands: thin async wrappers around lemmata-git for the open project.
//! Each runs on a blocking thread, so a slow network never freezes the window.

use crate::{AppState, CmdResult, LockSafe, err};
use lemmata_git as g;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// Two git commands on one repository at once fail on its index lock.
static GIT: Mutex<()> = Mutex::new(());

fn root(state: &State<AppState>) -> CmdResult<PathBuf> {
    let guard = state.project.lock_safe();
    Ok(guard.as_ref().ok_or("No project is open.")?.root.clone())
}

/// Runs `f` on the project's repository, one git operation at a time.
async fn with<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(PathBuf) -> g::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let root = root(state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _one_at_a_time = GIT.lock_safe();
        f(root).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn git_overview(state: State<'_, AppState>) -> CmdResult<g::Overview> {
    with(&state, |r| Ok(g::overview(&r))).await
}

#[tauri::command]
pub async fn git_init(state: State<'_, AppState>) -> CmdResult<()> {
    with(&state, |r| g::init(&r)).await
}

#[tauri::command]
pub async fn git_stage(state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<()> {
    with(&state, move |r| g::stage(&r, &paths)).await
}

#[tauri::command]
pub async fn git_unstage(state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<()> {
    with(&state, move |r| g::unstage(&r, &paths)).await
}

#[tauri::command]
pub async fn git_stage_all(state: State<'_, AppState>) -> CmdResult<()> {
    with(&state, |r| g::stage_all(&r)).await
}

#[tauri::command]
pub async fn git_commit(state: State<'_, AppState>, message: String) -> CmdResult<()> {
    with(&state, move |r| g::commit(&r, &message)).await
}

#[tauri::command]
pub async fn git_fetch(state: State<'_, AppState>) -> CmdResult<String> {
    with(&state, |r| g::fetch(&r)).await
}

#[tauri::command]
pub async fn git_pull(state: State<'_, AppState>) -> CmdResult<String> {
    with(&state, |r| g::pull(&r)).await
}

#[tauri::command]
pub async fn git_push(state: State<'_, AppState>) -> CmdResult<String> {
    with(&state, |r| g::push(&r)).await
}

#[tauri::command]
pub async fn git_branches(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    with(&state, |r| g::branches(&r)).await
}

#[tauri::command]
pub async fn git_switch(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    with(&state, move |r| g::switch_branch(&r, &name)).await
}

#[tauri::command]
pub async fn git_create_branch(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    with(&state, move |r| g::create_branch(&r, &name)).await
}

#[tauri::command]
pub async fn git_log(state: State<'_, AppState>) -> CmdResult<Vec<g::Commit>> {
    with(&state, |r| g::log(&r)).await
}

/// Clones into `parent/name` (the name defaults to the URL's last part) and returns the new folder.
/// Needs no open project: it runs from the start screen.
#[tauri::command]
pub async fn git_clone(url: String, parent: String, name: String) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || g::clone(&url, parent.as_ref(), &name))
        .await
        .map_err(err)?
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}
