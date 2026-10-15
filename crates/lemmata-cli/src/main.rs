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
    #[command(subcommand)]
    command: Cmd,
}

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
    /// Zip the project's source files (no build output) to a file.
    Export {
        /// Project folder.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The zip file to write.
        #[arg(long, value_name = "OUT.ZIP")]
        zip: PathBuf,
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
    /// Check the TeX installation.
    Doctor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ok = match Cli::parse().command {
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
        Cmd::Export { dir, zip } => return export::run(&dir, &zip),
        Cmd::Outline { dir, json } => return outline(&dir, json),
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

/// Keeps rebuilding while the project changes. Ctrl+C stops it.
async fn watch(dir: PathBuf, json: bool) -> anyhow::Result<()> {
    use notify::RecursiveMode;
    use notify_debouncer_mini::new_debouncer;
    use std::time::Duration;

    let root = Project::open(&dir)
        .with_context(|| format!("opening {}", dir.display()))?
        .root;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut debouncer = new_debouncer(Duration::from_millis(400), move |res| {
        let _ = tx.send(res);
    })?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;

    // A failing build isn't fatal here: the next edit may fix it.
    let mut last_build = std::time::SystemTime::now();
    build(&dir, json).await?;
    eprintln!(
        "Watching {} for changes. Press Ctrl+C to stop.",
        root.display()
    );
    while let Some(res) = rx.recv().await {
        let Ok(events) = res else { continue };
        // Reading a file during a build also raises an event, so only files
        // modified since the last build started count. Edits to hidden files
        // (.git, .lemmata) and our own temp files don't count either.
        let relevant = events.iter().any(|e| {
            let modified = std::fs::metadata(&e.path)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t > last_build);
            modified
                && e.path.strip_prefix(&root).is_ok_and(|rel| {
                    // The folder itself (its timestamp moves when a build starts) isn't a change.
                    rel.components().next().is_some()
                        && !rel
                            .components()
                            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                        && !rel.to_string_lossy().ends_with(".lemmata-tmp")
                })
        });
        if relevant {
            eprintln!("\nChange detected, rebuilding…");
            last_build = std::time::SystemTime::now();
            build(&dir, json).await?;
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
            None => println!("no PDF written ({} ms)", outcome.duration_ms),
        }
    }
    Ok(outcome)
}

/// Runs latexmk on the project without printing anything.
async fn compile(project: &Project) -> anyhow::Result<Outcome> {
    let det = lemmata_tex::detect(&[]);
    let Some(latexmk) = det.tool("latexmk") else {
        bail!("latexmk not found. Run `lemmata doctor` for install instructions.");
    };
    let (_cancel, rx) = tokio::sync::oneshot::channel();
    let opts = BuildOptions {
        latexmk,
        bin_dir: det.bin_dir.as_deref(),
        installer: Some(det.installer(lemmata_tex::os::detect().family)),
        miktex: matches!(det.distribution, lemmata_tex::Distribution::Miktex { .. }),
        perl_dir: det.tool("perl").and_then(|p| p.parent()),
    };
    Ok(lemmata_tex::build(project, &opts, rx).await?)
}

/// One diagnostic per block, compiler style: `file:line: error: message`, then the hint.
fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for d in diagnostics {
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
            println!("{:>8}  {}", f.words, f.file);
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
                h.file,
                h.line,
                "  ".repeat(usize::from(h.level)),
                h.title,
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
