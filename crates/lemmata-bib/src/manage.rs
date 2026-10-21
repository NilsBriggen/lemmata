//! The bibliography manager's side: every entry with its place in the file,
//! duplicates, and minimal edits (remove or rename exactly one entry).

use crate::{Entry, Error, make_entry};
use biblatex::{Bibliography, ChunksExt};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Where one `@type{key, …}` sits in a .bib file (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub kind: String,
    pub key: String,
    pub start: usize,
    pub end: usize,
    pub key_start: usize,
    pub key_end: usize,
    /// 1-based line of the `@`.
    pub line: usize,
}

/// The entries of a .bib text, without `@comment`, `@string` and `@preamble`.
/// Shortcut: braces inside quoted values must balance, as BibTeX itself needs.
pub fn scan(src: &str) -> Vec<Span> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line, mut counted) = (0, 1, 0);
    while i < b.len() {
        if b[i] != b'@' {
            i += 1;
            continue;
        }
        line += src[counted..i].matches('\n').count();
        counted = i;
        let start = i;
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        let kind = src[i + 1..j].to_string();
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let close = match b.get(j) {
            Some(b'{') => b'}',
            Some(b'(') => b')',
            _ => {
                i += 1;
                continue;
            }
        };
        let body = j + 1;
        // Braces nest; a backslash escapes the next byte.
        let (mut depth, mut k) = (0usize, body);
        while k < b.len() {
            match b[k] {
                b'\\' => k += 1,
                b'{' => depth += 1,
                b'}' if close == b'}' && depth == 0 => break,
                b'}' => depth = depth.saturating_sub(1),
                c if c == close && depth == 0 => break,
                _ => {}
            }
            k += 1;
        }
        let end = (k + 1).min(b.len());
        i = end;
        if matches!(
            kind.to_lowercase().as_str(),
            "comment" | "string" | "preamble"
        ) {
            continue;
        }
        let comma = src[body..end].find(',').map_or(end, |c| body + c);
        let raw = &src[body..comma];
        let lead = raw.len() - raw.trim_start().len();
        let key = raw.trim().to_string();
        out.push(Span {
            kind,
            key_start: body + lead,
            key_end: body + lead + key.len(),
            key,
            start,
            end,
            line,
        });
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedEntry {
    #[serde(flatten)]
    pub entry: Entry,
    /// 1-based line of the entry in `file`.
    pub line: usize,
    pub doi: String,
    /// All fields, by name.
    pub fields: BTreeMap<String, String>,
    /// Whether any .tex file of the project cites the key.
    pub cited: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Duplicate {
    /// "key", "doi" or "title".
    pub kind: &'static str,
    pub value: String,
    /// Indices into `Library::entries`.
    pub entries: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct Library {
    pub files: Vec<String>,
    pub entries: Vec<ManagedEntry>,
    pub duplicates: Vec<Duplicate>,
    /// Cited in the .tex files but defined in no .bib file.
    pub missing: Vec<String>,
}

/// Every entry of `files` with its line, fields and duplicate groups.
/// `cited` is the project's cited keys (see `cited_keys`).
pub fn library(root: &Path, files: &[String], cited: &[String]) -> Library {
    let cited_set: HashSet<&str> = cited.iter().map(String::as_str).collect();
    let mut entries = Vec::new();
    for file in files {
        let Ok(src) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        for span in scan(&src) {
            let parsed = Bibliography::parse(&src[span.start..span.end]).ok();
            let bib = parsed.as_ref().and_then(|b| b.iter().next());
            let mut entry = match bib {
                Some(e) => make_entry(e, file),
                None => Entry {
                    key: span.key.clone(),
                    kind: span.kind.to_lowercase(),
                    title: String::new(),
                    authors: String::new(),
                    year: String::new(),
                    file: file.clone(),
                },
            };
            // The text's own spelling of the key, which is what \cite must match.
            entry.key = span.key.clone();
            let fields: BTreeMap<String, String> = bib
                .map(|e| {
                    e.fields
                        .iter()
                        .map(|(k, v)| (k.clone(), v.format_verbatim()))
                        .collect()
                })
                .unwrap_or_default();
            entries.push(ManagedEntry {
                cited: cited_set.contains(span.key.as_str()),
                doi: fields.get("doi").cloned().unwrap_or_default(),
                line: span.line,
                fields,
                entry,
            });
        }
    }
    let known: HashSet<&str> = entries.iter().map(|e| e.entry.key.as_str()).collect();
    let missing = cited
        .iter()
        .filter(|k| !known.contains(k.as_str()))
        .cloned()
        .collect();
    let duplicates = duplicates(&entries);
    Library {
        files: files.to_vec(),
        entries,
        duplicates,
        missing,
    }
}

fn norm_doi(doi: &str) -> String {
    let d = doi.trim().to_lowercase();
    let d = d
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("dx.doi.org/")
        .trim_start_matches("doi.org/")
        .trim_start_matches("doi:");
    d.trim().to_string()
}

fn norm_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Groups of entries sharing a key (BibTeX ignores case), a DOI or a title.
/// Titles under 10 letters are too generic to count.
pub fn duplicates(entries: &[ManagedEntry]) -> Vec<Duplicate> {
    let mut out: Vec<Duplicate> = Vec::new();
    let mut seen: HashSet<Vec<usize>> = HashSet::new();
    for kind in ["key", "doi", "title"] {
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, e) in entries.iter().enumerate() {
            let id = match kind {
                "key" => e.entry.key.to_lowercase(),
                "doi" => norm_doi(&e.doi),
                _ => norm_title(&e.entry.title),
            };
            if id.is_empty() || (kind == "title" && id.chars().count() < 10) {
                continue;
            }
            groups.entry(id).or_default().push(i);
        }
        for (value, ids) in groups {
            // Two entries with one key often share a title too; report it once.
            if ids.len() > 1 && seen.insert(ids.clone()) {
                out.push(Duplicate {
                    kind,
                    value,
                    entries: ids,
                });
            }
        }
    }
    out
}

fn find<'a>(spans: &'a [Span], key: &str, line: usize) -> Result<&'a Span, Error> {
    spans
        .iter()
        .find(|s| s.line == line && s.key == key)
        .ok_or_else(|| {
            Error(
                "That entry has changed since the list was read. Reopen the manager and try again."
                    .into(),
            )
        })
}

