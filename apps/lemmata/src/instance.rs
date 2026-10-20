#![cfg_attr(debug_assertions, allow(dead_code))]
//! One Lemmata at a time, and `lemmata://` links (Tauri's single-instance and deep-link plugins).
//!
//! The first instance listens on a localhost port and writes the port with a random token into a
//! file only the user can read. A second launch finds the file, connects, sends its arguments with
//! the token and quits; the first opens what they name and comes to the front.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILE: &str = "instance";

#[derive(serde::Serialize, serde::Deserialize)]
struct Launch {
    token: String,
    /// With the program name first, as `std::env::args` gives them.
    args: Vec<String>,
    cwd: String,
}

fn file(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Hands this launch to a running Lemmata. True when one took it: this process should quit.
pub fn forward(dir: &Path, args: &[String], cwd: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file(dir)) else {
        return false;
    };
    let Some((port, token)) = text.trim().split_once(' ') else {
        return false;
    };
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let launch = Launch {
        token: token.to_owned(),
        args: args.to_vec(),
        cwd: cwd.to_string_lossy().into_owned(),
    };
    let Ok(line) = serde_json::to_string(&launch) else {
        return false;
    };
    if writeln!(s, "{line}").is_err() {
        return false;
    }
    // The running app answers "ok" once it has the request.
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut answer = String::new();
    BufReader::new(s).read_line(&mut answer).is_ok() && answer.trim() == "ok"
}

/// Becomes the instance others hand their launches to: `on` gets each one's arguments and folder.
pub fn listen(
    dir: &Path,
    on: impl Fn(Vec<String>, String) + Send + Sync + 'static,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = token().ok_or_else(|| std::io::Error::other("no secure random source"))?;
    std::fs::create_dir_all(dir)?;
    write_private(&file(dir), &format!("{port} {token}"))?;
    let shared = std::sync::Arc::new((token, on));
    std::thread::Builder::new()
        .name("instance".into())
        .spawn(move || {
            for s in listener.incoming().flatten() {
                // Each on its own thread, so a client that stalls holds up nobody else.
                let shared = shared.clone();
                std::thread::spawn(move || {
                    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut line = String::new();
                    let Ok(reader) = s.try_clone().map(BufReader::new) else {
                        return;
                    };
                    // A launch is a few hundred bytes: more is not one, and must not fill memory.
                    if reader.take(64 * 1024).read_line(&mut line).is_err() {
                        return;
                    }
                    // Only someone who could read the file (this user) may ask.
                    let Ok(launch) = serde_json::from_str::<Launch>(&line) else {
                        return;
                    };
                    if launch.token != shared.0 {
                        return;
                    }
                    let mut s = s;
                    let _ = writeln!(s, "ok");
                    (shared.1)(launch.args, launch.cwd);
                });
            }
        })?;
    Ok(())
}

/// 32 bytes from the OS's secure random source, as hex. None if it can't give any: then no
/// instance listens, rather than one with a guessable token.
fn token() -> Option<String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(text.as_bytes())
}

/// Makes this program the handler of `lemmata://` links, as the deep-link plugin did at start:
/// a `.desktop` entry and `xdg-mime` on Linux, the user's registry on Windows. macOS takes the
/// scheme from the app bundle.
pub fn register_scheme() {
    let Ok(bin) = std::env::current_exe() else {
        return;
    };
    #[cfg(target_os = "linux")]
    {
        // An AppImage registers itself, not the binary inside its mount.
        let exec = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .unwrap_or(bin.clone());
        let name = format!(
            "{}-handler.desktop",
            bin.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "lemmata".into())
        );
        let Some(data) = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))
        else {
            return;
        };
        let dir = data.join("applications");
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Lemmata\nExec=\"{}\" %u\nTerminal=false\nNoDisplay=true\nMimeType=x-scheme-handler/lemmata\n",
            exec.display()
        );
        let path = dir.join(&name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(entry.as_str()) {
            if std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, &entry))
                .is_err()
            {
                return;
            }
            let run = |cmd: &str, args: &[&str]| {
                let mut c = std::process::Command::new(cmd);
                lemmata_env::host(&mut c);
                // Their complaints (a missing qtpaths, say) are no business of the app's output.
                let _ = c
                    .args(args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            };
            run("update-desktop-database", &[&dir.to_string_lossy()]);
            run("xdg-mime", &["default", &name, "x-scheme-handler/lemmata"]);
        }
    }
    #[cfg(windows)]
    {
        let exe = bin.display().to_string();
        let key = r"HKCU\Software\Classes\lemmata";
        for (sub, name, value) in [
            ("", "", "URL:dev.briggen.lemmata protocol".to_owned()),
            ("", "URL Protocol", String::new()),
            (r"\DefaultIcon", "", format!("{exe},0")),
            (r"\shell\open\command", "", format!("\"{exe}\" \"%1\"")),
        ] {
            let mut c = std::process::Command::new("reg");
            c.arg("add").arg(format!("{key}{sub}")).arg("/f");
            if name.is_empty() {
                c.arg("/ve");
            } else {
                c.args(["/v", name]);
            }
            c.args(["/d", &value]);
            lemmata_tex::tectonic::no_window_std(&mut c);
            let _ = c.status();
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = bin;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_second_launch_hands_over_its_arguments() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            !forward(dir.path(), &["lemmata".into()], Path::new("/")),
            "nobody to hand to yet"
        );
        let (tx, rx) = mpsc::channel();
        listen(dir.path(), move |args, cwd| tx.send((args, cwd)).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(file(dir.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "only the user reads the token");
        }
        let args = vec!["lemmata".to_owned(), "thesis/main.tex".to_owned()];
        assert!(forward(dir.path(), &args, Path::new("/home/x")));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (args, "/home/x".to_owned())
        );
        // A wrong token is ignored.
        let text = std::fs::read_to_string(file(dir.path())).unwrap();
        let port = text.split(' ').next().unwrap();
        std::fs::write(file(dir.path()), format!("{port} wrong")).unwrap();
        assert!(!forward(dir.path(), &["lemmata".into()], Path::new("/")));
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
