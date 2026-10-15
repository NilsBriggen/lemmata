//! Citation picker and "Add reference" commands.

use crate::{AppState, CmdResult, project};
use lemmata_bib::Entry;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct Bibliography {
    files: Vec<String>,
    entries: Vec<Entry>,
}

#[tauri::command(async)]
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

#[tauri::command]
pub async fn zotero_status() -> lemmata_bib::zotero::Status {
    lemmata_bib::zotero::status().await
}

/// Opens Zotero's own citation picker; returns the chosen keys.
#[tauri::command]
pub async fn zotero_pick() -> CmdResult<Vec<String>> {
    lemmata_bib::zotero::pick().await.map_err(crate::err)
}

#[derive(Serialize)]
pub struct ZoteroSync {
    /// Entries written to zotero.bib.
    written: usize,
    /// Cited keys neither in the project's other .bib files nor in Zotero.
    missing: Vec<String>,
    /// Whether the document already loads zotero.bib.
    linked: bool,
}

pub const ZOTERO_BIB: &str = "zotero.bib";

/// Writes zotero.bib with every key the document cites that isn't already
/// in another .bib file of the project, exported from Zotero.
#[tauri::command]
pub async fn zotero_sync(state: State<'_, AppState>) -> CmdResult<ZoteroSync> {
    let p = project(&state)?;
    let files = lemmata_bib::bib_files(&p);
    let others: Vec<String> = files
        .iter()
        .filter(|f| f.as_str() != ZOTERO_BIB)
        .cloned()
        .collect();
    let known: std::collections::HashSet<String> = lemmata_bib::entries(&p.root, &others)
        .into_iter()
        .map(|e| e.key)
        .collect();
    let wanted: Vec<String> = lemmata_bib::cited_keys(&p)
        .into_iter()
        .filter(|k| !known.contains(k))
        .collect();
    let (text, missing) = if wanted.is_empty() {
        (String::new(), Vec::new())
    } else {
        lemmata_bib::zotero::export(&wanted)
            .await
            .map_err(crate::err)?
    };
    let header = "% Exported from Zotero by Lemmata. Changes here are overwritten on the next sync;\n% edit the references in Zotero instead.\n\n";
    std::fs::write(
        p.root.join(ZOTERO_BIB),
        format!("{header}{}", text.trim_start()),
    )
    .map_err(crate::err)?;
    Ok(ZoteroSync {
        written: wanted.len() - missing.len(),
        missing,
        linked: lemmata_bib::bib_files(&p).iter().any(|f| f == ZOTERO_BIB),
    })
}

/// Adds `\addbibresource{zotero.bib}` (biblatex) or extends `\bibliography{}`.
#[tauri::command(async)]
pub fn zotero_link(state: State<AppState>) -> CmdResult<()> {
    let p = project(&state)?;
    let main = p.main_abs();
    let text = std::fs::read_to_string(&main).map_err(crate::err)?;
    let new = if let Some(pos) = text.find("\\bibliography{") {
        let close = text[pos..].find('}').ok_or("Unclosed \\bibliography{.")? + pos;
        format!("{},zotero{}", &text[..close], &text[close..])
    } else if let Some(pos) = text.find("\\addbibresource") {
        let line_end = text[pos..].find('\n').map_or(text.len(), |e| pos + e);
        format!(
            "{}\n\\addbibresource{{{ZOTERO_BIB}}}{}",
            &text[..line_end],
            &text[line_end..]
        )
    } else {
        return Err("The document has no bibliography yet. Add \\addbibresource{zotero.bib} (biblatex) or \\bibliography{zotero} yourself.".into());
    };
    std::fs::write(&main, new).map_err(crate::err)
}
