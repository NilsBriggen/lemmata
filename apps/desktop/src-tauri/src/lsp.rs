//! Runs texlab for the open project and relays JSON-RPC between it and the
//! editor. Messages cross the IPC boundary as plain JSON strings; the
//! `Content-Length` framing stays on this side.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
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

#[derive(Clone, serde::Serialize)]
struct Message {
    generation: u64,
    body: String,
}

/// texlab next to our own executable (where the bundle puts the sidecar),
/// otherwise on PATH.
pub fn find_texlab() -> Option<PathBuf> {
    let exe = format!("texlab{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&exe)))
        .filter(|p| p.is_file())
        .or_else(|| which::which("texlab").ok())
}

pub async fn start(
    app: AppHandle,
    root: &Path,
    tex_bin: Option<&Path>,
    generation: u64,
) -> Result<Server, String> {
    let texlab = find_texlab().ok_or("texlab was not found, so completion and hover are off.")?;
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

    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader).await {
                Ok(Some(body)) => {
                    let _ = app.emit("lsp-message", Message { generation, body });
                }
                _ => {
                    let _ = app.emit("lsp-exit", generation);
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

    pub fn stop(mut self) {
        let _ = self.child.start_kill();
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
    let mut buf = vec![0; len];
    reader.read_exact(&mut buf).await?;
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
