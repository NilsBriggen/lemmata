//! `lemmata build | watch | doctor`: the build tools without the GUI.

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use lemmata_log::Severity;
use lemmata_project::Project;
use lemmata_tex::{BuildOptions, Status};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "lemmata",
    version,
    about = "Build LaTeX projects the way the Lemmata app does"
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
    /// Check the TeX installation.
    Doctor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Cmd::Doctor => doctor(),
        Cmd::Build { dir, json } => {
            let outcome = build(&dir, json).await?;
            if outcome.status != Status::Success {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Watch { dir, json } => watch(dir, json).await,
    }
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

async fn build(dir: &Path, json: bool) -> anyhow::Result<lemmata_tex::Outcome> {
    let project = Project::open(dir).with_context(|| format!("opening {}", dir.display()))?;
    let det = lemmata_tex::detect(&[]);
    let Some(latexmk) = det.tool("latexmk") else {
        bail!("latexmk not found. Run `lemmata doctor` for install instructions.");
    };
    let (_cancel, rx) = tokio::sync::oneshot::channel();
    let opts = BuildOptions {
        latexmk,
        bin_dir: det.bin_dir.as_deref(),
        installer: Some(det.installer(lemmata_tex::os::detect().family)),
    };
    let outcome = lemmata_tex::build(&project, &opts, rx).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome.diagnostics)?);
    } else {
        for d in &outcome.diagnostics {
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
        match &outcome.pdf {
            Some(p) => println!("{} in {} ms", p.display(), outcome.duration_ms),
            None => println!("no PDF written ({} ms)", outcome.duration_ms),
        }
    }
    Ok(outcome)
}
