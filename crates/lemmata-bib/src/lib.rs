//! The project's bibliography: which .bib files it uses, their entries for
//! the citation picker, and adding a reference by DOI or arXiv ID.

pub mod manage;
pub mod search;
pub mod zotero;

use biblatex::{Bibliography, ChunksExt};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
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
    /// All authors (else editors) as written, for searching.
    pub names: String,
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
    bib_files_with_tree(
        project,
        &lemmata_project::tree(&project.root, &project.config.outdir),
    )
}

pub fn bib_files_with_tree(
    project: &lemmata_project::Project,
    entries: &[lemmata_project::Entry],
) -> Vec<String> {
    let sources = lemmata_project::source_snapshot(project, entries);
    bib_files_with_sources(project, entries, &sources)
}

pub fn bib_files_with_sources(
    project: &lemmata_project::Project,
    entries: &[lemmata_project::Entry],
    sources: &lemmata_project::Sources,
) -> Vec<String> {
    let mut named = Vec::new();
    for rel in project_tex_files(entries) {
        let Some(text) = sources.get(Path::new(&rel)) else {
            continue;
        };
        for c in BIB_COMMAND.captures_iter(&lemmata_project::export::strip_comments(text)) {
            for name in c[1].split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let mut p = PathBuf::from(name);
                if p.extension().is_none() {
                    p.set_extension("bib");
                }
                let p = p.to_string_lossy().replace('\\', "/");
                // Outside the project only for a trusted one: on Windows a network path makes
                // the computer contact that server.
                let inside = lemmata_env::fs::inside_rel(p.trim_start_matches("./")).is_some();
                if (inside || project.trusted)
                    && project.root.join(&p).is_file()
                    && !named.contains(&p)
                {
                    named.push(p);
                }
            }
        }
    }
    if named.is_empty() {
        named = all_files(entries, "bib");
    }
    named
}

fn project_tex_files(entries: &[lemmata_project::Entry]) -> Vec<String> {
    all_files(entries, "tex")
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
    Regex::new(r"\\([A-Za-z]*[Cc]ite[A-Za-z]*)\*?(?:\s*\[[^\]]*\]){0,2}\s*\{([^{}]*)\}").unwrap()
});
/// The next argument of `\parencites[pre]{a}[post]{b}{c}`, after the first.
static MORE_KEYS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\[[^\]]*\]\s*){0,2}\{([^{}]*)\}").unwrap());

/// Every citation key the project's .tex files use, in first-use order. Shortcut: comment-looking
/// text inside a verbatim block counts as a comment.
pub fn cited_keys(project: &lemmata_project::Project) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut add = |group: &str| {
        for k in group
            .replace('%', "")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty() && *k != "*" && !k.starts_with('#'))
        {
            if !keys.iter().any(|x| x == k) {
                keys.push(k.to_string());
            }
        }
    };
    for rel in project_tex_files(&lemmata_project::tree(
        &project.root,
        &project.config.outdir,
    )) {
        let Ok(text) = lemmata_env::fs::read_text(project.root.join(&rel)) else {
            continue;
        };
        let code = lemmata_project::export::strip_comments(&text);
        for c in CITE.captures_iter(&code) {
            let name = &c[1];
            if matches!(
                name,
                "setcitestyle" | "citestyle" | "defcitealias" | "citetext"
            ) {
                continue;
            }
            add(&c[2]);
            // \parencites{a}{b}: every group is a list of keys.
            let mut end = c.get(0).map_or(0, |m| m.end());
            while name.ends_with('s')
                && let Some(m) = MORE_KEYS.captures(&code[end..])
            {
                add(&m[1]);
                end += m.get(0).map_or(0, |m| m.end());
            }
        }
    }
    keys
}

