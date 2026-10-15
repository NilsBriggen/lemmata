//! Bringing images into the project: dropped files are copied, pasted
//! screenshots are saved, both into the project's figures folder.

use crate::{AppState, CmdResult, err, project};
use std::path::{Path, PathBuf};
use tauri::State;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "pdf", "eps", "svg"];

/// A file name not yet used in `dir`: `name.ext`, `name-2.ext`, ...
fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let p = dir.join(name);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

/// Turns "My Plot (final).PNG" into "my-plot-final", safe for \includegraphics.
fn slug(stem: &str) -> String {
    let s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() { "figure".into() } else { s }
}

fn rel(p: &lemmata_project::Project, abs: &Path) -> String {
    abs.strip_prefix(&p.root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Copies an image into the figures folder, unless it's already inside the
/// project. Returns its project-relative path.
#[tauri::command(async)]
pub fn import_figure(state: State<AppState>, path: String) -> CmdResult<String> {
    let p = project(&state)?;
    let src = PathBuf::from(&path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        return Err(format!(
            "{} isn't an image LaTeX can include (PNG, JPEG, PDF, EPS or SVG).",
            src.display()
        ));
    }
    if let Ok(canon) = src.canonicalize()
        && canon.starts_with(&p.root)
    {
        return Ok(rel(&p, &canon));
    }
    let dir = p.root.join(&p.config.figures_dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("figure");
    let dest = unique(&dir, &slug(stem), &ext);
    std::fs::copy(&src, &dest).map_err(|e| format!("Could not copy the image: {e}"))?;
    Ok(rel(&p, &dest))
}

/// Saves a pasted image (PNG bytes in the request body).
#[tauri::command(async)]
pub fn save_pasted_image(
    state: State<AppState>,
    request: tauri::ipc::Request,
) -> CmdResult<String> {
    let p = project(&state)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected image bytes.".into());
    };
    if !bytes.starts_with(b"\x89PNG") {
        return Err("Only PNG screenshots can be pasted.".into());
    }
    let dir = p.root.join(&p.config.figures_dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let dest = unique(&dir, "pasted-figure", "png");
    std::fs::write(&dest, bytes).map_err(err)?;
    Ok(rel(&p, &dest))
}

/// Whether the main file loads `package` (directly or via a list).
#[tauri::command(async)]
pub fn has_package(state: State<AppState>, package: String) -> CmdResult<bool> {
    let p = project(&state)?;
    let text = std::fs::read_to_string(p.main_abs()).map_err(err)?;
    Ok(text.lines().any(|l| {
        let code = l.split('%').next().unwrap_or("");
        code.contains("\\usepackage")
            && code
                .split(['{', '}', ','])
                .any(|part| part.trim() == package)
            || code.contains("\\documentclass") && code.contains("beamer") && package == "graphicx"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_unique_names() {
        assert_eq!(slug("My Plot (final)"), "my-plot-final");
        assert_eq!(slug("ü"), "figure");
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), "").unwrap();
        assert!(unique(d.path(), "a", "png").ends_with("a-2.png"));
        assert!(unique(d.path(), "b", "png").ends_with("b.png"));
    }
}
