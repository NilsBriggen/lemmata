//! `lemmata`: the build tools and project checks without the GUI.

mod bib;
mod export;
mod lint;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use lemmata_log::{Diagnostic, Severity};
use lemmata_project::Project;
use lemmata_tex::{BuildOptions, Outcome, Status};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "lemmata",
    version,
    about = "Build, check and export LaTeX projects the way the Lemmata app does",
    after_help = "Exit status: 0 on success, 1 when the check finds problems, 2 on usage errors. \
                  Every command takes a project folder (default: the current folder); \
                  --json prints machine-readable output."
)]
struct Cli {
    /// Let the project run programs: its latexmkrc, and shell escape when its lemmata.toml asks
    /// for it. Only for projects whose authors you trust.
    #[arg(long, global = true)]
    trust: bool,
    #[command(subcommand)]
    command: Cmd,
}

/// `--trust` was given.
static TRUST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Subcommand)]
enum Cmd {
    /// Compile a project once and print its diagnostics.
    Build {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print diagnostics as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Compile now, then again whenever a file in the project changes.
    Watch {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print diagnostics as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Compile, then report errors, missing citations and undefined labels.
    /// Exits 1 if there are any errors.
    Lint {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print diagnostics as JSON.
        #[arg(long)]
        json: bool,
        /// Warnings also fail the check (exit 1).
        #[arg(long)]
        strict: bool,
    },
    /// Count words in the document body, in total and per file.
    Wordcount {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print the full statistics as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List the bibliography entries, or only the unused or missing keys.
    /// `--missing` exits 1 when a cited key is in no .bib file.
    Bib {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Entries that no .tex file cites.
        #[arg(long, conflicts_with = "missing")]
        unused: bool,
        /// Keys that are cited but defined in no .bib file.
        #[arg(long)]
        missing: bool,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Zip the project's source files (no build output), or with --arxiv the
    /// files an upload to arXiv needs.
    Export {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Write all source files to this zip.
        #[arg(long, value_name = "OUT.ZIP", conflicts_with = "arxiv")]
        zip: Option<PathBuf>,
        /// With --zip: include the latest compiled PDF alongside the sources.
        #[arg(long, requires = "zip")]
        with_pdf: bool,
        /// Build, then write the files the build used, plus the compiled
        /// bibliography, to this zip, with comments removed.
        #[arg(long, value_name = "OUT.ZIP", required_unless_present = "zip")]
        arxiv: Option<PathBuf>,
        /// With --arxiv: merge \input and \include files into the main file.
        #[arg(long, requires = "arxiv")]
        flatten: bool,
        /// With --arxiv: keep the % comments in the .tex files.
        #[arg(long, requires = "arxiv")]
        keep_comments: bool,
        /// With --arxiv: export even if the build has errors.
        #[arg(long, requires = "arxiv")]
        anyway: bool,
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show the section tree with line numbers and word counts.
    Outline {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Remove cached build output files (.aux, .log, .bbl, ...).
    Clean {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Format a .tex file (or the project's main file) using tex-fmt or latexindent.
    Format {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Specific .tex file relative to the project folder (default: main file).
        #[arg(long)]
        file: Option<PathBuf>,
        /// Number of spaces per indent level (1..=8), or "tab".
        #[arg(long, default_value = "2")]
        indent: String,
        /// Check formatting without modifying files (exits 1 if changes are needed).
        #[arg(long)]
        check: bool,
    },
    /// Manage project snapshots (list, create, or restore).
    History {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
        #[command(subcommand)]
        action: HistoryCmd,
    },
    /// Inspect Git status, log, or branches for a trusted project.
    Git {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
        #[command(subcommand)]
        action: GitCmd,
    },
    /// Check the TeX installation.
    Doctor,
}

#[derive(Subcommand)]
enum HistoryCmd {
    /// List recorded snapshots.
    List {
        /// Maximum number of snapshots to show.
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Record a new snapshot of the project.
    Snapshot {
        /// Label for the snapshot.
        #[arg(default_value = "Snapshot")]
        label: String,
    },
    /// Restore a single file or the entire project from a snapshot.
    Restore {
        /// Snapshot commit id.
        id: String,
        /// Optional relative file path to restore (restores entire project when omitted).
        file: Option<String>,
    },
}

#[derive(Subcommand)]
enum GitCmd {
    /// Show repository branch and working-tree status.
    Status,
    /// Show recent commits.
    Log,
    /// List local branches.
    Branches,
}

/// Where the desktop app keeps the Tectonic it downloads: its data folder (Tauri's
/// app_data_dir for the identifier dev.briggen.lemmata), then `tectonic`.
fn app_tectonic_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let data = if cfg!(windows) {
        var("APPDATA")?
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))?
    };
    Some(data.join("dev.briggen.lemmata").join("tectonic"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The app's Tectonic counts as installed here too.
    if let Some(dir) = app_tectonic_dir() {
        lemmata_tex::tectonic::set_install_dir(dir);
    }
    let cli = Cli::parse();
    TRUST.store(cli.trust, std::sync::atomic::Ordering::Relaxed);
    let ok = match cli.command {
        Cmd::Doctor => return doctor(),
        Cmd::Build { dir, json } => build(&dir, json).await?.status == Status::Success,
        Cmd::Watch { dir, json } => return watch(dir, json).await,
        Cmd::Lint { dir, json, strict } => lint::run(&dir, json, strict).await?,
        Cmd::Wordcount { dir, json } => return wordcount(&dir, json),
        Cmd::Bib {
            dir,
            unused,
            missing,
            json,
        } => bib::run(&dir, bib::Filter::from_flags(unused, missing), json)?,
        Cmd::Export {
            dir,
            zip,
            with_pdf,
            arxiv,
            flatten,
            keep_comments,
            anyway,
            json,
        } => match (zip, arxiv) {
            (Some(zip), _) => export::run(&dir, &zip, with_pdf, None, json).await?,
            (None, Some(out)) => {
                let a = export::Arxiv {
                    strip: !keep_comments,
                    flatten,
                    anyway,
                };
                export::run(&dir, &out, false, Some(a), json).await?
            }
            (None, None) => unreachable!("clap requires --zip or --arxiv"),
        },
        Cmd::Outline { dir, json } => return outline(&dir, json),
        Cmd::Clean { dir, json } => return clean(&dir, json),
        Cmd::Format {
            dir,
            file,
            indent,
            check,
        } => format_cmd(&dir, file.as_deref(), &indent, check)?,
        Cmd::History { dir, json, action } => return history_cmd(&dir, json, action),
        Cmd::Git { dir, json, action } => return git_cmd(&dir, json, action),
    };
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}

fn doctor() -> anyhow::Result<()> {
    let det = lemmata_tex::detect(&[]);
    let os = lemmata_tex::os::detect();
    println!("System:       {}", os.name);
    println!("Distribution: {:?}", det.distribution);
    for t in &det.tools {
        let mark = match (&t.path, t.required) {
            (Some(_), _) => "ok     ",
            (None, true) => "MISSING",
            (None, false) => "missing",
        };
        let path = t
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        println!("  {mark} {:<9} {path}", t.name);
    }
    if det.outdated {
        println!(
            "\nThis TeX Live is older than {}. Some packages may be missing features.",
            lemmata_tex::detect::MIN_TEXLIVE_YEAR
        );
    }
    if !det.ready {
        let i = lemmata_tex::os::instructions_for(os.family);
        println!("\nTo install TeX Live on {}:\n  {}", i.label, i.recommended);
        std::process::exit(1);
    }
    Ok(())
}

/// Whether the file was written, or on Unix moved or linked in (which keeps its old modified
/// time), after `since`.
fn touched_since(m: &std::fs::Metadata, since: std::time::SystemTime) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let changed = std::time::UNIX_EPOCH
            + std::time::Duration::new(m.ctime().max(0) as u64, m.ctime_nsec().max(0) as u32);
        if changed > since {
            return true;
        }
    }
    m.modified().is_ok_and(|t| t > since)
}

fn watch_path_relevant(
    root: &Path,
    outdir: &Path,
    path: &Path,
    last_build: std::time::SystemTime,
) -> bool {
    // A file that is gone was removed (or moved away) since: that is a change too.
    let changed = match std::fs::metadata(path) {
        Ok(m) => touched_since(&m, last_build),
        Err(_) => true,
    };
    changed
        && path.strip_prefix(root).is_ok_and(|rel| {
            let outdir: PathBuf = outdir
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .collect();
            rel.components().next().is_some()
                && !rel.starts_with(&outdir)
                && !rel
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                && !rel.to_string_lossy().ends_with(".lemmata-tmp")
        })
}

/// Keeps rebuilding while the project changes. Ctrl+C stops it.
async fn watch(dir: PathBuf, json: bool) -> anyhow::Result<()> {
    use notify::RecursiveMode;
    use notify_debouncer_mini::new_debouncer;
    use std::time::Duration;

    let project = Project::open(&dir).with_context(|| format!("opening {}", dir.display()))?;
    let root = project.root.clone();
    let mut outdir = project.config.outdir.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut debouncer = new_debouncer(Duration::from_millis(400), move |res| {
        let _ = tx.send(res);
    })?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;

    // A failing build isn't fatal here: the next edit (to lemmata.toml, say) may fix it.
    let mut last_build = std::time::SystemTime::now();
    if let Err(e) = build(&dir, json).await {
        eprintln!("{e:#}");
    }
    eprintln!(
        "Watching {} for changes. Press Ctrl+C to stop.",
        root.display()
    );
    while let Some(res) = rx.recv().await {
        let events = match res {
            Ok(events) => events,
            Err(e) => {
                // Changes may be missed from here on: say so instead of watching in silence.
                eprintln!("The file watcher reported an error: {e:?}");
                continue;
            }
        };
        // Reload `outdir` when `lemmata.toml` changes so edits to `outdir` take effect live.
        if events.iter().any(|e| {
            e.path
                .file_name()
                .is_some_and(|n| n == lemmata_project::CONFIG_FILE)
        }) && let Ok(updated) = Project::open(&dir)
        {
            outdir = updated.config.outdir;
        }
        // Reading a file during a build also raises an event, so only files
        // modified since the last build started count. Hidden files, our own
        // output and our temp files don't count either.
        let relevant = events
            .iter()
            .any(|e| watch_path_relevant(&root, Path::new(&outdir), &e.path, last_build));
        if relevant {
            eprintln!("\nChange detected, rebuilding…");
            last_build = std::time::SystemTime::now();
            if let Ok(updated) = Project::open(&dir) {
                outdir = updated.config.outdir;
            }
            if let Err(e) = build(&dir, json).await {
                eprintln!("{e:#}");
            }
        }
    }
    Ok(())
}

/// Compiles the project and prints its diagnostics.
async fn build(dir: &Path, json: bool) -> anyhow::Result<Outcome> {
    let project = Project::open(dir).with_context(|| format!("opening {}", dir.display()))?;
    let outcome = compile(&project).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome.diagnostics)?);
    } else {
        print_diagnostics(&outcome.diagnostics);
        match &outcome.pdf {
            Some(p) => println!("{} in {} ms", p.display(), outcome.duration_ms),
            // Nothing changed since the last good build, so TeX didn't write the PDF again.
            None if outcome.status == Status::Success && project.pdf_path().is_file() => println!(
                "{} is up to date ({} ms)",
                project.pdf_path().display(),
                outcome.duration_ms
            ),
            None => println!("no PDF written ({} ms)", outcome.duration_ms),
        }
    }
    Ok(outcome)
}

/// Builds the project without printing anything: with latexmk, or with Tectonic when the
/// project asks for it or no TeX is installed (the app's rule).
async fn compile(project: &Project) -> anyhow::Result<Outcome> {
    // Like the app: a project runs nothing of its own unless the user says so (--trust).
    let mut project = project.clone();
    project.trusted = TRUST.load(std::sync::atomic::Ordering::Relaxed);
    let project = &project;
    let det = lemmata_tex::detect(&[]);
    let has_engine = det.tool(project.config.engine.binary()).is_some();
    if project.config.engine == lemmata_project::Engine::Tectonic
        || (det.tool("latexmk").is_none() && !has_engine && det.tectonic.is_some())
    {
        let Some(exe) = det.tectonic.as_deref() else {
            bail!(
                "This project uses Tectonic, which isn't installed. Install it from Lemmata's start screen, or put tectonic on PATH."
            );
        };
        let (_cancel, rx) = tokio::sync::oneshot::channel();
        return Ok(lemmata_tex::tectonic::build(project, exe, rx, false, &|_| {}).await?);
    }
    let latexmk = match det.tool("latexmk") {
        Some(p) => p,
        None if has_engine => Path::new(""),
        None => bail!("latexmk not found. Run `lemmata doctor` for install instructions."),
    };
    let (_cancel, rx) = tokio::sync::oneshot::channel();
    let opts = BuildOptions {
        latexmk,
        bin_dir: det.bin_dir.as_deref(),
        installer: Some(det.installer(lemmata_tex::os::detect().family)),
        miktex: matches!(det.distribution, lemmata_tex::Distribution::Miktex { .. }),
        perl_dir: det.tool("perl").and_then(|p| p.parent()),
    };
    Ok(lemmata_tex::build_with(project, &opts, rx, !latexmk.is_file(), false).await?)
}

/// `s` without control characters other than line breaks and tabs: text from a document or a
/// log mustn't reach the terminal as escape sequences that rewrite what is shown.
pub(crate) fn plain(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

/// One diagnostic per block, compiler style: `file:line: error: message`, then the hint.
fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for d in diagnostics {
        let d = Diagnostic {
            message: plain(&d.message),
            hint: d.hint.as_deref().map(plain),
            file: d.file.as_deref().map(plain),
            ..d.clone()
        };
        let loc = match (&d.file, d.line) {
            (Some(f), Some(l)) => format!("{f}:{l}: "),
            (Some(f), None) => format!("{f}: "),
            _ => String::new(),
        };
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        println!("{loc}{sev}: {}", d.message);
        if let Some(h) = &d.hint {
            println!("    {h}");
        }
        if let Some(fix) = &d.fix {
            println!("    {}", fix.explanation);
            if let Some(cmd) = &fix.command {
                println!("    Install it with: {cmd}");
            }
        }
    }
}

fn wordcount(dir: &Path, json: bool) -> anyhow::Result<()> {
    let project = open(dir)?;
    let counts = lemmata_project::extras::counts(&project);
    if json {
        println!("{}", serde_json::to_string_pretty(&counts)?);
    } else {
        println!("{} words in total", counts.total_words);
        for f in &counts.files {
            println!("{:>8}  {}", f.words, plain(&f.file));
        }
    }
    Ok(())
}

fn outline(dir: &Path, json: bool) -> anyhow::Result<()> {
    let project = open(dir)?;
    let outline = lemmata_project::outline(&project);
    if json {
        println!("{}", serde_json::to_string_pretty(&outline)?);
    } else {
        for h in &outline.headings {
            println!(
                "{}:{}: {}{} ({} words)",
                plain(&h.file),
                h.line,
                "  ".repeat(usize::from(h.level)),
                plain(&h.title),
                h.words
            );
        }
        println!("{} words in total", outline.total_words);
    }
    Ok(())
}

fn open(dir: &Path) -> anyhow::Result<Project> {
    Project::open(dir).with_context(|| format!("opening {}", dir.display()))
}

fn clean(dir: &Path, json: bool) -> anyhow::Result<()> {
    let project = open(dir)?;
    let removed = lemmata_tex::clean(&project)?;
    if json {
        println!("{}", serde_json::json!({ "removed": removed }));
    } else {
        println!("Removed {removed} cached file(s).");
    }
    Ok(())
}

fn format_cmd(
    dir: &Path,
    file: Option<&Path>,
    indent: &str,
    check_only: bool,
) -> anyhow::Result<bool> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let project = open(dir)?;
    let target = match file {
        Some(f) => {
            let rel = f.to_string_lossy().replace('\\', "/");
            let safe = lemmata_env::fs::inside_rel(&rel)
                .with_context(|| format!("{} isn't inside the project", f.display()))?;
            project.root.join(safe)
        }
        None => project.main_abs(),
    };
    let text = lemmata_env::fs::read_text(&target)
        .with_context(|| format!("reading {}", target.display()))?;
    let unit = if indent.eq_ignore_ascii_case("tab") {
        "\t".to_string()
    } else {
        let n: usize = indent
            .parse()
            .ok()
            .filter(|&n| (1..=8).contains(&n))
            .ok_or_else(|| anyhow::anyhow!("--indent must be 1..=8 or \"tab\""))?;
        " ".repeat(n)
    };
    let det = lemmata_tex::detect(&[]);
    let find = |name: &str| {
        let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        det.bin_dir
            .as_deref()
            .map(|d| d.join(&exe))
            .filter(|p| p.is_file())
            .or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|d| d.join(&exe))
                        .find(|p| p.is_file())
                })
            })
    };
    let mut cmd = if unit != "\t"
        && let Some(bin) = find("tex-fmt")
    {
        let mut c = Command::new(bin);
        c.args(["--stdin", "--tabsize", &unit.len().to_string()]);
        c
    } else if let Some(bin) = find("latexindent") {
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let y = if unit == "\t" {
            "\"\\t\"".to_string()
        } else {
            format!("'{unit}'")
        };
        let mut c = Command::new(bin);
        c.args(["-g", null, &format!("-y=defaultIndent: {y}"), "-"]);
        c
    } else {
        bail!("Formatting needs tex-fmt or latexindent.");
    };
    lemmata_env::host(&mut cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("starting formatter")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let formatted = String::from_utf8_lossy(&out.stdout).into_owned();
    if formatted == text {
        return Ok(true);
    }
    if check_only {
        println!("{} would be reformatted", target.display());
        return Ok(false);
    }
    let bytes = lemmata_env::fs::encode_like_disk(&target, &formatted)?;
    lemmata_env::fs::write_atomic(&target, &bytes)?;
    println!("Formatted {}", target.display());
    Ok(true)
}