/// `src` without the entry `key` at `line`, and without the line(s) it
/// occupied. The rest of the file is untouched.
pub fn remove_entry(src: &str, key: &str, line: usize) -> Result<String, Error> {
    let spans = scan(src);
    let s = find(&spans, key, line)?;
    // Take the entry's whole lines, unless other text shares them.
    let line_start = src[..s.start].rfind('\n').map_or(0, |n| n + 1);
    let a = if src[line_start..s.start].trim().is_empty() {
        line_start
    } else {
        s.start
    };
    let line_end = src[s.end..].find('\n').map_or(src.len(), |n| s.end + n + 1);
    let b = if a == line_start && src[s.end..line_end].trim().is_empty() {
        line_end
    } else {
        s.end
    };
    let (head, mut tail) = (&src[..a], &src[b..]);
    // Don't leave two blank lines where one entry used to be.
    if head.is_empty() || head.ends_with("\n\n") || head.ends_with("\r\n\r\n") {
        tail = tail.trim_start_matches(['\n', '\r']);
    }
    let mut out = format!("{head}{tail}");
    if tail.is_empty() && out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}

/// A key BibTeX accepts: no spaces or any of `, { } " # % ' ( ) =`.
pub fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && !k
            .chars()
            .any(|c| c.is_whitespace() || ",{}\"#%'()=\\".contains(c))
}

/// `src` with the key of the entry `key` at `line` changed to `new_key`.
pub fn rename_entry(src: &str, key: &str, line: usize, new_key: &str) -> Result<String, Error> {
    if !valid_key(new_key) {
        return Err(Error(
            "A key can't be empty or contain spaces, commas, braces or quotes.".into(),
        ));
    }
    let spans = scan(src);
    let s = find(&spans, key, line)?;
    Ok(format!(
        "{}{new_key}{}",
        &src[..s.key_start],
        &src[s.key_end..]
    ))
}

