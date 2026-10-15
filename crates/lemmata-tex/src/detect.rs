//! Finds a TeX distribution. Apps launched from a desktop or the macOS Dock
//! don't inherit the shell's PATH, so besides PATH we probe the folders the
//! usual installers write to.

use crate::os::Family;
use lemmata_pkgmap::Installer;
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const REQUIRED: &[&str] = &["latexmk", "pdflatex"];
/// Without synctex, building works but PDF ↔ source jumps don't.
pub const RECOMMENDED: &[&str] = &["synctex", "biber", "bibtex", "xelatex", "lualatex"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum Distribution {
    TexLive { year: Option<u16> },
    Miktex { version: Option<String> },
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub path: Option<PathBuf>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    /// Folder holding the TeX binaries, when one was found.
    pub bin_dir: Option<PathBuf>,
    pub distribution: Distribution,
    pub tools: Vec<Tool>,
    /// True when the app can build: every required tool was found, or Tectonic is available.
    pub ready: bool,
    /// True when every required TeX tool was found (`ready` also holds for Tectonic alone).
    pub tex_ready: bool,
    /// The Tectonic binary: the one Lemmata downloaded, else one on PATH.
    pub tectonic: Option<PathBuf>,
    /// TeX Live older than this gets a warning.
    pub outdated: bool,
}

impl Detection {
    pub fn tool(&self, name: &str) -> Option<&Path> {
        self.tools.iter().find(|t| t.name == name)?.path.as_deref()
    }

    /// How missing packages get installed with this TeX: the distro's
    /// package manager for TeX Live in /usr/bin, otherwise tlmgr or MiKTeX.
    pub fn installer(&self, family: Family) -> Installer<'static> {
        if matches!(self.distribution, Distribution::Miktex { .. }) {
            return Installer::Miktex;
        }
        let bin = self.bin_dir.as_deref().unwrap_or(Path::new(""));
        let distro_key = match family {
            Family::Arch | Family::Debian | Family::Fedora | Family::Suse => Some(family.key()),
            _ => None,
        };
        match distro_key {
            Some(key) if bin.starts_with("/usr/bin") => Installer::Distro(key),
            // A per-user TeX Live (e.g. ~/texlive) is writable without sudo.
            _ => Installer::Tlmgr {
                sudo: !cfg!(windows)
                    && !std::env::var_os("HOME").is_some_and(|h| bin.starts_with(h)),
            },
        }
    }
}

pub const MIN_TEXLIVE_YEAR: u16 = 2022;

/// `extra` folders (chosen by the user) are tried first.
pub fn detect(extra: &[PathBuf]) -> Detection {
    let mut best: Option<(usize, Vec<Tool>, Option<PathBuf>)> = None;

    for source in sources(extra) {
        let tools = probe(&source);
        let found_required = tools
            .iter()
            .filter(|t| t.required && t.path.is_some())
            .count();
        let bin_dir = tools
            .iter()
            .find(|t| t.name == "pdflatex")
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.parent().map(Path::to_path_buf));
        if found_required == REQUIRED.len() {
            best = Some((found_required, tools, bin_dir));
            break;
        }
        if best.as_ref().is_none_or(|(n, _, _)| found_required > *n) {
            best = Some((found_required, tools, bin_dir));
        }
    }

    let (found, tools, bin_dir) = best.unwrap_or_else(|| (0, probe(&Source::Path), None));
    let distribution = tools
        .iter()
        .find(|t| t.name == "pdflatex")
        .and_then(|t| t.path.as_deref())
        .map(identify)
        .unwrap_or(Distribution::Unknown);
    let outdated =
        matches!(distribution, Distribution::TexLive { year: Some(y) } if y < MIN_TEXLIVE_YEAR);

    // latexmk is a Perl script and MiKTeX does not ship Perl.
    let mut tools = tools;
    if cfg!(windows) && matches!(distribution, Distribution::Miktex { .. }) {
        tools.push(Tool {
            name: "perl".into(),
            path: find_perl(),
            required: false,
        });
    }

    let tectonic = crate::tectonic::find();
    let tex_ready = found == REQUIRED.len();
    Detection {
        bin_dir,
        distribution,
        ready: tex_ready || tectonic.is_some(),
        tex_ready,
        tectonic,
        tools,
        outdated,
    }
}

