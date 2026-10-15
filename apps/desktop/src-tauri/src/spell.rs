//! Spell-check commands. The dictionary loads in milliseconds, so it's
//! loaded on demand when the language changes. Bundled dictionaries are
//! looked up first, then the ones downloaded into the app data folder.

use crate::{AppState, CmdResult, LockSafe};
use lemmata_spell::{Available, Speller};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn bundled_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app
        .path()
        .resource_dir()
        .map_err(crate::err)?
        .join("dictionaries"))
}

fn downloaded_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(crate::err)?
        .join("dictionaries"))
}

/// Every language with whether it can be used now or needs a download.
#[tauri::command(async)]
pub fn spell_available(app: AppHandle) -> CmdResult<Vec<Available>> {
    Ok(lemmata_spell::available(
        &bundled_dir(&app)?,
        &downloaded_dir(&app)?,
    ))
}

/// Downloads a dictionary (checksum-verified) into the app data folder.
#[tauri::command]
pub async fn spell_download(app: AppHandle, code: String) -> CmdResult<()> {
    lemmata_spell::download(&downloaded_dir(&app)?, &code)
        .await
        .map_err(crate::err)
}

#[tauri::command(async)]
pub fn spell_set_language(
    app: AppHandle,
    state: State<AppState>,
    lang: String,
    words: Vec<String>,
) -> CmdResult<()> {
    let mut slot = state.speller.lock_safe();
    if lang == "off" {
        *slot = None;
        return Ok(());
    }
    let (bundled, downloaded) = (bundled_dir(&app)?, downloaded_dir(&app)?);
    let mut speller = Speller::load(&[&bundled, &downloaded], &lang).map_err(crate::err)?;
    for w in &words {
        speller.add(w);
    }
    *slot = Some(speller);
    Ok(())
}

/// The words from `words` that are misspelled.
#[tauri::command(async)]
pub fn spell_check(state: State<AppState>, words: Vec<String>) -> Vec<String> {
    let slot = state.speller.lock_safe();
    let Some(speller) = slot.as_ref() else {
        return Vec::new();
    };
    words.into_iter().filter(|w| !speller.check(w)).collect()
}

#[tauri::command(async)]
pub fn spell_suggest(state: State<AppState>, word: String) -> Vec<String> {
    let slot = state.speller.lock_safe();
    slot.as_ref().map(|s| s.suggest(&word)).unwrap_or_default()
}

#[tauri::command(async)]
pub fn spell_add(state: State<AppState>, word: String) {
    if let Some(s) = state.speller.lock_safe().as_mut() {
        s.add(&word);
    }
}
