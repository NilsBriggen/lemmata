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

/// Drops the `\\?\` prefix Windows puts on verbatim paths, so they compare
/// equal to the plain `C:\...` form latexmk writes in the `.fls` file.
fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// Project files the last build read, from latexmk's `.fls` recorder file.
pub fn used_files(fls: &str, root: &Path, outdir: &str) -> BTreeSet<PathBuf> {
    let root = plain(root);
    let pwd = fls
        .lines()
        .find_map(|l| l.strip_prefix("PWD "))
        .map(|p| plain(Path::new(p)))
        .unwrap_or_else(|| root.clone());
    fls.lines()
        .filter_map(|l| l.strip_prefix("INPUT "))
        .filter_map(|p| {
            let p = plain(Path::new(p));
            let abs = if p.is_absolute() { p } else { pwd.join(p) };
            let rel = abs.strip_prefix(&root).ok()?;
            // Normalise "./chapters/a.tex" and skip build output.
            let rel: PathBuf = rel
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir))
                .collect();
            // Whole path components: "build" must not match "buildings.tex".
            let in_outdir = !outdir.is_empty() && rel.starts_with(outdir);
            (!in_outdir && !rel.to_string_lossy().starts_with('.') && rel.as_os_str() != "")
                .then_some(rel)
        })
        .collect()
}

/// Environments whose body is printed as written, so `%` is not a comment.
const VERBATIM_ENVS: [&str; 5] = ["verbatim", "Verbatim", "lstlisting", "minted", "alltt"];

/// Length of the braced argument at the start of `r` (after its `{`),
/// including the closing `}`; the whole string if it never closes.
fn braced_len(r: &str) -> usize {
    let mut depth = 1;
    for (n, c) in r.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return n + 1;
                }
            }
            _ => {}
        }
    }
    r.len()
}

/// Finds where a real comment starts on `line`, skipping `\verb`, `\url`,
/// `\href` and inline verbatim environments. Also returns the name of a
/// verbatim environment that is still open at the end of the line.
fn scan_line(line: &str) -> (Option<usize>, Option<String>) {
    let mut i = 0;
    while let Some(c) = line[i..].chars().next() {
        if c == '%' {
            return (Some(i), None);
        }
        if c != '\\' {
            i += c.len_utf8();
            continue;
        }
        let after = &line[i + 1..];
        let rest_start = |rest: &str| line.len() - rest.len();
        if let Some(r) = after.strip_prefix("verb")
            && !r.starts_with(|c: char| c.is_ascii_alphabetic())
        {
            // \verb|..| or \verb*|..|: any delimiter, closed on the same line.
            let r = r.strip_prefix('*').unwrap_or(r);
            if let Some(d) = r.chars().next() {
                let body = &r[d.len_utf8()..];
                let end = body.find(d).map_or(body.len(), |k| k + d.len_utf8());
                i = rest_start(body) + end;
                continue;
            }
        }
        if let Some(r) = ["url{", "href{"].iter().find_map(|c| after.strip_prefix(c)) {
            i = rest_start(r) + braced_len(r);
            continue;
        }
        if let Some(r) = after.strip_prefix("begin{")
            && let Some(n) = r.find('}')
            && VERBATIM_ENVS.contains(&r[..n].trim_end_matches('*'))
        {
            let end = format!("\\end{{{}}}", &r[..n]);
            let body = &r[n + 1..];
            match body.find(&end) {
                Some(k) => i = rest_start(body) + k + end.len(),
                None => return (None, Some(r[..n].to_string())),
            }
            continue;
        }
        // Any other escape (`\%`, `\\`): the next character is not special.
        i += 1 + after.chars().next().map_or(0, char::len_utf8);
    }
    (None, None)
}

/// Removes `%` comments (keeping `\%`), dropping lines that were only a comment.
/// Verbatim code, `\verb`, `\url` and `\href` are left alone.
pub fn strip_comments(tex: &str) -> String {
    let mut out = String::with_capacity(tex.len());
    let mut open: Option<String> = None;
    for line in tex.lines() {
        if let Some(env) = &open {
            if line.contains(&format!("\\end{{{env}}}")) {
                open = None;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let (cut, opened) = scan_line(line);
        open = opened;
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

    #[test]
    fn keeps_percent_in_verbatim_environments() {
        for env in ["verbatim", "Verbatim", "verbatim*", "lstlisting", "minted"] {
            let tex =
                format!("a % x\n\\begin{{{env}}}\n% kept\nx = 5 % kept\n\\end{{{env}}}\nb % y\n");
            assert_eq!(
                strip_comments(&tex),
                format!("a %\n\\begin{{{env}}}\n% kept\nx = 5 % kept\n\\end{{{env}}}\nb %\n")
            );
        }
        // Arguments on the \begin line belong to the environment too.
        assert_eq!(
            strip_comments("\\begin{minted}[linenos]{c} % kept\n% kept\n\\end{minted}\n"),
            "\\begin{minted}[linenos]{c} % kept\n% kept\n\\end{minted}\n"
        );
    }

    #[test]
    fn keeps_percent_in_verb_with_any_delimiter() {
        assert_eq!(strip_comments("\\verb|50%| ok % c\n"), "\\verb|50%| ok %\n");
        assert_eq!(strip_comments("\\verb+a%b+ x % c\n"), "\\verb+a%b+ x %\n");
        assert_eq!(strip_comments("\\verb*!a%b! x % c\n"), "\\verb*!a%b! x %\n");
        assert_eq!(strip_comments("\\verb|a%\n"), "\\verb|a%\n");
    }

    #[test]
    fn keeps_percent_in_url_and_href() {
        assert_eq!(
            strip_comments("\\url{http://a.b/c%20d} text % c\n"),
            "\\url{http://a.b/c%20d} text %\n"
        );
        assert_eq!(
            strip_comments("\\href{http://a.b/c%20d}{link} % c\n"),
            "\\href{http://a.b/c%20d}{link} %\n"
        );
    }

    #[test]
    fn comment_after_escaped_backslash_is_stripped() {
        assert_eq!(
            strip_comments("a \\\\ % c\nb\\\\% c\n"),
            "a \\\\ %\nb\\\\%\n"
        );
    }

    #[test]
    fn outdir_match_is_by_path_component() {
        let fls = "PWD /p\nINPUT main.tex\nINPUT buildings.tex\nINPUT build-notes/a.tex\nINPUT build/main.aux\nINPUT build/sub/x.tex\n";
        let got: Vec<_> = used_files(fls, Path::new("/p"), "build")
            .into_iter()
            .collect();
        assert_eq!(
            got,
            [
                PathBuf::from("build-notes/a.tex"),
                PathBuf::from("buildings.tex"),
                PathBuf::from("main.tex")
            ]
        );
    }

    #[test]
    fn verbatim_windows_prefix_is_ignored() {
        assert_eq!(plain(Path::new(r"\\?\C:\proj")), PathBuf::from(r"C:\proj"));
        // A verbatim root still matches the plain paths in the .fls.
        let fls = "PWD /p\nINPUT main.tex\nINPUT /p/a.tex\n";
        let got: Vec<_> = used_files(fls, Path::new(r"\\?\/p"), "build")
            .into_iter()
            .collect();
        assert_eq!(got, [PathBuf::from("a.tex"), PathBuf::from("main.tex")]);
    }
}
