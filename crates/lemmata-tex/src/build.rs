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
    #[error("could not start {0}: {1}")]
    SpawnTool(String, std::io::Error),
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
        .stderr(err);
    set_path(&mut cmd, opts);

    let limit = match project.config.timeout_secs {
        0 => Duration::from_secs(60 * 60 * 24),
        n => Duration::from_secs(n),
    };
    let mut cancel = cancel;
    let mut status = run_step(cmd, limit, &mut cancel)
        .await?
        .ok_or(Error::Spawn(std::io::Error::other("could not start")))?;

    // latexmk can fail to start at all (Perl missing, or blocked by a Windows
    // application control policy) and then writes no log. Rather than give up,
    // run the engine ourselves: the same passes, without latexmk.
    // Strictly newer than the start: a log left by the last build must not count, even a second old.
    let log_fresh = std::fs::metadata(project.log_path())
        .and_then(|m| m.modified())
        .is_ok_and(|t| t >= started_at);
    let mut fell_back = false;
    if status == Status::Failed
        && !log_fresh
        && tool(opts, project.config.engine.binary()).is_some()
    {
        match direct_build(project, opts, limit, &mut cancel).await {
            Ok(s) => {
                status = s;
                fell_back = true;
            }
            Err(e) => eprintln!("lemmata: building without latexmk failed too: {e}"),
        }
    }

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

    if fell_back {
        let tail = std::fs::read(&output_path)
            .map(|b| tail_of(&String::from_utf8_lossy(&b), 4000))
            .unwrap_or_default();
        let mut note = lemmata_log::launcher::launcher_failure(&tail);
        note.severity = lemmata_log::Severity::Info;
        note.message = format!(
            "latexmk could not run, so Lemmata built with {} directly. {}",
            project.config.engine.binary(),
            note.message
        );
        diagnostics.push(note);
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

/// Puts the TeX and Perl folders first on PATH.
fn set_path(cmd: &mut Command, opts: &BuildOptions<'_>) {
    let mut path_dirs: Vec<&Path> = Vec::new();
    path_dirs.extend(opts.bin_dir);
    path_dirs.extend(opts.perl_dir);
    if !path_dirs.is_empty() {
        cmd.env("PATH", prepend_paths(&path_dirs));
    }
}

/// Runs one command to the end, killing it and its children on timeout or
/// cancel. `None` means it could not be started.
async fn run_step(
    mut cmd: Command,
    limit: Duration,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<Option<Status>, Error> {
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(Error::Spawn(e)),
    };
    let pid = child.id();
    Ok(Some(tokio::select! {
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
    }))
}

/// A tool by name from one folder, if it is there.
fn in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    Some(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))).filter(|p| p.is_file())
}

/// The engine must be the one TeX was detected with: from its folder, or from
/// PATH when no folder is known.
fn tool(opts: &BuildOptions<'_>, name: &str) -> Option<PathBuf> {
    match opts.bin_dir {
        Some(dir) => in_dir(dir, name),
        None => which::which(name).ok(),
    }
}

/// biber and bibtex can live elsewhere than the engine (Arch puts biber in
/// vendor_perl): the TeX folder, the Perl folder, then PATH.
fn helper(opts: &BuildOptions<'_>, name: &str) -> Option<PathBuf> {
    opts.bin_dir
        .and_then(|d| in_dir(d, name))
        .or_else(|| opts.perl_dir.and_then(|d| in_dir(d, name)))
        .or_else(|| which::which(name).ok())
}

const MAX_PASSES: usize = 5;

