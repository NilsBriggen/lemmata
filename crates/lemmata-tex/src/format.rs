//! Precompiled preambles: a TeX format holding everything up to `\begin{document}`, so a warm
//! pdflatex rebuild doesn't load every package again. A format is made in the background after a
//! good build, and used only once one run with it gave byte for byte the output of one without it.

use crate::build::{Status, pass_args, run_step};
use lemmata_project::{Engine, Project};
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::sync::oneshot;

/// Folder inside the build folder.
pub(crate) const DIR: &str = "fmt";
const SHIM_FILE: &str = "lemmata-preamble.tex";

/// Read before the main file when the format is made. mylatexformat dumps at `\begin{document}`;
/// a dump keeps macros and registers but not what is on the page, so this keeps that too.
const SHIM: &str = r"\catcode64=11 \catcode95=11 \catcode58=11
% Map files are not part of a format and would silently go missing: refuse them.
\def\LEMMATA@refuse{\gdef\LEMMATA@refused{}}
\let\LEMMATA@mapfile\pdfmapfile \let\LEMMATA@mapline\pdfmapline
\protected\def\pdfmapfile{\LEMMATA@refuse\LEMMATA@mapfile}
\protected\def\pdfmapline{\LEMMATA@refuse\LEMMATA@mapline}
\let\tex_pdfmapfile:D\pdfmapfile \let\tex_pdfmapline:D\pdfmapline
\let\LEMMATA@endofdump\endofdump
\def\endofdump{%
  \ifdefined\LEMMATA@refused
    \errmessage{Lemmata: this preamble loads font map files, so it is not precompiled}\fi
  \let\pdfmapfile\LEMMATA@mapfile \let\tex_pdfmapfile:D\LEMMATA@mapfile
  \let\pdfmapline\LEMMATA@mapline \let\tex_pdfmapline:D\LEMMATA@mapline
  % The class's \clearpage leaves \prevdepth at 0pt (a \vspace* at the top of the first page
  % depends on it), and \write or \color in the preamble leave whatsits on the page.
  \xdef\LEMMATA@restore{%
    \prevdepth=\the\prevdepth\relax
    \deadcycles=\the\deadcycles\relax
    \catcode64=\the\catcode64\relax}%
  \begingroup
    \output{\global\setbox32767\box255 }%
    \hbox{}\penalty-10000
  \endgroup
  \global\setbox32767\vbox{\unvbox32767 \setbox0\lastbox\unskip}%
  \everyjob\expandafter{\the\everyjob\unvbox32767 \LEMMATA@restore}%
  % Fonts loaded ahead of time get other numbers, and so other names in the PDF.
  \let\MYLATEX@fontpreloading\relax
  \LEMMATA@endofdump}
\catcode64=12 \catcode95=8 \catcode58=12
\input{@MAIN@}
";

/// Whether this project's builds may use a precompiled preamble at all.
pub(crate) fn eligible(project: &Project, miktex: bool) -> bool {
    let c = &project.config;
    // Paths go into TeX code and onto the command line unquoted.
    let plain = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/\\".contains(c))
    };
    c.fast_preamble
        && c.engine == Engine::Pdflatex
        && !c.shell_escape
        && !c.draft
        && !miktex
        && plain(&c.outdir)
        && plain(&project.main.to_string_lossy())
}

/// The `-fmt` argument for a usable format of the current preamble.
pub(crate) fn usable(project: &Project, engine: &Path) -> Option<String> {
    (recorded(project, engine)?
        && dir(project)
            .join(format!("{}.fmt", stem(project)))
            .is_file())
    .then(|| format!("{}/{}", rel_dir(project), stem(project)))
}

/// Makes and checks a format for the current preamble in the background, unless that was done already.
pub(crate) fn spawn(project: Project, engine: PathBuf, path: Option<OsString>, limit: Duration) {
    if recorded(&project, &engine).is_some() {
        return;
    }
    static BUSY: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let dir = dir(&project);
    {
        let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        if busy.contains(&dir) {
            return;
        }
        busy.push(dir.clone());
    }
    tokio::spawn(async move {
        make(&project, &engine, path.as_ref(), limit).await;
        let _ = std::fs::remove_dir_all(dir.join("tmp"));
        let _ = std::fs::remove_dir_all(dir.join("check"));
        BUSY.lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|d| d != &dir);
    });
}