/// `Bibliography::parse` made safe for any file. biblatex resolves `@string` names and
/// `crossref`/`xdata` parents by recursion, so a loop among them, a very long chain, or braces
/// nested thousands deep would overflow the stack and end the app. Such links are cut first:
/// a looping `@string` stands for its own name, and a looping or too deep entry loses its
/// `crossref` and `xdata`; the rest of the file reads as usual.
pub fn parse_bib(src: &str) -> Result<Bibliography, String> {
    const MAX_DEPTH: usize = 64;
    let (mut depth, mut deepest) = (0usize, 0usize);
    for b in src.bytes() {
        match b {
            b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if deepest > MAX_DEPTH {
        return Err(format!("braces are nested more than {MAX_DEPTH} deep"));
    }
    let mut raw = biblatex::RawBibliography::parse(src).map_err(|e| e.to_string())?;

    // @string names that lead back to themselves.
    let uses: HashMap<&str, Vec<&str>> = raw
        .abbreviations
        .iter()
        .map(|p| {
            let names = p.value.v.iter().filter_map(|c| match c.v {
                biblatex::RawChunk::Abbreviation(n) => Some(n),
                biblatex::RawChunk::Normal(_) => None,
            });
            (p.key.v, names.collect())
        })
        .collect();
    let looping = cyclic(&uses, MAX_DEPTH);
    for p in &mut raw.abbreviations {
        if looping.contains(p.key.v) {
            let span = p.value.span.clone();
            p.value.v = vec![biblatex::Spanned::new(
                biblatex::RawChunk::Normal(p.key.v),
                span,
            )];
        }
    }

    // Entries whose crossref/xdata parents loop or go on too long.
    let text = |f: &biblatex::Field| -> String {
        f.iter()
            .map(|c| match c.v {
                biblatex::RawChunk::Normal(s) | biblatex::RawChunk::Abbreviation(s) => s,
            })
            .collect()
    };
    let parents: HashMap<&str, Vec<String>> = raw
        .entries
        .iter()
        .map(|e| {
            let mut ps = Vec::new();
            for f in &e.v.fields {
                let k = f.key.v.to_ascii_lowercase();
                if k == "crossref" || k == "xdata" {
                    ps.extend(text(&f.value.v).split(',').map(|s| s.trim().to_string()));
                }
            }
            (e.v.key.v, ps)
        })
        .collect();
    let graph: HashMap<&str, Vec<&str>> = parents
        .iter()
        .map(|(k, ps)| (*k, ps.iter().map(String::as_str).collect()))
        .collect();
    let cut = cyclic(&graph, MAX_DEPTH);
    for e in &mut raw.entries {
        if cut.contains(e.v.key.v) {
            e.v.fields.retain(|f| {
                let k = f.key.v.to_ascii_lowercase();
                k != "crossref" && k != "xdata"
            });
        }
    }
    Bibliography::from_raw(raw).map_err(|e| e.to_string())
}

/// The nodes of `graph` from which following edges loops, or runs longer than `max` steps.
/// Without recursion, so no input can overflow the stack here either.
fn cyclic<'a>(
    graph: &HashMap<&'a str, Vec<&'a str>>,
    max: usize,
) -> std::collections::HashSet<&'a str> {
    let mut bad = std::collections::HashSet::new();
    for &start in graph.keys() {
        // Depth-first, the path kept: a node seen again on it is a loop.
        let mut path: Vec<(&str, usize)> = vec![(start, 0)];
        while let Some(&mut (node, ref mut next)) = path.last_mut() {
            let kids = graph.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if *next >= kids.len() {
                path.pop();
                continue;
            }
            let kid = kids[*next];
            *next += 1;
            if path.iter().any(|(n, _)| *n == kid) || path.len() > max {
                bad.insert(start);
                break;
            }
            if graph.contains_key(kid) {
                path.push((kid, 0));
            }
        }
    }
    bad
}

/// Every entry in the given .bib files. A file that doesn't parse as a whole (a duplicate key,
/// an unknown `@string`) is read entry by entry, so one bad entry doesn't hide the rest.
pub fn entries(root: &Path, files: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    for file in files {
        let Ok(src) = lemmata_env::fs::read_text(root.join(file)) else {
            continue;
        };
        match parse_bib(&src) {
            Ok(bib) => out.extend(bib.iter().map(|e| make_entry(e, file))),
            Err(_) => out.extend(
                manage::parse_spans(&src)
                    .iter()
                    .map(|(s, e)| manage::entry_of(s, e, file)),
            ),
        }
    }
    out
}

