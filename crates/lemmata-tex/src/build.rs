//! Runs latexmk for a project and parses the resulting log.

use lemmata_log::Diagnostic;
use lemmata_project::Project;
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};
use tokio::process::Command;
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("latexmk was not found. Install TeX Live, or choose its folder in settings.")]
    NoLatexmk,
    #[error("could not start latexmk: {0}")]
    Spawn(std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Success,
    Failed,
    Cancelled,
    /// Stopped because it ran longer than the project's time limit.
    Timeout,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub status: Status,
    pub duration_ms: u64,
    /// The PDF, if this build wrote one. A failed build in nonstop mode
    /// often still produces a usable PDF.
    pub pdf: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct BuildOptions<'a> {
    pub latexmk: &'a Path,
    /// TeX bin folder, put first on PATH so latexmk finds the matching engine.
    pub bin_dir: Option<&'a Path>,
    /// Used to suggest install commands for missing packages.
    pub installer: Option<lemmata_pkgmap::Installer<'a>>,
    /// MiKTeX can't ask before installing a package without a window, so it
    /// is told to install them unattended.
    pub miktex: bool,
    /// Folder holding `perl`, when it isn't on the PATH Lemmata started with.
    pub perl_dir: Option<&'a Path>,
}

/// The latexmk arguments for a project.
pub fn latexmk_args(project: &Project, miktex: bool) -> Vec<OsString> {
    let c = &project.config;
    let mut args: Vec<OsString> = vec![
        c.engine.latexmk_flag().into(),
        "-synctex=1".into(),
        "-interaction=nonstopmode".into(),
        "-file-line-error".into(),
        if c.stop_on_error {
            "-halt-on-error"
        } else {
            // Keep going past errors (run biber, rerun for references) so the
            // user still gets the most complete PDF possible, like Overleaf.
            "-f"
        }
        .into(),
        if c.shell_escape {
            "-shell-escape"
        } else {
            "-no-shell-escape"
        }
        .into(),
        format!("-outdir={}", c.outdir).into(),
    ];
    if c.bib == lemmata_project::Bib::Bibtex {
        args.push("-bibtex".into());
    }
    if miktex {
        let (flag, bin) = match c.engine {
            lemmata_project::Engine::Pdflatex => ("-pdflatex", "pdflatex"),
            lemmata_project::Engine::Xelatex => ("-xelatex", "xelatex"),
            lemmata_project::Engine::Lualatex => ("-lualatex", "lualatex"),
        };
        args.push(format!("{flag}={bin} --enable-installer %O %S").into());
    }
    if c.draft {
        args.push("-usepretex=\\PassOptionsToPackage{draft}{graphicx}".into());
    }
    args.push(project.main.as_os_str().to_owned());
    args
}