/// A build with the format failed where one without it worked: never use it for this preamble again.
pub(crate) fn reject(project: &Project) {
    let path = key_path(project);
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Some(rest) = text.strip_prefix("ok ")
    {
        let _ = write_atomic(&path, &format!("bad {rest}"));
    }
}

/// Whether a log is from a run that started from a precompiled preamble.
pub(crate) fn was_used(log: &str) -> bool {
    log.contains("(mylatexformat)Info: start reading document")
}

/// The log of the run that made the format in use: what the preamble said is only there.
pub(crate) fn preamble_log(project: &Project) -> String {
    std::fs::read(dir(project).join(format!("{}.log", stem(project))))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

fn dir(project: &Project) -> PathBuf {
    project.outdir_abs().join(DIR)
}

fn rel_dir(project: &Project) -> String {
    format!("{}/{DIR}", project.config.outdir.trim_end_matches('/'))
}

fn stem(project: &Project) -> String {
    project
        .main
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into())
}

fn key_path(project: &Project) -> PathBuf {
    dir(project).join(format!("{}.key", stem(project)))
}

/// The main file up to `\begin{document}`, which mylatexformat needs to find in the main file itself.
fn preamble(main: &str) -> Option<&str> {
    main.find("\\begin{document}").map(|i| &main[..i])
}

/// What the format depends on: any change means a new format, checked again. `inputs` are the
/// files the preamble read (from the format run's .fls); their size and time stand for their content.
fn key(project: &Project, engine: &Path, preamble: &str, inputs: &[String]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let stat = |p: &Path| {
        std::fs::metadata(p)
            .ok()
            .map(|m| (m.len(), m.modified().ok()))
    };
    // A new day, so dates the preamble stores are never stale for long.
    let day = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400);
    (SHIM, preamble, &project.main, stat(engine), day).hash(&mut h);
    for i in inputs {
        (i, stat(&project.root.join(i))).hash(&mut h);
    }
    // A file the preamble looks for but doesn't find yet leaves no trace in the .fls.
    tex_files(&lemmata_project::tree(
        &project.root,
        &project.config.outdir,
    ))
    .hash(&mut h);
    format!("{:016x}", h.finish())
}

fn tex_files(entries: &[lemmata_project::Entry]) -> Vec<&str> {
    let mut out = Vec::new();
    for e in entries {
        if e.is_dir {
            out.extend(tex_files(&e.children));
        } else if [
            ".tex", ".sty", ".cls", ".cfg", ".def", ".clo", ".fd", ".ltx",
        ]
        .iter()
        .any(|x| e.path.ends_with(x))
        {
            out.push(e.path.as_str());
        }
    }
    out
}

/// What was recorded for the current preamble: `Some(true)` checked and good, `Some(false)` not
/// suitable, `None` nothing yet or stale.
pub(crate) fn recorded(project: &Project, engine: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(key_path(project)).ok()?;
    let mut lines = text.lines();
    let (state, recorded_key) = lines.next()?.split_once(' ')?;
    let inputs: Vec<String> = lines.map(String::from).collect();
    let main = std::fs::read_to_string(project.main_abs()).ok()?;
    (recorded_key == key(project, engine, preamble(&main)?, &inputs)).then_some(state == "ok")
}

/// The files a run read according to its .fls, but not the main file (its preamble is hashed
/// instead) or anything in the build folder.
fn inputs(fls: &str, project: &Project) -> Vec<String> {
    let (outdir, main) = (project.outdir_abs(), project.main_abs());
    let mut v: Vec<String> = fls
        .lines()
        .filter_map(|l| l.strip_prefix("INPUT "))
        .filter(|p| {
            let abs = project.root.join(p);
            !abs.starts_with(&outdir) && abs != main
        })
        .map(String::from)
        .collect();
    v.sort();
    v.dedup();
    v
}

