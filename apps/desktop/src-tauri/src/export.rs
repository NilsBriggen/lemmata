//! Exporting the PDF, and an arXiv-ready source zip.

use crate::{AppState, CmdResult, err, project};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::State;

#[tauri::command(async)]
pub fn export_pdf(state: State<AppState>, dest: String) -> CmdResult<()> {
    let p = project(&state)?;
    std::fs::copy(p.pdf_path(), &dest)
        .map_err(|_| "Build the project first; there's no PDF yet.".to_string())?;
    Ok(())
}

/// Project files the last build read, from latexmk's `.fls` recorder file.
pub fn used_files(fls: &str, root: &Path, outdir: &str) -> BTreeSet<PathBuf> {
    let pwd = fls
        .lines()
        .find_map(|l| l.strip_prefix("PWD "))
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    fls.lines()
        .filter_map(|l| l.strip_prefix("INPUT "))
        .filter_map(|p| {
            let abs = if Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                pwd.join(p)
            };
            let rel = abs.strip_prefix(root).ok()?;
            // Normalise "./chapters/a.tex" and skip build output.
            let rel: PathBuf = rel
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir))
                .collect();
            let s = rel.to_string_lossy();
            (!s.starts_with(outdir) && !s.starts_with('.') && !s.is_empty()).then_some(rel)
        })
        .collect()
}

/// Removes `%` comments (keeping `\%`), dropping lines that were only a comment.
pub fn strip_comments(tex: &str) -> String {
    let mut out = String::with_capacity(tex.len());
    for line in tex.lines() {
        let b = line.as_bytes();
        let cut = (0..b.len()).find(|&i| b[i] == b'%' && (i == 0 || b[i - 1] != b'\\'));
        match cut {
            Some(0) => continue,
            // Keep the % so TeX still joins the lines as the author intended.
            Some(i) if line[..i].trim().is_empty() => continue,
            Some(i) => {
                out.push_str(&line[..i]);
                out.push('%');
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// Every project file a backup needs: the whole tree except hidden files
/// and the build folder.
pub fn project_files(root: &Path, outdir: &str) -> Vec<String> {
    fn walk(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, out);
            } else {
                out.push(e.path.clone());
            }
        }
    }
    let mut out = Vec::new();
    walk(&lemmata_project::tree(root, outdir), &mut out);
    out
}

/// Zips the whole project (not hidden files or the build folder), for a
/// backup or to share it. With `with_pdf`, the latest PDF is added too.
#[tauri::command(async)]
pub fn export_zip(state: State<AppState>, dest: String, with_pdf: bool) -> CmdResult<usize> {
    let p = project(&state)?;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&dest).map_err(err)?);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    for rel in project_files(&p.root, &p.config.outdir) {
        zip.start_file(&rel, opts).map_err(err)?;
        zip.write_all(&std::fs::read(p.root.join(&rel)).map_err(err)?)
            .map_err(err)?;
        count += 1;
    }
    if with_pdf && let Ok(pdf) = std::fs::read(p.pdf_path()) {
        let name = p
            .pdf_path()
            .file_name()
            .map_or("main.pdf".into(), |n| n.to_string_lossy().into_owned());
        zip.start_file(name, opts).map_err(err)?;
        zip.write_all(&pdf).map_err(err)?;
        count += 1;
    }
    zip.finish().map_err(err)?;
    Ok(count)
}

/// Zips the files the last build used, plus the .bbl (arXiv doesn't run
/// biber), ready to upload to arXiv.
#[tauri::command(async)]
pub fn export_arxiv(state: State<AppState>, dest: String, strip: bool) -> CmdResult<usize> {
    let p = project(&state)?;
    let stem = p
        .main
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main")
        .to_string();
    let outdir = p.outdir_abs();
    let fls = std::fs::read_to_string(outdir.join(format!("{stem}.fls"))).map_err(|_| {
        "Build the project once before exporting, so Lemmata knows which files it uses.".to_string()
    })?;
    let files = used_files(&fls, &p.root, p.config.outdir.trim_start_matches("./"));

    let mut zip = zip::ZipWriter::new(std::fs::File::create(&dest).map_err(err)?);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    for rel in &files {
        let name = rel.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(p.root.join(rel)).map_err(err)?;
        let bytes = if strip && name.ends_with(".tex") {
            strip_comments(&String::from_utf8_lossy(&bytes)).into_bytes()
        } else {
            bytes
        };
        zip.start_file(name, opts).map_err(err)?;
        zip.write_all(&bytes).map_err(err)?;
        count += 1;
    }
    // The bibliography, already formatted, next to the main file.
    let main_dir = p
        .main
        .parent()
        .map(|d| d.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if let Ok(bbl) = std::fs::read(outdir.join(format!("{stem}.bbl"))) {
        let name = if main_dir.is_empty() {
            format!("{stem}.bbl")
        } else {
            format!("{main_dir}/{stem}.bbl")
        };
        zip.start_file(name, opts).map_err(err)?;
        zip.write_all(&bbl).map_err(err)?;
        count += 1;
    }
    zip.finish().map_err(err)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_files_skip_hidden_and_build_output() {
        let d = tempfile::tempdir().unwrap();
        for (f, t) in [
            ("main.tex", "x"),
            ("chapters/a.tex", "y"),
            (".lemmata/build/main.pdf", "z"),
            (".git/HEAD", "h"),
        ] {
            let full = d.path().join(f);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, t).unwrap();
        }
        assert_eq!(
            project_files(d.path(), ".lemmata/build"),
            ["chapters/a.tex", "main.tex"]
        );
    }

    #[test]
    fn collects_project_inputs_from_fls() {
        let fls = "PWD /p/thesis\nINPUT /usr/share/texmf/tex/latex/base/report.cls\nINPUT main.tex\nINPUT .lemmata/build/main.aux\nINPUT ./chapters/one.tex\nINPUT ./chapters/one.tex\nINPUT /p/thesis/figures/a.pdf\nOUTPUT main.pdf\n";
        let got: Vec<_> = used_files(fls, Path::new("/p/thesis"), ".lemmata/build")
            .into_iter()
            .collect();
        assert_eq!(
            got,
            [
                PathBuf::from("chapters/one.tex"),
                PathBuf::from("figures/a.pdf"),
                PathBuf::from("main.tex")
            ]
        );
    }

    #[test]
    fn strips_comments_but_not_escaped_percent() {
        let tex = "% header\nText 50\\% done % note\n  % indented\nkeep\n";
        assert_eq!(strip_comments(tex), "Text 50\\% done %\nkeep\n");
    }
}
