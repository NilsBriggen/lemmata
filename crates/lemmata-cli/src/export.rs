//! `lemmata export`: the project's source files in one zip (`--zip`), or an
//! arXiv-ready bundle (`--arxiv`). Both share their code with the app, in
//! `lemmata_project::export`.

use crate::{compile, open, print_diagnostics};
use anyhow::anyhow;
use lemmata_project::export;
use lemmata_tex::Status;
use serde_json::json;
use std::path::Path;

/// What `--arxiv` does besides zipping.
pub struct Arxiv {
    pub strip: bool,
    pub flatten: bool,
    /// Export even when the build has errors.
    pub anyway: bool,
}

/// Zips the project sources, or with `arxiv` the files of a fresh build.
/// Returns whether it succeeded.
pub async fn run(dir: &Path, out: &Path, arxiv: Option<Arxiv>, json: bool) -> anyhow::Result<bool> {
    let project = open(dir)?;
    let count = match arxiv {
        None => export::zip_project(&project, out, false),
        Some(a) => {
            // Like the app: build first, since the files used come from that build.
            let outcome = compile(&project).await?;
            if outcome.status != Status::Success && !a.anyway {
                if !json {
                    print_diagnostics(&outcome.diagnostics);
                }
                eprintln!(
                    "The document doesn't build without errors, so arXiv would likely reject it. \
                     Fix them, or pass --anyway to export regardless."
                );
                return Ok(false);
            }
            export::zip_arxiv(&project, out, a.strip, a.flatten)
        }
    }
    .map_err(|e| anyhow!(e))?;
    if json {
        println!("{}", json!({ "path": out, "files": count }));
    } else {
        println!("{count} files written to {}", out.display());
    }
    Ok(true)
}