async fn make(project: &Project, engine: &Path, path: Option<&OsString>, limit: Duration) {
    let Some(pre) = std::fs::read_to_string(project.main_abs())
        .ok()
        .and_then(|t| preamble(&t).map(String::from))
    else {
        return;
    };
    let (dir, stem) = (dir(project), stem(project));
    let tmp = dir.join("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    let main = project.main.to_string_lossy().replace('\\', "/");
    if std::fs::create_dir_all(&tmp).is_err()
        || std::fs::write(dir.join(SHIM_FILE), SHIM.replace("@MAIN@", &main)).is_err()
    {
        return;
    }
    let started = SystemTime::now();
    let mut cmd = command(project, engine, path);
    cmd.args([
        "-ini".to_string(),
        format!("-jobname={stem}"),
        "-interaction=nonstopmode".into(),
        "-halt-on-error".into(),
        "-file-line-error".into(),
        "-no-shell-escape".into(),
        "-recorder".into(),
        format!("-output-directory={}/tmp", rel_dir(project)),
        "&pdflatex".into(),
        "mylatexformat.ltx".into(),
        format!("{}/{SHIM_FILE}", rel_dir(project)),
    ]);
    let status = run(cmd, limit).await;
    if matches!(status, Status::Cancelled | Status::Timeout) {
        return;
    }
    let fls = std::fs::read_to_string(tmp.join(format!("{stem}.fls"))).unwrap_or_default();
    let inputs = inputs(&fls, project);
    // Changed while the format was made: it may already be stale, so leave it for the next build.
    let changed = std::fs::read_to_string(project.main_abs())
        .ok()
        .is_none_or(|t| preamble(&t) != Some(pre.as_str()))
        || inputs.iter().any(|i| {
            std::fs::metadata(project.root.join(i))
                .and_then(|m| m.modified())
                .is_ok_and(|t| t > started)
        });
    if changed {
        return;
    }
    let fmt = format!("{stem}.fmt");
    // A failed dump still writes a format file; only a clean exit counts.
    let same = if status == Status::Success && tmp.join(&fmt).is_file() {
        validate(project, engine, path, limit).await
    } else {
        Some(false)
    };
    let Some(same) = same else {
        return;
    };
    let good = same
        && std::fs::rename(tmp.join(&fmt), dir.join(&fmt)).is_ok()
        && std::fs::rename(
            tmp.join(format!("{stem}.log")),
            dir.join(format!("{stem}.log")),
        )
        .is_ok();
    if !good {
        let _ = std::fs::remove_file(dir.join(&fmt));
    }
    let state = if good { "ok" } else { "bad" };
    let mut text = format!("{state} {}\n", key(project, engine, &pre, &inputs));
    for i in &inputs {
        text.push_str(i);
        text.push('\n');
    }
    let _ = write_atomic(&key_path(project), &text);
}

/// Builds once without and once with the new format, from copies of the build folder, and
/// compares everything written: the PDF apart from its /ID, the .aux and the like byte for byte.
/// `None` when a file changed meanwhile (a build ran, the user kept typing), so the runs may have
/// read different things.
async fn validate(
    project: &Project,
    engine: &Path,
    path: Option<&OsString>,
    limit: Duration,
) -> Option<bool> {
    let (outdir, check) = (project.outdir_abs(), dir(project).join("check"));
    let _ = std::fs::remove_dir_all(&check);
    let (read, built) = (sources(project), files_under(&outdir, &dir(project)));
    // The second copy is made from the first, so both runs start from the very same files.
    copy_build(&outdir, &check.join("plain"), &dir(project)).ok()?;
    copy_build(&check.join("plain"), &check.join("fmt"), &check.join("fmt")).ok()?;
    if files_under(&outdir, &dir(project)) != built {
        return None;
    }
    // Both runs see the same time, so \today and the PDF dates agree.
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .to_string();
    let fmt = format!("{}/tmp/{}", rel_dir(project), stem(project));
    let run_side = |side: &str, fmt: Option<&str>| {
        let mut cmd = command(project, engine, path);
        cmd.args(pass_args(
            project,
            &format!("{}/check/{side}", rel_dir(project)),
            fmt,
            false,
        ))
        .env("SOURCE_DATE_EPOCH", &epoch)
        .env("FORCE_SOURCE_DATE", "1");
        run(cmd, limit)
    };
    let (plain, with_fmt) = tokio::join!(run_side("plain", None), run_side("fmt", Some(&fmt)));
    if sources(project) != read || [plain, with_fmt].contains(&Status::Timeout) {
        return None;
    }
    Some(
        plain == Status::Success
            && with_fmt == Status::Success
            && same_outputs(&check.join("plain"), &check.join("fmt")),
    )
}

type Snapshot = Vec<(PathBuf, Option<(u64, SystemTime)>)>;

/// Size and time of every project file.
fn sources(project: &Project) -> Snapshot {
    fn walk(entries: &[lemmata_project::Entry], root: &Path, out: &mut Vec<PathBuf>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, root, out);
            } else {
                out.push(root.join(&e.path));
            }
        }
    }
    let mut paths = Vec::new();
    walk(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &project.root,
        &mut paths,
    );
    stat_all(paths)
}

