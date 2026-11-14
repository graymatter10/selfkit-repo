//! Local IPC: line-delimited JSON over a Unix socket (Linux) or named pipe
//! (Windows). No network ports. Request: `{"cmd":"status"}`; reply: one JSON line.

use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::state::Shared;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Pause,
    Resume,
    SyncNow,
    Today,
    /// Diagnostics for the Reflect health screen (same document as health.json).
    Health,
    /// Run due backups now; `full: true` also forces a full export.
    BackupNow {
        #[serde(default)]
        full: bool,
    },
}

#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    dir.join("selfkit-agent.sock")
}

#[cfg(windows)]
pub const PIPE_NAME: &str = r"\\.\pipe\selfkit-agent";

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(stream: S, shared: Arc<Shared>) -> Result<()> {
    let (r, mut w) = tokio::io::split(stream);
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(req) => shared.handle(req).await,
            Err(e) => serde_json::json!({"ok": false, "error": format!("bad request: {e}")}),
        };
        let mut out = serde_json::to_vec(&reply)?;
        out.push(b'\n');
        w.write_all(&out).await?;
    }
    Ok(())
}

#[cfg(unix)]
pub async fn serve(shared: Arc<Shared>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let path = socket_path();
    if path.exists() {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            anyhow::bail!("another selfkit-agent is already running ({})", path.display());
        }
        std::fs::remove_file(&path)?;
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %path.display(), "ipc listening");
    loop {
        let (stream, _) = listener.accept().await?;
        let sh = shared.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, sh).await {
                tracing::debug!("ipc client: {e}");
            }
        });
    }
}

#[cfg(windows)]
pub async fn serve(shared: Arc<Shared>) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let mut server = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(PIPE_NAME)
        .map_err(|e| anyhow::anyhow!("another selfkit-agent may be running: {e}"))?;
    loop {
        server.connect().await?;
        let client = server;
        server = ServerOptions::new().reject_remote_clients(true).create(PIPE_NAME)?;
        let sh = shared.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(client, sh).await {
                tracing::debug!("ipc client: {e}");
            }
        });
    }
}

/// Client side, used by the CLI subcommands (and later by the apps).
pub async fn request(req: &Request) -> Result<serde_json::Value> {
    let mut line = serde_json::to_vec(req)?;
    line.push(b'\n');
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path()).await
        .map_err(|e| anyhow::anyhow!("agent not running? ({e})"))?;
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(PIPE_NAME)
        .map_err(|e| anyhow::anyhow!("agent not running? ({e})"))?;
    let (r, mut w) = tokio::io::split(stream);
    w.write_all(&line).await?;
    let mut resp = String::new();
    BufReader::new(r).read_line(&mut resp).await?;
    Ok(serde_json::from_str(&resp)?)
}
