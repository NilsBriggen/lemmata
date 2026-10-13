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
    /// True when every required tool was found.
    pub ready: bool,
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

    Detection {
        bin_dir,
        distribution,
        ready: found == REQUIRED.len(),
        tools,
        outdated,
    }
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
    }
    if cfg!(unix) {
        // Vanilla TeX Live: /usr/local/texlive/<year>/bin/<platform>, newest year first.
        dirs.extend(year_dirs(Path::new("/usr/local/texlive"), |y| {
            y.join("bin")
        }));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.extend(year_dirs(&PathBuf::from(home).join("texlive"), |y| {
                y.join("bin")
            }));
        }
        dirs.push(PathBuf::from("/usr/bin"));
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Programs\MiKTeX\miktex\bin\x64"));
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
    years
        .iter()
        .flat_map(|y| {
            std::fs::read_dir(bin(y))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
        })
        .filter(|p| p.is_dir())
        .collect()
}

fn probe(source: &Source) -> Vec<Tool> {
    let find = |name: &str| -> Option<PathBuf> {
        match source {
            Source::Path => which::which(name).ok(),
            Source::Dir(dir) => {
                let exe = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
                exe.is_file().then_some(exe)
            }
        }
    };
    REQUIRED
        .iter()
        .map(|n| (n, true))
        .chain(RECOMMENDED.iter().map(|n| (n, false)))
        .map(|(n, required)| Tool {
            name: n.to_string(),
            path: find(n),
            required,
        })
        .collect()
}

fn identify(pdflatex: &Path) -> Distribution {
    let Ok(out) = Command::new(pdflatex).arg("--version").output() else {
        return Distribution::Unknown;
    };
    parse_version(&String::from_utf8_lossy(&out.stdout))
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
}