/// Perl on PATH, or where Strawberry Perl and Git for Windows put it (a
/// fresh install isn't on the PATH of an app that was already running).
pub fn find_perl() -> Option<PathBuf> {
    if let Ok(p) = which::which("perl") {
        return Some(p);
    }
    let mut dirs = vec![PathBuf::from(r"C:\Strawberry\perl\bin")];
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(pf) = std::env::var_os(var) {
            dirs.push(PathBuf::from(pf).join(r"Git\usr\bin"));
        }
    }
    dirs.into_iter()
        .map(|d| d.join(format!("perl{}", std::env::consts::EXE_SUFFIX)))
        .find(|p| p.is_file())
}

enum Source {
    Path,
    Dir(PathBuf),
}

fn sources(extra: &[PathBuf]) -> Vec<Source> {
    let mut out: Vec<Source> = extra.iter().cloned().map(Source::Dir).collect();
    out.push(Source::Path);
    out.extend(known_dirs().into_iter().map(Source::Dir));
    out
}

fn known_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/Library/TeX/texbin"));
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/opt/local/bin")); // MacPorts
    }
    if cfg!(unix) {
        // Vanilla TeX Live: /usr/local/texlive/<year>/bin/<platform>, newest year first.
        dirs.extend(year_dirs(Path::new("/usr/local/texlive"), |y| {
            y.join("bin")
        }));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            dirs.extend(year_dirs(&home.join("texlive"), |y| y.join("bin")));
            // TinyTeX lives under ~/Library on macOS and ~/.TinyTeX elsewhere.
            let tiny = if cfg!(target_os = "macos") {
                "Library/TinyTeX"
            } else {
                ".TinyTeX"
            };
            dirs.extend(subdirs(&home.join(tiny).join("bin")));
        }
        dirs.push(PathBuf::from("/usr/bin"));
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Programs\MiKTeX\miktex\bin\x64"));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join(r"TinyTeX\bin\windows"));
        }
        dirs.push(PathBuf::from(r"C:\Program Files\MiKTeX\miktex\bin\x64"));
        dirs.extend(year_dirs(Path::new(r"C:\texlive"), |y| y.join("bin")));
    }
    dirs
}

/// For each `<root>/<year>`, newest first, every platform folder inside `bin(year)`.
fn year_dirs(root: &Path, bin: impl Fn(&Path) -> PathBuf) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut years: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() == 4 && n.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    years.sort();
    years.reverse();
    years.iter().flat_map(|y| subdirs(&bin(y))).collect()
}

/// The folders directly inside `dir`, or none when it doesn't exist.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect()
}

fn probe(source: &Source) -> Vec<Tool> {
    probe_with(|name| match source {
        Source::Path => which::which(name).ok(),
        Source::Dir(dir) => in_dir(dir, name),
    })
}

fn in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    Some(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))).filter(|p| p.is_file())
}

/// `find` locates one tool by name (a PATH search can take long on Windows).
/// Once pdflatex is found, the other tools are looked for in its folder first,
/// where they almost always are, and `find` only runs for the ones missing there.
fn probe_with(find: impl Fn(&str) -> Option<PathBuf>) -> Vec<Tool> {
    let pdflatex = find("pdflatex");
    let sibling_dir = pdflatex.as_deref().and_then(Path::parent);
    REQUIRED
        .iter()
        .map(|n| (n, true))
        .chain(RECOMMENDED.iter().map(|n| (n, false)))
        .map(|(n, required)| Tool {
            name: n.to_string(),
            path: if *n == "pdflatex" {
                pdflatex.clone()
            } else {
                sibling_dir.and_then(|d| in_dir(d, n)).or_else(|| find(n))
            },
            required,
        })
        .collect()
}

/// Names the distribution from where it is installed, without starting a
/// program. Starting `pdflatex` can take seconds on a Windows setup that checks
/// every launch, and the answer is in the path for the usual installs.
pub fn identify_by_path(pdflatex: &Path) -> Option<Distribution> {
    let path = pdflatex.to_string_lossy().to_lowercase().replace('\\', "/");
    if path.contains("/miktex/") {
        return Some(Distribution::Miktex { version: None });
    }
    // .../texlive/2025/bin/<platform>/pdflatex
    let after = &path[path.find("/texlive/")? + "/texlive/".len()..];
    let year = after.split('/').next()?;
    (year.len() == 4 && year.chars().all(|c| c.is_ascii_digit())).then(|| Distribution::TexLive {
        year: year.parse().ok(),
    })
}

