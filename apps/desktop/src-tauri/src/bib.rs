//! Citation picker and "Add reference" commands.

use crate::{AppState, CmdResult};
use lemmata_bib::Entry;
use serde::Serialize;
use tauri::State;

fn project(state: &State<AppState>) -> CmdResult<lemmata_project::Project> {
    state
        .project
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No project is open.".into())
}

#[derive(Serialize)]
pub struct Bibliography {
    files: Vec<String>,
    entries: Vec<Entry>,
}

#[tauri::command]
pub fn bib_entries(state: State<AppState>) -> CmdResult<Bibliography> {
    let p = project(&state)?;
    let files = lemmata_bib::bib_files(&p);
    let entries = lemmata_bib::entries(&p.root, &files);
    Ok(Bibliography { files, entries })
}

#[derive(Serialize)]
pub struct Added {
    key: String,
    file: String,
}

/// Looks up a DOI or arXiv ID on doi.org and appends it to `file` (a .bib
/// file in the project; created if missing). Network access happens only
/// here, when the user asks for it.
#[tauri::command]
pub async fn bib_add_reference(
    state: State<'_, AppState>,
    id: String,
    file: String,
) -> CmdResult<Added> {
    let p = project(&state)?;
    if !file.ends_with(".bib") || file.contains("..") || std::path::Path::new(&file).is_absolute() {
        return Err("Choose a .bib file inside the project.".into());
    }
    let doi = lemmata_bib::to_doi(&id).ok_or("That doesn't look like a DOI or an arXiv ID.")?;
    let bibtex = lemmata_bib::fetch_bibtex(&doi).await.map_err(crate::err)?;
    let existing: Vec<String> = lemmata_bib::entries(&p.root, &lemmata_bib::bib_files(&p))
        .into_iter()
        .map(|e| e.key)
        .collect();
    let (key, text) = lemmata_bib::prepare_entry(&bibtex, &existing).map_err(crate::err)?;
    lemmata_bib::append(&p.root.join(&file), &text).map_err(crate::err)?;
    Ok(Added { key, file })
}