/// A free key based on `base`: `base`, then `basea`, `baseb`, …
pub fn free_key(base: &str, taken: &[String]) -> String {
    let mut key = base.to_string();
    let mut n = 0u8;
    while taken.iter().any(|k| k.eq_ignore_ascii_case(&key)) && n < 26 {
        key = format!("{base}{}", (b'a' + n) as char);
        n += 1;
    }
    key
}

/// A crash mid-write must not leave a half-written file.
pub fn write_atomic(path: &Path, text: impl AsRef<[u8]>) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".lemmata-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIB: &str = "% my refs\n@string{acm = \"ACM\"}\n\n@article{a,\n  title = {First {One}},\n  author = {Doe, Jane},\n  year = {2020},\n  doi = {10.1/abc},\n}\n\n@book{b, title={Second}, year=2001}\n\n@misc{c,\n  title={Third},\n}\n";

    #[test]
    fn scans_entries_with_lines() {
        let s = scan(BIB);
        let got: Vec<_> = s.iter().map(|s| (s.key.as_str(), s.line)).collect();
        assert_eq!(got, [("a", 4), ("b", 11), ("c", 13)]);
        assert_eq!(
            &BIB[s[1].start..s[1].end],
            "@book{b, title={Second}, year=2001}"
        );
        assert_eq!(&BIB[s[0].key_start..s[0].key_end], "a");
    }

    #[test]
    fn removes_one_entry_and_keeps_the_rest() {
        let out = remove_entry(BIB, "b", 11).unwrap();
        assert_eq!(
            out,
            BIB.replace("@book{b, title={Second}, year=2001}\n\n", "")
        );
        let last = remove_entry(BIB, "c", 13).unwrap();
        assert!(last.ends_with("year=2001}\n"), "{last:?}");
        let first = remove_entry(BIB, "a", 4).unwrap();
        assert!(
            first.starts_with("% my refs\n@string{acm = \"ACM\"}\n\n@book{b"),
            "{first:?}"
        );
        // A stale line number must not delete anything.
        assert!(remove_entry(BIB, "b", 12).is_err());
    }

    #[test]
    fn removes_only_the_entry_when_text_shares_the_line() {
        let src = "@misc{x, title={X}} % note\n@misc{y, title={Y}}\n";
        assert_eq!(
            remove_entry(src, "x", 1).unwrap(),
            " % note\n@misc{y, title={Y}}\n"
        );
    }

    #[test]
    fn renames_the_second_of_two_equal_keys() {
        let src = "@misc{k, title={One}}\n@misc{ k , title={Two}}\n";
        let out = rename_entry(src, "k", 2, "k2").unwrap();
        assert_eq!(out, "@misc{k, title={One}}\n@misc{ k2 , title={Two}}\n");
        assert!(rename_entry(src, "k", 2, "bad key").is_err());
        assert_eq!(free_key("k", &["K".into(), "ka".into()]), "kb");
    }

    #[test]
    fn finds_duplicates_and_missing_keys() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("one.bib"),
            "@article{a, title={A Very Long Title!}, doi={10.1/X}}\n@misc{dup, title={Other}}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("two.bib"),
            "@article{a2, title={a very long title}, doi={https://doi.org/10.1/x}}\n@misc{DUP, title={Short}}\n@misc{lonely, title={Lonely entry here}}\n",
        )
        .unwrap();
        let files = vec!["one.bib".to_string(), "two.bib".to_string()];
        let cited = vec!["a".to_string(), "ghost".to_string()];
        let lib = library(d.path(), &files, &cited);
        assert_eq!(lib.entries.len(), 5);
        assert_eq!(lib.missing, ["ghost"]);
        let kinds: Vec<_> = lib
            .duplicates
            .iter()
            .map(|d| (d.kind, d.entries.clone()))
            .collect();
        // Same DOI also means same title; it is reported once, as the DOI.
        assert_eq!(kinds, [("key", vec![1, 3]), ("doi", vec![0, 2])]);
        assert!(lib.entries[0].cited && !lib.entries[1].cited);
        assert_eq!(lib.entries[3].line, 2);
    }
}
