//! Labels and citation keys across the project: where they are defined, where
//! they are used, and renaming one everywhere. This works without texlab, so
//! "go to label" and "rename" are there even when the language server isn't.

use super::{Entry, Project, strip_comment, tree};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `\label{x}` and the `\ref`-family that points at it.
    Label,
    /// A `@article{key,` entry and the `\cite`-family that points at it.
    Cite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Def,
    Use,
}

/// One mention of a label or citation key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Occurrence {
    pub kind: Kind,
    pub role: Role,
    pub name: String,
    pub file: String,
    /// 1-based.
    pub line: u32,
    /// Where the name starts in the line, in characters.
    pub col: u32,
    /// The whole line, trimmed, for previews.
    pub text: String,
    #[serde(skip)]
    pub byte_start: usize,
    #[serde(skip)]
    pub byte_end: usize,
}

static LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\label\s*\{([^{}]*)\}").unwrap());
static REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\([a-zA-Z]*ref)\*?(?:\[[^\]]*\])?\s*\{([^{}]*)\}").unwrap());
static CITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\[[^\]]*\]){0,2}\s*\{([^{}]*)\}").unwrap()
});
static BIB_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*@[A-Za-z]+\s*[{(]\s*([^,\s{}()]+)\s*,").unwrap());

fn files_with(entries: &[Entry], ext: &str, out: &mut Vec<String>) {
    for e in entries {
        if e.is_dir {
            files_with(&e.children, ext, out);
        } else if e.path.ends_with(ext) {
            out.push(e.path.clone());
        }
    }
}

/// Names inside one braced argument, split at commas (`\cref{a,b}`), with
/// their byte offsets in the line.
fn names_in(line: &str, group: regex::Match) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut at = group.start();
    for part in group.as_str().split(',') {
        let lead = part.len() - part.trim_start().len();
        let name = part.trim();
        if !name.is_empty() && !name.contains('\\') {
            let start = at + lead;
            out.push((name.to_string(), start, start + name.len()));
        }
        at += part.len() + 1;
    }
    let _ = line;
    out
}

/// Every label, reference, citation key and citation in one file's text.
pub fn scan_text(file: &str, text: &str, is_bib: bool) -> Vec<Occurrence> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let make = |kind, role, name: String, start: usize, end: usize| Occurrence {
            kind,
            role,
            name,
            file: file.to_string(),
            line: i as u32 + 1,
            col: raw[..start].chars().count() as u32,
            text: raw.trim().chars().take(120).collect(),
            byte_start: start,
            byte_end: end,
        };
        if is_bib {
            if let Some(g) = BIB_ENTRY.captures(raw).and_then(|c| c.get(1)) {
                out.push(make(
                    Kind::Cite,
                    Role::Def,
                    g.as_str().to_string(),
                    g.start(),
                    g.end(),
                ));
            }
            continue;
        }
        let code = strip_comment(raw);
        for c in LABEL.captures_iter(code) {
            if let Some(g) = c.get(1) {
                for (n, s, e) in names_in(code, g) {
                    out.push(make(Kind::Label, Role::Def, n, s, e));
                }
            }
        }
        for c in REF.captures_iter(code) {
            // \href{url}{text} and \hyperref take links and text, not labels.
            if matches!(c.get(1).map(|m| m.as_str()), Some("href" | "hyperref")) {
                continue;
            }
            if let Some(g) = c.get(2) {
                for (n, s, e) in names_in(code, g) {
                    out.push(make(Kind::Label, Role::Use, n, s, e));
                }
            }
        }
        for c in CITE.captures_iter(code) {
            if let Some(g) = c.get(1) {
                for (n, s, e) in names_in(code, g) {
                    out.push(make(Kind::Cite, Role::Use, n, s, e));
                }
            }
        }
    }
    out
}

/// All occurrences in the project's .tex and .bib files.
pub fn scan(project: &Project) -> Vec<Occurrence> {
    let entries = tree(&project.root, &project.config.outdir);
    let (mut tex, mut bib) = (Vec::new(), Vec::new());
    files_with(&entries, ".tex", &mut tex);
    files_with(&entries, ".bib", &mut bib);
    let mut out = Vec::new();
    for (files, is_bib) in [(tex, false), (bib, true)] {
        for f in files {
            if let Ok(text) = fs::read_to_string(project.root.join(&f)) {
                out.extend(scan_text(&f, &text, is_bib));
            }
        }
    }
    out
}