/// Builds the project. Sending on `cancel` (or dropping the sender) stops the
/// build and everything latexmk started.
pub async fn build(
    project: &Project,
    opts: &BuildOptions<'_>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome, Error> {
    if !opts.latexmk.exists() {
        return Err(Error::NoLatexmk);
    }
    let started = Instant::now();
    let started_at = SystemTime::now();
    prepare_outdir(project);

    // latexmk's own output is the only explanation when it dies before TeX
    // writes a log (no Perl, a MiKTeX prompt), so keep it in a file.
    let output_path = project.outdir_abs().join("latexmk-output.txt");
    let (out, err) =
        match std::fs::File::create(&output_path).and_then(|f| f.try_clone().map(|g| (f, g))) {
            Ok((f, g)) => (Stdio::from(f), Stdio::from(g)),
            Err(_) => (Stdio::null(), Stdio::null()),
        };
    let mut cmd = Command::new(opts.latexmk);
    cmd.args(latexmk_args(project, opts.miktex))
        .current_dir(&project.root)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true);
    let mut path_dirs: Vec<&Path> = Vec::new();
    path_dirs.extend(opts.bin_dir);
    path_dirs.extend(opts.perl_dir);
    if !path_dirs.is_empty() {
        cmd.env("PATH", prepend_paths(&path_dirs));
    }
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn().map_err(Error::Spawn)?;
    let pid = child.id();

    let limit = match project.config.timeout_secs {
        0 => Duration::from_secs(60 * 60 * 24),
        n => Duration::from_secs(n),
    };
    let status = tokio::select! {
        res = child.wait() => match res {
            Ok(s) if s.success() => Status::Success,
            _ => Status::Failed,
        },
        _ = tokio::time::sleep(limit) => {
            kill_tree(pid);
            let _ = child.wait().await;
            Status::Timeout
        }
        _ = cancel => {
            kill_tree(pid);
            let _ = child.wait().await;
            Status::Cancelled
        }
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    if status == Status::Cancelled {
        return Ok(Outcome {
            status,
            duration_ms,
            pdf: None,
            diagnostics: Vec::new(),
        });
    }
    if status == Status::Timeout {
        let mut d = std::fs::read(project.log_path())
            .map(|b| lemmata_log::parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        d.insert(
            0,
            Diagnostic {
                severity: lemmata_log::Severity::Error,
                kind: lemmata_log::Kind::Other,
                file: None,
                line: None,
                message: format!(
                    "The build took longer than {} seconds and was stopped.",
                    project.config.timeout_secs
                ),
                hint: Some(
                    "Try Fast mode in the Recompile menu, or look for an endless loop in a macro. You can change the limit with timeout_secs in lemmata.toml.".into(),
                ),
                fix: None,
            },
        );
        return Ok(Outcome {
            status,
            duration_ms,
            pdf: None,
            diagnostics: d,
        });
    }

    let pdf = project.pdf_path();
    let fresh = std::fs::metadata(&pdf)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t + Duration::from_secs(1) >= started_at);
    let mut diagnostics = std::fs::read(project.log_path())
        .map(|b| lemmata_log::parse(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    // The bibliography tool's own log explains what the main log can't.
    let blg_path = project
        .outdir_abs()
        .join(project.main.with_extension("blg"));
    let blg_fresh = std::fs::metadata(&blg_path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t + Duration::from_secs(1) >= started_at);
    if blg_fresh && let Ok(bytes) = std::fs::read(&blg_path) {
        for d in lemmata_log::parse_blg(&String::from_utf8_lossy(&bytes)) {
            // A missing key is already reported from the main log.
            if !diagnostics.iter().any(|x| x.kind == d.kind) {
                diagnostics.push(d);
            }
        }
    } else if diagnostics
        .iter()
        .any(|d| matches!(d.kind, lemmata_log::Kind::UndefinedCitation { .. }))
    {
        for d in &mut diagnostics {
            if matches!(d.kind, lemmata_log::Kind::UndefinedCitation { .. }) {
                d.hint = Some("The bibliography tool did not run, most often because a .bib file named in \\addbibresource{} or \\bibliography{} is missing from the project. Also check that biber (or bibtex) is installed.".into());
            }
        }
    }
    // Files in the build folder (.bbl, .aux) aren't something the user can edit.
    let outdir = project.config.outdir.trim_start_matches("./");
    for d in &mut diagnostics {
        if d.file
            .as_deref()
            .is_some_and(|f| f.trim_start_matches("./").starts_with(outdir))
        {
            d.file = None;
            d.line = None;
        }
        if let (lemmata_log::Kind::MissingFile { name }, Some(installer)) =
            (&d.kind, opts.installer)
        {
            d.fix = lemmata_pkgmap::fix_for(name, installer).map(|f| lemmata_log::Fix {
                explanation: f.explanation,
                command: f.command,
                package: None,
            });
        }
    }

    // Failed without a single error in the log: latexmk never got to TeX.
    if status == Status::Failed
        && !diagnostics
            .iter()
            .any(|d| d.severity == lemmata_log::Severity::Error)
    {
        let tail = std::fs::read(&output_path)
            .map(|b| tail_of(&String::from_utf8_lossy(&b), 4000))
            .unwrap_or_default();
        diagnostics.insert(0, lemmata_log::launcher::launcher_failure(&tail));
    }

    Ok(Outcome {
        status,
        duration_ms,
        pdf: fresh.then_some(pdf),
        diagnostics,
    })
}

/// The last `max` bytes of `text`, cut at a character boundary.
fn tail_of(text: &str, max: usize) -> String {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// File types TeX and its helpers write next to the PDF. "Clear cached
/// files" removes only these, so a build folder that was ever pointed at a
/// source folder can't take sources with it.
const ARTEFACTS: &[&str] = &[
    "aux",
    "log",
    "toc",
    "lof",
    "lot",
    "out",
    "bbl",
    "blg",
    "bcf",
    "fls",
    "fdb_latexmk",
    "gz",
    "xml",
    "nav",
    "snm",
    "vrb",
    "xdv",
    "idx",
    "ind",
    "ilg",
    "glo",
    "gls",
    "glg",
    "ist",
    "acn",
    "acr",
    "alg",
    "thm",
    "tdo",
    "spl",
    "pyg",
    "cut",
    "dvi",
    "run",
    "synctex",
    "bak",
    "mw",
    "loa",
    "lol",
    "brf",
];

fn is_artefact(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            name.ends_with(".synctex.gz")
                || name.ends_with(".run.xml")
                || name.ends_with("-blx.bib")
                || path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| ARTEFACTS.contains(&e))
        })
        // The PDF is what the preview shows; it is kept.
        && path.extension().is_none_or(|e| e != "pdf")
}

