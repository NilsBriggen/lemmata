//! Tectonic, an optional engine for people without a TeX distribution. It is
//! one self-contained program that fetches the packages a document needs the
//! first time it needs them. Lemmata downloads the pinned release itself.
//!
//! Builds run `tectonic -X compile` once: Tectonic does its own reruns and runs
//! bibtex itself. biblatex with biber needs a separate biber, which only works
//! if its version matches Tectonic's bundled biblatex (TeX Live 2022).

use crate::build::{
    Error, OK_STAMP, Outcome, Status, cached_outcome, mark_ok, merge_blg, prepare_outdir, read_log,
    tail_of, written_since,
};
use lemmata_log::{Diagnostic, Kind, Severity};
use lemmata_project::Project;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};
use tokio::process::Command;
use tokio::sync::oneshot;

pub const VERSION: &str = "0.17.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Archive {
    TarGz,
    Zip,
}

/// One pinned download. The checksum is of the archive as published on GitHub.
#[derive(Debug)]
pub struct Release {
    pub target: &'static str,
    pub sha256: &'static str,
    /// Bytes, for the "~N MB" shown to the user.
    pub size: u64,
    archive: Archive,
}

impl Release {
    pub fn url(&self) -> String {
        let ext = match self.archive {
            Archive::TarGz => "tar.gz",
            Archive::Zip => "zip",
        };
        format!(
            "https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40{VERSION}/tectonic-{VERSION}-{}.{ext}",
            self.target
        )
    }
}

const LINUX_X64: Release = Release {
    target: "x86_64-unknown-linux-musl",
    sha256: "8533d07f9ccbd7a65824b9e0459041bca34af1eb33daba48f59215593753a3b7",
    size: 10_151_914,
    archive: Archive::TarGz,
};
const LINUX_ARM64: Release = Release {
    target: "aarch64-unknown-linux-musl",
    sha256: "b10954a95404f3ab2328d2fa59a5ebab8e657f893fab096f98be8db7c0c979b8",
    size: 9_926_834,
    archive: Archive::TarGz,
};
const MAC_X64: Release = Release {
    target: "x86_64-apple-darwin",
    sha256: "7c90ef5b6ddb1eb1937e4337add5237b79338e4b9676459fa91187d24d6cdf80",
    size: 21_790_177,
    archive: Archive::TarGz,
};
const MAC_ARM64: Release = Release {
    target: "aarch64-apple-darwin",
    sha256: "a3f1cac7c5678f01661a92212f58480ae3b0634115d880dbc59e2953ded45667",
    size: 21_704_674,
    archive: Archive::TarGz,
};
// There is no Windows ARM build; Windows runs the x64 one under emulation.
const WINDOWS_X64: Release = Release {
    target: "x86_64-pc-windows-msvc",
    sha256: "f61ce51f0b0ade1015b7de7ef368541c5424e9756ecbd0d7af97d6d48030845f",
    size: 21_060_223,
    archive: Archive::Zip,
};

fn release_for(os: &str, arch: &str) -> Option<&'static Release> {
    Some(match (os, arch) {
        ("linux", "x86_64") => &LINUX_X64,
        ("linux", "aarch64") => &LINUX_ARM64,
        ("macos", "x86_64") => &MAC_X64,
        ("macos", "aarch64") => &MAC_ARM64,
        ("windows", "x86_64" | "aarch64") => &WINDOWS_X64,
        _ => return None,
    })
}

/// The pinned release for this computer, if there is one.
pub fn release() -> Option<&'static Release> {
    release_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn binary_name() -> String {
    format!("tectonic{}", std::env::consts::EXE_SUFFIX)
}

