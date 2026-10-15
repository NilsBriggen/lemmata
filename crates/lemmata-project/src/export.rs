//! Exporting a project as a zip: the whole source tree, or an arXiv-ready
//! bundle. Shared by the app and the `lemmata` command.

use crate::Project;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

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
/// verbatim environment that is still open at the end of the line, and the
/// byte ranges of the skipped parts.
fn scan_line(line: &str) -> (Option<usize>, Option<String>, Vec<std::ops::Range<usize>>) {
    let mut skipped = Vec::new();
    let mut i = 0;
    while let Some(c) = line[i..].chars().next() {
        if c == '%' {
            return (Some(i), None, skipped);
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
                let next = rest_start(body) + end;
                skipped.push(i..next);
                i = next;
                continue;
            }
        }
        if let Some(r) = ["url{", "href{"].iter().find_map(|c| after.strip_prefix(c)) {
            let next = rest_start(r) + braced_len(r);
            skipped.push(i..next);
            i = next;
            continue;
        }
        if let Some(r) = after.strip_prefix("begin{")
            && let Some(n) = r.find('}')
            && VERBATIM_ENVS.contains(&r[..n].trim_end_matches('*'))
        {
            let end = format!("\\end{{{}}}", &r[..n]);
            let body = &r[n + 1..];
            match body.find(&end) {
                Some(k) => {
                    let next = rest_start(body) + k + end.len();
                    skipped.push(i..next);
                    i = next;
                }
                None => {
                    skipped.push(i..line.len());
                    return (None, Some(r[..n].to_string()), skipped);
                }
            }
            continue;
        }
        // Any other escape (`\%`, `\\`): the next character is not special.
        i += 1 + after.chars().next().map_or(0, char::len_utf8);
    }
    (None, None, skipped)
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
        let (cut, opened, _) = scan_line(line);
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
    fn walk(entries: &[crate::Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, out);
            } else {
                out.push(e.path.clone());
            }
        }
    }
    let mut out = Vec::new();
    walk(&crate::tree(root, outdir), &mut out);
    out
}

