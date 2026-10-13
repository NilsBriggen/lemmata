//! Spell-check commands. The dictionary loads in milliseconds, so it's
//! loaded on demand when the language changes.

use crate::{AppState, CmdResult};
use lemmata_spell::Speller;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn spell_set_language(
    app: AppHandle,
    state: State<AppState>,
    lang: String,
    words: Vec<String>,
) -> CmdResult<()> {
    let mut slot = state.speller.lock().unwrap();
    if lang == "off" {
        *slot = None;
        return Ok(());
    }
    let dir = app
        .path()
        .resource_dir()
        .map_err(crate::err)?
        .join("dictionaries");
    let mut speller = Speller::load(&dir, &lang).map_err(crate::err)?;
    for w in &words {
        speller.add(w);
    }
    *slot = Some(speller);
    Ok(())
}

/// The words from `words` that are misspelled.
#[tauri::command]
pub fn spell_check(state: State<AppState>, words: Vec<String>) -> Vec<String> {
    let slot = state.speller.lock().unwrap();
    let Some(speller) = slot.as_ref() else {
        return Vec::new();
    };
    words.into_iter().filter(|w| !speller.check(w)).collect()
}

#[tauri::command]
pub fn spell_suggest(state: State<AppState>, word: String) -> Vec<String> {
    let slot = state.speller.lock().unwrap();
    slot.as_ref().map(|s| s.suggest(&word)).unwrap_or_default()
}

#[tauri::command]
pub fn spell_add(state: State<AppState>, word: String) {
    if let Some(s) = state.speller.lock().unwrap().as_mut() {
        s.add(&word);
    }
}
