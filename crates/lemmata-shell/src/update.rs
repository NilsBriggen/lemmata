//! The self-updater (Tauri's updater plugin): the manifest on lemmata.briggen.dev, which
//! redirects to the latest GitHub release (docs/website-handoff.md). Downloads are signed with the
//! same minisign key as before, so scripts/finalize-release.py and the release signing stay as
//! they are. The manifest only ever names stable releases.

use base64::Engine;
use std::path::PathBuf;
use std::time::Duration;

const MANIFEST: &str = "https://lemmata.briggen.dev/update/latest.json";
/// The `pubkey` of the Tauri updater config: a minisign public key file, base64-encoded.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEJBNTI5M0JBMzY4N0VDOUQKUldTZDdJYzJ1cE5TdWh4c242bDZqdUxFY1VqclFadHZkTkpXMmJEQUYwdEUwSTRQb1N5eVpIdTIK";

#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
    pub signature: String,
}

/// What runs once Lemmata has quit: the new version, or on Windows its installer.
pub type AfterExit = (PathBuf, Vec<String>);

/// The manifest's name for this computer.
fn platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "aarch64") => "darwin-aarch64",
        ("macos", "x86_64") => "darwin-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => return None,
    })
}

/// The update `manifest` offers `platform`, if it is newer than `current`.
fn pick(manifest: &[u8], platform: &str, current: &str) -> Result<Option<Update>, String> {
    let bad = || "The update information can't be read.".to_string();
    let m: serde_json::Value = serde_json::from_slice(manifest).map_err(|_| bad())?;
    let version = m["version"]
        .as_str()
        .ok_or_else(bad)?
        .trim_start_matches('v');
    let parse = |v: &str| semver::Version::parse(v).map_err(|_| bad());
    if parse(version)? <= parse(current)? {
        return Ok(None);
    }
    let p = &m["platforms"][platform];
    let (Some(url), Some(signature)) = (p["url"].as_str(), p["signature"].as_str()) else {
        return Err(format!(
            "Lemmata {version} has no download for this computer yet."
        ));
    };
    Ok(Some(Update {
        version: version.to_owned(),
        url: url.to_owned(),
        signature: signature.to_owned(),
    }))
}

/// When this version's release commit was made (seconds since 1970), set by the release build.
/// An update must have been signed after it: the manifest isn't signed, so without this check
/// whoever controls it could offer an older, validly signed build as the newer version.
fn released() -> u64 {
    option_env!("LEMMATA_RELEASE_EPOCH")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// An update is never more than this; a server that sends more is not the release.
const MAX_DOWNLOAD: u64 = 1 << 30;

/// `signature` and `key` as the manifest and the config hold them: base64 of minisign's files.
/// `not_before`: the signature's signed timestamp must be later than this.
fn verify(data: &[u8], signature: &str, key: &str, not_before: u64) -> Result<(), String> {
    let bad = || "The download's signature doesn't match, so it was not installed.".to_string();
    let text = |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(bad)
    };
    let key = minisign_verify::PublicKey::decode(&text(key)?).map_err(|_| bad())?;
    let sig = minisign_verify::Signature::decode(&text(signature)?).map_err(|_| bad())?;
    // Legacy (not prehashed) signatures too, as Tauri's updater accepted them.
    key.verify(data, &sig, true).map_err(|_| bad())?;
    // "timestamp:1794158560\tfile:…", covered by the signature's global part.
    let signed = sig
        .trusted_comment()
        .split('\t')
        .find_map(|f| f.strip_prefix("timestamp:"))
        .and_then(|t| t.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if signed <= not_before {
        return Err(
            "The download is an older version than this one, so it was not installed.".into(),
        );
    }
    Ok(())
}

fn offline(e: reqwest::Error) -> String {
    format!("Couldn't reach lemmata.briggen.dev ({e}). Check your internet connection.")
}

/// The newer release, if there is one. `current` is this build's version.
pub async fn check(current: &str) -> Result<Option<Update>, String> {
    let platform = platform().ok_or("There are no updates for this computer.")?;
    // Tauri's updater replaced only the AppImage; .deb and .rpm installs update with the system.
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        return Err("This Lemmata was installed by a package manager: update it there.".into());
    }
    let res = crate::http_client(Duration::from_secs(30))?
        .get(MANIFEST)
        .send()
        .await
        .map_err(offline)?;
    if !res.status().is_success() {
        return Err(format!("The update server answered {}.", res.status()));
    }
    pick(&res.bytes().await.map_err(offline)?, platform, current)
}