fn history_cmd(dir: &Path, json: bool, action: HistoryCmd) -> anyhow::Result<()> {
    let project = open(dir)?;
    let trusted = TRUST.load(std::sync::atomic::Ordering::Relaxed);
    let h = match lemmata_history::History::open(&project.root, trusted) {
        Ok(h) => h,
        Err(lemmata_history::Error::NotARepo(_))
            if matches!(action, HistoryCmd::Snapshot { .. }) =>
        {
            lemmata_history::History::init(&project.root)?
        }
        Err(e) => return Err(e.into()),
    };
    let _ = h.exclude_folder(&project.config.outdir);
    match action {
        HistoryCmd::List { limit } => {
            let list = h.list(limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for s in &list {
                    println!(
                        "{}  {}  ({} file(s))",
                        &s.id[..s.id.len().min(8)],
                        plain(&s.label),
                        s.changes.len()
                    );
                }
            }
        }
        HistoryCmd::Snapshot { label } => {
            let id = h.snapshot(&label)?;
            if json {
                println!("{}", serde_json::json!({ "id": id }));
            } else if let Some(id) = id {
                println!("Saved snapshot {}", &id[..id.len().min(8)]);
            } else {
                println!("No changes since the last snapshot.");
            }
        }
        HistoryCmd::Restore { id, file } => {
            if let Some(f) = file {
                h.restore_file(&id, &f)?;
                if json {
                    println!("{}", serde_json::json!({ "restored": f }));
                } else {
                    println!("Restored {}", plain(&f));
                }
            } else {
                let n = h.restore_all(&id)?;
                if json {
                    println!("{}", serde_json::json!({ "restored_files": n }));
                } else {
                    println!("Restored {n} file(s).");
                }
            }
        }
    }
    Ok(())
}

