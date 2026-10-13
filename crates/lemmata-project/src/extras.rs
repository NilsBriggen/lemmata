//! Project-wide scans the editor asks for: labels (for `\ref` completion)
//! and the detailed word count.

use super::{Entry, Project, braced, include_target, strip_comment, tree};
use crate::prose;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    pub label: String,
    pub file: String,
    /// 1-based.
    pub line: u32,
    /// The text around the label, for the completion's info box.
    pub context: String,
}

fn tex_files(entries: &[Entry], out: &mut Vec<String>) {
    for e in entries {
        if e.is_dir {
            tex_files(&e.children, out);
        } else if e.path.ends_with(".tex") {
            out.push(e.path.clone());
        }
    }
}

/// Every `\label{…}` in the project's .tex files.
pub fn labels(project: &Project) -> Vec<Label> {
    let mut files = Vec::new();
    tex_files(&tree(&project.root, &project.config.outdir), &mut files);
    let mut out = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(project.root.join(&file)) else {
            continue;
        };
        for (i, raw) in text.lines().enumerate() {
            let code = strip_comment(raw);
            let mut rest = code;
            while let Some(pos) = rest.find("\\label") {
                rest = &rest[pos + "\\label".len()..];
                if let Some(label) = braced(rest) {
                    let context = code
                        .replacen(&format!("\\label{{{label}}}"), "", 1)
                        .trim()
                        .chars()
                        .take(90)
                        .collect();
                    out.push(Label {
                        label,
                        file: file.clone(),
                        line: i as u32 + 1,
                        context,
                    });
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FileCount {
    pub file: String,
    pub words: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub total_words: u32,
    pub files: Vec<FileCount>,
    pub headings: u32,
    pub inline_math: u32,
    pub display_math: u32,
    pub floats: u32,
    pub citations: u32,
}

/// Words and document statistics, following `\input` from the main file.
pub fn counts(project: &Project) -> Counts {
    let mut w = Counter::default();
    w.file(&project.root, &project.main, 0, true);
    w.out
}

#[derive(Default)]
struct Counter {
    out: Counts,
    seen: HashSet<PathBuf>,
    prose: prose::State,
}

const FLOATS: &[&str] = &[
    "figure",
    "figure*",
    "table",
    "table*",
    "wrapfigure",
    "sidewaysfigure",
];
const DISPLAY: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "eqnarray",
    "eqnarray*",
    "displaymath",
    "flalign",
    "flalign*",
];

impl Counter {
    fn file(&mut self, root: &Path, rel: &Path, depth: usize, is_main: bool) {
        if depth > 16 || !self.seen.insert(rel.to_path_buf()) {
            return;
        }
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            return;
        };
        let name = rel.to_string_lossy().replace('\\', "/");
        let mut in_body = !is_main || !text.contains("\\begin{document}");
        let mut words = 0u32;
        let index = self.out.files.len();
        self.out.files.push(FileCount {
            file: name,
            words: 0,
        });
        for raw in text.lines() {
            let code = strip_comment(raw);
            if !in_body {
                in_body = code.contains("\\begin{document}");
                continue;
            }
            let n = prose::words(raw, &mut self.prose).len() as u32;
            words += n;
            self.out.total_words += n;
            self.stats(code);
            if let Some(child) = includes(code) {
                self.out.files[index].words = words;
                let mut child = PathBuf::from(child.trim());
                if child.extension().is_none() {
                    child.set_extension("tex");
                }
                self.file(root, &child, depth + 1, false);
                words = self.out.files[index].words;
            }
        }
        self.out.files[index].words = words;
    }

    fn stats(&mut self, code: &str) {
        let c = &mut self.out;
        for h in [
            "\\part",
            "\\chapter",
            "\\section",
            "\\subsection",
            "\\subsubsection",
            "\\paragraph",
        ] {
            let mut rest = code;
            while let Some(p) = rest.find(h) {
                rest = &rest[p + h.len()..];
                if !rest.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                    c.headings += 1;
                }
            }
        }
        for m in code.match_indices("\\begin{") {
            if let Some(env) = braced(&code[m.0 + 6..]) {
                if FLOATS.contains(&env.as_str()) {
                    c.floats += 1;
                } else if DISPLAY.contains(&env.as_str()) {
                    c.display_math += 1;
                }
            }
        }
        c.display_math +=
            code.matches("\\[").count() as u32 + code.matches("$$").count() as u32 / 2;
        c.inline_math += code.matches("\\(").count() as u32;
        let singles = code.replace("$$", "").replace("\\$", "");
        c.inline_math += singles.matches('$').count() as u32 / 2;
        for m in code.match_indices("cite") {
            // \cite, \citep, \textcite, \parencite …: count each key.
            let rest = &code[m.0 + 4..];
            let rest = rest.trim_start_matches(|ch: char| ch.is_ascii_alphabetic() || ch == '*');
            let mut rest = rest;
            while let Some(r) = rest.strip_prefix('[') {
                match r.find(']') {
                    Some(e) => rest = &r[e + 1..],
                    None => break,
                }
            }
            if m.0 > 0
                && code.as_bytes()[m.0 - 1] != b'\\'
                && !code[..m.0].ends_with(|ch: char| ch.is_ascii_alphabetic())
            {
                continue;
            }
            if let Some(keys) = braced(rest) {
                c.citations += keys.split(',').filter(|k| !k.trim().is_empty()).count() as u32;
            }
        }
    }
}

/// The file included on this line, if any.
fn includes(code: &str) -> Option<String> {
    let mut rest = code;
    while let Some(pos) = rest.find('\\') {
        rest = &rest[pos + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let after = rest[name.len()..].trim_start_matches('*');
        if let Some(t) = include_target(&name, after) {
            return Some(t);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn finds_labels_with_context() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n% \\label{hidden}\n\\input{b}\n\\end{document}\n",
        );
        write(d.path(), "b.tex", "E = mc^2 \\label{eq:e} \\label{eq:f}\n");
        let p = Project::open(d.path()).unwrap();
        let mut l = labels(&p);
        l.sort_by(|a, b| a.label.cmp(&b.label));
        let names: Vec<_> = l.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(names, ["eq:e", "eq:f", "sec:a"]);
        assert_eq!(l[2].context, "\\section{A}");
        assert_eq!(l[0].file, "b.tex");
        assert_eq!(l[0].line, 1);
    }

    #[test]
    fn counts_statistics_per_file() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\section{A}\nOne two three $x$ and \\(y\\).\n\\begin{equation}a\\end{equation}\n\\begin{figure}\\end{figure}\nSee \\cite{a,b} and \\textcite[p.~3]{c}.\n\\input{sub}\n\\end{document}\n",
        );
        write(d.path(), "sub.tex", "Four five.\n\\[ z \\]\n");
        let p = Project::open(d.path()).unwrap();
        let c = counts(&p);
        assert_eq!(c.files.len(), 2);
        assert_eq!(c.files[1].file, "sub.tex");
        assert_eq!(c.files[1].words, 2);
        assert_eq!(c.headings, 1);
        assert_eq!(c.inline_math, 2);
        assert_eq!(c.display_math, 2);
        assert_eq!(c.floats, 1);
        assert_eq!(c.citations, 3);
        assert_eq!(c.total_words, c.files.iter().map(|f| f.words).sum::<u32>());
    }
}