/// Replaces every mention of `old` of this kind with `new` in `text`.
/// Returns the new text and how many names changed.
pub fn rename_in_text(
    file: &str,
    text: &str,
    is_bib: bool,
    kind: Kind,
    old: &str,
    new: &str,
) -> (String, usize) {
    let hits: Vec<_> = scan_text(file, text, is_bib)
        .into_iter()
        .filter(|o| o.kind == kind && o.name == old)
        .collect();
    if hits.is_empty() {
        return (text.to_string(), 0);
    }
    // Byte offsets are per line: rebuild line by line.
    let mut out = String::with_capacity(text.len());
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        let mut line = raw.to_string();
        let mut here: Vec<_> = hits.iter().filter(|o| o.line == i as u32 + 1).collect();
        here.sort_by_key(|o| std::cmp::Reverse(o.byte_start));
        for o in here {
            line.replace_range(o.byte_start..o.byte_end, new);
        }
        out.push_str(&line);
    }
    (out, hits.len())
}

/// Is `name` usable as a label or key? No spaces, braces, commas or backslashes.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || "{}(),\\%#~".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(occ: &[Occurrence]) -> Vec<(Kind, Role, &str)> {
        occ.iter()
            .map(|o| (o.kind, o.role, o.name.as_str()))
            .collect()
    }

    #[test]
    fn finds_labels_refs_and_cites_but_not_comments() {
        let text = "\\href{http://x}{y}\\section{A}\\label{sec:a}\nSee \\ref{sec:a} and \\cref{sec:a,eq:b}, \\autoref{fig:c}.\nAs \\citep[p.~3]{knuth84, lamport94} said. % \\ref{ignored}\n\\nocite{lonely}\n";
        let got = scan_text("a.tex", text, false);
        assert_eq!(
            names(&got),
            [
                (Kind::Label, Role::Def, "sec:a"),
                (Kind::Label, Role::Use, "sec:a"),
                (Kind::Label, Role::Use, "sec:a"),
                (Kind::Label, Role::Use, "eq:b"),
                (Kind::Label, Role::Use, "fig:c"),
                (Kind::Cite, Role::Use, "knuth84"),
                (Kind::Cite, Role::Use, "lamport94"),
                (Kind::Cite, Role::Use, "lonely"),
            ]
        );
    }

    #[test]
    fn bib_entries_are_definitions_with_columns() {
        let text =
            "@article{knuth84,\n  title={x}\n}\n  @book { lamport94 , a=b }\n@comment{ignored}\n";
        let got = scan_text("r.bib", text, true);
        assert_eq!(
            names(&got),
            [
                (Kind::Cite, Role::Def, "knuth84"),
                (Kind::Cite, Role::Def, "lamport94")
            ]
        );
        assert_eq!((got[0].line, got[0].col), (1, 9));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let got = scan_text("a.tex", "ä\\label{x}", false);
        assert_eq!(got[0].col, 8);
    }

    #[test]
    fn rename_changes_every_mention_of_that_kind_only() {
        let tex = "\\label{a}\\ref{a}\\cref{b,a}\\cite{a}\n% \\ref{a}\n";
        let (new, n) = rename_in_text("m.tex", tex, false, Kind::Label, "a", "intro");
        assert_eq!(
            new,
            "\\label{intro}\\ref{intro}\\cref{b,intro}\\cite{a}\n% \\ref{a}\n"
        );
        assert_eq!(n, 3);
        let (new, n) = rename_in_text("m.tex", tex, false, Kind::Cite, "a", "knuth");
        assert_eq!(
            (new.as_str(), n),
            (
                "\\label{a}\\ref{a}\\cref{b,a}\\cite{knuth}\n% \\ref{a}\n",
                1
            )
        );
    }

    #[test]
    fn rename_in_bib_and_with_unicode_and_crlf() {
        let bib = "@article{ü1,\r\n a=b}\r\n";
        let (new, n) = rename_in_text("r.bib", bib, true, Kind::Cite, "ü1", "knuth84");
        assert_eq!((new.as_str(), n), ("@article{knuth84,\r\n a=b}\r\n", 1));
    }

    #[test]
    fn names_must_be_plain() {
        assert!(valid_name("sec:intro-2"));
        assert!(!valid_name("two words"));
        assert!(!valid_name("a,b"));
        assert!(!valid_name(""));
    }
}
