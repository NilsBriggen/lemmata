//! A Lemmata project is a plain folder. `lemmata.toml` is optional; without
//! it the main file is the `.tex` file containing `\documentclass`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub mod export;
pub mod extras;
pub mod prose;
pub mod symbols;

pub const CONFIG_FILE: &str = "lemmata.toml";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not a folder")]
    NotADirectory(PathBuf),
    // The app recognises this message (App.svelte openProject) to offer creating a main.tex.
    #[error("no .tex file with \\documentclass found in {0}")]
    NoMainFile(PathBuf),
    #[error("The main file {} set in lemmata.toml doesn't exist.", .0.display())]
    MainMissing(PathBuf),
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("lemmata.toml is invalid: {0}")]
    Config(#[from] toml::de::Error),
    #[error("could not write lemmata.toml: {0}")]
    ConfigWrite(#[from] toml::ser::Error),
    #[error("lemmata.toml is invalid: {0}")]
    ConfigEdit(#[from] toml_edit::TomlError),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Pdflatex,
    Xelatex,
    Lualatex,
    /// Tectonic: a self-contained XeTeX-based engine that fetches packages as needed.
    Tectonic,
}

impl Engine {
    /// The latexmk flag selecting this engine.
    pub fn latexmk_flag(self) -> &'static str {
        match self {
            Engine::Pdflatex => "-pdf",
            Engine::Xelatex => "-xelatex",
            Engine::Lualatex => "-lualatex",
            // Never run through latexmk; XeTeX is what Tectonic is built on.
            Engine::Tectonic => "-xelatex",
        }
    }

    pub fn binary(self) -> &'static str {
        match self {
            Engine::Pdflatex => "pdflatex",
            Engine::Xelatex => "xelatex",
            Engine::Lualatex => "lualatex",
            Engine::Tectonic => "tectonic",
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

/// A named way to build the project: another main file, and optionally
/// another compiler (a paper and its slides, say). Picking one in the
/// toolbar sets `main` and `engine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub main: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
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
    /// Spell-check language ("en", "en-GB", "de", "de-CH", "fr" or "off").
    /// None means: guess from babel or polyglossia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spellcheck: Option<String>,
    /// Words the spell checker should accept in this project.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<String>,
    /// Grammar rules (LanguageTool rule ids) the author chose to ignore in this project.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub grammar_ignore: Vec<String>,
    /// Where dropped and pasted images go.
    pub figures_dir: String,
    /// Fast mode: images are drawn as empty boxes so big figures don't slow
    /// every build. The final PDF should be built without it.
    #[serde(skip_serializing_if = "is_false")]
    pub draft: bool,
    /// Stop at the first error instead of pushing on to a complete PDF.
    #[serde(skip_serializing_if = "is_false")]
    pub stop_on_error: bool,
    /// A build running longer than this is stopped. 0 means no limit.
    #[serde(skip_serializing_if = "is_default_timeout")]
    pub timeout_secs: u64,
    /// Named build presets; empty for a project with one main file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
    /// Precompile the preamble (pdflatex only) so rebuilds after an edit start faster.
    /// Each precompiled preamble is checked once against a normal build before it is used.
    #[serde(skip_serializing_if = "is_true")]
    pub fast_preamble: bool,
}

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_OUTDIR: &str = ".lemmata/build";

/// Is `outdir` a folder strictly inside the project? A project can come from
/// anywhere, and the build folder gets cleaned, so "." or "../x" or an
/// absolute path must never be honoured.
pub fn valid_outdir(outdir: &str) -> bool {
    let p = Path::new(outdir);
    !outdir.trim().is_empty()
        && p.is_relative()
        && !p.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
        && p.components()
            .any(|c| matches!(c, std::path::Component::Normal(_)))
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_default_timeout(t: &u64) -> bool {
    *t == DEFAULT_TIMEOUT_SECS
}

impl Default for Config {
    fn default() -> Self {
        Config {
            main: None,
            engine: Engine::default(),
            bib: Bib::default(),
            shell_escape: false,
            outdir: DEFAULT_OUTDIR.into(),
            spellcheck: None,
            words: Vec::new(),
            grammar_ignore: Vec::new(),
            figures_dir: "figures".into(),
            draft: false,
            stop_on_error: false,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            targets: Vec::new(),
            fast_preamble: true,
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
        let root = canonical(root).map_err(|source| Error::Io {
            path: root.into(),
            source,
        })?;
        let mut config = read_config(&root)?;
        // An unsafe build folder (".", "..", absolute) falls back to the default.
        if !valid_outdir(&config.outdir) {
            config.outdir = DEFAULT_OUTDIR.into();
        }
        let main_candidates = find_main_candidates(&root, &config.outdir);
        let main = match &config.main {
            Some(m) => {
                let m = PathBuf::from(m);
                if !main_inside(&root, &m) {
                    return Err(Error::MainMissing(m));
                }
                m
            }
            None => {
                let picked =
                    pick_main(&main_candidates).ok_or_else(|| Error::NoMainFile(root.clone()))?;
                // "% !TeX root = ../thesis.tex" in the picked file names the real main file.
                magic(&root.join(&picked), "root")
                    .map(|r| {
                        let dir = picked.parent().unwrap_or(Path::new(""));
                        normalize(&dir.join(r))
                    })
                    .filter(|r| main_inside(&root, r))
                    .unwrap_or(picked)
            }
        };
        // "% !TeX program = xelatex" sets the engine when lemmata.toml doesn't.
        if !sets_key(&root, "engine")
            && let Some(engine) = magic(&root.join(&main), "program").and_then(|p| engine_named(&p))
        {
            config.engine = engine;
        }
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

    /// Writes the settings into lemmata.toml, changing only the keys that
    /// differ so the user's comments, key order and unknown keys survive.
    pub fn save_config(&self) -> Result<()> {
        let path = self.root.join(CONFIG_FILE);
        let old_text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(source) => return Err(Error::Io { path, source }),
        };
        let old: toml::Table = toml::from_str(&old_text)?;
        let new = toml::Table::try_from(&self.config)?;
        let mut doc = old_text.parse::<toml_edit::DocumentMut>()?;
        let new_doc = toml::to_string_pretty(&new)?.parse::<toml_edit::DocumentMut>()?;
        for (key, item) in new_doc.iter() {
            if old.get(key) == new.get(key) {
                continue;
            }
            let mut item = item.clone();
            // Keep a trailing comment like `engine = "xelatex" # for the font`.
            if let (Some(was), Some(now)) =
                (doc.get(key).and_then(|i| i.as_value()), item.as_value_mut())
            {
                *now.decor_mut() = was.decor().clone();
            }
            doc.insert(key, item);
        }
        // Optional settings that are now unset or default are not serialised.
        for key in CONFIG_KEYS {
            if old.contains_key(*key) && !new.contains_key(*key) {
                doc.remove(key);
            }
        }
        write_atomic(&path, &doc.to_string()).map_err(|source| Error::Io { path, source })
    }
}

/// Every key `Config` can write; anything else in the file is the user's.
const CONFIG_KEYS: &[&str] = &[
    "main",
    "engine",
    "bib",
    "shell_escape",
    "outdir",
    "spellcheck",
    "words",
    "grammar_ignore",
    "figures_dir",
    "draft",
    "stop_on_error",
    "timeout_secs",
    "targets",
    "fast_preamble",
];

/// A crash mid-write must not leave a half-written file, so write beside it
/// and rename over.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text)
        .and_then(|()| fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
}

/// Is `main` (relative to `root`) an existing file inside the project?
fn main_inside(root: &Path, main: &Path) -> bool {
    main.is_relative()
        && !main
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        && canonical(&root.join(main)).is_ok_and(|p| p.starts_with(root) && p.is_file())
}

/// `canonicalize`, minus the `\\?\` prefix Windows adds, which breaks path
/// comparisons and looks odd in the UI.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    let p = path.canonicalize()?;
    #[cfg(windows)]
    let p = PathBuf::from(strip_verbatim(&p.to_string_lossy()));
    Ok(p)
}

/// `\\?\C:\x` becomes `C:\x` and `\\?\UNC\srv\share` becomes `\\srv\share`.
/// Other verbatim paths (volume GUIDs) can't be written any shorter.
#[cfg_attr(not(windows), allow(dead_code))]
fn strip_verbatim(path: &str) -> String {
    let Some(rest) = path.strip_prefix(r"\\?\") else {
        return path.to_string();
    };
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        format!(r"\\{unc}")
    } else if rest.as_bytes().get(1) == Some(&b':') {
        rest.to_string()
    } else {
        path.to_string()
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
/// A `% !TeX <key> = value` comment (also `TS-program`) near the top of a file, as TeXShop, TeXstudio and VS Code read them.
fn magic(file: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    text.lines().take(40).find_map(|line| {
        let rest = line
            .trim_start()
            .strip_prefix('%')?
            .trim_start()
            .strip_prefix('!')?
            .trim_start();
        let rest = rest
            .get(..3)
            .filter(|t| t.eq_ignore_ascii_case("tex"))
            .map(|_| &rest[3..])?;
        let (name, value) = rest.trim_start().split_once('=')?;
        let name = name.trim().to_ascii_lowercase();
        (name == key || name == format!("ts-{key}"))
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn engine_named(program: &str) -> Option<Engine> {
    match program.to_ascii_lowercase().as_str() {
        "pdflatex" | "pdftex" | "latex" => Some(Engine::Pdflatex),
        "xelatex" | "xetex" => Some(Engine::Xelatex),
        "lualatex" | "luatex" | "lualatex-dev" => Some(Engine::Lualatex),
        "tectonic" => Some(Engine::Tectonic),
        _ => None,
    }
}

/// Whether lemmata.toml sets `key` itself (rather than leaving the default).
fn sets_key(root: &Path, key: &str) -> bool {
    fs::read_to_string(root.join(CONFIG_FILE))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key(key))
}

/// `chapters/../main.tex` → `main.tex`, without touching the disk. A path that climbs above the root keeps its `..`, so main_inside rejects it.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

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
    /// Bytes; 0 for folders.
    pub size: u64,
    /// Last change, seconds since the Unix epoch; 0 when unknown.
    pub modified: u64,
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
                let meta = e.metadata().ok();
                // file_type() doesn't follow links. A linked folder is shown as
                // a folder but never entered: a link to "." or to ~ would loop
                // or walk the whole disk on every build.
                let ft = e.file_type().ok();
                let real_dir = ft.is_some_and(|t| t.is_dir());
                let is_dir = real_dir || ft.is_some_and(|t| t.is_symlink()) && path.is_dir();
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: rel,
                    is_dir,
                    size: meta.as_ref().filter(|_| !is_dir).map_or(0, |m| m.len()),
                    modified: meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs()),
                    children: if real_dir {
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
    /// 0 = \part, 1 = \chapter, 2 = \section, 3 = \subsection, 4 = \subsubsection,
    /// 5 = \paragraph. Beamer frames count as 3.
    pub level: u8,
    pub title: String,
    /// Relative to the project root, with forward slashes.
    pub file: String,
    /// 1-based.
    pub line: u32,
    /// Prose words from this heading up to the next one (of any level).
    pub words: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Outline {
    pub headings: Vec<Heading>,
    /// Prose words in the whole document body.
    pub total_words: u32,
}

const HEADINGS: &[(&str, u8)] = &[
    ("part", 0),
    ("chapter", 1),
    ("section", 2),
    ("subsection", 3),
    ("subsubsection", 4),
    ("paragraph", 5),
    // Beamer slides sit at subsection depth.
    ("frametitle", 3),
];

/// The document's headings in reading order with word counts, following
/// `\input`, `\include` and `\subfile` from the main file.
pub fn outline(project: &Project) -> Outline {
    let mut walk = Walk::default();
    walk.file(&project.root, &project.main, 0, true);
    walk.out
}

#[derive(Default)]
struct Walk {
    out: Outline,
    seen: std::collections::HashSet<PathBuf>,
    prose: prose::State,
}

impl Walk {
    fn count(&mut self, line: &str) {
        let n = prose::words(line, &mut self.prose).len() as u32;
        self.out.total_words += n;
        if let Some(h) = self.out.headings.last_mut() {
            h.words += n;
        }
    }

    /// In the main file, words count only after `\begin{document}`.
    fn file(&mut self, root: &Path, rel: &Path, depth: usize, is_main: bool) {
        if depth > 16 || !self.seen.insert(rel.to_path_buf()) {
            return;
        }
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            return;
        };
        let file = rel.to_string_lossy().replace('\\', "/");
        let mut in_body = !is_main || !text.contains("\\begin{document}");
        for (i, raw) in text.lines().enumerate() {
            if !in_body {
                in_body = strip_comment(raw).contains("\\begin{document}");
                continue;
            }
            self.line(root, &file, i, raw, depth);
        }
    }

    fn line(&mut self, root: &Path, file: &str, i: usize, raw: &str, depth: usize) {
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
                    self.out.headings.push(Heading {
                        level,
                        title: clean_title(&title),
                        file: file.to_string(),
                        line: i as u32 + 1,
                        words: 0,
                    });
                }
            } else if name == "begin"
                && let Some(env) = braced(after)
                && env == "frame"
                && let Some(title) = braced(skip_optional(&after[env.len() + 2..]))
            {
                self.out.headings.push(Heading {
                    level: 3,
                    title: clean_title(&title),
                    file: file.to_string(),
                    line: i as u32 + 1,
                    words: 0,
                });
            } else if let Some(target) = include_target(&name, after) {
                let mut child = PathBuf::from(target.trim());
                if child.extension().is_none() {
                    child.set_extension("tex");
                }
                // This line's own text counts before the included file's.
                self.count(raw);
                self.file(root, &child, depth + 1, false);
                return;
            }
        }
        self.count(raw);
    }
}

/// The file named by `\input`, `\include`, `\subfile` or `\import{dir}{file}`.
/// `\input file` without braces is accepted too.
fn include_target(name: &str, after: &str) -> Option<String> {
    match name {
        "input" | "include" | "subfile" => braced(after).or_else(|| {
            let bare: String = after
                .trim_start()
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '\\' && *c != '{')
                .collect();
            (!bare.is_empty() && after.starts_with(char::is_whitespace)).then_some(bare)
        }),
        "import" | "subimport" | "includefrom" | "inputfrom" => {
            let dir = braced(after)?;
            let rest = after.trim_start();
            let file = braced(&rest[dir.len() + 2..])?;
            Some(format!("{}/{}", dir.trim_end_matches('/'), file))
        }
        _ => None,
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
    #[test]
    fn magic_comments_pick_the_root_and_the_engine() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("chapters")).unwrap();
        std::fs::write(
            d.path().join("thesis.tex"),
            "% !TEX TS-program = xelatex\n\\documentclass{book}\n",
        )
        .unwrap();
        // The only file with \documentclass that would be picked first points elsewhere.
        std::fs::write(
            d.path().join("main.tex"),
            "%!TeX root = thesis.tex\n\\documentclass{article}\n",
        )
        .unwrap();
        let p = Project::open(d.path()).unwrap();
        assert_eq!(p.main, PathBuf::from("thesis.tex"));
        assert_eq!(p.config.engine, Engine::Xelatex);
        // An engine set in lemmata.toml wins over the comment.
        std::fs::write(d.path().join(CONFIG_FILE), "engine = \"pdflatex\"\n").unwrap();
        assert_eq!(
            Project::open(d.path()).unwrap().config.engine,
            Engine::Pdflatex
        );
        // A root outside the project is ignored.
        std::fs::remove_file(d.path().join(CONFIG_FILE)).unwrap();
        std::fs::write(
            d.path().join("main.tex"),
            "% !TeX root = ../elsewhere.tex\n\\documentclass{article}\n",
        )
        .unwrap();
        assert_eq!(
            Project::open(d.path()).unwrap().main,
            PathBuf::from("main.tex")
        );
        assert_eq!(
            magic(&d.path().join("thesis.tex"), "program").as_deref(),
            Some("xelatex")
        );
        assert_eq!(engine_named("LuaLaTeX"), Some(Engine::Lualatex));
        assert_eq!(engine_named("context"), None);
    }

    use super::*;

    #[test]
    fn outline_handles_paragraphs_frames_and_imports() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{Intro}\nHi\n\\end{frame}\n\\frametitle{Two}\n\\paragraph{Para}\n\\input sub/a\n\\import{sub/}{b}\n\\end{document}\n",
        );
        write(d.path(), "sub/a.tex", "\\section{A}\n");
        write(d.path(), "sub/b.tex", "\\section{B}\n");
        let p = Project::open(d.path()).unwrap();
        let got: Vec<_> = outline(&p)
            .headings
            .iter()
            .map(|h| (h.level, h.title.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (3, "Intro".into()),
                (3, "Two".into()),
                (5, "Para".into()),
                (2, "A".into()),
                (2, "B".into()),
            ]
        );
    }

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
    fn unsafe_build_folders_fall_back_to_the_default() {
        for bad in [".", "./", "..", "../out", "/tmp/x", "", "a/../.."] {
            assert!(!valid_outdir(bad), "{bad:?}");
        }
        for good in [".lemmata/build", "build", "out/pdf", "./build"] {
            assert!(valid_outdir(good), "{good:?}");
        }
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(d.path(), "lemmata.toml", "outdir = \".\"\n");
        assert_eq!(
            Project::open(d.path()).unwrap().config.outdir,
            DEFAULT_OUTDIR
        );
    }

    #[test]
    fn targets_survive_a_config_round_trip() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(d.path(), "slides.tex", "\\documentclass{beamer}");
        let mut p = Project::open(d.path()).unwrap();
        p.config.targets = vec![
            Target {
                name: "Paper".into(),
                main: "main.tex".into(),
                engine: None,
            },
            Target {
                name: "Slides".into(),
                main: "slides.tex".into(),
                engine: Some(Engine::Xelatex),
            },
        ];
        p.save_config().unwrap();
        let text = fs::read_to_string(d.path().join("lemmata.toml")).unwrap();
        assert!(text.contains("[[targets]]") && text.contains("xelatex"));
        let again = Project::open(d.path()).unwrap();
        assert_eq!(again.config.targets, p.config.targets);
        // Projects without targets don't gain an empty table.
        let q = tempfile::tempdir().unwrap();
        write(q.path(), "main.tex", "\\documentclass{article}");
        let plain = Project::open(q.path()).unwrap();
        plain.save_config().unwrap();
        assert!(
            !fs::read_to_string(q.path().join("lemmata.toml"))
                .unwrap()
                .contains("targets")
        );
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
    fn tree_reports_size_and_modified_time() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "12345");
        write(d.path(), "figures/a.png", "");
        let t = tree(d.path(), ".lemmata/build");
        let main = t.iter().find(|e| e.name == "main.tex").unwrap();
        assert_eq!(main.size, 5);
        assert!(main.modified > 1_600_000_000);
        assert_eq!(t[0].size, 0, "folders have no size");
    }

    #[test]
    fn explicit_main_must_be_a_file_inside_the_project() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(d.path(), "sub/ok.tex", "x");
        fs::create_dir(d.path().join("dir.tex")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "out.tex", "x");
        let abs = outside.path().join("out.tex");
        let abs = abs.to_string_lossy().replace('\\', "/");
        for bad in ["gone.tex", "dir.tex", "../out.tex", abs.as_str()] {
            write(d.path(), CONFIG_FILE, &format!("main = \"{bad}\"\n"));
            match Project::open(d.path()) {
                Err(Error::MainMissing(m)) => assert_eq!(m, PathBuf::from(bad)),
                other => panic!("{bad}: {other:?}"),
            }
        }
        let msg = Error::MainMissing("gone.tex".into()).to_string();
        assert_eq!(
            msg,
            "The main file gone.tex set in lemmata.toml doesn't exist."
        );
        write(d.path(), CONFIG_FILE, "main = \"sub/ok.tex\"\n");
        assert_eq!(
            Project::open(d.path()).unwrap().main,
            PathBuf::from("sub/ok.tex")
        );
    }

    #[test]
    fn strips_the_windows_verbatim_prefix() {
        assert_eq!(strip_verbatim(r"\\?\C:\Users\a"), r"C:\Users\a");
        assert_eq!(strip_verbatim(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
        assert_eq!(strip_verbatim(r"\\?\Volume{1}\x"), r"\\?\Volume{1}\x");
        assert_eq!(strip_verbatim("/home/a"), "/home/a");
        assert_eq!(strip_verbatim(r"C:\a"), r"C:\a");
    }

    #[cfg(unix)]
    #[test]
    fn tree_does_not_follow_symlinked_folders() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(d.path(), "real/a.tex", "");
        std::os::unix::fs::symlink(".", d.path().join("loop")).unwrap();
        std::os::unix::fs::symlink("real", d.path().join("alias")).unwrap();
        let t = tree(d.path(), ".lemmata/build");
        let names: Vec<_> = t.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alias", "loop", "real", "main.tex"]);
        for link in &t[..2] {
            assert!(link.is_dir && link.children.is_empty(), "{}", link.name);
        }
        assert_eq!(t[2].children.len(), 1);
    }

    #[test]
    fn save_config_keeps_comments_and_unknown_keys() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        write(
            d.path(),
            CONFIG_FILE,
            "# my project\nfuture_option = 42\nengine = \"xelatex\" # for the font\nwords = [\"foo\"]\n",
        );
        let mut p = Project::open(d.path()).unwrap();
        p.config.bib = Bib::Bibtex;
        p.config.words.clear();
        p.save_config().unwrap();
        let text = fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(
            text.starts_with("# my project\nfuture_option = 42\n"),
            "{text}"
        );
        assert!(
            text.contains("engine = \"xelatex\" # for the font"),
            "{text}"
        );
        assert!(
            text.contains("bib = \"bibtex\"") && !text.contains("words"),
            "{text}"
        );
        assert!(!d.path().join("lemmata.toml.tmp").exists());
        let again = Project::open(d.path()).unwrap();
        assert_eq!(
            (again.config.engine, again.config.bib),
            (Engine::Xelatex, Bib::Bibtex)
        );
        // Changing a value that has a trailing comment keeps the comment.
        p.config.engine = Engine::Lualatex;
        p.save_config().unwrap();
        let text = fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(
            text.contains("engine = \"lualatex\" # for the font"),
            "{text}"
        );
    }

    #[test]
    fn ignored_grammar_rules_and_words_round_trip_and_can_be_removed() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        let mut p = Project::open(d.path()).unwrap();
        p.config.words = vec!["lemmata".into(), "synctex".into()];
        p.config.grammar_ignore = vec!["EN_QUOTES".into()];
        p.save_config().unwrap();
        let mut again = Project::open(d.path()).unwrap();
        assert_eq!(again.config.grammar_ignore, ["EN_QUOTES"]);
        // Removing them in Project settings takes the keys out of the file.
        again.config.words.retain(|w| w != "lemmata");
        again.config.grammar_ignore.clear();
        again.save_config().unwrap();
        let text = fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(
            text.contains("synctex")
                && !text.contains("lemmata\"")
                && !text.contains("grammar_ignore"),
            "{text}"
        );
    }

    #[test]
    fn save_config_creates_the_file() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        Project::open(d.path()).unwrap().save_config().unwrap();
        assert!(
            fs::read_to_string(d.path().join(CONFIG_FILE))
                .unwrap()
                .contains("engine")
        );
    }

    #[test]
    fn fast_preamble_is_on_unless_turned_off() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "main.tex", "\\documentclass{article}");
        let mut p = Project::open(d.path()).unwrap();
        assert!(p.config.fast_preamble);
        p.config.fast_preamble = false;
        p.save_config().unwrap();
        let text = fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(text.contains("fast_preamble = false"), "{text}");
        assert!(!Project::open(d.path()).unwrap().config.fast_preamble);
        p.config.fast_preamble = true;
        p.save_config().unwrap();
        let text = fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(!text.contains("fast_preamble"), "{text}");
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
        let o = outline(&p).headings;
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

    #[test]
    fn counts_words_per_section_in_the_body() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "main.tex",
            "\\documentclass{article}\n\\title{Not counted}\n\\begin{document}\nTwo words.\n\\section{Intro}\nThree more words~\\cite{k}.\n\\input{b}\n\\end{document}\n",
        );
        write(d.path(), "b.tex", "\\subsection{Sub}\nFour words in b.\n");
        let o = outline(&Project::open(d.path()).unwrap());
        let counts: Vec<_> = o
            .headings
            .iter()
            .map(|h| (h.title.as_str(), h.words))
            .collect();
        // Heading titles count as words of their section.
        assert_eq!(counts, [("Intro", 4), ("Sub", 5)]);
        assert_eq!(o.total_words, 11);
    }
}
