//! Which OS / distro family we run on, and the matching TeX install
//! instructions from `data/install.toml`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Arch,
    Debian,
    Fedora,
    Suse,
    Macos,
    Windows,
    Other,
}

impl Family {
    pub fn key(self) -> &'static str {
        match self {
            Family::Arch => "arch",
            Family::Debian => "debian",
            Family::Fedora => "fedora",
            Family::Suse => "suse",
            Family::Macos => "macos",
            Family::Windows => "windows",
            Family::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OsInfo {
    pub family: Family,
    /// Human-readable name, e.g. "CachyOS".
    pub name: String,
}

pub fn detect() -> OsInfo {
    if cfg!(target_os = "macos") {
        return OsInfo {
            family: Family::Macos,
            name: "macOS".into(),
        };
    }
    if cfg!(windows) {
        return OsInfo {
            family: Family::Windows,
            name: "Windows".into(),
        };
    }
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    from_os_release(&text)
}

pub fn from_os_release(text: &str) -> OsInfo {
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
    };
    let ids = format!(
        "{} {}",
        get("ID").unwrap_or_default(),
        get("ID_LIKE").unwrap_or_default()
    );
    let has = |id: &str| ids.split_whitespace().any(|x| x == id);
    let family = if has("arch") {
        Family::Arch
    } else if has("debian") || has("ubuntu") {
        Family::Debian
    } else if has("fedora") || has("rhel") {
        Family::Fedora
    } else if ids
        .split_whitespace()
        .any(|x| x.starts_with("opensuse") || x == "suse")
    {
        Family::Suse
    } else {
        Family::Other
    };
    let name = get("NAME").unwrap_or_else(|| "Linux".into());
    OsInfo { family, name }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instructions {
    pub label: String,
    pub recommended: String,
    pub full: String,
    #[serde(default)]
    pub recommended_size: String,
    #[serde(default)]
    pub full_size: String,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Deserialize)]
struct InstallFile {
    platform: BTreeMap<String, Instructions>,
}

const INSTALL_TOML: &str = include_str!("../../../data/install.toml");

/// Instructions for every platform, keyed by [`Family::key`].
pub fn all_instructions() -> BTreeMap<String, Instructions> {
    toml::from_str::<InstallFile>(INSTALL_TOML)
        .expect("data/install.toml is valid")
        .platform
}

pub fn instructions_for(family: Family) -> Instructions {
    let mut all = all_instructions();
    all.remove(family.key())
        .or_else(|| all.remove("other"))
        .expect("an 'other' entry exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_from_os_release() {
        let cachy = "NAME=\"CachyOS Linux\"\nID=cachyos\nID_LIKE=arch\n";
        assert_eq!(from_os_release(cachy).family, Family::Arch);
        assert_eq!(from_os_release(cachy).name, "CachyOS Linux");
        assert_eq!(
            from_os_release("ID=ubuntu\nID_LIKE=debian\n").family,
            Family::Debian
        );
        assert_eq!(from_os_release("ID=fedora\n").family, Family::Fedora);
        assert_eq!(
            from_os_release("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n").family,
            Family::Suse
        );
        assert_eq!(from_os_release("ID=nixos\n").family, Family::Other);
    }

    #[test]
    fn every_family_has_instructions() {
        let all = all_instructions();
        for f in [
            Family::Arch,
            Family::Debian,
            Family::Fedora,
            Family::Suse,
            Family::Macos,
            Family::Windows,
            Family::Other,
        ] {
            assert!(all.contains_key(f.key()), "missing {}", f.key());
        }
    }
}
