//! Runs latexmk for a project and parses the resulting log.

use crate::format;
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
    #[error(
        "Tectonic was not found. Use Tectonic from the start screen, or choose another compiler in the project settings."
    )]
    NoTectonic,
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
    /// latexmk could not run and the engine was run directly. Callers can pass
    /// `skip_latexmk` to `build_with` next time to avoid the failed start.
    #[serde(skip)]
    pub latexmk_failed: bool,
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
            // Tectonic builds never come here (see tectonic::build).
            lemmata_project::Engine::Tectonic => ("-xelatex", "xelatex"),
        };
        args.push(format!("{flag}={bin} --enable-installer %O %S").into());
    }
    if c.draft {
        args.push("-usepretex=\\PassOptionsToPackage{draft}{graphicx}".into());
    }
    // A project's latexmkrc is a Perl program: only a trusted project's runs. The user's own
    // rc files still apply.
    if !project.trusted {
        args.push("-norc".into());
        for rc in user_rcs().into_iter().filter(|p| p.is_file()) {
            args.push("-r".into());
            args.push(rc.into());
        }
    }
    args.push(project.main.as_os_str().to_owned());
    args
}

/// The user's own latexmk rc files, where latexmk looks for them.
fn user_rcs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    home.into_iter()
        .flat_map(|h| {
            [
                h.join(".latexmkrc"),
                h.join(".config").join("latexmk").join("latexmkrc"),
            ]
        })
        .collect()
}

/// Builds the project. Sending on `cancel` (or dropping the sender) stops the
/// build and everything latexmk started.
pub async fn build(
    project: &Project,
    opts: &BuildOptions<'_>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome, Error> {
    build_with(project, opts, cancel, false, false).await
}

/// Like `build`. With `skip_latexmk` (set after an earlier build reported
/// `latexmk_failed`) the engine is run directly at once, when it can be.
pub async fn build_with(
    project: &Project,
    opts: &BuildOptions<'_>,
    cancel: oneshot::Receiver<()>,
    skip_latexmk: bool,
    // Build even when no project file changed (an explicit Recompile).
    force: bool,
) -> Result<Outcome, Error> {
    if !opts.latexmk.exists() {
        return Err(Error::NoLatexmk);
    }
    let started = Instant::now();
    // Nothing changed since the last good build: no TeX run at all, just its result again.
    if !force && let Some(o) = cached_outcome(project, started) {
        return Ok(o);
    }
    let started_at = SystemTime::now();
    prepare_outdir(project);
    // With the slack in written_since, last build's log would look fresh; no log is the signal that latexmk never ran TeX.
    let _ = std::fs::remove_file(project.log_path());

    // latexmk's own output is the only explanation when it dies before TeX
    // writes a log (no Perl, a MiKTeX prompt), so keep it in a file.
    let output_path = project.outdir_abs().join("latexmk-output.txt");
    let limit = match project.config.timeout_secs {
        0 => Duration::from_secs(60 * 60 * 24),
        n => Duration::from_secs(n),
    };
    let mut cancel = cancel;
    let engine = tool(opts, project.config.engine.binary());
    let can_go_direct = engine.is_some();
    let warm = !skip_latexmk && can_go_direct && warm_ok(project);
    // No stamp: the last build failed or was stopped midway, and a killed TeX run may have taken
    // the log and synctex with it. latexmk's record still says the PDF matches, so it would do
    // nothing once the text is back to what it last built (undoing a broken edit, say).
    if !project.outdir_abs().join(OK_STAMP).is_file() {
        let _ = std::fs::remove_file(out_file(project, "fdb_latexmk"));
    }
    let _ = std::fs::remove_file(project.outdir_abs().join(OK_STAMP));
    // A warm rebuild can start from the precompiled preamble; an explicit Recompile never does.
    let fmt = match &engine {
        Some(e) if warm && !force && format::eligible(project, opts.miktex) => {
            format::usable(project, e)
        }
        _ => None,
    };
    // On skipping, the earlier failure's output file stays to explain the note below.
    let mut status = Status::Failed;
    if warm && let Ok(s) = direct_build(project, opts, limit, &mut cancel, fmt.as_deref()).await {
        status = s;
    }
    let fmt_failed = fmt.is_some() && status == Status::Failed;
    if fmt_failed {
        // Its errors could be the format's own: only a build without it gets to report them.
        let _ = std::fs::remove_file(project.log_path());
    }
    // A failed direct build is run again by latexmk, which keeps going past errors the way the user chose.
    let settled = warm && status != Status::Failed;
    if !settled && !(skip_latexmk && can_go_direct) {
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
        status = match run_step(cmd, limit, &mut cancel).await {
            Ok(s) => s.ok_or(Error::Spawn(std::io::Error::other("could not start")))?,
            // Blocked from starting (a Windows policy): the direct build below can still work.
            Err(Error::Spawn(e)) if can_go_direct => {
                let _ = std::fs::write(&output_path, e.to_string());
                Status::Failed
            }
            Err(e) => return Err(e),
        };
    }

    // latexmk can fail to start at all (Perl missing, or blocked by a Windows
    // application control policy) and then writes no log. Rather than give up,
    // run the engine ourselves: the same passes, without latexmk.
    let log_fresh = written_since(&project.log_path(), started_at);
    let mut fell_back = false;
    if status == Status::Failed && !log_fresh && can_go_direct {
        match direct_build(project, opts, limit, &mut cancel, None).await {
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
            latexmk_failed: false,
        });
    }
    if status == Status::Timeout {
        let mut d = read_log(project);
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
            latexmk_failed: false,
        });
    }

    let pdf = project.pdf_path();
    let fresh = written_since(&pdf, started_at);
    let mut diagnostics = read_log(project);
    merge_blg(project, &mut diagnostics, started_at);
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

    if status == Status::Success && fresh {
        mark_ok(project, started_at);
        if fmt_failed {
            format::reject(project);
        }
        if let Some(e) = engine
            && format::eligible(project, opts.miktex)
        {
            format::spawn(project.clone(), e, path_env(opts), limit);
        }
    }

    Ok(Outcome {
        status,
        duration_ms,
        pdf: fresh.then_some(pdf),
        diagnostics,
        latexmk_failed: fell_back,
    })
}

