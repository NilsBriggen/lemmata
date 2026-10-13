//! `lemmata build | doctor`: the build tools without the GUI.

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use lemmata_log::Severity;
use lemmata_project::Project;
use lemmata_tex::{BuildOptions, Status};
use std::path::PathBuf;

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
    /// Check the TeX installation.
    Doctor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Cmd::Doctor => doctor(),
        Cmd::Build { dir, json } => build(dir, json).await,
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

async fn build(dir: PathBuf, json: bool) -> anyhow::Result<()> {
    let project = Project::open(&dir).with_context(|| format!("opening {}", dir.display()))?;
    let det = lemmata_tex::detect(&[]);
    let Some(latexmk) = det.tool("latexmk") else {
        bail!("latexmk not found. Run `lemmata doctor` for install instructions.");
    };
    let (_cancel, rx) = tokio::sync::oneshot::channel();
    let opts = BuildOptions {
        latexmk,
        bin_dir: det.bin_dir.as_deref(),
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
        }
        match &outcome.pdf {
            Some(p) => println!("{} in {} ms", p.display(), outcome.duration_ms),
            None => println!("no PDF written ({} ms)", outcome.duration_ms),
        }
    }
    if outcome.status != Status::Success {
        std::process::exit(1);
    }
    Ok(())
}
