//! `lemmata bib`: the project's bibliography, with unused and missing keys.

use crate::open;
use lemmata_bib::manage::{Library, ManagedEntry};
use lemmata_project::{Entry, Project};
use std::collections::HashSet;
use std::fs;
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
                println!("{key}");
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
                e.entry.key, e.entry.year, e.entry.title, e.entry.file, e.line
            );
        }
    }
    Ok(true)
}

/// Keys the .tex files cite but no .bib file defines. Keys of a hand-written
/// `thebibliography` (`\bibitem`) count as defined.
/// shortcut: the same rule as the desktop app's "missing" list, plus \bibitem keys;
/// no check of .bbl files, so a key only defined there is still reported.
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

/// Keys written as `\bibitem[label]{key}` in the project's .tex files.
fn bibitem_keys(project: &Project) -> HashSet<String> {
    fn tex(entries: &[Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                tex(&e.children, out);
            } else if e.path.ends_with(".tex") {
                out.push(e.path.clone());
            }
        }
    }
    let mut files = Vec::new();
    tex(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &mut files,
    );
    let mut keys = HashSet::new();
    for file in files {
        let Ok(text) = fs::read_to_string(project.root.join(&file)) else {
            continue;
        };
        for line in text.lines() {
            let code = line.split('%').next().unwrap_or("");
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
        }
    }
    keys
}