static INSTALL_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the downloaded binary lives (the app's data folder). Set once at startup,
/// before the first detection.
pub fn set_install_dir(dir: PathBuf) {
    let _ = INSTALL_DIR.set(dir);
}

/// The Tectonic Lemmata downloaded, if it is there.
pub fn installed_path() -> Option<PathBuf> {
    INSTALL_DIR
        .get()
        .map(|d| d.join(binary_name()))
        .filter(|p| p.is_file())
}

/// The downloaded Tectonic, else one on PATH.
pub fn find() -> Option<PathBuf> {
    installed_path().or_else(|| which::which("tectonic").ok())
}

/// "0.17.0" from `tectonic --version` ("Tectonic 0.17.0").
pub fn version(exe: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--version");
    no_window_std(&mut cmd);
    let out = cmd.output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .map(String::from)
}

// ---------------------------------------------------------------- building

/// The arguments for compiling a project, run from the project root.
pub fn tectonic_args(project: &Project) -> Vec<OsString> {
    let c = &project.config;
    let mut args: Vec<OsString> = ["-X", "compile", "--outdir"].map(Into::into).into();
    args.push(c.outdir.clone().into());
    args.extend(["--synctex", "--keep-logs", "--keep-intermediates"].map(OsString::from));
    if !c.stop_on_error {
        // Like latexmk -f: keep going past errors for the most complete PDF possible.
        args.extend(["-Z", "continue-on-errors"].map(OsString::from));
    }
    if c.shell_escape {
        args.extend(["-Z", "shell-escape"].map(OsString::from));
    }
    let mut search = OsString::from("search-path=");
    search.push(&project.root);
    args.extend(["-Z".into(), search]);
    args.push(project.main.as_os_str().to_owned());
    args
}

/// Tectonic's package cache. Empty on first use, when the first build downloads the format and packages.
fn cache_dir() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"TectonicProject\Tectonic"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches/Tectonic"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".cache")))
            .map(|d| d.join("tectonic"))
    }
}

/// How many files Tectonic has fetched so far, from its output ("note: downloading cmr10.tfm").
fn files_fetched(output: &str) -> usize {
    output
        .lines()
        .filter(|l| l.starts_with("note: downloading "))
        .count()
}