fn git_cmd(dir: &Path, json: bool, action: GitCmd) -> anyhow::Result<()> {
    let project = open(dir)?;
    if !TRUST.load(std::sync::atomic::Ordering::Relaxed) {
        bail!(
            "Git commands are off for restricted projects (repositories can carry hooks and filters). Pass --trust to run git here."
        );
    }
    match action {
        GitCmd::Status => {
            let ov = lemmata_git::overview(&project.root);
            if json {
                println!("{}", serde_json::to_string_pretty(&ov)?);
            } else {
                match ov {
                    lemmata_git::Overview::NoGit { message } => bail!("{message}"),
                    lemmata_git::Overview::NotARepo => println!("Not a git repository."),
                    lemmata_git::Overview::Repo(s) => {
                        println!(
                            "Branch: {}",
                            s.branch.as_deref().unwrap_or("(detached HEAD)")
                        );
                        if let Some(op) = &s.operation {
                            println!("In progress: {op}");
                        }
                        for (label, list) in [
                            ("Conflicted", &s.conflicted),
                            ("Staged", &s.staged),
                            ("Unstaged", &s.unstaged),
                            ("Untracked", &s.untracked),
                        ] {
                            for c in list {
                                println!("  {:<10} {} {}", label, c.status, plain(&c.path));
                            }
                        }
                    }
                }
            }
        }
        GitCmd::Log => {
            let commits = lemmata_git::log(&project.root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&commits)?);
            } else {
                for c in &commits {
                    println!("{}  {}  ({})", c.hash, plain(&c.subject), plain(&c.date));
                }
            }
        }
        GitCmd::Branches => {
            let list = lemmata_git::branches(&project.root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for b in &list {
                    println!("{}", plain(b));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_from_a_document_never_reach_the_terminal() {
        assert_eq!(plain("a\u{1b}]52;c;x\u{7}b\u{9b}2J"), "a]52;c;xb2J");
        assert_eq!(plain("line\n\tnext"), "line\n\tnext");
    }

    #[test]
    fn watch_ignores_only_the_configured_output_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for name in [
            "build/main.pdf",
            "build/nested/main.log",
            "build-notes/main.tex",
            ".git/main.tex",
            "main.lemmata-tmp",
            "main.tex",
        ] {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let outdir = Path::new("./build");
        let since = std::time::SystemTime::UNIX_EPOCH;
        assert!(!watch_path_relevant(
            root,
            outdir,
            &root.join("build/main.pdf"),
            since
        ));
        assert!(!watch_path_relevant(
            root,
            outdir,
            &root.join("./build/main.log"),
            since
        ));
        assert!(!watch_path_relevant(
            root,
            outdir,
            &root.join("build/nested/main.log"),
            since,
        ));
        assert!(watch_path_relevant(
            root,
            outdir,
            &root.join("build-notes/main.tex"),
            since,
        ));
        assert!(!watch_path_relevant(
            root,
            outdir,
            &root.join(".git/main.tex"),
            since
        ));
        assert!(!watch_path_relevant(
            root,
            outdir,
            &root.join("main.lemmata-tmp"),
            since
        ));
        assert!(watch_path_relevant(
            root,
            outdir,
            &root.join("main.tex"),
            since
        ));
        assert!(!watch_path_relevant(root, outdir, root, since));
        // A deleted figure, and (on Unix) one moved in with an old modified time, rebuild.
        let now = std::time::SystemTime::now();
        assert!(watch_path_relevant(
            root,
            outdir,
            &root.join("figures/gone.pdf"),
            now
        ));
        #[cfg(unix)]
        {
            let moved = root.join("moved.pdf");
            let f = std::fs::File::create(&moved).unwrap();
            f.set_modified(std::time::UNIX_EPOCH).unwrap();
            let before = now - std::time::Duration::from_secs(5);
            assert!(watch_path_relevant(root, outdir, &moved, before));
        }
    }
}
