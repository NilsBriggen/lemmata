//! `lemmata bib`: the project's bibliography, with unused and missing keys.

use crate::open;
use lemmata_bib::manage::{Library, ManagedEntry};
use lemmata_project::{Entry, Project};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Unused,
    Missing,
}

impl Filter {
    pub fn from_flags(unused: bool, missing: bool) -> Filter {
        match (unused, missing) {
            (_, true) => Filter::Missing,
            (true, _) => Filter::Unused,
            _ => Filter::All,
        }
    }
}

/// Prints the chosen list and returns whether it is free of problems.
/// Only `Missing` counts as a problem: an unused entry is a tidying chore.
pub fn run(dir: &Path, filter: Filter, json: bool) -> anyhow::Result<bool> {
    let project = open(dir)?;
    if filter == Filter::Missing {
        let missing = missing_keys(&project);
        if json {
            println!("{}", serde_json::to_string_pretty(&missing)?);
        } else {
            for key in &missing {
                println!("{}", crate::plain(key));
            }
        }
        return Ok(missing.is_empty());
    }

    let files = lemmata_bib::bib_files(&project);
    let cited = lemmata_bib::cited_keys(&project);
    let lib: Library = lemmata_bib::manage::library(&project.root, &files, &cited);
    let entries: Vec<&ManagedEntry> = lib
        .entries
        .iter()
        .filter(|e| filter == Filter::All || !e.cited)
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for e in &entries {
            println!(
                "{:<28} {:<4}  {}  ({}:{})",
                crate::plain(&e.entry.key),
                crate::plain(&e.entry.year),
                crate::plain(&e.entry.title),
                crate::plain(&e.entry.file),
                e.line
            );
        }
    }
    Ok(true)
}

/// Keys the .tex files cite but neither a .bib file, a hand-written
/// `thebibliography` (`\bibitem`), nor a compiled `.bbl` defines.
pub fn missing_keys(project: &Project) -> Vec<String> {
    let files = lemmata_bib::bib_files(project);
    let cited = lemmata_bib::cited_keys(project);
    let own = bibitem_keys(project);
    lemmata_bib::manage::library(&project.root, &files, &cited)
        .missing
        .into_iter()
        .filter(|k| !own.contains(k))
        .collect()
}

/// Keys written as `\bibitem[label]{key}` in the project's `.tex` files or in compiled `.bbl`
/// files (including biblatex `\entry{key}{type}{...}` blocks).
fn bibitem_keys(project: &Project) -> HashSet<String> {
    fn tex(entries: &[Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                tex(&e.children, out);
            } else if Path::new(&e.path)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("tex") || x.eq_ignore_ascii_case("bbl"))
            {
                out.push(e.path.clone());
            }
        }
    }
    let mut files = Vec::new();
    tex(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &mut files,
    );
    let mut paths: Vec<std::path::PathBuf> =
        files.into_iter().map(|f| project.root.join(f)).collect();
    // Also check the compiled `.bbl` in the build output directory.
    let stem = project.main.file_stem().unwrap_or_default();
    let mut bbl_name = stem.to_os_string();
    bbl_name.push(".bbl");
    let built_bbl = project.outdir_abs().join(bbl_name);
    if built_bbl.is_file() {
        paths.push(built_bbl);
    }

    let mut keys = HashSet::new();
    for path in paths {
        let Ok(text) = lemmata_env::fs::read_text(&path) else {
            continue;
        };
        let is_bbl = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bbl"));
        let mut scanner = lemmata_project::CodeScanner::default();
        for line in text.lines() {
            let masked = scanner.mask_line(line);
            let code = masked.as_ref();
            let mut rest = code;
            while let Some(at) = rest.find("\\bibitem") {
                rest = &rest[at + "\\bibitem".len()..];
                let mut after = rest.trim_start();
                if let Some(opt) = after.strip_prefix('[') {
                    after = opt.split_once(']').map_or("", |(_, a)| a).trim_start();
                }
                if let Some((key, _)) = after.strip_prefix('{').and_then(|a| a.split_once('}')) {
                    keys.insert(key.trim().to_string());
                }
            }
            if is_bbl {
                let mut rest = code;
                while let Some(at) = rest.find("\\entry{") {
                    rest = &rest[at + "\\entry{".len()..];
                    if let Some((key, _)) = rest.split_once('}') {
                        keys.insert(key.trim().to_string());
                    }
                }
            }
        }
    }
    keys
}
