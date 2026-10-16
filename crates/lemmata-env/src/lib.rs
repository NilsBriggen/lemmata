//! Programs Lemmata starts (git, latexmk, texlab, a PDF viewer, …) must run with the
//! user's environment. Inside an AppImage, Lemmata's own environment puts the AppImage's
//! libraries first on `LD_LIBRARY_PATH` and points GTK and GIO into it; a system program
//! that inherits that loads those older libraries and can crash ("symbol lookup error").

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Gives `cmd` the user's environment: without any AppImage's paths and settings.
/// Changes nothing when no AppImage is involved.
pub fn host(cmd: &mut Command) -> &mut Command {
    let appdir = std::env::var_os("APPDIR");
    for (key, value) in changes(appdir.as_deref(), std::env::vars_os()) {
        match value {
            Some(v) => cmd.env(key, v),
            None => cmd.env_remove(key),
        };
    }
    cmd
}

/// The AppImage's own variables, and settings its start script forces on every program.
const APPIMAGE_ONLY: &[&str] = &["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"];

/// Where AppImages are mounted while they run (`/tmp/.mount_lemmatXXXXXX`). Not only the
/// running one: after an update, the new version is started by the old one and inherits
/// its paths, which still point into the old version's mount.
const MOUNT: &str = "/.mount_";

/// What to change in `vars` so nothing points into an AppImage: lists (`a:b:c`) lose those
/// entries, and a variable left empty, or that is the AppImage's alone, goes.
pub fn changes(
    appdir: Option<&OsStr>,
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, Option<OsString>)> {
    let dir = appdir
        .map(|d| d.to_string_lossy().trim_end_matches('/').to_string())
        .filter(|d| !d.is_empty());
    let inside = |p: &str| p.contains(MOUNT) || dir.as_deref().is_some_and(|d| p.starts_with(d));
    let mut out = Vec::new();
    for (key, value) in vars {
        if dir.is_some() && APPIMAGE_ONLY.iter().any(|k| OsStr::new(k) == key) {
            out.push((key, None));
            continue;
        }
        let text = value.to_string_lossy();
        if !text.split(':').any(inside) {
            continue;
        }
        let kept: Vec<&str> = text
            .split(':')
            .filter(|p| !p.is_empty() && !inside(p))
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
            Some(OsStr::new("/tmp/.mount_lemm/")),
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
        let user = vars(&[("LD_LIBRARY_PATH", "/x"), ("GTK_THEME", "Adwaita:dark")]);
        assert!(changes(None, user.clone()).is_empty());
        assert!(changes(Some(OsStr::new("")), user).is_empty());
    }

    #[test]
    fn drops_the_old_version_s_mount_after_an_update() {
        // The updated app was started by the old one: its paths are still in the list.
        let got = changes(
            Some(OsStr::new("/tmp/.mount_lemmatNEW")),
            vars(&[(
                "LD_LIBRARY_PATH",
                "/tmp/.mount_lemmatNEW/usr/lib/:/tmp/.mount_lemmatOLD/usr/lib/:/opt/mine/lib",
            )]),
        );
        assert_eq!(
            got,
            vec![("LD_LIBRARY_PATH".into(), Some("/opt/mine/lib".into()))]
        );
    }
}
