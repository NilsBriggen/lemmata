//! History commands: thin wrappers around lemmata-history for the open project.

use crate::{AppState, CmdResult, err};
use lemmata_history::{Error, History, Snapshot};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum HistoryStatus {
    On,
    /// The folder isn't a git repository; history can be turned on.
    NotARepo,
    /// Git isn't installed.
    NoGit {
        message: String,
    },
}

fn root(state: &State<AppState>) -> CmdResult<PathBuf> {
    let guard = state.project.lock().unwrap();
    Ok(guard.as_ref().ok_or("No project is open.")?.root.clone())
}

fn open(state: &State<AppState>) -> CmdResult<History> {
    History::open(&root(state)?).map_err(err)
}

#[tauri::command]
pub fn history_status(state: State<AppState>) -> CmdResult<HistoryStatus> {
    Ok(match History::open(&root(&state)?) {
        Ok(_) => HistoryStatus::On,
        Err(Error::NotARepo(_)) => HistoryStatus::NotARepo,
        Err(e @ Error::NoGit) => HistoryStatus::NoGit {
            message: e.to_string(),
        },
        Err(e) => return Err(e.to_string()),
    })
}

#[tauri::command]
pub fn history_enable(state: State<AppState>) -> CmdResult<()> {
    let h = History::init(&root(&state)?).map_err(err)?;
    h.snapshot("History turned on").map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn history_snapshot(state: State<AppState>, label: String) -> CmdResult<Option<String>> {
    open(&state)?.snapshot(&label).map_err(err)
}

#[tauri::command]
pub fn history_list(state: State<AppState>) -> CmdResult<Vec<Snapshot>> {
    open(&state)?.list(200).map_err(err)
}

#[tauri::command]
pub fn history_file(state: State<AppState>, id: String, path: String) -> CmdResult<Option<String>> {
    open(&state)?.file_at(&id, &path).map_err(err)
}

#[tauri::command]
pub fn history_restore(state: State<AppState>, id: String, path: String) -> CmdResult<()> {
    open(&state)?.restore_file(&id, &path).map_err(err)
}
