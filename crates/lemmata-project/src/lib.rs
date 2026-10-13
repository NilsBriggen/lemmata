//! A Lemmata project is a plain folder. `lemmata.toml` is optional; without
//! it the main file is the `.tex` file containing `\documentclass`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const CONFIG_FILE: &str = "lemmata.toml";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not a folder")]
    NotADirectory(PathBuf),
    #[error("no .tex file with \\documentclass found in {0}")]
    NoMainFile(PathBuf),
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("lemmata.toml is invalid: {0}")]
    Config(#[from] toml::de::Error),
    #[error("could not write lemmata.toml: {0}")]
    ConfigWrite(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Pdflatex,
    Xelatex,
    Lualatex,
}

impl Engine {
    /// The latexmk flag selecting this engine.
    pub fn latexmk_flag(self) -> &'static str {
        match self {
            Engine::Pdflatex => "-pdf",
            Engine::Xelatex => "-xelatex",
            Engine::Lualatex => "-lualatex",
        }
    }

    pub fn binary(self) -> &'static str {
        match self {
            Engine::Pdflatex => "pdflatex",
            Engine::Xelatex => "xelatex",
            Engine::Lualatex => "lualatex",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bib {
    #[default]
    Biber,
    Bibtex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub main: Option<String>,
    pub engine: Engine,
    pub bib: Bib,
    /// Lets the document run shell commands. Off by default because opening
    /// someone else's project must never run their code.
    pub shell_escape: bool,
    pub outdir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            main: None,
            engine: Engine::default(),
            bib: Bib::default(),
            shell_escape: false,
            outdir: ".lemmata/build".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
    /// Main file, relative to `root`.
    pub main: PathBuf,
    /// Every file that could be the main file, relative to `root`.
    pub main_candidates: Vec<PathBuf>,
}

impl Project {
    pub fn open(root: impl AsRef<Path>) -> Result<Project> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(Error::NotADirectory(root.to_path_buf()));
        }
        let root = root.canonicalize().map_err(|source| Error::Io {
            path: root.into(),
            source,
        })?;
        let config = read_config(&root)?;
        let main_candidates = find_main_candidates(&root, &config.outdir);
        let main = match &config.main {
            Some(m) => PathBuf::from(m),
            None => pick_main(&main_candidates).ok_or_else(|| Error::NoMainFile(root.clone()))?,
        };
        Ok(Project {
            root,
            config,
            main,
            main_candidates,
        })
    }

    pub fn main_abs(&self) -> PathBuf {
        self.root.join(&self.main)
    }

    pub fn outdir_abs(&self) -> PathBuf {
        self.root.join(&self.config.outdir)
    }

    /// The PDF latexmk writes for the main file.
    pub fn pdf_path(&self) -> PathBuf {
        let stem = self.main.file_stem().unwrap_or_default();
        self.outdir_abs().join(stem).with_extension("pdf")
    }

    pub fn log_path(&self) -> PathBuf {
        self.pdf_path().with_extension("log")
    }

    pub fn save_config(&self) -> Result<()> {
        let text = toml::to_string_pretty(&self.config)?;
        let path = self.root.join(CONFIG_FILE);
        fs::write(&path, text).map_err(|source| Error::Io { path, source })
    }
}

fn read_config(root: &Path) -> Result<Config> {
    let path = root.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(source) => Err(Error::Io { path, source }),
    }
}

fn is_hidden_or_build(entry: &walkdir::DirEntry, outdir: &str) -> bool {
    let name = entry.file_name().to_string_lossy();
    entry.depth() > 0
        && (name.starts_with('.') || name == "node_modules" || entry.path().ends_with(outdir))
}

fn find_main_candidates(root: &Path, outdir: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_build(e, outdir))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "tex"))
        .filter(|e| fs::read_to_string(e.path()).is_ok_and(|t| has_documentclass(&t)))
        .filter_map(|e| e.path().strip_prefix(root).ok().map(Path::to_path_buf))
        .collect();
    found.sort_by_key(|p| (p.components().count(), p.clone()));
    found
}

fn has_documentclass(text: &str) -> bool {
    text.lines().any(|l| {
        let code = l.split('%').next().unwrap_or("");
        code.contains("\\documentclass")
    })
}

/// Prefers `main.tex`, then the shallowest candidate.
fn pick_main(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == "main.tex"))
        .or_else(|| candidates.first())
        .cloned()
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    /// Relative to the project root, with forward slashes.
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<Entry>,
}

/// The project's files for the sidebar, folders first, hidden files and the
/// build folder left out.
pub fn tree(root: &Path, outdir: &str) -> Vec<Entry> {
    fn walk(dir: &Path, root: &Path, outdir: &str) -> Vec<Entry> {
        let Ok(read) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut entries: Vec<Entry> = read
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                !name.starts_with('.') && name != "node_modules" && !e.path().ends_with(outdir)
            })
            .map(|e| {
                let path = e.path();
                let is_dir = path.is_dir();
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: rel,
                    is_dir,
                    children: if is_dir {
                        walk(&path, root, outdir)
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        entries
    }
    walk(root, root, outdir)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    /// 0 = \part, 1 = \chapter, 2 = \section, 3 = \subsection, 4 = \subsubsection.
    pub level: u8,
    pub title: String,
    /// Relative to the project root, with forward slashes.
    pub file: String,
    /// 1-based.
    pub line: u32,
}

const HEADINGS: &[(&str, u8)] = &[
    ("part", 0),
    ("chapter", 1),
    ("section", 2),
    ("subsection", 3),
    ("subsubsection", 4),
];

/// The document's headings in reading order, following `\input`,
/// `\include` and `\subfile` from the main file.
pub fn outline(project: &Project) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk_outline(&project.root, &project.main, &mut out, &mut seen, 0);
    out
}

