//! The project's bibliography: which .bib files it uses, their entries for
//! the citation picker, and adding a reference by DOI or arXiv ID.

pub mod zotero;

use biblatex::{Bibliography, ChunksExt};
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub kind: String,
    pub title: String,
    /// "Knuth", "Schulman et al.", or empty.
    pub authors: String,
    pub year: String,
    /// The .bib file, relative to the project root.
    pub file: String,
}

static BIB_COMMAND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:addbibresource(?:\[[^\]]*\])?|bibliography)\{([^}]*)\}").unwrap()
});

/// The .bib files the document names (`\addbibresource`, `\bibliography`),
/// or every .bib file in the project when it names none.
pub fn bib_files(project: &lemmata_project::Project) -> Vec<String> {
    let mut named = Vec::new();
    for rel in project_tex_files(project) {
        let Ok(text) = std::fs::read_to_string(project.root.join(&rel)) else {
            continue;
        };
        for c in BIB_COMMAND.captures_iter(&text) {
            for name in c[1].split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let mut p = PathBuf::from(name);
                if p.extension().is_none() {
                    p.set_extension("bib");
                }
                let p = p.to_string_lossy().replace('\\', "/");
                if project.root.join(&p).is_file() && !named.contains(&p) {
                    named.push(p);
                }
            }
        }
    }
    if named.is_empty() {
        named = all_files(
            &lemmata_project::tree(&project.root, &project.config.outdir),
            "bib",
        );
    }
    named
}

fn project_tex_files(project: &lemmata_project::Project) -> Vec<String> {
    all_files(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        "tex",
    )
}

fn all_files(entries: &[lemmata_project::Entry], ext: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        if e.is_dir {
            out.extend(all_files(&e.children, ext));
        } else if e.name.ends_with(&format!(".{ext}")) {
            out.push(e.path.clone());
        }
    }
    out
}

static CITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\[A-Za-z]*cite[A-Za-z]*\*?(?:\[[^\]]*\]){0,2}\{([^}]*)\}").unwrap()
});

/// Every citation key the project's .tex files use, in first-use order.
pub fn cited_keys(project: &lemmata_project::Project) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for rel in project_tex_files(project) {
        let Ok(text) = std::fs::read_to_string(project.root.join(&rel)) else {
            continue;
        };
        for line in text.lines() {
            let code = line.split('%').next().unwrap_or("");
            for c in CITE.captures_iter(code) {
                for k in c[1]
                    .split(',')
                    .map(str::trim)
                    .filter(|k| !k.is_empty() && *k != "*")
                {
                    if !keys.iter().any(|x| x == k) {
                        keys.push(k.to_string());
                    }
                }
            }
        }
    }
    keys
}

/// Every entry in the given .bib files. Files that fail to parse are skipped.
pub fn entries(root: &Path, files: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    for file in files {
        let Ok(src) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        let Ok(bib) = Bibliography::parse(&src) else {
            continue;
        };
        for e in bib.iter() {
            let field = |k: &str| e.get(k).map(|c| c.format_verbatim()).unwrap_or_default();
            let year = {
                let y = field("year");
                if y.is_empty() {
                    field("date").chars().take(4).collect()
                } else {
                    y
                }
            };
            let names = if e.get("author").is_some() {
                field("author")
            } else {
                field("editor")
            };
            out.push(Entry {
                key: e.key.clone(),
                kind: e.entry_type.to_string(),
                title: clean(&field("title")),
                authors: short_authors(&names),
                year,
                file: file.clone(),
            });
        }
    }
    out
}

fn clean(s: &str) -> String {
    s.replace(['{', '}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Schulman, John and Wolski, Filip" → "Schulman et al."
fn short_authors(names: &str) -> String {
    let people: Vec<&str> = names
        .split(" and ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let surname = |p: &str| -> String {
        let p = clean(p);
        match p.split_once(',') {
            Some((last, _)) => last.trim().to_string(),
            None => p.split_whitespace().last().unwrap_or("").to_string(),
        }
    };
    match people.as_slice() {
        [] => String::new(),
        [one] => surname(one),
        [a, b] => format!("{} and {}", surname(a), surname(b)),
        [a, ..] => format!("{} et al.", surname(a)),
    }
}

static DOI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(10\.\d{4,9}/[^\s]+)").unwrap());
static ARXIV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:arxiv[:./\s]*(?:abs/|pdf/)?)?(\d{4}\.\d{4,5}|[a-z-]+(?:\.[A-Z]{2})?/\d{7})(?:v\d+)?").unwrap()
});

/// The DOI to look up for what the user pasted: a DOI, a doi.org link, an
/// arXiv ID or an arXiv link (arXiv papers have DataCite DOIs).
pub fn to_doi(input: &str) -> Option<String> {
    let input = input.trim();
    if let Some(c) = DOI.captures(input) {
        return Some(c[1].trim_end_matches(['.', ',', ';']).to_string());
    }
    let c = ARXIV.captures(input)?;
    Some(format!("10.48550/arXiv.{}", &c[1]))
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Fetches BibTeX for a DOI from doi.org (content negotiation).
pub async fn fetch_bibtex(doi: &str) -> Result<String, Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!(
            "Lemmata/",
            env!("CARGO_PKG_VERSION"),
            " (https://lemmata.briggen.dev)"
        ))
        .build()
        .map_err(|e| Error(e.to_string()))?;
    let res = client
        .get(format!("https://doi.org/{doi}"))
        .header("Accept", "application/x-bibtex; charset=utf-8")
        .send()
        .await
        .map_err(|e| Error(format!("Couldn't reach doi.org: {e}")))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error(format!("No reference found for {doi}.")));
    }
    if !res.status().is_success() {
        return Err(Error(format!("doi.org answered {}.", res.status())));
    }
    res.text().await.map_err(|e| Error(e.to_string()))
}

