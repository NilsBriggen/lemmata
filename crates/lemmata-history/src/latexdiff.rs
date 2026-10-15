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
    /// How packages are installed here, for the hint when one is missing.
    pub installer: Option<lemmata_pkgmap::Installer<'a>>,
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
    let mut child = cmd.spawn().map_err(|e| {
        Error::Tool(format!(
            "Couldn't start {}: {e}",
            Path::new(cmd.get_program()).display()
        ))
    })?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if started.elapsed() > timeout {
            #[cfg(unix)]
            let _ = lemmata_env::host(&mut Command::new("kill"))
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

fn last_lines(text: &str) -> String {
    let lines: Vec<_> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(8)..].join("\n")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Why the diff document didn't compile, in plain words. A missing package says how to install it.
fn explain_tex_failure(
    log: &str,
    latexmk_out: &str,
    installer: Option<lemmata_pkgmap::Installer>,
) -> String {
    let first = lemmata_log::parse(log)
        .into_iter()
        .find(|d| d.severity == lemmata_log::Severity::Error);
    let Some(d) = first else {
        return format!(
            "The PDF of the changes couldn't be made. {}",
            last_lines(latexmk_out)
        );
    };
    if let lemmata_log::Kind::MissingFile { name } = &d.kind {
        let fix = installer.and_then(|i| lemmata_pkgmap::fix_for(name, i));
        let how = match fix.as_ref().map(|f| (&f.explanation, &f.command)) {
            Some((why, Some(cmd))) => format!("{why} Install it with `{cmd}` and try again."),
            Some((why, None)) => format!("{why} Install it and try again."),
            None => match lemmata_pkgmap::lookup(name) {
                Some(p) => format!(
                    "It is part of the TeX Live package \"{}\". Install that with your TeX distribution and try again.",
                    p.package
                ),
                None => "Install it with your TeX distribution and try again.".into(),
            },
        };
        return format!(
            "The PDF of the changes needs {name}, which isn't installed. latexdiff's markup uses it. {how}"
        );
    }
    let hint = d.hint.map(|h| format!(" {h}")).unwrap_or_default();
    format!(
        "The PDF of the changes couldn't be made. TeX says: {}{hint}",
        d.message
    )
}

/// Why latexdiff itself failed, in plain words.
fn explain_latexdiff_failure(stderr: &str) -> String {
    if let Some(module) = stderr
        .split("Can't locate ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
    {
        return format!(
            "latexdiff couldn't run because its Perl module {module} is missing. Reinstall latexdiff or Perl with your package manager so that the module is available."
        );
    }
    format!("latexdiff failed. {}", last_lines(stderr))
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
    if !root.join(main).is_file() {
        return Err(Error::Tool(format!(
            "{} doesn't exist any more, so there is nothing to compare the snapshot with.",
            main.display()
        )));
    }
    let result = diff_and_compile(root, main, &work, &old_main, tools);
    for side in ["old", "new"] {
        let _ = std::fs::remove_dir_all(root.join(&work).join(side));
    }
    result
}

/// Name of the main file's copy at the top of a staged folder.
const STAGED_MAIN: &str = "lemmata-main.tex";

/// latexdiff looks for `\input` files next to the main file, TeX next to the project folder (where
/// latexmk runs). With the main file in a subfolder the two differ, so each side's .tex files get a
/// folder of their own with the main file at its top. Returns the staged old and new main files.
fn stage(root: &Path, main: &Path, work: &Path) -> Result<(PathBuf, PathBuf)> {
    let (old, new) = (work.join("old"), work.join("new"));
    copy_tex_files(root, &root.join(&new))?;
    for side in [&old, &new] {
        std::fs::copy(
            root.join(side).join(main),
            root.join(side).join(STAGED_MAIN),
        )?;
    }
    Ok((old.join(STAGED_MAIN), new.join(STAGED_MAIN)))
}

/// Copies the .tex files below `from` to `to`, keeping the folder structure. Folders starting
/// with a dot (.git, Lemmata's own) are skipped.
fn copy_tex_files(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        let file = path.file_name().unwrap_or_default().to_owned();
        if file.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            copy_tex_files(&path, &to.join(&file))?;
        } else if path.extension().is_some_and(|e| e == "tex") {
            std::fs::copy(&path, to.join(&file))?;
        }
    }
    Ok(())
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
    let in_subfolder = main.parent().is_some_and(|p| !p.as_os_str().is_empty());
    let (old_main, new_main) = if in_subfolder {
        stage(root, main, work)?
    } else {
        (old_main.to_path_buf(), main.to_path_buf())
    };
    let mut diff = Command::new(tools.latexdiff);
    diff.current_dir(root)
        .args(["--flatten", "--encoding=utf8"])
        .arg(old_main)
        .arg(new_main);
    path_env(&mut diff);
    let log = abs.join("latexdiff.log");
    if !run(diff, &tex, &log, tools.timeout)? {
        return Err(Error::Tool(explain_latexdiff_failure(&read(&log))));
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
        return Err(Error::Tool(explain_tex_failure(
            &read(&abs.join("diff.log")),
            &read(&out),
            tools.installer,
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

    const MISSING_ULEM_LOG: &str = "(/usr/share/texmf-dist/tex/latex/base/article.cls)\n! LaTeX Error: File `ulem.sty' not found.\n\nType X to quit or <RETURN> to proceed,\nor enter new name. (Default extension: sty)\n\nEnter file name: \n./.lemmata/build/latexdiff/diff.tex:8: Emergency stop.\n<read *> \n";

    #[test]
    fn a_missing_ulem_says_how_to_install_it() {
        use lemmata_pkgmap::Installer;
        let m = explain_tex_failure(MISSING_ULEM_LOG, "", Some(Installer::Tlmgr { sudo: true }));
        assert!(
            m.contains("needs ulem.sty") && m.contains("`sudo tlmgr install ulem`"),
            "{m}"
        );
        let m = explain_tex_failure(MISSING_ULEM_LOG, "", Some(Installer::Distro("arch")));
        assert!(m.contains("pacman"), "{m}");
        let m = explain_tex_failure(MISSING_ULEM_LOG, "", None);
        assert!(m.contains("TeX Live package \"ulem\""), "{m}");
    }

    #[test]
    fn other_tex_failures_are_in_plain_words() {
        let log =
            "./.lemmata/build/latexdiff/diff.tex:30: Undefined control sequence.\nl.30 \\foo\n";
        let m = explain_tex_failure(log, "", None);
        assert!(
            m.contains("TeX says: LaTeX doesn't know the command \\foo"),
            "{m}"
        );
        // No error in the log: what latexmk said is all there is.
        let m = explain_tex_failure("", "one\n\ntwo\n", None);
        assert!(m.ends_with("one\ntwo"), "{m}");
    }

    #[test]
    fn latexdiff_failures_are_explained() {
        let m =
            explain_latexdiff_failure("Can't locate Text/Foo.pm in @INC (you may need to install)");
        assert!(m.contains("Perl module Text/Foo.pm is missing"), "{m}");
        assert_eq!(
            explain_latexdiff_failure("boom\n"),
            "latexdiff failed. boom"
        );
    }

    #[test]
    fn staging_puts_each_sides_tex_files_under_the_main_file() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        for (path, text) in [
            ("paper/main.tex", "new main"),
            ("paper/sec.tex", "new sec"),
            ("paper/fig.png", "x"),
            (".git/hidden.tex", "x"),
            (".lemmata/build/latexdiff/old/paper/main.tex", "old main"),
        ] {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), text).unwrap();
        }
        let work = Path::new(LATEXDIFF_DIR);
        let (old, new) = stage(root, Path::new("paper/main.tex"), work).unwrap();
        assert_eq!(fs::read_to_string(root.join(old)).unwrap(), "old main");
        assert_eq!(fs::read_to_string(root.join(&new)).unwrap(), "new main");
        let tree = root.join(LATEXDIFF_DIR).join("new");
        assert_eq!(
            fs::read_to_string(tree.join("paper/sec.tex")).unwrap(),
            "new sec"
        );
        assert!(!tree.join("paper/fig.png").exists() && !tree.join(".git").exists());
    }

    #[test]
    fn a_main_file_deleted_since_is_explained() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join(LATEXDIFF_DIR).join("old");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("main.tex"), "x").unwrap();
        let tools = Tools {
            latexdiff: Path::new("latexdiff"),
            latexmk: Path::new("latexmk"),
            engine_flag: "-pdf",
            installer: None,
            extra_args: Vec::new(),
            path_dirs: Vec::new(),
            timeout: Duration::from_secs(1),
        };
        let e = latexdiff_pdf(d.path(), Path::new("main.tex"), &tools).unwrap_err();
        assert!(e.to_string().contains("doesn't exist any more"));
    }

    #[test]
    fn a_main_file_that_did_not_exist_is_explained() {
        let d = tempfile::tempdir().unwrap();
        let tools = Tools {
            latexdiff: Path::new("latexdiff"),
            latexmk: Path::new("latexmk"),
            engine_flag: "-pdf",
            installer: None,
            extra_args: Vec::new(),
            path_dirs: Vec::new(),
            timeout: Duration::from_secs(1),
        };
        let e = latexdiff_pdf(d.path(), Path::new("main.tex"), &tools).unwrap_err();
        assert!(e.to_string().contains("didn't exist yet"));
    }
}