fn identify(pdflatex: &Path) -> Distribution {
    if let Some(known) = identify_by_path(pdflatex) {
        return known;
    }
    // Ask the program itself, but not for long: a slow start must not hold up the app.
    let exe = pdflatex.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut cmd = Command::new(exe);
        lemmata_env::host(&mut cmd).arg("--version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let _ = tx.send(cmd.output());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(4)) {
        Ok(Ok(out)) => parse_version(&String::from_utf8_lossy(&out.stdout)),
        _ => Distribution::Unknown,
    }
}

pub fn parse_version(text: &str) -> Distribution {
    let first = text.lines().next().unwrap_or("");
    if first.contains("MiKTeX") {
        let version = Regex::new(r"\(MiKTeX ([\d.]+)\)")
            .unwrap()
            .captures(first)
            .map(|c| c[1].to_string());
        return Distribution::Miktex { version };
    }
    if let Some(c) = Regex::new(r"TeX Live (\d{4})").unwrap().captures(first) {
        return Distribution::TexLive {
            year: c[1].parse().ok(),
        };
    }
    Distribution::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_the_usual_installs_from_their_path() {
        let id = |p: &str| identify_by_path(Path::new(p));
        assert_eq!(
            id(r"C:\Users\Jan\AppData\Local\Programs\MiKTeX\miktex\bin\x64\pdflatex.exe"),
            Some(Distribution::Miktex { version: None })
        );
        assert_eq!(
            id(r"C:\texlive\2025\bin\windows\pdflatex.exe"),
            Some(Distribution::TexLive { year: Some(2025) })
        );
        assert_eq!(
            id("/usr/local/texlive/2024/bin/x86_64-linux/pdflatex"),
            Some(Distribution::TexLive { year: Some(2024) })
        );
        assert_eq!(id("/usr/bin/pdflatex"), None);
        assert_eq!(id("/opt/texlive/current/bin/pdflatex"), None);
    }

    #[test]
    fn identifies_distributions() {
        assert_eq!(
            parse_version(
                "pdfTeX 3.141592653-2.6-1.40.27 (TeX Live 2025/Arch Linux)\nkpathsea version 6.4.1"
            ),
            Distribution::TexLive { year: Some(2025) }
        );
        assert_eq!(
            parse_version("MiKTeX-pdfTeX 4.20 (MiKTeX 24.4)"),
            Distribution::Miktex {
                version: Some("24.4".into())
            }
        );
        assert_eq!(parse_version("something else"), Distribution::Unknown);
    }

    #[test]
    fn lists_only_platform_folders() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("x86_64-linux")).unwrap();
        std::fs::create_dir(d.path().join("universal-darwin")).unwrap();
        std::fs::write(d.path().join("readme.txt"), "").unwrap();
        let mut got = subdirs(d.path());
        got.sort();
        assert_eq!(
            got,
            [
                d.path().join("universal-darwin"),
                d.path().join("x86_64-linux")
            ]
        );
    }

    #[test]
    fn user_folder_wins() {
        let d = tempfile::tempdir().unwrap();
        for n in REQUIRED {
            let p = d
                .path()
                .join(format!("{n}{}", std::env::consts::EXE_SUFFIX));
            std::fs::write(&p, "").unwrap();
        }
        let det = detect(&[d.path().to_path_buf()]);
        assert!(det.ready);
        assert_eq!(det.bin_dir.as_deref(), Some(d.path()));
    }

    #[test]
    fn sibling_tools_are_found_without_a_path_search() {
        let d = tempfile::tempdir().unwrap();
        let exe = |n: &str| {
            d.path()
                .join(format!("{n}{}", std::env::consts::EXE_SUFFIX))
        };
        for n in ["pdflatex", "latexmk", "synctex", "xelatex"] {
            std::fs::write(exe(n), "").unwrap();
        }
        let searched = std::cell::RefCell::new(Vec::new());
        let tools = probe_with(|n| {
            searched.borrow_mut().push(n.to_string());
            (n == "pdflatex").then(|| exe("pdflatex"))
        });
        let path = |n: &str| tools.iter().find(|t| t.name == n).unwrap().path.clone();
        assert_eq!(path("latexmk"), Some(exe("latexmk")));
        assert_eq!(path("xelatex"), Some(exe("xelatex")));
        assert_eq!(path("biber"), None);
        // Only pdflatex, then the tools missing from its folder, were searched for.
        assert_eq!(
            *searched.borrow(),
            ["pdflatex", "biber", "bibtex", "lualatex"]
        );
    }
}