/// Downloads the update, checks its signature and puts it in place. `progress` gets the
/// fraction done.
pub async fn install(u: &Update, progress: impl Fn(f32)) -> Result<AfterExit, String> {
    let mut res = crate::http_client(Duration::from_secs(30 * 60))?
        .get(&u.url)
        .send()
        .await
        .map_err(offline)?;
    if !res.status().is_success() {
        return Err(format!("The download answered {}.", res.status()));
    }
    let too_big = || "The download is far larger than an update, so it was stopped.".to_string();
    if res.content_length().is_some_and(|n| n > MAX_DOWNLOAD) {
        return Err(too_big());
    }
    let total = res.content_length().unwrap_or(0) as f32;
    let mut data = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(offline)? {
        data.extend_from_slice(&chunk);
        if data.len() as u64 > MAX_DOWNLOAD {
            return Err(too_big());
        }
        if total > 0.0 {
            progress(data.len() as f32 / total);
        }
    }
    verify(&data, &u.signature, PUBLIC_KEY, released())?;
    let version = u.version.clone();
    tokio::task::spawn_blocking(move || put_in_place(&data, &version))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(target_os = "linux")]
fn put_in_place(data: &[u8], _version: &str) -> Result<AfterExit, String> {
    use std::os::unix::fs::PermissionsExt;
    let target = PathBuf::from(std::env::var_os("APPIMAGE").ok_or("Not an AppImage.")?);
    let dir = target.parent().ok_or("The AppImage has no folder.")?;
    // Beside it, so the rename that swaps them can't be half done.
    let tmp = dir.join(format!(".lemmata-update-{}", std::process::id()));
    let write = || -> std::io::Result<()> {
        std::fs::write(&tmp, data)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &target)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Couldn't replace {}: {e}", target.display())
    })?;
    Ok((target, Vec::new()))
}

#[cfg(target_os = "macos")]
fn put_in_place(data: &[u8], _version: &str) -> Result<AfterExit, String> {
    // …/Lemmata.app/Contents/MacOS/lemmata-app
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app = exe
        .ancestors()
        .nth(3)
        .filter(|a| a.extension().is_some_and(|e| e == "app"))
        .ok_or("Lemmata isn't running from an app bundle.")?
        .to_path_buf();
    let dir = app.parent().ok_or("The app has no folder.")?;
    let work = dir.join(format!(".lemmata-update-{}", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let archive = work.join("update.tar.gz");
        std::fs::write(&archive, data).map_err(|e| e.to_string())?;
        let args = [
            std::ffi::OsString::from("-xzf"),
            archive.into(),
            "-C".into(),
            work.clone().into(),
        ];
        lemmata_tex::tectonic::run_tool("tar", &args.iter().collect::<Vec<_>>())?;
        let new = std::fs::read_dir(&work)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or("The download holds no app.")?;
        swap(&app, &new, &work.join("old.app"), |a, b| {
            std::fs::rename(a, b)
        })
    })();
    // The old app may still be in the work folder (the swap couldn't put it back): keep it then.
    if !work.join("old.app").exists() {
        let _ = std::fs::remove_dir_all(&work);
    }
    result.map_err(|e| format!("Couldn't replace {}: {e}", app.display()))?;
    Ok((
        "open".into(),
        vec!["-n".into(), app.to_string_lossy().into_owned()],
    ))
}

/// Puts `new` where `app` is, the old one moved to `old` first and back again if that fails. When
/// even that fails, the old app stays at `old` and the message says where.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn swap(
    app: &std::path::Path,
    new: &std::path::Path,
    old: &std::path::Path,
    rename: impl Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
) -> Result<(), String> {
    rename(app, old).map_err(|e| e.to_string())?;
    if let Err(e) = rename(new, app) {
        return match rename(old, app) {
            Ok(()) => Err(e.to_string()),
            Err(back) => Err(format!(
                "{e}; the previous Lemmata couldn't be put back ({back}) and is at {}",
                old.display()
            )),
        };
    }
    let _ = std::fs::remove_dir_all(old);
    Ok(())
}

