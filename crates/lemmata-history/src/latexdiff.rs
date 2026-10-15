//! A PDF of the changes since a snapshot: the snapshot's whole tree is
//! unpacked into `.lemmata/build/latexdiff/old`, `latexdiff --flatten` marks
//! the differences from there to the current files, and latexmk compiles that.

use crate::{Error, History, Result, check, no_window};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Below the project folder; the old tree, `diff.tex`, `diff.pdf` and logs go here.
pub const LATEXDIFF_DIR: &str = ".lemmata/build/latexdiff";

pub struct Tools<'a> {
    pub latexdiff: &'a Path,
    pub latexmk: &'a Path,
    /// latexmk's engine flag: `-pdf`, `-xelatex` or `-lualatex`.
    pub engine_flag: &'a str,
    /// Extra latexmk arguments (MiKTeX's unattended package installs).
    pub extra_args: Vec<OsString>,
    /// Folders put first on PATH: the TeX bin folder, Perl's.
    pub path_dirs: Vec<PathBuf>,
    pub timeout: Duration,
}

/// `latexdiff` in the TeX folder, else on PATH.
pub fn find_latexdiff(bin_dir: Option<&Path>) -> Option<PathBuf> {
    let names = ["latexdiff", "latexdiff.exe", "latexdiff.bat"];
    bin_dir
        .and_then(|d| names.iter().map(|n| d.join(n)).find(|p| p.is_file()))
        .or_else(|| which::which("latexdiff").ok())
}

pub const MISSING_LATEXDIFF: &str = "latexdiff isn't installed. It ships with TeX Live: install it with `tlmgr install latexdiff`, or the distribution's package (Debian/Ubuntu: texlive-extra-utils, Fedora: texlive-latexdiff, Arch: texlive-binextra). With MiKTeX, install the latexdiff package; it also needs Perl.";

impl History {
    /// Writes every file the project had in snapshot `id` below `dest`
    /// (emptied first). Returns how many files.
    pub fn extract_tree(&self, id: &str, dest: &Path) -> Result<usize> {
        let _ = std::fs::remove_dir_all(dest);
        std::fs::create_dir_all(dest)?;
        let prefix = self.run(&["rev-parse", "--show-prefix"])?;
        let prefix = prefix.trim();
        let paths = self.paths(&["ls-tree", "-r", "-z", "--name-only", id, "--", "."])?;
        for path in &paths {
            let out = check(
                self.cmd()
                    .args(["show", &format!("{id}:{prefix}{path}")])
                    .output()?,
            )?;
            let to = dest.join(path);
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(to, out.stdout)?;
        }
        Ok(paths.len())
    }
}