fn io(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn zip_writer(dest: &Path) -> Result<zip::ZipWriter<std::fs::File>, String> {
    Ok(zip::ZipWriter::new(
        std::fs::File::create(dest).map_err(io)?,
    ))
}

fn add(
    zip: &mut zip::ZipWriter<std::fs::File>,
    name: impl Into<String>,
    bytes: &[u8],
) -> Result<(), String> {
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(name.into(), opts).map_err(io)?;
    zip.write_all(bytes).map_err(io)
}

/// Zips the whole project (not hidden files or the build folder), for a
/// backup or to share it. With `with_pdf`, the latest PDF is added too.
/// Returns the number of files written.
pub fn zip_project(p: &Project, dest: &Path, with_pdf: bool) -> Result<usize, String> {
    let mut zip = zip_writer(dest)?;
    let mut count = 0;
    for rel in project_files(&p.root, &p.config.outdir) {
        add(
            &mut zip,
            &rel,
            &std::fs::read(p.root.join(&rel)).map_err(io)?,
        )?;
        count += 1;
    }
    if with_pdf && let Ok(pdf) = std::fs::read(p.pdf_path()) {
        let name = p
            .pdf_path()
            .file_name()
            .map_or("main.pdf".into(), |n| n.to_string_lossy().into_owned());
        add(&mut zip, name, &pdf)?;
        count += 1;
    }
    zip.finish().map_err(io)?;
    Ok(count)
}

/// How deep `\input` nesting may go before flattening gives up.
const MAX_DEPTH: usize = 32;

/// Replaces `\printbibliography` in a flattened file: biblatex output needs
/// its package, so the command and the .bbl stay.
const BIBLATEX_NOTE: &str = "% Lemmata: biblatex is kept as is; keep the .bbl next to this file.";

/// A flattened main file: its new text, the project files it absorbed (to
/// leave out of the zip), and whether the .bbl text replaced `\bibliography`.
#[derive(Debug)]
struct Flattened {
    text: String,
    inlined: BTreeSet<PathBuf>,
    bbl_used: bool,
}

/// `name` joined onto `base` (both relative to the project root), with `.`
/// and `..` folded. None for macros, absolute paths and paths that leave the
/// project, so an arXiv zip never holds files from outside it.
fn resolve(base: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['\\', '#']) {
        return None;
    }
    let mut out = PathBuf::new();
    for c in base.join(name).components() {
        match c {
            std::path::Component::Normal(s) => out.push(s),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// The `{...}` argument at or after `p` in `s` (spaces allowed before it):
/// its text, and the index just past its closing `}`.
fn braced_arg(s: &str, p: usize) -> Option<(&str, usize)> {
    let trimmed = s[p..].trim_start_matches([' ', '\t']);
    let start = s.len() - trimmed.len();
    let body = trimmed.strip_prefix('{')?;
    let mut depth = 1;
    for (n, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&body[..n], start + 1 + n + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// The part of a subfile that `\subfile` prints: its document body.
fn subfile_body(text: &str) -> &str {
    const BEGIN: &str = "\\begin{document}";
    let start = text.find(BEGIN).map_or(0, |k| k + BEGIN.len());
    let end = text[start..]
        .find("\\end{document}")
        .map_or(text.len(), |k| start + k);
    &text[start..end]
}

struct Flattener<'a> {
    read: &'a dyn Fn(&Path) -> Option<String>,
    bbl: Option<&'a str>,
    /// The main file's folder: `\import` paths are relative to it.
    top: PathBuf,
    /// Files being expanded, outermost first. Seeing one again is a cycle.
    stack: Vec<PathBuf>,
    inlined: BTreeSet<PathBuf>,
    bbl_used: bool,
}

impl Flattener<'_> {
    /// `text`, a file in folder `base`, with its inputs expanded. Comments and
    /// verbatim parts are copied as they are.
    fn expand(&mut self, text: &str, base: &Path) -> Result<String, String> {
        let mut out = String::with_capacity(text.len());
        let mut open: Option<String> = None;
        for line in text.lines() {
            let mut line = line;
            if let Some(env) = open.take() {
                let end = format!("\\end{{{env}}}");
                let Some(k) = line.find(&end) else {
                    open = Some(env);
                    out.push_str(line);
                    out.push('\n');
                    continue;
                };
                out.push_str(&line[..k + end.len()]);
                line = &line[k + end.len()..];
            }
            let (cut, opened, skipped) = scan_line(line);
            open = opened;
            let code = &line[..cut.unwrap_or(line.len())];
            let (mut i, mut j) = (0, 0);
            while let Some(d) = code[j..].find('\\') {
                let k = j + d;
                if skipped.iter().any(|r| r.contains(&k)) {
                    j = k + 1;
                    continue;
                }
                match self.command(code, k, base)? {
                    Some((rep, end)) => {
                        out.push_str(&line[i..k]);
                        out.push_str(&rep);
                        i = end;
                        j = end;
                    }
                    // Step over an escaped character too, so `\\input` is not a command.
                    None => j = k + 1 + code[k + 1..].chars().next().map_or(0, char::len_utf8),
                }
            }
            out.push_str(&line[i..]);
            out.push('\n');
        }
        Ok(out)
    }

    /// What replaces the command at `code[k..]`, and the index just past it.
    /// None leaves the command as it is.
    fn command(
        &mut self,
        code: &str,
        k: usize,
        base: &Path,
    ) -> Result<Option<(String, usize)>, String> {
        let name: String = code[k + 1..]
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        let arity = match name.as_str() {
            "input" | "include" | "subfile" | "bibliography" => 1,
            "import" | "subimport" => 2,
            "printbibliography" => 0,
            _ => return Ok(None),
        };
        let mut p = k + 1 + name.len();
        let mut args = Vec::new();
        for _ in 0..arity {
            let Some((arg, next)) = braced_arg(code, p) else {
                return Ok(None);
            };
            args.push(arg);
            p = next;
        }
        match name.as_str() {
            "printbibliography" => {
                return Ok(Some((format!("{BIBLATEX_NOTE}\n\\printbibliography"), p)));
            }
            "bibliography" => {
                let Some(bbl) = self.bbl else {
                    return Ok(None);
                };
                self.bbl_used = true;
                return Ok(Some((bbl.trim_end().to_string(), p)));
            }
            _ => {}
        }
        // `\import` paths are relative to the main file's folder, `\subimport`
        // paths to the current one. The file itself is looked up from there.
        let (new_base, file) = if name == "import" || name == "subimport" {
            let top = if name == "import" {
                self.top.as_path()
            } else {
                base
            };
            let Some(dir) = resolve(top, args[0].trim()) else {
                return Ok(None);
            };
            (dir, args[1])
        } else {
            (base.to_path_buf(), args[0])
        };
        let Some(target) = resolve(&new_base, file.trim()) else {
            return Ok(None);
        };
        let Some((rel, text)) = self.find(&target) else {
            return Ok(None);
        };
        if self.stack.contains(&rel) {
            return Err(format!(
                "{} includes itself, so \\{name} would loop forever.",
                rel.display()
            ));
        }
        if self.stack.len() > MAX_DEPTH {
            return Err(format!(
                "Files nest more than {MAX_DEPTH} levels deep, at {}.",
                rel.display()
            ));
        }
        let src = if name == "subfile" {
            subfile_body(&text)
        } else {
            text.as_str()
        };
        self.stack.push(rel.clone());
        let body = self.expand(src, &new_base);
        self.stack.pop();
        let body = body?;
        self.inlined.insert(rel);
        let body = body.trim_end_matches('\n');
        let rep = if name == "include" {
            format!("\\clearpage\n{body}\n\\clearpage")
        } else {
            body.to_string()
        };
        Ok(Some((rep, p)))
    }

    /// The first of `target.tex` and `target` that can be read, with its text.
    /// A name that already ends in `.tex` is only tried as it is.
    fn find(&self, target: &Path) -> Option<(PathBuf, String)> {
        let mut with_tex = target.as_os_str().to_owned();
        with_tex.push(".tex");
        let cands = if target.extension().is_some_and(|e| e == "tex") {
            vec![target.to_path_buf()]
        } else {
            vec![PathBuf::from(with_tex), target.to_path_buf()]
        };
        cands
            .into_iter()
            .find_map(|c| (self.read)(&c).map(|t| (c, t)))
    }
}

/// Inlines the inputs of the main file `files[0]` (paths relative to the
/// project root), and the .bbl text in place of `\bibliography` when `bbl` is
/// given. A missing or unsafe input is left as it is.
fn flatten_tex(
    main: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
    bbl: Option<&str>,
) -> Result<Flattened, String> {
    let text = read(main).ok_or_else(|| format!("Can't read {}.", main.display()))?;
    let top = main.parent().unwrap_or(Path::new("")).to_path_buf();
    let mut f = Flattener {
        read,
        bbl,
        top: top.clone(),
        stack: vec![main.to_path_buf()],
        inlined: BTreeSet::new(),
        bbl_used: false,
    };
    let text = f.expand(&text, &top)?;
    Ok(Flattened {
        text,
        inlined: f.inlined,
        bbl_used: f.bbl_used,
    })
}

/// Zips the files the last build used, plus the .bbl (arXiv doesn't run
/// biber), ready to upload to arXiv. With `strip`, `%` comments are removed
/// from the .tex files. With `flatten`, the main file has its inputs inlined,
/// and the files inlined are left out of the zip. Returns the number of files
/// written.
pub fn zip_arxiv(p: &Project, dest: &Path, strip: bool, flatten: bool) -> Result<usize, String> {
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
    let bbl = std::fs::read(outdir.join(format!("{stem}.bbl"))).ok();
    let flat = if flatten {
        let read = |rel: &Path| {
            std::fs::read(p.root.join(rel))
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let bbl_text = bbl
            .as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned());
        Some(flatten_tex(&p.main, &read, bbl_text.as_deref())?)
    } else {
        None
    };

    let mut zip = zip_writer(dest)?;
    let mut count = 0;
    for rel in &files {
        if flat.as_ref().is_some_and(|f| f.inlined.contains(rel)) {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        let bytes = match &flat {
            Some(f) if *rel == p.main => f.text.as_bytes().to_vec(),
            _ => std::fs::read(p.root.join(rel)).map_err(io)?,
        };
        let bytes = if strip && name.ends_with(".tex") {
            strip_comments(&String::from_utf8_lossy(&bytes)).into_bytes()
        } else {
            bytes
        };
        add(&mut zip, name, &bytes)?;
        count += 1;
    }
    // The bibliography, already formatted, next to the main file.
    let main_dir = p
        .main
        .parent()
        .map(|d| d.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let bbl_inlined = flat.as_ref().is_some_and(|f| f.bbl_used);
    if let Some(bbl) = bbl.filter(|_| !bbl_inlined) {
        let name = if main_dir.is_empty() {
            format!("{stem}.bbl")
        } else {
            format!("{main_dir}/{stem}.bbl")
        };
        add(&mut zip, name, &bbl)?;
        count += 1;
    }
    zip.finish().map_err(io)?;
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

    /// Flattens the first file, with the others readable as project files.
    fn run(files: &[(&str, &str)], bbl: Option<&str>) -> Result<Flattened, String> {
        let read = |p: &Path| {
            files
                .iter()
                .find(|(n, _)| Path::new(n) == p)
                .map(|(_, t)| t.to_string())
        };
        flatten_tex(Path::new(files[0].0), &read, bbl)
    }

    fn text(files: &[(&str, &str)]) -> String {
        run(files, None).unwrap().text
    }

    #[test]
    fn flatten_expands_nested_inputs() {
        let f = run(
            &[
                ("main.tex", "\\input{a}\n"),
                ("a.tex", "A \\input{b} A\n"),
                ("b.tex", "B\n"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(f.text, "A B A\n");
        assert_eq!(
            f.inlined.into_iter().collect::<Vec<_>>(),
            [PathBuf::from("a.tex"), PathBuf::from("b.tex")]
        );
    }

    #[test]
    fn flatten_include_adds_clearpage() {
        assert_eq!(
            text(&[("main.tex", "\\include{ch}\n"), ("ch.tex", "CH\n")]),
            "\\clearpage\nCH\n\\clearpage\n"
        );
    }

    #[test]
    fn flatten_input_without_extension() {
        assert_eq!(
            text(&[
                ("main.tex", "\\input{a}\n\\input{b.tex}\n"),
                ("a.tex", "A\n"),
                ("b.tex", "B\n"),
            ]),
            "A\nB\n"
        );
    }

    #[test]
    fn flatten_leaves_comments_alone() {
        let f = run(
            &[
                ("main.tex", "% \\input{a}\n\\input{a} % \\input{a}\n"),
                ("a.tex", "A\n"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(f.text, "% \\input{a}\nA % \\input{a}\n");
        assert_eq!(f.inlined.len(), 1);
    }

    #[test]
    fn flatten_leaves_verbatim_alone() {
        assert_eq!(
            text(&[
                (
                    "main.tex",
                    "\\begin{verbatim}\n\\input{a}\n\\end{verbatim}\n\\input{a}\nx \\verb|\\input{a}| y\n"
                ),
                ("a.tex", "A\n"),
            ]),
            "\\begin{verbatim}\n\\input{a}\n\\end{verbatim}\nA\nx \\verb|\\input{a}| y\n"
        );
    }

    #[test]
    fn flatten_import_and_subimport_change_the_folder() {
        // \import is relative to the main folder, \subimport to the current one.
        assert_eq!(
            text(&[
                ("main.tex", "\\import{sec}{intro}\n"),
                (
                    "sec/intro.tex",
                    "\\subimport{sub}{x}\n\\import{other}{y}\n\\input{z}\n"
                ),
                ("sec/sub/x.tex", "X\n"),
                ("other/y.tex", "Y\n"),
                ("sec/z.tex", "Z\n"),
            ]),
            "X\nY\nZ\n"
        );
        // A main file in a subfolder: \import is relative to that folder.
        let f = run(
            &[
                ("thesis/main.tex", "\\import{ch}{a}\n"),
                ("thesis/ch/a.tex", "A\n"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(f.text, "A\n");
        assert!(f.inlined.contains(Path::new("thesis/ch/a.tex")));
    }

    #[test]
    fn flatten_stops_on_cycles() {
        let err = run(
            &[
                ("main.tex", "\\input{a}\n"),
                ("a.tex", "\\input{b}\n"),
                ("b.tex", "\\input{a}\n"),
            ],
            None,
        )
        .unwrap_err();
        assert!(err.contains("loop forever"), "{err}");
        let err = run(&[("main.tex", "\\input{main}\n")], None).unwrap_err();
        assert!(err.contains("loop forever"), "{err}");
    }

    #[test]
    fn flatten_never_reads_outside_the_project() {
        let src = "\\input{../secret}\n\\input{/etc/x}\n";
        assert_eq!(text(&[("main.tex", src), ("secret.tex", "S\n")]), src);
    }

    #[test]
    fn flatten_subfile_takes_only_the_body() {
        assert_eq!(
            text(&[
                ("main.tex", "\\subfile{s}\n"),
                (
                    "s.tex",
                    "\\documentclass{article}\n\\begin{document}\nS\n\\end{document}\n"
                ),
            ]),
            "\nS\n"
        );
    }

    #[test]
    fn flatten_inlines_bibtex_bibliography() {
        let bbl = "\\begin{thebibliography}{1}\n\\end{thebibliography}\n";
        let f = run(
            &[(
                "main.tex",
                "Text \\bibliography{refs}\n% \\bibliography{x}\n",
            )],
            Some(bbl),
        )
        .unwrap();
        assert_eq!(
            f.text,
            "Text \\begin{thebibliography}{1}\n\\end{thebibliography}\n% \\bibliography{x}\n"
        );
        assert!(f.bbl_used);
    }

    #[test]
    fn flatten_keeps_biblatex_bibliography() {
        let f = run(&[("main.tex", "\\printbibliography\n")], Some("bbl")).unwrap();
        assert_eq!(f.text, format!("{BIBLATEX_NOTE}\n\\printbibliography\n"));
        assert!(!f.bbl_used);
    }
}