#[cfg(windows)]
fn put_in_place(data: &[u8], version: &str) -> Result<AfterExit, String> {
    let setup = std::env::temp_dir().join(format!("Lemmata_{version}_setup.exe"));
    std::fs::write(&setup, data).map_err(|e| e.to_string())?;
    // shortcut: /P (passive) and /R (start Lemmata after) as Tauri's NSIS template took them;
    // unchecked against cargo-packager's template on Linux, the Windows CI job only builds.
    Ok((setup, vec!["/P".into(), "/R".into()]))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn put_in_place(_: &[u8], _: &str) -> Result<AfterExit, String> {
    Err("There are no updates for this computer.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A throwaway key and its signature of DATA, made like Tauri's signer (prehashed).
    const KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDExODA0RkI3OTc4NDA3RTYKUldUbUI0U1h0MCtBRWEvQkhFOTYwVEIxbmU1MmN4bVhaMWFsa1FndExGbVJmYVFGNUY0a2VueHoK";
    const SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUbUI0U1h0MCtBRVkwMmVBMmE4elFEZEQ4WXRDR3lId0ZBRFVLMzRWYXViRmI3Wi8yUCszWVkvTW1vNktnOFFWMkNoSmsxOG9mMEZ4R2M3ZW1wV3E5VUlZK1prVzhWUndjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzAwMDAwMDAwCWZpbGU6dGVzdC5BcHBJbWFnZQpNUHNuWVZlMVNGazludFd0MEZPRDlZdUh4eGM5QWhwVVdKQzRsSlAwY01PUzBmejExZnVGelQzZXd0QjVaWGVmeUdRSkh5TWg2THdGN3VhRHhGOWlEdz09Cg==";
    const DATA: &[u8] = b"Lemmata update test\n";

    #[test]
    fn only_a_signed_download_passes() {
        verify(DATA, SIG, KEY, 0).unwrap();
        assert!(
            verify(b"Lemmata update tesT\n", SIG, KEY, 0).is_err(),
            "changed bytes"
        );
        assert!(verify(DATA, SIG, PUBLIC_KEY, 0).is_err(), "another key");
        assert!(verify(DATA, "garbage", KEY, 0).is_err());
        // Signed at 1700000000: older than a version released after that.
        verify(DATA, SIG, KEY, 1_699_999_999).unwrap();
        let e = verify(DATA, SIG, KEY, 1_700_000_000).unwrap_err();
        assert!(e.contains("older version"), "{e}");
    }

    #[test]
    fn a_failed_swap_puts_the_old_app_back_or_says_where_it_is() {
        use std::cell::Cell;
        use std::path::Path;
        let d = tempfile::tempdir().unwrap();
        let (app, new, old) = (
            d.path().join("Lemmata.app"),
            d.path().join("new.app"),
            d.path().join("old.app"),
        );
        let setup = || {
            let _ = std::fs::remove_dir_all(&app);
            let _ = std::fs::remove_dir_all(&old);
            std::fs::create_dir_all(app.join("v1")).unwrap();
            std::fs::create_dir_all(new.join("v2")).unwrap();
        };
        // The second rename (new into place) fails: the old app goes back.
        setup();
        let n = Cell::new(0);
        let fail_second = |a: &Path, b: &Path| {
            n.set(n.get() + 1);
            if n.get() == 2 {
                Err(std::io::Error::other("busy"))
            } else {
                std::fs::rename(a, b)
            }
        };
        assert!(swap(&app, &new, &old, fail_second).is_err());
        assert!(app.join("v1").is_dir(), "the old app is back");
        // Putting it back fails too: it stays at `old`, and the message says so.
        setup();
        let n = Cell::new(0);
        let fail_after_first = |a: &Path, b: &Path| {
            n.set(n.get() + 1);
            if n.get() >= 2 {
                Err(std::io::Error::other("busy"))
            } else {
                std::fs::rename(a, b)
            }
        };
        let e = swap(&app, &new, &old, fail_after_first).unwrap_err();
        assert!(old.join("v1").is_dir(), "the old app is kept");
        assert!(e.contains(&old.display().to_string()), "{e}");
        // Both work: the new app is in place and the old one gone.
        setup();
        swap(&app, &new, &old, |a, b| std::fs::rename(a, b)).unwrap();
        assert!(app.join("v2").is_dir() && !old.exists());
    }

    #[test]
    fn picks_a_newer_version_for_this_platform() {
        let m = br#"{"version":"v2.3.0","notes":"","platforms":{"linux-x86_64":{"url":"https://x/a.AppImage","signature":"s"}}}"#;
        let u = pick(m, "linux-x86_64", "2.2.1").unwrap().unwrap();
        assert_eq!(
            (u.version.as_str(), u.url.as_str()),
            ("2.3.0", "https://x/a.AppImage")
        );
        assert_eq!(
            pick(m, "linux-x86_64", "2.3.0").unwrap(),
            None,
            "same version"
        );
        assert_eq!(pick(m, "linux-x86_64", "2.4.0").unwrap(), None, "older");
        assert!(
            pick(m, "linux-x86_64", "2.3.0-rc.1").unwrap().is_some(),
            "an rc moves on to its release"
        );
        assert!(
            pick(m, "windows-x86_64", "2.2.1")
                .unwrap_err()
                .contains("no download")
        );
        assert!(pick(b"<html>", "linux-x86_64", "2.2.1").is_err());
    }
}