/// Builds the project with Tectonic at `exe`. Sending on `cancel` stops it.
/// `on_fetch` is told how many files Tectonic has downloaded so far while it works.
pub async fn build(
    project: &Project,
    exe: &Path,
    cancel: oneshot::Receiver<()>,
    // Build even when no project file changed (an explicit Recompile).
    force: bool,
    on_fetch: &(dyn Fn(usize) + Sync),
) -> Result<Outcome, Error> {
    if !exe.exists() {
        return Err(Error::NoTectonic);
    }
    let started = Instant::now();
    if !force && let Some(o) = cached_outcome(project, started) {
        return Ok(o);
    }
    let started_at = SystemTime::now();
    prepare_outdir(project);
    let outdir = project.outdir_abs();
    let _ = std::fs::remove_file(project.log_path());
    let _ = std::fs::remove_file(outdir.join(OK_STAMP));

    let output_path = outdir.join("tectonic-output.txt");
    // Its notes ("Running external tool biber") and errors share one file.
    let (stdout, stderr) =
        match std::fs::File::create(&output_path).and_then(|f| f.try_clone().map(|g| (f, g))) {
            Ok((f, g)) => (Stdio::from(f), Stdio::from(g)),
            Err(_) => (Stdio::null(), Stdio::null()),
        };
    let mut limit = match project.config.timeout_secs {
        0 => Duration::from_secs(60 * 60 * 24),
        n => Duration::from_secs(n),
    };
    // The first build downloads the format and packages, which a short limit would cut off.
    if cache_dir().is_none_or(|c| !c.join("formats").is_dir()) {
        limit = limit.max(Duration::from_secs(15 * 60));
    }
    let mut cmd = Command::new(exe);
    cmd.args(tectonic_args(project))
        .current_dir(&project.root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    let mut cancel = cancel;
    // Tectonic writes its notes straight into the output file, so reading it shows how far it is.
    let watch = async {
        let mut shown = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let text = std::fs::read(&output_path).unwrap_or_default();
            let n = files_fetched(&String::from_utf8_lossy(&text));
            if n != shown {
                shown = n;
                on_fetch(n);
            }
        }
    };
    let mut status = tokio::select! {
        r = crate::build::run_step(cmd, limit, &mut cancel) => r?,
        () = watch => unreachable!(),
    }
    .unwrap_or(Status::Failed);

    let duration_ms = started.elapsed().as_millis() as u64;
    let outcome = |status, pdf, diagnostics| Outcome {
        status,
        duration_ms,
        pdf,
        diagnostics,
        latexmk_failed: false,
    };
    match status {
        Status::Cancelled => return Ok(outcome(status, None, Vec::new())),
        Status::Timeout => {
            let mut d = read_log(project);
            d.insert(
                0,
                Diagnostic {
                    severity: Severity::Error,
                    kind: Kind::Other,
                    file: None,
                    line: None,
                    message: format!(
                        "The build took longer than {} seconds and was stopped.",
                        limit.as_secs()
                    ),
                    hint: Some("The first Tectonic build downloads packages and can be slow on a poor connection; try again. Otherwise look for an endless loop in a macro. You can change the limit with timeout_secs in lemmata.toml.".into()),
                    fix: None,
                },
            );
            return Ok(outcome(status, None, d));
        }
        _ => {}
    }

    let stderr_text = std::fs::read(&output_path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut diagnostics = read_log(project);
    merge_blg(project, &mut diagnostics, started_at);
    // Files in the build folder (.bbl, .aux) aren't something the user can edit.
    let rel_outdir = project.config.outdir.trim_start_matches("./");
    for d in &mut diagnostics {
        if d.file
            .as_deref()
            .is_some_and(|f| f.trim_start_matches("./").starts_with(rel_outdir))
        {
            d.file = None;
            d.line = None;
        }
    }
    let has_error = |d: &[Diagnostic]| d.iter().any(|d| d.severity == Severity::Error);
    // Without a log (it never got to TeX), stderr is all there is, TeX errors included.
    let log_had_errors = has_error(&diagnostics);
    diagnostics.extend(diagnostics_from_stderr(&stderr_text, !log_had_errors));
    // continue-on-errors exits 0 after reporting errors.
    if status == Status::Success && has_error(&diagnostics) {
        status = Status::Failed;
    }
    if status == Status::Failed && !has_error(&diagnostics) {
        diagnostics.insert(
            0,
            Diagnostic {
                severity: Severity::Error,
                kind: Kind::Other,
                file: None,
                line: None,
                message: "Tectonic stopped without saying why.".into(),
                hint: Some(tail_of(stderr_text.trim(), 600)),
                fix: None,
            },
        );
    }

    let pdf = project.pdf_path();
    let fresh = written_since(&pdf, started_at);
    if status == Status::Success && fresh {
        mark_ok(project, started_at);
    }
    Ok(outcome(status, fresh.then_some(pdf), diagnostics))
}

/// Tectonic prefixes TeX errors twice ("! ! LaTeX Error: …") and doubles the final period.
pub(crate) fn normalize_log(log: &str) -> String {
    let mut out = String::with_capacity(log.len());
    for line in log.lines() {
        let line = line
            .strip_prefix("! ! ")
            .map_or_else(|| line.to_string(), |rest| format!("! {rest}"));
        let line = if line.starts_with("! ") && line.ends_with("..") {
            &line[..line.len() - 1]
        } else {
            &line
        };
        out.push_str(line);
        out.push('\n');
    }
    out
}

const BIBER_HINT: &str = "Tectonic can run BibTeX by itself, but biblatex with biber needs the separate biber program, and Tectonic's old biblatex only works with biber 2.17. Easiest fix: choose BibTeX under Bibliography in the project settings (natbib, or biblatex with backend=bibtex). Or install TeX Live and build with that.";

/// Problems Tectonic reported on stderr that the TeX log does not show: a missing
/// or incompatible biber, no network, a crash. With `tex_errors` the TeX errors
/// it summarises there are included too (for when no log was written).
pub(crate) fn diagnostics_from_stderr(text: &str, tex_errors: bool) -> Vec<Diagnostic> {
    // Fontconfig prints its complaints about the system's font setup here too.
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with("Fontconfig"))
        .collect();
    let all = lines.join("\n");
    let err = |message: String, hint: Option<&str>| Diagnostic {
        severity: Severity::Error,
        kind: Kind::Other,
        file: None,
        line: None,
        message,
        hint: hint.map(String::from),
        fix: None,
    };
    let mut out = Vec::new();

    if all.contains("couldn't get it from the internet") || all.contains("error sending request") {
        out.push(err(
            "Tectonic could not download what it needs.".into(),
            Some("Tectonic fetches its format and packages from the internet the first time it needs them. Check your connection, then compile again. After the first successful build most documents work offline."),
        ));
        return out;
    }
    if all.contains("Running external tool biber") {
        // Not found: "error: No such file or directory"; failed: biber's own output follows.
        let failed = lines.iter().any(|l| {
            l.starts_with("error: No such file or directory")
                || l.starts_with("error: the external tool exited")
        });
        if failed {
            let message = if all.contains("incompatible") || all.contains("control file version") {
                "biber is installed, but its version does not match Tectonic's biblatex."
                    .to_string()
            } else if all.contains("INFO - This is Biber") {
                let detail = lines
                    .iter()
                    .find_map(|l| l.strip_prefix("ERROR - "))
                    .unwrap_or("it reported errors");
                format!("biber failed: {}", detail.trim())
            } else {
                "This document needs biber, and biber is not installed.".to_string()
            };
            out.push(err(message, Some(BIBER_HINT)));
            return out;
        }
    }
    if let Some(i) = lines.iter().position(|l| l.contains("panicked at")) {
        let detail = lines.get(i + 1).copied().unwrap_or("");
        out.push(err(format!("Tectonic crashed: {}", detail.trim()), None));
        return out;
    }

    let located = Regex::new(r"^error: (.+?):(\d+): (.*)$").unwrap();
    for l in &lines {
        let Some(msg) = l.strip_prefix("error: ") else {
            continue;
        };
        if let Some(c) = located.captures(l) {
            if tex_errors {
                let mut d = err(c[3].trim().trim_start_matches("! ").to_string(), None);
                d.file = Some(c[1].to_string());
                d.line = c[2].parse().ok();
                out.push(d);
            }
            continue;
        }
        // Wrapper lines around the engine output, whose substance is in the log.
        let wrapper = [
            "something bad happened inside",
            "the XeTeX engine had an unrecoverable error",
            "its stderr was",
            "the external tool exited",
            "caused by",
        ];
        if wrapper.iter().any(|w| msg.starts_with(w)) {
            continue;
        }
        out.push(err(msg.trim().to_string(), None));
    }
    out
}

