//! Answers "File `x.sty' not found" with the command that installs it on
//! this system.
//!
//! The file → TeX Live package → collection table is generated from
//! TeX Live's package database by `scripts/gen-pkgmap.py`. How distros split
//! TeX Live into their own packages lives in `data/distro-packages.toml`.

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::OnceLock;

const PKGMAP_GZ: &[u8] = include_bytes!("../../../data/pkgmap.tsv.gz");
const DISTROS_TOML: &str = include_str!("../../../data/distro-packages.toml");

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provider {
    /// TeX Live package, e.g. `pgf` for `tikz.sty`.
    pub package: String,
    /// TeX Live collection, e.g. `collection-pictures`. Empty if unknown.
    pub collection: String,
}

/// How TeX is installed, which decides the install command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer<'a> {
    /// TeX Live from the distro's package manager; the key is a section of
    /// `data/distro-packages.toml` (arch, debian, fedora, suse).
    Distro(&'a str),
    /// Upstream TeX Live; `sudo` when it lives in a system folder.
    Tlmgr {
        sudo: bool,
    },
    Miktex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fix {
    pub provider: Provider,
    /// Shell command to run, when there is one.
    pub command: Option<String>,
    /// One sentence for the user.
    pub explanation: String,
}

fn table() -> &'static HashMap<String, Provider> {
    static TABLE: OnceLock<HashMap<String, Provider>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut text = String::new();
        GzDecoder::new(PKGMAP_GZ)
            .read_to_string(&mut text)
            .expect("pkgmap.tsv.gz is valid gzip");
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut parts = l.split('\t');
                let (file, package, collection) =
                    (parts.next()?, parts.next()?, parts.next().unwrap_or(""));
                Some((
                    file.to_string(),
                    Provider {
                        package: package.into(),
                        collection: collection.into(),
                    },
                ))
            })
            .collect()
    })
}

#[derive(Deserialize)]
struct Distro {
    command: String,
    by: String,
    prefix: String,
    #[serde(default)]
    lang_dash: bool,
    #[serde(default)]
    rename: BTreeMap<String, String>,
}

fn distros() -> &'static BTreeMap<String, Distro> {
    static DISTROS: OnceLock<BTreeMap<String, Distro>> = OnceLock::new();
    DISTROS
        .get_or_init(|| toml::from_str(DISTROS_TOML).expect("data/distro-packages.toml is valid"))
}

/// The TeX Live package that provides `file` (e.g. `pgfplots.sty`).
pub fn lookup(file: &str) -> Option<&'static Provider> {
    table().get(file)
}

fn distro_package(d: &Distro, p: &Provider) -> Option<String> {
    if d.by == "package" {
        return Some(format!("{}{}", d.prefix, p.package));
    }
    let short = p.collection.strip_prefix("collection-")?;
    if let Some(name) = d.rename.get(short) {
        return Some(name.clone());
    }
    if d.lang_dash
        && let Some(lang) = short.strip_prefix("lang")
    {
        return Some(format!("{}lang-{lang}", d.prefix));
    }
    Some(format!("{}{short}", d.prefix))
}

/// How to get `file` installed with this installer.
pub fn fix_for(file: &str, installer: Installer) -> Option<Fix> {
    let provider = lookup(file)?.clone();
    let (command, explanation) = match installer {
        Installer::Distro(key) => match distros()
            .get(key)
            .and_then(|d| Some((d, distro_package(d, &provider)?)))
        {
            Some((d, pkg)) => (
                Some(d.command.replace("{packages}", &pkg)),
                format!(
                    "{file} is part of the TeX Live package \"{}\", which your system ships in {pkg}.",
                    provider.package
                ),
            ),
            None => (
                None,
                format!(
                    "{file} is part of the TeX Live package \"{}\".",
                    provider.package
                ),
            ),
        },
        Installer::Tlmgr { sudo } => (
            Some(format!(
                "{}tlmgr install {}",
                if sudo { "sudo " } else { "" },
                provider.package
            )),
            format!(
                "{file} is part of the TeX Live package \"{}\".",
                provider.package
            ),
        ),
        Installer::Miktex => (
            Some(format!("miktex packages install {}", provider.package)),
            format!(
                "{file} is part of \"{}\". MiKTeX installs it automatically if \"install missing packages on-the-fly\" is on.",
                provider.package
            ),
        ),
    };
    Some(Fix {
        provider,
        command,
        explanation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_providers() {
        assert_eq!(lookup("tikz.sty").unwrap().package, "pgf");
        assert_eq!(
            lookup("pgfplots.sty").unwrap().collection,
            "collection-pictures"
        );
        assert_eq!(lookup("IEEEtran.cls").unwrap().package, "ieeetran");
        assert!(lookup("lemmata-does-not-exist.sty").is_none());
    }

    #[test]
    fn distro_commands() {
        let arch = fix_for("pgfplots.sty", Installer::Distro("arch")).unwrap();
        assert_eq!(
            arch.command.as_deref(),
            Some("sudo pacman -S --needed texlive-pictures")
        );

        let deb = fix_for("siunitx.sty", Installer::Distro("debian")).unwrap();
        assert_eq!(
            deb.command.as_deref(),
            Some("sudo apt install texlive-science")
        );

        let fedora = fix_for("tikz.sty", Installer::Distro("fedora")).unwrap();
        assert_eq!(
            fedora.command.as_deref(),
            Some("sudo dnf install texlive-pgf")
        );
    }

    #[test]
    fn debian_language_collections() {
        let babel_german = table()
            .iter()
            .find(|(_, p)| p.collection == "collection-langgerman")
            .map(|(f, _)| f.clone())
            .unwrap();
        let fix = fix_for(&babel_german, Installer::Distro("debian")).unwrap();
        assert_eq!(
            fix.command.as_deref(),
            Some("sudo apt install texlive-lang-german")
        );
    }

    #[test]
    fn tlmgr_and_miktex() {
        let f = fix_for("minted.sty", Installer::Tlmgr { sudo: true }).unwrap();
        assert_eq!(f.command.as_deref(), Some("sudo tlmgr install minted"));
        let m = fix_for("minted.sty", Installer::Miktex).unwrap();
        assert_eq!(m.command.as_deref(), Some("miktex packages install minted"));
    }
}