/// Renames the fetched entry to a readable key (`schulman2017proximal`),
/// unique among `existing`, and returns the key and the BibTeX to append.
pub fn prepare_entry(bibtex: &str, existing: &[String]) -> Result<(String, String), Error> {
    let bib = Bibliography::parse(bibtex)
        .map_err(|e| Error(format!("The reference couldn't be read: {e}")))?;
    let mut entry = bib
        .iter()
        .next()
        .cloned()
        .ok_or_else(|| Error("The reference was empty.".into()))?;
    let field = |k: &str| {
        entry
            .get(k)
            .map(|c| c.format_verbatim())
            .unwrap_or_default()
    };
    let surname = short_authors(&field("author"))
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let surname: String = surname.chars().filter(|c| c.is_alphanumeric()).collect();
    let year = {
        let y = field("year");
        if y.is_empty() {
            field("date").chars().take(4).collect()
        } else {
            y
        }
    };
    const SKIP: &[&str] = &[
        "a", "an", "the", "on", "of", "for", "and", "in", "to", "with", "towards", "toward",
    ];
    let word: String = clean(&field("title"))
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .find(|w| w.len() > 2 && !SKIP.contains(&w.as_str()))
        .unwrap_or_default();
    let base = format!(
        "{}{}{}",
        if surname.is_empty() { "ref" } else { &surname },
        year,
        word
    );
    let mut key = base.clone();
    let mut suffix = b'a';
    while existing.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
        key = format!("{base}{}", suffix as char);
        suffix += 1;
    }
    entry.key = key.clone();
    let text = entry
        .to_bibtex_string()
        .unwrap_or_else(|_| entry.to_biblatex_string());
    Ok((key, text))
}

/// Appends an entry to a .bib file, separated by a blank line.
pub fn append(path: &Path, entry: &str) -> std::io::Result<()> {
    let current = std::fs::read_to_string(path).unwrap_or_default();
    let sep = match current.len() {
        0 => "",
        _ if current.ends_with("\n\n") => "",
        _ if current.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    std::fs::write(path, format!("{current}{sep}{}\n", entry.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_dois_and_arxiv_ids() {
        assert_eq!(
            to_doi("10.1145/3292500.3330701").as_deref(),
            Some("10.1145/3292500.3330701")
        );
        assert_eq!(
            to_doi("https://doi.org/10.1038/nature14539.").as_deref(),
            Some("10.1038/nature14539")
        );
        assert_eq!(
            to_doi("arXiv:1707.06347v2").as_deref(),
            Some("10.48550/arXiv.1707.06347")
        );
        assert_eq!(
            to_doi("https://arxiv.org/abs/2106.09685").as_deref(),
            Some("10.48550/arXiv.2106.09685")
        );
        assert_eq!(
            to_doi("hep-th/9901001").as_deref(),
            Some("10.48550/arXiv.hep-th/9901001")
        );
        assert_eq!(to_doi("not an id"), None);
    }

    #[test]
    fn readable_unique_keys() {
        let fetched = "@article{Schulman_2017, title={Proximal Policy Optimization Algorithms}, author={Schulman, John and Wolski, Filip}, year={2017}, journal={arXiv}}";
        let (key, text) = prepare_entry(fetched, &[]).unwrap();
        assert_eq!(key, "schulman2017proximal");
        assert!(text.contains("@article{schulman2017proximal,"), "{text}");
        let (key2, _) = prepare_entry(fetched, &["schulman2017proximal".into()]).unwrap();
        assert_eq!(key2, "schulman2017proximala");
    }

    #[test]
    fn finds_bib_files_and_entries() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\n\\addbibresource{refs.bib}\n\\bibliography{more}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("refs.bib"),
            "@book{knuth1984, author={Knuth, Donald E.}, title={The {\\TeX}book}, year={1984}, publisher={AW}}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("more.bib"),
            "@misc{x, author={A and B and C}, title={T}, date={2020-05-01}}\n",
        )
        .unwrap();
        std::fs::write(d.path().join("unused.bib"), "@misc{y, title={U}}\n").unwrap();
        let p = lemmata_project::Project::open(d.path()).unwrap();
        let files = bib_files(&p);
        assert_eq!(files, ["refs.bib", "more.bib"]);
        let e = entries(&p.root, &files);
        assert_eq!(e[0].authors, "Knuth");
        assert_eq!(e[0].title, "The TeXbook");
        assert_eq!(e[1].authors, "A et al.");
        assert_eq!(e[1].year, "2020");
    }

    #[test]
    fn append_keeps_a_blank_line() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("refs.bib");
        std::fs::write(&p, "@misc{a, title={A}}").unwrap();
        append(&p, "@misc{b, title={B}}").unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "@misc{a, title={A}}\n\n@misc{b, title={B}}\n"
        );
    }

    /// Hits doi.org; run with `cargo test -p lemmata-bib -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn fetches_from_doi_org() {
        let bib = fetch_bibtex(&to_doi("arXiv:1707.06347").unwrap())
            .await
            .unwrap();
        let (key, _) = prepare_entry(&bib, &[]).unwrap();
        assert!(key.starts_with("schulman2017"), "{key}");
    }

    #[test]
    fn collects_cited_keys_in_order() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\nSee \\cite{a, b} and \\parencite[p.~2]{c}.\n% \\cite{hidden}\n\\textcite{a}\\nocite{*}\n",
        )
        .unwrap();
        let p = lemmata_project::Project::open(d.path()).unwrap();
        assert_eq!(cited_keys(&p), ["a", "b", "c"]);
    }
}
