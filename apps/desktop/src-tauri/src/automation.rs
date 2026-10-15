//! Test automation bridge, compiled into debug builds only and started only
//! when `LEMMATA_AUTOMATION` is set. It listens on a local socket (a named
//! pipe on Windows) for one JSON request per line, `{"id":1,"method":"state",
//! "params":{}}`, runs it with `window.__lemmata.call` in the main window and
//! answers `{"id":1,"result":…}` or `{"id":1,"error":"…"}`. No network port.
//! Clients: scripts/lemmata-drive.mjs and the developer MCP server.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

#[derive(Default)]
pub struct Pending {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
}

/// The webview's answer to a forwarded request.
#[tauri::command]
pub fn automation_reply(state: State<Pending>, id: u64, result: Value) {
    if let Some(tx) = state.waiting.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(result);
    }
}

/// Where to listen: `LEMMATA_AUTOMATION` holds a path, or "1" for the default.
pub fn address() -> Option<String> {
    let v = std::env::var("LEMMATA_AUTOMATION").ok()?;
    if v != "1" {
        return Some(v);
    }
    if cfg!(windows) {
        return Some(r"\\.\pipe\lemmata-automation".into());
    }
    // The runtime dir is private to the user (0700); fall back to the temp dir.
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    Some(
        dir.join("lemmata-automation.sock")
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn start(app: &AppHandle) {
    let Some(addr) = address() else { return };
    app.manage(Pending::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app, &addr).await {
            eprintln!("automation: {addr}: {e}");
        }
    });
}

#[cfg(unix)]
async fn serve(app: AppHandle, addr: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::remove_file(addr);
    let listener = tokio::net::UnixListener::bind(addr)?;
    std::fs::set_permissions(addr, std::fs::Permissions::from_mode(0o600))?;
    eprintln!("automation: listening on {addr}");
    loop {
        let (stream, _) = listener.accept().await?;
        tauri::async_runtime::spawn(connection(app.clone(), stream));
    }
}

#[cfg(windows)]
async fn serve(app: AppHandle, addr: &str) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(addr)?;
    eprintln!("automation: listening on {addr}");
    loop {
        server.connect().await?;
        let client = std::mem::replace(&mut server, ServerOptions::new().create(addr)?);
        tauri::async_runtime::spawn(connection(app.clone(), client));
    }
}

async fn connection<S: AsyncRead + AsyncWrite + Unpin>(app: AppHandle, stream: S) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let answer = match req.get("method").and_then(Value::as_str) {
            Some(method) => match forward(
                &app,
                method,
                req.get("params").cloned().unwrap_or(json!({})),
            )
            .await
            {
                Ok(result) => json!({ "id": id, "result": result }),
                Err(error) => json!({ "id": id, "error": error }),
            },
            None => json!({ "id": id, "error": "expected {\"id\", \"method\", \"params\"}" }),
        };
        if write
            .write_all(format!("{answer}\n").as_bytes())
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn forward(app: &AppHandle, method: &str, params: Value) -> Result<Value, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("The main window is not open.")?;
    let pending = app.state::<Pending>();
    let id = pending.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    pending
        .waiting
        .lock()
        .map_err(|_| "automation state is broken")?
        .insert(id, tx);
    let method = serde_json::to_string(method).map_err(|e| e.to_string())?;
    let script = format!(
        "(async () => {{
            const reply = (result) => window.__TAURI_INTERNALS__.invoke('automation_reply', {{ id: {id}, result }});
            try {{
                if (!window.__lemmata) throw new Error('Automation is not loaded (is this a test build?).');
                reply({{ ok: await window.__lemmata.call({method}, {params}) }});
            }} catch (e) {{
                reply({{ error: String((e && e.message) || e) }});
            }}
        }})()"
    );
    window.eval(&script).map_err(|e| e.to_string())?;
    let answer = tokio::time::timeout(Duration::from_secs(180), rx)
        .await
        .map_err(|_| "The app did not answer within 3 minutes.".to_string())?
        .map_err(|_| "The request was dropped.".to_string())?;
    match answer.get("error") {
        Some(e) => Err(e.as_str().unwrap_or("error").to_string()),
        None => Ok(answer.get("ok").cloned().unwrap_or(Value::Null)),
    }
}