fn clean_dir(dir: &Path, keep: &Path) -> std::io::Result<usize> {
    let mut removed = 0;
    for e in std::fs::read_dir(dir)?.flatten() {
        let path = e.path();
        // Never follow links out of the build folder.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            removed += clean_dir(&path, keep)?;
            // Mirrored folders are kept for the next build; drop them only when they are empty.
            if std::fs::read_dir(&path).is_ok_and(|mut d| d.next().is_none())
                && std::fs::remove_dir(&path).is_ok()
            {
                removed += 1;
            }
        } else if path != keep && meta.is_file() && is_artefact(&path) {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Deletes the TeX byproducts in the build folder (aux files, logs, synctex
/// data) except the PDF, so the next build starts from scratch. Returns how
/// many entries were removed. Refuses to touch a folder that contains the
/// project itself.
pub fn clean(project: &Project) -> std::io::Result<usize> {
    let outdir = project.outdir_abs();
    if !lemmata_project::valid_outdir(&project.config.outdir) || project.root.starts_with(&outdir) {
        return Err(std::io::Error::other(
            "the build folder is not a folder inside the project",
        ));
    }
    if !outdir.is_dir() {
        return Ok(0);
    }
    clean_dir(&outdir, &project.pdf_path())
}

/// `\include{chapters/x}` writes `chapters/x.aux` inside the output folder,
/// and TeX can't create folders, so mirror the project's folders there.
fn prepare_outdir(project: &Project) {
    fn mirror(entries: &[lemmata_project::Entry], outdir: &Path) {
        for e in entries.iter().filter(|e| e.is_dir) {
            let _ = std::fs::create_dir_all(outdir.join(&e.path));
            mirror(&e.children, outdir);
        }
    }
    let outdir = project.outdir_abs();
    let _ = std::fs::create_dir_all(&outdir);
    mirror(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &outdir,
    );
}

fn prepend_paths(dirs: &[&Path]) -> OsString {
    let mut paths: Vec<PathBuf> = dirs.iter().map(|d| d.to_path_buf()).collect();
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap_or_default()
}

/// latexmk spawns the engine and biber as children; kill them all.
fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        // The child leads its own process group (process_group(0) above).
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outdir_mirrors_project_folders() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{report}").unwrap();
        std::fs::create_dir_all(d.path().join("chapters/appendix")).unwrap();
        let p = Project::open(d.path()).unwrap();
        prepare_outdir(&p);
        assert!(p.outdir_abs().join("chapters/appendix").is_dir());
        assert!(!p.outdir_abs().join(".lemmata").exists());
    }

    #[test]
    fn args_respect_config() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        let args: Vec<String> = latexmk_args(&p, false)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-pdf".to_string()));
        assert!(args.contains(&"-no-shell-escape".to_string()));
        assert_eq!(args.last().unwrap(), "main.tex");

        p.config.engine = lemmata_project::Engine::Lualatex;
        p.config.shell_escape = true;
        let args: Vec<String> = latexmk_args(&p, false)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-lualatex".to_string()));
        assert!(args.contains(&"-shell-escape".to_string()));
        assert!(args.contains(&"-f".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("-usepretex")));
    }

    #[test]
    fn draft_and_stop_on_error_change_args() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.config.draft = true;
        p.config.stop_on_error = true;
        let args: Vec<String> = latexmk_args(&p, false)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-halt-on-error".to_string()));
        assert!(!args.contains(&"-f".to_string()));
        assert!(args.contains(&"-usepretex=\\PassOptionsToPackage{draft}{graphicx}".to_string()));
    }

    #[test]
    fn miktex_installs_packages_unattended() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        let args: Vec<String> = latexmk_args(&p, true)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-pdflatex=pdflatex --enable-installer %O %S".to_string()));
        assert_eq!(args.last().unwrap(), "main.tex");
    }

    #[test]
    fn tail_cuts_on_a_character_boundary() {
        assert_eq!(tail_of("abcdef", 3), "def");
        assert_eq!(tail_of("aéé", 3), "é");
        assert_eq!(tail_of("ab", 10), "ab");
    }

    /// latexmk exits 1 without ever writing a log: the build must say why.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_latexmk_that_never_reaches_tex_is_explained() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let fake = d.path().join("fake-latexmk");
        std::fs::write(
            &fake,
            "#!/bin/sh\necho 'perl: command not found' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = Project::open(d.path()).unwrap();
        let (_tx, rx) = oneshot::channel();
        let opts = BuildOptions {
            latexmk: &fake,
            bin_dir: None,
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let out = build(&p, &opts, rx).await.unwrap();
        assert_eq!(out.status, Status::Failed);
        assert_eq!(out.diagnostics.len(), 1);
        assert!(out.diagnostics[0].message.contains("Perl"));
    }

    #[test]
    fn clean_keeps_the_pdf_and_never_removes_sources() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        prepare_outdir(&p);
        let out = p.outdir_abs();
        std::fs::write(out.join("main.aux"), "").unwrap();
        std::fs::write(out.join("main.log"), "").unwrap();
        std::fs::write(out.join("notes.tex"), "a source that ended up here").unwrap();
        std::fs::create_dir_all(out.join("chapters")).unwrap();
        std::fs::write(p.pdf_path(), "pdf").unwrap();
        assert_eq!(
            clean(&p).unwrap(),
            3,
            "aux, log and the now-empty mirror folder"
        );
        assert!(p.pdf_path().exists());
        assert!(out.join("notes.tex").exists());
        assert!(!out.join("main.aux").exists());
        assert!(!out.join("chapters").exists());

        // A build folder that is the project itself is refused outright.
        p.config.outdir = ".".into();
        assert!(clean(&p).is_err());
        assert!(d.path().join("main.tex").exists());
    }
}