/// Runs `cmd` with its output going to files, killing it after `timeout`.
fn run(mut cmd: Command, out: &Path, err: &Path, timeout: Duration) -> Result<bool> {
    cmd.stdin(Stdio::null())
        .stdout(std::fs::File::create(out)?)
        .stderr(std::fs::File::create(err)?);
    no_window(&mut cmd);
    // A group of its own, so a timeout can stop latexmk and the engine it started.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Tool(format!("could not start {:?}: {e}", cmd.get_program())))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if started.elapsed() > timeout {
            #[cfg(unix)]
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Tool(format!(
                "Making the PDF of the changes took longer than {} seconds and was stopped.",
                timeout.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<_> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(8)..].join("\n")
}

/// The first TeX errors in a log, which say more than its end.
fn tex_errors(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let errors: Vec<_> = text
        .lines()
        .filter(|l| l.starts_with('!') || l.contains(":error:") || l.contains(": Emergency stop"))
        .take(3)
        .collect();
    errors.join("\n")
}

/// Marks the changes between the snapshot already unpacked at
/// `root/LATEXDIFF_DIR/old` (see `History::extract_tree`) and the current
/// files, starting from the main file `main` (relative to `root`), and
/// compiles the result. Returns the PDF's path. Unpacked files are removed afterwards.
pub fn latexdiff_pdf(root: &Path, main: &Path, tools: &Tools) -> Result<PathBuf> {
    let work = PathBuf::from(LATEXDIFF_DIR);
    let old_main = work.join("old").join(main);
    if !root.join(&old_main).is_file() {
        return Err(Error::Tool(format!(
            "{} didn't exist yet in that snapshot, so there is nothing to compare it with.",
            main.display()
        )));
    }
    let result = diff_and_compile(root, main, &work, &old_main, tools);
    let _ = std::fs::remove_dir_all(root.join(&work).join("old"));
    result
}

fn diff_and_compile(
    root: &Path,
    main: &Path,
    work: &Path,
    old_main: &Path,
    tools: &Tools,
) -> Result<PathBuf> {
    let abs = root.join(work);
    let path_env = |c: &mut Command| {
        let mut dirs = tools.path_dirs.clone();
        dirs.extend(
            std::env::var_os("PATH")
                .iter()
                .flat_map(std::env::split_paths),
        );
        if let Ok(p) = std::env::join_paths(dirs) {
            c.env("PATH", p);
        }
    };
    let (tex, pdf) = (abs.join("diff.tex"), abs.join("diff.pdf"));
    let _ = std::fs::remove_file(&pdf);
    let started = Instant::now();

    // Relative to the project folder: it is also where latexmk runs, so the
    // figures and bibliographies the document names are found as in a normal build.
    let mut diff = Command::new(tools.latexdiff);
    diff.current_dir(root)
        .args(["--flatten", "--encoding=utf8"])
        .arg(old_main)
        .arg(main);
    path_env(&mut diff);
    let log = abs.join("latexdiff.log");
    if !run(diff, &tex, &log, tools.timeout)? {
        return Err(Error::Tool(format!("latexdiff failed: {}", tail(&log))));
    }

    let mut mk = Command::new(tools.latexmk);
    mk.current_dir(root)
        .arg(tools.engine_flag)
        .args([
            "-interaction=nonstopmode",
            "-file-line-error",
            "-f",
            "-no-shell-escape",
        ])
        .arg(format!("-outdir={}", work.display()))
        .args(&tools.extra_args)
        .arg(work.join("diff.tex"));
    path_env(&mut mk);
    let left = tools.timeout.saturating_sub(started.elapsed());
    let out = abs.join("latexmk.out");
    // -f keeps going past errors, so a failing run can still leave a PDF.
    run(mk, &out, &abs.join("latexmk.err"), left)?;
    if !pdf.is_file() {
        let errors = tex_errors(&abs.join("diff.log"));
        let mut why = if errors.is_empty() {
            tail(&out)
        } else {
            errors
        };
        if why.contains("ulem.sty") {
            why.push_str("\nlatexdiff's markup needs the LaTeX package ulem (TeX Live: texlive-latexextra, or `tlmgr install ulem`).");
        }
        return Err(Error::Tool(format!(
            "The PDF of the changes couldn't be made. {why}"
        )));
    }
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> (tempfile::TempDir, History) {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("chapters")).unwrap();
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nIntro text.\n\\input{chapters/one}\n\\end{document}\n",
        )
        .unwrap();
        fs::write(
            root.join("chapters/one.tex"),
            "The first chapter says hello.\n",
        )
        .unwrap();
        let h = History::init(root).unwrap();
        (d, h)
    }

    #[test]
    fn extracts_a_snapshots_whole_tree() {
        let (d, h) = project();
        let root = d.path();
        let first = h.snapshot("v1").unwrap().unwrap();
        fs::write(root.join("chapters/one.tex"), "changed\n").unwrap();
        fs::write(root.join("extra.tex"), "new\n").unwrap();
        h.snapshot("v2").unwrap().unwrap();

        let dest = root.join(LATEXDIFF_DIR).join("old");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("stale.tex"), "x").unwrap();
        assert_eq!(h.extract_tree(&first, &dest).unwrap(), 2);
        assert_eq!(
            fs::read_to_string(dest.join("chapters/one.tex")).unwrap(),
            "The first chapter says hello.\n"
        );
        assert!(!dest.join("extra.tex").exists() && !dest.join("stale.tex").exists());
        // Lemmata's own folder never gets into a snapshot, so not into the unpacked tree either.
        assert!(!dest.join(".lemmata").exists());
    }

    #[test]
    fn makes_a_pdf_of_the_changes() {
        // The markup latexdiff adds needs ulem, which is not part of every TeX install.
        let has_ulem = Command::new("kpsewhich")
            .arg("ulem.sty")
            .output()
            .is_ok_and(|o| !o.stdout.is_empty());
        let (Some(latexdiff), Ok(latexmk), Ok(_), true) = (
            find_latexdiff(None),
            which::which("latexmk"),
            which::which("pdflatex"),
            has_ulem,
        ) else {
            return;
        };
        let (d, h) = project();
        let root = d.path();
        let first = h.snapshot("v1").unwrap().unwrap();
        fs::write(
            root.join("chapters/one.tex"),
            "The first chapter says goodbye.\n",
        )
        .unwrap();
        h.extract_tree(&first, &root.join(LATEXDIFF_DIR).join("old"))
            .unwrap();
        let tools = Tools {
            latexdiff: &latexdiff,
            latexmk: &latexmk,
            engine_flag: "-pdf",
            extra_args: Vec::new(),
            path_dirs: Vec::new(),
            timeout: Duration::from_secs(120),
        };
        let pdf = latexdiff_pdf(root, Path::new("main.tex"), &tools).unwrap();
        assert!(fs::metadata(&pdf).unwrap().len() > 1000);
        let tex = fs::read_to_string(root.join(LATEXDIFF_DIR).join("diff.tex")).unwrap();
        assert!(
            tex.contains("\\DIFdel{hello}") && tex.contains("\\DIFadd{goodbye}"),
            "{tex}"
        );
        assert!(!root.join(LATEXDIFF_DIR).join("old").exists());
    }

    #[test]
    fn a_main_file_that_did_not_exist_is_explained() {
        let d = tempfile::tempdir().unwrap();
        let tools = Tools {
            latexdiff: Path::new("latexdiff"),
            latexmk: Path::new("latexmk"),
            engine_flag: "-pdf",
            extra_args: Vec::new(),
            path_dirs: Vec::new(),
            timeout: Duration::from_secs(1),
        };
        let e = latexdiff_pdf(d.path(), Path::new("main.tex"), &tools).unwrap_err();
        assert!(e.to_string().contains("didn't exist yet"));
    }
}