/// The part of latexmk that matters, without latexmk: run the engine, run
/// biber or bibtex once when citations need it, and run the engine again
/// while the log asks for another pass. Returns the status of the last run.
async fn direct_build(
    project: &Project,
    opts: &BuildOptions<'_>,
    limit: Duration,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<Status, Error> {
    let engine = project.config.engine.binary();
    let engine_path = tool(opts, engine)
        .ok_or_else(|| Error::SpawnTool(engine.into(), std::io::Error::other("not found")))?;
    let outdir = project.outdir_abs();
    let mirror = |ext: &str| outdir.join(project.main.with_extension(ext));
    let main = project.main.to_string_lossy().replace('\\', "/");
    let mut bib_checked = false;
    let mut status = Status::Failed;

    for pass in 0..MAX_PASSES {
        let mut cmd = Command::new(&engine_path);
        cmd.args(["-interaction=nonstopmode", "-file-line-error", "-synctex=1"]);
        cmd.arg(if project.config.shell_escape {
            "-shell-escape"
        } else {
            "-no-shell-escape"
        });
        if project.config.stop_on_error {
            cmd.arg("-halt-on-error");
        }
        if opts.miktex {
            cmd.arg("--enable-installer");
        }
        cmd.arg(format!("-output-directory={}", project.config.outdir));
        if project.config.draft {
            cmd.arg(format!(
                "\\PassOptionsToPackage{{draft}}{{graphicx}}\\input{{{main}}}"
            ));
        } else {
            cmd.arg(&main);
        }
        cmd.current_dir(&project.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        set_path(&mut cmd, opts);
        status = run_step(cmd, limit, cancel)
            .await?
            .unwrap_or(Status::Failed);
        if status != Status::Success {
            return Ok(status);
        }

        let log = std::fs::read(mirror("log"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let rerun = [
            "Rerun to get",
            "Please rerun LaTeX",
            "Label(s) may have changed",
        ]
        .iter()
        .any(|m| log.contains(m));

        // The bibliography tool runs at most once, and only when what it reads has changed.
        let mut again = rerun;
        if !bib_checked && pass < MAX_PASSES - 1 {
            bib_checked = true;
            match run_bibliography(project, opts, limit, cancel).await? {
                Bib::Stopped(s) => return Ok(s),
                // The new .bbl only matters if it differs from the one the last pass used.
                Bib::Ran { changed } => again |= changed,
                Bib::Skipped => {}
            }
        }
        if !again {
            break;
        }
    }
    Ok(status)
}

enum Bib {
    Skipped,
    Ran { changed: bool },
    Stopped(Status),
}

/// Hash of what the bibliography tool would read: the citations TeX collected
/// (from the .bcf for biber, the \citation lines of the .aux for bibtex) and the
/// size and time of every .bib file.
fn bib_fingerprint(project: &Project, biber: bool) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let mirror = |ext: &str| project.outdir_abs().join(project.main.with_extension(ext));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if biber {
        std::fs::read(mirror("bcf")).ok()?.hash(&mut h);
    } else {
        let aux = std::fs::read_to_string(mirror("aux")).ok()?;
        if !aux.contains("\\bibdata") {
            return None;
        }
        for line in aux.lines().filter(|l| {
            l.starts_with("\\citation") || l.starts_with("\\bibstyle") || l.starts_with("\\bibdata")
        }) {
            line.hash(&mut h);
        }
    }
    let mut bibs = Vec::new();
    fn walk(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, out);
            } else if e.path.ends_with(".bib") {
                out.push(format!("{}:{}:{}", e.path, e.size, e.modified));
            }
        }
    }
    walk(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &mut bibs,
    );
    bibs.hash(&mut h);
    Some(format!("{:x}", h.finish()))
}

