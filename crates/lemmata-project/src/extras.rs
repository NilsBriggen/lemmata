//! Project-wide scans the editor asks for: labels (for `\ref` completion)
//! and the detailed word count.

use super::{Entry, Project, braced, include_target, skip_optional, strip_comment, tree};
use crate::prose;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    pub label: String,
    pub file: String,
    /// 1-based.
    pub line: u32,
    /// The text around the label, for the completion's info box.
    pub context: String,
    /// What the label is attached to: the innermost environment (`equation`,
    /// `figure`, `theorem`…), a sectioning command (`section`…), or empty.
    pub env: String,
    /// The equation body, the caption, the heading or the start of the
    /// environment, at most [`SNIPPET_MAX`] characters.
    pub snippet: String,
    /// The first `\includegraphics` path inside the environment, or empty.
    pub graphic: String,
}

const SNIPPET_MAX: usize = 400;

fn tex_files(entries: &[Entry], out: &mut Vec<String>) {
    for e in entries {
        if e.is_dir {
            tex_files(&e.children, out);
        } else if e.path.ends_with(".tex") {
            out.push(e.path.clone());
        }
    }
}

/// Environments a label is not "about": the label belongs to what they contain.
const TRANSPARENT: &[&str] = &[
    "document",
    "center",
    "flushleft",
    "flushright",
    "minipage",
    "adjustbox",
    "itemize",
    "enumerate",
    "description",
];

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(?:(begin|end)\s*\{([^}]*)\}|label\s*\{([^}]*)\}|((?:sub)*section|chapter|part|paragraph)\*?)",
    )
    .unwrap()
});
static LABEL_CMD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\label\s*\{[^}]*\}|\\(?:nonumber|notag)\b").unwrap());

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The braced argument of `cmd` (after an optional `[..]`) in `body`.
fn command_arg(body: &str, cmd: &str) -> Option<String> {
    let at = body.find(cmd)?;
    braced(skip_optional(&body[at + cmd.len()..]))
}

/// What a label inside `env` points at: (snippet, graphic path).
fn describe(env: &str, body: &str) -> (String, String) {
    let base = env.trim_end_matches('*');
    let text = if matches!(
        base,
        "figure"
            | "subfigure"
            | "wrapfigure"
            | "sidewaysfigure"
            | "table"
            | "subtable"
            | "sidewaystable"
    ) {
        command_arg(body, "\\caption")
            .map(|c| squash(&c))
            .unwrap_or_default()
    } else {
        LABEL_CMD.replace_all(body, "").trim().to_string()
    };
    let graphic = command_arg(body, "\\includegraphics").unwrap_or_default();
    (
        text.chars().take(SNIPPET_MAX).collect(),
        graphic.trim().to_string(),
    )
}

struct Open {
    name: String,
    body_from: usize,
    /// Indices into the output of labels waiting for this environment's body.
    labels: Vec<usize>,
}

/// Every `\label{…}` in the project's .tex files.
pub fn labels(project: &Project) -> Vec<Label> {
    let mut files = Vec::new();
    tex_files(&tree(&project.root, &project.config.outdir), &mut files);
    let mut out: Vec<Label> = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(project.root.join(&file)) else {
            continue;
        };
        // Comments removed, newlines kept, so offsets map to the original lines.
        let lines: Vec<&str> = text.lines().map(strip_comment).collect();
        let code = lines.join("\n");
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for l in &lines {
            starts.push(at);
            at += l.len() + 1;
        }
        let mut stack: Vec<Open> = Vec::new();
        // The last sectioning command: (name, title, offset just after its title).
        let mut section: Option<(String, String, usize)> = None;
        for m in TOKEN.captures_iter(&code) {
            let whole = m.get(0).unwrap();
            if let (Some(kind), Some(name)) = (m.get(1), m.get(2)) {
                if kind.as_str() == "begin" {
                    stack.push(Open {
                        name: name.as_str().to_string(),
                        body_from: whole.end(),
                        labels: Vec::new(),
                    });
                } else if let Some(i) = stack.iter().rposition(|o| o.name == name.as_str()) {
                    let open = stack.drain(i..).next().unwrap();
                    let (snippet, graphic) =
                        describe(&open.name, &code[open.body_from..whole.start()]);
                    for l in open.labels {
                        out[l].snippet = snippet.clone();
                        out[l].graphic = graphic.clone();
                    }
                }
            } else if let Some(cmd) = m.get(4) {
                let rest = skip_optional(&code[whole.end()..]);
                let title = braced(rest).unwrap_or_default();
                let end = code.len() - rest.len() + title.len() + 2;
                section = Some((
                    cmd.as_str().to_string(),
                    squash(&title),
                    end.min(code.len()),
                ));
            } else if let Some(label) = m.get(3) {
                let pos = whole.start();
                let line = starts.partition_point(|&s| s <= pos);
                let context = lines[line - 1]
                    .replacen(whole.as_str(), "", 1)
                    .trim()
                    .chars()
                    .take(90)
                    .collect();
                let mut item = Label {
                    label: label.as_str().to_string(),
                    file: file.clone(),
                    line: line as u32,
                    context,
                    env: String::new(),
                    snippet: String::new(),
                    graphic: String::new(),
                };
                let inner = stack
                    .iter()
                    .rposition(|o| !TRANSPARENT.contains(&o.name.as_str()));
                if let Some(i) = inner {
                    item.env = stack[i].name.clone();
                    stack[i].labels.push(out.len());
                } else if let Some((cmd, title, end)) = &section
                    && *end <= pos
                    && code[*end..pos].trim().is_empty()
                {
                    item.env = cmd.clone();
                    item.snippet = title.chars().take(SNIPPET_MAX).collect();
                }
                out.push(item);
            }
        }
        // Environments never closed: describe what follows.
        for open in stack {
            let (snippet, graphic) = describe(&open.name, &code[open.body_from..]);
            for l in open.labels {
                out[l].snippet = snippet.clone();
                out[l].graphic = graphic.clone();
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
        assert_eq!(l[2].env, "section");
        assert_eq!(l[2].snippet, "A");
    }

    #[test]
    fn labels_know_their_environment() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\begin{align}\n a &= b \\label{eq:a} \\\\\n c &= d\n\\end{align}\n\\begin{figure}[h]\n\\centering\n\\includegraphics[width=3cm]{img/p.png}\n\\caption{A\n  plot.}\\label{fig:p}\n\\end{figure}\n\\begin{theorem}Be\\label{thm:t} % c\n\\end{theorem}\n\\item\\label{loose}\n\\end{document}\n",
        );
        let p = Project::open(d.path()).unwrap();
        let l = labels(&p);
        let get = |n: &str| l.iter().find(|x| x.label == n).unwrap();
        assert_eq!(get("eq:a").env, "align");
        assert!(get("eq:a").snippet.contains("c &= d") && !get("eq:a").snippet.contains("label"));
        assert_eq!(get("eq:a").line, 4);
        assert_eq!(get("fig:p").env, "figure");
        assert_eq!(get("fig:p").snippet, "A plot.");
        assert_eq!(get("fig:p").graphic, "img/p.png");
        assert_eq!(get("thm:t").env, "theorem");
        assert_eq!(get("loose").env, "");
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