/// Size and time of every file under `dir` but not under `skip`.
fn files_under(dir: &Path, skip: &Path) -> Snapshot {
    fn walk(dir: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                out.push(p);
            } else if p != skip {
                walk(&p, skip, out);
            }
        }
    }
    let mut paths = Vec::new();
    walk(dir, skip, &mut paths);
    stat_all(paths)
}

fn stat_all(paths: Vec<PathBuf>) -> Snapshot {
    paths
        .into_iter()
        .map(|p| {
            let stat = std::fs::metadata(&p)
                .ok()
                .and_then(|m| Some((m.len(), m.modified().ok()?)));
            (p, stat)
        })
        .collect()
}

fn command(project: &Project, engine: &Path, path: Option<&OsString>) -> Command {
    let mut cmd = Command::new(engine);
    cmd.current_dir(&project.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    cmd
}

async fn run(cmd: Command, limit: Duration) -> Status {
    // Nothing cancels a background run but its time limit; the sender must live as long as the run.
    let (_keep, mut rx) = oneshot::channel();
    run_step(cmd, limit, &mut rx)
        .await
        .ok()
        .flatten()
        .unwrap_or(Status::Failed)
}

/// Copies what TeX reads back from the build folder (.aux, .toc, .bbl, ...) with its subfolders, but not `skip`.
fn copy_build(from: &Path, to: &Path, skip: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let (src, name) = (e.path(), e.file_name());
        let n = name.to_string_lossy();
        if e.file_type()?.is_dir() {
            if src != skip {
                copy_build(&src, &to.join(&name), skip)?;
            }
        } else if !skipped(&n) {
            std::fs::copy(&src, to.join(&name))?;
        }
    }
    Ok(())
}

/// Files that differ between the two runs without the document differing: logs, and the
/// synctex file whose list of inputs lacks the preamble's files.
fn skipped(name: &str) -> bool {
    [
        ".log",
        ".synctex.gz",
        ".fls",
        ".fdb_latexmk",
        ".pdf",
        "latexmk-output.txt",
    ]
    .iter()
    .any(|x| name.ends_with(x))
}

fn same_outputs(a: &Path, b: &Path) -> bool {
    let (fa, fb) = (files(a, Path::new("")), files(b, Path::new("")));
    !fa.is_empty()
        && fa == fb
        && fa.iter().all(|f| {
            let (Ok(x), Ok(y)) = (std::fs::read(a.join(f)), std::fs::read(b.join(f))) else {
                return false;
            };
            if f.extension().is_some_and(|e| e == "pdf") {
                without_id(&x) == without_id(&y)
            } else {
                x == y
            }
        })
}