/// Runs biber or bibtex when the .bbl is missing or what the tool reads changed since it last ran.
async fn run_bibliography(
    project: &Project,
    opts: &BuildOptions<'_>,
    limit: Duration,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<Bib, Error> {
    let biber = project.config.bib == lemmata_project::Bib::Biber;
    let mirror = |ext: &str| project.outdir_abs().join(project.main.with_extension(ext));
    let Some(fingerprint) = bib_fingerprint(project, biber) else {
        return Ok(Bib::Skipped);
    };
    let (bbl, stamp) = (mirror("bbl"), mirror("lemmata-bib"));
    if bbl.is_file() && std::fs::read_to_string(&stamp).is_ok_and(|s| s == fingerprint) {
        return Ok(Bib::Skipped);
    }
    let stem = project
        .main
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    let outdir = project.config.outdir.trim_end_matches('/');
    let (name, args) = if biber {
        (
            "biber",
            vec![
                format!("--input-directory={outdir}"),
                format!("--output-directory={outdir}"),
                stem,
            ],
        )
    } else {
        ("bibtex", vec![format!("{outdir}/{stem}")])
    };
    let Some(path) = helper(opts, name) else {
        return Ok(Bib::Skipped);
    };
    let before = std::fs::read(&bbl).ok();
    let mut cmd = Command::new(path);
    cmd.args(args)
        .current_dir(&project.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    set_path(&mut cmd, opts);
    // A failing bibliography run shows up in the .blg; the next pass still matters.
    if let Some(s @ (Status::Cancelled | Status::Timeout)) = run_step(cmd, limit, cancel).await? {
        return Ok(Bib::Stopped(s));
    }
    let _ = std::fs::write(&stamp, &fingerprint);
    Ok(Bib::Ran {
        changed: std::fs::read(&bbl).ok() != before,
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

    /// latexmk dies without a log (Perl blocked); the engine is run directly, twice because the first log asks for a rerun.
    #[cfg(unix)]
    #[tokio::test]
    async fn builds_without_latexmk_when_it_cannot_start() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let script = |name: &str, body: &str| {
            let p = tools.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        let latexmk = script(
            "latexmk",
            "echo 'Windows API error 4551: blocked' >&2\nexit 1\n",
        );
        script(
            "pdflatex",
            "out=.lemmata/build\nfor a in \"$@\"; do case $a in -output-directory=*) out=${a#-output-directory=};; esac; done\nn=$(cat .count 2>/dev/null || echo 0); n=$((n+1)); echo $n > .count\nif [ $n -eq 1 ]; then echo 'LaTeX Warning: Label(s) may have changed. Rerun to get cross-references right.' > $out/main.log; else echo done > $out/main.log; fi\necho pdf > $out/main.pdf\n",
        );
        let p = Project::open(d.path()).unwrap();
        let (_tx, rx) = oneshot::channel();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let out = build(&p, &opts, rx).await.unwrap();
        assert_eq!(out.status, Status::Success);
        assert_eq!(
            std::fs::read_to_string(d.path().join(".count"))
                .unwrap()
                .trim(),
            "2"
        );
        assert!(out.pdf.is_some());
        let note = out
            .diagnostics
            .iter()
            .find(|x| x.message.contains("built with pdflatex directly"))
            .expect("a note about the fallback");
        assert_eq!(note.severity, lemmata_log::Severity::Info);
        assert!(note.message.contains("blocked"), "{}", note.message);
    }

    /// With real TeX: the direct build needs biber and a second pass to resolve
    /// \ref and \cite, and still reports what really is undefined.
    #[cfg(unix)]
    #[tokio::test]
    async fn direct_build_resolves_references_and_citations() {
        use std::os::unix::fs::PermissionsExt;
        let (Ok(pdflatex), Ok(_), true) = (which::which("pdflatex"), which::which("biber"), true)
        else {
            return; // no TeX on this machine: the CI job with TeX runs this
        };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article");
        let d = tempfile::tempdir().unwrap();
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                let (src, dst) = (e.path(), to.join(e.file_name()));
                if src.is_dir() {
                    copy(&src, &dst)
                } else {
                    std::fs::copy(&src, &dst).map(|_| ()).unwrap()
                }
            }
        }
        copy(&fixture, d.path());
        let latexmk = d.path().join("latexmk-fake");
        std::fs::write(&latexmk, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&latexmk, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = Project::open(d.path()).unwrap();
        let (_tx, rx) = oneshot::channel();
        let bin = pdflatex.parent().unwrap().to_path_buf();
        // biber lives elsewhere on some systems (vendor_perl); put its folder on PATH as the app would.
        let biber_dir = which::which("biber")
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&bin),
            installer: None,
            miktex: false,
            perl_dir: Some(&biber_dir),
        };
        let out = build(&p, &opts, rx).await.unwrap();
        let messages: Vec<&str> = out.diagnostics.iter().map(|x| x.message.as_str()).collect();
        assert!(out.pdf.is_some(), "{messages:?}");
        assert!(
            messages.iter().any(|m| m.contains("nobody2020")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("sec:missing")),
            "{messages:?}"
        );
        assert!(
            !messages
                .iter()
                .any(|m| m.contains("schulman2017ppo") || m.contains("eq:objective")),
            "{messages:?}"
        );
    }

    /// With real TeX and programs that count their starts: a fresh build takes the passes latexmk would,
    /// and an unchanged one starts pdflatex once and biber not at all.
    #[cfg(unix)]
    #[tokio::test]
    async fn direct_build_is_incremental() {
        use std::os::unix::fs::PermissionsExt;
        let (Ok(pdflatex), Ok(biber)) = (which::which("pdflatex"), which::which("biber")) else {
            return;
        };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article");
        let d = tempfile::tempdir().unwrap();
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                let (src, dst) = (e.path(), to.join(e.file_name()));
                if src.is_dir() {
                    copy(&src, &dst)
                } else {
                    std::fs::copy(&src, &dst).map(|_| ()).unwrap()
                }
            }
        }
        copy(&fixture, d.path());
        let tools = tempfile::tempdir().unwrap();
        let calls = tools.path().join("calls");
        let wrap = |name: &str, real: &Path| {
            let p = tools.path().join(name);
            std::fs::write(
                &p,
                format!(
                    "#!/bin/sh\necho {name} >> {}\nexec {} \"$@\"\n",
                    calls.display(),
                    real.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        wrap("pdflatex", &pdflatex);
        wrap("biber", &biber);
        let latexmk = d.path().join("latexmk-fake");
        std::fs::write(&latexmk, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&latexmk, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = Project::open(d.path()).unwrap();
        let count = |name: &str| {
            std::fs::read_to_string(&calls)
                .unwrap_or_default()
                .lines()
                .filter(|l| *l == name)
                .count()
        };
        let run = || async {
            let (_tx, rx) = oneshot::channel();
            let opts = BuildOptions {
                latexmk: &latexmk,
                bin_dir: Some(tools.path()),
                installer: None,
                miktex: false,
                perl_dir: None,
            };
            build(&p, &opts, rx).await.unwrap()
        };
        let first = run().await;
        assert!(first.pdf.is_some());
        let (latex_first, biber_first) = (count("pdflatex"), count("biber"));
        assert_eq!(biber_first, 1, "biber runs once on a fresh build");
        assert!(
            (2..=3).contains(&latex_first),
            "a fresh build needs two or three passes, not {latex_first}"
        );
        run().await;
        assert_eq!(
            count("pdflatex") - latex_first,
            1,
            "an unchanged build is one pass"
        );
        assert_eq!(count("biber"), biber_first, "and biber is not asked again");
        // A new citation changes what biber reads, so it runs again.
        let text = std::fs::read_to_string(d.path().join("chapters/method.tex")).unwrap();
        std::fs::write(
            d.path().join("chapters/method.tex"),
            text.replace("schulman2017ppo", "schulman2017ppo,mnih2015human"),
        )
        .unwrap();
        run().await;
        assert_eq!(count("biber"), biber_first + 1);
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
            bin_dir: Some(d.path()), // no engine in there: nothing to fall back to
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