fn walk_outline(
    root: &Path,
    rel: &Path,
    out: &mut Vec<Heading>,
    seen: &mut std::collections::HashSet<PathBuf>,
    depth: usize,
) {
    if depth > 16 || !seen.insert(rel.to_path_buf()) {
        return;
    }
    let Ok(text) = fs::read_to_string(root.join(rel)) else {
        return;
    };
    let file = rel.to_string_lossy().replace('\\', "/");
    for (i, raw) in text.lines().enumerate() {
        let code = strip_comment(raw);
        let mut rest = code;
        while let Some(pos) = rest.find('\\') {
            rest = &rest[pos + 1..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            let after = rest[name.len()..].trim_start_matches('*');
            if let Some(&(_, level)) = HEADINGS.iter().find(|(h, _)| *h == name) {
                if let Some(title) = braced(skip_optional(after)) {
                    out.push(Heading {
                        level,
                        title: clean_title(&title),
                        file: file.clone(),
                        line: i as u32 + 1,
                    });
                }
            } else if matches!(name.as_str(), "input" | "include" | "subfile")
                && let Some(target) = braced(after)
            {
                let mut child = PathBuf::from(target.trim());
                if child.extension().is_none() {
                    child.set_extension("tex");
                }
                walk_outline(root, &child, out, seen, depth + 1);
            }
        }
    }
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'%' && (i == 0 || bytes[i - 1] != b'\\') {
            return &line[..i];
        }
    }
    line
}

fn skip_optional(s: &str) -> &str {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return rest[end + 1..].trim_start();
    }
    s
}

/// The contents of a `{...}` group at the start of `s`, braces balanced.
fn braced(s: &str) -> Option<String> {
    let s = s.trim_start().strip_prefix('{')?;
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[..i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Drops simple markup like `\emph{x}` → `x` for display.
fn clean_title(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    chars.next();
                }
            }
            '{' | '}' | '$' => {}
            '~' => out.push(' '),
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn finds_main_by_documentclass() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "thesis.tex",
            "\\documentclass{book}\n\\input{chapters/a}",
        );
        write(
            d.path(),
            "chapters/a.tex",
            "% \\documentclass{article}\nHello",
        );
        let p = Project::open(d.path()).unwrap();
        assert_eq!(p.main, PathBuf::from("thesis.tex"));
        assert_eq!(p.main_candidates.len(), 1);
        assert!(p.pdf_path().ends_with(".lemmata/build/thesis.pdf"));
    }

    #[test]
    fn prefers_main_tex_and_respects_config() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.tex", "\\documentclass{article}");
        write(d.path(), "main.tex", "\\documentclass{article}");
        assert_eq!(
            Project::open(d.path()).unwrap().main,
            PathBuf::from("main.tex")
        );

        write(
            d.path(),
            CONFIG_FILE,
            "main = \"a.tex\"\nengine = \"xelatex\"\n",
        );
        let p = Project::open(d.path()).unwrap();
        assert_eq!(p.main, PathBuf::from("a.tex"));
        assert_eq!(p.config.engine, Engine::Xelatex);
        assert!(!p.config.shell_escape);
    }

    #[test]
    fn tree_skips_hidden_and_build() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(d.path(), ".lemmata/build/main.pdf", "");
        write(d.path(), ".git/HEAD", "");
        write(d.path(), "figures/a.png", "");
        let t = tree(d.path(), ".lemmata/build");
        let names: Vec<_> = t.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["figures", "main.tex"]);
        assert_eq!(t[0].children[0].path, "figures/a.png");
    }

    #[test]
    fn empty_folder_has_no_main() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(Project::open(d.path()), Err(Error::NoMainFile(_))));
    }

    #[test]
    fn outline_follows_inputs_in_order() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{report}\n\\begin{document}\n\\chapter{Intro}\n% \\section{Hidden}\n\\input{chapters/method}\n\\section*{Coda with \\emph{style}}\n\\end{document}\n",
        );
        write(
            d.path(),
            "chapters/method.tex",
            "\\section[Short]{Method $x$}\\label{sec:m}\n\\subsection{Setup}\n\\input{chapters/method}\n",
        );
        let p = Project::open(d.path()).unwrap();
        let o = outline(&p);
        let got: Vec<_> = o
            .iter()
            .map(|h| (h.level, h.title.as_str(), h.file.as_str(), h.line))
            .collect();
        assert_eq!(
            got,
            [
                (1, "Intro", "main.tex", 3),
                (2, "Method x", "chapters/method.tex", 1),
                (3, "Setup", "chapters/method.tex", 2),
                (2, "Coda with style", "main.tex", 6),
            ]
        );
    }
}
