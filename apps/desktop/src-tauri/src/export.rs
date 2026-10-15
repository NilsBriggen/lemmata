//! Exporting the PDF, and a source zip. The zip logic is shared with the
//! `lemmata` command, in `lemmata_project::export`.

use crate::{AppState, CmdResult, project};
use lemmata_project::export;
use std::path::Path;
use tauri::State;

#[tauri::command(async)]
pub fn export_pdf(state: State<AppState>, dest: String) -> CmdResult<()> {
    let p = project(&state)?;
    std::fs::copy(p.pdf_path(), &dest)
        .map_err(|_| "Build the project first; there's no PDF yet.".to_string())?;
    Ok(())
}

/// Zips the whole project (not hidden files or the build folder), for a
/// backup or to share it. With `with_pdf`, the latest PDF is added too.
#[tauri::command(async)]
pub fn export_zip(state: State<AppState>, dest: String, with_pdf: bool) -> CmdResult<usize> {
    export::zip_project(&project(&state)?, Path::new(&dest), with_pdf)
}

/// Zips the files the last build used, plus the .bbl (arXiv doesn't run
/// biber), ready to upload to arXiv. With `flatten`, the main file has its
/// inputs inlined, and the files inlined are left out of the zip.
#[tauri::command(async)]
pub fn export_arxiv(
    state: State<AppState>,
    dest: String,
    strip: bool,
    flatten: Option<bool>,
) -> CmdResult<usize> {
    export::zip_arxiv(
        &project(&state)?,
        Path::new(&dest),
        strip,
        flatten.unwrap_or(false),
    )
}