/// The result of the last good build again, when nothing changed since.
pub(crate) fn cached_outcome(project: &Project, started: Instant) -> Option<Outcome> {
    if !up_to_date(project) {
        return None;
    }
    let outdir = project.config.outdir.trim_start_matches("./").to_string();
    let mut diagnostics = read_log(project);
    for d in &mut diagnostics {
        if d.file
            .as_deref()
            .is_some_and(|f| f.trim_start_matches("./").starts_with(&outdir))
        {
            d.file = None;
            d.line = None;
        }
    }
    Some(Outcome {
        status: Status::Success,
        duration_ms: started.elapsed().as_millis() as u64,
        pdf: None,
        diagnostics,
        latexmk_failed: false,
    })
}

/// The bibliography tool's own log explains what the main log can't.
pub(crate) fn merge_blg(
    project: &Project,
    diagnostics: &mut Vec<Diagnostic>,
    started_at: SystemTime,
) {
    let blg_path = out_file(project, "blg");
    let blg_fresh = written_since(&blg_path, started_at);
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
        for d in diagnostics.iter_mut() {
            if matches!(d.kind, lemmata_log::Kind::UndefinedCitation { .. }) {
                d.hint = Some("The bibliography tool did not run, most often because a .bib file named in \\addbibresource{} or \\bibliography{} is missing from the project. Also check that biber (or bibtex) is installed.".into());
            }
        }
    }
}

/// Where TeX and its helpers write `ext` output for the main file. They put it
/// flat in the build folder, so `src/main.tex` gives `main.ext`, not `src/main.ext`.
pub(crate) fn out_file(project: &Project, ext: &str) -> PathBuf {
    let stem = project.main.file_stem().unwrap_or_default();
    let mut name = stem.to_os_string();
    name.push(".");
    name.push(ext);
    project.outdir_abs().join(name)
}

/// The diagnostics in the main log. A run from the precompiled preamble didn't read the preamble,
/// so what it said then comes from the log of the run that made the format.
pub(crate) fn read_log(project: &Project) -> Vec<Diagnostic> {
    let mut log = std::fs::read(project.log_path())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    if project.config.engine == lemmata_project::Engine::Tectonic {
        log = crate::tectonic::normalize_log(&log);
    }
    let mut d = lemmata_log::parse(&log);
    if format::was_used(&log) {
        let mut pre = lemmata_log::parse(&format::preamble_log(project));
        pre.retain(|x| !d.contains(x));
        pre.append(&mut d);
        d = pre;
    }
    d
}

/// File times can trail the clock: FAT/exFAT and network shares round them, and
/// Linux stamps them from a coarse clock a few milliseconds behind.
const MTIME_SLACK: Duration = Duration::from_secs(2);

/// Whether `path` was written by a build that began at `started_at`.
pub(crate) fn written_since(path: &Path, started_at: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t + MTIME_SLACK >= started_at)
}

/// Puts the TeX and Perl folders first on PATH.
fn set_path(cmd: &mut Command, opts: &BuildOptions<'_>) {
    if let Some(p) = path_env(opts) {
        cmd.env("PATH", p);
    }
}

/// PATH with the TeX and Perl folders first, if either is known.
fn path_env(opts: &BuildOptions<'_>) -> Option<OsString> {
    let mut path_dirs: Vec<&Path> = Vec::new();
    path_dirs.extend(opts.bin_dir);
    path_dirs.extend(opts.perl_dir);
    (!path_dirs.is_empty()).then(|| prepend_paths(&path_dirs))
}

