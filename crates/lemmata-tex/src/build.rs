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
    #[error("could not start {0}: {1}")]
    SpawnTool(String, std::io::Error),
    /// The project is set up in a way that could make a build act outside it.
    #[error("{0}")]
    Refused(String),
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
    args.push(main_arg(&project.main));
    args
}

/// The main file as a command-line argument. A file named `-shell-escape.tex` would be read as
/// an option (past `-norc`), `&x.tex` as a format and `\x.tex` as TeX code: `./` keeps each a file.
pub(crate) fn main_arg(main: &Path) -> OsString {
    let mut arg = OsString::new();
    if main.to_string_lossy().starts_with(['-', '&', '\\']) {
        arg.push("./");
    }
    arg.push(main);
    arg
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
    refuse_unsafe(project)?;
    let restricted = restrict(project);
    let project = restricted.as_ref().unwrap_or(project);
    let started = Instant::now();
    // What the build reads, once: for "nothing changed" and for the stamp it leaves.
    let tree = lemmata_project::tree(&project.root, &project.config.outdir);
    let files = files_hash_with_tree(project, &tree);
    // Nothing changed since the last good build: no TeX run at all, just its result again.
    if !force && let Some(o) = cached_outcome_with(project, started, &tree, &files) {
        return Ok(o);
    }
    let started_at = SystemTime::now();
    prepare_outdir(project);
    // With the slack in written_since, last build's log would look fresh; no log is the signal that latexmk never ran TeX.
    let _ = std::fs::remove_file(project.log_path());

    // latexmk's own output is the only explanation when it dies before TeX
    // writes a log (no Perl, a MiKTeX prompt), so keep it in a file.
    let output_path = project.outdir_abs().join("latexmk-output.txt");
    let limit = time_limit(project);
    let deadline = started + limit;
    let mut cancel = cancel;
    let engine = tool(opts, project.config.engine.binary());
    let can_go_direct = engine.is_some();
    // latexmk runs the project's latexmkrc and builds shell command lines from file names. A
    // project the user hasn't trusted, or one whose names a shell would act on, gets the engine
    // run directly instead, with its arguments passed as they are.
    let no_latexmk = !project.trusted
        || lemmata_project::shell_unsafe(&project.main.to_string_lossy())
        || lemmata_project::shell_unsafe(&project.config.outdir);
    if no_latexmk && !can_go_direct {
        return Err(Error::SpawnTool(
            project.config.engine.binary().into(),
            std::io::Error::other("not found"),
        ));
    }
    // Like latexmk's -f: past errors to the most complete PDF, unless the project asks to stop.
    let keep_going = no_latexmk && !project.config.stop_on_error;
    let warm = can_go_direct && (no_latexmk || !skip_latexmk) && warm_ok(project);
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
    if warm
        && let Ok(s) = direct_build(
            project,
            opts,
            deadline.saturating_duration_since(Instant::now()),
            &mut cancel,
            fmt.as_deref(),
            keep_going,
        )
        .await
    {
        status = s;
    }
    let fmt_failed = fmt.is_some() && status == Status::Failed;
    if fmt_failed {
        // Its errors could be the format's own: only a build without it gets to report them.
        let _ = std::fs::remove_file(project.log_path());
    }
    // A failed direct build is run again by latexmk, which keeps going past errors the way the user
    // chose. Without latexmk it already went on past them, unless the precompiled preamble failed.
    let settled = warm && (status != Status::Failed || (no_latexmk && !fmt_failed));
    if !settled && no_latexmk {
        status = direct_build(
            project,
            opts,
            deadline.saturating_duration_since(Instant::now()),
            &mut cancel,
            None,
            keep_going,
        )
        .await?;
    } else if !settled && !(skip_latexmk && can_go_direct) {
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
        status = match run_step(
            cmd,
            deadline.saturating_duration_since(Instant::now()),
            &mut cancel,
        )
        .await
        {
            Ok(s) => s.ok_or_else(|| {
                Error::SpawnTool("latexmk".into(), std::io::Error::other("could not start"))
            })?,
            // Blocked from starting (a Windows policy): the direct build below can still work.
            Err(Error::SpawnTool(_, e)) if can_go_direct => {
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
    if !no_latexmk && status == Status::Failed && !log_fresh && can_go_direct {
        match direct_build(
            project,
            opts,
            deadline.saturating_duration_since(Instant::now()),
            &mut cancel,
            None,
            !project.config.stop_on_error,
        )
        .await
        {
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
        let tail = tail_of(
            &String::from_utf8_lossy(&read_tail(&output_path, 16_000)),
            4000,
        );
        let mut note = lemmata_log::launcher::launcher_failure(&tail);
        note.severity = lemmata_log::Severity::Info;
        note.message = format!(
            "latexmk could not run, so Lemmata built with {} directly. {}",
            project.config.engine.binary(),
            note.message
        );
        diagnostics.push(note);
    }

    if !project.trusted {
        diagnostics.extend(restricted_notes(project));
    }

    // Failed without a single error in the log: latexmk never got to TeX.
    if status == Status::Failed
        && !diagnostics
            .iter()
            .any(|d| d.severity == lemmata_log::Severity::Error)
    {
        let tail = tail_of(
            &String::from_utf8_lossy(&read_tail(&output_path, 16_000)),
            4000,
        );
        diagnostics.insert(0, lemmata_log::launcher::launcher_failure(&tail));
    }

    if status == Status::Success && fresh {
        mark_ok(project, started_at, &files);
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

/// A project the user hasn't trusted never gets shell escape, whatever its lemmata.toml asks
/// for. Returns the changed copy when there is something to change.
pub(crate) fn restrict(project: &Project) -> Option<Project> {
    (!project.trusted && project.config.shell_escape).then(|| {
        let mut p = project.clone();
        p.config.shell_escape = false;
        p
    })
}

/// Refuses a project whose main file name holds control characters, or whose build folder (or a
/// folder on the way to it) is a link out of the project: the build would write where the link points.
pub(crate) fn refuse_unsafe(project: &Project) -> Result<(), Error> {
    if project.main.to_string_lossy().chars().any(char::is_control) {
        return Err(Error::Refused(
            "The main file's name holds control characters. Rename it to build the project.".into(),
        ));
    }
    if outdir_leaves_project(project) {
        return Err(Error::Refused(format!(
            "The build folder {} links to somewhere outside the project, so nothing is built there. Remove the link.",
            project.config.outdir
        )));
    }
    Ok(())
}

pub(crate) fn outdir_leaves_project(project: &Project) -> bool {
    let Ok(root) = lemmata_project::canonical(&project.root) else {
        return true;
    };
    let mut p = project.root.clone();
    for c in Path::new(&project.config.outdir).components() {
        if matches!(c, std::path::Component::CurDir) {
            continue;
        }
        p.push(c);
        match lemmata_project::canonical(&p) {
            Ok(real) if !real.starts_with(&root) || real == root => return true,
            Ok(_) => {}
            // A link to nowhere: creating the folder would follow it.
            Err(_) if p.symlink_metadata().is_ok() => return true,
            // Not there yet: prepare_outdir makes it inside the project.
            Err(_) => break,
        }
    }
    false
}

/// What a build of a project the user hasn't trusted leaves out, said once per build.
fn restricted_notes(project: &Project) -> Vec<Diagnostic> {
    let note = |message: &str| Diagnostic {
        severity: lemmata_log::Severity::Info,
        kind: lemmata_log::Kind::Other,
        file: None,
        line: None,
        message: message.into(),
        hint: Some("Trust the project in its settings if you know where it comes from.".into()),
        fix: None,
    };
    let mut notes = Vec::new();
    if lemmata_project::runs_programs(&project.root, &lemmata_project::Config::default()) {
        notes.push(note(
            "This project has a latexmkrc, which can run programs, so restricted builds leave it out.",
        ));
    }
    let index = std::fs::read_dir(project.outdir_abs()).is_ok_and(|d| {
        d.flatten().any(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| ["idx", "glo", "nlo", "acn"].contains(&x))
        })
    });
    if index {
        notes.push(note(
            "This document makes an index or glossary. Restricted builds don't run the tools that sort them.",
        ));
    }
    notes
}

/// The result of the last good build again, when nothing changed since; `tree` and `files`
/// (its `files_hash_with_tree`) are what this build reads.
pub(crate) fn cached_outcome_with(
    project: &Project,
    started: Instant,
    tree: &[lemmata_project::Entry],
    files: &str,
) -> Option<Outcome> {
    if !up_to_date_hashed(project, tree, files) {
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
    // The tool is rightly skipped when its input is unchanged: its log from the last run still holds.
    let biber = project.config.bib == lemmata_project::Bib::Biber;
    let blg_ok = written_since(&blg_path, started_at)
        || (blg_path.is_file()
            && bib_fingerprint(project, biber).is_some_and(|fp| {
                std::fs::read_to_string(out_file(project, "lemmata-bib")).is_ok_and(|s| s == fp)
            }));
    if blg_ok && let Ok(bytes) = lemmata_env::fs::read(&blg_path) {
        for d in lemmata_log::parse_blg(&String::from_utf8_lossy(&bytes)) {
            // A missing key is already reported from the main log.
            let known = matches!(d.kind, lemmata_log::Kind::UndefinedCitation { .. })
                && diagnostics.iter().any(|x| x.kind == d.kind);
            if !known {
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

/// The time a build may take: `timeout_secs`, where 0 (no limit, a day) is only for projects the
/// user trusts; a downloaded project can't make a runaway build fill the disk for a day.
/// How long a build may run. A day at most (0 asks for no limit); a project the user hasn't
/// trusted gets at most the default, whatever its lemmata.toml says.
pub(crate) fn time_limit(project: &Project) -> Duration {
    const DAY: u64 = 60 * 60 * 24;
    let default = lemmata_project::DEFAULT_TIMEOUT_SECS;
    Duration::from_secs(match project.config.timeout_secs {
        0 if project.trusted => DAY,
        n if project.trusted => n.min(DAY),
        0 => default,
        n => n.min(default),
    })
}

/// Logs bigger than this are read as their first and last half: a runaway document can write
/// gigabytes, and errors sit near the start or the end.
const LOG_CAP: u64 = 32 << 20;

/// A build file, or only its first and last part when it is huge.
pub(crate) fn read_capped(path: &Path) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    // A regular file only: a planted link to /dev/zero reports no length and never ends.
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return Vec::new();
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let mut out = Vec::new();
    if len <= LOG_CAP {
        let _ = f.take(LOG_CAP).read_to_end(&mut out);
        return out;
    }
    let half = LOG_CAP / 2;
    let _ = (&mut f).take(half).read_to_end(&mut out);
    out.extend_from_slice(b"\n[... the middle of a very long log is left out ...]\n");
    if f.seek(SeekFrom::End(-(half as i64))).is_ok() {
        let _ = f.take(half).read_to_end(&mut out);
    }
    out
}

/// The last `n` bytes of a file.
pub(crate) fn read_tail(path: &Path, n: u64) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return Vec::new();
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(n)));
    let mut out = Vec::new();
    let _ = f.take(n).read_to_end(&mut out);
    out
}

/// The diagnostics in the main log. A run from the precompiled preamble didn't read the preamble,
/// so what it said then comes from the log of the run that made the format.
pub(crate) fn read_log(project: &Project) -> Vec<Diagnostic> {
    let mut log = String::from_utf8_lossy(&read_capped(&project.log_path())).into_owned();
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
    let program = Path::new(cmd.as_std().get_program())
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(Error::SpawnTool(program, e)),
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

/// Beside `OK_STAMP`: a hash of every project file's path and bytes when the last good build
/// began. Content, not times: an autosave just before a build is in it, an edit during it isn't,
/// and a deleted file or one synced in with an old time still shows.
const FILES_STAMP: &str = ".lemmata-files";

#[cfg(test)]
pub(crate) fn files_hash(project: &Project) -> String {
    files_hash_with_tree(
        project,
        &lemmata_project::tree(&project.root, &project.config.outdir),
    )
}

pub(crate) fn files_hash_with_tree(
    project: &Project,
    entries: &[lemmata_project::Entry],
) -> String {
    use std::hash::{Hash, Hasher};
    // Figures and data files are big and aren't edited in the app: their size and time (to the
    // nanosecond) stand for their content. Sources, which an autosave rewrites right before a
    // build, are hashed by content.
    const BIG: u64 = 1 << 20;
    fn walk(root: &Path, entries: &[lemmata_project::Entry], h: &mut impl Hasher) {
        for e in entries {
            (&e.path, e.is_dir, e.size).hash(h);
            if e.is_dir {
                walk(root, &e.children, h);
                continue;
            }
            // What a link names decides, not the link: one to /dev/zero or a huge file isn't read.
            let path = root.join(&e.path);
            match std::fs::metadata(&path) {
                Ok(m) if m.is_file() && m.len() <= BIG => match std::fs::read(&path) {
                    Ok(bytes) => bytes.hash(h),
                    Err(e) => e.to_string().hash(h),
                },
                Ok(m) => (m.len(), m.modified().ok()).hash(h),
                Err(e) => e.to_string().hash(h),
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    walk(&project.root, entries, &mut h);
    format!("{:x}", h.finish())
}

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

/// The last build succeeded, its PDF and log are there, and every project file is as it was when it began.
pub fn up_to_date(project: &Project) -> bool {
    up_to_date_with_tree(
        project,
        &lemmata_project::tree(&project.root, &project.config.outdir),
    )
}

pub fn up_to_date_with_tree(project: &Project, entries: &[lemmata_project::Entry]) -> bool {
    // The cheap checks first: the hash reads every project file.
    let stamp = project.outdir_abs().join(OK_STAMP);
    let main = project.main.to_string_lossy();
    std::fs::read_to_string(&stamp).is_ok_and(|s| s == main)
        && project.pdf_path().is_file()
        && project.log_path().is_file()
        && up_to_date_hashed(project, entries, &files_hash_with_tree(project, entries))
}

fn up_to_date_hashed(project: &Project, entries: &[lemmata_project::Entry], files: &str) -> bool {
    let stamp = project.outdir_abs().join(OK_STAMP);
    let main = project.main.to_string_lossy();
    std::fs::read_to_string(&stamp).is_ok_and(|s| s == main)
        && project.pdf_path().is_file()
        && project.log_path().is_file()
        && std::fs::read_to_string(project.outdir_abs().join(FILES_STAMP)).is_ok_and(|s| s == files)
        && !others_changed(project, entries, &stamp)
}

/// Files the last build read (its .fls) that the project's hash doesn't cover: outside the
/// project (`\input{../common/macros}`, a .sty in TEXMFHOME), in a linked folder, or hidden.
/// Changed since the stamp's time, the PDF is out of date. By time, as these aren't hashed: at
/// worst one build too many.
fn others_changed(project: &Project, entries: &[lemmata_project::Entry], stamp: &Path) -> bool {
    let Ok(since) = std::fs::metadata(stamp).and_then(|m| m.modified()) else {
        return true;
    };
    let Ok(fls) = std::fs::read_to_string(out_file(project, "fls")) else {
        return false;
    };
    fn files(entries: &[lemmata_project::Entry], out: &mut std::collections::HashSet<String>) {
        for e in entries {
            if e.is_dir {
                files(&e.children, out);
            } else {
                out.insert(e.path.clone());
            }
        }
    }
    let mut hashed = std::collections::HashSet::new();
    files(entries, &mut hashed);
    let pwd = fls
        .lines()
        .find_map(|l| l.strip_prefix("PWD "))
        .map_or(project.root.clone(), PathBuf::from);
    let outdir = project.outdir_abs();
    let read: std::collections::HashSet<&str> = fls
        .lines()
        .filter_map(|l| l.strip_prefix("INPUT "))
        .collect();
    read.into_iter().any(|p| {
        let abs = pwd.join(p);
        let rel = abs.strip_prefix(&project.root).ok().map(|r| {
            r.to_string_lossy()
                .trim_start_matches("./")
                .replace('\\', "/")
        });
        if abs.starts_with(&outdir) || rel.as_deref().is_some_and(|r| hashed.contains(r)) {
            return false;
        }
        std::fs::metadata(&abs)
            .and_then(|m| m.modified())
            .is_ok_and(|t| t + MTIME_SLACK >= since)
    })
}

/// True when some project file was modified after the time on `stamp`, the start of the last good build
/// (or up to `MTIME_SLACK` before it: at worst one build too many, never an edit left out).
fn changed_since(project: &Project, stamp: &Path) -> bool {
    changed_since_with_tree(
        project,
        stamp,
        &lemmata_project::tree(&project.root, &project.config.outdir),
    )
}

fn changed_since_with_tree(
    project: &Project,
    stamp: &Path,
    entries: &[lemmata_project::Entry],
) -> bool {
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
    any_newer(&project.root, entries, since)
}

/// Records a successful build, and what the bibliography tool read, so the next one can go direct without running it again.
/// `files` is `files_hash` from when the build began.
pub(crate) fn mark_ok(project: &Project, started_at: SystemTime, files: &str) {
    let biber = project.config.bib == lemmata_project::Bib::Biber;
    // A .bib saved while the build ran was maybe not what the tool read: then it runs next time.
    let bib_edited = bib_files(project).iter().any(|e| {
        std::fs::metadata(project.root.join(&e.path))
            .and_then(|m| m.modified())
            .is_ok_and(|t| t + MTIME_SLACK >= started_at)
    });
    match bib_fingerprint(project, biber) {
        Some(fp) if !bib_edited => {
            let _ = std::fs::write(out_file(project, "lemmata-bib"), fp);
        }
        _ => {
            let _ = std::fs::remove_file(out_file(project, "lemmata-bib"));
        }
    }
    let stamp = project.outdir_abs().join(OK_STAMP);
    let _ = std::fs::write(&stamp, project.main.to_string_lossy().as_bytes());
    let _ = std::fs::write(project.outdir_abs().join(FILES_STAMP), files);
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
    // Only the format's name: with a folder in it, pdfTeX's -recorder writes into a folder of that
    // name under the output folder, which doesn't exist. The folder goes in TEXFORMATS (`texformats`).
    let mut args: Vec<String> = fmt
        .map(|f| {
            let name = Path::new(f)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            format!("-fmt={}", name.unwrap_or_else(|| f.to_string()))
        })
        .into_iter()
        .collect();
    args.extend(
        [
            "-interaction=nonstopmode",
            "-file-line-error",
            "-synctex=1",
            // The .fls list of files read and written, as latexmk asks for it: the arXiv export uses it.
            "-recorder",
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
    let main = main_arg(&project.main).to_string_lossy().replace('\\', "/");
    args.push(if c.draft {
        format!("\\PassOptionsToPackage{{draft}}{{graphicx}}\\input{{{main}}}")
    } else {
        main
    });
    args
}

/// TEXFORMATS for a run from the format `fmt` (as `pass_args` takes it): its folder, then the
/// usual places (an empty last entry).
pub(crate) fn texformats(fmt: &str) -> OsString {
    let dir = Path::new(fmt).parent().unwrap_or(Path::new("."));
    std::env::join_paths([dir, Path::new("")]).unwrap_or_default()
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
    // On past a pass that reported errors, as latexmk -f does.
    keep_going: bool,
) -> Result<Status, Error> {
    let engine = project.config.engine.binary();
    let engine_path = tool(opts, engine)
        .ok_or_else(|| Error::SpawnTool(engine.into(), std::io::Error::other("not found")))?;
    let deadline = Instant::now() + limit;
    let mut bib_checked = false;
    let mut status = Status::Failed;
    let mut failed = false;
    // This writes over latexmk's outputs without latexmk knowing: its record would later call a
    // stale PDF up to date (e.g. after undoing a broken edit) and skip the build.
    let _ = std::fs::remove_file(out_file(project, "fdb_latexmk"));

    for pass in 0..MAX_PASSES {
        let pass_started = SystemTime::now();
        let before = toc_state(project);
        let mut cmd = Command::new(&engine_path);
        cmd.args(pass_args(project, &project.config.outdir, fmt, opts.miktex));
        if let Some(f) = fmt {
            cmd.env("TEXFORMATS", texformats(f));
        }
        cmd.current_dir(&project.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        set_path(&mut cmd, opts);
        status = run_step(
            cmd,
            deadline.saturating_duration_since(Instant::now()),
            cancel,
        )
        .await?
        .unwrap_or(Status::Failed);
        let wrote_log = written_since(&out_file(project, "log"), pass_started);
        if status == Status::Failed && keep_going && wrote_log {
            failed = true;
        } else if status != Status::Success {
            return Ok(status);
        }

        let log = String::from_utf8_lossy(&read_capped(&out_file(project, "log"))).into_owned();
        let rerun = [
            "Rerun to get",
            "Please rerun LaTeX",
            "Label(s) may have changed",
            "Rerun LaTeX",
        ]
        .iter()
        .any(|m| log.contains(m));

        // The bibliography tool runs at most once, and only when what it reads has changed.
        // A contents list that changed in this pass needs another one to print it.
        let mut again = rerun || toc_state(project) != before;
        if !bib_checked && pass < MAX_PASSES - 1 {
            bib_checked = true;
            match run_bibliography(
                project,
                opts,
                deadline.saturating_duration_since(Instant::now()),
                cancel,
            )
            .await?
            {
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
    Ok(if failed { Status::Failed } else { status })
}

/// The contents, figure and table lists, which a pass writes for the next one to print.
fn toc_state(project: &Project) -> Vec<Vec<u8>> {
    ["toc", "lof", "lot"]
        .iter()
        .map(|e| read_capped(&out_file(project, e)))
        .collect()
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
        let mut aux = std::fs::read_to_string(mirror("aux")).ok()?;
        if !aux.contains("\\bibdata") {
            return None;
        }
        // \include sends its \citation lines to the chapter's own .aux, which main.aux names.
        let chapters: Vec<String> = aux
            .lines()
            .filter_map(|l| l.strip_prefix("\\@input{")?.strip_suffix('}'))
            .filter_map(lemmata_env::fs::inside_rel)
            .map(|n| {
                lemmata_env::fs::read_to_string(project.outdir_abs().join(n)).unwrap_or_default()
            })
            .collect();
        aux.push('\n');
        aux.push_str(&chapters.join("\n"));
        for line in aux.lines().filter(|l| {
            l.starts_with("\\citation") || l.starts_with("\\bibstyle") || l.starts_with("\\bibdata")
        }) {
            line.hash(&mut h);
        }
    }
    let bibs: Vec<String> = bib_files(project)
        .iter()
        .map(|e| format!("{}:{}:{}", e.path, e.size, e.modified))
        .collect();
    bibs.hash(&mut h);
    // The sources the tool read last time, as its log names them: a .bib outside the project
    // (`\addbibresource{~/library.bib}`) is not among the project's files.
    let blg = std::fs::read_to_string(mirror("blg")).unwrap_or_default();
    for line in blg.lines() {
        let named = line
            .split_once("Found BibTeX data source '")
            .and_then(|(_, r)| r.split_once('\''))
            .map(|(p, _)| p)
            .or_else(|| {
                line.strip_prefix("Database file #")
                    .and_then(|r| r.split_once(": "))
                    .map(|(_, p)| p.trim())
            });
        if let Some(p) = named {
            let path = [project.root.join(p), project.outdir_abs().join(p)]
                .into_iter()
                .find(|p| p.is_file());
            let stat = path
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| (m.len(), m.modified().ok()));
            (p, stat).hash(&mut h);
        }
    }
    Some(format!("{:x}", h.finish()))
}

/// The project's .bib files.
fn bib_files(project: &Project) -> Vec<lemmata_project::Entry> {
    fn walk(entries: Vec<lemmata_project::Entry>, out: &mut Vec<lemmata_project::Entry>) {
        for e in entries {
            if e.is_dir {
                walk(e.children, out);
            } else if e.path.ends_with(".bib") {
                out.push(e);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        lemmata_project::tree(&project.root, &project.config.outdir),
        &mut out,
    );
    out
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
        // From the build folder, as latexmk does: the chapters' .aux files are named relative to it.
        ("bibtex", vec![stem])
    };
    let Some(path) = helper(opts, name) else {
        return Ok(Bib::Skipped);
    };
    let before = std::fs::read(&bbl).ok();
    let mut cmd = Command::new(path);
    cmd.args(args)
        .current_dir(if biber {
            project.root.clone()
        } else {
            project.outdir_abs()
        })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    set_path(&mut cmd, opts);
    if !biber {
        // The project's .bib and .bst files, then the user's own folders and the usual places.
        for var in ["BIBINPUTS", "BSTINPUTS"] {
            let mut dirs = vec![project.root.clone()];
            if let Some(old) = std::env::var_os(var) {
                dirs.extend(std::env::split_paths(&old).filter(|d| !d.as_os_str().is_empty()));
            }
            dirs.push(PathBuf::new());
            cmd.env(var, std::env::join_paths(dirs).unwrap_or_default());
        }
    }
    // A failing bibliography run shows up in the .blg; the next pass still matters.
    // A tool that cannot start (blocked by a policy) is like a missing one: the PDF is still built.
    let Ok(status) = run_step(cmd, limit, cancel).await else {
        let _ = std::fs::remove_file(&stamp);
        return Ok(Bib::Skipped);
    };
    if let Some(s @ (Status::Cancelled | Status::Timeout)) = status {
        return Ok(Bib::Stopped(s));
    }
    let succeeded = matches!(status, Some(Status::Success));
    if succeeded {
        let _ = std::fs::write(&stamp, &fingerprint);
    }
    Ok(Bib::Ran {
        changed: !succeeded || std::fs::read(&bbl).ok() != before,
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
    // A project can bring `.lemmata` or the build folder as a link to somewhere else.
    let inside = lemmata_project::canonical(&outdir).is_ok_and(|o| {
        lemmata_project::canonical(&project.root).is_ok_and(|r| o.starts_with(&r) && o != r)
    });
    if !inside {
        return Err(std::io::Error::other(
            "the build folder links to somewhere outside the project, so it is left alone",
        ));
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
    // Only a folder Lemmata made (or found empty) is marked as its own; someone's data folder is not.
    let ours = std::fs::read_dir(&outdir).is_ok_and(|mut d| d.next().is_none()) || !outdir.exists();
    let _ = std::fs::create_dir_all(&outdir);
    if ours {
        let _ = std::fs::write(outdir.join(MARKER), "");
    }
    // A TeX run stopped part way leaves its file list as `pdflatex12345.fls`; one per cancel.
    for e in std::fs::read_dir(&outdir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let stray = name
            .strip_suffix(".fls")
            .and_then(|n| {
                ["pdflatex", "xelatex", "lualatex"]
                    .iter()
                    .find_map(|p| n.strip_prefix(p))
            })
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
        if stray {
            let _ = std::fs::remove_file(e.path());
        }
    }
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
pub(crate) fn kill_tree(pid: Option<u32>, force: bool) {
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
    /// A test that needs real TeX skips without it, unless LEMMATA_REQUIRE_TEX is set (CI's
    /// TeX job), where skipping would hide a broken install.
    fn tex_missing(what: &str) {
        assert!(
            std::env::var_os("LEMMATA_REQUIRE_TEX").is_none(),
            "LEMMATA_REQUIRE_TEX is set, but {what} is missing"
        );
    }

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
    fn huge_logs_and_endless_builds_are_bounded() {
        let d = tempfile::tempdir().unwrap();
        let big = d.path().join("big.log");
        let mut text = b"! First error.\n".to_vec();
        text.resize(LOG_CAP as usize + 1000, b'x');
        text.extend_from_slice(b"\n! Last error.\n");
        std::fs::write(&big, &text).unwrap();
        let read = read_capped(&big);
        assert!(read.len() <= LOG_CAP as usize + 100);
        assert!(read.starts_with(b"! First error.") && read.ends_with(b"! Last error.\n"));
        assert_eq!(read_tail(&big, 6), b"rror.\n");

        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.config.timeout_secs = 0;
        assert_eq!(
            time_limit(&p),
            Duration::from_secs(lemmata_project::DEFAULT_TIMEOUT_SECS),
            "not a day for a downloaded project"
        );
        p.trusted = true;
        assert_eq!(time_limit(&p), Duration::from_secs(60 * 60 * 24));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_timeout_covers_all_engine_passes() {
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
        let latexmk = script("latexmk", "exit 0\n");
        script(
            "pdflatex",
            "sleep 0.4\nout=.lemmata/build\nfor a in \"$@\"; do case $a in -output-directory=*) out=${a#-output-directory=};; esac; done\necho 'Label(s) may have changed. Rerun to get cross-references right.' > $out/main.log\necho pdf > $out/main.pdf\n",
        );
        let mut p = Project::open(d.path()).unwrap();
        p.config.timeout_secs = 1;
        let (_cancel, rx) = oneshot::channel();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let started = Instant::now();
        let out = build_with(&p, &opts, rx, true, false).await.unwrap();
        assert_eq!(
            out.status,
            Status::Timeout,
            "the whole build must stop at one second"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
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
    fn a_main_file_named_like_an_option_stays_a_file() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.main = "-shell-escape.tex".into();
        let last = |a: Vec<OsString>| a.last().unwrap().to_string_lossy().into_owned();
        assert_eq!(last(latexmk_args(&p, false)), "./-shell-escape.tex");
        assert_eq!(
            last(crate::tectonic::tectonic_args(&p)),
            "./-shell-escape.tex"
        );
        assert_eq!(
            last(
                pass_args(&p, "out", None, false)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            ),
            "./-shell-escape.tex"
        );
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
            let mut p = Project::open(self.dir.path()).unwrap();
            p.trusted = true; // these exercise the latexmk path
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
            tex_missing("pdflatex, latexmk or biber");
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
            tex_missing("pdflatex, latexmk or biber");
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
        let first = build(&p, &opts, rx).await.unwrap();
        assert_eq!(first.status, Status::Success, "{:?}", first.diagnostics);
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
            &files_hash(&p),
        );
        assert!(!changed_since(&p, &stamp));
        // Saved while the build ran, which then finished.
        let started = SystemTime::now();
        std::fs::write(p.root.join(&p.main), "\\documentclass{article}% edit").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        mark_ok(&p, started, &files_hash(&p));
        assert!(changed_since(&p, &stamp));
    }

    #[test]
    fn a_save_just_before_a_build_is_in_its_pdf_and_one_during_it_is_not() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(p.pdf_path(), "pdf").unwrap();
        std::fs::write(p.log_path(), "log").unwrap();
        // Autosaved a moment before Ctrl+S built: well inside the times' slack.
        std::fs::write(p.root.join(&p.main), "\\documentclass{article}% a").unwrap();
        let (started, files) = (SystemTime::now(), files_hash(&p));
        mark_ok(&p, started, &files);
        assert!(
            up_to_date(&p),
            "a save just before the build counted as unbuilt"
        );
        // Saved while it ran.
        let (started, files) = (SystemTime::now(), files_hash(&p));
        std::fs::write(p.root.join(&p.main), "\\documentclass{article}% b").unwrap();
        mark_ok(&p, started, &files);
        assert!(
            !up_to_date(&p),
            "an edit made during the build counted as built"
        );
    }

    #[test]
    fn a_deleted_or_old_dated_file_makes_the_pdf_stale() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("ch.tex"), "x").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(p.pdf_path(), "pdf").unwrap();
        std::fs::write(p.log_path(), "log").unwrap();
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
            &files_hash(&p),
        );
        assert!(up_to_date(&p));
        std::fs::remove_file(d.path().join("ch.tex")).unwrap();
        assert!(!up_to_date(&p), "a deleted include");
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
            &files_hash(&p),
        );
        assert!(up_to_date(&p));
        // Unzipped or synced in, keeping the time it had elsewhere.
        let old = SystemTime::now() - Duration::from_secs(86_400);
        let f = std::fs::File::create(d.path().join("ch.tex")).unwrap();
        f.set_modified(old).unwrap();
        assert!(!up_to_date(&p), "an old-dated new file");
    }

    /// A figure over 1 MB counts by size and time, not by reading it: replaced by one of the same
    /// size, its new time still makes the PDF stale.
    #[test]
    fn a_changed_big_figure_makes_the_pdf_stale() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let fig = d.path().join("photo.png");
        std::fs::write(&fig, vec![1u8; 2 << 20]).unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(p.pdf_path(), "pdf").unwrap();
        std::fs::write(p.log_path(), "log").unwrap();
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
            &files_hash(&p),
        );
        assert!(up_to_date(&p));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&fig, vec![2u8; 2 << 20]).unwrap();
        assert!(!up_to_date(&p), "a replaced figure was taken as unchanged");
    }

    /// A project's link to an endless file (/dev/zero) or a huge one is hashed by what it names,
    /// not read to the end.
    #[cfg(unix)]
    #[test]
    fn a_link_to_an_endless_file_is_not_read() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::os::unix::fs::symlink("/dev/zero", d.path().join("data.csv")).unwrap();
        let p = Project::open(d.path()).unwrap();
        let started = std::time::Instant::now();
        assert_eq!(files_hash(&p), files_hash(&p));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn same_size_and_timestamp_content_is_still_stale() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("main.tex");
        std::fs::write(&file, "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(p.pdf_path(), "pdf").unwrap();
        std::fs::write(p.log_path(), "log").unwrap();
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
            &files_hash(&p),
        );
        let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::fs::write(&file, "\\documentclass{articlX}").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(!up_to_date(&p), "different bytes were accepted as current");
    }

    #[test]
    fn a_bib_saved_during_a_build_runs_bibtex_again() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("r.bib"), "@misc{k, title={K}}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        // What bibtex or biber read, whichever the project uses.
        std::fs::write(out_file(&p, "aux"), "\\citation{k}\n\\bibdata{r}\n").unwrap();
        std::fs::write(out_file(&p, "bcf"), "<bcf/>").unwrap();
        let fp = || std::fs::read_to_string(out_file(&p, "lemmata-bib")).ok();
        mark_ok(
            &p,
            SystemTime::now() + MTIME_SLACK + Duration::from_millis(50),
            &files_hash(&p),
        );
        assert!(fp().is_some());
        let started = SystemTime::now();
        std::fs::write(d.path().join("r.bib"), "@misc{k, title={Changed}}").unwrap();
        mark_ok(&p, started, &files_hash(&p));
        assert_eq!(fp(), None, "the next build must not skip bibtex");
    }

    /// A \cite in an \include'd chapter lands in the chapter's .aux, which main.aux only names.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_cite_in_an_included_chapter_runs_bibtex_from_the_build_folder() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        p.config.bib = lemmata_project::Bib::Bibtex;
        std::fs::create_dir_all(p.outdir_abs().join("chapters")).unwrap();
        std::fs::write(
            out_file(&p, "aux"),
            "\\@input{chapters/a.aux}\n\\bibdata{r}\n\\bibstyle{plain}\n",
        )
        .unwrap();
        let chapter = p.outdir_abs().join("chapters/a.aux");
        std::fs::write(&chapter, "\\citation{k1}\n").unwrap();
        let fp1 = bib_fingerprint(&p, false);
        std::fs::write(&chapter, "\\citation{k1,k2}\n").unwrap();
        assert_ne!(bib_fingerprint(&p, false), fp1);

        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let bibtex = tools.join("bibtex");
        std::fs::write(&bibtex, "#!/bin/sh\npwd > cwd\necho \"$@\" > args\n").unwrap();
        std::fs::set_permissions(&bibtex, std::fs::Permissions::from_mode(0o755)).unwrap();
        let opts = BuildOptions {
            latexmk: &tools,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_keep, mut rx) = oneshot::channel();
        run_bibliography(&p, &opts, Duration::from_secs(5), &mut rx)
            .await
            .unwrap();
        let out = p.outdir_abs();
        assert_eq!(std::fs::read_to_string(out.join("args")).unwrap().trim(), "main");
        assert!(out.join("cwd").is_file(), "bibtex ran in the build folder");
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
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true; // these exercise the latexmk path
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
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true; // these exercise the latexmk path
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

    /// Without a working latexmk the engine still goes on past a nonfatal error, so references resolve.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_latexmk_fallback_goes_on_past_a_failed_pass() {
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
        let latexmk = script("latexmk", "exit 1\n");
        script(
            "pdflatex",
            "out=.lemmata/build\necho x >> passes\nif [ $(wc -l < passes) -eq 1 ]; then echo 'Label(s) may have changed.' > $out/main.log; echo pdf > $out/main.pdf; exit 1; fi\necho done > $out/main.log\necho pdf > $out/main.pdf\n",
        );
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true;
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_tx, rx) = oneshot::channel();
        build_with(&p, &opts, rx, false, false).await.unwrap();
        let passes = std::fs::read_to_string(d.path().join("passes")).unwrap();
        assert_eq!(passes.lines().count(), 2, "went on past the failed pass");
    }

    /// A pass that writes a new contents list, or a log asking for "Rerun LaTeX.", gets another pass.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_changed_contents_list_or_a_rerun_request_runs_the_engine_again() {
        use std::os::unix::fs::PermissionsExt;
        // (what the engine writes to the log, a contents list from an earlier build, passes expected)
        let cases = [
            ("ok", None, 2),
            ("ok", Some("a"), 1),
            ("Table widths have changed. Rerun LaTeX.", Some("a"), 2),
        ];
        for (log, old_toc, passes) in cases {
            let d = tempfile::tempdir().unwrap();
            std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
            let tools = d.path().join("tools");
            std::fs::create_dir_all(&tools).unwrap();
            let engine = tools.join("pdflatex");
            // The second pass of the "rerun" case is silent, so it ends the build.
            std::fs::write(
                &engine,
                format!(
                    "#!/bin/sh\nout=.lemmata/build\necho x >> passes\nprintf a > $out/main.toc\necho pdf > $out/main.pdf\nif [ $(wc -l < passes) -eq 1 ]; then echo '{log}' > $out/main.log; else echo ok > $out/main.log; fi\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
            let p = Project::open(d.path()).unwrap();
            assert!(!p.trusted);
            std::fs::create_dir_all(p.outdir_abs()).unwrap();
            if let Some(t) = old_toc {
                std::fs::write(out_file(&p, "toc"), t).unwrap();
            }
            let opts = BuildOptions {
                latexmk: &tools,
                bin_dir: Some(&tools),
                installer: None,
                miktex: false,
                perl_dir: None,
            };
            let (_tx, rx) = oneshot::channel();
            build_with(&p, &opts, rx, false, false).await.unwrap();
            let n = std::fs::read_to_string(d.path().join("passes")).unwrap().lines().count();
            assert_eq!(n, passes, "{log:?} with toc {old_toc:?}");
        }
    }

    /// An untrusted project never reaches latexmk (it runs a project's latexmkrc and builds shell
    /// command lines), and a pass that reports errors doesn't stop the build, as latexmk -f wouldn't.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_untrusted_project_is_built_without_latexmk_and_past_errors() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("latexmkrc"), "").unwrap();
        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let script = |name: &str, body: &str| {
            let p = tools.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        let latexmk = script("latexmk", "touch latexmk-ran\n");
        // Each pass appends to `passes`; the first asks for a rerun and fails, like an error in nonstop mode.
        script(
            "pdflatex",
            "out=.lemmata/build\nfor a in \"$@\"; do case $a in -output-directory=*) out=${a#-output-directory=};; esac; done\necho x >> passes\nif [ $(wc -l < passes) -eq 1 ]; then echo 'Label(s) may have changed.' > $out/main.log; echo pdf > $out/main.pdf; exit 1; fi\necho done > $out/main.log\necho pdf > $out/main.pdf\n",
        );
        let p = Project::open(d.path()).unwrap();
        assert!(!p.trusted);
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_tx, rx) = oneshot::channel();
        let out = build_with(&p, &opts, rx, false, false).await.unwrap();
        assert!(!d.path().join("latexmk-ran").exists(), "latexmk ran");
        let passes = std::fs::read_to_string(d.path().join("passes")).unwrap();
        assert_eq!(passes.lines().count(), 2, "went on past the failed pass");
        assert_eq!(
            out.status,
            Status::Failed,
            "a pass with errors still counts"
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|x| x.message.contains("latexmkrc")
                    && x.severity == lemmata_log::Severity::Info),
            "says why the latexmkrc was left out: {:?}",
            out.diagnostics
        );
    }

    #[test]
    fn build_time_limits_are_bounded() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        let limit = |p: &Project, secs: u64| {
            let mut p = p.clone();
            p.config.timeout_secs = secs;
            time_limit(&p).as_secs()
        };
        let day = 60 * 60 * 24;
        assert_eq!(limit(&p, u64::MAX), lemmata_project::DEFAULT_TIMEOUT_SECS);
        assert_eq!(limit(&p, 0), lemmata_project::DEFAULT_TIMEOUT_SECS);
        assert_eq!(limit(&p, 10), 10);
        p.trusted = true;
        assert_eq!(limit(&p, u64::MAX), day, "no overflow past the deadline");
        assert_eq!(limit(&p, 0), day);
        assert_eq!(limit(&p, 3600), 3600);
    }

    #[test]
    fn an_untrusted_project_never_gets_shell_escape() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(d.path().join("lemmata.toml"), "shell_escape = true\n").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        assert!(p.config.shell_escape);
        let r = restrict(&p).expect("a restricted copy");
        assert!(
            pass_args(&r, ".lemmata/build", None, false).contains(&"-no-shell-escape".to_string())
        );
        p.trusted = true;
        assert!(
            restrict(&p).is_none(),
            "a trusted project keeps what it asks for"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_build_folder_that_links_out_of_the_project_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), d.path().join(".lemmata")).unwrap();
        let p = Project::open(d.path()).unwrap();
        assert!(outdir_leaves_project(&p));
        let latexmk = d.path().join("no-latexmk");
        std::fs::write(&latexmk, "").unwrap();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: None,
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_tx, rx) = oneshot::channel();
        let e = build_with(&p, &opts, rx, false, true).await.unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");
        assert!(
            std::fs::read_dir(elsewhere.path())
                .unwrap()
                .next()
                .is_none(),
            "wrote outside"
        );
        // A link that stays inside the project is fine.
        std::fs::remove_file(d.path().join(".lemmata")).unwrap();
        std::fs::create_dir_all(d.path().join("real")).unwrap();
        std::os::unix::fs::symlink(d.path().join("real"), d.path().join(".lemmata")).unwrap();
        assert!(!outdir_leaves_project(&Project::open(d.path()).unwrap()));
    }

    /// With real TeX: a build stopped after its direct pass leaves a newer PDF and file list but
    /// no log; the next build must still run TeX and report the error, not take them as current.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_error_after_a_stopped_build_is_reported() {
        let (Ok(pdflatex), Ok(latexmk)) = (which::which("pdflatex"), which::which("latexmk"))
        else {
            tex_missing("pdflatex, latexmk or biber");
            return; // no TeX on this machine: the CI job with TeX runs this
        };
        let bin = pdflatex.parent().unwrap().to_path_buf();
        let d = tempfile::tempdir().unwrap();
        let main = d.path().join("main.tex");
        std::fs::write(
            &main,
            "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n",
        )
        .unwrap();
        let opts = BuildOptions {
            latexmk: &latexmk,
            bin_dir: Some(&bin),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true;
        p.config.fast_preamble = false;
        let (_tx, rx) = oneshot::channel();
        assert_eq!(
            build_with(&p, &opts, rx, false, false)
                .await
                .unwrap()
                .status,
            Status::Success
        );
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(
            &main,
            "\\documentclass{article}\n\\begin{document}\nx\\notacommand\n\\end{document}\n",
        )
        .unwrap();
        // A build stopped after its direct pass: a newer PDF and file list, no log, no stamp.
        let pass = std::process::Command::new(&pdflatex)
            .args(pass_args(&p, &p.config.outdir, None, false))
            .current_dir(d.path())
            .output()
            .unwrap();
        assert!(!pass.status.success());
        let _ = std::fs::remove_file(p.log_path());
        let _ = std::fs::remove_file(p.outdir_abs().join(OK_STAMP));
        let (_tx, rx) = oneshot::channel();
        let out = build_with(&p, &opts, rx, false, false).await.unwrap();
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.severity == lemmata_log::Severity::Error),
            "{:?} {:?}",
            out.status,
            out.diagnostics
        );
    }

    /// With real TeX: names a shell would act on are passed to the engine as they are, trusted or
    /// not, and ordinary names with umlauts and spaces build in both modes.
    #[cfg(unix)]
    #[tokio::test]
    async fn file_names_never_reach_a_shell_and_unusual_names_build() {
        let (Ok(pdflatex), Ok(latexmk)) = (which::which("pdflatex"), which::which("latexmk"))
        else {
            tex_missing("pdflatex, latexmk or biber");
            return; // no TeX on this machine: the CI job with TeX runs this
        };
        let bin = pdflatex.parent().unwrap().to_path_buf();
        let doc = "\\documentclass{article}\\begin{document}x\\end{document}\n";
        for trusted in [false, true] {
            for (main, outdir) in [
                ("a`touch marker`.tex", ".lemmata/build"),
                ("main.tex", "$(touch marker)"),
                ("Einführung.tex", ".lemmata/build"),
                ("my chapter.tex", "out dir"),
            ] {
                let d = tempfile::tempdir().unwrap();
                std::fs::write(d.path().join(main), doc).unwrap();
                std::fs::write(
                    d.path().join("lemmata.toml"),
                    format!("main = {main:?}\noutdir = {outdir:?}\n"),
                )
                .unwrap();
                let mut p = Project::open(d.path()).unwrap();
                assert_eq!(
                    p.config.outdir, outdir,
                    "{outdir:?} is a folder name like any other"
                );
                p.trusted = trusted;
                let opts = BuildOptions {
                    latexmk: &latexmk,
                    bin_dir: Some(&bin),
                    installer: None,
                    miktex: false,
                    perl_dir: None,
                };
                let (_tx, rx) = oneshot::channel();
                let out = build_with(&p, &opts, rx, false, true).await.unwrap();
                assert!(
                    !d.path().join("marker").exists(),
                    "{main} / {outdir}: a shell ran it"
                );
                assert_eq!(
                    out.status,
                    Status::Success,
                    "{main} / {outdir} (trusted {trusted}): {:?}",
                    out.diagnostics
                );
                assert!(out.pdf.is_some_and(|p| p.is_file()), "{main} / {outdir}");
            }
        }
    }

    /// With real TeX: the direct build needs biber and a second pass to resolve
    /// \ref and \cite, and still reports what really is undefined.
    #[cfg(unix)]
    #[tokio::test]
    async fn direct_build_resolves_references_and_citations() {
        use std::os::unix::fs::PermissionsExt;
        let (Ok(pdflatex), Ok(_), true) = (which::which("pdflatex"), which::which("biber"), true)
        else {
            tex_missing("pdflatex or biber");
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
            tex_missing("pdflatex, latexmk or biber");
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
        assert!(
            first.pdf.is_some(),
            "{:?} {:?}",
            first.status,
            first.diagnostics
        );
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

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_bibliography_run_is_not_cached() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(out_file(&p, "bcf"), "<bcf/>").unwrap();
        std::fs::write(out_file(&p, "bbl"), "old").unwrap();
        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let biber = tools.join("biber");
        std::fs::write(&biber, "#!/bin/sh\necho x >> calls\nexit 1\n").unwrap();
        std::fs::set_permissions(&biber, std::fs::Permissions::from_mode(0o755)).unwrap();
        let opts = BuildOptions {
            latexmk: &tools,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let (_keep, mut rx) = oneshot::channel();
        for _ in 0..2 {
            let _ = run_bibliography(&p, &opts, Duration::from_secs(1), &mut rx)
                .await
                .unwrap();
        }
        assert!(
            !out_file(&p, "lemmata-bib").exists(),
            "failure was stamped as success"
        );
        assert_eq!(
            std::fs::read_to_string(d.path().join("calls"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    /// A bibliography tool that cannot start is named in the error and does not stop the build.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_bibliography_tool_that_cannot_start_is_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let (_keep, mut rx) = oneshot::channel();
        let err = run_step(
            Command::new("/nonexistent/biber"),
            Duration::from_secs(1),
            &mut rx,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("biber") && !err.contains("latexmk"), "{err}");

        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d.path()).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        std::fs::write(out_file(&p, "bcf"), "<bcf/>").unwrap();
        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let biber = tools.join("biber");
        std::fs::write(&biber, "").unwrap(); // not executable
        std::fs::set_permissions(&biber, std::fs::Permissions::from_mode(0o644)).unwrap();
        let opts = BuildOptions {
            latexmk: &tools,
            bin_dir: Some(&tools),
            installer: None,
            miktex: false,
            perl_dir: None,
        };
        let r = run_bibliography(&p, &opts, Duration::from_secs(1), &mut rx).await;
        assert!(matches!(r, Ok(Bib::Skipped)));
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
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true; // these exercise the latexmk path
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

    #[cfg(unix)]
    #[test]
    fn clean_refuses_a_build_folder_that_links_outside() {
        let d = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("thesis.log"), "keep me").unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::create_dir(d.path().join(".lemmata")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), d.path().join(".lemmata/build")).unwrap();
        let p = Project::open(d.path()).unwrap();
        assert!(clean(&p).is_err());
        assert!(elsewhere.path().join("thesis.log").exists());
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
        // A build into someone's folder does not make it Lemmata's.
        prepare_outdir(&p);
        assert!(clean(&p).is_err());
        assert!(p.outdir_abs().join("mine.log").exists());
        std::fs::remove_file(p.outdir_abs().join("mine.log")).unwrap();
        prepare_outdir(&p);
        std::fs::write(p.outdir_abs().join("main.log"), "").unwrap();
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
        let mut p = Project::open(d.path()).unwrap();
        p.trusted = true; // these exercise the latexmk path
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

    fn blg_project(d: &Path) -> Project {
        std::fs::write(d.join("main.tex"), "\\documentclass{article}").unwrap();
        let p = Project::open(d).unwrap();
        std::fs::create_dir_all(p.outdir_abs()).unwrap();
        p
    }

    fn undefined_cite() -> Diagnostic {
        Diagnostic {
            severity: lemmata_log::Severity::Warning,
            kind: lemmata_log::Kind::UndefinedCitation { key: "k".into() },
            file: None,
            line: None,
            message: "Citation `k' undefined".into(),
            hint: None,
            fix: None,
        }
    }

    #[test]
    fn a_bib_error_is_kept_next_to_any_other_warning() {
        let d = tempfile::tempdir().unwrap();
        let p = blg_project(d.path());
        std::fs::write(
            out_file(&p, "blg"),
            "ERROR - BibTeX subsystem: Cannot find 'refs.bib'",
        )
        .unwrap();
        let mut diags = vec![Diagnostic {
            kind: lemmata_log::Kind::Other,
            message: "There were undefined references.".into(),
            ..undefined_cite()
        }];
        merge_blg(&p, &mut diags, SystemTime::now() - Duration::from_secs(60));
        assert!(diags.iter().any(|x| x.message.contains("refs.bib")), "{diags:?}");
    }

    #[test]
    fn the_blg_of_a_skipped_biber_run_is_still_read() {
        let d = tempfile::tempdir().unwrap();
        let mut p = blg_project(d.path());
        p.config.bib = lemmata_project::Bib::Biber;
        std::fs::write(out_file(&p, "bcf"), "cites").unwrap();
        std::fs::write(out_file(&p, "blg"), "WARN - I didn't find a database entry for 'k'")
            .unwrap();
        let old = SystemTime::now() - Duration::from_secs(600);
        let run = |p: &Project| {
            let mut diags = vec![undefined_cite()];
            merge_blg(p, &mut diags, SystemTime::now());
            diags
        };
        std::fs::File::options()
            .write(true)
            .open(out_file(&p, "blg"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        // No stamp: biber did not run on this input.
        let diags = run(&p);
        assert!(diags[0].hint.as_deref().unwrap().contains("did not run"));
        std::fs::write(
            out_file(&p, "lemmata-bib"),
            bib_fingerprint(&p, true).unwrap(),
        )
        .unwrap();
        let diags = run(&p);
        assert!(!diags[0].hint.clone().unwrap_or_default().contains("did not run"));
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
        p.trusted = true; // these exercise the latexmk path
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
                tex_missing("pdflatex or latexmk");
                return None;
            };
            let has_mylatexformat = std::process::Command::new("kpsewhich")
                .arg("mylatexformat.ltx")
                .output()
                .is_ok_and(|o| !o.stdout.is_empty());
            if !has_mylatexformat {
                tex_missing("mylatexformat");
                return None;
            }
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("main.tex"), main).unwrap();
            std::fs::write(dir.path().join("lemmata.toml"), toml).unwrap();
            let bin = pdflatex.parent().unwrap().to_path_buf();
            Some(Real { dir, latexmk, bin })
        }
        fn project(&self) -> Project {
            let mut p = Project::open(self.dir.path()).unwrap();
            p.trusted = true;
            p
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
