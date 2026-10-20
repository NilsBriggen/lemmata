//! Runs texlab for the open project and relays JSON-RPC between it and the
//! editor. Messages cross the IPC boundary as plain JSON strings; the
//! `Content-Length` framing stays on this side.

use crate::{Event, Sink};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

pub struct Server {
    stdin: Arc<Mutex<ChildStdin>>,
    child: Child,
    /// Increases with every start, so the frontend can drop messages from
    /// a server that has since been replaced.
    pub generation: u64,
}

/// The texlab release Lemmata downloads on first use, and the checksums of its archives
/// (bump both with the version).
pub const TEXLAB_VERSION: &str = "v5.26.0";

fn texlab_asset() -> Option<(&'static str, &'static str)> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => (
            "x86_64-linux.tar.gz",
            "8697bd5e479d4584b14b7eed5c320c80ec4e1d91ebefbb6801e6bf38e9971300",
        ),
        ("linux", "aarch64") => (
            "aarch64-linux.tar.gz",
            "a85cdfcd22454b8d8550f4b0f0620c45ab51760f302fac7a12bc18a890f70f8c",
        ),
        ("macos", "x86_64") => (
            "x86_64-macos.tar.gz",
            "6091611f756b28e1a57612b130c196df4b0bb6e22dde5cf5d890578513397daf",
        ),
        ("macos", "aarch64") => (
            "aarch64-macos.tar.gz",
            "af7972ffd230711ba04ada9b69cc32ce9111d9196ba69538062872faefdbee56",
        ),
        ("windows", "x86_64") => (
            "x86_64-windows.zip",
            "cb028d44c3d2b85d36a2ed52d41a0ff43a341b1f04c500c56c4524c4eb72b316",
        ),
        _ => return None,
    })
}

fn exe_name() -> String {
    format!("texlab{}", std::env::consts::EXE_SUFFIX)
}

/// Where the downloaded texlab lives in the app's data folder.
pub fn downloaded_texlab(data: &Path) -> PathBuf {
    data.join("texlab").join(TEXLAB_VERSION).join(exe_name())
}

/// texlab next to our own executable (where a bundle puts the sidecar), the one downloaded
/// earlier, or one on PATH.
pub fn find_texlab(data: &Path) -> Option<PathBuf> {
    let exe = exe_name();
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&exe)))
        .filter(|p| p.is_file())
        .or_else(|| Some(downloaded_texlab(data)).filter(|p| p.is_file()))
        .or_else(|| which::which("texlab").ok())
}

/// Downloads the pinned texlab into the data folder, checked against its checksum. Blocking.
pub fn install_texlab(data: &Path) -> Result<PathBuf, String> {
    use lemmata_tex::tectonic::{run_tool, sha256_hex};
    use std::ffi::OsString;
    let (asset, sum) = texlab_asset().ok_or("There is no texlab download for this computer.")?;
    let dest = downloaded_texlab(data);
    let dir = dest.parent().expect("a folder");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let work = dir.join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        let archive = work.join("archive");
        let url = format!(
            "https://github.com/latex-lsp/texlab/releases/download/{TEXLAB_VERSION}/texlab-{asset}"
        );
        let args: Vec<OsString> = [
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--retry",
            "2",
            "--connect-timeout",
            "20",
            "--max-time",
            "300",
            "--output",
        ]
        .map(OsString::from)
        .into_iter()
        .chain([archive.clone().into(), url.into()])
        .collect();
        run_tool("curl", &args.iter().collect::<Vec<_>>())
            .map_err(|e| format!("Downloading texlab failed: {e}"))?;
        if !sha256_hex(&archive)
            .map_err(|e| e.to_string())?
            .eq_ignore_ascii_case(sum)
        {
            return Err(
                "The texlab download did not match its checksum, so it was discarded.".to_string(),
            );
        }
        // GNU tar needs -z for gzip; bsdtar (macOS, Windows) reads zip and ignores the flag.
        let flag = if asset.ends_with(".zip") {
            "-xf"
        } else {
            "-xzf"
        };
        let args: Vec<OsString> = vec![
            flag.into(),
            archive.into(),
            "-C".into(),
            work.clone().into(),
        ];
        run_tool("tar", &args.iter().collect::<Vec<_>>())
            .map_err(|e| format!("Unpacking texlab failed: {e}"))?;
        let bin = work.join(exe_name());
        if !bin.is_file() {
            return Err("The texlab download holds no texlab program.".to_string());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&bin, &dest).map_err(|e| e.to_string())?;
        Ok(dest.clone())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

pub async fn start(
    sink: Sink,
    root: &Path,
    data: &Path,
    tex_bin: Option<&Path>,
    generation: u64,
) -> Result<Server, String> {
    let texlab = match find_texlab(data) {
        Some(t) => t,
        // First use: download it, off the async threads.
        None => {
            let data = data.to_owned();
            tokio::task::spawn_blocking(move || install_texlab(&data))
                .await
                .map_err(|e| e.to_string())??
        }
    };
    let mut cmd = Command::new(texlab);
    lemmata_env::host(cmd.as_std_mut());
    cmd.current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(bin) = tex_bin {
        // texlab runs kpsewhich to find packages for completion.
        let mut paths = vec![bin.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start texlab: {e}"))?;
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader).await {
                Ok(Some(body)) => {
                    sink(Event::LspMessage { generation, body });
                }
                _ => {
                    sink(Event::LspExit { generation });
                    break;
                }
            }
        }
    });

    Ok(Server {
        stdin: Arc::new(Mutex::new(stdin)),
        child,
        generation,
    })
}

impl Server {
    pub async fn send(&self, body: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().await;
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        stdin
            .write_all(frame.as_bytes())
            .await
            .map_err(|e| format!("texlab stopped: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("texlab stopped: {e}"))
    }

    /// Ends texlab and waits for it: on Windows its folder can't be moved while it runs there.
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }
}

/// Reads one `Content-Length`-framed message. `Ok(None)` at end of stream.
async fn read_message<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<String>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            length = v.trim().parse().ok();
        }
    }
    let Some(len) = length else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message without Content-Length",
        ));
    };
    // A message is far smaller; a wrong length must not allocate gigabytes.
    if len > 64 << 20 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut buf = vec![0; len];
    reader.read_exact(&mut buf).await?;
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Downloads the real release; run by hand: `cargo test -p lemmata-shell -- --ignored texlab`.
    #[test]
    #[ignore]
    fn downloads_and_verifies_texlab() {
        let dir = tempfile::tempdir().unwrap();
        let bin = install_texlab(dir.path()).unwrap();
        assert!(bin.is_file());
        assert!(find_texlab(dir.path()).is_some());
        let out = std::process::Command::new(&bin)
            .arg("--version")
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("5.26"),
            "{out:?}"
        );
    }

    #[tokio::test]
    async fn reads_framed_messages() {
        let data =
            b"Content-Length: 7\r\nContent-Type: x\r\n\r\n{\"a\":1}Content-Length: 2\r\n\r\n{}";
        let mut r = BufReader::new(&data[..]);
        assert_eq!(
            read_message(&mut r).await.unwrap().as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(read_message(&mut r).await.unwrap().as_deref(), Some("{}"));
        assert_eq!(read_message(&mut r).await.unwrap(), None);
        // A wrong length is an error, not a huge allocation.
        let mut r = BufReader::new(&b"Content-Length: 99999999999999\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).await.is_err());
    }
}
