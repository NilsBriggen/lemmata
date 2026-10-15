//! Programs Lemmata starts (git, latexmk, texlab, a PDF viewer, …) must run with the
//! user's environment. Inside an AppImage, Lemmata's own environment puts the AppImage's
//! libraries first on `LD_LIBRARY_PATH` and points GTK and GIO into it; a system program
//! that inherits that loads those older libraries and can crash ("symbol lookup error").

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Gives `cmd` the user's environment: without the AppImage's paths and settings.
/// Does nothing outside an AppImage.
pub fn host(cmd: &mut Command) -> &mut Command {
    if let Some(dir) = std::env::var_os("APPDIR") {
        for (key, value) in changes(&dir, std::env::vars_os()) {
            match value {
                Some(v) => cmd.env(key, v),
                None => cmd.env_remove(key),
            };
        }
    }
    cmd
}

/// The AppImage's own variables, and settings its start script forces on every program.
const APPIMAGE_ONLY: &[&str] = &["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"];

/// What to change in `vars` so nothing points into `appdir`: lists (`a:b:c`) lose the
/// entries inside it, and a variable left empty, or that is the AppImage's alone, goes.
pub fn changes(
    appdir: &OsStr,
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, Option<OsString>)> {
    let dir = appdir.to_string_lossy().trim_end_matches('/').to_string();
    if dir.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (key, value) in vars {
        if APPIMAGE_ONLY.iter().any(|k| OsStr::new(k) == key) {
            out.push((key, None));
            continue;
        }
        let text = value.to_string_lossy();
        if !text.contains(&dir) {
            continue;
        }
        let kept: Vec<&str> = text
            .split(':')
            .filter(|p| !p.is_empty() && !p.starts_with(&dir))
            .collect();
        out.push((key, (!kept.is_empty()).then(|| kept.join(":").into())));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(list: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        list.iter().map(|(k, v)| (k.into(), v.into())).collect()
    }

    #[test]
    fn strips_the_appimage_and_keeps_the_rest() {
        let got = changes(
            OsStr::new("/tmp/.mount_lemm/"),
            vars(&[
                (
                    "LD_LIBRARY_PATH",
                    "/tmp/.mount_lemm/usr/lib/:/tmp/.mount_lemm/usr/lib/x86_64-linux-gnu/:/opt/mine/lib",
                ),
                ("GIO_MODULE_DIR", "/tmp/.mount_lemm//usr/lib/gio/modules"),
                ("XDG_DATA_DIRS", "/tmp/.mount_lemm/usr/share:/usr/share"),
                ("GTK_THEME", "Adwaita:light"),
                ("APPIMAGE", "/home/me/lemmata.appimage"),
                ("HOME", "/home/me"),
                ("PATH", "/usr/bin:/bin"),
            ]),
        );
        let get = |k: &str| got.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        assert_eq!(get("LD_LIBRARY_PATH"), Some(Some("/opt/mine/lib".into())));
        assert_eq!(get("GIO_MODULE_DIR"), Some(None));
        assert_eq!(get("XDG_DATA_DIRS"), Some(Some("/usr/share".into())));
        assert_eq!(get("GTK_THEME"), Some(None));
        assert_eq!(get("APPIMAGE"), Some(None));
        // Untouched: nothing of the AppImage in them.
        assert_eq!(get("HOME"), None);
        assert_eq!(get("PATH"), None);
    }

    #[test]
    fn outside_an_appimage_nothing_changes() {
        assert!(changes(OsStr::new(""), vars(&[("LD_LIBRARY_PATH", "/x")])).is_empty());
    }
}