/// Runs one command to the end, killing it and its children on timeout or
/// cancel. `None` means it could not be started.
pub(crate) async fn run_step(
    mut cmd: Command,
    limit: Duration,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<Option<Status>, Error> {
    // Every TeX tool starts here: with the user's environment, not an AppImage's.
    lemmata_env::host(cmd.as_std_mut());
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(Error::Spawn(e)),
    };
    Ok(Some(tokio::select! {
        res = child.wait() => match res {
            Ok(s) if s.success() => Status::Success,
            _ => Status::Failed,
        },
        _ = tokio::time::sleep(limit) => {
            stop(&mut child).await;
            Status::Timeout
        }
        _ = cancel => {
            stop(&mut child).await;
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

/// Written into the build folder after a successful build, holding the main
/// file. Its presence means the folder is in a state the direct build can continue from.
pub(crate) const OK_STAMP: &str = ".lemmata-ok";

/// Whether the engine can be run directly instead of latexmk: the last build here
/// succeeded, and nothing configures steps that only latexmk runs.
fn warm_ok(project: &Project) -> bool {
    let outdir = project.outdir_abs();
    let main = project.main.to_string_lossy();
    if !std::fs::read_to_string(outdir.join(OK_STAMP)).is_ok_and(|s| s == main) {
        return false;
    }
    // Nothing changed since the last good build: latexmk sees that at once (faster than an engine run).
    if !changed_since(project, &outdir.join(OK_STAMP)) {
        return false;
    }
    let rcs = [
        project.root.join("latexmkrc"),
        project.root.join(".latexmkrc"),
    ]
    .into_iter()
    .chain(user_rcs());
    if rcs.into_iter().any(|p| p.exists()) {
        return false;
    }
    // Index and glossary files need makeindex or makeglossaries, which only latexmk runs.
    let needs_tools = std::fs::read_dir(&outdir).is_ok_and(|d| {
        d.flatten().any(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| ["idx", "glo", "nlo", "acn", "ist", "xdy"].contains(&x))
        })
    });
    !needs_tools
}

/// The last build succeeded, its PDF and log are there, and no project file changed since it began.
pub fn up_to_date(project: &Project) -> bool {
    let stamp = project.outdir_abs().join(OK_STAMP);
    let main = project.main.to_string_lossy();
    std::fs::read_to_string(&stamp).is_ok_and(|s| s == main)
        && project.pdf_path().is_file()
        && project.log_path().is_file()
        && !changed_since(project, &stamp)
}

/// True when some project file was modified after the time on `stamp`, the start of the last good build
/// (or up to `MTIME_SLACK` before it: at worst one build too many, never an edit left out).
fn changed_since(project: &Project, stamp: &Path) -> bool {
    let Ok(since) = std::fs::metadata(stamp).and_then(|m| m.modified()) else {
        return true;
    };
    fn any_newer(
        root: &Path,
        entries: &[lemmata_project::Entry],
        since: std::time::SystemTime,
    ) -> bool {
        entries.iter().any(|e| {
            if e.is_dir {
                any_newer(root, &e.children, since)
            } else {
                std::fs::metadata(root.join(&e.path))
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t + MTIME_SLACK >= since)
            }
        })
    }
    any_newer(
        &project.root,
        &lemmata_project::tree(&project.root, &project.config.outdir),
        since,
    )
}

/// Records a successful build, and what the bibliography tool read, so the next one can go direct without running it again.
pub(crate) fn mark_ok(project: &Project, started_at: SystemTime) {
    let biber = project.config.bib == lemmata_project::Bib::Biber;
    if let Some(fp) = bib_fingerprint(project, biber) {
        let _ = std::fs::write(out_file(project, "lemmata-bib"), fp);
    }
    let stamp = project.outdir_abs().join(OK_STAMP);
    let _ = std::fs::write(&stamp, project.main.to_string_lossy().as_bytes());
    // Dated when the build began: a file saved while it ran was maybe not read, so it still counts as changed.
    let _ = std::fs::File::options()
        .write(true)
        .open(&stamp)
        .and_then(|f| f.set_modified(started_at));
}

/// The engine's arguments for one pass writing to `outdir`, from the format `fmt` if given.
pub(crate) fn pass_args(
    project: &Project,
    outdir: &str,
    fmt: Option<&str>,
    miktex: bool,
) -> Vec<String> {
    let c = &project.config;
    let mut args: Vec<String> = fmt.map(|f| format!("-fmt={f}")).into_iter().collect();
    args.extend(
        [
            "-interaction=nonstopmode",
            "-file-line-error",
            "-synctex=1",
            if c.shell_escape {
                "-shell-escape"
            } else {
                "-no-shell-escape"
            },
        ]
        .map(String::from),
    );
    if c.stop_on_error {
        args.push("-halt-on-error".into());
    }
    if miktex {
        args.push("--enable-installer".into());
    }
    args.push(format!("-output-directory={outdir}"));
    let main = project.main.to_string_lossy().replace('\\', "/");
    args.push(if c.draft {
        format!("\\PassOptionsToPackage{{draft}}{{graphicx}}\\input{{{main}}}")
    } else {
        main
    });
    args
}

/// The part of latexmk that matters, without latexmk: run the engine, run
/// biber or bibtex once when citations need it, and run the engine again
/// while the log asks for another pass. Returns the status of the last run.
async fn direct_build(
    project: &Project,
    opts: &BuildOptions<'_>,
    limit: Duration,
    cancel: &mut oneshot::Receiver<()>,
    fmt: Option<&str>,
) -> Result<Status, Error> {
    let engine = project.config.engine.binary();
    let engine_path = tool(opts, engine)
        .ok_or_else(|| Error::SpawnTool(engine.into(), std::io::Error::other("not found")))?;
    let mut bib_checked = false;
    let mut status = Status::Failed;
    // This writes over latexmk's outputs without latexmk knowing: its record would later call a
    // stale PDF up to date (e.g. after undoing a broken edit) and skip the build.
    let _ = std::fs::remove_file(out_file(project, "fdb_latexmk"));

    for pass in 0..MAX_PASSES {
        let mut cmd = Command::new(&engine_path);
        cmd.args(pass_args(project, &project.config.outdir, fmt, opts.miktex));
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

        let log = std::fs::read(out_file(project, "log"))
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
    let mirror = |ext: &str| out_file(project, ext);
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
    let mirror = |ext: &str| out_file(project, ext);
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
pub(crate) fn tail_of(text: &str, max: usize) -> String {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// Written into the build folder when Lemmata prepares it. `clean` deletes
/// only where it finds this, so a build folder pointed at someone's own folder is safe.
const MARKER: &str = ".lemmata-build";

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
    // Lemmata's own default folder is safe even when a build from before the marker existed made it.
    if !outdir.join(MARKER).is_file() && project.config.outdir != lemmata_project::DEFAULT_OUTDIR {
        return Err(std::io::Error::other(
            "the build folder was not created by Lemmata, so it is left alone",
        ));
    }
    let _ = std::fs::remove_file(outdir.join(OK_STAMP));
    let _ = std::fs::remove_dir_all(outdir.join(format::DIR));
    clean_dir(&outdir, &project.pdf_path())
}

/// `\include{chapters/x}` writes `chapters/x.aux` inside the output folder,
/// and TeX can't create folders, so mirror the project's folders there.
pub(crate) fn prepare_outdir(project: &Project) {
    fn mirror(entries: &[lemmata_project::Entry], outdir: &Path) {
        for e in entries.iter().filter(|e| e.is_dir) {
            let _ = std::fs::create_dir_all(outdir.join(&e.path));
            mirror(&e.children, outdir);
        }
    }
    let outdir = project.outdir_abs();
    let _ = std::fs::create_dir_all(&outdir);
    let _ = std::fs::write(outdir.join(MARKER), "");
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

/// How long a build gets to exit after the polite kill before it is forced.
const KILL_GRACE: Duration = Duration::from_secs(2);

/// Kills the child and everything it started, and waits for it, without ever
/// hanging: TERM first, then force after a grace period.
async fn stop(child: &mut tokio::process::Child) {
    let pid = child.id();
    kill_tree(pid, false);
    if tokio::time::timeout(KILL_GRACE, child.wait())
        .await
        .is_err()
    {
        kill_tree(pid, true);
        // If even this fails the child is dropped, and kill_on_drop has the last word.
        let _ = tokio::time::timeout(KILL_GRACE, child.wait()).await;
    }
}

/// latexmk spawns the engine and biber as children; signal them all.
fn kill_tree(pid: Option<u32>, force: bool) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        // The child leads its own process group (process_group(0) above). Signal the group with the system
        // call itself: Debian's /usr/bin/kill can read "-<pid>" as a signal number and hit other processes.
        let Ok(group) = i32::try_from(pid) else {
            return;
        };
        if group > 1 {
            // SAFETY: kill(2) only sends a signal; a negative pid addresses exactly that process group.
            unsafe { libc::kill(-group, if force { libc::SIGKILL } else { libc::SIGTERM }) };
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = force; // taskkill /F is already as hard as Windows gets
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
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
        assert!(
            args.contains(&"-norc".to_string()),
            "a project's latexmkrc doesn't run until it is trusted"
        );
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

    /// A project whose latexmk and pdflatex only record that they ran (in `calls`) and write a log and PDF.
    /// Either fails, leaving a log, while a file `fail` exists in the project.
    #[cfg(unix)]
    struct Fake {
        dir: tempfile::TempDir,
        tools: PathBuf,
    }

    #[cfg(unix)]
    impl Fake {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("main.tex"), "\\documentclass{article}").unwrap();
            let tools = dir.path().join("tools");
            std::fs::create_dir_all(&tools).unwrap();
            let body = |name: &str| {
                format!(
                    "#!/bin/sh\nout=.lemmata/build\nfor a in \"$@\"; do case $a in -output-directory=*) out=${{a#-output-directory=}};; -outdir=*) out=${{a#-outdir=}};; esac; done\necho {name} >> calls\necho done > $out/main.log\necho pdf > $out/main.pdf\n[ -f fail ] && exit 1\nexit 0\n"
                )
            };
            for name in ["latexmk", "pdflatex"] {
                let p = tools.join(name);
                std::fs::write(&p, body(name)).unwrap();
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            Fake { dir, tools }
        }
        /// Builds after an edit, like a rebuild in the app (with nothing changed, latexmk handles it).
        async fn build(&self) -> Outcome {
            // File times are only a few milliseconds fine; an edit right after a build could look older.
            std::thread::sleep(Duration::from_millis(30));
            let main = self.dir.path().join("main.tex");
            let mut text = std::fs::read_to_string(&main).unwrap_or_default();
            text.push_str("% edit\n");
            std::fs::write(&main, text).unwrap();
            let p = Project::open(self.dir.path()).unwrap();
            let opts = BuildOptions {
                latexmk: &self.tools.join("latexmk"),
                bin_dir: Some(&self.tools),
                installer: None,
                miktex: false,
                perl_dir: None,
            };
            let (_tx, rx) = oneshot::channel();
            build(&p, &opts, rx).await.unwrap()
        }
        fn calls(&self) -> String {
            let c = std::fs::read_to_string(self.dir.path().join("calls")).unwrap_or_default();
            let _ = std::fs::remove_file(self.dir.path().join("calls"));
            c.lines().collect::<Vec<_>>().join(" ")
        }
        fn outdir(&self) -> PathBuf {
            self.dir.path().join(".lemmata/build")
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_warm_rebuild_skips_latexmk() {
        let f = Fake::new();
        let first = f.build().await;
        assert_eq!(first.status, Status::Success);
        assert_eq!(f.calls(), "latexmk");
        let second = f.build().await;
        assert_eq!(second.status, Status::Success);
        assert!(second.pdf.is_some() && !second.latexmk_failed);
        assert!(second.diagnostics.is_empty());
        assert_eq!(f.calls(), "pdflatex");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn latexmk_returns_after_a_failure_and_after_clearing_the_cache() {
        let f = Fake::new();
        f.build().await;
        f.calls();
        // The direct build fails, so latexmk gets to try as well.
        std::fs::write(f.dir.path().join("fail"), "").unwrap();
        assert_eq!(f.build().await.status, Status::Failed);
        assert_eq!(f.calls(), "pdflatex latexmk");
        std::fs::remove_file(f.dir.path().join("fail")).unwrap();
        assert_eq!(f.build().await.status, Status::Success);
        assert_eq!(f.calls(), "latexmk");
        assert_eq!(f.build().await.status, Status::Success);
        assert_eq!(f.calls(), "pdflatex");
        clean(&Project::open(f.dir.path()).unwrap()).unwrap();
        f.build().await;
        assert_eq!(f.calls(), "latexmk");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_latexmkrc_keeps_latexmk_in_charge() {
        for rc in ["latexmkrc", ".latexmkrc"] {
            let f = Fake::new();
            std::fs::write(f.dir.path().join(rc), "$pdflatex = 'x';").unwrap();
            f.build().await;
            f.build().await;
            f.build().await;
            assert_eq!(f.calls(), "latexmk latexmk latexmk", "{rc}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn index_files_keep_latexmk_in_charge() {
        let f = Fake::new();
        f.build().await;
        assert_eq!(f.calls(), "latexmk");
        std::fs::write(f.outdir().join("main.idx"), "").unwrap();
        f.build().await;
        assert_eq!(f.calls(), "latexmk");
        std::fs::remove_file(f.outdir().join("main.idx")).unwrap();
        f.build().await;
        assert_eq!(f.calls(), "pdflatex");
    }

    /// With real TeX: the second build is direct, and still has the PDF and resolved citations and references.
    #[cfg(unix)]
    #[tokio::test]
    async fn two_real_builds_resolve_citations() {
        let (Ok(pdflatex), Ok(biber), Ok(latexmk)) = (
            which::which("pdflatex"),
            which::which("biber"),
            which::which("latexmk"),
        ) else {
            return;
        };
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\n\\usepackage[backend=biber]{biblatex}\n\\addbibresource{r.bib}\n\\begin{document}\nSee \\cite{k} and Section~\\ref{s}.\n\\section{A}\\label{s}\n\\printbibliography\n\\end{document}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("r.bib"),
            "@book{k, author={Doe, Jane}, title={T}, year={2020}, publisher={P}}\n",
        )
        .unwrap();
        let p = Project::open(d.path()).unwrap();
        let bin = pdflatex.parent().unwrap().to_path_buf();
        let biber_dir = biber.parent().unwrap().to_path_buf();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&bin),
            installer: None,
            miktex: false,
            perl_dir: Some(&biber_dir),
        };
        for round in 0..2 {
            if round == 1 {
                // A rebuild follows an edit; with nothing changed there is nothing to build.
                std::thread::sleep(Duration::from_millis(30));
                let main = p.root.join(&p.main);
                let text = std::fs::read_to_string(&main).unwrap();
                std::fs::write(&main, format!("{text}% edit\n")).unwrap();
            }
            let (_tx, rx) = oneshot::channel();
            let out = build(&p, &opts, rx).await.unwrap();
            let messages: Vec<&str> = out.diagnostics.iter().map(|x| x.message.as_str()).collect();
            assert_eq!(out.status, Status::Success, "round {round}: {messages:?}");
            assert!(out.pdf.is_some_and(|f| f.is_file()), "round {round}");
            assert!(messages.is_empty(), "round {round}: {messages:?}");
            if round == 0 {
                assert!(p.outdir_abs().join(OK_STAMP).is_file());
            }
        }
    }

    /// A build stopped midway leaves no log, synctex or stamp, and latexmk's record of the last good
    /// build. Back at that text (an undo), the next build must run, not call the old PDF up to date.
    #[cfg(unix)]
    #[tokio::test]
    async fn after_a_stopped_build_undoing_back_still_builds() {
        let (Ok(pdflatex), Ok(latexmk)) = (which::which("pdflatex"), which::which("latexmk"))
        else {
            return;
        };
        let d = tempfile::tempdir().unwrap();
        let good = "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n";
        let main = d.path().join("main.tex");
        std::fs::write(&main, good).unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.config.fast_preamble = false;
        let bin = pdflatex.parent().unwrap().to_path_buf();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&bin),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_tx, rx) = oneshot::channel();
        assert_eq!(build(&p, &opts, rx).await.unwrap().status, Status::Success);
        // What a run killed while building a broken edit leaves behind.
        let out = p.outdir_abs();
        for f in ["main.log", "main.synctex.gz", OK_STAMP] {
            let _ = std::fs::remove_file(out.join(f));
        }
        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(&main, good).unwrap();
        let (_tx, rx) = oneshot::channel();
        assert_eq!(build(&p, &opts, rx).await.unwrap().status, Status::Success);
        assert!(
            out.join("main.synctex.gz").is_file() && p.log_path().is_file(),
            "latexmk didn't run"
        );
    }

    #[test]
    fn an_edit_saved_during_a_build_still_counts_as_changed() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        let stamp = p.outdir_abs().join(OK_STAMP);
        // Nothing touched since well before the build began: up to date.
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
        );
        assert!(!changed_since(&p, &stamp));
        // Saved while the build ran, which then finished.
        let started = SystemTime::now();
        std::fs::write(p.root.join(&p.main), "\\documentclass{article}% edit").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        mark_ok(&p, started);
        assert!(changed_since(&p, &stamp));
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

    /// After one fallback, the caller can skip latexmk: this latexmk would leave a marker if it ran.
    #[cfg(unix)]
    #[tokio::test]
    async fn skip_latexmk_goes_straight_to_the_engine() {
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
        let latexmk = script("latexmk", "touch latexmk-ran\nexit 1\n");
        script(
            "pdflatex",
            "out=.lemmata/build\nfor a in \"$@\"; do case $a in -output-directory=*) out=${a#-output-directory=};; esac; done\necho done > $out/main.log\necho pdf > $out/main.pdf\n",
        );
        let p = Project::open(d.path()).unwrap();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_tx, rx) = oneshot::channel();
        let out = build_with(&p, &opts, rx, true, false).await.unwrap();
        assert_eq!(out.status, Status::Success);
        assert!(out.latexmk_failed);
        assert!(!d.path().join("latexmk-ran").exists());
        assert!(
            out.diagnostics
                .iter()
                .any(|x| x.message.contains("built with pdflatex directly"))
        );
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
        // Files saved just before a build count as changed (MTIME_SLACK); a real project's are older.
        fn age(dir: &Path) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                if e.path().is_dir() {
                    age(&e.path());
                } else {
                    let old = SystemTime::now() - Duration::from_secs(60);
                    std::fs::File::options()
                        .write(true)
                        .open(e.path())
                        .unwrap()
                        .set_modified(old)
                        .unwrap();
                }
            }
        }
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
        age(d.path());
        let mut p = Project::open(d.path()).unwrap();
        // A precompiled preamble is made with pdflatex in the background, which would count too.
        p.config.fast_preamble = false;
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
            0,
            "an unchanged project isn't built again"
        );
        assert_eq!(count("biber"), biber_first, "and biber is not asked again");
        // A new citation changes what biber reads, so it runs again.
        std::thread::sleep(Duration::from_millis(30)); // file times are only a few ms fine
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

    #[test]
    fn clean_refuses_a_folder_lemmata_did_not_create() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        // A folder of the user's own; Lemmata's default .lemmata/build is always its own.
        std::fs::write(d.path().join("lemmata.toml"), "outdir = \"out\"\n").unwrap();
        let p = Project::open(d.path()).unwrap();
        // The folder exists and holds an artefact-looking file, but no build ever prepared it.
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(p.outdir_abs().join("mine.log"), "keep me").unwrap();
        assert!(clean(&p).is_err());
        assert!(p.outdir_abs().join("mine.log").exists());
        prepare_outdir(&p);
        assert_eq!(clean(&p).unwrap(), 1);
        assert!(
            p.outdir_abs().join(MARKER).exists(),
            "the marker survives a clean"
        );
    }

    #[test]
    fn outputs_are_flat_in_the_build_folder() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("lemmata.toml"), "main = \"src/main.tex\"\n").unwrap();
        let p = Project::open(d.path()).unwrap();
        assert_eq!(p.main, Path::new("src/main.tex"));
        for ext in ["blg", "bbl", "log", "pdf", "lemmata-bib"] {
            assert_eq!(
                out_file(&p, ext),
                p.outdir_abs().join(format!("main.{ext}"))
            );
        }
        assert_eq!(out_file(&p, "pdf"), p.pdf_path());
        assert_eq!(out_file(&p, "log"), p.log_path());
    }

    /// latexmk writes the .blg flat even for src/main.tex; its errors must reach the user.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_blg_of_a_main_file_in_a_subfolder_is_read() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("lemmata.toml"), "main = \"src/main.tex\"\n").unwrap();
        let fake = d.path().join("fake-latexmk");
        std::fs::write(
            &fake,
            "#!/bin/sh\nout=.lemmata/build\necho x > $out/main.log\necho pdf > $out/main.pdf\necho \"ERROR - BibTeX subsystem: Cannot find 'refs.bib'\" > $out/main.blg\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = Project::open(d.path()).unwrap();
        assert_eq!(p.config.outdir, ".lemmata/build");
        let (_tx, rx) = oneshot::channel();
        let opts = BuildOptions {
            latexmk: &fake,
            bin_dir: None,
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let out = build(&p, &opts, rx).await.unwrap();
        assert!(out.pdf.is_some());
        assert!(
            out.diagnostics
                .iter()
                .any(|x| x.message.contains("refs.bib")),
            "{:?}",
            out.diagnostics
        );
    }

    #[test]
    fn freshness_allows_two_seconds_of_slack() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("x.log");
        std::fs::write(&f, "").unwrap();
        let started = SystemTime::now();
        let set = |ago: u64| {
            let t = started - Duration::from_secs(ago);
            std::fs::File::options()
                .write(true)
                .open(&f)
                .unwrap()
                .set_modified(t)
                .unwrap();
        };
        set(1);
        assert!(written_since(&f, started));
        set(5);
        assert!(!written_since(&f, started));
        assert!(!written_since(&d.path().join("missing"), started));
    }

    /// A build that ignores SIGTERM must still be stopped within a few seconds, by timeout and by cancel.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_build_that_ignores_sigterm_is_still_stopped() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let fake = d.path().join("fake-latexmk");
        std::fs::write(&fake, "#!/bin/sh\ntrap '' TERM\nsleep 60\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.config.timeout_secs = 1;
        let opts = BuildOptions {
            latexmk: &fake,
            bin_dir: None,
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let t = Instant::now();
        let (_tx, rx) = oneshot::channel();
        let out = build(&p, &opts, rx).await.unwrap();
        assert_eq!(out.status, Status::Timeout);
        assert!(t.elapsed() < Duration::from_secs(8), "{:?}", t.elapsed());

        p.config.timeout_secs = 0;
        let t = Instant::now();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
        let out = build(&p, &opts, rx).await.unwrap();
        assert_eq!(out.status, Status::Cancelled);
        assert!(t.elapsed() < Duration::from_secs(8), "{:?}", t.elapsed());
    }

    /// A project built with this machine's TeX, for the precompiled preamble. None without
    /// pdflatex, latexmk or mylatexformat (the CI job with TeX runs these).
    #[cfg(unix)]
    struct Real {
        dir: tempfile::TempDir,
        latexmk: PathBuf,
        bin: PathBuf,
    }

    #[cfg(unix)]
    impl Real {
        fn new(main: &str, toml: &str) -> Option<Self> {
            let (Ok(pdflatex), Ok(latexmk)) = (which::which("pdflatex"), which::which("latexmk"))
            else {
                return None;
            };
            let has_mylatexformat = std::process::Command::new("kpsewhich")
                .arg("mylatexformat.ltx")
                .output()
                .is_ok_and(|o| !o.stdout.is_empty());
            if !has_mylatexformat {
                return None;
            }
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("main.tex"), main).unwrap();
            std::fs::write(dir.path().join("lemmata.toml"), toml).unwrap();
            let bin = pdflatex.parent().unwrap().to_path_buf();
            Some(Real { dir, latexmk, bin })
        }
        fn project(&self) -> Project {
            Project::open(self.dir.path()).unwrap()
        }
        fn edit(&self, from: &str, to: &str) {
            // File times are only a few milliseconds fine; an edit right after a build could look older.
            std::thread::sleep(Duration::from_millis(30));
            let main = self.dir.path().join("main.tex");
            let text = std::fs::read_to_string(&main).unwrap();
            assert!(text.contains(from), "{from}");
            std::fs::write(&main, text.replacen(from, to, 1)).unwrap();
        }
        /// Builds after an edit to the body, as a rebuild in the app.
        async fn build(&self) -> Outcome {
            self.edit("\\end{document}", "\\end{document}\n% edit");
            let opts = BuildOptions {
                latexmk: &self.latexmk,
                bin_dir: Some(&self.bin),
                installer: None,
                miktex: false,
                perl_dir: None,
            };
            let (_tx, rx) = oneshot::channel();
            build(&self.project(), &opts, rx).await.unwrap()
        }
        fn used_format(&self) -> bool {
            format::was_used(
                &std::fs::read_to_string(self.project().log_path()).unwrap_or_default(),
            )
        }
        /// Waits for the background work to settle on the current preamble: usable or not.
        async fn verdict(&self) -> bool {
            for _ in 0..600 {
                if let Some(ok) = format::recorded(&self.project(), &self.bin.join("pdflatex")) {
                    return ok;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("no verdict on the precompiled preamble within a minute");
        }
        fn errors(out: &Outcome) -> Vec<&str> {
            out.diagnostics
                .iter()
                .filter(|d| d.severity == lemmata_log::Severity::Error)
                .map(|d| d.message.as_str())
                .collect()
        }
    }

    /// The case that once moved the title up by 12.7pt: \vspace* at the top of the first page.
    #[cfg(unix)]
    const TITLEPAGE: &str = "\\documentclass{article}\n\\PackageWarningNoLine{demo}{said in the preamble}\n\\begin{document}\n\\begin{titlepage}\\vspace*{3cm}{\\Huge Title\\par}\\end{titlepage}\nText.\n\\end{document}\n";

    #[cfg(unix)]
    #[tokio::test]
    async fn a_warm_rebuild_starts_from_the_checked_preamble() {
        let Some(r) = Real::new(TITLEPAGE, "") else {
            return;
        };
        let first = r.build().await;
        assert_eq!(first.status, Status::Success);
        assert!(!r.used_format(), "the first build has no format yet");
        assert!(r.verdict().await, "the title page comes out the same");
        let fmt = r.project().outdir_abs().join("fmt/main.fmt");
        assert!(fmt.is_file());
        let second = r.build().await;
        assert_eq!(second.status, Status::Success);
        assert!(second.pdf.is_some());
        assert!(r.used_format());
        // What the preamble said is still reported, though this run never read the preamble.
        assert!(
            second
                .diagnostics
                .iter()
                .any(|d| d.message.contains("said in the preamble")),
            "{:?}",
            second.diagnostics
        );
        assert_eq!(first.diagnostics.len(), second.diagnostics.len());
        // A deliberate Recompile builds without it.
        let opts = BuildOptions {
            latexmk: &r.latexmk,
            bin_dir: Some(&r.bin),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        r.edit("Text.", "More text.");
        let (_tx, rx) = oneshot::channel();
        let forced = build_with(&r.project(), &opts, rx, false, true)
            .await
            .unwrap();
        assert_eq!(forced.status, Status::Success);
        assert!(!r.used_format());
        // Clearing the cache removes it.
        clean(&r.project()).unwrap();
        assert!(!fmt.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn changing_the_preamble_or_a_file_it_reads_drops_the_format() {
        let Some(r) = Real::new(
            "\\documentclass{article}\n\\input{macros}\n\\begin{document}\n\\x\n\\end{document}\n",
            "",
        ) else {
            return;
        };
        std::fs::write(r.dir.path().join("macros.tex"), "\\newcommand\\x{one}\n").unwrap();
        assert_eq!(r.build().await.status, Status::Success);
        assert!(r.verdict().await);
        r.build().await;
        assert!(r.used_format());

        r.edit(
            "\\begin{document}",
            "\\usepackage{amsmath}\n\\begin{document}",
        );
        let out = r.build().await;
        assert_eq!(out.status, Status::Success);
        assert!(!r.used_format(), "the old format holds the old preamble");
        assert!(r.verdict().await, "a new one is made");
        r.build().await;
        assert!(r.used_format());

        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(r.dir.path().join("macros.tex"), "\\newcommand\\x{two}\n").unwrap();
        let out = r.build().await;
        assert_eq!(out.status, Status::Success);
        assert!(!r.used_format(), "macros.tex changed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_preamble_a_format_cannot_hold_is_never_used() {
        // Map lines are not kept in a format; an object written at once goes missing from the PDF.
        for preamble in [
            "\\pdfmapline{=cmr10 CMR10 <cmr10.pfb}",
            "\\immediate\\pdfobj{<< /Lemmata 1 >>}",
        ] {
            let Some(r) = Real::new(
                &format!(
                    "\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\nA\n\\end{{document}}\n"
                ),
                "",
            ) else {
                return;
            };
            assert_eq!(r.build().await.status, Status::Success);
            assert!(!r.verdict().await, "{preamble}");
            let out = r.build().await;
            assert_eq!(out.status, Status::Success);
            assert!(!r.used_format(), "{preamble}");
            assert!(!r.project().outdir_abs().join("fmt/main.fmt").exists());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn draft_and_xelatex_builds_never_use_a_format() {
        let doc = "\\documentclass{article}\n\\begin{document}\nA\n\\end{document}\n";
        let Some(r) = Real::new(doc, "") else {
            return;
        };
        r.build().await;
        assert!(r.verdict().await);
        std::fs::write(r.dir.path().join("lemmata.toml"), "draft = true\n").unwrap();
        let out = r.build().await;
        assert_eq!(out.status, Status::Success);
        assert!(!r.used_format(), "draft");

        if which::which("xelatex").is_err() {
            return;
        }
        let Some(r) = Real::new(doc, "engine = \"xelatex\"\n") else {
            return;
        };
        r.build().await;
        let out = r.build().await;
        assert_eq!(out.status, Status::Success);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!r.used_format());
        assert!(!r.project().outdir_abs().join("fmt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_broken_format_or_preamble_falls_back_cleanly() {
        let Some(r) = Real::new(TITLEPAGE, "") else {
            return;
        };
        r.build().await;
        assert!(r.verdict().await);
        // A format that dies: the build is redone without it, shows no error of its own, and drops it.
        let fmt = r.project().outdir_abs().join("fmt/main.fmt");
        std::fs::write(&fmt, "not a format").unwrap();
        let out = r.build().await;
        assert_eq!(out.status, Status::Success, "{:?}", Real::errors(&out));
        assert!(Real::errors(&out).is_empty(), "{:?}", Real::errors(&out));
        assert!(!r.used_format());
        assert_eq!(
            format::recorded(&r.project(), &r.bin.join("pdflatex")),
            Some(false)
        );

        // A preamble error is reported as without a format.
        r.edit(
            "\\begin{document}",
            "\\usepackage{lemmata-no-such-package}\n\\begin{document}",
        );
        let out = r.build().await;
        assert_eq!(out.status, Status::Failed);
        assert!(!r.used_format());
        assert!(
            out.diagnostics.iter().any(|d| matches!(
                &d.kind,
                lemmata_log::Kind::MissingFile { name } if name.contains("lemmata-no-such-package")
            )),
            "{:?}",
            out.diagnostics
        );
    }
}