/// Every output file under `dir`, relative to it, sorted. The PDF counts; logs don't.
fn files(dir: &Path, rel: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir.join(rel))
        .into_iter()
        .flatten()
        .flatten()
    {
        let r = rel.join(e.file_name());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(files(dir, &r));
        } else if r.extension().is_some_and(|e| e == "pdf") || !skipped(&r.to_string_lossy()) {
            out.push(r);
        }
    }
    out.sort();
    out
}

/// The PDF without its /ID, which pdfTeX derives from the output path and time.
fn without_id(pdf: &[u8]) -> Vec<u8> {
    let at = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle);
    let mut out = pdf.to_vec();
    if let Some(i) = at(pdf, b"/ID [")
        && let Some(j) = pdf[i..].iter().position(|&c| c == b']')
    {
        out.drain(i..=i + j);
    }
    out
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("key-tmp");
    std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(main: &str) -> (tempfile::TempDir, Project) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("main.tex"), main).unwrap();
        let p = Project::open(d.path()).unwrap();
        (d, p)
    }

    #[test]
    fn only_plain_pdflatex_builds_qualify() {
        let (_d, mut p) = project("\\documentclass{article}\n\\begin{document}\n\\end{document}\n");
        assert!(eligible(&p, false));
        assert!(!eligible(&p, true), "MiKTeX");
        for engine in [Engine::Xelatex, Engine::Lualatex] {
            p.config.engine = engine;
            assert!(!eligible(&p, false), "{engine:?}");
        }
        p.config.engine = Engine::Pdflatex;
        p.config.draft = true;
        assert!(!eligible(&p, false), "draft");
        p.config.draft = false;
        p.config.shell_escape = true;
        assert!(!eligible(&p, false), "shell escape");
        p.config.shell_escape = false;
        p.config.fast_preamble = false;
        assert!(!eligible(&p, false), "turned off");
        p.config.fast_preamble = true;
        p.config.outdir = "my build".into();
        assert!(!eligible(&p, false), "a space in a path");
    }

    #[test]
    fn the_key_follows_the_preamble_and_what_it_read_but_not_the_body() {
        let (d, p) = project(
            "\\documentclass{article}\n\\input{macros}\n\\begin{document}\nA\n\\end{document}\n",
        );
        std::fs::write(d.path().join("macros.tex"), "\\newcommand\\x{x}").unwrap();
        let engine = d.path().join("pdflatex"); // any file stands in for the engine binary
        std::fs::write(&engine, "").unwrap();
        let inputs = vec!["macros.tex".to_string(), "/no/such/file.sty".to_string()];
        let pre = "\\documentclass{article}\n\\input{macros}\n";
        let k = key(&p, &engine, pre, &inputs);
        assert_eq!(k, key(&p, &engine, pre, &inputs), "stable");
        assert_ne!(k, key(&p, &engine, "\\documentclass{report}\n", &inputs));
        assert_ne!(k, key(&p, &engine, pre, &inputs[..1]));
        std::fs::write(d.path().join("macros.tex"), "\\newcommand\\x{yy}").unwrap();
        assert_ne!(k, key(&p, &engine, pre, &inputs), "an input changed");
        let k = key(&p, &engine, pre, &inputs);
        std::fs::write(d.path().join("chapter.tex"), "").unwrap();
        assert_ne!(k, key(&p, &engine, pre, &inputs), "a TeX file appeared");
        let k = key(&p, &engine, pre, &inputs);
        std::fs::write(d.path().join("photo.png"), "").unwrap();
        assert_eq!(
            k,
            key(&p, &engine, pre, &inputs),
            "other files don't matter"
        );
    }

    #[test]
    fn recorded_state_needs_the_current_key() {
        let (d, p) = project("\\documentclass{article}\n\\begin{document}\nA\n\\end{document}\n");
        let engine = d.path().join("pdflatex");
        std::fs::write(&engine, "").unwrap();
        assert_eq!(recorded(&p, &engine), None);
        std::fs::create_dir_all(dir(&p)).unwrap();
        let pre = "\\documentclass{article}\n";
        let write = |state: &str| {
            let k = key(&p, &engine, pre, &[]);
            std::fs::write(key_path(&p), format!("{state} {k}\n")).unwrap();
        };
        write("ok");
        assert_eq!(recorded(&p, &engine), Some(true));
        assert_eq!(usable(&p, &engine), None, "no format file");
        std::fs::write(dir(&p).join("main.fmt"), "").unwrap();
        assert_eq!(
            usable(&p, &engine).as_deref(),
            Some(".lemmata/build/fmt/main")
        );
        reject(&p);
        assert_eq!(recorded(&p, &engine), Some(false));
        assert_eq!(usable(&p, &engine), None);
        write("ok");
        // Editing the body keeps the format; editing the preamble drops it.
        std::fs::write(
            p.main_abs(),
            "\\documentclass{article}\n\\begin{document}\nB\n\\end{document}\n",
        )
        .unwrap();
        assert_eq!(recorded(&p, &engine), Some(true));
        std::fs::write(
            p.main_abs(),
            "\\documentclass{report}\n\\begin{document}\nB\n\\end{document}\n",
        )
        .unwrap();
        assert_eq!(recorded(&p, &engine), None);
        std::fs::write(p.main_abs(), "\\documentclass{article}\n\\input{body}\n").unwrap();
        assert_eq!(
            recorded(&p, &engine),
            None,
            "no \\begin{{document}} in the main file"
        );
    }

    #[test]
    fn inputs_leave_out_the_main_file_and_the_build_folder() {
        let (_d, p) = project("\\documentclass{article}\n");
        let fls = format!(
            "PWD {r}\nINPUT /usr/share/texmf-dist/tex/latex/base/article.cls\nINPUT ./main.tex\nINPUT {r}/main.tex\nINPUT ./.lemmata/build/fmt/lemmata-preamble.tex\nINPUT macros.tex\nOUTPUT .lemmata/build/fmt/tmp/main.log\nINPUT /usr/share/texmf-dist/tex/latex/base/article.cls\n",
            r = p.root.display()
        );
        assert_eq!(
            inputs(&fls, &p),
            [
                "/usr/share/texmf-dist/tex/latex/base/article.cls",
                "macros.tex"
            ]
        );
    }

    #[test]
    fn outputs_compare_byte_for_byte_except_the_pdf_id() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        for (side, id) in [(&a, "AAAA"), (&b, "BBBB")] {
            std::fs::create_dir_all(side.join("chapters")).unwrap();
            std::fs::write(
                side.join("main.pdf"),
                format!("%PDF\n1 0 obj\n<< /ID [<{id}> <{id}>] /Root 1 0 R >>\n"),
            )
            .unwrap();
            std::fs::write(side.join("main.aux"), "\\relax").unwrap();
            std::fs::write(side.join("chapters/a.aux"), "\\relax").unwrap();
            std::fs::write(side.join("main.log"), id).unwrap();
            std::fs::write(side.join("main.synctex.gz"), id).unwrap();
        }
        assert!(same_outputs(&a, &b));
        std::fs::write(b.join("chapters/a.aux"), "\\relax ").unwrap();
        assert!(!same_outputs(&a, &b), "an .aux differs");
        std::fs::write(b.join("chapters/a.aux"), "\\relax").unwrap();
        std::fs::write(b.join("main.toc"), "").unwrap();
        assert!(!same_outputs(&a, &b), "one side wrote more");
        std::fs::remove_file(b.join("main.toc")).unwrap();
        std::fs::write(
            b.join("main.pdf"),
            "%PDF\n1 0 obj\n<< /ID [<BBBB> <BBBB>] /Root 2 0 R >>\n",
        )
        .unwrap();
        assert!(!same_outputs(&a, &b), "the PDF differs");
        let empty = d.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!same_outputs(&empty, &empty), "no output is no evidence");
    }

    #[test]
    fn the_shim_inputs_the_main_file() {
        assert!(SHIM.ends_with("\\input{@MAIN@}\n"));
        assert_eq!(SHIM.matches("@MAIN@").count(), 1);
        assert_eq!(preamble("a\n\\begin{document}\nb"), Some("a\n"));
        assert_eq!(preamble("a\n\\input{x}"), None);
    }
}