// ---------------------------------------------------------------- installing

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("There is no Tectonic download for this computer ({0}).")]
    Unsupported(String),
    #[error("Downloading Tectonic failed: {0}")]
    Download(String),
    #[error("The downloaded file does not match its checksum, so it was discarded.")]
    Checksum,
    #[error("Unpacking Tectonic failed: {0}")]
    Extract(String),
    #[error("The downloaded Tectonic does not run: {0}")]
    NotWorking(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn sha256_hex(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn verify(path: &Path, expected: &str) -> Result<(), InstallError> {
    if sha256_hex(path)?.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(InstallError::Checksum)
    }
}

/// curl and tar ship with Windows 10+, macOS and Linux. A GitHub download is
/// checked against a pinned checksum, so how it was fetched does not matter, and
/// curl needs no TLS stack of our own (and follows the system's proxy settings).
/// On Windows the System32 copies are used: a `tar` earlier on PATH may not read zip.
fn system_tool(name: &str) -> PathBuf {
    if cfg!(windows)
        && let Some(root) = std::env::var_os("SystemRoot")
    {
        let p = PathBuf::from(root)
            .join("System32")
            .join(format!("{name}.exe"));
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(name)
}

fn no_window_std(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = cmd;
}

fn run_tool(name: &str, args: &[&OsString]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(system_tool(name));
    cmd.args(args).stdin(Stdio::null());
    no_window_std(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("could not run {name}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Downloads the pinned release for this computer into `dir` and returns the binary's path.
/// Blocking, and big: call it off the main thread.
pub fn install(dir: &Path) -> Result<PathBuf, InstallError> {
    let rel = release().ok_or_else(|| {
        InstallError::Unsupported(format!(
            "{}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    install_release(rel, &rel.url(), dir)
}

fn install_release(rel: &Release, url: &str, dir: &Path) -> Result<PathBuf, InstallError> {
    std::fs::create_dir_all(dir)?;
    let work = dir.join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let result = (|| {
        let archive = work.join("archive");
        let args: Vec<OsString> = [
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--retry",
            "2",
            "--connect-timeout",
            "20",
            "--max-time",
            "900",
            "--output",
        ]
        .map(OsString::from)
        .into_iter()
        .chain([archive.clone().into(), url.into()])
        .collect();
        run_tool("curl", &args.iter().collect::<Vec<_>>()).map_err(InstallError::Download)?;
        verify(&archive, rel.sha256)?;

        let unpacked = work.join("unpacked");
        std::fs::create_dir_all(&unpacked)?;
        // GNU tar needs -z for gzip; bsdtar (macOS, Windows) detects it and ignores the flag.
        let flag = match rel.archive {
            Archive::TarGz => "-xzf",
            Archive::Zip => "-xf",
        };
        let args: Vec<OsString> = vec![
            flag.into(),
            archive.into(),
            "-C".into(),
            unpacked.clone().into(),
        ];
        run_tool("tar", &args.iter().collect::<Vec<_>>()).map_err(InstallError::Extract)?;

        let bin = unpacked.join(binary_name());
        if !bin.is_file() {
            return Err(InstallError::Extract(
                "the archive holds no tectonic program".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
        }
        let ok = {
            let mut cmd = std::process::Command::new(&bin);
            cmd.arg("--version");
            no_window_std(&mut cmd);
            cmd.output().is_ok_and(|o| o.status.success())
        };
        if !ok {
            return Err(InstallError::NotWorking(
                "it did not start. This computer may block downloaded programs.".into(),
            ));
        }
        let dest = dir.join(binary_name());
        // Windows can't replace a running program; the old file is simply removed first.
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&bin, &dest)?;
        Ok(dest)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let mut p = Project::open(d.path()).unwrap();
        // The tempdir must outlive the test only for these argument checks.
        std::mem::forget(d);
        p.config.engine = lemmata_project::Engine::Tectonic;
        p
    }

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.to_string_lossy().into())
            .collect()
    }

    #[test]
    fn compile_arguments() {
        let p = project();
        let args = strings(tectonic_args(&p));
        let root = p.root.to_string_lossy().to_string();
        assert_eq!(
            args,
            [
                "-X",
                "compile",
                "--outdir",
                &p.config.outdir,
                "--synctex",
                "--keep-logs",
                "--keep-intermediates",
                "-Z",
                "continue-on-errors",
                "-Z",
                &format!("search-path={root}"),
                "main.tex",
            ]
        );
    }

    #[test]
    fn halting_and_shell_escape_follow_the_settings() {
        let mut p = project();
        p.config.stop_on_error = true;
        p.config.shell_escape = true;
        let args = strings(tectonic_args(&p));
        assert!(!args.contains(&"continue-on-errors".to_string()));
        assert!(args.windows(2).any(|w| w == ["-Z", "shell-escape"]));
    }

    #[test]
    fn the_pdf_and_synctex_land_where_the_app_looks() {
        // --outdir is the project's build folder and Tectonic names outputs after the main file.
        let p = project();
        assert_eq!(p.pdf_path(), p.outdir_abs().join("main.pdf"));
    }

    #[test]
    fn tex_error_in_the_log_is_normalised() {
        let log = include_str!("../tests/tectonic/missing-package.log");
        let d = lemmata_log::parse(&normalize_log(log));
        let e = d.iter().find(|d| d.severity == Severity::Error).unwrap();
        assert_eq!(
            e.kind,
            Kind::MissingFile {
                name: "nonexistentpkg.sty".into()
            }
        );
        assert_eq!(e.line, Some(3));
    }

    #[test]
    fn undefined_macro_is_located() {
        let log = include_str!("../tests/tectonic/undefined-macro.log");
        let d = lemmata_log::parse(&normalize_log(log));
        let e = d
            .iter()
            .find(|d| d.kind == Kind::UndefinedControlSequence)
            .unwrap();
        assert_eq!(e.line, Some(11));
    }

    #[test]
    fn stderr_with_only_tex_errors_adds_nothing() {
        let text = include_str!("../tests/tectonic/undefined-macro.stderr");
        assert!(diagnostics_from_stderr(text, false).is_empty());
        // Without a log those same errors are all there is.
        let d = diagnostics_from_stderr(text, true);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, Some(11));
        assert_eq!(d[0].file.as_deref(), Some("main.tex"));
        assert_eq!(d[0].message, "Undefined control sequence");
    }

    #[test]
    fn incompatible_biber_is_explained() {
        let text = include_str!("../tests/tectonic/biber-mismatch.stderr");
        let d = diagnostics_from_stderr(text, false);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("does not match"));
        assert!(d[0].hint.as_deref().unwrap().contains("BibTeX"));
    }

    #[test]
    fn missing_biber_is_explained() {
        let text = include_str!("../tests/tectonic/biber-missing.stderr");
        let d = diagnostics_from_stderr(text, false);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("biber is not installed"));
        assert!(d[0].hint.as_deref().unwrap().contains("BibTeX"));
    }

    #[test]
    fn offline_first_run_is_explained() {
        let text = include_str!("../tests/tectonic/offline.stderr");
        let d = diagnostics_from_stderr(text, false);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("download"));
    }

    #[test]
    fn fetched_files_are_counted() {
        let text = "note: Running TeX ...\nnote: generating format \"latex\"\nnote: downloading latex.ltx\nnote: downloading cmr10.tfm\nwarning: downloading is slow\n";
        assert_eq!(files_fetched(text), 2);
        assert_eq!(files_fetched(""), 0);
    }

    #[test]
    fn warnings_and_notes_are_ignored() {
        let text = "note: Running TeX ...\nwarning: main.tex:12: Underfull \\hbox (badness 10000)\nFontconfig warning: \"x\", line 1: invalid\n";
        assert!(diagnostics_from_stderr(text, true).is_empty());
    }

    #[test]
    fn pinned_releases_cover_the_supported_platforms() {
        for (os, arch) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
            ("windows", "aarch64"),
        ] {
            let r = release_for(os, arch).unwrap();
            assert_eq!(r.sha256.len(), 64, "{os}/{arch}");
            assert!(r.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(r.url().starts_with(
                "https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40"
            ));
            assert!(r.url().contains(r.target));
        }
        assert!(release_for("freebsd", "x86_64").is_none());
    }

    #[test]
    fn checksum_is_checked() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        std::fs::write(&f, "abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(&f).unwrap(), abc);
        assert!(verify(&f, abc).is_ok());
        assert!(verify(&f, &abc.to_uppercase()).is_ok());
        assert!(matches!(
            verify(&f, &"0".repeat(64)),
            Err(InstallError::Checksum)
        ));
    }

    /// Network: downloads the real release (~10-20 MB) and runs it.
    #[test]
    #[ignore]
    fn installs_the_pinned_release() {
        let d = tempfile::tempdir().unwrap();
        let exe = install(d.path()).unwrap();
        eprintln!("installed {}", exe.display());
        assert_eq!(version(&exe).as_deref(), Some(VERSION));
        // No leftovers besides the binary.
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    /// The release downloaded into a temp folder, once per test run (never the one on PATH, so CI
    /// tests what users get). Run these tests with --test-threads=1: they share Tectonic's package cache.
    fn downloaded() -> &'static Path {
        static EXE: OnceLock<PathBuf> = OnceLock::new();
        EXE.get_or_init(|| {
            let d = tempfile::tempdir().unwrap();
            let exe = install(d.path()).unwrap();
            std::mem::forget(d);
            eprintln!("tectonic: {} ({:?})", exe.display(), version(&exe));
            exe
        })
    }

    /// What Tectonic printed, for failure messages: a CI log is all there is to diagnose from.
    fn output_of(project: &Project) -> String {
        std::fs::read_to_string(project.outdir_abs().join("tectonic-output.txt"))
            .unwrap_or_else(|e| format!("(no output: {e})"))
    }

    /// Network on first run: a real build with the downloaded Tectonic.
    #[tokio::test]
    #[ignore]
    async fn builds_a_project_with_bibtex_and_reports_errors() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let exe = downloaded();
        let d = tempfile::tempdir().unwrap();
        let refs = "@article{k, author={A, B}, title={T}, journal={J}, year={2020}}\n";
        std::fs::write(d.path().join("refs.bib"), refs).unwrap();
        let main = "\\documentclass{article}\n\\begin{document}\nSee \\cite{k}.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
        std::fs::write(d.path().join("main.tex"), main).unwrap();
        let project = Project::open(d.path()).unwrap();
        let fetched = AtomicUsize::new(0);
        let (_tx, rx) = oneshot::channel();
        let o = build(&project, exe, rx, true, &|n| {
            fetched.store(n, Ordering::Relaxed)
        })
        .await
        .unwrap();
        let output = output_of(&project);
        eprintln!("{output}\ncache: {:?}", cache_dir());
        assert_eq!(o.status, Status::Success, "{:?}\n{output}", o.diagnostics);
        assert!(project.pdf_path().is_file());
        assert!(project.pdf_path().with_extension("synctex.gz").is_file());
        // The package cache was empty, so the first build fetched files and said so.
        assert!(fetched.load(Ordering::Relaxed) > 0, "{output}");

        // An error still gives a PDF (continue-on-errors) but the build is Failed and located.
        std::fs::write(d.path().join("main.tex"), main.replace("See", "\\oops See")).unwrap();
        let (_tx, rx) = oneshot::channel();
        let o = build(&project, exe, rx, true, &|_| {}).await.unwrap();
        assert_eq!(o.status, Status::Failed, "{}", output_of(&project));
        let e = o
            .diagnostics
            .iter()
            .find(|d| d.kind == Kind::UndefinedControlSequence)
            .unwrap();
        assert_eq!(e.line, Some(3));
    }

    /// The article fixture uses biblatex with biber, which Tectonic can't drive without a matching biber.
    #[tokio::test]
    #[ignore]
    async fn fixture_with_biber_reports_the_limitation() {
        let exe = downloaded();
        let d = tempfile::tempdir().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article");
        for f in ["main.tex", "refs.bib", "chapters/method.tex"] {
            let to = d.path().join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(src.join(f), to).unwrap();
        }
        let project = Project::open(d.path()).unwrap();
        let (_tx, rx) = oneshot::channel();
        let o = build(&project, exe, rx, true, &|_| {}).await.unwrap();
        let output = output_of(&project);
        assert_eq!(o.status, Status::Failed, "{output}");
        let b = o
            .diagnostics
            .iter()
            .find(|d| d.message.contains("biber"))
            .unwrap_or_else(|| panic!("{:?}\n{output}", o.diagnostics));
        assert!(b.hint.as_deref().unwrap().contains("BibTeX"));
    }

    #[test]
    fn a_bad_download_is_rejected_and_cleaned_up() {
        // Serving a local file stands in for the network; it must fail the checksum.
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src.tar.gz");
        std::fs::write(&src, "not the release").unwrap();
        let dest = d.path().join("bin");
        let url = format!("file://{}", src.display());
        let r = install_release(&LINUX_X64, &url, &dest);
        if cfg!(unix) {
            assert!(matches!(r, Err(InstallError::Checksum)), "{r:?}");
            assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
        }
    }
}