fn make_entry(e: &biblatex::Entry, file: &str) -> Entry {
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
    Entry {
        key: e.key.clone(),
        kind: e.entry_type.to_string(),
        title: clean(&field("title")),
        authors: short_authors(&names),
        names,
        year,
        file: file.to_string(),
    }
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
        return Some(trim_doi(&c[1]).to_string());
    }
    let c = ARXIV.captures(input)?;
    Some(format!("10.48550/arXiv.{}", &c[1]))
}

/// A DOI without the punctuation that follows it in prose: `.,;`, and a closing bracket or quote
/// that has no opening one inside the DOI (`(doi:10.1/x)`, `<https://doi.org/10.1/x>`).
fn trim_doi(doi: &str) -> &str {
    let mut d = doi;
    while let Some(last) = d.chars().last() {
        let open = match last {
            '.' | ',' | ';' | '>' | '"' | '\'' | '}' => None,
            ')' => Some('('),
            ']' => Some('['),
            _ => break,
        };
        if open.is_some_and(|o| d.matches(o).count() >= d.matches(last).count()) {
            break;
        }
        d = &d[..d.len() - last.len_utf8()];
    }
    d
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A client builder with ring installed as rustls' crypto provider (reqwest
/// is built without its own, so only one TLS crypto stack is compiled in).
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    static PROVIDER: std::sync::Once = std::sync::Once::new();
    PROVIDER.call_once(|| {
        // Err means another provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
}

/// Fetches BibTeX for a DOI from doi.org (content negotiation).
pub async fn fetch_bibtex(doi: &str) -> Result<String, Error> {
    let client = client_builder()
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
    let bib =
        parse_bib(bibtex).map_err(|e| Error(format!("The reference couldn't be read: {e}")))?;
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
    let key = unique_key(base, existing);
    entry.key = key.clone();
    let text = entry
        .to_bibtex_string()
        .unwrap_or_else(|_| entry.to_biblatex_string());
    Ok((key, text))
}

/// `base`, or `basea`, `baseb`, … (then `base26`, …) when `existing` has it (without case).
fn unique_key(base: String, existing: &[String]) -> String {
    let mut key = base.clone();
    let mut n = 0u32;
    while existing.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
        key = match u8::try_from(n).ok().filter(|n| *n < 26) {
            Some(n) => format!("{base}{}", (b'a' + n) as char),
            None => format!("{base}{n}"),
        };
        n += 1;
    }
    key
}

/// The entries of BibTeX the user pasted; none unless the text starts with `@` and parses.
pub fn parse_pasted(text: &str) -> Vec<biblatex::Entry> {
    let text = text.trim();
    if !text.starts_with('@') {
        return Vec::new();
    }
    parse_bib(text).map_or_else(|_| Vec::new(), |b| b.iter().cloned().collect())
}

/// What to append for pasted BibTeX, one `(key, text)` per entry. `known` are the library's
/// `(key, doi)`. The pasted key stays when it is free, else gets a letter; an entry whose DOI is
/// already in the library (or earlier in the paste) is not added again: its text is `None`.
pub fn prepare_pasted(
    text: &str,
    known: &[(String, String)],
) -> Result<Vec<(String, Option<String>)>, Error> {
    let entries = parse_pasted(text);
    if entries.is_empty() {
        return Err(Error("That isn't BibTeX.".into()));
    }
    let mut keys: Vec<String> = known.iter().map(|k| k.0.clone()).collect();
    let mut dois: Vec<(String, String)> = known
        .iter()
        .filter(|k| !k.1.is_empty())
        .map(|k| (manage::norm_doi(&k.1), k.0.clone()))
        .collect();
    let mut out = Vec::new();
    for mut e in entries {
        let doi = e
            .get("doi")
            .map(|c| manage::norm_doi(&c.format_verbatim()))
            .unwrap_or_default();
        if let Some((_, k)) = dois.iter().find(|(d, _)| !doi.is_empty() && *d == doi) {
            out.push((k.clone(), None));
            continue;
        }
        let key = unique_key(e.key.clone(), &keys);
        e.key = key.clone();
        keys.push(key.clone());
        if !doi.is_empty() {
            dois.push((doi, key.clone()));
        }
        let text = e
            .to_bibtex_string()
            .unwrap_or_else(|_| e.to_biblatex_string());
        out.push((key, Some(text)));
    }
    Ok(out)
}

/// Appends an entry to a .bib file, separated by a blank line. The file's bytes are kept as they
/// are (it may not be UTF-8), and it is replaced in one step.
pub fn append(path: &Path, entry: &str) -> std::io::Result<()> {
    let mut bytes = match lemmata_env::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let sep: &[u8] = match bytes.as_slice() {
        [] => b"",
        [.., b'\n', b'\n'] => b"",
        [.., b'\n'] => b"\n",
        _ => b"\n\n",
    };
    bytes.extend_from_slice(sep);
    bytes.extend_from_slice(entry.trim_end().as_bytes());
    bytes.push(b'\n');
    manage::write_atomic(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of these once overflowed the stack inside biblatex, which ends the whole app.
    #[test]
    fn loops_and_deep_nesting_in_a_bib_file_dont_crash() {
        let ok = |src: &str| parse_bib(src).unwrap_or_else(|e| panic!("{e}: {src}"));
        let b = ok("@string{a = b}\n@string{b = a}\n@misc{k, title = a}\n");
        assert_eq!(b.get("k").unwrap().title().unwrap().format_verbatim(), "a");
        let b = ok("@misc{k, crossref = {k}, title = {T}}\n");
        assert!(b.get("k").is_some());
        ok(
            "@misc{a, crossref = {b}}\n@misc{b, xdata = {c}}\n@misc{c, crossref = {a}}\n@misc{d, crossref = {a}, title = {D}}\n",
        );
        let chain: String = (0..500)
            .map(|i| format!("@misc{{e{i}, crossref = {{e{}}}, title = {{T}}}}\n", i + 1))
            .collect();
        ok(&chain);
        let deep = format!(
            "@misc{{k, title = {}{}}}\n",
            "{".repeat(5000),
            "}".repeat(5000)
        );
        assert!(parse_bib(&deep).is_err());
        // An ordinary crossref still resolves.
        let b = ok(
            "@book{p, title = {Proceedings}, year = {2020}}\n@inproceedings{c, crossref = {p}, title = {Paper}}\n",
        );
        // biblatex keeps an inherited year as `date`, and the book's title as `booktitle`.
        assert!(
            b.get("c").unwrap().get("booktitle").is_some(),
            "inherited from its parent"
        );
    }

    #[test]
    fn pasted_bibtex_gets_free_keys_and_skips_known_dois() {
        let known = [
            ("smith".to_string(), "10.1/known".to_string()),
            ("old".to_string(), String::new()),
        ];
        let paste = "@article{smith, title={New One}, doi={10.1/new}}\n@book{old, title={Again}}\n@misc{dup, doi={https://doi.org/10.1/KNOWN}}";
        let got = prepare_pasted(paste, &known).unwrap();
        assert_eq!(got[0].0, "smitha");
        assert!(got[0].1.as_deref().unwrap().contains("smitha"));
        assert_eq!(got[1].0, "olda");
        assert_eq!(got[2], ("smith".to_string(), None));
        assert!(prepare_pasted("just words", &known).is_err());
        assert!(parse_pasted("@article{a, title={Half").is_empty());
    }

    #[test]
    fn appending_keeps_a_file_that_isnt_utf8() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("refs.bib");
        std::fs::write(&p, b"@book{m, author={M\xfcller}}\n").unwrap();
        append(&p, "@misc{x, title={X}}").unwrap();
        assert_eq!(
            std::fs::read(&p).unwrap(),
            b"@book{m, author={M\xfcller}}\n\n@misc{x, title={X}}\n"
        );
        let new = d.path().join("new.bib");
        append(&new, "@misc{y}").unwrap();
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "@misc{y}\n");
    }

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
    fn bibliography_discovery_matches_the_supplied_source_snapshot() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\n\\addbibresource{refs.bib}\n",
        )
        .unwrap();
        std::fs::write(d.path().join("refs.bib"), "@misc{x, title={X}}\n").unwrap();
        std::fs::write(d.path().join("snapshot.bib"), "@misc{y, title={Y}}\n").unwrap();
        let p = lemmata_project::Project::open(d.path()).unwrap();
        let tree = lemmata_project::tree(&p.root, &p.config.outdir);
        let mut sources = lemmata_project::Sources::new();
        sources.insert(
            PathBuf::from("main.tex"),
            "\\addbibresource{snapshot.bib}\n".into(),
        );
        assert_eq!(
            bib_files_with_sources(&p, &tree, &sources),
            ["snapshot.bib"]
        );
        sources.clear();
        assert_eq!(
            bib_files_with_sources(&p, &tree, &sources),
            ["refs.bib", "snapshot.bib"]
        );
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
        std::fs::write(
            d.path().join("main.tex"),
            "\\setcitestyle{authoryear,round}\nup 50\\% \\cite{x}\n\\cite{a,\n b} \\parencites[p.~1]{c}[2]{d}{e}\n\\Textcite{f}\n% \\cite{hidden}\n\\cite{ unfinished\n\\section{Intro}\n",
        )
        .unwrap();
        assert_eq!(cited_keys(&p), ["x", "a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn a_commented_out_bib_resource_is_not_used() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("old.bib"), "@misc{o}\n").unwrap();
        std::fs::write(d.path().join("refs.bib"), "@misc{r}\n").unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "\\documentclass{article}\n% \\addbibresource{old.bib}\n\\addbibresource{refs.bib}\n",
        )
        .unwrap();
        let p = lemmata_project::Project::open(d.path()).unwrap();
        assert_eq!(bib_files(&p), ["refs.bib"]);
    }

    #[test]
    fn one_bad_entry_does_not_hide_the_file() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("refs.bib"),
            "@string{jfm = {J. Fluid Mech.}}\n@article{a, title={One Title Here}, journal=jfm}\n@article{a, title={Dup}}\n@article{b, title={Bee}, journal=nosuchstring}\n",
        )
        .unwrap();
        let got = entries(d.path(), &["refs.bib".to_string()]);
        let titles: Vec<_> = got
            .iter()
            .map(|e| (e.key.as_str(), e.title.as_str()))
            .collect();
        assert_eq!(titles, [("a", "One Title Here"), ("a", "Dup"), ("b", "")]);
    }

    #[test]
    fn a_latin1_bib_file_is_read() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("refs.bib"),
            b"@book{m, author={M\xfcller, A}}\n",
        )
        .unwrap();
        let got = entries(d.path(), &["refs.bib".to_string()]);
        assert_eq!(got[0].authors, "M\u{fc}ller");
    }

    #[test]
    fn dois_lose_the_brackets_and_quotes_around_them() {
        for s in [
            "(doi:10.1038/nature14539)",
            "<https://doi.org/10.1038/nature14539>",
            "\"10.1038/nature14539\".",
            "[10.1038/nature14539]",
        ] {
            assert_eq!(to_doi(s).as_deref(), Some("10.1038/nature14539"), "{s}");
        }
        // A bracket that belongs to the DOI stays.
        assert_eq!(
            to_doi("10.1002/(SICI)1097-4571(199806)49:8<693::AID-ASI4>3.0.CO;2-O").as_deref(),
            Some("10.1002/(SICI)1097-4571(199806)49:8<693::AID-ASI4>3.0.CO;2-O")
        );
    }
}
